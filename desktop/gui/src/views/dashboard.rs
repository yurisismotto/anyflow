//! The dashboard: what is here, and what can be done with it now.

use adw::prelude::*;
use anyflow_daemon::control::{DeviceReport, Request, Response};

use super::Pages;
use crate::widgets::{self, Status, SPACING_MD, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};

pub fn render(container: &gtk::Box, state: &DaemonState, pages: &Pages) {
    widgets::clear(container);

    if let Some(error) = &state.error {
        container.append(&widgets::security_notice(
            "The AnyFlow daemon is not reachable",
            error,
            true,
        ));
        return;
    }

    // --- header ---------------------------------------------------------
    let header = widgets::row(SPACING_SM);
    header.append(&widgets::heading("Devices"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let pair = widgets::secondary_button("Pair device", Some("list-add-symbolic"));
    {
        let pages = pages.clone();
        pair.connect_clicked(move |b| {
            super::present_pairing_dialog(b.root().and_downcast::<gtk::Window>().as_ref(), &pages);
        });
    }
    header.append(&pair);
    container.append(&header);

    // --- devices ---------------------------------------------------------
    let devices: Vec<&DeviceReport> = state
        .devices
        .as_deref()
        .or(state.status.as_ref().map(|s| s.devices.as_slice()))
        .unwrap_or(&[])
        .iter()
        .filter(|d| !d.revoked)
        .collect();

    if devices.is_empty() {
        container.append(&widgets::empty_state(
            "Connect your first device",
            "Pair a phone or tablet to send files and share your clipboard. \
             Nothing leaves your network.",
        ));
        let cta = widgets::cta_button("Pair device", Some("camera-photo-symbolic"));
        cta.set_halign(gtk::Align::Center);
        {
            let pages = pages.clone();
            cta.connect_clicked(move |b| {
                super::present_pairing_dialog(
                    b.root().and_downcast::<gtk::Window>().as_ref(),
                    &pages,
                );
            });
        }
        container.append(&cta);
        return;
    }

    for device in &devices {
        container.append(&device_card(device));
    }

    // --- activity and quick actions --------------------------------------
    let columns = widgets::row(SPACING_SM);
    columns.set_homogeneous(true);
    columns.append(&activity_card(state));
    columns.append(&quick_actions_card(&devices));
    container.append(&columns);
}

/// One device, as the reference draws it.
fn device_card(device: &DeviceReport) -> gtk::Box {
    let card = widgets::card();

    let top = widgets::row(SPACING_SM);
    let tile = widgets::icon_tile(platform_icon(&device.platform), "af-tile-blue");
    tile.set_valign(gtk::Align::Center);
    top.append(&tile);

    let text = widgets::column(2);
    text.set_valign(gtk::Align::Center);
    text.append(&widgets::subtitle(&device.device_name));
    // The reference shows an IP here. The control protocol does not report a
    // peer address, and inventing one would be worse than useless — the
    // fingerprint is both true and the thing that actually identifies the
    // device.
    let meta = widgets::caption(&format!(
        "{} · {}",
        device.platform, device.fingerprint_short
    ));
    meta.add_css_class("af-mono");
    text.append(&meta);
    text.append(&widgets::status_badge(Status::from_device_state(
        device.state,
    )));
    text.set_hexpand(true);
    top.append(&text);

    // Battery only while a session is live: telemetry is dropped when a peer
    // disconnects, so an offline device never carries a number here.
    if let Some(battery) = &device.battery {
        let panel = widgets::column(2);
        panel.add_css_class("af-card-sunken");
        panel.set_valign(gtk::Align::Start);
        panel.append(&widgets::caption("Battery"));
        let pct = widgets::subtitle(&format!("{}%", battery.percentage));
        panel.append(&pct);
        let bar = widgets::progress(Some(battery.percentage as f64 / 100.0));
        bar.set_size_request(120, -1);
        panel.append(&bar);
        // A reading old enough to be history is labelled as such rather than
        // shown as though it were current.
        panel.append(&widgets::caption(&if battery.stale {
            format!("last known · {}s ago", battery.age_secs)
        } else {
            battery.charging_state.replace('_', " ")
        }));
        top.append(&panel);
    }
    card.append(&top);

    card.append(&widgets::separator());

    // Grants, not live state: a chip says "Clipboard" because the capability
    // is granted, not because something is syncing right now.
    let caps = widgets::row(SPACING_MD);
    // Chips sit together on the left rather than spreading across the card.
    caps.set_halign(gtk::Align::Start);
    for (id, label, icon, tone) in [
        (
            "clipboard.v1",
            "Clipboard",
            "edit-paste-symbolic",
            "af-tile-teal",
        ),
        ("files.v1", "Files", "folder-symbolic", "af-tile-blue"),
        (
            "battery.v1",
            "Battery",
            "battery-symbolic",
            "af-tile-violet",
        ),
    ] {
        let granted = device.granted_capabilities.iter().any(|c| c == id);
        let chip = widgets::row(SPACING_XS);
        let tile = widgets::icon_tile(icon, if granted { tone } else { "af-tile-neutral" });
        tile.set_size_request(28, 28);
        chip.append(&tile);
        let text = widgets::column(0);
        let name = widgets::caption(label);
        if granted {
            name.add_css_class("af-text-primary");
        }
        text.append(&name);
        text.append(&widgets::caption(if granted {
            "Allowed"
        } else {
            "Not allowed"
        }));
        chip.append(&text);
        chip.set_valign(gtk::Align::Center);
        chip.set_tooltip_text(Some(&format!(
            "{label}: {}",
            if granted { "granted" } else { "not granted" }
        )));
        caps.append(&chip);
    }
    card.append(&caps);
    card
}

fn platform_icon(platform: &str) -> &'static str {
    let p = platform.to_ascii_lowercase();
    if p.contains("android") || p.contains("ios") {
        "phone-symbolic"
    } else {
        "computer-symbolic"
    }
}

/// What is moving right now.
///
/// The reference calls this "Recent activity" and shows timestamps going back
/// half an hour. AnyFlow keeps no such log: the daemon reports the transfers
/// of *this run* and nothing is written to disk. So this shows exactly that,
/// and says so, rather than implying a history that does not exist.
fn activity_card(state: &DaemonState) -> gtk::Box {
    let card = widgets::card();
    card.append(&widgets::section_label("Activity"));

    let transfers = state.transfers.as_deref().unwrap_or(&[]);
    if transfers.is_empty() {
        card.append(&widgets::body_muted(
            "Nothing has moved since the daemon started.",
        ));
    } else {
        for t in transfers.iter().take(5) {
            let r = widgets::row(SPACING_XS);
            let sending = t.direction.eq_ignore_ascii_case("outgoing")
                || t.direction.eq_ignore_ascii_case("sending");
            let tile = widgets::icon_tile(
                if sending {
                    "document-send-symbolic"
                } else {
                    "folder-download-symbolic"
                },
                if sending {
                    "af-tile-blue"
                } else {
                    "af-tile-violet"
                },
            );
            tile.set_size_request(28, 28);
            r.append(&tile);
            let text = widgets::column(0);
            let name = widgets::caption(&t.filename);
            name.add_css_class("af-text-primary");
            text.append(&name);
            text.append(&widgets::caption(&format!(
                "{} {} · {}",
                if sending { "to" } else { "from" },
                t.device_name,
                t.state
            )));
            text.set_hexpand(true);
            r.append(&text);
            card.append(&r);
        }
    }
    card.append(&widgets::caption(
        "This run only. AnyFlow keeps no transfer history on disk.",
    ));
    card
}

/// Only actions the daemon can actually perform.
fn quick_actions_card(devices: &[&DeviceReport]) -> gtk::Box {
    let card = widgets::card();
    card.append(&widgets::section_label("Quick actions"));

    let connected: Vec<&DeviceReport> = devices.iter().copied().filter(|d| d.connected).collect();
    let target = connected
        .iter()
        .find(|d| d.granted_capabilities.iter().any(|c| c == "files.v1"))
        .copied();

    let send = widgets::secondary_button("Send a file…", Some("document-send-symbolic"));
    send.set_sensitive(target.is_some());
    if let Some(device) = target {
        let device_id = device.device_id.clone();
        let name = device.device_name.clone();
        send.connect_clicked(move |button| {
            choose_and_send_file(button, device_id.clone(), name.clone());
        });
    } else {
        send.set_tooltip_text(Some(
            "Connect a device and grant it files.v1 to send a file.",
        ));
    }
    card.append(&send);

    if let Some(device) = target {
        card.append(&widgets::caption(&format!(
            "Sends to {}",
            device.device_name
        )));
    } else if connected.is_empty() {
        card.append(&widgets::caption("No device is connected right now."));
    } else {
        card.append(&widgets::caption(
            "No connected device has files.v1 granted.",
        ));
    }
    card
}

fn choose_and_send_file(button: &gtk::Button, device_id: String, device_name: String) {
    let window = button.root().and_downcast::<gtk::Window>();
    let dialog = gtk::FileDialog::builder().title("Send a file").build();
    dialog.open(
        window.as_ref(),
        gtk::gio::Cancellable::NONE,
        move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            let path = path.to_string_lossy().into_owned();
            client::send(
                Request::Send {
                    device: device_id,
                    path,
                },
                move |reply| {
                    // The daemon streams transfer events on this connection; the
                    // dashboard picks the transfer up on its next refresh, so
                    // only an outright refusal needs reporting here.
                    if let Ok(Response::Error { message }) = reply {
                        eprintln!(
                            "anyflow-gui: could not offer the file to {device_name}: {message}"
                        );
                    }
                },
            );
        },
    );
}
