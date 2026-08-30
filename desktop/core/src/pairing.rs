//! Pairing: turning an unknown device into a trusted peer.
//!
//! # What the pairing token is and is not
//!
//! The token is a short-lived, single-use, 160-bit random secret shown in a
//! QR code. It is **not** a credential: it never authorizes anything by
//! itself, it is never sent over the wire, and it is destroyed the moment
//! pairing completes. Its only job is to prove, once, that the human holding
//! the phone is the same human sitting at the computer.
//!
//! The durable credential is the identity keypair on each side. After
//! pairing, the token's existence is irrelevant.
//!
//! # Why the proof is bound to both identities and a nonce
//!
//! ```text
//! proof = HMAC-SHA256(
//!     key = token,
//!     msg = domain_separator
//!           || len_prefixed(responder_fingerprint)
//!           || len_prefixed(initiator_fingerprint)
//!           || len_prefixed(nonce))
//! ```
//!
//! * Binding to the **responder** fingerprint means a proof captured on one
//!   machine is worthless against another machine.
//! * Binding to the **initiator** fingerprint means a captured proof cannot
//!   be replayed by a different device: the responder recomputes the proof
//!   using the fingerprint of the TLS peer that actually connected, so a
//!   replaying attacker would need the victim's private key.
//! * Binding to a fresh server **nonce** means a proof cannot be replayed
//!   even by the same device on a later connection.
//!
//! Length-prefixing every field prevents a concatenation ambiguity where two
//! different field splits hash to the same message.
//!
//! We use HMAC-SHA256, a standard MAC, keyed by the token. No custom
//! construction (implementation rule 1).

use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::TryRngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::{Error, PairingError, Result};
use crate::fingerprint::Fingerprint;

type HmacSha256 = Hmac<Sha256>;

const PROOF_DOMAIN: &[u8] = b"anyflow/pairing-proof/v1";
const CONFIRM_DOMAIN: &[u8] = b"anyflow/pairing-confirm/v1";

/// 160 bits. Comfortably beyond brute force even without rate limiting, and
/// it still encodes to a QR code that a phone camera reads instantly.
pub const TOKEN_LEN: usize = 20;

/// Server challenge size.
pub const NONCE_LEN: usize = 32;

/// How long a pairing window stays open. Short enough that a shoulder-surfed
/// QR photo is near-useless, long enough for a person to pick up their phone.
pub const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(120);

/// Failed proof attempts tolerated before the whole pairing session aborts.
///
/// With a 160-bit token this is belt-and-braces, but it also bounds the CPU
/// an unpaired peer can make us spend and turns a noisy online guessing
/// attempt into an immediately visible failure.
pub const MAX_FAILED_ATTEMPTS: u32 = 3;

/// The secret behind a QR code.
///
/// Deliberately not `Clone` and not `Debug`-printable, and zeroed on drop.
pub struct PairingToken([u8; TOKEN_LEN]);

impl PairingToken {
    pub fn generate() -> Result<Self> {
        let mut buf = [0u8; TOKEN_LEN];
        rand::rngs::OsRng
            .try_fill_bytes(&mut buf)
            .map_err(|_| Error::Store("system CSPRNG unavailable".into()))?;
        Ok(Self(buf))
    }

    /// Unpadded RFC 4648 base32, uppercase: 32 characters for 20 bytes.
    /// Base32 keeps the QR in alphanumeric mode, which yields a lower-density
    /// and therefore more reliably scannable code than base64 would.
    pub fn to_base32(&self) -> String {
        data_encoding::BASE32_NOPAD.encode(&self.0)
    }

    pub fn from_base32(s: &str) -> Result<Self> {
        let raw = data_encoding::BASE32_NOPAD
            .decode(s.as_bytes())
            .map_err(|_| Error::Pairing(PairingError::BadProof))?;
        let arr: [u8; TOKEN_LEN] = raw
            .try_into()
            .map_err(|_| Error::Pairing(PairingError::BadProof))?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; TOKEN_LEN] {
        &self.0
    }
}

impl Drop for PairingToken {
    /// Scrubbed via `zeroize`, which guarantees the write is not optimized
    /// away. Rolling our own would need `unsafe`, which this crate forbids.
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

impl std::fmt::Debug for PairingToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingToken(<redacted>)")
    }
}

/// Computes the pairing proof.
///
/// `responder` is the device that displayed the QR (the desktop);
/// `initiator` is the device that scanned it (the phone).
pub fn compute_proof(
    token: &PairingToken,
    responder: &Fingerprint,
    initiator: &Fingerprint,
    nonce: &[u8],
) -> [u8; 32] {
    mac(PROOF_DOMAIN, token, responder, initiator, nonce)
}

/// Computes the responder's confirmation, proving it also knew the token.
pub fn compute_confirmation(
    token: &PairingToken,
    responder: &Fingerprint,
    initiator: &Fingerprint,
    nonce: &[u8],
) -> [u8; 32] {
    mac(CONFIRM_DOMAIN, token, responder, initiator, nonce)
}

fn mac(
    domain: &[u8],
    token: &PairingToken,
    responder: &Fingerprint,
    initiator: &Fingerprint,
    nonce: &[u8],
) -> [u8; 32] {
    // HMAC accepts a key of any length, so this cannot fail.
    let mut m = <HmacSha256 as Mac>::new_from_slice(token.as_bytes())
        .expect("HMAC accepts keys of any length");
    m.update(domain);
    update_len_prefixed(&mut m, responder.as_bytes());
    update_len_prefixed(&mut m, initiator.as_bytes());
    update_len_prefixed(&mut m, nonce);
    m.finalize().into_bytes().into()
}

fn update_len_prefixed(m: &mut HmacSha256, data: &[u8]) {
    m.update(&(data.len() as u32).to_be_bytes());
    m.update(data);
}

/// Constant-time comparison of a received proof against the expected value.
pub fn verify_proof(expected: &[u8; 32], received: &[u8]) -> bool {
    if received.len() != expected.len() {
        return false;
    }
    expected.ct_eq(received).into()
}

/// Generates a fresh single-use server nonce.
pub fn generate_nonce() -> Result<[u8; NONCE_LEN]> {
    let mut buf = [0u8; NONCE_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|_| Error::Store("system CSPRNG unavailable".into()))?;
    Ok(buf)
}

/// An open pairing window on the responder side.
///
/// Exactly one may be active at a time. Expiry is judged against this
/// process's own monotonic clock (`Instant`), never against a timestamp sent
/// by a peer and never against wall-clock time, so neither clock skew nor a
/// lying peer can extend the window.
pub struct PairingSession {
    token: PairingToken,
    opened_at: Instant,
    ttl: Duration,
    failed_attempts: u32,
    /// Set once a proof verifies. Enforces single use even if two
    /// connections race.
    consumed: bool,
}

impl PairingSession {
    pub fn new(ttl: Duration) -> Result<Self> {
        Ok(Self {
            token: PairingToken::generate()?,
            opened_at: Instant::now(),
            ttl,
            failed_attempts: 0,
            consumed: false,
        })
    }

    pub fn token(&self) -> &PairingToken {
        &self.token
    }

    pub fn is_expired(&self) -> bool {
        self.opened_at.elapsed() >= self.ttl
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed
    }

    pub fn remaining(&self) -> Duration {
        self.ttl.saturating_sub(self.opened_at.elapsed())
    }

    /// Checks a peer's proof and, on success, consumes the session.
    ///
    /// Every rejection path is checked *before* the MAC comparison so that an
    /// expired or exhausted session costs an attacker nothing to discover and
    /// gives them no oracle.
    pub fn verify_and_consume(
        &mut self,
        responder: &Fingerprint,
        initiator: &Fingerprint,
        nonce: &[u8],
        received_proof: &[u8],
    ) -> std::result::Result<[u8; 32], PairingError> {
        if self.consumed {
            return Err(PairingError::AlreadyUsed);
        }
        if self.is_expired() {
            return Err(PairingError::Expired);
        }
        if self.failed_attempts >= MAX_FAILED_ATTEMPTS {
            return Err(PairingError::RateLimited);
        }

        let expected = compute_proof(&self.token, responder, initiator, nonce);
        if !verify_proof(&expected, received_proof) {
            self.failed_attempts += 1;
            return Err(PairingError::BadProof);
        }

        self.consumed = true;
        Ok(compute_confirmation(
            &self.token,
            responder,
            initiator,
            nonce,
        ))
    }

    pub fn failed_attempts(&self) -> u32 {
        self.failed_attempts
    }

    /// True once the window is useless and the caller should drop it.
    pub fn is_exhausted(&self) -> bool {
        self.consumed || self.is_expired() || self.failed_attempts >= MAX_FAILED_ATTEMPTS
    }
}
