//! A frame is a big-endian length and that many bytes of canonical JSON, and
//! a reader refuses every frame that is not exactly that — the length before
//! anything is allocated for it.
//!
//! **On a paused clock**, so a reader that waited for bytes nobody sends is a
//! test that fails at once rather than one that hangs.

use nc_canon::Value;
use nc_ipc::{Answer, IpcError, Request, Token, frame};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// The bound the reader holds a frame's body to.
const MOST: u64 = nc_catalog::DAEMON.ipc_frame_max_bytes;

/// The bytes a frame of `body` is: its length, big-endian, and the body.
fn framed(body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(body.len())
        .expect("a test's body fits a prefix")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(body);
    bytes
}

/// What a reader makes of `bytes`, followed by the writer closing.
async fn read_of(bytes: &[u8]) -> Result<Option<Value>, IpcError> {
    let mut reader = bytes;
    frame::read(&mut reader).await
}

#[tokio::test(start_paused = true)]
async fn a_value_round_trips_through_one_frame_of_its_canonical_text() {
    let values = [
        Value::object([
            ("b", Value::Integer(-1)),
            ("a", Value::text("é\u{2028}\"\\")),
        ]),
        Value::Array(vec![
            Value::Null,
            Value::Bool(true),
            Value::object(Vec::<(&str, Value)>::new()),
        ]),
        Value::text(""),
    ];
    for value in values {
        let mut written = Vec::new();
        frame::write(&mut written, &value).await.expect("written");
        assert_eq!(
            written,
            framed(value.to_canonical().as_bytes()),
            "a frame is its length and the value's canonical text"
        );
        let mut reader = written.as_slice();
        let read = frame::read(&mut reader)
            .await
            .expect("read")
            .expect("a value");
        assert_eq!(read, value);
        assert!(
            frame::read(&mut reader).await.expect("closed").is_none(),
            "a connection closed at a frame's boundary is read as closed"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_connection_that_ends_inside_a_frame_was_lost_and_one_at_its_boundary_closed() {
    assert!(read_of(&[]).await.expect("closed").is_none());
    let whole = framed(b"{\"a\":1}");
    for cut in 1..whole.len() {
        match read_of(&whole[..cut]).await {
            Err(IpcError::Lost(_)) => {}
            other => panic!("a frame cut after {cut} bytes was {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_frame_longer_than_the_bound_is_refused_from_its_prefix_before_its_body_is_read() {
    for claimed in [MOST + 1, u64::from(u32::MAX)] {
        // **The writer stays open and sends nothing past the prefix**: a reader
        // that read, or allocated and waited for, the body it was promised
        // would never answer, and the paused clock ends the wait at once.
        let (mut writer, mut reader) = tokio::io::duplex(64);
        let prefix = u32::try_from(claimed).expect("a prefix").to_be_bytes();
        writer.write_all(&prefix).await.expect("the prefix is sent");
        let verdict =
            tokio::time::timeout(std::time::Duration::from_secs(1), frame::read(&mut reader))
                .await
                .expect("refused from the prefix, without waiting for the body");
        match verdict {
            Err(refused @ IpcError::TooLarge { length }) => {
                assert_eq!(length, claimed);
                assert_eq!(refused.code(), nc_catalog::ErrorCode::IpcFrameTooLarge);
            }
            other => panic!("a frame claiming {claimed} bytes was {other:?}"),
        }
        drop(writer);
    }
    // **At the bound exactly, it is taken**: a string whose quotes and letters
    // are the whole bound.
    let at_the_bound = Value::text("a".repeat(usize::try_from(MOST).expect("a size") - 2));
    let mut written = Vec::new();
    frame::write(&mut written, &at_the_bound)
        .await
        .expect("a frame at the bound is written");
    assert_eq!(read_of(&written).await.expect("taken"), Some(at_the_bound));
}

#[tokio::test(start_paused = true)]
async fn a_frame_of_no_length_is_refused() {
    match read_of(&[0, 0, 0, 0]).await {
        Err(refused @ IpcError::Malformed(_)) => {
            assert_eq!(refused.code(), nc_catalog::ErrorCode::IpcMalformed);
            // Refused as a frame of no length, from its prefix: read as a body,
            // it would be refused only as text that is not JSON.
            assert!(refused.to_string().contains("no length"), "{refused}");
        }
        other => panic!("a frame of no length was {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn text_that_is_not_the_canonical_spelling_of_its_value_is_refused() {
    let bodies: [&[u8]; 7] = [
        b"{\"b\":1,\"a\":2}",
        b"{\"a\": 1}",
        b"{\"a\":1} ",
        b"\"\\u0041\"",
        b"{\"a\":",
        b"\xff\xfe",
        b"[1,]",
    ];
    for body in bodies {
        match read_of(&framed(body)).await {
            Err(IpcError::Malformed(_)) => {}
            other => panic!("{:?} was {other:?}", String::from_utf8_lossy(body)),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_writer_refuses_a_value_no_reader_would_take_and_writes_nothing() {
    let past = Value::text("a".repeat(usize::try_from(MOST).expect("a size") - 1));
    let mut written = Vec::new();
    match frame::write(&mut written, &past).await {
        Err(IpcError::TooLarge { length }) => assert_eq!(length, MOST + 1),
        other => panic!("a value past the bound was {other:?}"),
    }
    assert!(written.is_empty(), "a refused frame was partly written");
}

/// A dispatch's answer carrying the kernel's whole payload, and a dispatch
/// carrying arguments as large, each cross one frame.
///
/// **This is what `[daemon] ipc_frame_max_bytes` is held above**, by the
/// compiler, as the payload and what a frame wraps it in: were the bound one
/// the compiler let through and a payload at the kernel's limit did not fit,
/// this is where it would show.
#[tokio::test(start_paused = true)]
async fn a_dispatch_and_its_answer_at_the_kernels_whole_payload_each_cross_one_frame() {
    let payload = usize::try_from(nc_catalog::KERNEL.payload_bytes).expect("a size");
    // An object of one key whose canonical text is the whole payload.
    let largest = Value::object([("p", Value::text("a".repeat(payload - 8)))]);
    assert_eq!(largest.to_canonical().len(), payload);
    let token = Token::fresh().expect("a token");
    let welcome = welcome();
    let (daemon, client) = tokio::io::duplex(1 << 16);
    let (served, connected) = tokio::join!(
        nc_ipc::serve(daemon, &token, &welcome),
        nc_ipc::connect(client, &token)
    );
    let mut served = served.expect("served");
    let (mut client, _) = connected.expect("connected");
    let request = Request::Dispatch {
        command: "reference.notes.put".to_owned(),
        args: largest.clone(),
    };
    let answer = Answer::Result(largest);
    let (asked, ()) = tokio::join!(client.ask(&request), async {
        let heard = served.next().await.expect("a request").expect("one");
        assert_eq!(heard, request);
        served.answer(&answer).await.expect("answered");
    });
    assert_eq!(asked.expect("the answer"), answer);
}

/// A welcome for a test's daemon.
fn welcome() -> nc_ipc::Welcome {
    nc_ipc::Welcome {
        pid: std::process::id(),
        instance: [7; 16],
        channel: nc_channel::Channel::Staging,
        state: nc_ipc::State::Running,
    }
}

#[tokio::test(start_paused = true)]
async fn a_frame_read_from_a_stream_leaves_the_next_one_whole() {
    let (mut writer, mut reader) = tokio::io::duplex(8);
    let first = Value::text("first");
    let second = Value::object([("n", Value::Integer(2))]);
    let sending = async {
        frame::write(&mut writer, &first).await.expect("written");
        frame::write(&mut writer, &second).await.expect("written");
        writer.shutdown().await.expect("closed");
    };
    let reading = async {
        let one = frame::read(&mut reader).await.expect("read");
        let two = frame::read(&mut reader).await.expect("read");
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).await.expect("read");
        (one, two, rest)
    };
    let ((), (one, two, rest)) = tokio::join!(sending, reading);
    assert_eq!((one, two), (Some(first), Some(second)));
    assert!(rest.is_empty(), "a frame left bytes of its own behind");
}
