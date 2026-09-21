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

use common::*;
use omnibridge_capability_notifications::backend::{CloseReason, SinkError};
use omnibridge_capability_notifications::{LockPolicy, NotificationPolicy};
use omnibridge_proto::v1::capabilities as pb;
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
            "y".repeat(omnibridge_core::notifications::MAX_BODY_BYTES)
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

/// N4's own path: a human dismissal, the request it produces, and the source's
/// verdict on it. Three new log lines, none of which may carry content.
#[tokio::test]
async fn the_dismissal_path_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start_dismissing().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
        let server_id = h.sink.last_server_id().expect("displayed");

        h.close(server_id, CloseReason::Dismissed).await;
        let (id, _) = h.next_dismiss().await;
        assert_eq!(id, id_bytes(1));

        // The source's answer, and the removal that follows it.
        h.send(&result(1, pb::NotificationOutcome::Removed)).await;
        h.send(&remove(1)).await;
        h.expect_outcome(pb::NotificationOutcome::UnknownNotification)
            .await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(
        logs.contains("asked the source to dismiss it too"),
        "the wanted diagnostic is absent, so the assertion above proved nothing"
    );
    assert!(logs.contains("peer reported a notification outcome"));
}

/// The refusal paths log a **reason class** and an opaque identity prefix, and
/// nothing else. This is the line somebody reads when they turned the setting
/// on and their phone did not clear, so it has to be present *and* clean.
#[tokio::test]
async fn a_refused_dismissal_logs_a_reason_class_and_no_content() {
    let logs = capture(|| async {
        // Policy off: the ordinary case, and the one with the most traffic.
        let mut h = Harness::start_ungranted().await;
        h.policies
            .grant(h.peer, NotificationPolicy::default())
            .await;
        h.expect_roles().await;
        h.send(&roles(
            &[
                pb::NotificationRole::Source,
                pb::NotificationRole::DismissTarget,
            ],
            1,
        ))
        .await;

        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
        let server_id = h.sink.last_server_id().expect("displayed");
        h.close(server_id, CloseReason::Dismissed).await;
        h.expect_no_dismiss().await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("a human dismissal was not sent to the source"));
    assert!(
        logs.contains("policy"),
        "the reason class is what makes the line useful"
    );
}

/// A close reason that must never travel still produces a log line, and that
/// line names the reason and no notification.
#[tokio::test]
async fn a_non_human_close_logs_its_reason_and_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start_dismissing().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
        let server_id = h.sink.last_server_id().expect("displayed");

        h.close(server_id, CloseReason::Expired).await;
        h.expect_no_dismiss().await;
    })
    .await;

    assert_no_canaries(&logs);
    assert!(logs.contains("the desktop closed a mirror"));
    assert!(logs.contains("expired"));
    assert!(
        !logs.contains("asked the source to dismiss"),
        "an expiry must not even look like a dismissal in the journal"
    );
}

/// A `DismissRequest` this desktop *receives* is still refused, and refusing
/// it logs no content either.
#[tokio::test]
async fn an_inbound_dismiss_refusal_logs_no_content() {
    let logs = capture(|| async {
        let mut h = Harness::start_dismissing().await;
        h.send_upsert(canary_upsert(1)).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
        h.send(&dismiss(1)).await;
        h.expect_outcome(pb::NotificationOutcome::RejectedRole)
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
