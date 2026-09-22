# Post-Wave 0 Micro-Sprint — B-1 and B-2

**Status: B-1 PASS · B-2 PASS · MICRO-SPRINT READY FOR REVIEW**

Nothing was committed, pushed, merged, or closed. The working tree carries the
changes for review.

---

## 1. Branch

`fix/post-wave0-certification-debts`

## 2. Baseline / HEAD

| | |
| --- | --- |
| HEAD at start and now | `1fdce85` |
| HEAD subject | *Merge pull request #14 from yurisismotto/feature/core-platform-abstraction-v1* |
| Wave 0 merge present in base | yes — `a595d30 docs: certify Wave 0 platform abstraction`, `cfd33f6 refactor: establish cross-platform core platform boundary` |
| Working tree at start | clean |
| `git diff --check` at start | clean |

Both issues were read from GitHub with `gh`, not from the prompt text:

* **#12** — *P2: Sharesheet file offer can remain stuck on Sending after failure.* "SendActivity.startSend ignores the Result returned by files.offer()…"
* **#13** — *P3: bound real_backend wl-copy --clear during locked GNOME session.* "…invokes wl-copy --clear via blocking Command::status() without the production BACKEND_TIMEOUT."

Both descriptions were confirmed against the actual code before anything was
changed. Both were accurate.

## 3. Files modified

Six files, all implementation or test. No protocol, no build config, no docs.

| File | ± | Why |
| --- | --- | --- |
| `android/.../ui/SendActivity.kt` | +64 −20 | B-1: the fix |
| `android/.../ui/UiMapping.kt` | +75 −2 | B-1: the state vocabulary and the two decisions, as plain functions |
| `android/.../files/FileTransferManager.kt` | +9 −1 | B-1: stop a platform exception carrying a `content://` URI out of `offer` |
| `android/.../ui/MainActivity.kt` | +3 −1 | B-1: the other `offer` caller, same safe mapping |
| `android/.../test/.../UiMappingTest.kt` | +179 −1 | B-1: 11 tests |
| `desktop/capabilities/clipboard/tests/real_backend.rs` | +248 −8 | B-2: the bound, plus 5 tests |

---

# B-1 — Sharesheet stuck on "Sending…"

## 4. Root cause

`SendActivity.startSend` was three lines, and the middle one threw away a
`Result`:

```kotlin
lifecycleScope.launch {
    app.files.offer(peer.fingerprint, uri)   // Result<String>, discarded
}
```

The screen's own state was a bare `var started = false`, set to `true` on tap
and **never set back**. The button rendered `if (started) "Sending…" else
"Send"` and was `enabled = !started`.

The part that makes this a real hardware bug rather than a cosmetic one is
*where* `offer` fails. It has five early refusals that return **before** a
`Transfer` is created and `publishState()` is called:

| Refusal | `FileTransferManager.kt` |
| --- | --- |
| peer not granted `files.v1` | `isAuthorized` check |
| no control session | `controlSink == null` |
| at `MAX_CONCURRENT_TRANSFERS` | `activeCount(peer)` check |
| name the sanitizer refuses | `shared.displayName() == null` |
| the URI cannot be opened or measured | `shared.measure()` throws |

On all five, nothing is ever added to `files.visible`. So the screen's other
branch — `transfers.filter { it.sending && it.filename == name }` — stayed
empty, the `TransferRow` never appeared, and the disabled "Sending…" button
was the terminal state of the screen. The person's only recourse was to
dismiss the Sharesheet. Nothing had been offered and nothing ever would be.

The late failure path (`sendControl` returning false) was *not* affected: it
calls `finish(transfer, FAILED, TRANSPORT)` on a transfer that already exists,
so a row appears. That is why this reproduced only on some failures, which
matches how it was found.

## 5. The fix

**No architecture change, no protocol change, no new error model.** The
success path is byte-for-byte the same `files.v1` flow.

1. **`UiMapping.SendAttempt`** — a four-state sealed interface (`Idle`,
   `Sending`, `Sent`, `Failed(message)`) replacing the boolean. `Sending` is
   the only non-terminal state, and it is the only one no `Result` can map to.

2. **`UiMapping.sendOutcome(Result<String>): SendAttempt`** — a *total*
   function over `Result`. This is where the bug actually died: `Result` has no
   failure branch you are forced to take, so the branch was simply absent.
   Stated as a total function, there is no path that returns `Sending` and none
   that returns nothing.

3. **`UiMapping.sendButtonLabel(SendAttempt)`** — shared by both Sharesheet
   screens so their labels cannot drift.

4. **`startSend`** now takes an `onOutcome` callback and calls it on every
   path. The composable holds `attempt` instead of `started`, renders the
   failure message above the button, and re-enables the button as "Try again".

Retry works because `Failed.canSend` is `true` and no state outside the
composable was mutated by a refused offer — `offer` returned before creating a
transfer, so there is nothing to reset and no phantom transfer to clean up.

### Concurrency and lifecycle

* **Double tap** — `enabled = attempt.canSend`, false for both `Sending` and
  `Sent`. The click handler sets `Sending` synchronously before launching.
* **Callback after the Activity leaves** — `lifecycleScope` is cancelled at
  `onDestroy`, so the continuation after `offer` does not resume into a dead
  composition. No cancellation state is invented: a cancelled scope means the
  screen is gone and there is no one to tell.
* **Repeated attempts** — each retry re-enters `Sending` and gets its own
  outcome. Nothing accumulates.
* **Success does not regress to a stuck button** — verified that
  `FileTransferManager.transfers` is never pruned (no `remove`/`clear`
  anywhere), so once an offer succeeds its `TransferRow` persists for the life
  of the screen.

### Adjacent defect fixed with it — please note in review

`startTextSend`, forty lines away, had the **identical** stuck-`started`
defect on the `clipboard.v1` share path: the failure toast was shown but
`started` was never reset, so the button stayed disabled on "Sending…". Issue
#12 names `files.offer` only. I fixed both, because the fix is the same helper
applied to the same defect and leaving one behind would have been strange.
**This is one function beyond the letter of #12** — flagging it explicitly
rather than burying it.

### Security

* `FileTransferManager.offer` no longer returns the raw platform exception
  from `shared.measure()`. It restates it as `IllegalStateException("that
  file could not be read", e)` — cause kept for a debugger, message free of
  file data. A `FileNotFoundException` from a content provider carries the
  whole `content://` URI, and on many providers that plus the display name
  identifies the file.
* `UiMapping.sendFailureMessage` is the backstop: it shows the message only
  for `IllegalStateException` — the type the capability uses for its own
  refusals, all authored for a person — and replaces anything else with
  "AnyFlow could not send that file." A future path that reintroduced a raw
  platform exception degrades safely instead of leaking.
* `SendAttempt.Failed` holds a `String`, not a `Throwable`, so there is no
  field a stack trace or URI could travel in to a log or crash reporter.
* The new code logs **nothing**. No filename, no URI, no content.
* `MainActivity`'s file-picker `offer` caller was routed through the same
  mapper; it previously showed `it.message` raw and had the same leak.

## 6. Tests — B-1

11 new JVM tests in `UiMappingTest`, which the file's own doc explains is the
right home ("instrumented run destroys the pairing every time"). No
instrumented test was needed: the whole decision is now plain functions.

| Test | Covers |
| --- | --- |
| `a successful offer leaves the sending state` | offer success, no regression |
| `a failed offer leaves the sending state and says why` | Sending removed, error appears |
| `a failure can be retried from` | retry stays possible |
| `an attempt in flight cannot be started a second time` | double tap / re-entry |
| `no offer result can leave the screen sending` | all 8 result shapes → never `Sending` |
| `the capability's own refusals are shown as written` | the 5 authored messages survive |
| `a platform error never puts the shared file on screen` | URI / path / filename leak |
| `an error with no message still says something` | null, blank, absent message |
| `the failure message carries nothing but the message` | no throwable in the state |
| `the button never reads Sending once the attempt has finished` | the literal symptom |
| `every offer result produces a button a person can read` | label agrees with enablement |

The leak test uses the real shapes: `FileNotFoundException: No content
provider: content://media/external/images/media/1234`, a `SecurityException`
naming a provider, and a `RuntimeException` carrying
`/storage/emulated/0/Documents/tax-return-2025.pdf`.

No existing test was weakened, changed, or deleted.

## 7. Verdict

**B-1 PASS.**

* `files.offer` failure is handled — ✅
* UI leaves "Sending…" — ✅
* the error is observable on screen — ✅
* retry works without restarting the app — ✅
* success continues to work unchanged — ✅
* tests protect the regression — ✅ (11)
* no protocol change — ✅
* no persistence or logging of sensitive content — ✅

---

# B-2 — `wl-copy --clear` could block the test forever

## 8. Root cause

One call site, `real_backend.rs`, in the empty-clipboard test:

```rust
let cleared = std::process::Command::new("wl-copy")
    .arg("--clear")
    .stdin(...).stdout(...).stderr(...)
    .status();          // blocking, unbounded
```

`std::process::Command::status()` blocks the thread until the child exits. On
a locked GNOME seat `wl-copy` waits forever for a seat and a serial the
compositor will not grant while the session is locked — the exact condition
`BACKEND_TIMEOUT` exists to bound. The test therefore had a path the
**production backend does not**: production wraps every invocation in
`tokio::time::timeout(BACKEND_TIMEOUT, …)` with `kill_on_drop(true)`.

The failure mode was also silent: no output, no timeout, indistinguishable
from a slow machine.

## 9. The fix

A `bounded_status(program, args, limit)` helper in the test file, using
**deliberately the same mechanism as `WaylandBackend::write_text`** — tokio
process spawn, stdio to `/dev/null`, `kill_on_drop(true)`, wrapped in
`tokio::time::timeout` — and a `clear_clipboard()` wrapper that passes the
production `limits::BACKEND_TIMEOUT` itself.

It is **not** a second timeout policy. The real call site uses the production
constant; `limit` is a parameter only so the tests can prove the bound in
milliseconds instead of making every run wait five seconds.

**Why not extract a helper from production instead:** the production timeout
is inline inside `read_text` and `write_text`, which are certified Wave 0
clipboard hot paths. Extracting a shared helper would have meant refactoring
them — a larger and riskier change than the P3 debt it fixes, and outside this
sprint's scope. The smallest correct abstraction that shares the same constant
is the one taken.

The call site now distinguishes three outcomes and names the locked-seat one
out loud rather than skipping quietly:

```
wl-copy --clear timed out after 5s — the seat is most likely locked.
Unlock the screen and run this again.
```

This is not "skip always": the test still runs, still asserts, and the
locked-seat exit is a loud, named, bounded stop, consistent with the suite's
existing documented stance on that environment ("a certification run that
quietly failed for this reason would be worse than one that stopped and said
unlock the screen").

## 10. Timeout and process cleanup

| Property | How |
| --- | --- |
| bound | `tokio::time::timeout`, `BACKEND_TIMEOUT` (5 s) at the real call site |
| same constant as production | yes — `limits::BACKEND_TIMEOUT`, imported |
| child killed | `child.start_kill()` on expiry |
| child **reaped** | `child.wait().await` after the kill, so no zombie — stricter than production's `kill_on_drop`, which reaps asynchronously |
| orphans | none: `--clear` is the one `wl-copy` mode with no content to serve, so it forks no daemonised survivor |
| deterministic error | `BoundedError::{TimedOut, Spawn, Wait}` — a named variant, not a string to parse |
| no `kill -9` without cleanup, no sleep, no infinite loop | ✅ |
| clipboard semantics | untouched — no production file changed |
| coverage removed | none — all 9 `#[ignore]` tests intact |

## 11. Tests — B-2

5 new tests, and importantly they are **not** `#[ignore]`d and touch no
clipboard, so the regression is caught by plain `cargo test` on a headless
machine rather than only by someone who remembers to lock their screen.

| Test | Covers |
| --- | --- |
| `a bounded helper that exits reports its status` | normal command still works; non-zero ≠ success |
| `a helper that never returns is bounded and fails deterministically` | locked-seat simulation (`sleep 30`, 200 ms bound) → `TimedOut` |
| `a timed out helper leaves no child behind` | pid gone from `/proc` after kill+reap |
| `the bound never reports the content it was given` | no payload in the error or its `Debug` |
| `a missing program is distinguishable from a locked seat` | `Spawn` ≠ `TimedOut` — "install wl-clipboard" vs "unlock your screen" |

The locked-seat test uses a stand-in child at 200 ms, so the suite does not
wait real seconds repeatedly. The hardware `real_backend` tests remain
separate and `#[ignore]`d.

## 12. Verdict

**B-2 PASS.**

* no known path where the test's `wl-copy --clear` blocks indefinitely — ✅
* bounded by the production constant — ✅
* child killed and reaped — ✅
* deterministic error variant — ✅
* production does not regress (no production file touched; all 9 hardware
  tests pass on a real compositor) — ✅
* tests protect the regression — ✅ (5)

---

# Regression results

## 13. Rust

Run sequentially in `desktop/`, one suite at a time.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | clean |
| `cargo build --workspace --locked` | success |
| `cargo test --workspace --locked` | **371 passed, 0 failed, 9 ignored** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean |
| `cargo test -p …-clipboard --test real_backend -- --ignored --test-threads=1` | **9 passed, 0 failed** |

| | Baseline | New | Total | Failures | Ignored |
| --- | --- | --- | --- | --- | --- |
| automated | 366 | +5 | **371** | 0 | 9 |
| hardware `real_backend` | 9 | 0 | **9** | 0 | — |
| **executed** | 375 | +5 | **380** | **0** | |

The 9 ignored count is unchanged — no `real_backend` coverage was removed or
converted.

`clippy` initially failed on my new tests: this crate lints
`clippy::unwrap_used`, and I had used `unwrap_err()`. Changed to `expect_err()`
with messages. Not a production issue.

### Hardware clipboard gate

Executed on a real compositor: `XDG_SESSION_TYPE=wayland`,
`WAYLAND_DISPLAY=wayland-0`, `LockedHint=no`, `wl-copy`/`wl-paste` present.
All 9 passed in 8.54 s, including
`an_empty_clipboard_is_bounded_and_classified_never_an_unexplained_failure` —
the test whose clear call was the debt. Normal clipboard behaviour is
unchanged.

**The locked-seat case itself was not exercised on hardware** — locking the
session would have locked the operator out mid-run. It is covered
deterministically by the never-returning-child test. Noted as a residual, not
claimed as a hardware pass.

### Wave 0 portability gate

`cargo test --locked --no-run --no-default-features -p anyflow-capability-clipboard`
compiles, including `real_backend.rs`. The MSVC half of that gate
(`--target x86_64-pc-windows-msvc`) is CI-only and was **not** run here — no
MSVC target on this machine. The new code is portable std + tokio with
`#[cfg(unix)]` / `#[cfg(target_os = "linux")]` on the three tests that need it.

## 14. Android JVM

`./gradlew :app:testDebugUnitTest` (JDK 21 + `ANDROID_HOME`, per this
machine's toolchain — the default JDK 25 is refused by AGP).

| | Count |
| --- | --- |
| baseline | 232 |
| new | +11 |
| **total** | **243** |
| **failures** | **0** |
| skipped | 0 |

All 11 new tests are in `UiMappingTest` (26 total there). No existing test was
modified to hold the count.

## 15. Android instrumented / hardware

**HARDWARE TEST NOT EXECUTED.**

`adb devices` lists no attached device. `connectedDebugAndroidTest` was not
run and no result is claimed for it. The reference hardware from Wave 0 is
Samsung SM-X620, Android 16 / API 36.

This is the one gap in the evidence for B-1: the fix is proven by JVM tests
over the extracted decision functions and by reading the composables, not by
re-running the Sharesheet on a device. **Recommend a manual Sharesheet check
on hardware before merge** — share a file with the phone disconnected from
the desktop, confirm the button returns to "Try again" with a message, then
reconnect and confirm the retry sends.

## 16. Security regression

Nothing certified was touched. TLS 1.3, SPKI pinning, explicit pairing,
capability grants, clipboard policies, `sensitive_hint` fail-closed, no
clipboard history, no clipboard persistence, file challenge/HMAC, filename
hardening, identity no-silent-regeneration — all unchanged; no file
implementing any of them is in the diff.

Net change is in the **safe** direction on two counts: a content-provider URI
can no longer reach the Sharesheet screen or the main screen's error toast,
and the B-2 helper cannot carry clipboard content into a log.

The 9 hardware clipboard tests and the crate's `logging.rs` and `security.rs`
suites all pass.

## 17. Protocol unchanged

```
$ git diff --name-only | grep -E '(^protocol/|\.proto$)'
(no output)
```

**`protocol/** = unchanged`. `*.proto` = unchanged.**

Also unchanged: protocol version, framing, TLS, SPKI, pairing, capability
negotiation, state schema, Wave 0 platform seams. No `notifications.v1`, no
Windows, no macOS, no new features.

Deliberately *not* done: `FailureReason` was considered as the error
vocabulary for `offer`'s refusals and rejected — it is mapped to the proto
enum `TransferFailureReason` at `FileTransferManager.kt:797`, so adding a
value to cover "not connected" would have pushed at the protocol boundary.
The existing `IllegalStateException` messages were kept instead.

## 18. Secrets / artifact audit

```
$ git diff --name-only | grep -E '\.(key|pem|p12|pfx|jks|keystore|apk|aab)$|state\.json|identity\.key|trust-store|/target/|/build/|logs|captures'
(no output)
```

No untracked files. The six modified files are all `.kt` or `.rs` source.

## 19. `git diff --check`

Clean. (Also clean at the start, and re-checked after every edit.)

## 20. `git status`

```
 M android/app/src/main/java/io/github/yurisismotto/anyflow/files/FileTransferManager.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/SendActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/UiMapping.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/UiMappingTest.kt
 M desktop/capabilities/clipboard/tests/real_backend.rs
?? POST-WAVE0-DEBTS-MICRO-SPRINT.md
```

The untracked file is this report, written to the repo root alongside the
existing `WAVE-0-*` reports. It is not part of either fix — drop it if you
prefer the report to live only in review.

Nothing committed, staged, pushed, or merged. No issue closed, no PR opened,
no GitHub comment posted.

## 21. Issues

**#12 READY TO CLOSE** — with the caveat in §15: no on-device Sharesheet
re-test was possible. If you want hardware confirmation before closing, that
is the one thing missing.

**#13 READY TO CLOSE.**

Neither was closed and no comment was posted. The decision is yours after
review and merge.

## 22. Risks and remaining debts

1. **No on-device verification of B-1.** The largest gap. See §15.
2. **The locked-seat path is proven by simulation, not by a locked seat.**
   Deterministic, but not the real compositor. See §13.
3. **MSVC portability gate not run locally** — CI-only. Low risk; the new code
   is portable and the Linux `--no-default-features` compile passes.
4. **`startTextSend` was fixed too**, one function beyond the letter of #12.
   Called out in §5 so review can accept or reject it deliberately.
5. **Pre-existing, untouched:** `FileTransferManager.offer` logs the filename
   at `Log.i` on the *success* path (`"offering ${transfer.filename}…"`). Not
   in scope for B-1, which is about failure handling, and not changed. Worth a
   separate look if filenames in logcat are a concern.
6. **`sendFailureMessage` keys on `IllegalStateException`.** It is a type
   whitelist, so a future refusal thrown as a different type degrades to the
   generic message — safe, but slightly less informative. The alternative
   (a typed error carrying `FailureReason`) was rejected for the protocol
   reason in §17.

## 23. Recommended commit message

```
fix: bound the two debts Wave 0 certification left open

B-1 (#12): SendActivity.startSend discarded the Result of files.offer, and
the screen's own state was a boolean that was set on tap and never cleared.
The five refusals inside offer that return before a Transfer exists — not
granted, not connected, at the concurrency limit, an unusable name, an
unreadable URI — therefore published nothing to files.visible, so the
progress row never appeared and the disabled "Sending…" button was the
terminal state of the Sharesheet. Nothing had been offered and nothing ever
would be.

The send state becomes UiMapping.SendAttempt and the mapping from Result
becomes a total function, so there is no longer a path that returns Sending
and none that returns nothing. A failure is stated above the button and the
button returns as "Try again". startTextSend had the identical defect on the
clipboard share path and is fixed with it.

offer no longer hands back the raw platform exception from measure(): a
content provider names the whole content:// URI in its message. It is
restated with the cause kept for a debugger, and the UI mapping refuses to
show anything but the capability's own authored messages.

B-2 (#13): the real_backend empty-clipboard test cleared the clipboard with
a blocking std::process::Command::status(), which on a locked GNOME seat
waits forever — the exact condition BACKEND_TIMEOUT exists to bound, giving
the test a more dangerous path than production. The clear now goes through a
bounded helper using the same mechanism as WaylandBackend::write_text and the
same BACKEND_TIMEOUT, killing and reaping the child on expiry and reporting a
named variant rather than a string. The locked seat is now a loud, bounded
stop instead of a silent hang.

No protocol, framing, TLS, pairing, capability or state-schema change. Wave 0
architecture and its certification are untouched.

Rust: 371 passed (366 + 5), 0 failed, 9 ignored; 9 hardware real_backend
tests pass on a live compositor.
Android JVM: 243 passed (232 + 11), 0 failed.
Android instrumented: not executed, no device attached.

Refs #12, #13
```

---

# Final verdict

**B-1 PASS**
**B-2 PASS**
**MICRO-SPRINT READY FOR REVIEW**

---

## Post-review hardware smoke — B-1

Manual hardware smoke executed by the maintainer after the implementation review.

Result: PASS.

Verified flow:

- Android Sharesheet -> AnyFlow;
- desktop/daemon unavailable;
- send attempt did not remain stuck on "Sending…";
- failure became visible to the user;
- retry became available;
- desktop/daemon restored;
- retry successfully sent the file.

This closes the remaining manual hardware evidence gap identified during the micro-sprint review.

**B-1 HARDWARE SMOKE: PASS**
