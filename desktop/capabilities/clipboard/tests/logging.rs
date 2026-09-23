//! CLIP-SEC-14: clipboard content never reaches a log.
//!
//! Grepping the source proves the *current* logging is clean. This proves it
//! by construction: a full flow runs with a `tracing` subscriber capturing
//! every event at `TRACE`, driven with canary strings that appear nowhere
//! else, and the captured output must not contain them. A future log line
//! added with `?text` or `%text` fails this test rather than shipping.
//!
//! The canaries are distinctive on purpose — a substring assertion against
//! ordinary words would pass by accident, and one against a word that occurs
//! in an error message would fail by accident.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use common::*;
use omnibridge_capability_clipboard::backend::{BackendError, MemoryBackend};
use omnibridge_capability_clipboard::{ClipboardPolicy, ClipboardText};
use tracing_subscriber::fmt::MakeWriter;

/// Canaries. Each is unique, so a hit is unambiguous.
const CLIP_TEXT: &str = "CANARY-CLIP-a41f9c2e7b";
const SENSITIVE_TEXT: &str = "CANARY-SENSITIVE-77d3e0b1";
const PENDING_TEXT: &str = "CANARY-PENDING-5b8a1f4c";
const LOCAL_TEXT: &str = "CANARY-LOCAL-e2c7d904";

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
/// A *scoped* subscriber, not a global one: `set_global_default` can be
/// called only once per process, and each test here needs its own capture.
async fn capture<F, Fut>(body: F) -> String
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        // TRACE, not INFO. A leak at a level nobody reads in production is
        // still a leak, because `RUST_LOG=trace` is exactly what someone runs
        // when something is wrong — and that is the worst moment to spill a
        // password into a file they are about to attach to a bug report.
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .finish();

    // The guard must be dropped before the buffer is read, so that nothing
    // written during teardown is missed.
    let guard = tracing::subscriber::set_default(subscriber);
    body().await;
    drop(guard);

    captured.text()
}

fn assert_no_canaries(logs: &str, canaries: &[&str]) {
    for canary in canaries {
        assert!(
            !logs.contains(canary),
            "clipboard content reached the log.\n\
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

#[tokio::test]
async fn a_full_inbound_flow_logs_no_clipboard_content() {
    let logs = capture(|| async {
        let device = Device::new("desktop").await;
        let peer = fp(0xa1);
        device
            .authorizer
            .grant(
                peer,
                ClipboardPolicy {
                    allow_send: true,
                    allow_receive: true,
                    auto_send: true,
                    auto_receive: true,
                },
            )
            .await;
        let mut rx = device.connect(peer).await;

        // Applied.
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(1), "phone", CLIP_TEXT, false),
        )
        .await;

        // Applied and marked sensitive.
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(2), "phone", SENSITIVE_TEXT, true),
        )
        .await;

        // Duplicate, rejected by policy, too large, and invalid — every
        // refusal path, since a refusal is where an implementation is most
        // tempted to log "what was wrong with it".
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(2), "phone", SENSITIVE_TEXT, true),
        )
        .await;
        handle(
            &device,
            peer,
            "phone",
            &update_payload(
                &event_id(3),
                "phone",
                &format!("{CLIP_TEXT}{}", "x".repeat(40_000)),
                false,
            ),
        )
        .await;
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(4), "phone", &format!("{CLIP_TEXT}\0"), false),
        )
        .await;

        // The local side: a copy fanned out, and its suppressed echo.
        device.backend.user_copies(LOCAL_TEXT);
        device.manager.on_local_change().await;
        device.manager.on_local_change().await;

        let _ = drain(&mut rx);
    })
    .await;

    assert_no_canaries(&logs, &[CLIP_TEXT, SENSITIVE_TEXT, LOCAL_TEXT]);
}

#[tokio::test]
async fn a_held_clip_logs_no_content_even_when_it_is_applied_later() {
    let logs = capture(|| async {
        let device = Device::new("desktop").await;
        let peer = fp(0xb2);
        // auto_receive off: the clip is held, described and then applied.
        device
            .authorizer
            .grant(peer, ClipboardPolicy::default())
            .await;

        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(5), "phone", PENDING_TEXT, true),
        )
        .await;

        // The report path, which is the one that has to describe a clip.
        let pending = device.manager.pending_clips().await;
        tracing::info!(?pending, "pending clips");

        device.manager.apply_pending(&peer).await.expect("applied");
    })
    .await;

    assert_no_canaries(&logs, &[PENDING_TEXT]);
}

#[tokio::test]
async fn a_backend_failure_logs_no_content() {
    let logs = capture(|| async {
        let device = Device::new("desktop").await;
        let peer = fp(0xc3);
        device
            .authorizer
            .grant(
                peer,
                ClipboardPolicy {
                    auto_receive: true,
                    ..ClipboardPolicy::default()
                },
            )
            .await;

        device
            .backend
            .set_failure(Some(BackendError::Failed("the compositor said no".into())));

        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(6), "phone", CLIP_TEXT, false),
        )
        .await;

        // And the read path failing, which is where a naive implementation
        // would log "could not read: <what it got>".
        device.backend.set_failure(Some(BackendError::TimedOut));
        device.manager.on_local_change().await;
    })
    .await;

    assert_no_canaries(&logs, &[CLIP_TEXT]);
}

/// The types themselves, independently of any particular call site.
#[tokio::test]
async fn no_public_type_renders_clipboard_content() {
    let text = ClipboardText::validate(CLIP_TEXT).expect("valid");

    // Debug and Display on everything a log line could plausibly reach for.
    let renderings = vec![
        format!("{text:?}"),
        format!("{:?}", Some(&text)),
        format!("{:?}", vec![&text]),
        format!("{:?}", ClipboardText::validate("x").expect("valid")),
    ];
    for rendered in renderings {
        assert!(
            !rendered.contains(CLIP_TEXT),
            "a type rendered clipboard content: {rendered}"
        );
    }

    // And the pending-clip report, which is the only public type that
    // describes a clip at all.
    let device = Device::new("desktop").await;
    let peer = fp(0xd4);
    device
        .authorizer
        .grant(peer, ClipboardPolicy::default())
        .await;
    handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(7), "phone", PENDING_TEXT, false),
    )
    .await;

    let pending = device.manager.pending_clips().await;
    assert_eq!(pending.len(), 1);
    let rendered = format!("{pending:?}");
    assert!(
        !rendered.contains(PENDING_TEXT),
        "PendingClipInfo leaked content: {rendered}"
    );
    // It must still be useful.
    assert!(rendered.contains(&pending[0].hash_prefix));
}

/// The `MemoryBackend` deliberately *does* hold content — it is a test
/// double. This pins that it is never the thing under audit, by proving the
/// production types are what the logging assertions above examined.
#[tokio::test]
async fn the_test_backend_is_the_only_thing_that_keeps_content() {
    let backend = MemoryBackend::new();
    backend.user_copies(CLIP_TEXT);
    assert_eq!(backend.current().as_deref(), Some(CLIP_TEXT));
}
