//! Turning attacker-controlled text into something safe to hand a shell.
//!
//! # Everything here is untrusted
//!
//! `app_id`, `app_label`, `title` and `body` all originate in an arbitrary
//! application on someone else's phone. OmniBridge's peer is authenticated; the
//! *content* it forwards is not, and cannot be — the whole point of the
//! capability is to carry text an unrelated third party wrote.
//!
//! `omnibridge_core::notifications` has already refused anything past a field
//! limit, anything containing U+0000, and — structurally, because protobuf
//! will not decode an invalid `string` — anything that is not UTF-8. What is
//! left is this module's job: characters that are individually legal UTF-8 and
//! individually hostile to a *display*.
//!
//! # Three rules
//!
//! 1. **Control characters go.** They forge terminal output in a journal, and
//!    a carriage return in a summary rewrites the line. `\n` and `\t` survive
//!    in a body, because a chat message legitimately contains both and a
//!    notification body is a multi-line field; nothing survives in a summary,
//!    which is one line by definition.
//! 2. **Unicode format characters go**, the bidi overrides above all. The
//!    reasoning and the character list are `files.v1`'s `filename.rs`, and
//!    they are shared deliberately: `U+202E RIGHT-TO-LEFT OVERRIDE` reverses
//!    the rendering of everything after it, which is how `…cod.exe` displays
//!    as `…exe.doc`, and a notification body is read by exactly the same eyes.
//! 3. **Markup is escaped, never interpreted.** GNOME advertises `body-markup`
//!    (HOST VERIFIED, `GetCapabilities`), so a body containing `<b>` would be
//!    parsed. A peer must not be able to forge emphasis, hide text inside a
//!    tag, or break the parser — so `&`, `<` and `>` are escaped before the
//!    body reaches the server.
//!
//! Stripping rather than refusing, for `filename.rs`'s reason: a reduced piece
//! of the user's own text is still their text, and a refusal here would drop a
//! notification because somebody typed a zero-width space.
//!
//! # Nothing here logs
//!
//! No function in this module writes a `tracing` event, and none of them
//! returns an error carrying the value it rejected. The only thing that leaves
//! is the sanitized string itself, and it goes to the notification server.

use crate::limits::{MAX_APP_NAME_CHARS, MAX_BODY_CHARS, MAX_SUMMARY_CHARS};

/// Unicode `Cf` format characters and the invisible tag block.
///
/// Copied from `omnibridge_capability_files::filename` rather than shared through
/// a dependency, because a capability crate depending on another capability
/// crate to sanitize its own display text would make the two features'
/// lifetimes one. Ranges are Unicode 16.0; a character added to `Cf` later is
/// a missed strip, not an unsafe one.
fn is_format_char(c: char) -> bool {
    matches!(c as u32,
        0x00AD
        | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2
        | 0x180E
        | 0x200B..=0x200F
        | 0x202A..=0x202E
        | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF
        | 0xFFF9..=0xFFFB
        | 0x110BD | 0x110CD
        | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3
        | 0x1D173..=0x1D17A
        | 0xE0001 | 0xE0020..=0xE007F
    )
}

/// Truncates on a **character** boundary and marks the cut.
///
/// Characters rather than bytes: the wire limits are byte limits and are
/// already enforced upstream, whereas this is a presentation limit and cutting
/// a 3-byte character in half here would produce a string that is no longer
/// UTF-8. The ellipsis is visible on purpose — a silently shortened message is
/// a message the reader believes they have all of.
fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A one-line field: a summary, or an application name.
///
/// Every control character goes, newlines included, and the result is
/// collapsed at the edges. A summary that is only whitespace becomes empty,
/// and the caller decides what to do about that — this function never
/// substitutes a placeholder, because inventing a headline for a notification
/// whose own headline was hostile hides the fact from the reader.
pub fn sanitize_line(raw: &str, max_chars: usize) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !is_format_char(*c))
        .collect();
    truncate_chars(cleaned.trim(), max_chars)
}

/// A multi-line field: a notification body.
///
/// `\n` and `\t` survive; every other control character does not. Trailing
/// whitespace goes, leading whitespace goes, and the interior is left exactly
/// as the app wrote it.
pub fn sanitize_block(raw: &str, max_chars: usize) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| (!c.is_control() || *c == '\n' || *c == '\t') && !is_format_char(*c))
        .collect();
    truncate_chars(cleaned.trim(), max_chars)
}

/// Escapes the three characters a `body-markup` server parses.
///
/// Only three, and deliberately not a general HTML escaper: the freedesktop
/// body markup is a small named subset (`<b>`, `<i>`, `<u>`, `<a>`, `<img>`)
/// parsed with an XML parser, and `&`, `<`, `>` are what open a tag or an
/// entity. `'` and `"` matter inside an attribute value, and nothing here ever
/// produces an attribute, because nothing here ever produces a tag.
///
/// The order matters: `&` first, or the ampersand introduced by escaping `<`
/// would be escaped a second time.
pub fn escape_markup(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// The application name handed to the server as `app_name`.
pub fn app_name(label: &str, app_id: &str) -> String {
    let label = sanitize_line(label, MAX_APP_NAME_CHARS);
    if !label.is_empty() {
        return label;
    }
    // The source resolves the label from its own package database and can fail
    // to — a package uninstalled between posting and mirroring, a label that
    // was only whitespace. Falling back to the package name is honest: it is
    // the identifier the notification actually carries, and it is the one the
    // per-app rules are written against. It is sanitized like everything else,
    // because a package name is a string an app chose.
    sanitize_line(app_id, MAX_APP_NAME_CHARS)
}

/// The summary handed to the server.
///
/// The freedesktop specification says the summary is a *"single line"* and
/// *"should not be markup"*, so it is sanitized and **not** escaped: escaping
/// it would render a literal `&amp;` to someone whose message said `&`.
pub fn summary(title: &str) -> String {
    sanitize_line(title, MAX_SUMMARY_CHARS)
}

/// The body handed to the server.
///
/// `markup` says whether the server advertised `body-markup`. When it did the
/// text is escaped; when it did not, escaping would show the reader literal
/// entities, so it is left alone. Sanitisation happens either way — that part
/// is not about the server's capabilities, it is about the characters.
///
/// Escaping happens **before** the length cap, so the cap measures what is
/// actually sent and a body full of ampersands cannot triple in size on the
/// way out.
pub fn body(raw: &str, markup: bool) -> String {
    let cleaned = sanitize_block(raw, usize::MAX);
    let rendered = if markup {
        escape_markup(&cleaned)
    } else {
        cleaned
    };
    truncate_chars(&rendered, MAX_BODY_CHARS)
}

/// A summary and a body, ready to hand to a server.
pub fn presented(title: &str, raw_body: &str, markup: bool) -> (String, String) {
    (summary(title), body(raw_body, markup))
}

/// The presentation cap on a summary, re-exported so tests can name it.
pub const SUMMARY_CHARS: usize = MAX_SUMMARY_CHARS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped_not_interpreted() {
        assert_eq!(
            body("<b>bold</b> & <i>more</i>", true),
            "&lt;b&gt;bold&lt;/b&gt; &amp; &lt;i&gt;more&lt;/i&gt;"
        );
    }

    #[test]
    fn escaping_does_not_double_escape_its_own_ampersands() {
        assert_eq!(escape_markup("<"), "&lt;");
        assert_eq!(escape_markup("&lt;"), "&amp;lt;");
    }

    #[test]
    fn a_server_without_body_markup_is_not_given_entities() {
        // Showing a reader `&amp;` because their friend typed `&` would be a
        // bug caused entirely by defending against a parser that is not there.
        assert_eq!(body("tea & biscuits", false), "tea & biscuits");
    }

    #[test]
    fn control_characters_go_but_a_body_keeps_its_newlines() {
        assert_eq!(body("one\ntwo\tthree\u{7}", false), "one\ntwo\tthree");
        assert_eq!(summary("one\ntwo"), "onetwo");
        assert_eq!(summary("bell\u{7}"), "bell");
    }

    #[test]
    fn every_bidi_control_goes_not_only_the_famous_one() {
        for c in [
            '\u{202a}',
            '\u{202b}',
            '\u{202c}',
            '\u{202d}',
            '\u{202e}',
            '\u{2066}',
            '\u{2067}',
            '\u{2068}',
            '\u{2069}',
            '\u{200e}',
            '\u{200f}',
            '\u{061c}',
            '\u{feff}',
            '\u{e0001}',
        ] {
            let hostile = format!("invoice{c}cod.exe");
            let out = body(&hostile, true);
            assert!(!out.contains(c), "{c:?} survived sanitisation");
            assert_eq!(out, "invoicecod.exe");
        }
    }

    #[test]
    fn a_long_field_is_truncated_on_a_character_boundary_and_says_so() {
        // Multi-byte throughout: a byte-wise cut would produce invalid UTF-8.
        let long = "é".repeat(MAX_BODY_CHARS + 50);
        let out = body(&long, false);
        assert_eq!(out.chars().count(), MAX_BODY_CHARS);
        assert!(out.ends_with('…'));

        let long_title = "字".repeat(500);
        let out = summary(&long_title);
        assert_eq!(out.chars().count(), MAX_SUMMARY_CHARS);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn escaping_happens_before_the_cap_so_the_cap_measures_what_is_sent() {
        let ampersands = "&".repeat(MAX_BODY_CHARS);
        let out = body(&ampersands, true);
        assert_eq!(out.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn an_app_name_falls_back_to_the_package_and_never_to_a_placeholder() {
        assert_eq!(app_name("Signal", "org.thoughtcrime.securesms"), "Signal");
        assert_eq!(
            app_name("   ", "org.thoughtcrime.securesms"),
            "org.thoughtcrime.securesms"
        );
        assert_eq!(app_name("", ""), "");
    }

    #[test]
    fn an_app_name_is_sanitized_like_everything_else() {
        assert_eq!(app_name("Sig\u{202e}nal", "x"), "Signal");
        assert_eq!(app_name("Sig\nnal", "x"), "Signal");
    }

    #[test]
    fn presented_returns_both_halves() {
        let (s, b) = presented("Title <hi>", "Body <hi>", true);
        assert_eq!(s, "Title <hi>");
        assert_eq!(b, "Body &lt;hi&gt;");
    }
}
