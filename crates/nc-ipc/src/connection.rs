//! A connection once its handshake is over: one request at a time, and a
//! heartbeat both ways.
//!
//! **Two tasks per connection**, so the heartbeat runs whatever the caller is
//! doing — a dispatch the daemon is running included. The writer sends what
//! the caller hands it, and a ping whenever it has sent nothing for
//! `ipc_heartbeat_ms`; the reader takes each frame, drops a ping, and drops
//! the connection when nothing has arrived for `ipc_silent_after_ms` — any
//! frame resets that. A goodbye from the peer is the peer's refusal, by its
//! code.
//!
//! **One request at a time**: the daemon's reader refuses a request that
//! arrives before the last one was answered, and the client's reader an
//! answer nobody asked for, each saying goodbye with `NC-IPC-0002` and closing.
//!
//! **A write the peer does not take within `ipc_silent_after_ms` ends the
//! connection too** (`NC-IPC-0017`): the writer lets go of its half and tells
//! the reader, which stops and hands the caller why, so a peer that pings and
//! never reads holds neither half of the stream open. A value too long for a
//! frame is refused to the caller before it is handed to the writer
//! (`NC-IPC-0016`), and the connection is left as it was.
//!
//! Dropping a [`Client`] or a [`Served`] closes the connection: the reader
//! stops, and the writer sends what it was handed and shuts its half.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nc_canon::Value;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio::sync::{mpsc, oneshot};

use crate::message::{self, Heard};
use crate::{Answer, IpcError, Request, frame};

/// Which end of a connection this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    /// The daemon, which reads requests and answers them.
    Daemon,
    /// The client, which asks and reads the answer.
    Client,
}

/// How long a side stays silent before it pings: `ipc_heartbeat_ms`.
const fn heartbeat() -> std::time::Duration {
    std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_heartbeat_ms)
}

/// How long a peer may say nothing, or take nothing, before the connection
/// to it is dropped: `ipc_silent_after_ms`.
const fn silence() -> std::time::Duration {
    std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_silent_after_ms)
}

/// One connection's two tasks, and the ends of the channels they are reached
/// through.
#[derive(Debug)]
pub(crate) struct Connection {
    /// What the writer sends: frames, each made by the caller that hands it
    /// over.
    outbox: mpsc::Sender<Vec<u8>>,
    /// What the reader took that is the caller's, or why it stopped.
    inbox: mpsc::Receiver<Result<Value, IpcError>>,
    /// Whether a request is being answered, on the daemon's side, or an
    /// answer awaited, on the client's.
    awaiting: Arc<AtomicBool>,
    /// The reader, stopped when the connection is dropped.
    reading: tokio::task::JoinHandle<()>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.reading.abort();
    }
}

impl Connection {
    /// The connection over `stream`, read and written by two tasks of its own
    /// on the runtime this is called in.
    pub(crate) fn start<S>(stream: S, side: Side) -> Self
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        let (outbox, sending) = mpsc::channel(1);
        let (taken, inbox) = mpsc::channel(1);
        let (gave_up, writer_gave_up) = oneshot::channel();
        let awaiting = Arc::new(AtomicBool::new(false));
        tokio::spawn(writing(writer, sending, gave_up));
        let reading = tokio::spawn(reading(
            reader,
            taken,
            outbox.clone(),
            writer_gave_up,
            Arc::clone(&awaiting),
            side,
        ));
        Self {
            outbox,
            inbox,
            awaiting,
            reading,
        }
    }

    /// Hands `frame` to the writer.
    async fn send(&self, frame: Vec<u8>) -> Result<(), IpcError> {
        self.outbox.send(frame).await.map_err(|_| {
            IpcError::Lost("the connection was closed before it was written to".to_owned())
        })
    }
}

/// Sends what `sending` is handed, and a ping after each heartbeat of silence.
///
/// It goes on until every sender is gone — or until a write is not taken in
/// time, or fails, which it says through `gave_up` once it has let go of its
/// half.
async fn writing<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut sending: mpsc::Receiver<Vec<u8>>,
    gave_up: oneshot::Sender<IpcError>,
) {
    let stopped = loop {
        let frame = match tokio::time::timeout(heartbeat(), sending.recv()).await {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                let _ = tokio::time::timeout(silence(), writer.shutdown()).await;
                return;
            }
            Err(_) => match frame::encode(&message::ping()) {
                Ok(ping) => ping,
                Err(error) => break error,
            },
        };
        match tokio::time::timeout(silence(), frame::put(&mut writer, &frame)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => break error,
            Err(_) => break IpcError::Unread,
        }
    };
    drop(writer);
    let _ = gave_up.send(stopped);
}

/// Takes each frame of `reader`: a ping is dropped, a goodbye ends the
/// connection with the peer's code, and anything else is the caller's — once
/// it is the one message this side expects now.
///
/// The writer's word that it gave up ends the connection too, whatever is
/// still arriving.
async fn reading<R: AsyncRead + Unpin>(
    mut reader: R,
    taken: mpsc::Sender<Result<Value, IpcError>>,
    outbox: mpsc::Sender<Vec<u8>>,
    mut writer_gave_up: oneshot::Receiver<IpcError>,
    awaiting: Arc<AtomicBool>,
    side: Side,
) {
    let stopped = loop {
        let read = tokio::select! {
            biased;
            gave_up = &mut writer_gave_up => break gave_up.unwrap_or_else(|_| {
                IpcError::Lost("the connection's writer stopped".to_owned())
            }),
            read = tokio::time::timeout(silence(), frame::read(&mut reader)) => read,
        };
        let value = match read {
            Err(_) => break IpcError::Silent,
            // Closed at a frame's boundary: the caller reads that as the end.
            Ok(Ok(None)) => return,
            Ok(Err(error)) => break error,
            Ok(Ok(Some(value))) => value,
        };
        let value = match message::heard(value) {
            Ok(Heard::Ping) => continue,
            Ok(Heard::Goodbye(code)) => break IpcError::TurnedAway(code),
            Ok(Heard::Message(value)) => value,
            Err(error) => break error,
        };
        // The daemon takes a request only when none is being answered; the
        // client takes an answer only when it asked for one.
        let expected = match side {
            Side::Daemon => !awaiting.swap(true, Ordering::SeqCst),
            Side::Client => awaiting.swap(false, Ordering::SeqCst),
        };
        if !expected {
            break IpcError::Malformed(match side {
                Side::Daemon => "a request before the last one was answered".to_owned(),
                Side::Client => "an answer nobody asked for".to_owned(),
            });
        }
        if taken.send(Ok(value)).await.is_err() {
            return;
        }
    };
    // Said where the peer is still there to hear it: not to one that closed,
    // turned this side away, or takes nothing.
    if !matches!(
        stopped,
        IpcError::Lost(_) | IpcError::TurnedAway(_) | IpcError::Unread
    ) && let Ok(goodbye) = frame::encode(&message::goodbye(stopped.code()))
    {
        let _ = outbox.try_send(goodbye);
    }
    let _ = taken.send(Err(stopped)).await;
}

/// The client's end of a connection: one request at a time, each awaited
/// until it is answered or the daemon is gone.
#[derive(Debug)]
pub struct Client {
    /// The connection.
    connection: Connection,
    /// Whether a request was abandoned before its answer, which would be read
    /// as the next one's.
    abandoned: bool,
}

impl From<Connection> for Client {
    fn from(connection: Connection) -> Self {
        Self {
            connection,
            abandoned: false,
        }
    }
}

impl Client {
    /// Asks `request`, and answers what the daemon answered.
    ///
    /// **Not to be abandoned**: a call dropped before its answer leaves that
    /// answer on its way, so every later call is refused rather than handed
    /// it.
    ///
    /// # Errors
    ///
    /// [`IpcError::Unsendable`] for a request too long for a frame, which
    /// leaves the connection as it was; [`IpcError::Silent`] for a daemon
    /// silent past `ipc_silent_after_ms`, and [`IpcError::Unread`] for one that
    /// took nothing in that time; [`IpcError::TurnedAway`] for a daemon that
    /// closed saying why, [`IpcError::Lost`] for one that closed without, and
    /// [`IpcError::Malformed`] for an answer that is not one — or a call made
    /// after one was abandoned.
    pub async fn ask(&mut self, request: &Request) -> Result<Answer, IpcError> {
        if self.abandoned {
            return Err(IpcError::Malformed(
                "a request was abandoned before its answer, so this connection is not asked \
                 again"
                    .to_owned(),
            ));
        }
        // Its frame made first, so one too long is refused before anything
        // of this connection changes.
        let frame = frame::encode(&request.to_value())?;
        self.abandoned = true;
        self.connection.awaiting.store(true, Ordering::SeqCst);
        self.connection.send(frame).await?;
        let answer = match self.connection.inbox.recv().await {
            None => Err(IpcError::Lost(
                "the daemon closed the connection before it answered".to_owned(),
            )),
            Some(Err(error)) => Err(error),
            Some(Ok(value)) => Answer::from_value(value),
        };
        self.abandoned = false;
        answer
    }
}

/// The daemon's end of a connection: each request in turn, and its answer.
#[derive(Debug)]
pub struct Served {
    /// The connection.
    connection: Connection,
}

impl From<Connection> for Served {
    fn from(connection: Connection) -> Self {
        Self { connection }
    }
}

impl Served {
    /// The next request, or nothing once the client has closed the
    /// connection.
    ///
    /// # Errors
    ///
    /// [`IpcError::Silent`] for a client silent past `ipc_silent_after_ms`,
    /// [`IpcError::Unread`] for one that took nothing written to it in that
    /// time, [`IpcError::Malformed`] for a request that is not one or one
    /// before the last was answered, and [`IpcError::TurnedAway`],
    /// [`IpcError::TooLarge`] and [`IpcError::Lost`] as the frames answer.
    pub async fn next(&mut self) -> Result<Option<Request>, IpcError> {
        match self.connection.inbox.recv().await {
            None => Ok(None),
            Some(Err(error)) => Err(error),
            Some(Ok(value)) => Request::from_value(value).map(Some),
        }
    }

    /// Answers the request [`Served::next`] handed out last.
    ///
    /// **An answer too long for a frame is refused, and the request goes on
    /// waiting for one**, so the daemon can answer it with a refusal instead.
    ///
    /// # Errors
    ///
    /// [`IpcError::Unsendable`] for an answer too long for a frame,
    /// [`IpcError::Malformed`] when no request is waiting for an answer, and
    /// [`IpcError::Lost`] when the connection is gone.
    pub async fn answer(&mut self, answer: &Answer) -> Result<(), IpcError> {
        let frame = frame::encode(&answer.to_value())?;
        if !self.connection.awaiting.swap(false, Ordering::SeqCst) {
            return Err(IpcError::Malformed(
                "an answer with no request waiting for it".to_owned(),
            ));
        }
        self.connection.send(frame).await
    }
}
