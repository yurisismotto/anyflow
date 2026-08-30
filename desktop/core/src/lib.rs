//! Fedroid Bridge core: identity, pairing, transport and the capability model.
//!
//! This crate is deliberately free of any daemon, CLI or GUI concerns. It has
//! no global state and does no logging of user content. Everything it needs
//! from the host application arrives through [`session::SessionHost`], which
//! is what makes the whole protocol testable in-process.
//!
//! Layering, bottom to top:
//!
//! ```text
//!   framing      length-prefixed protobuf frames
//!   tls          TLS 1.3 + SPKI pinning
//!   session      handshake, pairing, replay guard, routing
//!   capability   plugin registry; the transport knows no feature names
//! ```

pub mod capability;
pub mod discovery;
pub mod error;
pub mod fingerprint;
pub mod framing;
pub mod identity;
pub mod pairing;
pub mod qr;
pub mod session;
pub mod store;
pub mod tls;

pub use error::{Error, PairingError, Result};
pub use fingerprint::Fingerprint;

/// Default TCP port. Above 1024 so the daemon never needs privileges.
/// Advertised over mDNS, so a conflicting deployment can simply use another.
pub const DEFAULT_PORT: u16 = 55432;

/// ALPN identifier. Negotiated by both ends, so a client that reaches an
/// unrelated TLS service (or vice versa) fails fast during the handshake
/// instead of exchanging garbage frames.
pub const ALPN_PROTOCOL: &[u8] = b"fedroid/1";

/// DNS-SD service type used for LAN discovery.
pub const SERVICE_TYPE: &str = "_fedroid-bridge._tcp.local.";
