//! One frame: a four-byte big-endian length, and that many bytes of the
//! canonical JSON of one value (`NOCTUA.md §11.1`, `§16.4`).
//!
//! **What is refused, and before what.** A length of zero, and one above
//! `ipc_frame_max_bytes`, are refused from the four bytes of the prefix, before
//! a byte of the body is read or allocated: a peer cannot make the reader hold
//! more than one frame's bound by claiming more. A body that is not UTF-8, not
//! JSON `nc-canon` parses, or not exactly the text `nc-canon` writes for the
//! value it parses to, is refused once it is read: there is one spelling of
//! every message, so what one end hashes, logs or compares is what the other
//! sent. A writer refuses to send what a reader would refuse, as a refusal of
//! its own: nothing claimed the length, and nothing of it is written.
//!
//! **A connection that ends at a frame's boundary has closed; one that ends
//! inside a frame was lost**, and the two are told apart.

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::IpcError;

/// The most a frame's body may hold: `ipc_frame_max_bytes`.
const MOST: u64 = nc_catalog::DAEMON.ipc_frame_max_bytes;

/// The next frame of `reader`, as the value it holds — or nothing, when the
/// connection closed at a frame's boundary.
///
/// # Errors
///
/// [`IpcError::TooLarge`] for a prefix claiming more than the bound,
/// [`IpcError::Malformed`] for a frame of no length or a body that is not
/// canonical JSON, and [`IpcError::Lost`] for a connection that ended inside
/// a frame or could not be read.
pub async fn read<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<nc_canon::Value>, IpcError> {
    let mut prefix = [0_u8; 4];
    let mut have = 0;
    while have < prefix.len() {
        let read = reader
            .read(&mut prefix[have..])
            .await
            .map_err(|error| IpcError::Lost(format!("a frame could not be read: {error}")))?;
        if read == 0 {
            return if have == 0 {
                Ok(None)
            } else {
                Err(IpcError::Lost(format!(
                    "the connection ended {have} bytes into a frame's length"
                )))
            };
        }
        have += read;
    }
    let length = u64::from(u32::from_be_bytes(prefix));
    // Judged before the body is allocated, so a length past the bound costs
    // nothing but its four bytes.
    if length == 0 {
        return Err(IpcError::Malformed(
            "a frame of no length, which holds no value".to_owned(),
        ));
    }
    if length > MOST {
        return Err(IpcError::TooLarge { length });
    }
    let mut body = vec![0_u8; usize::try_from(length).unwrap_or(usize::MAX)];
    reader.read_exact(&mut body).await.map_err(|error| {
        IpcError::Lost(format!(
            "the connection ended inside a frame of {length} bytes: {error}"
        ))
    })?;
    let text = String::from_utf8(body)
        .map_err(|_| IpcError::Malformed("a frame that is not UTF-8".to_owned()))?;
    let value = nc_canon::parse(&text)
        .map_err(|error| IpcError::Malformed(format!("a frame that is not JSON: {error}")))?;
    if value.to_canonical() != text {
        return Err(IpcError::Malformed(
            "a frame that is not canonical JSON: its text is not the one its value is written \
             as"
            .to_owned(),
        ));
    }
    Ok(Some(value))
}

/// Writes `value` to `writer` as one frame, and flushes it.
///
/// # Errors
///
/// [`IpcError::Unsendable`] for a value whose canonical text is longer than
/// the bound, which no reader would take, with nothing written; and
/// [`IpcError::Lost`] when it could not be written.
pub async fn write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &nc_canon::Value,
) -> Result<(), IpcError> {
    put(writer, &encode(value)?).await
}

/// The bytes of `value`'s frame: its length, big-endian, and its canonical text.
///
/// Made before anything is written, so a value too long for one is refused
/// while nothing of it has been sent.
pub(crate) fn encode(value: &nc_canon::Value) -> Result<Vec<u8>, IpcError> {
    let text = value.to_canonical();
    let length = u64::try_from(text.len()).unwrap_or(u64::MAX);
    if length > MOST {
        return Err(IpcError::Unsendable { length });
    }
    let prefix = u32::try_from(length).map_err(|_| IpcError::Unsendable { length })?;
    let mut frame = Vec::with_capacity(4 + text.len());
    frame.extend_from_slice(&prefix.to_be_bytes());
    frame.extend_from_slice(text.as_bytes());
    Ok(frame)
}

/// Writes the bytes of a frame [`encode`] made, and flushes them.
pub(crate) async fn put<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &[u8],
) -> Result<(), IpcError> {
    let lost =
        |error: std::io::Error| IpcError::Lost(format!("a frame could not be written: {error}"));
    writer.write_all(frame).await.map_err(lost)?;
    writer.flush().await.map_err(lost)
}
