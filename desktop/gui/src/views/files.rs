//! Transfers: what is moving, and what finished during this daemon run.

use anyflow_control::{Request, Response, TransferReport};
use gtk::prelude::*;

use crate::widgets::{self, Status, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};

pub fn render(container: &gtk::Box, state: &DaemonState) {
    widgets::clear(container);
    container.append(&widgets::title("Transfers"));

    let transfers = state.transfers.as_deref().unwrap_or(&[]);
    if transfers.is_empty() {
        container.append(&widgets::empty_state(
            "No transfers yet",
            "Files you send or receive appear here while they are moving, and stay \
             listed until the daemon restarts.",
        ));
        return;
    }

    let (active, finished): (Vec<_>, Vec<_>) = transfers.iter().partition(|t| is_active(t));

    if !active.is_empty() {
        container.append(&widgets::section_label("In progress"));
        for t in &active {
            container.append(&transfer_card(t, true));
        }
    }
    if !finished.is_empty() {
        container.append(&widgets::section_label("Completed"));
        for t in &finished {
            container.append(&transfer_card(t, false));
        }
    }

    // The reference offers a persistent history. There is none to offer: the
    // daemon holds transfers in memory for the life of the process and writes
    // nothing about them to disk.
    container.append(&widgets::caption(
        "This list covers the current daemon run. AnyFlow keeps no transfer history on disk.",
    ));
}

fn is_active(t: &TransferReport) -> bool {
    !matches!(
        t.state.as_str(),
        "completed" | "failed" | "cancelled" | "rejected"
    )
}

fn transfer_card(t: &TransferReport, active: bool) -> gtk::Box {
    let card = widgets::card();
    let sending =
        t.direction.eq_ignore_ascii_case("outgoing") || t.direction.eq_ignore_ascii_case("sending");

    let top = widgets::row(SPACING_SM);
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
    top.append(&tile);

    let text = widgets::column(2);
    // Sanitized long before it reaches here: a raw peer-supplied name could
    // otherwise forge the rest of this card.
    text.append(&widgets::subtitle(&t.filename));
    // Direction is stated in words rather than implied by an arrow's
    // rotation, which is invisible to a screen reader and ambiguous to
    // everyone else.
    text.append(&widgets::caption(&format!(
        "{} · {} {}",
        human_bytes(t.size_bytes),
        if sending { "to" } else { "from" },
        t.device_name
    )));
    text.set_hexpand(true);
    top.append(&text);

    if let Some(pct) = t.percentage {
        let l = widgets::caption(&format!("{pct}%"));
        l.add_css_class("af-status-transferring");
        l.set_valign(gtk::Align::Center);
        top.append(&l);
    }

    if active {
        let cancel = gtk::Button::from_icon_name("window-close-symbolic");
        cancel.add_css_class("flat");
        cancel.set_tooltip_text(Some("Cancel this transfer"));
        cancel.update_property(&[gtk::accessible::Property::Label("Cancel this transfer")]);
        let id = t.transfer_id.clone();
        cancel.connect_clicked(move |_| {
            client::send(
                Request::CancelTransfer {
                    transfer: id.clone(),
                },
                |reply| {
                    if let Ok(Response::Error { message }) = reply {
                        eprintln!("anyflow-gui: could not cancel: {message}");
                    }
                },
            );
        });
        top.append(&cancel);
    }
    card.append(&top);

    if active {
        card.append(&widgets::progress(t.percentage.map(|p| p as f64 / 100.0)));
        let line = widgets::row(SPACING_XS);
        line.append(&widgets::status_badge(if t.state == "verifying" {
            // Named apart from "transferring": the bytes have all arrived and
            // the hash is being checked, which is a different wait.
            Status::Connecting
        } else {
            Status::Transferring
        }));
        line.append(&widgets::caption(&format!(
            "{} of {}",
            human_bytes(t.bytes_transferred),
            human_bytes(t.size_bytes)
        )));
        card.append(&line);
    } else {
        let status = match t.state.as_str() {
            "completed" => Status::Success,
            "cancelled" | "rejected" => Status::Disconnected,
            _ => Status::Error,
        };
        let line = widgets::row(SPACING_XS);
        line.append(&widgets::status_badge(status));
        if let Some(failure) = &t.failure {
            line.append(&widgets::caption(failure));
        } else if let Some(stored) = &t.stored_at {
            // Local information only: an absolute path on the receiver is
            // exactly what the offer format deliberately has no room for.
            line.append(&widgets::caption(&format!("Saved to {stored}")));
        }
        card.append(&line);
    }
    card
}

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
