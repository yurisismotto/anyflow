//! Every bound `clipboard.v1` enforces, in one place.
//!
//! The rule these follow is the one `files.v1` follows: a limit exists to stop
//! a hostile or broken peer from consuming something unbounded, and it is set
//! high enough that an honest user never meets it.

use std::time::Duration;

/// Length of a clipboard event id, in bytes. 128 bits of CSPRNG output.
pub const EVENT_ID_LEN: usize = 16;

/// Length of a content hash, in bytes. SHA-256.
pub const CONTENT_HASH_LEN: usize = 32;

/// Largest clipboard text this device will send or accept, in UTF-8 bytes.
///
/// # Why 32 KiB, and why it is not the frame limit
///
/// `clipboard.v1` is control-plane data: an update travels inside an
/// `Envelope`, so it is bounded by `MAX_FRAME_LEN` (64 KiB) whether we say so
/// or not. Setting the text ceiling *at* the frame limit would mean a
/// legal-by-our-rules clip could not be framed at all, because the envelope
/// also carries a 16-byte message id, a 32-byte hash, an event id, a device
/// id, protobuf tags and length prefixes. The capability limit therefore has
/// to sit below the transport limit with room to spare.
///
/// 32 KiB leaves ~32 KiB of headroom — far more than the few hundred bytes of
/// overhead actually needed — which is deliberate: the margin should absorb a
/// future field without becoming a wire-format change.
///
/// It is also generous for what a clipboard *is*. 32 KiB is roughly 32,000
/// ASCII characters, or around 8,000 CJK characters: a long shell command, a
/// stack trace, a whole configuration file. Content past that is a document,
/// and a document is what `files.v1` is for — which is exactly what the
/// TOO_LARGE outcome tells the sender.
///
/// Raising this is not free: it would have to be raised on both platforms at
/// once, and it must never approach `MAX_FRAME_LEN`.
pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 32 * 1024;

/// How many recently handled `event_id`s are remembered per peer.
///
/// De-duplication has to be idempotent across a burst, not across all time.
/// 256 covers a rapid copy sequence many times over, and the cache stores
/// ids and monotonic instants — never content.
pub const EVENT_CACHE_ENTRIES: usize = 256;

/// How long a handled `event_id` stays in the de-duplication cache.
///
/// Bounded in time as well as in count so that a quiet session does not keep
/// yesterday's ids alive. Both bounds are enforced; whichever bites first
/// wins.
pub const EVENT_CACHE_TTL: Duration = Duration::from_secs(300);

/// How many content hashes may sit in the loop-suppression cache at once.
///
/// One entry is added each time a *remote* clip is written to the local
/// clipboard. In practice the cache holds zero or one; the bound exists so
/// that a peer flooding updates cannot grow it without limit.
pub const SUPPRESSION_ENTRIES: usize = 64;

/// How long a suppression entry survives if the local watcher never fires.
///
/// This window is the gap between "we wrote the clipboard" and "the platform
/// told us the clipboard changed". On a healthy desktop that is milliseconds.
/// It is deliberately short: a stale entry would swallow a *legitimate*
/// re-copy of the same text, which is the failure mode a naive
/// `if new == old` check has permanently.
pub const SUPPRESSION_TTL: Duration = Duration::from_secs(10);

/// How long a clip that needs a human sits in memory before it is dropped.
///
/// Applies when `auto_receive` is off: the text is held, unwritten, until the
/// user applies it or this expires. Clipboard content must not outlive the
/// reason it exists, and "the user walked away" is not a reason.
pub const PENDING_CLIP_TTL: Duration = Duration::from_secs(300);

/// How long any single clipboard backend operation may take.
///
/// Not a nicety. On a GNOME Wayland session `wl-copy` and `wl-paste` need to
/// acquire a seat and a serial, which the compositor will not give them while
/// the session is locked — the call then blocks *forever* rather than
/// failing. Every backend invocation is bounded by this and the child is
/// killed when it expires, so a locked screen degrades to a clean
/// `Unavailable` instead of leaking a stuck process per attempt.
pub const BACKEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Shortest gap between two restarts of a failed watcher.
///
/// The watcher is supervised: if the backend dies — the compositor restarted,
/// the helper was killed — it is restarted rather than silently lost. Backoff
/// starts here and doubles to [`WATCH_RESTART_MAX_BACKOFF`], so a backend
/// that is permanently unavailable costs one attempt a minute rather than a
/// spin loop.
pub const WATCH_RESTART_MIN_BACKOFF: Duration = Duration::from_secs(1);

/// Longest gap between two restarts of a failed watcher.
pub const WATCH_RESTART_MAX_BACKOFF: Duration = Duration::from_secs(60);

/// How long a control message may wait for room in the session's outbound
/// queue before it is given up on.
///
/// Mirrors `files.v1`'s bound of the same name and exists for the same
/// reason: a capability handler runs *inside* the session's dispatch loop, so
/// an unbounded wait to enqueue a reply is a wait on the loop that would
/// drain it. The writer is a separate task now, which is what makes this a
/// backstop rather than the only thing preventing a deadlock — but a peer
/// that has stopped reading must still not be able to park a handler
/// indefinitely.
pub const CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(2);
