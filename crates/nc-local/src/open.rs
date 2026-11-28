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
//!
//! **And a missing database is made whole before it has its name.** It is
//! made under [`crate::CREATION_NAME`], turned into a write-ahead log there
//! ([`made`]), and renamed to [`crate::FILE_NAME`] only then, so a store at its
//! own name is never one half-made. What an open stopped part of the way left
//! under the creation name is the one thing this crate ever removes, and only
//! once it is judged by the rule the store is held to ([`begun`]).

use std::io;
use std::path::{Path, PathBuf};

use nc_platform::{Judged, Kind, User};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};

use crate::LocalError;

/// The files SQLite keeps beside the database, each held to the same rule.
const COMPANIONS: [&str; 3] = ["-wal", "-shm", "-journal"];

/// The path SQLite keeps `file`'s companion of `suffix` at: the file's name
/// with the suffix after it.
fn beside(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether the path `judged` answers for is there.
///
/// Held to the rule it was judged by: `false` when nothing is, and the
/// refusal or the failure of anything else, as what it is here.
fn there(judged: Result<(), Judged>) -> Result<bool, LocalError> {
    match judged {
        Ok(()) => Ok(true),
        Err(Judged::Failed(call)) if call.error().kind() == io::ErrorKind::NotFound => Ok(false),
        Err(judged) => Err(not_taken(judged, Holding::Database)),
    }
}

/// The companions SQLite keeps beside `file` that are there, each judged
/// first and refused when it is not this user's alone.
///
/// **SQLite makes them, with the system's own security**, so on Windows each
/// is owned by whoever the token makes the owner of what this process creates
/// — the Administrators group, in an elevated process — and is judged as such
/// ([`nc_platform::judge_made_by_default`]); the directory, the lock and the
/// database file are this crate's, made owned by the user, and held to that.
fn companions_of(file: &Path, user: &User) -> Result<Vec<PathBuf>, LocalError> {
    let mut found = Vec::new();
    for suffix in COMPANIONS {
        let companion = beside(file, suffix);
        if there(nc_platform::judge_made_by_default(&companion, user))? {
            found.push(companion);
        }
    }
    Ok(found)
}

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
/// inside it, private — or, where none is, a new one begun under the creation
/// name ([`begun`]).
///
/// Returns the file's path, the directory held ([`nc_platform::hold`]), which
/// every other open of the profile waits on until it is dropped, and the file
/// begun when there was none, which [`made`] puts at that path.
///
/// **A companion with no database beside it is refused**, `NC-DB-0007`, and
/// left as it is: this build never removes the database, so a `-wal`, a
/// `-shm` or a `-journal` at its name with nothing there is not one it left,
/// and SQLite would read a log or roll a journal back into the file made new
/// beside it.
///
/// **One open of a profile at a time** — two app instances, the daemon and a
/// command-line tool beside it — because what an open does to a profile
/// nobody has opened is not something SQLite lets two do at once: its journal
/// turned to a write-ahead log, which a connection asking at the same moment
/// is refused as a database that is locked, and its migrations applied, which
/// `sqlx` takes no lock for. Unheld, both would refuse opens of a sound
/// profile, measured. Waited for in place, as every filesystem call of an open
/// is — on the blocking pool ([`off_the_runtime`]): an open holds it for as
/// long as it takes to make, judge and migrate the file, its caller awaiting
/// it or not ([`crate::LocalStore::open`]), and one that ends releases it — a
/// process stopped while it holds it holds every other open of the profile
/// until it goes on or ends.
pub(crate) fn private_file_in(directory: &Path) -> Result<Prepared, LocalError> {
    private_directory(directory)?;
    let user = judged_directory(directory, Holding::Database)?;
    let held = nc_platform::hold(directory, crate::LOCK_NAME, &user)
        .map_err(|judged| not_taken(judged, Holding::Database))?;
    let file = directory.join(crate::FILE_NAME);
    let found = there(nc_platform::judge(&file, Kind::File, &user))?;
    let beside_it = companions_of(&file, &user)?;
    // **A file found in a directory this call just made is another open's**,
    // made between the two statements, and is judged as any other: refusing it
    // would refuse every open but the first of a fresh profile opened at once.
    if found {
        return Ok((file, held, None));
    }
    if let Some(alone) = beside_it.first() {
        return Err(LocalError::Unexpected(format!(
            "`{}` stands where there is no database for it to belong to, and this build \
             never removes the database, so it is not a file this build left. It was left as \
             it is, and nothing was made beside it",
            alone.display()
        )));
    }
    let making = begun(directory, user)?;
    Ok((file, held, Some(making)))
}

/// What [`private_file_in`] answers: the database file's path, the profile
/// held for this open, and the file begun when there was none.
pub(crate) type Prepared = (PathBuf, nc_platform::Held, Option<Making>);

/// A database file made new, private and empty, under
/// [`crate::CREATION_NAME`], with nothing beside it: what [`made`] turns into
/// a database and puts at the store's own name.
#[derive(Debug)]
pub(crate) struct Making {
    /// The directory it is made in, which the rename is made durable in.
    directory: PathBuf,
    /// Its path, under the creation name.
    creation: PathBuf,
    /// The handle it was made with, which flushes it once SQLite has
    /// written it and is closed before the rename.
    made: std::fs::File,
    /// The user the directory was judged to be, whom what is left beside it
    /// is held to.
    user: User,
}

/// What an earlier open stopped while it made the database left under the
/// creation name taken away, and the file made new there, private.
///
/// **Only names this crate makes, and never the store's**: the creation
/// file, and the `-journal`, `-wal` and `-shm` SQLite made beside it while it
/// was turned into a write-ahead log ([`made`]) — what a process killed as it
/// starts, or a power cut on a person's very first start, leaves. Each is
/// judged first by the rule its twin at the store's name is held to — not a
/// link, this user's alone — and one that is not is refused and left
/// (`NC-DB-0006`), as the store would be; only then is it removed, the
/// companions before the file. Nothing at [`crate::FILE_NAME`] is removed or
/// repaired here or anywhere: a file there is the store, held to what it is.
fn begun(directory: &Path, user: User) -> Result<Making, LocalError> {
    let creation = directory.join(crate::CREATION_NAME);
    let mut left = companions_of(&creation, &user)?;
    if there(nc_platform::judge(&creation, Kind::File, &user))? {
        left.push(creation.clone());
    }
    for path in &left {
        std::fs::remove_file(path).map_err(|error| {
            failed(
                &format!(
                    "`{}`, left by an open that stopped, could not be removed",
                    path.display()
                ),
                &error,
            )
        })?;
    }
    let made = nc_platform::create_private(&creation).map_err(|error| {
        failed(
            &format!("`{}` could not be created", creation.display()),
            &error,
        )
    })?;
    Ok(Making {
        directory: directory.to_owned(),
        creation,
        made,
        user,
    })
}

/// A point an open passes, which a test of this crate stops one at to drop
/// what awaits it there ([`crate::LocalStore::open`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Point {
    /// The file begun, before the connection that turns it into a
    /// write-ahead log opens ([`made`]).
    Converting,
    /// The file turned and flushed, before it is renamed to the store's
    /// name.
    Renaming,
    /// The file at the store's name, before it is judged.
    Judging,
}

/// The file `making` holds turned into an empty database by SQLite, under its
/// creation name, and then put at `file` whole; `held`, the profile's hold,
/// handed back once it is.
///
/// **So that a store at its own name is never half-made.** The first
/// connection that sets `journal_mode = WAL` on a new file writes its first
/// page through a rollback journal, and deletes the journal once the page is
/// down: a process stopped between the two left the file at the store's name
/// beside a hot journal, which every open after refused as a file this build
/// did not leave (`NC-DB-0007`) until somebody deleted the journal by hand —
/// met by a daemon killed as it started, twice on the Windows runner, and
/// measured on Linux by killing an open at the journal's unlink. Here that
/// conversion runs on one connection of its own, opened with the options of
/// every connection of the pool ([`options`]), and closed, so that SQLite
/// checkpoints and deletes any `-wal` and `-shm` it kept; then nothing may be
/// left beside the file, which is flushed, renamed to `file`, and the rename
/// made durable where the system does that by syncing the directory
/// ([`nc_platform::sync_directory`]). Stopped anywhere before the rename, an
/// open leaves its work under the creation name, which the next one removes
/// ([`begun`]); after it, an empty database in write-ahead-log mode, which the
/// next one migrates.
///
/// **Under the profile's hold, to the end of the work** ([`private_file_in`]):
/// the whole of it runs on one thread of the blocking pool, which owns `held`
/// and lets it go only once the rename is durable or the work has failed, so
/// no other open of this build makes, removes or renames a file in the
/// directory meanwhile, and `file` was found missing under it. **A future
/// would not**: one dropped while it awaited the conversion or the rename —
/// a daemon told to stop as it started drops its start, and a runtime shut
/// down drops every task — would drop the hold with it while SQLite went on
/// on its own thread and the rename on the blocking pool, and an open that
/// took the hold then would remove the creation file and make its own, which
/// the first open's rename put at the store's name — measured, the second
/// open refused `NC-DB-0001`, its file renamed from under it. A task of the
/// pool is never stopped part of the way: a runtime dropped waits for it, and
/// one shut down without waiting leaves it to run to its end. `between` is
/// called at [`Point::Converting`] and [`Point::Renaming`].
///
/// # Errors
///
/// [`LocalError::Unavailable`] when SQLite or the filesystem did not answer —
/// a full disk, a companion SQLite left when it closed the file — and what
/// [`said`](crate::said) and [`failed`] make of anything else they answer.
pub(crate) async fn made(
    making: Making,
    held: nc_platform::Held,
    file: &Path,
    between: impl Fn(Point) + Send + 'static,
) -> Result<nc_platform::Held, LocalError> {
    let file = file.to_owned();
    let runtime = tokio::runtime::Handle::current();
    let work = move || {
        between(Point::Converting);
        converted(&runtime, &making.creation)?;
        put_in_place(making, &file, &between)
    };
    let done = tokio::task::spawn_blocking(move || work().map(|()| held)).await;
    done.unwrap_or_else(|_| {
        Err(LocalError::Unavailable(
            "the new database could not be made: the task that made it did not finish".to_owned(),
        ))
    })
}

/// The file at `creation` turned into an empty database in write-ahead-log
/// mode by one connection opened with the options of the pool's
/// ([`options`]), and closed, so that SQLite checkpoints and deletes any
/// `-wal` and `-shm` it kept.
///
/// **Driven to its end on the thread that calls it**, one of the blocking
/// pool's, through the runtime's handle: `sqlx` keeps a connection on a
/// thread of its own and answers through channels, so nothing here waits on
/// the runtime's timers or its I/O, and it goes on whether or not the
/// runtime does.
fn converted(runtime: &tokio::runtime::Handle, creation: &Path) -> Result<(), LocalError> {
    use sqlx::{ConnectOptions as _, Connection as _};
    runtime.block_on(async {
        let converting = options(creation)
            .connect()
            .await
            .map_err(|error| crate::said("making the local database", &error))?;
        converting.close().await.map_err(|error| {
            crate::said(
                "closing the connection that made the local database",
                &error,
            )
        })
    })
}

/// What [`made`] does once SQLite has closed the file: nothing left beside
/// it, the file flushed, renamed to `file`, and the rename made durable.
///
/// **Before the rename**, `between` is called at [`Point::Renaming`].
fn put_in_place(making: Making, file: &Path, between: &impl Fn(Point)) -> Result<(), LocalError> {
    let Making {
        directory,
        creation,
        made,
        user,
    } = making;
    if let Some(left) = companions_of(&creation, &user)?.first() {
        return Err(LocalError::Unavailable(format!(
            "`{}` was left beside the new database when SQLite closed it, so the database was \
             not put in place; the next open makes it again",
            left.display()
        )));
    }
    made.sync_all().map_err(|error| {
        failed(
            &format!("`{}` could not be flushed", creation.display()),
            &error,
        )
    })?;
    drop(made);
    between(Point::Renaming);
    std::fs::rename(&creation, file).map_err(|error| {
        failed(
            &format!(
                "`{}` could not be renamed to `{}`",
                creation.display(),
                file.display()
            ),
            &error,
        )
    })?;
    nc_platform::sync_directory(&directory).map_err(|error| {
        failed(
            &format!("`{}` could not be synced", directory.display()),
            &error,
        )
    })
}

/// What [`private_file_in`] does, on the runtime's blocking pool: the
/// directory made and judged, the file found or begun, and the profile held
/// for this open.
///
/// **Never on a thread that drives futures.** The lock is waited for in place,
/// and an open holds it across every `.await` of its judging and migrating:
/// waited for on such a thread, a second open of the profile would block the
/// thread the first needs to go on and let go — measured, two opens spawned on
/// a current-thread runtime, and three on one of two workers, would wait thirty
/// seconds and answer nothing. Each open's wait is a thread of the blocking
/// pool's instead.
///
/// **What it answers is the open's from then on, wherever its caller is**: a
/// task of the open's own awaits it ([`crate::LocalStore::open`]), and the
/// hold goes with the file begun to the thread that makes it ([`made`]).
///
/// # Errors
///
/// What [`private_file_in`] refuses, and [`LocalError::Unavailable`] when the
/// task it ran on did not finish.
pub(crate) async fn off_the_runtime(directory: &Path) -> Result<Prepared, LocalError> {
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

    /// A private scratch directory, and the user it is held to.
    fn held_to_its_user(scratch: &Scratch) -> nc_platform::User {
        nc_platform::testing::make_private(&scratch.0).expect("private");
        nc_platform::user_in(&scratch.0).expect("this user's")
    }

    /// What an open stopped while it made the database left under the
    /// creation name — the file, and each companion SQLite makes — is taken
    /// away before the file is made again, empty and private.
    ///
    /// **Taken away here, before SQLite sees the new file**: SQLite deletes a
    /// `-wal` or a `-journal` it finds beside a file of no pages itself, so an
    /// open that left them to it would pass every suite that opens a store,
    /// measured. This is where the open is held to its own rule.
    #[test]
    fn what_an_open_stopped_while_making_the_database_left_is_taken_away_first() {
        let scratch = Scratch::new("left");
        let user = held_to_its_user(&scratch);
        let creation = scratch.0.join(crate::CREATION_NAME);
        let mut left = vec![creation.clone()];
        left.extend(
            super::COMPANIONS
                .iter()
                .map(|suffix| super::beside(&creation, suffix)),
        );
        for path in &left {
            let mut file = nc_platform::create_private(path).expect("a file left");
            std::io::Write::write_all(&mut file, b"left by an open that stopped").expect("written");
        }
        let making = super::begun(&scratch.0, user).expect("begun");
        for suffix in super::COMPANIONS {
            assert!(
                std::fs::symlink_metadata(super::beside(&creation, suffix)).is_err(),
                "`{suffix}` was left for SQLite to find"
            );
        }
        assert_eq!(
            std::fs::metadata(&creation).expect("made").len(),
            0,
            "the file was not made new"
        );
        assert!(
            nc_platform::testing::is_as_created(&creation),
            "made private"
        );
        drop(making);
    }

    /// A companion left beside the new file once SQLite has closed it keeps
    /// the file from the store's name, whichever it is.
    #[test]
    fn a_file_left_beside_the_new_database_keeps_it_from_the_stores_name() {
        let scratch = Scratch::new("kept-from-its-name");
        let user = held_to_its_user(&scratch);
        let file = scratch.0.join(crate::FILE_NAME);
        for suffix in super::COMPANIONS {
            let making = super::begun(&scratch.0, user.clone()).expect("begun");
            let left = super::beside(&scratch.0.join(crate::CREATION_NAME), suffix);
            drop(nc_platform::create_private(&left).expect("a file left beside it"));
            let refused =
                super::put_in_place(making, &file, &|_| {}).expect_err("not put in place");
            assert!(
                matches!(&refused, crate::LocalError::Unavailable(why) if why.contains(suffix)),
                "{suffix}: {refused:?}"
            );
            assert!(
                std::fs::symlink_metadata(&file).is_err(),
                "`{suffix}` beside it, the file was put at the store's name"
            );
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

    /// A link where the blobs' directory should be is refused as a link, made
    /// by either door, and the directory it points at is left as it was.
    ///
    /// **Both doors**, since on Windows a junction and a directory's symbolic
    /// link are reparse points of two tags; on Unix the two are one link.
    #[test]
    fn a_linked_blobs_directory_is_refused_however_it_is_linked() {
        /// A door that makes a link at its second path to the directory at its first.
        type Door = fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>;

        let scratch = Scratch::new("blob-link");
        let real = scratch.0.join("real");
        super::blob_directory(&real).expect("made");
        let before = nc_platform::testing::access(&real).expect("readable");
        let doors: [(&str, Door); 2] = [
            ("linked", nc_platform::testing::link_directory),
            ("symlinked", nc_platform::testing::symlink_directory),
        ];
        for (name, link) in doors {
            let linked = scratch.0.join(name);
            link(&real, &linked).expect("a link");
            let refused = super::blob_directory(&linked).expect_err("a link is refused");
            let crate::LocalError::Refused(why) = &refused else {
                panic!("{name}: refused as not private, and it was {refused:?}");
            };
            assert!(
                why.contains("symbolic link") && why.contains("so the blobs were not opened"),
                "{name}: {refused}"
            );
        }
        assert_eq!(
            nc_platform::testing::access(&real).expect("readable"),
            before,
            "the directory a refused link points at was changed"
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
