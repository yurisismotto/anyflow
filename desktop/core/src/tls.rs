//! TLS 1.3 transport with public-key pinning.
//!
//! # The shape of the problem
//!
//! There is no CA in this system and there never will be one — provisioning a
//! PKI for two devices on a home network is absurd. So the usual WebPKI chain
//! validation has nothing to validate against. What we do instead is pin the
//! peer's SPKI fingerprint, exactly as the pairing step established it.
//!
//! This means writing custom `rustls` verifiers, which is the single most
//! dangerous thing in this codebase. The trap everyone falls into is a
//! verifier that returns `Ok(())` from `verify_tls13_signature`. Doing that
//! disables the proof-of-possession step: any attacker could then replay a
//! *copy* of the legitimate certificate (certificates are public!) and be
//! accepted without ever holding the private key. Both verifiers below
//! therefore delegate to `rustls::crypto::verify_tls13_signature`, which is
//! the real check.
//!
//! What each verifier does:
//!
//! * [`PinnedServerCertVerifier`] — used when dialing a known peer. Requires
//!   the presented leaf's SPKI fingerprint to equal the pinned one. Any other
//!   identity, including a valid certificate for a different device, is
//!   rejected.
//!
//! * [`RecordingClientCertVerifier`] — used when listening. It cannot pin,
//!   because during pairing the client is by definition not yet known. It
//!   requires client authentication, checks the certificate is well-formed
//!   and that the client actually holds the private key, and then defers the
//!   *authorization* decision to the session layer, which requires either a
//!   trust-store hit or a valid pairing proof. Discovery and reachability
//!   grant nothing (principle: discovery is not trust).
//!
//! TLS 1.2 and below are not merely discouraged, they are not compiled in:
//! the configs are built with `TLS13` only and both `verify_tls12_signature`
//! implementations return an error unconditionally.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

use crate::error::{Error, Result};
use crate::fingerprint::Fingerprint;
use crate::identity::LocalIdentity;

/// Only TLS 1.3. Not a runtime toggle, and not overridable by a peer.
static TLS13_ONLY: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13];

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Signature schemes we accept for the handshake signature.
///
/// P-256/SHA-256 is what our own certificates use. The wider set is offered
/// because rustls requires us to declare what we can verify, and rejecting
/// schemes we could verify would only produce confusing failures for future
/// peers that legitimately use a stronger key.
fn supported_schemes() -> Vec<SignatureScheme> {
    vec![
        SignatureScheme::ECDSA_NISTP256_SHA256,
        SignatureScheme::ECDSA_NISTP384_SHA384,
        SignatureScheme::ED25519,
        SignatureScheme::RSA_PSS_SHA256,
        SignatureScheme::RSA_PSS_SHA384,
        SignatureScheme::RSA_PSS_SHA512,
    ]
}

/// Structural checks every peer certificate must pass, whether or not it is
/// pinned: it must parse as X.509, and it must be within its validity window.
///
/// Validity is a hygiene check, not the trust anchor — the pin is. It is
/// checked anyway so that a long-forgotten certificate surfaces as a clear
/// error instead of working forever. A generous skew allowance keeps a phone
/// with a slightly wrong clock from being locked out.
const CLOCK_SKEW_TOLERANCE_SECS: u64 = 86_400;

fn validate_certificate_structure(
    cert_der: &[u8],
    now: UnixTime,
) -> std::result::Result<(), rustls::Error> {
    let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;

    let now_secs = now.as_secs();
    let not_before = cert.validity().not_before.timestamp() as u64;
    let not_after = cert.validity().not_after.timestamp() as u64;

    if now_secs + CLOCK_SKEW_TOLERANCE_SECS < not_before {
        return Err(rustls::Error::InvalidCertificate(
            rustls::CertificateError::NotValidYet,
        ));
    }
    if now_secs > not_after.saturating_add(CLOCK_SKEW_TOLERANCE_SECS) {
        return Err(rustls::Error::InvalidCertificate(
            rustls::CertificateError::Expired,
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Client side: pin the server
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PinnedServerCertVerifier {
    expected: Fingerprint,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl PinnedServerCertVerifier {
    pub fn new(expected: Fingerprint) -> Self {
        Self {
            expected,
            provider: provider(),
        }
    }
}

impl ServerCertVerifier for PinnedServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        validate_certificate_structure(end_entity, now)?;

        // The identity check. Note that `_server_name` is ignored on purpose:
        // hostnames and IPs are not identity in this protocol (principle 6),
        // and our certificates carry no SANs precisely so that nobody can
        // accidentally reintroduce name-based trust.
        let presented = Fingerprint::from_certificate_der(end_entity).map_err(|_| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;

        if presented != self.expected {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ));
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // Unreachable with a TLS1.3-only config; an explicit refusal so that
        // it stays unreachable if someone widens the version list later.
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // Proof of possession. Without this, pinning would be worthless:
        // certificates are public, so anyone could present a copy.
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        supported_schemes()
    }
}

// ---------------------------------------------------------------------------
// Server side: authenticate possession, authorize later
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RecordingClientCertVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
    empty_hints: Vec<DistinguishedName>,
}

impl RecordingClientCertVerifier {
    pub fn new() -> Self {
        Self {
            provider: provider(),
            empty_hints: Vec::new(),
        }
    }
}

impl Default for RecordingClientCertVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientCertVerifier for RecordingClientCertVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        // No CA, so no hints to give.
        &self.empty_hints
    }

    fn client_auth_mandatory(&self) -> bool {
        // A connection without a client certificate has no identity at all
        // and can never be authorized. Reject it during the handshake.
        true
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        // We accept an unknown certificate here, and that is safe *only*
        // because of two things:
        //
        //   1. `verify_tls13_signature` below still proves the peer holds the
        //      matching private key, so the SPKI we extract after the
        //      handshake genuinely belongs to whoever is on the socket.
        //   2. The session layer treats such a peer as `Unauthenticated`. It
        //      may send HELLO and PAIR_REQUEST and nothing else, and only
        //      while a pairing window is open.
        //
        // If either of those is ever removed, this becomes an
        // accept-all-clients hole. See `session.rs`.
        validate_certificate_structure(end_entity, now)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        supported_schemes()
    }
}

// ---------------------------------------------------------------------------
// Config builders
// ---------------------------------------------------------------------------

/// Server config: TLS 1.3 only, mandatory client certificates.
pub fn server_config(identity: &LocalIdentity) -> Result<Arc<rustls::ServerConfig>> {
    let mut config = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(TLS13_ONLY)
        .map_err(Error::Tls)?
        .with_client_cert_verifier(Arc::new(RecordingClientCertVerifier::new()))
        .with_single_cert(
            vec![identity.certificate_der().clone()],
            identity.rustls_private_key(),
        )
        .map_err(Error::Tls)?;

    config.alpn_protocols = vec![crate::ALPN_PROTOCOL.to_vec()];
    // Session resumption is disabled: it would let a peer skip a full
    // handshake, and full handshakes are where our pinning check lives.
    // Handshakes happen rarely enough that the cost is irrelevant.
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

/// Client config that will accept exactly one server identity.
pub fn client_config(
    identity: &LocalIdentity,
    expected_server: Fingerprint,
) -> Result<Arc<rustls::ClientConfig>> {
    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(TLS13_ONLY)
        .map_err(Error::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedServerCertVerifier::new(expected_server)))
        .with_client_auth_cert(
            vec![identity.certificate_der().clone()],
            identity.rustls_private_key(),
        )
        .map_err(Error::Tls)?;

    config.alpn_protocols = vec![crate::ALPN_PROTOCOL.to_vec()];
    Ok(Arc::new(config))
}

/// Extracts the peer's pinned-identity fingerprint from a completed handshake.
///
/// Returns an error if the peer sent no certificate. That should be
/// impossible on the server side (client auth is mandatory) and on the client
/// side (a server always sends one), so it is treated as a hard failure
/// rather than an anonymous-peer fallback.
pub fn peer_fingerprint(conn: &rustls::CommonState) -> Result<Fingerprint> {
    let certs = conn
        .peer_certificates()
        .ok_or(Error::Certificate("peer presented no certificate"))?;
    let leaf = certs
        .first()
        .ok_or(Error::Certificate("empty peer certificate chain"))?;
    Fingerprint::from_certificate_der(leaf)
}
