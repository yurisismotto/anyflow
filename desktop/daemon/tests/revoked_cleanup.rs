//! "Remove from list", from the control socket down to the wire.
//!
//! The store tests in `anyflow-core` prove what a tombstone *is*. These prove
//! what it *does*: that a device removed from the list is still turned away by
//! the real handshake, that the control socket refuses to use the operation as
//! a shortcut to revoking, and that a fresh pairing ceremony — the whole one,
//! with a live token and a human saying yes — is still the only way back.
//!
//! Everything here is local. Nothing in this feature sends a byte to a peer,
//! and the one test that opens a socket to the removed device does so to be
//! refused.

mod common;

use std::sync::Arc;
use std::time::Duration;

use anyflow_core::session::{PeerStatus, SessionHost};
use anyflow_core::Error;
use anyflow_daemon::control::{DeviceReport, DeviceState, Request, Response};
use anyflow_daemon::server;
use anyflow_daemon::state::DaemonState;
use common::{TestClient, TestServer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const TTL: Duration = Duration::from_secs(30);

/// A control socket for one test, in a temp dir of its own.
struct Control {
    path: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl Control {
    fn start(state: Arc<DaemonState>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");
        let listener = server::bind(&path).expect("bind control socket");
        tokio::spawn(async move {
            let _ = server::run(listener, state).await;
        });
        Self { path, _dir: dir }
    }

    async fn request(&self, request: Request) -> Response {
        let stream = UnixStream::connect(&self.path).await.expect("connect");
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let mut bytes = serde_json::to_vec(&request).expect("encode");
        bytes.push(b'\n');
        write.write_all(&bytes).await.expect("write");
        write.flush().await.expect("flush");
        let line = lines
            .next_line()
            .await
            .expect("read")
            .expect("daemon replied");
        serde_json::from_str(&line).expect("decode")
    }

    async fn devices(&self) -> Vec<DeviceReport> {
        match self.request(Request::Devices).await {
            Response::Devices(d) => d,
            other => panic!("expected Devices, got {other:?}"),
        }
    }

    async fn remove(&self, fingerprint: &str) -> Response {
        self.request(Request::HideRevokedDevice {
            fingerprint: fingerprint.to_string(),
        })
        .await
    }
}

fn ok_message(response: Response) -> String {
    match response {
        Response::Ok { message } => message,
        other => panic!("expected Ok, got {other:?}"),
    }
}

fn error_message(response: Response) -> String {
    match response {
        Response::Error { message } => message,
        other => panic!("expected Error, got {other:?}"),
    }
}

/// Pairs `phone` with `server`, then revokes it — the state this whole sprint
/// starts from.
async fn pair_then_revoke(server: &TestServer, phone: &TestClient) {
    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;
    let mut store = server.state.store.lock().await;
    assert!(store.revoke_peer(&phone.fingerprint).expect("revoke"));
}

// ---------------------------------------------------------------------------
// D1 / D4 — what Settings is shown, before and after
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_revoked_device_is_listed_until_it_is_removed() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;

    // D1: it is there, and it says why.
    let rows = control.devices().await;
    let row = rows
        .iter()
        .find(|d| d.fingerprint == phone.fingerprint.to_hex())
        .expect("a revoked device stays on the list until the owner removes it");
    assert!(row.revoked);
    assert!(!row.paired);
    assert_eq!(row.state, DeviceState::Revoked);

    let message = ok_message(control.remove(&phone.fingerprint.to_hex()).await);
    assert!(
        message.contains("stays revoked"),
        "the reply must not suggest the revocation went with it: {message}"
    );

    // D4: gone from the list, with no restart in between — this is the same
    // daemon process answering.
    assert!(
        !control
            .devices()
            .await
            .iter()
            .any(|d| d.fingerprint == phone.fingerprint.to_hex()),
        "a tombstone must not appear in the ordinary device list"
    );

    // And gone from every other peer-listing surface too.
    match control.request(Request::ClipboardStatus).await {
        Response::Clipboard(report) => assert!(report
            .peers
            .iter()
            .all(|p| p.fingerprint_short != phone.fingerprint.to_display_short())),
        other => panic!("expected Clipboard, got {other:?}"),
    }
    match control.request(Request::NotificationsStatus).await {
        Response::Notifications(report) => assert!(report
            .peers
            .iter()
            .all(|p| p.fingerprint_short != phone.fingerprint.to_display_short())),
        other => panic!("expected Notifications, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// D2 — a trusted device is not removable this way
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_trusted_device_cannot_be_removed_from_the_list() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");

    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;

    let message = error_message(control.remove(&phone.fingerprint.to_hex()).await);
    assert!(
        message.contains("revoke it"),
        "the refusal should say what to do instead: {message}"
    );
    assert!(server.is_paired(phone.fingerprint).await);
    assert!(control
        .devices()
        .await
        .iter()
        .any(|d| d.fingerprint == phone.fingerprint.to_hex()));
}

#[tokio::test]
async fn a_malformed_fingerprint_is_refused_without_touching_anything() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;

    for bad in ["", "   ", "not-hex", "ab12", "../../etc/passwd"] {
        let message = error_message(control.remove(bad).await);
        assert!(message.contains("fingerprint"), "{bad:?}: {message}");
    }
    assert_eq!(control.devices().await.len(), 1, "nothing was removed");
}

// ---------------------------------------------------------------------------
// D5 — the central gate: a tombstoned device still cannot connect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_device_removed_from_the_list_is_still_refused_at_the_door() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;
    ok_message(control.remove(&phone.fingerprint.to_hex()).await);

    // The daemon's own answer to "who is this?" — the question
    // `session::accept` asks before anything else is decided.
    assert!(
        matches!(
            server.state.lookup_peer(&phone.fingerprint).await,
            PeerStatus::Revoked
        ),
        "a tombstone must answer Revoked, not Unknown: an unknown key is \
         greeted with PAIRING_REQUIRED and this daemon's capability list, a \
         revoked one with REJECTED"
    );

    // And over a real TLS 1.3 handshake, from the device itself.
    let err = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("a removed device must still be refused");
    assert!(matches!(err, Error::NotAuthorized), "{err:?}");

    // Presenting a token changes nothing while no window is open.
    let err = phone
        .connect(
            server.addr,
            server.fingerprint,
            Some(&anyflow_core::pairing::PairingToken::generate().expect("token")),
        )
        .await
        .expect_err("a token nobody issued must not readmit it either");
    assert!(matches!(err, Error::NotAuthorized), "{err:?}");
}

// ---------------------------------------------------------------------------
// D6 — one fingerprint, not one name
// ---------------------------------------------------------------------------

#[tokio::test]
async fn removing_one_device_leaves_a_same_named_device_listed() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));

    // Two tablets with the same name on screen. Only the keys differ.
    let first = TestClient::new("SM-X620");
    let second = TestClient::new("SM-X620");
    pair_then_revoke(&server, &first).await;
    pair_then_revoke(&server, &second).await;
    assert_eq!(control.devices().await.len(), 2);

    ok_message(control.remove(&first.fingerprint.to_hex()).await);

    let rows = control.devices().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fingerprint, second.fingerprint.to_hex());
    assert!(rows[0].revoked, "and it is still revoked, not repaired");
}

// ---------------------------------------------------------------------------
// D11 / D12 — the way back is the pairing ceremony, and it yields one row
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_fresh_pairing_brings_a_removed_device_back_exactly_once() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;
    ok_message(control.remove(&phone.fingerprint.to_hex()).await);

    // The owner runs `anyflow pair` and confirms the fingerprint by hand.
    let fresh = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&fresh))
        .await
        .expect("a removed device must still be recoverable by pairing again");

    let rows = control.devices().await;
    assert_eq!(
        rows.len(),
        1,
        "one cryptographic identity is one row; a tombstone must not become a \
         second"
    );
    assert_eq!(rows[0].fingerprint, phone.fingerprint.to_hex());
    assert!(!rows[0].revoked && rows[0].paired);
    assert_eq!(
        rows[0].device_name, "SM-X620",
        "fresh display metadata comes from the handshake"
    );

    // D10: nothing sensitive came back. `auto_grant` is `battery.v1` alone.
    let granted = server.granted_capabilities(phone.fingerprint).await;
    assert!(!granted.iter().any(|c| c == "clipboard.v1"));
    assert!(!granted.iter().any(|c| c == "notifications.v1"));
    assert!(!granted.iter().any(|c| c == "files.v1"));

    session.close().await;
}

/// D10, the other half: a failed ceremony must leave the tombstone alone.
#[tokio::test]
async fn a_failed_pairing_leaves_the_tombstone_exactly_as_it_was() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;
    ok_message(control.remove(&phone.fingerprint.to_hex()).await);

    // A window is open and the operator declines.
    let token = server.open_pairing_with(TTL, false).await;
    let err = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect_err("a declined confirmation must not readmit anything");
    assert!(matches!(err, Error::Pairing(_)), "{err:?}");

    let store = server.state.store.lock().await;
    let record = store.peer_record(&phone.fingerprint).expect("still there");
    assert!(
        record.revoked && record.hidden,
        "unchanged, in both respects"
    );
    assert!(record.device_name.is_empty());
    drop(store);
    assert!(control.devices().await.is_empty());
}

// ---------------------------------------------------------------------------
// D13 / D14 — a different key gets new-peer semantics and inherits nothing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_different_key_is_a_new_device_and_the_tombstone_gives_it_nothing() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));

    let old = TestClient::new("SM-X620");
    pair_then_revoke(&server, &old).await;
    server.set_grant(old.fingerprint, "files.v1", true).await;
    {
        // Even if something had left a grant on the revoked record, removing
        // it must not carry that across to another key.
        let store = server.state.store.lock().await;
        assert!(store.trusted_peer(&old.fingerprint).is_none());
    }
    ok_message(control.remove(&old.fingerprint.to_hex()).await);

    // Fingerprint B. Same name, same model, different key.
    let new = TestClient::new("SM-X620");
    let token = server.open_pairing(TTL).await;
    let session = new
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("a new key pairs as a new device");

    // B is trusted with new-peer grants and nothing else.
    let granted = server.granted_capabilities(new.fingerprint).await;
    assert!(!granted.iter().any(|c| c == "files.v1"));
    assert!(!granted.iter().any(|c| c == "clipboard.v1"));
    assert!(!granted.iter().any(|c| c == "notifications.v1"));

    // A is untouched: still revoked, still hidden, still refused.
    assert!(matches!(
        server.state.lookup_peer(&old.fingerprint).await,
        PeerStatus::Revoked
    ));
    let rows = control.devices().await;
    assert_eq!(rows.len(), 1, "B is listed; A's tombstone is not");
    assert_eq!(rows[0].fingerprint, new.fingerprint.to_hex());

    session.close().await;
}

// ---------------------------------------------------------------------------
// D18 / D19 / D20 — bulk removal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bulk_removal_takes_the_revoked_and_leaves_the_trusted() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));

    let keeper = TestClient::new("Fedora");
    let token = server.open_pairing(TTL).await;
    keeper
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;
    server.set_grant(keeper.fingerprint, "files.v1", true).await;

    let a = TestClient::new("SM-X620");
    let b = TestClient::new("SM-X620");
    let c = TestClient::new("SM-X620");
    for phone in [&a, &b, &c] {
        pair_then_revoke(&server, phone).await;
    }
    assert_eq!(control.devices().await.len(), 4);

    let before = server.granted_capabilities(keeper.fingerprint).await;
    let message = ok_message(control.request(Request::HideAllRevokedDevices).await);
    assert!(message.contains('3'), "the count is reported: {message}");

    let rows = control.devices().await;
    assert_eq!(rows.len(), 1, "only the trusted device is still listed");
    assert_eq!(rows[0].fingerprint, keeper.fingerprint.to_hex());

    // D19: the trusted device came through semantically unchanged.
    assert!(!rows[0].revoked && rows[0].paired);
    assert_eq!(rows[0].device_name, "Fedora");
    assert_eq!(
        server.granted_capabilities(keeper.fingerprint).await,
        before,
        "a bulk cleanup must not touch a grant"
    );

    // Every removed key is still refused, one at a time.
    for phone in [&a, &b, &c] {
        assert!(
            matches!(
                server.state.lookup_peer(&phone.fingerprint).await,
                PeerStatus::Revoked
            ),
            "a bulk removal must leave three tombstones, not three deletions"
        );
    }
    let err = a
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("still refused over the wire");
    assert!(matches!(err, Error::NotAuthorized), "{err:?}");
}

#[tokio::test]
async fn bulk_removal_with_nothing_revoked_says_so_and_changes_nothing() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("Fedora");
    let token = server.open_pairing(TTL).await;
    phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair")
        .close()
        .await;
    server.state.end_pairing().await;

    // D20: the GUI decides whether to offer the action from the same set the
    // daemon acts on, so "nothing to remove" and "no button" are one fact.
    assert_eq!(
        control.devices().await.iter().filter(|d| d.revoked).count(),
        0
    );
    let message = ok_message(control.request(Request::HideAllRevokedDevices).await);
    assert!(message.contains("no revoked devices"), "{message}");
    assert_eq!(control.devices().await.len(), 1);
    assert!(server.is_paired(phone.fingerprint).await);
}

// ---------------------------------------------------------------------------
// D21 — an open session cannot outlive the cleanup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_live_session_does_not_survive_a_removal() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");

    let token = server.open_pairing(TTL).await;
    let session = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");
    server.state.end_pairing().await;

    // Revoke *while the session is up*, then remove from the list. The
    // removal calls the same teardown the revoke does, so there is no second
    // kill path whose absence could leave a session running.
    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&phone.fingerprint).expect("revoke");
    }
    ok_message(control.remove(&phone.fingerprint.to_hex()).await);

    common::wait_until(Duration::from_secs(5), || async {
        server.state.session_for(&phone.fingerprint).await.is_none()
    })
    .await;

    // And it cannot come back by dialling again.
    let err = phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect_err("reconnecting must not resurrect a removed device");
    assert!(matches!(err, Error::NotAuthorized), "{err:?}");
    assert!(control.devices().await.is_empty());

    session.close().await;
}

// ---------------------------------------------------------------------------
// Selectors must not be able to name a tombstone
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_tombstone_cannot_be_named_by_any_device_selector() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let phone = TestClient::new("SM-X620");
    pair_then_revoke(&server, &phone).await;

    let device_id = control.devices().await[0].device_id.clone();
    let prefix = phone.fingerprint.to_hex()[..16].to_string();
    ok_message(control.remove(&phone.fingerprint.to_hex()).await);

    // The id it used to have, a fingerprint prefix, and the empty string —
    // the last because a tombstone's device id is literally empty, and an
    // empty selector must not match it.
    for selector in [device_id.as_str(), prefix.as_str(), ""] {
        let response = control
            .request(Request::Grant {
                device: selector.to_string(),
                capability: "files.v1".into(),
                granted: true,
            })
            .await;
        let message = error_message(response);
        assert!(
            !message.is_empty(),
            "selector {selector:?} must not resolve to a tombstone"
        );
    }
    assert!(!server.is_paired(phone.fingerprint).await);
}
