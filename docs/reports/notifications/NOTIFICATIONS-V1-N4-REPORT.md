# `notifications.v1` — N4: dismissal synchronisation

**Branch:** `feature/notifications-v1-n4-dismiss-sync`
**Date:** 2026-09-10 → 2026-09-12
**Hardware:** Samsung SM-X620 (Android 16 / API 36 / One UI 8.0.5) ↔ Fedora 44,
GNOME Shell 50.4, freedesktop notification spec 1.2

---

## 1. Baseline

N3 merged and certified at `8d8948a`:

```console
$ git log --oneline -3
8d8948a Merge pull request #20 from yurisismotto/feature/notifications-v1-n3-consent-ui
98e8b70 feat(android,desktop): add notifications.v1 consent and privacy UX
4d33205 Merge pull request #19 from yurisismotto/feature/notifications-v1-n2-linux-sink
```

Entering this wave the working tree carried one unrelated, pre-existing change:
the CI duplicate-trigger cleanup in `.github/workflows/portable-windows-msvc.yml`
(dropping `feature/**` from the `push` trigger). It is untouched by N4 and is
reported in §33 rather than claimed as part of this wave.

`git diff --check` was clean at entry and is clean at exit.

---

## 2. Scope

Exactly one user-visible effect was implemented:

> A human dismisses a mirrored notification on the Fedora desktop → AnyFlow asks
> the Android source to dismiss the original → Android dismisses it only when
> every permission, policy, identity and clearability check passes.

Nothing else. No notification actions, no `RemoteInput`, no reply, no
`PendingIntent` execution, no open-app, no snooze, no clear-all, no notification
history. N5 was not started.

**The protocol is unchanged** (§28) — `DismissRequest` and both dismiss roles
already existed, and the existing contract turned out to be sufficient in every
respect.

---

## 3. Files changed

39 files touched, 4 added, **3966 insertions / 294 deletions**.

### New

| File | Lines | What |
| --- | --- | --- |
| `android/…/notifications/NotificationDismiss.kt` | 222 | `NotificationDismissRules` — the pure decision that guards the only remote effect |
| `android/…/notifications/NotificationEcho.kt` | 168 | `EchoSuppression` — one pending listener-cancel, bounded and single-use |
| `android/…/test/…/NotificationDismissRulesTest.kt` | 412 | The decision table and the echo cache, enumerated |
| `desktop/capabilities/notifications/tests/dismiss.rs` | 752 | 33 tests; mostly negative |

### Modified — runtime

**Android.** `NotificationRoleState.kt` (`SOURCING` now `{SOURCE,
DISMISS_TARGET}`), `NotificationSource.kt` (inbound `handleDismiss`, echo
suppression in `handleRemoved`, `ListenerControl.cancel`/`activeNotification`,
role re-announce on policy change, per-peer dismiss counters),
`AnyFlowNotificationListener.kt` (`cancelNotification`, the keyed
`getActiveNotifications`, `REASON_LISTENER_CANCEL` reduced to one boolean),
`NotificationQueue.kt` (`Removed.listenerCancelled`), `NotificationText.kt`
(`DEVICE_ID_HEX_LENGTH`), `NotificationReadiness.kt`
(`NotificationDismissReadiness` + two gates).

**Desktop.** `backend/mod.rs` (`SinkCapabilities.dismiss_reporting`,
`MemorySink::last_server_id`), `backend/dbus.rs` (signal pumps restructured so
`dismiss_reporting` reflects whether the `NotificationClosed` subscription
actually succeeded), `mirror.rs` (`MirrorEntry.origin_device_id`;
`forget_server_id` returns the entry), `roles.rs` (`DISMISS_REPORTER`),
`queue.rs` (`Work::Dismiss`), `lib.rs` (`note_closed` →
`maybe_request_dismissal` → `send_dismiss`, result attribution, counters,
`closes_observed`), `core/src/notification_policy.rs` (docs).

### Modified — surface

`NotificationSettingsScreen.kt`, `NotificationUiMapping.kt`, `MainState.kt`,
`strings.xml` (Android UI); `gui/src/views/notifications.rs` (desktop UI +
`DismissReadiness`); `control/src/lib.rs`, `runtime/src/server.rs`,
`cli/src/main.rs` (reporting).

---

## 4. Dismiss architecture

```text
  a person closes a mirror on this desktop
        │
        ▼  NotificationClosed(server_id, reason = 2)        ← reason 2 ONLY
  MirrorTable::forget_server_id(server_id) -> Some(entry)   ← still live?
        │
        ▼  policy.may_sync_dismissals()   (allow_mirror && allow_dismiss_sync)
        ▼  peer_roles.has(DismissTarget)
        ▼  local_roles.announced(DismissReporter)
  Work::Dismiss ──▶ the peer's worker ──▶ DismissRequest{notification_id, origin}
        │
        ▼  ═══════════════ TLS 1.3, pinned ═══════════════
        │
  Android: NotificationDismissRules.screen(...)             ← six gates
        ▼  SourceIdMap: notification_id -> raw platform key ← the one reverse path
        ▼  activeNotification(key): still there? clearable NOW?
        ▼  echo.arm(idHex, peerHex)                         ← BEFORE the cancel
  cancelNotification(rawKey)
        │
        ▼  onNotificationRemoved(REASON_LISTENER_CANCEL)
  NotificationRemove ──▶ every granted peer EXCEPT the one that asked
```

Five gates on the desktop side, six on the Android side, each failing closed and
each answering a different question. The **order** is load-bearing on both ends
and is asserted by test, not merely documented.

---

## 5. Role model

ADR-0017 §1's v1 assignment, now complete on both ends:

| | Announces | Never announces |
| --- | --- | --- |
| Android | `SOURCE`, `DISMISS_TARGET` | `SINK`, `DISMISS_REPORTER` |
| Linux | `SINK`, `DISMISS_REPORTER` | `SOURCE`, `DISMISS_TARGET` |

**Android announces both source-side roles together because they are true
together.** Both need a bound listener, the OS notification-access grant and a
usable notification secret — and the secret is the reason: a `DismissRequest`
names a notification by its derived id, and the only way back to a platform key
is the `SourceIdMap`, whose entries are built by deriving ids with that secret.
With no secret every dismissal would answer `UNKNOWN_NOTIFICATION` for ever,
which is a promise not kept rather than a capability. If a future platform can
honour dismissals without sourcing, the set stops being one thing; nothing in
the protocol assumes they travel together.

**`DISMISS_REPORTER` has its own input on the desktop**, and this is the part
that was easiest to get wrong. Being able to *display* a notification does not
imply being able to say *why it closed*: `Notify` is a method every server
implements, `NotificationClosed` is a signal this process must be subscribed to.
So `SinkCapabilities.dismiss_reporting` is `false` unless the subscription
actually succeeded. An earlier shape of `DbusSink::spawn_signal_pumps` stored
`Some(receiver)` unconditionally, which would have let a session with no match
rule promise dismissal reports it could never send; the function now returns the
receiver so the capability set can be assembled from whether it exists.

**Neither policy is a role input.** A role says what the machine can do; whether
a particular dismissal may travel is answered per event. Announcing
`DISMISS_TARGET` while `allowDismissSync` is off is correct and deliberate — it
tells a peer "I *can*", and the peer learns "you *may not*" from a
`REJECTED_POLICY` outcome. Collapsing the two would make the desktop unable to
distinguish "that device cannot do this" from "that device refused", which is
exactly the distinction its UI needs.

Epochs remain strictly monotonic from 1, per connection, never persisted. Losing
the listener narrows **both** roles at once — asserted, because a narrowing that
dropped `SOURCE` and kept `DISMISS_TARGET` would leave a desktop sending
dismissals into a device that answers `UNKNOWN_NOTIFICATION` for ever.

---

## 6. Consent model — the two-policy rule

Both ends' `allow_dismiss_sync` are required, and **the canonical design already
specified this**; N4 invented nothing.

| Question | Decided by | Where |
| --- | --- | --- |
| May a local human dismissal be *propagated*? | the desktop's `allow_dismiss_sync` | `desktop/core/src/notification_policy.rs` |
| May a remote device *act* on this phone? | Android's `allowDismissSync` | `NotificationPolicy.kt` |

ADR-0015 §6 approves `allow_dismiss_sync` defaulting to `false`, opt-in **per
peer**, and both stores already held the field with that default —
`desktop/core`'s documented as *"whether a human closing a mirror here may
dismiss the notification on the source device"* and Android's as the source-side
authority. That is the split the brief's §4 security preference asks for, and it
is what is implemented and what §26's hardware matrix verifies.

Containment is applied on both ends identically: `allow_mirror` off makes a
stale dismiss flag inert (`may_sync_dismissals()` on the desktop,
`!policy.allowMirror || !policy.allowDismissSync` on Android). A missing policy
is never read as enabled — verified for an absent field, an empty object and a
null store.

The grant is re-read at the moment the dismissal would be sent, not reused from
the upsert that put the notification on screen: a person who revoked the grant a
second ago has not authorised a message the desktop is about to send.

---

## 7. Android — the dismiss target

`NotificationDismissRules.screen()` is a pure function of a decoded
`DismissRequest`, this device's id, the effective policy, and one boolean
("can this phone act at all"). The order is the specification:

| # | Check | Failure |
| --- | --- | --- |
| 1 | `notification_id` is exactly 16 bytes | **refused and not answered** |
| 2 | `origin_device_id` is 32 lowercase hex | `INVALID` |
| 3 | the peer holds a `notifications.v1` grant | `NOT_AUTHORIZED` |
| 4 | this phone can act (listener + access + secret) | `REJECTED_ROLE` |
| 5 | `allowMirror && allowDismissSync` | `REJECTED_POLICY` |
| 6 | `origin_device_id` names **this** device | `INVALID` |

Only then does anything touch the platform: the `SourceIdMap` lookup, the live
`activeNotification(key)` re-read, and the cancel.

Two properties of that order are asserted rather than assumed:

* **A bad-width id is decided first**, because it decides whether an answer is
  possible at all (ADR-0016 §9) — `assertEquals(Unanswerable, screen(4-byte id,
  ungranted, wrong origin, cannot act))`.
* **`DismissRequest` is not a lookup oracle.** A peer that fails an earlier gate
  never reaches the id lookup, so the answer cannot be used to learn whether a
  notification exists on the phone. A peer with the setting off is told
  `REJECTED_POLICY` and never learns whether it even named the right device.

The raw Android key is confined to `handleDismiss`'s local `platformKey` and the
`cancel(platformKey)` call. It is not transmitted, not persisted, not logged, not
put into an answer, and not counted — verified three ways in §17 and §19.

---

## 8. Linux — the dismiss reporter

`note_closed` is the whole of it, and its shape is the design:

```rust
let Some((id, entry)) = state.mirrors.forget_server_id(closed.id) else { continue };
// … log the close, with a reason and an opaque id prefix …
if closed.reason.is_human_dismissal() {
    self.maybe_request_dismissal(&slot, id, entry.origin_device_id).await;
}
```

`is_human_dismissal()` is a single `matches!` on a single variant, deliberately
not a list of exclusions: a rule written as "everything except expiry" grows a
hole the day a fifth reason is added, and the hole would clear somebody's phone.

The request is **queued for the peer's own worker** rather than sent from the
close pump, so one peer's traffic stays a single ordered producer and a dismiss
can never overtake the result or role announcement that preceded it.

`origin_device_id` is echoed back from the upsert that created the mirror,
because the source requires that the message names the source. It is retained on
`MirrorEntry` for that reason alone; it is not identity and is not what addresses
the entry — the pinned fingerprint is.

A dismiss for a departed peer is **dropped, never queued** (ADR-0015 §6),
verified by reconnecting and proving nothing arrives on the new session.

---

## 9. `SourceIdMap` usage

Unchanged from N1 except in its documentation, which now says what it is for.
This map is the single reverse path in the design: a peer's sixteen opaque bytes
become a platform key here and nowhere else, and only if this device put them
there itself.

No remote field can name an Android notification. Asserted directly: the
fixture's real platform key, padded to sixteen bytes and sent as a
`notification_id`, resolves to nothing — the map is keyed on HMAC output, which a
peer cannot construct. A notification that was never mirrored (denied app) has no
id issued for it at all.

The map stays memory-only, bounded at `MAX_TRACKED_NOTIFICATIONS`, cleared with
the listener lifecycle, and rebuilt by re-deriving over `getActiveNotifications`.
An id no longer present converges as `UNKNOWN_NOTIFICATION`, and a notification
the platform has dropped also has its entry forgotten so the map does not hold a
key nothing can act on again.

---

## 10. Clearability rules

Read from the **live** platform, never from the wire value the desktop holds:

```kotlin
current == null                        -> UNKNOWN_NOTIFICATION
!current.clearable || current.ongoing  -> NOT_DISMISSIBLE
else                                   -> REMOVED
```

`activeNotification(key)` uses `getActiveNotifications(arrayOf(key))` — the keyed
overload, so the platform is asked about exactly the notification in hand rather
than handing this process the whole shade to filter.

Both flags are consulted, not one standing in for the other: `isClearable()` is
already false for an ongoing notification on every Android AnyFlow supports, but
they are separate platform concepts and a rule that relies on one implying the
other breaks on the release where it stops.

The case this exists for is covered explicitly: a notification mirrored as
`dismissible = true` which the app then re-posts as ongoing. The desktop's copy
is stale; the phone refuses `NOT_DISMISSIBLE`. All five cases from the brief are
tested — normal clearable, ongoing, non-clearable, removed between mirror and
request, updated between mirror and request.

---

## 11. Human-dismiss detection

`CloseReason` was already correct from N2; N4 acts on it. Mapping re-verified
against real GNOME Shell 50.4 (§21) and against the product path four times
(§22).

| freedesktop reason | `CloseReason` | May travel? |
| --- | --- | --- |
| 1 expired | `Expired` | **No** |
| 2 dismissed by the user | `Dismissed` | **Yes — the only one** |
| 3 closed by `CloseNotification` | `Closed` | **No** |
| 4 undefined, and anything future | `Undefined` | **No** |

An expiry must never dismiss the phone's notification, and that is the single
easiest way to build a feature users would rightly call broken: a desktop banner
times out on a screen nobody is looking at, every day.

---

## 12. Echo and loop suppression

The critical sequence converges silently, and it does so for **two independent
reasons**, which is worth separating because the weaker one is the one that looks
like the mechanism:

1. **The desktop purges its mirror before it sends the request.** So the
   `NotificationRemove` that follows finds nothing and answers
   `UNKNOWN_NOTIFICATION` — an answer, not an error, and not something that
   produces another message. *This* is what makes a second lap impossible.
2. **The source suppresses the echo to the peer that asked.** `EchoSuppression`
   removes a redundant message. On GNOME the un-suppressed removal is a wasted
   D-Bus call; on a spec-literal server (dunst, mako) it is a D-Bus **error**, on
   every dismissal, for ever.

Saying which is which bounds what a bug in the cache can cost: an entry that
expires early produces a removal the desktop converges on; one that never
arrives expires and swallows nothing. Neither can dismiss anything or loop.

The cache is single-use (so an app re-posting under the same key does not have
its *next*, genuine removal swallowed), 10 s TTL on `SystemClock.elapsedRealtime`
injected as `() -> Long`, bounded at 64, armed strictly **before** the cancel
because `onNotificationRemoved` can arrive on the main thread while
`cancelNotification` is still returning, and released immediately when the cancel
fails.

Only a removal the platform attributes to `REASON_LISTENER_CANCEL` consults the
cache. A person swiping the notification away on the phone in the same ten
seconds is a genuine removal and reaches every peer — asserted, because
suppressing it would strand a mirror nothing could take off.

**The removal reason is never transmitted.** It is reduced to one boolean at the
listener callback so no portable type ever holds the number, which is what stops
a future implementation branching on `REASON_PACKAGE_BANNED` or
`REASON_CLEAR_DATA`.

---

## 13. Idempotency and races

Every race in the desktop design resolves in one function, and this is the part
of N4 that is structural rather than careful:

> `MirrorTable::forget_server_id` answers `Some` only when the close signal
> found a mirror that was **still live**, and everything that removes a mirror
> for any other reason removes the entry *first* and closes the notification
> afterwards.

So each of these reaches `note_closed` and sends nothing, with no timer, no flag,
no window and no cache to expire at the wrong moment — each one a test in
`tests/dismiss.rs`:

| Race | Why it is inert |
| --- | --- |
| duplicate `NotificationClosed` | the first consumed the entry |
| stale id after a server restart | `invalidate_server_ids` dropped every handle |
| our own `CloseNotification` returning | `apply_remove` removed the entry first |
| snapshot reconciliation | same |
| lock policy `Suppress` | same |
| per-peer ceiling eviction | same |
| grant revocation / role narrowing / grace expiry | `close_all` drained the table |
| a close for an id this desktop never used | no entry |
| a mirror whose `display` failed | it has no server id at all |

Plus the ones with a positive half:

* **dismiss twice** — first `REMOVED`, second `UNKNOWN_NOTIFICATION`, and the
  platform asked *exactly once*;
* **re-posted identity** — a closed freedesktop id is dead, so the re-post gets a
  fresh server id; a late signal for the *old* number sends nothing and the
  current one still works;
* **update then dismiss** — an update replaces in place and keeps its server id,
  so the dismissal still names the right identity;
* **already-removed on the phone** — converges `UNKNOWN_NOTIFICATION`, cancels
  nothing.

### Mutation testing

Because most of these tests are negative, each gate was removed in turn and the
suites re-run, to prove the assertions bite rather than pass vacuously:

| Gate removed | Failing tests |
| --- | --- |
| desktop: `is_human_dismissal()` | 4 |
| desktop: `may_sync_dismissals()` | 3 |
| desktop: peer/local role check | 1 |
| Android: `allowMirror && allowDismissSync` | 7 |
| Android: origin-is-this-device | 2 |
| Android: echo suppression | 3 |
| Android: clearability re-check | 3 |

The first run of this exercise found a real weakness: `dismiss_sync_is_off_by_
default_and_sends_nothing` passed with the policy gate removed, because the
harness's peer had never announced `DISMISS_TARGET` and the *role* gate was doing
the work. The test now opens every other gate explicitly so the policy is the
only thing holding it.

A second weakness found and fixed the same way: `expect_no_dismiss` could race
the close pump, because `MemorySink::user_closes` returns as soon as the signal
is buffered — before `note_closed` has run. A negative assertion made in that
window would pass because the thing it was watching for had not happened *yet*,
which is the most comfortable kind of wrong. `NotificationManager::closes_observed`
is a monotonic count of *fully decided* close signals, and `Harness::close` waits
on it; no test calls `user_closes` directly any more.

---

## 14. Result handling

The existing `NotificationResult` / `NotificationOutcome` contract, with no
ad-hoc acknowledgement anywhere. Android answers `REMOVED`, `UNKNOWN_NOTIFICATION`,
`NOT_DISMISSIBLE`, `REJECTED_POLICY`, `REJECTED_ROLE`, `NOT_AUTHORIZED`,
`INVALID`, `UNAVAILABLE` or `FAILED`; a bad-width id is refused and not answered.

No exception text, platform error string, package name or D-Bus message reaches a
peer — the answer is an identity and an enum, asserted against the encoded bytes
(`< 32 bytes`) rather than against the fields.

**Attribution needs no correlation state**, which is worth spelling out because
the obvious implementation would be a table of outstanding ids — exactly the
dismiss event journal §25 forbids. Every `NotificationResult` reaching this
desktop is a verdict on a `DismissRequest`, because a `DismissRequest` is the only
answerable message a sink-only device ever sends. So:

* `REMOVED` → success;
* `UNKNOWN_NOTIFICATION` → **convergence, not a refusal.** Both ends agreeing it
  is gone is correct, and counting it as a refusal would tell a user their phone
  said no when it did not;
* everything else → the source declining.

---

## 15. Android UI

The inert N3 text is now a real switch, done **last**, after the runtime was
implemented and tested.

**"Allow this computer to dismiss notifications"**, default **off**, written as a
security control rather than as a feature description:

> Off. When on, dismissing a mirrored notification on that computer also
> dismisses the original here. That is all it can do: it cannot tap a
> notification's buttons, reply, open an app, or clear everything at once.
> Ongoing notifications are never dismissed this way.

The second sentence exists because *"allow this computer to dismiss
notifications"* is exactly the phrase somebody could read as *"allow this
computer to control my notifications"*, and the copy has to say which of the two
it is. A test asserts all four bounds are on screen, and a second test asserts
that no control mentioning replies, actions, clear-all, snooze or open-app exists
anywhere on the page.

Below the switch, a state line from `NotificationDismissReadiness` — because the
switch's position says on or off, and what a person also needs is whether "on" is
currently doing anything:

| State | Line |
| --- | --- |
| `NEEDS_ANDROID_ACCESS` | Android has not given AnyFlow notification access, so nothing on this device can be dismissed from anywhere. |
| `OFF` | Off. Dismissing a notification on that computer leaves this one alone. |
| `NOT_CONNECTED` | On. It takes effect the next time that computer connects. |
| `PEER_CANNOT_REPORT` | On, and that computer has not said it can report when you dismiss something. … If you have just changed a setting there, the two may need to reconnect. |
| `ACTIVE` | On. Dismissing a mirrored notification on that computer dismisses the original here. |

`OFF` and `NOT_CONNECTED` are **not** warnings: one is a choice and the other is
a device that is merely elsewhere, and drawing either as a fault trains people to
ignore the amber badge that means something is wrong.

The whole card sits inside `if (granted)`, so a control that could not be honoured
is never offered. Every string is a resource (§30). The switch writes
`policy.copy(allowDismissSync = it)` and nothing else — asserted field by field
against the grant, the app list, the ongoing and work-profile switches and the
lock policy.

Diagnostics now render role *sets* (`SOURCE + DISMISS_TARGET`, `SINK +
DISMISS_REPORTER`) and a content-free `N asked · N done` line.

---

## 16. Desktop UI

The "Not available yet" row is now a real switch, **"Let this computer dismiss
notifications on the device"**, default off, writing
`NotificationSetting::DismissSync` — one variant carrying one boolean, which
cannot reach the grant, the mirror switch or the lock policy.

`DismissReadiness` is a second pure function with its own decision table, for the
reason §15 of the brief gives: it is entirely ordinary for mirroring to be
`Ready` while dismissal is unavailable, and one "Ready" bit would have to lie
about whichever half was worse. The states are `Off`, `NoReporting`,
`NotConnected`, `PeerCannotDismiss`, `Active`; only the middle two and
`PeerCannotDismiss` draw a warning badge.

When the peer has not announced `DISMISS_TARGET`, the switch **still shows the
stored choice** — it is the person's — and the line underneath says plainly that
nothing will happen and what would change it. A refused capability must not
silently rewrite a stored setting, and that is asserted.

---

## 17. Readiness

Mirroring readiness and dismissal readiness are deliberately **not** collapsed.
Asserted on both ends:

```rust
// desktop/gui/src/views/notifications.rs
assert_eq!(Readiness::of(&p, true), Readiness::Ready);
assert_eq!(DismissReadiness::of(&p), DismissReadiness::PeerCannotDismiss);
```

```kotlin
// android/…/NotificationReadinessTest.kt
assertEquals(NotificationReadiness.READY, g.readiness())
assertEquals(NotificationDismissReadiness.PEER_CANNOT_REPORT, g.dismissReadiness())
```

Every value of both enums is reachable from a real gate combination, asserted
exhaustively by set equality against `entries`. Protocol role names appear only
in the Android Details section and in `anyflow notifications status`.

### The mid-session grant debt (brief §17)

N3's finding is **confirmed on hardware and not redesigned.** Observed this wave:
after granting `notifications.v1` on the tablet mid-session, the phone announced
`no role · epoch 1` and the desktop's announcement was dropped, because the
session predated capability negotiation — so the listener never bound and the app
picker could not see a notifying app. A disconnect/reconnect from the tablet's
own Connection row fixed it immediately, after which both ends announced their
full role sets.

N4 did not touch HELLO or session negotiation. What it did do is make the copy
truthful: the desktop's `PeerNotSourcing` detail and the `PeerCannotDismiss` /
`PEER_CANNOT_REPORT` lines on both ends now end with *"If you have just changed
either, the two may need to reconnect before it takes effect."* Recorded as a
debt for N5 (§31).

---

## 18. Privacy

`DismissRequest` carries an identity and a device id. Nothing was added to it, to
any result, or to any diagnostic: no title, body, app label, package name, raw
Android key, D-Bus text or notification content.

Logs carry only a peer fingerprint prefix, a derived-id prefix, an outcome enum
and a reason class. Every `DismissRefusal` value is a screaming-snake name, and a
test asserts the enum cannot carry data.

**No dismiss event history exists.** The counters are counts: per-connection on
Android (`dismissRequests`, `dismissesPerformed`), per-peer in memory on the
desktop (`dismissals_sent`, `dismissals_refused`). Which notification was
dismissed is deliberately not retained anywhere.

---

## 19. Logging canary

### Desktop (`tests/logging.rs`, 11 tests, 4 new)

`TRACE`-level capture with a scoped subscriber, canaries that appear nowhere
else, and a non-empty assertion first so the suite cannot pass vacuously. New
flows: the successful dismissal path, a policy-refused dismissal, a non-human
close, and an inbound dismiss refusal. Each also asserts the *wanted* diagnostic
is present — `"asked the source to dismiss it too"`, `"a human dismissal was not
sent to the source"` with `reason = "policy"` — so a clean log is not a silent
one.

### Android (`NotificationLoggingCanaryTest`, 13 tests, 5 new)

The NOTIF-SEC-25 pattern extended: real logcat, read through the instrumentation
shell (the app holds no `READ_LOGS` and must not), canaries that include **the
raw platform key** `0|example.canary.q7x.app|4711|…|10123`.

New flows exercise a permitted dismissal, every refusal (bad width, bad origin,
another device's origin, unknown id, ungranted peer), a policy refusal, a
non-dismissible refusal, and — the one that matters — a `cancel` that **throws
with the canaries and the raw key in its exception message**, which is the shape
of exception a careless `catch` would log verbatim. Each asserts the effect
really happened (`listener.cancelled.contains(KEY)`) so the audit is not vacuous.

### Live hardware audit

From the real E2E, before any test wiped device state:

```console
$ adb logcat -d NotificationSource:V AnyFlowListener:V … '*:S'
09-10 19:40:41.387  I NotificationSource: dismiss refused: DISMISS_SYNC_OFF for 003a10db
09-10 19:46:04.874  I NotificationSource: dismissed at a peer's request: f7d617b7
```

59 AnyFlow lines captured; zero occurrences of `ANYFLOW-N4-E2E`, `BODY-N4-E2E`,
`anyflow-n4-`, `com.android.shell`, `|2020|` or `|2000`. The desktop's two logs:
zero occurrences of any of them either.

---

## 20. Persistence audit

### Desktop

```console
$ find ~/.local/share/anyflow -type f
~/.local/share/anyflow/identity.key
~/.local/share/anyflow/state.json
```

Every leaf key in `state.json`: `allow_dismiss_sync, allow_mirror, allow_receive,
allow_send, auto_grant, auto_receive, auto_send, certificate_der_b64, device_id,
device_name, fingerprint, key_backing, last_protocol_version, listen_port,
paired_at_unix, platform, revoked, schema_version, v1, when_sink_locked`.

Three notification keys, all policy. No notification id, no server id, no
dismiss journal, no content.

### Android

```console
$ adb shell run-as io.github.yurisismotto.anyflow find /data/data/… -type f
/data/data/io.github.yurisismotto.anyflow/files/trust-store.json
/data/data/io.github.yurisismotto.anyflow/files/profileInstalled
/data/data/io.github.yurisismotto.anyflow/shared_prefs/android.app.ActivityThread.IDS.xml
```

`notificationPolicy` keys: `allowDismissSync, allowMirror, allowedApps,
includeOngoing, includeWorkProfile, knownApps, whenSourceLocked` — the field set
pinned by `the_stored_form_has_exactly_the_documented_fields`.

Grep across everything the app owns, after the full E2E:

| Pattern | Files |
| --- | --- |
| `ANYFLOW-N4-E2E`, `BODY-N4-E2E`, `anyflow-n4-` | 0 |
| `com.android.shell\|2020` (the raw key form) | 0 |
| `f7d617b7`, `003a10db` (derived ids) | 0 |
| `notificationId`, `dismissHistory`, `dismissLog`, `serverId` | 0 |

`/sdcard/Android/data/io.github.yurisismotto.anyflow/` does not exist.

`com.android.shell` *does* appear in `allowedApps` and `knownApps` — a package
name the user chose in the picker, which the design explicitly permits as a
setting.

The brief's ordering was followed: pair → configure → E2E → persistence sweep →
`connectedDebugAndroidTest` last, because the connected suite uninstalls the app
and takes the pairing with it.

---

## 21. Android JVM suite

```console
$ ./gradlew testDebugUnitTest --rerun-tasks
BUILD SUCCESSFUL
```

**tests=510  failures=0  errors=0  skipped=0** (N3: 447 → **+63**).

New: 24 dismissal cases in `NotificationSourceTest`, 27 in
`NotificationDismissRulesTest` (decision table) + `EchoSuppressionTest`, 9
dismissal-readiness cases, 4 policy-persistence cases, plus the role-set
rewrites in `NotificationRolesTest`.

---

## 22. Android connected suite

```console
$ ANDROID_SERIAL=RX2Y500C7SY ./gradlew :app:connectedDebugAndroidTest
Finished 96 tests on SM-X620 - 16
BUILD SUCCESSFUL in 2m 54s
```

JUnit XML: **tests=96  failures=0  errors=0  skipped=0**

| Class | Tests |
| --- | --- |
| `NotificationConsentUiTest` | 30 |
| `AppPickerUiTest` | 18 |
| `NotificationLoggingCanaryTest` | 13 |
| `NotificationHardwareGateTest` | 10 |
| `ClipboardInstrumentedTest` | 9 |
| `DownloadsTest` | 5 |
| `DeviceIdentityTest` | 4 |
| `NotificationSecretInstrumentedTest` | 4 |
| `ClipboardPersistenceTest` | 3 |

Getting here took five runs and the failures were instructive, so they are
recorded rather than smoothed over:

| Run | Result | Cause |
| --- | --- | --- |
| 1 | 96, 2 failures | two stale assertions of mine: an over-broad Compose substring match, and `NotificationHardwareGateTest` still asserting N1's "must not claim `DISMISS_TARGET`" |
| 2 | 96, 5 failures | one more stale role assertion, + 4 `ClipboardInstrumentedTest` window-focus failures (Android Settings still held focus from my earlier navigation) |
| 3 | 96, 8 failures | **all notification tests passed**; 8 clipboard window-focus failures, made worse by my own periodic `KEYCODE_WAKEUP` nudges stealing focus |
| 4 | partial (20, 22) | the tablet's Wi-Fi adbd died mid-suite |
| 5 | **96, 0 failures** | after diagnosing and clearing the real cause |

The clipboard failures were **not** an N4 defect and not even an AnyFlow defect.
They failed identically when the class was run alone, which ruled out contention.
The cause was a stuck `NotificationShade` window
(`com.android.systemui`, `ty=NOTIFICATION_SHADE`, `shown=true`) holding window
focus **device-wide** — `am start` of Settings could not take focus either, which
is the quick way to tell this apart from an app bug. `cmd statusbar collapse`,
`KEYCODE_HOME`, `KEYCODE_BACK`, swipes and `am force-stop com.android.systemui`
all failed to release it; `adb reboot` plus a swipe past the boot keyguard fixed
it, and the same suite then ran 96/96. N4 modifies no clipboard file
(`git status | grep -i clip` is empty).

---

## 23. Rust regression

```console
$ cargo fmt --all --check                                        # clean
$ cargo build --workspace --locked -j 2                          # ok
$ cargo test  --workspace --locked -j 2                          # 644 passed, 0 failed, 21 ignored
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings   # 0 warnings, 0 errors
```

Explicit ignored gates:

```console
$ cargo test -p anyflow-capability-notifications --test real_dbus -- --ignored --test-threads=1
test result: ok. 9 passed; 0 failed
$ cargo test -p anyflow-capability-notifications --test real_lock -- --ignored --test-threads=1
test result: ok. 2 passed; 0 failed
$ cargo test -p anyflow-gui -- --ignored --test-threads=1
test result: ok. 1 passed; 0 failed        # the widget-tree / accessibility gate
```

Notification test counts: `tests/dismiss.rs` 33 (new), `tests/sink.rs` 60,
`tests/logging.rs` 11, crate unit tests 55, `daemon/tests/notifications.rs` 27,
`tests/real_dbus.rs` 9.

`desktop/gui` needs `. ~/.local/gtk4-prefix/ENV.sh` sourced on this machine
(pre-existing environment fact, N0 debt 5).

---

## 24. Real freedesktop / D-Bus gate

GNOME Shell **50.4**, spec **1.2**, vendor GNOME; capabilities `actions, body,
body-markup, icon-static, persistence, sound`.

| Brief §22 requirement | Result | Evidence |
| --- | --- | --- |
| 1. mirrored notification appears | **PASS** | four real Android notifications displayed on the desktop |
| 2. human dismissal produces the expected close reason | **PASS** | `reason="dismissed"` in the daemon log, four times, from four physical dismissals |
| 3. programmatic close does not look like a human dismissal | **PASS** | `the_server_reports_our_own_close_with_reason_three` — reason 3 against the real server |
| 4. expiry does not produce remote dismissal | **PASS** | asserted in `tests/dismiss.rs`; the real path covered by matrix rows 1 and 3 sending nothing |
| 5. replacement / `replaces_id` still maps correctly | **PASS** | `a_notification_is_created_replaced_in_place_and_closed` — three calls, id preserved |
| 6. one physical dismissal → at most one AnyFlow request | **PASS** | `dismissals: 2 sent, 1 declined` across four dismissals; rows 1 and 3 sent zero |

New this wave: `the_real_session_can_report_human_dismissals` asserts
`capabilities().dismiss_reporting` against the real bus — the single input to
`DISMISS_REPORTER`, and false unless the `NotificationClosed` subscription
actually succeeded on this session.

Two further `real_dbus` tests were added and **skipped loudly** (they require
`ANYFLOW_HUMAN_DISMISS=1` and a person):
`a_human_dismissal_on_this_desktop_is_reported_as_reason_two` and
`a_human_dismissal_end_to_end_produces_exactly_one_dismiss_request`. They were
not needed, because §25 performed the same certification four times through the
**product** path rather than a synthetic one, which is strictly stronger.

---

## 25. Human-dismiss certification and the policy matrix

Both are the same run, so they are reported together. Every dismissal was a
**physical, GUI-level action by the operator** on the real GNOME notification —
no `CloseNotification`, no injected signal, no mock, no test seam. AT-SPI
automation of the GNOME banner was attempted and abandoned: gnome-shell's
accessibility tree exposes 1567 nodes but no banner close action reachable by
name, so the brief's documented-manual-action path was used instead.

Setup, all through real UIs: pairing by camera QR scan; the desktop
`notifications.v1` grant through the GTK switch driven by **AT-SPI**
(`actions: ['toggle']`); the Android grant, the OS notification-access toggle in
Settings, the app picker (one app: `com.android.shell`) and the Android
dismiss-sync switch through **real touch events**. No JSON edits, and no CLI
policy command as a substitute for a UI.

| # | Android | Desktop | Expected | Observed |
| --- | --- | --- | --- | --- |
| 1 | OFF | OFF | no remote dismissal | `reason="dismissed"`, then `not sent to the source, reason="policy"`. Nothing sent. **PASS** |
| 2 | OFF | ON | no remote dismissal | one request sent; tablet `dismiss refused: DISMISS_SYNC_OFF`; wire outcome `RejectedPolicy`. **PASS** |
| 3 | ON | OFF | no remote dismissal | `not sent to the source, reason="policy"`; zero new lines in the tablet's log. **PASS** |
| 4 | ON | ON | dismissal allowed | `asked the source to dismiss it too`; tablet `dismissed at a peer's request: f7d617b7`; wire outcome `Removed`. **PASS** |

### The device-side proof

The decisive evidence is not the desktop's log but the **tablet's own account of
its shade**, obtained two independent ways.

First, through the protocol: restarting the daemon made the phone reconnect and
send an active-state snapshot of its live shade.

```console
snapshot opened  peer=9C9C 3BD7 8914 02D0
notification upsert … notification=c2a0acfa  outcome="displayed"   # row 3 survivor
notification upsert … notification=003a10db  outcome="displayed"   # row 2 survivor
notification upsert … notification=59f7cb3d  outcome="displayed"   # row 1 survivor
… four unrelated shell notifications …
snapshot complete  named=7  closed=0
```

`f7d617b7` — the one the desktop was permitted to dismiss — **is not named**, and
the snapshot itself produced zero dismiss requests.

Second, directly from the platform:

```console
$ adb shell cmd notification list | grep anyflow-n4-
  anyflow-n4-a: 1      # row 1, both OFF        -> survived
  anyflow-n4-b: 1      # row 2, Android OFF     -> refused, survived
  anyflow-n4-c: 1      # row 3, Desktop OFF     -> never asked, survived
  anyflow-n4-d: 0      # row 4, both ON         -> dismissed, exactly once
```

Nothing failed open at any point, and one physical dismissal never produced more
than one request.

### Not executed

**The ongoing / non-clearable hardware fixture (brief §23 step 14).**
`cmd notification post` has no flag for `FLAG_ONGOING_EVENT` or
`FLAG_NO_CLEAR` — its full option list is title, icon, large-icon, style,
content-intent and user — so the fixture is not creatable by the documented adb
path. The step is explicitly conditional on that. The path itself is covered by
four JVM tests including the stale-wire-value case, and the live re-check runs
through the real `activeNotification()` seam.

**Battery / files / clipboard unaffected (step 15).** Verified by the connected
suite rather than by hand: `ClipboardInstrumentedTest` 9/9,
`ClipboardPersistenceTest` 3/3, `DownloadsTest` 5/5, `DeviceIdentityTest` 4/4,
all green on the device after the N4 E2E.

---

## 26. N3 regression

| N3 property | Result | How |
| --- | --- | --- |
| mirroring works with dismiss sync OFF | **PASS** | matrix rows 1 and 3 mirrored and displayed normally |
| Android app picker deny-by-default | **PASS** | `AppPickerUiTest` 18/18; on hardware the picker opened at "No app chosen" / "1 of 98 apps" after one tick |
| Android lock `AppOnly` unchanged | **PASS** | JVM suite; `whenSourceLocked: APP_ONLY` persisted untouched by the dismiss switch |
| desktop lock `AppOnly` unchanged | **PASS** | `real_lock` 2/2; `the session locked; reduced what is on the screen reduced=7` observed live |
| update still replaces | **PASS** | `an_update_replaces_the_same_notification_rather_than_adding_one`; real-D-Bus `replaces_id` gate |
| Android source remove still closes the desktop mirror | **PASS** | `a_source_removal_generates_no_dismiss_request` asserts the close *and* that it sends nothing |
| grant revocation still closes mirrors | **PASS** | `a_grant_revocation_generates_no_dismiss_request`; live `closed every mirror for a peer … reason="revoked"` |
| no notification content persisted | **PASS** | §20, both ends |
| no content in AnyFlow logs | **PASS** | §19, both ends, plus the live hardware audit |
| accessibility refresh fix still holds | **PASS** | the widget-tree gate passes with the third switch present; `the_dismiss_switch_survives_refreshes_and_still_follows_the_daemon` is new |
| AT-SPI control identity stable under unchanged polling | **PASS** | at the shipped `REFRESH_SECS = 2`, ten unchanged refreshes leave all three switches object-identical; and the real grant was driven through AT-SPI on live hardware |

---

## 27. Two-policy matrix

See §25. All four rows PASS with device-side evidence. Nothing failed open.

---

## 28. Protocol guard

```console
$ git diff --exit-code -- protocol/
  protocol/ is UNCHANGED (zero diff)
```

`notifications_v1.proto` was not edited, and no change was needed. `DismissRequest`
carried exactly the two fields required; `NotificationOutcome` had every verdict
the implementation needed, including the two that make idempotency work
(`UNKNOWN_NOTIFICATION`, `NOT_DISMISSIBLE`); and both dismiss roles already
existed with the epoch mechanism to narrow them.

One place where the contract's shape decided an implementation question rather
than merely permitting it: because `NotificationResult` echoes only an id, a
malformed id is *unanswerable*, and that is why the width check must come first on
both ends.

---

## 29. Security scope guard

```console
$ grep -rn "cancelNotification" android/app/src/main/java/ | grep -v '^\s*\*\|//'
  …/notifications/AnyFlowNotificationListener.kt:92:  cancelNotification(platformKey)
```

One call, one argument, one caller. Every other occurrence in the tree is prose.

| Forbidden | Present? |
| --- | --- |
| notification action execution | **No** |
| `RemoteInput` / reply | **No** — 3 files, all prose asserting absence |
| `RemoteViews` | **No** — 2 files, both prose |
| open-app / arbitrary `Intent` from a remote notification | **No** |
| `PendingIntent` from a remote notification | **No** — 6 files reference it; all six are AnyFlow's *own* UI (`ConnectionService` foreground notification, `ClipboardTileService`, `ClipboardNotifications`) or prose. None is on the `notifications.v1` path |
| notification history | **No** — §20 |
| remote clear-all | **No** — `cancelAllNotifications`: 0 files |
| remote snooze | **No** — `snoozeNotification`: 0 files. The single `"Snooze"` string in the diff is in a *negative* test asserting no such control exists |
| arbitrary package/id/tag cancellation | **No** — the only handle is the HMAC-derived id; a padded real platform key resolves to nothing (§9) |

The desktop's D-Bus `actions` array is `Vec::new()` and always will be, so there
is nothing for an `ActionInvoked` signal to be about.

---

## 30. Internationalization

Every new Android user-facing string is a resource: `notif_dismiss_title`,
`notif_dismiss_description`, the five `notif_dismiss_state_*`,
`notif_dismiss_counts`, `notif_diag_dismiss`, `notif_diag_role_dismiss_target`,
`notif_diag_role_dismiss_reporter`. No user-facing text was inlined in Compose.

The desktop GUI has no localization mechanism at all, so its strings remain
inline — pre-existing, N3 debt 8, unchanged by this wave rather than worsened by
it. Notification content itself is never translated.

---

## 31. Accessibility

**Android.** The dismiss switch is an `AnyFlowCapabilityRow`, so it carries an
accessible label and a toggleable state like every other switch on the page.
`switchFor("Allow this computer to dismiss notifications")` — a query by
`isToggleable() and hasContentDescription(...)` — resolves and reports on/off in
four instrumented tests.

**Desktop.** Keyboard-operable and AT-SPI reachable, and this was verified on the
real desktop rather than only in a test: the `notifications.v1` grant for the
certification peer was granted by invoking the GTK switch's AT-SPI `toggle`
action, and the tree exposes `switch: 'Let this computer dismiss notifications on
the device' [unchecked]`.

**The N3 widget-rebuild defect is not reintroduced.** At the shipped
`REFRESH_SECS = 2`, ten unchanged refreshes leave all three switches
**object-identical** (GObject identity is exactly what AT-SPI holds a reference
to), still parented and still visible; the lock-policy list box likewise. And the
other half of the property holds: a changed dismiss policy does redraw, switch
state and caption together. Both are in `the_notifications_page_widget_tree`.

---

## 32. Windows CI readiness

Not claimed locally, per the brief. `desktop/capabilities/notifications` keeps its
`linux-dbus` feature gate: `backend/dbus.rs` and `backend/logind.rs` are behind
it, and `tests/real_dbus.rs` and `tests/real_lock.rs` are `#![cfg(feature =
"linux-dbus")]` so they compile to empty test binaries with the feature off.
`tests/dismiss.rs` is new and **portable** — it names no bus, no desktop and no
`target_os`, and drives `MemorySink`/`MemoryLock` only. The portable-boundary test
(`desktop/core/tests/portable_boundary.rs`) is green in the workspace run.

The pre-existing CI trigger cleanup in the working tree is what makes the Windows
job run once rather than twice per push; it is unrelated to N4.

---

## 33. Debts and risks

1. **Mid-session grant needs a reconnect** (N3's finding, confirmed on hardware
   in §17). Not redesigned here. The copy on both ends now says so. **This is
   the strongest N5 candidate**: it is the one state where a person who has done
   everything right sees nothing happen.
2. **The ongoing / non-clearable hardware fixture is not creatable via adb**
   (§25). A one-activity fixture app that can post an ongoing notification on
   demand would close this and also retire N3's debt 3 — `com.android.shell` only
   appears in the picker while it is actually notifying, which needs the listener
   bound, which needs a granted peer connected.
3. **This tablet's adb is unreliable on both transports.** USB falls back to
   MTP-only with no host-side recovery (N3 debt 4, still true), and
   adb-over-Wi-Fi — enabled first thing this wave, as N3 recommended — dies when
   the tablet sleeps. It cost one truncated suite run. `settings put system
   screen_off_timeout 1800000` helps; periodic `KEYCODE_WAKEUP` **does not** and
   actively steals window focus.
4. **A stuck `NotificationShade` window breaks every focus-dependent instrumented
   test, device-wide, and only a reboot clears it** (§22). Worth checking
   `dumpsys window | grep mCurrentFocus` before believing a focus failure is a
   code defect.
5. **The desktop GUI still has no localization mechanism** (N3 debt 8).
6. **One UI ignores `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME`** (N3 debt 6) —
   reconfirmed. The intent resolves to `NotificationAccessDetailsActivity` but
   lands on the full app list, one tap from AnyFlow's own row.
7. **`knownApps` is still a second per-peer list of package names** (N3 debt 5),
   unchanged.
8. **Work-profile installation is still undetectable** (N3 debt 7), unchanged.
9. **Pairing still cannot be driven from the host** (N3 debt 9). This wave used
   the real camera scan rather than reviving the throwaway instrumented helper,
   so no such helper exists in the tree. It cost two operator interactions,
   because the first pairing half-completed: the desktop stored it and the tablet
   timed out waiting for a confirmation I was too slow to give. An auto-confirm
   watcher on the pairing stream fixed that and is worth keeping in mind for the
   next wave.

**No P0 and no BLOCKER.** Nothing in this list is a known fault in shipped N4
code; items 1 and 2 are scope deferrals, 3–4 and 6 are environment, and 5, 7, 8
are pre-existing.

---

## 34. Git status

Nothing was added, committed, pushed or opened as a PR.

```console
$ git status --short
 M .github/workflows/portable-windows-msvc.yml      ← PRE-EXISTING, not N4
 M android/app/src/androidTest/.../NotificationConsentUiTest.kt
 M android/app/src/androidTest/.../NotificationHardwareGateTest.kt
 M android/app/src/androidTest/.../NotificationLoggingCanaryTest.kt
 M android/app/src/androidTest/.../NotificationUiFixtures.kt
 M android/app/src/main/.../capability/NotificationsCapability.kt
 M android/app/src/main/.../notifications/AnyFlowNotificationListener.kt
 M android/app/src/main/.../notifications/NotificationIdentity.kt
 M android/app/src/main/.../notifications/NotificationPolicy.kt
 M android/app/src/main/.../notifications/NotificationQueue.kt
 M android/app/src/main/.../notifications/NotificationReadiness.kt
 M android/app/src/main/.../notifications/NotificationRoleState.kt
 M android/app/src/main/.../notifications/NotificationSnapshot.kt
 M android/app/src/main/.../notifications/NotificationSource.kt
 M android/app/src/main/.../notifications/NotificationText.kt
 M android/app/src/main/.../ui/MainState.kt
 M android/app/src/main/.../ui/NotificationSettingsScreen.kt
 M android/app/src/main/.../ui/NotificationUiMapping.kt
 M android/app/src/main/res/values/strings.xml
 M android/app/src/test/.../NotificationPolicyStorageTest.kt
 M android/app/src/test/.../NotificationReadinessTest.kt
 M android/app/src/test/.../NotificationRolesTest.kt
 M android/app/src/test/.../NotificationSourceTest.kt
 M desktop/capabilities/notifications/src/backend/dbus.rs
 M desktop/capabilities/notifications/src/backend/mod.rs
 M desktop/capabilities/notifications/src/lib.rs
 M desktop/capabilities/notifications/src/mirror.rs
 M desktop/capabilities/notifications/src/queue.rs
 M desktop/capabilities/notifications/src/roles.rs
 M desktop/capabilities/notifications/tests/common/mod.rs
 M desktop/capabilities/notifications/tests/logging.rs
 M desktop/capabilities/notifications/tests/real_dbus.rs
 M desktop/capabilities/notifications/tests/sink.rs
 M desktop/cli/src/main.rs
 M desktop/control/src/lib.rs
 M desktop/core/src/notification_policy.rs
 M desktop/daemon/tests/common/mod.rs
 M desktop/daemon/tests/notifications.rs
 M desktop/gui/src/views/notifications.rs
 M desktop/runtime/src/server.rs
?? NOTIFICATIONS-V1-N4-REPORT.md                    ← this file
?? android/app/src/main/.../notifications/NotificationDismiss.kt
?? android/app/src/main/.../notifications/NotificationEcho.kt
?? android/app/src/test/.../NotificationDismissRulesTest.kt
?? desktop/capabilities/notifications/tests/dismiss.rs

$ git diff --check
  (clean)

$ git diff --stat -- android/ desktop/ | tail -1
  39 files changed, 3966 insertions(+), 294 deletions(-)

$ git log --oneline -1
  8d8948a Merge pull request #20 …          ← HEAD unmoved; nothing staged
```

Artefact audit — `*.apk *.aab *.key *.pem *.p12 *.pfx *.jks *.keystore *.log`,
QR images, UI dumps, `state.json`, trust stores, `identity.key`, `target/`,
`build/`: **none present.** Every working file used for the hardware gates lives
in the session scratchpad, outside the repository.

---

## 35. Acceptance

| Criterion | Result |
| --- | --- |
| human Linux dismissal produces exactly one `DismissRequest` | **PASS** §25 |
| only human dismissal does so | **PASS** §11, §13, §25 |
| Android validates authenticated / granted / authorized peer | **PASS** §7 |
| Android policy required | **PASS** §25 rows 2, 4 |
| desktop policy required (canonical two-policy design) | **PASS** §6, §25 rows 3, 4 |
| remote `DISMISS_TARGET` role required | **PASS** §5 |
| clearable source notification dismissed exactly once | **PASS** §25 |
| non-dismissible source refuses | **PASS** §10 (JVM; hardware fixture not creatable — §25) |
| unknown / already-gone converges safely | **PASS** §13 |
| loop suppression proven | **PASS** §12 |
| duplicate / race behaviour proven | **PASS** §13 |
| no arbitrary Android notification can be targeted | **PASS** §9, §29 |
| raw Android key never leaves Android | **PASS** §7, §19, §20 |
| no content / history persistence | **PASS** §20 |
| no content / raw-key logging | **PASS** §19 |
| real GNOME human-dismiss hardware gate | **PASS** §24, §25 |
| real SM-X620 source dismissal hardware gate | **PASS** §25 |
| policy matrix | **PASS** §25 |
| N3 regression | **PASS** §26 |
| Android JVM green | **PASS** 510 / 0 / 0 / 0 |
| `connectedDebugAndroidTest` green, skipped=0 | **PASS** 96 / 0 / 0 / 0 |
| Rust green | **PASS** 644 passed, clippy clean, fmt clean |
| real D-Bus green | **PASS** 9 / 0 |
| accessibility stable | **PASS** §31 |
| `notifications_v1.proto` unchanged | **PASS** §28 |
| no actions / replies / remote-intent execution | **PASS** §29 |
| no BLOCKER / P0 | **PASS** §33 |

---

NOTIFICATIONS.V1 N4 PASS
N5 READY FOR IMPLEMENTATION
