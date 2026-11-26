//! The interface follows its daemon's state over a watch, asks over a
//! connection of its own, and waits for a daemon that went away rather than
//! give up on it (`NOCTUA.md §12.1`, §11.1).
//!
//! **A scripted daemon, through the real endpoint and the real handshake**
//! (`nc_ipc::testing`): the answers are the script's, and every request the
//! interface sent is handed back, so what it asked is held as well as what it
//! read. No daemon process is started.

use nc_canon::Value;
use nc_desktop::client::{Asker, Flow, Heard, again_after, follow, watch_wait};
use nc_ipc::testing::{Scratch, Script, Scripted};
use nc_ipc::{Answer, Note, Refusal, RefusalKind, Request, Token};

/// A state as the kernel writes the first run's, `next` naming the next
/// identifier, so two states tell apart.
fn state(next: i64) -> Value {
    Value::object([
        (
            "windows",
            Value::Array(vec![Value::object([
                ("id", Value::Integer(1)),
                ("shown", Value::Bool(false)),
                ("pages", Value::Array(Vec::new())),
            ])]),
        ),
        ("pages", Value::Array(Vec::new())),
        (
            "drawer",
            Value::object([
                ("pages", Value::Array(Vec::new())),
                ("collapsed", Value::Bool(false)),
                ("toward", Value::text("workspace")),
                ("flipped", Value::Bool(false)),
                ("share", Value::Integer(33)),
            ]),
        ),
        ("sidebar", Value::object([("shown", Value::Bool(false))])),
        ("footer", Value::object([("shown", Value::Bool(true))])),
        ("info", Value::object([("expanded", Value::Bool(false))])),
        ("prompt", Value::object([("draft", Value::text(""))])),
        ("seats", Value::Array(Vec::new())),
        ("next", Value::Integer(next)),
    ])
}

fn changed(revision: u64, state: Option<Value>) -> Answer {
    Answer::Changed { revision, state }
}

/// What the interface heard, written so a test compares it whole.
fn written(heard: &Heard) -> String {
    match heard {
        Heard::Reached => "reached".to_owned(),
        Heard::Changed(changed) => format!(
            "changed {} {}",
            changed.revision,
            changed
                .state
                .as_ref()
                .map_or_else(|| "nothing".to_owned(), Value::to_canonical)
        ),
        Heard::Lost(_) => "lost".to_owned(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("a runtime")
}

/// Each state is handed over once, at its revision, and a wait that ended
/// with nothing changed is not; each watch names the revision the one before
/// it was answered at, and asks the daemon's own bound; a connection that
/// closes is lost, and the daemon is asked again after the heartbeat, its
/// watch having seen nothing.
#[test]
fn each_state_is_handed_over_once_and_a_lost_daemon_is_asked_again() {
    let installed = Scratch::new("follow");
    let token = Token::fresh().expect("a token");
    installed.token(&token);
    let daemon = Scripted::serve(
        &installed,
        Script {
            token,
            welcome: installed.welcome(),
            connections: vec![
                vec![
                    changed(1, Some(state(2))),
                    changed(1, None),
                    changed(2, Some(state(3))),
                ],
                vec![changed(1, Some(state(4)))],
            ],
        },
    );
    let mut heard: Vec<String> = Vec::new();
    let mut lost_at = None;
    let mut reached_again_at = None;
    runtime().block_on(follow(&installed.found, &mut |one| {
        if matches!(one, Heard::Lost(_)) {
            lost_at = Some(std::time::Instant::now());
        }
        if matches!(one, Heard::Reached) && lost_at.is_some() {
            reached_again_at = Some(std::time::Instant::now());
        }
        heard.push(written(&one));
        if heard.len() == 6 {
            Flow::Stop
        } else {
            Flow::Go
        }
    }));
    assert_eq!(
        heard,
        [
            "reached".to_owned(),
            format!("changed 1 {}", state(2).to_canonical()),
            format!("changed 2 {}", state(3).to_canonical()),
            "lost".to_owned(),
            "reached".to_owned(),
            format!("changed 1 {}", state(4).to_canonical()),
        ]
    );
    let waited = reached_again_at.expect("reached again") - lost_at.expect("lost");
    let heartbeat = std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_heartbeat_ms);
    assert_eq!(again_after(), heartbeat);
    assert!(
        waited >= heartbeat,
        "the daemon was asked again after {waited:?}, before the heartbeat"
    );
    let wait_ms = u64::try_from(watch_wait().as_millis()).expect("a wait");
    assert_eq!(wait_ms, nc_catalog::DAEMON.ipc_watch_wait_ms);
    let watch = |seen| Request::Watch { seen, wait_ms };
    assert_eq!(
        daemon.heard(),
        [watch(None), watch(Some(1)), watch(Some(1)), watch(None)],
        "each watch names the revision the one before it was answered at, and a \
         connection made again has seen nothing"
    );
}

/// No daemon at all is lost, and asked again, for as long as the caller goes
/// on.
#[test]
fn no_daemon_is_lost_and_asked_again() {
    let installed = Scratch::new("none");
    installed.token(&Token::fresh().expect("a token"));
    let mut lost = 0;
    runtime().block_on(follow(&installed.found, &mut |one| {
        assert!(matches!(one, Heard::Lost(_)), "{}", written(&one));
        lost += 1;
        if lost == 2 { Flow::Stop } else { Flow::Go }
    }));
    assert_eq!(lost, 2);
}

/// A command is dispatched as asked, and its answer handed back; a note the
/// daemon refused keeps the connection, which the next request goes over;
/// and a connection that failed is made again by the next request.
#[test]
fn an_asker_dispatches_and_notes_and_makes_its_connection_again_once_it_failed() {
    let installed = Scratch::new("ask");
    let token = Token::fresh().expect("a token");
    installed.token(&token);
    let refusal = Refusal {
        code: nc_catalog::ErrorCode::ValidationArgumentsInvalid,
        kind: RefusalKind::Invalid,
        sentence: "a note says one fact at least".to_owned(),
    };
    let daemon = Scripted::serve(
        &installed,
        Script {
            token,
            welcome: installed.welcome(),
            connections: vec![
                vec![
                    Answer::Result(Value::object([("closed", Value::Bool(true))])),
                    Answer::Refused(refusal.clone()),
                    Answer::Noted,
                ],
                vec![Answer::Noted],
            ],
        },
    );
    let draft = || {
        Note::new(Value::object([(
            "draft",
            Value::object([("text", Value::text("a"))]),
        )]))
    };
    let mut asker = Asker::new(installed.found.clone());
    let close = Value::object([("window", Value::Integer(1))]);
    runtime().block_on(async {
        let answered = asker
            .dispatch("window.close", close.clone())
            .await
            .expect("asked");
        assert_eq!(answered, Ok(Value::object([("closed", Value::Bool(true))])));
        let refused = asker
            .note(draft().expect("a note"))
            .await
            .expect_err("refused");
        assert!(
            matches!(&refused, nc_ipc::client::Unasked::Refused(said) if *said == refusal),
            "{refused:?}"
        );
        asker
            .note(draft().expect("a note"))
            .await
            .expect("taken over the connection the refusal kept");
        // The script ends the first connection here: the next request finds
        // it closed, fails, and the one after it makes another.
        let failed = asker.note(draft().expect("a note")).await;
        assert!(failed.is_err(), "the closed connection fails: {failed:?}");
        asker
            .note(draft().expect("a note"))
            .await
            .expect("taken over a connection made again");
    });
    let note = Request::Note(draft().expect("a note"));
    let heard = daemon.heard();
    assert_eq!(
        heard,
        [
            Request::Dispatch {
                command: "window.close".to_owned(),
                args: close,
            },
            note.clone(),
            note.clone(),
            note,
        ]
    );
}
