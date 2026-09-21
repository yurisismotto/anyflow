//! Helpers for talking about a notification without printing one.
//!
//! Every diagnostic in this capability — a log line, a CLI row, a test
//! failure, an error `Display` — goes through here. The rule is absolute and
//! has no debug override: **a notification's title, body, application label
//! and application id are never rendered**, not at `trace` level, not behind a
//! feature flag, not in a panic message.
//!
//! What is said instead is the first 8 hex characters of the already-opaque
//! `notification_id`, the short form of the peer fingerprint, an event kind,
//! an outcome name and a count. Together those are enough to line up two
//! machines' logs; separately or together they are useless to someone reading
//! one.
//!
//! The `notification_id` is safe to prefix for a reason the clipboard's hash
//! prefix is not quite: it is `HMAC(per-install secret, platform key)` and
//! carries no content at all, not even a hash of one. Eight characters of it
//! identify a mirror across two logs and identify nothing else anywhere.

use omnibridge_core::notifications::NotificationId;

/// Lowercase hex for a byte slice.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut acc, b| {
        // Writing to a String cannot fail; the result is discarded rather
        // than unwrapped so this stays panic-free.
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// First 8 hex characters of a validated notification identity.
pub fn id_prefix(id: &NotificationId) -> String {
    to_hex(&id.as_bytes()[..4])
}

/// First 8 hex characters of an identity that has not been validated yet.
///
/// Used on the one path where the width is wrong and there is therefore no
/// [`NotificationId`] to talk about. Short input is rendered as far as it
/// goes rather than padded or panicked on.
pub fn raw_id_prefix(bytes: &[u8]) -> String {
    to_hex(&bytes[..bytes.len().min(4)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lowercase_and_fixed_width() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(to_hex(&[]), "");
    }

    #[test]
    fn a_prefix_is_eight_characters_and_survives_short_input() {
        let id = NotificationId::from_bytes(
            &[0xde, 0xad, 0xbe, 0xef, 0x11]
                .iter()
                .copied()
                .chain(std::iter::repeat(0u8))
                .take(16)
                .collect::<Vec<_>>(),
        )
        .expect("16 bytes");
        assert_eq!(id_prefix(&id), "deadbeef");
        assert_eq!(raw_id_prefix(&[0xde, 0xad]), "dead");
        assert_eq!(raw_id_prefix(&[]), "");
    }
}
