//! mDNS/DNS-SD advertisement.
//!
//! The daemon only *advertises*; it does not browse. Android is always the
//! initiator: a phone's address changes constantly and its ability to accept
//! inbound connections is unreliable, while a desktop is a stable listener.
//! Making the connection direction fixed also means there is exactly one
//! handshake path to reason about.

use std::collections::HashMap;

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use omnibridge_core::discovery;

use crate::listener::Families;

/// Live advertisement. Dropping this withdraws the record.
pub struct Advertisement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertisement {
    /// Publishes the service record.
    ///
    /// `families` says which address families the TCP listener actually
    /// accepts on, and the record is restricted to match. Advertising an
    /// address the daemon cannot accept on is worse than advertising nothing:
    /// the phone dials it, the connection is refused, and the failure looks
    /// identical to the computer being asleep. That is exactly the defect
    /// this parameter exists to prevent.
    pub fn publish(
        device_id: &str,
        device_name: &str,
        port: u16,
        families: Families,
    ) -> anyhow::Result<Self> {
        let daemon = ServiceDaemon::new()?;

        if !families.ipv6 {
            // No AAAA records and no IPv6 responder: nothing here can be
            // reached over IPv6, so nothing here claims to be.
            daemon.disable_interface(IfKind::IPv6)?;
        }
        if !families.ipv4 {
            daemon.disable_interface(IfKind::IPv4)?;
        }

        let properties: HashMap<String, String> = discovery::build_txt(device_id, device_name)
            .into_iter()
            .collect();

        // The instance name is the device id, not the human-readable name:
        // DNS-SD instance names must be unique on the link, and two machines
        // called "fedora" is the common case, not the exotic one.
        let instance = device_id;
        let hostname = format!("{device_id}.local.");

        let service = ServiceInfo::new(
            omnibridge_core::SERVICE_TYPE,
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

        tracing::info!(
            port,
            families = %families,
            "advertising {}",
            omnibridge_core::SERVICE_TYPE
        );
        Ok(Self { daemon, fullname })
    }
}

impl Drop for Advertisement {
    fn drop(&mut self) {
        // Best-effort goodbye packet so peers do not keep a stale record.
        let _ = self.daemon.unregister(&self.fullname);
    }
}
