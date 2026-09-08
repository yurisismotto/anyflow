# 05 — `notifications.v1` implementation plan

| Field | Value |
| --- | --- |
| **Title** | Waves N0 – N6, scoped to this repository |
| **Status** | Plan — nothing implemented |
| **Last reviewed** | 2026-09-08 |
| **Related** | [01](01-FUNCTIONAL-SPECIFICATION.md), [02](02-PROTOCOL-AND-EVENT-MODEL.md), [03](03-PRIVACY-SECURITY-THREAT-MODEL.md), [06](06-OPEN-QUESTIONS-AND-POCS.md) |

---

## 0. Preconditions

**No implementation wave may start while a P0 question in
[06](06-OPEN-QUESTIONS-AND-POCS.md) is open.**

> **Status 2026-09-08: the N0 preconditions are met.** The decision sprint
> closed OQ-01, OQ-02, OQ-03 and OQ-04, ran POC-NOTIF-01 and POC-NOTIF-02 on
> the certification hardware, and left **no BLOCKER and no open P0 architecture
> or security question**. See
> [ADR-0015](../../adr/ADR-0015-notification-access.md) and
> [the decision report](../../../NOTIFICATIONS-V1-DECISION-REPORT.md).

The two questions that gated N0 itself, and how they closed:

* **OQ-01** — the manifest and threat-model stance that forbade
  `BIND_NOTIFICATION_LISTENER_SERVICE` is **amended**, under the explicit
  security contract in [ADR-0015 §1](../../adr/ADR-0015-notification-access.md).
* **OQ-04** — **CDM is not adopted in v1**
  ([ADR-0015 §10](../../adr/ADR-0015-notification-access.md)), so nothing in
  this plan may introduce a CompanionDeviceManager association.

`POC-NOTIF-01` ran ahead of N0 rather than during it. Its answer — platform OTP
redaction is **absent** on the certification target
([evidence](poc/POC-NOTIF-01.md)) — changes product copy and residual risk, and
changes nothing in the architecture, exactly as predicted.

Two decisions from the sprint bind waves later in this plan and are easy to
implement from memory of the *old* text, so they are restated here:

* **N3** — `allow_dismiss_sync` defaults **off**, not on
  ([ADR-0015 §6](../../adr/ADR-0015-notification-access.md)).
* **N2** — the sink's authoritative lock source is `logind LockedHint`;
  `org.gnome.ScreenSaver.ActiveChanged` is a hint whose boolean is discarded
  ([POC-NOTIF-02](poc/POC-NOTIF-02.md)).

---

## N0 — Protocol, ADRs and the decision to build it

**Scope.** Write the schema, reverse the documented stance, and change no
behaviour.

| Deliverable | Path |
| --- | --- |
| Capability schema | `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto` |
| ~~**ADR-0015** — Notification access and the manifest stance~~ | `docs/adr/ADR-0015-notification-access.md` — **written and Accepted, 2026-09-08** |
| **ADR-0016** — Notification identity and update semantics | `docs/adr/ADR-0016-notification-identity.md` |
| **ADR-0017** — Capability roles inside a capability | `docs/adr/ADR-0017-capability-roles.md` |
| Architecture doc | `docs/architecture/NOTIFICATIONS.md` |
| Threat-model amendment | `docs/security/THREAT_MODEL.md` — T26 rewritten, T-N01…T-N16 folded in |
| Protocol doc | `docs/architecture/PROTOCOL.md` — a `notifications.v1` section beside `clipboard.v1`'s |
| ADR index | `docs/adr/README.md` |

ADR-0015 is the load-bearing one. It must state what changes (the manifest gains
one system-held permission), what does **not** (no accessibility service, no
default IME, no `QUERY_ALL_PACKAGES`, no `SYSTEM_ALERT_WINDOW`, no root, no
hidden APIs), and why the README's *"No root, no accessibility service, no ADB,
no hidden permissions"* survives unchanged
([03 §T-N16](03-PRIVACY-SECURITY-THREAT-MODEL.md)).

ADR-0017 is worth its own record because it is a *pattern*, not a feature: it is
the first time a capability negotiates anything of its own, and the reasoning —
roles change at runtime, so they cannot live in `HELLO`
([02 §3.1](02-PROTOCOL-AND-EVENT-MODEL.md)) — will apply again.

**Tests.** `protox` compiles the new `.proto` in the existing build; the Android
`protobuf` plugin generates from the same file. A round-trip encode/decode test
on each side.

**Security gates.** The schema contains no field capable of holding a
`PendingIntent`, an action, a `RemoteViews` or a serialized platform
notification — asserted by review against
[02 §6.6](02-PROTOCOL-AND-EVENT-MODEL.md), and by a test that the generated
Rust type has exactly the expected field set.

**Exit criteria.** ADRs merged. `docs/architecture/NOTIFICATIONS.md` exists and
is linked from `OVERVIEW.md`. Both toolchains generate from the same file.
`cargo test` and the Android suite are green with **no behaviour change** —
nothing is registered yet.

---

## N1 — Android source adapter

**Scope.** Observe, filter, derive identity, encode. No UI beyond what is needed
to turn it on, no dismissal handling yet.

| Area | Files |
| --- | --- |
| Listener | `android/app/src/main/java/…/notifications/AnyFlowNotificationListener.kt` |
| Extraction and text rules | `…/notifications/NotificationSnapshot.kt`, `NotificationText.kt` |
| Identity derivation + key map | `…/notifications/NotificationIdentity.kt` |
| Filtering | `…/notifications/NotificationFilter.kt` |
| Policy type | `…/notifications/NotificationPolicy.kt` |
| Capability | `…/capability/NotificationsCapability.kt` |
| Bind lifecycle | `…/service/ConnectionService.kt` (request/release the listener binding) |
| Trust store | `…/store/TrustStore.kt` — `notificationPolicy`, `notificationPolicyFor` |
| Manifest | `android/app/src/main/AndroidManifest.xml` — the service, the metadata, and the rewritten comment block |

Design points that are easy to get wrong and must be in review:

* Callbacks run on the **main thread**. The listener extracts a plain data
  object and hands it to a coroutine; no protobuf, no hashing, no I/O on that
  thread ([00 §1.2](00-RESEARCH-FINDINGS.md)).
* The listener is bound **on demand** (`META_DATA_DEFAULT_AUTOBIND = false`,
  `requestRebind` / `requestUnbind`) and only while a granted peer is connected
  ([01 §4](01-FUNCTIONAL-SPECIFICATION.md)).
* Drop AnyFlow's own package first, before every other check
  ([02 §9.4](02-PROTOCOL-AND-EVENT-MODEL.md)).
* `getActiveNotifications()` on `onListenerConnected` rebuilds the id map
  ([02 §5.2](02-PROTOCOL-AND-EVENT-MODEL.md)).

**Tests (JVM).** Identity derivation against the pinned cross-language vector;
`key` parsing; filter decisions including work profile, ongoing, own-package and
`SECRET`; text rules (NUL, UTF-8 boundary truncation, control stripping);
importance and category mapping; the id map rebuild; coalescing behaviour.

**Hardware gates.** `NOTIF-HW-01`: on the SM-X620, grant access and confirm that
posting a notification produces exactly one encoded upsert with the expected
identity, and that updating it produces a second upsert **with the same id**.

**Security gates.** NOTIF-SEC-10, -11, -13, -14, -15, -17, -18, -19
([03 §5](03-PRIVACY-SECURITY-THREAT-MODEL.md)).

**Exit criteria.** With no peer granted, the listener is **not bound** —
verified with `dumpsys notification`. Android JVM suite green. No notification
content in logcat at any level.

---

## N2 — Linux notification sink

**Scope.** Display, update in place, close, and the platform seam Wave 0
deliberately did not create
([platform-expansion 28](../platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md):
*"AnyFlow implements no notifications on any platform. Nothing to abstract"* —
no longer true).

| Area | Path |
| --- | --- |
| New crate | `desktop/capabilities/notifications/` (`anyflow-capability-notifications`) |
| Portable half | `src/lib.rs`, `src/identity.rs`, `src/text.rs`, `src/limits.rs`, `src/dedup.rs`, `src/policy.rs`, `src/redact.rs` |
| Backend seam | `src/backend/mod.rs` — `NotificationSink` trait, mirroring `ClipboardBackend` |
| Linux impl | `src/backend/dbus.rs` (default-on feature), `src/backend/lock.rs` |
| Policy type | `desktop/core/src/notification_policy.rs`, re-exported by the capability |
| Trust store | `desktop/core/src/store.rs` — `TrustedPeer.notification_policy`, `#[serde(default)]` |
| Wiring | `desktop/daemon/src/main.rs`, `desktop/runtime/src/state.rs` |
| Workspace | root `Cargo.toml`, `desktop/core/tests/portable_boundary.rs` |

The `NotificationSink` trait follows the `ClipboardBackend` template — the
capability above it deals in its own types and would work unchanged on another
platform:

```rust
trait NotificationSink {
    fn display(&self, n: &Mirror, replaces: Option<u32>) -> Result<u32>;
    fn close(&self, id: u32) -> Result<()>;          // a "no such id" error IS success
    fn closed_events(&self) -> ClosedStream;         // (id, reason)
    fn describe(&self) -> String;
    fn availability(&self) -> Result<(), Unavailable>;
}
```

Rules that came out of the research and must not be lost in implementation:

* Detect the server **once at startup**, report honestly when absent, and do not
  advertise `SINK` if there is none ([ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md)
  discipline; [02 §13](02-PROTOCOL-AND-EVENT-MODEL.md)).
* **Never poll.** Lock state comes from `logind LockedHint` plus
  `org.gnome.ScreenSaver.ActiveChanged`, and **not** from
  `org.freedesktop.ScreenSaver`, which on GNOME serves idle-inhibit and refuses
  `GetActive` ([00 §2.5](00-RESEARCH-FINDINGS.md)).
* Treat a `CloseNotification` error as success ([00 §2.3](00-RESEARCH-FINDINGS.md)).
* Drop the mirror-table entry on `NotificationClosed` — the id is invalidated
  before the signal is sent ([02 §8](02-PROTOCOL-AND-EVENT-MODEL.md)).
* Escape body markup before `Notify` ([02 §11.3](02-PROTOCOL-AND-EVENT-MODEL.md)).
* Never emit `urgency = 2` ([02 §6.1](02-PROTOCOL-AND-EVENT-MODEL.md)).

**Tests.** `capabilities/notifications/tests/` — identity, text, dedup, mirror
table, id invalidation on close, markup escaping, importance mapping, unknown
enum conservatism. `desktop/daemon/tests/notifications.rs` drives the capability
over real TLS with real pinning and the real trust store, with a fake sink.
`portable_boundary.rs` must list the new crate and it must compile with
`--no-default-features` for a non-Unix target.

**Security gates.** NOTIF-SEC-01…09, -12, -16, -20, -21, -27.

**Exit criteria.** `anyflow notifications status` reports the real server, the
real capability list and the real lock source. `--no-default-features` build of
the new crate is clean on `x86_64-pc-windows-msvc`. Rust suite green.

---

## N3 — Grants, filters, privacy UI

**Scope.** The consent surface — the part that decides whether this feature is
defensible.

| Area | Path |
| --- | --- |
| Android settings | `…/ui/NotificationSettingsScreen.kt`, `…/ui/AppPickerScreen.kt`, `…/ui/PeerDetailScreen.kt` |
| Permission flow | `ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS` + component extra; `isNotificationListenerAccessGranted` polling **only** on resume |
| Strings | `android/app/src/main/res/values/strings.xml` |
| CLI | `desktop/cli/src/main.rs` — `notifications status|policy|clear`, `grant` |
| GUI | `desktop/gui/src/views/devices.rs`, `dashboard.rs` |
| Control contract | `desktop/control/src/lib.rs` |

**Tests.** Policy defaults are the documented ones; a store written before this
capability loads with automatic behaviour off; the app list defaults empty; a
newly installed app is not shared; revocation closes mirrors. Compose UI tests
for the picker's select-all and the "N new apps" affordance.

**Security gates.** NOTIF-SEC-02, -13, -14, -20, **-25**, **-26**. NOTIF-SEC-25
is the `logging.rs` canary suite and is the gate this wave exists to pass.

**Exit criteria.** The two permissions are visibly distinct in the UI
([01 §7](01-FUNCTIONAL-SPECIFICATION.md)). No screen anywhere lists received
notifications. Granting OS access alone mirrors nothing to anyone — asserted by
a test, not by inspection.

---

## N4 — Dismissal synchronisation

**Scope.** Close the loop, and prove it does not become one.

| Area | Path |
| --- | --- |
| Sink → request | `desktop/capabilities/notifications/src/lib.rs` (reason-2 filter), `backend/dbus.rs` |
| Source → cancel | `…/notifications/AnyFlowNotificationListener.kt`, `…/capability/NotificationsCapability.kt` |
| Echo suppression | `…/notifications/NotificationCaches.kt`, `src/dedup.rs` |

**Tests.** Reason 1 and reason 3 produce **no** dismissal request — this is the
test that stops the feature clearing the user's phone every time a banner times
out. Echo suppression delivers to other peers and not to the requester.
Dismissing twice is idempotent. A non-dismissible notification is refused.
Dismissing a notification never sent to that peer is refused.

**Hardware gates.** `NOTIF-HW-02`: dismiss a mirror on the Fedora desktop, watch
it disappear from the SM-X620. `NOTIF-HW-03`: dismiss on the phone, watch the
mirror disappear. `NOTIF-HW-04`: let a mirror expire on the desktop and confirm
the phone's notification **survives**.

**Security gates.** NOTIF-SEC-04, -05, -06, -23.

**Exit criteria.** All three hardware gates executed on real hardware, not
reasoned about. No `PendingIntent` is referenced anywhere in the Android
capability — asserted by a source-level check, the way the `unsafe_code` rule is.

---

## N5 — Reconnect, rate limiting, hardening

**Scope.** The behaviours that only show up under stress.

Snapshot on `onListenerConnected`, `SyncMarker` bracketing, stale-mirror removal,
the 60 s reconnect grace, per-identity coalescing, token buckets, bounded queues,
and the `RATE_LIMITED` path.

**Tests.** A reconnect converges with **zero duplicates** and removes stale
mirrors. A flapping link (connect/disconnect × 20) leaves exactly the active set.
A 1000-update progress burst produces ≤ 2 messages/second and the final value is
always delivered. A flood never drops a removal. A saturated session still shuts
down — the `clipboard.v1` session-writer regression, repeated here because it was
found the hard way once already.

**Hardware gates.** `NOTIF-HW-05`: Wi-Fi off/on with 20 active notifications →
no duplicates. `NOTIF-HW-06`: a real download's progress notification produces
one desktop entry, not hundreds. `NOTIF-HW-07`: overnight idle with the screen
off — the listener survives One UI's app-sleep management, or the failure is
documented ([POC-NOTIF-04](06-OPEN-QUESTIONS-AND-POCS.md)).

**Security gates.** NOTIF-SEC-08, -09, -21, -22, -24.

**Exit criteria.** No duplicate mirror is reproducible by any reconnect
sequence. Memory is flat over a 12-hour soak.

---

## N6 — Hardware certification

**Scope.** Execute the gates on the real pair. Nothing new is written.

Certification target: **SM-X620, Android 16, One UI 8.0** ↔ **Fedora 44, GNOME
Shell 50.4**, both verified present on 2026-09-08.

| Gate | What it proves |
| --- | --- |
| NOTIF-HW-01…07 | The waves above, on hardware |
| NOTIF-HW-08 | Grant, use, revoke: mirrors close immediately on revoke |
| NOTIF-HW-09 | Locked phone → app name only, verified by capturing the encoded bytes |
| NOTIF-HW-10 | Locked desktop → no body on screen |
| NOTIF-HW-11 | An OTP notification behaves as POC-NOTIF-01 predicted |
| NOTIF-HW-12 | Work-profile switch off → nothing mirrored, with a profile present |
| NOTIF-HW-13 | Revoking OS notification access mid-session closes every mirror |
| NOTIF-HW-14 | `RUST_LOG=trace` + logcat over a full session: no content |

**Exit criteria.** Every gate **executed**, not inferred. Per the standing rule
from the `files.v1` sprint, an unexecuted security or integrity gate blocks
certification — a gate that could not run is reported as blocked, never as
passed.

---

## Sequencing

```text
N0 ──▶ N1 ──┬──▶ N3 ──┬──▶ N4 ──▶ N5 ──▶ N6
            │         │
      N2 ───┴─────────┘
```

N1 and N2 are independent after N0 and can run in parallel — they share only the
`.proto`. N3 needs both, because the consent UI has two ends. N4 needs N3,
because dismissal without a grant surface is untestable. N6 needs everything.

**Estimated shape**, not a schedule: N0 is small and mostly prose; N1 and N2 are
each comparable to `clipboard.v1`'s per-platform half; N3 is the largest UI wave
the project will have done; N4 is small; N5 is small in code and large in tests.

## What is deliberately *not* planned

Reply and actions · icons and images · Linux as a source · Android as a sink ·
Windows, macOS or iOS implementations · notification history in any form ·
a `notifications.v2`. Each is a separate decision with its own threat model, and
[04 §3](04-PLATFORM-CAPABILITY-MATRIX.md) explains why the protocol will not
need reshaping to accommodate the ones that eventually happen.
