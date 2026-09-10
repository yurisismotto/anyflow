//! Every bound this sink applies, and which side of the wire owns it.
//!
//! # None of these is protocol
//!
//! [02 §7.5](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md)
//! settles this and it is worth restating where the numbers actually live:
//! only the `BEGIN`/`END` bracketing and the remove-what-is-not-named rule are
//! protocol. Everything in this file is **sink-local**. It is never
//! negotiated, never on the wire, and a future release may tune any of it
//! without a version bump, a capability flag or a compatibility note. Two
//! peers running different values interoperate correctly.
//!
//! What *is* normative about the grace is its bound, not its value: greater
//! than zero, or a three-second Wi-Fi blip clears the desktop and re-posts
//! everything; and finite, or a phone that left the building leaves
//! notifications on a screen that can no longer update or dismiss them.

use std::time::Duration;

/// How long a disconnected peer's mirrors stay on screen before they are
/// closed.
///
/// Chosen, not measured (OQ-06). N5 owns the tuning; the reason a chosen value
/// is acceptable is the paragraph above.
pub const RECONNECT_GRACE: Duration = Duration::from_secs(60);

/// How long an unfinished snapshot may stay open before it is abandoned.
///
/// Abandoning removes **nothing**: an incomplete snapshot can never be
/// completed, so it must never become authoritative. The grace above runs
/// underneath it and is what eventually clears a peer that has gone silent.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(30);

/// The most mirrors one peer may hold on this desktop at once.
///
/// The cap is per peer rather than global, so a flooding peer cannot evict
/// another peer's notifications — the same containment the mirror key gives.
pub const MAX_MIRRORS_PER_PEER: usize = 200;

/// The most work items queued for one peer before the oldest non-terminal one
/// is evicted.
///
/// The queue exists so that a hung notification server cannot stall the
/// session's dispatch loop and take `battery.v1`, `files.v1` and
/// `clipboard.v1` down with it. It is bounded so that a flooding peer cannot
/// turn that isolation into unbounded memory.
pub const MAX_QUEUED_WORK: usize = 256;

/// How long any single backend call may take before it is abandoned.
///
/// A `Notify` on a healthy GNOME session is about a millisecond. This is not
/// a performance budget; it is the bound that makes "a broken sink does not
/// break the session" true even when the server is present but wedged.
pub const BACKEND_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest summary this sink will hand to a notification server.
///
/// Presentation, not protocol: the wire already caps `title` at 512 bytes and
/// this is not that limit re-stated. It is the point past which a summary
/// stops being a summary, and it exists because sanitisation can only remove
/// characters — a peer that sends 512 bytes of one long word would otherwise
/// hand the shell a 512-byte headline.
pub const MAX_SUMMARY_CHARS: usize = 120;

/// The longest body this sink will hand to a notification server.
///
/// Same reasoning. Markup escaping can grow a string — `&` becomes `&amp;` —
/// so this is applied *after* escaping, where the real size is known.
pub const MAX_BODY_CHARS: usize = 2000;

/// The longest application name this sink will hand to a notification server.
pub const MAX_APP_NAME_CHARS: usize = 64;
