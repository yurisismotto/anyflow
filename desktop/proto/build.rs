//! Compiles the shared `.proto` definitions into Rust.
//!
//! We use `protox` (a pure-Rust protobuf compiler) rather than shelling out to
//! a system `protoc`. Rationale: the daemon must be buildable from a clean
//! Fedora checkout with nothing but a Rust toolchain, and vendoring a
//! prebuilt `protoc` binary into the build would be a supply-chain wart we
//! do not want in a security-sensitive project. See ADR-0004.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/proto")
        .canonicalize()?;

    let files = [
        "omnibridge/v1/envelope.proto",
        "omnibridge/v1/core.proto",
        "omnibridge/v1/capabilities/battery_v1.proto",
        "omnibridge/v1/capabilities/files_v1.proto",
        "omnibridge/v1/capabilities/clipboard_v1.proto",
        "omnibridge/v1/capabilities/notifications_v1.proto",
    ];

    for f in &files {
        println!("cargo:rerun-if-changed={}", proto_root.join(f).display());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let descriptors = protox::compile(files, [&proto_root])?;

    prost_build::Config::new()
        .skip_protoc_run()
        .compile_fds(descriptors)?;

    Ok(())
}
