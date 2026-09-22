# Android Clipboard Truthfulness v1

GitHub #7 · remaining GitHub #8 · QP-DEBT-06

| | |
|---|---|
| Branch | `fix/android-clipboard-truthfulness-v1` |
| Base | `1c6e09f` — merge of PR #34 (`develop`) |
| Hardware | SM-X620, Android 16 / API 36, One UI 8.0.5 · Fedora (`DF65 D3E4 BA28 EDF9`) |
| Protocol change | **none** — see §24 |
| Verdict | **PASS** |

---

## 1. Baseline

```
$ git branch --show-current
fix/android-clipboard-truthfulness-v1
$ git log -1 --oneline
1c6e09f Merge pull request #34 from yurisismotto/feature/revoked-device-cleanup-v1
$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md        # untouched historical evidence
$ git diff --check
(clean)
```

PR #34's merge commit is `HEAD` itself: this branch was cut from `develop` at the
merge and had no commits of its own at the start of the sprint.

Both issues were read first (`gh issue view 7`, `gh issue view 8` including its
scope-update comment), together with `CLIPBOARD-V1-CERTIFICATION.md`,
`QUICK-PANEL-BRANDING-V1.md`, `U2-HARDENING-P3-NOTIFICATION-ROLE-CONVERGENCE.md`,
`U2-HARDENING-P1-MULTIPEER.md` and
`UX-DEBT-CLEANUP-RETRY-REPAIR-SCANNER-INSETS.md`. Everything in §2–§8 below is
what the **current tree** does, read from the source, not what the briefs claim.

---

## 2. GitHub #7 — the original defect

> The Send Clipboard Quick Settings Tile launches MainActivity correctly, but
> clipboard reading occurs during intent delivery before the Activity regains
> window focus. On Android 16, `hasPrimaryClip()` returns false and the UI
> incorrectly reports that the clipboard is empty.

Confirmed still true on the tree as committed.

---

## 3. #7 — the pre-change path, traced

```
ClipboardTileService.onClick
  └─ startActivityAndCollapse(PendingIntent → MainActivity, ACTION_SEND_CLIPBOARD)
       └─ MainActivity.onCreate(savedInstanceState)
            ├─ notificationPermission.launch(...)
            ├─ handleIntent(intent)                    ← HERE
            │    └─ ACTION_SEND_CLIPBOARD -> sendClipboardFromShortcut()
            │         └─ sendClipboard(peer)
            │              └─ lifecycleScope.launch { app.clipboard.sendCurrentClipboard(...) }
            │                   └─ SystemClipboard.read()
            │                        └─ ClipboardManager.hasPrimaryClip() / getPrimaryClip()
            └─ setContent { ... }                      ← the window does not exist yet
```

The read was reached from `onCreate`, **before `setContent`**, and therefore
before `onStart`, `onResume` and `onWindowFocusChanged`. `lifecycleScope.launch`
defers it by one main-loop turn, which is still long before the window is
attached, let alone focused. Android has refused `getPrimaryClip` to an
unfocused app since API 29.

`MainActivity.onWindowFocusChanged` **did not exist**.

### The wider audit the brief asked for

| Question | Pre-change answer |
|---|---|
| Intent arrives while already focused | `onNewIntent` → `handleIntent` → sends. Worked, by luck of being the one focused case. |
| Two tile intents before focus | Two `handleIntent` calls, two sends attempted. Nothing coalesced them. |
| Activity loses focus again | Irrelevant — nothing waited for focus. |
| Configuration change can replay it | **Yes.** `onNewIntent` calls `setIntent(intent)`, so `getIntent()` keeps returning the tile's intent; a rotation recreates the Activity, `onCreate` runs `handleIntent` again, and the clipboard is sent a **second time**, silently. This is a real defect the issue does not mention. |
| Process recreation can duplicate it | Same mechanism; saved state held nothing about the request. |
| Shared with the normal in-app send | Only downstream: `sendClipboardFromShortcut` → `sendClipboard`, and `sendClipboard` is also the button's path. The **entry** was not shared. |
| Sharesheet involved | No. `SendActivity` is a separate Activity with its own path. Untouched by #7. |

### One thing that was already right

`SystemClipboard.read()` already distinguishes *empty* from *refused*: it asks
`hasPrimaryClip()` to disambiguate a null `primaryClip`, and returns
`ReadFailure.Empty` or `ReadFailure.NotAllowed` accordingly — with a comment
naming the SM-X620 on Android 16. So §5's "do not collapse the two" was not
violated at the read layer. The defect was that the read happened at a moment
when only one of the two answers was ever possible.

---

## 4. The focus-state model

New: `ui/ClipboardShortcut.kt`. A pure Kotlin class, no Android types, so the
rule is a JVM test rather than an instrumented run that would uninstall the app
and destroy the tablet's pairing.

```
                onRequest(id, focused=true)
   NoRequest ─────────────────────────────────► Consumed(id) ──► SEND
       │                                            ▲
       │ onRequest(id, focused=false)               │ onWindowFocusChanged(true)
       ▼                                            │
   Pending(id) ─────────────────────────────────────┘
       │  ▲
       │  └─ onRequest(id'): id' replaces id  (coalesce — same clipboard)
       └──── onWindowFocusChanged(false): stays Pending (not a cancellation)

   onRequest(id) where id == consumedId  ──►  NOTHING   (replay)
   onRequest(null)                       ──►  NOTHING   (unidentified)
```

Three properties carry the weight:

* **No timer anywhere.** No `delay`, no `postDelayed`, no `yield`. The only
  thing that drains a request is `onWindowFocusChanged(true)`, which is the
  platform's own answer to the question `getPrimaryClip` actually asks.
  `onResume` is deliberately *not* used: an Activity behind the Quick Settings
  shade or behind a permission dialog is resumed and unfocused, and both are on
  the path a tile press takes.
* **`drain()` marks the id consumed before returning `SEND`**, so anything
  re-entering while the caller's coroutine reads the clipboard finds nothing.
* **A per-press idempotency token.** `ClipboardTileService` mints 16 random
  bytes per press into `MainActivity.EXTRA_REQUEST_ID`;
  `PendingIntent.FLAG_UPDATE_CURRENT` makes the reused `PendingIntent` carry the
  new one. `MainActivity` carries the last consumed id through
  `onSaveInstanceState`, so a replayed launch intent after a rotation is inert
  while a genuine second press is not. It is an idempotency token and nothing
  else: it authorizes nothing, and the send it leads to still asks the trust
  store for the grant and the per-peer policy.

**Quick Settings grants no authority.** The tile still resolves its destination
from grants and the per-peer policy, still refuses when the choice is ambiguous,
still passes through `UiMapping.clipboardSendGate`, and a sensitive clip still
stops at the confirmation dialog.

---

## 5. #7 — implementation

| File | Change |
|---|---|
| `ui/ClipboardShortcut.kt` | **new** — the state machine above |
| `ui/ClipboardTileService.kt` | mints `EXTRA_REQUEST_ID` per press |
| `ui/MainActivity.kt` | `handleIntent` **records** instead of executing; new `onWindowFocusChanged` drains; `onSaveInstanceState`/`onCreate` carry the consumed id; `sendClipboardFromShortcut` now runs the shared gate |

Failure semantics after focus (§5) are unchanged and all still reachable: empty
clipboard → `ReadFailure.Empty`'s honest message; API unavailable →
`ReadFailure.NotAllowed`; no eligible target → the existing target UX; several
peers → "Choose which computer…"; disconnected → the gate's own wording. What
changed is that "*inaccessible because the Activity lacked focus*" is now
**structurally unreachable** on this path rather than being one more message.

---

## 6. GitHub #8 — the original defect

From the issue's scope-update comment, the Android half:

* send UI does not gate on the live negotiated capability set;
* `ClipboardSync.sendText` reports success as soon as the frame is written;
* `MainActivity` can therefore show "Sent N bytes" even when the peer rejected it;
* `ERROR_CODE_UNSUPPORTED_CAPABILITY` is only logged and is not correlated;
* changing the Android clipboard grant mid-session does not converge the session.

All five confirmed on the tree as committed.

---

## 7. The pre-change send/result contract

Answering the brief's A–J directly.

**A/B. What did "sendText succeeded" mean?** Exactly *frame written to the
session's outbound channel*:

```kotlin
return try {
    send(update.toByteString())
    Log.i(TAG, "clipboard update sent to … bytes=${text.byteLength} …")
    Result.success(text.byteLength)          // ← this
} catch (e: Exception) { … }
```

Not queued-and-flushed, not received, not accepted. `MainActivity` then did
`.onSuccess { bytes -> showError("Sent $bytes bytes to $name.") }`.

**C. What peer verdicts already exist?** Two, and both are already on the wire:

1. `ClipboardResult { bytes event_id = 1; ClipboardOutcome outcome = 2; }` —
   nine outcomes, sent by the receiver "for every well-formed update it can
   attribute to an `event_id`".
2. `Envelope.Error { ErrorCode code; string message; bool fatal; }` — for the
   case where the receiver will not speak `clipboard.v1` at all.

**D/E. Correlation?** Yes, **two real protocol identities, neither invented**:

* `ClipboardResult.event_id` echoes `ClipboardUpdate.event_id` — 16 random
  bytes minted per clipboard event. The proto comments already explain why it
  is deliberately *not* the envelope's `message_id`.
* `Envelope.correlation_id` — "Set on a reply to the `message_id` of the request
  being answered." The desktop already sets it when refusing a capability:
  `WriteRequest::reply_to(Error{UnsupportedCapability, …}, &env.message_id)`
  in `core/src/session.rs`.

**F/G. Enough information?** Yes, for both. `ClipboardOutcome` already
distinguishes applied / pending-user / duplicate / not-authorized /
rejected-by-policy / rejected-as-sensitive / too-large / invalid-text / failed.
`ErrorCode` already distinguishes unsupported-capability / not-authorized /
rate-limited / the rest.

**H. Did Android discard a result it could use?** **Yes, both of them.**

```kotlin
ClipboardControl.BodyCase.RESULT -> {
    val outcome = …
    _lastOutcome.value = _lastOutcome.value + (peer.toHex() to outcome)  // event_id dropped
    …
}
…
Envelope.BodyCase.ERROR -> {
    Log.w(TAG, "peer error code=… fatal=…")                            // correlation_id dropped
    if (envelope.error.fatal) { … }
}
```

The result was keyed by *peer*, so it could not say **which** send it was about;
the error was keyed by nothing at all. And `CapabilityContext.send` returned
`Unit`, so a capability never learned the `message_id` its frame went out under —
the id an error would name.

**I. Did desktop discard a result it could use?** No. The desktop already keeps
`last_results` per peer and surfaces it in the Quick Panel's clipboard row.

**J. Grant becomes granted mid-session?** Desktop side: handled, by
`runtime/src/renegotiate.rs`. Android side: see §10.

---

## 8. Protocol capability audit — and the asymmetry it found

The audit turned up one thing worth stating plainly, because it changes what a
"negotiated capability gate" on Android can and cannot prove.

`core/src/session.rs`, the `PeerStatus::Trusted` arm, computes **two** sets:

```rust
let mutual = registry.negotiate(&hello.capabilities);
let effective: Vec<String> = mutual.into_iter()
    .filter(|c| granted_capabilities.iter().any(|g| g == c))
    .collect();

let ack = … v1::HelloAck {
    capabilities: registry.advertised(),   // ← the SUPPORTED set, not `effective`
    …
};
… Established { negotiated_capabilities: effective, … }
```

The desktop keeps `effective` for itself and tells the phone
`registry.advertised()`. Android then computes
`registry.negotiate(ack.capabilitiesList)` — supported ∩ supported — so

> **On Android, `negotiatedCapabilities` is effectively a constant.** It is the
> intersection of two fixed supported sets and is *not* the desktop's
> grant-filtered effective set.

Consequences, stated honestly:

* Gating the Android UI on the negotiated set is **correct and worth having** —
  it is false when there is no live session for that peer, and it would be false
  against an older desktop that does not implement `clipboard.v1` — but it
  **cannot** detect a desktop that has not granted the capability.
* Therefore the truthful answer for the desktop-grant case must come from the
  **peer verdict**, which is exactly what issue #8's remaining acceptance #2
  asks for and exactly what §11 implements.

Changing `HelloAck.capabilities` to carry `effective` would make the Android
negotiated set truthful, and it is arguably the deeper fix. It was **not** done:
it is a wire-semantics change affecting all four capabilities and the pairing
path's `grantedCapabilities = negotiatedCapabilities - NEVER_AUTO_GRANTED`
derivation, it is not needed to make the sender truthful, and this is a
micro-sprint with an explicit "do not expand into a protocol change" gate. It is
recorded as a debt in §26.

---

## 9. Negotiated-capability exposure

`PeerConnection.negotiatedCapabilities` already existed and was unreachable from
the UI. Exposed as **session state, never persisted**:

```kotlin
// AnyFlowApp
data class LiveSession(val peerHex: String, val negotiated: Set<String>)
val liveSession: StateFlow<LiveSession?>
```

* Written by `ConnectionService` in the `DialResult.Established` block, from
  `connection.negotiatedCapabilities`, beside the existing
  `currentSessionPeerHex`.
* Cleared in that block's `finally`, and in `onDestroy`.
* Carries **`peerHex`**, so a capability negotiated with one computer cannot
  authorize a button aimed at another (C5). A global boolean is the same class of
  defect as U2 §39.17 — a global fact standing in for a per-identity one.
* Nothing is written to the trust store. A process restart begins at `null`.

### The gate

`UiMapping.clipboardSendGate(peer, session)` answers `Ready` or
`Blocked(reason)`. The order is the order a person can act on:

| # | Condition | Wording |
|---|---|---|
| 1 | `clipboard.v1` granted to this peer | "Turn on clipboard sharing for *X* first." |
| 2 | `clipboardPolicy.allowSend` | "Sending your clipboard to *X* is turned off." |
| 3 | live session, **and it is this peer's** | "Connect to *X* to send your clipboard." |
| 4 | `clipboard.v1` in the live negotiated set | "This connection with *X* has not negotiated clipboard sharing yet." |

A side-finding worth recording: `UiMapping.canSendClipboard` **already existed
and no screen called it**. The home screen and the device card each wrote a
different subset of the conditions inline, and the send screen asked only
whether a link was up — making the screen a Quick Settings press is most likely
to land on the most permissive of the three. All four call sites (home, device
card, send screen, tile) now go through the one function, and the device and send
screens print `reasonOrNull` under the disabled button.

---

## 10. Grant convergence — and why Android needs no reconnect

The brief asked for "the smallest safe way for an Android-side clipboard grant
change to converge". Audited, the answer is **nothing, because it already
converges** — and inventing a reconnect would be a regression, not a fix.

* **Android's advertisement is grant-independent.** `HELLO` carries
  `registry.advertised()`, which is the four capability ids this build
  implements. An Android-side grant is therefore never an input to negotiation,
  so no Android grant change can make a negotiated set stale.
* **Widening bites immediately.** Every clipboard decision re-asks
  `ClipboardSync.Authorizer.policyFor(peer)`, which reads the trust store fresh.
  Turning the grant on makes the very next send permitted, on the session that
  is already up.
* **Narrowing bites immediately, and harder.** Same mechanism for the send path;
  additionally `ClipboardTileService.updateTile()` re-reads the store and sets
  `STATE_UNAVAILABLE`, so the tile stops being pressable at all.

Both directions are pinned by test **C16/C17** and both were exercised on
hardware (§18, §17).

The side whose advertisement *is* grant-filtered is the desktop, and it already
has `runtime/src/renegotiate.rs`: a grant that widens past what the live session
negotiated ends that session, once, and the phone's own `ConnectionCoordinator`
redials. That mechanism was observed working on hardware in §18 — including its
own log line. Nothing about it was changed, and **no clipboard-specific shadow
negotiation was added**: the Android side never widens a negotiated set locally
(test C18), and `HELLO` remains the only thing that decides one.

---

## 11. The sender verdict model

New: `clipboard/ClipboardDelivery.kt`. Five states, kept permanently apart:

| State | Meaning | Success? |
|---|---|---|
| `Enqueued(bytes)` | the frame is on the session — **local knowledge only** | no |
| `Confirmed(outcome, bytes)` | peer verdict: applied / pending-user / duplicate | **yes** |
| `Rejected(outcome, bytes)` | peer verdict: not-authorized / policy / sensitive / too-large / invalid / failed | no |
| `Refused(reason, bytes)` | peer refused the **capability**; the clip was never examined | no |
| `Unconfirmed(reason, bytes)` | disconnected, or timed out — no verdict, and there will not be one | no |

`succeeded` is a whitelist of one (`Confirmed`), so anything added later is not
a success until somebody says so here. `describe(peerName)` is the single
failure vocabulary the brief's §14 asked for — every state's wording lives in
one `when` and nowhere else. `ClipboardDelivery` has **no `String` field a clip
could reach**, so "the outcome model never holds clipboard text" is a property of
the type.

### How a verdict is correlated

```
ClipboardSync.sendText
  ├─ mints eventId (16 random bytes, ClipboardLimits.EVENT_ID_LENGTH)
  ├─ registers outstanding[eventIdHex] = { peerHex, bytes, epoch, CompletableDeferred }
  │      ← registered BEFORE the write: a LAN peer can answer while send() returns
  ├─ messageId = send(frame)                    ← PeerConnection now returns it
  ├─ byMessageId[messageIdHex] = eventIdHex
  └─ returns SendReceipt(eventIdHex, bytes, deferred)

peer answers in clipboard.v1  → handleControl(RESULT) → resolveByEventId(peer, result.event_id, outcome)
peer refuses the capability   → Envelope.ERROR       → onPeerRefusal(peer, correlation_id, code)
session ends                  → detachSession        → settle all as Unconfirmed(DISCONNECTED)
```

Three things must agree before anything is settled: the id is one this device
minted, the peer answering is the peer it was sent to, and the session epoch
matches. Any of the three failing discards the result rather than applying it.
**Nothing is ever matched on peer name, timestamp, payload, byte count or list
position.**

Concurrency: the protocol does not serialise clipboard operations, so state is
keyed by `event_id` rather than by a single "waiting" flag. Several sends may be
outstanding and each is resolved by its own result, out of order. The map is
bounded at 32 and eviction **settles** as `Unconfirmed(TIMED_OUT)` rather than
dropping, so a caller waiting on an evicted send is still answered.

### The transport seam

Capability-agnostic, no wire change:

* `CapabilityContext.send(id, payload)` now returns the `message_id` the
  envelope went out under. `PeerConnection` builds the envelope first so it can
  answer with it.
* New `Capability.onPeerError(context, correlationId, code)`, default no-op.
  `PeerConnection`'s `ERROR` arm fans it out to every negotiated capability when
  `correlation_id` is non-empty. The fan-out is safe because a `message_id` is 16
  bytes of randomness minted per frame: a capability that did not mint this one
  matches nothing. **Observed working on hardware** — `notifications.v1`'s role
  announcement is refused on every session with this peer, and not one of those
  refusals was attributed to a clipboard send (§21).
* `ClipboardCapability.onPeerError` routes to `ClipboardSync.onPeerRefusal`. The
  decision about what a code *means* stays in `ClipboardSync`, as every other
  clipboard decision does.

### What the person sees

`MainActivity.sendClipboard` and `SendActivity.startTextSend` now
`receipt.awaitVerdict(ClipboardLimits.VERDICT_TIMEOUT_MS)` — a suspending wait in
the Activity's own scope, no thread blocked — and show
`delivery.describe(peerName)`. On a LAN the verdict arrives in **14–30 ms**
(measured, §16/§17), so in practice the person sees the real outcome; when
nothing comes back within 5 s the message says *"delivery not confirmed"* rather
than inventing an answer. `ClipboardSync.lastDelivery` is the state authority the
UI draws from; the toast is the announcement, never the authority.

---

## 12. QP-DEBT-06 — desktop Quick Panel

**Before:** `Ok(_) => panel.toast(&format!("Clipboard sent to {peer}"))`. The
daemon answers `ClipboardSend` as soon as the frame is on the session, so the
message was optimistic by exactly one round trip — the certification report
already recorded the hardware case where the panel said "sent" and the tablet had
refused.

Option **B** from the brief, because option A would mean blocking on a network
round trip behind a GTK button. Three pure functions in `panel/model.rs`, so the
wording is testable without a display:

* `clipboard_submitted_message(peer)` → `"Clipboard submitted to <peer>; awaiting confirmation"`
* `clipboard_send_error_message(msg)` — the daemon refusing the press outright
  **is** final, so that one stays final
* `clipboard_outcome_note(outcome, peer)` — the closed vocabulary for the status row

The status row is the authority and now resolves what the toast defers:
previously it printed the daemon's inter-process strings verbatim
(`"Last clip sent: not authorized by Tablet."`) and said **nothing at all** when a
clip landed. It now states both, in English:

| daemon outcome | row says |
|---|---|
| `applied` | "The last clip reached *X*." |
| `pending` | "The last clip reached *X* and is waiting to be applied there." |
| `duplicate` | "*X* already had the last clip." |
| `not authorized` | "*X* is not set up to accept this computer's clipboard." |
| `rejected by policy` | "*X* is not accepting clipboard text from this computer." |
| `rejected as sensitive` | "*X* refuses clipboard text marked sensitive." |
| `too large` | "The last clip was too large for *X*." |
| `invalid text` | "*X* could not read the last clip as text." |
| anything else | "*X* could not use the last clip." |

No new outcome model, no new routing authority, no redesign: the panel's action
gating, target resolution and request composition are byte-for-byte unchanged.

---

## 13. Android tests

`:app:testDebugUnitTest` — **721 tests, 0 failures** (48 classes).

**`ClipboardShortcutTest` — 13 tests, GitHub #7 (brief §16).** Pure JUnit, no
Robolectric, no Android framework.

| | |
|---|---|
| F1 | a request before focus does not execute, and is remembered |
| F2 | focus drains it, exactly once |
| F3 | five further `focus=true` callbacks do nothing |
| F4 | losing focus neither executes nor cancels; the later focus still drains |
| F5 | a request while already focused executes immediately |
| F6 | a second genuine press executes again — both via focus and while focused |
| F7 | an ordinary launch executes nothing |
| F8 | a request with no id, and with an empty id, is refused and arms nothing |
| F9 | three focus false/true cycles with nothing pending are inert |
| F10 | the id is spent before `SEND` is returned; re-entry produces 0 further actions |
| F11 | three rapid presses before focus coalesce to one send; all three ids are spent |
| F12 | recreation with the saved consumed id does not replay; a new press still works |
| + | `restore(null)` (process death) leaves a fresh machine — the honest worst case |

**`ClipboardTruthfulnessTest` — 20 tests, GitHub #8 (brief §17).**

| | |
|---|---|
| C1 | grant + policy + live session, capability **not** negotiated → blocked, and the reason names the negotiation, not the connection |
| C2 | negotiated → allowed |
| C3 | grant revoked mid-session → blocked at once, in the UI **and** in the send path, on the same live session |
| C4 | policy denied → blocked whatever the session negotiated |
| C5 | a session with peer B does not authorize a send to peer A |
| C6 | no session → blocked |
| C7 | `ERROR_CODE_UNSUPPORTED_CAPABILITY` correlated → `Refused(NOT_NEGOTIATED)`, wording says nothing was sent |
| C8 | all six refusing outcomes → `Rejected`, none reads as success |
| C9 | `APPLIED` and `PENDING_USER` → `Confirmed`; nothing left outstanding |
| C10 | a local frame write is `Enqueued`: not `succeeded`, not `settled`, and the wording does not claim delivery. Still not a success after the wait gives up |
| C11 | disconnect settles a pending send as `Unconfirmed(DISCONNECTED)` and publishes it |
| C12 | a verdict from a previous session resolves nothing; the current session's own still lands |
| C13 | B claiming A's `event_id` moves nothing; B refusing A's `message_id` moves nothing; each peer's own answer resolves only its own |
| C14 | a canary through every state, every outcome, every refusal and every unconfirmed reason — no clipboard text in the model or its wording |
| C15 | a sensitive clip still needs confirmation, nothing leaves, nothing is recorded, and a sensitive refusal is not a success |
| C16/C17 | grant widening and narrowing both converge on the live session with no handshake |
| C18 | no grant combination puts a capability on a session that did not negotiate it |
| + | concurrent sends each resolved by their own result, out of order |
| + | the outstanding map stays bounded and eviction settles honestly |

Also updated: `ClipboardSyncTest` (30) and `UiMappingTest` (31) for the new
signatures. One assertion changed meaning and is worth naming: a
`ClipboardResult` for an `event_id` this device never minted used to record
`APPLIED` against the peer; it now records **nothing**, which is the point.

---

## 14. Desktop tests

`cargo test --workspace -j 2` — **908 passed, 0 failed, 23 ignored.**
`cargo test -p anyflow-gui -- --ignored --test-threads=1` — **1 passed** (the
display-gated widget-tree test).

**`gui/src/panel/model/tests.rs` — 10 new tests (brief §18).** Pure model, no
GTK, no socket.

| | |
|---|---|
| D1 | the immediate message contains no "Clipboard sent", "arrived", "received", "delivered" or "copied" |
| D2 | it says "awaiting confirmation" and names the destination |
| D3 | `applied`/`pending`/`duplicate` read as success; the note says "reached" |
| D4 | `not authorized` is an explicit rejection and never says "reached" |
| D5 | every refusing outcome **including an unknown one** is refused and still says something |
| D6 | no verdict → no claim about a last clip |
| D7 | a verdict on device B does not reach device A's row, and does reach B's |
| D8 | no daemon protocol string is echoed into the row; the submitted message is exact |
| D9 | the row's existing direction sentences, mobile caveat and `StatusValue` are unchanged |
| D10 | the wording functions decide no destination; a past refusal does not change the request |

Plus the pre-existing `a_refusal_by_the_receiving_device_is_reported`, updated to
the new vocabulary.

---

## 15. androidTest compile gate

Run **before** any physical testing, as the brief requires, and not skipped on
the grounds that `connectedDebugAndroidTest` would not run:

```
$ ./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest
BUILD SUCCESSFUL in 23s
```

`connectedDebugAndroidTest` was **not** run: the harness uninstalls and
reinstalls, which destroys the tablet's pairing and every piece of evidence
below. The debug APK was updated with `adb install -r`; `firstInstallTime`
stayed `2026-09-15 14:33:28`, so app data and the Keystore identity survived.

---

## 16. Physical evidence — Quick Settings tile (#7)

SM-X620 · Android 16 / API 36 · One UI 8.0.5. The tile was added to the Quick
Settings panel with `cmd statusbar add-tile` (placement only — every press below
went through SystemUI's real `onClick`).

The clipboard was populated over AnyFlow's own **receive** path, which is a
separate, already-certified code path from the one under test, and which lets a
canary of known length and hash be placed on the tablet without printing it:
desktop `wl-copy` → `anyflow clipboard send` → held pending on the tablet → the
person taps **Copy**.

| Gate | Result |
|---|---|
| Q1 | canary 1 on the tablet clipboard: **36 bytes, sha256 `07bbcdac6c2b936e`** |
| Q2 | AnyFlow not focused — `mCurrentFocus=…launcher.LauncherActivity` |
| Q3 | tile invoked through SystemUI (`TileLifecycleManager … ClipboardTileService` at 19:04:16.684) |
| Q4 | `mCurrentFocus=…anyflow.ui.MainActivity` after the press |
| Q5 | **no** `ClipboardService: Denying clipboard access to io.github.yurisismotto.anyflow` anywhere in 96 447 lines of logcat. The only denials in the capture are for `com.google.android.as`, unrelated. Nothing in this app attempted an unfocused read. |
| Q6 | exactly **one** `clipboard update sent … bytes=36` for one press |
| Q7 | Fedora received **36 bytes, sha256 `07bbcdac6c2b936e`** — byte-for-byte the canary |
| Q8 | no "clipboard is empty" message; the toast was the peer's verdict |
| Q9 | **repeated with a real finger tap** on the tile in the expanded panel, canary 2: **47 bytes, sha256 `9e571a66…`**, one frame, received intact |
| Q10 | **BLOCKED** — see below |

Timing, run 2, straight from logcat:

```
19:06:12.354 I/ClipboardSync: clipboard update sent … event=2eae57ca bytes=47 sensitive=false
19:06:12.379 D/ClipboardSync: peer reported clipboard outcome: PENDING_USER     (+25 ms)
19:06:12.436 I/Toast: show: caller = …MainActivity.showError:569                (+57 ms)
```

The toast is emitted **after** the verdict. That ordering is the fix.

### Q10 — BLOCKED, and why

Q10 needs a genuinely empty system clipboard. On this device that requires either
a reboot — which needs the lock credential (`locksettings verify` → *"User has a
lock credential"*), which this session does not have and must not guess at — or
the Samsung keyboard's clipboard panel "Delete all", which lives in an IME window
`uiautomator` does not expose. Both AOSP and Samsung routes were probed
(`cmd`/`service call` from shell are refused because shell is not the focused
app; `com.samsung.android.app.clipboardedge` exposes only an Edge-panel
provider, no launchable activity). Nothing was faked.

What *is* covered: `SystemClipboard.read()`'s empty-vs-refused disambiguation is
pre-existing, unchanged by this sprint, and carries its own SM-X620/Android 16
comment; and the adjacent property Q10 really guards — that a tile press which
cannot send says *why*, truthfully — is proven three different ways in §17 and
§18. This is recorded as a remaining physical debt in §26.

---

## 17. Physical evidence — capability mismatch (§21)

Constructed without weakening anything: the **desktop's** `clipboard.v1` grant
for the tablet was withdrawn while the tablet's own grant, policy and trust were
left exactly as they were. Under the old UI both cases below sent and reported
*"Sent 47 bytes to Fedora."*

### Case A — peer verdict `NOT_AUTHORIZED` (grant off, session still up)

```
19:11:46.301 I/ClipboardSync: clipboard update sent … event=1a95774c bytes=47 sensitive=false
19:11:46.315 D/ClipboardSync: peer reported clipboard outcome: NOT_AUTHORIZED   (+14 ms)
```

On screen (`07-toast-not-authorized-b.png`):

> **Fedora is not set up to accept your clipboard.**

### Case B — `ERROR_CODE_UNSUPPORTED_CAPABILITY` (reconnected with the grant off)

The session was torn down and re-established so the desktop recomputed its
effective set. The daemon's own log shows the difference:

```
session established … peer=573C CB84 DA6C 993B capabilities=["battery.v1", "files.v1"]
```

`clipboard.v1` genuinely absent. The tablet's own negotiated set still contains
it — §8's asymmetry — so the gate passed and the frame went out:

```
19:13:13.589 I/ClipboardSync:   clipboard update sent … event=0437e47c bytes=47 sensitive=false
19:13:13.607 W/PeerConnection:  peer error code=ERROR_CODE_UNSUPPORTED_CAPABILITY fatal=false
19:13:13.607 I/ClipboardSync:   peer refused a clipboard frame: NOT_NEGOTIATED   (+18 ms)
```

On screen (`08-toast-unsupported-a.png`):

> **This connection has not negotiated clipboard sharing yet. Nothing was sent to Fedora.**

This is the exact case GitHub #8 names, with the exact wording the brief's §8
asked for. No peer verdict was faked at any point: every one of them came off
the wire from the real daemon.

### The isolation that came free

`W/PeerConnection: peer error code=ERROR_CODE_UNSUPPORTED_CAPABILITY` also fires
at **every** session start with this peer — that is `notifications.v1`'s role
announcement being refused, because the desktop has no notifications grant here.
The new fan-out offers each of those to `ClipboardSync` too, and not one was
attributed to a clipboard send. C13's correlation discipline, observed on
hardware rather than only in a unit test.

---

## 18. Physical evidence — grant convergence (§22)

| Step | Observed |
|---|---|
| A | session up, `granted battery.v1, files.v1` — `clipboard.v1` neither granted nor negotiated |
| B | `anyflow grant 573ccb84… clipboard.v1` → *"granted clipboard.v1 … (reconnecting the device so it takes effect now)"* |
| C/D | daemon log: `granted a capability this session cannot use; ending it so the device reconnects and negotiates again peer=573C CB84 DA6C 993B session=7 capability="clipboard.v1"`, then `session established … capabilities=["battery.v1", "clipboard.v1", "files.v1"]` — a **fresh authenticated HELLO**, not an in-memory edit |
| E | live at **t+6 s**, with **no daemon restart and no app restart** |
| F | one clipboard sent: `event=dd61d73d bytes=47` |
| G | verdict `PENDING_USER` at **+19 ms**; on screen: **"Fedora received 47 bytes and is holding them until you apply them there."** (`09-toast-converged.png`) |
| H | the **Android-side** grant switched off in the app — the session stayed `Connected`, and the device screen collapsed to *"Clipboard is off. Fedora cannot send or receive clipboard text."* |
| I | tile pressed: the tile itself was **greyed out and unavailable** (`11-toast-grant-off.png`). No Activity launched. Frames sent before **6**, after **6** — nothing left the device |

Step I is the strongest form of C17: narrowing did not merely block the send, it
withdrew the affordance, immediately, with no reconnect. Step C/D is the
existing `renegotiate_after_grant` seam doing its job — nothing about negotiation
was bypassed and no in-memory negotiated set was edited.

---

## 19. Physical evidence — desktop Quick Panel (§23)

Real `anyflow-gui --quick-panel` against the real daemon and the real tablet,
driven and read through AT-SPI.

| Gate | Observed |
|---|---|
| P1 | `Send clipboard to SM-X620` pressed (AT-SPI `click`) |
| P2/P3 | immediate feedback, sampled at 0.25 s → 2.4 s: **"Clipboard submitted to SM-X620; awaiting confirmation"** — never "Clipboard sent" |
| P4 | status row description resolves it: *"…Clips from SM-X620 are held until you apply them. SM-X620 sends only when you ask it to there.* **The last clip reached SM-X620 and is waiting to be applied there.**" |
| P5 | refusal case: "Receive clipboard" turned off on the tablet; immediate feedback again **"Clipboard submitted to SM-X620; awaiting confirmation"**; the tablet logged `outcome=REJECTED_POLICY`; daemon `last result rejected by policy` |
| P6 | status row: **"SM-X620 is not accepting clipboard text from this computer."** — an explicit refusal, in English, never presented as success |

The panel was not redesigned: the same widget, the same action gating, the same
status row.

No screenshot of the panel is attached — `import -window root` cannot capture
this Wayland-native window in this session. The AT-SPI accessible names and
descriptions above are the verbatim strings the panel exposes and are what a
screen reader would announce.

---

## 20. Multi-peer isolation

**JVM (exhaustive):** C5 (another peer's session does not authorize this one),
C12 (a previous session's verdict resolves nothing), C13 (B claiming A's
`event_id` or A's `message_id` moves nothing, in both directions).

**Hardware:** with five peers in the trust store and one live session, the Quick
Panel was retargeted from `573C CB84 DA6C 993B` — which had just recorded
*"not accepting clipboard text from this computer"* — to `8768 F2C9 2C71 9DDB`.
The row became **"Clipboard is not enabled for SM-X620."** and the action lost
its target. The verdict did not follow the selection.

Identity is fingerprint-based end to end: `LiveSession.peerHex`, the
`Outstanding.peerHex` check, `ConnectionService.EXTRA_TARGET_FINGERPRINT`, and
the panel's `selected_peer` hex. Nothing added here reads a name or a list
position. Session teardown is explicit: `detachSession` settles every outstanding
send for that peer as `Unconfirmed(DISCONNECTED)` and removes both index entries.

---

## 21. Sensitive clipboard regression

Unchanged and still fail-closed.

* `ClipboardSync.sendCurrentClipboard` still refuses a clip the platform marked
  sensitive with `SendFailure.NeedsConfirmation`, before `sendText`, so the
  content never reaches the session (C15 asserts `session.frames.isEmpty()`).
* The confirmation dialog path in `MainActivity` is untouched.
* `ClipboardPreview` still withholds the text of a sensitive clip.
* `sensitiveHint` still travels per-clip, and a peer that refuses sensitive
  clips produces `Rejected(REJECTED_SENSITIVE)` → *"Fedora refuses clipboard
  text marked sensitive."* — a refusal, never a success (C8, C15).
* The desktop's `sensitive_available` reporting is unchanged; there is no new
  fallback path, and nothing here can downgrade a sensitive send to an ordinary
  one.

---

## 22. Security and privacy

Verified unchanged:

| | |
|---|---|
| TLS 1.3 + SPKI pinning | untouched — `PeerConnection.openTls`, `PinnedTrustManager`, `TlsFactory` not modified |
| Proof-of-possession pairing | untouched — the `handshake` pairing arms are byte-for-byte unchanged |
| Fingerprint-based targeting | extended, never relaxed: `LiveSession.peerHex` and `Outstanding.peerHex` are both fingerprint hex |
| Per-peer grants | authoritative; the gate reflects them and never widens one (C18) |
| Clipboard send/receive policy | re-read per operation, unchanged |
| Sensitive fail-closed | §21 |
| Clipboard content logging | none — §23 |
| Clipboard persistence / history | none added. `ClipboardDelivery` is in-memory and has no `String` field; `LiveSession` is never persisted |
| Telemetry / cloud | none |
| Background clipboard workaround | **none.** The #7 fix is legitimate foreground Activity window focus and nothing else |

The #7 fix uses **no** AccessibilityService, **no** notification-listener trick,
**no** foreground clipboard polling, **no** hidden API, **no** reflection, **no**
root, and **no** ADB-only production behaviour. It is one lifecycle callback the
platform already provides.

One honest note on `EXTRA_REQUEST_ID`: `MainActivity` is exported for the
launcher, so another application could send it an explicit intent carrying
`ACTION_SEND_CLIPBOARD` and a request id of its own. That was true before this
sprint too, and the id is **not** claimed as a security boundary — the grant, the
per-peer policy, the fingerprint-based target resolution and the sensitive-clip
confirmation are, and all four are asked downstream regardless. What the id
changes is that an *unidentified* request is now refused rather than honoured
(F8). The Activity is also visible and the person sees where the text goes.

---

## 23. Logging audit

Android — 96 447 logcat lines captured across the whole physical run, from a
clean `logcat -c`:

| Check | Result |
|---|---|
| canary bodies (`ANYFLOW-QS7…`, `ANYFLOW-QP6…`, `CANARY`) | **0 occurrences** |
| full content hash | **0 occurrences** |
| pairing token / proof / private key / stream challenge | **0 occurrences** |

Every AnyFlow line emitted in the run was reviewed; the complete vocabulary is:
short public fingerprint, 8-hex `event_id` prefix, byte count, `sensitive`
boolean, outcome enum, refusal enum, endpoint, capability id, `ErrorCode`, and
exception *class names*. Two lines are new this sprint and both were reviewed:

```
I/ClipboardSync:  peer refused a clipboard frame: NOT_NEGOTIATED
W/PeerConnection: capability <id> failed on a peer error: <ExceptionClass>
```

The peer's `Error.message` — a free string from the other end of the wire — is
deliberately **not** logged and never shown. No production logging was widened
for this sprint.

Desktop daemon log: same result. Canary count **0**. Clipboard lines carry
`peer=<short fingerprint> event=<8 hex> bytes=<n> sensitive=<bool>` and
`outcome="<enum>"` and nothing else.

---

## 24. Protocol impact — the §26 gate, answered

**No protobuf or wire change was made, and none is needed.**

1. **What result/verdict messages already exist?** `ClipboardResult{event_id,
   outcome}` with nine outcomes, and `Envelope.Error{code,…}` with
   `Envelope.correlation_id`.
2. **Why can they not satisfy the requirement?** They can. Both were already
   sent by the desktop and both were already discarded by Android.
3. **What exact missing correlation/state existed?** None on the wire. The gap
   was **in the Android process**: `CapabilityContext.send` returned `Unit`, so a
   capability never learned the `message_id` its frame went out under, and the
   `RESULT` handler threw `event_id` away and keyed by peer. Both are local
   plumbing.
4. **Why is a local state-machine fix insufficient?** It is sufficient. That is
   exactly what was built.
5. **Compatibility impact with older peers.** None. Not one byte on the wire
   changed. An older desktop that sends no `ClipboardResult` and no correlated
   `Error` simply leaves the send at `Unconfirmed(TIMED_OUT)` — *"delivery not
   confirmed"* — which is the honest thing to say about such a peer.

The one wire-*semantics* change that was considered and **rejected** —
`HelloAck.capabilities` carrying `effective` instead of `advertised()` — is
documented in §8 and carried as a debt in §26. It was not needed to make the
sender truthful and is out of scope for a micro-sprint.

---

## 25. Files changed

**25 code and test files in total**, plus this report:

| | |
|---|---|
| tracked files modified | **21** — `git diff --stat`: 1 134 insertions(+), 98 deletions(-) |
| new files, still untracked | **4** — the two new production classes and their two test suites |
| code and test files in total | **25** |
| plus | `ANDROID-CLIPBOARD-TRUTHFULNESS-V1.md`, this report, untracked until it is versioned |

`git diff --stat` counts only the 21 tracked files: an untracked file has no
index entry for a diff to be taken against, so the four new sources and the
report are absent from that line and are listed separately below. The
`??` entries in §27 are the same five files.


**Android — new**

```
app/src/main/java/…/clipboard/ClipboardDelivery.kt      the five-state verdict vocabulary
app/src/main/java/…/ui/ClipboardShortcut.kt             the focus state machine
app/src/test/java/…/ClipboardShortcutTest.kt            F1–F12
app/src/test/java/…/ClipboardTruthfulnessTest.kt        C1–C18
```

**Android — modified**

```
…/AnyFlowApp.kt                    LiveSession + its StateFlow, never persisted
…/capability/Capability.kt         send() returns the message id; onPeerError hook
…/capability/ClipboardCapability.kt  routes onPeerError to ClipboardSync
…/clipboard/ClipboardSync.kt       receipts, event-id/message-id correlation,
                                   epochs, settle-on-detach, bounded outstanding map
…/clipboard/ClipboardText.kt       ClipboardLimits.VERDICT_TIMEOUT_MS
…/net/PeerConnection.kt            builds the envelope first; ERROR fan-out
…/service/ConnectionService.kt     publishes and clears the live session
…/ui/ClipboardTileService.kt       mints EXTRA_REQUEST_ID per press
…/ui/MainActivity.kt               focus wiring, saved state, truthful verdict
…/ui/MainState.kt                  clipboardDeliveries, liveSession, the gate helper
…/ui/UiMapping.kt                  ClipboardSendGate replaces the unused boolean
…/ui/DevicesScreen.kt              uses the gate
…/ui/PeerDetailScreen.kt           uses the gate; prints the reason
…/ui/SendClipboardScreen.kt        uses the gate; prints the reason
…/ui/SendActivity.kt               awaits the verdict instead of announcing one
…/test/…/ClipboardSyncTest.kt      new API; the uncorrelated-result assertion inverted
…/test/…/UiMappingTest.kt          new signature + the negotiation condition
…/androidTest/…/NotificationUiFixtures.kt   MainUiState fields
```

**Desktop — modified**

```
gui/src/panel/model.rs         clipboard_submitted_message / _send_error_message /
                               _outcome_succeeded / _outcome_note; status row wording
gui/src/panel/mod.rs           the toast uses them
gui/src/panel/model/tests.rs   D1–D10, and the existing refusal test updated
```

Untouched, as instructed: `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`. No protobuf, no
trust store, no pairing, no file transfer, no notifications, no branding, no
packaging.

---

## 26. Remaining debts

1. **`HelloAck.capabilities` carries the desktop's supported set, not its
   effective one** (§8). Until that changes, Android's `negotiatedCapabilities`
   cannot detect a desktop-side grant that is off, and the truthful answer for
   that case has to come from the peer verdict — which it now does. Fixing it
   properly is a wire-semantics change across four capabilities plus the
   pairing-time `grantedCapabilities` derivation, and deserves its own sprint
   with its own compatibility analysis.
2. **Q10 — the empty-clipboard tile message is not physically certified** (§16).
   Blocked on having no way to clear the SM-X620's system clipboard without the
   device lock credential. Worth one minute of a person's time at the tablet.
3. **`clipboardDeliveries` is carried in `MainUiState` and not yet drawn.** The
   toast reports the verdict and `lastDelivery` is the authority, but no screen
   renders a per-peer "last clipboard" row yet. The desktop has one; Android
   does not.
4. **`CapabilityAnnounce` is still accepted and ignored on both sides.** Not a
   defect — widening from a peer's assertion would be wrong — but it remains the
   obvious place a future in-session renegotiation would live.
5. **The AnyFlow Quick Settings tile was added to the tablet's panel** during
   certification (`cmd statusbar add-tile`) and left there. It is the feature
   under test; remove it from Quick Settings by hand if it is not wanted.

---

## 27. Git status

```
$ git branch --show-current
fix/android-clipboard-truthfulness-v1

$ git status --short
 M android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/AnyFlowApp.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/capability/Capability.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/capability/ClipboardCapability.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/clipboard/ClipboardSync.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/clipboard/ClipboardText.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/net/PeerConnection.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/service/ConnectionService.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/ClipboardTileService.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/DevicesScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainState.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PeerDetailScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/SendActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/SendClipboardScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/UiMapping.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/ClipboardSyncTest.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/UiMappingTest.kt
 M desktop/gui/src/panel/mod.rs
 M desktop/gui/src/panel/model.rs
 M desktop/gui/src/panel/model/tests.rs
?? ANDROID-CLIPBOARD-TRUTHFULNESS-V1.md
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? android/app/src/main/java/io/github/yurisismotto/anyflow/clipboard/ClipboardDelivery.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/ClipboardShortcut.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/ClipboardShortcutTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/ClipboardTruthfulnessTest.kt

$ git diff --check
(clean)
```

`ANDROID-CLIPBOARD-TRUTHFULNESS-V1.md` — this report — appears correctly as a
new, untracked file (`??`) and is unstaged, as are the four new sources. The
21 modified files appear as ` M`. That is the expected pre-stage state for a
report created during the sprint: it is new rather than modified because no
file of that name existed on this branch before.

Nothing committed, nothing pushed, no PR, no issue touched. `git add .` was never
used. `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untouched and unstaged.

---

## 28. Gate matrix

| Gate | Result |
|---|---|
| QS TILE FOCUS SAFETY | **PASS** |
| QS TILE REAL CLIPBOARD SEND | **PASS** |
| ANDROID NEGOTIATED CAPABILITY GATE | **PASS** |
| ANDROID CLIPBOARD VERDICT TRUTHFULNESS | **PASS** |
| ANDROID GRANT CONVERGENCE | **PASS** |
| DESKTOP QUICK PANEL FEEDBACK | **PASS** |
| MULTI-PEER ISOLATION | **PASS** |
| SENSITIVE CLIPBOARD REGRESSION | **PASS** |
| SECURITY / PRIVACY REGRESSION | **PASS** |
| ANDROID TEST GATES | **PASS** — 721 unit tests, `assembleDebug`, `assembleDebugAndroidTest` |
| DESKTOP TEST GATES | **PASS** — fmt, 908 tests, clippy `-D warnings`, 1 display-gated |

Sub-gate not met: **physical Q10** (empty-clipboard tile message) — **BLOCKED**,
§16, debt 2.

### Issue acceptance

**GitHub #7** — the clipboard read happens only after real Activity window focus
(§4, F1–F12, §16 Q5); the Quick Settings hardware gate passes twice with the
content verified by hash (§16 Q6–Q9); no duplicate action, including across a
configuration change, which the issue did not mention (F3, F10–F12).

**GitHub #8** — capability mismatch is visible to the Android sender, in both
forms, on hardware (§17); no sender path claims confirmed success any more —
`MainActivity` and `SendActivity` both await a verdict, and `Enqueued` is
structurally not a success (C10); grant/session negotiation converges correctly
in both directions with no handshake bypassed (§10, §18); desktop feedback
remains truthful (§12, §19). **No sender path still lies about delivery**: the
two that existed, `MainActivity.sendClipboard` and `SendActivity.startTextSend`,
were the only callers of `sendCurrentClipboard`/`sendText`, and both were
changed.

**QP-DEBT-06** — the immediate desktop feedback no longer claims confirmed
success (D1, §19 P2); the status row is authoritative and now resolves what the
toast defers (D3–D6, §19 P4/P6).

---

## 29. Final verdict

**ANDROID CLIPBOARD TRUTHFULNESS V1: PASS**

* the Quick Settings false-empty defect is fixed, and certified on the real
  SM-X620 with the content verified by length and hash;
* Android no longer claims confirmed success for a rejected or unconfirmed clip —
  demonstrated on hardware for a peer `NOT_AUTHORIZED` verdict **and** for
  `ERROR_CODE_UNSUPPORTED_CAPABILITY`, the case the issue names;
* capability mismatch has honest, specific feedback in the person's own words;
* QP-DEBT-06 is resolved;
* no security regression: no protocol change, no new permission, no background
  clipboard access, no content in any log, and nothing new written to disk.
