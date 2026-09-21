//! Shared harness for the `clipboard.v1` suites.
//!
//! Each integration test is its own crate, so a helper used by one suite and
//! not the other is dead code from that crate's point of view. The allow is
//! on the module rather than on each item so the list does not have to be
//! maintained.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use omnibridge_capability_clipboard::backend::MemoryBackend;
use omnibridge_capability_clipboard::{
    ClipboardAuthorizer, ClipboardManager, ClipboardPolicy, CAPABILITY_ID,
};
use omnibridge_core::capability::OutboundMessage;
use omnibridge_core::Fingerprint;
use omnibridge_proto::v1::capabilities as pb;
use omnibridge_proto::Message;
use tokio::sync::{mpsc, RwLock};

/// A fingerprint made of one repeated byte, so tests can name peers by digit.
pub fn fp(byte: u8) -> Fingerprint {
    Fingerprint::from_hex(&format!("{byte:02x}").repeat(32)).expect("valid fingerprint")
}

/// An authorizer a test can rewrite at will.
///
/// It mirrors the daemon's real rule rather than approximating it: a peer that
/// is not in the map, or whose grant is off, gets [`ClipboardPolicy::DENIED`]
/// — so a test cannot accidentally pass by having a permissive default the
/// daemon does not have.
#[derive(Default)]
pub struct TestAuthorizer {
    peers: RwLock<HashMap<Fingerprint, (bool, ClipboardPolicy)>>,
}

impl TestAuthorizer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Grants `clipboard.v1` with the given policy.
    pub async fn grant(&self, peer: Fingerprint, policy: ClipboardPolicy) {
        self.peers.write().await.insert(peer, (true, policy));
    }

    /// Known, but with the capability not granted (or revoked).
    pub async fn revoke(&self, peer: Fingerprint) {
        let mut peers = self.peers.write().await;
        if let Some(entry) = peers.get_mut(&peer) {
            entry.0 = false;
        } else {
            peers.insert(peer, (false, ClipboardPolicy::default()));
        }
    }

    pub async fn set_policy(&self, peer: Fingerprint, policy: ClipboardPolicy) {
        let mut peers = self.peers.write().await;
        let granted = peers.get(&peer).map(|e| e.0).unwrap_or(true);
        peers.insert(peer, (granted, policy));
    }
}

#[async_trait::async_trait]
impl ClipboardAuthorizer for TestAuthorizer {
    async fn policy_for(&self, peer: &Fingerprint) -> ClipboardPolicy {
        match self.peers.read().await.get(peer) {
            Some((true, policy)) => *policy,
            _ => ClipboardPolicy::DENIED,
        }
    }

    async fn auto_send_peers(&self) -> Vec<Fingerprint> {
        self.peers
            .read()
            .await
            .iter()
            .filter(|(_, (granted, policy))| *granted && policy.may_auto_send())
            .map(|(fp, _)| *fp)
            .collect()
    }
}

/// One device under test: a manager, its clipboard, and its authorizer.
pub struct Device {
    pub manager: Arc<ClipboardManager>,
    pub backend: Arc<MemoryBackend>,
    pub authorizer: Arc<TestAuthorizer>,
    pub device_id: String,
}

impl Device {
    pub async fn new(device_id: &str) -> Self {
        let backend = Arc::new(MemoryBackend::new());
        let manager = ClipboardManager::new(
            Arc::clone(&backend)
                as Arc<dyn omnibridge_capability_clipboard::backend::ClipboardBackend>,
            device_id,
        );
        let authorizer = TestAuthorizer::new();
        manager
            .set_authorizer(Arc::clone(&authorizer) as Arc<dyn ClipboardAuthorizer>)
            .await;
        Self {
            manager,
            backend,
            authorizer,
            device_id: device_id.to_string(),
        }
    }

    /// Attaches a peer session and returns the receiver the peer would read.
    pub async fn connect(&self, peer: Fingerprint) -> mpsc::Receiver<OutboundMessage> {
        let (tx, rx) = mpsc::channel(256);
        self.manager.attach_session(peer, tx).await;
        rx
    }
}

/// Builds a `ClipboardUpdate` payload as a peer would send one.
pub fn update_payload(
    event_id: &[u8],
    origin_device_id: &str,
    text: &str,
    sensitive: bool,
) -> Vec<u8> {
    update_payload_with_hash(
        event_id,
        origin_device_id,
        text,
        sensitive,
        omnibridge_capability_clipboard::text::content_hash(text).to_vec(),
    )
}

/// The same, with a caller-chosen `content_hash` — including a wrong one.
pub fn update_payload_with_hash(
    event_id: &[u8],
    origin_device_id: &str,
    text: &str,
    sensitive: bool,
    content_hash: Vec<u8>,
) -> Vec<u8> {
    pb::ClipboardControl {
        body: Some(pb::clipboard_control::Body::Update(pb::ClipboardUpdate {
            event_id: event_id.to_vec(),
            origin_device_id: origin_device_id.to_string(),
            text_utf8: text.to_string(),
            content_hash,
            sensitive_hint: sensitive,
            timestamp_unix_ms: 1_700_000_000_000,
        })),
    }
    .encode_to_vec()
}

/// 16 bytes, distinct per `seed`.
pub fn event_id(seed: u8) -> Vec<u8> {
    vec![seed; 16]
}

/// Decodes an outbound message as a `ClipboardResult`, failing loudly if it
/// is anything else.
pub fn expect_result(message: &OutboundMessage) -> pb::ClipboardResult {
    assert_eq!(message.capability_id, CAPABILITY_ID);
    match pb::ClipboardControl::decode(message.payload.as_slice())
        .expect("a well-formed control message")
        .body
    {
        Some(pb::clipboard_control::Body::Result(r)) => r,
        other => panic!("expected a ClipboardResult, got {other:?}"),
    }
}

/// Decodes an outbound message as a `ClipboardUpdate`.
pub fn expect_update(message: &OutboundMessage) -> pb::ClipboardUpdate {
    assert_eq!(message.capability_id, CAPABILITY_ID);
    match pb::ClipboardControl::decode(message.payload.as_slice())
        .expect("a well-formed control message")
        .body
    {
        Some(pb::clipboard_control::Body::Update(u)) => u,
        other => panic!("expected a ClipboardUpdate, got {other:?}"),
    }
}

/// The outcome the manager returned for one update, read from its reply.
pub async fn handle(
    device: &Device,
    peer: Fingerprint,
    peer_device_id: &str,
    payload: &[u8],
) -> Option<pb::ClipboardOutcome> {
    let reply = device
        .manager
        .handle_control(peer, peer_device_id, payload)
        .await
        .expect("a well-formed payload is handled");
    reply.map(|message| {
        let result = expect_result(&message);
        pb::ClipboardOutcome::try_from(result.outcome).expect("a known outcome")
    })
}

/// Drains everything currently queued on a peer's session.
pub fn drain(rx: &mut mpsc::Receiver<OutboundMessage>) -> Vec<OutboundMessage> {
    let mut out = Vec::new();
    while let Ok(message) = rx.try_recv() {
        out.push(message);
    }
    out
}
