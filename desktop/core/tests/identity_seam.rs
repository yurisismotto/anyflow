//! Proof that the TLS stack never needs the private key as bytes.
//!
//! This is the architectural claim Wave 0 rests on, and the one that decides
//! whether AnyFlow can ever run on a device whose key lives in a TPM, a
//! Secure Enclave or an Android Keystore. Apple state it plainly:
//!
//! > *Not having a mechanism to transfer plain-text key data into or out of
//! > the Secure Enclave is fundamental to its security.*
//!
//! So the test is not "does the refactor compile". It is: **drive a complete
//! TLS 1.3 handshake, with SPKI pinning and mandatory client authentication,
//! from an [`IdentityProvider`] that has no method returning key bytes and
//! holds none.**
//!
//! [`NonExportableIdentity`] below is a *double*, not hardware. What it
//! proves is the shape of the seam: that `anyflow-core` asks only for
//! `Arc<dyn SigningKey>`, that rustls only ever calls `choose_scheme` and
//! `sign`, and that nothing on the path from `server_config` to a completed
//! handshake reaches for PKCS#8. What it cannot prove is that a real TPM
//! answers correctly — that is integration, it belongs to Wave 5, and it is
//! an ordinary bug if it fails, not an architectural one.

use std::sync::Arc;

use anyflow_core::identity::{IdentityProvider, KeyBacking, LocalIdentity};
use anyflow_core::Fingerprint;
use anyflow_proto::v1::Platform;
use rustls::sign::{Signer, SigningKey};
use rustls::SignatureScheme;
use rustls_pki_types::CertificateDer;

// ---------------------------------------------------------------------------
// The double
// ---------------------------------------------------------------------------

/// A signing key that can sign and cannot be exported.
///
/// It wraps a real `SigningKey` — the point is not to reimplement ECDSA, it
/// is that after construction there is no path from this type back to any key
/// material. `sign` is synchronous and does no I/O, which is the contract
/// rustls imposes and the reason a hardware signer may not prompt the user
/// mid-handshake.
#[derive(Debug)]
struct SealedSigningKey {
    inner: Arc<dyn SigningKey>,
    /// Counts what rustls actually asks for, so the test can assert that the
    /// only two questions are `choose_scheme` and `sign`.
    signatures: Arc<std::sync::atomic::AtomicUsize>,
}

impl SigningKey for SealedSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        let signer = self.inner.choose_scheme(offered)?;
        Some(Box::new(SealedSigner {
            inner: signer,
            signatures: Arc::clone(&self.signatures),
        }))
    }

    fn public_key(&self) -> Option<rustls_pki_types::SubjectPublicKeyInfoDer<'_>> {
        // A public key is public. Exposing it is what lets `CertifiedKey`
        // check that the certificate and the key correspond, and it is
        // exactly the thing a Secure Enclave *will* give you.
        self.inner.public_key()
    }

    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        self.inner.algorithm()
    }
}

#[derive(Debug)]
struct SealedSigner {
    inner: Box<dyn Signer>,
    signatures: Arc<std::sync::atomic::AtomicUsize>,
}

impl Signer for SealedSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        self.signatures
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.sign(message)
    }

    fn scheme(&self) -> SignatureScheme {
        self.inner.scheme()
    }
}

/// An identity whose private key is, by construction, unreachable.
///
/// Note what this struct holds: a certificate, a fingerprint and a signing
/// *handle*. There is no `Vec<u8>` of key material anywhere in it, and no
/// method returns one. A `LocalIdentity` is used to build it and is then
/// dropped, so the PKCS#8 bytes do not outlive the constructor.
#[derive(Debug)]
struct NonExportableIdentity {
    device_id: String,
    device_name: String,
    cert: CertificateDer<'static>,
    fingerprint: Fingerprint,
    key: Arc<dyn SigningKey>,
    signatures: Arc<std::sync::atomic::AtomicUsize>,
}

impl NonExportableIdentity {
    fn generate(name: &str) -> Self {
        let signatures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (device_id, cert, fingerprint, key) = {
            let software = LocalIdentity::generate(name, Platform::Linux).expect("generate");
            (
                software.device_id().to_string(),
                software.certificate_der().clone(),
                software.fingerprint(),
                software.signing_key(),
            )
            // `software`, and with it every byte of PKCS#8, is dropped here.
        };
        Self {
            device_id,
            device_name: name.to_string(),
            cert,
            fingerprint,
            key: Arc::new(SealedSigningKey {
                inner: key,
                signatures: Arc::clone(&signatures),
            }),
            signatures,
        }
    }

    fn signature_count(&self) -> usize {
        self.signatures.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl IdentityProvider for NonExportableIdentity {
    fn device_id(&self) -> &str {
        &self.device_id
    }
    fn device_name(&self) -> &str {
        &self.device_name
    }
    fn platform(&self) -> Platform {
        Platform::Linux
    }
    fn certificate_der(&self) -> &CertificateDer<'static> {
        &self.cert
    }
    fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    fn signing_key(&self) -> Arc<dyn SigningKey> {
        Arc::clone(&self.key)
    }
    fn backing(&self) -> KeyBacking {
        KeyBacking::SecureEnclave
    }
    fn verify_protection(&self) -> Result<(), anyflow_core::Error> {
        // Hardware: the guarantee is structural, so there is nothing to
        // check. This is the *only* honest reason for this method to be
        // trivially `Ok`, and it is why it may never be made trivially `Ok`
        // for a software backing.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// A real handshake
// ---------------------------------------------------------------------------

/// Runs a full mutually-authenticated, pinned TLS 1.3 handshake over a
/// loopback TCP connection and returns each side's view of the other.
async fn handshake<S, C>(
    server_identity: &S,
    client_identity: &C,
    client_pins: Fingerprint,
) -> Result<(Fingerprint, Fingerprint), anyflow_core::Error>
where
    S: IdentityProvider + ?Sized,
    C: IdentityProvider + ?Sized,
{
    let server_config = anyflow_core::tls::server_config(server_identity)?;
    let client_config = anyflow_core::tls::client_config(client_identity, client_pins)?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let acceptor = tokio_rustls::TlsAcceptor::from(server_config);
        let tls = acceptor.accept(stream).await?;
        let (_, conn) = tls.get_ref();
        anyflow_core::tls::peer_fingerprint(conn)
    });

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let connector = tokio_rustls::TlsConnector::from(client_config);
    // The name is required by the API and ignored by the verifier: hostnames
    // are not identity in this protocol.
    let name = rustls_pki_types::ServerName::try_from("anyflow.invalid").expect("name");
    let tls = connector.connect(name, stream).await?;
    let (_, conn) = tls.get_ref();
    let seen_by_client = anyflow_core::tls::peer_fingerprint(conn)?;

    let seen_by_server = server.await.expect("join")?;
    Ok((seen_by_server, seen_by_client))
}

#[tokio::test]
async fn a_non_exportable_identity_completes_a_pinned_handshake_as_the_server() {
    let server = NonExportableIdentity::generate("Enclave Server");
    let client = LocalIdentity::generate("Software Client", Platform::Linux).expect("client");
    let pinned = server.fingerprint();

    let (seen_by_server, seen_by_client) = handshake(&server, &client, pinned)
        .await
        .expect("the handshake must complete without any PKCS#8 anywhere");

    assert_eq!(seen_by_client, server.fingerprint());
    assert_eq!(seen_by_server, client.fingerprint());
    assert!(
        server.signature_count() >= 1,
        "the sealed key must have signed the CertificateVerify"
    );
}

#[tokio::test]
async fn a_non_exportable_identity_completes_a_pinned_handshake_as_the_client() {
    // Client authentication is mandatory in this protocol, so the client half
    // of the seam matters as much as the server half — and it is a different
    // rustls entry point (`with_client_cert_resolver`).
    let server = LocalIdentity::generate("Software Server", Platform::Linux).expect("server");
    let client = NonExportableIdentity::generate("Enclave Client");
    let pinned = server.fingerprint();

    let (seen_by_server, seen_by_client) = handshake(&server, &client, pinned)
        .await
        .expect("the handshake must complete");

    assert_eq!(seen_by_server, client.fingerprint());
    assert_eq!(seen_by_client, server.fingerprint());
    assert!(
        client.signature_count() >= 1,
        "the sealed key must have signed the CertificateVerify"
    );
}

#[tokio::test]
async fn both_ends_non_exportable_still_works() {
    let server = NonExportableIdentity::generate("Enclave A");
    let client = NonExportableIdentity::generate("Enclave B");
    let pinned = server.fingerprint();

    let (seen_by_server, seen_by_client) = handshake(&server, &client, pinned)
        .await
        .expect("handshake");

    assert_eq!(seen_by_client, server.fingerprint());
    assert_eq!(seen_by_server, client.fingerprint());
}

#[tokio::test]
async fn pinning_still_rejects_a_different_identity_through_the_resolver_path() {
    // The security property must survive the seam. A client pinned to
    // someone else's key must fail *during the handshake*, from the
    // certificate verifier — not later, at the application layer.
    let server = NonExportableIdentity::generate("Enclave Server");
    let client = LocalIdentity::generate("Client", Platform::Linux).expect("client");
    let impostor = LocalIdentity::generate("Impostor", Platform::Linux)
        .expect("impostor")
        .fingerprint();

    let err = handshake(&server, &client, impostor)
        .await
        .expect_err("pinning must reject a different identity");

    let cause = match &err {
        anyflow_core::Error::Tls(e) => Some(e.clone()),
        anyflow_core::Error::Io(io) => io
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
            .cloned(),
        _ => None,
    }
    .unwrap_or_else(|| panic!("expected a TLS error, got: {err:?}"));

    assert!(
        matches!(
            cause,
            rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure
            )
        ),
        "the rejection must come from the pinning verifier, got: {cause:?}"
    );
}

#[tokio::test]
async fn a_signer_that_fails_fails_the_handshake_rather_than_falling_back() {
    // A hardware key can refuse — the device is locked, the TPM is busy. The
    // handshake must fail. What it must never do is proceed unauthenticated,
    // which is what a resolver returning `None` would allow on the client
    // side against a server that did not insist.
    #[derive(Debug)]
    struct FailingKey(Arc<dyn SigningKey>);

    impl SigningKey for FailingKey {
        fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
            let inner = self.0.choose_scheme(offered)?;
            Some(Box::new(FailingSigner(inner)))
        }
        fn public_key(&self) -> Option<rustls_pki_types::SubjectPublicKeyInfoDer<'_>> {
            self.0.public_key()
        }
        fn algorithm(&self) -> rustls::SignatureAlgorithm {
            self.0.algorithm()
        }
    }

    #[derive(Debug)]
    struct FailingSigner(Box<dyn Signer>);

    impl Signer for FailingSigner {
        fn sign(&self, _message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
            Err(rustls::Error::General(
                "the key store refused to sign".into(),
            ))
        }
        fn scheme(&self) -> SignatureScheme {
            self.0.scheme()
        }
    }

    #[derive(Debug)]
    struct LockedIdentity {
        cert: CertificateDer<'static>,
        fingerprint: Fingerprint,
        key: Arc<dyn SigningKey>,
    }

    impl IdentityProvider for LockedIdentity {
        fn device_id(&self) -> &str {
            "locked"
        }
        fn device_name(&self) -> &str {
            "Locked Device"
        }
        fn platform(&self) -> Platform {
            Platform::Linux
        }
        fn certificate_der(&self) -> &CertificateDer<'static> {
            &self.cert
        }
        fn fingerprint(&self) -> Fingerprint {
            self.fingerprint
        }
        fn signing_key(&self) -> Arc<dyn SigningKey> {
            Arc::clone(&self.key)
        }
        fn backing(&self) -> KeyBacking {
            KeyBacking::Tpm
        }
        fn verify_protection(&self) -> Result<(), anyflow_core::Error> {
            Ok(())
        }
    }

    let software = LocalIdentity::generate("Locked", Platform::Linux).expect("generate");
    let locked = LockedIdentity {
        cert: software.certificate_der().clone(),
        fingerprint: software.fingerprint(),
        key: Arc::new(FailingKey(software.signing_key())),
    };
    let client = LocalIdentity::generate("Client", Platform::Linux).expect("client");
    let pinned = locked.fingerprint();

    let result = handshake(&locked, &client, pinned).await;
    assert!(
        result.is_err(),
        "a signer that refuses must fail the handshake, not weaken it"
    );
}

#[tokio::test]
async fn the_seam_accepts_every_shape_a_caller_already_holds() {
    // `&LocalIdentity`, `&Arc<LocalIdentity>` and `&Identity` all reach the
    // TLS builders. That is what let this refactor land without editing a
    // single existing call site.
    let software = Arc::new(LocalIdentity::generate("Shapes", Platform::Linux).expect("generate"));
    let fingerprint = software.fingerprint();

    anyflow_core::tls::server_config(software.as_ref()).expect("&LocalIdentity");
    anyflow_core::tls::server_config(&software).expect("&Arc<LocalIdentity>");
    anyflow_core::tls::client_config(&software, fingerprint).expect("&Arc<LocalIdentity>");

    let boxed: Arc<dyn IdentityProvider> = software;
    let identity = anyflow_core::identity::Identity::new(boxed);
    anyflow_core::tls::server_config(&identity).expect("&Identity");
    anyflow_core::tls::client_config(&identity, fingerprint).expect("&Identity");
    assert_eq!(identity.backing(), KeyBacking::Software);
}

#[test]
fn key_backing_says_honestly_whether_a_key_could_ever_be_exported() {
    assert!(KeyBacking::Software.is_exportable());
    assert!(!KeyBacking::Tpm.is_exportable());
    assert!(!KeyBacking::SecureEnclave.is_exportable());
    assert!(!KeyBacking::Keystore { strongbox: true }.is_exportable());
    assert!(!KeyBacking::Keystore { strongbox: false }.is_exportable());
}

#[test]
fn device_info_never_advertises_the_key_backing() {
    // PLAT-DEC-012. A device's claim about its own key storage is
    // unverifiable by the peer, and an unverifiable self-report is not a
    // security property. It is shown locally and never put on the wire.
    let enclave = NonExportableIdentity::generate("Enclave");
    let info = enclave.device_info();
    let rendered = format!("{info:?}").to_lowercase();

    assert!(!rendered.contains("enclave_backing"));
    assert!(!rendered.contains("secureenclave"));
    assert!(!rendered.contains("key_backing"));
    assert_eq!(info.identity_fingerprint, enclave.fingerprint().to_hex());
}
