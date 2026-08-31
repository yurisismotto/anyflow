//! Clipboard: what this machine can do, and what each device is allowed to do.
//!
//! # What is deliberately absent
//!
//! The design reference shows a **clipboard history** panel with previous
//! clips and their text. AnyFlow has none, by design and not by omission:
//! clipboard content is never written to disk, and the control socket carries
//! no clip text at all — a pending clip is described by its size, a hash
//! prefix and its age (`PendingClipReport`), which is enough to tell two
//! clips apart and useless for recovering either.
//!
//! Building that panel would have meant starting to store exactly what the
//! product refuses to store. So the panel is gone, and what replaces it says
//! plainly that nothing is kept.

use anyflow_daemon::control::{ClipboardFlag, ClipboardPeerReport, Request, Response};
use gtk::prelude::*;

use super::Pages;
use crate::widgets::{self, Status, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};

pub fn render(container: &gtk::Box, state: &DaemonState, pages: &Pages) {
    widgets::clear(container);
    container.append(&widgets::title("Clipboard"));

    let Some(report) = &state.clipboard else {
        container.append(&widgets::body_muted("Waiting for the daemon…"));
        return;
    };

    if !report.enabled {
        container.append(&widgets::security_notice(
            "clipboard.v1 is not registered",
            "This daemon was built or started without the clipboard capability.",
            true,
        ));
        return;
    }

    // --- what this machine can do ----------------------------------------
    let backend = widgets::card();
    backend.append(&widgets::section_label("This computer"));
    let row = widgets::row(SPACING_SM);
    row.append(&widgets::icon_tile("edit-paste-symbolic", "af-tile-teal"));
    let text = widgets::column(2);
    text.append(&widgets::subtitle(&report.backend));
    text.append(&widgets::caption(&report.backend_detail));
    text.set_hexpand(true);
    row.append(&text);
    backend.append(&row);

    // Whether clipboard *changes* can be observed here is what decides
    // whether automatic sending is even offerable. Stating it is what stops
    // someone enabling a toggle that silently does nothing.
    backend.append(&widgets::status_badge(if report.watch_available {
        Status::Connected
    } else {
        Status::Warning
    }));
    backend.append(&widgets::caption(if report.watch_available {
        "Clipboard changes can be observed here, so automatic sending is available."
    } else {
        "This session cannot report clipboard changes, so automatic sending is \
         unavailable. Sending by hand still works."
    }));
    container.append(&backend);

    // --- per device -------------------------------------------------------
    if report.peers.is_empty() {
        container.append(&widgets::body_muted(
            "No paired device has clipboard.v1 granted.",
        ));
    } else {
        container.append(&widgets::section_label("Devices"));
        for peer in &report.peers {
            container.append(&peer_card(peer, report.watch_available, pages));
        }
    }

    // --- clips waiting ----------------------------------------------------
    if !report.pending.is_empty() {
        container.append(&widgets::section_label("Waiting to be applied"));
        for clip in &report.pending {
            let card = widgets::card();
            let row = widgets::row(SPACING_SM);
            row.append(&widgets::icon_tile(
                if clip.sensitive {
                    "security-high-symbolic"
                } else {
                    "edit-paste-symbolic"
                },
                if clip.sensitive {
                    "af-tile-amber"
                } else {
                    "af-tile-teal"
                },
            ));
            let text = widgets::column(2);
            text.append(&widgets::subtitle(&format!("From {}", clip.device_name)));
            // Size, hash prefix and age — never content. This is everything
            // the control socket carries, and everything it should.
            text.append(&widgets::caption(&format!(
                "{} bytes · sha256:{} · {}s ago{}",
                clip.bytes,
                clip.hash_prefix,
                clip.age_secs,
                if clip.sensitive {
                    " · marked sensitive"
                } else {
                    ""
                }
            )));
            text.set_hexpand(true);
            row.append(&text);

            let apply = widgets::cta_button("Copy", Some("edit-paste-symbolic"));
            let device = clip.fingerprint_short.replace(' ', "");
            let pages = pages.clone();
            apply.connect_clicked(move |_| {
                let pages = pages.clone();
                client::send(
                    Request::ClipboardApply {
                        device: device.clone(),
                    },
                    move |reply| {
                        if let Ok(Response::Error { message }) = reply {
                            eprintln!("anyflow-gui: could not apply the clip: {message}");
                        }
                        pages.render();
                    },
                );
            });
            row.append(&apply);
            card.append(&row);
            container.append(&card);
        }
    }

    container.append(&widgets::security_notice(
        "Clipboard text is never stored",
        "AnyFlow keeps no clipboard history. A received clip waits in memory with a \
         five-minute expiry and is gone once applied, dismissed or expired — nothing \
         about it reaches a log or a file.",
        false,
    ));
}

fn peer_card(peer: &ClipboardPeerReport, watch_available: bool, pages: &Pages) -> gtk::Box {
    let card = widgets::card();

    let head = widgets::row(SPACING_SM);
    head.append(&widgets::icon_tile("phone-symbolic", "af-tile-blue"));
    let text = widgets::column(2);
    text.append(&widgets::subtitle(&peer.device_name));
    let fp = widgets::caption(&peer.fingerprint_short);
    fp.add_css_class("af-mono");
    text.append(&fp);
    text.set_hexpand(true);
    head.append(&text);
    // Reported apart from the grant on purpose: "not granted" and "this
    // device is no longer trusted at all" are different situations with
    // different fixes, and showing the second as the first understates it.
    head.append(&widgets::status_badge(if peer.revoked {
        Status::Revoked
    } else if peer.connected {
        Status::Connected
    } else {
        Status::Available
    }));
    card.append(&head);

    if !peer.granted {
        card.append(&widgets::body_muted(
            "clipboard.v1 is not granted to this device. Every setting below would \
             be inert, so none is offered.",
        ));
        return card;
    }

    card.append(&widgets::separator());

    let device = peer.device_id.clone();
    card.append(&policy_switch(
        "Allow receiving",
        "Accept clipboard text from this device",
        peer.allow_receive,
        true,
        &device,
        ClipboardFlag::Receive,
        pages,
    ));
    card.append(&policy_switch(
        "Apply automatically",
        "Received text replaces this computer's clipboard as it arrives",
        peer.auto_receive,
        peer.allow_receive,
        &device,
        ClipboardFlag::AutoReceive,
        pages,
    ));
    card.append(&policy_switch(
        "Allow sending",
        "Send this computer's clipboard to this device",
        peer.allow_send,
        true,
        &device,
        ClipboardFlag::Send,
        pages,
    ));
    card.append(&policy_switch(
        "Send automatically",
        if watch_available {
            "Push local clipboard changes as they happen"
        } else {
            "Unavailable: this session cannot report clipboard changes"
        },
        peer.auto_send,
        peer.allow_send && watch_available,
        &device,
        ClipboardFlag::AutoSend,
        pages,
    ));

    let actions = widgets::row(SPACING_XS);
    let send = widgets::cta_button("Send clipboard", Some("document-send-symbolic"));
    send.set_sensitive(peer.connected && peer.allow_send);
    {
        let device = device.clone();
        let pages = pages.clone();
        send.connect_clicked(move |_| {
            let pages = pages.clone();
            client::send(
                // `sensitive` is the sender asking the receiver to treat the
                // clip as a secret. It is a presentation hint and never an
                // access control, so it is not set on the operator's behalf.
                Request::ClipboardSend {
                    device: device.clone(),
                    sensitive: false,
                },
                move |reply| {
                    if let Ok(Response::Error { message }) = reply {
                        eprintln!("anyflow-gui: could not send the clipboard: {message}");
                    }
                    pages.render();
                },
            );
        });
    }
    actions.append(&send);
    if !peer.connected {
        let note = widgets::caption("Connect to this device to send your clipboard.");
        note.set_valign(gtk::Align::Center);
        actions.append(&note);
    }
    card.append(&actions);

    if let Some(outcome) = &peer.last_outcome {
        card.append(&widgets::caption(&format!(
            "Last result: {}",
            outcome.to_lowercase().replace('_', " ")
        )));
    }
    card
}

#[allow(clippy::too_many_arguments)]
fn policy_switch(
    title: &str,
    description: &str,
    active: bool,
    sensitive: bool,
    device: &str,
    flag: ClipboardFlag,
    pages: &Pages,
) -> gtk::Box {
    let row = widgets::row(SPACING_SM);
    let text = widgets::column(0);
    let t = widgets::body(title);
    text.append(&t);
    // Not filler: every row here changes what another machine may do with
    // this one, and the title alone does not say in which direction.
    text.append(&widgets::caption(description));
    text.set_hexpand(true);
    row.append(&text);

    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_sensitive(sensitive);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(title)]);

    let device = device.to_string();
    let pages = pages.clone();
    sw.connect_state_set(move |_, wanted| {
        let pages = pages.clone();
        client::send(
            Request::ClipboardPolicy {
                device: device.clone(),
                flag,
                enabled: wanted,
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("anyflow-gui: the daemon refused the policy change: {message}");
                }
                // Re-read rather than assume: the daemon is the authority on
                // policy, and a refused change must not leave the switch
                // showing a state the daemon does not hold.
                pages.render();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);
    row
}
