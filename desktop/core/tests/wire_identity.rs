//! The wire identities, pinned as literals.
//!
//! # Why literals, and why here
//!
//! Every value below is a **cross-implementation contract**: the Android app
//! carries the same string, and a peer that disagrees about any one of them
//! does not fail gracefully. A wrong ALPN fails the TLS handshake, a wrong
//! service type makes the daemon invisible to discovery, a wrong QR prefix
//! rejects a valid pairing code, and a wrong domain separator produces a
//! proof that verifies nowhere — all of which look like "the network is
//! broken" rather than "somebody renamed a constant".
//!
//! Asserting `SERVICE_TYPE == SERVICE_TYPE` would prove nothing, so these
//! tests spell the expected bytes out. The mirror image of this file is
//! `android/app/src/test/.../WireIdentityTest.kt`, which spells out the same
//! strings from the Kotlin side. Changing an identity means changing both, in
//! the same commit — which is exactly the reviewable event the OmniBridge
//! rename (and the AnyFlow rename before it, ADR-0011) needed and did not
//! have.
//!
//! The values themselves are recorded in ADR-0018.

use omnibridge_core::qr::QR_SCHEME;
use omnibridge_core::{ALPN_DATA_PROTOCOL, ALPN_PROTOCOL, SERVICE_TYPE};

#[test]
fn control_alpn_is_omnibridge_1() {
    assert_eq!(ALPN_PROTOCOL, b"omnibridge/1");
}

#[test]
fn data_alpn_is_omnibridge_data_1() {
    assert_eq!(ALPN_DATA_PROTOCOL, b"omnibridge-data/1");
}

#[test]
fn the_two_alpn_identifiers_are_distinct() {
    // The listener decides which of the two connections it just accepted from
    // this value alone, before reading an application byte.
    assert_ne!(ALPN_PROTOCOL, ALPN_DATA_PROTOCOL);
}

#[test]
fn mdns_service_type_is_omnibridge_tcp() {
    assert_eq!(SERVICE_TYPE, "_omnibridge._tcp.local.");
}

#[test]
fn qr_scheme_is_omnibridge1() {
    assert_eq!(QR_SCHEME, "omnibridge1");
}

#[test]
fn no_pre_rename_identity_survives() {
    // The point of a clean pre-v1 rename is that a grep for the dead name is
    // unambiguously a bug (ADR-0011 "Consequences", carried into ADR-0018).
    for dead in ["anyflow", "fedroid"] {
        assert!(!String::from_utf8_lossy(ALPN_PROTOCOL).contains(dead));
        assert!(!String::from_utf8_lossy(ALPN_DATA_PROTOCOL).contains(dead));
        assert!(!SERVICE_TYPE.contains(dead));
        assert!(!QR_SCHEME.contains(dead));
    }
}
