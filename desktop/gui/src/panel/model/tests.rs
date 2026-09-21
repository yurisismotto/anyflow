//! The Quick Panel's rules, tested on plain data.
//!
//! Every test here runs with no display, no daemon and no socket. What they
//! cover is the part of the feature that can be wrong in a way nobody sees
//! until it sends a file to the wrong device: target resolution, action
//! availability, and the battery and capability semantics two earlier
//! hardening rounds fixed.
//!
//! They deliberately do **not** re-test the layers below. Whether a grant is
//! honoured, whether a fingerprint pins a certificate, whether a revoked
//! device can open a session — all of that has its own tests in
//! `omnibridge-core`, `omnibridge-runtime` and the capability crates, and is
//! re-checked by the daemon on every request regardless of what this model
//! decided two seconds earlier.

use super::*;
use omnibridge_control::{
    BatteryReport, ClipboardPeerReport, ClipboardStatusReport, ConnectionReport, DeviceReport,
    DeviceState, NotificationPeerReport, NotificationsStatusReport, StatusReport,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A paired, connected Android device with everything granted and negotiated.
///
/// Tests narrow from here, so what a test changes is what a test is about.
fn device(name: &str, fingerprint: &str) -> DeviceReport {
    DeviceReport {
        device_id: format!("id-{fingerprint}"),
        device_name: name.to_string(),
        platform: "android".into(),
        fingerprint: fingerprint.to_string(),
        fingerprint_short: format!("SHORT {fingerprint}"),
        paired_at_unix: 0,
        granted_capabilities: vec![
            FILES.to_string(),
            CLIPBOARD.to_string(),
            NOTIFICATIONS.to_string(),
            BATTERY.to_string(),
        ],
        revoked: false,
        paired: true,
        connected: true,
        state: DeviceState::Connected,
        silent_secs: Some(1),
        last_seen_secs_ago: None,
        battery: Some(BatteryReport {
            percentage: 78,
            charging_state: "Discharging".into(),
            age_secs: 3,
            stale: false,
        }),
    }
}

fn offline(mut d: DeviceReport) -> DeviceReport {
    d.connected = false;
    d.state = DeviceState::Disconnected;
    d.silent_secs = None;
    d.last_seen_secs_ago = Some(30);
    // The daemon drops telemetry when a peer disconnects; an offline device
    // never carries a battery number, and the builder must not invent one.
    d.battery = None;
    d
}

fn connection(d: &DeviceReport, negotiated: &[&str]) -> ConnectionReport {
    ConnectionReport {
        device_id: d.device_id.clone(),
        device_name: d.device_name.clone(),
        fingerprint_short: d.fingerprint_short.clone(),
        negotiated_capabilities: negotiated.iter().map(|s| s.to_string()).collect(),
        battery: d.battery.clone(),
        session_id: 1,
        state: d.state,
        silent_secs: d.silent_secs.unwrap_or(0),
    }
}

const ALL_CAPS: [&str; 4] = [FILES, CLIPBOARD, NOTIFICATIONS, BATTERY];

fn status(devices: &[DeviceReport]) -> StatusReport {
    StatusReport {
        device_name: "Fedora".into(),
        device_id: "self".into(),
        fingerprint: "ff".repeat(32),
        fingerprint_short: "SELF".into(),
        key_backing: "software".into(),
        listen_port: 55432,
        listen_families: "IPv4+IPv6".into(),
        protocol_version_min: 1,
        protocol_version_max: 1,
        capabilities: ALL_CAPS.iter().map(|s| s.to_string()).collect(),
        paired_devices: devices.len(),
        connections: devices
            .iter()
            .filter(|d| d.connected)
            .map(|d| connection(d, &ALL_CAPS))
            .collect(),
        devices: devices.to_vec(),
        pairing_active: false,
    }
}

fn clipboard_peer(d: &DeviceReport) -> ClipboardPeerReport {
    ClipboardPeerReport {
        device_id: d.device_id.clone(),
        device_name: d.device_name.clone(),
        fingerprint_short: d.fingerprint_short.clone(),
        granted: d.granted_capabilities.iter().any(|c| c == CLIPBOARD),
        revoked: d.revoked,
        connected: d.connected,
        allow_send: true,
        allow_receive: true,
        auto_send: false,
        auto_receive: true,
        last_outcome: None,
    }
}

fn clipboard(devices: &[DeviceReport]) -> ClipboardStatusReport {
    ClipboardStatusReport {
        enabled: true,
        backend: "wl-clipboard".into(),
        backend_detail: "wl-copy / wl-paste".into(),
        backend_available: true,
        watch_available: false,
        sensitive_available: true,
        sensitive_detail: String::new(),
        event_cache_entries: 0,
        suppression_cache_entries: 0,
        peers: devices.iter().map(clipboard_peer).collect(),
        pending: Vec::new(),
    }
}

fn notification_peer(d: &DeviceReport) -> NotificationPeerReport {
    NotificationPeerReport {
        device_id: d.device_id.clone(),
        device_name: d.device_name.clone(),
        fingerprint_short: d.fingerprint_short.clone(),
        granted: d.granted_capabilities.iter().any(|c| c == NOTIFICATIONS),
        revoked: d.revoked,
        connected: d.connected,
        allow_mirror: true,
        when_locked: "full".into(),
        allow_dismiss_sync: false,
        mirrors: 0,
        displayed: 0,
        evicted: 0,
        local_roles: 1,
        local_epoch: 1,
        peer_is_source: true,
        peer_is_dismiss_target: false,
        peer_epoch: 1,
        local_reports_dismissals: false,
        dismissals_sent: 0,
        dismissals_refused: 0,
        snapshot_open: false,
        queued: 0,
        coalesced: 0,
        dropped: 0,
    }
}

fn notifications(devices: &[DeviceReport]) -> NotificationsStatusReport {
    NotificationsStatusReport {
        enabled: true,
        backend: "freedesktop".into(),
        backend_detail: "gnome-shell 49".into(),
        available: true,
        body_markup: true,
        persistence: true,
        lock_source: "logind".into(),
        lock_detail: String::new(),
        locked: false,
        mirrors: 0,
        peers: devices.iter().map(notification_peer).collect(),
    }
}

/// A healthy daemon reporting exactly these devices.
fn live(devices: &[DeviceReport]) -> DaemonState {
    DaemonState {
        status: Some(status(devices)),
        devices: Some(devices.to_vec()),
        transfers: Some(Vec::new()),
        clipboard: Some(clipboard(devices)),
        notifications: Some(notifications(devices)),
        error: None,
    }
}

fn model(devices: &[DeviceReport], chosen: Option<&str>) -> PanelModel {
    PanelModel::build(&live(devices), chosen)
}

// ===========================================================================
// §27 — panel state model
// ===========================================================================

/// A. Zero peers is an empty panel, not an error and not a guess.
#[test]
fn zero_peers_produces_no_target_and_no_action() {
    let m = model(&[], None);
    assert_eq!(m.health, Health::Available);
    assert!(m.peers.is_empty());
    assert_eq!(m.target, Target::NoTrustedPeer);
    assert_eq!(m.target.fingerprint(), None);
    assert!(!m.send_file.is_ready());
    assert!(!m.send_clipboard.is_ready());
    assert!(m.send_file.reason().is_some_and(|r| r.contains("paired")));
}

/// B. One connected peer needs no selector and both actions work.
#[test]
fn one_connected_peer_is_the_target_without_being_chosen() {
    let m = model(&[device("Tablet", "aa11")], None);
    assert_eq!(m.target, Target::OnlyTrustedPeer("aa11".into()));
    assert_eq!(m.send_file.target(), Some("aa11"));
    assert_eq!(m.send_clipboard.target(), Some("aa11"));
    assert_eq!(m.peers[0].link, Link::Connected);
    assert_eq!(
        m.peers[0].available_capabilities(),
        ["Clipboard", "Files", "Notifications"]
    );
}

/// C. One offline peer still appears, and nothing that needs a session is
/// offered against it.
#[test]
fn one_offline_peer_appears_but_offers_no_live_action() {
    let m = model(&[offline(device("Tablet", "aa11"))], None);
    assert_eq!(m.peers.len(), 1);
    assert_eq!(m.peers[0].link, Link::Offline);
    // The grant survives the session; the *usability* does not.
    assert!(m.peers[0].files.granted);
    assert!(!m.peers[0].files.live);
    assert!(m.peers[0].available_capabilities().is_empty());
    for action in [&m.send_file, &m.send_clipboard] {
        assert!(!action.is_ready());
        assert_eq!(action.reason(), Some("Tablet is offline."));
    }
}

/// D. Several peers with no choice made: the panel asks rather than picking.
#[test]
fn several_peers_and_no_choice_asks_instead_of_guessing() {
    let m = model(&[device("Tablet", "aa11"), device("Phone", "bb22")], None);
    assert_eq!(
        m.target,
        Target::MustChoose {
            stale_choice: false
        }
    );
    assert_eq!(m.target.fingerprint(), None);
    assert!(!m.send_file.is_ready());
    assert_eq!(
        m.send_file.reason(),
        Some("Choose which device to send to.")
    );
}

/// E. The chosen fingerprint names its peer whatever order the devices arrive
/// in. This is the property U2 P1 found violated on Android, where the trust
/// store's order changes every time a session comes up.
#[test]
fn the_choice_maps_to_the_same_peer_in_any_order() {
    let (a, b, c) = (
        device("Tablet", "aa11"),
        device("Phone", "bb22"),
        device("Laptop", "cc33"),
    );
    let orders = [
        vec![a.clone(), b.clone(), c.clone()],
        vec![c.clone(), b.clone(), a.clone()],
        vec![b.clone(), a.clone(), c.clone()],
    ];
    for order in orders {
        let m = model(&order, Some("bb22"));
        assert_eq!(m.target, Target::Selected("bb22".into()));
        assert_eq!(m.send_file.target(), Some("bb22"));
        assert_eq!(m.target_peer().map(|p| p.name.as_str()), Some("Phone"));
        // And exactly one row is marked selected, whichever way round.
        let selected: Vec<&str> = m
            .peers
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.fingerprint.as_str())
            .collect();
        assert_eq!(selected, ["bb22"]);
    }
}

/// F. A stored choice that no longer names a trusted peer sends nothing.
///
/// Both halves matter. With several peers left there is obviously nothing to
/// fall back to — and with exactly *one* peer left this still refuses, which
/// is the deliberate divergence from Android's self-heal documented on
/// [`Target::MustChoose`].
#[test]
fn a_stale_choice_is_refused_rather_than_repointed() {
    let several = model(
        &[device("Tablet", "aa11"), device("Phone", "bb22")],
        Some("dead99"),
    );
    assert_eq!(several.target, Target::MustChoose { stale_choice: true });
    assert_eq!(several.target.fingerprint(), None);
    assert!(!several.peers.iter().any(|p| p.selected));

    let only_one = model(&[device("Tablet", "aa11")], Some("dead99"));
    assert_eq!(only_one.target, Target::MustChoose { stale_choice: true });
    assert_eq!(only_one.send_file.target(), None);
    assert_eq!(only_one.send_clipboard.target(), None);
    assert!(only_one
        .send_file
        .reason()
        .is_some_and(|r| r.contains("no longer paired")));
}

/// G. Zero percent is a battery at zero percent.
#[test]
fn a_present_battery_at_zero_renders_as_a_real_zero() {
    let mut d = device("Tablet", "aa11");
    d.battery = Some(BatteryReport {
        percentage: 0,
        charging_state: "Discharging".into(),
        age_secs: 1,
        stale: false,
    });
    let m = model(&[d], None);
    assert_eq!(
        m.peers[0].battery,
        Battery::Present {
            percent: 0,
            status: String::new(),
            stale: false
        }
    );
    assert_eq!(m.peers[0].battery.label(), "0%");
}

/// H. An absent battery is never a number — the U2 P2 defect, pinned.
#[test]
fn an_absent_battery_never_renders_as_zero() {
    let mut d = device("Tablet", "aa11");
    // Live, running battery.v1, reporting nothing: exactly what a device
    // with no battery does, because `battery.v1` expresses absence by
    // sending no frame at all.
    d.battery = None;
    let m = model(&[d], None);
    assert_eq!(m.peers[0].battery, Battery::Absent);
    let label = m.peers[0].battery.label();
    assert!(
        !label.contains('%'),
        "absence rendered as a percentage: {label}"
    );
    assert_eq!(label, "No battery reported");
}

/// I. Unavailable is a different state from Absent, with different words.
#[test]
fn an_unavailable_battery_is_distinct_from_an_absent_one() {
    // Offline: we cannot ask.
    let off = model(&[offline(device("Tablet", "aa11"))], None);
    assert_eq!(off.peers[0].battery, Battery::Unavailable);

    // Connected, but this session never negotiated battery.v1.
    let d = device("Phone", "bb22");
    let mut state = live(std::slice::from_ref(&d));
    let mut report = state.status.clone().expect("status");
    report.connections = vec![connection(&d, &[FILES, CLIPBOARD])];
    report.devices[0].battery = None;
    state.status = Some(report);
    state.devices.as_mut().expect("devices")[0].battery = None;
    let m = PanelModel::build(&state, None);
    assert_eq!(m.peers[0].battery, Battery::Unavailable);

    assert_ne!(Battery::Unavailable.label(), Battery::Absent.label());
}

/// J. Send File needs the grant *and* a session that negotiated it.
#[test]
fn the_file_action_needs_a_live_authorized_capability() {
    let ungranted = {
        let mut d = device("Tablet", "aa11");
        d.granted_capabilities.retain(|c| c != FILES);
        model(&[d], None)
    };
    assert!(!ungranted.send_file.is_ready());
    assert_eq!(
        ungranted.send_file.reason(),
        Some("Files are not enabled for Tablet.")
    );

    // Granted, connected, but the session did not negotiate files.v1.
    let d = device("Tablet", "aa11");
    let mut state = live(std::slice::from_ref(&d));
    let mut report = state.status.clone().expect("status");
    report.connections = vec![connection(&d, &[CLIPBOARD, BATTERY])];
    state.status = Some(report);
    let m = PanelModel::build(&state, None);
    assert!(m.peers[0].files.granted);
    assert!(!m.peers[0].files.live);
    assert!(!m.send_file.is_ready());
    assert!(m
        .send_file
        .reason()
        .is_some_and(|r| r.contains("not negotiated file transfer")));
}

/// K. Send Clipboard needs a working backend, the grant, and `allow_send`.
#[test]
fn the_clipboard_action_needs_every_precondition() {
    let d = device("Tablet", "aa11");

    let ok = model(std::slice::from_ref(&d), None);
    assert!(ok.send_clipboard.is_ready());

    let mut no_backend = live(std::slice::from_ref(&d));
    no_backend
        .clipboard
        .as_mut()
        .expect("report")
        .backend_available = false;
    assert!(!PanelModel::build(&no_backend, None)
        .send_clipboard
        .is_ready());

    let mut sending_off = live(std::slice::from_ref(&d));
    sending_off.clipboard.as_mut().expect("report").peers[0].allow_send = false;
    let m = PanelModel::build(&sending_off, None);
    assert!(!m.send_clipboard.is_ready());
    assert_eq!(
        m.send_clipboard.reason(),
        Some("Sending the clipboard to Tablet is turned off.")
    );
    // Sending being off does not take Send File with it.
    assert!(m.send_file.is_ready());

    let mut ungranted = d.clone();
    ungranted.granted_capabilities.retain(|c| c != CLIPBOARD);
    let m = model(&[ungranted], None);
    assert!(!m.send_clipboard.is_ready());
    assert_eq!(
        m.send_clipboard.reason(),
        Some("Clipboard is not enabled for Tablet.")
    );
}

/// L. The notifications row follows the grant and the mirror switch, not the
/// fact that a device advertised the capability.
#[test]
fn the_notifications_row_reflects_grant_and_policy() {
    let d = device("Tablet", "aa11");

    let on = model(std::slice::from_ref(&d), None);
    assert_eq!(on.notifications.value, StatusValue::On);

    // Advertised and negotiated, but the per-peer mirror switch is off.
    let mut muted = live(std::slice::from_ref(&d));
    muted.notifications.as_mut().expect("report").peers[0].allow_mirror = false;
    let m = PanelModel::build(&muted, None);
    assert_eq!(m.notifications.value, StatusValue::Off);
    assert!(m.notifications.detail.contains("turned off"));
    // The capability is still negotiated on the session — which is exactly
    // the thing that must not be read as "on".
    assert!(m.peers[0].notifications.usable());

    // No notification server in this session.
    let mut no_server = live(std::slice::from_ref(&d));
    no_server.notifications.as_mut().expect("report").available = false;
    assert_eq!(
        PanelModel::build(&no_server, None).notifications.value,
        StatusValue::Unavailable
    );

    // Granted but not revoked-safe: a revoked peer is Off, never On.
    let mut revoked = live(std::slice::from_ref(&d));
    revoked.notifications.as_mut().expect("report").peers[0].revoked = true;
    assert_eq!(
        PanelModel::build(&revoked, None).notifications.value,
        StatusValue::Off
    );
}

/// M. An offline peer cannot be the destination of a quick action, even when
/// it is the only device and everything is granted.
#[test]
fn an_offline_peer_cannot_receive_a_quick_action() {
    let m = model(&[offline(device("Tablet", "aa11"))], None);
    assert_eq!(m.target, Target::OnlyTrustedPeer("aa11".into()));
    // Resolving to a peer is not the same as being able to act on it.
    assert!(send_file_request(&m.send_file, "/tmp/x".into()).is_none());
    assert!(send_clipboard_request(&m.send_clipboard).is_none());
}

/// N. Two devices called the same thing stay two devices.
#[test]
fn a_display_name_collision_does_not_affect_routing() {
    let m = model(
        &[device("SM-X620", "aa11"), device("SM-X620", "bb22")],
        Some("bb22"),
    );
    assert_eq!(m.target, Target::Selected("bb22".into()));
    assert_eq!(m.send_file.target(), Some("bb22"));
    assert_eq!(m.peers.iter().filter(|p| p.selected).count(), 1);
    assert_eq!(
        m.peers
            .iter()
            .find(|p| p.selected)
            .map(|p| p.fingerprint.as_str()),
        Some("bb22")
    );
    // The two rows are distinguishable by something other than the name.
    assert_ne!(m.peers[0].fingerprint_short, m.peers[1].fingerprint_short);
}

/// O. An unreachable daemon is a calm, non-actionable state — not a raw
/// error and not a set of live-looking buttons.
#[test]
fn an_unreachable_daemon_produces_a_safe_state() {
    let state = DaemonState {
        error: Some(
            "could not reach the OmniBridge daemon at /run/user/1000/omnibridge/control.sock: \
             No such file or directory"
                .into(),
        ),
        ..DaemonState::default()
    };
    let m = PanelModel::build(&state, Some("aa11"));
    assert_eq!(
        m.health,
        Health::Unavailable {
            headline: "OmniBridge service is not available".into()
        }
    );
    assert!(m.peers.is_empty());
    assert!(!m.send_file.is_ready());
    assert!(!m.send_clipboard.is_ready());
    for line in [&m.files, &m.clipboard, &m.notifications] {
        assert_eq!(line.value, StatusValue::Unavailable);
    }
    // No socket path, no errno in front of the person.
    let shown = format!(
        "{}{}{}",
        m.send_file.reason().unwrap_or_default(),
        m.clipboard.detail,
        m.notifications.detail
    );
    assert!(!shown.contains("control.sock"), "raw error leaked: {shown}");
}

/// The first poll is not a failure.
#[test]
fn an_unanswered_first_poll_is_not_an_error() {
    let m = PanelModel::build(&DaemonState::default(), None);
    assert_eq!(m.health, Health::Reaching);
    assert!(!m.send_file.is_ready());
}

// ===========================================================================
// §28 — action routing
// ===========================================================================

/// A + B. Both actions carry the selected fingerprint into the request the
/// daemon will actually receive.
#[test]
fn both_actions_target_the_selected_fingerprint() {
    let m = model(
        &[device("Tablet", "aa11"), device("Phone", "bb22")],
        Some("bb22"),
    );
    match send_file_request(&m.send_file, "/tmp/holiday.jpg".into()) {
        Some(Request::Send { device, path }) => {
            assert_eq!(device, "bb22");
            assert_eq!(path, "/tmp/holiday.jpg");
        }
        other => panic!("expected a Send to bb22, got {other:?}"),
    }
    match send_clipboard_request(&m.send_clipboard) {
        Some(Request::ClipboardSend { device, sensitive }) => {
            assert_eq!(device, "bb22");
            // The panel never claims a clip is a secret on the user's behalf.
            assert!(!sensitive);
        }
        other => panic!("expected a ClipboardSend to bb22, got {other:?}"),
    }
}

/// C. Changing the choice changes the destination immediately.
#[test]
fn switching_the_selected_peer_switches_the_destination() {
    let peers = [device("Tablet", "aa11"), device("Phone", "bb22")];
    assert_eq!(model(&peers, Some("aa11")).send_file.target(), Some("aa11"));
    assert_eq!(model(&peers, Some("bb22")).send_file.target(), Some("bb22"));
}

/// D. The destination does not move when only the names move.
#[test]
fn a_display_name_never_chooses_the_destination() {
    let mut peers = [device("Tablet", "aa11"), device("Phone", "bb22")];
    let before = model(&peers, Some("aa11"))
        .send_file
        .target()
        .map(String::from);
    // Swap the names over. Nothing about identity changed.
    peers[0].device_name = "Phone".into();
    peers[1].device_name = "Tablet".into();
    let after = model(&peers, Some("aa11"))
        .send_file
        .target()
        .map(String::from);
    assert_eq!(before.as_deref(), Some("aa11"));
    assert_eq!(after, before);
}

/// E. Nor when only the order moves.
#[test]
fn list_ordering_never_chooses_the_destination() {
    let a = device("Tablet", "aa11");
    let b = device("Phone", "bb22");
    let forward = model(&[a.clone(), b.clone()], Some("aa11"));
    let backward = model(&[b, a], Some("aa11"));
    assert_eq!(forward.send_file.target(), Some("aa11"));
    assert_eq!(backward.send_file.target(), Some("aa11"));
    assert_eq!(forward.target, backward.target);
    // And the rows themselves come out in the same order either way, so the
    // panel does not reshuffle under the pointer when a session comes up.
    let names = |m: &PanelModel| m.peers.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&forward), names(&backward));
}

/// F. No fallback. A choice that has gone stale reaches nobody.
#[test]
fn a_stale_choice_never_falls_back_to_another_peer() {
    for peers in [
        vec![device("Tablet", "aa11")],
        vec![device("Tablet", "aa11"), device("Phone", "bb22")],
    ] {
        let m = model(&peers, Some("cc33"));
        assert!(send_file_request(&m.send_file, "/tmp/x".into()).is_none());
        assert!(send_clipboard_request(&m.send_clipboard).is_none());
    }
}

/// G. Nothing paired, nothing sent.
#[test]
fn with_no_peer_there_is_no_send() {
    let m = model(&[], Some("aa11"));
    assert!(send_file_request(&m.send_file, "/tmp/x".into()).is_none());
    assert!(send_clipboard_request(&m.send_clipboard).is_none());
}

/// H. The one-peer convenience still goes to that one peer and nowhere else.
#[test]
fn the_one_peer_convenience_stays_safe() {
    let m = model(&[device("Tablet", "aa11")], None);
    assert_eq!(m.send_file.target(), Some("aa11"));
    // Convenience, not silence: the panel still names the destination.
    match &m.send_file {
        Action::Ready { peer_name, .. } => assert_eq!(peer_name, "Tablet"),
        other => panic!("expected Ready, got {other:?}"),
    }
}

/// I. The panel decides from a poll that is already up to two seconds old, so
/// it is not the thing that enforces a grant. This pins the *shape* of that:
/// an action carries a fingerprint and nothing else, so the daemon's fresh
/// trust-store check is what the request actually meets.
#[test]
fn a_quick_action_carries_only_a_fingerprint_for_the_daemon_to_recheck() {
    let m = model(&[device("Tablet", "aa11")], None);
    let Some(Request::Send { device, .. }) = send_file_request(&m.send_file, "/tmp/x".into())
    else {
        panic!("expected a Send");
    };
    // No grant, no capability list, no session id — nothing this layer
    // believes travels with the request for the daemon to trust.
    assert_eq!(device, m.peers[0].fingerprint);
    assert!(device.bytes().all(|b| b.is_ascii_hexdigit()));
}

/// J. A refused action leaves the choice alone.
///
/// The model is a pure function of (daemon state, stored choice): there is no
/// path by which evaluating an action writes the selection. Pinning it here
/// means a later "helpfully re-target on failure" cannot arrive unnoticed.
#[test]
fn a_blocked_action_does_not_move_the_selection() {
    let peers = [offline(device("Tablet", "aa11")), device("Phone", "bb22")];
    let m = model(&peers, Some("aa11"));
    assert!(!m.send_file.is_ready());
    // Still aimed at the offline device the person chose, not at the
    // connected one standing next to it.
    assert_eq!(m.target, Target::Selected("aa11".into()));
    assert_eq!(
        m.target_peer().map(|p| p.fingerprint.as_str()),
        Some("aa11")
    );
    assert_eq!(
        m.peers
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.fingerprint.as_str())
            .collect::<Vec<_>>(),
        ["aa11"]
    );
}

// ===========================================================================
// Presentation rules the widget layer is not allowed to re-decide
// ===========================================================================

/// Revoked devices are not an everyday surface and do not reach the panel —
/// including as a possible destination.
#[test]
fn a_revoked_device_is_not_in_the_panel_or_the_target_set() {
    let mut revoked = device("Old tablet", "dd44");
    revoked.revoked = true;
    revoked.paired = false;
    revoked.connected = false;
    revoked.state = DeviceState::Revoked;
    revoked.granted_capabilities.clear();

    let m = model(&[revoked, device("Tablet", "aa11")], None);
    assert_eq!(m.peers.len(), 1);
    assert_eq!(m.peers[0].fingerprint, "aa11");
    // One trusted peer left, so the convenience case applies — to the
    // trusted one.
    assert_eq!(m.target, Target::OnlyTrustedPeer("aa11".into()));
}

/// D8, at the layer that decides: with the choice cleared and more than one
/// peer left, the panel **asks**.
///
/// The removal itself clears the choice (`Selection::forget_if`) and does
/// nothing else. This is the other half — that "nothing else" really does
/// leave the panel in the state that asks, rather than falling through to
/// whatever is left. A cleanup that quietly re-aimed the Send button would be
/// the "fallback to another peer" this model was built to refuse, and it would
/// look identical on screen to a deliberate choice.
#[test]
fn clearing_the_choice_asks_again_rather_than_picking_a_survivor() {
    let peers = [device("Tablet", "aa11"), device("Laptop", "bb22")];

    // Before: a real choice, honoured.
    assert_eq!(
        model(&peers, Some("bb22")).target,
        Target::Selected("bb22".into())
    );

    // After a removal cleared it: no destination, and nothing chosen for the
    // person.
    let m = model(&peers, None);
    assert_eq!(
        m.target,
        Target::MustChoose {
            stale_choice: false
        }
    );
    assert_eq!(m.target.fingerprint(), None);
    assert!(!m.send_file.is_ready(), "and the actions say so");
    assert!(!m.send_clipboard.is_ready());
}

/// And a removal that clears the choice must not be confusable with a *stale*
/// one: a device that is gone from the list is gone, and the panel says
/// "choose", not "the device you chose has gone".
#[test]
fn a_tombstoned_choice_reads_as_a_stale_choice_until_it_is_cleared() {
    let peers = [device("Tablet", "aa11"), device("Laptop", "bb22")];
    // The fingerprint was removed from the list but the choice still names
    // it — the state between the daemon agreeing and the choice being
    // dropped. It must not resolve to anything.
    let m = model(&peers, Some("cc33"));
    assert_eq!(m.target, Target::MustChoose { stale_choice: true });
    assert_eq!(m.target.fingerprint(), None);
}

/// Connected rows sort above offline ones, and ties break on something
/// stable rather than on arrival order.
#[test]
fn rows_are_ordered_for_reading_not_for_routing() {
    let m = model(
        &[
            offline(device("Aardvark", "aa11")),
            device("Zebra", "bb22"),
            offline(device("Beetle", "cc33")),
        ],
        None,
    );
    let names: Vec<&str> = m.peers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Zebra", "Aardvark", "Beetle"]);
}

/// A stale session labels its reading as history rather than as current.
#[test]
fn a_stale_session_marks_its_battery_reading_as_history() {
    let mut d = device("Tablet", "aa11");
    d.state = DeviceState::Stale;
    d.silent_secs = Some(90);
    let m = model(&[d], None);
    assert_eq!(m.peers[0].link, Link::Stale);
    match &m.peers[0].battery {
        Battery::Present { stale, percent, .. } => {
            assert!(*stale, "a reading over a stale session is history");
            assert_eq!(*percent, 78);
        }
        other => panic!("expected a reading, got {other:?}"),
    }
    assert!(m.peers[0].battery.label().contains("last known"));
    // And nothing that needs a live session is offered.
    assert!(!m.send_file.is_ready());
}

/// A refusal by the receiving device reaches the panel.
///
/// The send request is answered as soon as the frame is on the session, so
/// "sent" is all the pressing end can be told at the time. The peer's verdict
/// arrives afterwards, and if it is a refusal the panel has to say so — this
/// is the real-hardware case where an Android device without the matching
/// grant silently dropped every clip.
#[test]
fn a_refusal_by_the_receiving_device_is_reported() {
    let d = device("Tablet", "aa11");
    let mut refused = live(std::slice::from_ref(&d));
    refused.clipboard.as_mut().expect("report").peers[0].last_outcome =
        Some("not authorized".into());
    let m = PanelModel::build(&refused, None);
    assert!(
        m.clipboard
            .detail
            .contains("Tablet is not set up to accept this computer's clipboard."),
        "{}",
        m.clipboard.detail
    );

    // A clip that landed says so. It used to say nothing, which was fine
    // while the press claimed "Clipboard sent" — but the press now says
    // "awaiting confirmation", and an empty row is not what that resolves to.
    for (landed, expected) in [
        ("applied", "The last clip reached Tablet."),
        (
            "pending",
            "The last clip reached Tablet and is waiting to be applied there.",
        ),
        ("duplicate", "Tablet already had the last clip."),
    ] {
        let mut ok = live(std::slice::from_ref(&d));
        ok.clipboard.as_mut().expect("report").peers[0].last_outcome = Some(landed.into());
        let detail = PanelModel::build(&ok, None).clipboard.detail;
        assert!(detail.contains(expected), "{landed}: {detail}");
    }
}

// ---------------------------------------------------------------------------
// QP-DEBT-06 — the immediate Send clipboard feedback
// ---------------------------------------------------------------------------

/// **D1 — a local enqueue is never labelled a confirmed success.**
///
/// The whole of QP-DEBT-06. `ClipboardSend` is answered as soon as the frame
/// is on the session; the press used to be acknowledged with "Clipboard sent
/// to SM-X620", and on hardware the tablet had refused the clip.
#[test]
fn d1_the_immediate_message_does_not_claim_delivery() {
    let message = clipboard_submitted_message("SM-X620");
    assert!(
        !message.contains("Clipboard sent"),
        "the press must not claim delivery: {message}"
    );
    // A whitelist would be brittle; what matters is that no reading of it
    // says the clip arrived.
    for claim in ["arrived", "received", "delivered", "copied"] {
        assert!(!message.contains(claim), "{claim} in {message}");
    }
}

/// **D2 — and it says, in words, that it is waiting.**
#[test]
fn d2_the_immediate_message_names_the_pending_state() {
    let message = clipboard_submitted_message("SM-X620");
    assert!(message.contains("awaiting confirmation"), "{message}");
    // It names the destination, so a person with two devices paired knows
    // which one the answer will be about.
    assert!(message.contains("SM-X620"), "{message}");
}

/// **D3 — a confirmed peer verdict reads as success.**
#[test]
fn d3_a_confirmed_verdict_reads_as_success() {
    assert!(clipboard_outcome_succeeded("applied"));
    // `pending` is the peer holding the clip for a person to apply: it
    // arrived. `duplicate` means an earlier copy of it did.
    assert!(clipboard_outcome_succeeded("pending"));
    assert!(clipboard_outcome_succeeded("duplicate"));
    assert!(clipboard_outcome_note("applied", "SM-X620").contains("reached SM-X620"));
}

/// **D4 — a not-authorized verdict is an explicit rejection.**
#[test]
fn d4_a_not_authorized_verdict_is_explicit() {
    assert!(!clipboard_outcome_succeeded("not authorized"));
    let note = clipboard_outcome_note("not authorized", "SM-X620");
    assert!(note.contains("not set up to accept"), "{note}");
    assert!(!note.contains("reached"), "{note}");
}

/// **D5 — every other refusal is a refusal, including one we do not know.**
///
/// The default arm matters more than the named ones: a newer daemon reporting
/// an outcome this build has never heard of must not fall through to
/// something that reads as success.
#[test]
fn d5_every_refusing_outcome_is_refused_including_unknown_ones() {
    for outcome in [
        "rejected by policy",
        "rejected as sensitive",
        "too large",
        "invalid text",
        "failed",
        "some-future-outcome",
        "",
    ] {
        assert!(
            !clipboard_outcome_succeeded(outcome),
            "{outcome} must not read as success"
        );
        let note = clipboard_outcome_note(outcome, "SM-X620");
        assert!(!note.contains("reached SM-X620"), "{outcome}: {note}");
        assert!(!note.is_empty(), "{outcome} must say something");
    }
}

/// **D6 — no verdict is not a success.**
///
/// A device that has answered nothing leaves the row saying nothing about a
/// last clip, which is the honest state: the panel does not know.
#[test]
fn d6_no_verdict_produces_no_claim() {
    let d = device("Tablet", "aa11");
    let report = live(std::slice::from_ref(&d));
    let detail = PanelModel::build(&report, None).clipboard.detail;
    assert!(!detail.contains("last clip"), "{detail}");
    assert!(!detail.contains("reached"), "{detail}");
}

/// **D7 — a verdict for one device does not describe another.**
///
/// The row is built from the selected peer's own `ClipboardPeerReport`, which
/// is matched on device id and short fingerprint together.
#[test]
fn d7_a_verdict_for_another_device_does_not_reach_this_row() {
    let a = device("Tablet", "aa11");
    let b = device("Laptop", "bb22");
    let mut report = live(&[a.clone(), b.clone()]);
    // Only the *second* device refused something.
    let peers = &mut report.clipboard.as_mut().expect("report").peers;
    peers[1].last_outcome = Some("not authorized".into());

    let m = PanelModel::build(&report, Some("aa11"));
    assert_eq!(m.target.fingerprint(), Some("aa11"));
    assert!(
        !m.clipboard.detail.contains("not set up to accept"),
        "{}",
        m.clipboard.detail
    );

    // And it does reach the row it belongs to.
    let m = PanelModel::build(&report, Some("bb22"));
    assert!(
        m.clipboard
            .detail
            .contains("Laptop is not set up to accept"),
        "{}",
        m.clipboard.detail
    );
}

/// **D8 — no clipboard content can reach the panel's model or wording.**
///
/// Structural: `ClipboardPeerReport` has no field that could carry text, and
/// the vocabulary is composed from a device name and a fixed sentence. This
/// pins the wording half by feeding a canary through as the device name — the
/// only attacker-influenced string the row interpolates — and checking that
/// no outcome path invents anything else.
#[test]
fn d8_no_clipboard_content_appears_in_the_row() {
    for outcome in [
        "applied",
        "pending",
        "duplicate",
        "not authorized",
        "rejected by policy",
        "rejected as sensitive",
        "too large",
        "invalid text",
        "failed",
        "unknown",
    ] {
        let note = clipboard_outcome_note(outcome, "Tablet");
        // The daemon's inter-process vocabulary is not English and is never
        // echoed: "Last clip sent: rejected as sensitive by Tablet." is what
        // echoing it read like.
        assert!(
            !note.contains(&format!("{outcome} by")),
            "{outcome}: the protocol string leaked into {note}"
        );
    }
    assert_eq!(
        clipboard_submitted_message("Tablet"),
        "Clipboard submitted to Tablet; awaiting confirmation"
    );
}

/// **D9 — the row's existing semantics are unchanged.**
///
/// The direction sentences, the mobile caveat and the grant/policy states are
/// what they were; QP-DEBT-06 appends a verdict and changes nothing else.
#[test]
fn d9_the_existing_row_semantics_are_preserved() {
    let d = device("Tablet", "aa11");
    let mut report = live(std::slice::from_ref(&d));
    report.clipboard.as_mut().expect("report").peers[0].last_outcome = Some("applied".into());
    let row = PanelModel::build(&report, None).clipboard;

    assert_eq!(row.value, StatusValue::On);
    assert!(
        row.detail
            .contains("Sends this computer's clipboard only when you press Send clipboard."),
        "{}",
        row.detail
    );
    assert!(
        row.detail
            .contains("Clips from Tablet replace this clipboard as they arrive."),
        "{}",
        row.detail
    );
    assert!(
        row.detail
            .contains("Tablet sends only when you ask it to there."),
        "{}",
        row.detail
    );
}

/// **D10 — no new routing authority is introduced.**
///
/// The wording functions take strings and answer strings. Nothing here reads
/// a target, resolves a device or composes a request, so nothing here can
/// become a second answer to "which computer".
#[test]
fn d10_the_feedback_wording_decides_no_destination() {
    let d = device("Tablet", "aa11");
    let mut report = live(std::slice::from_ref(&d));
    report.clipboard.as_mut().expect("report").peers[0].last_outcome =
        Some("not authorized".into());

    // A refusal in the row does not disable, enable or re-point the action:
    // the gate is grant, negotiation and policy, and a past verdict is none
    // of them.
    let m = PanelModel::build(&report, None);
    assert!(m.send_clipboard.is_ready());
    match send_clipboard_request(&m.send_clipboard) {
        Some(Request::ClipboardSend { device, sensitive }) => {
            assert_eq!(device, "aa11");
            assert!(!sensitive);
        }
        other => panic!("expected a ClipboardSend to aa11, got {other:?}"),
    }
}

/// The clipboard row must not describe either direction as more automatic
/// than it is, and must not use the word the platform makes false.
#[test]
fn the_clipboard_row_states_each_direction_truthfully() {
    let d = device("Tablet", "aa11");

    // GNOME: no clipboard watch, so sending is manual whatever auto_send says.
    let mut gnome = live(std::slice::from_ref(&d));
    {
        let report = gnome.clipboard.as_mut().expect("report");
        report.watch_available = false;
        report.peers[0].auto_send = true;
        report.peers[0].auto_receive = true;
    }
    let m = PanelModel::build(&gnome, None);
    assert_eq!(m.clipboard.value, StatusValue::On);
    assert!(
        m.clipboard.detail.contains("only when you press"),
        "manual send not stated: {}",
        m.clipboard.detail
    );
    // The Android limit is stated, not implied away.
    assert!(m
        .clipboard
        .detail
        .contains("sends only when you ask it to there"));
    assert!(
        !m.clipboard.detail.to_lowercase().contains("sync"),
        "the word the platform makes false: {}",
        m.clipboard.detail
    );

    // A session that can watch, with auto-send on, may say so.
    let mut watching = live(std::slice::from_ref(&d));
    {
        let report = watching.clipboard.as_mut().expect("report");
        report.watch_available = true;
        report.peers[0].auto_send = true;
    }
    let m = PanelModel::build(&watching, None);
    assert!(m.clipboard.detail.contains("as it changes"));
}

/// Every blocked action explains itself in a sentence a person can act on.
#[test]
fn every_blocked_reason_is_a_sentence_not_an_error_code() {
    let cases = [
        model(&[], None),
        model(&[offline(device("Tablet", "aa11"))], None),
        model(&[device("A", "aa11"), device("B", "bb22")], None),
        model(&[device("A", "aa11")], Some("dead99")),
        PanelModel::build(&DaemonState::default(), None),
    ];
    for m in cases {
        for action in [&m.send_file, &m.send_clipboard] {
            let reason = action.reason().expect("a blocked action has a reason");
            assert!(!reason.is_empty());
            assert!(
                reason.chars().next().is_some_and(char::is_uppercase),
                "not a sentence: {reason}"
            );
            assert!(
                !reason.contains("Err") && !reason.contains("::"),
                "raw Rust in the UI: {reason}"
            );
        }
    }
}

/// The spoken form of a row carries everything the visual form does, so
/// nothing on it is conveyed by colour or position alone.
#[test]
fn a_row_announces_its_state_in_words() {
    let m = model(
        &[device("Tablet", "aa11"), device("Phone", "bb22")],
        Some("aa11"),
    );
    let tablet = m
        .peers
        .iter()
        .find(|p| p.fingerprint == "aa11")
        .expect("row");
    let spoken = tablet.announcement();
    assert!(spoken.contains("Tablet"));
    assert!(spoken.contains("selected device"));
    assert!(spoken.contains("connected"));
    assert!(spoken.contains("battery 78%"));
    assert!(spoken.contains("Clipboard"));

    let phone = m
        .peers
        .iter()
        .find(|p| p.fingerprint == "bb22")
        .expect("row");
    assert!(!phone.announcement().contains("selected"));

    let off = model(&[offline(device("Tablet", "aa11"))], None);
    let spoken = off.peers[0].announcement();
    assert!(spoken.contains("offline"));
    assert!(spoken.contains("no capabilities available"));
}

// ===========================================================================
// QP-POLISH-01 — recent transfers
// ===========================================================================
//
// The vocabulary these build on is not invented here. `omnibridge-control` names
// every value `TransferReport::state`, `direction` and `failure_code` can
// carry, and `desktop/runtime` pins those names to the capability crate that
// produces them — so a test below that uses `transfer_state::COMPLETED` is
// using the daemon's own token, and a reworded label in the state machine
// fails in the runtime rather than passing quietly here.

/// One transfer report. `seq` is the daemon's creation counter and is the only
/// thing that knows which of two transfers is newer.
fn transfer(seq: u64, filename: &str, peer: &str, direction: &str, state: &str) -> TransferReport {
    TransferReport {
        transfer_id: format!("{:032x}", seq),
        seq,
        device_name: peer.to_string(),
        fingerprint_short: format!("SHORT {peer}"),
        direction: direction.to_string(),
        filename: filename.to_string(),
        mime_type: "application/octet-stream".into(),
        size_bytes: 1024,
        bytes_transferred: 1024,
        percentage: Some(100),
        state: state.to_string(),
        failure: None,
        failure_code: None,
        stored_at: None,
    }
}

fn sent(seq: u64, filename: &str, peer: &str) -> TransferReport {
    transfer(
        seq,
        filename,
        peer,
        transfer_direction::SENDING,
        transfer_state::COMPLETED,
    )
}

fn received(seq: u64, filename: &str, peer: &str) -> TransferReport {
    transfer(
        seq,
        filename,
        peer,
        transfer_direction::RECEIVING,
        transfer_state::COMPLETED,
    )
}

/// A terminal transfer that did not complete, named by the daemon's own
/// failure token.
fn ended(seq: u64, filename: &str, peer: &str, state: &str, code: &str) -> TransferReport {
    let mut t = transfer(seq, filename, peer, transfer_direction::RECEIVING, state);
    t.failure_code = Some(code.to_string());
    // The prose the daemon would also send. Deliberately *not* what anything
    // branches on — see `Outcome::read`.
    t.failure = Some("some sentence for a person to read".into());
    t.bytes_transferred = 0;
    t.percentage = Some(0);
    t
}

fn with_transfers(devices: &[DeviceReport], transfers: Vec<TransferReport>) -> DaemonState {
    let mut state = live(devices);
    state.transfers = Some(transfers);
    state
}

fn panel_with(transfers: Vec<TransferReport>) -> PanelModel {
    let devices = [device("Tablet", "aa11")];
    PanelModel::build(&with_transfers(&devices, transfers), Some("aa11"))
}

/// A. Nothing finished yet is no section at all, not an empty one.
#[test]
fn zero_terminal_transfers_shows_no_recent_section() {
    assert!(panel_with(Vec::new()).recent.is_empty());

    // A daemon that has not answered yet is also not an empty section.
    let mut unanswered = live(&[device("Tablet", "aa11")]);
    unanswered.transfers = None;
    assert!(PanelModel::build(&unanswered, Some("aa11"))
        .recent
        .is_empty());

    // And neither is one holding only transfers that are still moving.
    let moving = panel_with(vec![transfer(
        1,
        "moving.bin",
        "Tablet",
        transfer_direction::SENDING,
        transfer_state::TRANSFERRING,
    )]);
    assert!(moving.recent.is_empty());
}

/// B. A completed outgoing transfer reads "To <peer> · Sent".
#[test]
fn a_completed_outgoing_transfer_reads_as_sent_to_the_peer() {
    let m = panel_with(vec![sent(1, "document.pdf", "SM-X620")]);
    let row = &m.recent[0];
    assert_eq!(row.outcome, Outcome::Sent);
    assert!(row.outgoing);
    assert_eq!(row.direction_word(), "To");
    assert_eq!(row.line(), "To SM-X620 · Sent");
    assert_eq!(row.accessible_label(), "Sent document.pdf to SM-X620");
    assert!(row.outcome.succeeded());
}

/// C. A completed incoming transfer reads "From <peer> · Received".
#[test]
fn a_completed_incoming_transfer_reads_as_received_from_the_peer() {
    let m = panel_with(vec![received(1, "photo.jpg", "SM-X620")]);
    let row = &m.recent[0];
    assert_eq!(row.outcome, Outcome::Received);
    assert!(!row.outgoing);
    assert_eq!(row.direction_word(), "From");
    assert_eq!(row.line(), "From SM-X620 · Received");
    assert_eq!(row.accessible_label(), "Received photo.jpg from SM-X620");
}

/// D. A refusal reads "Declined", and the detail says *who* refused — the
/// one word is the same for "they said no" and "I said no", which are
/// opposite events.
#[test]
fn a_declined_transfer_reads_as_declined() {
    let m = panel_with(vec![ended(
        1,
        "backup.zip",
        "SM-X620",
        transfer_state::CANCELLED,
        transfer_failure::DECLINED_BY_USER,
    )]);
    let row = &m.recent[0];
    assert_eq!(row.outcome, Outcome::Declined);
    assert_eq!(row.outcome.label(), "Declined");
    assert_eq!(row.line(), "From SM-X620 · Declined");
    assert_eq!(row.accessible_label(), "Declined backup.zip from SM-X620");
    assert!(!row.outcome.succeeded());
    // Incoming: this computer refused it.
    assert!(row.detail().contains("This computer declined"));

    // Outgoing: the peer refused it. Same word, opposite meaning, and the
    // detail is what tells them apart.
    let mut outgoing = ended(
        2,
        "backup.zip",
        "SM-X620",
        transfer_state::CANCELLED,
        transfer_failure::DECLINED_BY_USER,
    );
    outgoing.direction = transfer_direction::SENDING.into();
    let m = panel_with(vec![outgoing]);
    assert_eq!(m.recent[0].outcome, Outcome::Declined);
    assert!(m.recent[0].detail().contains("SM-X620 declined this file"));
}

/// E. A failure reads "Failed", and the daemon's sentence is not the label.
#[test]
fn a_failed_transfer_reads_as_failed() {
    for code in [
        transfer_failure::INTEGRITY,
        transfer_failure::STORAGE,
        transfer_failure::NOT_AUTHORIZED,
        transfer_failure::TOO_LARGE,
        transfer_failure::REVOKED,
    ] {
        let m = panel_with(vec![ended(
            1,
            "big.iso",
            "SM-X620",
            transfer_state::FAILED,
            code,
        )]);
        let row = &m.recent[0];
        assert_eq!(row.outcome, Outcome::Failed, "for {code}");
        assert_eq!(row.line(), "From SM-X620 · Failed");
    }

    // The raw sentence the daemon sent is nowhere in what is drawn. Detailed
    // error text stays in Settings, which shows it verbatim.
    let m = panel_with(vec![ended(
        1,
        "big.iso",
        "SM-X620",
        transfer_state::FAILED,
        transfer_failure::INTEGRITY,
    )]);
    let row = &m.recent[0];
    for text in [row.line(), row.accessible_label(), row.detail()] {
        assert!(
            !text.contains("some sentence for a person to read"),
            "the daemon's prose reached the panel: {text}"
        );
    }
}

/// F. A timeout reads "Timed out", and a dropped connection reads
/// "Disconnected" — both are represented in the daemon's reason set, so both
/// are told apart rather than flattened into "Failed".
#[test]
fn a_timeout_and_a_dropped_connection_are_named_apart() {
    let m = panel_with(vec![ended(
        1,
        "slow.bin",
        "SM-X620",
        transfer_state::FAILED,
        transfer_failure::TIMED_OUT,
    )]);
    assert_eq!(m.recent[0].outcome, Outcome::TimedOut);
    assert_eq!(m.recent[0].outcome.label(), "Timed out");
    assert!(m.recent[0].detail().contains("not answered in time"));

    let m = panel_with(vec![ended(
        1,
        "slow.bin",
        "SM-X620",
        transfer_state::FAILED,
        transfer_failure::TRANSPORT,
    )]);
    assert_eq!(m.recent[0].outcome, Outcome::Disconnected);
    assert_eq!(m.recent[0].outcome.label(), "Disconnected");

    // A cancel is a cancel and not an error.
    let m = panel_with(vec![ended(
        1,
        "slow.bin",
        "SM-X620",
        transfer_state::CANCELLED,
        transfer_failure::CANCELLED_BY_USER,
    )]);
    assert_eq!(m.recent[0].outcome, Outcome::Cancelled);

    // A terminal state the daemon gave no reason for still gets a truthful
    // word rather than being called a success.
    let m = panel_with(vec![transfer(
        1,
        "slow.bin",
        "SM-X620",
        transfer_direction::RECEIVING,
        transfer_state::CANCELLED,
    )]);
    assert_eq!(m.recent[0].outcome, Outcome::Cancelled);
    let m = panel_with(vec![transfer(
        1,
        "slow.bin",
        "SM-X620",
        transfer_direction::RECEIVING,
        transfer_state::FAILED,
    )]);
    assert_eq!(m.recent[0].outcome, Outcome::Failed);
}

/// G. Three at most, however many the daemon is holding.
#[test]
fn at_most_three_recent_transfers_are_shown() {
    let many: Vec<TransferReport> = (1..=9)
        .map(|n| sent(n, &format!("file-{n}.txt"), "SM-X620"))
        .collect();
    let m = panel_with(many);
    assert_eq!(m.recent.len(), RECENT_LIMIT);
    assert_eq!(RECENT_LIMIT, 3);
}

/// H. Newest first — and *newest* is the daemon's creation counter, not the
/// order the list arrived in. The daemon keys transfers by a random 128-bit
/// id, so arrival order is the order of a random number.
#[test]
fn recent_transfers_are_newest_first() {
    // Deliberately handed over in an order that is neither oldest- nor
    // newest-first, the way a map keyed by a random id delivers them.
    let m = panel_with(vec![
        sent(7, "seventh.txt", "SM-X620"),
        sent(2, "second.txt", "SM-X620"),
        sent(9, "ninth.txt", "SM-X620"),
        sent(4, "fourth.txt", "SM-X620"),
    ]);
    let names: Vec<&str> = m.recent.iter().map(|r| r.filename.as_str()).collect();
    assert_eq!(names, ["ninth.txt", "seventh.txt", "fourth.txt"]);
    assert!(m.recent[0].sort_key > m.recent[1].sort_key);

    // And the three kept are the three newest, not the first three seen.
    assert!(!names.contains(&"second.txt"));
}

/// I. An in-flight transfer appears once, as progress — never also as an
/// outcome that has not happened.
#[test]
fn an_in_flight_transfer_is_not_duplicated_into_the_recent_list() {
    let moving = transfer(
        5,
        "moving.bin",
        "Tablet",
        transfer_direction::SENDING,
        transfer_state::TRANSFERRING,
    );
    let m = panel_with(vec![moving.clone(), sent(1, "done.txt", "Tablet")]);

    assert_eq!(
        m.transfer.as_ref().map(|t| t.filename.as_str()),
        Some("moving.bin")
    );
    assert_eq!(m.recent.len(), 1);
    assert_eq!(m.recent[0].filename, "done.txt");
    assert!(
        !m.recent.iter().any(|r| r.filename == "moving.bin"),
        "a transfer still moving was given an outcome"
    );

    // Every non-terminal state, not just the interesting one.
    for state in [
        transfer_state::OFFERED,
        transfer_state::WAITING_ACCEPT,
        transfer_state::TRANSFERRING,
        transfer_state::VERIFYING,
    ] {
        let mut t = moving.clone();
        t.state = state.to_string();
        assert!(
            panel_with(vec![t]).recent.is_empty(),
            "{state} was treated as finished"
        );
    }
}

/// J. Two peers sharing a display name do not confuse a row's identity — and
/// a row's identity never comes from the selection.
#[test]
fn a_display_name_collision_does_not_affect_transfer_identity() {
    let devices = [device("SM-X620", "aa11"), device("SM-X620", "bb22")];
    // A transfer that happened with the *second* device, while the *first* is
    // the one selected for sending.
    let mut with_second = sent(1, "photo.jpg", "SM-X620");
    with_second.fingerprint_short = "SHORT bb22".into();
    let state = with_transfers(&devices, vec![with_second]);

    let chosen_first = PanelModel::build(&state, Some("aa11"));
    let chosen_second = PanelModel::build(&state, Some("bb22"));

    // The row is identical either way: it describes what happened, and what
    // happened does not change when someone picks a different device to send
    // to next.
    assert_eq!(chosen_first.recent, chosen_second.recent);
    assert_eq!(chosen_first.recent[0].peer_fingerprint_short, "SHORT bb22");
    // The short fingerprint is in the *description*, which is what tells two
    // identically named devices apart for someone who cannot see the screen.
    assert!(chosen_first.recent[0].detail().contains("bb22"));
    // And not on the visible row.
    assert!(!chosen_first.recent[0].line().contains("bb22"));

    // Meanwhile the destination for the *next* file is unmoved by any of it.
    assert_eq!(chosen_first.target, Target::Selected("aa11".into()));
    assert_eq!(chosen_second.target, Target::Selected("bb22".into()));
}

/// K. The recent list is not a routing authority. Whatever it holds, the
/// target is what the stored choice says — the list cannot move it, and is
/// not consulted when nothing has been chosen.
#[test]
fn a_recent_transfer_does_not_change_the_selected_peer() {
    let devices = [device("Tablet", "aa11"), device("Phone", "bb22")];

    // Transfers with `bb22` only, and a stored choice of `aa11`.
    let mut with_other = sent(3, "photo.jpg", "Phone");
    with_other.fingerprint_short = "SHORT bb22".into();
    let state = with_transfers(&devices, vec![with_other.clone()]);

    let m = PanelModel::build(&state, Some("aa11"));
    assert_eq!(m.target, Target::Selected("aa11".into()));
    assert_eq!(
        m.send_file.target(),
        Some("aa11"),
        "the most recent transfer re-aimed the Send button"
    );

    // And with no choice at all, a recent transfer is not a choice: several
    // peers still means asking.
    let none = PanelModel::build(&state, None);
    assert_eq!(
        none.target,
        Target::MustChoose {
            stale_choice: false
        }
    );
    assert!(!none.send_file.is_ready());
    assert_eq!(none.recent.len(), 1, "the row is still shown");

    // A stale choice is still refused, even though a recent transfer names a
    // peer that does exist.
    let stale = PanelModel::build(&state, Some("cc33"));
    assert_eq!(stale.target, Target::MustChoose { stale_choice: true });
    assert!(!stale.send_file.is_ready());
}

/// L. "View all transfers" opens the Settings Transfers page through the
/// application's own action, and that action is parameterless so a tray or a
/// keybinding can call it by name.
#[test]
fn view_all_targets_the_settings_transfers_page() {
    assert_eq!(crate::ACTION_TRANSFERS, "transfers");
    // The page it opens is an existing Settings page, reachable by the name
    // the `--page` seam already accepts. Nothing new is built for it: that
    // page is where the full, daemon-run-scoped transfer list already lives.
    assert_eq!(crate::Page::from_name("files"), Some(crate::Page::Files));
    // That the action actually presents Settings on that page needs real GTK
    // objects and is asserted in `application_gate`.
}

/// M. A row is metadata. Nothing on it can carry content, a hash, a stored
/// path or an id.
#[test]
fn a_recent_row_carries_no_content_hash_or_identifier() {
    let mut t = received(1, "photo.jpg", "SM-X620");
    t.transfer_id = "deadbeefdeadbeefdeadbeefdeadbeef".into();
    t.stored_at = Some("/home/yuri/Downloads/OmniBridge/photo.jpg".into());
    t.mime_type = "image/jpeg".into();
    t.size_bytes = 4_194_304;

    let m = panel_with(vec![t]);
    let row = &m.recent[0];

    // Asserted on every string the row can produce, not only the ones the
    // current widget code happens to draw.
    let rendered = format!(
        "{} {} {} {}",
        row.filename,
        row.line(),
        row.accessible_label(),
        row.detail()
    );
    for forbidden in [
        "deadbeef",   // the transfer id
        "/home/yuri", // the stored path
        "Downloads",  // ditto
        "image/jpeg", // the mime type
        "4194304",    // the size
        "4.0 MB",     // ditto, rendered
    ] {
        assert!(
            !rendered.contains(forbidden),
            "{forbidden:?} reached the panel in {rendered:?}"
        );
    }

    // The struct itself has no field for any of it either — a row cannot grow
    // one by someone drawing more of it.
    let debug = format!("{row:?}");
    assert!(!debug.contains("deadbeef"), "the id is on the row: {debug}");
    assert!(
        !debug.contains("Downloads"),
        "the path is on the row: {debug}"
    );
    assert!(
        !debug.contains("4194304"),
        "the size is on the row: {debug}"
    );

    // The one identifier it does carry is the short fingerprint, which is
    // what disambiguates two devices with the same name and is already what
    // the peer rows expose. Never the full one.
    assert_eq!(row.peer_fingerprint_short, "SHORT SM-X620");
}

/// The section is not a history, and says so by construction: it holds only
/// what the daemon is holding, and the daemon holds nothing across a restart.
#[test]
fn the_recent_list_is_only_the_current_daemon_run() {
    // A daemon that has just restarted reports no transfers, and the panel
    // has nothing to show — there is no store for it to fall back on.
    let mut restarted = live(&[device("Tablet", "aa11")]);
    restarted.transfers = Some(Vec::new());
    assert!(PanelModel::build(&restarted, Some("aa11"))
        .recent
        .is_empty());

    // And an unreachable daemon shows none either, rather than the last set
    // it saw.
    let mut down = live(&[device("Tablet", "aa11")]);
    down.error = Some("connection refused".into());
    down.transfers = None;
    let m = PanelModel::build(&down, Some("aa11"));
    assert!(m.recent.is_empty());
    assert!(!m.health.is_available());
}

/// A direction this build does not recognise is not guessed at. Every row
/// reads "to" or "from" somebody, so an unknown direction would not be a
/// vaguer label but a false statement about where a file went.
#[test]
fn a_transfer_with_an_unrecognised_direction_is_not_shown() {
    let mut odd = sent(1, "mystery.bin", "SM-X620");
    odd.direction = "sideways".into();
    assert!(panel_with(vec![odd]).recent.is_empty());

    // The two real values, from the daemon's own vocabulary, both work.
    assert!(!panel_with(vec![sent(1, "a", "P")]).recent.is_empty());
    assert!(!panel_with(vec![received(1, "a", "P")]).recent.is_empty());
}

/// Every outcome says its piece in words, and none of them is an error code.
#[test]
fn every_outcome_is_a_calm_word_rather_than_an_error_code() {
    for outcome in [
        Outcome::Sent,
        Outcome::Received,
        Outcome::Declined,
        Outcome::Cancelled,
        Outcome::TimedOut,
        Outcome::Disconnected,
        Outcome::Failed,
    ] {
        let label = outcome.label();
        assert!(!label.is_empty());
        // A label, not a token: capitalised for a person, with no snake_case
        // and no punctuation from a debug format.
        assert!(!label.contains('_'), "{label} is a token");
        assert!(!label.contains("Error"), "{label} shouts");
        assert!(
            label.chars().next().is_some_and(|c| c.is_uppercase()),
            "{label} is not a sentence-case word"
        );
    }
    assert!(Outcome::Sent.succeeded());
    assert!(Outcome::Received.succeeded());
    for bad in [
        Outcome::Declined,
        Outcome::Cancelled,
        Outcome::TimedOut,
        Outcome::Disconnected,
        Outcome::Failed,
    ] {
        assert!(!bad.succeeded(), "{:?} claimed success", bad);
    }
}
