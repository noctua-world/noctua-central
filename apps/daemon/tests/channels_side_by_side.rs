//! Production, staging and two development checkouts run side by side, and
//! stopping one leaves the other three answering (`NOCTUA.md §17.1`;
//! `NOCTUA-PLAN.md`, Stage 13, acceptance 3).
//!
//! **Installed as an installer installs one, under a scratch root and a
//! namespace of its own**: the four are made by `nc_cli::install` — the
//! installer `cargo xtask dev --install` runs — from the daemon this build
//! made, each in its user's base directories read from an environment that
//! names the scratch root alone, the two development checkouts through
//! `nc_cli::install::development`, the very reader `dev --install` reads its
//! environment with. Every name the system sees carries the namespace, which
//! carries this process, so no install here meets a developer's.
//!
//! **Driven through the CLI in-process** (`nc_cli::run`): each is started
//! with `start`, which spawns its daemon detached, held running by `status`,
//! and stopped with `stop`. Their profiles and endpoints are held apart on
//! disk; then each in turn is stopped while the other three are asked again,
//! and started again; then one is restarted. It spawns processes, so no
//! mutation command runs it; a guard stops every daemon through the CLI
//! however the test leaves.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use nc_cli::exit::Exit;

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// A scratch root of its own, removed however the test leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = nc_platform::testing::make_private(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// A private directory under the temporary one, named for this process.
    ///
    /// **Short**, because macOS holds a socket's path to 103 bytes, and its
    /// profiles sit in the Logs folder of the Library in the home this names:
    /// the process's id is written in base 36.
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nc-z{}", short(std::process::id())));
        let _ = nc_platform::testing::make_private(&root);
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch root");
        Self(root)
    }
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

/// The installs, each stopped through the CLI however the test leaves —
/// declared before the scratch they live in, so they go first.
struct Running(Vec<nc_channel::found::Found>);

impl Drop for Running {
    fn drop(&mut self) {
        for found in &self.0 {
            let _ = run(found, "stop");
        }
    }
}

/// The CLI run with `verb` for `found`'s install.
fn run(found: &nc_channel::found::Found, verb: &str) -> nc_cli::Outcome {
    nc_cli::run(&[OsString::from(verb)], found, &nc_cli::Env::of(|_| None))
}

/// An environment that names `root` as the home and the folders every system
/// keeps a user's profiles under, and `namespace`; nothing else of this
/// process's.
fn environment(root: &Path, namespace: &str) -> impl Fn(&str) -> Option<OsString> + use<> {
    let pairs = [
        ("HOME", root.as_os_str().to_owned()),
        ("LOCALAPPDATA", root.join("l").into_os_string()),
        ("APPDATA", root.join("r").into_os_string()),
        (
            nc_cli::install::NAMESPACE,
            OsString::from(namespace.to_owned()),
        ),
    ];
    move |name| {
        pairs
            .iter()
            .find(|(variable, _)| *variable == name)
            .map(|(_, value)| value.clone())
    }
}

/// Every path one install keeps, each of which no other may hold or lie
/// under.
fn paths_of(layout: &nc_channel::Layout) -> [&Path; 7] {
    [
        &layout.data,
        &layout.config,
        &layout.cache,
        &layout.state,
        &layout.install,
        &layout.runtime,
        &layout.socket,
    ]
}

/// The four installs — production, staging and two development checkouts —
/// made by the installer under `scratch`, relocated under `namespace`, each in
/// the base directories `environment` names.
fn installed(
    scratch: &Path,
    namespace: &str,
    environment: &dyn Fn(&str) -> Option<OsString>,
) -> Running {
    let world = nc_cli::install::development_world(include_str!("../../../policy/world.json"))
        .expect("the test world");
    assert_eq!(
        world,
        nc_daemon::testing::a_world(),
        "the development install's world is not the one the suites decide with"
    );
    let cli = scratch.join("cli");
    std::io::Write::write_all(
        &mut nc_platform::create_private(&cli).expect("a stand-in CLI"),
        b"the CLI is driven in-process",
    )
    .expect("written");
    let found_bases = nc_platform::base_directories(environment).expect("the base directories");
    let bases = || {
        nc_channel::Bases::new(
            found_bases.data.clone(),
            found_bases.config.clone(),
            found_bases.cache.clone(),
            found_bases.state.clone(),
        )
        .expect("plain bases")
    };
    let relocated = || Some(nc_channel::Namespace::parse(namespace).expect("a namespace"));
    let mut installs = vec![
        nc_channel::Install {
            channel: nc_channel::Channel::Production,
            bases: bases(),
            namespace: relocated(),
            log_endpoint: None,
        },
        nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: bases(),
            namespace: relocated(),
            log_endpoint: None,
        },
    ];
    for checkout in ["a", "b"] {
        let root = scratch.join(checkout);
        nc_platform::create_private_directory(&root).expect("a checkout");
        let install = nc_cli::install::development(&root, environment, None)
            .expect("the environment names a development install");
        assert_eq!(
            install.bases,
            bases(),
            "the environment's bases were not read"
        );
        assert_eq!(install.namespace, relocated());
        installs.push(install);
    }
    let mut running = Running(Vec::new());
    for install in installs {
        let plan = nc_cli::install::Plan {
            install,
            cli: &cli,
            daemon: Path::new(BINARY),
            world: &world,
            plugins: Vec::new(),
        };
        let installed = nc_cli::install::install(&plan).expect("installed");
        running.0.push(installed.found);
    }
    running
}

/// Whether no path one install keeps is another's or lies under another's,
/// and no two share a pipe.
fn held_apart(running: &Running) {
    for (at, one) in running.0.iter().enumerate() {
        for (other, another) in running.0.iter().enumerate().skip(at + 1) {
            for mine in paths_of(&one.layout) {
                for theirs in paths_of(&another.layout) {
                    assert!(
                        !mine.starts_with(theirs) && !theirs.starts_with(mine),
                        "installs {at} and {other} meet: `{}` and `{}`",
                        mine.display(),
                        theirs.display()
                    );
                }
            }
            assert_ne!(one.layout.pipe, another.layout.pipe);
        }
    }
}

/// Four channels run at once, their profiles and endpoints apart, each
/// stopped while the other three answer, and one restarted.
#[test]
fn four_installs_run_side_by_side_and_one_stopped_leaves_the_others() {
    let scratch = Scratch::new();
    // A namespace holds a digit, which an id in base 36 may not.
    let mut namespace = format!("t{}", short(std::process::id()));
    if !namespace.bytes().any(|byte| byte.is_ascii_digit()) {
        namespace.push('0');
    }
    let running = installed(&scratch.0, &namespace, &environment(&scratch.0, &namespace));
    held_apart(&running);
    for found in &running.0 {
        let started = run(found, "start");
        assert_eq!(started.exit, Exit::Success, "{started:?}");
        assert!(started.stdout.contains("is running"), "{started:?}");
    }
    let mut pids: Vec<u32> = running
        .0
        .iter()
        .map(|found| {
            let status = run(found, "status");
            assert_eq!(status.exit, Exit::Success, "{status:?}");
            nc_channel::record::read(&found.layout.pid_file, &found.user)
                .expect("a running daemon's record")
                .pid
        })
        .collect();
    for (one, pid) in pids.iter().enumerate() {
        assert_eq!(
            pids.iter().filter(|other| *other == pid).count(),
            1,
            "install {one} shares its daemon"
        );
    }
    for stopped in 0..running.0.len() {
        let outcome = run(&running.0[stopped], "stop");
        assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
        let status = run(&running.0[stopped], "status");
        assert_eq!(
            status.exit,
            Exit::Unreachable,
            "a stopped daemon answers: {status:?}"
        );
        for (other, found) in running.0.iter().enumerate() {
            if other == stopped {
                continue;
            }
            let status = run(found, "status");
            assert_eq!(
                status.exit,
                Exit::Success,
                "stopping {stopped} stopped {other}: {status:?}"
            );
            assert!(
                status.stdout.contains(&format!("\"pid\":{}", pids[other])),
                "stopping {stopped} restarted {other}: {status:?}"
            );
        }
        let started = run(&running.0[stopped], "start");
        assert_eq!(started.exit, Exit::Success, "{started:?}");
        let found = &running.0[stopped];
        pids[stopped] = nc_channel::record::read(&found.layout.pid_file, &found.user)
            .expect("a daemon started again has a record")
            .pid;
    }
    // A restart ends the daemon its record names and starts another, the mark
    // written and removed between them, and leaves the other three running.
    let restarted = &running.0[0];
    let record = || {
        nc_channel::record::read(&restarted.layout.pid_file, &restarted.user)
            .expect("a running daemon's record")
    };
    let before = record();
    let outcome = run(restarted, "restart");
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    assert!(outcome.stdout.contains("is running"), "{outcome:?}");
    let after = record();
    assert_ne!(
        (after.pid, &after.started, after.instance),
        (before.pid, &before.started, before.instance),
        "the restart left the daemon it was to stop"
    );
    let ended = nc_platform::process::Verified::open(&before.identity());
    assert!(
        matches!(ended, Err(nc_platform::process::Unverified::Gone(_))),
        "the daemon a restart stopped still runs: {ended:?}"
    );
    assert!(
        !restarted.layout.stopped.exists(),
        "a restart left the `stopped` mark"
    );
    let status = run(restarted, "status");
    assert!(
        status.stdout.contains(&format!("\"pid\":{}", after.pid)),
        "the restarted daemon is not the one its record names: {status:?}"
    );
    for (other, found) in running.0.iter().enumerate().skip(1) {
        let status = run(found, "status");
        assert!(
            status.stdout.contains(&format!("\"pid\":{}", pids[other])),
            "restarting the first restarted {other}: {status:?}"
        );
    }
    drop(running);
    drop(scratch);
}
