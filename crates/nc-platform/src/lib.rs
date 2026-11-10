//! What makes a path this user's alone, on the system this was built for: who
//! this process runs as, a directory and a file created private, an existing
//! path judged, a profile held for one open, a directory's entries made
//! durable, and bytes from the system's own generator (`NOCTUA.md §2.7`).
//!
//! **One interface, an implementation per system.** No code outside it
//! decides what to do by the system it runs on, except `xtask`'s
//! `check::RUNS_THE_STACK`, which decides which gate steps apply there, and
//! two of the gate's tests that hold that answer to the system they run on;
//! `no_system_is_asked_outside_the_platform` in `xtask/src/gates.rs` refuses a
//! `cfg` naming a system anywhere else. `xtask`'s `ui::SYSTEM` names the system
//! in what the gate prints, and tests name it in their skips. `unix.rs` is the Unix half, which macOS takes
//! too: modes and owners through `std::os::unix`. `windows.rs` is the Windows
//! half: the token's user and the access lists of the Win32 security calls.
//! The local database's open (`nc_local`) and the kernel's blob directory are
//! two callers, and the daemon's IPC endpoint, [`ipc`], is the third: the
//! socket or the pipe a daemon is reached through, and who is at the other end
//! of each connection. A daemon's start asks three things more: its profile
//! held at once or not at all ([`try_hold`]), a session of its own
//! ([`leave_the_session`]), and which process it is, as the system says ([`process`]).
//! This crate is what they need, and no more.
//!
//! **And a program registered with this system's service manager**
//! ([`service`]), for an installer and the tests that stand in for one: its
//! definition written in the format the manager reads, and the manager's tool
//! asked by an absolute path. That module is compiled on every system, so the
//! Linux job tests every manager's half of it, and which manager a system
//! has is one constant, [`service::MANAGER`].
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

pub mod service;
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

/// The endpoint a daemon is reached through: a socket or a pipe, this user's alone.
pub mod ipc;
/// Which process this is, as the system says: its boot, its start and its
/// executable.
pub mod process;

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

/// What this system has that a suite needs: asked by the suite before it
/// runs, and by the gate to name each suite that stands aside.
///
/// **Compiled in every build, not behind `test-support`**, because the gate
/// that names them is `xtask`'s own build and no test: a suite that returns
/// early is counted by cargo as passed, and what it prints as it does is
/// captured with the rest of a passing test's output, so the gate's test step
/// is what says, beside its count and its verdict, which suites this system
/// stood aside. Each answer is fixed when this crate is built for a system,
/// never a probe of the machine, so a Linux machine cannot stand a suite
/// aside, and `linux_lacks_nothing_a_suite_stands_aside_for` holds every
/// answer on every system.
pub mod system {
    /// A directory that lists its entries in the order they were made, where
    /// this system has one: Linux's `tmpfs` at `/dev/shm`.
    ///
    /// **What a suite stands aside for where it is `None`.** macOS's APFS
    /// lists a directory by a hash of each name and Windows' NTFS by the name
    /// itself, so there two layouts of one set of names come back in one
    /// order, and a suite about the order on disk has nothing to compare. It
    /// says so in its own output, by the system's name, and the Linux job
    /// runs it.
    pub const LISTED_AS_MADE: Option<&str> = crate::imp::LISTED_AS_MADE;

    /// Whether this system has a POSIX shell of its own, in which a path is
    /// spent as POSIX spends it: false on Windows.
    ///
    /// **The `sh` Git ships there runs a script, and is not one.** It hands
    /// its paths to Windows, which spends a `..` by striking out the name
    /// before it, link or not, and reads two leading slashes as a network
    /// host; and a native caller hands it a drive's path that is not rooted
    /// at `/` at all. A guard written against POSIX paths is not, there, the
    /// guard it reads as: measured on a Windows runner, a decoy that reaches a
    /// script through a junction is taken for the repository the script
    /// belongs to. A suite driving such a guard stands aside where this is
    /// false and says so in its own output, by the system's name, and the
    /// Linux and macOS jobs run it.
    pub const A_POSIX_SHELL: bool = crate::imp::A_POSIX_SHELL;

    /// Whether a process here is in a session of the POSIX kind, which it
    /// leaves by starting its own (`setsid`) and which a terminal's hangup
    /// reaches: false on Windows.
    ///
    /// **What the suites about leaving a session — this crate's and the
    /// daemon's — stand aside for where it is false**: a process there has no
    /// such session, so there is none to read that one left. Each says so in
    /// its own output, by the system's name, and the Linux and macOS jobs run
    /// them.
    pub const POSIX_SESSIONS: bool = crate::imp::POSIX_SESSIONS;
}

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

/// `directory`, held as [`hold`] holds it, unless another hold has it now:
/// nothing then, at once, rather than a wait.
///
/// **What one daemon of a profile holds for as long as it runs**, so a second
/// is told at once that the first is there. Where this system cannot lock a
/// directory the lock is the file `name` in it, as [`hold`]'s is.
///
/// # Errors
///
/// [`Judged::Refused`] for a lock file that is not this user's alone, and
/// [`Judged::Failed`] when the directory or its lock could not be opened or
/// asked.
pub fn try_hold(directory: &Path, name: &str, user: &User) -> Result<Option<Held>, Judged> {
    imp::try_hold(directory, name, &user.0).map(|held| held.map(Held))
}

/// Leaves the session and the terminal this process was started from, where
/// this system has them.
///
/// **On Unix a session of its own** (`setsid`), so a terminal's hangup or its
/// keys never reach a daemon, and no terminal is its controlling one; a
/// process that already leads its process group — what a service manager or
/// a spawner made it — cannot start one, and is answered as detached, its
/// group its own. **On Windows nothing**: a process has no such session, and
/// one started detached has no console, which its spawner gives it.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it.
pub fn leave_the_session() -> std::io::Result<()> {
    imp::detach()
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

/// Fills `buffer` with bytes the system's own generator draws, every one of
/// them, or refuses.
///
/// **The system's, never a seed of this process's**: Unix reads
/// `/dev/urandom`, which the kernel keeps seeded, and Windows asks
/// `BCryptGenRandom` for the system's preferred generator. A secret drawn here
/// — a device's seed, the token the daemon is reached by — is one no other
/// process can predict from this one, and an answer that filled less than the
/// whole buffer is an error rather than a key with a tail of zeros.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it, when it could not be asked or
/// filled less than asked.
pub fn random(buffer: &mut [u8]) -> std::io::Result<()> {
    imp::random(buffer)
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

    /// The system's generator fills every byte it is handed, to the last, and
    /// two draws differ.
    #[test]
    fn random_fills_every_byte_it_is_handed_and_two_draws_differ() {
        let mut first = [0_u8; 32];
        let mut second = [0_u8; 32];
        super::random(&mut first).expect("the system's generator answers");
        super::random(&mut second).expect("the system's generator answers");
        assert_ne!(
            first, [0_u8; 32],
            "a draw came back as the zeros it was handed"
        );
        assert_ne!(
            first, second,
            "two draws of a seed's length came back alike"
        );
        // **To the last byte**: a fill that stopped short leaves the tail as it
        // was handed in, which a key read whole would carry as zeros.
        let mut long = vec![0_u8; 1 << 20];
        super::random(&mut long).expect("the system's generator answers");
        assert!(
            long[long.len() - 64..].iter().any(|byte| *byte != 0),
            "the last bytes of a long draw are the zeros it was handed"
        );
        super::random(&mut []).expect("nothing to fill is no error");
    }

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

    /// What each system lacks that a suite stands aside for, and that Linux
    /// lacks none of it.
    ///
    /// **The Linux job runs every suite another system's job cannot**
    /// (`NOCTUA.md §22.2`), so an answer of "lacking" there is a suite no job
    /// judges at all. Held per system, so an answer moved on any of the three
    /// is a failure on that system's own job.
    #[test]
    fn linux_lacks_nothing_a_suite_stands_aside_for() {
        let expected = if cfg!(target_os = "linux") {
            (Some("/dev/shm"), true, true)
        } else if cfg!(windows) {
            (None, false, false)
        } else {
            (None, true, true)
        };
        assert_eq!(
            (
                super::system::LISTED_AS_MADE,
                super::system::A_POSIX_SHELL,
                super::system::POSIX_SESSIONS
            ),
            expected,
            "what this system lacks for a suite is not what it is written to lack"
        );
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
        // **Asked again until a bound, as in
        // `a_directory_held_is_not_held_again_at_once`.** A child another test
        // of this binary starts holds a copy of each descriptor from its fork
        // until its exec, so the lock can still be held for that moment: the
        // hosted macOS runner failed here once. A lock never let go still
        // fails, at the bound.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let held_now = loop {
            let held_now = testing::is_held(&scratch.0, "lock");
            if !held_now || std::time::Instant::now() >= deadline {
                break held_now;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(!held_now, "nothing holds it now");
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

    /// A directory held is not held again at once: a second try is answered
    /// with nothing rather than a wait, while the first hold — of either kind
    /// — lives, and is held once the first lets go.
    #[test]
    fn a_directory_held_is_not_held_again_at_once() {
        let scratch = Scratch::new("tried");
        let user = user_in(&scratch.0).expect("private");
        let first = super::try_hold(&scratch.0, "lock", &user)
            .expect("asked")
            .expect("nothing held it, so it is held");
        assert!(
            testing::is_held(&scratch.0, "lock"),
            "held, and nothing holds it"
        );
        assert!(
            super::try_hold(&scratch.0, "lock", &user)
                .expect("asked")
                .is_none(),
            "a directory held was held a second time"
        );
        drop(first);
        let waited = super::hold(&scratch.0, "lock", &user).expect("held");
        assert!(
            super::try_hold(&scratch.0, "lock", &user)
                .expect("asked")
                .is_none(),
            "a directory a waiting hold has was held a second time"
        );
        drop(waited);
        // **Asked again until a bound, because a lock is let go once every
        // copy of it is closed.** A child another test of this binary starts
        // holds a copy of each descriptor from its fork until its exec, so the
        // directory can still be held for that moment: measured by a review,
        // one run of this binary in ten failed here when it asked once. A lock
        // that is never let go still fails, at the bound.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let again = loop {
            let tried = super::try_hold(&scratch.0, "lock", &user).expect("asked");
            if tried.is_some() || std::time::Instant::now() >= deadline {
                break tried;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(again.is_some(), "a directory nothing holds was not held");
    }

    /// The variable that turns [`a_detached_process_leads_a_session_of_its_own`]
    /// into its own child.
    const DETACHING: &str = "NC_PLATFORM_DETACHING_CHILD";

    /// A process that detaches leads a session of its own, where this system
    /// has sessions, and not the one it was started in.
    ///
    /// **A child of this test binary, started as no group's leader**: detaching
    /// the test process itself would take the whole suite out of the session
    /// it was started in. The child detaches, says so, and waits for its input
    /// to close while the parent reads its session off the system.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the process that detaches has to be another one, so the session it leaves is \
                  not this suite's; the child is this same test binary, its input a pipe the \
                  parent closes to end it"
    )]
    fn a_detached_process_leads_a_session_of_its_own() {
        use std::io::{BufRead as _, Write as _};

        if !super::system::POSIX_SESSIONS {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(
                out,
                "skipped, not applicable on {}: no process here is in a session of the POSIX \
                 kind, so there is none to leave, and the Linux and macOS jobs run it",
                std::env::consts::OS
            );
            return;
        }
        if std::env::var_os(DETACHING).is_some() {
            super::leave_the_session().expect("detached");
            super::leave_the_session().expect("detached again, as a session's leader");
            let mut out = std::io::stdout().lock();
            out.write_all(b"detached\n").expect("said");
            out.flush().expect("said");
            let mut rest = String::new();
            let _ = std::io::stdin().read_line(&mut rest);
            return;
        }
        let mut child = Child(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--exact",
                    "tests::a_detached_process_leads_a_session_of_its_own",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(DETACHING, "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("the child starts"),
        );
        let pid = child.0.id();
        let mut said = String::new();
        let mut lines = std::io::BufReader::new(child.0.stdout.take().expect("piped"));
        while !said.contains("detached") {
            said.clear();
            assert!(
                lines
                    .read_line(&mut said)
                    .expect("the child's output reads")
                    > 0,
                "the child ended without detaching"
            );
        }
        let session = testing::session_of(pid)
            .expect("the child's session is read")
            .expect("a system with sessions read none for the child");
        let ours = testing::session_of(std::process::id())
            .expect("this session is read")
            .expect("a system with sessions read none for this process");
        drop(child.0.stdin.take());
        assert_eq!(session, pid, "the detached child does not lead a session");
        assert_ne!(
            session, ours,
            "the detached child is still in this one's session"
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
    ///
    /// **And by the rule for what this crate made, not the companions'**: one
    /// owned by whoever the system makes the owner of what this process
    /// creates — the Administrators group, on Windows in an elevated process —
    /// is refused, since the lock file this crate makes it makes owned by the
    /// user. Where that owner is the user there is nothing to stage, and the
    /// test says so; the file is then the user's own, and taken.
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
        let owned_by_default = scratch.0.join("owned-by-default");
        drop(super::create_private(&owned_by_default).expect("a file"));
        let given_away =
            testing::owned_as_this_process_creates(&owned_by_default).expect("its owner is set");
        if !given_away {
            #[expect(
                clippy::print_stdout,
                reason = "a test saying, in its own output, that there was nothing to stage"
            )]
            {
                println!(
                    "nothing to stage for a lock file owned as the system owns what this \
                     process creates, on {}: that owner is this user",
                    std::env::consts::OS
                );
            }
        }
        for (name, says) in [
            ("linked", Some("symbolic link")),
            ("reachable", Some(said.as_str())),
            ("owned-by-default", given_away.then_some("belongs to user")),
        ] {
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
            match (testing::HOLDS_THROUGH_A_FILE, says) {
                (true, Some(says)) => {
                    let refused = match held {
                        Err(Judged::Refused(refused)) => refused.to_string(),
                        other => panic!("{name}: refused, and it was {other:?}"),
                    };
                    assert!(refused.contains(says), "{name}: {refused}");
                }
                (true, None) => drop(held.expect("the lock file is the user's own, and held")),
                (false, _) => {
                    drop(held.expect("the directory is held, and the name is not the hold's"));
                }
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

    /// A runtime of one thread, as the daemon's endpoint is listened on.
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
    }

    /// A pipe's name no other test, run or checkout takes: the test prefix,
    /// the checkout this was built in, this process and a nonce.
    ///
    /// **A pipe's name is the whole machine's**, so a test that reused one
    /// would meet another run's, or a real daemon's.
    fn pipe_named(name: &str) -> String {
        let checkout = env!("CARGO_MANIFEST_DIR")
            .bytes()
            .fold(0x811c_9dc5_u32, |hash, byte| {
                (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
            });
        let mut nonce = [0_u8; 4];
        super::random(&mut nonce).expect("the system's generator answers");
        let nonce = u32::from_le_bytes(nonce);
        format!(
            r"\\.\pipe\nc-test-{checkout:08x}-{}-{nonce:08x}-{name}",
            std::process::id()
        )
    }

    /// The endpoint of a scratch runtime directory: its socket `s`, and a
    /// pipe of its own.
    fn endpoint_in(scratch: &Scratch, name: &str) -> super::ipc::Endpoint {
        super::ipc::Endpoint::new(&scratch.0, scratch.0.join("s"), pipe_named(name))
    }

    /// What `future` comes to, or a failure naming `what` once a minute has
    /// passed without it: a test whose other end never came fails, rather
    /// than holding the gate until its job's own timeout.
    async fn within<F: core::future::Future>(what: &str, future: F) -> F::Output {
        tokio::time::timeout(std::time::Duration::from_secs(60), future)
            .await
            .unwrap_or_else(|_| panic!("{what} did not come within a minute"))
    }

    /// One connection to `endpoint`, made by a client of `user`'s and
    /// accepted by `listener`, and what each end came to — within a minute,
    /// so a client that fails before it reaches the listener fails the test.
    async fn connection(
        listener: &mut super::ipc::Listener,
        endpoint: &super::ipc::Endpoint,
        user: &super::User,
    ) -> (
        Result<super::ipc::Accepted, super::ipc::Refused>,
        Result<(super::ipc::Stream, super::ipc::Peer), super::ipc::Refused>,
    ) {
        within("a connection, accepted and made,", async {
            tokio::join!(listener.accept(), super::ipc::connect(endpoint, user))
        })
        .await
    }

    /// A child process this test started, killed and waited for however the
    /// test leaves.
    struct Child(std::process::Child);

    /// The child killed and waited for, whether it ended or not.
    impl Drop for Child {
        /// Kills the child, which may have ended already, and reaps it.
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// What a test asks of its child.
    impl Child {
        /// Waits for the child to end, for a minute at most, and answers
        /// whether it ended as a passing test does.
        fn ended_well(&mut self) -> bool {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            loop {
                if let Some(status) = self.0.try_wait().expect("the child is asked") {
                    return status.success();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "the child did not end within a minute"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    /// The name a child of [`the_peer_is_the_process_at_the_other_end_and_not_this_one`]
    /// is run by.
    ///
    /// That test holds its own name to it: renamed alone, the child would run
    /// no test and connect to nothing.
    const PEER_TEST: &str = "tests::the_peer_is_the_process_at_the_other_end_and_not_this_one";
    /// The role a child of [`the_peer_is_the_process_at_the_other_end_and_not_this_one`]
    /// plays: `connect`, or `serve`.
    const CHILD_ROLE: &str = "NC_PLATFORM_IPC_CHILD_ROLE";
    /// The runtime directory of the endpoint the child plays its role at.
    const CHILD_RUNTIME: &str = "NC_PLATFORM_IPC_CHILD_RUNTIME";
    /// The endpoint's socket.
    const CHILD_SOCKET: &str = "NC_PLATFORM_IPC_CHILD_SOCKET";
    /// The endpoint's pipe.
    const CHILD_PIPE: &str = "NC_PLATFORM_IPC_CHILD_PIPE";

    /// This test binary, run again as a child playing `role` at `endpoint`.
    #[expect(
        clippy::disallowed_methods,
        reason = "the other end of a connection has to be another process for its id to be \
                  told from this one's; the child is this same test binary, its output \
                  discarded"
    )]
    fn spawned(role: &str, endpoint: &super::ipc::Endpoint) -> Child {
        Child(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args(["--exact", PEER_TEST, "--test-threads=1"])
                .env(CHILD_ROLE, role)
                .env(CHILD_RUNTIME, &endpoint.runtime)
                .env(CHILD_SOCKET, &endpoint.socket)
                .env(CHILD_PIPE, &endpoint.pipe)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("the child starts"),
        )
    }

    /// Reads `stream` until its other end closes it.
    async fn until_closed(stream: &mut super::ipc::Stream) {
        use tokio::io::AsyncReadExt as _;

        let mut buffer = [0_u8; 16];
        while matches!(stream.read(&mut buffer).await, Ok(read) if read > 0) {}
    }

    /// What a child of the peer test does: connects to the endpoint, or binds
    /// it and accepts one connection, and holds the connection until its
    /// other end closes it.
    fn as_the_child(role: &str) {
        let read = |name| std::env::var(name).expect("the parent names the endpoint");
        let endpoint =
            super::ipc::Endpoint::new(read(CHILD_RUNTIME), read(CHILD_SOCKET), read(CHILD_PIPE));
        let user = user_in(endpoint.runtime()).expect("the parent's runtime directory is private");
        runtime().block_on(async {
            if role == "serve" {
                let instance =
                    super::hold(endpoint.runtime(), "instance.lock", &user).expect("held");
                let mut listener = super::ipc::Listener::bind(&endpoint, &user, &instance, 4)
                    .expect("the child binds");
                let mut accepted = listener.accept().await.expect("the parent connects");
                // Said once the parent has been read off the system: macOS no
                // longer names a peer that has closed, so the parent waits for
                // this before it closes.
                tokio::io::AsyncWriteExt::write_all(&mut accepted.stream, b"!")
                    .await
                    .expect("the parent hears it");
                until_closed(&mut accepted.stream).await;
            } else {
                let (mut stream, _) = super::ipc::connect(&endpoint, &user)
                    .await
                    .expect("the child reaches the parent's endpoint");
                until_closed(&mut stream).await;
            }
        });
    }

    /// Both ends of an endpoint read the other off the system, and bytes
    /// cross it both ways.
    #[test]
    fn both_ends_of_an_endpoint_read_the_other_off_the_system() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let scratch = Scratch::new("ipc-both-ends");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "both-ends");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            let mut listener =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 4).expect("bound");
            let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
            let mut accepted = accepted.expect("accepted");
            let (mut client, server) = connected.expect("connected");
            assert_eq!(
                accepted.peer.pid,
                std::process::id(),
                "the client's process"
            );
            assert_eq!(server.pid, std::process::id(), "the server's process");
            client.write_all(b"ping").await.expect("written");
            let mut heard = [0_u8; 4];
            accepted.stream.read_exact(&mut heard).await.expect("read");
            assert_eq!(&heard, b"ping");
            accepted.stream.write_all(b"pong").await.expect("written");
            client.read_exact(&mut heard).await.expect("read");
            assert_eq!(&heard, b"pong");
        });
    }

    /// The process each end reads is the one at the other end — a child's,
    /// never this process's own — whichever end the child is.
    ///
    /// **Against a child, because one process at both ends cannot tell the
    /// two apart**: a reader answering this process's own credentials passes
    /// every test whose two ends are this process.
    #[test]
    fn the_peer_is_the_process_at_the_other_end_and_not_this_one() {
        if let Ok(role) = std::env::var(CHILD_ROLE) {
            as_the_child(&role);
            return;
        }
        // libtest runs each test on a thread of its name.
        assert_eq!(
            std::thread::current().name(),
            Some(PEER_TEST),
            "this test's child is run by another name"
        );
        let scratch = Scratch::new("ipc-child-connects");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "child-connects");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        let mut child = runtime().block_on(async {
            let mut listener =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 4).expect("bound");
            let child = spawned("connect", &endpoint);
            let accepted = within("the connecting child", listener.accept())
                .await
                .expect("the child's connection");
            assert_eq!(
                accepted.peer.pid,
                child.0.id(),
                "the client read is not the child"
            );
            assert_ne!(accepted.peer.pid, std::process::id());
            child
        });
        // **Waited for once the runtime is gone**, never on it: a pipe's handle
        // is closed only as the runtime driving it lets it go, so a wait that
        // blocked that runtime would wait for itself — measured on a Windows
        // runner, the child never saw its connection end.
        assert!(child.ended_well(), "the connecting child failed");

        // The child's own lock, on a directory of its own: this process holds
        // the first one's.
        let second = Scratch::new("ipc-child-serves");
        let endpoint = endpoint_in(&second, "child-serves");
        let mut child = runtime().block_on(async {
            let child = spawned("serve", &endpoint);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            let (mut stream, server) = loop {
                match super::ipc::connect(&endpoint, &user).await {
                    Ok(connected) => break connected,
                    Err(super::ipc::Refused::Nobody(_)) if std::time::Instant::now() < deadline => {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                    Err(other) => panic!("the child's endpoint was not reached: {other}"),
                }
            };
            assert_eq!(server.pid, child.0.id(), "the server read is not the child");
            assert_ne!(server.pid, std::process::id());
            // Closed only once the child has read this end off the system:
            // measured on a macOS runner, a connection closed first left the
            // child's accept with no peer to name.
            let mut heard = [0_u8; 1];
            within(
                "the serving child's word",
                tokio::io::AsyncReadExt::read_exact(&mut stream, &mut heard),
            )
            .await
            .expect("the child accepted");
            child
        });
        assert!(child.ended_well(), "the serving child failed");
    }

    /// The peer predicate refuses a user that is not this one, naming both
    /// and which part of the other end it was, and takes this user.
    ///
    /// **Driven with a forged identity**, since no test can be another user:
    /// what the system says of a real other account reaches this same
    /// comparison.
    #[test]
    fn the_peer_predicate_refuses_someone_else_and_takes_this_user() {
        use super::ipc::{Refused, Said, Whose, admits, admitted};

        let scratch = Scratch::new("ipc-predicate");
        let user = user_in(&scratch.0).expect("private");
        let other = testing::someone_else(&user);
        for whose in [Whose::Process, Whose::Pipe] {
            admits(&user.0.who(), whose, &user).expect("this user is taken");
            match admits(&other.0.who(), whose, &user) {
                Err(Refused::Peer {
                    whose: said,
                    found,
                    user: ours,
                }) => {
                    assert_eq!((found, ours), (other.to_string(), user.to_string()));
                    let refused = admits(&other.0.who(), whose, &user)
                        .expect_err("refused")
                        .to_string();
                    assert!(refused.starts_with(said), "{refused}");
                }
                verdict => panic!("{whose:?}: someone else was taken: {verdict:?}"),
            }
        }
        let said = |users: Vec<(Whose, _)>, pid| Said { users, pid };
        assert!(
            matches!(
                admitted(
                    said(
                        vec![(Whose::Pipe, user.0.who()), (Whose::Process, other.0.who())],
                        Some(7)
                    ),
                    &user
                ),
                Err(Refused::Peer { .. })
            ),
            "one part of the other end that is someone else's was let through"
        );
        for pid in [None, Some(0), Some(-1), Some(i64::from(u32::MAX) + 1)] {
            assert!(
                matches!(
                    admitted(said(vec![(Whose::Process, user.0.who())], pid), &user),
                    Err(Refused::NoProcess)
                ),
                "{pid:?} was taken for a process"
            );
        }
        assert_eq!(
            admitted(said(vec![(Whose::Process, user.0.who())], Some(42)), &user)
                .expect("this user's process 42")
                .pid,
            42
        );
    }

    /// A runtime directory another user can reach is refused, each way it can
    /// be reached, and no endpoint is made in it.
    #[test]
    fn a_runtime_directory_another_user_can_reach_is_refused_and_nothing_is_bound() {
        let scratch = Scratch::new("ipc-reachable");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "reachable");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            for opening in testing::every_way_another_user_can_reach(Kind::Directory) {
                let said = opening.apply(&scratch.0).expect("staged");
                let verdict = super::ipc::Listener::bind(&endpoint, &user, &instance, 4);
                testing::make_private(&scratch.0).expect("private again");
                match verdict {
                    Err(super::ipc::Refused::NotPrivate(refused)) => {
                        let refused = refused.to_string();
                        assert!(
                            refused.contains(&said) && refused.contains("another user can reach"),
                            "{said}: {refused}"
                        );
                    }
                    other => panic!("{said}: refused, and it was {other:?}"),
                }
                assert!(
                    matches!(
                        super::ipc::connect(&endpoint, &user).await,
                        Err(super::ipc::Refused::Nobody(_))
                    ),
                    "{said}: an endpoint was made in a directory that was refused"
                );
            }
            let elsewhere = super::ipc::Endpoint::new(
                &scratch.0,
                scratch.0.join("deeper").join("s"),
                pipe_named("elsewhere"),
            );
            match super::ipc::Listener::bind(&elsewhere, &user, &instance, 4) {
                Err(super::ipc::Refused::NotPrivate(refused)) => assert!(
                    refused
                        .to_string()
                        .contains("is not in the runtime directory"),
                    "{refused}"
                ),
                other => panic!("a socket outside its runtime directory was {other:?}"),
            }
        });
    }

    /// The endpoint is made as this crate makes what is private, and is read
    /// back so: the socket's mode as `0600`, the pipe's list as one entry.
    #[test]
    fn the_endpoint_is_made_private_and_read_back_so() {
        let scratch = Scratch::new("ipc-as-created");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "as-created");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            let listener =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 4).expect("bound");
            assert!(
                testing::endpoint_is_as_created(&listener),
                "the endpoint lets in more than its user"
            );
        });
    }

    /// A name exactly as long as the system holds is bound, and one longer is
    /// refused from either end with a sentence naming the limit.
    #[test]
    fn a_name_longer_than_the_system_holds_is_refused_by_its_limit() {
        let scratch = Scratch::new("ipc-long");
        let user = user_in(&scratch.0).expect("private");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        let room = super::imp::NAME_MAX - scratch.0.as_os_str().len() - 1;
        let pipe = pipe_named("long");
        let filled = |length: usize| format!("{pipe}{}", "x".repeat(length - pipe.len()));
        let at_the_limit = super::ipc::Endpoint::new(
            &scratch.0,
            scratch.0.join("x".repeat(room)),
            filled(super::imp::NAME_MAX),
        );
        let past_it = super::ipc::Endpoint::new(
            &scratch.0,
            scratch.0.join("x".repeat(room + 1)),
            filled(super::imp::NAME_MAX + 1),
        );
        runtime().block_on(async {
            drop(
                super::ipc::Listener::bind(&at_the_limit, &user, &instance, 4)
                    .expect("a name at the limit is bound"),
            );
            let refused = [
                super::ipc::Listener::bind(&past_it, &user, &instance, 4).map(drop),
                super::ipc::connect(&past_it, &user).await.map(drop),
            ];
            for verdict in refused {
                match verdict {
                    Err(refused @ super::ipc::Refused::TooLong { .. }) => {
                        let said = refused.to_string();
                        assert!(
                            said.contains(&format!(
                                "{} {}",
                                super::imp::NAME_MAX,
                                super::imp::NAME_UNIT
                            )) && said.contains(&past_it.name()),
                            "{said}"
                        );
                    }
                    other => panic!("a name past the limit was {other:?}"),
                }
            }
        });
    }

    /// An endpoint something answers at is not taken, and what answers there
    /// goes on answering.
    #[test]
    fn an_endpoint_something_answers_at_is_not_taken() {
        let scratch = Scratch::new("ipc-taken");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "taken");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            let mut first =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 4).expect("bound");
            match super::ipc::Listener::bind(&endpoint, &user, &instance, 4) {
                Err(super::ipc::Refused::Taken { name }) => assert_eq!(name, endpoint.name()),
                other => panic!("an endpoint something answers at was {other:?}"),
            }
            // **The refused bind asked by connecting**, and that connection is
            // the first listener's to accept first, closed already: measured on
            // a macOS runner, which no longer names a peer that has closed, it
            // is refused there. The listener goes on accepting either way.
            let (accepted, connected) = within("a connection to the first listener", async {
                tokio::join!(
                    async {
                        match first.accept().await {
                            Ok(accepted) => accepted,
                            Err(_) => first
                                .accept()
                                .await
                                .expect("the first listener still accepts"),
                        }
                    },
                    super::ipc::connect(&endpoint, &user)
                )
            })
            .await;
            drop(accepted);
            connected.expect("and is reached");
        });
    }

    /// A dead socket of this user's is replaced, and anything else at the
    /// endpoint's name — a file, a link, a dead socket another user owns — is
    /// refused and left as it was, where the endpoint is a name on disk; where
    /// it is not, the name at the socket's path is nothing a bind reads, and is
    /// left alone. A file put at the name while a listener lived is left as it
    /// was when the listener is let go, on every system.
    ///
    /// **Another user's socket is judged with a forged user**, since no test
    /// can make a socket another user owns: what a bind makes of a real other
    /// account's socket is this same judgement, handed `testing::someone_else`
    /// as the user it runs as.
    #[test]
    fn a_dead_socket_is_replaced_and_anything_else_at_the_name_is_left_as_it_was() {
        use std::io::Write as _;

        let scratch = Scratch::new("ipc-stale");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "stale");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            testing::leave_a_dead_endpoint(&endpoint).expect("a dead endpoint");
            // **Asked again until a bound.** A child another test of this
            // binary starts holds a copy of the dead listener's descriptor
            // from its fork until its exec, and a socket answers while any copy
            // of it is open: measured by a review, a share of local runs failed
            // here when it asked once. An endpoint that keeps answering still
            // fails, at the bound.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let nobody = loop {
                let nobody = matches!(
                    super::ipc::connect(&endpoint, &user).await,
                    Err(super::ipc::Refused::Nobody(_))
                );
                if nobody || std::time::Instant::now() >= deadline {
                    break nobody;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert!(nobody, "a dead endpoint answered");
            if testing::ENDPOINT_ON_DISK {
                let before = testing::identity(&endpoint.socket).expect("the dead socket");
                let other = testing::someone_else(&user);
                match testing::judge_what_stands_at(&endpoint, &other) {
                    Err(super::ipc::Refused::NotPrivate(refused)) => {
                        let refused = refused.to_string();
                        assert!(
                            refused.contains(&format!("belongs to user {user}"))
                                && refused.contains(&format!("runs as user {other}")),
                            "{refused}"
                        );
                    }
                    verdict => panic!("a dead socket another user owns was {verdict:?}"),
                }
                testing::judge_what_stands_at(&endpoint, &user)
                    .expect("this user's own dead socket is one a bind asks");
                assert_eq!(
                    testing::identity(&endpoint.socket).expect("still there"),
                    before,
                    "a dead socket was changed by being judged"
                );
            }
            let listener = super::ipc::Listener::bind(&endpoint, &user, &instance, 4)
                .expect("a dead socket of this user's is replaced");
            // While the listener lives, its socket goes and a file of this
            // user's takes its name: a listener let go removes the socket it
            // made and nothing put there since. A pipe never touches this
            // path, so the file is kept on every system.
            if testing::ENDPOINT_ON_DISK {
                std::fs::remove_file(&endpoint.socket).expect("the listener's socket goes");
            }
            super::create_private(&endpoint.socket)
                .expect("a file at the socket's path")
                .write_all(b"kept")
                .expect("written");
            drop(listener);
            assert_eq!(
                std::fs::read(&endpoint.socket).expect("still there"),
                b"kept",
                "a listener let go removed what was put at its name since"
            );
            let target = scratch.0.join("target");
            drop(super::create_private(&target).expect("a file"));
            for (staged, says) in [("a file", "is not a socket"), ("a link", "symbolic link")] {
                if staged == "a link" {
                    std::fs::remove_file(&endpoint.socket).expect("the file goes");
                    testing::link_file(&target, &endpoint.socket).expect("a link");
                }
                let before = std::fs::symlink_metadata(&endpoint.socket).expect("there");
                let verdict = super::ipc::Listener::bind(&endpoint, &user, &instance, 4);
                let after = std::fs::symlink_metadata(&endpoint.socket).expect("still there");
                assert_eq!(
                    (after.file_type().is_symlink(), after.len()),
                    (before.file_type().is_symlink(), before.len()),
                    "{staged} at the socket's path was changed"
                );
                match (testing::ENDPOINT_ON_DISK, verdict) {
                    (true, Err(super::ipc::Refused::NotPrivate(refused))) => {
                        assert!(refused.to_string().contains(says), "{staged}: {refused}");
                    }
                    (false, Ok(listener)) => drop(listener),
                    (_, other) => panic!("{staged} at the socket's path: {other:?}"),
                }
            }
            if testing::ENDPOINT_ON_DISK {
                assert_eq!(std::fs::read(&target).expect("readable"), b"");
            }
        });
    }

    /// A connection past the bound is closed as soon as it is accepted, one
    /// closed makes room for the next, and a bound of none or past what a pipe
    /// holds is refused.
    #[test]
    fn a_connection_past_the_bound_is_closed_and_one_closed_makes_room() {
        use tokio::io::AsyncReadExt as _;

        let scratch = Scratch::new("ipc-bound");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "bound");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            for refused in [0, super::ipc::CONNECTIONS_MAX + 1] {
                assert!(
                    matches!(
                        super::ipc::Listener::bind(&endpoint, &user, &instance, refused),
                        Err(super::ipc::Refused::Failed(_))
                    ),
                    "a bound of {refused} connections was taken"
                );
            }
            let mut listener =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 2).expect("bound");
            let mut open = Vec::new();
            for _ in 0..2 {
                let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
                open.push((
                    accepted.expect("within the bound"),
                    connected.expect("reached"),
                ));
            }
            let (past, connected) = connection(&mut listener, &endpoint, &user).await;
            assert!(
                matches!(past, Err(super::ipc::Refused::Busy { connections: 2 })),
                "a connection past the bound was {past:?}"
            );
            let (mut closed, _) = connected.expect("reached, and then closed");
            let mut buffer = [0_u8; 1];
            assert!(
                !matches!(
                    within("the end of a connection past the bound", closed.read(&mut buffer)).await,
                    Ok(read) if read > 0
                ),
                "a connection past the bound was not closed"
            );
            drop(open.remove(0));
            let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
            accepted.expect("one closed made room");
            connected.expect("reached");
        });
    }

    /// An accept given up before a client came leaves the listener as it
    /// was: the next client is accepted, with a connection open and with none.
    ///
    /// **The instance a pipe waits with is what is at stake**: given up with
    /// it taken out of the listener, the name stood with none of the
    /// listener's instances behind it, and the next accept made a first
    /// instance, which the system refuses while a connection holds the name.
    #[test]
    fn an_accept_given_up_before_a_client_came_leaves_the_listener_listening() {
        let scratch = Scratch::new("ipc-given-up");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "given-up");
        let instance = super::hold(&scratch.0, "instance.lock", &user).expect("held");
        runtime().block_on(async {
            let mut listener =
                super::ipc::Listener::bind(&endpoint, &user, &instance, 4).expect("bound");
            let mut open = Vec::new();
            for open_already in 0..2 {
                let given_up =
                    tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                        .await;
                assert!(
                    given_up.is_err(),
                    "an accept no client came to was {given_up:?}"
                );
                assert!(
                    testing::endpoint_is_as_created(&listener),
                    "with {open_already} open, an accept given up left no instance waiting"
                );
                let (accepted, connected) = connection(&mut listener, &endpoint, &user).await;
                open.push((
                    accepted.expect("the next client is accepted"),
                    connected.expect("and reaches the listener"),
                ));
            }
        });
    }

    /// Nothing listening at an endpoint is told apart from a refusal.
    #[test]
    fn nothing_listening_is_told_apart_from_a_refusal() {
        let scratch = Scratch::new("ipc-nobody");
        let user = user_in(&scratch.0).expect("private");
        let endpoint = endpoint_in(&scratch, "nobody");
        runtime().block_on(async {
            match super::ipc::connect(&endpoint, &user).await {
                Err(refused @ super::ipc::Refused::Nobody(_)) => {
                    assert!(refused.to_string().contains("nothing answers"), "{refused}");
                }
                other => panic!("an endpoint nothing listens at was {other:?}"),
            }
        });
    }
}
