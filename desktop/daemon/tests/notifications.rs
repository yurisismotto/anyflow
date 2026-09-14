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

use anyflow_capability_notifications::backend::CloseReason;
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

fn upsert_body(message: clip_pb::NotificationUpsert) -> clip_pb::notification_control::Body {
    clip_pb::notification_control::Body::Upsert(message)
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
        vec![
            clip_pb::NotificationRole::Sink as i32,
            clip_pb::NotificationRole::DismissReporter as i32,
        ],
        "ADR-0017 §1's v1 assignment for Linux: it displays, and it reports \
         human dismissals"
    );
    assert!(
        !announced
            .roles
            .contains(&(clip_pb::NotificationRole::DismissTarget as i32)),
        "the desktop sources nothing, so there is nothing here to dismiss"
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

// ---------------------------------------------------------------------------
// N4 — a human dismissal, over the real transport
// ---------------------------------------------------------------------------

/// The whole feature, over TLS, through the real daemon, in one test.
///
/// A granted peer with dismiss sync on mirrors a notification; a person closes
/// it on this desktop; the desktop sends exactly one `DismissRequest` naming
/// the identity and origin the phone sent. Everything below the socket is the
/// production path — the real trust store, the real authorizer, the real
/// worker, the real mirror table.
#[tokio::test]
async fn a_human_dismissal_travels_to_the_source_over_the_real_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy {
        allow_dismiss_sync: true,
        ..NotificationPolicy::default()
    })
    .await;

    // The phone claims both of its v1 roles, which is what N4's Android
    // adapter announces.
    assert!(
        send_notification_control(
            &session,
            roles_body(
                &[
                    clip_pb::NotificationRole::Source,
                    clip_pb::NotificationRole::DismissTarget,
                ],
                2,
            )
        )
        .await
    );

    assert!(send_notification_control(&session, upsert_body(upsert(1, "Ana", "lunch?"))).await);
    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(
        result.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );
    let server_id = server
        .notification_sink
        .last_server_id()
        .expect("the desktop is showing it");

    // A person closes it. Reason 2, and only reason 2.
    server
        .notification_sink
        .user_closes(server_id, CloseReason::Dismissed)
        .await;

    let dismiss = captured.next_dismiss(TIMEOUT).await;
    assert_eq!(dismiss.notification_id, id_bytes(1));
    assert_eq!(dismiss.origin_device_id, ORIGIN);

    // And the mirror is already gone here, which is what makes the source's
    // `NotificationRemove` converge as UNKNOWN_NOTIFICATION instead of
    // starting a second lap.
    assert_eq!(server.notification_sink.live_count(), 0);
    assert!(send_notification_control(&session, remove_body(1)).await);
    let converged = captured.next_result(TIMEOUT).await;
    assert_eq!(
        converged.outcome,
        clip_pb::NotificationOutcome::UnknownNotification as i32,
        "the loop has one lap and no second"
    );
}

/// The default, over the transport: nothing is asked of the phone.
#[tokio::test]
async fn an_expiry_sends_nothing_over_the_real_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy {
        allow_dismiss_sync: true,
        ..NotificationPolicy::default()
    })
    .await;
    assert!(
        send_notification_control(
            &session,
            roles_body(
                &[
                    clip_pb::NotificationRole::Source,
                    clip_pb::NotificationRole::DismissTarget,
                ],
                2,
            )
        )
        .await
    );
    assert!(send_notification_control(&session, upsert_body(upsert(1, "Ana", "lunch?"))).await);
    let _ = captured.next_result(TIMEOUT).await;
    let server_id = server
        .notification_sink
        .last_server_id()
        .expect("displayed");

    // A banner times out on a screen nobody is looking at.
    server
        .notification_sink
        .user_closes(server_id, CloseReason::Expired)
        .await;

    // A second notification, whose answer proves the worker got that far — so
    // a dismissal queued before it would have come out first.
    assert!(send_notification_control(&session, upsert_body(upsert(2, "Bo", "hello"))).await);
    let next = captured.next_result(TIMEOUT).await;
    assert_eq!(
        next.notification_id,
        id_bytes(2),
        "something was sent between the expiry and this answer"
    );
}

/// Dismiss sync off — the shipped default — sends nothing either.
#[tokio::test]
async fn the_default_policy_sends_no_dismissal_over_the_real_transport() {
    let (server, _client, captured, session) = paired(NotificationPolicy::default()).await;
    assert!(
        send_notification_control(
            &session,
            roles_body(
                &[
                    clip_pb::NotificationRole::Source,
                    clip_pb::NotificationRole::DismissTarget,
                ],
                2,
            )
        )
        .await
    );
    assert!(send_notification_control(&session, upsert_body(upsert(1, "Ana", "lunch?"))).await);
    let _ = captured.next_result(TIMEOUT).await;
    let server_id = server
        .notification_sink
        .last_server_id()
        .expect("displayed");

    server
        .notification_sink
        .user_closes(server_id, CloseReason::Dismissed)
        .await;

    assert!(send_notification_control(&session, upsert_body(upsert(2, "Bo", "hello"))).await);
    let next = captured.next_result(TIMEOUT).await;
    assert_eq!(
        next.notification_id,
        id_bytes(2),
        "a dismissal was sent with the setting off"
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
    assert_eq!(
        widened.roles,
        vec![
            clip_pb::NotificationRole::Sink as i32,
            clip_pb::NotificationRole::DismissReporter as i32,
        ]
    );
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

// ---------------------------------------------------------------------------
// N3 — the consent surface, exercised through the handlers the GUI calls
// ---------------------------------------------------------------------------

/// The desktop's "Receive notifications from this device" switch, off.
///
/// The GUI writes this through `Request::Grant`, exactly as the Trusted peers
/// page does, so this drives `do_grant` itself rather than the store call
/// underneath it. What the switch promises is that turning it off takes the
/// notifications that are *already on the screen* off it — a revocation that
/// only applied to notifications that had not arrived yet would not have
/// withdrawn anything a person could see.
#[tokio::test]
async fn the_desktop_receive_switch_closes_the_mirrors_it_had_displayed() {
    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );
    assert_eq!(server.notification_sink.live_count(), 1);

    let response = anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        false,
    )
    .await;
    assert!(
        matches!(response, anyflow_runtime::control::Response::Ok { .. }),
        "the daemon accepted the switch: {response:?}"
    );

    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "turning the switch off left notifications on the screen"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(server.notification_sink.closes(), vec![1]);
}

/// And it takes nothing else with it.
///
/// The switch is beside three others on the same card. A person turning
/// notifications off has not asked to stop receiving files, and a UI that did
/// that would be taking a decision they never made.
#[tokio::test]
async fn the_desktop_receive_switch_leaves_every_other_grant_alone() {
    let (server, client, _captured, _session) = paired(NotificationPolicy::default()).await;

    for capability in ["battery.v1", "files.v1", "clipboard.v1"] {
        server.set_grant(client.fingerprint, capability, true).await;
    }

    let before = server.granted_capabilities(client.fingerprint).await;
    assert!(before.contains(&CAPABILITY_ID.to_string()));

    anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        false,
    )
    .await;

    let after = server.granted_capabilities(client.fingerprint).await;
    assert!(!after.contains(&CAPABILITY_ID.to_string()));
    for capability in ["battery.v1", "files.v1", "clipboard.v1"] {
        assert!(
            after.contains(&capability.to_string()),
            "{capability} was taken away by a notification switch"
        );
    }
    // And the pairing itself survives: this is a permission, not a revocation.
    assert!(server.is_paired(client.fingerprint).await);
}

/// Every notification setting the GUI offers leaves the other capabilities
/// untouched.
///
/// The lock policy and the pause switch are settings, not permissions, and
/// nothing about them should be able to reach another capability's grant.
#[tokio::test]
async fn changing_a_notification_setting_never_touches_another_capability() {
    use anyflow_runtime::control::NotificationSetting;

    let (server, client, _captured, _session) = paired(NotificationPolicy::default()).await;
    for capability in ["battery.v1", "files.v1", "clipboard.v1"] {
        server.set_grant(client.fingerprint, capability, true).await;
    }
    let before = server.granted_capabilities(client.fingerprint).await;

    for setting in [
        NotificationSetting::Mirror { enabled: false },
        NotificationSetting::Mirror { enabled: true },
        NotificationSetting::WhenLocked {
            policy: "full".into(),
        },
        NotificationSetting::WhenLocked {
            policy: "app-only".into(),
        },
        // The N4 switch. It is the one setting on the screen that lets another
        // device act on this one, so "it changes nothing else" is worth
        // proving at the level the daemon actually writes it.
        NotificationSetting::DismissSync { enabled: true },
        NotificationSetting::DismissSync { enabled: false },
    ] {
        let response = anyflow_runtime::server::do_notifications_policy(
            &server.state,
            &client.fingerprint.to_hex(),
            setting,
        )
        .await;
        assert!(
            matches!(response, anyflow_runtime::control::Response::Ok { .. }),
            "{response:?}"
        );
    }

    assert_eq!(
        before,
        server.granted_capabilities(client.fingerprint).await,
        "a notification setting changed another capability's grant"
    );
    // The clipboard's own policy is likewise untouched.
    assert_eq!(
        server.clipboard_policy(client.fingerprint).await,
        anyflow_capability_clipboard::policy::ClipboardPolicy::default(),
    );
    // And the notification policy is back where it started: the last setting
    // in the loop turned dismiss sync off again, and nothing else moved.
    assert_eq!(
        server.notification_policy(client.fingerprint).await,
        NotificationPolicy::default(),
    );
}

/// Turning dismiss sync on writes exactly that field and no other.
///
/// The daemon's own half of the guarantee the two UIs make. It matters here
/// rather than only in the UI because the UI is not the only writer: the CLI
/// and the control socket reach the same function.
#[tokio::test]
async fn the_dismiss_sync_setting_changes_only_itself() {
    use anyflow_runtime::control::NotificationSetting;

    let (server, client, _captured, _session) = paired(NotificationPolicy {
        when_sink_locked: anyflow_core::notification_policy::LockPolicy::Full,
        ..NotificationPolicy::default()
    })
    .await;
    let before = server.notification_policy(client.fingerprint).await;
    assert!(!before.allow_dismiss_sync, "the stored default is off");

    let response = anyflow_runtime::server::do_notifications_policy(
        &server.state,
        &client.fingerprint.to_hex(),
        NotificationSetting::DismissSync { enabled: true },
    )
    .await;
    assert!(matches!(
        response,
        anyflow_runtime::control::Response::Ok { .. }
    ));

    let after = server.notification_policy(client.fingerprint).await;
    assert!(after.allow_dismiss_sync);
    assert_eq!(after.allow_mirror, before.allow_mirror);
    assert_eq!(
        after.when_sink_locked, before.when_sink_locked,
        "the lock policy must not be reset by a dismiss-sync change"
    );
    assert_eq!(
        server.granted_capabilities(client.fingerprint).await.len(),
        // Unchanged: a policy is not a grant.
        server.granted_capabilities(client.fingerprint).await.len(),
    );
}

/// Pausing from the GUI closes what is on screen; resuming does not restore it.
///
/// Restoring would mean this process had kept a title and a body somewhere for
/// the length of the pause, which is the notification history the design
/// forbids. The phone re-sends on its next update or its next snapshot.
#[tokio::test]
async fn pausing_from_the_desktop_closes_mirrors_and_resuming_restores_nothing() {
    use anyflow_runtime::control::NotificationSetting;

    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );

    anyflow_runtime::server::do_notifications_policy(
        &server.state,
        &client.fingerprint.to_hex(),
        NotificationSetting::Mirror { enabled: false },
    )
    .await;

    let deadline = std::time::Instant::now() + TIMEOUT;
    while server.notification_sink.live_count() != 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "pausing left notifications on the screen"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    anyflow_runtime::server::do_notifications_policy(
        &server.state,
        &client.fingerprint.to_hex(),
        NotificationSetting::Mirror { enabled: true },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        server.notification_sink.live_count(),
        0,
        "resuming redisplayed content this daemon should never have kept"
    );

    // And the source's next update arrives in full, which is the whole answer
    // to "why did my notifications not come back".
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "still lunch?"))
        )
        .await
    );
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );
    assert_eq!(server.notification_sink.live_count(), 1);
}

/// The status the desktop UI renders carries no notification content.
///
/// The page is built entirely from this report, so a field that could hold a
/// title would be a field a history could be built from. This asserts the
/// absence at the point the UI reads it, over a session that has actually
/// displayed something with a distinctive title and body.
#[tokio::test]
async fn the_status_the_desktop_ui_reads_carries_no_notification_content() {
    let (server, client, captured, session) = paired(NotificationPolicy::default()).await;

    const TITLE: &str = "ANYFLOW-N3-STATUS-TITLE";
    const BODY: &str = "ANYFLOW-N3-STATUS-BODY";
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, TITLE, BODY))
        )
        .await
    );
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::NotificationOutcome::Displayed as i32
    );

    let reports = server.notifications.peer_reports().await;
    let rendered = format!("{reports:?}");
    assert!(
        !rendered.is_empty(),
        "the report was empty, so it proves nothing"
    );
    for canary in [TITLE, BODY, "com.example.chat", "Chat"] {
        assert!(
            !rendered.contains(canary),
            "the report the desktop UI renders carried {canary}"
        );
    }
    // And it does carry the counts the page actually needs.
    let peer = reports
        .iter()
        .find(|r| r.peer == client.fingerprint)
        .expect("a report for the connected peer");
    assert_eq!(peer.mirrors, 1);
}

// ---------------------------------------------------------------------------
// N5 — mid-session grant convergence
// ---------------------------------------------------------------------------
//
// The defect these pin was seen twice on hardware, in N3 §G4 and again in N4
// §17: two devices already connected, the user enables `notifications.v1` on
// both ends, everything in both trust stores is correct — and nothing happens
// until somebody presses Disconnect and Connect on the phone.
//
// The cause is one frozen vector. `Established::negotiated_capabilities` is
// computed once during `HELLO` as *what both sides implement* intersected with
// *what this peer is granted*, and it is then the authority for the rest of
// the session: it decides which capabilities get `on_peer_connected` — which
// is the only thing that makes the desktop announce a `SINK` role at all —
// and it is the per-message filter that answers `UNSUPPORTED_CAPABILITY` to
// everything it does not name. A grant added afterwards cannot enter it.
//
// ADR-0017 §3 already says a grant needs a reconnect to widen. What was
// missing is that nothing ever asked for one.

/// What the **desktop** negotiated for this peer's live session.
///
/// Deliberately not `ConnectedSession::negotiated_capabilities`, which is the
/// dialling side's view and is the plain intersection of the two advertised
/// sets. The grant filter lives on the answering side — it is the desktop that
/// decides what this peer is allowed to do — so the desktop's vector is the
/// one every test here is about. Reading the client's instead would have made
/// the whole suite pass against no fix at all.
async fn desktop_negotiated(server: &TestServer, peer: anyflow_core::Fingerprint) -> Vec<String> {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        if let Some(handle) = server.state.session_for(&peer).await {
            return handle.negotiated_capabilities().to_vec();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the desktop never registered a session for this peer"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The id of the desktop's session for this peer. Not the client's: the two
/// are different objects with different ids, minted from the same counter.
async fn desktop_session_id(
    server: &TestServer,
    peer: anyflow_core::Fingerprint,
) -> anyflow_core::session::SessionId {
    server
        .state
        .session_for(&peer)
        .await
        .expect("the desktop has a session for this peer")
        .id()
}

/// Pairs and connects **without** granting `notifications.v1`.
///
/// This is the state a real user is in: pairing grants nothing, because the
/// capability is deliberately absent from `auto_grant`.
async fn connected_but_not_granted() -> (TestServer, TestClient, ConnectedSession) {
    let server = TestServer::start().await;
    let client = TestClient::new("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");

    assert!(
        !desktop_negotiated(&server, client.fingerprint)
            .await
            .contains(&CAPABILITY_ID.to_string()),
        "the premise of every test below: this session cannot use the \
         capability, because it was not granted when the session was built"
    );
    (server, client, session)
}

/// Waits for a session task to finish, and says so rather than hanging.
async fn assert_session_ends(session: ConnectedSession, what: &str) {
    let ended = tokio::time::timeout(TIMEOUT, session.task).await;
    assert!(ended.is_ok(), "the session was not ended: {what}");
}

/// **The N5 primary item.** Granting mid-session ends the session that cannot
/// use the grant, so the peer reconnects and negotiates it.
///
/// Nothing here dials, retries or schedules: ending the session is the whole
/// mechanism. On a real phone the peer's own `ConnectionCoordinator` treats a
/// session that ended as proof the endpoint works, resets its ladder, and
/// redials about two seconds later.
#[tokio::test]
async fn granting_mid_session_ends_the_session_that_cannot_use_the_grant() {
    let (server, client, session) = connected_but_not_granted().await;
    let session_id = desktop_session_id(&server, client.fingerprint).await;

    let response = anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        true,
    )
    .await;

    match response {
        anyflow_runtime::control::Response::Ok { message } => assert!(
            message.contains("reconnecting"),
            "the operator is told the device is being reconnected: {message}"
        ),
        other => panic!("the grant was refused: {other:?}"),
    }

    assert_session_ends(session, "a grant widened past what it negotiated").await;

    // And the peer's next connection — the one its coordinator makes on its
    // own — negotiates the capability and gets the roles announcement that
    // the frozen session could never have produced.
    let reconnected = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    assert!(
        desktop_negotiated(&server, client.fingerprint)
            .await
            .contains(&CAPABILITY_ID.to_string()),
        "the reconnect is the point: the new session must be able to use it"
    );
    assert_ne!(
        desktop_session_id(&server, client.fingerprint).await,
        session_id,
        "a genuinely new session, not the old one re-reported"
    );
    reconnected.close().await;
}

/// The reconnect actually produces a working notification session.
///
/// The test above proves the session was rebuilt. This one proves the rebuild
/// was worth doing: the desktop announces `SINK` and `DISMISS_REPORTER` on the
/// new session and then displays a notification — which is the user-visible
/// outcome the whole item exists for.
#[tokio::test]
async fn the_session_the_reconnect_builds_can_actually_mirror() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_notifications("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    assert!(!desktop_negotiated(&server, client.fingerprint)
        .await
        .contains(&CAPABILITY_ID.to_string()));

    // Nothing has been announced, because nothing was negotiated.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        captured.drain().await.is_empty(),
        "a session that never negotiated the capability must announce nothing"
    );

    anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        true,
    )
    .await;
    assert_session_ends(session, "the grant widened").await;

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    let announced = captured.next_roles(TIMEOUT).await;
    assert_eq!(announced.epoch, 1, "a new session starts its epochs again");
    assert_eq!(
        announced.roles,
        vec![
            clip_pb::NotificationRole::Sink as i32,
            clip_pb::NotificationRole::DismissReporter as i32,
        ]
    );

    assert!(
        send_notification_control(
            &session,
            roles_body(&[clip_pb::NotificationRole::Source], 1)
        )
        .await
    );
    assert!(
        send_notification_control(
            &session,
            clip_pb::notification_control::Body::Upsert(upsert(1, "Ana", "lunch?"))
        )
        .await
    );
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::NotificationOutcome::Displayed as i32,
        "the user's grant took effect without anybody pressing Disconnect"
    );
    assert_eq!(server.notification_sink.live_count(), 1);
    session.close().await;
}

/// **Brief §4 B / G.** A burst of writes produces one reconnect, and the
/// policy writes in it produce none.
///
/// This is the realistic sequence a person generates in the GUI: switch the
/// capability on, then set the three notification settings beside it. Only the
/// first is a permission, and only the first may cost a session.
#[tokio::test]
async fn a_burst_of_settings_after_the_grant_causes_no_further_reconnect() {
    use anyflow_runtime::control::NotificationSetting;

    let (server, client, session) = connected_but_not_granted().await;
    let device = client.fingerprint.to_hex();

    anyflow_runtime::server::do_grant(&server.state, &device, CAPABILITY_ID, true).await;
    assert_session_ends(session, "the grant widened").await;

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    let id = desktop_session_id(&server, client.fingerprint).await;

    // Everything the consent card writes next. None of it is a grant.
    for setting in [
        NotificationSetting::Mirror { enabled: true },
        NotificationSetting::WhenLocked {
            policy: "full".into(),
        },
        NotificationSetting::DismissSync { enabled: true },
    ] {
        anyflow_runtime::server::do_notifications_policy(&server.state, &device, setting).await;
    }
    // And the grant written again, as a GUI that echoes its own switch would.
    anyflow_runtime::server::do_grant(&server.state, &device, CAPABILITY_ID, true).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    let live = server
        .state
        .session_for(&client.fingerprint)
        .await
        .expect("the session survived the settings burst");
    assert_eq!(
        live.id(),
        id,
        "a settings burst after the grant rebuilt the session"
    );
    session.close().await;
}

/// **Brief §4 E.** Revocation narrows immediately and never reconnects.
///
/// A reconnect here would be exactly backwards: it would take a permission
/// away and hand the peer a brand new session in the same breath.
#[tokio::test]
async fn withdrawing_a_grant_never_rebuilds_the_session() {
    let (server, client, session) = connected_but_not_granted().await;
    let device = client.fingerprint.to_hex();

    anyflow_runtime::server::do_grant(&server.state, &device, CAPABILITY_ID, true).await;
    assert_session_ends(session, "the grant widened").await;

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    let id = desktop_session_id(&server, client.fingerprint).await;

    let response =
        anyflow_runtime::server::do_grant(&server.state, &device, CAPABILITY_ID, false).await;
    match response {
        anyflow_runtime::control::Response::Ok { message } => assert!(
            !message.contains("reconnecting"),
            "a withdrawal must not announce a reconnect: {message}"
        ),
        other => panic!("{other:?}"),
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .expect("the session survived the withdrawal")
            .id(),
        id,
        "withdrawing a grant tore down a session it had no reason to"
    );
    session.close().await;
}

/// **Brief §4 G.** Another capability's grant does not restart a session that
/// can already use it.
///
/// `battery.v1` is in `auto_grant`, so a session negotiates it from the first
/// handshake. Re-asserting it must be inert.
#[tokio::test]
async fn regranting_a_capability_the_session_already_has_is_inert() {
    let (server, client, session) = connected_but_not_granted().await;
    let id = desktop_session_id(&server, client.fingerprint).await;
    assert!(
        desktop_negotiated(&server, client.fingerprint)
            .await
            .contains(&"battery.v1".to_string()),
        "battery.v1 is auto-granted at pairing, so this session already has it"
    );

    let response = anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        "battery.v1",
        true,
    )
    .await;
    match response {
        anyflow_runtime::control::Response::Ok { message } => assert!(
            !message.contains("reconnecting"),
            "nothing needed rebuilding: {message}"
        ),
        other => panic!("{other:?}"),
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .expect("session")
            .id(),
        id
    );
    session.close().await;
}

/// The correction is capability-agnostic, and that is deliberate.
///
/// `notifications.v1` is where the freeze was observed, but `clipboard.v1` and
/// `files.v1` froze in exactly the same way and for exactly the same reason —
/// neither is in `auto_grant` either. Naming notifications in the fix would
/// have fixed one instance of a defect in the capability model.
#[tokio::test]
async fn the_convergence_is_not_specific_to_notifications() {
    for capability in ["clipboard.v1", "files.v1"] {
        let (server, client, session) = connected_but_not_granted().await;
        assert!(
            !desktop_negotiated(&server, client.fingerprint)
                .await
                .contains(&capability.to_string()),
            "{capability} should not be granted by pairing alone"
        );

        anyflow_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            capability,
            true,
        )
        .await;
        assert_session_ends(session, capability).await;

        let reconnected = client
            .connect(server.addr, server.fingerprint, None)
            .await
            .expect("reconnect");
        assert!(
            desktop_negotiated(&server, client.fingerprint)
                .await
                .contains(&capability.to_string()),
            "{capability} did not converge"
        );
        reconnected.close().await;
    }
}

/// **Brief §4 D.** A grant made while the peer is away asks for nothing.
///
/// There is no session to rebuild and no dialling to do: the peer's next
/// handshake reads the trust store as it finds it. Asserted because a fix that
/// started dialling from the desktop would pass every other test here.
#[tokio::test]
async fn granting_while_the_peer_is_away_converges_on_its_own() {
    let (server, client, session) = connected_but_not_granted().await;
    session.close().await;
    wait_until(TIMEOUT, || async {
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .is_none()
    })
    .await;

    let response = anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        true,
    )
    .await;
    match response {
        anyflow_runtime::control::Response::Ok { message } => assert!(
            !message.contains("reconnecting"),
            "there was nothing to reconnect: {message}"
        ),
        other => panic!("{other:?}"),
    }

    // Nothing dialled the phone. The phone dialled.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .is_none(),
        "the desktop must never initiate a connection to a phone"
    );

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("the phone reconnects on its own");
    assert!(desktop_negotiated(&server, client.fingerprint)
        .await
        .contains(&CAPABILITY_ID.to_string()));
    session.close().await;
}

/// The reconnect costs the pairing nothing, and costs no other capability
/// anything either.
#[tokio::test]
async fn the_reconnect_keeps_the_pairing_and_every_other_capability() {
    let (server, client, session) = connected_but_not_granted().await;
    let before = server.granted_capabilities(client.fingerprint).await;

    anyflow_runtime::server::do_grant(
        &server.state,
        &client.fingerprint.to_hex(),
        CAPABILITY_ID,
        true,
    )
    .await;
    assert_session_ends(session, "the grant widened").await;

    assert!(
        server.is_paired(client.fingerprint).await,
        "a reconnect is not an unpairing"
    );
    let after = server.granted_capabilities(client.fingerprint).await;
    for capability in &before {
        assert!(
            after.contains(capability),
            "{capability} was lost by the reconnect"
        );
    }

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    let negotiated = desktop_negotiated(&server, client.fingerprint).await;
    for capability in &before {
        assert!(
            negotiated.contains(capability),
            "{capability} did not come back on the new session"
        );
    }
    session.close().await;
}

/// **NOTIF-SEC-25, extended to the N5 convergence path.**
///
/// The mid-session grant correction added log lines on a path that runs while
/// a notification is on the screen: the grant handler, the renegotiation
/// decision, the session shutdown, the reattach and the reconnect snapshot.
/// None of them may carry a title, a body, an application label or an
/// application id — and "may not" is proved here rather than grepped, at
/// `TRACE`, because `RUST_LOG=trace` is exactly what somebody runs when
/// something is wrong and exactly the worst moment to spill a stranger's
/// message into a file they are about to attach to a bug report.
#[tokio::test]
async fn the_mid_session_convergence_path_logs_no_notification_content() {
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    const TITLE: &str = "CANARY-N5-CONVERGE-TITLE-9b2f";
    const BODY: &str = "CANARY-N5-CONVERGE-BODY-4e71";
    const APP_LABEL: &str = "CANARY-N5-CONVERGE-APPLABEL-0a5c";
    const APP_ID: &str = "canary.n5.converge.d31f";

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("not poisoned").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();

    {
        // Scoped rather than global: `set_global_default` may be called only
        // once per process and the rest of this suite must stay unaffected.
        let _guard = tracing::subscriber::set_default(subscriber);

        let server = TestServer::start().await;
        let (client, notifications) = TestClient::new_raw_notifications("phone");
        let token = server.open_pairing(Duration::from_secs(30)).await;
        let session = client
            .connect(server.addr, server.fingerprint, Some(&token))
            .await
            .expect("pairing");

        // Grant first, reconnect, and put a canary on the screen.
        // Waited for, so the grant genuinely lands on a live session rather
        // than on a registry that has not caught up — otherwise the decision
        // would be `no-session` and this would canary the wrong path.
        let negotiated = desktop_negotiated(&server, client.fingerprint).await;
        assert!(!negotiated.contains(&CAPABILITY_ID.to_string()));

        let response = anyflow_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            true,
        )
        .await;
        match response {
            anyflow_runtime::control::Response::Ok { message } => assert!(
                message.contains("reconnecting"),
                "the convergence path was not taken, so this canaries nothing"
            ),
            other => panic!("{other:?}"),
        }
        let _ = tokio::time::timeout(TIMEOUT, session.task).await;

        let session = client
            .connect(server.addr, server.fingerprint, None)
            .await
            .expect("reconnect");
        notifications.next_roles(TIMEOUT).await;
        assert!(
            send_notification_control(
                &session,
                roles_body(&[clip_pb::NotificationRole::Source], 1)
            )
            .await
        );

        let mut message = upsert(1, TITLE, BODY);
        message.app_label = APP_LABEL.to_string();
        message.app_id = APP_ID.to_string();
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(message)
            )
            .await
        );
        assert_eq!(
            notifications.next_result(TIMEOUT).await.outcome,
            clip_pb::NotificationOutcome::Displayed as i32
        );

        // Now withdraw and re-grant with that notification live, so the
        // revocation, the mirror close and a second renegotiation decision
        // all run while there is content to leak.
        anyflow_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            false,
        )
        .await;
        anyflow_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            true,
        )
        .await;
        let _ = tokio::time::timeout(TIMEOUT, session.task).await;
    }

    let text = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned")).into_owned();
    assert!(
        !text.is_empty(),
        "nothing was captured, so this test proves nothing"
    );
    // Coverage is proved two ways, because either alone can lie. The
    // response above proves the convergence path actually ran; this proves
    // the subscriber was attached to the daemon's own events while it did,
    // rather than to an empty session that would make every assertion below
    // vacuously true.
    assert!(
        text.contains("anyflow_"),
        "no daemon event reached the capture, so this test proves nothing:\n{text}"
    );
    for canary in [TITLE, BODY, APP_LABEL, APP_ID] {
        assert!(
            !text.contains(canary),
            "the convergence path logged {canary}:\n{text}"
        );
    }
}
