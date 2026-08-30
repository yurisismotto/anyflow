//! The QR code payload.
//!
//! # What goes in, and why
//!
//! ```text
//! fedroidb1:<responder-fingerprint-hex>:<token-base32>:<device-id>:<addr>[,<addr>...]
//! ```
//!
//! * **fingerprint** — the whole point. The scanning device pins this SPKI
//!   fingerprint *before* it opens a socket, so the TLS handshake is
//!   authenticated from the very first connection. There is no
//!   trust-on-first-use window and therefore no MITM window during pairing.
//! * **token** — the single-use secret proving co-presence.
//! * **device id / addresses** — routing and UI hints only. If they are
//!   wrong or spoofed the connection simply fails the pinned-key check.
//!
//! # What deliberately does not go in
//!
//! No private key material, no persistent credential, no capability grants.
//! A photographed QR is useless once the window closes or the token is used.
//!
//! The scheme carries a version tag (`b1`) so a future format can be
//! recognised and rejected cleanly rather than misparsed.

use std::net::SocketAddr;

use crate::error::{Error, Result};
use crate::fingerprint::Fingerprint;
use crate::pairing::PairingToken;

pub const QR_SCHEME: &str = "fedroidb1";

/// Cap on the encoded payload. Bounds what a malicious QR can push into the
/// parser on the phone before any of it is interpreted.
pub const MAX_QR_PAYLOAD_LEN: usize = 512;

pub struct QrPayload {
    pub fingerprint: Fingerprint,
    pub token_base32: String,
    pub device_id: String,
    /// Address hints where the responder is listening. Untrusted.
    pub addresses: Vec<SocketAddr>,
}

impl QrPayload {
    pub fn encode(
        fingerprint: &Fingerprint,
        token: &PairingToken,
        device_id: &str,
        addresses: &[SocketAddr],
    ) -> String {
        let addrs = addresses
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{}:{}:{}:{}:{}",
            QR_SCHEME,
            fingerprint.to_hex(),
            token.to_base32(),
            device_id,
            addrs
        )
    }

    /// Parses a scanned payload.
    ///
    /// Strict by construction: fixed field count, known scheme tag, and every
    /// field validated. An unparseable address is dropped rather than
    /// failing the whole payload, because addresses are only hints.
    pub fn parse(input: &str) -> Result<Self> {
        if input.len() > MAX_QR_PAYLOAD_LEN {
            return Err(Error::Protocol("QR payload too large"));
        }

        // Addresses contain ':' themselves (ports, IPv6), so split into
        // exactly 5 pieces and keep the remainder as the address list.
        let mut parts = input.splitn(5, ':');

        let scheme = parts.next().unwrap_or_default();
        if scheme != QR_SCHEME {
            return Err(Error::Protocol("unknown QR scheme"));
        }

        let fingerprint = Fingerprint::from_hex(parts.next().unwrap_or_default())?;

        let token_base32 = parts.next().unwrap_or_default().to_string();
        // Validate now so a malformed token fails at scan time rather than
        // mid-handshake. The parsed value is dropped; only the text is kept.
        let _ = PairingToken::from_base32(&token_base32)?;

        let device_id = parts.next().unwrap_or_default().to_string();
        if device_id.is_empty() || device_id.len() > 64 || !is_hex(&device_id) {
            return Err(Error::Protocol("malformed device id in QR payload"));
        }

        let addresses = parts
            .next()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<SocketAddr>().ok())
            .collect();

        Ok(Self {
            fingerprint,
            token_base32,
            device_id,
            addresses,
        })
    }

    pub fn token(&self) -> Result<PairingToken> {
        PairingToken::from_base32(&self.token_base32)
    }
}

fn is_hex(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_hexdigit())
}
