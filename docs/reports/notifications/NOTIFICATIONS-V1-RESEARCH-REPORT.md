# Research sprint report — `notifications.v1`

**Branch:** `research/notifications-v1` · **Working tree:** uncommitted, as instructed
**Date:** 2026-09-08

> ## Outcome: **NOTIFICATIONS.V1 SPEC READY FOR REVIEW**
>
> The architecture, protocol, privacy model and implementation plan are
> complete and internally consistent. **One BLOCKER and three P0 questions are
> open, and all four are project decisions rather than engineering ones.** Per
> the brief's own rule, no implementation sprint may start until they are
> answered — which is the correct state for a research sprint to end in, not a
> failure of it.

---

## 1. Research performed

| Method | What |
| --- | --- |
| **Repository audit** | `envelope.proto`, `core.proto`, all three capability protos, `capability.rs`, `store.rs`, `clipboard_policy.rs`, `daemon/src/main.rs`, `Capability.kt`, `TrustStore.kt`, `AndroidManifest.xml`, ADR-0008/0010/0014, `OVERVIEW.md`, `PROTOCOL.md`, `CLIPBOARD.md`, `THREAT_MODEL.md`, platform-expansion 03/06/08/10/11/17/22/25/28 |
| **Executed on this host** | `gdbus` + `dbus-monitor` against `org.freedesktop.Notifications`; `gdbus introspect` on `org.gnome.ScreenSaver`; `gdbus call` on `org.freedesktop.ScreenSaver`; `loginctl show-session` |
| **Executed on the target device** | `adb getprop`, `settings get secure`, `dumpsys notification` on the attached SM-X620 |
| **Platform source** | `NotificationListenerService.java`, `StatusBarNotification.java`, `Notification.java`, `NotificationManager.java`, `Settings.java`, `NotificationManagerService.java`, `ManagedServices.java` — fetched from `aosp-mirror/platform_frameworks_base` `master` and grepped locally |
| **Normative spec** | freedesktop Desktop Notifications Specification §9 (D-BUS Protocol), full text |
| **Vendor docs** | Android 15 behaviour changes; `UserNotificationListener`; Windows capability declarations; Apple `UserNotifications`, `UNNotificationCategoryOptions.customDismissAction` |

## 2. Authoritative platform findings

**Android (AOSP VERIFIED)**

* `key = userId|pkg|id|tag|uid` — stable across updates, unique per app and
  profile, and the argument to `cancelNotification`.
* 23 removal reasons; `REASON_LOCKDOWN` javadoc: *"all listeners shall ensure
  canceled notifications are removed to prevent data leaking."*
* `onListenerConnected()` — *"You are safe to call `getActiveNotifications()`"*.
  Callbacks are on the **main thread**. The system **ignores listeners running
  in a work profile**; a personal-profile listener *does* see work-profile
  notifications unless the DPC blocks it.
* `META_DATA_DISABLED_FILTER_TYPES` javadoc names the use case outright:
  *"a type that the listener will never **bridge to their paired devices**"*.
  `META_DATA_DEFAULT_AUTOBIND=false` allows on-demand binding.
* Android 15+ redacts OTP notifications from untrusted listeners.
  `isAppTrustedNotificationListenerService` treats **a CDM association as
  trust** — so adopting CompanionDeviceManager would turn the protection off.

**Android (DEVICE VERIFIED — SM-X620)**: Android 16 / API 36 / One UI 8.0,
patch 2026-07-05. Google's notification assistant (the OTP classifier) is live.
The redaction aconfig flag was not readable unprivileged → PoC.

**Linux (HOST VERIFIED — GNOME Shell 50.4)**

* `GetServerInformation` → `('gnome-shell','GNOME','50.4','1.2')`;
  `GetCapabilities` → `actions, body, body-markup, icon-static, persistence,
  sound`. **No `body-images`.**
* `replaces_id` honoured exactly — three `Notify(replaces_id=5)` calls all
  returned `5`. This is the whole answer to progress notifications.
* `CloseNotification` is **idempotent here**: closing an already-closed id and a
  never-existed id both returned success with no error and no signal, where the
  spec mandates an error. Three closes, one `NotificationClosed(5, 3)`.
* `org.gnome.ScreenSaver` has `GetActive` + **`ActiveChanged`**; `logind`
  exposes `LockedHint`. **`org.freedesktop.ScreenSaver` on GNOME is idle-inhibit
  and refuses `GetActive`** — a detector built on it would fail open.

**Linux (SPEC VERIFIED)**: `NotificationClosed` reasons 1 expired / 2 user
dismissal / 3 our own close / 4 undefined; and *"The ID … is invalidated before
the signal is sent."*

**Windows / macOS / iOS**: Windows can source (`UserNotificationListener`, but
needs the restricted `userNotificationListener` capability and hence MSIX
packaging), sink and cancel-at-source. macOS can sink and detect dismissal
(only with `customDismissAction`) but has no API to read other apps'
notifications. iOS can do neither usefully.

## 3–6. Architecture, roles, events, identity

One capability id `notifications.v1`, on the existing control session, no second
socket, no envelope or `core.proto` change.

**Roles** are announced *inside* the capability with an epoch, not in `HELLO`,
because Android notification access is revocable mid-session and roles must
narrow immediately: `SOURCE`, `SINK`, `DISMISS_TARGET`, `DISMISS_REPORTER`.
**Absent roles mean no roles** — fail closed. v1: Android = source +
dismiss-target, Linux = sink + dismiss-reporter.

**Events** — six messages. `Posted` and `Updated` collapse into one idempotent
`NotificationUpsert`, because no platform distinguishes them: Android fires
`onNotificationPosted` for both, `Notify` creates or replaces, and macOS
replaces by identifier. Plus `NotificationRemove`, `DismissRequest`,
`NotificationResult`, `NotificationRoles`, `SyncMarker`.

**Identity** — `notification_id = HMAC-SHA256(device_secret, "…/id/v1" ||
len32(key) || key)[0..16]`. Derived rather than random so it survives an Android
process restart (the map is rebuilt from `getActiveNotifications()`), which is
the difference between a reconnect being invisible and a reconnect duplicating
the whole shade. The platform `key`, its uid and its profile id never reach the
wire. The sink keys mirrors on `(peer_fingerprint, notification_id)` — the
**pinned TLS identity**, never the claimed `origin_device_id`.

## 7–13. Privacy, filters, lock, dismissal, loops, resync, rate limits

* **Privacy model**: grant (never in `auto_grant`) ≠ policy, following
  `clipboard.v1`. Nothing logged, nothing persisted, no history anywhere.
  `VISIBILITY_SECRET` never transmitted. **AnyFlow does not attempt to detect
  OTPs** — no regex, no heuristic; the same reasoning `THREAT_MODEL.md` T10
  already applies to clipboard passwords.
* **App filter**: **deny-all by default**, presented as an onboarding picker
  with select-all. Newly installed apps default deny with a passive "N new apps"
  affordance. Work-profile and ongoing notifications are separate, default-off
  switches. AnyFlow's own package is never mirrored — a hard rule, not a setting.
* **Lock policy**: `Full` / `AppOnly` / `Suppress` on both ends, default
  `AppOnly`. The source reduces **before encoding**, so withheld content never
  exists on the wire. Unlock is not retroactive. Unknown lock state is treated
  as locked.
* **Dismissal**: `NotificationClosed` **reason 2 only** → `DismissRequest` →
  `cancelNotification(key)`. Reason 1 (expired) must never dismiss the phone's
  notification — that single rule is what stops the feature clearing a user's
  phone whenever a desktop banner times out. `DismissRequest` has no field
  capable of holding an action or a `PendingIntent`.
* **Loops**: own package dropped first; no relay (enforced by absence, as in
  clipboard); `origin_device_id` on every message; sink mirrors marked with
  AnyFlow's `desktop-entry`; single-use, 10 s echo suppression on
  `REASON_LISTENER_CANCEL`.
* **Resync**: mirrors retained `RECONNECT_GRACE = 60 s`, then closed. Reconnect
  sends `SyncMarker{BEGIN}` + upserts for currently-active notifications +
  `SyncMarker{END}`; the sink removes anything not named. Active-state snapshot
  ≠ history, and the distinction is tabulated rather than asserted.
* **Rate limits**: per-identity pending *slot* (not queue), 500 ms minimum
  update interval, 20 new/10 s, 2 msg/s sustained, 200 mirrors/peer, 100-entry
  snapshot, 8 KiB message. **Terminal removals never coalesce away and are never
  dropped.**

## 14–15. Failure and limits

Fail closed throughout: no grant → `NOT_AUTHORIZED`; no D-Bus server → `SINK`
never advertised; unknown enum → most conservative value (unknown privacy is
treated as `SECRET`); bad-length id → refused and **not answered**; oversized →
`TOO_LARGE`, never truncated by the receiver. Nothing is fatal to the session.

Truncation differs deliberately from `clipboard.v1`: the **source** truncates
display text on a UTF-8 boundary with `U+2026`, because a shortened chat message
is still the message — whereas a shortened password is a different, plausible,
wrong value. Identifiers are never truncated.

## 16–21. i18n, UX, compatibility, threats, matrix, MVP

* **i18n**: notification content is opaque user text and is never translated,
  normalised or re-encoded; `app_label` is resolved in the *source's* locale.
  All AnyFlow strings externalised.
* **UX**: two permissions are structurally distinct — OS notification access
  grants **nothing** to any peer. No history screen, and none promised.
* **Compatibility**: additive. Old peers never negotiate it; an unknown
  capability id is already a non-fatal error. No protocol version bump.
* **Threat model**: T-N01…T-N16, each with impact, mitigation and honest
  residual risk, plus 27 named security test cases (NOTIF-SEC-01…27).
* **Matrix**: 6 platforms × 9 capabilities, every cell marked VERIFIED /
  LIKELY / UNKNOWN / UNSUPPORTED with its evidence.
* **MVP boundary**: confirmed as proposed. Windows' ability to fill every role
  argued for a symmetric *protocol* (adopted) but not for a wider v1 (rejected).

## 22. Implementation waves

N0 protocol + ADR-0015/0016/0017 · N1 Android source · N2 Linux sink (+ the
`NotificationSink` seam Wave 0 declined) · N3 grants/filters/privacy UI ·
N4 dismissal · N5 reconnect/rate-limit hardening · N6 hardware certification.
N1 and N2 parallelise after N0. Each wave has scope, files, tests, hardware
gates, security gates and exit criteria.

## 23. Documents

Created — `docs/research/notifications-v1/`: `README.md`,
`00-RESEARCH-FINDINGS.md`, `01-FUNCTIONAL-SPECIFICATION.md`,
`02-PROTOCOL-AND-EVENT-MODEL.md`, `03-PRIVACY-SECURITY-THREAT-MODEL.md`,
`04-PLATFORM-CAPABILITY-MATRIX.md`, `05-IMPLEMENTATION-PLAN.md`,
`06-OPEN-QUESTIONS-AND-POCS.md` (3,626 lines; 257 relative links, all resolving).
Changed — `README.md`, one line adding `docs/research/` to the tree.

## 24. Blockers

| id | Class | Question |
| --- | --- | --- |
| **OQ-01** | **BLOCKER** | Is AnyFlow willing to hold `BIND_NOTIFICATION_LISTENER_SERVICE`? The manifest, `THREAT_MODEL.md` T26 and `README.md` all currently say it will not |
| **OQ-04** | **P0** | Will AnyFlow ever adopt `CompanionDeviceManager`? Doing so silently disables Android's OTP redaction |
| **OQ-02** | **P0** | Confirm deny-all app filter default |
| **OQ-03** | **P0** | Confirm `allow_dismiss_sync` defaults on |

Six PoCs are specified with procedure, pass and fail criteria: POC-NOTIF-01
(OTP redaction on Android 16, **P0**), -02 (lock detection, **P0**), -03 (KDE
parity), -04 (One UI app-sleep vs. a bound listener), -05 (GNOME reason-2 on a
real dismissal), -06 (does update-in-place re-alert).

## Recommendation for the next prompt

**Do not open an implementation sprint.** Open a short decision sprint that
(a) answers OQ-01 and OQ-04 as ADR-0015, (b) signs off OQ-02 and OQ-03, and
(c) runs POC-NOTIF-01 and POC-NOTIF-02 — both are under an hour on hardware that
is attached today. If OQ-01 is answered *no*, the correct outcome is to close
this branch and record why; the research stands either way.
