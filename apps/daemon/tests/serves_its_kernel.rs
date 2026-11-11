//! A real daemon process serves its kernel at its endpoint (`NOCTUA.md
//! §11.1`): the registry of its kernel and the plugins installed in its
//! profile, its status, every dispatch decided by the kernel and journaled,
//! the pump and the sweep on their schedule, one daemon to a profile, none of
//! its spawner's handles held, a session of its own, and a stop that leaves
//! nothing of the run.
//!
//! **The binary this build made, copied where an installer puts it**, beside
//! a manifest under a scratch root and a namespace of its own
//! (`nc_daemon::testing::Installed`), and reached as the CLI reaches it:
//! through the endpoint, proven by the token the daemon wrote. It spawns a
//! process, so no mutation command runs it.

use std::collections::BTreeMap;
use std::path::Path;

use core::time::Duration;

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_daemon::testing::{Installed, PATIENCE};
use nc_ipc::{Answer, Request, State};

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// The reference plugin's manifest, as the plugin ships it.
const REFERENCE: &str = include_str!("../../../plugins/reference/plugin.toml");

/// `request`, answered.
async fn asked(client: &mut nc_ipc::Client, request: Request) -> Answer {
    tokio::time::timeout(PATIENCE, client.ask(&request))
        .await
        .expect("answered in time")
        .expect("answered")
}

/// The daemon's status, as one object's fields.
async fn status(client: &mut nc_ipc::Client) -> BTreeMap<String, Value> {
    match asked(client, Request::Status).await {
        Answer::Status(status) => status.into_object().expect("an object"),
        other => panic!("the status was answered with {other:?}"),
    }
}

/// The status, once `holds` holds of it.
async fn once(
    client: &mut nc_ipc::Client,
    holds: impl Fn(&BTreeMap<String, Value>) -> bool,
) -> BTreeMap<String, Value> {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let status = status(client).await;
        if holds(&status) {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the status did not come to hold within {PATIENCE:?}: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Whether the daemon runs, by its status.
fn runs(status: &BTreeMap<String, Value>) -> bool {
    status.get("state") == Some(&Value::text(State::Running.word()))
}

/// The number `key` holds in the object `value`.
fn number(value: &Value, key: &str) -> Option<i64> {
    match value.clone().into_object()?.get(key)? {
        Value::Integer(number) => Some(*number),
        _ => None,
    }
}

/// The text `key` holds in the object `value`.
fn text(value: &Value, key: &str) -> Option<String> {
    value.clone().into_object()?.get(key)?.clone().into_text()
}

/// The lines of the event `name` the daemon wrote to its standard error.
fn logged(installed: &Installed, name: &str) -> Vec<String> {
    installed
        .standard_error()
        .lines()
        .filter(|line| line.contains(&format!("\"_msg\":\"{name}\"")))
        .map(str::to_owned)
        .collect()
}

/// Stops the daemon through its endpoint, and waits for it to end well.
async fn stopped(client: &mut nc_ipc::Client, daemon: &mut nc_daemon::testing::Running) {
    assert_eq!(asked(client, Request::Stop).await, Answer::Stopping);
    assert_eq!(daemon.ended().await, Some(0), "the daemon did not end well");
}

/// The registry a daemon answers is its kernel's: every command the catalog
/// declares and every command of each plugin installed in its profile, each
/// with every field the registry holds; a plugin that is refused is shown in
/// the status with its code and logged, and the daemon runs without it.
#[tokio::test(flavor = "multi_thread")]
async fn the_registry_is_the_kernels_and_the_plugins_installed() {
    let installed = Installed::new(Path::new(BINARY));
    installed.install_plugin(
        "reference",
        REFERENCE,
        &nc_kernel::component_for_tests("reference"),
    );
    installed.install_plugin("broken", "id = 7", b"");
    let mut daemon = installed.spawn();
    let (mut client, welcome) = daemon.connected().await;
    assert_eq!(welcome.pid, daemon.pid());
    assert_eq!(welcome.channel, nc_channel::Channel::Staging);
    let status = once(&mut client, runs).await;
    let plugins = status
        .get("plugins")
        .cloned()
        .and_then(Value::into_array)
        .expect("the status lists the plugins");
    assert_eq!(plugins.len(), 2, "{plugins:?}");
    assert_eq!(text(&plugins[0], "name").as_deref(), Some("broken"));
    assert_eq!(
        text(&plugins[0], "code").as_deref(),
        Some(ErrorCode::PluginManifestInvalid.code())
    );
    assert_eq!(text(&plugins[1], "name").as_deref(), Some("reference"));
    assert_eq!(
        plugins[1]
            .clone()
            .into_object()
            .and_then(|fields| fields.get("loaded").cloned()),
        Some(Value::Bool(true))
    );
    let Answer::Registry(commands) = asked(&mut client, Request::Registry).await else {
        panic!("the registry was not answered");
    };
    let mut kernels = Vec::new();
    let mut plugins = Vec::new();
    for command in &commands {
        let fields = command.clone().into_object().expect("an object");
        assert_eq!(
            fields.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "action",
                "args_schema",
                "mutating",
                "name",
                "owner",
                "resource_selector",
                "reversibility",
                "title"
            ],
            "{command:?}"
        );
        let name = text(command, "name").expect("a name");
        match text(command, "owner").as_deref() {
            Some("kernel") => kernels.push(name),
            Some("plugin") => plugins.push(name),
            other => panic!("{name} is owned by {other:?}"),
        }
    }
    let catalog: Vec<String> = nc_catalog::COMMANDS
        .iter()
        .map(|command| command.name.to_owned())
        .collect();
    assert_eq!(
        kernels, catalog,
        "the kernel's commands are not the catalog's"
    );
    assert!(
        !plugins.is_empty() && plugins.iter().all(|name| name.starts_with("reference.")),
        "{plugins:?}"
    );
    stopped(&mut client, &mut daemon).await;
    let refused = logged(&installed, "daemon.plugin_refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(
        refused[0].contains(ErrorCode::PluginManifestInvalid.code()),
        "{refused:?}"
    );
    let started = logged(&installed, "daemon.started");
    assert_eq!(started.len(), 1, "{started:?}");
    assert!(
        started[0].contains("\"loaded\":\"1\"") && started[0].contains("\"refused\":\"1\""),
        "{started:?}"
    );
}

/// Every dispatch is the kernel's to decide, as the person the profile's
/// world names, and each one is in the journal afterwards with the outcome it
/// was answered with.
#[tokio::test(flavor = "multi_thread")]
async fn every_dispatch_is_the_kernels_decision_and_is_journaled() {
    use nc_journal::Store as _;

    let installed = Installed::new(Path::new(BINARY));
    let mut daemon = installed.spawn();
    let (mut client, _) = daemon.connected().await;
    once(&mut client, runs).await;
    let dispatched = [
        ("journal.verify", "{\"from\":0}", "Read"),
        ("kernel.undo", "{}", "Write"),
        ("no.such.command", "{}", ""),
    ];
    let mut answers = Vec::new();
    for (command, args, _) in dispatched {
        answers.push(
            asked(
                &mut client,
                Request::Dispatch {
                    command: command.to_owned(),
                    args: nc_canon::parse(args).expect("JSON"),
                },
            )
            .await,
        );
    }
    stopped(&mut client, &mut daemon).await;
    let store = nc_local::LocalStore::open(&installed.layout.data)
        .await
        .expect("the stopped daemon's database opens");
    let read = store.journal().from(0).await.expect("the journal reads");
    store.close().await;
    assert_eq!(read.stopped_at, None);
    // Each dispatch's own entry, in order: the brackets of a unit an
    // authorized command opened are the unit's.
    let decided: Vec<&nc_journal::Entry> = read
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.record.result,
                nc_journal::Outcome::Allowed
                    | nc_journal::Outcome::Denied
                    | nc_journal::Outcome::Failed
            )
        })
        .collect();
    assert_eq!(
        decided.len(),
        dispatched.len(),
        "{:?}",
        read.entries
            .iter()
            .map(|entry| &entry.record.action)
            .collect::<Vec<_>>()
    );
    for (((command, _, action), answer), entry) in dispatched.iter().zip(&answers).zip(decided) {
        let record = &entry.record;
        assert_eq!(record.principal.as_str(), "ana", "{command}");
        assert_eq!(record.realm.as_str(), "home", "{command}");
        if !action.is_empty() {
            assert_eq!(record.action, *action, "{command}");
        }
        match answer {
            Answer::Result(_) => {
                assert_eq!(record.result, nc_journal::Outcome::Allowed, "{command}");
                // Decided with the grant and the epoch the profile's world
                // names.
                assert_eq!(
                    record.grant_used.as_ref().map(nc_ids::OpaqueId::as_str),
                    Some(nc_daemon::testing::GRANT),
                    "{command}"
                );
                assert_eq!(
                    record.policy_epoch,
                    u64::try_from(nc_daemon::testing::EPOCH).ok(),
                    "{command}"
                );
            }
            Answer::Refused(refusal) => {
                assert_ne!(record.result, nc_journal::Outcome::Allowed, "{command}");
                assert!(
                    refusal.code.code().starts_with("NC-POL-")
                        || refusal.code.code().starts_with("NC-KERN-"),
                    "{command}: {refusal:?}"
                );
            }
            other => panic!("{command} was answered with {other:?}"),
        }
    }
}

/// The pump runs every `pump_every_ms`, the first as soon as the daemon runs,
/// and the sweep once at once and then not again before `sweep_every_ms`.
#[tokio::test(flavor = "multi_thread")]
async fn the_pump_and_the_sweep_run_on_their_schedule() {
    let installed = Installed::new(Path::new(BINARY));
    let mut daemon = installed.spawn();
    let (mut client, _) = daemon.connected().await;
    let schedule = |status: &BTreeMap<String, Value>| {
        status
            .get("schedule")
            .cloned()
            .expect("the status holds the schedule")
    };
    let pumps = |status: &BTreeMap<String, Value>| number(&schedule(status), "pumps");
    let first = once(&mut client, |status| {
        runs(status) && pumps(status) >= Some(1)
    })
    .await;
    let (counted, at) = (
        pumps(&first).expect("counted"),
        number(&schedule(&first), "last_pump").expect("timed"),
    );
    let later = once(&mut client, |status| pumps(status) >= Some(counted + 2)).await;
    let later_at = number(&schedule(&later), "last_pump").expect("timed");
    let every = i64::try_from(nc_catalog::DAEMON.pump_every_ms).expect("a bound");
    let apart = later_at - at;
    let pumped = pumps(&later).expect("counted") - counted;
    assert!(
        apart >= every * (pumped - 1),
        "{pumped} pumps came {apart} ms apart, and the schedule is one every {every} ms"
    );
    assert_eq!(number(&schedule(&later), "sweeps"), Some(1));
    assert!(number(&schedule(&later), "last_sweep").is_some());
    assert_eq!(
        number(&schedule(&later), "sweep_every_ms"),
        i64::try_from(nc_catalog::DAEMON.sweep_every_ms).ok()
    );
    stopped(&mut client, &mut daemon).await;
}

/// A stop is answered, the daemon ends well, and nothing of its run is left:
/// its PID record, its token and its socket are gone, its lock is let go, and
/// the profile starts again and answers.
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_ends_the_daemon_well_and_leaves_nothing_of_the_run() {
    let installed = Installed::new(Path::new(BINARY));
    let mut daemon = installed.spawn();
    let (mut client, welcome) = daemon.connected().await;
    once(&mut client, runs).await;
    let layout = &installed.layout;
    assert!(layout.pid_file.is_file() && layout.token.is_file());
    stopped(&mut client, &mut daemon).await;
    assert!(!layout.pid_file.exists(), "the PID record was left");
    assert!(!layout.token.exists(), "the token was left");
    assert!(
        !(nc_platform::testing::ENDPOINT_ON_DISK && layout.socket.exists()),
        "the socket was left"
    );
    let lock = layout
        .instance_lock
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    drop(
        nc_platform::try_hold(&layout.state, &lock, &installed.user)
            .expect("asked")
            .expect("the instance lock was let go"),
    );
    let stopped_line = logged(&installed, "daemon.stopped");
    assert_eq!(stopped_line.len(), 1, "{stopped_line:?}");
    assert!(
        stopped_line[0].contains("\"abandoned\":\"0\""),
        "{stopped_line:?}"
    );
    let mut again = installed.spawn();
    let (mut client, second) = again.connected().await;
    assert_ne!(
        second.instance, welcome.instance,
        "a new start drew the old instance"
    );
    once(&mut client, runs).await;
    stopped(&mut client, &mut again).await;
}

/// A second daemon of the channel refuses to start while the first holds its
/// profile, and the first goes on answering.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_daemon_of_the_channel_refuses_to_start() {
    let installed = Installed::new(Path::new(BINARY));
    let mut first = installed.spawn();
    let (mut client, welcome) = first.connected().await;
    once(&mut client, runs).await;
    let mut second = installed.spawn();
    assert_eq!(
        second.ended().await,
        Some(1),
        "{}",
        installed.standard_error()
    );
    assert!(
        installed
            .standard_error()
            .contains(&format!("[{}]", ErrorCode::DaemonInstanceHeld.code())),
        "{}",
        installed.standard_error()
    );
    let status = status(&mut client).await;
    assert_eq!(
        status.get("pid"),
        Some(&Value::Integer(i64::from(welcome.pid))),
        "the first daemon no longer answers"
    );
    stopped(&mut client, &mut first).await;
}

/// A daemon its spawner started as no group's leader leads a session of its
/// own, so no terminal it was started from reaches it.
///
/// **Read off the system once the daemon answers**, which it does only after
/// it left the session it was started in: this suite's, which the daemon is
/// started in as every suite starts one, without a group of its own.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_leads_a_session_of_its_own() {
    if !nc_platform::system::POSIX_SESSIONS {
        println!(
            "skipped, not applicable on {}: no process here is in a session of the POSIX \
             kind, so there is none for the daemon to leave, and the Linux and macOS jobs \
             run it",
            std::env::consts::OS
        );
        return;
    }
    let installed = Installed::new(Path::new(BINARY));
    let mut daemon = installed.spawn();
    let (mut client, _) = daemon.connected().await;
    let session = nc_platform::testing::session_of(daemon.pid())
        .expect("the daemon's session is read")
        .expect("a system with sessions read none for the daemon");
    let ours = nc_platform::testing::session_of(std::process::id())
        .expect("this session is read")
        .expect("a system with sessions read none for this process");
    stopped(&mut client, &mut daemon).await;
    assert_eq!(
        session,
        daemon.pid(),
        "the daemon does not lead a session of its own"
    );
    assert_ne!(
        session, ours,
        "the daemon is still in the session of the suite that started it"
    );
}

/// The variables that turn
/// [`the_daemon_holds_none_of_its_spawners_standard_handles`] into its own
/// child: the scratch install's binary to start there, and its standard error.
const SPAWNING: &str = "NC_DAEMON_SPAWNING_CHILD";
const SPAWNING_ERROR: &str = "NC_DAEMON_SPAWNING_CHILD_ERROR";

/// A daemon holds none of its spawner's standard handles: a spawner whose
/// output is read to its end ends it while the daemon it started still runs.
///
/// **A child of this test binary is the spawner**, its output a pipe this test
/// reads: it starts the daemon as every suite does, says the daemon's process,
/// and exits. Were the daemon holding the child's output, the pipe would not
/// end while the daemon ran — and a test runner reading a suite's output to
/// its end would wait on a daemon that held it for as long as that daemon
/// lived.
#[test]
#[expect(
    clippy::disallowed_methods,
    clippy::zombie_processes,
    reason = "the spawner has to be another process, whose output this test reads to its end, \
              and it leaves the daemon it started running, never waited for, as a spawner \
              does; the parent stops the daemon through its endpoint"
)]
fn the_daemon_holds_none_of_its_spawners_standard_handles() {
    use std::io::{Read as _, Write as _};

    if let (Some(executable), Some(error)) =
        (std::env::var_os(SPAWNING), std::env::var_os(SPAWNING_ERROR))
    {
        let daemon = nc_daemon::testing::start(Path::new(&executable), Path::new(&error));
        let mut out = std::io::stdout().lock();
        writeln!(out, "spawned {}", daemon.id()).expect("said");
        out.flush().expect("said");
        return;
    }
    let installed = Installed::new(Path::new(BINARY));
    let child = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--exact",
            "the_daemon_holds_none_of_its_spawners_standard_handles",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(SPAWNING, &installed.executable)
        .env(SPAWNING_ERROR, &installed.layout.daemon_err)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the spawner starts");
    let mut spawner = Spawner(child);
    let mut output = spawner.0.stdout.take().expect("piped");
    let (read, reading) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut said = String::new();
        let _ = output.read_to_string(&mut said);
        let _ = read.send(said);
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let said = reading.recv_timeout(PATIENCE);
    // Stopped through its endpoint whatever the verdict, found by its record
    // where the spawner's output never ended, so a daemon this test started
    // is never left running.
    let recorded = nc_daemon::pidfile::read(&installed.layout.pid_file, &installed.user)
        .ok()
        .map(|record| record.pid);
    let pid = said.as_ref().ok().and_then(|said| {
        // The harness writes the test's name on the line the child's words
        // continue.
        said.split("spawned ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|pid| pid.parse::<u32>().ok())
    });
    let welcome = pid.or(recorded).map(|pid| {
        runtime.block_on(async {
            let (mut client, welcome) = installed.reach(pid).await;
            assert_eq!(asked(&mut client, Request::Stop).await, Answer::Stopping);
            welcome
        })
    });
    let said = said.expect("the spawner's output ended while its daemon ran");
    let pid = pid.unwrap_or_else(|| panic!("the spawner said no daemon: {said:?}"));
    assert_eq!(welcome.map(|welcome| welcome.pid), Some(pid));
    assert!(spawner.0.wait().expect("the spawner ended").success());
    let deadline = std::time::Instant::now() + PATIENCE;
    while installed.layout.pid_file.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the daemon did not stop within {PATIENCE:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The spawner, killed and waited for however the test leaves.
struct Spawner(std::process::Child);

impl Drop for Spawner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
