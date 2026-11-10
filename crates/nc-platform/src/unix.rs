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

use crate::ipc::{Endpoint, Refused, Said, Whose};
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

/// Every process here is in a session, which `setsid` starts and a
/// terminal's hangup reaches.
pub(crate) const POSIX_SESSIONS: bool = true;

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

/// Whom the system says something belongs to: a user's number.
pub(crate) type Who = u32;

/// What a peer's credentials are held to.
impl User {
    /// The number a peer's credentials are compared with.
    pub(crate) const fn who(&self) -> Who {
        self.0
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
    let held = to_hold(directory)?;
    held.lock().map_err(|error| {
        Failed::new(
            format!("`{}` could not be held for this open", directory.display()),
            error,
        )
    })?;
    Ok(held)
}

/// `directory`, held through a handle of its own unless another holds it now:
/// nothing then, at once.
pub(crate) fn try_hold(
    directory: &Path,
    _name: &str,
    _user: &User,
) -> Result<Option<std::fs::File>, Judged> {
    let held = to_hold(directory)?;
    match held.try_lock() {
        Ok(()) => Ok(Some(held)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(Judged::Failed(Failed::new(
            format!("`{}` could not be held", directory.display()),
            error,
        ))),
    }
}

/// A handle of `directory`'s own, which a lock is taken through.
fn to_hold(directory: &Path) -> Result<std::fs::File, Judged> {
    std::fs::File::open(directory).map_err(|error| {
        Judged::Failed(Failed::new(
            format!("`{}` could not be read", directory.display()),
            error,
        ))
    })
}

/// Starts a session of this process's own, so no terminal it was started from
/// signals it or is its controlling one.
///
/// **A process group's leader cannot start a session** (`EPERM`), and is
/// answered as detached: it leads a group of its own already — the one a
/// service manager or a spawner made it — which no other group's signals
/// reach.
pub(crate) fn detach() -> io::Result<()> {
    match rustix::process::setsid() {
        Ok(_) | Err(rustix::io::Errno::PERM) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Nothing: a spawn puts what it hands a child, null or not, in place of
/// each standard handle, and the standard library opens every other
/// descriptor closed on `exec`.
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for both halves, and the Windows one can fail"
)]
pub(crate) fn keep_standard_handles_from_children() -> io::Result<()> {
    Ok(())
}

/// The session `pid` is in, as the system numbers it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn session_of(pid: u32) -> io::Result<Option<u32>> {
    let pid = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let session = rustix::process::getsid(Some(pid))?;
    Ok(u32::try_from(session.as_raw_nonzero().get()).ok())
}

/// The boot this process runs in and when it started, as the system says.
///
/// **Linux** reads the boot's own identifier and the start in clock ticks
/// since boot, field 22 of `/proc/<pid>/stat` — counted after the last `)`,
/// which ends a name that may hold spaces and parentheses of its own.
#[cfg(target_os = "linux")]
pub(crate) fn this_process() -> io::Result<(Option<String>, String)> {
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let stat = std::fs::read_to_string("/proc/self/stat")?;
    let started = crate::process::started_in_stat(&stat).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "`/proc/self/stat` holds no start time after its name",
        )
    })?;
    Ok((Some(boot.trim().to_owned()), started))
}

/// The boot this process runs in and when it started, as the system says.
///
/// **macOS** has no `/proc`: the boot's session identifier is the kernel's
/// `kern.bootsessionuuid`, and the start is what `ps` prints for the process,
/// both asked of the system's own tools by their absolute paths, with an
/// environment of nothing but the C locale and UTC, so the text is the one a
/// later reading compares with.
#[cfg(not(target_os = "linux"))]
pub(crate) fn this_process() -> io::Result<(Option<String>, String)> {
    let pid = std::process::id().to_string();
    let boot = boot_session()?;
    let said = asked("/bin/ps", &["-o", "lstart=", "-p", &pid])?;
    let started = crate::process::started_in_ps(&said).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "`ps` printed no start for this process",
        )
    })?;
    Ok((Some(boot), started))
}

/// The boot's session identifier, `kern.bootsessionuuid`, asked once.
///
/// A boot does not change under a process that runs in it, and a wait reads
/// it again at every look.
#[cfg(not(target_os = "linux"))]
fn boot_session() -> io::Result<String> {
    static BOOT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(boot) = BOOT.get() {
        return Ok(boot.clone());
    }
    let said = asked("/usr/sbin/sysctl", &["-n", "kern.bootsessionuuid"])?;
    Ok(BOOT.get_or_init(|| said.trim().to_owned()).clone())
}

/// What an executable's name ends with here: nothing.
pub(crate) const EXECUTABLE_SUFFIX: &str = "";

/// Where this system keeps a user's profiles: the XDG directories on Linux,
/// the home's Library on macOS.
pub(crate) const BASES: crate::Bases = if cfg!(target_os = "macos") {
    crate::Bases::Library
} else {
    crate::Bases::Xdg
};

/// A new file, `0700`, refusing one that exists — a link included: an
/// executable its owner alone reads, writes and runs.
#[expect(
    clippy::disallowed_methods,
    reason = "an installer's copy of an executable, created private before anything else can \
              make it, inside a directory its caller judged"
)]
pub(crate) fn create_private_executable(path: &Path) -> io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(0o700))?;
    Ok(file)
}

/// `command` made to start the leader of a process group of its own, so no
/// signal a terminal sends this process's group reaches it.
pub(crate) fn detached(command: &mut std::process::Command) {
    std::os::unix::process::CommandExt::process_group(command, 0);
}

/// The process group `pid` leads or is in, as the system numbers it.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn group_of(pid: u32) -> io::Result<Option<u32>> {
    let pid = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let group = rustix::process::getpgid(Some(pid))?;
    Ok(u32::try_from(group.as_raw_nonzero().get()).ok())
}

/// The process id `pid` as the system's calls take one, or nothing for a
/// number no process can hold.
fn pid_of(pid: u32) -> Option<rustix::process::Pid> {
    i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
}

/// A process held for a signal: on Linux through a pidfd, which names one
/// process for as long as it is open.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct Process {
    /// Its id.
    pid: u32,
    /// The pidfd it is held through.
    pidfd: std::os::fd::OwnedFd,
}

/// The process `record` names, held through a pidfd opened before anything is
/// read of it, and what the system says of it; nothing when no process holds
/// its id.
///
/// **The pidfd first, and asked again last.** What `/proc/<pid>` says is read
/// after the pidfd is open, and the pidfd is then asked which process it holds:
/// while it still names `pid`, no other process can have been handed that id,
/// so everything read was of the process the pidfd holds — the one every
/// signal goes to.
#[cfg(target_os = "linux")]
pub(crate) fn open(
    record: &crate::process::Identity,
) -> io::Result<Option<(Process, crate::process::Seen)>> {
    let Some(pid) = pid_of(record.pid) else {
        return Ok(None);
    };
    let pidfd = match rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        Ok(pidfd) => pidfd,
        Err(rustix::io::Errno::SRCH) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let process = Process {
        pid: record.pid,
        pidfd,
    };
    let directory = std::path::PathBuf::from(format!("/proc/{}", record.pid));
    let (Some(stat), Some(owner)) = (
        gone_if_missing(std::fs::read_to_string(directory.join("stat")))?,
        gone_if_missing(std::fs::metadata(&directory))?,
    ) else {
        return Ok(None);
    };
    let started = crate::process::started_in_stat(&stat).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "`/proc/{}/stat` holds no start time after its name",
                record.pid
            ),
        )
    })?;
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let executable = std::fs::read_link(directory.join("exe")).ok();
    let seen = crate::process::Seen {
        boot: Some(boot.trim().to_owned()),
        started,
        executable,
        ours: owner.uid() == rustix::process::getuid().as_raw(),
        ended: ended_in(&stat),
    };
    if !process.holds_its_id()? {
        return Ok(None);
    }
    Ok(Some((process, seen)))
}

/// Whether a `stat` line says its process has ended: a zombie, or one dead.
#[cfg(target_os = "linux")]
fn ended_in(stat: &str) -> bool {
    matches!(crate::process::state_in_stat(stat), Some("Z" | "X" | "x"))
}

/// `read`'s answer, or nothing when what it read is not there: a process that
/// ended while it was read.
#[cfg(target_os = "linux")]
fn gone_if_missing<T>(read: io::Result<T>) -> io::Result<Option<T>> {
    match read {
        Ok(read) => Ok(Some(read)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
impl Process {
    /// Whether the pidfd still holds a process with its id: one not yet
    /// reaped, which no other process can share the id of.
    fn holds_its_id(&self) -> io::Result<bool> {
        use std::os::fd::AsRawFd as _;
        let fdinfo =
            std::fs::read_to_string(format!("/proc/self/fdinfo/{}", self.pidfd.as_raw_fd()))?;
        let held = crate::process::pid_in_fdinfo(&fdinfo).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "the system does not say which process a pidfd holds",
            )
        })?;
        Ok(held == i64::from(self.pid))
    }
}

/// Sends `process` the signal `ending` names, through its pidfd; one that has
/// already ended is not an error.
#[cfg(target_os = "linux")]
pub(crate) fn signal(process: &Process, ending: crate::process::Ending) -> io::Result<()> {
    match rustix::process::pidfd_send_signal(&process.pidfd, signal_for(ending)) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Whether `process` has ended: reaped, or a zombie its parent has yet to hear.
#[cfg(target_os = "linux")]
pub(crate) fn ended(process: &Process) -> io::Result<bool> {
    let stat = gone_if_missing(std::fs::read_to_string(format!(
        "/proc/{}/stat",
        process.pid
    )))?;
    // Asked after the `stat` is read, so a `stat` of a process the id was
    // handed to since is never taken for this one's.
    Ok(!process.holds_its_id()? || stat.is_none_or(|stat| ended_in(&stat)))
}

/// The signal `ending` names.
fn signal_for(ending: crate::process::Ending) -> rustix::process::Signal {
    match ending {
        crate::process::Ending::Asked => rustix::process::Signal::TERM,
        crate::process::Ending::Forced => rustix::process::Signal::KILL,
    }
}

/// A process held for a signal: on macOS by its id and the record it was held
/// to, which is read again before each signal.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub(crate) struct Process {
    /// The record it was verified against.
    record: crate::process::Identity,
}

/// The process `record` names, and what the system says of it; nothing when
/// no process holds its id.
///
/// **macOS** has no `/proc` and no pidfd: whether a process holds the id is
/// asked with signal 0, and what it is with `ps` — its user, its start and its
/// executable, then its state — by the absolute path, in the C locale and UTC.
#[cfg(not(target_os = "linux"))]
pub(crate) fn open(
    record: &crate::process::Identity,
) -> io::Result<Option<(Process, crate::process::Seen)>> {
    let Some(pid) = pid_of(record.pid) else {
        return Ok(None);
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) | Err(rustix::io::Errno::PERM) => {}
        Err(rustix::io::Errno::SRCH) => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let number = record.pid.to_string();
    let Some(said) = asked_of_one("/bin/ps", &["-o", "uid=,lstart=,comm=", "-p", &number])? else {
        return Ok(None);
    };
    let (uid, started, command) = crate::process::seen_in_ps(&said).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("`ps` printed no user, start and executable for process {number}"),
        )
    })?;
    // A zombie keeps its id, its start and its name until its parent hears
    // it, so its state is what says it has ended.
    let state = asked_of_one("/bin/ps", &["-o", "stat=", "-p", &number])?.unwrap_or_default();
    let seen = crate::process::Seen {
        boot: Some(boot_session()?),
        started,
        executable: std::fs::canonicalize(command).ok(),
        ours: uid == rustix::process::getuid().as_raw(),
        ended: state.trim_start().starts_with('Z'),
    };
    Ok(Some((
        Process {
            record: record.clone(),
        },
        seen,
    )))
}

/// Sends `process` the signal `ending` names, once it is read again.
///
/// Only while it is still the process it was verified as: one that has ended
/// is not an error, and one that is no longer it is sent nothing.
#[cfg(not(target_os = "linux"))]
pub(crate) fn signal(process: &Process, ending: crate::process::Ending) -> io::Result<()> {
    let Some((_, seen)) = open(&process.record)? else {
        return Ok(());
    };
    match crate::process::judged(&process.record, &seen) {
        Ok(()) => {}
        Err(crate::process::Unverified::Gone(_)) => return Ok(()),
        Err(unverified) => {
            return Err(io::Error::other(format!(
                "nothing was sent: read again, {unverified}"
            )));
        }
    }
    let Some(pid) = pid_of(process.record.pid) else {
        return Ok(());
    };
    match rustix::process::kill_process(pid, signal_for(ending)) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Whether `process` has ended: no process holds its id, or the one that does
/// is no longer the one verified — a zombie among them, whose executable is
/// no longer read.
#[cfg(not(target_os = "linux"))]
pub(crate) fn ended(process: &Process) -> io::Result<bool> {
    Ok(match open(&process.record)? {
        None => true,
        Some((_, seen)) => crate::process::judged(&process.record, &seen).is_err(),
    })
}

/// What `program` prints for `arguments` about one process, in the C locale
/// and UTC; nothing when it says there is no such process — its exit code
/// nonzero, and nothing printed.
#[cfg(not(target_os = "linux"))]
#[expect(
    clippy::disallowed_methods,
    reason = "the system's own tool, named by its absolute path and handed an environment of \
              the C locale and UTC alone, is how macOS says what a process is"
)]
fn asked_of_one(program: &str, arguments: &[&str]) -> io::Result<Option<String>> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .env_clear()
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()?;
    if !output.status.success() && output.stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let said = String::from_utf8(output.stdout)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "an answer that is not UTF-8"))?;
    if output.status.success() {
        Ok(Some(said))
    } else {
        Err(io::Error::other(format!(
            "`{program}` answered {}",
            output.status
        )))
    }
}

/// What the system's tool `program` prints for `arguments`, in the C locale
/// and UTC and nothing else of this process's environment.
#[cfg(not(target_os = "linux"))]
#[expect(
    clippy::disallowed_methods,
    reason = "the system's own tool, named by its absolute path and handed an environment of \
              the C locale and UTC alone, is how macOS says when a process started"
)]
fn asked(program: &str, arguments: &[&str]) -> io::Result<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .env_clear()
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "`{program}` answered {}",
            output.status
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "an answer that is not UTF-8"))
}

/// The directory synced through a handle of its own, which is what makes a
/// rename into it durable.
pub(crate) fn sync_directory(directory: &Path) -> io::Result<()> {
    std::fs::File::open(directory)?.sync_all()
}

/// `buffer` filled from `/dev/urandom`, whole, or the error that stopped it.
///
/// `read_exact` reads until every byte is there, so a short read is retried
/// rather than handed back as a key with a tail of zeros.
pub(crate) fn random(buffer: &mut [u8]) -> io::Result<()> {
    use std::io::Read as _;
    std::fs::File::open("/dev/urandom")?.read_exact(buffer)
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

/// The most bytes a socket's path holds here, its terminating NUL apart: what
/// the system's `sun_path` holds, less one.
///
/// **Measured on the CI runners** as the longest path each binds: 107 bytes
/// on Linux, whose `sun_path` is 108, and 103 on macOS, whose is 104.
pub(crate) const NAME_MAX: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// How a refusal says what [`NAME_MAX`] bounds, before the number.
pub(crate) const NAME_HOLDS: &str = "a socket's path on this system holds at most";

/// What [`NAME_MAX`] counts.
pub(crate) const NAME_UNIT: &str = "bytes";

/// How long the endpoint's name is, as [`NAME_MAX`] counts it: its socket's
/// path, in bytes.
pub(crate) fn length_of(endpoint: &Endpoint) -> usize {
    endpoint.socket.as_os_str().len()
}

/// The endpoint's name as a person reads it: its socket's path.
pub(crate) fn name_of(endpoint: &Endpoint) -> String {
    endpoint.socket.display().to_string()
}

/// A connection through the socket.
pub(crate) type Stream = tokio::net::UnixStream;

/// The socket, listened on, and which file it made, so it removes that one
/// and nothing put at its name since.
#[derive(Debug)]
pub(crate) struct Listener {
    /// The listening socket.
    listening: tokio::net::UnixListener,
    /// Its path.
    path: std::path::PathBuf,
    /// The device and the inode of the file it made.
    made: (u64, u64),
}

/// A listener let go removes the socket it made.
impl Drop for Listener {
    /// The socket's file removed, when it is still the one this made: a socket
    /// nothing listens on is what the next bind judges and replaces.
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|found| (found.dev(), found.ino()) == self.made)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Whether what stands at a socket's name, read as `found`, is a socket the
/// user owns: the one thing a bind replaces, and only once nothing answers at it.
fn a_socket_of_its_own(path: &Path, found: &Metadata, user: &User) -> Result<(), Refused> {
    use std::os::unix::fs::FileTypeExt as _;

    let refused = |what: String| Err(Refused::NotPrivate(Refusal::of(path, what)));
    let file_type = found.file_type();
    if file_type.is_symlink() {
        return refused("is a symbolic link".to_owned());
    }
    if !file_type.is_socket() {
        return refused("is not a socket".to_owned());
    }
    if found.uid() != user.0 {
        return refused(format!(
            "belongs to user {} and this process runs as user {user}",
            found.uid()
        ));
    }
    Ok(())
}

/// The socket at `endpoint`'s path, bound for `user` and made `0600`.
///
/// **What stands at the name already is judged, never removed unread**: a
/// socket of this user's that refuses a connection is a dead daemon's and is
/// replaced; one something answers at is taken; anything else is refused and
/// left as it was. The caller holds the profile's instance lock, so no daemon
/// of the profile is between its stop and its socket's removal.
///
/// **Whether something answers is asked by connecting**, so a live listener is
/// handed that connection, closed at once: macOS no longer names a peer that
/// has closed, so its accept refuses it, and Linux, which still does, hands it
/// over as any other.
pub(crate) fn bind(
    endpoint: &Endpoint,
    user: &User,
    _connections: usize,
) -> Result<Listener, Refused> {
    let path = &endpoint.socket;
    let failed = |doing: &str, error: io::Error| {
        Refused::Failed(Failed::new(format!("`{}` {doing}", path.display()), error))
    };
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(failed("could not be read", error)),
        Ok(found) => {
            a_socket_of_its_own(path, &found, user)?;
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(_) => {
                    return Err(Refused::Taken {
                        name: name_of(endpoint),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(path)
                        .map_err(|error| failed("could not be removed", error))?;
                }
                Err(error) => return Err(failed("could not be asked whether it answers", error)),
            }
        }
    }
    let listening = tokio::net::UnixListener::bind(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AddrInUse {
            Refused::Taken {
                name: name_of(endpoint),
            }
        } else {
            failed("could not be bound", error)
        }
    })?;
    let made =
        std::fs::symlink_metadata(path).map_err(|error| failed("could not be read", error))?;
    // Built before the mode is set, so a failure from here on removes it.
    let listener = Listener {
        listening,
        path: path.clone(),
        made: (made.dev(), made.ino()),
    };
    std::fs::set_permissions(path, Permissions::from_mode(OWNER_OF_A_FILE))
        .map_err(|error| failed("could not be made private", error))?;
    Ok(listener)
}

/// What the system says about the other end of `stream`: the user its peer
/// credentials name, and the process.
fn said_by(stream: &Stream) -> io::Result<Said> {
    let credentials = stream.peer_cred()?;
    Ok(Said {
        users: vec![(Whose::Process, credentials.uid())],
        pid: credentials.pid().map(i64::from),
    })
}

/// The next connection to `listener`, and what the system says of its other
/// end.
pub(crate) async fn accept(listener: &mut Listener) -> io::Result<(Stream, Said)> {
    let (stream, _) = listener.listening.accept().await?;
    let said = said_by(&stream)?;
    Ok((stream, said))
}

/// A connection to `endpoint`'s socket, and what the system says of the
/// process listening on it.
pub(crate) async fn connect(endpoint: &Endpoint) -> io::Result<(Stream, Said)> {
    let stream = tokio::net::UnixStream::connect(&endpoint.socket).await?;
    let said = said_by(&stream)?;
    Ok((stream, said))
}

/// Whether an endpoint is a name on disk, which a bind judges: a socket's
/// path here.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const ENDPOINT_ON_DISK: bool = true;

/// Whether `listener`'s socket is what a bind makes: a socket of `user`'s,
/// mode `0600`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn endpoint_is_as_created(listener: &Listener, user: &User) -> bool {
    use std::os::unix::fs::FileTypeExt as _;

    std::fs::symlink_metadata(&listener.path).is_ok_and(|found| {
        found.file_type().is_socket()
            && found.uid() == user.0
            && found.mode() & 0o777 == OWNER_OF_A_FILE
    })
}

/// A socket at `endpoint`'s path that nothing listens on: what a daemon
/// stopped without removing its own leaves.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn leave_a_dead_endpoint(endpoint: &Endpoint) -> io::Result<()> {
    drop(std::os::unix::net::UnixListener::bind(&endpoint.socket)?);
    Ok(())
}

/// What a bind makes of what stands at `endpoint`'s path for `user`.
///
/// The very judgement [`bind`] makes before it asks whether anything answers
/// there, handed a user a test cannot be.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn judge_what_stands_at(endpoint: &Endpoint, user: &User) -> Result<(), Refused> {
    let path = &endpoint.socket;
    let found = std::fs::symlink_metadata(path).map_err(|error| {
        Refused::Failed(Failed::new(
            format!("`{}` could not be read", path.display()),
            error,
        ))
    })?;
    a_socket_of_its_own(path, &found, user)
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
