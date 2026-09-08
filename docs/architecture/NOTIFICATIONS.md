# `notifications.v1`

**Status after N0: protocol only. Nothing mirrors a notification yet.**

This document describes the whole design and marks, in every section, what
exists today and what is future work. It summarises rather than restates the
research corpus: the authority for the wire format is
[02](../research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md), for the
access contract [ADR-0015](../adr/ADR-0015-notification-access.md), for naming
[ADR-0016](../adr/ADR-0016-notification-identity.md), and for roles
[ADR-0017](../adr/ADR-0017-capability-roles.md).

## What exists after N0

| Thing | State |
| --- | --- |
| `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto` | **Exists.** Compiled by both toolchains |
| `anyflow_core::notifications` — limits, validation, roles, snapshot framing | **Exists.** Portable, pure functions of decoded messages |
| Capability id `notifications.v1` | **Defined**, registered by nobody |
| Android `NotificationListenerService` | **Does not exist.** The manifest is unchanged |
| Linux notification sink, D-Bus code | **Does not exist** |
| `NotificationSink` trait | **Does not exist** — deliberately, see below |
| Settings UI, app picker, permission flow | **Does not exist** |
| Grants, policy, filtering, lock policy | **Does not exist** |
| Any runtime that mirrors or dismisses anything | **Does not exist** |

Because nothing registers the capability, `notifications.v1` never appears in a
`HELLO`, is never negotiated, and is invisible to every peer. `battery.v1`,
`files.v1` and `clipboard.v1` behave exactly as they did.

## The shape, end to end

```text
   ┌─────────────────────────────── Android phone ───────────────────────────────┐
   │  NotificationListenerService            (N1 — does not exist yet)           │
   │        │  onNotificationPosted / Removed / ListenerConnected                │
   │        ▼                                                                    │
   │  filter · lock policy · SECRET drop · own-package drop     (N1, N3)         │
   │        │                                                                    │
   │        ▼                                                                    │
   │  identity derivation: HMAC(secret, platform key)[0..16]    (N1, ADR-0016)   │
   │        │                                                                    │
   │        ▼                                                                    │
   │  NotificationsCapability — encode                          (N1)             │
   └────────┼────────────────────────────────────────────────────────────────────┘
            │  NotificationControl, ≤ 8 KiB, as CapabilityMessage.payload
            ▼
   ┌──────────────── the existing authenticated control session ────────────────┐
   │  Envelope · replay guard · u32 length prefix · MAX_FRAME_LEN 64 KiB        │
   │  TLS 1.3, mutual auth, SPKI pinning        (exists — unchanged by N0)      │
   └────────┼───────────────────────────────────────────────────────────────────┘
            │  one ordered stream per peer
            ▼
   ┌─────────────────────────────── Fedora desktop ──────────────────────────────┐
   │  anyflow-capability-notifications — decode, validate       (N2)             │
   │        │      uses anyflow_core::notifications             (exists now)     │
   │        ▼                                                                    │
   │  grant check · policy · dedup · MirrorTable                (N2, N3)         │
   │        │                                                                    │
   │        ▼                                                                    │
   │  NotificationSink  ◀── the platform seam                   (N2)             │
   │        │                                                                    │
   │        ▼                                                                    │
   │  org.freedesktop.Notifications over D-Bus                  (N2)             │
   └─────────────────────────────────────────────────────────────────────────────┘
```

**One channel, the existing one.** No second socket and no data stream:
`files.v1` needed one because a file does not fit in a frame and must not delay
a cancel ([ADR-0012](../adr/ADR-0012-bulk-transfer-and-frame-limit.md),
[ADR-0013](../adr/ADR-0013-file-transfer-data-stream.md)); a notification is a
few hundred bytes of text. The moment a notification carried an *image* that
would stop being true, which is one of the reasons v1 carries none.

## Ownership boundaries

| Concern | Owner | Never owned by |
| --- | --- | --- |
| Which notifications leave the phone | The **source**: filter, lock policy, `SECRET` exclusion, own-package drop | The sink. A sink cannot ask for more than it is sent |
| What a notification is called | The **source** (derivation), the **schema** (width, validation) | The sink, which treats the id as opaque |
| Who a message is from | The **pinned TLS fingerprint** | `origin_device_id`, which is display data and a lie a peer gains nothing by telling |
| Whether a peer may use the capability | The **local trust store**, re-checked per message | The peer's `HELLO`, or its roles |
| What a peer can physically do | The peer's own **roles**, announced at runtime | Anything that could be read as authorization |
| Whether a notification can be dismissed at all | The **source** (`isClearable()`) | The sink, which may only ask |
| How a mirror looks | The **sink**, locally | The source. Presentation limits are not transmission limits |
| Timers: reconnect grace, snapshot abandon, mirror ceilings | Each side, **locally** | The protocol. None is negotiated or on the wire |

## Event lifecycle

Six message bodies, and no seventh:

```protobuf
NotificationControl {
  oneof body { roles | upsert | remove | dismiss | result | sync }
}
```

**Posted and Updated are one message.** No platform AnyFlow targets has a
separate update operation — `onNotificationPosted` fires for both with the same
key; freedesktop `Notify` with `replaces_id` is the same method; re-adding a
macOS request with the same identifier replaces it. A protocol that invented the
distinction would make both ends keep the same state purely to produce and
discard a field that changes nothing. So `NotificationUpsert` means *"this is
the current state of this identity"*, and reconnect resynchronisation becomes
free: a snapshot is just upserts.

At the sink, one mirrored notification:

```text
                     upsert (new id)
        ∅ ──────────────────────────────────▶ MIRRORED
        ▲                                     │  ▲
        │                                     │  └─ upsert (same id): replace
        │  remove | dismissed by a human |    │     in place, one local handle
        │  snapshot omission | grace expiry   │
        └─────────────────────────────────────┘
```

There is no `PENDING`, no `ACKED` and no retry state. A notification is
soft-realtime data: if an upsert is lost because the link dropped, the reconnect
snapshot restores the truth, and a retry queue would only add a way to deliver
stale content late.

`NotificationRemove` carries **no reason code**. All 23 Android removal reasons
mean the same thing to a mirror — it is gone — and shipping the reason would
leak facts about the user's device while inviting an implementation to treat
some removals as "soft".

## Role lifecycle

Full reasoning in [ADR-0017](../adr/ADR-0017-capability-roles.md). In one page:

```text
connect ──▶ each side sends NotificationRoles{epoch=1} FIRST
                    │
                    ├─ Android: {SOURCE, DISMISS_TARGET}
                    └─ Linux:   {SINK, DISMISS_REPORTER}

user revokes notification access in Android Settings
                    │
                    ▼
        onListenerDisconnected()
                    │
                    ▼
        NotificationRoles{roles=[], epoch=2}   ← narrows immediately
                    │
                    ▼
        the desktop closes every mirror for that peer
```

* **Absent roles mean no roles.** The fail-closed default.
* **The epoch is strictly monotonic**, so a replayed announcement cannot
  re-widen a set that has narrowed.
* **An unknown role is ignored**, never assumed granted.
* **A role is not an authorization input.** Platform capability, peer grant and
  role are three different questions; ADR-0017 §6 tabulates why collapsing any
  two would be a consent failure.

## Privacy boundary

```text
       ┌──────────────────────────────────────────────────┐
       │ the source device                                │
       │                                                  │
       │  everything the listener sees                    │
       │        │                                         │
       │        ▼   own package · SECRET · filter ·        │
       │            work profile · lock policy            │
       │        │                                         │
       │        ▼                                         │
       │  what is encoded  ◀── the boundary is HERE       │
       └────────┼─────────────────────────────────────────┘
                │
                ▼   nothing withheld ever enters the wire
```

**Content is reduced at the source, before encoding.** A withheld body does not
exist in the message, so there is nothing on the wire to leak and nothing at the
sink to mishandle. Unlocking is not retroactive: content withheld while locked
is not delivered later.

Three rules that are not negotiable:

1. **AnyFlow does not detect sensitive content itself.** No OTP regex, no
   keyword list, no "looks like a bank" heuristic. A guess dressed as a security
   control is worse than an honest boundary — the reasoning
   [THREAT_MODEL.md](../security/THREAT_MODEL.md) T10 already applies to
   clipboard passwords.
2. **No rule assumes the platform protects us.** On the certification target,
   Android 16 / One UI 8 delivered OTP-shaped notifications to an untrusted
   listener entirely unredacted
   ([POC-NOTIF-01](../research/notifications-v1/poc/POC-NOTIF-01.md)). Platform
   redaction is a bonus, never a control, and no product copy may offer it as
   reassurance.
3. **`VISIBILITY_SECRET` is never mirrored**, at the source, unconditionally —
   not a default a settings screen can flip.

**No notification content is persisted anywhere, on either side, at any time.**
Not a file, not a table, not a ring buffer, not a "recent" screen. Not in
`state.json`. Not in a log at any level, including `TRACE`. The only
notification state that exists is what is currently active on the source and
currently displayed on the sink, both in memory.

That is structural rather than promised: `anyflow_core::notifications::Snapshot`
holds identities and has no field that could hold text, and its `Debug` is
asserted content-free by test.

## Snapshot model

```text
source                                          sink
  │  SyncMarker{sync_id, BEGIN}                   │
  │──────────────────────────────────────────────▶│
  │  NotificationUpsert × N  (each active,        │
  │      filter- and policy-passing               │
  │      notification, right now)                 │
  │──────────────────────────────────────────────▶│
  │  SyncMarker{sync_id, END}                     │
  │──────────────────────────────────────────────▶│
  │                          sink removes every    │
  │                          mirror for this peer  │
  │                          not named in between  │
```

**An active-state snapshot is not a history**, and the distinction is precise:

| | Active-state snapshot | Notification history |
| --- | --- | --- |
| Contents | Only what is in the source's shade at that instant | Things that existed at some point |
| Lifetime | Discarded as soon as it is applied | Retained |
| Storage | Never written to disk on either side | Written somewhere by definition |
| Already visible to the user? | Yes — they are on the phone's screen | Not necessarily |

A history is not a feature deferred for time; it is one the design forbids
([ADR-0015 §1 clause 8](../adr/ADR-0015-notification-access.md)).

Failure handling, all fail-safe:

* an `END` with no `BEGIN` is **refused**, never read as "remove everything";
* an `END` whose `sync_id` does not match the open `BEGIN` is **refused**, and
  the open snapshot survives it;
* a second `BEGIN` **abandons** the open snapshot without applying it — an
  incomplete snapshot can never be completed, so it must never remove anything;
* a duplicate item is idempotent;
* upserts outside a snapshot are ordinary **live** traffic, not an error.

**The snapshot path grants a peer no authority it does not already have.** It is
a reconciliation mechanism, not a privileged one: everything it can do, ordinary
upserts can do.

**Which numbers are protocol, and which are not.** Only the `BEGIN`/`END`
bracketing and the remove-what-is-not-named rule are protocol. The reconnect
grace, the snapshot abandon timeout, the per-peer mirror ceiling and the
snapshot entry cap are **local tunables**: never negotiated, never on the wire,
and changing any of them cannot break interoperability. What is normative about
the grace is only its bound — greater than zero, or a Wi-Fi blip clears the
desktop and re-posts everything; and finite, or a departed phone leaves
notifications on a screen that can no longer update or dismiss them.

## Ordering — an assumption that was checked, not assumed

The snapshot design depends on in-order delivery: a notification removed while
the snapshot is being built must have its `NotificationRemove` applied *after*
the upsert that carried it, so both sides converge on "removed".

**The existing transport provides this**, at three points, all in
`desktop/core/src/session.rs`:

1. TLS 1.3 over TCP is an ordered byte stream, so frames arrive in the order
   they were written.
2. One reader task parses frames sequentially into a bounded FIFO channel, and
   the dispatch loop **awaits** `on_message` before taking the next message —
   so two messages for one capability are never in flight at once. Pinned by
   `session::dispatch_tests::inbound_capability_messages_reach_the_handler_in_wire_order`.
3. Exactly one writer task owns the write half and the envelope factory, and it
   drains a single FIFO channel for capability output — so sequence numbers are
   assigned in the order frames actually go out. Pinned by
   `replies_keep_their_order_under_queue_pressure`.

**One constraint this places on N1 and N2.** The outbound capability channel is
an `mpsc` whose sender is cloned; FIFO is guaranteed *per producer*, not across
producers. A notification capability must therefore produce its outbound
messages from a **single ordered producer** — one task, or one lock — or a
removal could overtake the upsert it refers to. This is a requirement on the
implementation, not a gap in the transport, and it is the reason per-identity
coalescing is a *slot* rather than a queue.

The one honest gap: if the session drops between an upsert and its removal, the
sink holds a mirror the source no longer has. The grace timer closes it, and the
next reconnect's snapshot does not name it. A stale mirror for at most one
grace window, never permanently.

## Dismissal model

```text
  a human closes the mirror on the desktop
        │
        ▼  NotificationClosed(id, reason = 2)   ← reason 2 ONLY
  sink → DismissRequest{notification_id}
        │
        ▼  source: id → platform key, then cancelNotification(key)
  the platform removes it and reports the removal
        │
        ▼  source → NotificationRemove to every OTHER granted peer,
           and NOT to the peer that asked          ← echo suppression
```

`DismissRequest` is **the only message that travels sink → source and causes an
effect on the source device**, and it maps to exactly one platform call. It
carries no action index, no intent, no payload, no free text and no reply.

There is nothing in it that could be widened into remote action execution
*because there is no field to widen* — the guarantee is the shape of the
message, not a check that a later change could invert. A descriptor-level
regression test (`anyflow-proto`, `notifications_schema`) fails if a field is
added to it, if any field name hints at an action or a reply, or if any `bytes`
field appears in the schema that is not one of the four fixed-width identifiers.

Rules: only reason 2 (a human dismissed it) may cancel at the source — expiry
must never dismiss, or a desktop banner timing out would clear someone's phone
every time they walked away. Dismissal is idempotent, and dismiss-sync is opt-in
per peer, default off ([ADR-0015 §6](../adr/ADR-0015-notification-access.md)).

## The platform seams

### `NotificationSink` — **N2 owns this, and N0 deliberately did not create it**

Wave 0 declined to create a `NotificationSink` before a real capability required
one, on the grounds that AnyFlow implemented no notifications anywhere and there
was nothing to abstract. That is no longer true, but the seam still does not
belong in N0: **an abstraction with no implementation on either side of it is a
guess about a shape**, and the shape is exactly what writing the first D-Bus
sink will teach.

The implementation plan places it with that first implementation. Its future
location, fixed now so N2 does not have to rediscover it:

```text
desktop/capabilities/notifications/src/backend/mod.rs   ← the trait
desktop/capabilities/notifications/src/backend/dbus.rs  ← the Linux impl
```

modelled on `ClipboardBackend`, roughly:

```rust
trait NotificationSink {
    fn display(&self, n: &Mirror, replaces: Option<u32>) -> Result<u32>;
    fn close(&self, id: u32) -> Result<()>;      // a "no such id" error IS success
    fn closed_events(&self) -> ClosedStream;     // (id, reason)
    fn describe(&self) -> String;
    fn availability(&self) -> Result<(), Unavailable>;
}
```

When it is created it must be a pure portable abstraction: no D-Bus, no GTK, no
`target_os`, and it must keep `desktop/core/tests/portable_boundary.rs` green
with the new crate listed among the portable ones.

### The portable half that N0 *did* create

`anyflow_core::notifications` holds the wire contract: field limits, identifier
widths, the role/epoch reduction, the snapshot bracketing machine, and
conservative enum resolution. It is a pure function of decoded protobuf
messages — no I/O, no timers, no policy, no platform.

It lives in `anyflow-core` beside `clipboard_policy.rs`, which is the existing
precedent for a portable capability-adjacent type in that crate, and which the
plan follows again for `notification_policy.rs`. N2's capability crate
re-exports it rather than reimplementing it, so the two ends of the protocol
cannot drift.

### Cross-language lockstep

Both toolchains compile the same `.proto` — `protox` for Rust in
`desktop/proto/build.rs`, the Android protobuf plugin from the same
`protocol/proto` directory. Beyond compiling it, both sides assert the same
canonical encoded vector byte for byte
(`CANONICAL_UPSERT_HEX` in `desktop/core/tests/notifications_protocol.rs`,
`canonicalUpsertHex` in `NotificationsProtocolTest.kt`), so a field number, wire
type or enum value that drifted on one side fails a test rather than a user.

## Wave boundaries

| Wave | Adds | State |
| --- | --- | --- |
| **N0** | Schema, portable contract, ADR-0016, ADR-0017, this document, tests | **Done** |
| **N1** | Android `NotificationListenerService`, extraction, identity derivation, filter, the manifest service | Future |
| **N2** | Linux sink crate, `NotificationSink` seam, D-Bus display, lock source | Future |
| **N3** | Grants, per-app filter, privacy UI on both ends, CLI | Future |
| **N4** | Dismissal synchronisation and echo suppression | Future |
| **N5** | Reconnect, snapshot runtime, rate limiting, hardening | Future |
| **N6** | Hardware certification on SM-X620 ↔ Fedora 44 | Future |

N1 and N2 are independent after N0 and share only the `.proto`. N3 needs both,
because the consent surface has two ends. N4 needs N3, because dismissal
without a grant surface is untestable.

## What is deliberately not planned

Reply and actions · icons and images · Linux as a source · Android as a sink ·
Windows, macOS or iOS implementations · notification history in any form ·
a `notifications.v2`.

Each is a separate decision with its own threat model.
[04](../research/notifications-v1/04-PLATFORM-CAPABILITY-MATRIX.md) explains why
the protocol will not need reshaping to accommodate the ones that eventually
happen.
