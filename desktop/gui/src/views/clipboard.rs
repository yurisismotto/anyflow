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
                        eprintln!("omnibridge-gui: could not send the clipboard: {message}");
                    }
                    pages.refresh_now();
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
