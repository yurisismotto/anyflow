//! Local device identity.
//!
//! # Key algorithm
//!
//! The identity key is **ECDSA P-256**. Ed25519 would be the nicer choice on
//! its own merits, but it is not a workable *shared* choice here:
//!
//!   * Android Keystore offers hardware-backed (TEE / StrongBox) key storage
//!     for EC P-256 essentially everywhere; Ed25519 keystore support is not
//!     universally available and is not usable as a TLS client-certificate
//!     key through `javax.net.ssl`'s `KeyManager`.
//!   * The private key must never leave the Android TEE. That constraint
//!     outranks a marginal preference between two well-studied signature
//!     schemes.
//!
//! P-256 is supported by rustls (via *ring*) and by Android's Conscrypt, so
//! both ends can use the same primitive. See ADR-0006.
//!
//! # Certificate
//!
//! The identity key is wrapped in a self-signed X.509 certificate purely
//! because that is the credential shape TLS 1.3 accepts. There is no CA and
//! no chain: the certificate is a transport envelope for a raw public key.
//! Trust comes entirely from pinning the SPKI fingerprint (see
//! [`crate::fingerprint`]).

use rand::TryRngCore;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use crate::error::{Error, Result};
use crate::fingerprint::Fingerprint;

/// How long a freshly generated identity certificate is valid.
///
/// Long-lived on purpose: expiry is a hygiene check, not the trust anchor.
/// Because we pin the SPKI rather than the certificate, the certificate can
/// be regenerated from the same key when it nears expiry without breaking a
/// single existing pairing.
const CERT_VALIDITY_DAYS: i64 = 3650;

/// A device's own identity: a keypair, its certificate, and its metadata.
pub struct LocalIdentity {
    device_id: String,
    device_name: String,
    platform: anyflow_proto::v1::Platform,
    cert_der: CertificateDer<'static>,
    key_pkcs8_der: Vec<u8>,
    fingerprint: Fingerprint,
}

impl LocalIdentity {
    /// Generates a brand-new identity. Called once, on first run.
    pub fn generate(device_name: &str, platform: anyflow_proto::v1::Platform) -> Result<Self> {
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|_| Error::Certificate("failed to generate P-256 keypair"))?;
        let device_id = random_device_id()?;
        let cert_der = Self::issue_certificate(&key_pair, &device_id)?;
        Self::from_parts(
            device_id,
            device_name.to_string(),
            platform,
            cert_der,
            key_pair.serialize_der(),
        )
    }

    /// Rebuilds an identity from persisted material.
    pub fn from_parts(
        device_id: String,
        device_name: String,
        platform: anyflow_proto::v1::Platform,
        cert_der: Vec<u8>,
        key_pkcs8_der: Vec<u8>,
    ) -> Result<Self> {
        let fingerprint = Fingerprint::from_certificate_der(&cert_der)?;
        Ok(Self {
            device_id,
            device_name,
            platform,
            cert_der: CertificateDer::from(cert_der),
            key_pkcs8_der,
            fingerprint,
        })
    }

    /// Issues a self-signed certificate over an existing key.
    ///
    /// The subject carries only the device id. No DNS names, no IP SANs:
    /// hostname verification is meaningless for us and we explicitly do not
    /// want anyone (including a future maintainer) to be able to fall back
    /// to name-based validation. A `rustls` client would normally require a
    /// SAN, which is exactly why our verifier does key pinning instead.
    fn issue_certificate(key_pair: &rcgen::KeyPair, device_id: &str) -> Result<Vec<u8>> {
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new())
            .map_err(|_| Error::Certificate("bad certificate parameters"))?;

        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, format!("anyflow:{device_id}"));
        params.distinguished_name = dn;

        params.not_before = rcgen::date_time_ymd(2020, 1, 1);
        let not_after = std::time::SystemTime::now()
            + std::time::Duration::from_secs(CERT_VALIDITY_DAYS as u64 * 86_400);
        params.not_after = not_after.into();

        params.is_ca = rcgen::IsCa::NoCa;
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::DigitalSignature,
            rcgen::KeyUsagePurpose::KeyEncipherment,
        ];
        // Both roles: a device is a TLS server when it listens and a TLS
        // client when it dials. One identity, one certificate.
        params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let cert = params
            .self_signed(key_pair)
            .map_err(|_| Error::Certificate("failed to self-sign certificate"))?;
        Ok(cert.der().to_vec())
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn set_device_name(&mut self, name: String) {
        self.device_name = name;
    }

    pub fn platform(&self) -> anyflow_proto::v1::Platform {
        self.platform
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    pub fn certificate_der(&self) -> &CertificateDer<'static> {
        &self.cert_der
    }

    pub fn private_key_pkcs8_der(&self) -> &[u8] {
        &self.key_pkcs8_der
    }

    pub(crate) fn rustls_private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_pkcs8_der.clone()))
    }

    /// The public `DeviceInfo` this device puts on the wire.
    pub fn device_info(&self) -> anyflow_proto::v1::DeviceInfo {
        anyflow_proto::v1::DeviceInfo {
            device_id: self.device_id.clone(),
            device_name: self.device_name.clone(),
            platform: self.platform as i32,
            identity_fingerprint: self.fingerprint.to_hex(),
        }
    }
}

impl std::fmt::Debug for LocalIdentity {
    /// Hand-written so a stray `{:?}` can never spill the private key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalIdentity")
            .field("device_id", &self.device_id)
            .field("platform", &self.platform)
            .field("fingerprint", &self.fingerprint)
            .field("private_key", &"<redacted>")
            .finish()
    }
}

/// 128 bits from the OS CSPRNG, lowercase hex.
///
/// Random rather than derived from any hardware serial: a device id must not
/// be a stable cross-install tracking handle, and it must be re-derivable by
/// nobody but this device.
pub fn random_device_id() -> Result<String> {
    let mut buf = [0u8; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|_| Error::Store("system CSPRNG unavailable".into()))?;
    Ok(data_encoding::HEXLOWER.encode(&buf))
}
