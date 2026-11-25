//! A daemon that answers as a script says, on an install made in a scratch
//! directory: **for tests**, behind `test-support`.
//!
//! What a client's suite connects to — this crate's own, the CLI's, the
//! desktop's — so no daemon process is started: a real endpoint bound in a
//! scratch runtime directory, the real handshake, and then the answers the
//! script lists, in turn, whatever was asked. Every request it heard is handed
//! back, so a test holds what a client sent as well as what it read.
//!
//! **Every wait is bounded**, so a client that never comes fails the test
//! rather than hanging it, and the scratch is removed however the test leaves.

#![cfg(any(test, feature = "test-support"))]

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use nc_channel::found::Found;

use crate::{Answer, Request, State, Token, Welcome};

/// How long any one wait of a scripted daemon may take before the test
/// fails rather than hangs.
const WAIT: Duration = Duration::from_secs(60);

/// An install in a scratch directory, removed however the test leaves.
#[derive(Debug)]
pub struct Scratch {
    /// The scratch directory.
    root: PathBuf,
    /// The install read from its manifest, its pipe named for this test.
    pub found: Found,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = nc_platform::testing::make_private(&self.root);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Scratch {
    /// A staging install relocated under `tag`, its runtime directory
    /// made, read beside a stand-in executable as the CLI reads one.
    ///
    /// **Short names**, because macOS holds a socket's path to 103 bytes;
    /// **and the pipe named for this test**, because a pipe's name is the
    /// whole machine's: a test prefix, the checkout, this process and a
    /// nonce.
    ///
    /// # Panics
    ///
    /// When the scratch install cannot be made or read, which fails the test
    /// that asked for it.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        use std::io::Write as _;
        let root = std::env::temp_dir().join(format!("nc-ask-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        nc_platform::create_private_directory(&root).expect("a scratch directory");
        let base = |name: &str| root.join(name);
        let install = nc_channel::Install {
            channel: nc_channel::Channel::Staging,
            bases: nc_channel::Bases::new(base("d"), base("c"), base("k"), base("s"))
                .expect("plain absolute bases"),
            namespace: Some(nc_channel::Namespace::parse("t1").expect("a namespace")),
            log_endpoint: None,
        };
        let mut manifest =
            nc_platform::create_private(&root.join(nc_channel::MANIFEST)).expect("a manifest");
        manifest
            .write_all(install.to_canonical().as_bytes())
            .expect("written");
        drop(manifest);
        nc_platform::create_private(&root.join("noctua")).expect("a stand-in binary");
        let mut found =
            nc_channel::found::beside(&root.join("noctua")).expect("the install is read");
        for directory in [
            base("s"),
            found.layout.state.clone(),
            found.layout.runtime.clone(),
        ] {
            nc_platform::create_private_directory(&directory).expect("a directory");
        }
        let checkout =
            nc_channel::Checkout::of(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")));
        let mut nonce = [0_u8; 4];
        nc_platform::random(&mut nonce).expect("random bytes");
        found.layout.pipe = format!(
            r"\\.\pipe\nc-test-{checkout}-{}-{:08x}-{tag}",
            std::process::id(),
            u32::from_le_bytes(nonce)
        );
        Self { root, found }
    }

    /// `name` in the scratch directory, which holds the install's manifest.
    #[must_use]
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// The token file written as a daemon writes it, private.
    ///
    /// # Panics
    ///
    /// When the file cannot be written.
    pub fn token(&self, token: &Token) {
        self.write_token(&token.to_text());
    }

    /// The token file holding `text`, private.
    ///
    /// # Panics
    ///
    /// When the file cannot be written.
    pub fn write_token(&self, text: &str) {
        use std::io::Write as _;
        let path = &self.found.layout.token;
        let _ = std::fs::remove_file(path);
        nc_platform::create_private(path)
            .expect("a token file")
            .write_all(text.as_bytes())
            .expect("written");
    }

    /// The welcome a daemon of this install gives: this process, which
    /// is what the system says is at the other end of an in-process
    /// listener.
    #[must_use]
    pub fn welcome(&self) -> Welcome {
        Welcome {
            pid: std::process::id(),
            instance: [7; 16],
            channel: self.found.install.channel,
            state: State::Running,
        }
    }
}

/// What a scripted daemon does with each connection it accepts: the
/// welcome it gives, and the answer to each request, in turn.
#[derive(Debug)]
pub struct Script {
    /// The token it proves itself with.
    pub token: Token,
    /// The welcome it gives.
    pub welcome: Welcome,
    /// For each connection, the answers to its requests in order.
    pub connections: Vec<Vec<Answer>>,
}

/// A daemon serving a script on a thread of its own.
#[derive(Debug)]
pub struct Scripted {
    /// The thread, which hands back every request it heard.
    thread: Option<std::thread::JoinHandle<Vec<Request>>>,
}

impl Scripted {
    /// Binds `installed`'s endpoint and serves `script` on it, once
    /// bound.
    ///
    /// # Panics
    ///
    /// When the endpoint is not bound within the wait; the daemon's thread
    /// panics, failing [`Scripted::heard`], when a client it waits for does
    /// not come within it or an answer cannot be handed over.
    #[must_use]
    pub fn serve(installed: &Scratch, script: Script) -> Self {
        let found = installed.found.clone();
        let lock = installed.root.clone();
        let (bound, ready) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .expect("a runtime");
            runtime.block_on(async move {
                let held =
                    nc_platform::hold(&lock, "instance.lock", &found.user).expect("the lock");
                let endpoint = nc_platform::ipc::Endpoint::new(
                    &found.layout.runtime,
                    &found.layout.socket,
                    found.layout.pipe.as_str(),
                );
                let mut listener = crate::listen(&endpoint, &found.user, &held).expect("bound");
                bound.send(()).expect("the test waits for the bind");
                let mut heard = Vec::new();
                for answers in script.connections {
                    // A client that closed before the listener read who it
                    // is — one that refused before its handshake, whose
                    // peer macOS no longer names once it has gone — is a
                    // connection this script has nothing to serve.
                    let Ok(accepted) = tokio::time::timeout(WAIT, listener.accept())
                        .await
                        .expect("a client within the wait")
                    else {
                        continue;
                    };
                    let Ok(mut served) = tokio::time::timeout(
                        WAIT,
                        crate::serve(accepted.stream, &script.token, &script.welcome),
                    )
                    .await
                    .expect("a handshake within the wait") else {
                        continue;
                    };
                    for answer in answers {
                        match tokio::time::timeout(WAIT, served.next())
                            .await
                            .expect("a request within the wait")
                        {
                            Ok(Some(request)) => heard.push(request),
                            _ => break,
                        }
                        served.answer(&answer).await.expect("answered");
                    }
                    // The client closes the connection when it is done.
                    let _ = tokio::time::timeout(WAIT, served.next()).await;
                }
                heard
            })
        });
        ready
            .recv_timeout(WAIT)
            .expect("the scripted daemon bound its endpoint");
        Self {
            thread: Some(thread),
        }
    }

    /// Every request the daemon heard, once it has served its script.
    ///
    /// # Panics
    ///
    /// When the daemon's thread did not finish its script.
    #[must_use]
    pub fn heard(mut self) -> Vec<Request> {
        self.thread
            .take()
            .expect("joined once")
            .join()
            .expect("the scripted daemon finished its script")
    }
}

impl Drop for Scripted {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A registry entry as the daemon lists a command.
///
/// Every field it holds, the ones the CLI reads among them, its texts
/// written from `display_name`.
#[must_use]
pub fn entry(name: &str, display_name: &str, args_schema: &str) -> nc_canon::Value {
    let texts = |all: &[&str]| {
        nc_canon::Value::Array(all.iter().map(|&one| nc_canon::Value::text(one)).collect())
    };
    nc_canon::Value::object([
        ("name", nc_canon::Value::text(name)),
        ("display_name", nc_canon::Value::text(display_name)),
        (
            "line",
            nc_canon::Value::text(format!("{display_name}: the line")),
        ),
        (
            "paragraph",
            nc_canon::Value::text(format!("{display_name}: the paragraph")),
        ),
        ("tags", texts(&["one", "two"])),
        ("categories", texts(&["noctua.history"])),
        ("glyph", nc_canon::Value::text("U+F058")),
        ("args_schema", nc_canon::Value::text(args_schema)),
        ("mutating", nc_canon::Value::Bool(false)),
        ("action", nc_canon::Value::text("Read")),
        ("resource_selector", nc_canon::Value::text("realm")),
        ("reversibility", nc_canon::Value::text("read")),
        ("owner", nc_canon::Value::text("kernel")),
    ])
}
