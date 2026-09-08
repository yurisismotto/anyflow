//! `notifications.v1` — the portable wire contract.
//!
//! # What this module is, and what it is not
//!
//! This is the **protocol half** of `notifications.v1`, and nothing else. It
//! holds the field limits, the identifier widths, the role/epoch reduction and
//! the snapshot bracketing state machine: the rules that two independent
//! implementations must agree on or fail to interoperate. Every function here
//! is a pure function of a decoded protobuf message.
//!
//! It contains **no** notification source, **no** sink, **no** D-Bus, **no**
//! `NotificationListenerService`, no policy, no filtering, no caches, no
//! timers and no I/O. Those are the runtime halves and they belong to the
//! platform waves: the Android source adapter (N1) and the Linux sink crate
//! `anyflow-capability-notifications` (N2), which will re-export this module
//! exactly as the plan has `desktop/core/src/notification_policy.rs`
//! re-exported by the capability.
//!
//! There is deliberately **no `NotificationSink` trait here.** Wave 0 declined
//! to create one before a real capability required it, and the implementation
//! plan places the seam in N2's crate beside its first Linux implementation.
//! Creating it now would be a portable abstraction with nothing on either side
//! of it. See `docs/architecture/NOTIFICATIONS.md` §"The platform seams".
//!
//! # Nothing here is content
//!
//! No function in this module stores, logs, hashes for display, or returns a
//! notification's title or body. Rejections carry a *kind* and a *field name*,
//! never a value — the same discipline `clipboard.v1` applies through
//! `redact.rs` and `ClipboardText`'s hand-written `Debug`. On the
//! certification hardware the platform's own OTP redaction did not fire at all
//! (POC-NOTIF-01), so every rule here treats notification text as fully
//! sensitive user data.
//!
//! # Fail closed
//!
//! Absent roles mean no roles. An unset epoch is refused. An unknown enum
//! value resolves to the most conservative option, never to the most
//! permissive one. An `END` that does not match an open `BEGIN` is refused
//! rather than applied.

use std::collections::BTreeSet;

use anyflow_proto::v1::capabilities as pb;

/// The canonical capability id.
///
/// The version is part of the id, as everywhere else in the protocol
/// (ADR-0008): a breaking change ships as `notifications.v2` and both may be
/// advertised at once during a migration. There is no version field inside
/// the capability.
///
/// **This constant does not register anything.** After N0 no implementation of
/// [`crate::capability::Capability`] claims it, so it never appears in a
/// `HELLO` and is never negotiated. N1 and N2 add the two implementations.
pub const CAPABILITY_ID: &str = "notifications.v1";

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/// Exactly, never "at most". An identifier of any other width is refused.
pub const NOTIFICATION_ID_LEN: usize = 16;
/// One CSPRNG value per snapshot. Same width, same rule.
pub const SYNC_ID_LEN: usize = 16;
/// Truncated SHA-256 of the source's group key. Absent or exactly this.
pub const GROUP_ID_LEN: usize = 8;
/// SHA-256. Absent or exactly this.
pub const CONTENT_HASH_LEN: usize = 32;
/// The existing device-id format: 32 lowercase hex characters.
pub const DEVICE_ID_HEX_LEN: usize = 32;

/// Android's package-name ceiling.
pub const MAX_APP_ID_BYTES: usize = 255;
pub const MAX_APP_LABEL_BYTES: usize = 128;
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_BODY_BYTES: usize = 4096;

/// The whole encoded `NotificationControl`, before the envelope.
///
/// Well under `MAX_FRAME_LEN` (64 KiB), leaving the same deliberate headroom
/// `clipboard.v1` reserves so that adding a field later is not a wire-format
/// change.
pub const MAX_NOTIFICATION_BYTES: usize = 8 * 1024;

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

/// Why a message was refused.
///
/// Carries a field *name* and, for a length problem, a *count* — never a
/// value. Nothing constructed here can be printed into a log and leak a
/// notification's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// A fixed-width identifier was not that width.
    ///
    /// Identifiers are never truncated to fit. Shortening an identifier is how
    /// collisions are manufactured, and a collision here means one person's
    /// notification addressing another's mirror.
    BadIdentifierWidth {
        field: &'static str,
        expected: usize,
        got: usize,
    },
    /// A length-limited text field was over its ceiling.
    ///
    /// The **source** truncates display text on a UTF-8 boundary before
    /// sending; the **receiver** refuses. A receiver that repairs malformed
    /// input is a receiver whose limits are advisory.
    TooLong {
        field: &'static str,
        limit: usize,
        got: usize,
    },
    /// A string field contained U+0000.
    ///
    /// Refused rather than stripped, for the reason `clipboard.v1` gives: NUL
    /// cannot be carried faithfully end to end, and any consumer that touches
    /// a C string API truncates at it silently.
    ContainsNul { field: &'static str },
    /// `origin_device_id` was not 32 lowercase hex characters.
    MalformedDeviceId,
    /// A roles announcement carried epoch 0, which is "unset".
    UnsetEpoch,
    /// The encoded message exceeded [`MAX_NOTIFICATION_BYTES`].
    MessageTooLarge { limit: usize, got: usize },
    /// The `oneof` was empty. A `NotificationControl` with no body is not a
    /// forward-compatible unknown message; it is a message with nothing in it.
    EmptyBody,
}

impl Rejection {
    /// The outcome a receiver answers with.
    ///
    /// Every value is safe to send to a peer: it says what happened in a
    /// vocabulary the sender can act on and nothing about local state.
    pub fn outcome(&self) -> pb::NotificationOutcome {
        match self {
            Self::TooLong { .. } | Self::MessageTooLarge { .. } => {
                pb::NotificationOutcome::TooLarge
            }
            _ => pb::NotificationOutcome::Invalid,
        }
    }

    /// Whether the sender can be answered at all.
    ///
    /// A `NotificationResult` echoes a `notification_id`. If the id itself is
    /// the wrong width there is nothing coherent to correlate a reply with, so
    /// the message is refused **and not answered** — the same rule
    /// `clipboard.v1` applies to a bad-width `event_id`.
    pub fn is_answerable(&self) -> bool {
        !matches!(
            self,
            Self::BadIdentifierWidth {
                field: "notification_id",
                ..
            }
        )
    }
}

/// A validated identifier. Construction is the only way to get one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotificationId([u8; NOTIFICATION_ID_LEN]);

impl NotificationId {
    /// Accepts exactly [`NOTIFICATION_ID_LEN`] bytes and nothing else.
    ///
    /// The value is opaque here on purpose. Deriving it needs the source
    /// device's `device_notification_secret`, which exists only on the source
    /// platform, so the derivation lives in the Android adapter (N1) and this
    /// crate manufactures no Android implementation of it. See ADR-0016.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Rejection> {
        let got = bytes.len();
        let arr: [u8; NOTIFICATION_ID_LEN] =
            bytes
                .try_into()
                .map_err(|_| Rejection::BadIdentifierWidth {
                    field: "notification_id",
                    expected: NOTIFICATION_ID_LEN,
                    got,
                })?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; NOTIFICATION_ID_LEN] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/// A peer's claim about which half of the capability it implements.
///
/// A role is **never an authorization input**. The pinned TLS identity and the
/// local grant decide what a peer may do; a role only stops a sender wasting
/// content on a device that would drop it. A peer cannot widen its own grants
/// by claiming a role, and this type has no method that would let it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Source,
    Sink,
    DismissTarget,
    DismissReporter,
}

impl Role {
    /// Unknown and unspecified values map to `None` and are **ignored**.
    ///
    /// A future role must not be assumed granted by a peer that has never
    /// heard of it. Ignoring is the conservative direction: the worst outcome
    /// is that an old peer declines to use a capability half it could not have
    /// implemented anyway.
    pub fn from_wire(value: i32) -> Option<Self> {
        match pb::NotificationRole::try_from(value).ok()? {
            pb::NotificationRole::Source => Some(Self::Source),
            pb::NotificationRole::Sink => Some(Self::Sink),
            pb::NotificationRole::DismissTarget => Some(Self::DismissTarget),
            pb::NotificationRole::DismissReporter => Some(Self::DismissReporter),
            pb::NotificationRole::Unspecified => None,
        }
    }
}

/// What one peer may currently do, on this connection.
///
/// The [`Default`] is **empty**, and that is the whole point: a peer that
/// never announces roles gets nothing sent to it and has nothing accepted from
/// it. Absent roles mean no roles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerRoles {
    roles: BTreeSet<Role>,
    epoch: u32,
}

/// Why a roles announcement was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolesRejection {
    /// Epoch 0 means "unset" and cannot order anything.
    UnsetEpoch,
    /// Not strictly greater than the epoch already accepted.
    ///
    /// This is the rule that makes narrowing safe. A reordered, duplicated or
    /// replayed announcement carrying an older epoch would otherwise re-widen
    /// a set that has already narrowed — and narrowing is exactly what happens
    /// when a user revokes Android notification access mid-session.
    StaleEpoch { last: u32, got: u32 },
}

impl PeerRoles {
    /// The fail-closed starting point: no roles, no epoch.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    pub fn has(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    /// True when the peer claims no role at all.
    ///
    /// A peer can reach this state two ways — by never announcing, or by
    /// announcing an empty set — and they mean the same thing on purpose. An
    /// Android device whose notification access was just revoked announces an
    /// empty set, and it must be indistinguishable from one that never had
    /// any.
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Role> + '_ {
        self.roles.iter().copied()
    }

    /// Applies an announcement, or refuses it.
    ///
    /// The announcement is the **complete set**, not a delta, so this replaces
    /// rather than merges. Narrowing and widening both take effect
    /// immediately: unlike a capability grant, which needs a reconnect to
    /// widen, a role is a statement about what the peer can do *right now*.
    /// Delaying a narrowing is the dangerous direction and this never does it.
    pub fn apply(&mut self, announcement: &pb::NotificationRoles) -> Result<(), RolesRejection> {
        if announcement.epoch == 0 {
            return Err(RolesRejection::UnsetEpoch);
        }
        if announcement.epoch <= self.epoch {
            return Err(RolesRejection::StaleEpoch {
                last: self.epoch,
                got: announcement.epoch,
            });
        }
        self.roles = announcement
            .roles
            .iter()
            .filter_map(|r| Role::from_wire(*r))
            .collect();
        self.epoch = announcement.epoch;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Conservative enum resolution
// ---------------------------------------------------------------------------

/// Unknown importance resolves to `NORMAL`.
///
/// Never to `HIGH`: an unrecognised value must not make a mirror louder than
/// the notification it mirrors.
pub fn importance_or_default(value: i32) -> pb::NotificationImportance {
    match pb::NotificationImportance::try_from(value) {
        Ok(pb::NotificationImportance::Unspecified) | Err(_) => pb::NotificationImportance::Normal,
        Ok(known) => known,
    }
}

/// Unknown or unset privacy resolves to the **most restrictive** option.
///
/// `UNSPECIFIED` is treated as `PRIVATE`, per the schema. An unknown value —
/// which can only come from a future implementation this one cannot reason
/// about — is treated as `SECRET`, meaning "do not display". A privacy control
/// that fails open is not a control.
pub fn privacy_or_default(value: i32) -> pb::NotificationPrivacy {
    match pb::NotificationPrivacy::try_from(value) {
        Ok(pb::NotificationPrivacy::Unspecified) => pb::NotificationPrivacy::Private,
        Ok(known) => known,
        Err(_) => pb::NotificationPrivacy::Secret,
    }
}

/// Unknown category resolves to `OTHER`.
///
/// Categories are presentation and coalescing only, never a security
/// boundary, so the conservative answer is simply the one that claims
/// nothing.
pub fn category_or_default(value: i32) -> pb::NotificationCategory {
    match pb::NotificationCategory::try_from(value) {
        Ok(pb::NotificationCategory::Unspecified) | Err(_) => pb::NotificationCategory::Other,
        Ok(known) => known,
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn check_text(field: &'static str, value: &str, limit: usize) -> Result<(), Rejection> {
    // Protobuf refuses to decode a `string` field that is not valid UTF-8, so
    // an invalid encoding never reaches this function — the same property
    // `clipboard.v1` relies on. What is left to check is the ceiling and NUL.
    if value.len() > limit {
        return Err(Rejection::TooLong {
            field,
            limit,
            got: value.len(),
        });
    }
    if value.contains('\0') {
        return Err(Rejection::ContainsNul { field });
    }
    Ok(())
}

fn check_device_id(value: &str) -> Result<(), Rejection> {
    if value.len() != DEVICE_ID_HEX_LEN
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Rejection::MalformedDeviceId);
    }
    Ok(())
}

fn check_optional_digest(
    field: &'static str,
    value: &[u8],
    expected: usize,
) -> Result<(), Rejection> {
    // Absent is accepted; present-and-wrong-width is not. An optional field
    // whose *width* is negotiable is an optional field with no meaning.
    if value.is_empty() || value.len() == expected {
        Ok(())
    } else {
        Err(Rejection::BadIdentifierWidth {
            field,
            expected,
            got: value.len(),
        })
    }
}

/// Validates an upsert against every protocol bound.
///
/// Returns the validated identity so a caller cannot accidentally act on an
/// unvalidated one. Nothing about the notification's text is returned,
/// retained or logged.
pub fn validate_upsert(msg: &pb::NotificationUpsert) -> Result<NotificationId, Rejection> {
    let id = NotificationId::from_bytes(&msg.notification_id)?;
    check_device_id(&msg.origin_device_id)?;
    check_text("app_id", &msg.app_id, MAX_APP_ID_BYTES)?;
    check_text("app_label", &msg.app_label, MAX_APP_LABEL_BYTES)?;
    check_text("title", &msg.title, MAX_TITLE_BYTES)?;
    check_text("body", &msg.body, MAX_BODY_BYTES)?;
    check_optional_digest("group_id", &msg.group_id, GROUP_ID_LEN)?;
    check_optional_digest("content_hash", &msg.content_hash, CONTENT_HASH_LEN)?;
    Ok(id)
}

/// Validates a removal.
pub fn validate_remove(msg: &pb::NotificationRemove) -> Result<NotificationId, Rejection> {
    let id = NotificationId::from_bytes(&msg.notification_id)?;
    check_device_id(&msg.origin_device_id)?;
    Ok(id)
}

/// Validates a dismissal request.
///
/// The message names one opaque notification and carries nothing else. There
/// is no action index to bound, no intent to reject and no reply text to
/// sanitize, because the schema has no field for any of them.
pub fn validate_dismiss(msg: &pb::DismissRequest) -> Result<NotificationId, Rejection> {
    let id = NotificationId::from_bytes(&msg.notification_id)?;
    check_device_id(&msg.origin_device_id)?;
    Ok(id)
}

/// Validates a roles announcement's epoch.
///
/// The role *list* needs no validation: unknown values are ignored, and an
/// empty list is meaningful.
pub fn validate_roles(msg: &pb::NotificationRoles) -> Result<(), Rejection> {
    if msg.epoch == 0 {
        return Err(Rejection::UnsetEpoch);
    }
    Ok(())
}

/// Validates a snapshot marker's `sync_id` width.
pub fn validate_sync_marker(msg: &pb::SyncMarker) -> Result<(), Rejection> {
    let got = msg.sync_id.len();
    if got != SYNC_ID_LEN {
        return Err(Rejection::BadIdentifierWidth {
            field: "sync_id",
            expected: SYNC_ID_LEN,
            got,
        });
    }
    Ok(())
}

/// Validates one decoded `NotificationControl`, whatever it carries.
///
/// `encoded_len` is the size of the payload as it arrived, checked against
/// [`MAX_NOTIFICATION_BYTES`] before anything else: a message past the
/// ceiling is refused for being past the ceiling, not for whichever field
/// happened to be examined first.
pub fn validate_control(
    msg: &pb::NotificationControl,
    encoded_len: usize,
) -> Result<(), Rejection> {
    if encoded_len > MAX_NOTIFICATION_BYTES {
        return Err(Rejection::MessageTooLarge {
            limit: MAX_NOTIFICATION_BYTES,
            got: encoded_len,
        });
    }
    match msg.body.as_ref().ok_or(Rejection::EmptyBody)? {
        pb::notification_control::Body::Roles(m) => validate_roles(m),
        pb::notification_control::Body::Upsert(m) => validate_upsert(m).map(|_| ()),
        pb::notification_control::Body::Remove(m) => validate_remove(m).map(|_| ()),
        pb::notification_control::Body::Dismiss(m) => validate_dismiss(m).map(|_| ()),
        pb::notification_control::Body::Result(m) => {
            NotificationId::from_bytes(&m.notification_id).map(|_| ())
        }
        pb::notification_control::Body::Sync(m) => validate_sync_marker(m),
    }
}

// ---------------------------------------------------------------------------
// Snapshot framing
// ---------------------------------------------------------------------------

/// What a receiver should do after feeding one message to [`Snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotStep {
    /// A snapshot opened. Nothing is removed yet.
    Opened,
    /// An upsert was recorded as part of the open snapshot.
    Recorded,
    /// No snapshot is open; this upsert is ordinary live traffic.
    ///
    /// Upserts outside a snapshot are **normal and allowed** — the live
    /// mirroring path is exactly that. A snapshot is a reconciliation
    /// mechanism layered on top, not a mode the protocol has to be in.
    Live,
    /// The snapshot completed. Every mirror for this peer whose id is **not**
    /// in the returned set is now stale and should be removed.
    Complete { named: BTreeSet<NotificationId> },
    /// A new `BEGIN` arrived while one was open.
    ///
    /// The open snapshot is abandoned: it can never be completed, so it must
    /// never be applied. Nothing is removed on its behalf. This is the
    /// fail-safe reading of an interrupted snapshot — an incomplete one that
    /// removed mirrors would delete notifications the source never said were
    /// gone.
    Restarted,
    /// The marker was ignored, and why.
    Ignored(SnapshotRejection),
}

/// Why a snapshot marker was not acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotRejection {
    /// An `END` with no `BEGIN` before it.
    ///
    /// Refused, never treated as "remove everything I did not hear about",
    /// which with no recorded items would mean removing every mirror for the
    /// peer on the strength of a single unpaired frame.
    EndWithoutBegin,
    /// An `END` whose `sync_id` is not the open snapshot's.
    ///
    /// A stale or interleaved snapshot cannot silently become authoritative.
    /// The open snapshot stays open; only its own `END` can complete it.
    WrongSyncId,
}

/// The receiving half of the `BEGIN` … items … `END` bracket.
///
/// Holds identities and nothing else. No title, no body, no digest, no
/// timestamp — there is no field on this type that could hold notification
/// content, which is why implementing the reconnect grace costs no
/// persistence of any kind.
///
/// This type deliberately has **no timer**. Abandoning an incomplete snapshot
/// after some interval, and closing a departed peer's mirrors after another,
/// are sink-local implementation tunables: never negotiated, never on the
/// wire, and unable to break interoperability when changed. Only the
/// bracketing and the remove-what-is-not-named rule are protocol, and only
/// those are here.
#[derive(Debug, Default)]
pub struct Snapshot {
    open: Option<Open>,
}

#[derive(Debug)]
struct Open {
    sync_id: Vec<u8>,
    named: BTreeSet<NotificationId>,
}

impl Snapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// How many distinct identities the open snapshot has named.
    ///
    /// A caller enforces its own per-peer ceiling on this; the ceiling itself
    /// is not protocol.
    pub fn named_count(&self) -> usize {
        self.open.as_ref().map_or(0, |o| o.named.len())
    }

    /// Feeds a validated marker.
    pub fn marker(&mut self, msg: &pb::SyncMarker) -> SnapshotStep {
        match msg.phase() {
            pb::sync_marker::Phase::Begin => {
                let restarted = self.open.is_some();
                self.open = Some(Open {
                    sync_id: msg.sync_id.clone(),
                    named: BTreeSet::new(),
                });
                if restarted {
                    SnapshotStep::Restarted
                } else {
                    SnapshotStep::Opened
                }
            }
            pb::sync_marker::Phase::End => match self.open.take() {
                None => SnapshotStep::Ignored(SnapshotRejection::EndWithoutBegin),
                Some(open) => {
                    // Constant-time comparison is not needed: `sync_id` is not
                    // a secret and guessing it buys an attacker nothing they
                    // could not do with an ordinary upsert.
                    if open.sync_id == msg.sync_id {
                        SnapshotStep::Complete { named: open.named }
                    } else {
                        // Put it back. A mismatched END must not close a
                        // snapshot it does not belong to, and must not leave
                        // the receiver in a state where the real END is
                        // rejected as unpaired.
                        self.open = Some(open);
                        SnapshotStep::Ignored(SnapshotRejection::WrongSyncId)
                    }
                }
            },
            // A future phase this implementation does not know. Ignored rather
            // than guessed: an unknown marker must not be able to complete or
            // abandon a snapshot.
            pb::sync_marker::Phase::Unspecified => {
                SnapshotStep::Ignored(SnapshotRejection::EndWithoutBegin)
            }
        }
    }

    /// Feeds a validated upsert identity.
    ///
    /// Recording is idempotent: a duplicate item inside one snapshot names the
    /// same identity twice and means the same thing once.
    pub fn upsert(&mut self, id: NotificationId) -> SnapshotStep {
        match self.open.as_mut() {
            Some(open) => {
                open.named.insert(id);
                SnapshotStep::Recorded
            }
            None => SnapshotStep::Live,
        }
    }

    /// Abandons an open snapshot without applying it.
    ///
    /// What a sink calls when its own abandon timer fires, or when the peer
    /// disconnects mid-snapshot. Nothing is removed: an incomplete snapshot
    /// never becomes authoritative.
    pub fn abandon(&mut self) {
        self.open = None;
    }
}
