# POC-NOTIF-01 — Does Android 16 / One UI 8 redact OTP notifications from an untrusted listener?

| Field | Value |
| --- | --- |
| **Class** | P0 |
| **Defined in** | [06 §3](../06-OPEN-QUESTIONS-AND-POCS.md) |
| **Executed** | 2026-09-08 |
| **Verdict** | **CONCLUSIVE — redaction ABSENT.** The criterion's *Fail* branch: *"OTP-shaped notifications arrive with content intact — meaning the protection is absent on this device and the design's assumption (that it must be assumed absent) was correct and load-bearing."* |
| **Consequence** | No architecture change. Documentation changes to [03 §T-N02](../03-PRIVACY-SECURITY-THREAT-MODEL.md) and [01 §8.1](../01-FUNCTIONAL-SPECIFICATION.md) |

> **Read the verdict carefully.** The pass criterion asks whether *Android*
> protects us. It does not. That is a statement about the platform, not a
> defect in `notifications.v1`: the design already assumes no redaction
> ([03 assumption 3](../03-PRIVACY-SECURITY-THREAT-MODEL.md)). The PoC's value
> is that the assumption is now **measured** rather than prudent.

---

## 1. Platform under test

| Field | Value | Source |
| --- | --- | --- |
| Model | `SM-X620` | `getprop ro.product.model` |
| Android | 16 | `getprop ro.build.version.release` |
| API level | 36 | `getprop ro.build.version.sdk` |
| One UI | 8.0 (`80500`) | `getprop ro.build.version.oneui` |
| Security patch | 2026-07-05 | `getprop ro.build.version.security_patch` |
| Build | `X620XXS9CZG3` | `getprop ro.build.PDA` |

## 2. Preconditions, verified on the device

**The redaction feature flag is ON.** The research could not read it from an
unprivileged shell ([00 §1.7](../00-RESEARCH-FINDINGS.md)); it is readable
through `device_config`, which is where it actually lives:

```console
$ adb shell cmd device_config list systemui | grep -i redact
android.service.notification.redact_sensitive_notifications_big_text_style=true
android.service.notification.redact_sensitive_notifications_from_untrusted_listeners=true
```

**The OTP classifier is installed, enabled and bound.** Google's assistant is
the primary notification assistant and holds a live binding:

```console
$ adb shell dumpsys notification | sed -n '/Notification assistant services/,+8p'
  Notification assistant services:
    Allowed notification assistants:
      com.google.android.ext.services/android.ext.services.notification.Assistant (user: 0 isPrimary: true isUserChanged: false)
    Live notification assistants (1):
      ComponentInfo{com.google.android.ext.services/android.ext.services.notification.Assistant} (user 0): ...

$ adb shell cmd device_config list device_personalization_services | grep -i otp
Notification__enable_otp_in_smart_suggestion=true
```

Note that `settings get secure enabled_notification_assistant` returns `null`
on this device — the assistant is the platform default, not a user selection,
so that setting is the wrong place to look. `dumpsys` is authoritative.

**The listener under test is untrusted, and there is no CDM association at
all** — on the whole device, not just for our app:

```console
$ adb shell dumpsys notification | grep -m1 mTrustedListenerUids
    mTrustedListenerUids={1000, 10064, 10135}

$ adb shell dumpsys notification | grep -m1 "Approved uids for user 0"
    Approved uids for user 0: [10212, 10135, 10408]      # 10408 = the PoC listener

$ adb shell dumpsys companiondevice | head -1
Companion Device Associations: <empty>
```

The PoC listener's uid (**10408**) is in the *approved* set and **not** in the
*trusted* set. It is exactly the position `notifications.v1` will occupy.

> **A correction to the research, recorded here rather than buried.**
> [00 §1.7](../00-RESEARCH-FINDINGS.md) and
> [03 §T-N03](../03-PRIVACY-SECURITY-THREAT-MODEL.md) present a CDM association
> as *the* route by which a listener becomes trusted. On this device the
> trusted set is non-empty (`10135`, a Samsung system listener) while the CDM
> association table is **empty**. So CDM is *a* route to trust, not the only
> one. The AOSP reading behind [OQ-04](../06-OPEN-QUESTIONS-AND-POCS.md) is
> unaffected — a CDM association still confers trust — but the claim must not
> be stated in reverse.

## 3. Tooling

Two throwaway APKs, built outside the repository in a scratch directory, with
`aapt2` + `javac --release 17` + `d8` + `apksigner` from `build-tools 35.0.0`.
**No OmniBridge production source was modified, and no PoC binary is committed.**

* `dev.throwaway.poclistener` — a `NotificationListenerService`.
* `dev.throwaway.pocposter` — an `Activity` that posts the test cases and exits.

The listener obeys the PoC's logging rule. It records the package, the field
**lengths**, and a boolean saying whether each received string is byte-identical
to one the poster posted. A received string is printed verbatim **only when it
matches nothing the poster posted** — i.e. only when it is platform-generated
text. Notification content is therefore never logged, and platform redaction
text cannot be missed.

## 4. Test inputs

All strings are synthetic. `483920` is invented; no real service issued it, and
no real OTP, banking alert, message or credential was used at any point.

| id | Vector | Title | Text |
| --- | --- | --- | --- |
| 100 | plain | `Verification` | `Your verification code is 483920` |
| 101 | plain | `Passcode` | `483920 is your one-time passcode` |
| 102 | plain | `Google` | `G-483920` |
| 103 | **control** | `Control` | `Lunch is ready in the kitchen` |
| 110 | `BigTextStyle` | `Bank` | `Your verification code is 483920` (+ big text) |
| 111 | `CATEGORY_MESSAGE` | `Chat` | `483920 is your one-time passcode` |
| 112 | `CATEGORY_EMAIL` + big text | `Email` | `G-483920 is your Google verification code` |

Cases 110–112 are a **second vector**, added after the first round returned no
redaction, because the flag name (`…_big_text_style`) and the classifier's
documented behaviour both suggest style and category might gate it.

## 5. Procedure

```console
$ adb install -r -g listener.apk && adb install -r -g poster.apk
$ adb shell pm grant dev.throwaway.pocposter android.permission.POST_NOTIFICATIONS
$ adb shell cmd notification allow_listener dev.throwaway.poclistener/poc.PocListener
$ adb logcat -c
$ adb shell am start -n dev.throwaway.pocposter/poc.PostActivity
$ adb logcat -d -s POCNOTIF01:I
```

The listener additionally re-reads each notification through
`getActiveNotifications(key)` on a delay, so that a redaction applied *after*
the assistant classifies would still be caught.

## 6. Observed

```text
POSTED pkg=dev.throwaway.pocposter id=100 | title.len=12 title.verbatim=true | text.len=32 text.verbatim=true | bigText=<null> | cat=null
POSTED pkg=dev.throwaway.pocposter id=101 | title.len=8  title.verbatim=true | text.len=32 text.verbatim=true | bigText=<null> | cat=null
POSTED pkg=dev.throwaway.pocposter id=102 | title.len=6  title.verbatim=true | text.len=8  text.verbatim=true | bigText=<null> | cat=null
POSTED pkg=dev.throwaway.pocposter id=103 | title.len=7  title.verbatim=true | text.len=29 text.verbatim=true | bigText=<null> | cat=null
POSTED pkg=dev.throwaway.pocposter id=110 | title.len=4  title.verbatim=true | text.len=32 text.verbatim=true | bigText.len=62 bigText.verbatim=true | cat=null
POSTED pkg=dev.throwaway.pocposter id=111 | title.len=4  title.verbatim=true | text.len=32 text.verbatim=true | bigText=<null> | cat=msg
POSTED pkg=dev.throwaway.pocposter id=112 | title.len=5  title.verbatim=true | text.len=41 text.verbatim=true | bigText.len=41 bigText.verbatim=true | cat=email
```

Round one additionally logged a `RECHECK` five seconds after each post; every
`RECHECK` line was identical to its `POSTED` line.

| id | Expected if redaction fires | Received |
| --- | --- | --- |
| 100 | title → app label, text → platform string | **verbatim, unredacted** |
| 101 | title → app label, text → platform string | **verbatim, unredacted** |
| 102 | title → app label, text → platform string | **verbatim, unredacted** |
| 103 (control) | intact | intact |
| 110 | redacted | **verbatim, unredacted** |
| 111 | redacted | **verbatim, unredacted** |
| 112 | redacted | **verbatim, unredacted** |

`verbatim=false` appeared **zero** times across both rounds.

## 7. Why it did not fire

The assistant is bound but is issuing no adjustments at all. Read after the
posts had settled:

```console
$ adb shell dumpsys notification | grep -A90 "pkg=dev.throwaway.pocposter ... id=110" | grep -m2 "mSensitiveContent\|mAdjustments"
      mSensitiveContent=false
      mAdjustments=[]

$ adb shell dumpsys notification | grep -c "mSensitiveContent=true"
0
```

**No notification on the device — 93 records in total, from every installed
app — is classified as sensitive.** So this is not our test strings failing a
regex; the classification pipeline is inert on this build. The platform side
of the mechanism (`redact_sensitive_notifications_from_untrusted_listeners`) is
enabled and would presumably act if an adjustment ever arrived, but nothing
sets `mSensitiveContent`.

Why the Google assistant issues no adjustments on this Samsung build was not
established, and establishing it is not necessary: the design must work whether
or not it does.

## 8. An incidental finding — adaptive bundling reaches the listener

One UI 8 posted a **synthetic group summary** that our listener received, with
a key OmniBridge did not cause and null title and text:

```text
POSTED pkg=dev.throwaway.pocposter id=0
  key=0|dev.throwaway.pocposter|0|0|dev.throwaway.pocposter|g:Aggregate_NormalNotificationSection|10409|...
  | title=<null> | text=<null> | bigText=<null> | cat=null
```

This is direct evidence for [OQ-13](../06-OPEN-QUESTIONS-AND-POCS.md) (Android
16 adaptive bundling) and bears on [OQ-10](../06-OPEN-QUESTIONS-AND-POCS.md)
(group summaries): a listener sees synthetic aggregate summaries carrying no
content, and a filter that mirrors everything would mirror an empty
notification. It reinforces the provisional answer — drop
`FLAG_GROUP_SUMMARY`, mirror members. Both remain **P1/P2 for N1**, not for N0.

## 9. Verdict and consequences

**Redaction is ABSENT on the certification target.** The design's assumption is
confirmed and is load-bearing.

| What changes | Where |
| --- | --- |
| Residual risk in T-N02 moves from *"High where platform redaction is absent, and presence is unknown"* to *"High, and absence is measured on the certification target"* | [03 §T-N02](../03-PRIVACY-SECURITY-THREAT-MODEL.md) |
| Product copy must not offer platform redaction as reassurance, on any device | [01 §8.1](../01-FUNCTIONAL-SPECIFICATION.md) |
| The per-app allow-list is the **only** effective control over OTP mirroring, which is an argument for [OQ-02](../06-OPEN-QUESTIONS-AND-POCS.md)'s deny-by-default, not against it | [ADR-0015](../../../adr/ADR-0015-notification-access.md) |
| Nothing in the protocol, identity, filter or lock design changes | — |

## 10. Cleanup

```console
$ adb shell cmd notification disallow_listener dev.throwaway.poclistener/poc.PocListener
$ adb shell pm uninstall dev.throwaway.pocposter && adb shell pm uninstall dev.throwaway.poclistener
$ adb shell settings put secure enabled_notification_listeners "<original value>"
```

Verified afterwards: no `throwaway` package present; `enabled_notification_listeners`
byte-identical to its pre-PoC value; `Approved uids for user 0` back to
`[10212, 10135]`; no PoC notification active. No PoC artifact is committed.
