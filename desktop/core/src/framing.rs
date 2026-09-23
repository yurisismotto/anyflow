//! Length-prefixed framing for protobuf envelopes.
//!
//! Protobuf is not self-delimiting, so each message is preceded by a 4-byte
//! big-endian length.
//!
//! The length is checked *before* allocating. A peer that claims a 4 GiB
//! frame gets an error and a closed connection, not an allocation. This is
//! the cheapest denial-of-service surface in the whole protocol and it is
//! worth being blunt about: `MAX_FRAME_LEN` is small on purpose. Nothing in
//! this protocol version is large — a battery update is a few dozen bytes —
//! and it can be raised deliberately when a capability actually needs it.

use omnibridge_proto::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{Error, Result};

/// 64 KiB. See module docs.
pub const MAX_FRAME_LEN: u32 = 64 * 1024;

pub async fn write_envelope<W: AsyncWrite + Unpin>(
    w: &mut W,
    envelope: &omnibridge_proto::v1::Envelope,
) -> Result<()> {
    let body = envelope.encode_to_vec();
    let len: u32 = body
        .len()
        .try_into()
        .map_err(|_| Error::FrameTooLarge(u32::MAX, MAX_FRAME_LEN))?;
    if len > MAX_FRAME_LEN {
        return Err(Error::FrameTooLarge(len, MAX_FRAME_LEN));
    }
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_envelope<R: AsyncRead + Unpin>(
    r: &mut R,
) -> Result<omnibridge_proto::v1::Envelope> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(Error::Closed),
        Err(e) => return Err(Error::Io(e)),
    }

    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_LEN {
        return Err(Error::FrameTooLarge(len, MAX_FRAME_LEN));
    }
    if len == 0 {
        return Err(Error::Protocol("zero-length frame"));
    }

    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => Error::Closed,
        _ => Error::Io(e),
    })?;

    Ok(omnibridge_proto::v1::Envelope::decode(&body[..])?)
}
