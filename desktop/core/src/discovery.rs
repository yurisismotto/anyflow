//! LAN discovery over mDNS/DNS-SD.
//!
//! # Discovery is not trust
//!
//! This module answers exactly one question: *what addresses might be worth
//! dialling?* Everything it returns is attacker-controllable — anyone on the
//! link can publish a record claiming any name, id or address. Nothing here
//! feeds an authorization decision. A spoofed record leads to a TCP
//! connection that fails the pinned-key check in [`crate::tls`], which is a
//! non-event.
//!
//! # What we publish, and the privacy cost
//!
//! TXT record:
//!
//! ```text
//!   v  = 1                TXT schema version
//!   pv = "1-1"            supported protocol version range
//!   id = <device id hex>  stable pseudonymous id
//!   dn = <device name>    user-visible label
//! ```
//!
//! Publishing a stable `id` and a `dn` on an untrusted network is a real,
//! if modest, tracking signal: an observer in a café can tell that the same
//! laptop came back. We accept it in v1 because the alternative — dialling
//! every discovered service and completing a TLS handshake to find out who it
//! is — is far worse for battery and far noisier on the network. The
//! mitigation is a per-network toggle to suppress advertisement entirely;
//! see the threat model. We deliberately do **not** publish the identity
//! fingerprint, so a passive observer cannot enumerate the trust graph.

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};

/// A service instance seen on the network. Purely a hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// Value of the `id` TXT key, if present. Untrusted.
    pub device_id: Option<String>,
    /// Value of the `dn` TXT key. Untrusted, attacker-controlled: sanitize
    /// and length-cap before displaying.
    pub device_name: Option<String>,
    pub min_protocol_version: Option<u32>,
    pub max_protocol_version: Option<u32>,
    pub addresses: Vec<SocketAddr>,
}

pub const TXT_SCHEMA_VERSION: &str = "1";

/// Longest device name we will publish or accept from a TXT record.
pub const MAX_DEVICE_NAME_LEN: usize = 64;

/// Builds the TXT key/value pairs this device advertises.
pub fn build_txt(device_id: &str, device_name: &str) -> BTreeMap<String, String> {
    let mut txt = BTreeMap::new();
    txt.insert("v".into(), TXT_SCHEMA_VERSION.into());
    txt.insert(
        "pv".into(),
        format!(
            "{}-{}",
            crate::session::PROTOCOL_VERSION_MIN,
            crate::session::PROTOCOL_VERSION_MAX
        ),
    );
    txt.insert("id".into(), device_id.into());
    txt.insert("dn".into(), sanitize_device_name(device_name));
    txt
}

/// Parses a discovered instance's TXT record and addresses.
///
/// Never fails: a malformed record yields a peer with missing fields rather
/// than an error, because the only consequence of bad discovery data is a
/// wasted connection attempt.
pub fn parse_discovered(
    txt: &BTreeMap<String, String>,
    addresses: &[IpAddr],
    port: u16,
) -> DiscoveredPeer {
    let (min, max) = txt
        .get("pv")
        .and_then(|s| s.split_once('-'))
        .map(|(a, b)| (a.parse::<u32>().ok(), b.parse::<u32>().ok()))
        .unwrap_or((None, None));

    DiscoveredPeer {
        device_id: txt
            .get("id")
            .filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .cloned(),
        device_name: txt.get("dn").map(|s| sanitize_device_name(s)),
        min_protocol_version: min,
        max_protocol_version: max,
        addresses: addresses
            .iter()
            .map(|ip| SocketAddr::new(*ip, port))
            .collect(),
    }
}

/// True if this build can talk to the advertised version range.
///
/// A convenience for skipping obviously-incompatible peers before dialling.
/// It is a hint, not a check: the authoritative negotiation happens in HELLO.
pub fn version_compatible(peer: &DiscoveredPeer) -> bool {
    match (peer.min_protocol_version, peer.max_protocol_version) {
        (Some(min), Some(max)) => crate::session::negotiate_version(min, max).is_some(),
        // Missing version info: try anyway and let the handshake decide.
        _ => true,
    }
}

/// Strips control characters and caps the length of an untrusted device name.
///
/// Device names are rendered in notifications and terminal output, where a
/// raw control sequence could forge UI or corrupt a terminal.
pub fn sanitize_device_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(MAX_DEVICE_NAME_LEN)
        .collect()
}
