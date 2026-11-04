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

/// What a path lets in, read so that two readings compare: on Unix its mode
/// bits; on Windows its owner and every entry of its access list.
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

/// A user that is not `user`: what a path another user owns is judged
/// against, since one cannot be made without the superuser.
#[must_use]
pub fn someone_else(user: &User) -> User {
    User(imp::someone_else(&user.0))
}

/// One way a path can be not this user's alone, staged on demand.
#[derive(Debug, Clone)]
pub struct Opening(imp::Opening);

impl Opening {
    /// Stages this on `path`, and answers the words its refusal says.
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
/// an entry letting a group in, and, on a directory, one only what is made
/// inside it inherits.
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
/// alone, and an entry granting less than all of them.
#[must_use]
pub fn every_way_its_owner_lacks(kind: Kind) -> Vec<Opening> {
    imp::every_way_its_owner_lacks(kind)
        .into_iter()
        .map(Opening)
        .collect()
}
