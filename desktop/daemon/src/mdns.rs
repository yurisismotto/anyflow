//! mDNS/DNS-SD advertisement.
//!
//! The daemon only *advertises*; it does not browse. Android is always the
//! initiator: a phone's address changes constantly and its ability to accept
//! inbound connections is unreliable, while a desktop is a stable listener.
//! Making the connection direction fixed also means there is exactly one
//! handshake path to reason about.

use std::collections::HashMap;

use anyflow_core::discovery;
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Live advertisement. Dropping this withdraws the record.
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertisement {
    pub fn publish(device_id: &str, device_name: &str, port: u16) -> anyhow::Result<Self> {
        let daemon = ServiceDaemon::new()?;

        let properties: HashMap<String, String> = discovery::build_txt(device_id, device_name)
            .into_iter()
            .collect();

        // The instance name is the device id, not the human-readable name:
        // DNS-SD instance names must be unique on the link, and two machines
        // called "fedora" is the common case, not the exotic one.
        let instance = device_id;
        let hostname = format!("{device_id}.local.");

        let service = ServiceInfo::new(
            anyflow_core::SERVICE_TYPE,
            instance,
            &hostname,
            "",
            port,
            properties,
        )?
        // Let the mDNS stack track interface addresses itself, so the record
        // stays correct across Wi-Fi/dock changes without a restart.
        .enable_addr_auto();

        let fullname = service.get_fullname().to_string();
        daemon.register(service)?;

        tracing::info!(port, "advertising {}", anyflow_core::SERVICE_TYPE);
        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        // Best-effort goodbye packet so peers do not keep a stale record.
        let _ = self.daemon.unregister(&self.fullname);
    }
}
