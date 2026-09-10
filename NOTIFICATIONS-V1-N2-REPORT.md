# `notifications.v1` — N2: the Linux notification sink

**Wave:** N2 · **Branch:** `feature/notifications-v1-n2-linux-sink` ·
**Date:** 2026-09-09

**Baseline commit:** `33db8fb1`
(*Merge pull request #18 from yurisismotto/feature/notifications-v1-n1-android-source*)

A notification posted on the Samsung tablet now appears on the Fedora desktop,
as a real GNOME notification, over the existing authenticated TLS session.
Updating it replaces it in place; removing it closes it; a denied application
never leaves the phone; a reconnect restores the screen without a single
duplicate. All of that was executed on the hardware and is transcribed below
from the session bus and the daemon's own log.

---

## 1. Baseline and scope

| | |
| --- | --- |
| Base | `33db8fb`, N1 merged and certified (**NOTIFICATIONS.V1 N1 PASS**) |
| Canonical design | ADR-0015, ADR-0016, ADR-0017, `docs/architecture/NOTIFICATIONS.md`, `docs/research/notifications-v1/**`, the two N0/N1 reports, `notifications_v1.proto` |
| Desktop under test | Fedora 44, GNOME Shell **50.4** (spec 1.2), Wayland, `anyflow-daemon` `DF65 D3E4 BA28 EDF9` |
| Phone under test | Samsung **SM-X620**, Android **16** / API 36, One UI **8.0**, peer `7E63 7B4E 937B 7732` |
| Commits made | **none** — nothing committed, nothing pushed, no PR |

Explicitly **not** in this wave, and verified absent below: the Android
settings/app-picker UI (N3), the dismissal-synchronisation runtime (N4),
Windows/macOS/iOS, notification actions or reply, notification history, any
cloud relay, and any change to the approved protobuf contract.

> **A note on the hardware model.** Every brief in this series names a "Samsung
> Galaxy S25". The device actually attached and certified against is a **Galaxy
> Tab S10 FE+ Wi-Fi (SM-X620)**, `ro.product.device=gts10fepwifi`, adb serial
> `RX2Y500C7SY`. All gate results below are reported against the real model.

---

## 2. Files changed

### New — the capability crate (`desktop/capabilities/notifications/`)

| File | Lines | What it owns |
| --- | ---: | --- |
| `src/lib.rs` | 1666 | The manager: sessions, the decision path for every inbound message, the per-peer worker, the grace, the lock reduction, and `NotificationsCapability` |
| `src/backend/mod.rs` | 769 | **The seam.** `NotificationSink`, `LockSource`, `Mirror`, `SinkError`, `CloseReason`, and the in-memory implementations of both traits |
| `src/backend/dbus.rs` | 425 | `org.freedesktop.Notifications` over the session bus |
| `src/backend/logind.rs` | 393 | `org.freedesktop.login1.Session.LockedHint` over the system bus |
| `src/queue.rs` | 438 | The bounded, coalescing hand-off that keeps a wedged shell off the session's dispatch loop |
| `src/mirror.rs` | 379 | `MirrorTable`: `(peer fingerprint, notification id) → server id`, memory only |
| `src/text.rs` | 290 | Sanitisation, markup escaping, presentation caps |
| `src/roles.rs` | 179 | What this desktop announces, and the epoch that orders it |
| `src/limits.rs` | 72 | Every sink-local tunable, and why none of them is protocol |
| `src/redact.rs` | 72 | Talking about a notification without printing one |
| `src/policy.rs` | 55 | `NotificationAuthorizer`, and the re-export of the persisted policy type |

### New — elsewhere

| File | Lines | What it owns |
| --- | ---: | --- |
| `desktop/core/src/notification_policy.rs` | 251 | `NotificationPolicy` and `LockPolicy` — persisted, so they live in core |
| `desktop/daemon/tests/notifications.rs` | 850 | The capability over real TLS, real pinning, the real trust store — 18 tests |

### Modified

| File | Change |
| --- | --- |
| `desktop/Cargo.toml` | The new workspace member |
| `core/src/lib.rs`, `core/src/store.rs` | `TrustedPeer.notification_policy` (`#[serde(default)]`), `Store::set_notification_policy` |
| `core/tests/portable_boundary.rs` | The new crate listed as portable, plus **two new tests** — see §24 |
| `core/tests/identity_and_store.rs`, `daemon/examples/fake_phone.rs` | One field each, at the two other `TrustedPeer` literals |
| `runtime/src/state.rs` | `DaemonState.notifications`, `with_notifications`, `notify_notifications_revoked`, and the `NotificationAuthorizer` impl |
| `runtime/src/server.rs` | `NotificationsStatus` / `NotificationsPolicy` handlers; grant withdrawal and unpair now close mirrors |
| `control/src/lib.rs` | The two requests, the response, and the two report types |
| `cli/src/main.rs` | `anyflow notifications status \| mirror \| when-locked \| dismiss-sync` |
| `daemon/src/main.rs` | Probes both platform seams, registers the capability, wires the authorizer, starts the three event pumps |
| `daemon/tests/common/mod.rs` | A memory sink and a steerable lock on the test server; `new_raw_notifications`; `CapturedNotifications` |
| `daemon/tests/e2e.rs` | Two negotiated-capability lists now include `notifications.v1`, and assert it is **not** auto-granted |
| `.github/workflows/portable-windows-msvc.yml` | The new crate in the portable set, `linux-dbus` in the forbidden-feature list, and a test-classification guard |

**No Android production file was touched** (§26), and the `.proto` is unchanged
(§26).

---

## 3. The capability crate

`anyflow-capability-notifications`, beside `battery`, `clipboard` and `files`,
following their conventions exactly: `unsafe_code = "forbid"`, `anyflow-core`
with `default-features = false` so `unix-fs` cannot unify back on, the platform
half behind a default-on feature (`linux-dbus`), and `zbus` optional so a D-Bus
client is never mandatory in a crate a Windows adapter will reuse.

It **re-exports** `anyflow_core::notifications` rather than reimplementing it.
Every limit, every identifier width, the role/epoch reduction, the conservative
enum resolution and the snapshot bracketing machine are N0's, called from here.
There is no second copy of any of them, which is what stops the two ends of the
protocol drifting.

No Linux code went into `anyflow-core`.

---

## 4. The `NotificationSink` seam

Wave 0 declined to create this trait before anything implemented one, on the
grounds that *an abstraction with nothing on either side of it is a guess about
a shape*. N2 is the first implementation, so N2 is where it belongs — and two
things changed from the sketch in `docs/architecture/NOTIFICATIONS.md`, both
because writing the D-Bus sink taught them:

```rust
#[async_trait]
pub trait NotificationSink: Send + Sync {
    fn id(&self) -> &'static str;
    fn describe(&self) -> String;
    fn capabilities(&self) -> SinkCapabilities;
    async fn availability(&self) -> SinkResult<()>;
    async fn display(&self, mirror: &Mirror, replaces: Option<ServerId>) -> SinkResult<ServerId>;
    async fn close(&self, id: ServerId) -> SinkResult<()>;
    fn closed_events(&self) -> Option<mpsc::Receiver<Closed>>;
    fn availability_events(&self) -> Option<mpsc::Receiver<bool>>;
}
```

1. **`capabilities()` appears.** Whether a body must be escaped is a question
   only the backend can answer (`body-markup`), and answering it above the seam
   would mean the capability knowing about freedesktop.
2. **`display` returns a `ServerId` that is always stored**, even when
   `replaces` was passed. The specification only promises the id is preserved
   when `replaces_id` is non-zero; a caller that *assumed* preservation would
   silently lose track of the notification on a server that behaved otherwise.
   GNOME does preserve it — measured, §23 — and the code does not rely on it.

`availability_events()` is the third addition and is what turns a GNOME Shell
restart from a silent failure into an observed one (§18).

The trait contains **zero** zbus imports, no GNOME or KDE types, no GTK, no
file descriptors and no object paths. That is not a claim; it is checked by a
new test (§24).

**No actions.** There is no `add_action`, no button, no callback carrying a
chosen index, and no method that could invoke one. `notifications_v1.proto` has
no field that could carry an action either, so the guarantee is the shape of
the interface rather than a check a later change could invert.

### The close-event type, and why its four values are kept apart

```rust
pub enum CloseReason { Expired, Dismissed, Closed, Undefined }
```

N2 acts on none of them remotely. They are modelled precisely anyway, because
N4's most important rule depends on the distinction and collapsing it now would
make that rule unimplementable: **only `Dismissed` (reason 2) may ever become a
`DismissRequest`**, `Expired` must never dismiss anything on the phone, and
`Closed` is our own call coming back to us. `CloseReason::is_human_dismissal()`
is the seam N4 will read; §17 records what N2 does with it, which is nothing.

---

## 5. The Linux D-Bus implementation

Three spec details the implementation is built around, all re-verified on this
host during N2:

```console
$ gdbus call --session --dest org.freedesktop.Notifications \
      --object-path /org/freedesktop/Notifications \
      --method org.freedesktop.Notifications.GetServerInformation
('gnome-shell', 'GNOME', '50.4', '1.2')

$ … GetCapabilities
(['actions', 'body', 'body-markup', 'icon-static', 'persistence', 'sound'],)
```

* **`replaces_id` is the update mechanism.** One remote identity maps to one
  freedesktop id for its whole life. Measured in `real_dbus.rs`, live:
  `ids: 8 -> 8 -> 8`.
* **A closed id is dead** — *"invalidated before the signal is sent"* — so
  `NotificationClosed` purges the mapping, or the next update creates a second
  notification instead of replacing the first.
* **`CloseNotification` on an unknown id.** The specification says the server
  answers with an error; GNOME answers with nothing at all. Both mean *the
  notification is gone*, which is what was asked for, so this implementation
  reports **success either way**. Without that rule AnyFlow would log a failure
  on every dismissal against a spec-literal server such as dunst or mako.

What is deliberately never sent: the `actions` array is always empty, there is
no `app_icon`, no `image-data`, no `image-path`, no `sound-file`, no hyperlink,
and `urgency` is 0 or 1 and **never 2** — a critical notification does not
auto-dismiss on GNOME, and `IMPORTANCE_HIGH` is what every chat app sets, so
mapping it up would turn each message into a banner the user must click away. A
mirror must never be more intrusive than the notification it mirrors.

One hint is sent besides `urgency`: `desktop-entry` naming AnyFlow's own
application id, which is how a future Linux *source* will recognise its own
output. It is visible in the transcripts at §27.

**A D-Bus error is rendered as a class, never as its message.** Some
notification servers quote the failing call's arguments back in the error, and
for `Notify` those arguments are somebody's message; `{e}` in a log line would
put a stranger's text into the journal through a path nobody would think to
audit. `ErrorClass` renders `method-error:<name>`, `io`, `address`,
`handshake`, `invalid-reply`, `name-taken` or `other`, and there is a test that
feeds it a canary and asserts the canary does not come out.

---

## 6. The role model

**N2 announces `SINK`, and only `SINK`.**

`DISMISS_REPORTER` means *"I will tell you when a human dismissed a mirror I
displayed"*. N2 observes those closes and records their reasons accurately, and
sends nothing — that runtime is N4 — so announcing the role would be a claim
this wave cannot honour, and a peer that believed it would wait for reports
that never come.

`SOURCE` is never claimed at all: there is no supported way for an ordinary
client to observe other applications' notifications through
`org.freedesktop.Notifications` without becoming the notification server, and
only one process may own that name.

The role set is a function of **platform availability alone**. The peer grant is
deliberately not an input: holding a grant does not manufacture a notification
server, and having one grants nothing to any peer.

Epochs are per connection, from 1, strictly increasing, and an unchanged set
produces no announcement at all — which is what makes every increment mean a
real change. Observed live, cross-device, at §27.

---

## 7. Grants

`notifications.v1` is registered and advertised in `HELLO` unconditionally, and
is **never in `auto_grant`**. Pairing alone grants nothing; the desktop's
auto-grant policy still covers `battery.v1` and nothing else.

Three independent checks, in three places:

| Question | Answered by | When |
| --- | --- | --- |
| May this device speak notifications with me at all? | the trust store | at handshake (the transport's own filter) **and** per message |
| And how much of each, and when? | `NotificationPolicy` | per message |
| What can that device physically do right now? | the announced roles | per message |

The per-message re-read matters more here than for `clipboard.v1`. The
transport intersects the negotiated capability list with the grant when the
session is *built*; a grant withdrawn after that point is not noticed there at
all, so the capability's own `policy_for` is the only thing standing between a
revoked device and the screen. Proved over real TLS by
`withdrawing_the_grant_stops_notifications_on_the_session_that_is_already_up`.

Withdrawing a grant, revoking a pairing, or turning `mirror` off does not only
stop *future* notifications: it **closes the ones already on the screen**,
immediately and with no grace. A grace is for a peer that may come back; a
revoked peer is one the user has just said should not be on this screen.

**A peer can never set its own policy.** There is no protocol message that
writes any field of `NotificationPolicy`, and the schema has none — the rule is
an absence rather than a check a later change could invert (NOTIF-SEC-20).

---

## 8. Mirror state

```text
(peer_fingerprint, notification_id)  →  freedesktop notification id
```

**The key is the pinned TLS identity**, never `origin_device_id`. So a peer can
only ever address its own mirrors: two phones sending the same 16 bytes touch
two different entries, a peer that replays another peer's `notification_id`
reaches its own namespace and finds it empty, and a peer that lies about
`origin_device_id` changes what a diagnostic prints and nothing more
(NOTIF-SEC-05, -07, -09, all tested).

What an entry holds, in full:

| Field | Why it has to be there |
| --- | --- |
| `server_id` | The local handle. Without it an update creates a second notification |
| `content_hash` | The **source's** opaque digest. Comparing it is how an identical re-send becomes a no-op |
| `app_name` | So mirrors already on screen can be re-posted reduced when the session locks |
| `urgency` | So a reduced re-post is neither louder nor quieter than what it replaces |
| `reduced` | So a lock event does not re-post what is already reduced |

**There is no title and no body on this type, and no field that could hold
one.** `app_name` is the only peer-supplied text retained, capped at 64
characters. Retaining a title and body so they could be restored on unlock was
considered and rejected: it would be a notification history in memory, and the
design forbids one. The consequence is stated where it is decided — locking
reduces, unlocking does not restore (§16).

Bounded at `MAX_MIRRORS_PER_PEER = 200`, **per peer** rather than globally, so a
flooding peer evicts only its own notifications; a global cap would let one
device push another device's messages off the screen. An evicted mirror is
*closed*, not merely forgotten — otherwise it would sit on the screen for ever
with nothing able to update or remove it. Nothing here is ever written to disk;
there is no serde derive in the module.

---

## 9. `content_hash` — the sink does not recompute it

N1 established the rule and N2 keeps it. This sink **never** computes a
`content_hash`. It compares the digest it was sent last for one
`(peer, notification_id)` with the digest it is sent now, and does nothing else
with it: no canonical encoding, no domain string, no SHA-256 of anything.
`grep -rniE 'sha2|sha256|Digest' desktop/capabilities/notifications/src` finds three
comments and no code, and `sha2` is not a dependency of the crate.

That is not an omission. A digest only one side computes is a *source-side
optimisation*: N1 may change how it is built, or stop sending it, without
breaking anybody. The moment a sink recomputes and compares, the construction
becomes a wire-format contract both implementations must agree on for ever, and
a field documented as optional becomes load-bearing.

Tested from both directions:

* `the_sink_never_recomputes_content_hash` sends **changed content carrying an
  unchanged digest** — a sink that recomputed would notice and redisplay; this
  one answers `DUPLICATE` — and then **unchanged content carrying a changed
  digest**, which redisplays. The sink believes the digest, not its own reading
  of the text.
* `an_upsert_with_no_content_hash_is_always_displayed`: absent means the source
  is not offering a de-duplication value, and inventing one would be
  recomputing it by another name.
* The test suite's own `content_hash` fixture is deliberately **not** ADR-0016's
  construction, which is how the suite proves the sink is not secretly checking.

---

## 10. Upsert

The order of checks is deliberate — authorization, then what the peer claims it
is, then what the schema says, then policy, then de-duplication, and only then
anything that touches the desktop:

1. **Grant**, re-read from the trust store. `NOT_AUTHORIZED`.
2. **The peer's `SOURCE` role.** Absent roles mean no roles. `REJECTED_ROLE`.
3. **Privacy**, resolved conservatively: unspecified is `PRIVATE`, and an
   unknown value from a future implementation is `SECRET`. `SECRET` is never
   displayed and any existing mirror is closed. The source already refuses to
   send one; the sink does not take its word for it.
4. **Lock state**, read from the platform per notification, any failure meaning
   locked.
5. **Reduction**, *before* the mirror is built.
6. **De-duplication** against the stored digest and the presentation form.
7. **The desktop**, with the stored `replaces` id.

Mapping: `app_name` ← `app_label`, falling back to `app_id` when the source
could not resolve a label (a package uninstalled between posting and
mirroring); `summary` ← `title`, falling back to the application's name, because
a notification with no title is ordinary on Android and an empty summary
renders as a notification with no visible identity; `body` ← `body`; `urgency`
← `importance`, capped at normal.

**Progress is not synthesised into the body.** GNOME advertises no progress
capability, and mutating an application's own text to add "47%" would corrupt
user content to simulate a feature the desktop does not have. What progress
buys on Linux is the update-in-place path, which is the thing that was actually
wanted.

**`app_id` is never presented as trusted.** It reaches the sink deliberately —
the user must be able to see which app a notification came from — and it is
sanitized like every other peer-supplied string before it can be displayed.

---

## 11. Update and `replaces_id`

`NotificationUpsert` is idempotent. First upsert → `Notify(replaces_id = 0)`;
a later changed upsert → `Notify(replaces_id = <stored id>)`; the returned id is
always stored, because the server is permitted to answer with a different one.
An identical re-send is a `DUPLICATE`: no D-Bus call, no visual change, no
re-alert.

A **timestamp-only** change does not redisplay, because the source excludes
`posted_at_unix_ms` from its digest precisely so an identical re-post collapses,
and the sink honours that by comparing the digest rather than the message.

Live transcript, from the session bus, §27.

---

## 12. Remove

A known mirror → `CloseNotification(id)`, entry dropped, `REMOVED`. An unknown
one → `UNKNOWN_NOTIFICATION`, nothing closed, no error: the mirror may have been
dismissed by the user a moment ago, and both ends converging on *it is gone* is
the correct outcome. That is what makes removal idempotent, and
`removal_is_idempotent` drives three removals through one mirror and asserts
exactly one close ever happened.

A removal for a mirror that is *known but was never displayed* — a failed
`Notify`, or a suppressed one — answers `REMOVED` without a D-Bus call, because
there is nothing on the screen to close.

If the close itself fails, the local entry is dropped **anyway** (the source
has said the notification no longer exists, and holding a mirror nothing can
ever remove is worse) and the answer reports the truth about the backend rather
than claiming a success that did not happen.

Removal is never assumed to follow an upsert: nothing in the remove path reads
or requires prior state beyond the table lookup.

---

## 13. Snapshot

N0's `Snapshot` state machine, unchanged and called from the worker.

* `BEGIN` opens a bracket and arms a `SYNC_TIMEOUT` deadline.
* Ordinary upserts inside it are recorded as active — **including duplicates**,
  because a snapshot names what is *active*, and an unchanged notification is
  as active as a changed one. Failing to record it would make the
  reconciliation remove it.
* A valid `END` closes every mirror for that peer not named in between.
* A second `BEGIN` **abandons** the open snapshot without applying it.
* An `END` with no `BEGIN` is refused and removes nothing.
* An `END` whose `sync_id` does not match is refused, the open snapshot
  survives, and its own `END` still completes it.
* An unfinished snapshot is abandoned after `SYNC_TIMEOUT` and removes nothing.

**An incomplete or malformed snapshot never removes a mirror.** Six tests cover
the six branches, and `an_unpaired_snapshot_end_removes_nothing_over_the_transport`
covers the most dangerous one over real TLS.

`60 s` / `30 s` / `200` / `100` are **local tunables**, documented as such in
`limits.rs`: never negotiated, never on the wire, and changing any of them
cannot break interoperability. What is normative about the grace is only its
bound — greater than zero, or a three-second Wi-Fi blip clears the desktop and
re-posts everything; and finite, or a departed phone leaves notifications on a
screen that can no longer update them.

---

## 14. The lock detector

`org.freedesktop.login1.Session.LockedHint`, read at connect and tracked
through `PropertiesChanged`. `org.gnome.ScreenSaver.ActiveChanged` is **not
subscribed at all**, and `org.freedesktop.ScreenSaver` is not touched. There is
a test that scans the module's own source and fails if either name reaches
executable code.

POC-NOTIF-02 measured why:

| Source | Verdict |
| --- | --- |
| `LockedHint` | **Authoritative.** Locked within 218 ms on the Super+L path, 1 ms on `loginctl lock-session`, and correctly stays `no` when the screen merely blanks |
| `ScreenSaver.ActiveChanged` | **Not a lock signal.** It lagged `LockedHint` by **665 ms** on a real `ScreenSaver.Lock()` — a fail-**open** window — and reports active for a blank *without* a lock |
| `org.freedesktop.ScreenSaver` | **A trap.** On GNOME the name is served by the idle-inhibit implementation and refuses `GetActive`, so a detector built on it reports "unlocked" for ever |

### 14.1 One defect this wave found by running the gate, not by reasoning

The obvious implementation resolves the session with
`GetSessionByPID(self)`. It is wrong twice over, and the hardware said so
immediately:

* **It fails outright** for a daemon under `systemd --user`, whose processes
  live in `user@.service` and belong to no session at all. The first run of
  `real_lock.rs` failed with exactly that: `logind has no session for this
  process`.
* **When it does answer it can answer with the wrong session.** Measured on
  this host while the graphical session was locked:

  ```console
  … session/_32   Type=wayland       LockedHint=true
  … session/_33   Type=unspecified   LockedHint=false
  ```

  A user with a graphical login and a second, non-graphical one has two
  sessions; only one has a screen, and the other never locks. A sink that asked
  about it would report "unlocked" for ever — **fail-open**, on a privacy
  control, which is the exact failure this module exists to prevent.

The question is *"is the screen these notifications appear on locked?"*, so it
is answered by **`User.Display`** — logind's own name for the user's primary
graphical session, which is where the notification server lives. The two
PID-based answers remain as ordered fallbacks. `the_implementation_agrees_with_loginctl`
now pins it against a second tool reading the same session.

**Any answer other than a confident "unlocked" is locked.** No system bus, no
logind, no session, a property that will not deserialize, a call that errors —
all answer `true`, and it is decided in the seam so no caller has to remember.
A machine with no logind at all composes `UnknownLock`, which is not a stub
saying "unlocked because we do not know" but the fail-closed answer written
down as a type.

---

## 15. Destination privacy

The source has already reduced according to *its* policy. The sink does not
assume that means desktop privacy is solved, and applies its own:

| `when_sink_locked`, screen locked | summary | body | `redacted` |
| --- | --- | --- | --- |
| `Full` | full | full | as sent |
| `AppOnly` *(default)* | the application's name | *(empty)* | **true** |
| `Suppress` | *nothing is displayed, and any existing mirror is closed* | | |

The reduction runs **before** the `Mirror` is built, so what policy withholds is
never handed to the notification server at all. There is no "redact on the way
out" step further down that could be skipped, and nothing downstream receives a
full body plus a flag saying not to use it.

A `redacted = true` from the source is carried through as a presentation signal
and is **never** an instruction to reconstruct anything: what was withheld is
not there to reconstruct.

---

## 16. Locking reduces; unlocking does not restore

When the session locks, every mirror already on the screen is re-posted in its
reduced form **through `replaces_id`**, so the full body stops being visible
without a second notification appearing. GNOME advertises `persistence`, so
leaving the full text in the notification list would defeat the setting for
exactly the case it exists for — the screen somebody walked away from.

When the session unlocks, **nothing happens**. Restoring the full text would
mean this process had kept a title and a body in memory for the length of the
lock, waiting to redisplay them — a notification history by another name. The
mirror stays reduced until its source updates it, at which point the new content
arrives and is displayed in full. Both halves are tested
(`unlocking_does_not_restore_content_the_sink_never_kept`,
`a_reduced_mirror_shows_its_new_content_when_the_source_updates_it`), and the
first was observed on hardware (§27).

If a reduced re-post *fails*, the mirror is **closed**: a notification that
cannot be reduced must not stay legible on a locked screen.

---

## 17. Close-signal observation

`NotificationClosed(id, reason)` is subscribed and acted on **locally only**:
the mirror entry is dropped, because the server invalidates the id before it
sends the signal and a later `replaces_id` naming it would create a second
notification rather than replace the first. Verified on hardware —
`the_server_reports_our_own_close_with_reason_three` observes a real
`NotificationClosed` from gnome-shell with reason 3.

**No `DismissRequest` is sent to any peer, ever.** `git grep DismissRequest
desktop/capabilities/notifications/src` finds only prose and the inbound refusal
path. `no_dismiss_request_is_ever_sent_to_a_peer` closes a mirror as a human
would and asserts that the only thing that comes back over the session is an
ordinary result.

The four reasons are preserved accurately, and
`an_expiry_is_recorded_as_an_expiry_and_not_as_a_dismissal` pins the rule N4
will depend on.

---

## 18. Backend restart and recovery

`NameOwnerChanged` on `org.freedesktop.Notifications` is subscribed, so a GNOME
Shell restart is an **observed event** rather than a silent failure. On the
name being lost:

* the `SINK` role narrows to the empty set, with a strictly higher epoch, on
  every session that is already up — no reconnect;
* on the name being reacquired the role widens again, and **every stale numeric
  id is dropped while the identities are kept**. A restarted server renumbers
  from 1, so an id this process still held could now belong to somebody else's
  notification.

The next upsert then creates the notification instead of replacing a number
that now means something else, and the next snapshot reconverges. **Nothing is
persisted to survive the restart**: the phone is the thing that knows what is
active, and asking it again is free.

`a_notification_server_restart_invalidates_the_stale_ids` and
`a_snapshot_reconverges_after_a_server_restart` drive it against a fake that
reproduces the restart faithfully rather than helpfully — it clears everything,
renumbers from 1, and emits **no** `NotificationClosed`, because a shell that
died did not tell anybody what it was holding.

A `SinkError::Unavailable` from any ordinary call narrows the role the same
way, so a server that goes away between signals is caught too.

---

## 19. Rate limiting and backpressure

The session's dispatch loop **awaits** `on_message` before reading the next
frame, and every capability on a connection shares it. A `Notify` that took four
seconds because gnome-shell was wedged would therefore hold up `battery.v1`,
`files.v1` and `clipboard.v1` for four seconds. So `on_message` decodes,
validates, checks the grant and hands the rest to a **per-peer worker task**;
no D-Bus call ever happens on the dispatch loop.

That worker is also the **single ordered producer** for everything this device
sends that peer, so a result can never overtake the announcement before it.

The queue is bounded at 256 and coalescing:

1. an `Upsert` for an identity already pending **replaces it in place** — a
   progress bar updating sixty times a second occupies one slot, always the
   current one;
2. a `Remove` supersedes a pending `Upsert` for the same identity, and still
   takes its own slot;
3. **coalescing never crosses a snapshot marker**, a lock change or a bulk
   close — merging across a `BEGIN` would mark an identity as named in a
   snapshot it was not part of, and so protect it from a removal that should
   have happened;
4. at the ceiling the **oldest non-terminal** item is evicted;
5. only if every pending item is terminal is a terminal one dropped, and that is
   **counted** (`dropped_terminal`), logged at `warn`, and answered
   `RATE_LIMITED`. Reaching it needs 256 simultaneous removals for one peer.

Every backend call additionally has a `BACKEND_TIMEOUT` of its own, because a
bound that lives only inside an implementation is one an implementation can
forget.

**One peer's flood costs that peer its own oldest pending update and costs
nobody else anything**: the queue is per peer, the mirror ceiling is per peer,
and `one_peers_flood_does_not_evict_another_peers_mirrors` sends 250
notifications from one peer and asserts the other still holds its one.

---

## 20. Logging audit

Thirty-three `tracing` call sites were added. Every interpolated value is one of: a
short peer fingerprint, the first 8 hex characters of an already-opaque
`notification_id`, an event kind, an outcome enum name, a role count, an epoch,
a mirror or queue count, a `SnapshotRejection` or `Rejection` debug value
(which carry a field *name* and a length, never a value), a backend error
**class**, or a lock policy name.

`tests/logging.rs` proves it by construction rather than by grep: seven tests
run full flows — display, update, duplicate, remove, four kinds of refusal, a
backend failure whose error message *contains the canaries*, the lock
reduction, the close signal, the snapshot, and a suppressing policy — with a
`tracing` subscriber capturing **`TRACE`**, and assert the canaries do not
appear. Each also asserts the capture was non-empty, so none of them can pass
vacuously.

Then the same thing was done against the real daemon on real hardware, at
`debug` (§29): **every notification line the daemon emitted, in full**, and zero
hits for any fixture title, body, tag, package name or application label.

---

## 21. Persistence audit

**New persisted state, in full:**

| Where | Field | Contents |
| --- | --- | --- |
| `state.json` | `peers[].notification_policy` | `allow_mirror`, `when_sink_locked`, `allow_dismiss_sync` |

**Nothing else.** No notification history in any form. No title, body,
application label, application id, `notification_id`, `content_hash`, server id
or snapshot is written anywhere, at any time, on disk. The mirror table is
memory-only and has no serde derive.

`no_notification_content_is_written_to_disk` runs a full flow with canaries and
then reads **every byte the daemon wrote**, recursively, asserting none of them
appear — and asserting the policy *is* there, so the test is looking at the
right file. Repeated against the real daemon at §29.

---

## 22. Unit and fake-backend tests

| Suite | Tests |
| --- | ---: |
| `anyflow-capability-notifications` unit (`--lib`) | **50** |
| `tests/sink.rs` — the whole sink against a fake desktop | **60** |
| `tests/logging.rs` — NOTIF-SEC-25, in process | **7** |
| `daemon/tests/notifications.rs` — over real TLS | **18** |
| `core/tests/portable_boundary.rs` — two new | **+2** |
| **Total added, portable** | **137** |

Coverage against the brief's required list: fake backend · first upsert creates
· update replaces the same mirror · identical `content_hash` does not spam ·
`content_hash` is not recomputed · two peers with the same id do not collide ·
remove known · remove unknown · snapshot BEGIN/upserts/END convergence ·
malformed snapshot removes nothing · server restart and stale ids · backend
unavailable isolated · lock locked · lock unknown fails closed · role
SOURCE/SINK rules · grant missing · grant revoked · markup injection · map
bound · queue bound · no persistence · no content in logs or errors · existing
capability isolation. **All present and green.**

Two rules earned their own transport-level tests because they only exist out
there: `a_removal_never_overtakes_its_upsert_across_the_transport` drives 25
post/remove pairs through the real session and asserts no identity's last word
was "displayed"; and
`a_broken_notification_sink_does_not_break_battery_or_the_clipboard` wedges the
sink, confirms the notification fails, and then confirms `battery.v1` still
delivers and the session still answers a ping on the same connection.

---

## 23. Real D-Bus tests

`tests/real_dbus.rs` (6) and `tests/real_lock.rs` (2) are whole-file
`#![cfg(feature = "linux-dbus")]` and `#[ignore]`d, so they compile everywhere
and run only when asked for by name. Executed against this session:

```console
$ cargo test -p anyflow-capability-notifications --test real_dbus -- --ignored --test-threads=1 --nocapture
server: org.freedesktop.Notifications — gnome-shell 50.4 (spec 1.2, GNOME)
capabilities: SinkCapabilities { body_markup: true, body: true, persistence: true }
ids: 8 -> 8 -> 8
test result: ok. 6 passed; 0 failed

$ cargo test -p anyflow-capability-notifications --test real_lock -- --ignored --nocapture
lock source: org.freedesktop.login1.Session.LockedHint on /org/freedesktop/login1/session/_32
LockedHint via this implementation: true
loginctl, session 2:
Type=wayland
LockedHint=yes
test result: ok. 2 passed; 0 failed
```

Recorded: the server's own identification, its capability set, `replaces_id`
preserved across three calls, `CloseNotification` on an id that never existed
returning success, a double close returning success, a real
`NotificationClosed` with **reason 3**, an escaped body accepted, and a
low-urgency notification accepted.

**Every notification these tests post, they close**, and the fixture text is
synthetic and non-sensitive throughout. `real_lock.rs` deliberately does **not**
lock the screen: a suite that made the maintainer type their password to get
back to work would not be run twice. The locked half is proved deterministically
in `sink.rs` and on the real session in §28.

---

## 24. The Windows MSVC portable gate

The crate is in the portable set and holds the boundary:

```console
$ cargo tree --no-default-features -e normal -p anyflow-capability-notifications
anyflow-capability-notifications v0.1.0 []
anyflow-core v0.1.0 []
anyflow-proto v0.1.0 []
```

No `zbus`, no `futures-lite`, no `unix-fs` unified back on. All test targets
compile with `--no-default-features`.

`portable_boundary.rs` gains **two new tests**, because the existing markers
would not have caught the failure that actually threatens this crate.
`PLATFORM_MARKERS` catches a crate reaching for the *operating system*
(`std::os`, `target_os`). It does not catch one reaching for a *desktop*: `zbus`
is pure Rust and `"org.freedesktop.Notifications"` is just a string, so a
`zbus::Connection` could appear in `lib.rs` and the gate would stay green right
up until the MSVC job went red for a reason nobody had encoded as a rule.

* `the_notification_seam_names_no_desktop_outside_its_two_backend_modules`
  refuses `zbus`, `org.freedesktop`, `org.gnome`, `org.kde`, `dbus`, `logind`
  and `gnome-shell` in executable code anywhere in the crate except the two
  declared backend modules. Prose is exempt, and so is the `#[cfg(feature)]`
  and `pub mod` pair that declares the boundary.
* `each_desktop_module_exists_and_is_behind_the_linux_feature` fails if either
  module disappears or stops being feature-gated — because the exception is
  only defensible while turning the feature off removes the module.

CI (`.github/workflows/portable-windows-msvc.yml`) was updated **deliberately**
and nothing was weakened: the crate is added to the package-set check, the
`check`, `build`, `test --no-run`, dependency-tree and unsafe-policy steps;
`linux-dbus` joins the forbidden-feature list; and a new
*Notification test files unchanged* guard fails if the crate's test set changes
without someone classifying the new target as portable or Linux-only. No test
was excluded from MSVC to make anything pass.

---

## 25. Rust regression

Run from `desktop/`, with the documented GTK prefix sourced:

```console
$ cargo fmt --all --check                                        FMT OK
$ cargo build --workspace --locked -j 2                          Finished
$ cargo test --workspace --locked -j 2       574 passed; 0 failed; 17 ignored
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
                                                                 Finished, no warnings
```

The 17 ignored are the platform gates that must be asked for by name: 9
clipboard `real_backend`, 6 `real_dbus`, 2 `real_lock` — and all 8 of the new
ones were executed (§23).

`anyflow-core`'s `notifications_protocol` suite is **47 tests, unchanged from
N1** — no portable contract was touched. `anyflow-proto`'s
`notifications_schema` descriptor guard is **12, unchanged**.

Two pre-existing daemon tests were updated rather than worked around:
`pairing_then_ping_pong_then_battery` and
`a_paired_device_reconnects_without_pairing_again` assert the *mutually
supported* capability list, which now legitimately includes `notifications.v1`.
Both were strengthened at the same time to assert it is **not** auto-granted,
alongside `files.v1` and `clipboard.v1`.

---

## 26. Android and protocol guards

```console
$ git diff -- android/
(empty)

$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
(empty)
```

**N1's contract was sufficient.** No implementation blocker required a schema
change and none was made; no Android production file was edited, and the
end-to-end gate below ran against the N1 build already installed on the tablet.

---

## 27. End-to-end: SM-X620 → Fedora

Executed 2026-09-09 against the tablet on the LAN and the daemon on this
machine. Preconditions were set programmatically because N3's picker does not
exist yet, and everything set is a **setting** — the tablet's own trust store
was saved first and nothing touched notification content.

| # | Requirement | Result |
| --: | --- | --- |
| 1 | Pairing / session | **PASS** — the N1 pairing survived; peer `7E63 7B4E 937B 7732` |
| 2 | `notifications.v1` explicitly granted | **PASS** — by hand on both ends; pairing granted it on neither |
| 3 | Android notification access enabled | **PASS** — `cmd notification allow_listener` |
| 4 | Exactly one source package allowed | **PASS** — `com.android.shell` |
| 5 | Linux advertises `SINK` | **PASS** |
| 6 | Android advertises `SOURCE` | **PASS** |
| 7 | Post a synthetic notification | **PASS** |
| 8 | A real notification appears on Fedora | **PASS** |
| 9 | Update the same notification | **PASS** |
| 10 | Fedora replaces it, does not duplicate | **PASS** |
| 11 | Remove it on Android | **PASS** |
| 12 | The Fedora mirror disappears | **PASS** |
| 13 | Post from a denied app | **PASS** |
| 14 | Nothing appears on Fedora | **PASS** |
| 15 | AnyFlow's own notification | **PASS** |
| 16 | Nothing mirrors | **PASS** |
| 17 | Reconnect / session restart | **PASS** |
| 18 | The snapshot converges without a duplicate wall | **PASS** |

### 27.1 The grant is two-sided, and it showed

The first connection after granting `notifications.v1` on **Fedora only**
produced this, live:

```text
session established  peer=7E63 7B4E 937B 7732 capabilities=["battery.v1", "clipboard.v1", "notifications.v1"]
announcing roles     peer=7E63 7B4E 937B 7732 roles=1 epoch=1
peer roles           peer=7E63 7B4E 937B 7732 roles=0 epoch=1
```

The desktop announced `SINK`; the tablet announced **nothing**, because it had
not granted `notifications.v1` to Fedora on *its* side, so N1 had no eligible
peer, did not bind its listener, and correctly claimed no source ability. That
is NOTIF-SEC-01 and -03 observed cross-device, before a line of content moved.

### 27.2 The listener binds only while an eligible peer is connected

Before any peer was eligible, with notification access **already approved**:

```console
$ adb shell dumpsys notification | sed -n '/Live notification listeners/,/Snoozed/p' | grep -c yurisismotto
0
```

Approved and **not bound** — `META_DATA_DEFAULT_AUTOBIND=false` honoured, ADR-0015 §3.
After the grant landed on both ends, the same command answered `1`.

### 27.3 The role widened mid-session, with no reconnect

```text
announcing roles  peer=7E63 7B4E 937B 7732 roles=1 epoch=1     ← Fedora: SINK
peer roles        peer=7E63 7B4E 937B 7732 roles=0 epoch=1     ← the tablet, listener not yet bound
peer roles        peer=7E63 7B4E 937B 7732 roles=1 epoch=2     ← the tablet: SOURCE, strictly higher epoch
snapshot opened
…
snapshot complete peer=7E63 7B4E 937B 7732 named=4 closed=0
```

ADR-0017 on hardware: the peer widened its role on the session that was already
up, and immediately sent its active-state snapshot.

### 27.4 Create, update, remove — the session bus, verbatim

Captured with `dbus-monitor --session` on
`interface='org.freedesktop.Notifications'`, `:1.332` being `anyflowd` and
`:1.25` gnome-shell re-emitting:

```text
:1.332 Notify replaces=0  app='Shell' summary='ANYFLOW-N2-GATE-TITLE' body='ANYFLOW-N2-GATE-BODY-V1'
:1.332 Notify replaces=19 app='Shell' summary='ANYFLOW-N2-GATE-TITLE' body='ANYFLOW-N2-GATE-BODY-V2-CHANGED'
:1.332 CloseNotification id=19
```

and the daemon's own view of the same three events:

```text
notification upsert   notification=ff7592a3 outcome="displayed"
notification upsert   notification=ff7592a3 outcome="displayed"
notification removal  notification=ff7592a3 outcome="removed"
```

**One identity, one freedesktop id, replaced in place, then closed.** The
identity `ff7592a3` is stable while the content changes, which is
`CONTENT IS NEVER IDENTITY` measured rather than asserted.

The `desktop-entry` hint naming AnyFlow was present on every `Notify`, as
designed:

```text
dict entry( string "urgency"       variant byte 1 )
dict entry( string "desktop-entry" variant string "io.github.yurisismotto.anyflow" )
```

`urgency` is **1**, never 2, on every call.

### 27.5 Deny by default, against a real notification shade

This is the strongest single piece of evidence in the wave, and it was not
synthetic. At the moment of the reconnect snapshot the tablet's shade held
**about 85 notifications** — the maintainer's own mail, social, shopping,
banking and messaging apps, plus system ones. The snapshot named **four**, and
all four were from the one allowed package:

```text
snapshot complete  peer=7E63 7B4E 937B 7732 named=4 closed=0
```

```console
$ adb shell cmd notification list | grep -c com.android.shell
4
$ adb shell cmd notification list | wc -l
85
```

**Eighty-one notifications from real applications stayed on the phone**, because
their packages were not in the allow-list. No content from any of them reached
this machine, the daemon log, this report, or the capture files. That is items
13 and 14, and it is `1 of 59 active` from N1's report seen from the other end.

### 27.6 AnyFlow's own notifications never mirror

During the run the tablet's shade held **three** notifications from
`io.github.yurisismotto.anyflow` — the ongoing connection notification, its
group summary, and a "Clipboard received from Fedora" raised by the §30
regression. The mirror count stayed at 4 throughout, and the snapshot named
four shell notifications and nothing else. Items 15 and 16: the own-package drop
is applied at the source, first, before any other check, and there is no setting
that turns it off.

### 27.7 Reconnect: the duplicate wall that did not happen

The session was dropped from the tablet's own **Disconnect** button, held
through the grace, and reconnected:

```text
—— during the disconnect ——
mirrored now  4
D-Bus calls   0            ← nothing closed, nothing re-posted

—— on reconnect ——
session established
announcing roles  roles=1 epoch=1
peer roles        roles=0 epoch=1
peer roles        roles=1 epoch=2
snapshot opened
notification upsert  notification=ed87f415 outcome="duplicate"
notification upsert  notification=727d6705 outcome="duplicate"
notification upsert  notification=f5f343a7 outcome="duplicate"
notification upsert  notification=3c01650d outcome="duplicate"
snapshot complete    named=4 closed=0
D-Bus calls          0            ← not one Notify, not one Close
mirrored now         4
```

All four identities were **byte-identical across the reconnect** — `ed87f415`,
`727d6705`, `f5f343a7`, `3c01650d` before and after — which is ADR-0016's whole
argument for a derived id rather than a random one, measured. Every upsert
answered `DUPLICATE`, and **zero** D-Bus calls were made. Items 17 and 18.

### 27.8 Teardown, which is item 11–12 four more times

Clearing the four fixtures from the tablet produced exactly four closes and an
empty screen:

```text
:1.332 CloseNotification id=15
:1.332 CloseNotification id=18
:1.332 CloseNotification id=17
:1.332 CloseNotification id=16
mirrored now  0
```

---

## 28. Lock hardware gate

| | State | Result |
| --- | --- | --- |
| **A** | Fedora **unlocked** | **PASS** — `Notify(replaces=0, summary='ANYFLOW-N2-GATE-TITLE', body='ANYFLOW-N2-GATE-BODY-V1')`: the full body reached gnome-shell |
| **B** | Fedora **locked** | **PASS** — see below |
| **C** | the lock probe **cannot answer** | **PASS** — see below |

**B, and it was the live-reduction path rather than the easy one.** A
notification arrived while the screen was unlocked and was displayed in full;
the session then locked, and the sink re-posted the mirror already on the screen
in its reduced form, through `replaces_id`:

```text
:1.320 Notify replaces=0  app='Shell' summary='ANYFLOW-N2-FIXTURE-TITLE' body='ANYFLOW-N2-FIXTURE-BODY-ONE'
:1.320 Notify replaces=11 app='Shell' summary='Shell'                    body=''
```

Same notification, no duplicate, title and body gone from the locked screen.
That is §16 and 01 §6.2 on real hardware, and it is the case a naive
implementation misses entirely.

**C** was forced by taking the system bus away from a second daemon instance
while leaving the session bus intact, so only logind was unreachable:

```console
$ DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent-system-bus anyflowd …
WARN anyflowd: no logind session to read LockedHint from; this desktop will be
     treated as locked, so notifications will be reduced

$ anyflow notifications status
  server        org.freedesktop.Notifications — gnome-shell 50.4 (spec 1.2, GNOME)
  reachable     yes
  lock state    no lock-state source on this platform; treated as locked
  screen        LOCKED
```

The real session was **unlocked** at that moment. The detector could not tell,
so it said locked. Fail-closed, on hardware.

`loginctl show-user <uid> -p Display` → `Display=2`, and
`loginctl show-session 2 -p LockedHint` was the authority throughout; no
`org.gnome.ScreenSaver` boolean was consulted anywhere. **The maintainer was not
asked to type a secret at any point**: the one unlock used
`loginctl unlock-session 2`.

---

## 29. Security and store audit, on the real machine

**Store.** Every byte the daemon wrote, after a full end-to-end flow:

```console
$ find ~/.local/share/anyflow -type f
/home/yuri/.local/share/anyflow/identity.key
/home/yuri/.local/share/anyflow/state.json

$ grep -rlE "ANYFLOW-N2-(GATE|FIXTURE)|anyflow-n2-|ANYFLOW-N2-CLIP" ~/.local/share/anyflow ~/.config/anyflow
(no matches)
```

The only new thing in `state.json` is the policy, which holds settings and has
no field that could hold anything else:

```json
"notification_policy": {"allow_mirror": true, "when_sink_locked": "app_only", "allow_dismiss_sync": false}
```

**No notification history file, no database, no cache.**

**Logs.** The daemon ran the whole gate at
`--log info,anyflow_capability_notifications=debug`. This is the complete set
of notification lines it emitted, deduplicated:

```text
  2  snapshot opened peer=7E63 7B4E 937B 7732
  2  snapshot complete peer=7E63 7B4E 937B 7732 named=4 closed=0
  2  peer roles peer=7E63 7B4E 937B 7732 roles=1 epoch=2
  2  peer roles peer=7E63 7B4E 937B 7732 roles=0 epoch=1
  2  announcing roles peer=7E63 7B4E 937B 7732 roles=1 epoch=1
  2  notification upsert  … notification=ff7592a3 outcome="displayed"
  1  notification removal … notification=ff7592a3 outcome="removed"
  1  notification upsert  … notification=f5f343a7 outcome="duplicate"
  1  notification upsert  … notification=f5f343a7 outcome="displayed"
  1  notification upsert  … notification=ed87f415 outcome="duplicate"
  1  notification upsert  … notification=ed87f415 outcome="displayed"
  1  notification upsert  … notification=727d6705 outcome="duplicate"
  1  notification upsert  … notification=727d6705 outcome="displayed"
  1  notification upsert  … notification=3c01650d outcome="duplicate"
  1  notification upsert  … notification=3c01650d outcome="displayed"
  1  lock state from logind LockedHint session=/org/freedesktop/login1/session/_32 locked=false
  1  notification server server=gnome-shell vendor=GNOME version=50.4 spec=1.2 body_markup=true persistence=true
  1  logind Display session session=2
```

Searched for every fixture string, both tags, the package name **and the
application label**:

| Needle | Hits |
| --- | ---: |
| `ANYFLOW-N2-GATE-TITLE` | **0** |
| `ANYFLOW-N2-GATE-BODY` | **0** |
| `ANYFLOW-N2-FIXTURE` | **0** |
| `anyflow-n2-gate` / `anyflow-n2-fixture` | **0** |
| `com.android.shell` | **0** |
| `Shell` (the resolved app label) | **0** |
| `ANYFLOW-N2-CLIP` | **0** |

No logcat or D-Bus capture is committed to git; they live in the session
scratchpad.

---

## 30. Existing-feature regression

| Area | Result |
| --- | --- |
| Rust workspace (574 tests) | **green** |
| `cargo clippy --all-targets -- -D warnings` | **clean** |
| Pairing, TLS, SPKI, replay guard, framing | **untouched** — no diff under `core/src/tls.rs`, `session.rs`, `pairing.rs`, `fingerprint.rs`, `framing.rs` |
| `battery.v1`, `files.v1`, `clipboard.v1` | **untouched** — no source change; their suites are green |
| Live `ping` over the same session | **PASS** — `pong in 69 ms` |
| Live `battery/status` | **PASS** — `80% (NotCharging)`, read live |
| Live Fedora → Android clipboard | **PASS** — `sent 26 bytes`; the tablet raised its "Clipboard received from Fedora" notification |
| `files.v1` routing | **untouched** — no shared routing change; `anyflow transfers` healthy |

Shared control/session code was **not** changed — the only edits outside the new
crate are the trust-store field, the daemon composition, the control protocol
and the CLI — so the escalated regression scope does not apply and full Wave 0
recertification is not required.

---

## 31. Remaining risks and debts

1. **N2 sends no `DismissRequest`, so a mirror closed on the desktop stays on
   the phone.** Deliberate, announced honestly (no `DISMISS_REPORTER` role), and
   N4's whole subject. Users will experience it as the feature being incomplete
   rather than broken.
2. **`content_hash` must stay one-sided.** N2 does not recompute it and N3/N4
   must not start, or a source-side optimisation becomes a wire-format contract.
   Flagged in the crate docs, in `mirror.rs`, and here.
3. **Unlocking does not restore full content.** Correct by design (§16), and it
   will read as a bug to somebody. It is the direct consequence of refusing to
   hold a title and body in memory across a lock, and the alternative is a
   notification history.
4. **The per-peer app allow-list has no UI.** N3 owns it. Until then the only
   way to name an application is to edit the phone's trust store, which is what
   the gate above did. This is the single largest gap between "the code works"
   and "a person can use it".
5. **`RECONNECT_GRACE` is still chosen, not measured** (OQ-06). Bounded and
   local; N5 owns the tuning.
6. **KDE Plasma is untested.** The same interface, the same code, and its
   `CloseNotification` error behaviour is the one place the spec and GNOME
   disagree — which is exactly why the sink treats that error as success.
   POC-NOTIF-03 is still one `gdbus` transcript long.
7. **The adb/MTP flakiness recurred and is not fixed.** The tablet dropped to
   MTP-only mid-session; a host-side `usbreset` re-enumerated it but did not
   restore adb, and a physical replug did. The AnyFlow TLS session was
   unaffected throughout. Mitigation remains replug-and-retry.
8. **The `logging.rs` canary suite for the Android side (NOTIF-SEC-25) is still
   N3's.** N2 has the desktop half, in process and against a real daemon.
9. **The lock detector resolves one session.** A user with two graphical
   sessions gets `User.Display`'s, which is the right answer for a desktop with
   one screen and an approximation for anything stranger. Documented in
   `logind.rs`.

**No BLOCKER and no P0 remains.**

---

## 32. Git status at the end

```console
$ git status --short
 M .github/workflows/portable-windows-msvc.yml
 M desktop/Cargo.lock
 M desktop/Cargo.toml
 M desktop/cli/src/main.rs
 M desktop/control/src/lib.rs
 M desktop/core/src/lib.rs
 M desktop/core/src/store.rs
 M desktop/core/tests/identity_and_store.rs
 M desktop/core/tests/portable_boundary.rs
 M desktop/daemon/Cargo.toml
 M desktop/daemon/examples/fake_phone.rs
 M desktop/daemon/src/main.rs
 M desktop/daemon/tests/common/mod.rs
 M desktop/daemon/tests/e2e.rs
 M desktop/runtime/Cargo.toml
 M desktop/runtime/src/server.rs
 M desktop/runtime/src/state.rs
?? desktop/capabilities/notifications/
?? desktop/core/src/notification_policy.rs
?? desktop/daemon/tests/notifications.rs

$ git diff --check
(clean)

$ git diff --stat
 17 files changed, 1015 insertions(+), 19 deletions(-)
```

Scanned the changed and untracked set for `*.key`, `*.pem`, `*.p12`, `*.pfx`,
`*.jks`, `*.keystore`, `*.apk`, `*.aab`, `*.log`, `*.png`, `state.json`, trust
stores, captures, `target/` and `build/`: **no matches**. The new crate is 17
files, all `.rs` and one `.toml`.

**Nothing was committed, nothing was pushed, no PR was created.**

### Machine state left behind

* The daemon running on this machine is now the **N2 build**, started by hand
  from `desktop/target/debug/anyflowd`.
* The desktop session was **unlocked** with `loginctl unlock-session 2` for
  gate 28-A and is currently unlocked.
* The tablet keeps its notification-listener grant, the `notifications.v1`
  grant to Fedora, and `com.android.shell` in its allow-list. Its original
  trust store and `enabled_notification_listeners` value are saved in the
  session scratchpad. The four synthetic fixtures were cleared from its shade.

---

## 33. Recommended commit message

```
feat(desktop): implement the notifications.v1 Linux sink

A notification posted on the phone now appears on this desktop, as a
real freedesktop notification over the existing TLS session. Updating it
replaces it in place; removing it closes it; a denied application never
leaves the phone; a reconnect restores the screen without a duplicate.

The NotificationSink seam is created here rather than in Wave 0, beside
the first implementation, because the shape is what writing the D-Bus
sink teaches. Two things changed from the sketch: display() returns a
server id that is always stored, since only a non-zero replaces_id is
promised to come back; and capabilities() exists, because whether to
escape a body is a question only the backend can answer.

The mirror table is keyed on (peer fingerprint, notification id), so a
peer can only ever address its own mirrors — two phones sending the same
16 bytes are two notifications, and a replayed id reaches an empty
namespace. It holds a server id, the source's opaque content_hash, an
app name and an urgency, and there is no field on it that could hold a
title or a body.

The sink never recomputes content_hash. It compares the digest it was
sent last with the one it is sent now, because a digest only one side
computes is a source-side optimisation, and the moment a sink recomputes
it the construction becomes a wire-format contract for ever.

Lock state comes from logind's LockedHint on the user's Display session
— not from this process's own session, which under systemd --user does
not exist and on a machine with an ssh login can be one that has no
screen and therefore never locks. Anything other than a confident
"unlocked" is locked. Locking re-posts what is on screen reduced,
through replaces_id; unlocking restores nothing, because restoring would
mean having kept a body in memory across the lock.

Display work happens on a per-peer task behind a bounded coalescing
queue, so a wedged notification server cannot stall the session's
dispatch loop and take battery, files and clipboard down with it. That
task is also the single ordered producer, so a removal never overtakes
the upsert it refers to.

notifications.v1 is advertised unconditionally and granted by hand;
withdrawing the grant closes the notifications already on the screen,
not just the ones that have not arrived. Linux announces SINK and only
SINK: it observes desktop closes and records their reasons, and sends no
DismissRequest, because that is N4.

137 new portable tests; 8 gates against the real gnome-shell and the
real logind session; an Android -> Fedora end-to-end run on the SM-X620
in which 81 of 85 real notifications correctly stayed on the phone. No
notification content in the daemon log at debug, or in state.json,
verified on the machine. notifications_v1.proto is unchanged and no
Android source was touched.
```

---

## 34. Recommendation for N3

N3 owns the consent surface, which is the part that decides whether this
feature is defensible. Six things this wave learned that it should carry:

1. **The app picker is the critical path, and it is the only one.** Everything
   else works today; without a picker the only way to share an application is to
   edit a JSON file on the phone. §27.5 is the argument for the picker's
   *default*: eighty-one real notifications stayed on the phone because the list
   was empty, and that is the behaviour to preserve, not soften.
2. **Show the three gates separately, because they fail separately.** Android's
   notification access, the per-peer grant, and the announced roles are three
   different questions, and the gate output above shows a real state where two
   were satisfied and the third was not — Fedora had granted, the tablet had
   not, and the honest symptom was `roles=0`. A UI that showed one switch would
   have made that state unexplainable.
3. **`anyflow notifications status` already reports what N3's screens need**:
   the server, its capabilities, the lock source and session path, the lock
   state, the mirror and queue counts, and each peer's grant, policy and role
   epochs. Read it rather than adding parallel plumbing, and keep its property
   that no field can hold content.
4. **Do not add an unlock-restores-content setting.** It cannot be implemented
   without keeping a body in memory across the lock, which is a notification
   history. If users ask, the answer is that the next update arrives in full.
5. **`allow_dismiss_sync` is stored and inert.** N3 may surface it; the CLI
   already labels it "inert in this release". It must stay default-off until N4
   makes it real, and the switch should say so rather than implying it works.
6. **The NOTIF-SEC-25 canary suite for the Android side is N3's.** N2 built the
   desktop half — `tests/logging.rs` — and it is a template worth copying: run
   real flows at `TRACE` with canaries, assert the capture is non-empty so it
   cannot pass vacuously, and include the *error* paths, which are where a
   content leak actually hides.

---

NOTIFICATIONS.V1 N2 PASS
N3 READY FOR IMPLEMENTATION

*A notification posted on the SM-X620 appeared on Fedora as a real GNOME
notification; updating it replaced it in place through `replaces_id`; removing
it closed it; 81 of 85 real notifications never left the phone; AnyFlow's own
three never mirrored; and a disconnect-and-reconnect restored the screen with
zero D-Bus calls and zero duplicates. The lock gate passed in all three states,
including the forced-failure one, where a detector that could not read the
session reported LOCKED while the session was in fact unlocked.*

*The Rust workspace is 574 tests green with clippy clean, the portable Windows
gate holds with two new tests defending it, `notifications_v1.proto` is
unchanged, and `git diff -- android/` is empty. Nothing was committed, pushed,
or opened as a PR.*
