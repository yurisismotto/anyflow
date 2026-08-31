//! Compiles the brand artwork into the binary.
//!
//! The SVGs are read straight from `docs/design/assets` rather than copied
//! here: one canonical set of artwork, and no chance of the copy drifting
//! from the original that the brand documentation points at.
fn main() {
    println!("cargo:rerun-if-changed=data/anyflow.gresource.xml");
    println!("cargo:rerun-if-changed=../../docs/design/assets");
    glib_build_tools::compile_resources(
        &["data", "../../docs/design/assets"],
        "data/anyflow.gresource.xml",
        "anyflow.gresource",
    );
}
