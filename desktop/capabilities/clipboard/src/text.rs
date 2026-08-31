//! What counts as clipboard text, and what it hashes to.
//!
//! This module is the single place that decides whether a string is
//! acceptable as a `clipboard.v1` payload. It has a twin in Kotlin
//! (`ClipboardText.kt`) and the two must agree exactly — a clip that one
//! platform sends and the other refuses is a bug, not a policy.

use sha2::{Digest, Sha256};

use crate::limits::{CONTENT_HASH_LEN, MAX_CLIPBOARD_TEXT_BYTES};

/// Why a candidate string is not usable as clipboard text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRejection {
    /// Nothing to share. An empty clip carries no information, and accepting
    /// one would hand a peer a way to blank the local clipboard.
    Empty,
    /// Past [`MAX_CLIPBOARD_TEXT_BYTES`]. Never truncated: see
    /// [`validate`].
    TooLarge {
        /// Size of the offending text in UTF-8 bytes.
        bytes: usize,
    },
    /// Contains U+0000. See [`validate`] for why this is refused rather than
    /// carried.
    ContainsNul,
}

impl TextRejection {
    /// A short, peer-safe description. Contains no clipboard content.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLarge { .. } => "too large",
            Self::ContainsNul => "contains a NUL character",
        }
    }
}

impl std::fmt::Display for TextRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(
                f,
                "clipboard text is {bytes} bytes; the limit is \
                 {MAX_CLIPBOARD_TEXT_BYTES} bytes. Use `anyflow send` to \
                 share something this size as a file."
            ),
            other => f.write_str(other.as_str()),
        }
    }
}

/// Validated clipboard text, with its content hash.
///
/// Constructing one is the only way to get text into an outbound update or
/// onto the local clipboard, so every path is validated by construction.
///
/// # Debug
///
/// The [`std::fmt::Debug`] implementation deliberately does **not** print the
/// text. A `#[derive(Debug)]` here would put clipboard content into every
/// `tracing` field that used `?value`, into every `unwrap` panic message, and
/// into every test-failure report — which is precisely the leak the logging
/// audit exists to prevent. See [`crate::redact`].
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardText {
    text: String,
    hash: [u8; CONTENT_HASH_LEN],
}

impl std::fmt::Debug for ClipboardText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardText")
            .field("bytes", &self.text.len())
            .field("hash", &crate::redact::hash_prefix(&self.hash))
            .finish_non_exhaustive()
    }
}

impl ClipboardText {
    /// Validates a candidate string.
    ///
    /// # What is accepted
    ///
    /// Any valid UTF-8 that is non-empty, within
    /// [`MAX_CLIPBOARD_TEXT_BYTES`], and free of U+0000. Nothing is
    /// normalised, trimmed or re-encoded: ASCII, accented Latin, CJK, emoji,
    /// tabs, CR, LF and CRLF all pass through byte-for-byte. A clipboard that
    /// "helpfully" rewrote content would corrupt exactly the payloads —
    /// indentation-sensitive code, a password with a trailing space — that a
    /// user is least able to notice.
    ///
    /// Rust's `String` is UTF-8 by construction, so the encoding check is
    /// structural here; on the wire it is real, because protobuf `string`
    /// fields are validated on decode and a peer that sends invalid UTF-8
    /// fails to parse at all.
    ///
    /// # Why NUL is refused
    ///
    /// U+0000 is valid UTF-8 and valid in a Rust `String`, so this is a
    /// choice rather than a consequence. It is refused because it cannot be
    /// carried *faithfully* end to end: the X11 `UTF8_STRING` and
    /// `text/plain` conventions do not carry it, and any consumer along the
    /// path that touches a C string API truncates at the first NUL — silently.
    /// Silent truncation of a clipboard is the failure this capability is
    /// least able to tolerate, and refusing is the only behaviour that is the
    /// same on both platforms and visible to the user.
    ///
    /// # Why oversize is refused rather than truncated
    ///
    /// A truncated password, key or command is not a degraded version of the
    /// original — it is a different, wrong value that looks plausible. The
    /// sender is told TOO_LARGE and pointed at `files.v1`.
    pub fn validate(text: impl Into<String>) -> Result<Self, TextRejection> {
        let text = text.into();

        if text.is_empty() {
            return Err(TextRejection::Empty);
        }
        if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return Err(TextRejection::TooLarge { bytes: text.len() });
        }
        if text.contains('\0') {
            return Err(TextRejection::ContainsNul);
        }

        let hash = content_hash(&text);
        Ok(Self { text, hash })
    }

    /// The text itself. Every caller of this is a place clipboard content
    /// legitimately leaves the type: the system clipboard, or the wire.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// SHA-256 over the UTF-8 bytes.
    pub fn hash(&self) -> &[u8; CONTENT_HASH_LEN] {
        &self.hash
    }

    /// Size in UTF-8 bytes. Safe to log.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        // Unreachable by construction — `validate` refuses an empty string —
        // but clippy wants it next to `len`, and an honest implementation
        // costs nothing.
        self.text.is_empty()
    }
}

/// SHA-256 over the UTF-8 bytes of `text`.
///
/// This is the cross-language contract: Kotlin's `ClipboardText.contentHash`
/// computes the same bytes, and the test vectors pin it.
pub fn content_hash(text: &str) -> [u8; CONTENT_HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trips_unchanged() {
        let t = ClipboardText::validate("hello").expect("ascii is valid");
        assert_eq!(t.as_str(), "hello");
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn unicode_is_never_modified() {
        // Portuguese, Spanish, emoji (including a flag, which is two
        // codepoints), CJK, and mixed line endings.
        for case in [
            "olá, ação, coração",
            "¿cómo estás? ñandú",
            "🇧🇷 🎉 👨‍👩‍👧‍👦",
            "日本語のテキスト 中文 한국어",
            "line1\nline2\r\nline3\ttabbed",
            "  leading and trailing  ",
        ] {
            let t = ClipboardText::validate(case).expect("valid text");
            assert_eq!(t.as_str(), case, "content was modified");
            assert_eq!(t.len(), case.len(), "byte length changed");
        }
    }

    #[test]
    fn empty_is_refused() {
        assert_eq!(ClipboardText::validate(""), Err(TextRejection::Empty));
    }

    #[test]
    fn nul_is_refused_explicitly() {
        // Interior and trailing, since a C-string consumer treats them
        // differently and neither is acceptable.
        assert_eq!(
            ClipboardText::validate("abc\0def"),
            Err(TextRejection::ContainsNul)
        );
        assert_eq!(
            ClipboardText::validate("abc\0"),
            Err(TextRejection::ContainsNul)
        );
    }

    #[test]
    fn the_size_limit_is_a_byte_limit_not_a_character_limit() {
        let max = "a".repeat(MAX_CLIPBOARD_TEXT_BYTES);
        assert!(
            ClipboardText::validate(&max).is_ok(),
            "the limit is inclusive"
        );

        let over = "a".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1);
        assert_eq!(
            ClipboardText::validate(&over),
            Err(TextRejection::TooLarge {
                bytes: MAX_CLIPBOARD_TEXT_BYTES + 1
            })
        );

        // A multi-byte character that crosses the boundary is measured in
        // bytes, not characters: 4-byte emoji.
        let emoji_count = MAX_CLIPBOARD_TEXT_BYTES / 4;
        let exact = "🎉".repeat(emoji_count);
        assert_eq!(exact.len(), MAX_CLIPBOARD_TEXT_BYTES);
        assert!(ClipboardText::validate(&exact).is_ok());
        assert!(ClipboardText::validate(exact + "🎉").is_err());
    }

    #[test]
    fn oversize_is_never_truncated() {
        let over = "a".repeat(MAX_CLIPBOARD_TEXT_BYTES + 100);
        // The point: there is no code path that returns a shortened Ok.
        assert!(ClipboardText::validate(over).is_err());
    }

    #[test]
    fn the_hash_is_sha256_over_utf8_bytes() {
        // Pinned vector, so Kotlin and Rust cannot drift.
        let t = ClipboardText::validate("hello").expect("valid");
        assert_eq!(
            crate::redact::to_hex(t.hash()),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );

        let t = ClipboardText::validate("olá 🇧🇷").expect("valid");
        // Recomputed here rather than pinned blind: the property under test
        // is that the hash is over the UTF-8 bytes, nothing else.
        assert_eq!(t.hash(), &content_hash("olá 🇧🇷"));
        assert_ne!(t.hash(), &content_hash("ola 🇧🇷"));
    }

    #[test]
    fn debug_never_prints_the_content() {
        let t = ClipboardText::validate("super-secret-password").expect("valid");
        let rendered = format!("{t:?}");
        assert!(
            !rendered.contains("super-secret-password"),
            "Debug leaked clipboard content: {rendered}"
        );
        assert!(rendered.contains("bytes"), "Debug should still be useful");
    }
}
