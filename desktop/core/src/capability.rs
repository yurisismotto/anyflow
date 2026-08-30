//! The capability plugin model.
//!
//! Every feature beyond the handshake is a capability: a versioned id, a
//! handler, and a schema the transport knows nothing about. The transport
//! sees only `CapabilityMessage { capability_id, payload }` and routes on the
//! id. There is no `if capability == "battery"` anywhere below the registry,
//! which is what makes clipboard, files and notifications additive changes
//! later rather than surgery on the transport.
//!
//! # Versioning
//!
//! The version is part of the id (`battery.v1`), not a separate field. A
//! breaking change ships as `battery.v2` and both can be advertised at once
//! during a migration. There is no negotiation of a version *within* a
//! capability: two ids either match or they don't.
//!
//! # Authorization
//!
//! Advertising is not authorization. A peer says what it supports in HELLO;
//! the receiver independently decides what that peer is allowed to use, from
//! its own trust store. Both checks happen in [`CapabilityRegistry::dispatch`]
//! callers — see `session.rs`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::Result;
use crate::fingerprint::Fingerprint;

/// Context handed to a capability when a message arrives for it.
pub struct CapabilityContext {
    /// Pinned identity of the peer the message came from.
    pub peer: Fingerprint,
    pub peer_device_id: String,
    /// Sends a message back to this peer, from inside a handler.
    pub outbound: tokio::sync::mpsc::Sender<OutboundMessage>,
}

/// A capability message queued for delivery.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub capability_id: String,
    pub payload: Vec<u8>,
}

/// One feature of the protocol.
#[async_trait::async_trait]
pub trait Capability: Send + Sync {
    /// Versioned id, e.g. `"battery.v1"`.
    fn id(&self) -> &str;

    /// Called when a peer connects and this capability is mutually supported
    /// and authorized.
    async fn on_peer_connected(&self, _ctx: &CapabilityContext) -> Result<()> {
        Ok(())
    }

    /// Called for each inbound message addressed to this capability.
    ///
    /// `payload` is untrusted, attacker-controlled bytes: validate before
    /// use. Returning an error is logged and does not tear down the
    /// connection — one misbehaving capability must not kill the others.
    async fn on_message(&self, ctx: &CapabilityContext, payload: &[u8]) -> Result<()>;

    /// Called when the peer disconnects.
    async fn on_peer_disconnected(&self, _peer: &Fingerprint) -> Result<()> {
        Ok(())
    }
}

/// The set of capabilities this device implements.
#[derive(Clone, Default)]
pub struct CapabilityRegistry {
    inner: Arc<BTreeMap<String, Arc<dyn Capability>>>,
}

impl CapabilityRegistry {
    pub fn builder() -> CapabilityRegistryBuilder {
        CapabilityRegistryBuilder::default()
    }

    /// Ids this device advertises, sorted for a deterministic HELLO.
    pub fn advertised(&self) -> Vec<String> {
        self.inner.keys().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Capability>> {
        self.inner.get(id)
    }

    pub fn supports(&self, id: &str) -> bool {
        self.inner.contains_key(id)
    }

    /// Capabilities both sides support, in sorted order.
    pub fn negotiate(&self, peer_advertised: &[String]) -> Vec<String> {
        let mut out: Vec<String> = peer_advertised
            .iter()
            .filter(|id| self.supports(id))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

#[derive(Default)]
pub struct CapabilityRegistryBuilder {
    inner: BTreeMap<String, Arc<dyn Capability>>,
}

impl CapabilityRegistryBuilder {
    pub fn register(mut self, capability: Arc<dyn Capability>) -> Self {
        self.inner.insert(capability.id().to_string(), capability);
        self
    }

    pub fn build(self) -> CapabilityRegistry {
        CapabilityRegistry {
            inner: Arc::new(self.inner),
        }
    }
}
