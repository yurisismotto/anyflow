//! Every device this daemon knows, including the ones it no longer trusts.

use gtk::prelude::*;

use crate::widgets::{self, Status, SPACING_SM};
use crate::DaemonState;

pub fn render(container: &gtk::Box, state: &DaemonState) {
    widgets::clear(container);
    container.append(&widgets::title("Devices"));

    let devices = state
        .devices
        .as_deref()
        .or(state.status.as_ref().map(|s| s.devices.as_slice()))
        .unwrap_or(&[]);

    if devices.is_empty() {
        container.append(&widgets::empty_state(
            "No devices yet",
            "Pair a device from the dashboard to see it here.",
        ));
        return;
    }

    for device in devices {
        let card = widgets::card();
        let row = widgets::row(SPACING_SM);
        row.append(&widgets::icon_tile(
            if device.platform.to_ascii_lowercase().contains("android") {
                "phone-symbolic"
            } else {
                "computer-symbolic"
            },
            if device.revoked {
                "ob-tile-neutral"
            } else {
                "ob-tile-blue"
            },
        ));
        let text = widgets::column(2);
        text.append(&widgets::subtitle(&device.device_name));
        let fp = widgets::caption(&device.fingerprint_short);
        fp.add_css_class("ob-mono");
        text.append(&fp);
        text.set_hexpand(true);
        row.append(&text);
        row.append(&widgets::status_badge(Status::from_device_state(
            device.state,
        )));
        card.append(&row);

        // Paired is durable; connected is momentary. Reporting them together
        // is what stops a dead session from reading as a live one.
        let mut facts = vec![format!("Platform: {}", device.platform)];
        if let Some(silent) = device.silent_secs {
            facts.push(format!("Silent for {silent}s"));
        }
        if let Some(seen) = device.last_seen_secs_ago {
            facts.push(format!("Last session ended {seen}s ago"));
        }
        if device.granted_capabilities.is_empty() {
            facts.push("No capabilities granted".into());
        } else {
            facts.push(format!(
                "Granted: {}",
                device.granted_capabilities.join(", ")
            ));
        }
        card.append(&widgets::caption(&facts.join(" · ")));
        container.append(&card);
    }
}
