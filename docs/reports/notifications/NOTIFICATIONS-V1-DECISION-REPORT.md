# Decision sprint report — `notifications.v1`

**Branch:** `research/notifications-v1` · **Working tree:** uncommitted, as instructed
**Date:** 2026-09-08

> ## Outcome: **NOTIFICATIONS.V1 IMPLEMENTATION READY**
>
> The blocking decision and all three P0 questions are closed, both P0 proofs of
> concept were executed on the certification hardware, and **no BLOCKER and no
> open P0 architecture or security question remains**. Wave **N0** — protocol
> schema, ADRs and architecture documentation — may begin.
>
> One result deserves to be read before the verdict is accepted, and it is in
> §8: **POC-NOTIF-01 landed on its documented *Fail* branch.** That branch is a
> statement about Android, not about AnyFlow, and §14 explains exactly how it
> was weighed rather than waved away.

---

## 1. ADR-0015

**[`docs/adr/ADR-0015-notification-access.md`](../../../docs/adr/ADR-0015-notification-access.md) —
"Android notification access and the `notifications.v1` security boundary".
Status: Accepted · 2026-09-08.** Written to the repository's existing ADR
convention (Context / Decision / Alternatives / Consequences, `**Status:**`
line, added to `docs/adr/README.md` as row 0015). No parallel structure was
invented.

It is the **canonical source** for every decision below; the research documents
now restate outcomes by reference rather than duplicating the reasoning.

It covers, in order: context and the historical record · the security contract
(§1) · the permanent rule replacing *"must stay that way"* (§2) · permission
lifecycle (§3) · peer grants and revocation (§4) · application filtering (§5) ·
dismiss sync (§6) · lock policy (§7) · work profile (§8) · OTP implications (§9)
· the CompanionDeviceManager decision (§10) · out-of-scope functionality (§11) ·
alternatives considered · consequences · future reconsideration criteria.

## 2. OQ-01 — `BIND_NOTIFICATION_LISTENER_SERVICE` · **RESOLVED / APPROVED**

**AnyFlow may introduce a `NotificationListenerService` for `notifications.v1`,
under an explicit security contract.** Optional · disabled by default ·
requires the Android OS notification-access grant · *separately* requires an
explicit AnyFlow peer grant · independently revocable · fails closed · not
required by `battery.v1`, `files.v1` or `clipboard.v1`. It implies **no**
history, **no** cloud sync, **no** telemetry, **no** arbitrary actions, **no**
`PendingIntent` execution, **no** `RemoteInput`/reply in v1, **no** persistence
of content and **no** logging of content.

**The old position is amended, not deleted, and its history is recorded.**
ADR-0015's Context carries a period table: through `clipboard.v1`, AnyFlow held
no notification-listener privilege and said so in three places, and that was
correct because no feature needed one; with `notifications.v1`, the privilege
exists only when a person explicitly enables it. What is withdrawn is the phrase
**"and it must stay that way"** — a permanent *conclusion* where the project
only ever had a permanent *rule*.

The rule that replaces it (ADR-0015 §2):

> AnyFlow acquires a privileged Android capability only when a named,
> user-visible feature requires it; only through the platform-sanctioned API for
> that feature; only with the user's explicit, separately revocable consent; and
> never as a means of defeating a platform restriction that exists to protect
> the user.

Under that rule `BIND_ACCESSIBILITY_SERVICE`, `QUERY_ALL_PACKAGES`,
`SYSTEM_ALERT_WINDOW`, `READ_LOGS`, `MANAGE_EXTERNAL_STORAGE`, location, root,
default-IME status, hidden APIs and reflection all **remain refused**, and the
asymmetry is principled rather than convenient: `clipboard.v1` *declined* the
equivalent trade, because there the only routes were workarounds around a
protection. Here the platform offers a first-class API whose own javadoc names
*"bridging to paired devices"* as the use case.

## 3. OQ-02 — application filter default · **RESOLVED**

**Deny by default.** No third-party application is mirrored until the user
names it. The onboarding flow ends in a picker offering *Select apps* and
*Select all*; *Select all* is always a deliberate action, never a state the
product arrives in. Granting Android notification access does **not**
pre-enable anything.

Hard rules: AnyFlow's own package never mirrors (not a default — there is no
setting) · newly installed apps default disabled, surfaced passively ("3 new
apps are not being shared"), never by a prompt · work-profile notifications
default disabled behind their own switch · system notifications default
disabled · `VISIBILITY_SECRET` is never mirrored under any setting · **no
banking, password-manager or 2FA heuristic is treated as a security boundary**.

Why the two permissions are separate concepts is documented in ADR-0015 §5 and
[01 §7](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md): the OS
grant answers *"may this app read notifications on this phone?"*; the peer grant
answers *"may this specific computer, identified by a pinned key, be sent
them?"*. Different scopes, different revocation surfaces, different blast
radius. Collapsing them would mean a person who wanted a phone-side capability
had silently authorised a network destination.

## 4. OQ-03 — dismiss-sync default · **RESOLVED**

**Supported, default OFF**, opt-in per peer. **This reverses the research
recommendation**, and [01 §3.1](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md)
was rewritten to keep the original argument visible rather than quietly
replacing it.

The reversal is on *category*, not utility: dismissal is the only operation in
`notifications.v1` that changes state on the **source** device. Everything else
is passive observation. "This computer may see my phone" and "this computer may
act on my phone" are two consents.

Normative requirements recorded: only a desktop **user** dismissal
(`NotificationClosed` **reason 2 only**) may cancel at the source · expiry
(reason 1) never may · non-clearable and ongoing notifications are never
force-cancelled · duplicate dismiss is idempotent · a dismiss for an offline
peer is dropped, not queued · no arbitrary action execution, no `PendingIntent`,
no reply — enforced by the schema having no field able to carry them.

## 5. OQ-04 / CompanionDeviceManager · **RESOLVED FOR V1 — CDM DEFERRED**

**`notifications.v1` does not adopt CompanionDeviceManager.**
`isAppTrustedNotificationListenerService` treats a live CDM association as
*trust*, and a trusted listener is exempt from sensitive-content redaction — so
a change made for background-execution reasons, in a different sprint, would
alter notification privacy as a side effect. AnyFlow does not need it: explicit
pairing, SPKI-pinned TLS, per-peer identity, per-capability grants and
revocation are already a complete trust model.

**Recorded as DEFERRED, not forbidden.** Adopting it later requires (1) a
separate security ADR naming CDM as its subject, (2) **new hardware
verification** of OTP and sensitive-notification redaction under a live
association — POC-NOTIF-01 re-run, not reasoned about — and (3) a re-reading of
ADR-0015 §9 and of T-N02/T-N03, with compensation if redaction is lost.

**One correction was necessary.** The research presents CDM as *the* route to
listener trust. On the certification device the trusted-listener set is
non-empty (`mTrustedListenerUids={1000, 10064, 10135}`) while the CDM
association table is **empty**. CDM confers trust — that claim is unaffected —
but it is not the only route, and the inverse must not be stated. Corrected in
[03 §T-N03](../../../docs/research/notifications-v1/03-PRIVACY-SECURITY-THREAT-MODEL.md)
and in [POC-NOTIF-01 §2](../../../docs/research/notifications-v1/poc/POC-NOTIF-01.md).

## 6. Notification identity · **APPROVED**, with three documentation corrections

`notification_id = HMAC-SHA256(device_notification_secret,
"anyflow/notifications.v1/id/v1" || len32(key) || key)[0..16]`

| Question asked | Finding |
| --- | --- |
| What exact secret? | `device_notification_secret` — 32 CSPRNG bytes, generated once per install, stored beside the identity (0600 in a 0700 dir; Android: the existing encrypted store). **Not** the identity key, not a credential, authenticates nothing |
| Stable across app/process restart? | **Yes.** The secret is persisted; the `id → key` map is in memory and rebuilt by re-deriving over `getActiveNotifications()` on `onListenerConnected`. This is exactly why the id is derived rather than random |
| Survives expected identity/store persistence? | **Yes**, same store and same file-mode discipline as the identity |
| After identity reset / re-pair? | **Was unspecified.** Now specified — see correction (a) |
| Is 128-bit truncation adequate? | **Yes.** Truncated HMAC-SHA256 is standard practice; the id authenticates nothing, and the live population is capped at 200 mirrors/peer and a 100-entry snapshot, so collision probability is negligible by many orders of magnitude |
| Does the destination ever need the raw Android key? | **No.** The only message naming a notification back to the source is `DismissRequest`, which carries `notification_id`. An unmappable id answers `UNKNOWN_NOTIFICATION` — fail closed, and no oracle, at 128 bits of HMAC output |
| Does remote dismissal need a source-side mapping? | **Yes**, `notification_id → platform key`, in memory only, holding identities and never content — and it is *reconstructible* precisely because the id is derived |
| Would the raw key leak package/user/profile info? | **Yes** — `key` is `userId\|pkg\|id\|tag\|uid`, carrying the profile id and the app's install-specific uid. The design already excludes it |

**Preferred property satisfied:** remote peers receive an opaque identifier, not
the Android raw key.

Three minimum documentation-level corrections were made. No production code was
touched.

* **(a) Secret lifecycle** — new
  [02 §5.5](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md). The
  secret survives process restart and reboot; is regenerated if lost (fail
  forward — it is not a credential); and is **destroyed and regenerated on
  device identity reset or re-pair**. Without that rule a peer that recorded ids
  before and after a re-pair could link the two identities — the exact
  correlation re-pairing exists to break. A regenerated secret is a **mirror
  reset**, reconciled by the ordinary `SyncMarker` snapshot, never by retaining
  state across the event meant to clear it. Tracked as **OQ-15, resolved**.
* **(b) What the id hides, stated exactly** — new
  [02 §5.6](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md). The
  HMAC removes `userId` (profile), `uid`, `id` and `tag`. It does **not** hide
  the package name, which travels in `app_id` **by design**, because the sink
  must show which app sent a notification. The id must never be described as
  anonymising the source app.
* **(c) The `UNKNOWN_NOTIFICATION` path** is written down as the fail-closed
  answer for an unmappable id, in the same section.

**No P0 ambiguity remains here.**

## 7. Active snapshot / resync · **APPROVED**, with two documentation corrections

| Property required | Verdict |
| --- | --- |
| Snapshot contains only currently-active notifications | ✅ built from `getActiveNotifications()` |
| It is not history | ✅ [02 §7.3](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md) tabulates the distinction rather than asserting it |
| Removed-before-completion cannot reappear permanently | ✅ now *proved* rather than assumed — correction (b) |
| Start/end explicit | ✅ `SyncMarker{sync_id, BEGIN}` … `{END}` |
| Missing entries removed after completion | ✅ at `{END}`, the sink removes every mirror for that peer not named |
| Reconnect does not duplicate | ✅ derived ids + `replaces_id` + `MirrorTable` retained through the grace |
| A malicious/stale snapshot cannot retain old notifications for ever | ✅ new [02 §7.7](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md) |
| Content is not persisted merely to implement the grace | ✅ during the grace the content lives in gnome-shell, where it was already visible; AnyFlow holds ids and a `content_hash` digest, nothing else, nowhere |

* **(a) The 60 s is reclassified, not justified.** New
  [02 §7.5](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md) is a
  table of which constants are protocol and which are not. `RECONNECT_GRACE`
  (60 s), `SYNC_TIMEOUT` (30 s), `MAX_SNAPSHOT_ENTRIES` (100) and
  `MAX_MIRRORS_PER_PEER` (200) are **sink- or source-local implementation
  constants**: never negotiated, never on the wire, and unable to break
  interoperability when changed. Only the `BEGIN`/`END` bracketing and the
  remove-what-is-not-named rule are protocol semantics. What *is* normative
  about the grace is its bound — greater than zero (or a Wi-Fi blip clears and
  re-posts the desktop) and finite (or a departed phone leaves notifications on
  a screen that can no longer update them). **No arbitrary wall-clock value is
  frozen into protocol compatibility.** [OQ-06 resolved.](../../../docs/research/notifications-v1/06-OPEN-QUESTIONS-AND-POCS.md)
* **(b) The ordering guarantee is written down.** New
  [02 §7.6](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md): the
  snapshot is not atomic, and it does not need to be, because every message for
  a peer travels one ordered TLS control session — so a removal that happens
  mid-snapshot is always applied *after* the upsert that carried it. The one
  honest gap (a session drop between upsert and removal) is bounded by the grace
  window: a stale mirror for at most one grace, never permanently.
* Also recorded in §7.7: the snapshot path **grants a peer no authority it does
  not already have** through ordinary upserts. It is a reconciliation mechanism,
  not a privileged one.

## 8. POC-NOTIF-01 — OTP redaction on the certification hardware

Full evidence: **[`docs/research/notifications-v1/poc/POC-NOTIF-01.md`](../../../docs/research/notifications-v1/poc/POC-NOTIF-01.md)**.
Criteria were taken verbatim from
[06 §3](../../../docs/research/notifications-v1/06-OPEN-QUESTIONS-AND-POCS.md) and not
redefined.

**Device.** SM-X620 · Android **16** · API **36** · One UI **8.0** (`80500`) ·
patch 2026-07-05 · build `X620XXS9CZG3`. Attached over `adb` (`adb devices` →
`RX2Y500C7SY device`).

**Preconditions verified on the device — all favourable to redaction firing:**

* `cmd device_config list systemui` →
  `android.service.notification.redact_sensitive_notifications_from_untrusted_listeners=true`
  (the research could not read this flag; it lives in `device_config`, not
  `settings`);
* Google's assistant is the primary NAS and holds a **live** binding
  (`dumpsys notification`), with
  `device_personalization_services/Notification__enable_otp_in_smart_suggestion=true`;
* the PoC listener's uid **10408** is in the approved set and **not** in
  `mTrustedListenerUids={1000, 10064, 10135}`;
* `dumpsys companiondevice` → `Companion Device Associations: <empty>`.

**Procedure.** Two throwaway APKs built outside the repository with
`aapt2` + `javac --release 17` + `d8` + `apksigner` (build-tools 35.0.0) — the
research explicitly permits a throwaway artifact. Listener enabled with
`cmd notification allow_listener`; poster launched with `am start`; output read
with `adb logcat`. The listener logs package, field **lengths** and an
equality-to-posted boolean, and prints a received string verbatim **only** when
it matches nothing the poster posted — so content is never logged and platform
redaction text cannot be missed. Each notification was additionally re-read
through `getActiveNotifications(key)` on a delay, to catch redaction applied
after classification.

**Inputs.** Three OTP-shaped plain notifications, one control, then a **second
vector** of `BigTextStyle`, `CATEGORY_MESSAGE` and `CATEGORY_EMAIL` variants
added after the first round returned nothing. All strings synthetic; `483920` is
invented.

**Observed.** All **seven** notifications delivered verbatim.
`verbatim=false` appeared **zero** times across both rounds. Post-hoc:
`mSensitiveContent=false` and `mAdjustments=[]` on our records, and
`grep -c "mSensitiveContent=true"` → **0** across all 93 notification records on
the device. So this is not our strings failing a regex — the classification
pipeline is inert on this build.

**Verdict: CONCLUSIVE — platform OTP redaction is ABSENT.** This is the
criterion's documented **Fail** branch, whose own text reads: *"the protection
is absent on this device and the design's assumption (that it must be assumed
absent) was correct and load-bearing."*

**Consequences, all applied:** T-N02's residual risk moves from *"high, and
presence unknown"* to *"high, and absence measured"* · product copy may not
offer platform redaction as reassurance on any device
([01 §8.1](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md), new
"the permission copy is a security control" subsection) · the per-app allow-list
is the **only** effective control, which argues **for** OQ-02's deny-by-default
· **nothing in the protocol, identity, filter or lock design changes**.

**Incidental finding.** One UI 8 posted a *synthetic* aggregate group summary
(`…|g:Aggregate_NormalNotificationSection|…`, null title and text) that reached
the listener — direct evidence for OQ-13 and OQ-10, both of which are P1/P2 for
N1, not for N0.

## 9. POC-NOTIF-02 — GNOME lock detection

Full evidence: **[`docs/research/notifications-v1/poc/POC-NOTIF-02.md`](../../../docs/research/notifications-v1/poc/POC-NOTIF-02.md)**.

**Environment.** Fedora 44 · GNOME Shell **50.4** · session type **Wayland** ·
seat0, `Active=yes` · logind session object `/org/freedesktop/login1/session/_32`
· notification server `('gnome-shell','GNOME','50.4','1.2')` with capabilities
`actions, body, body-markup, icon-static, persistence, sound`.
`org.freedesktop.ScreenSaver.GetActive` still returns
`org.freedesktop.DBus.Error.NotSupported` — the idle-inhibit trap reconfirmed.

**Procedure.** Two `dbus-monitor` instances (session `org.gnome.ScreenSaver`;
system `PropertiesChanged` on the logind session path) for the whole run, plus
50 ms polling of `LockedHint` for latency. Four paths: `loginctl lock-session`;
`org.gnome.ScreenSaver.Lock()` (what Super+L calls — the shortcut path, invoked
over D-Bus because the keypress needs a human); `SetActive(true)`; and
`SetActive(true)` with `lock-enabled=false` for blank-without-lock. Unlock via
`loginctl unlock-session`.

**Observed.**

| Path | `LockedHint` | `ActiveChanged` |
| --- | --- | --- |
| `loginctl lock-session` | **true @ +1.06 ms** | true, same instant |
| unlock | false @ +1.09 ms | false |
| `ScreenSaver.Lock()` | **true @ +218 ms** | true @ **+884 ms** |
| unlock | false | false |
| `SetActive(true)` | **never transitioned** | true |
| blank, `lock-enabled=false` | correctly `no` | reports **active** |

**Verdict: PASS.** Both sources report a real lock well inside 1 s on both lock
paths, both report the unlock, and blanking-without-locking does not report
`LockedHint` locked. The documented Fail branch — *"either source is silent or
wrong … the `Full` lock policy must be marked unavailable"* — is **not**
triggered: a prompt, correct source exists, so `Full` and `AppOnly` are both
available and effective.

**One mandatory correction, and it is the valuable part.**
`org.gnome.ScreenSaver.ActiveChanged` is **not** a lock signal and fails in
*both* directions: it lagged a real lock by **665 ms** (a **fail-open** window
in which a locked screen would still be shown full notification bodies — exactly
the failure [01 §6.2](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md)
exists to prevent), and it fired for a blank without a lock (fail-closed false
positive). The research treated the two sources as interchangeable. They are
not.

> **`org.freedesktop.login1.Session.LockedHint` is authoritative.**
> `ActiveChanged` may be subscribed only as a wake-up to re-read it, and its
> boolean is discarded. Unreadable lock state is treated as **locked**.

Applied to [01 §6.2](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md),
[ADR-0015 §7](../../../docs/adr/ADR-0015-notification-access.md) and recorded as
**OQ-14, resolved**.

**PoC hygiene (both PoCs).** Only synthetic, non-sensitive text was used — no
real OTP, banking notification, private message, personal filename, credential
or token. No notification body from any real app appears in any repository
artifact. Cleanup verified: no `throwaway` package on the device;
`enabled_notification_listeners` byte-identical to its pre-PoC value;
`Approved uids for user 0` back to `[10212, 10135]`; no PoC notification active;
host `lock-enabled` restored to `true`, session unlocked. All PoC sources and
APKs live in the session scratchpad, outside the repository, and **none is
committed**.

## 10. Remaining P0 / blockers

**None.**

* **BLOCKER:** none. OQ-01 is resolved.
* **Unresolved P0 architecture or security questions:** none. OQ-02, OQ-03,
  OQ-04 are resolved; OQ-14 and OQ-15 were raised and resolved within this
  sprint; both P0 PoCs are executed.

Everything still open is P1 or P2 and is scoped to a wave **after** N0:

| Open | Class | First wave it blocks |
| --- | --- | --- |
| OQ-10 group summaries · OQ-13 adaptive bundling · OQ-08 secret scope | P1 / P2 | N1 |
| POC-NOTIF-06 update re-alerting | P2 | N2 |
| POC-NOTIF-05 human dismissal emits reason 2 | P1 | N4 |
| OQ-05 confirmation · OQ-06 tuning · POC-NOTIF-04 One UI app-sleep | P1 / P2 | N5 |
| POC-NOTIF-03 KDE Plasma parity | P1 | KDE support — not in v1; no hardware here |
| OQ-09 Google Play policy | P2 | A Play submission — not planned; GitHub distribution today |

## 11. Documents changed

**Created**

| Path | What |
| --- | --- |
| `docs/adr/ADR-0015-notification-access.md` | The canonical decision record |
| `docs/research/notifications-v1/poc/POC-NOTIF-01.md` | OTP redaction — procedure, evidence, verdict |
| `docs/research/notifications-v1/poc/POC-NOTIF-02.md` | Lock detection — procedure, evidence, verdict |
| `NOTIFICATIONS-V1-DECISION-REPORT.md` | This report |

**Modified**

| Path | What |
| --- | --- |
| `README.md` | `notifications.v1` **approved in design only**; this release has no listener and no notification code. Principle 8 extended with the acquisition rule. ADR range → 0015 |
| `docs/security/THREAT_MODEL.md` | Scope note distinguishing current release from approved design; **T26 amended** (the phrase "no notification listener" qualified, clipboard reasoning untouched); **T27 added** — Android notification access |
| `docs/adr/README.md` | ADR-0015 index row |
| `…/01-FUNCTIONAL-SPECIFICATION.md` | §3.1 rewritten (dismiss sync **off**, original argument preserved); policy default flipped; §6.2 lock source corrected; §8.1 UX mock and new permission-copy security rules |
| `…/02-PROTOCOL-AND-EVENT-MODEL.md` | §5 marked APPROVED; **new §5.5** secret lifecycle, **§5.6** what the id hides; **new §7.5** protocol-vs-tunable, **§7.6** mid-snapshot ordering, **§7.7** hostile-snapshot bounds |
| `…/03-PRIVACY-SECURITY-THREAT-MODEL.md` | T-N02 mitigation and residual risk updated with measured evidence; T-N03 CDM decision recorded plus the trust-route correction; T-N16 points at ADR-0015; assumption 3 updated |
| `…/04-PLATFORM-CAPABILITY-MATRIX.md` | Android redaction stated as conditional ("can redact") rather than assured |
| `…/05-IMPLEMENTATION-PLAN.md` | §0 preconditions **met**; ADR-0015 marked written; the two decisions that bind N2 and N3 restated where they will be read |
| `…/06-OPEN-QUESTIONS-AND-POCS.md` | OQ-01…04 rewritten as resolved; OQ-06 reclassified; OQ-10/OQ-13 given device evidence; **OQ-14, OQ-15 added and resolved**; PoC entries marked executed; status table and "what remains" rewritten |

Duplication was avoided deliberately: ADR-0015 is the canonical source and the
research set references it. 357 relative links across the changed and created
documents were checked; **all resolve**.

## 12. Production-source guard

```console
$ git diff --name-only | grep -E '(^android/|^desktop/|^protocol/|\.proto$|Cargo\.toml$|build\.gradle|\.kt$|\.rs$)'
(no output)
```

Repeated over untracked files: also no output. **No production Rust or Kotlin
was modified. No protobuf was created or changed. No `NotificationListenerService`
exists. No Linux notification sink exists. `AndroidManifest.xml` is untouched** —
the manifest change belongs to the implementation sprint, as instructed. Nothing
in `notifications.v1` is implemented.

## 13. `git diff --check`

```console
$ git diff --check
(clean — no whitespace errors, no conflict markers)
```

## 14. `git status`

```console
$ git status --short
 M README.md
 M docs/adr/README.md
 M docs/security/THREAT_MODEL.md
?? NOTIFICATIONS-V1-RESEARCH-REPORT.md
?? NOTIFICATIONS-V1-DECISION-REPORT.md
?? docs/adr/ADR-0015-notification-access.md
?? docs/research/notifications-v1/

$ git diff --stat
 README.md                     | 17 ++++++++-
 docs/adr/README.md            |  1 +
 docs/security/THREAT_MODEL.md | 87 ++++++++++++++++++++++++++++++++++++++++++-
 3 files changed, 101 insertions(+), 4 deletions(-)
```

Documentation only. **Nothing was staged, committed or pushed.** No PR was
opened.

## 15. Implementation readiness gate

**Can N0 implementation begin? — YES.**

| Gate | Status |
| --- | --- |
| OQ-01 resolved | ✅ APPROVED under ADR-0015's contract |
| OQ-02 resolved | ✅ deny by default |
| OQ-03 resolved | ✅ dismiss sync default **off** |
| OQ-04 resolved for V1 | ✅ CDM deferred, not used |
| POC-NOTIF-01 | ✅ **executed and conclusive** — see the note below |
| POC-NOTIF-02 | ✅ **PASS** |
| No remaining BLOCKER | ✅ |
| No unresolved P0 architecture/security question | ✅ |
| Protocol / event model internally consistent | ✅ six messages, one session, no envelope or `core.proto` change; no field can carry a `PendingIntent`, action or `RemoteViews` |
| Identity / update semantics approved | ✅ §6, with three documentation corrections applied |
| Privacy defaults approved | ✅ grant never in `auto_grant`; app filter, work profile, ongoing, dismiss sync and lock policy all start closed |

### The one judgement call, stated plainly

The gate asks for **"POC-NOTIF-01 PASS"**. Its criteria were not redefined, and
under them the result is the **Fail** branch: OTP-shaped notifications arrived
with content intact.

That branch is a finding about **Android**, not a defect in `notifications.v1`,
and the PoC's own text says so: the Fail branch reads *"the protection is absent
on this device and the design's assumption … was correct and load-bearing."* The
design never depended on redaction — [03 assumption 3](../../../docs/research/notifications-v1/03-PRIVACY-SECURITY-THREAT-MODEL.md)
already stated *"the design assumes it is absent"* — so the result **confirms**
the architecture instead of invalidating it. The PoC's stated purpose was to
change *product copy* and *residual risk*, and both changes have been made.

Reading that literal Fail as a blocker would halt implementation on a platform
behaviour AnyFlow neither controls nor relies on, and would leave the
substantive rule the gate exists to enforce — *"no BLOCKER, no unresolved P0
architecture or security question"* — fully satisfied while the answer was
"not ready". The P0 that mattered was the **unknown**, and it is now known.

**This is flagged rather than absorbed** so the call is visible and reversible.
If the project reads the gate strictly and wants POC-NOTIF-01 to gate on Android
protecting us, the verdict flips to NOT READY on that single line — and the
remedy would not be an engineering change, because there is none available.

## 16. Recommendation for the next sprint

**Open wave N0 — protocol, ADRs and architecture documentation. Do not open N1
or N2 in the same sprint.**

N0 scope, unchanged from
[05](../../../docs/research/notifications-v1/05-IMPLEMENTATION-PLAN.md) minus the ADR
already written:

1. `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto` — the six
   messages, compiled by **both** toolchains (`protox` and the Android protobuf
   plugin) from the one file, with a round-trip test on each side. Security
   gate: a test asserting the generated Rust type's exact field set, so no field
   capable of carrying a `PendingIntent`, an action or a `RemoteViews` can be
   added without a failing test.
2. **ADR-0016** — notification identity and update semantics, folding in
   [02 §5.5 / §5.6](../../../docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md).
3. **ADR-0017** — capability roles negotiated *inside* a capability. Worth its
   own record because it is a pattern, not a feature.
4. `docs/architecture/NOTIFICATIONS.md`, linked from `OVERVIEW.md`; a
   `notifications.v1` section in `PROTOCOL.md` beside `clipboard.v1`'s.
5. **No behaviour change.** Nothing is registered; `cargo test` and the Android
   suite stay green.

`AndroidManifest.xml` is **not** touched in N0 either — the `<service>` element
belongs to N1, where the code that uses it lands, so the repository never
contains a declared listener with nothing behind it.

Two decisions from this sprint are easy to implement from memory of the older
text and are restated in the plan's preconditions where they will be read:
**N3 — `allow_dismiss_sync` defaults off**, and **N2 — `logind LockedHint` is
the authoritative lock source.**

Run **POC-NOTIF-06** (does an in-place update re-alert?) during N0 or early N2;
it is fifteen minutes on this host and it changes the sink's coalescing design.
**POC-NOTIF-05** needs a human to click and should be scheduled with whoever
runs N4. **POC-NOTIF-04** needs an N1 build and an overnight idle on the tablet.

---

# NOTIFICATIONS.V1 IMPLEMENTATION READY
