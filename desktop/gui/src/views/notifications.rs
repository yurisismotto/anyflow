//! Notifications: what this computer will show, and from whom.
//!
//! # What is deliberately absent
//!
//! Any list of notifications, past or present. The desktop's own notification
//! list is where mirrored notifications live, and duplicating it inside
//! AnyFlow would create exactly the history the design forbids. There is
//! nothing to build one out of either: `NotificationsStatusReport` carries
//! counts, states and platform identifiers, and has no field that could hold a
//! title, a body or an application's name.
//!
//! # Three gates, shown as three things
//!
//! The Android side has the same problem and solves it the same way. Whether
//! a notification reaches this screen depends on three independent answers —
//! whether a notification server is reachable here, whether this computer has
//! been told to accept notifications from that device, and whether the device
//! has said it can send them — and they fail separately. N2 measured a state
//! in which two were satisfied and the third was not, and the honest symptom
//! was a role count of zero. [`Readiness`] is what lets this page say that
//! rather than draw a switch that is either on or off.

use anyflow_control::{
    NotificationPeerReport, NotificationSetting, NotificationsStatusReport, Request, Response,
};
use gtk::prelude::*;

use super::Pages;
use crate::widgets::{self, Status, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};

/// What is actually happening for one device, resolved from every gate.
///
/// A plain enum with a pure constructor, so the decision table is a unit test
/// rather than something inspected on screen. Every variant has a different
/// cause and a different fix, and no variant claims to be mirroring unless it
/// is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// The pairing itself was withdrawn. Every setting below is moot.
    Revoked,
    /// No notification server on this session, so this desktop announces no
    /// `SINK` role at all and nothing would be shown for any device.
    NoBackend,
    /// `notifications.v1` is not granted to this device here.
    NotGranted,
    /// Granted, and displaying is switched off.
    Paused,
    /// Set up, and there is no session to the device right now.
    NotConnected,
    /// Connected, and the device has not said it can send notifications —
    /// usually because it has not granted `notifications.v1` on its own side.
    PeerNotSourcing,
    /// All three gates open.
    Ready,
}

impl Readiness {
    /// Resolves the state for one device.
    ///
    /// The order is the specification: what is broken for every device first,
    /// then what a person decided about this one, then what the two machines
    /// are doing. `available` comes first because a desktop with no
    /// notification server cannot show anything from anybody, and reporting a
    /// per-device cause for a machine-wide fault sends people to the wrong
    /// screen.
    pub fn of(peer: &NotificationPeerReport, available: bool) -> Readiness {
        if peer.revoked {
            Readiness::Revoked
        } else if !available {
            Readiness::NoBackend
        } else if !peer.granted {
            Readiness::NotGranted
        } else if !peer.allow_mirror {
            Readiness::Paused
        } else if !peer.connected {
            Readiness::NotConnected
        } else if !peer.peer_is_source {
            Readiness::PeerNotSourcing
        } else {
            Readiness::Ready
        }
    }

    /// The word on the badge. Never a colour alone — see [`widgets::Status`].
    pub fn label(self) -> &'static str {
        match self {
            Readiness::Revoked => "Revoked",
            Readiness::NoBackend => "Unavailable",
            Readiness::NotGranted => "Off",
            Readiness::Paused => "Paused",
            Readiness::NotConnected => "Not connected",
            Readiness::PeerNotSourcing => "Device is not sending",
            Readiness::Ready => "Ready",
        }
    }

    /// The sentence under it, which is the part that makes it actionable.
    pub fn detail(self) -> &'static str {
        match self {
            Readiness::Revoked => {
                "This device is no longer trusted and cannot connect until it pairs again."
            }
            Readiness::NoBackend => {
                "No notification server is reachable on this desktop session, so nothing \
                 can be shown here."
            }
            Readiness::NotGranted => {
                "This computer does not accept notifications from this device."
            }
            Readiness::Paused => {
                "Accepted, and paused. Nothing is shown until this is switched back on."
            }
            Readiness::NotConnected => {
                "Set up and waiting. Notifications appear once the device connects."
            }
            Readiness::PeerNotSourcing => {
                "Connected, and the device has not said it can send notifications. Check \
                 that AnyFlow on the device shares notifications with this computer, and \
                 that Android has given it notification access. If you have just changed \
                 either, the two may need to reconnect before it takes effect."
            }
            Readiness::Ready => {
                "Notifications from the apps chosen on the device appear on this desktop."
            }
        }
    }

    /// The dot and icon. Only [`Readiness::Ready`] is drawn as working.
    pub fn status(self) -> Status {
        match self {
            Readiness::Ready => Status::Connected,
            Readiness::Revoked => Status::Revoked,
            Readiness::NotGranted | Readiness::Paused => Status::Disconnected,
            Readiness::NotConnected => Status::Available,
            Readiness::NoBackend | Readiness::PeerNotSourcing => Status::Warning,
        }
    }

    /// Whether this desktop is showing this device's notifications right now.
    pub fn is_ready(self) -> bool {
        matches!(self, Readiness::Ready)
    }
}

/// What dismissal synchronisation is actually doing for one device.
///
/// # Why this is not folded into [`Readiness`]
///
/// Mirroring and dismissal sync are two features with four gates between them,
/// and they fail apart. The commonest real state is `Ready` mirroring beside
/// unavailable dismissal — a phone running a build that announces `SOURCE` but
/// not `DISMISS_TARGET`, or a desktop session whose notification server cannot
/// report closes. Collapsing them into one "Ready" bit would say the wrong
/// thing about whichever half was worse, and a person would either think their
/// phone is being cleared when it is not, or think mirroring is broken when it
/// is not.
///
/// So it is a second pure function with its own decision table, and its own
/// unit test enumerating every variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissReadiness {
    /// The setting is off, which is the default and is not a fault.
    Off,
    /// On, and this desktop's notification server cannot tell a human
    /// dismissal from a banner timing out — so this desktop announces no
    /// `DISMISS_REPORTER` and asks nothing of anybody.
    NoReporting,
    /// On, and there is no session to the device right now.
    NotConnected,
    /// On, connected, and the device has not said it will act on a dismissal.
    PeerCannotDismiss,
    /// On, and working.
    Active,
}

impl DismissReadiness {
    /// Resolves the state for one device.
    ///
    /// The order is the specification. `Off` comes before every capability
    /// question because when the setting is off there is nothing to fix and
    /// nothing to warn about — telling somebody their notification server
    /// cannot report closes, about a feature they have not enabled, is noise.
    /// Then this machine, then the network, then the far end.
    pub fn of(peer: &NotificationPeerReport) -> DismissReadiness {
        if !peer.allow_dismiss_sync || !peer.allow_mirror {
            // `allow_mirror` off makes the flag inert whatever it says — the
            // same containment rule `NotificationPolicy::may_sync_dismissals`
            // applies in the daemon, mirrored here so the two cannot drift.
            DismissReadiness::Off
        } else if !peer.local_reports_dismissals {
            DismissReadiness::NoReporting
        } else if !peer.connected {
            DismissReadiness::NotConnected
        } else if !peer.peer_is_dismiss_target {
            DismissReadiness::PeerCannotDismiss
        } else {
            DismissReadiness::Active
        }
    }

    /// The sentence under the switch, which is the part that makes it
    /// actionable — and, for `Off`, the part that says what turning it on
    /// would do to the phone.
    pub fn detail(self) -> &'static str {
        match self {
            DismissReadiness::Off => {
                "Off. When this is on, dismissing a mirrored notification here also \
                 dismisses the original on the device. Nothing else is sent: AnyFlow \
                 cannot press a notification's buttons, reply to it, open an app, or \
                 clear everything at once."
            }
            DismissReadiness::NoReporting => {
                "On, and this desktop cannot act on it. The notification server here \
                 does not report why a notification closed, so AnyFlow cannot tell a \
                 dismissal from a banner timing out — and it will never guess."
            }
            DismissReadiness::NotConnected => {
                "On. Dismissals will be sent once the device connects."
            }
            DismissReadiness::PeerCannotDismiss => {
                "On, and the device has not said it will act on a dismissal. Allow it \
                 on the device as well, under its notification settings for this \
                 computer. If you have just changed something there, the two may need \
                 to reconnect before it takes effect."
            }
            DismissReadiness::Active => {
                "On. Dismissing a mirrored notification here dismisses the original on \
                 the device. Notifications the device marks as ongoing or \
                 non-dismissible are refused there, and stay."
            }
        }
    }

    /// Whether this state needs the person to do something. `Off` does not:
    /// it is a choice, not a fault.
    pub fn needs_attention(self) -> bool {
        matches!(
            self,
            DismissReadiness::NoReporting | DismissReadiness::PeerCannotDismiss
        )
    }
}

/// The three lock policies, as the control protocol names them.
///
/// The wire values are `full`, `app-only` and `suppress`; the words beside
/// them are what a person reads. Keeping the pair together here is what stops
/// a label drifting from the value it sets.
const LOCK_POLICIES: [(&str, &str, &str); 3] = [
    (
        "full",
        "Show full content",
        "Titles and text appear on the lock screen as usual.",
    ),
    (
        "app-only",
        "Show app only",
        "The notification names the app and nothing else. The text is removed before it \
         reaches the notification server, so there is nothing on the locked screen to read.",
    ),
    (
        "suppress",
        "Do not show",
        "Nothing at all is shown while this computer is locked, and anything already on \
         screen is closed.",
    ),
];

pub fn render(container: &gtk::Box, state: &DaemonState, pages: &Pages) {
    widgets::clear(container);
    container.append(&widgets::title("Notifications"));

    let Some(report) = &state.notifications else {
        container.append(&widgets::body_muted("Waiting for the daemon…"));
        return;
    };

    if !report.enabled {
        container.append(&widgets::security_notice(
            "notifications.v1 is not registered",
            "This daemon was built or started without the notification capability.",
            true,
        ));
        return;
    }

    container.append(&this_computer(report));

    if report.peers.is_empty() {
        container.append(&widgets::body_muted(
            "No device is paired yet. Pairing one puts it here, with notifications off \
             until you allow them.",
        ));
    } else {
        container.append(&widgets::section_label("Devices"));
        for peer in &report.peers {
            container.append(&peer_card(peer, report.available, pages));
        }
    }

    container.append(&widgets::security_notice(
        "Nothing is kept",
        "AnyFlow keeps no notification history. A mirrored notification exists on this \
         desktop's own notification list and nowhere else — nothing about it reaches a \
         log, a database or a file, and closing it here leaves nothing behind.",
        false,
    ));
}

/// What this machine can do at all: the server, and the lock detector.
fn this_computer(report: &NotificationsStatusReport) -> gtk::Box {
    let card = widgets::card();
    card.append(&widgets::section_label("This computer"));

    let row = widgets::row(SPACING_SM);
    row.append(&widgets::icon_tile(
        "preferences-system-notifications-symbolic",
        if report.available {
            "af-tile-violet"
        } else {
            "af-tile-neutral"
        },
    ));
    let text = widgets::column(2);
    text.append(&widgets::subtitle(&report.backend_detail));
    text.append(&widgets::caption(&report.backend));
    text.set_hexpand(true);
    row.append(&text);
    row.append(&widgets::status_badge(if report.available {
        Status::Connected
    } else {
        Status::Warning
    }));
    card.append(&row);

    if !report.available {
        card.append(&widgets::caption(
            "No notification server answered. Until one does, this computer tells every \
             device that it cannot display notifications, so none are sent.",
        ));
    }

    card.append(&widgets::separator());
    // Deliberately not "logind LockedHint": which interface answers the
    // question belongs in diagnostics, not on the page where somebody decides
    // what appears on their lock screen.
    card.append(&widgets::body(if report.locked {
        "This computer is locked."
    } else {
        "This computer is unlocked."
    }));
    card.append(&widgets::caption(
        "Lock state is read from this desktop session. If it cannot be read, AnyFlow \
         treats the session as locked.",
    ));
    card.append(&widgets::caption(&format!(
        "Showing {} notification{} now.",
        report.mirrors,
        if report.mirrors == 1 { "" } else { "s" }
    )));
    card
}

fn peer_card(peer: &NotificationPeerReport, available: bool, pages: &Pages) -> gtk::Box {
    let card = widgets::card();
    let readiness = Readiness::of(peer, available);

    let head = widgets::row(SPACING_SM);
    head.append(&widgets::icon_tile("phone-symbolic", "af-tile-blue"));
    let text = widgets::column(2);
    text.append(&widgets::subtitle(&peer.device_name));
    let fp = widgets::caption(&peer.fingerprint_short);
    fp.add_css_class("af-mono");
    text.append(&fp);
    text.set_hexpand(true);
    head.append(&text);
    head.append(&widgets::status_badge(readiness.status()));
    card.append(&head);
    card.append(&widgets::caption(readiness.detail()));

    if peer.revoked {
        return card;
    }

    card.append(&widgets::separator());

    // Gate 2, and it is the one this side owns.
    card.append(&receive_switch(peer, pages));

    if !peer.granted {
        card.append(&widgets::caption(
            "Every setting below would be inert without this, so none is offered.",
        ));
        return card;
    }

    card.append(&policy_switch(
        "Show notifications",
        "Pause without changing anything else. Turning this off closes what is on \
         screen now.",
        peer.allow_mirror,
        &peer.device_id,
        NotificationSetting::Mirror {
            enabled: !peer.allow_mirror,
        },
        pages,
    ));

    card.append(&widgets::separator());
    card.append(&widgets::section_label("When this computer is locked"));
    card.append(&lock_choices(peer, pages));
    card.append(&widgets::caption(
        "Unlocking does not bring back text that was withheld: AnyFlow never kept it. \
         The next update from the app arrives in full.",
    ));

    card.append(&widgets::separator());
    card.append(&widgets::section_label("Dismissing"));
    card.append(&dismiss_sync_row(peer, pages));

    card.append(&widgets::caption(&format!(
        "{} showing of {} mirrored · this computer announced {} role{} (epoch {}) · the \
         device {} (epoch {})",
        peer.displayed,
        peer.mirrors,
        peer.local_roles,
        if peer.local_roles == 1 { "" } else { "s" },
        peer.local_epoch,
        if peer.peer_is_source {
            "can send notifications"
        } else {
            "claims no source role"
        },
        peer.peer_epoch,
    )));
    card
}

/// The grant itself, which is a capability grant and not a notification
/// setting — so it is written through `Grant`, the same request the Trusted
/// peers page uses. There is no second policy store.
fn receive_switch(peer: &NotificationPeerReport, pages: &Pages) -> gtk::Box {
    let row = widgets::row(SPACING_SM);
    let text = widgets::column(0);
    text.append(&widgets::body("Receive notifications from this device"));
    text.append(&widgets::caption(
        "Off until you allow it. Allowing it does not change any other permission, and \
         turning it off closes the notifications already on screen.",
    ));
    text.set_hexpand(true);
    row.append(&text);

    let sw = gtk::Switch::new();
    sw.set_active(peer.granted);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(
        "Receive notifications from this device",
    )]);
    let device = peer.device_id.clone();
    let pages = pages.clone();
    sw.connect_state_set(move |_, wanted| {
        let pages = pages.clone();
        client::send(
            Request::Grant {
                device: device.clone(),
                capability: "notifications.v1".into(),
                granted: wanted,
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("anyflow-gui: the daemon refused the grant change: {message}");
                }
                // Re-read rather than assume: the daemon is the authority, and
                // a refused change must not leave a switch claiming otherwise.
                pages.refresh_now();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);
    row
}

fn policy_switch(
    title: &str,
    description: &str,
    active: bool,
    device: &str,
    setting: NotificationSetting,
    pages: &Pages,
) -> gtk::Box {
    let row = widgets::row(SPACING_SM);
    let text = widgets::column(0);
    text.append(&widgets::body(title));
    text.append(&widgets::caption(description));
    text.set_hexpand(true);
    row.append(&text);

    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(title)]);

    let device = device.to_string();
    let pages = pages.clone();
    sw.connect_state_set(move |_, wanted| {
        let pages = pages.clone();
        let setting = match &setting {
            NotificationSetting::Mirror { .. } => NotificationSetting::Mirror { enabled: wanted },
            other => other.clone(),
        };
        client::send(
            Request::NotificationsPolicy {
                device: device.clone(),
                setting,
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("anyflow-gui: the daemon refused the policy change: {message}");
                }
                pages.refresh_now();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);
    row
}

/// The three lock policies, as one exclusive choice.
///
/// A single-selection `GtkListBox`, which is the same widget the window's own
/// sidebar uses — and deliberately **not** three grouped `GtkCheckButton`s.
/// Grouping is the obvious way to write this and it is the reason this
/// function exists: on GTK 4.22 calling `gtk_check_button_set_group` anywhere
/// in a page stopped the whole application registering with the AT-SPI
/// registry. Every individual widget still exposed its label and its state, so
/// nothing looked wrong — but no screen reader could reach the application at
/// all. Measured by bisecting this page against a build without it; a list box
/// registers cleanly and brings keyboard navigation and a selection state with
/// it.
///
/// Exclusivity is therefore a property of the *data*: the page is redrawn from
/// the daemon on every refresh, and the daemon holds exactly one policy.
fn lock_choices(peer: &NotificationPeerReport, pages: &Pages) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("af-choice");

    for (_, title, description) in LOCK_POLICIES {
        let text = widgets::column(0);
        text.append(&widgets::body(title));
        text.append(&widgets::caption(description));
        let row = gtk::ListBoxRow::builder().child(&text).build();
        // Announced once, as a phrase: the description below the title is
        // detail, and repeating it as a second label would make every option
        // take a paragraph to read out.
        row.update_property(&[gtk::accessible::Property::Label(title)]);
        list.append(&row);
    }

    // Selected *before* the handler is connected, so restoring the daemon's
    // own value cannot look like the operator choosing it — otherwise every
    // refresh would write the policy back and the log would fill with changes
    // nobody made.
    let selected = LOCK_POLICIES
        .iter()
        .position(|(value, _, _)| *value == peer.when_locked)
        // Not a silent fallback to the first option: an unrecognised stored
        // value selects nothing, which is visibly wrong rather than quietly
        // wrong.
        .map(|i| i as i32);
    if let Some(index) = selected {
        list.select_row(list.row_at_index(index).as_ref());
    }

    let device = peer.device_id.clone();
    let pages = pages.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else { return };
        let Some((value, _, _)) = LOCK_POLICIES.get(row.index().max(0) as usize) else {
            return;
        };
        let pages = pages.clone();
        client::send(
            Request::NotificationsPolicy {
                device: device.clone(),
                setting: NotificationSetting::WhenLocked {
                    policy: (*value).to_string(),
                },
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("anyflow-gui: the daemon refused the lock policy: {message}");
                }
                pages.refresh_now();
            },
        );
    });
    list
}

/// Dismissal sync: the one control on this page that does something to the
/// phone.
///
/// A real switch as of N4, and the copy is written accordingly. It names the
/// effect on the *other* device rather than describing a preference, and it
/// says what dismissal sync is **not** — because "let this computer dismiss
/// notifications" is exactly the phrase a person could read as "let this
/// computer control my phone's notifications", and it is not that.
///
/// The state below the switch is [`DismissReadiness`], not a second switch:
/// when the peer has not announced `DISMISS_TARGET`, or this desktop cannot
/// report human dismissals, the switch still writes the stored policy — it is
/// the person's choice and it should persist — and the line underneath says
/// plainly that nothing will happen yet, and what would change that.
fn dismiss_sync_row(peer: &NotificationPeerReport, pages: &Pages) -> gtk::Box {
    const TITLE: &str = "Let this computer dismiss notifications on the device";
    let state = DismissReadiness::of(peer);

    let row = widgets::row(SPACING_SM);
    let text = widgets::column(0);
    text.append(&widgets::body(TITLE));
    text.append(&widgets::caption(state.detail()));
    if peer.dismissals_sent > 0 || peer.dismissals_refused > 0 {
        // Counts, and only counts. There is no field on the report that could
        // name a notification, and no list of dismissals exists anywhere.
        text.append(&widgets::caption(&format!(
            "{} dismissal{} sent since this daemon started{}.",
            peer.dismissals_sent,
            if peer.dismissals_sent == 1 { "" } else { "s" },
            if peer.dismissals_refused > 0 {
                format!(", {} declined by the device", peer.dismissals_refused)
            } else {
                String::new()
            }
        )));
    }
    text.set_hexpand(true);
    row.append(&text);

    let sw = gtk::Switch::new();
    sw.set_active(peer.allow_dismiss_sync);
    sw.set_valign(gtk::Align::Center);
    sw.update_property(&[gtk::accessible::Property::Label(TITLE)]);

    let device = peer.device_id.clone();
    let pages = pages.clone();
    sw.connect_state_set(move |_, wanted| {
        let pages = pages.clone();
        client::send(
            Request::NotificationsPolicy {
                device: device.clone(),
                // Exactly one field. This request cannot reach the grant, the
                // mirror switch or the lock policy — `NotificationSetting` has
                // a separate variant for each and this one carries a single
                // boolean.
                setting: NotificationSetting::DismissSync { enabled: wanted },
            },
            move |reply| {
                if let Ok(Response::Error { message }) = reply {
                    eprintln!("anyflow-gui: the daemon refused the policy change: {message}");
                }
                // Re-read rather than assume: the daemon is the authority.
                pages.refresh_now();
            },
        );
        gtk::glib::Propagation::Proceed
    });
    row.append(&sw);

    let column = widgets::column(SPACING_XS);
    column.append(&row);
    if state.needs_attention() {
        column.append(&widgets::status_badge(Status::Warning));
    }
    column
}

#[cfg(test)]
mod tests {
    use super::{render, DismissReadiness, Readiness, LOCK_POLICIES};
    use crate::{DaemonState, Page};
    use anyflow_control::{NotificationPeerReport, NotificationsStatusReport};
    use gtk::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A peer with every gate open. Each test shuts exactly one.
    fn peer() -> NotificationPeerReport {
        NotificationPeerReport {
            device_id: "d".into(),
            device_name: "Tablet".into(),
            fingerprint_short: "7E63 7B4E 937B 7732".into(),
            granted: true,
            revoked: false,
            connected: true,
            allow_mirror: true,
            when_locked: "app-only".into(),
            allow_dismiss_sync: false,
            mirrors: 4,
            displayed: 4,
            evicted: 0,
            local_roles: 1,
            local_epoch: 1,
            peer_is_source: true,
            peer_is_dismiss_target: true,
            peer_epoch: 2,
            local_reports_dismissals: true,
            dismissals_sent: 0,
            dismissals_refused: 0,
            snapshot_open: false,
            queued: 0,
            coalesced: 0,
            dropped: 0,
        }
    }

    #[test]
    fn every_gate_open_is_ready() {
        assert_eq!(Readiness::of(&peer(), true), Readiness::Ready);
        assert!(Readiness::of(&peer(), true).is_ready());
    }

    #[test]
    fn a_missing_notification_server_is_reported_once_and_not_per_device() {
        // Machine-wide, so it must not be dressed up as something about this
        // device — that would send somebody to check a grant that is fine.
        assert_eq!(Readiness::of(&peer(), false), Readiness::NoBackend);
    }

    #[test]
    fn a_revoked_device_outranks_everything_including_a_dead_backend() {
        let mut p = peer();
        p.revoked = true;
        assert_eq!(Readiness::of(&p, false), Readiness::Revoked);
    }

    #[test]
    fn an_ungranted_device_is_off_rather_than_broken() {
        let mut p = peer();
        p.granted = false;
        assert_eq!(Readiness::of(&p, true), Readiness::NotGranted);
        assert!(!Readiness::of(&p, true).is_ready());
    }

    #[test]
    fn mirroring_switched_off_is_paused_and_not_off() {
        // Different cause, different fix: one is a capability grant, the other
        // is a switch on this page.
        let mut p = peer();
        p.allow_mirror = false;
        assert_eq!(Readiness::of(&p, true), Readiness::Paused);
    }

    #[test]
    fn a_configured_device_with_no_session_is_not_connected() {
        let mut p = peer();
        p.connected = false;
        assert_eq!(Readiness::of(&p, true), Readiness::NotConnected);
    }

    #[test]
    fn a_connected_device_that_claims_no_source_role_is_named_as_such() {
        // The state N2 measured cross-device: this desktop granted and
        // announcing SINK, the tablet's own grant absent, so it announced
        // roles=0 and nothing moved. A single switch cannot describe it.
        let mut p = peer();
        p.peer_is_source = false;
        assert_eq!(Readiness::of(&p, true), Readiness::PeerNotSourcing);
        assert!(!Readiness::of(&p, true).is_ready());
    }

    #[test]
    fn only_ready_claims_to_be_showing_notifications() {
        for readiness in [
            Readiness::Revoked,
            Readiness::NoBackend,
            Readiness::NotGranted,
            Readiness::Paused,
            Readiness::NotConnected,
            Readiness::PeerNotSourcing,
        ] {
            assert!(!readiness.is_ready(), "{readiness:?}");
        }
        assert!(Readiness::Ready.is_ready());
    }

    #[test]
    fn every_state_has_a_word_and_a_sentence() {
        // Nothing is communicated by colour alone, and no state is left with a
        // badge and no explanation of what to do about it.
        for readiness in [
            Readiness::Revoked,
            Readiness::NoBackend,
            Readiness::NotGranted,
            Readiness::Paused,
            Readiness::NotConnected,
            Readiness::PeerNotSourcing,
            Readiness::Ready,
        ] {
            assert!(!readiness.label().is_empty(), "{readiness:?}");
            assert!(readiness.detail().len() > 20, "{readiness:?}");
        }
    }

    // -----------------------------------------------------------------------
    // Dismissal readiness: its own table, because it fails apart from mirroring
    // -----------------------------------------------------------------------

    #[test]
    fn dismiss_sync_is_off_in_the_state_a_fresh_grant_leaves_behind() {
        // The fixture has every capability gate open and the setting off,
        // which is exactly what granting `notifications.v1` produces.
        assert_eq!(DismissReadiness::of(&peer()), DismissReadiness::Off);
    }

    #[test]
    fn switching_it_on_with_everything_working_is_active() {
        let mut p = peer();
        p.allow_dismiss_sync = true;
        assert_eq!(DismissReadiness::of(&p), DismissReadiness::Active);
    }

    /// The state §16 exists for: mirroring is Ready and dismissal is not.
    /// A single "Ready" bit would have to lie about one of them.
    #[test]
    fn mirroring_can_be_ready_while_dismissal_sync_is_not() {
        let mut p = peer();
        p.allow_dismiss_sync = true;
        p.peer_is_dismiss_target = false;
        assert_eq!(Readiness::of(&p, true), Readiness::Ready);
        assert_eq!(
            DismissReadiness::of(&p),
            DismissReadiness::PeerCannotDismiss
        );
        assert!(DismissReadiness::of(&p).needs_attention());
    }

    #[test]
    fn a_desktop_that_cannot_report_closes_says_so_rather_than_blaming_the_device() {
        let mut p = peer();
        p.allow_dismiss_sync = true;
        p.local_reports_dismissals = false;
        // Both are wrong at once here; the machine-wide cause is named first,
        // because sending somebody to change a setting on their phone for a
        // fault on this desktop is the wrong instruction.
        p.peer_is_dismiss_target = false;
        assert_eq!(DismissReadiness::of(&p), DismissReadiness::NoReporting);
    }

    #[test]
    fn an_offline_device_is_not_connected_rather_than_unable() {
        let mut p = peer();
        p.allow_dismiss_sync = true;
        p.connected = false;
        assert_eq!(DismissReadiness::of(&p), DismissReadiness::NotConnected);
        assert!(
            !DismissReadiness::of(&p).needs_attention(),
            "a device that is merely away needs nothing doing"
        );
    }

    #[test]
    fn turning_mirroring_off_makes_a_stale_dismiss_flag_read_as_off() {
        // The same containment rule the daemon applies in
        // `NotificationPolicy::may_sync_dismissals`, mirrored here so the page
        // cannot claim a dismissal will happen that the daemon will refuse.
        let mut p = peer();
        p.allow_dismiss_sync = true;
        p.allow_mirror = false;
        assert_eq!(DismissReadiness::of(&p), DismissReadiness::Off);
    }

    #[test]
    fn off_is_a_choice_and_never_needs_attention() {
        assert!(!DismissReadiness::Off.needs_attention());
        assert!(!DismissReadiness::Active.needs_attention());
    }

    #[test]
    fn every_dismiss_state_says_what_it_means_and_what_it_does_to_the_phone() {
        for state in [
            DismissReadiness::Off,
            DismissReadiness::NoReporting,
            DismissReadiness::NotConnected,
            DismissReadiness::PeerCannotDismiss,
            DismissReadiness::Active,
        ] {
            assert!(state.detail().len() > 20, "{state:?}");
        }
        // The copy must name the effect on the other device, and must not
        // imply anything wider than one dismissal.
        let off = DismissReadiness::Off.detail();
        assert!(off.contains("dismisses the original on the device"));
        for forbidden in ["reply", "buttons", "open an app", "clear everything"] {
            assert!(
                off.contains(forbidden),
                "the copy must say what this is NOT: {forbidden}"
            );
        }
    }

    #[test]
    fn the_lock_choices_are_exactly_the_control_protocols_values() {
        // A label that drifted from its wire value would silently set the
        // wrong policy, and the daemon would accept it.
        let values: Vec<&str> = LOCK_POLICIES.iter().map(|(v, _, _)| *v).collect();
        assert_eq!(values, vec!["full", "app-only", "suppress"]);
        for (_, title, description) in LOCK_POLICIES {
            assert!(!title.is_empty());
            assert!(description.len() > 20);
        }
    }

    // -----------------------------------------------------------------------
    // The page itself, built from a fabricated report
    // -----------------------------------------------------------------------
    //
    // These need a display, so they are `#[ignore]`d and asked for by name —
    // the same convention the notification capability's `real_dbus` gate uses.
    // They are not screenshots: what they assert is that the control a person
    // reaches for exists, carries a label an assistive technology can read,
    // and shows the state the daemon actually holds.
    //
    //   cargo test -p anyflow-gui -- --ignored --test-threads=1

    fn report(peers: Vec<NotificationPeerReport>, available: bool) -> NotificationsStatusReport {
        NotificationsStatusReport {
            enabled: true,
            backend: "freedesktop".into(),
            backend_detail: "gnome-shell 50.4 (spec 1.2, GNOME)".into(),
            available,
            body_markup: true,
            persistence: true,
            lock_source: "logind".into(),
            lock_detail: "LockedHint on /org/freedesktop/login1/session/_32".into(),
            locked: false,
            mirrors: peers.iter().map(|p| p.mirrors).sum(),
            peers,
        }
    }

    /// Renders the page into a fresh container and returns it.
    fn page(report: NotificationsStatusReport) -> gtk::Box {
        // Safe to call repeatedly; the tests run single-threaded.
        if !gtk::is_initialized() {
            gtk::init().expect("a display is needed: run with --ignored on a desktop session");
        }
        let state = Rc::new(RefCell::new(DaemonState {
            notifications: Some(report),
            ..DaemonState::default()
        }));
        let stack = gtk::Stack::new();
        let pages = super::Pages::new(&stack, state.clone());
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let borrowed = state.borrow();
        render(&container, &borrowed, &pages);
        // Keep the stack alive for as long as the page is inspected.
        let _ = Page::Notifications;
        let _ = stack;
        container
    }

    /// Every widget under `root`, depth first.
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

    fn switches(root: &gtk::Box) -> Vec<gtk::Switch> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Switch>().ok())
            .collect()
    }

    fn labels(root: &gtk::Box) -> Vec<String> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect()
    }

    /// Every widget-tree assertion, in one test.
    ///
    /// One rather than nine because GTK records the thread that initialised it
    /// and refuses every later call from another, and the test harness gives
    /// each `#[test]` a thread of its own. Splitting them would mean nine
    /// processes or nine skipped tests; this way each section still fails with
    /// its own name and message.
    #[test]
    #[ignore = "needs a display: cargo test -p anyflow-gui -- --ignored"]
    fn the_notifications_page_widget_tree() {
        an_ungranted_device_is_offered_the_grant_and_nothing_else();
        the_grant_switch_shows_what_the_daemon_holds();
        a_granted_device_is_offered_the_pause_and_the_lock_policy();
        the_lock_policy_is_one_exclusive_choice_with_the_stored_value_selected();
        an_unrecognised_stored_policy_selects_nothing_rather_than_the_first_option();
        dismissal_sync_is_a_real_switch_that_starts_off();
        the_dismissal_switch_shows_what_the_daemon_holds();
        an_unavailable_dismissal_state_is_truthful_rather_than_silent();
        the_dismissal_switch_is_not_offered_to_an_ungranted_device();
        every_switch_is_announced_as_a_switch_beside_words_that_name_it();
        a_desktop_with_no_notification_server_says_so_once();
        the_page_never_renders_a_notification();

        // The §18 re-render defect. These drive `Pages::render` rather than
        // `render`, because what they are about is *when* a page is rebuilt.
        an_unchanged_refresh_leaves_the_control_it_found_alone();
        an_activation_after_many_refreshes_still_reaches_the_handler();
        the_switch_is_still_activatable_from_the_keyboard_after_refreshes();
        the_dismiss_switch_survives_refreshes_and_still_follows_the_daemon();
        a_real_change_still_redraws_the_page();
        a_change_on_another_page_does_not_disturb_this_one();
    }

    fn an_ungranted_device_is_offered_the_grant_and_nothing_else() {
        let mut p = peer();
        p.granted = false;
        let container = page(report(vec![p], true));

        // Exactly one control: the grant. Every setting below it would be
        // inert, and offering an inert switch is how a person comes to believe
        // something is configured when it is not.
        assert_eq!(switches(&container).len(), 1);
        let text = labels(&container).join("\n");
        assert!(text.contains("Receive notifications from this device"));
        assert!(!text.contains("When this computer is locked"));
        assert!(text.contains("Every setting below would be inert"));
    }

    fn the_grant_switch_shows_what_the_daemon_holds() {
        let mut off = peer();
        off.granted = false;
        assert!(!switches(&page(report(vec![off], true)))[0].is_active());
        assert!(switches(&page(report(vec![peer()], true)))[0].is_active());
    }

    fn a_granted_device_is_offered_the_pause_and_the_lock_policy() {
        let container = page(report(vec![peer()], true));
        let sw = switches(&container);
        assert_eq!(
            sw.len(),
            3,
            "the grant, the pause, and — as of N4 — dismissal sync"
        );
        assert!(sw[1].is_active(), "mirroring is on for this fixture");

        let text = labels(&container).join("\n");
        assert!(text.contains("When this computer is locked"));
        for (_, title, _) in LOCK_POLICIES {
            assert!(text.contains(title), "{title} is not offered");
        }
    }

    fn the_lock_policy_is_one_exclusive_choice_with_the_stored_value_selected() {
        let mut p = peer();
        p.when_locked = "suppress".into();
        let container = page(report(vec![p], true));

        let lists: Vec<gtk::ListBox> = descendants(container.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
            .collect();
        assert_eq!(lists.len(), 1, "one choice, not three independent boxes");
        assert_eq!(lists[0].selection_mode(), gtk::SelectionMode::Single);

        let selected = lists[0].selected_row().expect("a policy is selected");
        assert_eq!(selected.index(), 2, "'Do not show' is the stored value");
    }

    fn an_unrecognised_stored_policy_selects_nothing_rather_than_the_first_option() {
        // Silently selecting "Show full content" for a value we do not
        // understand would be the most permissive possible guess.
        let mut p = peer();
        p.when_locked = "something-a-later-release-wrote".into();
        let container = page(report(vec![p], true));
        let list = descendants(container.upcast_ref())
            .into_iter()
            .find_map(|w| w.downcast::<gtk::ListBox>().ok())
            .expect("the choice list");
        assert!(list.selected_row().is_none());
    }

    fn dismissal_sync_is_a_real_switch_that_starts_off() {
        let container = page(report(vec![peer()], true));
        let sw = switches(&container);
        assert_eq!(sw.len(), 3, "the grant, the pause, and dismissal sync");
        assert!(
            !sw[2].is_active(),
            "dismissal sync must never be drawn on when it is stored off"
        );

        let text = labels(&container).join("\n");
        assert!(text.contains("Let this computer dismiss notifications on the device"));
        // The copy names the effect on the phone, and bounds it.
        assert!(text.contains("dismisses the original on the device"));
        assert!(text.contains("cannot press a notification's buttons"));
    }

    fn the_dismissal_switch_shows_what_the_daemon_holds() {
        let mut on = peer();
        on.allow_dismiss_sync = true;
        assert!(switches(&page(report(vec![on], true)))[2].is_active());
    }

    fn an_unavailable_dismissal_state_is_truthful_rather_than_silent() {
        // On, and the device cannot act on it. The switch still reflects the
        // stored choice — it is the person's — and the line underneath says
        // plainly that nothing will happen yet, and what would change that.
        let mut p = peer();
        p.allow_dismiss_sync = true;
        p.peer_is_dismiss_target = false;
        let container = page(report(vec![p], true));

        assert!(
            switches(&container)[2].is_active(),
            "a refused capability must not silently rewrite a stored setting"
        );
        let text = labels(&container).join("\n");
        assert!(text.contains("the device has not said it will act on a dismissal"));
        assert!(text.contains("may need to reconnect"));
    }

    fn the_dismissal_switch_is_not_offered_to_an_ungranted_device() {
        let mut p = peer();
        p.granted = false;
        let container = page(report(vec![p], true));
        let text = labels(&container).join("\n");
        assert!(
            !text.contains("Let this computer dismiss"),
            "a control that cannot be honoured must not be on the screen"
        );
    }

    fn every_switch_is_announced_as_a_switch_beside_words_that_name_it() {
        // A `GtkSwitch` carries no visible text of its own: the words live in
        // the row beside it, and the accessible label is set from the same
        // string. What this pins is that both exist — a switch with no words
        // anywhere near it is one nobody can act on, seeing or not.
        let container = page(report(vec![peer()], true));
        let text = labels(&container).join("\n");
        for sw in switches(&container) {
            assert_eq!(sw.accessible_role(), gtk::AccessibleRole::Switch);
        }
        assert!(text.contains("Receive notifications from this device"));
        assert!(text.contains("Show notifications"));
    }

    fn a_desktop_with_no_notification_server_says_so_once() {
        let container = page(report(vec![peer()], false));
        let text = labels(&container).join("\n");
        assert!(text.contains("No notification server answered"));
        assert!(text.contains(Readiness::NoBackend.detail()));
    }

    fn the_page_never_renders_a_notification() {
        // The report has no field that could carry one, and this asserts the
        // page cannot invent one either: everything on screen comes from the
        // fixture, and the fixture's device name is the only free text in it.
        let container = page(report(vec![peer()], true));
        let text = labels(&container).join("\n");
        assert!(text.contains("Tablet"));
        assert!(text.contains("Nothing is kept"));
        assert!(text.contains("keeps no notification history"));
    }

    // -----------------------------------------------------------------------
    // Re-rendering, and what an assistive technology needs from it
    // -----------------------------------------------------------------------
    //
    // The defect these pin, measured on hardware during the N3 desktop gate:
    // every page was rebuilt on every control-socket reply, so a control that
    // AT-SPI had located was destroyed before the activation it sent could
    // reach it. `do_action` answered `True` and nothing happened. What follows
    // asserts the property that fixes it — an unchanged poll does not touch
    // the widget tree — rather than asserting a timing.
    //
    // These live in this grouped test for the reason above it: GTK belongs to
    // the thread that initialised it, and each `#[test]` gets its own.

    /// A `Pages` wired to `state`, its Notifications page, and the stack that
    /// owns the page boxes (returned so the caller keeps it alive).
    fn live(state: Rc<RefCell<DaemonState>>) -> (super::Pages, gtk::Box, gtk::Stack) {
        if !gtk::is_initialized() {
            gtk::init().expect("a display is needed: run with --ignored on a desktop session");
        }
        let stack = gtk::Stack::new();
        let pages = super::Pages::new(&stack, state);
        let page = pages.notifications.clone();
        pages.render();
        (pages, page, stack)
    }

    fn lists(root: &gtk::Box) -> Vec<gtk::ListBox> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
            .collect()
    }

    /// Ten polls in which the daemon says exactly what it said before.
    ///
    /// A *new* report each time, equal to the last rather than the same
    /// object, so what is being tested is value equality and not that the
    /// test happened to hand back the same allocation.
    fn refresh_unchanged(pages: &super::Pages, state: &Rc<RefCell<DaemonState>>, times: usize) {
        for _ in 0..times {
            state.borrow_mut().notifications = Some(report(vec![peer()], true));
            pages.render();
        }
    }

    fn granted_state() -> Rc<RefCell<DaemonState>> {
        Rc::new(RefCell::new(DaemonState {
            notifications: Some(report(vec![peer()], true)),
            ..DaemonState::default()
        }))
    }

    fn an_unchanged_refresh_leaves_the_control_it_found_alone() {
        let state = granted_state();
        let (pages, page, _stack) = live(state.clone());

        // What an assistive technology holds after locating the controls.
        let located = switches(&page);
        let located_lists = lists(&page);
        assert_eq!(located.len(), 3, "the grant, the pause, and dismissal sync");
        assert_eq!(located_lists.len(), 1, "the lock policy");

        refresh_unchanged(&pages, &state, 10);

        let now = switches(&page);
        assert_eq!(now.len(), located.len());
        for (before, after) in located.iter().zip(&now) {
            // `GtkSwitch` is a `GObject`; equality here is object identity,
            // which is precisely the thing AT-SPI holds a reference to.
            assert_eq!(
                before, after,
                "an unchanged refresh destroyed and rebuilt the switch"
            );
        }
        assert_eq!(lists(&page), located_lists, "the lock policy was rebuilt");

        // Still in the tree, so still reachable from the accessibility tree.
        for sw in &located {
            assert!(sw.parent().is_some(), "the switch was unparented");
            assert!(sw.is_visible());
        }
        // And the selection an assistive technology would read back survived.
        assert_eq!(
            located_lists[0].selected_row().map(|r| r.index()),
            Some(1),
            "'Show app only' is the fixture's stored policy"
        );
    }

    fn an_activation_after_many_refreshes_still_reaches_the_handler() {
        let state = granted_state();
        let (pages, page, _stack) = live(state.clone());

        // A probe on the object located *before* the refreshes, standing in
        // for the reference AT-SPI keeps between locating and activating.
        let located = switches(&page)[0].clone();
        let fired = Rc::new(std::cell::Cell::new(0u32));
        {
            let fired = fired.clone();
            located.connect_state_set(move |_, _| {
                fired.set(fired.get() + 1);
                gtk::glib::Propagation::Proceed
            });
        }

        refresh_unchanged(&pages, &state, 10);

        // The activation is delivered to the object that was located. Under
        // the defect this object is no longer in the page, so the assertion
        // below is the one that fails.
        assert!(
            switches(&page).contains(&located),
            "the located switch is no longer the one on the page"
        );
        located.set_active(!located.is_active());
        assert_eq!(fired.get(), 1, "the activation reached no handler");
    }

    /// Runs the main loop until `done`, or for a second, whichever is first.
    fn pump(done: impl Fn() -> bool) -> bool {
        let ctx = gtk::glib::MainContext::default();
        for _ in 0..1000 {
            if done() {
                return true;
            }
            while ctx.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        done()
    }

    fn the_switch_is_still_activatable_from_the_keyboard_after_refreshes() {
        let state = granted_state();
        let (pages, page, stack) = live(state.clone());

        // The only test here that goes on screen, and it has to: `GtkSwitch`
        // animates its toggle, so an unmapped switch has no frame clock to
        // animate against and `activate` cannot complete. Keyboard activation
        // is exactly the path a screen-reader user takes, so it is asserted
        // against a real mapped widget rather than approximated.
        stack.set_visible_child_name(crate::Page::Notifications.name());
        // `GtkSwitch` toggles at the end of an animation, and an animation
        // needs frame callbacks the compositor is free not to send to a window
        // it is not showing. Switched off, the toggle is synchronous, which is
        // what makes this assertion about the widget rather than about the
        // compositor's scheduling.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
        let window = gtk::Window::builder().child(&stack).build();
        window.present();
        assert!(pump(|| page.is_mapped()), "the page never mapped");

        let located = switches(&page)[0].clone();
        assert!(located.is_focusable(), "a switch nobody can tab to");

        refresh_unchanged(&pages, &state, 10);

        let same = switches(&page)[0].clone();
        assert_eq!(located, same, "a refresh replaced the mapped switch");
        assert!(same.is_focusable(), "focusability was lost");
        assert!(same.is_sensitive(), "the switch cannot be activated");
        assert!(same.grab_focus(), "the switch cannot take focus");

        // `activate` is what the Space/Enter keybinding runs.
        let before = same.is_active();
        assert!(same.activate(), "the switch has no activate action");
        assert!(
            pump(|| same.is_active() != before),
            "keyboard activation did not change the switch"
        );

        window.destroy();
        pump(|| false);
    }

    /// N4's own half of the §32 property, at the shipped `REFRESH_SECS = 2`
    /// cadence: an unchanged dismiss policy leaves the switch alone, and a
    /// changed one is shown.
    ///
    /// The first half is what a screen reader needs — a switch destroyed and
    /// rebuilt five times every two seconds is one an AT-SPI activation can
    /// never land on, which is the N3 defect this page was fixed for. The
    /// second half is what stops that fix becoming "never redraw", which would
    /// leave a person looking at a switch that disagrees with the daemon about
    /// something that dismisses notifications on their phone.
    fn the_dismiss_switch_survives_refreshes_and_still_follows_the_daemon() {
        let state = granted_state();
        let (pages, page, _stack) = live(state.clone());

        let located = switches(&page)[2].clone();
        assert!(!located.is_active(), "the fixture stores it off");

        refresh_unchanged(&pages, &state, 10);
        assert!(
            switches(&page).contains(&located),
            "an unchanged refresh rebuilt the dismissal switch; AT-SPI would \
             be holding a widget that is no longer on the page"
        );
        assert!(located.parent().is_some());

        // The daemon now says it is on. The page must show that, and the
        // caption must change with it.
        let mut changed = peer();
        changed.allow_dismiss_sync = true;
        state.borrow_mut().notifications = Some(report(vec![changed], true));
        pages.render();

        assert!(
            switches(&page)[2].is_active(),
            "a real dismiss-policy change did not reach the page"
        );
        assert!(labels(&page)
            .join("\n")
            .contains("dismisses the original on the device"));
    }

    fn a_real_change_still_redraws_the_page() {
        // The other half of the property: not rebuilding an unchanged tree
        // must not become not rebuilding at all.
        let state = granted_state();
        let (pages, page, _stack) = live(state.clone());
        refresh_unchanged(&pages, &state, 3);
        assert_eq!(
            lists(&page)[0].selected_row().map(|r| r.index()),
            Some(1),
            "'Show app only'"
        );

        let mut changed = peer();
        changed.when_locked = "suppress".into();
        state.borrow_mut().notifications = Some(report(vec![changed], true));
        pages.render();

        assert_eq!(
            lists(&page)[0].selected_row().map(|r| r.index()),
            Some(2),
            "a real policy change did not reach the page"
        );

        // And a change that removes controls removes them.
        let mut ungranted = peer();
        ungranted.granted = false;
        state.borrow_mut().notifications = Some(report(vec![ungranted], true));
        pages.render();
        assert_eq!(
            switches(&page).len(),
            1,
            "the pause survived losing the grant"
        );
    }

    fn a_change_on_another_page_does_not_disturb_this_one() {
        // The fix is per page, not per application: a transfer starting on
        // the Files page must not cost the Notifications page its controls.
        // This is what makes it an architecture fix rather than a special
        // case for one switch.
        let state = granted_state();
        let (pages, page, _stack) = live(state.clone());
        let located = switches(&page);

        state.borrow_mut().transfers = Some(vec![transfer()]);
        pages.render();
        state.borrow_mut().transfers = Some(Vec::new());
        pages.render();

        let now = switches(&page);
        assert_eq!(now.len(), located.len());
        for (before, after) in located.iter().zip(&now) {
            assert_eq!(before, after, "another page's change rebuilt this one");
        }
    }

    fn transfer() -> anyflow_control::TransferReport {
        anyflow_control::TransferReport {
            transfer_id: "t".into(),
            device_name: "Tablet".into(),
            fingerprint_short: "7E63 7B4E 937B 7732".into(),
            direction: "incoming".into(),
            filename: "a.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 4,
            bytes_transferred: 2,
            percentage: Some(50),
            state: "transferring".into(),
            failure: None,
            stored_at: None,
        }
    }

    #[test]
    fn the_default_lock_choice_is_app_only() {
        // The stored default the daemon writes, mirrored here so the page
        // cannot show a different one selected on a fresh peer.
        assert_eq!(peer().when_locked, "app-only");
    }
}
