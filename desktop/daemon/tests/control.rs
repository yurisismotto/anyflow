//! What `anyflow status` and `anyflow devices` actually report.
//!
//! The defect behind these: one boolean called `connected` stood in for three
//! different facts — is this device paired, does it have a session right now,
//! and how old is the last thing it told us. A dead session kept displaying a
//! battery percentage as though the phone were still there.

mod common;

use std::sync::Arc;
use std::time::Duration;

use anyflow_daemon::control::{DeviceState, Request, Response};
use anyflow_daemon::server;
use anyflow_daemon::state::DaemonState;
use common::{wait_until, TestClient, TestServer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

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
}

fn devices(response: Response) -> Vec<anyflow_daemon::control::DeviceReport> {
    match response {
        Response::Devices(d) => d,
        other => panic!("expected Devices, got {other:?}"),
    }
}

fn status(response: Response) -> anyflow_daemon::control::StatusReport {
    match response {
        Response::Status(s) => s,
        other => panic!("expected Status, got {other:?}"),
    }
}

#[tokio::test]
async fn a_live_session_is_reported_as_paired_and_connected() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let client = TestClient::new("Galaxy S25");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    wait_until(Duration::from_secs(5), || async {
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .is_some()
    })
    .await;

    let list = devices(control.request(Request::Devices).await);
    let d = list.first().expect("one device");
    assert!(d.paired, "the trust store says paired");
    assert!(d.connected, "a session exists");
    assert_eq!(d.state, DeviceState::Connected);
    assert!(!d.revoked);
    assert!(
        d.silent_secs.is_some_and(|s| s < 5),
        "a session that just started has not been silent"
    );

    let s = status(control.request(Request::Status).await);
    assert_eq!(s.connections.len(), 1);
    assert_eq!(s.paired_devices, 1);
    // `status` lists devices too, so a paired-but-offline phone is visible
    // rather than silently missing from the output.
    assert_eq!(s.devices.len(), 1);
    assert!(!s.listen_families.is_empty());

    session.close().await;
}

#[tokio::test]
async fn a_device_whose_session_ended_is_paired_but_not_connected() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let client = TestClient::new("Galaxy S25");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

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
        devices(control.request(Request::Devices).await)
            .first()
            .and_then(|d| d.battery.as_ref())
            .is_some_and(|b| b.percentage == 57 && !b.stale)
    })
    .await;

    session.close().await;

    wait_until(Duration::from_secs(5), || async {
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .is_none()
    })
    .await;

    let list = devices(control.request(Request::Devices).await);
    let d = list.first().expect("one device");
    assert!(d.paired, "trust survives the session ending");
    assert!(!d.connected, "the session is gone");
    assert_eq!(d.state, DeviceState::Disconnected);
    assert!(
        d.battery.is_none(),
        "an offline device must not carry a battery percentage: that is \
         exactly the reading that used to look live"
    );
    assert!(d.silent_secs.is_none(), "there is no session to be silent");
    assert!(d.last_seen_secs_ago.is_some(), "we know when it went away");

    let s = status(control.request(Request::Status).await);
    assert!(s.connections.is_empty(), "no zombie connection row");
    assert_eq!(s.paired_devices, 1, "still paired");
}

#[tokio::test]
async fn unpairing_reports_revoked_and_not_connected() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let client = TestClient::new("Galaxy S25");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    wait_until(Duration::from_secs(5), || async {
        server
            .state
            .session_for(&client.fingerprint)
            .await
            .is_some()
    })
    .await;

    let response = control
        .request(Request::Unpair {
            device: client.identity.device_id().to_string(),
        })
        .await;
    assert!(matches!(response, Response::Ok { .. }), "{response:?}");

    let list = devices(control.request(Request::Devices).await);
    let d = list.first().expect("the device is still listed");
    assert!(d.revoked);
    assert!(!d.paired, "revoked is not paired");
    assert!(
        !d.connected,
        "revocation takes effect now, not at reconnect"
    );
    assert_eq!(d.state, DeviceState::Revoked);

    let s = status(control.request(Request::Status).await);
    assert!(s.connections.is_empty());
    assert_eq!(s.paired_devices, 0);

    let _ = session.task.await;
}

/// An idle-but-healthy session must not be reported as doubtful merely
/// because a capability has nothing new to say.
///
/// This is a defect found on real hardware after the first fix: device state
/// was derived from the age of the last battery reading, so a phone sitting
/// happily connected for three minutes — answering every liveness probe —
/// was reported `stale` because its battery had not moved a percent. The
/// two questions are unrelated and are now answered separately.
#[tokio::test]
async fn a_quiet_but_answering_session_stays_connected() {
    let server = TestServer::start().await;
    let control = Control::start(Arc::clone(&server.state));
    let client = TestClient::new("Galaxy Tab");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pair");

    session
        .handle
        .send_capability(anyflow_capability_battery::BatteryCapability::encode(
            &anyflow_capability_battery::BatteryReading {
                percentage: 80,
                charging_state: anyflow_proto::v1::capabilities::ChargingState::NotCharging,
                peer_timestamp_unix_ms: 1,
            },
        ))
        .await;

    wait_until(Duration::from_secs(5), || async {
        server.battery.get(&client.fingerprint).is_some()
    })
    .await;

    // Let the reading age past the battery staleness threshold while the
    // session keeps answering. Virtual time, so this costs milliseconds.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(
        anyflow_daemon::control::BATTERY_STALE_AFTER_SECS + 30,
    ))
    .await;
    tokio::time::resume();

    // Keep the session demonstrably alive: a round-trip proves the peer is
    // there right now.
    assert!(
        session.handle.ping(Duration::from_secs(5)).await.is_some(),
        "the session must still be answering"
    );

    let list = devices(control.request(Request::Devices).await);
    let d = list.first().expect("one device");
    assert_eq!(
        d.state,
        DeviceState::Connected,
        "an answering session is connected, whatever its telemetry's age"
    );
    let battery = d.battery.as_ref().expect("the last reading is still shown");
    assert!(
        battery.stale,
        "but the reading itself is honestly labelled as old"
    );
    assert_eq!(80, battery.percentage);

    session.close().await;
}
