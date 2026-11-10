//! A PID record naming a process that is not the daemon is never signalled
//! (`NOCTUA.md §17.1`; `NOCTUA-PLAN.md`, Stage 13, acceptance 4).
//!
//! **A sleeper stands in for whatever process an id was handed to next**:
//! this test binary started again, which says which process it is and waits
//! for its input to close. The daemon's PID record, in an install under a
//! scratch root whose daemon is not running, is written four ways it may be
//! found stale — the sleeper's id with a start that is not its own; the
//! sleeper's id and its own start with the daemon's executable; an id no
//! process holds; and a process numbered zero and below — and the CLI's
//! `stop`, in-process (`nc_cli::run`), is asked after each. Nothing answers at
//! the endpoint, so a stop that signalled an unverified process would signal
//! the sleeper: after each the sleeper still runs.
//!
//! **And the control that makes the four mean something**: the sleeper
//! recorded as it is, which a stop holds to its record and ends — so the four
//! are not passing because nothing is ever signalled. It spawns a process, so
//! no mutation command runs it.

use std::ffi::OsString;
use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};

use nc_catalog::ErrorCode;
use nc_channel::record::Record;
use nc_cli::exit::Exit;
use nc_daemon::testing::{Installed, PATIENCE};

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// What tells a child of this test that it is the sleeper.
const SLEEPER: &str = "NC_DAEMON_RECYCLED_PID_SLEEPER";

/// The sleeper, killed and waited for however the test leaves.
struct Sleeper(std::process::Child);

impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Sleeper {
    /// Whether it still runs.
    fn runs(&mut self) -> bool {
        self.0.try_wait().expect("the sleeper is asked").is_none()
    }
}

/// The sleeper started, and the record of it as it is.
#[expect(
    clippy::disallowed_methods,
    reason = "the process a stale record names has to be another one than this test; the \
              sleeper is this same test binary, its input a pipe the guard closes"
)]
fn a_sleeper() -> (Sleeper, Record) {
    let mut sleeper = Sleeper(
        std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "a_recycled_pid_is_never_signalled",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SLEEPER, "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("the sleeper starts"),
    );
    let mut lines = std::io::BufReader::new(sleeper.0.stdout.take().expect("piped"));
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            lines
                .read_line(&mut line)
                .expect("the sleeper's output reads")
                > 0,
            "the sleeper ended without saying which process it is"
        );
        if let Some(at) = line.find("record\t") {
            let record = Record::parse(line[at + "record\t".len()..].trim_end())
                .expect("the sleeper's record reads");
            return (sleeper, record);
        }
    }
}

/// `text` written as the PID record of `installed`, private and in place of
/// what was there.
fn recorded_as(installed: &Installed, text: &str) {
    let path = &installed.layout.pid_file;
    let _ = std::fs::remove_file(path);
    nc_platform::create_private(path)
        .expect("a record")
        .write_all(text.as_bytes())
        .expect("written");
}

/// The CLI's `stop`, for `found`'s install.
fn stop(found: &nc_channel::found::Found) -> nc_cli::Outcome {
    nc_cli::run(&[OsString::from("stop")], found, &nc_cli::Env::of(|_| None))
}

/// An id no process holds: a child's, once it has ended and been reaped.
#[expect(
    clippy::disallowed_methods,
    reason = "a child that lists nothing and ends, run for the id it leaves behind"
)]
fn an_id_no_process_holds() -> u32 {
    let mut child = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", "no::such::test"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the child starts");
    let pid = child.id();
    child.wait().expect("reaped");
    pid
}

/// A stale record — a recycled id, an id with another executable, an id no
/// process holds, a process numbered zero or below — is never signalled, and
/// the record of the sleeper as it is, is.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the sleeper says its record on its own output, which the parent reads"
)]
fn a_recycled_pid_is_never_signalled() {
    if std::env::var_os(SLEEPER).is_some() {
        let record = Record::of(
            nc_platform::process::this_process().expect("this process is read"),
            [9; 16],
        );
        let mut out = std::io::stdout().lock();
        writeln!(out, "record\t{}", record.to_canonical().expect("written")).expect("said");
        out.flush().expect("said");
        let mut rest = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut rest);
        return;
    }
    let installed = Installed::new(Path::new(BINARY));
    let found = nc_channel::found::beside(&installed.executable).expect("the install reads");
    let daemon: PathBuf =
        std::fs::canonicalize(&installed.executable).expect("the daemon is installed");
    let (mut sleeper, record) = a_sleeper();
    assert_ne!(record.executable, daemon);
    let gone = an_id_no_process_holds();
    let stale = [
        (
            "the sleeper's id with a start not its own",
            Record {
                started: format!("{}1", record.started),
                executable: daemon.clone(),
                ..record.clone()
            },
            Exit::Success,
        ),
        (
            "the sleeper's id and start with the daemon's executable",
            Record {
                executable: daemon.clone(),
                ..record.clone()
            },
            Exit::ProcessControl,
        ),
        (
            "an id no process holds",
            Record {
                pid: gone,
                executable: daemon,
                ..record.clone()
            },
            Exit::Success,
        ),
    ];
    for (named, stale, ends) in stale {
        recorded_as(&installed, &stale.to_canonical().expect("written"));
        let outcome = stop(&found);
        assert_eq!(outcome.exit, ends, "{named}: {outcome:?}");
        if ends == Exit::ProcessControl {
            assert!(
                outcome.stderr.starts_with(&format!(
                    "error[{}]",
                    ErrorCode::CliProcessUnverified.code()
                )),
                "{named}: {outcome:?}"
            );
        }
        assert!(sleeper.runs(), "{named}: the sleeper was signalled");
        assert!(installed.layout.stopped.is_file(), "{named}: no mark");
    }
    let whole = record.to_canonical().expect("written");
    let pid = format!("\"pid\":{}", record.pid);
    for nobody in ["0", "-1", &format!("-{}", record.pid)] {
        let planted = whole.replace(&pid, &format!("\"pid\":{nobody}"));
        assert_ne!(planted, whole);
        recorded_as(&installed, &planted);
        let outcome = stop(&found);
        assert_eq!(outcome.exit, Exit::ProcessControl, "{nobody}: {outcome:?}");
        assert!(
            outcome.stderr.contains("names no process"),
            "{nobody}: {outcome:?}"
        );
        assert!(sleeper.runs(), "process {nobody} was signalled");
    }
    // The control: the sleeper recorded as it is is held to its record, and
    // ended through it.
    recorded_as(&installed, &whole);
    let outcome = stop(&found);
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    let deadline = std::time::Instant::now() + PATIENCE;
    while sleeper.runs() {
        assert!(
            std::time::Instant::now() < deadline,
            "the sleeper recorded as it is was not ended: {outcome:?}"
        );
        std::thread::sleep(core::time::Duration::from_millis(20));
    }
}
