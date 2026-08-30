//! Identity fingerprints.
//!
//! A fingerprint is `SHA-256(DER SubjectPublicKeyInfo)` of a device's
//! long-lived identity key.
//!
//! We fingerprint the SPKI and **not** the certificate. The certificate is a
//! disposable container: it carries an expiry, and a device must be able to
//! re-issue one without invalidating every existing pairing. The public key
//! is the identity. This is the same reasoning behind SPKI pinning in HPKP
//! and in Android's `network-security-config`.

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// A 32-byte SHA-256 digest over a DER SubjectPublicKeyInfo.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    /// Computes the fingerprint of an already-extracted DER SPKI.
    pub fn from_spki_der(spki: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(spki);
        Self(h.finalize().into())
    }

    /// Extracts the SPKI from a DER X.509 certificate and fingerprints it.
    pub fn from_certificate_der(cert_der: &[u8]) -> Result<Self> {
        let (_, cert) = x509_parser::parse_x509_certificate(cert_der)
            .map_err(|_| Error::Certificate("malformed X.509 certificate"))?;
        Ok(Self::from_spki_der(cert.tbs_certificate.subject_pki.raw))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex. This is the canonical serialized form, used in the
    /// protocol, in the trust store, and in the QR payload.
    pub fn to_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let raw = data_encoding::HEXLOWER
            .decode(s.as_bytes())
            .map_err(|_| Error::Certificate("fingerprint is not lowercase hex"))?;
        let arr: [u8; 32] = raw
            .try_into()
            .map_err(|_| Error::Certificate("fingerprint is not 32 bytes"))?;
        Ok(Self(arr))
    }

    /// Human-comparable rendering, e.g. `A1B2 C3D4 E5F6 0718`.
    ///
    /// Only the first 8 bytes are shown. This form exists so a person can
    /// eyeball two screens; it is **never** parsed back or used for a trust
    /// decision, because 64 bits is not enough to resist a determined
    /// collision search.
    pub fn to_display_short(&self) -> String {
        let hex = data_encoding::HEXUPPER.encode(&self.0[..8]);
        hex.as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or("????"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl std::fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fingerprint({})", self.to_display_short())
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl serde::Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}
