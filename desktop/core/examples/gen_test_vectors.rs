//! Regenerates the shared cross-language test fixtures in `protocol/testdata`.
//!
//! Run with:
//!
//! ```text
//! cargo run -p omnibridge-core --example gen_test_vectors
//! ```
//!
//! The fixtures are real certificates produced by the real desktop identity
//! code, so the Kotlin tests exercise the same X.509 the daemon would actually
//! present rather than something hand-rolled for the test. Both sides then
//! assert the same SPKI fingerprints, which makes the fingerprint construction
//! a checked cross-language contract instead of a convention.
//!
//! The committed fixtures are regenerated only on purpose: rerunning this
//! produces new random keys and therefore new fingerprints, so the expected
//! values in `desktop/core/tests/identity_and_store.rs` and in
//! `android/app/src/test/.../FingerprintTest.kt` must be updated together.

use std::path::PathBuf;

use omnibridge_core::identity::LocalIdentity;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/testdata");
    std::fs::create_dir_all(&out)?;

    // "a" plays the paired desktop; "b" plays a different machine presenting a
    // perfectly valid certificate that simply is not the pinned one.
    for name in ["a", "b"] {
        let identity = LocalIdentity::generate("fixture", omnibridge_proto::v1::Platform::Linux)?;
        let path = out.join(format!("identity-{name}.der"));
        std::fs::write(&path, identity.certificate_der().as_ref())?;
        println!(
            "{}\n  device_id   {}\n  fingerprint {}",
            path.display(),
            identity.device_id(),
            identity.fingerprint().to_hex()
        );
    }
    Ok(())
}
