//! The notifications.v1 log-privacy canary, in a test binary of its own.
//!
//! # Why this is not in `notifications.rs`
//!
//! It used to be, and it failed intermittently inside `mock` — never on a
//! developer's machine — with `nothing was captured, so this test proves
//! nothing`. The cause is a `tracing` property that is easy to walk into and
//! very hard to see:
//!
//! 1. `tracing::subscriber::set_default` installs a subscriber on **one
//!    thread**, but it raises the process-wide maximum level so that callsites
//!    start being evaluated **everywhere**.
//! 2. `tracing` caches an `Interest` per callsite, once, the first time it is
//!    reached. The interest is computed from the **registering thread's**
//!    default subscriber.
//! 3. So a test running in parallel on another thread, with no subscriber of
//!    its own, can be the first to reach one of the daemon's callsites. It
//!    registers as `NoSubscriber`, whose `enabled()` is `false`, and the
//!    callsite is cached as `Interest::never()` — **globally, for the rest of
//!    the process**.
//! 4. The capturing test then runs its body and records nothing, because the
//!    callsites it needed were switched off by a thread that was not even
//!    looking at them.
//!
//! MEASURED with a standalone reproduction: eight subscriber-less threads
//! touching one shared callsite produced an empty capture on 3 runs out of 3.
//! `tracing::callsite::rebuild_interest_cache()` after `set_default` narrows
//! the window but does not close it — it still failed 1 run in 3, because
//! callsites registered *after* the rebuild are cached the same way.
//!
//! # The fix is isolation, not a tracing trick
//!
//! The race needs a concurrently running thread that has no subscriber. Every
//! test in this binary installs one, so there is no such thread and the race
//! cannot occur. That is the same reason
//! `capabilities/{clipboard,notifications}/tests/logging.rs` have never shown
//! this: every test in each of those binaries goes through one `capture()`
//! helper.
//!
//! **Keep it that way.** A test added here that does not install a capture
//! subscriber reintroduces the bug, and it reintroduces it as a
//! *log-privacy assertion that passes while proving nothing* — which is worse
//! than one that fails.

mod common;

use std::time::Duration;

use common::*;
use omnibridge_capability_notifications::CAPABILITY_ID;

const TIMEOUT: Duration = Duration::from_secs(5);

/// **NOTIF-SEC-25, extended to the N5 convergence path.**
///
/// The mid-session grant correction added log lines on a path that runs while
/// a notification is on the screen: the grant handler, the renegotiation
/// decision, the session shutdown, the reattach and the reconnect snapshot.
/// None of them may carry a title, a body, an application label or an
/// application id — and "may not" is proved here rather than grepped, at
/// `TRACE`, because `RUST_LOG=trace` is exactly what somebody runs when
/// something is wrong and exactly the worst moment to spill a stranger's
/// message into a file they are about to attach to a bug report.
#[tokio::test]
async fn the_mid_session_convergence_path_logs_no_notification_content() {
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    const TITLE: &str = "CANARY-N5-CONVERGE-TITLE-9b2f";
    const BODY: &str = "CANARY-N5-CONVERGE-BODY-4e71";
    const APP_LABEL: &str = "CANARY-N5-CONVERGE-APPLABEL-0a5c";
    const APP_ID: &str = "canary.n5.converge.d31f";

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
        // Scoped rather than global: `set_global_default` may be called only
        // once per process and the rest of this suite must stay unaffected.
        let _guard = tracing::subscriber::set_default(subscriber);

        // A probe, so that a failure below can say *which* thing broke.
        //
        // If this is captured, the subscriber is installed and working on this
        // thread, and an empty capture afterwards means the daemon's callsites
        // were disabled — which is the failure this whole file is arranged to
        // prevent. If it is not captured, the subscriber itself never took
        // effect. The two have completely different causes and the bare
        // "nothing was captured" could not tell them apart.
        //
        // It is cleared immediately: leaving those bytes in would make
        // `text.is_empty()` impossible and hide the very failure it explains.
        tracing::error!("CAPTURE-PROBE");
        probe_seen = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned"))
            .contains("CAPTURE-PROBE");
        captured.0.lock().expect("not poisoned").clear();

        let server = TestServer::start().await;
        let (client, notifications) = TestClient::new_raw_notifications("phone");
        let token = server.open_pairing(Duration::from_secs(30)).await;
        let session = client
            .connect(server.addr, server.fingerprint, Some(&token))
            .await
            .expect("pairing");

        // Grant first, reconnect, and put a canary on the screen.
        // Waited for, so the grant genuinely lands on a live session rather
        // than on a registry that has not caught up — otherwise the decision
        // would be `no-session` and this would canary the wrong path.
        let negotiated = desktop_negotiated(&server, client.fingerprint).await;
        assert!(!negotiated.contains(&CAPABILITY_ID.to_string()));

        let response = omnibridge_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            true,
        )
        .await;
        match response {
            omnibridge_runtime::control::Response::Ok { message } => assert!(
                message.contains("reconnecting"),
                "the convergence path was not taken, so this canaries nothing"
            ),
            other => panic!("{other:?}"),
        }
        let _ = tokio::time::timeout(TIMEOUT, session.task).await;

        let session = client
            .connect(server.addr, server.fingerprint, None)
            .await
            .expect("reconnect");
        notifications.next_roles(TIMEOUT).await;
        assert!(
            send_notification_control(
                &session,
                roles_body(&[clip_pb::NotificationRole::Source], 1)
            )
            .await
        );

        let mut message = upsert(1, TITLE, BODY);
        message.app_label = APP_LABEL.to_string();
        message.app_id = APP_ID.to_string();
        assert!(
            send_notification_control(
                &session,
                clip_pb::notification_control::Body::Upsert(message)
            )
            .await
        );
        assert_eq!(
            notifications.next_result(TIMEOUT).await.outcome,
            clip_pb::NotificationOutcome::Displayed as i32
        );

        // Now withdraw and re-grant with that notification live, so the
        // revocation, the mirror close and a second renegotiation decision
        // all run while there is content to leak.
        omnibridge_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            false,
        )
        .await;
        omnibridge_runtime::server::do_grant(
            &server.state,
            &client.fingerprint.to_hex(),
            CAPABILITY_ID,
            true,
        )
        .await;
        let _ = tokio::time::timeout(TIMEOUT, session.task).await;
    }

    let text = String::from_utf8_lossy(&captured.0.lock().expect("not poisoned")).into_owned();
    assert!(
        !text.is_empty(),
        "nothing was captured, so this test proves nothing. \
probe_seen={probe_seen}: if true, the subscriber worked and the daemon's \
callsites were disabled by a subscriber-less thread registering them first — \
see this file's header. If false, the subscriber never took effect at all."
    );
    // Coverage is proved two ways, because either alone can lie. The
    // response above proves the convergence path actually ran; this proves
    // the subscriber was attached to the daemon's own events while it did,
    // rather than to an empty session that would make every assertion below
    // vacuously true.
    assert!(
        text.contains("omnibridge_"),
        "no daemon event reached the capture, so this test proves nothing:\n{text}"
    );
    for canary in [TITLE, BODY, APP_LABEL, APP_ID] {
        assert!(
            !text.contains(canary),
            "the convergence path logged {canary}:\n{text}"
        );
    }
}
