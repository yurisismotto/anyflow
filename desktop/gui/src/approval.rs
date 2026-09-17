//! The incoming-file approval prompt.
//!
//! The product surface U2 found missing. A phone offers a file, the daemon
//! asks whoever is attached to its control socket, and this puts the question
//! in front of the person sitting at the machine:
//!
//! ```text
//!   ┌─────────────────────────────────────────┐
//!   │  Incoming file                          │
//!   │  Galaxy S25 wants to send you a file.   │
//!   │                                         │
//!   │  holiday photo.jpg                      │
//!   │  293.0 KB · image/jpeg                  │
//!   │  Verified device · A1B2 C3D4 E5F6 0718  │
//!   │                                         │
//!   │            [ Decline ]  [ Accept ]      │
//!   └─────────────────────────────────────────┘
//! ```
//!
//! # What this is not
//!
//! It is **consent, not a data path**. The only thing that leaves this file
//! is a boolean and the transfer id it belongs to. The bytes travel on the
//! daemon's own TLS data stream, are hashed there, and are written by the
//! existing `FileSink` — no part of a file ever reaches this process.
//!
//! It **grants nothing**. Accepting approves one offer. There is no "always
//! allow", nothing is written to disk, and the next offer from the same
//! device asks again.
//!
//! # Every way out is a decline
//!
//! `Decline` is the default response *and* the close response, so Escape, the
//! window-manager close and Enter all decline. There is no path through this
//! module that sends `accept = true` without a press of the Accept button —
//! and even if there were, the daemon would still have to have a prompt open
//! for that exact transfer id to act on it.
//!
//! Losing the daemon takes every prompt down (`Detached`), because a question
//! nobody can act on must not sit in front of a person with an Accept button
//! under it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use anyflow_control::FileOfferRequest;

use crate::client::{self, ApprovalUpdate};
use crate::widgets::{self, SPACING_SM, SPACING_XS};

/// Response ids. Strings because that is `AdwAlertDialog`'s vocabulary; the
/// safe one is named in three places below and is deliberately a constant.
const DECLINE: &str = "decline";
const ACCEPT: &str = "accept";

/// Attaches this application to the daemon as its incoming-file approval
/// provider.
///
/// The returned handle must be kept alive for as long as the application:
/// dropping it detaches, and a detached daemon goes back to declining every
/// file.
///
/// `parent` is asked, each time an offer arrives, which window the question
/// should appear over. It is a closure rather than a window because the
/// attachment outlives any one of them: AnyFlow has a Quick Panel and a
/// Settings window, either may be closed, and closing one must not stop the
/// machine being able to accept a file. When it answers `None` there is no
/// window to ask over and the offer is left unanswered — which the daemon
/// resolves as a decline, the safe direction.
pub fn install<P>(parent: P) -> Rc<client::ApprovalHandle>
where
    P: Fn() -> Option<gtk::Window> + 'static,
{
    // One dialog per pending offer, keyed by the full transfer id. Keyed
    // rather than counted because an answer, a withdrawal and a close all
    // name one specific offer, and "the dialog on top" is not that.
    let open: Rc<RefCell<HashMap<String, adw::AlertDialog>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // The update callback needs the very handle that the call creating it
    // returns, so the slot is filled afterwards. **`Weak`, not `Rc`**: the
    // callback is owned by the stream the handle shuts down, so a strong
    // reference here would be a cycle — the handle would never drop, the
    // stream would never close, and the window would stay attached after it
    // was gone.
    let slot: Rc<RefCell<Weak<client::ApprovalHandle>>> = Rc::new(RefCell::new(Weak::new()));

    let created = {
        let open = open.clone();
        let handle = slot.clone();
        Rc::new(client::watch_file_offers(move |update| match update {
            ApprovalUpdate::Attached { unattended } => {
                if unattended {
                    // Not a prompt, and not something this window can change.
                    // Said out loud because the alternative is a person
                    // waiting for a dialog that will never appear.
                    eprintln!(
                        "anyflow-gui: the daemon is running with \
                         --accept-files-without-asking, so incoming files are \
                         accepted without a prompt."
                    );
                }
            }
            ApprovalUpdate::Offer(request) => {
                let handle = handle.borrow().clone();
                if let Some(window) = parent() {
                    present(&window, &open, &handle, &request);
                }
            }
            ApprovalUpdate::Withdrawn(transfer_id) => {
                // The offer expired, the peer disconnected, or the pairing
                // was revoked. Take the question down.
                let dialog = open.borrow_mut().remove(&transfer_id);
                if let Some(dialog) = dialog {
                    dialog.close();
                }
            }
            ApprovalUpdate::Detached => {
                // Nothing on screen can be acted on any more.
                let dialogs: Vec<_> = open.borrow_mut().drain().map(|(_, d)| d).collect();
                for dialog in dialogs {
                    dialog.close();
                }
            }
        }))
    };
    *slot.borrow_mut() = Rc::downgrade(&created);
    created
}

/// Puts one offer in front of the user.
///
/// The handle is held **weakly** by the response closure, for the reason the
/// update callback holds it weakly: a dialog lives inside the map the stream
/// owns, so a strong reference from a button press would keep the stream that
/// delivered it alive for ever. An upgrade that fails means the window has
/// gone, and a decision with nowhere to go is a decline by default.
fn present(
    window: &gtk::Window,
    open: &Rc<RefCell<HashMap<String, adw::AlertDialog>>>,
    handle: &Weak<client::ApprovalHandle>,
    request: &FileOfferRequest,
) {
    // A second prompt for the same transfer would be the daemon repeating
    // itself; one question, one dialog.
    if open.borrow().contains_key(&request.transfer_id) {
        return;
    }

    let dialog = adw::AlertDialog::new(
        Some("Incoming file"),
        Some(&format!(
            "{} wants to send you a file.",
            request.device_name
        )),
    );

    dialog.set_extra_child(Some(&detail(request)));

    dialog.add_responses(&[(DECLINE, "Decline"), (ACCEPT, "Accept")]);
    // Both of these name Decline, and both matter. The default is what Enter
    // activates; the close response is what Escape and the window manager
    // send. Accepting a file must take a deliberate press of one button.
    dialog.set_default_response(Some(DECLINE));
    dialog.set_close_response(DECLINE);
    // Neither response is given a colour appearance. The Adwaita convention
    // would tint Accept as "suggested"; a consent gate should not nudge, and
    // the two buttons must not differ by colour alone in any case.

    {
        let open = open.clone();
        let handle = handle.clone();
        let transfer_id = request.transfer_id.clone();
        dialog.connect_response(None, move |_, response| {
            // Removed first, so the answer path and the withdrawal path
            // cannot both act on one dialog.
            if open.borrow_mut().remove(&transfer_id).is_none() {
                // Already withdrawn or already answered. The daemon ignores a
                // decision for a prompt it does not have open, so this is
                // belt and braces rather than the only guard — but it keeps
                // a closed-by-withdrawal dialog from sending anything.
                return;
            }
            if let Some(handle) = handle.upgrade() {
                handle.decide(&transfer_id, response == ACCEPT);
            }
        });
    }

    open.borrow_mut()
        .insert(request.transfer_id.clone(), dialog.clone());
    dialog.present(Some(window));
}

/// The facts the decision rests on.
///
/// Filename and size, then the device's authenticated identity. The
/// fingerprint is here because a name is not an identity: two devices can be
/// called the same thing, and the name shown is the one *this* machine stored
/// when it paired, never one the peer asserted in the offer.
fn detail(request: &FileOfferRequest) -> gtk::Box {
    let column = widgets::column(SPACING_XS);
    column.set_margin_top(SPACING_SM);

    let filename = widgets::heading(&request.filename);
    filename.set_selectable(true);
    filename.set_wrap(true);
    filename.set_xalign(0.0);
    column.append(&filename);

    let size = human_bytes(request.size_bytes);
    let mime = request.mime_type.trim();
    let line = if mime.is_empty() {
        size.clone()
    } else {
        format!("{size} · {mime}")
    };
    let meta = widgets::body_muted(&line);
    meta.set_xalign(0.0);
    column.append(&meta);

    let identity = widgets::caption(&format!("Verified device · {}", request.fingerprint_short));
    identity.set_xalign(0.0);
    column.append(&identity);

    // One sentence for assistive technology, so the essentials are announced
    // together rather than as three loose labels next to the dialog body.
    column.update_property(&[gtk::accessible::Property::Label(&format!(
        "{}, {}, from the verified device {}",
        request.filename, size, request.fingerprint_short
    ))]);

    column
}

/// Bytes in a form a person can weigh a decision against.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size shown must be the size offered, in a form that is read at a
    /// glance. A decision about a 4 GB file is not the same decision as one
    /// about a 4 KB file, and "4194304000" does not tell them apart quickly.
    #[test]
    fn sizes_are_rendered_for_a_human_to_weigh() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(300_000), "293.0 KB");
        assert_eq!(human_bytes(4 * 1024 * 1024 * 1024), "4.0 GB");
    }

    /// The safe response is the one Enter and Escape both reach. This pins
    /// the *names*; the wiring that uses them is three lines above and reads
    /// them from here.
    #[test]
    fn the_safe_response_is_the_default_and_the_close_response() {
        assert_eq!(DECLINE, "decline");
        assert_ne!(DECLINE, ACCEPT);
    }
}
