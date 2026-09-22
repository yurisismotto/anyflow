# OmniBridge — Release Readiness v1, Android↔guest peer-gate closure

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-peer-gates-closure-v1` |
| **Baseline** | `9cca549` (merge of PR #61, the lifecycle closure this phase continues) |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, GNOME 50.5 Wayland, libvirt `qemu:///system` via the `libvirt` group |
| **Guests** | `anyflow-u2404` Ubuntu 24.04.4 LTS · `anyflow-u2604` Ubuntu 26.04.1 LTS · `anyflow-d13` Debian 13 trixie — **one at a time**, macvtap on `enp0s13f0u2u2c2` |
| **Physical Android** | **SM-X620, Android 16**, `192.168.68.63/22`. Its pairing with the host `fedora` daemon was preserved throughout; nothing was reset, no app data was cleared. |
| **Artifacts under test** | the same CI build as [lifecycle closure](RELEASE-LIFECYCLE-CLOSURE-V1.md) — run [`35747130364`](https://github.com/yurisismotto/omnibridge/actions/runs/35747130364), `SHA256SUMS` verified on the host **and again inside each guest** |
| **Verdict** | **PEER GATES CLOSED ON ALL THREE DISTRIBUTIONS — 23 of 26 lifecycle gates now certified everywhere · 2 N/A · L14 PARTIAL on Debian 13 only · finding F-2 reproduced and evidenced** |

---

## 0. Executive summary

Lifecycle closure left the four peer gates — L12, L14, L15, L16 — measured on
Debian 13 and **not executed** on either Ubuntu. This phase ran them on both
Ubuntu guests, and then re-ran Debian 13, because fixing the harness invalidated
what Debian's numbers had meant.

**The first Ubuntu run produced eight failures. None of them was a product
defect.** Every one came from the harness tearing down, hiding or mis-selecting
the thing it was about to measure:

| What the run reported | What was actually true |
| --- | --- |
| `L15: no incoming-file prompt appeared on the phone` | the harness had `am force-stop`ed the app, which dropped the TLS session; `omnibridge send` answered *"protocol violation: that device is not connected"* |
| `L16: the phone claims no source role` | `com.android.shell` **cannot** be chosen in OmniBridge's app picker, so no notification source was ever selected |
| `L16: the mirrored count did not increase` | the same force-stops had **snoozed** Android's notification listener, which then received nothing |
| `L14: no clipboard activity on the phone` | the clip arrived correctly; the check grepped logcat for a string the app does not log |

Ten harness defects were found and fixed. They are §5, and they are the
substance of this phase as much as the gate results are.

**After the fixes**, against the same packages:

| Distribution | Result |
| --- | --- |
| Ubuntu 24.04.4 LTS | **67 passed, 0 failed, 2 n/a** |
| Ubuntu 26.04.1 LTS | **68 passed, 0 failed, 2 n/a** |
| Debian 13 trixie | **62 passed, 1 failed, 4 n/a** — the one failure *is* finding F-2 |

---

## 1. Method

Unchanged from lifecycle closure: `virsh qemu-agent-command` for root inside the
guest, macvtap in bridge mode for a real LAN, GDM autologin for a real graphical
session, packages delivered over virtio-serial and digest-verified inside the
guest. `packaging/tests/lifecycle-peer-gates.sh` drives everything except the
camera.

One operator action per newly paired guest: the harness renders the guest's own
pairing payload as a QR on the host screen, opens the tablet's scanner over adb
(verified via `topResumedActivity`), and stops. Ubuntu 24.04 paired after 505 s.
Ubuntu 26.04's first window **expired unscanned at 900 s** — the harness marked
nothing passing and exited 4, which is the correct behaviour and is recorded
here rather than hidden; a second window at 1800 s was scanned. Debian 13 was
already paired from the previous phase, so it needed no scan.

### 1.1 The notification fixture is now a precondition

`android/fixture` exists precisely because `cmd notification post` posts as
`com.android.shell`, which has **no launcher entry** — and OmniBridge's picker
offers apps a person can open from their home screen, plus apps that happen to
be notifying right now. The fixture's own README says so, and the harness had
been ignoring it. L16 now aborts unless
`io.github.yurisismotto.omnibridge.fixture` is installed and holds
`POST_NOTIFICATIONS`, because a gate that cannot select a source cannot measure
mirroring.

---

## 2. The peers, by identity

Every capability result below is attributed to one of these, by device id and
fingerprint, asserted on both sides before the gate ran.

| Guest | Device id | Fingerprint | LAN | systemd | GNOME |
| --- | --- | --- | --- | --- | --- |
| `anyflow-u2404` | `464d605c5d27908bc0397f4ba4b6b1bd` | `70E3 FD25 1DE6 E2DF` | 192.168.68.75 | 255.4-1ubuntu8.17 | 46.0 |
| `anyflow-u2604` | `87afd85000a03a3af0c3e6a112ad939e` | `6170 B697 373A 672C` | 192.168.68.78 | 259.5-0ubuntu3.4 | 50.1 |
| `anyflow-d13` | `dd32ae815d71c69b0eab2aff48759eda` | `A06B 75F3 C3EB 6DF7` | 192.168.68.59 | 257.13-1~deb13u1 | 48.7 |

The Android peer is `SM-X620`, device id `a8c964c4d1d6076187e57c3455ddd8b5`,
fingerprint `509B D0C1 CE97 C909` — asserted `platform=android` and asserted
**distinct from each guest's own fingerprint**, on every run.

The tablet ends this phase paired with four desktops: the three guests and the
**host `fedora` daemon at `149F 6B66 AB5D 8526`, which was present before the
phase, was asserted still present after every pairing, and still holds its own
capability grants.**

---

## 3. Gate matrix

| Gate | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 | |
| --- | :-: | :-: | :-: | --- |
| **L12** Android discovery | ✅ | ✅ | ✅ | §3.1 |
| **L14** clipboard | ✅ | ✅ | ⚠ | §3.2 — PARTIAL on Debian, finding F-2 |
| **L15** files | ✅ | ✅ | ✅ | §3.3 — guest → phone; the other direction needs a human |
| **L16** notifications | ✅ | ✅ | ✅ | §3.4 — mirroring, content-bound; privacy half → R2 |

### 3.1 L12 — Android physical-device discovery: **CERTIFIED on all three**

The Android app lists *paired* desktops, so "the desktop appears" only means
something against a baseline. On each guest:

```
ok  L12: the phone does NOT list 'anyflow-u2604' yet — the baseline is clean
ok  L12: the tablet's existing pairing with the host 'fedora' daemon is present before this run
…
ok  L12: the packaged desktop 'anyflow-u2604' now appears on the phone, and did not before
ok  L12: the tablet's pairing with the host 'fedora' daemon survived pairing with the guest
ok  the phone shows this guest's fingerprint (6170 B697), so the selected peer is the guest under test
```

Both halves hold everywhere: the packaged desktop is discovered, and the
existing pairing is untouched.

### 3.2 L14 — clipboard

**The `--sensitive` half passes on all three.** Ubuntu and Debian ship
wl-clipboard 2.2.1, which has no `--sensitive`; the daemon says
`sensitive clipboard unavailable` and **refuses** a clip Android marked
sensitive rather than writing it unmarked. That is U-1 behaving correctly.

**The transfer half separates the distributions, and the cause is measurable.**

| | Ubuntu 24.04 / 26.04 | Debian 13 |
| --- | --- | --- |
| `watch:` | **XFIXES on the Xwayland CLIPBOARD selection** | unavailable — *"the Xwayland fallback is not usable either (cannot connect to the X display: Connection refused (os error 111))"* |
| `auto-send` | **supported on this session** | NOT supported here |
| `omnibridge clipboard send` | **exit 0** | **exit 1**, clipboard timeout |

On both Ubuntu guests the gate is **CERTIFIED**, two-sided and bound to the
run's own sentinel:

```console
$ omnibridge clipboard send a8c964c4d1d6076187e57c3455ddd8b5
sent 27 bytes of clipboard text to 509B D0C1 CE97 C909

# guest journal, inside a window opened before the send
omnibridge_capability_clipboard: clipboard update sent peer=509B D0C1 CE97 C909 event=0ac30384 bytes=27 sensitive=false

# the tablet's own screen, on a phone that provably had no such card a moment earlier
Clipboard from anyflow-u2404
27 bytes            [Copy]  [Dismiss]
```

27 is the length of this run's `OBCLIP-…` sentinel. The receipt card is
dismissed and asserted **absent** before the send, because the app updates one
notification in place rather than posting a new one — a "the shade gained a
notification" test fails on a send that worked.

**phone → guest is NOT EXERCISED on any distribution, and that is the harness's
limit rather than the product's.** This Android build has no `cmd clipboard`
implementation, so nothing can put a sentinel on the tablet's clipboard from
adb. Driven anyway, the product answers correctly:

> There is nothing to send. Copy some text, then come back.

**On Debian 13 the transfer half is N/A and carries finding F-2** — §4.

### 3.3 L15 — files: **CERTIFIED guest → phone on all three**

Two-sided, with independent sentinels in the filename and the body, and bound
to one transfer id inside a window opened before the send:

```console
# guest journal — Debian 13
omnibridge_capability_files: offering a file transfer=1c6beaf6 peer=509B D0C1 CE97 C909 size=47
omnibridge_capability_files: sent; awaiting the receiver's verdict transfer=1c6beaf6 bytes=47
omnibridge_capability_files: the peer confirmed it stored the file transfer=1c6beaf6

# the tablet's Files tab
OBNAME-6CBIOYOANBPCBV3K22OQ.txt
From anyflow-d13 · 47 B
Received
```

The Accept tap is bound to **this run's filename**, not to the first
`Incoming file` row on screen: a stale offer from an earlier run sits in the
same list with its own Accept, and accepting that one would complete a transfer
this run never made.

**This supersedes the previous phase's result on Debian 13**, where the phone
never marked the file `Received` because the harness looked for the prompt on
the wrong screen. Both Ubuntu guests reach the same state.

**phone → guest remains NOT EXECUTED**, unchanged and for the unchanged reason:
adb cannot hand the app a readable URI — a file staged by the shell uid is
refused with *"that file could not be read"* even with
`--grant-read-uri-permission` — and the app's own document picker needs a human.
**The gate's `0600` assertion therefore still has nothing to measure** and is
not claimed.

### 3.4 L16 — notifications: **CERTIFIED for mirroring on all three, and now content-bound**

Role convergence first, then a baseline, then exactly one notification:

```
ok  L16: the phone announces a notification source role
ok  L16: baseline taken after clearing the fixture's own notifications -- 0 mirrored
ok  L16: a D-Bus monitor on org.freedesktop.Notifications.Notify is running before the post
ok  L16: the fixture's notification is on the phone's shade
ok  L16: exactly one notification was mirrored to the packaged desktop (0 -> 1)
ok  L16: the desktop's Notify call carries BOTH this run's sentinels
```

The count alone is not enough and was not trusted. `mirrored now` is a count of
what is *currently* mirrored, not a running total — an older entry expiring as a
new one arrives leaves it unchanged, which is exactly how the previous phase
recorded `3 → 3` on Debian 13 and still called mirroring certified. Two things
fix it: the fixture's own notifications are cleared first so the expected delta
is exactly **one**, and a `dbus-monitor` capture opened **before** the post and
closed after shows what the desktop was actually asked to display:

```
method call … org.freedesktop.Notifications; member=Notify
   string "OBNOTIF-2NUYDZMEQ7L2IOO5NTDQ"
   string "OBNBODY-L5UMUPTXDAGSDL3CIC2EUE5R"
```

Both strings are this run's sentinels. That is what turns "something was
mirrored" into "the notification the phone posted is the one the desktop
showed".

**The privacy half is still NOT claimed here.** The daemon logs nothing for a
mirrored notification at its default level, so the journal covering the
operation holds one line, and grepping one line for a sentinel is a vacuous
pass. It remains **R2's**, at `TRACE`, with sentinels — unchanged from the
previous phase's assignment.

---

## 4. Finding F-2, reproduced with proof

The previous phase recorded F-2 and carried it to R5 without measuring it
closely. It is now measured, and **both** of its sentences are false on this
session. The capture is quoted in full below; every line of it is a command run
on `anyflow-d13` within one minute, on one unlocked session.

What the product tells the user:

```
detail     wl-clipboard; watch: unavailable (the compositor does not implement the
           wlr/ext data-control protocol, and the Xwayland fallback is not usable
           either (cannot connect to the X display: Connection refused (os error
           111)). Clipboard auto-send cannot run; manual send still works.)
```

What manual send does:

```console
$ omnibridge clipboard send a8c964c4d1d6076187e57c3455ddd8b5
error: the clipboard did not respond in time. On GNOME Wayland this normally
means the session is locked: wl-copy and wl-paste cannot obtain a seat behind
the lock screen.
rc=1
```

What the session was doing, read before **and** after the failed send:

```console
$ loginctl show-session 2 -p Type -p Active -p LockedHint
Type=wayland
Active=yes
LockedHint=no
$ pgrep -a Xwayland
1435 /usr/bin/Xwayland :0 -rootless -noreset …
```

So, with a `wl-copy` provably owning the selection and the session provably
unlocked and active:

1. **"manual send still works" is false.** It fails, with a timeout.
2. **"this normally means the session is locked" is false here.** The session is
   not locked, Xwayland is running, and lock state has nothing to do with the
   failure.

The real cause is visible in the daemon's own `detail` line: `omnibridge
clipboard send` must *read* the current selection, and on a compositor with no
data-control protocol and an unreachable Xwayland it cannot. Manual send depends
on the same read path as auto-send, so the moment auto-send is unavailable for
that reason, manual send is too — and the message says the opposite.

Neither is a security issue and neither loses data: the operation **fails
closed**, exit 1, nothing wrong is sent. **F-2 is a diagnostics defect**, and
the honest classification of "is this a product fix?" belongs to **R5**, which
now has this evidence instead of a single observation.

A second, related observation, recorded because it was measured: on Debian 13
the daemon had started **before** GNOME Shell exported the display variables —
`/proc/886/environ` carried no `WAYLAND_DISPLAY`, and `clipboard status` then
said *"no graphical session found"*. Restarting the daemon inside the live
session gave it the variables and moved it to the F-2 state above. Whether
`omnibridged.service` should order itself after `graphical-session.target` is a
**packaging question for R5**, not a peer-gate result, and it is recorded here
rather than acted on.

---

## 5. Harness defects found by running it

The previous phase found nine. This one found **ten more**, every one in the
harness rather than the product. Seven of the ten produced a **false FAIL** —
the mirror image of a vacuous PASS, and just as invalid.

| # | Defect | How it presented |
| --- | --- | --- |
| 10 | `am force-stop` before L15 **dropped the TLS session** | `omnibridge send` → *"protocol violation: that device is not connected"*; four L15 checks failed against a connection the harness had killed |
| 11 | the same force-stops **snoozed Android's notification listener** | a snoozed listener receives nothing; L16 mirrored 0 and reported it as a product failure |
| 12 | the notification source was `com.android.shell`, which **the app picker cannot offer** | it has no launcher entry — the exact ordering problem `android/fixture` was built to solve; the choice silently never happened |
| 13 | the picker was searched for `shell`, and the miss was **one failed check, not a stop** | L16 then ran anyway against a peer announcing no source role, turning one harness miss into three more failures |
| 14 | `find 'No app chosen'` accepted **any** already-chosen app | Debian 13 read "1 of 99 apps" from an earlier wave; the chosen app was not the fixture, so the fixture's notification was shared with nobody |
| 15 | the notification row's status has **three spellings** — `Off`, `No apps chosen`, `N of M apps` | matching one gave `n/a` on a configured peer and a hard stop on a fresh one |
| 16 | `toggle-after 'Choose apps' "$FIXTURE_PKG"` matched the **search box**, whose text *is* the package name | a search field has no checkbox on its row, so the run stopped saying the picker does not offer the fixture — with the fixture listed directly below |
| 17 | the L14 "Send clipboard" sheet was **left open**, covering the Files tab | L15's prompt was drawn behind it and reported as never appearing |
| 18 | `find '^Files$' \| tail -1` picked a **permission row**, not the navigation tab | `find` prints one line, so `tail -1` did nothing; L15 searched the wrong screen. Fixed with a new `phone-ui.py nav` that requires a whole navigation row |
| 19 | `grep 'session established' \| tail -1` characterised the live session from an **ended** one | it matched the `capabilities=["battery.v1"]` line the phone opens *before* the grants are made |

Defect 19 is the previous phase's capture-window flaw in a third place, and its
fix is a different shape worth naming: a `--since` window goes **empty**
whenever the session was already up, and an empty capture is not evidence
either. The line is bound to the live session instead — of every session event
in the journal, the newest must be this peer's establishment.

Defects 10 and 11 share one principle, and it generalises the previous phase's:

> **Evidence collection must bracket the operation — and so must the state the
> operation needs.** A harness that tears down the session, the binding or the
> screen it is about to measure produces failures that say nothing about the
> product.

This, and defect 14's *"a count is not a confirmation"*, are the main inputs
this phase gives **R4**.

---

## 6. Lifecycle gate count after this closure

| | Before (PR #61) | After |
| --- | --- | --- |
| CERTIFIED on all three distributions | 20 | **23** |
| N/A with a stated reason | 2 (L13, L17) | 2 (L13, L17) |
| PARTIAL | 4 peer gates, one distribution only | **1** — L14 on Debian 13 |

L12, L15 and L16 move from "partially closed on one distribution" to certified
on Ubuntu 24.04, Ubuntu 26.04 and Debian 13. L14 is certified on both Ubuntu
guests and PARTIAL on Debian 13, where the compositor cannot serve the read and
the product's own message about it is wrong (§4).

---

## 7. What remains

| Item | Phase |
| --- | --- |
| **L16's privacy half** — `journalctl`/`logcat` sentinels at `TRACE` | **R2**, unchanged |
| **Finding F-2** — now evidenced; classify and decide whether truthful messaging needs a product fix | **R5** |
| **`omnibridged.service` ordering against `graphical-session.target`** — observed on Debian 13 (§4) | **R5** |
| **The nineteen harness defects**, nine from the previous phase and ten from this one | **R4** |
| **L15 phone → guest** and its `0600` assertion | needs the app's document picker, i.e. a human |
| **L14 phone → guest** | needs a sentinel on the Android clipboard, i.e. a human |

---

## 8. Product state, untouched

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json
```

Byte-identical to the values recorded before Packaging v1 Phase 1 and again at
lifecycle closure. The tablet's app data was never cleared, and the `fedora`
pairing at `149F 6B66 AB5D 8526` is listed with its grants at the end of this
phase exactly as at the start.

One phone-side setting was changed and restored: the approved-listener list is
read before `cmd notification disallow_listener` / `allow_listener` (which is
how a snoozed listener is rebound) and asserted **byte-identical** afterwards.
It names two listeners belonging to other apps; a run that widened or narrowed
it would be altering the device under certification.

---

## 9. Verdict

# PEER GATES CLOSED — 23 OF 26 LIFECYCLE GATES CERTIFIED ON ALL THREE DISTRIBUTIONS

**What closed.** L12, L15 and L16 are certified on Ubuntu 24.04, Ubuntu 26.04
and Debian 13, against CI-published packages on real installed desktops with a
real second device. L14 is certified on both Ubuntu guests. Discovery is bound
to a fingerprint, file transfer to a transfer id inside a bracketed window, and
notification mirroring to the sentinels in the desktop's own `Notify` call.

**What this phase was really for.** The first Ubuntu run failed eight checks and
not one of them was the product. A harness that force-stops the app it is
measuring will drop the session, snooze the listener and hide the prompt — and
then report three gates as broken. The rule the previous phase applied to
passes applies symmetrically: **a FAIL measured against a precondition the
harness destroyed is as invalid as a PASS measured against nothing.**

**What is not closed, and is not claimed.** L14's transfer half is N/A on
Debian 13's compositor, and finding **F-2** — the status line that promises
manual send works, and the error that blames a lock screen that is not
engaged — is reproduced with lock state read on both sides of the failure. L16's
privacy half is still R2's. Both phone → guest directions need a human at a
picker, and are recorded as not executed rather than passed.
