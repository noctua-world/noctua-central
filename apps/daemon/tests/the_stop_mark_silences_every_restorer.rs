//! The development daemon is restored after a crash and stays down after a
//! stop, whatever restores it (`NOCTUA.md §17.3`, `§17.5`; `NOCTUA-PLAN.md`,
//! Stage 13, acceptance 8).
//!
//! **Installed from the plan `cargo xtask dev --install` makes**,
//! `nc_cli::install::development_plan`, with no plugins: the daemon this
//! build made is registered with this system's service manager — a systemd
//! user unit, a launch agent, a scheduled task — as `noctua-daemon
//! --restored`, and started through it. The install is this checkout's
//! development channel, relocated under a namespace that carries this process
//! and a nonce, so every name it gives the system is the layout's and a
//! test's: the unit is written only in `$XDG_RUNTIME_DIR/systemd/user`, the
//! agent's property list stays in the scratch it is bootstrapped from, and the
//! task sits in the root folder, where it starts at each logon until it is
//! unregistered.
//!
//! **Read through the manager's own counters**, and through the daemon's own
//! PID record, never through sleeps alone: killed, it is restored as a new
//! instance; installed again over it, it is stopped, its restorer let go and
//! registered again, and started through it; stopped with `noctua stop`, the
//! manager starts it as a restorer would and it ends at once on the `stopped`
//! mark and stays down, and an install made then keeps it down; started
//! again, the mark written and then the daemon killed, it is restored once,
//! ends well on the mark, and stays down — on Windows, whose restorer is a
//! repetition every minute, started each minute and ending well each time.
//!
//! **What it leaves**: a guard uninstalls the install — the daemon stopped
//! through the CLI and its restorer unregistered, and unregistered directly
//! when the uninstall did not get that far — however the test leaves. Every
//! run first sweeps this checkout's leftovers whose owner is dead, by a lock
//! the owner holds in `nc-registered`, under the `target` of this checkout, as
//! the service manager's own suite does, and **only a name whose lock's
//! directory is there**: it is made before anything is registered under the
//! name and kept until its restorer is unregistered, so a service of this
//! checkout no run of this suite registered is never touched. It never
//! enables lingering, never reloads the manager, and touches no service it did
//! not register. It spawns processes and registers a service, so no mutation
//! command runs it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nc_channel::found::Found;
use nc_channel::record::Record;
use nc_cli::exit::Exit;
use nc_platform::process::{Unverified, Verified};
use nc_platform::service::{Counters, MANAGER, Manager, Registrar};

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// How long a manager has to restore the daemon, or to start it: a minute
/// of the repetition Task Scheduler restores by, and the slack around it.
const WITHIN: Duration = Duration::from_secs(150);

/// How long a daemon left down is watched staying down: five of the delays
/// its restorer waits.
const LEFT_DOWN_FOR: Duration = Duration::from_secs(5);

/// The lock a scratch's owner holds in it for as long as it lives.
const OWNER: &str = "owner.lock";

/// The file a scratch records its install's namespace in, for a sweep to
/// remove the root named for it.
const NAMESPACE: &str = "namespace";

/// How long a scratch is too new for a sweep to judge whether its owner
/// lives: past the moment between its directory and its lock.
const NEW_FOR: Duration = Duration::from_secs(60);

/// What a scratch leaves in the runtime unit directory it made.
///
/// The service manager's own suite marks it so too, and either takes it away
/// once nothing but the mark is left.
const MADE_BY_A_SCRATCH: &str = ".made-by-nc-platform-tests";

/// The directory of links `enable` makes for the units `default.target`
/// wants, in the unit directory a unit is kept in.
const WANTED: &str = "default.target.wants";

/// This checkout: the directory two above this crate's.
fn checkout() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("a crate two directories into its checkout")
        .to_owned()
}

/// Where every run of this checkout keeps its scratches' locks, whatever
/// its environment names as temporary.
fn scratches() -> PathBuf {
    checkout().join("target").join("nc-registered")
}

/// `number` in base 36, the shortest spelling a namespace's alphabet holds.
fn short(mut number: u32) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit(number % 36, 36).unwrap_or('0'));
        number /= 36;
        if number == 0 {
            break;
        }
    }
    digits.iter().rev().collect()
}

/// A namespace no other process's install spells: `r`, this process, `n`,
/// and a nonce, which holds the digit a namespace needs.
///
/// **Short**, because macOS holds a socket's path to 103 bytes and the
/// scratch root is named for it too: under a temporary directory of 49
/// bytes, as the hosted runner's is, the socket's path is 101 at most.
fn namespace() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    format!("r{}n{}", short(std::process::id()), nanos % 100)
}

/// The root an install relocated under `namespace` is made under, in this
/// process's temporary directory.
fn root_of(namespace: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nc-rst-{namespace}"))
}

/// The name a service manager knows a service by, without what the
/// manager adds to one.
fn stem_of(name: &str) -> &str {
    name.strip_suffix(".service")
        .unwrap_or(name)
        .trim_start_matches('\\')
}

/// Whether the owner of the scratch at `scratch` still holds its lock.
fn owner_lives(scratch: &Path) -> bool {
    std::fs::File::open(scratch.join(OWNER))
        .is_ok_and(|lock| matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
}

/// Whether the scratch at `scratch` is too new to judge: made or written in
/// less than [`NEW_FOR`] ago, or when cannot be read.
fn too_new(scratch: &Path) -> bool {
    std::fs::metadata(scratch)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_none_or(|age| age < NEW_FOR)
}

/// Removes the scratch at `scratch`, and the root of the install whose
/// namespace it records, under this process's temporary directory.
fn take_away(scratch: &Path) {
    let recorded = std::fs::read_to_string(scratch.join(NAMESPACE)).ok();
    if let Some(namespace) = recorded.as_deref().map(nc_channel::Namespace::parse)
        && let Ok(namespace) = namespace
    {
        let root = root_of(namespace.as_str());
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
    }
    let _ = std::fs::remove_dir_all(scratch);
}

/// Takes the runtime unit directory `units` away, if a scratch made it and
/// nothing but the mark is left in it.
fn take_the_units_away(units: &Path) {
    let Ok(entries) = std::fs::read_dir(units) else {
        return;
    };
    let names: Vec<OsString> = entries.flatten().map(|entry| entry.file_name()).collect();
    if names == [MADE_BY_A_SCRATCH] {
        let _ = std::fs::remove_file(units.join(MADE_BY_A_SCRATCH));
        let _ = std::fs::remove_dir(units);
    }
}

/// The CLI run with `verb` for `found`'s install, in-process.
fn run(found: &Found, verb: &str) -> nc_cli::Outcome {
    nc_cli::run(&[OsString::from(verb)], found, &nc_cli::Env::of(|_| None))
}

/// This checkout's development install, registered with the service manager.
///
/// It is relocated under a namespace of its own, and taken away however the
/// test leaves.
struct Scratch {
    /// The scratch holding the owner's lock, named for the service.
    lock_dir: PathBuf,
    /// The lock, held for as long as this lives.
    owner: Option<std::fs::File>,
    /// The install's root, under the temporary directory.
    root: PathBuf,
    /// The stand-in for the CLI the install copies, which is driven
    /// in-process.
    cli: PathBuf,
    /// The install, once made.
    found: Option<Found>,
    /// This system's manager, for this user.
    registrar: Registrar,
    /// The name the manager knows the daemon by.
    name: String,
    /// Where the install keeps its restorer's definition.
    kept_in: PathBuf,
    /// The directory of `default.target`'s links, when this test's enable is
    /// what makes it.
    wanted_made: Option<PathBuf>,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(found) = self.found.take() {
            let _ = nc_cli::install::uninstall(&found.install);
        }
        // **Unregistered directly when the uninstall did not get that far**:
        // one whose daemon did not stop returns before it lets the restorer
        // go.
        if !matches!(self.registrar.is_registered(&self.name), Ok(false)) {
            let _ = self.registrar.unregister(&self.name, &self.kept_in);
        }
        let gone = matches!(self.registrar.is_registered(&self.name), Ok(false));
        if let Some(wanted) = &self.wanted_made {
            let _ = std::fs::remove_dir(wanted);
        }
        if let Some(units) = self.registrar.volatile_units() {
            take_the_units_away(&units);
        }
        drop(self.owner.take());
        let _ = nc_platform::testing::make_private(&self.root);
        let _ = std::fs::remove_dir_all(&self.root);
        // **The lock's directory outlives a restorer that stays**, so the next
        // run's sweep finds it, its owner dead, and unregisters it.
        if gone {
            let _ = std::fs::remove_dir_all(&self.lock_dir);
        }
    }
}

impl Scratch {
    /// The install this test makes, once this checkout's dead leftovers are swept.
    ///
    /// This checkout's development channel under a namespace of its own, its bases
    /// under a short root in the temporary directory, and its owner's lock
    /// taken.
    fn new() -> (Self, nc_channel::Install) {
        let namespace = namespace();
        let root = root_of(&namespace);
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch root");
        let base = |name: &str| {
            let base = root.join(name);
            nc_platform::create_private_directory(&base).expect("a base");
            base
        };
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Development(nc_channel::Checkout::of(&checkout())),
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain absolute bases"),
            namespace: Some(nc_channel::Namespace::parse(&namespace).expect("a namespace")),
            log_endpoint: None,
        };
        let cli = root.join("cli");
        std::io::Write::write_all(
            &mut nc_platform::create_private(&cli).expect("a stand-in CLI"),
            b"the CLI is driven in-process",
        )
        .expect("written");
        let user = nc_platform::user_in(&root).expect("the scratch is private");
        let registrar = Registrar::for_user(&user).expect("this system's service manager");
        let layout = nc_channel::layout(&install, &user.to_string());
        let name = nc_cli::install::restorer_name(&layout, MANAGER).to_owned();
        let kept_in = nc_cli::install::kept_in(
            &install,
            &layout,
            MANAGER,
            registrar.volatile_units().as_deref(),
        )
        .expect("a directory the restorer is kept in");
        // **The lock's directory before anything is registered under the
        // name**, so a sweep reads every service of this suite's as one it
        // recorded.
        let scratches = scratches();
        std::fs::create_dir_all(&scratches).expect("this checkout's scratches");
        let lock_dir = scratches.join(stem_of(&name));
        nc_platform::create_private_directory(&lock_dir).expect("a lock's directory");
        let owner = nc_platform::create_private(&lock_dir.join(OWNER)).expect("a lock");
        owner.try_lock().expect("a lock nobody else holds");
        std::io::Write::write_all(
            &mut nc_platform::create_private(&lock_dir.join(NAMESPACE))
                .expect("the namespace's record"),
            namespace.as_bytes(),
        )
        .expect("written");
        let mut scratch = Self {
            lock_dir,
            owner: Some(owner),
            root,
            cli,
            found: None,
            registrar,
            name,
            kept_in,
            wanted_made: None,
        };
        scratch.sweep(&install, &user);
        scratch.wanted_made = scratch
            .registrar
            .volatile_units()
            .map(|units| units.join(WANTED))
            .filter(|wanted| !wanted.is_dir());
        if let Some(units) = scratch.registrar.volatile_units()
            && !units.is_dir()
        {
            nc_platform::create_private_directory(&units).expect("the runtime unit directory");
            drop(
                nc_platform::create_private(&units.join(MADE_BY_A_SCRATCH))
                    .expect("a mark in the directory just made"),
            );
        }
        (scratch, install)
    }

    /// Unregisters each service a run of this test registered for this
    /// checkout whose owner is dead, and takes away every scratch of theirs
    /// whose owner is.
    ///
    /// **The prefix is the layout's**, for a namespace opening as this test's
    /// do, and held again before each name is unregistered, since launchd
    /// and Task Scheduler list every service they hold. **And only a name a
    /// run of this test recorded**, by its lock's directory: a relocated
    /// install a person made of this checkout, whose namespace opens so too,
    /// has none, and is left as it is.
    fn sweep(&self, install: &nc_channel::Install, user: &nc_platform::User) {
        let probe = nc_channel::Install {
            namespace: Some(nc_channel::Namespace::parse("r0").expect("a namespace")),
            ..install.clone()
        };
        let named =
            nc_cli::install::restorer_name(&nc_channel::layout(&probe, &user.to_string()), MANAGER)
                .to_owned();
        let at = named.find("r0").expect("the namespace is in the name");
        let prefix = &named[..=at];
        assert!(
            prefix.ends_with("-r") || prefix.ends_with("_r"),
            "`{prefix}` does not end where a namespace begins"
        );
        let scratches = scratches();
        for name in self
            .registrar
            .registered_under(prefix)
            .expect("this checkout's services are listed")
        {
            assert!(
                name.starts_with(prefix),
                "`{name}` was answered as one of this checkout's, under `{prefix}`, and is not"
            );
            let theirs = scratches.join(stem_of(&name));
            if !theirs.is_dir() || owner_lives(&theirs) || too_new(&theirs) {
                continue;
            }
            let directory = self
                .registrar
                .volatile_units()
                .unwrap_or_else(|| theirs.clone());
            match self.registrar.unregister(&name, &directory) {
                Ok(()) => take_away(&theirs),
                // **Left for the manager's next start**: stopped, its file
                // and its links gone, and wanted by a unit the manager runs
                // since something reloaded it, which no test does.
                Err(nc_platform::service::Error::StillWanted(_)) => {}
                Err(error) => panic!("`{name}`, a leftover of this checkout, stays: {error}"),
            }
        }
        let stems = stem_of(prefix).to_owned();
        let listed = self.registrar.registered_under(prefix).unwrap_or_default();
        if let Ok(entries) = std::fs::read_dir(&scratches) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ours = entry
                    .file_name()
                    .to_str()
                    .is_some_and(|scratch| scratch.starts_with(&stems));
                let still_registered = listed
                    .iter()
                    .any(|name| scratches.join(stem_of(name)) == path);
                if ours
                    && !still_registered
                    && path != self.lock_dir
                    && !too_new(&path)
                    && !owner_lives(&path)
                {
                    take_away(&path);
                }
            }
        }
    }

    /// Installs `install` as `dev --install` does, and answers what it made.
    ///
    /// The daemon this build made, the test world, its restorer registered and the
    /// daemon started through it.
    fn install(&mut self, install: &nc_channel::Install) -> nc_cli::install::Installed {
        let world = nc_cli::install::development_world(include_str!("../../../policy/world.json"))
            .expect("the test world");
        let plan = nc_cli::install::development_plan(
            install.clone(),
            &self.cli,
            Path::new(BINARY),
            &world,
            Vec::new(),
        );
        let installed = nc_cli::install::install(&plan);
        // **Held before the outcome is read**, so an install that registered
        // and then failed is uninstalled by the guard too.
        let layout = nc_channel::layout(
            install,
            &nc_platform::user_in(&self.root)
                .expect("private")
                .to_string(),
        );
        self.found = nc_channel::found::beside(&layout.install.join(format!(
            "{}{}",
            layout.cli,
            nc_platform::EXECUTABLE_SUFFIX
        )))
        .ok();
        installed.unwrap_or_else(|unmade| panic!("not installed: {unmade}"))
    }

    /// The install made.
    fn found(&self) -> &Found {
        self.found.as_ref().expect("installed")
    }

    /// The daemon's PID record, if one reads.
    fn record(&self) -> Option<Record> {
        nc_channel::record::read(&self.found().layout.pid_file, &self.found().user).ok()
    }

    /// The PID record once `holds` is true of it, [`WITHIN`] at most.
    fn recorded_once(&self, holds: impl Fn(&Record) -> bool, what: &str) -> Record {
        let until = Instant::now() + WITHIN;
        loop {
            if let Some(record) = self.record()
                && holds(&record)
            {
                return record;
            }
            assert!(
                Instant::now() < until,
                "the daemon's record was waited for {WITHIN:?} to be {what}, and is {:?}. {}",
                self.record(),
                self.what_the_manager_says()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The manager's counters of the daemon.
    fn counters(&self) -> Counters {
        self.registrar
            .counters(&self.name)
            .unwrap_or_else(|error| panic!("`{}` was not counted: {error}", self.name))
    }

    /// The manager's counters once `holds` is true of them, waited for
    /// [`WITHIN`] at most.
    fn counted_once(&self, holds: impl Fn(&Counters) -> bool, what: &str) -> Counters {
        let until = Instant::now() + WITHIN;
        loop {
            let counted = self.counters();
            if holds(&counted) {
                return counted;
            }
            assert!(
                Instant::now() < until,
                "`{}` was waited for {WITHIN:?} to be {what}, and is {counted:?}. {}",
                self.name,
                self.what_the_manager_says()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The daemon running as a new instance, started through its manager:
    /// its record, after `before`'s, held to the process the manager says
    /// runs, and the manager's counters then.
    fn running_after(&self, before: &Record) -> (Record, Counters) {
        let record = self.recorded_once(
            |record| {
                record.instance != before.instance && Verified::open(&record.identity()).is_ok()
            },
            "a new instance, running",
        );
        let counted = self.counted_once(Counters::running, "running");
        if let Some(pid) = counted.pid() {
            assert_eq!(
                pid, record.pid,
                "its manager runs another process than its record"
            );
        }
        (record, counted)
    }

    /// The registration's own start ended on the mark.
    ///
    /// **As its manager counts it**: the start of launchd's load and of the
    /// registration trigger of Task Scheduler, each ended well, and under
    /// systemd the unit down again once its start returned.
    fn ended_on_the_mark_once_registered(&self) {
        match MANAGER {
            Manager::Systemd => {}
            Manager::Launchd => {
                self.counted_once(
                    |now| {
                        matches!(now, Counters::Launchd { runs, .. } if *runs >= 1)
                            && !now.running()
                            && now.ended_well() == Some(true)
                    },
                    "started by its load and ended well on the mark",
                );
            }
            Manager::TaskScheduler => {
                self.counted_once(
                    |now| !now.running() && now.ended_well() == Some(true),
                    "started by its registration and ended well on the mark",
                );
            }
        }
        self.counted_once(|now| !now.running(), "down, on the mark");
    }

    /// Whether the daemon stays down for [`LEFT_DOWN_FOR`]: its manager
    /// running nothing of it, nothing answering at its endpoint, and no PID
    /// record naming a process that runs.
    ///
    /// **Not watched under Task Scheduler**, whose repetition starts it each
    /// minute to end at once on the mark: what is held there is the record and
    /// the endpoint.
    fn stays_down(&self) {
        let until = Instant::now() + LEFT_DOWN_FOR;
        while Instant::now() < until {
            if !MANAGER.restarts_a_clean_exit() {
                let counted = self.counters();
                assert!(
                    !counted.running(),
                    "`{}` was started again and runs: {counted:?}",
                    self.name
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let status = run(self.found(), "status");
        assert_eq!(
            status.exit,
            Exit::Unreachable,
            "the daemon answers: {status:?}"
        );
        if let Some(record) = self.record() {
            assert!(
                matches!(Verified::open(&record.identity()), Err(Unverified::Gone(_))),
                "a daemon runs as its record names: {record:?}"
            );
        }
    }

    /// Everything the manager says of the daemon, for a failure to show.
    fn what_the_manager_says(&self) -> String {
        format!(
            "{} says {:?}; the record is {:?}",
            MANAGER,
            self.registrar.counters(&self.name),
            self.record()
        )
    }
}

/// The record `record` names a process this test kills, held to it first.
fn killed(record: &Record) {
    let verified = Verified::open(&record.identity())
        .unwrap_or_else(|unverified| panic!("{record:?} is not held to its process: {unverified}"));
    verified.kill().expect("killed");
    assert!(
        verified.wait(WITHIN).expect("asked"),
        "the daemon outlived a kill"
    );
}

/// After a kill with the `stopped` mark written, the restorer restored the
/// daemon once and the daemon ended well, as `before` the kill its counters
/// said — each manager by its own counters.
///
/// **systemd forgets a unit that ended well**, its counters with it, so its
/// restoration is read as it happens: the unit activating again after the
/// kill, then inactive — not failed, not running.
fn restored_once_and_ended_well(scratch: &Scratch, before: &Counters, killed: u32) {
    match MANAGER {
        Manager::Systemd => {
            let until = Instant::now() + WITHIN;
            let mut restoring = false;
            loop {
                let counted = scratch.counters();
                if let Counters::Systemd {
                    active_state,
                    main_pid,
                    ..
                } = &counted
                {
                    restoring |=
                        active_state == "activating" || (*main_pid != 0 && *main_pid != killed);
                    if restoring && active_state == "inactive" {
                        break;
                    }
                    assert_ne!(
                        active_state, "failed",
                        "the daemon restored on the mark did not end well"
                    );
                }
                assert!(
                    Instant::now() < until,
                    "`{}` was waited for {WITHIN:?} to be restored and to end well, and is \
                     {counted:?}{}",
                    scratch.name,
                    if restoring {
                        ", having been seen restoring"
                    } else {
                        ", never seen restoring"
                    }
                );
            }
        }
        Manager::Launchd => {
            let ended = scratch.counted_once(
                |now| now.ran_since(before) && !now.running() && now.ended_well() == Some(true),
                "restored and ended well",
            );
            if let (Counters::Launchd { runs, .. }, Counters::Launchd { runs: was, .. }) =
                (&ended, before)
            {
                assert_eq!(*runs, was + 1, "restored more than once: {ended:?}");
            }
        }
        Manager::TaskScheduler => {
            scratch.counted_once(
                |now| now.ran_since(before) && !now.running() && now.ended_well() == Some(true),
                "restored by a repetition and ended well",
            );
        }
    }
}

/// The stop mark silences every restorer, and a crash is restored.
///
/// Killed, the daemon is restored as a new instance; installed again over
/// it, it is started again through its restorer; stopped, a restorer's start
/// and an install leave it down; with the mark written and then killed, it is
/// restored once, ends well and stays down.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "acceptance 8 is one daemon's life under its restorer, in the order it is lived"
)]
fn the_stop_mark_silences_every_restorer() {
    let (mut scratch, install) = Scratch::new();
    let installed = scratch.install(&install);
    let registered = installed
        .registered
        .expect("the install registered its restorer");
    assert_eq!(registered.manager, MANAGER);
    assert_eq!(registered.name, scratch.name);
    assert_eq!(registered.kept_in, scratch.kept_in);
    // **Started once registered**: a fresh profile carries no mark. The
    // registration may have started it already, so which start it says is
    // read further down, after a stop, where only the manager's can.
    let started = registered.started.expect("the install started its daemon");
    assert!(started.contains("is running"), "{started}");
    assert!(
        scratch
            .registrar
            .is_registered(&scratch.name)
            .expect("asked"),
        "the restorer is not registered"
    );
    if let Some(units) = scratch.registrar.volatile_units() {
        assert_eq!(
            registered.kept_in, units,
            "a test's unit outside the runtime directory"
        );
        let wanted = units.join(WANTED).join(&scratch.name);
        assert!(
            wanted.symlink_metadata().is_ok(),
            "the unit is not enabled to start at login: no `{}`",
            wanted.display()
        );
    } else {
        assert_eq!(registered.kept_in, scratch.found().layout.state);
    }

    // **Killed, it is restored as a new instance.**
    let first = scratch.recorded_once(
        |record| Verified::open(&record.identity()).is_ok(),
        "held to a process that runs",
    );
    let before = scratch.counted_once(Counters::running, "running");
    if let Some(pid) = before.pid() {
        assert_eq!(
            pid, first.pid,
            "its manager runs another process than its record"
        );
    }
    killed(&first);
    let second = scratch.recorded_once(
        |record| record.instance != first.instance && Verified::open(&record.identity()).is_ok(),
        "a new instance, running",
    );
    let restored = scratch.counted_once(
        |now| now.ran_since(&before) && now.running(),
        "restored by its manager",
    );
    if let Some(pid) = restored.pid() {
        assert_eq!(pid, second.pid, "its manager restored another process");
    }

    // **Installed again over it: stopped, its restorer let go and registered
    // again, and started again through it.**
    let over = scratch.install(&install);
    let said = over
        .stopped
        .expect("the daemon the earlier install ran was not stopped first");
    assert!(
        said.contains(&format!(
            "process {}, stopped when it was asked to",
            second.pid
        )),
        "{said}"
    );
    let again = over.registered.expect("the restorer registered again");
    assert_eq!(again.name, scratch.name);
    let started = again.started.expect("the install started its daemon again");
    assert!(started.contains("is running"), "{started}");
    let (third, _) = scratch.running_after(&second);

    // **Stopped, a restorer's start and an install leave it down.**
    let stopped = run(scratch.found(), "stop");
    assert_eq!(stopped.exit, Exit::Success, "{stopped:?}");
    assert!(
        scratch.found().layout.stopped.is_file(),
        "the stop wrote no mark"
    );
    let after_the_stop = scratch.counted_once(|now| !now.running(), "down after the stop");
    scratch
        .registrar
        .start(&scratch.name)
        .unwrap_or_else(|error| panic!("`{}` was not started: {error}", scratch.name));
    if MANAGER != Manager::Systemd {
        scratch.counted_once(
            |now| {
                now.ran_since(&after_the_stop) && !now.running() && now.ended_well() == Some(true)
            },
            "started as a restorer starts it, and ended well on the mark",
        );
    }
    // **systemd's start returns once the program runs**, which here ends
    // at once on the mark: it is down once it has.
    scratch.counted_once(|now| !now.running(), "down again, on the mark");
    scratch.stays_down();
    assert!(
        scratch.record().is_none(),
        "a restorer's start wrote a record: {:?}",
        scratch.record()
    );
    let kept = scratch.install(&install);
    let registered = kept.registered.expect("the restorer registered again");
    assert!(
        registered.started.is_none(),
        "an install started a daemon a person had stopped: {registered:?}"
    );
    assert!(
        scratch.found().layout.stopped.is_file(),
        "an install took a person's stop away"
    );
    scratch.ended_on_the_mark_once_registered();
    scratch.stays_down();

    // **Started again, the mark written, then killed: restored once, it ends
    // well on the mark and stays down.**
    let again = run(scratch.found(), "start");
    assert_eq!(again.exit, Exit::Success, "{again:?}");
    assert!(
        again.stdout.contains(&format!("{MANAGER} started it")),
        "a start with the restorer registered went around it: {again:?}"
    );
    assert!(
        !scratch.found().layout.stopped.exists(),
        "a start left the mark"
    );
    let (fourth, before) = scratch.running_after(&third);
    nc_cli::control::mark(&scratch.found().layout, &scratch.found().user).expect("the mark");
    killed(&fourth);
    restored_once_and_ended_well(&scratch, &before, fourth.pid);
    scratch.stays_down();
    let left = scratch
        .record()
        .expect("the killed daemon's record is left");
    assert_eq!(
        left.instance, fourth.instance,
        "a start on the mark wrote a record of its own"
    );

    // **Gone from the manager once uninstalled.**
    let found = scratch.found.take().expect("installed");
    let uninstalled = nc_cli::install::uninstall(&found.install)
        .expect("uninstalled")
        .expect("there was an install");
    assert_eq!(
        uninstalled.unregistered.as_deref(),
        Some(scratch.name.as_str())
    );
    assert!(
        !scratch
            .registrar
            .is_registered(&scratch.name)
            .expect("asked"),
        "`{}` was uninstalled and its manager still holds it",
        scratch.name
    );
    if let Some(units) = scratch.registrar.volatile_units() {
        assert!(
            units
                .join(WANTED)
                .join(&scratch.name)
                .symlink_metadata()
                .is_err(),
            "the unit's link to start at login outlived it"
        );
    }
}
