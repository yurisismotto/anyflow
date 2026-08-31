//! Helpers for talking about clipboard content without printing it.
//!
//! Every diagnostic in this capability — a log line, a CLI row, a test
//! failure — goes through here. The rule is absolute and has no debug
//! override: **clipboard text is never rendered**, not at `trace` level, not
//! behind a feature flag, not in a panic message. What we say instead is the
//! size, the hash prefix, and the event id, which together are enough to
//! correlate two devices' logs and useless to anyone reading one.

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

/// First 8 hex characters of a content hash.
///
/// Enough to match the same clip across two machines' logs while carrying
/// nothing about what the clip says. A hash prefix is not a fingerprint of
/// the content in any useful sense for an attacker: recovering the text would
/// mean guessing it, and if you can guess it you did not need the log.
pub fn hash_prefix(hash: &[u8]) -> String {
    to_hex(&hash[..hash.len().min(4)])
}

/// First 8 hex characters of an event id, for correlating a result with the
/// update it answers.
pub fn event_prefix(event_id: &[u8]) -> String {
    to_hex(&event_id[..event_id.len().min(4)])
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
    fn prefixes_are_short_and_survive_short_input() {
        assert_eq!(hash_prefix(&[0xde, 0xad, 0xbe, 0xef, 0x99]), "deadbeef");
        assert_eq!(hash_prefix(&[0xde]), "de");
        assert_eq!(event_prefix(&[0x01, 0x02]), "0102");
    }
}
