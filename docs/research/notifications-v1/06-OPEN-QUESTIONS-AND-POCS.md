# 06 — `notifications.v1` open questions and proofs of concept

| Field | Value |
| --- | --- |
| **Title** | What is still unresolved, and how to resolve it |
| **Status** | Live list — maintained through implementation |
| **Last reviewed** | 2026-09-08 (decision sprint) |
| **Decisions** | OQ-01 … OQ-04 are **resolved** by [ADR-0015](../../adr/ADR-0015-notification-access.md), which is the canonical record. This file records the outcome and where it lives, not the reasoning |
| **Rule** | **No implementation sprint starts while a P0 architecture or security question is open.** BLOCKER outranks P0: it stops the feature, not just the sprint |
| **Related** | [00](00-RESEARCH-FINDINGS.md), [03](03-PRIVACY-SECURITY-THREAT-MODEL.md), [05](05-IMPLEMENTATION-PLAN.md) |

---

## 1. Classification

| Class | Meaning |
| --- | --- |
| **BLOCKER** | The feature cannot exist until this is answered. Not an engineering question |
| **P0** | Architecture or security. Blocks the start of implementation |
| **P1** | Affects a wave's design. Must be answered before that wave |
| **P2** | Affects polish or a later platform. Can be answered during implementation |
| **DEFERRED** | Deliberately out of scope for v1; recorded so it is not rediscovered |

---

## 2. Open questions

### OQ-01 — Is AnyFlow willing to hold `BIND_NOTIFICATION_LISTENER_SERVICE`? · **RESOLVED / APPROVED**

**Decision: yes**, under the explicit security contract in
[ADR-0015 §1](../../adr/ADR-0015-notification-access.md). Optional, disabled by
default, two separate permissions, independently revocable, fails closed, and
carrying none of history, cloud, telemetry, actions, `PendingIntent` or reply.

The published position is **amended, not erased**. Before `notifications.v1`
AnyFlow held no notification-listener privilege, and said so in the manifest,
in `THREAT_MODEL.md` T26 and in `README.md`. That was correct: there was no
feature that needed one. What is withdrawn is the phrase *"and it must stay
that way"* — a permanent conclusion where the project only ever had a permanent
rule. The rule that replaces it is
[ADR-0015 §2](../../adr/ADR-0015-notification-access.md), and under it
accessibility, `QUERY_ALL_PACKAGES`, `SYSTEM_ALERT_WINDOW`, `READ_LOGS`,
`MANAGE_EXTERNAL_STORAGE`, location, root, default-IME and hidden APIs all
remain refused.

**No longer a BLOCKER.**

### OQ-02 — Should the app-filter default be deny-all? · **RESOLVED**

**Decision: deny by default**, as recommended.
[ADR-0015 §5](../../adr/ADR-0015-notification-access.md).

Nothing third-party is mirrored until the user names it; the enabling flow ends
in a picker with *Select all* one tap away, and *Select all* is always a
deliberate action. AnyFlow's own package never mirrors; newly installed apps,
work-profile notifications and system notifications all default to disabled; no
banking / password-manager / 2FA heuristic is treated as a security boundary.

[POC-NOTIF-01](poc/POC-NOTIF-01.md) strengthens this: with platform OTP
redaction absent on the certification target, the allow-list is the **only**
effective control over whether a one-time code reaches a second screen.

### OQ-03 — Does `allow_dismiss_sync` default on? · **RESOLVED**

**Decision: supported, but default OFF**, opt-in per peer.
[ADR-0015 §6](../../adr/ADR-0015-notification-access.md).

This **reverses the recommendation** made in
[01 §3.1](01-FUNCTIONAL-SPECIFICATION.md). That argument is sound about utility
and is rejected on category: dismissal is the only thing in `notifications.v1`
that changes state on the *source* device, and crossing from "this computer can
see my phone" to "this computer can act on my phone" is a separate consent even
though the action is small.

Normative requirements: only a desktop user dismissal (`NotificationClosed`
reason **2**) may cancel at the source; expiry (reason 1) never may;
non-clearable and ongoing notifications are never force-cancelled; duplicate
dismiss is idempotent; a dismiss for an offline peer is dropped, not queued; no
action execution, no `PendingIntent`, no reply.

### OQ-04 — Will AnyFlow ever adopt `CompanionDeviceManager`? · **RESOLVED FOR V1 — CDM DEFERRED**

**Decision: `notifications.v1` does not use CDM.**
[ADR-0015 §10](../../adr/ADR-0015-notification-access.md).

A live CDM association makes AnyFlow a *trusted* listener, which switches off
Android 15+ sensitive-content redaction. AnyFlow already has explicit pairing,
SPKI-pinned TLS, peer identity, per-capability grants and revocation, so CDM
would buy platform convenience on top of a complete trust model at the cost of
a privacy property.

**Not forbidden permanently.** Adopting it later requires a separate security
ADR, and new hardware verification of OTP and sensitive-notification redaction
*under a live association* — [POC-NOTIF-01](poc/POC-NOTIF-01.md) re-run, not
reasoned about.

One correction from that PoC: on the certification device the trusted-listener
set is non-empty while the CDM association table is **empty**, so CDM is *a*
route to listener trust and not the only one. The forward claim (a CDM
association confers trust) stands; the inverse must not be made.

### OQ-05 — What happens to mirrors when the peer disconnects? · **P1** — *answered*

Answered in [02 §7.1](02-PROTOCOL-AND-EVENT-MODEL.md): retain for
`RECONNECT_GRACE = 60 s`, then close. Recorded here because the two obvious
alternatives are both wrong in ways that are not obvious, and someone will
propose one of them during N5.

### OQ-06 — Is 60 s the right grace? · **RESOLVED — reclassified, not measured**

The value is still chosen rather than measured, and that is now explicitly
fine, because `RECONNECT_GRACE` is **not protocol semantics**. It, `SYNC_TIMEOUT`
and `MAX_SNAPSHOT_ENTRIES` are **sink-local implementation constants**: never
negotiated, never on the wire, and unable to break interoperability when
changed ([02 §7.5](02-PROTOCOL-AND-EVENT-MODEL.md)).

What *is* semantics is only the bound: the grace must be **greater than zero**
(or a three-second Wi-Fi blip clears and re-posts the desktop) and **finite**
(or a departed phone's notifications stay on a screen that can no longer update
them). 60 s sits inside that bound. Tune it during N5 with real roaming; no
compatibility statement depends on the number.

### OQ-07 — Should `subtitle` be carried after all? · **P2**

Deferred in [02 §6.6](02-PROTOCOL-AND-EVENT-MODEL.md) because freedesktop has no
slot for it, while macOS and Windows do. Adding a proto field later is backward
compatible, so deferring costs nothing — but revisit when a second sink exists,
not before.

### OQ-08 — Per-device or per-peer notification-id secret? · **P2**

[02 §5.2](02-PROTOCOL-AND-EVENT-MODEL.md) chooses per-device: one map, simpler,
and the unlinkability a per-peer secret would add is worth little when each peer
already sees the content. Recorded because the alternative is cheap to switch to
before N1 ships and expensive after.

### OQ-09 — Google Play policy for notification access · **P2**

Notification access is a restricted area of Play policy. AnyFlow's posture is
about as defensible as the feature gets, and the app is distributed from GitHub
today. Marked **LIKELY**, not verified — the current policy text was not read
from the Play Console ([00 §1.11](00-RESEARCH-FINDINGS.md)). Answer before any
Play submission, not before N1.

### OQ-10 — Group summaries: mirror them, or mirror the members? · **P1**

Android posts a group summary plus its members. Mirroring both duplicates
content on the desktop; mirroring only members loses the "5 new messages"
rollup; mirroring only summaries loses the content.

**Provisional:** mirror **members only**, drop `FLAG_GROUP_SUMMARY`
notifications, and carry `group_id` so the sink can group them itself. Needs a
decision before N1 because it changes the filter. Not P0: it affects quality,
not privacy or architecture.

**Evidence added 2026-09-08.** [POC-NOTIF-01 §8](poc/POC-NOTIF-01.md) observed
One UI 8 posting a *synthetic* aggregate group summary — key
`…|g:Aggregate_NormalNotificationSection|…`, with null title, text and big
text — which reached the listener. A filter that mirrors everything would
mirror an empty notification. This reinforces the provisional answer rather
than changing it.

### OQ-11 — Should `anyflow notifications status` show mirror counts per app? · **P2**

Counts are not content, but a per-app count is a description of what the user is
receiving right now and it lands in terminal scrollback and pasted bug reports.
**Provisional: total count only** ([01 §8.2](01-FUNCTIONAL-SPECIFICATION.md)).

### OQ-12 — Media/transport notifications · **DEFERRED**

`CATEGORY_TRANSPORT` with a media session is a different product (remote media
control) with its own capability and its own threat model. Mirroring the
*notification* is in scope via `include_ongoing`; controlling playback is not.

### OQ-13 — Bundled/adaptive notifications on Android 16 · **P2**

Android 16 groups notifications adaptively, and the assistant may re-bundle
them. Whether this changes the `key` or produces synthetic summaries that reach
a listener was not established. Affects OQ-10's answer. Check during N1 on the
real device.

**Partially answered 2026-09-08.** [POC-NOTIF-01 §8](poc/POC-NOTIF-01.md): the
assistant does re-bundle, it produces a synthetic summary with a distinct
`groupKey`, and that summary **is delivered to an ordinary listener**. What was
not established is whether re-bundling ever changes the `key` of an existing
notification — which is the half that would matter for identity. Still P2, still
for N1, but now with a starting point.

### OQ-14 — Which D-Bus source is authoritative for desktop lock state? · **RESOLVED**

**`org.freedesktop.login1.Session.LockedHint` is authoritative.**
`org.gnome.ScreenSaver.ActiveChanged` may be subscribed only as a hint to
re-read `LockedHint`; its boolean is discarded. Unknown state is treated as
locked. [ADR-0015 §7](../../adr/ADR-0015-notification-access.md),
[POC-NOTIF-02](poc/POC-NOTIF-02.md).

Measured, not assumed. `ActiveChanged` fails in **both** directions: it lagged a
real lock by 665 ms (a fail-*open* window in which a locked screen would still
be shown full notification bodies), and it fired for a blank-without-lock (a
fail-closed false positive). `LockedHint` was correct and faster on every path.

Recorded as its own question because the research treated the two sources as
interchangeable, and they are not.

### OQ-15 — What happens to `notification_id` after a device identity reset? · **RESOLVED**

**The `device_notification_secret` is destroyed and regenerated whenever device
identity is reset**, and a regenerated secret is treated as a mirror reset
rather than something to reconcile ([02 §5.5](02-PROTOCOL-AND-EVENT-MODEL.md)).

Recorded because the research specified the secret's derivation and storage but
not its lifecycle, and the omission had a privacy consequence: a secret that
outlived an identity reset would let a peer that saw ids before and after
correlate the two identities, defeating the point of re-pairing.

---

## 3. Proofs of concept

Each is defined the way the brief requires: question, platform, exact procedure,
pass, fail, output. All are small; none needs code that survives.

### POC-NOTIF-01 — Does Android 16 / One UI 8 redact OTP notifications from an untrusted listener? · **P0 — EXECUTED 2026-09-08**

> **Result: CONCLUSIVE — redaction ABSENT.** Six OTP-shaped notifications across
> three vectors reached an untrusted listener **verbatim**, on a device where
> the redaction flag is enabled and Google's classifier is bound. This is the
> criterion's *Fail* branch, whose own text reads: *"the protection is absent on
> this device and the design's assumption (that it must be assumed absent) was
> correct and load-bearing."* No architecture change; documentation changes to
> [01 §8.1](01-FUNCTIONAL-SPECIFICATION.md) and
> [03 §T-N02](03-PRIVACY-SECURITY-THREAT-MODEL.md).
> **Full evidence: [poc/POC-NOTIF-01.md](poc/POC-NOTIF-01.md).**

**Question.** Does `redactSensitiveNotificationsFromUntrustedListeners` actually
fire on the certification target, for an app with no CDM association? The answer
changes the product copy in [01 §8.1](01-FUNCTIONAL-SPECIFICATION.md) and the
residual risk in [03 §T-N02](03-PRIVACY-SECURITY-THREAT-MODEL.md).

**Platform.** SM-X620, Android 16, One UI 8.0.

**Procedure.**
1. Build a throwaway APK with a `NotificationListenerService` that logs
   `sbn.getPackageName()`, the title and the text length — **never** the text.
2. Grant it notification access. Confirm it holds no CDM association.
3. From a second throwaway app, post notifications whose text is OTP-shaped in
   several forms: `"Your verification code is 483920"`,
   `"483920 is your one-time passcode"`, `"G-483920"`, plus a control
   notification with no code.
4. Record what the listener receives for each.

**Pass.** OTP-shaped notifications arrive with the title replaced by the app
label and the text replaced by the platform's redaction string, while the
control notification arrives intact.

**Fail.** OTP-shaped notifications arrive with content intact — meaning the
protection is absent on this device and the design's assumption (that it must be
assumed absent) was correct and load-bearing.

**Output.** A table of input → received, in
`docs/research/notifications-v1/poc/POC-NOTIF-01.md`. **No OTP-shaped string
from a real service is used.**

### POC-NOTIF-02 — Does lock detection actually report a locked GNOME session? · **P0 — EXECUTED 2026-09-08**

> **Result: PASS**, with one mandatory correction. `logind LockedHint` reported
> a real lock in 1.06 ms on `loginctl lock-session` and 218 ms on
> `org.gnome.ScreenSaver.Lock()`, reported both unlocks, and correctly stayed
> `no` for a blank without a lock. `org.gnome.ScreenSaver.ActiveChanged` lagged
> the lock by 665 ms and fired for the blank, so it is demoted to a hint —
> [OQ-14](#oq-14--which-d-bus-source-is-authoritative-for-desktop-lock-state--resolved).
> The `Full` lock policy is **available**, not degraded.
> **Full evidence: [poc/POC-NOTIF-02.md](poc/POC-NOTIF-02.md).**

**Question.** `logind LockedHint` and `org.gnome.ScreenSaver.ActiveChanged` both
*exist* here (HOST VERIFIED, [00 §2.5](00-RESEARCH-FINDINGS.md)) — but do they
report `true` for a real lock, promptly, in a Wayland GNOME session? A lock
detector that fails open silently defeats
[01 §6.2](01-FUNCTIONAL-SPECIFICATION.md).

**Platform.** Fedora 44, GNOME Shell 50.4, Wayland.

**Procedure.**
1. `dbus-monitor` on `org.gnome.ScreenSaver` and on the logind session object.
2. Lock the session (`loginctl lock-session`, and separately the keyboard
   shortcut — they are not always the same path).
3. Record `ActiveChanged` and `LockedHint` transitions and their latency.
4. Unlock; repeat.
5. Also record what happens when the screen merely blanks without locking.

**Pass.** Both sources report locked within 1 s of the lock, both report
unlocked on unlock, and blanking-without-locking does **not** report locked.

**Fail.** Either source is silent or wrong — in which case the sink falls back to
"treat unknown as locked" and the `Full` lock policy must be marked unavailable
rather than silently ineffective.

**Output.** `poc/POC-NOTIF-02.md` with the `dbus-monitor` transcript.

### POC-NOTIF-03 — KDE Plasma sink parity · **P1**

**Question.** Does Plasma's notification server honour `replaces_id`, emit
`NotificationClosed` with reason 2, and tolerate closing an unknown id — or does
it return the D-Bus error the spec mandates
([00 §2.3](00-RESEARCH-FINDINGS.md))?

**Platform.** Any Plasma 6 session. None available here.

**Procedure.** The exact `gdbus` sequence already run on GNOME
([00 §2.1–2.3](00-RESEARCH-FINDINGS.md)): `GetServerInformation`,
`GetCapabilities`, `Notify` ×3 with `replaces_id`, `CloseNotification` ×3
including an unknown id, with `dbus-monitor` capturing signals.

**Pass.** `replaces_id` preserved; reason 3 emitted on our close; an unknown-id
close either succeeds or returns an error that the sink treats as success.

**Fail.** `replaces_id` not honoured — which would mean the update-in-place
design needs a per-server fallback, and progress notifications would duplicate on
Plasma.

**Output.** `poc/POC-NOTIF-03.md`, one transcript.

### POC-NOTIF-04 — Does One UI suspend a bound notification listener? · **P1**

**Question.** Samsung's "sleeping apps" / "deep sleeping apps" management
restricts background components. Does it unbind or starve a
`NotificationListenerService` overnight? Unanswerable from documentation
([00 §1.9](00-RESEARCH-FINDINGS.md)).

**Platform.** SM-X620, One UI 8.0.

**Procedure.**
1. Install the N1 build; grant access; connect a peer so the listener binds.
2. Leave the device idle, screen off, unplugged, overnight (≥ 8 h), with the app
   **not** added to "never sleeping apps".
3. Post notifications at intervals via `adb shell cmd notification post`.
4. In the morning, check `dumpsys notification` for the binding and compare
   posted against received.
5. Repeat with the app marked as never-sleeping, to isolate the cause.

**Pass.** The listener stays bound and every notification is received.

**Fail.** The binding is lost or events are dropped — in which case
`requestRebind` recovery is required, and the Android UI must tell the user to
exempt AnyFlow from battery optimisation, the way `ADR-0009` already handles the
connection service.

**Output.** `poc/POC-NOTIF-04.md` with `dumpsys` before and after.

### POC-NOTIF-05 — Does GNOME emit `NotificationClosed` reason 2 on a real dismissal? · **P1**

**Question.** Reason 2 is the *only* trigger for a dismissal request
([02 §9.2](02-PROTOCOL-AND-EVENT-MODEL.md)). The spec says it means "dismissed
by the user"; GNOME's actual behaviour for a swipe, for the ✕ button, and for
"Clear all" was not observed — it needs a human to click, so it could not be part
of the non-interactive probe.

**Platform.** Fedora 44, GNOME Shell 50.4.

**Procedure.** `dbus-monitor` the interface; post four notifications; dismiss one
by swipe, one by its close button, one via "Clear all", and let one expire.
Record `(id, reason)` for each.

**Pass.** The three human dismissals emit reason 2; the expiry emits reason 1.

**Fail.** Any human dismissal emits reason 1 or 4 — which would make the
reason-2-only rule miss real dismissals, and the design would need a different
discriminator.

**Output.** `poc/POC-NOTIF-05.md`, one transcript.

### POC-NOTIF-06 — Does an update-in-place re-alert? · **P2**

**Question.** When a mirror is replaced via `replaces_id`, does GNOME re-play the
sound and re-show the banner? A progress notification updating twice a second
that re-alerts each time is unusable, and it is not covered by the spec.

**Platform.** Fedora 44, GNOME Shell 50.4.

**Procedure.** Post with `replaces_id=0`, then 10 updates at 500 ms with
`replaces_id` set, with sound enabled. Observe banners and sounds.

**Pass.** One banner, one sound, silent in-place updates thereafter.

**Fail.** Re-alerting — in which case the sink must additionally suppress by
setting `suppress-sound` on updates, and the coalescing interval matters more.

**Output.** `poc/POC-NOTIF-06.md`.

---

## 4. Status

| id | Class | Blocks | State |
| --- | --- | --- | --- |
| OQ-01 | ~~BLOCKER~~ | — | **RESOLVED / APPROVED** — [ADR-0015](../../adr/ADR-0015-notification-access.md) |
| OQ-02 | ~~P0~~ | — | **RESOLVED** — deny by default |
| OQ-03 | ~~P0~~ | — | **RESOLVED** — dismiss sync default **off** |
| OQ-04 | ~~P0~~ | — | **RESOLVED FOR V1** — CDM deferred, not used |
| OQ-05 | P1 | N5 | Answered ([02 §7.1](02-PROTOCOL-AND-EVENT-MODEL.md)) |
| OQ-06 | ~~P2~~ | — | **RESOLVED** — reclassified as a sink-local tunable |
| OQ-07 | P2 | post-v1 | Deferred, deliberately |
| OQ-08 | P2 | N1 | Decided (per-device); revisit before N1 ships |
| OQ-09 | P2 | Play submission | Open. Blocks no release — GitHub distribution |
| OQ-10 | P1 | N1 | Provisional answer, now with device evidence |
| OQ-11 | P2 | N3 | Provisional answer |
| OQ-12 | DEFERRED | — | Out of scope |
| OQ-13 | P2 | N1 | Partially answered on device; remainder open |
| OQ-14 | ~~P0~~ | — | **RESOLVED** — `LockedHint` authoritative |
| OQ-15 | ~~P0~~ | — | **RESOLVED** — secret rotates with device identity |
| POC-NOTIF-01 | P0 | product copy | **EXECUTED** — redaction absent ([evidence](poc/POC-NOTIF-01.md)) |
| POC-NOTIF-02 | P0 | N2 | **EXECUTED — PASS** ([evidence](poc/POC-NOTIF-02.md)) |
| POC-NOTIF-03 | P1 | KDE support | Not run — no Plasma hardware available |
| POC-NOTIF-04 | P1 | N5 | Not run — needs an N1 build and an overnight idle |
| POC-NOTIF-05 | P1 | N4 | Not run — needs a human to click |
| POC-NOTIF-06 | P2 | N2 | Not run |

### What remains, and what it blocks

**No BLOCKER remains. No P0 architecture or security question remains open.**

Everything still open is P1 or P2 and is scoped to a wave *after* N0:

| Still open | Class | First wave it blocks |
| --- | --- | --- |
| OQ-10 group summaries, OQ-13 adaptive bundling | P1 / P2 | N1 |
| OQ-08 per-device vs per-peer secret | P2 | N1 (before it ships) |
| POC-NOTIF-06 update re-alerting | P2 | N2 |
| POC-NOTIF-05 human dismissal emits reason 2 | P1 | N4 |
| OQ-05 confirmation, OQ-06 tuning, POC-NOTIF-04 app-sleep | P1 / P2 | N5 |
| POC-NOTIF-03 Plasma parity | P1 | KDE support, which is not in v1 |
| OQ-09 Play policy | P2 | A Play submission, which is not planned |

**N0 — protocol schema, ADRs and documentation — is unblocked.** Nothing in its
scope depends on any of the above.
