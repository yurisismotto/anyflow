# U2 HARDENING — P1: Android Multi-Peer Selection / Reachable Peer Routing

**Branch:** `fix/u2-p1-android-multipeer-selection`
**Baseline:** `9792330` (the commit `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` certifies)
**Date:** 2026-09-16
**Scope:** the single highest-priority U2 product defect, P1. Nothing else.

> `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is **not** modified by this branch. It remains
> evidence for baseline `9792330`, describing the code as it was certified — with the
> defect present. This report is the separate remediation record.

---

## 1. Defect of record

U2 §39.17, classified **PRODUCT — HIGH**, found on Debian but not Debian-specific.

The tablet trusted two desktops:

```text
[0] anyflow-u2404   trusted, POWERED OFF for the whole session
[1] anyflow-d13     trusted, running, reachable, ping 3/3, LISTEN *:55432
```

Both cards showed "Connecting…" forever. The Debian daemon logged **zero** connection
attempts across an explicit Connect, a full app restart, and eighty seconds of polling.
Removing only the *tablet-side trust entry* for `anyflow-u2404` — the Ubuntu VM and every
piece of Ubuntu evidence untouched — and Debian connected in **7 seconds**.

Three call sites, of which the brief named two:

| # | Site | Code | Named in brief |
|---|------|------|----------------|
| 1 | `service/ConnectionService.kt:238` | `pairedPeer()` → `app.trustStore.peers().firstOrNull()` | yes |
| 2 | `ui/SendActivity.kt:81` | `runCatching { app.trustStore.peers().firstOrNull() }` | yes |
| 3 | `ui/DevicesScreen.kt:127` | `state.peers.firstOrNull { state.isConnected(it) } ?: state.peers.first()` | **no — found by the §0 audit** |

Site 3 is the quick-actions target. U2 §39.17 mentions it only as "the only place that
prefers a connected peer… and it still falls back to `first()`". It is a third instance of
the same defect and is fixed here.

---

## 2. Root cause

**Trust and destination were the same question, answered by list position.**

`TrustStore.peers()` returns a `List`, and three components read element 0 as "the computer
to talk to". Nothing in the system ever recorded which computer a person wanted. There was
no in-app way to express one: `MainActions.onConnect` had type `() -> Unit`, so the identity
of the row a person tapped stopped at the UI and the service re-derived a destination of its
own.

Two aggravating properties found during this work:

- **The order is volatile.** `TrustStore.addPeer` appends (`remaining + peer`), and
  `rememberAddresses` — called on every *successful* connection — goes through `addPeer`.
  So connecting to a peer moves it to the end of the list. Observed live in §13 below: the
  two device cards swapped places between Scenario 3a and 3b. Under the old code the routing
  target silently changed whenever a session came up.
- **The failure was invisible.** Link state is global — there is one session — and every
  peer card rendered it, so the peer that was never being attempted looked exactly like the
  one that was.

---

## 3. Architecture before

```text
 DevicesScreen ── onConnect: () -> Unit ──► ConnectionService.start(context)
   (identity discarded at this boundary)         │
                                                 ▼
                                      pairedPeer() = peers().firstOrNull()
                                                 │
                                                 ▼
                                      candidateAddresses(peer, round) ──► dial
 SendActivity ── peers().firstOrNull() ──► files.offer(thatPeer, uri)
```

---

## 4. Architecture after

```text
                 ┌── TrustStore.peers()            (the SET of trusted peers)
 PeerTarget.resolve┤
                 └── TrustStore.selectedPeerHex    (the person's CHOICE, persisted)
                                │
                                ▼
                        Resolution
                  ├── Selected(peer)          explicit
                  ├── OnlyTrustedPeer(peer)   one candidate, nothing to be ambiguous about
                  ├── MustChoose(candidates)  several + no choice → the UI asks
                  └── NoTrustedPeer           nothing paired
                                │
 DevicesScreen ── onConnect(Fingerprint) ──► ConnectionService.start(ctx, fingerprint)
                                                 │  EXTRA_TARGET_FINGERPRINT
                                                 ▼
                                      applyRequestedTarget(intent)
                                        1. selectPeer(fp)         persist the choice
                                        2. end a session that belongs to another peer
                                        3. coordinator.onTargetChanged(prefix)
                                                 │
                                                 ▼
                                   endpointsFor / dial / blocked
                                   all re-read the resolution EVERY round
```

Nothing downstream of `PeerTarget.resolve` can see list position.

---

## 5. Files changed

| File | Δ | What |
|---|---|---|
| `store/PeerTarget.kt` | **new**, 139 | The resolution policy. Pure, no Android, no ordering. |
| `store/TrustStore.kt` | +68 | Persisted `selectedPeer`; `selectPeer`/`clearSelectedPeer`; `removePeer` drops the choice with the peer. |
| `AnyFlowApp.kt` | +31 | `targetResolution()` / `targetPeer()` / `selectPeer()`, failing closed. |
| `service/ConnectionService.kt` | +128 | `firstOrNull` → target resolution; `EXTRA_TARGET_FINGERPRINT`; retarget tears down a foreign session and wakes the loop. |
| `net/ConnectionCoordinator.kt` | +50 | `onTargetChanged()`; `blocked` precondition. |
| `net/ConnectionEvent.kt` | +11 | `TARGET_CHANGED` kind. |
| `ui/MainState.kt` | +45 | `selectedPeerHex`; derived `target`/`targetPeer`/`mustChooseTarget`/`isTarget`; `onConnect` becomes `(Fingerprint) -> Unit`. |
| `ui/MainActivity.kt` | +27 | Observes the selection; passes the row's fingerprint; pairing targets the new peer. |
| `ui/DevicesScreen.kt` | +107 | Per-row Connect/Disconnect; quick actions use the target, not `first()`; ambiguity stated. |
| `ui/SendActivity.kt` | +190 | Destination resolved over *eligible* peers; radio picker; no `firstOrNull`. |
| `ui/UiMapping.kt` | +16 | `statusFor(..., targeted)` — an untargeted peer no longer borrows the link state. |
| `test/UiMappingTest.kt` | +55 | 3 tests for the above. |
| `test/PeerTargetTest.kt` | **new**, 18 tests | Policy. |
| `test/MultiPeerRoutingTest.kt` | **new**, 13 tests | The running loop. |

`git diff --stat`: **11 modified files, 690 insertions, 38 deletions**, plus 3 new files.

---

## 6. Connection target semantics

1. **Explicit choice wins.** A trusted peer named by fingerprint becomes the target, whatever
   its position. Persisted, so it survives the service stopping and the process being killed.
2. **An offline target does not block a later reachable peer.** Choosing a different peer
   cuts the pending backoff short (`onTargetChanged`) instead of waiting out a ladder that
   can reach five minutes.
3. **Order is never consulted.** `PeerTarget.resolve` is a function of the peer *set* and the
   chosen fingerprint.
4. **No auto-connect by storage position.** With several trusted peers and no choice, the
   resolution is `MustChoose`; the loop stops with `"choose which computer to connect to"`
   and the UI says so.
5. **Identity is the fingerprint** — the value the TLS pin is checked against — never a name,
   an address, or an index.
6. **Discovery is still discovery.** mDNS results are filtered by device id and authenticated
   by the pin; being discovered grants nothing.
7. **A stale choice self-heals.** A fingerprint no longer in the trust store is ignored, and
   `removePeer` clears it outright.

**Fallback with no explicit choice:** exactly one trusted peer → use it; zero → say nothing
is paired; several → ask. This is the rule the app already used for the clipboard tile
(`sendClipboardFromShortcut`) and that the desktop CLI uses for an ambiguous device prefix.

**Persistence:** required, and deliberately minimal. `ConnectionService` is started by
`startForegroundService` and can be stopped and recreated by the platform; an in-memory
choice would be lost and the app would fall back to guessing. One additive optional JSON key
(`selectedPeer`), so `SCHEMA_VERSION` stays at **1** — a file written before this key reads
as "nobody has chosen", and an older build ignores a key it does not know. Nothing to migrate
in either direction. The value is a public fingerprint hex; no content is stored.

---

## 7. SendActivity semantics

Destination is resolved over the peers **eligible for this kind of share** — `files.v1` for a
file, `clipboard.v1` + `allowSend` for text — not over every trusted peer.

| Situation | Behaviour |
|---|---|
| Nothing paired | "No computer paired yet. Open AnyFlow and scan the pairing code first." |
| Paired, none eligible | Names the missing grant. **Not** "nothing is paired" — that would send someone to the QR scanner for a grant they already own. |
| Exactly one eligible | Used directly. |
| Several, one is the current target | Preselected **and** the full picker stays visible, so the convenience never becomes silent routing under a nicer name. |
| Several, none chosen | Picker only. No Send button until a destination is named. |

Choosing a destination goes through `ConnectionService.start(ctx, fp)` — the single writer —
because a transfer travels over the authenticated session, so a share aimed at a computer the
app is not connected to has nowhere to go. Doing it when the row is tapped rather than when
Send is pressed also lets the session come up while the person is still reading the screen.

Each row states the **fingerprint** as well as the name, so two desktops called the same thing
are distinguishable. Choosing grants nothing and pairs nothing.

---

## 8. Security invariants

Unchanged, and verified unchanged: TLS 1.3, SPKI pinning, proof-of-possession, QR pairing,
trust-store fingerprint validation, the peer identity model, notification/clipboard/file grants.

- Selecting is **not** a route to trust. `TrustStore.selectPeer` returns `false` and writes
  nothing for an untrusted fingerprint; `applyRequestedTarget` treats that as "no target".
- A chosen peer still authenticates as the pinned identity — `PeerConnection.connect(pinned =
  peer.fingerprint)` is untouched.
- An address that answers as a different computer is a **security** failure, not a session
  (test R12; the ladder change and the "keep trying other endpoints" rule are preserved).
- Grants are still re-read from the trust store on every question. Physically confirmed in
  §16: with two trusted peers and `clipboard.v1` granted to neither, the text share refuses.

---

## 9. Tests added

**`PeerTargetTest` — 18 tests.** T1 the certified store shape; T2 order reversed; T3 the
mirror (selecting the *first* entry must also work, so a "prefer the last" fix fails here);
T4/T5 three peers in three orderings, every peer reachable as a target; T6–T8 the no-choice
rule; T9 an untrusted fingerprint is ignored, not honoured; T10 a forgotten peer's stale
choice; T11 two peers sharing a display name; T12 uppercase/truncated hex selects nothing;
T13–T17 share destinations incl. zero eligible and a chosen-but-ineligible peer; T18 explicit
vs implicit are distinguishable.

**`MultiPeerRoutingTest` — 13 tests**, driving a real `ConnectionCoordinator` wired exactly as
`ConnectionService` wires it, on a virtual clock. R1 the certified reproduction as a running
loop; R2 order reversed; R3 an explicit choice is obeyed even when it fails (so the fix is not
"prefer whatever is reachable"); R4 both online, both orders, both choices; R5 three peers
where a "first reachable" implementation would pass everything earlier; R6 ambiguity dials
nothing and says why; R7 retargeting mid-backoff; R8 a peer coming online does not hijack;
R9 reconnect returns to the same peer; R10/R11 disconnect-then-choose, and restart determinism;
R12 identity mismatch; R13 the retarget log carries a fingerprint prefix and no device name.

**`UiMappingTest` — 3 added.** An untargeted peer reads `Available` under Connecting/Retrying/
Error; the targeted peer still shows the link state; a live session shows Connected regardless.

Every test asserts behaviour — a resolution, a dialled endpoint, a rendered status — not
private structure.

---

## 10. JVM / unit results

```text
./gradlew --offline :app:testDebugUnitTest --rerun-tasks

TOTAL   tests=557   passed=557   failed=0   skipped=0
  MultiPeerRoutingTest   13    PeerTargetTest    18
  UiMappingTest          31    ReconnectTest     18
  TrustStoreTest          4
```

Baseline was 523; this branch adds **34**. `compileDebugKotlin` emits **no warnings**.

**Android lint** (`:app:lintDebug`): 1 error, 47 warnings — **0 of them in any file this branch
touches**. The error is pre-existing in `ui/ClipboardTileService.kt:82`
(`StartActivityAndCollapseDeprecated`), untouched here and out of scope per §22.

**Instrumented tests were deliberately NOT run.** `connectedAndroidTest` uninstalls and
reinstalls the app, which destroys the tablet's trust store — the exact two-peer state this
regression depends on. `adb install -r` was used instead and preserves app data. This is a
known property of the project's harness, not a gap introduced here.

---

## 11. Physical regression topology

| Role | Guest | Identity | Address |
|---|---|---|---|
| Peer A | `anyflow-u2404` (Ubuntu 24.04) | `135A C045 BFE9 F0A5` · `db9dfc03…` | 192.168.68.75:55432 |
| Peer B | `anyflow-d13` (Debian 13) | `B52C DA20 46ED 006D` · `6185e1e6…` | 192.168.68.59:55432 |
| Peer C | `anyflow-u2604` (Ubuntu 26.04) | `1315 96BD 9834 BA6F` · `d81cd9a3…` | 192.168.68.78:55432 |
| Phone | SM-X620 (`RX2Y500C7SY`) | `573C CB84 DA6C 993B` · `6532889e…` | 192.168.68.63 |

All three identities are the **preserved U2 ones** — the fingerprints match the U2 report
byte for byte, so this is a regression against the certified machines, not fresh installs.

The tablet's trust store was empty at the start of this run, so **all three** peers were paired
from scratch by QR — and all three ended the run trusted simultaneously, which the defect made
impossible. Host RAM is 15 GiB; the two-peer scenario used one graphical guest plus one
headless guest, ballooned at runtime (`virsh setmem`, never `--config`), and the persisted
domain XML still reads `4194304 KiB` for both — the preserved VMs are left exactly as found.

---

## 12. Scenario 1 — Ubuntu 24.04 OFF, Debian 13 ON → **PASS**

Trust store at the moment of test, the certified shape:

```text
[0] anyflow-u2404   135ac045bfe9f0a5…   trusted, POWERED OFF
[1] anyflow-d13     b52cda2046ed006d…   trusted, ONLINE
```

Ubuntu was selected *and* first in storage. Tapping Connect on the Debian row:

```text
11:01:16.977  TARGET_CHANGED peer=B52C DA20 46ED 006D
11:01:16.977  RETRY_CANCELLED reason=woken early
11:01:21.997  DISCOVERY round=4 endpoints=2
11:01:21.997  CONNECT_ATTEMPT round=4 endpoint=192.168.68.59:55432 index=0 of=2
11:01:22.083  CONNECT_SUCCESS round=4 endpoint=192.168.68.59:55432
11:01:22.084  SESSION_START
```

**Tap → authenticated session: 5.107 s.** Debian confirmed `connected yes / state connected`,
battery 79% flowing. `192.168.68.75` (Ubuntu) was never dialled. Ubuntu's trust entry remained
present at `[0]`; nothing was forgotten; Debian was not re-paired.

Selecting the *offline* Ubuntu row first was also verified: `selectedPeer` moved to Ubuntu's
fingerprint and the live Debian session was torn down — the row's identity reaches the
connection layer in both directions.

---

## 13. Scenario 3 — two reachable peers → **PASS** (executed; not environment-limited)

Both daemons live simultaneously; host held 3.0 GiB free throughout.

**3a — select `anyflow-u2404` while Debian is also online:**

```text
11:53:24.061  TARGET_CHANGED peer=135A C045 BFE9 F0A5
11:53:24.116  SESSION_END reason=SocketException          ← the foreign session, ended
11:53:29.142  CONNECT_ATTEMPT round=34 endpoint=192.168.68.75:55432
11:53:29.307  SESSION_START                               ← 5.246 s
```
Desktops: `u2404=connected`, `d13=disconnected`.

**Between 3a and 3b the trust store reordered itself** — `rememberAddresses` re-appended the
peer that had just connected, so the cards swapped to `[0] anyflow-d13, [1] anyflow-u2404`.
This is the volatility described in §2, observed live.

**3b — select `anyflow-d13`, now at index `[0]`, both still online:**

```text
11:54:11.119  TARGET_CHANGED peer=B52C DA20 46ED 006D
11:54:16.198  CONNECT_ATTEMPT round=35 endpoint=192.168.68.59:55432
11:54:16.285  SESSION_START                               ← 5.166 s
```
Desktops: `u2404=disconnected`, `d13=connected`.

Both selections honoured, across a real change of storage order. **§9 satisfied on hardware.**

Also observed, satisfying §7 and §11: while Debian held the session, Ubuntu booted and became
reachable — trusted, online, and first in the list — and did **not** take the session. The
cards read `anyflow-d13: Connected` / `anyflow-u2404: Available`.

---

## 14. Scenario 2 — Debian 13 OFF, Ubuntu 26.04 ON → **PASS**

Executed after `anyflow-u2604` was paired, giving a **three-peer** trust store — a state that
was impossible to use at all before this fix:

```text
[0] anyflow-u2404   135ac045bfe9f0a5…   trusted, OFF
[1] anyflow-d13     b52cda2046ed006d…   trusted, OFF   ← selected, per the brief's precondition
[2] anyflow-u2604   131596bd9834ba6f…   trusted, ON
```

Stricter than the brief asks: the reachable peer is **last**, with **two** offline entries ahead
of it, and the *selected* one is offline. Selecting the offline Debian first tore down the live
Ubuntu 26.04 session, confirming the retarget path on a third identity. Then Connect on the
`anyflow-u2604` row:

```text
12:52:54.978  TARGET_CHANGED peer=1315 96BD 9834 BA6F
12:52:54.979  RETRY_CANCELLED reason=woken early
12:52:59.999  DISCOVERY round=70 endpoints=2
12:52:59.999  CONNECT_ATTEMPT round=70 endpoint=192.168.68.78:55432 index=0 of=2
12:53:00.082  CONNECT_SUCCESS round=70 endpoint=192.168.68.78:55432
12:53:00.085  SESSION_START
```

**Tap → authenticated session: 5.107 s.** Only `192.168.68.78` was dialled — neither
`192.168.68.75` (Ubuntu 24.04) nor `192.168.68.59` (Debian). Desktop confirms `connected yes /
state connected`.

All three trust entries survived, each still holding exactly `[battery.v1, files.v1]`. The
device list rendered `anyflow-u2404: Available`, `anyflow-d13: Available`,
`anyflow-u2604: Connected` — the two peers nobody is dialling do not claim to be connecting,
which is the §39.17 invisibility complaint answered directly.

## 15. SendActivity physical result → **PASS**

Driven through Android's **real share sheet**: My Files → long-press `p1-routed-to-d13.txt` →
Compartilhar → Mais → "AnyFlow · Send with AnyFlow". (An `am start` ACTION_SEND from `adb`
cannot delegate a MediaStore read grant, so the genuine share sheet was used.)

Trust store order at the moment of the send:

```text
[0] anyflow-u2404      ← a first-entry fallback would have sent it here
[1] anyflow-d13        ← chosen in the picker, and where it actually went
```

The picker listed **both** destinations, each with its name *and* fingerprint, preselected to
the current target. Choosing `anyflow-d13` moved the card header to "to anyflow-d13 ·
Fingerprint B52C DA20 46ED 006D" and the session followed.

```text
anyflow-d13:  incoming file offer transfer=6490e5d5 filename=p1-routed-to-d13.txt size=88534
              received, verified and stored transfer=6490e5d5 bytes=88534

SHA256 host    c8e182610128452350619a802b4361338080b3d925378741211432bcfc456606
SHA256 desktop c8e182610128452350619a802b4361338080b3d925378741211432bcfc456606   ✔ match

anyflow-u2404: /home/anyflow/Downloads/AnyFlow/ → a2d.bin only.  Nothing arrived.  ✔
```

An earlier run of the same share, with the Debian daemon in its default attended mode, was
**declined** by the desktop: *"declining an incoming file: no way to ask a human"*. That is the
desktop's consent gate behaving correctly on a headless guest — the known incoming-file GUI
approval debt, out of scope per §22. The daemon was restarted with
`--accept-files-without-asking` purely so the transfer could complete and the digest be
verified. **Routing was already proven by the declined attempt**: only the chosen peer received
the offer at all.

No stale/offline first peer blocked the operation at any point.

---

## 16. Capability smoke (§18)

Targeted only; U2 gates were not re-certified.

| Capability | Result |
|---|---|
| `files.v1` | **PASS** — transfer completed, SHA256 verified (§15). |
| `battery.v1` | **PASS** — 79% flowing to the connected desktop, `last frame 32s ago`. Goes `STALE` between periodic updates; the separate battery defect is out of scope per §22. |
| `clipboard.v1` | **PASS (negative)** — granted to neither peer, so the text share reads *"No paired computer is set up to receive your clipboard. Turn on 'Share clipboard with this computer' in AnyFlow first."* No Send offered. |
| `notifications.v1` | **PASS** — session establishes and announces roles (`announcing roles peer=573C CB84 DA6C 993B roles=2 epoch=1`). Withheld at pairing by design. |
| **Per-peer grants** | **PASS** — after pairing, each desktop holds exactly `[battery.v1, files.v1]` on the tablet; `clipboard.v1` and `notifications.v1` are withheld as `AnyFlowApp.pair` intends. The new target model widened nothing. |

---

## 17. Privacy / logging check (§19)

- **Zero** new `Log.*`, `println` or `Toast` statements introduced by this branch.
- Exactly one new structured field: `TARGET_CHANGED peer=<fingerprint display-short>` — a
  public 64-bit prefix, the same vocabulary `ConnectionEvent` already documents as permitted.
- Exactly one new persisted key: `selectedPeer`, a public fingerprint hex. No content of any
  kind is stored, and no existing persistence was widened.
- `adb logcat -d -s ConnectionService:I` over the entire physical run: **0** occurrences of a
  device name, pairing token, or secret.
- The destination picker renders device names and fingerprints only — never file contents,
  and for text shares nothing about the text (`SendTextScreen` still shows size, not content).

---

## 18. Remaining debts

Introduced or newly visible, none blocking:

1. **`SendActivity` display name for an `am start` URI.** When the provider refuses a
   DISPLAY_NAME query, `SharedFile.displayName()` falls back to the URI's last path segment,
   which rendered as "354". Harmless and harness-only — a real share sheet grant resolves the
   name correctly — but the fallback could be more honest.
2. **A picker choice retargets the global connection.** Correct today, because a transfer
   needs the session. If AnyFlow ever supports concurrent sessions, the share destination and
   the connection target should separate again.
3. **`ConnectionService` retarget happens before `ensureCoordinator()`** on a cold start, so
   `onTargetChanged` is a no-op on the very first start. Harmless — the loop reads the fresh
   target on round 1 — but it means the wake path is only exercised on re-targets.
4. **Trust-store order remains volatile** (`addPeer` appends). No longer harmful, but a peer
   list that reshuffles on every connection is surprising in the UI.

Explicitly **not** touched, per §22.

---

## 19. Git

```text
$ git status --short
 M android/app/src/main/java/io/github/yurisismotto/anyflow/AnyFlowApp.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/net/ConnectionCoordinator.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/net/ConnectionEvent.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/service/ConnectionService.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/store/TrustStore.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/DevicesScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainState.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/SendActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/UiMapping.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/UiMappingTest.kt
?? U2-HARDENING-P1-MULTIPEER.md
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? android/app/src/main/java/io/github/yurisismotto/anyflow/store/PeerTarget.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/MultiPeerRoutingTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/PeerTargetTest.kt

$ git diff --check
(clean)

$ git diff --stat
 11 files changed, 690 insertions(+), 38 deletions(-)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untracked at baseline and is **unmodified** by this
branch. Nothing was committed, pushed, or opened as a PR.

---

## 20. Source audit (§13)

```text
$ grep -RIn "firstOrNull()" android/app/src/main
```

Every remaining hit is either prose in a KDoc describing the old defect, or semantically
unrelated to peer destination selection:

| Site | Why it is not peer routing |
|---|---|
| `net/PinnedTrustManager.kt:38` | `chain?.firstOrNull()` — the **leaf certificate** of a TLS chain. |
| `ui/SendActivity.kt:257` | First URI of an `ACTION_SEND_MULTIPLE` share; documented as "only the first item is sent". |
| `ui/Navigation.kt:101` | First path segment of a route string. |
| `store/TrustStore.kt:149` | `peers().firstOrNull { it.fingerprint.contentEquals(…) }` — a **predicate lookup by fingerprint**, not a positional pick. Fingerprints are unique. |
| KDoc in `PeerTarget`, `ConnectionService`, `AnyFlowApp`, `SendActivity`, `DevicesScreen`, `MainState` | Prose quoting the defect being fixed. |

```text
$ grep -RInE "peers\(\).*first|first.*peer|trusted.*first" android/app/src/main
```
Only `TrustStore.kt:149` above, plus two unrelated comments in `NotificationSource.kt`.

Other positional picks reviewed and cleared: `Protocol.kt:99`, `ClipboardCaches.kt:75/138`,
`NotificationEcho.kt:99`, `NotificationIdentity.kt:242`, `NotificationSource.kt:269` — all
oldest-entry eviction in LRU/TTL caches, none peer-related.

Two peer-related single-element picks were rewritten to `single()` so no site in the app can
be *read* as "whichever is first": `MainActivity.kt:375` (clipboard tile, guarded by
`size == 1`) and `PeerTarget.kt:123` (guarded by `peers.size == 1`).

**No hidden first-peer routing remains.**

---

## 21. Scope check (§22)

Verified **not** fixed by this branch: battery absence / fake 0%; notification capability
convergence; clipboard sensitive test harness; `real_dbus` clippy assertion; CI clippy; Debian
packaging docs; incoming-file GUI approval; QR pairing camera orientation; branding / logo /
pastel palette; Quick Panel; KDE.

One change goes beyond the brief's three named files and is declared here rather than buried:
**`ui/UiMapping.kt` — `statusFor(…, targeted)`**. U2 §39.17 lists the invisibility as part of
P1 ("the UI shows Connecting… for the unreachable peer *and* for the reachable one, with no
indication that the second is never being attempted"). Fixing the routing without it would
have left the defect's own symptom in place. It is 16 lines, covered by 3 tests, and confined
to how a card is labelled.

---

## 22. Verdict

The certified reproduction is eliminated. An offline first trusted peer no longer blocks a
later selected reachable peer — proven on all three preserved U2 machines, in both directions,
with the trust store reordering itself mid-test, with three desktops trusted simultaneously and
the reachable one last behind two offline entries, and with a real Android share sheet routing a
file to the chosen peer by fingerprint and to no other.

All three mandatory physical scenarios pass. Scenario 3, which the brief allowed to be recorded
as environment-limited, was executed rather than skipped.

```text
U2 P1 MULTI-PEER REMEDIATION: PASS
FIRST-TRUSTED-PEER DEFECT FIXED
PHYSICAL REGRESSION PASS
```
