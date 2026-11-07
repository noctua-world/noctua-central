//! The Unix half, which macOS takes too: modes and owners through the
//! standard library's `std::os::unix`.
//!
//! **Refused, not repaired.** A directory with group bits, a file owned by
//! somebody else, a link where a file should be: each is something this did
//! not make, and loosening or chmod-ing it would be deciding on the user's
//! behalf that whatever made it was harmless.

#![cfg(unix)]

use std::fs::{DirBuilder, Metadata, Permissions};
use std::io;
use std::os::unix::fs::{
    DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _,
};
use std::path::Path;

use crate::{Failed, Judged, Kind, Refusal};

/// The mode bits nobody but the owner may hold on anything here.
const OTHERS: u32 = 0o077;

/// The bits the owner must hold on a directory: read, write and enter.
const OWNER_OF_A_DIRECTORY: u32 = 0o700;

/// The bits the owner must hold on a file: read and write.
const OWNER_OF_A_FILE: u32 = 0o600;

/// What a private directory is here, as a sentence says it.
pub(crate) const PRIVATE_DIRECTORY: &str = "`0700`";

/// What a private file is here, as a sentence says it.
pub(crate) const PRIVATE_FILE: &str = "`0600`";

/// A directory is synced through a handle of its own.
pub(crate) const SYNCS_A_DIRECTORY: bool = true;

/// A directory is held through a handle of its own, and makes no lock file.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const HOLDS_THROUGH_A_FILE: bool = false;

/// `tmpfs` at `/dev/shm` on Linux, which lists a directory in the order its
/// entries were made; APFS, which macOS mounts, lists by a hash of the name.
pub(crate) const LISTED_AS_MADE: Option<&str> = if cfg!(target_os = "linux") {
    Some("/dev/shm")
} else {
    None
};

/// The system's own `sh`, which spends a path as POSIX says.
pub(crate) const A_POSIX_SHELL: bool = true;

/// Python 3 by the name a Unix system gives it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const PYTHON: &str = "python3";

/// A user, by the number the system knows it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct User(u32);

impl core::fmt::Display for User {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The directory judged against its own owner, then against the user this
/// process runs as, whom a probe made in it names; answers that user.
pub(crate) fn user_in(directory: &Path) -> Result<User, Judged> {
    // **Its shape before anything is written in it**: the probe below creates
    // a file, and a directory another user can reach is not one to create
    // anything in. Judged against its own owner first, and against this
    // process's once the probe has said who that is.
    let found = metadata_of(directory)?;
    judged(directory, &found, &User(found.uid()), Kind::Directory)?;
    let user = whoami(directory)?;
    judged(directory, &metadata_of(directory)?, &user, Kind::Directory)?;
    Ok(user)
}

/// Whether `path` is what it should be and nobody else's.
pub(crate) fn judge(path: &Path, kind: Kind, user: &User) -> Result<(), Judged> {
    judged(path, &metadata_of(path)?, user, kind)
}

/// Whether a file another program made with the system's own mode and owner
/// is nobody else's: as any file is, since the owner a new file is given here
/// is the user this process runs as.
pub(crate) fn judge_made_by_default(path: &Path, user: &User) -> Result<(), Judged> {
    judge(path, Kind::File, user)
}

/// Whether one path, read as `found`, is what it should be and nobody else's.
///
/// **A link is refused however it points.** The metadata is the link's own —
/// `symlink_metadata` — so a link to a private file in a private directory is
/// still refused: what it points at is decided by whoever can change the link,
/// and that is not something the mode of the target says.
fn judged(path: &Path, found: &Metadata, owner: &User, kind: Kind) -> Result<(), Judged> {
    let refused = |what: String| Err(Judged::Refused(Refusal::of(path, what)));
    let file_type = found.file_type();
    if file_type.is_symlink() {
        return refused("is a symbolic link".to_owned());
    }
    match kind {
        Kind::Directory if !file_type.is_dir() => return refused("is not a directory".to_owned()),
        Kind::File if !file_type.is_file() => return refused("is not a regular file".to_owned()),
        _ => {}
    }
    if found.uid() != owner.0 {
        return refused(format!(
            "belongs to user {} and this process runs as user {owner}",
            found.uid()
        ));
    }
    let mode = found.mode() & 0o777;
    if mode & OTHERS != 0 {
        return refused(format!("has mode {mode:03o}, which another user can reach"));
    }
    // **And everything its owner needs.** Judged by the bits nobody else may
    // hold alone, a `0400` file would open — the local database's self-check
    // only reads — and the next commit come back `NC-DB-0007`, *not what this
    // version expects*, for a file whose only fault is its mode. A directory is
    // entered and written in, a file read and written; anything less is
    // refused here, before SQLite opens it read-only behind the caller's back.
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

/// The metadata of a path itself, never of what a link points at.
fn metadata_of(path: &Path) -> Result<Metadata, Failed> {
    std::fs::symlink_metadata(path)
        .map_err(|error| Failed::new(format!("`{}` could not be read", path.display()), error))
}

/// The directory, created `0700`, one level.
///
/// Not `recursive(true)`: every directory `recursive` made would take the
/// umask rather than this mode.
pub(crate) fn create_private_directory(directory: &Path) -> io::Result<()> {
    DirBuilder::new().mode(0o700).create(directory)
}

/// A new file, `0600`, refusing one that exists — a link included.
///
/// `create_new` is `O_CREAT | O_EXCL`, which fails on any existing name and
/// does not follow a link at the last component; the mode is then set outright,
/// because the one passed to `open` is narrowed by the umask and never widened.
#[expect(
    clippy::disallowed_methods,
    reason = "the local database's file, the owner probe and every writing of a blob are \
              created here, private, before SQLite or anybody else can make them; `ui` is the \
              output path and none is a terminal — each is a path inside a directory its \
              caller just judged"
)]
pub(crate) fn create_private(path: &Path) -> io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o600))?;
    Ok(file)
}

/// `directory`, held through a handle of its own under an exclusive lock.
///
/// **The directory's, never a file's**: POSIX releases every lock a process
/// holds on a file when it closes any descriptor of it, so a lock taken
/// through the database file would take SQLite's own with it when it is let
/// go. A directory takes no lock file, so `_name` is not made.
pub(crate) fn hold(directory: &Path, _name: &str, _user: &User) -> Result<std::fs::File, Judged> {
    let held = std::fs::File::open(directory).map_err(|error| {
        Failed::new(
            format!("`{}` could not be read", directory.display()),
            error,
        )
    })?;
    held.lock().map_err(|error| {
        Failed::new(
            format!("`{}` could not be held for this open", directory.display()),
            error,
        )
    })?;
    Ok(held)
}

/// The directory synced through a handle of its own, which is what makes a
/// rename into it durable.
pub(crate) fn sync_directory(directory: &Path) -> io::Result<()> {
    std::fs::File::open(directory)?.sync_all()
}

/// The time `path` was last written, set to `at` through a handle opened to
/// read it, which is all the owner needs here.
pub(crate) fn set_modified(path: &Path, at: std::time::SystemTime) -> io::Result<()> {
    std::fs::File::open(path)?.set_modified(at)
}

/// The user this process runs as, read off a file it creates.
///
/// **The directory is judged by the probe's owner, and the probe can only be
/// made where this process may write.** A `0700` directory another user owns
/// refuses the probe outright unless this process is the superuser, and then
/// the probe's owner — root — is not the directory's.
fn whoami(directory: &Path) -> Result<User, Failed> {
    let probe = directory.join(crate::probe_name());
    let made = create_private(&probe).map_err(|error| {
        Failed::new(
            format!(
                "`{}` is not a directory this process can write in",
                directory.display()
            ),
            error,
        )
    })?;
    let owner = made.metadata().map(|found| User(found.uid()));
    drop(made);
    let removed = std::fs::remove_file(&probe);
    let owner = owner
        .map_err(|error| Failed::new(format!("`{}` could not be read", probe.display()), error))?;
    removed.map_err(|error| {
        Failed::new(format!("`{}` could not be removed", probe.display()), error)
    })?;
    Ok(owner)
}

/// What a path lets in: its mode bits.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Access(u32);

/// The mode bits of `path`, as `ls -l` reads them.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn access(path: &Path) -> io::Result<Access> {
    Ok(Access(std::fs::symlink_metadata(path)?.mode() & 0o777))
}

/// The mode this crate creates a path of `path`'s kind with.
#[cfg(any(test, feature = "test-support"))]
fn private_mode(path: &Path) -> io::Result<u32> {
    Ok(if std::fs::symlink_metadata(path)?.is_dir() {
        OWNER_OF_A_DIRECTORY
    } else {
        OWNER_OF_A_FILE
    })
}

/// Whether `path` holds exactly the mode this crate creates it with.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_as_created(path: &Path) -> bool {
    matches!(
        (access(path), private_mode(path)),
        (Ok(Access(mode)), Ok(private)) if mode == private
    )
}

/// Whether `path` lets in its owner alone, with everything: here, the mode
/// this crate creates it with, since a mode says nothing more.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_private(path: &Path) -> bool {
    is_as_created(path)
}

/// Sets the mode of `path`.
#[cfg(any(test, feature = "test-support"))]
fn chmod(path: &Path, mode: u32) -> io::Result<()> {
    std::fs::set_permissions(path, Permissions::from_mode(mode))
}

/// Gives `path` the mode this crate creates it with.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn make_private(path: &Path) -> io::Result<()> {
    chmod(path, private_mode(path)?)
}

/// A group bit and an other bit on `path`, named by its mode.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn open_to_another_user(path: &Path) -> io::Result<String> {
    let mode = if std::fs::symlink_metadata(path)?.is_dir() {
        0o750
    } else {
        0o604
    };
    chmod(path, mode)?;
    Ok(format!("mode {mode:03o}"))
}

/// The owner's write bit off `path`, and what its refusal says.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn shut_its_owner_out(path: &Path) -> io::Result<String> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        chmod(path, 0o500)?;
        Ok("has mode 500, which does not let its owner read, enter and write in it".to_owned())
    } else {
        chmod(path, 0o400)?;
        Ok("has mode 400, which does not let its owner read and write it".to_owned())
    }
}

/// `0300`: written in and entered by its owner, and not listed.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn hide_its_entries(directory: &Path) -> io::Result<()> {
    chmod(directory, 0o300)
}

/// `0000`: read by nobody, its owner included.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn make_unreadable(path: &Path) -> io::Result<()> {
    chmod(path, 0o000)
}

/// Nothing: a file this process creates is owned by the user it runs as, so
/// there is no other owner to give it.
#[cfg(any(test, feature = "test-support"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the interface's answer: the Windows half says what setting the owner said"
)]
pub(crate) const fn owned_as_this_process_creates(_path: &Path) -> io::Result<bool> {
    Ok(false)
}

/// A symbolic link at `link` to `target`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn link_file(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// A symbolic link at `link` to the directory `target`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn link_directory(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// A symbolic link at `link` to the directory `target`: the one link a
/// directory takes here, as [`link_directory`] makes it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn symlink_directory(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// The device and the inode of `path`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn identity(path: &Path) -> io::Result<(u64, u64)> {
    let found = std::fs::symlink_metadata(path)?;
    Ok((found.dev(), found.ino()))
}

/// Whether a lock is held on `directory` now: one taken here would have to
/// wait.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn is_held(directory: &Path, _name: &str) -> bool {
    std::fs::File::open(directory)
        .is_ok_and(|handle| matches!(handle.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
}

/// The next user number: nobody this process is.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const fn someone_else(user: &User) -> User {
    User(user.0.wrapping_add(1))
}

/// Whether a child process ended the way `std::process::abort` ends one
/// here: by `SIGABRT`, signal 6.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn stopped_by_abort(status: std::process::ExitStatus) -> bool {
    std::os::unix::process::ExitStatusExt::signal(&status) == Some(6)
}

/// One mode a path is given, to be judged under it.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub(crate) struct Opening(u32);

/// Gives `path` the opening's mode, and answers how a refusal names it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn apply(opening: &Opening, path: &Path) -> io::Result<String> {
    chmod(path, opening.0)?;
    Ok(format!("has mode {:03o}", opening.0))
}

/// The private mode of `kind` with each of the six group and other bits,
/// alone: not only the loud ones.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn every_way_another_user_can_reach(kind: Kind) -> Vec<Opening> {
    let private = match kind {
        Kind::Directory => OWNER_OF_A_DIRECTORY,
        Kind::File => OWNER_OF_A_FILE,
    };
    [0o040, 0o020, 0o010, 0o004, 0o002, 0o001]
        .into_iter()
        .map(|bit| Opening(private | bit))
        .collect()
}

/// Each bit the owner of a `kind` needs, alone, taken away: a file read and
/// written, a directory read, entered and written in.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn every_way_its_owner_lacks(kind: Kind) -> Vec<Opening> {
    match kind {
        Kind::File => vec![Opening(0o400), Opening(0o200)],
        Kind::Directory => vec![Opening(0o600), Opening(0o500), Opening(0o300)],
    }
}

/// None: a mode grants or withholds each bit, and has no way to deny one
/// that leaves the path private with everything its owner needs.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const fn every_harmless_restriction(_kind: Kind) -> Vec<Opening> {
    Vec::new()
}
