//! The CLI's end of a connection to its channel's daemon: the endpoint
//! reached, the token read, the handshake, and one request at a time
//! (`NOCTUA.md §11.1`).
//!
//! # What is checked before anything is sent, in order
//!
//! 1. **The endpoint**, through `nc_platform::ipc::connect`: the socket or the
//!    pipe the install's layout names, and the user and the process the
//!    system says are at its other end — this user's, or the connection is
//!    closed before a byte crosses it. Nothing listening is told apart
//!    (`NC-IPC-0014`): it is a daemon that is not running.
//! 2. **The token**, read only once the runtime directory and the token file
//!    are judged this user's alone (`NC-CLI-0002`), and only as the 64
//!    hexadecimal digits a daemon writes (`NC-CLI-0003`). It is never printed,
//!    never sent, and the text it was read from is overwritten once read.
//! 3. **The handshake**, `nc_ipc::connect`: the daemon proves it holds the
//!    token, over both nonces, before the client sends anything past its own
//!    proof.
//! 4. **The welcome sent beside the proof**: the process it names must be the
//!    one the system says is at the other end, and the channel the install's
//!    (`NC-CLI-0004`) — so a daemon of another channel that somehow held this
//!    token is refused before a request is made, and so is a relay between the
//!    two that passes the daemon's own welcome on. **The daemon's proof binds
//!    the two nonces and not the welcome**, so a relay that rewrites the
//!    welcome to name itself is not refused by it; such a relay runs as this
//!    user, as both ends' peer checks require, which is the process below.
//!
//! Only then is a request sent, one at a time; an answer of another kind than
//! the request's is refused (`NC-CLI-0005`). A connection that opens an
//! agent's session ([`session`]) is that agent's from the daemon's answer on:
//! the daemon decides each of its dispatches as the agent, for the person who
//! made it, and answers it nothing but its tools and its dispatches.
//!
//! # What this does not answer
//!
//! A process running as the same user, which can read the token and answer
//! as the daemon, connect as a client, or relay a connection rewriting the
//! welcome — authorization re-decides every dispatch (`§14`); and the PID
//! record the daemon writes, which this does not
//! read: the welcome is held to the system's word for the other end, and not
//! to the record process control reads ([`crate::control`]).

use std::path::Path;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_channel::found::Found;
use nc_ipc::{Answer, IpcError, Refusal, Request, Token, Welcome};
use nc_platform::{Judged, Kind, User};

use crate::exit::Exit;

/// A command the registry holds, as the CLI reads it: its name, its entry's
/// texts in the locale the registry was asked in, its categories and its
/// argument schema's text.
///
/// **The fields the CLI shows of an entry, and nothing else**: an entry is
/// the kernel's description of a command, and the fields it holds beside
/// these — its glyph, which a terminal is not promised the font of, its
/// action, its selector, who runs it — are the daemon's to add without this
/// build refusing them. Every text here is the daemon's, the catalog's
/// commands' as much as a plugin's, and is escaped wherever it is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// Its registered name.
    pub name: String,
    /// The name a person reads first.
    pub display_name: String,
    /// One line saying what it does.
    pub line: String,
    /// A paragraph saying more.
    pub paragraph: String,
    /// The words a search finds it by.
    pub tags: Vec<String>,
    /// The categories it is listed under, by name.
    pub categories: Vec<String>,
    /// The JSON Schema of its arguments, as declared.
    pub args_schema: String,
}

impl Listed {
    /// The entry `value` is, or nothing for one this build does not read.
    #[must_use]
    pub fn read(value: &Value) -> Option<Self> {
        let Value::Object(fields) = value else {
            return None;
        };
        let text = |key: &str| match fields.get(key) {
            Some(Value::Text(text)) => Some(text.clone()),
            _ => None,
        };
        let texts = |key: &str| match fields.get(key) {
            Some(Value::Array(all)) => all
                .iter()
                .map(|one| match one {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        };
        Some(Self {
            name: text("name")?,
            display_name: text("display_name")?,
            line: text("line")?,
            paragraph: text("paragraph")?,
            tags: texts("tags")?,
            categories: texts("categories")?,
            args_schema: text("args_schema")?,
        })
    }
}

/// Why nothing was asked, or no answer was read.
#[derive(Debug)]
pub enum Unasked {
    /// The endpoint was not reached, or its other end was not this user's.
    Connecting(IpcError),
    /// The token file, or the runtime directory holding it, is not this
    /// user's alone; the text says which.
    TokenNotPrivate(String),
    /// The token file is missing, or holds no token; the text says which.
    TokenUnreadable(String),
    /// The handshake did not prove both ends.
    Handshake(IpcError),
    /// The welcome names another process or another channel; the text says
    /// which.
    Misnamed(String),
    /// A request was not answered: the connection failed or closed.
    Asking(IpcError),
    /// The daemon answered with something the CLI does not read; the text
    /// says what.
    Unread(String),
    /// The daemon refused what was asked.
    ///
    /// It refuses the registry or the tools asked before its kernel runs or
    /// once it is stopping; the registry asked on a session (`NC-DMN-0015`);
    /// a session's tools once MCP is off (`NC-DMN-0014`) or once its switch
    /// cannot be read, either of which also ends the session; and a session it
    /// does not open.
    Refused(Refusal),
    /// The runtime a connection runs on could not be started.
    Runtime(std::io::Error),
}

impl Unasked {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Connecting(error) | Self::Handshake(error) | Self::Asking(error) => error.code(),
            Self::TokenNotPrivate(_) => ErrorCode::CliTokenNotPrivate,
            Self::TokenUnreadable(_) => ErrorCode::CliTokenUnreadable,
            Self::Misnamed(_) => ErrorCode::CliDaemonMisnamed,
            Self::Unread(_) => ErrorCode::CliAnswerUnreadable,
            Self::Refused(refusal) => refusal.code,
            Self::Runtime(_) => ErrorCode::IpcEndpointUnavailable,
        }
    }

    /// The outcome a run that ends here carries.
    ///
    /// **Nothing listening is a daemon that is not running**, and so is one
    /// that stopped answering a request; everything refused on the way to the
    /// first request is a connection refused; a request too long to send is
    /// arguments refused; and an answer the CLI cannot read — a frame that is
    /// not one message, or one past the bound — from a daemon the handshake
    /// proved is the daemon answering what the CLI does not read.
    #[must_use]
    pub fn exit(&self) -> Exit {
        match self {
            Self::Connecting(error) if error.code() == ErrorCode::IpcUnreachable => {
                Exit::Unreachable
            }
            Self::Asking(IpcError::Unsendable { .. }) => Exit::Invalid,
            Self::Asking(IpcError::Malformed(_) | IpcError::TooLarge { .. }) | Self::Unread(_) => {
                Exit::Internal
            }
            Self::Asking(
                IpcError::Lost(_) | IpcError::Silent | IpcError::Unread | IpcError::TurnedAway(_),
            ) => Exit::Unreachable,
            Self::Connecting(_)
            | Self::TokenNotPrivate(_)
            | Self::TokenUnreadable(_)
            | Self::Handshake(_)
            | Self::Misnamed(_)
            | Self::Asking(_) => Exit::Refused,
            Self::Refused(refusal) => Exit::of_refusal(refusal),
            Self::Runtime(_) => Exit::Failed,
        }
    }

    /// What happened, in English, without the code in front of it.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Connecting(error) | Self::Handshake(error) | Self::Asking(error) => {
                crate::render::without_code(&error.to_string(), error.code())
            }
            Self::TokenNotPrivate(detail)
            | Self::TokenUnreadable(detail)
            | Self::Misnamed(detail)
            | Self::Unread(detail) => detail.clone(),
            Self::Refused(refusal) => refusal.sentence.clone(),
            Self::Runtime(error) => {
                format!("the runtime a connection runs on could not be started: {error}")
            }
        }
    }
}

/// A connection to the daemon, proven and held to the install.
#[derive(Debug)]
pub struct Connection {
    /// The client's end.
    client: nc_ipc::Client,
    /// What the daemon said it is.
    welcome: Welcome,
}

/// A dispatch answered: what the command answered, or the kernel's refusal.
pub type Dispatched = Result<Value, Refusal>;

/// A connection to the daemon of `found`'s install, once everything this
/// module's header lists holds.
///
/// # Errors
///
/// [`Unasked::Connecting`], [`Unasked::TokenNotPrivate`],
/// [`Unasked::TokenUnreadable`], [`Unasked::Handshake`] and
/// [`Unasked::Misnamed`], in the order they are checked.
pub async fn connect(found: &Found) -> Result<Connection, Unasked> {
    let layout = &found.layout;
    let endpoint =
        nc_platform::ipc::Endpoint::new(&layout.runtime, &layout.socket, layout.pipe.as_str());
    let (stream, peer) = nc_platform::ipc::connect(&endpoint, &found.user)
        .await
        .map_err(|refused| Unasked::Connecting(IpcError::from(refused)))?;
    let token = token_of(&layout.runtime, &layout.token, &found.user)?;
    let (client, welcome) = nc_ipc::connect(stream, &token)
        .await
        .map_err(Unasked::Handshake)?;
    drop(token);
    held_to(&welcome, peer.pid, found)?;
    Ok(Connection { client, welcome })
}

/// A connection to the daemon of `found`'s install acting as `agent`: one
/// [`connect`] makes, then the session opened on it.
///
/// **The daemon decides whose session it is**: it opens one only while MCP is
/// on, within `sessions_max`, and for an agent the profile's person made, and
/// closes the connection on any refusal. From the answer on, every dispatch on
/// the connection is decided as `agent`, and the connection asks for nothing
/// but its tools and dispatches.
///
/// # Errors
///
/// What [`connect`] refuses, in its order; [`Unasked::Asking`] as
/// [`Connection::ask`]; [`Unasked::Refused`] with the daemon's refusal — MCP
/// off (`NC-DMN-0014`), past `sessions_max` (`NC-DMN-0016`), no such agent of
/// the person's (`NC-AI-0001`), or not running; and [`Unasked::Unread`] for an
/// answer of another kind.
pub async fn session(found: &Found, agent: nc_ids::Snowflake) -> Result<Connection, Unasked> {
    let mut connection = connect(found).await?;
    match connection.ask(&Request::Session { agent }).await? {
        Answer::Session => Ok(connection),
        Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
        other => Err(unread_as("a session", &other)),
    }
}

/// The channel's token, from `token` in `runtime`, once both are judged this
/// user's alone.
fn token_of(runtime: &Path, token: &Path, user: &User) -> Result<Token, Unasked> {
    for (path, kind) in [(runtime, Kind::Directory), (token, Kind::File)] {
        match nc_platform::judge(path, kind, user) {
            Ok(()) => {}
            Err(Judged::Refused(refused)) => {
                return Err(Unasked::TokenNotPrivate(format!(
                    "{refused}, so the channel's token was not read and nothing was sent"
                )));
            }
            Err(Judged::Failed(failed)) => {
                return Err(Unasked::TokenUnreadable(format!(
                    "{failed}, so the channel's token was not read and nothing was sent"
                )));
            }
        }
    }
    let text = std::fs::read_to_string(token).map_err(|error| {
        Unasked::TokenUnreadable(format!("`{}` could not be read: {error}", token.display()))
    })?;
    let read = Token::from_text(&text);
    // What was read is the token, so it is overwritten rather than left to
    // the allocator, as far as safe Rust can say.
    let mut bytes = text.into_bytes();
    bytes.fill(0);
    core::hint::black_box(&bytes);
    read.ok_or_else(|| {
        Unasked::TokenUnreadable(format!(
            "`{}` does not hold a token as a daemon writes one, 64 lower-case hexadecimal \
             digits, so nothing was sent",
            token.display()
        ))
    })
}

/// Whether the welcome a proven daemon gave names the process the system
/// says is at the other end, `pid`, and the channel of `found`'s install.
fn held_to(welcome: &Welcome, pid: u32, found: &Found) -> Result<(), Unasked> {
    if welcome.pid != pid {
        return Err(Unasked::Misnamed(format!(
            "the daemon's welcome names process {}, and the system says process {pid} is at \
             the other end, so nothing was sent",
            welcome.pid
        )));
    }
    if welcome.channel != found.install.channel {
        return Err(Unasked::Misnamed(format!(
            "the daemon's welcome names the {} channel, and this install is {}, so nothing \
             was sent",
            welcome.channel.word(),
            found.install.channel.word()
        )));
    }
    Ok(())
}

impl Connection {
    /// What the daemon said it is.
    #[must_use]
    pub const fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// Asks `request`, and answers what the daemon answered.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`], with each refusal `nc_ipc::Client::ask` answers
    /// with.
    pub async fn ask(&mut self, request: &Request) -> Result<Answer, Unasked> {
        self.client.ask(request).await.map_err(Unasked::Asking)
    }

    /// Every command the registry holds, its texts in `locale`.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`], and [`Unasked::Unread`] for
    /// an answer that is not a registry, or an entry this build does not read.
    pub async fn registry(&mut self, locale: Locale) -> Result<Vec<Listed>, Unasked> {
        match self.ask(&Request::Registry { locale }).await? {
            Answer::Registry(entries) => entries
                .iter()
                .map(|entry| {
                    Listed::read(entry).ok_or_else(|| {
                        Unasked::Unread(
                            "the registry holds an entry with no name, display name, line, \
                             paragraph, tags, categories or argument schema written as it is \
                             read"
                                .to_owned(),
                        )
                    })
                })
                .collect(),
            Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
            other => Err(unread_as("the registry", &other)),
        }
    }

    /// Every command this connection may dispatch now, as the entries the
    /// daemon sent, their texts in `locale`.
    ///
    /// **The daemon's answer, as it is**: the entries a registry holds, cut to
    /// what the decision point would let the connection's principal dispatch
    /// — the session's agent, or the person — and read by whoever shows them.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`], [`Unasked::Refused`] with
    /// the daemon's refusal, and [`Unasked::Unread`] for an answer that is not
    /// a list of tools.
    pub async fn tools(&mut self, locale: Locale) -> Result<Vec<Value>, Unasked> {
        match self.ask(&Request::Tools { locale }).await? {
            Answer::Tools(entries) => Ok(entries),
            Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
            other => Err(unread_as("the tools", &other)),
        }
    }

    /// Runs `command` with `args`, and answers what it answered or how the
    /// kernel refused it.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`], and [`Unasked::Unread`] for
    /// an answer that is neither a result nor a refusal.
    pub async fn dispatch(&mut self, command: &str, args: Value) -> Result<Dispatched, Unasked> {
        let request = Request::Dispatch {
            command: command.to_owned(),
            args,
        };
        match self.ask(&request).await? {
            Answer::Result(payload) => Ok(Ok(payload)),
            Answer::Refused(refusal) => Ok(Err(refusal)),
            other => Err(unread_as("a dispatch", &other)),
        }
    }
}

/// An answer of another kind than the request that was asked.
fn unread_as(asked: &str, answer: &Answer) -> Unasked {
    let kind = match answer {
        Answer::Result(_) => "a result",
        Answer::Refused(_) => "a refusal",
        Answer::Registry(_) => "a registry",
        Answer::Tools(_) => "a list of tools",
        Answer::Session => "a session",
        Answer::Status(_) => "a status",
        Answer::Stopping => "a stop",
    };
    Unasked::Unread(format!(
        "{asked} was asked, and the daemon answered with {kind}"
    ))
}

/// A daemon that answers as a script says, on an install made in a scratch
/// directory: what this crate's tests connect to, so no daemon process is
/// started.
#[cfg(test)]
pub(crate) mod scripted {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    use nc_channel::found::Found;
    use nc_ipc::{Answer, Request, State, Token, Welcome};

    /// How long any one wait of a scripted daemon may take before the test
    /// fails rather than hangs.
    const WAIT: Duration = Duration::from_secs(60);

    /// An install in a scratch directory, removed however the test leaves.
    pub(crate) struct Scratch {
        /// The scratch directory.
        root: PathBuf,
        /// The install read from its manifest, its pipe named for this test.
        pub(crate) found: Found,
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
        pub(crate) fn new(tag: &str) -> Self {
            use std::io::Write as _;
            let root = std::env::temp_dir().join(format!("nc-cli-{}-{tag}", std::process::id()));
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
        pub(crate) fn path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        /// The token file written as a daemon writes it, private.
        pub(crate) fn token(&self, token: &Token) {
            self.write_token(&token.to_text());
        }

        /// The token file holding `text`, private.
        pub(crate) fn write_token(&self, text: &str) {
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
        pub(crate) fn welcome(&self) -> Welcome {
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
    pub(crate) struct Script {
        /// The token it proves itself with.
        pub(crate) token: Token,
        /// The welcome it gives.
        pub(crate) welcome: Welcome,
        /// For each connection, the answers to its requests in order.
        pub(crate) connections: Vec<Vec<Answer>>,
    }

    /// A daemon serving a script on a thread of its own.
    pub(crate) struct Scripted {
        /// The thread, which hands back every request it heard.
        thread: Option<std::thread::JoinHandle<Vec<Request>>>,
    }

    impl Scripted {
        /// Binds `installed`'s endpoint and serves `script` on it, once
        /// bound.
        pub(crate) fn serve(installed: &Scratch, script: Script) -> Self {
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
                    let mut listener =
                        nc_ipc::listen(&endpoint, &found.user, &held).expect("bound");
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
                            nc_ipc::serve(accepted.stream, &script.token, &script.welcome),
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
        pub(crate) fn heard(mut self) -> Vec<Request> {
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
    pub(crate) fn entry(name: &str, display_name: &str, args_schema: &str) -> nc_canon::Value {
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
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_catalog::{ErrorCode, Locale};
    use nc_ipc::{Answer, IpcError, Refusal, RefusalKind, Request, Token};

    use super::scripted::{Scratch, Script, Scripted, entry};
    use super::{Listed, Unasked};
    use crate::exit::Exit;

    fn token() -> Token {
        Token::fresh().expect("a token")
    }

    fn connected(installed: &Scratch) -> Result<super::Connection, Unasked> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("a runtime")
            .block_on(super::connect(&installed.found))
    }

    /// Runs `ask` over a connection to `installed`'s daemon, on a runtime of
    /// its own.
    fn over<T>(
        installed: &Scratch,
        ask: impl AsyncFnOnce(&mut super::Connection) -> Result<T, Unasked>,
    ) -> Result<T, Unasked> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("a runtime")
            .block_on(async {
                let mut connection = super::connect(&installed.found).await?;
                ask(&mut connection).await
            })
    }

    fn refused(unasked: Result<super::Connection, Unasked>) -> Unasked {
        unasked.expect_err("the connection is refused")
    }

    /// Nothing listening is a daemon that is not running: exit 9, with the
    /// endpoint's code.
    #[test]
    fn nothing_listening_is_a_daemon_not_running() {
        let installed = Scratch::new("none");
        installed.token(&token());
        let refusal = refused(connected(&installed));
        assert_eq!(refusal.code(), ErrorCode::IpcUnreachable, "{refusal:?}");
        assert_eq!(refusal.exit(), Exit::Unreachable);
    }

    /// A daemon that cannot prove it holds the token is refused, and so is the
    /// client it cannot prove: exit 10, before any request.
    #[test]
    fn a_handshake_that_proves_nobody_is_a_connection_refused() {
        let installed = Scratch::new("proof");
        installed.token(&token());
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: token(),
                welcome: installed.welcome(),
                connections: vec![Vec::new()],
            },
        );
        let refusal = refused(connected(&installed));
        assert!(matches!(refusal, Unasked::Handshake(_)), "{refusal:?}");
        assert_eq!(refusal.exit(), Exit::Refused);
        assert_eq!(
            daemon.heard(),
            [] as [Request; 0],
            "a request crossed a refused handshake"
        );
    }

    /// A token file another account can reach, or in a runtime directory it
    /// can reach, is not read, and nothing is sent: exit 10 with the CLI's
    /// code, the file left as it was.
    #[test]
    fn a_token_another_account_can_reach_is_not_read() {
        for (tag, opened) in [("tf", false), ("td", true)] {
            let installed = Scratch::new(tag);
            installed.token(&token());
            let daemon = Scripted::serve(
                &installed,
                Script {
                    token: token(),
                    welcome: installed.welcome(),
                    connections: vec![Vec::new()],
                },
            );
            let path = if opened {
                installed.found.layout.runtime.clone()
            } else {
                installed.found.layout.token.clone()
            };
            nc_platform::testing::open_to_another_user(&path).expect("opened to another user");
            let refusal = refused(connected(&installed));
            assert_eq!(refusal.code(), ErrorCode::CliTokenNotPrivate, "{refusal:?}");
            assert_eq!(refusal.exit(), Exit::Refused);
            assert!(!nc_platform::testing::is_private(&path), "left as it was");
            assert_eq!(daemon.heard(), [] as [Request; 0]);
        }
    }

    /// A token file that is missing, or that holds anything but a token as a
    /// daemon writes one, is refused with the CLI's code: exit 10.
    #[test]
    fn a_token_missing_or_malformed_is_refused() {
        let written = token().to_text();
        for (tag, text) in [
            ("m0", None),
            ("m1", Some(String::new())),
            ("m2", Some(format!("{written}\n"))),
            ("m3", Some(written.to_uppercase())),
            ("m4", Some(written[..62].to_owned())),
        ] {
            let installed = Scratch::new(tag);
            if let Some(text) = &text {
                installed.write_token(text);
            }
            let daemon = Scripted::serve(
                &installed,
                Script {
                    token: token(),
                    welcome: installed.welcome(),
                    connections: vec![Vec::new()],
                },
            );
            let refusal = refused(connected(&installed));
            assert_eq!(
                refusal.code(),
                ErrorCode::CliTokenUnreadable,
                "{tag}: {refusal:?}"
            );
            assert_eq!(refusal.exit(), Exit::Refused);
            assert!(
                !refusal.detail().contains(&written),
                "the token's text was repeated in a refusal"
            );
            assert_eq!(daemon.heard(), [] as [Request; 0]);
        }
    }

    /// A proven welcome naming another process than the system's, or another
    /// channel than the install's, is refused before any request: exit 10.
    #[test]
    fn a_welcome_naming_another_process_or_channel_is_refused() {
        for (tag, misnamed) in [("wp", true), ("wc", false)] {
            let installed = Scratch::new(tag);
            let shared = token();
            installed.token(&shared);
            let mut welcome = installed.welcome();
            if misnamed {
                welcome.pid = welcome.pid.wrapping_add(1).max(1);
            } else {
                welcome.channel = nc_channel::Channel::Production;
            }
            let daemon = Scripted::serve(
                &installed,
                Script {
                    token: shared,
                    welcome,
                    connections: vec![Vec::new()],
                },
            );
            let refusal = refused(connected(&installed));
            assert_eq!(
                refusal.code(),
                ErrorCode::CliDaemonMisnamed,
                "{tag}: {refusal:?}"
            );
            assert_eq!(refusal.exit(), Exit::Refused);
            assert_eq!(
                daemon.heard(),
                [] as [Request; 0],
                "{tag}: a request was sent"
            );
        }
    }

    /// A proven daemon is asked one request at a time, and its answers read.
    ///
    /// Its registry is read as the CLI reads an entry, a dispatch answered or
    /// refused, an answer of another kind refused as unread — exit 8 — and a
    /// registry the daemon refuses ends as its refusal does.
    #[test]
    fn a_proven_daemon_is_asked_and_its_answers_read() {
        let installed = Scratch::new("ask");
        let shared = token();
        installed.token(&shared);
        let schema = r#"{"type":"object","additionalProperties":false}"#;
        let refusal = Refusal {
            code: ErrorCode::PolicyDenied,
            kind: RefusalKind::Denied,
            sentence: "no".to_owned(),
        };
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: shared,
                welcome: installed.welcome(),
                connections: vec![vec![
                    Answer::Registry(vec![entry("a.b", "A b", schema)]),
                    Answer::Result(Value::Integer(4)),
                    Answer::Refused(refusal.clone()),
                    Answer::Stopping,
                    Answer::Registry(vec![Value::object([("name", Value::text("a.b"))])]),
                    Answer::Refused(Refusal {
                        code: ErrorCode::DaemonNotRunning,
                        kind: RefusalKind::Unavailable,
                        sentence: "recovering".to_owned(),
                    }),
                ]],
            },
        );
        let answered = over(&installed, async |connection| {
            assert_eq!(connection.welcome(), &installed.welcome());
            let registry = connection.registry(Locale::PtBR).await?;
            let result = connection
                .dispatch("a.b", Value::object::<&str>([]))
                .await?;
            let denied = connection
                .dispatch("a.b", Value::object::<&str>([]))
                .await?;
            let stopping = connection.dispatch("a.b", Value::object::<&str>([])).await;
            let unread = connection.registry(Locale::En).await;
            let starting = connection.registry(Locale::En).await;
            Ok((registry, result, denied, stopping, unread, starting))
        })
        .expect("asked");
        let (registry, result, denied, stopping, unread, starting) = answered;
        assert_eq!(registry, [read("a.b", "A b", schema)]);
        assert_eq!(result, Ok(Value::Integer(4)));
        assert_eq!(denied, Err(refusal));
        for unread in [stopping.map(|_| ()), unread.map(|_| ())] {
            let unread = unread.expect_err("an answer of another kind is refused");
            assert_eq!(unread.code(), ErrorCode::CliAnswerUnreadable, "{unread:?}");
            assert_eq!(unread.exit(), Exit::Internal);
        }
        let starting = starting.expect_err("a registry refused");
        assert_eq!(starting.code(), ErrorCode::DaemonNotRunning, "{starting:?}");
        assert_eq!(starting.exit(), Exit::Unavailable);
        assert_eq!(starting.detail(), "recovering");
        let heard = daemon.heard();
        assert_eq!(
            heard.first(),
            Some(&Request::Registry {
                locale: Locale::PtBR
            }),
            "the registry is asked in the locale the caller names"
        );
        assert_eq!(heard.len(), 6);
    }

    /// A session is opened and its tools read.
    ///
    /// **Over a scripted daemon**: a session asked for an agent is answered
    /// that it opened, and the connection is the session's; its tools are the
    /// entries the daemon sent, as it sent them, in the locale asked; tools
    /// refused are the daemon's refusal; and an answer of another kind is
    /// unread, exit 8. A session the daemon refuses ends as its refusal does,
    /// and one answered with anything but a session is unread.
    #[test]
    fn a_session_is_opened_and_its_tools_read() {
        let installed = Scratch::new("ses");
        let shared = token();
        installed.token(&shared);
        let flake = nc_ids::MachineFlake::from_value(4).expect("a flake");
        let agent = nc_ids::Minter::new(flake, nc_ids::SystemClock)
            .mint()
            .expect("minted");
        let schema = r#"{"type":"object","additionalProperties":false}"#;
        let tools = vec![entry("naming.read", "Ler um nome", schema)];
        let off = Refusal {
            code: ErrorCode::DaemonMcpOff,
            kind: RefusalKind::Denied,
            sentence: "off".to_owned(),
        };
        let full = Refusal {
            code: ErrorCode::DaemonTooManySessions,
            kind: RefusalKind::Unavailable,
            sentence: "full".to_owned(),
        };
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: shared,
                welcome: installed.welcome(),
                connections: vec![
                    vec![
                        Answer::Session,
                        Answer::Tools(tools.clone()),
                        Answer::Refused(off.clone()),
                        Answer::Registry(tools.clone()),
                    ],
                    vec![Answer::Refused(full.clone())],
                    vec![Answer::Stopping],
                ],
            },
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("a runtime");
        let (listed, refused, unread) = runtime.block_on(async {
            let mut session = super::session(&installed.found, agent)
                .await
                .expect("the session opened");
            let listed = session.tools(Locale::PtBR).await.expect("the tools read");
            let refused = session.tools(Locale::En).await;
            let unread = session.tools(Locale::En).await;
            drop(session);
            (listed, refused, unread)
        });
        assert_eq!(listed, tools, "the tools are the entries the daemon sent");
        match refused {
            Err(Unasked::Refused(refusal)) => assert_eq!(refusal, off),
            other => panic!("tools refused were read as {other:?}"),
        }
        let unread = unread.expect_err("a registry is not a list of tools");
        assert_eq!(unread.code(), ErrorCode::CliAnswerUnreadable, "{unread:?}");
        assert_eq!(unread.exit(), Exit::Internal);
        let refused = runtime
            .block_on(super::session(&installed.found, agent))
            .expect_err("a session the daemon refused");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonTooManySessions,
            "{refused:?}"
        );
        assert_eq!(refused.exit(), Exit::Unavailable);
        assert_eq!(refused.detail(), "full");
        let unread = runtime
            .block_on(super::session(&installed.found, agent))
            .expect_err("a stop is not a session");
        assert_eq!(unread.code(), ErrorCode::CliAnswerUnreadable, "{unread:?}");
        assert!(
            unread.detail().contains("a session was asked"),
            "{unread:?}"
        );
        let session = Request::Session { agent };
        assert_eq!(
            daemon.heard(),
            [
                session.clone(),
                Request::Tools {
                    locale: Locale::PtBR
                },
                Request::Tools { locale: Locale::En },
                Request::Tools { locale: Locale::En },
                session.clone(),
                session,
            ],
            "a session is asked for its agent, and its tools in the locale named"
        );
    }

    /// A request is answered or lost by how the connection ended: a daemon
    /// gone is not running, an answer the CLI cannot read is the daemon's
    /// failure, and a request too long to send is arguments refused.
    ///
    /// **Found by a review**: an answer refused as a frame after a proven
    /// handshake ended in 10, whose meaning is a connection that did not show
    /// both ends to be this install's, rather than in 8.
    #[test]
    fn a_request_not_answered_ends_by_how_the_connection_ended() {
        for (error, exit) in [
            (IpcError::Lost(String::new()), Exit::Unreachable),
            (IpcError::Silent, Exit::Unreachable),
            (IpcError::Unread, Exit::Unreachable),
            (
                IpcError::TurnedAway(ErrorCode::IpcSilent),
                Exit::Unreachable,
            ),
            (IpcError::Malformed(String::new()), Exit::Internal),
            (IpcError::TooLarge { length: 1 }, Exit::Internal),
            (IpcError::Protocol { theirs: 9 }, Exit::Refused),
            (IpcError::Unsendable { length: 1 }, Exit::Invalid),
        ] {
            assert_eq!(Unasked::Asking(error).exit(), exit);
        }
        // Nothing listening is held by a real endpoint, above; any other
        // refusal of it is a connection refused.
        assert_eq!(
            Unasked::Connecting(IpcError::Endpoint(nc_platform::ipc::Refused::NoProcess)).exit(),
            Exit::Refused
        );
    }

    /// The entry [`entry`] writes, as the CLI reads it.
    fn read(name: &str, display_name: &str, args_schema: &str) -> Listed {
        Listed {
            name: name.to_owned(),
            display_name: display_name.to_owned(),
            line: format!("{display_name}: the line"),
            paragraph: format!("{display_name}: the paragraph"),
            tags: vec!["one".to_owned(), "two".to_owned()],
            categories: vec!["noctua.history".to_owned()],
            args_schema: args_schema.to_owned(),
        }
    }

    /// An entry is read by its text fields, whatever else it holds, and one
    /// missing any of them, or holding one as another type, is not read.
    #[test]
    fn a_registry_entry_is_read_by_its_text_fields() {
        let schema = r#"{"type":"object","additionalProperties":false}"#;
        assert_eq!(
            Listed::read(&entry("a.b", "A", schema)),
            Some(read("a.b", "A", schema))
        );
        for list in ["tags", "categories"] {
            let mut fields = entry("a.b", "A", schema).into_object().expect("an object");
            fields.insert(
                list.to_owned(),
                Value::Array(vec![Value::text("one"), Value::Integer(2)]),
            );
            assert_eq!(Listed::read(&Value::Object(fields)), None, "{list}");
        }
        for missing in [
            "name",
            "display_name",
            "line",
            "paragraph",
            "tags",
            "categories",
            "args_schema",
        ] {
            let mut fields = entry("a.b", "A", schema).into_object().expect("an object");
            fields.remove(missing);
            assert_eq!(
                Listed::read(&Value::Object(fields.clone())),
                None,
                "{missing}"
            );
            fields.insert(missing.to_owned(), Value::Integer(1));
            assert_eq!(Listed::read(&Value::Object(fields)), None, "{missing}");
        }
        assert_eq!(Listed::read(&Value::text("a.b")), None);
    }
}
