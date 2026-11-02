//! The directory, the file and the connection options: everything before the
//! first statement.
//!
//! **Refused, not repaired.** A directory with group bits, a file owned by
//! somebody else, a link where a file should be: each is something this crate
//! did not make, and loosening or chmod-ing it would be deciding on the user's
//! behalf that whatever made it was harmless. The one thing created here is
//! what is missing, and it is created private.

use std::fs::{DirBuilder, Metadata, Permissions};
use std::io;
use std::os::unix::fs::{
    DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _,
};
use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};

use crate::LocalError;

/// The mode bits nobody but the owner may hold on anything here.
const OTHERS: u32 = 0o077;

/// The bits the owner must hold on the directory: read, write and enter.
const OWNER_OF_A_DIRECTORY: u32 = 0o700;

/// The bits the owner must hold on each file: read and write.
const OWNER_OF_A_FILE: u32 = 0o600;

/// How the name of the probe that says who this process is begins.
///
/// The standard library has no safe way to ask for the effective user, and
/// this workspace writes no `unsafe`. A file this process creates is owned by
/// that user, so one is created, read and removed. It lives in the directory
/// the database does, because that is the one place this crate must be able
/// to write anyway.
///
/// **One name per call, not one per directory.** The probe had one fixed name,
/// and every open removed whatever stood under it as a crashed run's leftover —
/// so two opens of one profile at once removed each other's probe. Measured:
/// four threads opening an existing profile a hundred times each failed 175 of
/// 400 opens, as `File exists` and as `No such file or directory`, and the
/// first of those reached the user as `NC-DB-0006`. SQLite's WAL lets a second
/// app instance or a command-line tool open the file beside the daemon, so the
/// name carries the process, the time and a count, and a call removes only the
/// probe it made. One a crash leaves between the two is an empty file nothing
/// reads.
const PROBE: &str = ".nc-local-owner-probe";

/// How many probes this process has made, so two made at once — two threads,
/// two pools — never carry the same name.
static PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    const fn said(self) -> &'static str {
        match self {
            Self::Database => {
                "the local database was not opened. It holds every plugin's data on this \
                 machine, and it is opened only when the directory is `0700`, the files are \
                 `0600`, and all of them are this user's own"
            }
            Self::Blobs => {
                "the blobs were not opened. That directory holds the bytes of every file \
                 plugins keep on this machine, and it is opened only when it is `0700`, the \
                 files in it are `0600`, and all of them are this user's own"
            }
        }
    }
}

/// The directory the kernel keeps the bytes of its blobs in: created `0700`
/// when it is missing, and refused when it is not this user's alone.
///
/// **By the rule the database's directory is held to**, written once: not a
/// link, a directory, owned by the user this process runs as — found by a
/// probe file made in it — with no group or other bits, and every bit its
/// owner needs. The files in it are the kernel's to create, `0600` each.
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
/// process runs as, whom a probe made in it names; answers that user.
fn judged_directory(directory: &Path, holding: Holding) -> Result<u32, LocalError> {
    // **Its shape before anything is written in it**: the probe below creates
    // a file, and a directory another user can reach is not one to create
    // anything in. Judged against its own owner first, and against this
    // process's once the probe has said who that is.
    let found = metadata_of(directory)?;
    judge(directory, &found, found.uid(), Kind::Directory, holding)?;
    let owner = whoami(directory)?;
    judge(
        directory,
        &metadata_of(directory)?,
        owner,
        Kind::Directory,
        holding,
    )?;
    Ok(owner)
}

/// The directory, private and held for this open, and the database file
/// inside it, private.
///
/// Returns the file's path and the directory held ([`held`]), which every
/// other open of the profile waits on until it is dropped.
pub(crate) fn private_file_in(directory: &Path) -> Result<(PathBuf, std::fs::File), LocalError> {
    private_directory(directory)?;
    let owner = judged_directory(directory, Holding::Database)?;
    let held = held(directory)?;
    let file = directory.join(crate::FILE_NAME);
    match std::fs::symlink_metadata(&file) {
        Ok(found) => judge(&file, &found, owner, Kind::File, Holding::Database)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_private(&file).map_err(|error| {
                failed(
                    &format!("`{}` could not be created", file.display()),
                    &error,
                )
            })?;
        }
        Err(error) => {
            return Err(failed(
                &format!("`{}` could not be read", file.display()),
                &error,
            ));
        }
    }
    for suffix in COMPANIONS {
        let companion = PathBuf::from(format!("{}{suffix}", file.display()));
        match std::fs::symlink_metadata(&companion) {
            Ok(found) => judge(&companion, &found, owner, Kind::File, Holding::Database)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(failed(
                    &format!("`{}` could not be read", companion.display()),
                    &error,
                ));
            }
        }
    }
    // **A file found in a directory this call just made is another open's**,
    // made between the two statements, and is judged as any other: refusing it
    // refused every open but the first of a fresh profile opened at once.
    Ok((file, held))
}

/// What [`private_file_in`] does, on the runtime's blocking pool: the
/// directory made and judged, the file made, and the profile held for this
/// open.
///
/// **Never on a thread that drives futures.** The lock is waited for in place,
/// and an open holds it across every `.await` of its judging and migrating:
/// waited for on such a thread, a second open of the profile blocked the
/// thread the first needed to go on and let go — measured by a reviewer, two
/// opens spawned on a current-thread runtime, and three on one of two
/// workers, waited thirty seconds and answered nothing. Each open's wait is a
/// thread of the blocking pool's instead.
///
/// # Errors
///
/// What [`private_file_in`] refuses, and [`LocalError::Unavailable`] when the
/// task it ran on did not finish.
pub(crate) async fn off_the_runtime(
    directory: &Path,
) -> Result<(PathBuf, std::fs::File), LocalError> {
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

/// `directory`, held for this open alone until the answer is dropped: an
/// exclusive lock every other open of the profile waits on.
///
/// **One open of a profile at a time** — two app instances, the daemon and a
/// command-line tool beside it — because what an open does to a profile
/// nobody has opened is not something SQLite lets two do at once: its journal
/// turned to a write-ahead log, which a connection asking at the same moment
/// is refused as a database that is locked, and its migrations applied, which
/// `sqlx` takes no lock for. A review measured both refusing opens of a sound
/// profile. Waited for in place, as every filesystem call of an open is — on
/// the blocking pool ([`off_the_runtime`]): an open holds it for as long as it
/// takes to judge and migrate, and one that ends releases it — a process
/// stopped while it holds it holds every other open of the profile until it
/// goes on or ends.
///
/// **The directory's, never the database file's**: POSIX releases every lock
/// a process holds on a file when it closes any descriptor of it, so a lock
/// taken through one would take SQLite's own with it when it is let go.
fn held(directory: &Path) -> Result<std::fs::File, LocalError> {
    let held = std::fs::File::open(directory).map_err(|error| {
        failed(
            &format!("`{}` could not be read", directory.display()),
            &error,
        )
    })?;
    held.lock().map_err(|error| {
        failed(
            &format!("`{}` could not be held for this open", directory.display()),
            &error,
        )
    })?;
    Ok(held)
}

/// What a path is supposed to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Directory,
    File,
}

/// Whether one path is what it should be and nobody else's.
///
/// **A link is refused however it points.** The metadata is the link's own —
/// `symlink_metadata` — so a link to a private file in a private directory is
/// still refused: what it points at is decided by whoever can change the link,
/// and that is not something the mode of the target says.
fn judge(
    path: &Path,
    found: &Metadata,
    owner: u32,
    kind: Kind,
    holding: Holding,
) -> Result<(), LocalError> {
    let refused = |what: String| {
        Err(LocalError::Refused(format!(
            "`{}` {what}, so {}",
            path.display(),
            holding.said()
        )))
    };
    let file_type = found.file_type();
    if file_type.is_symlink() {
        return refused("is a symbolic link".to_owned());
    }
    match kind {
        Kind::Directory if !file_type.is_dir() => return refused("is not a directory".to_owned()),
        Kind::File if !file_type.is_file() => return refused("is not a regular file".to_owned()),
        _ => {}
    }
    if found.uid() != owner {
        return refused(format!(
            "belongs to user {} and this process runs as user {owner}",
            found.uid()
        ));
    }
    let mode = found.mode() & 0o777;
    if mode & OTHERS != 0 {
        return refused(format!("has mode {mode:03o}, which another user can reach"));
    }
    // **And everything its owner needs.** Only the bits nobody else may hold
    // were judged, so a `0400` file opened — the self-check only reads — and
    // the next commit came back `NC-DB-0007`, *not what this version expects*,
    // for a file whose only fault was its mode. A directory is entered and
    // written in, a file read and written; anything less is refused here,
    // before SQLite opens it read-only behind this crate's back.
    let (needed, to) = match kind {
        Kind::Directory => (OWNER_OF_A_DIRECTORY, "read, enter and write in it"),
        Kind::File => (OWNER_OF_A_FILE, "read and write it"),
    };
    if mode & needed != needed {
        return refused(format!(
            "has mode {mode:03o}, which does not let its owner {to}"
        ));
    }
    Ok(())
}

/// The directory, created `0700` when it is missing.
///
/// One another open made between the look and the create is judged by the
/// caller as any directory found is.
fn private_directory(directory: &Path) -> Result<(), LocalError> {
    match std::fs::symlink_metadata(directory) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            // One level, not `recursive(true)`: a parent that does not exist is
            // a path nobody meant, and every directory `recursive` made would
            // take the umask rather than this mode.
            match DirBuilder::new().mode(0o700).create(directory) {
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

/// The metadata of a path itself, never of what a link points at.
fn metadata_of(path: &Path) -> Result<Metadata, LocalError> {
    std::fs::symlink_metadata(path)
        .map_err(|error| failed(&format!("`{}` could not be read", path.display()), &error))
}

/// The user this process runs as, read off a file it creates.
///
/// **The directory is judged by the probe's owner, and the probe can only be
/// made where this process may write.** A `0700` directory another user owns
/// refuses the probe outright unless this process is the superuser, and then
/// the probe's owner — root — is not the directory's.
fn whoami(directory: &Path) -> Result<u32, LocalError> {
    let probe = directory.join(probe_name());
    let made = create_private(&probe).map_err(|error| {
        failed(
            &format!(
                "`{}` is not a directory this process can write in",
                directory.display()
            ),
            &error,
        )
    })?;
    let owner = made.metadata().map(|found| found.uid());
    drop(made);
    let removed = std::fs::remove_file(&probe);
    let owner = owner
        .map_err(|error| failed(&format!("`{}` could not be read", probe.display()), &error))?;
    removed.map_err(|error| {
        failed(
            &format!("`{}` could not be removed", probe.display()),
            &error,
        )
    })?;
    Ok(owner)
}

/// A name for the owner probe that no other call makes, in this process or
/// another.
///
/// The process, because two processes count from zero; the time, because a
/// process id is reused once its owner is gone; the count, because two threads
/// read one clock.
fn probe_name() -> String {
    let made = PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{PROBE}-{}-{now}-{made}", std::process::id())
}

/// A new file, `0600`, refusing one that exists — a link included.
///
/// `create_new` is `O_CREAT | O_EXCL`, which fails on any existing name and
/// does not follow a link at the last component; the mode is then set outright,
/// because the one passed to `open` is narrowed by the umask and never widened.
#[expect(
    clippy::disallowed_methods,
    reason = "the database file and the owner probe are created here, private, before \
              SQLite or anybody else can make them; `ui` is the output path and neither \
              is a terminal — both are paths inside a directory this crate just judged"
)]
fn create_private(path: &Path) -> io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    Ok(file)
}

/// An I/O failure before SQLite is asked anything, as the failure it is here.
///
/// **Refused only when the answer is about who may reach the path**: a
/// permission this process does not have that its mode bits did not already
/// show — an access list, an immutable flag, a parent it may not enter — or a
/// name something else made between the look and the create. Everything else
/// the filesystem can say — a parent that does not exist, a full disk, a
/// read-only mount, an I/O error — is the data not being reachable,
/// `NC-DB-0001`. Every one of them was `NC-DB-0006`, measured: a path under a
/// parent that does not exist was refused with the message that other people
/// using the computer could reach the user's data. `NC-DB-0006` now says the
/// folder or the files are not set up for this user alone to read and write,
/// which is true of every refusal here.
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
/// asks. Measured, another application's rollback-journal file had its header
/// rewritten from `[1, 1]` to `[2, 2]` and a `-wal` and a `-shm` made beside
/// it, and was then refused as another application's with a message saying it
/// was left as it is. Every other pragma below belongs to the connection alone.
///
/// **And read-only, because closing a connection writes too.** SQLite's last
/// connection to a database in WAL mode checkpoints the `-wal` into the file
/// and deletes it when it closes, and its first read of a file beside a hot
/// rollback journal rolls the journal back; a review measured both on a file
/// it then refused — another application's, and one `quick_check` called
/// damaged. A connection opened read-only does neither: it reads through the
/// `-wal`, refuses to read past a hot journal ([`HOT_JOURNAL`]), and leaves
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
    if cfg!(target_os = "macos") {
        options.pragma("fullfsync", "ON")
    } else {
        options
    }
}

#[cfg(test)]
mod tests {
    use super::{Holding, Kind, PROBE, judge, probe_name};

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

    /// Every probe this process makes carries a name of its own.
    ///
    /// **One fixed name was shared by every open**, and each removed whatever
    /// stood under it, so two opens at once removed each other's probe —
    /// measured, 175 of 400 concurrent opens of one profile failed. A name
    /// repeated here is the same collision, decided without a race.
    #[test]
    fn every_probe_this_process_makes_has_a_name_of_its_own() {
        let names: Vec<String> = (0..64).map(|_| probe_name()).collect();
        let distinct: std::collections::BTreeSet<&String> = names.iter().collect();
        assert_eq!(distinct.len(), names.len(), "{names:?}");
        assert!(
            names.iter().all(|name| name.starts_with(PROBE)),
            "{names:?}"
        );
    }

    /// The blobs' directory is made `0700` when it is missing, and one another
    /// user can reach is refused in its own words and left as it was.
    #[test]
    fn the_blobs_directory_is_made_private_and_one_another_user_can_reach_is_refused() {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = Scratch::new("blob-directory");
        let mode = |path: &std::path::Path| {
            std::fs::symlink_metadata(path)
                .expect("there")
                .permissions()
                .mode()
                & 0o777
        };
        let missing = scratch.0.join("missing");
        super::blob_directory(&missing).expect("made");
        assert_eq!(mode(&missing), 0o700);
        super::blob_directory(&missing).expect("opened again as it is");
        let reachable = scratch.0.join("reachable");
        std::fs::create_dir(&reachable).expect("made");
        std::fs::set_permissions(&reachable, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        let refused = super::blob_directory(&reachable).expect_err("0755 is refused");
        assert!(
            matches!(&refused, crate::LocalError::Refused(why) if why.contains("so the blobs were not opened")),
            "{refused}"
        );
        assert_eq!(mode(&reachable), 0o755, "a refused directory was changed");
    }

    /// The blobs' directory is held to the database's rule, and its refusal
    /// says the directory is the blobs'.
    ///
    /// **One rule, two sentences**: a person told the database was refused
    /// would look in the wrong place for what to fix.
    #[test]
    fn the_blobs_directory_is_refused_by_the_same_rule_in_its_own_words() {
        let scratch = Scratch::new("blobs");
        std::fs::set_permissions(
            &scratch.0,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("chmod");
        let directory = std::fs::symlink_metadata(&scratch.0).expect("readable");
        let mine = std::os::unix::fs::MetadataExt::uid(&directory);
        let refused = judge(
            &scratch.0,
            &directory,
            mine.wrapping_add(1),
            Kind::Directory,
            Holding::Blobs,
        )
        .expect_err("another user's directory is refused for the blobs too");
        assert!(
            refused.to_string().contains("belongs to user")
                && refused.to_string().contains("so the blobs were not opened"),
            "{refused}"
        );
        assert!(
            judge(
                &scratch.0,
                &directory,
                mine,
                Kind::Directory,
                Holding::Blobs
            )
            .is_ok()
        );
    }

    /// Owner, kind and mode are each a refusal of their own, and so is each
    /// bit the owner needs and does not hold.
    ///
    /// A foreign owner cannot be made without the superuser, so the rule is
    /// driven here with the owner it is handed; the suite drives the rest
    /// against the filesystem.
    #[test]
    fn a_path_is_refused_for_each_way_it_is_not_private() {
        let scratch = Scratch::new("judge");
        let judging =
            |path: &std::path::Path, found: &std::fs::Metadata, owner: u32, kind: Kind| {
                judge(path, found, owner, kind, Holding::Database)
            };
        let directory = std::fs::symlink_metadata(&scratch.0).expect("readable");
        let mine = std::os::unix::fs::MetadataExt::uid(&directory);

        std::fs::set_permissions(
            &scratch.0,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("chmod");
        let directory = std::fs::symlink_metadata(&scratch.0).expect("readable");
        assert!(judging(&scratch.0, &directory, mine, Kind::Directory).is_ok());

        let someone_else = mine.wrapping_add(1);
        let refused = judging(&scratch.0, &directory, someone_else, Kind::Directory)
            .expect_err("another user's directory is refused");
        assert!(refused.to_string().contains("belongs to user"), "{refused}");
        assert!(
            refused
                .to_string()
                .contains("so the local database was not opened"),
            "{refused}"
        );
        let refused = judging(&scratch.0, &directory, mine, Kind::File)
            .expect_err("a directory is not a file");
        assert!(
            refused.to_string().contains("not a regular file"),
            "{refused}"
        );

        let file = scratch.0.join("f");
        std::fs::write(&file, b"").expect("a file");
        std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .expect("chmod");
        let found = std::fs::symlink_metadata(&file).expect("readable");
        assert!(judging(&file, &found, mine, Kind::File).is_ok());
        let refused =
            judging(&file, &found, mine, Kind::Directory).expect_err("a file is not a directory");
        assert!(refused.to_string().contains("not a directory"), "{refused}");

        // Each of the six bits, alone, not only the loud ones.
        for bit in [0o040, 0o020, 0o010, 0o004, 0o002, 0o001] {
            std::fs::set_permissions(
                &file,
                std::os::unix::fs::PermissionsExt::from_mode(0o600 | bit),
            )
            .expect("chmod");
            let found = std::fs::symlink_metadata(&file).expect("readable");
            let refused = judging(&file, &found, mine, Kind::File)
                .expect_err("a group or other bit is refused");
            assert!(
                refused.to_string().contains("another user can reach"),
                "{refused}"
            );
        }

        // And each bit its owner needs, alone: a file read and written, a
        // directory read, entered and written in. Judged first and asserted
        // after the directory is `0700` again, so a failure still leaves a
        // directory the scratch guard can remove.
        let mut judged = Vec::new();
        for (path, kind, mode) in [
            (&file, Kind::File, 0o400),
            (&file, Kind::File, 0o200),
            (&scratch.0, Kind::Directory, 0o600),
            (&scratch.0, Kind::Directory, 0o500),
            (&scratch.0, Kind::Directory, 0o300),
        ] {
            std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode))
                .expect("chmod");
            let found = std::fs::symlink_metadata(path).expect("readable");
            judged.push((mode, judging(path, &found, mine, kind)));
        }
        std::fs::set_permissions(
            &scratch.0,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("chmod");
        for (mode, verdict) in judged {
            let refused = verdict.expect_err("a bit its owner needs is missing");
            assert!(
                refused.to_string().contains(&format!(
                    "has mode {mode:03o}, which does not let its owner"
                )),
                "{refused}"
            );
        }
    }
}
