//! The endpoint a daemon is reached through, on this system: a Unix domain
//! socket on Linux and macOS and a named pipe on Windows, each this user's
//! alone, and the user and the process at the other end of every connection,
//! read from the system both ways (`NOCTUA.md §11.1`, `§2.7`).
//!
//! # What each system's endpoint is
//!
//! - **On every system** a listener judges the runtime directory this user's
//!   alone before it binds — the token and the PID record sit there beside the
//!   socket — and refuses a name longer than the system holds, by the limit.
//! - **Unix.** The socket is made in that directory and set `0600`. Something
//!   already at its name is replaced only when it is a socket this user owns
//!   and nothing answers at it — what a daemon that stopped without removing
//!   its own leaves — and only by a caller holding the profile's instance lock;
//!   a link, a file, another user's socket and a socket something answers at
//!   are each refused and left as they were.
//! - **Windows.** The pipe is created with the first-instance flag, so a name
//!   another process made first is refused rather than joined, and every
//!   instance with the descriptor a private file is made with: owned by the
//!   user, a protected access list holding one entry, the user. Remote clients
//!   are rejected and the instances are bounded. The listener reads the client's
//!   process and the user its token names in one synchronous call; the client
//!   opens the pipe at the identification level, so a server cannot act as it,
//!   and reads the pipe's owner and the server's process and user before
//!   anything is sent.
//!
//! **The comparisons are made here, in code every system runs**: each half
//! reports what the system said — who the other end runs as, who owns the
//! pipe, which process it is — and `admitted` holds it to this process's
//! user, so the decision is the one Linux's mutation runner judges, and the
//! Windows half, which nothing mutates, decides nothing.
//!
//! # What this answers (`§2.4`)
//!
//! Another account on this machine reaching a daemon's endpoint, standing in
//! for it, or being taken for this user: the socket's directory and the
//! socket, or the pipe's access list, let nobody else in; a name made first by
//! somebody else is refused, never joined or removed; and a connection whose
//! other end the system does not show to be this user's is closed before a
//! frame crosses it, from either end.
//!
//! # What it does not
//!
//! A process running as the same user, which can connect, bind first where
//! nothing answers, or replace a dead socket — authorization is what resists
//! it (`§14`); root, and on Windows the Administrators group, `SYSTEM` and a
//! process holding the take-ownership privilege; and on Windows the process at
//! the other end, which is read by its id and so may have exited and handed
//! its id to another process by the time its token is asked — of another user,
//! which is refused, or of this one.

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::{Failed, Held, Judged, Kind, Refusal, User, imp};

/// The most connections a listener may be bound to hold, on every system.
///
/// **A Windows pipe's bound, held everywhere so a bound is one number**: there
/// each connection is an instance of the pipe, one more always waits for the
/// next client, and one more is the connection past the bound, held until the
/// next one waits — and a pipe holds at most 254 instances.
pub const CONNECTIONS_MAX: usize = 252;

/// Where a daemon is reached: its runtime directory, the socket's path in it,
/// and the pipe's name.
///
/// **The three are those `nc_channel::Layout` calls `runtime`, `socket`
/// and `pipe`**, each named by the one function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The directory judged before a listener binds.
    pub(crate) runtime: PathBuf,
    /// The socket's path, on Linux and macOS.
    pub(crate) socket: PathBuf,
    /// The pipe's name, on Windows.
    pub(crate) pipe: String,
}

impl Endpoint {
    /// The endpoint of a runtime directory, the socket in it, and the pipe.
    #[must_use]
    pub fn new(
        runtime: impl Into<PathBuf>,
        socket: impl Into<PathBuf>,
        pipe: impl Into<String>,
    ) -> Self {
        Self {
            runtime: runtime.into(),
            socket: socket.into(),
            pipe: pipe.into(),
        }
    }

    /// The runtime directory a listener judges before it binds.
    #[must_use]
    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    /// What this system reaches the endpoint by, as a person reads it: the
    /// socket's path, or the pipe's name.
    #[must_use]
    pub fn name(&self) -> String {
        imp::name_of(self)
    }
}

/// Why an endpoint was not bound, a connection not accepted, or an endpoint
/// not reached.
#[derive(Debug)]
pub enum Refused {
    /// The runtime directory, or what stands at the endpoint's name, is not
    /// this user's alone; it is left as it was.
    NotPrivate(Refusal),
    /// The endpoint's name is longer than this system holds.
    TooLong {
        /// The name.
        name: String,
        /// How long it is, in the unit this system counts it in.
        length: usize,
    },
    /// Something already answers at the endpoint, or made its pipe first.
    Taken {
        /// The name.
        name: String,
    },
    /// The other end runs as another user, or its pipe belongs to another.
    Peer {
        /// Which part of the other end it was: its process, or its pipe.
        whose: &'static str,
        /// Whom the system said it is.
        found: String,
        /// The user this process runs as.
        user: String,
    },
    /// The system did not say which process is at the other end.
    NoProcess,
    /// As many connections as the listener was bound to hold are open.
    Busy {
        /// The bound.
        connections: usize,
    },
    /// Nothing answers at the endpoint.
    Nobody(Failed),
    /// A call the system did not answer.
    Failed(Failed),
}

impl Refused {
    /// A path judged and not taken.
    fn judged(judged: Judged) -> Self {
        match judged {
            Judged::Refused(refused) => Self::NotPrivate(refused),
            Judged::Failed(failed) => Self::Failed(failed),
        }
    }
}

impl core::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotPrivate(refused) => write!(
                formatter,
                "{refused}, so the endpoint was not used; it is left as it was"
            ),
            Self::TooLong { name, length } => write!(
                formatter,
                "`{name}` is {length} {unit} long, and {holds} {max} {unit}",
                unit = imp::NAME_UNIT,
                holds = imp::NAME_HOLDS,
                max = imp::NAME_MAX
            ),
            Self::Taken { name } => write!(
                formatter,
                "something already answers at `{name}`, so it was not taken"
            ),
            Self::Peer { whose, found, user } => write!(
                formatter,
                "{whose} {found}, and this process runs as user {user}, so the connection \
                 was closed before anything crossed it"
            ),
            Self::NoProcess => write!(
                formatter,
                "the system did not say which process is at the other end, so the \
                 connection was closed before anything crossed it"
            ),
            Self::Busy { connections } => write!(
                formatter,
                "{connections} connections are open already, the most this endpoint holds, \
                 so one more was closed as it was accepted"
            ),
            Self::Nobody(failed) => write!(formatter, "{failed}: nothing answers there"),
            Self::Failed(failed) => write!(formatter, "{failed}"),
        }
    }
}

impl std::error::Error for Refused {}

/// Which part of the other end of a connection the system named a user for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Whose {
    /// The process at the other end.
    Process,
    /// The owner of the pipe the connection is an instance of.
    #[cfg_attr(
        all(unix, not(test)),
        expect(
            dead_code,
            reason = "a socket has no owner a client reads; the Windows half names a pipe's"
        )
    )]
    Pipe,
}

impl Whose {
    /// How a refusal names it, before the user it was found to be.
    const fn said(self) -> &'static str {
        match self {
            Self::Process => "the process at the other end runs as user",
            Self::Pipe => "the pipe belongs to user",
        }
    }
}

/// What the system said about the other end of a connection: whom each part
/// of it belongs to, and which process it is, if it said.
#[derive(Debug)]
pub(crate) struct Said {
    /// Each part the system named a user for.
    pub(crate) users: Vec<(Whose, imp::Who)>,
    /// The process, as the system numbers it.
    pub(crate) pid: Option<i64>,
}

/// The other end of a connection, as the system named it and this crate
/// judged it this user's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer {
    /// The process at the other end, as the system numbers it: never zero.
    pub pid: u32,
}

/// Whether `found`, whom the system said `whose` belongs to, is `user`.
///
/// **The one comparison of a peer's user**, made in code every system runs,
/// so the decision is one Linux's mutation runner judges.
#[cfg_attr(
    unix,
    expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "a user's number on Unix and a SID on Windows: one signature for both"
    )
)]
pub(crate) fn admits(found: &imp::Who, whose: Whose, user: &User) -> Result<(), Refused> {
    let ours = user.0.who();
    if *found == ours {
        Ok(())
    } else {
        Err(Refused::Peer {
            whose: whose.said(),
            found: found.to_string(),
            user: ours.to_string(),
        })
    }
}

/// The other end of a connection, once every part the system named is the
/// user's and it named the process.
///
/// **A process numbered zero or below, or none, is refused**: it is no
/// process at the other end, and a number this crate hands on is one a caller
/// may hold a PID record against.
pub(crate) fn admitted(said: Said, user: &User) -> Result<Peer, Refused> {
    let Said { users, pid } = said;
    // Taken apart, so what the system said is spent here and not judged twice.
    for (whose, found) in &users {
        admits(found, *whose, user)?;
    }
    pid.and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .map(|pid| Peer { pid })
        .ok_or(Refused::NoProcess)
}

/// Whether `endpoint`'s name fits what this system holds, and where its
/// socket is: in the runtime directory judged for it.
fn named(endpoint: &Endpoint) -> Result<(), Refused> {
    let length = imp::length_of(endpoint);
    if length > imp::NAME_MAX {
        return Err(Refused::TooLong {
            name: endpoint.name(),
            length,
        });
    }
    if endpoint.socket.parent() != Some(endpoint.runtime.as_path()) {
        return Err(Refused::NotPrivate(Refusal::of(
            &endpoint.socket,
            format!(
                "is not in the runtime directory `{}`, which is the one judged for it",
                endpoint.runtime.display()
            ),
        )));
    }
    Ok(())
}

/// A listener on an endpoint, accepting connections whose other end is this
/// user's, as many at once as it was bound to hold.
#[derive(Debug)]
pub struct Listener {
    /// The system's listener.
    pub(crate) inner: imp::Listener,
    /// Who the other end of every connection is held to.
    pub(crate) user: User,
    /// One count for each connection open, and one for this.
    live: Arc<()>,
    /// The most connections held at once.
    connections: usize,
}

impl Listener {
    /// Binds `endpoint` for `user`, holding at most `connections` at once.
    ///
    /// **The [`Held`] is the profile's instance lock**, which the caller
    /// holds for as long as the listener lives: only under it is a dead socket
    /// at the endpoint's name one no other daemon of the profile can be about
    /// to answer at, and so one to replace. Called inside a Tokio runtime, whose
    /// reactor the endpoint is registered with.
    ///
    /// # Errors
    ///
    /// [`Refused::NotPrivate`] for a runtime directory, or something at the
    /// endpoint's name, that is not this user's alone; [`Refused::TooLong`];
    /// [`Refused::Taken`] where something answers, or made the pipe first; and
    /// [`Refused::Failed`] for a bound of no connections or more than
    /// [`CONNECTIONS_MAX`], and where the system did not answer.
    pub fn bind(
        endpoint: &Endpoint,
        user: &User,
        _instance: &Held,
        connections: usize,
    ) -> Result<Self, Refused> {
        if connections == 0 || connections > CONNECTIONS_MAX {
            return Err(Refused::Failed(Failed::new(
                format!(
                    "a listener bound to hold {connections} connections, which is not one to \
                     {CONNECTIONS_MAX}"
                ),
                io::Error::from(io::ErrorKind::InvalidInput),
            )));
        }
        crate::judge(&endpoint.runtime, Kind::Directory, user).map_err(Refused::judged)?;
        named(endpoint)?;
        let inner = imp::bind(endpoint, &user.0, connections)?;
        Ok(Self {
            inner,
            user: user.clone(),
            live: Arc::new(()),
            connections,
        })
    }

    /// The next connection whose other end is this user's.
    ///
    /// **One refused is closed, and answered for**: the caller decides what
    /// to log and asks again, and the listener goes on listening either way.
    ///
    /// # Errors
    ///
    /// [`Refused::Peer`] and [`Refused::NoProcess`] for a connection whose
    /// other end is not shown to be this user's, [`Refused::Busy`] for one
    /// past the bound, and [`Refused::Failed`] where the system did not
    /// answer.
    pub async fn accept(&mut self) -> Result<Accepted, Refused> {
        let (inner, said) = imp::accept(&mut self.inner).await.map_err(|error| {
            Refused::Failed(Failed::new(
                "a connection could not be accepted".to_owned(),
                error,
            ))
        })?;
        let peer = admitted(said, &self.user)?;
        // The listener's own count is one of these, so the connections open
        // are one fewer.
        if Arc::strong_count(&self.live) > self.connections {
            return Err(Refused::Busy {
                connections: self.connections,
            });
        }
        Ok(Accepted {
            stream: Stream {
                inner,
                _counted: Some(Arc::clone(&self.live)),
            },
            peer,
        })
    }
}

/// A connection accepted, and the other end it was judged by.
#[derive(Debug)]
pub struct Accepted {
    /// The connection.
    pub stream: Stream,
    /// Its other end.
    pub peer: Peer,
}

/// One connection to or from an endpoint: bytes read and written, in order.
#[derive(Debug)]
pub struct Stream {
    /// The system's stream.
    inner: imp::Stream,
    /// The listener's count of this connection, let go when it closes; a
    /// client's connection counts nothing.
    _counted: Option<Arc<()>>,
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(context, buffer)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(context)
    }
}

/// A connection to `endpoint`, once the system shows its other end to be the
/// user's — before anything is sent.
///
/// # Errors
///
/// [`Refused::TooLong`] for a name longer than this system holds,
/// [`Refused::Nobody`] where nothing answers, [`Refused::Peer`] and
/// [`Refused::NoProcess`] for an other end not shown to be this user's, and
/// [`Refused::Failed`] where the system did not answer.
pub async fn connect(endpoint: &Endpoint, user: &User) -> Result<(Stream, Peer), Refused> {
    named(endpoint)?;
    let (inner, said) = imp::connect(endpoint).await.map_err(|error| {
        let failed = Failed::new(format!("`{}` could not be reached", endpoint.name()), error);
        // **Nothing listening is told apart from everything else**: it is a
        // daemon that is not running, and every other answer is a refusal.
        if matches!(
            failed.error().kind(),
            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
        ) {
            Refused::Nobody(failed)
        } else {
            Refused::Failed(failed)
        }
    })?;
    let peer = admitted(said, user)?;
    Ok((
        Stream {
            inner,
            _counted: None,
        },
        peer,
    ))
}
