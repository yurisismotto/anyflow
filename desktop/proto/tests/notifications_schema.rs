//! The `notifications.v1` field-set regression.
//!
//! # What this guards, and why it is not a grep
//!
//! `notifications.v1` is deliberately the smallest schema that expresses the
//! feature. The pressure to grow it will arrive during the implementation
//! waves — an action index "just for the reply button", a `bytes` field "just
//! for the icon" — and each addition would be a small, locally reasonable
//! change to a schema whose whole security argument is the **absence** of
//! those fields (ADR-0015 §1 clauses 11-13, ADR-0016).
//!
//! So this test compiles the schema to descriptors with the same compiler the
//! build script uses and asserts against the structure. A plain-text grep over
//! the `.proto` would be defeated by a rename, a comment, or a field split
//! across lines; a descriptor cannot hide any of that. Adding a field to a
//! `notifications.v1` message fails this test, which makes the addition a
//! decision someone has to argue for rather than a diff nobody noticed.
//!
//! It is a *regression* test, not a security control: the control is that the
//! field does not exist. This is what stops it quietly coming to exist.

use std::collections::BTreeSet;
use std::path::PathBuf;

use prost_types::field_descriptor_proto::Type;
use prost_types::{DescriptorProto, FileDescriptorSet};

const PROTO_FILE: &str = "anyflow/v1/capabilities/notifications_v1.proto";

fn descriptors() -> FileDescriptorSet {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/proto")
        .canonicalize()
        .expect("the protocol directory should exist");
    protox::compile([PROTO_FILE], [&root]).expect("the schema should compile")
}

fn messages() -> Vec<DescriptorProto> {
    descriptors()
        .file
        .into_iter()
        .filter(|f| f.name() == PROTO_FILE)
        .flat_map(|f| f.message_type)
        .collect()
}

fn message(name: &str) -> DescriptorProto {
    messages()
        .into_iter()
        .find(|m| m.name() == name)
        .unwrap_or_else(|| panic!("message {name} should exist"))
}

fn field_names(m: &DescriptorProto) -> BTreeSet<String> {
    m.field.iter().map(|f| f.name().to_string()).collect()
}

fn expect_fields(name: &str, expected: &[&str]) {
    let m = message(name);
    let want: BTreeSet<String> = expected.iter().map(|s| (*s).to_string()).collect();
    let got = field_names(&m);
    assert_eq!(
        got, want,
        "the field set of {name} changed. Adding a field to notifications.v1 \
         is a protocol decision: update ADR-0016 and this test together, and \
         check the addition against the prohibited list below."
    );
}

// ---------------------------------------------------------------------------
// The exact field set
// ---------------------------------------------------------------------------

#[test]
fn the_message_set_is_exactly_the_approved_one() {
    let names: BTreeSet<String> = messages().iter().map(|m| m.name().to_string()).collect();
    let want: BTreeSet<String> = [
        "NotificationRoles",
        "Progress",
        "NotificationUpsert",
        "NotificationRemove",
        "DismissRequest",
        "NotificationResult",
        "SyncMarker",
        "NotificationControl",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(
        names, want,
        "a message was added to or removed from notifications.v1"
    );
}

#[test]
fn notification_upsert_has_exactly_the_approved_fields() {
    expect_fields(
        "NotificationUpsert",
        &[
            "notification_id",
            "origin_device_id",
            "app_id",
            "app_label",
            "title",
            "body",
            "posted_at_unix_ms",
            "importance",
            "privacy",
            "category",
            "progress",
            "ongoing",
            "dismissible",
            "group_id",
            "group_summary",
            "secondary_profile",
            "redacted",
            "content_hash",
        ],
    );
}

#[test]
fn the_small_messages_have_exactly_the_approved_fields() {
    expect_fields("NotificationRoles", &["roles", "epoch"]);
    expect_fields("Progress", &["current", "max", "indeterminate"]);
    expect_fields(
        "NotificationRemove",
        &["notification_id", "origin_device_id"],
    );
    expect_fields("DismissRequest", &["notification_id", "origin_device_id"]);
    expect_fields("NotificationResult", &["notification_id", "outcome"]);
    expect_fields("SyncMarker", &["sync_id", "phase"]);
    expect_fields(
        "NotificationControl",
        &["roles", "upsert", "remove", "dismiss", "result", "sync"],
    );
}

// ---------------------------------------------------------------------------
// The prohibited surface
// ---------------------------------------------------------------------------

/// Substrings that must not appear in any field name.
///
/// Each entry is a thing v1 refuses to carry, and refuses by *absence* rather
/// than by a check that a later change could invert.
const PROHIBITED_FIELD_SUBSTRINGS: &[&str] = &[
    // Remote execution. This is the line between mirroring a notification and
    // controlling the device that raised it.
    "action",
    "intent",
    "pending",
    "invoke",
    "execute",
    "command",
    "callback",
    // Reply. A notification the desktop can answer is a messaging client with
    // no consent story of its own.
    "reply",
    "remote_input",
    "input",
    // Serialized platform objects. A platform blob is not a protocol, and it
    // drags `Parcelable` versioning between two independent implementations.
    "remote_view",
    "notification_blob",
    "parcel",
    "bundle",
    "extras",
    // History. None exists anywhere in this design, by construction.
    "history",
    "archive",
    "log",
    "journal",
    // Unbounded media.
    "icon",
    "image",
    "picture",
    "bitmap",
    "avatar",
    "thumbnail",
    "sound",
    "attachment",
    // Excluded identifiers: the raw platform key, the app uid, the numeric
    // profile id, the app-internal channel and ranking fields.
    "sbn",
    "status_bar",
    "platform_key",
    "raw_key",
    "uid",
    "user_id",
    "channel",
    "sort_key",
    "people",
];

#[test]
fn no_field_name_hints_at_a_prohibited_capability() {
    for m in messages() {
        for f in &m.field {
            let name = f.name().to_ascii_lowercase();
            for banned in PROHIBITED_FIELD_SUBSTRINGS {
                assert!(
                    !name.contains(banned),
                    "{}.{} contains the prohibited substring {banned:?}. \
                     notifications.v1 carries no actions, no reply, no \
                     PendingIntent, no serialized platform object, no history \
                     and no media — see ADR-0015 §1 and ADR-0016.",
                    m.name(),
                    f.name()
                );
            }
        }
    }
}

/// A name check alone is defeated by a field called `x`. This is the check
/// that is not: **the only `bytes` fields in the whole schema are the four
/// fixed-width identifiers.**
///
/// An arbitrary `bytes` field is the universal escape hatch — a serialized
/// `Notification`, a `PendingIntent`, an icon, a reply payload all fit in one.
/// Every legitimate `bytes` field here is a digest or an id with an exact
/// width enforced in `anyflow_core::notifications`.
#[test]
fn the_only_bytes_fields_are_fixed_width_identifiers() {
    const ALLOWED: &[&str] = &["notification_id", "sync_id", "group_id", "content_hash"];

    for m in messages() {
        for f in &m.field {
            if f.r#type() == Type::Bytes {
                assert!(
                    ALLOWED.contains(&f.name()),
                    "{}.{} is an unbounded `bytes` field. notifications.v1 \
                     carries no binary payload: the only `bytes` fields are \
                     the fixed-width identifiers {ALLOWED:?}, each validated \
                     to an exact length.",
                    m.name(),
                    f.name()
                );
            }
        }
    }
}

/// No message may embed a type from outside this schema, and none may carry a
/// map or an `Any`. A `google.protobuf.Any` is a serialized-blob field wearing
/// a type url, and a `map<string, string>` is an extras bundle by another name.
#[test]
fn no_message_carries_an_open_ended_container() {
    for m in messages() {
        assert!(
            m.options.as_ref().and_then(|o| o.map_entry) != Some(true),
            "{} is a map entry",
            m.name()
        );
        for f in &m.field {
            if f.r#type() == Type::Message {
                let ty = f.type_name();
                assert!(
                    ty.starts_with(".anyflow.v1.capabilities."),
                    "{}.{} embeds {ty}, which is not part of this schema. \
                     notifications.v1 must not carry google.protobuf.Any, a \
                     map, or any type it does not define itself.",
                    m.name(),
                    f.name()
                );
            }
        }
    }
}

/// The dismissal primitive, checked on its own because it is the only message
/// that travels sink → source and causes an effect on the source device.
///
/// Two scalar fields: which notification, and whose. There is no field to
/// widen into remote action execution, and this test is what makes adding one
/// a visible decision.
#[test]
fn the_dismiss_primitive_has_no_remote_execution_path() {
    let m = message("DismissRequest");
    assert_eq!(
        m.field.len(),
        2,
        "DismissRequest must carry exactly two fields"
    );

    let id = &m.field[0];
    assert_eq!(id.name(), "notification_id");
    assert_eq!(id.r#type(), Type::Bytes);

    let origin = &m.field[1];
    assert_eq!(origin.name(), "origin_device_id");
    assert_eq!(origin.r#type(), Type::String);

    assert!(
        m.nested_type.is_empty(),
        "DismissRequest must have no nested types"
    );
    assert!(
        m.enum_type.is_empty(),
        "DismissRequest must have no nested enums"
    );
    assert!(m.oneof_decl.is_empty(), "DismissRequest must have no oneof");
}

/// The capability envelope has exactly six bodies and no seventh — no generic
/// "other", no escape hatch, no `Any`.
#[test]
fn the_control_envelope_has_no_escape_hatch() {
    let m = message("NotificationControl");
    assert_eq!(m.oneof_decl.len(), 1, "one oneof, named `body`");
    assert_eq!(m.oneof_decl[0].name(), "body");
    assert_eq!(m.field.len(), 6);
    for f in &m.field {
        assert_eq!(
            f.r#type(),
            Type::Message,
            "every body must be a message defined in this schema"
        );
        assert!(
            f.oneof_index.is_some(),
            "{} must be inside the oneof",
            f.name()
        );
    }
}

// ---------------------------------------------------------------------------
// Wire compatibility
// ---------------------------------------------------------------------------

/// Field numbers are the wire contract. Renumbering one, or reusing a retired
/// one, silently reinterprets an old peer's bytes as a different field.
#[test]
fn field_numbers_are_pinned() {
    let expected: &[(&str, &[(&str, i32)])] = &[
        ("NotificationRoles", &[("roles", 1), ("epoch", 2)]),
        (
            "Progress",
            &[("current", 1), ("max", 2), ("indeterminate", 3)],
        ),
        (
            "NotificationUpsert",
            &[
                ("notification_id", 1),
                ("origin_device_id", 2),
                ("app_id", 3),
                ("app_label", 4),
                ("title", 5),
                ("body", 6),
                ("posted_at_unix_ms", 7),
                ("importance", 8),
                ("privacy", 9),
                ("category", 10),
                ("progress", 11),
                ("ongoing", 12),
                ("dismissible", 13),
                ("group_id", 14),
                ("group_summary", 15),
                ("secondary_profile", 16),
                ("redacted", 17),
                ("content_hash", 18),
            ],
        ),
        (
            "NotificationRemove",
            &[("notification_id", 1), ("origin_device_id", 2)],
        ),
        (
            "DismissRequest",
            &[("notification_id", 1), ("origin_device_id", 2)],
        ),
        (
            "NotificationResult",
            &[("notification_id", 1), ("outcome", 2)],
        ),
        ("SyncMarker", &[("sync_id", 1), ("phase", 2)]),
        (
            "NotificationControl",
            &[
                ("roles", 1),
                ("upsert", 2),
                ("remove", 3),
                ("dismiss", 4),
                ("result", 5),
                ("sync", 6),
            ],
        ),
    ];

    for (msg_name, fields) in expected {
        let m = message(msg_name);
        for (field_name, number) in *fields {
            let f = m
                .field
                .iter()
                .find(|f| f.name() == *field_name)
                .unwrap_or_else(|| panic!("{msg_name}.{field_name} should exist"));
            assert_eq!(
                f.number(),
                *number,
                "{msg_name}.{field_name} was renumbered: an old peer's bytes \
                 would be reinterpreted as a different field"
            );
        }
    }
}

/// Nothing is reserved, because nothing has been removed. The day a field is
/// retired, its number must be reserved rather than reused — this test is
/// where that will be recorded.
#[test]
fn no_field_number_has_been_retired_or_reused() {
    for m in messages() {
        assert!(
            m.reserved_range.is_empty() && m.reserved_name.is_empty(),
            "{} reserves a field number or name. notifications.v1 has never \
             removed a field, so a reservation means one was retired: record \
             it in ADR-0016 and update this test.",
            m.name()
        );
        let numbers: BTreeSet<i32> = m.field.iter().map(|f| f.number()).collect();
        assert_eq!(
            numbers.len(),
            m.field.len(),
            "{} reuses a field number",
            m.name()
        );
    }
}

/// The enum vocabularies, pinned for the same reason the field numbers are.
///
/// `NOTIFICATION_IMPORTANCE_NONE` is absent on purpose: a notification the
/// phone does not show its own owner must not become a desktop banner.
#[test]
fn enum_values_are_pinned() {
    let file = descriptors()
        .file
        .into_iter()
        .find(|f| f.name() == PROTO_FILE)
        .expect("the file should be in the descriptor set");

    let by_name = |name: &str| -> Vec<(String, i32)> {
        file.enum_type
            .iter()
            .find(|e| e.name() == name)
            .unwrap_or_else(|| panic!("enum {name} should exist"))
            .value
            .iter()
            .map(|v| (v.name().to_string(), v.number()))
            .collect()
    };

    assert_eq!(
        by_name("NotificationRole"),
        vec![
            ("NOTIFICATION_ROLE_UNSPECIFIED".into(), 0),
            ("NOTIFICATION_ROLE_SOURCE".into(), 1),
            ("NOTIFICATION_ROLE_SINK".into(), 2),
            ("NOTIFICATION_ROLE_DISMISS_TARGET".into(), 3),
            ("NOTIFICATION_ROLE_DISMISS_REPORTER".into(), 4),
        ]
    );
    assert_eq!(
        by_name("NotificationImportance"),
        vec![
            ("NOTIFICATION_IMPORTANCE_UNSPECIFIED".into(), 0),
            ("NOTIFICATION_IMPORTANCE_LOW".into(), 1),
            ("NOTIFICATION_IMPORTANCE_NORMAL".into(), 2),
            ("NOTIFICATION_IMPORTANCE_HIGH".into(), 3),
        ]
    );
    assert_eq!(
        by_name("NotificationPrivacy"),
        vec![
            ("NOTIFICATION_PRIVACY_UNSPECIFIED".into(), 0),
            ("NOTIFICATION_PRIVACY_PUBLIC".into(), 1),
            ("NOTIFICATION_PRIVACY_PRIVATE".into(), 2),
            ("NOTIFICATION_PRIVACY_SECRET".into(), 3),
        ]
    );
    assert_eq!(by_name("NotificationCategory").len(), 8);
    assert_eq!(by_name("NotificationOutcome").len(), 15);
}

// ---------------------------------------------------------------------------
// The rest of the protocol is untouched
// ---------------------------------------------------------------------------

/// `notifications.v1` is purely additive: it adds one file and changes no
/// other. If this ever fails, the capability has stopped being additive and
/// every deployed peer is affected.
#[test]
fn the_new_schema_changes_no_existing_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/proto")
        .canonicalize()
        .expect("the protocol directory should exist");

    // Compiling the notifications schema pulls in whatever it depends on.
    // It should depend on nothing: no import, no shared type, no change to
    // the envelope or to another capability.
    let set = protox::compile([PROTO_FILE], [&root]).expect("compiles");
    let files: Vec<&str> = set.file.iter().map(|f| f.name()).collect();
    assert_eq!(
        files,
        vec![PROTO_FILE],
        "notifications_v1.proto must import nothing: it rides on the existing \
         CapabilityMessage as opaque bytes and touches no other schema"
    );

    let file = &set.file[0];
    assert_eq!(file.package(), "anyflow.v1.capabilities");
    assert_eq!(file.syntax(), "proto3");
    assert!(file.dependency.is_empty());
    assert!(
        file.service.is_empty(),
        "the schema declares no service: it is carried as an opaque payload"
    );
}
