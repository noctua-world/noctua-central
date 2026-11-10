//! Which process this is, as the system says: the boot it runs in, when it
//! started, and the executable it runs (`NOCTUA.md §17.1`).
//!
//! **What a PID record holds, so a process is never taken for another.** A
//! process id is reused once its process is gone, so a record of the id alone
//! names whichever process holds it next; the boot and the start, read off the
//! system, tell the process the record was written by from any later holder of
//! its id. Each system's half reads them ([`crate::process::this_process`]);
//! the texts they read are parsed here, in code every system compiles, so the
//! Linux job tests the parser of each.
//!
//! **The start is the system's own spelling**, compared as text: clock ticks
//! since boot on Linux, the date `ps` prints on macOS, the creation time in
//! hundred-nanosecond intervals on Windows. Windows names no boot, so its
//! record carries none.
//!
//! # A process signalled is one held to its record first
//!
//! [`Verified::open`](crate::process::Verified::open) is the one way to a
//! process this crate signals: the process a record names, asked of the system
//! and held to every part of the record — its boot, its start, its user and its
//! executable — before anything can be sent to it. What each system asks:
//!
//! | System | Held through | What is read |
//! |---|---|---|
//! | Linux | a pidfd, opened first | `/proc/<pid>/stat` field 22 and its state, the boot's id, `/proc/<pid>/exe` and the owner of `/proc/<pid>`, then the pidfd asked again which process it holds |
//! | macOS | the process id | `/bin/ps -o uid=,lstart=,comm=` in the C locale and UTC, its state, and the boot's session, read again before each signal |
//! | Windows | one process handle | its creation time, its image's path and its token's user |
//!
//! **A signal goes through what was verified**: a Linux pidfd names one
//! process for as long as it is open, so a process id handed out again after
//! the verification is never reached through it; a Windows handle the same.
//! macOS has neither, so the process is read again just before each signal,
//! and the moment between that reading and the signal is left open (`AGENTS.md`,
//! What is deliberately not claimed). **Windows has no signal a process can
//! answer**: a terminate there is `TerminateProcess`, as hard as a kill.
//!
//! Which part of a record a process failed on is decided here, in code every
//! system compiles (`judged`), so the Linux job tests and mutates the
//! decision of every system; each system's half only reads.

use std::path::{Path, PathBuf};

use core::time::Duration;

use crate::imp;

/// A process, as the system says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Its id, which the system hands out again once it is gone.
    pub pid: u32,
    /// The boot it runs in, where the system names one.
    pub boot: Option<String>,
    /// When it started, in the system's own spelling.
    pub started: String,
    /// The executable it runs, canonicalized.
    pub executable: PathBuf,
}

/// This process, as the system says it is.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it: the boot, the start or the
/// executable could not be read.
pub fn this_process() -> std::io::Result<Identity> {
    let (boot, started) = imp::this_process()?;
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    Ok(Identity {
        pid: std::process::id(),
        boot,
        started,
        executable,
    })
}

/// Keeps this process's own standard handles from every child it spawns,
/// but as the standard handles a spawn hands one.
///
/// **What a spawner of a daemon asks first**: a daemon that held its
/// spawner's output open would keep whatever reads that output waiting for
/// its end for as long as the daemon lives. On Unix a spawn already replaces
/// each standard handle with what it hands the child, and nothing else is
/// left open across `exec`, so this does nothing; on Windows, where a child
/// is handed every inheritable handle of its parent, it clears the flag that
/// would hand it this process's own.
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it.
pub fn keep_standard_handles_from_children() -> std::io::Result<()> {
    imp::keep_standard_handles_from_children()
}

/// What the system says of a process a record is held to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    /// The boot it runs in, where the system names one.
    pub(crate) boot: Option<String>,
    /// When it started, in the system's own spelling.
    pub(crate) started: String,
    /// The executable it runs, canonicalized, or nothing where it could not be
    /// read.
    pub(crate) executable: Option<PathBuf>,
    /// Whether it runs as the user this process runs as.
    pub(crate) ours: bool,
    /// Whether it has ended and only its parent has yet to hear it.
    pub(crate) ended: bool,
}

/// Why a record was not held to a process, and nothing may be signalled.
#[derive(Debug)]
pub enum Unverified {
    /// The process the record names runs no more; the text says why.
    ///
    /// No process holds its id, the one that holds it started at another time
    /// or in another boot, or it has ended.
    Gone(String),
    /// A process holds the record's id and its start, and is not what it names.
    ///
    /// It runs another executable, or as another user, or is a process this
    /// user may not ask about; the text says which.
    Other(String),
    /// The system could not be asked.
    Failed(std::io::Error),
}

impl core::fmt::Display for Unverified {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Gone(why) | Self::Other(why) => write!(formatter, "{why}"),
            Self::Failed(error) => write!(formatter, "the system could not be asked: {error}"),
        }
    }
}

impl std::error::Error for Unverified {}

/// A process held to its record, and the one door a signal reaches it by.
#[derive(Debug)]
pub struct Verified {
    /// Its id.
    pid: u32,
    /// What the system half holds it through.
    held: imp::Process,
}

impl Verified {
    /// The process `record` names, once the system shows it to be that one.
    ///
    /// # Errors
    ///
    /// [`Unverified::Gone`] when no process the record names runs,
    /// [`Unverified::Other`] for one that holds its id and is not it — and for
    /// process zero, which a signal reads as a whole group — and
    /// [`Unverified::Failed`] when the system could not be asked.
    pub fn open(record: &Identity) -> Result<Self, Unverified> {
        if record.pid == 0 {
            return Err(Unverified::Other(
                "the record names process 0, which a signal reads as a whole group".to_owned(),
            ));
        }
        match imp::open(record) {
            Ok(Some((held, seen))) => {
                judged(record, &seen)?;
                Ok(Self {
                    pid: record.pid,
                    held,
                })
            }
            Ok(None) => Err(Unverified::Gone(format!("no process {} runs", record.pid))),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(Unverified::Other(format!(
                    "process {} is not one this user may ask about: {error}",
                    record.pid
                )))
            }
            Err(error) => Err(Unverified::Failed(error)),
        }
    }

    /// Its process id.
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// Asks it to end: `SIGTERM` on Unix; on Windows, which has no such
    /// request, `TerminateProcess`.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as the system answered it; on macOS also when the
    /// process read again is no longer the one verified, and nothing was sent.
    pub fn terminate(&self) -> std::io::Result<()> {
        imp::signal(&self.held, Ending::Asked)
    }

    /// Ends it outright: `SIGKILL` on Unix, `TerminateProcess` on Windows.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] as [`Verified::terminate`] answers it.
    pub fn kill(&self) -> std::io::Result<()> {
        imp::signal(&self.held, Ending::Forced)
    }

    /// Whether it has ended within `within`, waited for in short steps.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the system could not be asked.
    pub fn wait(&self, within: Duration) -> std::io::Result<bool> {
        let deadline = std::time::Instant::now() + within;
        loop {
            if imp::ended(&self.held)? {
                return Ok(true);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// How a process is asked to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    /// Asked, as a process may answer: `SIGTERM`.
    Asked,
    /// Forced: `SIGKILL`.
    Forced,
}

/// Whether what the system says of a process is what `record` names.
///
/// **Gone before other**: a process that has ended, or a record of another
/// boot or start, names a process that runs no more, whatever now holds its
/// id; only a process with the record's boot and start is told apart by its
/// user and its executable.
pub(crate) fn judged(record: &Identity, seen: &Seen) -> Result<(), Unverified> {
    let pid = record.pid;
    if seen.ended {
        return Err(Unverified::Gone(format!("process {pid} has ended")));
    }
    if seen.boot != record.boot {
        return Err(Unverified::Gone(format!(
            "the record is of another boot than this one, so process {pid} is not the one it \
             names"
        )));
    }
    if seen.started != record.started {
        return Err(Unverified::Gone(format!(
            "process {pid} started at {}, and the record's at {}: its id was handed out again",
            seen.started, record.started
        )));
    }
    if !seen.ours {
        return Err(Unverified::Other(format!(
            "process {pid} runs as another user"
        )));
    }
    match &seen.executable {
        None => Err(Unverified::Other(format!(
            "the executable process {pid} runs could not be read"
        ))),
        Some(executable) if *executable != record.executable => Err(Unverified::Other(format!(
            "process {pid} runs `{}`, and the record names `{}`",
            executable.display(),
            record.executable.display()
        ))),
        Some(_) => Ok(()),
    }
}

/// A process started detached from this one, which is waited for once it is
/// let go of, so it never lingers as a process nobody reaped.
#[derive(Debug)]
pub struct Detached {
    /// The child, until it is handed to the thread that waits for it.
    child: Option<std::process::Child>,
}

impl Detached {
    /// Its process id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.as_ref().map_or(0, std::process::Child::id)
    }

    /// How it ended, if it has.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the system could not be asked.
    pub fn ended(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match self.child.as_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }
}

impl Drop for Detached {
    fn drop(&mut self) {
        // Waited for on a thread of its own, so a process this one leaves
        // running is reaped when it ends rather than lingering as one nobody
        // heard; the thread goes with this process if that comes first.
        if let Some(mut child) = self.child.take() {
            let reaper = std::thread::Builder::new()
                .name("nc-reaper".to_owned())
                .spawn(move || child.wait());
            drop(reaper);
        }
    }
}

/// Starts `executable` with `arguments`, detached from this process: its
/// standard input and output null, its standard error `standard_error`, and
/// none of this process's own handles.
///
/// **Detached as each system detaches one**: on Unix the leader of a process
/// group of its own (`process_group(0)`), so no signal a terminal sends this
/// one's group reaches it; on Windows with no console and a group of its own
/// (`DETACHED_PROCESS`, `CREATE_NEW_PROCESS_GROUP`, `CREATE_NO_WINDOW`).
///
/// # Errors
///
/// [`std::io::Error`] as the system answered it: this process's handles could
/// not be kept from the child, or the child could not be started.
#[expect(
    clippy::disallowed_methods,
    reason = "a daemon started as its spawner starts one: its standard handles null or a file \
              its caller judged, never this process's"
)]
pub fn spawn_detached(
    executable: &Path,
    arguments: &[&str],
    standard_error: std::fs::File,
) -> std::io::Result<Detached> {
    keep_standard_handles_from_children()?;
    let mut command = std::process::Command::new(executable);
    command
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(standard_error);
    imp::detached(&mut command);
    Ok(Detached {
        child: Some(command.spawn()?),
    })
}

/// The state a Linux `/proc/<pid>/stat` holds, `Z` for an end not yet heard.
///
/// It is field 3, the first after the `)` that ends the process's name; `Z` is
/// a process that has ended and whose parent has not yet heard it.
#[cfg_attr(
    all(not(target_os = "linux"), not(test)),
    expect(
        dead_code,
        reason = "macOS and Windows have no `/proc`; read on Linux alone"
    )
)]
pub(crate) fn state_in_stat(stat: &str) -> Option<&str> {
    stat[stat.rfind(')')? + 1..].split_ascii_whitespace().next()
}

/// The process a Linux pidfd holds, as `/proc/self/fdinfo/<fd>` says it: its
/// id, or `-1` once it has been reaped.
#[cfg_attr(
    all(not(target_os = "linux"), not(test)),
    expect(dead_code, reason = "a pidfd is Linux's; read on Linux alone")
)]
pub(crate) fn pid_in_fdinfo(fdinfo: &str) -> Option<i64> {
    fdinfo
        .lines()
        .find_map(|line| line.strip_prefix("Pid:"))
        .and_then(|pid| pid.trim().parse().ok())
}

/// What `ps -o uid=,lstart=,comm=` prints for one process on macOS: its user's
/// number, its start as [`started_in_ps`] spells it, and its executable's path
/// — all that follows the start, spaces and all.
#[cfg_attr(
    all(any(target_os = "linux", windows), not(test)),
    expect(
        dead_code,
        reason = "Linux reads `/proc` and Windows its own call; `ps` is macOS's"
    )
)]
pub(crate) fn seen_in_ps(said: &str) -> Option<(u32, String, PathBuf)> {
    let line = said.lines().find(|line| !line.trim().is_empty())?.trim();
    let (uid, rest) = line.split_once(char::is_whitespace)?;
    let uid = uid.parse().ok()?;
    // The start is five words — a day, a month, a date, a time and a year —
    // and the executable is everything after them.
    let mut rest = rest.trim_start();
    let mut words = Vec::with_capacity(5);
    for _ in 0..5 {
        let (word, after) = rest.split_once(char::is_whitespace)?;
        words.push(word);
        rest = after.trim_start();
    }
    let started = started_in_ps(&words.join(" "))?;
    // The line is trimmed, so a fifth word followed by whitespace is followed
    // by something: what is left is never empty.
    Some((uid, started, PathBuf::from(rest)))
}

/// The start a Linux `/proc/<pid>/stat` holds: its field 22, counted after the
/// `)` that ends the process's name.
///
/// **After the last `)`**, because the name is the process's to choose and may
/// hold spaces and parentheses of its own; what follows it is the system's.
#[cfg_attr(
    all(not(target_os = "linux"), not(test)),
    expect(
        dead_code,
        reason = "macOS and Windows have no `/proc`; read on Linux alone"
    )
)]
pub(crate) fn started_in_stat(stat: &str) -> Option<String> {
    let after = &stat[stat.rfind(')')? + 1..];
    // Field 3, the state, is the first after the name; field 22 is the
    // twentieth from there.
    let started = after.split_ascii_whitespace().nth(19)?;
    started
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| started.to_owned())
}

/// The start `ps -o lstart=` prints on macOS, with its runs of spaces made one
/// so a day of the month padded or not is one spelling.
#[cfg_attr(
    all(any(target_os = "linux", windows), not(test)),
    expect(
        dead_code,
        reason = "Linux reads `/proc` and Windows its own call; `ps` is macOS's"
    )
)]
pub(crate) fn started_in_ps(said: &str) -> Option<String> {
    let words: Vec<&str> = said.split_ascii_whitespace().collect();
    // `Sun Nov  8 12:24:11 2026`: a day, a month, a date, a time and a year.
    (words.len() == 5).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use core::time::Duration;

    use super::{
        Identity, Seen, Unverified, Verified, judged, pid_in_fdinfo, seen_in_ps, started_in_ps,
        started_in_stat, state_in_stat,
    };

    /// How long a child of these tests is waited for at most.
    const PATIENCE: Duration = Duration::from_secs(60);

    /// What tells a child of [`a_child_is_held_to_its_record_and_ended_through_it`]
    /// that it is one: it says which process it is, then waits.
    const SLEEPER: &str = "NC_PLATFORM_PROCESS_SLEEPER";

    /// The word a detached child finds among its arguments.
    const DETACHED: &str = "nc-platform-detached-child";

    /// A scratch directory for one test, removed however the test leaves.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        /// A private directory of its own, under the temporary one.
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("nc-process-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            crate::create_private_directory(&root).expect("a scratch directory");
            Self(root)
        }
    }

    /// A child of this test binary, killed and waited for however the test
    /// leaves.
    struct Child(std::process::Child);

    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// The line a child says which process it is on, after `word`.
    fn saying(word: &str, identity: &Identity) -> String {
        format!(
            "{word}\t{}\t{}\t{}\t{}",
            identity.pid,
            identity.boot.clone().unwrap_or_default(),
            identity.started,
            identity.executable.display()
        )
    }

    /// The process a child said it is on `line`, after `word` — wherever on
    /// the line, since the test harness writes the test's name before what
    /// the test says.
    fn said(word: &str, line: &str) -> Option<Identity> {
        let at = line.find(&format!("{word}\t"))?;
        let mut parts = line[at..].trim_end_matches(['\r', '\n']).split('\t');
        (parts.next()? == word).then_some(())?;
        let pid = parts.next()?.parse().ok()?;
        let boot = parts.next()?;
        let started = parts.next()?.to_owned();
        let executable = PathBuf::from(parts.next()?);
        Some(Identity {
            pid,
            boot: (!boot.is_empty()).then(|| boot.to_owned()),
            started,
            executable,
        })
    }

    /// A child of this test binary that says which process it is and waits
    /// for its input to close, and what it said.
    #[expect(
        clippy::disallowed_methods,
        reason = "the process verified and ended has to be another one than this test; the \
                  child is this same test binary, its input a pipe the guard closes"
    )]
    fn a_sleeper() -> (Child, Identity) {
        use std::io::BufRead as _;
        let mut child = Child(
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args([
                    "--exact",
                    "process::tests::a_child_is_held_to_its_record_and_ended_through_it",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(SLEEPER, "1")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("the child starts"),
        );
        let mut lines = std::io::BufReader::new(child.0.stdout.take().expect("piped"));
        let mut line = String::new();
        loop {
            line.clear();
            assert!(
                lines
                    .read_line(&mut line)
                    .expect("the child's output reads")
                    > 0,
                "the child ended without saying which process it is"
            );
            if let Some(identity) = said("identity", &line) {
                return (child, identity);
            }
        }
    }

    /// The start is field 22 of a `stat` line, counted after the name however
    /// the name is spelled.
    #[test]
    fn the_start_is_field_22_counted_after_the_name() {
        let stat = "4242 (noctua-daemon) S 1 4242 4242 0 -1 4194560 120 0 0 0 1 2 0 0 20 0 \
                    4 0 987654 12345678 300 18446744073709551615";
        assert_eq!(started_in_stat(stat).as_deref(), Some("987654"));
        let named = "4242 (a) b) S 1 4242 4242 0 -1 4194560 120 0 0 0 1 2 0 0 20 0 4 0 \
                     987654 12345678 300 18446744073709551615";
        assert_eq!(started_in_stat(named).as_deref(), Some("987654"));
        assert_eq!(started_in_stat("4242 (noctua-daemon) S 1 2 3"), None);
        assert_eq!(started_in_stat("no name at all"), None);
        let lettered = stat.replace("987654", "98765x");
        assert_eq!(started_in_stat(&lettered), None);
    }

    /// The start `ps` prints is one spelling however its date is padded, and
    /// anything that is not a date is none.
    #[test]
    fn the_start_ps_prints_is_one_spelling() {
        assert_eq!(
            started_in_ps("  Sun Nov  8 12:24:11 2026\n").as_deref(),
            Some("Sun Nov 8 12:24:11 2026")
        );
        assert_eq!(
            started_in_ps("Sun Nov 18 12:24:11 2026").as_deref(),
            Some("Sun Nov 18 12:24:11 2026")
        );
        assert_eq!(started_in_ps(""), None);
        assert_eq!(started_in_ps("Sun Nov 8 12:24:11"), None);
    }

    /// This process is read as the system says it is: its own id, a start, and
    /// the executable that runs it.
    #[test]
    fn this_process_is_read_as_the_system_says_it_is() {
        let identity = super::this_process().expect("this process is read");
        assert_eq!(identity.pid, std::process::id());
        assert_ne!(identity.started, "");
        assert_eq!(
            identity.executable,
            std::fs::canonicalize(std::env::current_exe().expect("this executable"))
                .expect("canonical")
        );
        assert_eq!(
            super::this_process().expect("read again").started,
            identity.started,
            "a process's start read twice is two spellings"
        );
    }

    /// The state is the first field after the name, and a pidfd's process is
    /// read off its `fdinfo` as a number, `-1` once reaped.
    #[test]
    fn the_state_and_a_pidfds_process_are_read_as_linux_writes_them() {
        assert_eq!(state_in_stat("4242 (a) b) Z 1 4242"), Some("Z"));
        assert_eq!(state_in_stat("4242 (noctua-daemon) S 1"), Some("S"));
        assert_eq!(state_in_stat("4242 (noctua-daemon)"), None);
        assert_eq!(state_in_stat("no name at all"), None);
        let fdinfo =
            "pos:\t0\nflags:\t02000002\nmnt_id:\t15\nino:\t1057\nPid:\t4242\nNSpid:\t4242\n";
        assert_eq!(pid_in_fdinfo(fdinfo), Some(4242));
        assert_eq!(
            pid_in_fdinfo(&fdinfo.replace("4242\nNS", "-1\nNS")),
            Some(-1)
        );
        assert_eq!(pid_in_fdinfo("pos:\t0\nflags:\t02000002\n"), None);
        assert_eq!(pid_in_fdinfo("Pid:\tmany\n"), None);
    }

    /// What `ps` prints for one process is a user, a start and an
    /// executable, the executable's spaces kept.
    #[test]
    fn what_ps_prints_is_a_user_a_start_and_an_executable() {
        assert_eq!(
            seen_in_ps("  501 Sun Nov  8 12:24:11 2026     /bin/bash\n"),
            Some((
                501,
                "Sun Nov 8 12:24:11 2026".to_owned(),
                PathBuf::from("/bin/bash")
            ))
        );
        assert_eq!(
            seen_in_ps("501 Sun Nov 18 12:24:11 2026 /Users/r/Library/Application Support/x d\n"),
            Some((
                501,
                "Sun Nov 18 12:24:11 2026".to_owned(),
                PathBuf::from("/Users/r/Library/Application Support/x d")
            ))
        );
        for unread in [
            "",
            "\n",
            "  501 Sun Nov  8 12:24:11 2026",
            "  501 Sun Nov  8 12:24:11 2026   \n",
            "  me Sun Nov  8 12:24:11 2026 /bin/bash",
            "  -1 Sun Nov  8 12:24:11 2026 /bin/bash",
            "  501 Sun Nov  8 12:24:11 /bin/bash",
        ] {
            assert_eq!(seen_in_ps(unread), None, "{unread:?}");
        }
    }

    /// A record is held to every part the system names, and which part it
    /// failed on says whether its process runs no more or is another.
    #[test]
    fn a_record_is_held_to_every_part_the_system_names() {
        let record = Identity {
            pid: 4242,
            boot: Some("b1".to_owned()),
            started: "987654".to_owned(),
            executable: PathBuf::from("/opt/noctua/noctua-daemon"),
        };
        let seen = Seen {
            boot: record.boot.clone(),
            started: record.started.clone(),
            executable: Some(record.executable.clone()),
            ours: true,
            ended: false,
        };
        judged(&record, &seen).expect("everything the record names is what the system says");
        let gone = |seen: Seen| match judged(&record, &seen) {
            Err(Unverified::Gone(why)) => why,
            other => panic!("gone, and it was {other:?}"),
        };
        let other = |seen: Seen| match judged(&record, &seen) {
            Err(Unverified::Other(why)) => why,
            other => panic!("another process, and it was {other:?}"),
        };
        let ended = gone(Seen {
            ended: true,
            ..seen.clone()
        });
        assert!(ended.contains("has ended"), "{ended}");
        for boot in [Some("b2".to_owned()), None] {
            let rebooted = gone(Seen {
                boot,
                ..seen.clone()
            });
            assert!(rebooted.contains("another boot"), "{rebooted}");
        }
        let restarted = gone(Seen {
            started: "987655".to_owned(),
            ..seen.clone()
        });
        assert!(restarted.contains("handed out again"), "{restarted}");
        // **Gone before other**: an id handed out again to another user's
        // process names a record's process that runs no more.
        let handed = gone(Seen {
            started: "1".to_owned(),
            ours: false,
            executable: None,
            ..seen.clone()
        });
        assert!(handed.contains("handed out again"), "{handed}");
        let theirs = other(Seen {
            ours: false,
            ..seen.clone()
        });
        assert!(theirs.contains("another user"), "{theirs}");
        let unread = other(Seen {
            executable: None,
            ..seen.clone()
        });
        assert!(unread.contains("could not be read"), "{unread}");
        let another = other(Seen {
            executable: Some(PathBuf::from("/bin/sleep")),
            ..seen
        });
        assert!(
            another.contains("`/bin/sleep`") && another.contains("noctua-daemon"),
            "{another}"
        );
    }

    /// A child is held to its record and ended through it.
    ///
    /// It is refused as soon as one part of the record is not its own, asked to
    /// end through what held it, and gone once it has; another is killed
    /// outright, and process zero is never opened.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the child says which process it is on its own output, which the parent reads"
    )]
    fn a_child_is_held_to_its_record_and_ended_through_it() {
        use std::io::{Read as _, Write as _};
        if std::env::var_os(SLEEPER).is_some() {
            let identity = super::this_process().expect("this process is read");
            let mut out = std::io::stdout().lock();
            writeln!(out, "{}", saying("identity", &identity)).expect("said");
            out.flush().expect("said");
            let mut rest = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut rest);
            return;
        }
        let (mut child, identity) = a_sleeper();
        assert_eq!(identity.pid, child.0.id());
        let verified = Verified::open(&identity).expect("the child is its own record");
        assert_eq!(verified.pid(), identity.pid);
        assert!(
            !verified.wait(Duration::ZERO).expect("asked"),
            "a child still running has ended"
        );
        for (record, gone) in [
            (
                Identity {
                    started: format!("{}0", identity.started),
                    ..identity.clone()
                },
                true,
            ),
            (
                Identity {
                    boot: Some("another boot".to_owned()),
                    ..identity.clone()
                },
                true,
            ),
            (
                Identity {
                    executable: identity.executable.with_file_name("noctua-daemon"),
                    ..identity.clone()
                },
                false,
            ),
        ] {
            match Verified::open(&record) {
                Err(Unverified::Gone(_)) if gone => {}
                Err(Unverified::Other(_)) if !gone => {}
                answered => panic!("{record:?} was answered {answered:?}"),
            }
        }
        assert!(matches!(
            Verified::open(&Identity {
                pid: 0,
                ..identity.clone()
            }),
            Err(Unverified::Other(_))
        ));
        verified.terminate().expect("asked to end");
        assert!(
            verified.wait(PATIENCE).expect("asked"),
            "the child did not end once asked to"
        );
        let status = child.0.wait().expect("reaped");
        assert!(!status.success(), "the child ended on its own: {status:?}");
        assert!(
            matches!(Verified::open(&identity), Err(Unverified::Gone(_))),
            "a child reaped is still taken for its record"
        );
        verified
            .terminate()
            .expect("a process that has ended is asked to end in vain, and no error");
        let (mut killed, identity) = a_sleeper();
        let verified = Verified::open(&identity).expect("the child is its own record");
        verified.kill().expect("killed");
        assert!(
            verified.wait(PATIENCE).expect("asked"),
            "the child outlived a kill"
        );
        assert!(!killed.0.wait().expect("reaped").success());
    }

    /// A child started detached writes where it was told and leads a group.
    ///
    /// The group is its own where the system has one, its standard error the
    /// file it was handed, and it is held to its record and ended through it.
    #[test]
    #[expect(
        clippy::print_stderr,
        reason = "the detached child's one report, to the standard error its parent handed it \
                  as a file"
    )]
    fn a_detached_child_leads_a_group_of_its_own_and_writes_where_it_was_told() {
        if std::env::args().any(|argument| argument == DETACHED) {
            let identity = super::this_process().expect("this process is read");
            let group = crate::testing::group_of(identity.pid).expect("the group is read");
            eprintln!(
                "{}\t{}",
                saying("detached", &identity),
                group.map_or_else(String::new, |group| group.to_string())
            );
            // Ended by its parent, and on its own once the wait is over, so
            // a parent that failed leaves nothing running for long.
            std::thread::sleep(PATIENCE);
            return;
        }
        let scratch = Scratch::new("detached");
        let error_file = scratch.0.join("detached.err");
        let error = crate::create_private(&error_file).expect("the child's standard error");
        let mut detached = super::spawn_detached(
            &std::env::current_exe().expect("this test binary"),
            &[
                "--exact",
                "process::tests::a_detached_child_leads_a_group_of_its_own_and_writes_where_it_was_told",
                "--nocapture",
                "--test-threads=1",
                "--skip",
                DETACHED,
            ],
            error,
        )
        .expect("the child starts");
        let deadline = std::time::Instant::now() + PATIENCE;
        let report = loop {
            let written = std::fs::read_to_string(&error_file).unwrap_or_default();
            if let Some(line) = written.lines().find(|line| line.contains("detached\t")) {
                break line.to_owned();
            }
            assert!(
                detached.ended().expect("asked").is_none(),
                "the detached child ended before it said anything: {written}"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "the detached child said nothing: {written}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        let identity = said("detached", &report).expect("the child said which process it is");
        assert_eq!(identity.pid, detached.pid(), "{report}");
        if crate::system::POSIX_SESSIONS {
            assert_eq!(
                report.rsplit('\t').next(),
                Some(identity.pid.to_string().as_str()),
                "the detached child does not lead a group of its own: {report}"
            );
        }
        let verified = Verified::open(&identity).expect("the detached child is its own record");
        verified.terminate().expect("asked to end");
        assert!(
            verified.wait(PATIENCE).expect("asked"),
            "the detached child did not end once asked to"
        );
        drop(detached);
    }
}
