//! The `DELETE` against the store that runs.
//!
//! The parent module's tests prove the arithmetic — when to run, how many to
//! take, what to report — against a target that holds numbers.
//!
//! **Whether to prune at all is the parent module's to prove**, and it does: it
//! is arithmetic over two numbers and needs no database. Were it decided only
//! here, `before <= max_rows` weakened to `before == max_rows` would leave
//! `cargo test -p nc-prune --lib` green and `cargo xtask check --offline` at
//! `check passed`, with a pruner that removes a batch from every table *below*
//! its limit — CI never passes `--offline`, but the parent module would be
//! claiming what it does not keep.
//! `a_table_below_or_at_its_limit_is_left_alone` is where it is kept. **They
//! cannot see whether the statement is SQL.** `DELETE … ORDER BY … LIMIT` is
//! not something PostgreSQL accepts, and a pruner that removes the *newest*
//! rows instead of the oldest satisfies every in-memory assertion there is: the
//! count goes down by the right amount either way.
//!
//! So this asserts on which rows survive, not on how many. It needs
//! `DATABASE_URL`; `cargo xtask dev` sets the stack up and prints it.

use nc_catalog::{Strategy, TableLimit};
use nc_prune::{Request, Target as _, postgres::Postgres};

/// This file's own tables, pruned by this file's own limits.
///
/// **`prune_these` refuses a name `nc_catalog::TABLES` does not declare, and
/// refuses a name it does declare unless the entry is handed over exactly as
/// the catalog wrote it.** That is what "the catalog decides what this crate
/// may touch" has to mean — a guard comparing one field lets three tables be
/// emptied through it — and it makes `rows_over_time`, which this file creates
/// and drops itself, not something `prune_these` will act on.
///
/// So these tests say which limits they mean. They are the authority for the
/// tables they created, and for nothing else: the journal is carved out by name
/// whatever an authority says, which
/// `the_catalog_decides_the_whole_entry_and_not_one_field_of_it` pins.
async fn prune_these(
    target: &mut Postgres,
    tables: &[TableLimit],
) -> Result<Vec<nc_prune::Pruned>, String> {
    nc_prune::prune_these_declared_by(target, tables, tables, false).await
}

/// The same, counting rather than deleting.
async fn rehearse(
    target: &mut Postgres,
    tables: &[TableLimit],
) -> Result<Vec<nc_prune::Pruned>, String> {
    nc_prune::prune_these_declared_by(target, tables, tables, true).await
}
use sqlx::postgres::PgPoolOptions;

/// A pruner on `pool`, in the schema `pool` was opened for.
///
/// **The pruner is told its schema.** Refusing any resolved path with a comma
/// in it fails on the server default the moment a schema named after the
/// connecting role exists — see `Postgres::in_schema`.
fn pruner(pool: &sqlx::PgPool, schema: &str) -> Postgres {
    Postgres::in_schema(pool.clone(), &format!("{schema}_{}", std::process::id()))
}

/// A pool in a schema of this test's own, or `None` when there is no database.
///
/// The same shape as `nc-journal`'s, and for the same two reasons: a schema per
/// test because these run concurrently, and the process id in its name because
/// `pool` drops what it is handed, so two concurrent runs of `cargo xtask check`
/// would otherwise destroy each other's tables mid-test.
///
/// **Fail-closed.** An absent `DATABASE_URL` is a panic, not a skip: a skipped
/// test counts as passed, so returning `None` would let this file prove nothing
/// while `check` prints a test count that has gone up.
async fn pool(schema: &str) -> Option<sqlx::PgPool> {
    let schema = &format!("{schema}_{}", std::process::id());
    let url = match std::env::var("DATABASE_URL").ok() {
        Some(url) => url,
        None if std::env::var_os("NC_NO_STACK").is_some() => {
            let why = std::env::var("NC_NO_STACK").unwrap_or_default();
            println!("nc-prune: skipped, no database: {why}");
            return None;
        }
        None => panic!(
            "DATABASE_URL is not set. This is the only test that runs the pruner's \
             DELETE against PostgreSQL, so skipping it silently is the failure \
             `cargo xtask check` exists to prevent. Run `cargo xtask dev`, or \
             `cargo xtask check --offline` to skip it on purpose"
        ),
    };
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("DATABASE_URL is set, so it has to connect");
    for prefix in THE_SCHEMAS_THIS_SUITE_MAKES {
        sweep_schemas_from_dead_runs(&admin, prefix).await;
    }
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
        .max_connections(2)
        .connect_with(options.options([("search_path", schema)]))
        .await
        .expect("connects into the schema");
    // A table of this test's own rather than a migration's: what is being proved
    // is the statement, and borrowing a real table would tie this to whatever
    // columns that one happens to have today.
    sqlx::query("CREATE TABLE rows_over_time (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)")
        .execute(&pool)
        .await
        .expect("the test's own table");
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
    // `current_schema()` can drop somebody else's: it is the first *existing*
    // schema on the path, not the one the pool asked for — so on a pool whose
    // path resolves elsewhere it drops whatever is there, the developer's
    // `public` schema included, with the journal, the access records and the
    // migration record in it. `nc-journal`'s sibling helper is the same helper
    // against the same database, and carries the same guard.
    //
    // The process id is the one thing every schema this suite makes carries and
    // no shared schema does. A prefix list grows by one on each run, which is an
    // exclusion list keeping a destructive command honest — the shape this
    // repository refuses everywhere else. A `DROP SCHEMA` in a shared database
    // that guesses has no undo.
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
    // and says nothing, because cargo swallows a passing test's output.
    // Measured on a developer's database: two left behind, dated to runs whose
    // processes were long gone. The tidying *is* part of the point: this file claims to leave
    // nothing behind, and a claim nothing checks is what this repository is
    // built against.
    pool.close().await;
    dropped.expect("the test schema could not be dropped, and this suite claims to leave none");
}

/// The limit under test: ten rows, pruned four at a time, oldest by `at`.
const LIMIT: TableLimit = TableLimit {
    name: "rows_over_time",
    database: nc_catalog::Database::Server,
    max_rows: Some(10),
    prune_batch: Some(4),
    order_by: "at",
    strategy: Strategy::OldestFirst,
};

async fn fill(pool: &sqlx::PgPool, rows: i64) {
    for id in 1..=rows {
        // `at` ascends with `id`, so "the oldest four" is `1..=4` and a pruner
        // that took the newest would leave a hole this test can see.
        sqlx::query("INSERT INTO rows_over_time (id, at) VALUES ($1, $2)")
            .bind(id)
            .bind(id * 1000)
            .execute(pool)
            .await
            .expect("the row inserts");
    }
}

async fn surviving(pool: &sqlx::PgPool) -> Vec<i64> {
    sqlx::query_scalar("SELECT id FROM rows_over_time ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("the rows read back")
}

#[tokio::test]
async fn the_oldest_rows_are_the_ones_that_go() {
    let Some(pool) = pool("prune_oldest").await else {
        return;
    };
    fill(&pool, 14).await;
    let mut target = pruner(&pool, "prune_oldest");

    assert_eq!(target.count("rows_over_time").await, Ok(14));

    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].before, Some(14));
    assert_eq!(done[0].removed, 4, "one batch, not the whole excess");

    // **Which rows**, which is the half no in-memory target can check: taking
    // the newest four would report exactly the same numbers.
    assert_eq!(surviving(&pool).await, (5..=14).collect::<Vec<_>>());

    // And a second run, now at the limit, does nothing at all.
    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done[0].before, Some(10));
    assert_eq!(done[0].removed, 0, "a table at its limit is not over it");
    assert_eq!(surviving(&pool).await, (5..=14).collect::<Vec<_>>());

    finish(pool).await;
}

/// The column the catalog names is what orders the delete.
///
/// **The other fixtures in this file write `at` ascending with `id`**, so the
/// physical order of the table and the order the catalog declares agree there
/// — and a pruner that stops reading `order_by` altogether is
/// indistinguishable from one that reads it. The header above says this file
/// asserts on *which rows survive* rather than on how many, precisely because
/// removing the newest satisfies every count; it does, and over a table whose
/// two orders agree that assertion is satisfied by insertion order too.
///
/// Measured without this, with the statement's `ORDER BY {order_by} ASC`
/// replaced by `WHERE {order_by} IS NOT NULL ORDER BY ctid ASC` — the column
/// still mentioned, so the lints and the source gates stay quiet:
///
/// ```text
/// cargo test -p nc-prune   ->  ok. 28 passed; 0 failed
/// cargo xtask check        ->  ok  860 test(s)   /   check passed
/// ```
///
/// `access_records`' `recorded` is assigned by a trigger, so today it happens to
/// track insertion order — which is what makes the gap invisible, and what will
/// keep it invisible until the first table whose ordering column is a timestamp
/// somebody supplies. The failure it hides is the silent destruction of exactly
/// the rows that had to be kept.
#[tokio::test]
async fn the_declared_column_is_what_orders_the_delete() {
    let Some(pool) = pool("prune_against_order").await else {
        return;
    };
    // `at` descends as `id` ascends. The four oldest rows *by `at`* are the last
    // four inserted, so a pruner that follows the table's own order removes
    // `1..=4` and one that follows the catalog removes `11..=14`.
    for id in 1..=14_i64 {
        sqlx::query("INSERT INTO rows_over_time (id, at) VALUES ($1, $2)")
            .bind(id)
            .bind((15 - id) * 1000)
            .execute(&pool)
            .await
            .expect("the row inserts");
    }
    let mut target = pruner(&pool, "prune_against_order");

    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done[0].removed, 4, "one batch, as everywhere else here");
    assert_eq!(
        surviving(&pool).await,
        (1..=10).collect::<Vec<_>>(),
        "the four oldest rows by `at` are 11..=14 and they are the ones that went; \
         `1..=10` surviving is the answer only a pruner reading the catalog's \
         column gives, and `5..=14` is the answer insertion order gives"
    );

    finish(pool).await;
}

/// Rows that tie on the ordering column are all equally old.
///
/// `ORDER BY … ASC LIMIT n` over a tie chooses between them however it likes,
/// so what can be asserted is that every removed row came from the tied-oldest
/// group and that the count is exact — not which of them went. Untested, the
/// same statement could satisfy the batch by reaching past the tie into rows
/// that are genuinely newer, which is the failure a tie makes possible and
/// nothing else here does.
#[tokio::test]
async fn a_tie_on_the_ordering_column_is_broken_inside_the_tie() {
    let Some(pool) = pool("prune_ties").await else {
        return;
    };
    // Six rows at one instant, eight at a later one: fourteen rows, a limit of
    // ten, and a batch of four that has to come out of the six.
    for id in 1..=14_i64 {
        sqlx::query("INSERT INTO rows_over_time (id, at) VALUES ($1, $2)")
            .bind(id)
            .bind(if id <= 6 { 1_000 } else { 2_000 })
            .execute(&pool)
            .await
            .expect("the row inserts");
    }
    let mut target = pruner(&pool, "prune_ties");

    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done[0].removed, 4);
    let left = surviving(&pool).await;
    assert_eq!(left.len(), 10, "one batch out of fourteen");
    let newer: Vec<i64> = (7..=14).collect();
    for id in &newer {
        assert!(
            left.contains(id),
            "row {id} is newer than every tied row and was removed anyway: {left:?}"
        );
    }
    // And the floor: a run that removed nothing from the tie would satisfy the
    // loop above by removing nothing at all.
    assert_eq!(
        left.iter().filter(|id| **id <= 6).count(),
        2,
        "four of the six tied rows went, so two of them are left: {left:?}"
    );

    finish(pool).await;
}

#[tokio::test]
async fn one_row_over_the_limit_takes_a_whole_batch() {
    // The boundary the parent module's header states: `max_rows` is how many
    // rows the table may hold, so the first run is at `max_rows + 1` — and it
    // removes `prune_batch`, not one.
    let Some(pool) = pool("prune_boundary").await else {
        return;
    };
    fill(&pool, 11).await;
    let mut target = pruner(&pool, "prune_boundary");

    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done[0].removed, 4);
    assert_eq!(surviving(&pool).await, (5..=11).collect::<Vec<_>>());

    finish(pool).await;
}

#[tokio::test]
async fn a_table_below_its_limit_is_left_alone() {
    // **The mirror of the boundary above.** Every other `prune_these` fixture
    // in this file is over the limit — 11, 14 or 20 rows against a `max_rows`
    // of 10 — and the two below-limit fixtures call `delete_oldest` directly,
    // which bypasses the arithmetic entirely. Without this the clause that
    // decides whether to prune at all is covered on one side only in this
    // file: measured, `if before <= max_rows` weakened to
    // `if before == max_rows` leaves every other test here green, while a table
    // of seven rows under a limit of ten loses four of them and reports an
    // ordinary `Pruned`. `access_records` is
    // `max_rows = 500_000, prune_batch = 50_000`, so that shape empties a live
    // table 50_000 rows at a time with every run answering `Ok`.
    //
    // `would_remove` is asserted as well as `removed`, because nothing else
    // pins the dry-run number on the untaken path either.
    let Some(pool) = pool("prune_under").await else {
        return;
    };
    fill(&pool, 9).await;
    let mut target = pruner(&pool, "prune_under");

    let done = prune_these(&mut target, &[LIMIT]).await.expect("prunes");
    assert_eq!(done[0].before, Some(9));
    assert_eq!(done[0].removed, 0, "a table below its limit is not over it");
    assert_eq!(done[0].would_remove, 0);
    assert_eq!(surviving(&pool).await, (1..=9).collect::<Vec<_>>());

    finish(pool).await;
}

#[tokio::test]
async fn a_batch_larger_than_the_table_takes_what_is_there() {
    // `LIMIT` above what exists is legal SQL and removes everything, which is
    // the case a `DELETE` written with an arithmetic offset would get wrong.
    let Some(pool) = pool("prune_short").await else {
        return;
    };
    fill(&pool, 3).await;
    let mut target = pruner(&pool, "prune_short");

    let removed = target
        .delete_oldest("rows_over_time", "at", 10)
        .await
        .expect("deletes");
    assert_eq!(removed, 3);
    assert_eq!(surviving(&pool).await, [] as [i64; 0]);

    // And on an empty table it removes nothing rather than failing.
    let removed = target
        .delete_oldest("rows_over_time", "at", 10)
        .await
        .expect("deletes");
    assert_eq!(removed, 0);

    finish(pool).await;
}

#[tokio::test]
async fn a_partitioned_table_is_refused_rather_than_pruned() {
    // **Every table above is one physical table**, and that is what hides the
    // defect: `ctid` is a row's address *within* a physical table, so on a
    // partitioned one `WHERE ctid IN (…)` matches a row in every partition that
    // shares an address with a chosen row. Measured with a `ctid` delete,
    // asking for four: `DELETE 8`, and the four newest rows among the eight.
    //
    // Partitioning by time is the ordinary thing to do to a table bounded at
    // half a million rows, so this waits for the first table that grows — and
    // the two failures it produces are exactly the two this file exists to
    // catch.
    let Some(pool) = pool("prune_partitioned").await else {
        return;
    };
    // Its own partitioned table, beside the flat one `pool` makes.
    for statement in [
        "CREATE TABLE parted (id BIGINT, at BIGINT NOT NULL) PARTITION BY RANGE (at)",
        "CREATE TABLE parted_old PARTITION OF parted FOR VALUES FROM (0) TO (10000)",
        "CREATE TABLE parted_new PARTITION OF parted FOR VALUES FROM (10000) TO (1000000)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own partitions");
    }
    for id in 1..=7_i64 {
        sqlx::query("INSERT INTO parted (id, at) VALUES ($1, $2)")
            .bind(id)
            .bind(id * 100)
            .execute(&pool)
            .await
            .expect("an old row");
        sqlx::query("INSERT INTO parted (id, at) VALUES ($1, $2)")
            .bind(100 + id)
            .bind(20_000 + id * 100)
            .execute(&pool)
            .await
            .expect("a new row");
    }

    // **Refused, not pruned.** Asserting that a partitioned table loses exactly
    // its batch makes a partition a door: two DDL statements that copy no data,
    // `CREATE TABLE access_records … PARTITION BY RANGE` and
    // `ALTER TABLE … ATTACH PARTITION victim`, put an ordinary heap table under
    // a catalog name and 50 000 of its rows are deleted through the shipped
    // entry point, measured. `access_records` carries no DELETE trigger; the
    // journal survives only through its own, which this crate says in as many
    // words it does not rely on.
    //
    // A partition is a table the catalog does not name, so what pruning one
    // means is something the catalog cannot express. Saying so is the answer.
    let mut target = pruner(&pool, "prune_partitioned");
    let refused = target
        .delete_oldest("parted", "at", 4)
        .await
        .expect_err("a partitioned table is refused");
    assert!(
        refused.why.contains("partitioned"),
        "the refusal does not say why: {refused:?}"
    );

    let survivors: i64 = sqlx::query_scalar("SELECT count(*) FROM parted")
        .fetch_one(&pool)
        .await
        .expect("the rows read back");
    assert_eq!(survivors, 14, "rows went from a table that was refused");

    // **And the partition itself, which is a different arm.** The refusal above
    // is `relkind = 'p'` — the parent. A *child* is an ordinary heap table that
    // happens to be attached, and without this the arm that names it
    // (`c.relispartition`) can be deleted with `cargo test -p nc-prune` and
    // `cargo xtask check` both green: the `pg_inherits WHERE inhrelid` arm two
    // lines below catches the same table and says something else. Which
    // sentence a developer is shown is what the arm decides, and *"part of a
    // table this catalog does not name"* is the true one — attaching a
    // partition is two DDL statements that copy no data, which is how an
    // ordinary table gets under a catalog name in the first place.
    let refused = target
        .delete_oldest("parted_old", "at", 4)
        .await
        .expect_err("a partition of another table is refused");
    assert!(
        refused.why.contains("partition of another table"),
        "the refusal names something other than the partition it is about, so the arm \
         that says what is wrong is decided by nothing: {refused:?}"
    );
    let survivors: i64 = sqlx::query_scalar("SELECT count(*) FROM parted")
        .fetch_one(&pool)
        .await
        .expect("the rows read back");
    assert_eq!(survivors, 14, "rows went from a partition that was refused");

    finish(pool).await;
}

/// A key that is not one column is refused, and so is no key at all.
///
/// **Both arms, decided here.** Every other `CREATE TABLE` in this file
/// is `(id BIGINT PRIMARY KEY, …)`, so `[only] => …` widened
/// to `[only, ..] => …` and `[] => Ok("\"id\"")` each leave the other tests
/// green. The first is the one that matters: with a composite key the delete
/// addresses rows by its *first* column alone,
/// so `DELETE … WHERE {key} IN (SELECT {key} … LIMIT n)` removes every row
/// sharing those values rather than the `n` the batch chose —
/// and `over_deleted` only notices when the overshoot exceeds the batch.
#[tokio::test]
async fn a_key_this_pruner_cannot_address_is_refused() {
    let Some(pool) = pool("prune_keys").await else {
        return;
    };
    for statement in [
        "CREATE TABLE two_column_key (a BIGINT, b BIGINT, at BIGINT NOT NULL, \
         PRIMARY KEY (a, b))",
        "CREATE TABLE no_key_at_all (id BIGINT, at BIGINT NOT NULL)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own tables");
    }

    let mut target = pruner(&pool, "prune_keys");
    for (table, said) in [
        ("two_column_key", "column(s) and this pruner deletes by one"),
        ("no_key_at_all", "primary key"),
    ] {
        let refused = target
            .delete_oldest(table, "at", 4)
            .await
            .expect_err("a key this pruner cannot address is refused, not guessed at");
        assert!(
            refused.why.contains(said),
            "`{table}` was refused and the refusal does not say why: {refused:?}"
        );
    }

    finish(pool).await;
}

#[tokio::test]
async fn an_inherited_table_is_refused_too() {
    // The older spelling of the same shape, and the same defect: `ctid` is
    // unique per child, not per hierarchy.
    let Some(pool) = pool("prune_inherited").await else {
        return;
    };
    for statement in [
        "CREATE TABLE inherited (id BIGINT, at BIGINT NOT NULL)",
        "CREATE TABLE inherited_child () INHERITS (inherited)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own hierarchy");
    }
    for id in 1..=5_i64 {
        sqlx::query("INSERT INTO inherited (id, at) VALUES ($1, $2)")
            .bind(id)
            .bind(id * 100)
            .execute(&pool)
            .await
            .expect("a parent row");
        sqlx::query("INSERT INTO inherited_child (id, at) VALUES ($1, $2)")
            .bind(100 + id)
            .bind(20_000 + id * 100)
            .execute(&pool)
            .await
            .expect("a child row");
    }

    // Refused for the same reason a partition is: a `DELETE` here reaches rows
    // this crate was not asked about, and `rows_affected` would not count them.
    let mut target = pruner(&pool, "prune_inherited");
    let refused = target
        .delete_oldest("inherited", "at", 3)
        .await
        .expect_err("a table something inherits from is refused");
    assert!(
        refused.why.contains("inherits"),
        "the refusal does not say why: {refused:?}"
    );
    let survivors: i64 = sqlx::query_scalar("SELECT count(*) FROM inherited")
        .fetch_one(&pool)
        .await
        .expect("the rows read back");
    assert_eq!(survivors, 10, "rows went from a table that was refused");

    finish(pool).await;
}

/// A row another transaction is updating is still pruned.
///
/// **A delete addressing rows by `ctid` addresses where a row sits.** An
/// `UPDATE` committed against a chosen row between the subselect's snapshot and
/// the delete's row lock moves it, so PostgreSQL re-checks the new tuple, finds
/// its address is not in the list, and skips it. Measured with no injected delay
/// at all — an ordinary single-row `UPDATE` loop beside a batch of 5000 —
/// `removed 4991 of 5000`; and on `access_records`, whose `recorded` its own
/// trigger preserves across an `UPDATE`, the skipped row stays at the head of
/// every later batch:
///
/// ```text
/// round 0..4: removed 3 of 4 each time, the same oldest row surviving
/// ```
///
/// which is `§15.8`'s only enforced bound quietly ceasing to converge. The
/// `UPDATE` is not hypothetical: `§15.6` fills in a city on an access record
/// after the fact.
///
/// The window is widened here — the updating transaction is held open while the
/// prune runs — because a race reproduced only sometimes is a test that fails
/// only sometimes.
#[tokio::test]
async fn a_row_another_transaction_is_updating_is_still_pruned() {
    let Some(pool) = pool("prune_moved_row").await else {
        return;
    };
    // `pool` created the table; this adds the column the other transaction
    // moves a row by writing to, and the rows.
    for statement in [
        "ALTER TABLE rows_over_time ADD COLUMN note TEXT",
        "INSERT INTO rows_over_time SELECT g, g, 'x' FROM generate_series(1, 20) g",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own table");
    }

    // The other transaction: it moves the oldest row and holds it until the
    // prune below has taken its snapshot.
    let holder = pool.clone();
    let (started, was_started) = tokio::sync::oneshot::channel::<()>();
    let moving = tokio::spawn(async move {
        let mut held = holder.begin().await.expect("a transaction of its own");
        sqlx::query("UPDATE rows_over_time SET note = 'geoip' WHERE id = 1")
            .execute(&mut *held)
            .await
            .expect("the update lands");
        started.send(()).expect("the prune is waiting for this");
        tokio::time::sleep(core::time::Duration::from_millis(900)).await;
        held.commit().await.expect("and commits");
    });
    was_started.await.expect("the update began");

    let mut target = pruner(&pool, "prune_moved_row");
    let removed = target
        .delete_oldest("rows_over_time", "at", 4)
        .await
        .expect("a row being updated is not a reason to refuse");
    moving.await.expect("the other transaction finished");

    assert_eq!(
        removed, 4,
        "the batch was four and {removed} were removed: a row this transaction had \
         chosen was moved by another and skipped"
    );
    let oldest: i64 = sqlx::query_scalar("SELECT min(id) FROM rows_over_time")
        .fetch_one(&pool)
        .await
        .expect("the table is readable");
    assert_eq!(
        oldest, 5,
        "row 1 survived a prune that was told to take the four oldest. Its ordering \
         column is unchanged, so it heads every later batch too — measured, five rounds \
         in a row, each reporting success"
    );

    finish(pool).await;
}

#[tokio::test]
async fn a_table_named_after_a_reserved_word_is_counted_and_pruned() {
    // **`SELECT count(*) FROM user` answers 1, with no error.** PostgreSQL reads
    // the bare word as the `USER` function, so a bounded table called `user` —
    // an unremarkable name in a platform whose own Cedar schema declares
    // `entity User` — would report one row for ever, never cross `max_rows`, and
    // never be pruned, with every gate green. The shape guard admits it,
    // because `[a-z_][a-z0-9_]*` admits every reserved word there is.
    let Some(pool) = pool("prune_reserved").await else {
        return;
    };
    for statement in [
        // A primary key, because the pruner deletes by one — a row's key is
        // what it *is*, where its `ctid` is only where it sits and a concurrent
        // `UPDATE` moves that. This fixture is about quoting a reserved word,
        // and the key does not weaken it.
        "CREATE TABLE \"user\" (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)",
        "INSERT INTO \"user\" SELECT g, g FROM generate_series(1, 5) g",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own reserved-word table");
    }

    let mut target = pruner(&pool, "prune_reserved");
    assert_eq!(
        target.count("user").await,
        Ok(5),
        "the table was counted as the USER function"
    );
    let removed = target
        .delete_oldest("user", "at", 2)
        .await
        .expect("deletes");
    assert_eq!(removed, 2);
    let survivors: Vec<i64> = sqlx::query_scalar("SELECT id FROM \"user\" ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("the rows read back");
    assert_eq!(survivors, vec![3, 4, 5]);

    finish(pool).await;
}

#[tokio::test]
async fn a_name_the_catalog_did_not_write_never_reaches_the_database() {
    // The guard, fired against a real connection: the refusal happens before
    // anything is sent, so the table it names is still there afterwards.
    let Some(pool) = pool("prune_guard").await else {
        return;
    };
    fill(&pool, 5).await;
    let mut target = pruner(&pool, "prune_guard");

    let refused = target
        .delete_oldest("rows_over_time; DROP TABLE rows_over_time", "at", 1)
        .await
        .expect_err("must be refused");
    assert!(refused.why.contains("plain identifier"), "{refused}");
    assert_eq!(surviving(&pool).await, (1..=5).collect::<Vec<_>>());

    // The order-by column too, which is the second identifier written into the
    // statement.
    let refused = target
        .delete_oldest("rows_over_time", "at DESC", 1)
        .await
        .expect_err("must be refused");
    assert!(refused.why.contains("plain identifier"), "{refused}");
    assert_eq!(surviving(&pool).await, (1..=5).collect::<Vec<_>>());

    finish(pool).await;
}

#[tokio::test]
async fn a_request_can_be_built_from_outside_this_crate() {
    // **`#[non_exhaustive]` with no constructor makes `Request` unbuildable
    // here**: `Request { table: Some(x), dry_run: true }` is
    // `error[E0639]: cannot create non-exhaustive struct using struct
    // expression`, and without one the only expression available is
    // `Request::default()` — every table, for real. The drift test lives inside
    // the crate and cannot see that, so the declared `limits.prune` surface
    // would not be reachable in the form a dispatcher would write.
    //
    // This file is outside the crate, which is what makes the assertion mean
    // something: it compiles, or the surface is still unreachable.
    let every = Request::every();
    assert_eq!(every.table, None);
    assert!(!every.dry_run);

    let one = Request::just("access_records");
    assert_eq!(one.table, Some("access_records"));
    assert!(!one.dry_run);

    let rehearsal = Request::just("access_records").dry();
    assert_eq!(rehearsal.table, Some("access_records"));
    assert!(rehearsal.dry_run);

    // And the two arguments the command declares are exactly these two.
    let declared = nc_catalog::command("limits.prune").expect("the command is declared");
    assert_eq!(declared.arguments, ["table", "dry_run"]);
}

#[tokio::test]
async fn a_dry_run_checks_the_names_it_would_write_into_the_statement() {
    // **A dry run that returns before `delete_oldest`** skips the only place the
    // identifier guard and the `ORDER BY` column are exercised — so it predicts
    // `would_remove: 50000` for a run that then fails with `column "id" does
    // not exist`. A rehearsal that cannot fail where the real thing fails is not
    // a rehearsal.
    let Some(pool) = pool("prune_rehearsal").await else {
        return;
    };
    fill(&pool, 14).await;
    let mut target = pruner(&pool, "prune_rehearsal");

    // The rehearsal, against a column that is not there.
    let bad = TableLimit {
        name: "rows_over_time",
        database: nc_catalog::Database::Server,
        max_rows: Some(10),
        prune_batch: Some(4),
        order_by: "no_such_column",
        strategy: Strategy::OldestFirst,
    };
    let rehearsal = rehearse(&mut target, std::slice::from_ref(&bad)).await;
    assert!(
        rehearsal.is_err(),
        "a dry run predicted a removal from a column that does not exist: {rehearsal:?}"
    );
    assert!(
        rehearsal.unwrap_err().contains("no_such_column"),
        "the rehearsal did not name the column"
    );
    // And the real run fails the same way, which is the point — the two answer
    // the same question.
    let real = prune_these(&mut target, std::slice::from_ref(&bad)).await;
    assert!(real.is_err(), "a bad order_by ran: {real:?}");
    assert_eq!(surviving(&pool).await, (1..=14).collect::<Vec<_>>());

    // And a rehearsal of a *good* limit removes nothing while reporting what a
    // real run would take.
    let good = TableLimit {
        order_by: "at",
        ..bad
    };
    let done = rehearse(&mut target, std::slice::from_ref(&good))
        .await
        .expect("rehearses");
    assert_eq!(done[0].removed, 0);
    assert_eq!(done[0].would_remove, 4);
    assert_eq!(surviving(&pool).await, (1..=14).collect::<Vec<_>>());

    finish(pool).await;
}

/// A caller cannot re-declare a table's retention on the way in.
///
/// **The carve-out is this crate's to enforce, not one migration's alone.**
/// `prune_these` takes a `&[TableLimit]` and every field of `TableLimit` is
/// `pub`, so a consumer can hand the crate whose header says of the journal
/// *"this must not touch it"* a `TableLimit { name: "journal_entries",
/// strategy: OldestFirst, max_rows: Some(0), … }`. The tree's own tests build
/// `TableLimit` exactly that way, so the shape is reachable from outside the
/// crate.
///
/// The trigger does hold — `ERROR: the journal is append-only: DELETE on
/// journal_entries was refused`, measured — but a guarantee that lives in the
/// database should not be claimed by a crate that does not enforce it.
#[tokio::test]
async fn a_caller_cannot_hand_this_crate_another_strategy() {
    const SCHEMA: &str = "nc_prune_restrategy";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut target = pruner(&pool, "nc_prune_restrategy");

    let forged = nc_catalog::TableLimit {
        name: "journal_entries",
        database: nc_catalog::Database::Server,
        strategy: nc_catalog::Strategy::OldestFirst,
        order_by: "sequence",
        max_rows: Some(0),
        prune_batch: Some(1_000_000),
    };
    let refused = nc_prune::prune_these(&mut target, &[forged]).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("carves it out of row-count pruning by name")),
        "the journal's retention was re-declared by a caller: {refused:?}"
    );

    // And a table that is not the journal, disagreeing with the catalog in a
    // field that is not `strategy`, is refused too — a guard comparing one
    // field lets `access_records` be emptied through it by putting back the
    // `order_by` column the catalog has just stopped using.
    let declared = nc_catalog::TABLES
        .iter()
        .find(|table| table.name == "access_records")
        .expect("access_records is a bounded table");
    let restyled = nc_prune::prune_these(
        &mut target,
        &[nc_catalog::TableLimit {
            order_by: "id",
            max_rows: Some(0),
            prune_batch: Some(400_000),
            ..*declared
        }],
    )
    .await;
    assert!(
        restyled
            .as_ref()
            .is_err_and(|why| why.contains("belongs to the catalog")),
        "a caller re-declared everything but the strategy: {restyled:?}"
    );

    // And the catalog's own entry for the same table goes through, so the rule
    // is about disagreeing with the catalog and not about the journal's name.
    let declared: Vec<nc_catalog::TableLimit> = nc_catalog::TABLES
        .iter()
        .filter(|table| table.name == "journal_entries")
        .copied()
        .collect();
    let left_alone = nc_prune::prune_these(&mut target, &declared)
        .await
        .expect("the catalog's own entry is accepted");
    assert_eq!(left_alone.len(), 1);
    assert_eq!(left_alone[0].removed, 0);

    finish(pool).await;
}

/// The refusals a store can be driven into, each made to happen.
///
/// **A branch normal operation never reaches still needs a test of its own.**
/// This drives two through a real store: a batch past what PostgreSQL can be
/// asked for, and an oldest-first table that names no limit. The conversions
/// no query can drive, and the `removed > batch` guard — which *is* the answer
/// to the `ctid` defect, and whose own comment says a pruner that has taken
/// rows it was not asked for cannot undo it — are driven by `postgres.rs`'s
/// unit tests. By this repository's rule — "the branch that cannot be reached
/// in normal operation is exactly the branch that needs a test of its own" — a
/// refusal no test drives is a gate wired to nothing.
#[tokio::test]
async fn every_refusal_this_crate_can_answer_is_driven() {
    const SCHEMA: &str = "nc_prune_refusals";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut target = pruner(&pool, "nc_prune_refusals");

    // The two conversions no query can drive — a negative `count(*)` and a
    // batch past `i64::MAX` — are refused at their own seam, in
    // `postgres.rs`'s unit tests. What is driven here is what a database can
    // actually produce.
    let refused = target.delete_oldest("rows_over_time", "at", u64::MAX).await;
    assert!(
        refused.is_err_and(|why| why.why.contains("than PostgreSQL can be asked for")),
        "a batch beyond i64 was cast rather than refused"
    );

    // **A bound with no numbers is a bound that bounds nothing.**
    let unbounded = nc_catalog::TableLimit {
        name: "rows_over_time",
        database: nc_catalog::Database::Server,
        strategy: nc_catalog::Strategy::OldestFirst,
        order_by: "at",
        max_rows: None,
        prune_batch: None,
    };
    let refused = prune_these(&mut target, &[unbounded]).await;
    assert!(
        refused.is_err_and(|why| why.contains("names no limit")),
        "an oldest-first table with no numbers was pruned anyway"
    );

    finish(pool).await;
}

/// A statement that cannot take its lock gives up rather than waiting for ever.
///
/// **Nothing else in the tree bounds this.** `grep` for `lock_timeout` outside
/// this crate finds no crate, migration, manifest or workflow that sets one.
/// Measured with another
/// session holding `ACCESS EXCLUSIVE`, which is what a migration or a
/// `VACUUM FULL` does:
///
/// ```text
/// count under ACCESS EXCLUSIVE after 5s: Err(Elapsed(()))
/// ```
///
/// It never returns — a scheduled tick that never completes and never reports,
/// holding a pool connection for the life of the lock.
#[tokio::test]
async fn a_locked_table_ends_the_run_rather_than_holding_it() {
    const SCHEMA: &str = "nc_prune_locked";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    // The lock is taken on a connection of its own and held for the whole
    // exchange; the pruner has the pool's other connection.
    let mut holder = pool.begin().await.expect("a transaction of its own");
    sqlx::query("LOCK TABLE rows_over_time IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *holder)
        .await
        .expect("takes the lock");

    let mut target = pruner(&pool, "nc_prune_locked");
    let started = std::time::Instant::now();
    let refused = target.delete_oldest("rows_over_time", "at", 10).await;
    let took = started.elapsed();

    assert!(
        refused.is_err(),
        "the pruner deleted rows from a table under ACCESS EXCLUSIVE"
    );
    assert!(
        took < std::time::Duration::from_secs(30),
        "the pruner waited {took:?} for a lock, which is a scheduled run that never \
         reports and a pool connection held for the life of the lock"
    );
    // **And that it was the *lock* bound that ended it.** The two assertions
    // above are `is_err()` and a 30 s ceiling over a 5 s constant, and neither
    // asks which timeout fired: measured, `lock_timeout` changed to
    // `statement_timeout` leaves the whole gate green without this — after
    // which a lock wait is unbounded, which is the "scheduled run that never
    // reports" case the constant exists for, *and* every legitimate long
    // `DELETE` is cancelled. `55P03` is the lock timeout; `57014` is the
    // statement timeout, and this crate classifies both as "waited".
    let why = format!("{refused:?}");
    assert!(
        why.contains("lock timeout") || why.contains("55P03"),
        "the run ended on something other than the lock bound: {why}"
    );
    // Near the bound rather than under a ceiling six times its size: a
    // `statement_timeout` of the same value would satisfy the ceiling too.
    assert!(
        took < std::time::Duration::from_secs(15),
        "the pruner waited {took:?} against a 5 s lock bound"
    );

    holder.rollback().await.expect("the lock is released");
    finish(pool).await;
}

#[tokio::test]
async fn the_journal_is_not_pruned_by_this() {
    // `§15.8`, and the one rule here that a wrong answer to would destroy an
    // audit log: a table a policy pack bounds is reported and left alone, and
    // this asserts against the *shipped* catalog rather than a fixture, because
    // the entry that matters is the real one.
    let Some(pool) = pool("prune_journal").await else {
        return;
    };
    let mut target = pruner(&pool, "prune_journal");

    // **By its strategy, not by being anything but oldest-first**: the local
    // database's user state is neither, and belongs to another target.
    let journal = nc_catalog::TABLES
        .iter()
        .find(|table| table.strategy == Strategy::PolicyPack)
        .expect("the catalog bounds something by a policy pack");
    let done = prune_these(&mut target, std::slice::from_ref(journal))
        .await
        .expect("runs");
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].removed, 0);
    assert!(
        done[0].left_alone.is_some(),
        "the journal was pruned by a global constant"
    );
    // It is never even counted, which is what "does not touch it" means: the
    // table does not exist in this schema, and a run that queried it would fail
    // rather than report.
    assert_eq!(done[0].before, None);

    finish(pool).await;
}

/// A decoy parent is not a door into the table that inherits from it.
///
/// **A table name is not a table.** PostgreSQL resolves an unqualified name to
/// the table *and every child that inherits from it*, and this crate decides
/// what may be touched by comparing `table.name` against the catalog — so one
/// statement made a decoy with no rows of its own a way into the real one:
///
/// ```text
/// ALTER TABLE access_records INHERIT decoy;
/// count(decoy) = 10          (its own rows: 0)
/// prune the decoy -> Pruned { table: "decoy", before: Some(10), removed: 5 }
/// access_records now 5 rows
/// ```
///
/// The whole-entry comparison that guard uses exists because a one-field guard
/// lets `access_records` be emptied through it; naming a parent walks past a
/// whole-entry comparison too. The journal survives only through the
/// migration's trigger, which this crate says in as many words it does not rely
/// on.
#[tokio::test]
async fn a_decoy_parent_is_not_a_door_into_the_table_that_inherits_from_it() {
    const SCHEMA: &str = "nc_prune_decoy";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    for statement in [
        "CREATE TABLE decoy (id BIGINT PRIMARY KEY, recorded BIGINT NOT NULL)",
        "CREATE TABLE real_rows (LIKE decoy INCLUDING ALL) INHERITS (decoy)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the scratch tables are ours");
    }
    for row in 1..=10_i64 {
        sqlx::query("INSERT INTO real_rows (id, recorded) VALUES ($1, $1)")
            .bind(row)
            .execute(&pool)
            .await
            .expect("writable");
    }

    let mut target = pruner(&pool, "nc_prune_decoy");
    let decoy = nc_catalog::TableLimit {
        name: "decoy",
        database: nc_catalog::Database::Server,
        strategy: nc_catalog::Strategy::OldestFirst,
        order_by: "recorded",
        max_rows: Some(5),
        prune_batch: Some(5),
    };
    // **Refused outright.** Asserting that the decoy's own rows are none, so
    // nothing is pruned, is true and leaves the decision to `ONLY` — and a
    // partition and a rule both walk past `ONLY`, so the question is asked
    // earlier: a table something else can be reached through is refused before
    // any statement runs.
    let refused = nc_prune::prune_these_declared_by(&mut target, &[decoy], &[decoy], false)
        .await
        .expect_err("a table something inherits from is refused");
    assert!(
        refused.contains("inherits"),
        "the refusal does not say why: {refused}"
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ONLY real_rows")
        .fetch_one(&pool)
        .await
        .expect("the table answers");
    assert_eq!(
        left, 10,
        "the inheriting table was emptied through its parent"
    );

    finish(pool).await;
}

/// A `DELETE` that can reach another table is refused before it runs.
///
/// **Three shapes walk past `DELETE FROM ONLY`**, each measured against the
/// real database through the shipped entry point:
///
/// ```text
/// ATTACH PARTITION      -> 50 000 rows of an ordinary table deleted
/// CREATE RULE … DO ALSO -> 24 rows went, the pruner reported 4, and returned Ok
/// AFTER DELETE trigger  -> 16 rows went elsewhere, reported 4, Ok
/// ```
///
/// `rows_affected()` counts neither a rule's deletions nor a trigger's, so the
/// over-deletion guard — whose comment says *"a pruner that has taken rows it
/// was not asked for has already taken them"* — never fires for either. What can
/// be checked is whether the table's `DELETE` stays where it is put.
#[tokio::test]
async fn a_delete_that_could_reach_another_table_is_refused() {
    let Some(pool) = pool("prune_reaches").await else {
        return;
    };
    for statement in [
        "CREATE TABLE elsewhere (id BIGINT PRIMARY KEY)",
        "CREATE TABLE by_rule (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)",
        "CREATE RULE spill AS ON DELETE TO by_rule DO ALSO DELETE FROM elsewhere",
        "CREATE TABLE by_trigger (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)",
        "CREATE FUNCTION spill_rows() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN DELETE FROM elsewhere; RETURN OLD; END $$",
        "CREATE TRIGGER spill AFTER DELETE ON by_trigger FOR EACH ROW \
         EXECUTE FUNCTION spill_rows()",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own tables");
    }
    for id in 1..=10_i64 {
        for table in ["by_rule", "by_trigger"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO {table} (id, at) VALUES ($1, $1)"
            )))
            .bind(id)
            .execute(&pool)
            .await
            .expect("writable");
        }
        sqlx::query("INSERT INTO elsewhere (id) VALUES ($1)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("writable");
    }

    let mut target = pruner(&pool, "prune_reaches");
    for (table, why) in [("by_rule", "rule"), ("by_trigger", "trigger")] {
        let refused = target
            .delete_oldest(table, "at", 4)
            .await
            .expect_err("a table whose DELETE reaches another is refused");
        assert!(
            refused.why.contains(why),
            "the refusal about `{table}` does not name the {why}: {refused:?}"
        );
        assert!(
            !refused.waited,
            "a permanent refusal was reported as a wait: {refused:?}"
        );
    }

    // The floor: nothing was taken from the table the rule and the trigger
    // reach, which is the whole of what was measured going missing.
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM elsewhere")
        .fetch_one(&pool)
        .await
        .expect("the table answers");
    assert_eq!(left, 10, "rows went from a table nobody asked about");

    finish(pool).await;
}

/// A view standing where a table's name resolves is refused, not pruned.
///
/// **Every branch of the reach guard is keyed on the named relation's oid, and a
/// view matches none of them.** Without an arm of its own, `relkind = 'v'`
/// falls through to "safe", and an
/// auto-updatable view satisfies the statement `delete_oldest` issues. Measured,
/// a view named `ledger` over the journal:
///
/// ```text
/// guard says   None (relkind v)
/// outcome      Ok([Pruned { table: "ledger", removed: 6, left_alone: None }])
/// journal_entries before=20 after=14
/// ```
///
/// With a cascade and an `AFTER DELETE` trigger behind it, a third table goes
/// from 100 rows to 0 while the run reports `removed: 6`. Every protection the
/// guard's other branches claim reopens behind one view — including the
/// carve-out that refuses `journal_entries` by name, because the view is not
/// called that.
#[tokio::test]
async fn a_view_standing_in_for_a_table_is_refused() {
    const SCHEMA: &str = "nc_prune_view";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    sqlx::query("CREATE VIEW ledger AS SELECT * FROM rows_over_time")
        .execute(&pool)
        .await
        .expect("the view is created");

    let mut target = pruner(&pool, "nc_prune_view");
    let refused = target.count("ledger").await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.why.contains("not an ordinary table")),
        "a view was counted as though it were a table: {refused:?}"
    );
    let refused = target.delete_oldest("ledger", "at", 4).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.why.contains("not an ordinary table")),
        "a view was pruned: {refused:?}"
    );

    // **And a partition child, which no branch about a parent matches.** It
    // is `relkind = 'r'` and an `inhrelid`, not an `inhparent`, so a guard
    // asking only about parents refuses the parent and passes the child:
    // measured, `parted` refused and `parted_a` pruned, with all five of the
    // parent's rows gone through it. `migrations/0001_journal.sql` names
    // partitioning as the journal's own retention plan and the carve-out
    // compares the bare name, so a partition of the journal would not be
    // carved out either.
    sqlx::query("CREATE TABLE parted (at BIGINT NOT NULL) PARTITION BY RANGE (at)")
        .execute(&pool)
        .await
        .expect("the parent is created");
    sqlx::query("CREATE TABLE parted_a PARTITION OF parted FOR VALUES FROM (0) TO (100)")
        .execute(&pool)
        .await
        .expect("the partition is created");
    for named in ["parted", "parted_a"] {
        let refused = target.count(named).await;
        assert!(
            refused.is_err(),
            "`{named}` was counted as though it were a table this catalog names: {refused:?}"
        );
    }

    // The floor: the table the view stands over is still prunable, so this is a
    // rule about what a name resolves to and not a ban on pruning.
    assert!(
        target.count("rows_over_time").await.is_ok(),
        "an ordinary table stopped being countable"
    );

    finish(pool).await;
}

/// A database that is not there is not a table that refuses.
///
/// **`waited` is read from a SQLSTATE, and a transport failure carries none.**
/// So with that alone, a pool timeout, a refused connection and a DNS failure
/// all reach `Failure::code()` as `waited: false` — which it reads as a
/// permanent refusal. Measured against a closed port:
///
/// ```text
/// why  = pool timed out while waiting for an open connection
/// code = NC-DB-0005 … retryable = false
/// en   = "Old records could not be cleared, and trying again will not help.
///         Contact whoever set up your Noctua."
/// ```
///
/// for a database that will be back in a minute, while the table grows past its
/// limit. `NC-DB-0001` — *"The database did not answer"*, `critical`,
/// `retryable` — is the code this path reaches instead.
#[tokio::test]
async fn a_database_that_is_not_there_says_so() {
    // A port nothing is listening on, and a pool that gives up quickly so this
    // test is not the slowest in the suite.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("bound").port();
    drop(listener);
    let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy(&format!("postgres://noctua:noctua@127.0.0.1:{port}/noctua"))
    else {
        return;
    };

    let mut target = pruner(&pool, "nc_prune_view");
    let refused = target
        .count("rows_over_time")
        .await
        .expect_err("nothing is listening");
    assert_eq!(
        refused.code(),
        nc_catalog::ErrorCode::DbUnreachable,
        "a database that did not answer was reported as a table that refused: {refused}"
    );
    assert!(
        refused.code().retryable(),
        "a database that is restarting is exactly what a later run finds up"
    );
    assert!(
        !refused.answered,
        "the store answered nothing and the failure says it did"
    );

    // The floor: a refusal this crate raises itself still says the store
    // answered, so the rule above is about the connection and not about
    // everything.
    assert!(
        nc_prune::Failure::refused("this table is archived").answered,
        "a refusal from this crate was attributed to the store"
    );

    // **And the whole-catalog run says so too.** `journal_entries` is pushed
    // without the database being touched — it is bounded by the pack — so
    // judged on `done` alone, `verdict` takes the `Ok` branch: measured with
    // the store unreachable, `run(Request::every())` answered `Ok` with two
    // rows while `run(Request::just("access_records"))` answered `Err` for the
    // same fault. One outage, two shapes, decided by the request.
    let mut target = pruner(&pool, "nc_prune_view");
    let every = nc_prune::run(&mut target, nc_prune::Request::every()).await;
    assert!(
        every.is_err(),
        "a run that reached the store for nothing reported success: {every:#?}"
    );
}

/// The default constructor reaches `public`, and this is its
/// caller.
///
/// **`Postgres::new` is what a deployment uses**, and every other test in this
/// file builds a store with `in_schema` so it can have a scratch schema of its
/// own — so without this `Self::in_schema(pool, "public")` could name any
/// schema at all with the suite green: measured with `"nowhere"`,
/// `check passed`.
///
/// Asked by *counting*, because counting writes nothing: `access_records` is a
/// table the migrations put in `public`, and a store aimed anywhere else answers
/// with an error naming the relation it could not find.
#[tokio::test]
async fn the_default_store_reaches_the_public_schema() {
    let Some(pool) = pool("nc_prune_default_schema").await else {
        return;
    };
    let mut target = Postgres::new(pool.clone());
    let counted = target.count("access_records").await;
    assert!(
        counted.is_ok(),
        "the default store could not count a table the migrations create in `public`: \
         {counted:?}"
    );
    finish(pool).await;
}

/// A caller's own authority cannot redescribe a table the catalog declares.
///
/// **Compared against the authority the caller hands over**, passing one
/// forged entry as *both* sides would be vacuous, and the shipped `pub` seam
/// would empty the access log — measured that way:
///
/// ```text
/// prune_these_declared_by(&mut target, &[mine], &[mine], false)
///   -> Ok  access_records before=Some(20) removed=20 would_remove=20 left_alone=None
///   rows left in access_records: 0
///   catalog: access_records order_by=recorded max_rows=Some(500000)
/// ```
///
/// Twenty of twenty rows, newest-first by a column
/// `migrations/0002_access_records.sql` documents as *not* creation order,
/// against a catalog limit of 500 000, reported as an ordinary success. A
/// carve-out of the journal by name protects nothing else.
#[tokio::test]
async fn a_caller_cannot_be_its_own_authority_for_a_catalog_table() {
    const SCHEMA: &str = "nc_prune_own_authority";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut target = pruner(&pool, "nc_prune_own_authority");
    let mine = nc_catalog::TableLimit {
        name: "access_records",
        database: nc_catalog::Database::Server,
        strategy: nc_catalog::Strategy::OldestFirst,
        order_by: "id",
        max_rows: Some(0),
        prune_batch: Some(500_000),
    };
    let refused = nc_prune::prune_these_declared_by(&mut target, &[mine], &[mine], false).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("belongs to the catalog")),
        "a caller was its own authority for a table the catalog declares: {refused:?}"
    );

    // **And under a schema-qualified alias**, the case the bare comparison
    // exists for. `<schema>.access_records` is a legal name for the same table;
    // matched on the whole string it is not "a table the catalog declares", so
    // the caller stays its own authority. Measured with `|| known.name == bare`
    // deleted from that one line:
    //
    // ```text
    // Ok([Pruned { table: "public.access_records", before: Some(20),
    //              removed: 20, would_remove: 20, left_alone: None }])
    // rows left in public.access_records: 0
    //   ok    953 test(s)
    // check passed
    // ```
    //
    // Twenty of twenty rows, ordered by `id` — the column
    // `migrations/0002_access_records.sql` documents as *not* creation order —
    // reported as an ordinary success, with the whole gate green. A carve-out
    // of the journal by bare name leaves the other declared table out; this is
    // the cross of the two cases the sibling assertions cover one at a time.
    let qualified = nc_catalog::TableLimit {
        name: "public.access_records",
        database: nc_catalog::Database::Server,
        ..mine
    };
    let refused = nc_prune::prune_these_declared_by(
        &mut target,
        core::slice::from_ref(&qualified),
        core::slice::from_ref(&qualified),
        false,
    )
    .await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.contains("belongs to the catalog")),
        "a caller was its own authority for `public.access_records`, which is the \
         catalog's `access_records` under a legal alias: {refused:?}"
    );

    // The floor: a table the catalog does **not** declare is still the caller's
    // to describe, which is the whole reason this seam exists.
    let ours = nc_catalog::TableLimit {
        name: "rows_over_time",
        database: nc_catalog::Database::Server,
        strategy: nc_catalog::Strategy::OldestFirst,
        order_by: "at",
        max_rows: Some(0),
        prune_batch: Some(4),
    };
    assert!(
        nc_prune::prune_these_declared_by(&mut target, &[ours], &[ours], true)
            .await
            .is_ok(),
        "a table the catalog does not know stopped being the caller's to describe"
    );

    finish(pool).await;
}

/// The door that issues the `DELETE` asks the catalog too.
///
/// **`Target` is public, so a catalog check one layer up is not enough.** With
/// the check there alone, the method that actually deletes asks a catalog
/// nothing:
///
/// ```text
/// delete_oldest("access_records", "occurred_at", 3) -> Ok(3)
///    SURVIVING: ["r01", …, "r07"]        <- the three NEWEST rows, deleted
/// delete_oldest("journal_entries", "sequence", 6)   -> Ok(6)
///    surviving sequences: [6, 7, 8, 9]   <- an audit log, truncated
/// ```
///
/// The first is the exact inversion `0006` exists to prevent — a column a
/// device's clock writes, against the one a trigger assigns. The second needs a
/// `journal_entries` carrying `0001`'s shape without the trigger migrations: a
/// restored dump, or a second deployment mid-migration. `lib.rs` says of the
/// journal that "the database refuses the `DELETE` as well, and this crate does
/// not rely on that" — through this door, unchecked, it would.
#[tokio::test]
async fn the_delete_door_orders_a_catalog_table_the_catalog_way() {
    const SCHEMA: &str = "nc_prune_delete_door";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    let mut target = pruner(&pool, "nc_prune_delete_door");

    let refused = target
        .delete_oldest("access_records", "occurred_at", 3)
        .await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.why.contains("is ordered by `recorded`")),
        "the access log was ordered by a column the catalog does not declare: {refused:?}"
    );

    // **And schema-qualified, which is how the catalog's own guard is
    // reached.** `the_catalog_orders_its_own_tables` compares the *bare* name —
    // `let bare = table.rsplit('.').next()` — and with only bare names handed
    // to it, dropping that line leaves the whole suite green while
    // `delete_oldest("public.access_records", "occurred_at", …)` finds no
    // catalog entry, returns `Ok`, and deletes by the caller's column. That is
    // the inversion this function's own doc says `0006` exists to prevent,
    // reached by writing the schema in front of the name.
    let refused = target
        .delete_oldest("public.access_records", "occurred_at", 3)
        .await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.why.contains("is ordered by `recorded`")),
        "a schema in front of the name walked past the catalog: {refused:?}"
    );

    let refused = target.delete_oldest("journal_entries", "sequence", 6).await;
    assert!(
        refused
            .as_ref()
            .is_err_and(|why| why.why.contains("bounded by the policy pack")),
        "the journal was offered an oldest to delete: {refused:?}"
    );

    // **The floor.** The catalog's own column gets past this rule — the schema
    // this test made holds no `access_records`, so it reaches the database and
    // fails there, which is a different refusal and the one that proves the
    // rule let it through. And a table the catalog does not declare is still
    // the caller's to order, which is what the `Target` seam is for.
    let reached = target.delete_oldest("access_records", "recorded", 1).await;
    assert!(
        reached
            .as_ref()
            .is_err_and(|why| why.why.contains("does not exist")),
        "the catalog's own ordering column was refused by this rule: {reached:?}"
    );
    // **PostgreSQL's sentence, without where in its own source it was raised**,
    // which sqlx 0.9 appends to the error's `Display` — see `said`.
    assert!(
        reached
            .as_ref()
            .is_err_and(|why| !why.why.contains(" at line ")),
        "the refusal names a line of PostgreSQL's C source as if it were one of the \
         statement's: {reached:?}"
    );
    assert!(
        target
            .delete_oldest("rows_over_time", "at", 1)
            .await
            .is_ok(),
        "a table the catalog does not know stopped being the caller's to order"
    );

    finish(pool).await;
}

/// A temporary table of the same name does not become the table being pruned.
///
/// **`bounded()`'s search-path refusal could not see this.** It reads
/// `current_schemas(false)` and refuses a comma — and `current_schemas(false)`
/// omits the implicitly-searched schemas, while PostgreSQL looks in the
/// temporary schema for a relation name *before* it consults `search_path` at
/// all. Measured on a single-schema path, a real table of 20 rows and a
/// temporary one of 3 sharing the name:
///
/// ```text
/// current_schemas(false) = Ok("{…}")   <- one schema, no comma
/// count()                = Ok(3)       <- the temporary table
/// prune -> Ok([Pruned { removed: 3, left_alone: None }])
/// the real table still holds 20
/// ```
///
/// An ordinary successful run, `code()` of `None`, and `§15.8`'s only enforced
/// bound pointing at something else for ever. The question belongs on the
/// relation the name resolved to, not on the path, so `only_for` asks
/// `relpersistence`.
#[tokio::test]
async fn a_temporary_table_of_the_same_name_is_refused() {
    let Some(pool) = pool("prune_temp_shadow").await else {
        return;
    };
    fill(&pool, 20).await;
    // **One connection, because a temporary relation belongs to a session.**
    // `pool()` hands out two, so a `CREATE TEMP TABLE` on one is invisible to the
    // other — which is exactly why this needs its own pool rather than the shared
    // one, and why the shadow is a real hazard for a service holding a single
    // long-lived connection.
    let options: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")
        .expect("checked by pool()")
        .parse()
        .expect("a valid url");
    let alone = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.options([(
            "search_path",
            // The same name `pool` built: it suffixes with the process id so two
            // concurrent runs cannot destroy each other's tables, and a second
            // pool has to agree or it looks into an empty schema.
            format!("prune_temp_shadow_{}", std::process::id()).as_str(),
        )]))
        .await
        .expect("connects into the schema");
    for statement in [
        "CREATE TEMP TABLE rows_over_time (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)",
        "INSERT INTO pg_temp.rows_over_time VALUES (1, 1), (2, 2), (3, 3)",
    ] {
        sqlx::query(statement)
            .execute(&alone)
            .await
            .expect("a temporary table is ours to make");
    }

    // **Pruned, not refused — and that is the stronger answer.** The pruner
    // pins its own path with `pg_temp` last, so a temporary table of the same
    // name is not what an unqualified name resolves to at all: the real table
    // is counted and pruned, and the shadow is simply not in the way. The
    // `relpersistence` arm below is still the guard for a caller who pins
    // `pg_temp` in front on purpose, and it is checked directly.
    let mut target = pruner(&alone, "prune_temp_shadow");
    let done = prune_these(&mut target, &[LIMIT])
        .await
        .expect("the real table is what an unqualified name reaches");
    assert_eq!(done[0].before, Some(20), "the temporary table was counted");

    // The shadow still holds its three rows, and the real table lost a batch.
    let shadow: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_temp.rows_over_time")
        .fetch_one(&alone)
        .await
        .expect("readable");
    assert_eq!(shadow, 3, "the temporary table was pruned");
    alone.close().await;
    assert_eq!(surviving(&pool).await, (5..=20).collect::<Vec<_>>());
    finish(pool).await;
}

/// Row-level security is judged by whether this role sees every row.
///
/// **Not by `rolbypassrls` alone.** `CREATE ROLE … SUPERUSER` leaves that
/// column `false` and a superuser bypasses RLS regardless, so asked that way,
/// the one role that *can* see every row is told it cannot — with
/// `NC-DB-0005`, which says retrying will not help. A bounded table under RLS
/// is then permanently unprunable on a superuser deployment, behind a sentence
/// naming a condition that does not hold:
///
/// ```text
/// plain  rolsuper=false  sees  2 of 20  ->  refused
/// super  rolsuper=true   sees 20 of 20  ->  refused, same sentence
/// ```
///
/// This runs as whichever role the developer's stack gives it and asserts the
/// answer matches what that role can actually see, so it is a real test on both
/// kinds of deployment rather than one that only holds on ours.
#[tokio::test]
async fn row_security_is_refused_only_when_this_role_cannot_see_past_it() {
    let Some(pool) = pool("prune_row_security").await else {
        return;
    };
    fill(&pool, 20).await;
    for statement in [
        "ALTER TABLE rows_over_time ENABLE ROW LEVEL SECURITY",
        "CREATE POLICY only_the_first ON rows_over_time USING (id < 3)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the test's own table is ours to manage");
    }

    let sees_everything: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&pool)
    .await
    .expect("the role can be asked about itself");
    // The floor under the assertion below: a role that bypasses RLS reads every
    // row, and one that does not reads two. If this ever came back equal, the
    // policy above did nothing and the test would be about nothing.
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM rows_over_time")
        .fetch_one(&pool)
        .await
        .expect("readable");
    assert_eq!(
        visible == 20,
        sees_everything,
        "this role sees {visible} of 20 rows, which does not match what pg_roles says \
         about it — so the two halves of the gate's own question disagree"
    );

    // **The role that separates the two halves of the question.** On a stack
    // whose role is both `rolsuper` and `rolbypassrls`, testing either one gives
    // the same answer — so this makes the one role that tells them apart:
    // superuser, and `rolbypassrls` false. PostgreSQL 18.6 sets the flag along
    // with `SUPERUSER`, so the role has to say `NOBYPASSRLS` — an ordinary thing
    // to write, and measured: it still reads every row.
    // It bypasses RLS, and a gate reading `rolbypassrls` alone says it cannot,
    // with `NC-DB-0005` — *retrying will not change it* — so a bounded table
    // under RLS is permanently unprunable on a superuser deployment.
    if sees_everything {
        // **Any this suite left behind first.** A role is cluster-global and
        // survives the schema its test made: a run that panics between creating
        // one and dropping it leaves it for ever, and twenty-five accumulate on
        // a developer's cluster that way, measured. The pid in the name is what
        // says whose it is, and a pid no longer running is nobody's — so this
        // sweeps those and leaves a concurrent run's alone, which a bare
        // `nc_prune_probe_%` sweep could not do.
        sweep_dead_probe_roles(&pool).await;
        let named = &format!("nc_prune_probe_super_{}", std::process::id());
        for statement in [
            format!("DROP ROLE IF EXISTS {named}"),
            format!("CREATE ROLE {named} SUPERUSER NOBYPASSRLS NOLOGIN"),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(&*statement))
                .execute(&pool)
                .await
                .expect("a scratch role is ours to make on our own stack");
        }
        // **One connection, because `SET ROLE` is session state.** The shared
        // pool hands out two, so through it the `SET ROLE` lands on one and
        // every later statement asks the other — which still answers as the
        // original role, and the assertion below catches it.
        let options: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")
            .expect("checked by pool()")
            .parse()
            .expect("a valid url");
        let alone = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options.options([(
                "search_path",
                format!("prune_row_security_{}", std::process::id()).as_str(),
            )]))
            .await
            .expect("connects into the schema");
        sqlx::query(sqlx::AssertSqlSafe(format!("SET ROLE {named}")))
            .execute(&alone)
            .await
            .expect("a superuser may become one");
        let (superuser, bypasses): (bool, bool) = sqlx::query_as(
            "SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(&alone)
        .await
        .expect("the role can be asked about itself");
        let mut as_super = pruner(&alone, "prune_row_security");
        let done = prune_these(&mut as_super, &[LIMIT]).await;
        alone.close().await;

        // **And the role this half made goes with it**, here and not in the
        // other half: a drop written beside the other half's role leaves this
        // one to outlive every run. A role is cluster-global, so "outlives"
        // means for ever.
        //
        // **Before the assertions, not after them.** With every drop on the
        // success path, a run that fails one of these assertions leaves a
        // `SUPERUSER` role behind — and the next `cargo xtask check` then fails
        // its journal and access-record protection step with `superusers=1`
        // against a declared `0`, which reads as a defect in the database rather
        // than as litter from a test. The sweep at the top of this function only
        // helps the *next* run, and only once the pid is dead. So the values are
        // taken, the connection closed and the role dropped, and only then is
        // anything asserted.
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE IF EXISTS {named}")))
            .execute(&pool)
            .await
            .expect("the scratch role goes with the test");

        assert!(
            superuser && !bypasses,
            "this role is not the one that separates the two questions: \
             rolsuper={superuser} rolbypassrls={bypasses}"
        );
        let done =
            done.expect("a superuser bypasses row-level security, whatever rolbypassrls says");
        assert_eq!(done[0].before, Some(20));

        the_refusal_side_too(&pool).await;
        finish(pool).await;
        return;
    }

    let mut target = pruner(&pool, "prune_row_security");
    let answer = prune_these(&mut target, &[LIMIT]).await;
    if sees_everything {
        let done = answer.expect("a role that reads every row can prune the table");
        assert_eq!(done[0].before, Some(20));
    } else {
        let refused = answer.expect_err("a role that reads two rows of twenty cannot");
        assert!(
            refused.contains("row-level security"),
            "refused for the wrong reason: {refused}"
        );
    }
    finish(pool).await;
}

/// Each half of the row-security question is decided by a case of its own.
///
/// **The arm has three clauses, and one test would decide only their
/// disjunction.** A probe role that is `SUPERUSER NOBYPASSRLS`
/// satisfies `pg_has_role(current_user, relowner, 'USAGE')` as surely as it
/// satisfies `rolsuper` — so each clause is the other's alibi. Measured
/// against the sibling test only, each deleted alone:
///
/// ```text
/// the owner clause deleted        cargo test -p nc-prune  SURVIVED, check passed
/// `rolsuper` deleted from its own SURVIVED, check passed
/// both deleted                    the sibling test FAILED
/// ```
///
/// The owner clause's own paragraph describes a plain `NOSUPERUSER NOBYPASSRLS`
/// owner of an RLS table without `FORCE`, which reads every row — and without
/// this, nothing exercises that arrangement. With the clause gone it is refused with
/// `NC-DB-0005`, *retrying will not help*, for ever.
///
/// So each case makes the role that isolates one clause:
///
/// * **the bypass clause** — a superuser over a table with `FORCE ROW LEVEL
///   SECURITY`, where the owner clause cannot rescue it and only `rolsuper` says
///   the count is the table;
/// * **the owner clause** — an ordinary role that owns the table without
///   `FORCE`, where the bypass clause is false and only ownership says so;
/// * **the privilege clause** — a role that cannot `SELECT` at all, which must
///   not be told about row-level security, because what stops it is a grant.
#[tokio::test]
async fn each_half_of_the_row_security_question_is_decided() {
    let Some(pool) = pool("prune_rls_clauses").await else {
        return;
    };
    let superuser: bool =
        sqlx::query_scalar("SELECT rolsuper FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&pool)
            .await
            .expect("the role can be asked about itself");
    // Every case here needs a role of its own, and only a superuser may make
    // one. On a stack whose role is ordinary this proves nothing and says so,
    // exactly as its sibling does.
    if !superuser {
        println!("nc-prune: the row-security clauses need a superuser to make probe roles");
        finish(pool).await;
        return;
    }
    fill(&pool, 20).await;
    sweep_dead_probe_roles(&pool).await;

    let schema = format!("prune_rls_clauses_{}", std::process::id());
    let mine = std::process::id();
    let names = [
        format!("nc_prune_probe_forced_{mine}"),
        format!("nc_prune_probe_owner_{mine}"),
        format!("nc_prune_probe_blind_{mine}"),
    ];
    for (named, how) in names.iter().zip([
        "SUPERUSER NOBYPASSRLS NOLOGIN",
        "NOSUPERUSER NOBYPASSRLS NOLOGIN",
        "NOSUPERUSER NOBYPASSRLS NOLOGIN",
    ]) {
        for statement in [
            format!("DROP ROLE IF EXISTS {named}"),
            format!("CREATE ROLE {named} {how}"),
            format!("GRANT USAGE ON SCHEMA {schema} TO {named}"),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(&*statement))
                .execute(&pool)
                .await
                .expect("a scratch role is ours to make on our own stack");
        }
    }
    for statement in [
        "ALTER TABLE rows_over_time ENABLE ROW LEVEL SECURITY".to_owned(),
        "CREATE POLICY only_the_first ON rows_over_time USING (id < 3)".to_owned(),
        // The blind role may name the table and may not read it, which is the
        // third clause's whole case.
        format!("GRANT DELETE ON rows_over_time TO {}", names[2]),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("the test's own table is ours to manage");
    }

    // **The bypass clause alone.** With `FORCE`, the owner clause is true for
    // everyone, so the only thing between this role and a refusal is `rolsuper`.
    sqlx::query("ALTER TABLE rows_over_time FORCE ROW LEVEL SECURITY")
        .execute(&pool)
        .await
        .expect("forcing is ours to set");
    let forced = as_role(&names[0], &schema).await;

    // **The owner clause alone.** No `FORCE`, an ordinary role that owns the
    // table: it reads every row, and nothing but ownership says so.
    for statement in [
        "ALTER TABLE rows_over_time NO FORCE ROW LEVEL SECURITY".to_owned(),
        format!("ALTER TABLE rows_over_time OWNER TO {}", names[1]),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("ours to set");
    }
    let owned = as_role(&names[1], &schema).await;

    // **The privilege clause alone.** A role that cannot read the table at all
    // must not be told that row-level security is why.
    let blind = as_role(&names[2], &schema).await;

    // **Everything put back and every role dropped before a single assertion**,
    // which is the discipline its sibling records the reason for: a role is
    // cluster-global, so an assertion failing here would otherwise leave a
    // `SUPERUSER` behind and the next `cargo xtask check` would fail its
    // protection census with `superusers=1`, reading as a defect in the database
    // rather than as litter from a test.
    sqlx::query("ALTER TABLE rows_over_time OWNER TO CURRENT_USER")
        .execute(&pool)
        .await
        .expect("ours to take back");
    for named in &names {
        for statement in [
            format!("REVOKE ALL ON rows_over_time FROM {named}"),
            format!("REVOKE ALL ON SCHEMA {schema} FROM {named}"),
            format!("DROP ROLE IF EXISTS {named}"),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(&*statement))
                .execute(&pool)
                .await
                .expect("the scratch roles go with the test");
        }
    }

    let forced = forced.expect("a superuser reads past FORCE ROW LEVEL SECURITY");
    assert_eq!(forced[0].removed, 4, "the forced case pruned nothing");
    let owned = owned.expect("a table's own owner reads every row of it without FORCE");
    assert_eq!(owned[0].removed, 4, "the owner case pruned nothing");
    let blind = blind.expect_err("a role with no SELECT cannot prune");
    assert!(
        !blind.contains("row-level security"),
        "a role that cannot read the table at all was told row-level security is why, \
         and what stops it is a grant: {blind}"
    );
    finish(pool).await;
}

/// One prune of `prune_rls_clauses`, run as `named` on a connection of its own.
///
/// `SET ROLE` is session state and the shared pool hands out more than one
/// connection, so a pool of one is the only way the role that runs the pruner is
/// the role that was set.
async fn as_role(named: &str, schema: &str) -> Result<Vec<nc_prune::Pruned>, String> {
    let options: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")
        .expect("checked by pool()")
        .parse()
        .expect("a valid url");
    let alone = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.options([("search_path", schema)]))
        .await
        .expect("connects into the schema");
    sqlx::query(sqlx::AssertSqlSafe(format!("SET ROLE {named}")))
        .execute(&alone)
        .await
        .expect("a superuser may become any role");
    let mut target = pruner(&alone, "prune_rls_clauses");
    let answer = prune_these(&mut target, &[LIMIT]).await;
    alone.close().await;
    answer
}

/// Every SQLSTATE the pruner treats as "waited, not refused" is named by a test.
///
/// **`WAITED_RATHER_THAN_REFUSED` is read back here, code by code, through
/// `waited_for_tests`.** Deleting the `40001` entry — whose own paragraph says
/// its absence ends a prune run and leaves every later table unlooked-at —
/// fails this test, and `the_codes_a_run_waited_for_are_the_ones_written_down`
/// holds the whole list from inside its own file. A list nothing reads back is
/// a list that shrinks quietly.
#[tokio::test]
async fn a_lock_this_run_waited_for_is_not_a_refusal() {
    let Some(pool) = pool("prune_waited").await else {
        return;
    };
    fill(&pool, 20).await;
    // One statement the database really refuses, so at least one code here is
    // PostgreSQL's own rather than a string written twice. A data exception is
    // the record's fault and never a wait.
    let refused = sqlx::query("SELECT (9223372036854775807::BIGINT + 1)")
        .execute(&pool)
        .await
        .expect_err("an overflow is refused");
    let code = refused
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .map(std::borrow::Cow::into_owned)
        .unwrap_or_default();
    assert_eq!(code, "22003");
    assert!(
        !nc_prune::postgres::waited_for_tests(&code),
        "a data exception is the record's fault; treating it as a wait would retry a \
         statement that cannot succeed"
    );
    // And the codes that really are permanent stay permanent, so the list above
    // is about which failures come back rather than about a list that says yes.
    for code in ["23505", "42P01", "42501", "22P02"] {
        assert!(
            !nc_prune::postgres::waited_for_tests(code),
            "`{code}` is a permanent refusal and this says a later run may succeed"
        );
    }
    // And the eight the list names, by name, so removing one fails here.
    //
    // **Each label beside its own code.** A comment written above its code
    // rather than beside it shifts every label below it along by one, and a
    // reader checking `55P03` against `query_canceled` then finds a
    // disagreement with PostgreSQL and has no way to tell which half is wrong.
    // **The operator-intervention class among them.** A restart
    // delivers `57P01` on the connection that is already open, so it arrives as
    // a `sqlx::Error::Database`, takes `answered = true`, and a code absent from
    // the list then reads as `NC-DB-0005` — *trying again will not help* — for a
    // server that is coming back. `Failure::answered` exists because a scheduler
    // "gave up for good on a database restart"; this is that outcome reached
    // through the other half of the same journey.
    //
    // Written out rather than iterated from the constant: a loop over
    // `WAITED_RATHER_THAN_REFUSED` builds its cases from the thing under test,
    // so removing an entry removes its own case.
    for code in [
        "55P03", "57014", "40P01", "40001", "57P01", "57P02", "57P03", "53300",
    ] {
        assert!(
            nc_prune::postgres::waited_for_tests(code),
            "`{code}` is in `WAITED_RATHER_THAN_REFUSED` and this says it is a refusal — \
             the run then ends and every later table goes unlooked-at"
        );
    }
    assert!(!nc_prune::postgres::waited_for_tests("23514"));
    finish(pool).await;
}

/// A schema named after the connecting role does not disable the pruner.
///
/// **A guard refusing any resolved path with a comma in it** fails on the
/// server default, `"$user", public` — one `CREATE SCHEMA` named after the
/// connecting role makes every bounded table permanently unprunable
/// behind `NC-DB-0005`, which says retrying will not help,
/// with `prune_these_with` returning on the first table. Measured through the
/// shipped entry point:
///
/// ```text
/// before: current_schemas(false) = {public}          -> Ok([Pruned { … }])
/// CREATE SCHEMA <role>;
/// after:  current_schemas(false) = {<role>,public}
///         Err("[NC-DB-0005] … the search path resolves to more than one schema")
/// ```
///
/// That is `§15.8`'s only enforced bound off for every table, caused by a schema
/// `CREATE SCHEMA AUTHORIZATION` creates on purpose.
#[tokio::test]
async fn a_schema_named_after_the_role_does_not_disable_the_pruner() {
    const SCHEMA: &str = "prune_role_schema";
    let Some(pool) = pool(SCHEMA).await else {
        return;
    };
    fill(&pool, 20).await;
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
        // And the schema the pruner is told about, which is what it must still
        // reach once `SET ROLE` has put the decoy in front on the path.
        format!(
            "GRANT USAGE ON SCHEMA \"{SCHEMA}_{}\" TO \"{role}\"",
            std::process::id()
        ),
        format!(
            "GRANT SELECT, DELETE ON ALL TABLES IN SCHEMA \"{SCHEMA}_{}\" TO \"{role}\"",
            std::process::id()
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("a scratch schema is ours to make on our own stack");
    }

    // A pool with no options at all, exactly as a deployment builds one, so its
    // path is the server default and resolves to two schemas.
    let plain = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("DATABASE_URL").expect("checked by pool()"))
        .await
        .expect("connects");
    // **The condition is established, not asserted about the caller.** Reading
    // the *server default* fails when `DATABASE_URL` names a schema — the very
    // form `in_schema` exists to support — so `cargo xtask check` could not
    // pass against it. `SET ROLE` makes `"$user"` resolve to the scratch role
    // this run created, which is what puts two schemas on the resolved path.
    for statement in [
        "SET search_path TO \"$user\", public",
        &format!("SET ROLE \"{role}\""),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&plain)
            .await
            .expect("ours to set on our own connection");
    }
    let resolved: String = sqlx::query_scalar("SELECT current_schemas(false)::text")
        .fetch_one(&plain)
        .await
        .expect("readable");
    assert!(
        resolved.contains(','),
        "the path resolves to `{resolved}`, so the old guard would not have fired and \
         this proves nothing"
    );

    let mut target =
        Postgres::in_schema(plain.clone(), &format!("{SCHEMA}_{}", std::process::id()));
    let done = prune_these(&mut target, &[LIMIT])
        .await
        .expect("a schema beside the one this pruner was told about is not its business");
    assert_eq!(done[0].before, Some(20));

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

/// A schema whose name needs quoting is still a schema this crate can prune.
///
/// **Not through `identifier`, the *table*-name validator**, which admits
/// only `[a-z_][a-z0-9_]*`. Through it, `"Prod"` or `audit-vault`, both legal
/// PostgreSQL schemas, make every bounded table permanently unprunable behind
/// `NC-DB-0005` and a sentence blaming `catalog/limits.toml`, which never
/// mentions the schema at all. Every bounded table sits in that schema, so
/// `§15.8`'s only enforced bound goes off for every table.
///
/// That is a rule refusing an ordinary arrangement. `nc-journal` binds its
/// schema and lets `quote_ident` handle any name.
#[tokio::test]
async fn a_schema_whose_name_needs_quoting_is_pruned_like_any_other() {
    let Some(pool) = pool("prune_quoted").await else {
        return;
    };
    let awkward = format!("Prune-Quoted {}", std::process::id());
    for statement in [
        format!("DROP SCHEMA IF EXISTS \"{awkward}\" CASCADE"),
        format!("CREATE SCHEMA \"{awkward}\""),
        format!(
            "CREATE TABLE \"{awkward}\".rows_over_time \
             (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)"
        ),
        format!(
            "INSERT INTO \"{awkward}\".rows_over_time \
             SELECT n, n FROM generate_series(1, 20) AS n"
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(&pool)
            .await
            .expect("a scratch schema is ours to make on our own stack");
    }

    let mut target = Postgres::in_schema(pool.clone(), &awkward);
    let done = prune_these(&mut target, &[LIMIT])
        .await
        .expect("a capital letter and a hyphen are not reasons to refuse a table");
    assert_eq!(done[0].before, Some(20));
    assert_eq!(done[0].removed, 4);

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA \"{awkward}\" CASCADE"
    )))
    .execute(&pool)
    .await
    .expect("ours to drop, because this run made it");
    finish(pool).await;
}

/// A foreign key with `ON DELETE CASCADE` pointing at the table is a refusal.
///
/// **The arm, driven.** `a_delete_that_could_reach_another_table_is_refused`
/// drives the rule arm and the trigger arm, which sit on either side of it in
/// the same `CASE`, and without this the cascade arm between them is held by
/// its own text. It is the one of the three that needs no code at all to arm: a
/// plain `REFERENCES … ON DELETE CASCADE` in another table's definition is
/// enough, which makes it the likeliest of the three to be met in practice and
/// the one whose loss would be quietest — `rows_affected` counts the rows this
/// crate deleted and not the rows the cascade took with them, so a run would
/// report `removed: 4` while eight rows went.
///
/// All three of `'c'`, `'n'` and `'d'` are driven, because the arm names three
/// `confdeltype` values and a test of one holds the other two by spelling.
#[tokio::test]
async fn a_cascade_pointing_at_the_table_is_refused() {
    let Some(pool) = pool("prune_cascade").await else {
        return;
    };
    fill(&pool, 20).await;
    for (child, action) in [
        ("cascades", "CASCADE"),
        ("nulls_out", "SET NULL"),
        ("defaults_back", "SET DEFAULT"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE {child} (id BIGINT PRIMARY KEY, \
             parent BIGINT DEFAULT 20 REFERENCES rows_over_time (id) ON DELETE {action})"
        )))
        .execute(&pool)
        .await
        .expect("the test's own table");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {child} SELECT n, n FROM generate_series(1, 10) AS n"
        )))
        .execute(&pool)
        .await
        .expect("writable");

        let mut target = pruner(&pool, "prune_cascade");
        let refused = target
            .delete_oldest("rows_over_time", "at", 4)
            .await
            .expect_err("a cascade into this table is a reason to refuse it");
        assert!(
            refused.why.contains("ON DELETE CASCADE"),
            "the refusal about `{action}` does not name the cascade: {refused:?}"
        );
        assert!(
            !refused.waited,
            "a permanent refusal was reported as a wait: {refused:?}"
        );

        // The floor for this constraint: nothing went from either table while
        // it stood. Dropped afterwards so the next `confdeltype` is the
        // only one in place and is what the refusal is answering.
        assert_eq!(surviving(&pool).await, (1..=20).collect::<Vec<_>>());
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {child}")))
            .execute(&pool)
            .await
            .expect("ours to drop");
    }

    // And with all three gone the same call goes through, so the refusals above
    // were the constraints and not something standing behind them.
    let mut target = pruner(&pool, "prune_cascade");
    assert_eq!(
        target
            .delete_oldest("rows_over_time", "at", 4)
            .await
            .expect("nothing references it now"),
        4
    );
    finish(pool).await;
}

/// The five exemptions inside the refusals are what keep an ordinary table prunable.
///
/// **Every arm of `only_for`'s `CASE`, driven from the accepting side
/// too.** The rule arm asks `ev_type = '4' AND rulename <> '_RETURN'`, the
/// constraint arm `confdeltype IN ('c','n','d')`, the trigger
/// arm `NOT tgisinternal AND tgenabled <> 'D'` — five clauses whose whole job
/// is to *let* an ordinary arrangement through. Measured with no test asking
/// any of them to, all five removed at once:
///
/// ```text
/// cargo test -p nc-prune  ->  27 passed / 34 passed
/// cargo xtask check       ->  check passed, ok 1195 test(s)
///
/// delete_oldest(t_disabled) -> "a trigger fires on DELETE and may touch another table"
/// delete_oldest(t_fk)       -> "another table references it with ON DELETE CASCADE, …"
/// delete_oldest(t_rule)     -> "a rule rewrites DELETE on it"
/// ```
///
/// Each sentence is false about the table it
/// refuses. `ALTER TABLE … DISABLE TRIGGER`, a
/// plain `REFERENCES … ON DELETE NO ACTION` and
/// an `ON INSERT … DO ALSO NOTHING` rule are ordinary things, and
/// behind `NC-DB-0005` — which says retrying will not help — the table then
/// grows for ever with the gate green. This is the direction that is easy to
/// get wrong.
#[tokio::test]
async fn an_ordinary_arrangement_is_pruned_rather_than_refused() {
    let Some(pool) = pool("prune_exempt").await else {
        return;
    };
    fill(&pool, 20).await;
    // Each of the three, put in place on its own and taken away again, so the
    // prune that follows is answering that one arrangement.
    for (what, put, take) in [
        (
            "a trigger that is disabled",
            "CREATE FUNCTION touches() RETURNS TRIGGER LANGUAGE plpgsql AS \
             $$ BEGIN RETURN OLD; END; $$; \
             CREATE TRIGGER on_delete AFTER DELETE ON rows_over_time \
             FOR EACH ROW EXECUTE FUNCTION touches(); \
             ALTER TABLE rows_over_time DISABLE TRIGGER on_delete",
            "DROP TRIGGER on_delete ON rows_over_time; DROP FUNCTION touches()",
        ),
        (
            "a reference that does not cascade",
            "CREATE TABLE holds_still (id BIGINT PRIMARY KEY, \
             parent BIGINT REFERENCES rows_over_time (id) ON DELETE NO ACTION)",
            "DROP TABLE holds_still",
        ),
        (
            "a rule that is not about DELETE",
            "CREATE RULE says_nothing AS ON INSERT TO rows_over_time DO ALSO NOTHING",
            "DROP RULE says_nothing ON rows_over_time",
        ),
    ] {
        sqlx::raw_sql(put)
            .execute(&pool)
            .await
            .expect("the test's own arrangement");
        let mut target = pruner(&pool, "prune_exempt");
        let taken = target.delete_oldest("rows_over_time", "at", 4).await;
        sqlx::raw_sql(take)
            .execute(&pool)
            .await
            .expect("ours to drop");
        assert_eq!(
            taken
                .as_ref()
                .copied()
                .map_err(|refused| refused.why.clone()),
            Ok(4),
            "`{what}` reaches no other table and the prune was refused"
        );
        // And the rows really went, so the exemption let the work happen rather
        // than answering four about nothing.
        sqlx::query(
            "INSERT INTO rows_over_time (id, at) SELECT n, n * 1000 FROM \
                     generate_series(1, 4) AS n",
        )
        .execute(&pool)
        .await
        .expect("writable");
        assert_eq!(surviving(&pool).await, (1..=20).collect::<Vec<_>>());
    }
    finish(pool).await;
}

/// A table that inherits from another is refused, which is the arm's own half.
///
/// **Three other tests name this arm and all three drive the one above
/// it.** `an_inherited_table_is_refused_too` prunes the *parent*, the
/// decoy-parent test prunes the decoy parent, and the view test prunes a
/// *partition* child, which `relispartition` answers. So `inhrelid = c.oid` —
/// the clause that exists because *"a partition child is `relkind = 'r'` and
/// an `inhrelid`, not an `inhparent`"* — is decided by this alone. Measured
/// with `inhrelid = 0` planted and this absent: `cargo test -p nc-prune`
/// green, `check passed`, and a child table pruned four rows deep.
#[tokio::test]
async fn a_table_that_inherits_from_another_is_refused() {
    let Some(pool) = pool("prune_child").await else {
        return;
    };
    fill(&pool, 10).await;
    sqlx::raw_sql(
        "CREATE TABLE inherits_rows (id BIGINT PRIMARY KEY, at BIGINT NOT NULL) \
         INHERITS (rows_over_time); \
         INSERT INTO inherits_rows (id, at) SELECT n, n * 1000 FROM generate_series(1, 10) AS n",
    )
    .execute(&pool)
    .await
    .expect("the test's own child table");

    let mut target = pruner(&pool, "prune_child");
    let refused = target
        .delete_oldest("inherits_rows", "at", 4)
        .await
        .expect_err("a table that inherits is part of one this catalog does not name");
    assert!(
        refused.why.contains("inherits"),
        "the refusal does not say what it is about: {refused:?}"
    );
    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM inherits_rows")
        .fetch_one(&pool)
        .await
        .expect("counts");
    assert_eq!(held, 10, "the refusal let rows go anyway");
    finish(pool).await;
}

/// A caller who pins `pg_temp` in front reaches the temporary table, and is refused.
///
/// **The arm's own test does not drive
/// it.** `a_temporary_table_of_the_same_name_is_refused` runs against
/// a `bounded()` that pins the configured schema with `pg_temp` last, so that
/// test measures the *real* table being pruned and the shadow being left alone
/// — the stronger answer, and not a test of `relpersistence` at all. Its own
/// comment says the arm "is checked directly", and this is where.
///
/// This is the route that remains: a store told its schema is `pg_temp`. The
/// pinned path is then `pg_temp, pg_temp` and an unqualified name resolves to the
/// temporary relation, which is precisely the case the arm's sentence describes —
/// the bounded table is not the table being counted.
#[tokio::test]
async fn a_store_pinned_to_the_temporary_schema_is_refused() {
    let Some(pool) = pool("prune_temp_pinned").await else {
        return;
    };
    fill(&pool, 20).await;
    let options: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")
        .expect("checked by pool()")
        .parse()
        .expect("a valid url");
    let alone = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.options([(
            "search_path",
            format!("prune_temp_pinned_{}", std::process::id()).as_str(),
        )]))
        .await
        .expect("connects into the schema");
    for statement in [
        "CREATE TEMP TABLE rows_over_time (id BIGINT PRIMARY KEY, at BIGINT NOT NULL)",
        "INSERT INTO pg_temp.rows_over_time VALUES (1, 1), (2, 2), (3, 3)",
    ] {
        sqlx::query(statement)
            .execute(&alone)
            .await
            .expect("a temporary table is ours to make");
    }

    let mut target = Postgres::in_schema(alone.clone(), "pg_temp");
    let refused = target
        .delete_oldest("rows_over_time", "at", 4)
        .await
        .expect_err("a temporary table is not the table the catalog bounds");
    assert!(
        refused.why.contains("temporary table"),
        "the refusal does not name what it refused: {refused:?}"
    );
    assert!(
        !refused.waited,
        "a permanent refusal was reported as a wait: {refused:?}"
    );

    // The floor: neither table lost a row — not the shadow the call reached, and
    // not the real one it would have reached under any other schema.
    let shadow: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_temp.rows_over_time")
        .fetch_one(&alone)
        .await
        .expect("readable");
    assert_eq!(shadow, 3, "the temporary table was pruned");
    alone.close().await;
    assert_eq!(surviving(&pool).await, (1..=20).collect::<Vec<_>>());
    finish(pool).await;
}

/// Everything this crate sets ends with its transaction.
///
/// **`set_config`'s third argument is the whole of it.** `true` is `SET LOCAL`
/// and `false` is `SET`; the two spellings differ by one word in a line nothing
/// read, and the wrong one is invisible to every other test in this file — the
/// prune succeeds either way. What it changes is what happens *after*: the
/// connection goes back to the pool holding `<schema>, pg_temp`, and the next
/// borrower of it — `nc-journal`, a migration, the application — resolves its own
/// unqualified names in a schema this crate chose. On a one-connection pool that
/// is every subsequent statement.
///
/// So the assertion is on the connection after the run, not on the run. A single
/// connection, because a pooled borrow is what carries the leak.
///
/// **And `lock_timeout`, which is the same statement one line up.** `bounded()`
/// writes two things onto its transaction — the bound and the path — and both
/// are held here. Measured without this, `SET LOCAL lock_timeout = '5s'`
/// replaced by `SET lock_timeout = '5s'`: `cargo run --quiet -p xtask -- check`
/// at `check passed`, console byte-identical to a clean run, with the
/// connection carrying `5s` back to the pool — there is no `after_release` hook
/// anywhere in this workspace. The next borrower of it then has every lock wait
/// cancelled at five seconds with `55P03`, which `nc-journal` maps to a
/// *retryable* `Unavailable`: an audit append reported as a transient outage,
/// and a migration taking `ACCESS EXCLUSIVE` simply aborting.
///
/// `a_locked_table_ends_the_run_rather_than_holding_it` proves the bound
/// *fires*; this is the half that proves it *ends*.
#[tokio::test]
async fn what_this_crate_sets_does_not_outlive_its_transaction() {
    let schema = format!("prune_local_{}", std::process::id());
    let Some(pool) = pool("prune_local").await else {
        return;
    };
    fill(&pool, 20).await;
    let options: sqlx::postgres::PgConnectOptions = std::env::var("DATABASE_URL")
        .expect("checked by pool()")
        .parse()
        .expect("a valid url");
    let alone = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.options([("search_path", schema.as_str())]))
        .await
        .expect("connects into the schema");

    let before: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&alone)
        .await
        .expect("the session answers");
    assert_eq!(
        before, schema,
        "the session did not start where it was pointed"
    );
    // The floor under the bound's half: a session that already carried a bound
    // would satisfy the comparison below by having been leaked into beforehand.
    let unbounded: String = sqlx::query_scalar("SHOW lock_timeout")
        .fetch_one(&alone)
        .await
        .expect("the session answers");
    assert_eq!(
        unbounded, "0",
        "this session already waits with a bound, so what it holds afterwards says \
         nothing about what the prune set"
    );

    let mut target = pruner(&alone, "prune_local");
    let done = prune_these(&mut target, &[LIMIT])
        .await
        .expect("an ordinary prune");
    // The floor: the run this is asking about actually happened. A prune that
    // refused would leave the path untouched for a reason that is not the one
    // being tested, and the assertion below would pass by not having run.
    assert_eq!(
        done[0].removed, 4,
        "nothing was pruned, so nothing was pinned"
    );

    let after: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&alone)
        .await
        .expect("the session answers");
    assert_eq!(
        after, before,
        "the pruner's path outlived its transaction and is now what every later \
         borrower of this connection resolves names in"
    );
    let bound: String = sqlx::query_scalar("SHOW lock_timeout")
        .fetch_one(&alone)
        .await
        .expect("the session answers");
    assert_eq!(
        bound, unbounded,
        "the pruner's lock bound outlived its transaction, so every later borrower \
         of this connection has its lock waits cancelled at five seconds — an audit \
         append reported as a retryable outage, and a migration taking ACCESS \
         EXCLUSIVE aborting"
    );
    alone.close().await;
    finish(pool).await;
}

/// The half of the RLS arm the shipped stack cannot otherwise reach.
///
/// **Its own function because the two halves would cross the line limit**, and
/// because they are two questions: above, that a role which sees every row is
/// *not* refused; here, that a role which sees two of twenty is. The refusal is
/// driven only on a deployment whose connecting role is neither `rolsuper`
/// nor `rolbypassrls`, and this stack's `noctua` is both — measured `t|t` — so
/// without this the `else` at the bottom of the caller never runs here and
/// deleting the whole arm leaves `cargo xtask check` green.
async fn the_refusal_side_too(pool: &sqlx::PgPool) {
    // **And the other side of the arm, which this stack cannot otherwise
    // reach.** Everything above proves the gate does *not* refuse a role that
    // sees every row. The refusal itself — the sentence the arm carries, and
    // the reason the arm exists — is otherwise driven only on a deployment
    // whose connecting role is neither `rolsuper` nor `rolbypassrls`, and the
    // shipped stack's `noctua` is both: measured `t|t`, so the `else` at the
    // bottom of this test never runs here. Without this, deleting the whole RLS
    // arm leaves `cargo xtask check` green.
    //
    // So the plain role is made too. It is not the table's owner and not a
    // member of the owner's role, which is what the arm's third clause asks,
    // and the policy above lets it see two rows of twenty.
    // **The process id, because a role is cluster-global.** A schema is
    // suffixed everywhere in this file for exactly this reason; unsuffixed,
    // two concurrent runs of the suite drop each other's probe roles — and a
    // run that dies holding privileges leaves a role the next run cannot
    // drop.
    let plain = &format!("nc_prune_probe_plain_{}", std::process::id());
    // **The table put back first.** The prune above took the four oldest
    // rows, and the policy is `id < 3` — so the plain role would see none of
    // them and the refusal below would be about an empty table rather than
    // about a partial view of a full one.
    sqlx::query("INSERT INTO rows_over_time SELECT n, n FROM generate_series(1, 4) AS n")
        .execute(pool)
        .await
        .expect("writable");
    assert_eq!(surviving(pool).await, (1..=20).collect::<Vec<_>>());
    for statement in [
        format!("DROP ROLE IF EXISTS {plain}"),
        format!("CREATE ROLE {plain} NOSUPERUSER NOBYPASSRLS NOLOGIN"),
        format!(
            "GRANT USAGE ON SCHEMA prune_row_security_{} TO {plain}",
            std::process::id()
        ),
        format!("GRANT SELECT, DELETE ON rows_over_time TO {plain}"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(pool)
            .await
            .expect("a scratch role is ours to make on our own stack");
    }
    let one_more = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            std::env::var("DATABASE_URL")
                .expect("checked by pool()")
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("a valid url")
                .options([(
                    "search_path",
                    format!("prune_row_security_{}", std::process::id()).as_str(),
                )]),
        )
        .await
        .expect("connects into the schema");
    sqlx::query(sqlx::AssertSqlSafe(format!("SET ROLE {plain}")))
        .execute(&one_more)
        .await
        .expect("a superuser may become one");
    // The floor: this role really is behind the policy. If it read twenty
    // rows the refusal below would be about something else.
    let behind: i64 = sqlx::query_scalar("SELECT count(*) FROM rows_over_time")
        .fetch_one(&one_more)
        .await
        .expect("readable");
    assert_eq!(behind, 2, "the plain role is not behind the policy");

    let mut restricted = pruner(&one_more, "prune_row_security");
    let refused = prune_these(&mut restricted, &[LIMIT])
        .await
        .expect_err("a role that reads two rows of twenty cannot prune the table");
    assert!(
        refused.contains("row-level security"),
        "refused for the wrong reason: {refused}"
    );
    // **And the sentence is one that is true of a role which sees everything,
    // too.** The predicate asks whether row security *applies*; it cannot ask
    // whether row security *hides* anything, because answering that means
    // comparing what this role sees with what is there, and that is the
    // privilege it lacks. So a role under a permissive `USING (true)` policy is
    // refused as well — measured, seeing 20 of 20 — and a sentence telling it
    // "neither of which is the table the catalog bounds" would be untrue.
    // Refusing is the safe direction; claiming to know why is not.
    assert!(
        refused.contains("cannot establish"),
        "the refusal asserts what row security is hiding, and this crate cannot know \
         it: {refused}"
    );
    assert!(
        refused.contains("BYPASSRLS"),
        "a permanent refusal that names no way out is a table nobody can prune: \
         {refused}"
    );

    // The role that sees every row is refused with the same true sentence,
    // rather than a different false one.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE POLICY everything_for_{plain} ON rows_over_time FOR ALL TO {plain} \
         USING (true)"
    )))
    .execute(pool)
    .await
    .expect("a permissive maintenance policy is an ordinary thing to write");
    let sees_all: i64 = sqlx::query_scalar("SELECT count(*) FROM rows_over_time")
        .fetch_one(&one_more)
        .await
        .expect("readable");
    assert_eq!(
        sees_all, 20,
        "the permissive policy did not take effect, so this proves nothing"
    );
    let mut permissive = pruner(&one_more, "prune_row_security");
    let refused = prune_these(&mut permissive, &[LIMIT])
        .await
        .expect_err("this crate cannot establish what a policy hides");
    assert!(
        refused.contains("cannot establish") && refused.contains("BYPASSRLS"),
        "a role that reads every row was refused with a sentence about rows it cannot \
         see: {refused}"
    );
    one_more.close().await;

    // And nothing went: a `DELETE` that ran under the policy would have taken
    // the two rows this role can reach and reported them as the batch.
    assert_eq!(surviving(pool).await, (1..=20).collect::<Vec<_>>());

    for statement in [
        format!("DROP OWNED BY {plain}"),
        format!("DROP ROLE IF EXISTS {plain}"),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(&*statement))
            .execute(pool)
            .await
            .expect("the scratch roles go with the test");
    }
}

/// A refused table carries the refusal's own code, and a locked one carries a retryable code.
///
/// **Two verdicts a scheduler reads, held here.**
///
/// `Pruned::code()`'s `ItWasRefused` arm — the one variant carrying a hard
/// failure — must not return `None`, and without this nothing says so:
/// replacing the arm with `Some(Untouched::ItWasRefused(_)) => None`
/// leaves `cargo test --workspace` green, and the arm's own comment says it
/// exists because in the catch-all *"a scheduler filtering
/// on `code().is_some()` would see every lock skip and miss every
/// refusal"*. `the_code_reaches_whoever_reads_the_result` asserts the lock arm
/// and the `None` arm and never builds a refusal.
///
/// `said()`'s SQLSTATE lookup — the thing that decides retryable from permanent
/// — is otherwise asserted only through `waited_for_tests`, a second function
/// reading the same list. Without this, nothing would say the shipped path
/// still consults it: `let waited = false;` in `said()` would be green across
/// the workspace, and the run then flips from a skip row to an error:
///
/// ```text
/// pristine:  Ok   left_alone = SomethingElseHeldALock
///                 code() = DbPruneFailed   retryable = true    NC-DB-0003
/// mutated:   Err  [NC-DB-0005] could not be counted: lock timeout
///                                          retryable = false   NC-DB-0005
/// ```
///
/// `NC-DB-0003` says *a later run may*; `NC-DB-0005` says *retrying will not
/// change it*. A responder is told a lock the next tick would get is permanent.
/// `a_locked_table_ends_the_run_rather_than_holding_it` sets a real lock up and
/// asserts only `is_err()` and the elapsed time, so it cannot see either.
#[tokio::test]
async fn a_skip_and_a_refusal_each_carry_the_code_a_scheduler_reads() {
    let Some(pool) = pool("prune_verdicts").await else {
        return;
    };
    fill(&pool, 20).await;

    // **The refusal**, reached the way a caller reaches one: a table the reach
    // guard will not prune, under a limit the catalog declares.
    sqlx::query("CREATE VIEW rows_over_time_v AS SELECT * FROM rows_over_time")
        .execute(&pool)
        .await
        .expect("the test's own view");
    let as_a_view = TableLimit {
        name: "rows_over_time_v",
        database: nc_catalog::Database::Server,
        ..LIMIT
    };
    // **Two tables, because that is when a refusal becomes a row.** A run whose
    // every table was refused returns `Err` — there is nothing to report but the
    // refusal — and it is the mixed run that carries one, which is exactly the
    // shape `Pruned::code()` exists for: a scheduler reading a result that holds
    // both a success and a hard failure.
    let mut target = pruner(&pool, "prune_verdicts");
    let done = prune_these(&mut target, &[LIMIT, as_a_view])
        .await
        .expect("a refusal beside a success is carried, not thrown");
    let refused = done
        .iter()
        .find(|pruned| pruned.table == "rows_over_time_v")
        .expect("one row per table asked about");
    assert!(
        matches!(
            refused.left_alone,
            Some(nc_prune::Untouched::ItWasRefused(_))
        ),
        "a view was not refused: {refused:?}"
    );
    let code = refused
        .code()
        .expect("a refusal is a hard failure and a scheduler filters on the code");
    assert!(
        !code.retryable(),
        "`{}` says a refusal is worth another tick: {refused:?}",
        code.code()
    );

    // **The skip**, reached with a real `ACCESS EXCLUSIVE` lock held by another
    // session, which is what the pruner's `lock_timeout` is for.
    let holder = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            std::env::var("DATABASE_URL")
                .expect("checked by pool()")
                .parse::<sqlx::postgres::PgConnectOptions>()
                .expect("a valid url")
                .options([(
                    "search_path",
                    format!("prune_verdicts_{}", std::process::id()).as_str(),
                )]),
        )
        .await
        .expect("connects into the schema");
    let mut held = holder.begin().await.expect("a transaction of its own");
    sqlx::query("LOCK TABLE rows_over_time IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *held)
        .await
        .expect("ours to lock");

    let done = prune_these(&mut target, &[LIMIT])
        .await
        .expect("a lock is a skip, not the end of the run");
    let skipped = done.first().expect("one row per table asked about");
    assert!(
        matches!(
            skipped.left_alone,
            Some(nc_prune::Untouched::SomethingElseHeldALock)
        ),
        "a locked table was not skipped: {skipped:?}"
    );
    let code = skipped
        .code()
        .expect("a skip carries a code a scheduler reads");
    assert!(
        code.retryable(),
        "`{}` tells a responder a lock the next tick would get is permanent: {skipped:?}",
        code.code()
    );

    // The floor: nothing went while the lock was held, so the skip is a skip.
    held.rollback().await.expect("ours to roll back");
    holder.close().await;
    // The four the first run took, and nothing more: the locked run skipped.
    assert_eq!(surviving(&pool).await, (5..=20).collect::<Vec<_>>());

    sqlx::query("DROP VIEW rows_over_time_v")
        .execute(&pool)
        .await
        .expect("ours to drop");
    finish(pool).await;
}

/// Every prefix a role of this suite begins with, as a `LIKE` pattern.
///
/// **Every prefix this file writes is swept.** A sweep asking
/// for `nc\_prune\_probe\_%` misses `nc_role_probe_<pid>`, a cluster-global
/// name this file also creates. A failing test skips `finish`, so the role
/// would stay for ever in a cluster the developer shares — and RULE ZERO's own
/// instruction is that a scratch role is dropped on the way in and on the way
/// out.
///
/// `\_` matters: unescaped, `_` is `LIKE`'s any-character wildcard, so
/// `nc\_prune\_probe\_` also matches a name belonging to something else.
const THE_ROLES_THIS_SUITE_MAKES: [&str; 2] = ["nc\\_prune\\_probe\\_", "nc\\_role\\_probe\\_"];

/// Drops probe roles left by runs that are no longer running.
///
/// The liveness question is asked here rather than in SQL, because the database
/// cannot ask it — and asking it is what separates a leftover from a concurrent
/// run's role. `DROP OWNED BY` first: a role holding a grant cannot be dropped,
/// so without it a leftover is one that cannot be cleaned up.
async fn sweep_dead_probe_roles(pool: &sqlx::PgPool) {
    // **Every prefix the list carries, and the list is what decides.** A query
    // with the prefix hard-coded beside `THE_ROLES_THIS_SUITE_MAKES` makes the
    // test that holds the names against the constant its only reader, so
    // `every_role_this_suite_makes_is_one_the_sweep_reaches` would assert
    // something the sweep does not do. Its sibling `pool()` does it right:
    // `for prefix in THE_SCHEMAS_THIS_SUITE_MAKES`. One rule, two copies, and
    // the copy that runs has to be the wide one.
    let mut left: Vec<String> = Vec::new();
    for prefix in THE_ROLES_THIS_SUITE_MAKES {
        let found: Vec<String> =
            sqlx::query_scalar("SELECT rolname::text FROM pg_roles WHERE rolname LIKE $1 || '%'")
                .bind(prefix)
                .fetch_all(pool)
                .await
                .expect("the catalog answers");
        left.extend(found);
    }
    for role in left {
        // **A role carrying this process's own pid is not a leftover.** Asked
        // with `filter(|pid| *pid != mine)` in front of it, the liveness
        // question answers "not running" for exactly the roles this run owns —
        // harmless with one test in this file making them, and a concurrent
        // sibling's roles drop out from under it the moment a second one does.
        // Measured: `SET ROLE nc_prune_probe_owner_<pid>` comes back *role does
        // not exist*, from a role created eleven lines earlier in the same test.
        //
        // What such a filter is for is a pid reused by a later process, and the
        // `DROP ROLE IF EXISTS` every creator here runs immediately before its
        // `CREATE ROLE` already covers that — by name, rather than by sweeping
        // names another test may be using.
        let running = role
            .rsplit('_')
            .next()
            .and_then(|pid| pid.parse::<i32>().ok())
            .is_some_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists());
        if running {
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

/// Every prefix a schema of this suite begins with, as a `LIKE` pattern.
///
/// **Every prefix this file writes is swept.** With `prune_` alone, the eight
/// tests here that name their schema `nc_prune_…` leave open the leak the sweep
/// exists to end — measured: three schemas planted from pids that are not
/// running, a full green run, and the two `nc_prune_` ones still standing
/// afterwards. A leaked schema is not a red test, which is why it needs this.
///
/// The sibling sweep in this file, `sweep_dead_probe_roles`, escapes its
/// underscores, and this one does too: one rule in one file has one spelling.
/// `\_` matters: unescaped, `_` is
/// `LIKE`'s any-character wildcard, so `prune_` also matches `prunes…` and a
/// schema belonging to something else.
///
/// It is a list rather than two calls so that
/// `every_schema_this_suite_makes_is_one_the_sweep_reaches` can hold it against
/// the names this file actually writes.
const THE_SCHEMAS_THIS_SUITE_MAKES: [&str; 4] = [
    "prune\\_",
    "nc\\_prune\\_",
    // **And the two this file builds with `format!`, which a scan of literal
    // openers cannot see.** `Prune-Quoted <pid>` and `nc_role_probe_<pid>` are
    // named in `format!` calls rather than written whole, so the two literal
    // openers the scan reads go past them — measured live: five schemas planted
    // from a dead pid, one green run, and these two still standing afterwards.
    "Prune-Quoted ",
    "nc\\_role\\_probe\\_",
];

/// The one binding in this file whose schema name has no head of its own.
///
/// Written out whole and required to be the file's only one, rather than
/// matched by a shape. Assembled, because this file reads itself: written
/// contiguously the constant would be its own second occurrence.
const THE_BINDING_WHOSE_HEAD_IS_AN_ARGUMENT: &str = concat!(
    "let schema = &format!(\"{schema}_{}\", std::",
    "process::id());"
);

/// The one statement that uses it, on the same terms.
const THE_STATEMENT_THAT_USES_IT: &str = concat!("format!(\"CREATE ", "SCHEMA {schema}\")");

/// Whether the name with no head of its own is **the** one, by exact text.
///
/// **A name with an empty head has to be excused by something, and only one
/// kind of something holds.** `format!("{schema}_{pid}")`
/// begins with a placeholder, so there is no literal head to match a swept
/// prefix against. It is legitimate exactly once, in the function every test
/// calls to get its pool, because the head is the argument — and the arguments
/// are the literals the loop below reads and already requires to be swept.
///
/// Excusing *any* binding whose `format!` opens with a placeholder turns the
/// rule off for a whole spelling. Narrowing it to a self-shadowing binding
/// inside `pool`, found by that function's name and the first `\n}\n` after it,
/// breaks in both directions at once. Four structural questions — the enclosing
/// item, the opener list, the parameter, the first binding — break in both
/// directions too: three lines of raw string holding a column-zero line that
/// reads `async fn pool(leak: &str)` **are** a function header to a rule that
/// reads the file as text, and three of its four conjuncts would be decided by
/// nothing.
///
/// **So it is a pin, not a rule.** An excuse is a hole in a
/// gate, and a hole must fail closed: the one pair this file actually needs is
/// named by its exact text, each half required to occur exactly **once**, and
/// every other shape — a second copy of either, a variant spelling, the
/// statement before the binding, a fake header in a string — is a red test that
/// says what to do. A pin is the narrowest possible excuse and it has no
/// too-weak direction at all; what it costs is that editing `pool`'s binding
/// turns this red, which is the loud half of the trade and the one to take.
///
/// Four conjuncts and no fifth: `bound < at` is left out, because the caller
/// finds the binding with `source[..at].rfind`, so it cannot be false. A
/// conjunct nothing can decide is the defect this pin replaces.
fn the_head_is_an_argument_already_read(source: &str, at: usize, bound: usize) -> bool {
    let opens = "format!(\"".len();
    source.matches(THE_STATEMENT_THAT_USES_IT).count() == 1
        && source
            .matches(THE_BINDING_WHOSE_HEAD_IS_AN_ARGUMENT)
            .count()
            == 1
        && source
            .find(THE_STATEMENT_THAT_USES_IT)
            .map(|found| found + opens)
            == Some(at)
        && source.find(THE_BINDING_WHOSE_HEAD_IS_AN_ARGUMENT) == Some(bound)
}

/// Each way the pin can be missed, decided by a source that misses it.
///
/// **Conjuncts this file satisfies all at once decide nothing**: measured, each
/// of three can be deleted with the rest of the file green, because this file
/// holds one name with an empty head and it satisfies every clause at once. A
/// test that computes `at` and `bound` with the same `find` the rule uses has
/// the same defect one level down — the two conjuncts comparing them cannot
/// fail. This one finds them the way the **caller** does instead: `at` from a
/// statement the walk reached, `bound` from `source[..at].rfind`.
#[test]
fn the_pin_decides_each_way_it_can_be_missed() {
    let binding = THE_BINDING_WHOSE_HEAD_IS_AN_ARGUMENT;
    let statement = THE_STATEMENT_THAT_USES_IT;
    let other = concat!("format!(\"CREATE ", "SCHEMA {other}\")");
    let elsewhere = "let schema = &format!(\"{schema}_x\");";
    let whole = |body: &str| format!("fn pool(schema: &str) {{\n    {body}\n}}\n");
    let cases: [(&str, String, &str, bool); 5] = [
        (
            "the pair this file has",
            whole(&format!("{binding}\n    let _ = q(&{statement});")),
            statement,
            true,
        ),
        (
            "the binding written twice, the second copy below the statement",
            whole(&format!(
                "{binding}\n    let _ = q(&{statement});\n    {binding}"
            )),
            statement,
            false,
        ),
        (
            "the statement written twice",
            whole(&format!(
                "{binding}\n    let _ = q(&{statement});\n    let _ = q(&{statement});"
            )),
            statement,
            false,
        ),
        (
            "a second statement, reached by the walk",
            whole(&format!(
                "{binding}\n    let _ = q(&{statement});\n    let _ = q(&{other});"
            )),
            other,
            false,
        ),
        (
            "a binding between the pinned one and the statement",
            whole(&format!(
                "{binding}\n    {elsewhere}\n    let _ = q(&{statement});"
            )),
            statement,
            false,
        ),
    ];
    for (what, source, reached, expected) in cases {
        let opens = "format!(\"".len();
        let at = source
            .find(reached)
            .map(|found| found + opens)
            .expect("every row holds the statement its walk reaches");
        let bound = source[..at]
            .rfind("let schema = ")
            .expect("every row binds the name above the statement");
        assert_eq!(
            the_head_is_an_argument_already_read(&source, at, bound),
            expected,
            "`{what}` decided differently from what this pin is written to decide:\n{source}"
        );
    }
}

/// Every literal head a role's variable can be built from, inside one function.
///
/// A `let` names its own `format!`; a `for` names the collection it walks, and
/// this file builds that collection out of `format!`s too. Two hops and no more:
/// a third shape is an empty answer and a red test, which is what
/// `every_role_this_suite_makes_is_one_the_sweep_reaches` asserts on.
fn the_literal_heads_of(body: &str, variable: &str) -> Vec<String> {
    let head_of = |built: &str| -> String {
        built
            .chars()
            .take_while(|c| *c != '{' && *c != '"')
            .collect()
    };
    let literal_heads = |from: usize, until: usize| -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = &body[from..until.min(body.len())];
        while let Some((_, after)) = rest.split_once("format!(\"") {
            found.push(head_of(after));
            rest = after;
        }
        found
    };
    if let Some(bound) = body.rfind(&format!("let {variable} = ")) {
        let ends = body[bound..]
            .find(";\n")
            .map_or(body.len(), |at| bound + at);
        return literal_heads(bound, ends);
    }
    // `for (named, how) in names.iter()` and `for named in names` alike: the
    // word after `in` is the collection, and its own binding is the second hop.
    let opens = body.rfind(&format!("for {variable} ")).or_else(|| {
        body.rfind(&format!("({variable}, "))
            .or_else(|| body.rfind(&format!(", {variable})")))
    });
    let Some(opens) = opens else {
        return Vec::new();
    };
    let Some(walked) = body[opens..]
        .split_once(" in ")
        .map(|(_, rest)| rest.trim_start())
        .and_then(|rest| {
            rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
        })
    else {
        return Vec::new();
    };
    let Some(bound) = body.rfind(&format!("let {walked} = ")) else {
        return Vec::new();
    };
    let ends = body[bound..]
        .find("];\n")
        .map_or(body.len(), |at| bound + at);
    literal_heads(bound, ends)
}

/// Every role this file names begins with a prefix the sweep reaches.
///
/// **The role half of this rule, beside the schema half.** A role sweep with
/// *a* prefix right is not enough: `nc_role_probe_<pid>`, a cluster-global
/// role this file creates, is outside `nc\_prune\_probe\_%`, so swept by that
/// prefix alone a red run would leave it for ever in a
/// cluster the developer shares. A prefix list is a list of spellings, and this
/// is the rule that makes it untrustworthy: the names are read out of this file
/// and each one is required to be swept.
#[test]
fn every_role_this_suite_makes_is_one_the_sweep_reaches() {
    let source = include_str!("in_postgres.rs");
    // The needle is assembled, because this test lives in the file it reads.
    let creates = concat!("CREATE ", "ROLE ");
    let swept: Vec<String> = THE_ROLES_THIS_SUITE_MAKES
        .iter()
        .map(|prefix| prefix.replace('\\', ""))
        .collect();
    let mut sites = 0_usize;
    for (at, _) in source.match_indices(creates) {
        let opens = source[..at].rfind('\n').map_or(0, |line| line + 1);
        if source[opens..at].trim_start().starts_with("//") {
            continue;
        }
        sites += 1;
        let tail = source[at + creates.len()..].trim_start_matches(['\\', '"']);
        // **Resolved inside the function it is written in, and no further.**
        // Taking the nearest `let <variable> = ` above and then the first
        // `format!("` from there to the end of the file resolves a `for`-loop
        // binding, which has no `let` near it at all, to a binding of the same
        // name in a different test further up, validating three roles it knows
        // nothing about — measured that way, renaming those three to genuinely
        // unswept names leaves the test green.
        //
        // The binding has to be in the same function, and a `for` names the
        // collection it walks, which is the second hop this file needs and the
        // last one it has.
        let opens = source[..at]
            .rfind("\nasync fn ")
            .or_else(|| source[..at].rfind("\nfn "))
            .unwrap_or(0);
        let heads: Vec<String> = if let Some(variable) = tail
            .strip_prefix('{')
            .and_then(|rest| rest.split('}').next())
        {
            the_literal_heads_of(&source[opens..at], variable)
        } else {
            vec![
                tail.chars()
                    .take_while(|c| *c != '{' && *c != '"' && *c != ' ' && *c != '\\')
                    .collect(),
            ]
        };
        assert!(
            !heads.is_empty(),
            "`{}` is a role name this rule cannot resolve inside the function that \
             creates it, so nothing here can say whether the sweep reaches it",
            tail.chars().take(40).collect::<String>()
        );
        for head in heads {
            assert!(
                !head.is_empty() && swept.iter().any(|prefix| head.starts_with(prefix)),
                "`{head}` is the head of a role name this suite creates and no prefix \
                 in `THE_ROLES_THIS_SUITE_MAKES` reaches it. Roles are cluster-global, \
                 a red run skips `finish`, and the name then stays for ever in a \
                 cluster the developer shares"
            );
        }
    }
    assert!(
        sites >= 3,
        "{sites} `{creates}` statement(s) were read out of this file, which is not \
         this file"
    );
}

/// Every schema this file names begins with a prefix the sweep reaches.
///
/// **The room the list above closes, closed.** A prefix list is a list of
/// spellings, and a spelling missing from it leaks a schema per red run into a
/// shared database without failing anything. So the list is not trusted: the
/// names are read out of this file and each one is required to be swept.
///
/// A name written in a test but not reachable from here is the same hole again,
/// which is why both spellings this file uses to name a schema are read.
#[test]
fn every_schema_this_suite_makes_is_one_the_sweep_reaches() {
    let source = include_str!("in_postgres.rs");
    let mut named: Vec<&str> = Vec::new();
    for opener in ["pool(\"", "const SCHEMA: &str = \""] {
        let mut from = 0_usize;
        while let Some(found) = source[from..].find(opener) {
            let at = from + found + opener.len();
            let Some(end) = source[at..].find('"') else {
                break;
            };
            named.push(&source[at..at + end]);
            from = at + end;
        }
    }
    named.sort_unstable();
    named.dedup();
    assert!(
        named.len() > 20,
        "{} schema name(s) were read out of this file, which is not this file",
        named.len()
    );
    let swept: Vec<String> = THE_SCHEMAS_THIS_SUITE_MAKES
        .iter()
        .map(|prefix| prefix.replace('\\', ""))
        .collect();
    let missed: Vec<&&str> = named
        .iter()
        .filter(|name| !swept.iter().any(|prefix| name.starts_with(prefix)))
        .collect();
    assert!(
        missed.is_empty(),
        "{missed:?} name a schema this suite creates and no prefix in \
         `THE_SCHEMAS_THIS_SUITE_MAKES` reaches it. A red run leaves it behind for \
         ever, in a database the developer shares"
    );

    // **And every `CREATE SCHEMA` this file writes.** The two openers above
    // read only the names written whole; two of this file's schemas are built
    // with `format!` and are invisible to them — measured live, both still
    // standing after a full green run. Each statement names a variable, and the
    // variable is resolved to the literal head of the `format!` that built it.
    // **A site this cannot resolve fails**, so a third shape is a red test
    // rather than a silent leak.
    //
    // The needle is assembled, because this test lives in the file it reads.
    let creates = concat!("CREATE ", "SCHEMA ");
    let mut sites = 0_usize;
    for (at, _) in source.match_indices(creates) {
        let opens = source[..at].rfind('\n').map_or(0, |line| line + 1);
        if source[opens..at].trim_start().starts_with("//") {
            continue;
        }
        sites += 1;
        let tail = source[at + creates.len()..].trim_start_matches(['\\', '"']);
        let variable = tail
            .strip_prefix('{')
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| {
                panic!(
                    "a `{creates}` in this file names something this rule cannot read: \
                     `{}`. Every schema this suite creates has to be reachable by a \
                     prefix in `THE_SCHEMAS_THIS_SUITE_MAKES`, and a statement nobody \
                     can resolve is a leak nobody can see",
                    &tail[..tail.len().min(40)]
                )
            });
        // **The binding above this statement, not the file's first.**
        // `source.split_once("let <variable> = ")` finds whichever binding
        // comes first in the file — and `pool`'s own
        // `let schema = &format!("{schema}_{}", …)` is at the top, with an
        // empty head that the skip below waves through as pool's shape. So any
        // later `CREATE SCHEMA {schema}` would resolve to that one whatever it
        // actually built: measured by taking a leaking plant the control
        // catches and renaming its variable to `schema`, nothing else, and the
        // rule goes quiet. A rule written to make a name list untrustworthy
        // cannot itself be keyed on a name.
        let binding = format!("let {variable} = ");
        let Some((bound, built)) = source[..at].rfind(&binding).and_then(|bound| {
            source[bound..]
                .split_once("format!(\"")
                .map(|(_, rest)| (bound, rest))
        }) else {
            panic!("`{variable}` is a schema name and no `format!` above this statement builds it")
        };
        let head: String = built
            .chars()
            .take_while(|c| *c != '{' && *c != '"')
            .collect();
        // An empty head is `pool`'s own `{schema}_{pid}`, whose prefixes are the
        // arguments the loop above already read.
        //
        // **And it has to be that one.** Testing only *this `format!` begins
        // with a placeholder*, which is true of any name built as
        // `format!("{…}…")` by any binding, turns the rule off for a whole
        // spelling. Measured with that test alone:
        // `let leaked = format!("{}_leaks", pid)` with a `CREATE SCHEMA {leaked}`
        // beside it passes, while the same leak written
        // `format!("leaks_{}", pid)` is caught. Pool's shape
        // interpolates *the name it is binding*; anything else with an empty
        // head is a leak this rule cannot see, and is a failure rather than a
        // skip.
        if head.is_empty() {
            // **Pool's binding, by where it is, not by what it looks like.** An
            // assertion holding a *shape* — the format starts with
            // `{<this variable>}_` — is satisfied exactly by any binding that
            // shadows itself:
            // `let zz = "zz_leak"; let zz = format!("{zz}_{}", pid);` passes
            // such an assertion, measured, while the identical leak written
            // head-first is caught. A rule written to make a name list
            // untrustworthy cannot be keyed on a name.
            //
            // There is one binding this skip is for, and what makes it that one
            // is not where it sits but **where its head comes from**: a
            // parameter of the function whose call sites the loop above already
            // read. Asking the position question with `pool`'s name and the
            // first `\n}\n` after it breaks in both directions at once — see
            // `the_head_is_an_argument_already_read`.
            assert!(
                the_head_is_an_argument_already_read(source, at, bound),
                "`{variable}` is a schema name built from a placeholder this rule cannot \
                 resolve, and its head is not an argument this test has already read — so \
                 no prefix in `THE_SCHEMAS_THIS_SUITE_MAKES` is known to reach it, and a \
                 red run leaves it behind for ever. Write the head of the name first, or \
                 give the sweep a prefix for it"
            );
            continue;
        }
        assert!(
            swept.iter().any(|prefix| head.starts_with(prefix)),
            "`{head}` is the head of a schema name this suite builds with `format!` \
             and no prefix in `THE_SCHEMAS_THIS_SUITE_MAKES` reaches it. A red run \
             leaves it behind for ever, in a database the developer shares"
        );
    }
    assert!(
        sites >= 3,
        "{sites} `{creates}` statement(s) were read out of this file, which is not \
         this file"
    );
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
        // **The trailing digits, not the last `_`-separated part.** One of this
        // file's own schemas is `Prune-Quoted <pid>`, where the pid is separated
        // by a space — `rsplit('_')` hands back the whole name, which parses as
        // no number, so a sweep reading it that way steps over it even once its
        // prefix is swept. What names a run is the number at the end, whatever stands in
        // front of it.
        let digits: String = schema
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        let pid = digits.as_str();
        // A separator in front of them, so a name that merely ends in a number
        // is not read as somebody's run.
        let separated = schema
            .strip_suffix(pid)
            .and_then(|head| head.chars().next_back())
            .is_some_and(|last| last == '_' || last == ' ' || last == '-');
        if pid.is_empty() || pid == mine || !separated {
            continue;
        }
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
