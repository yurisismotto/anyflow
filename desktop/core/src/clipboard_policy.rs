//! Per-peer clipboard policy, as it is stored.
//!
//! This type lives in `omnibridge-core` rather than in the clipboard capability
//! for one reason: it is *persisted*, and the trust store is core's. A
//! capability crate cannot own a field of `TrustedPeer` without core
//! depending on it, and core depending on a capability would invert the whole
//! plugin model. The capability re-exports it, so callers still say
//! `omnibridge_capability_clipboard::ClipboardPolicy`.
//!
//! # Two different questions
//!
//! `clipboard.v1` has a *grant* and a *policy*, and they are deliberately not
//! the same thing:
//!
//! * The **grant** — held in the trust store next to `files.v1`'s — answers
//!   "may this device speak clipboard with me at all?". It is never
//!   auto-granted (ADR-0008), it is what the capability negotiation filters
//!   on, and withdrawing it takes effect immediately.
//! * The **policy** — this type — answers "and in which directions, and how
//!   automatically?". It exists because a single yes/no cannot express the
//!   thing a clipboard user actually wants, which is usually "let my desktop
//!   push to my tablet, but do not let my tablet silently overwrite what I am
//!   about to paste".
//!
//! Collapsing them would mean either a grant that quietly enables automatic
//! two-way sync, or four capability ids. Neither is right.
//!
//! # A peer can never set its own policy
//!
//! Every field here is decided locally and stored locally. There is no
//! protocol message that changes any of them, on purpose: a policy a peer can
//! widen is not a policy. A peer's only influence is that its updates are
//! measured against them.

use serde::{Deserialize, Serialize};

/// What this device will do with clipboard traffic for one peer.
///
/// # Defaults
///
/// ```text
/// allow_send    true    manual send is what the grant was for
/// allow_receive true    manual receive is what the grant was for
/// auto_send     false   never automatic without a second, explicit yes
/// auto_receive  false   never automatic without a second, explicit yes
/// ```
///
/// The split is the point. Reaching these defaults already required an
/// explicit `omnibridge grant <device> clipboard.v1`, which is never automatic —
/// so the two `allow_*` flags are not a silent widening, they are what the
/// human just asked for. The two `auto_*` flags are the ones the brief cares
/// about and they are off: nothing leaves this machine because of a copy, and
/// nothing reaches this machine's clipboard because of a peer, until someone
/// turns that on for that specific device.
///
/// Concretely, with a fresh grant and nothing else:
///
/// * copying locally sends nothing anywhere;
/// * a peer's update is accepted, held in memory, and reported
///   `PENDING_USER` — it does not touch the system clipboard;
/// * `omnibridge clipboard send <device>` works, because the human asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPolicy {
    /// May this device send clipboard text to that peer at all — manually or
    /// automatically? Off makes every send a no-op, including
    /// `omnibridge clipboard send`.
    #[serde(default = "default_true")]
    pub allow_send: bool,

    /// May that peer's clipboard updates be accepted at all? Off answers
    /// every update with REJECTED_POLICY and the text is dropped without
    /// being held anywhere.
    #[serde(default = "default_true")]
    pub allow_receive: bool,

    /// Push local clipboard changes to that peer as they happen.
    ///
    /// This is the flag that turns on the local clipboard watcher. Off by
    /// default and off for every newly granted peer: it means *everything you
    /// copy* — passwords included — leaves this machine, and no desktop
    /// environment offers a reliable equivalent of Android's
    /// `EXTRA_IS_SENSITIVE` to filter that. Turning it on must be a decision,
    /// with that consequence stated.
    #[serde(default)]
    pub auto_send: bool,

    /// Write that peer's updates straight to the system clipboard.
    ///
    /// Off by default. With it off an accepted update is held in memory and
    /// reported `PENDING_USER`, so a paired-but-misbehaving device cannot
    /// replace what you are about to paste. On, it applies immediately, which
    /// is what makes desktop-to-phone sync feel automatic.
    #[serde(default)]
    pub auto_receive: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ClipboardPolicy {
    fn default() -> Self {
        Self {
            allow_send: true,
            allow_receive: true,
            auto_send: false,
            auto_receive: false,
        }
    }
}

impl ClipboardPolicy {
    /// The policy for a peer that has no `clipboard.v1` grant: nothing is
    /// permitted, whatever the stored flags say.
    ///
    /// Used so that a caller cannot forget to check the grant separately —
    /// the authorizer returns this and every direction is already closed.
    pub const DENIED: Self = Self {
        allow_send: false,
        allow_receive: false,
        auto_send: false,
        auto_receive: false,
    };

    /// True when nothing at all is permitted.
    pub fn is_denied(&self) -> bool {
        !self.allow_send && !self.allow_receive
    }

    /// Whether an automatic push to this peer is permitted right now.
    ///
    /// `auto_send` alone is not enough: a policy with `allow_send` off and
    /// `auto_send` on is contradictory, and it resolves to "no". Storing the
    /// two independently and resolving here means turning `allow_send` off
    /// cannot be defeated by a stale `auto_send`.
    pub fn may_auto_send(&self) -> bool {
        self.allow_send && self.auto_send
    }

    /// Whether an inbound update may be written to the clipboard without
    /// asking. Same containment rule as [`may_auto_send`].
    ///
    /// [`may_auto_send`]: Self::may_auto_send
    pub fn may_auto_receive(&self) -> bool {
        self.allow_receive && self.auto_receive
    }

    /// Renders the policy as the flag names the CLI uses. Contains no
    /// content and no identity; safe to log.
    pub fn describe(&self) -> String {
        let on = |b: bool| if b { "on" } else { "off" };
        format!(
            "send={} receive={} auto-send={} auto-receive={}",
            on(self.allow_send),
            on(self.allow_receive),
            on(self.auto_send),
            on(self.auto_receive),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_directions_are_off_by_default() {
        let p = ClipboardPolicy::default();
        assert!(p.allow_send, "a granted peer can be sent to by hand");
        assert!(
            p.allow_receive,
            "a granted peer can be received from by hand"
        );
        assert!(!p.auto_send, "auto-send must never default on");
        assert!(!p.auto_receive, "auto-receive must never default on");
        assert!(!p.may_auto_send());
        assert!(!p.may_auto_receive());
    }

    #[test]
    fn denied_closes_every_direction() {
        let p = ClipboardPolicy::DENIED;
        assert!(p.is_denied());
        assert!(!p.may_auto_send());
        assert!(!p.may_auto_receive());
    }

    #[test]
    fn withdrawing_a_direction_defeats_a_stale_auto_flag() {
        // The state a user reaches by turning auto-send on and then turning
        // the whole direction off. The direction must win.
        let p = ClipboardPolicy {
            allow_send: false,
            auto_send: true,
            allow_receive: false,
            auto_receive: true,
        };
        assert!(!p.may_auto_send());
        assert!(!p.may_auto_receive());
    }

    #[test]
    fn describe_is_stable_and_carries_no_content() {
        let p = ClipboardPolicy::default();
        assert_eq!(
            p.describe(),
            "send=on receive=on auto-send=off auto-receive=off"
        );
    }

    #[test]
    fn missing_fields_deserialize_to_the_documented_defaults() {
        // A trust store written before this capability existed has no
        // policy object at all, and one written by an older build may be
        // missing individual flags. Neither may silently turn automation on.
        let p: ClipboardPolicy = serde_json::from_str("{}").expect("empty object is valid");
        assert_eq!(p, ClipboardPolicy::default());

        let p: ClipboardPolicy =
            serde_json::from_str(r#"{"allow_send":false}"#).expect("partial object is valid");
        assert!(!p.allow_send);
        assert!(p.allow_receive);
        assert!(!p.auto_send);
        assert!(!p.auto_receive);
    }
}
