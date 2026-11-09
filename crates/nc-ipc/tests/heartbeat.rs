//! Each side pings after a heartbeat of its own silence and drops a peer
//! silent past the bound; one request is answered at a time, and an answer
//! goes only to the request that asked for it.
//!
//! **On a paused clock**, so a silence of seconds costs nothing and is
//! measured exactly. **A relay stands between a real client and a real
//! daemon**, forwarding every frame and, when a test says so, swallowing one
//! side's or repeating its next message — so no test speaks for either end,
//! and none computes a proof.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ipc::{Answer, Client, IpcError, Request, Served, State, Token, Welcome, frame};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};

/// How long a peer may be silent: `ipc_silent_after_ms`.
const SILENCE: std::time::Duration =
    std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_silent_after_ms);

/// What a relay does to the frames one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Passes each on.
    Forward,
    /// Passes none on.
    Swallow,
    /// Passes the next frame that is not a ping on twice, then forwards.
    RepeatTheNext,
}

/// One way of a relay: its rule, and how many pings it passed on.
#[derive(Debug, Default)]
struct Way {
    rule: Mutex<Option<Rule>>,
    pings: AtomicUsize,
}

impl Way {
    fn set(&self, rule: Rule) {
        *self.rule.lock().expect("the rule") = Some(rule);
    }

    fn rule(&self) -> Rule {
        self.rule.lock().expect("the rule").unwrap_or(Rule::Forward)
    }
}

/// Passes each frame of `from` on to `to` as `way` says.
async fn pass<R, W>(mut from: R, mut to: W, way: Arc<Way>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    while let Ok(Some(value)) = frame::read(&mut from).await {
        let ping = matches!(&value, Value::Object(fields)
            if fields.get("type") == Some(&Value::text("ping")));
        let times = match way.rule() {
            Rule::Forward => 1,
            Rule::Swallow => 0,
            Rule::RepeatTheNext if ping => 1,
            Rule::RepeatTheNext => {
                way.set(Rule::Forward);
                2
            }
        };
        if ping && times > 0 {
            way.pings.fetch_add(1, Ordering::SeqCst);
        }
        for _ in 0..times {
            if frame::write(&mut to, &value).await.is_err() {
                return;
            }
        }
    }
    let _ = to.shutdown().await;
}

/// The two ways of a relay: toward the daemon, and toward the client.
struct Relay {
    to_daemon: Arc<Way>,
    to_client: Arc<Way>,
}

/// A daemon serving and a client connected through a relay, with one token.
async fn through_a_relay() -> (Served, Client, Relay) {
    let (client, client_side) = tokio::io::duplex(1 << 16);
    let (daemon, daemon_side) = tokio::io::duplex(1 << 16);
    let (to_daemon, to_client) = (Arc::new(Way::default()), Arc::new(Way::default()));
    // One task each way, each reading one half of the relay's side of one
    // end and writing the other end's.
    let (client_reads, client_writes) = tokio::io::split(client_side);
    let (daemon_reads, daemon_writes) = tokio::io::split(daemon_side);
    tokio::spawn(pass(client_reads, daemon_writes, Arc::clone(&to_daemon)));
    tokio::spawn(pass(daemon_reads, client_writes, Arc::clone(&to_client)));
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    (
        served.expect("served"),
        connected.expect("connected").0,
        Relay {
            to_daemon,
            to_client,
        },
    )
}

/// A welcome for a test's daemon.
fn welcome() -> Welcome {
    Welcome {
        pid: std::process::id(),
        instance: [3; 16],
        channel: nc_channel::Channel::Production,
        state: State::Running,
    }
}

/// A daemon serving and a client connected directly, with one token.
async fn directly() -> (Served, Client) {
    let (daemon, client) = tokio::io::duplex(1 << 16);
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    (served.expect("served"), connected.expect("connected").0)
}

/// The status answer a test's daemon gives.
fn status() -> Answer {
    Answer::Status(Value::object([("state", Value::text("running"))]))
}

#[tokio::test(start_paused = true)]
async fn a_client_that_says_nothing_is_dropped_at_the_silence_bound_and_told_why() {
    let (mut served, mut client, relay) = through_a_relay().await;
    relay.to_daemon.set(Rule::Swallow);
    let began = tokio::time::Instant::now();
    match served.next().await {
        Err(refused @ IpcError::Silent) => assert_eq!(refused.code(), ErrorCode::IpcSilent),
        other => panic!("a silent client was {other:?}"),
    }
    assert_eq!(
        began.elapsed(),
        SILENCE,
        "dropped at the bound, and not before"
    );
    let heartbeat = nc_catalog::DAEMON.ipc_heartbeat_ms;
    let expected =
        usize::try_from(nc_catalog::DAEMON.ipc_silent_after_ms / heartbeat - 1).expect("a count");
    assert!(
        relay.to_client.pings.load(Ordering::SeqCst) >= expected,
        "the daemon did not ping after each heartbeat of its own silence"
    );
    match client.ask(&Request::Status).await {
        Err(IpcError::TurnedAway(code)) => assert_eq!(code, ErrorCode::IpcSilent),
        other => panic!("a client the daemon dropped was {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_daemon_that_says_nothing_is_dropped_by_the_client_at_the_silence_bound() {
    let (mut served, mut client, relay) = through_a_relay().await;
    relay.to_client.set(Rule::Swallow);
    let began = tokio::time::Instant::now();
    let (asked, heard) = tokio::join!(client.ask(&Request::Status), served.next());
    assert_eq!(heard.expect("read"), Some(Request::Status));
    match asked {
        Err(IpcError::Silent) => {}
        other => panic!("a silent daemon was {other:?}"),
    }
    assert_eq!(
        began.elapsed(),
        SILENCE,
        "dropped at the bound, and not before"
    );
}

#[tokio::test(start_paused = true)]
async fn two_ends_that_only_ping_keep_their_connection_through_any_silence() {
    let (mut served, mut client) = directly().await;
    tokio::time::sleep(SILENCE * 10).await;
    let (asked, ()) = tokio::join!(client.ask(&Request::Registry), async {
        assert_eq!(served.next().await.expect("read"), Some(Request::Registry));
        served
            .answer(&Answer::Registry(Vec::new()))
            .await
            .expect("answered");
    });
    assert_eq!(asked.expect("answered"), Answer::Registry(Vec::new()));
    drop(client);
    assert_eq!(
        served.next().await.expect("closed"),
        None,
        "a client gone at a frame's boundary is the end, not an error"
    );
}

#[tokio::test(start_paused = true)]
async fn a_dispatch_that_outlasts_the_silence_bound_is_waited_for() {
    let (mut served, mut client) = directly().await;
    let request = Request::Dispatch {
        command: "kernel.undo".to_owned(),
        args: Value::object(Vec::<(&str, Value)>::new()),
    };
    let answer = Answer::Result(Value::object(Vec::<(&str, Value)>::new()));
    let (asked, ()) = tokio::join!(client.ask(&request), async {
        assert_eq!(served.next().await.expect("read"), Some(request.clone()));
        // The daemon is busy for three silences, and its heartbeat goes on.
        tokio::time::sleep(SILENCE * 3).await;
        served.answer(&answer).await.expect("answered");
    });
    assert_eq!(asked.expect("waited for"), answer);
}

#[tokio::test(start_paused = true)]
async fn a_request_before_the_last_one_was_answered_is_refused() {
    let (mut served, mut client, relay) = through_a_relay().await;
    relay.to_daemon.set(Rule::RepeatTheNext);
    let (asked, ()) = tokio::join!(client.ask(&Request::Status), async {
        assert_eq!(served.next().await.expect("read"), Some(Request::Status));
        match served.next().await {
            Err(refused @ IpcError::Malformed(_)) => {
                assert!(
                    refused
                        .to_string()
                        .contains("before the last one was answered"),
                    "{refused}"
                );
            }
            other => panic!("a second request before the first was answered was {other:?}"),
        }
    });
    match asked {
        Err(IpcError::TurnedAway(code)) => assert_eq!(code, ErrorCode::IpcMalformed),
        other => panic!("the client of a refused connection was {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn an_answer_nobody_asked_for_is_refused() {
    let (mut served, mut client, relay) = through_a_relay().await;
    relay.to_client.set(Rule::RepeatTheNext);
    let (asked, ()) = tokio::join!(client.ask(&Request::Status), async {
        served.next().await.expect("read").expect("a request");
        served.answer(&status()).await.expect("answered");
    });
    assert_eq!(asked.expect("the first answer is the request's"), status());
    match client.ask(&Request::Status).await {
        Err(refused @ IpcError::Malformed(_)) => {
            assert!(
                refused.to_string().contains("nobody asked for"),
                "{refused}"
            );
        }
        other => panic!("an answer nobody asked for was {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn the_daemon_answers_only_a_request_it_was_handed() {
    let (mut served, _client) = directly().await;
    match served.answer(&status()).await {
        Err(IpcError::Malformed(_)) => {}
        other => panic!("an answer with no request waiting was {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_request_abandoned_before_its_answer_leaves_the_connection_unasked() {
    let (mut served, mut client) = directly().await;
    let abandoned = tokio::time::timeout(
        std::time::Duration::from_millis(1),
        client.ask(&Request::Status),
    );
    let (abandoned, heard) = tokio::join!(abandoned, served.next());
    assert!(
        abandoned.is_err(),
        "the request was answered, and not abandoned"
    );
    assert_eq!(heard.expect("read"), Some(Request::Status));
    served.answer(&status()).await.expect("answered late");
    match client.ask(&Request::Registry).await {
        Err(IpcError::Malformed(_)) => {}
        other => panic!("a connection with an answer on its way was asked again: {other:?}"),
    }
}
