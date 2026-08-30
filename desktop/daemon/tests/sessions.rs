//! Session-lifecycle regressions.
//!
//! These cover the "zombie session" defect: `anyflow status` reporting a
//! device as connected when its session was already gone, and the mirror
//! image of it — a live session being evicted by an older one's late close.
//!
//! Nothing here weakens the transport. Every session below is a real TLS
//! connection through the real listener.

mod common;

use std::time::Duration;

use anyflow_core::session::SessionHost;
use common::{wait_until, TestClient, TestServer};

/// Pairs a client with the server and returns a live session.
async fn paired_session(server: &TestServer, client: &TestClient) -> common::ConnectedSession {
    let token = server.open_pairing(Duration::from_secs(30)).await;
    client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair and connect")
}

#[tokio::test]
async fn a_session_that_ends_is_removed_from_the_registry() {
    let server = TestServer::start().await;
    let client = TestClient::new("Phone");

    let session = paired_session(&server, &client).await;
    let peer = client.fingerprint;

    wait_until(Duration::from_secs(5), || async {
        server.state.session_for(&peer).await.is_some()
    })
    .await;

    session.close().await;

    // The point of the test: "no longer connected" must become true on its
    // own, promptly, without anyone asking the daemon to re-check.
    wait_until(Duration::from_secs(5), || async {
        server.state.session_for(&peer).await.is_none()
    })
    .await;
}

/// The id the *daemon* has for the current session with `peer`.
///
/// Deliberately not the client's own id: the two ends of one connection run
/// separate session loops with separate ids, and the registry under test is
/// the daemon's.
async fn daemon_session_id(
    server: &TestServer,
    peer: &anyflow_core::Fingerprint,
) -> Option<anyflow_core::session::SessionId> {
    server.state.session_for(peer).await.map(|h| h.id())
}

#[tokio::test]
async fn a_reconnection_replaces_the_previous_session_rather_than_duplicating_it() {
    let server = TestServer::start().await;
    let client = TestClient::new("Phone");

    let first = paired_session(&server, &client).await;
    let peer = client.fingerprint;

    wait_until(Duration::from_secs(5), || async {
        daemon_session_id(&server, &peer).await.is_some()
    })
    .await;
    let first_id = daemon_session_id(&server, &peer).await.expect("first");

    // Reconnect while the first session is still registered. This is what a
    // phone does after a Wi-Fi drop: the old socket has not died yet.
    let second = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    // Exactly one session for the peer, and it is the new one.
    wait_until(Duration::from_secs(5), || async {
        daemon_session_id(&server, &peer)
            .await
            .is_some_and(|id| id != first_id)
    })
    .await;
    assert_eq!(
        server.state.session_handles().await.len(),
        1,
        "one device must never hold two sessions"
    );

    // The displaced session is shut down rather than left to rot: the phone
    // sees its old socket close.
    wait_until(Duration::from_secs(5), || async { !first.handle.is_live() }).await;

    second.close().await;
}

#[tokio::test]
async fn a_superseded_sessions_late_close_does_not_evict_the_live_one() {
    let server = TestServer::start().await;
    let client = TestClient::new("Phone");

    let first = paired_session(&server, &client).await;
    let peer = client.fingerprint;

    wait_until(Duration::from_secs(5), || async {
        daemon_session_id(&server, &peer).await.is_some()
    })
    .await;
    let first_id = daemon_session_id(&server, &peer).await.expect("first");

    let second = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    wait_until(Duration::from_secs(5), || async {
        daemon_session_id(&server, &peer)
            .await
            .is_some_and(|id| id != first_id)
    })
    .await;
    let second_id = daemon_session_id(&server, &peer).await.expect("second");

    // The displaced session now finishes dying, *after* its replacement was
    // registered. Unregistering by fingerprint alone would remove the live
    // session here and report a connected phone as offline — the defect this
    // guards against. Closing from the phone's end too makes sure the old
    // session is really gone before we look.
    first.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(
        daemon_session_id(&server, &peer).await,
        Some(second_id),
        "the live session must survive the older session's close"
    );

    second.close().await;
}

#[tokio::test]
async fn revoking_a_device_leaves_no_session_behind() {
    let server = TestServer::start().await;
    let client = TestClient::new("Phone");

    let session = paired_session(&server, &client).await;
    let peer = client.fingerprint;
    assert!(server.state.session_for(&peer).await.is_some());

    {
        let mut store = server.state.store.lock().await;
        assert!(store.revoke_peer(&peer).expect("revoke"));
    }
    if let Some(handle) = server.state.session_for(&peer).await {
        handle.shutdown().await;
    }
    server.state.drop_session(&peer).await;

    assert!(server.state.session_for(&peer).await.is_none());
    assert!(server.state.last_seen(&peer).await.is_some());

    // Trust is revoked, not merely absent: the device is still listed.
    let status = server.state.lookup_peer(&peer).await;
    assert_eq!(status, anyflow_core::session::PeerStatus::Revoked);

    let _ = session.task.await;
}

#[tokio::test]
async fn battery_telemetry_is_dropped_when_the_session_ends() {
    let server = TestServer::start().await;
    let client = TestClient::new("Phone");

    let session = paired_session(&server, &client).await;
    let peer = client.fingerprint;

    session
        .handle
        .send_capability(anyflow_capability_battery::BatteryCapability::encode(
            &anyflow_capability_battery::BatteryReading {
                percentage: 57,
                charging_state: anyflow_proto::v1::capabilities::ChargingState::Charging,
                peer_timestamp_unix_ms: 1,
            },
        ))
        .await;

    wait_until(Duration::from_secs(5), || async {
        server
            .battery
            .get(&peer)
            .is_some_and(|b| b.reading.percentage == 57)
    })
    .await;

    session.close().await;

    // A percentage with no session behind it is exactly the "connected,
    // battery 57%" lie this suite exists to prevent.
    wait_until(Duration::from_secs(5), || async {
        server.battery.get(&peer).is_none()
    })
    .await;
}

/// A peer that stops talking without closing its socket must not stay listed
/// as connected forever.
///
/// This is the half-open TCP case: a phone that walks out of Wi-Fi range
/// sends no FIN and no RST, so the kernel has nothing to report and `read`
/// simply never returns. The session's own liveness probe is the only thing
/// that notices.
///
/// The clock is paused only *after* the handshake. Pausing it earlier would
/// let virtual time run away from the real socket and trip the handshake
/// timeout before the peers had finished talking.
#[tokio::test]
async fn a_silent_peer_is_eventually_dropped_rather_than_left_connected() {
    use anyflow_core::session::ClientHandshake;

    let server = TestServer::start().await;
    let client = TestClient::new("Phone");
    let peer = client.fingerprint;

    let token = server.open_pairing(Duration::from_secs(30)).await;

    // Handshake, then deliberately never run a session loop. The socket stays
    // open — the test holds it — but nothing is ever sent or read again.
    let mut tls = client
        .tls_connect(server.addr, server.fingerprint)
        .await
        .expect("tls");
    let handshake = anyflow_core::session::connect_handshake(
        &mut tls,
        &client.host,
        server.fingerprint,
        Some(&token),
    )
    .await
    .expect("handshake");
    assert!(matches!(handshake, ClientHandshake::Established(_, _)));

    wait_until(Duration::from_secs(30), || async {
        server.state.session_for(&peer).await.is_some()
    })
    .await;

    // From here on the only thing that should happen is the passage of time,
    // so hand the clock to the runtime and let it skip ahead.
    tokio::time::pause();

    let mut dropped = false;
    // 400 virtual seconds, well past LIVENESS_DEAD_AFTER (180s).
    for _ in 0..400 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if server.state.session_for(&peer).await.is_none() {
            dropped = true;
            break;
        }
    }
    assert!(
        dropped,
        "a peer that stopped responding must not stay listed as connected"
    );

    // Trust survives: the link died, the pairing did not.
    assert_eq!(
        server.state.lookup_peer(&peer).await,
        anyflow_core::session::PeerStatus::Trusted {
            device_id: client.identity.device_id().to_string(),
            device_name: "Phone".to_string(),
            granted_capabilities: vec!["battery.v1".to_string()],
        },
    );

    drop(tls);
}
