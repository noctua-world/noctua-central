//! The doors a suite stages a path through, on every system the same way:
//! **for tests**, behind `test-support`.
//!
//! **One body for every system.** A test asks for a path another user can
//! reach, or one its owner cannot write, and is handed back the words the
//! refusal names it by on this system; it asks what a path lets in and
//! compares two answers, never a mode or an access list of its own. So a test
//! written once is the Unix test and its Windows twin, and the two cannot
//! drift apart.

#![cfg(any(test, feature = "test-support"))]

use std::io;
use std::path::Path;

use crate::{Kind, User, imp};

/// Whether [`crate::hold`] holds a directory through the lock file it names:
/// true on Windows, which locks no directory.
///
/// Where it does, the file is created private or judged as any file of the
/// user's; where it does not, nothing at that name is made or read.
pub const HOLDS_THROUGH_A_FILE: bool = imp::HOLDS_THROUGH_A_FILE;

/// The name Python 3 answers to on this system, for a suite that drives a
/// script in it: `python3` on Unix and `python` on Windows.
pub const PYTHON: &str = imp::PYTHON;

/// A process this system runs under an account of its own from its boot on,
/// which a test reads as another account's.
///
/// It is process 1 on Unix, `init` or `launchd`, and on Windows process 4,
/// the one named `System`.
pub const A_PROCESS_OF_ANOTHER_ACCOUNT: u32 = imp::A_PROCESS_OF_ANOTHER_ACCOUNT;

/// What a path lets in, read so that two readings compare: on Unix its mode
/// bits; on Windows its owner, every entry of its access list and its
/// read-only mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access(imp::Access);

/// What a path lets in now.
///
/// # Errors
///
/// [`io::Error`] as the system answered it, when it could not be read.
pub fn access(path: &Path) -> io::Result<Access> {
    imp::access(path).map(Access)
}

/// Whether `path` is exactly what this crate creates.
///
/// A directory or a file private to this user, as
/// [`crate::create_private_directory`] and [`crate::create_private`] make
/// one, and nothing else.
#[must_use]
pub fn is_as_created(path: &Path) -> bool {
    imp::is_as_created(path)
}

/// Whether `path` lets in this user alone, with everything — what SQLite
/// creates beside a database in a directory this crate made is this, though
/// not made by this crate.
#[must_use]
pub fn is_private(path: &Path) -> bool {
    imp::is_private(path)
}

/// Makes `path` private to this user again, as this crate creates one.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn make_private(path: &Path) -> io::Result<()> {
    imp::make_private(path)
}

/// Lets another user reach `path`, and answers the words a refusal of it
/// names that by: on Unix its mode (`mode 750`, `mode 604`); on Windows the
/// group let in.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn open_to_another_user(path: &Path) -> io::Result<String> {
    imp::open_to_another_user(path)
}

/// Takes from the owner of `path` what the store needs to write it — and
/// write in it, for a directory — and answers the words a refusal of it says.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn shut_its_owner_out(path: &Path) -> io::Result<String> {
    imp::shut_its_owner_out(path)
}

/// Takes from the owner of `directory` the right to list it, and
/// leaves it written in and entered: on Unix `0300`.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn hide_its_entries(directory: &Path) -> io::Result<()> {
    imp::hide_its_entries(directory)
}

/// Takes from the owner of the file `path` the right to read it, which
/// [`make_private`] gives back: on Unix `0000`; on Windows a deny entry.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn make_unreadable(path: &Path) -> io::Result<()> {
    imp::make_unreadable(path)
}

/// Gives `path` the owner the system gives what this process creates, and
/// answers whether that owner is somebody other than this user — on Windows,
/// in an elevated administrator's process, the Administrators group; nowhere
/// else.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn owned_as_this_process_creates(path: &Path) -> io::Result<bool> {
    imp::owned_as_this_process_creates(path)
}

/// A link at `link` to the file `target`, which need not exist.
///
/// # Errors
///
/// [`io::Error`] as the system answered it: on Windows, for a process that
/// may not make symbolic links.
pub fn link_file(target: &Path, link: &Path) -> io::Result<()> {
    imp::link_file(target, link)
}

/// A link at `link` to the directory `target`: a symbolic link on Unix, and
/// a junction on Windows, which needs no privilege to make.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn link_directory(target: &Path, link: &Path) -> io::Result<()> {
    imp::link_directory(target, link)
}

/// A symbolic link at `link` to the directory `target`, on every system.
///
/// On Windows a directory's symbolic link is a reparse point of another tag
/// than the junction [`link_directory`] makes there, and needs the privilege
/// to make symbolic links; on Unix the two doors make one link.
///
/// # Errors
///
/// [`io::Error`] as the system answered it: on Windows, for a process that
/// may not make symbolic links.
pub fn symlink_directory(target: &Path, link: &Path) -> io::Result<()> {
    imp::symlink_directory(target, link)
}

/// Which file `path` is, as two readings compare: the same answer is the same
/// file, kept or renamed, and another is a file made in its place.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn identity(path: &Path) -> io::Result<(u64, u64)> {
    imp::identity(path)
}

/// Whether something holds `directory`, as [`crate::hold`] would, now.
#[must_use]
pub fn is_held(directory: &Path, name: &str) -> bool {
    imp::is_held(directory, name)
}

/// The session the process `pid` is in, as the system numbers it, or nothing
/// where this system has no session of the POSIX kind.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn session_of(pid: u32) -> io::Result<Option<u32>> {
    imp::session_of(pid)
}

/// The process group the process `pid` is in, as the system numbers it, or
/// nothing where this system has no group of the POSIX kind.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn group_of(pid: u32) -> io::Result<Option<u32>> {
    imp::group_of(pid)
}

/// A user that is not `user`: what a path another user owns is judged
/// against, since one cannot be made without the superuser.
#[must_use]
pub fn someone_else(user: &User) -> User {
    User(imp::someone_else(&user.0))
}

/// Whether a child process ended as `std::process::abort` ends one on this
/// system: by `SIGABRT` on Unix, and through `__fastfail` on Windows.
#[must_use]
pub fn stopped_by_abort(status: std::process::ExitStatus) -> bool {
    imp::stopped_by_abort(status)
}

/// One way a path can be not this user's alone, or a restriction that leaves
/// it so, staged on demand.
#[derive(Debug, Clone)]
pub struct Opening(imp::Opening);

impl Opening {
    /// Stages this on `path`, and answers the words its refusal says — for
    /// a harmless restriction, which no refusal names, what was staged.
    ///
    /// # Errors
    ///
    /// [`io::Error`] as the system answered it.
    pub fn apply(&self, path: &Path) -> io::Result<String> {
        imp::apply(&self.0, path)
    }
}

/// Every way another user can reach a path of `kind`, one at a time.
///
/// On Unix each group and other bit alone; on Windows no access list at all,
/// an entry letting a group in, an entry of a type no rule there reads, and,
/// on a directory, one only what is made inside it inherits.
#[must_use]
pub fn every_way_another_user_can_reach(kind: Kind) -> Vec<Opening> {
    imp::every_way_another_user_can_reach(kind)
        .into_iter()
        .map(Opening)
        .collect()
}

/// Every way the owner of a path of `kind` can lack what the store needs.
///
/// One at a time: on Unix each owner bit alone; on Windows each right denied
/// alone, an entry granting less than all of them, on a directory every right
/// granted only to what is made inside it, and on a file the read-only mark.
#[must_use]
pub fn every_way_its_owner_lacks(kind: Kind) -> Vec<Opening> {
    imp::every_way_its_owner_lacks(kind)
        .into_iter()
        .map(Opening)
        .collect()
}

/// Every restriction a path of `kind` can carry that keeps it private and
/// takes nothing the store needs, so a path under it is still judged private.
///
/// One at a time: on Windows a deny entry for `Everyone` of a right the store
/// never asks of the path, and on a directory one that only the files made
/// inside it inherit; on Unix none, since a mode has no way to deny.
#[must_use]
pub fn every_harmless_restriction(kind: Kind) -> Vec<Opening> {
    imp::every_harmless_restriction(kind)
        .into_iter()
        .map(Opening)
        .collect()
}

/// Whether an IPC endpoint is a name on disk that a bind judges, which it is
/// on Unix alone.
///
/// **A socket's path on Unix; on Windows a pipe's name is the system's**, and
/// nothing is made at the socket's path there.
pub const ENDPOINT_ON_DISK: bool = imp::ENDPOINT_ON_DISK;

/// Whether `listener`'s endpoint is made as this crate makes one private, read
/// back from the system.
///
/// **On Unix a socket of the listener's user, mode `0600`**, read from the
/// socket's file; on Windows a pipe owned by the user whose protected access
/// list holds one entry, the user with every right, read from the instance
/// waiting for the next client.
#[must_use]
pub fn endpoint_is_as_created(listener: &crate::ipc::Listener) -> bool {
    imp::endpoint_is_as_created(&listener.inner, &listener.user.0)
}

/// What a daemon stopped without removing its endpoint leaves at it: on Unix
/// a socket nothing listens on; on Windows nothing, since a pipe goes with
/// the last of its instances.
///
/// # Errors
///
/// [`io::Error`] as the system answered it.
pub fn leave_a_dead_endpoint(endpoint: &crate::ipc::Endpoint) -> io::Result<()> {
    imp::leave_a_dead_endpoint(endpoint)
}

/// What a bind makes of what stands at `endpoint`'s name for `user`, before
/// it asks whether anything answers there: on Unix the very judgement a bind
/// makes of the socket's path, so a test can hand it a user it cannot be; on
/// Windows nothing, since a pipe's name is the system's.
///
/// # Errors
///
/// [`crate::ipc::Refused::NotPrivate`] for anything at the name but a socket
/// of `user`'s, which a bind leaves as it was, and
/// [`crate::ipc::Refused::Failed`] where the name could not be read.
pub fn judge_what_stands_at(
    endpoint: &crate::ipc::Endpoint,
    user: &User,
) -> Result<(), crate::ipc::Refused> {
    imp::judge_what_stands_at(endpoint, &user.0)
}
