//! The journal in PostgreSQL.
//!
//! The other implementation of [`Store`], and the one that runs. `InMemory` is
//! what makes acceptance criterion 3 provable at every position of a synthetic
//! history; this is what the platform writes to.
//!
//! # What this file does not decide
//!
//! **Append-only is the table's rule, not this type's.** `migrations/0001_journal.sql` puts
//! a trigger on `journal_entries` that raises on `UPDATE` and on `DELETE`, so a
//! second connection — a migration, a console, a future service, a person with
//! `psql` — meets the same refusal this code would. A rule enforced only where
//! it is convenient to enforce it is a rule about one code path.
//!
//! **Nothing here re-chains.** `append` takes an entry the caller has already
//! hashed and linked, and writes it as it is. Recomputing a hash on the way into
//! storage would mean the value verified is one this file produced rather than
//! one the caller committed to, and the chain would then attest to the database
//! layer instead of to the decision.
//!
//! # The two conversions that can fail, and why they are not casts
//!
//! PostgreSQL has no unsigned integers, so `sequence`, `recorded_at` and
//! `policy_epoch` are `BIGINT`. A `u64 as i64` cast turns everything above
//! `i64::MAX` into a negative number silently, and a negative sequence would sort
//! before every real entry — a chain that verifies right up until somebody reads
//! it in order. Both directions are `try_from`, and a value that does not fit is
//! an error naming the field.

use nc_ids::{OpaqueId, Snowflake};
use sqlx::{PgPool, Row as _};

use crate::{Entry, Error, Outcome, Read, Record, Store};

/// The journal's table in PostgreSQL.
#[derive(Debug, Clone)]
pub struct Postgres {
    pool: PgPool,
    /// The schema this store's statements run in, written down rather than
    /// resolved. See [`Postgres::in_schema`].
    schema: String,
}

/// Freezes this session's path to the schema this store was given, then `pg_temp`.
///
/// **Appending `pg_temp` is half the rule, and the half that does
/// less.** Keeping the caller's path and only moving the temporary schema to
/// the end leaves a production pool on `"$user", public, pg_temp`, and `$user`
/// is not a schema: it is an instruction to look for one named after whoever
/// connects, expanded afresh on every statement. Measured through the shipped
/// entry point that way, with a schema named after the connecting role holding
/// a copy of the table:
///
/// ```text
/// session search_path = "$user", public
/// append  -> Ok(())
/// public.journal_entries = 0,  noctua.journal_entries = 1
/// tail    -> Ok(true)
/// ```
///
/// `CREATE TABLE … LIKE … INCLUDING ALL` copies constraints and indexes and not
/// triggers, so nothing refuses the row, the writer believes it has appended,
/// and the audit log never receives it. That is the defect the migrations
/// close, one layer over — and the migrations' own answer is the one used here:
/// resolve the schema once, and write its name down.
///
/// **The name is a value, bound to `$1`.** Not `current_schema()`: `grep`
/// for `current_schema` in this crate's `src/` returns comments only, and a
/// paragraph condemning a written name would condemn exactly what
/// `Postgres::new` does — `Self::in_schema(pool, "public")` — so a reader who
/// trusted it would "fix" the store back into the hole the migrations closed.
///
/// [`Postgres::in_schema`]'s own doc carries the argument: every one
/// of `current_user`, `session_user` and `current_schema()` resolves
/// *through* `search_path`, which begins `"$user"` by default. So the schema is
/// a value the caller supplies, and `new` writes `public` down.
///
/// `quote_ident` is what makes a schema whose name holds a comma or a capital
/// survive being written into a path; one of the test schemas is named after a
/// script called "an entry after its own bracket, at the right sequence".
///
/// Running it twice on a pooled connection is the same as running it once: the
/// value bound to `$1` is the same both times.
const PATH: &str = "SELECT set_config('search_path', quote_ident($1) || ', pg_temp', false)";

impl Postgres {
    /// Why an `INSERT` that ran and wrote no row wrote none.
    ///
    /// Its own function because inside `append` it would cross a hundred lines,
    /// and because this is a different job from writing: everything above is
    /// the statement, and from here down the statement has already failed and
    /// the question is which of three things a caller can act on.
    ///
    /// # Errors
    ///
    /// Always — it is only called when the write was refused — and *which*
    /// refusal is the whole point of it: [`Error::UnitClosed`] or
    /// [`Error::UnitAlreadyOpen`] when the work unit is the reason,
    /// [`Error::OutOfOrder`] when the entry does not follow the tail,
    /// [`Error::Undecodable`] when the tail is not what this code wrote, and
    /// [`Error::Contended`] when the answer is that nothing is wrong and another
    /// writer settled it in between.
    ///
    /// **Not "always" alone.** Three of those five are not retryable
    /// and `Contended` is, so a caller acting on a sentence that stopped there
    /// rather than on the value would get the retry decision backwards for four
    /// cases out of five.
    async fn why_nothing_landed(&self, entry: &Entry) -> Result<(), Error> {
        let record = &entry.record;
        // **Every question in one statement, because two statements are two
        // snapshots.** Asking the four things — is the unit closed, is it open,
        // what sequence is free, does the entry link to the head — as four
        // separate reads, each taking its own `READ COMMITTED` snapshot after a
        // write that has already been refused, lets a winner committing
        // anywhere in that sequence change the answer half-way through, so a
        // loser is told `OutOfOrder` (or falls all the way through to
        // `Contended`, which is the same code) for a unit that is open and will
        // never not be.
        //
        // No arrangement of separate reads works, because the arrangement is
        // not the problem: asking the sequence first so the answer "cannot go
        // stale"; asking the unit again after the sequence read that proves a
        // writer landed; and folding the tail into the unit read so those two
        // agree. Measured on an unmodified tree with separate reads,
        // `cargo xtask check` is red in 4 of 8 runs, and the last of those
        // arrangements still fails 2 in 8 under the full suite:
        //
        // ```text
        // sixteen identical calls were given 2 different error codes:
        // {"JournalContended", "JournalUnitAlreadyOpen"}
        // ```
        //
        // `JournalContended` is `retryable = true`, severity *warning*, and no
        // retry ever opens a unit that is already open. A gate that fails half
        // the time is one people learn to re-run, which
        // `xtask/canaries/README.md` calls the same as not having it.
        //
        // One statement is one snapshot and the four answers are consistent by
        // construction. It is the argument this file already makes about the
        // `INSERT`'s own `WHERE` — *the check and the write are one statement,
        // so the answer cannot change underneath it* — applied to the diagnosis
        // that runs when that `WHERE` refuses.
        let unit = record.work_unit.map(nc_ids::Snowflake::to_crockford);
        let (top, head, closed, opened): (Option<i64>, Option<String>, bool, bool) =
            sqlx::query_as(
                "SELECT (SELECT MAX(sequence) FROM journal_entries), \
                 (SELECT hash FROM journal_entries ORDER BY sequence DESC LIMIT 1), \
                 EXISTS (SELECT 1 FROM journal_entries WHERE realm = $1 \
                   AND work_unit = $2 AND action = $3 AND result = ANY($4)), \
                 EXISTS (SELECT 1 FROM journal_entries WHERE realm = $1 \
                   AND work_unit = $2 AND action = $5 AND result = $6)",
            )
            .bind(record.realm.as_str())
            .bind(unit.as_deref())
            .bind(crate::unit::CLOSES)
            .bind(vec![
                Outcome::UnitCommitted.as_str().to_owned(),
                Outcome::UnitAborted.as_str().to_owned(),
            ])
            .bind(crate::unit::OPENS)
            .bind(Outcome::UnitBegun.as_str())
            .fetch_one(&mut *self.reachable().await?)
            .await
            .map_err(|error| unavailable(&error))?;

        // **The permanent answers first, and they are permanent because a unit
        // that is closed stays closed and one that is open stays open.** A
        // sequence moves; reporting the transient answer would tell fifteen of
        // sixteen concurrent openers to retry a condition that never clears.
        // **Only for an opening bracket, which is where the prologue asks it.**
        // Asking `closed` for every record here would move it in front of the
        // sequence and the link, and the in-memory store answers those first —
        // measured, two tests red on the rule that both stores answer one call
        // one way: `OutOfOrder { expected: 2, found: 9 }` against
        // `UnitClosed { .. }`. For an ordinary entry the closed-unit question
        // is asked last, below.
        let opening = record.action == crate::unit::OPENS && record.result == Outcome::UnitBegun;
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
        // **And the tail still has to be its own contents.** Reading the
        // sequence from the snapshot above skips the tail check unless it is
        // asked here — measured without it, three tests red:
        // `a_forged_tail_is_not_contention_and_retrying_does_not_bless_it`,
        // `an_entry_wrong_two_ways_answers_with_the_more_severe_one` and
        // `both_stores_answer_the_same_script_the_same_way`. A forged tail is a
        // permanent answer and outranks the sequence, so it is asked here.
        if let Some(tail) = self.tail().await?
            && tail.hash != tail.compute_hash()
        {
            return Err(Error::Undecodable {
                sequence: tail.sequence,
                why: "the entry this one would chain onto does not match its own contents, \
                      so the history it would extend is not the history it was read from"
                    .to_owned(),
            });
        }
        // The sequence, read from the same snapshot as the two above.
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
                why: "this entry does not link to the tail it would follow, so the history \
                      it joins could never verify. `Entry::chain` is what links one"
                    .to_owned(),
            });
        }
        // The closed-unit question for every other kind of record, in the
        // position `about_the_unit` held — and from the same snapshot as the
        // three answers above, which is the whole point of this rewrite.
        if let Some(unit) = record.work_unit
            && closed
        {
            return Err(Error::UnitClosed { unit });
        }
        // **And when every answer says "nothing wrong", say that.** Reaching
        // here means the statement was refused and every check it carries
        // passes in the snapshot this read took: another connection settled the
        // disagreement between the write and the question. Reported as
        // `OutOfOrder { expected: free, found: entry.sequence }` — reached only
        // when those two are equal — it would print the same number twice and
        // read as *nothing happened*.
        Err(Error::Contended)
    }

    /// The diagnosis, so a test can drive it without winning a race.
    ///
    /// **The arm this pins is only reachable under one.** `0012`'s density
    /// trigger fires when the `INSERT`'s `WHERE` passed and the tail moved
    /// before the row landed; there is no way to arrange that from one thread,
    /// and the sixteen-way race that does arrange it catches a defect there in
    /// roughly one run in six even with five rounds. A test that catches a race
    /// one run in six is one people learn to re-run.
    ///
    /// So the question is asked directly: for an opening bracket of a unit that
    /// is already open, the answer must be `UnitAlreadyOpen` whatever the
    /// sequence says — because a unit that is open stays open and a sequence
    /// moves. The race test beside it is what says this function is the one the
    /// refused `INSERT` reaches.
    ///
    /// # Errors
    ///
    /// Returns the refusal it diagnoses, or `Error::Contended` when every
    /// check passes.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn why_nothing_landed_for_tests(&self, entry: &Entry) -> Result<(), Error> {
        self.why_nothing_landed(entry).await
    }

    /// A store over an existing pool.
    ///
    /// Takes the pool rather than a URL: one process holds one pool, and a type
    /// that opened its own would make the connection limit a function of how
    /// many stores somebody happened to construct.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::in_schema(pool, "public")
    }

    /// The same, for a deployment whose journal is not in `public`.
    ///
    /// **The store cannot work out which journal is the journal, so it is
    /// told.** Asking the database — nothing at all, or "keep the caller's path
    /// and move `pg_temp` to the end", or `current_schema()` — resolves
    /// through `search_path` every time, which begins `"$user"` by
    /// default. That is not a schema: it is an instruction to look for one
    /// named after whoever connects, expanded afresh on every
    /// statement. Measured through the shipped entry point on a pool built with
    /// no options, with a schema named after the connecting role holding a copy
    /// of the table:
    ///
    /// ```text
    /// session search_path = "$user", public
    /// append  -> Ok(())
    /// public.journal_entries = 0,  <role>.journal_entries = 1
    /// tail    -> Ok(true)
    /// ```
    ///
    /// `LIKE … INCLUDING ALL` copies constraints and indexes and not triggers,
    /// so nothing refuses the row, the writer believes it has appended, and the
    /// audit log never receives it — `§14.9`'s fail-closed rule never engaging,
    /// because as far as the writer can tell nothing has failed.
    ///
    /// So the schema is a value. `new` writes `public` down because that is where
    /// the migrations create their tables; a deployment that puts them elsewhere
    /// says which, once, here. `xtask` reads the same answer out of the
    /// connection string rather than asking the connection, for the same reason.
    #[must_use]
    pub fn in_schema(pool: PgPool, schema: &str) -> Self {
        Self {
            pool,
            schema: schema.to_owned(),
        }
    }

    /// The connection this statement will run on, with the journal reachable.
    ///
    /// **Every statement in this file names `journal_entries` unqualified, so
    /// the path is pinned.** The migrations close that class inside trigger
    /// *bodies*; unpinned, the store's own statements would be decided by
    /// whoever planted a name first, and `pg_temp` is searched ahead
    /// of `search_path` for relations — which is the fact the migrations are
    /// built on. Measured, with the store's own statements copied out of this
    /// file:
    ///
    /// ```text
    /// CREATE TEMP TABLE journal_entries (LIKE public.journal_entries);
    /// INSERT INTO journal_entries (…) SELECT 0, … WHERE 0-1 = COALESCE((SELECT MAX(sequence) …
    /// INSERT 0 1                  <- rows_affected() == 1, so `append` returns Ok
    /// SELECT … FROM public.journal_entries;
    ///  0 | p | aaaa…              <- the audit log never received it
    /// ```
    ///
    /// No trigger fires, because the triggers are on the real table. The writer
    /// believes it appended and `head`/`tail` agree with it, so `§14.9`'s
    /// fail-closed rule never engages: the one table where that matters most
    /// would have neither of the two guards `nc-prune` already asks for its
    /// own.
    ///
    /// `pg_temp` last is the same rule the migrations follow, and it is set on
    /// the connection rather than asserted about it — a check would race a second
    /// statement on the same session.
    async fn reachable(&self) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, Error> {
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(|error| Error::Unavailable(told(&error)))?;
        sqlx::query(PATH)
            .bind(&self.schema)
            .execute(&mut *connection)
            .await
            .map_err(|error| Error::Unavailable(told(&error)))?;
        Ok(connection)
    }

    /// The pool it writes through.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Every column, in one place, so the three queries cannot drift apart.
/// `actor_chain` comes back twice: once decoded, and once as the **text the
/// column holds**, because the guard on it compares the stored bytes and
/// `jsonb` cannot be read as a string. The cast is PostgreSQL's own rendering,
/// which is the form this code writes through.
const COLUMNS: &str = "sequence, realm, principal, actor_chain, \
                       actor_chain::text AS actor_chain_text, action, resource, result, \
                       deciding_policy, policy_pack, policy_epoch, grant_used, work_unit, \
                       recorded_at, prev_hash, hash";

impl Store for Postgres {
    async fn tail(&self) -> Result<Option<Entry>, Error> {
        {
            let query =
                format!("SELECT {COLUMNS} FROM journal_entries ORDER BY sequence DESC LIMIT 1");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*query))
                .fetch_optional(&mut *self.reachable().await?)
                .await
                .map_err(|error| unavailable(&error))?;
            // Named, like `from`: a tail this code cannot read is a row somebody
            // else wrote, not a store that failed to answer.
            match row {
                None => Ok(None),
                Some(row) => match entry_from(&row) {
                    Ok(entry) => Ok(Some(entry)),
                    Err(why) => {
                        let (sequence, why) = where_it_stopped(&row, &why);
                        Err(Error::Undecodable { sequence, why })
                    }
                },
            }
        }
    }

    async fn head(&self) -> Result<Option<(u64, [u8; 32])>, Error> {
        use sqlx::Row as _;

        // **Two columns, and neither is one that can fail to decode.** The
        // resource is what a foreign `INSERT` poisons; the sequence and the hash
        // are a `BIGINT` and 64 hex characters, and a row whose *hash* is
        // unreadable is one nothing could chain onto anyway — so that is
        // reported rather than worked around.
        let row = sqlx::query(
            "SELECT sequence, hash FROM journal_entries ORDER BY sequence DESC LIMIT 1",
        )
        .fetch_optional(&mut *self.reachable().await?)
        .await
        .map_err(|error| unavailable(&error))?;
        let Some(row) = row else {
            return Ok(None);
        };
        // **A row this code cannot read is `Undecodable`, not `Unavailable`.**
        // `unsigned` and `digest` both build `Error::Unavailable`, which is
        // `NC-JRNL-0003` — *"the journal store did not answer"*,
        // `retryable = true` — and every read path remaps them at the row
        // boundary. Unremapped here, a negative `sequence` or a hash of the
        // wrong width would reach a responder as a transient outage. Measured,
        // one malformed row:
        //
        // ```text
        // head() code    = JournalUnavailable        retryable = true
        // head() display = the journal store did not answer: `hash` holds 2
        //                  byte(s) … was not written by this code
        // ```
        //
        // The sentence contradicts itself in one line — the store answered
        // perfectly well — which is the contradiction `reason` removes on the
        // other path, and which this crate states as a rule in three places.
        // Tampering evidence must not arrive as a retry.
        let poisoned = |sequence: u64| {
            move |error: Error| Error::Undecodable {
                sequence,
                why: reason(&error),
            }
        };
        // **A column that will not decode is a row this code did not write.**
        // Mapped to `unavailable` — `NC-JRNL-0003`, *"the journal store did not
        // answer"*, **critical**, `retryable = true` — it would contradict the
        // paragraph saying tampering evidence must not arrive as a retry, which
        // is why the `try_get` is remapped as well as `unsigned` and `digest`:
        // measured with it unmapped, after an
        // `ALTER COLUMN sequence TYPE NUMERIC`, which fires no row trigger,
        // `head()` answers the store being down for a column that has been
        // rewritten under it.
        let raw: i64 = row
            .try_get("sequence")
            .map_err(|error| Error::Undecodable {
                sequence: u64::MAX,
                why: format!(
                    "the `sequence` column does not read as a number this code wrote: {error}"
                ),
            })?;
        // The sequence names the row even when it is the thing that will not
        // decode: a negative one is still which row, and `u64::MAX` says "the
        // last row, and its number is not a number".
        let named = u64::try_from(raw).unwrap_or(u64::MAX);
        let sequence = unsigned("sequence", raw).map_err(poisoned(named))?;
        // **The fourth read, converted like the other three.** `sequence`'s
        // `try_get`, `unsigned` and `digest` all build `Undecodable`; built as
        // `Unavailable` here, a `hash` column rewritten under the table would
        // come back as `NC-JRNL-0003`, *"the journal store did not answer"*,
        // **critical**, `retryable = true`, while `tail()` on the same row says
        // the chain is broken. Two reads of one row, two answers.
        let hash: String = row.try_get("hash").map_err(|error| Error::Undecodable {
            sequence: named,
            why: format!("the `hash` column does not read as text this code wrote: {error}"),
        })?;
        let hash = digest("hash", &hash).map_err(poisoned(sequence))?;
        Ok(Some((sequence, hash)))
    }

    async fn append(&mut self, entry: Entry) -> Result<(), Error> {
        {
            // **Here, not one layer up.** `Journal::append` is `pub(crate)` and
            // this is public, so a rule living there would be a rule about one
            // code path — and this crate's own tests call `Store` directly six
            // times.
            crate::readable(&entry)?;
            // **The documented contract, not only what the index enforces.**
            // `Store::append` promises `OutOfOrder` when the entry does not
            // follow the tail, and noticing only a primary-key collision would
            // accept sequence 7 appended into an empty table, while `InMemory`
            // refuses the same call. Two implementations of one documented
            // method would disagree, and the chain only report it later as
            // `HistoryDoesNotStartHere`.
            //
            // **Checked inside the insert, not before it.** A `SELECT` for the
            // tail followed by an `INSERT` is two round trips — and
            // `Journal::append` has already read the tail to build the entry,
            // so every write would cost two identical queries. Worse, the gap
            // between the two is a race: two callers could both read tail 6,
            // both decide 7 was next, and the second would fail on the primary
            // key with a message about a duplicate rather than about ordering.
            // The `WHERE` below makes the check and the write one statement, so
            // the answer cannot change underneath it.
            //
            // **`$1 - 1 = MAX(sequence)`, not `$1 = MAX(sequence) + 1`.** The
            // second overflows `BIGINT` the moment any row holds `i64::MAX`,
            // and the whole table is then unwritable for ever: measured, one
            // ordinary `INSERT` at that sequence — no exotic column, nothing a
            // reader would call tampering, only the privilege the writing
            // service already has — and every later append comes back
            // `bigint out of range`. `DELETE`, `UPDATE` and `TRUNCATE` are all
            // refused by this table's own triggers and no command in
            // `catalog/commands.toml` can undo it, so recovery would mean
            // dropping the audit log. `$1` is a checked `i64` and sequences are
            // non-negative, so `$1 - 1` cannot overflow; the empty table reads
            // `-1`, which is the sequence before 0.
            //
            // **And the link.** `readable` verifies an entry against itself and
            // says nothing about what it claims to follow, so
            // `Entry::chain(n, Some([0xAA; 32]), record)` is self-consistent
            // and, unchecked here, lands through the public `Store::append` —
            // measured, with `verify` then condemning the history for ever at
            // that row while `Journal::record` keeps answering `Ok`, because
            // the forged entry's own hash *is* its own hash. `InMemory` refuses
            // the same call and says why; this is the same door, one field
            // along, closed in both stores. `IS NOT DISTINCT FROM` is what
            // makes the first entry's `NULL` link match an empty table.
            let record = &entry.record;
            let result = sqlx::query(
                "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
                 resource, result, deciding_policy, policy_pack, policy_epoch, grant_used, \
                 work_unit, recorded_at, prev_hash, hash) \
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15 \
                 WHERE $1 - 1 = COALESCE((SELECT MAX(sequence) FROM journal_entries), -1) \
                 AND $14 IS NOT DISTINCT FROM \
                 (SELECT hash FROM journal_entries ORDER BY sequence DESC LIMIT 1) \
                 AND NOT EXISTS (SELECT 1 FROM journal_entries \
                 WHERE realm = $2 AND work_unit = $12 AND action = $17 \
                 AND result = ANY($16)) \
                 AND NOT ($5 = $18 AND $7 = $19 AND EXISTS (SELECT 1 FROM journal_entries \
                 WHERE realm = $2 AND work_unit = $12 AND action = $18 AND result = $19))",
            )
            .bind(signed("sequence", entry.sequence)?)
            .bind(record.realm.as_str())
            .bind(record.principal.as_str())
            .bind(serde_json::Value::Array(
                record
                    .actor_chain
                    .iter()
                    .map(|actor| serde_json::Value::String(actor.as_str().to_owned()))
                    .collect(),
            ))
            .bind(&record.action)
            .bind(record.resource.to_canonical())
            .bind(record.result.as_str())
            .bind(record.deciding_policy.as_deref())
            .bind(record.policy_pack.as_deref())
            .bind(
                record
                    .policy_epoch
                    .map(|epoch| signed("policy_epoch", epoch))
                    .transpose()?,
            )
            .bind(record.grant_used.as_ref().map(OpaqueId::as_str))
            .bind(record.work_unit.map(nc_ids::Snowflake::to_crockford))
            .bind(signed("recorded_at", record.recorded_at)?)
            .bind(entry.previous.map(|hash| nc_canon::to_hex(&hash)))
            .bind(nc_canon::to_hex(&entry.hash))
            // **And the closed-unit check, in the same statement.**
            // `Store::unit_closed` answers the question, and `Journal::append`
            // asks it before reading the head and before writing — three
            // statements with nothing holding the answer still. Measured with
            // the gap made explicit and the check outside the statement: a
            // second journal on the same pool and realm commits the unit
            // between the check and the write, and the entry lands at sequence
            // 2, *after* its own `unit-committed` bracket at 1, with `verify`
            // answering `Ok(3)`.
            //
            // That is the defect the single-process path guards against, and
            // `Store::unit_closing`'s own doc sells the store query on the
            // multi-process case. This is the argument the ordering check above
            // already makes, applied to the second question: the `WHERE` makes
            // the check and the write one statement, so the answer cannot
            // change underneath it.
            //
            // `$12` is `work_unit`, which is `NULL` for an entry outside a unit
            // — and `work_unit = NULL` is never true, so the `NOT EXISTS` holds
            // vacuously and an ordinary entry is unaffected.
            .bind(vec![
                Outcome::UnitCommitted.as_str().to_owned(),
                Outcome::UnitAborted.as_str().to_owned(),
            ])
            // **And the action, which is what makes it a bracket.** This
            // predicate exists three times — here, in `unit_closing`, and in
            // the in-memory store — and all three ask realm, unit, action and
            // result. Without the action, an ordinary `Write` carrying
            // `Outcome::UnitCommitted` closes somebody else's group, measured
            // against a real database.
            .bind(crate::unit::CLOSES)
            // **And the opening bracket, in the same statement.** As a separate
            // `SELECT` in `Journal::append`, with `head()` and this INSERT
            // after it, two journals both ask before either writes, both are
            // told "not open", and both openings land. Measured against a real
            // database:
            // `openings for the unit, action and outcome both right: 2`. That
            // is verbatim the interleaving the paragraph above says a separate
            // statement cannot survive, which is why both rules are folded in
            // here.
            .bind(crate::unit::OPENS)
            .bind(Outcome::UnitBegun.as_str())
            .execute(&mut *self.reachable().await?)
            .await;

            match result {
                // **Zero rows is the ordering refusal**, and it is the common
                // one, because the check is the insert's own `WHERE`. Nothing
                // was written; the tail says what should have been.
                Ok(done) if done.rows_affected() == 0 => self.why_nothing_landed(&entry).await,
                Ok(_) => Ok(()),
                // **A primary-key collision is `OutOfOrder`, not `Unavailable`.**
                // It is what a second writer racing this one looks like from
                // here: the sequence it computed from the tail was taken between
                // the read and the write. Reporting it as the store being down
                // would send a responder to look at PostgreSQL.
                Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                    let _ = error;
                    // **The same diagnosis as the other arm, because it is the
                    // same question.** Asking about the work unit first here
                    // while the zero-rows arm asks the sequence first would
                    // give one call two answers depending on which arm the
                    // scheduler put it in: measured, 60 rounds of 8 ordinary
                    // entries racing their unit's closing bracket give 288
                    // `OutOfOrder`/`JournalContended`/warning/retryable and 151
                    // `UnitClosed`/`JournalUnitClosed`/error/not-retryable —
                    // 34% crossing to a different variant, a different code and
                    // the opposite advice, with `InMemory` never agreeing with
                    // the 151. A unique violation means the sequence is taken,
                    // which `why_nothing_landed` reaches on its own.
                    self.why_nothing_landed(&entry).await
                }
                // **And the density trigger's refusal is diagnosed, not
                // translated.** `unavailable` reads `0012`'s message —
                // "sequence 0 is not the next one, which is 1" — and returns
                // `OutOfOrder` from the two numbers in it, without ever asking
                // about the work unit. So translated alone, an opener of an
                // already-open unit whose `INSERT` passed the `WHERE` and was
                // then stopped by the trigger would come back
                // `JournalContended`: `retryable = true`, severity *warning*,
                // for a condition no retry clears.
                //
                // This is the third arm of the same statement. With only the
                // zero-rows arm and the unique-violation arm diagnosing,
                // measured, `cargo xtask check` is red in 4 of 8 runs with
                // `sixteen identical calls were given 2 different error codes: {"JournalContended", "JournalUnitAlreadyOpen"}`,
                // and rewriting the diagnosis to read every fact in one
                // snapshot does not touch it, because this arm never reaches
                // the diagnosis.
                //
                // Only for `OutOfOrder`: every other translation `unavailable`
                // makes is about the store rather than about this entry, and
                // re-reading the table would say nothing about it.
                Err(error) => match unavailable(&error) {
                    Error::OutOfOrder { .. } => self.why_nothing_landed(&entry).await,
                    other => Err(other),
                },
            }
        }
    }

    async fn from(&self, sequence: u64) -> Result<Read, Error> {
        {
            let from = signed("sequence", sequence)?;
            let query = format!(
                "SELECT {COLUMNS} FROM journal_entries WHERE sequence >= $1 ORDER BY sequence"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*query))
                .bind(from)
                .fetch_all(&mut *self.reachable().await?)
                .await
                .map_err(|error| unavailable(&error))?;
            // **One row at a time, and a row that will not decode is named.**
            // `collect()` over a `Result` fails the whole range on the first
            // bad row, so a single one would destroy reads of every *other* row
            // — which would make one bad `INSERT` unrecoverable, since the
            // append-only rule means it cannot be removed. Reporting it as
            // `Unavailable` would compound that: the store had answered. **The
            // rows before the bad one are kept.** Stopping and saying where is
            // a verdict; returning nothing is a failure to answer.
            let mut entries = Vec::with_capacity(rows.len());
            let mut stopped_at = None;
            for row in &rows {
                match entry_from(row) {
                    Ok(entry) => entries.push(entry),
                    Err(why) => {
                        stopped_at = Some(where_it_stopped(row, &why));
                        break;
                    }
                }
            }
            Ok(Read {
                entries,
                stopped_at,
            })
        }
    }

    async fn unit_opened(&self, realm: &OpaqueId, unit: nc_ids::Snowflake) -> Result<bool, Error> {
        // The same four columns as `unit_closing`, one bracket along, asked as
        // an `EXISTS` and covered by the same index.
        let opened: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM journal_entries \
             WHERE realm = $1 AND work_unit = $2 AND action = $3 AND result = $4)",
        )
        .bind(realm.as_str())
        .bind(unit.to_crockford())
        .bind(crate::unit::OPENS)
        .bind(Outcome::UnitBegun.as_str())
        .fetch_one(&mut *self.reachable().await?)
        .await
        .map_err(|error| unavailable(&error))?;
        Ok(opened)
    }

    async fn unit_closing(
        &self,
        realm: &OpaqueId,
        unit: nc_ids::Snowflake,
    ) -> Result<Option<Outcome>, Error> {
        // **Four columns, none of them one that can fail to decode**, which is
        // the argument [`Store::head`] makes and the reason this is not a read
        // of the history. `LIMIT 1` stops at the first match and the query is
        // covered by `journal_entries_work_unit`, so it does not grow with the
        // table — the scan it replaces read and fully decoded every row in the
        // journal on every write inside a work unit. `result` is the one column
        // it hands back, and the `WHERE` has already held it to the two
        // spellings `Outcome::parse` reads.
        //
        // The realm is a predicate rather than a filter applied afterwards: a
        // bracket in another realm's history closes nothing here, and
        // `Journal::append` sets `record.realm` from the journal precisely so
        // one realm cannot reach another's.
        //
        // The outcome names are `Outcome::as_str`'s, not literals: a second
        // spelling of a variant here would drift from the one `append` writes on
        // the day a variant is renamed, and the answer would silently become
        // `None`. The same argument `entry_from` makes about `Outcome::parse`.
        // **And the action, which is what makes it a bracket.** Asking realm,
        // unit and result — and never `action` — lets an ordinary
        // `action = "Write"` entry carrying `Outcome::UnitCommitted` close
        // somebody else's open unit. Measured against a real database that way:
        //
        // ```text
        // seq 0  kernel  kernel.begin  unit-begun
        // seq 1  hostile Write         unit-committed   <- closes it
        // the owner's next entry -> Err(UnitClosed { … })
        // verify -> Ok(2)
        // ```
        //
        // The group's remaining entries are lost, no `kernel.unit` bracket
        // exists in the history at all, and a reader looking for the closing
        // bracket by action finds none while this says closed. `CLOSES` is
        // named so that "when did this unit end" has one answer, and the query
        // that decides it consults the name.
        //
        // **The first by sequence**, which is the bracket that closed the unit:
        // `append`'s `INSERT` refuses any entry for a closed unit, so a second
        // one is a row written around this store rather than through it.
        let closing: Option<String> = sqlx::query_scalar(
            "SELECT result FROM journal_entries \
             WHERE realm = $1 AND work_unit = $2 AND action = $3 AND result = ANY($4) \
             ORDER BY sequence LIMIT 1",
        )
        .bind(realm.as_str())
        .bind(unit.to_crockford())
        .bind(crate::unit::CLOSES)
        .bind(vec![
            Outcome::UnitCommitted.as_str().to_owned(),
            Outcome::UnitAborted.as_str().to_owned(),
        ])
        .fetch_optional(&mut *self.reachable().await?)
        .await
        .map_err(|error| unavailable(&error))?;
        Ok(closing.as_deref().and_then(Outcome::parse))
    }

    async fn open_units(&self, realm: &OpaqueId) -> Result<Vec<Snowflake>, Error> {
        // **One statement, over the columns that cannot fail to decode.** The
        // opening brackets of the realm, each held to the absence of a closing
        // one for its unit, by the same four columns `unit_opened` and
        // `unit_closing` ask. The rest of a row is never read, so a row whose
        // resource or chain this code cannot decode hides no unit opened after
        // it, which a read of the history from its first entry does.
        //
        // **Which entries it visits is the planner's**, by the cost it
        // estimates, for the opening brackets and their closes apart, and the
        // choice moves with what the table holds and with whether the prepared
        // statement is planned for the values asked or for any: through
        // `0001`'s index on the realm (`journal_entries_realm`), its index on
        // the unit (`journal_entries_work_unit`), both, or over the whole
        // table. So a recovery here may visit every entry of every realm: no
        // index here is the local journal's `(realm, work_unit)`.
        //
        // **Each unit once, at its first opening**, which is the order recovery
        // reverses: `append` refuses a second opening bracket, so a second one
        // is a row written around this store, and it neither repeats the unit
        // nor moves it.
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT MIN(opening.sequence), opening.work_unit FROM journal_entries AS opening \
             WHERE opening.realm = $1 AND opening.action = $2 AND opening.result = $3 \
             AND opening.work_unit IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM journal_entries AS closing \
             WHERE closing.realm = opening.realm AND closing.work_unit = opening.work_unit \
             AND closing.action = $4 AND closing.result = ANY($5)) \
             GROUP BY opening.work_unit ORDER BY MIN(opening.sequence)",
        )
        .bind(realm.as_str())
        .bind(crate::unit::OPENS)
        .bind(Outcome::UnitBegun.as_str())
        .bind(crate::unit::CLOSES)
        .bind(vec![
            Outcome::UnitCommitted.as_str().to_owned(),
            Outcome::UnitAborted.as_str().to_owned(),
        ])
        .fetch_all(&mut *self.reachable().await?)
        .await
        .map_err(|error| unavailable(&error))?;
        rows.into_iter()
            .map(|(sequence, unit)| {
                // **Read back as it was written**, for the reason `entry_from`
                // gives: the Crockford reader is lenient, and a respelled unit
                // is one `unit_closing` never finds by its canonical text, so
                // read leniently it would be recovered as open for ever.
                match Snowflake::from_crockford(&unit) {
                    Ok(read) if read.to_crockford() == unit => Ok(read),
                    _ => Err(Error::Undecodable {
                        sequence: u64::try_from(sequence).unwrap_or(u64::MAX),
                        why: format!(
                            "the opening bracket names the work unit `{unit}`, which is not \
                             an identifier as this code writes one, so no unit by that name \
                             can be recovered"
                        ),
                    }),
                }
            })
            .collect()
    }
}

/// Why a row could not be decoded, without the sentence that says it was not.
///
/// **`Undecodable`'s message must not contradict itself in one
/// sentence.** `entry_from` builds every decode failure
/// as `Error::Unavailable`, whose `Display` opens *"the journal store did not
/// answer:"* — and stringified whole into `Undecodable.why`, whose
/// own `Display` closes *"The store answered; what it holds is not what this
/// wrote"*, it would say both. The store answered. This takes the part after
/// the colon, which is the part that describes the row.
fn reason(error: &Error) -> String {
    let whole = error.to_string();
    whole
        .split_once(": ")
        .map_or(whole.clone(), |(_, rest)| rest.to_owned())
}

/// Which row would not decode, and why — and what to say when even that is unknown.
///
/// **`unwrap_or(u64::MAX)` would assert a row that cannot exist.** The column is
/// `BIGINT` under `CHECK (sequence >= 0)` and is the primary key, so
/// 18446744073709551615 is a number no row in this table has ever held — and the
/// sentence built from it reads *"entry 18446744073709551615 … is in the table"*.
/// Measured, after `ALTER TABLE journal_entries ALTER COLUMN sequence TYPE
/// numeric USING sequence::numeric`:
///
/// ```text
/// BROKEN seq=18446744073709551615 EntryIsNotReadable NC-JRNL-0002
///   | entry 18446744073709551615 does not verify: it is in the table and this
///     code cannot read it, so it was not written here
/// ```
///
/// [`sequence_of`] exists so that a row which cannot be decoded can still say
/// *which* row it is. When the `sequence` column is itself the thing
/// that will not decode, the honest answer is that the row cannot say — and the
/// reason says so, rather than a placeholder standing in for a row number.
fn where_it_stopped(row: &sqlx::postgres::PgRow, why: &Error) -> (u64, String) {
    match sequence_of(row) {
        Some(sequence) => (sequence, reason(why)),
        None => (
            u64::MAX,
            format!(
                "its `sequence` column is one this code cannot read, so the row cannot say \
                 which entry it is and the number here is not one: {}",
                reason(why)
            ),
        ),
    }
}

/// The `sequence` column of a row, read on its own.
///
/// Read separately so a row that cannot be decoded can still say *which* row it
/// is. The column is a `BIGINT` primary key: if this fails the row is not one
/// this schema produced at all.
fn sequence_of(row: &sqlx::postgres::PgRow) -> Option<u64> {
    use sqlx::Row as _;
    u64::try_from(row.try_get::<i64, _>("sequence").ok()?).ok()
}

/// A `u64` as PostgreSQL's `BIGINT`, or an error naming the field.
fn signed(field: &'static str, value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| {
        // **Not `Unavailable`.** The store was never asked: this is a number
        // the caller supplied that no PostgreSQL column can hold, and reporting
        // it as the store not answering sends a responder to PostgreSQL for a
        // defect in the request (§15.1) — and `JournalUnavailable` is
        // `retryable`, so it also tells them to try again forever. The same
        // sentence `Error::Unstorable` exists for, one field over.
        Error::Unstorable {
            field,
            why: format!(
                "it is {value}, which PostgreSQL's BIGINT cannot hold. Casting it would \
                 store a negative number that sorts before every real entry"
            ),
        }
    })
}

/// The other direction.
fn unsigned(field: &str, value: i64) -> Result<u64, Error> {
    u64::try_from(value).map_err(|_| {
        Error::Unavailable(format!(
            "`{field}` came back as {value}, and a negative one means a row was written by \
             something that cast rather than converted"
        ))
    })
}

/// `unavailable`, reachable from this crate's integration tests.
///
/// The mapping is the subject — a database error that is the *record's* fault
/// must not reach a responder as the store being down — and the states that
/// produce one are reached by SQL, not through any public method. A test that
/// cannot call the function under test asserts something else.
///
/// # Errors
///
/// Always: it converts an error into one.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[must_use]
pub fn unavailable_for_tests(error: &sqlx::Error) -> Error {
    unavailable(error)
}

/// A database error as the sentence a responder reads.
///
/// **sqlx 0.9 appends where in PostgreSQL's own C source the error was
/// raised**: `relation "journal_entries" does not exist at line 1498` is
/// `parse_relation.c`, and it reads as a line of the statement or of a
/// migration and is neither. Only the text changes
/// here — the class arms in [`unavailable`] read the error's code, never this
/// sentence — and `a_write_the_store_refuses_is_not_the_store_being_down` holds
/// the text a refusal carries against the suffix.
fn told(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(database) => {
            format!("error returned from database: {}", database.message())
        }
        other => other.to_string(),
    }
}

fn unavailable(error: &sqlx::Error) -> Error {
    // **A data exception is the request's fault, not the store's.** Mapping
    // every sqlx error to "the journal store did not answer", `retryable`,
    // would send a responder to PostgreSQL (`§15.1`) for a defect that will
    // answer identically for ever. Measured: `bigint out of range` from an
    // arithmetic overflow in the statement's own predicate, so mapped, comes
    // back as the store being down. Class 22 is SQL's data exception — out of
    // range, bad encoding, untranslatable character — and every one of them is
    // about the values in the statement. **Class 23 as well as class 22.** An
    // integrity-constraint violation is the row being refused, not the store
    // failing — and the one this table carries is reachable: a foreign INSERT
    // at the top of the permitted range ends the history, and without this arm
    // the next append would answer `NC-JRNL-0003`, *"the journal store did not
    // answer"*, **critical**, `retryable = true`, for a database answering
    // instantly and identically for ever — a worse sentence than the
    // `NC-JRNL-0005`, `retryable = false`, the same state carries without the
    // constraint. **The density trigger is contention on this path, not an
    // unstorable record.** `INSERT … SELECT … WHERE` produces no row when the
    // `WHERE` is false, so the trigger fires only where the sequence *was*
    // right when the statement ran and the tail moved before the row landed —
    // which is exactly what a race looks like. Reported as `Unstorable`, a
    // racing writer would be told to give up, on the one failure retrying
    // fixes. The two numbers are in the message because this migration writes
    // them there for this.
    if let sqlx::Error::Database(database) = error
        && let Some((_, rest)) = database
            .message()
            .split_once("is not the next one, which is ")
        && let Some(found) = database
            .message()
            .split_once("sequence ")
            .and_then(|(_, tail)| tail.split(' ').next())
            .and_then(|found| found.parse::<u64>().ok())
        && let Some(expected) = rest
            .trim_end_matches('.')
            .split(&[' ', '.'][..])
            .next()
            .and_then(|expected| expected.parse::<u64>().ok())
    {
        return Error::OutOfOrder { expected, found };
    }
    // **Class 40 is contention, not the store not answering.** `40001` is a
    // serialization failure and `40P01` a deadlock:
    // the store answered instantly and correctly, and two transactions asked for
    // something it could not order. Measured, twelve concurrent appends on a
    // pool whose sessions are `SERIALIZABLE`:
    //
    // ```text
    //  2  NC-JRNL-0003 / critical / the journal store did not answer:
    //     could not serialize access due to read/write dependencies among transactions
    // ```
    //
    // `NC-JRNL-0003` says *"The journal store did not answer"*, and it did. This
    // is the self-contradicting sentence this crate removes at three other seams,
    // and `nc-prune` already classifies `40001` as waited-rather-than-refused
    // with a measurement of its own. The isolation level is the caller's — this
    // repository sets none — but the mapping is wrong whoever sets it.
    if let sqlx::Error::Database(database) = error
        && database.code().is_some_and(|code| code.starts_with("40"))
    {
        return Error::Contended;
    }
    if let sqlx::Error::Database(database) = error
        && database
            .code()
            .is_some_and(|code| code.starts_with("22") || code.starts_with("23"))
    {
        return Error::Unstorable {
            field: "a value in the statement",
            why: format!(
                "{}. The store answered; it is the request it could not take, so \
                 this is not the store being unavailable",
                told(error)
            ),
        };
    }
    // **And the store answering "no" is not the store being silent.** Without
    // the arms above, everything falls to `Unavailable` — `NC-JRNL-0003`, *"the
    // journal store did not answer"*, **critical**, `retryable = true`.
    // Measured that way, three different systems are reported identically:
    //
    // ```text
    // INSERT revoked (42501)   permission denied for table journal_entries
    // table dropped (42P01)    relation "journal_entries" does not exist
    // the store stopped        pool timed out while waiting for a connection
    //
    // all three -> NC-JRNL-0003 / critical / retryable
    //           -> "The activity history is not answering right now. Try again in a moment."
    // ```
    //
    // `§14.9` makes the journal fail closed, so the platform stops on any of
    // them — and the instruction a person gets is *try again*, for ever. The
    // classes here are the ones where PostgreSQL has answered and refused on
    // grounds no retry changes: `42` access rule violation (a missing `GRANT`,
    // a dropped table, a syntax error), `28` invalid authorization, `3D` an
    // unknown catalog, `3F` an unknown schema. `nc-prune` draws this line too;
    // this is the journal's.
    if let sqlx::Error::Database(database) = error
        && database.code().is_some_and(|code| {
            code.starts_with("42")
                || code.starts_with("28")
                || code.starts_with("3D")
                || code.starts_with("3F")
        })
    {
        return Error::Refused(format!(
            "{}. The store answered and refused: a grant, a schema, a role or a \
             statement it will refuse identically next time",
            told(error)
        ));
    }
    Error::Unavailable(told(error))
}

/// The delegation chain, read back exactly as it was written.
///
/// Split out because inside `entry_from` it would cross a hundred lines. The
/// reasoning is in the body, and it is the same one `resource` and `work_unit`
/// carry: the chain hashes a value, the table stores its text, and a reader
/// that accepts more spellings than the writer emits is a row that can be
/// respelled without moving the digest.
fn actor_chain_from(row: &sqlx::postgres::PgRow) -> Result<Vec<OpaqueId>, Error> {
    // **Read back as it was written, for the reason `resource` and `work_unit`
    // are — and this is the column that says who is answerable.**
    // `Entry::hashed_form` hashes the identifiers; `append` stores
    // `serde_json`'s array of them; and `serde_json` accepts more than it
    // emits — `\u0061` for `a`, and any whitespace. PostgreSQL's `jsonb`
    // normalises that away, which is all that holds it without this, and one
    // statement takes the normalisation with it. Measured on PostgreSQL 18.6, through the
    // `ALTER TABLE ... ALTER COLUMN ... TYPE ... USING` family that
    // `0005_the_journal_refuses_truncate.sql` lists as still reaching the rows,
    // firing no trigger and leaving `tgenabled = 'A'`:
    //
    // ```text
    // actor_chain text before: ["agent-1", "principal-1"]
    // actor_chain text after:  ["\u0061gent-1", "principal-1"]
    // rows matching '%agent-1%' before/after: 4/0
    // verify before: Ok(4)   verify after: Ok(4)
    // ```
    //
    // `actor_chain` is `§14.6`'s delegation chain — the log shows both the
    // agent and the operator — so without this every reader looking for that
    // agent finds nothing and criterion 3 calls the history clean. The text
    // this code would have written is the only text this accepts.
    let stored_actors: String = row
        .try_get("actor_chain_text")
        .map_err(|error| unavailable(&error))?;
    let actors: serde_json::Value = row
        .try_get("actor_chain")
        .map_err(|error| unavailable(&error))?;
    let actor_chain = actors
        .as_array()
        .ok_or_else(|| Error::Unavailable("`actor_chain` is not an array".to_owned()))?
        .iter()
        .map(|actor| {
            actor
                .as_str()
                .ok_or_else(|| Error::Unavailable("an actor is not a string".to_owned()))
                .and_then(|actor| OpaqueId::new(actor).map_err(|error| unavailable_id(&error)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    {
        // **PostgreSQL's rendering, not `serde_json`'s.** The column is `jsonb`,
        // which is a *normalised* representation: the text that comes back is
        // whatever `jsonb` prints, and `jsonb` separates elements with `", "`
        // where `serde_json::to_string` uses `","`. Measured on 18.6:
        //
        // ```text
        // '["a","b"]'::jsonb::text  ->  ["a", "b"]
        // '[]'::jsonb::text         ->  []
        // ```
        //
        // That normalisation is exactly what makes this column safe without the
        // guard, and changing the column's type to `json` is what takes it away — so
        // the guard has to know the form it is defending. An identifier's
        // character set holds no `"` or `\`, which `OpaqueId::new` has already
        // enforced on every element above, so nothing here needs escaping.
        //
        // The coupling to PostgreSQL's output is deliberate and belongs here:
        // this module is the mapping between an `Entry` and a row, and a guard
        // that did not know how the row is written could not tell a rewritten
        // one from an original.
        let canonical = if actor_chain.is_empty() {
            "[]".to_owned()
        } else {
            let inside: Vec<String> = actor_chain
                .iter()
                .map(|actor| format!("\"{}\"", actor.as_str()))
                .collect();
            format!("[{}]", inside.join(", "))
        };
        if canonical != stored_actors {
            return Err(Error::Unavailable(format!(
                "`actor_chain` is stored as `{stored_actors}` and this chain is written \
                 `{canonical}`. The chain hashes the identifiers, so a row respelled after \
                 it was written would verify and be a different row to every reader that \
                 looks for the text"
            )));
        }
    }

    Ok(actor_chain)
}

/// One row back into an entry.
///
/// **Nothing is recomputed.** The stored `hash` is read as it is, so that
/// verification compares what the writer committed to against what the contents
/// produce — which is the whole test. Recomputing here would make every row
/// verify by construction.
fn entry_from(row: &sqlx::postgres::PgRow) -> Result<Entry, Error> {
    let text = |column: &str| -> Result<String, Error> {
        row.try_get::<String, _>(column)
            .map_err(|error| unavailable(&error))
    };
    let maybe = |column: &str| -> Result<Option<String>, Error> {
        row.try_get::<Option<String>, _>(column)
            .map_err(|error| unavailable(&error))
    };

    let actor_chain = actor_chain_from(row)?;

    let record = Record {
        realm: OpaqueId::new(&text("realm")?).map_err(|error| unavailable_id(&error))?,
        principal: OpaqueId::new(&text("principal")?).map_err(|error| unavailable_id(&error))?,
        actor_chain,
        action: text("action")?,
        // **Read back as it was written, for the reason `work_unit` is, and
        // this one is the field a command acted on.** `Entry::hashed_form`
        // hashes the parsed `Value`; `append` stores `to_canonical()`; and
        // `nc_canon::parse` — by its own module header — "accepts more than it
        // emits": insignificant whitespace, `\u` escapes, leading zeros. So the
        // *text* of this column could be rewritten to a different spelling of
        // the same value and the digest would not move.
        //
        // Measured on PostgreSQL 18.6 against migrations 0001-0004, with both
        // append-only triggers `ENABLE ALWAYS` and refusing the ordinary
        // `UPDATE`, using `ALTER TABLE ... ALTER COLUMN ... TYPE text USING`,
        // which rewrites the heap and fires no row trigger:
        //
        // ```text
        // row 1 resource before: {"id":1,"kind":"content"}
        // row 1 resource after:  { "id" : 1 ,  "kind" : "\u0063ontent" }
        // triggers after: [("...no_update","A"), ("...no_delete","A")]
        // verify: Ok(4)
        // rows still matching the canonical text: 0
        // ```
        //
        // Without this, what a command acted on is changed, verification calls
        // the history clean, and every reader searching for what was written
        // finds nothing. This is that check: the stored bytes must be the ones
        // this code would have written, not merely bytes that parse to the same
        // value.
        resource: {
            let stored = text("resource")?;
            let parsed =
                nc_canon::parse(&stored).map_err(|error| Error::Unavailable(error.to_string()))?;
            let canonical = parsed.to_canonical();
            if canonical == stored {
                parsed
            } else {
                return Err(Error::Unavailable(format!(
                    "`resource` is stored as `{stored}` and this value is written \
                     `{canonical}`. The chain hashes the value, so a row respelled after \
                     it was written would verify and be a different row to every reader \
                     that looks for the text"
                )));
            }
        },
        // `Outcome::as_str` and `Outcome::parse` already exist and are what the
        // hashed form uses. A second mapping here would be a second answer to
        // "how is an outcome written", and the two would drift on the day a
        // variant is added.
        result: {
            let stored = text("result")?;
            Outcome::parse(&stored).ok_or_else(|| {
                Error::Unavailable(format!(
                    "`{stored}` is not an outcome this code knows. A row holding one was \
                     written by a newer version, and reading it as anything else would \
                     misreport what happened"
                ))
            })?
        },
        deciding_policy: maybe("deciding_policy")?,
        policy_pack: maybe("policy_pack")?,
        policy_epoch: row
            .try_get::<Option<i64>, _>("policy_epoch")
            .map_err(|error| unavailable(&error))?
            .map(|epoch| unsigned("policy_epoch", epoch))
            .transpose()?,
        grant_used: maybe("grant_used")?
            .map(|grant| OpaqueId::new(&grant).map_err(|error| unavailable_id(&error)))
            .transpose()?,
        // **Read back as it was written, or the chain does not cover it.**
        // `Entry::hashed_form` hashes `unit.to_crockford()` — the *canonical*
        // re-encoding — while `nc_ids`' Crockford reader is deliberately
        // lenient: case-insensitive, `O` for `0`, `I` and `L` for `1`. So
        // decoded with it alone, the stored text could be respelled and the
        // hash would not move.
        //
        // Measured that way: rewriting `2CGTGWE00W00` to `2cgtgweOOwOO` in the
        // column leaves `verify` answering `Ok(3)` while every SQL reader —
        // `WHERE work_unit = '<canonical>'`, and the
        // `journal_entries_work_unit` index — stops finding the row. So an
        // entry would silently leave its work unit and criterion 3 report the
        // history clean.
        //
        // `OpaqueId` is verbatim and `from_hex` is lowercase-only, so this is
        // the one lenient decoder in the hashed path. The leniency is right for
        // `nc-ids`' own purpose — a person typing an identifier — and wrong
        // here, where the text *is* the thing the hash committed to.
        work_unit: maybe("work_unit")?
            .map(|unit| {
                let read = Snowflake::from_crockford(&unit)
                    .map_err(|error| Error::Unavailable(error.to_string()))?;
                if read.to_crockford() == unit {
                    Ok(read)
                } else {
                    Err(Error::Unavailable(format!(
                        "the work unit is stored as `{unit}` and this identifier is written \
                         `{}`. The chain hashes the canonical spelling, so a row respelled \
                         after it was written would verify and be a different row to every \
                         reader that looks it up",
                        read.to_crockford()
                    )))
                }
            })
            .transpose()?,
        recorded_at: unsigned(
            "recorded_at",
            row.try_get::<i64, _>("recorded_at")
                .map_err(|error| unavailable(&error))?,
        )?,
    };

    Ok(Entry {
        sequence: unsigned(
            "sequence",
            row.try_get::<i64, _>("sequence")
                .map_err(|error| unavailable(&error))?,
        )?,
        record,
        previous: maybe("prev_hash")?
            .map(|hex| digest("prev_hash", &hex))
            .transpose()?,
        hash: digest("hash", &text("hash")?)?,
    })
}

/// `digest`, reachable from this crate's integration tests.
///
/// **`migrations/0009_a_hash_is_sixty_four_hex_characters.sql` constrains the column, so the row this guards against
/// cannot be written.** That is the stronger guarantee and it is what the test
/// asserts — but a column constraint is not a reason to stop being able to
/// read a row that got past one, so the guard stays under test at its own seam.
///
/// # Errors
///
/// Returns why the text is not a digest.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn digest_for_tests(column: &str, hex: &str) -> Result<[u8; 32], String> {
    digest(column, hex).map_err(|error| error.to_string())
}

/// A stored hex column back into a digest.
///
/// **The width is checked, not assumed.** `from_hex` returns a `Vec`, and a row
/// holding a truncated or over-long hash would otherwise panic on the array
/// conversion — inside verification, which is the code that runs when somebody
/// already suspects the table has been tampered with. A wrong width is exactly
/// what tampering looks like, so it is reported rather than raised.
///
/// # Errors
///
/// Returns [`Error::Unavailable`] naming the column — for text that is not hex,
/// and for a digest of the wrong width.
///
/// **That variant is worth a second look and is not this function's to change.**
/// The message it carries says *"a row with a hash of the wrong width was not
/// written by this code"*, which is [`Error::Undecodable`]'s sentence, and
/// `Unavailable` is the retryable one. `entry_from` builds `Unavailable` for
/// every "the row is not what we wrote" condition, not only this one, so the
/// taxonomy question is about that whole function rather than about `digest` —
/// and one caller already maps it: `head` wraps it through `poisoned`, which
/// makes it `Undecodable`. The two paths disagree about the same row.
fn digest(column: &str, hex: &str) -> Result<[u8; 32], Error> {
    let bytes = nc_canon::from_hex(hex)
        .map_err(|error| Error::Unavailable(format!("`{column}` is not hex: {error}")))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        Error::Unavailable(format!(
            "`{column}` holds {} byte(s) and a digest is 32. A row with a hash of the \
             wrong width was not written by this code",
            bytes.len()
        ))
    })
}

fn unavailable_id(error: &impl core::fmt::Display) -> Error {
    Error::Unavailable(format!("a stored identifier is not one: {error}"))
}

#[cfg(test)]
mod every_arm_of_the_insert_is_diagnosed {
    /// All three ways the `INSERT` can fail reach the diagnosis.
    ///
    /// **Two of the three are reachable only under a race, and both could be
    /// reverted silently.** The statement can come back three ways: zero rows
    /// (its `WHERE` refused), a unique violation (a racing writer took the
    /// sequence between the `WHERE` and the index), and a database error
    /// that `unavailable` translates to `OutOfOrder` (`0012`'s density trigger
    /// fired because the tail moved before the row landed). Each must
    /// reach `why_nothing_landed`, because only the diagnosis asks about the
    /// work unit — and a unit that is open stays open, while a sequence moves.
    ///
    /// Measured without this, each replaced with the weaker form, with the
    /// whole workspace suite green and thirty stress runs of the two race tests
    /// clean: the unique-violation arm as `Err(Error::Unavailable(...))`, which
    /// tells a responder to go and look at PostgreSQL for what is a racing
    /// writer; and the `unavailable` arm as a plain translation, which hands an
    /// opener of an already-open unit `JournalContended` — `retryable = true`,
    /// severity *warning* — for a condition no retry clears.
    ///
    /// A source assertion, because the two arms are reachable only by winning a
    /// race: a test that catches a defect one run in ten is one people learn to
    /// re-run. What is asserted is the property the race tests cannot pin —
    /// that every arm routes to the one place that knows the permanent answers.
    #[test]
    fn all_three_failure_arms_reach_why_nothing_landed() {
        let source = include_str!("postgres.rs");
        let body = source
            .split_once("match result {")
            .expect("the three arms are here")
            .1;
        // The match's own close, at its own indentation — `split_once("    }")`
        // would cut at the first nested brace and truncate the third arm away,
        // so the test would report it missing when it is there. A window that
        // stops early is the same defect as a walk that stops early.
        let arms = body
            .split_once("\n            }")
            .expect("the match closes")
            .0;

        for (arm, what) in [
            ("rows_affected() == 0", "the `WHERE` refused the row"),
            ("is_unique_violation()", "a racing writer took the sequence"),
            (
                "Error::OutOfOrder { .. } =>",
                "the density trigger fired because the tail moved",
            ),
        ] {
            let at = arms
                .find(arm)
                .unwrap_or_else(|| panic!("the arm for {what} is gone: `{arm}`"));
            let diagnosed = arms[at..]
                .find("why_nothing_landed")
                .unwrap_or_else(|| panic!("{what}, and the arm does not diagnose it"));
            // The next arm begins at the next `=>` past this one's body, so a
            // `why_nothing_landed` belonging to a *later* arm must not count.
            let next = arms[at..]
                .find("\n                Err(")
                .unwrap_or(usize::MAX);
            assert!(
                diagnosed < next,
                "{what}, and the arm answers without asking the diagnosis — which is \
                 the only place that knows whether the unit is open"
            );
        }
    }
}
