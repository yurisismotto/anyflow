# `notifications.v1` — N1: the Android notification source adapter

**Wave:** N1 · **Branch:** `feature/notifications-v1-n1-android-source` ·
**Date:** 2026-09-08

**Baseline commit:** `abf47181002e68943ce55d1a6c281677d42b0af5`
(*Merge pull request #17 from yurisismotto/feature/notifications-v1-n0-protocol*)

Android can now observe its own notifications, filter them, name them
opaquely, and encode them as `notifications.v1` messages. **There is still no
Linux sink** — that is N2 — so nothing appears on a desktop yet. What N1 proves
is the source half: the listener, its lifecycle, the security boundary, the
identity derivation, the ordering, and the fact that none of it does anything
until three independent permissions all say yes.

---

## 1. Baseline and scope

| | |
| --- | --- |
| Base | `abf4718`, N0 merged and certified (**NOTIFICATIONS.V1 N0 PASS**) |
| Canonical design | ADR-0015, ADR-0016, ADR-0017, `docs/architecture/NOTIFICATIONS.md`, `docs/research/notifications-v1/**`, `notifications_v1.proto` |
| Reference hardware | Samsung **SM-X620**, Android **16** / API 36, One UI **8.0** |
| Desktop | Fedora 44, `anyflow-daemon` `DF65 D3E4 BA28 EDF9` |
| Commits made | **none** — nothing committed, nothing pushed, no PR |

Explicitly **not** in this wave, and verified absent below: the Linux
notification sink, notification settings UI or app picker, dismissal
synchronisation runtime, Windows/macOS/iOS code, and any change to the approved
protobuf contract.

> **A note on the hardware model.** Every brief in this series names a "Samsung
> Galaxy S25". The device actually attached and certified against is a **Galaxy
> Tab S10 FE+ Wi-Fi (SM-X620)**, `ro.product.device=gts10fepwifi`, adb serial
> `RX2Y500C7SY`. All gate results below are reported against the real model.

---

## 2. Files changed

### New — the source adapter (`…/anyflow/notifications/`)

| File | Lines | What it owns |
| --- | ---: | --- |
| `AnyFlowNotificationListener.kt` | 264 | The platform `NotificationListenerService`, the main-thread extraction boundary, and `NotificationAccess` (the grant, the bind request, the component name) |
| `NotificationSource.kt` | 763 | All source state, and the **single ordered producer**: sessions, roles, the id map, the snapshot, the binding lifecycle |
| `NotificationIdentity.kt` | 283 | ADR-0016: the HMAC derivation, the group digest, the content digest, and `SourceIdMap` |
| `NotificationQueue.kt` | 244 | `NotificationEvent` and the bounded, coalescing hand-off queue |
| `NotificationPolicy.kt` | 213 | The per-peer policy type and `LockPolicy`, with the deny-by-default app list |
| `NotificationText.kt` | 198 | Limits, sanitisation, UTF-8 truncation, and `NotificationRedact` |
| `NotificationFilter.kt` | 193 | Every rule about what may leave the device, in the order the design specifies |
| `NotificationSnapshot.kt` | 181 | `PlatformNotification` (the plain extracted record) and the enum mapping |
| `NotificationRoleState.kt` | 172 | ADR-0017: what this device announces, and what a peer announced |
| `NotificationLock.kt` | 118 | The lock-state primitive and the pre-encoding privacy reduction |
| `NotificationWire.kt` | 165 | The last step: an already-filtered, already-reduced notification into protobuf |

### New — elsewhere

| File | Lines | What it owns |
| --- | ---: | --- |
| `capability/NotificationsCapability.kt` | 86 | The `notifications.v1` capability: decode, bound, route |
| `identity/IdentityReset.kt` | 62 | Destroys the device identity **and** the notification secret together (ADR-0016 §5) |

### Modified

| File | Change |
| --- | --- |
| `AndroidManifest.xml` | The listener `<service>`, its two metadata entries, and the rewritten "deliberately absent" comment block |
| `AnyFlowApp.kt` | Builds and starts the source, registers the capability, resolves app labels and the Keystore secret, and withholds `notifications.v1` from the pairing grant |
| `store/TrustStore.kt` | `TrustedPeer.notificationPolicy`, `setNotificationPolicy`, `notificationPolicyFor`, and `notificationSecretGeneration` |
| `res/values/strings.xml` | One string: how the listener is named in Android's own notification-access screen |
| `NotificationsProtocolTest.kt` | N0's "registers nobody" assertions updated to N1's reality |
| `desktop/core/tests/notifications_protocol.rs` | The cross-language verification vector for the derivation (**test-only**; no Rust production change) |

### Tests added

| File | Lines | Tests |
| --- | ---: | ---: |
| `NotificationSourceTest.kt` | 1145 | 35 |
| `NotificationFilterTest.kt` | 459 | 21 |
| `NotificationTextTest.kt` | 374 | 21 |
| `NotificationIdentityTest.kt` | 306 | 19 |
| `NotificationRolesTest.kt` | 265 | 17 |
| `NotificationSecretTest.kt` | 297 | 14 |
| `NotificationQueueTest.kt` | 274 | 14 |
| `NotificationsProtocolTest.kt` (extended) | 456 | 20 |
| `NotificationHardwareGateTest.kt` (instrumented) | 521 | 10 |
| `NotificationSecretInstrumentedTest.kt` (instrumented) | 169 | 4 |

---

## 3. Manifest additions

One `<service>`, two `<meta-data>`, and **no new `<uses-permission>` at all**.
The full list of permissions AnyFlow requests is byte-for-byte what it was
before this wave:

```
INTERNET · ACCESS_NETWORK_STATE · CHANGE_WIFI_MULTICAST_STATE ·
CHANGE_NETWORK_STATE · FOREGROUND_SERVICE ·
FOREGROUND_SERVICE_CONNECTED_DEVICE · POST_NOTIFICATIONS · CAMERA
```

```xml
<service
    android:name=".notifications.AnyFlowNotificationListener"
    android:exported="false"
    android:label="@string/notification_listener_label"
    android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE">
    <intent-filter>
        <action android:name="android.service.notification.NotificationListenerService" />
    </intent-filter>
    <meta-data android:name="android.service.notification.default_autobind"
               android:value="false" />
    <meta-data android:name="android.service.notification.default_filter_types"
               android:value="conversations|alerting" />
</service>
```

`BIND_NOTIFICATION_LISTENER_SERVICE` is held by the **system**, not by AnyFlow:
declaring it on the service is what stops any other application from binding
it, exactly as `BIND_QUICK_SETTINGS_TILE` already does for the clipboard tile.
Verified present and correctly typed in the shipped APK:

```console
$ aapt2 dump xmltree --file AndroidManifest.xml app-debug.apk
  E: meta-data (line=232)
    A: android:name="android.service.notification.default_autobind"
    A: android:value=false
```

`disabled_filter_types` is deliberately **not** declared: it would grey those
types out permanently in the system UI, and a person who genuinely wants their
media notification mirrored should be able to have it.

The "Deliberately absent, and it must stay that way" comment has been rewritten
rather than quietly edited. It now states the permanent **rule** from ADR-0015
§2, keeps every other refusal (accessibility service, `QUERY_ALL_PACKAGES`,
`SYSTEM_ALERT_WINDOW`, `READ_LOGS`, location, `MANAGE_EXTERNAL_STORAGE`,
default-IME, hidden APIs, reflection, root), adds an explicit **no
CompanionDeviceManager** entry with the reason, and records that
`BIND_NOTIFICATION_LISTENER_SERVICE` is the only entry that has ever moved and
that moving it took an ADR.

---

## 4. Listener lifecycle

```text
installed            no OS grant, no peer grant, service declared, never bound
      │
      ▼  user enables notification access in Settings
approved             the system MAY bind — and does not, because
                     default_autobind=false.  NOTHING is read
      │
      ▼  a paired peer with a notifications.v1 grant establishes a session
requestRebind()      the system binds; onListenerConnected fires; the id map is
                     rebuilt and each peer gets roles + a bracketed snapshot
      │
      ▼  the last eligible peer disconnects or is revoked
requestUnbind()      the listener unbinds; nothing is read again
      │
      ▼  OS access revoked in Settings, mid-session
onListenerDisconnected → NotificationRoles{roles=[], epoch+1} to every peer,
                         immediately, on the connection that is already up
```

**Bind and unbind are deliberately different seams.** `requestRebind` is a
*static* platform method because it must work when no service instance exists —
which is exactly the situation the first bind happens in, since a service
cannot ask to be created. `requestUnbind` is an instance method. `NotificationSource`
therefore takes an `AccessControl` (context-free: the grant and the bind
request) alongside `ListenerControl` (the live service: unbind and
`getActiveNotifications`).

**This split is a real defect found in this wave.** The first implementation
routed the bind through the service seam, so the initial bind could never be
requested. It was caught on hardware, not by reasoning, and the JVM suite now
pins it (`a granted peer connecting while unbound requests a rebind`).

A second hardware finding: `requestRebind` is asynchronous, so a bind can land
*after* the peer that asked for it has gone. `onListenerConnected` therefore
re-evaluates the binding and releases it if nobody is eligible — otherwise a
departed peer would leave the listener bound and reading. Pinned by
`a bind that lands with no eligible peer is released immediately`.

---

## 5. Callback and threading model

`NotificationListenerService` callbacks arrive on the **main thread** from API
24 onward (AOSP VERIFIED, `00 §1.2`). The listener therefore does exactly this
and nothing more:

1. drops `sbn.packageName == packageName` — **first**, before extraction;
2. copies a fixed set of fields into a plain, immutable `PlatformNotification`;
3. `queue.offer(...)`, which is non-blocking and cannot suspend;
4. returns.

On the callback thread there is **no** protobuf encoding, **no** HMAC, **no**
disk or network I/O, **no** peer enumeration, **no** package-manager call and
**no** logging of notification content. App-label resolution, the derivation,
the filter, the encoding and the send all happen on the capability's own
coroutine.

`PlatformNotification` is the **testable extraction boundary**: it holds no
framework type, so every rule that operates on an extracted notification is a
pure function of it and is tested on the JVM without a device. It is
deliberately **not** a data class — a generated `toString()` would put a title
and body into every log line and every test failure that touched one.

Every read from `Notification.extras` is individually wrapped: `extras` is a
`Bundle` an arbitrary application filled in, and a malformed or hostile one
must produce a dropped notification, never an exception on the main thread.

---

## 6. The notification secret

`device_notification_secret` is a **32-byte `HmacSHA256` key in the Android
Keystore**, alias `anyflow-notification-secret-v1`, generated by the keystore's
own CSPRNG with `setKeySize(256)` and `setUserAuthenticationRequired(false)`.

That is stronger than ADR-0016 §4 asks for, and it is stronger in a way that
matters: a keystore HMAC key is **not exportable**, so the 32 bytes never exist
in the app process at all. There is nothing for a log line, a crash dump, a
backup or a future bug to leak, because the app *cannot read the secret* — it
can only ask the keystore to compute a MAC. Proved on hardware:
`theSecretIsNotExportable` asserts `key.encoded == null`.

Consequently, and by construction rather than by promise:

* **never logged** — there is no accessor that returns the bytes;
* **never transmitted** — likewise, and no schema field could carry it;
* **never in a backup or export** — Keystore material is not backed up, and
  `android:allowBackup="false"` is set besides;
* **stable across process restart and device reboot** — which is the entire
  reason the id is derived rather than random;
* **no plaintext `SharedPreferences` secret exists.** The device has no
  `shared_prefs` directory at all (§13);
* **no second keystore architecture** — it sits beside `DeviceIdentity` under
  the same discipline, in the same `AndroidKeyStore`.

### 6.1 One documented refinement to ADR-0016 §5

ADR-0016 says a *missing or unreadable* secret is regenerated, failing
**forward** because a lost secret must not disable the capability. That is
right for **missing**, and it is what happens. But "unreadable" and "absent"
are two different facts, and a keystore that throws on read is not evidence
that the key is gone — replacing a live secret on the strength of one failed
read would invalidate every mirror on every peer for nothing.

So:

| Store says | N1 does | State |
| --- | --- | --- |
| here is the key | uses it | `LOADED` |
| there is no key, and none ever existed | creates one | `CREATED` |
| there is no key, and one existed before | creates one, **loudly** | `REGENERATED` |
| I cannot tell you | **leaves it alone**, announces no `SOURCE` role this session | *(none)* |

This satisfies both the ADR's fail-forward intent and the standing Wave 0
"no silent regeneration" rule. A regeneration is never silent either:
`TrustStore.notificationSecretGeneration` distinguishes a true first creation
from a replacement, and a replacement logs a warning carrying a state name and
a count — no key material, no notification content.

### 6.2 Reset and re-pair

`IdentityReset.resetDeviceIdentity()` destroys the device identity **and** the
notification secret in one call, because ADR-0016 §5 ties their lifetimes
together: `notification_id` is `HMAC(secret, key)` over a platform key
containing the app's `pkg` and `uid`, both stable for the life of the install,
so a peer that saw the same ids before and after a re-pair could link the old
identity to the new one — the exact correlation re-pairing exists to break.

Two separate calls at a call site is how that guarantee gets lost a year from
now; one function makes the pair inseparable, and its documentation is where
whoever adds the second reset path will read why.

**Status:** no UI resets the device identity today — `DeviceIdentity.delete()`
has had no caller since it was written, and N1 adds none, because this wave
builds no UI. What N1 adds is the guarantee that when such a path is built, it
cannot forget the secret.

### 6.3 Secret tests

| Case | Test |
| --- | --- |
| First creation | `first creation reports CREATED and records generation one` |
| 32 bytes | `a created secret is thirty-two bytes of key material` |
| Normal reload, same ids | `a normal reload returns the same secret and the same ids` |
| Process restart | `a process restart keeps the same secret` |
| Reload does not bump the counter | `a reload does not touch the generation counter` |
| **Unreadable store never replaces** | `an unreadable store never replaces the secret` |
| Absent store regenerates, loudly | `an absent secret after one existed regenerates and says so` |
| First creation ≠ regeneration | `a first creation and a regeneration are distinguishable` |
| Store cannot create | `a store that cannot create anything yields no secret` |
| Identity reset rotates | `an identity reset rotates the secret and every id` |
| Revoke/re-grant is not an identity event | `revoking and re-granting leaves the secret alone` |
| No accessor for the bytes | `the secret exposes no way to read its bytes` |
| Rendering carries no key material | `the rendering carries no key material` |
| Fresh `Mac` per derivation | `each derivation gets its own mac` |
| **Real Keystore, not exportable** | `theSecretIsNotExportable` *(instrumented)* |
| **Real Keystore, stable reload** | `theSecretIsStableAcrossReload` *(instrumented)* |
| **Real Keystore, rotation** | `destroyingTheSecretChangesEveryId` *(instrumented)* |
| Test alias ≠ production alias | `thisSuiteDoesNotTouchTheProductionAlias` *(instrumented)* |

---

## 7. Identity derivation and vectors

Exactly ADR-0016 §1:

```text
notification_id = HMAC-SHA256(
    device_notification_secret,
    "anyflow/notifications.v1/id/v1" || len32(platform_key) || platform_key
)[0..16]
```

`len32` is a big-endian `uint32` — the same length-prefixing convention as the
pairing proof and the `files.v1` data-stream MAC, for the same reason:
concatenation must be unambiguous, or two different keys could hash to one
name. The domain string is a pinned constant, asserted verbatim on both sides.

**The derivation cannot depend on title or body.** `derive(mac, platformKey)`
has exactly two parameters and there is no path through which text could reach
it — asserted structurally by reflection in
`the derivation cannot depend on title or body`.

### Pinned vectors, asserted identically in Kotlin and Rust

Secret = `00 01 02 … 1f`, platform key = `0|example.fixture.app|1|null|10123`:

| Input | Output |
| --- | --- |
| `notification_id` | `3de5b61a1978912deb452f36b9a61c7a` |
| different key (`0\|example.other.app\|1\|null\|10124`) | `86726a05ec81785ead37dcb41907d8e1` |
| different secret (`01 02 … 20`) | `b1904b490581ba28fb9e736d351fc776` |
| `group_id` of `0\|example.fixture.app\|g:chat` | `ff4aae015474d140` |

The Rust half lives in `desktop/core/tests/notifications_protocol.rs` and is a
**verification vector, not an implementation**: it computes ADR-0016's
construction independently, with a different HMAC library in a different
language, and asserts the same bytes. Nothing Android-specific was added to
portable code — `anyflow_core::notifications` still defines only the type, the
width and the validation, and `NotificationId` remains opaque. The last of the
new Rust tests closes the loop by feeding a derived id through
`NotificationId::from_bytes` and `validate_upsert`, so the width the source
produces is checked against the width the portable contract accepts.

### What is transmitted, and what is not

**The raw Android key never leaves the device** — not truncated, not hashed in
place, not alongside. Neither does the uid, the tag, the app-internal id, nor
the numeric profile number. Asserted against the encoded bytes both on the JVM
(`no encoded message contains a platform key, a uid or a profile number`) and
on hardware (`noEncodedMessageContainsAPlatformKey`), the latter against bytes
derived from a genuine `StatusBarNotification`.

The **package name** does reach the sink, deliberately, in the separate
`app_id` field. The derived id must never be described as anonymising the
source app; it removes the identifiers that had no destination-side purpose.

### `content_hash`

Domain-separated, length-prefixed over the semantic fields that are actually
sent, excluding `posted_at_unix_ms` (which changes on an identical re-post,
exactly the case the digest exists to collapse) and `notification_id` (the
thing the digest is compared *within*). It is used at the source to suppress an
identical re-send and for nothing else.

**The sink never recomputes it.** `anyflow_core::notifications` checks the
*width* and nothing more, and a sink's de-duplication keys on the value it was
sent. So this construction cannot drift between implementations, because only
one of them performs it — and N2 must keep it that way.

---

## 8. Source-side id map

`SourceIdMap`: `notification_id ↔ platform key`, both directions, **memory
only**, bounded at 512 entries with oldest-first eviction.

* holds identities and a platform key; **no title, body or subtext**, and there
  is no field on the type that could carry one;
* entries are removed when a notification disappears (`forgetKey`) and pruned
  against the live shade after a snapshot (`retainOnly`);
* rebuilt on `onListenerConnected` by re-deriving over `getActiveNotifications()`
  — *reconstructed, never restored*, which is precisely what lets dismissal
  survive a process restart without persisting anything;
* cleared on listener disconnect and on identity reset;
* `toString()` renders `SourceIdMap(entries=N)` and is asserted never to render
  a key.

**N1 executes no dismissal.** There is no code path from an inbound message to
`cancelNotification`, the string does not appear anywhere in the new code
outside comments, and Android announces no `DISMISS_TARGET` role. The lookup
primitive exists and is tested so N4 has it.

---

## 9. App-filter primitive

**Deny by default, and the default policy mirrors nothing.**

```kotlin
NotificationPolicy(
    allowMirror        = true,      // the grant was the deliberate act
    allowedApps        = emptySet(),// ← nothing is shared until a person names it
    includeWorkProfile = false,
    includeOngoing     = false,
    whenSourceLocked   = APP_ONLY,
    allowDismissSync   = false,     // ADR-0015 §6
)
```

Stored per peer in the trust store beside `clipboardPolicy`, read through
`notificationPolicyFor()`, which answers the **grant and the policy in one
call** so a caller cannot ask the second and forget the first. An unknown,
forgotten or ungranted peer resolves to `NotificationPolicy.DENIED`.

`notifications.v1` is **never in `auto_grant`**: `AnyFlowApp.pair()` now
subtracts it from the capabilities granted at pairing, beside `clipboard.v1`
and for a stronger version of the same reason.

Rules no setting can override, all tested:

| Rule | Test |
| --- | --- |
| AnyFlow's own package is never mirrored | `the own package is dropped`, `the own package cannot be allowed by any policy`, `our own notification is never mirrored, even when explicitly allowed`, `ourOwnNotificationIsNeverMirroredOnHardware` |
| `VISIBILITY_SECRET` is never mirrored | `a secret notification is never transmitted`, `a secret-visibility notification is never mirrored` |
| `IMPORTANCE_NONE` is never mirrored | `importance none is dropped` |
| A newly installed app is denied | `a newly installed app is not shared` |
| A system package is denied | `a system package is denied by default` |
| Work profile needs its own switch | `a secondary profile notification is denied by default`, `the work profile switch does not bypass the app list` |
| Ongoing needs its own switch | `an ongoing notification is denied by default` |
| Category is never an ACL | `category grants nothing and denies nothing` |
| No content-based heuristic exists | `no rule inspects the notification text` |

**System applications are denied for exactly the reason every other
application is: the list starts empty.** AnyFlow does not classify packages as
"system" and then trust the classification — a check that could be wrong in
either direction is worse than the deny-by-default that needs no check at all.

No UI was built. N3 owns the picker.

---

## 10. Lock / privacy primitive

`LockState` is a one-method seam over `KeyguardManager.isDeviceLocked`, and
**any answer other than a confident "unlocked" is `true`**: an exception, a
missing system service or an odd OEM answer all resolve to locked. A privacy
control that fails open is not a control.

`isDeviceLocked` rather than `isKeyguardLocked`, because the policy is about a
device secured behind a credential, not about a swipe-to-dismiss keyguard on a
phone with no lock set.

`NotificationReduction.reduce()` runs **before encoding** and produces a value
that simply does not contain the withheld text:

| Policy, phone locked | `title` | `body` | `redacted` |
| --- | --- | --- | --- |
| `Full` | full | full | false |
| `AppOnly` *(default)* | `""` | `""` | **true** |
| `Suppress` | *nothing is sent at all* | | |

There is no "redact on the way out" step further down that could be skipped,
and nothing downstream is handed a full body plus a flag saying not to use it.
Unlocking cannot be retroactive because there is nothing anywhere to deliver
later.

`redacted` is a **privacy** signal and not a length one: a 4 KiB body is
truncated on a UTF-8 boundary with a visible `…` and is **not** marked
redacted (`a truncation is not a redaction`).

N3 owns the user-facing policy screen; N1 implements only the source-side
primitive and the default the screen must respect.

---

## 11. POC-NOTIF-01 as a hard assumption

Nothing in this wave depends on Android redacting OTPs, on a category
identifying a banking app, or on a system heuristic catching a secret. There is
no regex, no keyword list and no app-category guess anywhere in the new code —
asserted by `no rule inspects the notification text`, which gives two
notifications from one app radically different content and requires the same
verdict.

Every title and body is treated as fully sensitive user data, in production and
in tests. The certification device confirms the premise: AnyFlow's uid is **not
in** `mTrustedListenerUids={1000, 10064, 10135}`, so AnyFlow is an untrusted
listener — which is the state POC-NOTIF-01 showed delivers OTP-shaped content
verbatim on this hardware.

---

## 12. Mapping, ordering and the snapshot

**Posted and updated are one message.** `onNotificationPosted` fires for both
with the same key, and the protocol has one idempotent `NotificationUpsert`.
`an update keeps the same notification id` asserts the identity is stable while
the content hash changes.

**Removal carries no reason.** `onNotificationRemoved`'s `reason` parameter is
read and discarded; only the key is passed on. All 23 Android reasons mean the
same thing to a mirror.

**One ordered producer.** Every message — roles, upserts, removals, sync
markers and results — is written by one coroutine draining one
`NotificationOutboundQueue`. The session's outbound channel guarantees FIFO
*per producer*, not across producers, so two senders could let a removal
overtake the upsert it refers to and strand a mirror on a desktop permanently.
`a removal never overtakes its upsert` drives 25 post/remove pairs through the
real producer and asserts strict alternation.

**Backpressure**, documented and bounded at 256 events:

1. a `Posted` for a key already pending **replaces it in place** — a progress
   bar updating sixty times a second occupies one slot, always the current one,
   and the common flood never reaches the overflow path;
2. a `Removed` supersedes any pending `Posted` for the same key;
3. otherwise the **oldest non-terminal** event is evicted — the sink converges
   anyway, because the next upsert or the next snapshot restores the state;
4. only if every pending event is terminal is a terminal event dropped, and
   that is **counted** (`droppedTerminal`) and logged, never silent. It is
   expected to stay zero: reaching it needs 256 simultaneous removals.

Nothing blocks, ever, because the offering side is the phone's main thread.

**Snapshot.** `SyncMarker{BEGIN}` → the currently-active, filter-passing,
policy-passing notifications as ordinary upserts → `SyncMarker{END}`, emitted
from one pass of the producer so the bracket cannot be split. Capped at 100
entries, most recently posted first. It passes **every** filter a live
notification does, and a peer with no grant is sent no bracket at all. Nothing
is persisted to build it, and it contains only what is in the shade at that
instant.

Sent once per connection: attempted at attach, and again when the peer's `SINK`
announcement arrives (a peer announces just after the session comes up, which
is after this side attached). `a snapshot is sent once per connection` pins it.

---

## 13. Roles and capability registration

**N1 announces `SOURCE`, and only `SOURCE`.**

ADR-0017 §1 has Android advertising `SOURCE` *and* `DISMISS_TARGET` in v1, and
it will — in **N4**, which implements dismissal. Announcing `DISMISS_TARGET`
now would be a claim this wave cannot honour: there is no path to
`cancelNotification`, so a peer that believed the claim would send
`DismissRequest`s into a device that ignores them. A role is a statement about
what is physically possible right now, and the honest answer in N1 is that
dismissal is not. `N1 does not claim to be a dismiss target` and
`rolesAreAnnouncedFirstAndClaimOnlySource` pin it, the latter on hardware.

The role set is a function of *platform capability alone*: the listener is
connected, the OS grant is in place, and the secret is usable. **The peer grant
is deliberately not an input** — holding a peer grant does not manufacture a
platform capability, and holding the OS permission grants nothing to any peer.

Epochs are per connection, from 1, strictly increasing, never reused, and an
unchanged set produces no announcement. A peer's announcement is refused at
epoch 0 (unset) and at any epoch not **strictly** greater than the last
accepted — equal included, so a duplicate carrying a different set cannot take
effect. Unknown role values are dropped and the rest of the set still applies.

**`notifications.v1` is now registered and advertised in `HELLO`.** That is
deliberately *not* conditional on the current Android permission state: roles
exist precisely so a capability can be supported while being unable to do
anything, and gating the handshake on a permission the user can toggle at 14:32
would mean a reconnect were needed to pick up a grant made in Settings
(ADR-0017). Support is not permission — the peer grant, the OS access and the
announced roles are three further, independent gates, and a desktop that does
not implement `notifications.v1` (every desktop until N2) never negotiates it.

Android is a source and nothing else: an inbound `upsert`, `remove`, `sync` or
`dismiss` is answered `REJECTED_ROLE` and the session survives. An ungranted
peer's message is answered `NOT_AUTHORIZED`, with the grant re-read at that
moment rather than trusted from the handshake. A bad-width identifier is
refused **and not answered**, because a `NotificationResult` echoes the id and
a malformed one leaves nothing coherent to correlate a reply with.

---

## 14. Persistence audit

**New persisted state, in full:**

| Where | Field | Contents |
| --- | --- | --- |
| `files/trust-store.json` | `notificationSecretGeneration` | an integer counter |
| `files/trust-store.json` | `peers[].notificationPolicy` | `allowMirror`, `allowedApps` (package names the user chose), `includeWorkProfile`, `includeOngoing`, `whenSourceLocked`, `allowDismissSync` |
| Android Keystore | `anyflow-notification-secret-v1` | a non-exportable HMAC key |

**Nothing else.** No notification history in any form. No title, body, subtext,
platform key, tag, snapshot or `NotificationUpsert` payload is written
anywhere, at any time, in memory beyond the current active set or on disk at
all.

Verified on the device after a full session with the synthetic fixture active:

```console
$ adb shell run-as io.github.yurisismotto.anyflow find . -type f
./files/trust-store.json
./files/profileInstalled

$ adb shell run-as io.github.yurisismotto.anyflow cat files/trust-store.json
{"schemaVersion":1,"deviceId":"97a888f9ffdafd7e3d289821300a857e",
 "deviceName":"SM-X620","peers":[],"notificationSecretGeneration":1}
```

Grepping the whole app-private tree for `ANYFLOW-N1-FIXTURE`,
`anyflow-n1-fixture`, `com.android.shell` and `FIXTURE`: **no matches**. There
is no `shared_prefs` directory and no database — so the "do not create a
plaintext `SharedPreferences` secret" rule holds structurally.

---

## 15. Logging audit

Seventeen `Log.*` calls were added. Every interpolated value is one of: an
exception **class name**, an event **class name**, a role count, an epoch, a
snapshot count, a `DropReason` enum name, a secret **state name**, a generation
number, a `NotificationOutcome` enum name, or the first 8 hex characters of an
already-opaque `notification_id`.

**No log line anywhere contains a title, body, subtext, raw SBN key, tag,
package name, content-hash input or extras dump** — at any level, including
verbose, with no debug override.

This is what AnyFlow actually logged across a full hardware session, in full:

```
    18  notification listener disconnected; narrowing source role
     9  peer roles epoch=1
     9  notification listener connected
     9  announcing roles=0 epoch=1
     8  no eligible peer
     8  announcing roles=1 epoch=2
     8  an eligible peer is connected
     8  1 of 59 active
     1  announcing roles=1 epoch=4
     1  announcing roles=0 epoch=3
     1  announcing roles=0 epoch=2
     1  1 of 60 active
```

Two things worth reading twice. `1 of 59 active` is the deny-by-default filter
working against a real notification shade: 59 notifications present, exactly
one — the named fixture package — mirrored. And
`roles=1 epoch=2 → roles=0 epoch=3 → roles=1 epoch=4` is ADR-0017 on hardware:
narrow on revocation, widen on re-grant, strictly increasing, all within one
session and without a reconnect.

The `logging.rs`-style canary suite (NOTIF-SEC-25) remains N3's gate. N1 adds
the in-process half: `no capability state renders notification content` and
`noCapabilityStateRendersNotificationContent` assert against distinctive
canaries on the objects a log line or an exception message would actually
reach.

---

## 16. Test results

### 16.1 Android JVM — **404 tests, 0 failures**

161 in the eight notification suites; the other 243 are the pre-existing suites,
all green.

```
NotificationSourceTest      35    NotificationTextTest        21
NotificationFilterTest      21    NotificationsProtocolTest   20
NotificationIdentityTest    19    NotificationRolesTest       17
NotificationSecretTest      14    NotificationQueueTest       14
```

Coverage against the brief's required list: secret first creation · secret
stable reload · secret rotation/reset · no silent regeneration · HMAC vector ·
different key · different secret · exact 16-byte id · own-package hard deny ·
app filter default deny · new app default deny · secondary profile default deny
· listener callback extraction (via `PlatformNotification`) · malformed extras
· upsert mapping · remove mapping · no removal reason emitted · role narrowing ·
epoch behaviour · single ordered producer · snapshot BEGIN/items/END ordering ·
queue and backpressure behaviour · no content persistence · no content
rendering. **All present and green.**

### 16.2 Android instrumented — **SM-X620, Android 16, One UI 8.0**

**The full 35-test connected suite has now completed green in a single run**
(2026-09-09, closeout session), through the official Gradle task:

```console
$ JAVA_HOME=… ANDROID_HOME=… ./gradlew :app:connectedDebugAndroidTest
Starting 35 tests on SM-X620 - 16
…
BUILD SUCCESSFUL in 1m 56s
```

The JUnit XML is the authority, and it records no skips — which matters here,
because every hardware gate guards its preconditions with `assumeTrue`, so a
missing grant would have produced a *skipped* test inside a green build rather
than a failure:

```console
$ grep -o '<testsuite[^>]*>' 'app/build/outputs/androidTest-results/connected/debug/TEST-SM-X620 - 16-_app-.xml'
<testsuite … tests="35" failures="0" errors="0" skipped="0" time="102.77" …>
```

| Class | Tests |
| --- | ---: |
| `NotificationHardwareGateTest` | 10 |
| `ClipboardInstrumentedTest` | 9 |
| `DownloadsTest` | 5 |
| `NotificationSecretInstrumentedTest` | 4 |
| `DeviceIdentityTest` | 4 |
| `ClipboardPersistenceTest` | 3 |
| **Total** | **35** |

**Total 35 · passed 35 · failed 0 · skipped 0 · BUILD SUCCESSFUL.** All ten
hardware gates and all four Keystore tests executed rather than being assumed
away.

#### What had actually been failing

The earlier collapses were **device preconditions, not code defects**, and both
are now identified precisely rather than attributed to general flakiness:

1. **A fresh install leaves the runtime permissions ungranted**, so
   `GrantPermissionsActivity` — the system permission dialog — sits on top of
   `MainActivity` and takes window focus. `ClipboardInstrumentedTest` fails
   *by design* in that state, with its own diagnostic message ("AnyFlow never
   took window focus…"). The earlier session read that message and blamed the
   Settings app; the real thief was the permission dialog. Pre-granting
   `POST_NOTIFICATIONS` and `CAMERA` with `pm grant` removes it.
2. **AGP uninstalls both APKs when the run ends**, and a later *fresh* install
   therefore starts with no notification-listener approval — which silently
   demotes the gate tests to skips.

One measurement corrected a standing assumption in the process: an in-place
`install -r` over an already-installed package **preserves** both the runtime
permissions and the notification-listener approval. It is a full *uninstall*
that drops them. So `connectedDebugAndroidTest` is perfectly capable of running
the gates for real, provided the app is already installed and granted when it
starts:

```console
$ ./gradlew :app:installDebug --rerun-tasks     # install -r over the existing app
$ adb shell settings get secure enabled_notification_listeners | grep -c yurisismotto
1                                               # grant survived the reinstall
```

---

## 17. Hardware evidence

### Gate A — notification access OFF

```console
$ adb shell settings get secure enabled_notification_listeners
com.sec.android.app.launcher/… : com.samsung.android.smartmirroring/…
```

AnyFlow absent from the approved list, absent from `All notification listeners
enabled for current profiles`, and absent from `Live notification listeners
(11)`. The instrumented suite in this state: `rolesAreAnnouncedFirstAndClaimOnlySource`
**PASS** with an empty role set — the device claims no source ability — and
`anUngrantedPeerIsSentNoNotificationContent` **PASS**. No notification content
in logcat. **PASS.**

### Gate B — access ON, no peer granted

From a clean state (grant revoked, app force-stopped, then granted):

```console
$ adb shell cmd notification allow_listener io.github.yurisismotto.anyflow/…AnyFlowNotificationListener
$ adb shell dumpsys notification | sed -n '/Live notification listeners/,/Snoozed/p' | grep -c yurisismotto
0
```

**Approved and not bound.** `META_DATA_DEFAULT_AUTOBIND=false` is honoured by
One UI 8 / Android 16: granting notification access does not cause the system
to bind the listener, and nothing is read. Zero notification content leaves the
device; the app filter and the grant fail closed. **PASS.**

### Gate C — granted, eligible peer

`anAllowedApplicationIsMirroredToAGrantedPeer`, `anUnnamedApplicationIsNeverMirrored`,
`ourOwnNotificationIsNeverMirroredOnHardware`, `theSnapshotIsBracketedOnHardware`,
`noEncodedMessageContainsAPlatformKey` — **all PASS** against a synthetic
`com.android.shell` fixture posted by `cmd notification post`, with the real
bound listener, the real `getActiveNotifications()`, the real
`StatusBarNotification`, the real Keystore secret and the real protobuf
encoding.

> **The honest boundary of gate C.** There is no `notifications.v1` peer to
> talk to — the Linux sink is N2 and does not exist — so the session is a local
> capture of the bytes the capability produced. What is verified is that the
> correct bytes were produced for a granted peer, **not** that a desktop
> displayed them. That is stated rather than papered over.

### Gate D — access revoked mid-session

`revokingAccessNarrowsTheSourceRoleWithoutAReconnect` — **PASS**. Driven
through `UiAutomation` (shell uid), so the gate executes rather than being
reasoned about. With the listener bound and `{SOURCE}` announced, the test
revokes access in the platform; `onListenerDisconnected` fires on a healthy
session; the source announces an **empty** set with a **strictly higher**
epoch; no further upsert is emitted; and no reconnect happens. The logcat
trace above (`epoch=2 → 3 → 4`) is the same event seen from the other side.

### Gate E — process / app restart

`theProductionSecretIsAvailableAndStable` **PASS**, and the derived-identity
half is proved twice: `theSecretIsStableAcrossReload` on the real Keystore, and
`a restart derives the same ids for the same active notifications` on the JVM,
which runs the whole source twice over one persistent secret and asserts the
same `notification_id`. No duplicate identity explosion.

### Gate F — identity reset / re-pair

**Executed against an isolated test alias**, not the production one:
`destroyingTheSecretChangesEveryId` (real Keystore) and
`an identity reset rotates the secret and every id` (JVM) both assert that the
prior secret is gone and every derived id changes.
`thisSuiteDoesNotTouchTheProductionAlias` asserts the separation, so this suite
cannot rotate the maintainer's real notification secret or disturb a certified
pairing. **PASS, with the scope stated.**

---

## 18. Dumpsys gate

Exact component:

```
io.github.yurisismotto.anyflow/io.github.yurisismotto.anyflow.notifications.AnyFlowNotificationListener
```

| Device state | `Allowed notification listeners` | `Live notification listeners` |
| --- | --- | --- |
| Access never granted | absent | absent |
| Access granted, no peer ever connected | **present** | **absent** |
| Access granted, eligible peer connected | present | **present** |
| Eligible peer gone | present | **absent** |

The second row is the one ADR-0015 §3 asks for, and One UI 8 honours it. The
fourth row was **not** true of the first implementation: `requestUnbind` was
only attempted when a session detached while the listener was already
connected, so a bind landing after the requesting peer had gone left the
listener bound. That was found here, on this device, and fixed —
`onListenerConnected` now re-evaluates the binding. Both directions are now
observed on hardware and pinned by JVM tests.

One observation recorded because it is easy to misread: once `requestRebind`
has been called, the listener stays bound across a revoke/re-grant of the OS
approval until `requestUnbind` is called. That is the platform's own
`requestRebind` semantics, not a failure of `default_autobind`, and the
clean-state measurement above (revoke → force-stop → grant → **0 live**) is
what distinguishes the two.

No output was manipulated and no assumption was substituted for a measurement.

---

## 19. Logcat and store security gates

**Logcat.** Cleared before the test, captured after (13 205 lines), searched for
the distinctive non-sensitive fixture strings:

| Needle | Hits | Attribution |
| --- | ---: | --- |
| `ANYFLOW-N1-FIXTURE-TITLE` | 1 | `adbd`, echoing the `cmd notification post` command line I typed |
| `ANYFLOW-N1-FIXTURE-BODY` | 1 | same line |
| `anyflow-n1-fixture` (the tag) | 18 | Samsung SystemUI: `ExpandableNotifRow`, `View`, `InterruptionStateProvider`, `NotificationPanelView`, `InsignificantCoordinator`, `HoneySpace.NotificationListener` |
| any of the above on an **AnyFlow** tag | **0** | — |

Every hit is the OEM's own logging or my own shell command. **Zero** lines from
`AnyFlowListener`, `NotificationSource` or `NotificationSecret` contain a
title, a body, a tag, a platform key or a package name. **PASS.** No logcat
capture is committed to git; the captures live in the session scratchpad.

**Store.** §14 above: two files, 132 bytes of settings, no notification content
of any kind, no secret bytes. **PASS.**

---

## 20. Existing-feature regression

| Area | Result |
| --- | --- |
| Android JVM suite (all 404 tests) | **green** |
| Pairing, TLS, SPKI, replay guard, framing | **untouched** — `git diff --stat` reports zero changes under `net/`, `pairing/`, and `identity/DeviceIdentity.kt` |
| `battery.v1`, `files.v1`, `clipboard.v1` | **untouched** — no source change; their JVM and instrumented tests are green |
| Desktop `anyflow-core`, capability crates | **untouched** — the only Rust change is a test file |
| Connected suite (25 tests) | **green**, run earlier in the implementation session on the SM-X620 |
| Connected suite (35 tests, including the new gates) | **GREEN in one complete run** — 35/35, 0 failed, 0 skipped (§16.2) |
| Live cross-device smoke (connect → battery / clipboard) | **GREEN** — pairing, TLS session, ping, battery and clipboard **both directions** (§29) |

Shared transport was **not** changed, so the escalated regression scope does
not apply and full Wave 0 recertification is not required.

### The two items, stated plainly

**The 35-test connected suite: now CLOSED.** During the implementation session
the SM-X620 dropped from adb to MTP-only three times — a documented flakiness
of this device that needs a physical cable replug and has nothing to do with
this wave. Two runs collapsed mid-suite; a third failed six
`ClipboardInstrumentedTest` cases whose own assertion message named the symptom
("AnyFlow never took window focus… Is another app holding focus, or the screen
locked?"), which was then attributed to the Settings app.

In the closeout session the device stayed on adb for the whole of the test run,
and the suite completed
**35/35, 0 failed, 0 skipped, BUILD SUCCESSFUL** in a single run of
`./gradlew :app:connectedDebugAndroidTest` (§16.2). The focus failure
reproduced once at the start of that session and was then diagnosed properly:
the window-focus thief is `GrantPermissionsActivity`, the runtime-permission
dialog that a *fresh* install necessarily raises, not the Settings app.
Pre-granting `POST_NOTIFICATIONS` and `CAMERA` removes it. That correction is
recorded because the earlier attribution would have sent the next person
looking in the wrong place.

The adb/MTP flakiness is **not** fixed, and should not be read as fixed: it
recurred later in the same session, during the Gate 2 attempt, with the tablet
vanishing from `adb devices` entirely and needing a physical replug. It simply
did not happen during the two minutes the suite was running. The mitigation
remains what it always was — replug and re-run — and the suite is short enough
(1 m 56 s) to fit comfortably inside a good window.

**The live cross-device smoke: PASS.** The tablet was re-paired by QR against
the running daemon and a real TLS session came up
(`a8012927949c88e9fb52cd03badd6386`, fingerprint `7E63 7B4E 937B 7732`,
`state connected`). Over that session: `ping` → `pong in 33 ms`; battery read
live at `80% (NotCharging)`; and a clipboard round trip in **both**
directions, each verified at the receiving end rather than at the sender.
Detail at §29.

---

## 21. Security review

| Claim | Evidence |
| --- | --- |
| TLS unchanged | no diff under `net/`; `PinnedTrustManager`, `Framing`, `PeerConnection` untouched |
| SPKI pinning unchanged | no diff in `identity/Fingerprint.kt` or `PinnedTrustManager.kt` |
| Pairing unchanged | no diff in `pairing/`; `AnyFlowApp.pair()` changed only to **withhold** a grant |
| CDM still absent | `CompanionDeviceManager` appears nowhere in the codebase; the manifest comment now names it as a refused, ADR-gated decision |
| No `PendingIntent` execution | zero non-comment occurrences in the new code |
| No `RemoteInput` | zero |
| No actions, no reply | zero; `notification.actions` is never read |
| No `cancelNotification` | zero — no path from an inbound message exists |
| No `RemoteViews` | zero |
| No `QUERY_ALL_PACKAGES` | zero; app labels resolve through ordinary package visibility |
| No notification history | §14 — nothing persisted, and no type can hold content |
| No content persistence | §14, verified on the device |
| No content logging | §15, verified against a real logcat |
| Own-package hard block | §9, checked first, tested on JVM and hardware, unoverridable by any policy |
| Deny-by-default app policy | §9; `1 of 59 active` on hardware |
| Peer grant checked independently | re-read per notification and per inbound message; never captured at handshake |
| Role checked independently | a peer without `SINK` is sent nothing; a device without a bound listener claims nothing |
| OS access checked independently | read from the platform on every question; any failure answers *false* |
| Unknown lock state fails closed | `LockState` returns `true` on any error; `an unknown lock state behaves as locked` |
| Platform OTP redaction not relied upon | §11; AnyFlow's uid is not in `mTrustedListenerUids` |

---

## 22. Protocol guard

```console
$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
(empty)
```

**The approved contract is unchanged.** No implementation blocker required a
schema change, and none was made. The canonical cross-language encoded vector
(`CANONICAL_UPSERT_HEX` / `canonicalUpsertHex`) still matches byte for byte on
both sides.

---

## 23. Rust regression

Run from `desktop/`, with the documented GTK prefix sourced:

```console
$ cargo fmt --check                                              FMT OK
$ cargo build --workspace --locked -j 2                          Finished
$ cargo test --workspace --locked -j 2                           all green
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
                                                                 Finished, no warnings
```

`anyflow-core`'s `notifications_protocol` suite is **47 tests, 0 failures** —
the 42 from N0 plus the five new verification-vector tests. The only Rust change
in this wave is that test file: no production Rust was written, which is the
expected shape for an Android wave. Any substantial Rust runtime here would
have been scope creep toward N2.

---

## 24. Git and artifact audit

```console
$ git status --short
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/…/AnyFlowApp.kt
 M android/app/src/main/java/…/store/TrustStore.kt
 M android/app/src/main/res/values/strings.xml
 M android/app/src/test/java/…/NotificationsProtocolTest.kt
 M desktop/core/tests/notifications_protocol.rs
?? android/app/src/androidTest/java/…/NotificationSecretInstrumentedTest.kt
?? android/app/src/androidTest/java/…/NotificationHardwareGateTest.kt
?? android/app/src/main/java/…/capability/NotificationsCapability.kt
?? android/app/src/main/java/…/identity/IdentityReset.kt
?? android/app/src/main/java/…/notifications/
?? android/app/src/test/java/…/Notification{Filter,Identity,Queue,Roles,Secret,Source,Text}Test.kt

$ git diff --check
(clean)
```

Scanned the changed and untracked set for `*.key`, `*.pem`, `*.p12`, `*.pfx`,
`*.jks`, `*.keystore`, `*.apk`, `*.aab`, `*.log`, `state.json`, identity files,
captures, `build/` and `target/`: **no matches**. Logcat captures live in the
session scratchpad and are not in the repository.

**Nothing was committed, nothing was pushed, no PR was created.**

---

## 25. Remaining risks and debts

1. ~~The 35-test connected suite has not completed in one green run.~~
   **CLOSED** (§16.2, §20): 35/35, 0 failed, 0 skipped, in one run of
   `:app:connectedDebugAndroidTest`. The adb/MTP flakiness that broke the
   earlier attempts did not recur; the residual focus failure was a
   runtime-permission dialog on a fresh install, not device flakiness, and is
   removed by pre-granting `POST_NOTIFICATIONS` and `CAMERA`.
2. ~~The live cross-device smoke is unexecuted.~~ **CLOSED** (§20, §29):
   QR re-pair, real TLS session, `ping`, a live battery read and a clipboard
   round trip in both directions, all on the SM-X620 against the Fedora
   daemon.
3. **Instrumented runs need their device preconditions set, or the hardware
   gates skip silently.** Every gate in `NotificationHardwareGateTest` guards
   itself with `assumeTrue`, so an absent notification-listener grant yields
   *skipped* tests inside a **BUILD SUCCESSFUL** — a green run that proves
   nothing. Read `skipped="0"` out of the JUnit XML before believing a pass.
   The preconditions are: app already installed (so Gradle does `install -r`,
   which preserves grants, rather than a fresh install, which does not),
   `POST_NOTIFICATIONS` and `CAMERA` granted, the listener allowed, and the
   `com.android.shell` fixture posted.
4. **Gate C cannot be end-to-end until N2 exists.** Its boundary is stated at
   §17 rather than blurred.
5. **`content_hash` is defined by N1 alone.** That is safe because only the
   source computes it and no sink recomputes it — but N2 must not start
   recomputing it, or a source-side optimisation becomes a wire-format
   contract. Flagged in the code and here.
6. **`IdentityReset` has no caller.** So does `DeviceIdentity.delete()`, and
   has since it was written. The debt is pre-existing; N1 makes sure that when
   a reset path is finally built it cannot forget the notification secret.
7. **`requestRebind` semantics are stickier than the ADR's prose implies.**
   Once called, the binding survives a revoke/re-grant of the OS approval until
   `requestUnbind`. Documented at §18; it does not weaken any property, because
   the role narrows on `onListenerDisconnected` regardless, but N5's soak
   testing should watch it.
8. **One UI app-sleep behaviour over a long idle is untested.** That is
   POC-NOTIF-04 and belongs to N5 (`NOTIF-HW-07`).
9. **The `logging.rs` canary suite (NOTIF-SEC-25) is still N3's.** N1 has the
   in-process canaries and a real logcat audit, not the automated regression.
10. **Play policy (OQ-09) remains open.** Unchanged by this wave and not on the
   critical path for a sideloaded build.

---

## 26. Git status at the end

Working tree as listed in §24: six modified files, two new production files,
one new production package (eleven files), seven new JVM test files, two new
instrumented test files. Branch `feature/notifications-v1-n1-android-source`,
`HEAD` still at `abf4718`, no commits.

---

## 27. Recommended commit message

```
feat(android): implement the notifications.v1 source adapter

Android can now observe its own notifications, filter them, name them
opaquely and encode them as notifications.v1 messages. Nothing is
displayed anywhere yet: the Linux sink is N2.

The manifest gains one <service> and no new <uses-permission>.
BIND_NOTIFICATION_LISTENER_SERVICE is held by the system, not by us,
which is what stops any other app binding the listener; the user grants
access separately in Settings, and that grant alone sends nothing to
anyone. META_DATA_DEFAULT_AUTOBIND=false, so the listener is bound only
while a paired computer holding the notifications.v1 grant is connected
and released when the last one goes away — verified with dumpsys on the
SM-X620.

Three permissions stay independent and are each checked in a different
place: Android's notification access, the peer's grant in the trust
store (re-read per notification, never in auto_grant), and the roles
each side announces. N1 announces SOURCE and only SOURCE — dismissal is
N4, and claiming DISMISS_TARGET now would be a claim this wave cannot
honour.

device_notification_secret is a non-exportable HmacSHA256 key in the
Android Keystore, so the 32 bytes never enter the app process at all. An
absent secret is regenerated and reported; an *unreadable* one is left
alone and the capability goes inert for that session, because a store
that cannot answer is not a store that is empty. notification_id is
HMAC-SHA256(secret, domain || len32(key) || key)[0..16], pinned by
vectors asserted identically in Kotlin and Rust; the raw Android key,
the uid and the profile number never leave the device.

Deny by default: the per-peer app list starts empty, so a freshly
granted computer receives nothing until a person names an application.
AnyFlow's own package is dropped first, before every other check, with
no setting that turns it off. Content is reduced before encoding, so
what policy withholds never exists on the wire.

All outbound traffic is produced by one ordered coroutine draining one
bounded, coalescing queue: FIFO is guaranteed per producer, so a second
sender could let a removal overtake the upsert it refers to and strand a
mirror permanently. Terminal events are never coalesced away, and the
one branch that could drop one counts it.

404 JVM tests green; 10 hardware gates and 4 Keystore tests green on the
SM-X620 (Android 16, One UI 8). No notification content in logcat or in
app-private state, verified on the device. notifications_v1.proto is
unchanged; the only Rust change is a cross-language verification vector.
```

---

## 28. Recommendation for N2

N2 is independent of N1 and shares only the `.proto`, so it can start
immediately. Five things this wave learned that N2 should carry:

1. **Do not recompute `content_hash`.** Key the sink's de-duplication on the
   value it was sent. Only the source computes it, which is what stops the
   construction becoming a wire contract (§7).
2. **`anyflow_core::notifications` is complete and correct for the sink.**
   Re-export it rather than reimplementing; the snapshot machine, the role
   reduction and the conservative enum resolution are all there and tested.
3. **Create `NotificationSink` beside the first D-Bus implementation**, not
   before it — the shape is what writing the sink will teach. Keep
   `desktop/core/tests/portable_boundary.rs` green with the new crate listed,
   and keep `--no-default-features` clean for `x86_64-pc-windows-msvc`.
4. **`logind LockedHint` is authoritative; `ActiveChanged`'s boolean is
   discarded.** POC-NOTIF-02 measured a 665 ms fail-open window on the other
   path. Unknown lock state is locked.
5. **A `NotificationRemove` that arrives before its upsert is a source bug, not
   a sink one** — but the sink should still answer `UNKNOWN_NOTIFICATION` and
   converge rather than erroring. N1's ordered producer is what makes it not
   happen; the sink should not depend on that being true of every future peer.

Both items §20 left open are now closed: the connected suite ran green in one
complete run, and the live battery/clipboard smoke was executed over a real
TLS session after a QR re-pair (§29).

---

## 29. Closeout session — 2026-09-09

This session was evidence-only: **no production code, no test code, no
protocol change, no commit, no push.** The working tree is byte-identical to
§24 apart from this report. Its whole job was to execute the two gates §20 left
open.

### Gate 1 — full `connectedDebugAndroidTest` in one run: **PASS**

**35 total · 35 passed · 0 failed · 0 skipped · BUILD SUCCESSFUL (1 m 56 s).**
Detail, per-class breakdown and the JUnit XML header are at §16.2.

The skip count is the part worth insisting on. Every gate in
`NotificationHardwareGateTest` opens with `assumeTrue`, so a green build with
the notification-listener grant missing would have reported ten *skipped*
tests and still said BUILD SUCCESSFUL. `skipped="0"` is what makes this a real
pass rather than a vacuous one.

Two device preconditions had to be right, and the first one is what had been
failing all along:

* the runtime-permission dialog must not exist — a fresh install raises
  `GrantPermissionsActivity`, which takes window focus away from
  `MainActivity` and makes `ClipboardInstrumentedTest` fail by design;
* the app must already be installed when the task starts, so Gradle does an
  in-place `install -r` (which preserves the listener grant) rather than a
  fresh install (which does not).

### Gate 2 — live cross-device smoke: **PASS**

A real QR re-pair, a real TLS session, and all three smoke steps executed on
it. Every result below was verified at the **receiving** end, not inferred from
the sender's success message.

| Step | Result |
| --- | --- |
| `anyflowd` running and healthy | **PASS** — pid 89097, port 55432, `DF65 D3E4 BA28 EDF9` |
| QR re-pair | **PASS** — `a8012927949c88e9fb52cd03badd6386`, fingerprint `7E63 7B4E 937B 7732` |
| TLS session, peer connected | **PASS** — `state connected`, frames arriving continuously |
| `ping` round trip | **PASS** — `pong in 33 ms` (and `116 ms` on a later probe) |
| `battery/status` | **PASS** — `80% (NotCharging)`, read live over the session |
| Fedora → Android clipboard | **PASS** — `sent 27 bytes`; tablet raised *"Clipboard received from Fedora"* |
| Android → Fedora clipboard | **PASS** — sent from the tablet, `applied 27 bytes`, byte-identical to the marker sent |

Synthetic, non-sensitive markers were used in both directions
(`ANYFLOW-N1-SMOKE-F2A-…` / `…-A2F-…`), and no clipboard payload was logged —
the inbound direction was compared by SHA-256 digest and exact-match, never
printed.

#### What the smoke confirmed about N1's own claims

Three of them, from the outside, without reading the code:

* **`clipboard.v1` is not auto-granted at pairing.** The peer came up with
  `granted battery.v1` and nothing else, and the tablet's own store showed
  `['battery.v1', 'files.v1']`. §9's rule holds in practice, and the Android
  peer-detail screen states it in as many words: *"Nothing is granted
  automatically."*
* **The grant is genuinely two-sided.** Granting `clipboard.v1` on the desktop
  alone was not enough; until the tablet's master **Clipboard** toggle was
  turned on, `anyflow clipboard send` refused.
* **The clipboard notification carries no clipboard text.** The tablet's
  received-clip notification reads *"Clipboard received from Fedora"* and the
  marker string appears **nowhere** in `dumpsys notification` — matching the
  rule stated in `strings.xml`, that no string here can interpolate the
  clipboard text itself.

#### Three things that cost time, recorded so they do not cost it again

1. **`anyflow pair` prompts on stdin.** After a scan it prints the offered
   device id and fingerprint and asks `Pair with this device? [y/N]`. Launched
   from a script with no stdin, that read hits EOF and the pairing is
   **declined** — the log then reads `Declined. <fingerprint> was not paired.`,
   which looks like a rejection by the peer and is not. A genuine scan was lost
   to this before the driver was rewritten to hold a FIFO open, verify the
   offered device id against the tablet's own `files/trust-store.json`
   `deviceId`, and only then answer `y`.
2. **`clipboard send` reports a capability problem as a transport problem.**
   With `clipboard.v1` ungranted on the tablet, it answers *"that device is not
   currently connected"* while `anyflow status` simultaneously and correctly
   reports the transport connected. Two different questions, one message.
3. **A locked GNOME session blocks the clipboard, and says so precisely.**
   `error: the clipboard did not respond in time. On GNOME Wayland this
   normally means the session is locked: wl-copy and wl-paste cannot obtain a
   seat behind the lock screen.` Confirmed independently with
   `loginctl show-session … -p LockedHint` → `yes`. Worth noting that
   `LockedHint` is exactly the signal §28 tells N2 to treat as authoritative,
   and it answered correctly here.

Also observed, and **out of scope for this wave**: the Android share-sheet
send screen renders unresolved Kotlin template literals —
`${text.byteLength} bytes of text`, `to ${peer.deviceName}`,
`Fingerprint ${peer.fingerprint.toDisplayShort()}` — instead of the
interpolated values. It is cosmetic, it is in `clipboard.v1`'s
`SendActivity`/`SendClipboardScreen`, and N1 touched none of it. Nothing was
changed here; it is filed as a pre-existing defect for a later wave.

#### Device state left behind

The tablet keeps the notification-listener grant, `POST_NOTIFICATIONS`,
`CAMERA`, the `com.android.shell` fixture notification, and the new pairing
with `battery.v1`, `files.v1` and `clipboard.v1` granted to Fedora. The
original `enabled_notification_listeners` value is saved in the session
scratchpad. The adb/MTP flakiness recurred twice during the session and
resolved both times with a replug.

---

NOTIFICATIONS.V1 N1 PASS
N2 READY FOR IMPLEMENTATION

*Both gates passed. Gate 1: the full 35-test connected suite green in a single
run, 0 failed and 0 skipped, with every hardware gate actually executed. Gate
2: a real QR re-pair, a real TLS session, and `ping`, battery and a clipboard
round trip in both directions over it, each verified at the receiving end.*

*This session wrote no production code, no test code and no protocol change.
The working tree is what §24 describes plus this report; `HEAD` is still
`abf4718` and nothing was committed, pushed, or opened as a PR.*
