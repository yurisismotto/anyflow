//! Security Certification v1 — the gates that needed new evidence.
//!
//! # What is here, and what is deliberately not
//!
//! Most of the certification's gates were already proved by suites written
//! alongside the features they guard, and duplicating them here would produce
//! a second, weaker copy that can drift. Those are cited by name in
//! `docs/certification/security/SECURITY-CERTIFICATION-V1.md` rather than
//! rewritten.
//!
//! Two gates had **no** evidence anywhere, and they are what this file is for:
//!
//! | Gate | Why nothing covered it |
//! | --- | --- |
//! | **SEC-TLS-01** | `core/src/tls.rs` builds both configs with `TLS13` only, and every test that speaks TLS asks for TLS 1.3 — so nothing ever asked what happens when a peer offers 1.2. A declaration is not a refusal. |
//! | **SEC-NET-02** | Nothing had ever looked at the bytes on the wire. `tcpdump` needs `CAP_NET_RAW`; a relay in the test process does not, and is deterministic besides. |
//!
//! # Neither test may pass vacuously
//!
//! Both carry a self-check, because both have an obvious silent-failure mode:
//! a TLS test that never reached the server, and a capture that recorded
//! nothing. Each asserts that it actually did its work before it asserts
//! anything about security. That discipline is not theoretical here — an
//! earlier packaging harness in this repository reported `23 passed` while
//! comparing an absent file to an absent file.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use omnibridge_core::Fingerprint;

const TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// SEC-TLS-01 — TLS 1.3 only
// ---------------------------------------------------------------------------

/// A hand-built TLS 1.2 `ClientHello`, sent over a raw socket, is refused.
///
/// # Why this is built by hand rather than with rustls
///
/// It cannot be built with rustls. MEASURED: `core/Cargo.toml` takes
/// `rustls = { default-features = false, features = ["std", "ring"] }`, so the
/// `tls12` feature is off and `rustls::version::TLS12` **does not exist in
/// this build** — an attempt to name it does not compile.
///
/// That is a stronger property than a refusal: a downgrade is not merely
/// rejected at runtime, the code to perform one was never compiled in. But it
/// is a property of *our* client, and SEC-TLS-01 is about what the **server**
/// does when a stranger offers 1.2. A stranger is not using our client.
///
/// So this writes the record itself: a `ClientHello` with
/// `client_version = 0x0303` and, deliberately, **no `supported_versions`
/// extension** — which is exactly how a real TLS 1.2-only peer announces
/// itself, and the only way to reach the server's version negotiation from
/// below.
#[tokio::test]
async fn sec_tls_01_a_raw_tls12_client_hello_is_refused() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let server = TestServer::start().await;
    let mut tcp = tokio::net::TcpStream::connect(server.addr)
        .await
        .expect("the listener is up, so the TCP connect must succeed");

    let hello = tls12_client_hello();
    // The hello must be a plausible record, or the server would be rejecting
    // it for being malformed rather than for being 1.2.
    assert_eq!(hello[0], 0x16, "record type must be handshake");
    assert_eq!(&hello[1..3], &[0x03, 0x01], "record layer version");
    assert_eq!(hello[5], 0x01, "handshake type must be ClientHello");
    assert_eq!(
        &hello[9..11],
        &[0x03, 0x03],
        "client_version must be TLS 1.2, or this tests nothing"
    );
    assert!(hello.len() > 60, "the hello is implausibly short");

    tcp.write_all(&hello).await.expect("writing the hello");

    let mut response = vec![0u8; 512];
    let n = match tokio::time::timeout(TIMEOUT, tcp.read(&mut response)).await {
        Err(_) => panic!("the server neither answered nor closed within {TIMEOUT:?}"),
        Ok(Ok(n)) => n,
        // A reset is a refusal.
        Ok(Err(_)) => 0,
    };

    if n == 0 {
        println!("SEC-TLS-01: the server closed the connection without replying");
        return;
    }
    println!(
        "SEC-TLS-01: the server replied with content type {:#04x}{}",
        response[0],
        if response[0] == 0x15 {
            format!(", alert level {} description {}", response[5], response[6])
        } else {
            String::new()
        }
    );

    // A reply is only acceptable if it is an alert. The failure this guards
    // against is a ServerHello: record type 0x16, handshake type 0x02.
    let content_type = response[0];
    assert_ne!(
        (content_type, response.get(5).copied()),
        (0x16, Some(0x02)),
        "the server answered a TLS 1.2 ClientHello with a ServerHello"
    );
    assert_eq!(
        content_type, 0x15,
        "expected a TLS alert (0x15), got content type {content_type:#04x}"
    );
}

/// A `ClientHello` offering TLS 1.2 and nothing newer.
///
/// Lengths are computed rather than written out, so the record cannot drift
/// into being rejected for the wrong reason.
fn tls12_client_hello() -> Vec<u8> {
    fn ext(id: u16, body: &[u8]) -> Vec<u8> {
        let mut v = id.to_be_bytes().to_vec();
        v.extend((body.len() as u16).to_be_bytes());
        v.extend_from_slice(body);
        v
    }

    let mut extensions = Vec::new();
    // signature_algorithms: ecdsa_secp256r1_sha256, rsa_pkcs1_sha256
    extensions.extend(ext(0x000d, &[0x00, 0x04, 0x04, 0x03, 0x04, 0x01]));
    // supported_groups: secp256r1
    extensions.extend(ext(0x000a, &[0x00, 0x02, 0x00, 0x17]));
    // ec_point_formats: uncompressed
    extensions.extend(ext(0x000b, &[0x01, 0x00]));
    // server_name: "omnibridge.invalid"
    let host = b"omnibridge.invalid";
    let mut sni = Vec::new();
    sni.extend(((host.len() + 3) as u16).to_be_bytes());
    sni.push(0x00);
    sni.extend((host.len() as u16).to_be_bytes());
    sni.extend_from_slice(host);
    extensions.extend(ext(0x0000, &sni));
    // NO supported_versions (0x002b). Its absence is what makes this a 1.2
    // client rather than a 1.3 client that also understands 1.2.

    let mut body = Vec::new();
    body.extend([0x03, 0x03]); // client_version = TLS 1.2
    body.extend([0x5a; 32]); // random — fixed, so the test is deterministic
    body.push(0x00); // session_id length
    body.extend([0x00, 0x04]); // cipher_suites length
    body.extend([0x00, 0x9c, 0x00, 0x2f]); // two TLS 1.2 suites
    body.extend([0x01, 0x00]); // compression: null
    body.extend((extensions.len() as u16).to_be_bytes());
    body.extend_from_slice(&extensions);

    let mut handshake = vec![0x01]; // ClientHello
    let len = body.len();
    handshake.extend([(len >> 16) as u8, (len >> 8) as u8, len as u8]);
    handshake.extend_from_slice(&body);

    let mut record = vec![0x16, 0x03, 0x01];
    record.extend((handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

/// A TLS 1.3 client *does* work against the same server.
///
/// The control for the test above. Without it, a server that refused every
/// connection for an unrelated reason would look like a pass.
#[tokio::test]
async fn sec_tls_01_control_a_tls13_client_is_accepted() {
    let server = TestServer::start().await;
    let client = TestClient::new("phone");
    let token = server.open_pairing(Duration::from_secs(30)).await;

    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("a TLS 1.3 peer must be able to pair, or the 1.2 refusal proves nothing");
    session.close().await;
}

/// The build cannot speak TLS 1.2 at all, and both configs pin 1.3.
///
/// Two separate claims, because they fail for different reasons: the first is
/// about the dependency graph, the second about our own configuration.
#[test]
fn sec_tls_01_tls12_is_not_compiled_in_and_both_configs_pin_tls13() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/Cargo.toml"),
    )
    .expect("reading core/Cargo.toml");
    for line in manifest
        .lines()
        .filter(|l| l.trim_start().starts_with("rustls ="))
    {
        assert!(
            line.contains("default-features = false"),
            "rustls is taken with default features, which enables tls12: {line}"
        );
        assert!(
            !line.contains("tls12"),
            "rustls is taken with the tls12 feature enabled: {line}"
        );
    }

    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src/tls.rs"),
    )
    .expect("reading core/src/tls.rs");
    assert!(
        src.contains("TLS13_ONLY"),
        "core/src/tls.rs no longer pins a single protocol version list"
    );
    // Both directions. A server pinned to 1.3 with a client that would accept
    // 1.2 is still a downgrade surface against other servers.
    let pins = src.matches("with_protocol_versions(TLS13_ONLY)").count();
    assert!(
        pins >= 2,
        "expected both the client and the server config to pin TLS13_ONLY, found {pins}"
    );
}

// ---------------------------------------------------------------------------
// SEC-NET-02 — nothing sensitive appears in plaintext on the wire
// ---------------------------------------------------------------------------

/// Everything that crossed the socket, in both directions.
///
/// A relay rather than a packet capture: `tcpdump` needs `CAP_NET_RAW`, and a
/// relay sees exactly the same bytes without it, deterministically, in-process
/// and on every developer's machine and CI runner.
#[derive(Default)]
struct Wire(Arc<std::sync::Mutex<Vec<u8>>>);

impl Wire {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().expect("not poisoned").clone()
    }
    fn record(&self, buf: &[u8]) {
        self.0.lock().expect("not poisoned").extend_from_slice(buf);
    }
}

/// Listens on an ephemeral port, forwards to `upstream`, records both
/// directions.
async fn relay(upstream: SocketAddr, wire: Arc<Wire>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("the relay must bind");
    let addr = listener.local_addr().expect("relay addr");
    tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            let wire = Arc::clone(&wire);
            tokio::spawn(async move {
                let Ok(mut server) = tokio::net::TcpStream::connect(upstream).await else {
                    return;
                };
                let (mut cr, mut cw) = client.split();
                let (mut sr, mut sw) = server.split();
                let up = {
                    let wire = Arc::clone(&wire);
                    async move {
                        let mut buf = vec![0u8; 16 * 1024];
                        loop {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            match cr.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    wire.record(&buf[..n]);
                                    if sw.write_all(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                };
                let down = async move {
                    let mut buf = vec![0u8; 16 * 1024];
                    loop {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        match sr.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                wire.record(&buf[..n]);
                                if cw.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                };
                tokio::join!(up, down);
            });
        }
    });
    addr
}

/// Pair and exchange over a recorded socket, then look for the sentinels.
///
/// The sentinels are long, unique and contain no substring that could occur by
/// chance in a TLS record, a protobuf tag or a length prefix — so a hit is a
/// leak and not a coincidence.
#[tokio::test]
async fn sec_net_02_no_sensitive_value_crosses_the_socket_in_plaintext() {
    const DEVICE_NAME: &str = "SENTINEL-DEVICE-NAME-a41f9c2e";

    let server = TestServer::start().await;
    let wire = Arc::new(Wire::default());
    let front = relay(server.addr, Arc::clone(&wire)).await;

    // Everything below dials the relay, never the daemon directly.
    let client = TestClient::new(DEVICE_NAME);
    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(front, server.fingerprint, Some(&token))
        .await
        .expect("pairing through the relay");
    session.close().await;

    let captured = wire.bytes();

    // ---- the capture must be real before it can prove anything -----------
    assert!(
        captured.len() > 512,
        "only {} bytes crossed the relay; the exchange did not happen and this \
         test would pass vacuously",
        captured.len()
    );
    // 0x16 is the TLS handshake content type. Its presence says these really
    // are TLS records and not, say, an empty buffer.
    assert!(
        captured.first() == Some(&0x16),
        "the first byte on the wire was {:#04x}, not a TLS handshake record; \
         the relay captured something other than the session",
        captured.first().copied().unwrap_or(0)
    );

    // ---- and now the property --------------------------------------------
    // The pairing token is the strongest sentinel available: it is a real
    // secret, it is exchanged during this very handshake, and it is what an
    // attacker on the LAN would most want.
    let token_text = token.to_base32();
    assert!(
        token_text.len() >= 8,
        "the pairing token is too short to be a meaningful sentinel"
    );

    for (what, needle) in [
        ("the pairing token", token_text.as_bytes()),
        ("the device name", DEVICE_NAME.as_bytes()),
    ] {
        assert!(
            !contains(&captured, needle),
            "{what} appeared in plaintext on the wire"
        );
    }
}

/// Substring search over the captured bytes.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The search itself must work, or the assertions above are decoration.
///
/// This is the control for `sec_net_02`: the same matcher, over a buffer that
/// is known to contain the needle.
#[test]
fn the_wire_search_finds_a_value_that_is_really_there() {
    let hay = b"\x16\x03\x01\x00\x45PLAINTEXT-SENTINEL-7c2a\xff\x00";
    assert!(contains(hay, b"PLAINTEXT-SENTINEL-7c2a"));
    assert!(!contains(hay, b"PLAINTEXT-SENTINEL-7c2b"));
    assert!(!contains(hay, b"not-present"));
}

// ---------------------------------------------------------------------------
// SEC-AUTH-01 — discovery is not trust, connection is not authorization
// ---------------------------------------------------------------------------

/// An unpaired peer cannot complete a session at all.
///
/// The capability-level half of SEC-AUTH-01 is covered by
/// `clip_sec_01_an_ungranted_peer_is_refused` and
/// `f1_an_offer_from_a_peer_without_a_grant_never_reaches_the_capability`.
/// This is the layer below those: a peer with no pairing at all, which is what
/// an attacker who has merely *discovered* the daemon over mDNS has.
#[tokio::test]
async fn sec_auth_01_an_unpaired_peer_cannot_establish_a_session() {
    let server = TestServer::start().await;
    let attacker = TestClient::new("attacker");

    // No token: this peer has never been paired and is not in the trust store.
    let result = attacker
        .connect(server.addr, server.fingerprint, None)
        .await;

    assert!(
        result.is_err(),
        "an unpaired peer established a session; discovery would then be trust"
    );
}

/// A peer that presents a *wrong* pinned server fingerprint is refused.
///
/// The client half of the pinning: OmniBridge must not talk to a server whose
/// key it has not pinned, which is what stops a LAN impostor answering on the
/// right port.
#[tokio::test]
async fn sec_auth_01_a_wrong_server_fingerprint_is_refused() {
    let server = TestServer::start().await;
    let other = TestServer::start().await;
    let client = TestClient::new("phone");
    let token = server.open_pairing(Duration::from_secs(30)).await;

    // Right address, wrong pinned identity.
    let wrong: Fingerprint = other.fingerprint;
    assert_ne!(
        wrong, server.fingerprint,
        "the two test servers must have different identities for this to mean anything"
    );

    let result = client.connect(server.addr, wrong, Some(&token)).await;
    assert!(
        result.is_err(),
        "a server presenting an unpinned identity was accepted"
    );
}
