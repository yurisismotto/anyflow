//! The trust store: who is trusted, with what, and how to stop.

use adw::prelude::*;
use anyflow_control::{Request, Response};

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

    for device in devices {
        let card = widgets::card();

        let head = widgets::row(SPACING_SM);
        let text = widgets::column(2);
        text.append(&widgets::subtitle(&device.device_name));
        text.set_hexpand(true);
        head.append(&text);
        if device.revoked {
            head.append(&widgets::status_badge(widgets::Status::Revoked));
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
        }
        container.append(&card);
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
                    eprintln!("anyflow-gui: the daemon refused the grant change: {message}");
                }
                pages.render();
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
                    eprintln!("anyflow-gui: could not revoke: {message}");
                }
                pages.render();
            },
        );
    });
    if let Some(window) = window {
        dialog.present(Some(&window));
    }
}
