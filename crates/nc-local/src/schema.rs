//! What the database must hold, and the check that it does.
//!
//! [`EXPECTED`] is every object `local-migrations/` builds, with the BLAKE3 of
//! the SQL `sqlite_schema` stores for it and the migration that builds it. The
//! store holds an open database to it every time it is opened — before the
//! migrator runs, to the objects of the migrations the file already records
//! ([`judge_at`]), and after, to the whole list ([`judge`]) — and
//! `cargo xtask check` holds a scratch database built from the files on disk
//! to it on every run, one migration at a time — so a migration changed
//! without this list, or this list changed without a migration, fails the gate
//! before it can make the kernel refuse its own file.
//!
//! **Nothing the store refuses at open has been written to.** Every check that
//! reads what a file holds runs before the migrator does, on a connection
//! opened read-only, so a file refused by one is left as it was — byte for
//! byte, its journal with it; the `-shm` SQLite builds to read a `-wal` that
//! had none is all it may leave beside it — and the check after it holds the
//! migrations' own work to the list, which the gate holds them to first.
//! `NC-DB-0007`'s message tells the user nothing was changed, and this order
//! is what keeps that true.
//!
//! **A digest and not the text**, because the text is a statement that builds a
//! table, and the source gate refuses one written anywhere in this crate's
//! sources but `local-migrations/`. The digest says as much — a trigger whose
//! body lost a word, a column that lost its `CHECK` — and a mismatch names the
//! object, so the file to read is the migration.
//!
//! **Why every object and not only the triggers.** A dropped trigger is the
//! attack the append-only images are about, and an added one is the same
//! attack from the other side: a `BEFORE INSERT … RAISE(IGNORE)` on a table
//! makes every write to it succeed having written nothing. A table whose
//! `CHECK` was dropped, or a new index nobody wrote, is the database no longer
//! being the one this build was tested against. So the comparison is of the
//! whole set, both ways.

use sqlx::Row as _;
use sqlx::SqliteConnection;

use crate::LocalError;

/// `'NCL1'` read as a big-endian integer: this platform's local database.
///
/// Written into the file's header by `local-migrations/0001_plugin_store.sql`,
/// in the transaction that builds the tables.
pub const APPLICATION_ID: i32 = 0x4E43_4C31;

/// The oldest SQLite this crate accepts.
///
/// **3.50.2 fixes CVE-2025-6965**, and `libsqlite3-sys` 0.30 — which `sqlx`
/// 0.9 still accepts — bundles 3.46.0. What chooses the bundled SQLite is
/// `Cargo.lock`, not a manifest (`cargo tree -i libsqlite3-sys` says which), so
/// an old lock would link an old SQLite with every manifest unchanged; this is
/// what refuses it at run time, and `cargo xtask check` refuses it at the gate.
pub const MINIMUM_SQLITE: (u32, u32, u32) = (3, 50, 2);

/// One object `sqlite_schema` must hold.
///
/// Borrowed rather than `'static`, so a test can hold a list of its own to the
/// same comparison without manufacturing a `'static` string at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object<'a> {
    /// `table`, `index` or `trigger`.
    pub kind: &'a str,
    /// Its name.
    pub name: &'a str,
    /// The table it belongs to.
    pub table: &'a str,
    /// The BLAKE3 of the SQL `sqlite_schema` stores for it, in lowercase hex —
    /// or `None` for the migrator's own bookkeeping, whose statement is
    /// `sqlx`'s and changes with it.
    pub sql: Option<&'a str>,
    /// The version of the migration that builds it — or `0` for the
    /// migrator's own bookkeeping, which a file holds once anything has
    /// migrated it.
    ///
    /// **So a file can be judged before it is migrated**, against what its
    /// own history built ([`judge_at`]). A later migration that changed an
    /// object an earlier one built would need the object listed once per
    /// shape; none does yet, and `cargo xtask check` builds the schema one
    /// migration at a time and judges each step, which is what says when one
    /// does.
    pub since: i64,
}

/// Every object the migrations build, and nothing else.
pub const EXPECTED: &[Object<'static>] = &[
    Object {
        kind: "table",
        name: "_sqlx_migrations",
        table: "_sqlx_migrations",
        sql: None,
        since: 0,
    },
    Object {
        kind: "index",
        name: "sqlite_autoindex__sqlx_migrations_1",
        table: "_sqlx_migrations",
        sql: None,
        since: 0,
    },
    Object {
        kind: "table",
        name: "blobs",
        table: "blobs",
        sql: Some("cb281f4ae96f52518633baec32bed910571c3d0af622af71eeed8528f1390b47"),
        since: 2,
    },
    Object {
        kind: "table",
        name: "local_journal_entries",
        table: "local_journal_entries",
        sql: Some("ffd488bbca7d9e205806b7083dcd265a0a23ec4091d25f31b0f89d0f969eee27"),
        since: 4,
    },
    Object {
        kind: "index",
        name: "local_journal_entries_by_unit",
        table: "local_journal_entries",
        sql: Some("1c590a75ac9d5bbcb59ba29af19f9e0530d5cd093a6326cb84efa8fd09cbe0fa"),
        since: 4,
    },
    Object {
        kind: "trigger",
        name: "local_journal_entries_are_never_deleted",
        table: "local_journal_entries",
        sql: Some("3132351f76f8faa577b1747fb87939d5dd6689549afb82938c873de70f91e5a7"),
        since: 4,
    },
    Object {
        kind: "trigger",
        name: "local_journal_entries_are_never_edited",
        table: "local_journal_entries",
        sql: Some("861d2cdb1648b7b967a86f47dc1e227a237268298b8c64c65f571fc4cf0fe0b7"),
        since: 4,
    },
    Object {
        kind: "trigger",
        name: "local_journal_entries_sequence_is_the_next_one",
        table: "local_journal_entries",
        sql: Some("a60c4cccb73559904562de995a89354e529ef167c48b66456c807413d9963ffd"),
        since: 4,
    },
    Object {
        kind: "table",
        name: "store_entries",
        table: "store_entries",
        sql: Some("d6d2d6a07dad0ab95b33750d9059549d06fd1a91e9d9769bec41df3ca0c9469c"),
        since: 1,
    },
    Object {
        kind: "table",
        name: "store_notes",
        table: "store_notes",
        sql: Some("c011d95dfadf9bc9b906bf264ed5f08a5b2503fd9e91e9d8fddeb264fa411f90"),
        since: 1,
    },
    Object {
        kind: "trigger",
        name: "store_notes_are_never_edited",
        table: "store_notes",
        sql: Some("173228f41732012c7243c3bcae143b8024ac2c259a98b7359ec799a70341a0a9"),
        since: 1,
    },
    Object {
        kind: "table",
        name: "store_units",
        table: "store_units",
        sql: Some("cc724652bd1116a7682515d182baf7995ade466c2a5221b3840a86add8367d29"),
        since: 1,
    },
    Object {
        kind: "index",
        name: "store_units_by_principal",
        table: "store_units",
        sql: Some("5f3325aff0e12b973c72b258f8c38ad5ed5553d48d1f8c230bfe328c9debf8b0"),
        since: 1,
    },
    Object {
        kind: "table",
        name: "store_unit_holds",
        table: "store_unit_holds",
        sql: Some("d220e56bf42ba73748358d6be79ec69146ab567bb1e35f943af45154fe7f22a1"),
        since: 3,
    },
    Object {
        kind: "table",
        name: "store_writes",
        table: "store_writes",
        sql: Some("5955a44f2819cc8673ff1f6d3a522e9e994012aabf8638d2a732093df6f74c8f"),
        since: 1,
    },
    Object {
        kind: "trigger",
        name: "store_writes_are_never_edited",
        table: "store_writes",
        sql: Some("2b5cd7dda8a0e5639d7ce0069dd7d19f14d1430f21c52289a1bb86c84857e929"),
        since: 1,
    },
];

/// One row of `sqlite_schema`, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// `type`.
    pub kind: String,
    /// `name`.
    pub name: String,
    /// `tbl_name`.
    pub table: String,
    /// `sql`, which SQLite leaves NULL for an index it made itself.
    pub sql: Option<String>,
}

/// The BLAKE3 of one statement, as [`Object::sql`] holds it.
#[must_use]
pub fn digest(sql: &str) -> String {
    blake3::hash(sql.as_bytes()).to_hex().to_string()
}

/// Every object in `main.sqlite_schema`, in name order.
///
/// # Errors
///
/// Whatever SQLite answered.
pub async fn found_in(connection: &mut SqliteConnection) -> Result<Vec<Found>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT type, name, tbl_name, sql FROM main.sqlite_schema ORDER BY name, type")
            .fetch_all(&mut *connection)
            .await?;
    let mut found = Vec::with_capacity(rows.len());
    for row in rows {
        found.push(Found {
            kind: row.try_get("type")?,
            name: row.try_get("name")?,
            table: row.try_get("tbl_name")?,
            sql: row.try_get("sql")?,
        });
    }
    Ok(found)
}

/// Whether what a database holds is [`EXPECTED`], exactly.
///
/// # Errors
///
/// Every object that is missing, added or different, one per clause, each
/// naming the object and what differs.
pub fn judge(found: &[Found]) -> Result<(), String> {
    judge_against(EXPECTED, found)
}

/// Whether what a database holds is what its own history built: the objects
/// of the migrations it records, the migrator's own beside them, and nothing
/// else.
///
/// `applied` is the versions `_sqlx_migrations` records, or `None` when the
/// file holds no such table — one nothing has migrated, which holds nothing.
///
/// # Errors
///
/// As [`judge`].
pub fn judge_at(found: &[Found], applied: Option<&[i64]>) -> Result<(), String> {
    let expected: Vec<Object<'static>> = EXPECTED
        .iter()
        .filter(|object| {
            applied.is_some_and(|versions| object.since == 0 || versions.contains(&object.since))
        })
        .copied()
        .collect();
    judge_against(&expected, found)
}

/// The same comparison against any list, so its clauses can be driven with
/// objects whose SQL a test knows.
///
/// **By kind and name, never by name alone.** SQLite keeps triggers in a
/// namespace of their own, so a trigger may bear the name of a table or an
/// index, and a view the name of a trigger. Matched by name, a trigger planted
/// under `blobs` that rewrites every value committed, and another under
/// `store_units_by_principal`, would each find the listed object and pass for
/// it, and the file would open.
fn judge_against(expected: &[Object<'_>], found: &[Found]) -> Result<(), String> {
    let mut wrong: Vec<String> = Vec::new();
    for object in expected {
        let same = |held: &&Found| held.kind == object.kind && held.name == object.name;
        match found.iter().find(same) {
            None => match found.iter().find(|held| held.name == object.name) {
                Some(held) => wrong.push(format!(
                    "`{}` is a {} on `{}`, and this build expects a {} on `{}`",
                    object.name, held.kind, held.table, object.kind, object.table
                )),
                None => wrong.push(format!("the {} `{}` is missing", object.kind, object.name)),
            },
            Some(held) => {
                if held.table != object.table {
                    wrong.push(format!(
                        "`{}` is a {} on `{}`, and this build expects a {} on `{}`",
                        object.name, held.kind, held.table, object.kind, object.table
                    ));
                }
                if let Some(digest_expected) = object.sql {
                    let digest_found = held.sql.as_deref().map(digest);
                    if digest_found.as_deref() != Some(digest_expected) {
                        wrong.push(format!(
                            "the {} `{}` is not the one `local-migrations/` builds: its SQL \
                             hashes to {} and this build expects {digest_expected}",
                            object.kind,
                            object.name,
                            digest_found.as_deref().unwrap_or("nothing")
                        ));
                    }
                }
            }
        }
    }
    for held in found {
        if !expected
            .iter()
            .any(|object| object.kind == held.kind && object.name == held.name)
        {
            wrong.push(format!(
                "the {} `{}` on `{}` is not one `local-migrations/` builds",
                held.kind, held.name, held.table
            ));
        }
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(wrong.join("; "))
    }
}

/// Whether a SQLite version string is at least `floor`.
///
/// `3.51.3` is `(3, 51, 3)`; anything that does not read as three numbers is
/// not a version this crate can vouch for, and is refused.
#[must_use]
pub fn at_least(version: &str, floor: (u32, u32, u32)) -> bool {
    let parts: Vec<Option<u32>> = version.split('.').map(|part| part.parse().ok()).collect();
    match parts.as_slice() {
        [Some(major), Some(minor), Some(patch)] => (*major, *minor, *patch) >= floor,
        _ => false,
    }
}

/// The self-check before the migrations run: everything in it only reads.
///
/// In this order, each refusing on its own: the SQLite this was linked with is
/// at least `floor`; `quick_check` says `ok`; no temporary object exists on the
/// connection; the history `_sqlx_migrations` records holds no migration that
/// did not run to its end and none this build does not carry, is a beginning
/// of `carried` — the versions this build applies, in their order, each with
/// the checksum `sqlx` keeps for it — and records each of them as the SQL this
/// build carries; and `sqlite_schema` holds what that history built and
/// nothing else ([`judge_at`]).
///
/// **Before, because the migrator writes.** Run first, it would write to a
/// file the self-check then refused — measured: a file one migration behind
/// with a trigger dropped would have `0002_blobs.sql` applied and then come
/// back `NC-DB-0007`, whose message to the user ends "Nothing was changed"; and
/// one whose index `quick_check` calls damaged would have a `CREATE TABLE`
/// written into it first. A write into a damaged file is how a damaged page
/// becomes a lost one.
///
/// **Everything the migrator would refuse, refused here first**: a version
/// this build does not carry — a newer build's database — a version recorded
/// as other SQL than this build carries for it, and one that did not run to
/// its end, still told as the migration that failed (`NC-DB-0002`). Left to the
/// migrator, the file would be written to before it refused: the pool that
/// runs it sets `journal_mode = WAL` as it connects, which would convert a file
/// in rollback mode under a message saying nothing was changed. The migrator's
/// refusal stays the second net;
/// `a_newer_builds_file_is_refused_by_its_version_and_left_as_it_was` and
/// `a_migration_edited_after_it_ran_is_refused` hold a file in rollback mode
/// to its bytes.
///
/// **The floor is handed in so its refusal can be driven**: the SQLite linked
/// is the lockfile's, and no test can link an older one. Measured, with the
/// floor fixed inside, the comparison switched off —
/// `if false && !at_least(…)` — would leave every target of
/// `cargo test -p nc-local` passing, and the one sentence that names
/// CVE-2025-6965 to a user a branch nothing entered. Handed in, a floor above
/// every SQLite there is enters it
/// (`a_sqlite_below_the_floor_is_refused_naming_both`).
///
/// # Errors
///
/// [`LocalError::Unexpected`] for each check that fails, naming it, but
/// [`LocalError::Migration`] for a migration the file records as not run to
/// its end; whatever SQLite answered when it could not be asked.
pub(crate) async fn before_migrating(
    connection: &mut SqliteConnection,
    floor: (u32, u32, u32),
    carried: &[(i64, Vec<u8>)],
) -> Result<(), LocalError> {
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| crate::said("reading the SQLite version", &error))?;
    if !at_least(&version, floor) {
        let (major, minor, patch) = floor;
        return Err(LocalError::Unexpected(format!(
            "this build links SQLite {version}, and the local database is opened only by \
             {major}.{minor}.{patch} or later — the release that fixes CVE-2025-6965. The \
             lockfile decides which: `cargo update -p libsqlite3-sys`"
        )));
    }
    let verdicts: Vec<String> = sqlx::query_scalar("PRAGMA main.quick_check")
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| crate::said("checking the file's structure", &error))?;
    if verdicts != ["ok"] {
        return Err(LocalError::Unexpected(format!(
            "SQLite's `quick_check` found the file damaged: {}",
            verdicts.join("; ")
        )));
    }
    no_temporary_object(connection).await?;
    let found = found_in(connection)
        .await
        .map_err(|error| crate::said("reading the schema", &error))?;
    let history = history_of(connection, &found).await?;
    let applied: Option<Vec<i64>> = history
        .as_ref()
        .map(|recorded| recorded.iter().map(|row| row.version).collect());
    if let (Some(recorded), Some(applied)) = (&history, &applied) {
        written_by_this_build(recorded, applied, carried)?;
    }
    judge_at(&found, applied.as_deref()).map_err(LocalError::Unexpected)
}

/// Refuses a history the migrator would refuse: a migration that did not run
/// to its end, which is what is left of one that failed (`NC-DB-0002`), and a
/// history this build did not write (`NC-DB-0007`) — one this build does not
/// carry, an order this build does not apply them in, and one recorded as
/// other SQL than this build carries for it.
fn written_by_this_build(
    recorded: &[Recorded],
    applied: &[i64],
    carried: &[(i64, Vec<u8>)],
) -> Result<(), LocalError> {
    let refused = |why: String| Err(LocalError::Unexpected(why));
    if let Some(row) = recorded.iter().find(|row| !row.success) {
        return Err(LocalError::Migration(format!(
            "applying the local migrations: migration {} is partially applied — the file \
             records it as one that did not run to its end — so nothing after it is applied",
            row.version
        )));
    }
    let this_builds = |version: i64| carried.iter().find(|(held, _)| *held == version);
    if let Some(row) = recorded
        .iter()
        .find(|row| this_builds(row.version).is_none())
    {
        return refused(format!(
            "the file records migration {}, which this build does not carry: it is a newer \
             build's database, or one another migrator wrote",
            row.version
        ));
    }
    let versions: Vec<i64> = carried.iter().map(|(version, _)| *version).collect();
    if !versions.starts_with(applied) {
        return refused(format!(
            "the file records the migrations {applied:?}, and this build applies \
             {versions:?} in that order and in no other, so this is a history it did not \
             write"
        ));
    }
    if let Some(row) = recorded
        .iter()
        .find(|row| this_builds(row.version).is_some_and(|(_, checksum)| *checksum != row.checksum))
    {
        return refused(format!(
            "the file records migration {} as other SQL than this build carries for it: a \
             migration edited after it ran",
            row.version
        ));
    }
    Ok(())
}

/// One row of `_sqlx_migrations`: the version, whether it ran to its end, and
/// the checksum of the SQL that ran.
#[derive(Debug)]
struct Recorded {
    /// The migration's number.
    version: i64,
    /// Whether it ran to its end.
    success: bool,
    /// The SHA-384 of the SQL that ran, as `sqlx` computed it.
    checksum: Vec<u8>,
}

/// What `_sqlx_migrations` records, in order of version — or `None` when the
/// file holds no such table, because nothing has migrated it yet.
async fn history_of(
    connection: &mut SqliteConnection,
    found: &[Found],
) -> Result<Option<Vec<Recorded>>, LocalError> {
    if !found
        .iter()
        .any(|held| held.kind == "table" && held.name == "_sqlx_migrations")
    {
        return Ok(None);
    }
    let read = |error: sqlx::Error| crate::said("reading the migrations' history", &error);
    let rows = sqlx::query(
        "SELECT version, success, checksum FROM main._sqlx_migrations ORDER BY version",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(read)?;
    rows.iter()
        .map(|row| {
            Ok(Recorded {
                version: row.try_get("version").map_err(read)?,
                success: row.try_get("success").map_err(read)?,
                checksum: row.try_get("checksum").map_err(read)?,
            })
        })
        .collect::<Result<Vec<_>, LocalError>>()
        .map(Some)
}

/// The self-check after the migrations ran: the database behind this
/// connection is the one this build expects, or the reason it is not.
///
/// In this order, each refusing on its own: the header carries
/// [`APPLICATION_ID`]; no temporary object exists on the connection; and
/// `sqlite_schema` holds [`EXPECTED`] and nothing else.
///
/// # Errors
///
/// [`LocalError::Unexpected`] for each check that fails, naming it; whatever
/// SQLite answered when it could not be asked.
pub(crate) async fn after_migrating(connection: &mut SqliteConnection) -> Result<(), LocalError> {
    let application: i32 = sqlx::query_scalar("PRAGMA main.application_id")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| crate::said("reading the file's header", &error))?;
    if application != APPLICATION_ID {
        return Err(LocalError::Unexpected(format!(
            "the file's header carries application {application}, and this platform's is \
             {APPLICATION_ID}"
        )));
    }
    no_temporary_object(connection).await?;
    let found = found_in(connection)
        .await
        .map_err(|error| crate::said("reading the schema", &error))?;
    judge(&found).map_err(LocalError::Unexpected)
}

/// No temporary object shadows or watches a `main` table on this connection.
///
/// **Asked of the connection that is about to write**, because a temporary
/// object belongs to the one connection that made it and no other can see it.
///
/// # Errors
///
/// [`LocalError::Unexpected`] naming how many there are.
pub(crate) async fn no_temporary_object(
    connection: &mut SqliteConnection,
) -> Result<(), LocalError> {
    let temporary: i64 = sqlx::query_scalar("SELECT count(*) FROM temp.sqlite_schema")
        .fetch_one(&mut *connection)
        .await
        .map_err(|error| crate::said("reading the temporary schema", &error))?;
    if temporary == 0 {
        Ok(())
    } else {
        Err(LocalError::Unexpected(format!(
            "{temporary} temporary object(s) exist on this connection, and a temporary \
             trigger on a `main` table can turn a write into one that reports success \
             having written nothing"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{Found, Object, at_least, digest, judge_against};

    /// A list of three objects whose SQL this test knows.
    const LIST: [Object<'static>; 3] = [
        Object {
            kind: "table",
            name: "t",
            table: "t",
            sql: None,
            since: 1,
        },
        Object {
            kind: "trigger",
            name: "t_is_never_edited",
            table: "t",
            sql: None,
            since: 1,
        },
        Object {
            kind: "table",
            name: "_sqlx_migrations",
            table: "_sqlx_migrations",
            sql: None,
            since: 1,
        },
    ];

    fn row(kind: &str, name: &str, table: &str, sql: Option<&str>) -> Found {
        Found {
            kind: kind.to_owned(),
            name: name.to_owned(),
            table: table.to_owned(),
            sql: sql.map(str::to_owned),
        }
    }

    /// Every way a set of objects can differ from the list is its own refusal.
    #[test]
    fn a_schema_is_refused_for_each_way_it_differs_from_the_list() {
        // Any text will do for SQL here — only its digest is compared — and
        // none of it builds a table, so the source gate over this crate has
        // nothing to refuse in it.
        let table_sql = "the SQL of table t";
        let trigger_sql = "the SQL of the trigger on t, SELECT 1";
        let digests = [digest(table_sql), digest(trigger_sql)];
        let mut list: [Object<'_>; 3] = LIST;
        list[0].sql = Some(digests[0].as_str());
        list[1].sql = Some(digests[1].as_str());
        let held = vec![
            row("table", "t", "t", Some(table_sql)),
            row("trigger", "t_is_never_edited", "t", Some(trigger_sql)),
            row(
                "table",
                "_sqlx_migrations",
                "_sqlx_migrations",
                Some("whatever sqlx wrote"),
            ),
        ];
        assert_eq!(
            judge_against(&list, &held),
            Ok(()),
            "the list itself is accepted"
        );

        // Missing.
        let refused = judge_against(
            &list,
            &held[..1]
                .iter()
                .chain(&held[2..])
                .cloned()
                .collect::<Vec<_>>(),
        )
        .expect_err("a dropped trigger is refused");
        assert!(
            refused.contains("the trigger `t_is_never_edited` is missing"),
            "{refused}"
        );

        // Added.
        let mut added = held.clone();
        added.push(row(
            "trigger",
            "silence",
            "t",
            Some("the SQL of a trigger nobody listed"),
        ));
        let refused = judge_against(&list, &added).expect_err("an added trigger is refused");
        assert!(
            refused.contains("the trigger `silence` on `t` is not one"),
            "{refused}"
        );

        // Edited: one word of the trigger's body.
        let mut edited = held.clone();
        edited[1].sql = Some(trigger_sql.replace("SELECT 1", "SELECT 2"));
        let refused = judge_against(&list, &edited).expect_err("an edited trigger is refused");
        assert!(refused.contains("its SQL hashes to"), "{refused}");

        // An expected statement with no SQL at all.
        let mut blank = held.clone();
        blank[0].sql = None;
        let refused = judge_against(&list, &blank).expect_err("a table with no SQL is refused");
        assert!(refused.contains("hashes to nothing"), "{refused}");

        // A different kind, or a different table, under an expected name.
        let mut kind = held.clone();
        kind[1].kind = "index".to_owned();
        let refused = judge_against(&list, &kind).expect_err("a different kind is refused");
        assert!(refused.contains("is a index on `t`"), "{refused}");
        let mut table = held.clone();
        table[1].table = "u".to_owned();
        let refused = judge_against(&list, &table).expect_err("a different table is refused");
        assert!(refused.contains("is a trigger on `u`"), "{refused}");

        // A namesake: a trigger under a listed table's name, and a view under
        // a listed trigger's, each an object no migration builds.
        for (kind, name) in [("trigger", "t"), ("view", "t_is_never_edited")] {
            let mut namesake = held.clone();
            namesake.push(row(kind, name, "t", Some("the SQL of a namesake")));
            let refused = judge_against(&list, &namesake).expect_err("a namesake is refused");
            assert!(
                refused.contains(&format!("the {kind} `{name}` on `t` is not one")),
                "{kind} {name}: {refused}"
            );
        }

        // The migrator's own objects are held by name, kind and table only.
        let mut sqlx_changed = held;
        sqlx_changed[2].sql = Some("whatever another sqlx writes".to_owned());
        assert_eq!(judge_against(&list, &sqlx_changed), Ok(()));
    }

    /// The digest is BLAKE3's, lowercase hex, sixty-four characters.
    #[test]
    fn a_digest_is_blake3_in_lowercase_hex() {
        let written = digest("the SQL of table t");
        assert_eq!(written.len(), 64);
        assert!(
            written
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
        assert_eq!(
            written,
            blake3::hash(b"the SQL of table t").to_hex().to_string()
        );
        assert_ne!(written, digest("the SQL of table u"));
    }

    /// The version floor is a floor, and a text that is not a version is below it.
    #[test]
    fn a_sqlite_older_than_the_floor_is_refused() {
        let floor = (3, 50, 2);
        assert!(!at_least("3.50.1", floor));
        assert!(!at_least("3.46.0", floor));
        assert!(!at_least("2.99.99", floor));
        assert!(at_least("3.50.2", floor));
        assert!(at_least("3.51.0", floor));
        assert!(at_least("4.0.0", floor));
        assert!(!at_least("3.51", floor), "two parts is not a version");
        assert!(!at_least("3.51.x", floor));
        assert!(!at_least("", floor));
    }

    /// The check after the migrations holds the file to the whole list.
    ///
    /// **No file reaches it from outside**: what a file holds is judged before
    /// the migrator runs, so one that differs from the list is refused there
    /// first — measured, with this judgement ignored, every other test of
    /// `cargo test -p nc-local` passes. What it alone sees is the migrator's own
    /// work, migrations that build what the list does not say. Driven here with
    /// the header stamped and nothing built, which differs from the list in
    /// every object.
    #[tokio::test]
    async fn the_check_after_the_migrations_holds_the_file_to_the_whole_list() {
        use sqlx::{ConnectOptions as _, Connection as _};
        let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
            .in_memory(true)
            .connect()
            .await
            .expect("a database in memory");
        assert_eq!(super::APPLICATION_ID, 1_313_033_265);
        sqlx::query("PRAGMA main.application_id = 1313033265")
            .execute(&mut connection)
            .await
            .expect("the header is written");
        let refused = super::after_migrating(&mut connection)
            .await
            .expect_err("a file that holds none of the list");
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::DbLocalSchemaUnexpected,
            "{refused}"
        );
        assert!(
            refused
                .to_string()
                .contains("the table `store_entries` is missing"),
            "{refused}"
        );
        connection.close().await.expect("closes");
    }

    /// The self-check refuses a SQLite below its floor before it reads
    /// anything else, naming the version linked and the advisory.
    ///
    /// **On a real connection**, whose SQLite is the one this build links: a
    /// floor above every release there is is the only way to stand below one.
    /// The control is the floor this crate ships, which the same connection
    /// passes — a database that holds nothing is what no history built — and
    /// the check after the migrations then fails at the header, saying nothing
    /// about a version.
    #[tokio::test]
    async fn a_sqlite_below_the_floor_is_refused_naming_both() {
        use sqlx::{ConnectOptions as _, Connection as _};
        let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
            .in_memory(true)
            .connect()
            .await
            .expect("a database in memory");
        let linked: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&mut connection)
            .await
            .expect("readable");

        let carried = [(1, Vec::new()), (2, Vec::new())];
        let refused = super::before_migrating(&mut connection, (99, 0, 0), &carried)
            .await
            .expect_err("no SQLite is 99.0.0");
        assert_eq!(
            refused.code(),
            nc_catalog::ErrorCode::DbLocalSchemaUnexpected,
            "{refused}"
        );
        let said = refused.to_string();
        assert!(
            said.contains(&format!("links SQLite {linked}"))
                && said.contains("99.0.0 or later")
                && said.contains("CVE-2025-6965"),
            "{said}"
        );

        super::before_migrating(&mut connection, super::MINIMUM_SQLITE, &carried)
            .await
            .expect("an empty database is what no history built");
        let past_the_floor = super::after_migrating(&mut connection)
            .await
            .expect_err("an empty database carries no header");
        assert!(
            past_the_floor.to_string().contains("carries application 0"),
            "{past_the_floor}"
        );
        assert!(
            !past_the_floor.to_string().contains("CVE-2025-6965"),
            "{past_the_floor}"
        );
        connection.close().await.expect("closes");
    }
}
