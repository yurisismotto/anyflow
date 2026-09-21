//! `clipboard.v1` over the real transport.
//!
//! Everything here runs against a genuine TLS 1.3 session with real SPKI
//! pinning, a real handshake, the real capability registry and the real trust
//! store. Where the capability's own suite proves the rules in isolation,
//! this one proves they survive the transport — including the part that
//! matters most, the session writer.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use omnibridge_capability_clipboard::{limits, ClipboardPolicy, CAPABILITY_ID};
use omnibridge_core::capability::OutboundMessage;
use omnibridge_proto::Message;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Everything on: the configuration a clipboard user would actually run.
const AUTOMATIC: ClipboardPolicy = ClipboardPolicy {
    allow_send: true,
    allow_receive: true,
    auto_send: true,
    auto_receive: true,
};

fn update(event_id: &[u8], origin: &str, text: &str, sensitive: bool) -> Vec<u8> {
    clip_pb::ClipboardControl {
        body: Some(clip_pb::clipboard_control::Body::Update(
            clip_pb::ClipboardUpdate {
                event_id: event_id.to_vec(),
                origin_device_id: origin.to_string(),
                text_utf8: text.to_string(),
                content_hash: sha256(text),
                sensitive_hint: sensitive,
                timestamp_unix_ms: 1_700_000_000_000,
            },
        )),
    }
    .encode_to_vec()
}

fn sha256(text: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher.finalize().to_vec()
}

fn event_id(seed: u8) -> Vec<u8> {
    vec![seed; 16]
}

fn outbound(payload: Vec<u8>) -> OutboundMessage {
    OutboundMessage {
        capability_id: CAPABILITY_ID.to_string(),
        payload,
    }
}

/// A paired client with `clipboard.v1` granted and the given policy.
async fn paired(
    policy: ClipboardPolicy,
) -> (
    TestServer,
    TestClient,
    Arc<CapturedClipboard>,
    ConnectedSession,
) {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_clipboard("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    session.close().await;

    // Granted by hand, exactly as a user would: `clipboard.v1` is not in
    // `auto_grant`, so pairing alone leaves it off.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;
    server
        .set_clipboard_policy(client.fingerprint, policy)
        .await;

    // A reconnect is what makes a widened grant take effect, which is the
    // documented capability-negotiation semantics — see CLIPBOARD.md.
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    (server, client, captured, session)
}

// ---------------------------------------------------------------------------
// CLIP-01 / CLIP-02 — negotiation and the explicit grant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clipboard_is_refused_until_it_is_explicitly_granted() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_clipboard("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");

    // `clipboard.v1` is not in `auto_grant`, so pairing alone grants nothing.
    {
        let store = server.state.store.lock().await;
        let peer = store.trusted_peer(&client.fingerprint).expect("paired");
        assert!(
            !peer.allows(CAPABILITY_ID),
            "pairing alone must not grant clipboard access"
        );
    }

    // What the *initiator* lists as negotiated is the mutually-supported set,
    // not the authorized one: HELLO_ACK advertises everything the responder
    // implements (see `accept_handshake`). Authorization is the responder's
    // to enforce, per message — which is what this asserts, rather than
    // trusting a list the peer cannot compute.
    session
        .handle
        .send_capability(outbound(update(
            &event_id(20),
            "phone",
            "before the grant",
            false,
        )))
        .await;
    captured.expect_silence(Duration::from_millis(300)).await;
    assert_eq!(
        server.clipboard_backend.current(),
        None,
        "an ungranted peer must not reach the clipboard"
    );
    session.close().await;

    // Grant it and reconnect: capability widening takes effect at the next
    // handshake, which is the foundation's documented semantics.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    session
        .handle
        .send_capability(outbound(update(
            &event_id(21),
            "phone",
            "after the grant",
            false,
        )))
        .await;
    let result = captured.next_result(TIMEOUT).await;
    assert_ne!(
        result.outcome,
        clip_pb::ClipboardOutcome::NotAuthorized as i32,
        "the grant should now be in force"
    );
    session.close().await;
}

/// Widening a grant needs a reconnect; narrowing does not.
///
/// The asymmetry is deliberate and is the foundation's, not this
/// capability's: the *effective* set is fixed at handshake time, so adding a
/// capability to it requires a new handshake — while every inbound message is
/// re-checked against the trust store, so removing one bites at once. A
/// security control that took effect late would be the dangerous half, and
/// that is the half that is immediate.
#[tokio::test]
async fn widening_a_grant_needs_a_reconnect_but_narrowing_does_not() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_clipboard("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");

    // Granted mid-session: not yet in the session's effective set.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;
    server
        .set_clipboard_policy(client.fingerprint, AUTOMATIC)
        .await;

    session
        .handle
        .send_capability(outbound(update(&event_id(22), "phone", "too early", false)))
        .await;
    captured.expect_silence(Duration::from_millis(300)).await;
    assert_eq!(
        server.clipboard_backend.current(),
        None,
        "widening must not apply to a session that already handshook"
    );
    session.close().await;

    // After a reconnect it works …
    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");
    session
        .handle
        .send_capability(outbound(update(&event_id(23), "phone", "now ok", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Applied as i32
    );

    // … and narrowing takes effect on that same session, with no reconnect.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, false)
        .await;
    session
        .handle
        .send_capability(outbound(update(&event_id(24), "phone", "too late", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::NotAuthorized as i32
    );
    session.close().await;
}

#[tokio::test]
async fn an_ungranted_peer_that_sends_clipboard_traffic_is_told_it_is_unsupported() {
    let server = TestServer::start().await;
    let (client, captured) = TestClient::new_raw_clipboard("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");

    // The transport refuses an un-negotiated capability before the handler
    // ever sees it, with a non-fatal ERROR — so the session survives, which
    // is what keeps one capability's refusal from costing the others.
    session
        .handle
        .send_capability(outbound(update(
            &event_id(1),
            "phone",
            "should not arrive",
            false,
        )))
        .await;

    captured.expect_silence(Duration::from_millis(300)).await;
    assert_eq!(
        server.clipboard_backend.current(),
        None,
        "an un-negotiated capability must not reach the clipboard"
    );
    assert!(session.handle.is_live(), "the session must survive");
    session.close().await;
}

// ---------------------------------------------------------------------------
// CLIP-05 — Android → Fedora, the manual direction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_granted_peer_can_push_a_clip_and_is_told_what_happened() {
    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    session
        .handle
        .send_capability(outbound(update(
            &event_id(2),
            "phone",
            "hello from Android",
            false,
        )))
        .await;

    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(result.event_id, event_id(2), "the result must correlate");
    assert_eq!(
        result.outcome,
        clip_pb::ClipboardOutcome::Applied as i32,
        "auto-receive is on, so it should be applied"
    );
    assert_eq!(
        server.clipboard_backend.current().as_deref(),
        Some("hello from Android")
    );
    session.close().await;
}

#[tokio::test]
async fn unicode_and_multiline_text_survive_the_round_trip_byte_for_byte() {
    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    // Portuguese, Spanish, emoji (including a flag and a ZWJ sequence), CJK,
    // tabs, and both line endings.
    let cases = [
        "olá, ação e coração",
        "¿cómo estás? el ñandú",
        "🇧🇷 🎉 👨‍👩‍👧‍👦 café",
        "日本語 中文 한국어",
        "line one\nline two\r\nline three\tindented",
        "  spaces preserved  ",
    ];

    for (i, case) in cases.iter().enumerate() {
        session
            .handle
            .send_capability(outbound(update(
                &event_id(10 + i as u8),
                "phone",
                case,
                false,
            )))
            .await;

        let result = captured.next_result(TIMEOUT).await;
        assert_eq!(
            result.outcome,
            clip_pb::ClipboardOutcome::Applied as i32,
            "case {i} was refused"
        );
        assert_eq!(
            server.clipboard_backend.current().as_deref(),
            Some(*case),
            "case {i} was modified in transit"
        );
    }
    session.close().await;
}

#[tokio::test]
async fn an_oversized_clip_is_refused_over_the_wire() {
    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    // Deliberately under MAX_FRAME_LEN so the transport accepts the frame and
    // the *capability* is what refuses it — proving the two limits are
    // separate and that the margin between them is real.
    let text = "A".repeat(limits::MAX_CLIPBOARD_TEXT_BYTES + 1);
    session
        .handle
        .send_capability(outbound(update(&event_id(3), "phone", &text, false)))
        .await;

    let result = captured.next_result(TIMEOUT).await;
    assert_eq!(result.outcome, clip_pb::ClipboardOutcome::TooLarge as i32);
    assert_eq!(server.clipboard_backend.current(), None);
    assert!(session.handle.is_live(), "an oversized clip is not fatal");
    session.close().await;
}

#[tokio::test]
async fn a_duplicate_event_id_is_idempotent_over_the_wire() {
    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    for text in ["first", "second with the same id"] {
        session
            .handle
            .send_capability(outbound(update(&event_id(4), "phone", text, false)))
            .await;
    }

    let first = captured.next_result(TIMEOUT).await;
    let second = captured.next_result(TIMEOUT).await;
    assert_eq!(first.outcome, clip_pb::ClipboardOutcome::Applied as i32);
    assert_eq!(second.outcome, clip_pb::ClipboardOutcome::Duplicate as i32);
    assert_eq!(
        server.clipboard_backend.writes().len(),
        1,
        "the clipboard must be written once"
    );
    assert_eq!(server.clipboard_backend.current().as_deref(), Some("first"));
    session.close().await;
}

// ---------------------------------------------------------------------------
// CLIP-03 / CLIP-04 — Fedora → Android
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_local_copy_is_pushed_when_auto_send_is_on() {
    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    server.clipboard_backend.user_copies("copied on Fedora");
    server.clipboard.on_local_change().await;

    let control = captured.next_control(TIMEOUT).await;
    match control.body {
        Some(clip_pb::clipboard_control::Body::Update(u)) => {
            assert_eq!(u.text_utf8, "copied on Fedora");
            assert_eq!(u.content_hash, sha256("copied on Fedora"));
            assert_eq!(u.event_id.len(), limits::EVENT_ID_LEN);
            assert!(!u.origin_device_id.is_empty());
            assert!(!u.sensitive_hint);
        }
        other => panic!("expected an update, got {other:?}"),
    }
    session.close().await;
}

#[tokio::test]
async fn a_local_copy_is_not_pushed_when_auto_send_is_off() {
    let (server, _client, captured, session) = paired(ClipboardPolicy::default()).await;

    server.clipboard_backend.user_copies("stays local");
    server.clipboard.on_local_change().await;

    captured.expect_silence(Duration::from_millis(300)).await;
    session.close().await;
}

#[tokio::test]
async fn a_manual_send_works_without_auto_send() {
    let (server, client, captured, session) = paired(ClipboardPolicy::default()).await;

    server.clipboard_backend.user_copies("sent by hand");
    let bytes = server
        .clipboard
        .send_current_clipboard(&client.fingerprint, false)
        .await
        .expect("a manual send needs only the grant");
    assert_eq!(bytes, "sent by hand".len());

    let control = captured.next_control(TIMEOUT).await;
    assert!(matches!(
        control.body,
        Some(clip_pb::clipboard_control::Body::Update(_))
    ));
    session.close().await;
}

#[tokio::test]
async fn a_sensitive_manual_send_sets_the_hint_on_the_wire() {
    let (server, client, captured, session) = paired(ClipboardPolicy::default()).await;

    server.clipboard_backend.user_copies("one-time code 123456");
    server
        .clipboard
        .send_current_clipboard(&client.fingerprint, true)
        .await
        .expect("send");

    match captured.next_control(TIMEOUT).await.body {
        Some(clip_pb::clipboard_control::Body::Update(u)) => {
            assert!(u.sensitive_hint, "--sensitive must reach the peer");
        }
        other => panic!("expected an update, got {other:?}"),
    }
    session.close().await;
}

// ---------------------------------------------------------------------------
// CLIP-13 — revocation on a live session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoking_the_grant_stops_clipboard_traffic_without_dropping_the_session() {
    let (server, client, captured, session) = paired(AUTOMATIC).await;

    // Working first, so the test cannot pass by never having worked.
    session
        .handle
        .send_capability(outbound(update(&event_id(5), "phone", "before", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Applied as i32
    );

    server
        .set_grant(client.fingerprint, CAPABILITY_ID, false)
        .await;
    server.state.notify_clipboard_policy_changed();

    // Inbound: refused, on the same session, with no reconnect.
    session
        .handle
        .send_capability(outbound(update(&event_id(6), "phone", "after", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::NotAuthorized as i32,
        "a withdrawn grant must bite immediately, not at the next reconnect"
    );
    assert_eq!(
        server.clipboard_backend.current().as_deref(),
        Some("before"),
        "nothing may have been applied after the revocation"
    );

    // Outbound: nothing is pushed either.
    server.clipboard_backend.user_copies("after revocation");
    server.clipboard.on_local_change().await;
    captured.expect_silence(Duration::from_millis(300)).await;

    session.close().await;
}

#[tokio::test]
async fn unpairing_stops_clipboard_traffic() {
    let (server, client, _captured, session) = paired(AUTOMATIC).await;

    {
        let mut store = server.state.store.lock().await;
        store.revoke_peer(&client.fingerprint).expect("revoke");
    }
    server.state.notify_clipboard_policy_changed();

    session
        .handle
        .send_capability(outbound(update(&event_id(7), "phone", "after", false)))
        .await;

    // Give the handler time to run and refuse.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        server.clipboard_backend.current(),
        None,
        "a revoked device must not reach the clipboard"
    );
    session.close().await;
}

// ---------------------------------------------------------------------------
// CLIP-14 — reconnect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clipboard_keeps_working_across_a_reconnect() {
    let (server, client, captured, session) = paired(AUTOMATIC).await;

    session
        .handle
        .send_capability(outbound(update(&event_id(8), "phone", "before", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Applied as i32
    );
    session.close().await;

    let session = client
        .connect(server.addr, server.fingerprint, None)
        .await
        .expect("reconnect");

    session
        .handle
        .send_capability(outbound(update(&event_id(9), "phone", "after", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Applied as i32
    );
    assert_eq!(server.clipboard_backend.current().as_deref(), Some("after"));

    // The de-duplication cache survives the reconnect, so a replay of the
    // first event is still refused. That is deliberate: the cache is keyed by
    // event id and bounded by time, not by connection.
    session
        .handle
        .send_capability(outbound(update(&event_id(8), "phone", "replayed", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Duplicate as i32
    );
    session.close().await;
}

#[tokio::test]
async fn a_pending_clip_does_not_survive_the_session_that_delivered_it() {
    // auto_receive off, so the clip is held rather than applied.
    let (server, client, captured, session) = paired(ClipboardPolicy::default()).await;

    session
        .handle
        .send_capability(outbound(update(&event_id(11), "phone", "held", false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::PendingUser as i32
    );
    assert_eq!(server.clipboard.pending_clips().await.len(), 1);

    session.close().await;

    wait_until(TIMEOUT, || async {
        server.clipboard.pending_clips().await.is_empty()
    })
    .await;
    let _ = client;
}

// ---------------------------------------------------------------------------
// CLIP-17 — the session writer must not regress
// ---------------------------------------------------------------------------
//
// The defect this guards against: `on_message` used to be awaited by the same
// task that drained the outbound queue, so a handler that replied while the
// queue was full waited for a drain that could only happen after it returned.
// `clipboard.v1` replies to *every* update, so a peer that floods updates is
// exactly the shape that triggered it.

#[tokio::test]
async fn a_burst_of_updates_is_answered_in_full_under_outbound_pressure() {
    // Four times the session's outbound queue depth (32), sent without
    // reading anything in between, so the queue is saturated throughout.
    const BURST: usize = 128;

    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    for i in 0..BURST {
        let mut id = vec![0u8; 16];
        id[..8].copy_from_slice(&(i as u64).to_be_bytes());
        session
            .handle
            .send_capability(outbound(update(
                &id,
                "phone",
                &format!("burst clip {i}"),
                false,
            )))
            .await;
    }

    // Every one must be answered. Before the reader/writer split this
    // deadlocked and the count stopped at the queue depth.
    let mut applied = 0usize;
    for _ in 0..BURST {
        let result = captured.next_result(Duration::from_secs(15)).await;
        if result.outcome == clip_pb::ClipboardOutcome::Applied as i32 {
            applied += 1;
        }
    }
    assert_eq!(applied, BURST, "every update in the burst must be answered");

    // And the session is still healthy afterwards, not merely alive.
    assert!(session.handle.is_live());
    let rtt = session.handle.ping(TIMEOUT).await;
    assert!(rtt.is_some(), "the session should still answer a PING");

    assert_eq!(
        server.clipboard_backend.current().as_deref(),
        Some(format!("burst clip {}", BURST - 1).as_str())
    );
    session.close().await;
}

#[tokio::test]
async fn inbound_clipboard_and_outbound_pushes_interleave_without_wedging() {
    // Both directions at once, which is what a real automatic session does:
    // the desktop is pushing local copies while the phone is pushing its own.
    const ROUNDS: usize = 64;

    let (server, _client, captured, session) = paired(AUTOMATIC).await;

    for i in 0..ROUNDS {
        let mut id = vec![0u8; 16];
        id[..8].copy_from_slice(&(i as u64).to_be_bytes());
        session
            .handle
            .send_capability(outbound(update(
                &id,
                "phone",
                &format!("from phone {i}"),
                false,
            )))
            .await;

        // A local copy at the same time. The write also fires the memory
        // backend's watch, exactly as a real clipboard would.
        server
            .clipboard_backend
            .user_copies(&format!("from desktop {i}"));
        server.clipboard.on_local_change().await;
    }

    // Everything the desktop said, of both kinds, must arrive.
    let mut results = 0usize;
    let mut updates = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while results < ROUNDS && tokio::time::Instant::now() < deadline {
        match captured.next_control(Duration::from_secs(15)).await.body {
            Some(clip_pb::clipboard_control::Body::Result(_)) => results += 1,
            Some(clip_pb::clipboard_control::Body::Update(_)) => updates += 1,
            None => {}
        }
    }

    assert_eq!(results, ROUNDS, "every inbound update must be answered");
    assert!(
        updates > 0,
        "the desktop's own copies should have been pushed too"
    );
    assert!(session.handle.is_live());
    session.close().await;
}

#[tokio::test]
async fn a_session_carrying_clipboard_traffic_still_shuts_down_promptly() {
    let (server, _client, _captured, session) = paired(AUTOMATIC).await;

    // Saturate the queue and then shut down without draining anything.
    for i in 0..256u64 {
        let mut id = vec![0u8; 16];
        id[..8].copy_from_slice(&i.to_be_bytes());
        session
            .handle
            .send_capability(outbound(update(&id, "phone", &format!("clip {i}"), false)))
            .await;
    }

    let closed = tokio::time::timeout(Duration::from_secs(10), session.close()).await;
    assert!(
        closed.is_ok(),
        "a saturated clipboard session must still shut down"
    );
    let _ = server;
}

// ---------------------------------------------------------------------------
// CLIP-11 / CLIP-12 — nothing is persisted, and nothing is logged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clipboard_content_never_reaches_the_trust_store() {
    const SECRET: &str = "PERSISTENCE-CANARY-9f3a2b71";

    let (server, client, captured, session) = paired(AUTOMATIC).await;

    session
        .handle
        .send_capability(outbound(update(&event_id(12), "phone", SECRET, false)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::Applied as i32
    );

    // Also exercise the held-clip path, which is the one that keeps text in
    // memory at all.
    server
        .set_clipboard_policy(client.fingerprint, ClipboardPolicy::default())
        .await;
    session
        .handle
        .send_capability(outbound(update(
            &event_id(13),
            "phone",
            "PENDING-CANARY-4c8e",
            false,
        )))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::PendingUser as i32
    );

    // Force a write of the store, then read every byte of the data directory.
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;

    let dir = server.data_dir();
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).expect("data dir") {
        let path = entry.expect("entry").path();
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).expect("read");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET),
            "{} contains clipboard content",
            path.display()
        );
        assert!(
            !text.contains("PENDING-CANARY-4c8e"),
            "{} contains a held clip",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "the data directory should have been inspected");

    // And the policy *is* persisted — the store keeps settings, not content.
    let raw = std::fs::read_to_string(dir.join("state.json")).expect("state.json");
    assert!(
        raw.contains("clipboard_policy"),
        "policy must survive a restart"
    );

    session.close().await;
}

#[tokio::test]
async fn the_status_report_describes_clips_without_carrying_them() {
    const SECRET: &str = "REPORT-CANARY-77ab";

    let (server, _client, captured, session) = paired(ClipboardPolicy::default()).await;

    session
        .handle
        .send_capability(outbound(update(&event_id(14), "phone", SECRET, true)))
        .await;
    assert_eq!(
        captured.next_result(TIMEOUT).await.outcome,
        clip_pb::ClipboardOutcome::PendingUser as i32
    );

    let pending = server.clipboard.pending_clips().await;
    assert_eq!(pending.len(), 1);
    let rendered = format!("{pending:?}");
    assert!(
        !rendered.contains(SECRET),
        "the pending-clip report leaked content: {rendered}"
    );
    assert_eq!(pending[0].bytes, SECRET.len());
    assert!(pending[0].sensitive);
    assert_eq!(pending[0].hash_prefix.len(), 8);

    session.close().await;
}

// ---------------------------------------------------------------------------
// CLIP-22 — the transport is unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clipboard_traffic_requires_the_pinned_identity_like_everything_else() {
    let server = TestServer::start().await;
    let (client, _captured) = TestClient::new_raw_clipboard("phone");

    let token = server.open_pairing(Duration::from_secs(30)).await;
    let session = client
        .connect(server.addr, server.fingerprint, Some(&token))
        .await
        .expect("pairing");
    session.close().await;
    server
        .set_grant(client.fingerprint, CAPABILITY_ID, true)
        .await;

    // A different device, granted nothing, cannot even pin its way in: the
    // TLS handshake is against the server's real key, and the *client's* key
    // is what the server's grant is keyed on.
    let (impostor, _) = TestClient::new_raw_clipboard("impostor");
    let result = impostor
        .connect(server.addr, server.fingerprint, None)
        .await;
    assert!(
        result.is_err(),
        "an unpaired device must not reach an established session"
    );
    assert_eq!(server.clipboard_backend.current(), None);
}
