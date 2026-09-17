//! Every bound `files.v1` enforces, in one place.
//!
//! The rule these follow: a limit exists to stop a hostile or broken peer
//! from consuming something unbounded, and it is set high enough that an
//! honest user never meets it. A limit that makes normal use fail is not a
//! security control, it is a bug with a good excuse.

use std::time::Duration;

/// Length of a transfer id, in bytes. 128 bits of CSPRNG output.
pub const TRANSFER_ID_LEN: usize = 16;

/// Length of the single-use data-stream challenge, in bytes.
///
/// Matches the pairing nonce: it is an HMAC key, and 256 bits is the natural
/// size for one keying SHA-256.
pub const STREAM_CHALLENGE_LEN: usize = 32;

/// How many transfers one peer may have in flight at once.
///
/// Bounds open file descriptors, temp files and data-stream tasks per peer.
/// Four is enough for a handful of photos shared in one gesture and far
/// below anything that could exhaust a desktop.
pub const MAX_CONCURRENT_TRANSFERS_PER_PEER: usize = 4;

/// Default ceiling on a single file, in bytes (16 GiB).
///
/// Deliberately large: the streaming implementation does not care how big a
/// file is, so a low limit would buy nothing and block real use (a video, a
/// VM image). It exists so that "how much disk can a paired peer fill in one
/// transfer" has an answer at all. Configurable per install.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Longest filename accepted, in bytes.
///
/// 255 is `NAME_MAX` on every filesystem this runs on, so a longer name could
/// not be written even if we wanted to. Counted in *bytes*, not characters,
/// because that is what the filesystem counts.
pub const MAX_FILENAME_BYTES: usize = 255;

/// Longest MIME type string accepted, in bytes. The longest real IANA type is
/// well under 100; this is generous and still bounded.
pub const MAX_MIME_TYPE_BYTES: usize = 128;

/// How long the receiver's human has to answer an offer.
///
/// After this the offer is withdrawn and the sender is told it timed out, so
/// a phone that shared a file and locked its screen does not leave a
/// transfer pending on the desktop forever.
pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(120);

/// How much longer than the accept timeout the approval *future* is allowed
/// to run before it is dropped.
///
/// There are two bounds on an unanswered offer and only one of them should
/// ever fire. The reaper's — `deadline_for(WaitingAccept)` — is the one with
/// the right word for what happened, [`crate::transfer::FailureReason::TimedOut`],
/// and it is the one the peer should hear. The timeout wrapped around
/// `confirm_receive` exists only so a provider that never answers cannot leak
/// a task; a decline is all it can say, and "declined by the user" is the
/// wrong thing to tell a phone about a prompt nobody touched.
///
/// So this grace makes the backstop strictly later than the reaper, by more
/// than one [`REAP_INTERVAL`] tick. Without it the two land on the same
/// instant and which reason the peer is told becomes a race.
pub const APPROVAL_BACKSTOP_GRACE: Duration = Duration::from_secs(5);

/// How long, after an acceptance, the dialer has to open the data stream.
///
/// Short: both peers are already connected and the dial is a local TCP+TLS
/// handshake. Its job is to reclaim a transfer whose sender agreed and then
/// vanished.
pub const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a data stream may make no progress before it is abandoned.
///
/// Applies to the *stream*, not the transfer: a large file legitimately takes
/// minutes, but a stream that has moved no bytes for a minute is stalled.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long the acceptor waits for the [`DataStreamAuth`] frame after the TLS
/// handshake, before dropping the connection.
///
/// [`DataStreamAuth`]: anyflow_proto::v1::capabilities::DataStreamAuth
pub const STREAM_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a sender waits, after its last byte, for the receiver's verdict.
///
/// The receiver still has to hash the whole file and promote it, which for a
/// large file is not instant. Generous for that reason, and bounded so a
/// receiver that simply stops talking does not leave the sender's transfer
/// live forever.
pub const VERDICT_TIMEOUT: Duration = Duration::from_secs(600);

/// How often the reaper looks for transfers that have timed out, lost their
/// control session, or lost their authorization.
pub const REAP_INTERVAL: Duration = Duration::from_secs(1);

/// How long a control message may wait for room in the session's outbound
/// queue before it is given up on.
///
/// This bound is not a nicety, it is what stops a deadlock. A capability's
/// `on_message` is awaited *inside* the session loop, and that same loop is
/// what drains the outbound queue — so a handler that blocks forever waiting
/// to enqueue a reply blocks the very task that would make room for it. A
/// peer that floods offers faster than the writer drains them could otherwise
/// wedge its session permanently.
///
/// Two seconds is far longer than a healthy session ever needs (the queue is
/// 32 deep and control messages are rare), and a peer that has not made room
/// in that time is not reading, which the liveness probe will conclude
/// shortly afterwards anyway.
pub const CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(2);

/// Copy buffer size. This, and not the file size, is what a transfer costs in
/// memory — the property FILE-13 exists to protect.
pub const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Cap on the two protobuf frames that open a data stream.
///
/// Both are tens of bytes. The cap is checked before allocation, exactly as
/// `MAX_FRAME_LEN` is on the control session, so an unauthenticated frame
/// cannot cause a large allocation.
pub const MAX_DATA_STREAM_FRAME: usize = 4096;

/// How many `name (n).ext` variants to try before giving up on a duplicate.
pub const MAX_DUPLICATE_SUFFIX: u32 = 999;

/// How long a sender waits for the peer's own verdict after its data stream
/// ends unexpectedly, before calling the transfer a transport failure.
///
/// A receiver that cancels does two things at once: it tears down the data
/// stream and it sends FILE_CANCEL on the control session. Those travel on
/// two different TLS connections and therefore race, and the stream's EOF
/// normally wins — so without this window the sender would report every
/// receiver-side cancellation as "the connection ended mid-transfer", which
/// is both wrong and indistinguishable from a real network failure.
///
/// Short, because it delays only the *reason* attached to an already-stopped
/// transfer, never any bytes.
pub const PEER_VERDICT_GRACE: Duration = Duration::from_millis(750);

/// How often [`PEER_VERDICT_GRACE`] is re-checked. Fine enough that the
/// common case — the cancel is already a few milliseconds behind — costs
/// almost nothing.
pub const PEER_VERDICT_POLL: Duration = Duration::from_millis(25);
