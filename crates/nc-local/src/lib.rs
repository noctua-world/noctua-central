//! The local app database: one SQLite file, private to the user who runs the
//! kernel (`NOCTUA.md §4`, "Local app database"; `§11.1`).
//!
//! [`LocalStore::open`] takes a directory — where it lives is the daemon's to
//! decide, in Stage 13 — and hands back the plugin store: what each plugin keeps
//! (`store_entries`), the units that wrote it with their before and after images
//! (`store_units`, `store_writes`, `store_notes`), and which blobs this machine
//! holds (`blobs`). The schema is `local-migrations/`, applied by the same
//! `sqlx` migrator the server uses, and it is never mixed with `migrations/`.
//! The bytes of those blobs are not in the file: they are files of their own,
//! in a directory [`blob_directory`] holds to the rule this crate's own
//! directory is held to.
//!
//! # What opening does, in order
//!
//! 1. **The directory is private.** Created `0700` when it is missing; when it
//!    exists it must be a directory and not a link to one, owned by this
//!    process's user, with no group or other bits and every owner bit. Anything
//!    else is refused — [`LocalError::Refused`], `NC-DB-0006`, whose message
//!    says the folder or the files are not set up for this user alone to read
//!    and write — and nothing is created in it. A path the filesystem cannot
//!    answer for — a parent that does not exist, a full disk, a read-only
//!    mount — is [`LocalError::Unavailable`], `NC-DB-0001`: nothing about it is
//!    how the folder was set up. **Then it is held for this open**, under an
//!    exclusive lock every other open of the profile waits on until this one
//!    has migrated the file: SQLite refuses two connections turning a new
//!    file's journal into a write-ahead log at once, and `sqlx` takes no lock
//!    around a migration.
//! 2. **The file is created by this crate, `0600`, before SQLite sees it.**
//!    SQLite would create it `0644` less the umask, and its `-wal` and `-shm`
//!    companions take the database file's mode rather than the umask, so the
//!    mode of all three is decided here. `create_new` is `O_CREAT | O_EXCL`,
//!    which does not follow a link at the last component. A file that exists is
//!    held to the same owner, mode and not-a-link rule — read and write for its
//!    owner, nothing for anybody else — and so are the companions when they are
//!    there.
//! 3. **The pragmas are the connection's, not the schema's**, set through
//!    `SqliteConnectOptions` on every connection the pool opens:
//!    `journal_mode = WAL`, `synchronous = FULL`, `foreign_keys = ON`,
//!    `trusted_schema = OFF`, `cell_size_check = ON`, `temp_store = MEMORY`,
//!    and `fullfsync = ON` on macOS. **The file is judged first, on a
//!    connection of its own, read-only, that sets all of them but
//!    `journal_mode`**, the one that writes the file: it converts a database
//!    in another journal mode on the first connection that asks, and set on
//!    the judging one it would convert another application's file, measured,
//!    before refusing it as another application's. Read-only because a
//!    connection that may write writes on its own as well — the last to close
//!    a file checkpoints its `-wal` into it, and the first to read one rolls a
//!    hot journal back — and a hot journal, which this build never leaves, is
//!    `NC-DB-0007`.
//! 4. **The file says whose it is.** Its header's `application_id` must be this
//!    platform's before anything is migrated — or zero on a file that holds no
//!    table yet, which is one this crate just created. `0001_plugin_store.sql`
//!    writes the number in the transaction that builds the tables, so a file
//!    has both or neither. Another application's file is
//!    [`LocalError::Unexpected`], `NC-DB-0007`, and is left as it is.
//! 5. **The self-check, before anything is written** ([`schema`]): the SQLite
//!    this was linked with, `quick_check`, no temporary object on the
//!    connection, a history of migrations that is a beginning of this build's,
//!    and every object in `sqlite_schema` compared with what that history
//!    built — the SQL of each table, index and trigger by its BLAKE3. A
//!    damaged page, a dropped trigger, an extra table or an edited one is
//!    [`LocalError::Unexpected`], `NC-DB-0007`, and the file is left as it was,
//!    which is what that code's message tells the user.
//! 6. **`sqlx::migrate!("../../local-migrations")`**, then the self-check
//!    again over the whole of [`schema::EXPECTED`], with the header. The store
//!    is handed out only when both pass. `cargo xtask check` builds a scratch
//!    database from the files on disk one migration at a time, and holds each
//!    step to the same list.
//!
//! # `synchronous = FULL`, and what it costs
//!
//! `FULL` syncs the WAL on every commit; `NORMAL`, which Zed and Signal Desktop
//! use, syncs only at a checkpoint and can lose the last commits to a power cut
//! — never corrupt, but lose. A unit the kernel told a plugin was committed and
//! then closed in the journal must still be there, so this is `FULL`. Measured
//! on this machine (ZFS, `sync=standard`, so the fsyncs are real): a
//! single-row commit takes a median of 0.26–0.33 ms under `FULL` and
//! 0.03–0.05 ms under `NORMAL`, measured as 200 single-row autocommit `INSERT`s
//! into a WAL database under each setting, timed one by one, median taken. That
//! procedure is what to repeat on another disk, because the number is the
//! disk's and not this crate's.
//!
//! # Every statement names `main.`, and every write counts what it changed
//!
//! A `TEMP` table shadows a `main` table of the same name for unqualified
//! names on the connection that made it, so every statement here writes
//! `main.`. That does not stop a `TEMP` *trigger* on a `main` table: measured,
//! `BEFORE UPDATE … RAISE(IGNORE)` turns the append-only refusal into success
//! with no row changed, and `BEFORE INSERT … RAISE(IGNORE)` makes an insert
//! report success having written nothing. So every write checks that
//! `rows_affected()` is the number it had to be, and every transaction begins
//! by checking `temp.sqlite_schema` is empty; either failing is
//! `NC-DB-0007`. The count is also what stops the same trigger in `main` when
//! another connection adds it after the store opened, which neither schema
//! check can see. A trigger's refusal is recognised by SQLite's extended code
//! 1811 (`SQLITE_CONSTRAINT_TRIGGER`) — `sqlx` reports its kind as `Other`.
//!
//! # What this answers (`§2.4`)
//!
//! Another account on the machine reading or rewriting the plugin store: the
//! directory is `0700` and every file `0600`, and a link or a foreign owner
//! anywhere in that is refused rather than opened. A plugin reaching another
//! plugin's keys: every read and write names the plugin, which the kernel binds
//! from the manifest, and no plugin ever supplies SQL — and the realm, so the
//! kernels of two realms on one machine never read or write each other's,
//! their units numbered by the one minter their journals share
//! (`nc_journal::Journal::sharing`). An image rewritten after
//! the fact: the append-only triggers, which hold under `trusted_schema = OFF`.
//! A half-written unit: one `BEGIN IMMEDIATE` transaction per commit, proved by
//! a child process that aborts between two of its statements.
//!
//! # What it deliberately does not
//!
//! **A process running as the same user.** SQLite has no roles: whoever can
//! open the file can `DROP TRIGGER` and then `DELETE`, and can race the checks
//! above between the check and the open. Nor an administrator, nor a disk read
//! offline without full-disk encryption — the file is not encrypted. An
//! anchor of the journal off this machine is a later stage's, which `NOCTUA.md
//! §14.9` leaves to choose. SQLite's `DEFENSIVE`
//! flag and an authorizer are not reachable through `sqlx`'s safe API, and
//! nothing here runs SQL anybody but this repository wrote. **Unix only**: the
//! modes and owners above are read through `std::os::unix`, and a build for
//! another family does not compile rather than opening a file it cannot judge.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

mod open;
pub mod schema;
mod store;

pub use open::blob_directory;
pub use store::{
    BlobCreated, BlobRow, CommitError, Committed, Entry, Expect, Image, LocalStore, Page, Quotas,
    Revision, StagedWrite, UnitDetail, UnitKind, UnitMeta, UnitRecord, UnitRow, UnitState,
};

use nc_catalog::ErrorCode;

/// The database file's name inside the directory [`LocalStore::open`] is given.
pub const FILE_NAME: &str = "local.sqlite3";

/// Why the local database could not be used.
///
/// Every variant carries the sentence a person reads and answers to one code
/// ([`LocalError::code`]). A conflict or a quota is not one of these: those are
/// a unit's outcomes, and [`CommitError`] carries them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalError {
    /// The directory or a file in it is not private to this user — another
    /// user can reach it, owns it, or decides where a link points — or this
    /// process may not read or write where it has to. Nothing was opened.
    Refused(String),
    /// The database is not the one this build expects: an object added,
    /// dropped or edited, a temporary object on the connection, a write that
    /// changed a different number of rows than it had to, another
    /// application's header, a damaged page, a SQLite version this crate does
    /// not accept, or a history of migrations this build did not write.
    Unexpected(String),
    /// A migration this build carries failed to apply.
    Migration(String),
    /// SQLite or the filesystem did not answer: busy past its timeout, an I/O
    /// error, a full disk, a read-only mount, a parent directory that does not
    /// exist.
    Unavailable(String),
}

impl LocalError {
    /// The registry code this failure carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Refused(_) => ErrorCode::DbLocalOpenRefused,
            Self::Unexpected(_) => ErrorCode::DbLocalSchemaUnexpected,
            Self::Migration(_) => ErrorCode::DbMigrationFailed,
            Self::Unavailable(_) => ErrorCode::DbUnreachable,
        }
    }
}

impl core::fmt::Display for LocalError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (Self::Refused(why)
        | Self::Unexpected(why)
        | Self::Migration(why)
        | Self::Unavailable(why)) = self;
        write!(formatter, "[{}] {why}", self.code().code())
    }
}

impl std::error::Error for LocalError {}

/// What SQLite answers when a trigger refuses a statement.
///
/// `SQLITE_CONSTRAINT_TRIGGER`: the primary code 19 with the extended bits
/// `7 << 8`. `sqlx` reports the kind of such an error as `Other`, so the code
/// is what tells a trigger's `RAISE(ABORT)` from any other failure.
pub const TRIGGER_REFUSED: &str = "1811";

/// What SQLite says, in the message of a refusal with [`TRIGGER_REFUSED`]'s
/// code, when a delete would leave a row referring to one that is gone.
///
/// **SQLite runs an `ON DELETE RESTRICT` as a trigger**, and answers it with
/// the trigger's code: measured on a unit still held in `store_unit_holds`,
/// whose `unit` refers to `store_units` that way. Its own sentence is what
/// tells the schema refusing as it declares from a trigger this crate never
/// installed.
pub const STILL_REFERRED_TO: &str = "FOREIGN KEY constraint failed";

/// What a refusal of a row another one still refers to is said with, after
/// what was being done and SQLite's own sentence.
pub const STILL_HELD: &str = "a row another one still refers to was to be deleted, and the \
                              schema's `ON DELETE RESTRICT` refused the whole statement";

/// Whether an error is a trigger refusing the statement.
#[must_use]
pub fn is_a_trigger_refusal(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == TRIGGER_REFUSED)
}

/// One `sqlx` error, as the failure it is here.
///
/// **The code decides, not the sentence.** SQLite's primary result code is
/// the low byte of the extended one: `SQLITE_BUSY` (5), `SQLITE_LOCKED` (6),
/// `SQLITE_READONLY` (8), `SQLITE_IOERR` (10), `SQLITE_FULL` (13) and
/// `SQLITE_CANTOPEN` (14) are the database not answering. Every other code —
/// `SQLITE_CORRUPT` (11), `SQLITE_NOTADB` (26), a constraint no statement here
/// should meet — is a file that is not what this build expects; a trigger's
/// refusal is too, because no statement here is one a trigger this crate
/// installed refuses. Anything that is not a database error at all — a pool
/// timeout, a worker that went away — is the database not answering.
///
/// **`SQLITE_READONLY` is a file this process may not write**, which is a
/// read-only mount or a mode changed under an open store — the filesystem's
/// answer, as a missing parent is, and not `NC-DB-0007`, which would tell the
/// user of a file made `0400` that their data is not what this version
/// expects. The open refuses such a file; this is what a write meets when the
/// mode changes after it.
pub(crate) fn said(doing: &str, error: &sqlx::Error) -> LocalError {
    let Some(database) = error.as_database_error() else {
        return LocalError::Unavailable(format!("{doing}: {error}"));
    };
    let why = format!("{doing}: {}", database.message());
    if is_a_trigger_refusal(error) && database.message() == STILL_REFERRED_TO {
        return LocalError::Unexpected(format!("{why} — {STILL_HELD}"));
    }
    if is_a_trigger_refusal(error) {
        return LocalError::Unexpected(format!(
            "{why} — a trigger refused a statement this crate writes, and no trigger the \
             schema declares refuses one of those"
        ));
    }
    let primary = database
        .code()
        .and_then(|code| code.parse::<u32>().ok())
        .map(|code| code & 0xff);
    match primary {
        Some(5 | 6 | 8 | 10 | 13 | 14) | None => LocalError::Unavailable(why),
        Some(_) => LocalError::Unexpected(why),
    }
}

#[cfg(test)]
mod tests {
    use super::LocalError;
    use nc_catalog::ErrorCode;

    /// Each failure answers to the code the registry declares for it.
    #[test]
    fn each_failure_carries_its_own_code() {
        for (error, code) in [
            (
                LocalError::Refused(String::new()),
                ErrorCode::DbLocalOpenRefused,
            ),
            (
                LocalError::Unexpected(String::new()),
                ErrorCode::DbLocalSchemaUnexpected,
            ),
            (
                LocalError::Migration(String::new()),
                ErrorCode::DbMigrationFailed,
            ),
            (
                LocalError::Unavailable(String::new()),
                ErrorCode::DbUnreachable,
            ),
        ] {
            assert_eq!(error.code(), code, "{error:?}");
            assert!(
                error.to_string().starts_with(&format!("[{}]", code.code())),
                "the sentence does not open with its code: {error}"
            );
        }
    }

    /// A failure that is not the database's is the database not answering.
    #[test]
    fn a_failure_with_no_database_answer_is_unavailable() {
        let error = super::said("counting", &sqlx::Error::PoolTimedOut);
        assert!(
            matches!(error, LocalError::Unavailable(ref why) if why.starts_with("counting")),
            "{error:?}"
        );
        assert!(
            error.code().retryable(),
            "a pool that timed out may answer later"
        );
        assert!(!super::is_a_trigger_refusal(&sqlx::Error::PoolTimedOut));
    }

    /// A database this process may not write is not answering, and not a file
    /// this build does not expect.
    ///
    /// **Driven with SQLite's own answer**, from a connection opened read-only:
    /// the code is what `said` reads, so the code is what the test hands it.
    #[tokio::test]
    async fn a_database_this_process_may_not_write_is_unavailable() {
        use sqlx::{ConnectOptions as _, Connection as _};
        let mut connection = sqlx::sqlite::SqliteConnectOptions::new()
            .in_memory(true)
            .read_only(true)
            .connect()
            .await
            .expect("a read-only database");
        let refused = sqlx::query("PRAGMA main.user_version = 1")
            .execute(&mut connection)
            .await
            .expect_err("a read-only database refuses a write");
        let primary = refused
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .and_then(|code| code.parse::<u32>().ok())
            .map(|code| code & 0xff);
        assert_eq!(primary, Some(8), "not SQLITE_READONLY: {refused}");
        let error = super::said("writing a key", &refused);
        assert!(
            matches!(error, LocalError::Unavailable(ref why) if why.starts_with("writing a key")),
            "{error:?}"
        );
        assert_eq!(error.code(), ErrorCode::DbUnreachable);
        connection.close().await.expect("closes");
    }
}
