# OmniBridge — Release Readiness v1, security evidence closure

| Field | Value |
| --- | --- |
| **Branch** | `feature/security-evidence-closure-v1` |
| **Baseline** | `7c9c6da` (merge of PR #62, the peer-gate closure this phase stands on) |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, GNOME 50.5 Wayland |
| **Guests** | `anyflow-u2404` Ubuntu 24.04.4 LTS · `anyflow-d13` Debian 13 trixie — one at a time, macvtap on the wired NIC |
| **Physical Android** | **SM-X620, Android 16**, `192.168.68.63/22`, fingerprint `509B D0C1 CE97 C909` |
| **Harness** | `packaging/tests/security-log-evidence.sh` (new) |
| **Verdict** | **SEC-LOG-03 CLOSED on real hardware · L16's privacy half CLOSED · no advisory outstanding · one harness defect found that could have passed a real leak** |

---

## 0. Executive summary

Three things were open. All three are closed, and a fourth was found on the way.

| | Result |
| --- | --- |
| **SEC-LOG-03's deferred half** — `journalctl` and `logcat` after a real transfer | **CLOSED** on Ubuntu 24.04 and Debian 13, at `TRACE` |
| **L16's privacy half**, deferred here by lifecycle closure | **CLOSED** on both, at `TRACE`, with the content proved to have arrived |
| **Dependency advisories and the TLS regression** | `cargo audit` **0 vulnerabilities** / 282 crates; the three `sec_tls_01_*` tests pass; **1030 workspace tests pass, 0 fail** |
| **Found here: the matcher itself could lose a match** | `grep -q` behind a pipe under `pipefail` missed a present string in **225 of 300 runs**. On a privacy gate that is a **PASS on a real leak.** Fixed in five harnesses and gated in CI. |

---

## 1. What was actually open, read from the source documents

Security Certification v1 §1 records SEC-LOG-03 as **PASS WITH FINDING**, closed
in-process by `daemon/tests/file_log_privacy.rs`. **That test is not reopened
and its verdict stands.** Its §13 item 7 deferred one thing to "the Phase 6
lifecycle certification, which drives the capabilities on real hardware" — and
L15 was one of the eleven lifecycle gates that never ran, so the deferral had
no destination.

The Release Readiness baseline §6.3 states the gap exactly:

| Layer | Before this phase | Now |
| --- | --- | --- |
| `tracing` events, in-process, `TRACE` | **closed** — `daemon/tests/file_log_privacy.rs` | unchanged |
| `journalctl --user -u omnibridged` after a real transfer | **open** | **closed** — §3 |
| Android `logcat` after a real transfer | **open** | **closed** — §3 |
| A real file crossing the wire between two real devices | **open** — gate L15 | **closed** by PR #62 |

---

## 2. Method

### 2.1 `TRACE`, because `journalctl` at `info` proves less

Security Certification v1 §7's method is to capture *every* `tracing` event at
`TRACE`, "which is strictly more than `journalctl` ever shows". The same
standard is applied here to the real journal: a drop-in sets
`Environment=RUST_LOG=trace`, the daemon is restarted, and **both** the
environment and its effect are asserted — `RUST_LOG=trace` in
`/proc/<pid>/environ`, *and* TRACE-level lines actually appearing. A filter
that parsed but did not apply would give a quieter journal and a clean sentinel
search for the wrong reason.

The drop-in is removed on every exit path, including an abort, and the removal
is verified.

### 2.2 A cursor, not a timestamp

The window is a **journal cursor** taken before the operation and passed to
`--after-cursor`. `journalctl` parses `--since` in the guest's *local* time
while the harness reads the clock with `date -u`; the two agree only while the
guest happens to be on UTC, and both of these are. A cursor carries no timezone
and cannot drift.

### 2.3 Two sentinels, asserted in opposite directions — with one correction

Baseline §6.4 sets the rule: the filename **present**, because that is finding
F-1 and its presence proves the capture is real and covers this operation; the
content **absent**, which is the gate.

**The correction, measured rather than assumed.** Both filename log sites in
`capabilities/files/src/lib.rs` — `:916` *"incoming file offer"* and `:1707`
*"received, verified and stored"* — are on the **receive** path. The desktop is
the *sender* in the only direction that can be driven without a human at a
document picker, so no filename reaches the guest journal at all, and a harness
asserting one does would fail on a correct product. Each side therefore anchors
on something the **product** wrote about **this** operation:

| Side | Non-vacuity anchor | The gate |
| --- | --- | --- |
| guest journal | the transfer id the daemon generated, plus the byte count | content absent |
| Android logcat | the filename, under the app's own `FileTransfer` tag | content absent |

**F-1 has an Android half, and it was not previously recorded.** The app logs
`FileTransfer: incoming <name>` and `received and verified <id> as <name>` —
the same metadata choice the desktop makes on its own receive path. It is
useful here as the anchor; it also means D-4's decision is about two
implementations, not one.

---

## 3. SEC-LOG-03 — the two open rows

Run on both distributions, one at a time, against the packages CI published.

```
ok  SEC-LOG-03: the source file exists, is 48 B, and provably carries the content sentinel
ok  SEC-LOG-03: journal cursor taken BEFORE the transfer
ok  SEC-LOG-03: logcat cleared before the transfer
ok  SEC-LOG-03: the transfer completed — id 833929a9, 48 B, to SM-X620
ok  SEC-LOG-03: the journal capture holds 66 line(s), 44 of them TRACE
ok  SEC-LOG-03: the capture carries unrelated daemon activity (mDNS/listener/session), as a real log does
ok  SEC-LOG-03: the capture names transfer=833929a9 — the window provably covers this operation
ok  SEC-LOG-03: the capture carries this file's byte count (48)
ok  SEC-LOG-03: the content sentinel is ABSENT from 66 journal lines at TRACE
ok  SEC-LOG-03: a 24-byte prefix of the content is ABSENT too
ok  SEC-LOG-03: logcat holds 1196 line(s) since it was cleared before the transfer
ok  SEC-LOG-03: logcat carries this transfer's filename under the app's FileTransfer tag
ok  SEC-LOG-03: the content sentinel is ABSENT from 1196 logcat lines
ok  SEC-LOG-03: a 24-byte prefix of the content is ABSENT from logcat too
```

| | Ubuntu 24.04 | Debian 13 |
| --- | --- | --- |
| transfer | `833929a9`, 48 B, completed | `900052bb`, 48 B, completed |
| journal capture | 66 lines, 44 TRACE | 75 lines, 55 TRACE |
| logcat capture | 1196 lines | 1196 lines |
| **content in either** | **no** | **no** |

Both runs: **35 and 36 checks passed, 0 failed, 0 n/a.**

The 24-byte prefix is searched separately, mirroring the in-process canary: a
truncated rendering that logged only the head of a payload would defeat a
whole-string search and not this one.

---

## 4. L16's privacy half

Lifecycle closure recorded this `n/a` rather than passing it, for a reason it
stated: at the packaged level the daemon logs nothing for a mirrored
notification, so the window held **one line**, and grepping one line for a
sentinel is a vacuous pass. At `TRACE` it is not.

The hard part is not the search. It is proving the content was **there to be
found**, because "the sentinel is absent from the journal" is equally true of a
notification that never arrived. Three things establish it:

1. the mirrored count moves by **exactly one**, from a baseline taken after
   clearing the fixture's own notifications;
2. a `dbus-monitor` capture opened **before** the post shows the desktop's own
   `org.freedesktop.Notifications.Notify` call carrying **both** sentinels —
   the content demonstrably reached this machine;
3. the notifications capability's own record of the mirroring is in the window.

```
ok  L16: exactly one notification was mirrored (0 -> 1)
ok  L16: the desktop's Notify call carries BOTH sentinels — they provably reached this machine
ok  L16: the journal capture holds 74 line(s), 65 of them TRACE
ok  L16: the capture carries the notifications capability's own record of this mirroring:
        DEBUG omnibridge_capability_notifications: notification upsert
              peer=509B D0C1 CE97 C909 notification=a5c0edcd outcome="displayed"
ok  L16/SEC-LOG-02: neither the title nor the body sentinel appears in 74 journal lines at TRACE
```

That one capability line is the whole contract, visible: the peer by
fingerprint, the notification by a **redacted id prefix**, the outcome — and no
title and no body. It is what `redact::id_prefix` exists for.

### 4.1 The sentinel that logcat *does* carry, and why it is not a leak

`adbd` writes the command line it was asked to run into logcat, so driving the
fixture with `--es title '<sentinel>'` puts the sentinel there by itself:

```
I adbd : adbd service requested 'shell,v2,raw:am start -n …/.FixtureActivity
         --es op post --es id 92 --es tag seclog --es title 'OBNTITLE-…' --es body 'OBNBODY-…''
```

A harness that grepped logcat for the sentinel would fail a correct product on
its own command echo. The assertion is therefore that **every** occurrence is
on an `adbd` line, and it fails if any other process logged one:

```
ok  L16: the 1 logcat line(s) carrying a sentinel are all adbd echoing this harness's
         own 'am start'; no OmniBridge process logged either
```

The fixture's own line is `op=post id=92 tag=seclog channel=omnibridge-fixture`
— id and tag, never the title or the body, exactly as `android/fixture`
promises. It doubles as the proof that the window covers the post.

---

## 5. The harness defect this phase found

The first run of the new harness reported

```
not ok  SEC-LOG-03: the capture carries no unrelated daemon activity
PRECONDITION FAILED: no journal line names transfer=abdee1d5
```

against a capture that plainly contained both. Re-running the same assertions
over the same saved capture sometimes passed and sometimes failed.

**Cause.** `grep -q` exits the instant it matches. The producer on the left of
the pipe is then killed by `SIGPIPE`, exit 141 — and under `set -o pipefail`,
which every harness here sets, the **pipeline** reports failure although the
pattern was found. Whether the producer finishes first is a scheduling race.

**Measured**, 300 iterations against a 33 KB journal capture, searching for a
string it contains:

| Form | Matches missed, out of 300 |
| --- | --- |
| `printf '%s' "$var" \| grep -q PATTERN` | **225** |
| `grep -q PATTERN <<<"$var"` | 0 |
| `grep -q PATTERN FILE` | 0 |

**Why it matters more than flakiness.** A privacy gate is written the unsafe
way round:

```sh
if grep -qF "$SENTINEL" <<<"$capture"; then notok "leaked"; else ok "absent"; fi
```

A lost match is a **PASS on a real leak** — the exact failure this whole wave
exists to stop, arriving through the *matcher* rather than through the
measurement. It is the mirror of the capture-window flaws: there the window did
not cover the operation; here the search did not cover the capture.

**Fixed** in all five harnesses — 53 sites converted to a here-string or to a
file, none of which is a pipeline and none of which can lose a match.

**Gated**, so it cannot come back: `packaging-checks.sh` H1 fails on any
host-side `| grep -q` in `packaging/tests/*.sh`, and CI runs it on every pull
request. Guest-side pipes inside a quoted `gx`/`gu`/`ga_wait_for` command are
exempt with a reason — they run under the guest's `/bin/sh -c`, which does not
set `pipefail`, so the pipeline status is grep's own.

The check was itself verified against a planted violation:

```
FAIL  H1: host-side pipe into grep -q in install-smoke.sh: 450:echo planted | grep -q planted
```

**No result recorded before this phase is invalidated.** The race can only turn
a *match* into a *miss*, so it can only weaken a positive assertion or falsely
pass a negative one — and the one negative assertion that had run, L16's
privacy half, was recorded `n/a` rather than passed. Every PASS on record is a
match that did occur.

---

## 6. Dependency advisories and the TLS regression

### 6.1 `cargo audit`

```console
$ cargo audit          # desktop/, against the committed Cargo.lock
    Loaded 1264 security advisories (from ~/.cargo/advisory-db)
    Scanning Cargo.lock for vulnerabilities (282 crate dependencies)
```

**0 vulnerabilities, 0 warnings, exit 0.** `rustls` is **0.23.45** in the
lockfile — past **RUSTSEC-2026-0285** (*"TLS 1.3 handshake messages incorrectly
accepted across encryption level boundaries"*), the advisory Security
Certification v1 found and PR-fixed. **No new High or Critical advisory
appeared, so no fix branch was needed and R2 was not interrupted.**

### 6.2 `cargo deny`

**Not run: not installed and not configured.** There is no `deny.toml` in the
repository. This is **debt D-8** in the Release Readiness baseline — *"cargo
deny not configured"* — recorded there as non-blocking and gated on a **licence
policy decision** that has not been made. Inventing a policy inside a
certification commit would be the wrong order; it is carried to R5 as the
baseline already has it.

### 6.3 The TLS regression, and the workspace

```
test sec_tls_01_a_raw_tls12_client_hello_is_refused ... ok
test sec_tls_01_control_a_tls13_client_is_accepted ... ok
test sec_tls_01_tls12_is_not_compiled_in_and_both_configs_pin_tls13 ... ok
```

All three hold: TLS 1.2 is not compiled in, both configurations pin TLS 1.3, a
raw TLS 1.2 `ClientHello` is refused, and — the control that stops the first
three passing on a server that refuses everything — a TLS 1.3 client is
accepted.

```console
$ cargo test --workspace --locked
1030 passed; 0 failed; 24 ignored
```

The 24 ignored are `#[ignore]` by construction: they need a real clipboard, a
real session bus, a real UPower or a real lock source, and they are the ones
the hardware certifications drive instead. None is skipped silently — each is
named in the run log.

---

## 7. Scope, stated rather than implied

| Claim | Basis |
| --- | --- |
| SEC-LOG-03, journal and logcat | measured on **two** distributions, real hardware, real transfer, at `TRACE` |
| L16 privacy | measured on the same two, with the content proved to have arrived |
| Direction | **guest → phone only.** phone → guest needs the app's document picker, i.e. a human, and is recorded as not executed by PR #62 |
| Fedora 44 | not covered here; the daemon's logging is source-determined and the two Debian-family runs exercise the same code, but that is an argument and not a measurement |
| `cargo deny` | not run — debt D-8, a licence-policy decision |

---

## 8. Product state, untouched

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json
```

Byte-identical to Packaging v1 Phase 1, to lifecycle closure and to peer-gate
closure. The tablet's `fedora` pairing was never touched. **No product code
changed in this phase** — the only source edits are to harnesses.

The `RUST_LOG=trace` drop-in is guest configuration, installed per run and
removed on every exit path including an abort; the removal is verified and
printed.

---

## 9. Verdict

# SECURITY EVIDENCE CLOSED — SEC-LOG-03 AND L16 PRIVACY MEASURED ON REAL HARDWARE, NO ADVISORY OUTSTANDING

**What closed.** SEC-LOG-03's two deferred rows are measured on Ubuntu 24.04
and Debian 13: a real 48-byte file crosses to a real phone, the capture is
bounded by a journal cursor taken before the send, it is proved to cover the
operation by the transfer id the daemon itself generated and by the filename
the app itself logged, and the content sentinel and its 24-byte prefix appear
in neither the journal at `TRACE` nor 1196 lines of logcat. L16's privacy half
is closed the same way, with the `Notify` call proving the title and body
reached the desktop before their absence from the journal is claimed.

**What this phase found.** The matcher could lose a match. `grep -q` behind a
pipe under `pipefail` missed a present string in 225 of 300 runs, and on a gate
written "absent is a pass" that is a pass on a real leak. Fifty-three sites
across five harnesses are fixed, and CI now fails on the pattern.

**No security release blocker remains.** No High or Critical advisory is
outstanding, the TLS 1.3 pinning that RUSTSEC-2026-0285 threatened is asserted
three ways plus a control, and the whole workspace is green. The two items
carried forward — **D-4**, the filename in logs, now known to have an Android
half as well, and **D-8**, `cargo deny`'s absent licence policy — are decisions
for R5, not defects.
