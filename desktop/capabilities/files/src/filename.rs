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
//! * **Windows reserved device names.** `CON`, `NUL`, `LPT1` and friends are
//!   not filenames on Windows even with an extension. Nothing here runs on
//!   Windows today, but a received file lands in a directory that is
//!   routinely shared over SMB or synced, and this costs one table lookup.
//!
//! The result is capped at [`MAX_FILENAME_BYTES`] on a UTF-8 boundary, with
//! the extension preserved where possible so a truncated name still opens
//! with the right application.
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
];

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

    // Drop control characters (NUL included) and any separator that somehow
    // survived. Belt and braces: the split above already removed those, and
    // this makes the invariant local rather than inherited.
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
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
