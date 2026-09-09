//! `notifications.v1` — the protocol contract.
//!
//! Every fixture here is obviously synthetic. No test in this file constructs
//! anything that reads like a real notification, and none logs a body: on the
//! certification hardware the platform's own OTP redaction did not fire
//! (POC-NOTIF-01), so notification text is treated as fully sensitive user
//! data everywhere, test code included.

use std::collections::BTreeSet;

use anyflow_core::capability::CapabilityRegistry;
use anyflow_core::notifications::{
    self as notif, PeerRoles, Rejection, Role, RolesRejection, Snapshot, SnapshotRejection,
    SnapshotStep,
};
use anyflow_proto::v1::capabilities as pb;
use anyflow_proto::Message;

const SYNTHETIC_DEVICE: &str = "0123456789abcdef0123456789abcdef";

fn id(byte: u8) -> Vec<u8> {
    vec![byte; notif::NOTIFICATION_ID_LEN]
}

fn upsert(byte: u8) -> pb::NotificationUpsert {
    pb::NotificationUpsert {
        notification_id: id(byte),
        origin_device_id: SYNTHETIC_DEVICE.into(),
        app_id: "example.fixture.app".into(),
        app_label: "Fixture App".into(),
        title: "FIXTURE TITLE".into(),
        body: "FIXTURE BODY".into(),
        posted_at_unix_ms: 1_700_000_000_000,
        importance: pb::NotificationImportance::Normal as i32,
        privacy: pb::NotificationPrivacy::Private as i32,
        category: pb::NotificationCategory::Message as i32,
        progress: None,
        ongoing: false,
        dismissible: true,
        group_id: Vec::new(),
        group_summary: false,
        secondary_profile: false,
        redacted: false,
        content_hash: Vec::new(),
    }
}

fn roles(epoch: u32, roles: &[pb::NotificationRole]) -> pb::NotificationRoles {
    pb::NotificationRoles {
        roles: roles.iter().map(|r| *r as i32).collect(),
        epoch,
    }
}

fn marker(sync_id: &[u8], phase: pb::sync_marker::Phase) -> pb::SyncMarker {
    pb::SyncMarker {
        sync_id: sync_id.to_vec(),
        phase: phase as i32,
    }
}

fn sync_id(byte: u8) -> Vec<u8> {
    vec![byte; notif::SYNC_ID_LEN]
}

// ---------------------------------------------------------------------------
// Capability id and negotiation (brief §3, §19)
// ---------------------------------------------------------------------------

#[test]
fn the_capability_id_is_the_canonical_one() {
    assert_eq!(notif::CAPABILITY_ID, "notifications.v1");
}

/// N0 defines the id and registers nothing. The Android source (N1) and the
/// Linux sink (N2) add the two implementations; until then the id must never
/// reach a `HELLO`, or a peer would be told AnyFlow can mirror notifications
/// when no code exists to do it.
#[test]
fn nothing_advertises_notifications_v1_after_n0() {
    let registry = CapabilityRegistry::builder().build();
    assert!(!registry.supports(notif::CAPABILITY_ID));
    assert!(!registry
        .advertised()
        .contains(&notif::CAPABILITY_ID.to_string()));
}

/// Adding the id changes nothing about how any other capability is agreed.
#[test]
fn negotiation_semantics_are_unchanged_for_existing_capabilities() {
    let registry = CapabilityRegistry::builder().build();

    // An empty local registry supports nothing, whatever a peer offers —
    // including the new id, which is the "peer is newer than us" case.
    let peer = vec![
        "battery.v1".to_string(),
        "clipboard.v1".to_string(),
        "files.v1".to_string(),
        notif::CAPABILITY_ID.to_string(),
    ];
    assert!(registry.negotiate(&peer).is_empty());

    // An unknown id is still simply absent from the intersection, which is
    // the forward-compatibility rule `notifications.v1` relies on to be
    // ignorable by an old peer.
    assert!(!registry.supports("notifications.v99"));
}

// ---------------------------------------------------------------------------
// Roles (brief §4)
// ---------------------------------------------------------------------------

/// Absent roles mean no roles. This is the fail-closed default and the reason
/// a peer that has never heard of `notifications.v1` interoperates harmlessly.
#[test]
fn absent_roles_mean_no_roles() {
    let peer = PeerRoles::none();
    assert!(peer.is_empty());
    assert_eq!(peer.epoch(), 0);
    for role in [
        Role::Source,
        Role::Sink,
        Role::DismissTarget,
        Role::DismissReporter,
    ] {
        assert!(!peer.has(role), "a silent peer must claim nothing");
    }
}

#[test]
fn roles_are_added_and_then_updated() {
    let mut peer = PeerRoles::none();

    peer.apply(&roles(1, &[pb::NotificationRole::Source]))
        .expect("first announcement");
    assert!(peer.has(Role::Source));
    assert!(!peer.has(Role::DismissTarget));
    assert_eq!(peer.epoch(), 1);

    peer.apply(&roles(
        2,
        &[
            pb::NotificationRole::Source,
            pb::NotificationRole::DismissTarget,
        ],
    ))
    .expect("widening announcement");
    assert!(peer.has(Role::Source));
    assert!(peer.has(Role::DismissTarget));
    assert_eq!(peer.epoch(), 2);
}

/// The announcement is the complete set, not a delta, so dropping a role is
/// simply not naming it. This is what an Android device does the instant the
/// user revokes notification access: it announces the set without `SOURCE`.
#[test]
fn a_role_is_removed_by_not_naming_it() {
    let mut peer = PeerRoles::none();
    peer.apply(&roles(
        1,
        &[
            pb::NotificationRole::Source,
            pb::NotificationRole::DismissTarget,
        ],
    ))
    .expect("first");

    peer.apply(&roles(2, &[pb::NotificationRole::DismissTarget]))
        .expect("narrowing");

    assert!(
        !peer.has(Role::Source),
        "narrowing must take effect at once"
    );
    assert!(peer.has(Role::DismissTarget));
}

/// An empty set is meaningful, and is exactly what revocation announces. It
/// must be indistinguishable from a peer that never claimed anything.
#[test]
fn an_empty_announcement_narrows_to_nothing() {
    let mut peer = PeerRoles::none();
    peer.apply(&roles(1, &[pb::NotificationRole::Source]))
        .expect("first");

    peer.apply(&roles(2, &[])).expect("revocation");

    assert!(peer.is_empty());
    assert!(!peer.has(Role::Source));
}

/// The rule the epoch exists for: a replayed or reordered announcement cannot
/// re-widen a set that has already narrowed.
#[test]
fn a_stale_epoch_cannot_re_widen_a_narrowed_set() {
    let mut peer = PeerRoles::none();
    peer.apply(&roles(5, &[pb::NotificationRole::Source]))
        .expect("first");
    peer.apply(&roles(6, &[])).expect("narrowing");
    assert!(peer.is_empty());

    // The old, wide announcement arrives again — duplicated, reordered, or
    // replayed by a peer that is confused or hostile.
    let replay = peer.apply(&roles(5, &[pb::NotificationRole::Source]));
    assert_eq!(replay, Err(RolesRejection::StaleEpoch { last: 6, got: 5 }));
    assert!(peer.is_empty(), "a stale message must not restore a role");

    // Equal is also refused: monotonicity is strict, so a duplicate of the
    // *current* epoch carrying a different set cannot take effect either.
    assert_eq!(
        peer.apply(&roles(6, &[pb::NotificationRole::Source])),
        Err(RolesRejection::StaleEpoch { last: 6, got: 6 })
    );
    assert!(peer.is_empty());
}

#[test]
fn epoch_zero_is_unset_and_refused() {
    let mut peer = PeerRoles::none();
    assert_eq!(
        peer.apply(&roles(0, &[pb::NotificationRole::Sink])),
        Err(RolesRejection::UnsetEpoch)
    );
    assert!(peer.is_empty());
    assert_eq!(
        notif::validate_roles(&roles(0, &[])),
        Err(Rejection::UnsetEpoch)
    );
}

/// A future role is ignored, never assumed granted. The rest of the
/// announcement still applies — one unknown value must not discard a set.
#[test]
fn an_unknown_future_role_is_ignored_not_granted() {
    let mut peer = PeerRoles::none();
    let announcement = pb::NotificationRoles {
        roles: vec![pb::NotificationRole::Sink as i32, 4242],
        epoch: 1,
    };

    peer.apply(&announcement).expect("applies");

    assert!(peer.has(Role::Sink));
    assert_eq!(peer.iter().count(), 1, "the unknown role must not be kept");
    assert_eq!(Role::from_wire(4242), None);
    assert_eq!(
        Role::from_wire(pb::NotificationRole::Unspecified as i32),
        None
    );
}

/// Advertising the capability is not the same as being able to use it. A peer
/// that negotiates `notifications.v1` and then claims no role enables no
/// direction at all.
#[test]
fn a_peer_with_the_capability_but_no_role_enables_no_direction() {
    let mut peer = PeerRoles::none();
    peer.apply(&roles(1, &[])).expect("an empty set is legal");

    assert!(peer.is_empty());
    assert!(!peer.has(Role::Sink), "nothing may be sent to it");
    assert!(!peer.has(Role::Source), "nothing may be accepted from it");
    assert!(!peer.has(Role::DismissTarget));
    assert!(!peer.has(Role::DismissReporter));
}

// ---------------------------------------------------------------------------
// Event model round trip (brief §5)
// ---------------------------------------------------------------------------

#[test]
fn every_body_round_trips() {
    let bodies = vec![
        pb::notification_control::Body::Roles(roles(1, &[pb::NotificationRole::Source])),
        pb::notification_control::Body::Upsert(upsert(0xa1)),
        pb::notification_control::Body::Remove(pb::NotificationRemove {
            notification_id: id(0xa1),
            origin_device_id: SYNTHETIC_DEVICE.into(),
        }),
        pb::notification_control::Body::Dismiss(pb::DismissRequest {
            notification_id: id(0xa1),
            origin_device_id: SYNTHETIC_DEVICE.into(),
        }),
        pb::notification_control::Body::Result(pb::NotificationResult {
            notification_id: id(0xa1),
            outcome: pb::NotificationOutcome::Displayed as i32,
        }),
        pb::notification_control::Body::Sync(marker(&sync_id(0x5c), pb::sync_marker::Phase::Begin)),
    ];

    for body in bodies {
        let original = pb::NotificationControl { body: Some(body) };
        let bytes = original.encode_to_vec();
        let decoded = pb::NotificationControl::decode(&bytes[..]).expect("decode");
        assert_eq!(original, decoded);
        assert!(bytes.len() <= notif::MAX_NOTIFICATION_BYTES);
    }
}

/// Posted and Updated are one message on purpose: no platform AnyFlow targets
/// has a separate update operation, so the same identity carrying different
/// content is the whole update mechanism.
#[test]
fn an_update_is_the_same_identity_with_different_content() {
    let first = upsert(0xb2);
    let mut second = upsert(0xb2);
    second.title = "FIXTURE TITLE, EDITED".into();

    assert_eq!(first.notification_id, second.notification_id);
    let a = notif::validate_upsert(&first).expect("valid");
    let b = notif::validate_upsert(&second).expect("valid");
    assert_eq!(a, b, "identity must not move when content changes");
}

#[test]
fn an_empty_control_body_is_refused() {
    let empty = pb::NotificationControl { body: None };
    assert_eq!(
        notif::validate_control(&empty, empty.encoded_len()),
        Err(Rejection::EmptyBody)
    );
}

// ---------------------------------------------------------------------------
// Identity (brief §6)
// ---------------------------------------------------------------------------

/// Exactly 16 bytes. Not "at most", not "at least".
#[test]
fn the_notification_id_is_exactly_sixteen_bytes() {
    assert_eq!(notif::NOTIFICATION_ID_LEN, 16);
    assert!(notif::NotificationId::from_bytes(&id(0x01)).is_ok());

    for bad in [0usize, 1, 8, 15, 17, 32] {
        let err = notif::NotificationId::from_bytes(&vec![0x01; bad])
            .expect_err("a wrong width must be refused");
        assert_eq!(
            err,
            Rejection::BadIdentifierWidth {
                field: "notification_id",
                expected: 16,
                got: bad
            }
        );
    }
}

/// A bad-width id is refused **and not answered**: a `NotificationResult`
/// echoes the id, so there is nothing coherent to correlate a reply with.
/// Same rule `clipboard.v1` applies to a bad-width `event_id`.
#[test]
fn a_bad_width_identity_is_not_answered() {
    let mut msg = upsert(0xc3);
    msg.notification_id = vec![0u8; 15];
    let err = notif::validate_upsert(&msg).expect_err("refused");
    assert!(!err.is_answerable());

    // Everything else is answerable: the sender can act on the verdict.
    let mut oversized = upsert(0xc3);
    oversized.title = "T".repeat(notif::MAX_TITLE_BYTES + 1);
    let err = notif::validate_upsert(&oversized).expect_err("refused");
    assert!(err.is_answerable());
    assert_eq!(err.outcome(), pb::NotificationOutcome::TooLarge);
}

/// The id is independent of title and body. Two notifications that read the
/// same are two notifications; one whose body changed is still the same one.
#[test]
fn identity_is_independent_of_content() {
    let mut same_content_a = upsert(0xd4);
    let mut same_content_b = upsert(0xe5);
    same_content_a.title = "IDENTICAL FIXTURE".into();
    same_content_b.title = "IDENTICAL FIXTURE".into();
    same_content_a.body = "IDENTICAL FIXTURE".into();
    same_content_b.body = "IDENTICAL FIXTURE".into();

    let a = notif::validate_upsert(&same_content_a).expect("valid");
    let b = notif::validate_upsert(&same_content_b).expect("valid");
    assert_ne!(a, b, "identical content must not merge two identities");
}

/// The sink namespaces by the authenticated peer, never by a claimed
/// `origin_device_id`. Two peers sending the same id address two different
/// mirrors, which is what makes "ids from two devices cannot collide" true
/// structurally rather than probabilistically.
#[test]
fn origin_device_id_is_not_identity() {
    let mut honest = upsert(0xf6);
    let mut liar = upsert(0xf6);
    honest.origin_device_id = SYNTHETIC_DEVICE.into();
    liar.origin_device_id = "ffffffffffffffffffffffffffffffff".into();

    // Both validate: the field is carried for display and future multi-hop
    // reasoning, and lying in it is not a protocol error. It is simply not an
    // input to anything, which is why a sink keys on
    // (peer_fingerprint, notification_id) instead.
    let a = notif::validate_upsert(&honest).expect("valid");
    let b = notif::validate_upsert(&liar).expect("valid");
    assert_eq!(a, b, "the id is the id regardless of the claimed origin");

    let mut malformed = upsert(0xf6);
    malformed.origin_device_id = "not-hex".into();
    assert_eq!(
        notif::validate_upsert(&malformed),
        Err(Rejection::MalformedDeviceId)
    );
}

// ---------------------------------------------------------------------------
// Field limits (brief §8)
// ---------------------------------------------------------------------------

/// Sets one length-limited text field of an upsert, so the limit table below
/// can drive every field through the same assertions.
type SetText = fn(&mut pb::NotificationUpsert, String);
/// Sets one string field to a fixed probe value.
type SetProbe = fn(&mut pb::NotificationUpsert);

#[test]
fn text_fields_accept_exactly_the_limit_and_refuse_one_more() {
    // (setter, limit, field name)
    let cases: Vec<(SetText, usize, &str)> = vec![
        (|m, v| m.app_id = v, notif::MAX_APP_ID_BYTES, "app_id"),
        (
            |m, v| m.app_label = v,
            notif::MAX_APP_LABEL_BYTES,
            "app_label",
        ),
        (|m, v| m.title = v, notif::MAX_TITLE_BYTES, "title"),
        (|m, v| m.body = v, notif::MAX_BODY_BYTES, "body"),
    ];

    for (set, limit, field) in cases {
        let mut at_max = upsert(0x11);
        set(&mut at_max, "x".repeat(limit));
        assert!(
            notif::validate_upsert(&at_max).is_ok(),
            "{field} must accept exactly {limit} bytes"
        );

        let mut over = upsert(0x11);
        set(&mut over, "x".repeat(limit + 1));
        assert_eq!(
            notif::validate_upsert(&over),
            Err(Rejection::TooLong {
                field,
                limit,
                got: limit + 1
            }),
            "{field} must refuse {} bytes",
            limit + 1
        );
    }
}

/// The limits are byte counts, not character counts, so multi-byte text is
/// measured the way the wire measures it.
#[test]
fn limits_are_measured_in_bytes_not_characters() {
    let four_byte = "\u{1F600}"; // one char, four UTF-8 bytes
    let mut at_max = upsert(0x12);
    at_max.body = four_byte.repeat(notif::MAX_BODY_BYTES / 4);
    assert_eq!(at_max.body.len(), notif::MAX_BODY_BYTES);
    assert!(notif::validate_upsert(&at_max).is_ok());

    let mut over = upsert(0x12);
    over.body = four_byte.repeat(notif::MAX_BODY_BYTES / 4 + 1);
    assert!(matches!(
        notif::validate_upsert(&over),
        Err(Rejection::TooLong { field: "body", .. })
    ));
}

/// Protobuf refuses to decode a `string` field that is not valid UTF-8, so an
/// invalid encoding never reaches the capability at all. This pins that
/// property rather than assuming it.
#[test]
fn invalid_utf8_never_decodes() {
    // Field 6 (`body`), wire type 2, length 2, then a lone continuation byte
    // and a bare 0xFF — neither is valid UTF-8.
    let bytes = [0x32u8, 0x02, 0x80, 0xFF];
    assert!(pb::NotificationUpsert::decode(&bytes[..]).is_err());
}

#[test]
fn nul_is_refused_in_every_string_field() {
    let cases: Vec<(SetProbe, &str)> = vec![
        (|m| m.app_id = "a\0b".into(), "app_id"),
        (|m| m.app_label = "a\0b".into(), "app_label"),
        (|m| m.title = "a\0b".into(), "title"),
        (|m| m.body = "a\0b".into(), "body"),
    ];
    for (set, field) in cases {
        let mut msg = upsert(0x13);
        set(&mut msg);
        assert_eq!(
            notif::validate_upsert(&msg),
            Err(Rejection::ContainsNul { field })
        );
    }
}

/// Identifiers are fixed-width or refused. They are never shortened to fit,
/// because shortening an identifier is how collisions are manufactured — and
/// a collision here means one person's notification addressing another's
/// mirror. Display text may be truncated (at the source); identity may not.
#[test]
fn identifiers_are_never_truncated_into_collisions() {
    let mut too_long = upsert(0x14);
    too_long.notification_id = vec![0x14; notif::NOTIFICATION_ID_LEN + 8];
    assert!(matches!(
        notif::validate_upsert(&too_long),
        Err(Rejection::BadIdentifierWidth { .. })
    ));

    // Two distinct ids that share their first 16 bytes must both be refused
    // rather than silently becoming one.
    let mut a = upsert(0x14);
    let mut b = upsert(0x14);
    a.notification_id = [id(0xaa), vec![0x01]].concat();
    b.notification_id = [id(0xaa), vec![0x02]].concat();
    assert!(notif::validate_upsert(&a).is_err());
    assert!(notif::validate_upsert(&b).is_err());
}

/// `group_id` and `content_hash` are optional: absent is accepted, and any
/// width other than the exact one is not.
#[test]
fn optional_digests_are_absent_or_exact() {
    let mut absent = upsert(0x15);
    absent.group_id = Vec::new();
    absent.content_hash = Vec::new();
    assert!(notif::validate_upsert(&absent).is_ok());

    let mut exact = upsert(0x15);
    exact.group_id = vec![0u8; notif::GROUP_ID_LEN];
    exact.content_hash = vec![0u8; notif::CONTENT_HASH_LEN];
    assert!(notif::validate_upsert(&exact).is_ok());

    let mut short_group = upsert(0x15);
    short_group.group_id = vec![0u8; notif::GROUP_ID_LEN - 1];
    assert_eq!(
        notif::validate_upsert(&short_group),
        Err(Rejection::BadIdentifierWidth {
            field: "group_id",
            expected: notif::GROUP_ID_LEN,
            got: notif::GROUP_ID_LEN - 1
        })
    );

    let mut long_hash = upsert(0x15);
    long_hash.content_hash = vec![0u8; notif::CONTENT_HASH_LEN + 1];
    assert!(matches!(
        notif::validate_upsert(&long_hash),
        Err(Rejection::BadIdentifierWidth {
            field: "content_hash",
            ..
        })
    ));
}

/// The whole-message ceiling is checked before any field, so a message past
/// the ceiling is refused for being past the ceiling.
#[test]
fn the_message_ceiling_is_enforced_and_sits_below_the_frame_limit() {
    assert!((notif::MAX_NOTIFICATION_BYTES as u64) < anyflow_core::framing::MAX_FRAME_LEN as u64);

    let control = pb::NotificationControl {
        body: Some(pb::notification_control::Body::Upsert(upsert(0x16))),
    };
    assert!(notif::validate_control(&control, control.encoded_len()).is_ok());
    assert_eq!(
        notif::validate_control(&control, notif::MAX_NOTIFICATION_BYTES + 1),
        Err(Rejection::MessageTooLarge {
            limit: notif::MAX_NOTIFICATION_BYTES,
            got: notif::MAX_NOTIFICATION_BYTES + 1
        })
    );
}

#[test]
fn a_sync_id_of_the_wrong_width_is_refused() {
    assert!(
        notif::validate_sync_marker(&marker(&sync_id(0x01), pb::sync_marker::Phase::Begin)).is_ok()
    );
    assert!(
        notif::validate_sync_marker(&marker(&[0x01; 8], pb::sync_marker::Phase::Begin)).is_err()
    );
}

// ---------------------------------------------------------------------------
// Conservative enum resolution (brief §9)
// ---------------------------------------------------------------------------

/// An unknown value resolves to the most conservative option, never to the
/// most permissive one. A privacy control that fails open is not a control.
#[test]
fn unknown_enum_values_resolve_conservatively() {
    assert_eq!(
        notif::privacy_or_default(9999),
        pb::NotificationPrivacy::Secret,
        "an unknown privacy must not be displayed"
    );
    assert_eq!(
        notif::privacy_or_default(pb::NotificationPrivacy::Unspecified as i32),
        pb::NotificationPrivacy::Private,
        "unset must not decay to PUBLIC"
    );
    assert_eq!(
        notif::importance_or_default(9999),
        pb::NotificationImportance::Normal,
        "an unknown importance must not make a mirror louder"
    );
    assert_eq!(
        notif::category_or_default(9999),
        pb::NotificationCategory::Other
    );
}

/// Android `IMPORTANCE_NONE` has no wire value: a notification the phone does
/// not show its own owner must not become a desktop banner.
#[test]
fn there_is_no_wire_value_for_importance_none() {
    let values: Vec<i32> = (0..=3).collect();
    for v in values {
        assert!(pb::NotificationImportance::try_from(v).is_ok());
    }
    assert!(pb::NotificationImportance::try_from(4).is_err());
}

// ---------------------------------------------------------------------------
// Snapshot framing (brief §10)
// ---------------------------------------------------------------------------

fn ids(bytes: &[u8]) -> BTreeSet<notif::NotificationId> {
    bytes
        .iter()
        .map(|b| notif::NotificationId::from_bytes(&id(*b)).expect("valid"))
        .collect()
}

#[test]
fn a_bracketed_snapshot_completes_with_exactly_what_it_named() {
    let mut snap = Snapshot::new();
    let sid = sync_id(0x21);

    assert_eq!(
        snap.marker(&marker(&sid, pb::sync_marker::Phase::Begin)),
        SnapshotStep::Opened
    );
    for byte in [0x01u8, 0x02, 0x03] {
        let nid = notif::NotificationId::from_bytes(&id(byte)).expect("valid");
        assert_eq!(snap.upsert(nid), SnapshotStep::Recorded);
    }
    assert_eq!(
        snap.marker(&marker(&sid, pb::sync_marker::Phase::End)),
        SnapshotStep::Complete {
            named: ids(&[0x01, 0x02, 0x03])
        }
    );
    assert!(!snap.is_open());
}

/// An `END` with no `BEGIN` is refused, never read as "remove everything I did
/// not hear about" — which, with nothing recorded, would clear every mirror
/// for the peer on the strength of one unpaired frame.
#[test]
fn end_without_begin_is_refused() {
    let mut snap = Snapshot::new();
    assert_eq!(
        snap.marker(&marker(&sync_id(0x22), pb::sync_marker::Phase::End)),
        SnapshotStep::Ignored(SnapshotRejection::EndWithoutBegin)
    );
    assert!(!snap.is_open());
}

/// A second `BEGIN` abandons the first: an incomplete snapshot can never be
/// completed, so it must never be applied. Nothing is removed on its behalf.
#[test]
fn a_nested_begin_abandons_the_open_snapshot_without_applying_it() {
    let mut snap = Snapshot::new();
    let first = sync_id(0x23);
    let second = sync_id(0x24);

    snap.marker(&marker(&first, pb::sync_marker::Phase::Begin));
    snap.upsert(notif::NotificationId::from_bytes(&id(0x01)).expect("valid"));

    assert_eq!(
        snap.marker(&marker(&second, pb::sync_marker::Phase::Begin)),
        SnapshotStep::Restarted
    );
    assert_eq!(
        snap.named_count(),
        0,
        "the abandoned items must not carry over"
    );

    // The first snapshot's END is now unpaired and cannot complete anything.
    assert_eq!(
        snap.marker(&marker(&first, pb::sync_marker::Phase::End)),
        SnapshotStep::Ignored(SnapshotRejection::WrongSyncId)
    );
    assert!(snap.is_open(), "the live snapshot must survive a stale END");

    snap.upsert(notif::NotificationId::from_bytes(&id(0x09)).expect("valid"));
    assert_eq!(
        snap.marker(&marker(&second, pb::sync_marker::Phase::End)),
        SnapshotStep::Complete {
            named: ids(&[0x09])
        }
    );
}

/// A stale or interleaved snapshot cannot silently become authoritative.
#[test]
fn a_mismatched_sync_id_cannot_complete_a_snapshot() {
    let mut snap = Snapshot::new();
    snap.marker(&marker(&sync_id(0x25), pb::sync_marker::Phase::Begin));
    snap.upsert(notif::NotificationId::from_bytes(&id(0x07)).expect("valid"));

    assert_eq!(
        snap.marker(&marker(&sync_id(0x26), pb::sync_marker::Phase::End)),
        SnapshotStep::Ignored(SnapshotRejection::WrongSyncId)
    );
    assert!(snap.is_open());
    assert_eq!(snap.named_count(), 1, "the open snapshot is untouched");
}

/// Upserts outside a snapshot are the ordinary live path, not an error. A
/// snapshot is a reconciliation mechanism layered on top of live mirroring,
/// not a mode the protocol has to be in.
#[test]
fn upserts_outside_a_snapshot_are_live_traffic() {
    let mut snap = Snapshot::new();
    let nid = notif::NotificationId::from_bytes(&id(0x08)).expect("valid");
    assert_eq!(snap.upsert(nid), SnapshotStep::Live);
    assert!(!snap.is_open());
}

/// Recording is idempotent: naming the same identity twice in one snapshot
/// means the same thing once.
#[test]
fn a_duplicate_item_inside_a_snapshot_is_idempotent() {
    let mut snap = Snapshot::new();
    let sid = sync_id(0x27);
    snap.marker(&marker(&sid, pb::sync_marker::Phase::Begin));

    for _ in 0..3 {
        let nid = notif::NotificationId::from_bytes(&id(0x0a)).expect("valid");
        assert_eq!(snap.upsert(nid), SnapshotStep::Recorded);
    }

    assert_eq!(
        snap.marker(&marker(&sid, pb::sync_marker::Phase::End)),
        SnapshotStep::Complete {
            named: ids(&[0x0a])
        }
    );
}

/// An interrupted snapshot removes nothing. Failing to remove is the safe
/// direction for a correctness bug; the sink's own grace timer — a local
/// tunable, not protocol — is what eventually clears a departed peer.
#[test]
fn an_abandoned_snapshot_never_becomes_authoritative() {
    let mut snap = Snapshot::new();
    let sid = sync_id(0x28);
    snap.marker(&marker(&sid, pb::sync_marker::Phase::Begin));
    snap.upsert(notif::NotificationId::from_bytes(&id(0x0b)).expect("valid"));

    snap.abandon();

    assert!(!snap.is_open());
    assert_eq!(
        snap.marker(&marker(&sid, pb::sync_marker::Phase::End)),
        SnapshotStep::Ignored(SnapshotRejection::EndWithoutBegin),
        "an abandoned snapshot's END must not complete it later"
    );
}

/// An unknown future phase can neither complete nor abandon a snapshot.
#[test]
fn an_unspecified_phase_cannot_complete_a_snapshot() {
    let mut snap = Snapshot::new();
    let sid = sync_id(0x29);
    snap.marker(&marker(&sid, pb::sync_marker::Phase::Begin));

    let unknown = pb::SyncMarker {
        sync_id: sid.clone(),
        phase: 99,
    };
    assert!(matches!(snap.marker(&unknown), SnapshotStep::Ignored(_)));
    assert!(snap.is_open(), "an unknown phase must change nothing");

    assert_eq!(
        snap.marker(&marker(&sid, pb::sync_marker::Phase::End)),
        SnapshotStep::Complete {
            named: BTreeSet::new()
        }
    );
}

/// The snapshot machine holds identities and nothing else. This is what makes
/// "no notification content is persisted to implement reconnect" structural
/// rather than a promise: there is no field on the type that could hold any.
#[test]
fn the_snapshot_holds_no_content() {
    let mut snap = Snapshot::new();
    snap.marker(&marker(&sync_id(0x2a), pb::sync_marker::Phase::Begin));
    snap.upsert(notif::NotificationId::from_bytes(&id(0x0c)).expect("valid"));

    let rendered = format!("{snap:?}");
    for fixture in ["FIXTURE TITLE", "FIXTURE BODY", "Fixture App"] {
        assert!(
            !rendered.contains(fixture),
            "the snapshot must not be able to hold content"
        );
    }
}

// ---------------------------------------------------------------------------
// Dismissal (brief §12)
// ---------------------------------------------------------------------------

/// The dismiss primitive identifies one opaque notification and nothing else.
#[test]
fn a_dismiss_request_names_only_an_opaque_notification() {
    let req = pb::DismissRequest {
        notification_id: id(0x31),
        origin_device_id: SYNTHETIC_DEVICE.into(),
    };
    assert!(notif::validate_dismiss(&req).is_ok());

    // Round-tripping proves the wire form carries these two fields and no
    // room for a third: a re-encode of the decoded message is byte-identical,
    // so nothing was silently carried in an unknown field.
    let bytes = req.encode_to_vec();
    let decoded = pb::DismissRequest::decode(&bytes[..]).expect("decode");
    assert_eq!(decoded, req);
    assert_eq!(decoded.encode_to_vec(), bytes);
}

/// Idempotence is a property of the *vocabulary*: dismissing something already
/// gone has an answer that is not an error, so a well-behaved implementation
/// has no reason to retry into a loop.
#[test]
fn dismissal_has_an_idempotent_answer() {
    for outcome in [
        pb::NotificationOutcome::UnknownNotification,
        pb::NotificationOutcome::Duplicate,
        pb::NotificationOutcome::NotDismissible,
    ] {
        let result = pb::NotificationResult {
            notification_id: id(0x32),
            outcome: outcome as i32,
        };
        let decoded = pb::NotificationResult::decode(&result.encode_to_vec()[..]).expect("decode");
        assert_eq!(decoded.outcome(), outcome);
    }
}

/// A dismissal is refused for a notification the receiver did not source. The
/// protocol carries the claim; the *check* is the receiver's, and it is
/// against its own records rather than against the claim.
#[test]
fn a_dismiss_for_a_third_devices_notification_is_representable_and_refusable() {
    let req = pb::DismissRequest {
        notification_id: id(0x33),
        origin_device_id: "ffffffffffffffffffffffffffffffff".into(),
    };
    // It validates — a lie is not a malformed message — and the receiver
    // answers UNKNOWN_NOTIFICATION because it has no such id of its own.
    assert!(notif::validate_dismiss(&req).is_ok());
}

// ---------------------------------------------------------------------------
// Cross-language lockstep
// ---------------------------------------------------------------------------

/// The exact bytes a fully-populated upsert encodes to.
///
/// Asserted identically by `NotificationsProtocolTest` on the Android side, so
/// the two independently-generated bindings cannot quietly disagree about a
/// field number, a wire type or an enum value. Both sides compile the same
/// `.proto`; this is what proves they also *agree* about it.
///
/// If this changes, the wire format changed. That is a protocol decision, not
/// a test to update — see ADR-0016 and the field-number pins in
/// `anyflow-proto`'s `notifications_schema` test.
pub const CANONICAL_UPSERT_HEX: &str = "\
12bd010a10000102030405060708090a0b0c0d0e0f1220303132333435363738396162636465663031323334353637383961626364656\
61a136578616d706c652e666978747572652e617070220b46697874757265204170702a0d46495854555245205449544c45320c464958\
5455524520424f44593880d095ffbc314003480250015a04080310076001720800010203040506077801800101880101920120000102\
030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn canonical_upsert() -> pb::NotificationControl {
    pb::NotificationControl {
        body: Some(pb::notification_control::Body::Upsert(
            pb::NotificationUpsert {
                notification_id: (0u8..16).collect(),
                origin_device_id: SYNTHETIC_DEVICE.into(),
                app_id: "example.fixture.app".into(),
                app_label: "Fixture App".into(),
                title: "FIXTURE TITLE".into(),
                body: "FIXTURE BODY".into(),
                posted_at_unix_ms: 1_700_000_000_000,
                importance: pb::NotificationImportance::High as i32,
                privacy: pb::NotificationPrivacy::Private as i32,
                category: pb::NotificationCategory::Message as i32,
                progress: Some(pb::Progress {
                    current: 3,
                    max: 7,
                    indeterminate: false,
                }),
                ongoing: true,
                dismissible: false,
                group_id: (0u8..8).collect(),
                group_summary: true,
                secondary_profile: true,
                redacted: true,
                content_hash: (0u8..32).collect(),
            },
        )),
    }
}

#[test]
fn the_canonical_upsert_encodes_to_the_shared_vector() {
    let hex: String = canonical_upsert()
        .encode_to_vec()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hex, CANONICAL_UPSERT_HEX);
}

#[test]
fn the_shared_vector_decodes_and_validates() {
    let bytes: Vec<u8> = (0..CANONICAL_UPSERT_HEX.len() / 2)
        .map(|i| u8::from_str_radix(&CANONICAL_UPSERT_HEX[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect();

    let decoded = pb::NotificationControl::decode(&bytes[..]).expect("decode");
    assert_eq!(decoded, canonical_upsert());
    assert!(notif::validate_control(&decoded, bytes.len()).is_ok());
}

// ---------------------------------------------------------------------------
// The identity derivation, verified against the Android source adapter
// ---------------------------------------------------------------------------
//
// ADR-0016 §12 puts the derivation in the **source platform adapter**, because
// it needs that device's secret, and N1 implements it in Kotlin. Nothing here
// implements Android source behaviour in portable code: `anyflow_core::
// notifications` still defines only the type, the width and the validation,
// and `NotificationId` is deliberately opaque.
//
// What these tests are is a *verification vector*. They compute ADR-0016's
// construction independently, with a different HMAC implementation, in a
// different language, and assert the same bytes `NotificationIdentityTest`
// pins on the Android side. A typo in a domain string, a length prefix that
// stopped being big-endian, or a truncation that moved off 16 bytes fails here
// or there rather than producing two devices that quietly disagree about which
// mirror is which — the same discipline `PairingProofTest` and `StreamAuthTest`
// already apply to their own constructions.
//
// A future Windows or macOS source inherits the vector unchanged: the
// derivation takes "the platform's own stable notification key" as its input,
// and neither needs a schema change.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// ADR-0016 §1. Pinned as a constant so a change to it is a visible diff.
const ID_DOMAIN: &str = "anyflow/notifications.v1/id/v1";
/// [02 §6.5].
const GROUP_DOMAIN: &str = "anyflow/notifications.v1/group/v1";

/// The same 32 bytes `NotificationSecretTest` uses. Obviously not from a
/// CSPRNG, and therefore obviously a test.
fn fixture_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    for (index, byte) in secret.iter_mut().enumerate() {
        *byte = index as u8;
    }
    secret
}

/// `HMAC-SHA256(secret, domain || len32(key) || key)[0..16]`.
///
/// `len32` is a big-endian `u32`, the same length-prefixing convention the
/// pairing proof and the `files.v1` data-stream MAC use, for the same reason:
/// concatenation must be unambiguous, or two different inputs could hash to
/// one value.
fn derive_notification_id(secret: &[u8], platform_key: &str) -> Vec<u8> {
    let mut mac = <Hmac<Sha256>>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(ID_DOMAIN.as_bytes());
    mac.update(&(platform_key.len() as u32).to_be_bytes());
    mac.update(platform_key.as_bytes());
    mac.finalize().into_bytes()[..notif::NOTIFICATION_ID_LEN].to_vec()
}

fn derive_group_id(group_key: &str) -> Vec<u8> {
    let mut digest = Sha256::new();
    digest.update(GROUP_DOMAIN.as_bytes());
    digest.update((group_key.len() as u32).to_be_bytes());
    digest.update(group_key.as_bytes());
    digest.finalize()[..notif::GROUP_ID_LEN].to_vec()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The shape of a real `StatusBarNotification.key`: `userId|pkg|id|tag|uid`.
/// Used as an opaque string — nothing parses it, and nothing transmits it.
const FIXTURE_PLATFORM_KEY: &str = "0|example.fixture.app|1|null|10123";

#[test]
fn the_notification_id_derivation_matches_the_android_vector() {
    assert_eq!(
        to_hex(&derive_notification_id(
            &fixture_secret(),
            FIXTURE_PLATFORM_KEY
        )),
        "3de5b61a1978912deb452f36b9a61c7a",
    );
}

#[test]
fn the_group_id_derivation_matches_the_android_vector() {
    assert_eq!(
        to_hex(&derive_group_id("0|example.fixture.app|g:chat")),
        "ff4aae015474d140",
    );
}

/// A different key is a different notification, and a different secret is a
/// different id space — which is what makes an identity reset a mirror reset
/// rather than something a peer could correlate across.
#[test]
fn a_different_key_or_secret_derives_a_different_id() {
    let secret = fixture_secret();
    let other_key = "0|example.other.app|1|null|10124";
    let mut other_secret = [0u8; 32];
    for (index, byte) in other_secret.iter_mut().enumerate() {
        *byte = ((index + 1) % 256) as u8;
    }

    assert_eq!(
        to_hex(&derive_notification_id(&secret, other_key)),
        "86726a05ec81785ead37dcb41907d8e1",
    );
    assert_eq!(
        to_hex(&derive_notification_id(&other_secret, FIXTURE_PLATFORM_KEY)),
        "b1904b490581ba28fb9e736d351fc776",
    );
}

/// The length prefix is why two keys cannot be concatenated into a third.
/// Without it `"ab" + "c"` and `"a" + "bc"` would hash identically, and two
/// unrelated notifications would share a name.
#[test]
fn the_length_prefix_makes_concatenation_unambiguous() {
    let secret = fixture_secret();
    assert_ne!(
        derive_notification_id(&secret, "ab|c"),
        derive_notification_id(&secret, "a|bc"),
    );
}

/// A derived id is a valid `NotificationId`: the width the source produces is
/// the width the portable contract accepts, checked rather than assumed.
#[test]
fn a_derived_id_satisfies_the_portable_contract() {
    let derived = derive_notification_id(&fixture_secret(), FIXTURE_PLATFORM_KEY);
    assert_eq!(derived.len(), notif::NOTIFICATION_ID_LEN);
    let parsed = notif::NotificationId::from_bytes(&derived).expect("a derived id is 16 bytes");
    assert_eq!(parsed.as_bytes().as_slice(), derived.as_slice());

    let derived_group = derive_group_id("0|example.fixture.app|g:chat");
    assert_eq!(derived_group.len(), notif::GROUP_ID_LEN);

    let mut message = upsert(0x11);
    message.notification_id = derived;
    message.group_id = derived_group;
    assert!(notif::validate_upsert(&message).is_ok());
}
