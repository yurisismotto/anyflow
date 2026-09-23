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

use std::sync::Arc;

use rand::TryRngCore;
use rustls::sign::{CertifiedKey, SigningKey};
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
///
/// This is the **software** implementation of [`IdentityProvider`] — the one
/// OmniBridge has always shipped, and the only one Wave 0 ships. It holds the
/// PKCS#8 bytes because a software key genuinely is bytes. A TPM, Secure
/// Enclave or Keystore implementation of the same trait holds a handle
/// instead and can never produce those bytes; that is the entire point of the
/// trait existing. Nothing above [`IdentityProvider`] may ask for them.
pub struct LocalIdentity {
    device_id: String,
    device_name: String,
    platform: omnibridge_proto::v1::Platform,
    cert_der: CertificateDer<'static>,
    key_pkcs8_der: Vec<u8>,
    fingerprint: Fingerprint,
    /// Built once, at construction, so that `signing_key()` is infallible
    /// exactly as rustls's resolver path requires — and so that a key that
    /// cannot be parsed at all fails at *load* time, where the store can
    /// classify it as `IDENTITY_CORRUPTED`, rather than mid-handshake.
    signing_key: Arc<dyn SigningKey>,
}

impl LocalIdentity {
    /// Generates a brand-new identity. Called once, on first run.
    pub fn generate(device_name: &str, platform: omnibridge_proto::v1::Platform) -> Result<Self> {
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
        platform: omnibridge_proto::v1::Platform,
        cert_der: Vec<u8>,
        key_pkcs8_der: Vec<u8>,
    ) -> Result<Self> {
        let fingerprint = Fingerprint::from_certificate_der(&cert_der)?;
        let cert_der = CertificateDer::from(cert_der);

        // Parsing the key here, rather than at first handshake, is what lets
        // `Store` tell `IDENTITY_CORRUPTED` from `IDENTITY_AVAILABLE`. An
        // unparseable key must be a refusal to start, never a reason to
        // generate a replacement.
        let signing_key = rustls::crypto::ring::sign::any_ecdsa_type(&PrivateKeyDer::Pkcs8(
            PrivatePkcs8KeyDer::from(key_pkcs8_der.clone()),
        ))
        .map_err(|_| Error::Certificate("identity key is not a usable P-256 private key"))?;

        // Continuity: the private key must be the one this certificate
        // attests to. Without this, a key and a certificate from two
        // different identities would load happily and then fail every
        // handshake with a signature error that named neither.
        let certified = CertifiedKey::new(vec![cert_der.clone()], Arc::clone(&signing_key));
        match certified.keys_match() {
            Ok(()) => {}
            // `Unknown` means the backend cannot expose the public key for
            // comparison. Not an error: a hardware backing may legitimately
            // be unable to answer, and refusing would ban it.
            Err(rustls::Error::InconsistentKeys(rustls::InconsistentKeys::Unknown)) => {}
            Err(_) => {
                return Err(Error::Certificate(
                    "identity key does not match the stored certificate",
                ))
            }
        }

        Ok(Self {
            device_id,
            device_name,
            platform,
            cert_der,
            key_pkcs8_der,
            fingerprint,
            signing_key,
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
        dn.push(rcgen::DnType::CommonName, format!("omnibridge:{device_id}"));
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

    pub fn platform(&self) -> omnibridge_proto::v1::Platform {
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

    /// The rustls signing handle for this key.
    ///
    /// Cloned from the one built at construction: rustls asks for it once per
    /// handshake through a certificate resolver, and `Signer::sign` is
    /// synchronous, so nothing here may do I/O or prompt.
    pub fn signing_key(&self) -> Arc<dyn SigningKey> {
        Arc::clone(&self.signing_key)
    }

    /// The public `DeviceInfo` this device puts on the wire.
    pub fn device_info(&self) -> omnibridge_proto::v1::DeviceInfo {
        omnibridge_proto::v1::DeviceInfo {
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

// ---------------------------------------------------------------------------
// The platform seam
// ---------------------------------------------------------------------------

/// How the private key is protected on this device.
///
/// Reported locally — `omnibridge status` shows it — and **never** put on the
/// wire. A peer's claim about its own key storage is unverifiable, and an
/// unverifiable self-report is not a security property (PLAT-DEC-012).
///
/// It is recorded in `state.json` at creation time. Without a recorded
/// *expectation* there is no way to tell "this device never had hardware
/// backing" from "the hardware backing has gone away", and that distinction
/// is the whole of the fail-safe property in [`IdentityState`]
/// (PLAT-DEC-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyBacking {
    /// A PKCS#8 key in a file this process can read. Everything OmniBridge has
    /// ever shipped, and the only backing Wave 0 implements.
    Software,
    /// Windows: a CNG key in the platform TPM.
    Tpm,
    /// macOS / iOS: a Secure Enclave key.
    SecureEnclave,
    /// Android: a Keystore key, optionally in StrongBox.
    Keystore { strongbox: bool },
}

impl KeyBacking {
    /// Whether the private key could, even in principle, be read out of this
    /// backing. `false` for every hardware backing.
    pub fn is_exportable(&self) -> bool {
        matches!(self, Self::Software)
    }

    /// One word for `omnibridge status`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Software => "software",
            Self::Tpm => "tpm",
            Self::SecureEnclave => "secure-enclave",
            Self::Keystore { strongbox: true } => "keystore-strongbox",
            Self::Keystore { strongbox: false } => "keystore",
        }
    }
}

impl std::fmt::Display for KeyBacking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Default for a `state.json` written before `key_backing` existed.
///
/// Correct for every identity in the field today: they are all software keys,
/// because no other backing has ever been implemented.
impl Default for KeyBacking {
    fn default() -> Self {
        Self::Software
    }
}

/// What the store found when it looked for this device's identity.
///
/// The six states are the whole of the fail-safe rule, and the rule is this:
///
/// > A new identity may be generated **only** from [`IdentityState::NotCreated`].
/// > Every other outcome is fatal.
///
/// "An identity exists but cannot be read right now" must never become
/// "generate a new one". Doing so would silently destroy every pairing, every
/// grant and the fingerprint continuity that every peer has pinned — and it
/// would look, from the user's side, like a hiccup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityState {
    /// No state file **and** no key material. A genuine first run, and the
    /// only state from which a key may be created.
    NotCreated,
    /// Key present, protection verified, backing matches what was recorded.
    Available,
    /// The backing exists but is not usable at this moment — a device locked
    /// before first unlock, a TPM that is busy, a transient I/O failure.
    /// Retryable. **Never regenerate.**
    TemporarilyUnavailable(String),
    /// `state.json` records a hardware backing and that hardware is absent,
    /// cleared or invalidated. Fatal. **Never regenerate.**
    HardwareUnavailable(String),
    /// Key material is present but unparseable, does not match the stored
    /// certificate, or the state file itself does not parse. Fatal.
    Corrupted(String),
    /// `state.json` describes an identity whose key is gone or unreadable —
    /// or key material exists with no state to describe it. Fatal.
    ///
    /// This is the case the pre-Wave-0 code got wrong: `Path::exists()`
    /// answers `false` for a permission error, so an unreadable key routed
    /// straight into "first run" and overwrote the trust store.
    Lost(String),
}

impl IdentityState {
    /// The single question every caller actually asks.
    pub fn may_create(&self) -> bool {
        matches!(self, Self::NotCreated)
    }

    /// Whether a retry could plausibly succeed without human intervention.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::TemporarilyUnavailable(_))
    }

    /// Stable machine-readable name, as used in the Wave 0 specification.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotCreated => "IDENTITY_NOT_CREATED",
            Self::Available => "IDENTITY_AVAILABLE",
            Self::TemporarilyUnavailable(_) => "IDENTITY_TEMPORARILY_UNAVAILABLE",
            Self::HardwareUnavailable(_) => "IDENTITY_HARDWARE_UNAVAILABLE",
            Self::Corrupted(_) => "IDENTITY_CORRUPTED",
            Self::Lost(_) => "IDENTITY_LOST",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::NotCreated => "no identity has been created on this device yet",
            Self::Available => "the identity is available",
            Self::TemporarilyUnavailable(d)
            | Self::HardwareUnavailable(d)
            | Self::Corrupted(d)
            | Self::Lost(d) => d,
        }
    }
}

impl std::fmt::Display for IdentityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.detail())
    }
}

impl From<IdentityState> for Error {
    /// Every non-creatable state becomes a refusal to start, phrased so the
    /// operator can see both what happened and that nothing was destroyed.
    fn from(state: IdentityState) -> Self {
        Error::Store(format!(
            "{state}. The existing identity has NOT been replaced: OmniBridge \
             never generates a new identity over one it cannot read, because \
             that would silently break every pairing on every peer."
        ))
    }
}

/// Everything the protocol needs from a device identity.
///
/// Deliberately does **not** expose the private key. That is the whole point:
/// a TPM, Secure Enclave or Keystore key can sign and can never be exported,
/// so any abstraction that hands out PKCS#8 bytes excludes those platforms by
/// construction. Wave 0 ships one implementation ([`LocalIdentity`], a
/// software key) and proves the seam with a non-exportable test double.
pub trait IdentityProvider: Send + Sync + std::fmt::Debug {
    fn device_id(&self) -> &str;
    fn device_name(&self) -> &str;

    /// Which platform this adapter represents.
    ///
    /// On the provider rather than on the storage layer: before Wave 0 the
    /// value was hardcoded in `store.rs`, which meant the persistence layer
    /// decided what kind of machine this was (audit finding C2).
    fn platform(&self) -> omnibridge_proto::v1::Platform;

    fn certificate_der(&self) -> &CertificateDer<'static>;
    fn fingerprint(&self) -> Fingerprint;

    /// The rustls signing handle.
    ///
    /// Returns rustls's own type rather than a `sign(scheme, message)`
    /// method, because rustls calls `SigningKey::choose_scheme` per handshake
    /// and flattening that into one call would force this crate to
    /// reimplement scheme negotiation. Three contracts an implementation must
    /// honour: `Signer::sign` is synchronous (no `await`, no user-presence
    /// prompt); the message is **not** pre-hashed; and an ECDSA signature
    /// must be X9.62 DER, not IEEE-P1363.
    fn signing_key(&self) -> Arc<dyn SigningKey>;

    fn backing(&self) -> KeyBacking;

    /// The platform's private-storage guarantee for this key: 0600 on Unix,
    /// an owner-only DACL on Windows, trivially satisfied by hardware.
    ///
    /// A hard error, never a warning. A platform with no equivalent does not
    /// get a pass — it gets an implementation.
    fn verify_protection(&self) -> Result<()>;

    /// The public `DeviceInfo` this device puts on the wire.
    ///
    /// Note what is absent: [`KeyBacking`]. See PLAT-DEC-012.
    fn device_info(&self) -> omnibridge_proto::v1::DeviceInfo {
        omnibridge_proto::v1::DeviceInfo {
            device_id: self.device_id().to_string(),
            device_name: self.device_name().to_string(),
            platform: self.platform() as i32,
            identity_fingerprint: self.fingerprint().to_hex(),
        }
    }

    /// The certificate and its signing handle, packaged as rustls wants them.
    ///
    /// One place builds this, so a future adapter cannot accidentally hand
    /// rustls a chain and a key that do not correspond.
    fn certified_key(&self) -> Arc<CertifiedKey> {
        Arc::new(CertifiedKey::new(
            vec![self.certificate_der().clone()],
            self.signing_key(),
        ))
    }
}

/// So that an `Arc<LocalIdentity>` — which is what several call sites already
/// hold — is usable anywhere an `IdentityProvider` is wanted, without every
/// one of them being rewritten.
impl<T: IdentityProvider + ?Sized> IdentityProvider for Arc<T> {
    fn device_id(&self) -> &str {
        (**self).device_id()
    }
    fn device_name(&self) -> &str {
        (**self).device_name()
    }
    fn platform(&self) -> omnibridge_proto::v1::Platform {
        (**self).platform()
    }
    fn certificate_der(&self) -> &CertificateDer<'static> {
        (**self).certificate_der()
    }
    fn fingerprint(&self) -> Fingerprint {
        (**self).fingerprint()
    }
    fn signing_key(&self) -> Arc<dyn SigningKey> {
        (**self).signing_key()
    }
    fn backing(&self) -> KeyBacking {
        (**self).backing()
    }
    fn verify_protection(&self) -> Result<()> {
        (**self).verify_protection()
    }
}

impl IdentityProvider for LocalIdentity {
    fn device_id(&self) -> &str {
        self.device_id()
    }

    fn device_name(&self) -> &str {
        self.device_name()
    }

    fn platform(&self) -> omnibridge_proto::v1::Platform {
        self.platform()
    }

    fn certificate_der(&self) -> &CertificateDer<'static> {
        self.certificate_der()
    }

    fn fingerprint(&self) -> Fingerprint {
        self.fingerprint()
    }

    fn signing_key(&self) -> Arc<dyn SigningKey> {
        self.signing_key()
    }

    fn backing(&self) -> KeyBacking {
        KeyBacking::Software
    }

    /// A software key's protection is a property of the file it lives in, and
    /// the file is the storage layer's business. [`crate::secret_store`]
    /// enforces it on every read, before a single byte is handed over, which
    /// is strictly earlier than this method could.
    fn verify_protection(&self) -> Result<()> {
        Ok(())
    }
}

/// An owning handle to whatever identity this device has.
///
/// A concrete type rather than a bare `Arc<dyn IdentityProvider>` so that
/// callers reach `fingerprint()`, `device_id()` and the rest without having
/// to import the trait — which keeps every existing call site compiling
/// unchanged while the thing behind it becomes swappable.
#[derive(Clone, Debug)]
pub struct Identity(Arc<dyn IdentityProvider>);

impl Identity {
    pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
        Self(provider)
    }

    /// The provider underneath, for code that is generic over the seam.
    pub fn provider(&self) -> &Arc<dyn IdentityProvider> {
        &self.0
    }

    pub fn device_id(&self) -> &str {
        self.0.device_id()
    }
    pub fn device_name(&self) -> &str {
        self.0.device_name()
    }
    pub fn platform(&self) -> omnibridge_proto::v1::Platform {
        self.0.platform()
    }
    pub fn certificate_der(&self) -> &CertificateDer<'static> {
        self.0.certificate_der()
    }
    pub fn fingerprint(&self) -> Fingerprint {
        self.0.fingerprint()
    }
    pub fn signing_key(&self) -> Arc<dyn SigningKey> {
        self.0.signing_key()
    }
    pub fn backing(&self) -> KeyBacking {
        self.0.backing()
    }
    pub fn verify_protection(&self) -> Result<()> {
        self.0.verify_protection()
    }
    pub fn device_info(&self) -> omnibridge_proto::v1::DeviceInfo {
        self.0.device_info()
    }
    pub fn certified_key(&self) -> Arc<CertifiedKey> {
        self.0.certified_key()
    }
}

impl IdentityProvider for Identity {
    fn device_id(&self) -> &str {
        self.0.device_id()
    }
    fn device_name(&self) -> &str {
        self.0.device_name()
    }
    fn platform(&self) -> omnibridge_proto::v1::Platform {
        self.0.platform()
    }
    fn certificate_der(&self) -> &CertificateDer<'static> {
        self.0.certificate_der()
    }
    fn fingerprint(&self) -> Fingerprint {
        self.0.fingerprint()
    }
    fn signing_key(&self) -> Arc<dyn SigningKey> {
        self.0.signing_key()
    }
    fn backing(&self) -> KeyBacking {
        self.0.backing()
    }
    fn verify_protection(&self) -> Result<()> {
        self.0.verify_protection()
    }
}

/// Creates and reloads identities for one kind of key backing.
///
/// The counterpart to [`IdentityProvider`]: that trait describes an identity
/// that already exists, this one is how it comes to exist. Split because the
/// creation path is where a hardware backing differs most — an Enclave key is
/// generated *inside* the Enclave and only a handle ever leaves it.
///
/// The `secret` these methods pass around is deliberately opaque. For
/// [`SoftwareBacking`] it is the PKCS#8 key. For a hardware backing it is a
/// **handle**, and the key itself never exists outside the hardware. Nothing
/// above this trait may assume the bytes are a key.
pub trait IdentityBackend: Send + Sync + std::fmt::Debug {
    fn backing(&self) -> KeyBacking;

    /// Name this backend keeps its material under in the secret store.
    fn secret_name(&self) -> &str;

    /// Creates a brand-new identity.
    ///
    /// Returns the provider and the bytes to persist. **Only ever called from
    /// [`IdentityState::NotCreated`]** — that invariant is enforced by
    /// [`crate::store::Store`] and is the reason this trait has no
    /// "create if missing" convenience.
    fn create(
        &self,
        device_name: &str,
        platform: omnibridge_proto::v1::Platform,
    ) -> Result<(Arc<dyn IdentityProvider>, Vec<u8>)>;

    /// Rebuilds an identity from what was persisted.
    fn load(
        &self,
        device_id: String,
        device_name: String,
        platform: omnibridge_proto::v1::Platform,
        certificate_der: Vec<u8>,
        secret: Vec<u8>,
    ) -> Result<Arc<dyn IdentityProvider>>;
}

/// The software backing: a PKCS#8 P-256 key in a file.
///
/// Wave 0's only implementation, and the one every OmniBridge install in the
/// field is already using.
#[derive(Debug, Clone, Copy, Default)]
pub struct SoftwareBacking;

impl IdentityBackend for SoftwareBacking {
    fn backing(&self) -> KeyBacking {
        KeyBacking::Software
    }

    fn secret_name(&self) -> &str {
        crate::secret_store::IDENTITY_SECRET
    }

    fn create(
        &self,
        device_name: &str,
        platform: omnibridge_proto::v1::Platform,
    ) -> Result<(Arc<dyn IdentityProvider>, Vec<u8>)> {
        let identity = LocalIdentity::generate(device_name, platform)?;
        let secret = identity.private_key_pkcs8_der().to_vec();
        Ok((Arc::new(identity), secret))
    }

    fn load(
        &self,
        device_id: String,
        device_name: String,
        platform: omnibridge_proto::v1::Platform,
        certificate_der: Vec<u8>,
        secret: Vec<u8>,
    ) -> Result<Arc<dyn IdentityProvider>> {
        Ok(Arc::new(LocalIdentity::from_parts(
            device_id,
            device_name,
            platform,
            certificate_der,
            secret,
        )?))
    }
}
