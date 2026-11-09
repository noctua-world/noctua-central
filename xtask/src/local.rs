//! The gates over the local database: what `local-migrations/` builds, and what
//! the crates that open the file may write.
//!
//! **The schema, built from the files on disk.** `crates/nc-local` embeds the
//! migrations when it is compiled and compares every database it opens with
//! `nc_local::schema::EXPECTED`. That comparison runs on a user's machine, after
//! the release — so a migration edited without the list, or the list without
//! the migration, is a kernel that refuses its own file on first start. This
//! applies the directory as it stands to a scratch file, one migration at a
//! time, and holds each step and the result to the same list, with the same
//! functions, on every run: every object, the migration that builds it, and
//! the BLAKE3 of its SQL, the trigger bodies included; `integrity_check`, which is
//! the whole of what `quick_check` samples; `foreign_key_check`; the header's
//! `application_id`; and the SQLite this build links, against the floor that
//! fixes CVE-2025-6965. No service is needed, so it runs everywhere, CI and
//! `--offline` included.
//!
//! **The SQL, in the crates that open the file.** Every statement there names
//! `main.` and none of them builds or changes a table: DDL lives in
//! `local-migrations/` and nowhere else. The source gate refuses, in every
//! source [`WRITERS`] names — which the members' manifests decide, both ways —
//! a statement that attaches another database, edits `sqlite_schema` through
//! `writable_schema`, undoes a pragma of the connection's hardening — each
//! value judged as SQLite reads it, the pragma door read as the statement
//! `sqlx` sends, and the list held to what `crates/nc-local` sets
//! ([`HARDENED`]) — sets a pragma through the door under a key no literal
//! reads as one pragma, copies the file out with `VACUUM` or points SQLite's
//! files at a directory of its own, loads an extension, gathers the planner's
//! statistics, or creates, drops or alters a table, an index, a trigger or a
//! view. Rust's comments are not read; strings are, because a
//! statement is a string, and each is read as the compiler hands it on — a
//! line continued with a `\` joined, `\n` a newline — with the SQL comments in
//! it read as SQLite reads them: as gaps between the words around them. A
//! crate's tests are outside it on purpose, in `tests/` and in `#[cfg(test)]`
//! modules alike: they tamper with the file to prove the store refuses what
//! they did.
//!
//! **And gathers no statistics.** `ANALYZE` and `PRAGMA optimize` — which
//! SQLite recommends before a connection closes, and which `sqlx` runs on close
//! when `optimize_on_close` is set — write `sqlite_stat1`, and with this build's
//! SQLite `sqlite_stat4`: tables no migration builds, which `crates/nc-local`'s
//! self-check refuses at every open after, so one such statement makes the
//! kernel refuse the user's own file for good — measured with each of the
//! three, which is why the gate refuses all three.
//!
//! **Nor rewrites what the file says it is.** Setting `application_id`, or
//! writing a row of `_sqlx_migrations`, makes every open after refuse the
//! file as surely — `crates/nc-local`'s `a_rewritten_header_is_refused_at_open`
//! and `a_migration_edited_after_it_ran_is_refused` measure it — and setting
//! `schema_version` tells SQLite its schema is other than the one it wrote,
//! which SQLite's own documentation warns leaves a file it misreads. So the
//! gate refuses each: the two pragmas set to any value, as SQL or through the
//! door, and an `INSERT`, `REPLACE`, `UPDATE` or `DELETE` naming the table;
//! without those refusals, all of them pass it. Reading either is the store's
//! self-check, and is not refused.
//!
//! **What neither answers.** A statement assembled from pieces no single
//! literal holds is not a statement this reads, and `sqlx`'s own statements —
//! the migrator's bookkeeping — are `sqlx`'s. `sqlx` 0.9 compiles
//! `SqliteConnectOptions::extension` only under its `load-extension` feature,
//! which `sqlite-bundled` leaves off, and makes it an `unsafe fn` when it is
//! on; the needle for it is the third net, not the first.

use std::path::{Path, PathBuf};

use sqlx::{ConnectOptions as _, Connection as _};

use crate::gates::Finding;

/// The sources of the crates that open the local database, each crate's whole
/// source tree.
///
/// **Held to the manifests, both ways** (`unread_writers`, in a test): every member
/// whose manifest declares a way to SQLite is read here or named in
/// `NOT_READ`. The pruner's SQLite target runs its statements on the user's
/// file through `nc-local`'s pool, and left out, `PRAGMA optimize` planted in
/// it passes this gate, measured, where the store's own suite shows that
/// statement makes every later open refuse the file. `crates/nc-kernel/src` is
/// read before its manifest takes `nc-local`, so its first statement meets the
/// gate. **And the pruner whole**: read as its one SQLite file, on nothing
/// firmer than the word that its other sources speak to PostgreSQL alone, the
/// member counts as read and a `DROP TRIGGER` on a `SqlitePool` in its `lib.rs`
/// passes every gate. **And the daemon's**, which opens the file through
/// `nc-local` and hands the store to its kernel: it writes no statement, and a
/// first one it wrote would meet this gate.
pub(crate) const WRITERS: [&str; 4] = [
    "apps/daemon/src",
    "crates/nc-local/src",
    "crates/nc-kernel/src",
    "crates/nc-prune/src",
];

/// The members that open the local database and whose sources this gate does
/// not read, each with the reason.
#[cfg(test)]
pub(crate) const NOT_READ: [(&str, &str); 1] = [(
    "xtask",
    "the gates' own crate: its schema gate applies `local-migrations/` to a scratch file \
     it makes and removes, and its tests plant statements in copies of the tree on \
     purpose — the file it builds is never a user's",
)];

/// What one build of the local schema showed.
#[derive(Debug)]
pub(crate) struct Built {
    /// How many objects `sqlite_schema` held, each held to the list.
    pub(crate) objects: usize,
    /// How many migrations were applied.
    pub(crate) migrations: usize,
    /// The SQLite this build links.
    pub(crate) sqlite: String,
}

/// A scratch directory, removed however the gate leaves.
#[derive(Debug)]
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How many scratch directories this process has made, so two made at once —
/// the gate's own and a test's, or two tests' — are never the same directory.
static MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Scratch {
    /// A fresh directory in the system's temporary directory, named by this
    /// process and by how many it made before, or what [`Scratch::at`] refused.
    fn new(name: &str) -> Result<Self, String> {
        let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self::at(std::env::temp_dir().join(format!(
            "nc-xtask-local-{}-{made}-{name}",
            std::process::id()
        )))
    }

    /// The directory `root`, made by this call and private to this user from
    /// its first moment, or the sentence saying why it was not.
    ///
    /// **Never one it found.** The gate opens a SQLite file here with
    /// `create_if_missing`, and SQLite reads whatever database and `-wal`
    /// already sit at that name: in a temporary directory every account
    /// shares, a directory another account made first, or could write into,
    /// would hand the gate that account's pair — the one way RUSTSEC-2026-0459
    /// (CVE-2026-119375) reaches a process linking a SQLite below 3.54.0. So
    /// the directory is made by `nc_platform::create_private_directory` —
    /// `0700` on Unix, one entry for the user on Windows, in the call that
    /// makes it — and a name that is already there is refused, never used and
    /// never cleared: what a `Scratch` holds, and its `Drop` removes, is a
    /// directory this process made.
    fn at(root: PathBuf) -> Result<Self, String> {
        match nc_platform::create_private_directory(&root) {
            Ok(()) => Ok(Self(root)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
                "`{}` was already there, so this run did not make it, and the gate opens \
                 no SQLite file in a directory it did not make: a database and its `-wal` \
                 that another account left there would be read as the gate's own. Remove it \
                 if it is yours, which a run that was killed leaves, and run the gate again",
                root.display()
            )),
            Err(error) => Err(format!(
                "`{}` could not be made private to this user ({error}), and the gate opens \
                 no SQLite file in a directory it did not make. Check that the temporary \
                 directory exists and that this user can write in it",
                root.display()
            )),
        }
    }
}

/// Applies `local-migrations/` to a scratch file and holds what it builds to
/// the list `crates/nc-local` checks every database against.
///
/// # Errors
///
/// What differs, naming the object, the check or the version, and what to do.
pub(crate) fn the_local_schema(root: &Path) -> Result<Built, String> {
    let scratch = Scratch::new("schema")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("no runtime to build the local schema with: {error}"))?;
    runtime.block_on(built_and_judged(
        &root.join(crate::limits::LOCAL_MIGRATIONS),
        &scratch.0.join("local.sqlite3"),
        nc_local::schema::MINIMUM_SQLITE,
    ))
}

/// The build and the judgement, on one connection to one scratch file, with
/// the SQLite this build links held to `floor`.
///
/// **The floor is handed in so its refusal can be driven**: the SQLite linked
/// is the lockfile's, and no test can link an older one.
/// `a_sqlite_below_the_floor_is_refused_naming_both` hands in a floor above
/// every release, and with the comparison switched off that test fails; with
/// the floor fixed rather than handed in, the sentence naming CVE-2025-6965
/// would be a branch no test enters.
async fn built_and_judged(
    migrations: &Path,
    file: &Path,
    floor: (u32, u32, u32),
) -> Result<Built, String> {
    let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(file)
        .create_if_missing(true)
        .foreign_keys(true)
        .connect()
        .await
        .map_err(|error| format!("a scratch SQLite file could not be opened: {error}"))?;
    let migrator = sqlx::migrate::Migrator::new(migrations)
        .await
        .map_err(|error| format!("`local-migrations/` could not be read: {error}"))?;
    built_one_migration_at_a_time(&migrator, &mut connection).await?;
    let sqlite: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("the SQLite version could not be read: {error}"))?;
    if !nc_local::schema::at_least(&sqlite, floor) {
        let (major, minor, patch) = floor;
        return Err(format!(
            "this build links SQLite {sqlite}, below {major}.{minor}.{patch} — the release \
             that fixes CVE-2025-6965 — and `crates/nc-local` refuses to open a database \
             with it. `Cargo.lock` decides which: `cargo update -p libsqlite3-sys`"
        ));
    }
    let found = nc_local::schema::found_in(&mut connection)
        .await
        .map_err(|error| format!("the scratch file's schema could not be read: {error}"))?;
    nc_local::schema::judge(&found).map_err(|differs| {
        format!(
            "`local-migrations/` does not build what `crates/nc-local/src/schema.rs` \
             expects — {differs}. The crate compares every database it opens with that \
             list and refuses one that differs, so change the migration and the list \
             together; `cargo test -p nc-local` prints each object's digest when they \
             disagree"
        )
    })?;
    let integrity: Vec<String> = sqlx::query_scalar("PRAGMA main.integrity_check")
        .fetch_all(&mut connection)
        .await
        .map_err(|error| format!("`integrity_check` could not run: {error}"))?;
    if integrity != ["ok"] {
        return Err(format!(
            "SQLite's `integrity_check` over the file `local-migrations/` builds says: {}",
            integrity.join("; ")
        ));
    }
    let orphans = sqlx::query("PRAGMA main.foreign_key_check")
        .fetch_all(&mut connection)
        .await
        .map_err(|error| format!("`foreign_key_check` could not run: {error}"))?
        .len();
    if orphans != 0 {
        return Err(format!(
            "`local-migrations/` leaves {orphans} row(s) whose foreign key names nothing \
             (`PRAGMA foreign_key_check`): a migration wrote them with the keys switched off"
        ));
    }
    let application: i32 = sqlx::query_scalar("PRAGMA main.application_id")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("the header could not be read: {error}"))?;
    if application != nc_local::schema::APPLICATION_ID {
        return Err(format!(
            "the file `local-migrations/` builds carries application {application} in its \
             header, and `crates/nc-local` opens only {} — `0001_plugin_store.sql` writes it, \
             in the transaction that builds the tables",
            nc_local::schema::APPLICATION_ID
        ));
    }
    connection
        .close()
        .await
        .map_err(|error| format!("the scratch file could not be closed: {error}"))?;
    Ok(Built {
        objects: found.len(),
        migrations: migrator.migrations.len(),
        sqlite,
    })
}

/// Applies each migration in turn and holds the schema after each one but the
/// last to what `crates/nc-local` dates to that step.
///
/// **One migration at a time, because the crate judges a file that way.** It
/// holds a file to the objects of the migrations the file records before it
/// migrates it, so an object `EXPECTED` dates to the wrong migration is a
/// kernel that refuses, on the next upgrade, every file the build before it
/// left — and the whole schema built at once cannot see that.
///
/// # Errors
///
/// The migration that did not apply, or the step whose schema differs.
async fn built_one_migration_at_a_time(
    migrator: &sqlx::migrate::Migrator,
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), String> {
    let versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();
    for (at, version) in versions.iter().enumerate() {
        migrator
            .run_direct(Some(*version), &mut *connection, false)
            .await
            .map_err(|error| {
                format!(
                    "`local-migrations/` does not apply to an empty SQLite file: {error}. \
                     Every kernel runs these on its first start, so this is the file a user \
                     would be left with"
                )
            })?;
        // The last step is the whole schema, which the caller holds to the
        // whole list with a message of its own.
        if at + 1 == versions.len() {
            break;
        }
        let found = nc_local::schema::found_in(connection)
            .await
            .map_err(|error| {
                format!(
                    "the scratch file's schema could not be read after migration {version}: {error}"
                )
            })?;
        nc_local::schema::judge_at(&found, Some(&versions[..=at])).map_err(|differs| {
            format!(
                "`local-migrations/` up to {version} does not build what \
                 `crates/nc-local/src/schema.rs` dates to it — {differs}. The crate holds a \
                 file one migration behind to what its history built before it migrates it, \
                 so set each object's `since` to the migration that builds it"
            )
        })?;
    }
    Ok(())
}

/// One kind of statement the crates that open the local database may not write.
struct Refused {
    /// The lines of a text, comments already removed, that hold one.
    holds: fn(&str, &[Word]) -> Vec<usize>,
    /// What it is, for the finding.
    what: &'static str,
    /// Why it is refused, and what to do instead.
    fix: &'static str,
}

/// Every kind of statement the source gate refuses.
const REFUSED: [Refused; 19] = [
    Refused {
        holds: attaches,
        what: "`ATTACH` or `DETACH` of another database",
        fix: "a database attached to the connection is one the self-check never compared \
              with its list, and a name its tables shadow is unqualified in every \
              statement that forgot `main.` — keep the local database to its one file",
    },
    Refused {
        holds: edits_the_schema_table,
        what: "`writable_schema`",
        fix: "it edits `sqlite_schema` directly, which drops or rewrites a trigger with no \
              `DROP TRIGGER` for anything to see — the schema changes only through \
              `local-migrations/`",
    },
    Refused {
        holds: sets_another_journal,
        what: "`journal_mode` set to anything but `WAL`",
        fix: "with no journal, or one kept in memory, a transaction cannot roll back, and \
              a unit killed in the middle of its commit is left half-written — the one \
              thing the commit exists to prevent; and a rollback journal takes away the \
              log the store's readers run beside its writer. The connection options set \
              `WAL` and nothing sets another",
    },
    Refused {
        holds: syncs_less,
        what: "`synchronous` set below `FULL`",
        fix: "a unit's commit is kept on the disk only once the disk says so, which \
              `FULL` waits for at each commit; below it a power cut after the commit \
              returned loses the unit the journal recorded as kept. The connection \
              options set `FULL` and nothing sets less",
    },
    Refused {
        holds: trusts_the_schema,
        what: "`trusted_schema` turned on",
        fix: "with it on, a trigger or a view in the file may call any function SQLite \
              has — the file's schema becomes code the kernel runs; the connection \
              options turn it off and nothing turns it on",
    },
    Refused {
        holds: turns_foreign_keys_off,
        what: "`foreign_keys` turned off",
        fix: "the images and notes of a unit are deleted with it through `ON DELETE \
              CASCADE`, which SQLite runs only with foreign keys on — off, the pruner \
              leaves them for ever; the connection options turn them on and nothing \
              turns them off",
    },
    Refused {
        holds: skips_the_cell_size_check,
        what: "`cell_size_check` turned off",
        fix: "it is what makes SQLite check each page's cells as it reads them, so a \
              damaged page is refused rather than read; the connection options turn it \
              on and nothing turns it off",
    },
    Refused {
        holds: spills_temporary_tables,
        what: "`temp_store` set to anything but `MEMORY`",
        fix: "a temporary table or index kept on disk is a file of the user's data \
              outside the private directory's one database; the connection options keep \
              them in memory and nothing sets another",
    },
    Refused {
        holds: syncs_without_the_platter,
        what: "`fullfsync` turned off",
        fix: "on macOS `fsync` does not reach the platter and `F_FULLFSYNC` does, so \
              `synchronous = FULL` means nothing without it; the connection options turn \
              it on there and nothing turns it off",
    },
    Refused {
        holds: ignores_the_checks,
        what: "`ignore_check_constraints` turned on",
        fix: "it switches off every `CHECK` the schema declares — a value that is not \
              JSON, a negative revision — which the tables hold the store's rows to; \
              nothing turns it on",
    },
    Refused {
        holds: loads_an_extension,
        what: "an extension loaded into SQLite",
        fix: "an extension is native code running inside the kernel with every privilege \
              it has; nothing here needs one, and `sqlx` 0.9 makes the door `unsafe`",
    },
    Refused {
        holds: gathers_statistics,
        what: "`ANALYZE`, `PRAGMA optimize` or `optimize_on_close`",
        fix: "each writes `sqlite_stat1` — and with this build's SQLite `sqlite_stat4` — \
              into the file: tables no migration builds, which `crates/nc-local` refuses \
              at every open after, so the user's own database is refused for good. The \
              query planner does without statistics here; leave them ungathered",
    },
    Refused {
        holds: vacuums,
        what: "`VACUUM`",
        fix: "`VACUUM INTO` writes the whole database to a path of its own choosing — the \
              user's data in a file outside the private directory, made with the umask's \
              mode and judged by nothing — and a plain `VACUUM` rebuilds the file the \
              store's checks judged. Nothing here needs either",
    },
    Refused {
        holds: names_a_directory,
        what: "`temp_store_directory` or `data_store_directory`",
        fix: "each points SQLite's own files at a directory of its choosing, outside the \
              private one; both are deprecated, and the connection keeps its temporary \
              tables in memory",
    },
    Refused {
        holds: opens_the_door_unread,
        what: "a pragma set through the door under a key no literal reads as one pragma",
        fix: "`sqlx` writes the key as it is into `PRAGMA {key} = {value}`, so a key built \
              by a macro, read from the build's environment or a file, or holding a \
              statement of its own, sets what no reader of this source can see; write \
              the pragma's name as one literal",
    },
    Refused {
        holds: rewrites_the_header,
        what: "`application_id` or `schema_version` set",
        fix: "`application_id` says the file is this platform's, and `crates/nc-local` \
              refuses a file whose header is not the one \
              `local-migrations/0001_plugin_store.sql` writes at every open after, so \
              setting it refuses the user's own file for good; `schema_version` is what \
              SQLite reads its schema by, and a value it did not write is a file SQLite \
              misreads. That migration writes the one and SQLite keeps the other",
    },
    Refused {
        holds: rewrites_the_history,
        what: "a write to `_sqlx_migrations`",
        fix: "the table is the record of which migrations ran and what each ran, which \
              `crates/nc-local` holds every file to at every open: a row added, changed or \
              removed is a history this build did not write, and the user's own file is \
              refused for good. Only `sqlx`'s migrator writes it, outside these sources",
    },
    Refused {
        holds: migrates_from_elsewhere,
        what: "a migrator or a migration built when the store runs",
        fix: "`Migrator::new` over a path, `with_migrations`, a `MigrationSource`, \
              `ResolveWith` and `resolve_blocking` read their `.sql` when the store runs — a \
              relative path against the daemon's working directory — and `sqlx` hands the \
              text to the database vouched for, so the user's file would be migrated with \
              whatever SQL sits there. Only the one `sqlx::migrate!` of \
              `local-migrations/`, which embeds the directory this gate builds and judges \
              when the crate compiles, migrates it",
    },
    Refused {
        holds: defines_the_schema,
        what: "a statement that creates, drops or alters a table, index, trigger or view",
        fix: "the local database's schema lives in `local-migrations/`, where the gate \
              builds it and `crates/nc-local/src/schema.rs` lists it — DDL anywhere else \
              is a change the self-check refuses at the next open, or one that runs \
              before it and is never seen",
    },
];

/// One word of a text, upper-cased, and the line it is on.
///
/// **A word, not a line**: a statement wraps where it reads best, and `CREATE`
/// at the end of one line of a string with `TABLE` at the start of the next is
/// the same statement to SQLite.
type Word = (String, usize);

/// Where one character of a source was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Rust, outside every string literal.
    Code,
    /// Inside the `n`th string literal of the source, as the string holds it.
    Literal(usize),
}

/// One character as the compiler hands it on, the line it was written on, and
/// where it was read.
type Read = (char, usize, Place);

/// A source as the compiler hands its strings on: Rust as written, and each
/// string literal as the text it is — escapes decoded, a continued line
/// joined, a raw string read to its own closing, a character literal one gap.
///
/// **A string continued across a line is one string to Rust, and so one to
/// SQLite.** A `\` that ends a line inside a literal drops the newline and the
/// whitespace that opens the next line, so `"DROP TRIG\` and `GER t"` reach
/// SQLite as `DROP TRIGGER t` — measured, a reader that splits each line into
/// words of its own lets it past. An escaped backslash at a line's end joins
/// nothing: it is one backslash, and the newline after it stays.
///
/// **And `\n` is a newline**, which is where a SQL `--` comment ends: the
/// literal `"CREATE -- x\n TABLE t"` is two lines to SQLite, and the second
/// builds a table. A character literal is read as one gap because `'"'` holds
/// a quote that opens no string, and taking it for one reads every literal
/// after it inside out.
fn as_compiled(code: &str) -> Vec<Read> {
    let characters: Vec<char> = code.chars().collect();
    let mut out = Vec::with_capacity(characters.len());
    let mut line = 1_usize;
    let mut literals = 0_usize;
    let mut at = 0;
    while at < characters.len() {
        if let Some((opener, hashes)) = crate::gates::raw_string_at(&characters, at) {
            literals += 1;
            at = raw_body(
                &characters,
                at + opener,
                hashes,
                &mut line,
                literals,
                &mut out,
            );
            continue;
        }
        let here = characters[at];
        if here == '\''
            && let Some(width) = crate::gates::char_literal_at(&characters, at)
        {
            out.push((' ', line, Place::Code));
            at += width;
            continue;
        }
        if here == '"' {
            literals += 1;
            at = string_body(&characters, at + 1, &mut line, literals, &mut out);
            continue;
        }
        out.push((here, line, Place::Code));
        line += usize::from(here == '\n');
        at += 1;
    }
    out
}

/// A raw string's text, read from just past its opener: the position just past
/// its closing quote and hashes.
fn raw_body(
    characters: &[char],
    mut at: usize,
    hashes: usize,
    line: &mut usize,
    literal: usize,
    out: &mut Vec<Read>,
) -> usize {
    while at < characters.len() {
        let here = characters[at];
        if here == '"' && (1..=hashes).all(|offset| characters.get(at + offset) == Some(&'#')) {
            return at + 1 + hashes;
        }
        out.push((here, *line, Place::Literal(literal)));
        *line += usize::from(here == '\n');
        at += 1;
    }
    at
}

/// An ordinary string literal's text, read from just past its opening quote:
/// the position just past its closing one.
fn string_body(
    characters: &[char],
    mut at: usize,
    line: &mut usize,
    literal: usize,
    out: &mut Vec<Read>,
) -> usize {
    while at < characters.len() {
        let here = characters[at];
        if here == '"' {
            return at + 1;
        }
        if here != '\\' {
            out.push((here, *line, Place::Literal(literal)));
            *line += usize::from(here == '\n');
            at += 1;
            continue;
        }
        match characters.get(at + 1) {
            // A continued line: the backslash, then every space, tab, carriage
            // return and newline after it, which Rust drops.
            Some('\n' | '\r') => {
                at += 1;
                while let Some(&next @ (' ' | '\t' | '\r' | '\n')) = characters.get(at) {
                    *line += usize::from(next == '\n');
                    at += 1;
                }
            }
            Some(&escaped) => {
                let (decoded, width) = escape(characters, at + 1, escaped);
                out.push((decoded, *line, Place::Literal(literal)));
                at += 1 + width;
            }
            None => at += 1,
        }
    }
    at
}

/// The character an escape stands for, and how many characters past the
/// backslash it spans; `at` is the character after the backslash.
fn escape(characters: &[char], at: usize, escaped: char) -> (char, usize) {
    let hex = |digits: &[char]| {
        u32::from_str_radix(&digits.iter().collect::<String>(), 16)
            .ok()
            .and_then(char::from_u32)
            .unwrap_or(char::REPLACEMENT_CHARACTER)
    };
    match escaped {
        'n' => ('\n', 1),
        'r' => ('\r', 1),
        't' => ('\t', 1),
        '0' => ('\0', 1),
        'x' => {
            let digits = characters.get(at + 1..at + 3).unwrap_or_default();
            (hex(digits), 1 + digits.len())
        }
        'u' => {
            // `\u{…}`, to its closing brace.
            let close = characters
                .get(at..)
                .and_then(|rest| rest.iter().position(|&c| c == '}'))
                .unwrap_or(1);
            (
                hex(characters.get(at + 2..at + close).unwrap_or_default()),
                close + 1,
            )
        }
        other => (other, 1),
    }
}

/// The words of a text, upper-cased, each with the one-based line it begins on:
/// what SQLite would read as keywords in its strings, and Rust's names around
/// them.
///
/// **A SQL comment inside a string is not a word.** Read as one, a word between
/// two keywords parts them, measured: `"DROP /* gone */ TRIGGER
/// main.store_writes_are_never_edited"` passes such a gate and SQLite runs it,
/// the trigger gone after; `"PRAGMA journal_mode = /* for speed */ OFF"` passes
/// and leaves the journal off; and a comment holding the word `off` after
/// `journal_mode =` is refused for a statement that sets nothing. So inside a
/// literal, `--` to the end of its line and `/* … */` are each a gap — unless a
/// SQL quote, `'…'` or `"…"`, is open, which holds them as text. A quote's own
/// words are still read: `journal_mode = 'OFF'` turns the journal off too.
fn words(code: &str) -> Vec<Word> {
    let read = as_compiled(code);
    let mut found = Vec::new();
    let mut word = String::new();
    let mut began = 1_usize;
    let mut quoted: Option<char> = None;
    let mut at = 0;
    while at < read.len() {
        let (here, line, place) = read[at];
        if at > 0 && read[at - 1].2 != place {
            flush(&mut word, began, &mut found);
            quoted = None;
        }
        if let Place::Literal(_) = place {
            let next = read
                .get(at + 1)
                .filter(|(_, _, after)| *after == place)
                .map(|(character, _, _)| *character);
            let closing: Option<&[char]> = match (quoted, here, next) {
                (None, '-', Some('-')) => Some(&['\n']),
                (None, '/', Some('*')) => Some(&['*', '/']),
                _ => None,
            };
            if let Some(closing) = closing {
                flush(&mut word, began, &mut found);
                at = past_the_comment(&read, at + 2, place, closing);
                continue;
            }
            if here == '\'' || here == '"' {
                flush(&mut word, began, &mut found);
                quoted = match quoted {
                    None => Some(here),
                    Some(open) if open == here => None,
                    still => still,
                };
                at += 1;
                continue;
            }
        }
        if here.is_alphanumeric() || here == '_' {
            if word.is_empty() {
                began = line;
            }
            word.push(here);
        } else {
            flush(&mut word, began, &mut found);
        }
        at += 1;
    }
    flush(&mut word, began, &mut found);
    found
}

/// Ends the word being read, if there is one.
fn flush(word: &mut String, began: usize, found: &mut Vec<Word>) {
    if !word.is_empty() {
        found.push((word.to_ascii_uppercase(), began));
        word.clear();
    }
}

/// The position just past a SQL comment whose body starts at `from`: past
/// `closing` in the same literal, or where the literal ends — SQLite ends a
/// comment with the statement it is in.
fn past_the_comment(read: &[Read], from: usize, place: Place, closing: &[char]) -> usize {
    let mut at = from;
    while at < read.len() && read[at].2 == place {
        let closes = closing.iter().enumerate().all(|(offset, wanted)| {
            read.get(at + offset)
                .is_some_and(|(character, _, there)| character == wanted && *there == place)
        });
        if closes {
            return at + closing.len();
        }
        at += 1;
    }
    at
}

/// Each line that attaches or detaches another database.
fn attaches(_: &str, words: &[Word]) -> Vec<usize> {
    words
        .iter()
        .filter(|(word, _)| word == "ATTACH" || word == "DETACH")
        .map(|(_, line)| *line)
        .collect()
}

/// Each line that runs `VACUUM`, into a file or in place.
fn vacuums(_: &str, words: &[Word]) -> Vec<usize> {
    words
        .iter()
        .filter(|(word, _)| word == "VACUUM")
        .map(|(_, line)| *line)
        .collect()
}

/// Each line that names a directory pragma, whatever it sets it to.
fn names_a_directory(_: &str, words: &[Word]) -> Vec<usize> {
    words
        .iter()
        .filter(|(word, _)| word == "TEMP_STORE_DIRECTORY" || word == "DATA_STORE_DIRECTORY")
        .map(|(_, line)| *line)
        .collect()
}

/// Each line that sets a pragma through the door under a key no literal
/// reads as one pragma ([`through_the_door`]).
fn opens_the_door_unread(code: &str, _: &[Word]) -> Vec<usize> {
    settings(code)
        .into_iter()
        .filter(|setting| setting.name == UNREAD)
        .map(|setting| setting.line)
        .collect()
}

/// Each line that names `writable_schema`, whatever it sets it to.
fn edits_the_schema_table(_: &str, words: &[Word]) -> Vec<usize> {
    words
        .iter()
        .filter(|(word, _)| word == "WRITABLE_SCHEMA")
        .map(|(_, line)| *line)
        .collect()
}

/// A pragma of the connection's hardening: what SQLite calls it, as the
/// gate's words spell it, and whether a value SQLite would read for it keeps
/// the hardening — what `crates/nc-local` sets it to, or a value SQLite reads
/// the same.
///
/// **Judged as SQLite reads a value, and allowed rather than refused by
/// name.** A list of the values that weaken one is a list SQLite's own reading
/// outruns: it reads `synchronous = ON`, `YES`, `TRUE` and `01` as `NORMAL`,
/// and any word it does not know as `NORMAL` too; a boolean as true only for
/// `ON`, `YES`, `TRUE` or a numeral not zero, so `foreign_keys = NONE` turns
/// them off and `trusted_schema = 2` on — measured on the store's own pool,
/// each spelling passing such a list, and, under one, a
/// `PRAGMA synchronous = ON` planted after a commit in the shipped store
/// passing the whole of `cargo xtask check`.
struct Hardened {
    /// The pragma, upper-cased.
    name: &'static str,
    /// Whether a value, upper-cased, keeps the hardening.
    keeps: fn(&str) -> bool,
}

/// Every pragma of the connection's hardening, each held to what keeps it.
///
/// Held to what `crates/nc-local`'s connection options set, both ways, by
/// `every_pragma_the_store_sets_is_one_the_gate_guards`.
const HARDENED: [Hardened; 8] = [
    Hardened {
        name: "JOURNAL_MODE",
        keeps: |value| value == "WAL",
    },
    Hardened {
        name: "SYNCHRONOUS",
        keeps: |value| match numeral(value) {
            Some(level) => level == 2 || level == 3,
            None => value == "FULL" || value == "EXTRA",
        },
    },
    Hardened {
        name: "FOREIGN_KEYS",
        keeps: truthy,
    },
    Hardened {
        name: "TRUSTED_SCHEMA",
        keeps: falsy,
    },
    Hardened {
        name: "CELL_SIZE_CHECK",
        keeps: truthy,
    },
    Hardened {
        name: "TEMP_STORE",
        keeps: |value| numeral(value).map_or(value == "MEMORY", |store| store == 2),
    },
    Hardened {
        name: "FULLFSYNC",
        keeps: truthy,
    },
    Hardened {
        name: "IGNORE_CHECK_CONSTRAINTS",
        keeps: falsy,
    },
];

/// The pragmas of [`HARDENED`] the connection options leave at SQLite's own
/// default, which keeps the hardening, rather than set.
#[cfg(test)]
const KEPT_AT_THEIR_DEFAULT: [&str; 1] = ["IGNORE_CHECK_CONSTRAINTS"];

/// The number SQLite reads a value as, when it reads it as one: a value that
/// begins with a digit is the run of digits it begins with, as `sqlite3Atoi`
/// reads it — `01` is one.
fn numeral(value: &str) -> Option<u64> {
    let digits: Vec<u8> = value.bytes().take_while(u8::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    Some(digits.iter().fold(0_u64, |read, digit| {
        read.saturating_mul(10)
            .saturating_add(u64::from(digit - b'0'))
    }))
}

/// Whether SQLite reads a value, upper-cased, as true: `ON`, `YES`, `TRUE`, or
/// a numeral not zero — and every other word as false.
fn truthy(value: &str) -> bool {
    numeral(value).map_or(matches!(value, "ON" | "YES" | "TRUE"), |read| read != 0)
}

/// Whether a value, upper-cased, is one SQLite reads as false and nothing
/// else: `OFF`, `NO`, `FALSE`, or a numeral of zeros.
///
/// **Allowed, never the opposite of [`truthy`].** "Not true" lets through every
/// value a reader takes differently from SQLite: measured, `+1`, `0x1`, a
/// quoted `'ON'` and `ON;` each turn `trusted_schema` on past such a reading,
/// SQLite reading the sign away, the hexadecimal as a number and the quotes
/// and the `;` as nothing. A pragma kept off is held to the spellings that say
/// off, and every other is refused rather than judged.
fn falsy(value: &str) -> bool {
    matches!(value, "OFF" | "NO" | "FALSE")
        || (!value.is_empty() && value.bytes().all(|digit| digit == b'0'))
}

/// The name a pragma set through the door is recorded under when no literal
/// reads as one pragma ([`through_the_door`]): never a pragma's.
const UNREAD: &str = "?";

/// One pragma set in a source: its name and the value it is set to, upper-
/// cased — none when no literal holds it — and the line it is set on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PragmaSet {
    name: String,
    value: Option<String>,
    line: usize,
}

/// Every pragma `code` sets: as SQL in a string — `PRAGMA [schema.]name =
/// value` or `PRAGMA name(value)` — through the pragma door, `.pragma(key,
/// value)` or `SqliteConnectOptions::pragma(options, key, value)`, and through
/// `sqlx`'s own options, `.journal_mode(..)`, `.synchronous(..)` and
/// `.foreign_keys(..)` or the same called as a path, read by the value handed
/// them and not by the name of its type, so a type imported under another name
/// changes nothing.
///
/// **A value is read only when it is written out**: a literal, a word or a
/// path to one. Read as the last word of its argument, `.foreign_keys(!true)`
/// turns them off and an `if` choosing `Off` passes as the `Wal` beside it —
/// measured — so any other value is one no literal holds, which
/// nothing can say keeps the hardening; and a setter named and not called, as
/// a function handed on, is one whose value nobody reads at all.
fn settings(code: &str) -> Vec<PragmaSet> {
    let pieces = pieces(&as_compiled(code));
    let mut found = Vec::new();
    for (at, (piece, line)) in pieces.iter().enumerate() {
        let word = match piece {
            Piece::Text(text) => {
                found.extend(pragmas_in(text, *line));
                continue;
            }
            Piece::Word(word) => word,
            Piece::Mark(_) => continue,
        };
        let on = at.checked_sub(1).and_then(|before| pieces.get(before));
        let after_a_dot = matches!(on, Some((Piece::Mark('.'), _)));
        let after_a_path = matches!(on, Some((Piece::Mark(':'), _)));
        let setter = matches!(
            word.as_str(),
            "journal_mode" | "synchronous" | "foreign_keys"
        );
        if !(after_a_dot || after_a_path) || !(setter || word == "pragma") {
            continue;
        }
        if !matches!(pieces.get(at + 1), Some((Piece::Mark('('), _))) {
            found.push(PragmaSet {
                name: if setter {
                    word.to_ascii_uppercase()
                } else {
                    UNREAD.to_owned()
                },
                value: None,
                line: *line,
            });
            continue;
        }
        let arguments = arguments_of(&pieces, at + 1);
        if word == "pragma" {
            found.extend(through_the_door(&arguments, *line));
        } else {
            found.push(PragmaSet {
                name: word.to_ascii_uppercase(),
                value: arguments.last().and_then(|value| written_out(value)),
                line: *line,
            });
        }
    }
    found
}

/// The value `argument` names when it is written out — one literal, one word,
/// or a path of words — upper-cased: the literal's text, the word, or the
/// path's last word.
fn written_out(argument: &[Piece]) -> Option<String> {
    let a_path = argument.len() % 3 == 1
        && argument.iter().enumerate().all(|(at, piece)| match piece {
            Piece::Word(_) => at % 3 == 0,
            Piece::Mark(':') => at % 3 != 0,
            Piece::Mark(_) | Piece::Text(_) => false,
        });
    match argument {
        [Piece::Text(text)] => Some(text.trim().to_ascii_uppercase()),
        [.., Piece::Word(last)] if a_path => Some(last.to_ascii_uppercase()),
        _ => None,
    }
}

/// A piece of a source as [`settings`] reads it: a Rust name, one of Rust's
/// marks, or a whole string literal's text.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Word(String),
    Mark(char),
    Text(String),
}

/// A compiled source, as pieces with the line each begins on.
fn pieces(read: &[Read]) -> Vec<(Piece, usize)> {
    let mut pieces = Vec::new();
    let mut at = 0;
    while at < read.len() {
        let (here, line, place) = read[at];
        if let Place::Literal(_) = place {
            let mut text = String::new();
            while at < read.len() && read[at].2 == place {
                text.push(read[at].0);
                at += 1;
            }
            pieces.push((Piece::Text(text), line));
            continue;
        }
        if here.is_alphanumeric() || here == '_' {
            let mut word = String::new();
            while at < read.len()
                && read[at].2 == Place::Code
                && (read[at].0.is_alphanumeric() || read[at].0 == '_')
            {
                word.push(read[at].0);
                at += 1;
            }
            pieces.push((Piece::Word(word), line));
            continue;
        }
        if !here.is_whitespace() {
            pieces.push((Piece::Mark(here), line));
        }
        at += 1;
    }
    pieces
}

/// The arguments of the call whose `(` is at `open`, each as its pieces,
/// split at the commas outside any bracket of its own.
fn arguments_of(pieces: &[(Piece, usize)], open: usize) -> Vec<Vec<Piece>> {
    let mut arguments = vec![Vec::new()];
    let mut depth = 0_usize;
    for (piece, _) in pieces.iter().skip(open + 1) {
        match piece {
            Piece::Mark('(' | '[' | '{') => depth += 1,
            Piece::Mark(')' | ']' | '}') if depth == 0 => break,
            Piece::Mark(')' | ']' | '}') => depth -= 1,
            Piece::Mark(',') if depth == 0 => {
                arguments.push(Vec::new());
                continue;
            }
            _ => {}
        }
        if let Some(argument) = arguments.last_mut() {
            argument.push(piece.clone());
        }
    }
    arguments
}

/// What a call of the pragma door sets: its last two arguments, the key and
/// the value, read as `sqlx` sends them — `PRAGMA {key} = {value}` — by the
/// reader [`pragmas_in`] reads a statement with, the value none when no
/// literal is all of it.
///
/// **One reader, so one reading.** Compared as it is spelt, a key with a
/// schema's name before it or quotes around it, both of which SQLite reads
/// through, turns `ignore_check_constraints` and `trusted_schema` on past such
/// a comparison, measured in the shipped store. A key no literal is all
/// of, or one SQLite would not read as exactly one pragma, is recorded under
/// [`UNREAD`] as well, which [`REFUSED`] refuses: what such a key sets, nobody
/// reading this source can say.
fn through_the_door(arguments: &[Vec<Piece>], line: usize) -> Vec<PragmaSet> {
    let literal = |argument: &Vec<Piece>| match argument.as_slice() {
        [Piece::Text(text)] => Some(text.clone()),
        _ => None,
    };
    let unread = PragmaSet {
        name: UNREAD.to_owned(),
        value: None,
        line,
    };
    let [.., key, value] = arguments else {
        return vec![unread];
    };
    let Some(key) = literal(key) else {
        return vec![unread];
    };
    let value = literal(value);
    let sent = format!("PRAGMA {key} = {}", value.as_deref().unwrap_or("NULL"));
    let mut found = pragmas_in(&sent, line);
    if value.is_none() {
        for setting in &mut found {
            setting.value = None;
        }
    }
    if found.len() != 1 {
        found.push(unread);
    }
    found
}

/// Every pragma the SQL in `text`, a string literal that begins on `line`,
/// sets — read as SQLite reads it: a `--` or `/* … */` comment a gap, and a
/// quoted value the text inside its quotes.
fn pragmas_in(text: &str, line: usize) -> Vec<PragmaSet> {
    let tokens = sql_tokens(text, line);
    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if tokens[at].0 != "PRAGMA" {
            continue;
        }
        let mut named = at + 1;
        if tokens.get(named + 1).is_some_and(|(token, _)| token == ".") {
            named += 2;
        }
        let Some((name, on)) = tokens.get(named) else {
            continue;
        };
        if tokens
            .get(named + 1)
            .is_some_and(|(token, _)| token == "=" || token == "(")
        {
            // A sign is read as SQLite's parser reads it: `+1` is `1`, and
            // `-1` a value of its own, which a boolean pragma reads as false.
            let value = match tokens.get(named + 2) {
                Some((sign, _)) if sign == "+" => {
                    tokens.get(named + 3).map(|(value, _)| value.clone())
                }
                Some((sign, _)) if sign == "-" => {
                    tokens.get(named + 3).map(|(value, _)| format!("-{value}"))
                }
                other => other.map(|(value, _)| value.clone()),
            };
            found.push(PragmaSet {
                name: name.clone(),
                value,
                line: *on,
            });
        }
    }
    found
}

/// The tokens of the SQL in `text`, upper-cased, each with its line: a word,
/// a quoted text's contents, or a mark.
fn sql_tokens(text: &str, mut line: usize) -> Vec<(String, usize)> {
    let characters: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < characters.len() {
        let here = characters[at];
        let closing: Option<&[char]> = match (here, characters.get(at + 1)) {
            ('-', Some('-')) => Some(&['\n']),
            ('/', Some('*')) => Some(&['*', '/']),
            _ => None,
        };
        if let Some(closing) = closing {
            at += 2;
            while at < characters.len() && !characters[at..].starts_with(closing) {
                line += usize::from(characters[at] == '\n');
                at += 1;
            }
            at += closing.len();
            continue;
        }
        let quote = match here {
            '\'' | '"' | '`' => Some(here),
            '[' => Some(']'),
            _ => None,
        };
        if let Some(close) = quote {
            let mut inside = String::new();
            at += 1;
            while at < characters.len() {
                if characters[at] == close {
                    if close != ']' && characters.get(at + 1) == Some(&close) {
                        inside.push(close);
                        at += 2;
                        continue;
                    }
                    break;
                }
                inside.push(characters[at]);
                at += 1;
            }
            tokens.push((inside.to_ascii_uppercase(), line));
            at += 1;
            continue;
        }
        if here.is_alphanumeric() || here == '_' {
            let mut word = String::new();
            while at < characters.len()
                && (characters[at].is_alphanumeric() || characters[at] == '_')
            {
                word.push(characters[at]);
                at += 1;
            }
            tokens.push((word.to_ascii_uppercase(), line));
            continue;
        }
        if !here.is_whitespace() {
            tokens.push((here.to_string(), line));
        }
        line += usize::from(here == '\n');
        at += 1;
    }
    tokens
}

/// Each line of `code` that sets the pragma `name` to a value that does not
/// keep the connection's hardening — or to one no literal holds, which nobody
/// can say keeps it.
fn undone(code: &str, name: &str) -> Vec<usize> {
    let Some(hardened) = HARDENED.iter().find(|hardened| hardened.name == name) else {
        return Vec::new();
    };
    settings(code)
        .into_iter()
        .filter(|setting| setting.name == name)
        .filter(|setting| !setting.value.as_deref().is_some_and(hardened.keeps))
        .map(|setting| setting.line)
        .collect()
}

/// Each line that sets `journal_mode` to anything but `WAL`.
fn sets_another_journal(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "JOURNAL_MODE")
}

/// Each line that sets `synchronous` below `FULL`, as SQLite reads it.
fn syncs_less(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "SYNCHRONOUS")
}

/// Each line that turns `trusted_schema` on, as SQLite reads it.
fn trusts_the_schema(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "TRUSTED_SCHEMA")
}

/// Each line that turns `foreign_keys` off, as SQLite reads it.
fn turns_foreign_keys_off(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "FOREIGN_KEYS")
}

/// Each line that turns `cell_size_check` off, as SQLite reads it.
fn skips_the_cell_size_check(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "CELL_SIZE_CHECK")
}

/// Each line that keeps temporary tables anywhere but in memory.
fn spills_temporary_tables(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "TEMP_STORE")
}

/// Each line that turns `fullfsync` off, as SQLite reads it.
fn syncs_without_the_platter(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "FULLFSYNC")
}

/// Each line that turns `ignore_check_constraints` on, as SQLite reads it.
fn ignores_the_checks(code: &str, _: &[Word]) -> Vec<usize> {
    undone(code, "IGNORE_CHECK_CONSTRAINTS")
}

/// The SQL function by name, and `sqlx`'s two doors by their call: a method
/// named `extension` that is handed an argument — `Path::extension()` is handed
/// none, and is how the kernel reads a file's type.
fn loads_an_extension(code: &str, words: &[Word]) -> Vec<usize> {
    let mut lines: Vec<usize> = words
        .iter()
        .filter(|(word, _)| word.contains("LOAD_EXTENSION") || word == "EXTENSION_WITH_ENTRYPOINT")
        .map(|(_, line)| *line)
        .collect();
    for (index, line) in code.lines().enumerate() {
        let mut from = 0;
        while let Some(at) = line[from..].find(".extension(") {
            let past = from + at + ".extension(".len();
            if !line[past..].trim_start().starts_with(')') {
                lines.push(index + 1);
            }
            from = past;
        }
    }
    lines
}

/// Each line that gathers the planner's statistics: `ANALYZE`,
/// `PRAGMA optimize`, `sqlx`'s `optimize_on_close`, and the `analysis_limit`
/// that only those two read.
///
/// **Whole words**, so a name that merely begins with one — `optimized`,
/// `analyze_layout` — is not a statement.
fn gathers_statistics(_: &str, words: &[Word]) -> Vec<usize> {
    const GATHER: [&str; 4] = ["ANALYZE", "OPTIMIZE", "OPTIMIZE_ON_CLOSE", "ANALYSIS_LIMIT"];
    words
        .iter()
        .filter(|(word, _)| GATHER.contains(&word.as_str()))
        .map(|(_, line)| *line)
        .collect()
}

/// Each line that sets `application_id` or `schema_version`, to any value, as
/// SQL or through the pragma door.
fn rewrites_the_header(code: &str, _: &[Word]) -> Vec<usize> {
    settings(code)
        .into_iter()
        .filter(|setting| setting.name == "APPLICATION_ID" || setting.name == "SCHEMA_VERSION")
        .map(|setting| setting.line)
        .collect()
}

/// Each line whose SQL writes `_sqlx_migrations`: the table named after
/// `INTO`, after `UPDATE` or `UPDATE OR <conflict>`, or after `DELETE FROM`,
/// a schema's name before it or not, quoted or not.
///
/// **A read is not refused**: the store's self-check reads the table to hold
/// a file to the history it records.
fn rewrites_the_history(code: &str, _: &[Word]) -> Vec<usize> {
    let mut lines = Vec::new();
    for (piece, line) in pieces(&as_compiled(code)) {
        let Piece::Text(text) = piece else {
            continue;
        };
        let tokens = sql_tokens(&text, line);
        for (at, (token, on)) in tokens.iter().enumerate() {
            if token != "_SQLX_MIGRATIONS" {
                continue;
            }
            let named = if at >= 2 && tokens[at - 1].0 == "." {
                at - 2
            } else {
                at
            };
            let before = |back: usize| {
                named
                    .checked_sub(back)
                    .map_or("", |index| tokens[index].0.as_str())
            };
            if matches!(before(1), "INTO" | "UPDATE")
                || (before(1) == "FROM" && before(2) == "DELETE")
                || (before(3) == "UPDATE" && before(2) == "OR")
            {
                lines.push(*on);
            }
        }
    }
    lines
}

/// The one line of the sources that open the local database that may build a
/// migrator: the migrations `local-migrations/` held when the crate compiled.
const EMBEDDED: &str =
    "static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!(\"../../local-migrations\");";

/// Each line that builds a migrator, or a migration, other than [`EMBEDDED`]:
/// one that names the `Migrator` type, calls `migrate!`, reads a
/// `MigrationSource` — `ResolveWith`, `resolve_blocking` — or builds a
/// `Migration` by its path or its fields, read as the Rust outside every
/// string literal.
///
/// **By the type's name, not by its constructors**, so a `use … as` or a
/// type alias that renames it is the line that names it. Read by its
/// constructors, the door would be open: the shipped store migrating from
/// `Migrator::new(Path::new("../../local-migrations"))` passes such a reading,
/// cargo running each suite from the package's root, where the relative path
/// is this repository's own directory.
fn migrates_from_elsewhere(code: &str, _: &[Word]) -> Vec<usize> {
    const DOORS: [&str; 5] = [
        "Migrator",
        "MigrationSource",
        "ResolveWith",
        "resolve_blocking",
        "with_migrations",
    ];
    let read = as_compiled(code);
    let in_a_name = |at: usize| {
        read.get(at).is_some_and(|(character, _, place)| {
            *place == Place::Code && (character.is_ascii_alphanumeric() || *character == '_')
        })
    };
    let mut lines = std::collections::BTreeSet::new();
    let mut at = 0;
    while at < read.len() {
        if !in_a_name(at) || (at > 0 && in_a_name(at - 1)) {
            at += 1;
            continue;
        }
        let start = at;
        while in_a_name(at) {
            at += 1;
        }
        let name: String = read[start..at]
            .iter()
            .map(|(character, ..)| *character)
            .collect();
        let next = read[at..]
            .iter()
            .find(|(character, _, place)| *place != Place::Code || !character.is_whitespace())
            .map(|(character, ..)| *character);
        if DOORS.contains(&name.as_str())
            || (name == "Migration" && matches!(next, Some(':' | '{')))
            || (name == "migrate" && next == Some('!'))
        {
            lines.insert(read[start].1);
        }
    }
    let embedded: Vec<usize> = code
        .lines()
        .enumerate()
        .filter(|(_, text)| text.trim() == EMBEDDED)
        .map(|(index, _)| index + 1)
        .collect();
    lines
        .into_iter()
        .filter(|line| !embedded.contains(line))
        .collect()
}

/// Each line that creates, drops or alters a table, an index, a trigger or a
/// view — a word apart or a line apart, as a statement wraps.
fn defines_the_schema(_: &str, words: &[Word]) -> Vec<usize> {
    const OBJECTS: [&str; 4] = ["TABLE", "INDEX", "TRIGGER", "VIEW"];
    const BETWEEN: [&str; 4] = ["TEMP", "TEMPORARY", "UNIQUE", "VIRTUAL"];
    let word = |at: usize| words.get(at).map_or("", |(word, _)| word.as_str());
    (0..words.len())
        .filter(|&at| match word(at) {
            "CREATE" => {
                OBJECTS.contains(&word(at + 1))
                    || (BETWEEN.contains(&word(at + 1)) && OBJECTS.contains(&word(at + 2)))
            }
            "DROP" => OBJECTS.contains(&word(at + 1)),
            "ALTER" => word(at + 1) == "TABLE",
            _ => false,
        })
        .map(|at| words[at].1)
        .collect()
}

/// Every statement the crates that open the local database write, held to
/// [`REFUSED`]. Returns the findings and how many files were read.
///
/// **Two counters, the second on the way out of the judgement**, held equal by
/// `gates::everything_reached_a_verdict`: one counted before a file was judged
/// pays a filter between the two as surely as a file read. With
/// `if path.starts_with(root.join("crates/nc-kernel")) { continue; }` written
/// below a single counter, the gate prints the same count with a `DROP TRIGGER`
/// in a shipped kernel source; `the_local_sql_walk_goes_everywhere_its_writers_have`
/// plants a statement in every directory its writers have, which is what
/// sees a filter the counters cannot.
///
/// # Errors
///
/// Why a source tree could not be walked.
pub(crate) fn no_local_sql_outside_the_migrations(
    root: &Path,
) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let (mut read, mut judged) = (0_usize, 0_usize);
    for path in the_sources_of_the_writers(root)? {
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        read += 1;
        findings.extend(refused_in(&crate::relative_to(root, &path), &text));
        judged += 1;
    }
    crate::gates::everything_reached_a_verdict(
        "file",
        "xtask/src/local.rs",
        read,
        judged,
        &mut findings,
    );
    Ok((findings, read))
}

/// Every `.rs` file [`WRITERS`] names, in order: the files under each tree,
/// and each file named on its own.
///
/// # Errors
///
/// Why a tree could not be walked, or that a path names nothing on disk.
fn the_sources_of_the_writers(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files = Vec::new();
    for named in WRITERS {
        let path = root.join(named);
        if path.is_file() {
            files.push(path);
            continue;
        }
        if !path.is_dir() {
            return Err(format!(
                "`{named}` is named in `WRITERS` in xtask/src/local.rs and is not on disk, \
                 so the source gate over local SQL reads nothing there"
            ));
        }
        let mut under = Vec::new();
        crate::gates::walk(&path, &mut |found| {
            if found.extension().is_some_and(|kind| kind == "rs") {
                under.push(found.to_path_buf());
            }
        })?;
        under.sort();
        files.extend(under);
    }
    Ok(files)
}

/// A finding for each member that opens the local database and is neither
/// read by this gate nor named in [`NOT_READ`], and for each name there that
/// no longer opens it.
///
/// `opening` is what the manifests say, through
/// `members::opening_the_local_database`. **A test holds the tree to it**,
/// inside `check` like every test, rather than the step itself: the step runs
/// over scratch trees that have no manifest for `cargo metadata` to read.
#[cfg(test)]
fn unread_writers(opening: &[String], writers: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for member in opening {
        // **Its whole source tree**, not a file of it: a file named here would
        // count the member as read, and a statement in the pruner's `lib.rs`
        // would then pass the gate beside its `sqlite.rs`.
        let read = writers
            .iter()
            .any(|named| *named == format!("{member}/src"));
        let named = NOT_READ.iter().any(|(name, _)| name == member);
        if !read && !named {
            findings.push(Finding {
                file: format!("{member}/Cargo.toml"),
                line: 1,
                what: format!(
                    "`{member}` can run a statement on the local database, and the source \
                     gate over local SQL does not read it"
                ),
                fix: "its manifest takes `sqlx` with a SQLite feature, or a crate that hands \
                      over the file — add the sources that run its statements to `WRITERS` in \
                      xtask/src/local.rs, or name the member in `NOT_READ` there with the \
                      reason it is not read"
                    .to_owned(),
            });
        }
    }
    for (name, _) in NOT_READ {
        if !opening.iter().any(|member| member == name) {
            findings.push(Finding {
                file: "xtask/src/local.rs".to_owned(),
                line: 1,
                what: format!(
                    "`{name}` is named in `NOT_READ` and its manifest no longer opens the local \
                     database"
                ),
                fix: "remove it from `NOT_READ`: a reason that outlived what it excused is \
                      one the next member that needs one can hide behind"
                    .to_owned(),
            });
        }
    }
    findings
}

/// The refused statements in one file, a finding for each.
///
/// **Its `#[cfg(test)]` modules are not read**, as its `tests/` are not: no
/// test runs against a user's file, and the pruner's own tests hold the SQL of
/// triggers and virtual tables as text, to prove the reader that tells them
/// apart.
fn refused_in(file: &str, text: &str) -> Vec<Finding> {
    let code = crate::gates::without_comments(&crate::gates::outside_the_tests(text), "//");
    let words = words(&code);
    let mut findings = Vec::new();
    for refused in &REFUSED {
        for line in (refused.holds)(&code, &words) {
            findings.push(Finding {
                file: file.to_owned(),
                line,
                what: format!("{} is written here", refused.what),
                fix: refused.fix.to_owned(),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::{Scratch, refused_in};

    /// This repository, which the gates read.
    fn root() -> &'static std::path::Path {
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// Each statement the gate refuses, as written in a source, and the words
    /// of the finding that names it.
    const REFUSED_CASES: [(&str, &str); 109] = [
        // A migrator or a migration built when the store runs — or the one
        // static, embedding another directory.
        (
            "static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!(\"../../elsewhere\")",
            "migrator",
        ),
        (
            "let found = Migrator::new(Path::new(\"../../local-migrations\")).await",
            "migrator",
        ),
        (
            "sqlx::migrate::Migrator::with_migrations(found)",
            "migrator",
        ),
        ("let built = Migrator { migrations, ..old }", "migrator"),
        ("use sqlx::migrate::Migrator as Plain", "migrator"),
        (
            "let source: &dyn sqlx::migrate::MigrationSource<'_> = &dir",
            "migrator",
        ),
        ("let found = ResolveWith(dir, config)", "migrator"),
        ("let found = resolve_blocking(dir, &config)", "migrator"),
        ("let embedded = sqlx::migrate!(\"./elsewhere\")", "migrator"),
        (
            "let one = sqlx::migrate::Migration::new(1, d, t, sql, false)",
            "migrator",
        ),
        ("sqlx::query(\"ATTACH DATABASE 'x' AS other\")", "ATTACH"),
        ("sqlx::query(\"detach other\")", "DETACH"),
        (".pragma(\"writable_schema\", \"ON\")", "writable_schema"),
        ("sqlx::query(\"PRAGMA journal_mode = OFF\")", "journal_mode"),
        (".pragma(\"journal_mode\", \"off\")", "journal_mode"),
        (".journal_mode(SqliteJournalMode::Off)", "journal_mode"),
        ("sqlx::query(\"SELECT load_extension('x')\")", "extension"),
        ("unsafe { options.extension(\"x\") }", "extension"),
        (
            "options.extension_with_entrypoint(\"x\", \"y\")",
            "extension",
        ),
        (
            "sqlx::query(\"CREATE TABLE main.t (x)\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"create temp trigger t\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"CREATE UNIQUE INDEX i ON t (x)\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"DROP TRIGGER main.t\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"ALTER TABLE main.t ADD COLUMN y\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"CREATE VIEW v AS SELECT 1\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"CREATE \\\n    TABLE t (x)\")",
            "creates, drops or alters",
        ),
        // One word split by a continuation is one word to SQLite.
        (
            "sqlx::query(\"DROP TRIG\\\n    GER t\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"ATT\\\r\n\t  ACH DATABASE 'x' AS other\")",
            "ATTACH",
        ),
        // Each writes `sqlite_stat1`, which the self-check refuses at the
        // next open: measured with all three in `crates/nc-kernel/src`.
        ("sqlx::query(\"PRAGMA optimize\")", "ANALYZE"),
        ("sqlx::query(\"PRAGMA optimize=0x10002\")", "ANALYZE"),
        ("sqlx::query(\"analyze\")", "ANALYZE"),
        (".pragma(\"optimize\", \"\")", "ANALYZE"),
        (".optimize_on_close(true, None)", "ANALYZE"),
        (".pragma(\"analysis_limit\", \"400\")", "ANALYZE"),
        // A SQL comment inside the string is a gap and not a word: read as a
        // word, each of these passes such a gate, measured, and SQLite runs the
        // first two.
        (
            "sqlx::query(\"DROP /* x */ TRIGGER t\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"PRAGMA journal_mode = /* x */ OFF\")",
            "journal_mode",
        ),
        (
            "sqlx::query(\"CREATE -- x\\n TABLE t (x)\")",
            "creates, drops or alters",
        ),
        // A raw string's comment may hold a quote, and still ends where
        // SQLite ends it.
        (
            "sqlx::query(r#\"DROP /* \"x\" */ TRIGGER t\"#)",
            "creates, drops or alters",
        ),
        // `--` inside a SQL string is the string's, so what follows it runs.
        (
            "sqlx::query(\"INSERT INTO t VALUES ('a -- b'); DROP TABLE t\")",
            "creates, drops or alters",
        ),
        // A character literal holding a quote opens no string, so the
        // literal after it is still read as one.
        (
            "let quote = '\"'; sqlx::query(\"DROP /* x */ TRIGGER t\")",
            "creates, drops or alters",
        ),
        // An escape is the character it stands for: `\\x20` is a space.
        (
            "sqlx::query(\"DROP\\x20TRIGGER t\")",
            "creates, drops or alters",
        ),
        (
            "sqlx::query(\"PRAGMA journal_mode = 'OFF'\")",
            "journal_mode",
        ),
        // **The rest of the connection's hardening**: each of these, written
        // in a shipped kernel source, passes a gate that does not name it, with
        // `ok` printed.
        (
            "sqlx::query(\"PRAGMA main.journal_mode = MEMORY\")",
            "journal_mode",
        ),
        (".journal_mode(SqliteJournalMode::Delete)", "journal_mode"),
        (
            "sqlx::query(\"PRAGMA main.synchronous = OFF\")",
            "synchronous",
        ),
        (".synchronous(SqliteSynchronous::Normal)", "synchronous"),
        (".pragma(\"synchronous\", \"1\")", "synchronous"),
        (
            "sqlx::query(\"PRAGMA trusted_schema = ON\")",
            "trusted_schema",
        ),
        (".pragma(\"trusted_schema\", \"1\")", "trusted_schema"),
        ("sqlx::query(\"PRAGMA foreign_keys = OFF\")", "foreign_keys"),
        (".foreign_keys(false)", "foreign_keys"),
        // **Every spelling SQLite reads as a weaker setting**, which a list of
        // the weakening values lets through: measured on the store's own
        // pool, `ON`, `YES`, `TRUE` and `01` leave `synchronous`
        // at `NORMAL`, `NO` and `FALSE` at `OFF`, and a word SQLite does not
        // know at `NORMAL` too; `trusted_schema = 2` turns it on, and
        // `foreign_keys = NONE` or `full` turns them off.
        ("sqlx::query(\"PRAGMA synchronous = ON\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = YES\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = TRUE\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = 01\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = NO\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = FALSE\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous = FAST\")", "synchronous"),
        ("sqlx::query(\"PRAGMA synchronous(1)\")", "synchronous"),
        (
            "sqlx::query(\"PRAGMA trusted_schema = 2\")",
            "trusted_schema",
        ),
        (
            "sqlx::query(\"PRAGMA foreign_keys = NONE\")",
            "foreign_keys",
        ),
        (".pragma(\"foreign_keys\", \"full\")", "foreign_keys"),
        // A type imported under another name is read by the value it hands.
        (".synchronous(Durability::Normal)", "synchronous"),
        (".journal_mode(Journal::Memory)", "journal_mode"),
        // The pragma door called as a path, and handed a value no literal
        // holds, which nobody can say keeps the hardening.
        (
            "SqliteConnectOptions::pragma(options, \"synchronous\", \"NORMAL\")",
            "synchronous",
        ),
        (".pragma(\"synchronous\", level)", "synchronous"),
        // **The rest of what `crates/nc-local` hardens the connection
        // with**: each of these passes a gate that refuses only some of it.
        (
            "sqlx::query(\"PRAGMA cell_size_check = OFF\")",
            "cell_size_check",
        ),
        (".pragma(\"cell_size_check\", \"OFF\")", "cell_size_check"),
        (
            "sqlx::query(\"PRAGMA main.temp_store = FILE\")",
            "temp_store",
        ),
        (".pragma(\"temp_store\", \"DEFAULT\")", "temp_store"),
        ("sqlx::query(\"PRAGMA fullfsync = OFF\")", "fullfsync"),
        (
            "sqlx::query(\"PRAGMA ignore_check_constraints = ON\")",
            "ignore_check_constraints",
        ),
        // **Each spelling SQLite reads past a narrower reader**, measured on
        // the store's own pool: a schema's name before the
        // door's key or quotes around it, a sign SQLite's parser reads away,
        // hexadecimal, a quoted value, a `;` after it; `sqlx`'s own setters
        // called as a path, or handed a value that is not written out; a key
        // no literal is all of; and a statement that copies the file out.
        (
            ".pragma(\"main.ignore_check_constraints\", \"ON\")",
            "ignore_check_constraints",
        ),
        (
            ".pragma(\"\\\"trusted_schema\\\"\", \"ON\")",
            "trusted_schema",
        ),
        (".pragma(\"main.trusted_schema\", \"ON\")", "trusted_schema"),
        (".pragma(\"main.foreign_keys\", \"OFF\")", "foreign_keys"),
        (".pragma(\"main.journal_mode\", \"OFF\")", "journal_mode"),
        (".pragma(\"main.synchronous\", \"OFF\")", "synchronous"),
        (".pragma(\"trusted_schema\", \"+1\")", "trusted_schema"),
        (".pragma(\"trusted_schema\", \"0x1\")", "trusted_schema"),
        (".pragma(\"trusted_schema\", \"'ON'\")", "trusted_schema"),
        (".pragma(\"trusted_schema\", \"ON;\")", "trusted_schema"),
        (
            "sqlx::query(\"PRAGMA trusted_schema = +1\")",
            "trusted_schema",
        ),
        (
            "sqlx::query(\"PRAGMA trusted_schema = 0x1\")",
            "trusted_schema",
        ),
        (
            "sqlx::query(\"PRAGMA ignore_check_constraints = +1\")",
            "ignore_check_constraints",
        ),
        (
            "SqliteConnectOptions::foreign_keys(options, false)",
            "foreign_keys",
        ),
        (
            "SqliteConnectOptions::journal_mode(options, SqliteJournalMode::Off)",
            "journal_mode",
        ),
        (
            "SqliteConnectOptions::synchronous(options, SqliteSynchronous::Off)",
            "synchronous",
        ),
        (".foreign_keys(!true)", "foreign_keys"),
        (
            ".journal_mode(if cold { SqliteJournalMode::Off } else { SqliteJournalMode::Wal })",
            "journal_mode",
        ),
        (".map(SqliteConnectOptions::foreign_keys)", "foreign_keys"),
        (
            ".pragma(stringify!(trusted_schema), \"ON\")",
            "no literal reads as one pragma",
        ),
        (
            ".pragma(concat!(\"trusted\", \"_schema\"), \"ON\")",
            "no literal reads as one pragma",
        ),
        (
            ".pragma(env!(\"NC_PRAGMA_KEY\"), \"ON\")",
            "no literal reads as one pragma",
        ),
        (
            ".pragma(include_str!(\"key.txt\"), \"ON\")",
            "no literal reads as one pragma",
        ),
        ("sqlx::query(\"VACUUM INTO '/tmp/copy.sqlite3'\")", "VACUUM"),
        ("sqlx::query(\"vacuum\")", "VACUUM"),
        (
            ".pragma(\"cache_size = 0; PRAGMA x\", \"1\")",
            "no literal reads as one pragma",
        ),
        (
            ".pragma(\"temp_store_directory\", \"'/tmp'\")",
            "temp_store_directory",
        ),
        // **What the file says it is**, which the store refuses at every
        // open once it is rewritten: a gate that does not name them lets each
        // of these pass.
        (
            "sqlx::query(\"PRAGMA main.application_id = 0\")",
            "application_id",
        ),
        (".pragma(\"application_id\", \"0\")", "application_id"),
        (
            "sqlx::query(\"PRAGMA schema_version = 1\")",
            "schema_version",
        ),
        (".pragma(\"main.schema_version\", \"7\")", "schema_version"),
        (
            "sqlx::query(\"DELETE FROM main._sqlx_migrations WHERE version = 3\")",
            "_sqlx_migrations",
        ),
        (
            "sqlx::query(\"UPDATE main._sqlx_migrations SET checksum = x'00'\")",
            "_sqlx_migrations",
        ),
        (
            "sqlx::query(\"INSERT INTO main._sqlx_migrations (version) VALUES (9)\")",
            "_sqlx_migrations",
        ),
        (
            "sqlx::query(\"INSERT OR REPLACE INTO \\\"_sqlx_migrations\\\" VALUES (9)\")",
            "_sqlx_migrations",
        ),
        (
            "sqlx::query(\"UPDATE OR IGNORE _sqlx_migrations SET success = 0\")",
            "_sqlx_migrations",
        ),
        (
            "sqlx::query(\"REPLACE INTO [_sqlx_migrations] VALUES (9)\")",
            "_sqlx_migrations",
        ),
    ];

    /// Sources the gate reads and refuses nothing in: comments, names that
    /// begin with a refused word, and statements that only look like one.
    const ALLOWED_CASES: [&str; 41] = [
        // The one migrator, the static that runs it, the store's error that
        // names a migration, and the error type the migrator answers with.
        "static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!(\"../../local-migrations\");",
        "fn f() { MIGRATOR.run_direct(None, &mut *connection, false); }",
        "fn f() -> LocalError { LocalError::Migration(String::new()) }",
        "fn migrated(error: &sqlx::migrate::MigrateError) {}",
        "// never ATTACH, never writable_schema, never CREATE TABLE here",
        "/// `DROP TRIGGER` is what a same-user process can do",
        ".journal_mode(SqliteJournalMode::Wal)",
        // What the connection options set, and a pragma read rather than set.
        ".synchronous(SqliteSynchronous::Full).foreign_keys(true)",
        ".pragma(\"trusted_schema\", \"OFF\")",
        "sqlx::query(\"PRAGMA main.journal_mode\")",
        "sqlx::query(\"PRAGMA synchronous = FULL\")",
        "std::fs::OpenOptions::new().create_new(true)",
        "let created = true; let dropped_table = 1;",
        "sqlx::query(\"SELECT count(*) FROM temp.sqlite_schema\")",
        "\"the key was created by another unit\"",
        "let kind = path.extension();",
        "if path.extension( ).is_some() {}",
        // An escaped backslash ends the line and continues nothing: SQLite
        // is handed `TRIG\`, a newline and `GER`, which build nothing.
        "sqlx::query(\"DROP TRIG\\\\\nGER t\")",
        // A word that begins with one is a name, not the statement.
        "let optimized = analyze_layout(); // never `PRAGMA optimize` here",
        // A comment's words are the comment's: this sets nothing off, and a
        // reader taking them as words refuses it as though it did.
        "sqlx::query(\"PRAGMA journal_mode = /* never off */ WAL\")",
        "sqlx::query(\"SELECT 1 -- and never DROP TABLE t\")",
        "sqlx::query(\"SELECT 1 /* ATTACH */\")",
        // A continued line is no newline to SQLite, so the comment runs on
        // past it and what follows is the comment's too.
        "sqlx::query(\"SELECT 1 -- x\\\n    DROP TABLE t\")",
        // A test module is not read: no test runs against a user's file.
        concat!(
            "#[cfg(",
            "test)]\nmod tests {\n    const T: &str = \"CREATE TABLE t (x)\";\n}"
        ),
        // What SQLite reads as the hardening is the hardening, however
        // spelt, and a pragma read or one outside the hardening sets nothing.
        "sqlx::query(\"PRAGMA synchronous = EXTRA\")",
        "sqlx::query(\"PRAGMA synchronous = 2\")",
        "sqlx::query(\"PRAGMA foreign_keys = YES\")",
        ".pragma(\"temp_store\", \"MEMORY\")",
        ".pragma(\"cell_size_check\", \"ON\")",
        ".pragma(\"fullfsync\", \"ON\")",
        "sqlx::query(\"PRAGMA temp_store\")",
        ".pragma(\"cache_size\", \"-2000\")",
        // Read through a schema's name, a path to the setter or a value
        // written out in full, what keeps the hardening still keeps it.
        ".pragma(\"main.trusted_schema\", \"OFF\")",
        ".pragma(\"trusted_schema\", \"0\")",
        "sqlx::query(\"PRAGMA ignore_check_constraints = NO\")",
        "SqliteConnectOptions::foreign_keys(options, true)",
        ".journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)",
        // A sign SQLite's parser reads away: on, as the hardening keeps it.
        "sqlx::query(\"PRAGMA foreign_keys = +1\")",
        // What the store's self-check reads of the header and the history.
        "sqlx::query_scalar(\"PRAGMA main.application_id\")",
        "sqlx::query(\"SELECT version, success, checksum FROM main._sqlx_migrations ORDER BY version\")",
        "if held.name == \"_sqlx_migrations\" {}",
    ];

    /// Each statement the gate names is refused, and a comment naming one is not.
    #[test]
    fn each_statement_the_gate_names_is_refused_and_a_comment_is_not() {
        for (written, what) in REFUSED_CASES {
            let findings = refused_in(
                "crates/nc-local/src/x.rs",
                &format!("fn f() {{ {written}; }}\n"),
            );
            assert_eq!(findings.len(), 1, "{written}: {findings:?}");
            assert!(
                findings[0].what.contains(what),
                "{written}: {}",
                findings[0].what
            );
            assert_eq!(findings[0].line, 1);
        }
        for allowed in ALLOWED_CASES {
            assert!(
                refused_in("crates/nc-local/src/x.rs", &format!("{allowed}\n")).is_empty(),
                "{allowed} was refused"
            );
        }
    }

    /// Every pragma the store's connection options set is one the gate guards,
    /// at a value the gate lets through, and every pragma it guards is one they
    /// set or one SQLite's default keeps.
    ///
    /// **Every pragma the connection hardens, not four of them**, measured:
    /// with a list of four, turning `cell_size_check` or `temp_store` back,
    /// `fullfsync` off or `ignore_check_constraints` on passes the gate, which
    /// then names as the connection's hardening only what it refuses.
    #[test]
    fn every_pragma_the_store_sets_is_one_the_gate_guards() {
        let open = std::fs::read_to_string(root().join("crates/nc-local/src/open.rs"))
            .expect("the store's options are readable");
        let code = crate::gates::without_comments(&crate::gates::outside_the_tests(&open), "//");
        let mut set = std::collections::BTreeSet::new();
        for setting in super::settings(&code) {
            let hardened = super::HARDENED
                .iter()
                .find(|hardened| hardened.name == setting.name)
                .unwrap_or_else(|| {
                    panic!("{setting:?} is set by the store and guarded by no rule")
                });
            assert!(
                setting.value.as_deref().is_some_and(hardened.keeps),
                "{setting:?}: the store sets what the gate refuses"
            );
            set.insert(setting.name);
        }
        let guarded: std::collections::BTreeSet<String> = super::HARDENED
            .iter()
            .map(|hardened| hardened.name.to_owned())
            .filter(|name| !super::KEPT_AT_THEIR_DEFAULT.contains(&name.as_str()))
            .collect();
        assert_eq!(set, guarded, "what the store sets and what the gate guards");
    }

    /// A `#[cfg(test)]` match arm or struct-literal field hides itself and
    /// nothing after it, and a test-only function is cut whole however its
    /// parameters wrap.
    ///
    /// **The shape it holds is a real one**: a test region that runs on to the
    /// brace closing the `match` would skip a production arm behind a test-only
    /// arm, so a shipped kernel source returning `DROP TRIGGER` there would pass
    /// with the gate printing `ok`.
    #[test]
    fn a_test_only_arm_or_field_hides_nothing_after_it() {
        let attribute = concat!("#[cfg(", "test)]");
        for source in [
            format!(
                "fn f(step: u8) -> &'static str {{\n    match step {{\n        {attribute}\n        \
                 0 => \"SELECT 1\",\n        1 => \"DROP TRIGGER main.t\",\n        _ => \
                 \"SELECT 2\",\n    }}\n}}\n"
            ),
            format!(
                "fn f() -> S {{\n    S {{\n        {attribute}\n        probe: \"SELECT 1\",\n        \
                 statement: \"DROP TRIGGER main.t\",\n    }}\n}}\n"
            ),
        ] {
            let findings = refused_in("crates/nc-local/src/x.rs", &source);
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| finding.line)
                    .collect::<Vec<_>>(),
                [5],
                "{source}"
            );
        }
        for hidden in [
            format!(
                "fn f(step: u8) -> &'static str {{\n    match step {{\n        {attribute}\n        \
                 0 => \"DROP TRIGGER main.t\",\n        _ => \"SELECT 2\",\n    }}\n}}\n"
            ),
            format!(
                "{attribute}\nfn only(\n    step: u8,\n) -> &'static str {{\n    \
                 \"DROP TRIGGER main.t\"\n}}\n"
            ),
        ] {
            assert!(
                refused_in("crates/nc-local/src/x.rs", &hidden).is_empty(),
                "what a `#[cfg(test)]` attribute governs was read: {hidden}"
            );
        }
    }

    /// The crates that open the local database write none of it.
    #[test]
    fn the_crates_that_open_the_local_database_write_no_schema() {
        let (findings, read) =
            super::no_local_sql_outside_the_migrations(root()).expect("the trees are read");
        assert!(findings.is_empty(), "{findings:?}");
        let on_disk: usize = super::WRITERS
            .iter()
            .map(|named| {
                let path = root().join(named);
                if path.is_file() {
                    return 1;
                }
                let mut count = 0;
                crate::gates::walk(&path, &mut |path| {
                    count += usize::from(path.extension().is_some_and(|kind| kind == "rs"));
                })
                .expect("walkable");
                count
            })
            .sum();
        assert_eq!(read, on_disk);
        assert!(read >= 5, "only {read} file(s) were read");
    }

    /// Every member whose manifest opens the local database is read by the
    /// gate or named as not, and a name that no longer opens it is a finding.
    ///
    /// **The pruner's SQLite target is inside the gate**: left out, `PRAGMA
    /// optimize` and a `DROP TRIGGER` planted in `crates/nc-prune/src/sqlite.rs`
    /// pass it, measured, while that file runs its statements on the user's
    /// database through `nc-local`'s pool. A list of directories written by
    /// hand drifts from the manifests, which name every member that can reach
    /// the file.
    #[test]
    fn every_member_that_opens_the_local_database_is_read_or_named() {
        let members = |paths: &[&str]| -> Vec<String> {
            paths.iter().map(|path| (*path).to_owned()).collect()
        };
        let this_tree = crate::members::opening_the_local_database(root()).expect("cargo metadata");
        let read = |opening: &[String]| super::unread_writers(opening, &super::WRITERS);
        assert!(read(&this_tree).is_empty(), "{:?}", read(&this_tree));
        assert_eq!(
            read(&members(&["crates/nc-local", "crates/nc-prune", "xtask"])),
            [] as [crate::gates::Finding; 0]
        );
        let unread = read(&members(&["crates/nc-local", "crates/nc-relay", "xtask"]));
        assert_eq!(unread.len(), 1, "{unread:?}");
        assert_eq!(unread[0].file, "crates/nc-relay/Cargo.toml");
        assert!(unread[0].what.contains("does not read it"), "{unread:?}");
        // A directory is not its prefix: a member beside `crates/nc-local`
        // whose name only begins with it is not read because that one is.
        let prefixed = read(&members(&["crates/nc-local-extra", "xtask"]));
        assert_eq!(prefixed.len(), 1, "{prefixed:?}");
        // **One file of a member reads none of the rest of it**: read that way,
        // a `DROP TRIGGER` in the pruner's `lib.rs` passes beside its
        // `sqlite.rs`, the member counted as read.
        let one_file = super::unread_writers(
            &members(&["crates/nc-local", "crates/nc-prune", "xtask"]),
            &["crates/nc-local/src", "crates/nc-prune/src/sqlite.rs"],
        );
        assert_eq!(one_file.len(), 1, "{one_file:?}");
        assert_eq!(one_file[0].file, "crates/nc-prune/Cargo.toml");
        // The other way: a name excused that no longer opens the file.
        let stale = read(&members(&["crates/nc-local"]));
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(
            stale[0].what.contains("`xtask` is named in `NOT_READ`"),
            "{stale:?}"
        );
    }

    /// A migration that builds an object the list dates to another migration
    /// is refused, though the schema it ends with is the listed one.
    ///
    /// **The whole schema built at once cannot see it**, and `crates/nc-local`
    /// holds a file one migration behind to what its history built: an object
    /// dated wrong is a kernel that refuses every file the build before it
    /// left. Here `blobs` moves into the first migration and the second builds
    /// nothing, so the end is identical and only the first step differs.
    #[test]
    fn an_object_the_list_dates_to_another_migration_is_refused() {
        let moved = edited("moved", |into| {
            let blobs = std::fs::read_to_string(into.join("0002_blobs.sql")).expect("readable");
            let first =
                std::fs::read_to_string(into.join("0001_plugin_store.sql")).expect("readable");
            std::fs::write(
                into.join("0001_plugin_store.sql"),
                format!("{first}\n{blobs}"),
            )
            .expect("writable");
            std::fs::write(into.join("0002_blobs.sql"), "SELECT 1;\n").expect("writable");
        });
        let refused = super::the_local_schema(&moved.0).expect_err("blobs dated to 0002");
        assert!(
            refused.contains("up to 1 does not build what")
                && refused.contains("the table `blobs` on `blobs` is not one"),
            "{refused}"
        );
    }

    /// The migrations this repository ships build exactly the listed schema.
    #[test]
    fn the_local_migrations_build_exactly_the_listed_schema() {
        let built = super::the_local_schema(root()).expect("the shipped migrations");
        assert_eq!(built.objects, nc_local::schema::EXPECTED.len());
        assert_eq!(built.migrations, 4);
        assert!(built.sqlite.starts_with("3."), "{}", built.sqlite);
    }

    /// A SQLite below the floor is refused, naming the version linked and the
    /// advisory the floor exists for.
    ///
    /// **A floor above every release there is**, because the SQLite linked is
    /// the lockfile's and no test can link an older one. The control is the
    /// shipped floor over the same directory, which builds.
    #[test]
    fn a_sqlite_below_the_floor_is_refused_naming_both() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let migrations = root().join(crate::limits::LOCAL_MIGRATIONS);
        let scratch = Scratch::new("floor").expect("a scratch directory this run made");
        let refused = runtime
            .block_on(super::built_and_judged(
                &migrations,
                &scratch.0.join("local.sqlite3"),
                (99, 0, 0),
            ))
            .expect_err("no SQLite is 99.0.0");
        let built = runtime
            .block_on(super::built_and_judged(
                &migrations,
                &scratch.0.join("control.sqlite3"),
                nc_local::schema::MINIMUM_SQLITE,
            ))
            .expect("the shipped floor");
        assert!(
            refused.contains(&format!(
                "this build links SQLite {}, below 99.0.0",
                built.sqlite
            )) && refused.contains("CVE-2025-6965"),
            "{refused}"
        );
    }

    /// A scratch directory that already exists is refused, and nothing is
    /// opened in it.
    ///
    /// **Found where the gate would make its own**, as a directory another
    /// account made first in a temporary directory every account shares would
    /// be, holding a file under the name the gate opens. It is refused naming
    /// the path, and left exactly as it was found: not used, so SQLite never
    /// reads the file or makes a companion beside it, and not removed, since
    /// what a `Scratch` removes is what this process made.
    #[test]
    fn a_scratch_directory_that_already_exists_is_refused_and_nothing_is_opened_in_it() {
        /// Each entry of `directory` with its bytes, by name.
        fn what_it_holds(directory: &std::path::Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
            let mut held: Vec<_> = std::fs::read_dir(directory)
                .expect("readable")
                .map(|entry| {
                    let entry = entry.expect("an entry");
                    (
                        entry.file_name(),
                        std::fs::read(entry.path()).expect("a file"),
                    )
                })
                .collect();
            held.sort();
            held
        }
        let held = Scratch::new("found").expect("a scratch directory this run made");
        let found = held.0.join("found");
        std::fs::create_dir(&found).expect("a directory made before the gate's");
        std::fs::write(found.join("local.sqlite3"), b"another account's database")
            .expect("writable");
        let before = what_it_holds(&found);
        let refused = Scratch::at(found.clone()).expect_err("a directory already there");
        assert!(
            refused.contains(&format!("`{}` was already there", found.display())),
            "{refused}"
        );
        assert_eq!(
            what_it_holds(&found),
            before,
            "a directory the gate did not make was opened, written or removed"
        );
    }

    /// The scratch directory the gate makes is this user's alone.
    ///
    /// **Judged as the local database's own directory is**, by
    /// `nc_platform::user_in`: no group or other bit on Unix, and on Windows an
    /// access list every entry of which is this user. Made with the umask's
    /// mode instead, it is `0755` under the common `022` — another account
    /// reads the gate's file — and on a host whose umask lets the group write,
    /// a directory that group can put a `-wal` in before the gate opens it.
    #[test]
    fn the_scratch_directory_the_gate_makes_is_this_users_alone() {
        let made = Scratch::new("private").expect("a scratch directory this run made");
        if let Err(judged) = nc_platform::user_in(&made.0) {
            panic!("the gate's scratch directory is not this user's alone: {judged}");
        }
    }

    /// A copy of `local-migrations/`, edited, as the root of a scratch tree.
    fn edited(name: &str, edit: impl Fn(&std::path::Path)) -> Scratch {
        let scratch = Scratch::new(name).expect("a scratch directory this run made");
        let into = scratch.0.join(crate::limits::LOCAL_MIGRATIONS);
        std::fs::create_dir(&into).expect("a directory");
        for entry in
            std::fs::read_dir(root().join(crate::limits::LOCAL_MIGRATIONS)).expect("readable")
        {
            let entry = entry.expect("an entry");
            std::fs::copy(entry.path(), into.join(entry.file_name())).expect("copied");
        }
        edit(&into);
        scratch
    }

    /// Replaces `from` in a copied migration, refusing a needle that drifted.
    fn rewrite(path: &std::path::Path, from: &str, to: &str) {
        let text = std::fs::read_to_string(path).expect("readable");
        assert!(
            text.contains(from),
            "{} no longer holds `{from}`",
            path.display()
        );
        std::fs::write(path, text.replace(from, to)).expect("writable");
    }

    /// A migration that builds something the list does not say is refused,
    /// naming the object.
    #[test]
    fn a_migration_that_differs_from_the_list_is_refused() {
        let dropped = edited("dropped", |into| {
            rewrite(
                &into.join("0001_plugin_store.sql"),
                "CREATE TRIGGER store_writes_are_never_edited",
                "CREATE TRIGGER store_writes_are_seldom_edited",
            );
        });
        let refused = super::the_local_schema(&dropped.0).expect_err("a renamed trigger");
        assert!(
            refused.contains("the trigger `store_writes_are_never_edited` is missing")
                && refused.contains("`store_writes_are_seldom_edited`"),
            "{refused}"
        );

        let weakened = edited("weakened", |into| {
            rewrite(
                &into.join("0001_plugin_store.sql"),
                "SELECT RAISE(ABORT, 'store_notes: a unit''s notes are never edited');",
                "SELECT 1;",
            );
        });
        let refused =
            super::the_local_schema(&weakened.0).expect_err("a trigger that does nothing");
        assert!(
            refused.contains("the trigger `store_notes_are_never_edited` is not the one"),
            "{refused}"
        );

        let added = edited("added", |into| {
            std::fs::write(
                into.join("0005_more.sql"),
                "CREATE TABLE extra (id INTEGER PRIMARY KEY);\n",
            )
            .expect("written");
        });
        let refused = super::the_local_schema(&added.0).expect_err("a table nobody listed");
        assert!(refused.contains("the table `extra`"), "{refused}");
    }

    /// A header without this platform's number is refused.
    #[test]
    fn a_file_without_this_platforms_header_is_refused() {
        let unstamped = edited("unstamped", |into| {
            rewrite(
                &into.join("0001_plugin_store.sql"),
                "PRAGMA application_id = 1313033265;",
                "PRAGMA application_id = 7;",
            );
        });
        let refused = super::the_local_schema(&unstamped.0).expect_err("another header");
        assert!(refused.contains("carries application 7"), "{refused}");
    }

    /// A row whose foreign key names nothing is refused.
    #[test]
    fn a_row_whose_foreign_key_names_nothing_is_refused() {
        let orphaned = edited("orphaned", |into| {
            std::fs::write(
                into.join("0005_an_orphan.sql"),
                "-- no-transaction\nPRAGMA foreign_keys = OFF;\n\
                 INSERT INTO store_notes (unit, seq, note) VALUES (1, 0, '1');\n",
            )
            .expect("written");
        });
        let refused = super::the_local_schema(&orphaned.0).expect_err("an orphan");
        assert!(refused.contains("foreign_key_check"), "{refused}");
    }

    /// A directory that does not apply is refused, and says so.
    #[test]
    fn a_migration_that_does_not_apply_is_refused() {
        let broken = edited("broken", |into| {
            std::fs::write(
                into.join("0005_broken.sql"),
                "CREATE TABLE store_units (x);\n",
            )
            .expect("written");
        });
        let refused = super::the_local_schema(&broken.0).expect_err("a table twice");
        assert!(
            refused.contains("does not apply to an empty SQLite file"),
            "{refused}"
        );
    }
}
