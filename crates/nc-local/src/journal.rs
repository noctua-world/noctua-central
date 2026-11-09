//! The journal's store on the person's machine: `nc_journal::Store` over the
//! local database's `local_journal_entries`
//! (`local-migrations/0004_local_journal.sql`).
//!
//! [`LocalStore::journal`](crate::LocalStore::journal) hands it out over the
//! store's own pool, so the journal and the plugin store are one file, opened
//! and judged once, and every statement either writes is in this crate, where
//! the source gate over the crates that open the file reads it (invariant 18).
//!
//! # It answers as the other two stores do
//!
//! `nc_journal::InMemory` is the trait's specification and
//! `nc_journal::postgres::Postgres` the store a server runs; this answers every
//! call as they do, refusal for refusal and in the same order, and
//! `crates/nc-local/tests/the_local_journal.rs` holds one transcript of all
//! three equal.
//!
//! - **An append is one statement, in one transaction.** The link, the
//!   sequence and the unit's brackets are the `WHERE` of the `INSERT`, as they
//!   are PostgreSQL's, inside a `BEGIN IMMEDIATE` that holds the file's one
//!   writer: the diagnosis of a refused entry reads the history the refusal
//!   was decided against, so no other writer can change the answer between the
//!   two, and `Error::Contended` — the race PostgreSQL's diagnosis can lose —
//!   is never this store's answer.
//! - **Every write is counted**, through the crate's `write_exactly`, and the
//!   transaction first asks that no temporary object sits on its connection
//!   (invariant 22): an `INSERT` a trigger silenced is refused, never believed.
//! - **A row this code did not write is named, never read as one it did.**
//!   `nc_journal::Entry` is sealed, so a row is read back through
//!   `Entry::chain` over its own stored link, and one whose stored hash is not
//!   the hash that gives — a row written around this store, or rewritten after
//!   — is one [`from`](nc_journal::Store::from) stops at and
//!   [`tail`](nc_journal::Store::tail) refuses as `Error::Undecodable`
//!   (`NC-JRNL-0001`), at its own sequence; `journal.verify` reports it there
//!   as a row it cannot read, `NC-JRNL-0002`, the code it gives a row whose
//!   hash is wrong. Every column that holds what the chain hashes is read back
//!   as the text this code writes, or the row is refused: a reader accepting
//!   more spellings than the writer emits is a row respelled without moving
//!   the digest.
//! - **Which units are open is asked of four columns**: the realm, the unit,
//!   the action and the outcome — over the index `(realm, work_unit)`, never
//!   over the history — so a row nothing can decode hides no unit opened after
//!   it. Every entry of the realm inside a unit is visited through the index,
//!   read from those columns and never decoded, so the answer costs the
//!   realm's units and not every realm's history. An opening bracket whose
//!   unit is not spelled as this code writes one is refused by name
//!   (`Error::Undecodable`), never skipped: read leniently it is a unit no
//!   question finds by its text, and skipped it might be the one a stopped
//!   process left.
//! - **The newest unit is the largest any entry names, in any realm**
//!   ([`LocalJournal::newest_unit`]): what a daemon's start resumes its
//!   identifiers above, beside its mark, so a mark lost or rolled back is
//!   still above every unit kept. One column, every row: the realms share one
//!   minter.
//! - **A row lands only at the next sequence**, by the table's own trigger,
//!   whoever writes it: a row planted at the top of the range would end the
//!   history, and every `CHECK` is one `PRAGMA ignore_check_constraints` away
//!   from a connection that changes no schema.
//!
//! # What it does not answer (`§2.4`)
//!
//! A process running as the same user, which can drop the triggers and then
//! edit or delete any row: SQLite has no roles (the crate's header). The chain
//! shows a row edited in the middle of the history; it does not show a
//! shortened one or a rewritten tail, and no anchor of it is kept off this
//! machine (`nc_journal`'s header; `NOCTUA.md §14.9` leaves the anchor to a
//! later stage). And whoever can write the file can insert, at the next
//! sequence, an opening bracket naming its unit in a spelling this code never
//! writes: `open_units` refuses it, so every kernel of that realm stays shut
//! until the row is repaired by hand — failing closed, and a residual the
//! repository's manual names.

use nc_canon::Value;
use nc_ids::{OpaqueId, Snowflake};
use nc_journal::{Entry, Error, Outcome, Read, Record};
use sqlx::Row as _;
use sqlx::SqliteConnection;
use sqlx::sqlite::{SqlitePool, SqliteRow};

use crate::LocalError;

/// The journal's store over the local database.
#[derive(Debug, Clone)]
pub struct LocalJournal {
    pool: SqlitePool,
}

impl LocalJournal {
    /// The journal over `pool`, which [`crate::LocalStore`] opened and judged.
    pub(crate) const fn over(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// What an entry that does not follow the tail it names is told.
///
/// **The in-memory store's sentence, word for word**, and PostgreSQL's: the
/// three stores answer one call with one value, and this is in it.
const NO_LINK: &str = "this entry does not link to the tail it would follow, so the history \
                       it joins could never verify. `Entry::chain` is what links one";

impl nc_journal::Store for LocalJournal {
    async fn tail(&self) -> Result<Option<Entry>, Error> {
        let mut connection = self.reachable().await?;
        tail_on(&mut connection).await
    }

    async fn head(&self) -> Result<Option<(u64, [u8; 32])>, Error> {
        // **Two columns, and neither is one a foreign row poisons**, which is
        // the argument `nc_journal::Store::head` makes: appending never depends
        // on reading the tail's record.
        let row = sqlx::query(
            "SELECT sequence, hash FROM main.local_journal_entries ORDER BY sequence DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| refused("reading the journal's head", &error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        // **A column that will not decode is a row this code did not write**,
        // `Undecodable` and never `Unavailable`: tampering evidence must not
        // arrive as a retry.
        let sequence = sequence_of(&row).ok_or_else(|| Error::Undecodable {
            sequence: u64::MAX,
            why: "the `sequence` column of the last row does not read as a sequence this code \
                  writes, so the row cannot say which entry it is"
                .to_owned(),
        })?;
        let hash = row
            .try_get::<String, _>("hash")
            .map_err(|error| format!("the `hash` column does not read as text: {error}"))
            .and_then(|hex| digest("hash", &hex))
            .map_err(|why| Error::Undecodable { sequence, why })?;
        Ok(Some((sequence, hash)))
    }

    async fn append(&mut self, entry: Entry) -> Result<(), Error> {
        // **First, before the store is asked**, as every store does: an entry
        // it could not read back is refused here, not one layer up.
        nc_journal::readable(&entry)?;
        let record = &entry.record;
        let sequence = signed("sequence", entry.sequence)?;
        let epoch = record
            .policy_epoch
            .map(|epoch| signed("policy_epoch", epoch))
            .transpose()?;
        let recorded_at = signed("recorded_at", record.recorded_at)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| refused("beginning an append to the journal", &error))?;
        crate::schema::no_temporary_object(&mut transaction)
            .await
            .map_err(from_local)?;
        // **The link stated inside the insert.** `?1 - 1` and not `MAX + 1`,
        // which is PostgreSQL's reason and holds here as well; `IS` is SQLite's
        // `IS NOT DISTINCT FROM`, so the first entry's `NULL` link matches an
        // empty table; and the two bracket rules ride in the same `WHERE`: no
        // entry for a unit this realm has closed, and no second opening of one
        // it has opened. Each asks realm, unit, action and outcome — the action
        // is what makes a bracket one.
        let written = sqlx::query(
            "INSERT INTO main.local_journal_entries (sequence, realm, principal, actor_chain, \
             action, resource, result, deciding_policy, policy_pack, policy_epoch, grant_used, \
             work_unit, recorded_at, prev_hash, hash) \
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15 \
             WHERE ?1 - 1 = coalesce((SELECT max(sequence) FROM main.local_journal_entries), -1) \
             AND ?14 IS (SELECT hash FROM main.local_journal_entries \
             ORDER BY sequence DESC LIMIT 1) \
             AND NOT EXISTS (SELECT 1 FROM main.local_journal_entries \
             WHERE realm = ?2 AND work_unit = ?12 AND action = ?16 AND result IN (?17, ?18)) \
             AND NOT (?5 = ?19 AND ?7 = ?20 AND EXISTS (SELECT 1 \
             FROM main.local_journal_entries \
             WHERE realm = ?2 AND work_unit = ?12 AND action = ?19 AND result = ?20))",
        )
        .bind(sequence)
        .bind(record.realm.as_str())
        .bind(record.principal.as_str())
        .bind(actors_written(&record.actor_chain))
        .bind(&record.action)
        .bind(record.resource.to_canonical())
        .bind(record.result.as_str())
        .bind(record.deciding_policy.as_deref())
        .bind(record.policy_pack.as_deref())
        .bind(epoch)
        .bind(record.grant_used.as_ref().map(OpaqueId::as_str))
        .bind(record.work_unit.map(Snowflake::to_crockford))
        .bind(recorded_at)
        .bind(entry.previous.map(|hash| nc_canon::to_hex(&hash)))
        .bind(nc_canon::to_hex(&entry.hash))
        .bind(nc_journal::unit::CLOSES)
        .bind(Outcome::UnitCommitted.as_str())
        .bind(Outcome::UnitAborted.as_str())
        .bind(nc_journal::unit::OPENS)
        .bind(Outcome::UnitBegun.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(|error| refused("appending an entry to the journal", &error))?;
        // **Zero rows is the refusal, and the diagnosis names it**, in the
        // transaction that made it. When every answer it reads says nothing is
        // wrong, nothing refused the row and something silenced it — which
        // the count below refuses.
        if written.rows_affected() == 0 {
            why_nothing_landed(&mut transaction, &entry).await?;
        }
        crate::store::write_exactly(Ok(written), 1, "appending an entry to the journal")
            .map_err(from_local)?;
        transaction
            .commit()
            .await
            .map_err(|error| refused("finishing an append to the journal", &error))
    }

    async fn from(&self, sequence: u64) -> Result<Read, Error> {
        let from = signed("sequence", sequence)?;
        let rows = sqlx::query(
            "SELECT sequence, realm, principal, actor_chain, action, resource, result, \
             deciding_policy, policy_pack, policy_epoch, grant_used, work_unit, recorded_at, \
             prev_hash, hash FROM main.local_journal_entries WHERE sequence >= ?1 \
             ORDER BY sequence",
        )
        .bind(from)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| refused("reading the journal", &error))?;
        // **The rows before a bad one are kept, and the bad one is named**: a
        // row the append-only rule leaves in place for ever must not make every
        // other row unreadable.
        let mut entries = Vec::with_capacity(rows.len());
        let mut stopped_at = None;
        for row in &rows {
            match entry_from(row) {
                Ok(entry) => entries.push(entry),
                Err(why) => {
                    stopped_at = Some(where_it_stopped(row, why));
                    break;
                }
            }
        }
        Ok(Read {
            entries,
            stopped_at,
        })
    }

    async fn unit_closing(
        &self,
        realm: &OpaqueId,
        unit: Snowflake,
    ) -> Result<Option<Outcome>, Error> {
        // The first closing bracket by sequence, by realm, unit, action and
        // outcome: the question `nc_journal::Store::unit_closing` documents.
        let closing: Option<String> = sqlx::query_scalar(
            "SELECT result FROM main.local_journal_entries \
             WHERE realm = ?1 AND work_unit = ?2 AND action = ?3 AND result IN (?4, ?5) \
             ORDER BY sequence LIMIT 1",
        )
        .bind(realm.as_str())
        .bind(unit.to_crockford())
        .bind(nc_journal::unit::CLOSES)
        .bind(Outcome::UnitCommitted.as_str())
        .bind(Outcome::UnitAborted.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| refused("asking how a unit closed", &error))?;
        Ok(closing.as_deref().and_then(Outcome::parse))
    }

    async fn unit_opened(&self, realm: &OpaqueId, unit: Snowflake) -> Result<bool, Error> {
        let opened: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM main.local_journal_entries \
             WHERE realm = ?1 AND work_unit = ?2 AND action = ?3 AND result = ?4)",
        )
        .bind(realm.as_str())
        .bind(unit.to_crockford())
        .bind(nc_journal::unit::OPENS)
        .bind(Outcome::UnitBegun.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| refused("asking whether a unit opened", &error))?;
        Ok(opened)
    }

    async fn open_units(&self, realm: &OpaqueId) -> Result<Vec<Snowflake>, Error> {
        // **The opening brackets of the realm with no closing one**, each unit
        // once at its first opening, oldest first — the statement PostgreSQL's
        // store asks, here over this table's index `(realm, work_unit)`. No
        // other column is read, so no row that fails to decode hides a unit.
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT min(opening.sequence), opening.work_unit \
             FROM main.local_journal_entries AS opening \
             WHERE opening.realm = ?1 AND opening.action = ?2 AND opening.result = ?3 \
             AND opening.work_unit IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM main.local_journal_entries AS closing \
             WHERE closing.realm = opening.realm AND closing.work_unit = opening.work_unit \
             AND closing.action = ?4 AND closing.result IN (?5, ?6)) \
             GROUP BY opening.work_unit ORDER BY min(opening.sequence)",
        )
        .bind(realm.as_str())
        .bind(nc_journal::unit::OPENS)
        .bind(Outcome::UnitBegun.as_str())
        .bind(nc_journal::unit::CLOSES)
        .bind(Outcome::UnitCommitted.as_str())
        .bind(Outcome::UnitAborted.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|error| refused("asking which units are open", &error))?;
        rows.into_iter()
            .map(|(sequence, unit)| {
                unit_of(&unit).map_err(|_| Error::Undecodable {
                    sequence: u64::try_from(sequence).unwrap_or(u64::MAX),
                    why: format!(
                        "the opening bracket names the work unit `{unit}`, which is not an \
                         identifier as this code writes one, so no unit by that name can be \
                         recovered"
                    ),
                })
            })
            .collect()
    }
}

impl LocalJournal {
    /// The largest unit any entry names, in every realm the file holds, or
    /// nothing in a journal that names none.
    ///
    /// **Ordered as the identifier, not as its text**: a unit is written in
    /// twelve Crockford digits until it needs thirteen, and digits sort before
    /// letters as the alphabet orders them, so the longest text, then the
    /// greatest of that length, is the largest identifier.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] or [`Error::Refused`] when the file could not be
    /// asked, and [`Error::Undecodable`] for a unit not spelled as this code
    /// writes one.
    pub async fn newest_unit(&self) -> Result<Option<Snowflake>, Error> {
        let row: Option<(i64, String)> = sqlx::query_as(
            "SELECT sequence, work_unit FROM main.local_journal_entries \
             WHERE work_unit IS NOT NULL \
             ORDER BY length(work_unit) DESC, work_unit DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| refused("asking for the newest unit", &error))?;
        row.map(|(sequence, unit)| {
            unit_of(&unit).map_err(|_| Error::Undecodable {
                sequence: u64::try_from(sequence).unwrap_or(u64::MAX),
                why: format!(
                    "the entry names the work unit `{unit}`, which is not an identifier as \
                     this code writes one"
                ),
            })
        })
        .transpose()
    }

    /// A connection of the pool, for the reads that run on one.
    async fn reachable(&self) -> Result<sqlx::pool::PoolConnection<sqlx::Sqlite>, Error> {
        self.pool
            .acquire()
            .await
            .map_err(|error| refused("reaching the journal", &error))
    }
}

/// The last entry on `connection`, or why the last row is not one.
async fn tail_on(connection: &mut SqliteConnection) -> Result<Option<Entry>, Error> {
    let row = sqlx::query(
        "SELECT sequence, realm, principal, actor_chain, action, resource, result, \
         deciding_policy, policy_pack, policy_epoch, grant_used, work_unit, recorded_at, \
         prev_hash, hash FROM main.local_journal_entries ORDER BY sequence DESC LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|error| refused("reading the journal's tail", &error))?;
    match row {
        None => Ok(None),
        Some(row) => entry_from(&row).map(Some).map_err(|why| {
            let (sequence, why) = where_it_stopped(&row, why);
            Error::Undecodable { sequence, why }
        }),
    }
}

/// Why an `INSERT` that ran and wrote no row wrote none, in the transaction
/// that ran it; `Ok` when nothing it reads refused the row.
///
/// **PostgreSQL's diagnosis, question for question and in its order**: an
/// opening bracket is told first of the permanent answers — its unit closed,
/// or open already — then the tail that is not one, the sequence, the link,
/// and the closed unit for every other entry. One statement reads the four
/// facts, so they are one snapshot.
async fn why_nothing_landed(connection: &mut SqliteConnection, entry: &Entry) -> Result<(), Error> {
    let record = &entry.record;
    let (top, head, closed, opened): (Option<i64>, Option<String>, bool, bool) = sqlx::query_as(
        "SELECT (SELECT max(sequence) FROM main.local_journal_entries), \
         (SELECT hash FROM main.local_journal_entries ORDER BY sequence DESC LIMIT 1), \
         EXISTS (SELECT 1 FROM main.local_journal_entries WHERE realm = ?1 \
           AND work_unit = ?2 AND action = ?3 AND result IN (?4, ?5)), \
         EXISTS (SELECT 1 FROM main.local_journal_entries WHERE realm = ?1 \
           AND work_unit = ?2 AND action = ?6 AND result = ?7)",
    )
    .bind(record.realm.as_str())
    .bind(record.work_unit.map(Snowflake::to_crockford))
    .bind(nc_journal::unit::CLOSES)
    .bind(Outcome::UnitCommitted.as_str())
    .bind(Outcome::UnitAborted.as_str())
    .bind(nc_journal::unit::OPENS)
    .bind(Outcome::UnitBegun.as_str())
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| refused("asking why an entry did not land", &error))?;
    let opening = record.action == nc_journal::unit::OPENS && record.result == Outcome::UnitBegun;
    if let Some(unit) = record.work_unit
        && opening
    {
        if closed {
            return Err(Error::UnitClosed { unit });
        }
        if opened {
            return Err(Error::UnitAlreadyOpen { unit });
        }
    }
    // A tail that does not read back as this code wrote it is a permanent
    // answer, and outranks the sequence.
    tail_on(connection).await?;
    let free = top.map_or(0, |top| u64::try_from(top).unwrap_or(u64::MAX) + 1);
    if entry.sequence != free {
        return Err(Error::OutOfOrder {
            expected: free,
            found: entry.sequence,
        });
    }
    if entry.previous.map(|hash| nc_canon::to_hex(&hash)) != head {
        return Err(Error::Undecodable {
            sequence: entry.sequence,
            why: NO_LINK.to_owned(),
        });
    }
    if let Some(unit) = record.work_unit
        && closed
    {
        return Err(Error::UnitClosed { unit });
    }
    Ok(())
}

/// One row back into an entry, or why it is not one this code wrote.
///
/// **Through `Entry::chain`, over the row's own link, and held to its stored
/// hash.** Nothing is recomputed in the row's favour: a stored hash that is not
/// the hash of the stored contents is the row refused, at its own sequence.
fn entry_from(row: &SqliteRow) -> Result<Entry, String> {
    let text = |column: &str| -> Result<String, String> {
        row.try_get::<String, _>(column)
            .map_err(|error| format!("`{column}` does not read as text: {error}"))
    };
    let maybe = |column: &str| -> Result<Option<String>, String> {
        row.try_get::<Option<String>, _>(column)
            .map_err(|error| format!("`{column}` does not read as text: {error}"))
    };
    let identifier = |column: &str, held: &str| -> Result<OpaqueId, String> {
        OpaqueId::new(held).map_err(|error| format!("`{column}` is not an identifier: {error}"))
    };
    let record = Record {
        realm: identifier("realm", &text("realm")?)?,
        principal: identifier("principal", &text("principal")?)?,
        actor_chain: actors_read(&text("actor_chain")?)?,
        action: text("action")?,
        resource: canonical("resource", &text("resource")?)?,
        result: {
            let stored = text("result")?;
            Outcome::parse(&stored).ok_or_else(|| {
                format!(
                    "`{stored}` is not an outcome this code knows, and reading it as anything \
                     else would misreport what happened"
                )
            })?
        },
        deciding_policy: maybe("deciding_policy")?,
        policy_pack: maybe("policy_pack")?,
        policy_epoch: row
            .try_get::<Option<i64>, _>("policy_epoch")
            .map_err(|error| format!("`policy_epoch` does not read as a number: {error}"))?
            .map(|epoch| unsigned("policy_epoch", epoch))
            .transpose()?,
        grant_used: maybe("grant_used")?
            .map(|grant| identifier("grant_used", &grant))
            .transpose()?,
        work_unit: maybe("work_unit")?.map(|unit| unit_of(&unit)).transpose()?,
        recorded_at: unsigned(
            "recorded_at",
            row.try_get::<i64, _>("recorded_at")
                .map_err(|error| format!("`recorded_at` does not read as a number: {error}"))?,
        )?,
    };
    let sequence = sequence_of(row).ok_or_else(|| {
        "the `sequence` column does not read as a sequence this code writes".to_owned()
    })?;
    let previous = maybe("prev_hash")?
        .map(|hex| digest("prev_hash", &hex))
        .transpose()?;
    let stored = digest("hash", &text("hash")?)?;
    let entry = Entry::chain(sequence, previous, record);
    if entry.hash == stored {
        Ok(entry)
    } else {
        Err(
            "its stored hash is not the hash of its contents, so the row was written around \
             this store or rewritten after it was"
                .to_owned(),
        )
    }
}

/// Which row stopped a read, and why — and what to say when even its sequence
/// does not read.
fn where_it_stopped(row: &SqliteRow, why: String) -> (u64, String) {
    match sequence_of(row) {
        Some(sequence) => (sequence, why),
        None => (
            u64::MAX,
            format!(
                "its `sequence` column is one this code cannot read, so the row cannot say \
                 which entry it is and the number here is not one: {why}"
            ),
        ),
    }
}

/// A row's `sequence`, read on its own so a row that does not decode can
/// still say which it is.
fn sequence_of(row: &SqliteRow) -> Option<u64> {
    u64::try_from(row.try_get::<i64, _>("sequence").ok()?).ok()
}

/// The delegation chain as it is written: the canonical JSON array of its
/// identifiers.
fn actors_written(chain: &[OpaqueId]) -> String {
    Value::Array(
        chain
            .iter()
            .map(|actor| Value::text(actor.as_str()))
            .collect(),
    )
    .to_canonical()
}

/// The delegation chain read back, only from the text [`actors_written`]
/// writes.
fn actors_read(stored: &str) -> Result<Vec<OpaqueId>, String> {
    let chain = canonical("actor_chain", stored)?
        .into_array()
        .ok_or_else(|| "`actor_chain` is not an array".to_owned())?
        .into_iter()
        .map(|actor| {
            actor
                .into_text()
                .ok_or_else(|| "an actor is not a string".to_owned())
                .and_then(|actor| {
                    OpaqueId::new(&actor)
                        .map_err(|error| format!("an actor is not an identifier: {error}"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(chain)
}

/// A column holding canonical JSON, read back only from the text this code
/// writes: `nc_canon::parse` accepts more spellings than it emits, and a row
/// respelled after it was written would verify and be another row to every
/// reader that looks for the text.
fn canonical(column: &str, stored: &str) -> Result<Value, String> {
    let parsed = nc_canon::parse(stored)
        .map_err(|error| format!("`{column}` is not canonical JSON: {error}"))?;
    let written = parsed.to_canonical();
    if written == stored {
        Ok(parsed)
    } else {
        Err(format!(
            "`{column}` is stored as `{stored}` and this value is written `{written}`, so the \
             row was respelled after it was written"
        ))
    }
}

/// A work unit read back only from the spelling this code writes: the
/// Crockford reader is lenient, and a respelled unit is one no question about
/// it finds.
///
/// **Through `from_crockford`, which is allowed here and nowhere the log is**:
/// this crate does not reach `nc-log`.
fn unit_of(stored: &str) -> Result<Snowflake, String> {
    let read = Snowflake::from_crockford(stored)
        .map_err(|error| format!("`work_unit` is not an identifier: {error}"))?;
    if read.to_crockford() == stored {
        Ok(read)
    } else {
        Err(format!(
            "the work unit is stored as `{stored}` and this identifier is written `{}`, so \
             the row was respelled after it was written",
            read.to_crockford()
        ))
    }
}

/// A stored hex column back into a digest, its width checked.
fn digest(column: &str, hex: &str) -> Result<[u8; 32], String> {
    let bytes =
        nc_canon::from_hex(hex).map_err(|error| format!("`{column}` is not hex: {error}"))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        format!(
            "`{column}` holds {} byte(s) and a digest is 32",
            bytes.len()
        )
    })
}

/// A stored integer as the unsigned number it was written from.
fn unsigned(column: &str, value: i64) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("`{column}` holds {value}, below zero"))
}

/// A number as SQLite's signed integer, or the record refused naming the field.
///
/// **`Unstorable`, never a cast**: a `u64` past `i64::MAX` cast would be a
/// negative number sorting before every real entry, and the store was never
/// asked, so it is not the store being down.
fn signed(field: &'static str, value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::Unstorable {
        field,
        why: format!(
            "it is {value}, which SQLite's INTEGER cannot hold. Casting it would store a \
             negative number that sorts before every real entry"
        ),
    })
}

/// The crate's failure as the journal's: SQLite not answering is the store
/// not answering, and anything else — a file this build does not expect, a
/// temporary object on the connection, a write something silenced — is the
/// store answering and refusing, which no retry changes.
fn from_local(error: LocalError) -> Error {
    match error {
        LocalError::Unavailable(why) => Error::Unavailable(why),
        LocalError::Refused(why) | LocalError::Unexpected(why) | LocalError::Migration(why) => {
            Error::Refused(why)
        }
    }
}

/// One `sqlx` error, as the journal's refusal.
///
/// **A constraint is the record refused, not the store failing**: what
/// PostgreSQL's class 23 is to `nc_journal::postgres`, SQLite's primary code
/// 19 (`SQLITE_CONSTRAINT`) is here — but a trigger's refusal, which carries
/// the same primary code, is the schema refusing a statement this crate
/// writes, which the crate's own reading names.
fn refused(doing: &str, error: &sqlx::Error) -> Error {
    let constraint = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .and_then(|code| code.parse::<u32>().ok())
        .is_some_and(|code| code & 0xff == 19);
    if constraint && !crate::is_a_trigger_refusal(error) {
        return Error::Unstorable {
            field: "a value in the statement",
            why: format!(
                "{doing}: {error}. The store answered; it is the request it could not take, so \
                 this is not the store being unavailable"
            ),
        };
    }
    from_local(crate::said(doing, error))
}

#[cfg(test)]
mod tests {
    use super::{
        actors_read, actors_written, canonical, digest, from_local, refused, signed, unit_of,
    };
    use crate::LocalError;
    use nc_ids::OpaqueId;
    use nc_journal::Error;

    /// A work unit is read back only from the spelling this code writes: the
    /// lenient reader's other spellings of the same identifier are refused.
    #[test]
    fn a_work_unit_is_read_back_only_as_it_is_written() {
        let written = "01J8XK2M9P51";
        assert_eq!(
            unit_of(written).map(nc_ids::Snowflake::to_crockford),
            Ok(written.to_owned())
        );
        for respelled in ["01j8xk2m9p51", "O1J8XK2M9P51"] {
            let refused = unit_of(respelled).expect_err("a respelled unit");
            assert!(refused.contains("respelled"), "{respelled}: {refused}");
        }
        assert!(unit_of("not a unit").is_err());
    }

    /// A column of canonical JSON is read back only as the text this code
    /// writes, though the parser accepts more.
    #[test]
    fn a_canonical_column_is_read_back_only_as_it_is_written() {
        assert!(canonical("resource", r#"{"a":1}"#).is_ok());
        for respelled in [r#"{ "a": 1 }"#, r#"{"\u0061":1}"#] {
            let refused = canonical("resource", respelled).expect_err("respelled");
            assert!(refused.contains("respelled"), "{respelled}: {refused}");
        }
        let refused = canonical("resource", "not json").expect_err("not JSON");
        assert!(refused.contains("not canonical JSON"), "{refused}");
    }

    /// The delegation chain goes back and forth through the text it is
    /// written as, and through no other.
    #[test]
    fn an_actor_chain_is_read_back_only_as_it_is_written() {
        let chain = vec![
            OpaqueId::new("agent-1").expect("an identifier"),
            OpaqueId::new("principal-1").expect("an identifier"),
        ];
        let written = actors_written(&chain);
        assert_eq!(written, r#"["agent-1","principal-1"]"#);
        assert_eq!(actors_read(&written), Ok(chain));
        assert_eq!(actors_read("[]"), Ok(Vec::new()));
        for refused in [r#"["agent-1", "principal-1"]"#, r#"{"a":1}"#, "[1]"] {
            assert!(actors_read(refused).is_err(), "{refused}");
        }
    }

    /// A digest is thirty-two bytes of hex, its width checked.
    #[test]
    fn a_digest_is_thirty_two_bytes_of_hex() {
        assert_eq!(digest("hash", &"ab".repeat(32)), Ok([0xab; 32]));
        let short = digest("hash", &"ab".repeat(31)).expect_err("short");
        assert!(short.contains("31 byte(s)"), "{short}");
        let not_hex = digest("hash", &"zz".repeat(32)).expect_err("not hex");
        assert!(not_hex.contains("not hex"), "{not_hex}");
    }

    /// A number past what SQLite holds is the record refused, never a cast.
    #[test]
    fn a_number_past_what_sqlite_holds_is_unstorable() {
        assert_eq!(signed("sequence", 7), Ok(7));
        assert!(
            matches!(
                signed("sequence", u64::MAX),
                Err(Error::Unstorable {
                    field: "sequence",
                    ..
                })
            ),
            "a sequence past `i64::MAX` is stored as nothing"
        );
    }

    /// The crate's own failures are the journal's: SQLite not answering is
    /// the store not answering, and every other is the store refusing.
    #[test]
    fn the_crates_failures_are_the_journals_refusals() {
        assert_eq!(
            from_local(LocalError::Unavailable("busy".to_owned())),
            Error::Unavailable("busy".to_owned())
        );
        for refusal in [
            LocalError::Unexpected("a temporary object".to_owned()),
            LocalError::Refused("not private".to_owned()),
            LocalError::Migration("did not apply".to_owned()),
        ] {
            assert!(
                matches!(from_local(refusal.clone()), Error::Refused(_)),
                "{refusal:?}"
            );
        }
    }

    /// Each answer SQLite gives is the journal refusal it is: a constraint the
    /// record refused, a trigger's refusal the store refusing, and an answer
    /// that never came the store not answering.
    #[tokio::test]
    async fn each_answer_sqlite_gives_is_the_refusal_it_is() {
        use sqlx::{ConnectOptions as _, Connection as _};
        let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
            .in_memory(true)
            .connect()
            .await
            .expect("a database in memory");
        for statement in [
            "CREATE TABLE bounded (x INTEGER CHECK (x > 0)) STRICT",
            "CREATE TABLE guarded (x INTEGER) STRICT",
            "CREATE TRIGGER no BEFORE INSERT ON guarded BEGIN SELECT RAISE(ABORT, 'no'); END",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("the scratch schema builds");
        }
        let constraint = sqlx::query("INSERT INTO bounded (x) VALUES (-1)")
            .execute(&mut connection)
            .await
            .expect_err("the check refuses it");
        assert!(
            matches!(
                refused("appending", &constraint),
                Error::Unstorable {
                    field: "a value in the statement",
                    ..
                }
            ),
            "{constraint}"
        );
        let trigger = sqlx::query("INSERT INTO guarded (x) VALUES (1)")
            .execute(&mut connection)
            .await
            .expect_err("the trigger refuses it");
        assert!(
            matches!(refused("appending", &trigger), Error::Refused(_)),
            "{trigger}"
        );
        assert!(matches!(
            refused("appending", &sqlx::Error::PoolTimedOut),
            Error::Unavailable(_)
        ));
        connection.close().await.expect("closes");
    }
}
