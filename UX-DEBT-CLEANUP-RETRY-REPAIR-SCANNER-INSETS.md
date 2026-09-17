# AnyFlow — UX Debt Cleanup

## Retry after Decline · Safe Re-pair after Revoke · Scanner Insets

Branch: `fix/ux-debt-retry-repair-scanner-insets`
Date: 2026-09-17
Host: Fedora 44, Linux 7.1.9-200.fc44.x86_64
Hardware under test: Samsung SM-X620 tablet, Android 16 (API 36), One UI 8.0

---

## 1. Scope

The three debts recorded in `UX-HARDENING-FILE-APPROVAL-QR-ORIENTATION.md` §27, and
nothing else.

| Id | Defect | Outcome |
| --- | --- | --- |
| UX-DEBT-01 | A declined Android file share cannot be retried with the same file | **PASS** |
| UX-DEBT-02 | A desktop-side revoke leaves the tablet unable to re-pair without "Forget this device" | **PASS** |
| UX-DEBT-03 | The ZXing prompt strip is clipped behind the navigation bar in portrait | **PASS** |

Not implemented, and verified absent from the diff in §27: branding, logo, palette
redesign, Quick Panel, KDE tray/StatusNotifier, RPM/DEB packaging, Android lint
cleanup or suppression, battery hotplug, notification queue recovery, concurrent
multi-peer sessions, trust-store cosmetic ordering, phone-form-factor certification,
unrelated UX cleanup.

**No protobuf and no wire change.** UX-DEBT-02 turned out to be exactly what the
brief predicted — a lifecycle/state-routing bug on the Android side. The desktop was
already correct end to end and **not one desktop file was modified** (§26).

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was neither modified nor staged (mtime still
2026-09-15 23:16:42). `git add .` was not used. Nothing was committed, pushed or
opened as a PR.

---

## 2. Baseline

```
$ git branch --show-current
fix/ux-debt-retry-repair-scanner-insets

$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md

$ git log --oneline -3
932234a Merge pull request #31 from yurisismotto/fix/ux-hardening-file-approval-qr-orientation
27c5935 fix(ux): add file approval and unlock pairing scanner
c0aac3a Merge pull request #30 from yurisismotto/docs/u2-test-ci-final-evidence

$ git rev-list --left-right --count develop...HEAD
0	0
```

Branch level with `develop`, PR #31 merged, working tree clean apart from the
historical untracked file. `git diff --check` clean.

Physical baseline, read off both machines before anything was touched:

* tablet trust store: four peers — `anyflow-u2604`, `anyflow-d13`, `anyflow-u2404`
  (the preserved U2 VM peers) and `Fedora`, each with `battery.v1` + `files.v1`;
  `selectedPeer = df65d3e4…`;
* desktop: tablet `6532889e82ba83d0782cc644e7a21fc3` / `573C CB84 DA6C 993B`, paired
  and connected.

---

## 3. UX-DEBT-01 — defect of record

Observed during the previous sprint and reproduced here: a file declined on the
desktop could not be offered again for the life of the Android process. §12 of that
report had to use a second filename to test an Accept.

---

## 4. UX-DEBT-01 — root cause

One line, in `SendActivity.SendScreen`:

```kotlin
val mine = transfers.filter { it.sending && it.filename == name }
if (mine.isEmpty()) { /* the Send button */ } else { for (t in mine) TransferRow(t) }
```

Three facts combine, and none is a bug alone:

1. **`transfers` is `app.files.visible`, which is every transfer of the process.**
   `FileTransferManager.transfers` is a `ConcurrentHashMap` that is *never* pruned —
   `finish()` only flips state; there is no `remove` anywhere in the class. That is
   deliberate: the Activity screen is a history.
2. **So one decline leaves a permanent row for that display name**, and `mine` is
   never empty again. The Send button is replaced by the terminal row and does not
   come back — not on a re-share, not on a new intent, not until the process dies.
3. **A display name is not an identity.** The predicate ignores the source URI and
   the destination peer, so two different files both called `photo.jpg` alias onto
   each other, and a transfer to one computer hides the Send button for another.

`TransferUi` carries neither a peer nor a source URI, so the screen could not have
disambiguated even if it had tried.

The protocol half was never wrong. `offer()` already mints a fresh 16-byte random
id per attempt **and returns it** — `UiMapping.sendOutcome` was throwing it away:

```kotlin
onSuccess = { SendAttempt.Sent },   // the id, discarded
```

So the fix is not to invent an identity. It is to stop discarding the one that
already existed.

---

## 5. Retry state model — before and after

### Before

```
SendAttempt = Idle | Sending | Sent | Failed(message)

screen  = if (transfers.any { it.sending && it.filename == name })
              terminal rows, no button, forever
          else
              Send button
```

### After

```
SendAttempt = Idle | Sending | Sent | Offered(transferId) | Failed(message)
              (Sent is now text-only: a clipboard send is delivered on return
               and the screen closes; a file offer has a transfer to follow)

SendSurface = Offer | InFlight(transfer?) | Ended(transfer)

sendSurface(attempt, transfers) = when (attempt) {
    is Offered -> transfers.firstOrNull { it.transferId == attempt.transferId }
                      .let { null -> InFlight(null); terminal -> Ended(it); else -> InFlight(it) }
    else       -> Offer
}

canStartSend(attempt, surface) = when (surface) {
    Offer    -> attempt.canSend      // Idle or Failed
    Ended    -> true                 // settled: a retry is a new attempt
    InFlight -> false                // one at a time from this screen
}
```

Both routes the brief allowed, because they fall out of the same change:

* **A — the terminal row exposes a Retry.** `Ended` renders the row plus a button
  labelled by `retryButtonLabel(state)`: `"Send again"` after a completed transfer,
  `"Try again"` after a decline, failure or cancellation. A completed send is not a
  failure and is not offered as one.
* **B — a fresh Sharesheet invocation works.** A new `SendActivity` starts at
  `Idle`, whose surface is `Offer` whatever history holds.

`canSend` was also flipped from a blacklist (`!is Sending && !is Sent`) to a
whitelist (`is Idle || is Failed`), so a state added later is not sendable until
somebody says it is.

---

## 6. UX-DEBT-01 — security invariants

| Invariant | How it holds |
| --- | --- |
| a retry is a **new** transfer | `startSend` → `files.offer(peer, uri)`, the ordinary path. `StreamAuth.newTransferId(random)` mints 16 fresh random bytes; nothing about the file feeds it |
| no TransferId reuse | observed on hardware: `6e980eac` → `203112a6` → `fc1736e6`, three distinct ids for the same file (§8) |
| no challenge/HMAC reuse | the challenge lives on the `Transfer` object and `finish()` sets it to `null`; a new transfer gets a new object and the desktop issues a new challenge |
| no state-machine reuse | `TransferState.canTransitionTo` lets nothing leave a terminal state. Asserted over all seven states |
| the old decline stays terminal | `anyflow transfers` after the run still reports `6e980eac … cancelled … declined by the user` |
| history is never rewritten | `CANCELLED → COMPLETED` and `FAILED → COMPLETED` are both refused by the transition table |
| retry is user-initiated only | it is a `Button.onClick`. No timer, no coroutine, no automatic re-offer anywhere in the diff |
| no retry storm | `canStartSend` refuses while `InFlight`; the handler re-reads live state so two taps in one frame cannot both fire. Four rapid taps produced **one** offer (§8) |
| the target peer is never inherited | `SendAttempt.Offered` carries a transfer id and nothing else. The destination is re-resolved from `PeerTarget.resolve(UiMapping.fileDestinations(peers), selectedHex)` at the moment of the tap, and is displayed above the button |
| grants are re-checked | `fileDestinations` is re-derived every recomposition, and `offer()` re-asks `isAuthorized` |
| no file bytes in retry state | `Offered` holds a 32-char hex string |

---

## 7. UX-DEBT-01 — tests

`android/app/src/test/.../SendRetryTest.kt`, **21 tests**, JVM only.

| Brief | Test |
| --- | --- |
| A declined attempt is terminal | `a declined attempt is terminal and shown as a settled row`; `nothing moves a declined transfer back to a live state`; `a settled attempt cannot be rewritten as a success` |
| B same file creates a fresh attempt | `a fresh share of the same file offers the send button again`; `the send surface is not decided by filename`; `a settled attempt offers a retry in place` |
| C fresh attempt, different TransferId | `every minted transfer id is fresh and full length` (500 mints, no repeat); `a retry of the same file gets a different transfer id`; `an attempt identifies a transfer and nothing else` |
| D failure can be retried | `a failed attempt can be retried`; `an offer refused before a transfer existed leaves the button live` |
| E completed does not block | `a completed send does not block sending the same file again` |
| F same name does not alias | `two transfers with the same filename do not alias`; `a received transfer is not this screen's attempt` |
| G rapid retry does not duplicate | `a second tap while an offer is in flight starts nothing`; `startability follows exactly the terminal boundary`; `no attempt state leaves the button claiming to be sending` |
| H target peer not silently changed | `a retry resolves its destination the same way a first send does` |
| I capability/grant state respected | `a retry respects a grant withdrawn since the declined attempt` |
| J disconnect takes the normal path | `a retry with no session is refused and stays retryable`; `a retry failure never puts a content uri on screen` |

`UiMappingTest` was updated for the new model (3 assertions) and still passes; P1
explicit-target routing (`MultiPeerRoutingTest`, `PeerTargetTest`) is untouched and
still green.

---

## 8. UX-DEBT-01 — real Decline → Retry → Accept

Fedora desktop, normal `anyflowd --log info`, normal `anyflow-gui` attached as the
approval provider, **no** `--accept-files-without-asking`. Real Android Sharesheet
(`ResolverActivity` → AnyFlow). Desktop buttons pressed through AT-SPI.

Fixture: `ux-debt-01-retry.txt`, 405 323 bytes, SHA256
`ce570b26c4abbf3b34bf0e6548ac951d940900b119f93f1818118ef8d89c2c79`.

```
10:04:13  tablet   offering ux-debt-01-retry.txt (405323B) as 6e980eac
10:04:13  desktop  incoming file offer transfer=6e980eac … filename=ux-debt-01-retry.txt size=405323
          desktop  GUI: "Incoming file — SM-X620 wants to send you a file.
                          ux-debt-01-retry.txt · 395.8 KB · text/plain"   [Decline] [Accept]
10:04:43  desktop  PRESSED: Decline
          desktop  transfer ended transfer=6e980eac state=cancelled reason=declined by the user
10:04:43  tablet   transfer 6e980eac CANCELLED (DECLINED_BY_USER)
```

The tablet screen at that moment — **this is the fix**:

```
  ux-debt-01-retry.txt
  To 395,8 KB
  declined
  [ Try again ]        ← did not exist before; the screen ended at [Close]
  [ Close ]
```

```
10:05:12  tablet   offering ux-debt-01-retry.txt (405323B) as 203112a6      ← NEW id
10:05:12  desktop  incoming file offer transfer=203112a6 …                   ← SECOND prompt
10:05:20  desktop  PRESSED: Accept
10:05:24  tablet   sent 203112a6; awaiting the computer's verdict
10:05:24  tablet   transfer 203112a6 COMPLETED
          desktop  received, verified and stored transfer=203112a6 … bytes=405323
```

```
source     : ce570b26c4abbf3b34bf0e6548ac951d940900b119f93f1818118ef8d89c2c79
destination: ce570b26c4abbf3b34bf0e6548ac951d940900b119f93f1818118ef8d89c2c79
             /home/yuri/Downloads/AnyFlow/ux-debt-01-retry.txt   405323 bytes
```

The completed screen reads `Sent` + `[ Send again ]`, not `[ Try again ]`.

**Two further physical checks on the same session:**

*The original defect scenario.* Closed the screen and re-shared the **same** file
through the Sharesheet, now with one declined *and* one completed transfer of that
filename in history:

```
  Send button present : True
  terminal row shown  : False
```

Under the old predicate this screen would have shown two terminal rows and no
button.

*Retry storm.* Four `input tap` events on Send in immediate succession produced
**one** offer, `fc1736e6`, and the screen showed `Waiting…` with no button.

Final ledger, `anyflow transfers`:

```
  203112a6  receiving <- SM-X620   completed
  6e980eac  receiving <- SM-X620   cancelled   declined by the user
  fc1736e6  receiving <- SM-X620   cancelled   declined by the user
```

PASS against every clause of §8: same original filename, no app restart, new
transfer identity, a second desktop approval prompt, matching SHA256, no stale
`.part` (`ls ~/Downloads/AnyFlow | grep -E '\.part|\.tmp|\.partial'` → none), and
the declined attempts still terminal.

---

## 9. UX-DEBT-02 — defect of record

With the desktop having revoked the tablet and the tablet still trusting the
desktop, scanning a fresh QR did not recover pairing. The desktop logged

```
a revoked device is pairing again; it must prove the new token and be confirmed by hand
… timed out waiting for PAIR_REQUEST
```

and recovery required "Forget this device" on the tablet.

---

## 10. UX-DEBT-02 — root cause

**Reproduced on the hardware, with no scan involved at all.** With the tablet
revoked on the desktop, a fresh pairing window open, and the operator simply
tapping *Connect* on the tablet's Fedora card:

```
daemon:  a revoked device is pairing again; it must prove the new token and be
         confirmed by hand  peer=573C CB84 DA6C 993B
daemon:  connection ended … error=protocol violation: timed out waiting for PAIR_REQUEST
logcat:  CONNECT_ATTEMPT round=1 endpoint=192.168.68.72:55432
logcat:  CONNECT_FAILURE round=1 kind=terminal reason=the computer no longer knows this device
logcat:  STOPPED reason=the computer no longer knows this device
```

Both recorded lines, verbatim. The mechanism:

**Two paths on this phone dial a desktop, and only one carries a token.**

* `AnyFlowApp.pair` — a scan. Dials with the QR's token and answers a challenge.
* `AnyFlowApp.connect`, driven by `ConnectionService`'s coordinator — a
  reconnection. Dials with **no** token, correctly: a reconnection is not a pairing
  and must never carry pairing authority.

Against a desktop that has revoked this device *and* has a pairing window open, the
desktop treats it as unknown (`anyflow_core::session`, `treat_as_unknown`), answers
`HELLO_STATUS_PAIRING_REQUIRED` with a nonce, and waits. The tokenless path then
stops at

```kotlin
val token = pairingToken ?: return ConnectResult.PairingRequired
```

having sent no `PAIR_REQUEST`. `ConnectionService` maps that to
`DialResult.Terminal("the computer no longer knows this device")`, the coordinator
sets `stopped`, publishes `GaveUp` and calls `stopSelf()` — *while the person is
part-way through re-pairing*, and holding the desktop's pairing connection open for
the full handshake timeout.

Two further defects fall out of the same area and are fixed with it:

**A scan's token is silently discarded when the desktop answers TRUSTED.**
`pair()` took whatever `HELLO_ACK` status came back; on `HELLO_STATUS_TRUSTED` it
went straight to `established` → `addPeer`, and the QR's single-use token was never
proved. That is the *correct* protocol behaviour — the responder decides what it
requires — but it was reported as a fresh pairing. The previous sprint recorded
exactly this in its §20 test 8a: *"the tablet was already paired with this desktop
from test 7, so the Android side took its already-trusted path and this resolved as
a reconnect rather than fresh trust."*

**A re-pair destroyed local settings.** `pair()` built a brand-new `TrustedPeer` and
`trustStore.addPeer()` replaced by fingerprint, so a re-pair reset the clipboard
policy, emptied the chosen notification app list, and reduced grants to
`negotiated − clipboard − notifications`. In this sprint's own physical run the
desktop's `auto_grant` negotiated only `battery.v1`, so the old code would have
**silently dropped `files.v1`** from the tablet's Fedora record.

### Not the cause, and checked

The desktop is correct and was not changed. `PairingSession` enforces single use,
expiry and a three-attempt cap; `pairing_mode_active` additionally requires an
operator attached to the confirmation channel; a timed-out `accept_pairing` does not
consume the window (verified: the window was still `OPEN` after the reproduction).
Nonce length (32) and handshake timeout (15 s) match on both sides.

---

## 11. Pairing lifecycle — before and after

### Before

```
scan ──► pair(payload)
             ├── dial with token ──► HELLO_ACK
             │                         TRUSTED          ──► established (token unused, reported as "paired")
             │                         PAIRING_REQUIRED ──► PAIR_REQUEST ──► proof ──► confirm ──► established
             └── on success: addPeer(fresh record)   ← replaces grants + policies

meanwhile, unsynchronised:
ConnectionService ──► connect() with NO token ──► PairingRequired ──► Terminal ──► stopSelf()
```

### After

```
scan ──► pair(payload)
             ├── claim pairingInFlight(fingerprint)      ← compareAndSet; a second scan is refused
             ├── dial with token ──► HELLO_ACK
             │                         TRUSTED          ──► Established(provedPairing = false)
             │                         PAIRING_REQUIRED ──► PAIR_REQUEST ──► proof ──► confirm
             │                                              ──► Established(provedPairing = true)
             ├── on success: upsertPairedPeer(record)    ← facts refresh, decisions persist
             ├── returns PairOutcome(peer, provedToken)  ← the screen says which happened
             └── finally: release pairingInFlight

ConnectionService.dial ──► PairingGate.holdFor(target, pairingInFlightHex)
                              ├── same computer  ──► DialResult.Transient  ← backs off, comes back
                              └── anything else  ──► dial as before
```

`Transient`, never `Terminal`: a pairing that fails leaves the link exactly where it
was instead of stopping the loop.

---

## 12. Trust / revocation invariants

Every rule in the brief's §13, and where it holds. **Nothing was weakened.**

| Rule | How it holds |
| --- | --- |
| desktop revoke stays authoritative | `anyflow_core::session` unchanged. A revoked fingerprint with no pairing window open still gets `HELLO_STATUS_REJECTED` → `FailureKind.REVOKED` → the loop stops |
| old local trust never bypasses proof | the phone sends its token and the *desktop's* `HELLO_ACK` status decides. There is no branch on the local trust store anywhere in `pair()` or `handshake()` |
| "if Android trusts it, accept anyway" | absent. Never written |
| "restore trust if the fingerprint matches" | absent. The fingerprint selects *which record to update*, never *whether to trust* |
| the QR's entry is not deleted on scan | `upsertPairedPeer` updates; `removePeer` is only reachable from "Forget this device" |
| identity is not replaced before proof | trust is written on exactly one line, inside `is ConnectResult.Established ->`. No other branch reaches the store |
| proof fails → record unchanged | every failure path returns `Result.failure` without touching the store, and leaves `selectedPeer` alone. Asserted over five failure outcomes |
| confirmation declined → nothing restored | `PAIR_STATUS_DECLINED_BY_USER` → `ConnectResult.Failed` |
| token expired → honest failure | the phone holds no expiry state to bypass; the QR payload carries no validity field at all (asserted). Expiry is `PairingSession::is_expired` on the desktop. Observed twice during this sprint: the 900 s and 3600 s windows both closed on time and produced `Pairing window expired` |
| different fingerprint → new-peer semantics | `mergePairing(existing = null, …)` returns the paired record verbatim |
| same fingerprint, changed device id/address | facts refresh, fingerprint/SPKI unchanged. Observed: `pairedAtUnix` refreshed, fingerprint byte-identical |
| never route by display name | the merge is keyed on fingerprint; `PeerTarget.resolve` is untouched |
| a scan cannot race the tokenless dialer | `PairingGate` |
| two scans cannot race each other | `pairingInFlight.compareAndSet` |

`upsertPairedPeer`'s rule, stated once: **facts refresh** (device id, display name,
the address that answered, pairing time) — **decisions persist** (capability grants,
unioned; clipboard policy; notification policy). Safe because the fingerprint is the
pinned SPKI the TLS handshake was checked against and the identity the proof was
bound to.

### 12.1 Grant non-escalation — pinned before PR

**The decision.** For an already-known peer with the **same fingerprint/SPKI**, a
re-pair preserves the local user's decisions: capability grants, clipboard policy
and notification policy. A desktop-side revoke does not mean the Android user
revoked their own consent for that same cryptographic identity, and silently
switching off notification mirroring somebody had configured would be a surprise
with no notice. This semantic is kept.

**The security boundary.** Preserving consent must never become *inventing* it. A
re-pair may not grant a sensitive capability merely because the fresh negotiation
advertised it — negotiating `notifications.v1` means both sides *support* it, not
that this computer may read the notification stream.

**What was actually wrong.** The invariants held, for the wrong reason. The merge
unioned grants and knew nothing about sensitivity; the only thing stopping
escalation was a subtraction written inline in a *different file*, at the point the
fresh record was built:

```kotlin
grantedCapabilities = connection.negotiatedCapabilities
    .toSet() - ClipboardCapability.ID - NotificationsCapability.ID
```

A refactor of that line, a widened auto-grant policy, or a third sensitive
capability added without remembering to subtract it would have let a re-pair hand a
computer the notification stream. So the rule is now **named once and enforced where
the union happens**.

New: `capability/SensitiveCapabilities.kt` —
`NEVER_AUTO_GRANTED = { clipboard.v1, notifications.v1 }`, with `files.v1` and
`battery.v1` deliberately outside it.

`TrustedPeer.mergePairing` gained two lines:

```kotlin
val grantable = paired.grantedCapabilities - SensitiveCapabilities.NEVER_AUTO_GRANTED

if (existing == null || !existing.fingerprint.contentEquals(paired.fingerprint)) {
    return paired.copy(grantedCapabilities = grantable)      // new peer: inherit nothing
}
… grantedCapabilities = existing.grantedCapabilities + grantable
```

A sensitive capability is in the result **if and only if `existing` already held
it** — which is to say, if and only if the person granted it themselves. The
fingerprint check makes invariant 5 structural rather than a property of the
caller: inheriting another computer's notification grant is the one mistake this
function is now unable to make. `AnyFlowApp` uses the same set instead of its two
inline subtractions.

**Behaviour today is unchanged**, and provably so: the fresh record already excluded
both sensitive ids, so `grantable == paired.grantedCapabilities` and the union is
identical. The change converts an emergent property into an enforced one.

**Tests** — 11 added to `PairingRecoveryTest` (19 → 30):

| Invariant | Test |
| --- | --- |
| the boundary itself | `the never auto granted set names both sensitive capabilities` |
| 1 preserve notifications | `a re-pair preserves an existing notification grant` |
| 2 never invent notifications | `a re-pair does not invent a notification grant` |
| 3 preserve clipboard policy | `a re-pair preserves existing clipboard policy` |
| 3 never invent clipboard | `a re-pair does not invent clipboard permission` |
| 4 auto-grantable still follow policy | `a re-pair still adds an auto grantable capability it negotiated` |
| 5 different fingerprint inherits nothing | `a different fingerprint inherits no grants` |
| 5 a first pairing is bound by the same rule | `a first pairing does not grant a sensitive capability` |
| 6 no mutation on failure | `a failed pairing leaves grants and policies byte-equivalent` |
| 6 structurally, not by convention | `pairing writes trust on exactly one line and only when established` |
| 7 chosen computer unchanged | `a re-pair leaves the chosen computer alone` |

The non-escalation tests carry the fresh record with `notifications.v1` and
`clipboard.v1` *already in it* — the shape a future mistake would produce — so they
fail if the rule is removed rather than passing on today's accident.

That was checked by mutation: reverting `mergePairing` to the naive union and
dropping the fingerprint guard fails exactly four tests —
`a re-pair does not invent a notification grant`,
`a re-pair does not invent clipboard permission`,
`a first pairing does not grant a sensitive capability`,
`a different fingerprint inherits no grants` — and **no others**. The preservation
tests stay green, so the two halves of the rule are pinned independently.

`a failed pairing leaves grants and policies byte-equivalent` compares the peer list
serialized through the real `ClipboardPolicy.toJson` / `NotificationPolicy.toJson`
across every failure outcome (rejected proof, not in pairing mode, declined, rate
limited, revoked, bad confirmation, `PairingRequired`). The structural companion
asserts that `pairWithToken` reaches the trust store through exactly one mutator,
once, and that no failure arm contains a `trustStore.` call at all.

---

## 13. UX-DEBT-02 — tests

`android/app/src/test/.../PairingRecoveryTest.kt`, **30 tests**, JVM only. The
11 covering grant non-escalation are listed in §12.1.

| Brief | Test |
| --- | --- |
| A unknown peer + fresh QR | `an unknown computer is stored exactly as the pairing produced it`; `a different fingerprint is a different computer however it is named` |
| B locally trusted + remote trusts | `a scan against a computer that still trusts us is reported as a reconnection` |
| C locally trusted + remote revoked | `a re-pair after a revoke keeps the record and adds nothing to it` |
| D expired QR cannot restore trust | `a scanned code carries no expiry the phone could honour or ignore` |
| E wrong token cannot restore trust | `a proof computed from the wrong code does not match` |
| F confirmation declined | `no failure outcome carries anything a trust record could be built from` |
| G no duplicate peer | `a successful re-pair keeps the same fingerprint and one record` |
| H metadata/grants preserved | `a re-pair preserves grants and policies the person set`; `a re-pair refreshes identity metadata and remembers both addresses`; `remembered addresses stay bounded` |
| I another peer untouched | `pairing one computer cannot touch another` |
| J selectedPeer fingerprint-based | `the chosen computer still resolves after a re-pair`; `a re-paired computer is not resolved by its position` |
| K stale connection cannot interfere | `reconnection is unaffected when no code is being proved`; `the tokenless dialer holds off for the computer being paired`; `the hold matches a fingerprint whatever case it is spelled in`; `another computer keeps connecting while one is being paired`; `the hold is a transient dial result and not a terminal one` |
| L proof-of-possession mandatory | `a proof computed from the wrong code does not match`, on top of the nine existing `PairingProofTest` bindings (responder, initiator, nonce, domain separation, length prefixing, two cross-language known answers) |

---

## 14. Real desktop-revoke → fresh-QR re-pair

Full sequence, Fedora + SM-X620, **no "Forget this device" at any point**.

```
01:55:50  desktop  anyflow unpair 6532889e… → "revoked 573C CB84 DA6C 993B"
01:56:40  tablet   CONNECT_FAILURE round=4 kind=terminal reason=this device's pairing was revoked
          tablet   STOPPED reason=this device's pairing was revoked
          tablet   card reads "Failed — this device's pairing was revoked", Fedora still
                   trusted locally, still Selected, grants intact
09:58     desktop  anyflow pair --ttl 28800  → fresh window, QR rendered and displayed
10:00:07  tablet   operator scans the QR with the rear camera
          desktop  a revoked device is pairing again; it must prove the new token and be
                   confirmed by hand  peer=573C CB84 DA6C 993B
          desktop  A device proved it holds the pairing code:
                     name        SM-X620
                     device id   6532889e82ba83d0782cc644e7a21fc3
                     fingerprint 573C CB84 DA6C 993B
          desktop  confirmed → "Paired with 573C CB84 DA6C 993B."
          desktop  session established device=6532889e… peer=573C CB84 DA6C 993B
                   capabilities=["battery.v1"]
10:00:50  tablet   CONNECT_SUCCESS round=1 endpoint=192.168.68.72:55432 → SESSION_START
```

The desktop's confirmation was answered programmatically by a watcher that verifies
the offered device id **and** fingerprint against the expected tablet before
answering `y`, and answers `n` on any mismatch — the same FIFO-driven procedure the
previous sprints used. The proof itself, the single-use token and the confirmation
gate are all the daemon's, unchanged.

Recovery required **proof + confirmation**, and **no Android "Forget this device"**.

The tablet's Fedora record across the whole re-pair, diffed field by field:

```
BEFORE  addresses ["192.168.68.72:55432"]  clipboardPolicy {send,receive,!auto}
        deviceId 795fec…  deviceName Fedora  grants [battery.v1, files.v1]
        notificationPolicy {mirror, allowedApps []}  pairedAtUnix 1789613105

AFTER   addresses ["192.168.68.72:55432"]  clipboardPolicy {send,receive,!auto}
        deviceId 795fec…  deviceName Fedora  grants [battery.v1, files.v1]
        notificationPolicy {mirror, allowedApps []}  pairedAtUnix 1789650050
```

The **only** changed field is `pairedAtUnix`. `selectedPeer` unchanged. Exactly one
Fedora peer — four rows before, four after.

This is also where the merge earned itself: the desktop negotiated only
`battery.v1`, so the old `addPeer` would have rewritten the record's grants to
`["battery.v1"]` and silently dropped `files.v1`. `anyflow grant … files.v1` was
then issued on the desktop for the file test, and the daemon's own renegotiation
("granted a capability this session cannot use; ending it so the device reconnects")
brought the session back with `["battery.v1", "files.v1"]`.

### The gate, in this run

`PairingGate` did not need to fire: the coordinator had already reached `GaveUp` and
stopped hours earlier, so no tokenless dial was in flight when the scan happened.
The gate is what stops the *reproduced* race (§10) — a live coordinator meeting a
pairing-mode desktop — and is covered by five JVM tests. Stated plainly rather than
claimed as physically exercised.

---

## 15. Preserved multi-peer state

The tablet's trust store compared against the snapshot taken at 01:54, before
anything in this sprint:

```
  peers at sprint start: 4    peers now: 4
  - anyflow-u2604   grants=[battery.v1, files.v1]  identical-to-start=True   changed=[]
  - anyflow-d13     grants=[battery.v1, files.v1]  identical-to-start=True   changed=[]
  - anyflow-u2404   grants=[battery.v1, files.v1]  identical-to-start=True   changed=[]
  - Fedora          grants=[battery.v1, files.v1]  identical-to-start=False  changed=[pairedAtUnix]
  selectedPeer: df65d3e4ba28edf9  (was df65d3e4ba28edf9)
```

The three preserved U2 VM peers are **byte-identical** — fingerprints, grants,
addresses, both policy objects, pairing times. No VM was booted. P1 explicit-target
routing is untouched; `MultiPeerRoutingTest` (13) and `PeerTargetTest` (18) still
pass, and the tablet's device list still shows four separate cards with the correct
per-peer capability chips.

---

## 16. UX-DEBT-03 — defect of record

Measured on the SM-X620 with the scanner open in portrait, before the fix:

```
zxing_status_view         [623,2842][1176,2880]
navigationBars inset      [0,2784][1800,2880]   bottom = 96px
navigationBarBackground   [0,2784][1800,2880]
```

Every pixel of the prompt was inside the navigation bar.

---

## 17. UX-DEBT-03 — root cause

Three facts, none a bug alone:

1. **The app targets SDK 35**, so Android 15's edge-to-edge enforcement applies: the
   activity window spans the whole display (`[0,0][1800,2880]`, confirmed in the
   dump) and the system bars are drawn *over* it.
2. **`zxing-android-embedded:4.3.0` predates that enforcement.** Its
   `zxing_barcode_scanner.xml` anchors `zxing_status_view` with
   `layout_gravity="bottom|center_horizontal"` in a full-bleed `FrameLayout` and
   consumes no insets anywhere. Read out of the AAR to confirm.
3. **`zxing_CaptureTheme` inherits `android:Theme.Holo.NoActionBar.Fullscreen`**,
   which hides the *status* bar and says nothing about the navigation bar, and sets
   no `fitsSystemWindows`.

So the prompt is laid out against the bottom of the window, and the bottom of the
window is behind the navigation bar. It is not the dependency's layout alone, not
the theme alone and not AnyFlow's manifest override from the previous sprint — it is
edge-to-edge meeting a pre-edge-to-edge layout.

---

## 18. Scanner inset architecture

The smallest Android-native solution the brief allowed: **a `CaptureActivity`
subclass that owns only inset handling.**

```kotlin
class PairingCaptureActivity : CaptureActivity() {
    override fun initializeContent(): DecoratedBarcodeView {
        val scanner = super.initializeContent()
        ScannerInsets.attach(scanner.statusView)
        return scanner
    }
}
```

`initializeContent` is the library's documented seam: `protected`, returns the
inflated `DecoratedBarcodeView`, and `getStatusView()` on it is public. The
superclass calls it, takes the result and hands it straight to `CaptureManager`, so
there is no window in which the prompt is on screen without its handler attached.

`ScannerInsets` is the rule, and it is pure:

```kotlin
data class Edges(val left: Int, val right: Int, val bottom: Int)

fun occludingTypes() = navigationBars() or displayCutout() or systemGestures()

fun padding(base: Edges, insets: Edges) = Edges(
    base.left + insets.left, base.right + insets.right, base.bottom + insets.bottom,
)

fun attach(status: View) {
    val base = Edges(status.paddingLeft, status.paddingRight, status.paddingBottom)
    ViewCompat.setOnApplyWindowInsetsListener(status) { view, windowInsets -> … }
    ViewCompat.requestApplyInsets(status)
}
```

Deliberate choices:

* **`base` is captured once, at attach.** Padding is absolute, never added to what
  the view already has. Insets are re-delivered on every rotation, navigation-mode
  change and inset-animation frame, and a handler that accumulated would walk the
  prompt up the screen a little further each time the tablet was turned. Proven on
  hardware in §23.
* **No top inset.** The strip is bottom-anchored; padding its top would push the
  text *down*, towards the bar being avoided.
* **`systemGestures` is included** even though on this device its frame is identical
  to `navigationBars` — it costs nothing here and is what makes the guarantee hold
  where it is wider.
* **The insets are returned unconsumed**, so any sibling can still see them.
* **The preview stays full-bleed.** Only the prompt is inset, which is what keeps
  the camera framing correct.
* **No immersive mode, no bar hiding.** Turning the bars off would make the
  measurement read correctly while taking the person's navigation away.
* **No fork, no copied layouts.** Every barcode is still decoded by ZXing; this
  class holds no scanning state, no camera handle and no result handling.

The manifest declares the new activity explicitly, because a subclass gets a
*separate* entry and inherits nothing from the library's:

```xml
<activity
    android:name=".ui.PairingCaptureActivity"
    android:exported="false"
    android:clearTaskOnLaunch="true"
    android:screenOrientation="unspecified"
    android:stateNotNeeded="true"
    android:theme="@style/zxing_CaptureTheme"
    android:windowSoftInputMode="stateAlwaysHidden" />
```

`PairingScanner.options()` gained `.setCaptureActivity(PairingCaptureActivity::class.java)`.

---

## 19. Scanner orientation regression

Every guarantee from the previous sprint is intact, and the new activity carries
them too.

Merged manifest that actually ships:

```xml
<activity android:name="com.journeyapps.barcodescanner.CaptureActivity"
    android:clearTaskOnLaunch="true" android:screenOrientation="unspecified"
    android:stateNotNeeded="true" android:theme="@style/zxing_CaptureTheme"
    android:windowSoftInputMode="stateAlwaysHidden" />
<activity android:name="io.github.yurisismotto.anyflow.ui.PairingCaptureActivity"
    android:clearTaskOnLaunch="true" android:exported="false"
    android:screenOrientation="unspecified" android:stateNotNeeded="true"
    android:theme="@style/zxing_CaptureTheme"
    android:windowSoftInputMode="stateAlwaysHidden" />
```

The only two occurrences of `sensorLandscape` anywhere in the merged manifest are
inside explanatory comments. The library's override is kept even though AnyFlow no
longer launches that activity, so the guarantee survives if anything ever does.

`setOrientationLocked(false)` is still on the scan request. The live activity, read
off the device with the scanner open:

```
overrideOrientation=SCREEN_ORIENTATION_UNSPECIFIED
requestedOrientation=SCREEN_ORIENTATION_UNSPECIFIED
```

No `setRequestedOrientation`, no `SCREEN_ORIENTATION_*` constant and no
`setOrientationLocked(true)` anywhere in production sources — still asserted by
source scan, now across the new files too.

---

## 20. Scanner tests

`android/app/src/test/.../ScannerInsetsTest.kt`, **14 tests**, plus 2 added to
`PairingScannerOrientationTest` (11 → **13**).

| Brief | Test |
| --- | --- |
| A orientation still unlocked | `the scan request explicitly unlocks the orientation`; `the scan request carries no orientation forcing configuration` |
| B manifest orientation unspecified | `the scanner activity is declared unspecified and overrides the library`; `no activity in the manifest forces or overrides an orientation`; **new** `AnyFlow's own scanner activity is declared unspecified too`; **new** `the scan request launches AnyFlow's capture activity` |
| C navigation-bar bottom inset applied | `a bottom navigation bar becomes bottom padding`; `the measured portrait case clears the measured navigation bar`; `the handler consumes navigation bars, cutouts and gesture insets`; `the handler does not pad for the status bar` |
| D zero inset, no gap | `no insets means no invented padding`; `a view with its own padding keeps exactly that when nothing occludes it` |
| E changed inset updates, does not accumulate | `repeated inset deliveries do not accumulate padding` (portrait → landscape → portrait, plus 100 deliveries); `the padding rule is a pure function of base and insets` |
| F independent of fixed pixel sizes | `every edge is cleared whatever the orientation puts where` (six inset vectors incl. gesture nav, side bars, cutouts, foldable, none); `the inset rule hard-codes no sizes` |
| G cancellation still Cancelled | `a cancelled scan is cancelled and not a failed pairing` |
| H pairing parser untouched | `a valid code flows into the existing pairing parser`; `an arbitrary uri is still not a pairing code`; `a rejected outcome carries none of the scanned text` |
| I no new camera permission | `the fix asks for no new permission` (exact ordered list of all eight) |
| J no immersive workaround | `no production code hides the system bars` (nine forbidden tokens, whole app); `no AnyFlow theme turns on a fullscreen or translucent bar flag`; `AnyFlow's capture activity adds inset handling and nothing else` |

Device pixel numbers appear only as *inputs*; every assertion is a relationship.

---

## 21. Physical portrait bounds

```
                        before                     after
zxing_status_view       [623,2842][1176,2880]      [623,2746][1176,2880]
navigationBars          [0,2784][1800,2880]        [0,2784][1800,2880]
```

The view is now 134 px tall — 38 px of text plus the 96 px bottom inset — so the
text occupies `2746..2784` and ends **exactly at the top edge of the navigation
bar**. Before, the text occupied `2842..2880`, entirely inside it.

A screenshot confirms the prompt *"Point at the QR code shown by `anyflow pair`"* is
fully legible above the bar, with the camera preview live behind it.

---

## 22. Physical landscape bounds

```
zxing_status_view       [1163,1666][1716,1800]
navigationBars          [0,1704][2880,1800]
```

Same relationship: 134 px tall, text at `1666..1704`, ending at the bar's top edge.

---

## 23. Rotation while open

Driven with the scanner open, four rotations:

```
step              rotation       zxing_status_view        CaptureActivity records
portrait          ROTATION_0     [623,2746][1176,2880]    1   (ActivityRecord 1789709)
landscape         ROTATION_90    [1163,1666][1716,1800]   1   (ActivityRecord 1789709)
portrait again    ROTATION_0     [623,2746][1176,2880]    1   (ActivityRecord 1789709)
landscape again   ROTATION_90    [1163,1666][1716,1800]   1   (ActivityRecord 1789709)
portrait final    ROTATION_0     [623,2746][1176,2880]    1   (ActivityRecord 1789709)
```

Three things at once:

* the prompt clears the bar in **every** state;
* the bounds are **identical** on every return to an orientation — this is the
  physical proof that padding does not accumulate;
* exactly **one** `CaptureActivity`, and the `ActivityRecord` identity never changes
  — the activity is reconfigured, not duplicated.

Cancel (Back) returned to `MainActivity`, `CaptureActivity` records dropped to 0, and
the camera was released — `dumpsys media.camera` shows
`DISCONNECT device 0 client for package io.github.yurisismotto.anyflow`.

Before this cycle the scanner was also opened under a **system rotation lock**
(`accelerometer_rotation=0`, `user_rotation=0`) and stayed portrait, with
`requestedOrientation=SCREEN_ORIENTATION_UNSPECIFIED` — AnyFlow still does not own
the user's orientation.

---

## 24. Privacy / logging audit

Daemon log (31 lines), Android logcat (78 lines) and GUI log audited with ANSI
escapes stripped, across the whole physical session.

| Must not be logged | Result |
| --- | --- |
| File contents | **0** — a 40-byte fragment of the transferred file appears nowhere |
| Clipboard contents | **0** |
| Notification contents | **0** |
| Full QR payload (`anyflow1:…`) | **0** |
| Pairing token | **0** — no base32 token-alphabet string of token length anywhere |
| Pairing proof | **0** |
| Stream challenge | **0** occurrences of `challenge` at any level |
| HMAC | **0** |
| Private key | **0** |
| Full 64-hex fingerprints | **0** — every peer is the short form `573C CB84 DA6C 993B` |
| Full 32-hex transfer ids | **0** — every one is the 8-hex display form: `6e980eac`, `203112a6`, `fc1736e6` |

The word `token` appears twice, both inside the *prose* of the daemon's own message
*"it must prove the new token and be confirmed by hand"*. No value.

Two 32-hex strings appear. Both are **device ids** (`6532889e…` the tablet,
`795fec…` the desktop) — non-secret identifiers the daemon has always logged at
startup and session establishment. Not transfer ids, not key material.

The GUI log is **0 bytes**.

Filenames: the sanitized display name appears in `files.v1` log lines, which is the
pre-existing policy, unchanged by this sprint.

Nothing added by this sprint persists anything new. Retry state is a 32-character
hex string; it holds no file bytes, no URI and no peer. `pairingInFlight` is an
in-memory fingerprint hex, cleared in a `finally`. No QR contents and no pairing
token are written to disk. `PairingGate.REASON` is a fixed sentence and carries no
fingerprint. No telemetry, no cloud.

One pre-existing behaviour, unchanged and outside scope, recorded again for honesty:
`anyflow pair` prints the payload — including the single-use token — to the
operator's own terminal as a documented fallback for a phone that cannot scan. For
these tests that terminal output was redirected to a scratch file so the QR could be
rendered from it.

---

## 25. Android quality

Run sequentially, throttled, never alongside Cargo. No emulator, no VM, no
`connectedAndroidTest`.

```
./gradlew --no-daemon --max-workers=2 :app:assembleDebug :fixture:assembleDebug   BUILD SUCCESSFUL
./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest               BUILD SUCCESSFUL
./gradlew --no-daemon --max-workers=2 :app:testDebugUnitTest                      BUILD SUCCESSFUL
```

```
45 suites, 659 tests, 0 failures, 0 errors, 0 skipped
```

New and changed suites:

```
  SendRetryTest                     21 tests   (new)
  PairingRecoveryTest               30 tests   (new)
  ScannerInsetsTest                 14 tests   (new)
  PairingScannerOrientationTest     13 tests   (11 → 13)
  UiMappingTest                     31 tests   (3 assertions updated for the new model)
```

`assembleDebugAndroidTest` caught a real compile break: `HostDrivenCertificationHarness`
uses `app.pair()`, whose return type changed to `Result<PairOutcome>`. Fixed, and the
harness now also reports `pair proved-token=` — exactly the gap that task exists to
catch, and the reason the brief insists on running it without a device.

Android lint remains out of scope and was neither run as a gate, fixed nor
suppressed.

---

## 26. Desktop quality

**Not applicable.** No desktop file was changed:

```
$ git status --short -- desktop/ | wc -l
0
```

UX-DEBT-02 was entirely an Android lifecycle/state-routing defect, exactly as the
brief anticipated. `anyflow_core::session`, `PairingSession`, `DaemonState` and the
GUI approval path were read closely and left alone. Per §26 no full desktop build
was run for ceremony; GitHub CI remains the clean-run authority.

The prebuilt daemon, CLI and GUI from the previous sprint were used unchanged for
every physical test.

---

## 27. Files changed

### Modified (12 production + 2 test)

```
 +37   -0    android/app/src/main/AndroidManifest.xml
 +97  -17    android/app/src/main/java/.../AnyFlowApp.kt
  +8   -3    android/app/src/main/java/.../files/FileTransferManager.kt
 +19   -0    android/app/src/main/java/.../files/StreamAuth.kt
 +31   -1    android/app/src/main/java/.../net/PeerConnection.kt
 +12   -0    android/app/src/main/java/.../service/ConnectionService.kt
+119   -0    android/app/src/main/java/.../store/TrustStore.kt
 +12   -2    android/app/src/main/java/.../ui/MainActivity.kt
 +14   -0    android/app/src/main/java/.../ui/PairingScanner.kt
 +51  -20    android/app/src/main/java/.../ui/SendActivity.kt
+169   -8    android/app/src/main/java/.../ui/UiMapping.kt
  +9   -3    android/app/src/androidTest/java/.../HostDrivenCertificationHarness.kt
 +51   -2    android/app/src/test/java/.../PairingScannerOrientationTest.kt
  +8   -3    android/app/src/test/java/.../UiMappingTest.kt
```

### Added (7)

```
android/app/src/main/java/.../capability/SensitiveCapabilities.kt
android/app/src/main/java/.../pairing/PairingGate.kt
android/app/src/main/java/.../ui/PairingCaptureActivity.kt
android/app/src/main/java/.../ui/ScannerInsets.kt
android/app/src/test/java/.../PairingRecoveryTest.kt
android/app/src/test/java/.../ScannerInsetsTest.kt
android/app/src/test/java/.../SendRetryTest.kt
UX-DEBT-CLEANUP-RETRY-REPAIR-SCANNER-INSETS.md   (this report)
```

Nothing outside `android/` except this report. No workflow, protobuf, packaging or
desktop file was touched.

---

## 28. Remaining debts

Newly discovered during this sprint, recorded rather than fixed:

1. **UX-DEBT-05 — a revoked peer's card is a dead end.** After a desktop-side
   revoke the card reads *"Failed — this device's pairing was revoked"* with a
   Connect button that can never succeed, and nothing points at the recovery
   (scan a fresh code). The recovery now *works* without "Forget this device", but
   the screen does not say so. Deliberately out of scope here, which was about the
   mechanism, not new affordances.
2. **UX-DEBT-06 — the desktop accumulates stale peer records.** `anyflow status`
   lists 15 Android peers, most of them revoked identities from earlier app
   reinstalls. Cosmetic, pre-existing, and untouched.
3. **UX-DEBT-07 — `PairingGate` is not yet physically exercised.** The race it
   closes was reproduced *before* the fix (§10); the fix is covered by five JVM
   tests, but the physical re-pair happened with the coordinator already stopped, so
   the gate never had to fire. Exercising it needs a re-pair staged while the
   coordinator is still live.
4. **Two hex conversions in `FileTransferManager`** (the incoming-offer id at :316
   and the data-stream id at :481) still inline `joinToString { "%02x" }` rather
   than `StreamAuth.toHex`. Cosmetic; not touched to keep the diff to the defect.
5. Carried forward, unchanged: **UX-DEBT-04** (orientation verified on a tablet
   only), Android lint, and `anyflow pair` printing the payload to the operator's
   terminal (§24).

### Scope check

Verified absent from the diff: branding, logo, palette redesign, Quick Panel, KDE
tray/StatusNotifier, RPM/DEB packaging, Android lint cleanup or suppression, battery
hotplug, notification queue recovery, concurrent multi-peer sessions, trust-store
ordering cosmetics, protocol/protobuf change, workflow change.

---

## 29. Physical state changed

Documented exactly, per §29:

* **Fedora ⇄ tablet pairing** was revoked on the desktop and re-created from a fresh
  QR. The tablet's Fedora record differs from its starting state in `pairedAtUnix`
  and nothing else.
* **`files.v1` was granted** to the tablet on the *desktop* side (`anyflow grant`),
  because the re-pair negotiated only `battery.v1` under the desktop's existing
  `auto_grant` policy. This restores what the desktop had before the revoke.
* **Three files** were written to `~/Downloads/AnyFlow/ux-debt-01-retry.txt` (one
  accepted transfer) and one fixture pushed to the tablet's
  `/sdcard/Download/ux-debt-01-retry.txt`.
* **Tablet rotation settings** were driven during the scanner tests and left at
  `accelerometer_rotation=1` (auto-rotate on).
* The app was **updated in place** (`adb install -r`); the trust store was verified
  byte-identical across the install.
* The three preserved U2 VM peers were **not** modified. No VM was booted. No
  emulator was started. No `connectedAndroidTest` was run.

---

## 30. Git status

```
$ git status --short
 M android/app/src/androidTest/java/io/github/yurisismotto/anyflow/HostDrivenCertificationHarness.kt
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/io/github/yurisismotto/anyflow/AnyFlowApp.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/files/FileTransferManager.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/files/StreamAuth.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/net/PeerConnection.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/service/ConnectionService.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/store/TrustStore.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PairingScanner.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/SendActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/UiMapping.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/PairingScannerOrientationTest.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/UiMappingTest.kt
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? UX-DEBT-CLEANUP-RETRY-REPAIR-SCANNER-INSETS.md
?? android/app/src/main/java/io/github/yurisismotto/anyflow/capability/SensitiveCapabilities.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/pairing/PairingGate.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PairingCaptureActivity.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/ScannerInsets.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/PairingRecoveryTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/ScannerInsetsTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/SendRetryTest.kt

$ git diff --check
(clean)

$ git diff --stat
 14 files changed, 637 insertions(+), 59 deletions(-)
```

Nothing staged, nothing committed, nothing pushed, no PR.
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` untouched and unstaged.

---

## 31. Verdict

```
UX DEBT CLEANUP: PASS
FILE RETRY AFTER DECLINE: PASS
SAFE RE-PAIR AFTER DESKTOP REVOKE: PASS
PAIRING REVOCATION SECURITY: PASS
SCANNER SYSTEM-BAR INSETS: PASS
QR ORIENTATION REGRESSION: PASS
```

Against the fail conditions in §31:

**UX-DEBT-01** — the same file was sent again with no rename and no restart
(§8); the retry produced a new TransferId (`6e980eac` → `203112a6`); the retry is a
button press and nothing in the diff retries automatically; the declined attempts
are still `cancelled … declined by the user` in the daemon's ledger.

**UX-DEBT-02** — old local trust never bypasses the proof: the desktop demanded the
new token, verified it and asked a human, and only then was trust restored. Nothing
restores trust without that. Expiry was observed to close two windows on time, and
the phone holds no expiry state to bypass. A remote trust claim cannot override a
local revoke — no code path reads the local trust store to decide whether to trust.

**UX-DEBT-03** — the prompt clears the navigation bar in portrait and landscape and
through four rotations; no immersive mode and no bar hiding anywhere in the app; the
new activity is `unspecified` and the live activity reports
`SCREEN_ORIENTATION_UNSPECIFIED`; rotation kept exactly one `CaptureActivity` with
an unchanging `ActivityRecord` identity.

Nothing is waived.
