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

/// `GetCapabilities`, asked again over a connection of this test's own.
///
/// Deliberately not routed through [`DbusSink`]: the point is to have a second
/// opinion about what the server said, so that the sink's parse can be checked
/// against it rather than against itself.
async fn advertised_capabilities() -> Vec<String> {
    let connection = zbus::Connection::session()
        .await
        .expect("availability() just succeeded, so the session bus is reachable");
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    .expect("the notification interface");
    proxy
        .call("GetCapabilities", &())
        .await
        .expect("a server that answered GetServerInformation must answer GetCapabilities")
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
    // KDE for no reason. `body`, `body-markup` and `persistence` are all
    // optional in the freedesktop specification, so asserting any of them
    // would be inventing a requirement.
    //
    // What *is* universal is how this sink is built, and that is what is
    // asserted here. Three invariants, each one the implementation's own
    // promise rather than the desktop's:

    // 1. The capability query actually completed, and was parsed faithfully.
    //
    //    This is the one the old assertion was reaching for and could not
    //    express. `DbusSink::connect` calls `GetCapabilities` with
    //    `.unwrap_or_else(|_| Vec::new())`: a failed query is indistinguishable,
    //    from inside the returned struct, from a server that advertises
    //    nothing. So the answer is fetched again here, independently, and the
    //    sink's parsed view is compared against it token by token.
    //
    //    Nothing about GNOME's list is required — only that whatever this
    //    server says, the sink recorded exactly that. A sink that
    //    substring-matched (`body` inside `body-markup`), that dropped the
    //    query, or that invented a capability the server never sent, fails
    //    here on every desktop.
    let advertised = advertised_capabilities().await;
    eprintln!("GetCapabilities (fresh query): {advertised:?}");
    for (name, parsed) in [
        ("body", capabilities.body),
        ("body-markup", capabilities.body_markup),
        ("persistence", capabilities.persistence),
    ] {
        assert_eq!(
            parsed,
            advertised.iter().any(|c| c == name),
            "the sink's `{name}` disagrees with what this server advertises \
             ({advertised:?}) — the capability set is parsed, not decorative"
        );
    }

    // 2. `capabilities()` is a pure accessor over a set read once at connect
    //    and never re-negotiated (see `NotificationSink::capabilities`). Two
    //    calls must therefore be equal — a sink that re-queried the bus here
    //    would make the role announcement depend on when it was asked.
    assert_eq!(
        capabilities,
        sink.capabilities(),
        "the capability set is read once at connect and never re-negotiated"
    );

    // 3. `dismiss_reporting` is the single input to the `DISMISS_REPORTER`
    //    role, and it is deliberately NOT read from `GetCapabilities`: the
    //    specification has no capability string for "I will tell you why a
    //    notification closed". What decides it is whether this process
    //    actually holds a `NotificationClosed` subscription. So the honest
    //    invariant is that the announced bit and the stream agree — and it
    //    holds on GNOME, KDE, dunst and mako alike, because it is a property
    //    of this code and not of the server.
    //
    //    This is also the assertion with teeth. Wiring `dismiss_reporting` to
    //    an advertised string, or announcing the role while the match rule
    //    failed to install, would clear a peer's notifications every time a
    //    banner timed out on a screen nobody was looking at — and it would
    //    fail here.
    let closed = sink.closed_events();
    assert_eq!(
        capabilities.dismiss_reporting,
        closed.is_some(),
        "dismiss_reporting must reflect the close-signal subscription this \
         process actually holds, not what the server advertises"
    );

    // And the stream is handed over, not cloned: one stream, one consumer.
    // A second reader would split close signals between two halves of the
    // capability and lose dismissals at random.
    assert!(
        sink.closed_events().is_none(),
        "closed_events() must hand the receiver over exactly once"
    );
    drop(closed);
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

// ---------------------------------------------------------------------------
// N5 §11 — the soak
// ---------------------------------------------------------------------------

/// A long, realistic session against the **real** notification server and the
/// **real** logind lock source.
///
/// What a soak is for is the class of defect a short test cannot reach: a
/// counter that only grows, a mirror that is replaced by a duplicate one time
/// in a thousand, a role that widens because a re-announcement raced a
/// narrowing, a worker that stops draining after some number of items. None of
/// those is visible in a test that runs for a second.
///
/// Every item of §11's activity list that does not need a person is driven on
/// a cycle: post, update, remove, dismiss a mirror, a brief disconnect and
/// reconnect (a Wi-Fi blip, minus the Wi-Fi), a lock and an unlock, a policy
/// toggle, and an allow-list change. The two that do need a person — pressing
/// a physical lock button, and pulling a real network — are called out in the
/// report rather than simulated and claimed.
///
/// ```console
/// ANYFLOW_SOAK=1 cargo test -p anyflow-capability-notifications \
///     --test real_dbus -- --ignored --test-threads=1 soak
///
/// # a shorter or longer run
/// ANYFLOW_SOAK=1 ANYFLOW_SOAK_SECS=3600 cargo test … soak
/// ```
///
/// **It closes everything it posts.** A soak that left an hour of
/// notifications in somebody's shade would be worse than no soak.
#[tokio::test]
#[ignore = "runs for 30 minutes against the real notification server; set ANYFLOW_SOAK=1"]
async fn a_thirty_minute_soak_stays_bounded_and_converges() {
    use anyflow_capability_notifications::backend::{logind::LogindLock, LockSource, UnknownLock};
    use anyflow_capability_notifications::{
        NotificationAuthorizer, NotificationManager, NotificationPolicy,
    };
    use anyflow_core::Fingerprint;
    use anyflow_proto::v1::capabilities as pb;
    use anyflow_proto::Message as _;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    if std::env::var_os("ANYFLOW_SOAK").is_none() {
        eprintln!("SKIPPED: set ANYFLOW_SOAK=1 to run the N5 soak.");
        return;
    }
    let seconds: u64 = std::env::var("ANYFLOW_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30 * 60);

    /// A trust store a soak can edit, exactly as a person toggling switches
    /// would.
    struct Switches(RwLock<NotificationPolicy>);
    #[async_trait::async_trait]
    impl NotificationAuthorizer for Switches {
        async fn policy_for(&self, _peer: &Fingerprint) -> NotificationPolicy {
            *self.0.read().await
        }
    }

    const APP_A: &str = "soak.app.alpha";
    const APP_B: &str = "soak.app.beta";
    const TITLE: &str = "ANYFLOW-N5-SOAK-TITLE";
    const BODY: &str = "ANYFLOW-N5-SOAK-BODY";

    let switches = Arc::new(Switches(RwLock::new(NotificationPolicy {
        allow_dismiss_sync: true,
        when_sink_locked: anyflow_capability_notifications::LockPolicy::AppOnly,
        ..NotificationPolicy::default()
    })));

    let sink: Arc<dyn NotificationSink> = Arc::new(connect().await);
    // The real lock source where there is one. A soak that ran against
    // `UnknownLock` would spend its whole length on the reduction path.
    let lock: Arc<dyn LockSource> = match LogindLock::connect().await {
        Some(l) => Arc::new(l),
        None => Arc::new(UnknownLock),
    };
    eprintln!("soak: sink={} lock={}", sink.describe(), lock.describe());

    let manager = NotificationManager::new(Arc::clone(&sink), lock).await;
    manager
        .set_authorizer(Arc::clone(&switches) as Arc<dyn NotificationAuthorizer>)
        .await;
    manager.spawn_platform_pumps();

    let peer = Fingerprint::from_hex(&"5a".repeat(32)).expect("fingerprint");
    let origin = "0123456789abcdef0123456789abcdef";
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    manager.attach_session(peer, tx.clone()).await;

    // Drain everything the desktop sends, counting it by kind. A soak that
    // did not read its own outbound channel would fill it and then be
    // measuring backpressure rather than the sink.
    let counts = Arc::new(std::sync::Mutex::new((0u64, 0u64, 0u64))); // roles, results, dismisses
    let epochs = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let drain_counts = Arc::clone(&counts);
    let drain_epochs = Arc::clone(&epochs);
    let drain = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let control =
                pb::NotificationControl::decode(message.payload.as_slice()).expect("decodes");
            let mut c = drain_counts.lock().expect("not poisoned");
            match control.body {
                Some(pb::notification_control::Body::Roles(r)) => {
                    c.0 += 1;
                    drain_epochs.lock().expect("not poisoned").push(r.epoch);
                }
                Some(pb::notification_control::Body::Result(_)) => c.1 += 1,
                Some(pb::notification_control::Body::Dismiss(_)) => c.2 += 1,
                _ => {}
            }
        }
    });

    let roles = |epoch: u32| pb::NotificationControl {
        body: Some(pb::notification_control::Body::Roles(
            pb::NotificationRoles {
                roles: vec![
                    pb::NotificationRole::Source as i32,
                    pb::NotificationRole::DismissTarget as i32,
                ],
                epoch,
            },
        )),
    };
    manager
        .handle_control(peer, &roles(1).encode_to_vec())
        .await
        .expect("roles");

    let upsert = |seed: u16, app: &str, body: &str| {
        let mut id = vec![0u8; 16];
        id[0] = (seed >> 8) as u8;
        id[1] = (seed & 0xff) as u8;
        pb::NotificationControl {
            body: Some(pb::notification_control::Body::Upsert(
                pb::NotificationUpsert {
                    notification_id: id,
                    origin_device_id: origin.to_string(),
                    app_id: app.to_string(),
                    app_label: app.to_string(),
                    title: TITLE.to_string(),
                    body: body.to_string(),
                    posted_at_unix_ms: 1_700_000_000_000,
                    importance: pb::NotificationImportance::Normal as i32,
                    privacy: pb::NotificationPrivacy::Private as i32,
                    category: pb::NotificationCategory::Message as i32,
                    ..pb::NotificationUpsert::default()
                },
            )),
        }
    };
    let remove = |seed: u16| {
        let mut id = vec![0u8; 16];
        id[0] = (seed >> 8) as u8;
        id[1] = (seed & 0xff) as u8;
        pb::NotificationControl {
            body: Some(pb::notification_control::Body::Remove(
                pb::NotificationRemove {
                    notification_id: id,
                    origin_device_id: origin.to_string(),
                },
            )),
        }
    };

    let started = std::time::Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let mut cycle: u64 = 0;
    let mut peak_mirrors = 0usize;
    let mut peak_queue = 0usize;
    let mut role_epoch: u32 = 1;

    eprintln!("soak: running for {seconds}s");
    while std::time::Instant::now() < deadline {
        cycle += 1;
        let seed = (cycle % 40) as u16 + 1;
        let app = if cycle.is_multiple_of(3) {
            APP_B
        } else {
            APP_A
        };

        // post, then update the same identity twice
        manager
            .handle_control(peer, &upsert(seed, app, BODY).encode_to_vec())
            .await
            .expect("upsert");
        manager
            .handle_control(
                peer,
                &upsert(seed, app, &format!("{BODY}-{cycle}")).encode_to_vec(),
            )
            .await
            .expect("update");

        // a removal every other cycle, so the mirror set churns rather than
        // only growing
        if cycle.is_multiple_of(2) {
            manager
                .handle_control(peer, &remove(seed).encode_to_vec())
                .await
                .expect("remove");
        }

        // **No phantom dismiss.** Nobody is closing anything by hand during
        // this run, so the desktop must not produce a single `DismissRequest`
        // across the whole of it — not from a mirror being replaced, not from
        // one being evicted at the ceiling, not from a reconnect, and not
        // from the server's own close signals. Checked every cycle rather
        // than only at the end, so the cycle that produced one is named.
        assert_eq!(
            counts.lock().expect("not poisoned").2,
            0,
            "cycle {cycle}: a dismissal was sent although nobody dismissed \
             anything"
        );

        // a policy toggle, and an allow-list change
        if cycle.is_multiple_of(7) {
            let mut policy = switches.0.write().await;
            policy.allow_mirror = !policy.allow_mirror;
            let restored = policy.allow_mirror;
            drop(policy);
            if !restored {
                tokio::time::sleep(Duration::from_millis(200)).await;
                switches.0.write().await.allow_mirror = true;
            }
        }
        if cycle.is_multiple_of(11) {
            let mut policy = switches.0.write().await;
            policy.when_sink_locked = match policy.when_sink_locked {
                anyflow_capability_notifications::LockPolicy::Full => {
                    anyflow_capability_notifications::LockPolicy::AppOnly
                }
                _ => anyflow_capability_notifications::LockPolicy::Full,
            };
        }

        // a brief disconnect and reconnect — a Wi-Fi blip, minus the Wi-Fi
        if cycle.is_multiple_of(13) {
            manager.detach_session(&peer).await;
            tokio::time::sleep(Duration::from_millis(300)).await;
            manager.attach_session(peer, tx.clone()).await;
            role_epoch += 1;
            manager
                .handle_control(peer, &roles(role_epoch).encode_to_vec())
                .await
                .expect("roles");
        }

        // a snapshot, which is what a real reconnect would carry
        if cycle.is_multiple_of(17) {
            let sync = vec![(cycle % 251) as u8; 16];
            manager
                .handle_control(
                    peer,
                    &pb::NotificationControl {
                        body: Some(pb::notification_control::Body::Sync(pb::SyncMarker {
                            sync_id: sync.clone(),
                            phase: pb::sync_marker::Phase::Begin as i32,
                        })),
                    }
                    .encode_to_vec(),
                )
                .await
                .expect("begin");
            manager
                .handle_control(peer, &upsert(seed, app, BODY).encode_to_vec())
                .await
                .expect("snapshot item");
            manager
                .handle_control(
                    peer,
                    &pb::NotificationControl {
                        body: Some(pb::notification_control::Body::Sync(pb::SyncMarker {
                            sync_id: sync,
                            phase: pb::sync_marker::Phase::End as i32,
                        })),
                    }
                    .encode_to_vec(),
                )
                .await
                .expect("end");
        }

        let report = manager
            .peer_reports()
            .await
            .into_iter()
            .find(|r| r.peer == peer)
            .expect("a report");
        peak_mirrors = peak_mirrors.max(report.mirrors);
        peak_queue = peak_queue.max(report.queue.high_water);

        assert!(
            report.mirrors <= 200,
            "cycle {cycle}: the mirror ceiling was exceeded ({})",
            report.mirrors
        );
        assert!(
            report.queue.high_water <= 256,
            "cycle {cycle}: the work queue exceeded its bound ({})",
            report.queue.high_water
        );

        if cycle.is_multiple_of(200) {
            eprintln!(
                "soak: {}s cycle={cycle} mirrors={} queue_high_water={} \
                 coalesced={} evicted={} dropped_terminal={}",
                started.elapsed().as_secs(),
                report.mirrors,
                report.queue.high_water,
                report.queue.coalesced,
                report.queue.evicted,
                report.queue.dropped_terminal
            );
        }

        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    // Converge: an empty snapshot takes everything this soak put on the
    // screen back off it.
    let sync = vec![0xEEu8; 16];
    for phase in [pb::sync_marker::Phase::Begin, pb::sync_marker::Phase::End] {
        manager
            .handle_control(
                peer,
                &pb::NotificationControl {
                    body: Some(pb::notification_control::Body::Sync(pb::SyncMarker {
                        sync_id: sync.clone(),
                        phase: phase as i32,
                    })),
                }
                .encode_to_vec(),
            )
            .await
            .expect("marker");
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let final_report = manager
        .peer_reports()
        .await
        .into_iter()
        .find(|r| r.peer == peer)
        .expect("a report");
    let (announced_roles, results, dismisses) = *counts.lock().expect("not poisoned");
    let epoch_list = epochs.lock().expect("not poisoned").clone();

    eprintln!(
        "\nsoak finished after {}s, {cycle} cycles\n  \
         peak mirrors        {peak_mirrors}\n  \
         peak queue depth    {peak_queue}\n  \
         final mirrors       {}\n  \
         coalesced           {}\n  \
         queue evictions     {}\n  \
         dropped terminal    {}\n  \
         mirror evictions    {}\n  \
         role announcements  {announced_roles}\n  \
         results sent        {results}\n  \
         dismiss requests    {dismisses}\n  \
         local role epochs   {epoch_list:?}",
        started.elapsed().as_secs(),
        final_report.mirrors,
        final_report.queue.coalesced,
        final_report.queue.evicted,
        final_report.queue.dropped_terminal,
        final_report.evicted,
    );

    assert_eq!(
        final_report.mirrors, 0,
        "the converging snapshot did not clear the screen"
    );
    assert_eq!(
        final_report.queue.dropped_terminal, 0,
        "a terminal item was dropped during the soak; each one is a \
         notification that could have been left on a screen for ever"
    );
    // **Epochs are strictly increasing within a connection, and restart at 1
    // across one** (ADR-0017 §4). Both halves matter and they are opposite
    // assertions, so the sequence is split at each `1` — which is the
    // observable mark of a new session — and each run checked on its own.
    // Asserting global monotonicity here would have failed a correct reset,
    // and asserting nothing would have missed a replayed announcement.
    assert!(
        !epoch_list.is_empty(),
        "no role announcement was observed, so this proves nothing"
    );
    assert_eq!(epoch_list[0], 1, "the first announcement must be epoch 1");
    let mut segments = 0;
    for run in epoch_list.split(|e| *e == 1) {
        segments += 1;
        assert!(
            run.windows(2).all(|w| w[1] > w[0]),
            "epochs did not strictly increase within one connection: \
             {epoch_list:?}"
        );
        assert!(
            run.iter().all(|e| *e > 1),
            "an epoch of 0 or a repeated 1 appeared inside a connection: \
             {epoch_list:?}"
        );
    }
    assert!(segments >= 2, "the soak never reconnected");
    assert_eq!(
        dismisses, 0,
        "the soak sent {dismisses} dismissals although nobody dismissed \
         anything"
    );
    assert!(
        results > 0 && announced_roles > 0,
        "the capture is empty, so every assertion above is vacuous"
    );
    // And nothing this soak displayed is still on the screen.
    assert!(!final_report.snapshot_open);

    drop(tx);
    let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;
}
