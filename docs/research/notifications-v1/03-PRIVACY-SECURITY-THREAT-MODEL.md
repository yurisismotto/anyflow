# 03 — `notifications.v1` privacy, security and threat model

| Field | Value |
| --- | --- |
| **Title** | What can go wrong, and what stops it |
| **Status** | Specification — proposed, not implemented |
| **Last reviewed** | 2026-09-08 |
| **Scope** | `notifications.v1` only. Extends, and does not replace, [THREAT_MODEL.md](../../security/THREAT_MODEL.md) |
| **Related** | [00](00-RESEARCH-FINDINGS.md), [01](01-FUNCTIONAL-SPECIFICATION.md), [02](02-PROTOCOL-AND-EVENT-MODEL.md), [06](06-OPEN-QUESTIONS-AND-POCS.md) |

---

## 1. What we are protecting, and why this is the worst of it

`notifications.v1` handles the most sensitive data AnyFlow has ever carried.
Not "sensitive" as a category label — sensitive in the specific sense that the
notification shade of an ordinary phone routinely contains, without the person
ever choosing to put it there:

* one-time passcodes and 2FA prompts;
* bank transaction alerts with amounts and merchants;
* message previews from every conversation the person has;
* medical appointment and medication reminders;
* delivery addresses, ride pickup locations, boarding passes;
* the *existence* and *timing* of every one of these, which is metadata that
  identifies a person's routine even when the content is redacted;
* an implicit list of every application the person has installed and uses.

The clipboard was previously the high-water mark, and
[THREAT_MODEL.md T10](../../security/THREAT_MODEL.md) says so. The clipboard has
one property this does not: **the user put it there deliberately.** A person
copies a password knowing they copied it. Nobody *decides* to receive a bank
alert. Every control in this document exists because the person whose data this
is was not present at the moment it was created.

### Trust-boundary additions

| New asset | Where it lives | Protection |
| --- | --- | --- |
| Notification content in flight | Memory only, on both ends | TLS 1.3 mutual + SPKI pinned; grant re-checked per message |
| The mirror table | Sink memory | Dropped on disconnect grace expiry; never written to disk |
| `notification_id` ↔ platform key map | Source memory | Rebuilt from `getActiveNotifications()`; never written to disk |
| `device_notification_secret` | Source, on disk | 32 CSPRNG bytes, 0600-in-0700 / Android encrypted store. Not a credential; authenticates nothing |
| The per-app allow-list | Both, on disk | Settings, but it describes what the user has installed → never sent to a peer |

### Two new attacker positions

The existing A1–A6 all apply unchanged. Two are worth restating because their
consequences change:

* **A6 (a previously paired device now hostile)** is now materially worse. The
  blast radius of a compromised, `notifications.v1`-granted peer is *every
  notification the filter allows, continuously, silently*. Before this
  capability the worst a compromised peer could do was read a battery
  percentage, receive a clipboard the user chose to send, and offer files a
  human had to accept. This is the first capability whose data flows without a
  per-event human decision.
* **A5 (brief physical access to the desktop)** becomes a notification-reading
  position. Someone who walks past an unlocked desk now reads the phone's
  notifications too. §T-N06.

---

## 2. Threats

Numbering continues the main threat model, which ends at T26.

### T-N01 — A malicious paired peer harvests notifications

*A6.* A granted peer receives every allowed notification for as long as the
grant lasts, and can store, forward or index them. AnyFlow cannot see what a
peer does with data it was granted.

**Mitigation.** Layered, and all of it consent rather than cleverness:
`notifications.v1` is never in `auto_grant`; the grant is per peer and explicit;
the per-app allow-list starts empty ([01 §5.1](01-FUNCTIONAL-SPECIFICATION.md));
work-profile and ongoing notifications need separate switches; the lock policy
reduces content at the source; `SECRET` never leaves at all. Revocation is
immediate and closes every mirror.

**Residual risk. High, and irreducible.** A peer you granted can read what you
granted it. This is the nature of the feature, not a gap in it. The mitigation
that actually matters is that the grant is *narrow by default and easy to
revoke*, and that the product never encourages a blanket yes. This risk must be
stated in the grant UI in those words, the way the clipboard's `auto_send`
warning already is.

### T-N02 — OTP and 2FA code leakage

*A6, A5, A1-by-observation.* A one-time code mirrored to a desktop is a
one-time code on a second screen, in a second room, on a second attack surface.

**Mitigation.**

* Android 15+ *may* redact OTP-classified notifications before AnyFlow ever sees
  them, because AnyFlow is an *untrusted* listener with no CDM association
  ([00 §1.7](00-RESEARCH-FINDINGS.md)). Where this fires, the content never
  reaches us. **It does not fire on the certification target.**
  [POC-NOTIF-01](poc/POC-NOTIF-01.md) posted six OTP-shaped notifications across
  three vectors to an untrusted listener on SM-X620 / Android 16 / One UI 8.0
  and every one arrived verbatim — with
  `redact_sensitive_notifications_from_untrusted_listeners=true` and Google's
  classifier bound, but issuing no adjustments at all: not one of the 93
  notification records on the device was classified sensitive. **This mitigation
  must therefore be treated as absent**, which is what the design already
  assumed.
* Where it does not fire, the per-app filter is the control: banking and
  authenticator apps are simply not in the allow-list unless the user puts them
  there, and the picker should surface that choice clearly for apps the user is
  most likely to regret.
* The lock policy means a code arriving while the phone is locked is not
  transmitted in full by default.

**AnyFlow does not attempt to detect OTPs itself.** No regex, no keyword list,
no "looks like a code" heuristic. The reasoning is
[THREAT_MODEL.md T10](../../security/THREAT_MODEL.md)'s, unchanged: a guess
dressed up as a security control is worse than an honest boundary, because the
user trusts it and it is wrong in cases neither of us can predict.

**Residual risk. High, and no longer speculative.** Redaction is **measured
absent** on the certification target ([POC-NOTIF-01](poc/POC-NOTIF-01.md)); the
flag *is* enabled, so a future build that starts classifying could turn this on
without warning, and the design must not come to depend on that either. What
this changes:

* the **per-app allow-list is the only effective control**, which is an argument
  for [OQ-02](06-OPEN-QUESTIONS-AND-POCS.md)'s deny-by-default rather than
  against it;
* the default `AppOnly` source lock policy is the second line — a code arriving
  on a locked phone is not transmitted in full;
* **no product copy may offer platform redaction as reassurance**
  ([01 §8.1](01-FUNCTIONAL-SPECIFICATION.md)).

A person who puts their authenticator app in the allow-list has chosen to mirror
their codes. AnyFlow's obligation is to make that a choice rather than a
surprise, and to be honest that it is not undone by the platform.

### T-N03 — Adopting CompanionDeviceManager would silently remove a protection

*Design-time threat, and the subtlest one here.*

A CDM association is the platform-sanctioned way to justify a companion app's
background behaviour, and it is attractive for reasons unrelated to
notifications. But `isAppTrustedNotificationListenerService` treats a non-revoked
CDM association as **trust**, which switches off sensitive-content redaction
([00 §1.7](00-RESEARCH-FINDINGS.md)).

So a change made for background-execution reasons, in a different sprint, by
someone who never read this document, would quietly start delivering unredacted
OTPs to AnyFlow.

**Mitigation — now a decision, not a note.**
[ADR-0015 §10](../../adr/ADR-0015-notification-access.md) records that
**`notifications.v1` does not adopt CDM**, and that adopting it later requires a
separate security ADR *plus* a re-run of [POC-NOTIF-01](poc/POC-NOTIF-01.md)
under a live association — verified on hardware, not reasoned about. If CDM is
ever adopted, `notifications.v1` must compensate; at minimum by defaulting
authenticator and banking categories out of the picker and saying why.
[OQ-04](06-OPEN-QUESTIONS-AND-POCS.md) is **RESOLVED FOR V1**.

**A correction to this threat's framing.** [POC-NOTIF-01](poc/POC-NOTIF-01.md)
found the certification device's trusted-listener set non-empty
(`mTrustedListenerUids={1000, 10064, 10135}`) while its CDM association table is
**empty**. So a CDM association confers listener trust — the claim this threat
rests on, and it is unaffected — but it is not the *only* route to trust, and
the inverse ("untrusted implies no CDM") must not be stated.

**Residual risk.** Organisational, not technical, and now lower: the constraint
lives in an Accepted ADR that a CDM proposal has to argue against, rather than
in a research note a future sprint might not read.

### T-N04 — Notification spoofing / forged application identity

*A6.* `app_id` and `app_label` arrive from the peer. A hostile source can claim
any package name and any label, and post a mirror that reads
`Your bank · Confirm the transfer by replying YES`.

**Mitigation.** Bounded by what a mirror *is*: a mirrored notification carries
no actions, no buttons, no links and no `PendingIntent`
([02 §6.6](02-PROTOCOL-AND-EVENT-MODEL.md)), so a spoofed one can lie but cannot
be *clicked into* doing anything. Beyond that: `app_label` is
control-character-stripped and length-capped before display
([THREAT_MODEL.md T17](../../security/THREAT_MODEL.md) discipline), the body is
markup-escaped before reaching a `body-markup`-capable server
([02 §11.3](02-PROTOCOL-AND-EVENT-MODEL.md)), and the sink attributes every
mirror to the **pinned peer**, not to the claimed origin — the desktop always
shows which device a notification came from.

**Residual risk.** A compromised paired phone can display convincing false text
on your desktop. It could also do so by posting a real notification on itself,
which the user would believe just as readily. Mirroring does not create the
problem; it moves it one screen over.

### T-N05 — Dismiss abuse, and dismissal as a foothold for remote control

*A6.* A hostile desktop dismisses the user's notifications — an annoyance, and
a denial of alerts that could matter (a fraud alert, a 2FA prompt the user was
waiting for).

**Mitigation.** `DismissRequest` carries a notification id and an origin device
id and **nothing else** ([02 §4.4](02-PROTOCOL-AND-EVENT-MODEL.md)). There is no
action index, no intent, no free text, no payload — so there is no field to
widen into remote action execution. It maps to exactly one platform call,
`cancelNotification(key)`.

Further bounds: only notifications *this peer was sent* can be dismissed by it
(the id is scoped to `(peer_fingerprint, notification_id)` at both ends,
[02 §5.3](02-PROTOCOL-AND-EVENT-MODEL.md)); non-dismissible notifications are
refused at the source with `NOT_DISMISSIBLE`; `allow_dismiss_sync` can be turned
off per peer; and dismissal requests are rate-limited on the same buckets as
everything else.

**No `PendingIntent` is ever executed, resolved, stored or transmitted.** This
is the single hardest line in the capability, and it is enforced by absence:
there is no field for one.

**Residual risk.** A granted, hostile peer can clear notifications you have not
read. It cannot do anything with them, and it cannot reach the app that posted
them.

### T-N06 — Lock-screen and shoulder-surfing leakage

*A5.* The desktop is locked or unattended; mirrors are on the screen.

**Mitigation.** `when_sink_locked` defaults to `AppOnly` and the reduction
happens before the notification is posted, because a D-Bus client cannot
influence lock-screen rendering ([00 §2.5](00-RESEARCH-FINDINGS.md)).
Notifications already displayed when the session locks are re-posted reduced via
`replaces_id`, so the setting is not defeated by timing. Lock state comes from
`logind LockedHint` plus `org.gnome.ScreenSaver.ActiveChanged`; **unknown is
treated as locked**.

**Residual risk.** Between the lock event and our re-post there is a window of
milliseconds. And a user who sets `Full` has chosen this. `org.freedesktop.ScreenSaver`
would have made the control fail open — [POC-NOTIF-02](06-OPEN-QUESTIONS-AND-POCS.md)
verifies the replacement actually reports a lock on the certification target.

### T-N07 — Work-profile / organisational data leakage

*A6, and a compliance surface.* A listener in the personal profile sees
work-profile notifications by default ([00 §1.8](00-RESEARCH-FINDINGS.md)).
Mirroring them to a personal laptop may breach an employer's policy and is
certainly not something to do by accident.

**Mitigation.** `include_work_profile` is a separate switch, **off by default**,
independent of the app allow-list. Work-profile mirrors are badged as such on
the desktop via the `secondary_profile` flag. Where the administrator has
disabled cross-profile listeners, the platform enforces it and AnyFlow reports
that it cannot see them rather than showing an empty list.

**Residual risk.** A user can turn it on. That is their decision and their
employer's policy; AnyFlow's job is to make it a decision rather than a default.

### T-N08 — Notification flooding and desktop spam

*A6, and also a *buggy* peer, which is more likely.* A source posting rapid
updates — or a hostile one doing it deliberately — could make the desktop
unusable, exhaust memory, or drown the session's outbound queue.

**Mitigation.** [02 §10](02-PROTOCOL-AND-EVENT-MODEL.md): per-identity coalescing
into a single pending slot (a 60 fps progress bar becomes one message), a 500 ms
minimum update interval per identity, a token bucket per peer, a cap of 200
active mirrors per peer, a 100-entry snapshot cap, and an 8 KiB message ceiling.
Overflow drops the **oldest non-terminal upsert** — removals are never dropped,
so a flood cannot leave a permanent mirror behind. Everything answers
`RATE_LIMITED` and nothing is fatal
([ADR-0008](../../adr/ADR-0008-capability-architecture.md)).

**Residual risk.** A peer can still consume its allowance. The session's
liveness probe eventually ends a session whose peer stops reading, which is the
same bound `clipboard.v1` relies on.

### T-N09 — Escalation from mirroring to remote control

*A6, design-time.* The natural next features — reply, action buttons, "open on
phone" — all require executing a `PendingIntent` on the source, which is
arbitrary code execution scoped by the notifying app rather than by AnyFlow.

**Mitigation.** v1 carries no `PendingIntent`, no action list, no `RemoteInput`
and no `RemoteViews`, and the schema has no field that could hold one
([02 §6.6](02-PROTOCOL-AND-EVENT-MODEL.md)). Adding them is not a field addition
but a **new capability id** with its own grant, its own threat model and its own
human-in-the-loop story — which is exactly the property ADR-0008's versioned ids
were designed to give.

**Residual risk.** None in v1, by construction. The risk is entirely that a
future sprint adds it casually; naming it here is the mitigation.

### T-N10 — Replay, duplication and stale mirrors

*A1, A3, A6.* Beneath the transport's own replay protection (`sequence`,
`message_id`, both scoped to one connection), the capability needs its own
idempotence with a longer lifetime.

**Mitigation.** `notification_id` is stable and derived, so a replayed upsert
updates a mirror in place instead of creating one
([02 §5](02-PROTOCOL-AND-EVENT-MODEL.md)); `UpsertDedup` on
`(peer, id, content_hash)` answers `DUPLICATE` and does nothing twice; removals
and dismissals are idempotent and answer `UNKNOWN_NOTIFICATION` when there is
nothing to act on; the reconnect snapshot converges the sink onto the source's
current state and removes anything not named in it
([02 §7](02-PROTOCOL-AND-EVENT-MODEL.md)); `timestamp` is informational and is
never an input to authorization, ordering, expiry or de-duplication.

**Residual risk.** A replayed *removal* closes a mirror that the source still
has. The next snapshot restores it, and the failure direction — showing less —
is the safe one.

### T-N11 — Leakage through logs and crash reports

*A3, A4.* This is the threat most likely to undo every other control here, and
the project has been bitten by the general shape of it before, which is why
`clipboard.v1` has a dedicated test suite for it.

**Mitigation.** [01 §11](01-FUNCTIONAL-SPECIFICATION.md): no notification
content at any log level on either platform; hand-written `Debug`/`toString` on
every wire type carrying content; all diagnostics through the existing
`redact.rs` / `Redact.kt`; ids logged as prefixes only; a `logging.rs` suite that
runs every flow under a **TRACE** subscriber with canary strings and fails if a
canary appears. `RUST_LOG=trace` is what someone runs when something is wrong,
which is the worst possible moment to write a 2FA code into a file they are
about to attach to a bug report.

Nothing built by this capability reaches a shell, an `argv` or
`/proc/<pid>/cmdline`: the D-Bus call takes strings as arguments over the bus,
not through a command line.

**Residual risk.** Unaudited third-party crash reporters would defeat this.
AnyFlow ships none.

### T-N12 — Persistence: a history nobody asked for

*A4, A5.* A notification history is the single most damaging artefact this
feature could accidentally create — a searchable record of a person's messages,
codes and alerts, on disk, surviving a reboot.

**Mitigation.** Structural. There is no history, and the absence is the
mechanism: no content in `state.json`, the trust store, a database, preferences,
telemetry or a crash report; the mirror table and the id map live in memory and
die with the process; the reconnect snapshot is *current active state*, is
discarded as soon as it is applied, and is bounded and filtered like any live
notification ([02 §7.3](02-PROTOCOL-AND-EVENT-MODEL.md) draws the line
explicitly). The GUI shows no received-notification list, so there is nothing to
back with storage later.

**Residual risk.** The desktop's *own* notification server persists what we post
to it — GNOME advertises `persistence` ([00 §2.1](00-RESEARCH-FINDINGS.md)), so
mirrors sit in the GNOME notification list until cleared. That is the platform's
notification list behaving normally, it is visible to the user, and
`anyflow notifications clear <device>` closes them. It is worth stating plainly
in the UI rather than leaving as a surprise.

### T-N13 — Revoked peer keeps receiving

*A6.* Revocation that only takes effect at the next handshake would leave a
hostile peer reading notifications for as long as it keeps the socket open.

**Mitigation.** Inherited and unchanged: the grant is re-checked from the trust
store on **every message**, never from a set captured at handshake time; `anyflow
unpair` clears grants and tears down the live session
([THREAT_MODEL.md T5](../../security/THREAT_MODEL.md)). `notifications.v1` adds
one requirement of its own — revocation must also **close every mirror already
displayed** for that peer, because a withdrawn grant that leaves forty
notifications on a screen has withdrawn nothing.

**Residual risk.** Until the user revokes, a compromised peer reads what it was
granted. There is no automatic detection of a compromised peer.

### T-N14 — Loops and amplification

*Design-time, and a real risk given AnyFlow posts its own notifications.*

**Mitigation.** Four independent rules, in
[02 §9.4](02-PROTOCOL-AND-EVENT-MODEL.md): AnyFlow's own package is dropped
before any other check and there is no setting to re-enable it; there is no code
path from an inbound upsert to an outbound one (no relay, enforced by absence,
exactly as in [CLIPBOARD.md](../../architecture/CLIPBOARD.md)); every message
carries `origin_device_id` so a future bidirectional platform can recognise its
own output; and sink-created mirrors are marked with AnyFlow's `desktop-entry`
hint. Dismissal echoes are suppressed single-use and short-lived
([02 §9.3](02-PROTOCOL-AND-EVENT-MODEL.md)).

**Residual risk.** None identified for the v1 topology. The rules exist mainly
to keep it that way when Linux gains a source.

### T-N15 — Oversized or malformed events

*A3, A6.* Bounded before anything is allocated or copied: 8 KiB per message
inside a 64 KiB frame checked before allocation; `notification_id` exactly 16
bytes or refused-and-unanswered; `content_hash` exactly 32 bytes or absent;
protobuf refuses non-UTF-8 `string` fields outright; NUL is refused explicitly on
both platforms; control characters are stripped for display; body markup is
escaped; unknown enum values resolve to the **most conservative** option, so an
unknown privacy class is treated as `SECRET` rather than as public
([02 §11.3](02-PROTOCOL-AND-EVENT-MODEL.md)).

A refusal is **not fatal**: one capability misbehaving must not cost the user
the session.

**Residual risk.** None beyond the general risk of a parser bug, which the
fuzz-style malformed-payload tests in [05 §N1](05-IMPLEMENTATION-PLAN.md) target.

### T-N16 — The Android permission itself

*Not an attack — a responsibility.* `BIND_NOTIFICATION_LISTENER_SERVICE` is
among the most powerful things an Android app can hold, and the project had
until 2026-09-08 advertised its **absence** as a feature
([00 §1.1](00-RESEARCH-FINDINGS.md)).

> **Resolved by [ADR-0015](../../adr/ADR-0015-notification-access.md)** (Accepted,
> 2026-09-08), which is the canonical record for the decision, the security
> contract it is granted under, and the permanent rule that replaced *"and it
> must stay that way"*. This section describes the residual responsibility, not
> the decision.

**Mitigation.** The permission is held **by the system, not by AnyFlow** — it is
what stops other apps binding our service, exactly as
`BIND_QUICK_SETTINGS_TILE` does for the existing tile. Access is granted by the
user in Settings, revocable there at any time, and grants **nothing** to any
peer on its own ([01 §7](01-FUNCTIONAL-SPECIFICATION.md)). The listener is not
even bound unless a granted peer is connected
([01 §4](01-FUNCTIONAL-SPECIFICATION.md)), so an installed-but-unused AnyFlow
reads no notifications at all.

What does **not** change: no accessibility service, no default-IME request, no
`QUERY_ALL_PACKAGES`, no `SYSTEM_ALERT_WINDOW`, no `READ_LOGS`, no root, no
hidden APIs, no reflection. The README's *"No root, no accessibility service, no
ADB, no hidden permissions"* survives intact.

**Residual risk.** The user must trust AnyFlow with notification access. That
trust is the feature. It is repaid by the code being open, by nothing leaving
the LAN, and by the app being unable to read anything while no peer is
connected — and it must be earned in the permission copy, not assumed.

---

## 3. Security requirements checklist

Restating the brief's §7 list against the design, so a reviewer can check each
one against a section rather than a paragraph:

| Requirement | Where |
| --- | --- |
| Explicit capability grant per peer | [01 §3](01-FUNCTIONAL-SPECIFICATION.md); never in `auto_grant` |
| Notification access granted explicitly at the Android OS level | [01 §7](01-FUNCTIONAL-SPECIFICATION.md) step 1 |
| AnyFlow permission separately required | [01 §7](01-FUNCTIONAL-SPECIFICATION.md) step 2 — and step 1 grants nothing downstream |
| No notification content in logs | [01 §11](01-FUNCTIONAL-SPECIFICATION.md), T-N11, proved by `logging.rs` |
| No notification history | [02 §7.3](02-PROTOCOL-AND-EVENT-MODEL.md), T-N12 |
| No notification content in `state.json` | [01 §11](01-FUNCTIONAL-SPECIFICATION.md), T-N12 |
| No telemetry | Project-wide; nothing added here |
| TLS 1.3 existing session | [02 §1](02-PROTOCOL-AND-EVENT-MODEL.md); no new socket |
| SPKI-pinned peer | Unchanged; identity decides everything ([02 §5.3](02-PROTOCOL-AND-EVENT-MODEL.md)) |
| Revoked peer stops receiving immediately | T-N13, plus mirror closure |
| Deny by default | Grant, app filter, work profile, ongoing, lock policy — every one starts closed |

## 4. Assumptions

Additional to the main threat model's five:

1. The Android notification listener API delivers only what the platform
   intends. We do not attempt to see more than it gives us.
2. The user's desktop session is not already being observed. Any process in a
   Linux session can talk to `org.freedesktop.Notifications`; `notifications.v1`
   neither worsens that nor can fix it — the same honest caveat the clipboard
   makes about a Linux session clipboard.
3. Platform OTP redaction, where it exists, is a **bonus and not a control**.
   The design assumes it is absent — and on the certification target it
   **is** absent, measured ([POC-NOTIF-01](poc/POC-NOTIF-01.md)). This is no
   longer a conservative assumption; it is the observed behaviour.
4. The person granting notification access understands they are granting access
   to notifications. The permission copy in
   [01 §8.1](01-FUNCTIONAL-SPECIFICATION.md) is load-bearing for this and should
   be reviewed as a security control, not as marketing.

## 5. Verification

Security behaviour is proved by tests that fail loudly, not by prose. The suite
`notifications.v1` must add, mirroring `CLIP-SEC-01…18`:

| id | Case |
| --- | --- |
| NOTIF-SEC-01 | An ungranted peer receives nothing and its upserts are refused |
| NOTIF-SEC-02 | A revoked peer stops immediately **and its mirrors are closed** |
| NOTIF-SEC-03 | A peer that never announced `SINK` is never sent an upsert |
| NOTIF-SEC-04 | A peer cannot dismiss a notification it was never sent |
| NOTIF-SEC-05 | A peer cannot dismiss another peer's notification |
| NOTIF-SEC-06 | A non-dismissible notification is refused with `NOT_DISMISSIBLE` |
| NOTIF-SEC-07 | A spoofed `origin_device_id` changes nothing |
| NOTIF-SEC-08 | A replayed upsert answers `DUPLICATE` and changes nothing |
| NOTIF-SEC-09 | A cross-peer-replayed `notification_id` cannot touch another peer's mirror |
| NOTIF-SEC-10 | `VISIBILITY_SECRET` is never transmitted, in any policy |
| NOTIF-SEC-11 | AnyFlow's own package is never sourced |
| NOTIF-SEC-12 | No relay: peer A's notification never reaches peer B |
| NOTIF-SEC-13 | An app not in the allow-list is never transmitted |
| NOTIF-SEC-14 | Work-profile notifications are not transmitted with the switch off |
| NOTIF-SEC-15 | Locked source transmits `app_label` only; title and body are absent **from the encoded bytes** |
| NOTIF-SEC-16 | Locked sink posts no body; and unknown lock state behaves as locked |
| NOTIF-SEC-17 | Oversized text is refused, never truncated by the receiver |
| NOTIF-SEC-18 | NUL, invalid UTF-8 and malformed protobuf are refused non-fatally |
| NOTIF-SEC-19 | Unknown enum values resolve conservatively (unknown privacy ⇒ not displayed) |
| NOTIF-SEC-20 | A peer cannot widen its own policy — no message exists that writes one |
| NOTIF-SEC-21 | Caches are bounded by count and by age |
| NOTIF-SEC-22 | A flood is rate-limited and **no removal is dropped** |
| NOTIF-SEC-23 | Dismissal echo is suppressed to the requester and delivered to others |
| NOTIF-SEC-24 | Reconnect converges without duplicates and removes stale mirrors |
| NOTIF-SEC-25 | No notification content in logs at TRACE, with canaries |
| NOTIF-SEC-26 | No notification content in `state.json` or the trust store after a full flow |
| NOTIF-SEC-27 | Body markup is escaped before reaching a `body-markup` server |

Cross-language contracts to pin on both sides, the way `PairingProofTest` and
`StreamAuthTest` already pin theirs: the `notification_id` derivation vector, the
`content_hash` construction, the `group_id` digest, and the text rules (NUL,
UTF-8 boundary truncation, control-character stripping).
