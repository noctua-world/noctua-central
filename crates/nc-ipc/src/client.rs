//! A client's end of a connection to its channel's daemon: the endpoint
//! reached, the token read, the handshake, and one request at a time
//! (`NOCTUA.md §11.1`).
//!
//! **One client for every program that asks a daemon**: the CLI and the
//! desktop interface connect through this module and no other, so what is
//! checked before anything is sent is checked once, and a refusal reads the
//! same from either. Its codes are `NC-CLI-`, the names the catalog gave them
//! when the CLI was their one reader. What a run that ends in a refusal
//! exits with is the program's own to say; this says the code and the
//! sentence.
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
//! 3. **The handshake**, [`crate::connect`]: the daemon proves it holds the
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
//! the request's is refused (`NC-CLI-0005`). **A watch is a request like the
//! others, and waits** ([`Connection::watch`]): its connection asks nothing
//! else until it is answered, so a client that watches and dispatches holds
//! two connections, and begins a new connection's watch having seen nothing —
//! a revision is one daemon's, and the daemon that answers after a restart is
//! another. A connection that opens an
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
//! to the record process control reads.

use std::path::Path;

use nc_canon::Value;
use nc_catalog::{ErrorCode, Locale};
use nc_channel::found::Found;
use nc_platform::{Judged, Kind, User};

use crate::{Answer, Graphical, IpcError, Note, Refusal, Request, Token, Welcome};

/// A command the registry holds, as a client reads it: its name, its entry's
/// texts in the locale the registry was asked in, its categories and its
/// argument schema's text.
///
/// **The fields a client shows of an entry, and nothing else**: an entry is
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
    /// The daemon answered with something this client does not read; the
    /// text says what.
    Unread(String),
    /// The daemon refused what was asked.
    ///
    /// It refuses the registry, the tools, a watch or a note asked before
    /// its kernel runs or once it is stopping — a watch that was waiting when
    /// a stop began among them; the registry, a watch, a note or a display
    /// asked on a session (`NC-DMN-0015`);
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

    /// What happened, in English, without the code in front of it.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Connecting(error) | Self::Handshake(error) | Self::Asking(error) => {
                without_code(&error.to_string(), error.code())
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

/// `said` without the `[<code>] ` an error's own text opens with, which
/// whoever shows the refusal writes once, in front.
fn without_code(said: &str, code: ErrorCode) -> String {
    said.strip_prefix(&format!("[{}] ", code.code()))
        .unwrap_or(said)
        .to_owned()
}

/// What a watch was answered: the revision the watched state is at, and the
/// state where the client does not hold it already.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    /// The revision the state is at, which the next watch names as seen.
    pub revision: u64,
    /// The state at that revision, as one object; nothing where the wait
    /// ended with the state as the client holds it.
    pub state: Option<Value>,
}

/// A connection to the daemon, proven and held to the install.
#[derive(Debug)]
pub struct Connection {
    /// The client's end.
    client: crate::Client,
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
    let (client, welcome) = crate::connect(stream, &token)
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
    /// [`Unasked::Asking`], with each refusal [`crate::Client::ask`] answers
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

    /// Waits until the state the daemon lets a client watch is at another
    /// revision than `seen`, for `wait` at most, and answers where it is.
    ///
    /// **`seen` is the revision this connection's last watch answered, and
    /// nothing on a connection's first**: the answer then holds the state.
    /// The daemon waits `ipc_watch_wait_ms` at most, whatever `wait` says,
    /// and an answer with no state says the wait ended with nothing changed.
    /// Nothing else is asked on this connection until the answer arrives.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`]; [`Unasked::Refused`] with
    /// the daemon's refusal; and [`Unasked::Unread`] for an answer that is
    /// not a watch's, or that names another revision than `seen` and sends
    /// no state of it — which would leave the client holding a revision it
    /// was never shown.
    pub async fn watch(
        &mut self,
        seen: Option<u64>,
        wait: core::time::Duration,
    ) -> Result<Changed, Unasked> {
        let request = Request::Watch {
            seen,
            wait_ms: u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
        };
        match self.ask(&request).await? {
            Answer::Changed {
                revision,
                state: None,
            } if seen != Some(revision) => Err(Unasked::Unread(format!(
                "a watch was asked, and the daemon answered that its state is at revision \
                 {revision} without sending it"
            ))),
            Answer::Changed { revision, state } => Ok(Changed { revision, state }),
            Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
            other => Err(unread_as("a watch", &other)),
        }
    }

    /// Tells the daemon `note`: what an interface says at the rate it draws,
    /// which is dispatched and journaled nowhere — the daemon's kernel applies
    /// it to the interface's state whole or not at all, each fact decided as
    /// its command would be.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`], [`Unasked::Refused`] with
    /// the daemon's refusal, and [`Unasked::Unread`] for any other answer.
    pub async fn note(&mut self, note: Note) -> Result<(), Unasked> {
        match self.ask(&Request::Note(note)).await? {
            Answer::Noted => Ok(()),
            Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
            other => Err(unread_as("a note", &other)),
        }
    }

    /// Announces `environment` as the graphical environment a window for the
    /// person would be shown in.
    ///
    /// # Errors
    ///
    /// [`Unasked::Asking`] as [`Connection::ask`], [`Unasked::Refused`] with
    /// the daemon's refusal, and [`Unasked::Unread`] for any other answer.
    pub async fn display(&mut self, environment: Graphical) -> Result<(), Unasked> {
        match self.ask(&Request::Display(environment)).await? {
            Answer::Displayed => Ok(()),
            Answer::Refused(refusal) => Err(Unasked::Refused(refusal)),
            other => Err(unread_as("a display", &other)),
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
        Answer::Changed { .. } => "a watch's answer",
        Answer::Noted => "a note taken",
        Answer::Displayed => "a display taken",
    };
    Unasked::Unread(format!(
        "{asked} was asked, and the daemon answered with {kind}"
    ))
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_catalog::{ErrorCode, Locale};

    use super::{Changed, Listed, Unasked};
    use crate::testing::{Scratch, Script, Scripted, entry};
    use crate::{Answer, Graphical, Note, Refusal, RefusalKind, Request, Token};

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

    /// Nothing listening is told apart, by the endpoint's code: a daemon that
    /// is not running.
    #[test]
    fn nothing_listening_is_a_daemon_not_running() {
        let installed = Scratch::new("none");
        installed.token(&token());
        let refusal = refused(connected(&installed));
        assert_eq!(refusal.code(), ErrorCode::IpcUnreachable, "{refusal:?}");
    }

    /// A daemon that cannot prove it holds the token is refused, and so is the
    /// client it cannot prove, before any request.
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
        assert_eq!(
            daemon.heard(),
            [] as [Request; 0],
            "a request crossed a refused handshake"
        );
    }

    /// A token file another account can reach, or in a runtime directory it
    /// can reach, is not read, and nothing is sent: refused with the token's
    /// own code, the file left as it was.
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
            assert!(!nc_platform::testing::is_private(&path), "left as it was");
            assert_eq!(daemon.heard(), [] as [Request; 0]);
        }
    }

    /// A token file that is missing, or that holds anything but a token as a
    /// daemon writes one, is refused with its own code.
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
            assert!(
                !refusal.detail().contains(&written),
                "the token's text was repeated in a refusal"
            );
            assert_eq!(daemon.heard(), [] as [Request; 0]);
        }
    }

    /// A proven welcome naming another process than the system's, or another
    /// channel than the install's, is refused before any request.
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
    /// refused, an answer of another kind refused as unread, and a registry
    /// the daemon refuses is its refusal.
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
        }
        let starting = starting.expect_err("a registry refused");
        assert_eq!(starting.code(), ErrorCode::DaemonNotRunning, "{starting:?}");
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
    /// unread. A session the daemon refuses is its refusal, and one answered
    /// with anything but a session is unread.
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
        let refused = runtime
            .block_on(super::session(&installed.found, agent))
            .expect_err("a session the daemon refused");
        assert_eq!(
            refused.code(),
            ErrorCode::DaemonTooManySessions,
            "{refused:?}"
        );
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

    /// A watch is answered the state, or that nothing changed, and nothing
    /// else is read as its answer.
    ///
    /// **Over a scripted daemon**: a first watch names no revision and is
    /// handed the state with its revision; one naming that revision is told
    /// nothing changed; an answer naming another revision without its state —
    /// or any revision to a watch that had seen none — is unread, since the
    /// client would hold a revision it was never shown; a refusal is the
    /// daemon's; and an answer of another kind is unread. Each is asked with
    /// the revision and the wait the caller named, a wait past what the wire
    /// holds as the most it holds.
    #[test]
    fn a_watch_is_answered_the_state_or_that_nothing_changed() {
        use core::time::Duration;
        let installed = Scratch::new("watch");
        let shared = token();
        installed.token(&shared);
        let state = Value::object([("shown", Value::Bool(true))]);
        let stopping = Refusal {
            code: ErrorCode::DaemonStopping,
            kind: RefusalKind::Unavailable,
            sentence: "stopping".to_owned(),
        };
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: shared,
                welcome: installed.welcome(),
                connections: vec![vec![
                    Answer::Changed {
                        revision: 7,
                        state: Some(state.clone()),
                    },
                    Answer::Changed {
                        revision: 7,
                        state: None,
                    },
                    Answer::Changed {
                        revision: 9,
                        state: None,
                    },
                    Answer::Changed {
                        revision: 0,
                        state: None,
                    },
                    Answer::Refused(stopping.clone()),
                    Answer::Noted,
                ]],
            },
        );
        let (first, same, unshown, never_shown, refused, unread) =
            over(&installed, async |connection| {
                let first = connection.watch(None, Duration::from_millis(250)).await?;
                let same = connection.watch(Some(7), Duration::MAX).await?;
                let unshown = connection.watch(Some(7), Duration::ZERO).await;
                let never_shown = connection.watch(None, Duration::ZERO).await;
                let refused = connection.watch(Some(7), Duration::ZERO).await;
                let unread = connection.watch(Some(7), Duration::ZERO).await;
                Ok((first, same, unshown, never_shown, refused, unread))
            })
            .expect("asked");
        assert_eq!(
            first,
            Changed {
                revision: 7,
                state: Some(state)
            }
        );
        assert_eq!(
            same,
            Changed {
                revision: 7,
                state: None
            }
        );
        for (answered, revision) in [(unshown, 9), (never_shown, 0)] {
            let answered = answered.expect_err("a revision named and not shown is unread");
            assert_eq!(
                answered.code(),
                ErrorCode::CliAnswerUnreadable,
                "{answered:?}"
            );
            assert_eq!(
                answered.detail(),
                format!(
                    "a watch was asked, and the daemon answered that its state is at revision \
                     {revision} without sending it"
                )
            );
        }
        match refused {
            Err(Unasked::Refused(refusal)) => assert_eq!(refusal, stopping),
            other => panic!("a watch refused was read as {other:?}"),
        }
        let unread = unread.expect_err("a note taken is not a watch's answer");
        assert_eq!(unread.code(), ErrorCode::CliAnswerUnreadable, "{unread:?}");
        assert_eq!(
            unread.detail(),
            "a watch was asked, and the daemon answered with a note taken"
        );
        let asked = |seen, wait_ms| Request::Watch { seen, wait_ms };
        assert_eq!(
            daemon.heard(),
            [
                asked(None, 250),
                asked(Some(7), u64::try_from(i64::MAX).expect("positive")),
                asked(Some(7), 0),
                asked(None, 0),
                asked(Some(7), 0),
                asked(Some(7), 0),
            ],
            "a watch is asked with the revision seen and the wait named"
        );
    }

    /// A note and a display are told, and nothing else is read as taken.
    ///
    /// **Over a scripted daemon**: each is sent as it was made and answered
    /// that it was taken; a refusal is the daemon's; and the other's answer is
    /// unread.
    #[test]
    fn a_note_and_a_display_are_told_and_nothing_else_is_read_as_taken() {
        let installed = Scratch::new("note");
        let shared = token();
        installed.token(&shared);
        let not_a_sessions = Refusal {
            code: ErrorCode::DaemonNotForASession,
            kind: RefusalKind::Denied,
            sentence: "no".to_owned(),
        };
        let daemon = Scripted::serve(
            &installed,
            Script {
                token: shared,
                welcome: installed.welcome(),
                connections: vec![vec![
                    Answer::Noted,
                    Answer::Refused(not_a_sessions.clone()),
                    Answer::Displayed,
                    Answer::Displayed,
                    Answer::Refused(not_a_sessions.clone()),
                    Answer::Noted,
                ]],
            },
        );
        let note = Note::new(Value::object([("scroll", Value::Integer(3))])).expect("a note");
        let environment = Graphical::new([("WAYLAND_DISPLAY", "wayland-1")]).expect("a display");
        let answered = over(&installed, async |connection| {
            let mut answered = Vec::new();
            for _ in 0..3 {
                answered.push(connection.note(note.clone()).await);
            }
            for _ in 0..3 {
                answered.push(connection.display(environment.clone()).await);
            }
            Ok(answered)
        })
        .expect("asked");
        for (at, said) in [
            (
                2,
                "a note was asked, and the daemon answered with a display taken",
            ),
            (
                5,
                "a display was asked, and the daemon answered with a note taken",
            ),
        ] {
            answered[at - 2].as_ref().expect("taken");
            match &answered[at - 1] {
                Err(Unasked::Refused(refusal)) => assert_eq!(refusal, &not_a_sessions),
                other => panic!("a refusal was read as {other:?}"),
            }
            let unread = answered[at].as_ref().expect_err("the other's answer");
            assert_eq!(unread.code(), ErrorCode::CliAnswerUnreadable, "{unread:?}");
            assert_eq!(unread.detail(), said);
        }
        let told = Request::Note(note);
        let announced = Request::Display(environment);
        assert_eq!(
            daemon.heard(),
            [
                told.clone(),
                told.clone(),
                told,
                announced.clone(),
                announced.clone(),
                announced
            ]
        );
    }

    /// Every answer the protocol holds is named when it is not the one
    /// asked for.
    ///
    /// **By a word of its own**, so a refusal says which answer arrived: no
    /// two answers share one.
    #[test]
    fn every_answer_is_named_by_a_word_of_its_own_when_it_was_not_asked_for() {
        let refusal = Refusal {
            code: ErrorCode::PolicyDenied,
            kind: RefusalKind::Denied,
            sentence: String::new(),
        };
        let all = [
            Answer::Result(Value::Null),
            Answer::Refused(refusal),
            Answer::Registry(Vec::new()),
            Answer::Tools(Vec::new()),
            Answer::Session,
            Answer::Status(Value::object::<&str>([])),
            Answer::Stopping,
            Answer::Changed {
                revision: 1,
                state: None,
            },
            Answer::Noted,
            Answer::Displayed,
        ];
        let said: Vec<String> = all
            .iter()
            .map(|answer| super::unread_as("a thing", answer).detail())
            .collect();
        for (at, one) in said.iter().enumerate() {
            let kind = one
                .strip_prefix("a thing was asked, and the daemon answered with ")
                .unwrap_or_else(|| panic!("{one}"));
            assert!(!kind.is_empty(), "{one}");
            assert!(
                !said[..at].contains(one),
                "two answers are both called {kind}"
            );
        }
    }

    /// The entry [`entry`] writes, as a client reads it.
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
