//! Pairing security tests: expiry, single-use, replay, binding, rate limiting.

use std::time::Duration;

use fedroid_core::error::PairingError;
use fedroid_core::pairing::{
    self, PairingSession, PairingToken, MAX_FAILED_ATTEMPTS, NONCE_LEN, TOKEN_LEN,
};
use fedroid_core::Fingerprint;

fn fp(byte: u8) -> Fingerprint {
    Fingerprint::from_hex(&format!("{byte:02x}").repeat(32)).expect("valid fingerprint")
}

#[test]
fn token_has_the_expected_entropy_and_encoding() {
    let token = PairingToken::generate().expect("generate");
    assert_eq!(token.as_bytes().len(), TOKEN_LEN);
    assert_eq!(TOKEN_LEN * 8, 160, "token must carry 160 bits");

    let encoded = token.to_base32();
    // Unpadded base32: 20 bytes -> 32 characters.
    assert_eq!(encoded.len(), 32);
    assert!(encoded
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));

    let decoded = PairingToken::from_base32(&encoded).expect("round trip");
    assert_eq!(decoded.as_bytes(), token.as_bytes());
}

#[test]
fn two_tokens_are_never_equal() {
    let a = PairingToken::generate().expect("a");
    let b = PairingToken::generate().expect("b");
    assert_ne!(a.as_bytes(), b.as_bytes());
}

#[test]
fn token_debug_never_leaks_the_secret() {
    let token = PairingToken::generate().expect("generate");
    let rendered = format!("{token:?}");
    assert_eq!(rendered, "PairingToken(<redacted>)");
    assert!(!rendered.contains(&token.to_base32()));
}

// ---------------------------------------------------------------------------
// Proof construction
// ---------------------------------------------------------------------------

#[test]
fn proof_is_deterministic_for_identical_inputs() {
    let token = PairingToken::generate().expect("t");
    let nonce = [3u8; NONCE_LEN];
    let a = pairing::compute_proof(&token, &fp(1), &fp(2), &nonce);
    let b = pairing::compute_proof(&token, &fp(1), &fp(2), &nonce);
    assert_eq!(a, b);
}

#[test]
fn proof_is_bound_to_the_responder_identity() {
    // A proof captured while pairing with one desktop must be useless
    // against a different desktop.
    let token = PairingToken::generate().expect("t");
    let nonce = [3u8; NONCE_LEN];
    let honest = pairing::compute_proof(&token, &fp(1), &fp(2), &nonce);
    let attacker = pairing::compute_proof(&token, &fp(0xAA), &fp(2), &nonce);
    assert_ne!(honest, attacker);
}

#[test]
fn proof_is_bound_to_the_initiator_identity() {
    // A captured proof must not be usable by a different phone.
    let token = PairingToken::generate().expect("t");
    let nonce = [3u8; NONCE_LEN];
    let honest = pairing::compute_proof(&token, &fp(1), &fp(2), &nonce);
    let impostor = pairing::compute_proof(&token, &fp(1), &fp(0xBB), &nonce);
    assert_ne!(honest, impostor);
}

#[test]
fn proof_is_bound_to_the_nonce() {
    let token = PairingToken::generate().expect("t");
    let a = pairing::compute_proof(&token, &fp(1), &fp(2), &[1u8; NONCE_LEN]);
    let b = pairing::compute_proof(&token, &fp(1), &fp(2), &[2u8; NONCE_LEN]);
    assert_ne!(a, b);
}

#[test]
fn proof_and_confirmation_are_domain_separated() {
    // The two MACs cover the same fields. Without domain separation, a
    // captured proof could be replayed back as a confirmation.
    let token = PairingToken::generate().expect("t");
    let nonce = [3u8; NONCE_LEN];
    let proof = pairing::compute_proof(&token, &fp(1), &fp(2), &nonce);
    let confirmation = pairing::compute_confirmation(&token, &fp(1), &fp(2), &nonce);
    assert_ne!(proof, confirmation);
}

#[test]
fn field_boundaries_cannot_be_shifted() {
    // Length-prefixing must make it impossible to move bytes between
    // adjacent fields and land on the same MAC.
    let token = PairingToken::generate().expect("t");
    let a = pairing::compute_proof(&token, &fp(1), &fp(2), b"ABCD");
    let b = pairing::compute_proof(&token, &fp(1), &fp(2), b"ABC");
    assert_ne!(a, b);
}

#[test]
fn verify_proof_rejects_wrong_length() {
    let expected = [1u8; 32];
    assert!(!pairing::verify_proof(&expected, &[]));
    assert!(!pairing::verify_proof(&expected, &[1u8; 31]));
    assert!(!pairing::verify_proof(&expected, &[1u8; 33]));
    assert!(pairing::verify_proof(&expected, &[1u8; 32]));
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

fn valid_proof(session: &PairingSession, nonce: &[u8]) -> [u8; 32] {
    pairing::compute_proof(session.token(), &fp(1), &fp(2), nonce)
}

#[test]
fn a_valid_proof_is_accepted_once() {
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let nonce = [5u8; NONCE_LEN];
    let proof = valid_proof(&session, &nonce);

    let confirmation = session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &proof)
        .expect("first attempt should succeed");

    assert_eq!(
        confirmation,
        pairing::compute_confirmation(session.token(), &fp(1), &fp(2), &nonce)
    );
    assert!(session.is_consumed());
}

#[test]
fn a_token_cannot_be_used_twice() {
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let nonce = [5u8; NONCE_LEN];
    let proof = valid_proof(&session, &nonce);

    session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &proof)
        .expect("first use");

    // Replaying the exact same proof must fail.
    let err = session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &proof)
        .expect_err("replay must be rejected");
    assert_eq!(err, PairingError::AlreadyUsed);
}

#[test]
fn an_expired_token_is_rejected() {
    // Zero TTL: expired the instant it exists.
    let mut session = PairingSession::new(Duration::ZERO).expect("session");
    let nonce = [5u8; NONCE_LEN];
    let proof = valid_proof(&session, &nonce);

    let err = session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &proof)
        .expect_err("expired token must be rejected");
    assert_eq!(err, PairingError::Expired);
}

#[test]
fn expiry_uses_a_monotonic_clock_not_a_peer_timestamp() {
    // The session exposes no way to influence its own expiry from outside,
    // which is the property we care about: a lying peer cannot extend it.
    let session = PairingSession::new(Duration::from_millis(50)).expect("session");
    assert!(!session.is_expired());
    std::thread::sleep(Duration::from_millis(80));
    assert!(session.is_expired());
    assert!(session.is_exhausted());
}

#[test]
fn a_wrong_proof_is_rejected_and_counted() {
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let nonce = [5u8; NONCE_LEN];

    let err = session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &[0u8; 32])
        .expect_err("bad proof");
    assert_eq!(err, PairingError::BadProof);
    assert_eq!(session.failed_attempts(), 1);
    assert!(!session.is_consumed());
}

#[test]
fn brute_force_is_cut_off_after_a_few_attempts() {
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let nonce = [5u8; NONCE_LEN];

    for i in 0..MAX_FAILED_ATTEMPTS {
        let err = session
            .verify_and_consume(&fp(1), &fp(2), &nonce, &[i as u8; 32])
            .expect_err("guess must fail");
        assert_eq!(err, PairingError::BadProof);
    }

    // Even the *correct* proof is now refused: the window is burned.
    let proof = valid_proof(&session, &nonce);
    let err = session
        .verify_and_consume(&fp(1), &fp(2), &nonce, &proof)
        .expect_err("session must be locked out");
    assert_eq!(err, PairingError::RateLimited);
    assert!(session.is_exhausted());
}

#[test]
fn a_proof_for_a_different_device_is_rejected() {
    // Simulates a captured proof replayed by an attacker whose TLS identity
    // is different from the device the proof was computed for.
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let nonce = [5u8; NONCE_LEN];
    let proof_for_victim = pairing::compute_proof(session.token(), &fp(1), &fp(2), &nonce);

    // The responder recomputes using the *attacker's* TLS fingerprint.
    let err = session
        .verify_and_consume(&fp(1), &fp(0xCC), &nonce, &proof_for_victim)
        .expect_err("cross-device replay must fail");
    assert_eq!(err, PairingError::BadProof);
}

#[test]
fn a_proof_for_a_previous_nonce_is_rejected() {
    let mut session = PairingSession::new(Duration::from_secs(60)).expect("session");
    let old_nonce = [1u8; NONCE_LEN];
    let stale = pairing::compute_proof(session.token(), &fp(1), &fp(2), &old_nonce);

    // A new connection means a new nonce.
    let fresh_nonce = pairing::generate_nonce().expect("nonce");
    let err = session
        .verify_and_consume(&fp(1), &fp(2), &fresh_nonce, &stale)
        .expect_err("stale-nonce replay must fail");
    assert_eq!(err, PairingError::BadProof);
}

#[test]
fn nonces_are_unique() {
    let a = pairing::generate_nonce().expect("a");
    let b = pairing::generate_nonce().expect("b");
    assert_ne!(a, b);
    assert_eq!(a.len(), NONCE_LEN);
}
