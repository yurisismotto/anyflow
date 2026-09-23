# ADR-0015 — Android notification access and the `notifications.v1` security boundary

**Status:** Accepted · 2026-09-08

Supersedes the absolute form of the manifest comment and of
[THREAT_MODEL.md](../security/THREAT_MODEL.md) T26 on notification access. It
does not supersede the reasoning behind them.

This ADR is the **canonical source** for the decisions below. The
`notifications.v1` research set restates them only by reference.

## Context

`notifications.v1` mirrors a phone's notifications onto a desktop and lets one
dismissal clear both. On Android the only supported way to read another app's
notifications is `NotificationListenerService`, bound by the system under
`BIND_NOTIFICATION_LISTENER_SERVICE`. There is no lesser API. The feature and
the permission are the same decision.

### What the project has said until now

OmniBridge has advertised the *absence* of that privilege as a feature, in three
places:

* `android/app/src/main/AndroidManifest.xml`, under the heading **"Deliberately
  absent, and it must stay that way"**, listing `BIND_NOTIFICATION_LISTENER_SERVICE`
  alongside `BIND_ACCESSIBILITY_SERVICE`, `QUERY_ALL_PACKAGES`, location and
  `MANAGE_EXTERNAL_STORAGE`;
* `THREAT_MODEL.md` **T26**, which repeats *"The manifest declares no
  accessibility service, no notification listener, no `QUERY_ALL_PACKAGES`, no
  location and no `SYSTEM_ALERT_WINDOW`"*;
* `README.md` principle 8, *"No root, no accessibility service, no ADB, no
  hidden permissions."*

**That position was correct and it is not being erased.** It is being made
precise. Read in context, T26 is an argument about `clipboard.v1`: every
technique that defeats Android's clipboard restriction — an accessibility
service, a default IME, a focus-stealing activity, `READ_LOGS`, root — is
user-hostile, and OmniBridge declines all of them. A notification listener appears
in that list because at the time OmniBridge had no honest reason to hold one, and
holding a powerful permission with no feature behind it is exactly the posture
the list condemns.

`notifications.v1` supplies the honest reason. What the list was really
asserting is not *"OmniBridge will never hold a powerful permission"* — OmniBridge
already holds `CHANGE_WIFI_MULTICAST_STATE` and `FOREGROUND_SERVICE_CONNECTED_DEVICE`
and argues in the manifest that it genuinely uses them — but *"OmniBridge will not
hold a privilege it does not need, will not acquire one by a side door, and
will not use one the user did not knowingly enable."* That rule survives this
ADR intact. The word that does not survive is **"must stay that way"**, which
stated a permanent conclusion where the project only ever had a permanent
*rule*.

### The historical record, stated plainly

| Period | Position |
| --- | --- |
| **Foundation … `clipboard.v1` (through 2026-09-08)** | OmniBridge holds no notification-listener privilege. There is no feature that needs one, and every clipboard workaround that would use one is refused |
| **`notifications.v1`, as approved here** | The privilege exists in the manifest, is held by the *system* rather than by OmniBridge, and is inert until the user enables notification access in Android Settings **and** separately grants `notifications.v1` to a specific paired peer |
| **Every release shipped so far** | Still contains no `NotificationListenerService`. Nothing in this ADR is implemented |

### Evidence gathered before deciding

Two proofs of concept were run on the certification hardware on 2026-09-08:

* [POC-NOTIF-01](../research/notifications-v1/poc/POC-NOTIF-01.md) — Android 16 /
  One UI 8.0 on SM-X620 delivers OTP-shaped notifications to an untrusted
  listener **entirely unredacted**, across three vectors, despite the platform
  flag being enabled and Google's classifier being bound. Nothing on the device
  is classified sensitive.
* [POC-NOTIF-02](../research/notifications-v1/poc/POC-NOTIF-02.md) — GNOME
  Shell 50.4 reports a real lock through `logind LockedHint` within 1 s on both
  lock paths, while `org.gnome.ScreenSaver.ActiveChanged` lags a lock by 665 ms
  and fires for blank-without-lock.

The first is why this ADR does not lean on platform OTP protection anywhere.

## Decision

**OmniBridge may declare a `NotificationListenerService` for `notifications.v1`,
under the explicit security contract below.** The contract is normative: a
change that violates any clause of it is a change to this ADR, not an
implementation detail.

### 1. The contract

Notification access:

1. **is optional** — the feature can be absent entirely and the product works;
2. **is disabled by default** — a fresh install reads nothing;
3. **requires the Android OS notification-access grant**, given by the user in
   Settings, revocable there at any time;
4. **separately requires an explicit OmniBridge peer grant** of `notifications.v1`.
   Neither permission implies the other, and the OS grant alone sends nothing
   to anyone;
5. **is independently revocable** — either permission can be withdrawn without
   the other, and withdrawing either stops delivery;
6. **must fail closed** — absent, unreadable, unknown or ambiguous state is
   treated as "no permission", never as "permitted";
7. **is not required for `battery.v1`, `files.v1` or `clipboard.v1`**, which
   must keep working with notification access denied.

And notification access does **not**:

8. imply notification **history** — none exists, anywhere, by design;
9. imply **cloud synchronisation** — there is no cloud;
10. imply **telemetry** — none is added;
11. permit **arbitrary notification actions**;
12. permit **`PendingIntent` execution** — no schema field can carry one;
13. permit **`RemoteInput`/reply** in v1;
14. make notification content **persistent** — nothing is written to disk on
    either side;
15. cause notification content to be **logged**, at any level, including `TRACE`.

### 2. The permanent rule that replaces "must stay that way"

> **OmniBridge acquires a privileged Android capability only when a named,
> user-visible feature requires it; only through the platform-sanctioned API
> for that feature; only with the user's explicit, separately revocable
> consent; and never as a means of defeating a platform restriction that exists
> to protect the user.**
>
> `BIND_ACCESSIBILITY_SERVICE`, `QUERY_ALL_PACKAGES`, `SYSTEM_ALERT_WINDOW`,
> `READ_LOGS`, `MANAGE_EXTERNAL_STORAGE`, location, root, default-IME status,
> hidden APIs and reflection remain **refused**, and no feature is a reason to
> revisit them, because each of them is either a workaround for a protection or
> a grant far wider than any OmniBridge feature needs.

`BIND_NOTIFICATION_LISTENER_SERVICE` moves out of the refused list and into the
list of permissions OmniBridge holds *and justifies*, beside multicast and the
connected-device foreground service. It is the only entry that has ever moved,
and moving it took an ADR.

Note the asymmetry that makes this consistent rather than convenient:
`clipboard.v1` **declined** an equivalent trade. Android restricts background
clipboard reads, so OmniBridge ships a manual Android → Fedora send and says why
(T26). Notifications carry no such restriction — the platform offers a
first-class API whose own javadoc names *"bridging to paired devices"* as the
use case. OmniBridge is taking the sanctioned path, not routing around a closed
one.

### 3. Permission lifecycle

| Stage | State |
| --- | --- |
| Installed | No OS grant, no peer grant. The service is declared and never bound |
| OS access granted | The system *may* bind. OmniBridge keeps `META_DATA_DEFAULT_AUTOBIND=false` and does not request a bind. **Nothing is sent to any peer** |
| Peer granted `notifications.v1`, `allow_mirror` on, session established | `requestRebind` — the listener binds and mirroring begins |
| Last such peer disconnects or is revoked | `requestUnbind` — the listener unbinds |
| OS access revoked | Roles narrow immediately; a `NotificationRoles` message with no `SOURCE` role is sent, and every mirror on every peer is closed |

The binding is therefore a function of *live peer state*, which makes the claim
"OmniBridge reads your notifications only while a granted computer is connected"
structurally true rather than a promise.

### 4. Peer grants and revocation

The `notifications.v1` grant lives in the trust store beside `files.v1`'s and
`clipboard.v1`'s and is **never** in `auto_grant` (ADR-0008). Per-peer policy —
mirror, dismiss-sync, lock policy, work profile, ongoing, app filter — is local
state that **no protocol message can write**. The rule is an absence in the
schema, not a check that could be inverted.

Revocation narrows immediately, on an established session, and additionally
**closes every mirror already on the desktop**. A withdrawn grant that leaves
forty notifications on a screen has not withdrawn anything.

### 5. Application filtering — deny by default

**Approved: deny-by-default.** When `notifications.v1` is first enabled for a
peer, no third-party application is mirrored until the user names it. The
onboarding flow ends in an app picker offering **Select apps** and **Select
all**; *Select all* is a deliberate action a person takes, never a state the
product arrives in.

Rules no setting can override:

* OmniBridge's own package is **never** mirrored — a hard rule, not a default;
* an app installed after the grant defaults to **disabled**, surfaced passively
  ("3 new apps are not being shared"), never by a prompt;
* **work-profile** notifications default to disabled, behind their own switch;
* **system** notifications default to disabled, and are allowed only where
  explicitly enabled and technically safe;
* `VISIBILITY_SECRET` is never mirrored under any setting.

**No banking, password-manager or 2FA heuristic is a security boundary.**
OmniBridge does not detect OTPs — no regex, no keyword list, no app-category
guess. A guess dressed as a control is worse than an honest boundary, because
the user trusts it and it is wrong in cases neither party can predict. This is
`THREAT_MODEL.md` T10's reasoning, unchanged. The picker may *order* or
*annotate* apps to help a person choose; it may never *decide* for them.

Granting Android notification access is not consent to share, and the two must
never be collapsed. The OS grant answers *"may this app read notifications on
this phone?"*. The peer grant answers *"may this specific computer, identified
by a pinned key, be sent them?"*. They have different scopes, different
revocation surfaces and different blast radii: revoking the OS grant stops all
reading; revoking one peer grant stops one destination. Presenting them as one
switch would mean a person who wanted a phone-side capability had silently
authorised a network destination.

### 6. Dismiss synchronisation — supported, default **off**

**Approved: `allow_dismiss_sync` defaults to `false`**, opt-in per peer.

This **reverses the research recommendation**
([01 §3.1](../research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md)), which
argued for on-by-default on the grounds that dismissal is not a new authority
and that a mirror you cannot dismiss is worse than none. The argument is sound
about *utility* and is rejected on *category*: dismissal is the only thing in
`notifications.v1` that changes state on the source device. Everything else is
passive observation. Crossing from "this computer can see my phone" to "this
computer can act on my phone" is a different consent, and the product asks for
it separately even though the action is small.

Requirements, all normative:

* only a **user dismissal on the desktop** may cancel at the source —
  concretely, `NotificationClosed` **reason 2 only**;
* **expiry must never dismiss** the source notification (reason 1). A desktop
  banner timing out must not clear a person's phone;
* non-clearable and ongoing notifications are **never** force-cancelled;
* a duplicate dismiss is **idempotent** and reports `UNKNOWN_NOTIFICATION`
  rather than failing;
* a dismiss for an offline peer is **harmless** — dropped, never queued;
* **no arbitrary action execution, no `PendingIntent`, no reply.**
  `DismissRequest` has no field capable of carrying any of them, and that is
  enforced by the schema rather than by a check.

### 7. Lock policy

Per peer, both ends, `Full` / `AppOnly` / `Suppress`, default **`AppOnly`**.
The source reduces content **before encoding**, so withheld content never
exists on the wire. Unlocking is **not** retroactive: content withheld while
locked is not delivered later. Unknown lock state is treated as locked.

Per [POC-NOTIF-02](../research/notifications-v1/poc/POC-NOTIF-02.md), the
Linux sink's authoritative lock source is
`org.freedesktop.login1.Session.LockedHint`.
`org.gnome.ScreenSaver.ActiveChanged` may be used only as a hint to re-read it;
its boolean is discarded, because it lags a real lock by 665 ms (fails open)
and fires for a blank without a lock (fails closed).

### 8. Work profile

A listener in the personal profile receives work-profile notifications unless
the device administrator blocks it; the system ignores a listener running *in*
a work profile. Work-profile mirroring is therefore possible, is a separate
switch from the app filter, and **defaults off**. An organisation's data must
not cross to a personal computer because a person enabled a consumer feature.

### 9. OTP and sensitive-content implications

Android 15+ can redact OTP-classified notifications from untrusted listeners.
**OmniBridge treats that as a bonus and never as a control**, and
[POC-NOTIF-01](../research/notifications-v1/poc/POC-NOTIF-01.md) shows why: on
the certification target it does not fire at all. Six OTP-shaped notifications
across three vectors arrived verbatim.

Consequences, binding on the implementation:

* no product copy may offer platform redaction as reassurance;
* the **per-app allow-list is the only effective control**, which is an
  argument for deny-by-default rather than against it;
* the default `AppOnly` lock policy remains the second line, because a code
  arriving on a locked phone is not transmitted in full;
* OmniBridge still does not attempt OTP detection itself (§5).

### 10. CompanionDeviceManager — not adopted in v1

**Decision: CDM is DEFERRED. `notifications.v1` does not use it.**

`isAppTrustedNotificationListenerService` treats a live CDM association as
*trust*, and a trusted listener is exempt from sensitive-content redaction. So a
CDM association adopted for an unrelated reason — it is the platform-sanctioned
justification for companion background behaviour — would change notification
privacy semantics as a side effect, in a sprint whose author may never have
read this file.

OmniBridge does not need it. It already has explicit pairing, SPKI-pinned TLS,
per-peer identity, per-capability grants and revocation
(ADR-0006, ADR-0007, ADR-0008). CDM would add platform convenience on top of a
trust model that is already complete, at the cost of a privacy property.

**This is not a permanent prohibition.** Adopting CDM later requires:

1. a **separate security ADR** naming CDM adoption as its subject;
2. **new hardware verification** of OTP and sensitive-notification redaction
   under a live association — POC-NOTIF-01 re-run with the association in
   place, not reasoned about;
3. a re-reading of this ADR §9 and of T-N02/T-N03, with compensation if the
   verification shows redaction lost.

A note for whoever finds this from the other direction, in a background-execution
sprint: **adopting CDM is a notification-privacy decision.** It is not a
background-execution detail, whatever the reason you reached for it.

Recorded observation, from POC-NOTIF-01: on the certification device the
trusted-listener set is non-empty while the CDM association table is empty, so
CDM is *a* route to listener trust and not the only one. That does not weaken
the decision; it means the inverse claim ("untrusted implies no CDM") must not
be made.

### 11. Out of scope for v1

Notification history · reply / `RemoteInput` · arbitrary actions ·
`PendingIntent` of any kind · icons, images and `RemoteViews` · media/transport
control · Linux → Android mirroring · notification content in any log, database
or state file · any cloud or telemetry path. Each is out of scope because the
design forbids it, not because it was cut for time.

## Alternatives considered

**Do not build `notifications.v1` at all.** The honest option, and the one that
keeps the manifest comment literally true. Rejected because the comment's real
content is a rule about *how* privileges are acquired, and this feature can
satisfy that rule. Had the answer been no, the correct outcome was to close the
research branch and record why; the research stands either way.

**Build it without a listener.** There is no other API. `dumpsys` needs
`READ_LOGS` or root; an accessibility service is a far larger grant, and using
one to read notifications is exactly the side door §2 forbids. Rejected as
strictly worse for the same feature.

**Grant per-app access at the Android level and skip the peer grant.** Collapses
two questions with different scopes into one switch. Rejected — §5.

**Everything on after the grant (the common product default).** What most
comparable products do, and what a person may expect from a switch labelled
"share notifications". Rejected: it is the single most permissive default the
product would contain, and it is a privacy regression that cannot be walked
back for existing installs once shipped. Discoverability is paid for with a
picker inside the enabling flow instead.

**`allow_dismiss_sync` on by default.** The research recommendation. Rejected —
§6.

**Adopt CDM for its background-execution benefits.** Rejected for v1 — §10.

**Depend on platform OTP redaction.** Rejected on principle before the PoC and
disproved by it — §9.

## Consequences

**The manifest gains one `<service>` element and one system-held permission.**
`BIND_NOTIFICATION_LISTENER_SERVICE` is held by the *system*, not by OmniBridge:
declaring it on the service is what stops any other app from binding it,
exactly as `BIND_QUICK_SETTINGS_TILE` already does for the clipboard tile. The
manifest comment must move `BIND_NOTIFICATION_LISTENER_SERVICE` out of the
"deliberately absent" list and explain the grant, rather than quietly dropping
a line.

**`README.md` principle 8 survives unchanged.** No root, no accessibility
service, no ADB, no hidden permissions — all four still true. The README's
feature status must nonetheless distinguish the *current release*, which
contains no listener, from this *approved design*, which will contain an
optional one.

**`THREAT_MODEL.md` T26 must be amended, not deleted.** Its clipboard reasoning
is untouched and still correct; the sentence enumerating a notification
listener among permanently-absent permissions acquires an exception with a
pointer here.

**The trust surface grows, and this is the real cost.** A person who enables
this is trusting OmniBridge with the most sensitive stream on their phone. That
trust is repaid by the code being open, by nothing leaving the LAN, by the
listener being unbound whenever no granted peer is connected, and by every
default starting closed — and it must be *earned in the permission copy*, which
should be reviewed as a security control rather than as marketing.

**Play policy is not yet cleared.** Notification access is a restricted area of
Google Play policy. OmniBridge is distributed from GitHub today, so this blocks no
release; it must be answered before any Play submission
([OQ-09](../research/notifications-v1/06-OPEN-QUESTIONS-AND-POCS.md)).

**Nothing here is implemented.** No `.proto`, no Kotlin, no Rust, no manifest
change has been made. This ADR authorises wave N0.

## Future reconsideration criteria

This ADR is revisited if any of the following becomes true:

* **CDM is proposed**, for any reason — §10's three requirements apply;
* **a reply, action or `PendingIntent` capability is proposed** — every one is
  an escalation from mirroring to remote control and needs its own ADR and
  threat model, not a field addition;
* **notification history is proposed**, in any form, including a "recent"
  screen or an on-disk cache — the design currently forbids it and the
  prohibition is load-bearing for T-N12;
* **platform OTP redaction changes** — if a later Android or One UI build
  starts classifying, POC-NOTIF-01 is re-run and §9's copy is revised. The
  design must still not depend on it;
* **an OS-level API narrower than a full listener appears** — OmniBridge should
  prefer it, by §2;
* **Play submission is contemplated** — OQ-09 first;
* **a second sink platform is added** — Windows can fill every role, which
  changes the role matrix but not this contract.
