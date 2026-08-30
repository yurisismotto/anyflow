//! Identity, fingerprinting, QR payload and trust-store persistence.

use std::os::unix::fs::PermissionsExt;

use fedroid_core::identity::LocalIdentity;
use fedroid_core::pairing::PairingToken;
use fedroid_core::qr::QrPayload;
use fedroid_core::store::{Store, TrustedPeer};
use fedroid_core::Fingerprint;
use fedroid_proto::v1::Platform;

fn identity() -> LocalIdentity {
    LocalIdentity::generate("Test Device", Platform::Linux).expect("generate identity")
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn generated_identities_are_distinct() {
    let a = identity();
    let b = identity();
    assert_ne!(a.device_id(), b.device_id());
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn device_id_is_128_bits_of_hex() {
    let id = identity();
    assert_eq!(id.device_id().len(), 32);
    assert!(id.device_id().bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn fingerprint_is_stable_and_derived_from_the_certificate() {
    let id = identity();
    let from_cert = Fingerprint::from_certificate_der(id.certificate_der()).expect("fingerprint");
    assert_eq!(from_cert, id.fingerprint());
    assert_eq!(id.fingerprint(), id.fingerprint());
}

#[test]
fn fingerprint_hex_round_trips() {
    let id = identity();
    let hex = id.fingerprint().to_hex();
    assert_eq!(hex.len(), 64);
    assert_eq!(
        Fingerprint::from_hex(&hex).expect("parse"),
        id.fingerprint()
    );
}

#[test]
fn malformed_fingerprints_are_rejected() {
    assert!(Fingerprint::from_hex("").is_err());
    assert!(Fingerprint::from_hex("zz").is_err());
    // Right length, wrong alphabet.
    assert!(Fingerprint::from_hex(&"g".repeat(64)).is_err());
    // Right alphabet, wrong length.
    assert!(Fingerprint::from_hex(&"ab".repeat(31)).is_err());
    // Uppercase is not the canonical form and must not be silently accepted,
    // or two spellings of one identity could disagree as map keys.
    assert!(Fingerprint::from_hex(&"AB".repeat(32)).is_err());
}

#[test]
fn identity_debug_never_leaks_the_private_key() {
    let id = identity();
    let rendered = format!("{id:?}");
    assert!(rendered.contains("<redacted>"));
    let key_hex = data_encoding::HEXLOWER.encode(id.private_key_pkcs8_der());
    assert!(!rendered.contains(&key_hex));
}

#[test]
fn device_info_carries_the_fingerprint_not_an_address() {
    let id = identity();
    let info = id.device_info();
    assert_eq!(info.identity_fingerprint, id.fingerprint().to_hex());
    assert_eq!(info.device_id, id.device_id());
    // Nothing in DeviceInfo should look like a network address.
    let serialized = format!("{info:?}");
    assert!(!serialized.contains("192.168"));
}

#[test]
fn certificate_carries_no_subject_alt_names() {
    // Our verifier ignores hostnames on purpose. Shipping a SAN would invite
    // a future change that starts trusting it.
    let id = identity();
    let (_, cert) = x509_parser::parse_x509_certificate(id.certificate_der()).expect("parse cert");
    assert!(
        cert.tbs_certificate
            .subject_alternative_name()
            .ok()
            .flatten()
            .is_none(),
        "identity certificates must not carry SANs"
    );
}

// ---------------------------------------------------------------------------
// QR payload
// ---------------------------------------------------------------------------

#[test]
fn qr_payload_round_trips() {
    let id = identity();
    let token = PairingToken::generate().expect("token");
    let addrs = vec!["192.168.1.10:55432".parse().expect("addr")];

    let encoded = QrPayload::encode(&id.fingerprint(), &token, id.device_id(), &addrs);
    let parsed = QrPayload::parse(&encoded).expect("parse");

    assert_eq!(parsed.fingerprint, id.fingerprint());
    assert_eq!(parsed.device_id, id.device_id());
    assert_eq!(parsed.addresses, addrs);
    assert_eq!(parsed.token().expect("token").as_bytes(), token.as_bytes());
}

#[test]
fn qr_payload_handles_ipv6_addresses() {
    let id = identity();
    let token = PairingToken::generate().expect("token");
    let addrs: Vec<std::net::SocketAddr> = vec![
        "[fe80::1]:55432".parse().expect("v6"),
        "10.0.0.5:55432".parse().expect("v4"),
    ];
    let encoded = QrPayload::encode(&id.fingerprint(), &token, id.device_id(), &addrs);
    let parsed = QrPayload::parse(&encoded).expect("parse");
    assert_eq!(parsed.addresses, addrs);
}

#[test]
fn qr_payload_rejects_hostile_input() {
    assert!(QrPayload::parse("").is_err());
    assert!(QrPayload::parse("http://evil.example/").is_err());
    // Right scheme, truncated.
    assert!(QrPayload::parse("fedroidb1:").is_err());
    // Wrong scheme version.
    let id = identity();
    let token = PairingToken::generate().expect("token");
    let good = QrPayload::encode(&id.fingerprint(), &token, id.device_id(), &[]);
    assert!(QrPayload::parse(&good.replace("fedroidb1", "fedroidb9")).is_err());
    // Oversized payload must be refused before parsing.
    assert!(QrPayload::parse(&"a".repeat(100_000)).is_err());
}

#[test]
fn qr_payload_rejects_a_tampered_fingerprint() {
    let id = identity();
    let token = PairingToken::generate().expect("token");
    let encoded = QrPayload::encode(&id.fingerprint(), &token, id.device_id(), &[]);
    let tampered = encoded.replace(&id.fingerprint().to_hex(), &"ab".repeat(20));
    assert!(QrPayload::parse(&tampered).is_err());
}

#[test]
fn qr_payload_drops_unparseable_addresses_but_keeps_the_rest() {
    // Addresses are hints; one bad entry must not make a valid code unusable.
    let id = identity();
    let token = PairingToken::generate().expect("token");
    let encoded = format!(
        "fedroidb1:{}:{}:{}:not-an-address,10.0.0.7:55432",
        id.fingerprint().to_hex(),
        token.to_base32(),
        id.device_id()
    );
    let parsed = QrPayload::parse(&encoded).expect("parse");
    assert_eq!(parsed.addresses.len(), 1);
    assert_eq!(parsed.addresses[0].to_string(), "10.0.0.7:55432");
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

fn peer(fingerprint: Fingerprint, name: &str) -> TrustedPeer {
    TrustedPeer {
        device_id: "0123456789abcdef0123456789abcdef".into(),
        device_name: name.into(),
        platform: Platform::Android as i32,
        fingerprint,
        paired_at_unix: 1_700_000_000,
        granted_capabilities: [("battery.v1".to_string(), true)].into_iter().collect(),
        last_protocol_version: 1,
        revoked: false,
    }
}

#[test]
fn store_generates_an_identity_on_first_run_and_reloads_it() {
    let dir = tempfile::tempdir().expect("tempdir");

    let first_fingerprint = {
        let store = Store::open(dir.path()).expect("open");
        store.identity().fingerprint()
    };

    let store = Store::open(dir.path()).expect("reopen");
    assert_eq!(
        store.identity().fingerprint(),
        first_fingerprint,
        "identity must survive a restart, or every pairing would break"
    );
}

#[test]
fn private_key_is_written_with_restrictive_permissions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _store = Store::open(dir.path()).expect("open");

    let key_mode = std::fs::metadata(dir.path().join("identity.key"))
        .expect("stat key")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(key_mode, 0o600, "private key must be owner-only");

    let dir_mode = std::fs::metadata(dir.path())
        .expect("stat dir")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        dir_mode & 0o077,
        0,
        "data directory must not be group/world accessible"
    );
}

#[test]
fn store_refuses_to_load_a_world_readable_private_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let _store = Store::open(dir.path()).expect("open");
    }

    let key_path = dir.path().join("identity.key");
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644))
        .expect("loosen permissions");

    let err = Store::open(dir.path())
        .err()
        .expect("must refuse a readable key");
    assert!(
        format!("{err}").contains("must not be group- or world-accessible"),
        "unexpected error: {err}"
    );
}

#[test]
fn peers_persist_across_restarts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fingerprint = identity().fingerprint();

    {
        let mut store = Store::open(dir.path()).expect("open");
        store
            .add_peer(peer(fingerprint, "Galaxy S25"))
            .expect("add");
    }

    let store = Store::open(dir.path()).expect("reopen");
    let loaded = store.trusted_peer(&fingerprint).expect("peer must persist");
    assert_eq!(loaded.device_name, "Galaxy S25");
    assert!(loaded.allows("battery.v1"));
}

#[test]
fn revocation_survives_a_restart_and_hides_the_peer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fingerprint = identity().fingerprint();

    {
        let mut store = Store::open(dir.path()).expect("open");
        store
            .add_peer(peer(fingerprint, "Galaxy S25"))
            .expect("add");
        assert!(store.revoke_peer(&fingerprint).expect("revoke"));
    }

    let store = Store::open(dir.path()).expect("reopen");
    assert!(
        store.trusted_peer(&fingerprint).is_none(),
        "a revoked peer must not be returned as trusted"
    );

    let record = store.peer_record(&fingerprint).expect("record is kept");
    assert!(record.revoked);
    assert!(
        record.granted_capabilities.is_empty(),
        "revocation must drop capability grants, not just set a flag"
    );
    assert!(!record.allows("battery.v1"));
}

#[test]
fn revoking_an_unknown_peer_is_not_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = Store::open(dir.path()).expect("open");
    assert!(!store
        .revoke_peer(&identity().fingerprint())
        .expect("revoke"));
}

#[test]
fn capability_grants_are_independent_of_what_a_peer_advertises() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fingerprint = identity().fingerprint();
    let mut store = Store::open(dir.path()).expect("open");

    let mut p = peer(fingerprint, "Galaxy S25");
    p.granted_capabilities.clear();
    store.add_peer(p).expect("add");

    let loaded = store.trusted_peer(&fingerprint).expect("peer");
    assert!(
        !loaded.allows("battery.v1"),
        "nothing is granted by default"
    );

    store
        .set_capability_grant(&fingerprint, "battery.v1", true)
        .expect("grant");
    assert!(store
        .trusted_peer(&fingerprint)
        .expect("peer")
        .allows("battery.v1"));
}

#[test]
fn a_newer_schema_version_is_refused_rather_than_misread() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let _store = Store::open(dir.path()).expect("open");
    }

    let path = dir.path().join("state.json");
    let raw = std::fs::read_to_string(&path).expect("read");
    let bumped = raw.replace("\"schema_version\": 1", "\"schema_version\": 99");
    assert_ne!(raw, bumped, "schema_version must be present in state.json");
    std::fs::write(&path, bumped).expect("write");

    let err = Store::open(dir.path())
        .err()
        .expect("must refuse a future schema");
    assert!(format!("{err}").contains("newer than supported"), "{err}");
}

#[test]
fn store_never_persists_message_or_clipboard_content() {
    // A structural guard: the on-disk state should contain only identity,
    // settings and peers. If a future change starts writing payloads here,
    // this test is meant to be the thing that notices.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut store = Store::open(dir.path()).expect("open");
    store
        .add_peer(peer(identity().fingerprint(), "Galaxy S25"))
        .expect("add");

    let raw = std::fs::read_to_string(dir.path().join("state.json")).expect("read");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("json");
    // serde_json's map is ordered, so compare against a sorted expectation.
    let keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec![
            "certificate_der_b64",
            "device_id",
            "peers",
            "schema_version",
            "settings"
        ],
        "state.json gained a top-level field; confirm it holds no user content"
    );
}
