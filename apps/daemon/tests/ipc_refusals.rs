//! A real daemon refuses every connection that does not prove it holds the
//! channel's token, logs each refusal, and lets nothing of it reach its
//! kernel (`NOCTUA.md §11.1`; Stage 13's criterion 6, its first layer).
//!
//! **Raw connections, made by hand**: each reaches the daemon's endpoint as
//! this user — the endpoint's own refusal of another user is
//! `nc-platform`'s, held by its tests — and then speaks the protocol wrong. A
//! hello with no proof, a proof keyed by another token, and a connection that
//! proves nothing at all are each turned away with the daemon's code, each
//! logged as `ipc.refused` with it, and each followed by a dispatch the daemon
//! never reads; a client that proves itself and then sends what is not a
//! request is closed and logged the same way. The journal holds nothing
//! afterwards. The install names no central log, so the lines are read where
//! such a daemon writes them, its standard error. It spawns a process, so no
//! mutation command runs it.

use std::path::Path;

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_daemon::testing::{Installed, PATIENCE};
use nc_ipc::{Answer, IpcError, Request};

/// The binary this build made.
const BINARY: &str = env!("CARGO_BIN_EXE_noctua-daemon");

/// A raw connection to the daemon's endpoint, its challenge read.
async fn raw(daemon: &nc_daemon::testing::Running) -> (nc_platform::ipc::Stream, Value) {
    let (mut stream, peer) = tokio::time::timeout(
        PATIENCE,
        nc_platform::ipc::connect(&daemon.endpoint(), &daemon.user()),
    )
    .await
    .expect("connected in time")
    .expect("the endpoint answers this user");
    assert_eq!(peer.pid, daemon.pid());
    let challenge = next(&mut stream)
        .await
        .expect("the daemon challenges first");
    assert_eq!(
        challenge
            .clone()
            .into_object()
            .and_then(|fields| fields.get("type").cloned()),
        Some(Value::text("challenge")),
        "{challenge:?}"
    );
    (stream, challenge)
}

/// The next frame of `stream`, read within [`PATIENCE`]: nothing once the
/// daemon closed it.
async fn next(stream: &mut nc_platform::ipc::Stream) -> Option<Value> {
    tokio::time::timeout(PATIENCE, nc_ipc::frame::read(stream))
        .await
        .expect("the daemon spoke or closed in time")
        .ok()
        .flatten()
}

/// What the daemon says once it turned `stream` away: its goodbye's code,
/// and then nothing — a dispatch sent after it is never answered.
async fn turned_away(stream: &mut nc_platform::ipc::Stream) -> String {
    let goodbye = next(stream).await.expect("the daemon says why");
    let fields = goodbye.into_object().expect("an object");
    assert_eq!(fields.get("type"), Some(&Value::text("goodbye")));
    let code = fields
        .get("code")
        .cloned()
        .and_then(Value::into_text)
        .expect("a goodbye names its code");
    let dispatch = Value::object([
        ("type", Value::text("dispatch")),
        ("command", Value::text("journal.verify")),
        ("args", Value::object([("from", Value::Integer(0))])),
    ]);
    // Written or not — the daemon may have closed already — it is never read.
    let _ = nc_ipc::frame::write(stream, &dispatch).await;
    assert_eq!(
        next(stream).await,
        None,
        "the daemon answered after a goodbye"
    );
    code
}

/// A hello with no proof, written on a raw connection: the code the daemon
/// turned it away with.
async fn a_hello_with_no_proof(daemon: &nc_daemon::testing::Running) -> String {
    let (mut stream, challenge) = raw(daemon).await;
    let nonce = challenge
        .into_object()
        .and_then(|fields| fields.get("nonce").cloned())
        .expect("a nonce");
    let hello = Value::object([
        ("type", Value::text("hello")),
        ("protocol", Value::Integer(i64::from(nc_ipc::PROTOCOL))),
        ("nonce", nonce),
    ]);
    nc_ipc::frame::write(&mut stream, &hello)
        .await
        .expect("written");
    turned_away(&mut stream).await
}

/// Each connection refused was logged once as `ipc.refused` with its code,
/// on the daemon's standard error, and nothing else was.
fn each_refusal_was_logged(installed: &Installed) {
    let said = installed.standard_error();
    let refusals: Vec<&str> = said
        .lines()
        .filter(|line| line.contains("\"_msg\":\"ipc.refused\""))
        .collect();
    // The hello with no proof and the request that is not one are each
    // malformed.
    for (code, times) in [
        (ErrorCode::IpcMalformed, 2),
        (ErrorCode::IpcClientUnproven, 1),
        (ErrorCode::IpcHandshakeSlow, 1),
    ] {
        assert_eq!(
            refusals
                .iter()
                .filter(|line| line.contains(&format!("\"code\":\"{}\"", code.code())))
                .count(),
            times,
            "{} was not logged {times} time(s): {refusals:?}",
            code.code()
        );
    }
    assert_eq!(refusals.len(), 4, "{refusals:?}");
    assert!(
        refusals.iter().all(|line| line.starts_with("nc-log: ")),
        "a daemon told of no central log wrote its lines elsewhere: {refusals:?}"
    );
}

/// A hello with no proof, a proof keyed by another token, and a connection
/// that proves nothing in time are each turned away with the daemon's code
/// and logged with it, and so is a proven one that sends what is not a
/// request; nothing any of them sent reaches the kernel: the journal holds
/// nothing, and the daemon goes on answering a client that proves itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_does_not_prove_the_token_is_refused_logged_and_reaches_nothing() {
    use nc_journal::Store as _;

    let installed = Installed::new(Path::new(BINARY));
    installed.without_a_central_log();
    let mut daemon = installed.spawn();
    let (mut client, welcome) = daemon.connected().await;

    // A hello with no proof.
    assert_eq!(
        a_hello_with_no_proof(&daemon).await,
        ErrorCode::IpcMalformed.code()
    );

    // A proof keyed by another token.
    let (stream, _) = tokio::time::timeout(
        PATIENCE,
        nc_platform::ipc::connect(&daemon.endpoint(), &daemon.user()),
    )
    .await
    .expect("connected in time")
    .expect("the endpoint answers this user");
    let wrong = nc_ipc::Token::fresh().expect("another token");
    let refused = tokio::time::timeout(PATIENCE, nc_ipc::connect(stream, &wrong))
        .await
        .expect("refused in time")
        .expect_err("a proof keyed by another token is refused");
    assert!(
        matches!(refused, IpcError::TurnedAway(ErrorCode::IpcClientUnproven)),
        "{refused:?}"
    );

    // Nothing at all, until the handshake's bound.
    let (mut stream, _) = raw(&daemon).await;
    assert_eq!(
        turned_away(&mut stream).await,
        ErrorCode::IpcHandshakeSlow.code()
    );

    // A client that proves itself and then sends a dispatch whose arguments
    // are not one object: closed, never answered.
    let (mut broken, _) = daemon.connected().await;
    let not_a_request = Request::Dispatch {
        command: "journal.verify".to_owned(),
        args: Value::Integer(0),
    };
    let closed = tokio::time::timeout(PATIENCE, broken.ask(&not_a_request))
        .await
        .expect("closed in time")
        .expect_err("what is not a request is not answered");
    assert!(
        matches!(closed, IpcError::Lost(_) | IpcError::TurnedAway(_)),
        "{closed:?}"
    );

    // The daemon goes on answering a client that proves itself.
    let Answer::Status(status) = tokio::time::timeout(PATIENCE, client.ask(&Request::Status))
        .await
        .expect("answered in time")
        .expect("answered")
    else {
        panic!("the status was not answered");
    };
    assert_eq!(
        status
            .into_object()
            .and_then(|fields| fields.get("pid").cloned()),
        Some(Value::Integer(i64::from(welcome.pid)))
    );
    assert_eq!(
        tokio::time::timeout(PATIENCE, client.ask(&Request::Stop))
            .await
            .expect("answered in time")
            .expect("answered"),
        Answer::Stopping
    );
    assert_eq!(
        daemon.ended().await,
        Some(0),
        "{}",
        installed.standard_error()
    );

    each_refusal_was_logged(&installed);
    let store = nc_local::LocalStore::open(&installed.layout.data)
        .await
        .expect("the stopped daemon's database opens");
    let read = store.journal().from(0).await.expect("the journal reads");
    store.close().await;
    assert!(
        read.entries.is_empty(),
        "a refused connection reached the kernel: {:?}",
        read.entries
    );
}
