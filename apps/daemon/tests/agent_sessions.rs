//! An agent's session at a real daemon's endpoint (`NOCTUA.md §20.3`): opened
//! only while MCP is on, for an agent the profile's person made and within
//! `sessions_max`; acting as that agent for that person, every dispatch
//! decided for both and journaled with both; asking for its tools and
//! dispatching commands and nothing else; ended once MCP is off; and what is
//! refused before the kernel logged by its code and never journaled.
//!
//! **The binary this build made, copied where an installer puts it**, beside a
//! manifest under a scratch root and a namespace of its own
//! (`nc_daemon::testing::Installed`), reached as `noctua mcp` reaches it:
//! through the endpoint, proven by the token the daemon wrote. The person's
//! own calls — switching MCP, making and granting an agent — go over
//! connections of their own, as the person's CLI would make them, and the
//! journal is read from the daemon's own database beside it. Every agent
//! named is one the daemon's minter made, or one a test minted that no person
//! made. It spawns a process, so no mutation command runs it.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

use core::time::Duration;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_daemon::testing::{Installed, PATIENCE, Running};
use nc_ipc::{Answer, IpcError, Request, State};
use nc_journal::Outcome;

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// The person the profile's world names.
const PERSON: &str = "ana";

/// The rule that refuses an agent past its budget.
const BUDGET: &str = "core.budget_is_a_permission";

/// The rule that refuses an agent what it holds no live grant for.
const PROVENANCE: &str = "core.provenance";

/// `request`, answered.
async fn asked(client: &mut nc_ipc::Client, request: Request) -> Answer {
    tokio::time::timeout(PATIENCE, client.ask(&request))
        .await
        .expect("answered in time")
        .expect("answered")
}

/// That `request` over `client` finds the connection closed.
async fn lost(client: &mut nc_ipc::Client, request: Request) {
    let asked = tokio::time::timeout(PATIENCE, client.ask(&request))
        .await
        .expect("answered or lost in time");
    assert!(
        matches!(asked, Err(IpcError::Lost(_))),
        "{request:?} on a connection the daemon closed was {asked:?}"
    );
}

/// That `answer` is a refusal with `code`.
fn refused_with(answer: &Answer, code: ErrorCode) {
    assert!(
        matches!(answer, Answer::Refused(refusal) if refusal.code == code),
        "{answer:?} is not refused with {}",
        code.code()
    );
}

/// A dispatch of `command` with `args`, written as JSON.
fn dispatch(command: &str, args: &str) -> Request {
    Request::Dispatch {
        command: command.to_owned(),
        args: nc_canon::parse(args).expect("JSON"),
    }
}

/// A proposal of a name, which an agent granted naming may make.
fn propose() -> Request {
    dispatch("naming.propose", r#"{"title":"a"}"#)
}

/// The tools a session is asked for, in English.
const TOOLS: Request = Request::Tools { locale: Locale::En };

/// An agent's identifier as a minter gives one, of no agent any person made.
fn minted() -> nc_ids::Snowflake {
    let flake = nc_ids::MachineFlake::from_value(6).expect("a flake");
    nc_ids::Minter::new(flake, nc_ids::SystemClock)
        .mint()
        .expect("minted")
}

/// The daemon of `installed`, started and running, with a connection of the
/// person's to it.
async fn started(installed: &Installed) -> (Running, nc_ipc::Client) {
    let mut daemon = installed.spawn();
    let (mut person, _) = daemon.connected().await;
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        match asked(&mut person, Request::Status).await {
            Answer::Status(status)
                if status
                    .to_canonical()
                    .contains(&format!("\"state\":\"{}\"", State::Running.word())) =>
            {
                break;
            }
            Answer::Status(_) => {}
            other => panic!("the status was answered with {other:?}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the daemon did not run within {PATIENCE:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (daemon, person)
}

/// Stops the daemon through `person`'s connection, and waits for it to end
/// well.
async fn stopped(person: &mut nc_ipc::Client, daemon: &mut Running, installed: &Installed) {
    assert_eq!(asked(person, Request::Stop).await, Answer::Stopping);
    assert_eq!(
        daemon.ended().await,
        Some(0),
        "{}",
        installed.standard_error()
    );
}

/// The daemon of `installed` started, MCP switched on by its person, and an
/// agent of theirs with `budget`, granted each of `features`.
async fn with_an_agent(
    installed: &Installed,
    budget: u32,
    features: &[&str],
) -> (Running, nc_ipc::Client, nc_ids::Snowflake) {
    let (mut daemon, person) = started(installed).await;
    daemon.switch_mcp(true).await;
    let agent = daemon.agent("assistant", budget).await;
    for feature in features {
        daemon.grant(agent, feature, 60).await;
    }
    (daemon, person, agent)
}

/// One decision of the journal, as a test reads it: whom it names, whom that
/// one acts for, the command, how it ended and what decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decided {
    principal: String,
    chain: Vec<String>,
    command: Option<String>,
    result: Outcome,
    deciding: String,
    grant: Option<String>,
}

/// Every decision the daemon's journal holds, in order, read from its own
/// database beside it while it runs.
async fn decided(installed: &Installed) -> Vec<Decided> {
    let store = nc_local::LocalStore::open(&installed.layout.data)
        .await
        .expect("the daemon's database opens beside it");
    let read = nc_journal::Store::from(&store.journal(), 0)
        .await
        .expect("the journal reads");
    store.close().await;
    read.entries
        .into_iter()
        .map(|entry| entry.record)
        .filter(|record| {
            matches!(
                record.result,
                Outcome::Allowed | Outcome::Denied | Outcome::Failed
            )
        })
        .map(|record| Decided {
            principal: record.principal.as_str().to_owned(),
            chain: record
                .actor_chain
                .iter()
                .map(|link| link.as_str().to_owned())
                .collect(),
            command: record
                .resource
                .clone()
                .into_object()
                .and_then(|mut fields| fields.remove("command"))
                .and_then(Value::into_text),
            result: record.result,
            deciding: record.deciding_policy.clone().unwrap_or_default(),
            grant: record
                .grant_used
                .as_ref()
                .map(|grant| grant.as_str().to_owned()),
        })
        .collect()
}

/// The decisions of `command` the journal holds as `agent`.
fn of_the_agent<'a>(
    decided: &'a [Decided],
    agent: nc_ids::Snowflake,
    command: &str,
) -> Vec<&'a Decided> {
    let agent = agent.to_crockford();
    decided
        .iter()
        .filter(|one| one.principal == agent && one.command.as_deref() == Some(command))
        .collect()
}

/// The codes of every `daemon.session_refused` the daemon wrote, in order.
fn session_refusals(installed: &Installed) -> Vec<String> {
    installed
        .standard_error()
        .lines()
        .filter(|line| line.contains("\"_msg\":\"daemon.session_refused\""))
        .filter_map(|line| {
            let value = nc_canon::parse(line.trim_start_matches("nc-log: ")).ok()?;
            value
                .into_object()?
                .remove("code")
                .and_then(Value::into_text)
        })
        .collect()
}

/// Each tool's name, as the daemon listed it.
fn names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|tool| {
            tool.clone()
                .into_object()
                .and_then(|mut fields| fields.remove("name"))
                .and_then(Value::into_text)
                .expect("a tool has a name")
        })
        .collect()
}

/// The tools `client` may dispatch, in `locale`.
async fn tools_of(client: &mut nc_ipc::Client, locale: Locale) -> Vec<Value> {
    match asked(client, Request::Tools { locale }).await {
        Answer::Tools(tools) => tools,
        other => panic!("the tools were answered with {other:?}"),
    }
}

/// MCP is off until the person switches it on, and a session ends once it is
/// off again.
///
/// **Off in a fresh daemon**: `settings.mcp.read` answers off, and a session
/// asked for the person's own agent is `NC-DMN-0014`, its connection closed
/// after. Switched on, a session opens and its agent's proposal is answered.
/// Switched off on another connection, the session's next request is
/// `NC-DMN-0014` and the one after finds its connection closed, and a new
/// session is refused; the person's undo of the off switches it on again, and
/// a new session opens. Each refusal is logged as `daemon.session_refused`
/// with its code.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_is_off_until_the_person_switches_it_on_and_a_session_ends_once_it_is_off_again() {
    let installed = Installed::new(Path::new(BINARY));
    installed.without_a_central_log();
    let (mut daemon, mut person) = started(&installed).await;
    match asked(&mut person, dispatch("settings.mcp.read", "{}")).await {
        Answer::Result(read) => assert_eq!(read.to_canonical(), r#"{"on":false}"#),
        other => panic!("the switch was read as {other:?}"),
    }
    let agent = daemon.agent("assistant", 100).await;
    daemon.grant(agent, "naming", 60).await;
    let (mut early, _) = daemon.connected().await;
    refused_with(
        &asked(&mut early, Request::Session { agent }).await,
        ErrorCode::DaemonMcpOff,
    );
    lost(&mut early, Request::Status).await;
    daemon.switch_mcp(true).await;
    let mut session = daemon.session(agent).await;
    assert!(matches!(
        asked(&mut session, propose()).await,
        Answer::Result(_)
    ));
    daemon.switch_mcp(false).await;
    refused_with(
        &asked(&mut session, propose()).await,
        ErrorCode::DaemonMcpOff,
    );
    lost(&mut session, propose()).await;
    let (mut again, _) = daemon.connected().await;
    refused_with(
        &asked(&mut again, Request::Session { agent }).await,
        ErrorCode::DaemonMcpOff,
    );
    assert!(matches!(
        asked(&mut person, dispatch("kernel.undo", "{}")).await,
        Answer::Result(_)
    ));
    let mut reopened = daemon.session(agent).await;
    assert!(matches!(
        asked(&mut reopened, propose()).await,
        Answer::Result(_)
    ));
    let switches: Vec<Decided> = decided(&installed)
        .await
        .into_iter()
        .filter(|one| one.command.as_deref() == Some("settings.mcp.set"))
        .collect();
    assert_eq!(switches.len(), 2, "{switches:?}");
    assert!(
        switches.iter().all(|one| one.principal == PERSON
            && one.chain.is_empty()
            && one.result == Outcome::Allowed
            && one.deciding == "family.a_member_switches_mcp"),
        "{switches:?}"
    );
    drop((early, session, again, reopened));
    stopped(&mut person, &mut daemon, &installed).await;
    let off = ErrorCode::DaemonMcpOff.code();
    assert_eq!(session_refusals(&installed), [off, off, off]);
}

/// A request refused before the kernel is logged with its code, and journals
/// nothing.
///
/// **Each of the four ways**, the daemon writing its lines to its standard
/// error alone: a session asked while MCP is off, one naming an agent no
/// person made, the registry asked on a session, and a session's request once
/// MCP was switched off under it — each logged as `daemon.session_refused`
/// with its code, in order, while the journal moves by the person's own two
/// switches alone and names no agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_refused_before_the_kernel_is_logged_with_its_code_and_journals_nothing() {
    let installed = Installed::new(Path::new(BINARY));
    installed.without_a_central_log();
    let (mut daemon, mut person) = started(&installed).await;
    let agent = daemon.agent("assistant", 100).await;
    daemon.grant(agent, "naming", 60).await;
    let before = decided(&installed).await.len();
    let (mut off, _) = daemon.connected().await;
    refused_with(
        &asked(&mut off, Request::Session { agent }).await,
        ErrorCode::DaemonMcpOff,
    );
    daemon.switch_mcp(true).await;
    let (mut stranger, _) = daemon.connected().await;
    refused_with(
        &asked(&mut stranger, Request::Session { agent: minted() }).await,
        ErrorCode::AiNoSuchAgent,
    );
    let mut session = daemon.session(agent).await;
    refused_with(
        &asked(&mut session, Request::Registry { locale: Locale::En }).await,
        ErrorCode::DaemonNotForASession,
    );
    daemon.switch_mcp(false).await;
    refused_with(&asked(&mut session, TOOLS).await, ErrorCode::DaemonMcpOff);
    let fresh: Vec<Decided> = decided(&installed).await.split_off(before);
    assert_eq!(fresh.len(), 2, "{fresh:?}");
    assert!(
        fresh.iter().all(
            |one| one.principal == PERSON && one.command.as_deref() == Some("settings.mcp.set")
        ),
        "{fresh:?}"
    );
    drop((off, stranger, session));
    stopped(&mut person, &mut daemon, &installed).await;
    assert_eq!(
        session_refusals(&installed),
        [
            ErrorCode::DaemonMcpOff.code(),
            ErrorCode::AiNoSuchAgent.code(),
            ErrorCode::DaemonNotForASession.code(),
            ErrorCode::DaemonMcpOff.code(),
        ]
    );
}

/// A session acts as its agent, and every dispatch of it is journaled with its
/// operator.
///
/// **The agent and whom it acts for, both**: the session's proposal is
/// journaled naming the agent, acting for the person, decided by one segment
/// for each, with the agent's own grant; its verification refused is
/// journaled the same way. The person's own proposal on a plain connection is
/// journaled as the person, acting for nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_acts_as_its_agent_and_every_dispatch_is_journaled_with_its_operator() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person) = started(&installed).await;
    daemon.switch_mcp(true).await;
    let agent = daemon.agent("assistant", 100).await;
    let grant = daemon.grant(agent, "naming", 60).await;
    let mut session = daemon.session(agent).await;
    assert!(matches!(
        asked(&mut session, propose()).await,
        Answer::Result(_)
    ));
    refused_with(
        &asked(&mut session, dispatch("journal.verify", r#"{"from":0}"#)).await,
        ErrorCode::PolicyDenied,
    );
    assert!(matches!(
        asked(&mut person, propose()).await,
        Answer::Result(_)
    ));
    let decided = decided(&installed).await;
    let proposed = of_the_agent(&decided, agent, "naming.propose");
    assert_eq!(proposed.len(), 1, "{decided:?}");
    assert_eq!(proposed[0].chain, [PERSON]);
    assert_eq!(proposed[0].result, Outcome::Allowed);
    assert_eq!(
        proposed[0].deciding.split(';').count(),
        2,
        "one segment for the agent and one for the person: {proposed:?}"
    );
    assert_eq!(proposed[0].grant.as_deref(), Some(grant.as_str()));
    let verified = of_the_agent(&decided, agent, "journal.verify");
    assert_eq!(verified.len(), 1, "{decided:?}");
    assert_eq!(
        (verified[0].chain.as_slice(), verified[0].result),
        ([PERSON.to_owned()].as_slice(), Outcome::Denied)
    );
    let the_persons: Vec<&Decided> = decided
        .iter()
        .filter(|one| one.principal == PERSON && one.command.as_deref() == Some("naming.propose"))
        .collect();
    assert_eq!(the_persons.len(), 1, "{decided:?}");
    assert!(the_persons[0].chain.is_empty(), "{the_persons:?}");
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A client of the daemon of `installed` whose next request is written by hand.
///
/// The handshake is made by the real ends, and the one request after it is
/// sent as `written` instead, through a relay between them.
async fn rewriting(daemon: &Running, installed: &Installed, written: Value) -> nc_ipc::Client {
    let (stream, _) = nc_platform::ipc::connect(&daemon.endpoint(), &daemon.user())
        .await
        .expect("the endpoint answers");
    let text = std::fs::read_to_string(&installed.layout.token).expect("the token is written");
    let token = nc_ipc::Token::from_text(&text).expect("a token");
    let (client_end, relay_end) = tokio::io::duplex(1 << 16);
    let (mut from_client, mut to_client) = tokio::io::split(relay_end);
    let (mut from_daemon, mut to_daemon) = tokio::io::split(stream);
    tokio::spawn(async move {
        // The hello is the first frame that is not a ping; the request after
        // it is the one written by hand.
        let mut said = 0_u32;
        while let Ok(Some(mut value)) = nc_ipc::frame::read(&mut from_client).await {
            let ping = matches!(&value, Value::Object(fields)
                if fields.get("type") == Some(&Value::text("ping")));
            if !ping {
                said += 1;
                if said == 2 {
                    value = written.clone();
                }
            }
            if nc_ipc::frame::write(&mut to_daemon, &value).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Ok(Some(value)) = nc_ipc::frame::read(&mut from_daemon).await {
            if nc_ipc::frame::write(&mut to_client, &value).await.is_err() {
                break;
            }
        }
        let _ = tokio::io::AsyncWriteExt::shutdown(&mut to_client).await;
    });
    nc_ipc::connect(client_end, &token)
        .await
        .expect("the daemon proves itself")
        .0
}

/// A session is opened only for an agent of the profile's person.
///
/// **An identifier no person made** is refused with `NC-AI-0001`, its
/// connection closed after it; and **one written as no identifier is**, in
/// lower case, is refused by the daemon's reader before anything is asked of
/// the kernel — the connection closed and logged as a message the protocol
/// does not read — while the person's own agent opens.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_is_opened_only_for_an_agent_of_the_profiles_person() {
    let installed = Installed::new(Path::new(BINARY));
    installed.without_a_central_log();
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let (mut stranger, _) = daemon.connected().await;
    refused_with(
        &asked(&mut stranger, Request::Session { agent: minted() }).await,
        ErrorCode::AiNoSuchAgent,
    );
    lost(&mut stranger, propose()).await;
    let lower = Value::object([
        ("agent", Value::text(agent.to_crockford().to_lowercase())),
        ("type", Value::text("session")),
    ]);
    let mut misspelt = rewriting(&daemon, &installed, lower).await;
    lost(&mut misspelt, Request::Session { agent }).await;
    let mut session = daemon.session(agent).await;
    assert!(matches!(
        asked(&mut session, propose()).await,
        Answer::Result(_)
    ));
    drop((stranger, misspelt, session));
    stopped(&mut person, &mut daemon, &installed).await;
    let malformed = format!("\"code\":\"{}\"", ErrorCode::IpcMalformed.code());
    let error = installed.standard_error();
    assert!(
        error
            .lines()
            .any(|line| line.contains("\"_msg\":\"ipc.refused\"") && line.contains(&malformed)),
        "{error}"
    );
    assert_eq!(
        session_refusals(&installed),
        [ErrorCode::AiNoSuchAgent.code()]
    );
}

/// An agent's session loses a feature the moment its person revokes it.
///
/// **End to end**: the session's proposal is answered; the person ends the
/// naming grant through the CLI, on a connection of its own; the session's
/// next proposal is refused, journaled as the agent acting for the person and
/// denied for want of a live grant.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_session_loses_a_feature_the_moment_its_person_revokes_it() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut session = daemon.session(agent).await;
    assert!(matches!(
        asked(&mut session, propose()).await,
        Answer::Result(_)
    ));
    let found = nc_channel::found::beside(&installed.executable)
        .expect("the CLI reads the install the daemon runs from");
    let argv: Vec<OsString> = [
        "agents.revoke".to_owned(),
        "--agent".to_owned(),
        agent.to_crockford(),
        "--feature".to_owned(),
        "naming".to_owned(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    let revoked =
        tokio::task::spawn_blocking(move || nc_cli::run(&argv, &found, &nc_cli::Env::of(|_| None)))
            .await
            .expect("the CLI ran");
    assert_eq!(revoked.exit, nc_cli::exit::Exit::Success, "{revoked:?}");
    refused_with(
        &asked(&mut session, propose()).await,
        ErrorCode::PolicyDenied,
    );
    let decided = decided(&installed).await;
    let proposed = of_the_agent(&decided, agent, "naming.propose");
    assert_eq!(proposed.len(), 2, "{decided:?}");
    assert_eq!(proposed[0].result, Outcome::Allowed);
    assert_eq!(proposed[1].result, Outcome::Denied);
    assert_eq!(proposed[1].chain, [PERSON]);
    assert!(
        proposed[1].deciding.contains(PROVENANCE),
        "{:?}",
        proposed[1]
    );
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// An agent is held to its own rights over the endpoint.
///
/// **What the person may, the agent may not, unless its own permit and grant
/// say so**: the journal's verification is answered on the person's
/// connection and refused to the session, whose agent holds no permit for it,
/// journaled as the agent acting for the person.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_is_held_to_its_own_rights_over_the_endpoint() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) =
        with_an_agent(&installed, 100, &["naming", "registry"]).await;
    let verify = || dispatch("journal.verify", r#"{"from":0}"#);
    assert!(matches!(
        asked(&mut person, verify()).await,
        Answer::Result(_)
    ));
    let mut session = daemon.session(agent).await;
    refused_with(
        &asked(&mut session, verify()).await,
        ErrorCode::PolicyDenied,
    );
    let decided = decided(&installed).await;
    let verified = of_the_agent(&decided, agent, "journal.verify");
    assert_eq!(verified.len(), 1, "{decided:?}");
    assert_eq!(
        (verified[0].chain.as_slice(), verified[0].result),
        ([PERSON.to_owned()].as_slice(), Outcome::Denied)
    );
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A session asks for its tools and dispatches commands, and for nothing else,
/// and stays bound.
///
/// **Refused and standing**: the registry, the status, a stop and a second
/// session asked on a session are each `NC-DMN-0015`; the daemon still runs,
/// its status answered on the person's connection; and the session's next
/// dispatch is journaled as the same agent, acting for the person.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_asks_for_its_tools_and_dispatches_and_for_nothing_else_and_stays_bound() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let other = daemon.agent("another", 100).await;
    let mut session = daemon.session(agent).await;
    for request in [
        Request::Registry { locale: Locale::En },
        Request::Status,
        Request::Stop,
        Request::Session { agent: other },
    ] {
        refused_with(
            &asked(&mut session, request).await,
            ErrorCode::DaemonNotForASession,
        );
    }
    match asked(&mut person, Request::Status).await {
        Answer::Status(status) => assert!(
            status.to_canonical().contains("\"state\":\"running\""),
            "{status:?}"
        ),
        other => panic!("the status was answered with {other:?}"),
    }
    assert!(matches!(
        asked(&mut session, propose()).await,
        Answer::Result(_)
    ));
    let decided = decided(&installed).await;
    let proposed = of_the_agent(&decided, agent, "naming.propose");
    assert_eq!(proposed.len(), 1, "{decided:?}");
    assert_eq!(proposed[0].chain, [PERSON]);
    assert!(
        of_the_agent(&decided, other, "naming.propose").is_empty(),
        "a second session rebound the connection: {decided:?}"
    );
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// What each answer the daemon gives holds: none is a confirmation, and none
/// issues one.
///
/// Exhaustive and with no wildcard, so an answer the wire gains is read for
/// what it carries before this compiles.
const fn carries(answer: &Answer) -> &'static str {
    match answer {
        Answer::Result(_) => "what a command answered",
        Answer::Refused(_) => "a code, a kind and a sentence",
        Answer::Registry(_) | Answer::Tools(_) => "commands' entries",
        Answer::Session | Answer::Stopping => "nothing",
        Answer::Status(_) => "what the daemon is",
    }
}

/// No request or argument of a session presents or obtains a confirmation.
///
/// **Nothing a session sends is read as one**: a reading of a name holding a
/// `confirmed_by_human` or a `_meta` beside its arguments is refused by the
/// command's schema (`NC-VAL-0002`) before anything runs; and no answer the
/// session is given carries a confirmation to present later.
#[tokio::test(flavor = "multi_thread")]
async fn no_request_or_argument_of_a_session_presents_or_obtains_a_confirmation() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut session = daemon.session(agent).await;
    let mut answers = Vec::new();
    for args in [
        r#"{"confirmed_by_human":true,"name":"a"}"#,
        r#"{"_meta":{},"name":"a"}"#,
    ] {
        let answer = asked(&mut session, dispatch("naming.read", args)).await;
        refused_with(&answer, ErrorCode::ValidationArgumentsInvalid);
        answers.push(answer);
    }
    answers.push(asked(&mut session, propose()).await);
    answers.push(asked(&mut session, TOOLS).await);
    for answer in &answers {
        let carried = carries(answer);
        let held = match answer {
            Answer::Result(payload) => vec![payload.clone()],
            Answer::Registry(entries) | Answer::Tools(entries) => entries.clone(),
            Answer::Refused(_) | Answer::Session | Answer::Status(_) | Answer::Stopping => {
                Vec::new()
            }
        };
        let fields: Vec<String> = held.iter().flat_map(fields_of).collect();
        assert!(
            fields
                .iter()
                .all(|field| !["confirmation", "approval", "token"]
                    .iter()
                    .any(|token| field.contains(token))),
            "{carried}: {fields:?}"
        );
    }
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// Every field name `value` holds, at any depth.
fn fields_of(value: &Value) -> Vec<String> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(name, inner)| core::iter::once(name.clone()).chain(fields_of(inner)))
            .collect(),
        Value::Array(items) => items.iter().flat_map(fields_of).collect(),
        _ => Vec::new(),
    }
}

/// Each command whose resource is known without its arguments, with
/// arguments its schema takes: the kernel's own, which a plain connection's
/// registry selects by the realm or a feature.
const FITTING: &[(&str, &str)] = &[
    ("agents.create", r#"{"budget":1,"label":"a"}"#),
    (
        "agents.grant",
        r#"{"agent":"0000000000000","feature":"naming","minutes":1,"reason":"a"}"#,
    ),
    ("agents.list", "{}"),
    ("agents.revoke", r#"{"agent":"0000000000000"}"#),
    ("journal.verify", r#"{"from":0}"#),
    ("kernel.find", r#"{"query":"a"}"#),
    ("kernel.redo", "{}"),
    ("kernel.undo", "{}"),
    ("limits.prune", r#"{"dry_run":true}"#),
    ("naming.propose", r#"{"title":"a"}"#),
    ("naming.read", r#"{"name":"a"}"#),
    ("settings.mcp.read", "{}"),
    ("settings.mcp.set", r#"{"on":true}"#),
];

/// The tools of a session are exactly what its agent may dispatch.
///
/// **Never vacuous**: the session's tools hold the same names in both
/// locales, each entry's texts in its own; [`FITTING`] is held both ways to
/// the commands a plain connection's registry selects by the realm or a
/// feature; every row is dispatched as the session, with arguments its schema
/// takes, and a command is listed exactly when its entry says the decision
/// allowed it — `allowed`, or `failed` once allowed — none refused for its
/// arguments. The session's tools are among the person's.
#[tokio::test(flavor = "multi_thread")]
async fn the_tools_of_a_session_are_exactly_what_its_agent_may_dispatch() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) =
        with_an_agent(&installed, 100, &["naming", "registry"]).await;
    let Answer::Registry(registry) =
        asked(&mut person, Request::Registry { locale: Locale::En }).await
    else {
        panic!("the registry was not answered");
    };
    let selected: BTreeSet<String> = registry
        .iter()
        .filter(|entry| {
            Value::clone(entry)
                .into_object()
                .and_then(|mut fields| fields.remove("resource_selector"))
                .and_then(Value::into_text)
                .is_some_and(|selector| selector == "realm" || selector.starts_with("feature:"))
        })
        .map(|entry| names(core::slice::from_ref(entry)).remove(0))
        .collect();
    let fitting: BTreeSet<String> = FITTING.iter().map(|(name, _)| (*name).to_owned()).collect();
    assert_eq!(fitting, selected, "`FITTING` is the registry's, both ways");
    let mut session = daemon.session(agent).await;
    let english = tools_of(&mut session, Locale::En).await;
    let portuguese = tools_of(&mut session, Locale::PtBR).await;
    let listed = names(&english);
    assert_eq!(listed, names(&portuguese));
    assert_eq!(listed, ["kernel.find", "naming.propose", "naming.read"]);
    for (one, other) in english.iter().zip(&portuguese) {
        assert_ne!(one, other, "the texts are the same in both locales");
    }
    let persons = names(&tools_of(&mut person, Locale::En).await);
    assert!(
        listed.iter().all(|name| persons.contains(name)),
        "the session lists what its person may not: {listed:?}, {persons:?}"
    );
    for (name, args) in FITTING {
        let _decided_and_journaled = asked(&mut session, dispatch(name, args)).await;
    }
    let decided = decided(&installed).await;
    for (name, _) in FITTING {
        let own = of_the_agent(&decided, agent, name);
        assert_eq!(own.len(), 1, "{name}: {decided:?}");
        assert_eq!(own[0].chain, [PERSON], "{name}");
        assert_ne!(
            own[0].deciding, "kernel.malformed_arguments",
            "{name} was refused for its arguments"
        );
        let allowed = matches!(own[0].result, Outcome::Allowed | Outcome::Failed);
        assert_eq!(
            listed.iter().any(|listed| listed == name),
            allowed,
            "{name}: {:?}",
            own[0]
        );
    }
    drop(session);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A session's budget is its own, and once spent it is denied by the decision
/// point.
///
/// **Three units for an agent made with three**: three proposals are answered
/// and the fourth refused (`NC-POL-0003`), journaled as denied by the core's
/// budget rule; a second session of the same agent, open beside it, counts
/// from nothing and is answered.
#[tokio::test(flavor = "multi_thread")]
async fn a_sessions_budget_is_its_own_and_once_spent_is_denied_by_the_decision_point() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 3, &["naming"]).await;
    let mut session = daemon.session(agent).await;
    let mut second = daemon.session(agent).await;
    for _ in 0..3 {
        assert!(matches!(
            asked(&mut session, propose()).await,
            Answer::Result(_)
        ));
    }
    refused_with(
        &asked(&mut session, propose()).await,
        ErrorCode::PolicyDenied,
    );
    assert!(matches!(
        asked(&mut second, propose()).await,
        Answer::Result(_)
    ));
    let decided = decided(&installed).await;
    let proposed = of_the_agent(&decided, agent, "naming.propose");
    assert_eq!(proposed.len(), 5, "{decided:?}");
    let denied: Vec<&&Decided> = proposed
        .iter()
        .filter(|one| one.result == Outcome::Denied)
        .collect();
    assert_eq!(denied.len(), 1, "{proposed:?}");
    assert!(denied[0].deciding.contains(BUDGET), "{denied:?}");
    drop((session, second));
    stopped(&mut person, &mut daemon, &installed).await;
}

/// Sessions past their bound are refused, and a closed one's place is taken
/// again.
///
/// **`sessions_max` open at once**: the next is refused with `NC-DMN-0016`
/// and its connection closed, while the person's connection still answers;
/// once one session's connection is closed, a new session opens in its place.
#[tokio::test(flavor = "multi_thread")]
async fn sessions_past_their_bound_are_refused_and_a_closed_ones_place_is_taken_again() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let most = usize::try_from(nc_catalog::DAEMON.sessions_max).expect("a count");
    let mut open = Vec::with_capacity(most);
    for _ in 0..most {
        open.push(daemon.session(agent).await);
    }
    let (mut past, _) = daemon.connected().await;
    refused_with(
        &asked(&mut past, Request::Session { agent }).await,
        ErrorCode::DaemonTooManySessions,
    );
    lost(&mut past, Request::Status).await;
    assert!(matches!(
        asked(&mut person, Request::Status).await,
        Answer::Status(_)
    ));
    drop(open.pop());
    let deadline = tokio::time::Instant::now() + PATIENCE;
    let mut again = loop {
        let (mut client, _) = daemon.connected().await;
        match asked(&mut client, Request::Session { agent }).await {
            Answer::Session => break client,
            Answer::Refused(refusal) if refusal.code == ErrorCode::DaemonTooManySessions => {}
            other => panic!("a session was answered with {other:?}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no place was given back within {PATIENCE:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(matches!(
        asked(&mut again, propose()).await,
        Answer::Result(_)
    ));
    drop((open, again));
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A restarted daemon keeps the setting and the agent, and ends every session.
///
/// **Nothing of a session outlives its daemon**: the session's connection is
/// lost with the stop; the next start still has MCP on and still holds the
/// agent and its grant, and the same agent's session opens again with a new
/// count — its budget of two spent before the stop, and two more answered
/// after.
#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_daemon_keeps_the_setting_and_the_agent_and_ends_every_session() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 2, &["naming"]).await;
    let mut session = daemon.session(agent).await;
    for _ in 0..2 {
        assert!(matches!(
            asked(&mut session, propose()).await,
            Answer::Result(_)
        ));
    }
    refused_with(
        &asked(&mut session, propose()).await,
        ErrorCode::PolicyDenied,
    );
    stopped(&mut person, &mut daemon, &installed).await;
    let ended = tokio::time::timeout(PATIENCE, session.ask(&propose()))
        .await
        .expect("answered or lost in time");
    assert!(ended.is_err(), "a session outlived its daemon: {ended:?}");
    let (mut again, mut person) = started(&installed).await;
    match asked(&mut person, dispatch("settings.mcp.read", "{}")).await {
        Answer::Result(read) => assert_eq!(read.to_canonical(), r#"{"on":true}"#),
        other => panic!("the switch was read as {other:?}"),
    }
    let mut session = again.session(agent).await;
    for _ in 0..2 {
        assert!(matches!(
            asked(&mut session, propose()).await,
            Answer::Result(_)
        ));
    }
    drop(session);
    stopped(&mut person, &mut again, &installed).await;
}

/// A connection that opens no session is the person, as `NOCTUA.md §11.1`
/// says.
///
/// **The bypass, shown so the claim written about it is honest**: with MCP on
/// and the agent's session open beside it, a plain connection to the same
/// endpoint switches MCP and verifies the journal as the person, acting for
/// nobody — each of which the session is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_opens_no_session_is_the_person_as_section_eleven_one_says() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut session = daemon.session(agent).await;
    let (mut plain, _) = daemon.connected().await;
    for (command, args) in [
        ("journal.verify", r#"{"from":0}"#),
        ("settings.mcp.set", r#"{"on":true}"#),
    ] {
        refused_with(
            &asked(&mut session, dispatch(command, args)).await,
            ErrorCode::PolicyDenied,
        );
        assert!(
            matches!(
                asked(&mut plain, dispatch(command, args)).await,
                Answer::Result(_)
            ),
            "{command}"
        );
    }
    let decided = decided(&installed).await;
    for command in ["journal.verify", "settings.mcp.set"] {
        assert!(
            decided.iter().any(|one| one.principal == PERSON
                && one.chain.is_empty()
                && one.command.as_deref() == Some(command)
                && one.result == Outcome::Allowed),
            "{command}: {decided:?}"
        );
        assert_eq!(of_the_agent(&decided, agent, command).len(), 1, "{command}");
    }
    drop((session, plain));
    stopped(&mut person, &mut daemon, &installed).await;
}
