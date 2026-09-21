//! The trust store: who is trusted, with what, and how to stop.

use adw::prelude::*;
use omnibridge_control::{Request, Response};

use super::Pages;
use crate::widgets::{self, SPACING_SM};
use crate::{client, DaemonState};

/// The capabilities a device can be granted, and how to describe each one.
///
/// `files.v1` writes files to this machine and `clipboard.v1` moves text
/// between machines, so neither is ever granted automatically — that is
/// ADR-0008's rule and this screen is where a human applies it.
const CAPABILITIES: [(&str, &str, &str, &str); 3] = [
    (
        "clipboard.v1",
        "Clipboard",
        "Send and receive clipboard text",
        "edit-paste-symbolic",
    ),
    (
        "files.v1",
        "Files",
        "Offer and receive files",
        "folder-symbolic",
    ),
    (
        "battery.v1",
        "Battery",
        "Share battery level",
        "battery-symbolic",
    ),
];

pub fn render(container: &gtk::Box, state: &DaemonState, pages: &Pages) {
    widgets::clear(container);
    container.append(&widgets::title("Trusted peers"));

    let devices = state
        .devices
        .as_deref()
        .or(state.status.as_ref().map(|s| s.devices.as_slice()))
        .unwrap_or(&[]);

    if devices.is_empty() {
        container.append(&widgets::empty_state(
            "Nothing is trusted yet",
            "Pairing a device puts it here, with no capability granted until you say so.",
        ));
        return;
    }

    // Offered only when there is something for it to act on, and the count
    // is the count of *visible revoked* rows — the exact set the action
    // touches. With none, the button is not drawn at all rather than drawn
    // grey: there is nothing on this screen it could be read as referring to.
    let revoked_visible: Vec<&omnibridge_control::DeviceReport> =
        devices.iter().filter(|d| d.revoked).collect();
    if revoked_visible.len() > 1 {
        let card = widgets::card();
        card.append(&widgets::section_label("Revoked devices"));
        card.append(&widgets::caption(&format!(
            "{} revoked device(s) are still listed here. Removing them from the \
             list does not un-revoke them.",
            revoked_visible.len()
        )));
        let bulk = widgets::destructive_button("Remove all revoked devices");
        bulk.set_halign(gtk::Align::Start);
        bulk.update_property(&[gtk::accessible::Property::Description(&format!(
            "Removes {} revoked device(s) from the list. They stay revoked. \
             Devices you still trust are not affected.",
            revoked_visible.len()
        ))]);
        let fingerprints: Vec<String> = revoked_visible
            .iter()
            .map(|d| d.fingerprint.clone())
            .collect();
        let pages = pages.clone();
        bulk.connect_clicked(move |button| {
            confirm_remove_all(button, fingerprints.clone(), pages.clone());
        });
        card.append(&bulk);
        container.append(&card);
    }

    for device in devices {
        let card = widgets::card();

        let head = widgets::row(SPACING_SM);
        let text = widgets::column(2);
        text.append(&widgets::subtitle(&device.device_name));
        text.set_hexpand(true);
        head.append(&text);
        if device.revoked {
            head.append(&widgets::status_badge(widgets::Status::Revoked));
            // The badge is a word already (see `widgets::status_badge`) and
            // the card carries the sentence below it, so the state is never
            // colour alone. This adds the pair as one description, so the
            // device and its state can also be announced together rather than
            // only as two separate objects a reader walks past in turn.
            //
            // The role has to be set for the description to be exposed at
            // all: a plain `gtk::Box` is `generic`, which AT-SPI does not
            // surface, so the property was measured being dropped on the
            // floor before this line existed.
            card.set_accessible_role(gtk::AccessibleRole::Group);
            card.update_property(&[gtk::accessible::Property::Description(&format!(
                "{}, revoked. This device can no longer connect.",
                device.device_name
            ))]);
        }
        card.append(&head);

        // Never abbreviated for balance: this is the string compared against
        // the other device's screen, and it is the whole reason pairing is
        // safe.
        card.append(&widgets::section_label("Device fingerprint"));
        card.append(&widgets::fingerprint(&device.fingerprint));
        card.append(&widgets::caption(&format!(
            "Device id {}",
            device.device_id
        )));

        if !device.revoked {
            card.append(&widgets::separator());
            card.append(&widgets::section_label("Capabilities"));
            for (id, title, description, icon) in CAPABILITIES {
                card.append(&grant_row(
                    &device.device_id,
                    id,
                    title,
                    description,
                    icon,
                    device.granted_capabilities.iter().any(|c| c == id),
                    pages,
                ));
            }

            card.append(&widgets::separator());
            let revoke = widgets::destructive_button("Revoke this device");
            revoke.set_halign(gtk::Align::Start);
            let id = device.device_id.clone();
            let name = device.device_name.clone();
            let pages = pages.clone();
            revoke.connect_clicked(move |button| {
                confirm_revoke(button, id.clone(), name.clone(), pages.clone());
            });
            card.append(&revoke);
        } else {
            card.append(&widgets::caption(
                "Revoked. This device cannot connect until it pairs again.",
            ));
            card.append(&widgets::separator());
            // The destructive action carries a text label, not an icon: the
            // difference between taking a row off a list and taking away a
            // revocation is not something a glyph can carry.
            let remove = widgets::destructive_button("Remove from list");
            remove.set_halign(gtk::Align::Start);
            // Read aloud with the device it acts on and what it leaves
            // behind, because "Remove from list" on its own is the same
            // string on every card.
            //
            // `Description`, not `Label`: GTK derives a button's accessible
            // *name* from its own label, and an explicit `Label` property on
            // a `Button::with_label` is silently ignored — measured on this
            // screen through AT-SPI, where the longer string never appeared.
            // A description is additive and is what a screen reader reads
            // after the name, which is the right place for it anyway: the
            // name has to keep matching the visible text or voice control
            // stops being able to say it.
            remove.update_property(&[gtk::accessible::Property::Description(&format!(
                "Removes {} from the list. It stays revoked and cannot reconnect \
                 unless you pair it again.",
                device.device_name
            ))]);
            // The *fingerprint*, never the device id and never the name. Two
            // devices can be called SM-X620; only one of them is this key.
            let fingerprint = device.fingerprint.clone();
            let name = device.device_name.clone();
            let pages = pages.clone();
            remove.connect_clicked(move |button| {
                confirm_remove(button, fingerprint.clone(), name.clone(), pages.clone());
            });
            card.append(&remove);
        }
        container.append(&card);
    }
}

/// "Remove from list" for one revoked device.
///
/// The confirmation says what actually happens — the device stays revoked —
/// rather than naming the mechanism. Nothing here mentions deleting a key or
/// erasing trust, because neither is what the button does.
fn confirm_remove(button: &gtk::Button, fingerprint: String, name: String, pages: Pages) {
    let window = button.root().and_downcast::<gtk::Window>();
    let dialog = adw::AlertDialog::new(
        Some(&format!("Remove {name} from the list?")),
        Some(
            "The device will stay revoked and cannot reconnect unless you pair it \
             again.",
        ),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    // Cancel is both the default and what Escape does: the safe answer is the
    // one a mistimed keypress gives.
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response != "remove" {
            return;
        }
        let pages = pages.clone();
        let fingerprint = fingerprint.clone();
        client::send(
            Request::HideRevokedDevice {
                fingerprint: fingerprint.clone(),
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!(
                        "omnibridge-gui: could not remove the device from the list: {message}"
                    );
                    pages.refresh_now();
                    return;
                }
                // Only after the daemon agreed, and only for this
                // fingerprint. Nothing is chosen in its place.
                pages.forget_peer_choice(&fingerprint);
                pages.refresh_now();
            },
        );
    });
    if let Some(window) = window {
        dialog.present(Some(&window));
    }
}

/// "Remove all revoked devices".
///
/// The count is in the title and on the confirming button, because "all" is
/// the word a person is most likely to read as meaning more than it does.
fn confirm_remove_all(button: &gtk::Button, fingerprints: Vec<String>, pages: Pages) {
    let window = button.root().and_downcast::<gtk::Window>();
    let count = fingerprints.len();
    let dialog = adw::AlertDialog::new(
        Some(&format!("Remove {count} revoked devices from the list?")),
        Some(
            "They will remain revoked and cannot reconnect unless paired again. \
             Devices you still trust are not affected.",
        ),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("remove", &format!("Remove {count}"))]);
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response != "remove" {
            return;
        }
        let pages = pages.clone();
        let fingerprints = fingerprints.clone();
        client::send(Request::HideAllRevokedDevices, move |reply| {
            if let Ok(Response::Error { message }) = reply {
                eprintln!("omnibridge-gui: could not remove the revoked devices: {message}");
                pages.refresh_now();
                return;
            }
            // The choice is cleared only if it named one of the removed
            // fingerprints. A choice pointing at a device that was never
            // revoked is untouched.
            for fingerprint in &fingerprints {
                pages.forget_peer_choice(fingerprint);
            }
            pages.refresh_now();
        });
    });
    if let Some(window) = window {
        dialog.present(Some(&window));
    }
}

fn grant_row(
    device_id: &str,
    capability: &str,
    title: &str,
    description: &str,
    icon: &str,
    granted: bool,
    pages: &Pages,
) -> gtk::Box {
    let row = widgets::row(SPACING_SM);
    let tile = widgets::icon_tile(
        icon,
        if granted {
            "af-tile-teal"
        } else {
            "af-tile-neutral"
        },
    );
    tile.set_size_request(28, 28);
    row.append(&tile);

    let text = widgets::column(0);
    text.append(&widgets::body(title));
    text.append(&widgets::caption(description));
    text.set_hexpand(true);
    row.append(&text);

    let sw = gtk::Switch::new();
    sw.set_active(granted);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(&format!(
        "{title} for this device"
    ))]);
    let device = device_id.to_string();
    let capability = capability.to_string();
    let pages = pages.clone();
    sw.connect_state_set(move |_, wanted| {
        let pages = pages.clone();
        client::send(
            Request::Grant {
                device: device.clone(),
                capability: capability.clone(),
                granted: wanted,
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("omnibridge-gui: the daemon refused the grant change: {message}");
                }
                pages.refresh_now();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);
    row
}

/// Revoking is not a toggle: it is confirmed, and the fingerprint is shown.
fn confirm_revoke(button: &gtk::Button, device_id: String, name: String, pages: Pages) {
    let window = button.root().and_downcast::<gtk::Window>();
    let dialog = adw::AlertDialog::new(
        Some(&format!("Revoke {name}?")),
        Some(
            "This device will no longer be able to connect. Pairing it again means \
             scanning a new code and checking the fingerprint on both screens.",
        ),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("revoke", "Revoke")]);
    dialog.set_response_appearance("revoke", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response != "revoke" {
            return;
        }
        let pages = pages.clone();
        client::send(
            Request::Unpair {
                device: device_id.clone(),
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("omnibridge-gui: could not revoke: {message}");
                }
                pages.refresh_now();
            },
        );
    });
    if let Some(window) = window {
        dialog.present(Some(&window));
    }
}
