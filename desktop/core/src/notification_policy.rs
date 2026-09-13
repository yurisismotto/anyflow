//! Per-peer `notifications.v1` policy, as this device stores it.
//!
//! This type lives in `anyflow-core` rather than in the notifications
//! capability for the reason [`crate::clipboard_policy`] gives: it is
//! *persisted*, and the trust store is core's. A capability crate cannot own
//! a field of [`crate::store::TrustedPeer`] without core depending on it, and
//! core depending on a capability would invert the plugin model. The
//! capability re-exports it, so callers still say
//! `anyflow_capability_notifications::NotificationPolicy`.
//!
//! # Two different questions, and a third
//!
//! * The **grant** — in the trust store beside `files.v1`'s and
//!   `clipboard.v1`'s — answers *"may this device speak notifications with me
//!   at all?"*. It is never in `auto_grant` (ADR-0008, ADR-0015 §4).
//! * The **policy** — this type — answers *"and how much of each, and when?"*.
//! * The **roles** each side announces answer *"what can that device
//!   physically do right now?"*, and are never an authorization input
//!   (ADR-0017 §6).
//!
//! Collapsing any two of the three would mean somebody who wanted one thing
//! had silently authorised another.
//!
//! # A peer can never set its own policy
//!
//! Every field here is decided locally and stored locally. **There is no
//! protocol message that writes any of them** — `notifications_v1.proto` has
//! six bodies and none of them carries a setting — so the rule is an absence
//! rather than a check a later change could invert. That is NOTIF-SEC-20, and
//! it is the same guarantee `clipboard.v1` makes.
//!
//! # What is deliberately not here
//!
//! The **source-side** half of the policy — the per-app allow-list, the
//! work-profile switch, the ongoing switch, `when_source_locked` — belongs to
//! the device that owns the notifications, which in v1 is Android, and it is
//! stored there (`NotificationPolicy.kt`). A desktop copy of those fields
//! would be a second place to look and a second place to be wrong: the sink
//! cannot ask for more than it is sent, so the fields would have no effect
//! here even if they were set. N3 owns the surface that edits the Android
//! side; N2 stores only what this machine can actually enforce.

use serde::{Deserialize, Serialize};

/// How much of a notification is presented while **this** device is locked.
///
/// The names match the source-side enum deliberately: the two policies
/// compose, the source's applies first because it decides what crosses the
/// wire, and a reader comparing the two ends should not have to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockPolicy {
    /// Everything, as if unlocked.
    Full,
    /// The application's name only. No title, no body.
    #[default]
    AppOnly,
    /// Nothing is displayed at all, and an existing mirror is closed.
    Suppress,
}

impl LockPolicy {
    /// ADR-0015 §7: the default, on both ends.
    pub const DEFAULT: Self = Self::AppOnly;

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::AppOnly => "app-only",
            Self::Suppress => "suppress",
        }
    }

    /// Parses the spelling the CLI uses. Unknown input is **not** guessed at:
    /// a typo must not silently select the most permissive option.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "app-only" | "app_only" | "apponly" => Some(Self::AppOnly),
            "suppress" | "none" => Some(Self::Suppress),
            _ => None,
        }
    }
}

/// What this device will display for one peer.
///
/// # Defaults
///
/// ```text
/// allow_mirror       true      what the grant was for
/// when_sink_locked   AppOnly   ADR-0015 §7
/// allow_dismiss_sync false     ADR-0015 §6 — the only outbound authority
/// ```
///
/// `allow_mirror` defaulting on is not a silent widening: reaching this type
/// at all already required an explicit `anyflow grant <device>
/// notifications.v1`, which is never automatic. What the default *does* say is
/// that the grant means what it looks like it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPolicy {
    /// May this peer's notifications be displayed here at all?
    ///
    /// Off answers every upsert `REJECTED_POLICY` and closes any mirror the
    /// peer already has.
    #[serde(default = "default_true")]
    pub allow_mirror: bool,

    /// What is displayed while this desktop session is locked.
    ///
    /// The reduction happens **before** the notification is handed to the
    /// platform, so what policy withholds is never given to the notification
    /// server at all. A D-Bus client cannot tell GNOME how to render on the
    /// lock screen; the only lever is what goes into the call.
    #[serde(default)]
    pub when_sink_locked: LockPolicy,

    /// Whether a human closing a mirror here may dismiss the notification on
    /// the source device.
    ///
    /// **Off by default** (ADR-0015 §6), and per peer. Every other field in
    /// this type decides what this desktop will *display*; this one is the only
    /// authority that points outward, and it is the reason it is asked for
    /// separately even though the action it authorises is small.
    ///
    /// It is one of four independent gates on a dismissal, and the other three
    /// are deliberately not here: the peer's own `DISMISS_TARGET` claim, this
    /// device's `DISMISS_REPORTER` role, and the source's own
    /// `allowDismissSync`. A policy cannot manufacture a capability, and a
    /// capability cannot manufacture a permission.
    #[serde(default)]
    pub allow_dismiss_sync: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            allow_mirror: true,
            when_sink_locked: LockPolicy::DEFAULT,
            allow_dismiss_sync: false,
        }
    }
}

impl NotificationPolicy {
    /// The policy for a peer that has no `notifications.v1` grant: nothing is
    /// permitted, whatever the stored flags say.
    ///
    /// Returned by the authorizer so that a caller cannot ask "what is the
    /// policy?" and forget "is it granted?". Every field here is already
    /// closed, and `when_sink_locked` is `Suppress` rather than the default so
    /// that even a caller that read only that field reaches the safe answer.
    pub const DENIED: Self = Self {
        allow_mirror: false,
        when_sink_locked: LockPolicy::Suppress,
        allow_dismiss_sync: false,
    };

    /// True when nothing at all is permitted.
    pub fn is_denied(&self) -> bool {
        !self.allow_mirror
    }

    /// Whether a dismissal here may travel to the source.
    ///
    /// `allow_dismiss_sync` alone is not enough: a policy with `allow_mirror`
    /// off and `allow_dismiss_sync` on is contradictory and resolves to "no",
    /// the same containment rule `ClipboardPolicy::may_auto_send` applies.
    pub fn may_sync_dismissals(&self) -> bool {
        self.allow_mirror && self.allow_dismiss_sync
    }

    /// Renders the policy as the flag names the CLI uses. Contains no
    /// content and no identity; safe to log.
    pub fn describe(&self) -> String {
        let on = |b: bool| if b { "on" } else { "off" };
        format!(
            "mirror={} locked={} dismiss-sync={}",
            on(self.allow_mirror),
            self.when_sink_locked.as_str(),
            on(self.allow_dismiss_sync),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let p = NotificationPolicy::default();
        assert!(p.allow_mirror, "a granted peer's notifications are shown");
        assert_eq!(
            p.when_sink_locked,
            LockPolicy::AppOnly,
            "a locked desktop shows the app name and nothing else"
        );
        assert!(!p.allow_dismiss_sync, "dismiss sync must never default on");
        assert!(!p.may_sync_dismissals());
    }

    #[test]
    fn denied_closes_everything_including_the_lock_policy() {
        let p = NotificationPolicy::DENIED;
        assert!(p.is_denied());
        assert!(!p.may_sync_dismissals());
        assert_eq!(p.when_sink_locked, LockPolicy::Suppress);
    }

    #[test]
    fn withdrawing_mirroring_defeats_a_stale_dismiss_flag() {
        let p = NotificationPolicy {
            allow_mirror: false,
            allow_dismiss_sync: true,
            ..NotificationPolicy::default()
        };
        assert!(!p.may_sync_dismissals());
    }

    #[test]
    fn missing_fields_deserialize_to_the_documented_defaults() {
        // A trust store written before this capability existed has no policy
        // object at all, and one written by an older build may be missing
        // individual flags. Neither may silently widen what is displayed.
        let p: NotificationPolicy = serde_json::from_str("{}").expect("empty object is valid");
        assert_eq!(p, NotificationPolicy::default());

        let p: NotificationPolicy = serde_json::from_str(r#"{"allow_dismiss_sync":true}"#)
            .expect("partial object is valid");
        assert!(p.allow_mirror);
        assert_eq!(p.when_sink_locked, LockPolicy::AppOnly);
        assert!(p.allow_dismiss_sync);
    }

    #[test]
    fn an_unknown_lock_policy_spelling_is_refused_rather_than_guessed() {
        assert_eq!(LockPolicy::parse("full"), Some(LockPolicy::Full));
        assert_eq!(LockPolicy::parse("app-only"), Some(LockPolicy::AppOnly));
        assert_eq!(LockPolicy::parse("SUPPRESS"), Some(LockPolicy::Suppress));
        // Not `Full`, and not the default either: the caller is told.
        assert_eq!(LockPolicy::parse("ful"), None);
        assert_eq!(LockPolicy::parse(""), None);
    }

    #[test]
    fn describe_is_stable_and_carries_no_content() {
        assert_eq!(
            NotificationPolicy::default().describe(),
            "mirror=on locked=app-only dismiss-sync=off"
        );
    }
}
