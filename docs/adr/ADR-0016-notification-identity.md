# ADR-0016 — `notifications.v1` opaque notification identity

**Status:** Accepted · 2026-09-08

Canonical record for how a mirrored notification is *named* on the wire.
[ADR-0015](ADR-0015-notification-access.md) decides whether AnyFlow may read
notifications at all and under what contract; this ADR decides what a
notification is called once it may. The two do not overlap.

## Context

A mirror needs a name. Six things depend on getting it right, and they pull in
different directions:

* an update must **replace** its predecessor rather than stack beside it;
* a duplicate re-send must be **suppressed**;
* a reconnect must **reconcile** rather than duplicate the whole shade;
* a removal must target the **right mirror**;
* a dismissal must target the **right source notification**;
* two devices, and two apps, must not **collide**.

And one thing must not happen: the name must never be derived from the
notification's own text. An app that edits a message in place would otherwise
produce a second mirror, and two genuinely distinct alerts that happened to read
the same would merge into one.

### What the platform offers

Android's `StatusBarNotification.key` is `userId|pkg|id|tag|uid`. It is stable
across updates, unique across apps and profiles, and it is exactly the argument
`cancelNotification` takes. It satisfies every functional requirement above.

It is also the wrong thing to put on a wire. It contains:

| Component | What it discloses | Needed at the sink? |
| --- | --- | --- |
| `userId` | personal vs work profile, as a number | No |
| `uid` | the app *installation*'s kernel uid | No |
| `id`, `tag` | app-internal notification slots | No |
| `pkg` | which application | **Yes** — but it already travels in `app_id` |

Three of the five components would arrive at the desktop for every notification
of every app, for ever, having never had a destination-side purpose.

## Decision

### 1. The transmitted identity is derived, opaque and fixed-width

```text
notification_id = HMAC-SHA256(
    key = device_notification_secret,
    msg = "anyflow/notifications.v1/id/v1" || len32(platform_key) || platform_key
)[0..16]
```

* `len32` is a big-endian `uint32`, the same length-prefixing convention as the
  pairing proof and the `files.v1` data-stream MAC, for the same reason:
  concatenation must be unambiguous.
* 16 bytes = 128 bits, the same width as `event_id` and `transfer_id`.

**The raw Android key is never transmitted.** Not truncated, not hashed
in-place, not sent alongside. `NotificationUpsert`, `NotificationRemove` and
`DismissRequest` each carry `notification_id` and no platform identifier of any
kind, and there is no schema field that could hold one.

### 2. Derived, not random — and that choice is load-bearing

A random id would need a persistent map to stay stable across an Android
process restart. Losing that map means every notification arrives at the
desktop as a *new* one, and the reconnect snapshot duplicates the entire
notification shade — the exact "reconnect explodes into duplicates" failure this
design exists to avoid.

With a derived id the source rebuilds the map by walking
`getActiveNotifications()` on `onListenerConnected` and re-deriving. Ids survive
a process restart, and the snapshot reconciles in place. That is the difference
between a reconnect being invisible and a reconnect being a wall of duplicates.

The map is `notification_id → platform key`, held **in memory only**, rebuilt on
connect. It holds identities and never content.

### 3. 128-bit truncation

Truncated HMAC-SHA256 is standard practice, and the security argument here is
weaker than the usual one because **the id authenticates nothing**. It is a
name, not a credential. The live population is capped at 200 mirrors per peer
and a 100-entry snapshot, so collision probability at 128 bits is negligible by
many orders of magnitude. Nothing is gained by carrying 32 bytes where 16 will
do, and a shorter id is a smaller thing to log by accident.

### 4. `device_notification_secret` — what it is, and is not

32 CSPRNG bytes, generated once per install, stored beside the device identity
with the same file-mode discipline (0600 in a 0700 directory; on Android, the
existing encrypted store).

**It is not a credential and it authenticates nothing.** It is not the identity
key, it is not derived from it, and no protocol message proves knowledge of it.
Its only job is to make the transmitted id reveal nothing about the platform
key. A peer that somehow learned it could compute ids for notifications it
already receives, which is not an escalation.

### 5. Lifetime and rotation

| Event | Secret | Ids | Why |
| --- | --- | --- | --- |
| App or process restart | **Unchanged** — persisted | Unchanged | The whole reason the id is derived (§2) |
| Device reboot | **Unchanged** | Unchanged | As above |
| Secret file missing or unreadable | **Regenerated** | All change | Fail *forward*: a lost secret must not disable the capability. It is not a credential |
| **Device identity reset or re-pair** | **Destroyed and regenerated** | All change | See below |
| Grant revoked, then re-granted | **Unchanged** | Unchanged | Revocation is not an identity event |

**Why the secret must not outlive an identity reset.** `notification_id` is
`HMAC(secret, key)`, and `key` contains the app's `pkg` and `uid`, both stable
for the life of an install. A peer that recorded ids before a re-pair and saw
the same ids afterwards could link the old identity to the new one — the exact
correlation re-pairing exists to break. Rotating the secret with the identity
costs nothing, because a reset already invalidates every mirror when the peer
fingerprint changes, and it closes the link.

### 6. Identity reset is a mirror reset, never a reconciliation

A regenerated secret is **not** something to map across. The source does not
attempt to translate old ids to new ones: it cannot, and trying would require
retaining state across the very event that was supposed to clear it.

The ordinary reconnect flow does the work instead. A `SyncMarker{BEGIN}` …
`{END}` bracket names the currently-active notifications under their new ids,
and the sink removes every mirror for that peer not named in the snapshot. The
old mirrors close through a mechanism that already exists for another reason.

### 7. The sink namespaces by the pinned fingerprint, never by a claim

```text
(peer_fingerprint, notification_id)  →  local notification handle
```

**`peer_fingerprint`, not `origin_device_id`.** The pinned TLS identity is the
only thing that decides who a message is from — the same rule
[CLIPBOARD.md](../architecture/CLIPBOARD.md) states for
`ClipboardUpdate.origin_device_id`: *"it is NOT identity and is never an
authorization input."*

`origin_device_id` is carried for display and for future multi-hop reasoning. A
peer that lies in it gains nothing, because it can only ever address mirrors
inside its own namespace. This is what makes "ids from two Android devices
cannot collide" true **structurally** — two devices are two fingerprints — and
the per-install secret makes it true a second time, independently.

"Ids from two apps cannot collide" comes from `pkg` and `uid` being inside the
hashed input, and from HMAC-SHA256 not colliding at 128 bits.

### 8. What the derived id hides, and what it does not

Stated explicitly, because an HMAC is easy to over-read.

| Fact | Reaches the sink? | How |
| --- | --- | --- |
| Android `userId` (personal vs work profile) | **No** | Hashed away. Profile membership is expressed only by the boolean `secondary_profile`, and by a filter applied at the source |
| App `uid` | **No** | Hashed away |
| Notification `id` and `tag` | **No** | Hashed away |
| The **package name** | **Yes — deliberately** | The separate `app_id` field |

The sink must be able to say which app a notification came from, and to apply
per-app rules; `app_id` is how. **The derived id must never be described as
anonymising the source app.** It removes the identifiers that had no
destination-side purpose, and nothing more.

### 9. Collisions and unmappable ids fail closed

* A `DismissRequest` naming an id the source cannot map answers
  `UNKNOWN_NOTIFICATION`. It is not an error — the notification may have been
  dismissed on the phone a moment ago, and both sides converging on "it is
  gone" is the correct outcome.
* The message is not a lookup oracle: the id space is 128 bits of HMAC output,
  and a wrong guess is indistinguishable from a notification that has already
  been removed.
* An id of any width other than 16 bytes is refused **and not answered**. A
  `NotificationResult` echoes the id, so a malformed one leaves nothing
  coherent to correlate a reply with — the same rule `clipboard.v1` applies to
  a bad-width `event_id`.
* Identifiers are **never truncated to fit**. Display text may be shortened at
  the source; an identifier may not, because shortening an identifier is how
  collisions are manufactured.

### 10. Dismissal requires a source-side mapping, and that is the only reverse path

The only message that names a notification back to its source is
`DismissRequest`, and it names it by `notification_id`. The source maps back to
the platform key locally and calls `cancelNotification(key)`.

The destination therefore never needs the raw key, and the mapping is
*reconstructible* precisely because the id is derived (§2) — which is what keeps
dismissal working across a source process restart without persisting anything.

### 11. `content_hash` is not identity

```text
content_hash = SHA-256("anyflow/notifications.v1/content/v1" || len32-prefixed semantic fields)
```

Used for three things and no others: suppressing a re-send when a source
re-posts an identical notification, loop detection, and diagnostics that must
not log content. It is **not** authentication — TLS and the grant are — and it
is **not** identity. Two notifications with the same title and body from the
same app are two notifications if the platform says so, and one notification
whose body changed is still the same notification.

### 12. Where the derivation lives

The derivation needs the source device's secret, so it belongs to the source
platform adapter and ships in **N1**, with the Android listener.

**N0 implements the protocol type and its rules only**: the exact width, the
validation, the refusal semantics, the fail-closed answers, and the tests. It
manufactures no Android implementation in portable code, and defines no secret.
`anyflow_core::notifications::NotificationId` is deliberately opaque — it can be
constructed from 16 bytes and compared, and it knows nothing about how those
bytes were produced.

## Consequences

* **A desktop can never learn an app's uid or a work-profile number** from a
  notification, and gains no new fingerprinting surface as apps are installed.
* **A reconnect is invisible** in the normal case, because ids survive a
  process restart on the source.
* **A re-pair resets every mirror**, which is correct and is not a regression to
  work around: the old ids were linkable and the new ones must not be.
* **The source must hold a map** for as long as it sources notifications. It is
  in memory, it holds identities, and it is rebuilt rather than persisted.
* **A future platform inherits this unchanged.** The derivation takes "the
  platform's own stable notification key" as input; Windows and macOS each have
  one, and neither needs a schema change.
* **The 16-byte width is now a compatibility constant.** Changing it is a
  `notifications.v2` decision, and the field-set regression test in
  `anyflow-proto` pins it.

### What this costs

The sink cannot correlate a notification across a re-pair, or across a lost
secret. That is the property being bought, not a defect — but it does mean a
support question ("why did all my notifications reappear?") has an answer that
is only satisfying if someone remembers this file exists.

## Notes

* Wire schema and field limits:
  [02 §5](../research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md) and
  `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto`.
* Review record: [the decision report](../../NOTIFICATIONS-V1-DECISION-REPORT.md) §6.
* Roles and the runtime narrowing mechanism: [ADR-0017](ADR-0017-capability-roles.md).
* The access contract this operates under: [ADR-0015](ADR-0015-notification-access.md).
