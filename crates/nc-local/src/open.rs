//! The directory, the file and the connection options: everything before the
//! first statement.
//!
//! **Refused, not repaired.** A directory another user can reach, a file owned
//! by somebody else, a link where a file should be: each is something this
//! crate did not make, and loosening it would be deciding on the user's behalf
//! that whatever made it was harmless. The one thing created here is what is
//! missing, and it is created private. What each of those is on each system —
//! a mode or an access list, a symbolic link or any reparse point — is the
//! answer of `nc_platform`, which this module asks, and then says what was not
//! opened.

use std::io;
use std::path::{Path, PathBuf};

use nc_platform::{Judged, Kind, User};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};

use crate::LocalError;

/// The files SQLite keeps beside the database, each held to the same rule.
const COMPANIONS: [&str; 3] = ["-wal", "-shm", "-journal"];

/// What a private directory holds, which its refusal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holding {
    /// The local database and the files SQLite keeps beside it.
    Database,
    /// The bytes of the blobs the kernel keeps, one file each.
    Blobs,
}

impl Holding {
    /// What a refusal says was not opened, and why it is held to the rule.
    fn said(self) -> String {
        let (directory, file) = (nc_platform::PRIVATE_DIRECTORY, nc_platform::PRIVATE_FILE);
        match self {
            Self::Database => format!(
                "the local database was not opened. It holds every plugin's data on this \
                 machine, and it is opened only when the directory is {directory}, the files \
                 are {file}, and all of them are this user's own"
            ),
            Self::Blobs => format!(
                "the blobs were not opened. That directory holds the bytes of every file \
                 plugins keep on this machine, and it is opened only when it is {directory}, \
                 the files in it are {file}, and all of them are this user's own"
            ),
        }
    }
}

/// What was not taken, as the failure it is here: a refusal ends saying what
/// was not opened, and a call of the filesystem is read by [`failed`].
fn not_taken(judged: Judged, holding: Holding) -> LocalError {
    match judged {
        Judged::Refused(refused) => {
            LocalError::Refused(format!("{refused}, so {}", holding.said()))
        }
        Judged::Failed(call) => failed(call.doing(), call.error()),
    }
}

/// The directory the kernel keeps the bytes of its blobs in: created private
/// when it is missing, and refused when it is not this user's alone.
///
/// **By the rule the database's directory is held to**, written once
/// ([`nc_platform`]): not a link, a directory, owned by the user this process
/// runs as, reachable by nobody else, and with everything its owner needs.
/// The files in it are the kernel's to create, private each.
///
/// # Errors
///
/// [`LocalError::Refused`] when the directory is not private to this user, and
/// [`LocalError::Unavailable`] when the filesystem cannot be asked or the
/// directory made — a parent that does not exist, a full disk.
pub fn blob_directory(directory: &Path) -> Result<(), LocalError> {
    private_directory(directory)?;
    judged_directory(directory, Holding::Blobs).map(drop)
}

/// The directory judged against its own owner, then against the user this
/// process runs as; answers that user.
fn judged_directory(directory: &Path, holding: Holding) -> Result<User, LocalError> {
    nc_platform::user_in(directory).map_err(|judged| not_taken(judged, holding))
}

/// The directory, private and held for this open, and the database file
/// inside it, private.
///
/// Returns the file's path and the directory held ([`nc_platform::hold`]),
/// which every other open of the profile waits on until it is dropped.
///
/// **One open of a profile at a time** — two app instances, the daemon and a
/// command-line tool beside it — because what an open does to a profile
/// nobody has opened is not something SQLite lets two do at once: its journal
/// turned to a write-ahead log, which a connection asking at the same moment
/// is refused as a database that is locked, and its migrations applied, which
/// `sqlx` takes no lock for. Unheld, both would refuse opens of a sound
/// profile, measured. Waited for in place, as every filesystem call of an open
/// is — on the blocking pool ([`off_the_runtime`]): an open holds it for as
/// long as it takes to judge and migrate, and one that ends releases it — a
/// process stopped while it holds it holds every other open of the profile
/// until it goes on or ends.
pub(crate) fn private_file_in(
    directory: &Path,
) -> Result<(PathBuf, nc_platform::Held), LocalError> {
    private_directory(directory)?;
    let user = judged_directory(directory, Holding::Database)?;
    let held = nc_platform::hold(directory, crate::LOCK_NAME, &user)
        .map_err(|judged| not_taken(judged, Holding::Database))?;
    let file = directory.join(crate::FILE_NAME);
    match nc_platform::judge(&file, Kind::File, &user) {
        Ok(()) => {}
        Err(Judged::Failed(call)) if call.error().kind() == io::ErrorKind::NotFound => {
            nc_platform::create_private(&file).map_err(|error| {
                failed(
                    &format!("`{}` could not be created", file.display()),
                    &error,
                )
            })?;
        }
        Err(judged) => return Err(not_taken(judged, Holding::Database)),
    }
    // **SQLite makes the companions, with the system's own security**, so on
    // Windows each is owned by whoever the token makes the owner of what this
    // process creates — the Administrators group, in an elevated process —
    // and is judged as such; the directory, the lock and the file above are
    // this crate's, made owned by the user, and held to that.
    for suffix in COMPANIONS {
        let companion = PathBuf::from(format!("{}{suffix}", file.display()));
        match nc_platform::judge_made_by_default(&companion, &user) {
            Ok(()) => {}
            Err(Judged::Failed(call)) if call.error().kind() == io::ErrorKind::NotFound => {}
            Err(judged) => return Err(not_taken(judged, Holding::Database)),
        }
    }
    // **A file found in a directory this call just made is another open's**,
    // made between the two statements, and is judged as any other: refusing it
    // would refuse every open but the first of a fresh profile opened at once.
    Ok((file, held))
}

/// What [`private_file_in`] does, on the runtime's blocking pool: the
/// directory made and judged, the file made, and the profile held for this
/// open.
///
/// **Never on a thread that drives futures.** The lock is waited for in place,
/// and an open holds it across every `.await` of its judging and migrating:
/// waited for on such a thread, a second open of the profile would block the
/// thread the first needs to go on and let go — measured, two opens spawned on
/// a current-thread runtime, and three on one of two workers, would wait thirty
/// seconds and answer nothing. Each open's wait is a thread of the blocking
/// pool's instead.
///
/// # Errors
///
/// What [`private_file_in`] refuses, and [`LocalError::Unavailable`] when the
/// task it ran on did not finish.
pub(crate) async fn off_the_runtime(
    directory: &Path,
) -> Result<(PathBuf, nc_platform::Held), LocalError> {
    let directory = directory.to_owned();
    tokio::task::spawn_blocking(move || private_file_in(&directory))
        .await
        .unwrap_or_else(|_| {
            Err(LocalError::Unavailable(
                "the profile could not be prepared: the task that held it did not finish"
                    .to_owned(),
            ))
        })
}

/// The directory, created private when it is missing.
///
/// One another open made between the look and the create is judged by the
/// caller as any directory found is.
fn private_directory(directory: &Path) -> Result<(), LocalError> {
    match std::fs::symlink_metadata(directory) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // One level, never its parents: a parent that does not exist is a
            // path nobody meant.
            match nc_platform::create_private_directory(directory) {
                Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(failed(
                    &format!("`{}` could not be created", directory.display()),
                    &error,
                )),
                _ => Ok(()),
            }
        }
        Err(error) => Err(failed(
            &format!("`{}` could not be read", directory.display()),
            &error,
        )),
    }
}

/// An I/O failure before SQLite is asked anything, as the failure it is here.
///
/// **Refused only when the answer is about who may reach the path**: a
/// permission this process does not have that its mode bits did not already
/// show — an access list, an immutable flag, a parent it may not enter — or a
/// name something else made between the look and the create. Everything else
/// the filesystem can say — a parent that does not exist, a full disk, a
/// read-only mount, an I/O error — is the data not being reachable,
/// `NC-DB-0001`, and none of them `NC-DB-0006`, which says the folder or the
/// files are not set up for this user alone to read and write: true of every
/// refusal here, and of none of those.
fn failed(what: &str, error: &io::Error) -> LocalError {
    let why = format!("{what}: {error}");
    match error.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists => LocalError::Refused(why),
        _ => LocalError::Unavailable(why),
    }
}

/// The connection options every connection of the pool is opened with.
///
/// **Set on the connection, not in a migration**: a pragma is a property of
/// the connection that runs it, and a pool opens connections a migration never
/// sees. `create_if_missing(false)`, because the file was created above with a
/// mode SQLite would not have used.
pub(crate) fn options(file: &Path) -> SqliteConnectOptions {
    connection(file).journal_mode(SqliteJournalMode::Wal)
}

/// The options of the one connection that judges the file before anything may
/// write it: read-only, and without `journal_mode`.
///
/// **`journal_mode = WAL` is the one pragma here that writes the file**: it
/// converts a database in another journal mode on the first connection that
/// asks. Measured, another application's rollback-journal file, judged on
/// such a connection, would have its header rewritten from `[1, 1]` to
/// `[2, 2]` and a `-wal` and a `-shm` made beside it, and then be refused as
/// another application's with a message saying it was left as it is. Every
/// other pragma below belongs to the connection alone.
///
/// **And read-only, because closing a connection writes too.** SQLite's last
/// connection to a database in WAL mode checkpoints the `-wal` into the file
/// and deletes it when it closes, and its first read of a file beside a hot
/// rollback journal rolls the journal back; both are measured on a file the
/// open would then refuse — another application's, and one `quick_check`
/// calls damaged. A connection opened read-only does neither: it reads through
/// the `-wal`, refuses to read past a hot journal ([`HOT_JOURNAL`]), and leaves
/// the file and its journal byte for byte as they stood. What it may leave is
/// the `-shm` SQLite builds to read a `-wal` that has none beside it, which
/// holds no data and is SQLite's own index of the log.
pub(crate) fn for_judging(file: &Path) -> SqliteConnectOptions {
    connection(file).read_only(true)
}

/// SQLite's extended code for a read-only connection that found a hot
/// rollback journal beside the file, which only a writer may roll back:
/// `SQLITE_READONLY_ROLLBACK`.
pub(crate) const HOT_JOURNAL: &str = "776";

/// The options both the pool and the judging connection are opened with.
fn connection(file: &Path) -> SqliteConnectOptions {
    let options = SqliteConnectOptions::new()
        .filename(file)
        .create_if_missing(false)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .pragma("trusted_schema", "OFF")
        .pragma("cell_size_check", "ON")
        .pragma("temp_store", "MEMORY");
    // On macOS `fsync` does not reach the platter; `F_FULLFSYNC` does, and
    // `FULL` means nothing without it.
    if nc_platform::FSYNC_STOPS_SHORT_OF_THE_DISK {
        options.pragma("fullfsync", "ON")
    } else {
        options
    }
}

#[cfg(test)]
mod tests {
    use super::{Holding, not_taken};

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-local-open-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// The blobs' directory is made private when it is missing, and one
    /// another user can reach is refused in its own words and left as it was.
    #[test]
    fn the_blobs_directory_is_made_private_and_one_another_user_can_reach_is_refused() {
        let scratch = Scratch::new("blob-directory");
        let missing = scratch.0.join("missing");
        super::blob_directory(&missing).expect("made");
        assert!(
            nc_platform::testing::is_as_created(&missing),
            "made private"
        );
        super::blob_directory(&missing).expect("opened again as it is");
        let reachable = scratch.0.join("reachable");
        std::fs::create_dir(&reachable).expect("made");
        nc_platform::testing::make_private(&reachable).expect("private");
        let said = nc_platform::testing::open_to_another_user(&reachable).expect("opened up");
        let before = nc_platform::testing::access(&reachable).expect("readable");
        let refused = super::blob_directory(&reachable).expect_err("refused");
        let crate::LocalError::Refused(why) = &refused else {
            panic!("refused as not private, and it was {refused:?}");
        };
        assert!(
            why.contains("so the blobs were not opened") && why.contains(&said),
            "{said}: {refused}"
        );
        assert_eq!(
            nc_platform::testing::access(&reachable).expect("readable"),
            before,
            "a refused directory was changed"
        );
    }

    /// The blobs' directory is held to the database's rule, and its refusal
    /// says the directory is the blobs'.
    ///
    /// **One rule, two sentences**: a person told the database was refused
    /// would look in the wrong place for what to fix.
    #[test]
    fn the_blobs_directory_is_refused_by_the_same_rule_in_its_own_words() {
        let scratch = Scratch::new("blobs");
        nc_platform::testing::make_private(&scratch.0).expect("private");
        let mine = nc_platform::user_in(&scratch.0).expect("this user's");
        let someone_else = nc_platform::testing::someone_else(&mine);
        let refused = nc_platform::judge(&scratch.0, nc_platform::Kind::Directory, &someone_else)
            .map_err(|judged| not_taken(judged, Holding::Blobs))
            .expect_err("another user's directory is refused for the blobs too");
        assert!(
            refused.to_string().contains("belongs to user")
                && refused.to_string().contains("so the blobs were not opened"),
            "{refused}"
        );
        let database = nc_platform::judge(&scratch.0, nc_platform::Kind::Directory, &someone_else)
            .map_err(|judged| not_taken(judged, Holding::Database))
            .expect_err("and for the database");
        assert!(
            database
                .to_string()
                .contains("so the local database was not opened"),
            "{database}"
        );
        assert!(nc_platform::judge(&scratch.0, nc_platform::Kind::Directory, &mine).is_ok());
    }
}
