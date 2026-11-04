//! What makes a path this user's alone, on the system this was built for: who
//! this process runs as, a directory and a file created private, an existing
//! path judged, a profile held for one open, and a directory's entries made
//! durable (`NOCTUA.md §2.7`).
//!
//! **One interface, an implementation per system, and nothing outside it asks
//! which system it runs on.** `unix.rs` is the Unix half, which macOS takes
//! too: modes and owners through `std::os::unix`. `windows.rs` is the Windows
//! half: the token's user and the access lists of the Win32 security calls.
//! The local database's open (`nc_local`) and the kernel's blob directory are
//! the two callers, and this crate is what both need, and no more.
//!
//! # What private is, on each system
//!
//! - **Who.** Unix: the user that owns a probe file this process makes in the
//!   directory, since the standard library has no safe way to ask. Windows:
//!   the user the process token names (`TokenUser`) — never the owner of a new
//!   file, which in an elevated administrator's process is the Administrators
//!   group.
//! - **A directory or a file created.** Unix: `0700` and `0600`, the file
//!   with `O_CREAT | O_EXCL`, which follows no link. Windows: owned by that
//!   user, with a protected access list — nothing inherited from the parent —
//!   holding one entry, the user with full control; on a directory that entry
//!   is inherited by what is created in it, so the files SQLite makes beside
//!   the database are the user's alone too. A file is created with
//!   `CREATE_NEW` and `FILE_FLAG_OPEN_REPARSE_POINT`, so an existing name, a
//!   link included, is refused and never followed.
//! - **A path judged** — refused, and left exactly as it was — when it is a
//!   link (Unix: by its own metadata; Windows: any reparse point, opened as
//!   itself), not the kind expected, owned by another user, reachable by
//!   another user (Unix: a group or other bit; Windows: no access list at all,
//!   or an entry letting anybody but the user in, or one of a kind this does
//!   not read), or without what its owner needs (Unix: the owner's bits;
//!   Windows: what the user's entries grant, less what any deny entry takes —
//!   a file read, write and delete, a directory list, traverse, add a file,
//!   add a directory and delete a child — and a file not marked read-only).
//! - **One open of a profile at a time.** Unix: an exclusive lock on the
//!   directory, because POSIX lets every lock on a file go when any descriptor
//!   of it closes. Windows locks no directory, so a private file in it is
//!   locked instead — never the database file.
//! - **A rename made durable.** Unix: the directory is synced. Windows: there
//!   is no directory handle to sync, and NTFS journals its metadata, so
//!   nothing is done.
//!
//! # What this answers (`§2.4`)
//!
//! Another account on this machine reading, replacing or redirecting what the
//! local database and the blobs keep: what is created is created private in
//! one call, with no moment in which it is anybody else's, and what is found
//! is refused unless it already is. **`unsafe` is written here and nowhere
//! else in this tree**, in `windows.rs` alone, each block beside the reason
//! every pointer it hands the system is valid and where what the system
//! allocated is freed.
//!
//! # What it does not
//!
//! A process running as the same user, which can change any of this between
//! a judgement and the open that follows it; and whoever the system lets past
//! every access list — root on Unix; on Windows the Administrators group,
//! `SYSTEM` and a process holding the backup or take-ownership privilege. On
//! Windows, a file another program made with the system's own security —
//! what SQLite creates beside the database, judged by
//! [`judge_made_by_default`] alone — is taken as this user's when it is owned
//! by the group the token makes the owner of what this process creates: the
//! Administrators group, in an elevated process, whose members can take any
//! file already. Nothing this crate creates is judged that way, since it
//! names the user as the owner of each itself.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's lints.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod testing;
mod unix;
// **The one module of this tree that writes `unsafe`**, and the one place it
// is allowed: the Win32 security calls take and hand back raw pointers, and
// `windows-sys` declares each of them `unsafe`. Every block in it carries a
// `// SAFETY:` comment, and the gate that refuses `unsafe` everywhere else
// names this file as its one exception.
#[allow(
    unsafe_code,
    reason = "the Win32 security calls are `unsafe` to make, and this module is where they are \
              made; each block says why its pointers are valid and what it frees"
)]
mod windows;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

#[cfg(not(any(unix, windows)))]
compile_error!("nc-platform has a Unix half and a Windows half, and this target is neither");

use std::path::{Path, PathBuf};

/// What a path is expected to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A directory, which is listed, entered and written in.
    Directory,
    /// A regular file, which is read and written.
    File,
}

/// The user this process runs as, as this system names one.
///
/// **Found by [`user_in`], and nothing else.** On Unix the answer needs a
/// directory to make a probe in, so there is no answer apart from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User(imp::User);

impl core::fmt::Display for User {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Why a path is not this user's alone: the path, and what it is.
///
/// Its text reads `` `<path>` <what it is> `` — `is a symbolic link`, `has
/// mode 750, which another user can reach` — so a caller ends the sentence
/// with what it did not open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    path: PathBuf,
    what: String,
}

impl Refusal {
    /// The refusal of `path` for `what` it is.
    fn of(path: &Path, what: String) -> Self {
        Self {
            path: path.to_owned(),
            what,
        }
    }
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "`{}` {}", self.path.display(), self.what)
    }
}

/// A call of the filesystem that failed: what was being done, and its answer.
#[derive(Debug)]
pub struct Failed {
    doing: String,
    error: std::io::Error,
}

impl Failed {
    /// What was being done: `` `<path>` could not be read ``.
    #[must_use]
    pub fn doing(&self) -> &str {
        &self.doing
    }

    /// What the system answered.
    #[must_use]
    pub const fn error(&self) -> &std::io::Error {
        &self.error
    }

    /// `doing`, which failed with `error`.
    fn new(doing: String, error: std::io::Error) -> Self {
        Self { doing, error }
    }
}

impl core::fmt::Display for Failed {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}: {}", self.doing, self.error)
    }
}

/// Why a path was not taken.
#[derive(Debug)]
pub enum Judged {
    /// It is not this user's alone.
    Refused(Refusal),
    /// The filesystem could not answer for it — a path that is not there is
    /// one, with [`std::io::ErrorKind::NotFound`].
    Failed(Failed),
}

impl From<Failed> for Judged {
    fn from(failed: Failed) -> Self {
        Self::Failed(failed)
    }
}

impl From<Refusal> for Judged {
    fn from(refused: Refusal) -> Self {
        Self::Refused(refused)
    }
}

impl core::fmt::Display for Judged {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Refused(refused) => write!(formatter, "{refused}"),
            Self::Failed(failed) => write!(formatter, "{failed}"),
        }
    }
}

/// A profile held for one open, until this is dropped.
#[derive(Debug)]
pub struct Held(#[expect(dead_code, reason = "held for its drop alone")] std::fs::File);

/// What a private directory is on this system, as a sentence says it: "the
/// directory is …".
pub const PRIVATE_DIRECTORY: &str = imp::PRIVATE_DIRECTORY;

/// What a private file is on this system, as a sentence says it: "the files
/// are …".
pub const PRIVATE_FILE: &str = imp::PRIVATE_FILE;

/// Whether `fsync` here stops short of the disk, so a database asked to sync
/// in full must be asked for more: true on macOS, where `F_FULLFSYNC` is
/// what reaches the platter.
pub const FSYNC_STOPS_SHORT_OF_THE_DISK: bool = cfg!(target_os = "macos");

/// Whether [`sync_directory`] syncs anything here, and so can fail.
///
/// False on Windows, where a directory has no handle to sync: a rename it was
/// asked to make durable is as durable as the volume makes it.
pub const SYNCS_A_DIRECTORY: bool = imp::SYNCS_A_DIRECTORY;

/// The user this process runs as, once `directory` is judged private to its
/// own owner and then to that user.
///
/// **The directory's shape before anything is written in it**: on Unix the
/// answer is a probe file made there, and a directory another user can reach
/// is not one to create anything in.
///
/// # Errors
///
/// [`Judged::Refused`] when the directory is not this user's alone, and
/// [`Judged::Failed`] when it could not be read or the user could not be
/// asked — on Unix, a directory this process may not write in.
pub fn user_in(directory: &Path) -> Result<User, Judged> {
    imp::user_in(directory).map(User)
}

/// Whether `path` is a `kind`, and this user's alone with everything its owner
/// needs; it is left exactly as it was either way.
///
/// **A link is refused however it points**: what it points at is decided by
/// whoever can change the link, and nothing about the target says who that is.
///
/// # Errors
///
/// [`Judged::Refused`] for each way it is not, and [`Judged::Failed`] when it
/// could not be read — with [`std::io::ErrorKind::NotFound`] when nothing is
/// there.
pub fn judge(path: &Path, kind: Kind, user: &User) -> Result<(), Judged> {
    imp::judge(path, kind, &user.0)
}

/// Whether the file `path`, which another program made in a private directory
/// with the security the system gives a new file — SQLite's `-wal`, `-shm`
/// and `-journal` — is this user's alone with everything its owner needs; it
/// is left exactly as it was either way.
///
/// **As [`judge`] holds a file, but for one owner more**: the one the token
/// gives what this process creates, which on Windows, in an elevated
/// administrator's process, is the Administrators group and not the user. On
/// Unix that owner is the user, so the two are one rule there.
///
/// # Errors
///
/// [`Judged::Refused`] and [`Judged::Failed`], as [`judge`] answers them.
pub fn judge_made_by_default(path: &Path, user: &User) -> Result<(), Judged> {
    imp::judge_made_by_default(path, &user.0)
}

/// Creates `directory`, one level, private to this user from its first
/// moment.
///
/// One level, never its parents: a parent that does not exist is a path
/// nobody meant, and a parent made on the way would not be private.
///
/// # Errors
///
/// What the system answered — [`std::io::ErrorKind::AlreadyExists`] for a name
/// that is there, a link included.
pub fn create_private_directory(directory: &Path) -> std::io::Result<()> {
    imp::create_private_directory(directory)
}

/// A new file at `path`, private to this user from its first moment, open to
/// write.
///
/// **New, never truncated, never through a link**: any name that is there —
/// a link to anything at all included — is refused rather than opened.
///
/// # Errors
///
/// What the system answered — [`std::io::ErrorKind::AlreadyExists`] for a name
/// that is there.
pub fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    imp::create_private(path)
}

/// `directory`, held for one open until the answer is dropped: an exclusive
/// lock every other hold of it waits for.
///
/// **Waited for in place**, so it belongs on a thread that may block. Where
/// this system cannot lock a directory, the lock is the file `name` in it,
/// created private or judged as any file of `user`'s is; where it can, `name`
/// is not made.
///
/// # Errors
///
/// [`Judged::Refused`] for a lock file that is not this user's alone, and
/// [`Judged::Failed`] when the directory or its lock could not be opened or
/// held.
pub fn hold(directory: &Path, name: &str, user: &User) -> Result<Held, Judged> {
    imp::hold(directory, name, &user.0).map(Held)
}

/// Makes the entries of `directory` durable — a rename into it, above all —
/// where this system makes a directory durable by syncing it.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it, when the directory could not
/// be opened or synced.
pub fn sync_directory(directory: &Path) -> std::io::Result<()> {
    imp::sync_directory(directory)
}

/// Sets the time the file `path` was last written to `at`, and leaves its
/// bytes as they are.
///
/// **Through a handle that may set it**: on Windows one opened to read may
/// not, so a file named again by opening it to read would be refused there.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it, when the file could not be
/// opened or its time set.
pub fn set_modified(path: &Path, at: std::time::SystemTime) -> std::io::Result<()> {
    imp::set_modified(path, at)
}

/// How the name of a probe that says who this process is begins.
///
/// **One name per call, not one per directory.** With one fixed name, every
/// call would remove whatever stood under it as a crashed run's leftover — so
/// two opens of one profile at once would remove each other's probe. Measured:
/// four threads opening an existing profile a hundred times each would fail
/// 175 of 400 opens, as `File exists` and as `No such file or directory`, and
/// the first of those would reach the user as `NC-DB-0006`. SQLite's WAL lets a
/// second app instance or a command-line tool open the file beside the daemon,
/// so the name carries the process, the time and a count, and a call removes
/// only the probe it made. One a crash leaves between the two is an empty file
/// nothing reads.
#[cfg_attr(
    all(windows, not(test)),
    expect(
        dead_code,
        reason = "the Windows half asks the token, and makes no probe"
    )
)]
const PROBE: &str = ".nc-local-owner-probe";

/// How many probes this process has made, so two made at once — two threads,
/// two pools — never carry the same name.
#[cfg_attr(
    all(windows, not(test)),
    expect(
        dead_code,
        reason = "the Windows half asks the token, and makes no probe"
    )
)]
static PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A name for a probe that no other call makes, in this process or another.
///
/// The process, because two processes count from zero; the time, because a
/// process id is reused once its owner is gone; the count, because two threads
/// read one clock.
#[cfg_attr(
    all(windows, not(test)),
    expect(
        dead_code,
        reason = "the Windows half asks the token, and makes no probe"
    )
)]
fn probe_name() -> String {
    let made = PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{PROBE}-{}-{now}-{made}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::{Judged, Kind, PROBE, judge, probe_name, testing, user_in};

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Ok(found) = std::fs::symlink_metadata(&self.0)
                && found.is_dir()
            {
                let _ = testing::make_private(&self.0);
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A directory of its own, made private as this crate makes one.
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-platform-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            super::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// The words a refusal of `path` said, or why there was none.
    fn refused(verdict: Result<(), Judged>) -> String {
        match verdict {
            Err(Judged::Refused(refused)) => refused.to_string(),
            other => panic!("refused, and it was {other:?}"),
        }
    }

    /// Every probe this process makes carries a name of its own.
    ///
    /// **One fixed name shared by every open** would have each remove whatever
    /// stood under it, so two opens at once would remove each other's probe —
    /// measured, 175 of 400 concurrent opens of one profile failing. A name
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

    /// A directory and a file are created as this crate makes them, and a
    /// second create of either name is refused and changes nothing.
    #[test]
    fn what_is_created_is_private_and_a_name_that_is_there_is_refused() {
        let scratch = Scratch::new("created");
        assert!(testing::is_as_created(&scratch.0), "the directory");
        let file = scratch.0.join("file");
        drop(super::create_private(&file).expect("a new file"));
        assert!(testing::is_as_created(&file), "the file");
        let before = testing::access(&file).expect("readable");
        let again = super::create_private(&file).expect_err("the name is there");
        assert_eq!(again.kind(), std::io::ErrorKind::AlreadyExists, "{again}");
        assert_eq!(testing::access(&file).expect("readable"), before);
        let directory = scratch.0.join("directory");
        super::create_private_directory(&directory).expect("one level");
        assert!(testing::is_as_created(&directory), "the inner directory");
        let again = super::create_private_directory(&directory).expect_err("there");
        assert_eq!(again.kind(), std::io::ErrorKind::AlreadyExists, "{again}");
        let missing = scratch.0.join("no-such-parent").join("directory");
        assert_eq!(
            super::create_private_directory(&missing)
                .expect_err("one level only")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    /// A link where a file would be created is refused, and what it points at
    /// is neither made nor changed.
    #[test]
    fn a_file_is_never_created_through_a_link() {
        let scratch = Scratch::new("through-a-link");
        let target = scratch.0.join("target");
        let link = scratch.0.join("link");
        testing::link_file(&target, &link).expect("a dangling link");
        let refused = super::create_private(&link).expect_err("a link is a name that is there");
        assert_eq!(
            refused.kind(),
            std::io::ErrorKind::AlreadyExists,
            "{refused}"
        );
        assert!(
            std::fs::symlink_metadata(&target).is_err(),
            "a file was made where the link points"
        );
    }

    /// The user found in two directories is one user, and a path of its own
    /// that this crate made is judged its own.
    #[test]
    fn the_user_is_one_user_and_what_this_crate_made_is_its_own() {
        let first = Scratch::new("user-first");
        let second = Scratch::new("user-second");
        let user = user_in(&first.0).expect("the scratch is private");
        assert_eq!(user_in(&second.0).expect("private too"), user);
        let file = first.0.join("file");
        drop(super::create_private(&file).expect("a new file"));
        judge(&file, Kind::File, &user).expect("its own file");
        judge(&first.0, Kind::Directory, &user).expect("its own directory");
        assert!(
            std::fs::read_dir(&first.0)
                .expect("readable")
                .all(|entry| entry.expect("an entry").file_name() == "file"),
            "a probe was left behind"
        );
    }

    /// Each way a path is not private is a refusal of its own.
    ///
    /// Owner, kind, each way another user can reach it and each thing its
    /// owner needs and does not hold; and the path is left as it was by each.
    #[test]
    fn a_path_is_refused_for_each_way_it_is_not_private() {
        let scratch = Scratch::new("judge");
        let user = user_in(&scratch.0).expect("private");
        let someone_else = testing::someone_else(&user);
        let refused_for = refused(judge(&scratch.0, Kind::Directory, &someone_else));
        assert!(refused_for.contains("belongs to user"), "{refused_for}");
        let not_a_file = refused(judge(&scratch.0, Kind::File, &user));
        assert!(not_a_file.contains("not a regular file"), "{not_a_file}");
        let file = scratch.0.join("f");
        drop(super::create_private(&file).expect("a file"));
        judge(&file, Kind::File, &user).expect("private");
        let not_a_directory = refused(judge(&file, Kind::Directory, &user));
        assert!(
            not_a_directory.contains("not a directory"),
            "{not_a_directory}"
        );

        for (path, kind) in [(&file, Kind::File), (&scratch.0, Kind::Directory)] {
            for opening in testing::every_way_another_user_can_reach(kind) {
                let said = opening.apply(path).expect("staged");
                let before = testing::access(path).expect("readable");
                let verdict = judge(path, kind, &user);
                let after = testing::access(path).expect("readable");
                testing::make_private(path).expect("private again");
                let refused = refused(verdict);
                assert!(
                    refused.contains(&said) && refused.contains("another user can reach"),
                    "{kind:?}, {said}: {refused}"
                );
                assert_eq!(after, before, "{said}: a refused path was changed");
            }
            for lacking in testing::every_way_its_owner_lacks(kind) {
                let said = lacking.apply(path).expect("staged");
                let verdict = judge(path, kind, &user);
                testing::make_private(path).expect("private again");
                let refused = refused(verdict);
                assert!(
                    refused.contains(&said) && refused.contains("does not let its owner"),
                    "{kind:?}, {said}: {refused}"
                );
            }
        }
    }

    /// A path whose list takes away only what the store never needs is still
    /// private, and is judged so and left as it was.
    ///
    /// **A deny entry takes nothing from the rule**, and one only what is made
    /// inside a directory inherits takes nothing from the directory: a judge
    /// that refused either would refuse a path that is this user's alone.
    #[test]
    fn a_restriction_that_takes_nothing_the_store_needs_is_accepted() {
        let scratch = Scratch::new("restricted");
        let user = user_in(&scratch.0).expect("private");
        let file = scratch.0.join("f");
        drop(super::create_private(&file).expect("a file"));
        for (path, kind) in [(&file, Kind::File), (&scratch.0, Kind::Directory)] {
            for restriction in testing::every_harmless_restriction(kind) {
                let said = restriction.apply(path).expect("staged");
                let before = testing::access(path).expect("readable");
                let verdict = judge(path, kind, &user);
                let found = (kind == Kind::Directory).then(|| user_in(path));
                let after = testing::access(path).expect("readable");
                testing::make_private(path).expect("private again");
                assert!(verdict.is_ok(), "{kind:?}, {said}: {verdict:?}");
                if let Some(found) = found {
                    let found = found.unwrap_or_else(|judged| panic!("{said}: {judged}"));
                    assert_eq!(found, user, "{said}: its user");
                }
                assert_eq!(after, before, "{said}: a judged path was changed");
            }
        }
    }

    /// A link is refused as a link, whatever it points at: a file, a
    /// directory, or nothing.
    ///
    /// **A directory by both doors**, since on Windows a junction and a
    /// directory's symbolic link are reparse points of two tags.
    #[test]
    fn a_link_is_refused_however_it_points() {
        let scratch = Scratch::new("links");
        let user = user_in(&scratch.0).expect("private");
        let file = scratch.0.join("file");
        drop(super::create_private(&file).expect("a file"));
        let directory = scratch.0.join("directory");
        super::create_private_directory(&directory).expect("a directory");
        let to_a_file = scratch.0.join("to-a-file");
        testing::link_file(&file, &to_a_file).expect("a link");
        let to_a_directory = scratch.0.join("to-a-directory");
        testing::link_directory(&directory, &to_a_directory).expect("a link");
        let symlinked = scratch.0.join("symlinked-to-a-directory");
        testing::symlink_directory(&directory, &symlinked).expect("a symbolic link");
        let to_nothing = scratch.0.join("to-nothing");
        testing::link_file(&scratch.0.join("nothing"), &to_nothing).expect("a link");
        for (link, kind) in [
            (&to_a_file, Kind::File),
            (&to_a_directory, Kind::Directory),
            (&symlinked, Kind::Directory),
            (&to_nothing, Kind::File),
        ] {
            let refused = refused(judge(link, kind, &user));
            assert!(refused.contains("symbolic link"), "{refused}");
        }
        for link in [&to_a_directory, &symlinked] {
            let refused = match user_in(link) {
                Err(Judged::Refused(refused)) => refused.to_string(),
                other => panic!("refused, and it was {other:?}"),
            };
            assert!(refused.contains("symbolic link"), "{refused}");
        }
    }

    /// A path that is not there is a failure that says so, not a refusal.
    #[test]
    fn a_path_that_is_not_there_is_not_found() {
        let scratch = Scratch::new("missing");
        let user = user_in(&scratch.0).expect("private");
        match judge(&scratch.0.join("missing"), Kind::File, &user) {
            Err(Judged::Failed(failed)) => {
                assert_eq!(
                    failed.error().kind(),
                    std::io::ErrorKind::NotFound,
                    "{failed}"
                );
            }
            other => panic!("not found, and it was {other:?}"),
        }
    }

    /// A directory held once is held: a second hold waits until the first
    /// lets go, and the lock leaves nothing that is not private.
    ///
    /// **And a lock file only where a file is what holds the directory**: a
    /// hold that locks the directory itself makes nothing at the name it is
    /// handed, and one that did would leave a file in the profile that the
    /// suites reading it by name would take for the lock.
    #[test]
    fn a_directory_held_is_held_until_it_is_let_go() {
        let scratch = Scratch::new("held");
        let user = user_in(&scratch.0).expect("private");
        let held = super::hold(&scratch.0, "lock", &user).expect("held");
        assert!(
            testing::is_held(&scratch.0, "lock"),
            "held, and nothing holds it"
        );
        assert_eq!(
            std::fs::symlink_metadata(scratch.0.join("lock")).is_ok(),
            testing::HOLDS_THROUGH_A_FILE,
            "a lock file is made where a file is the hold, and only there"
        );
        let (took, taken) = std::sync::mpsc::channel();
        let waiting = {
            let (directory, user) = (scratch.0.clone(), user.clone());
            std::thread::spawn(move || {
                let second = super::hold(&directory, "lock", &user).expect("held in turn");
                took.send(()).expect("the test waits");
                drop(second);
            })
        };
        assert!(
            taken
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "a second hold did not wait for the first"
        );
        drop(held);
        taken
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the second hold went on once the first let go");
        waiting.join().expect("the second hold ended");
        assert!(
            !testing::is_held(&scratch.0, "lock"),
            "nothing holds it now"
        );
        let mut left = 0_usize;
        for entry in std::fs::read_dir(&scratch.0).expect("readable") {
            let path = entry.expect("an entry").path();
            judge(&path, Kind::File, &user).expect("what the lock leaves is private");
            left += 1;
        }
        assert_eq!(
            left,
            usize::from(testing::HOLDS_THROUGH_A_FILE),
            "two holds left more than their one lock file, or one where none is the hold"
        );
    }

    /// A lock file that is not the user's alone is refused, and left as it
    /// was, where a file is what holds a directory; elsewhere the name is not
    /// the hold's, and is left alone.
    ///
    /// **The lock file is judged as the database's file is**: one planted at
    /// its name — a link, or a file another user can reach — would otherwise
    /// be opened and locked, and whatever it points at or whoever reaches it
    /// decide when the profile is held.
    #[test]
    fn a_lock_file_that_is_not_its_own_is_refused_where_a_file_holds_the_directory() {
        let scratch = Scratch::new("lock-file");
        let user = user_in(&scratch.0).expect("private");
        let target = scratch.0.join("target");
        drop(super::create_private(&target).expect("a file"));
        testing::link_file(&target, &scratch.0.join("linked")).expect("a link");
        let reachable = scratch.0.join("reachable");
        drop(super::create_private(&reachable).expect("a file"));
        let said = testing::open_to_another_user(&reachable).expect("opened up");
        for (name, says) in [("linked", "symbolic link"), ("reachable", said.as_str())] {
            let path = scratch.0.join(name);
            let as_it_stands = || {
                let found = std::fs::symlink_metadata(&path).expect("there");
                (
                    testing::access(&path).expect("readable"),
                    found.file_type().is_symlink(),
                    found.len(),
                )
            };
            let before = as_it_stands();
            let held = super::hold(&scratch.0, name, &user);
            assert_eq!(
                as_it_stands(),
                before,
                "{name}: the hold changed what stood there"
            );
            if testing::HOLDS_THROUGH_A_FILE {
                let refused = match held {
                    Err(Judged::Refused(refused)) => refused.to_string(),
                    other => panic!("{name}: refused, and it was {other:?}"),
                };
                assert!(refused.contains(says), "{name}: {refused}");
            } else {
                drop(held.expect("the directory is held, and the name is not the hold's"));
            }
        }
    }

    /// The owner the system gives what this process creates is taken as the
    /// user's for a file the system made alone, and refused on any path this
    /// crate made, which it made owned by the user itself.
    ///
    /// **Where that owner is the user — Unix, and Windows unelevated — the
    /// two rules are one**, and both take the file.
    #[test]
    fn the_owner_the_system_gives_is_taken_only_for_what_the_system_made() {
        let scratch = Scratch::new("default-owner");
        let user = user_in(&scratch.0).expect("private");
        let file = scratch.0.join("made-by-default");
        drop(super::create_private(&file).expect("a file"));
        let another = testing::owned_as_this_process_creates(&file).expect("owned");
        super::judge_made_by_default(&file, &user).expect("owned as the system makes it");
        match judge(&file, Kind::File, &user) {
            Ok(()) => assert!(
                !another,
                "a file owned by another than the user was its own"
            ),
            Err(Judged::Refused(refused)) => assert!(
                another && refused.to_string().contains("belongs to user"),
                "{refused}"
            ),
            Err(failed) => panic!("judged, and it was {failed:?}"),
        }
        assert!(
            matches!(
                super::judge_made_by_default(&scratch.0, &user),
                Err(Judged::Refused(_))
            ),
            "a directory is not a file the system made beside one"
        );
    }

    /// The time a file was last written is set, and its bytes are kept.
    #[test]
    fn a_files_time_is_set_and_its_bytes_are_kept() {
        use std::io::Write as _;

        let scratch = Scratch::new("modified");
        let file = scratch.0.join("file");
        super::create_private(&file)
            .expect("a file")
            .write_all(b"kept")
            .expect("written");
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        super::set_modified(&file, at).expect("set");
        let found = std::fs::symlink_metadata(&file).expect("there");
        assert_eq!(found.modified().expect("a time"), at);
        assert_eq!(std::fs::read(&file).expect("readable"), b"kept");
        assert!(testing::is_as_created(&file), "its access was changed");
    }

    /// A directory's entries are made durable without an error, wherever
    /// that means anything.
    #[test]
    fn a_directory_is_synced() {
        let scratch = Scratch::new("synced");
        super::sync_directory(&scratch.0).expect("synced");
    }
}
