//! Every command the catalog declares and every command of the reference
//! plugin, run through the CLI against a real daemon (`NOCTUA-PLAN.md`,
//! Stage 13, acceptance 2): each decided as the person the profile's world
//! names, with the grant and the epoch it names, and found in the journal
//! afterwards; and help printed from the daemon's registry, each command by
//! its name and its title and each argument from its command's schema.
//!
//! **Acceptance 8 of Stage 14 here too**: a name `naming.propose` answers is
//! read back by `naming.read` as the fields that built it, through the CLI and
//! through the daemon's endpoint as one JSON object — a raw dispatch, the path
//! `noctua mcp` will take — each decided on the platform's `naming` feature.
//!
//! **The CLI in-process** (`nc_cli::run`), so no `noctua` a build left behind
//! is ever what runs, against the daemon this build made, copied where an
//! installer puts it beside a manifest under a scratch root and a namespace
//! of its own (`nc_daemon::testing::Installed`); and the journal read from the
//! daemon's own database after each run, while the daemon runs. The reference
//! plugin is installed with a manifest that grants it every capability its
//! commands reach for but the network and an event subscription, so a command
//! does what it is written to rather than meet a capability refused, and no
//! command reaches a network. It spawns a process, so no mutation command runs
//! it.

use std::collections::BTreeMap;
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

/// The reference plugin's manifest as this suite installs it.
///
/// It grants every capability the plugin's commands reach for, but `net` — no
/// suite reaches a network — and `events.subscribe`, which would deliver its
/// own events back to it.
const GRANTED: &str = "id = \"noctua:reference\"\nnamespace = \"reference\"\ncapabilities = \
                       [\"context\", \"store\", \"blobs.read\", \"blobs.write\", \
                       \"commands.dispatch\", \"commands.begin\", \"events.publish\", \"ui\", \
                       \"timers\"]\n";

/// The note every command of the reference plugin acts on: `ana`'s own, which
/// the realm may read.
const NOTE: &str = "photo";

/// The word in a row that stands for the hash of the blob [`BLOB`] makes.
const HASH: &str = "<hash>";

/// The text the blob a row makes holds, written once.
const BLOB: &str = "said";

/// How a command's own entry in the journal ended.
#[derive(Debug, Clone, Copy)]
enum Journaled {
    /// Allowed by a permit of the realm's pack, and answered.
    Allowed,
    /// Allowed by a permit of the realm's pack, then failed, for the reason
    /// the entry names.
    Failed(&'static str),
    /// Denied, by the rule named.
    Denied(&'static str),
}

/// One command run through the CLI: the words after its name, how the run
/// ends, the code a refusal carries, and its own entry in the journal.
struct Row {
    name: &'static str,
    words: &'static [&'static str],
    exit: Exit,
    code: Option<ErrorCode>,
    journaled: Journaled,
}

/// A row whose command runs and is answered.
const fn ran(name: &'static str, words: &'static [&'static str]) -> Row {
    Row {
        name,
        words,
        exit: Exit::Success,
        code: None,
        journaled: Journaled::Allowed,
    }
}

/// A row whose run ends in `exit` with `code`, its entry `journaled` so.
const fn ended(
    name: &'static str,
    words: &'static [&'static str],
    exit: Exit,
    code: ErrorCode,
    journaled: Journaled,
) -> Row {
    Row {
        name,
        words,
        exit,
        code: Some(code),
        journaled,
    }
}

/// The words that propose `NOCTUA.md §16.3`'s own example, as a person types
/// its parts.
const PROPOSED: &[&str] = &[
    "--title",
    "Praia de Copacabana",
    "--signature",
    "familia",
    "--signature",
    "fotos",
    "--keywords",
    "brasil",
    "--extension",
    "jpg",
];

/// The name `NOCTUA.md §16.3` writes down.
const THE_EXAMPLE: &str = "==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg";

/// The permit every naming command a person is allowed is decided by.
const NAMES_FILES: &str = "family.a_member_names_files";

/// The note's words, and the note with some text.
const A_NOTE: &[&str] = &["--note", NOTE];
const A_NOTE_AND_TEXT: &[&str] = &["--note", NOTE, "--text", "written"];

/// Every command the daemon's registry holds, in the order each is run.
///
/// **Held both ways to the registry** (`the_table_is_the_registry`).
/// **Undo and redo after the write they step through**; and **the abuses
/// last**, after the two commands that trap once they have written: a plugin
/// killed `restart_budget` times inside `restart_window_ms` is quarantined,
/// so the third kill — `abuse.trap`'s — quarantines the plugin and the three
/// abuses after it are refused by the quarantine without running. Each is
/// still dispatched through the CLI, decided and journaled; what each abuse
/// does when it runs is the kernel's suites'.
///
/// **A command that deletes is denied for want of a quorum**: only an approval
/// the kernel issued meets one, and nothing issues one yet, so it is a denial
/// (`NC-POL-0003`, exit 3) and never the Noctua window's exit 4, which belongs
/// to `NC-KERN-0006` alone.
const ROWS: &[Row] = &[
    ran("journal.verify", &["--from", "0"]),
    ran("naming.propose", PROPOSED),
    ran("naming.read", &["--name", THE_EXAMPLE]),
    ended(
        "limits.prune",
        &["--dry_run"],
        Exit::Denied,
        ErrorCode::PolicyDenied,
        Journaled::Denied("core.destructive_needs_quorum"),
    ),
    ran("reference.note.put", A_NOTE_AND_TEXT),
    ran("kernel.undo", &[]),
    ran("kernel.redo", &[]),
    ran("reference.note.get", A_NOTE),
    ran("reference.probe", A_NOTE),
    ended(
        "reference.note.put-then-fail",
        A_NOTE_AND_TEXT,
        Exit::Invalid,
        ErrorCode::PluginCommandRefused,
        Journaled::Failed("refused"),
    ),
    ended(
        "reference.note.delete",
        A_NOTE,
        Exit::Denied,
        ErrorCode::PolicyDenied,
        Journaled::Denied("core.destructive_needs_quorum"),
    ),
    ran("reference.note.share", A_NOTE_AND_TEXT),
    ran("reference.note.scribble", A_NOTE),
    ran(
        "reference.executions.count",
        &["--note", NOTE, "--at_most", "1000000", "--verbose"],
    ),
    ended(
        "reference.irreversible.do",
        A_NOTE,
        Exit::NeedsTheInterface,
        ErrorCode::KernelUnconfirmed,
        Journaled::Denied("kernel.irreversible_needs_confirmation"),
    ),
    ran(
        "reference.relay.write",
        &["--note", NOTE, "--name", "journal.verify", "--args", "{}"],
    ),
    ran(
        "reference.relay.read",
        &["--note", NOTE, "--name", "journal.verify", "--args", "{}"],
    ),
    ran("reference.echo", &["--note", NOTE, "--text", "\"said\""]),
    ran(
        "reference.effect.post",
        &["--note", NOTE, "--url", "https://example.com/"],
    ),
    ran(
        "reference.compensated.post",
        &["--note", NOTE, "--url", "https://example.com/"],
    ),
    ran("reference.unit.commit", A_NOTE_AND_TEXT),
    ended(
        "reference.unit.abort",
        A_NOTE_AND_TEXT,
        Exit::Internal,
        ErrorCode::KernelUnitAborted,
        Journaled::Allowed,
    ),
    ended(
        "reference.unit.drop",
        A_NOTE_AND_TEXT,
        Exit::Internal,
        ErrorCode::KernelUnitAborted,
        Journaled::Allowed,
    ),
    ran(
        "reference.event.publish",
        &["--note", NOTE, "--topic", "reference.said"],
    ),
    ran(
        "reference.event.announce",
        &["--note", NOTE, "--topic", "reference.said"],
    ),
    // An hour away, so no timer fires while the suite runs.
    ran(
        "reference.timer.set",
        &["--note", NOTE, "--ms", "3600000", "--tag", "later"],
    ),
    ran(
        "reference.timer.remind",
        &["--note", NOTE, "--ms", "3600000", "--tag", "later"],
    ),
    ran("reference.timer.cancel", &["--note", NOTE, "--timer", "1"]),
    ran(
        "reference.ui.show",
        &["--note", NOTE, "--title", "Shown", "--kind", "panel"],
    ),
    ran("reference.context.read", A_NOTE),
    ran(
        "reference.log.write",
        &["--note", NOTE, "--text", "said", "--level", "info"],
    ),
    ran("reference.blob.put", &["--note", NOTE, "--text", BLOB]),
    ran("reference.blob.get", &["--note", NOTE, "--hash", HASH]),
    ran("reference.blob.locate", &["--note", NOTE, "--hash", HASH]),
    ran(
        "reference.note.put-many",
        &["--note", NOTE, "--text", "written", "--parts", "2"],
    ),
    // The peer it writes in too is not installed, which it answers with.
    ran("reference.pair.put", A_NOTE_AND_TEXT),
    ran("reference.pair.put-then-trap", A_NOTE_AND_TEXT),
    ended(
        "reference.note.put-then-trap",
        A_NOTE_AND_TEXT,
        Exit::Internal,
        ErrorCode::PluginTrapped,
        Journaled::Failed("trapped"),
    ),
    ended(
        "reference.note.put-then-abuse",
        &["--note", NOTE, "--text", "written", "--abuse", "trap"],
        Exit::Internal,
        ErrorCode::PluginTrapped,
        Journaled::Failed("trapped"),
    ),
    ran(
        "reference.abuse.fetch",
        &["--note", NOTE, "--url", "https://example.com/"],
    ),
    ran("reference.abuse.reenter", A_NOTE),
    ended(
        "reference.abuse.trap",
        A_NOTE,
        Exit::Internal,
        ErrorCode::PluginQuarantined,
        Journaled::Failed("trapped"),
    ),
    ended(
        "reference.abuse.spin",
        A_NOTE,
        Exit::Unavailable,
        ErrorCode::PluginQuarantined,
        Journaled::Failed("quarantined"),
    ),
    ended(
        "reference.abuse.burn",
        A_NOTE,
        Exit::Unavailable,
        ErrorCode::PluginQuarantined,
        Journaled::Failed("quarantined"),
    ),
    ended(
        "reference.abuse.grow",
        A_NOTE,
        Exit::Unavailable,
        ErrorCode::PluginQuarantined,
        Journaled::Failed("quarantined"),
    ),
];

/// A command as the daemon's registry lists it: the fields this suite reads.
struct Registered {
    title: String,
    args_schema: String,
    action: String,
    resource_selector: String,
}

/// A daemon of a scratch install with the reference plugin installed, running,
/// with what the CLI reads its install by and its registry as it answers it.
struct Serving {
    // Fields drop in the order they are written, so a test that panics
    // kills the daemon and waits for it before its install is removed.
    daemon: nc_daemon::testing::Running,
    client: nc_ipc::Client,
    runtime: tokio::runtime::Runtime,
    found: nc_channel::found::Found,
    registry: BTreeMap<String, Registered>,
    installed: Installed,
}

impl Serving {
    /// The daemon started and running, its registry read.
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
        let (mut client, registry) = runtime.block_on(async {
            let (mut client, _) = daemon.connected().await;
            running(&mut client).await;
            let Answer::Registry(entries) = asked(&mut client, Request::Registry).await else {
                panic!("the registry was not answered");
            };
            (client, entries)
        });
        let registry = registry.iter().map(registered).collect();
        let found = nc_channel::found::beside(&installed.executable)
            .expect("the CLI reads the install the daemon runs from");
        // A daemon that refused the plugin would hold no reference command.
        runtime.block_on(async {
            let status = asked(&mut client, Request::Status).await;
            assert!(
                matches!(&status, Answer::Status(status)
                    if status.to_canonical().contains("\"loaded\":true")),
                "{status:?}"
            );
        });
        Self {
            daemon,
            client,
            runtime,
            found,
            registry,
            installed,
        }
    }

    /// The CLI run with `words`, in the language `env` names.
    fn run(&self, words: &[String], env: &nc_cli::Env) -> nc_cli::Outcome {
        let argv: Vec<OsString> = words.iter().map(OsString::from).collect();
        nc_cli::run(&argv, &self.found, env)
    }

    /// Stops the daemon through its endpoint, and waits for it to end well.
    fn stopped(mut self) {
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

/// A registry entry, by its name.
fn registered(entry: &Value) -> (String, Registered) {
    let fields = entry.clone().into_object().expect("an entry is an object");
    let text = |key: &str| {
        fields
            .get(key)
            .cloned()
            .and_then(Value::into_text)
            .unwrap_or_else(|| panic!("an entry's {key} is text: {entry:?}"))
    };
    (
        text("name"),
        Registered {
            title: text("title"),
            args_schema: text("args_schema"),
            action: text("action"),
            resource_selector: text("resource_selector"),
        },
    )
}

/// The language a person reading English names.
fn english() -> nc_cli::Env {
    nc_cli::Env::of(|_| None)
}

/// The words a row's command line is, its name first, with the blob's hash
/// where the row names it.
fn words_of(row: &Row) -> Vec<String> {
    let hash = blake3::hash(BLOB.as_bytes()).to_hex().to_string();
    std::iter::once(row.name)
        .chain(row.words.iter().copied())
        .map(|word| {
            if word == HASH {
                hash.clone()
            } else {
                word.to_owned()
            }
        })
        .collect()
}

/// The table names every command the daemon's registry holds, each once, and
/// none it does not; the abuses come last.
#[test]
fn the_table_is_the_registry() {
    let serving = Serving::new();
    let mut named: Vec<&str> = ROWS.iter().map(|row| row.name).collect();
    named.sort_unstable();
    let before = named.len();
    named.dedup();
    assert_eq!(named.len(), before, "a command is in the table twice");
    let registered: Vec<&str> = serving.registry.keys().map(String::as_str).collect();
    assert_eq!(
        named, registered,
        "the table is not the daemon's registry, both ways"
    );
    let catalog: Vec<&str> = nc_catalog::COMMANDS
        .iter()
        .map(|command| command.name)
        .collect();
    assert!(
        catalog.iter().all(|name| named.contains(name)),
        "a command the catalog declares is not run: {catalog:?}"
    );
    let abuse = |row: &Row| row.name.starts_with("reference.abuse.");
    let first = ROWS
        .iter()
        .position(abuse)
        .expect("the reference plugin's abuses are run");
    assert!(
        ROWS[first..].iter().all(abuse),
        "a command is run after the first abuse"
    );
    serving.stopped();
}

/// Every command is run through the CLI and ends as its row says, each decided
/// as the profile's person in the profile's realm, with the grant and the
/// epoch the world names, and each found in the journal afterwards with the
/// outcome its row says: allowed by a permit of the realm's pack, or denied by
/// the rule named.
#[test]
fn every_command_runs_through_the_cli_decided_and_journaled() {
    let serving = Serving::new();
    let store = serving
        .runtime
        .block_on(nc_local::LocalStore::open(&serving.installed.layout.data))
        .expect("the daemon's database opens beside it");
    let mut seen = 0;
    for row in ROWS {
        let outcome = serving.run(&words_of(row), &english());
        assert_eq!(outcome.exit, row.exit, "{}: {outcome:?}", row.name);
        match row.code {
            Some(code) => assert!(
                outcome
                    .stderr
                    .starts_with(&format!("error[{}]", code.code())),
                "{}: {}",
                row.name,
                outcome.stderr
            ),
            None => assert_eq!(outcome.stderr, "", "{}", row.name),
        }
        let read = serving
            .runtime
            .block_on(nc_journal::Store::from(&store.journal(), 0))
            .expect("the journal reads");
        let fresh = &read.entries[seen..];
        seen = read.entries.len();
        let own: Vec<&nc_journal::Record> = fresh
            .iter()
            .map(|entry| &entry.record)
            .filter(|record| {
                matches!(
                    record.result,
                    Outcome::Allowed | Outcome::Denied | Outcome::Failed
                ) && command_of(record).as_deref() == Some(row.name)
            })
            .collect();
        assert_eq!(own.len(), 1, "{}: {fresh:?}", row.name);
        held_to(row, own[0], &serving.registry[row.name]);
    }
    serving.runtime.block_on(store.close());
    serving.stopped();
}

/// The command a decision's entry names, as the decision point writes it into
/// the entry's resource.
fn command_of(record: &nc_journal::Record) -> Option<String> {
    record
        .resource
        .clone()
        .into_object()?
        .get("command")?
        .clone()
        .into_text()
}

/// `record`, the entry of `row`'s command, held to what the row and the
/// registry say.
fn held_to(row: &Row, record: &nc_journal::Record, registered: &Registered) {
    let name = row.name;
    assert_eq!(record.principal.as_str(), "ana", "{name}");
    assert_eq!(record.realm.as_str(), "home", "{name}");
    assert_eq!(record.action, registered.action, "{name}");
    assert_eq!(
        record.grant_used.as_ref().map(nc_ids::OpaqueId::as_str),
        Some(nc_daemon::testing::GRANT),
        "{name}: not decided with the grant the world names"
    );
    assert_eq!(
        record.policy_epoch,
        u64::try_from(nc_daemon::testing::EPOCH).ok(),
        "{name}: not decided at the epoch the world names"
    );
    let entity = match registered.resource_selector.as_str() {
        "realm" => "{\"id\":\"home\",\"type\":\"Realm\"}".to_owned(),
        selector => selector.strip_prefix("feature:").map_or_else(
            || format!("{{\"id\":\"{NOTE}\",\"type\":\"Content\"}}"),
            |feature| format!("{{\"id\":\"{feature}\",\"type\":\"Feature\"}}"),
        ),
    };
    let resource = record
        .resource
        .clone()
        .into_object()
        .expect("a decision's resource is an object");
    assert_eq!(
        resource.get("entity").map(Value::to_canonical),
        Some(entity),
        "{name}"
    );
    let failure = resource.get("failure").cloned().and_then(Value::into_text);
    let decided = record.deciding_policy.as_deref().unwrap_or_default();
    match row.journaled {
        Journaled::Allowed | Journaled::Failed(_) => {
            assert!(
                decided.split(',').all(|rule| rule.starts_with("family.")),
                "{name}: allowed by `{decided}`, which is no permit of the realm's pack"
            );
        }
        Journaled::Denied(rule) => assert_eq!(decided, rule, "{name}"),
    }
    match row.journaled {
        Journaled::Allowed => {
            assert_eq!(record.result, Outcome::Allowed, "{name}");
            assert_eq!(failure, None, "{name}");
        }
        Journaled::Failed(why) => {
            assert_eq!(record.result, Outcome::Failed, "{name}");
            assert_eq!(failure.as_deref(), Some(why), "{name}");
        }
        Journaled::Denied(_) => assert_eq!(record.result, Outcome::Denied, "{name}"),
    }
}

/// Help lists every command the registry holds, and each one's arguments.
///
/// Each command by its name and its title — the catalog's in the environment's
/// language, a plugin's label as it declared it — and help on each command
/// shows a flag for each argument its schema declares; nothing help prints is
/// dispatched.
#[test]
fn help_lists_every_command_by_its_name_and_title_and_each_argument_from_its_schema() {
    let serving = Serving::new();
    let portuguese = nc_cli::Env::of(|name| (name == "LANG").then(|| "pt_BR.UTF-8".to_owned()));
    for (env, locale) in [(english(), Locale::En), (portuguese, Locale::PtBR)] {
        let help = serving.run(&["help".to_owned()], &env);
        assert_eq!(help.exit, Exit::Success, "{help:?}");
        for (name, registered) in &serving.registry {
            let title = nc_catalog::command(name).map_or_else(
                || registered.title.clone(),
                |command| {
                    nc_catalog::message(command.title_key, locale)
                        .expect("the catalog's title")
                        .to_owned()
                },
            );
            assert!(
                help.stdout
                    .lines()
                    .any(|line| line.trim_start().starts_with(&format!("{name} "))
                        && line.trim_end().ends_with(&title)),
                "{name} is not listed with its title {title:?}:\n{}",
                help.stdout
            );
        }
    }
    for (name, registered) in &serving.registry {
        let one = serving.run(&["help".to_owned(), name.clone()], &english());
        assert_eq!(one.exit, Exit::Success, "{name}: {one:?}");
        let schema = nc_canon::parse(&registered.args_schema).expect("a schema is JSON");
        let properties = schema
            .into_object()
            .and_then(|fields| fields.get("properties").cloned())
            .and_then(Value::into_object)
            .unwrap_or_default();
        for property in properties.keys() {
            assert!(
                one.stdout.contains(&format!("\n  --{property}")),
                "{name}'s help shows no flag for `{property}`:\n{}",
                one.stdout
            );
        }
        if properties.is_empty() {
            assert!(
                one.stdout.contains("Arguments:\n  none\n"),
                "{}",
                one.stdout
            );
        }
        if let Some(command) = nc_catalog::command(name) {
            let description = nc_catalog::message(command.description_key, Locale::En)
                .expect("the catalog's description");
            assert!(one.stdout.contains(description), "{}", one.stdout);
        }
    }
    let store = serving
        .runtime
        .block_on(nc_local::LocalStore::open(&serving.installed.layout.data))
        .expect("the daemon's database opens beside it");
    let read = serving
        .runtime
        .block_on(nc_journal::Store::from(&store.journal(), 0))
        .expect("the journal reads");
    serving.runtime.block_on(store.close());
    assert!(
        read.entries.is_empty(),
        "help dispatched {:?}",
        read.entries
    );
    serving.stopped();
}

/// What `--json` wrote for `words`, run through the CLI: its answer, under
/// the `ok` it is written in, once the run ended well.
fn answered_to(serving: &Serving, words: &[&str]) -> Value {
    let argv: Vec<String> = std::iter::once("--json")
        .chain(words.iter().copied())
        .map(ToOwned::to_owned)
        .collect();
    let outcome = serving.run(&argv, &english());
    assert_eq!(outcome.exit, Exit::Success, "{words:?}: {outcome:?}");
    nc_canon::parse(outcome.stdout.trim_end())
        .expect("one canonical document")
        .into_object()
        .and_then(|mut fields| fields.remove("ok"))
        .unwrap_or_else(|| panic!("{words:?} answered no `ok`: {}", outcome.stdout))
}

/// The text the field `name` of `answer` holds.
fn text_of(answer: &Value, name: &str) -> String {
    answer
        .clone()
        .into_object()
        .and_then(|mut fields| fields.remove(name))
        .and_then(Value::into_text)
        .unwrap_or_else(|| panic!("`{name}` of {answer:?} is not text"))
}

/// A reading's fields, with the `canonical` it adds to a proposal's taken out
/// and answered beside them.
fn read_as_proposed(reading: &Value) -> (Value, Option<Value>) {
    let mut fields = reading
        .clone()
        .into_object()
        .expect("an answer is an object");
    let canonical = fields.remove("canonical");
    (Value::Object(fields), canonical)
}

/// Every naming command the daemon's journal holds, allowed, by the policy
/// that decided it.
fn naming_decided(serving: &Serving) -> Vec<String> {
    let store = serving
        .runtime
        .block_on(nc_local::LocalStore::open(&serving.installed.layout.data))
        .expect("the daemon's database opens beside it");
    let read = serving
        .runtime
        .block_on(nc_journal::Store::from(&store.journal(), 0))
        .expect("the journal reads");
    serving.runtime.block_on(store.close());
    read.entries
        .iter()
        .map(|entry| &entry.record)
        .filter(|record| {
            record.result == Outcome::Allowed
                && command_of(record).is_some_and(|command| command.starts_with("naming."))
        })
        .map(|record| record.deciding_policy.clone().unwrap_or_default())
        .collect()
}

/// A name the CLI proposes is read back by the CLI as the fields that built
/// it (Stage 14, acceptance 8, the CLI's half).
///
/// Proposed from flags as a person types the parts, the answer is the scheme's
/// own example but for the identifier, which the daemon's minter made; read
/// back from that name, it is every field again and already canonical. Two
/// proposals are two identifiers, `--id` keeps the one given in its canonical
/// spelling, and a name with no identifier is refused as one this scheme
/// cannot read. Every one allowed was decided by the naming permit and no
/// other rule.
#[test]
fn a_name_the_cli_proposes_is_read_back_by_the_cli_as_the_fields_that_built_it() {
    let serving = Serving::new();
    let words = |command: &'static str, rest: &[&'static str]| -> Vec<&'static str> {
        std::iter::once(command)
            .chain(rest.iter().copied())
            .collect()
    };
    let proposed = answered_to(&serving, &words("naming.propose", PROPOSED));
    let id = text_of(&proposed, "id");
    let name = text_of(&proposed, "name");
    assert_eq!(
        name,
        THE_EXAMPLE.replace("01J8XK2M9P51", &id),
        "the example's parts built another name"
    );
    let (fields, canonical) = read_as_proposed(&answered_to(
        &serving,
        &["naming.read", "--name", name.as_str()],
    ));
    assert_eq!(canonical, Some(Value::Bool(true)));
    assert_eq!(
        fields, proposed,
        "the name read back is not the fields that built it"
    );
    let again = answered_to(&serving, &words("naming.propose", PROPOSED));
    assert_ne!(
        text_of(&again, "id"),
        id,
        "two proposals were one identifier"
    );
    let lower = id.to_lowercase();
    let kept = answered_to(
        &serving,
        &[
            "naming.propose",
            "--title",
            "Ipanema",
            "--id",
            lower.as_str(),
        ],
    );
    assert_eq!(text_of(&kept, "id"), id);
    assert_eq!(text_of(&kept, "name"), format!("--ipanema@@{id}"));
    let unread = serving.run(
        &[
            "naming.read".to_owned(),
            "--name".to_owned(),
            "nothing.txt".to_owned(),
        ],
        &english(),
    );
    assert_eq!(unread.exit, Exit::Invalid, "{unread:?}");
    assert!(
        unread
            .stderr
            .starts_with(&format!("error[{}]", ErrorCode::IdsNameUnparsable.code())),
        "{}",
        unread.stderr
    );
    let decided = naming_decided(&serving);
    assert_eq!(decided.len(), 4, "{decided:?}");
    assert!(
        decided.iter().all(|rule| rule == NAMES_FILES),
        "{decided:?}"
    );
    serving.stopped();
}

/// A name proposed over the endpoint as one JSON object is read back the same
/// (Stage 14, acceptance 8, the endpoint's half).
///
/// **The path `noctua mcp` will take**: one `Request::Dispatch` carrying the
/// arguments as one JSON object, through `nc_ipc` and nothing of the CLI's,
/// answered as one; the name it answers is read back over the same endpoint
/// as the same fields, and through the CLI as the same answer. A name with no
/// identifier is refused there as one this scheme cannot read.
#[test]
fn a_name_proposed_over_the_endpoint_as_one_json_object_is_read_back_the_same() {
    let mut serving = Serving::new();
    let arguments = nc_canon::parse(
        r#"{"extension":"jpg","keywords":["brasil"],"signature":["familia","fotos"],"title":"Praia de Copacabana"}"#,
    )
    .expect("one JSON object");
    let (proposed, read, refused) = serving.runtime.block_on(async {
        let Answer::Result(proposed) = asked(
            &mut serving.client,
            Request::Dispatch {
                command: "naming.propose".to_owned(),
                args: arguments,
            },
        )
        .await
        else {
            panic!("the proposal was not answered");
        };
        let name = text_of(&proposed, "name");
        let Answer::Result(read) = asked(
            &mut serving.client,
            Request::Dispatch {
                command: "naming.read".to_owned(),
                args: Value::object([("name", Value::text(name))]),
            },
        )
        .await
        else {
            panic!("the reading was not answered");
        };
        let refused = asked(
            &mut serving.client,
            Request::Dispatch {
                command: "naming.read".to_owned(),
                args: Value::object([("name", Value::text("nothing.txt"))]),
            },
        )
        .await;
        (proposed, read, refused)
    });
    let name = text_of(&proposed, "name");
    assert_eq!(
        name,
        THE_EXAMPLE.replace("01J8XK2M9P51", &text_of(&proposed, "id")),
        "the example's parts built another name"
    );
    let (fields, canonical) = read_as_proposed(&read);
    assert_eq!(canonical, Some(Value::Bool(true)));
    assert_eq!(
        fields, proposed,
        "the name read back is not the fields that built it"
    );
    assert_eq!(
        answered_to(&serving, &["naming.read", "--name", name.as_str()]),
        read,
        "the CLI and the endpoint read one name two ways"
    );
    let Answer::Refused(refusal) = refused else {
        panic!("a name with no identifier was answered: {refused:?}");
    };
    assert_eq!(
        (refusal.code, refusal.kind),
        (ErrorCode::IdsNameUnparsable, nc_ipc::RefusalKind::Invalid),
        "{}",
        refusal.sentence
    );
    let decided = naming_decided(&serving);
    assert_eq!(
        decided,
        [NAMES_FILES, NAMES_FILES, NAMES_FILES],
        "{decided:?}"
    );
    serving.stopped();
}
