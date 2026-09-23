//! Finding F-2 — the product must not claim a capability it does not have.
//!
//! # What F-2 was
//!
//! On Debian 13 trixie, GNOME 48 Wayland, with a `wl-copy` provably owning the
//! selection and `loginctl show-session … LockedHint` reading `no` **before and
//! after** the call:
//!
//! * `omnibridge clipboard status` said *"Clipboard auto-send cannot run;
//!   **manual send still works**"* — and `omnibridge clipboard send` failed;
//! * the failure said *"this **normally means the session is locked**"* — and
//!   the session was not locked, and Xwayland was running.
//!
//! Two false statements: one about what the product can do, one about why it
//! could not. Neither loses data and neither is a security issue — the
//! operation fails closed — but a user is sent looking for a fault in their own
//! machine, and an error that misattributes its cause teaches people to ignore
//! it.
//!
//! Recorded in RELEASE-PEER-GATES-CLOSURE-V1.md §4 with the lock state read on
//! both sides of the failing send.
//!
//! # Why these are characterisation tests over message text
//!
//! The defect was never in the control flow. Both paths read a selection this
//! process does not own — the watcher continuously, `clipboard send` once — so
//! when the watcher cannot, the send cannot either. The code was right about
//! what it could do and wrong about what it *said*, and the only place that can
//! regress is the wording. So the wording is what is pinned.

use omnibridge_capability_clipboard::backend::BackendError;

/// The timeout message must not assert a locked session as *the* cause.
///
/// It may name it as *a* cause — on GNOME it is the common one — but the same
/// timeout occurs, identically, on a wide-awake session where no client may
/// read a selection it does not own.
#[test]
fn f2_the_timeout_does_not_blame_a_lock_screen_it_has_not_observed() {
    let msg = BackendError::TimedOut.to_string();

    assert!(
        !msg.contains("normally means the session is locked"),
        "the timeout message asserts a locked session as the usual cause, which \
         F-2 measured to be false on an unlocked session: {msg}"
    );
    assert!(
        msg.contains("one cause"),
        "the timeout message should offer the lock as ONE cause among others, \
         so a user on an unlocked session is not sent to the wrong place: {msg}"
    );
    // The actionable half: whatever the cause, there is a command that says
    // which one applies. The original message offered no next step at all.
    assert!(
        msg.contains("clipboard status"),
        "the timeout message should point at the command that distinguishes the \
         causes: {msg}"
    );
}

/// The timeout message must still explain the mechanism.
///
/// Removing the false cause must not turn a message that was wrong into one
/// that is merely empty — the seat is the reason these tools wait rather than
/// fail, and that is the part a person needs to understand what they are
/// looking at.
#[test]
fn f2_the_timeout_still_explains_why_the_tools_wait() {
    let msg = BackendError::TimedOut.to_string();
    assert!(
        msg.contains("seat"),
        "the timeout message no longer explains that the tools are waiting for a \
         seat: {msg}"
    );
    assert!(
        msg.contains("locked session"),
        "a locked session is the common cause on GNOME and must still be named: \
         {msg}"
    );
}

/// No shipped message may promise that sending by hand works when automatic
/// sending does not.
///
/// Four separate places said it: the Wayland backend's watch-source detection,
/// the daemon's startup log, the clipboard manager's watcher warning and the
/// GUI's clipboard page. Fixing one and leaving three is how F-2 would come
/// back, so the whole tree is asserted rather than the four known sites.
///
/// Comments are excluded: this file, and the fixes themselves, quote the old
/// wording to explain what was wrong with it.
#[test]
fn f2_no_shipped_message_claims_manual_send_works_when_the_watch_does_not() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let mut offenders = Vec::new();
    let mut scanned = 0usize;

    visit(&root, &mut |path: &std::path::Path| {
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            return;
        }
        // Tests may quote the old text; shipped code may not.
        if path.components().any(|c| c.as_os_str() == "tests") {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        scanned += 1;
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("///") || code.starts_with("//!") {
                continue;
            }
            if line.contains("still works") {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    });

    // Non-vacuity: a walk that found no files would pass this test while
    // proving nothing, which is the failure mode this repository keeps meeting.
    assert!(
        scanned >= 20,
        "the source walk visited only {scanned} files; it is not covering the tree"
    );
    assert!(
        offenders.is_empty(),
        "shipped code claims something 'still works' where F-2 showed it does \
         not. Say what is true, or say nothing:\n  {}",
        offenders.join("\n  ")
    );
}

fn visit(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        // `target/` is build output and holds a copy of everything.
        if name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            visit(&path, f);
        } else {
            f(&path);
        }
    }
}
