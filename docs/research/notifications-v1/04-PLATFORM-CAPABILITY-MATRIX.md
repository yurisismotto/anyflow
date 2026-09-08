# 04 — `notifications.v1` platform capability matrix

| Field | Value |
| --- | --- |
| **Title** | What each platform can actually do with notifications |
| **Status** | Research — complete |
| **Last reviewed** | 2026-09-08 |
| **Evidence** | Per cell. Nothing in this table is an assumption promoted to a fact |
| **Related** | [00](00-RESEARCH-FINDINGS.md), [02](02-PROTOCOL-AND-EVENT-MODEL.md), [platform-expansion 03](../platform-expansion/03-PLATFORM-CAPABILITY-MATRIX.md) |

---

## Legend

The brief asks for four values. They map onto the vocabulary
[platform-expansion 03](../platform-expansion/03-PLATFORM-CAPABILITY-MATRIX.md)
already uses, so the two matrices can be read together:

| Value | Meaning | Here |
| --- | --- | --- |
| **VERIFIED** | Executed here, or read from platform source / a normative spec | Transcript or file cited in [00](00-RESEARCH-FINDINGS.md) |
| **LIKELY** | A documented vendor API exists and fits; no known obstacle; not proven for AnyFlow | Vendor doc cited |
| **UNKNOWN** | Cannot be settled from documentation. Needs a PoC | PoC id cited |
| **UNSUPPORTED** | The platform does not expose it. No amount of engineering changes this | Reason cited |

---

## 1. The matrix

| | Android | Linux GNOME | Linux KDE | Windows | macOS | iOS / iPadOS |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| **Observe other apps' notifications** (source) | **VERIFIED**¹ | **UNSUPPORTED**² | **UNSUPPORTED**² | **LIKELY**³ | **UNSUPPORTED**⁴ | **UNSUPPORTED**⁵ |
| **Display a remote notification** (sink) | LIKELY⁶ | **VERIFIED**⁷ | LIKELY⁸ | LIKELY⁹ | LIKELY¹⁰ | **LIKELY, but useless in v1**¹¹ |
| **Update in place** (no duplicate entry) | LIKELY⁶ | **VERIFIED**¹² | LIKELY⁸ | LIKELY⁹ | LIKELY¹³ | LIKELY¹³ |
| **Close a mirror we posted** | LIKELY⁶ | **VERIFIED**¹⁴ | LIKELY⁸ | LIKELY⁹ | LIKELY¹⁵ | LIKELY¹⁵ |
| **Detect local dismissal by the user** | LIKELY⁶ | **VERIFIED**¹⁶ | LIKELY⁸ | UNKNOWN¹⁷ | **LIKELY**¹⁸ | LIKELY¹⁸ |
| **Cancel a notification at its source** | **VERIFIED**¹⁹ | **UNSUPPORTED**² | **UNSUPPORTED**² | LIKELY²⁰ | **UNSUPPORTED**⁴ | **UNSUPPORTED**⁵ |
| **Actions / reply feasibility** (post-v1) | LIKELY²¹ | **VERIFIED**²² | LIKELY⁸ | LIKELY | LIKELY | LIKELY |
| **Background feasibility** (hold the session) | **VERIFIED**²³ | **VERIFIED**²³ | **VERIFIED**²³ | LIKELY²³ | LIKELY²³ | **UNSUPPORTED**²⁴ |
| **Permission model** | OS Settings toggle¹ | none²⁵ | none²⁵ | restricted capability + MSIX³ | user authorization¹⁰ | user authorization |
| **v1 role** | `SOURCE`, `DISMISS_TARGET` | `SINK`, `DISMISS_REPORTER` | — | — | — | — |

---

## 2. Notes

**¹ Android source — VERIFIED.** `NotificationListenerService` with
`BIND_NOTIFICATION_LISTENER_SERVICE`, granted by the user in Settings
(`ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS` since API 30). Read from AOSP
source; the certification target (SM-X620, Android 16, One UI 8.0) has the
listener mechanism in active use — though both enabled listeners there are
Samsung's own, so third-party behaviour on One UI is a PoC, not a transcript
([00 §1.2, §1.9, §1.10](00-RESEARCH-FINDINGS.md)). Caveats that are also
verified: the system **ignores listeners running in a work profile**; a work
profile's administrator can block cross-profile delivery; callbacks arrive on
the **main thread**; and Android 15+ *can* redact OTP-classified notifications from
untrusted listeners ([00 §1.7](00-RESEARCH-FINDINGS.md)).

**² Linux source — UNSUPPORTED.** Only one process may own
`org.freedesktop.Notifications`. Observing other applications' notifications
means becoming the desktop's notification server, or monitoring the session bus
for their method calls — which needs bus policy amounting to "read every
notification on this machine before the shell does". Structurally the same
finding as [ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md) on
clipboard watching, and it lands the same way
([00 §2.7](00-RESEARCH-FINDINGS.md), [01 §9](01-FUNCTIONAL-SPECIFICATION.md)).

**³ Windows source — LIKELY, and gated on packaging.**
`Windows.UI.Notifications.Management.UserNotificationListener` (Windows 10
1607+): `RequestAccessAsync()` from a UI thread, `GetNotificationsAsync`,
`NotificationChanged` ("Occurs when a notification is added or removed"). It
needs the **`userNotificationListener` restricted capability**, which needs
**package identity** — an unpackaged desktop app cannot declare it
([00 §3](00-RESEARCH-FINDINGS.md)). So Windows-as-source is real but arrives
with an MSIX requirement attached.

**⁴ macOS source — UNSUPPORTED.** The `UserNotifications` framework manages an
app's *own* notifications. Apple exposes no public API for reading another
app's. Marked from the absence of an API rather than from a statement that it is
forbidden ([00 §4](00-RESEARCH-FINDINGS.md)).

**⁵ iOS source — UNSUPPORTED.** Same absence, more firmly: there is no iOS
analogue of `NotificationListenerService`, and
`UNNotificationServiceExtension` only mutates the app's *own* incoming remote
notifications ([00 §5](00-RESEARCH-FINDINGS.md)).

**⁶ Android sink — LIKELY, and out of scope for v1.** `NotificationManager.notify`
with a stable `(tag, id)` gives create/replace/cancel and, with a delete
intent, dismissal detection. Not implemented in v1 for product reasons, not
platform ones ([01 §9](01-FUNCTIONAL-SPECIFICATION.md)).

**⁷ GNOME sink — VERIFIED here.** HOST VERIFIED on the certification target:
`GetServerInformation` → `('gnome-shell', 'GNOME', '50.4', '1.2')`;
`GetCapabilities` → `actions, body, body-markup, icon-static, persistence,
sound`. Note the absences: no `body-images`, no `body-hyperlinks`, no
`action-icons` ([00 §2.1](00-RESEARCH-FINDINGS.md)).

**⁸ KDE Plasma — LIKELY, one transcript from VERIFIED.** Plasma serves the same
`org.freedesktop.Notifications` interface from `plasma-workspace`, so it is the
same sink code. Its capability set and its `CloseNotification` error behaviour
were **not measured** — no Plasma machine was available.
[POC-NOTIF-03](06-OPEN-QUESTIONS-AND-POCS.md) is three `gdbus` calls long.

**⁹ Windows sink — LIKELY.** `AppNotificationManager` (Windows App SDK) or
`ToastNotificationManager`; needs an AUMID, free with MSIX and hand-registered
when unpackaged. Tag/group give replace and removal. Already recorded in
[platform-expansion 08 §8](../platform-expansion/08-WINDOWS-FEASIBILITY.md).

**¹⁰ macOS sink — LIKELY.** `UNUserNotificationCenter` after
`requestAuthorization`. `UNMutableNotificationContent` has title, **subtitle**
and body — which is why `sub_text` is *deferred* rather than rejected in
[02 §6.6](02-PROTOCOL-AND-EVENT-MODEL.md).

**¹¹ iOS sink — LIKELY as an API, useless as a product in v1.**
`UNUserNotificationCenter` works, but only while the app is running, and iOS
suspends it shortly after backgrounding and *"may choose to reclaim resources
out from underneath a network socket used by the app"* — OFFICIAL DOC VERIFIED
in [platform-expansion 11](../platform-expansion/11-IOS-IPADOS-FEASIBILITY.md).
No `UIBackgroundModes` value covers holding a LAN control session. A
notification sink that only works while you are looking at the app is not a
notification sink.

**¹² GNOME update in place — VERIFIED here.** The spec: *"If `replaces_id` is not
0, the returned value is the same value as `replaces_id`"*, and the server *"must
atomically (ie with no flicker or other visual cues) replace"*. Confirmed by
running it — three `Notify` calls with `replaces_id=5` returned `5` each time
([00 §2.2](00-RESEARCH-FINDINGS.md)). This is the whole answer to progress
notifications.

**¹³ Apple update in place — LIKELY.** Re-adding a `UNNotificationRequest` with
the same identifier replaces the delivered notification — the same shape as
`replaces_id`.

**¹⁴ GNOME close — VERIFIED here, including a spec deviation.**
`CloseNotification(id)` emits `NotificationClosed(id, 3)`. Closing an
already-closed id **and** an id that never existed both returned success with no
error and no signal, where the spec mandates *"an empty D-BUS Error message is
sent back"*. Idempotent dismissal is free on GNOME — and the sink must still
treat a close error as success, because a spec-literal server will send one
([00 §2.3](00-RESEARCH-FINDINGS.md)).

**¹⁵ Apple removal — LIKELY.** `removeDeliveredNotifications(withIdentifiers:)`.

**¹⁶ GNOME dismissal detection — VERIFIED (mechanism), one PoC from complete.**
`NotificationClosed(id, reason)` with reason **2 = "dismissed by the user"**
(SPEC VERIFIED). Reason 3 (our own close) and reason 1 (expired) must **not**
propagate a dismissal to the phone — a desktop banner timing out is not a human
decision ([02 §9.2](02-PROTOCOL-AND-EVENT-MODEL.md)). Observing an actual
reason-2 signal requires a human to click, so it is
[POC-NOTIF-05](06-OPEN-QUESTIONS-AND-POCS.md) rather than a transcript here.

**¹⁷ Windows dismissal detection — UNKNOWN.** `ToastNotification.Dismissed`
exists in the UWP surface; whether it fires for a desktop app's own toasts on the
App SDK path was not established. Not on the v1 path.

**¹⁸ Apple dismissal detection — LIKELY, and only if asked for.**
`UNNotificationCategoryOptions.customDismissAction` — *"Send dismiss actions to
the `UNUserNotificationCenter` object's delegate for handling."* Without the
option, dismissals are silent. A detail worth carrying forward, because it is
the kind of thing that is discovered late and looks like a platform bug
([00 §4](00-RESEARCH-FINDINGS.md)).

**¹⁹ Android source-side cancel — VERIFIED.**
`NotificationListenerService.cancelNotification(String key)`, with `key` =
`userId|pkg|id|tag|uid` (AOSP VERIFIED). `sbn.isClearable()` — neither
`FLAG_ONGOING_EVENT` nor `FLAG_NO_CLEAR` — is what decides whether a given
notification can be cancelled at all, and it is checked at the source because
the source is the only end that knows
([00 §1.3, §1.5](00-RESEARCH-FINDINGS.md)).

**²⁰ Windows source-side cancel — LIKELY.**
`UserNotificationListener.RemoveNotification(UInt32)`. Same packaging gate as ³.

**²¹ Android actions/reply — LIKELY, and deliberately not in v1.** The
`Notification.Action` array and `RemoteInput` are reachable from a listener, and
using them means executing a `PendingIntent`. That is the line between mirroring
and remote control, and crossing it is a **new capability id** with its own
grant and threat model, not a field addition
([03 §T-N09](03-PRIVACY-SECURITY-THREAT-MODEL.md)).

**²² GNOME actions — VERIFIED as available.** `actions` is in the capability
list, and `ActionInvoked(id, action_key)` is specified. So the *desktop* half of
a future reply feature is already there; the constraint is entirely on the
Android half and on the security decision.

**²³ Background execution — as already established.** Nothing new: the Android
`connectedDevice` foreground service ([ADR-0009](../../adr/ADR-0009-android-background-execution.md))
and the Linux user daemon both already hold the session that
`notifications.v1` rides on. One Android-specific unknown is added by this
feature — whether One UI's "sleeping apps" battery management suspends a bound
`NotificationListenerService` ([POC-NOTIF-04](06-OPEN-QUESTIONS-AND-POCS.md)).

**²⁴ iOS background — UNSUPPORTED.** See ¹¹. This is the hardest constraint in
the whole expansion and it is not this sprint's to solve.

**²⁵ Linux permission model — none, and that is worth saying.** Any process in a
session may post to `org.freedesktop.Notifications`. There is no prompt and no
capability. AnyFlow neither worsens this nor can fix it — the same honest caveat
[CLIPBOARD.md](../../architecture/CLIPBOARD.md) makes about a Linux session
clipboard.

---

## 3. What the matrix forces on the protocol

Three asymmetries, each of which would break a design that assumed
"phone sources, desktop sinks":

1. **macOS and iOS can sink but never source.** So "sink" cannot imply "can also
   tell me about its own notifications".
2. **Windows can do everything, including cancel at the source.** So "source"
   cannot imply "Android", and dismissal cannot be modelled as
   desktop→phone-only.
3. **Roles change at runtime.** Android notification access is revocable in
   Settings while a session is up; a Linux session can lose its notification
   server. So roles cannot be a static property of a platform, or even of a
   build.

This is why [02 §3](02-PROTOCOL-AND-EVENT-MODEL.md) advertises roles per peer,
inside the capability, with an epoch, and treats *absent roles as no roles*.
