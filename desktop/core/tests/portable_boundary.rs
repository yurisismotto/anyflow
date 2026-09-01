//! The boundary regression test.
//!
//! Wave 0's completion test is a negative one: *adding a platform means
//! writing an adapter crate; it never means editing `anyflow-core`,
//! `tls.rs`, `session.rs` or a capability crate's protocol half.* This file
//! is what makes that checkable rather than aspirational.
//!
//! # What it proves, and what it does not
//!
//! It proves that no portable crate reaches for a platform API outside the
//! one feature-gated module that is allowed to. It says **nothing** about
//! whether AnyFlow works on Windows or macOS — that needs a real machine and
//! belongs to Wave 5 and later. Conflating the two is how a project talks
//! itself into believing it supports a platform it has never run on.
//!
//! # Why a grep and not a cross-compile
//!
//! Research v1 proposed `cargo build --target x86_64-pc-windows-msvc` on a
//! Linux host as the gate. It cannot work: `ring` requires a C toolchain, and
//! for MSVC targets that means Build Tools for Visual Studio, whose libraries
//! are not redistributable onto a Linux runner. The real compile gate needs a
//! Windows runner and is a CI job (CI-001). This is the cheap backstop that
//! runs everywhere, on every change, today.

use std::path::{Path, PathBuf};

/// Crates that must contain no platform-specific code.
///
/// `anyflow-runtime` is deliberately absent: it depends on `mdns-sd`, whose
/// Windows behaviour is an open question (V-12 / POC-WIN-02) and outside
/// Wave 0. It contains no `std::os` today, and the last test below checks
/// that, but it is not in the portable *contract*.
const PORTABLE_CRATES: &[&str] = &[
    "proto",
    "core",
    "control",
    "capabilities/battery",
    "capabilities/clipboard",
    "capabilities/files",
];

/// The one exception, and why it is allowed.
///
/// Each entry is a path, relative to `desktop/`, that is permitted to name a
/// platform — because it *is* the platform module, it is behind a Cargo
/// feature, and turning the feature off removes it from the build entirely.
/// A new entry here is a decision, not a detail: it should be argued for in
/// review, not added to make a test pass.
const FEATURE_GATED_PLATFORM_MODULES: &[&str] = &[
    // `SecretStore` on a Unix filesystem, `Store::open(dir)`, XDG paths.
    // Behind `anyflow-core/unix-fs`.
    "core/src/platform/unix_fs.rs",
    // The Unix download destination. Behind
    // `anyflow-capability-files/unix-fs`.
    "capabilities/files/src/destination.rs",
    // The wl-clipboard and XFIXES backends. Behind
    // `anyflow-capability-clipboard/linux-backends`.
    "capabilities/clipboard/src/backend/wayland.rs",
    "capabilities/clipboard/src/backend/x11.rs",
];

/// Markers that mean "this file knows what operating system it is on".
const PLATFORM_MARKERS: &[&str] = &[
    "std::os::unix",
    "std::os::windows",
    "std::os::fd",
    "OpenOptionsExt",
    "PermissionsExt",
    "UnixListener",
    "UnixStream",
    "target_os",
    "target_family",
];

fn desktop_root() -> PathBuf {
    // `core/` → `desktop/`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core has a parent")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // `target/` is build output, not source.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `.rs` file in a crate's `src/`, with its path relative to `desktop/`.
fn crate_sources(crate_dir: &str) -> Vec<(String, String)> {
    let root = desktop_root();
    let mut files = Vec::new();
    rust_sources(&root.join(crate_dir).join("src"), &mut files);
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("inside desktop/")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path).expect("read source");
            (relative, text)
        })
        .collect()
}

#[test]
fn the_portable_crates_name_no_platform_outside_a_feature_gated_module() {
    let mut violations = Vec::new();

    for krate in PORTABLE_CRATES {
        for (path, text) in crate_sources(krate) {
            if FEATURE_GATED_PLATFORM_MODULES.contains(&path.as_str()) {
                continue;
            }
            for (number, line) in text.lines().enumerate() {
                // A mention inside prose is a mention of the boundary, not a
                // crossing of it — these files document why the seam exists.
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with("*") {
                    continue;
                }
                for marker in PLATFORM_MARKERS {
                    if line.contains(marker) {
                        violations.push(format!("{}:{}: {}", path, number + 1, code.trim()));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "a portable crate reached for a platform API. Either the code belongs \
         in an adapter crate, or the module belongs in \
         FEATURE_GATED_PLATFORM_MODULES with an argument for why:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn every_declared_exception_still_exists() {
    // A stale exception is worse than none: it silently widens the allowance
    // to a path nothing occupies, and hides the next file that moves there.
    let root = desktop_root();
    for path in FEATURE_GATED_PLATFORM_MODULES {
        assert!(
            root.join(path).exists(),
            "{path} is listed as a feature-gated platform module but does not \
             exist. Remove the exception."
        );
    }
}

#[test]
fn each_exception_is_actually_behind_a_feature() {
    // The exception is only defensible because the module disappears when the
    // feature is off. Check that the module is declared with a `cfg(feature)`
    // somewhere in its crate, rather than merely being conventionally named.
    for path in FEATURE_GATED_PLATFORM_MODULES {
        let module = Path::new(path)
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .to_string();
        let crate_dir = path.split("/src/").next().expect("crate dir");

        let declared_behind_feature = crate_sources(crate_dir).into_iter().any(|(_, text)| {
            text.lines().collect::<Vec<_>>().windows(4).any(|window| {
                window.iter().any(|l| l.contains("cfg(feature"))
                    && window
                        .iter()
                        .any(|l| l.trim().starts_with("pub mod ") && l.contains(&module))
            })
        });

        assert!(
            declared_behind_feature,
            "{path} is an allowed platform module but its `pub mod {module}` \
             is not behind a `#[cfg(feature = ...)]`. Without the gate the \
             exception is just an exemption."
        );
    }
}

#[test]
fn the_security_critical_files_carry_no_platform_arm_at_all() {
    // Not even a feature gate. These decide who is trusted, what a filename
    // becomes and how bytes are framed; a `#[cfg]` in any of them would put a
    // security control behind an arm that CI never compiles.
    //
    // `filename.rs` is named explicitly because PLAT-DEC-014 settled it:
    // sanitisation is protocol-global, and the rule that matters most —
    // stripping bidi overrides — belongs to no single platform, so a
    // per-destination design would have fixed it nowhere.
    let files = [
        "core/src/tls.rs",
        "core/src/session.rs",
        "core/src/pairing.rs",
        "core/src/fingerprint.rs",
        "core/src/framing.rs",
        "core/src/capability.rs",
        "capabilities/files/src/filename.rs",
        "capabilities/files/src/auth.rs",
    ];

    let root = desktop_root();
    for file in files {
        let text = std::fs::read_to_string(root.join(file)).expect("read");
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("*") {
                continue;
            }
            assert!(
                !code.contains("cfg(target_os")
                    && !code.contains("cfg(feature")
                    && !code.contains("cfg(unix")
                    && !code.contains("cfg(windows"),
                "{file}:{}: a conditional arm in a security-critical file: {code}",
                number + 1
            );
        }
    }
}

#[test]
fn the_portable_crates_declare_unsafe_code_forbidden() {
    // SI-11. `forbid` cannot be relaxed by an inner `#[allow]`, which is the
    // property that makes it worth spelling out per crate rather than
    // inheriting a workspace default that a new crate would silently pick up
    // — or silently miss.
    let root = desktop_root();
    for krate in PORTABLE_CRATES.iter().chain(["runtime"].iter()) {
        let manifest =
            std::fs::read_to_string(root.join(krate).join("Cargo.toml")).expect("read manifest");
        assert!(
            manifest.contains(r#"unsafe_code = "forbid""#),
            "{krate} must forbid unsafe code"
        );
    }
}

#[test]
fn the_adapter_and_binary_crates_at_least_deny_unsafe_code() {
    // `deny`, not `forbid`: a future adapter will need FFI, and `forbid`
    // cannot be relaxed locally. `deny` means any `unsafe` needs a
    // deliberate, reviewable `#[allow(unsafe_code)]` with a justification.
    let root = desktop_root();
    for krate in ["platform-linux", "daemon", "cli", "gui"] {
        let manifest =
            std::fs::read_to_string(root.join(krate).join("Cargo.toml")).expect("read manifest");
        assert!(
            manifest.contains(r#"unsafe_code = "deny""#),
            "{krate} must at least deny unsafe code"
        );
    }
}

#[test]
fn no_crate_actually_uses_unsafe_today() {
    // Wave 0 adds no `unsafe` anywhere. The policy exists so a future adapter
    // *can*; if this test starts failing, that is a real architectural event
    // and wants a conversation, not a silenced assertion.
    let mut found = Vec::new();
    for krate in PORTABLE_CRATES
        .iter()
        .chain(["runtime", "platform-linux", "daemon", "cli", "gui"].iter())
    {
        for (path, text) in crate_sources(krate) {
            for (number, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with("*") {
                    continue;
                }
                if code.contains("unsafe ") || code.contains("allow(unsafe_code)") {
                    found.push(format!("{}:{}: {}", path, number + 1, code.trim()));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "unsafe appeared in the workspace:\n  {}",
        found.join("\n  ")
    );
}
