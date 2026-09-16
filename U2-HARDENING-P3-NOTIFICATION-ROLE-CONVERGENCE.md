# U2 HARDENING — P3: Notification Role / Capability Convergence Without Restart

**Branch:** `fix/u2-p3-notification-role-convergence`
**Base:** `72188c6` (develop, with P1 multi-peer and P2 battery absence merged)
**Date:** 2026-09-16
**Scope:** the single U2 defect P3. Nothing else.

> `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is **not** modified by this branch. It remains
> untracked evidence for baseline `9792330`, describing the code as it was certified —
> with the defect present. This report is the separate remediation record.

---

## 1. Defect of record

On a paired, healthy, **connected** session, turning notification sharing on did nothing.
The desktop showed `showing 0 of 0 mirrored` and the phone said *"the computer has not
said it can show notifications."* Restarting `anyflowd` fixed it within seconds.

U2 recorded it three times, on three distributions and three GNOME versions:

| | Ubuntu 24.04 (§34C.5b) | Debian 13 (§39.19) | Ubuntu 26.04 (§40.13) |
|---|---|---|---|
| GNOME | 46.0 | 48.7 | 50.1 |
| Converges on its own? | **No** | **No** | **No**, 100 s / 5 samples |
| Restart required? | **Yes** | **Yes** | **Yes** |
| After restart | `showing 3 of 3` | `showing 3 of 3`, < 5 s | `showing 4 of 4`, ≤ 15 s |

The characteristic reading, identical on all three:

```text
desktop:  this desktop announced 2 (epoch 1); the device can source notifications (epoch 2)
          showing 0 of 0 mirrored
phone:    This device announces   SOURCE + DISMISS_TARGET · epoch 2
          The computer announces  no role · epoch 0          ← nothing was ever recorded
```

**Reproduced live in this session** on Debian 13, against the pre-P3 APK, before any fix was
loaded — see §18. The phone's half of the contract converged perfectly. The desktop's half
never arrived.

---

## 2. Root cause

**Android refused the peer's role announcement whenever the peer did not yet hold a
`notifications.v1` grant — and the desktop announces its roles exactly once per session.**

`NotificationSource.handleInbound` applied the per-peer grant check to **every** inbound body:

```kotlin
val session = sessions[peer.toHex()] ?: return
if (policyFor(peer) == NotificationPolicy.DENIED) {
    answer(session, control, NOTIFICATION_OUTCOME_NOT_AUTHORIZED)
    return                       // ← ROLES died here
}
when (control.bodyCase) {
    NotificationControl.BodyCase.ROLES -> { session.peerRoles.apply(control.roles) ; … }
```

Five facts compose into the defect, and no single one of them is wrong on its own:

1. The desktop announces its roles **once**, from `attach_session`, and `LocalRoles::announce`
   answers `None` for every later call with the same set. There is no second announcement and
   nothing asks for one.
2. The app **deliberately withholds `notifications.v1` at pairing** — `AnyFlowApp.kt` grants
   `negotiatedCapabilities - Clipboard - Notifications`, because a computer that can see this
   phone's notifications sees banking alerts and 2FA codes. So *the ordinary order of events*
   is: pair, connect, then turn sharing on.
3. That puts the desktop's single announcement squarely inside the window where the phone had
   no grant for it, so it was refused.
4. `answer()` has no id to echo for a `ROLES` body and returns without sending. The refusal was
   therefore **silent** — no reply to the desktop, and no line in either log. Nothing in U2's
   evidence could have pointed at it.
5. Both of the phone's send paths — `sendSnapshot` and `handlePosted` — are gated on
   `peerRoles.has(SINK)`. With the announcement discarded, `PeerRoleState` stayed at epoch 0 for
   the life of the session and **nothing could ever be sent**.

A daemon restart worked because it built a *new* session, whose announcement arrived at a moment
when the grant already existed.

**The desktop has always been the correct side.** `handle_control` checks the grant **per body**
and exempts `Roles` deliberately. The fix is the two ends agreeing.

### Why refusing a role announcement was wrong in principle, not only in effect

A role says what a peer **can** do; a grant says what it is **allowed** to do, and ADR-0017 §6
answers the two in two places on purpose. `PeerRoleState` is read for exactly two things — *do
not send content to a peer that never claimed `SINK`*, and *tell the UI whether the computer
claims `DISMISS_REPORTER`* — so recording one can only ever **withhold**. It cannot widen
anything: every outbound path re-reads the grant for itself and every other inbound body is still
refused without one. The check bought no safety and cost the whole feature.

### Two further convergence gaps, both found by this work

Fixing the root cause alone left the user journey still broken, in two places. Both are the same
defect family — *state moved and nothing told the capability* — and both were found on hardware,
not by reasoning:

**(b) `MainActivity.onResume` re-read Android's notification-access grant for the screen and told
nobody.** The OS listener permission is one of the three independent inputs to whether this
device can source at all. A person who granted access in Android's settings and came back got a
consent screen reading "Allowed" over a session that had never asked the system to bind the
listener and had never re-announced its roles. Nothing converged until some *other* in-app write
happened to call `policyChanged()` as a side effect — and picking an app was usually that write,
which is why it could look like it worked.

**(c) A snapshot was owed once per connection, never again.** Turning sharing on and only *then*
choosing which apps to share is the ordinary order, and it produced a correctly-empty snapshot
followed by no second one. Everything already on the shade stayed invisible; only the next
notification to arrive was mirrored. Caught live on Ubuntu 24.04 (§19): `snapshot sent: 0 of 49
active`, then ticking the fixture app produced **nothing at all**.

---

## 3. Source audit

The complete role lifecycle, both sides, traced before anything was changed.

### Android

| Stage | Where |
|---|---|
| Platform readiness | `NotificationSource.isSourcing()` — listener bound **and** OS access granted **and** a usable secret. The peer grant is deliberately not an input. |
| Local roles + epoch | `SourceRoleState.announce()` — returns null for an unchanged set, so the epoch counts changes, not events. |
| Announcement triggers | `SessionAttached`, `ListenerConnected`, `ListenerDisconnected`, `PolicyChanged` — all on the one ordered producer coroutine. **Event-driven already, and correct.** |
| Peer roles | `PeerRoleState.apply()` — epoch 0 refused, `<=` refused, unknown role values dropped without discarding the set. |
| Inbound routing | `NotificationSource.handleInbound` — **the defect** (§2). |
| Send gates | `handlePosted` and `sendSnapshot`, both `peerRoles.has(SINK)` **and** a fresh `policyFor(peer)`. |

### Desktop

| Stage | Where |
|---|---|
| Platform readiness | `NotificationManager::is_available()` (a reachable server) and `reports_dismissals()` (a real close stream). |
| Local roles + epoch | `roles::LocalRoles::announce()` — same contract as the phone's, per connection, reset on attach and detach. |
| Announcement triggers | `attach_session` and `set_available` only. |
| Peer roles | `apply_peer_roles` → `PeerRoles::apply`, and a narrowing schedules `CloseAll(RoleNarrowed)`. |
| Inbound routing | `handle_control` — grant checked **per body**, `Roles` exempt. Correct as written. |
| Grant widening | `runtime::renegotiate` — a desktop-side grant that the live session cannot use ends that session, at most once per session (ADR-0017 §3). Untouched. |

### The two questions the audit had to answer

- **Where does the announcement die?** Not on the wire and not on the desktop. It is sent
  (`announcing roles … roles=2 epoch=1` in every daemon log) and discarded on arrival.
- **Why does the desktop never re-announce?** Because its roles are a function of the *platform*
  and nothing about the peer changed on the desktop — which is correct. The desktop had nothing
  to say. The phone simply never heard what it said the first time.

### One thing the audit found on the desktop and the fix corrects

`announce_roles` advanced `LocalRoles` **before** it took the outbound channel:

```rust
let announcement = { state.local_roles.announce(available, reporting) };   // epoch burned
let Some(announcement) = announcement else { return };
let Some(outbound) = slot.outbound.read().await.clone() else { return };   // …and nothing sent
```

`announce()` is a state transition, not a query. Recording an announcement that was never sent
loses it **permanently** for that session: the desktop goes on reporting `announced 2 (epoch 1)`
to a peer that was told nothing, and `announce()` will never produce it again. That is the P3
defect's exact shape, waiting on the other end of the wire. It is now unreachable by
construction — the channel is taken first.

---

## 4. Architecture before

```text
ANDROID                                    DESKTOP
                                           attach_session
                                             └─ announce  roles=2 epoch=1  ──────┐
SessionAttached                                                                  │
  ├─ announceRoles  (roles=0 epoch=1) ─────────────────► apply_peer_roles  OK    │
  └─ sendSnapshot → peerRoles.has(SINK)? NO → nothing                            │
                                                                                 │
handleInbound(ROLES)  ◄──────────────────────────────────────────────────────────┘
  └─ policyFor(peer) == DENIED           ← the phone has not granted this computer
       └─ answer(…, NOT_AUTHORIZED)      ← no id for a ROLES body → SENDS NOTHING
       └─ return                         ← PeerRoleState stays at epoch 0, for ever

person turns sharing on
  ├─ trustStore.setGrant(...) → policyChanged → announceRoles (roles=2 epoch=2) ─► desktop OK
  └─ sendSnapshot → peerRoles.has(SINK)? STILL NO → nothing

           result: showing 0 of 0 mirrored, until anyflowd is restarted
```

## 5. Architecture after

```text
ANDROID                                    DESKTOP
                                           attach_session
                                             └─ outbound taken FIRST, then announce
                                                  roles=2 epoch=1 ────────────┐
handleInbound                                                                 │
  ├─ bodyCase == ROLES ──► applyPeerRoles  ◄───────────────────────────────────┘
  │                          ├─ peerRoles.apply (epoch rule)
  │                          └─ converge(session)          ← no grant needed: a role
  │                                                          authorizes nothing
  └─ else ─► policyFor(peer) == DENIED → NOT_AUTHORIZED     ← unchanged, still fail-closed

converge(session)                     ← the single convergence seam
  ├─ announceRoles        (null for an unchanged set: no epoch, no traffic)
  ├─ !mirroringIsLive  →  snapshotShape = null     (authority lost: a later return resyncs)
  └─  mirroringIsLive  →  snapshotShape != shapeOf(policy) ? sendSnapshot : nothing

called from, and only from:
  SessionAttached · ListenerConnected · ListenerDisconnected · PolicyChanged · inbound ROLES
  ── plus MainActivity.onResume, which now reports a *changed* OS notification-access grant

           result: enable → roles epoch+1 → snapshot → mirrors, inside the live session
```

---

## 6. Role / epoch semantics

Unchanged from N0–N6 and re-asserted by test, not by assumption:

| Rule | Where it lives | Pinned by |
|---|---|---|
| A changed role set gets a strictly higher epoch | `SourceRoleState.announce` / `LocalRoles::announce` | Android D, desktop E |
| An unchanged set produces **no** announcement and burns no epoch | same | Android E, desktop D |
| Epoch 0 is "unset" and is refused | `PeerRoleState.apply` / `PeerRoles::apply` | Android "unset epoch", desktop K |
| `<=` is refused — *equal as well as lower* | same | Android F, G; desktop F, G |
| Epoch is per authenticated connection and restarts at 1 | `reset()` on attach and detach | Android H, desktop H |
| A role is never an authorization input | every send path re-reads the grant | Android I, desktop L |
| Announcements are idempotent | `announce` returns null; duplicate epochs inert | Android E, G |

**The epoch type, width and protobuf representation are untouched.** No new state was added to
the protocol; the one new piece of state is the Android session's `snapshotShape`, which is
per-connection, in memory, and never transmitted.

---

## 7. Files changed

| File | Δ | What |
|---|---|---|
| `android/.../notifications/NotificationSource.kt` | +160 / −21 | `ROLES` exempted from the grant check; `applyPeerRoles`; `converge`; `mirroringIsLive`; `SnapshotShape`; `snapshotSent` → `snapshotShape`. |
| `android/.../ui/MainActivity.kt` | +23 / −3 | `onResume` reports a *changed* OS notification-access grant to the capability. |
| `desktop/capabilities/notifications/src/lib.rs` | +16 / −2 | `announce_roles` takes the outbound channel before advancing `LocalRoles`. |
| `android/.../test/NotificationConvergenceTest.kt` | **new**, 19 tests | No Android framework; one ordered producer on the test scheduler. |
| `desktop/capabilities/notifications/tests/convergence.rs` | **new**, 14 tests | No D-Bus, no sleeps. |

```text
git diff --stat:  3 files changed, 207 insertions(+), 28 deletions(-)   (+ 2 new test files)
```

### Declared beyond the minimum

Two of the three production changes go beyond the single line that is the root cause, and both
are declared rather than buried:

- **`MainActivity.onResume`** — without it, granting Android notification access mid-session
  still converges nothing. Found on hardware in §18, where the Debian run with the root-cause fix
  alone produced no bind and no announcement. It is 4 lines and it fires only on a *change*.
- **`snapshotShape`** — without it, choosing which apps to share after turning sharing on still
  mirrors nothing that is already on the shade. Found on hardware in §19. The shape deliberately
  excludes `whenSourceLocked` (a lock-policy change must not replay what the lock withheld —
  ADR-0015 §7) and `knownApps` (the app picker rewrites it whenever it lists applications, and
  resyncing because a list was drawn would be the announcement storm in snapshot form).

Both are on the convergence path the brief names, neither widens any authority, and each is
caught by name in §17.

### Considered and removed

A desktop "recovery seam" was written and then **deleted**: when a peer announced roles while
this device had announced nothing, re-offer `Work::AnnounceRoles`. It is a sound idea — a lost
announcement is otherwise unrecoverable — but with `announce_roles` fixed it is reachable only
through queue eviction, which cannot be driven deterministically from the test harness. Untested
code on a safety path is a liability, so it is a recorded debt (§24 debt 1) rather than a shipped
line.

---

## 8. Protocol impact

**None. No protobuf change, no wire change, no new message, no new field, and `SCHEMA_VERSION`
is untouched.**

Per §24 of the brief the existing role announcement was checked first, and it already carries
everything needed: `NotificationRoles { repeated NotificationRole roles; uint32 epoch; }`. The
defect was never that an update could not be expressed — the desktop's update *was* expressed,
sent and received. It was discarded by the receiver.

- `desktop/proto/tests/notifications_schema.rs` — unchanged, passing.
- `desktop/core/tests/notifications_protocol.rs` — 47 tests, unchanged, passing.
- Old and current peers understand every message this branch produces, because it produces no
  message the previous build did not.
- **No compatibility handling is required and none was added.** A peer running the pre-P3 build
  is not made worse by a peer running this one: it receives the same announcements it always did.

---

## 9. Enable-mid-session tests

**Android `NotificationConvergenceTest` — the headline cases.**

| # | Test | What it pins |
|---|---|---|
| — | `an ungranted peer's role announcement is still recorded` | **the defect of record**, as one assertion: `SINK` recorded at epoch 1 while `notifications.v1` is not granted — and no content sent, so recording widened nothing |
| B | `B enabling sharing mid-session converges without a reconnect` | the whole journey: 3 active notifications, sharing off → on → role epoch 1→2, bracketed snapshot, 3 upserts, no duplicates; then a 4th notification → exactly one more mirror |
| — | `a grant made with the listener already bound sends the snapshot` | the case `onListenerConnected` cannot rescue — another peer keeps the listener bound, so no role changes and no bind event; asserts the listener never cycled |
| — | `choosing apps after enabling sharing sends the snapshot` | gap (c): an empty bracket first (entitled to nothing, told so), then the app is chosen and the shade arrives |
| A | `A session establishment announces roles first` | the baseline, unmoved |

**Desktop `convergence.rs`.**

| # | Test | What it pins |
|---|---|---|
| A | `a_session_establishment_announces_roles_once` | epoch 1, `SINK + DISMISS_REPORTER`, and nothing re-announces for free |
| B | `b_an_announcement_with_nowhere_to_go_is_not_recorded` | the desktop's own version of the defect (§3) |
| C | `c_an_announcement_racing_an_attach_is_made_exactly_once` | the attach announcement and the peer's own announcement genuinely race; one epoch either way |
| E | `e_a_real_change_crosses_the_live_session` | narrow then widen on one connection, epochs 1→2→3, no reconnect |

---

## 10. Revocation tests

Read against the certified N0–N6 semantics first; **no new retention semantics were invented.**
The certified narrowing path is: the last eligible peer's grant goes → the listener unbinds →
`ListenerDisconnected` narrows the announced set on the live session → the desktop runs
`CloseAll(RoleNarrowed)`.

| # | Test | What it pins |
|---|---|---|
| C | `C revoking sharing mid-session narrows the role and stops content` | epoch 2→3 with an empty set, unbind requested, and **nothing at all** sent afterwards |
| — | `revocation stops content for one peer while another keeps sourcing` | the multi-peer shape: the listener stays bound (a role says what the machine *can* do), and what stops is the content, through the per-peer grant re-read per notification |
| F | `F a stale epoch is ignored` | a replay of the older `SINK` cannot restore authority, and with no `SINK` nothing is sent however ready the device is |

Desktop: `f_a_stale_epoch_cannot_re_widen_a_narrowed_peer`, `k_epoch_zero_is_refused`, plus the
pre-existing `CloseAll(RoleNarrowed)` coverage in `sink.rs` and `hardening.rs`, all green.

**On hardware (§18, §20): revocation propagated in 112 ms, mirrors 6 → 0, and a notification
posted afterwards did not reach the desktop — no canary in the daemon log, `showing 0 of 0`.
No restart and no reconnect.** Nothing about this branch weakens it; the fail-closed direction
was already correct and is asserted here so it stays that way.

---

## 11. Re-enable tests

| # | Test | What it pins |
|---|---|---|
| D | `D the full toggle cycle converges inside one session` | disabled → enabled → disabled → enabled, epochs 1→2→3→4, a **fresh** snapshot on the second enable rather than a resumption, and the peer's own epoch never restarting — which is what proves no reconnect |

On hardware, Debian 13, one TLS session: `roles=2 epoch=2` → `roles=0 epoch=3` → `roles=2 epoch=4`,
with `snapshot sent` on each widening and none on the narrowing.

---

## 12. Stale / duplicate epoch tests

| # | Test | Side |
|---|---|---|
| F | `F a stale epoch is ignored` | Android |
| G | `G a duplicate epoch cannot change the set` | Android — *equal* is refused too, so a duplicate carrying an extra role cannot widen by repetition |
| — | `an unset epoch is refused` | Android — epoch 0 is the wire's reserved value |
| F | `f_a_stale_epoch_cannot_re_widen_a_narrowed_peer` | desktop |
| G | `g_a_duplicate_epoch_changes_nothing` | desktop |
| K | `k_epoch_zero_is_refused` | desktop |
| D/E | `d_recomputing_an_unchanged_role_set_announces_nothing`, `E an unchanged role set produces no epoch bump and no traffic` | both — §9's anti-storm rule: 8 repeated events, 0 epochs burned, 0 extra messages |

---

## 13. Snapshot / resync tests

The certified model is preserved exactly: one bracketed snapshot, idempotent upsert, terminal
removals terminal, reconnect grace unchanged. **No second sync protocol was invented.**

| # | Test | What it pins |
|---|---|---|
| B | `B enabling sharing mid-session converges without a reconnect` | `sync:BEGIN`, 3 upserts, `sync:END`, each identity exactly once; then one new notification → exactly one upsert and **no second snapshot** |
| J | `choosing apps after enabling sharing sends the snapshot` | an entitlement change re-owes the snapshot |
| — | `a policy write that changes no entitlement sends no snapshot` | `knownApps` rewritten and the lock policy changed → **no** resync; the anti-storm half of the same rule |
| H | `H a reconnect converges with no manual toggle` | a second session gets one coherent snapshot, one mirror, no duplicate |

Hardware, Ubuntu 24.04, one session, no role announcement in between:

```text
15:41:18.947  snapshot sent: 12 of 49 active   →  desktop: snapshot complete named=12 closed=0
15:42:29.130  snapshot sent:  0 of 49 active   →  desktop: snapshot complete named=0  closed=3
15:42:38.177  snapshot sent: 12 of 49 active   →  desktop: snapshot complete named=12 closed=0
```

The middle line is "Clear all" in the app picker: the reconciliation closed the three live mirrors
because the snapshot named none of them. That is the certified bracket doing exactly its job,
driven by a policy change rather than by a reconnect.

---

## 14. Multi-peer isolation

| # | Test | What it pins |
|---|---|---|
| I | `I role and grant state is scoped to one peer` | A is allowed and B is not: A gets the snapshot, B gets its own role announcement and nothing else; a new notification goes to A alone; and B's computer announcing `SINK` still gets B nothing, because the grant is a separate question |
| — | `revocation stops content for one peer while another keeps sourcing` | narrowing one peer's entitlement leaves the other's untouched |
| I/M | `i_role_state_is_scoped_to_one_peer`, `m_every_peer_gets_its_own_first_announcement` | desktop: per-slot `LocalRoles`/`PeerRoles`, each connection announced once |

**On hardware, sequentially, exactly as §22 asks.** The tablet trusted all three desktops
throughout. It was mirroring to Debian 13 with `SOURCE + DISMISS_TARGET · epoch 2`, then
retargeted to Ubuntu 24.04, which the phone had **not** granted:

```text
15:22:33.173  TARGET_CHANGED peer=135A C045 BFE9 F0A5
15:22:38.337  SESSION_START
15:22:38.359  announcing roles=0 epoch=1      ← no role announced to the ungranted peer
15:22:38.367  peer roles epoch=1              ← its SINK recorded, and it bought nothing
desktop u2404: showing 0 of 0 mirrored; the device claims no source role (epoch 1)
```

Nothing was inherited. Retargeting back restored each peer's own policy.

> **Harness misstep, disclosed.** While navigating to Ubuntu 24.04's settings I tapped the wrong
> card — the device list reorders when the connected peer changes — and granted
> `notifications.v1` to Ubuntu 26.04 by mistake. It was noticed by reading the trust store rather
> than the screen, reverted immediately, and the navigation was rewritten to verify the detail
> screen's header before touching any control. No measurement in this report was taken through
> the wrong screen; every result below was re-taken with the header-verified navigation.

---

## 15. Desktop tests

```text
$ cargo test -p anyflow-capability-notifications --locked
   lib 55 · convergence 14 · dismiss 33 · hardening 40 · logging 11 · sink 60
   real_dbus 10 ignored · real_lock 2 ignored
   213 passed   0 failed   12 ignored

$ cargo test -p anyflow-capability-notifications -p anyflow-daemon \
             -p anyflow-core -p anyflow-runtime --locked -j 2
   TOTAL                     525 passed   0 failed   12 ignored
     core: protocol 12 · pairing 21 · identity_and_store 24 · identity_states 20 ·
           identity_seam 8 · notifications_protocol 47 · portable_boundary 9 · lib 25
     daemon: e2e 18 · files 36 · notifications 36 · clipboard 21 · wire 10 ·
             sessions 6 · listen 5 · control 4
     runtime: lib 10       notifications: 213

$ cargo fmt --all --check
   (clean)

$ cargo clippy -p anyflow-capability-notifications -p anyflow-daemon \
               -p anyflow-runtime --all-targets --locked
   0 warnings, 0 errors
```

This branch adds **14** desktop tests.

**The known `real_dbus` tautological-assertion clippy defect was not touched.** It does not fire
on this host's toolchain and fired on all three guest toolchains in U2 (§34C.1, §39.9.1, §40.11).
Either way: it is in a test file this branch does not modify, and the P3 production diff
introduces **no new clippy finding** — the three changed packages are clean with `--all-targets`.

---

## 16. Android tests

```text
$ ./gradlew --offline :app:testDebugUnitTest
TOTAL   tests=581   passed=581   failed=0   skipped=0
  NotificationConvergenceTest  19   (new)
  NotificationSourceTest       60   NotificationRolesTest        19
  NotificationHardeningTest    13   NotificationsProtocolTest    20
  MultiPeerRoutingTest         13   PeerTargetTest               18   UiMappingTest 31   (P1)
  BatteryReadingTest            5                                     (P2)
```

Baseline was 562 (the P2 total); this branch adds **19**.

No existing test asserted the behaviour that was removed — nothing anywhere claimed that an
ungranted peer's role announcement should be refused. The nearest neighbour,
`NotificationSourceTest.an ungranted peer is sent no notification content`, already stated the
principle in the *outbound* direction and its comment reads *"It is still told what this device
can do — a role is not a grant."* The inbound direction contradicted it; now it does not.

---

## 17. Mutation evidence

Every mutation was applied to the **final** code, run, and reverted. No mutation code is
committed (`grep -rn MUTATION` over `android/app/src` and `desktop/capabilities` returns nothing).

```text
MUTATION 1 — the pre-P3 behaviour: the grant check back in front of ROLES
  581 tests, 7 failed:
      an ungranted peer's role announcement is still recorded
      B enabling sharing mid-session converges without a reconnect
      D the full toggle cycle converges inside one session
      I role and grant state is scoped to one peer
      K dismiss sync is correct after a mid-session convergence
      L the lock policy still applies to a mid-session snapshot
      a grant made with the listener already bound sends the snapshot

MUTATION 2 — the dangerous opposite: accept an older epoch after a newer one
  581 tests, 5 failed:
      F a stale epoch is ignored
      G a duplicate epoch cannot change the set
      NotificationRolesTest: a replayed announcement cannot re-widen a narrowed set
      NotificationRolesTest: an equal epoch is refused
      NotificationRolesTest: a large epoch is compared unsigned

MUTATION 5 — a snapshot is owed once per connection, never again
  581 tests, 1 failed:
      choosing apps after enabling sharing sends the snapshot

MUTATION 3 (desktop) — announce_roles advances the state before taking the channel
  14 tests, 1 failed:
      b_an_announcement_with_nowhere_to_go_is_not_recorded   (left: 1, right: 0)
```

One earlier mutation, run against an intermediate build and recorded for completeness:
**1b** (PolicyChanged announces roles but does not converge) failed `a grant made with the
listener already bound sends the snapshot` alone.

**A mutation that found a bad test rather than good code.** Mutation 3 initially *passed*. The
desktop test was reading `peer_reports()` without waiting for the worker, so it raced and passed
for the wrong reason. It now waits for the peer's queue to drain — an explicit condition, not a
sleep — and the mutation is caught. Recorded because a mutation check that reports a test it did
not really run is worse than no mutation check.

**Not covered by a mutation:** `MainActivity.onResume`. It is Android-framework lifecycle code
and this project's suite is plain JVM with no Robolectric; adding that dependency is outside P3.
Its evidence is the hardware A/B in §18, where the run with the root-cause fix alone produced no
listener bind and no announcement, and the run with it produced both. Recorded as debt 3.

---

## 18. Debian 13 physical regression → **PASS**

`anyflow-d13`, `192.168.68.59`, fingerprint `B52C DA20 46ED 006D`. GNOME 48.7, the preserved U2
guest. **This is the A/B**, on one machine, in one sitting, with the desktop daemon binary held
constant and only the APK swapped.

Starting state, verified from the tablet's own trust store before anything was touched: all three
desktops paired, each granted `battery.v1, files.v1` — **`notifications.v1` granted to nobody**,
which is the state the app deliberately leaves after pairing and the exact precondition the
defect needs.

### Before — the pre-P3 APK, the defect reproduced live

```text
        SESSION_START (tablet → 192.168.68.59)
        daemon: announcing roles peer=573C CB84 DA6C 993B roles=2 epoch=1
        daemon: peer roles peer=573C CB84 DA6C 993B roles=0 epoch=1

tablet, notification settings for anyflow-d13:
        Android notification access   Not allowed
        Notification listener         Not bound
        notifications.v1 grant        Not granted
        This device announces         no role · epoch 1
        The computer announces        no role · epoch 0      ← the desktop said 2 @ epoch 1
```

Sharing then turned on **with the session alive** — grant on, notification access allowed,
fixture app chosen — and the session sampled for 100 seconds:

```text
t+10s   showing 0 of 0 mirrored   roles: announced 2 (epoch 1); device can source (epoch 2)
t+25s   showing 0 of 0 mirrored
t+45s   showing 0 of 0 mirrored
t+70s   showing 0 of 0 mirrored
t+100s  showing 0 of 0 mirrored

tablet: Computer is not receiving — "This device is connected, but the computer has not said it…"
        Android notification access   Allowed
        Notification listener         Bound
        notifications.v1 grant        Granted
        This device announces         SOURCE + DISMISS_TARGET · epoch 2
        The computer announces        no role · epoch 0      ← still nothing, 100 s later
```

**Daemon restarted, nothing else changed: `showing 4 of 4 mirrored` within ~15 s.** U2 §39.19 and
§40.13, reproduced exactly.

### After — the same guest, the same daemon process, the fixed APK

Daemon pid `2307`, started `17:52:02`, **never restarted between the two runs**.

```text
tablet, identical preconditions (grant withdrawn, listener disallowed, APK reinstalled):
        notifications.v1 grant        Not granted
        The computer announces        SINK + DISMISS_REPORTER · epoch 1   ← recorded
```

The full §20 sequence, from the tablet's own log. **One `SESSION_START` and zero `SESSION_END`:**

```text
14:57:31.559  SESSION_START                                    ← the only session
14:57:31.574  announcing roles=0 epoch=1
14:57:31.579  peer roles epoch=1                               ← the fix, on the wire

14:58:20.175  (person turns sharing on)
14:58:22.344  (Android notification access allowed)
14:58:24.628  requesting listener bind: an eligible peer is connected
14:58:24.730  notification listener connected
14:58:25.183  announcing roles=2 epoch=2          ← toggle → role update:     ~5.0 s
14:58:25.252  snapshot sent: 4 of 40 active       ← toggle → first mirror:    ~5.1 s
14:58:25.313  (4 x NOTIFICATION_OUTCOME_DISPLAYED) ← snapshot complete:       ~5.1 s
              desktop: showing 4 of 4 mirrored; the device can source notifications (epoch 2)

14:58:48      one new notification posted          → showing 4 → 5, exactly one mirror

15:00:06.853  (sharing revoked)
15:00:06.965  requesting listener unbind: no eligible peer      ← +112 ms
15:00:06.970  announcing roles=0 epoch=3
              desktop: showing 6 → 0; the device claims no source role (epoch 3)
15:00:13      a notification posted after the revocation → showing 0 of 0, no canary anywhere

15:00:39.896  (sharing re-enabled, same session)
15:00:40.579  announcing roles=2 epoch=4          ← re-enable → role update:  ~0.68 s
15:00:40.683  snapshot sent: 7 of 43 active       ← re-enable → resync:       ~0.79 s
              desktop: showing 7 of 7 mirrored
```

Measured from the toggle, the ~5 s on the first enable is three human actions (grant, then the OS
permission, then returning to the app); from the last of them the convergence took **0.5 s**. The
re-enable, which is a single action, took **0.68 s**.

- Session established and held throughout; epochs 1→2→3→4, strictly monotonic. OK
- No `SESSION_END`, no second `CONNECT_SUCCESS`, no `TARGET_CHANGED`, no daemon restart,
  no app restart, no re-pairing. OK
- No duplicate mirrors: each snapshot identity answered `DISPLAYED` exactly once. OK
- Removal propagation still correct: `remove` on the phone → desktop 10 → 9. OK

---

## 19. Ubuntu 24.04 physical regression → **PASS**

`anyflow-u2404`, `192.168.68.75`, fingerprint `135A C045 BFE9 F0A5`. GNOME **46.0** — the oldest
of the three, and the distribution where the defect was first seen (§34C.5b).

**This guest ran the fixed daemon**, built inside the guest from the P3 desktop patch transferred
by hash-verified base64 (`f0a050bd…`, 1715 bytes, sha256 matched on both sides) and compiled with
the distribution's own `rustc` in 19.8 s. So Ubuntu 24.04 is where the desktop half of this branch
was exercised on real hardware.

```text
before:   notifications.v1 grant   Not granted
          The computer announces   SINK + DISMISS_REPORTER · epoch 1

15:40:25.116  SESSION_START
15:40:25.136  peer roles epoch=1
15:41:17.988  (sharing turned on — one action; access was already granted)
15:41:18.101  requesting listener bind                  ← +113 ms
15:41:18.282  notification listener connected           ← +294 ms
15:41:18.762  announcing roles=2 epoch=2                ← +774 ms
15:41:18.947  snapshot sent: 12 of 49 active            ← +959 ms
              desktop: snapshot complete named=12 closed=0
              12 x NOTIFICATION_OUTCOME_DISPLAYED, no duplicates
```

**Gap (c) was found here, on hardware, and then fixed.** Before the `snapshotShape` change, this
guest produced `snapshot sent: 0 of 49 active` (correct — no app chosen yet) and then, when the
fixture app was ticked, **nothing at all**: `showing 0 of 0 mirrored`, no second snapshot, and the
desktop's own log confirming `snapshot complete named=0 closed=0` and nothing after it. With the
fix, the same actions produce the three-line sequence in §13.

The steady-state mirror count settles below the number sent (`showing 3 of 3` against 12
displayed) because gnome-shell 46.0 expires banners faster than Debian's 48.7 — every one of the
12 was answered `DISPLAYED`, and `showing N of N` means mirrors and tracked agree. That is the
server's retention, not this branch's behaviour.

---

## 20. Ubuntu 26.04 physical regression → **PASS**

`anyflow-u2604`, `192.168.68.78`, fingerprint `1315 96BD 9834 BA6F`. GNOME **50.1**, the newest.

**This guest ran a completely unmodified desktop** — its checkout carries no P3 change at all —
which makes it the cleanest statement this report can make: *the convergence fix is entirely on
the Android side, and the desktop needs no change to converge.*

```text
15:46:53.519  TARGET_CHANGED peer=1315 96BD 9834 BA6F
15:46:58.687  SESSION_START
15:46:58.704  announcing roles=0 epoch=1
15:46:58.707  peer roles epoch=1                        ← recorded while ungranted

before:   notifications.v1 grant   Not granted
          The computer announces   SINK + DISMISS_REPORTER · epoch 1

15:47:51.849  (sharing turned on)
15:47:51.956  requesting listener bind                  ← +107 ms
15:47:52.051  notification listener connected           ← +202 ms
15:47:52.580  announcing roles=2 epoch=2                ← +731 ms
15:47:52.619  snapshot sent: 0 of 49 active             ← +770 ms, correctly empty
15:48:06.235  (fixture app chosen)
15:48:06.521  snapshot sent: 12 of 49 active            ← +286 ms, the shape resync
              desktop: showing 10 of 10 mirrored; the device can source notifications (epoch 2)

15:48:32.812  one new notification → mirrored exactly once (13 DISPLAYED in total)

15:49:04.697  (sharing revoked)
15:49:04.838  announcing roles=0 epoch=3                ← +141 ms
              desktop: closed every mirror for a peer closed=10 reason="peer is no longer a source"
                       showing 0 of 0 mirrored
```

§21 proof for this target: exactly one `TARGET_CHANGED`, one `CONNECT_SUCCESS`, one
`SESSION_START`, and **no `SESSION_END`** across the whole enable → resync → new → revoke cycle.

All three guests were run **sequentially**, one at a time, ballooned at runtime with
`virsh setmem` (never `--config`), and each was shut down cleanly before the next was started.

---

## 21. Dismiss smoke

Both roles converge correctly and are confirmed on hardware. After mid-session convergence on
Debian 13, with dismiss sync enabled on both ends:

```text
desktop:  dismissal: this desktop reports human dismissals; the device will act on a dismiss request
tablet:   This device announces  SOURCE + DISMISS_TARGET · epoch 4
          The computer announces SINK + DISMISS_REPORTER · epoch 1
```

Deterministic coverage, all green:

| Test | What it pins |
|---|---|
| `K dismiss sync is correct after a mid-session convergence` | a clearable mirror arriving through the convergence path is cancelled on this phone when the computer asks, `cancelNotification(key)` called with the right key, answered `REMOVED` |
| `an ongoing notification is not dismissed after convergence` | an ongoing notification is **not** dismissible, three requests are each answered once, the original stays, and there is no retry loop |
| desktop `dismiss.rs` | 33 tests, unchanged, passing |

**Honest limitation: the human dismissal on the desktop was not completed on hardware in this
wave.** The GNOME 48.7 banner was located through AT-SPI (`FOUND label 'P3DISMISS3'`, after
enabling `toolkit-accessibility`, which was off) and a nearby button was activated, but no
`NotificationClosed(reason=2)` followed — the activated widget was not the close control, and the
banner had collapsed on the retry. Faking it was not an option: synthesising a `CloseNotification`
call would produce reason 3, which the code correctly ignores, so it would have proved nothing.

What this costs is bounded and stated plainly: **this branch changes no line on the dismiss
path** (`git diff` touches `handleInbound`'s routing, `converge`, `onResume` and `announce_roles`
and nothing else), the role prerequisites for dismissal are verified converged on hardware above,
the behaviour itself is pinned by the two tests named, and U2 §39.20 already certified the full
human-dismissal cycle on this exact guest. Recorded as debt 4.

The drivable half of the same path *was* exercised: a removal on the phone propagated to the
desktop over the converged session, 10 → 9 mirrors.

---

## 22. Lock-policy smoke

Converging roles must not become a way past the lock reduction. On Debian 13, with the tablet
locked and its policy at `whenSourceLocked = APP_ONLY`, the notification was mirrored (9 → 10, so
not suppressed) and the desktop's **rendered** notification was read back through AT-SPI:

```text
LABEL: AnyFlow Fixture
canaries (P3LOCKTITLECANARY / P3LOCKBODYCANARY) in the rendered labels: 0
```

The app label and nothing else — the reduction was applied at the source, and the title and body
never crossed the wire. A second sweep after the lock state changed found **0** canaries again:
nothing withheld was replayed.

Pinned deterministically by `L the lock policy still applies to a mid-session snapshot` — a
snapshot delivered by the convergence path is `redacted`, with empty title and body, and a later
policy event replays nothing.

**FULL / APP_ONLY / SUPPRESS were not redesigned and not touched.** The `SnapshotShape` used to
decide a resync deliberately excludes `whenSourceLocked` for exactly this reason (§7).

---

## 23. Privacy / logging

- **No new persisted state of any kind.** No notification history, no cache, no new file, no
  schema change. The one new piece of state, `Session.snapshotShape`, is per-connection, in
  memory, and dropped when the session ends. It is never transmitted.
- **Net new log statements: zero.** The Android diff adds no `Log.*`; `applyPeerRoles` carries the
  two lines that were already there. The desktop diff adds none. No new `println!`, `eprintln!`,
  `Toast`, or `tracing` call appears anywhere in the diff.
- **No notification content is logged anywhere by this branch.** Titles and bodies continue to be
  absent from every log statement on both sides; the phone logs counts, enum names and an opaque
  id prefix, and the desktop logs counts and epochs.
- Pinned by `role announcements carry no notification content` — the convergence traffic this
  branch makes more frequent is asserted to contain no title, no body, no package name and no
  platform key.
- **Hardware sweep with fresh canaries** (`P3-FIXTURE-*`, `P3-BODY-CANARY-*`, `P3DISMISS*`,
  `P3LOCKTITLECANARY`, `P3LOCKBODYCANARY`, `MUSTNOTAPPEAR`):

```text
canaries in ~/p3-before.log                        0
canaries in ~/p3-after-restart.log                 0
canaries in ~/.local/share/anyflow/state.json      0
canaries in ~/.local/share/anyflow/identity.key    0
any notification-content file under ~/.local/share/anyflow   (none)
```

- No telemetry, no cloud, no network behaviour change, no grant widening, and **no authority
  derived from a remote claim** — the trust store and the product's own consent remain the only
  authorities, and §9's `an ungranted peer's role announcement is still recorded` asserts that
  recording a role grants nothing.

---

## 24. Remaining debts

None blocking.

1. **A lost desktop role announcement is still unrecoverable within a session.** `announce_roles`
   can no longer *silently* lose one, but `Work::AnnounceRoles` is non-terminal, so a queue full
   enough to evict it would leave a peer never told. A recovery seam was written and removed
   because it cannot be tested deterministically (§7). `PropertiesChanged`-style re-announcement,
   or making the item non-evictable, is the obvious future seam.
2. **`Unavailable` and a genuinely empty role set are indistinguishable to a peer**, by design
   (ADR-0017 §2). Noted because it was read closely during this work; unchanged.
3. **`MainActivity.onResume` has no unit test.** It is Android lifecycle code and this suite is
   plain JVM; its evidence is the hardware A/B in §18. A Robolectric dependency would cover it and
   is outside P3.
4. **The human dismissal on the desktop was not completed on hardware in this wave** (§21). It
   needs a working AT-SPI path to the GNOME banner's close control, or a person to click it.
5. **Residual tablet policy fields.** The three peers' `grantedCapabilities` were restored exactly
   (`battery.v1, files.v1` each) and AnyFlow's OS notification listener was disallowed again, but
   each peer's `allowedApps` now lists the fixture app and Debian 13's `allowDismissSync` is left
   on. Both are inert while `notifications.v1` is ungranted. Recorded rather than churned.
6. **Observation, not this branch's doing:** `anyflow-u2604`'s checkout still carries the P2
   battery patch as uncommitted changes; P2's report said each guest was returned to a clean
   `9792330`. Its `capabilities/notifications` tree is untouched, so §20's claim holds. Left as
   found.

---

## 25. Git

```text
$ git status --short
 M android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationSource.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M desktop/capabilities/notifications/src/lib.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? U2-HARDENING-P3-NOTIFICATION-ROLE-CONVERGENCE.md
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationConvergenceTest.kt
?? desktop/capabilities/notifications/tests/convergence.rs

$ git diff --check
(clean)

$ git diff --stat
 .../anyflow/notifications/NotificationSource.kt    | 191 ++++++++++++++++++---
 .../github/yurisismotto/anyflow/ui/MainActivity.kt |  26 ++-
 desktop/capabilities/notifications/src/lib.rs      |  18 +-
 3 files changed, 207 insertions(+), 28 deletions(-)

$ git diff --name-status
M	android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationSource.kt
M	android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
M	desktop/capabilities/notifications/src/lib.rs
```

Nothing was staged, committed, pushed, or opened as a PR.

### Scope check (§27)

Verified **not** fixed, changed, or touched by this branch: the clipboard sensitive test harness;
the notifications `real_dbus` tautological clippy assertion; CI clippy; Android CI; Debian
packaging docs; incoming-file GUI approval; QR scanner camera orientation; trust-store ordering
cosmetics; branding / logo / palette; Quick Panel; KDE. No protobuf, no `SCHEMA_VERSION`, no
`renegotiate` change, no new dependency.

One CI file was touched after the evidence above was captured:
`.github/workflows/portable-windows-msvc.yml`, whose notification test-file classification guard
lists the expected contents of `capabilities/notifications/tests` and would otherwise fail on the
new file. `convergence.rs` was added to that list as **portable** — it has no `cfg` gate, no
D-Bus, no session and no GUI, and runs against the same in-memory harness as `sink.rs` and
`dismiss.rs`. The portable test command already covers it via `-p anyflow-capability-notifications`
and was not changed; no other CI semantics were altered.

**P1 remains green:** `MultiPeerRoutingTest` 13, `PeerTargetTest` 18, `UiMappingTest` 31 — and P1's
routing was exercised on hardware throughout, with four retargets across three desktops all
honoured. **P2 remains green:** `BatteryReadingTest` 5, and the battery crate's tests are
untouched.

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untracked, unmodified, unstaged, and its historical results
are quoted here only as the baseline they were — never rewritten as if they had used the fix.

### Machine state left behind

Three VMs preserved and shut down, each returned to a clean `9792330` checkout (Ubuntu 24.04's P3
patch reverted and its daemon rebuilt from the clean tree). GDM autologin, enabled on Debian 13
and Ubuntu 26.04 so the daemon could reach a real notification server, was reverted from the
backups taken first; Debian 13's `toolkit-accessibility` was returned to `false`. The tablet's
three pairings are intact with their original grants, AnyFlow's notification listener is
disallowed again, and `svc power stayon` is back to its default.

---

## 26. Verdict

**The defect is fixed at its cause, and the cause was one line on the phone.** A role announcement
is a statement about capability, not a claim on authority, and refusing one from an ungranted peer
discarded the only thing the desktop ever said about itself. Because the desktop announces once
per session and the app deliberately withholds the notifications grant at pairing, the ordinary
user journey — pair, connect, turn sharing on — put that single announcement inside the window
where it was thrown away, silently, with no reply and no log line. A restart worked only because
it manufactured a second announcement at a luckier moment.

Two further gaps on the same path were found on hardware and closed: the OS notification-access
grant reaching only the screen and not the capability, and a snapshot being owed once per
connection rather than once per entitlement.

**Reproduced and then fixed on the same machine, in the same sitting, with the same daemon
process.** On Debian 13 the pre-P3 APK held `showing 0 of 0 mirrored` for 100 seconds against a
desktop that had announced `SINK + DISMISS_REPORTER` at epoch 1, and a daemon restart cleared it
in under 15 seconds. The fixed APK, against daemon pid 2307 which was never restarted, converged
inside the live session in 0.5 s. Ubuntu 24.04 confirmed it at 0.96 s against a daemon carrying
this branch's desktop change, and Ubuntu 26.04 at 0.77 s against a desktop carrying no P3 change
at all — three distributions, three GNOME versions, 46.0, 48.7 and 50.1.

**Revocation was not weakened to make convergence green.** It propagated in 112 ms, took every
mirror off the screen, and blocked new content — verified by posting a canary afterwards and
finding it nowhere. A stale epoch cannot restore what a revocation gave up, asserted by name and
proved by mutation.

```text
U2 P3 NOTIFICATION ROLE CONVERGENCE REMEDIATION: PASS
MID-SESSION ROLE CHANGES CONVERGE WITHOUT RESTART
REVOCATION FAIL-CLOSED REGRESSION PASS
```
