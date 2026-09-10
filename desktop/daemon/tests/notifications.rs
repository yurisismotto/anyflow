//! `notifications.v1` over the real transport.
//!
//! Everything here runs against a genuine TLS 1.3 session with real SPKI
//! pinning, a real handshake, the real capability registry and the real trust
//! store. Where the capability's own suite proves the rules in isolation, this
//! one proves they survive the transport, the negotiation and the grant
//! plumbing — including the parts that only exist out here: that pairing alone
//! grants nothing, that a peer AnyFlow has never granted cannot reach the
//! capability at all, and that a broken sink does not take the other three
//! capabilities down with it.
//!
//! The notification server is in-memory. That is not a weakening: the real
//! `org.freedesktop.Notifications` server is exercised in
//! `anyflow-capability-notifications`'s own `real_dbus` gate, and a suite that
//! posted notifications onto the developer's desktop on every `cargo test`
//! would not survive contact with anybody's patience.

mod common;

use std::time::Duration;

use anyflow_capability_notifications::{LockPolicy, NotificationPolicy, CAPABILITY_ID};
use common::*;

const TIMEOUT: Duration = Duration::from_secs(5);
const ORIGIN: &str = "0123456789abcdef0123456789abcdef";

fn id_bytes(seed: u16) -> Vec<u8> {
    let mut out = vec![0u8; 16];
    out[0] = (seed >> 8) as u8;
    out[1] = (seed & 0xff) as u8;
    out
}

fn content_hash(seed: u16, body: &str) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    out[0] = (seed & 0xff) as u8;
    for (index, byte) in body.bytes().enumerate() {
        out[1 + index % 31] ^= byte;
    }
    out
}

fn upsert(seed: u16, title: &str, body: &str) -> clip_pb::NotificationUpsert {
    clip_pb::NotificationUpsert {
        notification_id: id_bytes(seed),
        origin_device_id: ORIGIN.to_string(),
        app_id: "com.example.chat".to_string(),
        app_label: "Chat".to_string(),
        title: title.to_string(),
        body: body.to_string(),
        posted_at_unix_ms: 1_700_000_000_000,
        importance: clip_pb::NotificationImportance::Normal as i32,
        privacy: clip_pb::NotificationPrivacy::Private as i32,
        category: clip_pb::NotificationCategory::Message as i32,
        content_hash: content_hash(seed, body),
        ..clip_pb::NotificationUpsert::default()
    }
}

fn roles_body(
    roles: &[clip_pb::NotificationRole],
    epoch: u32,
) -> clip_pb::notification_control::Body {
    clip_pb::notification_control::Body::Roles(clip_pb::NotificationRoles {
        roles: roles.iter().map(|r| *r as i32).collect(),
        epoch,
    })
}

fn marker_body(
    sync_id: &[u8],
    phase: clip_pb::sync_marker::Phase,
) -> clip_pb::notification_control::Body {
    clip_pb::notification_control::Body::Sync(clip_pb::SyncMarker {
        sync_id: sync_id.to_vec(),
        phase: phase as i32,
    })
}

fn remove_body(seed: u16) -> clip_pb::notification_control::Body {
    clip_pb::notification_control::Body::Remove(clip_pb::NotificationRemove {
        notification_id: id_bytes(seed),
        origin_device_id: ORIGIN.to_string(),
    })
}

/// A paired client with `notifications.v1` granted, connected, and having
/// announced `SOURCE`.
async fn paired(
    policy: NotificationPolicy,
) -> (
    TestServer,
    TestClient,
    std::sync::Arc<CapturedNotifications>,
    ConnectedSession,
) {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_notifications("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    session.close().await;

    // Granted by hand, exactly as a user would: `notifications.v1` is not in
    // `auto_grant`, so pairing alone leaves it off.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;
    server
        .set_notification_policy(client.fingerprint, policy)
        .await;

    // A reconnect is what makes a widened grant take effect: the negotiated
    // set is intersected with the grant when the session is built. That is the
    // canonical capability-negotiation semantics and this wave did not invent
    // a notification-only exception to it.
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    // The desktop announces its roles first, on connect.
    let announced = captured.next_roles(TIMEOUT).await;
    assert_eq!(announced.epoch, 1);
    assert_eq!(
        announced.roles,
        vec![clip_pb::NotificationRole::Sink as i32],
        "the desktop announces SINK, and only SINK"
    );

    // And the phone says it can source.
    assert!(
        send_notification_control(
            &session,
            roles_body(&[clip_pb::NotificationRole::Source], 1)
        )
        .await
    );

    (server, client, captured, session)
}

// ---------------------------------------------------------------------------
// Negotiation and the explicit grant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_capability_is_advertised_by_the_daemon() {
    let server = TestServer::start().await;
    assert!(
        server
            .state
            .registry
            .advertised()
            .iter()
            .any(|c| c == CAPABILITY_ID),
        "notifications.v1 must appear in HELLO whatever the platform state is: \
         roles exist so a capability can be supported while being unable to do \
         anything, and gating the handshake on a permission the user can toggle \
         would mean a reconnect were needed to pick up a change"
    );
}

#[tokio::test]
async fn pairing_alone_grants_nothing() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_notifications("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");

    {
        let store = server.state.store.lock().await;
        let peer = store.trusted_peer(&client.fingerprint).expect("paired");
        assert!(
            !peer.allows(CAPABILITY_ID),
            "pairing alone must not grant notification access"
        );
    }

    // An ungranted peer's message does not reach the capability at all: the
    // transport refuses it as un-negotiated. Nothing is displayed and no
    // notification result comes back.
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        server.notification_sink.displays().is_empty(),
        "nothing reached the desktop"
    );
    assert!(
        captured.drain().await.is_empty(),
        "and the capability never even saw it"
    );
}

#[tokio::test]
async fn a_granted_peer_puts_a_notification_on_the_desktop() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );

    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(result.notification_id, id_bytes(1));
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );

    let displays = server.notification_sink.displays();
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].1.summary, "Ana");
    assert_eq!(displays[0].1.body, "lunch?");
}

#[tokio::test]
async fn withdrawing_the_grant_stops_notifications_on_the_session_that_is_already_up() {
    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );

    // The grant goes away. No reconnect: the transport's own filter ran when
    // the session was built, so this is the only thing standing between a
    // revoked device and the screen — which is why the capability re-reads the
    // trust store per message rather than trusting the handshake.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, false)
        .await;
    server
        .state
        .notify_notifications_revoked(&client.fingerprint)
        .await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(2, "Ana", "still there?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::NotAuthorized as i32,
        "a withdrawn grant bites immediately"
    );

    // And what was already on the screen came off it.
    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the revoked peer's notifications stayed on the screen"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(server.notification_sink.closes(), vec![1]);
}

#[tokio::test]
async fn a_revoked_pairing_takes_the_notifications_off_the_screen() {
    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    captured.next_result(TIMEOUT).await;

    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&client.fingerprint).expect("revoke");
    }
    server
        .state
        .notify_notifications_revoked(&client.fingerprint)
        .await;

    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "a revoked device's notifications stayed on the screen"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ---------------------------------------------------------------------------
// Roles, over the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_peer_that_never_claimed_source_is_refused_over_the_real_transport() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_notifications("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    session.close().await;
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    captured.next_roles(TIMEOUT).await;

    // Granted, connected, and silent about its roles.
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::RejectedRole as i32
    );
    assert!(server.notification_sink.displays().is_empty());
}

#[tokio::test]
async fn a_dismiss_request_is_refused_over_the_real_transport() {
    let (_server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Dismiss(clip_pb::DismissRequest {
                notification_id: id_bytes(1),
                origin_device_id: ORIGIN.to_string(),
            })
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::RejectedRole as i32,
        "this desktop sources nothing, so it can dismiss nothing"
    );
}

#[tokio::test]
async fn the_role_narrows_and_widens_without_a_reconnect() {
    let (server, _client, captured, _session) = paired(NotificationPolicy::default()).await;

    server.notification_sink.go_away().await;
    let narrowed = captured.next_roles(TIMEOUT).await;
    assert!(narrowed.roles.is_empty());
    assert_eq!(narrowed.epoch, 2);

    server.notification_sink.come_back().await;
    let widened = captured.next_roles(TIMEOUT).await;
    assert_eq!(widened.roles, vec![clip_pb::NotificationRole::Sink as i32]);
    assert_eq!(widened.epoch, 3);
}

// ---------------------------------------------------------------------------
// Update, remove, snapshot — over the transport
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_update_replaces_and_a_removal_closes_over_the_real_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    for body in ["first", "second", "third"] {
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", body))
            )
            .await
        );
        let result = captured.next_result(TIMEOUT).await;
        assert_eq!(
            result.outcome,
            clip_pb::NotificationOutcome::Displayed as i32
        );
    }

    assert_eq!(
        server.notification_sink.live_count(),
        1,
        "three updates, one notification"
    );

    assert!(send_notification_control(&session, remove_body(1)).await);
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(result.outcome, clip_pb::NotificationOutcome::Removed as i32);
    assert_eq!(server.notification_sink.live_count(), 0);
}

#[tokio::test]
async fn a_removal_never_overtakes_its_upsert_across_the_transport() {
    // The transport guarantees FIFO *per producer* and the capability produces
    // from one task, so twenty-five post/remove pairs must converge on
    // "removed" every time. A single inversion would strand a notification on
    // a desktop permanently, which is the one failure mode worse than being
    // slow.
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    for seed in 1..=25u16 {
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(upsert(seed, "Ana", "flash"))
            )
            .await
        );
        assert!(send_notification_control(&session, remove_body(seed)).await);
    }

    // Wait for the screen to settle rather than for a fixed number of answers.
    // A removal that arrives while its upsert is still queued *supersedes* it,
    // so the number of answers is not fixed — but the end state is: nothing on
    // the screen, and no identity whose last word was "displayed".
    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "{} notifications were stranded on the screen",
            server.notification_sink.live_count()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let displayed = clip_pb::NotificationOutcome::Displayed as i32;
    let removed = clip_pb::NotificationOutcome::Removed as i32;
    let unknown = clip_pb::NotificationOutcome::UnknownNotification as i32;

    let mut last: std::collections::BTreeMap<Vec<u8>, i32> = std::collections::BTreeMap::new();
    let mut answers = 0usize;
    for control in captured.drain().await {
        if let Some(clip_pb::notification_control::Body::Result(result)) = control.body {
            assert!(
                [displayed, removed, unknown].contains(&result.outcome),
                "unexpected outcome {}",
                result.outcome
            );
            last.insert(result.notification_id, result.outcome);
            answers += 1;
        }
    }

    assert!(answers >= 25, "every identity was answered at least once");
    assert_eq!(last.len(), 25, "every identity was answered");
    for (id, outcome) in &last {
        assert_ne!(
            *outcome, displayed,
            "identity {id:?} was last answered 'displayed', so its removal              overtook the upsert it refers to"
        );
    }
}

#[tokio::test]
async fn a_reconnect_snapshot_converges_without_duplicating_the_screen() {
    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    for seed in 1..=3u16 {
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(upsert(seed, "Ana", "before"))
            )
            .await
        );
        captured.next_result(TIMEOUT).await;
    }
    assert_eq!(server.notification_sink.live_count(), 3);
    let displays_before = server.notification_sink.displays().len();

    // The link drops and comes back. The mirrors stay up during the grace,
    // which is what keeps a Wi-Fi blip from clearing the screen and re-posting
    // everything onto a `persistence` server.
    session.close().await;
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    let announced = captured.next_roles(TIMEOUT).await;
    assert_eq!(announced.epoch, 1, "a fresh connection starts again at 1");
    assert!(
        send_notification_control(
            &session,
            roles_body(&[clip_pb::NotificationRole::Source], 1)
        )
        .await
    );

    // The phone re-states what is active. Two of the three are still there.
    let sync = [0xA1u8; 16];
    assert!(
        send_notification_control(
            &session,
            marker_body(&sync, clip_pb::sync_marker::Phase::Begin)
        )
        .await
    );
    for seed in [1u16, 3] {
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(upsert(seed, "Ana", "before"))
            )
            .await
        );
        let result = captured.next_result(TIMEOUT).await;
        assert_eq!(
            result.outcome,
            clip_pb::NotificationOutcome::Duplicate as i32,
            "an identical re-send touches no desktop"
        );
    }
    assert!(
        send_notification_control(
            &session,
            marker_body(&sync, clip_pb::sync_marker::Phase::End)
        )
        .await
    );

    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the snapshot did not converge: {} notifications on the screen",
            server.notification_sink.live_count()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        server.notification_sink.displays().len(),
        displays_before,
        "and no wall of duplicates: the reconnect displayed nothing new"
    );
    assert_eq!(server.notification_sink.closes(), vec![2]);
}

#[tokio::test]
async fn an_unpaired_snapshot_end_removes_nothing_over_the_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    captured.next_result(TIMEOUT).await;

    let sync = [0xA1u8; 16];
    assert!(
        send_notification_control(
            &session,
            marker_body(&sync, clip_pb::sync_marker::Phase::End)
        )
        .await
    );

    // A barrier: one worker drains one queue in order, so an answer to this
    // proves the marker has already been handled.
    assert!(send_notification_control(&session, remove_body(9)).await);
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::UnknownNotification as i32
    );

    assert!(
        server.notification_sink.closes().is_empty(),
        "an END with no BEGIN must never be read as 'remove everything'"
    );
    assert_eq!(server.notification_sink.live_count(), 1);
}

// ---------------------------------------------------------------------------
// Lock policy, over the transport
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_locked_desktop_shows_the_application_and_nothing_else() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;
    server.notification_lock.set_locked(true).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "the code is 123456"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );

    let (_, mirror) = &server.notification_sink.displays()[0];
    assert_eq!(mirror.summary, "Chat");
    assert_eq!(mirror.body, "");
    assert!(mirror.redacted);
}

#[tokio::test]
async fn an_unknown_lock_state_withholds_the_body_over_the_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;
    server.notification_lock.set_unknown(true);

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "the code is 123456"))
        )
        .await
    );
    captured.next_result(TIMEOUT).await;

    assert_eq!(server.notification_sink.displays()[0].1.body, "");
}

#[tokio::test]
async fn a_suppress_policy_refuses_over_the_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy {
        when_sink_locked: LockPolicy::Suppress,
        ..NotificationPolicy::default()
    })
    .await;
    server.notification_lock.set_locked(true).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::RejectedPolicy as i32
    );
    assert!(server.notification_sink.displays().is_empty());
}

// ---------------------------------------------------------------------------
// Isolation — the hard gate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_broken_notification_sink_does_not_break_battery_or_the_clipboard() {
    use anyflow_capability_notifications::backend::SinkError;

    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    // The notification server is wedged: every call fails.
    server
        .notification_sink
        .set_failure(Some(SinkError::Unavailable("gone".into())));

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Unavailable as i32
    );

    // `battery.v1` still works on the same session.
    assert!(
        session
            .handle
            .send_capability(anyflow_core::capability::OutboundMessage {
                capability_id: "battery.v1".to_string(),
                payload: <clip_pb::BatteryState as prost::Message>::encode_to_vec(
                    &clip_pb::BatteryState {
                        percentage: 42,
                        charging_state: clip_pb::ChargingState::Discharging as i32,
                        timestamp_unix_ms: 1_700_000_000_000,
                    }
                ),
            })
            .await
    );

    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        if server
            .battery
            .get(&client.fingerprint)
            .is_some_and(|b| b.reading.percentage == 42)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "a broken notification sink stalled battery.v1 on the same session"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // And the session itself is healthy.
    let rtt = session.handle.ping(TIMEOUT).await;
    assert!(rtt.is_some(), "the session survived a broken sink");
}

#[tokio::test]
async fn a_malformed_notification_payload_does_not_close_the_session() {
    let (_server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        session
            .handle
            .send_capability(anyflow_core::capability::OutboundMessage {
                capability_id: CAPABILITY_ID.to_string(),
                payload: vec![0xff, 0xff, 0xff, 0xff],
            })
            .await
    );

    // The session is still there, and the capability still works.
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );
    assert!(session.handle.ping(TIMEOUT).await.is_some());
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_notification_content_is_written_to_disk() {
    const CANARY_TITLE: &str = "ANYFLOW-N2-STORE-TITLE";
    const CANARY_BODY: &str = "ANYFLOW-N2-STORE-BODY";
    const CANARY_APP: &str = "ANYFLOW-N2-STORE-APPLABEL";

    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;

    let mut message = upsert(1, CANARY_TITLE, CANARY_BODY);
    message.app_label = CANARY_APP.to_string();
    message.app_id = "canary.n2.store".to_string();
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(message)
        )
        .await
    );
    captured.next_result(TIMEOUT).await;

    // Every byte the daemon wrote, read back and searched.
    let mut files = Vec::new();
    collect_files(&server.data_dir(), &mut files);
    assert!(!files.is_empty(), "the daemon wrote something to audit");

    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        let text = String::from_utf8_lossy(&bytes);
        for canary in [CANARY_TITLE, CANARY_BODY, CANARY_APP, "canary.n2.store"] {
            assert!(
                !text.contains(canary),
                "{canary} was written to {}",
                path.display()
            );
        }
    }

    // And the policy that *is* persisted holds no content, by construction:
    // there is no field on it that could.
    let store_json = std::fs::read_to_string(server.data_dir().join("state.json"))
        .expect("the trust store exists");
    assert!(
        store_json.contains("notification_policy"),
        "the policy is persisted, so this test is looking at the right file"
    );
    for canary in [CANARY_TITLE, CANARY_BODY, CANARY_APP] {
        assert!(!store_json.contains(canary));
    }
}

fn collect_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}
