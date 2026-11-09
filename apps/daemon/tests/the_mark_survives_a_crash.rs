//! A daemon killed while it mints, and started again with its clock moved
//! back, hands out no identifier twice (`NOCTUA.md §16.2`).
//!
//! **The test runs itself as a child**, the way the local database's kill test
//! does: the child opens the real mark file and mints behind it on a scripted
//! clock, printing each identifier once `mint` has returned it, and the parent
//! reads them and kills the child abruptly — `SIGKILL` on Unix,
//! `TerminateProcess` on Windows — so no destructor, no clean stop and no
//! settle runs. The parent then starts from the mark the child left with its
//! clock moved back, by more than the lease and by more than a day.

use std::collections::BTreeSet;
use std::io::BufRead as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use core::time::Duration;

use nc_catalog::ErrorCode;
use nc_daemon::mark::MarkFile;
use nc_ids::{Clock, EPOCH_MS, MachineFlake, MintError, Minter};

/// The environment variable that turns this test into its own child: the
/// mark it mints behind.
const MINT_BEHIND: &str = "NC_DAEMON_MINT_BEHIND";
/// The millisecond the child's scripted clock starts at.
const MINT_FROM: &str = "NC_DAEMON_MINT_FROM";
/// How many identifiers the parent reads before it kills the child: enough
/// that the ceiling was raised more than once.
const READ_BEFORE_THE_KILL: usize = 5_000;
/// The most a child mints, so one whose parent has gone stops by itself.
const AT_MOST: usize = 1_000_000;
/// A day, in milliseconds: further ahead than this a mark is a wrong number.
const A_DAY_MS: u64 = 24 * 60 * 60 * 1_000;

/// A clock the test scripts: each reading is a millisecond after the last,
/// and a wait moves it past what it waits for.
#[derive(Debug, Clone)]
struct Scripted(Arc<AtomicU64>);

impl Scripted {
    fn at(millis: u64) -> Self {
        Self(Arc::new(AtomicU64::new(millis)))
    }

    fn advance(&self, by: Duration) {
        self.0.fetch_add(
            u64::try_from(by.as_millis()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }
}

impl Clock for Scripted {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }

    fn wait_past(&self, millis: u64) {
        self.0.fetch_max(millis + 1, Ordering::SeqCst);
    }
}

/// The flake the profile's device carries, the child's and the parent's.
fn flake() -> MachineFlake {
    MachineFlake::from_value(5).expect("a flake")
}

/// A scratch directory for one test, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("nc-daemon-crash-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        Self(root)
    }
}

/// The child, killed however the test leaves.
struct Minting(std::process::Child);

impl Drop for Minting {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Runs this test as a child that mints behind `mark` from `from`.
#[expect(
    clippy::disallowed_methods,
    reason = "the crash test's child is this same test binary, its standard error discarded \
              and its standard output read by the parent — the one way to kill a process \
              mid-mint and look at the mark it leaves"
)]
fn spawned(mark: &Path, from: u64) -> Minting {
    Minting(
        std::process::Command::new(std::env::current_exe().expect("this test binary"))
            .args([
                "--exact",
                "a_daemon_killed_while_it_mints_and_started_with_its_clock_back_repeats_none",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MINT_BEHIND, mark)
            .env(MINT_FROM, from.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("the child runs"),
    )
}

/// The child's whole life: mint behind the mark, printing each identifier and
/// its millisecond once `mint` has handed it out, until it is killed.
fn the_child(mark: &Path, from: u64) {
    let directory = mark.parent().expect("the mark is in a directory");
    let user = nc_platform::user_in(directory).expect("the scratch is private");
    let (file, read) = MarkFile::open(mark, &user, nc_catalog::DAEMON.mark_lease_ms)
        .expect("the child opens the mark");
    let clock = Scripted::at(from);
    let mut minter = nc_daemon::mark::minter(
        file,
        read,
        None,
        flake(),
        &clock,
        &nc_kernel::logging::Captured::default(),
        &mut |_| panic!("the child's clock is never behind its own mark"),
    )
    .expect("the child resumes");
    for _ in 0..AT_MOST {
        let handed_out = minter.mint().expect("the child mints");
        println!(
            "minted {} {}",
            handed_out.value(),
            handed_out.millis_since_epoch() + EPOCH_MS
        );
    }
}

/// A daemon killed while it mints, started again with its clock back by more
/// than the lease, waits for the clock, logs it, and repeats no identifier;
/// with its clock back by more than a day, it refuses to start.
#[test]
fn a_daemon_killed_while_it_mints_and_started_with_its_clock_back_repeats_none() {
    if let (Ok(mark), Ok(from)) = (std::env::var(MINT_BEHIND), std::env::var(MINT_FROM)) {
        the_child(Path::new(&mark), from.parse().expect("a millisecond"));
        return;
    }
    let scratch = Scratch::new("killed");
    let mark = scratch.0.join("ids.mark");
    let from = EPOCH_MS + 30 * A_DAY_MS;
    let mut child = spawned(&mark, from);
    let mut printed: Vec<(u64, u64)> = Vec::new();
    let lines = std::io::BufReader::new(child.0.stdout.take().expect("the child's output"));
    for line in lines.lines() {
        let line = line.expect("the child's output reads");
        let Some(rest) = line.strip_prefix("minted ") else {
            continue;
        };
        let (value, millis) = rest
            .split_once(' ')
            .expect("an identifier and its millisecond");
        printed.push((
            value.parse().expect("an identifier"),
            millis.parse().expect("a millisecond"),
        ));
        if printed.len() == READ_BEFORE_THE_KILL {
            break;
        }
    }
    assert_eq!(
        printed.len(),
        READ_BEFORE_THE_KILL,
        "the child stopped before it was killed"
    );
    child.0.kill().expect("the child is killed");
    let status = child.0.wait().expect("the child is reaped");
    assert!(!status.success(), "the child was not killed: {status}");

    let handed: BTreeSet<u64> = printed.iter().map(|(value, _)| *value).collect();
    assert_eq!(handed.len(), printed.len(), "the child itself repeated one");
    let last = printed
        .iter()
        .map(|(_, millis)| *millis)
        .max()
        .expect("minted");
    let user = nc_platform::user_in(&scratch.0).expect("the scratch is private");
    let lease = nc_catalog::DAEMON.mark_lease_ms;
    let (file, read) = MarkFile::open(&mark, &user, lease).expect("the mark the child left reads");
    let ceiling = read.expect("the child raised the ceiling before it minted");
    assert!(
        ceiling >= last,
        "the child handed out {last} with the durable ceiling at {ceiling}"
    );

    // **A day back: the start refuses**, since a mark that far ahead of the
    // clock is a wrong number rather than an early one.
    let a_day_back = Scripted::at(last - 2 * A_DAY_MS);
    let refusal = nc_daemon::mark::minter(
        file,
        read,
        None,
        flake(),
        &a_day_back,
        &nc_kernel::logging::Captured::default(),
        &mut |_| panic!("a mark more than a day ahead was waited for"),
    )
    .expect_err("a mark more than a day ahead refuses the start");
    assert_eq!(refusal.code(), ErrorCode::ValidationOutOfRange, "{refusal}");

    // **Back by more than the lease: minting is refused until the clock
    // passes, the wait is logged, and nothing handed out is handed out again.**
    let (file, read) = MarkFile::open(&mark, &user, lease).expect("the mark reads again");
    let behind = Scripted::at(last - (lease + 1_000));
    assert!(
        matches!(
            Minter::resuming_at(flake(), behind.clone(), ceiling),
            Err(MintError::ClockMovedBackwards { .. })
        ),
        "a minter resumed with its clock behind the mark"
    );
    let log = nc_kernel::logging::Captured::default();
    let mut waited: Vec<Duration> = Vec::new();
    let moved = behind.clone();
    let mut minter =
        nc_daemon::mark::minter(file, read, None, flake(), &behind, &log, &mut |drift| {
            waited.push(drift);
            moved.advance(drift + Duration::from_millis(1));
        })
        .expect("the start waits for the clock, and resumes");
    assert!(
        !waited.is_empty(),
        "a clock behind the mark was not waited for"
    );
    let logged = log.named("ids.clock_backwards");
    assert_eq!(logged.len(), 1, "{logged:?}");
    let after: Vec<u64> = (0..1_000)
        .map(|_| minter.mint().expect("mints past the mark").value())
        .collect();
    assert!(
        after.iter().all(|value| !handed.contains(value)),
        "an identifier the killed daemon handed out was handed out again"
    );
    let highest = handed.iter().max().expect("handed out");
    assert!(
        after.iter().all(|value| value > highest),
        "the restarted daemon minted below what the killed one reached"
    );
}
