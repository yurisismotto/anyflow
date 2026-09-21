//! The dashboard: what is here, and what can be done with it now.

use adw::prelude::*;
use omnibridge_control::{DeviceReport, Response};

use super::Pages;
use crate::panel::model::{self, Action, PanelModel};
use crate::widgets::{self, Status, SPACING_MD, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};

pub fn render(container: &gtk::Box, state: &DaemonState, pages: &Pages) {
    widgets::clear(container);

    if let Some(error) = &state.error {
        container.append(&widgets::security_notice(
            "The OmniBridge daemon is not reachable",
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

    // The same model the Quick Panel draws, from the same state and the same
    // stored choice. Building it here rather than re-deriving a destination
    // is the point: there is one answer to "where does Send go", and both
    // surfaces read it.
    let panel = PanelModel::build(state, pages.selection.current().as_deref());

    for device in &devices {
        container.append(&device_card(device, &panel, pages));
    }

    // --- activity and quick actions --------------------------------------
    let columns = widgets::row(SPACING_SM);
    columns.set_homogeneous(true);
    columns.append(&activity_card(state));
    columns.append(&quick_actions_card(&panel));
    container.append(&columns);
}

/// One device, as the reference draws it.
fn device_card(device: &DeviceReport, panel: &PanelModel, pages: &Pages) -> gtk::Box {
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
        let gauge = widgets::column(2);
        gauge.add_css_class("af-card-sunken");
        gauge.set_valign(gtk::Align::Start);
        gauge.append(&widgets::caption("Battery"));
        let pct = widgets::subtitle(&format!("{}%", battery.percentage));
        gauge.append(&pct);
        let bar = widgets::progress(Some(battery.percentage as f64 / 100.0));
        bar.set_size_request(120, -1);
        gauge.append(&bar);
        // A reading old enough to be history is labelled as such rather than
        // shown as though it were current.
        gauge.append(&widgets::caption(&if battery.stale {
            format!("last known · {}s ago", battery.age_secs)
        } else {
            battery.charging_state.replace('_', " ")
        }));
        top.append(&gauge);
    }
    // Which device the quick actions mean. Stated on the card, and settable
    // from it, because this is the screen where a person is looking at their
    // devices — and because a Send button that does not say where it sends is
    // how U2 P1 happened.
    let selected = panel
        .peers
        .iter()
        .find(|p| p.fingerprint.eq_ignore_ascii_case(&device.fingerprint))
        .is_some_and(|p| p.selected);
    let choice = widgets::row(SPACING_XS);
    choice.set_valign(gtk::Align::Center);
    if selected {
        let icon = gtk::Image::from_icon_name("object-select-symbolic");
        icon.set_pixel_size(16);
        choice.append(&icon);
        let label = widgets::caption("Selected for quick actions");
        choice.append(&label);
        choice.set_accessible_role(gtk::AccessibleRole::Group);
        choice.update_property(&[gtk::accessible::Property::Label(
            "Selected for quick actions",
        )]);
    } else {
        let use_this = widgets::secondary_button("Use this device", None);
        use_this.update_property(&[gtk::accessible::Property::Label(&format!(
            "Use {} for quick actions",
            device.device_name
        ))]);
        let fingerprint = device.fingerprint.clone();
        let pages = pages.clone();
        use_this.connect_clicked(move |_| pages.choose_peer(&fingerprint));
        choice.append(&use_this);
    }
    top.append(&choice);
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
        // Listed for the same reason the other three are: a person looking at
        // this card should be able to see every grant a device holds, and
        // omitting the one that carries their messages would be the worst
        // omission to make. "Allowed" here still means only that the grant
        // exists — whether anything is actually mirroring is the Notifications
        // page's question, and it has more than one answer.
        (
            "notifications.v1",
            "Notifications",
            "preferences-system-notifications-symbolic",
            "af-tile-amber",
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
/// half an hour. OmniBridge keeps no such log: the daemon reports the transfers
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
        "This run only. OmniBridge keeps no transfer history on disk.",
    ));
    card
}

/// Only actions the daemon can actually perform, aimed where the application
/// says they are aimed.
///
/// This used to pick a destination of its own — the first connected device
/// that happened to hold `files.v1` — which is list position with a filter in
/// front of it, and the trust store's order is not stable. The destination is
/// now [`PanelModel::send_file`], the same value the Quick Panel's button
/// carries, resolved from the fingerprint the person chose.
fn quick_actions_card(panel: &PanelModel) -> gtk::Box {
    let card = widgets::card();
    card.append(&widgets::section_label("Quick actions"));

    let send = widgets::secondary_button("Send a file…", Some("document-send-symbolic"));
    send.set_sensitive(panel.send_file.is_ready());
    match &panel.send_file {
        Action::Ready { peer_name, .. } => {
            let text = format!("Send a file to {peer_name}");
            send.set_tooltip_text(Some(&text));
            send.update_property(&[gtk::accessible::Property::Label(&text)]);
            let action = panel.send_file.clone();
            send.connect_clicked(move |button| choose_and_send_file(button, &action));
        }
        Action::Blocked { reason } => {
            send.set_tooltip_text(Some(reason));
            send.update_property(&[
                gtk::accessible::Property::Label("Send a file"),
                gtk::accessible::Property::Description(reason),
            ]);
        }
    }
    card.append(&send);

    card.append(&widgets::caption(&match &panel.send_file {
        Action::Ready { peer_name, .. } => format!("Sends to {peer_name}"),
        Action::Blocked { reason } => reason.clone(),
    }));
    card
}

fn choose_and_send_file(button: &gtk::Button, action: &Action) {
    let window = button.root().and_downcast::<gtk::Window>();
    let dialog = gtk::FileDialog::builder().title("Send a file").build();
    let action = action.clone();
    dialog.open(
        window.as_ref(),
        gtk::gio::Cancellable::NONE,
        move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            let Some(request) =
                model::send_file_request(&action, path.to_string_lossy().into_owned())
            else {
                return;
            };
            client::send(request, move |reply| {
                // The daemon streams transfer events on this connection; the
                // dashboard picks the transfer up on its next refresh, so only an
                // outright refusal needs reporting here.
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("omnibridge-gui: could not offer the file: {message}");
                }
            });
        },
    );
}
