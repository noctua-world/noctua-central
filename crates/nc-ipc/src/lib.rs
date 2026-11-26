//! The protocol a daemon is reached through: frames of canonical JSON, a
//! handshake in which each end proves it holds the channel's token without
//! sending it, one request at a time, and a heartbeat (`NOCTUA.md §11.1`).
//!
//! # The protocol, in order
//!
//! - **A frame** ([`frame`]) is a length — four bytes, big-endian — and that
//!   many bytes of canonical JSON. A length of zero, or above
//!   `ipc_frame_max_bytes`, is refused from the prefix alone, before anything
//!   is allocated for it, and text that is not exactly what `nc-canon` writes
//!   for the value it parses to is refused too: there is one spelling of every
//!   message. A value too long for a frame is refused to its sender, and
//!   nothing of it is sent.
//! - **The handshake** ([`serve`], [`connect`]). The daemon sends a challenge:
//!   the protocol it speaks and a nonce of its own. The client answers with
//!   its nonce and its proof, BLAKE3 keyed by the token over the client's
//!   domain and both nonces. The daemon checks the protocol and the proof and
//!   answers with its own proof, keyed the same way over its own domain, and
//!   its [`Welcome`]: its process, its instance, its channel and its state.
//!   The client checks that proof before it sends anything else. Both proofs
//!   are compared in constant time, and **the token never crosses the wire**:
//!   each end shows it holds the token without sending it, so a daemon learns
//!   nothing a client did not already know and an impostor nothing at all.
//!   The whole of it is bounded by `ipc_handshake_ms` at either end.
//! - **Requests** ([`Request`], [`Answer`]): a dispatch, the registry, the
//!   tools, a session, the status, a stop, a watch, a note, or a display, one
//!   at a time — a request
//!   before the last one was answered is refused, and so is an answer nobody
//!   asked for. A dispatch is decided as the connection's principal: the
//!   profile's person, or the agent of the session the connection opened, which
//!   a session request names by the one spelling its identifier has.
//! - **The watch** ([`Request::Watch`], [`Answer::Changed`]) is how a client
//!   learns that something changed without the daemon ever sending a frame
//!   nobody asked for: the client names the revision of the state it holds —
//!   or none — and how long it will wait, and the daemon answers at once
//!   where its state is at another revision, with that revision and the
//!   state; otherwise at the first change, the same way; or once the wait is
//!   over, with the revision alone. The daemon waits `ipc_watch_wait_ms` at
//!   most, whatever the client names. **It is one request, so nothing else is
//!   asked on its connection while it waits**: a client that also dispatches
//!   holds a second connection for its watch. A revision is compared for
//!   being another one, never for being later: it is one start's, it begins
//!   at zero and turns to zero after the largest whole number the wire
//!   writes, and a client holding one of a daemon that has since started
//!   again may be holding the number the new one is at — so a client begins
//!   each connection's watch having seen nothing
//!   ([`client::Connection::watch`]). What the daemon serves of this, and
//!   what the watched state holds, is its own to say
//!   (`apps/daemon/src/server.rs`).
//! - **A note** ([`Note`], [`Answer::Noted`]) is what an interface says at
//!   the rate it draws — a scroll position, a prompt's draft, what a pointer
//!   is over — as one object of `ipc_note_max_bytes` at most. It is not a
//!   command: nothing of it is dispatched or journaled. The daemon hands it to
//!   its kernel, which reads it as the facts of the interface's state it
//!   names, each decided as the command it is the equivalent of would be, and
//!   applies it whole or not at all (`nc_kernel::Kernel::note`).
//! - **A display** ([`Graphical`], [`Answer::Displayed`]) announces the
//!   graphical environment a window for the person would be shown in: some of
//!   `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY` and `XDG_RUNTIME_DIR` and no
//!   other name, one of the first two at least, each value at most
//!   `ipc_display_value_max_bytes`, not empty and free of control characters,
//!   and the two paths absolute. The daemon holds the one announced last.
//! - **The heartbeat** ([`Client`], [`Served`]): each side pings after
//!   `ipc_heartbeat_ms` of its own silence, while a dispatch runs as well as
//!   between requests, and drops a peer silent for `ipc_silent_after_ms`, or
//!   one that leaves a write to it waiting as long — which a write does only
//!   once the system's buffer for the connection is full.
//!
//! **Before closing for a reason it can name, a side says goodbye** with the
//! code, so the other end refuses with the daemon's word rather than a closed
//! connection. Every refusal is an [`IpcError`], which carries its code
//! (`§15.1`).
//!
//! # The client ([`client`]) and the Leader ([`leader`])
//!
//! What every program that asks a daemon shares — the CLI, and the desktop
//! interface: the endpoint reached, the token read once its file is judged
//! the user's alone, the handshake, the welcome held to the process the
//! system shows and to the install's channel, and then each request with its
//! answer held to the request's kind. [`testing`], behind `test-support`, is
//! the scripted daemon their suites connect to. What a press of the Leader is
//! sent as, which process of the install listens for one on each system, and
//! how a refusal of it is told are [`leader`]'s, shared by the daemon, the CLI
//! and the desktop interface alike.
//!
//! # What protocol 4 does not hold yet
//!
//! **Protocol 4 is unreleased while Stage 16 is built, and three messages
//! join it under the same number**: `attach`, by which the desktop the daemon
//! started proves it is that one, and `confirm` and `decline`, by which that
//! desktop alone settles a call held for the person's confirmation. No build
//! of this protocol reads them yet, and none is stubbed: until they are
//! written, each is refused as every message no request is — malformed — and
//! a command declared irreversible is refused as it was under protocol 3.
//!
//! **This is the one exception to a new message being a new number**, and
//! `AGENTS.md` writes it beside the rule (*an IPC message*): it holds only
//! for a protocol no release carries, and only until the Stage that builds
//! it closes. **What it costs** is a development install whose daemon was
//! built from an earlier commit of protocol 4 than its client: the two pass
//! the handshake, and the daemon closes the connection at the first of the
//! three it does not read. `cargo xtask dev --install` replaces both ends,
//! and no released build is ever in that place — a release that carries 4
//! carries all of it.
//!
//! # What this answers (`§2.4`)
//!
//! Something on this machine that did not read the channel's token, talking to
//! the daemon or standing in for it: a client that cannot prove it holds the
//! token is turned away before a request is read, and a client never sends a
//! request to what cannot prove it to the client; the token itself is never
//! sent, so neither end learns it from the other; a peer cannot make the
//! reader allocate more than one frame's bound, queue more than one request,
//! hold a connection open without speaking, or leave a write to it waiting
//! past the silence bound.
//!
//! # What it does not
//!
//! A process running as the same user, which can read the token from the
//! runtime directory and speak for anybody — authorization is what resists it
//! (`§14`); the channel's confidentiality from such a process, which is the
//! endpoint's (`nc_platform::ipc`): nothing here is encrypted; and a peer that
//! pings and reads nothing, which holds its connection until this side's
//! writes fill the system's buffer for it — between requests, a ping each
//! `ipc_heartbeat_ms`, so minutes on a Linux socket and hours on a Windows
//! pipe.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

pub mod client;
mod connection;
pub mod frame;
mod handshake;
pub mod leader;
mod message;
pub mod testing;

pub use connection::{Client, Served};
pub use handshake::{connect, serve};
pub use message::{Answer, Graphical, Note, Refusal, RefusalKind, Request, State, Welcome};

use nc_catalog::ErrorCode;

/// The version of the protocol this build speaks, which each end names in its
/// handshake and the other refuses unless it is its own.
///
/// **4 since a client may watch, note and announce a display**: a daemon of 3
/// reads each of the three as malformed and closes the connection, and a
/// client of 3 would be the one end of a channel that cannot be told what
/// changed, so the two builds are refused at the handshake rather than break
/// at the first watch. **What a client of 3 sees of a daemon of 4** is the
/// refusal every other version sees, before it sends anything: the daemon's
/// challenge names 4, the client reads that first and ends with
/// [`IpcError::Protocol`] (`NC-IPC-0004`), saying the other end speaks
/// version 4 and it speaks 3 — for the CLI, a connection refused, exit 10 —
/// and a client of 4 ends the same way at a daemon of 3's challenge. A
/// daemon handed a hello naming 3 refuses it by the same code and says so in
/// its goodbye. `cargo xtask dev --install` replaces both.
///
/// (3 since a connection may open an agent's session, after which each
/// dispatch on it is decided as the connection's principal — the session's
/// agent — and a session asks for its tools: a client of 2 would mean a
/// dispatch as the person on a connection the daemon decides as an agent. 2
/// since the registry is asked in a locale.)
pub const PROTOCOL: u32 = 4;

/// The secret a channel's connections are proven by: 32 bytes the daemon
/// draws at each start, which neither end ever sends.
///
/// **Its `Debug` prints nothing of it**, and it is overwritten when dropped,
/// as far as safe Rust can say.
pub struct Token([u8; 32]);

impl Token {
    /// A token drawn from the system's own generator.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the system's generator did not answer.
    pub fn fresh() -> std::io::Result<Self> {
        let mut bytes = [0_u8; 32];
        nc_platform::random(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// The token as its file holds it: 64 lower-case hexadecimal digits.
    #[must_use]
    pub fn to_text(&self) -> String {
        nc_canon::to_hex(&self.0)
    }

    /// The token its file holds, or nothing for text that is not one.
    ///
    /// **Exactly 64 lower-case hexadecimal digits**, as [`Token::to_text`]
    /// writes it, so one token has one spelling.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let bytes = nc_canon::from_hex(text).ok()?;
        <[u8; 32]>::try_from(bytes.as_slice()).ok().map(Self)
    }

    /// The proof `domain` makes for the two nonces, keyed by this token.
    ///
    /// **Over both nonces and a domain of its own for each end**, so neither
    /// a proof replayed from another connection nor one end's proof reflected
    /// back is the other end's.
    fn prove(&self, domain: &str, daemons: &[u8; 32], clients: &[u8; 32]) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new_keyed(&self.0);
        hasher.update(domain.as_bytes());
        hasher.update(&[0]);
        hasher.update(daemons);
        hasher.update(clients);
        hasher.finalize()
    }
}

impl core::fmt::Debug for Token {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Token(..)")
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        self.0 = [0; 32];
        // A write nothing reads again may be removed by the optimiser; this
        // asks it not to, which no compiler promises to honour.
        core::hint::black_box(&self.0);
    }
}

/// Why a connection was refused, closed or not made.
#[derive(Debug)]
pub enum IpcError {
    /// A frame read claimed more than `ipc_frame_max_bytes`.
    TooLarge {
        /// What its prefix claimed.
        length: u64,
    },
    /// A value this side was to send is longer than `ipc_frame_max_bytes`
    /// as a frame, so nothing of it was sent.
    Unsendable {
        /// How long its frame's body would have been.
        length: u64,
    },
    /// A frame that is not one message of the protocol, or one where the
    /// protocol does not read it; the text says which.
    Malformed(String),
    /// The connection closed, or was lost, before what was being read or
    /// written was whole; the text says what.
    Lost(String),
    /// The other end speaks another version of the protocol.
    Protocol {
        /// The version it named.
        theirs: i64,
    },
    /// The client's proof is not keyed by the channel's token.
    ClientUnproven,
    /// The daemon's proof is not keyed by the channel's token.
    DaemonUnproven,
    /// The handshake did not finish within `ipc_handshake_ms`.
    Slow,
    /// The peer was silent for `ipc_silent_after_ms`.
    Silent,
    /// A write to the peer waited `ipc_silent_after_ms`, the system's buffer
    /// for the connection full and the peer taking nothing from it.
    Unread,
    /// The other end closed the connection saying why, by its code.
    TurnedAway(ErrorCode),
    /// The endpoint refused: to be bound, to accept, or to be reached.
    Endpoint(nc_platform::ipc::Refused),
}

impl IpcError {
    /// The catalog code this maps to, which is what a log is queried by.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        use nc_platform::ipc::Refused;
        match self {
            Self::TooLarge { .. } => ErrorCode::IpcFrameTooLarge,
            Self::Unsendable { .. } => ErrorCode::IpcUnsendable,
            Self::Malformed(_) => ErrorCode::IpcMalformed,
            Self::Lost(_) => ErrorCode::IpcLost,
            Self::Protocol { .. } => ErrorCode::IpcProtocol,
            Self::ClientUnproven => ErrorCode::IpcClientUnproven,
            Self::DaemonUnproven => ErrorCode::IpcDaemonUnproven,
            Self::Slow => ErrorCode::IpcHandshakeSlow,
            Self::Silent => ErrorCode::IpcSilent,
            Self::Unread => ErrorCode::IpcUnread,
            Self::TurnedAway(code) => *code,
            Self::Endpoint(refused) => match refused {
                Refused::NotPrivate(_) => ErrorCode::IpcEndpointNotPrivate,
                Refused::TooLong { .. } => ErrorCode::IpcEndpointTooLong,
                Refused::Taken { .. } => ErrorCode::IpcEndpointTaken,
                Refused::Peer { .. } | Refused::NoProcess => ErrorCode::IpcPeerUnverified,
                Refused::Busy { .. } => ErrorCode::IpcBusy,
                Refused::Nobody(_) => ErrorCode::IpcUnreachable,
                Refused::Failed(_) => ErrorCode::IpcEndpointUnavailable,
            },
        }
    }
}

impl From<nc_platform::ipc::Refused> for IpcError {
    fn from(refused: nc_platform::ipc::Refused) -> Self {
        Self::Endpoint(refused)
    }
}

impl core::fmt::Display for IpcError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "[{}] ", self.code().code())?;
        match self {
            Self::TooLarge { length } => write!(
                formatter,
                "a frame claimed {length} bytes, and `ipc_frame_max_bytes` is {}; it was \
                 refused from its prefix, before any of it was read",
                nc_catalog::DAEMON.ipc_frame_max_bytes
            ),
            Self::Unsendable { length } => write!(
                formatter,
                "a value to be sent is {length} bytes as a frame, and `ipc_frame_max_bytes` \
                 is {}; nothing of it was sent",
                nc_catalog::DAEMON.ipc_frame_max_bytes
            ),
            Self::Malformed(what) | Self::Lost(what) => write!(formatter, "{what}"),
            Self::Protocol { theirs } => write!(
                formatter,
                "the other end speaks version {theirs} of the protocol and this build speaks \
                 {PROTOCOL}"
            ),
            Self::ClientUnproven => write!(
                formatter,
                "the client's proof is not keyed by the channel's token, so it was turned \
                 away before a request was read"
            ),
            Self::DaemonUnproven => write!(
                formatter,
                "what answered at the endpoint did not prove it holds the channel's token, \
                 so nothing was sent to it"
            ),
            Self::Slow => write!(
                formatter,
                "the handshake did not finish within `ipc_handshake_ms`, {} milliseconds",
                nc_catalog::DAEMON.ipc_handshake_ms
            ),
            Self::Silent => write!(
                formatter,
                "the other end said nothing for `ipc_silent_after_ms`, {} milliseconds, so \
                 the connection was dropped",
                nc_catalog::DAEMON.ipc_silent_after_ms
            ),
            Self::Unread => write!(
                formatter,
                "the other end took nothing written to it for `ipc_silent_after_ms`, {} \
                 milliseconds, so the connection was dropped",
                nc_catalog::DAEMON.ipc_silent_after_ms
            ),
            Self::TurnedAway(code) => write!(
                formatter,
                "the other end closed the connection with {}",
                code.code()
            ),
            Self::Endpoint(refused) => write!(formatter, "{refused}"),
        }
    }
}

impl std::error::Error for IpcError {}

/// The listener a daemon serves its endpoint on, for `user`, holding the
/// connections `ipc_connections_max` allows.
///
/// **The bound is the catalog's**, read here, so no caller chooses how many
/// connections a daemon holds; the `Held` is the profile's instance lock, as
/// `nc_platform::ipc::Listener::bind` asks for it.
///
/// # Errors
///
/// [`IpcError::Endpoint`], with each refusal the endpoint answers with.
pub fn listen(
    endpoint: &nc_platform::ipc::Endpoint,
    user: &nc_platform::User,
    instance: &nc_platform::Held,
) -> Result<nc_platform::ipc::Listener, IpcError> {
    let connections = usize::try_from(nc_catalog::DAEMON.ipc_connections_max).unwrap_or(0);
    Ok(nc_platform::ipc::Listener::bind(
        endpoint,
        user,
        instance,
        connections,
    )?)
}
