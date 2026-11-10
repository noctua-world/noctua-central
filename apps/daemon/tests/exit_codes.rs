//! Each way a run of the CLI ends against a real daemon, with its exit code
//! from the one table (`nc_cli::exit`, `NOCTUA.md §20.2`): one test per
//! outcome, and among them a command that cannot be undone, refused with the
//! code that names the Noctua window and ended in an exit code of its own
//! (`NOCTUA-PLAN.md`, Stage 13, acceptance 7).
//!
//! **The CLI in-process** (`nc_cli::run`) against the daemon this build made,
//! copied where an installer puts it beside a manifest under a scratch root and
//! a namespace of its own (`nc_daemon::testing::Installed`), with the
//! reference plugin installed and granted what its commands reach for but the
//! network; and 11, process control's, against the same install with no
//! daemon running, and with one that ends before it answers. One outcome of
//! the table is not run here, because no daemon brings it about: 1, the CLI
//! unable to write what it answered, which `noctua`'s own unit tests hold. It
//! spawns a process, so no mutation command runs it.

use std::ffi::OsString;
use std::path::Path;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_cli::exit::Exit;
use nc_daemon::testing::{Installed, PATIENCE};
use nc_ipc::{Answer, Request, State};
use nc_journal::Outcome;

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// The reference plugin's manifest as this suite installs it: what its
/// commands reach for but the network.
const GRANTED: &str = "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = \
                       [\"context\", \"store\", \"blobs.read\", \"blobs.write\", \
                       \"commands.dispatch\", \"commands.begin\", \"events.publish\", \"ui\", \
                       \"timers\"]\n";

/// A daemon of a scratch install with the reference plugin installed, and
/// what the CLI reads its install by.
struct Serving {
    // Fields drop in the order they are written, so a test that panics
    // kills the daemon and waits for it before its install is removed.
    daemon: nc_daemon::testing::Running,
    client: nc_ipc::Client,
    runtime: tokio::runtime::Runtime,
    found: nc_channel::found::Found,
    installed: Installed,
}

impl Serving {
    /// The daemon started, and running.
    fn new() -> Self {
        let installed = Installed::new(Path::new(BINARY));
        installed.install_plugin(
            "reference",
            GRANTED,
            &nc_kernel::component_for_tests("reference"),
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let mut daemon = installed.spawn();
        let client = runtime.block_on(async {
            let (mut client, _) = daemon.connected().await;
            running(&mut client).await;
            client
        });
        let found = nc_channel::found::beside(&installed.executable)
            .expect("the CLI reads the install the daemon runs from");
        Self {
            daemon,
            client,
            runtime,
            found,
            installed,
        }
    }

    /// The CLI run with `line`, in English.
    fn run(&self, line: &[&str]) -> nc_cli::Outcome {
        self.run_in(line, &nc_cli::Env::of(|_| None))
    }

    /// The CLI run with `line`, in the language `env` names.
    fn run_in(&self, line: &[&str], env: &nc_cli::Env) -> nc_cli::Outcome {
        let argv: Vec<OsString> = line.iter().map(OsString::from).collect();
        nc_cli::run(&argv, &self.found, env)
    }

    /// Every entry of the daemon's journal, read from its database beside it.
    fn journal(&self) -> Vec<nc_journal::Record> {
        self.runtime.block_on(async {
            let store = nc_local::LocalStore::open(&self.installed.layout.data)
                .await
                .expect("the daemon's database opens beside it");
            let read = nc_journal::Store::from(&store.journal(), 0)
                .await
                .expect("the journal reads");
            store.close().await;
            read.entries.into_iter().map(|entry| entry.record).collect()
        })
    }

    /// Stops the daemon through its endpoint, and waits for it to end well.
    fn stop(&mut self) {
        self.runtime.block_on(async {
            assert_eq!(
                asked(&mut self.client, Request::Stop).await,
                Answer::Stopping
            );
            assert_eq!(
                self.daemon.ended().await,
                Some(0),
                "{}",
                self.installed.standard_error()
            );
        });
    }
}

/// `request`, answered.
async fn asked(client: &mut nc_ipc::Client, request: Request) -> Answer {
    tokio::time::timeout(PATIENCE, client.ask(&request))
        .await
        .expect("answered in time")
        .expect("answered")
}

/// Waits until the daemon's status says it runs.
async fn running(client: &mut nc_ipc::Client) {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        if let Answer::Status(status) = asked(client, Request::Status).await
            && status
                .to_canonical()
                .contains(&format!("\"state\":\"{}\"", State::Running.word()))
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the daemon did not run within {PATIENCE:?}"
        );
        tokio::time::sleep(core::time::Duration::from_millis(20)).await;
    }
}

/// Whether `outcome` ended in `exit` with `code` first on its standard error.
fn ended_in(outcome: &nc_cli::Outcome, exit: Exit, code: ErrorCode) {
    assert_eq!(outcome.exit, exit, "{outcome:?}");
    assert_eq!(outcome.code(), exit.code());
    assert!(
        outcome
            .stderr
            .starts_with(&format!("error[{}]", code.code())),
        "{}",
        outcome.stderr
    );
}

/// The decision entries of `command` in `journal`, as the decision point
/// names the command in each entry's resource.
fn decisions<'a>(journal: &'a [nc_journal::Record], command: &str) -> Vec<&'a nc_journal::Record> {
    journal
        .iter()
        .filter(|record| {
            matches!(
                record.result,
                Outcome::Allowed | Outcome::Denied | Outcome::Failed
            ) && record
                .resource
                .clone()
                .into_object()
                .and_then(|fields| fields.get("command").cloned())
                .and_then(Value::into_text)
                .as_deref()
                == Some(command)
        })
        .collect()
}

/// A command that runs ends in 0, its answer on standard output — one
/// canonical document under `--json`.
#[test]
fn a_command_that_runs_is_exit_0() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["journal.verify", "--from", "0"]);
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    assert_eq!(outcome.code(), 0);
    assert!(
        outcome.stdout.starts_with("{\"verified\":"),
        "{}",
        outcome.stdout
    );
    assert_eq!(outcome.stderr, "");
    let json = serving.run(&["--json", "journal.verify"]);
    assert_eq!(json.exit, Exit::Success, "{json:?}");
    assert!(
        json.stdout.starts_with("{\"ok\":{\"verified\":"),
        "{}",
        json.stdout
    );
    serving.stop();
}

/// Arguments refused end in 2: by the CLI against the schema the registry
/// lists, before anything is sent, and by the command itself.
#[test]
fn arguments_refused_by_the_cli_or_by_the_command_are_exit_2() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["journal.verify", "--from", "-1"]);
    ended_in(
        &outcome,
        Exit::Invalid,
        ErrorCode::ValidationArgumentsInvalid,
    );
    assert!(
        decisions(&serving.journal(), "journal.verify").is_empty(),
        "arguments the schema refuses reached the kernel"
    );
    let outcome = serving.run(&[
        "reference.note.put-then-fail",
        "--note",
        "photo",
        "--text",
        "written",
    ]);
    ended_in(&outcome, Exit::Invalid, ErrorCode::PluginCommandRefused);
    serving.stop();
}

/// A denial ends in 3, and so does a command a quorum must approve: only an
/// approval the kernel issued meets a quorum, and nothing issues one yet.
#[test]
fn a_denial_is_exit_3_and_so_is_a_quorums() {
    let mut serving = Serving::new();
    // `recipe` is `carla`'s, and private: nothing permits `ana` to read it.
    let outcome = serving.run(&["reference.note.get", "--note", "recipe"]);
    ended_in(&outcome, Exit::Denied, ErrorCode::PolicyDenied);
    let outcome = serving.run(&["limits.prune", "--dry_run"]);
    ended_in(&outcome, Exit::Denied, ErrorCode::PolicyDenied);
    let journal = serving.journal();
    let pruned = decisions(&journal, "limits.prune");
    assert_eq!(pruned.len(), 1, "{journal:?}");
    assert_eq!(pruned[0].result, Outcome::Denied);
    assert_eq!(
        pruned[0].deciding_policy.as_deref(),
        Some("core.destructive_needs_quorum")
    );
    serving.stop();
}

/// A command that cannot be undone ends the run in 4, with `NC-KERN-0006`.
///
/// The code's sentence names the Noctua window in the environment's language,
/// the exit is 4 under `--json` too, and the refusal is journaled with nothing
/// done.
#[test]
fn an_irreversible_command_is_exit_4_with_the_code_that_names_the_window() {
    let mut serving = Serving::new();
    let line = ["reference.irreversible.do", "--note", "photo"];
    let outcome = serving.run(&line);
    ended_in(
        &outcome,
        Exit::NeedsTheInterface,
        ErrorCode::KernelUnconfirmed,
    );
    assert_eq!(outcome.code(), 4);
    assert!(
        outcome.stderr.contains("Noctua window"),
        "{}",
        outcome.stderr
    );
    assert_eq!(outcome.stdout, "");
    let portuguese = nc_cli::Env::of(|name| (name == "LANG").then(|| "pt_BR.UTF-8".to_owned()));
    let outcome = serving.run_in(&line, &portuguese);
    assert_eq!(outcome.exit, Exit::NeedsTheInterface, "{outcome:?}");
    let window = nc_catalog::message("error.kernel.unconfirmed", Locale::PtBR)
        .expect("the code's sentence in Portuguese");
    assert!(window.contains("janela do Noctua"), "{window}");
    assert!(outcome.stderr.contains(window), "{}", outcome.stderr);
    let json = serving.run(&["--json", "reference.irreversible.do", "--note", "photo"]);
    assert_eq!(json.exit, Exit::NeedsTheInterface, "{json:?}");
    let document = nc_canon::parse(json.stdout.trim_end()).expect("one document");
    let error = document
        .into_object()
        .and_then(|fields| fields.get("error").cloned())
        .and_then(Value::into_object)
        .expect("the document is the refusal");
    assert_eq!(
        error.get("code"),
        Some(&Value::text(ErrorCode::KernelUnconfirmed.code()))
    );
    assert_eq!(error.get("exit"), Some(&Value::Integer(4)));
    let journal = serving.journal();
    let refused = decisions(&journal, "reference.irreversible.do");
    assert_eq!(refused.len(), 3, "{journal:?}");
    for record in refused {
        assert_eq!(record.result, Outcome::Denied);
        assert_eq!(
            record.deciding_policy.as_deref(),
            Some("kernel.irreversible_needs_confirmation")
        );
    }
    // Nothing of the command ran: no unit was opened for it.
    assert!(
        journal
            .iter()
            .all(|record| record.result != Outcome::UnitBegun),
        "{journal:?}"
    );
    serving.stop();
}

/// What a command names that is not there ends in 5: the step an undo would
/// take back, and a command the registry does not hold, which is never sent.
#[test]
fn what_is_not_there_is_exit_5() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["kernel.undo"]);
    ended_in(&outcome, Exit::NotFound, ErrorCode::KernelNothingToUndo);
    let outcome = serving.run(&["reference.nowhere.to-go"]);
    ended_in(&outcome, Exit::NotFound, ErrorCode::KernelUnknownCommand);
    assert!(
        decisions(&serving.journal(), "reference.nowhere.to-go").is_empty(),
        "a command the registry does not hold was sent"
    );
    serving.stop();
}

/// An undo whose step's write was changed since ends the run in 6.
///
/// The change is another person's of the realm, written into the daemon's
/// database beside it, and the undo refused changes nothing.
#[test]
fn a_conflict_is_exit_6() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["reference.note.put", "--note", "photo", "--text", "written"]);
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    changed_by_bruno(&serving, "note/photo");
    let outcome = serving.run(&["kernel.undo"]);
    ended_in(&outcome, Exit::Conflict, ErrorCode::KernelUndoConflict);
    let held = serving.run(&["reference.note.get", "--note", "photo"]);
    assert_eq!(held.exit, Exit::Success, "{held:?}");
    assert_eq!(
        held.stdout, "{\"text\":\"changed\"}\n",
        "an undo refused changed it"
    );
    serving.stop();
}

/// Commits a step of `bruno`'s that writes `key` of the reference plugin.
///
/// It lands in the daemon's database while the daemon runs: what another
/// person's write would leave there.
fn changed_by_bruno(serving: &Serving, key: &str) {
    let layout = &serving.installed.layout;
    let pdp = nc_authz::Pdp::embedded("family").expect("the family pack is carried");
    let world =
        nc_authz::MemoryDirectory::from_world(include_str!("../../../policy/world.json"), &pdp)
            .expect("the world loads");
    // A flake that is not the daemon's, so no unit of this one is one of its.
    let device = nc_daemon::device::Device::read(layout, &serving.installed.user)
        .expect("the daemon's device");
    let flake =
        nc_ids::MachineFlake::from_value((device.flake().value() + 1) % 1_024).expect("a flake");
    let mut minter = nc_ids::Minter::new(flake, nc_ids::SystemClock);
    let unit = nc_local::UnitRecord {
        unit: minter.mint().expect("a unit"),
        meta: nc_local::UnitMeta {
            realm: world.realm_id("home").expect("home is a realm"),
            principal: world
                .principal(nc_authz::PrincipalKind::User, "bruno")
                .expect("bruno is the world's")
                .id()
                .clone(),
            kind: nc_local::UnitKind::Step,
            label: "changed by bruno".to_owned(),
            committed_at_ms: nc_daemon::server::now_ms(),
        },
        writes: vec![nc_local::StagedWrite {
            // The identity the plugin is loaded under, which its store is kept by.
            plugin: "noctua:reference".to_owned(),
            key: key.to_owned(),
            expect: nc_local::Expect::Anything,
            after: Some("{\"text\":\"changed\"}".to_owned()),
        }],
        blobs: Vec::new(),
        notes: Vec::new(),
        unpinned: Vec::new(),
        marks: Vec::new(),
    };
    let quotas = nc_local::Quotas {
        entries_per_plugin: 100,
        lowered: std::collections::BTreeMap::new(),
        blobs: 10,
        writes_per_unit: 10,
        notes_per_unit: 10,
    };
    serving.runtime.block_on(async {
        let store = nc_local::LocalStore::open(&layout.data)
            .await
            .expect("the daemon's database opens beside it");
        store
            .commit(&unit, &quotas)
            .await
            .expect("bruno's step lands");
        store.close().await;
    });
}

/// A plugin quarantined for the kills it took ends a command of it in 7, the
/// command never run: the third kill inside the window quarantines it.
#[test]
fn a_command_of_a_quarantined_plugin_is_exit_7() {
    let mut serving = Serving::new();
    let line = ["reference.abuse.trap", "--note", "photo"];
    let budget = nc_catalog::KERNEL.restart_budget;
    for kill in 1..=budget {
        let outcome = serving.run(&line);
        let code = if kill < budget {
            ErrorCode::PluginTrapped
        } else {
            ErrorCode::PluginQuarantined
        };
        ended_in(&outcome, Exit::Internal, code);
    }
    let outcome = serving.run(&line);
    ended_in(&outcome, Exit::Unavailable, ErrorCode::PluginQuarantined);
    let outcome = serving.run(&["reference.echo", "--note", "photo", "--text", "\"said\""]);
    ended_in(&outcome, Exit::Unavailable, ErrorCode::PluginQuarantined);
    serving.stop();
}

/// A plugin that traps ends the run in 8, and is restarted: the next command
/// of it runs.
#[test]
fn a_plugin_that_traps_is_exit_8() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["reference.abuse.trap", "--note", "photo"]);
    ended_in(&outcome, Exit::Internal, ErrorCode::PluginTrapped);
    let outcome = serving.run(&["reference.echo", "--note", "photo", "--text", "\"said\""]);
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    assert_eq!(outcome.stdout, "\"said\"\n");
    serving.stop();
}

/// A daemon that stopped ends the run in 9: nothing listens at its endpoint.
#[test]
fn a_stopped_daemon_is_exit_9() {
    let mut serving = Serving::new();
    let outcome = serving.run(&["journal.verify"]);
    assert_eq!(outcome.exit, Exit::Success, "{outcome:?}");
    serving.stop();
    let outcome = serving.run(&["journal.verify"]);
    ended_in(&outcome, Exit::Unreachable, ErrorCode::IpcUnreachable);
}

/// A token that is not the daemon's ends the run in 10, before anything is sent.
///
/// The daemon turns the client away and logs it, and the journal holds nothing
/// of it.
#[test]
fn a_token_that_is_not_the_daemons_is_exit_10() {
    use std::io::Write as _;
    let mut serving = Serving::new();
    let token = &serving.installed.layout.token;
    std::fs::remove_file(token).expect("the daemon's token is removed");
    nc_platform::create_private(token)
        .and_then(|mut file| {
            file.write_all(
                nc_ipc::Token::fresh()
                    .expect("another token")
                    .to_text()
                    .as_bytes(),
            )
        })
        .expect("another token, written as a daemon writes one");
    let outcome = serving.run(&["journal.verify"]);
    assert_eq!(outcome.exit, Exit::Refused, "{outcome:?}");
    assert_eq!(outcome.code(), 10);
    assert!(
        outcome.stderr.starts_with("error[NC-IPC-"),
        "{}",
        outcome.stderr
    );
    assert!(serving.journal().is_empty(), "something was dispatched");
    serving.stop();
    assert!(
        serving
            .installed
            .standard_error()
            .lines()
            .any(|line| line.contains("\"_msg\":\"ipc.refused\"")),
        "{}",
        serving.installed.standard_error()
    );
}

/// Process control that does not finish ends the run in 11: a start whose
/// daemon is not where the install keeps it, and a stop nothing answers whose
/// PID record names a process that is not the daemon — which it never
/// signals.
///
/// **This test process stands in for that process**, recorded with another
/// executable, so a stop that signalled it would end the test.
#[test]
fn process_control_that_does_not_finish_is_exit_11() {
    use std::io::Write as _;
    let installed = Installed::new(Path::new(BINARY));
    let found = nc_channel::found::beside(&installed.executable).expect("the install reads");
    let run = |verb: &str| nc_cli::run(&[OsString::from(verb)], &found, &nc_cli::Env::of(|_| None));
    // The suites install the daemon under the name the build gave it, and
    // `start` runs the one the layout names, which is not there.
    let outcome = run("start");
    ended_in(
        &outcome,
        Exit::ProcessControl,
        ErrorCode::CliDaemonDidNotStart,
    );
    let mut record = nc_channel::record::Record::of(
        nc_platform::process::this_process().expect("this process is read"),
        [7; 16],
    );
    record.executable = std::fs::canonicalize(&installed.executable).expect("installed");
    nc_platform::create_private(&installed.layout.pid_file)
        .expect("a record")
        .write_all(record.to_canonical().expect("written").as_bytes())
        .expect("written");
    let outcome = run("stop");
    ended_in(
        &outcome,
        Exit::ProcessControl,
        ErrorCode::CliProcessUnverified,
    );
    assert!(
        outcome.stderr.contains("nothing was signalled"),
        "{}",
        outcome.stderr
    );
    assert!(installed.layout.stopped.is_file(), "the stop wrote no mark");
}

/// A start whose daemon ends before it answers ends the run in 11 at once,
/// with what that daemon wrote to its standard error.
///
/// The daemon is the one this build made, where the layout names it beside the
/// manifest, and the manifest names a central log off this machine, which the
/// daemon refuses as it starts.
#[test]
fn a_start_whose_daemon_ends_before_it_answers_is_exit_11_with_what_it_wrote() {
    let installed = Installed::new(Path::new(BINARY));
    let layout = &installed.layout;
    let daemon = layout.install.join(format!(
        "{}{}",
        layout.daemon,
        nc_platform::EXECUTABLE_SUFFIX
    ));
    if std::fs::hard_link(BINARY, &daemon).is_err() {
        std::fs::copy(BINARY, &daemon).expect("the daemon is copied where the layout names it");
    }
    installed.with_a_central_log_refused();
    let found = nc_channel::found::beside(&installed.executable).expect("the install reads");
    let began = std::time::Instant::now();
    let outcome = nc_cli::run(
        &[OsString::from("start")],
        &found,
        &nc_cli::Env::of(|_| None),
    );
    let took = began.elapsed();
    ended_in(
        &outcome,
        Exit::ProcessControl,
        ErrorCode::CliDaemonDidNotStart,
    );
    assert!(
        outcome.stderr.contains("before it answered"),
        "{}",
        outcome.stderr
    );
    assert!(
        outcome
            .stderr
            .contains(&format!("[{}]", ErrorCode::DaemonNoCentralLog.code())),
        "the daemon's refusal is not shown: {}",
        outcome.stderr
    );
    let within = core::time::Duration::from_millis(nc_catalog::DAEMON.start_within_ms);
    assert!(
        took < within / 2,
        "a start waited {took:?} for a daemon that had ended"
    );
}

/// An install the CLI cannot read ends its run in 12, and stops the daemon too.
///
/// The daemon beside it refuses to start from it with the same code, because
/// both read the manifest through one reader: no manifest, and one another
/// account can reach.
#[test]
fn no_install_is_exit_12_and_the_daemon_refuses_it_with_the_same_code() {
    let installed = Installed::new(Path::new(BINARY));
    let manifest = installed.layout.manifest.clone();
    let text = std::fs::read(&manifest).expect("the manifest reads");
    let env = nc_cli::Env::of(|_| None);
    let argv = [OsString::from("journal.verify")];
    std::fs::remove_file(&manifest).expect("the manifest is removed");
    let reopened = || {
        use std::io::Write as _;
        nc_platform::create_private(&manifest)
            .and_then(|mut file| file.write_all(&text))
            .expect("the manifest is written again");
    };
    for (code, before) in [
        (ErrorCode::ChannelNoManifest, None),
        (ErrorCode::ChannelManifestNotPrivate, Some(&reopened)),
    ] {
        if let Some(before) = before {
            before();
            nc_platform::testing::open_to_another_user(&manifest)
                .expect("the manifest is opened to another user");
        }
        let refused = nc_channel::found::beside(&installed.executable)
            .expect_err("the CLI reads no install there");
        assert_eq!(refused.code(), code, "{refused}");
        let outcome = nc_cli::unfound(&argv, &refused, &env);
        ended_in(&outcome, Exit::NoInstall, code);
        let mut daemon = installed.spawn();
        let ended = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a runtime")
            .block_on(daemon.ended());
        assert_eq!(ended, Some(1), "{}", installed.standard_error());
        assert!(
            installed
                .standard_error()
                .contains(&format!("noctua-daemon: [{}]", code.code())),
            "{}",
            installed.standard_error()
        );
    }
}
