# 01 — `notifications.v1` functional specification

| Field | Value |
| --- | --- |
| **Title** | What the feature does, and what a person has to agree to first |
| **Status** | Specification — proposed, not implemented |
| **Last reviewed** | 2026-09-08 |
| **Scope** | Android notification source → Linux desktop sink, with synchronised dismissal |
| **Related** | [00](00-RESEARCH-FINDINGS.md), [02](02-PROTOCOL-AND-EVENT-MODEL.md), [03](03-PRIVACY-SECURITY-THREAT-MODEL.md), [05](05-IMPLEMENTATION-PLAN.md), [CLIPBOARD.md](../../architecture/CLIPBOARD.md), [UI-GUIDELINES.md](../../design/UI-GUIDELINES.md) |

---

## 1. What this is, stated accurately

```text
Automatic  Android → Fedora   notification mirroring   (opt-in, per device, per app)
Manual     Fedora  → Android  dismissal only           (a person dismisses a mirror)
```

It is **notification mirroring with dismissal sync**. It is not "notifications
on your desktop" in the sense that phrase is usually sold, and the difference is
worth stating in the product copy rather than discovered:

* You cannot **reply** from the desktop. Not in v1, and not as a hidden setting.
* You cannot **press a notification's buttons** from the desktop.
* There is **no history**. A dismissed notification is gone; there is no screen
  that lists what arrived earlier ([02 §7.3](02-PROTOCOL-AND-EVENT-MODEL.md)).
* There is **no cloud**, no relay, no account. The phone talks to the computer
  over the LAN inside the existing TLS 1.3 session, or it does not talk at all.
* Nothing is **persisted**: no notification content reaches `state.json`, the
  trust store, a database, a log, or a crash report.

## 2. Scope

### In scope for v1

| | |
| --- | --- |
| Direction | Android → Linux |
| Events | Posted / updated (one idempotent upsert), removed |
| Content | App id, app label, title, body, importance, privacy class, category, progress, ongoing/dismissible flags, group, timestamps |
| Identity | Stable, derived, per-notification ([02 §5](02-PROTOCOL-AND-EVENT-MODEL.md)) |
| Consent | Per-peer capability grant, **plus** Android OS notification access, **plus** a per-app allow list |
| Privacy | Lock-screen policy on both ends; `SECRET` notifications never mirrored |
| Dismissal | Desktop dismiss → phone `cancelNotification(key)`, where the notification is dismissible |

### Not in scope for v1

Reply · action buttons · `RemoteInput` · `PendingIntent` execution of any kind ·
icons and images · arbitrary notification blobs · notification history · cloud
relay · Linux → Android notification mirroring (§9) · Windows, macOS and
iOS/iPadOS implementations.

The brief invited a challenge to this boundary if research found a reason. It
found one candidate and rejected it: Windows can technically fill every role
([00 §3](00-RESEARCH-FINDINGS.md)), which argues for keeping the *protocol*
symmetric — and [02 §3](02-PROTOCOL-AND-EVENT-MODEL.md) does exactly that — but
it is not an argument for implementing Windows in v1. The boundary stands.

---

## 3. Grant and policy: two different questions, again

`notifications.v1` follows the structure `clipboard.v1` established, because the
reasoning transfers exactly.

```text
              notifications.v1 grant       ← never automatic (ADR-0008)
                     │
       ┌─────────────┼──────────────┬────────────────┐
       │             │              │                │
  allow_mirror  allow_dismiss   app filter     lock policy
                                (per app)      (per peer)
```

* The **grant** lives in the trust store beside `files.v1`'s and `clipboard.v1`'s
  and answers *"may this device speak notifications with me at all?"*. It is
  **not** in `auto_grant` — `auto_grant` still contains `battery.v1` alone, and a
  capability that carries a person's messages is the clearest possible case of
  ADR-0008's "anything with side effects must be granted explicitly".
* The **policy** answers *"and which notifications, and how much of them?"*.

```rust
// desktop/core/src/notification_policy.rs   (PROPOSED — not implemented)
//
// Lives in core, not in the capability crate, for the reason
// `clipboard_policy.rs` gives: it is *persisted*, and the trust store is core's.
pub struct NotificationPolicy {
    pub allow_mirror: bool,        // default true  — what the grant was for
    pub allow_dismiss_sync: bool,  // default FALSE — see §3.1
    pub when_source_locked: LockPolicy,      // default AppOnly
    pub when_sink_locked: LockPolicy,        // default AppOnly
    pub include_work_profile: bool,          // default false
    pub include_ongoing: bool,               // default false
    pub app_filter: AppFilter,               // default DenyAll (§5)
}
```

Stored per peer, `#[serde(default)]` throughout so a trust store written before
this capability existed loads with the safe defaults rather than failing or
silently disabling something.

**A peer can never set its own policy.** There is no protocol message that
writes any of these fields — the schema has no such message, so the rule is an
absence rather than a check that could be inverted. This is the same guarantee
`clipboard.v1` makes and it is worth preserving by construction.

### 3.1 Why `allow_dismiss_sync` defaults **off**

> **Decided 2026-09-08. This section reverses the recommendation it originally
> made.** The canonical record is
> [ADR-0015 §6](../../adr/ADR-0015-notification-access.md); the original
> argument is kept below because it is the strongest case against the decision
> and a future reader deserves to see it.

**Decision: supported, opt-in per peer, default `false`.**

The case that was made for defaulting it *on*, and it is a real one: dismissal
sync is not a new authority, because the desktop can only dismiss a notification
it is *already being shown*, and being shown it is what the grant was for. The
action is one platform call with no parameters beyond an identity, it cannot be
widened into anything else ([02 §4.4](02-PROTOCOL-AND-EVENT-MODEL.md)), and it
is reversible in the only sense that matters — the app can post again. Against
that, a mirror you cannot dismiss is actively bad: clearing your desk
notifications leaves your phone buzzing with the same twenty items, so the
feature's *absence* is what a user would report as a bug.

**Why it was rejected anyway.** The argument is about utility and the objection
is about category. Every other thing `notifications.v1` does is passive
observation: the desktop learns what the phone already decided to show. Dismiss
sync is the **only** operation that changes state on the source device. "This
computer may see my phone" and "this computer may act on my phone" are two
consents, and the second is worth asking for separately even when the action it
authorises is small — because the boundary being crossed, not the size of the
step, is what a person is agreeing to. Every other default in this capability
starts closed; this one should not be the exception.

The discoverability cost is real and is paid the same way §5.1 pays it: the
per-peer screen offers the switch beside the app picker at the moment the
feature is enabled, so it is one tap for anyone who wants it, and it is never a
thing that happened to them.

The flag is per peer, because "my work laptop may see my notifications but may
not touch my phone" is a coherent thing to want — and now it is also the
starting state.

### 3.2 Narrowing is immediate; widening needs a reconnect

Inherited from the foundation, unchanged: the effective capability set is fixed
at handshake time, so *adding* `notifications.v1` needs a new handshake, while
every inbound message is re-checked against the trust store, so *removing* it
bites at once — including on an established session.

`notifications.v1` adds one consequence the other capabilities do not have: a
revocation must also **close every mirror already on the desktop**. A withdrawn
grant that leaves the last forty notifications sitting on a screen has not
actually withdrawn anything.

---

## 4. When the listener is bound at all

`META_DATA_DEFAULT_AUTOBIND = false` ([00 §1.6](00-RESEARCH-FINDINGS.md)).

The system therefore does **not** bind OmniBridge's listener merely because the app
is installed and access is granted. OmniBridge binds it with `requestRebind` only
while:

* at least one paired peer holds a `notifications.v1` grant, **and**
* that peer has `allow_mirror` on, **and**
* a session to it is established.

and unbinds with `requestUnbind` when the last such peer goes away.

This is the same discipline the clipboard watcher already follows — *"it starts
only when needed: with no `auto_send` peer there is no watcher, no child process
and no X connection"* — and it buys the same three things: no notification data
is read when nobody is listening, the battery cost is zero when idle, and the
claim "OmniBridge reads your notifications only while your computer is connected"
is structurally true rather than a promise.

---

## 5. Per-app filtering

### 5.1 The recommendation: deny by default, chosen during onboarding

The brief offers (A) all apps on after the grant, (B) nothing until selected,
(C) an explicit onboarding choice.

**Recommendation: (B) semantics, presented as (C).** The list starts empty; the
user is taken straight to an app-picker as the last step of enabling the
feature; the picker has a **Select all** button one tap away.

Why not (A). Granting notification access and immediately having your phone's
entire notification stream appear on a screen — possibly a screen in an office,
possibly with a colleague at your shoulder — is a surprise with real
consequences. The set includes banking alerts, 2FA codes, medical reminders and
whatever your messaging apps put in a preview. The project's stated posture is
deny-by-default in every comparable place: `auto_grant` holds only read-only
telemetry, `auto_send`/`auto_receive` are off after an explicit clipboard grant,
and every file transfer needs a human. Notifications are more sensitive than any
of those, so defaulting them *on* would be the single most permissive default in
the product.

Why not pure (B). A feature that does nothing after you turn it on reads as
broken, and a user who cannot find the app list will conclude the mirroring does
not work. That is a real cost and it is why the picker is part of the enabling
flow rather than a settings screen the user has to discover.

The result is honest in both directions: nothing is shared until a person names
what to share, and naming everything takes one tap for the people who want that.

### 5.2 Newly installed apps: default deny

An app installed after the grant is **not** shared. OmniBridge does not
retroactively widen a decision the user made about a different set of apps.

The discoverability cost is paid with a passive affordance, not a prompt:
the notifications settings screen shows *"3 new apps are not being shared"* with
a link to the picker. No notification about notifications, no interruption, no
dialog. If the user never looks, the safe thing keeps happening.

### 5.3 Rules that no filter setting can override

| Rule | Why |
| --- | --- |
| **OmniBridge's own package is never mirrored** | Loop prevention ([02 §9.4](02-PROTOCOL-AND-EVENT-MODEL.md)). Not a default — there is no setting |
| **`VISIBILITY_SECRET` is never mirrored** | The app said "not even on a lock screen". [02 §6.2](02-PROTOCOL-AND-EVENT-MODEL.md) |
| **Work-profile notifications need `include_work_profile`** | Separate, default-off, and independent of the app list. §5.4 |
| **Ongoing notifications need `include_ongoing`** | Default off. §5.5 |

### 5.4 Work profile

A listener in the personal profile receives work-profile notifications unless
the profile's administrator blocks it ([00 §1.8](00-RESEARCH-FINDINGS.md)), and
`sbn.getUserId()` distinguishes them.

Mirroring an employer's data onto a personal machine is a decision with
consequences that are not OmniBridge's to make on someone's behalf, so
`include_work_profile` is **off by default** and is a separate switch from the
app list — a user who shares "Slack" from their personal profile has not thereby
asked to share work Slack.

Two honest notes for the UI copy: work-profile notifications are shown as such
on the desktop (a badge, from the `secondary_profile` flag), and if the
administrator has blocked notification listeners the switch explains that
OmniBridge cannot see them at all rather than silently showing nothing.

If OmniBridge is installed *in* the work profile it receives nothing whatsoever —
the system ignores listeners running in a work profile — and the settings screen
must say that, since it is otherwise indistinguishable from a bug.

### 5.5 Ongoing and foreground-service notifications

Off by default. These are the persistent "Maps is navigating", "Spotify is
playing", "App is running" entries. They are not events, they update constantly,
they are usually not dismissible, and mirroring them fills the desktop with rows
that cannot be cleared.

Users who want media or navigation on the desktop can turn them on per app.

At the OS level, the same intent is declared in the manifest so the platform's
own listener-filter UI agrees with ours
([00 §1.6](00-RESEARCH-FINDINGS.md)):

```xml
<meta-data android:name="android.service.notification.default_filter_types"
           android:value="conversations|alerting" />
```

`disabled_filter_types` is deliberately **not** declared: it would grey the
types out permanently in the system UI, and a user who genuinely wants their
media notification mirrored should be able to have it.

---

## 6. Lock-screen policy

Two devices, two locks, two independent policies. The mechanism is the same on
both ends, which is the point.

```rust
pub enum LockPolicy { Full, AppOnly, Suppress }
```

| | `Full` | `AppOnly` *(default)* | `Suppress` |
| --- | --- | --- | --- |
| What the peer receives | everything | `app_label` only; `title` and `body` are **empty**, `redacted = true` | nothing |
| Where the reduction happens | — | **at the source, before encoding** | at the source |

### 6.1 Phone locked

`when_source_locked`, default **`AppOnly`**.

The reduction happens at the **source**, so the title and body of a notification
that arrived while the phone was locked never enter a protobuf message, never
touch the socket and never exist on the desktop. This is the strong form: there
is no content to leak because there is no content.

`VISIBILITY_PRIVATE` notifications are reduced the same way even when the phone
is unlocked if the user chooses; `VISIBILITY_SECRET` is never sent in any state
([02 §6.2](02-PROTOCOL-AND-EVENT-MODEL.md)).

The unlock is **not retroactive**. Notifications reduced while locked stay
reduced; they are not re-sent in full when the phone is unlocked. Anything else
would mean holding the withheld content somewhere, which is a history by another
name.

### 6.2 Desktop locked

`when_sink_locked`, default **`AppOnly`**.

A D-Bus client cannot tell GNOME how to render on the lock screen
([00 §2.5](00-RESEARCH-FINDINGS.md)) — the only lever is what goes into the
`Notify` call. So the sink reduces the notification *before* posting it, and
posts a body-less notification naming the app.

Detection, **measured in [POC-NOTIF-02](poc/POC-NOTIF-02.md) and corrected
from what this section originally said**:

* **`org.freedesktop.login1.Session.LockedHint` is authoritative.** It is read
  once at startup and tracked through `PropertiesChanged` on the session
  object — event-driven, so ADR-0014's no-polling rule holds.
* **`org.gnome.ScreenSaver.ActiveChanged` is a hint, and its boolean is
  discarded.** It may be subscribed as an extra prompt to re-read `LockedHint`,
  and nothing more.
* **`org.freedesktop.ScreenSaver` is not used**: on GNOME that name is served by
  the idle-inhibit implementation and refuses `GetActive`, so a detector built
  on it would report "unlocked" forever. Reconfirmed on the host.

`ActiveChanged` was originally described here as "the low-latency signal". It is
not, and it is not a lock signal either — it fails in both directions:

| Measured | `LockedHint` | `ActiveChanged` |
| --- | --- | --- |
| `ScreenSaver.Lock()` — the Super+L path | locked after **218 ms** | active after **884 ms** — a **665 ms window** in which the screen is locked and a sink gated on this signal would still be posting full bodies |
| Screen blanks without locking | correctly stays `no` | reports **active** — content withheld for an unlocked session |

The first row is the one that matters: it is a **fail-open** window, which is
the failure this whole section exists to prevent.

**If lock state cannot be determined, the sink treats the session as locked.** A
privacy control that fails open is not a control.

Notifications already on screen when the session locks are **re-posted reduced**
(via `replaces_id`, so no duplicate appears) rather than left in full. The lock
happens after the banner; the notification list persists on GNOME, and leaving
full content there would defeat the setting for exactly the case it exists for.

### 6.3 Both locked

The two policies compose, and the source's applies first because it is the one
that decides what crosses the wire. Source `AppOnly` plus sink `AppOnly` is
`AppOnly`. Source `Suppress` means nothing is sent, whatever the sink says.

### 6.4 What this does not claim

It does not stop a person reading the *phone's* lock screen, and it does not
change how the desktop renders notifications from other applications. It governs
what OmniBridge transmits and what OmniBridge posts. Stating the boundary is part of
the feature.

---

## 7. Two permissions, and never pretending they are one

This is the part the brief singles out, and it is the part most similar products
get wrong.

```text
┌────────────────────────────────────────────────────────────┐
│ 1. Android OS notification access                          │
│    Granted in Settings, to OmniBridge, for the whole device.  │
│    Lets OmniBridge read notifications AT ALL.                 │
│    Revocable in Settings at any time.                      │
└────────────────────────────────────────────────────────────┘
                            │  necessary, NOT sufficient
                            ▼
┌────────────────────────────────────────────────────────────┐
│ 2. OmniBridge peer grant: notifications.v1 for THIS computer  │
│    Granted per paired device, in OmniBridge.                  │
│    Lets that ONE computer receive them.                    │
└────────────────────────────────────────────────────────────┘
                            │  and then
                            ▼
┌────────────────────────────────────────────────────────────┐
│ 3. Which apps                                              │
└────────────────────────────────────────────────────────────┘
```

Rules for the flow:

* Completing step 1 grants **nothing** to any peer. The UI must never present OS
  notification access as "turning on notification sharing".
* Step 2 is per device and is what `omnibridge grant <device> notifications.v1`
  and the Android device-card switch write.
* Revoking step 1 revokes everything downstream at once
  ([02 §13](02-PROTOCOL-AND-EVENT-MODEL.md)): the role announcement drops
  `SOURCE` and every mirror closes.
* Revoking step 2 for one device affects only that device.
* OmniBridge links to step 1 with `ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS`
  plus `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME`
  ([00 §1.10](00-RESEARCH-FINDINGS.md)), which lands on OmniBridge's own switch
  rather than a list of every app on the device.

---

## 8. UX specification

### 8.1 Android — Settings → Notifications

```text
┌─ Notifications ──────────────────────────────────────────┐
│                                                          │
│  Notification access                        [ Granted ]  │
│  Android needs to allow OmniBridge to read notifications    │
│  before any of this works.                → Open Settings│
│                                                          │
│  ────────────────────────────────────────────────────    │
│                                                          │
│  Share with                                              │
│                                                          │
│   ▸ fedora-desktop                          [  ON  ]     │
│     Sharing 6 apps · dismissals not synced                │
│                                                          │
│   ▸ work-laptop                             [  OFF ]     │
│     Not sharing notifications                            │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

and, per device:

```text
┌─ fedora-desktop · Notifications ─────────────────────────┐
│                                                          │
│  Share notifications with this computer     [  ON  ]     │
│                                                          │
│  Applications                                            │
│   6 of 74 apps                                    →      │
│   3 new apps are not being shared                        │
│                                                          │
│  Sync dismissals                            [  OFF ]     │
│   Off by default. When on, dismissing a notification on  │
│   the computer dismisses it here too.                    │
│                                                          │
│  When this phone is locked                               │
│   ( ) Share everything                                   │
│   (•) Share the app name only                            │
│   ( ) Share nothing                                      │
│                                                          │
│  Work profile notifications                 [  OFF ]     │
│  Ongoing notifications                      [  OFF ]     │
│   Music, navigation and "app is running" notices.        │
│                                                          │
│  ────────────────────────────────────────────────────    │
│  OmniBridge never shares its own notifications, and never   │
│  shares notifications an app marks as private to the     │
│  lock screen.                                            │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

Notes that are requirements, not decoration:

* The app picker is the **last step of turning the switch on**, not a screen
  reached later (§5.1).
* The "N new apps" line is passive text, never a notification.
* "Sync dismissals" is greyed with an explanation when the peer has not
  advertised `DISMISS_REPORTER` — the setting reflects what the other device can
  actually do ([02 §3](02-PROTOCOL-AND-EVENT-MODEL.md)).
* **There is no notification history screen**, and none that says "coming soon".
* **Sync dismissals is off until a person turns it on** (§3.1). The picker and
  this switch are both part of the enabling flow, so neither is a setting a user
  has to go looking for.

#### The permission copy is a security control

It should be reviewed as one, not as marketing. Two rules bind it:

* **It must not offer Android's OTP protection as reassurance — on any device.**
  [POC-NOTIF-01](poc/POC-NOTIF-01.md) measured the certification target
  delivering six OTP-shaped notifications, across three vectors, to an untrusted
  listener **entirely unredacted**, with the platform flag enabled and Google's
  classifier bound. Copy of the form "Android hides verification codes
  automatically" would be false on the one device OmniBridge certifies against.
* **It must not imply that OmniBridge detects sensitive content.** It does not, by
  design ([ADR-0015 §5](../../adr/ADR-0015-notification-access.md)). The honest
  statement is the one the product can keep: *what you choose to share is
  shared, in full, with the computer you named* — which is exactly why the app
  picker is deny-by-default and sits inside the enabling flow.

### 8.2 Linux — CLI

Following the existing `omnibridge clipboard status` pattern, which exists so a
person can find out what will and will not work *before* turning something on:

```console
$ omnibridge notifications status
notifications.v1
  role                sink, dismiss-reporter
  server              gnome-shell 50.4 (GNOME), spec 1.2
  capabilities        actions, body, body-markup, icon-static, persistence, sound
  lock detection      logind LockedHint (+ org.gnome.ScreenSaver.ActiveChanged)
  session             unlocked
  mirrors             4 active

  galaxy-tab-s10       granted    mirroring    dismiss-sync on
  work-phone           not granted

$ omnibridge notifications policy galaxy-tab-s10 --when-locked app-only
$ omnibridge grant galaxy-tab-s10 notifications.v1
$ omnibridge notifications clear galaxy-tab-s10      # close every mirror from this peer
```

`omnibridge notifications status` **never lists notification content, titles, or
app names of active mirrors.** It reports counts. A status command that printed
the last four notifications would put them in a terminal scrollback and in every
bug report that pastes it.

### 8.3 Linux — GUI

The device card gains a `notifications.v1` row beside `clipboard.v1`'s, with the
grant switch, the lock policy, and a "clear all mirrors from this device"
action. No list of received notifications, ever: the desktop's own notification
list is where they live, and duplicating it inside OmniBridge would create the
history this design forbids.

---

## 9. Why Linux → Android is not in v1

Not scope-trimming — a platform constraint, and the same one ADR-0014 documents
for the clipboard.

There is no supported way for an ordinary Linux client to observe other
applications' notifications ([00 §2.7](00-RESEARCH-FINDINGS.md)). Only one
process may own `org.freedesktop.Notifications`, and taking it means *becoming*
the desktop's notification server. The alternative — monitoring the session bus
for other applications' method calls — needs bus policy that amounts to "let
OmniBridge read every notification on this machine before the shell does", which is
not a thing this project should ask a user to configure.

So: Linux is a sink. The protocol does not encode that assumption anywhere
([02 §3](02-PROTOCOL-AND-EVENT-MODEL.md)), because Windows can source
([00 §3](00-RESEARCH-FINDINGS.md)) and the day a Linux mechanism appears the
capability should need a backend, not a redesign.

Android is not a sink in v1 either, for a plainer reason: showing a computer's
notifications on a phone is a much weaker product than the reverse, and it would
need its own channel, its own grouping and its own dismissal path for a use case
nobody has asked for. `NOTIFICATION_ROLE_SINK` exists in the schema so that this
is a later decision rather than a later migration.

---

## 10. Internationalization

**Notification content is opaque user text and is never translated,
transliterated, normalised, spell-corrected or re-encoded by OmniBridge.** It is
carried byte-for-byte within the encoding rules of
[02 §11.3](02-PROTOCOL-AND-EVENT-MODEL.md) — the same guarantee `clipboard.v1`
makes about clipboard text, and for the same reason: a "helpful" rewrite would
corrupt exactly the content a user is least able to notice.

That includes `app_label`, which is resolved on the source *in the source
device's locale*. A phone in Portuguese sends "Definições"; the desktop shows
"Definições" even if it runs in English. Re-resolving app names locally is
impossible anyway — the desktop has no package database — and guessing would be
worse than being consistent with the phone.

OmniBridge's **own** UI strings follow the existing localization strategy and must
be externalised, not inlined:

| Surface | Where |
| --- | --- |
| Android settings copy, app-picker labels, lock-policy option labels | `android/app/src/main/res/values/strings.xml` |
| Permission explanations (both permissions of §7) | same |
| Error and empty states ("notification access not granted", "installed in a work profile", "administrator has blocked this") | same |
| Desktop CLI output, GUI labels, policy names | the desktop's existing string handling |
| `NotificationOutcome` values shown to a person | mapped to localized strings at the UI edge; the enum itself stays English on the wire |

Two specific rules:

* Numbers in OmniBridge's own strings ("6 of 74 apps") use the platform's locale
  formatting; they are never concatenated into a sentence by hand.
* An `app_label` is rendered **as data**, never interpolated into a translatable
  format string in a way that would let it change a sentence's grammar or, worse,
  forge UI. It is control-character-stripped and length-capped first
  ([02 §11.3](02-PROTOCOL-AND-EVENT-MODEL.md)).

---

## 11. Logging and persistence

Inherited verbatim from `clipboard.v1`, which sets the strictest rule in the
project, and extended to a second content type.

* **No notification content reaches a log, at any level, on either platform.**
  Not the title, not the body, not the app label. What is logged is the peer's
  short fingerprint, the notification-id prefix, the content-hash prefix, the
  byte count and the outcome — enough to correlate two devices' logs, useless to
  anyone reading one.
* The wire types carry **hand-written** `Debug`/`toString` implementations. A
  derived one would put content into every `tracing` field using `?value`, every
  panic message and every test failure.
* **Nothing is persisted.** No content in `state.json`, the trust store, a
  database, preferences, a crash report or telemetry. What *is* persisted is the
  per-peer policy and the app allow-list — settings, not content.
* The app allow-list is a list of package names. It is settings, but it is also
  a description of what the user has installed, so it lives in the same
  0600-in-0700 discipline as the rest of the store and is never sent to a peer.
* Caches hold ids, digests and monotonic timestamps only
  ([02 §8](02-PROTOCOL-AND-EVENT-MODEL.md)).
* This is enforced by a test, not by discipline: N3's exit criteria include a
  `logging.rs` suite that drives every flow under a `TRACE` subscriber with
  canary strings and fails if a canary appears — the same suite shape
  `capabilities/clipboard/tests/logging.rs` already has
  ([05 §N3](05-IMPLEMENTATION-PLAN.md)).
