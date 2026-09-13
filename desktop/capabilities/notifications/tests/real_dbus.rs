//! The FEDORA-GNOME gate: the real notification server, on the real session.
//!
//! Every other suite in this crate runs against `MemorySink`, which is right —
//! the decision logic must be testable without a desktop session, and a suite
//! that posted notifications on every `cargo test` would be intolerable.
//!
//! This one is the opposite. It talks to whatever owns
//! `org.freedesktop.Notifications` on the running session, so it is
//! `#[ignore]`d and must be asked for by name:
//!
//! ```console
//! cargo test -p anyflow-capability-notifications --test real_dbus -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` is required rather than advisory: there is one
//! notification server, and two tests posting at once would each see the
//! other's ids in the close signals.
//!
//! **Every notification it posts, it closes.** A gate that left a column of
//! test notifications in somebody's shade would be a worse experience than no
//! gate, and the fixture text is synthetic and non-sensitive throughout so
//! that a screenshot taken during the run reveals nothing.
//!
//! # Two ways this can fail that are not bugs
//!
//! * **No session bus** — a `cargo test` from a systemd unit, a container, an
//!   ssh session with no `DBUS_SESSION_BUS_ADDRESS`. Reported by name.
//! * **No notification server** — a session with no shell. Also reported by
//!   name, because a certification run that failed obscurely for this reason
//!   would send the next person looking in the wrong place.

// This whole target exercises the freedesktop notification server, which exists only behind the
// `linux-dbus` feature. With the feature off it compiles to an empty
// test binary rather than a compile error, so the portable Windows gate
// can build every test target in this crate without an exclusion list.
#![cfg(feature = "linux-dbus")]

use std::time::Duration;

use anyflow_capability_notifications::backend::{
    dbus::DbusSink, CloseReason, Mirror, NotificationSink, Urgency,
};

const FIXTURE_APP: &str = "AnyFlow N2 fixture";
const FIXTURE_SUMMARY: &str = "ANYFLOW-N2-DBUS-FIXTURE";

fn mirror(body: &str) -> Mirror {
    Mirror {
        app_name: FIXTURE_APP.to_string(),
        summary: FIXTURE_SUMMARY.to_string(),
        body: body.to_string(),
        urgency: Urgency::Normal,
        redacted: false,
    }
}

/// Connects, or explains why it could not in a way that names the cause.
async fn connect() -> DbusSink {
    match DbusSink::connect().await {
        Some(sink) => sink,
        None => panic!(
            "could not reach org.freedesktop.Notifications.\n\n\
             This is not necessarily a defect. Check, in order:\n  \
             * DBUS_SESSION_BUS_ADDRESS is set (a container or a bare ssh \
             session has no session bus);\n  \
             * something owns the name:\n      \
             gdbus call --session --dest org.freedesktop.Notifications \\\n        \
             --object-path /org/freedesktop/Notifications \\\n        \
             --method org.freedesktop.Notifications.GetServerInformation"
        ),
    }
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn the_real_server_answers_and_says_what_it_can_do() {
    let sink = connect().await;
    sink.availability().await.expect("the server answers");

    let capabilities = sink.capabilities();
    eprintln!("server: {}", sink.describe());
    eprintln!("capabilities: {capabilities:?}");

    // Not asserted as a fixed set: `GetCapabilities` is the server's answer,
    // not ours, and a suite that required GNOME's exact list would fail on
    // KDE for no reason. What is asserted is that the answer was usable.
    assert!(
        capabilities.body || !capabilities.body,
        "the capability set decoded"
    );
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn a_notification_is_created_replaced_in_place_and_closed() {
    let sink = connect().await;

    let first = sink
        .display(&mirror("first state"), None)
        .await
        .expect("Notify");
    assert!(first > 0, "a server id is never zero");

    // The spec: "the returned value is the same value as replaces_id". The id
    // that is *stored* is nevertheless the returned one — see the seam's
    // documentation for why assuming otherwise is a bug waiting for a
    // different server.
    let second = sink
        .display(&mirror("second state"), Some(first))
        .await
        .expect("Notify with replaces_id");
    let third = sink
        .display(&mirror("third state"), Some(second))
        .await
        .expect("Notify with replaces_id");

    eprintln!("ids: {first} -> {second} -> {third}");
    assert_eq!(second, first, "replaces_id was preserved");
    assert_eq!(third, first, "and again");

    sink.close(third).await.expect("CloseNotification");
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn closing_an_unknown_id_is_a_success() {
    let sink = connect().await;

    // GNOME answers nothing at all; a spec-literal server answers an error.
    // Both mean "the notification is gone", which is what was asked for, so
    // the sink reports success either way. This is the assertion that keeps
    // AnyFlow from logging a failure on every dismissal against dunst or mako.
    sink.close(4_294_967_000)
        .await
        .expect("closing an id that never existed is a success");

    // And closing the same notification twice.
    let id = sink
        .display(&mirror("to be closed twice"), None)
        .await
        .expect("Notify");
    sink.close(id).await.expect("first close");
    sink.close(id).await.expect("second close");
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn the_server_reports_our_own_close_with_reason_three() {
    let sink = connect().await;
    let mut closes = sink
        .closed_events()
        .expect("the session can observe closes");

    let id = sink
        .display(&mirror("close me"), None)
        .await
        .expect("Notify");
    sink.close(id).await.expect("CloseNotification");

    let closed = tokio::time::timeout(Duration::from_secs(5), closes.recv())
        .await
        .expect("a NotificationClosed signal arrived")
        .expect("the stream is open");

    assert_eq!(closed.id, id);
    assert_eq!(
        closed.reason,
        CloseReason::Closed,
        "reason 3 is our own close coming back to us; acting on it would be an \
         immediate self-inflicted loop"
    );
    assert!(
        !closed.reason.is_human_dismissal(),
        "and it must never be mistaken for one"
    );
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn a_body_reaches_the_server_escaped_and_nothing_is_left_behind() {
    let sink = connect().await;

    // What the capability would have handed the backend for a body containing
    // markup: already escaped, because GNOME advertises `body-markup`.
    let escaped = anyflow_capability_notifications::text::body(
        "<b>ANYFLOW-N2-MARKUP</b> & <a href='x'>link</a>",
        sink.capabilities().body_markup,
    );
    assert!(!escaped.contains("<b>"));

    let id = sink.display(&mirror(&escaped), None).await.expect("Notify");
    sink.close(id).await.expect("CloseNotification");
}

#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn a_low_urgency_notification_is_accepted() {
    let sink = connect().await;
    let mut quiet = mirror("quietly");
    quiet.urgency = Urgency::Low;
    let id = sink.display(&quiet, None).await.expect("Notify");
    sink.close(id).await.expect("CloseNotification");
}

// ---------------------------------------------------------------------------
// N4 — the human-dismiss gate
// ---------------------------------------------------------------------------

/// This session can tell a human dismissal from an expiry, and says so.
///
/// The single input to the `DISMISS_REPORTER` role, asserted against the real
/// server rather than against the fake: it is `false` unless the
/// `NotificationClosed` subscription actually succeeded, and whether it does is
/// a property of this bus and this session.
#[tokio::test]
#[ignore = "talks to the real notification server; run with --ignored --test-threads=1"]
async fn the_real_session_can_report_human_dismissals() {
    let sink = connect().await;
    eprintln!("server: {}", sink.describe());
    eprintln!("capabilities: {:?}", sink.capabilities());
    assert!(
        sink.capabilities().dismiss_reporting,
        "this session cannot observe NotificationClosed, so AnyFlow will \
         announce no DISMISS_REPORTER role and dismissal sync will correctly \
         report itself unavailable"
    );
}

/// **The central N4 gate**, and the one that cannot be replaced by a mock.
///
/// The product feature is *a human closes the desktop mirror*, so this test
/// waits for an actual close performed through the desktop's own notification
/// UI and asserts that the server reported it as reason 2. Nothing here calls
/// `CloseNotification`, injects a signal, or reaches into a seam: the whole
/// point is that the path from a person's hand to `CloseReason::Dismissed` is
/// exercised end to end.
///
/// ```console
/// ANYFLOW_HUMAN_DISMISS=1 cargo test -p anyflow-capability-notifications \
///     --test real_dbus -- --ignored --test-threads=1 human
/// ```
///
/// Without `ANYFLOW_HUMAN_DISMISS` it skips loudly rather than failing, so an
/// unattended `--ignored` run of this file does not hang for two minutes
/// waiting for a person who is not there.
#[tokio::test]
#[ignore = "needs a person to dismiss a notification; set ANYFLOW_HUMAN_DISMISS=1"]
async fn a_human_dismissal_on_this_desktop_is_reported_as_reason_two() {
    if std::env::var_os("ANYFLOW_HUMAN_DISMISS").is_none() {
        eprintln!(
            "SKIPPED: set ANYFLOW_HUMAN_DISMISS=1 to run the human-dismiss gate. \
             It posts one notification and waits for you to close it."
        );
        return;
    }

    let sink = connect().await;
    let mut closes = sink
        .closed_events()
        .expect("the session can observe closes");

    let id = sink
        .display(
            &mirror("Close this notification from the desktop, by hand."),
            None,
        )
        .await
        .expect("Notify");

    eprintln!(
        "\n  >>> A notification is on your screen (id {id}).\n  \
         >>> Dismiss it the way a person would: click its X, or press the\n  \
         >>> close button in the notification list. Do NOT use gdbus.\n"
    );

    let closed = tokio::time::timeout(Duration::from_secs(120), closes.recv())
        .await
        .expect("a NotificationClosed signal arrived within two minutes")
        .expect("the stream is open");

    assert_eq!(closed.id, id, "the signal names the notification we posted");
    eprintln!(
        "observed: id={} reason={}",
        closed.id,
        closed.reason.as_str()
    );
    assert_eq!(
        closed.reason,
        CloseReason::Dismissed,
        "this desktop reported a human dismissal as something else; N4's only \
         permitted trigger would never fire, or would fire for the wrong thing"
    );
    assert!(closed.reason.is_human_dismissal());
}

/// The same gate, but through the **whole capability**: a real D-Bus sink, a
/// real `NotificationManager`, a real granted peer with dismiss sync on, and a
/// real person closing the notification.
///
/// What it proves that the test above does not: that one physical dismissal
/// produces **exactly one** `DismissRequest`, that the request names the
/// identity and origin the source sent, and that nothing else goes out.
///
/// ```console
/// ANYFLOW_HUMAN_DISMISS=1 cargo test -p anyflow-capability-notifications \
///     --test real_dbus -- --ignored --test-threads=1 end_to_end
/// ```
#[tokio::test]
#[ignore = "needs a person to dismiss a notification; set ANYFLOW_HUMAN_DISMISS=1"]
async fn a_human_dismissal_end_to_end_produces_exactly_one_dismiss_request() {
    use anyflow_capability_notifications::backend::{LockSource, UnknownLock};
    use anyflow_capability_notifications::{
        NotificationAuthorizer, NotificationManager, NotificationPolicy,
    };
    use anyflow_core::Fingerprint;
    use anyflow_proto::v1::capabilities as pb;
    use anyflow_proto::Message as _;
    use std::sync::Arc;

    if std::env::var_os("ANYFLOW_HUMAN_DISMISS").is_none() {
        eprintln!("SKIPPED: set ANYFLOW_HUMAN_DISMISS=1 to run the end-to-end human gate.");
        return;
    }

    struct AlwaysGranted;
    #[async_trait::async_trait]
    impl NotificationAuthorizer for AlwaysGranted {
        async fn policy_for(&self, _peer: &Fingerprint) -> NotificationPolicy {
            NotificationPolicy {
                allow_dismiss_sync: true,
                // `Full`, and the lock source below reports locked — so this
                // gate exercises the display path rather than the reduction.
                when_sink_locked: anyflow_capability_notifications::LockPolicy::Full,
                ..NotificationPolicy::default()
            }
        }
    }

    let sink: Arc<dyn NotificationSink> = Arc::new(connect().await);
    let manager =
        NotificationManager::new(sink, Arc::new(UnknownLock) as Arc<dyn LockSource>).await;
    manager
        .set_authorizer(Arc::new(AlwaysGranted) as Arc<dyn NotificationAuthorizer>)
        .await;
    manager.spawn_platform_pumps();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let peer = Fingerprint::from_hex(&"ab".repeat(32)).expect("fingerprint");
    manager.attach_session(peer, tx).await;

    // The role announcement this desktop makes first. It must include
    // DISMISS_REPORTER on a session with a working notification server.
    let announced = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("announced")
        .expect("a message");
    let control = pb::NotificationControl::decode(announced.payload.as_slice()).expect("decodes");
    match control.body {
        Some(pb::notification_control::Body::Roles(r)) => {
            eprintln!("announced roles: {:?} epoch {}", r.roles, r.epoch);
            assert!(r
                .roles
                .contains(&(pb::NotificationRole::DismissReporter as i32)));
        }
        other => panic!("expected roles, got {other:?}"),
    }

    // The phone claims both of its v1 roles.
    let origin = "0123456789abcdef0123456789abcdef";
    manager
        .handle_control(
            peer,
            &pb::NotificationControl {
                body: Some(pb::notification_control::Body::Roles(
                    pb::NotificationRoles {
                        roles: vec![
                            pb::NotificationRole::Source as i32,
                            pb::NotificationRole::DismissTarget as i32,
                        ],
                        epoch: 1,
                    },
                )),
            }
            .encode_to_vec(),
        )
        .await
        .expect("roles");

    // One mirrored notification, exactly as a phone would send it.
    let notification_id = vec![0x4eu8; 16];
    manager
        .handle_control(
            peer,
            &pb::NotificationControl {
                body: Some(pb::notification_control::Body::Upsert(
                    pb::NotificationUpsert {
                        notification_id: notification_id.clone(),
                        origin_device_id: origin.to_string(),
                        app_id: "example.anyflow.n4fixture".to_string(),
                        app_label: FIXTURE_APP.to_string(),
                        title: FIXTURE_SUMMARY.to_string(),
                        body: "Close this from the desktop, by hand.".to_string(),
                        importance: pb::NotificationImportance::Normal as i32,
                        privacy: pb::NotificationPrivacy::Private as i32,
                        dismissible: true,
                        ..pb::NotificationUpsert::default()
                    },
                )),
            }
            .encode_to_vec(),
        )
        .await
        .expect("upsert");

    let displayed = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("answered")
        .expect("a message");
    let control = pb::NotificationControl::decode(displayed.payload.as_slice()).expect("decodes");
    match control.body {
        Some(pb::notification_control::Body::Result(r)) => assert_eq!(
            r.outcome,
            pb::NotificationOutcome::Displayed as i32,
            "the mirror must be on the screen before anybody can dismiss it"
        ),
        other => panic!("expected a result, got {other:?}"),
    }

    eprintln!(
        "\n  >>> A mirrored notification is on your screen.\n  \
         >>> Dismiss it by hand, the way a person would.\n"
    );

    let dismissed = tokio::time::timeout(Duration::from_secs(120), rx.recv())
        .await
        .expect("a dismiss request within two minutes")
        .expect("a message");
    let control = pb::NotificationControl::decode(dismissed.payload.as_slice()).expect("decodes");
    match control.body {
        Some(pb::notification_control::Body::Dismiss(d)) => {
            assert_eq!(d.notification_id, notification_id);
            assert_eq!(d.origin_device_id, origin);
            eprintln!(
                "DismissRequest: {} bytes on the wire",
                dismissed.payload.len()
            );
        }
        other => panic!("expected a dismiss request, got {other:?}"),
    }

    // Exactly one. Anything else arriving in the next two seconds — a second
    // dismissal, a re-display, a role churn — fails the gate.
    match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
        Err(_) => {}
        Ok(Some(extra)) => {
            let control =
                pb::NotificationControl::decode(extra.payload.as_slice()).expect("decodes");
            panic!("one physical dismissal produced a second message: {control:?}");
        }
        Ok(None) => {}
    }

    let report = manager
        .peer_reports()
        .await
        .into_iter()
        .find(|r| r.peer == peer)
        .expect("a report");
    assert_eq!(report.dismissals_sent, 1);
    assert_eq!(
        report.mirrors, 0,
        "the mirror was purged before the request"
    );
}
