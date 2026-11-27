//! The interface's two connections to its daemon, through the one client
//! every program asks a daemon through (`nc_ipc::client`, `NOCTUA.md §11.1`).
//!
//! **A watch waits, so it has a connection of its own** ([`follow`]): it asks
//! for the interface's state having seen nothing, then again naming the
//! revision each answer was at, and hands every state that arrives to its
//! caller; the other connection ([`Asker`]) carries what the window asks —
//! a command, and a note of what moves at the rate it draws. Neither holds
//! anything the daemon does not: the state is the kernel's (`§12.1`).
//!
//! **A daemon that is not there is waited for, never given up on.** A
//! connection refused, a watch that fails and a connection that closes are
//! each handed to the caller and tried again after `ipc_heartbeat_ms`, the
//! catalog's beat for a connection — the window keeps the state it held and
//! the daemon may come back (`§12.17`); a connection made again begins having
//! seen nothing, since a revision is one daemon's. The caller ends the loop,
//! and nothing else does.
//!
//! # What this answers (`NOCTUA.md §2.4`)
//!
//! Whatever `nc_ipc::client` checks before anything is sent — the endpoint's
//! other end this user's, the token private, the handshake proving both ends,
//! the welcome naming that process and this channel — is checked for both
//! connections, by that one module. A wait is bounded by the daemon at
//! `ipc_watch_wait_ms` whatever is asked, and asked at that bound here, so a
//! watch never asks to hold its connection longer than the daemon would.

use core::time::Duration;

use nc_canon::Value;
use nc_channel::found::Found;
use nc_ipc::client::{Changed, Connection, Dispatched, Unasked};

/// What following the daemon heard, in the order it happened.
#[derive(Debug)]
pub enum Heard {
    /// A connection was made, and its watch begins having seen nothing.
    Reached,
    /// The state is at a revision this connection had not seen.
    Changed(Changed),
    /// No connection was made, or the one made failed or closed: the daemon
    /// is asked again after `ipc_heartbeat_ms`.
    Lost(Unasked),
}

/// Whether to go on following.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Go on.
    Go,
    /// Stop, now.
    Stop,
}

/// How long a watch asks the daemon to wait: `ipc_watch_wait_ms`, the most
/// the daemon waits anyway.
#[must_use]
pub fn watch_wait() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.ipc_watch_wait_ms)
}

/// How long to wait before the daemon is asked again, once a connection was
/// lost or never made: `ipc_heartbeat_ms`, the catalog's beat for a
/// connection.
#[must_use]
pub fn again_after() -> Duration {
    Duration::from_millis(nc_catalog::DAEMON.ipc_heartbeat_ms)
}

/// Follows the interface's state the daemon of `found`'s install holds,
/// handing `heard` everything that happens, until it answers
/// [`Flow::Stop`].
///
/// Each state is handed over once, at the revision it is at; an answer that
/// says nothing changed is not handed over. A connection lost is tried again
/// after [`again_after`], whatever lost it.
pub async fn follow(found: &Found, heard: &mut impl FnMut(Heard) -> Flow) {
    loop {
        match nc_ipc::client::connect(found).await {
            Err(unasked) => {
                if heard(Heard::Lost(unasked)) == Flow::Stop {
                    return;
                }
            }
            Ok(connection) => {
                if heard(Heard::Reached) == Flow::Stop {
                    return;
                }
                if watched(connection, heard).await == Flow::Stop {
                    return;
                }
            }
        }
        tokio::time::sleep(again_after()).await;
    }
}

/// Watches over `connection` until it is lost, or `heard` says stop.
async fn watched(mut connection: Connection, heard: &mut impl FnMut(Heard) -> Flow) -> Flow {
    let mut seen = None;
    loop {
        match connection.watch(seen, watch_wait()).await {
            Ok(changed) => {
                seen = Some(changed.revision);
                if changed.state.is_some() && heard(Heard::Changed(changed)) == Flow::Stop {
                    return Flow::Stop;
                }
            }
            Err(unasked) => return heard(Heard::Lost(unasked)),
        }
    }
}

/// The connection the window asks through: a command, and a note.
///
/// **Made when it is first asked through, and made again after it fails**,
/// so a daemon that went away and came back is reached by the next request,
/// with nothing to tell it apart from the first.
#[derive(Debug)]
pub struct Asker {
    /// The install whose daemon is asked.
    found: Found,
    /// The connection, while one is held.
    connection: Option<Connection>,
}

impl Asker {
    /// An asker of the daemon of `found`'s install, holding no connection yet.
    #[must_use]
    pub const fn new(found: Found) -> Self {
        Self {
            found,
            connection: None,
        }
    }

    /// The connection held, made first when none is.
    async fn connection(&mut self) -> Result<&mut Connection, Unasked> {
        if self.connection.is_none() {
            self.connection = Some(nc_ipc::client::connect(&self.found).await?);
        }
        self.connection
            .as_mut()
            .ok_or_else(|| Unasked::Unread("a connection was made and is not held".to_owned()))
    }

    /// The result of what was asked over the connection, which is let go
    /// when it failed, so the next request makes another — and kept when
    /// the daemon answered with a refusal, which says the connection works.
    fn kept<T>(&mut self, asked: Result<T, Unasked>) -> Result<T, Unasked> {
        if asked
            .as_ref()
            .is_err_and(|unasked| !matches!(unasked, Unasked::Refused(_)))
        {
            self.connection = None;
        }
        asked
    }

    /// Runs `command` with `args`, as the person, and answers what it
    /// answered or how the kernel refused it.
    ///
    /// # Errors
    ///
    /// What `nc_ipc::client::connect` refuses, and what
    /// `nc_ipc::client::Connection::dispatch` does.
    pub async fn dispatch(&mut self, command: &str, args: Value) -> Result<Dispatched, Unasked> {
        let asked = match self.connection().await {
            Ok(connection) => connection.dispatch(command, args).await,
            Err(unasked) => Err(unasked),
        };
        self.kept(asked)
    }

    /// Every command the daemon's registry holds — the catalog's and every
    /// loaded plugin's — its texts in `locale`.
    ///
    /// # Errors
    ///
    /// What `nc_ipc::client::connect` refuses, and what
    /// `nc_ipc::client::Connection::registry` does.
    pub async fn registry(
        &mut self,
        locale: nc_catalog::Locale,
    ) -> Result<Vec<nc_ipc::client::Listed>, Unasked> {
        let asked = match self.connection().await {
            Ok(connection) => connection.registry(locale).await,
            Err(unasked) => Err(unasked),
        };
        self.kept(asked)
    }

    /// Tells the daemon `note`, which its kernel applies whole or not at all.
    ///
    /// # Errors
    ///
    /// What `nc_ipc::client::connect` refuses, and what
    /// `nc_ipc::client::Connection::note` does — the daemon's refusal among
    /// them.
    pub async fn note(&mut self, note: nc_ipc::Note) -> Result<(), Unasked> {
        let asked = match self.connection().await {
            Ok(connection) => connection.note(note).await,
            Err(unasked) => Err(unasked),
        };
        self.kept(asked)
    }
}
