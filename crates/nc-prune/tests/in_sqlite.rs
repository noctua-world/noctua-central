//! The `DELETE` against the local database: which rows survive.
//!
//! The same question `crates/nc-prune/tests/in_postgres.rs` asks of the server,
//! asked of a SQLite file in a scratch directory — so it needs no service and
//! runs everywhere, CI included. **Which rows, not how many**: a pruner that
//! took the newest would report the same numbers.

use std::path::PathBuf;
use std::time::Duration;

use nc_catalog::{Database, Strategy, TableLimit};
use nc_prune::{Pruned, Target as _, Untouched, sqlite::Sqlite};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-prune-sqlite-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("a scratch directory");
        Self(root)
    }
}

/// A pool over a file in `scratch`, foreign keys on, as `nc-local`'s is.
async fn pool(scratch: &Scratch, connections: u32) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(scratch.0.join("prune.sqlite3"))
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_millis(200));
    SqlitePoolOptions::new()
        .max_connections(connections)
        .connect_with(options)
        .await
        .expect("a scratch database")
}

async fn run(pool: &SqlitePool, statement: &'static str) {
    sqlx::query(statement)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("{statement}: {error}"));
}

/// The limit under test: six rows, pruned four at a time, oldest by `at`.
const LIMIT: TableLimit = TableLimit {
    name: "rows_over_time",
    database: Database::LocalFile,
    strategy: Strategy::OldestFirst,
    order_by: "at",
    max_rows: Some(6),
    prune_batch: Some(4),
};

/// Ten rows whose `at` runs against their `id`, so the order the catalog names
/// and the order the table was written in disagree.
async fn fill(pool: &SqlitePool) {
    run(
        pool,
        "CREATE TABLE rows_over_time (id INTEGER PRIMARY KEY, at INTEGER NOT NULL)",
    )
    .await;
    for id in 1_i64..=10 {
        sqlx::query("INSERT INTO rows_over_time (id, at) VALUES (?1, ?2)")
            .bind(id)
            .bind((11 - id) * 1000)
            .execute(pool)
            .await
            .expect("a row");
    }
}

async fn surviving(pool: &SqlitePool, table: &'static str) -> Vec<i64> {
    let statement = match table {
        "rows_over_time" => "SELECT id FROM rows_over_time ORDER BY id",
        "parents" => "SELECT id FROM parents ORDER BY id",
        _ => "SELECT parent FROM children ORDER BY parent",
    };
    sqlx::query_scalar(statement)
        .fetch_all(pool)
        .await
        .expect("the rows read back")
}

async fn prune(pool: &SqlitePool, tables: &[TableLimit], dry: bool) -> Result<Vec<Pruned>, String> {
    let mut target = Sqlite::new(pool.clone());
    nc_prune::prune_these_declared_by(&mut target, tables, tables, dry).await
}

/// The oldest rows by the catalog's column go, and the newest stay.
#[tokio::test]
async fn the_oldest_rows_go_and_the_newest_stay() {
    let scratch = Scratch::new("oldest");
    let pool = pool(&scratch, 2).await;
    fill(&pool).await;
    assert_eq!(
        Sqlite::new(pool.clone()).count("rows_over_time").await,
        Ok(10)
    );

    let done = prune(&pool, &[LIMIT], false).await.expect("prunes");
    assert_eq!(
        done,
        vec![Pruned {
            table: "rows_over_time",
            before: Some(10),
            removed: 4,
            would_remove: 4,
            left_alone: None,
        }]
    );
    // `at` runs against `id`: the oldest four by `at` are the four highest ids.
    assert_eq!(
        surviving(&pool, "rows_over_time").await,
        (1..=6).collect::<Vec<_>>()
    );

    // At the limit, nothing more.
    let done = prune(&pool, &[LIMIT], false).await.expect("prunes");
    assert_eq!((done[0].before, done[0].removed), (Some(6), 0));
    assert_eq!(
        surviving(&pool, "rows_over_time").await,
        (1..=6).collect::<Vec<_>>()
    );
    pool.close().await;
}

/// A rehearsal issues the statement and takes nothing.
#[tokio::test]
async fn a_rehearsal_takes_nothing_and_says_what_it_would() {
    let scratch = Scratch::new("dry");
    let pool = pool(&scratch, 2).await;
    fill(&pool).await;
    let done = prune(&pool, &[LIMIT], true).await.expect("rehearses");
    assert_eq!((done[0].removed, done[0].would_remove), (0, 4));
    assert_eq!(surviving(&pool, "rows_over_time").await.len(), 10);
    pool.close().await;
}

/// A cascade the schema declares runs, and takes the children of the rows that
/// went and no others.
#[tokio::test]
async fn a_declared_cascade_takes_the_children_of_what_went() {
    let scratch = Scratch::new("cascade");
    let pool = pool(&scratch, 2).await;
    run(
        &pool,
        "CREATE TABLE parents (id INTEGER PRIMARY KEY, at INTEGER NOT NULL)",
    )
    .await;
    run(
        &pool,
        "CREATE TABLE children (parent INTEGER NOT NULL REFERENCES parents (id) ON DELETE CASCADE)",
    )
    .await;
    for id in 1_i64..=8 {
        sqlx::query("INSERT INTO parents (id, at) VALUES (?1, ?1)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("a parent");
        sqlx::query("INSERT INTO children (parent) VALUES (?1)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("a child");
    }
    let parents = TableLimit {
        name: "parents",
        order_by: "at",
        max_rows: Some(4),
        prune_batch: Some(4),
        ..LIMIT
    };
    let done = prune(&pool, &[parents], false).await.expect("prunes");
    assert_eq!(done[0].removed, 4, "the parents alone are counted");
    assert_eq!(surviving(&pool, "parents").await, vec![5, 6, 7, 8]);
    assert_eq!(surviving(&pool, "children").await, vec![5, 6, 7, 8]);
    pool.close().await;
}

/// A row another table holds by a reference `ON DELETE RESTRICT` is not the
/// pruner's to take: the oldest rows no reference holds go in its place, and
/// it goes once nothing holds it.
///
/// **A reviewer found a real bug with it**: the kernels of the realms one
/// person belongs to share the local file, and the prune of one took the
/// record of a unit another's stopped process left open, which that realm's
/// recovery reverts from — `nc-local`'s `store_unit_holds` holds it now, and
/// this is the selection that honours it.
#[tokio::test]
async fn a_row_another_table_holds_is_left_and_the_next_oldest_go() {
    let scratch = Scratch::new("held");
    let pool = pool(&scratch, 2).await;
    run(
        &pool,
        "CREATE TABLE parents (id INTEGER PRIMARY KEY, at INTEGER NOT NULL)",
    )
    .await;
    run(
        &pool,
        "CREATE TABLE holds (id INTEGER NOT NULL REFERENCES parents (id) ON DELETE RESTRICT, \
         holder INTEGER NOT NULL, PRIMARY KEY (id, holder)) WITHOUT ROWID",
    )
    .await;
    for id in 1_i64..=8 {
        sqlx::query("INSERT INTO parents (id, at) VALUES (?1, ?1)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("a parent");
    }
    run(
        &pool,
        "INSERT INTO holds (id, holder) VALUES (1, 9), (3, 9)",
    )
    .await;
    let parents = TableLimit {
        name: "parents",
        order_by: "at",
        max_rows: Some(4),
        prune_batch: Some(4),
        ..LIMIT
    };
    let done = prune(&pool, &[parents], false).await.expect("prunes");
    assert_eq!(done[0].removed, 4, "{done:?}");
    assert_eq!(
        surviving(&pool, "parents").await,
        vec![1, 3, 7, 8],
        "the held rows stay, and the oldest four nothing holds go"
    );
    run(&pool, "DELETE FROM holds").await;
    let done = prune(&pool, &[parents], false).await.expect("prunes");
    assert_eq!(done[0].removed, 0, "at the bound, nothing more: {done:?}");
    let parents = TableLimit {
        max_rows: Some(2),
        prune_batch: Some(2),
        ..parents
    };
    prune(&pool, &[parents], false).await.expect("prunes");
    assert_eq!(
        surviving(&pool, "parents").await,
        vec![7, 8],
        "once nothing holds them, the oldest go first"
    );
    pool.close().await;
}

/// Each way a `DELETE` could reach past its table is refused, and nothing goes.
///
/// **Two of these walked through**, measured. A virtual table was meant to be
/// refused and the test for it never matched, so an FTS5 table under a declared
/// name was counted and pruned. And `sqlite_schema` keeps a trigger's SQL with
/// its comments, so `AFTER DELETE/**/ON` and `AFTER DELETE --x` with `ON` on
/// the next line were not read as firing on `DELETE`: the prune ran through
/// the trigger, and a table nobody asked about lost every row.
#[tokio::test]
async fn a_table_a_delete_could_reach_past_is_refused() {
    let cases: [(&str, &[&'static str], &str); 8] = [
        (
            "set-null",
            &[
                "CREATE TABLE referrers (row INTEGER REFERENCES rows_over_time (id) ON DELETE SET NULL)",
            ],
            "SET NULL",
        ),
        (
            "trigger",
            &["CREATE TRIGGER elsewhere AFTER DELETE ON rows_over_time BEGIN SELECT 1; END"],
            "fires on DELETE",
        ),
        (
            "trigger-block-comment",
            &["CREATE TRIGGER elsewhere AFTER DELETE/**/ON rows_over_time BEGIN SELECT 1; END"],
            "fires on DELETE",
        ),
        (
            "trigger-line-comment",
            &["CREATE TRIGGER elsewhere AFTER DELETE --x\nON rows_over_time BEGIN SELECT 1; END"],
            "fires on DELETE",
        ),
        (
            "trigger-named-begin",
            &["CREATE TRIGGER begin AFTER DELETE ON rows_over_time BEGIN SELECT 1; END"],
            "fires on DELETE",
        ),
        (
            "view",
            &[
                "ALTER TABLE rows_over_time RENAME TO underneath",
                "CREATE VIEW rows_over_time AS SELECT id, at FROM underneath",
            ],
            "not an ordinary table",
        ),
        (
            "virtual",
            &[
                "DROP TABLE rows_over_time",
                "CREATE VIRTUAL TABLE rows_over_time USING fts5(id, at)",
            ],
            "not an ordinary table",
        ),
        ("missing", &["DROP TABLE rows_over_time"], "no such table"),
    ];
    for (name, statements, why) in cases {
        let scratch = Scratch::new(name);
        let pool = pool(&scratch, 2).await;
        fill(&pool).await;
        for statement in statements {
            run(&pool, statement).await;
        }
        let refused = prune(&pool, &[LIMIT], false).await.expect_err(name);
        assert!(refused.contains(why), "{name}: {refused}");
        assert!(
            refused.contains(nc_catalog::ErrorCode::DbPruneRefused.code()),
            "{name}: the refusal carries no code: {refused}"
        );
        pool.close().await;
    }
}

/// Parents pruned four at a time to four, whose children cascade from them.
const PARENTS: TableLimit = TableLimit {
    name: "parents",
    max_rows: Some(4),
    prune_batch: Some(4),
    ..LIMIT
};

/// Eight parents, a child of each that cascades from it, and three bystanders
/// no `DELETE` from `parents` has any business reaching.
async fn a_family(pool: &SqlitePool) {
    for statement in [
        "CREATE TABLE parents (id INTEGER PRIMARY KEY, at INTEGER NOT NULL)",
        "CREATE TABLE children (id INTEGER PRIMARY KEY, \
         parent INTEGER NOT NULL REFERENCES parents (id) ON DELETE CASCADE)",
        "CREATE TABLE bystanders (id INTEGER PRIMARY KEY)",
        "WITH RECURSIVE n(id) AS (SELECT 1 UNION ALL SELECT id + 1 FROM n WHERE id < 8) \
         INSERT INTO parents (id, at) SELECT id, id FROM n",
        "INSERT INTO children (id, parent) SELECT id, id FROM parents",
        "INSERT INTO bystanders (id) VALUES (1), (2), (3)",
    ] {
        run(pool, statement).await;
    }
}

/// How many parents, children and bystanders there are.
async fn the_family_counted(pool: &SqlitePool) -> (i64, i64, i64) {
    let mut counts = [0_i64; 3];
    for (count, statement) in counts.iter_mut().zip([
        "SELECT count(*) FROM parents",
        "SELECT count(*) FROM children",
        "SELECT count(*) FROM bystanders",
    ]) {
        *count = sqlx::query_scalar(statement)
            .fetch_one(pool)
            .await
            .expect("countable");
    }
    (counts[0], counts[1], counts[2])
}

/// A `DELETE` reaches every table a declared cascade takes rows from, and each
/// way past one of those is refused as it is on the table itself.
///
/// **Measured before the guard followed the cascade**: with `children`
/// cascading from `parents` and `AFTER DELETE ON children BEGIN DELETE FROM
/// bystanders; END`, the prune of `parents` returned `Ok` with `bystanders`
/// emptied — rows outside the table it counted, deleted without a refusal. A
/// temporary trigger on a child and a table referencing a child with
/// `ON DELETE SET NULL` walked past the same way.
#[tokio::test]
async fn a_table_a_cascade_reaches_past_is_refused() {
    let cases: [(&str, &[&'static str], &str); 4] = [
        (
            "child-trigger",
            &["CREATE TRIGGER reach AFTER DELETE ON children BEGIN DELETE FROM bystanders; END"],
            "a trigger on `children`",
        ),
        (
            "child-temporary",
            &["CREATE TEMP TRIGGER reach AFTER DELETE ON main.children \
               BEGIN DELETE FROM bystanders; END"],
            "watches `children`",
        ),
        (
            "child-referrer",
            &[
                "CREATE TABLE referrers (child INTEGER REFERENCES children (id) ON DELETE SET NULL)",
                "INSERT INTO referrers (child) SELECT id FROM children",
            ],
            "references `children`",
        ),
        // Two cascades down: the guard follows a cascade as far as it goes.
        (
            "grandchild-trigger",
            &[
                "CREATE TABLE grandchildren \
                 (child INTEGER NOT NULL REFERENCES children (id) ON DELETE CASCADE)",
                "INSERT INTO grandchildren (child) SELECT id FROM children",
                "CREATE TRIGGER reach AFTER DELETE ON grandchildren \
                 BEGIN DELETE FROM bystanders; END",
            ],
            "a trigger on `grandchildren`",
        ),
    ];
    for (name, statements, why) in cases {
        let scratch = Scratch::new(name);
        // One connection, so a temporary trigger is on the one the prune uses.
        let pool = pool(&scratch, 1).await;
        a_family(&pool).await;
        for statement in statements {
            run(&pool, statement).await;
        }
        let refused = prune(&pool, &[PARENTS], false).await.expect_err(name);
        assert!(refused.contains(why), "{name}: {refused}");
        assert!(
            refused.contains(nc_catalog::ErrorCode::DbPruneRefused.code()),
            "{name}: the refusal carries no code: {refused}"
        );
        assert_eq!(
            the_family_counted(&pool).await,
            (8, 8, 3),
            "{name}: a refused prune deleted something"
        );
        pool.close().await;
    }
    // And the family with nothing past its cascade is pruned, the children of
    // what went with it, and no bystander.
    let scratch = Scratch::new("family");
    let pool = pool(&scratch, 1).await;
    a_family(&pool).await;
    let done = prune(&pool, &[PARENTS], false).await.expect("prunes");
    assert_eq!(done[0].removed, 4);
    assert_eq!(the_family_counted(&pool).await, (4, 4, 3));
    pool.close().await;
}

/// A temporary object on the connection is refused, whatever it does.
#[tokio::test]
async fn a_temporary_object_on_the_connection_is_refused() {
    let scratch = Scratch::new("temporary");
    let pool = pool(&scratch, 1).await;
    fill(&pool).await;
    run(
        &pool,
        "CREATE TEMP TRIGGER keep BEFORE DELETE ON main.rows_over_time BEGIN SELECT RAISE(IGNORE); END",
    )
    .await;
    let refused = prune(&pool, &[LIMIT], false).await.expect_err("refused");
    assert!(refused.contains("temporary"), "{refused}");
    assert_eq!(surviving(&pool, "rows_over_time").await.len(), 10);
    pool.close().await;
}

/// A connection with foreign keys off would leave a cascade's children behind.
#[tokio::test]
async fn a_connection_without_foreign_keys_is_refused() {
    let scratch = Scratch::new("no-foreign-keys");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(scratch.0.join("prune.sqlite3"))
                .create_if_missing(true)
                .foreign_keys(false),
        )
        .await
        .expect("a scratch database");
    fill(&pool).await;
    let refused = prune(&pool, &[LIMIT], false).await.expect_err("refused");
    assert!(refused.contains("foreign keys are off"), "{refused}");
    pool.close().await;
}

/// Another writer holding the file is a skip, and the next tick tries again.
#[tokio::test]
async fn a_database_another_writer_holds_is_a_skip() {
    let scratch = Scratch::new("busy");
    let pool = pool(&scratch, 2).await;
    fill(&pool).await;
    let mut holder = pool.acquire().await.expect("a connection");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *holder)
        .await
        .expect("held for writing");
    let done = prune(&pool, &[LIMIT], false)
        .await
        .expect("a skip is not a failure");
    assert_eq!(done[0].left_alone, Some(Untouched::SomethingElseHeldALock));
    assert_eq!(done[0].code(), Some(nc_catalog::ErrorCode::DbPruneFailed));
    sqlx::query("ROLLBACK")
        .execute(&mut *holder)
        .await
        .expect("released");
    drop(holder);
    assert_eq!(surviving(&pool, "rows_over_time").await.len(), 10);
    pool.close().await;
}

/// A table of the server is refused by this target, and a local one by the
/// server's, before either is asked anything.
#[tokio::test]
async fn a_table_of_the_other_database_is_refused() {
    let scratch = Scratch::new("other-database");
    let pool = pool(&scratch, 2).await;
    fill(&pool).await;
    let servers = TableLimit {
        database: Database::Server,
        ..LIMIT
    };
    let refused = prune(&pool, &[servers], false).await.expect_err("refused");
    assert!(
        refused.contains("this target prunes the local SQLite file"),
        "{refused}"
    );
    assert!(
        refused.contains(nc_catalog::ErrorCode::DbTableNotBounded.code()),
        "{refused}"
    );
    assert_eq!(surviving(&pool, "rows_over_time").await.len(), 10);
    pool.close().await;
}

/// Over the real local database, a whole run considers the local tables alone:
/// it counts what it may prune and leaves user state to its writer.
#[tokio::test]
async fn a_run_over_the_local_database_reads_the_local_tables_alone() {
    let scratch = Scratch::new("local-store");
    let store = nc_local::LocalStore::open(&scratch.0.join("profile"))
        .await
        .expect("the local database opens");
    let mut target = Sqlite::new(store.pool().clone());
    assert_eq!(target.database(), Database::LocalFile);
    let done = nc_prune::run(&mut target, nc_prune::Request::every())
        .await
        .expect("a run");
    let local: Vec<&str> = nc_catalog::TABLES
        .iter()
        .filter(|table| table.database == Database::LocalFile)
        .map(|table| table.name)
        .collect();
    assert_eq!(
        done.iter().map(|pruned| pruned.table).collect::<Vec<_>>(),
        local,
        "a run over the local database considered a table it does not hold"
    );
    for pruned in &done {
        let declared = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == pruned.table)
            .expect("declared");
        match declared.strategy {
            Strategy::RefuseAtLimit => assert_eq!(
                pruned.left_alone,
                Some(Untouched::ItsWriterRefusesAtTheLimit),
                "{}",
                pruned.table
            ),
            _ => assert_eq!(
                (pruned.before, pruned.removed, &pruned.left_alone),
                (Some(0), 0, &None),
                "{}",
                pruned.table
            ),
        }
    }
    // And the server's own tables, handed to this target by name, are refused.
    let refused = nc_prune::run(&mut target, nc_prune::Request::just("access_records"))
        .await
        .expect_err("a server table");
    assert!(
        refused.contains("this target prunes the local SQLite file"),
        "{refused}"
    );
    store.close().await;
}
