//! The data stream itself: its two handshake frames, and the copy loop.
//!
//! # Why this is not the envelope path
//!
//! Nothing here goes through `anyflow_core::framing`, the replay guard, the
//! sequence counter or the capability dispatch table. That machinery exists
//! for a low-rate stream of small control messages, and ADR-0012 records why
//! pushing a multi-gigabyte transfer through it is the wrong shape. What is
//! reused instead is everything that decides *trust*: TLS 1.3, mutual
//! authentication, the pinned identity, the same listener and the same port.
//!
//! # Memory
//!
//! A transfer costs [`COPY_BUFFER_BYTES`] of memory and one file descriptor,
//! whatever the file's size. Nothing here ever holds a whole file, or a whole
//! chunk list, or an offset table. That is FILE-13, and it is a property of
//! the loop below rather than a limit configured somewhere.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyflow_proto::Message;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

use crate::limits::{COPY_BUFFER_BYTES, MAX_DATA_STREAM_FRAME, STREAM_IDLE_TIMEOUT};
use crate::transfer::FailureReason;

/// How long to wait, after the last expected byte, to see whether the sender
/// keeps talking.
///
/// A well-behaved sender closes its write half immediately, so this normally
/// costs nothing. It is bounded so that a sender which simply holds the
/// socket open cannot stall the transfer it has already completed correctly.
const TRAILING_BYTE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Handshake framing
// ---------------------------------------------------------------------------

/// Writes one length-prefixed protobuf frame.
///
/// Same 4-byte big-endian prefix as the control session, deliberately: one
/// framing convention in the project is easier to reason about than two. The
/// cap is different and much smaller, because these two messages are tens of
/// bytes and nothing else is ever sent this way.
pub async fn write_frame<W, M>(w: &mut W, message: &M) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    M: Message,
{
    let body = message.encode_to_vec();
    if body.len() > MAX_DATA_STREAM_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data stream frame too large",
        ));
    }
    let len = body.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

/// Reads one length-prefixed protobuf frame.
///
/// The length is checked **before** the buffer is allocated, exactly as it is
/// on the control session. This frame arrives from a peer that has completed
/// TLS but has not yet proved anything about which transfer it wants, so it
/// is the least trusted input in the whole capability.
pub async fn read_frame<R, M>(r: &mut R) -> io::Result<M>
where
    R: AsyncRead + Unpin,
    M: Message + Default,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "zero-length data stream frame",
        ));
    }
    if len > MAX_DATA_STREAM_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data stream frame exceeds the limit",
        ));
    }

    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    M::decode(&body[..])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "malformed data stream frame"))
}

// ---------------------------------------------------------------------------
// The copy loop
// ---------------------------------------------------------------------------

/// Streams exactly `expected` bytes from `reader` into `writer`, hashing as
/// it goes.
///
/// Returns the SHA-256 of what was written. The caller compares it against
/// the offer; this function deliberately does not, so that "did the bytes
/// arrive" and "are they the right bytes" stay two separate answers.
///
/// Enforces, in order:
///
/// * **cancellation** — a signal on `cancel` stops the loop promptly rather
///   than at the next timeout, which is what makes a cancel feel immediate;
/// * **idle timeout** — no progress for [`STREAM_IDLE_TIMEOUT`] ends it;
/// * **truncation** — EOF before `expected` bytes is a failure, never a
///   short file quietly accepted (F10, F12);
/// * **overrun** — a sender that keeps writing past `expected` is a failure,
///   not something to ignore (F11).
pub async fn receive_exactly<R, W>(
    reader: &mut R,
    writer: &mut W,
    expected: u64,
    progress: &Arc<AtomicU64>,
    cancel: &mut watch::Receiver<bool>,
) -> std::result::Result<[u8; 32], FailureReason>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // One buffer for the whole transfer. This, and not the file, is what the
    // transfer costs in memory.
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut remaining = expected;

    while remaining > 0 {
        // Never read more than is still expected, so an overrun cannot be
        // written to the file even for an instant.
        let want = remaining.min(buf.len() as u64) as usize;

        let read = tokio::select! {
            biased;
            _ = cancel.changed() => return Err(FailureReason::CancelledByUser),
            r = tokio::time::timeout(STREAM_IDLE_TIMEOUT, reader.read(&mut buf[..want])) => r,
        };

        let n = match read {
            Ok(Ok(0)) => return Err(FailureReason::Integrity), // truncated
            Ok(Ok(n)) => n,
            Ok(Err(_)) => return Err(FailureReason::Transport),
            Err(_) => return Err(FailureReason::TimedOut),
        };

        hasher.update(&buf[..n]);
        if writer.write_all(&buf[..n]).await.is_err() {
            return Err(FailureReason::Storage);
        }

        remaining -= n as u64;
        progress.store(expected - remaining, Ordering::Relaxed);
    }

    if writer.flush().await.is_err() {
        return Err(FailureReason::Storage);
    }

    // The sender must be finished. Anything further means it disagreed with
    // its own offer about how large the file is.
    let mut extra = [0u8; 1];
    match tokio::time::timeout(TRAILING_BYTE_GRACE, reader.read(&mut extra)).await {
        Ok(Ok(0)) => {}                                    // clean EOF: expected
        Ok(Ok(_)) => return Err(FailureReason::Integrity), // wrote past its size
        Ok(Err(_)) => {}                                   // reset after a complete file: fine
        Err(_) => {}                                       // sender simply did not close
    }

    Ok(hasher.finalize().into())
}

/// Streams exactly `expected` bytes from `reader` to `writer`.
///
/// The sending half. It closes its write side on success, which is what lets
/// the receiver distinguish "the file ended" from "the link stalled".
pub async fn send_exactly<R, W>(
    reader: &mut R,
    writer: &mut W,
    expected: u64,
    progress: &Arc<AtomicU64>,
    cancel: &mut watch::Receiver<bool>,
) -> std::result::Result<(), FailureReason>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    let mut remaining = expected;

    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;

        let n = match reader.read(&mut buf[..want]).await {
            Ok(0) => {
                // The local file is shorter than the offer said. Sending a
                // short stream would make the receiver fail its hash check
                // for a reason it could not diagnose; fail here instead.
                return Err(FailureReason::Integrity);
            }
            Ok(n) => n,
            Err(_) => return Err(FailureReason::Storage),
        };

        let write = tokio::select! {
            biased;
            _ = cancel.changed() => return Err(FailureReason::CancelledByUser),
            r = tokio::time::timeout(STREAM_IDLE_TIMEOUT, writer.write_all(&buf[..n])) => r,
        };

        match write {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(FailureReason::Transport),
            Err(_) => return Err(FailureReason::TimedOut),
        }

        remaining -= n as u64;
        progress.store(expected - remaining, Ordering::Relaxed);
    }

    if writer.flush().await.is_err() {
        return Err(FailureReason::Transport);
    }
    // Half-close, so the receiver sees a definite end of file.
    let _ = writer.shutdown().await;
    Ok(())
}

/// Hashes a local file, returning its size and SHA-256.
///
/// Streamed in the same bounded buffer as everything else: offering a 4 GiB
/// file must not cost 4 GiB of memory just to compute its digest.
pub async fn hash_file(path: &std::path::Path) -> io::Result<(u64, [u8; 32])> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut size = 0u64;

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }

    Ok((size, hasher.finalize().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyflow_proto::v1::capabilities as pb;

    fn progress() -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(0))
    }

    fn no_cancel() -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        // Kept alive for the life of the test; dropping the sender would make
        // `changed()` resolve immediately and look like a cancellation.
        Box::leak(Box::new(tx));
        rx
    }

    #[tokio::test]
    async fn a_frame_round_trips() {
        let message = pb::DataStreamAuth {
            protocol_version: 1,
            transfer_id: vec![7; 16],
            mac: vec![9; 32],
        };
        let mut buf = Vec::new();
        write_frame(&mut buf, &message).await.expect("write");

        let mut cursor = std::io::Cursor::new(buf);
        let back: pb::DataStreamAuth = read_frame(&mut cursor).await.expect("read");
        assert_eq!(back.transfer_id, message.transfer_id);
        assert_eq!(back.mac, message.mac);
    }

    #[tokio::test]
    async fn an_oversized_length_prefix_is_refused_before_allocating() {
        // The claim is 4 GiB. It must cost a rejection, not four gigabytes.
        let mut framed = Vec::new();
        framed.extend_from_slice(&u32::MAX.to_be_bytes());
        let mut cursor = std::io::Cursor::new(framed);
        let result: io::Result<pb::DataStreamAuth> = read_frame(&mut cursor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn a_zero_length_frame_is_refused() {
        let mut framed = Vec::new();
        framed.extend_from_slice(&0u32.to_be_bytes());
        let mut cursor = std::io::Cursor::new(framed);
        let result: io::Result<pb::DataStreamReady> = read_frame(&mut cursor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn an_exact_transfer_hashes_what_it_wrote() {
        let payload = b"the quick brown fox".repeat(100);
        let mut reader = std::io::Cursor::new(payload.clone());
        let mut sink = Vec::new();
        let bytes = progress();

        let digest = receive_exactly(
            &mut reader,
            &mut sink,
            payload.len() as u64,
            &bytes,
            &mut no_cancel(),
        )
        .await
        .expect("complete");

        assert_eq!(sink, payload);
        assert_eq!(bytes.load(Ordering::Relaxed), payload.len() as u64);

        let expected: [u8; 32] = Sha256::digest(&payload).into();
        assert_eq!(digest, expected);
    }

    #[tokio::test]
    async fn a_truncated_stream_fails_rather_than_producing_a_short_file() {
        // F10/F12: the peer promised 100 bytes and sent 10.
        let mut reader = std::io::Cursor::new(vec![0u8; 10]);
        let mut sink = Vec::new();
        let result =
            receive_exactly(&mut reader, &mut sink, 100, &progress(), &mut no_cancel()).await;
        assert_eq!(result.expect_err("must fail"), FailureReason::Integrity);
    }

    #[tokio::test]
    async fn a_stream_longer_than_its_offer_is_refused() {
        // F11: the peer promised 10 bytes and kept writing.
        let mut reader = std::io::Cursor::new(vec![0u8; 100]);
        let mut sink = Vec::new();
        let result =
            receive_exactly(&mut reader, &mut sink, 10, &progress(), &mut no_cancel()).await;
        assert_eq!(result.expect_err("must fail"), FailureReason::Integrity);
        // Only the declared number of bytes was ever written out.
        assert_eq!(sink.len(), 10);
    }

    #[tokio::test]
    async fn a_zero_byte_file_is_a_valid_transfer() {
        let mut reader = std::io::Cursor::new(Vec::new());
        let mut sink = Vec::new();
        let digest = receive_exactly(&mut reader, &mut sink, 0, &progress(), &mut no_cancel())
            .await
            .expect("empty is fine");
        let expected: [u8; 32] = Sha256::digest(b"").into();
        assert_eq!(digest, expected);
    }

    #[tokio::test]
    async fn cancelling_stops_the_copy() {
        // F13: a cancel must take effect while bytes are moving, not at the
        // end of the file.
        let (tx, rx) = watch::channel(false);
        let mut rx = rx;
        tx.send(true).expect("signal");

        let mut reader = std::io::Cursor::new(vec![0u8; 1_000_000]);
        let mut sink = Vec::new();
        let result = receive_exactly(&mut reader, &mut sink, 1_000_000, &progress(), &mut rx).await;
        assert_eq!(
            result.expect_err("must fail"),
            FailureReason::CancelledByUser
        );
    }

    #[tokio::test]
    async fn progress_is_reported_as_the_copy_runs() {
        let payload = vec![0u8; COPY_BUFFER_BYTES * 3];
        let mut reader = std::io::Cursor::new(payload.clone());
        let mut sink = Vec::new();
        let bytes = progress();

        receive_exactly(
            &mut reader,
            &mut sink,
            payload.len() as u64,
            &bytes,
            &mut no_cancel(),
        )
        .await
        .expect("complete");

        assert_eq!(bytes.load(Ordering::Relaxed), payload.len() as u64);
    }

    #[tokio::test]
    async fn the_send_half_refuses_to_ship_a_file_shorter_than_its_offer() {
        let mut reader = std::io::Cursor::new(vec![1u8; 5]);
        let mut sink = Vec::new();
        let result = send_exactly(&mut reader, &mut sink, 50, &progress(), &mut no_cancel()).await;
        assert_eq!(result.expect_err("must fail"), FailureReason::Integrity);
    }

    #[tokio::test]
    async fn hashing_a_file_matches_hashing_its_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sample.bin");
        let payload = b"anyflow files.v1".repeat(5000);
        tokio::fs::write(&path, &payload).await.expect("write");

        let (size, digest) = hash_file(&path).await.expect("hash");
        assert_eq!(size, payload.len() as u64);
        let expected: [u8; 32] = Sha256::digest(&payload).into();
        assert_eq!(digest, expected);
    }
}
