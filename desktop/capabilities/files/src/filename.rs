//! Turning a peer-supplied filename into something safe to create.
//!
//! # The rule
//!
//! A filename from the network is a *hint about what to call the file*, never
//! an instruction about where to put it. This module reduces it to a bare
//! name and the caller joins that onto a directory it chose itself. There is
//! no code path anywhere in `files.v1` that lets a peer influence the
//! directory.
//!
//! # What is stripped, and why
//!
//! * **Everything up to the last separator.** `../../etc/passwd` becomes
//!   `passwd`. Both `/` and `\` count as separators, so a Windows-shaped path
//!   cannot smuggle a component past a Unix-only split.
//! * **Control characters, including NUL.** A NUL truncates a C string, so a
//!   name that survives Rust's checks could still mean something different to
//!   a syscall or to a program the file is later handed to. Control
//!   characters also forge terminal and notification output.
//! * **Trailing dots and spaces.** Harmless on Linux, silently trimmed by
//!   Windows and SMB — which means `evil.txt.` and `evil.txt` are the same
//!   file over a share, and a check that treated them as different would be
//!   wrong there.
//! * **`.` and `..` outright.** They name directories, not files.
//! * **Windows reserved device names.** `CON`, `NUL`, `LPT1`, `CONIN$`,
//!   `CONOUT$` and friends are not filenames on Windows even with an
//!   extension. Nothing here runs on Windows today, but a received file lands
//!   in a directory that is routinely shared over SMB or synced, and this
//!   costs one table lookup.
//! * **`:` and the rest of `< > : " | ? *`.** A colon is the alternate data
//!   stream separator: `invoice.pdf:payload.exe` written to an NTFS volume
//!   creates a *hidden stream* on a file that looks ordinary, and the stream
//!   survives most copies while being invisible to `dir` and to Explorer.
//!   The others are simply not legal in a Win32 filename. They are stripped
//!   rather than rejected, for the same reason control characters are: the
//!   name is being reduced to something safe, not invented.
//! * **Unicode format characters (category `Cf`).** These are invisible, and
//!   `U+202E RIGHT-TO-LEFT OVERRIDE` reverses the rendering of everything
//!   after it — so `invoice\u{202E}cod.exe` displays as `invoiceexe.doc` in
//!   every file manager, terminal and notification on **every** platform.
//!   This one is not a Windows concern that a Linux build can defer; it is a
//!   present-tense spoofing hole here, today, which is why it is stripped
//!   globally with no `#[cfg]` anywhere near it (PLAT-DEC-014).
//!
//! The result is capped at [`MAX_FILENAME_BYTES`] on a UTF-8 boundary, with
//! the extension preserved where possible so a truncated name still opens
//! with the right application.
//!
//! # One rule set, everywhere
//!
//! There is no per-platform arm in this file and there must not be one
//! (PLAT-DEC-014, SI-10). A `#[cfg]` here would put a security control behind
//! an arm that never compiles on CI, and the rule that matters most —
//! stripping bidi overrides — belongs to no single platform, so a
//! per-destination design would have fixed it nowhere.
//!
//! # What is deliberately *not* rejected
//!
//! A leading dot. `.bashrc` is a perfectly ordinary filename, and it is being
//! created inside a dedicated download directory where a dotfile has no
//! special meaning. Rejecting it would be theatre.

use crate::limits::MAX_FILENAME_BYTES;

/// Windows device names, which are not usable as filenames there even with an
/// extension (`CON.txt` is still `CON`).
const RESERVED_STEMS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    // The two console handles. Less famous than `CON` and just as reserved.
    "conin$", "conout$",
];

/// Characters that are not legal in a Win32 filename, plus `:`.
///
/// `:` is the one that matters beyond legality: it is the NTFS alternate data
/// stream separator, so `report.pdf:payload.exe` is not a file called
/// `report.pdf:payload.exe` — it is a hidden stream attached to `report.pdf`.
const WINDOWS_ILLEGAL: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// Whether a character is in Unicode general category `Cf` (format).
///
/// Written out as ranges rather than pulled from a crate: this is one table,
/// it is auditable in place, and adding a Unicode database to the crate that
/// decides what lands on the user's disk is a larger thing to trust than a
/// list. Ranges are Unicode 16.0; a character added to `Cf` later is a
/// missed strip, not an unsafe one, and the list is checked against
/// `unicode.org/Public/UNIDATA/UnicodeData.txt` when it is revised.
///
/// `U+202E` is the reason this exists. `U+E0001` and the tag block are the
/// reason the list does not stop at the BMP: tag characters are invisible
/// everywhere and can carry a whole second string inside a filename.
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

/// Reduces a peer-supplied name to a safe basename.
///
/// Returns `None` when nothing usable survives — an empty name, a pure path,
/// a dot directory, or a reserved device name. `None` means "reject this
/// offer", never "pick a name for them": inventing a name for a file whose
/// own name was hostile hides the attack from the user.
pub fn sanitize(raw: &str) -> Option<String> {
    // Take the last path component, treating both separators as separators.
    // Done before anything else so that nothing after this point can contain
    // a separator at all.
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();

    // Drop, in one pass:
    //
    //   * control characters (NUL included), which truncate C strings and
    //     forge terminal output;
    //   * any separator that somehow survived — belt and braces, the split
    //     above already removed those;
    //   * Unicode format characters, which are invisible and, in the case of
    //     the bidi overrides, actively reverse how the rest of the name is
    //     displayed;
    //   * the Win32-illegal set, of which `:` is the dangerous one.
    //
    // Stripping rather than rejecting, consistently with how this module has
    // always treated a hostile *character* as opposed to a hostile *name*: a
    // reduced name is still the peer's name, an invented one is not.
    let cleaned: String = base
        .chars()
        .filter(|c| {
            !c.is_control()
                && *c != '/'
                && *c != '\\'
                && !is_format_char(*c)
                && !WINDOWS_ILLEGAL.contains(c)
        })
        .collect();

    // Leading whitespace is cosmetic; trailing dots and spaces change what
    // the name means on other filesystems.
    let trimmed = cleaned.trim_start().trim_end_matches([' ', '.', '\t']);

    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }

    let stem = trimmed.split('.').next().unwrap_or_default();
    if RESERVED_STEMS.contains(&stem.to_ascii_lowercase().as_str()) {
        return None;
    }

    let capped = truncate_preserving_extension(trimmed);
    if capped.is_empty() {
        return None;
    }
    Some(capped)
}

/// Caps a name at [`MAX_FILENAME_BYTES`], keeping the extension when it fits.
///
/// A name is cut on a character boundary, so the result is always valid
/// UTF-8. Keeping the extension matters more than keeping the tail of the
/// stem: `holiday-photo-….jpg` still opens, `holiday-photo-…` does not.
fn truncate_preserving_extension(name: &str) -> String {
    if name.len() <= MAX_FILENAME_BYTES {
        return name.to_string();
    }

    // `rfind` on the *string*, so the extension is whatever follows the last
    // dot — the same thing every file manager shows.
    let extension = match name.rfind('.') {
        // A dot at position 0 is a leading dot, not an extension separator.
        Some(dot) if dot > 0 => &name[dot..],
        _ => "",
    };

    // An "extension" that is itself enormous is not an extension; drop it
    // rather than spending the whole budget on it.
    let extension = if extension.len() <= 16 { extension } else { "" };

    let budget = MAX_FILENAME_BYTES - extension.len();
    let stem = &name[..name.len() - extension.len()];
    let cut = floor_char_boundary(stem, budget);
    format!("{}{}", &stem[..cut], extension)
}

/// `str::floor_char_boundary` is still unstable, so walk back to one.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_name_is_kept_verbatim() {
        assert_eq!(sanitize("photo.jpg").as_deref(), Some("photo.jpg"));
        assert_eq!(
            sanitize("Report 2026.pdf").as_deref(),
            Some("Report 2026.pdf")
        );
    }

    #[test]
    fn a_unix_traversal_keeps_only_the_basename() {
        assert_eq!(sanitize("../../etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(
            sanitize("../../../../root/.ssh/authorized_keys").as_deref(),
            Some("authorized_keys")
        );
    }

    #[test]
    fn a_windows_traversal_keeps_only_the_basename() {
        assert_eq!(
            sanitize("..\\..\\windows\\system32\\cmd.exe").as_deref(),
            Some("cmd.exe")
        );
        // Mixed separators must not let a component through.
        assert_eq!(
            sanitize("../foo\\..\\bar/baz.txt").as_deref(),
            Some("baz.txt")
        );
    }

    #[test]
    fn an_absolute_path_keeps_only_the_basename() {
        assert_eq!(sanitize("/etc/shadow").as_deref(), Some("shadow"));
        assert_eq!(sanitize("/home/yuri/.bashrc").as_deref(), Some(".bashrc"));
        assert_eq!(sanitize("C:\\Windows\\win.ini").as_deref(), Some("win.ini"));
    }

    #[test]
    fn a_name_that_is_only_a_path_is_rejected() {
        assert_eq!(sanitize("/"), None);
        assert_eq!(sanitize("../../"), None);
        assert_eq!(sanitize("/etc/"), None);
        assert_eq!(sanitize(""), None);
    }

    #[test]
    fn dot_directories_are_rejected() {
        assert_eq!(sanitize("."), None);
        assert_eq!(sanitize(".."), None);
        assert_eq!(sanitize("...."), None);
        // A trailing-dot form that Windows would trim back to `..`.
        assert_eq!(sanitize("..."), None);
    }

    #[test]
    fn a_nul_byte_cannot_survive() {
        // The danger is a name that Rust sees as one thing and a syscall, or
        // a downstream C program, sees as another.
        assert_eq!(sanitize("safe.txt\u{0}.sh").as_deref(), Some("safe.txt.sh"));
        assert!(sanitize("\u{0}").is_none());
    }

    #[test]
    fn control_characters_are_stripped() {
        // A newline in a filename forges a second line of terminal output.
        assert_eq!(sanitize("a\nb\rc\td.txt").as_deref(), Some("abcd.txt"));
        // An ANSI escape would repaint the operator's terminal.
        assert_eq!(sanitize("x\u{1b}[2Ky.txt").as_deref(), Some("x[2Ky.txt"));
    }

    #[test]
    fn trailing_dots_and_spaces_go() {
        assert_eq!(sanitize("evil.txt.").as_deref(), Some("evil.txt"));
        assert_eq!(sanitize("evil.txt   ").as_deref(), Some("evil.txt"));
        assert_eq!(sanitize("evil.txt . . ").as_deref(), Some("evil.txt"));
    }

    #[test]
    fn windows_device_names_are_rejected() {
        assert_eq!(sanitize("CON"), None);
        assert_eq!(sanitize("con.txt"), None);
        assert_eq!(sanitize("NUL.jpg"), None);
        assert_eq!(sanitize("lpt9.tar.gz"), None);
        // Not reserved: only the exact stems are.
        assert_eq!(sanitize("console.log").as_deref(), Some("console.log"));
    }

    #[test]
    fn a_leading_dot_is_allowed() {
        // It lands in a dedicated directory where a dotfile means nothing.
        assert_eq!(sanitize(".gitignore").as_deref(), Some(".gitignore"));
    }

    #[test]
    fn a_long_name_is_capped_on_a_character_boundary_keeping_the_extension() {
        let long = format!("{}.jpg", "é".repeat(400));
        let out = sanitize(&long).expect("still a usable name");
        assert!(out.len() <= MAX_FILENAME_BYTES, "{} bytes", out.len());
        assert!(out.ends_with(".jpg"));
        // The cut must not have split a multi-byte character.
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        assert!(!out.contains('\u{fffd}'));
    }

    #[test]
    fn a_long_name_with_no_extension_is_still_capped() {
        let out = sanitize(&"a".repeat(1000)).expect("usable");
        assert_eq!(out.len(), MAX_FILENAME_BYTES);
    }

    #[test]
    fn an_absurd_extension_is_not_allowed_to_eat_the_whole_budget() {
        let long = format!("{}.{}", "a".repeat(300), "b".repeat(300));
        let out = sanitize(&long).expect("usable");
        assert_eq!(out.len(), MAX_FILENAME_BYTES);
    }

    // -----------------------------------------------------------------
    // SEC-004 / PLAT-DEC-014 — the Wave 0 additions
    // -----------------------------------------------------------------

    #[test]
    fn a_colon_cannot_survive_and_no_alternate_data_stream_can_be_named() {
        // `report.pdf:payload.exe` on NTFS is not a file with a funny name,
        // it is a hidden stream attached to `report.pdf`. The colon has to be
        // gone before the name reaches any filesystem.
        assert_eq!(sanitize("a:b").as_deref(), Some("ab"));
        assert_eq!(
            sanitize("report.pdf:payload.exe").as_deref(),
            Some("report.pdfpayload.exe")
        );
        // The `::$DATA` form, which names the default stream explicitly.
        assert_eq!(
            sanitize("notes.txt::$DATA").as_deref(),
            Some("notes.txt$DATA")
        );
        for raw in [
            "a:b",
            "report.pdf:payload.exe",
            "notes.txt::$DATA",
            "C:file",
        ] {
            if let Some(name) = sanitize(raw) {
                assert!(!name.contains(':'), "{raw} -> {name}");
            }
        }
    }

    #[test]
    fn the_windows_illegal_set_is_stripped() {
        assert_eq!(sanitize("x|y.txt").as_deref(), Some("xy.txt"));
        assert_eq!(sanitize("q?.txt").as_deref(), Some("q.txt"));
        assert_eq!(sanitize("a<b>c.txt").as_deref(), Some("abc.txt"));
        assert_eq!(sanitize("wild*card.txt").as_deref(), Some("wildcard.txt"));
        assert_eq!(sanitize("say\"hi\".txt").as_deref(), Some("sayhi.txt"));
        // A name made of nothing but illegal characters leaves nothing.
        assert_eq!(sanitize("<>:\"|?*"), None);
    }

    #[test]
    fn a_right_to_left_override_cannot_disguise_an_extension() {
        // Displayed as `invoiceexe.doc` in every file manager on every
        // platform. This is the present-tense Linux defect, not a future
        // Windows one.
        let hostile = "invoice\u{202e}cod.exe";
        let out = sanitize(hostile).expect("a usable name survives");
        assert_eq!(out, "invoicecod.exe");
        assert!(!out.contains('\u{202e}'));

        // The photo variant from the specification.
        assert_eq!(
            sanitize("photo\u{202e}gnp.exe").as_deref(),
            Some("photognp.exe")
        );
    }

    #[test]
    fn every_bidi_control_goes_not_only_the_famous_one() {
        for c in [
            '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}', '\u{200e}', '\u{200f}', '\u{061c}',
        ] {
            let raw = format!("a{c}b.txt");
            assert_eq!(
                sanitize(&raw).as_deref(),
                Some("ab.txt"),
                "U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn invisible_format_characters_go_even_when_they_are_not_bidi() {
        // Zero-width and joiner characters make two different names render
        // identically, which is enough to make a user open the wrong file.
        assert_eq!(sanitize("a\u{200b}b.txt").as_deref(), Some("ab.txt"));
        assert_eq!(sanitize("a\u{200d}b.txt").as_deref(), Some("ab.txt"));
        assert_eq!(
            sanitize("\u{feff}report.pdf").as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            sanitize("soft\u{00ad}hyphen.txt").as_deref(),
            Some("softhyphen.txt")
        );
        // Tag characters: invisible, outside the BMP, and able to carry a
        // whole second string.
        assert_eq!(
            sanitize("ok\u{e0001}\u{e0065}\u{e0078}\u{e0065}.txt").as_deref(),
            Some("ok.txt")
        );
        // A name that is nothing but invisibles is not a name.
        assert_eq!(sanitize("\u{200b}\u{202e}\u{feff}"), None);
    }

    #[test]
    fn the_console_handles_are_reserved_too() {
        assert_eq!(sanitize("CONIN$"), None);
        assert_eq!(sanitize("conout$.txt"), None);
        assert_eq!(sanitize("CONIN$.tar.gz"), None);
        // A colon-suffixed device name must not slip past by looking
        // different: the colon is stripped first, so the stem is checked.
        assert_eq!(sanitize("NUL:"), None);
        assert_eq!(sanitize("con:"), None);
        // Not reserved: only the exact stems are.
        assert_eq!(sanitize("continue$.txt").as_deref(), Some("continue$.txt"));
    }

    #[test]
    fn ordinary_unicode_is_untouched() {
        // The strip list must not become "anything non-ASCII".
        for name in [
            "relatório.pdf",
            "Транзакция.txt",
            "写真.jpg",
            "café ☕.md",
            "naïve-résumé.docx",
        ] {
            assert_eq!(sanitize(name).as_deref(), Some(name), "{name}");
        }
    }

    #[test]
    fn the_result_is_always_valid_utf8_and_carries_nothing_invisible() {
        // A property check over the whole hostile corpus, so a future change
        // to the filter cannot re-open one of these by accident.
        for raw in [
            "../../etc/passwd",
            "..\\..\\windows\\system32\\cmd.exe",
            "a:b",
            "photo\u{202e}gnp.exe",
            "x|y.txt",
            "q?.txt",
            "notes.txt::$DATA",
            "\u{feff}\u{200b}weird.bin",
            "trailing.   ",
            "safe.txt\u{0}.sh",
        ] {
            let Some(name) = sanitize(raw) else { continue };
            assert!(std::str::from_utf8(name.as_bytes()).is_ok(), "{raw}");
            assert!(!name.contains('/'), "{raw} -> {name}");
            assert!(!name.contains('\\'), "{raw} -> {name}");
            assert!(!name.chars().any(is_format_char), "{raw} -> {name}");
            assert!(
                !name.chars().any(|c| WINDOWS_ILLEGAL.contains(&c)),
                "{raw} -> {name}"
            );
            assert!(!name.chars().any(char::is_control), "{raw} -> {name}");
            assert!(
                !name.ends_with('.') && !name.ends_with(' '),
                "{raw} -> {name}"
            );
            assert!(name.len() <= MAX_FILENAME_BYTES, "{raw} -> {name}");
        }
    }

    #[test]
    fn the_result_never_contains_a_separator() {
        for raw in [
            "../../etc/passwd",
            "..\\..\\x",
            "/a/b/c",
            "a/b\\c/d.txt",
            "\u{0}/../x",
        ] {
            if let Some(name) = sanitize(raw) {
                assert!(!name.contains('/'), "{raw} -> {name}");
                assert!(!name.contains('\\'), "{raw} -> {name}");
                assert_ne!(name, "..");
                assert_ne!(name, ".");
            }
        }
    }
}
