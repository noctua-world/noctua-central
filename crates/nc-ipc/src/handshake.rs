//! The handshake: each end proves it holds the channel's token, and neither
//! sends it.
//!
//! 1. The daemon sends a challenge: the protocol and a nonce of its own.
//! 2. The client answers with the protocol, its nonce, and its proof — BLAKE3
//!    keyed by the token, over the client's domain and both nonces.
//! 3. The daemon holds the protocol to its own and the proof to the one it
//!    makes itself, and answers with its proof — keyed the same way, over its
//!    own domain — and its welcome.
//! 4. The client holds that proof to the one it makes, and only then has a
//!    connection to send a request through.
//!
//! **Each proof is compared in constant time** (`blake3::Hash`'s equality), so
//! how long a refusal takes says nothing of how much of a proof was right; and
//! **each end's proof is over a domain of its own and the nonces of both**, so
//! a proof replayed from another connection, or one end's reflected back at
//! it, is not the other's. Either end bounds the whole of it by
//! `ipc_handshake_ms`, and says goodbye with the code before it closes for a
//! reason of its own.

use tokio::io::{AsyncRead, AsyncWrite};

use crate::connection::{Connection, Side};
use crate::message::{self, Welcome};
use crate::{Client, IpcError, Served, Token, frame};

/// The domain a client's proof is made over.
const CLIENT: &str = "nc-ipc/1 the client's proof";

/// The domain the daemon's proof is made over.
const DAEMON: &str = "nc-ipc/1 the daemon's proof";

/// How long either end has for the whole handshake: `ipc_handshake_ms`.
const fn deadline() -> std::time::Duration {
    std::time::Duration::from_millis(nc_catalog::DAEMON.ipc_handshake_ms)
}

/// A nonce, drawn from the system's own generator.
fn nonce() -> Result<[u8; 32], IpcError> {
    let mut nonce = [0_u8; 32];
    nc_platform::random(&mut nonce).map_err(|error| {
        IpcError::Lost(format!(
            "the system's generator did not answer, so no nonce was drawn: {error}"
        ))
    })?;
    Ok(nonce)
}

/// The next frame of `stream`, which must be there: the handshake has no
/// place where the connection may close.
async fn next<S: AsyncRead + Unpin>(stream: &mut S) -> Result<nc_canon::Value, IpcError> {
    frame::read(stream).await?.ok_or_else(|| {
        IpcError::Lost("the other end closed the connection during the handshake".to_owned())
    })
}

/// Says goodbye with `error`'s code, where the other end is still there to
/// hear it, within the handshake's bound.
async fn goodbye<S: AsyncWrite + Unpin>(stream: &mut S, error: &IpcError) {
    if matches!(error, IpcError::Lost(_) | IpcError::TurnedAway(_)) {
        return;
    }
    let said = message::goodbye(error.code());
    let _ = tokio::time::timeout(deadline(), frame::write(stream, &said)).await;
}

/// The daemon's half of the handshake over `stream`: a client that does not
/// prove it holds `token` is turned away, and one that does is welcomed with
/// `welcome`.
///
/// # Errors
///
/// [`IpcError::Protocol`] for a client speaking another version,
/// [`IpcError::ClientUnproven`] for a proof not keyed by the token,
/// [`IpcError::Slow`] past `ipc_handshake_ms`, and [`IpcError::Malformed`],
/// [`IpcError::TooLarge`], [`IpcError::Lost`] and [`IpcError::TurnedAway`] as
/// the frames answer.
pub async fn serve<S>(mut stream: S, token: &Token, welcome: &Welcome) -> Result<Served, IpcError>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let verdict =
        match tokio::time::timeout(deadline(), welcomed(&mut stream, token, welcome)).await {
            Ok(verdict) => verdict,
            Err(_) => Err(IpcError::Slow),
        };
    match verdict {
        Ok(()) => Ok(Served::from(Connection::start(stream, Side::Daemon))),
        Err(error) => {
            goodbye(&mut stream, &error).await;
            Err(error)
        }
    }
}

/// The daemon's half, until its welcome is sent.
async fn welcomed<S>(stream: &mut S, token: &Token, welcome: &Welcome) -> Result<(), IpcError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let ours = nonce()?;
    frame::write(stream, &message::challenge(&ours)).await?;
    let (theirs, proof) = message::read_hello(next(stream).await?)?;
    if proof != token.prove(CLIENT, &ours, &theirs) {
        return Err(IpcError::ClientUnproven);
    }
    let own = token.prove(DAEMON, &ours, &theirs);
    frame::write(stream, &message::welcomed(&own, welcome)).await
}

/// The client's half of the handshake over `stream`, which sends nothing past
/// its hello to a daemon that does not prove it holds `token`.
///
/// # Errors
///
/// [`IpcError::Protocol`] for a daemon speaking another version,
/// [`IpcError::DaemonUnproven`] for a proof not keyed by the token,
/// [`IpcError::TurnedAway`] with the daemon's code when it turned this client
/// away, [`IpcError::Slow`] past `ipc_handshake_ms`, and
/// [`IpcError::Malformed`], [`IpcError::TooLarge`] and [`IpcError::Lost`] as
/// the frames answer.
pub async fn connect<S>(mut stream: S, token: &Token) -> Result<(Client, Welcome), IpcError>
where
    S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    let verdict = match tokio::time::timeout(deadline(), welcome_of(&mut stream, token)).await {
        Ok(verdict) => verdict,
        Err(_) => Err(IpcError::Slow),
    };
    match verdict {
        Ok(welcome) => Ok((
            Client::from(Connection::start(stream, Side::Client)),
            welcome,
        )),
        Err(error) => {
            goodbye(&mut stream, &error).await;
            Err(error)
        }
    }
}

/// The client's half, until the daemon's welcome is proven.
async fn welcome_of<S>(stream: &mut S, token: &Token) -> Result<Welcome, IpcError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let theirs = message::read_challenge(next(stream).await?)?;
    let ours = nonce()?;
    let proof = token.prove(CLIENT, &theirs, &ours);
    frame::write(stream, &message::hello(&ours, &proof)).await?;
    let (proof, welcome) = message::read_welcomed(next(stream).await?)?;
    if proof != token.prove(DAEMON, &theirs, &ours) {
        return Err(IpcError::DaemonUnproven);
    }
    Ok(welcome)
}
