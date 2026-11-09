//! Each end of a connection proves it holds the channel's token, and neither
//! sends it: a client that cannot prove it is turned away before a request is
//! read, a daemon that cannot prove it is sent nothing more, and either end
//! gives up on the other at the handshake's bound.
//!
//! **The peers that misbehave are driven by hand**, frame by frame through
//! `nc_ipc::frame`, and never compute a proof: a test that made one would be
//! a second implementation of the thing it tests.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_ipc::{Answer, IpcError, Request, State, Token, Welcome, frame};

/// A welcome for a test's daemon.
fn welcome() -> Welcome {
    Welcome {
        pid: std::process::id(),
        instance: [9; 16],
        channel: nc_channel::Channel::Development(
            nc_channel::Checkout::parse("0123abcd").expect("a checkout"),
        ),
        state: State::Running,
    }
}

/// A stream that keeps a copy of every byte written through it.
struct Recorded {
    /// The stream.
    inner: tokio::io::DuplexStream,
    /// Every byte written through it, in order.
    written: Arc<Mutex<Vec<u8>>>,
}

impl Recorded {
    /// `inner`, recording into a record of its own, which is answered too.
    fn new(inner: tokio::io::DuplexStream) -> (Self, Arc<Mutex<Vec<u8>>>) {
        let written = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                inner,
                written: Arc::clone(&written),
            },
            written,
        )
    }
}

impl tokio::io::AsyncRead for Recorded {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(context, buffer)
    }
}

impl tokio::io::AsyncWrite for Recorded {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let written = Pin::new(&mut this.inner).poll_write(context, buffer);
        if let Poll::Ready(Ok(count)) = written {
            this.written
                .lock()
                .expect("the record")
                .extend_from_slice(&buffer[..count]);
        }
        written
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(context)
    }
}

/// Every frame `bytes` holds, in order.
async fn frames_of(bytes: &[u8]) -> Vec<Value> {
    let mut reader = bytes;
    let mut frames = Vec::new();
    while let Some(value) = frame::read(&mut reader).await.expect("a recorded frame") {
        frames.push(value);
    }
    frames
}

/// The field `key` of the message `value`.
fn field(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(fields) => fields.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// The message `kind` holding `fields`.
fn message(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut pairs = vec![("type", Value::text(kind))];
    pairs.extend(fields);
    Value::object(pairs)
}

/// Random bytes as the protocol writes 32 of them: no proof, and no nonce of
/// anything's.
fn random_hex() -> Value {
    let mut bytes = [0_u8; 32];
    nc_platform::random(&mut bytes).expect("random bytes");
    Value::text(nc_canon::to_hex(&bytes))
}

/// The code of the goodbye the next frame of `stream` is, and that nothing
/// follows it.
async fn the_goodbye<S: tokio::io::AsyncRead + Unpin>(stream: &mut S) -> String {
    let said = frame::read(stream)
        .await
        .expect("a frame")
        .expect("a goodbye");
    assert_eq!(field(&said, "type"), Value::text("goodbye"), "{said:?}");
    match frame::read(stream).await {
        Ok(None) | Err(IpcError::Lost(_)) => {}
        other => panic!("something followed the goodbye: {other:?}"),
    }
    field(&said, "code").into_text().expect("a code")
}

#[tokio::test(start_paused = true)]
async fn a_client_holding_the_token_is_welcomed_and_its_request_answered() {
    let token = Token::fresh().expect("a token");
    let (daemon, client) = tokio::io::duplex(1 << 12);
    let welcome = welcome();
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    let mut served = served.expect("the client proved itself");
    let (mut client, welcomed) = connected.expect("the daemon proved itself");
    assert_eq!(welcomed, welcome, "the welcome is what the daemon said");
    let status = Answer::Status(Value::object([("state", Value::text("running"))]));
    let (asked, ()) = tokio::join!(client.ask(&Request::Status), async {
        assert_eq!(served.next().await.expect("read"), Some(Request::Status));
        served.answer(&status).await.expect("answered");
    });
    assert_eq!(asked.expect("answered"), status);
}

#[tokio::test(start_paused = true)]
async fn a_client_whose_proof_is_wrong_or_missing_is_turned_away_before_a_request_is_read() {
    for (proof, refused) in [
        (Some(random_hex()), ErrorCode::IpcClientUnproven),
        (None, ErrorCode::IpcMalformed),
    ] {
        let token = Token::fresh().expect("a token");
        let (daemon, mut client) = tokio::io::duplex(1 << 12);
        let welcome = welcome();
        let serving = nc_ipc::serve(daemon, &token, &welcome);
        let by_hand = async {
            let challenge = frame::read(&mut client)
                .await
                .expect("read")
                .expect("a challenge");
            let mut fields = vec![("protocol", Value::Integer(1)), ("nonce", random_hex())];
            fields.extend(proof.clone().map(|proof| ("proof", proof)));
            frame::write(&mut client, &message("hello", fields))
                .await
                .expect("sent");
            // The request a client turned away must never be read.
            let _ = frame::write(&mut client, &message("status", Vec::new())).await;
            (challenge, the_goodbye(&mut client).await)
        };
        let (verdict, (challenge, goodbye)) = tokio::join!(serving, by_hand);
        match verdict {
            Err(error) => assert_eq!(error.code(), refused, "{error}"),
            Ok(_) => panic!("a client whose proof is {proof:?} was served"),
        }
        assert_eq!(field(&challenge, "type"), Value::text("challenge"));
        assert_eq!(goodbye, refused.code(), "the goodbye says why");
    }
}

#[tokio::test(start_paused = true)]
async fn a_hello_holding_a_field_the_protocol_does_not_read_is_refused() {
    let token = Token::fresh().expect("a token");
    let (daemon, mut client) = tokio::io::duplex(1 << 12);
    let welcome = welcome();
    let by_hand = async {
        frame::read(&mut client)
            .await
            .expect("read")
            .expect("a challenge");
        let hello = message(
            "hello",
            vec![
                ("protocol", Value::Integer(1)),
                ("nonce", random_hex()),
                ("proof", random_hex()),
                ("user", Value::text("ana")),
            ],
        );
        frame::write(&mut client, &hello).await.expect("sent");
        the_goodbye(&mut client).await
    };
    let (verdict, goodbye) = tokio::join!(nc_ipc::serve(daemon, &token, &welcome), by_hand);
    match verdict {
        Err(refused @ IpcError::Malformed(_)) => {
            assert!(refused.to_string().contains("`user`"), "{refused}");
        }
        other => panic!("a hello with a field nothing reads was {other:?}"),
    }
    assert_eq!(goodbye, ErrorCode::IpcMalformed.code());
}

#[tokio::test(start_paused = true)]
async fn a_client_of_another_protocol_is_refused_as_that() {
    let token = Token::fresh().expect("a token");
    let (daemon, mut client) = tokio::io::duplex(1 << 12);
    let welcome = welcome();
    let by_hand = async {
        frame::read(&mut client)
            .await
            .expect("read")
            .expect("a challenge");
        let hello = message(
            "hello",
            vec![
                ("protocol", Value::Integer(2)),
                ("nonce", random_hex()),
                ("proof", random_hex()),
            ],
        );
        frame::write(&mut client, &hello).await.expect("sent");
        the_goodbye(&mut client).await
    };
    let (verdict, goodbye) = tokio::join!(nc_ipc::serve(daemon, &token, &welcome), by_hand);
    match verdict {
        Err(IpcError::Protocol { theirs: 2 }) => {}
        other => panic!("a client of protocol 2 was {other:?}"),
    }
    assert_eq!(goodbye, ErrorCode::IpcProtocol.code());
}

#[tokio::test(start_paused = true)]
async fn either_end_gives_up_on_a_silent_other_at_the_handshakes_bound() {
    let bound = std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_handshake_ms);
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    // A client that reads the challenge and says nothing.
    let (daemon, mut client) = tokio::io::duplex(1 << 12);
    let began = tokio::time::Instant::now();
    let (verdict, ()) = tokio::join!(nc_ipc::serve(daemon, &token, &welcome), async {
        frame::read(&mut client)
            .await
            .expect("read")
            .expect("a challenge");
        assert_eq!(
            the_goodbye(&mut client).await,
            ErrorCode::IpcHandshakeSlow.code()
        );
    });
    assert!(matches!(verdict, Err(IpcError::Slow)), "{verdict:?}");
    assert_eq!(
        began.elapsed(),
        bound,
        "the daemon gave up at its bound, and not before"
    );
    // A daemon that says nothing at all.
    let (_daemon, client) = tokio::io::duplex(1 << 12);
    let began = tokio::time::Instant::now();
    let verdict = nc_ipc::connect(client, &token).await;
    assert!(matches!(verdict, Err(IpcError::Slow)), "{verdict:?}");
    assert_eq!(
        began.elapsed(),
        bound,
        "the client gave up at its bound, and not before"
    );
}

#[tokio::test(start_paused = true)]
async fn a_daemon_whose_proof_is_forged_or_reflected_is_sent_nothing_more() {
    for reflected in [false, true] {
        let token = Token::fresh().expect("a token");
        let (mut daemon, client) = tokio::io::duplex(1 << 12);
        let by_hand = async {
            let challenge = message(
                "challenge",
                vec![("protocol", Value::Integer(1)), ("nonce", random_hex())],
            );
            frame::write(&mut daemon, &challenge).await.expect("sent");
            let hello = frame::read(&mut daemon)
                .await
                .expect("read")
                .expect("a hello");
            // The client's own proof sent back as the daemon's, or one made up.
            let proof = if reflected {
                field(&hello, "proof")
            } else {
                random_hex()
            };
            let welcome = message(
                "welcome",
                vec![
                    ("proof", proof),
                    ("pid", Value::Integer(7)),
                    ("instance", Value::text("00".repeat(16))),
                    ("channel", Value::text("production")),
                    ("state", Value::text("running")),
                ],
            );
            frame::write(&mut daemon, &welcome).await.expect("sent");
            the_goodbye(&mut daemon).await
        };
        let (verdict, goodbye) = tokio::join!(nc_ipc::connect(client, &token), by_hand);
        match verdict {
            Err(IpcError::DaemonUnproven) => {}
            other => panic!("a daemon reflecting={reflected} was {other:?}"),
        }
        assert_eq!(
            goodbye,
            ErrorCode::IpcDaemonUnproven.code(),
            "nothing but why"
        );
    }
}

/// A welcome naming no process is refused as that, before its proof is
/// read: a pid of zero is what a PID record is never held against.
#[tokio::test(start_paused = true)]
async fn a_welcome_naming_no_process_is_refused() {
    let token = Token::fresh().expect("a token");
    let (mut daemon, client) = tokio::io::duplex(1 << 12);
    let by_hand = async {
        let challenge = message(
            "challenge",
            vec![("protocol", Value::Integer(1)), ("nonce", random_hex())],
        );
        frame::write(&mut daemon, &challenge).await.expect("sent");
        frame::read(&mut daemon)
            .await
            .expect("read")
            .expect("a hello");
        let welcome = message(
            "welcome",
            vec![
                ("proof", random_hex()),
                ("pid", Value::Integer(0)),
                ("instance", Value::text("00".repeat(16))),
                ("channel", Value::text("production")),
                ("state", Value::text("running")),
            ],
        );
        frame::write(&mut daemon, &welcome).await.expect("sent");
        the_goodbye(&mut daemon).await
    };
    let (verdict, goodbye) = tokio::join!(nc_ipc::connect(client, &token), by_hand);
    match verdict {
        Err(refused @ IpcError::Malformed(_)) => {
            assert!(refused.to_string().contains("no process"), "{refused}");
        }
        other => panic!("a welcome naming process 0 was {other:?}"),
    }
    assert_eq!(goodbye, ErrorCode::IpcMalformed.code());
}

#[tokio::test(start_paused = true)]
async fn a_client_holding_another_token_is_told_why_it_was_turned_away() {
    let (daemon, client) = tokio::io::duplex(1 << 12);
    let ours = Token::fresh().expect("a token");
    let theirs = Token::fresh().expect("another token");
    let welcome = welcome();
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &ours, &welcome),
        nc_ipc::connect(client, &theirs)
    );
    assert!(
        matches!(served, Err(IpcError::ClientUnproven)),
        "{served:?}"
    );
    match connected {
        Err(IpcError::TurnedAway(code)) => assert_eq!(code, ErrorCode::IpcClientUnproven),
        other => panic!("a client holding another token was {other:?}"),
    }
}

/// The token's bytes, and its text, are in no frame either end sent — through
/// the handshake and a request answered.
#[tokio::test(start_paused = true)]
async fn the_tokens_bytes_cross_the_wire_in_no_frame() {
    let token = Token::fresh().expect("a token");
    let text = token.to_text();
    let bytes = nc_canon::from_hex(&text).expect("hex");
    let (daemon, client) = tokio::io::duplex(1 << 12);
    let (daemon, daemons) = Recorded::new(daemon);
    let (client, clients) = Recorded::new(client);
    let welcome = welcome();
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    let mut served = served.expect("served");
    let (mut client, _) = connected.expect("connected");
    let request = Request::Dispatch {
        command: "journal.verify".to_owned(),
        args: Value::object([("from", Value::Integer(0))]),
    };
    let (asked, ()) = tokio::join!(client.ask(&request), async {
        served.next().await.expect("read").expect("a request");
        served
            .answer(&Answer::Result(Value::object([(
                "verified",
                Value::Integer(0),
            )])))
            .await
            .expect("answered");
    });
    asked.expect("answered");
    drop((client, served));
    for (end, written) in [("the daemon", daemons), ("the client", clients)] {
        let written = written.lock().expect("the record").clone();
        let frames = frames_of(&written).await;
        assert!(frames.len() >= 2, "{end} sent {} frames", frames.len());
        assert!(
            !written
                .windows(bytes.len())
                .any(|window| window == bytes.as_slice()),
            "{end} sent the token's bytes"
        );
        assert!(
            !String::from_utf8_lossy(&written).contains(&text),
            "{end} sent the token's text"
        );
    }
}

/// A hello recorded from one connection is refused on the next: its proof is
/// over the first connection's challenge.
#[tokio::test(start_paused = true)]
async fn a_hello_replayed_from_another_connection_is_refused() {
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    let (daemon, client) = tokio::io::duplex(1 << 12);
    let (client, clients) = Recorded::new(client);
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    drop((served.expect("served"), connected.expect("connected")));
    let recorded = clients.lock().expect("the record").clone();
    let hello = frames_of(&recorded)
        .await
        .into_iter()
        .find(|frame| field(frame, "type") == Value::text("hello"))
        .expect("the client's hello");
    let (daemon, mut client) = tokio::io::duplex(1 << 12);
    let replaying = async {
        frame::read(&mut client)
            .await
            .expect("read")
            .expect("a challenge");
        frame::write(&mut client, &hello).await.expect("sent");
        the_goodbye(&mut client).await
    };
    let (verdict, goodbye) = tokio::join!(nc_ipc::serve(daemon, &token, &welcome), replaying);
    assert!(
        matches!(verdict, Err(IpcError::ClientUnproven)),
        "{verdict:?}"
    );
    assert_eq!(goodbye, ErrorCode::IpcClientUnproven.code());
}

/// A challenge and a welcome recorded from one connection are refused on the
/// next: the client draws a nonce of its own, and the daemon's proof is over it.
///
/// **The stand-in chooses the daemon's nonce**, by sending the recorded
/// challenge again, so the client's nonce is all that tells the recorded
/// welcome from one made for this connection.
#[tokio::test(start_paused = true)]
async fn a_welcome_replayed_from_another_connection_is_refused() {
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    let (daemon, client) = tokio::io::duplex(1 << 12);
    let (daemon, daemons) = Recorded::new(daemon);
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    drop((served.expect("served"), connected.expect("connected")));
    let recorded = daemons.lock().expect("the record").clone();
    let recorded = frames_of(&recorded).await;
    let sent = |kind: &str| {
        recorded
            .iter()
            .find(|frame| field(frame, "type") == Value::text(kind))
            .cloned()
            .unwrap_or_else(|| panic!("the daemon's {kind}"))
    };
    let (challenge, welcomed) = (sent("challenge"), sent("welcome"));
    let (mut daemon, client) = tokio::io::duplex(1 << 12);
    let replaying = async {
        frame::write(&mut daemon, &challenge).await.expect("sent");
        frame::read(&mut daemon)
            .await
            .expect("read")
            .expect("a hello");
        frame::write(&mut daemon, &welcomed).await.expect("sent");
        the_goodbye(&mut daemon).await
    };
    let (verdict, goodbye) = tokio::join!(nc_ipc::connect(client, &token), replaying);
    assert!(
        matches!(verdict, Err(IpcError::DaemonUnproven)),
        "{verdict:?}"
    );
    assert_eq!(goodbye, ErrorCode::IpcDaemonUnproven.code());
}

#[tokio::test(start_paused = true)]
async fn a_token_has_one_spelling_and_its_debug_says_nothing_of_it() {
    let token = Token::fresh().expect("a token");
    let text = token.to_text();
    assert_eq!(text.len(), 64);
    let again = Token::from_text(&text).expect("its own text");
    assert_eq!(again.to_text(), text);
    assert!(!format!("{token:?}").contains(&text[..8]), "{token:?}");
    for not_one in [
        text.to_uppercase(),
        text[..62].to_owned(),
        format!("{text}00"),
        String::new(),
    ] {
        assert!(
            Token::from_text(&not_one).is_none(),
            "{not_one:?} was read as a token"
        );
    }
    assert_ne!(
        Token::fresh().expect("a token").to_text(),
        text,
        "two tokens drawn alike"
    );
}
