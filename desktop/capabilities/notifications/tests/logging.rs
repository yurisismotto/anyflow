//! NOTIF-SEC-25, the in-process half: notification content never reaches a log.
//!
//! Grepping the source proves the *current* logging is clean. This proves it
//! by construction: a full flow runs with a `tracing` subscriber capturing
//! every event at `TRACE`, driven with canary strings that appear nowhere
//! else, and the captured output must not contain them. A future log line
//! added with `?message` or `%title` fails this test rather than shipping.
//!
//! `TRACE`, not `INFO`. A leak at a level nobody reads in production is still
//! a leak, because `RUST_LOG=trace` is exactly what somebody runs when
//! something is wrong — and that is the worst possible moment to spill a
//! stranger's message into a file they are about to attach to a bug report.
//!
//! The canaries are distinctive on purpose: a substring assertion against
//! ordinary words would pass by accident, and one against a word that occurs
//! in an error message would fail by accident.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use anyflow_capability_notifications::backend::{CloseReason, SinkError};
use anyflow_capability_notifications::{LockPolicy, NotificationPolicy};
use anyflow_proto::v1::capabilities as pb;
use common::*;
use tracing_subscriber::fmt::MakeWriter;

const TITLE: &str = "CANARY-TITLE-a41f9c2e7b";
const BODY: &str = "CANARY-BODY-77d3e0b1c5";
const APP_LABEL: &str = "CANARY-APPLABEL-5b8a1f4c";
const APP_ID: &str = "canary.appid.e2c7d904";

/// Collects everything a `tracing` subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("not poisoned")).into_owned()
    }
}

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

/// Runs `body` with every `tracing` event captured, and returns the output.
///
/// A *scoped* subscriber, not a global one: `set_global_default` can be called
/// only once per process, and each test here needs its own capture.
async fn capture<F, Fut>(body: F) -> String
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();

    let guard = tracing::subscriber::set_default(subscriber);
    body().await;
    drop(guard);

    captured.text()
}

fn assert_no_canaries(logs: &str) {
    for canary in [TITLE, BODY, APP_LABEL, APP_ID, "CANARY"] {
        assert!(
            !logs.contains(canary),
            "notification content reached the log.\n\
             canary: {canary}\n\
             captured output:\n{logs}"
        );
    }
    // Sanity: the capture is actually working. A test that captured nothing
    // would pass vacuously, which is the failure mode this guards against.
    assert!(
        !logs.is_empty(),
        "no log output was captured at all; the assertion above proved nothing"
    );
}

fn canary_upsert(seed: u16) -> pb::NotificationUpsert {
    pb::NotificationUpsert {
        app_id: APP_ID.to_string(),
        app_label: APP_LABEL.to_string(),
        ..upsert(seed, TITLE, BODY)
    }
}

#[tokio::test]
async fn a_full_display_update_and_remove_flow_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;

        // An update, so the replacement path logs too.
        let mut updated = canary_upsert(1);
        updated.body = format!("{BODY}-UPDATED");
        updated.content_hash = content_hash(TITLE, &updated.body);
        h.send_upsert(updated).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;

        // An identical re-send, so the duplicate path logs too.
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;

        h.send(&remove(1)).await;
        let _ = h.next_result().await;
    })
    .await;

    assert_no_canaries(&logs);
    // And the diagnostics that *are* wanted are present, so this is not
    // passing because nothing was logged about notifications at all.
    assert!(logs.contains("notification upsert"));
    assert!(logs.contains("notification removal"));
}

#[tokio::test]
async fn a_refusal_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start_ungranted().await;
        h.expect_roles().await;
        h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

        // Ungranted.
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::NotAuthorized)
            .await;

        // Over a field limit.
        h.policies
            .grant(h.peer, NotificationPolicy::default())
            .await;
        let mut oversized = canary_upsert(2);
        oversized.body = format!(
            "{BODY}{}",
            "y".repeat(anyflow_core::notifications::MAX_BODY_BYTES)
        );
        h.send_upsert(oversized).await;
        h.expect_outcome(pb::NotificationOutcome::TooLarge).await;

        // Containing a NUL.
        let mut nul = canary_upsert(3);
        nul.title = format!("{TITLE}\0");
        nul.content_hash.clear();
        h.send_upsert(nul).await;
        h.expect_outcome(pb::NotificationOutcome::Invalid).await;

        // A secret notification.
        let mut secret = canary_upsert(4);
        secret.privacy = pb::NotificationPrivacy::Secret as i32;
        h.send_upsert(secret).await;
        h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
            .await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("refused a notifications.v1 message"));
}

#[tokio::test]
async fn a_backend_failure_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start().await;
        // A backend error message that quotes the call's arguments is a real
        // possibility: some notification servers do exactly that. The class,
        // not the message, is what may be logged.
        h.sink.set_failure(Some(SinkError::Failed(format!(
            "Notify failed for summary={TITLE:?} body={BODY:?}"
        ))));
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Failed).await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("a notification backend call failed"));
}

#[tokio::test]
async fn the_lock_reduction_and_the_close_signal_log_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;

        h.manager.set_locked(true).await;
        h.wait_for("the screen lock to reduce the mirror", || {
            h.sink.live(1).is_some_and(|m| m.body.is_empty())
        })
        .await;

        h.sink.user_closes(1, CloseReason::Dismissed).await;
        h.barrier().await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("the session locked"));
    assert!(logs.contains("the desktop closed a mirror"));
}

#[tokio::test]
async fn the_snapshot_and_role_paths_log_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;

        let sync = [0xA1u8; 16];
        h.send(&marker(&sync, pb::sync_marker::Phase::Begin)).await;
        h.send(&marker(&sync, pb::sync_marker::Phase::End)).await;
        h.wait_for("the snapshot to converge", || !h.sink.closes().is_empty())
            .await;

        // A revocation, and a narrowing role announcement.
        h.send(&roles(&[], 2)).await;
        h.barrier().await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("snapshot complete"));
}

#[tokio::test]
async fn a_suppressing_lock_policy_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start().await;
        h.policies
            .grant(
                h.peer,
                NotificationPolicy {
                    when_sink_locked: LockPolicy::Suppress,
                    ..NotificationPolicy::default()
                },
            )
            .await;
        h.lock.set_locked(true).await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
            .await;
    })
    .await;

    assert_no_canaries(&logs);
}

#[tokio::test]
async fn the_mirror_table_rendering_carries_no_content() {
    // The one type that holds a peer-supplied string — the application's
    // name — has a hand-written `Display` that counts instead of describing.
    // Even an application's name is something the user did not ask to have
    // written into a journal.
    let mut h = Harness::start().await;
    h.send_upsert(canary_upsert(1)).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let report = h.report().await;
    let rendered = format!("{report:?}");
    for canary in [TITLE, BODY, APP_LABEL, APP_ID] {
        assert!(!rendered.contains(canary), "{canary} reached a report");
    }
}
