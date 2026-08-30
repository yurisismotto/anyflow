//! Test harness: a full second device, over real TLS, in-process.
//!
//! Nothing here weakens security to make tests pass. The client uses the same
//! `anyflow_core::tls::client_config` as production code, with real
//! certificate pinning. Tests that expect a rejection get one from the actual
//! verifier, not from a stub.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyflow_capability_battery::{BatteryCapability, BatteryState};
use anyflow_core::capability::CapabilityRegistry;
use anyflow_core::error::{PairingError, Result};
use anyflow_core::identity::LocalIdentity;
use anyflow_core::pairing::PairingToken;
use anyflow_core::session::{self, ClientHandshake, PeerStatus, SessionHandle, SessionHost};
use anyflow_core::store::Store;
use anyflow_core::Fingerprint;
use anyflow_daemon::state::DaemonState;
use anyflow_proto::v1;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Installs the crypto provider once per test process.
pub fn init_crypto() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

// ---------------------------------------------------------------------------
// Server side (the Fedora daemon)
// ---------------------------------------------------------------------------

pub struct TestServer {
    pub addr: SocketAddr,
    pub state: Arc<DaemonState>,
    pub fingerprint: Fingerprint,
    pub battery: Arc<BatteryState>,
    _dir: tempfile::TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        init_crypto();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("store");
        let fingerprint = store.identity().fingerprint();

        let battery = Arc::new(BatteryState::default());
        let registry = CapabilityRegistry::builder()
            .register(Arc::new(BatteryCapability::new(Arc::clone(&battery))))
            .build();

        let tls = anyflow_core::tls::server_config(store.identity()).expect("server config");
        let acceptor = TlsAcceptor::from(tls);

        let state = Arc::new(DaemonState::new(store, registry, Arc::clone(&battery)));

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let accept_state = Arc::clone(&state);
        tokio::spawn(async move {
            let _ = anyflow_daemon::listener::run(listener, acceptor, accept_state).await;
        });

        Self {
            addr,
            state,
            fingerprint,
            battery,
            _dir: dir,
        }
    }

    /// Opens a pairing window with an auto-accepting operator, and returns
    /// the token a client would have read from the QR code.
    pub async fn open_pairing(&self, ttl: Duration) -> PairingToken {
        self.open_pairing_with(ttl, true).await
    }

    /// Opens a pairing window whose operator answers `accept`.
    pub async fn open_pairing_with(&self, ttl: Duration, accept: bool) -> PairingToken {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let token_b32 = self.state.begin_pairing(ttl, tx).await.expect("begin");

        // Stands in for the human at the terminal.
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                let _ = request.reply.send(accept);
            }
        });

        PairingToken::from_base32(&token_b32).expect("token")
    }
}

// ---------------------------------------------------------------------------
// Client side (the Android phone)
// ---------------------------------------------------------------------------

/// A second device with its own identity, trust store and capability set.
pub struct TestClient {
    pub identity: Arc<LocalIdentity>,
    pub fingerprint: Fingerprint,
    pub host: Arc<dyn SessionHost>,
    pub battery: Arc<BatteryState>,
    trusted: Arc<Mutex<Vec<Fingerprint>>>,
}

struct ClientHost {
    info: v1::DeviceInfo,
    registry: CapabilityRegistry,
    trusted: Arc<Mutex<Vec<Fingerprint>>>,
}

#[async_trait::async_trait]
impl SessionHost for ClientHost {
    fn local_device_info(&self) -> v1::DeviceInfo {
        self.info.clone()
    }
    fn registry(&self) -> CapabilityRegistry {
        self.registry.clone()
    }
    async fn lookup_peer(&self, fingerprint: &Fingerprint) -> PeerStatus {
        if self.trusted.lock().await.contains(fingerprint) {
            PeerStatus::Trusted {
                device_id: "server".into(),
                device_name: "Fedora".into(),
                granted_capabilities: vec!["battery.v1".into()],
            }
        } else {
            PeerStatus::Unknown
        }
    }
    async fn pairing_mode_active(&self) -> bool {
        false
    }
    async fn verify_pairing_proof(
        &self,
        _initiator: &Fingerprint,
        _nonce: &[u8],
        _proof: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError> {
        // The client never acts as a pairing responder in this Sprint.
        Err(PairingError::NotInPairingMode)
    }
    async fn confirm_pairing(&self, _d: &v1::DeviceInfo, _f: &Fingerprint) -> bool {
        false
    }
    async fn store_peer(
        &self,
        _device: &v1::DeviceInfo,
        fingerprint: &Fingerprint,
        _caps: &[String],
        _version: u32,
    ) -> Result<()> {
        self.trusted.lock().await.push(*fingerprint);
        Ok(())
    }
}

impl TestClient {
    pub fn new(name: &str) -> Self {
        init_crypto();
        let identity =
            Arc::new(LocalIdentity::generate(name, v1::Platform::Android).expect("identity"));
        let fingerprint = identity.fingerprint();
        let battery = Arc::new(BatteryState::default());
        let registry = CapabilityRegistry::builder()
            .register(Arc::new(BatteryCapability::new(Arc::clone(&battery))))
            .build();
        let trusted = Arc::new(Mutex::new(Vec::new()));

        let host: Arc<dyn SessionHost> = Arc::new(ClientHost {
            info: identity.device_info(),
            registry,
            trusted: Arc::clone(&trusted),
        });

        Self {
            identity,
            fingerprint,
            host,
            battery,
            trusted,
        }
    }

    /// Marks a server as already-trusted, simulating a persisted pairing.
    pub async fn trust(&self, fingerprint: Fingerprint) {
        self.trusted.lock().await.push(fingerprint);
    }

    pub async fn is_trusted(&self, fingerprint: &Fingerprint) -> bool {
        self.trusted.lock().await.contains(fingerprint)
    }

    /// Opens a real TLS connection with the given pinned server identity.
    pub async fn tls_connect(
        &self,
        addr: SocketAddr,
        pinned: Fingerprint,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
        let config = anyflow_core::tls::client_config(&self.identity, pinned)?;
        let connector = TlsConnector::from(config);
        let tcp = TcpStream::connect(addr).await?;
        // The name is irrelevant: our verifier pins the key and ignores it.
        let name = rustls_pki_types::ServerName::try_from("anyflow.invalid").expect("static name");
        Ok(connector.connect(name, tcp).await?)
    }

    /// Full connect + handshake. `token` triggers the pairing exchange.
    pub async fn connect(
        &self,
        addr: SocketAddr,
        pinned: Fingerprint,
        token: Option<&PairingToken>,
    ) -> Result<ConnectedSession> {
        let mut tls = self.tls_connect(addr, pinned).await?;
        match session::connect_handshake(&mut tls, &self.host, pinned, token).await? {
            ClientHandshake::Established(established, state) => {
                let host = Arc::clone(&self.host);
                let capabilities = established.negotiated_capabilities.clone();
                let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

                let notifier = Arc::new(HandleNotifier {
                    inner: Arc::clone(&self.host),
                    tx: Mutex::new(Some(ready_tx)),
                });
                let host_for_run: Arc<dyn SessionHost> = notifier;
                let _ = host;

                let task = tokio::spawn(async move {
                    session::run_session(tls, host_for_run, established, state).await
                });

                let handle = ready_rx.await.map_err(|_| {
                    anyflow_core::Error::Protocol("session ended before it started")
                })?;

                Ok(ConnectedSession {
                    handle,
                    task,
                    negotiated_capabilities: capabilities,
                })
            }
            ClientHandshake::PairingRequired => {
                Err(anyflow_core::Error::Pairing(PairingError::NotInPairingMode))
            }
        }
    }
}

/// Wraps a host so the test can capture the `SessionHandle`.
struct HandleNotifier {
    inner: Arc<dyn SessionHost>,
    tx: Mutex<Option<tokio::sync::oneshot::Sender<SessionHandle>>>,
}

#[async_trait::async_trait]
impl SessionHost for HandleNotifier {
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

impl std::fmt::Debug for ConnectedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedSession")
            .field("peer", &self.handle.peer())
            .field("capabilities", &self.negotiated_capabilities)
            .finish()
    }
}

pub struct ConnectedSession {
    pub handle: SessionHandle,
    pub task: tokio::task::JoinHandle<Result<()>>,
    pub negotiated_capabilities: Vec<String>,
}

impl ConnectedSession {
    pub async fn close(self) {
        self.handle.shutdown().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

/// Polls until `f` returns true, or panics after `timeout`.
pub async fn wait_until<F, Fut>(timeout: Duration, mut f: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if f().await {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("condition not met within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
