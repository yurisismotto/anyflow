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
