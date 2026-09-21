//! The protobuf namespace, asserted from descriptors.
//!
//! The package name is a cross-implementation identity: `prost` derives the
//! generated Rust module path from it (`OUT_DIR/omnibridge.v1.rs`, included as
//! `omnibridge_proto::v1`), and `java_package` decides where the Kotlin
//! classes land. A rename that updated one and missed the other would leave
//! two implementations that still compile and no longer agree about what they
//! are speaking.
//!
//! A grep over the `.proto` text would be defeated by a comment or a line
//! break, so this reads the compiled descriptors — the same ones the build
//! script feeds to `prost-build`. The Kotlin mirror is the
//! `generated protobuf types live in the omnibridge namespace` case in
//! `android/app/src/test/.../WireIdentityTest.kt`.
//!
//! Recorded in ADR-0018.

use std::path::PathBuf;

use prost_types::FileDescriptorSet;

const FILES: [&str; 6] = [
    "omnibridge/v1/envelope.proto",
    "omnibridge/v1/core.proto",
    "omnibridge/v1/capabilities/battery_v1.proto",
    "omnibridge/v1/capabilities/files_v1.proto",
    "omnibridge/v1/capabilities/clipboard_v1.proto",
    "omnibridge/v1/capabilities/notifications_v1.proto",
];

fn descriptors() -> FileDescriptorSet {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/proto")
        .canonicalize()
        .expect("the protocol directory should exist");
    protox::compile(FILES, [&root]).expect("the schema should compile")
}

#[test]
fn every_file_declares_an_omnibridge_package() {
    for file in descriptors().file {
        let name = file.name().to_owned();
        let package = file.package().to_owned();
        assert!(
            package == "omnibridge.v1" || package == "omnibridge.v1.capabilities",
            "{name} declares package {package:?}"
        );
    }
}

#[test]
fn capability_schemas_are_in_the_capabilities_package() {
    for file in descriptors().file {
        let expected = if file.name().contains("/capabilities/") {
            "omnibridge.v1.capabilities"
        } else {
            "omnibridge.v1"
        };
        assert_eq!(file.package(), expected, "{}", file.name());
    }
}

#[test]
fn the_java_package_tracks_the_proto_package() {
    // Kotlin resolves `io.github.yurisismotto.omnibridge.proto.Envelope` from
    // this option, not from the package above. They are two independent
    // strings that have to move together.
    for file in descriptors().file {
        let java = file
            .options
            .as_ref()
            .and_then(|o| o.java_package.clone())
            .unwrap_or_else(|| panic!("{} declares no java_package", file.name()));
        let expected = file
            .package()
            .replace("omnibridge.v1", "io.github.yurisismotto.omnibridge.proto");
        assert_eq!(java, expected, "{}", file.name());
    }
}

#[test]
fn no_pre_rename_namespace_survives() {
    for file in descriptors().file {
        for dead in ["anyflow", "fedroid"] {
            assert!(!file.package().contains(dead), "{}", file.name());
            assert!(!file.name().contains(dead));
        }
    }
}
