//! The pruner against the store that runs.
//!
//! The arithmetic in the parent module is provable without a database. This is
//! the one `DELETE` that is not: with only an in-memory target, `§15.8`'s "one
//! pruner enforces all of them" would be true of a trait and of nothing that
//! touches rows.
//!
//! # The identifier is not a parameter, and that is the danger
//!
//! A table name and an `ORDER BY` column cannot be bound — PostgreSQL takes
//! parameters where values go, not where identifiers do — so both are written
//! into the statement text. They come from `nc_catalog::TABLES`, which is
//! generated from `catalog/limits.toml` by a compiler that runs at build time,
//! so nothing a request carries reaches here. That is an argument about the
//! *current* callers, and this file is where a future one that gets it wrong
//! would land, so `identifier` checks the shape anyway and refuses rather than
//! interpolating. A guard that is unnecessary today costs one comparison; the
//! absence of one costs the database.
//!
//! # Why the primary key, and not a physical address
//!
//! `DELETE … ORDER BY … LIMIT` is not SQL PostgreSQL accepts. The oldest rows
//! are chosen by a subselect and deleted by key, inside one statement.
//!
//! **An address is wrong in both of its forms.** `ctid`
//! alone is an address only within one physical table: a partitioned or
//! inherited table is several, and the same `(page, tuple)` pair exists in
//! each, so `WHERE ctid IN (…)` matches every row in every partition that
//! shares an address with a chosen one. Measured on PostgreSQL 18.6, seven old
//! rows in one partition and seven new in another, asking for four:
//!
//! ```text
//! DELETE 8
//!  id | 5 6 7 105 106 107
//! ```
//!
//! Four asked for, **eight deleted**, and rows 101–104 — the *newest* in the
//! table — among them.
//!
//! `(tableoid, ctid)` fixes that and is still an address, and **an address
//! moves**. An `UPDATE` committed against a chosen row between the subselect's
//! snapshot and the delete's row lock re-points it, PostgreSQL re-checks the
//! new tuple, finds its `ctid` is not in the list, and skips it. Measured, an
//! ordinary single-row `UPDATE` loop beside a batch of 5000:
//!
//! ```text
//! removed 4991 of 5000, rows left 15009 where 15000 was expected
//! ```
//!
//! and worse, the skipped row keeps its place at the head of every future batch
//! — `0006`'s trigger preserves its ordering column across an `UPDATE` — so
//! `§15.8`'s only enforced bound stops converging while every run reports `Ok`.
//!
//! **A primary key does not move.** The re-check compares the new tuple's key
//! against the same materialised list and the row is deleted; measured against
//! the identical race, `DELETE 4` of 4 with the oldest row gone. The reach
//! guard refuses a partitioned or inherited relation outright, so the first
//! failure above cannot arise either.
//!
//! The full measurement of both races is at the statement itself; this page
//! says which design ships, so that the two cannot disagree about it.

use sqlx::{PgPool, Row as _};

use crate::Target;

/// The tables in one PostgreSQL database.
#[derive(Debug, Clone)]
pub struct Postgres {
    pool: PgPool,
    /// The schema this pruner's tables are in, written down rather than resolved.
    /// See [`Postgres::in_schema`].
    schema: String,
}

impl Postgres {
    /// A target over an existing pool.
    ///
    /// Takes the pool rather than a URL, for the reason `nc_journal::postgres`
    /// gives: one process holds one pool, and a type that opened its own would
    /// make the connection limit a function of how many were constructed.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::in_schema(pool, "public")
    }

    /// The same, for a deployment whose tables are not in `public`.
    ///
    /// **The schema is a value, because every way of asking resolves
    /// through `search_path`.** Refusing any path with more than one schema on
    /// it fails on the server default, `"$user", public`: one `CREATE SCHEMA`
    /// named after the connecting role makes every bounded table permanently
    /// unprunable behind `NC-DB-0005`, which says retrying will not
    /// help. Measured through `prune_these` on a production-shaped pool: `Ok`
    /// before the schema existed, `Err` after, with `§15.8`'s only enforced
    /// bound off for every table and `prune_these_with` returning on the first
    /// one.
    ///
    /// `nc-journal`'s store carries the same value for the same reason, and
    /// `xtask` reads it out of the connection string. Nothing guesses.
    #[must_use]
    pub fn in_schema(pool: PgPool, schema: &str) -> Self {
        Self {
            pool,
            schema: schema.to_owned(),
        }
    }

    /// The pool it prunes through.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// What PostgreSQL answered, as a number of rows.
///
/// PostgreSQL has no unsigned integers, and `as` on a negative would wrap to
/// something enormous — so this is the conversion saying so rather than
/// assuming it.
///
/// **Its own function because `count(*)` cannot be negative.** The branch is
/// unreachable through the query above, which is exactly why it needs a seam:
/// without one, `n as u64` in its place leaves the whole suite green, and a
/// gate that cannot be driven is a gate nobody knows is wired up.
fn as_a_count(n: i64, table: &str) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| format!("`{table}` counted {n} rows, which is not a count"))
}

/// A batch size, as the parameter PostgreSQL takes.
///
/// `LIMIT` takes a signed integer, and `batch as i64` on a number past
/// `i64::MAX` wraps to a negative — which PostgreSQL refuses, but only after
/// the statement has been built and sent. Same reason as above: written as a
/// conversion that can fail, so the failure is this crate's sentence rather than
/// the driver's.
fn as_a_limit(batch: u64, table: &str) -> Result<i64, String> {
    i64::try_from(batch).map_err(|_| {
        format!("`{table}`: a batch of {batch} is more rows than PostgreSQL can be asked for")
    })
}

/// An identifier that is safe to write into a statement, or why it is not.
///
/// **Refuses rather than quotes.** Quoting would accept anything and make the
/// name whatever was passed, so a caller with a mistake would get a table nobody
/// declared instead of an error. Every name the catalog generates is a plain
/// lowercase identifier; anything else means something other than the catalog
/// wrote it, and that is the case worth stopping.
fn identifier(name: &str) -> Result<String, String> {
    // **A schema-qualified name is two identifiers, not one.** `limits.rs`
    // deliberately reports `archive.access_records` for a table created in a
    // schema, and `catalog/limits.toml` can declare exactly that as
    // `[tables."archive.access_records"]` — so checked as one identifier, a
    // name written where the gate asks for it would reach here and be refused,
    // with a message blaming whoever wrote it. The two halves of the platform
    // would disagree about what a table name is; the catalog is right, and this
    // checks each part.
    let parts: Vec<&str> = name.split('.').collect();
    // **Said once, not twice.** `!name.is_empty()` and `name.len() <= 127` here
    // would both be unfalsifiable: `"".split('.')` yields `[""]`, so the empty
    // name is refused by `!part.is_empty()` below, and 63 + 1 + 63 is 127, so
    // the whole-name bound is implied by the two per-part bounds. Deleting
    // either leaves this crate green, which is what `mutants.py`'s own header
    // says the answer to "the code says this twice" is.
    let shaped = parts.len() <= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && part.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if shaped {
        // **Quoted, once the shape is known.** Every part passes
        // `[a-z_][a-z0-9_]*`, which admits every SQL reserved word — and a
        // table called `user` is an unremarkable name in a platform whose own
        // Cedar schema declares `entity User`. Unquoted, PostgreSQL 18.6 reads
        // `SELECT count(*) FROM user` as the `USER` function and answers **1**,
        // with no error: the table would report one row for ever, never cross
        // `max_rows`, and never be pruned, with every gate green.
        // `session_user` and `current_date` do the same; `order`, `select` and
        // `grant` produce a raw syntax error instead of this function's own
        // diagnostic.
        //
        // Quoting is safe precisely because the shape was checked first: every
        // part is already lowercase, so `"access_records"` and
        // `access_records` name the same table.
        Ok(name
            .split('.')
            .map(|part| format!("\"{part}\""))
            .collect::<Vec<_>>()
            .join("."))
    } else {
        Err(format!(
            "`{name}` is not a plain identifier or a schema-qualified pair of them, and \
             this writes it into the statement rather than binding it. Every name here \
             comes from catalog/limits.toml through the compiler, so one that does not \
             look like it did was not written there"
        ))
    }
}

/// How long a pruning statement may wait for a lock before giving up.
///
/// **A pruner that never returns is a pruner that stopped.** This is the one
/// bound on that wait: neither the journal's store nor the pool options set a
/// `lock_timeout`. Measured without it — another session holding
/// `LOCK TABLE … IN ACCESS EXCLUSIVE MODE`, which is what a migration or a
/// `VACUUM FULL` does:
///
/// ```text
/// count under ACCESS EXCLUSIVE after 5s: Err(Elapsed(()))
/// ```
///
/// It never returns. On a schedule that is a tick that never completes and
/// never reports, holding a pool connection for the life of the lock, while
/// `§15.8`'s "one pruner enforces all of them, on a schedule" quietly stops.
/// `lib.rs` argues at length about not *holding* a lock for as long as a
/// backlog takes; this is the same argument about waiting for one.
///
/// Five seconds because a pruning run has nowhere to be: the next tick will try
/// again, and a table that is locked for longer than this is a table somebody
/// is working on.
///
/// **`SET LOCAL`, and this is what decides the word.** Replaced
/// by `SET`, `cargo run --quiet -p xtask -- check` answers `check passed`
/// without this, console byte-identical to a clean run — and the connection
/// then carries `5s` back to the pool, because nothing in this workspace
/// installs an `after_release` hook. The next borrower of it has every lock
/// wait cancelled at five seconds with `55P03`, which `nc-journal` maps to a
/// *retryable* `Unavailable`: an audit append reported as a transient outage,
/// and a migration taking `ACCESS EXCLUSIVE` simply aborting.
/// `a_locked_table_ends_the_run_rather_than_holding_it` proves the bound fires;
/// `what_this_crate_sets_does_not_outlive_its_transaction` is the half that
/// proves it ends.
const LOCK_TIMEOUT: &str = "SET LOCAL lock_timeout = '5s'";

/// Whether one SQLSTATE is a wait rather than a refusal.
///
/// **A door for a test, because the list is private to this file.** The
/// integration suite asks for every code the list holds through this function,
/// so deleting `40001` — the entry whose own paragraph says its absence ends a
/// prune run and leaves every later table unlooked-at — fails there as well as
/// in this file's own equality test. A list nothing reads back shrinks quietly.
#[doc(hidden)]
#[must_use]
pub fn waited_for_tests(code: &str) -> bool {
    WAITED_RATHER_THAN_REFUSED.contains(&code)
}

/// The SQLSTATEs that mean *the work was not done, and nothing says it cannot be*.
///
/// **A bounded wait is a skip, not a failed run.** As an error, the timeout
/// above makes "one table is locked" a failed run, and in a loop that returns
/// on the first error, "no table after it was looked at" — measured that way,
/// two tables both over their limit with an `ACCESS EXCLUSIVE` lock on the
/// first:
///
/// ```text
/// after    5.0017s
/// outcome  Err("`a_locked` could not be counted: … lock timeout")
/// b_free   still holds 10 rows, over its limit of 4, never looked at
/// ```
///
/// This file's own case list makes that argument about a *different* door —
/// refusing a schema-qualified name would fail every run on a name written
/// exactly where the gate asks for it — and the same shape holds for this one.
///
/// **All three, not `lock_timeout` alone.** `LOCK_TIMEOUT` above is reached only
/// if nothing else cancels first, and two ordinary settings cancel sooner: a
/// `statement_timeout` below five seconds, from `postgresql.conf` or
/// `ALTER ROLE … SET`, and a deadlock, whose `deadlock_timeout` defaults to one
/// second. Measured with `statement_timeout = 1s` and the same
/// `ACCESS EXCLUSIVE` lock:
///
/// ```text
/// after 1.000887067s: Err("… canceling statement due to statement timeout")
/// ```
///
/// `57014`, not `55P03` — so with `lock_timeout` alone the run ends, and every
/// table after it goes unlooked-at. All three are the same fact about the same
/// run.
const WAITED_RATHER_THAN_REFUSED: [&str; 8] = [
    // **The operator-intervention class, `57`.** A database restart delivers
    // `57P01` — *terminating connection due to administrator command* — on the
    // connection that is already open, so it arrives as a
    // `sqlx::Error::Database` and takes `answered = true`, and a code not in
    // this list then reads as a permanent refusal: `NC-DB-0005`, *trying again
    // will not help*. Measured:
    //
    // ```text
    // error       = terminating connection due to administrator command
    // sqlstate    = Some("57P01")   sqlx::Error::Database = true
    // waited      = false
    // => DbPruneRefused   retryable = false
    // ```
    //
    // `Failure::answered` exists because without it "a scheduler keyed on
    // `retryable` gives up for good **on a database restart**" — and this is that outcome,
    // reached by the other half of the same journey. The connect-time path is
    // right (`PoolTimedOut` is not a `Database` error), so the gap would be
    // only for a restart that lands mid-statement, which is the ordinary way a
    // restart is seen.
    "57P01", // `admin_shutdown` — the server is going down, and will come back.
    "57P02", // `crash_shutdown`.
    "57P03", // `cannot_connect_now` — starting up, and not ready yet.
    "53300", // `too_many_connections`, when the server reports it.
    "55P03", // `lock_not_available`, which is what `lock_timeout` raises.
    "57014", // `query_canceled` — `statement_timeout`, and a cancel from outside.
    "40P01", // `deadlock_detected`.
    // **`serialization_failure`.** Under `REPEATABLE READ` or `SERIALIZABLE` it
    // is *the* retry-me error, and as a refusal it ends the run — so every
    // table after the one that hit it goes unlooked-at, which is exactly the
    // harm `Untouched::SomethingElseHeldALock` exists to prevent. Measured
    // against the shipped target without it:
    // `Err("… could not serialize access due to concurrent update")`.
    "40001",
];

/// Whether a `DELETE` took more rows than the batch asked for.
///
/// **Named so it can be driven, because the call site cannot be.** The reach
/// guard refuses every partitioned and inherited relation before the statement
/// runs, so the condition this catches is unreachable by construction today —
/// which is exactly why it is the last line of defence and exactly why nothing
/// else exercises it: without this, deleting the whole block
/// leaves `cargo test -p nc-prune` green. Its own paragraph says
/// the `SELECT`-versus-`DELETE` window "is named here rather than closed" and
/// that this is what stands behind it.
///
/// Equal is not over: a batch of four taking four is the ordinary run.
pub(crate) fn over_deleted(removed: u64, batch: u64) -> Option<String> {
    (removed > batch).then(|| {
        format!(
            "{removed} rows were removed for a batch of {batch}. The statement selected \
             more than it was asked for. This transaction is rolled back, so the rows \
             are still there"
        )
    })
}

/// One database error with a sentence around it, and its own SQLSTATE kept.
///
/// The wrapping sentences — "could not begin", "could not commit the pruning" —
/// are what a reader needs; the code is what the caller needs, and building the
/// sentence keeps both.
fn said_while(doing: &str, error: &sqlx::Error) -> crate::Failure {
    let carried = said(error);
    // **The sentence is the only thing this changes.** Re-stating the two flags
    // field by field leaves `waited: carried.waited` decided by nothing:
    // measured, replaced by `waited: false`, `cargo test -p nc-prune` green and
    // `cargo xtask check` at `check passed`. Five call sites route through here
    // and exactly one is driven — with a *transport* failure, which carries no
    // SQLSTATE and is `waited: false` either way.
    //
    // What that costs is stated in this function's own reason for existing:
    // `40001` is raised at COMMIT and reaches the `could not commit the pruning`
    // site, so a serialization failure would become `retryable = false` and a
    // skip the caller should retry would be recorded against the table as
    // *"trying again will not help"*.
    //
    // Written as the rest of `said`'s answer rather than as two copies of it,
    // so there is no clause left to weaken — the shape a source assertion in
    // this file's test module holds.
    crate::Failure {
        why: format!("{doing}: {}", carried.why),
        ..carried
    }
}

/// One database error, carrying whether it is worth another tick.
///
/// The SQLSTATE decides, not the sentence: the sentence is composed by the
/// database and carries text the application wrote — see [`crate::Failure`].
fn said(error: &sqlx::Error) -> crate::Failure {
    let waited = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| WAITED_RATHER_THAN_REFUSED.contains(&code.as_ref()));
    // **And a database that is not there answered nothing at all.** `waited` is
    // read from a SQLSTATE, and a pool timeout, a refused connection, a DNS
    // failure and a TLS failure carry none — so read from a SQLSTATE alone,
    // every transport failure comes back `waited: false`, which
    // `Failure::code()` reads as a permanent refusal. Measured against a closed
    // port:
    //
    // ```text
    // why  = pool timed out while waiting for an open connection
    // code = NC-DB-0005 … retryable = false
    // en   = "Old records could not be cleared, and trying again will not help.
    //         Contact whoever set up your Noctua."
    // ```
    //
    // for a database that will be back in a minute, while the table grows. A
    // database error has a `Database` payload; everything else here is the
    // connection, and `NC-DB-0001` — *"The database did not answer"* — is the
    // code for it.
    let answered = matches!(error, sqlx::Error::Database(_));
    // **The database's own sentence, not where in its source it was raised.**
    // sqlx 0.9 appends `at line N` to a PostgreSQL error's `Display`, and `N`
    // is a line of the server's C source — measured,
    // `relation "access_records" does not exist at line 636` — which reads as a
    // line of the statement this crate wrote and is not. The SQLSTATE above is
    // what decides, and
    // `the_delete_door_orders_a_catalog_table_the_catalog_way` holds the
    // sentence against the suffix.
    let why = match error {
        sqlx::Error::Database(database) => {
            format!("error returned from database: {}", database.message())
        }
        other => other.to_string(),
    };
    crate::Failure {
        why,
        waited,
        answered,
    }
}

impl Postgres {
    /// A transaction with the lock bound set, or why one could not start.
    ///
    /// `SET LOCAL` rather than `SET`, so the bound belongs to this statement
    /// and does not outlive it on a pooled connection somebody else will get.
    ///
    /// **A `Failure`, not a `String`.** `From<String> for Failure`
    /// writes `waited: false` — so as a `String`, nothing that happened while
    /// opening the transaction or setting `lock_timeout` could ever be a wait,
    /// including the two errors most likely to arrive there. Measured, both
    /// through the real server:
    ///
    /// ```text
    /// 57014  "could not bound the wait for a lock: … canceling statement due to
    ///         statement timeout"     -> the SQLSTATE thrown away, run ended
    /// pool   "could not begin: pool timed out while waiting for an open connection"
    /// ```
    ///
    /// `57014` is in `WAITED_RATHER_THAN_REFUSED` and has to be consulted
    /// here. Matching `"timed out"` in the message instead is prose again, in a
    /// crate that does not match prose, and reads a host that does not exist
    /// (`pool timed out`, nothing holding anything) as contention for
    /// ever. The SQLSTATE is in hand at both call sites below.
    async fn bounded(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, crate::Failure> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| said_while("could not begin", &error))?;
        sqlx::query(LOCK_TIMEOUT)
            .execute(&mut *transaction)
            .await
            .map_err(|error| said_while("could not bound the wait for a lock", &error))?;
        // **And which `access_records` this is.** Every name here is
        // unqualified, and `only_for`, `count` and the `DELETE` each resolve it
        // again at their own statement time — so unpinned, a pool built with a
        // different `search_path` silently redirects `§15.8`'s only enforced
        // bound: measured, `count("access_records")` answering 10 and then 2 with a
        // decoy schema in front. `Postgres::new` takes the caller's pool, so the
        // path is the caller's unless this pins it.
        //
        // **The path is pinned, not refused.** Reading `current_schemas(false)`
        // and refusing any path with a comma in it fails on the server default,
        // `"$user", public`: one `CREATE SCHEMA` named after the connecting
        // role makes *every* table permanently unprunable. Measured through
        // `prune_these` on a production-shaped pool:
        //
        // ```text
        // before: current_schemas(false) = {public}    -> Ok([Pruned { … }])
        // CREATE SCHEMA noctua;
        // after:  current_schemas(false) = {noctua,public}
        //         Err("[NC-DB-0005] … the search path resolves to more than one schema")
        // ```
        //
        // `NC-DB-0005` says retrying will not help, and every table resolves
        // through the same path — so a guard refusing it turns `§15.8`'s only
        // enforced bound off for every table, because of a schema
        // `CREATE SCHEMA AUTHORIZATION` creates on purpose. A guard that refuses
        // the ordinary default is a guard somebody removes.
        //
        // What such a guard is for is that an unqualified name must not resolve
        // somewhere this crate was not asked about. Setting the path says which
        // schema that is, for the length of this transaction, and `only_for`'s
        // `relpersistence` arm then refuses a temporary shadow of it — the two
        // together are the question a comma test would only stand in for.
        // **Quoted by the server, not validated by the table-name rule.**
        // `identifier` admits `[a-z_][a-z0-9_]*` — so through it, `"Prod"` or
        // `audit-vault`, both legal schemas, make every bounded table
        // permanently unprunable behind a sentence blaming `catalog/limits.toml`,
        // which never mentions the schema: a rule that refuses an ordinary
        // arrangement, like the comma test. `nc-journal` binds its schema and
        // lets `quote_ident` handle any name; so does this.
        sqlx::query("SELECT set_config('search_path', quote_ident($1) || ', pg_temp', true)")
            .bind(&self.schema)
            .execute(&mut *transaction)
            .await
            .map_err(|error| said_while("could not pin the search path", &error))?;
        Ok(transaction)
    }
}

impl Postgres {
    /// The one column a table's primary key is made of, quoted.
    ///
    /// **A row is deleted by its key and not by its address.** `ctid` is where a
    /// row sits, and a concurrent `UPDATE` moves it; the key is what the row
    /// *is*. Refused rather than guessed at when there is not exactly one
    /// column: a declared prunable table without a single-column primary key is
    /// a table this pruner cannot address safely, and saying so is better than
    /// falling back to the thing that skips rows.
    ///
    /// Asked of the same connection as the inheritance guard, and by `regclass`,
    /// so the name resolves the way the delete's own name will.
    async fn primary_key_of(
        on: &mut sqlx::PgConnection,
        table: &str,
    ) -> Result<String, crate::Failure> {
        let rows = sqlx::query(
            "SELECT a.attname::text AS column_name \
             FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = $1::regclass AND i.indisprimary",
        )
        .bind(table)
        .fetch_all(&mut *on)
        .await
        .map_err(|error| said(&error))?;
        let mut names: Vec<String> = Vec::with_capacity(rows.len());
        for row in &rows {
            names.push(
                row.try_get("column_name")
                    .map_err(|error| error.to_string())?,
            );
        }
        match names.as_slice() {
            [only] => identifier(only).map_err(crate::Failure::refused),
            [] => Err(crate::Failure::refused(format!(
                "`{table}` has no primary key, and this pruner deletes rows by theirs. \
                 Addressing them by `ctid` instead skips any row a concurrent `UPDATE` \
                 has moved, which leaves the oldest rows behind for ever while every run \
                 reports success. Give the table a primary key"
            ))),
            many => Err(crate::Failure::refused(format!(
                "`{table}`'s primary key is {} column(s) and this pruner deletes by one. \
                 Widening it here is a change to the statement, not to this message",
                many.len()
            ))),
        }
    }

    /// `ONLY ` for the one shape this crate prunes, and a refusal for the rest.
    ///
    /// **A table name is not a table.** PostgreSQL resolves an unqualified name
    /// to the table *and every child that inherits from it*, so
    /// one `ALTER TABLE access_records INHERIT decoy` makes a decoy with no
    /// rows of its own a door into the real one —
    /// measured, `Pruned { table: "decoy", removed: 5 }` with `access_records`
    /// down to five rows, past a guard that compares the catalog's entry by
    /// name. The journal survives that only because of the migration's trigger,
    /// which this crate says in as many words it does not rely on.
    ///
    /// **And `ONLY` is wrong for a partitioned table**, whose rows all live in
    /// its partitions: `DELETE FROM ONLY parent` deletes nothing. So the
    /// statement asks which shape it has — and a partitioned parent is
    /// **refused**, not pruned without `ONLY`: the address a batch is chosen by
    /// is a primary key, and a parent's key is not unique across its
    /// partitions. `a_partitioned_table_is_refused_rather_than_pruned` holds
    /// that. `relkind = 'p'` is a declarative partition parent; anything else
    /// with children got them from `INHERITS`, which is the other shape being
    /// closed.
    ///
    /// There is one spelling and ten refusals: the statement does not choose
    /// between spellings by shape.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Failure`] when the store cannot be reached, and — which
    /// is the whole purpose of this function — when the named relation is one a
    /// `DELETE` can reach beyond. Its `CASE` has ten arms and this list is all
    /// ten: a temporary table shadowing a permanent one; a table under
    /// row-level security; a partitioned parent; anything that is not an
    /// ordinary table (a view, a materialised view, a foreign table, a
    /// sequence); a table another inherits from; a table that *is* a partition
    /// of another; a table that inherits from another; a `DELETE` rule; a
    /// foreign key with `ON DELETE CASCADE`, `SET NULL` or `SET DEFAULT`; and a
    /// `DELETE` trigger.
    ///
    /// **It does not look at the primary key.** That is
    /// [`Self::primary_key_of`], called separately. The row-level-security and
    /// temporary-table arms are the two a deployment actually meets.
    // allow-logging: `only_for` hands back a column name from the catalog constant, never from a caller
    async fn only_for(
        on: &mut sqlx::PgConnection,
        table: &str,
    ) -> Result<&'static str, crate::Failure> {
        // **An ordinary table, first, because everything below assumes one.**
        // Every branch of this `CASE` is keyed on the *named* relation's oid, and
        // a view matches none of them: without this arm `relkind = 'v'` falls
        // through to `ELSE NULL` — "safe" — and an auto-updatable view satisfies the statement
        // `delete_oldest` issues. Measured, a view named `ledger` over
        // `journal_entries`:
        //
        // ```text
        // guard says   None (relkind v)
        // outcome      Ok([Pruned { table: "ledger", removed: 6, left_alone: None }])
        // journal_entries before=20 after=14
        // ```
        //
        // and with a cascade and an `AFTER DELETE` trigger behind it, a third
        // table goes from 100 rows to 0 with the run reporting `removed: 6`.
        // Every protection the branches below claim reopens behind one view.
        // A materialised view, a foreign table and a sequence are the same
        // shape, so the rule is what this crate prunes — an ordinary table —
        // rather than a list of what it will not.
        //
        // **`ONLY`, always — and a table a `DELETE` can reach beyond is refused
        // rather than pruned.** Three shapes walk past a guard without it, each
        // measured against the real database through the shipped entry point:
        //
        // ```text
        // ATTACH PARTITION      -> 50 000 rows of an ordinary table deleted
        //                          (`ONLY` was skipped for `relkind = 'p'`)
        // CREATE RULE … DO ALSO -> 24 rows went, the pruner reported 4, Ok
        // AFTER DELETE trigger  -> 16 rows went elsewhere, reported 4, Ok
        // ```
        //
        // **Row-level security first, because it changes what every other
        // question means.** A `CASE` reading `relkind`, `relispartition` and
        // the oid, and never `relrowsecurity`, lets a policy of
        // `USING (recorded < 3)`, under a role RLS applies to, make `count`
        // answer 3 for a table holding 20 against a bound of 5, and the run
        // reports
        // `Pruned { before: Some(3), removed: 0, left_alone: None }` with
        // `code() = None`: a table four times over its limit, reported clean,
        // for ever. It does not bite under the superuser this stack ships with,
        // because a superuser bypasses RLS — it bites the moment `§18`'s
        // non-superuser role lands, which `0004` and `0005` name as the answer
        // to their own residual risks.
        //
        // `rows_affected()` counts neither a rule's nor a trigger's deletions,
        // so the over-deletion guard below cannot see either.
        //
        // **And the child side of inheritance, which no branch about a parent
        // matches.** A partition child is `relkind = 'r'` and an `inhrelid`,
        // not an `inhparent`, so a guard asking only about parents refuses the
        // parent and passes the child: measured, `parted` refused and
        // `parted_a` pruned, with all five of the parent's rows gone through it.
        // `migrations/0001_journal.sql` names partitioning as the journal's own
        // retention plan, and the carve-out compares the bare name — so a
        // partition of the journal would not be carved out either.
        //
        // What this crate
        // can check is whether the table it is about to delete from is one whose
        // `DELETE` stays where it is put: no partitions, no children, no rules,
        // no cascading reference, and no `DELETE` trigger.
        //
        // **`NOT tgisinternal` lets referential integrity straight through.** A
        // cascade is implemented as an internal trigger, so read through that
        // filter alone, `ON DELETE CASCADE` is exempt by construction —
        // measured, one `ALTER TABLE … ADD FOREIGN KEY … ON DELETE CASCADE` and
        // then `Pruned { table: "access_records", removed: 3 }` with 1500 rows
        // gone from the referencing table. Aimed at `journal_entries` such a
        // guard passes it too, and only the migration's own append-only trigger
        // stops the delete — the thing the paragraph above says this crate
        // does not rely on. So the constraint is asked about by name, before
        // the trigger clause, because it has a sentence a reader can act on.
        //
        // **And no trigger is exempt by name.** Trigger names are unique per
        // table, not globally, so an exemption for `journal_entries_%` is a
        // free name on any table: measured, the identical `AFTER DELETE`
        // trigger refused under an honest name and accepted under an exempt
        // one, taking 100 rows of another table with `removed: 3, Ok` — and, in
        // the other direction, a `BEFORE DELETE` trigger returning `NULL` gave
        // three rounds of `removed: 0, would_remove: 3` against a `max_rows` of
        // 4, unbounded growth reported as an ordinary successful run.
        // `journal_entries` is refused by name one layer up (`§15.8`'s
        // carve-out), and a caller reaching this door directly is told about
        // the trigger, which is also true.
        //
        // **`pg_temp` is searched before the path, so `bounded`'s guard could not
        // see the shadow it exists to stop.** That guard reads
        // `current_schemas(false)` and refuses a comma; `current_schemas(false)`
        // omits the implicitly-searched schemas, and a temporary relation resolves
        // ahead of every one it does list. Measured on a single-schema path, a real
        // table of 20 rows and a temp table of 3 sharing the name:
        //
        // ```text
        // current_schemas(false) = Ok("{r31c_pa_real}")   <- one schema, no comma
        // count()                = Ok(3)                  <- the temp table
        // prune  -> Ok([Pruned { removed: 3, left_alone: None }])
        // the real table still holds 20
        // ```
        //
        // An ordinary successful run, `code()` of `None`, and `§15.8`'s only
        // enforced bound silently pointed at something else for ever. The path is
        // the wrong place to ask: `$1::regclass` has already resolved the name, so
        // the question belongs on the relation it resolved to. `relpersistence`
        // is that question, and it is the first arm because a shadowed name makes
        // every arm below it an answer about the wrong table.
        //
        // **And the row-security arm asks about more than `rolbypassrls`.** A
        // superuser bypasses row security whatever that column says, and the two
        // come apart: `CREATE ROLE x SUPERUSER NOBYPASSRLS` is an ordinary thing
        // to write, and measured on PostgreSQL 18.6 —
        //
        // ```text
        // rolsuper=t rolbypassrls=f  ->  sees 3 of 3 rows under USING (id < 2)
        // ```
        //
        // — so asked only about that column, the one role that *can* see every
        // row is told it cannot. (`CREATE ROLE … SUPERUSER` alone does set the
        // flag on 18.6, which is why this needs the explicit spelling to reach;
        // such an arm is wrong either way, and a version that stops setting it
        // would make it wrong by default.)
        //
        // **And the owner, who is not subject to row security unless the table
        // says `FORCE`.** That is the ordinary defensive setup — an owner
        // enables RLS and writes a policy for everybody else — and refusing it
        // answers `NC-DB-0005`, which says retrying will not help. Measured on
        // 18.6 with a plain `NOSUPERUSER NOBYPASSRLS` role owning the table:
        //
        // ```text
        // rows the owner really sees: 20 of 20
        // only_for: REFUSED — row-level security is on and this role does not bypass it
        // ```
        //
        // The sentence is false and the table is then permanently unprunable, so
        // `§15.8`'s only enforced bound is off for it. This is the same
        // condemnation the paragraph above makes of an arm asking only about
        // `rolbypassrls`, in the other direction: a refusal naming a condition
        // that does not hold.
        //
        // **And the sentence asks the question the predicate asks.** The
        // predicate asks whether row security *applies*; saying the count "is
        // not the table the catalog bounds" is a claim about whether row
        // security *hides* anything — a different question, and one this role
        // cannot answer, because answering it means comparing what it sees with
        // what is there and that is exactly the privilege it lacks. Measured, a
        // non-owner under a permissive `USING (true)` policy:
        //
        // ```text
        // rows this role sees   20 of 20
        // rls_active            t
        // guard refuses         t   with "neither of which is the table the
        //                            catalog bounds"
        // ```
        //
        // The role reads every row and that sentence tells it it does not.
        // `FORCE ROW LEVEL SECURITY` plus a permissive maintenance policy is the
        // recommended hardened arrangement, and `access_records` — per-person
        // data — is the table RLS exists for, so with that sentence `§15.8`'s
        // only enforced bound is off on exactly the deployment that hardened its
        // database.
        //
        // Refusing stays: it is the safe direction, because a count that is
        // short means the table grows past its bound in silence. What the
        // sentence says is what is true — this crate cannot establish it — and
        // it names the two things a deployment does about it.
        //
        // In the superuser case the code a refusal carries is `NC-DB-0005`,
        // which says retrying will not help — so refused there, a bounded table
        // under RLS is permanently unprunable on a superuser deployment, with a
        // diagnostic naming a condition that does not hold.
        let reachable: Option<String> = sqlx::query_scalar(
            "SELECT CASE \
               WHEN c.relpersistence = 't' \
                 THEN 'it is a temporary table, and a temporary table shadows a \
                   permanent one of the same name for this session alone — so the \
                   bounded table is not the table being counted, and pruning this one \
                   leaves the real one over its limit with nothing said' \
               WHEN c.relrowsecurity AND NOT pg_catalog.has_table_privilege( \
                     current_user, c.oid, 'SELECT') IS DISTINCT FROM TRUE \
                     AND NOT (SELECT rolsuper OR rolbypassrls FROM pg_roles \
                              WHERE rolname = current_user) \
                     AND NOT (pg_catalog.pg_has_role( \
                              current_user, c.relowner, 'USAGE') \
                              AND NOT c.relforcerowsecurity) \
                 THEN 'row-level security is on and this role does not bypass it, so \
                   this crate cannot establish that what it counts is the whole table. \
                   It may be: a permissive policy over this role sees every row, and a \
                   role that cannot bypass row security has no way to find out. Give the \
                   pruning role BYPASSRLS, or run it as the table owner without FORCE ROW \
                   LEVEL SECURITY, and the count is the table' \
               WHEN c.relkind = 'p' THEN 'it is partitioned, and a partition is a table \
                 this catalog does not name' \
               WHEN c.relkind <> 'r' THEN 'it is not an ordinary table — a view, a \
                 materialised view, a foreign table and a sequence all answer to a \
                 name and none of them is what this crate prunes' \
               WHEN EXISTS (SELECT 1 FROM pg_inherits WHERE inhparent = c.oid) \
                 THEN 'another table inherits from it' \
               WHEN c.relispartition \
                 THEN 'it is a partition of another table, so deleting here empties part \
                   of a table this catalog does not name' \
               WHEN EXISTS (SELECT 1 FROM pg_inherits WHERE inhrelid = c.oid) \
                 THEN 'it inherits from another table, so it is part of one this catalog \
                   does not name' \
               WHEN EXISTS (SELECT 1 FROM pg_rewrite r WHERE r.ev_class = c.oid \
                 AND r.ev_type = '4' AND r.rulename <> '_RETURN') \
                 THEN 'a rule rewrites DELETE on it' \
               WHEN EXISTS (SELECT 1 FROM pg_constraint fk WHERE fk.confrelid = c.oid \
                 AND fk.contype = 'f' AND fk.confdeltype IN ('c', 'n', 'd')) \
                 THEN 'another table references it with ON DELETE CASCADE, SET NULL or \
                   SET DEFAULT, so deleting a row here changes rows there' \
               WHEN EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgrelid = c.oid \
                 AND (t.tgtype & 8) <> 0 AND NOT t.tgisinternal \
                 AND t.tgenabled <> 'D') \
                 THEN 'a trigger fires on DELETE and may touch another table' \
               ELSE NULL END \
             FROM pg_class c WHERE c.oid = $1::regclass",
        )
        .bind(table)
        .fetch_one(on)
        .await
        .map_err(|error| said(&error))?;
        if let Some(why) = reachable {
            return Err(crate::Failure::refused(format!(
                "`{table}` cannot be pruned safely: {why}. A `DELETE` here would reach \
                 rows this crate was not asked about, and `rows_affected` would not count \
                 them — so it is refused rather than run"
            )));
        }
        Ok("ONLY ")
    }
}

impl Target for Postgres {
    fn database(&self) -> nc_catalog::Database {
        nc_catalog::Database::Server
    }

    async fn count(&mut self, table: &str) -> Result<u64, crate::Failure> {
        let table = identifier(table)?;
        // **The guard first, then the connection.** Taking a connection,
        // opening a transaction on it and *then* asking an `only_for` that goes
        // to the pool for a second one waits on itself against a pool of one:
        // measured, `Err(Failure { why: "pool timed out while waiting for an
        // open connection" })` after 30.0 s, six times `LOCK_TIMEOUT`.
        //
        // **Inside the transaction, on its connection.** Asking the pool for a
        // second one is both a self-deadlock and a window: with the guard on one
        // connection and the statement on another, a `CREATE RULE` 1.2 s after
        // the guard passed applies to the delete — measured,
        // `delete_oldest -> Ok(6)` with 100 rows gone from a third table, and
        // `LOCK_TIMEOUT` is 5 s, which is the size of the window. One
        // connection and one transaction close both.
        let mut bounded = self.bounded().await?;
        let only = Self::only_for(&mut bounded, &table).await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) AS n FROM {only}{table}"
        )))
        .fetch_one(&mut *bounded)
        .await
        .map_err(|error| said(&error))?;
        bounded
            .commit()
            .await
            .map_err(|error| said_while("could not finish counting", &error))?;
        let n: i64 = row.try_get("n").map_err(|error| error.to_string())?;
        as_a_count(n, &table).map_err(crate::Failure::refused)
    }

    async fn delete_oldest(
        &mut self,
        table: &str,
        order_by: &str,
        batch: u64,
    ) -> Result<u64, crate::Failure> {
        // Before `identifier`, which returns a *quoted* name: the catalog holds
        // the bare one, and comparing the two forms silently matched nothing.
        crate::the_catalog_orders_its_own_tables(table, order_by, self.database())?;
        let table = identifier(table)?;
        let order_by = identifier(order_by)?;
        // Bound as a value, because it is one. The cast is `i64` for the same
        // reason as above, and a batch that does not fit is an error rather than
        // a negative `LIMIT` PostgreSQL would refuse with a message about
        // syntax.
        let limit = as_a_limit(batch, &table)?;
        // The guard runs on this transaction's own connection, which is what
        // removes the *self-deadlock* and shrinks the window: on another
        // connection, a `CREATE RULE` 1.2 s after the guard passed applied to
        // the delete, against a `LOCK_TIMEOUT` of 5 s, measured.
        //
        // **It does not close that window, and saying it does would be an
        // overclaim.** The guard is a `SELECT` against `pg_class` and takes no
        // lock on the target relation, so a concurrent DDL can still commit
        // between the two statements — narrower by a transaction's worth of
        // time rather than eliminated. What would close it is
        // `LOCK TABLE … IN ACCESS EXCLUSIVE MODE` before the guard, and that is
        // a bigger trade than it looks: this crate's whole reason for
        // `LOCK_TIMEOUT` is that a pruner must not hold a table against the
        // platform, and an exclusive lock is exactly that. So the window is
        // named here rather than closed, and the over-deletion guard below is
        // what stands behind it.
        let mut bounded = self.bounded().await?;
        let only = Self::only_for(&mut bounded, &table).await?;
        // **By primary key, because a `ctid` is where a row *is* and another
        // transaction can move it.** With the subselect choosing
        // `(tableoid, ctid)`, an `UPDATE` committed against a chosen row
        // between the subselect's snapshot and the delete's row lock re-points
        // that row — so PostgreSQL re-checks the new tuple, finds its `ctid` is
        // not in the list, and **skips it**. Measured, no injected delay, an ordinary single-row
        // `UPDATE` loop beside a batch of 5000:
        //
        // ```text
        // removed 4991 of 5000, rows left 15009 where 15000 was expected
        // ```
        //
        // and with the window held open, on `access_records` and its declared
        // ordering column, five rounds:
        //
        // ```text
        // delete_oldest(access_records, recorded, 4) -> Ok(3)
        // the oldest row's ctid (0,1) -> (0,21), its `recorded` unchanged
        // round 0..4: removed 3 each time, the same oldest row surviving
        // ```
        //
        // That last line is what matters. `recorded` is preserved by `0006`'s
        // trigger on `UPDATE` — written so that "a row that can renumber itself
        // can make itself immortal" — so a skipped row keeps its place at the
        // head of every future batch and is skipped again. `§15.8`'s only
        // enforced bound stops converging and every run reports `Ok`. The
        // `UPDATE` that does it is not hypothetical: `§15.6` fills in a city on
        // an access record after the fact.
        //
        // A primary key does not move. The re-check compares the new tuple's key
        // against the same materialised list and the row is deleted; measured
        // against the identical race, `DELETE 4` of 4 with the oldest row gone.
        //
        // **And not `SELECT … FOR UPDATE`, which is worse than the defect.** It
        // works, and it needs the `UPDATE` privilege — measured, the probe role
        // holding `SELECT, DELETE` is refused with
        // `permission denied for table rows_over_time`. Granting the pruner
        // `UPDATE` on `journal_entries` to make it prunable is the one privilege
        // the append-only design exists to withhold.
        let key = Self::primary_key_of(&mut bounded, &table).await?;
        let removed = sqlx::query(sqlx::AssertSqlSafe(format!(
            // **`ONLY`, because a table name is not a table.** PostgreSQL
            // resolves an unqualified name to the table *and every child that
            // inherits from it*, so one
            // `ALTER TABLE access_records INHERIT r24c_decoy` makes a decoy
            // with no rows of its own a door into the real one: measured,
            // `Pruned { table: "r24c_decoy", removed: 5 }` and `access_records`
            // down to five rows, past a guard that compares the catalog's entry
            // by name. The journal survives only because of the migration's
            // trigger, which this crate says in as many words it does not rely
            // on.
            //
            // The count is `ONLY` too, or a decoy would report a size it does
            // not have and the arithmetic would be about somebody else's rows.
            "DELETE FROM {only}{table} WHERE {key} IN \
             (SELECT {key} FROM {only}{table} ORDER BY {order_by} ASC LIMIT $1)"
        )))
        .bind(limit)
        .execute(&mut *bounded)
        .await
        .map_err(|error| said(&error))?
        .rows_affected();
        // **More than was asked for is a defect, not a result.** A `ctid`
        // version removed eight rows for a batch of four and reported eight,
        // measured, and `Pruned.removed` carries the number on to whoever reads
        // it. A pruner that has taken rows it was not asked for has already
        // taken them, so this cannot undo it — what it can do is refuse to
        // report the run as ordinary.
        if let Some(why) = over_deleted(removed, batch) {
            bounded.rollback().await.map_err(|error| {
                crate::Failure::refused(format!(
                    "`{table}`: {why}, and rolling that back failed: {error}"
                ))
            })?;
            return Err(crate::Failure::refused(format!("`{table}`: {why}")));
        }
        bounded.commit().await.map_err(|error| {
            // **Through `said`, because this is where `40001` arrives.**
            // `serialization_failure` is raised at *commit*, and a refusal
            // built by hand here would consult the entry in
            // `WAITED_RATHER_THAN_REFUSED` for it nowhere near the place it
            // happens.
            said_while("could not commit the pruning", &error)
        })?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::{WAITED_RATHER_THAN_REFUSED, as_a_count, as_a_limit, identifier};

    /// The codes a run waited for are the ones written down.
    ///
    /// **A subset is not an equality.** The integration test names eight codes
    /// and asserts each one waits, which holds for any superset: measured, a
    /// ninth — `22012`, *division by zero* — added leaves that suite green,
    /// so `said()` would mark a data exception the record's own fault as
    /// retryable and a scheduler would retry for ever a statement that can
    /// never succeed. That is the mirror of the incident `Failure::answered`
    /// exists for, and the list needs both directions.
    #[test]
    fn the_codes_a_run_waited_for_are_the_ones_written_down() {
        assert_eq!(
            WAITED_RATHER_THAN_REFUSED,
            [
                "57P01", "57P02", "57P03", "53300", "55P03", "57014", "40P01", "40001"
            ],
            "the codes read as `the server was busy` moved — one added is a permanent \
             refusal reported as retryable, and one removed is a restart reported as final"
        );
    }

    /// The two conversions, which no query can drive.
    ///
    /// `count(*)` cannot be negative and a batch past `i64::MAX` cannot be
    /// asked for through the catalog — which is precisely why both are written
    /// as conversions that can fail, and precisely why `n as u64`
    /// and `batch as i64` in their place leave the whole suite green without
    /// this.
    #[test]
    fn a_number_that_is_not_a_count_or_a_limit_is_refused() {
        assert_eq!(as_a_count(0, "t"), Ok(0));
        assert_eq!(as_a_count(1_000_000, "t"), Ok(1_000_000));
        let refused = as_a_count(-1, "t").expect_err("a negative is not a count");
        assert!(refused.contains("which is not a count"), "{refused}");

        assert_eq!(as_a_limit(50_000, "t"), Ok(50_000));
        let refused = as_a_limit(u64::MAX, "t").expect_err("more rows than can be asked for");
        assert!(
            refused.contains("than PostgreSQL can be asked for"),
            "{refused}"
        );
    }

    #[test]
    fn a_name_the_catalog_did_not_write_is_refused() {
        for (name, quoted) in [
            ("access_records", "\"access_records\""),
            ("journal_entries", "\"journal_entries\""),
            ("_x", "\"_x\""),
            ("t9", "\"t9\""),
            // **Schema-qualified**, which the unbounded-table gate emits for a
            // table created in a schema and which `catalog/limits.toml` can
            // declare. Refusing it would fail every prune run on a name
            // written exactly where the gate asks for it.
            ("archive.access_records", "\"archive\".\"access_records\""),
            // **A reserved word**, which the shape test admits and which
            // PostgreSQL reads as a function when it is unquoted.
            ("user", "\"user\""),
            ("order", "\"order\""),
        ] {
            assert_eq!(identifier(name), Ok(quoted.to_owned()), "{name}");
        }
        // The shapes that must not reach a statement. None of these is reachable
        // from the catalog today, which is exactly why the guard needs its own
        // test: nothing else would notice it being deleted.
        for name in [
            "",
            "9rows",
            "Access_Records",
            // **Uppercase after the first character**, which `starts_with` does
            // not see: without this the `.all(…)` clause beside it is decided
            // by nothing, and `is_ascii_alphanumeric()` in its place leaves
            // this crate green. The name is quoted after the shape is known, so
            // `aCcess` and `access` would be two tables — breaking the
            // invariant the quoting relies on, that every part is already
            // lowercase.
            "aCcess_records",
            "access records",
            "access-records",
            "access_records; DROP TABLE journal_entries",
            // Still refused: an empty part, a third part, and a quote.
            ".access_records",
            "archive.",
            "a.b.c",
            "archive.\"access_records\"",
            "access_records\"",
            "a".repeat(64).as_str(),
        ] {
            let refused = identifier(name).expect_err("must be refused");
            assert!(refused.contains("plain identifier"), "{refused}");
        }
    }
}

#[cfg(test)]
mod the_last_line_of_defence {
    /// A batch that takes what it asked for is not over-deletion; one more is.
    ///
    /// **The guard is reached by this and by nothing else.** The reach guard
    /// refuses every partitioned and inherited relation first, so the condition
    /// is unreachable by construction — and that is what makes it the last line
    /// of defence behind the `SELECT`-versus-`DELETE` window this file names
    /// rather than closes. Without this, deleting the whole block
    /// leaves `cargo test -p nc-prune` green.
    #[test]
    fn over_deletion_is_more_than_the_batch_and_not_the_batch() {
        assert!(
            super::over_deleted(4, 4).is_none(),
            "a batch of four taking four is the ordinary run"
        );
        assert!(super::over_deleted(0, 4).is_none());
        assert!(super::over_deleted(3, 4).is_none());
        // **One over, which is the boundary the sentence above defines and the
        // only value that separates `>` from `>=` and from `> batch + 1`.**
        // With only 8-for-4 (double) and 4-for-4 (equal), a guard written one
        // row wide lets exactly one extra row through — measured,
        // `removed > batch.saturating_add(1)` leaves `check passed`, and a
        // `DELETE` that took one row more than its batch is committed and
        // reported as an ordinary run. This is the last thing between an
        // irreversible delete and a `ctid` that reached further than it was
        // asked to.
        let one_over = super::over_deleted(5, 4).expect("five for a batch of four is over");
        assert!(
            one_over.starts_with("5 rows were removed for a batch of 4"),
            "one row over the batch was not reported, or not reported as itself: {one_over}"
        );
        let why = super::over_deleted(8, 4).expect("eight for a batch of four is over");
        // **In that order.** `contains('8') && contains('4')` is a presence
        // test for two digits, so swapping the two numbers in the sentence —
        // telling an operator four rows were removed for a batch of eight —
        // leaves such a suite green. The sentence is what a person reads after
        // an irreversible delete.
        assert!(
            why.starts_with("8 rows were removed for a batch of 4"),
            "the numbers are not in the order the sentence says: {why}"
        );
        assert!(
            why.contains("rolled back"),
            "the sentence does not say the rows are still there: {why}"
        );
    }

    /// Every statement that reads or deletes rows says `ONLY`.
    ///
    /// **The clause the module header leads with, decided here.** Without
    /// this, `only_for` returning `""` instead of `"ONLY "` leaves the whole
    /// gate green: the reach guards refuse every partitioned and inherited
    /// relation before the statement runs, so today it is behaviour-preserving
    /// — which is exactly why nothing else notices, and exactly what makes it
    /// the last line of defence rather than the first.
    ///
    /// A source assertion, like the guard order below it and for the same
    /// reason: the condition it protects against is unreachable while the guards
    /// hold, and what matters is that the clause is still written where the rows
    /// are counted, chosen and deleted.
    #[test]
    fn every_statement_that_touches_rows_says_only() {
        // **The shipped statements, not this test's own array.** Scanning the
        // whole file, the two needles below are string literals *in* that file
        // — so `source.contains(statement)` would match itself and could not be
        // false. Measured that way: `{only}` removed from both halves of the
        // real `DELETE` leaves `nc-prune` and `xtask` green. And the count needle
        // has to match the implementation, which writes
        // `SELECT count(*) AS n FROM …`.
        let source = include_str!("postgres.rs")
            .split("mod tests")
            .next()
            .expect("this file has a test module");
        assert!(
            source.contains("..carried"),
            "`said_while` no longer carries the rest of `said`'s answer. It re-stated \
             `waited` and `answered` field by field once, and `waited: carried.waited` was \
             decided by no test at all — five call sites reach it and the one that is \
             driven carries a transport failure, which has no SQLSTATE to read. `40001` is \
             raised at COMMIT, so what that costs is a retryable skip recorded as a \
             permanent refusal"
        );
        assert!(
            source.contains("Ok(\"ONLY \")"),
            "`only_for` no longer answers `ONLY ` for an ordinary table, so a decoy \
             that inherits from a catalog table becomes a door into it"
        );
        let mut said = 0_usize;
        for statement in [
            "SELECT count(*) AS n FROM {only}{table}",
            "DELETE FROM {only}{table}",
            // **The clause that *chooses* the rows.** This doc says "counted,
            // chosen and deleted", and with only two of the three in the array,
            // dropping `ONLY` from the inner subquery leaves the workspace and
            // the gate green. The outer `DELETE` names the parent, and the
            // subquery is what decides which rows it is handed.
            "(SELECT {key} FROM {only}{table} ORDER BY",
        ] {
            assert!(
                source.contains(statement),
                "no statement reads `{statement}`, so either it was rewritten without \
                 `ONLY` or this test is looking for the wrong text"
            );
            said += 1;
        }
        assert_eq!(
            said, 3,
            "the rows are counted, chosen and deleted, and all three say `ONLY`"
        );
    }

    /// The rollback happens before anything is committed.
    ///
    /// A source assertion, because the branch cannot be reached at run time: the
    /// property that matters is the *order* — a `DELETE` that took too much must
    /// be undone rather than committed and reported. `release.rs` holds its own
    /// "the gate runs before the first write" the same way, for the same reason.
    #[test]
    fn the_over_deletion_guard_runs_before_the_commit() {
        // **The shipped code, not this file.** `include_str!` reads the whole
        // file including this module, and all three needles below are string
        // literals this module contains — so uncut, every `expect` has a
        // guaranteed fallback inside the test's own text and the assertion can
        // degrade into three positions in itself. Measured: the guard and the
        // rollback deleted outright and the commit moved first, with
        // `check passed`. The sibling five screens up cuts at `mod tests` for
        // the same reason.
        let source = include_str!("postgres.rs")
            .split("mod tests")
            .next()
            .expect("this file has a test module");
        let body = source
            .split_once("let removed = sqlx::query(sqlx::AssertSqlSafe(format!(")
            .expect("the delete is here")
            .1;
        let guard = body
            .find("if let Some(why) = over_deleted(removed, batch)")
            .expect("the over-deletion guard is here");
        let rollback = body
            .find("bounded.rollback()")
            .expect("the rollback is here");
        let commit = body.find("bounded.commit()").expect("the commit is here");
        assert!(
            guard < rollback && rollback < commit,
            "the guard is at {guard}, the rollback at {rollback} and the commit at \
             {commit}: a `DELETE` that took more than it was asked for would be \
             committed and reported as the batch"
        );
    }
}
