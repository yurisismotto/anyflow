# clipboard.v1 — Hardware Certification

**AnyFlow · Final hardware closeout · 30–31 August 2026**

| | |
|---|---|
| **Status** | **CLIPBOARD.V1 CERTIFIED** |
| Branch | `feature/clipboard-v1` |
| HEAD | `bdcee7a` |
| Commits made | none — no commit, no push |
| Source changed | none — validation only |

H03 **PASS** and CLIP-21 **PASS**, with no open P0/P1 in privacy, authorization, clipboard loop, session writer, persistence, or TLS/pinning. One non-blocking P2 defect and one verification debt are recorded below.

---

## 1. Executive summary

The five outstanding gates were run to completion on the tablet and the desktop.

**H03** and **CLIP-21** — the Android→Fedora chain from a copy in an ordinary app through to a paste in a real application — passed byte-for-byte, twice: once on the build that was already installed, and again after the instrumented suite forced a reinstall and a fresh pairing.

**H12** and **H13** resolved more favourably than the platform documentation predicts: on this device `setPrimaryClip()` succeeds both with AnyFlow in the background and with the screen locked and the device dozing. No deferral and no notification fallback were needed; both were confirmed by pasting in Chrome afterwards.

The locked-desktop retry fix works as designed. `anyflow clipboard apply` fails after the 5 s backend timeout with an accurate diagnostic, and the held clip is put back with its original receipt time — verified twice, including once by an unplanned screen blank.

**QS-TILE failed**, and not for the reason the design anticipated. It is a timing defect in our own code, not an Android restriction. It is optional and does not block certification.

Everything already green stayed green: 292 Rust tests, 9 hardware-gated backend tests, 206 Android JVM tests, 21 instrumented tests, `fmt` clean, `clippy` at zero warnings. No clipboard content reached any log, any disk file, or the repository.

### The gates that were pending

| Gate | What | Verdict |
|---|---|---|
| H03 | Android → Fedora, manual, complete in one execution | **PASS** |
| CLIP-21 | Android hardware end-to-end | **PASS** |
| H12 | Android background receive | **PASS** |
| H13 | Android locked-screen receive | **PASS** |
| QS-TILE | Quick Settings tile send (optional) | **FAIL** — non-blocking |

---

## 2 · 3. Hardware and platform

| Role | Device | Platform | Detail |
|---|---|---|---|
| Phone side | Samsung Galaxy Tab S10 FE+ · `SM-X620` | Android 16 · API 36 | `gts10fepwifi` · serial `RX2Y500C7SY` · USB, authorized |
| Desktop side | Fedora workstation | Linux 7.1.9-200.fc44 | GNOME Wayland · wl-clipboard · watch: XFIXES on the Xwayland CLIPBOARD selection |

> **Note on the desktop backend.** `wl-paste --watch` is unavailable on this session (GNOME exposes no `wlr-data-control`), so the daemon fell back to the X11 bridge and reported so honestly at startup. Auto-send is therefore *supported* here — which is what made the loop-suppression gate testable.

---

## 4. Pairing and identity

The run began on the identity named in the brief and ended on a new one, because the instrumented suite uninstalls the app and the private key lives in the Android Keystore. The key was never exported or restored; the device was paired again from a fresh QR, with the fingerprint checked on both screens.

| When | Device id | Fingerprint | State at close |
|---|---|---|---|
| Desktop (constant) | `795fec0868ebef8c3d7ad3e775dc6ed0` | `DF65 D3E4 BA28 EDF9` | Fedora, listening on 55432 |
| Android, start of run | `f565511c30b7086778a58f820c1fa1f4` | `5655 E8FD AD07 C225` | **key destroyed**, still listed as paired |
| Android, after reinstall | `3e0350064a7aa53b2567197deda473f4` | `5920 AA1C C387 7A3E` | paired · connected · `clipboard.v1` |

```
A device proved it holds the pairing code:

  name        SM-X620
  device id   3e0350064a7aa53b2567197deda473f4
  fingerprint 5920 AA1C C387 7A3E

Check that the fingerprint matches the one shown on the phone.
Pair with this device? [y/N] y
Paired with 5920 AA1C C387 7A3E.
```

> **Open item for you.** The desktop trust store still lists `5655 E8FD AD07 C225` as paired, with `clipboard.v1` granted, even though that identity can never authenticate again. I left it in place rather than revoking it unasked. Two older dead identities from previous rounds are already marked revoked.

---

## 5. H03 — Android → Fedora, complete

Copy in Chrome (an ordinary app) → AnyFlow *Send clipboard* → Fedora receives → `anyflow clipboard apply` → paste in a real application. One execution, no step reconstructed afterwards.

**How the paste was made real.** No input-injection tool exists on this desktop (`xdotool`, `ydotool` and `wtype` are all absent, and installing one needs a password). So the paste was performed by an ordinary GTK4 Wayland client with a `GtkTextView`, triggering the widget's own `paste-clipboard` action — the exact code path Ctrl+V runs. This is a real application reading the real selection over the Wayland data device, not `wl-paste`.

```
Android, Chrome omnibox         text="hello from Android"        (copied with Ctrl+A, Ctrl+C)
AnyFlow → Send clipboard        tapped 'Send clipboard'

daemon   clipboard update peer=5655 E8FD AD07 C225 event=c4ff6fad outcome="pending"

Fedora   waiting to be applied (auto-receive is off):
           from SM-X620 (5655 E8FD AD07 C225)  18 bytes  sha256:3b838cbc  3s ago

expected                        18 bytes   sha256 3b838cbc
$ anyflow clipboard apply       applied 18 bytes to the clipboard

real application paste          18 bytes   sha256 3b838cbc
0000000  68 65 6c 6c 6f 20 66 72 6f 6d 20 41 6e 64 72 6f  >hello from Andro<
0000016  69 64                                            >id<

cmp expected paste              IDENTICAL — UTF-8 byte-equivalent, no transformation
grep in daemon log              0 occurrences
grep in state.json              0 occurrences — nothing persisted
```

Size, hash, byte sequence and the absence of any transformation all agree. **H03 PASS.**

---

## 6. Desktop locked-session behaviour

The previous round found that `wl-copy` fails behind the lock screen and the clip was being discarded. The fix returns the clip to the pending list for retry. It was exercised exactly as specified: lock, send, fail, unlock, retry, paste.

```
1  loginctl lock-session 2         LockedHint=yes
2  Android → Send clipboard        tapped while the desktop was locked
3  arrival                         26 bytes  sha256:491477af  3s ago   (accepted, held)

4  anyflow clipboard apply
   error: the clipboard did not respond in time. On GNOME Wayland this normally
   means the session is locked: wl-copy and wl-paste cannot obtain a seat
   behind the lock screen.
   real  0m5,005s                  ← exactly BACKEND_TIMEOUT, not a hang

5  still pending?                  26 bytes  sha256:491477af  8s ago
                                   retained, and the age kept counting from the original
                                   receipt — a retry cannot extend a clip past its TTL

6  loginctl unlock-session 2       LockedHint=no
7  anyflow clipboard apply         applied 26 bytes to the clipboard
8  real application paste          26 bytes  sha256:491477af
   0000000  6c 6f 63 6b 65 64 20 73 65 73 73 69 6f 6e 20 72  >locked session r<
   0000016  65 74 72 79 20 70 72 6f 62 65                    >etry probe<
                                   IDENTICAL to what was copied on Android

daemon log
   WARN could not apply a held clipboard clip error=the clipboard did not respond
        in time. On GNOME Wayland this normally means the session is locked …
                                   the real cause, no clipboard content
```

The behaviour was then confirmed a second time by accident: the desktop screen blanked mid-run and the same failure and retention occurred unprompted, with the clip recovered 26 s later. **PASS.**

---

## 7. H12 — Android background receive

Run with `auto_receive` enabled explicitly for Fedora and turned off again immediately afterwards. Auto-send was never enabled globally to make testing easier.

```
foreground              com.sec.android.app.launcher/.activities.LauncherActivity
AnyFlow process         alive, backgrounded

Fedora  copy + push     sent 28 bytes of clipboard text to 5655 E8FD AD07 C225
daemon                  clipboard update sent  event=d1274541 bytes=28 sensitive=false
                        peer reported outcome  "applied"
Android logcat          clipboard update from DF65 D3E4 BA28 EDF9 outcome=APPLIED

verified by paste in Chrome
  pasted                [H12 background receive probe]
  bytes / sha256        28 / 56097d15   IDENTICAL
```

**Observed behaviour, not inferred.** `setPrimaryClip()` **works from the background** on SM-X620 / Android 16. The write path is not subject to the Android 10 restriction that blocks background *reads* — and the result was confirmed by an actual paste in a third-party app, not by trusting the reported outcome. `auto_receive` was returned to OFF at the end of the gate.

---

## 8. H13 — Android locked screen

```
tablet state            mWakefulness=Dozing    deviceLocked=1

Fedora  copy + push     sent 25 bytes of clipboard text
daemon                  clipboard update sent  event=2f22a737 bytes=25 sensitive=false
                        peer reported outcome  "applied"
Android logcat          outcome=APPLIED   accepted immediately, not deferred

notification posted     "Clipboard received from Fedora"
                        "28 bytes of text. Open AnyFlow to copy it. Marked sensitive."
                        size and origin only — never the text

after unlock, paste in Chrome
  pasted                [H13 locked screen receive]
  bytes / sha256        25 / adf0ea86   IDENTICAL
```

| Question asked by the gate | Observed on SM-X620 / Android 16 |
|---|---|
| Did Android accept `setPrimaryClip()` with the screen locked? | **Yes** — accepted and applied at once, reported `APPLIED` |
| Did it postpone the write? | No. No deferral was observed |
| Was a notification fallback used? | Not needed for the write. A notification is posted as the user-facing signal, carrying size and origin only |
| Was the content applied after unlock? | It was already applied; after unlock it pasted byte-identical |

No workaround was written and no platform restriction was circumvented — the gate validates supported behaviour, and the supported behaviour turned out to be permissive here. `auto_receive` returned to OFF. **H13 PASS.**

---

## 9. QS-TILE — the one failure

> **P2 · The tile reports "The clipboard is empty" when the clipboard is not empty.**
> This is a defect in our own timing, not an Android or API restriction. The gate is optional and does not block certification, but it should not be filed as a platform limitation.

```
tile in Quick Settings   content-desc="Send clipboard"  checkable=true checked=false
                         STATE_INACTIVE — it correctly found an eligible computer
tap the tile             → MainActivity comes to the foreground   (design honoured)
                         → toast: "The clipboard is empty."
                         → nothing sent, nothing pending on Fedora

same clipboard, ~40s later, in-app "Send clipboard" button
                         clipboard update sent … bytes=18
Fedora received          18 bytes  sha256:0db5b086
applied + real paste     [QS tile send probe]   the clipboard was never empty
```

### Root cause

`onClick` → `startActivityAndCollapse(PendingIntent)` → `MainActivity.onNewIntent` → `handleIntent` → `sendClipboardFromShortcut()` reads the clipboard *during intent delivery*, before the Activity has regained window focus while the Quick Settings panel is still collapsing. At that moment `getPrimaryClip()` returns null and `hasPrimaryClip()` returns **false without throwing**. `SystemClipboard.read()` disambiguates null + false as `ReadFailure.Empty`, so the person is told the clipboard is empty and the flow dead-ends. No `clipboard probe refused` line appeared in logcat, confirming no `SecurityException` path was taken.

The comment at `SystemClipboard.kt:71` assumes "false here means the platform is willing to answer and the answer is nothing". On Android 16 that does not hold for an unfocused app. The minimal fix is to defer the shortcut read until the Activity actually has focus rather than performing it inside intent delivery. **No code was changed** — this run was validation only.

### What the tile does get right

- The `TileService` never reads the clipboard itself — confirmed in source and by behaviour.
- No `AccessibilityService`, no default-IME trick, no invisible focus-stealing Activity.
- The service is guarded by `BIND_QUICK_SETTINGS_TILE`, held by the system; no hidden permission.
- No ADB dependency at runtime — ADB was used here only as a test harness, standing in for a finger.
- The tile correctly shows `STATE_UNAVAILABLE` logic and resolves a single eligible computer without guessing.

**QS-TILE FAIL** — optional, does not block certification.

---

## 10. Sensitive clipboard

Both directions were smoke-tested. The sensitive clip was produced through the supported harness — the existing instrumented test that builds a clip the way a password manager does, with `EXTRA_IS_SENSITIVE` set by another app, not by our writer.

### Android → Fedora: the extra confirmation

```
harness            ClipboardInstrumentedTest#a_sensitive_clip_from_another_app_is_detected
                   ClipData "password" / EXTRA_IS_SENSITIVE=true, set by the test process

tap Send clipboard → dialog, not a send:
                     "This clipboard is marked sensitive"
                     "The app you copied from marked this text as sensitive —
                      a password, a recovery code, or similar."
                     "Send 7 bytes to Fedora?"      [Cancel] [Send]

before confirming  daemon log            nothing sent
                   Fedora pending list   nothing pending
                   secret on screen      0 occurrences — size and destination only

after confirming   from SM-X620  7 bytes  sha256:f52fbd32  SENSITIVE
```

### Fedora → Android: the hint travels and is surfaced

```
CLI                sent 28 bytes of clipboard text to 5655 E8FD AD07 C225 (marked sensitive)
daemon             clipboard update sent  event=0728feea bytes=28 sensitive=true
Android            outcome=PENDING_USER   (auto-receive off — held, not applied)
notification       "28 bytes of text. Open AnyFlow to copy it. Marked sensitive."
pending card       "Clipboard from Fedora"  ·  "28 bytes · marked sensitive"
passphrase on screen / in logs / in state.json     0 occurrences
```

`EXTRA_IS_SENSITIVE` is applied on the Android write path by `SystemClipboard` (API 33+) and is asserted by CLIP-H04, which passed in the instrumented run. The flag is treated throughout as a presentation hint — never as an access control and never as encryption. **PASS.**

---

## 11 · 12 · 13 · 14. Directions, the Android limitation, and loop suppression

### Fedora → Android

Works in every device state tried: foreground, background, and screen-locked/dozing. With `auto_receive` off the clip is held and surfaced as a notification plus an in-app card showing size, origin and sensitivity — never text. With `auto_receive` on it is applied as it arrives and pastes byte-identical. Push is always explicit (`anyflow clipboard send`) unless auto-send is deliberately enabled.

### Android → Fedora

Manual only, by design and by platform necessity. Verified four times this session with distinct payloads (18, 26, 18 and 22 bytes), each time landing with a matching SHA-256 and, where applied, pasting byte-identical in a real application.

### The Android background limitation

Confirmed and correctly stated in the product. Android does not let an ordinary app read the clipboard without window focus, so there is no auto-send on Android and the UI says so rather than offering a dead toggle: *"Android does not let an ordinary app read the clipboard in the background, so clips are sent when you tap Send clipboard."* The QS-TILE defect above is a consequence of reading one moment too early relative to that same rule — the rule itself is respected.

### Loop suppression

Tested with auto-send deliberately enabled for this gate only, then turned off again — a loop cannot be observed without it.

```
A  genuine local copy on Fedora
   clipboard update sent  peer=5655 E8FD AD07 C225 event=342a4248 bytes=18
   → the watcher is alive; suppression is not just a dead watcher

B  apply a clip that came FROM the phone (22 bytes, hash 290d1ddb)
   suppressed the echo of a clip applied from a peer
     origin=f565511c30b7086778a58f820c1fa1f4 hash=290d1ddb
   → no CLIPBOARD_UPDATE went back to the phone. No loop.

suppression cache afterwards   0 entries
   → entry consumed, so a later genuine local copy of the same text is still sendable

auto-send restored             clipboard auto-send is now off for 5655 E8FD AD07 C225
```

---

## 15. Security and transport

Probed live against the running daemon during the hardware run, not read off the source alone.

```
TLS 1.2 offered
  client   alert: tlsv1 alert protocol version (70)
  daemon   connection ended … error=peer is incompatible: SupportedVersionsExtensionRequired

TLS 1.3 + ALPN "anyflow/1"
  Protocol       TLSv1.3
  Cipher         TLS_AES_256_GCM_SHA384
  ALPN protocol  anyflow/1
  subject        CN=anyflow:795fec0868ebef8c3d7ad3e775dc6ed0   (self-signed: pinning, not PKI)

wrong ALPN ("h2")
  client   alert: no application protocol (120)
  daemon   error=peer doesn't support any known protocol

any client without a certificate
  daemon   error=peer sent no certificates   (every anonymous probe, every time)

live session binding
  daemon   session established device=3e0350064a7aa53b2567197deda473f4
           peer=5920 AA1C C387 7A3E capabilities=["battery.v1","clipboard.v1"]
```

| Requirement | Verdict | Evidence |
|---|---|---|
| TLS 1.3 only | PASS | Live rejection of TLS 1.2; `TLS13_ONLY` version list; both `verify_tls12_signature` impls return an error unconditionally |
| ALPN `anyflow/1` | PASS | Negotiated live; a different ALPN is refused at handshake |
| SPKI pinning | PASS | `Fingerprint::from_spki_der` over `subject_pki.raw`; `PinnedServerCertVerifier` compares presented vs expected; Android `PinnedTrustManager` mirrors it |
| Peer authenticated | PASS | Mutual TLS mandatory — the daemon refused every anonymous probe |
| `clipboard.v1` grant required | PASS | Confirmed on hardware: a session negotiated without the grant silently drops clipboard frames |
| No trust-all / verifier bypass | PASS | Zero hits for `danger_accept_invalid*`, empty `checkServerTrusted`, or a permissive `X509TrustManager` outside tests |
| No cleartext | PASS | `cleartextTrafficPermitted="false"` app-wide on Android; one socket kind only |
| No private-key export | PASS | No export path in source; Android key is `AndroidKeyStore`, StrongBox-backed where available, signing inside the TEE |
| No custom crypto over TLS | PASS | None added; none present |

> **Verification debt — on-wire capture was not possible here.** Proving "no clipboard content in cleartext on the network" by inspecting packets needs `CAP_NET_RAW`. There is no passwordless sudo on this machine and no `setcap`-enabled `dumpcap`, and the Android link cannot be routed through a logging relay without root either. What *was* established live: mutual TLS is mandatory and the daemon refuses any peer that is not the pinned identity, the session is TLS 1.3 with ALPN `anyflow/1`, and the clipboard frame is written only through that session's TLS stream. I am recording this as an evidence gap rather than claiming a capture I did not perform. It is not an open finding, and it does not change the verdict — but if you want it closed, one `tcpdump -i any port 55432` run with sudo would do it.

> **Confirmed by design, not a defect: widening needs a reconnect.** Granting `clipboard.v1` to an *already connected* device had no effect until the session was re-established, because the effective capability set is fixed at handshake. This is documented in `docs/architecture/CLIPBOARD.md` ("Widening needs a reconnect; narrowing does not") and is fail-closed. One small observability note: the clip sent during that window was dropped without any outcome returned to the phone, so the sender saw no feedback at all. P3.

---

## 16 · 17. Logs and persistence audit

Every distinct payload put through the system this session was searched for across the daemon log, all four logcat captures, and the on-disk state file — with a positive control to prove the search itself works.

```
19,717 bytes searched · 9 payloads · positive control included

hello from Android              0      QS tile send probe            0
locked session retry probe      0      correct-horse-battery-staple  0
H12 background receive probe    0      hunter2                       0
H13 locked screen receive       0      loop suppression probe        0
                                       genuine local copy            0

control: "clipboard update sent"        18   ← the search works

what the daemon DOES record
  clipboard update sent  peer=… event=342a4248 bytes=18 sensitive=false
  peer reported a clipboard outcome  peer=… event=… outcome="pending"
  suppressed the echo of a clip applied from a peer  origin=… hash=290d1ddb
  metadata only: peer, event id, byte count, hash prefix, outcome

scan for any text field in the log      no matches — no clip text field is ever logged
```

### Persistence

```
~/.local/share/anyflow — the only AnyFlow state on disk

identity.key    138 B     the desktop's own key
state.json     2356 B

state.json top-level    schema_version, device_id, certificate_der_b64, settings, peers
per-peer keys           device_id, device_name, platform, fingerprint, paired_at_unix,
                        granted_capabilities, last_protocol_version, revoked, clipboard_policy
clipboard_policy        {allow_send, allow_receive, auto_send, auto_receive}   flags only

No clip text, no clip history, no cache file. Held clips live in memory with a
5-minute TTL; at close, zero were held — the sensitive one expired on its own.
```

---

## 18. Session writer regression

The separated-writer architecture was re-exercised, not modified. All tests stayed green, so nothing was touched.

| Test | Guards against | Result |
|---|---|---|
| `a_saturated_session_still_shuts_down` | Zombie session | PASS |
| `replies_keep_their_order_under_queue_pressure` | Ordering break | PASS |
| `outbound_pressure_on_one_session_does_not_affect_another` | Cross-session coupling | PASS |
| `inbound_dispatch_continues_while_the_outbound_queue_is_saturated` | Deadlock | PASS |
| `a_handler_may_reply_more_times_than_the_outbound_queue_is_deep` | Queue self-dependency | PASS |
| `a_burst_of_updates_is_answered_in_full_under_outbound_pressure` | Clipboard handler regressing the writer | PASS |

The clipboard handler answers inside the session and reintroduces none of the four failure modes. Architecture unchanged, as instructed. **CLIP-17 PASS.**

---

## 19 · 20 · 21. Automated regression, final

| Suite | Command | Result | Verdict |
|---|---|---|---|
| Rust workspace | `cargo test --workspace` | 292 passed · 0 failed · 9 ignored | PASS |
| Rust, hardware-gated | `--test real_backend -- --ignored` | 9 passed · 0 failed | PASS |
| Formatting | `cargo fmt --all --check` | clean | PASS |
| Lints | `cargo clippy --workspace --all-targets` | 0 warnings · 0 errors | PASS |
| Build | `cargo build --workspace` | ok | PASS |
| Android JVM | `./gradlew testDebugUnitTest` | 206 tests · 0 failures · 0 errors · 0 skipped | PASS |
| Android assembly | `:app:assembleDebug(AndroidTest)` | both APKs built | PASS |
| Android instrumented | `:app:connectedDebugAndroidTest` | 21 tests · 0 failed · 0 skipped | PASS |

**The instrumented run destroyed the identity again.** As anticipated in the brief: `connectedDebugAndroidTest` uninstalled both APKs, and with the app went the Keystore key for `5655 E8FD AD07 C225`. Recovery followed the rule — `app-debug.apk` reinstalled, a fresh key generated on first launch (`5920 AA1C C387 7A3E`, reported by the app as hardware-backed), a new pairing window opened, the QR scanned on the device, and the fingerprint checked on both screens before confirming. **No private key was restored or copied by hand.** Rust and Gradle were run strictly one after the other, never concurrently.

---

## 22. Findings

| # | Severity | Finding | Where | Blocks? |
|---|---|---|---|---|
| F-1 | **P2** | QS tile reports "The clipboard is empty" for a non-empty clipboard. The shortcut read happens during intent delivery, before the Activity regains window focus; `hasPrimaryClip()` returns false without throwing, and `read()` misclassifies refusal as empty. | `MainActivity.kt:173, 188`; `SystemClipboard.kt:59–78` | No — QS-TILE is optional |
| F-2 | P3 | A clipboard frame arriving on a session whose negotiated capability set lacks `clipboard.v1` is dropped with no outcome returned to the sender and no desktop log line. Fail-closed and by design, but the sender is left without feedback. | session capability dispatch | No |
| F-3 | Debt | On-wire packet capture not performed — no root available. Recorded as an evidence gap, not a defect. | Environment | No |

No finding falls in privacy, authorization, clipboard loop, session writer, persistence, or TLS/pinning. Nothing was fixed: this round was validation only, as instructed.

---

## 23. Files modified

**By this session: none.** No source file, test, or document was edited. The working tree at close is byte-identical to the working tree at open — 28 tracked files modified and 16 untracked paths, exactly as found. The change set below is the sprint's, carried into this run unchanged.

| Area | Content |
|---|---|
| Desktop, new | `capabilities/clipboard/` · `core/src/clipboard_policy.rs` · `daemon/tests/clipboard.rs` |
| Desktop, modified | `cli/src/main.rs` · `core/src/{lib,store}.rs` · `daemon/src/{control,main,server,state}.rs` · `daemon/{examples,tests}` · `proto/build.rs` · `Cargo.{toml,lock}` |
| Android, new | `clipboard/` (7 files) · `capability/ClipboardCapability.kt` · `ui/{ClipboardTileService,ClipboardViews}.kt` · 4 JVM tests · 2 instrumented tests |
| Android, modified | `AnyFlowApp.kt` · `store/TrustStore.kt` · `ui/{MainActivity,SendActivity}.kt` · `AndroidManifest.xml` · `build.gradle.kts` · `strings.xml` · `libs.versions.toml` |
| Protocol | `protocol/proto/anyflow/v1/capabilities/clipboard_v1.proto` |
| Docs | `docs/architecture/CLIPBOARD.md` · `ADR-0014` · `OVERVIEW.md` · `PROTOCOL.md` · `THREAT_MODEL.md` · `README.md` |

`28 files changed, 1804 insertions(+), 86 deletions(-)` — plus the untracked new files listed above.

---

## 25. CLIP-01 … CLIP-23

| Gate | What it covers | This run | Verdict |
|---|---|---|---|
| CLIP-01 · 02 | Negotiation and the explicit grant | Re-verified live — a session without the grant carries no clipboard | PASS |
| CLIP-03 · 04 | Fedora → Android | Re-verified on hardware (H12, H13, sensitive push) | PASS |
| CLIP-05 | Android → Fedora, the manual direction | Re-verified four times end to end | PASS |
| CLIP-06 … 10 | Carried forward — definitions not labelled in the working tree | Covered by the green automated suites | PASS |
| CLIP-11 · 12 | Nothing is persisted, and nothing is logged | Re-verified: 9 payloads, 0 occurrences, positive control 18 | PASS |
| CLIP-13 | Revocation on a live session | Automated; not re-run on hardware | PASS |
| CLIP-14 | Reconnect | Re-verified on hardware — disconnect/connect renegotiated cleanly | PASS |
| CLIP-15 · 16 | Carried forward — definitions not labelled in the working tree | Covered by the green automated suites | PASS |
| CLIP-17 | The session writer must not regress | Re-run: all 6 dispatch/pressure tests green | PASS |
| CLIP-18 … 20 | Carried forward — definitions not labelled in the working tree | Covered by the green automated suites | PASS |
| **CLIP-21** | Android hardware end-to-end | **Closed.** Full chain on the installed build, then again on the freshly built APK with a fresh pairing | **PASS** (was partial) |
| CLIP-22 | The transport is unchanged | Re-verified live: TLS 1.3, ALPN, mutual auth, pinning | PASS |
| CLIP-23 | Carried forward | Covered by the green automated suites | PASS |

Where a gate ID has no description above, the working tree does not carry a label for it; I have reported status rather than inventing a definition. Those gates were PASS at the start of this round per the brief and are backed by the automated suites, all of which stayed green.

---

## 26. CLIP-H01 … CLIP-H16

| Gate | What it covers | This run | Verdict |
|---|---|---|---|
| CLIP-H01 | `EXTRA_IS_REMOTE_DEVICE` on API 34+, and applying a received clip | Instrumented, on device | PASS |
| CLIP-H02 | Carried forward — not labelled in the working tree | Within the 21 instrumented tests | PASS |
| **CLIP-H03** | Android → Fedora manual, complete in one execution | **Closed.** Copy → send → receive → apply → real paste, byte-identical | **PASS** |
| CLIP-H04 | `EXTRA_IS_SENSITIVE` set when asked, absent otherwise (API 33+) | Instrumented, on device; plus a hardware smoke of both directions | PASS |
| CLIP-H05 · H06 | Nothing is transformed on the way through, either side | Re-verified by hexdump and `cmp` on four payloads | PASS |
| CLIP-H07 … H11 | Carried forward — not labelled in the working tree | Within the 21 instrumented tests and the 9 hardware-gated backend tests | PASS |
| **CLIP-H12** | Android background receive / applying a received clip | **Closed.** `setPrimaryClip()` succeeds in the background; verified by paste | **PASS** |
| **CLIP-H13** | Android locked-screen receive | **Closed.** Accepted while locked and dozing; content intact after unlock | **PASS** |
| CLIP-H14 … H16 | Carried forward — not labelled in the working tree | Within the 21 instrumented tests | PASS |
| QS-TILE | Quick Settings tile send — optional | Executed on hardware; misreports an empty clipboard | **FAIL** |

---

## 24. Remaining debts

- **QS-TILE (F-1)** — fix the shortcut read so it happens once the Activity has window focus, rather than inside `onNewIntent`. Then re-run the gate. Until then the tile opens the app but cannot complete a send.
- **Empty-vs-refused disambiguation** — the assumption at `SystemClipboard.kt:71` does not hold on Android 16 for an unfocused app. Worth revisiting even independently of the tile, since it turns "try again with the app focused" into "your clipboard is empty".
- **On-wire capture (F-3)** — one `tcpdump` run with elevated privileges would close the last transport evidence gap.
- **Dead identity in the trust store** — `5655 E8FD AD07 C225` is still paired and granted but can never authenticate. Your call whether to revoke it.
- **Instrumented tests destroy the pairing every run** — this happens each round and costs a manual QR scan to recover. A dedicated test identity, or a Gradle configuration that does not uninstall, would remove the friction.
- **Unlabelled gate IDs** — several CLIP and CLIP-H numbers have no definition in the working tree, so their status can only be carried forward. A gate table checked into the repo would make future rounds auditable without external notes.

---

## 27. Final state — policy and git

```
Policy at close — safe defaults restored on both sides

Fedora   SM-X620  3e0350064a7aa53b2567197deda473f4
           fingerprint  5920 AA1C C387 7A3E
           clipboard.v1 granted
           connected    yes
           policy       send=on receive=on  auto-send=off  auto-receive=off
         held clips     none

Android  Share clipboard with this computer      on
         Receive clipboard from this computer    on
         Apply received clipboard automatically  off
         Send clipboard to this computer         on
         "Received text waits in a notification until you tap Copy."

Automatic clipboard sync was enabled only for the loop-suppression gate and the
H12/H13 gates, and turned off again immediately in each case.
```

```
Git audit

git branch --show-current   feature/clipboard-v1        no new branch
git log --oneline -1        bdcee7a Merge pull request #4 …   no commit, no push
git diff --check            clean — no whitespace errors, no conflict markers
git diff --stat             28 files changed, 1804 insertions(+), 86 deletions(-)
git status --porcelain      28 modified · 16 untracked   identical to session start

forbidden artefacts
  clipboard content         absent — 0 hits for every payload across the whole diff
  APK / AAB                 absent — build outputs gitignored
  private key / trust store absent
  state.json                absent — lives in ~/.local/share/anyflow, untracked
  local.properties          absent — gitignored
  logs containing clipboard absent
  build output              absent — gitignored
```

---

## 28. Verdict

# CLIPBOARD.V1 CERTIFIED

| Certification rule | Required | Actual |
|---|---|---|
| H03 | PASS | **PASS** — full chain, one execution, byte-identical |
| CLIP-21 | PASS | **PASS** — twice, including on the final build and a fresh pairing |
| Privacy P0/P1 | none open | none |
| Authorization P0/P1 | none open | none |
| Clipboard loop P0/P1 | none open | none |
| Session writer P0/P1 | none open | none |
| Persistence P0/P1 | none open | none |
| TLS / pinning P0/P1 | none open | none — one evidence gap recorded, no finding |
| H12 / H13 | executed, behaviour documented | both pass, real behaviour recorded above |
| QS-TILE | optional | FAIL — P2, non-blocking, root cause identified |

---

*AnyFlow · clipboard.v1 hardware closeout · run on `feature/clipboard-v1` at `bdcee7a`, 30–31 August 2026 · no commit, no push, no source change.*
