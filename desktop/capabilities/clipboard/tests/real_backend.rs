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
//! # Two ways this can fail that are not bugs
//!
//! * **`wl-clipboard` is not installed** — reported as `Unavailable`, and the
//!   test says so rather than failing obscurely.
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

/// `wl-copy --sensitive` must not corrupt the content it marks.
#[tokio::test]
#[ignore = "touches the real system clipboard; run with --ignored --test-threads=1"]
async fn a_sensitive_write_still_round_trips() {
    preserving_clipboard(|b| async move {
        let value = ClipboardText::validate("sensitive round trip").expect("valid");
        write(&b, &value, true).await;
        assert_eq!(
            read(&b).await.expect("text").as_str(),
            "sensitive round trip"
        );
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
        let cleared = std::process::Command::new("wl-copy")
            .arg("--clear")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if cleared.map(|s| !s.success()).unwrap_or(true) {
            eprintln!("could not clear the clipboard; skipping");
            return;
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
