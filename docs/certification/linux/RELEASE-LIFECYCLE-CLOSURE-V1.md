# OmniBridge — Release Readiness v1, Linux lifecycle closure

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-lifecycle-closure-v1` |
| **Baseline** | `a73509a` (merge of PR #60, the systemd user-unit fix this phase produced) |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, GNOME 50.5 Wayland, libvirt `qemu:///system` via the `libvirt` group |
| **Guests** | `anyflow-u2404` Ubuntu 24.04.4 LTS · `anyflow-u2604` Ubuntu 26.04.1 LTS · `anyflow-d13` Debian 13 trixie — one at a time, macvtap on the wired NIC |
| **Physical Android** | **SM-X620, Android 16**, `192.168.68.63/22`. Its pairing with the host `fedora` daemon was preserved throughout; nothing was reset. |
| **Artifacts under test** | CI run [`35747130364`](https://github.com/yurisismotto/omnibridge/actions/runs/35747130364), rebuilt from `a73509a` — 14 artifacts, `SHA256SUMS` verified on the host **and again inside each guest** |
| **Verdict** | **LIFECYCLE CLOSURE: 20 of 26 gates CERTIFIED ON ALL THREE DISTRIBUTIONS · 2 N/A · 4 peer gates partially closed on one distribution · 1 finding · 1 release-blocking defect found and fixed** |

---

## 0. Executive summary

Packaging v1 could not run eleven lifecycle gates. The
[baseline audit](../../audits/release/RELEASE-READINESS-V1-BASELINE.md) found
that the stated cause — *"bridged or macvtap networking needs root"* — was not
true, and that three LAN-capable guests were already built and powered off.
One Ethernet cable later, all of them were reachable.

**Running them found a release-blocking defect on the first gate.**

| | |
| --- | --- |
| **What** | `omnibridged.service` could not start **at all** on Ubuntu 24.04 or Ubuntu 26.04 — `status=218/CAPABILITIES` before `ExecStart`, retried forever by `Restart=on-failure`, no control socket, nothing listening |
| **Cause** | `ProtectKernelModules=true` implies `CapabilityBoundingSet=~CAP_SYS_MODULE`, which a *user* manager can only apply from inside an unprivileged user namespace |
| **Fixed** | PR #60, on its own branch, before certification continued — [`SYSTEMD-USER-UNIT-CAPABILITIES-FIX`](../../reports/linux/SYSTEMD-USER-UNIT-CAPABILITIES-FIX.md) |

That is the whole argument for this phase. Eleven gates were blocked, and the
first one that ran found the packages were unusable on half the supported
distributions.

**After the fix**, against the rebuilt artifacts:

| Distribution | Result |
| --- | --- |
| Ubuntu 24.04.4 LTS | **102 passed, 0 failed, 2 n/a** |
| Ubuntu 26.04.1 LTS | **102 passed, 0 failed, 2 n/a** |
| Debian 13 trixie | **102 passed, 0 failed, 2 n/a** |

---

## 1. Method

**Root inside a guest replaces root on the host**, which this environment does
not grant.

| Need | How |
| --- | --- |
| root on an installed desktop | `virsh qemu-agent-command … guest-exec` over virtio-serial. No IP path, so it cannot influence discovery, pairing or anything under certification. |
| a real LAN | **macvtap in bridge mode** on `enp0s13f0u2u2c2`. Each guest gets its own MAC and DHCPs from the LAN's own server — `192.168.68.75`, `.59` — on the same `/22` as the phone. Host↔guest does not loop back, which is expected and was asserted. |
| a graphical session | GDM autologin for `anyflow`; `loginctl` reports `Type=wayland`, `seat0`. |
| eyes on a screen | `virsh screenshot` against the guest's real framebuffer. |
| the packages | delivered over virtio-serial and digest-verified **inside** the guest, so what was installed is provably what CI published. |

`packaging/tests/lifecycle-gates.sh` (L1–L11, L13, L17–L26) and
`packaging/tests/lifecycle-peer-gates.sh` (L12, L14, L15, L16).

### 1.1 Guest configuration this phase changed, and why

Recorded rather than hidden. All of it is guest-side; none is product code.

| Change | Reason | Undo |
| --- | --- | --- |
| GDM autologin enabled on `anyflow-u2604` and `anyflow-d13` | both sat at a greeter with **no graphical session**, so every session gate would have measured nothing. The originals are saved as `custom.conf.pre-omnibridge-r1` / `daemon.conf.pre-omnibridge-r1`. | restore the saved file, restart the display manager |
| `linger` turned off around L3/L19, restored after | a lingering user manager survives a logout, so `terminate-user` would not stop the daemon and both gates would pass without measuring a logout | the harness restores it and asserts it did |
| capability grants for the guest on the tablet | pairing grants nothing; the session negotiates the **intersection** of both ends' grants | `omnibridge unpair`, or the tablet's per-peer screen |

---

## 2. Gate matrix

**CERTIFIED** means measured on that distribution, on a real installed
desktop, against the CI-published package.

| Gate | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 | Note |
| --- | :-: | :-: | :-: | --- |
| **L1** clean install | ✅ | ✅ | ✅ | exit 0, no scriptlet error, both packages `ii` |
| **L2** installed files manifest | ✅ | ✅ | ✅ | 7 core + 6 gui files; no path under a home; no file owned twice; **the installed unit carries ≥15 hardening directives and `ProtectSystem=strict`** |
| **L3** autostart after login | ✅ | ✅ | ✅ | new pid after a real session cycle |
| **L4** runs as the user, not root | ✅ | ✅ | ✅ | |
| **L5** runtime dir / socket | ✅ | ✅ | ✅ | `0700` dir, `0600` socket, user-owned |
| **L6** GUI launches from the menu | ✅ | ✅ | ✅ | `desktop-file-validate` clean; icon resolves to a real SVG; framebuffer **changed** after `gtk-launch` |
| **L7** D-Bus cold activation | ✅ | ✅ | ✅ | no GUI running; `Peer.Ping` → `()`; the bus started the process |
| **L8** activation in the SAME live session | ✅ | ✅ | ✅ | the name is activatable **with no logout** — the self-heal, on a packaged install |
| **L9** tray integration | ✅ | ✅ | ✅ | tray host present → exactly 1 `StatusNotifierItem`; **no package owns a shell extension** |
| **L10** mDNS advertisement | ✅ | ✅ | ✅ | journal + UDP 5353 bound; corroborated from the host's own resolver |
| **L11** TCP 55432 / firewall | ✅ | ✅ | ✅ | listening; **reachable from the phone** (`OPEN`, 0% loss both ways) |
| **L12** Android discovery | — | — | ✅ | §4 |
| **L13** trust preserved across upgrade | n/a | n/a | n/a | one published build per Debian target; nothing to upgrade from |
| **L14** clipboard smoke | — | — | ⚠ | §4 — PARTIAL by the product's own contract |
| **L15** files smoke | — | — | ⚠ | §4 — one direction certified |
| **L16** notifications smoke | — | — | ⚠ | §4 — mirroring certified |
| **L17** package upgrade | n/a | n/a | n/a | as L13 |
| **L18** daemon restart | ✅ | ✅ | ✅ | socket recreated `0600`; exactly one process |
| **L19** logout / login | ✅ | ✅ | ✅ | new session id, new user manager, cold activation still works |
| **L20** reboot | ✅ | ✅ | ✅ | **boot_id changed**; L3, L5, L7, L10, L11 all hold afterwards |
| **L21** package remove | ✅ | ✅ | ✅ | |
| **L22** package reinstall | ✅ | ✅ | ✅ | the daemon finds the **same identity key** |
| **L23** user state preserved | ✅ | ✅ | ✅ | digest, mode and owner identical across remove, reinstall and purge |
| **L24** purge semantics | ✅ | ✅ | ✅ | `apt purge` leaves the trust store untouched; no conffile left |
| **L25** no orphaned files | ✅ | ✅ | ✅ | six named paths gone |
| **L26** no root-owned user state | ✅ | ✅ | ✅ | |

**20 gates CERTIFIED on all three distributions. 2 N/A with a stated reason.
4 peer gates partially closed on one distribution.**

Against the baseline's list of eleven blocked gates — L1, L2, L3, L6, L8, L9,
L14, L15, L16, L19, L20 — **eight are closed on every distribution**, and the
remaining three are §4.

---

## 3. The defect this phase found

Full detail in
[`SYSTEMD-USER-UNIT-CAPABILITIES-FIX.md`](../../reports/linux/SYSTEMD-USER-UNIT-CAPABILITIES-FIX.md).
Summarised because it is the reason the phase existed:

```console
$ systemctl --user status omnibridged.service          # Ubuntu 24.04, shipped 0.1.0-1
Active: activating (auto-restart) (Result: exit-code)
Process: 3727 ExecStart=/usr/bin/omnibridged (code=exited, status=218/CAPABILITIES)
(ibridged)[3727]: omnibridged.service: Failed to drop capabilities: Operation not permitted
$ ls /run/user/1000/omnibridge
ls: cannot access …: No such file or directory
```

| Distribution | systemd | Shipped unit |
| --- | --- | --- |
| Fedora 44 | `259.9-1.fc44` | starts |
| Debian 13 | `257.13-1~deb13u1` | starts |
| **Ubuntu 24.04** | `255.4-1ubuntu8.17` | **FAILS** |
| **Ubuntu 26.04** | `259.5-0ubuntu3.4` | **FAILS** |

Not a version boundary; not the capability sets, which are byte-identical on
all four. `prctl(PR_CAPBSET_DROP, CAP_SYS_MODULE)` is `EPERM` for uid 1000
everywhere, including where the unit starts — so systemd applies the
restriction from inside an unprivileged user namespace where it can, and
Ubuntu restricts those.

The directive was removed **with an argument, not for convenience**: the three
module syscalls are absent from the `SystemCallFilter=@system-service`
allow-list, an unprivileged process has `CapEff=0`, and `NoNewPrivileges=true`
stops the bounding set growing across an `execve`. The honest cost is
recorded: `systemd-analyze security` moves 4.7 OK → 5.0 MEDIUM, because it
scores a directive's presence rather than its effect.

---

## 4. The peer gates — L12, L14, L15, L16

These need a second real device. They were run on **Debian 13**, with the
SM-X620 paired to the guest by a QR scan performed by the operator.

### 4.1 L12 — Android physical-device discovery: **CERTIFIED**

The Android app lists **paired** desktops; it has no browse list of unpaired
ones, because pairing is by QR and the payload carries the address. So "the
desktop appears" is asserted across the pairing boundary, and the baseline is
what makes it mean anything:

```
ok  L12: the phone does NOT list 'anyflow-d13' yet — the baseline is clean
ok  L12: the tablet's existing pairing with the host 'fedora' daemon is present before this run
…
ok  L12: the packaged desktop 'anyflow-d13' now appears on the phone, and did not before
ok  L12: the tablet's pairing with the host 'fedora' daemon survived pairing with the guest
ok  the phone shows this guest's fingerprint (A06B 75F3), so the selected peer is the guest under test
```

Peer binding, because the tablet ends this wave paired with several desktops:
exactly one peer paired, `platform=android`, fingerprint `509B D0C1 CE97 C909`
distinct from the guest's own `A06B 75F3 C3EB 6DF7`.

**Both halves of the gate hold** — the desktop appears, and the existing
pairing is untouched.

### 4.2 L15 — files: **CERTIFIED guest → phone**

Two-sided, with independent sentinels in the filename and the body:

```console
# guest journal
omnibridge_capability_files: offering a file transfer=6ffc5e1f peer=509B D0C1 CE97 C909 size=32
omnibridge_capability_files: sent; awaiting the receiver's verdict transfer=6ffc5e1f bytes=32
omnibridge_capability_files: the peer confirmed it stored the file transfer=6ffc5e1f

# guest CLI
6ffc5e1f  sending -> SM-X620   file OBNAME-HLFUEHXJIUKLL6Y.txt   state completed

# the tablet's own Files tab
OBNAME-HLFUEHXJIUKLL6Y.txt
From anyflow-d13 · 32 B
Received
```

The incoming-file prompt on the tablet **named the guest's fingerprint**,
which is what binds the transfer to the machine under test.

A declined transfer is in the same record, deliberately: the first attempt
shows `CANCELLED (DECLINED_BY_USER)` because nobody accepted it. The approval
is real and the product is right to require it.

**phone → guest: NOT EXECUTED.** adb cannot hand the app a readable URI — a
file staged by the shell uid is refused with *"that file could not be read"*
even with `--grant-read-uri-permission`, because it belongs to another uid.
Reaching it means the app's own document picker, which is a human choosing a
file. **The gate's `0600` assertion therefore has nothing to measure on this
distribution**, and is not claimed.

### 4.3 L16 — notifications: **CERTIFIED for mirroring**

Role convergence first, because mirroring with a peer that claims no source
role would post into a void:

```
roles: this desktop announced 2 (epoch 1); the device can source notifications (epoch 3)
mirrored now  3
showing 3 of 3 mirrored
```

**The privacy half is deliberately NOT claimed here.** The daemon logs nothing
for a mirrored notification at its default level, so the journal covering the
operation held **one line** — and grepping one line for a sentinel is a
vacuous pass, which this wave exists to stop. It is recorded `n/a` with that
reason and belongs to **R2**, which takes it at `TRACE` with sentinels.

### 4.4 L14 — clipboard: **PARTIAL, with one finding**

The `--sensitive` half passes and is the important one:

```
ok  L14: wl-copy has no --sensitive (wl-clipboard 2.2.1) and status says 'unavailable'
```

That is U-1 behaving correctly: Ubuntu and Debian ship wl-clipboard 2.2.1, a
clip Android marks sensitive is **refused rather than written unmarked**, and
`clipboard status` says so before the first password fails to arrive.

The transfer half is **N/A on this compositor**. GNOME implements neither
`wlr-data-control` nor `ext-data-control`, so no client can read a selection it
does not own, and the daemon reports this itself
(`auto-send NOT supported here`). `omnibridge clipboard send` fails **clearly**
rather than sending something wrong:

```
error: the clipboard did not respond in time. On GNOME Wayland this normally
means the session is locked: wl-copy and wl-paste cannot obtain a seat behind
the lock screen.
```

> ### Finding F-2 — the status line overpromises
>
> `clipboard status` says *"Clipboard auto-send cannot run; **manual send still
> works**"*. On this session manual send did **not** work, and the session was
> **not locked** — `loginctl show-session … LockedHint` was `no` throughout,
> and turning the screen lock off changed nothing.
>
> Two things are therefore inaccurate for a user on GNOME: the claim that
> manual send still works, and the error's guess that a lock is the usual
> cause. Neither is a security issue and neither loses data — the operation
> fails closed. It is recorded, not fixed, because changing product messaging
> in the middle of certifying it is the wrong order. **Carried to R5 as a
> non-blocking debt.**

### 4.5 Ubuntu 24.04 and Ubuntu 26.04: **NOT EXECUTED**

Neither PASS nor FAIL. §7 carries the exact steps.

---

## 5. Harness defects found by running it

Packaging v1 named seven cases where a harness reported success while
measuring nothing. This phase found **nine more**, every one in the harness
rather than the product, and each is fixed with the reason in the code:

| # | Defect | How it presented |
| --- | --- | --- |
| 1 | `dpkg-query -f "${Status}"` crossed a `sh -c` in the guest, where the unescaped `${Status}` expanded to empty | the installed-package count silently became **0** |
| 2 | the S3 journal grep tailed `-n 200` of a **persistent** journal | it matched a *previous* session's failures and reported them as this run's |
| 3 | a backslash-continued `gdbus` command did not survive `sh -c` | three activation gates failed on `sh: 1: call: not found` — a typo, not D-Bus |
| 4 | `omnibridge pair` backgrounded with **no stdin** read EOF from its `[y/N]` prompt | it **declined** the pairing silently; two operator scans were lost before one journal line explained it |
| 5 | `am start` cannot open `PairingCaptureActivity` — it is `exported="false"`, correctly | adb was refused and the harness ignored it, so nothing was ever scanning |
| 6 | the image viewer runs as a D-Bus service, so a second QR did not replace the first | an operator scanned a **stale QR** from an earlier attempt |
| 7 | `wl-copy` never exits — it owns the selection — and `wl-copy --clear` blocks the same way | the run stalled, last line printed looking like a pass |
| 8 | `GA_EXEC_TIMEOUT` was tightened with `${VAR:-180}` *after* the library had applied its 900s default | the tightening never happened; a hang ran for fifteen minutes |
| 9 | the L15 journal check tailed files-capability lines unbounded | it matched a **previous** run's successful transfer and reported it as this one's |

Defects 2 and 9 are the same flaw in two places: **a capture window that does
not correspond to the operation under test.** Both are now bound — S3 to the
unit's `InvocationID`, L15 to that transfer's id inside a window opened before
the send. This is the single most valuable input to **R4**.

---

## 6. An investigation that did not replicate, recorded because it was reported

Mid-phase this document would have carried a second release-blocking product
defect: `omnibridged` appeared to stop publishing its IPv4 address record
permanently after an interface went down and came back, which would have
broken discovery after any Wi-Fi drop, dock or suspend.

**It does not reproduce.** The measurements behind it were **single**
`avahi-resolve` calls, and avahi negative-caches on the measuring host after a
responder is briefly unreachable — so a lone resolve fails for reasons that
have nothing to do with the daemon. Restarting the daemon "fixed" it by
forcing a fresh announcement that overwrote the cache entry, which is exactly
what a real fix would look like.

With a repeatable probe (8–20 samples, 15–20 s apart) against the **unfixed**
binary:

| Scenario | Result |
| --- | --- |
| steady state | **4/4** resolved |
| after `ip link down 6s; up`, past the record TTL | **8/8** resolved |
| daemon **started** while the link was down, then link up | **8/8** resolved |
| after a flap, probed every 15 s for five minutes | **20/20** resolved |

Three independent negatives. **No product change was shipped**; the branch was
deleted and the working tree returned to `develop`. A proposed watcher is kept
at `scratchpad/mdns-investigation/proposed-watcher.patch` and is not proposed.

Upstream `mdns-sd` 0.21.4 was investigated first, as instructed, and **not
adopted**: its `#507` fix is for IPv6 privacy-address rotation, and measured in
the same guest it was *worse* in the flap case.

The rule this wave applies to its tests applies to its findings too: **a FAIL
on insufficient evidence is as invalid as a PASS on none.**

---

## 7. What remains, exactly

### 7.1 Ubuntu 24.04 and Ubuntu 26.04 peer gates

Per guest, one at a time:

```console
$ virsh -c qemu:///system start anyflow-u2404          # or anyflow-u2604
$ # install the CI packages and start the daemon, then:
$ ./packaging/tests/lifecycle-peer-gates.sh \
      --domain anyflow-u2404 --distro ubuntu2404 \
      --evidence <dir> --phone-ip 192.168.68.63 --pair-ttl 900
```

**One operator action each:** the harness opens a QR on the host screen and
opens the tablet's scanner (verified via `topResumedActivity`); the operator
points the tablet at the QR. Everything else — capability grants on both ends,
the notification opt-in, Connect, and the gates — is driven from the harness.

Expected outcome, from Debian 13: **L12 CERTIFIED, L15 CERTIFIED one
direction, L16 CERTIFIED for mirroring, L14 PARTIAL.** Both Ubuntu guests run
GNOME with the same wl-clipboard 2.2.1, so L14 will classify identically.

### 7.2 Carried to later phases

| Item | Phase |
| --- | --- |
| **L16's privacy half** — `journalctl`/`logcat` sentinels for notification and file content | **R2** |
| **Finding F-2** — `clipboard status` claims manual send works where it does not | **R5** as a non-blocking debt |
| **The nine harness defects**, especially the two capture-window flaws | **R4** |
| **L15 phone → guest** and its `0600` assertion | needs the app's document picker, i.e. a human |

---

## 8. Product state, untouched

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json
```

Byte-identical to the values recorded before Packaging v1 Phase 1. The host's
`fedora` pairing with the SM-X620 was never reset, and the tablet's app data
was never cleared. The guests' own trust stores are separate files.

---

## 9. Verdict

# LIFECYCLE CLOSURE: D-1 SUBSTANTIALLY CLOSED — 20 OF 26 GATES CERTIFIED ON ALL THREE DISTRIBUTIONS

**What closed.** Eight of the eleven gates Packaging v1 could not run — clean
install, manifest, autostart after login, launcher, activation into a live
session, tray, logout/login and reboot — are certified on Ubuntu 24.04,
Ubuntu 26.04 and Debian 13, against the packages CI published, on real
installed desktops with real graphical sessions. So are the twelve that had
only container or Fedora-host evidence. **L10 is no longer PARTIAL**: the
advertisement is measured from three packaged installs on the physical LAN.
**L12 is no longer PARTIAL** on Debian 13: the phone discovers a *packaged*
desktop, and its existing pairing survives.

**What this phase was really for.** The first gate that ran found the packages
could not start on either Ubuntu. Eleven blocked gates had been hiding a
defect that made half the supported distributions unusable, and no amount of
green packaging gates would have found it.

**What is not closed, and is not claimed.** The peer gates on the two Ubuntu
guests were not executed — §7.1 has the exact command and the single operator
action each needs. L14's transfer half is N/A on GNOME by the product's own
contract, with finding **F-2** recorded against its status message. L15's
phone → guest direction and its `0600` assertion need a human at a file
picker. L16's privacy half is deferred to R2 rather than passed on a
one-line journal.

**Three distributions that were "build-supported" are now runtime-certified
for everything a desktop does without a second device.** The honest word for
the remainder is in §7, gate by gate.
