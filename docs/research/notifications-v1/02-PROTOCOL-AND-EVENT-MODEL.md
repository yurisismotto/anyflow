# 02 — `notifications.v1` protocol and event model

| Field | Value |
| --- | --- |
| **Title** | Wire schema, identity, lifecycle, limits |
| **Status** | Specification — proposed, not implemented |
| **Last reviewed** | 2026-09-08 |
| **Capability id** | `notifications.v1` |
| **Protocol version** | No change. No `Envelope` change, no `core.proto` change |
| **Related** | [00](00-RESEARCH-FINDINGS.md), [01](01-FUNCTIONAL-SPECIFICATION.md), [03](03-PRIVACY-SECURITY-THREAT-MODEL.md), [ADR-0008](../../adr/ADR-0008-capability-architecture.md), [PROTOCOL.md](../../architecture/PROTOCOL.md), [CLIPBOARD.md](../../architecture/CLIPBOARD.md) |

> **Nothing in this document is implemented.** No `.proto` file was created or
> modified on this branch. The schema below is a proposal in fenced blocks, to
> be turned into `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto`
> in wave N0.

---

## 1. Where it lives

One channel, the existing one:

```text
┌──────────────────────────────────────────┐
│ notifications.v1   NotificationControl   │  ≤ 8 KiB per message
├──────────────────────────────────────────┤
│ session            Envelope, replay guard│
├──────────────────────────────────────────┤
│ framing            u32 length + protobuf │  MAX_FRAME_LEN = 64 KiB
├──────────────────────────────────────────┤
│ TLS 1.3            mutual auth, SPKI pin │
└──────────────────────────────────────────┘
```

No second socket. `files.v1` needed one because a file does not fit in a frame
and must not delay a cancel ([ADR-0012](../../adr/ADR-0012-bulk-transfer-and-frame-limit.md),
[ADR-0013](../../adr/ADR-0013-file-transfer-data-stream.md)); a notification is
a few hundred bytes of text. The moment a notification carries an *image* that
stops being true — which is one of the reasons v1 carries none (§6.4).

---

## 2. The shape of the schema

```protobuf
// protocol/proto/anyflow/v1/capabilities/notifications_v1.proto   (PROPOSED)
syntax = "proto3";
package anyflow.v1.capabilities;

option java_package = "io.github.yurisismotto.anyflow.proto.capabilities";
option java_outer_classname = "NotificationsV1Proto";
option java_multiple_files = true;

// Capability id: "notifications.v1"
//
// Carried as the opaque `payload` of a CapabilityMessage on the ordinary
// control session. Control-plane data only: no icon, no image, no PendingIntent,
// no RemoteViews, no serialized platform Notification object, ever.
message NotificationControl {
  oneof body {
    NotificationRoles  roles   = 1;
    NotificationUpsert upsert  = 2;
    NotificationRemove remove  = 3;
    DismissRequest     dismiss = 4;
    NotificationResult result  = 5;
    SyncMarker         sync    = 6;
  }
}
```

Six messages. The brief asked for the smallest model that keeps the semantics
clear; §4 explains why `Posted` and `Updated` collapsed into one and why the
other five each survive.

---

## 3. Roles — asymmetry is announced, never assumed

A peer must never infer that every platform supports both directions
([00 §3, §4, §5](00-RESEARCH-FINDINGS.md): Windows can do everything, macOS can
sink but not source, iOS can barely sink, Linux sinks only in v1).

```protobuf
enum NotificationRole {
  NOTIFICATION_ROLE_UNSPECIFIED = 0;
  // I can observe this device's own notifications and mirror them to you.
  NOTIFICATION_ROLE_SOURCE = 1;
  // I can display notifications you send me.
  NOTIFICATION_ROLE_SINK = 2;
  // I will act on your DismissRequest for notifications I sourced.
  NOTIFICATION_ROLE_DISMISS_TARGET = 3;
  // I will tell you when a human dismissed a mirror I displayed.
  NOTIFICATION_ROLE_DISMISS_REPORTER = 4;
}

message NotificationRoles {
  repeated NotificationRole roles = 1;
  // Monotonic per connection, starting at 1. A receiver ignores a roles
  // message whose epoch is not greater than the last it accepted, so a
  // reordered or duplicated announcement cannot re-widen a narrowed set.
  uint32 epoch = 2;
}
```

### 3.1 Why roles are inside the capability and not in `HELLO`

`HELLO.capabilities` is a flat list of ids and the transport is deliberately
capability-agnostic ([ADR-0008](../../adr/ADR-0008-capability-architecture.md):
*"There is no capability name anywhere in `anyflow-core`'s transport code"*).
Putting role vocabulary there would push notification semantics into the
handshake, and `notifications.v1/source` as a separate id is exactly the
"four capability ids" alternative that ADR-0008 rejected for clipboard.

There is also a behavioural reason, and it is the stronger one: **roles change
while the connection is up.** The user can revoke Android notification access in
Settings at any moment, which fires `onListenerDisconnected()`
([00 §1.2](00-RESEARCH-FINDINGS.md)). The phone must be able to say "I am no
longer a source" *now*, without a reconnect. `CapabilityAnnounce` cannot express
that — it is all-or-nothing for the whole capability, and dropping
`notifications.v1` entirely would also stop the phone accepting dismissals it
should still accept.

### 3.2 Rules

* Each side sends `NotificationRoles` **first**, from `on_peer_connected`,
  before any other `notifications.v1` message.
* **Absent roles mean no roles.** A peer that never sends one gets nothing sent
  to it and has nothing accepted from it. This is the fail-closed default and it
  is what lets a minimal or older implementation interoperate harmlessly.
* Sending an `upsert` to a peer that did not claim `SINK` is a protocol error on
  the sender's side; the receiver answers `REJECTED_ROLE` and does not close the
  session.
* A role announcement **narrows immediately and widens immediately** — unlike a
  capability grant, which needs a reconnect to widen. Narrowing must never be
  delayed; that is the dangerous direction.
* Roles are a peer's claim about *itself*. They are never an authorization
  input. The grant and the local policy decide what is allowed; roles only avoid
  sending content to a device that would drop it.

### 3.3 v1 role assignment

| Platform | Roles advertised in v1 |
| --- | --- |
| Android | `SOURCE`, `DISMISS_TARGET` |
| Linux | `SINK`, `DISMISS_REPORTER` |

Android does **not** advertise `SINK` in v1 — see [01 §9](01-FUNCTIONAL-SPECIFICATION.md)
for why Linux → Android notification mirroring is out of scope.

---

## 4. The event model

### 4.1 Posted and Updated are one message

Android does not distinguish them: `onNotificationPosted` fires for both a new
notification and an update to an existing one, and the `key` is identical in
both cases ([00 §1.2, §1.3](00-RESEARCH-FINDINGS.md)). freedesktop does not
distinguish them either: `Notify` with `replaces_id=0` creates, `Notify` with
`replaces_id=N` replaces, and it is the *same method*
([00 §2.2](00-RESEARCH-FINDINGS.md)). macOS is the same again — re-adding a
request with the same identifier replaces the delivered notification.

Three platforms, none of which has a separate "update" operation. A protocol
that invented one would have to synthesize the distinction on the source
(by remembering what it had already sent) purely so the sink could throw it away
(by looking up whether it already had a mirror). Both ends would keep the same
state to produce and consume a field that changes nothing.

**Decision: one idempotent upsert.** `NotificationUpsert` means *"this is the
current state of this notification identity"*. The sink creates or replaces
accordingly. This also makes reconnect resynchronisation free — a snapshot is
just upserts (§7).

### 4.2 `NotificationUpsert`

```protobuf
message NotificationUpsert {
  // Stable remote identity. Exactly 16 bytes. See §5.
  bytes  notification_id = 1;
  // Device where the notification actually exists. NOT identity: the pinned
  // TLS peer is. Same rule and same reason as ClipboardUpdate.origin_device_id.
  string origin_device_id = 2;

  // Reverse-DNS application identifier, e.g. "com.example.chat".
  // Attacker-controlled: sanitized and length-capped before display.
  string app_id = 3;
  // Human-readable application name, resolved on the source because only the
  // source has a package database. Also attacker-controlled.
  string app_label = 4;

  string title = 5;   // may be empty
  string body  = 6;   // may be empty

  // Source's wall clock when the notification was posted. Informational only:
  // never an input to authorization, ordering, expiry or de-duplication.
  int64  posted_at_unix_ms = 7;

  NotificationImportance importance = 8;
  NotificationPrivacy    privacy    = 9;
  NotificationCategory   category   = 10;

  // Present only when the source notification actually has a progress bar.
  Progress progress = 11;

  // FLAG_ONGOING_EVENT on Android. A persistent, usually non-dismissible
  // notification: media, navigation, a foreground service.
  bool ongoing = 12;
  // Whether a human can dismiss this at the source at all. Android:
  // isClearable(). False means a DismissRequest for it will be refused, and
  // the sink should not offer dismissal sync for it.
  bool dismissible = 13;

  // Opaque 8-byte digest of the source's group key. Groups mirrored
  // notifications without revealing the platform group string. Absent = ungrouped.
  bytes group_id = 14;
  // This notification is the summary of its group rather than a member.
  bool group_summary = 15;

  // The notification belongs to a secondary profile (Android work profile).
  // A boolean, not the numeric user id: the semantic is portable, the number
  // is an Android implementation detail and a fingerprinting surface.
  bool secondary_profile = 16;

  // Set when the SOURCE reduced this notification before sending it: locked
  // device policy, a per-app "app name only" rule, or platform redaction.
  // The sink shows it as reduced rather than pretending it is complete.
  bool redacted = 17;

  // SHA-256 over the canonical encoding of the semantic fields (§5.4).
  // For de-duplication and for diagnostics that must not log content.
  // NOT authentication. Optional; absent is accepted.
  bytes content_hash = 18;
}
```

**`sensitive_hint` is deliberately absent as a separate field.** Clipboard has
one because Android hands it `ClipDescription.EXTRA_IS_SENSITIVE` — a single
boolean with no richer alternative. Notifications have `visibility`, which is a
three-valued, app-declared statement about lock-screen behaviour and strictly
more informative. Carrying both would invite the two to disagree. See §6.2.

### 4.3 `NotificationRemove`

```protobuf
message NotificationRemove {
  bytes  notification_id  = 1;
  string origin_device_id = 2;
}
```

No reason code. All 23 Android removal reasons
([00 §1.4](00-RESEARCH-FINDINGS.md)) mean the same thing to a mirror: *it is
gone, remove it*. Sending the reason would leak why — `REASON_PACKAGE_BANNED`
and `REASON_CLEAR_DATA` say things about the user's device that a mirror does
not need — and would create a temptation to treat some reasons as "soft"
removals. `REASON_LOCKDOWN`'s javadoc (*"all listeners shall ensure canceled
notifications are removed to prevent data leaking"*) is the reason removal is
unconditional: there is no reason code for which the correct action is to keep
showing it.

The reason **is** used on the source, locally, for echo suppression (§9).

### 4.4 `DismissRequest`

```protobuf
message DismissRequest {
  bytes  notification_id  = 1;
  // Which device's notification this refers to. The receiver additionally
  // requires that it is the origin — a peer cannot dismiss a third device's
  // notification through us.
  string origin_device_id = 2;
}
```

This is the **only** message that travels sink → source and causes an effect on
the source device, and it maps to exactly one platform call:
`cancelNotification(key)`. It carries no action index, no intent, no payload and
no free text. There is nothing in this message that could be widened into remote
action execution, because there is no field to widen: see
[03 §T-N09](03-PRIVACY-SECURITY-THREAT-MODEL.md).

### 4.5 `NotificationResult`

```protobuf
enum NotificationOutcome {
  NOTIFICATION_OUTCOME_UNSPECIFIED = 0;
  NOTIFICATION_OUTCOME_DISPLAYED    = 1;  // created or replaced a mirror
  NOTIFICATION_OUTCOME_REMOVED      = 2;  // mirror closed, or source cancelled
  NOTIFICATION_OUTCOME_DUPLICATE    = 3;  // already handled; nothing happened twice
  NOTIFICATION_OUTCOME_NOT_AUTHORIZED = 4;  // no notifications.v1 grant here
  NOTIFICATION_OUTCOME_REJECTED_POLICY = 5; // granted, but policy says no
  NOTIFICATION_OUTCOME_REJECTED_ROLE   = 6; // I never claimed that role
  NOTIFICATION_OUTCOME_REJECTED_FILTER = 7; // this app is not shared with me
  NOTIFICATION_OUTCOME_NOT_DISMISSIBLE = 8; // source refuses: ongoing / no-clear
  NOTIFICATION_OUTCOME_UNKNOWN_NOTIFICATION = 9; // no such id; already gone
  NOTIFICATION_OUTCOME_TOO_LARGE    = 10;
  NOTIFICATION_OUTCOME_INVALID      = 11; // malformed, bad UTF-8, NUL, bad hash
  NOTIFICATION_OUTCOME_RATE_LIMITED = 12;
  NOTIFICATION_OUTCOME_UNAVAILABLE  = 13; // no notification server / no listener
  NOTIFICATION_OUTCOME_FAILED       = 14; // backend refused
}

message NotificationResult {
  bytes notification_id = 1;
  NotificationOutcome outcome = 2;
}
```

Every value is safe to send to a peer. Same rule as `ClipboardOutcome`: it says
what happened in a vocabulary the sender can act on and **nothing** about the
receiver's local state — no exception text, no D-Bus error, no app list, and
never any part of a notification's content.

`UNKNOWN_NOTIFICATION` and `DUPLICATE` are answers, not errors: they are what
makes dismissal and delivery idempotent (§8).

### 4.6 `SyncMarker`

```protobuf
message SyncMarker {
  enum Phase {
    PHASE_UNSPECIFIED = 0;
    PHASE_BEGIN = 1;
    PHASE_END   = 2;
  }
  bytes sync_id = 1;  // exactly 16 CSPRNG bytes, one per snapshot
  Phase phase   = 2;
}
```

Brackets an active-state snapshot (§7). Without an END marker the sink cannot
tell "the source has finished telling me what is active" from "the source has
gone quiet", and therefore cannot safely remove the mirrors that were not
mentioned.

### 4.7 State machine

One mirrored notification, at the sink:

```text
                     upsert (new id)
        ∅ ──────────────────────────────────▶ MIRRORED
        ▲                                     │  ▲
        │                                     │  │ upsert (same id)
        │  remove | dismissed-by-user |       │  └─ replace in place,
        │  snapshot omission | grace expiry   │     one freedesktop id
        └─────────────────────────────────────┘
```

and at the source, per notification identity:

```text
   onNotificationPosted ──▶ upsert to every granted, filtered, SINK peer
   onNotificationPosted ──▶ upsert again (same identity, new content)
   onNotificationRemoved ─▶ remove to every peer, minus echo suppression (§9)
   DismissRequest ────────▶ cancelNotification(key) ──▶ onNotificationRemoved
```

There is no `PENDING`, no `ACKED`, no retry state. A notification is
soft-realtime data: if an upsert is lost because the link dropped, the reconnect
snapshot (§7) restores the truth, and a retry queue would only add a way to
deliver stale content late.

---

## 5. Stable identity

> **Status: APPROVED** (decision sprint, 2026-09-08), with the lifecycle rules
> in §5.5 added. The derivation, the truncation width and the composite sink
> key are accepted as specified. See the review record in
> [the decision report](../../../NOTIFICATIONS-V1-DECISION-REPORT.md).

The requirement list from the brief, in full: updates replace, duplicates are
suppressed, reconnect does not explode, removal targets the right mirror,
dismissal targets the right source notification, two devices cannot collide, two
apps cannot collide, and the body is never part of the identity.

### 5.1 What the platform gives us

Android's `key` is `userId|pkg|id|tag|uid`
([00 §1.3](00-RESEARCH-FINDINGS.md)) — stable across updates, unique across apps
and profiles, and exactly the argument to `cancelNotification`.

It is also the wrong thing to transmit. It contains a uid and a profile id;
`uid` is an install-specific number that identifies the app installation, and it
would arrive at the desktop for every notification of every app forever, having
never been needed there.

### 5.2 The derived id

```text
notification_id = HMAC-SHA256(
    key = device_notification_secret,
    msg = "anyflow/notifications.v1/id/v1" || len32(key) || key
)[0..16]
```

* `device_notification_secret` — 32 CSPRNG bytes, generated once per install,
  stored beside the identity with the same file mode discipline (0600 in a 0700
  directory; Android: the existing encrypted store). It is **not** a credential
  and authenticates nothing; it exists so the transmitted id reveals nothing
  about the platform key.
* `len32` — big-endian `uint32`, the same length-prefixing convention as the
  pairing proof and the `files.v1` data-stream MAC, for the same reason:
  concatenation must be unambiguous.
* 16 bytes = 128 bits. Same width as `event_id` and `transfer_id`.

**Derived rather than random**, and that choice is load-bearing. A random id
would need a persistent map to stay stable, and an Android process restart would
lose it — so every notification would arrive at the desktop as a *new* one, and
the reconnect snapshot would duplicate the entire notification shade. With a
derived id, the source rebuilds the map by walking `getActiveNotifications()`
and re-deriving, so ids survive a process restart and the snapshot reconciles in
place. That is the difference between a reconnect being invisible and a
reconnect being a wall of duplicates.

The map is `notification_id → platform key`, held **in memory only**, rebuilt on
`onListenerConnected`. It holds identities, never content.

### 5.3 The composite identity at the sink

The sink keys its mirror table on:

```text
(peer_fingerprint, notification_id)  →  freedesktop notification id
```

**`peer_fingerprint`, not `origin_device_id`.** The pinned TLS identity is the
only thing that decides who a message is from, exactly as in
[CLIPBOARD.md](../../architecture/CLIPBOARD.md): *"it is NOT identity and is
never an authorization input — the pinned TLS identity of the sending peer
is."* `origin_device_id` is carried for display and for future multi-hop
reasoning; a peer that lies in it gains nothing, because it can only ever
address its own mirrors.

This structurally satisfies "IDs from two Android devices cannot collide" —
two devices are two fingerprints, and the collision domain is per-peer. The
per-install secret makes it true a second time, independently.

"IDs from two apps cannot collide" comes from `pkg` and `uid` being inside the
hashed key, and from HMAC-SHA256 not colliding at 128 bits.

### 5.4 `content_hash`, and what identity is *not*

```text
content_hash = SHA-256(
    "anyflow/notifications.v1/content/v1"
    || len32(app_id) || app_id || len32(title) || title || len32(body) || body
    || len32(importance) || len32(privacy) || len32(progress_repr) || …)
```

Used for three things and no others: suppressing a re-send when a source
re-posts an identical notification, loop detection, and diagnostics that must
not log content. Not authentication — TLS and the grant are.

**Content is never identity.** Two notifications with the same title and body
from the same app are two notifications if the platform says so, and one
notification whose body changed is still the same notification. Using content as
identity would make an app that edits a message in place create a second mirror,
and would merge two genuinely distinct alerts that happened to read the same.

### 5.5 Lifecycle of `device_notification_secret`

Specified here because the derivation is only half of an identity scheme; the
other half is what happens when the inputs change.

| Event | What happens to the secret | What happens to ids | Why |
| --- | --- | --- | --- |
| App or process restart | **Unchanged** — it is persisted beside the identity | Unchanged. The in-memory `notification_id → key` map is rebuilt by re-deriving over `getActiveNotifications()` | This is the whole reason the id is derived rather than random (§5.2) |
| Device reboot | **Unchanged** | Unchanged | As above |
| Secret file missing or unreadable | **Regenerated** (32 fresh CSPRNG bytes) | All ids change | Fail forward, not closed: a lost secret must not disable the capability. It is not a credential and authenticates nothing |
| **Device identity reset or re-pair** | **Destroyed and regenerated** | All ids change | See below |
| Grant revoked, then re-granted | **Unchanged** | Unchanged | Revocation is not an identity event |

**A regenerated secret is a mirror reset, never something to reconcile.** The
source does not attempt to map old ids to new ones — it cannot, and trying
would require retaining state across the very event that was supposed to clear
it. On the next connection the ordinary reconnect flow (§7.2) does the work: a
`SyncMarker{BEGIN}` … `{END}` bracket names the currently-active notifications
under their new ids, and the sink removes every mirror for that peer not named
in the snapshot. The old mirrors are closed by the mechanism that already
exists.

**Why the secret must not outlive an identity reset.** `notification_id` is
`HMAC(secret, key)` and `key` contains the app's `pkg` and `uid`, both stable
for the life of an install. A peer that recorded ids before a re-pair and saw
the same ids after it could link the old identity to the new one — the exact
correlation re-pairing exists to break. Rotating the secret with the identity
costs nothing (a reset already invalidates every mirror, because the peer
fingerprint changes) and closes it.

### 5.6 What the derived id hides, and what it does not

Stated explicitly because the HMAC is easy to over-read.

| Fact | Reaches the sink? | Via |
| --- | --- | --- |
| Android `userId` (personal vs work profile) | **No** | Hashed away. Profile membership is expressed only by the `include_work_profile` filter, on the source |
| App `uid` (install-specific number) | **No** | Hashed away. It was never needed at the destination |
| Notification `id` and `tag` (app-internal) | **No** | Hashed away |
| The **package name** | **Yes — deliberately** | The separate `app_id` field. The sink must be able to say which app a notification came from, and to apply per-app rules |

So the derived id removes the identifiers that had no destination-side purpose.
It is not, and must not be described as, anonymisation of the source app.

**Does the destination ever need the raw platform key?** No. The only thing the
sink ever sends back that names a notification is `DismissRequest`, and it
names it by `notification_id`; the source maps back to the platform key
locally (§9.1). An id the source cannot map answers `UNKNOWN_NOTIFICATION` —
fail closed, and the message is never a lookup oracle because the id space is
128 bits of HMAC output.

---

## 6. The data model, field by field

### 6.1 Importance

```protobuf
enum NotificationImportance {
  NOTIFICATION_IMPORTANCE_UNSPECIFIED = 0;
  NOTIFICATION_IMPORTANCE_LOW    = 1;  // Android MIN(1), LOW(2)
  NOTIFICATION_IMPORTANCE_NORMAL = 2;  // Android DEFAULT(3)
  NOTIFICATION_IMPORTANCE_HIGH   = 3;  // Android HIGH(4), MAX(5)
}
```

Android importance `NONE(0)` never appears: a notification with importance NONE
is not shown to the user on their own device, so mirroring it would make AnyFlow
*more* intrusive than the phone.

Sink mapping on Linux, and the decision behind it:

| Wire | freedesktop `urgency` |
| --- | --- |
| `LOW` | 0 (low) |
| `NORMAL` | 1 (normal) |
| `HIGH` | 1 (normal) |
| unknown value | 1 (normal) |

**v1 never emits urgency 2 (critical).** On GNOME a critical notification does
not auto-dismiss ([00 §2.4](00-RESEARCH-FINDINGS.md)). Mapping Android `HIGH` —
which every chat app sets — onto critical would turn each message into a banner
the user must click away. A mirror must never be more intrusive than the
original. `HIGH` is still carried on the wire so a future sink with a better
answer can use it, and so the sink can order or group by it.

### 6.2 Privacy

```protobuf
enum NotificationPrivacy {
  NOTIFICATION_PRIVACY_UNSPECIFIED = 0;  // treated as PRIVATE
  NOTIFICATION_PRIVACY_PUBLIC  = 1;  // Android VISIBILITY_PUBLIC  (1)
  NOTIFICATION_PRIVACY_PRIVATE = 2;  // Android VISIBILITY_PRIVATE (0)
  NOTIFICATION_PRIVACY_SECRET  = 3;  // Android VISIBILITY_SECRET (-1)
}
```

The app's own declaration of how it wants to appear on a lock screen. It is the
richest sensitivity signal the platform offers, and it is still **a hint, not an
ACL** — the same sentence [CLIPBOARD.md](../../architecture/CLIPBOARD.md) applies
to `sensitive_hint`. A `PUBLIC` notification from an ungranted peer is still
refused; a `SECRET` one from a granted peer is still governed by policy, not by
the flag.

Two rules make the hint fail safe rather than fail useful:

* **`UNSPECIFIED` is treated as `PRIVATE`.** An unknown or unset value must not
  decay to the most permissive one. This is the general rule for every unknown
  enum value in this schema (§11.3).
* **`SECRET` is never mirrored at all, at the source, unconditionally.** Not
  policy-dependent, not a default that a settings screen can flip. An app that
  marked a notification "do not show this on a lock screen" has said something
  clear enough that forwarding it to another machine cannot be right.

And the rule that matters most, carried over verbatim from the brief and from
the clipboard work: **AnyFlow does not attempt to detect sensitive content
itself.** No OTP regex, no "looks like a bank" heuristic, no keyword list. A
guess dressed up as a security control is worse than an honest boundary —
[THREAT_MODEL.md T10](../../security/THREAT_MODEL.md) already says this about
clipboard passwords and the same reasoning holds here. Where the platform
redacts for us ([00 §1.7](00-RESEARCH-FINDINGS.md)) we pass the redacted text
through and set `redacted`; where it does not, the per-app filter and the lock
policy are the controls, and they are the user's to set.

### 6.3 Category

```protobuf
enum NotificationCategory {
  NOTIFICATION_CATEGORY_UNSPECIFIED = 0;
  NOTIFICATION_CATEGORY_MESSAGE   = 1;  // CATEGORY_MESSAGE, CATEGORY_EMAIL, CATEGORY_SOCIAL
  NOTIFICATION_CATEGORY_CALL      = 2;  // CATEGORY_CALL, CATEGORY_MISSED_CALL, CATEGORY_VOICEMAIL
  NOTIFICATION_CATEGORY_ALARM     = 3;  // CATEGORY_ALARM, CATEGORY_REMINDER, CATEGORY_EVENT
  NOTIFICATION_CATEGORY_PROGRESS  = 4;  // CATEGORY_PROGRESS
  NOTIFICATION_CATEGORY_TRANSPORT = 5;  // CATEGORY_TRANSPORT (media)
  NOTIFICATION_CATEGORY_SYSTEM    = 6;  // CATEGORY_SYSTEM, CATEGORY_SERVICE, CATEGORY_ERROR
  NOTIFICATION_CATEGORY_OTHER     = 7;
}
```

Android defines 24 categories ([00 §1.5](00-RESEARCH-FINDINGS.md)); this is a
lossy, deliberately portable projection of them. Categories are used for
**presentation and coalescing only** — grouping, an icon, a rate-limit class.
They are never a security boundary, exactly as the brief requires: an app
chooses its own category, so `CATEGORY_SYSTEM` is a claim by an arbitrary
application and nothing more.

### 6.4 Progress

```protobuf
message Progress {
  uint32 current       = 1;
  uint32 max           = 2;
  bool   indeterminate = 3;
}
```

Carried because it is semantically portable — a Windows toast has a real
progress bar and macOS can render a percentage — and because it tells the sink
*this identity will update rapidly*, which is a rate-limiting input (§10).

On GNOME it renders nothing: `GetCapabilities` offers no progress capability and
no standard hint exists ([00 §2.1](00-RESEARCH-FINDINGS.md)). The Linux sink
therefore **does not synthesize a percentage into the body**: mutating the app's
own text to add "47%" would corrupt user content to simulate a feature the
desktop does not have. What progress buys on Linux is the update-in-place path
via `replaces_id`, which is the thing the brief actually asked for — a download
produces one notification entry, not hundreds.

### 6.5 Groups

`group_id` is an 8-byte digest, not the platform group string:

```text
group_id = SHA-256("anyflow/notifications.v1/group/v1" || len32(group_key) || group_key)[0..8]
```

Android's `getGroupKey()` embeds the package and often app-internal identifiers.
The sink needs only to know *these mirrors belong together*; 64 bits of digest
says that and nothing else. Collisions across apps are possible in principle and
harmless in practice — the worst case is two unrelated notifications grouped in
a UI, and the sink additionally scopes groups by `app_id`.

### 6.6 Excluded from v1, and why

| Field | Verdict | Reason |
| --- | --- | --- |
| `PendingIntent`, actions, `RemoteInput` | **Never in v1** | This is the line between mirroring and remote control. [03 §T-N09](03-PRIVACY-SECURITY-THREAT-MODEL.md) |
| `RemoteViews` / `contentView` | **Never** | Serialized UI, unbounded, platform-specific, and a deserialization surface |
| Serialized `Notification` / `Bundle` | **Never** | A platform blob is not a protocol. It also drags in `Parcelable` versioning between two independently-versioned implementations |
| Small icon, large icon, picture | **Deferred** | GNOME advertises no `body-images` ([00 §2.1](00-RESEARCH-FINDINGS.md)). Icons are the one place a *bulk* channel might later be justified; until then a mirror shows an app *name*, which is what identifies it anyway |
| `sub_text` | **Deferred** | Portable to macOS/Windows (`subtitle`) but there is no slot in the freedesktop schema, and the v1 sink is Linux. Adding a proto field later is backward compatible, so deferring costs nothing |
| `big_text` / `EXTRA_TEXT_LINES` | **Deferred** | Multiplies payload size for an expanded view the v1 sink cannot render |
| `channel_id` | **Excluded** | App-internal string; `importance` is the portable part of a channel |
| Ranking `rank`, `sortKey` | **Excluded** | Only meaningful inside the source's own shade |
| Android `key`, `uid`, numeric `userId` | **Excluded** | §5.2, §6 — replaced by the derived id and by `secondary_profile` |
| `people` / `EXTRA_PEOPLE` | **Excluded** | Contact URIs. Contact data crossing devices needs its own consent story, not a notification field |

---

## 7. Reconnect and resync

**No notification history exists anywhere in this design.** Not a file, not a
table, not a ring buffer, not a "recent" screen. The only notification state
that exists is what is *currently active on the source* and what is *currently
displayed on the sink*, both in memory.

### 7.1 What happens on disconnect

The sink keeps its mirrors for `RECONNECT_GRACE = 60 s`, then closes all mirrors
for that peer.

The two obvious alternatives are both worse:

* *Close immediately.* A three-second Wi-Fi blip clears the user's desktop and
  then re-posts everything, which on a `persistence` server like GNOME means the
  notification list fills with duplicates. This is precisely the "reconnect
  explodes into duplicates" failure the brief names.
* *Keep indefinitely.* The phone leaves the building and its notifications stay
  on a screen that can no longer update or dismiss them. A stale mirror of a
  banking alert is exactly the leak this feature must not create.

The grace window resolves both: a blip is invisible, and a departure clears the
screen within a minute. During the grace the id map is retained, so a reconnect
inside the window updates mirrors **in place** via `replaces_id` — no flicker,
no duplicates.

### 7.2 What happens on reconnect

```text
source                                          sink
  │  NotificationRoles                            │
  │──────────────────────────────────────────────▶│
  │  SyncMarker{sync_id, BEGIN}                   │
  │──────────────────────────────────────────────▶│
  │  NotificationUpsert × N   (each active,        │
  │      filter-passing, policy-passing            │
  │      notification, right now)                  │
  │──────────────────────────────────────────────▶│
  │  SyncMarker{sync_id, END}                     │
  │──────────────────────────────────────────────▶│
  │                          sink removes every    │
  │                          mirror for this peer  │
  │                          not named in the      │
  │                          snapshot              │
```

The snapshot is built from `getActiveNotifications()`, whose own documentation
defines it as *"the list of outstanding notifications (that is, those that are
visible to the current user)"* ([00 §1.2](00-RESEARCH-FINDINGS.md)).

### 7.3 Active state is not history — the distinction, stated precisely

| | Active-state snapshot | Notification history |
| --- | --- | --- |
| Contents | Only notifications present in the source's shade at the instant of the snapshot | Notifications that existed at some point in the past |
| Lifetime | Discarded as soon as it is applied | Retained |
| Storage | Never written to disk on either side | Written somewhere by definition |
| Visible to the user already? | Yes — they are on the phone's screen right now | Not necessarily; may include ones already dismissed |
| Effect of dismissing on the phone | Disappears from the next snapshot | Still in the history |

An active snapshot tells the desktop something the user can see by picking up
their phone. A history tells it something the user thought was gone. Only the
first is in scope, and the second is not a feature we declined to build for time
— it is one the design forbids.

### 7.4 Bounds and privacy

* `MAX_SNAPSHOT_ENTRIES = 100`. Beyond that the source sends the 100
  most recently posted and stops; the excess simply is not mirrored until it
  updates. A snapshot must not be a way to make one peer send 4 MB.
* The snapshot passes through **every** filter a live notification does: grant,
  policy, per-app list, work-profile switch, lock policy, `SECRET` exclusion. It
  is not a privileged path.
* If the source is locked at snapshot time, the lock policy applies to the
  snapshot, so unlocking is not retroactive: content withheld while locked is
  not delivered later. It arrives, reduced, or not at all.
* A snapshot with no `END` marker within `SYNC_TIMEOUT = 30 s` is abandoned; the
  sink keeps what it has and does not remove anything. Failing to remove is the
  safe direction for a *correctness* bug but the unsafe one for privacy, so the
  grace timer of §7.1 still runs underneath and eventually clears the peer.

### 7.5 Which of these numbers are protocol, and which are not

> **Status: APPROVED** (decision sprint, 2026-09-08). `RECONNECT_GRACE = 60 s`
> was *chosen, not measured* ([OQ-06](06-OPEN-QUESTIONS-AND-POCS.md)), and this
> section is why that is acceptable rather than a deferred problem.

| Constant | Where it lives | Negotiated? | On the wire? | Changing it breaks interop? |
| --- | --- | --- | --- | --- |
| `RECONNECT_GRACE` (60 s) | **Sink only** | No | No | **No** |
| `SYNC_TIMEOUT` (30 s) | **Sink only** | No | No | **No** |
| `MAX_SNAPSHOT_ENTRIES` (100) | **Source only** | No | No | **No** |
| `MAX_MIRRORS_PER_PEER` (200) | Sink only | No | No | No |
| `SyncMarker{BEGIN}` / `{END}` bracketing | **Both** | — | **Yes** | **Yes** |
| "remove every mirror for this peer not named between BEGIN and END" | **Both** | — | Semantics | **Yes** |

**Only the last two rows are protocol.** The timers are local policy: two peers
running different grace windows interoperate correctly, and a future release may
tune any of them without a version bump, a capability flag or a compatibility
note. Freezing a wall-clock value into protocol compatibility would be a cost
paid for ever in exchange for nothing.

What *is* normative about the grace is its bound, not its value: it must be
**greater than zero**, or a brief Wi-Fi blip clears the desktop and re-posts
everything, and it must be **finite**, or a departed phone leaves notifications
on a screen that can no longer update or dismiss them. Any value inside that
bound is a correct implementation; 60 s is a starting point to be tuned in N5.

### 7.6 Why a notification removed mid-snapshot cannot come back

The snapshot is not atomic — notifications can be removed while it is being
built and sent — and the resolution is ordering, not locking.

Every `notifications.v1` message for a peer travels the **one control session**,
in order, over a single TLS connection (§1). So if a notification is read into
the snapshot and then removed on the phone before `{END}`, the
`NotificationRemove` that `onNotificationRemoved` produces is queued *after* the
upsert that carried it and *before or after* `{END}`, but never before the
upsert. The sink therefore always applies "show" then "remove", converging on
removed. A notification removed *before* the snapshot read it is simply never
in it.

That leaves one honest gap, and it is bounded: if the session drops between the
upsert and the removal, the sink holds a mirror the source no longer has. The
grace timer of §7.1 closes it, and the next reconnect's snapshot would not name
it. The failure mode is a stale mirror for at most one grace window, never a
permanent one.

### 7.7 What a hostile or broken snapshot can and cannot do

* **Never send `{END}`.** The sink abandons the sync after `SYNC_TIMEOUT` and
  removes nothing (§7.4). A peer that loops `{BEGIN}`-without-`{END}` keeps its
  own mirrors on screen — but it could keep them there with ordinary upserts
  anyway. **The snapshot path grants a peer no authority it does not already
  have**, which is the property that matters: it is a reconciliation mechanism,
  not a privileged one.
* **Send a huge snapshot.** Bounded by `MAX_SNAPSHOT_ENTRIES` at the source and
  `MAX_MIRRORS_PER_PEER` at the sink, so the memory cost is capped regardless of
  what a peer claims.
* **Keep old notifications alive for ever.** It cannot. Mirrors belong to a
  peer; revoking the grant closes all of them immediately, and a disconnected
  peer's mirrors close after the grace.
* **Cause content to be persisted.** It cannot. During the grace the *content*
  lives in the notification server (gnome-shell), where it was already visible;
  AnyFlow's `MirrorTable` holds `(peer_fingerprint, notification_id) →`
  freedesktop id plus a `content_hash` digest. No notification body is written
  anywhere by AnyFlow, in memory or on disk, to implement the grace.

---

## 8. Idempotence and duplicate suppression

Three caches, all bounded by count **and** age, all holding identities and
digests and never content, all on monotonic clocks
(`tokio::time::Instant` / `SystemClock.elapsedRealtime`) so that a clock change
cannot steer expiry. This is the `clipboard.v1` pattern exactly.

| Cache | Where | Key | Purpose | Bound |
| --- | --- | --- | --- | --- |
| `MirrorTable` | sink | `(peer_fingerprint, notification_id)` | The live map to the freedesktop id. Authoritative for update-in-place | 200 / peer |
| `UpsertDedup` | sink | `(peer_fingerprint, notification_id, content_hash)` | An identical re-send does nothing and answers `DUPLICATE` | 256 / 5 min |
| `EchoSuppression` | source | `notification_id` | One pending listener-cancel, so a dismissal does not echo back to its requester (§9) | 64 / 10 s |

Rules:

* An upsert whose `(id, content_hash)` matches the live mirror is a
  `DUPLICATE`: no D-Bus call, no visual change, no re-alert.
* An upsert whose id matches but whose hash differs **replaces in place** via
  `replaces_id`, and answers `DISPLAYED`.
* A `NotificationRemove` for an id with no mirror answers
  `UNKNOWN_NOTIFICATION`. Not an error — the mirror may have been dismissed by
  the user a moment ago, and both sides converging on "it is gone" is the
  correct outcome.
* A `DismissRequest` for a notification the source no longer has answers
  `UNKNOWN_NOTIFICATION`. Dismissing twice, or dismissing something already
  dismissed, is a no-op that reports success-shaped truth.
* **`NotificationClosed` invalidates the id** ([00 §2.3](00-RESEARCH-FINDINGS.md):
  *"The ID … is invalidated before the signal is sent and may not be used in any
  further communications"*). The sink drops the `MirrorTable` entry on the
  signal, so a subsequent upsert for the same notification creates a fresh
  freedesktop id instead of silently failing to update.

---

## 9. Dismissal, and loop suppression

### 9.1 The flow

```text
  human dismisses the mirror on the desktop
        │
        ▼  NotificationClosed(id, reason=2)      ← reason 2 ONLY
  sink → DismissRequest{notification_id}
        │
        ▼  source: id → key, then cancelNotification(key)
  Android removes it, fires onNotificationRemoved(reason=REASON_LISTENER_CANCEL)
        │
        ▼  source → NotificationRemove … to every OTHER granted peer
           and NOT to the peer that asked          ← the suppression
```

### 9.2 Only reason 2

The freedesktop `NotificationClosed` reasons are `1 expired`, `2 dismissed by
the user`, `3 closed by a call to CloseNotification`, `4 undefined`
([00 §2.3](00-RESEARCH-FINDINGS.md)).

* **1 (expired)** must not dismiss the phone's notification. A desktop banner
  timing out is not a human decision, and treating it as one would silently
  clear the user's phone every time they walked away from their desk. This is
  the single easiest way to build a feature users would rightly call broken.
* **3 (closed by call)** is our own `CloseNotification` returning to us —
  acting on it would be an immediate self-inflicted loop.
* **4 (undefined)** carries no information, so it cannot justify an action on
  another device.

Only 2 produces a `DismissRequest`.

### 9.3 Echo suppression

Without it: desktop dismisses → source cancels → source observes its own
cancellation → source sends `NotificationRemove` to the desktop → the desktop
closes an already-closed mirror. On GNOME that is merely a wasted D-Bus call
(closing an unknown id is a silent no-op, HOST VERIFIED). On a spec-literal
server it is a D-Bus **error** for every dismissal, forever.

So the source arms an `EchoSuppression` entry *before* calling
`cancelNotification`, keyed on the notification id and recording which peer
asked. When `onNotificationRemoved` arrives with `REASON_LISTENER_CANCEL`, the
removal is sent to every granted peer **except** that one, and the entry is
consumed.

Single-use and short-lived, for the same reasons the clipboard's suppression
cache is: an entry whose callback never arrives expires in 10 seconds rather
than swallowing a later, genuine removal, and a failed cancel releases it
immediately because no echo is coming.

### 9.4 The four hard loop rules

1. **AnyFlow never sources its own notifications.** The listener drops
   `sbn.getPackageName() == context.packageName` before anything else — before
   the filter, before policy, before the cache. It is a hard rule, not a
   default: there is no setting that turns it off. This is what stops the
   ongoing-connection notification (`ConnectionService`'s `connectedDevice`
   foreground-service notification, which exists on every running install) from
   being mirrored, and it stops the mirror-of-a-mirror class of loop before it
   can start.
2. **No relay.** A notification received from peer A is never forwarded to peer
   B. Enforced by *absence*, exactly as in
   [CLIPBOARD.md](../../architecture/CLIPBOARD.md): the only function that turns
   a notification into outbound traffic takes a *local platform notification* as
   its input, and there is no code path from an inbound `NotificationUpsert` to
   an outbound one.
3. **`origin_device_id` travels with every message** so that a future
   bidirectional platform can recognise its own notification coming back and
   drop it. Linux is sink-only in v1 and does not need this today; the field
   exists so that the day a Linux source is added, the loop is already
   impossible rather than newly possible.
4. **Sink-created mirrors are marked.** Every notification the sink posts sets
   the `desktop-entry` hint to AnyFlow's own application id. A future Linux
   source skips them by that marker — the same "recognise your own output"
   discipline, applied on the platform where it will be needed next.

---

## 10. Rate limiting, coalescing and backpressure

Notifications update fast: a download bar, a media scrubber, turn-by-turn
navigation, a group chat at lunchtime.

### 10.1 Coalescing at the source

* Outbound upserts are held in a **per-identity slot**, not a queue. A newer
  upsert for the same `notification_id` **replaces** the pending one. A progress
  bar updating 60 times a second produces one pending message, always the
  current one — the same insight as ADR-0014's "the watch yields a signal, not
  content": reading the state after the fact is better than queuing stale states.
* A per-identity **minimum interval** of `MIN_UPDATE_INTERVAL = 500 ms` with
  trailing-edge delivery. The last value always ships; intermediate ones do not.
* **Terminal events never coalesce away.** A `NotificationRemove` replaces any
  pending upsert for that identity and is sent immediately, ignoring the
  interval. Losing a removal to a rate limiter would leave a mirror on screen
  forever, which is the one failure mode that is worse than being slow. The
  brief calls this out and it is the reason coalescing is per-identity rather
  than a global queue drop.

### 10.2 Limits

| Limit | Value | What it protects |
| --- | --- | --- |
| `MIN_UPDATE_INTERVAL` | 500 ms per identity | Progress and media storms |
| `MAX_NEW_PER_10S` | 20 distinct new identities per peer | A malicious or broken source spamming the desktop |
| `MAX_SUSTAINED_RATE` | 2 messages/s per peer, token bucket, burst 20 | Steady-state flood |
| `MAX_ACTIVE_MIRRORS` | 200 per peer at the sink | Unbounded growth |
| `MAX_SNAPSHOT_ENTRIES` | 100 | A snapshot as an amplification vector |
| `MAX_NOTIFICATION_BYTES` | 8 KiB encoded | Frame budget (§11) |

### 10.3 Backpressure

The session's existing bounded outbound channel is the backpressure mechanism —
the same one the `clipboard.v1` session-writer regression test exercises. When
it is full, the notification capability **drops the oldest non-terminal pending
upsert** rather than blocking the listener callback, because that callback runs
on the **Android main thread** ([00 §1.2](00-RESEARCH-FINDINGS.md)) and blocking
it would stall the UI of the entire phone.

Exceeding a limit answers `RATE_LIMITED` and is **never fatal**: one capability
misbehaving must not cost the user the session
([ADR-0008](../../adr/ADR-0008-capability-architecture.md)).

---

## 11. Limits, encoding and unknown fields

### 11.1 Text and identifier limits

| Field | Limit | Notes |
| --- | --- | --- |
| `notification_id` | **exactly** 16 bytes | Any other length is refused and **not answered** — there is nothing coherent to correlate a reply with. Same rule as `event_id` |
| `sync_id` | exactly 16 bytes | |
| `content_hash` | exactly 32 bytes, or absent | Absent is accepted; present-and-mismatched is `INVALID` |
| `group_id` | exactly 8 bytes, or absent | |
| `origin_device_id` | 32 lowercase hex characters | Existing device-id format |
| `app_id` | 255 bytes | Android's package-name ceiling |
| `app_label` | 128 bytes | |
| `title` | 512 bytes | |
| `body` | 4096 bytes | |
| Whole message | 8 KiB encoded | Well under `MAX_FRAME_LEN` (64 KiB), leaving the same deliberate headroom `clipboard.v1` reserves so a future field is not a wire-format change |

### 11.2 Truncation — and why it is allowed here but forbidden in `clipboard.v1`

`clipboard.v1` refuses oversized content outright, because *"a truncated
password, key or command is not a degraded version of the original — it is a
different, wrong value that looks plausible."*

A notification body is not that. It is display text that the user is *already*
reading in truncated form on their phone's shade, and a 4 KiB chat message
shortened to its first 4 KiB is still the message. Refusing it would drop the
notification entirely, which is worse.

So: **the source truncates, the receiver refuses.**

* The **source** truncates `title` and `body` to the limit, on a UTF-8 character
  boundary, appending `U+2026` (…) so the truncation is visible rather than
  silent, and sets nothing else — the truncation is not signalled as `redacted`,
  because it is a length limit, not a privacy reduction.
* The **receiver** refuses anything over the limit with `TOO_LARGE`. It never
  truncates: at that point the message is already malformed by our own rules,
  and a receiver that repairs malformed input is a receiver whose limits are
  advisory.
* **Identifiers are never truncated.** `notification_id`, `group_id` and
  `content_hash` are fixed-length or refused. Shortening an identifier is how
  collisions are manufactured.
* The **protocol storage limit is not the presentation limit.** A sink may show
  the first line of a 4 KiB body; that is a UI decision made locally and it does
  not change what was transmitted or stored.

### 11.3 Encoding, and unknown values

* Protobuf refuses to decode a `string` field that is not valid UTF-8, so
  invalid encodings never reach the capability — the same property
  `clipboard.v1` relies on.
* **NUL (U+0000) is refused** in every string field, for the reason
  [CLIPBOARD.md](../../architecture/CLIPBOARD.md) gives: it cannot be carried
  faithfully end to end and any consumer touching a C string API truncates at it
  silently.
* Control characters are **stripped for display**, not refused, reusing the
  existing `sanitize_device_name` / `TrustStore.sanitizeDeviceName` discipline.
  `app_label`, `title` and `body` are attacker-controlled strings that reach a
  terminal and a notification popup ([THREAT_MODEL.md T17](../../security/THREAT_MODEL.md)).
* `body-markup`: GNOME advertises it, so a body containing `<b>` would be
  *interpreted*. The Linux sink therefore **escapes** the body before passing it
  to `Notify` — a notification whose text happens to contain a `<` must not be
  able to forge emphasis or break the markup parser.
* **Unknown enum values resolve to the most conservative option, not to
  `UNSPECIFIED`-as-permissive:** unknown `NotificationPrivacy` → treat as
  `SECRET` (do not display); unknown `NotificationImportance` → `NORMAL`;
  unknown `NotificationCategory` → `OTHER`; unknown `NotificationRole` →
  ignored, so a future role cannot be assumed to be granted.
* **Unknown fields are preserved and ignored** (proto3 default). A v1 peer
  receiving a message from a hypothetical richer implementation must not fail.

---

## 12. Compatibility

`notifications.v1` requires **no change to `envelope.proto` or `core.proto`**.
It adds one file under `protocol/proto/anyflow/v1/capabilities/`, one
`Capability` implementation per side, and one registry entry — the four-step
recipe in [OVERVIEW.md "Extending it"](../../architecture/OVERVIEW.md).

| Situation | Behaviour |
| --- | --- |
| Old peer, no `notifications.v1` in `HELLO` | Not in the negotiated set. Nothing is ever sent to it. `battery.v1`, `files.v1`, `clipboard.v1` continue untouched |
| Old peer receives a `notifications.v1` `CapabilityMessage` anyway | Existing behaviour: **non-fatal** `ERROR{UNSUPPORTED_CAPABILITY}`, session survives |
| New peer, capability advertised but not granted | `NOT_AUTHORIZED`, non-fatal. Grant is re-checked per message, so revocation bites immediately |
| New peer, granted but no `SINK` role | `REJECTED_ROLE`. Nothing displayed, nothing stored |
| Breaking schema change later | Ships as `notifications.v2`; both may be advertised during migration. There is no version field inside the capability ([ADR-0008](../../adr/ADR-0008-capability-architecture.md)) |

No protocol version bump. No global break. Nothing about this capability is
visible to a peer that does not want it.

---

## 13. Failure semantics — fail closed

| Condition | Behaviour |
| --- | --- |
| Peer disconnected | Nothing queued for later delivery. Mirrors held `RECONNECT_GRACE`, then closed (§7.1) |
| `notifications.v1` not granted | `NOT_AUTHORIZED`. No content is parsed beyond what identifies the message, nothing held, nothing displayed |
| Grant revoked mid-session | Effective **immediately**, on the next message, from the trust store — never from a set captured at handshake ([THREAT_MODEL.md T5](../../security/THREAT_MODEL.md)). All mirrors for that peer are closed at once |
| Android notification access revoked by the user | `onListenerDisconnected()` → announce roles without `SOURCE` → the sink closes every mirror for that peer. The phone stops sourcing before it stops being connected |
| Listener never bound / access never granted | `SOURCE` is never advertised. The desktop shows "this device is not sharing notifications", not an empty screen |
| Linux D-Bus / notification server unavailable | `SINK` is **not advertised at all** — detected once at startup, exactly as the clipboard backend is ([ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md)). Peers never send content that would be dropped |
| Notification server disappears mid-session | `UNAVAILABLE` for each message; role re-announced without `SINK`; reconnect with backoff, never a spin |
| Display rejected by the server | `FAILED`. No retry loop |
| Malformed payload | `INVALID`, non-fatal, logged as a type with no content ([ADR-0008](../../adr/ADR-0008-capability-architecture.md)) |
| Bad-length `notification_id` | Refused and **not answered** |
| Unknown field / unknown enum | Ignored / conservative default (§11.3). Never a session error |
| Oversized text | `TOO_LARGE`. Never truncated by the receiver (§11.2) |
| Rate limit reached | `RATE_LIMITED`, non-fatal, oldest non-terminal upsert dropped first (§10.3) |
| Dismissal of a non-dismissible notification | `NOT_DISMISSIBLE`. `isClearable()` is checked at the **source**, which is the only place that knows |
| Dismissal of an unknown or already-gone notification | `UNKNOWN_NOTIFICATION`. Idempotent, not an error |
| Source locked, policy says reduce | Content is reduced **at the source**. It never enters the message, so there is nothing on the wire to leak (§ [01 §6](01-FUNCTIONAL-SPECIFICATION.md)) |
| Lock state unknown on the sink | **Treated as locked.** A privacy control that fails open is not a control ([00 §2.5](00-RESEARCH-FINDINGS.md)) |

Nothing in this table widens a permission, and nothing retries into a loop.
