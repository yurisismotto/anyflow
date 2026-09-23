//! SEC-LOG-03 — file content never reaches a log.
//!
//! # Why this file did not exist before
//!
//! `clipboard.v1` and `notifications.v1` each got a log-privacy canary when
//! they were built: `capabilities/clipboard/tests/logging.rs` and
//! `capabilities/notifications/tests/logging.rs`. `files.v1` did not, and
//! Security Certification v1 found the gap — SEC-LOG-03 had no evidence
//! anywhere while SEC-LOG-01 and SEC-LOG-02 were covered twice over.
//!
//! # Its own binary, deliberately
//!
//! Every test here installs a `tracing` subscriber. That is a requirement, not
//! a coincidence, and it is why this is a separate file rather than three more
//! tests in `files.rs`.
//!
//! `tracing::subscriber::set_default` installs a subscriber on **one thread**
//! but raises the process-wide max level, after which any *subscriber-less*
//! thread that first registers a callsite caches it as `Interest::never()`
//! **globally**. A capture sharing a binary with 36 tests that do not install
//! subscribers therefore records nothing, intermittently, and the log-privacy
//! assertion passes while proving nothing. That is not hypothetical: it is
//! what `daemon/tests/notification_log_privacy.rs` was extracted to fix, and
//! its header carries the measurement.
//!
//! **Keep it that way.** A test added here that does not install a capture
//! subscriber reintroduces the bug in its worst form.
//!
//! # What this asserts, and what it deliberately does not
//!
//! It asserts that the **bytes of a transferred file** never appear in the
//! daemon's output, at `TRACE`, across an entire successful transfer.
//!
//! It does **not** assert that the filename is absent, because it is not:
//! `capabilities/files/src/lib.rs` logs `filename = %filename` at `info` on
//! the incoming offer and again on storage. That is a deliberate product
//! choice about metadata rather than a leak of content, and SEC-LOG-03 is
//! about content — but it is a real difference from how `notifications.v1`
//! treats a title, and the certification records it as a finding rather than
//! letting this test quietly imply that nothing about a file is logged.

mod common;

use std::time::Duration;

use common::*;
use omnibridge_capability_files::transfer::TransferState;

const GRACE: Duration = Duration::from_secs(10);

/// Paired, with `files.v1` granted.
///
/// A local copy of `files.rs`'s helper: it lives in that test binary, and this
/// one is deliberately separate (see the header). Six lines is a smaller cost
/// than putting a capture-less test in the same process as a capture.
async fn paired(server: &TestServer, phone: &TestClient) -> ConnectedSession {
    let token = server.open_pairing(Duration::from_secs(30)).await;
    let first = phone
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    server.set_grant(phone.fingerprint, "files.v1", true).await;
    first.close().await;

    phone
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect with the grant in place")
}

/// The file's bytes, at `TRACE`, across a whole transfer.
#[tokio::test]
async fn sec_log_03_file_content_never_reaches_the_log() {
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    // Long, unique, and not a substring of anything a formatter emits.
    const CONTENT: &str = "CANARY-FILE-CONTENT-3f9a1c седьмой-内容-e71b";

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("not poisoned").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();

    let probe_seen;
    {
        let _guard = tracing::subscriber::set_default(subscriber);

        // A probe, so a failure can say *which* thing broke: a subscriber that
        // never took effect, or callsites disabled by another thread. Cleared
        // immediately, or its bytes would make `is_empty()` impossible and
        // hide the failure it exists to explain.
        tracing::error!("CAPTURE-PROBE");
        probe_seen = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned"))
            .contains("CAPTURE-PROBE");
        captured.0.lock().expect("not poisoned").clear();

        let server = TestServer::start().await;
        let phone = TestClient::new("phone");
        let session = paired(&server, &phone).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("quarterly-results.txt");
        // Repeated so the payload is large enough to cross the copy buffer,
        // which is where a debug print of a chunk would live if one existed.
        let payload = CONTENT.repeat(400);
        std::fs::write(&source, &payload).expect("writing the sample file");

        let id = phone
            .transfers
            .offer_file(server.fingerprint, source)
            .await
            .expect("offer");

        let snapshot = wait_for_terminal(&phone.transfers, id, GRACE).await;
        assert_eq!(
            snapshot.state,
            TransferState::Completed,
            "the transfer must actually succeed, or the log has nothing to leak: {:?}",
            snapshot.failure
        );

        // It really arrived, byte for byte.
        let stored = server.downloads.join("quarterly-results.txt");
        assert_eq!(
            std::fs::read(&stored).expect("reading the stored file"),
            payload.as_bytes(),
            "the stored file does not match what was sent"
        );

        session.close().await;
    }

    let text = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned")).into_owned();

    // ---- the capture must be real before it can prove anything -----------
    assert!(
        !text.is_empty(),
        "nothing was captured, so this test proves nothing. probe_seen={probe_seen}: \
         if true the subscriber worked and the daemon's callsites were disabled by a \
         subscriber-less thread registering them first; if false the subscriber never \
         took effect at all. See this file's header."
    );
    assert!(
        text.contains("omnibridge_"),
        "no daemon event reached the capture, so this test proves nothing:\n{text}"
    );
    // The transfer's own log lines must be present, or the capture missed the
    // very code path under test.
    assert!(
        text.contains("transfer"),
        "the capture contains no transfer events; it did not observe the transfer:\n{text}"
    );

    // ---- and now the property --------------------------------------------
    assert!(
        !text.contains(CONTENT),
        "file content reached the log.\ncanary: {CONTENT}\ncaptured output:\n{text}"
    );
    // A partial leak is still a leak: a truncated debug print of the first
    // chunk would not contain the whole canary.
    let prefix = &CONTENT[..24];
    assert!(
        !text.contains(prefix),
        "a prefix of the file content reached the log.\nprefix: {prefix}\n\
         captured output:\n{text}"
    );
}

/// The filename *is* logged, and this records it rather than hiding it.
///
/// Not an assertion that it should be — it is a characterisation test. If a
/// later change redacts the filename, this fails and whoever made that change
/// deletes it deliberately, with the certification's finding in front of them.
/// If a later change starts logging something worse, the first test above
/// fails.
#[tokio::test]
async fn sec_log_03_the_filename_is_logged_and_that_is_a_recorded_finding() {
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    const FILENAME: &str = "CANARY-FILENAME-8b2d.txt";

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("not poisoned").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Captured;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();

    {
        let _guard = tracing::subscriber::set_default(subscriber);
        tracing::error!("CAPTURE-PROBE");
        captured.0.lock().expect("not poisoned").clear();

        let server = TestServer::start().await;
        let phone = TestClient::new("phone");
        let session = paired(&server, &phone).await;

        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join(FILENAME);
        std::fs::write(&source, b"short").expect("writing the sample file");

        let id = phone
            .transfers
            .offer_file(server.fingerprint, source)
            .await
            .expect("offer");
        let snapshot = wait_for_terminal(&phone.transfers, id, GRACE).await;
        assert_eq!(
            snapshot.state,
            TransferState::Completed,
            "{:?}",
            snapshot.failure
        );
        session.close().await;
    }

    let text = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned")).into_owned();
    assert!(
        !text.is_empty(),
        "nothing was captured; see this file's header"
    );

    assert!(
        text.contains(FILENAME),
        "the filename no longer appears in the log. That is very likely an \
         improvement — SECURITY-CERTIFICATION-V1.md records filename logging as \
         finding F-1 — but it is a deliberate change in behaviour, so delete this \
         test along with it rather than leaving a characterisation of something \
         that is no longer true.\ncaptured output:\n{text}"
    );
}
