# OmniBridge — `notifications.v1` research and specification

**Mirror the phone's notifications onto the desktop. Dismiss them once.**

| Field | Value |
| --- | --- |
| **Status** | **Research, specification and decisions complete. Nothing implemented.** |
| **Last reviewed** | 2026-09-08 (decision sprint) |
| **Branch** | `research/notifications-v1` |
| **Scope** | Android notification source → Linux desktop sink, with synchronised dismissal. Windows, macOS and iOS/iPadOS surveyed only far enough to keep the protocol honest |
| **Decision status** | **Approved.** [ADR-0015](../../adr/ADR-0015-notification-access.md) is Accepted; OQ-01 … OQ-04 are resolved; both P0 proofs of concept have been run on the certification hardware. **No BLOCKER and no open P0 remain** — see [06 §4](06-OPEN-QUESTIONS-AND-POCS.md) |
| **Canonical decisions** | [ADR-0015](../../adr/ADR-0015-notification-access.md). The documents here restate its outcomes by reference and must not diverge from it |
| **Certification target** | SM-X620 · Android 16 · One UI 8.0 ↔ Fedora 44 · GNOME Shell 50.4 (both verified present, 2026-09-08) |

---

## What this is

The answer to one question:

> *What does OmniBridge need to define before it can mirror Android notifications
> to a Linux desktop without an unresolved architectural or privacy ambiguity?*

It is research, a specification and a plan. **No production source file was
modified.** No `.proto` was created or changed. No `NotificationListenerService`
exists. No Linux notification backend exists. No protocol, TLS, pairing or
capability behaviour changed.

---

## Start here

**If you have five minutes:** [00 §0 — the four findings that change the design](00-RESEARCH-FINDINGS.md).

**If you want the decisions:** [ADR-0015](../../adr/ADR-0015-notification-access.md).
It is the canonical record: what is approved, under what contract, and what is
still refused.

**If you are implementing:** [02](02-PROTOCOL-AND-EVENT-MODEL.md), then
[05](05-IMPLEMENTATION-PLAN.md).

---

## Index

| # | Document | Purpose | Evidence |
| --- | --- | --- | --- |
| 00 | [Research findings](00-RESEARCH-FINDINGS.md) | What each platform actually does, with transcripts | HOST / DEVICE / AOSP / SPEC VERIFIED |
| 01 | [Functional specification](01-FUNCTIONAL-SPECIFICATION.md) | Product behaviour, consent model, filters, lock policy, UX, i18n | Design |
| 02 | [Protocol and event model](02-PROTOCOL-AND-EVENT-MODEL.md) | Schema, identity, lifecycle, limits, failure semantics | Design |
| 03 | [Privacy, security and threat model](03-PRIVACY-SECURITY-THREAT-MODEL.md) | T-N01 … T-N16, and the test list that proves them | Design |
| 04 | [Platform capability matrix](04-PLATFORM-CAPABILITY-MATRIX.md) | Six platforms × nine capabilities, per-cell evidence | Mixed, per cell |
| 05 | [Implementation plan](05-IMPLEMENTATION-PLAN.md) | Waves N0 – N6, mapped onto this repository | Plan |
| 06 | [Open questions and PoCs](06-OPEN-QUESTIONS-AND-POCS.md) | 15 questions, 6 proofs of concept | Live list |
| — | [poc/POC-NOTIF-01.md](poc/POC-NOTIF-01.md) | OTP redaction on Android 16 / One UI 8 — **executed** | DEVICE VERIFIED |
| — | [poc/POC-NOTIF-02.md](poc/POC-NOTIF-02.md) | GNOME lock detection — **executed, PASS** | HOST VERIFIED |

---

## Dashboard

| Area | Status | One-line finding |
| --- | --- | --- |
| **The decision to build it** | **APPROVED** | [ADR-0015](../../adr/ADR-0015-notification-access.md), Accepted 2026-09-08. Optional, off by default, two separate permissions, independently revocable, fails closed. The old position is amended and its history recorded, not deleted |
| **Android source** | **VERIFIED** | `NotificationListenerService` fits exactly. The API is explicitly designed for *"bridging to paired devices"*, and gives us an OS-level type filter and on-demand binding |
| **Stable identity** | **VERIFIED** | Android's `key` is `userId\|pkg\|id\|tag\|uid`. We transmit an HMAC-derived 16-byte id instead, so it survives a process restart without carrying a uid |
| **Linux sink** | **VERIFIED** | GNOME Shell 50.4 honours `replaces_id` exactly — measured here. Progress notifications update one entry rather than creating hundreds |
| **Dismissal** | **VERIFIED** mechanism, **PoC** for the human half | `NotificationClosed` reason **2 only**. Reason 1 (expired) must never dismiss the phone's notification |
| **Idempotent close** | **VERIFIED**, with a spec deviation | GNOME returns success for closing an unknown id where the spec mandates an error. The sink must treat a close error as success anyway |
| **OTP protection** | **MEASURED ABSENT** | Six OTP-shaped notifications, three vectors, all delivered verbatim to an untrusted listener on the certification target, with the platform flag on → [POC-NOTIF-01](poc/POC-NOTIF-01.md). The design already assumed this. The allow-list is the only real control |
| **CompanionDeviceManager** | **NOT USED IN V1** | Deferred, not forbidden. Re-adopting it needs its own security ADR and a re-run of POC-NOTIF-01 under a live association → [ADR-0015 §10](../../adr/ADR-0015-notification-access.md) |
| **Dismiss sync** | **DEFAULT OFF** | Supported, opt-in per peer. Reverses the research recommendation: it is the only operation that changes state on the source device → [ADR-0015 §6](../../adr/ADR-0015-notification-access.md) |
| **Lock detection** | **MEASURED**, with two traps | **`logind LockedHint` is authoritative** (218 ms on a real lock). `org.gnome.ScreenSaver.ActiveChanged` is a hint only — it lags a lock by 665 ms (**fails open**) and fires for a blank without a lock. `org.freedesktop.ScreenSaver` is idle-inhibit on GNOME and refuses `GetActive` → [POC-NOTIF-02](poc/POC-NOTIF-02.md) |
| **Work profile** | **VERIFIED** | A personal-profile listener sees work notifications unless the administrator blocks it. Separate switch, default off |
| **App filter default** | **APPROVED — deny-all** | Nothing mirrors until a person names it; select-all is one tap inside the enabling flow → [ADR-0015 §5](../../adr/ADR-0015-notification-access.md) |
| **History** | **Forbidden by design** | Active-state snapshot on reconnect ≠ history. The distinction is specified rather than asserted → [02 §7.3](02-PROTOCOL-AND-EVENT-MODEL.md) |
| **Windows / macOS / iOS** | **Surveyed** | Windows can fill *every* role; macOS can sink but never source; iOS can do neither usefully. This asymmetry is why roles are announced per peer |

---

## The design in one page

```text
  Android (SOURCE, DISMISS_TARGET)                Linux (SINK, DISMISS_REPORTER)
  ────────────────────────────────                ──────────────────────────────
  NotificationListenerService                     org.freedesktop.Notifications
        │ onNotificationPosted                          ▲ Notify(replaces_id)
        │                                               │
        ├─ drop own package        (hard rule)          │
        ├─ per-app allow list      (default empty)      │
        ├─ work profile / ongoing  (default off)        │
        ├─ VISIBILITY_SECRET       (never)              │
        ├─ lock policy             (reduce at source)   │
        ├─ id = HMAC(secret, key)[0..16]                │
        │                                               │
        └──── NotificationUpsert ───────────────────────┤
              NotificationRemove  ──────────────────────┤
                                                        │
              DismissRequest      ◀───────────────────── NotificationClosed(reason=2)
        │                                                        ONLY reason 2
        └─ cancelNotification(key), echo suppressed to the requester
```

One capability id. One control session. Six message types. No second socket, no
`PendingIntent`, no images, no history, no cloud.

---

## What was verified by running it

Everything in [00](00-RESEARCH-FINDINGS.md) marked HOST VERIFIED or DEVICE
VERIFIED was executed on 2026-09-08:

* `GetServerInformation` / `GetCapabilities` against the live GNOME notification
  server;
* three `Notify` calls proving `replaces_id` is honoured;
* three `CloseNotification` calls proving closes are idempotent here and emit
  exactly one `NotificationClosed(id, 3)`;
* `org.gnome.ScreenSaver` introspection showing `ActiveChanged`, and
  `org.freedesktop.ScreenSaver` refusing `GetActive`;
* `loginctl show-session` for `LockedHint`;
* `adb` against the attached SM-X620 for the OS version, One UI version, the
  enabled notification listeners and the live notification assistant.

Everything marked AOSP VERIFIED was read from
`aosp-mirror/platform_frameworks_base` at `master`; everything marked SPEC
VERIFIED from the freedesktop Desktop Notifications Specification §9.

## What was not verified

Six things, each with a PoC written for it in
[06 §3](06-OPEN-QUESTIONS-AND-POCS.md). **Two have since been run**, on
2026-09-08, on the certification hardware:

* [POC-NOTIF-01](poc/POC-NOTIF-01.md) — Android 16 / One UI 8 does **not**
  redact OTPs for us. Measured, not assumed.
* [POC-NOTIF-02](poc/POC-NOTIF-02.md) — GNOME lock detection works, and
  **`ActiveChanged` is the wrong signal to build on**. PASS, with a correction.

Four remain, and none of them blocks N0: whether KDE Plasma matches GNOME (no
hardware here), whether One UI's app-sleep management suspends a bound listener
(needs an N1 build and an overnight idle), whether a human dismissal on GNOME
really emits reason 2 (needs a human to click), and whether an in-place update
re-alerts.

None of them was guessed at.
