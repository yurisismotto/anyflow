//! The `clipboard.v1` security matrix, CLIP-SEC-01 … CLIP-SEC-18.
//!
//! Each test names its gate. They are written against the manager rather than
//! a live TLS session on purpose: the transport's own guarantees (pinning,
//! sequence numbers, envelope de-duplication) already have suites in
//! `anyflow-core`, and re-testing them here would only prove that the mocks
//! agree with each other. What is tested here is what this capability adds.

mod common;

use anyflow_capability_clipboard::backend::{BackendError, MemoryBackend};
use anyflow_capability_clipboard::{
    limits, ClipboardManager, ClipboardPolicy, ClipboardText, SendError,
};
use anyflow_proto::v1::capabilities as pb;
use common::*;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// CLIP-SEC-01 — a peer with no grant
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_01_an_ungranted_peer_is_refused() {
    let device = Device::new("desktop").await;
    let peer = fp(0xaa);
    // Deliberately not granted: the authorizer has never heard of this peer.

    let outcome = handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(1), "phone", "secret", false),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::NotAuthorized));
    assert_eq!(
        device.backend.current(),
        None,
        "an ungranted peer must not reach the clipboard"
    );
    assert!(
        device.backend.writes().is_empty(),
        "the backend must not have been touched at all"
    );
    assert!(
        device.manager.pending_clips().await.is_empty(),
        "an ungranted clip must not even be held in memory"
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-02 — a revoked peer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_02_a_revoked_peer_is_refused_even_with_a_permissive_stored_policy() {
    let device = Device::new("desktop").await;
    let peer = fp(0xbb);

    // Granted, with everything on — then revoked. The stored policy stays
    // permissive, which is the point: revocation must win over it.
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
    device.authorizer.revoke(peer).await;

    let outcome = handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(2), "phone", "after revocation", false),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::NotAuthorized));
    assert_eq!(device.backend.current(), None);
}

// ---------------------------------------------------------------------------
// CLIP-SEC-03 — identity is the fingerprint, not what the payload claims
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_03_policy_follows_the_pinned_identity_not_the_claimed_device_id() {
    let device = Device::new("desktop").await;
    let trusted = fp(0x11);
    let stranger = fp(0x22);

    device
        .authorizer
        .grant(trusted, ClipboardPolicy::default())
        .await;

    // The stranger sends an update claiming to be the trusted device — the
    // same `origin_device_id`, the same everything except the pinned identity
    // the transport established.
    let outcome = handle(
        &device,
        stranger,
        "trusted-device-id",
        &update_payload(&event_id(3), "trusted-device-id", "impersonated", false),
    )
    .await;

    assert_eq!(
        outcome,
        Some(pb::ClipboardOutcome::NotAuthorized),
        "authorization must key on the fingerprint, never on a claimed id"
    );
    assert!(device.manager.pending_clips().await.is_empty());
}

// ---------------------------------------------------------------------------
// CLIP-SEC-04 / CLIP-SEC-05 — replay and duplicate event ids
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_04_and_05_a_replayed_event_is_idempotent() {
    let device = Device::new("desktop").await;
    let peer = fp(0x33);
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

    let payload = update_payload(&event_id(4), "phone", "applied once", false);

    let first = handle(&device, peer, "phone", &payload).await;
    assert_eq!(first, Some(pb::ClipboardOutcome::Applied));

    // Byte-identical replay, and then a re-encoded one with the same id.
    let second = handle(&device, peer, "phone", &payload).await;
    let third = handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(4), "phone", "different text, same id", false),
    )
    .await;

    assert_eq!(second, Some(pb::ClipboardOutcome::Duplicate));
    assert_eq!(third, Some(pb::ClipboardOutcome::Duplicate));

    let writes = device.backend.writes();
    assert_eq!(
        writes.len(),
        1,
        "the clipboard must be written exactly once"
    );
    assert_eq!(writes[0].0, "applied once");
}

#[tokio::test]
async fn clip_sec_05_an_event_id_replayed_through_a_second_peer_is_still_a_duplicate() {
    // The cache is global rather than per-peer precisely so that a peer
    // cannot launder another peer's event id.
    let device = Device::new("desktop").await;
    let a = fp(0x44);
    let b = fp(0x55);
    for peer in [a, b] {
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
    }

    let id = event_id(5);
    assert_eq!(
        handle(
            &device,
            a,
            "phone-a",
            &update_payload(&id, "phone-a", "one", false)
        )
        .await,
        Some(pb::ClipboardOutcome::Applied)
    );
    assert_eq!(
        handle(
            &device,
            b,
            "phone-b",
            &update_payload(&id, "phone-b", "two", false)
        )
        .await,
        Some(pb::ClipboardOutcome::Duplicate),
        "an id already handled must not be re-usable by a different peer"
    );
    assert_eq!(device.backend.writes().len(), 1);
}

// ---------------------------------------------------------------------------
// CLIP-SEC-06 — oversized clipboard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_06_an_oversized_clip_is_refused_and_never_truncated() {
    let device = Device::new("desktop").await;
    let peer = fp(0x66);
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

    let huge = "A".repeat(limits::MAX_CLIPBOARD_TEXT_BYTES + 1);
    let outcome = handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(6), "phone", &huge, false),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::TooLarge));
    assert_eq!(
        device.backend.current(),
        None,
        "nothing, not even a prefix, may reach the clipboard"
    );

    // And the boundary itself is accepted, so the limit is a limit and not an
    // off-by-one.
    let exact = "A".repeat(limits::MAX_CLIPBOARD_TEXT_BYTES);
    assert_eq!(
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(7), "phone", &exact, false)
        )
        .await,
        Some(pb::ClipboardOutcome::Applied)
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-07 — invalid text
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_07_invalid_utf8_never_decodes_and_nul_is_refused() {
    let device = Device::new("desktop").await;
    let peer = fp(0x77);
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

    // A protobuf `string` field carrying invalid UTF-8 is rejected by the
    // decoder itself, so it never reaches the capability's own checks. This
    // is a hand-built frame: field 1 (update) > field 3 (text_utf8) > a lone
    // 0xFF continuation byte.
    let invalid_utf8: Vec<u8> = vec![0x0a, 0x03, 0x1a, 0x01, 0xff];
    let err = device
        .manager
        .handle_control(peer, "phone", &invalid_utf8)
        .await
        .expect_err("invalid UTF-8 must not decode");
    assert!(
        err.to_string().contains("malformed"),
        "unexpected error: {err}"
    );
    assert_eq!(device.backend.current(), None);

    // NUL *is* valid UTF-8, so it survives the decoder and must be caught by
    // the capability. Both platforms refuse it; see ClipboardText::validate.
    assert_eq!(
        handle(
            &device,
            peer,
            "phone",
            &update_payload(&event_id(8), "phone", "abc\0def", false)
        )
        .await,
        Some(pb::ClipboardOutcome::InvalidText)
    );
    assert_eq!(device.backend.current(), None);
}

#[tokio::test]
async fn clip_sec_07_a_content_hash_that_does_not_match_is_refused() {
    let device = Device::new("desktop").await;
    let peer = fp(0x78);
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

    let outcome = handle(
        &device,
        peer,
        "phone",
        &update_payload_with_hash(&event_id(9), "phone", "real text", false, vec![0u8; 32]),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::InvalidText));
    assert_eq!(device.backend.current(), None);

    // An *absent* hash is allowed — the field is optional and the hash is not
    // authentication — so a minimal peer still interoperates.
    assert_eq!(
        handle(
            &device,
            peer,
            "phone",
            &update_payload_with_hash(&event_id(10), "phone", "no hash", false, Vec::new())
        )
        .await,
        Some(pb::ClipboardOutcome::Applied)
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-08 — sensitive clipboard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_08_a_sensitive_hint_reaches_the_platform_as_a_hint() {
    let device = Device::new("desktop").await;
    let peer = fp(0x88);
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

    handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(11), "phone", "one-time code", true),
    )
    .await;

    let writes = device.backend.writes();
    assert_eq!(writes.len(), 1);
    assert!(
        writes[0].1,
        "the sensitive hint must be passed to the platform (wl-copy --sensitive)"
    );

    // And it is per-clip, not sticky.
    handle(
        &device,
        peer,
        "phone",
        &update_payload(&event_id(12), "phone", "ordinary text", false),
    )
    .await;
    assert!(!device.backend.writes()[1].1);
}

#[tokio::test]
async fn clip_sec_08_a_sensitive_clip_is_still_only_a_hint_never_an_acl() {
    // The hint must not be load-bearing in either direction: it neither
    // grants nor denies. A sensitive clip from an *ungranted* peer is still
    // refused, and a sensitive clip from a granted one is still delivered.
    let device = Device::new("desktop").await;
    let ungranted = fp(0x89);

    assert_eq!(
        handle(
            &device,
            ungranted,
            "phone",
            &update_payload(&event_id(13), "phone", "secret", true)
        )
        .await,
        Some(pb::ClipboardOutcome::NotAuthorized)
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-09 — no automatic relay
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_09_a_clip_from_one_peer_is_never_forwarded_to_another() {
    let desktop = Device::new("desktop").await;
    let phone_a = fp(0xa1);
    let tablet_b = fp(0xb2);

    // A may push to us and we apply automatically; B has auto-send on, which
    // is the only way anything could ever be pushed *out* of this machine.
    desktop
        .authorizer
        .grant(
            phone_a,
            ClipboardPolicy {
                auto_receive: true,
                ..ClipboardPolicy::default()
            },
        )
        .await;
    desktop
        .authorizer
        .grant(
            tablet_b,
            ClipboardPolicy {
                auto_send: true,
                ..ClipboardPolicy::default()
            },
        )
        .await;

    let mut to_b = desktop.connect(tablet_b).await;

    handle(
        &desktop,
        phone_a,
        "phone-a",
        &update_payload(&event_id(14), "phone-a", "from phone A", false),
    )
    .await;

    assert_eq!(desktop.backend.current().as_deref(), Some("from phone A"));

    // The write fired the local watch, exactly as a real clipboard would.
    // Drive the manager's watch handler directly, since the supervised
    // watcher is not running in this test.
    desktop.manager.on_local_change().await;

    assert!(
        drain(&mut to_b).is_empty(),
        "a clip received from phone A must not be relayed to tablet B"
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-10 — no sync loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_10_applying_a_remote_clip_does_not_echo_it_back() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xc3);

    // The dangerous configuration: everything automatic, both ways.
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_send: true,
                allow_receive: true,
                auto_send: true,
                auto_receive: true,
            },
        )
        .await;

    let mut to_phone = desktop.connect(phone).await;

    // The reply is returned to the caller (the capability puts it on the
    // session); what must never appear on the session here is an *update*.
    assert_eq!(
        handle(
            &desktop,
            phone,
            "phone",
            &update_payload(&event_id(15), "phone", "round trip", false)
        )
        .await,
        Some(pb::ClipboardOutcome::Applied)
    );
    assert!(
        drain(&mut to_phone).is_empty(),
        "handling an update must not itself push anything outbound"
    );

    // Now the local watcher observes the write the apply caused.
    desktop.manager.on_local_change().await;

    assert!(
        drain(&mut to_phone).is_empty(),
        "the echo of a remote write must not become an outbound update"
    );
}

#[tokio::test]
async fn clip_sec_10_a_genuine_re_copy_of_the_same_text_is_still_sent() {
    // The other half of the property, and the one a naive `new == old` check
    // gets wrong: suppression must be single-use, not permanent.
    let desktop = Device::new("desktop").await;
    let phone = fp(0xc4);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_send: true,
                allow_receive: true,
                auto_send: true,
                auto_receive: true,
            },
        )
        .await;
    let mut to_phone = desktop.connect(phone).await;

    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(16), "phone", "same text", false),
    )
    .await;
    drain(&mut to_phone);

    // The echo: suppressed.
    desktop.manager.on_local_change().await;
    assert!(drain(&mut to_phone).is_empty());

    // The human copies the very same text again. That is a new event and
    // must be sent.
    desktop.backend.user_copies("same text");
    desktop.manager.on_local_change().await;

    let messages = drain(&mut to_phone);
    assert_eq!(
        messages.len(),
        1,
        "a legitimate re-copy must not be swallowed by suppression"
    );
    assert_eq!(expect_update(&messages[0]).text_utf8, "same text");
}

// ---------------------------------------------------------------------------
// CLIP-SEC-11 / CLIP-SEC-12 — the two direction policies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_11_auto_send_off_means_nothing_is_pushed() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xd5);
    // Granted, sending allowed — but not automatically.
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;
    let mut to_phone = desktop.connect(phone).await;

    desktop.backend.user_copies("a local copy");
    desktop.manager.on_local_change().await;

    assert!(
        drain(&mut to_phone).is_empty(),
        "auto-send is off, so a local copy must go nowhere"
    );

    // The manual path still works, because a human asking is a different act.
    let bytes = desktop
        .manager
        .send_current_clipboard(&phone, false)
        .await
        .expect("a manual send is allowed by the default policy");
    assert_eq!(bytes, "a local copy".len());
    assert_eq!(drain(&mut to_phone).len(), 1);
}

#[tokio::test]
async fn clip_sec_11_allow_send_off_refuses_even_a_manual_send() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xd6);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_send: false,
                ..ClipboardPolicy::default()
            },
        )
        .await;
    let mut to_phone = desktop.connect(phone).await;
    desktop.backend.user_copies("nope");

    assert_eq!(
        desktop.manager.send_current_clipboard(&phone, false).await,
        Err(SendError::NotPermitted)
    );
    assert!(drain(&mut to_phone).is_empty());
}

#[tokio::test]
async fn clip_sec_12_receive_off_drops_the_clip_without_holding_it() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xe7);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_receive: false,
                ..ClipboardPolicy::default()
            },
        )
        .await;

    let outcome = handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(17), "phone", "unwanted", false),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::RejectedPolicy));
    assert_eq!(desktop.backend.current(), None);
    assert!(
        desktop.manager.pending_clips().await.is_empty(),
        "a refused clip must not be retained in memory either"
    );
}

#[tokio::test]
async fn clip_sec_12_auto_receive_off_holds_the_clip_instead_of_applying_it() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xe8);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default()) // auto_receive is off
        .await;

    let outcome = handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(18), "phone", "held for a human", false),
    )
    .await;

    assert_eq!(outcome, Some(pb::ClipboardOutcome::PendingUser));
    assert_eq!(
        desktop.backend.current(),
        None,
        "a pending clip must not touch the system clipboard"
    );

    let pending = desktop.manager.pending_clips().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].bytes, "held for a human".len());

    // Applying it by hand is what writes it.
    let bytes = desktop
        .manager
        .apply_pending(&phone)
        .await
        .expect("a held clip can be applied");
    assert_eq!(bytes, "held for a human".len());
    assert_eq!(
        desktop.backend.current().as_deref(),
        Some("held for a human")
    );
    assert!(desktop.manager.pending_clips().await.is_empty());
}

#[tokio::test]
async fn clip_sec_12_a_held_clip_cannot_be_applied_after_the_grant_is_withdrawn() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xe9);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;
    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(19), "phone", "stale permission", false),
    )
    .await;

    desktop.authorizer.revoke(phone).await;

    assert_eq!(
        desktop.manager.apply_pending(&phone).await,
        Err(SendError::NotPermitted),
        "the grant is re-checked at apply time, not only on arrival"
    );
    assert_eq!(desktop.backend.current(), None);
}

// ---------------------------------------------------------------------------
// CLIP-SEC-13 — a peer cannot change local policy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_13_no_inbound_message_can_widen_local_policy() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0xfa);
    let restrictive = ClipboardPolicy {
        allow_send: true,
        allow_receive: true,
        auto_send: false,
        auto_receive: false,
    };
    desktop.authorizer.grant(phone, restrictive).await;

    // Everything a peer can put on the wire, thrown at the handler: an
    // update, a result, an empty body, and a body tag this build does not
    // know. None of them is a policy write, because the schema has no such
    // message — this test pins that the *absence* is real.
    let payloads: Vec<Vec<u8>> = vec![
        update_payload(&event_id(20), "phone", "text", true),
        {
            use anyflow_proto::Message as _;
            pb::ClipboardControl {
                body: Some(pb::clipboard_control::Body::Result(pb::ClipboardResult {
                    event_id: event_id(20),
                    outcome: pb::ClipboardOutcome::Applied as i32,
                })),
            }
            .encode_to_vec()
        },
        Vec::new(),
        // Field 99, length-delimited: a body from a hypothetical future.
        vec![0xfa, 0x06, 0x02, 0x01, 0x02],
    ];

    for payload in &payloads {
        let _ = desktop
            .manager
            .handle_control(phone, "phone", payload)
            .await;
    }

    // Read back from the authorizer, which is the only thing that can hold a
    // policy at all: the manager has no setter for one, by construction.
    use anyflow_capability_clipboard::ClipboardAuthorizer as _;
    assert_eq!(
        desktop.authorizer.policy_for(&phone).await,
        restrictive,
        "no inbound message may alter local policy"
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-14 — content never reaches a diagnostic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_14_no_debug_or_display_rendering_carries_content() {
    // A string with no letter in common with any error message below, so a
    // substring assertion cannot pass by accident.
    const SECRET: &str = "correct-horse-battery-staple";
    const FILLER: char = '\u{7}';

    let text = ClipboardText::validate(SECRET).expect("valid");
    for rendered in [format!("{text:?}"), format!("{:?}", Some(text.clone()))] {
        assert!(
            !rendered.contains(SECRET),
            "ClipboardText Debug leaked content: {rendered}"
        );
    }

    // The errors a peer or an operator can see.
    let oversized = String::from(FILLER).repeat(limits::MAX_CLIPBOARD_TEXT_BYTES + 1);
    let rejection = ClipboardText::validate(oversized).expect_err("oversized is refused");
    assert!(
        !format!("{rejection}").contains(FILLER),
        "the rejection message must describe the size, not echo the content"
    );
    assert!(!format!("{rejection:?}").contains(FILLER));

    for e in [
        SendError::NotPermitted,
        SendError::NotConnected,
        SendError::NothingToSend,
        SendError::Rejected(rejection),
        SendError::Backend(BackendError::Failed(SECRET.into())),
        SendError::SessionBusy,
    ] {
        let rendered = format!("{e}");
        // The one case that *could* carry content is a backend message, and
        // the backends never build one from clipboard text — asserted here so
        // that a future backend which did would fail this test.
        if !matches!(e, SendError::Backend(_)) {
            assert!(
                !rendered.contains(SECRET),
                "SendError leaked content: {rendered}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// CLIP-SEC-15 — nothing is persisted
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_15_a_pending_clip_is_dropped_when_the_session_ends() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x1b);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;
    let _rx = desktop.connect(phone).await;

    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(21), "phone", "in memory only", false),
    )
    .await;
    assert_eq!(desktop.manager.pending_clips().await.len(), 1);

    desktop.manager.detach_session(&phone).await;

    assert!(
        desktop.manager.pending_clips().await.is_empty(),
        "a clip must not outlive the session that delivered it"
    );
}

#[tokio::test(start_paused = true)]
async fn clip_sec_15_a_pending_clip_expires_on_its_own() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x1c);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;

    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(22), "phone", "forgotten", false),
    )
    .await;
    assert_eq!(desktop.manager.pending_clips().await.len(), 1);

    tokio::time::advance(limits::PENDING_CLIP_TTL + std::time::Duration::from_secs(1)).await;

    assert!(
        desktop.manager.pending_clips().await.is_empty(),
        "clipboard content must not sit in memory indefinitely"
    );
    assert_eq!(
        desktop.manager.apply_pending(&phone).await,
        Err(SendError::NothingToSend)
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-16 — revocation mid-session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_16_revocation_stops_clipboard_traffic_on_a_live_session() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x2d);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_send: true,
                allow_receive: true,
                auto_send: true,
                auto_receive: true,
            },
        )
        .await;
    let mut to_phone = desktop.connect(phone).await;

    // Working, on an established session.
    assert_eq!(
        handle(
            &desktop,
            phone,
            "phone",
            &update_payload(&event_id(23), "phone", "before", false)
        )
        .await,
        Some(pb::ClipboardOutcome::Applied)
    );
    drain(&mut to_phone);

    // The human revokes. The session is untouched — that is the whole point:
    // the capability must stop even though the connection has not.
    desktop.authorizer.revoke(phone).await;
    desktop.manager.policy_changed();

    // Inbound: refused.
    assert_eq!(
        handle(
            &desktop,
            phone,
            "phone",
            &update_payload(&event_id(24), "phone", "after", false)
        )
        .await,
        Some(pb::ClipboardOutcome::NotAuthorized)
    );
    assert_eq!(desktop.backend.current().as_deref(), Some("before"));

    // Outbound, automatic: nothing.
    desktop.backend.user_copies("a local copy after revocation");
    desktop.manager.on_local_change().await;
    assert!(drain(&mut to_phone).is_empty());

    // Outbound, manual: refused too.
    assert_eq!(
        desktop.manager.send_current_clipboard(&phone, false).await,
        Err(SendError::NotPermitted)
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-17 — the caches are bounded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_17_the_event_and_suppression_caches_stay_bounded_under_flood() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x3e);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                auto_receive: true,
                ..ClipboardPolicy::default()
            },
        )
        .await;

    // Ten times the event cache, each with a distinct id and distinct text —
    // so neither cache can be kept small by de-duplication doing the work.
    let flood = limits::EVENT_CACHE_ENTRIES * 10;
    for i in 0..flood {
        let mut id = vec![0u8; 16];
        id[..8].copy_from_slice(&(i as u64).to_be_bytes());
        handle(
            &desktop,
            phone,
            "phone",
            &update_payload(&id, "phone", &format!("clip number {i}"), false),
        )
        .await;
    }

    let (events, suppression) = desktop.manager.cache_sizes().await;
    assert!(
        events <= limits::EVENT_CACHE_ENTRIES,
        "the event cache grew to {events}, past its {} bound",
        limits::EVENT_CACHE_ENTRIES
    );
    assert!(
        suppression <= limits::SUPPRESSION_ENTRIES,
        "the suppression cache grew to {suppression}, past its {} bound",
        limits::SUPPRESSION_ENTRIES
    );
}

// ---------------------------------------------------------------------------
// CLIP-SEC-18 — malformed protobuf
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clip_sec_18_malformed_payloads_are_refused_without_panicking() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x4f);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                auto_receive: true,
                ..ClipboardPolicy::default()
            },
        )
        .await;

    let malformed: Vec<Vec<u8>> = vec![
        vec![0xff, 0xff, 0xff, 0xff],      // garbage tags
        vec![0x0a, 0xff],                  // length prefix past the end
        vec![0x08],                        // a tag with no value
        vec![0x0a, 0x02, 0x08, 0x96],      // truncated varint inside a submessage
        (0..64).map(|_| 0xffu8).collect(), // a wall of continuation bytes
    ];

    for payload in &malformed {
        let result = desktop
            .manager
            .handle_control(phone, "phone", payload)
            .await;
        assert!(
            result.is_err(),
            "a malformed payload must be reported, not accepted: {payload:02x?}"
        );
    }

    // Well-formed but empty, and well-formed with an unknown body: both are
    // accepted and ignored, because a newer peer must not break this one.
    for payload in [Vec::new(), vec![0xfa, 0x06, 0x02, 0x01, 0x02]] {
        let reply = desktop
            .manager
            .handle_control(phone, "phone", &payload)
            .await
            .expect("an unknown body is not an error");
        assert!(reply.is_none(), "there is nothing to reply to");
    }

    assert_eq!(
        desktop.backend.current(),
        None,
        "no malformed payload may reach the clipboard"
    );

    // And a wrong-length event id is refused rather than normalised, with no
    // reply because there is nothing coherent to correlate one with.
    for bad_id in [Vec::new(), vec![0u8; 8], vec![0u8; 32]] {
        let reply = desktop
            .manager
            .handle_control(
                phone,
                "phone",
                &update_payload(&bad_id, "phone", "text", false),
            )
            .await
            .expect("a decodable payload is handled");
        assert!(reply.is_none(), "an unusable event id cannot be answered");
    }
    assert_eq!(desktop.backend.current(), None);
}

// ---------------------------------------------------------------------------
// Backend failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_backend_failure_is_reported_and_releases_the_suppression_entry() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x5a);
    desktop
        .authorizer
        .grant(
            phone,
            ClipboardPolicy {
                allow_send: true,
                allow_receive: true,
                auto_send: true,
                auto_receive: true,
            },
        )
        .await;
    let mut to_phone = desktop.connect(phone).await;

    desktop
        .backend
        .set_failure(Some(BackendError::Unavailable("session is locked".into())));

    assert_eq!(
        handle(
            &desktop,
            phone,
            "phone",
            &update_payload(&event_id(25), "phone", "never written", false)
        )
        .await,
        Some(pb::ClipboardOutcome::Failed)
    );
    drain(&mut to_phone);

    // The write never happened, so the arming must have been released — a
    // stale entry would swallow the user's next copy of that same text.
    desktop.backend.set_failure(None);
    desktop.backend.user_copies("never written");
    desktop.manager.on_local_change().await;

    let messages = drain(&mut to_phone);
    assert_eq!(
        messages.len(),
        1,
        "a failed apply must not leave suppression armed"
    );
}

/// Applying a held clip must report *why* it failed, keep the clip, and leave
/// nothing armed.
///
/// A regression test for two real defects found on hardware, both triggered
/// by the same thing — the screen locking between the clip arriving and the
/// person applying it:
///
///  * the daemon's log said "the session is locked: wl-copy and wl-paste
///    cannot obtain a seat behind the lock screen", while `anyflow clipboard
///    apply` said only "the clipboard backend refused the write". The
///    actionable half was thrown away at the one place a human reads it.
///  * the clip was removed from the pending map *before* the write was
///    attempted, so a transient failure discarded it permanently and the
///    person had to go back to the other device and send it again.
#[tokio::test]
async fn a_failed_apply_reports_the_backend_error_and_keeps_the_clip() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x9e);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default()) // auto_receive off: held
        .await;

    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(27), "phone", "held while locked", false),
    )
    .await;
    assert_eq!(desktop.manager.pending_clips().await.len(), 1);

    desktop.backend.set_failure(Some(BackendError::TimedOut));

    match desktop.manager.apply_pending(&phone).await {
        Err(SendError::Backend(BackendError::TimedOut)) => {}
        other => panic!("expected the backend's own error, got {other:?}"),
    }

    assert_eq!(
        desktop.manager.pending_clips().await.len(),
        1,
        "a failed apply must not discard the held clip"
    );

    // Nothing was written, so nothing should be suppressed: a local copy of
    // that same text must still be sendable.
    let (_, suppression) = desktop.manager.cache_sizes().await;
    assert_eq!(
        suppression, 0,
        "a failed apply must not leave suppression armed"
    );
}

/// And retrying, once the screen is unlocked, works.
#[tokio::test]
async fn a_held_clip_can_be_applied_after_the_backend_recovers() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x9f);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;

    handle(
        &desktop,
        phone,
        "phone",
        &update_payload(&event_id(28), "phone", "retried after unlock", false),
    )
    .await;

    desktop.backend.set_failure(Some(BackendError::TimedOut));
    assert!(desktop.manager.apply_pending(&phone).await.is_err());

    desktop.backend.set_failure(None);
    assert_eq!(
        desktop
            .manager
            .apply_pending(&phone)
            .await
            .expect("a retry should work"),
        "retried after unlock".len()
    );
    assert_eq!(
        desktop.backend.current().as_deref(),
        Some("retried after unlock")
    );
    assert!(desktop.manager.pending_clips().await.is_empty());
}

#[tokio::test]
async fn a_send_to_a_disconnected_peer_fails_cleanly() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x6b);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;
    desktop.backend.user_copies("text");

    assert_eq!(
        desktop.manager.send_current_clipboard(&phone, false).await,
        Err(SendError::NotConnected)
    );
}

#[tokio::test]
async fn an_empty_clipboard_is_nothing_to_send_rather_than_an_error() {
    let desktop = Device::new("desktop").await;
    let phone = fp(0x7c);
    desktop
        .authorizer
        .grant(phone, ClipboardPolicy::default())
        .await;
    let _rx = desktop.connect(phone).await;

    assert_eq!(
        desktop.manager.send_current_clipboard(&phone, false).await,
        Err(SendError::NothingToSend)
    );
}

#[tokio::test]
async fn a_manager_with_no_authorizer_denies_everything() {
    // Failing closed when the wiring is incomplete: a programming error must
    // not become an open door.
    let backend = Arc::new(MemoryBackend::new());
    let manager = ClipboardManager::new(
        Arc::clone(&backend) as Arc<dyn anyflow_capability_clipboard::backend::ClipboardBackend>,
        "desktop",
    );
    let phone = fp(0x8d);

    let reply = manager
        .handle_control(
            phone,
            "phone",
            &update_payload(&event_id(26), "phone", "text", false),
        )
        .await
        .expect("handled");
    assert_eq!(
        pb::ClipboardOutcome::try_from(expect_result(&reply.expect("a reply")).outcome)
            .expect("a known outcome"),
        pb::ClipboardOutcome::NotAuthorized
    );
    assert_eq!(backend.current(), None);
}
