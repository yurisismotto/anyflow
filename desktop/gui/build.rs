//! Compiles the brand artwork into the binary.
//!
//! The SVGs are read straight from `docs/design/assets` rather than copied
//! here: one canonical set of artwork, and no chance of the copy drifting
//! from the original that the brand documentation points at.
//!
//! The application icon needs one more thing than the others — it has to be
//! reachable by *name* from a `GtkIconTheme`, which means living at a path
//! that looks like an icon theme: `icons/<size>/<context>/<name>.svg`. That
//! is a second location for one file, so it is derived here rather than
//! committed twice and left to drift.

use std::path::Path;

/// Must match `APP_ID` in `src/lib.rs`: the icon is looked up by that name.
const APP_ID: &str = "io.github.yurisismotto.anyflow";

fn main() {
    println!("cargo:rerun-if-changed=data/anyflow.gresource.xml");
    println!("cargo:rerun-if-changed=../../docs/design/assets");

    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let icon_dir = Path::new(&out).join("icons/scalable/apps");
    std::fs::create_dir_all(&icon_dir).expect("the icon theme directory should be creatable");
    std::fs::copy(
        "../../docs/design/assets/app-icon.svg",
        icon_dir.join(format!("{APP_ID}.svg")),
    )
    .expect("the canonical app icon should be readable");

    glib_build_tools::compile_resources(
        &["data", "../../docs/design/assets", &out],
        "data/anyflow.gresource.xml",
        "anyflow.gresource",
    );
}
