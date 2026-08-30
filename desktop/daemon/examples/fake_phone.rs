//! A stand-in for the Android app, for demonstrating and manually testing the
//! daemon end to end without an Android device.
//!
//! It speaks the real protocol over real TLS with real pinning: it is the
//! same code path a phone takes, driven from a terminal.
//!
//! ```bash
//! # terminal 1
//! anyflowd
//! # terminal 2
//! anyflow pair                       # copy the payload it prints
//! # terminal 3
//! cargo run -p anyflow-daemon --example fake_phone -- pair '<payload>'
//! cargo run -p anyflow-daemon --example fake_phone -- connect
//! ```
//!
//! Its identity is stored under `--data-dir` (default:
//! `/tmp/anyflow-fake-phone`) so that "reconnect without pairing again" can
//! actually be demonstrated across runs.

use std::sync::Arc;
use std::time::Duration;

use anyflow_capability_battery::{BatteryCapability, BatteryReading, BatteryState};
use anyflow_core::capability::CapabilityRegistry;
use anyflow_core::error::{PairingError, Result};
use anyflow_core::qr::QrPayload;
use anyflow_core::session::{self, ClientHandshake, PeerStatus, SessionHandle, SessionHost};
use anyflow_core::store::Store;
use anyflow_core::Fingerprint;
use anyflow_proto::v1;
use anyflow_proto::v1::capabilities::ChargingState;
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;

/// Minimal host: a trust store on disk plus a capability registry.
struct PhoneHost {
    info: v1::DeviceInfo,
    registry: CapabilityRegistry,
    store: Mutex<Store>,
}

#[async_trait::async_trait]
impl SessionHost for PhoneHost {
    fn local_device_info(&self) -> v1::DeviceInfo {
        self.info.clone()
    }
    fn registry(&self) -> CapabilityRegistry {
        self.registry.clone()
    }
    async fn lookup_peer(&self, fingerprint: &Fingerprint) -> PeerStatus {
        match self.store.lock().await.trusted_peer(fingerprint) {
            Some(p) => PeerStatus::Trusted {
                device_id: p.device_id.clone(),
                device_name: p.device_name.clone(),
                granted_capabilities: vec!["battery.v1".into()],
            },
            None => PeerStatus::Unknown,
        }
    }
    async fn pairing_mode_active(&self) -> bool {
        false
    }
    async fn verify_pairing_proof(
        &self,
        _i: &Fingerprint,
        _n: &[u8],
        _p: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError> {
        Err(PairingError::NotInPairingMode)
    }
    async fn confirm_pairing(&self, _d: &v1::DeviceInfo, _f: &Fingerprint) -> bool {
        false
    }
    async fn store_peer(
        &self,
        device: &v1::DeviceInfo,
        fingerprint: &Fingerprint,
        capabilities: &[String],
        version: u32,
    ) -> Result<()> {
        let mut store = self.store.lock().await;
        store.add_peer(anyflow_core::store::TrustedPeer {
            device_id: device.device_id.clone(),
            device_name: device.device_name.clone(),
            platform: device.platform,
            fingerprint: *fingerprint,
            paired_at_unix: 0,
            granted_capabilities: capabilities.iter().map(|c| (c.clone(), true)).collect(),
            last_protocol_version: version,
            revoked: false,
        })
    }
    async fn on_established(&self, _peer: &Fingerprint, _handle: SessionHandle) {}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("crypto provider already installed"))?;

    let data_dir =
        std::env::var("FAKE_PHONE_DIR").unwrap_or_else(|_| "/tmp/anyflow-fake-phone".to_string());
    let store = Store::open(&data_dir)?;
    println!(
        "fake phone identity: {}",
        store.identity().fingerprint().to_display_short()
    );

    let battery_state = Arc::new(BatteryState::default());
    let registry = CapabilityRegistry::builder()
        .register(Arc::new(BatteryCapability::new(Arc::clone(&battery_state))))
        .build();

    // Presented as an Android device so the demo output reads the way the
    // real app would. Only the fingerprint is load-bearing; the name and
    // platform are cosmetic metadata the daemon sanitizes before display.
    let mut info = store.identity().device_info();
    info.device_name = "Fake Phone (dev tool)".to_string();
    info.platform = v1::Platform::Android as i32;

    let host = Arc::new(PhoneHost {
        info,
        registry,
        store: Mutex::new(store),
    });

    match command {
        "pair" => {
            let raw = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("usage: fake_phone pair '<qr payload>'"))?;
            let payload = QrPayload::parse(raw)?;
            let address = *payload
                .addresses
                .first()
                .ok_or_else(|| anyhow::anyhow!("the QR payload carried no address"))?;
            let token = payload.token()?;
            println!(
                "pairing with {} at {address}",
                payload.fingerprint.to_display_short()
            );
            run(host, address, payload.fingerprint, Some(token)).await
        }
        "connect" => {
            let (fingerprint, address) = {
                let store = host.store.lock().await;
                let peer = store
                    .peers()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("not paired yet; run `pair` first"))?;
                let address: std::net::SocketAddr = args
                    .get(2)
                    .map(String::as_str)
                    .unwrap_or("127.0.0.1:55432")
                    .parse()?;
                (peer.fingerprint, address)
            };
            println!(
                "connecting to {} at {address}",
                fingerprint.to_display_short()
            );
            run(host, address, fingerprint, None).await
        }
        _ => {
            eprintln!("usage: fake_phone pair '<qr payload>' | connect [addr:port]");
            std::process::exit(2);
        }
    }
}

async fn run(
    host: Arc<PhoneHost>,
    address: std::net::SocketAddr,
    pinned: Fingerprint,
    token: Option<anyflow_core::pairing::PairingToken>,
) -> anyhow::Result<()> {
    let identity_config = {
        let store = host.store.lock().await;
        anyflow_core::tls::client_config(store.identity(), pinned)?
    };

    let connector = TlsConnector::from(identity_config);
    let tcp = tokio::net::TcpStream::connect(address).await?;
    tcp.set_nodelay(true)?;
    let name = rustls_pki_types::ServerName::try_from("anyflow.invalid")?;
    let mut tls = connector.connect(name, tcp).await?;
    println!("TLS established and server identity pinned");

    let session_host: Arc<dyn SessionHost> = host.clone();
    let handshake =
        session::connect_handshake(&mut tls, &session_host, pinned, token.as_ref()).await?;

    let (established, state) = match handshake {
        ClientHandshake::Established(e, s) => (e, s),
        ClientHandshake::PairingRequired => {
            anyhow::bail!("the daemon does not know this device; run `pair` first")
        }
    };

    println!(
        "session established with {} ({}), capabilities: {:?}",
        established.device.device_name,
        established.peer.to_display_short(),
        established.negotiated_capabilities
    );

    let capabilities = established.negotiated_capabilities.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let notifier: Arc<dyn SessionHost> = Arc::new(Notifier {
        inner: session_host,
        tx: Mutex::new(Some(ready_tx)),
    });

    let task =
        tokio::spawn(async move { session::run_session(tls, notifier, established, state).await });

    let handle = ready_rx.await?;

    if let Some(rtt) = handle.ping(Duration::from_secs(5)).await {
        println!("PING/PONG round trip: {} ms", rtt.as_millis());
    } else {
        println!("no PONG received");
    }

    if capabilities.iter().any(|c| c == "battery.v1") {
        let reading = BatteryReading {
            percentage: 87,
            charging_state: ChargingState::Charging,
            peer_timestamp_unix_ms: 0,
        };
        handle
            .send_capability(BatteryCapability::encode(&reading))
            .await;
        println!("sent battery.v1: 87% charging");
    }

    // Stay connected briefly so `anyflow status` can be run against a live
    // session in another terminal.
    tokio::time::sleep(Duration::from_secs(
        std::env::var("FAKE_PHONE_HOLD_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3),
    ))
    .await;

    handle.shutdown().await;
    let _ = task.await;
    println!("disconnected");
    Ok(())
}

/// Captures the `SessionHandle` when the session starts.
struct Notifier {
    inner: Arc<dyn SessionHost>,
    tx: Mutex<Option<tokio::sync::oneshot::Sender<SessionHandle>>>,
}

#[async_trait::async_trait]
impl SessionHost for Notifier {
    fn local_device_info(&self) -> v1::DeviceInfo {
        self.inner.local_device_info()
    }
    fn registry(&self) -> CapabilityRegistry {
        self.inner.registry()
    }
    async fn lookup_peer(&self, f: &Fingerprint) -> PeerStatus {
        self.inner.lookup_peer(f).await
    }
    async fn pairing_mode_active(&self) -> bool {
        self.inner.pairing_mode_active().await
    }
    async fn verify_pairing_proof(
        &self,
        i: &Fingerprint,
        n: &[u8],
        p: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError> {
        self.inner.verify_pairing_proof(i, n, p).await
    }
    async fn confirm_pairing(&self, d: &v1::DeviceInfo, f: &Fingerprint) -> bool {
        self.inner.confirm_pairing(d, f).await
    }
    async fn store_peer(
        &self,
        d: &v1::DeviceInfo,
        f: &Fingerprint,
        c: &[String],
        v: u32,
    ) -> Result<()> {
        self.inner.store_peer(d, f, c, v).await
    }
    async fn on_established(&self, peer: &Fingerprint, handle: SessionHandle) {
        if let Some(tx) = self.tx.lock().await.take() {
            let _ = tx.send(handle.clone());
        }
        self.inner.on_established(peer, handle).await;
    }
    async fn on_closed(&self, peer: &Fingerprint) {
        self.inner.on_closed(peer).await;
    }
}
