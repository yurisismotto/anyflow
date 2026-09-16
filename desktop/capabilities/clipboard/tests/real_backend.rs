//! The FEDORA-WAYLAND gate: the real clipboard, on the real compositor.
//!
//! Every other suite runs against `MemoryBackend`, which is right — the
//! decision logic must be testable without a desktop session, and a test that
//! overwrote the developer's clipboard on every `cargo test` would be
//! intolerable.
//!
//! This one is the opposite: it touches the actual system clipboard, so it is
//! `#[ignore]`d and must be asked for by name:
//!
//! ```console
//! cargo test -p anyflow-capability-clipboard --test real_backend -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` is required, not advisory: there is exactly one system
//! clipboard, and two tests racing on it would each see the other's writes.
//!
//! It restores whatever was on the clipboard when it started, so running it
//! does not silently eat the clipboard content someone was about to paste.
//!
//! # Three ways this can fail that are not bugs
//!
//! * **`wl-clipboard` is not installed** — reported as `Unavailable`, and the
//!   test says so rather than failing obscurely.
//! * **`wl-copy` cannot mark a clip sensitive** — wl-clipboard below 2.3.0,
//!   which is what Ubuntu 24.04, Ubuntu 26.04 and Debian 13 ship. Also not a
//!   failure: the sensitive test below asserts the *refusal* contract on such
//!   a host instead of a round trip that cannot happen there (U2 TC1).
//!   Ordinary clipboard mirroring is unaffected.
//! * **The session is locked** — on GNOME, `wl-copy` and `wl-paste` block
//!   indefinitely behind a lock screen waiting for a seat, so the backend's
//!   timeout fires and the result is `TimedOut`. The tests below name that
//!   case explicitly, because a certification run that quietly failed for this
//!   reason would be worse than one that stopped and said "unlock the screen".
//!
//! The same root cause has a second, subtler symptom, measured here rather
//! than assumed: a clipboard with **no owner at all** also makes `wl-paste`
//! block, for the same reason — no data-control protocol means it needs a
//! seat, and nothing owns the selection to give it one. It is bounded by
//! `BACKEND_TIMEOUT` and treated as "nothing to send", never as a fault.

use std::time::Duration;

use anyflow_capability_clipboard::backend::{self, BackendError, ClipboardBackend};
use anyflow_capability_clipboard::limits::BACKEND_TIMEOUT;
use anyflow_capability_clipboard::text::ClipboardText;

/// Fails with an actionable message rather than a bare assertion.
fn explain(e: &BackendError) -> String {
    match e {
        BackendError::TimedOut => format!(
            "{e}\n\nIf the screen is locked, unlock it and run this again. This \
             is the documented GNOME behaviour, not a defect."
        ),
        other => other.to_string(),
    }
}

fn backend() -> std::sync::Arc<dyn ClipboardBackend> {
    let backend = backend::detect();
    eprintln!("backend: {}", backend.describe());
    backend
}

async fn read(b: &std::sync::Arc<dyn ClipboardBackend>) -> Option<ClipboardText> {
    match b.read_text().await {
        Ok(text) => text,
        Err(e) => panic!("read failed: {}", explain(&e)),
    }
}

async fn write(b: &std::sync::Arc<dyn ClipboardBackend>, text: &ClipboardText, sensitive: bool) {
    if let Err(e) = b.write_text(text, sensitive).await {
        panic!("write failed: {}", explain(&e));
    }
}

/// Saves and restores the clipboard around a test body.
async fn preserving_clipboard<F, Fut>(body: F)
where
    F: FnOnce(std::sync::Arc<dyn ClipboardBackend>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let b = backend();
    let saved = b.read_text().await.ok().flatten();
    body(std::sync::Arc::clone(&b)).await;
    if let Some(saved) = saved {
        let _ = b.write_text(&saved, false).await;
    }
}

// ---------------------------------------------------------------------------
// Bounded helper processes
// ---------------------------------------------------------------------------

/// Why a bounded helper process did not produce an exit status.
///
/// Deterministic on purpose: a caller distinguishes "the seat is locked" from
/// "`wl-copy` is not installed" without parsing a string.
#[derive(Debug, PartialEq, Eq)]
enum BoundedError {
    /// The child outlived its bound and was killed and reaped.
    TimedOut,
    /// The program could not be started at all — typically not installed.
    Spawn(String),
    /// The child was started but could not be waited on.
    Wait(String),
}

impl std::fmt::Display for BoundedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => write!(f, "timed out"),
            Self::Spawn(e) => write!(f, "could not be started: {e}"),
            Self::Wait(e) => write!(f, "did not exit: {e}"),
        }
    }
}

/// Runs a helper process under the same bound the production backend uses.
///
/// # Why this exists (issue #13)
///
/// This suite used to clear the clipboard with a blocking
/// `std::process::Command::status()`. On a locked GNOME seat `wl-copy` waits
/// forever for a seat and a serial the compositor will not grant — the exact
/// condition [`BACKEND_TIMEOUT`] exists to bound — so the *test* had a path
/// the *production* backend does not: an unbounded hang, with no output, that
/// a certification run could not tell from a slow machine.
///
/// The mechanism here is deliberately the one
/// `WaylandBackend::write_text` already uses: `tokio` process spawn, stdio to
/// `/dev/null`, `kill_on_drop`, wrapped in `tokio::time::timeout`. It is not
/// a second timeout policy — the real call site passes [`BACKEND_TIMEOUT`]
/// itself, and `limit` is a parameter only so the tests below can prove the
/// bound in milliseconds rather than making every run wait five seconds.
///
/// # Cleanup
///
/// On expiry the child is killed *and awaited* rather than left to
/// `kill_on_drop`'s asynchronous reaping, so by the time this returns the pid
/// is gone rather than a zombie. `wl-copy --clear` is also the one `wl-copy`
/// mode that leaves no daemonised survivor to outlive us: with `--clear`
/// there is no content to serve, so there is nothing to fork off and hold it.
///
/// No argument or output of the child is logged: the callers pass no
/// clipboard content, and keeping it that way is what stops a future caller
/// from putting a clip in the test log.
async fn bounded_status(
    program: &str,
    args: &[&str],
    limit: Duration,
) -> Result<std::process::ExitStatus, BoundedError> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| BoundedError::Spawn(e.to_string()))?;

    match tokio::time::timeout(limit, child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(e)) => Err(BoundedError::Wait(e.to_string())),
        Err(_) => {
            // Kill and reap, in that order, before telling the caller the
            // bound expired. `start_kill` is idempotent against a child that
            // has since exited on its own.
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(BoundedError::TimedOut)
        }
    }
}

/// Clears the system clipboard, bounded by the production timeout.
async fn clear_clipboard() -> Result<std::process::ExitStatus, BoundedError> {
    bounded_status("wl-copy", &["--clear"], BACKEND_TIMEOUT).await
}

// ---------------------------------------------------------------------------
// FEDORA-WAYLAND: read and write
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn the_real_clipboard_round_trips_text() {
    preserving_clipboard(|b| async move {
        let value = ClipboardText::validate("anyflow real-backend round trip").expect("valid");
        write(&b, &value, false).await;

        let read_back = read(&b)
            .await
            .expect("something should be on the clipboard");
        assert_eq!(read_back.as_str(), value.as_str());
        assert_eq!(
            read_back.hash(),
            value.hash(),
            "the content hash must survive the platform"
        );
    })
    .await;
}

/// CLIP-H05 / CLIP-H06 on the desktop side.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn the_real_clipboard_preserves_unicode_and_multiline_text_byte_for_byte() {
    preserving_clipboard(|b| async move {
        let cases = [
            "olá, ação e coração",
            "¿cómo estás? el ñandú",
            "🇧🇷 🎉 👨‍👩‍👧‍👦 café",
            "日本語 中文 한국어",
            "line one\nline two\r\nline three\tindented",
            "  significant  spaces  ",
            // A trailing newline is the case `wl-paste` gets wrong without
            // `--no-newline`, so it is worth its own case.
            "ends with a newline\n",
        ];

        for case in cases {
            let value = ClipboardText::validate(case).expect("valid");
            write(&b, &value, false).await;
            let read_back = read(&b).await.expect("clipboard should hold text");
            assert_eq!(
                read_back.as_str(),
                case,
                "the platform altered the text (bytes: {:?} -> {:?})",
                case.len(),
                read_back.len()
            );
        }
    })
    .await;
}

/// The size ceiling holds against the real clipboard, not just in memory.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn the_real_clipboard_carries_a_maximum_sized_clip() {
    preserving_clipboard(|b| async move {
        let big = "x".repeat(anyflow_capability_clipboard::limits::MAX_CLIPBOARD_TEXT_BYTES);
        let value = ClipboardText::validate(big.clone()).expect("valid at the limit");
        write(&b, &value, false).await;

        let read_back = read(&b).await.expect("clipboard should hold text");
        assert_eq!(
            read_back.len(),
            anyflow_capability_clipboard::limits::MAX_CLIPBOARD_TEXT_BYTES,
            "a maximum-sized clip must survive intact"
        );
        assert_eq!(read_back.as_str(), big);
    })
    .await;
}

/// A sensitive clip does on this machine exactly what this machine says it
/// can do — and nothing else.
///
/// # The defect this replaces (U2 TC1)
///
/// This test used to write a sensitive clip and assert it round-tripped,
/// unconditionally. That is only true where `wl-copy` understands
/// `--sensitive`, which is a property of the *installed tool* and not of the
/// platform: Ubuntu 24.04, Ubuntu 26.04 and Debian 13 all ship wl-clipboard
/// 2.2.1 without the flag, while Fedora's `2.2.1^git…` snapshot has it
/// (PLAT-DEC-013, U0 §8).
///
/// On those distributions the product did the right thing — ordinary
/// clipboard available, sensitive capability reported unavailable, sensitive
/// write refused fail-closed, no content handed to an unmarked `wl-copy` —
/// and this test failed anyway. A red gate for correct fail-closed behaviour
/// is worse than no gate: it trains the next person to ignore it.
///
/// # What is asserted instead
///
/// The **product contract**, which is the same sentence on every host:
///
/// > `sensitive_support()` is a promise, and `write_text(_, true)` keeps it
/// > in both directions.
///
/// * supported → the marked write succeeds and the content survives intact;
/// * unsupported → the write is refused with the typed capability error
///   [`BackendError::Unavailable`], the ordinary clipboard is untouched and
///   still works, and **no sensitive byte reaches the clipboard at all**.
///
/// Unsupported is an *asserted* state here, not a skipped one. A host without
/// the flag runs strictly more assertions than a host with it, because the
/// fail-closed path is the one with a privacy consequence if it breaks.
///
/// The measurement for "no byte reached the clipboard" is the clipboard
/// itself: an ordinary sentinel is placed first, and after the refusal the
/// clipboard must still hold that sentinel. An implementation that fell back
/// to an unmarked `wl-copy` would have replaced it with the canary, and that
/// is precisely the privacy regression PLAT-DEC-013 refuses to make.
/// `tests/sensitive_capability.rs` proves the same property one level lower,
/// against the child process's stdin, on every machine and without a
/// compositor.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn a_sensitive_write_honours_this_backend_s_advertised_capability() {
    /// Never written to an unmarked clipboard, and asserted to be absent when
    /// the capability is missing.
    const CANARY: &str = "anyflow-sensitive-canary-3f9c1a";
    const SENTINEL: &str = "anyflow ordinary sentinel";

    preserving_clipboard(|b| async move {
        // The suite cannot say anything about sensitive clips on a machine
        // with no usable clipboard at all. That is a different outcome from
        // "no `--sensitive`", and it is named rather than folded in.
        if let Err(why) = b.availability() {
            panic!(
                "this session has no usable clipboard, so the sensitive \
                 contract cannot be gated here: {why}"
            );
        }

        let support = b.sensitive_support();
        eprintln!(
            "sensitive marking on this host: {}",
            match &support {
                Ok(()) => "supported".to_string(),
                Err(why) => format!("unsupported ({why})"),
            }
        );

        // A known ordinary clip first. It proves the ordinary path works
        // before anything sensitive is attempted, and it is the marker the
        // refusal case measures against.
        let sentinel = ClipboardText::validate(SENTINEL).expect("valid");
        write(&b, &sentinel, false).await;
        assert_eq!(
            read(&b)
                .await
                .expect("the sentinel is on the clipboard")
                .as_str(),
            SENTINEL,
            "the ordinary clipboard must work before the sensitive path is judged"
        );

        let canary = ClipboardText::validate(CANARY).expect("valid");
        let outcome = b.write_text(&canary, true).await;

        match (support, outcome) {
            // ---------------------------------------------------------------
            // Supported: the promise is kept, and the marking does not corrupt
            // what it marks.
            // ---------------------------------------------------------------
            (Ok(()), Ok(())) => {
                let read_back = read(&b).await.expect("the marked clip is on the clipboard");
                assert_eq!(
                    read_back.as_str(),
                    CANARY,
                    "a marked clip must survive the platform byte for byte"
                );
                assert_eq!(
                    read_back.hash(),
                    canary.hash(),
                    "the content hash must survive marking"
                );
            }

            // ---------------------------------------------------------------
            // Unsupported: refused, typed, and inert.
            // ---------------------------------------------------------------
            (Err(why), Err(e)) => {
                // The typed capability error, not a generic failure. Before
                // Wave 0 this surfaced as `wl-copy exited with 1`, which told
                // the operator nothing and named no remedy.
                assert!(
                    matches!(e, BackendError::Unavailable(_)),
                    "a refused sensitive clip must be a capability error, not {e:?}"
                );
                // The reason a person can act on travelled with it.
                assert!(
                    !why.is_empty(),
                    "an unsupported backend must say why, so status can print it"
                );
                assert!(
                    !e.to_string().contains(CANARY),
                    "the refusal must not carry the content it refused"
                );

                // Fail-closed, measured rather than assumed: nothing was
                // written. The clipboard still holds the ordinary sentinel,
                // so no unmarked fallback copy happened.
                let after = read(&b).await.expect("the sentinel must still be there");
                assert_eq!(
                    after.as_str(),
                    SENTINEL,
                    "a refused sensitive clip must leave the clipboard untouched"
                );
                assert!(
                    !after.as_str().contains(CANARY),
                    "sensitive content reached the clipboard on a host that \
                     cannot mark it — this is the privacy regression \
                     PLAT-DEC-013 exists to prevent"
                );

                // And the ordinary clipboard is unharmed by the refusal: the
                // capability degrades, it does not break.
                let ordinary = ClipboardText::validate("ordinary after refusal").expect("valid");
                write(&b, &ordinary, false).await;
                assert_eq!(
                    read(&b).await.expect("text").as_str(),
                    "ordinary after refusal",
                    "ordinary mirroring must keep working where sensitive marking cannot"
                );
            }

            // ---------------------------------------------------------------
            // The predicate lied. Both directions are defects, and each has a
            // different consequence worth naming.
            // ---------------------------------------------------------------
            (Ok(()), Err(e)) => panic!(
                "sensitive_support() said this backend can mark a clip, and the \
                 marked write failed: {}\n\nA status screen that promises \
                 sensitive clips will arrive, on a machine where they do not, \
                 is worse than one that admits the gap.",
                explain(&e)
            ),
            (Err(why), Ok(())) => panic!(
                "sensitive_support() said this backend CANNOT mark a clip \
                 ({why}) and the write succeeded anyway.\n\nEither the probe \
                 is wrong, or the clip was written unmarked — and an unmarked \
                 write of a sensitive clip leaves a password in the desktop's \
                 clipboard history without telling anyone."
            ),
        }
    })
    .await;
}

/// An empty clipboard is "nothing to send", and never an unclassified fault.
///
/// A regression test for a real defect found on hardware. `wl-paste` exits
/// non-zero for a clipboard with no usable text and says either *"Nothing is
/// copied"* or *"Clipboard content is not available as requested type"* — and
/// the first classifier recognised neither, so an empty clipboard was
/// reported as a backend failure. The watcher logged an error on every
/// clipboard clear, and a manual send would have told the person their
/// clipboard was broken rather than empty.
///
/// # What is asserted, and why it is not `Ok(None)`
///
/// On GNOME, a clipboard with **no owner at all** — the state `wl-copy
/// --clear` leaves behind — makes `wl-paste` *block* rather than answer.
/// Measured at 6–8 s before the harness killed it, twice, for both MIME
/// types. That is the same root cause as the locked-screen case: with no
/// data-control protocol, `wl-paste` needs a seat and a serial from the
/// compositor, and with nothing owning the selection it waits.
///
/// So the assertion is the property that actually matters and that the
/// capability actually relies on: the read is **bounded**, and its result is
/// **classified**. `Ok(None)` and `TimedOut` are both fine — the manager
/// treats each as "nothing to send" and carries on. An unclassified
/// `Failed`, which is what the defect produced, is not.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn an_empty_clipboard_is_bounded_and_classified_never_an_unexplained_failure() {
    preserving_clipboard(|b| async move {
        // Bounded (issue #13). On a locked seat this returns `TimedOut`
        // after BACKEND_TIMEOUT instead of hanging the run forever.
        match clear_clipboard().await {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("wl-copy --clear exited with {status}; skipping");
                return;
            }
            Err(e @ BoundedError::TimedOut) => {
                // The locked-seat case, named rather than silent: this is the
                // same condition the backend's own timeout covers, and a run
                // that stopped here must say so out loud.
                eprintln!(
                    "wl-copy --clear {e} after {BACKEND_TIMEOUT:?} — the seat is \
                     most likely locked. Unlock the screen and run this again."
                );
                return;
            }
            Err(e) => {
                eprintln!("wl-copy --clear {e}; skipping");
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let started = std::time::Instant::now();
        let result = b.read_text().await;
        let elapsed = started.elapsed();

        // Bounded. The backend's own timeout is the ceiling; a little slack
        // for process spawn.
        assert!(
            elapsed < Duration::from_secs(12),
            "reading an empty clipboard took {elapsed:?}; it must be bounded"
        );

        match result {
            Ok(None) => eprintln!("empty clipboard: Ok(None) in {elapsed:?}"),
            Ok(Some(text)) => {
                // Another application re-took the clipboard between the clear
                // and the read — a real race on a live desktop, and not a
                // failure of the thing under test.
                eprintln!(
                    "another app took the clipboard mid-test ({} bytes); \
                     inconclusive rather than failing",
                    text.len()
                );
            }
            Err(BackendError::TimedOut) => {
                // The documented GNOME behaviour for an unowned selection.
                // The manager treats this as "nothing to send" and the
                // watcher logs it at debug, so nothing is broken.
                eprintln!("empty clipboard: TimedOut in {elapsed:?} (expected on GNOME)");
            }
            Err(e @ BackendError::Failed(_)) => panic!(
                "an empty clipboard must not be an unclassified failure: {}",
                explain(&e)
            ),
            Err(BackendError::Unavailable(why)) => {
                eprintln!("clipboard unavailable: {why}");
            }
        }
    })
    .await;
}

// ---------------------------------------------------------------------------
// FEDORA-WAYLAND: the watcher
// ---------------------------------------------------------------------------

/// The gate that matters: does this session report clipboard changes at all,
/// and does it do so without polling?
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn the_watcher_reports_every_local_change() {
    preserving_clipboard(|b| async move {
        let mut watch = match b.watch_changes() {
            Ok(watch) => watch,
            Err(BackendError::Unavailable(why)) => {
                // A legitimate outcome on a session with neither data-control
                // nor Xwayland. Reported, not silently passed.
                panic!(
                    "this session cannot report clipboard changes: {why}\n\n\
                     Auto-send is unavailable here; manual send still works. \
                     Record FEDORA-WAYLAND accordingly."
                );
            }
            Err(e) => panic!("could not start the watcher: {}", explain(&e)),
        };
        eprintln!("watch source: {}", watch.source);

        // Three distinct copies. Each must produce at least one signal.
        for i in 0..3 {
            let value = ClipboardText::validate(format!("watched change {i}")).expect("valid");
            write(&b, &value, false).await;

            let signalled = tokio::time::timeout(Duration::from_secs(5), watch.changes.recv())
                .await
                .unwrap_or_else(|_| panic!("no clipboard-change signal for copy {i} within 5s"));
            assert!(signalled.is_some(), "the watcher ended early");

            // Drain any coalesced extras so the next iteration starts clean.
            while watch.changes.try_recv().is_ok() {}

            // And the content the manager would read is the new one.
            let read_back = read(&b).await.expect("text");
            assert_eq!(read_back.as_str(), format!("watched change {i}"));
        }
    })
    .await;
}

/// The watcher must be idle when nothing happens — no polling, no wake-ups.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn the_watcher_is_silent_while_the_clipboard_is_unchanged() {
    preserving_clipboard(|b| async move {
        let Ok(mut watch) = b.watch_changes() else {
            eprintln!("no watch source on this session; nothing to assert");
            return;
        };

        // Settle: whatever the clipboard was doing before this test.
        let value = ClipboardText::validate("settling").expect("valid");
        write(&b, &value, false).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        while watch.changes.try_recv().is_ok() {}

        // Two seconds of nothing. A poller would tick; an event-driven source
        // says nothing at all.
        let spurious = tokio::time::timeout(Duration::from_secs(2), watch.changes.recv()).await;
        assert!(
            spurious.is_err(),
            "the watcher signalled a change that did not happen — this is what \
             a polling implementation looks like"
        );
    })
    .await;
}

/// Dropping the watch must release the platform side, so a restart does not
/// leave two watchers running.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn a_watch_can_be_stopped_and_restarted() {
    preserving_clipboard(|b| async move {
        for round in 0..3 {
            let Ok(mut watch) = b.watch_changes() else {
                eprintln!("no watch source on this session; nothing to assert");
                return;
            };

            let value = ClipboardText::validate(format!("restart round {round}")).expect("valid");
            write(&b, &value, false).await;

            let signalled = tokio::time::timeout(Duration::from_secs(5), watch.changes.recv())
                .await
                .unwrap_or_else(|_| panic!("round {round}: no signal within 5s"));
            assert!(signalled.is_some());

            // Dropping it here is the thing under test: the next iteration
            // must be able to start a fresh one.
            drop(watch);
        }
    })
    .await;
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// `anyflow clipboard status` must be able to say what works *before* anything
/// is attempted, so a person is not told to turn on a flag that cannot work.
#[tokio::test]
#[ignore = "inspects the real session; run with --ignored --test-threads=1"]
async fn detection_describes_this_session_accurately() {
    let b = backend();
    let description = b.describe();
    assert!(!description.is_empty());

    match b.watch_availability() {
        Ok(()) => {
            // The predicate promised a watch; starting one must work.
            let watch = b
                .watch_changes()
                .expect("watch_availability said yes, so watch_changes must succeed");
            eprintln!("FEDORA-WAYLAND watch source: {}", watch.source);
        }
        Err(why) => {
            eprintln!("FEDORA-WAYLAND watch unavailable: {why}");
            // And the predicate must not be lying in the other direction
            // either: a caller that ignored it and tried anyway gets the same
            // answer rather than a surprise success.
            assert!(
                b.watch_changes().is_err(),
                "watch_availability said no but watch_changes succeeded"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The bound itself (issue #13)
//
// These are *not* `#[ignore]`d and touch no clipboard: they drive
// `bounded_status` with stand-in commands so the locked-seat regression is
// caught by `cargo test` on a headless machine, not only by someone who
// remembers to lock their screen before a certification run.
// ---------------------------------------------------------------------------

/// A helper that exits normally is still reported normally.
#[tokio::test]
#[cfg(unix)]
async fn a_bounded_helper_that_exits_reports_its_status() {
    let ok = bounded_status("/bin/sh", &["-c", "exit 0"], Duration::from_secs(5))
        .await
        .expect("a command that exits must not time out");
    assert!(ok.success());

    let bad = bounded_status("/bin/sh", &["-c", "exit 3"], Duration::from_secs(5))
        .await
        .expect("a failing command still produces a status, not an error");
    assert!(!bad.success(), "a non-zero exit must not read as success");
}

/// The locked-seat simulation: a child that never returns is bounded.
///
/// `sleep 30` stands in for `wl-copy` behind a lock screen — a process that
/// has started fine and will simply never exit. Before the fix this suite
/// waited on exactly that with a blocking `status()`, and the run hung.
#[tokio::test]
#[cfg(unix)]
async fn a_helper_that_never_returns_is_bounded_and_fails_deterministically() {
    let started = std::time::Instant::now();
    let outcome = bounded_status("/bin/sh", &["-c", "sleep 30"], Duration::from_millis(200)).await;
    let elapsed = started.elapsed();

    // Deterministic: one named variant, not a string to be parsed.
    assert_eq!(
        outcome.expect_err("a child that never returns must time out"),
        BoundedError::TimedOut,
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the bound did not hold: waited {elapsed:?} for a 200ms limit"
    );
}

/// The killed child is reaped, not leaked as a zombie.
///
/// The production backend leaks nothing per locked-screen attempt, and a test
/// helper that did would be a worse version of the bug it fixes.
#[tokio::test]
#[cfg(target_os = "linux")]
async fn a_timed_out_helper_leaves_no_child_behind() {
    // Spawned the same way `bounded_status` does, so the pid is observable.
    let mut child = tokio::process::Command::new("/bin/sh")
        .args(["-c", "sleep 30"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("sh must be runnable");
    let pid = child.id().expect("a running child has a pid");

    assert!(
        std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "the child should be alive before the bound expires"
    );

    // The bounded_status timeout arm, verbatim.
    let _ = tokio::time::timeout(Duration::from_millis(200), child.wait()).await;
    let _ = child.start_kill();
    let _ = child.wait().await;

    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "pid {pid} survived the bound: killed but never reaped"
    );
}

/// The bound reports what happened without ever carrying content.
///
/// `bounded_status` takes its arguments explicitly and logs none of them, and
/// its error type has no field a clip could travel in. This pins that: a
/// clipboard payload passed as an argument must not reach the message.
#[tokio::test]
#[cfg(unix)]
async fn the_bound_never_reports_the_content_it_was_given() {
    const SECRET: &str = "correct-horse-battery-staple";

    let outcome = bounded_status(
        "/bin/sh",
        &["-c", &format!("echo {SECRET} >/dev/null; sleep 30")],
        Duration::from_millis(200),
    )
    .await;

    let error = outcome
        .as_ref()
        .expect_err("the stand-in never exits, so this must be an error");
    let rendered = format!("{outcome:?} {error}");
    assert!(
        !rendered.contains(SECRET),
        "the bound's own error must not carry what it was asked to run"
    );
}

/// A program that is not installed is `Spawn`, never `TimedOut`.
///
/// The two failures need different advice — "install wl-clipboard" versus
/// "unlock your screen" — so collapsing them into one error would put the
/// wrong instruction in a certification log.
#[tokio::test]
async fn a_missing_program_is_distinguishable_from_a_locked_seat() {
    let outcome = bounded_status(
        "anyflow-no-such-program-exists",
        &[],
        Duration::from_millis(200),
    )
    .await;

    match outcome.expect_err("a program that does not exist cannot be run") {
        BoundedError::Spawn(_) => {}
        other => panic!("a missing program must not report as {other:?}"),
    }
}
