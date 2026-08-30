//! Daemon-wide shared state, and the [`SessionHost`] implementation that
//! connects the protocol engine to the trust store and the human.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use fedroid_core::capability::CapabilityRegistry;
use fedroid_core::error::{PairingError, Result};
use fedroid_core::pairing::PairingSession;
use fedroid_core::session::{PeerStatus, SessionHandle, SessionHost};
use fedroid_core::store::{Store, TrustedPeer};
use fedroid_core::Fingerprint;
use fedroid_proto::v1;
use tokio::sync::{oneshot, Mutex, RwLock};

/// A pending "is this device you?" question waiting on a human.
pub struct ConfirmRequest {
    pub device: v1::DeviceInfo,
    pub fingerprint: Fingerprint,
    pub reply: oneshot::Sender<bool>,
}

pub struct DaemonState {
    pub store: Mutex<Store>,
    pub registry: CapabilityRegistry,
    pub battery: Arc<fedroid_capability_battery::BatteryState>,

    /// The single open pairing window, if any.
    pairing: Mutex<Option<PairingSession>>,
    /// Where to send confirmation questions. Present only while a `fedroid
    /// pair` control session is attached: with no operator watching there is
    /// nobody to answer, and auto-accepting would defeat the whole point.
    confirm_tx: Mutex<Option<tokio::sync::mpsc::Sender<ConfirmRequest>>>,

    sessions: RwLock<HashMap<Fingerprint, SessionHandle>>,

    device_info: v1::DeviceInfo,

    /// The port actually bound at startup, which is not necessarily the one
    /// in settings: a `--port` override or a port-0 bind both change it, and
    /// advertising the wrong one would hand out a QR code nobody can dial.
    listen_port: std::sync::atomic::AtomicU16,
}

impl DaemonState {
    pub fn new(
        store: Store,
        registry: CapabilityRegistry,
        battery: Arc<fedroid_capability_battery::BatteryState>,
    ) -> Self {
        let device_info = store.identity().device_info();
        Self {
            store: Mutex::new(store),
            registry,
            battery,
            pairing: Mutex::new(None),
            confirm_tx: Mutex::new(None),
            sessions: RwLock::new(HashMap::new()),
            device_info,
            listen_port: std::sync::atomic::AtomicU16::new(0),
        }
    }

    /// Records the port the listener actually bound.
    pub fn set_listen_port(&self, port: u16) {
        self.listen_port
            .store(port, std::sync::atomic::Ordering::Relaxed);
    }

    /// The bound port, falling back to the configured one before the
    /// listener has started.
    pub async fn listen_port(&self) -> u16 {
        match self.listen_port.load(std::sync::atomic::Ordering::Relaxed) {
            0 => self.store.lock().await.settings().listen_port,
            port => port,
        }
    }

    /// Opens a pairing window, replacing any previous one.
    ///
    /// Replacing rather than refusing is deliberate: the previous window's
    /// token is dropped (and zeroed), so a stale QR left on screen stops
    /// working the moment a new one is generated.
    pub async fn begin_pairing(
        &self,
        ttl: Duration,
        confirm_tx: tokio::sync::mpsc::Sender<ConfirmRequest>,
    ) -> Result<String> {
        let session = PairingSession::new(ttl)?;
        let token_b32 = session.token().to_base32();
        *self.pairing.lock().await = Some(session);
        *self.confirm_tx.lock().await = Some(confirm_tx);
        Ok(token_b32)
    }

    /// Closes the pairing window and detaches the confirmation channel.
    pub async fn end_pairing(&self) {
        *self.pairing.lock().await = None;
        *self.confirm_tx.lock().await = None;
    }

    pub async fn pairing_remaining(&self) -> Option<Duration> {
        self.pairing
            .lock()
            .await
            .as_ref()
            .filter(|s| !s.is_exhausted())
            .map(|s| s.remaining())
    }

    pub async fn register_session(&self, handle: SessionHandle) {
        self.sessions.write().await.insert(handle.peer(), handle);
    }

    pub async fn unregister_session(&self, peer: &Fingerprint) {
        self.sessions.write().await.remove(peer);
    }

    pub async fn session_handles(&self) -> Vec<SessionHandle> {
        self.sessions.read().await.values().cloned().collect()
    }

    pub async fn session_for(&self, peer: &Fingerprint) -> Option<SessionHandle> {
        self.sessions.read().await.get(peer).cloned()
    }

    pub fn device_info(&self) -> v1::DeviceInfo {
        self.device_info.clone()
    }

    /// Resolves a user-supplied device selector: an exact device id, or a
    /// case-insensitive fingerprint prefix of at least 8 hex characters.
    ///
    /// The minimum length exists so a one-character prefix cannot silently
    /// match the wrong device in a destructive command like `unpair`. An
    /// ambiguous prefix is an error, never a guess.
    pub async fn resolve_device(&self, selector: &str) -> std::result::Result<Fingerprint, String> {
        let store = self.store.lock().await;
        let needle = selector.trim().to_ascii_lowercase();

        if let Some(p) = store.peers().find(|p| p.device_id == needle) {
            return Ok(p.fingerprint);
        }

        if needle.len() < 8 {
            return Err(format!(
                "no device with id '{selector}'; a fingerprint prefix must be \
                 at least 8 characters"
            ));
        }

        let matches: Vec<&TrustedPeer> = store
            .peers()
            .filter(|p| p.fingerprint.to_hex().starts_with(&needle))
            .collect();

        match matches.as_slice() {
            [one] => Ok(one.fingerprint),
            [] => Err(format!("no device matches '{selector}'")),
            many => Err(format!(
                "'{}' is ambiguous: it matches {} devices",
                selector,
                many.len()
            )),
        }
    }
}

#[async_trait::async_trait]
impl SessionHost for DaemonState {
    fn local_device_info(&self) -> v1::DeviceInfo {
        self.device_info.clone()
    }

    fn registry(&self) -> CapabilityRegistry {
        self.registry.clone()
    }

    async fn lookup_peer(&self, fingerprint: &Fingerprint) -> PeerStatus {
        let store = self.store.lock().await;
        match store.peer_record(fingerprint) {
            None => PeerStatus::Unknown,
            Some(p) if p.revoked => PeerStatus::Revoked,
            Some(p) => PeerStatus::Trusted {
                device_id: p.device_id.clone(),
                device_name: p.device_name.clone(),
                granted_capabilities: p
                    .granted_capabilities
                    .iter()
                    .filter(|(_, granted)| **granted)
                    .map(|(id, _)| id.clone())
                    .collect(),
            },
        }
    }

    async fn pairing_mode_active(&self) -> bool {
        // A window with no operator attached is not usable, so it does not
        // count as active.
        let has_operator = self.confirm_tx.lock().await.is_some();
        let window_open = self
            .pairing
            .lock()
            .await
            .as_ref()
            .is_some_and(|s| !s.is_exhausted());
        has_operator && window_open
    }

    async fn verify_pairing_proof(
        &self,
        initiator: &Fingerprint,
        nonce: &[u8],
        proof: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError> {
        let responder = self.device_info.identity_fingerprint.clone();
        let responder = Fingerprint::from_hex(&responder).map_err(|_| PairingError::BadProof)?;

        let mut guard = self.pairing.lock().await;
        let Some(session) = guard.as_mut() else {
            return Err(PairingError::NotInPairingMode);
        };

        let result = session.verify_and_consume(&responder, initiator, nonce, proof);

        // Once the window can no longer be used, drop it immediately so its
        // token is zeroed rather than lingering in memory.
        if session.is_exhausted() {
            *guard = None;
        }
        result
    }

    async fn confirm_pairing(&self, device: &v1::DeviceInfo, fingerprint: &Fingerprint) -> bool {
        let tx = { self.confirm_tx.lock().await.clone() };
        let Some(tx) = tx else {
            return false;
        };

        let (reply, rx) = oneshot::channel();
        let request = ConfirmRequest {
            device: device.clone(),
            fingerprint: *fingerprint,
            reply,
        };
        if tx.send(request).await.is_err() {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    async fn store_peer(
        &self,
        device: &v1::DeviceInfo,
        fingerprint: &Fingerprint,
        negotiated_capabilities: &[String],
        protocol_version: u32,
    ) -> Result<()> {
        let mut store = self.store.lock().await;
        let auto_grant = store.settings().auto_grant.clone();

        // Grants are intersected with the local auto-grant policy. A peer
        // advertising a capability never grants itself that capability.
        let granted = negotiated_capabilities
            .iter()
            .map(|c| (c.clone(), auto_grant.iter().any(|a| a == c)))
            .collect();

        let peer = TrustedPeer {
            device_id: device.device_id.clone(),
            device_name: fedroid_core::discovery::sanitize_device_name(&device.device_name),
            platform: device.platform,
            fingerprint: *fingerprint,
            paired_at_unix: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            granted_capabilities: granted,
            last_protocol_version: protocol_version,
            revoked: false,
        };
        store.add_peer(peer)
    }

    async fn on_established(&self, _peer: &Fingerprint, handle: SessionHandle) {
        self.register_session(handle).await;
    }

    async fn on_closed(&self, peer: &Fingerprint) {
        self.unregister_session(peer).await;
    }
}
