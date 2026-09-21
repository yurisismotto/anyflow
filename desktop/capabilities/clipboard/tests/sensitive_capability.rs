//! The distro capability gap, driven end to end against a fake `wl-clipboard`.
//!
//! # Why a fake binary rather than a mock
//!
//! The thing under test is a **process boundary**. `probe_sensitive()` asks
//! the real `wl-copy` what its options are, and `write_text` pipes clipboard
//! content into the real `wl-copy`'s stdin. A mock of the backend would move
//! that boundary out of the test and leave the one question §16 exists to
//! answer — *did any content byte reach the child process?* — unanswerable.
//!
//! So each test writes a small shell script called `wl-copy` (and one called
//! `wl-paste`) into a temporary directory, puts that directory on `PATH`, and
//! lets `WaylandBackend::detect()` find it exactly as it would find the real
//! one. Every invocation appends its `argv` — and, for a real copy, its
//! stdin between two markers — to a log file. The log is the evidence.
//!
//! Two of these fakes differ from each other only in whether `--help` lists
//! `--sensitive`, and their `--version` strings are deliberately the wrong way
//! round: the "old" one supports the flag and the "new" one does not. That is
//! not a contrivance. Fedora ships `2.2.1^git20251124`, which *has* the flag,
//! while Ubuntu 24.04, Ubuntu 26.04 and Debian 13 all ship a plain `2.2.1`
//! that does not — same version string, different capability (U0 §8). Any
//! implementation that reads the version number fails these two tests.
//!
//! # PATH is process-global
//!
//! These tests mutate `PATH`, so they take a mutex for the duration rather
//! than relying on `--test-threads=1` being remembered. Nothing here touches
//! the real system clipboard; `tests/real_backend.rs` is the suite that does.

// This whole target drives the `wl-clipboard` helpers through a real process
// boundary: it writes `#!/bin/sh` fakes, chmods them with
// `std::os::unix::fs::PermissionsExt` and asks `WaylandBackend::detect()` to
// find them. All three exist only on Unix with `linux-backends` on, so the
// target is classified the same way `notifications/tests/real_dbus.rs` is:
// whole-file `cfg`, compiling to an empty test binary rather than a compile
// error when the feature is off.
//
// Without this the portable Windows gate cannot build the clipboard crate's
// test targets at all — `cargo test --no-run --no-default-features` fails on
// the `backend::wayland` import here. Measured on the U2 baseline, where that
// step was already red (U2 TC1 §15).
#![cfg(all(unix, feature = "linux-backends"))]

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use omnibridge_capability_clipboard::backend::wayland::WaylandBackend;
use omnibridge_capability_clipboard::backend::{BackendError, ClipboardBackend};
use omnibridge_capability_clipboard::text::ClipboardText;

/// The content no unsafe path may ever pass to a child process.
const CANARY: &str = "canary-correct-horse-battery-staple-9f3a1c";

fn path_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    // A poisoned lock means another test panicked; the useful failure is that
    // panic, not a second one from here.
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// A disposable directory holding the fake tools and their log.
struct Fakes {
    dir: PathBuf,
    log: PathBuf,
    original_path: Option<std::ffi::OsString>,
    _guard: MutexGuard<'static, ()>,
}

/// What the fake `wl-copy` should claim about itself.
#[derive(Clone, Copy)]
struct WlCopySpec {
    /// Whether `--help` lists `--sensitive`.
    help_lists_sensitive: bool,
    /// What `--version` prints. Deliberately allowed to disagree with the
    /// line above, because on real distributions it does.
    version: &'static str,
}

impl Fakes {
    /// Installs the requested fakes on `PATH` and returns the handle.
    ///
    /// `wl_copy: None` / `wl_paste: false` leave that binary absent, which is
    /// how the "not installed" cases are driven.
    fn install(wl_copy: Option<WlCopySpec>, wl_paste: bool) -> Self {
        let guard = path_lock();

        let dir = std::env::temp_dir().join(format!(
            "omnibridge-fake-wl-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let log = dir.join("invocations.log");
        std::fs::write(&log, b"").expect("log");

        if let Some(spec) = wl_copy {
            let sensitive_line = if spec.help_lists_sensitive {
                "\\t--sensitive\\tMark the clipboard content as sensitive.\\n"
            } else {
                ""
            };
            // Mirrors the real tool closely enough for the paths under test:
            // `--help` prints the option list and exits without reading
            // stdin; `--version` prints a version; anything else is a copy,
            // which reads stdin to EOF. The log records argv always, and
            // stdin only on a real copy, between markers.
            write_script(
                &dir.join("wl-copy"),
                &format!(
                    r#"log='{log}'
printf 'ARGV wl-copy %s\n' "$*" >> "$log"
case "$1" in
  --help)
    printf 'Usage: wl-copy [options] text to copy\n'
    printf '\t-o, --paste-once\tOnly serve one paste request.\n'
    printf '\t-p, --primary\tUse the primary selection.\n'
    printf '\t-t, --type mime/type\tSet the MIME type.\n'
    printf '{sensitive_line}'
    exit 0
    ;;
  --version)
    printf '{version}\n'
    exit 0
    ;;
esac
printf 'STDIN-BEGIN' >> "$log"
/bin/cat >> "$log"
printf 'STDIN-END\n' >> "$log"
exit 0
"#,
                    log = log.display(),
                    sensitive_line = sensitive_line,
                    version = spec.version,
                ),
            );
        }

        if wl_paste {
            // `--watch` exits at once, which is what wl-paste does on a
            // compositor without data-control — i.e. what Mutter produces on
            // every GNOME target. The watch source then falls through, which
            // is the realistic case and keeps the probe short.
            write_script(
                &dir.join("wl-paste"),
                &format!(
                    r#"log='{log}'
printf 'ARGV wl-paste %s\n' "$*" >> "$log"
case "$1" in
  --watch) exit 1 ;;
esac
exit 1
"#,
                    log = log.display(),
                ),
            );
        }

        let original_path = std::env::var_os("PATH");
        // `PATH` is *replaced*, not prepended. The development host has a
        // real `wl-clipboard` installed, and prepending would let the
        // "missing tool" cases find it and quietly pass for the wrong reason —
        // which is exactly what happened the first time this was written.
        // Nothing is lost by replacing it: the fakes are found here, `#!/bin/sh`
        // is resolved by the kernel from an absolute path, and the one external
        // command they run is spelled `/bin/cat`.
        std::env::set_var("PATH", std::env::join_paths([dir.clone()]).expect("PATH"));

        Self {
            dir,
            log,
            original_path,
            _guard: guard,
        }
    }

    fn backend(&self) -> WaylandBackend {
        WaylandBackend::detect()
    }

    /// Everything the fakes recorded, as one string.
    fn invocations(&self) -> String {
        std::fs::read_to_string(&self.log).expect("log is readable")
    }

    /// Every byte any fake received on stdin, concatenated.
    ///
    /// This is the measurement §16 turns on: not "was an error returned", but
    /// "did the child process receive anything at all".
    fn stdin_bytes(&self) -> String {
        let log = self.invocations();
        let mut out = String::new();
        let mut rest = log.as_str();
        while let Some(start) = rest.find("STDIN-BEGIN") {
            rest = &rest[start + "STDIN-BEGIN".len()..];
            let end = rest.find("STDIN-END").unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        }
        out
    }
}

impl Drop for Fakes {
    fn drop(&mut self) {
        match &self.original_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write_script(at: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(at, format!("#!/bin/sh\n{body}")).expect("script");
    std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn text(s: &str) -> ClipboardText {
    ClipboardText::validate(s).expect("valid clipboard text")
}

// ---------------------------------------------------------------------------
// A — wl-copy present, `--sensitive` supported
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_supported_wl_copy_offers_both_ordinary_and_sensitive_clipboard() {
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: true,
            version: "wl-clipboard 2.3.0",
        }),
        true,
    );
    let backend = fakes.backend();

    assert_eq!(backend.availability(), Ok(()), "ordinary clipboard");
    assert_eq!(backend.sensitive_support(), Ok(()), "sensitive clipboard");
    assert!(backend.describe().contains("sensitive marking: yes"));

    backend
        .write_text(&text(CANARY), true)
        .await
        .expect("a marked write must go through");

    // The flag was actually passed, and the content did reach the tool —
    // which is the whole point of the supported case.
    let log = fakes.invocations();
    assert!(log.contains("--sensitive"), "{log}");
    assert!(fakes.stdin_bytes().contains(CANARY));
}

// ---------------------------------------------------------------------------
// B — wl-copy present, `--sensitive` unsupported
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unsupported_wl_copy_keeps_the_ordinary_clipboard_and_refuses_sensitive_clips() {
    // Ubuntu 24.04, Ubuntu 26.04 and Debian 13, as measured in U0 §8.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: false,
            version: "wl-clipboard 2.2.1",
        }),
        true,
    );
    let backend = fakes.backend();

    // The two answers differ, which is the state the whole capability model
    // exists to be able to express.
    assert_eq!(
        backend.availability(),
        Ok(()),
        "ordinary clipboard must stay available"
    );
    assert!(
        backend.sensitive_support().is_err(),
        "sensitive marking must not be claimed"
    );
    assert!(backend.describe().contains("sensitive marking: no"));

    // An ordinary clip is completely unaffected.
    backend
        .write_text(&text("an ordinary clip"), false)
        .await
        .expect("ordinary writes must keep working");
    assert!(fakes.stdin_bytes().contains("an ordinary clip"));

    // A sensitive clip is refused.
    let err = backend
        .write_text(&text(CANARY), true)
        .await
        .expect_err("a sensitive clip must be refused");
    assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");
    assert!(!err.to_string().contains(CANARY), "no content in an error");
}

// ---------------------------------------------------------------------------
// §16 — the security regression
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_refused_sensitive_clip_reaches_no_process_at_all() {
    // The assertion is about bytes, not about an error string. A future
    // refactor that moved the capability check after the spawn would still
    // return `Unavailable` and would still fail here, which is why this test
    // measures the child process rather than the return value.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: false,
            version: "wl-clipboard 2.2.1",
        }),
        true,
    );
    let backend = fakes.backend();

    let err = backend
        .write_text(&text(CANARY), true)
        .await
        .expect_err("a sensitive clip must be refused on this system");
    assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");

    let log = fakes.invocations();
    assert!(
        !log.contains(CANARY),
        "the sensitive content reached a child process:\n{log}"
    );
    assert_eq!(
        fakes.stdin_bytes(),
        "",
        "a refused sensitive clip must pass ZERO content bytes to wl-copy"
    );
    // And no unmarked copy was attempted either: the only `wl-copy` the
    // backend ran is the capability probe.
    let copies: Vec<&str> = log
        .lines()
        .filter(|l| l.starts_with("ARGV wl-copy"))
        .filter(|l| !l.contains("--help"))
        .collect();
    assert!(
        copies.is_empty(),
        "wl-copy was invoked for a refused clip: {copies:?}"
    );
}

// ---------------------------------------------------------------------------
// C — wl-copy missing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_missing_wl_copy_makes_the_whole_clipboard_backend_unavailable() {
    let fakes = Fakes::install(None, true);
    let backend = fakes.backend();

    let why = backend
        .availability()
        .expect_err("no wl-copy means no clipboard");
    assert!(why.contains("wl-copy"), "{why}");
    assert!(why.contains("wl-clipboard"), "the package is named: {why}");
    assert!(backend.sensitive_support().is_err());
    assert!(backend.watch_availability().is_err());
    assert!(matches!(
        backend.read_text().await,
        Err(BackendError::Unavailable(_))
    ));
    assert!(backend.describe().contains("unavailable"));
}

// ---------------------------------------------------------------------------
// D — wl-paste missing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_missing_wl_paste_is_reported_truthfully_rather_than_as_a_working_clipboard() {
    // `wl-paste` is how this backend *reads* the clipboard and how it watches
    // for changes. Without it neither is possible, so the honest answer is
    // that the backend is unavailable — not that reading works and only the
    // watch is gone.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: true,
            version: "wl-clipboard 2.3.0",
        }),
        false,
    );
    let backend = fakes.backend();

    let why = backend
        .availability()
        .expect_err("no wl-paste means nothing can be read");
    assert!(why.contains("wl-paste"), "the missing tool is named: {why}");

    let watch = backend
        .watch_availability()
        .expect_err("no wl-paste means no watch");
    assert!(watch.contains("wl-paste"), "{watch}");

    assert!(matches!(
        backend.read_text().await,
        Err(BackendError::Unavailable(_))
    ));
    // And it does not claim sensitive marking either, despite the `wl-copy`
    // on this machine having the flag: there is no usable backend to mark
    // anything with.
    assert!(backend.sensitive_support().is_err());
}

// ---------------------------------------------------------------------------
// E / F — the version string is never the source of truth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_old_looking_version_that_has_the_flag_is_treated_as_supported() {
    // Fedora 44's `2.2.1^git20251124`: the version reads older than 2.3.0 and
    // the flag is there. A `>= 2.3` test would wrongly refuse this machine's
    // sensitive clips.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: true,
            version: "wl-clipboard 2.2.1",
        }),
        true,
    );
    let backend = fakes.backend();

    assert_eq!(
        backend.sensitive_support(),
        Ok(()),
        "capability, not version, decides"
    );
    backend
        .write_text(&text(CANARY), true)
        .await
        .expect("a marked write must go through on this machine");

    assert!(
        !fakes.invocations().contains("--version"),
        "the backend must never consult --version:\n{}",
        fakes.invocations()
    );
}

#[tokio::test]
async fn a_new_looking_version_without_the_flag_is_treated_as_unsupported() {
    // The mirror image, and the dangerous direction: believing a version
    // number here would hand a password to a `wl-copy` that cannot mark it.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: false,
            version: "wl-clipboard 9.9.9",
        }),
        true,
    );
    let backend = fakes.backend();

    assert!(
        backend.sensitive_support().is_err(),
        "a version number must not buy a capability"
    );
    backend
        .write_text(&text(CANARY), true)
        .await
        .expect_err("the clip must be refused despite the version");

    assert_eq!(fakes.stdin_bytes(), "", "no content may reach this wl-copy");
    assert!(
        !fakes.invocations().contains("--version"),
        "the backend must never consult --version"
    );
}

// ---------------------------------------------------------------------------
// U2 TC1 — the contract `tests/real_backend.rs` asserts against a live
// compositor, asserted here against both fakes on every machine.
// ---------------------------------------------------------------------------

/// Drives one backend through the whole sensitive contract and returns which
/// branch it took.
///
/// # Why this exists rather than a third pair of hand-written tests
///
/// `real_backend.rs` is `#[ignore]`d: it needs a Wayland session and is asked
/// for by name during a certification run, so no CI job ever executes it. The
/// contract it gates is therefore ungated between certifications — which is
/// how U2 TC1 could sit in the tree at all.
///
/// This function is the same sentence, run twice by ordinary `cargo test`,
/// once against a `wl-copy` that has the flag and once against one that does
/// not. `false` and `true` are both passes; neither is a skip. The Linux
/// distro matrix runs this package, so every supported distribution asserts
/// it on every pull request.
async fn assert_sensitive_contract(backend: &WaylandBackend, fakes: &Fakes) -> bool {
    assert_eq!(
        backend.availability(),
        Ok(()),
        "this helper judges the sensitive path, and needs a usable clipboard first"
    );

    // An ordinary clip first: the marker the refusal case measures against,
    // and proof that ordinary mirroring works before anything is marked.
    backend
        .write_text(&text("an ordinary clip"), false)
        .await
        .expect("ordinary writes must work regardless of sensitive support");

    let support = backend.sensitive_support();
    let outcome = backend.write_text(&text(CANARY), true).await;

    match (support, outcome) {
        (Ok(()), Ok(())) => {
            assert!(
                fakes.invocations().contains("--sensitive"),
                "a backend that claims marking must actually pass the flag:\n{}",
                fakes.invocations()
            );
            assert!(
                fakes.stdin_bytes().contains(CANARY),
                "a marked write must deliver its content"
            );
            assert!(backend.describe().contains("sensitive marking: yes"));
            true
        }
        (Err(why), Err(e)) => {
            assert!(
                matches!(e, BackendError::Unavailable(_)),
                "a refused sensitive clip must be the typed capability error, not {e:?}"
            );
            assert!(!why.is_empty(), "an unsupported backend must say why");
            assert!(
                !e.to_string().contains(CANARY),
                "the refusal must not carry the content it refused"
            );
            assert!(backend.describe().contains("sensitive marking: no"));

            // Fail-closed, measured at the process boundary: not one content
            // byte, and no unmarked fallback copy.
            assert!(
                !fakes.stdin_bytes().contains(CANARY),
                "sensitive content reached wl-copy on a system that cannot mark it"
            );
            assert!(
                !fakes.invocations().contains("--sensitive"),
                "the flag was passed to a wl-copy that does not understand it"
            );
            // The one copy that did run is the ordinary clip above; the
            // canary added none.
            let copies = fakes
                .invocations()
                .lines()
                .filter(|l| l.starts_with("ARGV wl-copy"))
                .filter(|l| !l.contains("--help"))
                .count();
            assert_eq!(copies, 1, "a refused clip must spawn no wl-copy of its own");
            false
        }
        (Ok(()), Err(e)) => {
            panic!("sensitive_support() promised marking and the marked write failed: {e:?}")
        }
        (Err(why), Ok(())) => panic!(
            "sensitive_support() said marking is impossible ({why}) and the write \
             succeeded anyway — the clip went out unmarked"
        ),
    }
}

#[tokio::test]
async fn the_sensitive_contract_holds_where_the_flag_exists() {
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: true,
            version: "wl-clipboard 2.3.0",
        }),
        true,
    );
    let backend = fakes.backend();

    assert!(
        assert_sensitive_contract(&backend, &fakes).await,
        "this fake advertises `--sensitive`, so the supported branch is the \
         one that must be taken"
    );
}

#[tokio::test]
async fn the_sensitive_contract_holds_where_the_flag_is_missing() {
    // Ubuntu 24.04 / 26.04 and Debian 13 (U0 §8) — the host shape that made
    // the old `real_backend.rs` assertion fail for correct behaviour.
    let fakes = Fakes::install(
        Some(WlCopySpec {
            help_lists_sensitive: false,
            version: "wl-clipboard 2.2.1",
        }),
        true,
    );
    let backend = fakes.backend();

    assert!(
        !assert_sensitive_contract(&backend, &fakes).await,
        "this fake has no `--sensitive`, so the refusal branch is the one that \
         must be taken — an unsupported host is an asserted state, never a skip"
    );
}
