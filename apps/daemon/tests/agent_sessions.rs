//! An agent's session at a real daemon's endpoint (`NOCTUA.md §20.3`): opened
//! only while MCP is on, for an agent the profile's person made and within
//! `sessions_max`; acting as that agent for that person, every dispatch
//! decided for both and journaled with both; asking for its tools and
//! dispatching commands and nothing else; ended once MCP is off; and what is
//! refused with no command dispatched logged by its code and never journaled.
//!
//! **And `noctua mcp <agent>` serving an MCP client against it**: an in-house
//! client written from the protocol's `2025-11-25` schema drives
//! `nc_cli::run_with` on a thread of its own, over two pipes the system makes,
//! as a host drives the binary over its standard streams — off until MCP is
//! switched on, its tools what its agent may dispatch in the environment's
//! language, every call it sends journaled as the agent's, no field of any
//! message a confirmation, and its session kept while it is idle and ended
//! with its daemon.
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

/// The `field` of every `event` the daemon wrote, in order.
fn logged(installed: &Installed, event: &str, field: &str) -> Vec<String> {
    let named = format!("\"_msg\":\"{event}\"");
    installed
        .standard_error()
        .lines()
        .filter(|line| line.contains(&named))
        .filter_map(|line| {
            let value = nc_canon::parse(line.trim_start_matches("nc-log: ")).ok()?;
            value
                .into_object()?
                .remove(field)
                .and_then(Value::into_text)
        })
        .collect()
}

/// The codes of every `daemon.session_refused` the daemon wrote, in order.
fn session_refusals(installed: &Installed) -> Vec<String> {
    logged(installed, "daemon.session_refused", "code")
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

/// A request refused with no command dispatched is logged with its code,
/// and journals nothing.
///
/// **Each of the four ways**, the daemon writing its lines to its standard
/// error alone: a session asked while MCP is off, one naming an agent no
/// person made, the registry asked on a session, and a session's request once
/// MCP was switched off under it — each logged as `daemon.session_refused`
/// with its code, in order, while the journal moves by the person's own two
/// switches alone and names no agent. The one session that opened is logged
/// once as opened and once as ended, naming its agent, and the stranger's is
/// never logged as opened.
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
    let unknown = minted();
    let (mut stranger, _) = daemon.connected().await;
    refused_with(
        &asked(&mut stranger, Request::Session { agent: unknown }).await,
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
    let agent = agent.to_crockford();
    let opened = logged(&installed, "daemon.session_opened", "agent");
    assert!(!opened.contains(&unknown.to_crockford()), "{opened:?}");
    assert_eq!(opened, [agent.as_str()]);
    assert_eq!(
        logged(&installed, "daemon.session_ended", "agent"),
        [agent.as_str()]
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

/// What the person's own CLI ends in, run in-process with `words` against the
/// daemon of `installed`, as they would type it.
async fn by_the_person(installed: &Installed, words: &[&str]) -> nc_cli::Outcome {
    let found = nc_channel::found::beside(&installed.executable)
        .expect("the CLI reads the install the daemon runs from");
    let argv: Vec<OsString> = words.iter().map(OsString::from).collect();
    tokio::task::spawn_blocking(move || nc_cli::run(&argv, &found, &nc_cli::Env::of(|_| None)))
        .await
        .expect("the CLI ran")
}

/// The environment's language as Brazilian Portuguese.
fn in_portuguese() -> nc_cli::Env {
    nc_cli::Env::of(|name| (name == "LANG").then(|| "pt_BR.UTF-8".to_owned()))
}

/// `value`'s field `key`, when it is an object holding one.
fn at<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.get(key),
        _ => None,
    }
}

/// The text `value` holds, when it is text.
fn text_of(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::Text(text)) => Some(text),
        _ => None,
    }
}

/// What a result `holds` is shaped as, by the protocol's `2025-11-25` schema.
#[derive(Debug, Clone, Copy)]
enum Holds {
    /// `InitializeResult`: the revision, the capabilities and the server's
    /// name and version.
    Initialized,
    /// `ListToolsResult`: the tools, each a name and an object's input schema.
    Tools,
    /// `CallToolResult`: the content, each a block of text here, and whether
    /// it is an error.
    Called,
}

/// That `result` is what `holds` says, field by field as the schema writes
/// it: what this client reads of a result, and nothing it does not.
fn shaped(result: &Value, holds: Holds) {
    let Value::Object(fields) = result else {
        panic!("{result:?} is not an object");
    };
    match holds {
        Holds::Initialized => {
            assert!(
                text_of(fields.get("protocolVersion")).is_some(),
                "{result:?}"
            );
            assert!(matches!(fields.get("capabilities"), Some(Value::Object(_))));
            let server = fields.get("serverInfo").expect("the server's own");
            assert!(text_of(at(server, "name")).is_some(), "{result:?}");
            assert!(text_of(at(server, "version")).is_some(), "{result:?}");
        }
        Holds::Tools => {
            let Some(Value::Array(tools)) = fields.get("tools") else {
                panic!("{result:?} lists no tools");
            };
            for tool in tools {
                assert!(text_of(at(tool, "name")).is_some(), "{tool:?}");
                let schema = at(tool, "inputSchema").expect("an input schema");
                assert_eq!(text_of(at(schema, "type")), Some("object"), "{tool:?}");
                if let Some(described) = at(tool, "description") {
                    assert!(matches!(described, Value::Text(_)), "{tool:?}");
                }
            }
        }
        Holds::Called => {
            let Some(Value::Array(content)) = fields.get("content") else {
                panic!("{result:?} holds no content");
            };
            for block in content {
                assert_eq!(text_of(at(block, "type")), Some("text"), "{block:?}");
                assert!(text_of(at(block, "text")).is_some(), "{block:?}");
            }
            if let Some(error) = fields.get("isError") {
                assert!(matches!(error, Value::Bool(_)), "{result:?}");
            }
        }
    }
}

/// An MCP client, written from the protocol's `2025-11-25` schema, of the
/// server `noctua mcp <agent>` runs, served in this process.
///
/// **As a host runs the binary**: `nc_cli::run_with` on a thread of its own,
/// over two pipes the system makes, one the client writes its messages to and
/// one it reads the answers from, line by line, on a thread of its own; the
/// install is the one the daemon runs from, read as the CLI reads it. Every
/// line the server writes is kept, and each answer is held to the schema of
/// what it answers.
struct McpClient {
    /// The server's standard input: the client's messages, one a line.
    input: Option<std::io::PipeWriter>,
    /// The server's lines, as it writes them.
    lines: std::sync::mpsc::Receiver<String>,
    /// The server's run, which hands back how it ended.
    server: Option<std::thread::JoinHandle<nc_cli::Outcome>>,
    /// The id the next request is sent with.
    next: i64,
    /// Every line the server wrote, read.
    written: Vec<Value>,
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.input.take());
        let deadline = std::time::Instant::now() + PATIENCE;
        if let Some(server) = self.server.take() {
            while !server.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if server.is_finished() {
                let _ = server.join();
            }
        }
    }
}

impl McpClient {
    /// `noctua mcp <agent>` served against the daemon of `installed`, in the
    /// language `env` names.
    fn serve(installed: &Installed, agent: nc_ids::Snowflake, env: nc_cli::Env) -> Self {
        let found = nc_channel::found::beside(&installed.executable)
            .expect("the CLI reads the install the daemon runs from");
        let (server_in, input) = std::io::pipe().expect("a pipe");
        let (output, server_out) = std::io::pipe().expect("a pipe");
        let argv = [OsString::from("mcp"), OsString::from(agent.to_crockford())];
        let server = std::thread::spawn(move || {
            let streams = nc_cli::mcp::Streams::new(server_in, server_out);
            nc_cli::run_with(&argv, &found, &env, streams)
        });
        let (sent, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let read = std::io::BufReader::new(output);
            for line in std::io::BufRead::lines(read) {
                let Ok(line) = line else { break };
                if sent.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            input: Some(input),
            lines,
            server: Some(server),
            next: 1,
            written: Vec::new(),
        }
    }

    /// Writes `line` to the server.
    fn write(&mut self, line: &str) {
        let input = self.input.as_mut().expect("the input is open");
        std::io::Write::write_all(input, format!("{line}\n").as_bytes()).expect("written");
    }

    /// The next line the server writes, read, within [`PATIENCE`].
    fn read(&mut self) -> Value {
        let line = tokio::task::block_in_place(|| self.lines.recv_timeout(PATIENCE))
            .expect("the server answered in time");
        assert!(!line.contains('\r'), "{line}");
        let read = nc_canon::parse(&line).expect("one JSON value a line");
        self.written.push(read.clone());
        read
    }

    /// The response to `method` with `params`, sent as the next request.
    ///
    /// **One response to it**: `jsonrpc` 2.0 and the id sent, holding a
    /// result or an error, never both, and an error held to the schema's
    /// shape.
    fn ask(&mut self, method: &str, params: &str) -> Value {
        let id = self.next;
        self.next += 1;
        self.write(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#
        ));
        let answered = self.read();
        assert_eq!(
            text_of(at(&answered, "jsonrpc")),
            Some("2.0"),
            "{answered:?}"
        );
        assert_eq!(
            at(&answered, "id"),
            Some(&Value::Integer(id)),
            "{answered:?}"
        );
        assert!(at(&answered, "method").is_none(), "{answered:?}");
        match (at(&answered, "result"), at(&answered, "error")) {
            (Some(_), None) => {}
            (None, Some(error)) => {
                assert!(
                    matches!(at(error, "code"), Some(Value::Integer(_))),
                    "{error:?}"
                );
                assert!(text_of(at(error, "message")).is_some(), "{error:?}");
            }
            _ => panic!("{answered:?} is not one response"),
        }
        answered
    }

    /// `initialize`, asking for the newest revision, and its response; an
    /// opened session's notifications/initialized sent after it.
    fn initialize(&mut self, capabilities: &str) -> Value {
        let params = format!(
            r#"{{"protocolVersion":"2025-11-25","capabilities":{capabilities},"clientInfo":{{"name":"the suite's client","version":"1"}}}}"#
        );
        let answered = self.ask("initialize", &params);
        if let Some(result) = at(&answered, "result") {
            shaped(result, Holds::Initialized);
            self.write(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        }
        answered
    }

    /// The tools the session lists now.
    fn tools(&mut self) -> Vec<Value> {
        let answered = self.ask("tools/list", "{}");
        let result = at(&answered, "result").expect("the tools are listed");
        shaped(result, Holds::Tools);
        match at(result, "tools") {
            Some(Value::Array(tools)) => tools.clone(),
            other => panic!("the tools were {other:?}"),
        }
    }

    /// The response to a call of `tool` with `arguments`, written as JSON:
    /// its result held to the schema's shape.
    fn call(&mut self, tool: &str, arguments: &str) -> Value {
        let answered = self.ask(
            "tools/call",
            &format!(r#"{{"name":"{tool}","arguments":{arguments}}}"#),
        );
        if let Some(result) = at(&answered, "result") {
            shaped(result, Holds::Called);
        }
        answered
    }

    /// How the server ended once it closed the session on its own, nothing
    /// more written.
    fn left(mut self) -> nc_cli::Outcome {
        let server = self.server.take().expect("the server runs");
        let outcome = tokio::task::block_in_place(|| server.join()).expect("the server ran");
        let after = self.lines.recv_timeout(Duration::from_millis(200));
        assert!(after.is_err(), "the server wrote after it ended: {after:?}");
        outcome
    }

    /// How the server ended once its input did.
    fn ended(mut self) -> nc_cli::Outcome {
        drop(self.input.take());
        self.left()
    }
}

/// The text a call's result carries, read back as the JSON it is.
fn answered_by(called: &Value) -> Value {
    let result = at(called, "result").expect("a result");
    assert!(
        !matches!(at(result, "isError"), Some(Value::Bool(true))),
        "{called:?}"
    );
    let Some(Value::Array(content)) = at(result, "content") else {
        panic!("{called:?} holds no content");
    };
    let text = text_of(content.first().and_then(|block| at(block, "text")));
    nc_canon::parse(text.expect("a block of text")).expect("the answer's canonical text")
}

/// Each tool's name, as a client was handed it.
fn tool_names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .map(|tool| text_of(at(tool, "name")).expect("a name").to_owned())
        .collect()
}

/// `answered`'s error code and the catalog code in its data, when it is an
/// error.
fn error_of(answered: &Value) -> (Option<i64>, Option<String>) {
    let error = at(answered, "error");
    let code = match error.and_then(|error| at(error, "code")) {
        Some(Value::Integer(code)) => Some(*code),
        _ => None,
    };
    let catalog = text_of(
        error
            .and_then(|error| at(error, "data"))
            .and_then(|data| at(data, "code")),
    )
    .map(str::to_owned);
    (code, catalog)
}

/// MCP is off until it is switched on, for an MCP client.
///
/// **A fresh daemon refuses the client's session**: `initialize` is one
/// `-32603` carrying `NC-DMN-0014` and exit 3, and the server ends in 3 with
/// the refusal on standard error, having listed nothing. Switched on through
/// the person's own CLI, the same client's session opens and lists the tools
/// its agent was granted.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_is_off_until_it_is_switched_on_for_an_mcp_client() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person) = started(&installed).await;
    let agent = daemon.agent("assistant", 100).await;
    daemon.grant(agent, "naming", 60).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    let refused = client.initialize("{}");
    let off = ErrorCode::DaemonMcpOff.code();
    assert_eq!(error_of(&refused), (Some(-32_603), Some(off.to_owned())));
    assert_eq!(
        at(
            at(at(&refused, "error").expect("an error"), "data").expect("data"),
            "exit"
        ),
        Some(&Value::Integer(3))
    );
    let written = client.written.len();
    let outcome = client.left();
    assert_eq!(written, 1, "the server wrote more than its refusal");
    assert_eq!(outcome.exit, nc_cli::exit::Exit::Denied, "{outcome:?}");
    assert_eq!(outcome.stdout, "");
    assert!(
        outcome.stderr.starts_with(&format!("error[{off}]")),
        "{outcome:?}"
    );
    let switched = by_the_person(&installed, &["settings.mcp.set", "--on"]).await;
    assert_eq!(switched.exit, nc_cli::exit::Exit::Success, "{switched:?}");
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    assert_eq!(
        tool_names(&client.tools()),
        ["naming_propose", "naming_read"]
    );
    let outcome = client.ended();
    assert_eq!(outcome.exit, nc_cli::exit::Exit::Success, "{outcome:?}");
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A client lists and calls `naming_propose` and `naming_read`, and the name
/// reads back as it was proposed.
///
/// **Stage 14's round trip, over MCP**: each tool listed reads back through
/// the command-name parser to a command; the name proposed for a title and
/// two keywords is read back by `naming_read` to the same identifier, title
/// and keywords, already canonical.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_lists_and_calls_naming_propose_and_naming_read_and_the_name_reads_back_as_proposed()
 {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    let names = tool_names(&client.tools());
    assert_eq!(names, ["naming_propose", "naming_read"]);
    for name in &names {
        let command = nc_schema::entry::command_of_tool(name).expect("a command's tool");
        assert!(
            nc_schema::command_name_is_well_formed(&command),
            "{command}"
        );
    }
    let proposed = answered_by(&client.call(
        "naming_propose",
        r#"{"title":"a report","keywords":["draft","q3"]}"#,
    ));
    let name = text_of(at(&proposed, "name")).expect("a name").to_owned();
    let read = answered_by(&client.call("naming_read", &format!(r#"{{"name":"{name}"}}"#)));
    for field in ["id", "keywords", "title", "name"] {
        assert_eq!(at(&read, field), at(&proposed, field), "{field}: {read:?}");
    }
    assert_eq!(at(&read, "canonical"), Some(&Value::Bool(true)));
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// The tools a session lists are the commands it may dispatch, in the
/// environment's language.
///
/// **Held to the daemon's own answer, and to the grants**: in each language,
/// the client's tools, read back to commands, are what a second session of
/// the same agent is answered when it asks for its tools over the wire — the
/// commands its grants of naming and the registry reach — each described by
/// that answer's line and paragraph in that language, which differ between
/// the two.
#[tokio::test(flavor = "multi_thread")]
async fn the_tools_a_session_lists_are_the_commands_it_may_dispatch_in_the_environments_language() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) =
        with_an_agent(&installed, 100, &["naming", "registry"]).await;
    let mut second = daemon.session(agent).await;
    let mut described = Vec::new();
    for (locale, env) in [
        (Locale::En, nc_cli::Env::of(|_| None)),
        (Locale::PtBR, in_portuguese()),
    ] {
        let answered = tools_of(&mut second, locale).await;
        let mut client = McpClient::serve(&installed, agent, env);
        assert!(at(&client.initialize("{}"), "result").is_some());
        let tools = client.tools();
        let commands: Vec<String> = tool_names(&tools)
            .iter()
            .map(|name| nc_schema::entry::command_of_tool(name).expect("a command's tool"))
            .collect();
        assert_eq!(commands, names(&answered), "{locale:?}");
        assert_eq!(commands, ["kernel.find", "naming.propose", "naming.read"]);
        for (tool, entry) in tools.iter().zip(&answered) {
            let said = format!(
                "{}\n\n{}",
                text_of(at(entry, "line")).expect("a line"),
                text_of(at(entry, "paragraph")).expect("a paragraph")
            );
            assert_eq!(text_of(at(tool, "description")), Some(said.as_str()));
        }
        described.push(
            tools
                .iter()
                .map(|tool| {
                    text_of(at(tool, "description"))
                        .unwrap_or_default()
                        .to_owned()
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    }
    for (english, portuguese) in described[0].iter().zip(&described[1]) {
        assert_ne!(english, portuguese);
    }
    drop(second);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// Every call from a client is journaled as its session's agent.
///
/// **One entry a call, naming the agent and the person it acts for**: a
/// proposal and a reading answered, decided for both, and a verification of
/// the journal the agent may not make — not listed, still sent, read by the
/// client as an unknown tool — denied by the agent's half; nothing else of
/// the agent's is journaled, neither the opening nor the lists.
#[tokio::test(flavor = "multi_thread")]
async fn every_call_from_a_client_is_journaled_as_its_sessions_agent() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    let _ = client.tools();
    let proposed = answered_by(&client.call("naming_propose", r#"{"title":"a"}"#));
    let name = text_of(at(&proposed, "name")).expect("a name").to_owned();
    let _ = answered_by(&client.call("naming_read", &format!(r#"{{"name":"{name}"}}"#)));
    let verified = client.call("journal_verify", r#"{"from":0}"#);
    assert_eq!(error_of(&verified).0, Some(-32_602), "{verified:?}");
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    let decided = decided(&installed).await;
    let agents: Vec<&Decided> = decided
        .iter()
        .filter(|one| one.principal == agent.to_crockford())
        .collect();
    assert_eq!(agents.len(), 3, "{decided:?}");
    for (command, result) in [
        ("naming.propose", Outcome::Allowed),
        ("naming.read", Outcome::Allowed),
        ("journal.verify", Outcome::Denied),
    ] {
        let own = of_the_agent(&decided, agent, command);
        assert_eq!(own.len(), 1, "{command}: {decided:?}");
        assert_eq!(
            (own[0].chain.as_slice(), own[0].result),
            ([PERSON.to_owned()].as_slice(), result)
        );
        // Allowed for each link, or denied by the first that refused.
        let links = if result == Outcome::Allowed { 2 } else { 1 };
        assert_eq!(
            own[0].deciding.split(';').count(),
            links,
            "{command}: {own:?}"
        );
    }
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A revoked or spent session that lists again is still journaled for every
/// call.
///
/// **Its list empty, its call still sent**: once the person revokes the
/// agent's naming through the CLI, the client lists nothing and its reading
/// is an unknown tool to it, and journaled denied for want of a live grant,
/// acting for the person; an agent made with one unit proposes once, lists
/// nothing, and its second proposal is an unknown tool to it, journaled
/// denied by the budget's rule.
#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_or_spent_session_that_lists_again_is_still_journaled_for_every_call() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    assert_eq!(client.tools().len(), 2);
    let _ = answered_by(&client.call("naming_propose", r#"{"title":"a"}"#));
    let agent_text = agent.to_crockford();
    let revoked = by_the_person(
        &installed,
        &[
            "agents.revoke",
            "--agent",
            &agent_text,
            "--feature",
            "naming",
        ],
    )
    .await;
    assert_eq!(revoked.exit, nc_cli::exit::Exit::Success, "{revoked:?}");
    assert_eq!(client.tools(), [] as [Value; 0]);
    let read = client.call("naming_read", r#"{"name":"a"}"#);
    assert_eq!(error_of(&read).0, Some(-32_602), "{read:?}");
    assert_eq!(
        text_of(at(at(&read, "error").expect("an error"), "message")),
        Some("Unknown tool: naming_read")
    );
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    let spent = daemon.agent("spent", 1).await;
    daemon.grant(spent, "naming", 60).await;
    let mut client = McpClient::serve(&installed, spent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    assert_eq!(client.tools().len(), 2);
    let _ = answered_by(&client.call("naming_propose", r#"{"title":"a"}"#));
    assert_eq!(client.tools(), [] as [Value; 0]);
    let again = client.call("naming_propose", r#"{"title":"b"}"#);
    assert_eq!(error_of(&again).0, Some(-32_602), "{again:?}");
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    let decided = decided(&installed).await;
    let read = of_the_agent(&decided, agent, "naming.read");
    assert_eq!(read.len(), 1, "{decided:?}");
    assert_eq!(
        (read[0].chain.as_slice(), read[0].result),
        ([PERSON.to_owned()].as_slice(), Outcome::Denied)
    );
    assert!(read[0].deciding.contains(PROVENANCE), "{read:?}");
    let proposed = of_the_agent(&decided, spent, "naming.propose");
    assert_eq!(proposed.len(), 2, "{decided:?}");
    assert_eq!(proposed[0].result, Outcome::Allowed);
    assert_eq!(proposed[1].result, Outcome::Denied);
    assert_eq!(proposed[1].chain, [PERSON]);
    assert!(proposed[1].deciding.contains(BUDGET), "{proposed:?}");
    stopped(&mut person, &mut daemon, &installed).await;
}

/// No field any message can carry obtains or redeems a confirmation.
///
/// **Whatever the client claims**: an `initialize` offering every capability
/// that lets a server ask; calls whose `_meta` forges a confirmation, an
/// approval and a host's own key, which run as without it; calls with a
/// task, input responses or a request state, refused before anything is
/// sent; responses to requests the server never made; arguments holding
/// `confirmed_by_human`, refused by the command's schema; and an undo, which
/// the agent may not, read as an unknown tool. The server wrote nothing but
/// responses to the ids sent; the agent's entries hold no command allowed but
/// the two reads; and nothing the agent was listed cannot be undone.
#[tokio::test(flavor = "multi_thread")]
async fn no_field_any_message_can_carry_obtains_or_redeems_a_confirmation() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    let opened = client.initialize(
        r#"{"elicitation":{"form":{},"url":{}},"sampling":{"tools":{}},"roots":{"listChanged":true},"tasks":{"requests":{"elicitation":{"create":{}}}}}"#,
    );
    let capabilities = at(at(&opened, "result").expect("opened"), "capabilities").expect("some");
    assert_eq!(
        capabilities.to_canonical(),
        r#"{"tools":{"listChanged":false}}"#
    );
    let tools = client.tools();
    assert!(
        tools.iter().all(|tool| {
            let command =
                nc_schema::entry::command_of_tool(text_of(at(tool, "name")).unwrap_or_default());
            nc_catalog::COMMANDS
                .iter()
                .filter(|declared| Some(declared.name) == command.as_deref())
                .all(|declared| declared.reversibility != nc_catalog::Reversibility::Irreversible)
        }),
        "{tools:?}"
    );
    let proposed = answered_by(&client.call("naming_propose", r#"{"title":"a"}"#));
    let name = text_of(at(&proposed, "name")).expect("a name").to_owned();
    let forged = client.ask(
        "tools/call",
        &format!(
            r#"{{"name":"naming_read","arguments":{{"name":"{name}"}},"_meta":{{"confirmed":true,"confirmation":"granted","approval":{{"by":"the person"}},"host.example/requiresUserInteraction":false,"progressToken":1}}}}"#
        ),
    );
    let _ = answered_by(&forged);
    for field in [
        r#""task":{"ttl":60000}"#,
        r#""inputResponses":{"e1":{"action":"accept","content":{"confirmed":true}}}"#,
        r#""requestState":"confirmed""#,
    ] {
        let refused = client.ask(
            "tools/call",
            &format!(r#"{{"name":"naming_propose","arguments":{{"title":"b"}},{field}}}"#),
        );
        assert_eq!(error_of(&refused).0, Some(-32_602), "{field}: {refused:?}");
    }
    for response in [
        r#"{"jsonrpc":"2.0","id":"e1","result":{"action":"accept","content":{"confirmed":true}}}"#,
        r#"{"jsonrpc":"2.0","id":0,"result":{"model":"m","role":"assistant","content":{"type":"text","text":"yes"}}}"#,
    ] {
        client.write(response);
    }
    let unread = client.call("naming_read", r#"{"name":"a","confirmed_by_human":true}"#);
    let result = at(&unread, "result").expect("a tool error");
    assert_eq!(
        at(result, "isError"),
        Some(&Value::Bool(true)),
        "{unread:?}"
    );
    let undone = client.call("kernel_undo", "{}");
    assert_eq!(error_of(&undone).0, Some(-32_602), "{undone:?}");
    let written = client.written.clone();
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    let sent: Vec<i64> = (1..=9).collect();
    assert_eq!(written.len(), sent.len(), "{written:?}");
    for (line, id) in written.iter().zip(sent) {
        assert!(
            at(line, "method").is_none(),
            "a request of the server's: {line:?}"
        );
        assert_eq!(at(line, "id"), Some(&Value::Integer(id)), "{line:?}");
    }
    let decided = decided(&installed).await;
    let allowed: BTreeSet<&str> = decided
        .iter()
        .filter(|one| one.principal == agent.to_crockford() && one.result == Outcome::Allowed)
        .filter_map(|one| one.command.as_deref())
        .collect();
    assert_eq!(allowed, BTreeSet::from(["naming.propose", "naming.read"]));
    let undo = of_the_agent(&decided, agent, "kernel.undo");
    assert_eq!(undo.len(), 1, "{decided:?}");
    assert_eq!(undo[0].result, Outcome::Denied);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// An idle session keeps its connection past the daemon's silence bound.
///
/// **Read off the runtime's thread**: the client says nothing for longer than
/// the daemon waits on a silent connection, and its next list is answered,
/// because the heartbeat ran while the server waited for a line.
#[tokio::test(flavor = "multi_thread")]
async fn an_idle_session_keeps_its_connection_past_the_daemons_silence_bound() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, mut person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    let silent = nc_catalog::DAEMON.ipc_silent_after_ms + nc_catalog::DAEMON.ipc_heartbeat_ms;
    tokio::time::sleep(Duration::from_millis(silent)).await;
    assert_eq!(client.tools().len(), 2);
    assert_eq!(client.ended().exit, nc_cli::exit::Exit::Success);
    stopped(&mut person, &mut daemon, &installed).await;
}

/// A daemon stopped mid-session ends the session in nine.
///
/// **Through the person's own `stop`**: the client's next list is `-32603`,
/// carrying exit 9, and the server ends in 9 with the protocol's code on
/// standard error.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_stopped_mid_session_ends_the_session_in_nine() {
    let installed = Installed::new(Path::new(BINARY));
    let (mut daemon, person, agent) = with_an_agent(&installed, 100, &["naming"]).await;
    let mut client = McpClient::serve(&installed, agent, nc_cli::Env::of(|_| None));
    assert!(at(&client.initialize("{}"), "result").is_some());
    assert_eq!(client.tools().len(), 2);
    drop(person);
    let stop = by_the_person(&installed, &["stop"]).await;
    assert_eq!(stop.exit, nc_cli::exit::Exit::Success, "{stop:?}");
    assert_eq!(
        daemon.ended().await,
        Some(0),
        "{}",
        installed.standard_error()
    );
    let listed = client.ask("tools/list", "{}");
    assert_eq!(error_of(&listed).0, Some(-32_603), "{listed:?}");
    let exit = at(
        at(at(&listed, "error").expect("an error"), "data").expect("data"),
        "exit",
    );
    assert_eq!(exit, Some(&Value::Integer(9)), "{listed:?}");
    let outcome = client.left();
    assert_eq!(outcome.exit, nc_cli::exit::Exit::Unreachable, "{outcome:?}");
    assert!(outcome.stderr.starts_with("error[NC-IPC-"), "{outcome:?}");
}
