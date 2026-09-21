# 00 — `notifications.v1` research findings

| Field | Value |
| --- | --- |
| **Title** | What each platform actually does with notifications |
| **Status** | Research — complete |
| **Last reviewed** | 2026-09-08 |
| **Branch** | `research/notifications-v1` |
| **Scope** | Android as notification source; Linux (GNOME/KDE) as sink; Windows, macOS, iOS/iPadOS surveyed only far enough to keep the protocol honest |
| **Evidence** | Per-finding. **HOST VERIFIED** = executed on this Fedora 44 / GNOME 50.4 machine. **DEVICE VERIFIED** = executed on the attached SM-X620. **AOSP VERIFIED** = read from platform source. **SPEC VERIFIED** = read from the normative specification. **LIKELY** / **UNKNOWN** as marked |
| **Related** | [01](01-FUNCTIONAL-SPECIFICATION.md), [02](02-PROTOCOL-AND-EVENT-MODEL.md), [03](03-PRIVACY-SECURITY-THREAT-MODEL.md), [04](04-PLATFORM-CAPABILITY-MATRIX.md), [ADR-0008](../../adr/ADR-0008-capability-architecture.md), [ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md), [CLIPBOARD.md](../../architecture/CLIPBOARD.md) |

---

## 0. The four findings that change the design

Everything else in this document is detail. These four are the ones that would
have produced a wrong specification if they had been assumed instead of checked.

1. **The manifest currently forbids this feature in writing.**
   `android/app/src/main/AndroidManifest.xml` contains a block listing
   permissions that are *"Deliberately absent, and it must stay that way"*, and
   `BIND_NOTIFICATION_LISTENER_SERVICE` is the second entry. The same claim is
   repeated in [THREAT_MODEL.md §T26](../../security/THREAT_MODEL.md) and in
   [README.md](../../../README.md). `notifications.v1` cannot be built without
   *reversing a documented security position* — which is an ADR, not a code
   comment edit. See §1.1.

2. **Android's own notification API is explicitly designed for this use case.**
   The javadoc for `META_DATA_DISABLED_FILTER_TYPES` says the disabled types
   appear greyed out *"so users don't enable a type that the listener will never
   **bridge to their paired devices**"* (AOSP VERIFIED). Notification bridging
   is a first-class, named scenario, and the platform gives us an OS-level type
   filter to declare. §1.6.

3. **Android 15+ redacts OTP notifications from untrusted listeners — and
   "trusted" includes a CompanionDeviceManager association.** The exact rule is
   in `NotificationManagerService.isAppTrustedNotificationListenerService`
   (AOSP VERIFIED). OmniBridge holds no CDM association today. This is
   simultaneously a privacy *gift* (the platform may hide OTPs for us) and a
   trap (we must not depend on it, and adopting CDM to become "trusted" would
   deliberately turn the protection off). §1.7.

4. **GNOME honours `replaces_id` exactly, and is forgiving about closes.**
   Verified by running it here: three `Notify` calls with `replaces_id=5`
   returned `5` every time, and `CloseNotification` on an already-closed id and
   on an id that never existed both returned success with no error and emitted
   no signal — where the spec says an error is required. Update-in-place and
   idempotent dismissal both work on the certification target. §2.2, §2.3.

---

## 1. Android

### 1.1 The manifest stance is a blocker until it is reversed

```console
$ grep -n "BIND_NOTIFICATION_LISTENER_SERVICE" -B4 -A4 android/app/src/main/AndroidManifest.xml
      Deliberately absent, and it must stay that way:
        * no BIND_ACCESSIBILITY_SERVICE
        * no BIND_NOTIFICATION_LISTENER_SERVICE
        * no QUERY_ALL_PACKAGES
        ...
```

This is not an oversight to be quietly deleted. Three documents state it as a
property of the product:

| Where | What it says |
| --- | --- |
| `AndroidManifest.xml` | "Deliberately absent, and it must stay that way" |
| `docs/security/THREAT_MODEL.md` §T26 | "The manifest declares no accessibility service, **no notification listener**, no `QUERY_ALL_PACKAGES` …" |
| `README.md` line 34 | "No root, no accessibility service, no ADB, no hidden permissions." |

`BIND_NOTIFICATION_LISTENER_SERVICE` is not a hidden permission and not root —
it is a documented, user-granted, Settings-gated capability, so the README line
survives unchanged. But T26 and the manifest comment are direct contradictions
and must be **amended by an ADR**, not by an edit. See
[05 §N0](05-IMPLEMENTATION-PLAN.md).

> **This was the single P0 blocker of the whole feature**
> ([06 · OQ-01](06-OPEN-QUESTIONS-AND-POCS.md)) — a product decision, not an
> engineering one.
>
> **Resolved 2026-09-08 by
> [ADR-0015](../../adr/ADR-0015-notification-access.md)** (Accepted): approved,
> under an explicit security contract. `THREAT_MODEL.md` T26 is amended and
> T27 added; the manifest comment is amended in N1, when the code that needs it
> lands. The finding above stands exactly as written — this note records that
> it has since been answered.

### 1.2 `NotificationListenerService` — the contract

AOSP VERIFIED against
`core/java/android/service/notification/NotificationListenerService.java`.

```xml
<service android:name=".NotificationListener"
         android:exported="false"
         android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE">
    <intent-filter>
        <action android:name="android.service.notification.NotificationListenerService" />
    </intent-filter>
</service>
```

The permission is held **by the system, not by us** — exactly like
`BIND_QUICK_SETTINGS_TILE` on the existing `ClipboardTileService`. It is what
stops any other app binding our service. The user grants *access* separately, in
Settings.

| Member | Behaviour | Consequence for us |
| --- | --- | --- |
| `onListenerConnected()` | *"You are safe to call `getActiveNotifications()` at this time"* | This is the resync trigger. [02 §7](02-PROTOCOL-AND-EVENT-MODEL.md) |
| `onListenerDisconnected()` | *"You will not receive any events after this call, and may only call `requestRebind()`"* | Role withdrawal must be announced to peers |
| `onNotificationPosted(sbn, rankingMap)` | Post **and** update — Android does not distinguish them | Drives the single-upsert event model. [02 §4](02-PROTOCOL-AND-EVENT-MODEL.md) |
| `onNotificationRemoved(sbn, rankingMap, reason)` | 23 reason codes | Only some justify telling the peer. §1.4 |
| `getActiveNotifications()` | *"the list of outstanding notifications (that is, those that are visible to the current user)"* | Active state, **not** history — the privacy line in [02 §7](02-PROTOCOL-AND-EVENT-MODEL.md) |
| `cancelNotification(String key)` | Dismiss one notification at the source | The whole of dismissal sync |
| `requestRebind(ComponentName)` | *"the **only** [method] that is safe to call before `onListenerConnected()` or after `onListenerDisconnected()`"* | Recovery path |

Two more constraints from the class javadoc, both AOSP VERIFIED and both
load-bearing:

* *"The system also **ignores notification listeners running in a work
  profile**."* → OmniBridge installed in a work profile receives nothing. Must be
  detected and said out loud, not left as a mystery.
* *"From `N` onward all callbacks are called on the **main thread**."* → every
  callback must hand off immediately; no protobuf encoding, no I/O, no hashing
  on that thread.

The `StatusBarNotification` delivered to `onNotificationRemoved` is explicitly
*"light"* — `getNotification()` "may be missing some heavyweight fields such as
`contentView` and `largeIcon`". We must never depend on removal carrying
content. We do not: removal carries an identity and nothing else.

### 1.3 Stable identity: the `key`

AOSP VERIFIED, `StatusBarNotification.key()`:

```java
String sbnKey = user.getIdentifier() + "|" + pkg + "|" + id + "|" + tag + "|" + uid;
```

This is the single most important fact for [02 §5](02-PROTOCOL-AND-EVENT-MODEL.md):

* it is **stable across updates** of the same notification — an app updating a
  download-progress notification reuses `id`/`tag`, so the key does not change;
* it is **unique across apps** (`pkg`, `uid`) and **across profiles**
  (`user.getIdentifier()`), so work and personal cannot collide;
* it is exactly what `cancelNotification(String key)` consumes, so dismissal
  needs no second identity;
* it contains a **uid and a profile id**, which is why it is *not* what we put
  on the wire.

### 1.4 Removal reasons

AOSP VERIFIED. All 23, with the ones that matter marked:

| # | Constant | Mirror should… |
| --: | --- | --- |
| 1 | `REASON_CLICK` | remove — user acted on it |
| **2** | **`REASON_CANCEL`** | **remove — user dismissed it on the phone** |
| **3** | **`REASON_CANCEL_ALL`** | **remove — user cleared all** |
| 4 | `REASON_ERROR` | remove |
| 5 | `REASON_PACKAGE_CHANGED` | remove |
| 6 | `REASON_USER_STOPPED` | remove |
| 7 | `REASON_PACKAGE_BANNED` | remove |
| **8** | **`REASON_APP_CANCEL`** | **remove — the app withdrew it** |
| 9 | `REASON_APP_CANCEL_ALL` | remove |
| **10** | **`REASON_LISTENER_CANCEL`** | **remove, but suppress the echo to the peer that asked** ([02 §9](02-PROTOCOL-AND-EVENT-MODEL.md)) |
| 11 | `REASON_LISTENER_CANCEL_ALL` | remove, same suppression |
| 12 | `REASON_GROUP_SUMMARY_CANCELED` | remove |
| 13 | `REASON_GROUP_OPTIMIZATION` | remove |
| 14 | `REASON_PACKAGE_SUSPENDED` | remove |
| 15 | `REASON_PROFILE_TURNED_OFF` | remove |
| 16 | `REASON_UNAUTOBUNDLED` | remove |
| 17 | `REASON_CHANNEL_BANNED` | remove |
| **18** | **`REASON_SNOOZED`** | **remove — it will be re-posted later** |
| 19 | `REASON_TIMEOUT` | remove |
| 20 | `REASON_CHANNEL_REMOVED` | remove |
| 21 | `REASON_CLEAR_DATA` | remove |
| 22 | `REASON_ASSISTANT_CANCEL` | remove |
| **23** | **`REASON_LOCKDOWN`** | **remove, urgently** |

`REASON_LOCKDOWN`'s javadoc is unusually direct: *"Notification was canceled
when entering lockdown mode… **all listeners shall ensure canceled
notifications are removed to prevent data leaking**."* A mirror that survives
lockdown mode is a data leak with a name. [02 §4.3](02-PROTOCOL-AND-EVENT-MODEL.md)
makes removal unconditional and reason-independent for this reason: **every**
removal removes the mirror. The reason is used only to decide whether to
*suppress the echo*, never whether to remove.

### 1.5 The data available on a `StatusBarNotification`

AOSP VERIFIED (`Notification.java`). Relevant, portable, and requested by the brief:

| Source | Constant / accessor | Verdict for v1 |
| --- | --- | --- |
| Package | `sbn.getPackageName()` | **carry** |
| App label | `PackageManager.getApplicationLabel` | **carry** (resolved on Android; the desktop has no package database) |
| Title | `extras.getCharSequence(EXTRA_TITLE)` `"android.title"` | **carry** |
| Body | `EXTRA_TEXT` `"android.text"` | **carry** |
| Big text | `EXTRA_BIG_TEXT` `"android.bigText"` | defer — see [02 §6](02-PROTOCOL-AND-EVENT-MODEL.md) |
| Sub text | `EXTRA_SUB_TEXT` `"android.subText"` | **defer** — no slot in the freedesktop schema |
| Progress | `EXTRA_PROGRESS`, `EXTRA_PROGRESS_MAX`, `EXTRA_PROGRESS_INDETERMINATE` | **carry** |
| Post time | `sbn.getPostTime()` | **carry**, informational only |
| Ongoing | `sbn.isOngoing()` (`FLAG_ONGOING_EVENT`) | **carry** |
| Dismissible | `sbn.isClearable()` (neither `FLAG_ONGOING_EVENT` nor `FLAG_NO_CLEAR`) | **carry** — decides whether dismissal sync is even offerable |
| Group | `sbn.getGroupKey()`, `FLAG_GROUP_SUMMARY` | **carry, hashed** |
| Category | `notification.category` (24 values, `CATEGORY_CALL` … `CATEGORY_VOICEMAIL`) | **carry a portable subset** |
| Visibility | `VISIBILITY_PUBLIC=1`, `VISIBILITY_PRIVATE=0`, `VISIBILITY_SECRET=-1` | **carry as a privacy enum** |
| Importance | `Ranking.getImportance()` / `getChannel().getImportance()` | **carry, mapped** |
| Profile | `sbn.getUserId()` | **carry as a boolean**, never the number |
| Icons | `EXTRA_LARGE_ICON`, `smallIcon` | **excluded from v1** |
| Actions | `notification.actions[]` (`PendingIntent`) | **excluded, permanently, from v1** |
| `RemoteViews` | `contentView` | **never** |

### 1.6 Android gives us an OS-level filter

AOSP VERIFIED. A listener may declare, in its manifest:

```xml
<meta-data android:name="android.service.notification.default_filter_types"
           android:value="conversations|alerting" />
<meta-data android:name="android.service.notification.disabled_filter_types"
           android:value="ongoing|silent" />
```

with types `FLAG_FILTER_TYPE_CONVERSATIONS=1`, `ALERTING=2`, `SILENT=4`,
`ONGOING=8`. The javadoc for the disabled list is the sentence quoted in §0.2:
the types *"will appear as 'off' and 'disabled' in the user interface, so users
don't enable a type that the listener will never bridge to their paired
devices."*

This is a real, free, OS-enforced narrowing that sits *underneath* OmniBridge's own
per-app filter, and it is enforced in `NotificationManagerService.isVisibleToListener`
via `NotificationListenerFilter`. [01 §5](01-FUNCTIONAL-SPECIFICATION.md) uses it.

There is also `META_DATA_DEFAULT_AUTOBIND`: setting it to `false` means the
system does **not** bind the listener by default and we bind on demand with
`requestRebind`. That is the difference between "OmniBridge is reading your
notifications whenever it is installed" and "OmniBridge reads your notifications
only while a peer that you granted is connected". [01 §4](01-FUNCTIONAL-SPECIFICATION.md)
takes the second.

### 1.7 Android 15+ sensitive-content redaction

AOSP VERIFIED, `NotificationManagerService.isAppTrustedNotificationListenerService`:

```java
if (mPackageManager.checkUidPermission(RECEIVE_SENSITIVE_NOTIFICATIONS, uid) == PERMISSION_GRANTED
        || mPackageManagerInternal.isPlatformSigned(pkg)
        || mAppOps.noteOpNoThrow(OP_RECEIVE_SENSITIVE_NOTIFICATIONS, uid, pkg, null, null) == MODE_ALLOWED) {
    return true;
}
// check if there is a CDM association with the listener
… for each AssociationInfo: if (!assocInfo.isRevoked() && pkg.equals(assocInfo.getPackageName())
        && assocInfo.getUserId() == UserHandle.getUserId(uid)) return true;
```

An untrusted listener receives a rebuilt notification whose title is the *app
label*, whose text is a fixed *"sensitive content hidden"* string, and which has
`EXTRA_SUB_TEXT`, `EXTRA_TEXT_LINES` and `EXTRA_LARGE_ICON_BIG` removed
(`redactStatusBarNotification`, AOSP VERIFIED). Sensitivity is set by the
**NotificationAssistantService** via `Adjustment.KEY_SENSITIVE_CONTENT` — that
is, by an on-device classifier, not by the posting app.

Three consequences:

* OmniBridge, as an ordinary app with no CDM association, is **untrusted** and
  would receive redacted OTP notifications. Good.
* Whether the mechanism is actually live on a given device depends on an
  aconfig flag (`redactSensitiveNotificationsFromUntrustedListeners`) *and* on an
  assistant that implements the classifier. On the certification target the
  assistant is present and live (DEVICE VERIFIED, §1.9) but the flag state could
  not be read from an unprivileged shell (`device_config get` returned `null`) →
  **[POC-NOTIF-01](06-OPEN-QUESTIONS-AND-POCS.md)**.
* **Adopting `CompanionDeviceManager` would switch this protection off.** It is
  tempting for other reasons (a CDM association is the platform-sanctioned way
  to justify a companion background service). It must not be adopted casually:
  the day OmniBridge gains a CDM association is the day it starts receiving
  unredacted OTPs. → **[OQ-04, P0](06-OPEN-QUESTIONS-AND-POCS.md)**.

**We therefore design as if no platform redaction exists.** Anything else would
be depending on a flag we cannot read, on a device we do not control, for a
protection the user believes they have.

### 1.8 Work profiles

AOSP VERIFIED, `ManagedServices.ManagedServiceInfo.enabledAndUserMatches`:

```java
return supportsProfiles()
        && mUserProfiles.isCurrentProfile(nid)
        && isPermittedForProfile(nid);
```

and `isPermittedForProfile` defers to
`DevicePolicyManager.isNotificationListenerServicePermitted(pkg, userId)`.

So: a listener in the **personal** profile *does* receive **work-profile**
notifications by default, and the work profile's administrator can turn that off
for us specifically. `sbn.getUserId()` is how we tell them apart. Mirroring an
organisation's notifications to a personal laptop is a decision an employee
should make deliberately, so [01 §5](01-FUNCTIONAL-SPECIFICATION.md) puts work-profile
notifications behind a separate, default-off switch — independent of the per-app list.

### 1.9 The certification target

DEVICE VERIFIED, 2026-09-08, `adb` against the attached device:

```console
$ adb shell getprop ro.product.model            → SM-X620
$ adb shell getprop ro.build.version.release    → 16
$ adb shell getprop ro.build.version.sdk        → 36
$ adb shell getprop ro.build.version.oneui      → 80500          (One UI 8.0)
$ adb shell getprop ro.build.version.security_patch → 2026-07-05
$ adb shell settings get secure enabled_notification_listeners
com.sec.android.app.launcher/…NotificationListener:com.samsung.android.smartmirroring/…NotificationService
$ adb shell dumpsys notification | grep -A8 "Notification assistant"
  Allowed notification assistants:
    com.google.android.ext.services/android.ext.services.notification.Assistant (user: 0 isPrimary: true)
  Live notification assistants (1): … ext.services…Assistant (user 0)
  (user) Denied Adjustment keys: user 0: [key_summarization]
```

Readings:

* API 36 / One UI 8.0 — **well past the Android 15 redaction change**, so §1.7
  applies to the very device we certify on.
* Two notification listeners are **enabled** on the device — but both are
  Samsung's own (`com.sec.android.app.launcher`,
  `com.samsung.android.smartmirroring`). So this transcript shows that the
  listener mechanism is in use on One UI; it does **not** evidence how One UI
  treats a *third-party* listener, and it was not checked whether either is
  currently bound. That gap is exactly what POC-NOTIF-04 measures.
* The Google assistant that implements the OTP classifier **is live**, and
  `KEY_SENSITIVE_CONTENT` is not in the denied-adjustment list. That is
  *suggestive*, not proof, that redaction is active — hence POC-NOTIF-01.
* One UI's "sleeping apps" / "deep sleeping apps" battery management is a
  documented Samsung behaviour that suspends background components. Its precise
  effect on a bound `NotificationListenerService` is **UNKNOWN** and cannot be
  settled from documentation → **[POC-NOTIF-04](06-OPEN-QUESTIONS-AND-POCS.md)**.
  This is the same class of risk that `ADR-0009` already handles for the
  connection service.

### 1.10 Granting notification access

AOSP VERIFIED (`android.provider.Settings`):

| API | Since | Use |
| --- | --- | --- |
| `ACTION_NOTIFICATION_LISTENER_SETTINGS` | 21 | The global list of listeners |
| `ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS` + `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME` | 30 | **Our** entry directly |
| `NotificationManager.isNotificationListenerAccessGranted(ComponentName)` | 27 | Read the current state |

`minSdk` for OmniBridge should be checked during N1, but the API-30 detail intent is
what makes the permission flow in [01 §7](01-FUNCTIONAL-SPECIFICATION.md) land the
user on one switch instead of a list of forty apps.

### 1.11 Distribution

Notification access is a "restricted" area of Google Play policy: it must be a
genuine core feature and the content must not be harvested. OmniBridge's posture —
local-first, no cloud, no history, no telemetry, explicit per-peer grant — is
about as defensible as this feature gets, and the app is distributed from GitHub
today rather than Play. Marked **LIKELY**, not verified: the current policy text
was not read from the Play Console, and it is not on the critical path for a
sideloaded build. → [OQ-09, P2](06-OPEN-QUESTIONS-AND-POCS.md).

---

## 2. Linux — `org.freedesktop.Notifications`

### 2.1 What the certification target actually is

HOST VERIFIED, 2026-09-08, this machine:

```console
$ gdbus call --session --dest org.freedesktop.Notifications \
      --object-path /org/freedesktop/Notifications \
      --method org.freedesktop.Notifications.GetServerInformation
('gnome-shell', 'GNOME', '50.4', '1.2')

$ … GetCapabilities
(['actions', 'body', 'body-markup', 'icon-static', 'persistence', 'sound'],)
```

GNOME Shell 50.4, speaking spec 1.2. Note what is **absent**: `body-images`,
`body-hyperlinks`, `action-icons`, `icon-multi`. GNOME will not render an image
in a notification body — which independently supports the v1 decision to carry
no images at all ([02 §6](02-PROTOCOL-AND-EVENT-MODEL.md)).

`persistence` is present, meaning *"Notifications will be retained until they
are acknowledged or removed by the user or recalled by the sender"* (SPEC
VERIFIED). Every mirror we post lands in the GNOME notification list and stays
there. That makes duplicate suppression a *visible* requirement rather than a
tidy one — a bug here produces a wall of stale notifications the user has to
clear by hand.

### 2.2 `replaces_id` — the update mechanism

SPEC VERIFIED:

> *"The optional notification ID that this notification replaces. The server
> must atomically (ie with no flicker or other visual cues) replace the given
> notification with this one. This allows clients to effectively modify the
> notification while it's active."*
>
> *"If `replaces_id` is not 0, the returned value is the same value as
> `replaces_id`."*

HOST VERIFIED — three calls, same id back every time:

```console
Notify(replaces_id=0)  -> (uint32 5,)
Notify(replaces_id=5)  -> (uint32 5,)
Notify(replaces_id=5)  -> (uint32 5,)
REPLACES_ID_PRESERVED=YES
```

This is the entire answer to "progress notifications must not create hundreds of
entries". One Android notification identity maps to one freedesktop id for its
whole life, and every update is a `Notify` with `replaces_id` set.

### 2.3 `CloseNotification` and `NotificationClosed`

SPEC VERIFIED — reasons are exhaustive and small:

| reason | Meaning |
| --: | --- |
| 1 | The notification expired |
| **2** | **The notification was dismissed by the user** |
| 3 | Closed by a call to `CloseNotification` |
| 4 | Undefined/reserved |

and, critically:

> *"The ID specified in the signal is invalidated **before** the signal is sent
> and may not be used in any further communications with the server."*

So a closed id is dead: a later `Notify` with that `replaces_id` creates a *new*
notification instead of updating. The sink's id map must be purged on
`NotificationClosed`, or a reconnect produces exactly the duplicate storm the
brief warns about. [02 §8](02-PROTOCOL-AND-EVENT-MODEL.md) specifies it.

The spec also says: *"If the notification no longer exists, an empty D-BUS Error
message is sent back."*

**GNOME does not do this.** HOST VERIFIED:

```console
--- close 5 (already displayed) ---      ()      → NotificationClosed(5, 3)
--- close 5 again ---                    ()      → no signal
--- close 999999 (never existed) ---     ()      → no signal
=== NotificationClosed signal count: 1 ===
```

Three closes, one signal, no errors. Idempotent dismissal is free on GNOME. But
this is a **deviation from the specification in our favour**, and other servers
(dunst, mako, a spec-literal implementation) may well return the error the spec
mandates. The sink must therefore treat a `CloseNotification` error as *success*
— the notification is gone either way, which is what was asked for. Writing that
rule down now is cheaper than debugging it on KDE later.

Only **reason 2** may be translated into a dismissal request to the phone.
Reason 1 (expired) is a desktop timeout, not a human decision, and reason 3 is
our own close coming back to us. [02 §9](02-PROTOCOL-AND-EVENT-MODEL.md).

### 2.4 Urgency, and why v1 never sends `critical`

SPEC VERIFIED: the `urgency` hint is a byte — 0 low, 1 normal, 2 critical. On
GNOME, critical notifications do not auto-dismiss.

A mirrored notification must never be *more* intrusive than the original. A chat
message with Android importance `HIGH` that becomes an undismissable banner on
the desktop is a worse product than no mirroring. [02 §6](02-PROTOCOL-AND-EVENT-MODEL.md)
maps Android importance onto `{low, normal}` only, and reserves `critical` as
unused in v1.

### 2.5 Lock state

The freedesktop notification interface says nothing about lock state, and a
client **cannot** control how the shell renders a notification on the lock
screen. The only lever we hold is what we put into the notification in the first
place. So the sink must know whether the session is locked.

HOST VERIFIED:

```console
$ gdbus call --session --dest org.gnome.ScreenSaver \
      --object-path /org/gnome/ScreenSaver --method org.gnome.ScreenSaver.GetActive
(false,)

$ gdbus introspect --session --dest org.gnome.ScreenSaver … | grep -A2 signals
    signals:
      ActiveChanged(b new_value);

$ loginctl show-session 2 -p LockedHint -p Type
Type=wayland
LockedHint=no
```

Two usable sources, and one trap:

* `org.gnome.ScreenSaver` — `GetActive()` plus an **`ActiveChanged(b)` signal**.
  Event-driven, so it satisfies ADR-0014's standing "never poll" rule. GNOME-specific.
* `logind` `LockedHint` on the session object — desktop-agnostic, and correct
  only where the locker sets it. GNOME does.
* **`org.freedesktop.ScreenSaver` is a trap.** The "portable-looking" name is
  served on GNOME by the *idle-inhibit* implementation and refuses `GetActive`:

  ```console
  Erro: GDBus.Error:org.freedesktop.DBus.Error.NotSupported: This method is not
  part of the idle inhibition specification
  ```

  Reaching for it because it has `freedesktop` in the name would produce a lock
  detector that never reports locked — failing *open*, on a privacy control.

Recommendation: `logind LockedHint` as the primary source with
`org.gnome.ScreenSaver.ActiveChanged` as the low-latency signal, and — because
this is a privacy control — **treat "unknown" as locked**. → [POC-NOTIF-02](06-OPEN-QUESTIONS-AND-POCS.md).

> **Corrected 2026-09-08 by [POC-NOTIF-02](poc/POC-NOTIF-02.md).** The primary
> source and the "unknown means locked" rule are confirmed. *"`ActiveChanged`
> as the low-latency signal" is wrong* — measured, it is neither low-latency
> nor a lock signal. It **lagged** `LockedHint` by 665 ms on a real
> `ScreenSaver.Lock()` (a fail-**open** window), and it fires for a blank
> *without* a lock. `LockedHint` is authoritative; `ActiveChanged` may be
> subscribed only as a prompt to re-read it, and its boolean is discarded.
> [OQ-14](06-OPEN-QUESTIONS-AND-POCS.md).

### 2.6 KDE Plasma

Plasma serves the same `org.freedesktop.Notifications` interface from
`plasma-workspace`, as already recorded in
[platform-expansion 06 §6](../platform-expansion/06-KDE-PLASMA-WAYLAND.md). The
sink is the same code. Its capability set and its `CloseNotification`
error behaviour were **not** measured (no Plasma machine here) → **LIKELY**, and
[POC-NOTIF-03](06-OPEN-QUESTIONS-AND-POCS.md) is one `gdbus` transcript long.

### 2.7 Linux as a notification *source*

Not in v1, and not for lack of ambition: there is no supported way for an
ordinary client to observe other applications' notifications through
`org.freedesktop.Notifications`. Doing it means becoming the notification server
(only one may own the name) or monitoring the bus for method calls, which
requires bus policy OmniBridge should not ask for. This is structurally the same
finding as ADR-0014's on clipboard watching, and it lands the same way: **Linux
is a sink in v1**, and the protocol is built so that it does not have to stay
one.

---

## 3. Windows — survey only

| Question | Finding | Evidence |
| --- | --- | --- |
| Display a remote notification? | `AppNotificationManager` (Windows App SDK) or `ToastNotificationManager`. Needs an AUMID — free with MSIX, registered by hand when unpackaged. Already recorded in [platform-expansion 08 §8](../platform-expansion/08-WINDOWS-FEASIBILITY.md) | OFFICIAL DOC |
| Observe *other apps'* notifications? | **Yes** — `Windows.UI.Notifications.Management.UserNotificationListener`, Windows 10 1607+. `RequestAccessAsync()` must be called from a UI thread; `GetAccessStatus()`, `GetNotificationsAsync(NotificationKinds)`, `NotificationChanged` ("Occurs when a notification is added or removed") | OFFICIAL DOC |
| Remove one at the source? | `RemoveNotification(UInt32)` — so Windows could be a dismissal target too | OFFICIAL DOC |
| Cost | Requires the **`userNotificationListener` restricted capability**, which requires **package identity**. An unpackaged desktop app cannot declare it | OFFICIAL DOC |
| Detect local dismissal of a toast we posted? | Not established here. `ToastNotification.Dismissed` exists in the UWP API surface; whether it fires for a desktop app's own toasts in the App SDK path is **UNKNOWN** | — |

**Conclusion for the protocol:** Windows can plausibly fill *every* role —
source, sink, and dismissal target. The protocol must therefore not assume
"source ⇒ Android" or "sink ⇒ desktop" anywhere. It does not:
[02 §3](02-PROTOCOL-AND-EVENT-MODEL.md) advertises roles per peer at runtime.

---

## 4. macOS — survey only

| Question | Finding | Evidence |
| --- | --- | --- |
| Display? | `UserNotifications` (`UNUserNotificationCenter`, `UNMutableNotificationContent`, `UNNotificationRequest`) after `requestAuthorization` | OFFICIAL DOC |
| Update in place? | Yes — re-adding a request with the **same identifier** replaces the delivered notification. Same shape as `replaces_id` | OFFICIAL DOC |
| Remove a mirror? | `removeDeliveredNotifications(withIdentifiers:)` | OFFICIAL DOC |
| Learn that the user dismissed one? | **Yes, but only if asked for.** `UNNotificationCategoryOptions.customDismissAction` — *"Send dismiss actions to the `UNUserNotificationCenter` object's delegate for handling."* Without it, dismissals are silent | OFFICIAL DOC |
| Observe *other apps'* notifications? | **No public API.** The `UserNotifications` framework manages an app's own notifications only | OFFICIAL DOC (by absence) |
| Title/subtitle/body | `UNMutableNotificationContent` has all three — so a `subtitle` field would be portable to macOS and Windows, but not to freedesktop | OFFICIAL DOC |

macOS is therefore a **sink with working dismissal detection**, and cannot be a
source. That asymmetry — sink yes, source no — is precisely why roles are
advertised per peer rather than inferred from platform.

---

## 5. iOS / iPadOS — survey only, stated honestly

| Role | Verdict | Why |
| --- | --- | --- |
| Notification **source** (mirror other apps' notifications) | **UNSUPPORTED** | No public API exposes another app's notifications to a third-party app. `UNNotificationServiceExtension` modifies *your own* app's incoming remote notifications and nothing else. There is no iOS analogue of `NotificationListenerService` |
| Notification **sink** | **LIMITED** | `UNUserNotificationCenter` can display a local notification with the same identifier-replacement and removal semantics as macOS — but only while the app is *running*. iOS suspends the app shortly after backgrounding and *"may choose to reclaim resources out from underneath a network socket used by the app"* — already OFFICIAL DOC VERIFIED in [platform-expansion 11](../platform-expansion/11-IOS-IPADOS-FEASIBILITY.md). No `UIBackgroundModes` value covers "hold a LAN control session" |
| Dismissal reporting | **LIMITED** | The macOS `customDismissAction` mechanism is UserNotifications, so it is available — but it needs the app to be alive to hear it |

The honest sentence, which must survive into the product copy: **iOS can be told
about a notification while OmniBridge is open, and cannot watch its own
notifications at all.** Anything better requires APNs and a relay server, which
is the exact trap [platform-expansion 11 §1](../platform-expansion/11-IOS-IPADOS-FEASIBILITY.md)
exists to prevent, and which would break "no cloud dependency".

---

## 6. What the repository already decided

Read before designing, per the brief:

| Source | What it constrains |
| --- | --- |
| [ADR-0008](../../adr/ADR-0008-capability-architecture.md) | Capability = versioned id + handler + opaque payload. **"Advertising is not authorization."** `auto_grant` holds `battery.v1` only. A breaking change is `name.v2`, never a version field |
| [ADR-0010](../../adr/ADR-0010-protocol-envelope-and-framing.md), [ADR-0012](../../adr/ADR-0012-bulk-transfer-and-frame-limit.md) | 64 KiB frames, length checked before allocation |
| [ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md) | **No polling, ever.** Event sources are detected once at startup and reported honestly when absent |
| [CLIPBOARD.md](../../architecture/CLIPBOARD.md) | Grant ≠ policy. `EventCache` + `SuppressionCache`. No relay, enforced by absence. Content never logged, never persisted. Monotonic clocks |
| [THREAT_MODEL.md](../../security/THREAT_MODEL.md) | T5 revocation is immediate; T11 logging rules; T26 the notification-listener stance that must be amended |
| [platform-expansion 28 §159](../platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md) | Wave 0 explicitly **declined** to create a `NotificationBackend` seam: *"OmniBridge implements no notifications on any platform. Nothing to abstract."* That is no longer true, and N2 creates the seam |
| `desktop/core/src/store.rs` | `TrustedPeer.granted_capabilities`, `clipboard_policy` — the exact pattern `notification_policy` follows |
| `desktop/core/src/clipboard_policy.rs` | Policy lives in core because it is *persisted*; the capability re-exports it |

`notifications.v1` invents no new mechanism. It adds one capability id, one
`.proto`, one policy struct, one Android service, and one Linux backend seam.

---

## 7. Sources

**Executed here** (HOST VERIFIED / DEVICE VERIFIED): `gdbus call` and
`dbus-monitor` against `org.freedesktop.Notifications` and
`org.gnome.ScreenSaver` on Fedora 44 / GNOME Shell 50.4; `loginctl show-session`;
`adb shell getprop`, `settings get secure`, `dumpsys notification` against
SM-X620 / Android 16 / One UI 8.0. Transcripts are quoted inline above.

**Platform source** (AOSP VERIFIED): `NotificationListenerService.java`,
`StatusBarNotification.java`, `Notification.java`, `NotificationManager.java`,
`Settings.java`, `NotificationManagerService.java`, `ManagedServices.java`, from
`aosp-mirror/platform_frameworks_base` `master`.

**Normative specifications** (SPEC VERIFIED): Desktop Notifications
Specification, §9 D-BUS Protocol —
<https://specifications.freedesktop.org/notification/latest/protocol.html>

**Vendor documentation** (OFFICIAL DOC): Android 15 behaviour changes;
`UserNotificationListener`; Windows app capability declarations; Apple
`UserNotifications` and `UNNotificationCategoryOptions.customDismissAction`.
