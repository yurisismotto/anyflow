//! End-to-end tests over a real TLS 1.3 connection on loopback.
//!
//! These exercise the acceptance criteria for this Sprint: pairing, trusted
//! reconnection, ping/pong, `battery.v1`, and the rejection paths that make
//! the whole thing worth having.

mod common;

use std::sync::Arc;
use std::time::Duration;

use anyflow_capability_battery::{BatteryCapability, BatteryReading};
use anyflow_core::error::PairingError;
use anyflow_core::identity::LocalIdentity;
use anyflow_core::pairing::PairingToken;
use anyflow_core::session::{PeerStatus, SessionHost};
use anyflow_core::Error;
use anyflow_proto::v1;
use anyflow_proto::v1::capabilities::ChargingState;
use common::{wait_until, TestClient, TestServer};

const TTL: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pairing_then_ping_pong_then_battery() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    // --- pair -------------------------------------------------------------
    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing should succeed");

    // The desktop now trusts the phone, by pinned key.
    assert!(
        matches!(
            server.state.lookup_peer(&phone.fingerprint).await,
            PeerStatus::Trusted { .. }
        ),
        "server must record the phone as trusted"
    );
    // And the phone trusts the desktop.
    assert!(phone.is_trusted(&server.fingerprint).await);

    // --- capabilities were negotiated -------------------------------------
    //
    // This is the *mutually supported* set, which is not the same as what the
    // phone is allowed to do. `files.v1`, `clipboard.v1` and
    // `notifications.v1` appear here because both sides implement them, and
    // none of them is granted: the desktop's auto-grant policy covers
    // `battery.v1` only, because writing files, writing a clipboard and
    // putting somebody's messages on a screen are all side effects (ADR-0008,
    // ADR-0015 §4). The two lists being different is the point.
    assert_eq!(
        session.negotiated_capabilities,
        vec![
            "battery.v1".to_string(),
            "clipboard.v1".to_string(),
            "files.v1".to_string(),
            "notifications.v1".to_string()
        ]
    );
    let store = server.state.store.lock().await;
    let peer = store.trusted_peer(&phone.fingerprint).expect("paired");
    assert!(peer.allows("battery.v1"), "battery is auto-granted");
    assert!(!peer.allows("files.v1"), "files must not be");
    assert!(!peer.allows("clipboard.v1"), "nor the clipboard");
    assert!(!peer.allows("notifications.v1"), "nor notifications");
    assert!(!peer.allows("clipboard.v1"), "clipboard must not be either");
    drop(store);

    let granted: Vec<String> = {
        let store = server.state.store.lock().await;
        store
            .peer_record(&phone.fingerprint)
            .map(|p| {
                p.granted_capabilities
                    .iter()
                    .filter(|(_, g)| **g)
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        granted,
        vec!["battery.v1".to_string()],
        "advertising files.v1 must not grant it"
    );

    // --- ping / pong ------------------------------------------------------
    let rtt = session
        .handle
        .ping(Duration::from_secs(5))
        .await
        .expect("PONG must come back");
    assert!(rtt < Duration::from_secs(5));

    // --- battery.v1 -------------------------------------------------------
    let reading = BatteryReading {
        percentage: 87,
        charging_state: ChargingState::Charging,
        peer_timestamp_unix_ms: 1_700_000_000_000,
    };
    assert!(
        session
            .handle
            .send_capability(BatteryCapability::encode(&reading))
            .await
    );

    let battery = Arc::clone(&server.battery);
    let peer = phone.fingerprint;
    wait_until(Duration::from_secs(5), || {
        let battery = Arc::clone(&battery);
        async move { battery.get(&peer).is_some() }
    })
    .await;

    let stored = server.battery.get(&phone.fingerprint).expect("reading");
    assert_eq!(stored.reading.percentage, 87);
    assert_eq!(stored.reading.charging_state, ChargingState::Charging);

    session.close().await;
}

#[tokio::test]
async fn a_paired_device_reconnects_without_pairing_again() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    let first = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("initial pairing");
    first.close().await;

    // Simulates losing Wi-Fi and coming back: no pairing window is open, no
    // token is offered, and the identity is the persisted one.
    server.state.end_pairing().await;
    let second = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("a trusted device must reconnect with no token");

    assert_eq!(
        second.negotiated_capabilities,
        vec![
            "battery.v1".to_string(),
            "clipboard.v1".to_string(),
            "files.v1".to_string(),
            "notifications.v1".to_string()
        ]
    );
    assert!(second.handle.ping(Duration::from_secs(5)).await.is_some());
    second.close().await;
}

#[tokio::test]
async fn several_pings_round_trip_on_one_connection() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");
    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    // Exercises sequence numbering and the de-duplication window across
    // many messages: each ping must produce a fresh, distinct envelope.
    for _ in 0..10 {
        assert!(
            session.handle.ping(Duration::from_secs(5)).await.is_some(),
            "every ping must be answered"
        );
    }
    session.close().await;
}

// ---------------------------------------------------------------------------
// Trust boundaries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unpaired_device_cannot_use_the_protocol() {
    let server = TestServer::start().await;
    let stranger = TestClient::new("Attacker");

    // No pairing window is open. The stranger is reachable on the network,
    // completes TLS, and must still get nothing.
    let err = stranger
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("an unpaired device must be refused");

    assert!(
        matches!(err, Error::Pairing(PairingError::NotInPairingMode)),
        "unexpected error: {err:?}"
    );
    assert!(matches!(
        server.state.lookup_peer(&stranger.fingerprint).await,
        PeerStatus::Unknown
    ));
}

#[tokio::test]
async fn a_different_server_identity_is_rejected_by_the_client() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    // The phone expects a different key than the one the server holds — the
    // exact situation a MITM would create.
    let impostor = LocalIdentity::generate("Impostor", v1::Platform::Linux)
        .expect("identity")
        .fingerprint();

    let err = phone
        .tls_connect(server.addr, impostor)
        .await
        .expect_err("pinning must reject a different identity");

    // The rejection must come from the certificate verifier during the
    // handshake, not from any later application-layer check. tokio-rustls
    // surfaces handshake failures wrapped in an io::Error, so unwrap it and
    // assert on the exact rustls cause.
    let cause = match &err {
        Error::Tls(e) => Some(e.clone()),
        Error::Io(io) => io
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
            .cloned(),
        _ => None,
    }
    .unwrap_or_else(|| panic!("expected a TLS error, got: {err:?}"));

    assert!(
        matches!(
            cause,
            rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure
            )
        ),
        "expected the pinning verifier to reject, got: {cause:?}"
    );
}

#[tokio::test]
async fn a_revoked_device_is_refused_on_its_next_connection() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");
    session.close().await;
    server.state.end_pairing().await;

    // The user runs `anyflow unpair`.
    {
        let mut store = server.state.store.lock().await;
        assert!(store.revoke_peer(&phone.fingerprint).expect("revoke"));
    }

    let err = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("a revoked device must be refused");
    assert!(
        matches!(err, Error::NotAuthorized),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn a_revoked_device_cannot_re_pair_without_a_new_token() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;

    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&phone.fingerprint).expect("revoke");
    }

    // Replaying the original token must not work: it was consumed, and the
    // window is closed.
    let err = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("a consumed token must not resurrect a revoked device");
    assert!(
        matches!(err, Error::NotAuthorized),
        "unexpected error: {err:?}"
    );
}

/// Revocation must be reversible by the owner, or `anyflow unpair` is a
/// permanent brick rather than a control.
///
/// The defect this covers was found on real hardware during G17: a revoked
/// fingerprint was refused at HELLO, *before* it could be offered a pairing
/// nonce, so the phone could never pair again through the documented flow —
/// scanning a fresh QR just produced "this device's pairing was revoked"
/// forever. The trust store was always willing to take it back; the handshake
/// never let it get that far.
#[tokio::test]
async fn a_revoked_device_can_pair_again_when_the_owner_opens_a_new_window() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;

    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&phone.fingerprint).expect("revoke");
    }

    // While no window is open it stays out. Revocation is still revocation.
    let err = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("a revoked device must be refused with no window open");
    assert!(matches!(err, Error::NotAuthorized), "{err:?}");

    // The owner runs `anyflow pair` again and confirms at the terminal.
    let fresh = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&fresh))
        .await
        .expect("a revoked device must be able to pair again");

    {
        let store = server.state.store.lock().await;
        let record = store.peer_record(&phone.fingerprint).expect("record");
        assert!(!record.revoked, "pairing again clears the revocation");
        assert!(
            record.granted_capabilities.values().any(|g| *g),
            "and restores the grants the auto-grant policy allows"
        );
    }

    session.close().await;
}

/// Letting a revoked device back in must cost it a full pairing ceremony,
/// not merely the existence of an open window.
#[tokio::test]
async fn a_revoked_device_still_needs_the_right_token_and_a_human() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;
    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&phone.fingerprint).expect("revoke");
    }

    // A window is open, but the device offers the old, consumed token.
    server.open_pairing(TTL).await;
    let err = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("a stale token must not readmit a revoked device");
    assert!(matches!(err, Error::Pairing(_)), "{err:?}");
    {
        let store = server.state.store.lock().await;
        assert!(
            store
                .peer_record(&phone.fingerprint)
                .expect("record")
                .revoked,
            "a failed attempt must leave the revocation in place"
        );
    }
    server.state.end_pairing().await;

    // A window whose operator declines must not readmit it either.
    let fresh = server.open_pairing_with(TTL, false).await;
    let err = phone
        .connect(server.addr, server.fingerprint, Some(&fresh))
        .await
        .expect_err("a declined confirmation must not readmit a revoked device");
    assert!(matches!(err, Error::Pairing(_)), "{err:?}");
    {
        let store = server.state.store.lock().await;
        assert!(
            store
                .peer_record(&phone.fingerprint)
                .expect("record")
                .revoked,
            "declining leaves the device revoked"
        );
    }
}

// ---------------------------------------------------------------------------
// Pairing failure modes, over the wire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_wrong_pairing_token_is_rejected() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let _real = server.open_pairing(TTL).await;
    let guessed = PairingToken::generate().expect("token");

    let err = phone
        .connect(server.addr, server.fingerprint, Some(&guessed))
        .await
        .expect_err("a wrong token must be rejected");
    assert!(
        matches!(err, Error::Pairing(PairingError::BadProof)),
        "unexpected error: {err:?}"
    );
    assert!(matches!(
        server.state.lookup_peer(&phone.fingerprint).await,
        PeerStatus::Unknown
    ));
}

#[tokio::test]
async fn an_expired_pairing_window_rejects_a_valid_token() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(Duration::from_millis(1)).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let err = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("an expired window must reject even the correct token");
    // The window is no longer advertised as active, so the server never
    // issues a nonce.
    assert!(
        matches!(err, Error::Pairing(_)),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn a_token_cannot_pair_a_second_device() {
    let server = TestServer::start().await;
    let first = TestClient::new("Galaxy S25");
    let second = TestClient::new("Second Phone");

    let token = server.open_pairing(TTL).await;
    first
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("first device pairs")
        .close()
        .await;

    // The same QR, scanned by another phone moments later.
    let err = second
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("a single-use token must not pair twice");
    assert!(
        matches!(err, Error::Pairing(_)),
        "unexpected error: {err:?}"
    );
    assert!(matches!(
        server.state.lookup_peer(&second.fingerprint).await,
        PeerStatus::Unknown
    ));
}

#[tokio::test]
async fn a_declined_confirmation_does_not_pair() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    // Correct token, but the human at the desktop says no.
    let token = server.open_pairing_with(TTL, false).await;
    let err = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("a declined pairing must fail");

    assert!(
        matches!(err, Error::Pairing(PairingError::DeclinedByUser)),
        "unexpected error: {err:?}"
    );
    assert!(matches!(
        server.state.lookup_peer(&phone.fingerprint).await,
        PeerStatus::Unknown
    ));
}

#[tokio::test]
async fn a_captured_proof_cannot_be_replayed_by_another_device() {
    // The interesting case: an attacker who photographs the QR *and* watches
    // the victim pair. The token is consumed, so the attacker is locked out
    // even though it knows the token value.
    let server = TestServer::start().await;
    let victim = TestClient::new("Galaxy S25");
    let attacker = TestClient::new("Attacker");

    let token = server.open_pairing(TTL).await;
    victim
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("victim pairs")
        .close()
        .await;

    let err = attacker
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("replay must fail");
    assert!(
        matches!(err, Error::Pairing(_)),
        "unexpected error: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Capability authorization
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_capability_the_peer_was_not_granted_is_not_delivered() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");

    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    // Revoke the grant behind the peer's back, then reconnect: the server
    // must not negotiate battery.v1 any more.
    {
        let mut store = server.state.store.lock().await;
        store
            .set_capability_grant(&phone.fingerprint, "battery.v1", false)
            .expect("revoke grant");
    }
    session.close().await;
    server.state.end_pairing().await;

    let reconnected = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    // The phone still advertises the capability; the server refuses to use it.
    let reading = BatteryReading {
        percentage: 42,
        charging_state: ChargingState::Discharging,
        peer_timestamp_unix_ms: 1_700_000_000_000,
    };
    assert!(
        reconnected
            .handle
            .send_capability(BatteryCapability::encode(&reading))
            .await
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        server.battery.get(&phone.fingerprint).is_none(),
        "an ungranted capability must not reach its handler"
    );
    reconnected.close().await;
}

#[tokio::test]
async fn an_unknown_capability_id_is_refused_without_closing_the_session() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");
    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    assert!(
        session
            .handle
            .send_capability(anyflow_core::capability::OutboundMessage {
                capability_id: "clipboard.v1".into(),
                payload: b"secret".to_vec(),
            })
            .await
    );

    // A non-fatal ERROR comes back and the session survives, so one bad
    // message does not cost the user their connection.
    assert!(
        session.handle.ping(Duration::from_secs(5)).await.is_some(),
        "session must survive an unsupported capability"
    );
    session.close().await;
}

#[tokio::test]
async fn a_malformed_capability_payload_does_not_kill_the_session() {
    let server = TestServer::start().await;
    let phone = TestClient::new("Galaxy S25");
    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    assert!(
        session
            .handle
            .send_capability(anyflow_core::capability::OutboundMessage {
                capability_id: "battery.v1".into(),
                payload: vec![0xff; 32],
            })
            .await
    );

    assert!(session.handle.ping(Duration::from_secs(5)).await.is_some());
    assert!(
        server.battery.get(&phone.fingerprint).is_none(),
        "garbage must not become state"
    );
    session.close().await;
}

#[tokio::test]
async fn an_out_of_range_battery_percentage_is_rejected() {
    // Validated at the capability boundary, so a hostile peer cannot push a
    // nonsense value into anything downstream.
    let payload = {
        use prost::Message;
        v1::capabilities::BatteryState {
            percentage: 250,
            charging_state: ChargingState::Charging as i32,
            timestamp_unix_ms: 0,
        }
        .encode_to_vec()
    };
    assert!(BatteryCapability::decode(&payload).is_err());

    let valid = {
        use prost::Message;
        v1::capabilities::BatteryState {
            percentage: 100,
            charging_state: ChargingState::Full as i32,
            timestamp_unix_ms: 0,
        }
        .encode_to_vec()
    };
    assert_eq!(
        BatteryCapability::decode(&valid)
            .expect("decode")
            .percentage,
        100
    );
}
