//! The chain, against the store that actually runs.
//!
//! `InMemory` is what makes acceptance criterion 3 provable at every position of
//! a synthetic history. This is the other half: the same chain, through
//! PostgreSQL, so that the mapping between an `Entry` and a row is exercised
//! rather than assumed.
//!
//! **What it is for is the round trip.** Every field goes out through `append`
//! and comes back through `from`, and the entry that comes back has to hash to
//! the same value — which is the only way to notice a column that was written
//! from the wrong field, or read into one. A unit test cannot see that: the
//! encoding is only wrong once it has been through the database.
//!
//! It needs `DATABASE_URL`. `cargo xtask dev` sets the stack up and prints it.

use nc_canon::Value;
use nc_ids::{MachineFlake, Minter, OpaqueId, Snowflake, SystemClock};
use nc_journal::postgres::Postgres;
use nc_journal::{Closing, Entry, Journal, Outcome, Record, Store as _};
use sqlx::postgres::PgPoolOptions;

fn id(text: &str) -> OpaqueId {
    OpaqueId::new(text).expect("a valid identifier")
}

/// A store on `pool`, in the schema `pool` was opened for.
///
/// **The store is told its schema, and every test says which.** `Postgres::new`
/// resolves nothing — see its own doc for why every way of asking the database
/// gets `"$user"` back. Tests open a schema of their own, so they say which.
fn store_in(pool: &sqlx::PgPool, schema: &str) -> Postgres {
    Postgres::in_schema(pool.clone(), &format!("{schema}_{}", std::process::id()))
}

/// The prefixes every schema this suite makes begins with.
///
/// Two shapes reach the database — `pool`'s `{label}_{pid}`
/// and `a_schema_named`'s `{prefix}_{pid}_{label}` — and the sweeper that
/// removes what a dead run left behind matches by prefix. Matching `nc_` alone
/// would leave the two labels that do not begin with it unswept.
const SWEPT: [&str; 3] = ["nc_", "recorded_is_assigned", "head_undecodable"];

/// The most concurrent `append`s any test in this file spawns.
///
/// Named so the pool and the tests cannot drift apart:
/// `every_racing_test_fits_in_the_pool` reads both.
const THE_LARGEST_FAN_OUT: u32 = 16;

/// A pool in a schema of this test's own, or `None` when there is no database.
///
/// **A schema per test, and not a shared table.** These run concurrently, and
/// the journal is a single dense sequence — two tests appending at sequence 0
/// would collide, and the first thing they would do is drop each other's table.
/// A schema is PostgreSQL's own answer and costs one statement.
///
/// **Fail-closed, and skipping is something the gate has to ask
/// for.** Returning `None` whenever `DATABASE_URL` is unset would make the skip
/// indistinguishable from a run: a skipped test still counts as passed, so
/// cutting the one wire that hands the url to the test child would
/// leave `cargo xtask check` printing `ok postgres is reachable` and its test
/// count with all four of these having done nothing. The reachability line is
/// about the gate's own socket, not the child's.
///
/// So the default is to fail. `cargo xtask check` sets `NC_NO_STACK` under
/// `--offline` and on a system that runs no stack, with the reason as its
/// value, and that — not the absence of a variable — is what permits a skip. Cutting
/// either wire turns the suite red, which is `§2.4`'s rule applied to a test:
/// if the check cannot be performed, the answer is no.
///
/// **Wherever the gate runs.** A child inheriting `DATABASE_URL` from the
/// parent would make cutting the wire invisible to anyone with it exported —
/// and the CI workflow sets it at job level, so it would be invisible
/// in CI. `check.rs` removes it from the child's environment before setting
/// what it measured.
async fn pool(schema: &str) -> Option<sqlx::PgPool> {
    // **The process id, so two runs do not destroy each other.** `pool` drops
    // and recreates the schema it is handed, so with fixed schema names
    // `cargo xtask check` is not safe to run twice against one database:
    // measured, three consecutive paired runs produce 9, 11 and 5 failures, all
    // of them one run dropping the other's tables mid-test. The failure presents
    // as `duplicate key … Key (sequence)=(0) already exists`, which sends a
    // reader to the chain rather than to the schema.
    //
    // It leaves nothing behind: `finish` drops each test's schema at the end of
    // each test, and its own doc carries the measurement — 1170 schemas in one
    // developer's database — that makes it necessary.
    //
    // **A drop that fails fails the test.** Reported with `eprintln!`, which
    // cargo swallows on a passing test, the claim above would be checked by
    // nothing — measured on a developer's database: two schemas left by runs
    // whose processes were long gone. What is *not* claimed is a schema left by
    // a test that panicked: the panic is the finding, and the schema is
    // evidence to look at.
    let schema = &format!("{schema}_{}", std::process::id());
    let url = match std::env::var("DATABASE_URL").ok() {
        Some(url) => url,
        None if std::env::var_os("NC_NO_STACK").is_some() => {
            // **Announced, so the count cannot mean two things.** `check`
            // reports the test count from cargo's own totals, and a skipped
            // test still counts as passed — so unannounced, `--offline` would
            // print the same number as a run that had proved the chain against
            // a real store. The line below reaches the gate's output through
            // cargo's `--nocapture`-less stdout on a *failing* run only, so
            // `check` also counts these and says so beside the number.
            let why = std::env::var("NC_NO_STACK").unwrap_or_default();
            println!("nc-journal: skipped, no database: {why}");
            return None;
        }
        None => panic!(
            "DATABASE_URL is not set. These tests prove the chain against the store that \
             runs, so skipping them silently is the failure `cargo xtask check` exists to \
             prevent. Run `cargo xtask dev` and then `cargo xtask check`, or \
             `cargo xtask check --offline` to skip them on purpose"
        ),
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("DATABASE_URL is set, so it has to connect");
    // Dropped and recreated, so a run starts from nothing whatever the last one
    // left behind. The journal's trigger refuses `DELETE`, which is the point of
    // the table — so the schema goes, not the rows.
    // **Every prefix this suite names, not the one that is obvious.**
    // Matching `LIKE 'nc_%'` alone misses the two `pool` labels that do not
    // begin with it, and those are the schemas a panicking test leaves — one
    // per script. Held to
    // the file by `every_prefix_this_suite_uses_is_swept`, which reads the
    // `pool(` call sites, so a new label that starts otherwise fails here rather
    // than accumulating quietly.
    for prefix in SWEPT {
        sweep_schemas_from_dead_runs(&admin, prefix).await;
    }
    // **And the roles, which are cluster-global.** A schema belongs to a
    // database and a role does not: one left behind by a run that panicked is
    // visible to every database on the cluster and to every other repository
    // sharing it. Measured on a developer's cluster without this sweep: seven,
    // from runs whose processes were long gone — all from tests in this file
    // and in `nc-prune`'s, each named with the pid that made it, and nothing
    // removing one.
    sweep_roles_from_dead_runs(&admin).await;
    for statement in [
        format!("DROP SCHEMA IF EXISTS {schema} CASCADE"),
        format!("CREATE SCHEMA {schema}"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&admin)
            .await
            .expect("the test's own schema is ours to manage");
    }
    admin.close().await;

    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("a valid url");
    let pool = PgPoolOptions::new()
        // **Enough for the fan-out the tests in this file create.** Two, against
        // sixteen concurrent `append`s — and a refused append acquires three
        // times, so those sixteen are forty-eight acquisitions through two
        // slots. Under load the pool times out and the append comes back
        // `Unavailable`/`JournalUnavailable`, which is neither of the two codes
        // `one_call_gets_one_answer_however_the_scheduler_orders_it` and
        // `a_race_between_writers_is_contention_and_says_so` assert, so both
        // fail with a message about the journal for a reason that is the
        // harness. Measured that way on an unmutated tree while other builds
        // share the machine; both pass alone.
        //
        // The number is the largest fan-out any test here spawns. It is per test
        // *schema*, and each of these schemas exists for one test, so the
        // ceiling on the server is the number of tests running at once — the
        // same as before, sixteen times over, and PostgreSQL's default
        // `max_connections` of 100 is what bounds it. `acquire_timeout` is left
        // at its default: a wait that long is a hang, not contention.
        .max_connections(THE_LARGEST_FAN_OUT)
        .connect_with(options.options([("search_path", schema)]))
        .await
        .expect("connects into the schema");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("the migrations apply");
    Some(pool)
}

/// Drops the test's schema and closes its pool.
///
/// **Without this, nothing drops one, and they accumulate.** `pool` drops and
/// recreates the schema it is *handed*, at setup, and every schema name carries
/// the process id — so two runs never collide and no run cleans up. Measured on
/// a developer's stack: **1170** schemas from dozens of runs, in the shared
/// database, in the stage whose subject is that records do not grow without
/// bound — the suite itself the unbounded thing.
///
/// Called at the end of each test instead of in a `Drop`, because dropping a
/// schema is a query and `Drop` cannot await one. A test that **panics** leaves
/// its schema behind, and that is deliberate: a failing run's rows are what a
/// developer wants to look at, and `cargo xtask dev --down` removes the volume.
async fn finish(pool: sqlx::PgPool) {
    // **Never `public`, and never anything this suite did not name.**
    // `current_schema()` is the first *existing* schema on the path, not the one
    // the pool asked for — so on a pool whose path resolves elsewhere it drops
    // the developer's `public` schema, taking the journal, the access records
    // and the migration record with it. That is the same `$user` resolution
    // this whole class is about, in the cleanup helper.
    //
    // **The process id, which is the one thing every schema this suite makes
    // carries and no shared schema does.** A prefix list grows by one on each
    // run — `nc_test`, then `head_undecodable`, then `recorded_is_assigned` —
    // which is an exclusion list keeping a destructive command honest, exactly
    // the shape this repository refuses everywhere else. A `DROP SCHEMA` in a
    // shared database that guesses has no undo.
    //
    // Asked here rather than in a `DO` block, because a `DO` block takes no
    // parameters and the alternative is interpolating the answer into the
    // statement that checks it.
    let here: Option<String> = sqlx::query_scalar("SELECT current_schema()::text")
        .fetch_one(&pool)
        .await
        .expect("the connection can say where it is");
    let mine = here
        .as_deref()
        .is_some_and(|name| name.ends_with(&format!("_{}", std::process::id())));
    assert!(
        mine,
        "this suite will not drop `{}`: it is not a schema this run made",
        here.as_deref().unwrap_or("nothing")
    );
    let dropped = sqlx::query(
        "DO $$ BEGIN EXECUTE format('DROP SCHEMA %I CASCADE', current_schema()); END $$",
    )
    .execute(&pool)
    .await;
    // **Asserted, because a message here is invisible.** Reported with
    // `eprintln!`, on the reasoning that a test which has made its point should
    // not fail on the tidying, a drop that stops working leaves schemas behind
    // and says nothing, because cargo swallows the output of a passing test.
    // Measured on a developer's database: two left behind, dated to runs whose
    // processes were long gone.
    //
    // The tidying *is* part of the point: the module header two screens up says
    // this suite leaves nothing behind, and a claim nothing checks is the shape
    // this repository is built against.
    pool.close().await;
    dropped.expect("the test schema could not be dropped, and this suite claims to leave none");
}

fn full_record() -> Record {
    // Every optional field set, so a column read from the wrong field cannot
    // pass by both being `NULL`.
    let mut record = Record::new(
        id("principal-1"),
        "Delete",
        Value::object([
            ("kind", Value::text("content")),
            ("id", Value::text("c-1")),
            (
                "nested",
                Value::Array(vec![Value::Integer(-1), Value::Bool(true)]),
            ),
        ]),
        Outcome::Denied,
        1_767_225_600_123,
    );
    record.realm = id("realm-1");
    record.actor_chain = vec![id("agent-1"), id("principal-1")];
    record.deciding_policy = Some("core.destructive_needs_quorum".to_owned());
    record.policy_pack = Some("family@1".to_owned());
    record.policy_epoch = Some(7);
    record.grant_used = Some(id("grant-1"));
    record.work_unit = Some(Snowflake::from_crockford("01J8XK2M9P51").expect("a valid unit"));
    record
}

/// A record with nothing optional set, for a test about one field.
///
/// `full_record` sets every optional field so a column read from the wrong one
/// cannot pass by both being `NULL`; that is the wrong fixture for a test whose
/// subject is the work unit, because it already carries one.
fn entry_record(action: &str) -> Record {
    Record::new(
        id("principal-1"),
        action,
        Value::text("a resource"),
        Outcome::Allowed,
        1_767_225_600_123,
    )
}

/// A journal over `store`, in `realm-1`, with a flake of its own.
///
/// One flake per journal, because two sharing one mint the same work-unit
/// identifiers — and a test about which realm closed a unit would then be a
/// test about two journals that agree by accident.
fn journal(store: Postgres) -> Journal<Postgres> {
    use core::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(1);
    let flake = NEXT.fetch_add(1, Ordering::Relaxed) % nc_ids::flake::RANGE;
    Journal::new(
        store,
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(flake).unwrap_or_else(|_| unreachable!("masked")),
            SystemClock,
        ),
    )
}

#[tokio::test]
async fn a_write_that_lands_while_the_chain_is_verified_is_not_tampering() {
    // **Reading the entries and then the tail, and comparing them with `!=`,**
    // an append landing between the two makes the tail newer than the read,
    // and `verify` answers `HistoryDoesNotReachTheTail` — the journal reporting
    // **tampering** because somebody wrote to it.
    //
    // Measured without either half, 300 concurrent appends against a continuous
    // verifier: entries-then-tail gives 187 false alarms in 309 rounds, and
    // `!=` with the read order corrected still gives 16 in 601. Each half
    // reverted cleanly with the rest of the workspace green, so this is what
    // holds them.
    const SCHEMA: &str = "nc_test_racing_verify";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut writer = Journal::new(
        store_in(&pool, "nc_test_racing_verify"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(11).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let reader = Journal::new(
        store_in(&pool, "nc_test_racing_verify"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(12).expect("fits in ten bits"),
            SystemClock,
        ),
    );

    writer.record(full_record()).await.expect("the first entry");
    // Interleaved by hand rather than by threads: the race is between two
    // queries, and doing one write between them reproduces it exactly and
    // deterministically.
    for _ in 0..20 {
        let verifying = reader.verify(0);
        let writing = writer.record(full_record());
        // The write lands first, so the read that follows sees more than the
        // tail this verification began with.
        let (written, verified) = tokio::join!(writing, verifying);
        written.expect("the append succeeds");
        let verdict = verified.expect("the store answers");
        assert!(
            verdict.is_ok(),
            "a concurrent append was reported as tampering: {verdict:?}"
        );
    }
    assert!(reader.verify(0).await.expect("answers").is_ok());
    finish(pool).await;
}

#[tokio::test]
async fn a_read_that_falls_short_of_the_tail_is_still_caught() {
    // The other side of the same comparison. Loosening `!=` to `<` must not
    // stop this from firing: a store whose read hides a suffix while still
    // reporting the real last sequence is the one shape a chain of links cannot
    // see, because every remaining link is consistent.
    const SCHEMA: &str = "nc_test_short_read";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = Journal::new(
        store_in(&pool, "nc_test_short_read"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(13).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    for _ in 0..4 {
        journal.record(full_record()).await.expect("appends");
    }
    assert!(journal.verify(0).await.expect("answers").is_ok());

    // A verification that starts *after* the tail has nothing to read, which is
    // the same shape from the other end and must not read as clean.
    let past_the_end = journal.verify(99).await.expect("answers");
    assert!(
        past_the_end.is_err(),
        "a read that returned nothing verified clean: {past_the_end:?}"
    );
    finish(pool).await;
}

#[tokio::test]
async fn the_public_store_refuses_what_the_journal_refuses() {
    // **The rule is held at the store, not one layer up.** `Journal::append` is
    // `pub(crate)`; `Store::append` is public, `Postgres` is public, and this
    // very file calls `Store` directly six times — so a guard in the journal
    // alone is a rule about one code path, which is the shape `postgres.rs`'s
    // own module header names.
    //
    // Measured through the public path with the guard one layer up: the append
    // succeeds and from that moment `tail`, `from`, `verify` at any sequence,
    // and every later `record` all fail, with `DELETE` refused by the
    // trigger.
    const SCHEMA: &str = "nc_test_public_append";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_public_append");

    let mut deep = Value::Integer(1);
    for _ in 0..=nc_canon::parse::MAX_DEPTH {
        deep = Value::object([("a", deep)]);
    }
    let mut record = full_record();
    record.resource = deep;
    let entry = nc_journal::Entry::chain(0, None, record);

    let refusal = store
        .append(entry)
        .await
        .expect_err("a store refuses what it could not read back");
    assert!(
        matches!(refusal, nc_journal::Error::TooDeep { .. }),
        "{refusal:?}"
    );

    // And the store still works, which is the whole point of refusing.
    assert_eq!(store.tail().await.expect("answers"), None);
    finish(pool).await;
}

#[tokio::test]
async fn a_negative_sequence_cannot_reach_the_table_at_all() {
    // **A row nothing could see and nothing could remove.** `sequence` is a
    // `BIGINT` carrying a `u64`, and without a constraint a second connection
    // can insert one at -1: `from` filters `sequence >= $1` and `tail` orders
    // descending, so both step straight over it. Measured without the
    // constraint — a row recording `Escalate` by `attacker` sits in an
    // append-only table, can never be deleted, and `verify(0)` calls the
    // history clean.
    //
    // The check belongs on the table because the caller who writes such a row
    // is not this code.
    const SCHEMA: &str = "nc_test_negative_sequence";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let refused = sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (-1, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{}', 'allowed', 0, 0, \
         NULL, repeat('0', 64))",
    )
    .execute(&pool)
    .await
    .expect_err("a negative sequence is not a sequence");
    assert!(
        refused.to_string().contains("sequence_is_not_negative")
            || refused.to_string().contains("is not the next one"),
        "{refused}"
    );
    finish(pool).await;
}

#[tokio::test]
async fn a_row_this_code_did_not_write_is_a_verdict_rather_than_a_silence() {
    // **A second connection can `INSERT` whatever it likes**: the trigger guards
    // `UPDATE` and `DELETE`, which is the append-only rule, and says nothing
    // about what an `INSERT` may contain. Read naively, a row holding a
    // resource past `MAX_DEPTH` makes `from` fail for the *whole range* — one
    // bad row destroying reads of every other row — and `verify` returns no
    // verdict at all, reported as `Unavailable`, when the store has answered
    // perfectly well.
    //
    // Nothing this crate writes is unreadable, so a row that will not decode
    // arrived by another route. That is what verification is for, and it is
    // what verification says.
    const SCHEMA: &str = "nc_test_foreign_row";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = Journal::new(
        store_in(&pool, "nc_test_foreign_row"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(15).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    journal
        .record(full_record())
        .await
        .expect("the first entry");
    assert!(matches!(journal.verify(0).await, Ok(Ok(_))));

    let mut deep = String::new();
    for _ in 0..200 {
        deep.push_str("{\"a\":");
    }
    deep.push('1');
    for _ in 0..200 {
        deep.push('}');
    }
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (1, 'realm-1', 'attacker', '[]'::jsonb, 'Write', $1, 'allowed', 0, 0, \
         NULL, repeat('0', 64))",
    )
    .bind(&deep)
    .execute(&pool)
    .await
    .expect("a second connection may insert; the trigger only guards UPDATE and DELETE");

    // **And the append is refused, saying which row and why.** It is not
    // asserted that the journal still works: a `Journal::append` falling back
    // to `head()`, which reads the sequence and hash without parsing the
    // record, would chain onto a row this code cannot read.
    //
    // No fallback, because one achieves neither of the two things it is
    // between. It does not keep the journal writable — `head()` parses the
    // *hash column*, and without `0009`'s constraint one `INSERT` with a
    // non-hex hash refuses there too. And where it does work, it hands the next
    // honest entry whatever sixty-four characters the foreign row put in
    // `hash`: measured, *the honest entry chained onto the attacker's chosen
    // hash*.
    //
    // So the choice is refusing or extending a history this code cannot read,
    // and `§14.9` decides it: the audit log fails closed. The refusal names the
    // row, which is what an operator needs to decide what to do about it.
    let refused = journal
        .record(full_record())
        .await
        .expect_err("a history this code cannot read is not one to extend");
    assert!(
        matches!(refused, nc_journal::Error::Undecodable { sequence: 1, .. }),
        "the refusal does not name the row that cannot be read: {refused:?}"
    );

    // And the rows before it are still readable, which failing the whole range
    // on the first bad row would also destroy.
    let read = store_in(&pool, "nc_test_foreign_row")
        .from(0)
        .await
        .expect("the store answers");
    assert_eq!(
        read.entries.len(),
        1,
        "the rows before the bad one were lost"
    );
    let (at, why) = read.stopped_at.expect("the read stopped somewhere");
    assert_eq!(at, 1);
    assert!(why.contains("nests deeper"), "the reason was lost: {why}");
    assert!(
        !why.contains("did not answer"),
        "the store answered, and the message says it did not: {why}"
    );

    // The verdict, not a silence — and it names the row.
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answered, so there is a verdict")
        .expect_err("a row this code did not write is not a clean history");
    assert_eq!(verdict.sequence, 1, "{verdict}");
    assert_eq!(verdict.code(), nc_catalog::ErrorCode::JournalEntryTampered);
    assert!(verdict.to_string().contains("cannot read it"), "{verdict}");
    finish(pool).await;
}

#[tokio::test]
async fn a_record_the_journal_could_not_read_back_is_refused_before_it_is_written() {
    // **One call can permanently destroy the audit log.** `nc_canon` writes a
    // value of any depth; `nc_canon::parse` refuses one past `MAX_DEPTH`. A
    // record whose resource nests past the bound, accepted, hashed and
    // stored, leaves from that moment:
    //
    //   verify(0)        -> Err(Unavailable), at any starting sequence
    //   from(0)          -> Err(… nests deeper than this parser will follow)
    //   append(anything) -> Err(…), because tail() cannot decode the tail
    //   DELETE           -> refused; the table is append-only, by design
    //
    // No reads, no verification, no writes, no repair. And it is one hop from
    // ordinary input: a payload at exactly the limit, which `parse` accepts,
    // wrapped once under a key.
    const SCHEMA: &str = "nc_test_too_deep";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = Journal::new(
        store_in(&pool, "nc_test_too_deep"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(9).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let with = |resource: Value| {
        let mut record = full_record();
        record.resource = resource;
        record
    };

    let mut deep = Value::Integer(1);
    for _ in 0..=nc_canon::parse::MAX_DEPTH {
        deep = Value::object([("a", deep)]);
    }
    let refusal = journal
        .record(with(deep))
        .await
        .expect_err("a record the journal cannot read back is refused");
    assert!(
        matches!(refusal, nc_journal::Error::TooDeep { .. }),
        "{refusal:?}"
    );
    assert!(refusal.to_string().contains("append-only"), "{refusal}");

    // And the journal is untouched: nothing was written, and it still works.
    assert!(
        matches!(journal.verify(0).await, Ok(Ok(_))),
        "the journal moved"
    );
    journal
        .record(with(Value::text("an ordinary one")))
        .await
        .expect("the journal still takes a record");
    assert!(matches!(journal.verify(0).await, Ok(Ok(_))));

    // At the bound it is accepted, so the refusal is a boundary and not a ban.
    let mut at_the_limit = Value::Integer(1);
    for _ in 1..nc_canon::parse::MAX_DEPTH {
        at_the_limit = Value::object([("a", at_the_limit)]);
    }
    journal
        .record(with(at_the_limit))
        .await
        .expect("a record at the bound is readable and therefore storable");
    assert!(matches!(journal.verify(0).await, Ok(Ok(_))));
    finish(pool).await;
}

#[tokio::test]
async fn every_field_survives_the_round_trip_through_a_row() {
    const SCHEMA: &str = "nc_test_round_trip";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_round_trip");

    let written = Entry::chain(0, None, full_record());
    store.append(written.clone()).await.expect("appends");

    let read = store.from(0).await.expect("reads").entries;
    assert_eq!(read.len(), 1);
    // Field for field, because `Entry`'s `PartialEq` is what a wrong column
    // would defeat — and then the hash, because a field that round-tripped into
    // the wrong *place* would still compare equal field by field.
    assert_eq!(read[0], written);
    assert_eq!(read[0].hash, written.hash);
    assert_eq!(read[0].compute_hash(), written.hash);
    finish(pool).await;
}

#[tokio::test]
async fn a_chain_written_to_postgres_verifies() {
    const SCHEMA: &str = "nc_test_chain";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = Journal::new(
        store_in(&pool, "nc_test_chain"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(3).expect("fits in ten bits"),
            SystemClock,
        ),
    );

    {
        let mut unit = journal.begin("a real unit").await.expect("opens");
        for step in 0..5 {
            let mut record = full_record();
            record.resource = Value::Integer(step);
            unit.record(record).await.expect("records");
        }
        unit.commit().await.expect("commits");
    }

    let verdict = journal.verify(0).await.expect("readable");
    assert_eq!(
        verdict,
        Ok(7),
        "an opening entry, five records and a bracket"
    );
    finish(pool).await;
}

#[tokio::test]
async fn the_table_refuses_to_be_edited() {
    const SCHEMA: &str = "nc_test_append_only";
    // **The append-only rule is the table's, not this type's.** A second
    // connection — a console, a migration, a person with `psql` — meets the same
    // refusal, which is the whole reason it is a trigger rather than a check in
    // Rust.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_append_only");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let updated = sqlx::query("UPDATE journal_entries SET action = 'Read' WHERE sequence = 0")
        .execute(&pool)
        .await;
    assert!(updated.is_err(), "the journal accepted an UPDATE");

    let deleted = sqlx::query("DELETE FROM journal_entries WHERE sequence = 0")
        .execute(&pool)
        .await;
    assert!(deleted.is_err(), "the journal accepted a DELETE");

    // **And in replication mode**, which is one statement away from undoing
    // all of it. `CREATE TRIGGER` leaves `tgenabled = 'O'` — origin only — and
    // `SET session_replication_role = replica` is what a logical-replication
    // apply worker sets so an incoming row does not re-fire local triggers. It
    // takes effect immediately and leaves **no catalog trace**, unlike
    // `ALTER TABLE … DISABLE TRIGGER`. Measured without
    // `migrations/0004_the_journal_refuses_replication_mode.sql`: `UPDATE 1`
    // and `DELETE 1` against the shipped triggers, so the audit log can be
    // edited and truncated and `verify` still reports it clean.
    //
    // The role in `DATABASE_URL` is a superuser, which is exactly what
    // `session_replication_role` requires — so this is the application's own
    // connection, not a hypothetical intruder's.
    let updated = in_replication_mode(
        &pool,
        "UPDATE journal_entries SET action = 'Escalate' WHERE sequence = 0",
    )
    .await;
    assert!(
        updated.is_err(),
        "the journal accepted an UPDATE in replication mode"
    );
    let deleted =
        in_replication_mode(&pool, "DELETE FROM journal_entries WHERE sequence = 0").await;
    assert!(
        deleted.is_err(),
        "the journal accepted a DELETE in replication mode"
    );

    // And the row is still there and still verifies, so the refusals above were
    // refusals rather than silent no-ops.
    let read = store.from(0).await.expect("reads").entries;
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].record.action, "Delete");
    finish(pool).await;
}

#[tokio::test]
async fn an_actor_chain_respelled_in_the_column_is_not_read_as_the_same_row() {
    // **The twelfth way through criterion 3, and it names who is answerable.**
    // `actor_chain` is `§14.6`'s delegation chain — the log shows both the agent
    // and the operator. `Entry::hashed_form` hashes the identifiers, `append`
    // stores `serde_json`'s array of them, and `serde_json` accepts `\u0061` for
    // `a`. What kept that from mattering is that the column is `jsonb`, which
    // normalises the text it is given — so the attack is one statement that
    // takes the normalisation away, changing the column's type to `json`, which
    // preserves what it is handed.
    //
    // Measured without the guard: `verify` answers `Ok(4)` both before and
    // after, while `WHERE actor_chain::text LIKE '%agent-1%'` goes from 4 rows
    // to 0. The chain, which is supposed to be the detector, says clean.
    const SCHEMA: &str = "nc_test_actors_respelled";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_actors_respelled");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let canonical: String = sqlx::query_scalar("SELECT actor_chain::text FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");

    // `jsonb` would normalise the escape away, so the type goes with it — which
    // is the whole attack, and needs only table ownership.
    sqlx::query(
        r#"ALTER TABLE journal_entries ALTER COLUMN actor_chain TYPE json
           USING replace(actor_chain::text, '"agent-1"', '"\u0061gent-1"')::json"#,
    )
    .execute(&pool)
    .await
    .expect("a row trigger is not a rewrite hook, which is the point of this test");

    let respelled: String = sqlx::query_scalar("SELECT actor_chain::text FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the bytes");
    assert!(respelled.contains("u0061"), "{respelled}");
    // And the damage: every reader looking for that agent as text finds nothing.
    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE actor_chain::text LIKE '%agent-1%'",
    )
    .fetch_one(&pool)
    .await
    .expect("counts");
    assert_eq!(found, 0, "the fixture did not hide the agent");

    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a respelled actor chain is not the row that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(
        problem.contains("actor_chain"),
        "the error does not name the column: {problem}"
    );
    let verdict = journal(store_in(&pool, "nc_test_actors_respelled"))
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a respelled actor chain is not a clean history");
    assert_eq!(verdict.sequence, 0, "{verdict}");

    finish(pool).await;
}

#[tokio::test]
async fn the_first_of_two_unreadable_rows_is_the_one_named() {
    const SCHEMA: &str = "nc_test_two_unreadable";
    // **An attacker must not choose which row gets investigated, by writing one
    // more.** `verify` reports an unreadable row after the loop precisely so a
    // row poisoned further on cannot hide a forgery before it — but a verdict
    // carrying `tail()`'s sequence, when `tail()` decodes the *last* row, would
    // answer `Broken { sequence: 4 }` with rows 3 and 4 both unreadable, while
    // `verify`'s own doc promises "the **first** entry that does not verify"
    // and `§15.1` asks a diagnostic to point at an exact location.
    //
    // `read.stopped_at` is where the read from `from` stopped, which *is* the
    // first such row.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_two_unreadable");
    let mut previous = None;
    for sequence in 0..3 {
        let entry = Entry::chain(sequence, previous, full_record());
        previous = Some(entry.hash);
        store.append(entry).await.expect("appends");
    }

    // Two rows a second connection wrote, each holding an outcome this code
    // does not know. The order matters: 4 is the tail, 3 is the answer.
    for sequence in [3_i64, 4] {
        sqlx::query(
            "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
             resource, result, recorded_at, hash) \
             VALUES ($1, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', 'null', \
             'quarantined', 1, $2)",
        )
        .bind(sequence)
        .bind("ab".repeat(32))
        .execute(&pool)
        .await
        .expect("the triggers refuse UPDATE, DELETE and TRUNCATE, not INSERT");
    }

    let verdict = journal(store_in(&pool, "nc_test_two_unreadable"))
        .verify(0)
        .await
        .expect("a row this code cannot read is a verdict, not a failure to answer")
        .expect_err("two unreadable rows are not a clean history");
    assert_eq!(
        verdict.sequence, 3,
        "verification named a later unreadable row than the first: {verdict}"
    );
    finish(pool).await;
}

#[tokio::test]
async fn the_table_refuses_to_be_truncated() {
    const SCHEMA: &str = "nc_test_truncate";
    // **The statement that empties the table without firing either trigger.** A
    // row trigger fires per row and `TRUNCATE` removes rows without visiting
    // them, so both of the refusals above are beside the point: measured
    // against migrations 0001-0004, with both triggers already `ENABLE ALWAYS`,
    // `TRUNCATE journal_entries` succeeds and `verify(0)` answers `Ok(0)`.
    //
    // No replication mode and no superuser — table ownership is enough — and
    // the selective form is the same statement with a temp table either side of
    // it, which is an attacker choosing what the audit log remembers.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_truncate");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let emptied = sqlx::query("TRUNCATE journal_entries").execute(&pool).await;
    let refusal = emptied
        .expect_err("the journal accepted a TRUNCATE")
        .to_string();
    assert!(
        refusal.contains("append-only") && refusal.contains("TRUNCATE"),
        "the refusal does not name the statement it refused: {refusal}"
    );

    // And in replication mode, for the reason `0004` gives: `CREATE TRIGGER`
    // leaves `tgenabled = 'O'`, so a new trigger arrives with the hole 0004
    // closed for the other two.
    let emptied = in_replication_mode(&pool, "TRUNCATE journal_entries").await;
    assert!(
        emptied.is_err(),
        "the journal accepted a TRUNCATE in replication mode"
    );

    // The row is still there, so the refusals were refusals.
    assert_eq!(store.from(0).await.expect("reads").entries.len(), 1);
    finish(pool).await;
}

#[tokio::test]
async fn a_row_this_code_cannot_read_stops_the_unit_being_closed_and_says_which_row() {
    const SCHEMA: &str = "nc_test_unit_poisoned";
    // **One `INSERT` must not reopen every closed unit in the journal.** A
    // check that scans `from(0)` stops at the first row it cannot decode —
    // the row `postgres.rs` documents a second connection as free to write.
    // So a poisoned row hides every `UnitCommitted` bracket *behind* it, and
    // an entry naming a committed unit lands after its own closing bracket:
    // inside the group to every SQL reader, with `verify` unable to say so.
    //
    // **The poisoned row goes in before the bracket, and that is the whole
    // fixture.** Written the other way round — the unit committed first, the
    // unreadable row appended after — the scan still sees the bracket and the
    // test passes with the defect present.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, "nc_test_unit_poisoned"));
    let mut group = journal.begin("unit").await.expect("begins");
    group
        .record(entry_record("Read"))
        .await
        .expect("records inside the unit");
    let unit = group.id();

    // One row a second connection wrote, holding an outcome this code does not
    // know — the shape `an_outcome_this_code_does_not_know_is_reported` covers.
    // It lands between the entries of the unit and its closing bracket.
    let next: i64 = sqlx::query_scalar("SELECT max(sequence) + 1 FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("there are rows");
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, hash) \
         VALUES ($1, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', 'null', \
         'quarantined', 1, $2)",
    )
    .bind(next)
    .bind("ab".repeat(32))
    .execute(&pool)
    .await
    .expect("the triggers refuse UPDATE, DELETE and TRUNCATE, not INSERT");

    // **The commit is refused too.** Asserting the bracket lands past the
    // poisoned row would rest on a fallback that hands the next entry the
    // foreign row's own `hash` column. An audit log fails closed (`§14.9`): a
    // history this code cannot read is not one to extend, whatever the entry is
    // for.
    group
        .commit()
        .await
        .expect_err("a bracket does not extend a history this code cannot read");

    // And the unit is still closed *as a question*, which is what this test is
    // named for: the three columns that answer it are `realm`, `work_unit` and
    // `result`, none of them the part of a row that fails to decode. **And the
    // unit is still open, which is the truthful statement.** With the bracket
    // refused, the group is never closed — so what this fixture shows is not
    // that a poisoned row fails to reopen a closed unit but that it stops the
    // unit being closed at all, and says which row. The question itself is
    // still answered from the three columns that do not fail to decode, which
    // is what `unit_closed` reads:
    assert!(
        !store_in(&pool, "nc_test_unit_poisoned")
            .unit_closed(&id("realm-1"), unit)
            .await
            .expect("the store answers"),
        "the bracket was refused, so the unit cannot be closed"
    );
    // And the row is named, so an operator knows what to look at.
    let refused = journal
        .record(entry_record("Delete"))
        .await
        .expect_err("a history this code cannot read is not one to extend");
    assert!(
        matches!(refused, nc_journal::Error::Undecodable { .. }),
        "the refusal does not name the unreadable row: {refused:?}"
    );
    finish(pool).await;
}

/// One entry of `realm`'s under `action`, ending as `result` in `unit`,
/// chained onto the store's head and appended through the store.
///
/// **The head, never the tail**: chaining needs the last sequence and hash,
/// neither of which is the part of a row a second connection poisons.
async fn entered<S: nc_journal::Store>(
    store: &mut S,
    realm: &str,
    action: &str,
    result: Outcome,
    unit: Snowflake,
) {
    let head = store.head().await.expect("the head reads");
    let mut record = entry_record(action);
    record.result = result;
    record.realm = id(realm);
    record.work_unit = Some(unit);
    let sequence = head.map_or(0, |(sequence, _)| sequence + 1);
    store
        .append(Entry::chain(sequence, head.map(|(_, hash)| hash), record))
        .await
        .expect("the history is well formed");
}

/// The units a realm left open are named once each, oldest first, in
/// PostgreSQL too — and a row past which a read of the history stops hides
/// none of them.
///
/// **The in-memory store's case, against the store that runs**, every clause
/// of the two brackets decided by an entry that shares all but one of them,
/// with two rows a second connection wrote: one this code cannot read, in the
/// middle — `from(0)` stops at it, and a recovery that read the history found
/// no unit opened past it — and a second opening bracket of a unit already
/// open, which the store's own `INSERT` refuses and which must not name the
/// unit twice.
#[tokio::test]
async fn the_units_a_realm_left_open_are_named_past_a_row_a_read_stops_at() {
    const SCHEMA: &str = "nc_test_open_units";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    let unit = |text: &str| Snowflake::from_crockford(text).expect("a test unit");
    let (kept, left, lookalike, aborted, unclosed, denied, there) = (
        unit("01J8XK2M9P51"),
        unit("01J8XK2M9P52"),
        unit("01J8XK2M9P53"),
        unit("01J8XK2M9P54"),
        unit("01J8XK2M9P55"),
        unit("01J8XK2M9P56"),
        unit("01J8XK2M9P57"),
    );
    let (opens, closes) = (nc_journal::unit::OPENS, nc_journal::unit::CLOSES);
    for (realm, action, result, unit) in [
        ("realm-1", opens, Outcome::UnitBegun, kept),
        ("realm-1", opens, Outcome::UnitBegun, left),
        ("realm-1", "Write", Outcome::UnitBegun, lookalike),
        ("realm-1", closes, Outcome::UnitCommitted, kept),
    ] {
        entered(&mut store, realm, action, result, unit).await;
    }
    let (poisoned, link) = store.head().await.expect("the head reads").expect("a head");
    let (poisoned, link) = (poisoned + 1, nc_canon::to_hex(&link));
    insert_foreign(&pool, poisoned, &link, "Read", "quarantined", None).await;
    for (realm, action, result, unit) in [
        ("realm-1", opens, Outcome::UnitBegun, aborted),
        ("realm-1", closes, Outcome::UnitAborted, aborted),
        ("realm-1", opens, Outcome::Denied, denied),
        ("realm-2", opens, Outcome::UnitBegun, there),
        ("realm-2", closes, Outcome::UnitCommitted, left),
        ("realm-1", opens, Outcome::UnitBegun, unclosed),
        ("realm-1", "Write", Outcome::UnitCommitted, unclosed),
    ] {
        entered(&mut store, realm, action, result, unit).await;
    }
    let (last, link) = store.head().await.expect("the head reads").expect("a head");
    insert_foreign(
        &pool,
        last + 1,
        &nc_canon::to_hex(&link),
        opens,
        Outcome::UnitBegun.as_str(),
        Some(&left.to_crockford()),
    )
    .await;
    let read = store.from(0).await.expect("the history reads");
    assert_eq!(
        read.stopped_at.map(|(sequence, _)| sequence),
        Some(poisoned),
        "a read of the history stops at the row it cannot decode"
    );
    assert_eq!(
        store.open_units(&id("realm-1")).await,
        Ok(vec![left, unclosed]),
        "what a kernel of realm-1 recovers"
    );
    assert_eq!(store.open_units(&id("realm-2")).await, Ok(vec![there]));
    assert_eq!(store.open_units(&id("realm-3")).await, Ok(Vec::new()));
    finish(pool).await;
}

/// An opening bracket a second connection wrote, naming its unit as this
/// code never writes one, is refused by name when the realm is asked which
/// units are open, and another realm's answer is untouched — in PostgreSQL as
/// in the local journal.
///
/// **Refused, not skipped**, for the reason the local journal's case gives:
/// read leniently, `01j8xk2m9p52` is a unit no question finds by its canonical
/// text, and skipped, a unit a stopped process left might be the one dropped.
/// `journal_entries.work_unit` holds any text, so a party that can only insert
/// can write this row — the residual `AGENTS.md` names beside the closing
/// bracket such a party forges.
#[tokio::test]
async fn an_opening_naming_its_unit_as_this_code_never_writes_one_is_refused_by_name() {
    const SCHEMA: &str = "nc_test_respelled_opening";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    let unit = |text: &str| Snowflake::from_crockford(text).expect("a test unit");
    let (open, there) = (unit("01J8XK2M9P51"), unit("01J8XK2M9P52"));
    let opens = nc_journal::unit::OPENS;
    entered(&mut store, "realm-1", opens, Outcome::UnitBegun, open).await;
    entered(&mut store, "realm-2", opens, Outcome::UnitBegun, there).await;
    let (last, link) = store.head().await.expect("the head reads").expect("a head");
    insert_foreign(
        &pool,
        last + 1,
        &nc_canon::to_hex(&link),
        opens,
        Outcome::UnitBegun.as_str(),
        Some(&there.to_crockford().to_lowercase()),
    )
    .await;
    let refused = store.open_units(&id("realm-1")).await;
    assert!(
        matches!(&refused, Err(nc_journal::Error::Undecodable { sequence, why })
            if *sequence == last + 1
                && why.contains("01j8xk2m9p52")
                && why.contains("not an identifier as this code writes one")),
        "{refused:?}"
    );
    assert_eq!(
        refused.as_ref().map_err(nc_journal::Error::code),
        Err(nc_catalog::ErrorCode::JournalChainBroken)
    );
    assert_eq!(
        store.open_units(&id("realm-2")).await,
        Ok(vec![there]),
        "another realm's units are asked of its own rows"
    );
    finish(pool).await;
}

/// A row a second connection writes, at `sequence` and linked to `link`.
///
/// It opens the unit spelled `unit` when one is named, and is otherwise a row
/// of the outcome `result` — one this code does not know, which no read of it
/// decodes.
async fn insert_foreign(
    pool: &sqlx::PgPool,
    sequence: u64,
    link: &str,
    action: &str,
    result: &str,
    unit: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, work_unit, recorded_at, prev_hash, hash) \
         VALUES ($1, 'realm-1', 'principal-1', '[]'::jsonb, $2, 'null', $3, $4, 1, $5, $6)",
    )
    .bind(i64::try_from(sequence).expect("a short test history"))
    .bind(action)
    .bind(result)
    .bind(unit)
    .bind(link)
    .bind("ab".repeat(32))
    .execute(pool)
    .await
    .expect("the triggers refuse UPDATE, DELETE and TRUNCATE, not INSERT");
}

/// The triggers are there, always enabled, and call a function that refuses.
///
/// **Behaviour alone does not check the append-only guarantee**, because
/// behaviour is checked against the function the triggers call — which is one
/// `CREATE OR REPLACE FUNCTION` away from returning `COALESCE(NEW, OLD)` and
/// letting everything through. That statement needs **function ownership only**:
/// no superuser, no `session_replication_role`, no `DISABLE TRIGGER`, no
/// `DROP TRIGGER`. The role that ran the migrations owns it.
///
/// Afterwards the catalog is exactly what `0005`'s list holds up as proof —
/// three triggers, all present, all `tgenabled = 'A'` — while UPDATE, DELETE
/// and TRUNCATE are all accepted and the table can be emptied, with `verify`
/// then answering `Ok(0)` about an empty history. Measured, without this test.
/// sqlx's checksums hash the migration files and not the database, so
/// they say nothing about it either.
///
/// So this reads the catalog rather than trusting it: the three triggers by
/// name, their enabled state, and the *body* of what they call. It cannot stop
/// an owner — nothing in a database can — but it means the state is checked by
/// something rather than assumed by everything.
#[tokio::test]
async fn the_triggers_are_there_and_call_what_they_were_written_to_call() {
    const SCHEMA: &str = "nc_test_trigger_catalog";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };

    // `tgisinternal` filters out the foreign-key and constraint triggers
    // PostgreSQL creates for itself, which are not this table's rules.
    let triggers: Vec<(String, String)> = sqlx::query_as(
        "SELECT tgname, tgenabled::text FROM pg_trigger \
         WHERE tgrelid = 'journal_entries'::regclass AND NOT tgisinternal \
         ORDER BY tgname",
    )
    .fetch_all(&pool)
    .await
    .expect("the catalog answers");
    assert_eq!(
        triggers,
        vec![
            ("journal_entries_no_delete".to_owned(), "A".to_owned()),
            ("journal_entries_no_truncate".to_owned(), "A".to_owned()),
            ("journal_entries_no_update".to_owned(), "A".to_owned()),
            // **And the one that makes the other three enough.** They stop a
            // row being edited or removed and say nothing about where one may
            // land, so without it a foreign INSERT can put a row at the top of
            // the column's range and every later append is refused for ever. Every
            // `CHECK` has that shape — a bound says which values are legal and
            // nothing about which comes next — so this is a trigger and not a
            // constraint. `ENABLE ALWAYS`, like the others: a replication
            // session is not an exemption.
            (
                "journal_entries_sequence_is_the_next_one".to_owned(),
                "A".to_owned()
            ),
        ],
        "the journal's triggers are not the three it was given, or one is no \
         longer `ENABLE ALWAYS` — `tgenabled = 'O'` is undone by a single \
         `SET session_replication_role = replica`"
    );

    // **And what all three call — the function *these* triggers call.** Asked
    // of `pg_proc` as `proname = 'journal_is_append_only'` with no schema
    // predicate, the question matches every test schema's, and `public`'s.
    // `fetch_one` returns the first row, so the assertion reads somebody else's
    // body while the triggers under test call a tampered one:
    //
    // ```text
    // rows the unqualified query matches: 2
    //   public       RAISE EXCEPTION 'the journal is append-only: % on % …'
    //   the schema   IF TG_OP = 'UPDATE' THEN RAISE …; END IF; RETURN COALESCE(…)
    // what fetch_one returned: public's
    // DELETE refused: false   TRUNCATE refused: false   rows left: 0
    // ```
    //
    // Resolved through the trigger itself — `tgfoid` is the function this
    // table's trigger actually calls, whatever it is named and wherever it
    // lives — so there is no name left to be ambiguous about.
    let body: String = sqlx::query_scalar(
        "SELECT p.prosrc FROM pg_trigger t JOIN pg_proc p ON p.oid = t.tgfoid \
         WHERE t.tgrelid = 'journal_entries'::regclass AND NOT t.tgisinternal \
         AND t.tgname = 'journal_entries_no_update'",
    )
    .fetch_one(&pool)
    .await
    .expect("the function is there");
    assert!(
        body.contains("RAISE EXCEPTION"),
        "the function every trigger on the journal calls no longer raises: {body}"
    );
    assert!(
        body.contains("the journal is append-only"),
        "the function no longer says why it refused: {body}"
    );

    // And all three *refusal* triggers call the same function, or reading one
    // body says nothing about the other two. The density trigger is deliberately
    // its own: it answers a different question — not "may this row change" but
    // "may this row land here" — and folding it in would make one body say two
    // things.
    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT p.prosrc FROM pg_trigger t JOIN pg_proc p ON p.oid = t.tgfoid \
         WHERE t.tgrelid = 'journal_entries'::regclass AND NOT t.tgisinternal \
         AND t.tgname <> 'journal_entries_sequence_is_the_next_one'",
    )
    .fetch_all(&pool)
    .await
    .expect("the catalog answers");
    assert_eq!(
        bodies.len(),
        1,
        "the journal's refusal triggers no longer call one function, so reading one body \
         says nothing about the others: {bodies:#?}"
    );

    // And the density trigger says why it refused, in its own words.
    let dense: String = sqlx::query_scalar(
        "SELECT p.prosrc FROM pg_trigger t JOIN pg_proc p ON p.oid = t.tgfoid \
         WHERE t.tgrelid = 'journal_entries'::regclass \
         AND t.tgname = 'journal_entries_sequence_is_the_next_one'",
    )
    .fetch_one(&pool)
    .await
    .expect("the catalog answers");
    assert!(
        dense.contains("is not the next one"),
        "the density trigger no longer says why it refused: {dense}"
    );

    // The behaviour, in the same test, so the two can be read together: this is
    // what the catalog above is the explanation for.
    //
    // **All three statements, not only the one.** The substring assertions
    // above are satisfied by a body that raises on `UPDATE` and returns for
    // everything else, and trying `UPDATE` alone would leave the test whose doc
    // says it reads the body rather than assuming it blind to exactly the route
    // it names. Measured, with the migration's `RAISE` wrapped in
    // `IF TG_OP = 'UPDATE'` and `UPDATE` alone tried: this test passes while
    // `the_table_refuses_to_be_edited` and `the_table_refuses_to_be_truncated`
    // fail.
    let mut store = store_in(&pool, "nc_test_trigger_catalog");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");
    for statement in [
        "UPDATE journal_entries SET action = 'Escalate'",
        "DELETE FROM journal_entries",
        "TRUNCATE journal_entries",
    ] {
        let refused = sqlx::query(statement).execute(&pool).await;
        assert!(refused.is_err(), "the journal accepted `{statement}`");
    }
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("counts");
    assert_eq!(left, 1, "the row this test wrote is gone");

    finish(pool).await;
}

/// A schema name that survives PostgreSQL's 63-byte truncation with its pid intact.
///
/// **A pid placed last is what truncation eats.** `NAMEDATALEN` is 64, so an
/// identifier is silently cut at 63 bytes — and
/// `nc_test_two_stores_an_entry_after_its_own_bracket,_at_the_wrong_sequence_1059300`
/// is 80, cut to `…_at_the_wrong`. Two concurrent runs then build the *same*
/// schema, and `a_schema_of_its_own`'s `DROP SCHEMA … CASCADE` destroys the
/// other's between its `CREATE SCHEMA` and its migration:
///
/// ```text
/// the migrations apply: PgDatabaseError { code: "3F000",
///   message: "no schema has been selected to create in" }
/// ```
///
/// Measured with the pid last: **100 % of paired runs started together**,
/// about 10 % with a stagger. `pool()`'s own paragraph puts the pid in the name
/// "so two runs do not destroy each other", and with the pid last that is
/// exactly what two runs do, to any name long enough to lose it.
///
/// So the pid goes directly after the prefix, where truncation cannot reach it,
/// and the label — which is only for a person reading `\dn` — is what gets cut.
/// The cut is made here rather than left to the server, so the name this code
/// holds is the name the server holds.
fn a_schema_named(prefix: &str, label: &str) -> String {
    // PostgreSQL's `NAMEDATALEN` is 64 and an identifier is truncated to 63
    // bytes. `identifier` in `nc-prune` names the same number for the same
    // reason.
    const LONGEST: usize = 63;
    let head = format!("{prefix}_{}_", std::process::id());
    assert!(
        head.len() < LONGEST,
        "`{head}` is already {} bytes, so no label can be added and the pid itself \
         would be cut",
        head.len()
    );
    let tail: String = label
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    let mut name = format!("{head}{tail}");
    name.truncate(LONGEST);
    // The floor: whatever was cut, the pid is still in it — which is the whole
    // point of the ordering above.
    assert!(
        name.contains(&std::process::id().to_string()),
        "`{name}` lost its process id to truncation, so two concurrent runs share it"
    );
    name
}

/// A schema with the migrations applied, and a pool pinned to it.
///
/// **Dropped first.** `CREATE SCHEMA IF NOT EXISTS` would leave the previous
/// run's rows in place, so the second run would find a bracket already
/// committed and the two stores disagree at step 0 — a test that passes once
/// and fails afterwards is worse than one that never passed.
async fn a_schema_of_its_own(pool: &sqlx::PgPool, name: &str) -> sqlx::PgPool {
    // **Dropped first.** `IF NOT EXISTS` would leave the previous run's rows in
    // place, so the second run would find a bracket already committed and the
    // two stores disagree at step 0 — a test that passes once and fails
    // afterwards is worse than one that never passed.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS \"{name}\" CASCADE"
    )))
    .execute(pool)
    .await
    .expect("a schema of its own");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA \"{name}\"")))
        .execute(pool)
        .await
        .expect("a schema of its own");
    let scope = name.to_owned();
    let scoped = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let scope = scope.clone();
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "SET search_path TO \"{scope}\""
                )))
                .execute(&mut *connection)
                .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DATABASE_URL").expect("set"))
        .await
        .expect("connects");
    sqlx::migrate!("../../migrations")
        .run(&scoped)
        .await
        .expect("the migrations apply");

    scoped
}

/// One step of a script: what the entry says, whether it names a unit, the
/// sequence to chain it at, the realm it is written in, and the action it
/// carries when that is not the one its outcome implies.
///
/// **The last two reach two of the three clauses either store's `unit_opened`
/// asks.** Without them, every step writes `realm-1` and every `UnitBegun` step
/// carries `OPENS`, so the script reaches neither a second realm nor a
/// non-bracket entry with a bracket's outcome — and the PostgreSQL predicate's
/// `realm`, `action` and `result` comparisons can each be replaced by
/// `OR TRUE` with `cargo xtask check` green. The trait's whole
/// premise is that both implementations answer one call the same way, and this
/// is the test that holds them to it.
type Step = (Outcome, Option<Snowflake>, u64, &'static str, &'static str);

/// The bracket questions, asked of both stores and compared.
///
/// **The loop that drives `Store::append` never consults them**, so without
/// this `unit_opened` and `unit_closed` are outside the two-store comparison
/// entirely — and each of the PostgreSQL predicate's three comparisons can be
/// replaced by `OR TRUE` with the whole gate green. They are asked here, in both
/// realms the scripts write and for a unit no script mentions, so a store that
/// ignores the realm, the action or the outcome answers differently from the one
/// that does not.
///
/// **And `unit_closing`, which is what each store implements.** `unit_closed`
/// is provided over it, so comparing only the `bool` would let one store say a
/// unit rolled back when the other says it committed, which are the two answers
/// a caller acts on differently. The scripts close units both ways.
///
/// Its own function because inline it takes
/// `both_stores_answer_the_same_script_the_same_way` past clippy's hundred-line
/// bound.
async fn the_two_stores_agree_about_the_brackets(
    memory: &nc_journal::InMemory,
    postgres: &Postgres,
    unit: Snowflake,
    name: &str,
) {
    let elsewhere = Snowflake::from_crockford("01J8XK2M9P59").expect("a unit");
    for (which, asked) in [("realm-1", unit), ("realm-2", unit), ("realm-1", elsewhere)] {
        let realm = id(which);
        assert_eq!(
            memory.unit_opened(&realm, asked).await.expect("readable"),
            postgres.unit_opened(&realm, asked).await.expect("readable"),
            "`{name}`: the two stores disagree about whether {asked} is open in \
             {which}, and `Journal::append` refuses on the strength of that answer"
        );
        assert_eq!(
            memory.unit_closed(&realm, asked).await.expect("readable"),
            postgres.unit_closed(&realm, asked).await.expect("readable"),
            "`{name}`: the two stores disagree about whether {asked} is closed in {which}"
        );
        assert_eq!(
            memory.unit_closing(&realm, asked).await.expect("readable"),
            postgres
                .unit_closing(&realm, asked)
                .await
                .expect("readable"),
            "`{name}`: the two stores disagree about how {asked} closed in {which}"
        );
    }
    // **And which units are open, in both realms the scripts write**: a
    // kernel opening over either store recovers what this names, newest first,
    // so the list and its order are the answer.
    for which in ["realm-1", "realm-2"] {
        let realm = id(which);
        assert_eq!(
            memory.open_units(&realm).await.expect("readable"),
            postgres.open_units(&realm).await.expect("readable"),
            "`{name}`: the two stores disagree about which units are open in {which}, \
             and a kernel opening recovers what that answer names"
        );
    }
}

/// The scripts both stores are driven through, written out rather than derived.
///
/// A step that starts failing is a step somebody has to look at, which a
/// generated set would not give. Its own function because inline the scripts
/// take the test past clippy's hundred-line bound — among them a second realm,
/// an ordinary entry carrying a bracket's outcome, and a begin that was denied.
/// <!-- allow-logging: `the_scripts_both_stores_answer` hands back script, realm and action names written as literals in this file, and nothing a caller chose -->
fn the_scripts_both_stores_answer(unit: Snowflake) -> Vec<(String, Vec<Step>)> {
    let mut scripts = vec![
        (
            "an entry after its own bracket, at the right sequence".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitCommitted, Some(unit), 1, "realm-1", ""),
                (Outcome::Allowed, Some(unit), 2, "realm-1", ""),
            ],
        ),
        (
            "an entry after its own bracket, at the wrong sequence".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitCommitted, Some(unit), 1, "realm-1", ""),
                (Outcome::Allowed, Some(unit), 9, "realm-1", ""),
            ],
        ),
        (
            "out of order inside a unit that is still open".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::Allowed, Some(unit), 7, "realm-1", ""),
            ],
        ),
        // **A unit that was rolled back.** Without this row
        // `Outcome::UnitAborted` appears in no script. Both stores read a
        // closing bracket as *committed or aborted*, and both halves can then be
        // narrowed to committed-only with the whole gate green — so
        // `Store::unit_closing` (and `unit_closed` over it), which is public
        // and is what the kernel asks, answers "still open" for a unit
        // that has ended.
        (
            "a unit closed by being rolled back".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitAborted, Some(unit), 1, "realm-1", ""),
                (Outcome::Allowed, Some(unit), 2, "realm-1", ""),
            ],
        ),
        // **A bracket's outcome under an action that is not the bracket**, with
        // a closing result. `unit_closing`'s `action = $3` clause — the one its
        // own doc argues for at length, because "when did this unit end" must
        // have one answer — is decided by this: without it `AND $3 = $3` leaves
        // the gate green. An ordinary `Write` carrying `UnitCommitted` must not
        // close the unit, or `Journal::append` refuses the unit's rightful
        // owner.
        (
            "a bracket's outcome on an ordinary action".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitCommitted, Some(unit), 1, "realm-1", "Write"),
                (Outcome::Allowed, Some(unit), 2, "realm-1", ""),
            ],
        ),
        (
            "a first entry at a sequence that is not zero".to_owned(),
            vec![(Outcome::Allowed, None, 7, "realm-1", "")],
        ),
        (
            "a sequence PostgreSQL cannot hold".to_owned(),
            vec![(Outcome::Allowed, None, u64::MAX, "realm-1", "")],
        ),
        (
            "another unit's entry after this one's bracket".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitCommitted, Some(unit), 1, "realm-1", ""),
                (
                    Outcome::Allowed,
                    Some(Snowflake::from_crockford("01J8XK2M9P52").expect("a unit")),
                    2,
                    "realm-1",
                    "",
                ),
            ],
        ),
        // **A second realm**, which every step before this one lacked: the
        // bracket is opened in `realm-1` and the entry is written in `realm-2`,
        // so a store that ignores the realm reads the unit as open and one that
        // reads it does not.
        (
            "a unit opened in one realm, an entry written in another".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::Allowed, Some(unit), 1, "realm-2", ""),
            ],
        ),
        // **A bracket's outcome on an entry that is not one.** `action` is what
        // makes a bracket a bracket, and no step can say so while the action
        // is derived from the outcome.
        (
            "an ordinary entry carrying a bracket's outcome".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", "Write"),
                (Outcome::Allowed, Some(unit), 1, "realm-1", ""),
            ],
        ),
        // And its mirror: a bracket's action with an ordinary outcome, which is
        // the other of the two clauses.
        (
            "a begin that was denied".to_owned(),
            vec![
                (Outcome::Denied, Some(unit), 0, "realm-1", "kernel.begin"),
                (Outcome::Allowed, Some(unit), 1, "realm-1", ""),
            ],
        ),
    ];
    scripts.extend(the_scripts_about_the_opener_guard(unit));
    scripts.extend(the_scripts_about_the_closed_unit_guard(unit));
    scripts
}

/// The scripts about the guard that decides whether a unit may be opened.
///
/// Split from [`the_scripts_both_stores_answer`] because together they are
/// past clippy's hundred-line bound, and that function is itself split from
/// the test for the same reason, as its doc says. The division is by subject: there,
/// what happens around a bracket already in the history; here and in
/// [`the_scripts_about_the_closed_unit_guard`], the two guards that decide
/// whether a bracket may be written at all.
/// <!-- allow-logging: `the_scripts_about_the_opener_guard` hands back script, realm and action names written as literals in this file, and nothing a caller chose -->
fn the_scripts_about_the_opener_guard(unit: Snowflake) -> Vec<(String, Vec<Step>)> {
    vec![
        // **Seven scripts about the opener guard, which is written four
        // times.** The `InMemory` copy's own tests say in as many words that
        // the shape is *"what the store that runs accepts"* — and this asks the
        // store that runs. Measured without these, fifteen clauses of the other
        // three copies deleted one at a time, each against the whole crate
        // suite with a database up: **every one survives**, and two of them
        // survive `cargo xtask check` with the console byte-identical.
        //
        // Every step below is *accepted* except the one marked, and each one
        // separates a clause: the guard fires only for an entry that is an
        // opening bracket, of this unit, in this realm, and only when one is
        // already open.
        //
        // What they cost when the clause is gone is not a wrong sentence but a
        // write that never lands: the `INSERT` refuses, the diagnosis is a
        // second query whose own clauses are intact, it finds nothing wrong, and
        // the caller is told `Contended` — *retryable* — for a write no retry
        // will ever complete. `§15.5` says a denial is recorded "not optional
        // and not sampled".
        (
            "a second opener, in another realm".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitBegun, Some(unit), 1, "realm-2", ""),
            ],
        ),
        (
            "another unit opened while one is open".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (
                    Outcome::UnitBegun,
                    Some(Snowflake::from_crockford("01J8XK2M9P52").expect("a unit")),
                    1,
                    "realm-1",
                    "",
                ),
            ],
        ),
        (
            "an ordinary action carrying the opener's outcome, inside an open unit".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", "Write"),
            ],
        ),
        (
            "a begin that was denied, inside an open unit".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::Denied, Some(unit), 1, "realm-1", "kernel.begin"),
            ],
        ),
        (
            "a close that was denied does not close the unit".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::Denied, Some(unit), 1, "realm-1", "kernel.unit"),
                (Outcome::Allowed, Some(unit), 2, "realm-1", ""),
            ],
        ),
        // **The one that must be refused**, so the six above are not six ways of
        // saying the guard never fires.
        (
            "a genuine second opener for one unit in one realm".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", ""),
            ],
        ),
        // **And at a sequence that is wrong**, which is the only route into the
        // diagnosis: the `INSERT` refuses on ordering, and the second query is
        // what decides whether the caller hears *out of order* or *already
        // open*. Its three clauses about the opener are decided here.
        (
            "another unit opened at the wrong sequence while one is open".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (
                    Outcome::UnitBegun,
                    Some(Snowflake::from_crockford("01J8XK2M9P52").expect("a unit")),
                    9,
                    "realm-1",
                    "",
                ),
            ],
        ),
        // **A near miss in the table, then a genuine opener at a wrong
        // sequence.** The diagnosis asks whether an opener for this unit exists
        // by *its* action and *its* result, and without this each of those two
        // clauses is decided by nothing: with either gone, an entry that is not
        // an opener answers the question, and a caller whose only fault is the
        // sequence is told `UnitAlreadyOpen` — which is not retryable — for a
        // unit nobody has opened. A wrong sequence is the only route into that
        // query, so the near miss has to be written first and the opener has to
        // be second.
        (
            "a genuine opener at a wrong sequence, after an ordinary action carrying its outcome"
                .to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", "Write"),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        (
            "a genuine opener at a wrong sequence, after a begin that was denied".to_owned(),
            vec![
                (Outcome::Denied, Some(unit), 0, "realm-1", "kernel.begin"),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        // And the closed half of the same query, which asks by the closing
        // action and a list of two outcomes.
        (
            "a genuine opener at a wrong sequence, after an ordinary action carrying a close"
                .to_owned(),
            vec![
                (Outcome::UnitCommitted, Some(unit), 0, "realm-1", "Write"),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
    ]
}

/// The scripts about the guard that decides whether a unit is already closed.
///
/// The other half of [`the_scripts_about_the_opener_guard`], and its own
/// function for the same reason that one is: the pair together is past
/// clippy's hundred-line bound.
/// <!-- allow-logging: `the_scripts_about_the_closed_unit_guard` hands back script, realm and action names written as literals in this file, and nothing a caller chose -->
fn the_scripts_about_the_closed_unit_guard(unit: Snowflake) -> Vec<(String, Vec<Step>)> {
    vec![
        // **The closed-unit half of both guards, clause by clause.** The four
        // above reach the opener half; these reach the one asked beside it, and
        // the two halves are written in both the `INSERT`'s `WHERE` and the
        // diagnosis. A widened clause here does not report a wrong sentence: it
        // refuses a write that is entirely legitimate, and the caller is told a
        // unit is closed that nobody closed.
        //
        // The first is accepted and the rest are refused for their sequence, so
        // that the answer comes from the diagnosis rather than from the
        // `INSERT`'s own `WHERE`. Those are the only two routes into it.
        (
            "a unit closed in another realm, then opened here".to_owned(),
            vec![
                (Outcome::UnitCommitted, Some(unit), 0, "realm-2", ""),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", ""),
            ],
        ),
        (
            "a unit closed in another realm, then opened here at the wrong sequence".to_owned(),
            vec![
                (Outcome::UnitCommitted, Some(unit), 0, "realm-2", ""),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        (
            "another unit closed here, then this one opened at the wrong sequence".to_owned(),
            vec![
                (
                    Outcome::UnitCommitted,
                    Some(Snowflake::from_crockford("01J8XK2M9P52").expect("a unit")),
                    0,
                    "realm-1",
                    "",
                ),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        (
            "a close that was denied, then a genuine opener at the wrong sequence".to_owned(),
            vec![
                (Outcome::Denied, Some(unit), 0, "realm-1", "kernel.unit"),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        (
            "a unit opened in another realm, then opened here at the wrong sequence".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-2", ""),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", ""),
            ],
        ),
        // And the two that reach the `INSERT`'s own opener clause, which is
        // asked only of an entry that is itself an opening bracket: an entry in
        // the unit that is not one, and a bracket's action that was denied. Both
        // are opened at the sequence that follows, so both must land.
        (
            "an ordinary entry in a unit, then the unit opened".to_owned(),
            vec![
                (Outcome::Allowed, Some(unit), 0, "realm-1", "Write"),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", ""),
            ],
        ),
        (
            "a begin that was denied, then a genuine opener".to_owned(),
            vec![
                (Outcome::Denied, Some(unit), 0, "realm-1", "kernel.begin"),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", ""),
            ],
        ),
        // The mirror of the one above, and the last clause of the sixteen to be
        // decided by anything: the outcome is the opener's and the action is
        // not. Only an entry carrying *both* has opened the unit, and the two
        // scripts have to be written separately because either one alone leaves
        // the other clause free.
        (
            "an ordinary action carrying the opener's outcome, then a genuine opener".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", "Write"),
                (Outcome::UnitBegun, Some(unit), 1, "realm-1", ""),
            ],
        ),
        // **And the near miss as the entry being diagnosed**, which is the one
        // shape the four above cannot express: each of them writes the near
        // miss first, at the sequence that follows, so it lands and the
        // diagnosis never sees it. The diagnosis asks whether the record in its
        // hands is an opening bracket before it consults either permanent
        // answer, and without this both clauses of that question are decided by
        // nothing — deleted one at a time, `check passed` with no warn.
        //
        // What it costs is the same sentence in both directions: an entry whose
        // only fault is its sequence is told `UnitAlreadyOpen`, which is not
        // retryable, so a caller that would have succeeded on a retry stops for
        // good. A denied `kernel.begin` opens nothing and an ordinary `Write`
        // is not a bracket whatever outcome it carries.
        (
            "a begin that was denied, at a wrong sequence, while the unit is open".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::Denied, Some(unit), 9, "realm-1", "kernel.begin"),
            ],
        ),
        (
            "an ordinary action carrying the opener's outcome, at a wrong sequence, while the \
             unit is open"
                .to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitBegun, Some(unit), 9, "realm-1", "Write"),
            ],
        ),
        // The same pair against the other permanent answer, so neither clause is
        // held by one half of the question.
        (
            "a begin that was denied, at a wrong sequence, after the unit was closed".to_owned(),
            vec![
                (Outcome::UnitBegun, Some(unit), 0, "realm-1", ""),
                (Outcome::UnitCommitted, Some(unit), 1, "realm-1", ""),
                (Outcome::Denied, Some(unit), 9, "realm-1", "kernel.begin"),
            ],
        ),
    ]
}

#[tokio::test]
/// One script, both stores, entry for entry — including which refusal — and
/// what each then says about every unit's brackets, which units are open
/// among them.
///
/// **The comparison the "both stores answer alike" argument needs.** With each
/// store tested on its own and nothing driving one sequence through both, the
/// two can drift in the one place the argument is about: an entry that is
/// *both* out of order and inside a closed unit could be `OutOfOrder` in memory
/// and `UnitClosed` in PostgreSQL, the two resolving the refusals in opposite
/// orders. Those map to different error codes, so for one call the cheap store
/// would tell a responder the hash chain was broken and the real store would
/// tell them a work unit was closed — and the cheap store is what criterion 3's
/// every-position proof runs on.
///
/// A sequence past `i64::MAX` was the same shape one field along: `OutOfOrder`
/// in memory, `Unstorable` in PostgreSQL.
async fn both_stores_answer_the_same_script_the_same_way() {
    const SCHEMA: &str = "nc_test_two_stores";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Snowflake::from_crockford("01J8XK2M9P51").expect("a valid unit");

    // Each step is a record, the sequence to chain it at, and whether it should
    // land — written out rather than derived, so a step that starts failing is
    // a step somebody has to look at.
    let scripts = the_scripts_both_stores_answer(unit);

    for (name, script) in scripts {
        // A schema of this script's own, so the two stores each start empty.
        let fresh = a_schema_named(SCHEMA, &name);
        let scoped = a_schema_of_its_own(&pool, &fresh).await;
        let mut memory = nc_journal::InMemory::default();
        let mut postgres = Postgres::in_schema(scoped.clone(), &fresh);
        for (step, (result, in_unit, sequence, realm, action)) in script.iter().enumerate() {
            // **A bracket carries the bracket's action.** A script writing
            // `Read` for every step, including the `UnitCommitted` one, closes
            // nothing, because `unit_closed` asks for `kernel.unit` — which is
            // the rule working: an ordinary entry carrying a bracket's
            // *outcome* is not a bracket. The script is about what happens
            // after a real one.
            let mut record = entry_record(if action.is_empty() {
                match result {
                    Outcome::UnitBegun => nc_journal::unit::OPENS,
                    Outcome::UnitCommitted | Outcome::UnitAborted => nc_journal::unit::CLOSES,
                    _ => "Read",
                }
            } else {
                action
            });
            record.result = *result;
            record.work_unit = *in_unit;
            record.realm = id(realm);

            let in_memory = memory.tail().await.expect("readable").map(|tail| tail.hash);
            let in_postgres = postgres
                .tail()
                .await
                .expect("readable")
                .map(|tail| tail.hash);
            let one = memory
                .append(Entry::chain(*sequence, in_memory, record.clone()))
                .await;
            let other = postgres
                .append(Entry::chain(*sequence, in_postgres, record))
                .await;
            assert_eq!(
                one, other,
                "`{name}` step {step}: the two stores answered differently, and \
                 `Error::code()` maps these to different error codes — so one of them \
                 sends a responder to the wrong place"
            );
        }
        the_two_stores_agree_about_the_brackets(&memory, &postgres, unit, &name).await;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA \"{fresh}\" CASCADE"
        )))
        .execute(&pool)
        .await
        .expect("its schema is dropped");
    }

    finish(pool).await;
}

/// A clock that reads one instant, so two minters on one flake mint the same
/// identifiers.
///
/// The two journals below are compared answer for answer, and an answer names
/// its unit — so the units have to be the same units. Nothing else in either
/// journal reads this clock: a bracket's instant comes from the system clock,
/// and the comparison leaves `recorded_at` out.
#[derive(Debug, Clone, Copy)]
struct Stopped;

impl nc_ids::Clock for Stopped {
    fn now_ms(&self) -> u64 {
        // 2026-09-21T14:13:20Z (`date -u -d @1790000000`): inside the
        // identifier layout's range.
        1_790_000_000_000
    }

    fn wait_past(&self, _millis: u64) {}
}

/// What a caller can see of one call's answer: the error, or the closing
/// bracket's action, outcome, resource, unit and principal.
///
/// `recorded_at` is the one field left out, and with it the hash and the link:
/// it is the system clock's, and the two journals ask it at different moments.
fn what_a_caller_sees(answer: &Result<Entry, nc_journal::Error>) -> String {
    match answer {
        Ok(entry) => format!(
            "{} {:?} {} {:?} {}",
            entry.record.action,
            entry.record.result,
            entry.record.resource.to_canonical(),
            entry.record.work_unit,
            entry.record.principal
        ),
        Err(refused) => format!("{refused:?}"),
    }
}

/// Two openings neither of the journals below wrote through `open_unit`.
///
/// `by_hand` through `Journal::record`, so it is in this realm and the journal
/// holds no label for it; `foreign` through the store, because `record` stamps
/// the journal's own realm and this one is in `realm-2`, where it opens nothing
/// the journal can close.
async fn opened_by_hand<S: nc_journal::Store>(
    journal: &mut Journal<S, Stopped>,
    by_hand: Snowflake,
    foreign: Snowflake,
) {
    let mut opening = entry_record(nc_journal::unit::OPENS);
    opening.result = Outcome::UnitBegun;
    opening.work_unit = Some(by_hand);
    let mut elsewhere = opening.clone();
    elsewhere.work_unit = Some(foreign);
    elsewhere.realm = id("realm-2");
    journal
        .record(opening)
        .await
        .expect("a hand-written opening lands");
    let (sequence, hash) = journal
        .store()
        .head()
        .await
        .expect("readable")
        .expect("a head");
    journal
        .store_mut()
        .append(Entry::chain(sequence + 1, Some(hash), elsewhere))
        .await
        .expect("another realm's opening lands");
}

/// Both journals answer `open_unit` and `close_unit` the same way, call for
/// call — and both stores then say the same thing about how each unit closed.
///
/// **The scripts above drive `Store::append`, and these two calls are the
/// `Journal`'s.** `close_unit` asks `unit_opened` before it writes and
/// `append` asks `unit_closed` after, so each store's answers decide what a
/// caller is told: a store that read a foreign realm's opening as this one's
/// would let a close through that the other refuses as never opened.
#[tokio::test]
async fn both_journals_open_and_close_units_the_same_way() {
    const SCHEMA: &str = "nc_test_two_journals";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let minter = || Minter::new(MachineFlake::from_value(23).expect("a flake"), Stopped);
    let mut memory = Journal::new(
        nc_journal::InMemory::default(),
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    let mut postgres = Journal::new(
        store_in(&pool, SCHEMA),
        id("realm-1"),
        id("kernel"),
        minter(),
    );
    let never = Snowflake::from_crockford("01J8XK2M9P65").expect("a unit");
    let by_hand = Snowflake::from_crockford("01J8XK2M9P66").expect("a unit");
    let foreign = Snowflake::from_crockford("01J8XK2M9P67").expect("a unit");

    let first = memory.open_unit("first").await.expect("opens");
    assert_eq!(
        postgres.open_unit("first").await,
        Ok(first),
        "the two minters did not mint the same unit, so nothing below compares"
    );
    let second = memory.open_unit("second").await.expect("opens");
    assert_eq!(postgres.open_unit("second").await, Ok(second));
    opened_by_hand(&mut memory, by_hand, foreign).await;
    opened_by_hand(&mut postgres, by_hand, foreign).await;

    // And what each call must answer, so the two cannot agree by both being
    // wrong: `None` is a close that lands.
    let never_opened = |unit| Some(nc_journal::Error::UnitNeverOpened { unit });
    let closed = |unit| Some(nc_journal::Error::UnitClosed { unit });
    let calls = [
        (never, Closing::Committed, never_opened(never)),
        (first, Closing::Committed, None),
        (first, Closing::Aborted, closed(first)),
        (second, Closing::Aborted, None),
        (second, Closing::Committed, closed(second)),
        (by_hand, Closing::Committed, None),
        (foreign, Closing::Committed, never_opened(foreign)),
    ];
    for (step, (unit, how, refusal)) in calls.into_iter().enumerate() {
        let one = memory.close_unit(unit, how).await;
        let other = postgres.close_unit(unit, how).await;
        assert_eq!(
            what_a_caller_sees(&one),
            what_a_caller_sees(&other),
            "step {step}, `close_unit({unit}, {how:?})`: the two journals answered differently"
        );
        assert_eq!(
            one.err(),
            refusal,
            "step {step}, `close_unit({unit}, {how:?})`: not the answer the call earns"
        );
    }
    for (unit, expected) in [
        (first, Some(Outcome::UnitCommitted)),
        (second, Some(Outcome::UnitAborted)),
        (by_hand, Some(Outcome::UnitCommitted)),
        (never, None),
        (foreign, None),
    ] {
        for realm in ["realm-1", "realm-2"] {
            let here = if realm == "realm-1" { expected } else { None };
            assert_eq!(
                memory.store().unit_closing(&id(realm), unit).await,
                Ok(here),
                "in memory, {unit} in {realm}"
            );
            assert_eq!(
                postgres.store().unit_closing(&id(realm), unit).await,
                Ok(here),
                "in PostgreSQL, {unit} in {realm}"
            );
        }
    }
    assert_eq!(
        postgres.verify(0).await.expect("readable"),
        memory.verify(0).await.expect("readable"),
        "the two histories are not the same length, or one does not verify"
    );

    finish(pool).await;
}

/// How a unit closed is its first closing bracket, not one a second connection
/// wrote around the store afterwards.
///
/// **`ORDER BY sequence`, decided.** `append`'s `INSERT` refuses a second
/// closing bracket, so every other test here holds one, and without this `DESC`
/// in `unit_closing`'s query leaves every suite green — measured alongside the
/// in-memory scan taking its last match. A plain `INSERT` from another
/// connection is not refused: nothing in the table's constraints knows what a
/// bracket is. Under `DESC`, a unit rolled back and then "committed" that way
/// reads as committed, which is the one answer of the two that says its effects
/// stand.
#[tokio::test]
async fn how_a_unit_closed_is_its_first_closing_bracket_not_one_written_around_the_store() {
    const SCHEMA: &str = "nc_test_first_closing_bracket";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, SCHEMA));
    let unit = journal.open_unit("rolled back").await.expect("opens");
    journal
        .close_unit(unit, Closing::Aborted)
        .await
        .expect("closes");
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, work_unit, recorded_at, hash) VALUES (2, 'realm-1', 'p', \
         '[]'::jsonb, $1, 'null', 'unit-committed', $2, 1, repeat('a', 64))",
    )
    .bind(nc_journal::unit::CLOSES)
    .bind(unit.to_crockford())
    .execute(&pool)
    .await
    .expect("a second connection can insert a closing bracket the store would refuse");
    assert_eq!(
        journal.store().unit_closing(&id("realm-1"), unit).await,
        Ok(Some(Outcome::UnitAborted)),
        "a closing bracket written around the store changed how the unit closed, so a \
         rolled-back unit reads as one that stands"
    );
    finish(pool).await;
}

/// A party that can only insert chooses how a unit that is still open closed,
/// in PostgreSQL too.
///
/// The twin of `unit.rs`'s `a_party_that_can_only_append_chooses_how_an_open_unit_closed`,
/// against the store that runs. The forged bracket goes in through a second
/// store over the same table — another connection, stating the link the
/// `INSERT`'s `WHERE` checks — and nothing in that statement asks who wrote
/// the bracket, only whether the unit is still open. The first-bracket rule
/// then keeps the forger's answer rather than the owner's.
#[tokio::test]
async fn a_party_that_can_only_insert_chooses_how_an_open_unit_closed_in_postgres_too() {
    const SCHEMA: &str = "nc_test_forged_first_bracket";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, SCHEMA));
    let mut forger = store_in(&pool, SCHEMA);
    let unit = journal.open_unit("Import album").await.expect("opens");
    let (sequence, hash) = forger.head().await.expect("readable").expect("a head");
    let mut closing = entry_record(nc_journal::unit::CLOSES);
    closing.realm = id("realm-1");
    closing.principal = id("intruder");
    closing.result = Outcome::UnitAborted;
    closing.work_unit = Some(unit);
    forger
        .append(Entry::chain(sequence + 1, Some(hash), closing))
        .await
        .expect("a second connection chains a closing bracket onto a unit it did not open");

    assert_eq!(
        journal.close_unit(unit, Closing::Committed).await,
        Err(nc_journal::Error::UnitClosed { unit }),
        "the owner's commit was not refused as closed after a closing bracket it did not \
         write"
    );
    assert_eq!(
        journal.store().unit_closing(&id("realm-1"), unit).await,
        Ok(Some(Outcome::UnitAborted)),
        "PostgreSQL no longer reports the outcome the inserting party chose, so \
         `Store::unit_closing`'s paragraph on the first bracket is stale"
    );
    assert_eq!(
        journal.verify(0).await.expect("readable"),
        Ok(2),
        "`verify` no longer answers `Ok` over a closing bracket its owner did not write, \
         so the paragraph saying it does is stale"
    );
    finish(pool).await;
}

/// A NUL in a unit's label is stored escaped in both stores, and the close
/// repeats it as written.
///
/// **No path builds `Unstorable` for such a label.** The label is the bracket's
/// resource, which canonical JSON escapes — measured, both stores opening the
/// unit and the PostgreSQL chain verifying. The column is asked too, because
/// "escaped" is a claim about what the row holds and not only about what comes
/// back.
#[tokio::test]
async fn a_label_holding_a_nul_is_stored_escaped_in_both_stores() {
    const SCHEMA: &str = "nc_test_nul_label";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let label = "a\0b";
    let mut memory = Journal::new(
        nc_journal::InMemory::default(),
        id("realm-1"),
        id("kernel"),
        Minter::new(MachineFlake::from_value(31).expect("a flake"), SystemClock),
    );
    let mut postgres = journal(store_in(&pool, SCHEMA));
    let in_memory = memory
        .open_unit(label)
        .await
        .expect("a NUL in a label is not refused in memory");
    let in_postgres = postgres
        .open_unit(label)
        .await
        .expect("a NUL in a label is not refused by PostgreSQL");
    let closings = [
        memory
            .close_unit(in_memory, Closing::Committed)
            .await
            .expect("closes in memory"),
        postgres
            .close_unit(in_postgres, Closing::Committed)
            .await
            .expect("closes in PostgreSQL"),
    ];
    for closing in &closings {
        assert_eq!(
            closing.record.resource,
            Value::text(label),
            "the close did not repeat the label as it was written"
        );
    }

    let stored: String =
        sqlx::query_scalar("SELECT resource FROM journal_entries WHERE sequence = 0")
            .fetch_one(&pool)
            .await
            .expect("the opening is in the table");
    assert!(
        !stored.contains('\0') && stored.contains("\\u0000"),
        "the label reached the column as `{stored:?}`, not escaped"
    );
    let read = postgres.store().from(0).await.expect("readable");
    assert_eq!(read.stopped_at, None, "the opening did not decode");
    assert_eq!(
        read.entries.first().map(|entry| &entry.record.resource),
        Some(&Value::text(label)),
        "the label did not come back as it was written"
    );
    assert_eq!(postgres.verify(0).await.expect("readable"), Ok(2));
    finish(pool).await;
}

#[tokio::test]
async fn an_aborted_unit_is_closed_for_every_way_in() {
    // **Abort is the other half of "closed".** Both the insert's `WHERE` and
    // `unit_closing` list `unit-committed` *and* `unit-aborted`; every other
    // closed-unit test in this crate closes with `commit`, and
    // `an_aborted_unit_keeps_every_entry_including_a_denial` aborts without
    // ever trying to write afterwards. Without this, deleting `UnitAborted`
    // from the check leaves the suite green — so an entry could land after a
    // `unit-aborted` bracket, inside a group that was rolled back, and verify
    // clean.
    const SCHEMA: &str = "nc_test_unit_aborted_closed";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, "nc_test_unit_aborted_closed"));
    let mut group = journal.begin("unit").await.expect("begins");
    let unit = group.id();
    group
        .record(entry_record("Read"))
        .await
        .expect("records inside the unit");
    group.abort().await.expect("aborts");

    // Through the journal, which asks `unit_closed`.
    let mut racing = Journal::new(
        store_in(&pool, "nc_test_unit_aborted_closed"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(7 % nc_ids::flake::RANGE).expect("a flake"),
            SystemClock,
        ),
    );
    let mut record = entry_record("Delete");
    record.work_unit = Some(unit);
    assert_eq!(
        racing.record(record).await,
        Err(nc_journal::Error::UnitClosed { unit }),
        "an entry landed after the bracket that aborted its unit"
    );

    // And through the store, whose `WHERE` is the one that cannot be raced.
    let mut store = store_in(&pool, "nc_test_unit_aborted_closed");
    let head = store.head().await.expect("reads").expect("there are rows");
    let mut smuggled = entry_record("Delete");
    smuggled.work_unit = Some(unit);
    smuggled.realm = id("realm-1");
    let refused = store
        .append(Entry::chain(head.0 + 1, Some(head.1), smuggled))
        .await;
    assert_eq!(
        refused,
        Err(nc_journal::Error::UnitClosed { unit }),
        "the public store accepted an entry into an aborted unit: {refused:?}"
    );

    // The rows say so: the last thing carrying the unit is its own bracket.
    let after: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sequence, result FROM journal_entries WHERE work_unit = $1 ORDER BY sequence",
    )
    .bind(unit.to_crockford())
    .fetch_all(&pool)
    .await
    .expect("reads");
    assert_eq!(
        after.last().map(|(_, result)| result.as_str()),
        Some("unit-aborted")
    );

    finish(pool).await;
}

/// A row this code did not write, chained onto whatever the tail is now.
///
/// Every column is valid and the hash is not its own, so the row decodes and
/// then fails to verify — which is the only way to tell an answer that came
/// from the unit guard from one that came from the tail.
async fn forge_the_tail(pool: &sqlx::PgPool) {
    let (sequence, prev): (i64, String) =
        sqlx::query_as("SELECT sequence, hash FROM journal_entries ORDER BY sequence DESC LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("a history has a head");
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES ($1, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{}', 'allowed', 0, 1, \
         $2, repeat('a', 64))",
    )
    .bind(sequence + 1)
    .bind(prev)
    .execute(pool)
    .await
    .expect("a second writer can insert");
}

/// **A closed unit is the answer before the tail is even read.**
///
/// `Journal::append` asks the store whether the unit is closed, and whether it
/// is already open, before it reads and verifies the tail — its own paragraph
/// argues for that order in as many words. Both questions are asked a second
/// time by `Store::append`'s `WHERE`, which reports them with the same two
/// errors, so the outer copy is invisible through any history the store
/// accepts: measured, each of the two guards in `Journal::append` made never to
/// fire leaves the whole crate suite green with a database up, without this.
///
/// A forged tail is what tells the two copies apart. With the guard, the unit
/// is the answer; without it the append reaches the tail, the tail does not
/// verify, and the caller is told the history was tampered with — which is
/// true, is not what was asked, and sends whoever reads it to the chain instead
/// of to the unit they wrote into.
#[tokio::test]
async fn a_closed_unit_is_the_answer_before_the_tail_is_read() {
    const SCHEMA: &str = "nc_test_closed_before_tail";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, SCHEMA));
    let mut group = journal.begin("a unit that ends").await.expect("begins");
    let closed = group.id();
    group.commit().await.expect("commits");

    forge_the_tail(&pool).await;

    let mut inside = entry_record("Write");
    inside.work_unit = Some(closed);
    assert_eq!(
        journal.record(inside).await.err(),
        Some(nc_journal::Error::UnitClosed { unit: closed }),
        "the tail answered before the unit did, and the caller is sent to the chain"
    );

    finish(pool).await;
}

/// The same door one bracket along: an open unit, and a genuine second opener.
#[tokio::test]
async fn an_open_unit_is_the_answer_before_the_tail_is_read() {
    const SCHEMA: &str = "nc_test_open_before_tail";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, SCHEMA));
    let open = journal
        .begin("a unit that stays open")
        .await
        .expect("begins")
        .id();

    forge_the_tail(&pool).await;

    let mut again = entry_record(nc_journal::unit::OPENS);
    again.result = Outcome::UnitBegun;
    again.work_unit = Some(open);
    assert_eq!(
        journal.record(again).await.err(),
        Some(nc_journal::Error::UnitAlreadyOpen { unit: open }),
        "the tail answered before the unit did, and the caller is sent to the chain"
    );

    finish(pool).await;
}

/// **And the guard is only about an opener**, which is its other half.
///
/// The two clauses that make an entry an opening bracket — the action and the
/// outcome, and it takes both — are decided here and by nothing else in this
/// copy: either one made always true leaves the rest of the crate suite green,
/// and an ordinary entry written inside an open unit is
/// refused `UnitAlreadyOpen`, which is not retryable, for a unit its own owner
/// had opened. The store's `WHERE` accepts both entries below, so nothing
/// downstream would refuse them.
#[tokio::test]
async fn the_journals_opener_guard_is_only_about_an_opener() {
    const SCHEMA: &str = "nc_test_opener_guard_only";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, SCHEMA));
    let open = journal
        .begin("a unit that stays open")
        .await
        .expect("begins")
        .id();

    let mut carrying = entry_record("Write");
    carrying.result = Outcome::UnitBegun;
    carrying.work_unit = Some(open);
    journal
        .record(carrying)
        .await
        .expect("an ordinary action carrying the outcome is not a second opener");

    let mut denied = entry_record(nc_journal::unit::OPENS);
    denied.result = Outcome::Denied;
    denied.work_unit = Some(open);
    journal
        .record(denied)
        .await
        .expect("a begin that was denied opens nothing");

    finish(pool).await;
}

#[tokio::test]
async fn a_unit_closed_between_the_check_and_the_write_is_still_closed() {
    // **Check-then-write, with nothing holding the answer still.** Asking
    // `unit_closed`, then reading the head, then writing — three statements —
    // lets a second writer close the unit in between, and the entry lands
    // *after* its own `unit-committed` bracket, inside the group to every SQL
    // reader, with `verify` answering clean. That is the single-process
    // defect `WorkUnit::record` and `Journal::record` both refuse, and
    // `Store::unit_closing`'s own doc sells the store query on the
    // multi-process case.
    //
    // The gap is made explicit rather than raced for: this writes the closing
    // bracket itself, directly, *after* the check `Journal::record` has already
    // made — which is exactly the state a slow second writer produces, and
    // reproduces every run instead of sometimes.
    const SCHEMA: &str = "nc_test_unit_race";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, "nc_test_unit_race"));
    let mut group = journal.begin("unit").await.expect("begins");
    let unit = group.id();
    group
        .record(entry_record("Read"))
        .await
        .expect("records inside the unit");
    group.commit().await.expect("commits");

    // The unit is closed. A writer that asked *before* this commit landed still
    // holds `false`, and what stops it is the insert's own `WHERE`.
    let mut racing = Journal::new(
        store_in(&pool, "nc_test_unit_race"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(7 % nc_ids::flake::RANGE).expect("a flake"),
            SystemClock,
        ),
    );
    let mut record = entry_record("Delete");
    record.work_unit = Some(unit);
    assert_eq!(
        racing.record(record).await,
        Err(nc_journal::Error::UnitClosed { unit }),
        "an entry landed after its own closing bracket"
    );

    // **And the store refuses it directly**, which is where the check lives: a
    // caller that chained the entry itself and went straight to `Store::append`
    // walks past `Journal` entirely.
    let mut store = store_in(&pool, "nc_test_unit_race");
    let head = store.head().await.expect("reads").expect("there are rows");
    let mut smuggled = entry_record("Delete");
    smuggled.work_unit = Some(unit);
    smuggled.realm = id("realm-1");
    let refused = store
        .append(Entry::chain(head.0 + 1, Some(head.1), smuggled))
        .await;
    assert_eq!(
        refused,
        Err(nc_journal::Error::UnitClosed { unit }),
        "the public store accepted an entry into a closed unit: {refused:?}"
    );

    // The rows say so: nothing after the bracket carries the unit.
    let after: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sequence, result FROM journal_entries WHERE work_unit = $1 ORDER BY sequence",
    )
    .bind(unit.to_crockford())
    .fetch_all(&pool)
    .await
    .expect("reads");
    assert_eq!(
        after.last().map(|(_, result)| result.as_str()),
        Some("unit-committed")
    );

    finish(pool).await;
}

/// An entry that is not a bracket does not end a unit, in either store.
///
/// **The clause `unit_closing`'s own doc argues for,
/// decided.** `AND action = $3` is what makes a closing bracket a *bracket*:
/// without it an ordinary `Write` carrying `Outcome::UnitCommitted` — a row any
/// caller can ask for — closes somebody else's unit, and `Journal::append` then
/// refuses that unit's rightful owner. Measured without this
/// test: `AND $3 = $3` leaves the whole gate green, because the other tests
/// that reach `unit_closed` compare the two stores against each other and
/// **both** carry the clause, so neither the comparison nor the behaviour tests
/// can separate the rule from the rule beside it.
///
/// **Asked of each store directly, and the realm said out loud.** A record
/// built with `Record::new`, which copies the principal into the realm, asks
/// about a realm the record is not in, and passes with the clause deleted. A
/// test written to close a hole in coverage that cannot fail is the same defect
/// one level up.
#[tokio::test]
async fn an_ordinary_action_carrying_a_brackets_outcome_closes_nothing() {
    const SCHEMA: &str = "nc_test_unit_action";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Snowflake::from_crockford("01J8XK2M9P53").expect("a valid unit");
    for closing in [Outcome::UnitCommitted, Outcome::UnitAborted] {
        let fresh = a_schema_named(SCHEMA, closing.as_str());
        let scoped = a_schema_of_its_own(&pool, &fresh).await;
        let mut store = Postgres::in_schema(scoped.clone(), &fresh);
        let mut memory = nc_journal::InMemory::default();

        let mut record = entry_record("Write");
        record.work_unit = Some(unit);
        record.result = closing;
        // **The realm, said out loud.** `Record::new` copies the principal into
        // the realm — a journal overwrites it on append — so a record built
        // here and asked about under `realm-1` would answer "not closed"
        // whatever the predicate does, and pass with the clause it is written
        // for deleted.
        record.realm = id("realm-1");
        let entry = nc_journal::Entry::chain(0, None, record);
        store
            .append(entry.clone())
            .await
            .expect("an ordinary row lands");
        memory.append(entry).await.expect("an ordinary row lands");

        for (which, closed) in [
            ("postgres", store.unit_closed(&id("realm-1"), unit).await),
            ("in memory", memory.unit_closed(&id("realm-1"), unit).await),
        ] {
            assert!(
                !closed.expect("the store answers"),
                "{which}: a `{}` carried by an ordinary `Write` closed the unit, so an \
                 entry nobody wrote as a bracket ends somebody else's work",
                closing.as_str()
            );
        }
    }
    finish(pool).await;
}

#[tokio::test]
async fn a_unit_closed_in_one_realm_is_not_closed_in_another() {
    const SCHEMA: &str = "nc_test_unit_realm";
    // **A journal must not refuse a write on the strength of another realm's
    // history.** A scan reading every row in the table lets a `UnitCommitted`
    // bracket written by `realm-1` close the same unit for `realm-2` — and `Journal::append`
    // overwrites `record.realm` from the journal precisely so that one realm
    // cannot reach another's history.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut first = journal(store_in(&pool, "nc_test_unit_realm"));
    let unit = {
        let mut group = first.begin("unit").await.expect("begins");
        let unit = group.id();
        group.commit().await.expect("commits");
        unit
    };

    let mut second = Journal::new(
        store_in(&pool, "nc_test_unit_realm"),
        id("realm-2"),
        id("principal-1"),
        Minter::new(
            MachineFlake::from_value(999 % nc_ids::flake::RANGE).expect("a flake"),
            SystemClock,
        ),
    );
    let mut record = entry_record("Read");
    record.work_unit = Some(unit);
    let written = second
        .record(record)
        .await
        .expect("realm-2 has closed nothing");
    assert_eq!(written.record.realm, id("realm-2"));

    // The brackets that closed it are realm-1's, and they are all that is there.
    let realms: Vec<(String, String)> =
        sqlx::query_as("SELECT realm, action FROM journal_entries ORDER BY sequence")
            .fetch_all(&pool)
            .await
            .expect("reads");
    assert_eq!(realms.len(), 3, "{realms:?}");
    assert_eq!(realms[2].0, "realm-2", "{realms:?}");
    finish(pool).await;
}

#[tokio::test]
async fn the_first_broken_entry_is_the_one_named() {
    // **An attacker must not choose which row gets investigated.** Returned
    // *before* the loop, the unreadable-row verdict would contradict its own
    // comment — "after the loop below has had its chance at an earlier broken
    // link" — and a forged early entry and a poisoned later one would make
    // verification name the later one.
    //
    // Built with `INSERT`s rather than `UPDATE`s: the table refuses an update
    // in every session role
    // (`migrations/0004_the_journal_refuses_replication_mode.sql`), which is
    // what makes this a test of the *order of the verdicts* rather than of the
    // trigger. A second connection may insert, which is the hole `verify`
    // exists to find.
    const SCHEMA: &str = "nc_test_first_broken";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_first_broken");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry");

    // Sequence 1: a hash that does not match its contents — a forgery the loop
    // finds.
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (1, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{}'::jsonb, \
         'allowed', 0, 0, NULL, repeat('0', 64))",
    )
    .execute(&pool)
    .await
    .expect("a second connection may insert");

    // Sequence 2: a row that will not decode at all — the verdict that must not
    // win.
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (2, 'realm-1', 'attacker', '[]'::jsonb, 'Write', '{}'::jsonb, \
         'invented', 0, 0, NULL, repeat('0', 64))",
    )
    .execute(&pool)
    .await
    .expect("a second connection may insert");

    let journal = Journal::new(
        store_in(&pool, "nc_test_first_broken"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(13).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a forged entry is not a clean history");
    assert_eq!(
        verdict.sequence, 1,
        "verification named a later row rather than the first broken one: {verdict}"
    );

    finish(pool).await;
}

#[tokio::test]
async fn an_unreadable_row_still_gets_its_verdict() {
    // The other side of the ordering: holding the unreadable-row verdict until
    // the loop has looked must not turn it into a silence when nothing earlier
    // is wrong. A good chain, and one row at the end this code cannot read.
    const SCHEMA: &str = "nc_test_only_poisoned";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    store_in(&pool, "nc_test_only_poisoned")
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry");
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (1, 'realm-1', 'someone', '[]'::jsonb, 'Write', '{}'::jsonb, \
         'invented', 0, 0, NULL, repeat('0', 64))",
    )
    .execute(&pool)
    .await
    .expect("a second connection may insert");

    let journal = Journal::new(
        store_in(&pool, "nc_test_only_poisoned"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(14).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("an unreadable row is not a clean history");
    assert_eq!(verdict.sequence, 1, "{verdict}");
    assert_eq!(verdict.code(), nc_catalog::ErrorCode::JournalEntryTampered);

    finish(pool).await;
}

/// A row whose own `sequence` will not decode says so, and names no row number.
///
/// **`unwrap_or(u64::MAX)` would assert a row that cannot exist.** The column
/// is `BIGINT` under `CHECK (sequence >= 0)` and is the primary key, so
/// 18446744073709551615 is a number no row in this table has ever held — and the
/// verdict built from it reads *"entry 18446744073709551615 … is in the table"*.
/// Reachable through exactly the channel the three canonical guards defend
/// against: a heap rewrite that fires no row trigger.
#[tokio::test]
async fn a_row_whose_sequence_will_not_decode_names_no_row_number() {
    const SCHEMA: &str = "nc_test_sequence_unreadable";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_sequence_unreadable");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry");

    // `numeric` is still a number to PostgreSQL — so `CHECK (sequence >= 0)`
    // still holds and the rewrite is accepted — and it is not a `BIGINT` to
    // `sqlx`, so the column this code reads by type stops decoding.
    sqlx::query(
        "ALTER TABLE journal_entries ALTER COLUMN sequence TYPE numeric USING sequence::numeric",
    )
    .execute(&pool)
    .await
    .expect("a row trigger is not a rewrite hook, which is the point of this test");

    let refusal = store
        .tail()
        .await
        .expect_err("a row this code cannot read is a verdict, not a silence");
    let said = refusal.to_string();
    assert!(
        said.contains("cannot read") && said.contains("the number here is not one"),
        "the verdict names a row number it invented: {said}"
    );

    // **And through `verify`, which is the caller a person reads.** `postgres.rs`
    // writes the sentence where it is produced; a `verify` reading only the
    // sequence out of `stopped_at` and discarding the reason would still show
    // `entry 18446744073709551615 … is in the table`, which the table's own
    // `CHECK (sequence >= 0)` makes impossible.
    let journal = Journal::new(
        store_in(&pool, "nc_test_sequence_unreadable"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(17).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a row this code cannot read is not a clean history");
    let said = verdict.to_string();
    assert!(
        said.contains("the number here is not one"),
        "the verdict a reader sees still names a row number it invented: {said}"
    );
    finish(pool).await;
}

/// The first entry that does not verify is named, when the tail still decodes.
///
/// **The ordering has a test and the test takes the other
/// branch.** `the_first_broken_entry_is_the_one_named` makes the *last* row
/// unreadable, so `tail()` fails and `verify` answers through the empty-tail
/// branch — what it exercises is `verdict_after_checking`, not the `stopped_at`
/// return after the loop. Without this, that return could be moved above the
/// loop with the whole crate green:
///
/// ```text
/// as shipped:  seq=1 HashDoesNotMatchContents   (the forged row)
/// moved back:  seq=2 EntryIsNotReadable         (the poisoned row)
/// ```
///
/// Which hands an attacker back the choice of which row a responder
/// investigates: poison a row after the forgery and the forgery is not the one
/// named. `verify`'s own doc promises "the **first** entry that does not
/// verify".
#[tokio::test]
async fn the_forged_row_is_named_even_when_a_later_row_will_not_decode() {
    const SCHEMA: &str = "nc_test_forged_before_poison";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_forged_before_poison");
    let mut previous = None;
    for sequence in 0..4 {
        let entry = Entry::chain(sequence, previous, full_record());
        previous = Some(entry.hash);
        store.append(entry).await.expect("a good chain");
    }

    // Entry 1 forged, entry 2 poisoned, entry 3 left readable — so `tail()`
    // succeeds and `verify` answers through the branch the ordering lives on.
    rewrite_column(
        &pool,
        "action",
        "CASE WHEN sequence = 1 THEN 'Escalate' ELSE action END",
    )
    .await;
    rewrite_column(
        &pool,
        "result",
        "CASE WHEN sequence = 2 THEN 'invented' ELSE result END",
    )
    .await;

    let journal = Journal::new(
        store_in(&pool, "nc_test_forged_before_poison"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(16).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a forged entry is not a clean history");
    assert_eq!(
        verdict.sequence, 1,
        "a row poisoned after the forgery chose which row was named: {verdict}"
    );
    assert_eq!(verdict.code(), nc_catalog::ErrorCode::JournalEntryTampered);
    finish(pool).await;
}

/// The first row of a read being unreadable is said as that, not as a gap.
///
/// **A `verify` returning on the empty-`entries` branch before it consults
/// where the read stopped** reports a row rewritten in place at the start of a
/// range as `HistoryDoesNotStartHere` — whose own doc says *"the rows before
/// it are gone, and their absence is invisible to a chain of links"*, which is
/// false: the row is on disk and was rewritten where it lies. Measured with
/// such a `verify`, one tamper applied at each position of a history:
///
/// ```text
/// position 0 -> HistoryDoesNotStartHere / JournalChainBroken
/// position 1 -> EntryIsNotReadable      / JournalEntryTampered
/// ```
///
/// The two codes exist so a responder knows whether to look at one row or at
/// the whole history above it, and the sentence a person reads differs too. The
/// sibling branch — the one for an unreadable *tail* — consults `stopped_at`
/// too, so both halves of the function answer alike.
///
/// **The tail has to stay readable**, or `verify` answers through that sibling
/// branch and this one is never reached — and the test would pass with the
/// ordering reverted.
#[tokio::test]
async fn the_first_row_of_a_read_being_unreadable_is_not_a_missing_history() {
    const SCHEMA: &str = "nc_test_unreadable_first";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_unreadable_first");
    let mut previous = None;
    for sequence in 0..3 {
        let entry = Entry::chain(sequence, previous, full_record());
        previous = Some(entry.hash);
        store.append(entry).await.expect("a good chain");
    }

    // Only sequence 0, so the tail this read is checked against still decodes.
    rewrite_column(
        &pool,
        "result",
        "CASE WHEN sequence = 0 THEN 'invented' ELSE result END",
    )
    .await;

    let journal = Journal::new(
        store_in(&pool, "nc_test_unreadable_first"),
        id("realm-1"),
        id("kernel"),
        Minter::new(
            MachineFlake::from_value(15).expect("fits in ten bits"),
            SystemClock,
        ),
    );
    let verdict = journal
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("an unreadable first row is not a clean history");
    assert_eq!(verdict.sequence, 0, "{verdict}");
    assert_eq!(
        verdict.code(),
        nc_catalog::ErrorCode::JournalEntryTampered,
        "a row rewritten in place was reported as the rows before it being gone: {verdict}"
    );
    // **And the reason travels with the sequence.** `Broken::detail` is what
    // turns "entry 0 does not verify" into a sentence naming the two spellings,
    // and an assertion on the sequence and the code alone would stay green
    // with it dropped from both paths through `chained_and_contiguous`; this
    // reads what a responder is actually handed.
    let said = verdict.to_string();
    assert!(
        said.contains(" — "),
        "the verdict carries no reason at all: {said}"
    );
    // **And which kind of damage it says this is.** `Broken::code()` maps both
    // `EntryIsNotReadable` and `HashDoesNotMatchContents` to
    // `JournalEntryTampered`, so without this a site constructing the wrong one
    // passes every assertion here — producing `its hash is not the hash of its contents —
    // \`invented\` is not an outcome this code knows`, which contradicts itself.
    assert!(
        said.contains("this code cannot read it"),
        "the verdict names the wrong kind of damage: {said}"
    );
    assert!(
        said.contains("invented"),
        "the reason does not name the value that would not decode: {said}"
    );

    finish(pool).await;
}

/// Runs one statement with `session_replication_role = replica`, on **one**
/// connection.
///
/// **A pool is not a session.** `SET session_replication_role` is a *session*
/// setting, and a `PgPool` hands each statement whichever connection is free —
/// so on a pool the `SET` and the statement it is meant to affect run on
/// different backends. Measured against a pool shaped exactly like `pool()`'s:
///
/// ```text
/// after SET, statement 0: pid=7364 role=origin
/// after SET, statement 1: pid=7363 role=replica
/// after SET, statement 2: pid=7364 role=origin
/// ```
///
/// The connections alternate, so whether a replication-mode half on a pool
/// tests anything at all is decided by how many statements happen to precede
/// the `SET`. Proof that it matters: deleting `ENABLE ALWAYS`
/// from `0005_the_journal_refuses_truncate.sql` leaves a pooled truncate test
/// green ten runs out of ten, while the trigger is genuinely bypassable —
/// verified by hand in `psql`, where one session really is one session.
///
/// `pool.acquire()` pins a single connection for the whole block, which is what
/// a session is.
async fn in_replication_mode(
    pool: &sqlx::PgPool,
    statement: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    use sqlx::Acquire as _;

    let mut held = pool.acquire().await.expect("a connection of our own");
    let connection = held.acquire().await.expect("the same one");
    sqlx::query("SET session_replication_role = replica")
        .execute(&mut *connection)
        .await
        .expect("the test's own session is ours to set");
    // Asserted, not assumed: if the setting did not take on *this* connection,
    // the refusal below would prove nothing and would look like a pass.
    let role: String = sqlx::query_scalar("SHOW session_replication_role")
        .fetch_one(&mut *connection)
        .await
        .expect("the session answers");
    assert_eq!(
        role, "replica",
        "the SET did not reach the connection under test"
    );
    let outcome = sqlx::query(sqlx::AssertSqlSafe(statement))
        .execute(&mut *connection)
        .await;
    sqlx::query("SET session_replication_role = origin")
        .execute(&mut *connection)
        .await
        .expect("set back");
    outcome
}

/// An unreadable tail is never a clean verdict, whatever `from` says.
///
/// **The fallback that says so, decided.** `verify` carries where `tail()`
/// stopped past the read, and answers with the read's own stopping point when
/// there is one — the
/// ordering `the_forged_row_is_named_even_when_a_later_row_will_not_decode`
/// exists for. The `.or` behind it is the other case, and without this deleting
/// it leaves the whole crate suite green: measured, `verify` then
/// answers `Ok(0)` — *a clean history of nothing* — for a table whose last row
/// this code cannot read.
///
/// It is reached when the read starts past the unreadable row, so the read
/// itself meets nothing and only `tail()` did. A `from` beyond the tail is a
/// caller whose checkpoint is ahead of the history — a restored backup, or a
/// number it was handed — and *"there is nothing there"* is exactly the wrong
/// thing to tell it while the last row on disk is one nobody here wrote.
#[tokio::test]
async fn an_unreadable_tail_is_never_clean_however_far_ahead_from_is() {
    const SCHEMA: &str = "nc_test_unreadable_tail_ahead";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    let mut previous = None;
    for sequence in 0..3 {
        let entry = Entry::chain(sequence, previous, full_record());
        previous = Some(entry.hash);
        store.append(entry).await.expect("a good chain");
    }
    // The last row, and only it: `tail()` stops, and a read starting after it
    // finds nothing to stop on.
    rewrite_column(
        &pool,
        "result",
        "CASE WHEN sequence = 2 THEN 'invented' ELSE result END",
    )
    .await;

    let journal = journal(store_in(&pool, SCHEMA));
    let verdict = journal
        .verify(4)
        .await
        .expect("the store answers")
        .expect_err("a history whose last row will not decode is not a clean one");
    assert_eq!(
        verdict.sequence, 2,
        "the verdict named a row other than the one that will not decode: {verdict}"
    );
    assert_eq!(
        verdict.reason,
        nc_journal::Reason::EntryIsNotReadable,
        "the row nobody here wrote was reported as something else: {verdict}"
    );

    finish(pool).await;
}

/// Rewrites one column in place, past both append-only triggers.
///
/// **This is not `UPDATE` and no trigger fires.**
/// `ALTER TABLE … ALTER COLUMN … TYPE text USING <expr>` rewrites the heap, and
/// a row trigger is not a rewrite hook — so it edits rows the table refuses to
/// let anyone edit, and unlike `DISABLE TRIGGER` it leaves `tgenabled = 'A'`,
/// so the catalog records nothing afterwards. It needs table ownership, which
/// the application's own role has.
///
/// The chain catches this the moment it touches a *hashed* field, which is the
/// point of hashing them. What it cannot catch without the round-trip guards
/// in `postgres.rs` is a rewrite that keeps the value and changes the spelling
/// — so this helper is how both of those guards are exercised for real, rather
/// than through `SET session_replication_role = replica`,
/// which `migrations/0004_the_journal_refuses_replication_mode.sql` refuses.
async fn rewrite_column(pool: &sqlx::PgPool, column: &str, expression: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE journal_entries ALTER COLUMN {column} TYPE text USING {expression}"
    )))
    .execute(pool)
    .await
    .expect("a row trigger is not a rewrite hook, which is the point of this test");
}

#[tokio::test]
async fn a_row_respelled_without_changing_its_length_is_not_read_as_the_same_row() {
    // **A fixture that changes the number of bytes cannot tell a length from
    // the bytes.** `"content"` becoming `"\u0063ontent"` is nine characters to
    // fourteen, and `01J8XK2M9P51` becoming `OIJ8XK2M9P5I` is the same length
    // but exercises only two of the three leniencies the guard's own comment
    // names. So a comparison that read the **length** rather than the bytes
    // would pass every such test. Measured, each planted on its own and all
    // three together:
    //
    // ```text
    // if canonical.len() == stored.len() {          (resource)
    // if canonical.len() != stored_actors.len() {   (actor_chain)
    // if read.to_crockford().eq_ignore_ascii_case(&unit) {   (work_unit)
    //   -> nc-journal and nc-canon green at 182 tests
    //   -> cargo xtask check: check passed, ok 1185 test(s)
    // ```
    //
    // and, through the same `ALTER TABLE … USING` a table's owner can run,
    // `{"kind":"content","id":"c-1",…}` read clean where
    // `{"id":"c-1","kind":"content",…}` was written, `[ "agent-1","principal-1"]`
    // read clean where `["agent-1", "principal-1"]` was, and the work unit read
    // clean in lower case while `WHERE work_unit = '<canonical>'` found nothing.
    //
    // Each rewrite here keeps the byte count exactly, so a length comparison
    // cannot tell it from the text that was written. One test rather than three,
    // because it is one claim: what these guards compare is the bytes.
    const SCHEMA: &str = "nc_test_same_length";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    // The keys of the resource, exchanged: the same characters in another order.
    let canonical: String = sqlx::query_scalar("SELECT resource FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    rewrite_column(
        &pool,
        "resource",
        r#"replace(resource, '{"id":"c-1","kind":"content"', '{"kind":"content","id":"c-1"')"#,
    )
    .await;
    let respelled: String = sqlx::query_scalar("SELECT resource FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the bytes");
    assert_eq!(
        respelled.len(),
        canonical.len(),
        "this fixture exists to keep the length: {canonical} against {respelled}"
    );
    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a resource whose keys were exchanged is not the row that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(problem.contains("resource"), "{problem}");
    finish(pool).await;
}

#[tokio::test]
async fn an_actor_chain_respelled_without_changing_its_length_is_refused() {
    // The chain half of the claim above: a space moved from after the comma to
    // before the first name. The same characters, the same parsed array, and the
    // same number of bytes — so a length comparison reads it as what was
    // written, while `WHERE actor_chain::text LIKE '%"agent-1", %'` does not.
    // `actor_chain` is `§14.6`'s delegation chain, the column that says who is
    // answerable.
    const SCHEMA: &str = "nc_test_actors_same_length";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let canonical: String = sqlx::query_scalar("SELECT actor_chain::text FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    sqlx::query(
        r#"ALTER TABLE journal_entries ALTER COLUMN actor_chain TYPE json
           USING replace(actor_chain::text, '["agent-1", "principal-1"]', '[ "agent-1","principal-1"]')::json"#,
    )
    .execute(&pool)
    .await
    .expect("a row trigger is not a rewrite hook, which is the point of this test");
    let respelled: String = sqlx::query_scalar("SELECT actor_chain::text FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the bytes");
    assert_eq!(
        respelled.len(),
        canonical.len(),
        "this fixture exists to keep the length: {canonical} against {respelled}"
    );
    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("an actor chain respaced is not the chain that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(problem.contains("actor_chain"), "{problem}");
    finish(pool).await;
}

#[tokio::test]
async fn a_work_unit_in_another_case_is_not_read_as_the_same_row() {
    // **The first leniency the guard's own comment names.** It says the
    // Crockford reader is *"case-insensitive, `O` for `0`, `I` and `L` for
    // `1`"*; with only `translate(work_unit, '01', 'OI')`, the second and
    // third, a comparison written
    // `read.to_crockford().eq_ignore_ascii_case(&unit)` passes everything —
    // measured, with `lower(work_unit)` in the column, `from(0)` reads clean
    // and no row is found by the canonical spelling.
    const SCHEMA: &str = "nc_test_unit_lowered";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let canonical: String = sqlx::query_scalar("SELECT work_unit FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    rewrite_column(&pool, "work_unit", "lower(work_unit)").await;
    let respelled: String = sqlx::query_scalar("SELECT work_unit FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the case");
    assert_eq!(
        respelled,
        canonical.to_lowercase(),
        "the fixture lowers the case and changes nothing else"
    );
    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a work unit in another case is not the identifier that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(problem.contains("work unit"), "{problem}");
    finish(pool).await;
}

#[tokio::test]
async fn a_work_unit_respelled_in_the_column_is_not_read_as_the_same_row() {
    // **A row edit the hash does not cover.** `Entry::hashed_form` hashes
    // `unit.to_crockford()` — the canonical re-encoding — while a reader
    // decoding the column with `nc-ids`' Crockford reader alone is
    // deliberately lenient: case-insensitive, `O` for `0`, `I` and `L` for
    // `1`. So the stored text can be respelled and the hash does not move.
    //
    // Measured without the guard: `verify` answers `Ok`, and every SQL reader —
    // `WHERE work_unit = '<canonical>'`, and the table's own index — stops
    // finding the row. The entry silently leaves its work unit while criterion
    // 3 reports the history clean.
    const SCHEMA: &str = "nc_test_respelled";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_respelled");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let canonical: String = sqlx::query_scalar("SELECT work_unit FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    // The same identifier, spelled the way the lenient reader also accepts.
    rewrite_column(&pool, "work_unit", "translate(work_unit, '01', 'OI')").await;
    let respelled: String = sqlx::query_scalar("SELECT work_unit FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(
        respelled, canonical,
        "the fixture has to change the spelling"
    );

    // **`stopped_at`, not `Err`.** `from` reports a row it cannot read as the
    // point the read stopped and returns the entries before it — an `Err` would
    // throw away the history that is still good. An `expect_err` inside an `if`
    // that the append-only trigger makes false proves nothing in either
    // direction.
    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a respelled work unit is not the row that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(
        problem.contains(&canonical) && problem.contains(&respelled),
        "the error does not name both spellings: {problem}"
    );
    // And the verdict a responder gets, which is the criterion-3 promise.
    let verdict = journal(store_in(&pool, "nc_test_respelled"))
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a respelled work unit is not a clean history");
    assert_eq!(verdict.sequence, 0, "{verdict}");

    finish(pool).await;
}

/// A respelling **after** rows that verify is reported with its reason.
///
/// **The other branch of `verify`.** When the first row of a read will not
/// decode, `entries` comes back empty and the verdict is built before the loop;
/// when the rows in front of it are fine and a *later* one will not decode, the
/// loop runs to the end and the verdict is built after it. Both
/// consult `stopped_at` and both carry its reason, and an assertion on the
/// sequence and the code alone would stay green with the second changed to
/// `detail: None`; this reads the sentence.
///
/// Which matters here more than anywhere: two rows verified, so a responder is
/// being told the history is broken at row 2 and needs to know that what is
/// wrong with it is a respelling, with both spellings in front of them.
#[tokio::test]
async fn a_later_row_respelled_names_the_row_and_says_what_is_wrong_with_it() {
    const SCHEMA: &str = "nc_test_later_respelled";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_later_respelled");
    // Four rows. The one that gets rewritten is in the *middle*: the tail has
    // to stay readable, or `verify` answers through its own early branch and
    // the one this test is about is never reached.
    for sequence in 0..4 {
        let head = store.head().await.expect("reads");
        store
            .append(Entry::chain(
                sequence,
                head.map(|(_, hash)| hash),
                full_record(),
            ))
            .await
            .expect("appends");
    }

    let canonical: String =
        sqlx::query_scalar("SELECT resource FROM journal_entries WHERE sequence = 1")
            .fetch_one(&pool)
            .await
            .expect("the row is there");
    // One row only: the `CASE` leaves every other row the bytes it was written
    // with, so row 0 goes on verifying and row 3 stays a readable tail.
    rewrite_column(
        &pool,
        "resource",
        r#"CASE WHEN sequence = 1 THEN replace(resource, '"content"', '"\u0063ontent"') ELSE resource END"#,
    )
    .await;
    let respelled: String =
        sqlx::query_scalar("SELECT resource FROM journal_entries WHERE sequence = 1")
            .fetch_one(&pool)
            .await
            .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the bytes");

    let verdict = journal(store_in(&pool, "nc_test_later_respelled"))
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a respelled row is not a clean history");
    assert_eq!(
        verdict.sequence, 1,
        "the row that was rewritten is not the one named: {verdict}"
    );
    assert_eq!(verdict.code(), nc_catalog::ErrorCode::JournalEntryTampered);
    let said = verdict.to_string();
    assert!(
        said.contains(&canonical) && said.contains(&respelled),
        "the verdict names neither spelling: {said}"
    );
    // And which kind of damage, which the shared error code hides: both
    // `EntryIsNotReadable` and `HashDoesNotMatchContents` answer
    // `JournalEntryTampered`.
    assert!(
        said.contains("this code cannot read it"),
        "the verdict names the wrong kind of damage: {said}"
    );

    finish(pool).await;
}

#[tokio::test]
async fn a_resource_respelled_in_the_column_is_not_read_as_the_same_row() {
    // **The same hole, in the field that says what a command acted on.**
    // `Entry::hashed_form` hashes the parsed `Value`, `append` stores
    // `to_canonical()`, and `nc_canon::parse` accepts more than it emits —
    // insignificant whitespace, `\u` escapes, leading zeros. So the text of the
    // column could be rewritten to another spelling of the same value, the
    // digest would not move, and without the guard `verify` answers `Ok` while
    // every reader searching for what was written finds nothing.
    //
    // It is the harder one to notice because the value really is unchanged: the
    // damage is to everyone who reads the table as text, which is everyone.
    const SCHEMA: &str = "nc_test_resource_respelled";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_resource_respelled");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");

    let canonical: String = sqlx::query_scalar("SELECT resource FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    // `"content"` written with the `c` escaped. Same value, different bytes.
    rewrite_column(
        &pool,
        "resource",
        r#"replace(resource, '"content"', '"\u0063ontent"')"#,
    )
    .await;
    let respelled: String = sqlx::query_scalar("SELECT resource FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_ne!(respelled, canonical, "the fixture has to change the bytes");
    assert!(respelled.contains("u0063"), "{respelled}");

    let (sequence, problem) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a respelled resource is not the row that was written");
    assert_eq!(sequence, 0, "{problem}");
    assert!(
        problem.contains(&canonical) && problem.contains(&respelled),
        "the error does not name both spellings: {problem}"
    );
    let verdict = journal(store_in(&pool, "nc_test_resource_respelled"))
        .verify(0)
        .await
        .expect("the store answers")
        .expect_err("a respelled resource is not a clean history");
    assert_eq!(verdict.sequence, 0, "{verdict}");
    // **And the verdict carries the reason, not only the row number.** The
    // sentence a responder reads is what distinguishes "this row was respelled
    // after it was written, here are both spellings" from "entry 0 does not
    // verify"; an assertion on the sequence and the code alone would stay
    // green with `Broken::detail` dropped from this path, so this reads what is
    // handed to a person.
    let said = verdict.to_string();
    assert!(
        said.contains(&canonical) && said.contains(&respelled),
        "the verdict names neither spelling: {said}"
    );
    // And which kind of damage, which the shared error code hides: both
    // `EntryIsNotReadable` and `HashDoesNotMatchContents` answer
    // `JournalEntryTampered`.
    assert!(
        said.contains("this code cannot read it"),
        "the verdict names the wrong kind of damage: {said}"
    );

    finish(pool).await;
}

#[tokio::test]
async fn a_second_writer_racing_this_one_is_reported_as_out_of_order() {
    const SCHEMA: &str = "nc_test_racing";
    // What a primary-key collision means from here: the sequence this writer
    // computed from the tail was taken between the read and the write. Reporting
    // it as the store being unreachable would send a responder to PostgreSQL.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_racing");
    let entry = Entry::chain(0, None, full_record());
    store.append(entry.clone()).await.expect("appends");

    let again = store.append(entry).await;
    // **The numbers, not just the shape.** Asserting
    // `matches!(…, OutOfOrder { .. })` lets both fields carry the same value,
    // which renders as "the journal expected sequence 0 and was handed 0" — a
    // sentence that says a number differs from itself.
    assert_eq!(
        again,
        Err(nc_journal::Error::OutOfOrder {
            expected: 1,
            found: 0
        }),
        "{again:?}"
    );
    let Err(error) = again else {
        unreachable!("asserted above")
    };
    let rendered = error.to_string();
    assert!(
        !rendered.contains("expected sequence 0 and was handed 0"),
        "the refusal contradicts itself: {rendered}"
    );
    finish(pool).await;
}

/// Writes one row directly, the way a second writer would.
///
/// `migrations/0001_journal.sql` puts the trigger on `UPDATE` and `DELETE` only,
/// so a second connection — an older version of this code, a migration, somebody
/// with `psql` — can `INSERT` whatever it likes. That is the shape every read
/// guard in `postgres.rs` exists for, and this is what exercises them.
async fn insert_raw(pool: &sqlx::PgPool, hash: &str, result: &str, recorded_at: i64) {
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, hash) \
         VALUES (0, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', 'null', $1, $2, $3)",
    )
    .bind(result)
    .bind(recorded_at)
    .bind(hash)
    .execute(pool)
    .await
    .expect("the trigger refuses UPDATE and DELETE, not INSERT");
}

/// The two halves of `0009` that a wrong width does not reach.
///
/// **Sixty-four *lowercase* hex characters, and the link at all.** Both tests
/// that name `0009` plant a hash of the wrong width — `abcd` and `ab` — so the
/// width is the only half either one reaches, so without this
/// `journal_entries_prev_hash_is_hex` is exercised by nothing in either
/// direction. Both assertions also read `contains("hash_is_hex")`, which is a
/// substring of the other constraint's name, so neither can say which of the
/// two refused.
///
/// Lowercase is not a preference. `nc_canon::to_hex` emits lowercase and the
/// chain compares the column's text, so an uppercase spelling of the same
/// digest is a second spelling of one value: a row that can be respelled
/// without moving the digest, which is the shape `actor_chain_from`,
/// `resource` and `work_unit` are each written against.
#[tokio::test]
async fn a_hash_is_lowercase_hex_and_so_is_the_link() {
    const SCHEMA: &str = "nc_test_hash_case";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let hash = "a".repeat(64);
    let shouting = "A".repeat(64);
    // A hash of the right width, in the wrong case — whole and by one
    // character, because a class widened to `[0-9a-fA-F]` accepts both and a
    // rule anchored to the wrong end accepts one.
    for wrong in [shouting.clone(), format!("{}B", "a".repeat(63))] {
        let refused = a_row_at(&pool, 0, None, &wrong)
            .await
            .expect_err("a hash that is not lowercase hex is not a hash");
        assert!(
            refused.to_string().contains("journal_entries_hash_is_hex"),
            "`{wrong}` was refused by something other than the column's own rule: {refused}"
        );
    }
    // The row both constraints are written to accept, so what is above is not a
    // table that refuses everything.
    a_row_at(&pool, 0, None, &hash)
        .await
        .expect("sixty-four lowercase hex characters are a hash");
    // And the link, which carries the same rule and one more: it may be absent,
    // and only the first entry's is.
    for wrong in ["not hex at all".to_owned(), shouting.clone()] {
        let refused = a_row_at(&pool, 1, Some(&wrong), &hash)
            .await
            .expect_err("a link that is not a hash is not a link");
        assert!(
            refused
                .to_string()
                .contains("journal_entries_prev_hash_is_hex"),
            "link `{wrong}` was refused by something other than the column's own rule: \
             {refused}"
        );
    }

    finish(pool).await;
}

/// One row, written past this crate entirely, so the column's own rule answers.
async fn a_row_at(
    pool: &sqlx::PgPool,
    sequence: i64,
    prev: Option<&str>,
    hash: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, prev_hash, hash) \
         VALUES ($1, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', 'null', 'allowed', 1, \
         $2, $3)",
    )
    .bind(sequence)
    .bind(prev)
    .bind(hash)
    .execute(pool)
    .await
}

#[tokio::test]
async fn a_hash_of_the_wrong_width_cannot_be_written_and_is_read_as_tampering() {
    const SCHEMA: &str = "nc_test_short_hash";
    // A wrong width is what tampering looks like, and without the reader's check
    // the array conversion panics — inside verification, which is the code that
    // runs when somebody already suspects the table.
    //
    // **And the column refuses it.** A `hash` this code cannot read would lock
    // the journal for ever and make the documented anchor repair impossible, so
    // `migrations/0009_a_hash_is_sixty_four_hex_characters.sql` constrains the
    // column to sixty-four lowercase hex characters. Such a row cannot exist,
    // which is the stronger guarantee — and the reader's guard stays under test
    // at the seam, because a column constraint is not a reason to stop being
    // able to read a row that got past one.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let refused = sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, hash) \
         VALUES (0, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', 'null', 'allowed', 1, \
         'abcd')",
    )
    .execute(&pool)
    .await
    .expect_err("a hash of the wrong width is not a hash");
    assert!(
        refused.to_string().contains("journal_entries_hash_is_hex"),
        "the row was refused by something other than the column's own rule: {refused}"
    );

    // And the reader still says which row and why, rather than panicking: the
    // guard is what stands behind the constraint if a future migration ever
    // drops it.
    let said = nc_journal::postgres::digest_for_tests("hash", "abcd")
        .expect_err("a short hash is not a digest");
    assert!(said.contains("32"), "{said}");

    // **And too long.** Fed only *short* hex, the exact-width conversion could
    // be replaced by a silently truncating one with the whole gate green — and
    // a 66-character `hash` then reads back as its first 32 bytes, so `verify`
    // condemns the row as `HashDoesNotMatchContents` ("edited in place, look at
    // this one row") instead of `EntryIsNotReadable` ("a row this code did not
    // write"). `Broken::code` splits those two so a responder knows whether to
    // look at one row or at the history above it. Measured:
    // `bytes.truncate(32)` leaves `check passed` without this.
    let too_long = nc_journal::postgres::digest_for_tests("hash", &"ab".repeat(33))
        .expect_err("a hash wider than a digest is not a digest");
    assert!(
        too_long.contains("32"),
        "a row whose hash is wider than a digest was read as its first 32 bytes: {too_long}"
    );

    finish(pool).await;
}

#[tokio::test]
async fn an_outcome_this_code_does_not_know_is_reported_rather_than_guessed() {
    const SCHEMA: &str = "nc_test_outcome";
    // Written by a newer version. Reading it as anything else would misreport
    // what happened, in the log that exists to say what happened.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    insert_raw(&pool, &"ab".repeat(32), "quarantined", 1).await;
    let store = store_in(&pool, "nc_test_outcome");
    let (_, error) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("an outcome this code does not know is not an outcome");
    assert!(error.contains("quarantined"), "{error}");
    finish(pool).await;
}

#[tokio::test]
async fn a_negative_number_in_an_unsigned_column_is_reported() {
    const SCHEMA: &str = "nc_test_negative";
    // `unsigned()` — a negative value means a row was written by something that
    // cast rather than converted, which is exactly what the guard on the way out
    // exists to prevent.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    insert_raw(&pool, &"ab".repeat(32), "allowed", -1).await;
    let store = store_in(&pool, "nc_test_negative");
    let (_, error) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a negative instant is not one");
    assert!(error.contains("recorded_at"), "{error}");
    finish(pool).await;
}

#[tokio::test]
async fn an_actor_chain_that_is_not_an_array_is_reported() {
    const SCHEMA: &str = "nc_test_actors";
    // The last of the read guards. `actor_chain` is delegation, never
    // impersonation (`§14.6`) — the log shows both the agent and the operator —
    // so a column holding something that is not a list of them is a row whose
    // chain of responsibility cannot be read, and reading it as empty would
    // report an action as nobody's.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, hash) \
         VALUES (0, 'realm-1', 'principal-1', '\"not a list\"'::jsonb, 'Read', 'null', \
         'allowed', 1, $1)",
    )
    .bind("ab".repeat(32))
    .execute(&pool)
    .await
    .expect("the trigger refuses UPDATE and DELETE, not INSERT");

    let store = store_in(&pool, "nc_test_actors");
    let (_, error) = store
        .from(0)
        .await
        .expect("the store answers")
        .stopped_at
        .expect("a string is not an actor chain");
    assert!(error.contains("actor_chain"), "{error}");
    finish(pool).await;
}

#[tokio::test]
async fn a_sequence_past_the_signed_range_is_refused_rather_than_cast() {
    const SCHEMA: &str = "nc_test_range";
    // `signed()` — reachable from Rust alone: `entry.rs`'s own tamper list
    // already carries "recorded_at past the signed range". A cast would store a
    // negative number that sorts before every real entry.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_range");
    // **Through the sequence, which is the one `signed()` still answers.**
    // `recorded_at = u64::MAX` is refused by `readable_record` before the store
    // is reached, so a test driving it would not exercise the function it is
    // named for. Both messages say "BIGINT", so asserting only that would leave
    // reverting `signed()` to `Error::Unavailable` with the whole gate green.
    //
    // A sequence past the signed range reaches `signed()` on both paths, and
    // neither is the store failing to answer: `JournalUnavailable` is
    // `retryable`, so classified that way it would tell a responder to look at
    // PostgreSQL and to try again forever.
    let error = store
        .append(Entry::chain(u64::MAX, None, full_record()))
        .await
        .expect_err("a sequence PostgreSQL cannot hold is not one");
    assert!(error.to_string().contains("BIGINT"), "{error}");
    assert_eq!(
        error.code(),
        nc_catalog::ErrorCode::JournalUnstorable,
        "{error}"
    );
    assert!(!error.code().retryable(), "{error}");

    // And on the read path, where there is no record at all.
    let error = store
        .from(u64::MAX)
        .await
        .expect_err("a sequence PostgreSQL cannot hold is not one");
    assert_eq!(
        error.code(),
        nc_catalog::ErrorCode::JournalUnstorable,
        "{error}"
    );

    // The caller's own instant is refused too, before any store is asked — a
    // different rule, in `readable_record`, and the one that would intercept
    // this test.
    let mut record = full_record();
    record.recorded_at = u64::MAX;
    let error = store
        .append(Entry::chain(0, None, record))
        .await
        .expect_err("an instant PostgreSQL cannot hold is not one");
    assert!(error.to_string().contains("BIGINT"), "{error}");
    finish(pool).await;
}

#[tokio::test]
async fn an_entry_that_does_not_follow_the_tail_is_refused_with_both_numbers() {
    const SCHEMA: &str = "nc_test_out_of_order";
    // `Store::append`'s documented contract, which both stores enforce:
    // without the check, appending sequence 7 into an empty table is accepted,
    // and the chain reports it later as `HistoryDoesNotStartHere` — a different
    // problem, one step further from the caller who caused it.
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_out_of_order");
    let refused = store.append(Entry::chain(7, None, full_record())).await;
    assert_eq!(
        refused,
        Err(nc_journal::Error::OutOfOrder {
            expected: 0,
            found: 7
        }),
        "{refused:?}"
    );

    // And the entry that does follow is accepted, so the refusal is about the
    // sequence and not about appending.
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("sequence 0 follows an empty tail");
    finish(pool).await;
}

/// `access_records.recorded` is the database's to write, on every path in.
///
/// **Not the journal's subject, and this is where the shipped schema lives.**
/// `pool` applies the real `migrations/` into a schema of its own, which is what
/// this needs: `nc-prune`'s own suite creates tables of its own design and could
/// not see the trigger at all.
///
/// The column carries the pruner's idea of *oldest* (`NOCTUA.md §15.8`
/// and `catalog/limits.toml`), and `GENERATED ALWAYS AS IDENTITY` would refuse
/// a plain INSERT naming it and nothing else. A row planted at `i64::MAX`
/// through either of the other two paths sorts last for ever, is never pruned,
/// and enough of them stop the table ever coming under its limit while every
/// run reports ordinary.
#[tokio::test]
async fn the_pruners_order_is_the_databases_to_write() {
    let Some(pool) = pool("recorded_is_assigned").await else {
        return;
    };
    let put = |id: &'static str, recorded: i64, overriding: bool| {
        let pool = pool.clone();
        async move {
            let naming = if overriding {
                "OVERRIDING SYSTEM VALUE"
            } else {
                ""
            };
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO access_records (id, realm, principal, occurred_at, outcome, \
                 recorded) {naming} VALUES ($1, 'r', 'p', 1, 'ok', $2)"
            )))
            .bind(id)
            .bind(recorded)
            .execute(&pool)
            .await
        }
    };

    // A plain INSERT naming it, and the same INSERT with the keyword that used
    // to be the whole way past this.
    put("a", 42, false).await.expect("the row inserts");
    put("b", i64::MAX, true).await.expect("the row inserts");

    // And `COPY`, which needs no keyword at all: `-1` would take a slot of
    // every batch from then on.
    {
        let mut connection = pool.acquire().await.expect("a connection");
        let mut copying = connection
            .copy_in_raw(
                "COPY access_records (id, realm, principal, occurred_at, outcome, recorded) \
                 FROM STDIN",
            )
            .await
            .expect("the copy starts");
        copying
            .send(&b"c\tr\tp\t1\tok\t-1\n"[..])
            .await
            .expect("the row is sent");
        copying.finish().await.expect("the copy finishes");
        // Released here, deliberately: `finish` drops the schema, and a
        // connection still holding this table waits for ever rather than
        // failing.
    }

    let written: Vec<(String, i64)> = sqlx::query_as("SELECT id, recorded FROM access_records")
        .fetch_all(&pool)
        .await
        .expect("the rows read back");
    assert_eq!(
        written,
        vec![
            ("a".to_owned(), 1),
            ("b".to_owned(), 2),
            ("c".to_owned(), 3)
        ],
        "a caller's value reached the column the pruner orders by"
    );

    // And an UPDATE cannot renumber a row: one that could would make itself
    // immortal, which is the same thing said from the other end.
    sqlx::query("UPDATE access_records SET recorded = $1 WHERE id = 'c'")
        .bind(i64::MAX)
        .execute(&pool)
        .await
        .expect("the update runs");
    let after: i64 = sqlx::query_scalar("SELECT recorded FROM access_records WHERE id = 'c'")
        .fetch_one(&pool)
        .await
        .expect("the row reads back");
    assert_eq!(
        after, 3,
        "a row renumbered itself out of the pruner's reach"
    );

    // **And in replication mode, which `ENABLE ALWAYS` is the whole point of.**
    // `0006` ends with `ALTER TABLE access_records ENABLE ALWAYS TRIGGER …` and
    // gives the same three-word reasoning `0004` gives for the journal — and
    // each of the journal's three triggers is exercised through
    // `in_replication_mode`, so this one is too. Measured without this, with
    // that line deleted and the schema rebuilt: `nc-journal` and `nc-prune`
    // both green, while a caller in replication mode writes its own value into
    // the column the pruner orders by.
    //
    // `session_replication_role = replica` is what a logical-replication apply
    // worker runs as, and an `ORIGIN` trigger does not fire under it.
    in_replication_mode(
        &pool,
        "INSERT INTO access_records (id, realm, principal, occurred_at, outcome, recorded) \
         OVERRIDING SYSTEM VALUE VALUES ('d', 'r', 'p', 1, 'ok', 9223372036854775807)",
    )
    .await
    .expect("the row inserts");
    let replicated: i64 = sqlx::query_scalar("SELECT recorded FROM access_records WHERE id = 'd'")
        .fetch_one(&pool)
        .await
        .expect("the row reads back");
    assert_eq!(
        replicated, 4,
        "a caller in replication mode wrote its own value into the column the pruner \
         orders by: the trigger is `ORIGIN` rather than `ALWAYS`, so it did not fire"
    );

    finish(pool).await;
}

/// A row `head` cannot read is unreadable, not unavailable.
///
/// **The two error identities send a responder to two different places.**
/// `NC-JRNL-0003` is *"the journal store did not answer"* and is `retryable`;
/// `NC-JRNL-0001` is *"a row this code cannot read"* and is not. A `head`
/// building both of its decode failures as `Unavailable` hands a hash of the
/// wrong width — which is tampering, since nothing this code writes has one —
/// to a responder as a transient outage. Measured that way:
///
/// ```text
/// head() code    = JournalUnavailable        retryable = true
/// head() display = the journal store did not answer: `hash` holds 2 byte(s)
///                  … was not written by this code
/// ```
///
/// The sentence contradicts itself, which is the contradiction `reason` exists
/// to remove on the read path. `§15.1`: an error whose identity changes as it
/// crosses a boundary cannot be traced — and here it changes by *not* changing.
#[tokio::test]
async fn a_head_row_this_code_cannot_read_cannot_be_written_at_all() {
    let Some(pool) = pool("head_undecodable").await else {
        return;
    };
    let mut journal = journal(store_in(&pool, "head_undecodable"));
    journal
        .record(entry_record("Read"))
        .await
        .expect("the first entry lands");

    // **A hash that is not a digest, and the column refuses one.**
    // `ALTER TABLE … TYPE … USING` rewrites the heap past every row trigger,
    // which is how this file's other rewrite tests plant a poisoned row — and
    // `migrations/0009_a_hash_is_sixty_four_hex_characters.sql` constrains
    // `hash`, so the rewrite is refused by the constraint rather than by a
    // trigger. That is the guarantee the migration exists for: a `hash` this
    // code cannot read would lock the journal for ever and make the documented
    // anchor repair impossible.
    let refused = sqlx::query("ALTER TABLE journal_entries ALTER COLUMN hash TYPE text USING 'ab'")
        .execute(&pool)
        .await
        .expect_err("the column refuses a hash that is not one");
    assert!(
        refused.to_string().contains("journal_entries_hash_is_hex"),
        "the rewrite was refused by something other than the column's own rule: {refused}"
    );

    // And the reader still says which row and why, at its own seam, because a
    // column constraint is not a reason to stop being able to read a row that
    // got past one.
    let said = nc_journal::postgres::digest_for_tests("hash", "ab")
        .expect_err("a short hash is not a digest");
    assert!(said.contains("32"), "{said}");

    // **And `head()` cannot be made to answer `Undecodable` at all**, which is
    // what the migration buys. It reads `sequence` and `hash`: the first is a
    // `BIGINT` bounded top and bottom by `0003` and `0008`, the second is
    // sixty-four lowercase hex by `0009`. Both are refused by the database
    // rather than reported by the reader — so the outage such a row would cause
    // is unreachable, and that is the assertion.
    let head = journal
        .store()
        .head()
        .await
        .expect("a head this code wrote is readable")
        .expect("a history has a head");
    assert_eq!(head.0, 0, "the head moved");

    // The classification the reader still makes, at the seam where a row that
    // got past a constraint would arrive: tampering, not an outage, and not
    // something to retry.
    let failed = nc_journal::Error::Undecodable {
        sequence: 0,
        why: said,
    };
    assert_eq!(
        failed.code(),
        nc_catalog::ErrorCode::JournalChainBroken,
        "the code a responder queries by: a row this code did not write is the chain \
         being broken, not the store being unreachable"
    );
    assert!(
        !failed.code().retryable(),
        "tampering was reported as something to try again"
    );

    // **And that `head` still makes it**, which the two assertions above cannot
    // ask: they build the error by hand, and the reader's own arm is
    // unreachable behind `0009`'s constraint. Measured —
    // `.map_err(poisoned(sequence))` deleted from `head`'s digest read —
    // without this the whole gate stays green, and an unreadable head would
    // answer `NC-JRNL-0003`, *"the journal store did not answer"*, `retryable`,
    // which is a caller retrying a tampered row for ever.
    //
    // A source assertion, for the reason `nc-prune`'s
    // `the_over_deletion_guard_runs_before_the_commit` gives: the branch cannot
    // be reached at run time, and what matters is that the classification is
    // still written where the read happens.
    let source = include_str!("../src/postgres.rs");
    let reading = source
        .split_once("async fn head(")
        .expect("`head` is here")
        .1;
    let digest = reading.find("digest(\"hash\"").expect("it reads the hash");
    let arm = reading[digest..]
        .split_once(';')
        .expect("the statement ends")
        .0;
    assert!(
        arm.contains("poisoned("),
        "`head` reads the hash without classifying a failure as tampering: {arm}"
    );

    finish(pool).await;
}

/// A unit is opened once through the store, and both stores say so alike.
///
/// **The predicate is in the `INSERT`'s `WHERE`, and in the diagnosis.** With
/// zero rows resolved by asking `unit_closed` and nothing else, a second
/// opening bracket for an open unit would fall through to the ordering arm and
/// come back as
///
/// ```text
/// postgres = OutOfOrder { expected: 1, found: 1 }   "expected sequence 1 and was handed 1"
/// memory   = UnitAlreadyOpen { unit: … }            "work unit 2CZK0GV40400 was already open"
/// ```
///
/// — the self-contradicting sentence this crate refuses, and two
/// different `ErrorCode`s for one call. `Journal::append` answers correctly
/// because it asks `unit_opened` first; `Store::append` is public and has no
/// such path, which is `postgres.rs`'s own "a rule about one code path" again.
///
/// This also puts `Postgres::unit_opened` and the `action = $18` clause under
/// test. Without it, deleting the pre-check in `Journal::append` leaves the
/// whole suite green.
#[tokio::test]
async fn a_unit_is_opened_once_through_the_store_too() {
    const SCHEMA: &str = "nc_test_opened_once";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_opened_once");
    let unit = Minter::new(MachineFlake::from_value(7).expect("a flake"), SystemClock)
        .mint()
        .expect("a unit");

    let opening = |sequence: u64, at: u64| {
        let mut record = full_record();
        record.action = nc_journal::unit::OPENS.to_owned();
        record.result = Outcome::UnitBegun;
        record.work_unit = Some(unit);
        record.recorded_at = at;
        Entry::chain(sequence, None, record)
    };

    let first = opening(0, 1_700_000_000_000);
    let head = first.hash;
    store.append(first).await.expect("the first opening lands");

    // **Linked to the tail, which is what puts the question to the clause that
    // answers it.** Written unlinked, the `INSERT`'s prev-hash predicate
    // refuses the row before the open-unit predicate three lines below it is
    // ever evaluated, and the verdict then comes from `why_nothing_landed`
    // reading the table afterwards. The assertion below passes either way, so
    // the clause `AND NOT ($5 = $18 AND $7 = $19 AND EXISTS …)` — the one that
    // makes the check and the write a single statement, and the only reason two
    // concurrent openers cannot both land — would be held by nothing: deleting
    // it would leave an unlinked version of this test green. Linked, deleting
    // it fails here.
    let refused = store
        .append(Entry::chain(
            1,
            Some(head),
            opening(1, 1_700_000_000_001).record,
        ))
        .await
        .expect_err("a unit is opened once");
    assert!(
        matches!(refused, nc_journal::Error::UnitAlreadyOpen { .. }),
        "the store reported the wrong refusal: {refused:?}"
    );
    // The in-memory store answers the same call the same way, which is the rule
    // `postgres.rs` states about the two of them.
    let mut memory = nc_journal::memory::InMemory::default();
    let first = opening(0, 1_700_000_000_000);
    let linked = {
        let mut second = opening(1, 1_700_000_000_001);
        // **Linked to the first**, because the store checks that too: an entry
        // that does not follow the tail is refused before anything about work
        // units is asked, which is the door `readable` leaves open one field
        // along from the hash.
        second = Entry::chain(1, Some(first.hash), second.record);
        second
    };
    memory.append(first).await.expect("the first opening lands");
    let in_memory = memory
        .append(linked)
        .await
        .expect_err("a unit is opened once");
    assert_eq!(
        refused.code(),
        in_memory.code(),
        "the two stores give different codes for one call: {refused:?} and {in_memory:?}"
    );

    // The floor: exactly one opening is in the table, so the refusal above is a
    // refusal and not a second row nobody looked for.
    let openings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE work_unit = $1 AND action = $2",
    )
    .bind(unit.to_crockford())
    .bind(nc_journal::unit::OPENS)
    .fetch_one(&pool)
    .await
    .expect("the table answers");
    assert_eq!(openings, 1, "a second opening bracket landed");

    finish(pool).await;
}

/// Sixteen callers opening one work unit are told one thing, not two.
///
/// **Which arm the scheduler puts a caller in must not decide what it is
/// told.** The refused `INSERT` comes back either as zero rows or as a unique
/// violation, and with only the first arm asking about the work unit, for
/// sixteen identical calls nine were told `UnitAlreadyOpen` (`NC-JRNL-0004`,
/// severity error) and six were told `OutOfOrder`, which mapped
/// to `NC-JRNL-0001`: *the journal's hash chain is broken*, severity
/// critical. One call, two codes, chosen by timing, and one of them the
/// platform's loudest alarm about a chain that was perfectly intact.
///
/// Safety is not in question — exactly one opening lands either way. What this
/// pins is the diagnosis.
#[tokio::test]
async fn one_call_gets_one_answer_however_the_scheduler_orders_it() {
    fn opening(unit: Snowflake) -> Entry {
        let mut record = full_record();
        record.action = nc_journal::unit::OPENS.to_owned();
        record.result = Outcome::UnitBegun;
        record.work_unit = Some(unit);
        Entry::chain(0, None, record)
    }

    const SCHEMA: &str = "nc_test_one_answer";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Minter::new(MachineFlake::from_value(11).expect("a flake"), SystemClock)
        .mint()
        .expect("a unit");
    let mut racers = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        racers.push(tokio::spawn(async move {
            store_in(&pool, "nc_test_one_answer")
                .append(opening(unit))
                .await
        }));
    }
    let mut landed = 0_usize;
    let mut codes = std::collections::BTreeSet::new();
    for racer in racers {
        match racer.await.expect("the task ran") {
            Ok(()) => landed += 1,
            Err(refused) => {
                // **The error beside the code.** Both codes map from more than
                // one `Error`, so a message naming only the two codes would say
                // "two different error codes" and leave a reader to work out
                // which arm produced which. The error is what names the arm:
                // `OutOfOrder { expected: 1, found: 0 }`, translated out of the
                // density trigger's message by `unavailable`, is on a path that
                // never asks about the work unit.
                codes.insert(format!("{:?} <- {refused:?}", refused.code()));
            }
        }
    }
    assert_eq!(landed, 1, "a unit is opened once, whatever the timing");
    assert_eq!(
        codes.len(),
        1,
        "sixteen identical calls were given {} different error codes: {codes:?}",
        codes.len()
    );
    // And it is the one that says what happened, not the one that says the
    // audit chain is broken.
    assert!(
        !codes.iter().any(|code| code.contains("JournalChainBroken")),
        "a second opening bracket was reported as a broken hash chain: {codes:?}"
    );

    // **And again, four more times, because one round of sixteen catches a
    // defect here at roughly one run in ten.** The third arm of the same
    // `INSERT` is the one at stake: `0012`'s density trigger fires when the
    // `WHERE` passed and the tail moved before the row landed, and translating
    // its message — "sequence 0 is not the next one, which is 1" — straight into
    // `OutOfOrder` never asks about the work unit. The zero-rows arm and the
    // unique-violation arm both diagnose; an arm that does not is untouched by
    // any change to the diagnosis, because it never reaches it.
    //
    // A test that catches a race one run in ten is a test people learn to
    // re-run. Every racer in these rounds is refused — the unit is open and
    // stays open — so each round is sixteen more passes through the three arms,
    // and the assertion is that all of them say the same permanent thing.
    for round in 0..4_u64 {
        let mut racers = Vec::new();
        for _ in 0..16 {
            let pool = pool.clone();
            racers.push(tokio::spawn(async move {
                let mut store = store_in(&pool, "nc_test_one_answer");
                let at = match store.head().await {
                    Ok(Some((sequence, _))) => sequence + 1,
                    Ok(None) => 0,
                    Err(error) => return Err(error),
                };
                let mut record = full_record();
                record.action = nc_journal::unit::OPENS.to_owned();
                record.result = Outcome::UnitBegun;
                record.work_unit = Some(unit);
                store.append(Entry::chain(at, None, record)).await
            }));
        }
        let mut again = std::collections::BTreeSet::new();
        for racer in racers {
            match racer.await.expect("the task ran") {
                Ok(()) => panic!("round {round}: a unit was opened twice"),
                Err(refused) => {
                    again.insert(format!("{:?} <- {refused:?}", refused.code()));
                }
            }
        }
        assert_eq!(
            again.len(),
            1,
            "round {round}: sixteen identical calls were given {} different error codes: \
             {again:?}",
            again.len()
        );
        assert!(
            again
                .iter()
                .all(|code| code.starts_with("JournalUnitAlreadyOpen")),
            "round {round}: an opener of an open unit was told something retrying \
             clears: {again:?}"
        );
    }

    let openings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE work_unit = $1 AND action = $2",
    )
    .bind(unit.to_crockford())
    .bind(nc_journal::unit::OPENS)
    .fetch_one(&pool)
    .await
    .expect("the table answers");
    assert_eq!(openings, 1, "a second opening bracket landed");

    finish(pool).await;
}

/// A race between ordinary writers is contention, not a broken chain.
///
/// **Contention is not a broken chain.** `head()` then `append()` is exactly
/// what `Journal::append` does; run it from twelve threads and three land.
/// Measured with the refusal misread: every one of the nine refusals carries
/// `NC-JRNL-0001` — *the journal's hash chain is broken*, severity critical,
/// `retryable = false` — for a chain that is intact, so nine of twelve
/// ordinary appends lose their audit entry on an instruction not to retry. A
/// writer of one entry per command from more than one task would meet it at
/// three quarters of its commands.
#[tokio::test]
async fn a_race_between_writers_is_contention_and_says_so() {
    const SCHEMA: &str = "nc_test_contention";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut racers = Vec::new();
    for _ in 0..12 {
        let pool = pool.clone();
        racers.push(tokio::spawn(async move {
            let mut store = store_in(&pool, "nc_test_contention");
            // **Linked to the tail this racer read, which is what
            // `Journal::append` does.** Passing `None` in place of the hash
            // `head()` returns makes a racer whose `head()` lands *after*
            // another racer's append compute sequence 1 and then build an entry
            // at 1 linking to nothing, which is a genuinely broken link and not
            // contention at all. The store diagnoses it correctly as
            // `Undecodable`/`JournalChainBroken`, and a blanket assertion
            // would call that a defect:
            //
            // ```text
            // a refused race says JournalChainBroken, and a caller acting on it
            // gives up: Undecodable { sequence: 1, why: "this entry does not
            // link to the tail it would follow…" }
            // ```
            //
            // Measured on a pristine tree without this: 3 of 20 runs alone, and
            // 3 of 11 full `cargo xtask check` runs — the gate itself, red
            // about a quarter of the time, for a defect in its own fixture.
            // This file's own doc calls `head()` then `append()` "exactly what
            // `Journal::append` does", and so this does it.
            let (at, previous) = match store.head().await {
                Ok(Some((sequence, hash))) => (sequence + 1, Some(hash)),
                Ok(None) => (0, None),
                Err(error) => return Err(error),
            };
            store
                .append(Entry::chain(at, previous, full_record()))
                .await
        }));
    }
    let mut refusals = Vec::new();
    for racer in racers {
        if let Err(refused) = racer.await.expect("the task ran") {
            refusals.push(refused);
        }
    }
    assert!(
        !refusals.is_empty(),
        "nothing raced, so this test proves nothing about racing"
    );
    for refused in &refusals {
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::JournalContended,
            "a refused race says {:?}, and a caller acting on it gives up: {refused:?}",
            refused.code()
        );
        assert!(
            refused.code().retryable(),
            "a race is the one failure retrying fixes, and this one says do not"
        );
        // **And the sentence is not the same number twice.** The diagnosis runs
        // after the refused statement and asks each of the `WHERE`'s three
        // checks again; when another connection has settled all three in
        // between, none of them answers, and an arm reporting
        // `OutOfOrder { expected: free, found: entry.sequence }` — reached only
        // when those two are equal — would tell a responder *the journal
        // expected sequence 3 and was handed 3*. Measured under these racers:
        // 11 of 400 rounds. That case is `Contended`, and this holds the shape
        // rather than the frequency, because a race reproduces when it feels
        // like it.
        if let nc_journal::Error::OutOfOrder { expected, found } = refused {
            assert_ne!(
                expected, found,
                "a refusal read `{refused}`, which is the same number twice and says \
                 nothing happened"
            );
        }
    }

    // Both sentences a contended write can carry are true of one, whether or not
    // this run produced it: a floor on the arm above, which is otherwise proven
    // only when the scheduler happens to hit it.
    assert_eq!(
        nc_journal::Error::Contended.code(),
        nc_catalog::ErrorCode::JournalContended,
        "the arm for a refusal whose reason was already undone does not say retry"
    );
    let said = nc_journal::Error::Contended.to_string();
    assert!(
        said.contains("nothing wrong") && said.contains("retry"),
        "a caller is told what happened and what to do about it: {said}"
    );

    finish(pool).await;
}

/// A forged tail is tampering, not contention — and the advice does not cement it.
///
/// **A retry must not finish a forgery.** `OutOfOrder` carries `NC-JRNL-0006`,
/// which says retry, because a write race is not a broken chain; the same arm
/// answers a caller whose tail was *replaced*, and retrying there finishes the
/// forgery. Measured without the recomputation below: a well-formed row with a
/// hash that does not match its contents, inserted at the caller's sequence —
///
/// ```text
/// append -> OutOfOrder   NC-JRNL-0006  Warning  retryable = true
/// verify -> Broken { sequence: 2, reason: HashDoesNotMatchContents }  NC-JRNL-0002
/// the retry -> Ok(())          the honest entry now chains onto the forged hash
/// ```
///
/// One rule reads the history as compromised and the other tells the caller to
/// extend it. The refusal path recomputes the tail's own hash before it calls
/// anything contention.
#[tokio::test]
async fn a_forged_tail_is_not_contention_and_retrying_does_not_bless_it() {
    const SCHEMA: &str = "nc_test_forged_tail";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_forged_tail");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry lands");
    let head = store
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");

    // A row this code did not write: every column valid, the hash not its own.
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (1, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{}', 'allowed', 0, 1, \
         $1, repeat('a', 64))",
    )
    .bind(nc_canon::to_hex(&head.1))
    .execute(&pool)
    .await
    .expect("a second writer can insert");

    let refused = store
        .append(Entry::chain(1, Some(head.1), full_record()))
        .await
        .expect_err("the sequence is taken");
    assert_ne!(
        refused.code(),
        nc_catalog::ErrorCode::JournalContended,
        "a forged tail was reported as contention, and the retry that code asks for \
         chains onto it: {refused:?}"
    );
    assert!(
        !refused.code().retryable(),
        "tampering was reported as retryable: {refused:?}"
    );

    finish(pool).await;
}

/// An honest entry never chains onto a forged one, and `record` says so.
///
/// **The hash guard is not only on the refusal path.** A tamperer need never
/// provoke a refusal: with a forged but decodable row at the tail and no race
/// at all, a `record` guarded only there returns `Ok` and chains the next
/// honest entry onto it — measured, `the entry landed on the forgery at 3`. The
/// path that succeeds is asked as well as the refusal path.
#[tokio::test]
async fn an_honest_entry_never_chains_onto_a_forged_one() {
    const SCHEMA: &str = "nc_test_never_chains";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut journal = journal(store_in(&pool, "nc_test_never_chains"));
    journal
        .record(full_record())
        .await
        .expect("the first entry lands");
    let head = store_in(&pool, "nc_test_never_chains")
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");

    // A row this code can read, whose hash is not the hash of its contents.
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (1, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{}', 'allowed', 0, 1, \
         $1, repeat('a', 64))",
    )
    .bind(nc_canon::to_hex(&head.1))
    .execute(&pool)
    .await
    .expect("a second writer can insert");

    let refused = journal
        .record(full_record())
        .await
        .expect_err("an honest entry does not extend a forged history");
    assert!(
        matches!(refused, nc_journal::Error::Undecodable { .. }),
        "the append succeeded onto a forgery, or said the wrong thing: {refused:?}"
    );
    assert!(
        !refused.code().retryable(),
        "chaining onto a forgery was reported as something to retry: {refused:?}"
    );

    // The floor: exactly two rows, so the refusal is a refusal.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the table answers");
    assert_eq!(rows, 2, "the honest entry landed anyway");

    finish(pool).await;
}

/// One ordinary `INSERT` cannot make the table unwritable for ever.
///
/// **A row at `i64::MAX` leaves no representable sequence after it.** In the
/// statement's own arithmetic `$1 = MAX(sequence) + 1` overflows `BIGINT`, and
/// computed outside it the refusal only moves one layer out:
/// `MAX(sequence) + 1` is `2^63`, which `signed()` refuses, so every append
/// answers
///
/// ```text
/// Unstorable { field: "sequence", why: "it is 9223372036854775808, which
///              PostgreSQL's BIGINT cannot hold" }
/// ```
///
/// at every sequence, for ever. `DELETE`, `UPDATE` and `TRUNCATE` are refused
/// by this table's own triggers and no command in `catalog/commands.toml` can
/// undo it, so recovery means DDL on the audit log — destroying the evidence to
/// get the platform back — and `§14.9`'s fail-closed rule makes that a total
/// outage from one statement.
///
/// So the row cannot exist: `0007` bounds the top of the column, leaving
/// exactly the one number the invariant needs.
#[tokio::test]
async fn a_row_can_only_land_where_the_next_one_goes() {
    const SCHEMA: &str = "nc_test_range_end";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_range_end");
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry lands");
    let head = store
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");

    // **Every value but the next one**, which is the whole rule. The top of the
    // column's range and the value one below `0008`'s bound are here because
    // each of them, planted, would end the history: a bound says which values
    // are legal and nothing about which comes next, so the row that bricks the
    // table is always the one the bound permits.
    for at in [
        "9223372036854775807",
        "4611686018427387903",
        "4611686018427387904",
        "7",
        "0",
    ] {
        let refused = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
             resource, result, policy_epoch, recorded_at, prev_hash, hash) \
             VALUES ({at}, 'realm-1', 'attacker', '[]'::jsonb, 'Escalate', '{{}}', \
             'allowed', 0, 1, $1, repeat('a', 64))"
        )))
        .bind(nc_canon::to_hex(&head.1))
        .execute(&pool)
        .await
        .expect_err("a row lands where the next one goes and nowhere else");
        assert!(
            refused.to_string().contains("is not the next one"),
            "a row at {at} was refused by something other than the density rule: {refused}"
        );
    }

    // **And the next one lands**, so this is a rule about where and not a ban on
    // writing — and it is the value an operator's anchor is written at, which is
    // what keeps the repair `0009_a_hash_is_sixty_four_hex_characters.sql`
    // documents available.
    store
        .append(Entry::chain(1, Some(head.1), full_record()))
        .await
        .expect("the next sequence lands");

    // The refusal a caller reads is about the record and not about the store:
    // a class-23 violation is the row being refused, and reporting it as the
    // store not answering sends a responder to a database that is working.
    let past_the_end = sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, policy_epoch, recorded_at, prev_hash, hash) \
         VALUES (99, 'realm-1', 'x', '[]'::jsonb, 'W', '{}', 'allowed', 0, 1, NULL, \
         repeat('c', 64))",
    )
    .execute(&pool)
    .await
    .expect_err("the density rule refuses it");
    // **And it reads as ordering, not as the store being down.** The trigger
    // fires only where the statement's own `WHERE` passed — an `INSERT … SELECT
    // … WHERE` produces no row otherwise — so on the shipped path it means the
    // tail moved between the check and the row landing, which is a race. The
    // two numbers are in the message because the migration writes them there
    // for this.
    let carried = nc_journal::postgres::unavailable_for_tests(&past_the_end);
    assert_eq!(
        carried.code(),
        nc_catalog::ErrorCode::JournalSequenceAhead,
        "a row past the end of the history was reported as {:?}: {carried}",
        carried.code()
    );
    assert!(
        matches!(
            carried,
            nc_journal::Error::OutOfOrder {
                expected: 2,
                found: 99
            }
        ),
        "the numbers the trigger wrote did not reach the caller: {carried:?}"
    );

    finish(pool).await;
}

/// A forged link is refused by the statement, in both stores, with one code.
///
/// **`readable` verifies an entry against itself and says nothing about what it
/// claims to follow.** `Entry::chain(n, Some([0xAA; 32]), record)` is
/// self-consistent, so read by `readable` alone it lands through the public
/// `Store::append` — no raw SQL, no feature flag — and `verify` then condemns
/// the history for ever at that row while `Journal::record` keeps answering
/// `Ok`, because the forged entry's own hash *is* its own hash. `InMemory`
/// refuses the same call and says why; a door closed in one store has to be
/// closed in both.
///
/// The whole refusal matrix is here rather than the one case, because the
/// failure is two implementations of one documented method disagreeing.
#[tokio::test]
async fn a_forged_link_is_refused_and_both_stores_say_the_same_thing() {
    const SCHEMA: &str = "nc_test_forged_link";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut in_postgres = store_in(&pool, "nc_test_forged_link");
    let mut in_memory = nc_journal::InMemory::default();
    for store in 0..2 {
        let first = Entry::chain(0, None, full_record());
        if store == 0 {
            in_postgres.append(first).await.expect("the first lands");
        } else {
            in_memory.append(first).await.expect("the first lands");
        }
    }
    let head = in_postgres
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");

    for (about, entry) in [
        ("a forged link at the right sequence", {
            Entry::chain(1, Some([0xAA; 32]), full_record())
        }),
        // **The genesis entry, which a link check written `if let Some(tail)`
        // skips.** In that form the check does not run when there is no tail —
        // so an entry claiming to follow a hash that never existed is accepted
        // in memory as the first row of a history, and `verify` answers
        // `Broken { sequence: 0, LinkDoesNotMatchPredecessor }` for ever;
        // PostgreSQL's statement refuses it and its diagnosis calls it
        // `Contended`, `retryable = true`, for a forgery no retry clears. Run
        // against a history that has rows, this case is only reachable through
        // the sequence — which is why the empty-table half is below.
        ("no link at all at the right sequence", {
            Entry::chain(1, None, full_record())
        }),
        ("a forged link and a stale sequence", {
            Entry::chain(7, Some([0xAA; 32]), full_record())
        }),
    ] {
        let from_postgres = in_postgres
            .append(entry.clone())
            .await
            .expect_err(&format!("{about} was accepted by PostgreSQL"));
        let from_memory = in_memory
            .append(entry)
            .await
            .expect_err(&format!("{about} was accepted in memory"));
        assert_eq!(
            format!("{from_postgres:?}"),
            format!("{from_memory:?}"),
            "{about}: the two stores answer one call two ways"
        );
        assert_eq!(
            from_postgres.code(),
            from_memory.code(),
            "{about}: the two stores answer one call two codes"
        );
    }

    // The floor: the honest entry still lands, so the rule above is a rule and
    // not a ban on writing.
    in_postgres
        .append(Entry::chain(1, Some(head.1), full_record()))
        .await
        .expect("an entry that links to the tail lands");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the table answers");
    assert_eq!(rows, 2, "a forged link landed");

    finish(pool).await;
}

/// Sixteen openers of one unit are told the answer no retry clears.
///
/// **When the permanent and the transient answer are both true, the permanent
/// one is reported.** A unit that is open stays open; a sequence moves. With
/// `why_nothing_landed` asking the sequence first, twelve to fifteen of every
/// sixteen concurrent openers receive `JournalContended`, `retryable = true`,
/// severity warning — for a condition retrying never clears. Measured across
/// five rounds of sixteen: 5/10, 0/15, 1/14, 3/12 and 2/13 split between two
/// codes, decided by the scheduler.
///
/// `one_call_gets_one_answer_however_the_scheduler_orders_it` asserts one code
/// per run; this asserts **which** code, which is what that test could not see.
#[tokio::test]
async fn a_second_opening_bracket_is_told_the_thing_retrying_will_not_fix() {
    const SCHEMA: &str = "nc_test_which_answer";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Minter::new(MachineFlake::from_value(13).expect("a flake"), SystemClock)
        .mint()
        .expect("a unit");
    let opening = move || {
        let mut record = full_record();
        record.action = nc_journal::unit::OPENS.to_owned();
        record.result = Outcome::UnitBegun;
        record.work_unit = Some(unit);
        Entry::chain(0, None, record)
    };
    let mut racers = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        racers.push(tokio::spawn(async move {
            store_in(&pool, "nc_test_which_answer")
                .append(opening())
                .await
        }));
    }
    let mut refusals = Vec::new();
    for racer in racers {
        if let Err(refused) = racer.await.expect("the task ran") {
            refusals.push(refused);
        }
    }
    assert_eq!(refusals.len(), 15, "one opening lands and fifteen do not");
    for refused in &refusals {
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::JournalUnitAlreadyOpen,
            "an opener of an open unit was told {:?}, {}: {refused}",
            refused.code(),
            if refused.code().retryable() {
                "which says retry a condition that never clears"
            } else {
                "which is not what happened"
            }
        );
        assert!(
            !refused.code().retryable(),
            "a unit that is already open does not become closed by asking again"
        );
    }

    // And the code's own sentence is about a unit that is open, not one that
    // finished: `JournalUnitClosed` says "already committed or aborted", and
    // both locales tell the reader to start the action again — which is the
    // operation that was just refused.
    assert_ne!(
        nc_catalog::ErrorCode::JournalUnitAlreadyOpen,
        nc_catalog::ErrorCode::JournalUnitClosed,
        "one code cannot describe a unit that is open and one that has finished"
    );

    finish(pool).await;
}

/// A sequence past the end of the history is not another writer's doing.
///
/// **`JournalContended` says *another writer took this sequence
/// first*.** Writing only moves the end of a history forward, so an entry
/// numbered past it is nobody else's doing — and a caller must not be told it
/// was, with `retryable = true`, on an empty table with nobody racing:
///
/// ```text
/// Store::append(sequence 7) -> OutOfOrder { expected: 0, found: 7 }
///   NC-JRNL-0006  Warning  retryable = true
///   "Another writer took this sequence first, so the entry was not written"
///   retry 0, 1, 2 -> the same
/// ```
#[tokio::test]
async fn a_sequence_past_the_end_is_not_reported_as_someone_else_racing() {
    const SCHEMA: &str = "nc_test_ahead";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, "nc_test_ahead");
    let mut in_memory = nc_journal::InMemory::default();

    let ahead = Entry::chain(7, None, full_record());
    let from_postgres = ahead.clone();
    let refused = store
        .append(from_postgres)
        .await
        .expect_err("an empty history does not begin at 7");
    assert_eq!(
        refused.code(),
        nc_catalog::ErrorCode::JournalSequenceAhead,
        "nobody was racing and the caller was told somebody was: {refused}"
    );
    assert!(
        !refused.code().retryable(),
        "retrying does not move the end of a history backwards"
    );
    assert_eq!(
        format!("{:?}", in_memory.append(ahead).await.expect_err("likewise")),
        format!("{refused:?}"),
        "the two stores answer one call two ways"
    );

    // And the other direction still says retry, because a retry is what fixes
    // it: the tail moved on while this caller held a number read before it.
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("the first entry lands");
    let behind = store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect_err("sequence 0 is taken");
    assert_eq!(
        behind.code(),
        nc_catalog::ErrorCode::JournalContended,
        "a stale sequence is what a race looks like: {behind}"
    );
    assert!(behind.code().retryable());

    finish(pool).await;
}

/// The first entry of a history does not follow a hash that never existed.
///
/// **Written `if let Some(tail)`, neither link check runs when there is no
/// tail.** Measured on an empty table in that form:
///
/// ```text
/// InMemory::append(Entry::chain(0, Some([0xAA; 32]), record)) -> Ok(())
///   verify(0) -> Broken { sequence: 0, LinkDoesNotMatchPredecessor }   for ever
/// Postgres::append(the same call)  -> Err(Contended)
///   "found nothing wrong … Read the tail again and retry"   NC-JRNL-0006, retryable
/// ```
///
/// One store accepts a forgery as the first row of an audit log; the other
/// refuses it in the statement and then tells the caller to try again. The
/// genesis entry's link is `None`, and that is as much a fact about the tail as
/// any other.
#[tokio::test]
async fn a_first_entry_does_not_follow_a_hash_that_never_existed() {
    const SCHEMA: &str = "nc_test_genesis_link";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut in_postgres = store_in(&pool, "nc_test_genesis_link");
    let mut in_memory = nc_journal::InMemory::default();
    let genesis = Entry::chain(0, Some([0xAA; 32]), full_record());

    let from_memory = in_memory
        .append(genesis.clone())
        .await
        .expect_err("a first entry does not follow a hash that never existed");
    let from_postgres = in_postgres
        .append(genesis)
        .await
        .expect_err("a first entry does not follow a hash that never existed");
    assert_eq!(
        format!("{from_postgres:?}"),
        format!("{from_memory:?}"),
        "the two stores answer one call two ways"
    );
    assert_eq!(from_postgres.code(), from_memory.code());
    assert!(
        !from_postgres.code().retryable(),
        "a forged genesis link was reported as something a retry fixes: {from_postgres}"
    );

    // The floor: an honest genesis entry still lands.
    in_postgres
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("a first entry that links to nothing lands");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries")
        .fetch_one(&pool)
        .await
        .expect("the table answers");
    assert_eq!(rows, 1, "a forged genesis link landed");

    finish(pool).await;
}

/// Reopening a **closed** unit says it is closed, not that it is open.
///
/// **The opening bracket stays in the table when a unit is committed** — it is
/// what says the unit began — so "has this unit been opened" answers `true` for
/// one that has finished. Asked first for an opening bracket, both stores would
/// answer `NC-JRNL-0007`: *"A work unit that is already open was opened
/// again"*, and in both locales *"Finish or cancel it before starting it
/// again"* — about something that already finished, and naming the two
/// operations that are no longer possible.
///
/// Measured with `unit_opened` and `unit_closed` both `true` on one unit.
#[tokio::test]
async fn reopening_a_closed_unit_says_it_is_closed() {
    const SCHEMA: &str = "nc_test_reopen_closed";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Minter::new(MachineFlake::from_value(17).expect("a flake"), SystemClock)
        .mint()
        .expect("a unit");
    let bracket = |sequence: u64, previous: Option<[u8; 32]>, action: &str, result: Outcome| {
        let mut record = full_record();
        record.action = action.to_owned();
        record.result = result;
        record.work_unit = Some(unit);
        Entry::chain(sequence, previous, record)
    };

    let mut in_postgres = store_in(&pool, "nc_test_reopen_closed");
    let mut in_memory = nc_journal::InMemory::default();
    for store in 0..2 {
        let mut previous = None;
        for (sequence, action, result) in [
            (0, nc_journal::unit::OPENS, Outcome::UnitBegun),
            (1, nc_journal::unit::CLOSES, Outcome::UnitCommitted),
        ] {
            let entry = bracket(sequence, previous, action, result);
            previous = Some(entry.hash);
            if store == 0 {
                in_postgres.append(entry).await.expect("the bracket lands");
            } else {
                in_memory.append(entry).await.expect("the bracket lands");
            }
        }
    }

    // The unit really is both, which is what made the wrong answer reachable.
    assert!(
        in_postgres
            .unit_closed(&full_record().realm, unit)
            .await
            .expect("the store answers"),
        "the unit did not close, so this test proves nothing"
    );

    let head = in_postgres
        .head()
        .await
        .expect("the store answers")
        .expect("a history has a head");
    let reopening = bracket(2, Some(head.1), nc_journal::unit::OPENS, Outcome::UnitBegun);
    let from_postgres = in_postgres
        .append(reopening.clone())
        .await
        .expect_err("a closed unit does not reopen");
    let from_memory = in_memory
        .append(reopening)
        .await
        .expect_err("a closed unit does not reopen");
    assert_eq!(
        format!("{from_postgres:?}"),
        format!("{from_memory:?}"),
        "the two stores answer one call two ways"
    );
    assert_eq!(
        from_postgres.code(),
        nc_catalog::ErrorCode::JournalUnitClosed,
        "a closed unit was reported as one that is open: {from_postgres}"
    );

    finish(pool).await;
}

/// An entry that is wrong two ways answers with the more severe one.
///
/// **A forged link inside a closed unit must not come back `UnitClosed`**
/// — `NC-JRNL-0004`, severity **error**, "that action finished" —
/// from PostgreSQL and `Undecodable` — `NC-JRNL-0001`, **critical**, the chain
/// being broken — from memory. One call, two variants, two codes, and the store
/// that runs would be the one downgrading the forgery to a bracket problem.
#[tokio::test]
async fn an_entry_wrong_two_ways_answers_with_the_more_severe_one() {
    {
        const SCHEMA: &str = "nc_test_forged_in_a_closed_unit";
        let Some(pool) = pool(SCHEMA).await else {
            return;
        };
        let unit = Minter::new(MachineFlake::from_value(23).expect("a flake"), SystemClock)
            .mint()
            .expect("a unit");
        let bracket = |sequence: u64, previous: Option<[u8; 32]>, action: &str, result| {
            let mut record = full_record();
            record.action = String::from(action);
            record.result = result;
            record.work_unit = Some(unit);
            Entry::chain(sequence, previous, record)
        };
        let mut in_postgres = store_in(&pool, "nc_test_forged_in_a_closed_unit");
        let mut in_memory = nc_journal::InMemory::default();
        for store in 0..2 {
            let mut previous = None;
            for (sequence, action, result) in [
                (0, nc_journal::unit::OPENS, Outcome::UnitBegun),
                (1, nc_journal::unit::CLOSES, Outcome::UnitCommitted),
            ] {
                let entry = bracket(sequence, previous, action, result);
                previous = Some(entry.hash);
                if store == 0 {
                    in_postgres.append(entry).await.expect("the bracket lands");
                } else {
                    in_memory.append(entry).await.expect("the bracket lands");
                }
            }
        }
        let mut record = full_record();
        record.work_unit = Some(unit);
        let forged = Entry::chain(2, Some([0xAA; 32]), record);
        let from_postgres = in_postgres
            .append(forged.clone())
            .await
            .expect_err("a forged link in a closed unit is refused");
        let from_memory = in_memory
            .append(forged)
            .await
            .expect_err("a forged link in a closed unit is refused");
        assert_eq!(
            format!("{from_postgres:?}"),
            format!("{from_memory:?}"),
            "an entry wrong two ways: the two stores answer one call two ways"
        );
        assert_eq!(
            from_postgres.code(),
            nc_catalog::ErrorCode::JournalChainBroken,
            "a forged link was reported as a bracket problem: {from_postgres}"
        );
        finish(pool).await;
    }
}

/// A temporary table of the journal's own name does not become the journal.
///
/// **The store's statements name `journal_entries` unqualified, so the path is
/// pinned.** The migrations close that class inside trigger *bodies*; unpinned,
/// the store's own reads and writes are decided by whoever plants a name
/// first, and `pg_temp` is searched ahead of `search_path` for relations — the
/// fact those migrations are built on. Measured with the store's statements
/// copied out of the source:
///
/// ```text
/// CREATE TEMP TABLE journal_entries (LIKE public.journal_entries);
/// INSERT INTO journal_entries (…) SELECT 0, … WHERE 0-1 = COALESCE((SELECT MAX(sequence) …
/// INSERT 0 1                  <- rows_affected() == 1, so `append` returned Ok
/// SELECT … FROM public.journal_entries;
///  0 | p | aaaa…              <- the audit log never received it
/// ```
///
/// No trigger fires, because the triggers are on the real table; the writer
/// believes it has appended and `head`/`tail` agree with it, so `§14.9`'s
/// fail-closed rule never engages. `nc-prune` asks both of these questions
/// about its own tables, and the journal — where it matters most — asks them
/// too.
#[tokio::test]
async fn a_temporary_table_of_the_same_name_is_not_the_journal() {
    const SCHEMA: &str = "nc_test_temp_shadow";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    // One connection, because a temporary relation belongs to a session — which
    // is also why this is a real hazard for a service holding a long-lived one.
    // The name `pool` actually made: it suffixes with the process id so two
    // concurrent runs cannot destroy each other's tables.
    let named = format!("{SCHEMA}_{}", std::process::id());
    let alone = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            std::env::var("DATABASE_URL")
                .expect("checked by pool()")
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("a valid url")
                .options([("search_path", named.as_str())]),
        )
        .await
        .expect("connects into the schema");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TEMP TABLE journal_entries (LIKE \"{named}\".journal_entries)"
    )))
    .execute(&alone)
    .await
    .expect("a temporary table is ours to make");
    let mut store = store_in(&alone, "nc_test_temp_shadow");
    let record = entry_record("Read");
    store
        .append(Entry::chain(0, None, record))
        .await
        .expect("the first entry lands");

    // In the journal, not in the temporary table beside it.
    let real: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM \"{named}\".journal_entries"
    )))
    .fetch_one(&pool)
    .await
    .expect("readable");
    let shadow: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_temp.journal_entries")
        .fetch_one(&alone)
        .await
        .expect("readable");
    assert_eq!(
        (real, shadow),
        (1, 0),
        "the entry went to the temporary table: the writer believes it appended, \
         `head` and `tail` agree with it, and the audit log never received it"
    );
    // **And the trigger's own pin, which the assertion above cannot reach.**
    // The store pins the path for its own statements, so its append resolves to
    // the real table whether or not the *function* is pinned — which is why,
    // without this, removing `SET search_path FROM CURRENT` from `0012`'s
    // density trigger leaves both integration suites green, and why the shadow
    // above being *empty* makes no difference either: it answers the same
    // maximum as the journal.
    //
    // So this half is a writer the store has never touched, on its own
    // connection, with its own populated shadow. The temporary schema is
    // searched first on such a connection, so an unpinned trigger reads the
    // shadow's maximum and refuses the row that belongs in the journal.
    let unpinned = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            std::env::var("DATABASE_URL")
                .expect("checked by pool()")
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("a valid url")
                .options([("search_path", named.as_str())]),
        )
        .await
        .expect("connects into the schema");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TEMP TABLE journal_entries (LIKE \"{named}\".journal_entries)"
    )))
    .execute(&unpinned)
    .await
    .expect("a temporary table is ours to make");
    sqlx::query(
        "INSERT INTO pg_temp.journal_entries (sequence, realm, principal, actor_chain, \
         action, resource, result, recorded_at, hash) \
         VALUES (4611686018427387902, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', \
         '{}', 'allowed', 1, repeat('a', 64))",
    )
    .execute(&unpinned)
    .await
    .expect("the shadow is ours to fill");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO \"{named}\".journal_entries (sequence, realm, principal, \
         actor_chain, action, resource, result, recorded_at, hash) \
         VALUES (1, 'realm-1', 'principal-1', '[]'::jsonb, 'Read', '{{}}', 'allowed', \
         2, repeat('b', 64))"
    )))
    .execute(&unpinned)
    .await
    .expect(
        "the density trigger read the temporary table beside the journal: it is the \
         next sequence in the journal that decides, and this writer does not pin its \
         own path",
    );
    unpinned.close().await;

    alone.close().await;
    finish(pool).await;
}

/// A store that answers and refuses is not a store that did not answer.
///
/// **Three different systems, not reported identically.** Without this, a
/// revoked `INSERT`, a dropped table and a stopped container all reach a caller as
/// `NC-JRNL-0003` — *"the journal store did not answer"*, **critical**,
/// `retryable = true` — and `§14.9` makes the journal fail closed, so the
/// platform stops on any of them while the only instruction anybody gets is
/// *try again in a moment*:
///
/// ```text
/// INSERT revoked (42501)  permission denied for table journal_entries
/// table dropped (42P01)   relation "journal_entries" does not exist
/// the store stopped       pool timed out while waiting for a connection
/// ```
///
/// `nc-prune` separates these too — `NC-DB-0005` is "refused, and retrying will
/// not change it". This is the journal's, and it is what tells a responder
/// which system to go and look at.
#[tokio::test]
async fn a_write_the_store_refuses_is_not_the_store_being_down() {
    const SCHEMA: &str = "nc_test_refused";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let role = format!("nc_refused_{}", std::process::id());
    // A role that may look and may not write: the shape a deployment gets wrong
    // on its first day.
    for statement in [
        format!("CREATE ROLE \"{role}\" NOLOGIN"),
        format!("GRANT \"{role}\" TO CURRENT_USER"),
        format!(
            "GRANT USAGE ON SCHEMA \"{SCHEMA}_{}\" TO \"{role}\"",
            std::process::id()
        ),
        format!(
            "GRANT SELECT ON ALL TABLES IN SCHEMA \"{SCHEMA}_{}\" TO \"{role}\"",
            std::process::id()
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("the test's own role");
    }

    let plain = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL").expect("the suite has one"))
        .await
        .expect("connects");
    for statement in [
        format!("SET search_path TO \"{SCHEMA}_{}\"", std::process::id()),
        format!("SET ROLE \"{role}\""),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&plain)
            .await
            .expect("ours to set on our own connection");
    }

    let mut store = store_in(&plain, SCHEMA);
    let refused = store
        .append(Entry::chain(0, None, entry_record("Read")))
        .await
        .expect_err("a role with no INSERT cannot append");
    assert_eq!(
        refused.code(),
        nc_catalog::ErrorCode::JournalRefused,
        "a store that answered and refused was reported as one that did not answer: \
         {refused:?}"
    );
    assert!(
        !refused.code().retryable(),
        "a missing grant was reported as something to try again, and the journal fails \
         closed — so the platform stops and the advice is to wait"
    );
    // **PostgreSQL's sentence, without where in its own source it was raised**,
    // which sqlx 0.9 appends to the error's `Display` — see `told`.
    let sentence = refused.to_string();
    assert!(
        sentence.contains("permission denied") && !sentence.contains(" at line "),
        "the refusal names a line of PostgreSQL's C source as if it were one of the \
         statement's: {sentence}"
    );

    plain.close().await;
    for statement in [
        format!("DROP OWNED BY \"{role}\""),
        format!("DROP ROLE IF EXISTS \"{role}\""),
    ] {
        let _ = sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await;
    }
    finish(pool).await;
}

/// A store that cannot read its table refuses the unit questions rather than
/// answering them.
///
/// **Each of the three could answer "no" to a failure with every other suite
/// green.** `close_unit` asks `unit_opened` before it writes, and `!= Ok(true)`
/// in place of its `?` reports a store that could not answer as a unit nobody
/// opened: `JournalUnitNeverOpened`, which is not retryable and sends a person
/// to whoever set up their Noctua, in place of the store's own
/// refusal. `Postgres::unit_closing` with its error mapped to `None`, and the
/// provided `unit_closed` with its error flattened away, each answer "not
/// closed" for a table that is not there — each plant alone
/// leaving `cargo test -p nc-journal` green without this. The kernel
/// asks `unit_closing` before an undo reverts anything and acts on its answer
/// in recovery, so "could not read" must never read as "there is no closing
/// bracket".
///
/// **A schema that does not exist, on a pool of its own.** An undefined table
/// is SQLSTATE class 42, which `unavailable` maps to `Refused`. The store pins
/// its search path on the session it borrows, so on the suite's pool that path
/// stays on a pooled connection, and `finish` asks that pool where it is.
#[tokio::test]
async fn a_store_that_cannot_read_refuses_the_unit_questions_rather_than_answering_them() {
    const SCHEMA: &str = "nc_test_unit_questions_refused";
    /// A schema nothing creates, so a store over it cannot read its table.
    const NOWHERE: &str = "nc_no_such_schema";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let unit = Snowflake::from_crockford("01J8XK2M9P68").expect("a unit");
    let elsewhere = PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL").expect("`pool` found one"))
        .await
        .expect("connects");
    // The premise, said out loud: a schema by this name holding a journal
    // would make every answer below an ordinary one.
    let absent: bool =
        sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
            .bind(NOWHERE)
            .fetch_one(&elsewhere)
            .await
            .expect("the catalog answers");
    assert!(
        absent,
        "`{NOWHERE}` exists, so this asks a store that can read"
    );

    let nowhere = Postgres::in_schema(elsewhere.clone(), NOWHERE);
    let realm = id("realm-1");
    // Each answer as the text it would have printed, so the three share a type.
    let said = |answer: &dyn core::fmt::Debug| format!("{answer:?}");
    for (asked, answer) in [
        (
            "unit_closing",
            nowhere.unit_closing(&realm, unit).await.map(|it| said(&it)),
        ),
        (
            "unit_closed",
            nowhere.unit_closed(&realm, unit).await.map(|it| said(&it)),
        ),
        (
            "unit_opened",
            nowhere.unit_opened(&realm, unit).await.map(|it| said(&it)),
        ),
    ] {
        let refused = match answer {
            Ok(answered) => panic!(
                "`{asked}` answered `{answered}` over a store that could not read its \
                 table, as though it had read it"
            ),
            Err(refused) => refused,
        };
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::JournalRefused,
            "`{asked}` answered a store that could not read its table with something \
             other than its refusal: {refused:?}"
        );
    }
    let mut over_nothing = journal(nowhere.clone());
    let refused = over_nothing
        .close_unit(unit, Closing::Aborted)
        .await
        .expect_err("a close over a store that cannot read is refused");
    assert_eq!(
        refused.code(),
        nc_catalog::ErrorCode::JournalRefused,
        "a store that could not answer was reported as a unit nobody opened: {refused:?}"
    );
    elsewhere.close().await;
    finish(pool).await;
}

#[tokio::test]
/// A schema named after the connecting role does not become the journal.
///
/// **The store's path does not begin `"$user"`.** Appending `pg_temp` to the
/// caller's path moves the temporary schema and leaves the other half of the
/// rule alone — and `$user` is not a schema, it is an instruction to look for
/// one named after whoever connects, expanded afresh on every
/// statement. Measured through the shipped entry point on a pool built with no
/// options, with the caller's path kept:
///
/// ```text
/// session search_path = "$user", public
/// append  -> Ok(())
/// public.journal_entries = 0,  noctua.journal_entries = 1
/// tail    -> Ok(true)
/// ```
///
/// `LIKE … INCLUDING ALL` copies constraints and indexes and not triggers, so
/// nothing refused the row, the writer believed it had appended, and the audit
/// log never received it.
async fn a_schema_named_after_the_role_does_not_become_the_journal() {
    const SCHEMA: &str = "nc_test_role_schema";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let named = format!("{SCHEMA}_{}", std::process::id());
    // The role this connects as, so the decoy is what `"$user"` would find.
    // **A role of this run's own, not the connecting one.** Naming the decoy
    // after `current_user` and then dropping it `CASCADE` destroys whatever a
    // deployment keeps there — measured, a pre-existing `noctua.keepme` holding
    // three rows is gone after one run — and two concurrent runs collide on it
    // in three rounds of five. `finish`'s own paragraph says a guessing
    // `DROP SCHEMA` has no undo and that every schema this suite makes carries
    // the process id; this role and its schema carry it too.
    //
    // `SET ROLE` makes `"$user"` resolve to this name for the length of the
    // session, which is the condition the test needs, without touching a name
    // anybody else uses.
    let role = format!("nc_role_probe_{}", std::process::id());
    for statement in [
        format!("DROP ROLE IF EXISTS \"{role}\""),
        format!("CREATE ROLE \"{role}\" NOLOGIN"),
        format!("GRANT \"{role}\" TO CURRENT_USER"),
        // A fresh role can enter neither `public` nor the schema this run makes
        // for it, and `current_schemas` silently drops what it cannot enter — so
        // without these the resolved path is empty and the test measures nothing.
        format!("GRANT USAGE ON SCHEMA public TO \"{role}\""),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("a scratch role is ours to make on our own stack");
    }
    for statement in [
        format!("DROP SCHEMA IF EXISTS \"{role}\" CASCADE"),
        format!("CREATE SCHEMA \"{role}\""),
        format!("GRANT USAGE ON SCHEMA \"{role}\" TO \"{role}\""),
        format!(
            "CREATE TABLE \"{role}\".journal_entries \
             (LIKE \"{named}\".journal_entries INCLUDING ALL)"
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("a scratch schema is ours to make on our own stack");
    }

    // A pool with **no options at all**, exactly as a deployment builds one, so
    // its path is the server default and begins `"$user"`.
    let plain = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL").expect("checked by pool()"))
        .await
        .expect("connects");
    // **The condition is established, not asserted about the caller.** Reading
    // the *server default* would fail when `DATABASE_URL` names a schema —
    // which is the very form `in_schema` and `configured_schema` exist to
    // support, so `cargo xtask check` could not pass against it. The path is
    // set here instead, and `SET ROLE` makes `"$user"` resolve to the scratch
    // role.
    for statement in [
        "SET search_path TO \"$user\", public",
        &format!("SET ROLE \"{role}\""),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&plain)
            .await
            .expect("ours to set on our own connection");
    }
    let path: String = sqlx::query_scalar("SELECT current_setting('search_path')")
        .fetch_one(&plain)
        .await
        .expect("readable");
    assert!(
        path.contains("$user"),
        "the path was set to `{path}`, so the decoy schema is not in front of anything"
    );

    let mut store = Postgres::new(plain.clone());
    let landed = store
        .append(Entry::chain(0, None, entry_record("Read")))
        .await;
    // **Why it was refused, asserted.** With `landed` bound and then only
    // interpolated into the message below, `Postgres::new`'s `"public"` could
    // become any other schema name and this test go on passing: measured,
    // `Self::in_schema(pool, "nowhere")` with the whole gate green. This is the
    // shipped default constructor's only caller in the tree, and checking only
    // where the row did *not* go is satisfied just as well by a store aimed at
    // a schema that does not exist.
    //
    // The role owns the decoy schema and holds no grant on the real journal, so
    // *permission denied* is the answer only a store that reached
    // `public.journal_entries` can get. A schema that is not there answers
    // `relation "…" does not exist`, and a store that used the search path would
    // have succeeded into the decoy.
    let why = format!("{landed:?}");
    assert!(
        why.contains("permission denied"),
        "the default store did not reach `public.journal_entries`: {why}"
    );
    let decoy: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM \"{role}\".journal_entries"
    )))
    .fetch_one(&pool)
    .await
    .expect("readable");
    assert_eq!(
        decoy, 0,
        "the entry went to a schema named after the connecting role: {landed:?}"
    );

    plain.close().await;
    for statement in [
        format!("DROP SCHEMA \"{role}\" CASCADE"),
        // Every grant made to the role is an object that depends on it, and
        // enumerating them by hand is a list that grows with the test. `DROP
        // OWNED BY` removes what this role owns and what was granted to it, in
        // this database, which is exactly the set this run created.
        format!("DROP OWNED BY \"{role}\""),
        format!("DROP ROLE IF EXISTS \"{role}\""),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("ours to drop, because this run made it");
    }
    finish(pool).await;
}

/// Every SQLSTATE class `unavailable` names decides something.
///
/// **Both class-based arms, pinned.** `unavailable_for_tests`
/// is `#[doc(hidden)] pub` purely so a test can reach the mapping, and a caller
/// that builds an error hitting the *first* arm — the trigger-message parse —
/// asserts only that. Measured without this, with both predicates disabled
/// (`starts_with("40")` → `"zz40"`, `"22"`/`"23"` → `"zz22"`/`"zz23"`), so
/// every serialization failure, deadlock, data exception and constraint
/// violation falls through to `Unavailable`: `check passed`.
///
/// The whole point of the mapping is that a database error which is the
/// *record's* fault does not reach a responder as `NC-JRNL-0003 "The journal
/// store did not answer"` — a sentence about a store that answered instantly.
#[tokio::test]
async fn each_sqlstate_class_reaches_the_error_it_is_written_for() {
    const SCHEMA: &str = "nc_test_sqlstate_classes";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    // Each row is a statement this database really refuses, the class it refuses
    // it with, and what a caller must be told. Driven through the database rather
    // than through a hand-built error, so the classes are PostgreSQL's own.
    for (statement, expected) in [
        // 22003, numeric_value_out_of_range — a data exception.
        (
            "SELECT (9223372036854775807::BIGINT + 1)",
            nc_catalog::ErrorCode::JournalUnstorable,
        ),
        // 22P02, invalid_text_representation.
        (
            "SELECT 'not a number'::BIGINT",
            nc_catalog::ErrorCode::JournalUnstorable,
        ),
        // **Class 40, which rows of class 22 and 23 do not reach.** With only
        // those, `starts_with("40")` is unpinned: changed to `"zz40"`,
        // `check passed`. A serialization failure then goes back to
        // `NC-JRNL-0003 "The journal store did not answer"`, critical and
        // retryable, about a store that answered instantly.
        //
        // `40P01` is a deadlock and needs two sessions; `40001` is raised
        // directly by this statement under any isolation, which is what makes it
        // usable as a row here.
        (
            "DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '40001'; END $$",
            nc_catalog::ErrorCode::JournalContended,
        ),
        // And a deadlock, which is the other member of the class.
        (
            "DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '40P01'; END $$",
            nc_catalog::ErrorCode::JournalContended,
        ),
        // 23514, check_violation — an integrity constraint.
        (
            "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
             resource, result, recorded_at, hash) VALUES (-1, 'r', 'p', '[]'::jsonb, 'a', \
             'res', 'ok', 1, repeat('a', 64))",
            nc_catalog::ErrorCode::JournalUnstorable,
        ),
        // **And the four classes the third arm names, every one reached.** That
        // arm lists `42`, `28`, `3D` and `3F`, and the rows above stop at class
        // 40 — so without these three of the four are decided by nothing: each
        // prefix in turn replaced with one that matches no SQLSTATE leaves the
        // crate suite green with a database up, and a store that answered
        // *permission denied* or *no such schema* goes back to
        // `NC-JRNL-0003 "The journal store did not answer"`, retryable, which
        // asks a person to keep trying something that will refuse identically
        // for ever.
        //
        // 42P01, undefined_table — a table reference no search path reaches,
        // which is what a dropped table and a schema-qualified name into a
        // schema that is not there both answer with. Measured: the missing
        // *schema* is this class rather than `3F`, because the reference is
        // resolved as a relation name.
        (
            "SELECT 1 FROM \"nc_no_such_schema\".journal_entries",
            nc_catalog::ErrorCode::JournalRefused,
        ),
        // 3F000, invalid_schema_name — the schema named where a relation is
        // being created, which is where PostgreSQL reports the schema itself.
        // The statement creates nothing: it is refused before it does.
        (
            "CREATE TABLE \"nc_no_such_schema\".t (x INT)",
            nc_catalog::ErrorCode::JournalRefused,
        ),
        // 28000 and 3D000 are raised rather than provoked, and that is the
        // honest note: the store reaches the first by connecting as a role it
        // may not use and the second by naming a database that is not there,
        // both of which happen before a statement runs and neither of which
        // this suite may arrange without changing the cluster's roles. What is
        // pinned here is the mapping, which is what the arm decides.
        (
            "DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '28000'; END $$",
            nc_catalog::ErrorCode::JournalRefused,
        ),
        (
            "DO $$ BEGIN RAISE EXCEPTION USING ERRCODE = '3D000'; END $$",
            nc_catalog::ErrorCode::JournalRefused,
        ),
    ] {
        let refused = sqlx::query(statement)
            .execute(&pool)
            .await
            .expect_err("this statement is refused");
        let mapped = nc_journal::postgres::unavailable_for_tests(&refused);
        assert_eq!(
            mapped.code(),
            expected,
            "`{statement}` was refused and came back as {mapped:?}, which is a sentence \
             about a store that answered"
        );
    }
    finish(pool).await;
}

/// Both CHECK constraints are on the table, and each says the number it was written with.
///
/// **The behaviour tests could not tell a constraint from a trigger.**
/// `a_negative_sequence_cannot_reach_the_table_at_all` asserts
///
/// ```rust,ignore
/// refused.contains("sequence_is_not_negative") || refused.contains("is not the next one")
/// ```
///
/// and the density trigger from `0010`/`0012` satisfies the right-hand side on
/// its own — so without this, replacing both `ALTER TABLE … ADD CONSTRAINT`
/// statements with `SELECT 1;` leaves `cargo test --workspace` green, and so
/// does lowering the ceiling from `4611686018427387904` to `1000`. The same is
/// true of `a_row_can_only_land_where_the_next_one_goes`, which asserts only
/// the trigger's sentence.
///
/// A constraint is a schema fact, and a behaviour assertion cannot separate it
/// from another rule that refuses the same row first. So this asks the catalog:
/// both are present, on this table, and each carries its own number. `check`
/// does notice a *edited* migration — but with `an applied migration was edited`,
/// a checksum drift guard, which says nothing about what the statement contained
/// and nothing at all about a fresh database.
#[tokio::test]
async fn the_table_carries_both_of_its_own_check_constraints() {
    const SCHEMA: &str = "nc_test_constraints";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    // The migrations run into this suite's own schema, so the table is here.
    let found: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.conname::text, pg_catalog.pg_get_constraintdef(c.oid) \
         FROM pg_constraint c \
         JOIN pg_class t ON t.oid = c.conrelid \
         JOIN pg_namespace n ON n.oid = t.relnamespace \
         WHERE t.relname = 'journal_entries' AND n.nspname = current_schema() \
           AND c.contype = 'c' ORDER BY c.conname",
    )
    .fetch_all(&pool)
    .await
    .expect("the catalog answers");

    for (name, holds) in [
        // The floor under `sequence`, which is a `BIGINT` carrying a `u64`: a
        // row at -1 is invisible to `from` (which filters `>= $1`) and to `tail`
        // (which orders descending), and cannot be deleted.
        ("journal_entries_sequence_is_not_negative", "sequence >= 0"),
        // And the ceiling, which is `i64::MAX / 2`: `tail` computes the next
        // sequence, and a history that reached the top would have nowhere to
        // put it. The number is written out here because lowering it is
        // otherwise green.
        (
            "journal_entries_ends_far_from_the_top",
            // PostgreSQL renders the literal back with its type, so the number
            // is what is asserted rather than the whole clause.
            "4611686018427387904",
        ),
    ] {
        let definition = found.iter().find(|(found, _)| found == name).map_or_else(
            || {
                panic!(
                    "`{name}` is not on `journal_entries`. The constraints present are \
                     {found:#?}"
                )
            },
            |(_, definition)| definition.as_str(),
        );
        assert!(
            definition.contains(holds),
            "`{name}` is `{definition}` and this test says it must hold `{holds}`"
        );
    }
    finish(pool).await;
}

/// Every racing test in this file fits in the pool it races through.
///
/// **With a pool of two against fan-outs of sixteen, sixteen and twelve**, a
/// refused `append` acquires three times, so sixteen racers are forty-eight
/// acquisitions through two slots; under load the pool times out and the append
/// comes back `JournalUnavailable`, which is neither of the codes those tests
/// assert. Measured: both fail that way on an unmutated tree while other builds
/// share the machine, and both pass alone — the shape of a harness defect
/// reported as a defect in the thing under test.
///
/// Read out of this file's own source rather than restated, because a number
/// restated is a number that drifts: a test spawning twenty racers would
/// otherwise reintroduce it silently, and the failure it produces does not point
/// here.
#[test]
fn every_racing_test_fits_in_the_pool() {
    let source = include_str!("in_postgres.rs");
    let mut spawned = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let Some(count) = line
            .trim()
            .strip_prefix("for _ in 0..")
            .and_then(|rest| rest.strip_suffix(" {"))
            .and_then(|digits| digits.parse::<u32>().ok())
        else {
            continue;
        };
        // A loop that spawns is one whose next few lines hand work to the
        // runtime; a plain counting loop is not, and there are several here.
        let races = source
            .lines()
            .skip(index)
            .take(4)
            .any(|after| after.contains("tokio::spawn"));
        if races {
            spawned.push((index + 1, count));
        }
    }
    // The floor: if the shape of these loops ever changes, this reads nothing
    // and passes by having found nothing to judge.
    assert!(
        spawned.len() >= 3,
        "only {} racing loop(s) were found in this file, so this proves nothing \
         about the pool: {spawned:?}",
        spawned.len()
    );
    let widest = spawned
        .iter()
        .map(|(_, count)| *count)
        .max()
        .expect("checked above");
    assert_eq!(
        widest, THE_LARGEST_FAN_OUT,
        "a test spawns {widest} concurrent append(s) and the pool holds \
         {THE_LARGEST_FAN_OUT}: the ones that do not fit come back \
         `JournalUnavailable`, which is a sentence about the journal for a fault \
         that is the harness. The loops found were {spawned:?}"
    );
}

/// The permanent answer wins over the transient one, whatever the sequence says.
///
/// **Driven directly, because the arm it pins is only reachable under a race.**
/// `0012`'s density trigger fires when the `INSERT`'s `WHERE` passed and the
/// tail moved before the row landed — and an `unavailable` translating its
/// message, "sequence 0 is not the next one, which is 1", straight into
/// `OutOfOrder` never asks about the work unit. That is `JournalContended`:
/// `retryable = true`, severity *warning*, for a condition no retry clears.
///
/// It is the third arm of the same statement: the zero-rows arm and the
/// unique-violation arm both diagnose. Rewriting the diagnosis to read every
/// fact in one snapshot changes nothing for an arm that never reaches the
/// diagnosis. Measured with it translated, `cargo xtask check` is red in 4 of 8
/// runs; the sixteen-way race with five rounds still catches the regression
/// only about one run in six, which is not a test.
///
/// So this asks the diagnosis the question the trigger's arm routes to
/// it.
#[tokio::test]
async fn an_open_unit_outranks_the_sequence_whatever_refused_the_write() {
    const SCHEMA: &str = "nc_test_permanent_wins";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    let unit = Minter::new(MachineFlake::from_value(19).expect("a flake"), SystemClock)
        .mint()
        .expect("a unit");
    let opening = |sequence: u64| {
        let mut record = full_record();
        record.action = nc_journal::unit::OPENS.to_owned();
        record.result = Outcome::UnitBegun;
        record.work_unit = Some(unit);
        Entry::chain(sequence, None, record)
    };
    store
        .append(opening(0))
        .await
        .expect("the first opening lands");

    // The state a loser of the race is in: its entry is an opening bracket for a
    // unit that is now open, and the sequence it computed is taken.
    let refused = store
        .why_nothing_landed_for_tests(&opening(0))
        .await
        .expect_err("the write was refused, so the diagnosis says why");
    assert!(
        matches!(refused, nc_journal::Error::UnitAlreadyOpen { .. }),
        "the sequence is stale and the unit is open; the diagnosis chose the answer \
         retrying clears: {refused:?}"
    );
    assert!(
        !refused.code().retryable(),
        "`{}` tells a caller to retry opening a unit that is already open",
        refused.code().code()
    );

    // The floor: with the unit question out of the way, the same call about a
    // stale sequence really does answer `OutOfOrder` — so the assertion above is
    // about which answer won and not about the only answer there is.
    let mut plain = full_record();
    plain.work_unit = None;
    let stale = Entry::chain(0, None, plain);
    let refused = store
        .why_nothing_landed_for_tests(&stale)
        .await
        .expect_err("sequence 0 is taken");
    assert!(
        matches!(
            refused,
            nc_journal::Error::OutOfOrder {
                expected: 1,
                found: 0
            }
        ),
        "an entry outside any unit at a taken sequence is out of order: {refused:?}"
    );
    finish(pool).await;
}

/// Drops every role this suite's family makes whose process is gone.
///
/// **A role outlives the database it was used in.** `DROP SCHEMA … CASCADE`
/// takes the tables and leaves the role; the role is cluster-global, so it is
/// still there for every other database and every other checkout. Measured on a
/// developer's cluster: seven `nc_role_probe_*` roles from runs whose pids no
/// longer existed.
///
/// The same rule as the schema sweep: a name this family makes, a pid that
/// parses, and a process that is not running. `DROP OWNED BY` first, because a
/// grant made to a role is an object that depends on it.
async fn sweep_roles_from_dead_runs(pool: &sqlx::PgPool) {
    // Every prefix a test in this family names a role with. Written out, and
    // held to the two suites by `every_role_prefix_this_family_uses_is_swept`.
    const ROLE_PREFIXES: [&str; 3] = ["nc_role_probe_", "nc_refused_", "nc_prune_probe_"];
    let mine = std::process::id().to_string();
    for prefix in ROLE_PREFIXES {
        let left: Vec<String> =
            sqlx::query_scalar("SELECT rolname::text FROM pg_roles WHERE rolname LIKE $1 || '%'")
                .bind(prefix)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        for role in left {
            let Some(pid) = role
                .split('_')
                .next_back()
                .filter(|pid| *pid != mine)
                .filter(|pid| pid.parse::<u32>().is_ok())
            else {
                continue;
            };
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                continue;
            }
            for statement in [
                format!("DROP OWNED BY \"{role}\""),
                format!("DROP ROLE IF EXISTS \"{role}\""),
            ] {
                let _ = sqlx::query(sqlx::AssertSqlSafe(&*statement))
                    .execute(pool)
                    .await;
            }
        }
    }
}

/// Drops schemas this suite left behind in runs that are no longer running.
///
/// **A failing test skips `finish`, and the schema stays for ever.** `pool()`
/// drops its own pid-suffixed name at the start, which cleans up a *re-run* of
/// the same process and nothing else — so every red run leaves one schema per
/// test, in a shared database. Measured on a developer's stack after one
/// session: 121 of them, every one from a process long gone.
///
/// The liveness question is asked here rather than in SQL, because the database
/// cannot ask it — and asking it is what separates a leftover from a concurrent
/// run's schema. A `DROP SCHEMA` that guesses has no undo, and `finish`'s own
/// paragraph says what a guess reaches: the developer's `public` schema.
async fn sweep_schemas_from_dead_runs(pool: &sqlx::PgPool, prefix: &str) {
    let left: Vec<String> =
        sqlx::query_scalar("SELECT nspname::text FROM pg_namespace WHERE nspname LIKE $1 || '%'")
            .bind(prefix)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    let mine = std::process::id().to_string();
    for schema in left {
        // **The pid is where the builders put it, which is not the end.**
        // `pool` names a schema `{label}_{pid}` and `a_schema_named` names one
        // `{prefix}_{pid}_{label}` — the pid is at the front there on purpose,
        // so `NAMEDATALEN` truncation cannot eat it, and a test pins that. Read
        // with `rsplit('_')`, `nc_test_two_stores_<pid>_a_forged_link` yields
        // `link`, fails to parse, and sweeps nothing: measured, three schemas
        // planted from a dead pid and one full suite run afterwards leave two
        // of them standing. The first numeric segment is the pid under both
        // shapes — a label is words, and the one that begins with a digit
        // (`2nd_try`) does not parse as a number.
        let Some(pid) = schema
            .split('_')
            .find(|part| part.parse::<u32>().is_ok())
            .filter(|pid| *pid != mine)
        else {
            continue;
        };
        // Only a name this suite makes: a numeric suffix, and a process that is
        // not running. Anything else is somebody else's and is left alone.
        if pid.parse::<u32>().is_err() || std::path::Path::new(&format!("/proc/{pid}")).exists() {
            continue;
        }
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"
        )))
        .execute(pool)
        .await;
    }
}

/// Every schema name this suite builds keeps its process id.
///
/// **PostgreSQL truncates an identifier at 63 bytes without saying so**, so a
/// pid placed last is lost from the one name long enough — and two concurrent
/// runs then share a schema, each `DROP SCHEMA … CASCADE` destroying the
/// other's mid-test. Measured: 100 % of paired runs started together fail with
/// `3F000: no schema has been selected to create in`, about 10 % with a stagger.
///
/// The labels are read out of the script table rather than restated, so a script
/// added with a longer name is checked by this without anyone remembering to add
/// it — remembering is the failure mode this closes.
#[test]
fn no_schema_name_loses_its_process_id_to_truncation() {
    let source = include_str!("in_postgres.rs");
    // Every string literal handed to `a_schema_named`, plus the longest label
    // any script in this file uses. A label is prose in a table; the names are
    // what the server sees.
    let mut labels: Vec<&str> = source
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|rest| rest.split_once('"'))
        .map(|(text, _)| text)
        .filter(|text| text.len() > 20 && text.contains(' '))
        .collect();
    labels.push("a label longer than any script here would reasonably be, and then some");
    assert!(
        labels.len() > 3,
        "only {} label(s) were read out of this file, so this proves little",
        labels.len()
    );
    for label in labels {
        let name = a_schema_named("nc_test_two_stores", label);
        assert!(
            name.len() <= 63,
            "`{name}` is {} bytes and PostgreSQL cuts at 63 without saying so",
            name.len()
        );
        assert!(
            name.contains(&std::process::id().to_string()),
            "`{name}` lost its process id, so two concurrent runs share it"
        );
    }

    // And two different labels that agree for their first sixty-three bytes
    // still make two different schemas, because the pid is not the part that is
    // cut — which is the property the ordering buys.
    let one = a_schema_named("nc_test_two_stores", &"x".repeat(200));
    let other = a_schema_named("nc_test_two_stores", &format!("{}y", "x".repeat(199)));
    assert_eq!(
        one, other,
        "two labels this long are one schema, deliberately"
    );
    assert!(one.contains(&std::process::id().to_string()));
}

/// Every role name this family builds begins with a prefix the sweeper matches.
///
/// **A role outlives its schema and its database.** Roles have the same shape
/// as the schemas the sweep removes, and they accumulate the same way —
/// measured on one developer's cluster, 121 such schemas and seven
/// `nc_role_probe_*` roles from runs whose processes were long gone — visible
/// to every database on the cluster and to every other checkout sharing it.
///
/// Read from where the names are *built*, not from the `CREATE ROLE` statements:
/// both suites interpolate a variable into the statement, and the literal that
/// decides what a sweeper can match is the `format!` above it. Both files,
/// because `nc-prune` makes one too and the cluster does not care which crate a
/// role came from.
#[test]
fn every_role_prefix_this_family_uses_is_swept() {
    const ROLE_PREFIXES: [&str; 3] = ["nc_role_probe_", "nc_refused_", "nc_prune_probe_"];
    let mut built = 0_usize;
    for source in [
        include_str!("in_postgres.rs"),
        include_str!("../../nc-prune/tests/in_postgres.rs"),
    ] {
        for line in source.lines() {
            let trimmed = line.trim();
            // A role name is built next to the word `role` or `named`, and the
            // template is the literal that follows.
            if !(trimmed.starts_with("let role = ") || trimmed.starts_with("let named = ")) {
                continue;
            }
            let Some((_, rest)) = trimmed.split_once('"') else {
                continue;
            };
            let template = rest.split('"').next().unwrap_or("");
            // `{SCHEMA}_…` builds a *schema* name; the schema sweep covers those.
            if template.starts_with('{') {
                continue;
            }
            assert!(
                ROLE_PREFIXES
                    .iter()
                    .any(|prefix| template.starts_with(prefix)),
                "`{template}` names a role and no prefix in `ROLE_PREFIXES` matches it, so \
                 a run that dies leaves it on the cluster for every other database"
            );
            built += 1;
        }
    }
    assert!(
        built >= 3,
        "only {built} role name(s) were read, and this family builds more"
    );
}

/// Every `pool` label in this file begins with a prefix the sweeper matches.
///
/// **The list is a hand list, and this is what ties it to the file.** The
/// sweeper matches by prefix; a label that begins otherwise is a schema nothing
/// removes when the run that made it dies. Measured without `SWEPT`: three
/// schemas planted from a dead pid, one full suite run, two left standing.
#[test]
fn every_prefix_this_suite_uses_is_swept() {
    let source = include_str!("in_postgres.rs");
    let mut checked = 0_usize;
    for (at, _) in source.match_indices("pool(\"") {
        let label = source[at + "pool(\"".len()..]
            .split('"')
            .next()
            .expect("a closing quote");
        assert!(
            SWEPT.iter().any(|prefix| label.starts_with(prefix)),
            "`pool(\"{label}\")` makes a schema no prefix in `SWEPT` matches, so a run \
             that dies leaves it behind for ever"
        );
        checked += 1;
    }
    // And the constant-named ones, which are the other fifty call sites.
    for (at, _) in source.match_indices("const SCHEMA: &str = \"") {
        let label = source[at + "const SCHEMA: &str = \"".len()..]
            .split('"')
            .next()
            .expect("a closing quote");
        assert!(
            SWEPT.iter().any(|prefix| label.starts_with(prefix)),
            "`{label}` is a schema name no prefix in `SWEPT` matches"
        );
        checked += 1;
    }
    // The floor: a reader that found no call sites would pass by finding none.
    assert!(checked > 20, "only {checked} schema name(s) were read");
}

/// The `set_config` statements a migration writes, in the order it writes them.
///
/// **Comments go before the split, not after it.** `0012` draws its whole
/// sequence in an ASCII diagram, and one of those comment lines carries a `;` —
/// so splitting first and dropping comment *lines* afterwards would leave the
/// statement under that diagram inside a chunk that began mid-comment, and the
/// save this file's restore reads silently not replayed. The failure presents
/// as `unrecognized configuration parameter "nc.path_before_0012"`, which sends
/// a reader to the migration rather than to the reader.
///
/// Split on `;` because a statement may be written over more than one line and
/// `0011`'s epilogue is. A fragment inside a `$$` body is discarded by the same
/// filter, since no function in `migrations/` calls `set_config`.
fn the_path_statements_of(sql: &str) -> Vec<String> {
    let code: String = sql
        .lines()
        .map(|line| line.split("--").next().unwrap_or_default())
        .collect::<Vec<&str>>()
        .join("\n");
    code.split(';')
        .filter_map(|chunk| {
            let statement = chunk.trim().to_owned();
            statement
                .starts_with("SELECT set_config(")
                .then(|| format!("{statement};"))
        })
        .collect()
}

/// A migration that changes the search path for its own length puts it back.
///
/// **The restore, measured.** `0011` and `0012` each widen or replace the
/// session's `search_path` so that the functions they create resolve names the
/// way those functions must, and each ends with a statement that restores
/// it. Measured without this, `0012`'s epilogue replaced
/// by `SELECT set_config('search_path', current_setting('search_path'), false);`
/// — a shape the static gate must clear, because it reads the path and writes
/// the path, and it is a no-op:
///
/// ```text
///   BEFORE            "$user", public
///   FROZEN            public, pg_temp
///   AFTER (planted)   public, pg_temp
///   AFTER (shipped)   "$user", public
/// ```
///
/// `cargo xtask check` answers `check passed`, console byte-identical to a
/// clean run. `0011`'s epilogue is the same, and it is the file where the class
/// starts.
///
/// What that costs: `set_config(…, false)` is session-level on a connection sqlx
/// returns to a pool, and `0012` then wipes its own saved copy — so nothing on
/// the connection records what the path was. `0012`'s header says `0013` would
/// inherit it. `PROTECTIONS` and `SHAPES` are snapshots of what the database
/// holds and an epilogue leaves no trace in the catalog; `CONDITIONS` is the
/// file-read census for the half the database does not keep, and it covers
/// `IF … THEN` guards.
///
/// **Replayed rather than re-implemented.** The statements are read out of each
/// file in the order the file writes them and run on one connection whose path
/// is the default — which is the shape `sqlx::migrate!` runs them in — so what
/// is asserted is the file's own arithmetic and not a second copy of it. A
/// migration that deliberately left the path changed would fail here, and that
/// is the rule: nothing after a migration inherits a change made for it.
#[tokio::test]
async fn a_migration_leaves_the_search_path_as_it_found_it() {
    let Some(pool) = pool("nc_test_path_restore").await else {
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("the migrations are at {}: {error}", root.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "sql"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 12,
        "only {} migration(s) were read from {}, so this walks a directory it did not find",
        files.len(),
        root.display()
    );

    let url = std::env::var("DATABASE_URL").expect("checked by pool()");
    let mut moved = 0_usize;
    let mut replayed = 0_usize;
    for file in &files {
        let name = file
            .file_name()
            .expect("a file")
            .to_string_lossy()
            .into_owned();
        let statements =
            the_path_statements_of(&std::fs::read_to_string(file).expect("a readable migration"));
        if statements.is_empty() {
            continue;
        }
        // One connection, and its own — the leak this is about is what a pooled
        // connection carries to whoever borrows it next.
        let alone = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("DATABASE_URL is set, so it has to connect");
        let path = || async {
            sqlx::query_scalar::<_, String>("SHOW search_path")
                .fetch_one(&alone)
                .await
                .expect("the session answers")
        };
        let before = path().await;
        let mut seen: Vec<String> = Vec::new();
        for statement in &statements {
            sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&alone)
                .await
                .unwrap_or_else(|error| panic!("{name} could not replay `{statement}`: {error}"));
            seen.push(path().await);
            replayed += 1;
        }
        // The floor: a file whose statements never move the path would satisfy
        // the comparison below by leaving it alone, and this test would then be
        // about a migration that does nothing.
        assert!(
            seen.iter().any(|during| during != &before),
            "{name} writes {} `set_config` statement(s) and the path never moved, so the \
             restore below is compared against nothing",
            statements.len()
        );
        let after = path().await;
        assert_eq!(
            after, before,
            "{name} left the session holding `{after}` where it found `{before}`, so a \
             connection sqlx returns to the pool resolves every later unqualified name in \
             a schema this migration chose — and the migration that runs after it inherits \
             the change"
        );
        alone.close().await;
        moved += 1;
    }
    // The floor under the walk: an extractor that matched nothing would pass by
    // checking nothing.
    assert!(
        moved >= 2 && replayed >= 5,
        "{moved} migration(s) and {replayed} statement(s) were replayed, and this file \
         reads two migrations that change the path and five statements between them"
    );
    finish(pool).await;
}

/// A read starts where it was asked to, and comes back in sequence order.
///
/// **`Postgres::from`'s `WHERE sequence >= $1` and its `ORDER BY sequence`,
/// each covered.** Every other `from` in this file passes `0`, and every record
/// it writes carries a `recorded_at` that rises with the sequence — so the
/// filter is the identity on every other input the suite has and the heap order
/// is the sequence order. Measured without this, one clause removed at a
/// time: `cargo test -p nc-journal --all-features` green for both.
///
/// `Store::from`'s own sentence is *every entry from `sequence` onwards, in
/// order*, and `Journal::verify` is written against both halves — the
/// checkpoint case is proven against `InMemory` alone by
/// `verifying_from_a_checkpoint_checks_the_link_at_the_checkpoint`.
///
/// `CLUSTER` is what separates the heap order from the sequence order: ordinary
/// maintenance, table ownership, no trigger fires and no row changes. And
/// `recorded_at` is the caller's, so a clock that steps backwards is enough to
/// make the two disagree with nobody meaning anything by it.
#[tokio::test]
async fn a_read_starts_where_it_was_asked_and_comes_back_in_order() {
    const SCHEMA: &str = "nc_test_read_window";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    for step in 0..4_u64 {
        let previous = store.tail().await.expect("reads").map(|tail| tail.hash);
        let mut record = full_record();
        // Descending in time, ascending in sequence.
        record.recorded_at = 1_800_000_000_000 - step;
        store
            .append(Entry::chain(step, previous, record))
            .await
            .expect("appends");
    }
    sqlx::query("CLUSTER journal_entries USING journal_entries_recorded_at")
        .execute(&pool)
        .await
        .expect("this test's own table is ours to cluster");

    let read = |at: u64| {
        let store = store.clone();
        async move {
            store
                .from(at)
                .await
                .expect("reads")
                .entries
                .iter()
                .map(|entry| entry.sequence)
                .collect::<Vec<u64>>()
        }
    };
    assert_eq!(
        read(0).await,
        vec![0, 1, 2, 3],
        "the read came back in the order the heap holds rather than in sequence order"
    );
    // **And the two single-row reads, which order by the same column.**
    // `tail()`'s is pinned by `a_row_can_only_land_where_the_next_one_goes`;
    // without this, `head()`'s is pinned by nothing, because every record this
    // suite writes carries a `recorded_at` that rises with the sequence. Measured,
    // `ORDER BY recorded_at DESC` in its place: the whole suite green.
    assert_eq!(
        store.head().await.expect("reads").map(|(at, _)| at),
        Some(3),
        "`head` named a row that is not the last one: it says `the last sequence and          its hash`, and it ordered by a column the caller writes"
    );
    assert_eq!(
        store
            .tail()
            .await
            .expect("reads")
            .map(|entry| entry.sequence),
        Some(3),
        "`tail` named a row that is not the last one"
    );
    assert_eq!(
        read(2).await,
        vec![2, 3],
        "a read asked to start at 2 did not start there, so a verification from a \
         checkpoint reads a window other than the one it asked for"
    );
    finish(pool).await;
}

/// Every check passing and the write refused is contention, and says so.
///
/// **The arm that says so is reachable only by winning a race, and reverting it
/// would be silent.** `why_nothing_landed`'s own comment records what the wrong
/// answer would be — `OutOfOrder { expected: free, found: entry.sequence }`,
/// reached only where those two are equal, so it would print one number twice
/// and read as *nothing happened*. Measured, put back without
/// this: `cargo test -p nc-journal --all-features` green.
///
/// Asked through the door for the same reason
/// `an_open_unit_outranks_the_sequence_whatever_refused_the_write` uses it: the
/// state is a race between the `WHERE` and the row landing, and a test that
/// catches a race one run in six is one people learn to re-run.
#[tokio::test]
async fn a_refusal_with_nothing_wrong_is_contention() {
    const SCHEMA: &str = "nc_test_nothing_wrong";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");
    let tail = store.tail().await.expect("reads").expect("a tail");
    // The entry that would land: the next sequence, linked to the tail, in no
    // closed unit and opening none. Every question the diagnosis asks answers
    // "nothing wrong", which is the one state that is somebody else's write.
    let next = Entry::chain(1, Some(tail.hash), full_record());
    assert_eq!(
        store.why_nothing_landed_for_tests(&next).await,
        Err(nc_journal::Error::Contended),
        "a refusal every check calls fine was reported as an ordering failure, which \
         prints the same number twice and reads as nothing having happened"
    );
    finish(pool).await;
}

/// A row this code cannot read keeps the sentence that says what is wrong.
///
/// **`reason` takes the text after the *first* `": "`, and this says so.** Its
/// whole job is to drop `Error::Unavailable`'s own opener — *"the journal store
/// did not answer: "* — and keep the part that describes the row; every other
/// message this suite reaches it with holds one colon, so `rsplit_once` would
/// be the identity on all of them. Measured, `split_once` swapped
/// for `rsplit_once`: `cargo test -p nc-journal --all-features` green without
/// this, while a row whose `realm` is not an identifier comes back holding the
/// parser's detail and nothing that says what was being parsed.
#[tokio::test]
async fn an_unreadable_row_keeps_the_sentence_that_says_what_is_wrong() {
    const SCHEMA: &str = "nc_test_reason_prefix";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut store = store_in(&pool, SCHEMA);
    store
        .append(Entry::chain(0, None, full_record()))
        .await
        .expect("appends");
    // A realm no `OpaqueId` admits, written by a second connection — the row
    // this table's own trigger will not then let anybody remove.
    sqlx::query(
        "INSERT INTO journal_entries (sequence, realm, principal, actor_chain, action, \
         resource, result, recorded_at, hash) VALUES (1, 'not a realm!', 'p', \
         '[]'::jsonb, 'Read', '{}', 'allowed', 1, repeat('a', 64))",
    )
    .execute(&pool)
    .await
    .expect("a second connection can insert, which is why this guard exists");
    let (at, why) = store
        .from(0)
        .await
        .expect("reads")
        .stopped_at
        .expect("the read stopped at the foreign row");
    assert_eq!(at, 1);
    assert!(
        why.contains("a stored identifier is not one"),
        "the verdict kept the innermost detail and dropped the sentence that says what \
         is wrong with the row: {why}"
    );
    finish(pool).await;
}
