//! `battery.v1` — the first functional capability.
//!
//! Scope: a peer reports its battery percentage and charging state; we keep
//! the most recent reading **in memory only**. Nothing is written to disk:
//! a battery history would be a movement-and-usage log of the person holding
//! the phone, and we have no reason to keep one.
//!
//! This crate is a worked example of the plugin contract. Note that it does
//! not appear anywhere in `anyflow-core`'s transport code: the daemon
//! registers it, and the transport routes to it by id.

mod upower;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyflow_core::capability::{Capability, CapabilityContext, OutboundMessage};
use anyflow_core::error::{Error, Result};
use anyflow_core::Fingerprint;
use anyflow_proto::v1::capabilities as pb;
use prost::Message;

pub use upower::UPowerReader;

pub const CAPABILITY_ID: &str = "battery.v1";

/// A reading held for one peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryReading {
    pub percentage: u32,
    pub charging_state: pb::ChargingState,
    /// The peer's own clock, as it sent it. Display only.
    pub peer_timestamp_unix_ms: i64,
}

/// One peer's latest state plus when *we* received it.
#[derive(Debug, Clone)]
pub struct PeerBattery {
    pub reading: BatteryReading,
    /// Local monotonic receipt time. Used for staleness, because the peer's
    /// clock cannot be trusted for that.
    pub received_at: Instant,
}

/// In-memory store of the newest reading per peer.
#[derive(Default)]
pub struct BatteryState {
    peers: RwLock<HashMap<Fingerprint, PeerBattery>>,
}

impl BatteryState {
    pub fn get(&self, peer: &Fingerprint) -> Option<PeerBattery> {
        self.peers.read().ok()?.get(peer).cloned()
    }

    pub fn snapshot(&self) -> Vec<(Fingerprint, PeerBattery)> {
        self.peers
            .read()
            .map(|m| m.iter().map(|(k, v)| (*k, v.clone())).collect())
            .unwrap_or_default()
    }

    fn update(&self, peer: Fingerprint, reading: BatteryReading) {
        if let Ok(mut m) = self.peers.write() {
            // Drop an update that is older than what we already hold, but
            // only when both readings come from the same peer clock. This is
            // ordering hygiene, not security: `admit()` in the session layer
            // is what actually stops replay.
            if let Some(existing) = m.get(&peer) {
                if reading.peer_timestamp_unix_ms < existing.reading.peer_timestamp_unix_ms {
                    return;
                }
            }
            m.insert(
                peer,
                PeerBattery {
                    reading,
                    received_at: Instant::now(),
                },
            );
        }
    }

    fn forget(&self, peer: &Fingerprint) {
        if let Ok(mut m) = self.peers.write() {
            m.remove(peer);
        }
    }
}

/// The capability handler.
pub struct BatteryCapability {
    state: Arc<BatteryState>,
    /// Reads this machine's own battery, if it has one. `None` on a desktop
    /// or when the `upower` feature is off: we then only receive.
    local: Option<Arc<dyn LocalBatterySource>>,
}

/// Source of this device's own battery level.
#[async_trait::async_trait]
pub trait LocalBatterySource: Send + Sync {
    async fn read(&self) -> Option<BatteryReading>;
}

impl BatteryCapability {
    pub fn new(state: Arc<BatteryState>) -> Self {
        Self { state, local: None }
    }

    /// Also report our own battery to peers.
    pub fn with_local_source(mut self, source: Arc<dyn LocalBatterySource>) -> Self {
        self.local = Some(source);
        self
    }

    pub fn state(&self) -> Arc<BatteryState> {
        Arc::clone(&self.state)
    }

    /// Encodes a reading for the wire. Public so the daemon can push updates.
    pub fn encode(reading: &BatteryReading) -> OutboundMessage {
        let msg = pb::BatteryState {
            percentage: reading.percentage,
            charging_state: reading.charging_state as i32,
            timestamp_unix_ms: reading.peer_timestamp_unix_ms,
        };
        OutboundMessage {
            capability_id: CAPABILITY_ID.to_string(),
            payload: msg.encode_to_vec(),
        }
    }

    /// Decodes and validates an inbound payload.
    ///
    /// Every field is checked. `percentage` in particular is bounded here and
    /// not at the display layer, so a hostile peer cannot push a value that
    /// overflows a progress bar or a format string downstream.
    pub fn decode(payload: &[u8]) -> Result<BatteryReading> {
        let msg = pb::BatteryState::decode(payload)?;
        if msg.percentage > 100 {
            return Err(Error::Protocol("battery percentage out of range"));
        }
        let charging_state = pb::ChargingState::try_from(msg.charging_state)
            .unwrap_or(pb::ChargingState::Unspecified);
        Ok(BatteryReading {
            percentage: msg.percentage,
            charging_state,
            peer_timestamp_unix_ms: msg.timestamp_unix_ms,
        })
    }
}

#[async_trait::async_trait]
impl Capability for BatteryCapability {
    fn id(&self) -> &str {
        CAPABILITY_ID
    }

    async fn on_peer_connected(&self, ctx: &CapabilityContext) -> Result<()> {
        // Push our own level once on connect, so the peer has something to
        // show immediately rather than waiting for the next change event.
        if let Some(src) = &self.local {
            if let Some(reading) = src.read().await {
                let _ = ctx.outbound.send(Self::encode(&reading)).await;
            }
        }
        Ok(())
    }

    async fn on_message(&self, ctx: &CapabilityContext, payload: &[u8]) -> Result<()> {
        let reading = Self::decode(payload)?;
        // Logged at debug with the level only. A battery percentage is not
        // sensitive the way clipboard content is, but it is still peer data,
        // so it stays out of info-level logs.
        tracing::debug!(
            peer = %ctx.peer.to_display_short(),
            percentage = reading.percentage,
            "battery update"
        );
        self.state.update(ctx.peer, reading);
        Ok(())
    }

    async fn on_peer_disconnected(&self, peer: &Fingerprint) -> Result<()> {
        // The value is meaningless once the peer is gone, and keeping it
        // would be a small, pointless retention of someone else's data.
        self.state.forget(peer);
        Ok(())
    }
}
