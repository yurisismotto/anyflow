//! Authenticating a data stream.
//!
//! # The problem this solves
//!
//! A data stream is a second TLS connection. TLS with a pinned identity
//! already answers "which *device* is on this socket", and that is necessary
//! but not sufficient: it says nothing about *which transfer* the connection
//! is for, and a device may legitimately have several in flight.
//!
//! The tempting shortcut is to let the dialer name a `transfer_id` and treat
//! it as proof. That makes the id a bearer token, and bearer tokens leak:
//! into logs, into a crash report, into whatever else can see the control
//! session's plaintext on a compromised device. ADR-0012 forbids it, and so
//! does the sprint brief.
//!
//! # The construction
//!
//! ```text
//! mac = HMAC-SHA256(
//!     key = stream_challenge,
//!     msg = "omnibridge/files.v1/data-stream/v1"
//!           || len_prefixed(acceptor_fingerprint)
//!           || len_prefixed(dialer_fingerprint)
//!           || len_prefixed(transfer_id))
//! ```
//!
//! This is deliberately the same shape as the pairing proof in
//! `omnibridge_core::pairing`: a standard MAC, a domain separator, and every
//! field length-prefixed so two different field splits cannot produce the
//! same message. No new cryptography was invented here; a reader who has
//! understood the pairing proof has already understood this.
//!
//! What each part buys, against the attacks in the brief:
//!
//! | Binding | Stops |
//! | --- | --- |
//! | challenge is random, single-use, and only ever sent inside the control session's TLS | transfer-id guessing (F4), replay (F5), using an id as a bearer token |
//! | acceptor fingerprint | a proof captured against one machine being used against another |
//! | dialer fingerprint | a different device replaying a captured proof (F3) |
//! | transfer id | a stream authorized for one transfer being attached to another (hijacking) |
//!
//! The acceptor *additionally* requires that the TLS peer certificate on the
//! data connection is the same pinned identity that negotiated the transfer
//! on the control session. The MAC is defence in depth on top of that check,
//! never a substitute for it: an unknown peer (F2) is refused by TLS and by
//! the trust store before this module is reached.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use omnibridge_core::error::{Error, Result};
use omnibridge_core::Fingerprint;
use rand::TryRngCore;

use crate::limits::{STREAM_CHALLENGE_LEN, TRANSFER_ID_LEN};
use crate::transfer::TransferId;

type HmacSha256 = Hmac<Sha256>;

/// Domain separator. Versioned with the capability, so a future `files.v2`
/// cannot have a proof from `files.v1` replayed into it.
const DATA_STREAM_DOMAIN: &[u8] = b"omnibridge/files.v1/data-stream/v1";

/// The single-use secret that keys a data stream's MAC.
///
/// Not `Clone` and not printable, and zeroed on drop, exactly like a
/// [`PairingToken`]. It is a key, and keys do not belong in logs, in `Debug`
/// output or in memory after use.
///
/// [`PairingToken`]: omnibridge_core::pairing::PairingToken
pub struct StreamChallenge([u8; STREAM_CHALLENGE_LEN]);

impl StreamChallenge {
    /// Generates a fresh challenge from the OS CSPRNG.
    pub fn generate() -> Result<Self> {
        let mut buf = [0u8; STREAM_CHALLENGE_LEN];
        rand::rngs::OsRng
            .try_fill_bytes(&mut buf)
            .map_err(|_| Error::Store("system CSPRNG unavailable".into()))?;
        Ok(Self(buf))
    }

    /// Reconstructs a challenge received over the control session.
    ///
    /// Rejects anything that is not exactly [`STREAM_CHALLENGE_LEN`] bytes, so
    /// a peer cannot weaken the MAC by offering a short — or empty — key.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; STREAM_CHALLENGE_LEN] = bytes.try_into().ok()?;
        Some(Self(arr))
    }

    /// The bytes to place in a control message. The only legitimate way this
    /// value leaves the process, and only ever inside the control session's
    /// TLS.
    pub fn expose_for_control_message(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl Drop for StreamChallenge {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}

impl std::fmt::Debug for StreamChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StreamChallenge(<redacted>)")
    }
}

/// Generates a transfer id from the OS CSPRNG.
pub fn generate_transfer_id() -> Result<TransferId> {
    let mut buf = [0u8; TRANSFER_ID_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|_| Error::Store("system CSPRNG unavailable".into()))?;
    TransferId::from_bytes(&buf).ok_or(Error::Protocol("transfer id length"))
}

/// Computes the data-stream MAC.
///
/// `acceptor` is the device that accepts data-stream connections and issued
/// the challenge (the desktop daemon); `dialer` is the device that opens the
/// stream and proves it (the phone). The roles are fixed by which side
/// listens, never by which side is sending the file.
pub fn compute_stream_mac(
    challenge: &StreamChallenge,
    acceptor: &Fingerprint,
    dialer: &Fingerprint,
    transfer_id: &TransferId,
) -> [u8; 32] {
    // HMAC accepts a key of any length, so this cannot fail.
    let mut m =
        <HmacSha256 as Mac>::new_from_slice(&challenge.0).expect("HMAC accepts keys of any length");
    m.update(DATA_STREAM_DOMAIN);
    update_len_prefixed(&mut m, acceptor.as_bytes());
    update_len_prefixed(&mut m, dialer.as_bytes());
    update_len_prefixed(&mut m, transfer_id.as_bytes());
    m.finalize().into_bytes().into()
}

fn update_len_prefixed(m: &mut HmacSha256, data: &[u8]) {
    m.update(&(data.len() as u32).to_be_bytes());
    m.update(data);
}

/// Constant-time comparison of a received MAC against the expected value.
///
/// Constant-time because a byte-at-a-time comparison would let an attacker
/// with many attempts learn the expected MAC one byte per round trip. The
/// challenge is single-use, which already makes that impractical, but a
/// timing-safe compare costs nothing and removes the argument.
pub fn verify_stream_mac(expected: &[u8; 32], received: &[u8]) -> bool {
    if received.len() != expected.len() {
        return false;
    }
    expected.ct_eq(received).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built from hex rather than raw bytes so the test uses the same
    /// constructor the protocol does, with no test-only entry point added to
    /// `Fingerprint` for its convenience.
    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_hex(&format!("{byte:02x}").repeat(32)).expect("32 bytes of hex")
    }

    fn challenge(byte: u8) -> StreamChallenge {
        StreamChallenge::from_bytes(&[byte; 32]).expect("32 bytes")
    }

    fn id(byte: u8) -> TransferId {
        TransferId::from_bytes(&[byte; 16]).expect("16 bytes")
    }

    #[test]
    fn a_correct_mac_verifies() {
        let mac = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        assert!(verify_stream_mac(&mac, &mac));
    }

    #[test]
    fn a_different_challenge_does_not_verify() {
        let expected = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        let other = compute_stream_mac(&challenge(9), &fp(2), &fp(3), &id(4));
        assert!(!verify_stream_mac(&expected, &other));
    }

    #[test]
    fn a_proof_for_one_transfer_does_not_authorize_another() {
        // The transfer-hijacking case: same peers, same challenge, different
        // transfer.
        let expected = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        let other = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(5));
        assert!(!verify_stream_mac(&expected, &other));
    }

    #[test]
    fn a_proof_is_worthless_against_a_different_acceptor() {
        let expected = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        let other = compute_stream_mac(&challenge(1), &fp(9), &fp(3), &id(4));
        assert!(!verify_stream_mac(&expected, &other));
    }

    #[test]
    fn a_proof_cannot_be_replayed_by_a_different_dialer() {
        let expected = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        let other = compute_stream_mac(&challenge(1), &fp(2), &fp(9), &id(4));
        assert!(!verify_stream_mac(&expected, &other));
    }

    #[test]
    fn swapping_the_two_fingerprints_changes_the_mac() {
        // Length-prefixing is what guarantees this: without it, a
        // concatenation of two 32-byte values would be ambiguous.
        let a = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        let b = compute_stream_mac(&challenge(1), &fp(3), &fp(2), &id(4));
        assert!(!verify_stream_mac(&a, &b));
    }

    #[test]
    fn a_truncated_or_padded_mac_is_refused() {
        let mac = compute_stream_mac(&challenge(1), &fp(2), &fp(3), &id(4));
        assert!(!verify_stream_mac(&mac, &mac[..31]));
        assert!(!verify_stream_mac(&mac, &[]));
        let mut long = mac.to_vec();
        long.push(0);
        assert!(!verify_stream_mac(&mac, &long));
    }

    #[test]
    fn a_challenge_must_be_exactly_thirty_two_bytes() {
        // A peer that could supply a short or empty challenge would be
        // choosing the MAC key.
        assert!(StreamChallenge::from_bytes(&[]).is_none());
        assert!(StreamChallenge::from_bytes(&[0u8; 31]).is_none());
        assert!(StreamChallenge::from_bytes(&[0u8; 33]).is_none());
        assert!(StreamChallenge::from_bytes(&[0u8; 32]).is_some());
    }

    #[test]
    fn a_challenge_never_prints_its_contents() {
        let c = challenge(0xab);
        assert_eq!(format!("{c:?}"), "StreamChallenge(<redacted>)");
    }

    #[test]
    fn generated_challenges_and_ids_differ_every_time() {
        let a = StreamChallenge::generate().expect("csprng");
        let b = StreamChallenge::generate().expect("csprng");
        assert_ne!(
            a.expose_for_control_message(),
            b.expose_for_control_message()
        );

        let x = generate_transfer_id().expect("csprng");
        let y = generate_transfer_id().expect("csprng");
        assert_ne!(x, y);
    }

    /// The value the Kotlin suite must reproduce.
    ///
    /// Pinning it here makes the MAC a checked cross-language contract rather
    /// than two implementations that happen to agree today. `KeyDigestsTest`
    /// and `PairingProofTest` do the same for the other shared constructions.
    #[test]
    fn the_mac_matches_the_published_cross_language_vector() {
        let mac = compute_stream_mac(&challenge(0x01), &fp(0x02), &fp(0x03), &id(0x04));
        assert_eq!(
            mac.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "503aaf7d8c15b38971f4fbcc3ec34742ae27263ac94f2507ecab7c3691764576"
        );
    }
}
