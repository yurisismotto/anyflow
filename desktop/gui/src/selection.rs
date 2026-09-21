//! Which device this desktop is talking to, and where that choice is kept.
//!
//! # Why this exists at all
//!
//! The desktop had no concept of a chosen device. Every control request names
//! one — `Request::Send { device, .. }`, `Request::ClipboardSend { device, .. }`
//! — and the CLI gets that name from the person typing it. The GUI had no
//! person typing, so `views/dashboard.rs` picked one:
//!
//! ```text
//! connected.iter().find(|d| d.granted_capabilities.contains("files.v1"))
//! ```
//!
//! which is *list position* with a filter in front of it. That is the same
//! defect U2 P1 fixed on Android, where `TrustStore.peers().firstOrNull()`
//! decided which computer a file went to, and where the report recorded that
//! the order is not even stable: connecting to a peer moves it to the end of
//! the list. The Quick Panel makes the question unavoidable, because a panel
//! with two device rows and one Send button has to say which row the button
//! means.
//!
//! # The model, and why it mirrors Android's
//!
//! [`crate::panel::model::Target`] is the desktop half of
//! `store/PeerTarget.kt`: trust is the *set* of peers, the selection is the
//! person's *choice*, and a destination is a function of the two. Nothing
//! downstream can see list position. Keeping the two platforms' rules the
//! same is itself a safety property — a person who learns "OmniBridge asks when
//! it is ambiguous" on the phone should not find the desktop guessing.
//!
//! # Where it is stored, and why not in the daemon
//!
//! `$XDG_CONFIG_HOME/omnibridge/gui.json`, as an application preference.
//!
//! The daemon would be the better long-term owner: the CLI, the GUI and a
//! future tray would then agree without any of them writing a file. That
//! needs a new control request, and this sprint adds no protocol — so the
//! choice lives with the application that has the windows, is shared by both
//! of them, and is recorded as a debt rather than smuggled into the socket.
//!
//! Deliberately **not** `$XDG_DATA_HOME/omnibridge`: that is the trust store,
//! whose 0700/0600 modes are verified on load, and a GUI preference has no
//! business inside a directory with that contract.
//!
//! # What is stored
//!
//! One public fingerprint hex. No name, no address, no content. A file that
//! cannot be read, cannot be parsed, or names something that is not hex is
//! treated as "nobody has chosen" — the ambiguous state, which asks, rather
//! than a state that sends.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// The on-disk shape. One additive key, so a later addition is a new field
/// and not a migration.
const SCHEMA: u32 = 1;

/// A fingerprint is 32 bytes of SHA-256 rendered as hex. Bounded here so a
/// hostile or corrupt file cannot make this hold an arbitrary string that
/// later gets handed to the daemon as a device selector.
const MAX_HEX: usize = 128;

/// The person's choice of device, persisted.
///
/// Cheap to share: the whole thing is one optional string behind a `RefCell`,
/// and both windows hold the same `Rc`.
pub struct Selection {
    path: PathBuf,
    current: RefCell<Option<String>>,
}

impl Selection {
    /// Loads the choice from the default location.
    pub fn load() -> Self {
        Self::at(default_path())
    }

    /// Loads the choice from `path`. The seam the tests use.
    pub fn at(path: PathBuf) -> Self {
        let current = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| parse(&text));
        Selection {
            path,
            current: RefCell::new(current),
        }
    }

    /// The chosen fingerprint, lowercase hex, if one has been chosen.
    ///
    /// This is a *stored* value and says nothing about whether the device is
    /// still paired. Resolving it against the trust store is
    /// [`crate::panel::model::Target`]'s job, and a value that no longer
    /// names a peer is refused there rather than repaired here — see the
    /// note on stale choices in that module.
    pub fn current(&self) -> Option<String> {
        self.current.borrow().clone()
    }

    /// Records a deliberate choice.
    ///
    /// Rejects anything that is not a fingerprint hex. The value is used as a
    /// device selector on the control socket, so the one place it enters the
    /// program is the one place to check it.
    pub fn choose(&self, fingerprint: &str) {
        let Some(hex) = normalise(fingerprint) else {
            return;
        };
        if self.current.borrow().as_deref() == Some(hex.as_str()) {
            return;
        }
        *self.current.borrow_mut() = Some(hex);
        self.persist();
    }

    /// Forgets the choice **only** if it names `fingerprint`.
    ///
    /// What "the device I had chosen just left the list" does. The whole rule
    /// is the comparison: an unrelated choice is left alone, and nothing is
    /// chosen in its place. Choosing a replacement — the next peer, the only
    /// peer, the one with the same name — is precisely the guess
    /// [`crate::panel::model::Target`] exists to refuse, and a removal is not
    /// a special case that earns one.
    ///
    /// Returns whether the choice was dropped, so a caller can say so.
    pub fn forget_if(&self, fingerprint: &str) -> bool {
        let Some(hex) = normalise(fingerprint) else {
            // Not a fingerprint, so it cannot be what is stored — the stored
            // value is normalised on the way in. Refusing to act on it is the
            // same fail-safe `choose` applies.
            return false;
        };
        if self.current.borrow().as_deref() != Some(hex.as_str()) {
            return false;
        }
        self.clear();
        true
    }

    /// Forgets the choice, returning the panel to asking.
    pub fn clear(&self) {
        if self.current.borrow().is_none() {
            return;
        }
        *self.current.borrow_mut() = None;
        self.persist();
    }

    /// Writes the file, best effort.
    ///
    /// A preference that cannot be written is not worth an error dialog: the
    /// choice still holds for this run, and the next run asks. Writing the
    /// parent directory 0700 matches every other OmniBridge directory.
    fn persist(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        let mut doc = serde_json::Map::new();
        doc.insert("schema".into(), SCHEMA.into());
        if let Some(hex) = self.current.borrow().as_deref() {
            doc.insert("selected_peer".into(), hex.into());
        }
        if let Ok(body) = serde_json::to_string_pretty(&doc) {
            let _ = std::fs::write(&self.path, body + "\n");
        }
    }

    /// Where the choice is kept. For diagnostics and tests.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// `$XDG_CONFIG_HOME/omnibridge/gui.json`, else `~/.config/omnibridge/gui.json`.
fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("omnibridge").join("gui.json")
}

/// Lowercases and checks that the value is fingerprint hex.
fn normalise(fingerprint: &str) -> Option<String> {
    let hex = fingerprint.trim().to_ascii_lowercase();
    if hex.is_empty() || hex.len() > MAX_HEX || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex)
}

/// Pulls `selected_peer` out of the stored JSON.
///
/// Every failure lands in the same place: unreadable, unparseable, missing,
/// the wrong JSON type, or not hex — all of them mean "nobody has chosen",
/// which is the state that *asks* rather than the state that sends.
fn parse(text: &str) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    normalise(doc.get("selected_peer")?.as_str()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "omnibridge-selection-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        p.push("gui.json");
        p
    }

    #[test]
    fn nothing_is_chosen_before_anyone_chooses() {
        let selection = Selection::at(temp("fresh"));
        assert_eq!(selection.current(), None);
    }

    #[test]
    fn a_choice_survives_the_application_restarting() {
        let path = temp("round-trip");
        let first = Selection::at(path.clone());
        first.choose("AB12CD34");
        // Stored lowercase, so two spellings of one fingerprint are one
        // choice rather than two.
        assert_eq!(first.current().as_deref(), Some("ab12cd34"));

        let second = Selection::at(path.clone());
        assert_eq!(second.current().as_deref(), Some("ab12cd34"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clearing_the_choice_persists_too() {
        let path = temp("cleared");
        let first = Selection::at(path.clone());
        first.choose("ab12cd34");
        first.clear();
        assert_eq!(Selection::at(path.clone()).current(), None);
        let _ = std::fs::remove_file(&path);
    }

    /// The stored value becomes a device selector on the control socket, so
    /// a file that says something else must not be believed.
    #[test]
    fn a_value_that_is_not_fingerprint_hex_is_refused() {
        for bad in [
            "",
            "   ",
            "not-hex",
            "ab12cd34 or-anything-else",
            "../../etc/passwd",
        ] {
            assert_eq!(normalise(bad), None, "{bad:?} should not be accepted");
        }
        assert_eq!(normalise(&"a".repeat(MAX_HEX + 1)), None);
    }

    #[test]
    fn a_corrupt_or_foreign_file_reads_as_no_choice() {
        for text in [
            "",
            "not json at all",
            "{}",
            "{\"schema\":1}",
            "{\"selected_peer\": 7}",
            "{\"selected_peer\": \"zzzz\"}",
        ] {
            assert_eq!(parse(text), None, "{text:?} should not yield a choice");
        }
    }

    // -----------------------------------------------------------------------
    // D7 / D8 — what a removal does to the chosen device, and what it does not
    // -----------------------------------------------------------------------

    /// D7: removing the chosen device clears the choice.
    #[test]
    fn removing_the_chosen_device_clears_the_choice() {
        let path = temp("forget-self");
        let selection = Selection::at(path.clone());
        selection.choose("ab12cd34");

        assert!(selection.forget_if("AB12CD34"), "spelling is not identity");
        assert_eq!(selection.current(), None);
        // And it stayed cleared: a choice that came back on restart would
        // re-aim the Quick Panel at a device that is no longer listed.
        assert_eq!(Selection::at(path.clone()).current(), None);
        let _ = std::fs::remove_file(&path);
    }

    /// D8: removing some *other* device changes nothing.
    ///
    /// The half that matters more, because the failure is silent: a cleanup
    /// that dropped the choice would send the next file to whatever the panel
    /// resolved to instead.
    #[test]
    fn removing_another_device_leaves_the_choice_alone() {
        let path = temp("forget-other");
        let selection = Selection::at(path.clone());
        selection.choose("ab12cd34");

        assert!(!selection.forget_if("ffffffff"));
        assert_eq!(selection.current().as_deref(), Some("ab12cd34"));
        let _ = std::fs::remove_file(&path);
    }

    /// Nothing here can *make* a choice — there is no path from a removal to
    /// a selection, which is what "does not auto-select another peer" means at
    /// this layer. With the choice gone, `Target::resolve` is the only thing
    /// that decides, and with several peers it asks.
    #[test]
    fn forgetting_never_selects_anything() {
        let path = temp("forget-nothing");
        let selection = Selection::at(path.clone());
        assert!(!selection.forget_if("ab12cd34"));
        assert_eq!(selection.current(), None);

        selection.choose("ab12cd34");
        selection.forget_if("ab12cd34");
        assert_eq!(
            selection.current(),
            None,
            "cleared means cleared, not re-pointed"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_value_that_is_not_a_fingerprint_never_clears_a_choice() {
        let path = temp("forget-garbage");
        let selection = Selection::at(path.clone());
        selection.choose("ab12cd34");
        for bad in ["", "   ", "not-hex", "../../etc/passwd"] {
            assert!(!selection.forget_if(bad), "{bad:?}");
        }
        assert_eq!(selection.current().as_deref(), Some("ab12cd34"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_well_formed_file_yields_the_fingerprint() {
        assert_eq!(
            parse("{\"schema\": 1, \"selected_peer\": \"A1B2C3\"}").as_deref(),
            Some("a1b2c3")
        );
    }

    /// No name, no address, no content — one public fingerprint and a schema
    /// number. This pins that, because the file is the only thing this
    /// feature writes to disk.
    #[test]
    fn the_stored_file_holds_nothing_but_the_fingerprint() {
        let path = temp("contents");
        let selection = Selection::at(path.clone());
        selection.choose("ab12cd34");
        let text = std::fs::read_to_string(&path).expect("written");
        assert!(text.contains("\"selected_peer\": \"ab12cd34\""));
        assert!(text.contains("\"schema\": 1"));
        assert_eq!(
            text.matches('"').count(),
            6,
            "two keys and one value, nothing else: {text}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
