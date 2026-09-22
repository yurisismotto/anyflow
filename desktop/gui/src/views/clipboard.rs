//! Clipboard: what this machine can do, and what each device is allowed to do.
//!
//! # What is deliberately absent
//!
//! The design reference shows a **clipboard history** panel with previous
//! clips and their text. OmniBridge has none, by design and not by omission:
//! clipboard content is never written to disk, and the control socket carries
//! no clip text at all — a pending clip is described by its size, a hash
//! prefix and its age (`PendingClipReport`), which is enough to tell two
//! clips apart and useless for recovering either.
//!
//! Building that panel would have meant starting to store exactly what the
//! product refuses to store. So the panel is gone, and what replaces it says
//! plainly that nothing is kept.

use gtk::prelude::*;
use omnibridge_control::{
    ClipboardFlag, ClipboardPeerReport, ClipboardStatusReport, Request, Response,
};

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
    row.append(&widgets::icon_tile("edit-paste-symbolic", "ob-tile-cyan"));
    let text = widgets::column(2);
    // `report.backend` and `report.backend_detail` are diagnostics —
    // "wl-clipboard", "watch: XFIXES on the Xwayland CLIPBOARD selection".
    // True, useful when something is wrong, and not what a clipboard
    // settings page should open with. The headline is derived from exactly
    // the same bit the raw strings describe; nothing new is claimed.
    let summary = BackendSummary::of(report);
    text.append(&widgets::subtitle(summary.title()));
    text.append(&widgets::caption(summary.detail()));
    text.set_hexpand(true);
    row.append(&text);
    // The diagnostics are not deleted, just demoted: they stay on the wire,
    // in `omnibridge status`, and here on hover for anyone debugging a
    // clipboard that is misbehaving.
    row.set_tooltip_text(Some(&format!(
        "{} — {}",
        report.backend, report.backend_detail
    )));
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
        "Clipboard changes can be detected on this computer, so automatic sending \
         is available."
    } else {
        "Clipboard changes cannot be detected on this computer, so automatic \
         sending is unavailable. Sending by hand still works."
    }));

    // Sensitive marking is its own row, and it is here rather than only in a
    // failure message on purpose: without it, a person discovers that this
    // desktop will not accept a password at the moment a password does not
    // arrive. It is separate from the watch row above because the two fail
    // for unrelated reasons and a desktop routinely has one and not the
    // other — every current Ubuntu LTS and Debian Stable ships a `wl-copy`
    // that works and cannot mark a clip.
    let sensitive = SensitiveState::of(report);
    let notice = widgets::security_notice(
        sensitive.title(),
        sensitive.detail(),
        sensitive.needs_attention(),
    );
    // The state is in the words, not in the tint or the icon: `title()` names
    // it outright, so nothing here depends on seeing a colour. Announced as
    // one phrase rather than as an icon followed by two fragments.
    notice.set_accessible_role(gtk::AccessibleRole::Group);
    notice.update_property(&[gtk::accessible::Property::Label(&format!(
        "{}. {}",
        sensitive.title(),
        sensitive.detail()
    ))]);
    backend.append(&notice);

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
                    "ob-tile-amber"
                } else {
                    "ob-tile-cyan"
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
                            eprintln!("omnibridge-gui: could not apply the clip: {message}");
                        }
                        pages.refresh_now();
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
        "OmniBridge keeps no clipboard history. A received clip waits in memory with a \
         five-minute expiry and is gone once applied, dismissed or expired — nothing \
         about it reaches a log or a file.",
        false,
    ));
}

/// What this computer's clipboard can do, in the product's own words.
///
/// The control socket reports the backend as `wl-clipboard` and its detail as
/// `wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD selection; sensitive
/// marking: yes`. Both are true and both are diagnostics: they name a helper
/// binary, an X11 extension and a selection, none of which a person changing
/// a clipboard setting has any use for.
///
/// This is the same fact — [`ClipboardStatusReport::backend_available`], the
/// bit those strings describe — said in language the page can lead with. It
/// derives nothing and claims nothing the report did not already state, which
/// is the difference between rewording and inventing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackendSummary {
    /// The ordinary clipboard works here.
    Ready,
    /// There is no usable clipboard on this session.
    Unavailable,
}

impl BackendSummary {
    pub(super) fn of(report: &ClipboardStatusReport) -> Self {
        if report.backend_available {
            Self::Ready
        } else {
            Self::Unavailable
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Ready => "Clipboard ready",
            Self::Unavailable => "Clipboard unavailable",
        }
    }

    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::Ready => {
                "Text you copy here can be sent to your devices, and text they send \
                 can be copied here."
            }
            // No package name and no command: which helper is missing is a
            // per-distribution answer, and the tooltip above already carries
            // the exact one for anyone diagnosing it.
            Self::Unavailable => {
                "This desktop session has no clipboard OmniBridge can use, so \
                 clipboard text cannot be sent or received."
            }
        }
    }
}

/// Whether this desktop can mark a clip *sensitive*, as one named state.
///
/// A three-valued answer rather than a boolean, because "there is no
/// clipboard at all" and "the clipboard works but cannot mark a clip" need
/// different words and lead somewhere different. Collapsing them would tell a
/// user with no `wl-clipboard` to go looking for a newer `wl-clipboard`
/// version, and a user with the wrong version that their clipboard is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SensitiveState {
    /// Sensitive clips are written, marked.
    Available,
    /// The clipboard works; this build of `wl-copy` cannot mark a clip.
    NotMarkable,
    /// There is no usable clipboard here at all, so the question is moot.
    NoClipboard,
}

impl SensitiveState {
    pub(super) fn of(report: &ClipboardStatusReport) -> Self {
        // Order matters: a machine with no clipboard also reports no
        // sensitive marking, and naming the narrower fault first would send
        // somebody after a package version when nothing is installed.
        if !report.backend_available {
            Self::NoClipboard
        } else if report.sensitive_available {
            Self::Available
        } else {
            Self::NotMarkable
        }
    }

    /// The state, in words. Never a colour and never an icon alone.
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Available => "Sensitive clipboard: available",
            Self::NotMarkable => "Sensitive clipboard: unavailable",
            Self::NoClipboard => "Sensitive clipboard: unavailable",
        }
    }

    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::Available => {
                "A clip your phone marks as a password or other secret is written here \
                 marked sensitive, so clipboard managers leave it out of their history."
            }
            // No package-manager command: OmniBridge does not know which package
            // manager this machine has, and the package is named the same on
            // every distribution OmniBridge supports. The version is offered as
            // guidance for choosing a build, not as the test — some
            // distributions backport the flag into an earlier version, which
            // is why OmniBridge asks the tool instead of reading its version.
            Self::NotMarkable => {
                "Ordinary clipboard sharing works normally. What this desktop cannot do is \
                 mark a clip as sensitive: its wl-copy has no --sensitive option. OmniBridge \
                 therefore refuses a clip your phone marked as a secret rather than writing \
                 it unmarked, because an unmarked password would be kept in your clipboard \
                 manager's history without you being told. Installing a wl-clipboard build \
                 whose wl-copy accepts --sensitive restores it (upstream added the option \
                 in 2.3.0)."
            }
            Self::NoClipboard => {
                "There is no usable clipboard on this session, so nothing can be marked \
                 sensitive either. The line above says what is missing."
            }
        }
    }

    /// Whether to wear the caution tint. `NoClipboard` does not: the missing
    /// clipboard is already reported above, and saying it twice in warning
    /// colours overstates one fault into two.
    pub(super) fn needs_attention(self) -> bool {
        matches!(self, Self::NotMarkable)
    }
}

/// Why "Send clipboard" cannot be pressed for this device, or `None`.
///
/// Both conditions had to be named. The button was gated on
/// `connected && allow_send` while only the *disconnected* case printed a
/// note beside it, so a device that was connected with sending turned off
/// got a greyed-out button and no explanation at all — the dead control the
/// guidelines forbid, in the one place where the fix is a switch three rows
/// further up the same card.
///
/// Order matters: sending being off is the more specific fact and the one
/// the person can act on without leaving the page.
pub(super) fn send_blocked_reason(peer: &ClipboardPeerReport) -> Option<&'static str> {
    if !peer.allow_send {
        Some("Sending is turned off for this device. Turn on \u{201c}Allow sending\u{201d} above.")
    } else if !peer.connected {
        Some("Connect to this device to send your clipboard.")
    } else {
        None
    }
}

fn peer_card(peer: &ClipboardPeerReport, watch_available: bool, pages: &Pages) -> gtk::Box {
    let card = widgets::card();

    let head = widgets::row(SPACING_SM);
    head.append(&widgets::icon_tile("phone-symbolic", "ob-tile-blue"));
    let text = widgets::column(2);
    text.append(&widgets::subtitle(&peer.device_name));
    let fp = widgets::caption(&peer.fingerprint_short);
    fp.add_css_class("ob-mono");
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
    let blocked = send_blocked_reason(peer);
    send.set_sensitive(blocked.is_none());
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
                        eprintln!("omnibridge-gui: could not send the clipboard: {message}");
                    }
                    pages.refresh_now();
                },
            );
        });
    }
    actions.append(&send);
    if let Some(reason) = blocked {
        let note = widgets::caption(reason);
        note.set_valign(gtk::Align::Center);
        note.set_wrap(true);
        // The reason is part of what the button means, so it is announced
        // with it rather than as a stray sentence further down the card.
        send.update_property(&[gtk::accessible::Property::Description(reason)]);
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
                    eprintln!("omnibridge-gui: the daemon refused the policy change: {message}");
                }
                // Re-read rather than assume: the daemon is the authority on
                // policy, and a refused change must not leave the switch
                // showing a state the daemon does not hold.
                pages.refresh_now();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);
    row
}

#[cfg(test)]
pub(in crate::views) mod tests {
    use super::{render, SensitiveState};
    use crate::{DaemonState, Page};
    use gtk::prelude::*;
    use omnibridge_control::ClipboardStatusReport;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A desktop where everything works. Each test breaks exactly one thing.
    fn report() -> ClipboardStatusReport {
        ClipboardStatusReport {
            enabled: true,
            backend: "wl-clipboard".into(),
            backend_detail: "wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD \
                             selection; sensitive marking: yes"
                .into(),
            backend_available: true,
            watch_available: true,
            sensitive_available: true,
            sensitive_detail: String::new(),
            event_cache_entries: 0,
            suppression_cache_entries: 0,
            peers: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// The Ubuntu 24.04 / Ubuntu 26.04 / Debian 13 machine: `wl-copy` is
    /// present and works, and has no `--sensitive`.
    fn wl_clipboard_2_2_1() -> ClipboardStatusReport {
        ClipboardStatusReport {
            sensitive_available: false,
            sensitive_detail: "this system's wl-copy does not support sensitive clipboard \
                               marking."
                .into(),
            ..report()
        }
    }

    #[test]
    fn a_working_desktop_says_sensitive_clips_are_available() {
        assert_eq!(SensitiveState::of(&report()), SensitiveState::Available);
        assert!(!SensitiveState::Available.needs_attention());
    }

    #[test]
    fn an_old_wl_copy_is_not_markable_rather_than_a_broken_clipboard() {
        // The distinction this whole state exists for. Ordinary mirroring on
        // these three distributions is fine, and a page that called the
        // clipboard broken would send somebody to fix something that works.
        let state = SensitiveState::of(&wl_clipboard_2_2_1());
        assert_eq!(state, SensitiveState::NotMarkable);
        assert!(state.needs_attention());
    }

    #[test]
    fn no_clipboard_at_all_outranks_the_narrower_fault() {
        // Both bits are false on a machine with no wl-clipboard. Naming the
        // version problem there would send somebody after a newer build of a
        // package they have not installed.
        let degraded = ClipboardStatusReport {
            backend_available: false,
            sensitive_available: false,
            sensitive_detail: "wl-clipboard is not installed".into(),
            ..report()
        };
        assert_eq!(SensitiveState::of(&degraded), SensitiveState::NoClipboard);
    }

    #[test]
    fn sensitive_marking_is_independent_of_the_change_watch() {
        // GNOME cannot report clipboard changes through data-control and
        // Fedora's wl-copy can mark a clip; Ubuntu is the other way round on
        // the second axis. Neither bit may be derived from the other.
        let no_watch = ClipboardStatusReport {
            watch_available: false,
            ..report()
        };
        assert_eq!(SensitiveState::of(&no_watch), SensitiveState::Available);

        let no_sensitive = wl_clipboard_2_2_1();
        assert!(no_sensitive.watch_available);
        assert_eq!(
            SensitiveState::of(&no_sensitive),
            SensitiveState::NotMarkable
        );
    }

    #[test]
    fn every_state_names_itself_in_words_and_explains_the_consequence() {
        // Nothing is communicated by colour or by an icon alone: the title
        // carries the state and the body carries what it means.
        for state in [
            SensitiveState::Available,
            SensitiveState::NotMarkable,
            SensitiveState::NoClipboard,
        ] {
            assert!(
                state.title().contains("available"),
                "{state:?} must state the answer in words"
            );
            assert!(state.detail().len() > 40, "{state:?}");
        }
        assert!(SensitiveState::NotMarkable.title().contains("unavailable"));
        assert!(SensitiveState::Available.title().contains(": available"));
    }

    #[test]
    fn the_unavailable_copy_reassures_about_ordinary_sharing_and_explains_the_refusal() {
        let detail = SensitiveState::NotMarkable.detail();
        // Ordinary clipboard sharing still works, and the page must say so
        // before it says anything is wrong.
        assert!(detail.contains("Ordinary clipboard sharing works normally"));
        // Why OmniBridge refuses rather than downgrading (PLAT-DEC-013).
        assert!(detail.contains("refuses"));
        assert!(detail.contains("history"));
        // And what to do about it, without choosing the user's package manager.
        assert!(detail.contains("wl-clipboard"));
    }

    #[test]
    fn no_state_prescribes_a_package_manager() {
        // L4: package-manager commands belong in documentation, where they can
        // be correct per distribution. A runtime string cannot be.
        for state in [
            SensitiveState::Available,
            SensitiveState::NotMarkable,
            SensitiveState::NoClipboard,
        ] {
            let text = format!("{} {}", state.title(), state.detail());
            for forbidden in ["dnf", "apt", "apt-get", "pacman", "zypper", "rpm", "sudo"] {
                assert!(
                    !text.contains(forbidden),
                    "{state:?} names a package manager: {forbidden}"
                );
            }
            for distro in ["Fedora", "Ubuntu", "Debian"] {
                assert!(!text.contains(distro), "{state:?} names {distro}");
            }
        }
    }

    // ---- the primary UI speaks the product's language --------------------

    #[test]
    fn a_working_clipboard_leads_with_plain_language() {
        let summary = super::BackendSummary::of(&report());
        assert_eq!(summary, super::BackendSummary::Ready);
        assert_eq!(summary.title(), "Clipboard ready");
    }

    #[test]
    fn no_usable_clipboard_says_so_without_naming_a_helper() {
        let none = ClipboardStatusReport {
            backend_available: false,
            ..report()
        };
        let summary = super::BackendSummary::of(&none);
        assert_eq!(summary, super::BackendSummary::Unavailable);
        assert!(summary.title().contains("unavailable"));
    }

    /// The headline is a rewording of an existing bit, not a new claim.
    ///
    /// `backend_available` is the whole input. If these ever disagree, the
    /// page has started asserting something the daemon did not report.
    #[test]
    fn the_summary_tracks_backend_available_and_nothing_else() {
        for available in [true, false] {
            let r = ClipboardStatusReport {
                backend_available: available,
                // Deliberately varied: neither of these may move the answer.
                watch_available: !available,
                sensitive_available: !available,
                ..report()
            };
            let expected = if available {
                super::BackendSummary::Ready
            } else {
                super::BackendSummary::Unavailable
            };
            assert_eq!(super::BackendSummary::of(&r), expected);
        }
    }

    /// No implementation detail reaches the primary UI strings.
    ///
    /// The control socket reports `wl-clipboard` and `watch: XFIXES on the
    /// Xwayland CLIPBOARD selection`. Both are true, both are diagnostics,
    /// and neither belongs in the first line of a settings page.
    #[test]
    fn the_primary_strings_carry_no_implementation_jargon() {
        let jargon = [
            "wl-clipboard",
            "wl-copy",
            "XFIXES",
            "Xwayland",
            "CLIPBOARD selection",
            "data-control",
            "gsettings",
        ];
        let mut strings: Vec<&str> = Vec::new();
        for s in [
            super::BackendSummary::Ready,
            super::BackendSummary::Unavailable,
        ] {
            strings.push(s.title());
            strings.push(s.detail());
        }
        for text in strings {
            for term in jargon {
                assert!(
                    !text.contains(term),
                    "the primary clipboard UI says {term:?}: {text}"
                );
            }
        }
    }

    /// The truthful capability state is still represented, just reworded.
    #[test]
    fn every_summary_state_explains_what_it_means_for_the_user() {
        for s in [
            super::BackendSummary::Ready,
            super::BackendSummary::Unavailable,
        ] {
            assert!(!s.title().is_empty());
            assert!(s.detail().len() > 40, "{s:?} explains nothing");
        }
        assert!(super::BackendSummary::Ready.detail().contains("sent"));
        assert!(super::BackendSummary::Unavailable
            .detail()
            .contains("cannot"));
    }

    /// Sensitive marking keeps its own words, untouched by the rewording.
    #[test]
    fn the_sensitive_state_is_still_reported_separately() {
        let r = wl_clipboard_2_2_1();
        // Ordinary clipboard fine, sensitive marking not — the two questions
        // stay independent after the rewrite.
        assert_eq!(super::BackendSummary::of(&r), super::BackendSummary::Ready);
        assert_eq!(SensitiveState::of(&r), SensitiveState::NotMarkable);
    }

    // ---- the send button's reason ----------------------------------------

    fn peer(connected: bool, allow_send: bool) -> omnibridge_control::ClipboardPeerReport {
        omnibridge_control::ClipboardPeerReport {
            device_id: "d0".into(),
            device_name: "Pixel".into(),
            fingerprint_short: "A1B2 C3D4".into(),
            granted: true,
            revoked: false,
            connected,
            allow_send,
            allow_receive: true,
            auto_send: false,
            auto_receive: false,
            last_outcome: None,
        }
    }

    #[test]
    fn a_ready_peer_has_no_blocked_reason() {
        assert_eq!(super::send_blocked_reason(&peer(true, true)), None);
    }

    /// The regression this function exists for.
    ///
    /// A connected device with sending switched off used to get a greyed-out
    /// button and nothing else — a dead control, in the one place where the
    /// fix is a switch three rows further up the same card.
    #[test]
    fn sending_turned_off_is_explained_rather_than_left_dead() {
        let reason =
            super::send_blocked_reason(&peer(true, false)).expect("a disabled send must say why");
        assert!(
            reason.contains("Allow sending"),
            "the reason should point at the switch that fixes it: {reason}"
        );
    }

    #[test]
    fn a_disconnected_peer_is_told_to_connect() {
        let reason =
            super::send_blocked_reason(&peer(false, true)).expect("a disabled send must say why");
        assert!(reason.contains("Connect"), "{reason}");
    }

    /// Every disabled state carries a reason. Never a dead control.
    #[test]
    fn every_unsendable_combination_carries_a_reason() {
        for connected in [true, false] {
            for allow_send in [true, false] {
                let p = peer(connected, allow_send);
                let reason = super::send_blocked_reason(&p);
                if connected && allow_send {
                    assert!(reason.is_none());
                } else {
                    assert!(
                        reason.is_some_and(|r| !r.is_empty()),
                        "connected={connected} allow_send={allow_send} has no reason"
                    );
                }
            }
        }
    }

    /// Sending-off outranks disconnected: it is the more specific fact and
    /// the one that can be acted on without leaving the page.
    #[test]
    fn the_more_specific_reason_wins_when_both_apply() {
        let reason = super::send_blocked_reason(&peer(false, false)).expect("a reason");
        assert!(reason.contains("Allow sending"), "{reason}");
    }

    // -----------------------------------------------------------------------
    // The page itself, built from a fabricated report
    // -----------------------------------------------------------------------
    //
    // Needs a display, so it is `#[ignore]`d and asked for by name — the same
    // convention `views::notifications` uses:
    //
    //   cargo test -p omnibridge-gui -- --ignored --test-threads=1

    fn page(report: ClipboardStatusReport) -> gtk::Box {
        if !gtk::is_initialized() {
            gtk::init().expect("a display is needed: run with --ignored on a desktop session");
        }
        let state = Rc::new(RefCell::new(DaemonState {
            clipboard: Some(report),
            ..DaemonState::default()
        }));
        let stack = gtk::Stack::new();
        let pages = super::Pages::new(&stack, state.clone(), crate::views::test_selection());
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let borrowed = state.borrow();
        render(&container, &borrowed, &pages);
        let _ = Page::Clipboard;
        let _ = stack;
        container
    }

    fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut child = root.first_child();
        while let Some(w) = child {
            out.push(w.clone());
            out.extend(descendants(&w));
            child = w.next_sibling();
        }
        out
    }

    fn labels(root: &gtk::Box) -> Vec<String> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect()
    }

    /// Every widget-tree assertion for this page, in one section.
    ///
    /// Called from `views::display_gate` rather than being a `#[test]` of its
    /// own: GTK is initialised once per process and binds to the thread that
    /// did it, while libtest gives every `#[test]` a thread. Two display
    /// tests in one binary means the second one panics inside GTK.
    pub(in crate::views) fn the_clipboard_page_widget_tree() {
        the_gap_is_stated_on_the_page_before_any_clip_fails();
        an_ordinary_desktop_does_not_warn_about_sensitive_clips();
        the_state_is_readable_by_an_assistive_technology();
        the_rendered_page_speaks_the_products_language();
    }

    /// The jargon check, against the real widget tree rather than the strings.
    ///
    /// The constants test covers what `BackendSummary` returns; this covers
    /// what actually reaches a label, which is the thing a reviewer sees.
    fn the_rendered_page_speaks_the_products_language() {
        let page = page(report());
        let text = labels(&page).join("\n");

        assert!(
            text.contains("Clipboard ready"),
            "the page must lead with plain language, got:\n{text}"
        );
        for term in ["wl-clipboard", "XFIXES", "Xwayland", "CLIPBOARD selection"] {
            assert!(
                !text.contains(term),
                "{term:?} reached a label on the clipboard page:\n{text}"
            );
        }
        // The truthful capability statement survives the rewording.
        assert!(
            text.contains("automatic sending is available"),
            "the watch state must still be stated:\n{text}"
        );
        assert!(text.contains("Sensitive clipboard: available"), "{text}");
    }

    fn the_gap_is_stated_on_the_page_before_any_clip_fails() {
        let page = page(wl_clipboard_2_2_1());
        let text = labels(&page).join("\n");
        assert!(
            text.contains("Sensitive clipboard: unavailable"),
            "the page must name the gap up front, got:\n{text}"
        );
        assert!(
            text.contains("Ordinary clipboard sharing works normally"),
            "and must not imply the whole clipboard is broken:\n{text}"
        );
    }

    fn an_ordinary_desktop_does_not_warn_about_sensitive_clips() {
        let page = page(report());
        let text = labels(&page).join("\n");
        assert!(text.contains("Sensitive clipboard: available"), "{text}");
        assert!(
            !text.contains("refuses"),
            "a working desktop must not be told about a refusal that cannot happen"
        );
    }

    fn the_state_is_readable_by_an_assistive_technology() {
        // The notice carries its own label, so the state is announced as one
        // phrase rather than being inferred from a tint.
        let page = page(wl_clipboard_2_2_1());
        let announced = descendants(page.upcast_ref()).into_iter().any(|w| {
            w.accessible_role() == gtk::AccessibleRole::Group
                && w.first_child().is_some()
                && labels_of(&w)
                    .iter()
                    .any(|l| l.contains("Sensitive clipboard"))
        });
        assert!(announced, "the sensitive state must be exposed to AT-SPI");
    }

    fn labels_of(root: &gtk::Widget) -> Vec<String> {
        descendants(root)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect()
    }
}
