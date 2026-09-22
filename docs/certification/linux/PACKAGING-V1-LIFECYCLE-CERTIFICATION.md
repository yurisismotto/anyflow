# OmniBridge — Packaging v1, Linux lifecycle certification

| Field | Value |
| --- | --- |
| **Branch** | `feature/linux-packaging-certification-v1` |
| **Baseline** | `1bfd431` (merge of PR #55, Security Certification v1) |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, systemd 259.9, GNOME 50.5 Wayland, dbus-broker 37, podman 5.x |
| **Physical Android** | **SM-X620, Android 16**, `192.168.68.63/22`, paired throughout |
| **Verdict** | **PARTIAL — 13 of 26 gates certified, 13 BLOCKED. This is NOT a full lifecycle certification and Packaging v1 must not be called runtime-certified on the strength of it.** |

Claims are **MEASURED** (a command was run here and its output is quoted) or
**BLOCKED** (the gate could not be run, with the specific reason).

---

## 0. Executive summary — read this first

**This certification is incomplete, and the missing half is the half that
needs a graphical desktop.**

Thirteen gates were genuinely measured and pass. Thirteen could not be run at
all. Nothing was inferred, approximated or marked as passing on the strength
of a related measurement.

Two blockers, both environmental and neither fixable in scope:

| # | Blocker | What it costs |
| --- | --- | --- |
| **B-1** | **No root.** `sudo` on this host requires a password. Every gate that begins "install the package on a real system" cannot start. | L1–L3, L6, L8, L9, L14–L16, L19, L20 as *packaged-system* gates |
| **B-2** | **No VM can reach the LAN.** KVM is available and `qemu:///session` works — but user-session QEMU uses SLIRP, which **accepts no inbound connections and does not carry mDNS multicast**. A VM built without root can never be discovered by the phone. Bridged or macvtap networking needs root. | The whole per-distribution runtime matrix, and L10/L12/L13 inside any VM |

> **⚠ B-2's stated cause is SUPERSEDED — 2026-09-22, `feature/release-readiness-v1`.**
> *"Bridged or macvtap networking needs root"* is **not true on this host and was
> not true when this was written.** The invoking user is in the **`libvirt`**
> group, so `virsh -c qemu:///system` succeeds with no `sudo`, no password and
> no polkit prompt — the privileged half is done by `virtqemud`, not by the
> caller. [`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`](../../audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md)
> §4.10 measured this on 2026-09-15, and three guests with macvtap over the
> wired NIC were built on the strength of it. The real blocker is that
> `enp0s13f0u2u2c2` has **no carrier**. See
> [`RELEASE-READINESS-V1-BASELINE.md`](../../audits/release/RELEASE-READINESS-V1-BASELINE.md)
> §3. **Everything B-2 says about SLIRP, and every gate verdict in this
> document, stands unchanged.**

B-2 is worth stating precisely because it is not a matter of effort. The four
installation ISOs are staged on this host (`Fedora-KDE-Desktop-Live-44`,
`ubuntu-24.04.4-desktop`, `ubuntu-26.04.1-desktop`, `debian-13.7.0-netinst`)
and automated installs could be written. It would not help: the resulting VMs
could not participate in LAN discovery, so L10, L12, L13 and everything that
depends on a paired phone would still be unreachable.

A third limit is not environmental but human: **L6** ("click the entry; the
window opens with the OmniBridge icon, not a grey square") and **L9** (tray:
"one correct icon, 3 menu items") are visual confirmations. No automation
substitutes for somebody looking at the screen.

### What the phone proves, and what it does not

The SM-X620 was attached and paired for the whole session and its **pairing,
trust and grants were preserved** — no reset, no re-pair. **MEASURED** at the
end of this phase, against the digests recorded before Phase 1 began:

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd…ca21  identity.key     (identical to the pre-Phase-1 value)
a539ea91…e6ce  state.json       (identical to the pre-Phase-1 value)

$ omnibridge status
  paired      1 device(s)
    SM-X620  a8c964c4d1d6076187e57c3455ddd8b5
       fingerprint 509B D0C1 CE97 C909
       paired      yes
```

That survived six phases, a daemon stopped and restarted three times, two
systemd unit-gate runs and a D-Bus activation test. It certifies **L13's
preservation half on this host**. It does **not** certify L12 on a packaged
install, because the daemon it is paired with is the development build.

---

## 1. Gate matrix

| Gate | Verdict | Where |
| --- | --- | --- |
| **L1** clean install | **PASS (container)** | §2 — real `dnf`/`apt`, four package sets |
| **L2** installed files manifest | **PASS** | §2 — matches audit §12 exactly; no path under `$HOME` |
| L3 daemon autostart after login | **BLOCKED** | B-1 + needs a login cycle |
| **L4** runs as the user, not root | **PASS** | §3 |
| **L5** control socket runtime directory | **PASS** | §3 — gates S1/S2/S3, 21/21 |
| L6 GUI launches from the application menu | **BLOCKED** | B-1 + visual confirmation |
| **L7** D-Bus cold activation | **PASS** | §4 — measured on the live session bus |
| L8 activation immediately after install into a live session | **BLOCKED** | B-1. The *mechanism* is proved (§4.2); the packaged case is not. |
| L9 tray integration | **BLOCKED** | B-1 + visual confirmation |
| **L10** mDNS discovery | **PARTIAL** | §5 — advertised and measured; the phone's view is the dev daemon |
| **L11** TCP 55432 / firewall | **PASS** | §5 — external scan from the phone |
| **L12** Android physical discovery | **PARTIAL** | §5 — paired and connected, against the dev daemon |
| **L13** pairing/trust preserved across upgrade | **PASS** | §6 — byte-identical across a real `dnf upgrade` |
| L14 clipboard smoke | **BLOCKED** | B-1 |
| L15 files smoke | **BLOCKED** | B-1 |
| L16 notifications smoke | **BLOCKED** | B-1 |
| **L17** package upgrade | **PASS** | §6 — 0.1.0-2 → 0.1.0-3 |
| **L18** daemon restart | **PASS** | §3 — socket recreated at 0600 |
| L19 logout / login | **BLOCKED** | needs a session cycle on the certifying host |
| L20 reboot | **BLOCKED** | needs a reboot of the certifying host |
| **L21** package remove | **PASS** | §7 |
| **L22** package reinstall | **PASS** | §7 |
| **L23** user state preserved across L17/L21/L22 | **PASS** | §7 |
| **L24** purge semantics (DEB) | **PASS** | §7 — all three Debian targets |
| **L25** no orphaned package-owned files | **PASS** | §7 |
| **L26** no root-owned OmniBridge user state | **PASS** | §7 |

**13 PASS · 2 PARTIAL · 11 BLOCKED.**

### Per-distribution applicability

| Gate group | Fedora 44 | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 |
| --- | --- | --- | --- | --- |
| L1, L2, L21–L23, L25, L26 | ✔ container | ✔ container | ✔ container | ✔ container |
| L24 (purge) | — RPM has no purge | ✔ | ✔ | ✔ |
| L17, L13 (upgrade) | ✔ | ✖ only one build exists | ✖ | ✖ |
| L4, L5, L7, L11, L18 | ✔ real host | ✖ | ✖ | ✖ |
| L3, L6, L8, L9, L14–L16, L19, L20 | ✖ | ✖ | ✖ | ✖ |

**No distribution has a complete column. None is runtime-certified.** Fedora
44 is the only row with host-level evidence at all, and even it is missing
every graphical gate.

---

## 2. L1, L2 — install and manifest

**MEASURED**, `packaging/tests/install-smoke.sh`, real package managers in
disposable containers that had never seen OmniBridge:

| Target | Packages | Result |
| --- | --- | --- |
| Fedora 44 | `omnibridge-0.1.0-3`, `omnibridge-gui-0.1.0-3` | **31 passed, 0 failed** |
| Debian 13 trixie | `omnibridge_0.1.0-1`, `omnibridge-gui_0.1.0-1` | **27 passed, 0 failed** |
| Ubuntu 24.04 LTS | same | **27 passed, 0 failed** |
| Ubuntu 26.04 LTS | same | **27 passed, 0 failed** |

Every set installed with exit 0 and **no scriptlet error**. The manifests match
audit §12.1/§12.2 exactly — nine files and six — and **no package owns a path
under a home directory**, asserted rather than eyeballed.

Also asserted on the installed system, not merely in the spec:

```
ok    P5: the D-Bus Exec is the absolute installed path
ok    the D-Bus Exec target exists and is executable
ok    the installed unit carries the S1/S2 directives and keeps ProtectSystem=strict
ok    R7: the unit is installed disabled
ok    installing the package started no daemon
ok    R10: no docs/ tree installed (0 file(s) under doc/)
```

**A container is not a desktop.** It has no compositor, no session bus, no
logind session and no tray host. These results certify the *package
lifecycle* and say nothing about whether OmniBridge works on a desktop.

---

## 3. L4, L5, L18 — the daemon and its runtime directory

**MEASURED** on this host, with the daemon running:

```console
$ ps -eo user,pid,cmd | grep [o]mnibridged
yuri  691427  ./target/release/omnibridged --log info
```

**L4 PASS** — not root, and structurally cannot be: the unit is a
`systemd --user` unit, the package starts nothing, and no maintainer script
can reach a user's service manager.

**L5 and L18** are certified by `packaging/tests/systemd-unit-gates.sh`, re-run
for this phase against the real unit with a throwaway `XDG_DATA_HOME`:
**21 passed, 0 failed.**

```
ok    S2: /run/user/1000/omnibridge exists, mode 0700, owned by yuri
ok    S2: control.sock mode 0600, owned by yuri
ok    the daemon runs as yuri, not root
ok    NoNewPrivs is 1 on the running process
ok    restart: the socket is recreated at 0600          <- L18
ok    restart: the unit is active again                 <- L18
ok    S3: no ProtectSystem, ReadWritePaths, namespace or seccomp denial
ok    systemd removed the runtime directory with the unit
ok    /home/yuri/.local/share/omnibridge is byte- and mode-identical to before the run
```

The last line matters: the gate run does not touch the real trust store, and
proves it rather than promising it.

---

## 4. L7 — D-Bus cold activation

### 4.1 The measurement

**MEASURED** on the real `dbus-broker` session bus, with **no GUI process
running** — the cold case:

```console
$ pgrep -a omnibridge-gui
(none)
$ gdbus call --session --dest io.github.yurisismotto.omnibridge \
      --object-path /io/github/yurisismotto/omnibridge \
      --method org.freedesktop.DBus.Peer.Ping
()
$ pgrep -a omnibridge-gui
688950 …/omnibridge-gui --gapplication-service
```

The bus **started the process** on demand and the call succeeded. The process
then **exited on idle**, which is the documented lifecycle — the GUI is not
resident and `omnibridged` remains the only long-lived process.

`Peer.Ping` was used rather than `ActivateAction quick-panel` deliberately: it
forces activation without opening a window on the operator's desktop. What L7
is really asking — *can the bus start the name from cold* — is exactly what
`Ping` answers, and `ServiceUnknown` is the failure it is guarding against.

### 4.2 A real defect this found, in the environment rather than the product

The **first** attempt failed:

```
Erro: GDBus.Error:org.freedesktop.DBus.Error.NameHasNoOwner:
  Could not activate remote peer 'io.github.yurisismotto.omnibridge': unit failed
```

The host's *development* activation entry named
`/home/yuri/.local/bin/omnibridge-gui`, **which does not exist** — a stale
`~/.local` install whose binary had gone. That is precisely the failure mode
L7 exists to catch, and precisely why `install-smoke.sh` asserts *"the D-Bus
Exec target exists and is executable"* on every packaged install rather than
only checking that the path is absolute.

The measurement above was then taken with an entry pointing at the real built
binary. **The operator's original file was restored byte-identically
afterwards** and verified with `diff`.

### 4.3 What is still BLOCKED

**L8** — activation immediately after a package install into a live session —
needs the package installed as root. The *mechanism* is certified elsewhere:
`PACKAGING-V1-DBUS-ACTIVATION.md` measured `HealedByReload` on this same live
`dbus-broker` session. What is uncertified is the packaged case end to end.

---

## 5. L10, L11, L12 — network and the phone

**L11 PASS.** From the SM-X620, a genuine second machine on the same `/22`:
of nine TCP ports this host listens on, three are LAN-reachable, and
**OmniBridge contributes exactly one: 55432**. A sweep of 21/22/23/25/80/139/
443/445/3389/8080/8443 found none open. Firewalld's `FedoraWorkstation` zone
already permits the flows, so **no firewall change was needed** — which
confirms readiness-audit §4.6 and is why the packaged firewalld service is
shipped-but-never-enabled. Full detail in
[`SECURITY-CERTIFICATION-V1.md`](../security/SECURITY-CERTIFICATION-V1.md) §2.

**L10 PARTIAL.** The daemon advertises `_omnibridge._tcp.local.` on UDP 5353
over IPv4 and IPv6 (MEASURED in the journal), and the phone reaches it. But
the daemon the phone sees is the development build, not a packaged install.

**L12 PARTIAL**, for the same reason. The phone is paired, its record is in
the trust store, and it connects — but against the development daemon.

Making either of these full requires a packaged install (B-1). Doing it in a
VM requires bridged networking (B-2).

---

## 6. L13, L17 — upgrade

**MEASURED** — a real `dnf upgrade` from the previous release to the current
one, with a trust store planted beforehand as a non-root user:

```
Upgrade (gate L17, L13)
  ok    L17: trust-store fixture planted before the upgrade
  ok    L17: the older build installed (0.1.0-2.fc44)
  ok    L17: upgraded to 0.1.0-3.fc44 (exit 0)
  ok    L17: no scriptlet error during the upgrade
  ok    L13/L17: the trust store is byte- and mode-identical across the upgrade
  ok    L17: the upgrade started no daemon
```

The last line is audit §4.9 asserted rather than assumed: **an upgrade does
not restart a running user daemon**, because a root scriptlet has no route to
a user's service manager. That is inherent to user units, it is documented in
`packaging/common/README.md`, and it is now measured.

**Fedora only.** The Debian and Ubuntu packages exist in one version
(`0.1.0-1`), so there is no upgrade path to exercise. Not a failure — there is
simply nothing to upgrade from yet.

### 6.1 A silent skip this gate caught in its own harness

The first run of L17 reported **`25 passed, 0 failed` with the entire upgrade
group absent**. The guard was
`ls /old/*.rpm /old/*.deb`, which exits non-zero whenever only **one** format
is present, because the unmatched pattern stays literal and `ls` reports it
missing. The group was skipped and the suite said nothing.

Both halves are fixed: the patterns are counted separately, and
`UPGRADE_REQUESTED` now turns *"an upgrade was requested but no older package
arrived"* into an explicit failure rather than a silence.

This is the third harness in this packaging effort to report a pass while
measuring nothing — after `install-smoke.sh`'s absent `runuser` and the
notification canary's empty capture. The pattern is consistent enough to be
worth naming: **a test that can skip must fail when it skips something it was
asked to do.**

---

## 7. L21–L26 — remove, reinstall, purge, and the trust store

**MEASURED** on all four package sets. A fake `identity.key` and `state.json`
are planted as a non-root user, then compared by **digest, mode and owner**
across every transaction:

```
ok    trust-store fixture planted, owned by tester
ok    the before-fingerprint names the trust store
ok    L21/L23: the trust store is byte- and mode-identical after remove
ok    L25: no package-owned file survived removal
ok    L22: reinstall succeeded
ok    L22/L23: the trust store is unchanged after reinstall
ok    L24: purge succeeded                        (DEB only)
ok    L24: the trust store is unchanged after PURGE   (DEB only)
ok    L26: every file in the user's state is owned by the user
```

**Purge is the one that matters most** and it holds on Debian 13, Ubuntu 24.04
and Ubuntu 26.04: `apt purge` leaves `~/.local/share/omnibridge` untouched.
There are no hand-written maintainer scripts at all, and
`packaging/tests/packaging-checks.sh` greps every generated one for `$HOME`
and `.local/share`.

The first version of this fixture **passed vacuously** — `runuser` is absent
from a minimal Fedora image, so nothing was ever planted and the comparison
was between two absent files. It now fails loudly when the fixture is missing.

---

## 8. What would make this certification complete

Nothing here needs new engineering. It needs access this environment does not
grant.

| Need | Unlocks |
| --- | --- |
| **A root password on a Fedora 44 desktop** | L1–L3, L6, L8, L9, L14–L16 on a real packaged install; L19/L20 with a logout and a reboot |
| **Root for bridged/macvtap VM networking**, or pre-built VMs with LAN access | the whole per-distribution matrix: Ubuntu 24.04, Ubuntu 26.04 and Debian 13 runtime gates, and L10/L12/L13 inside them |
| **An operator at the screen** | L6 and L9, which are visual by nature |
| **A second published build of the `.deb`** | L17/L13 on the Debian targets |

Until then the honest word for Ubuntu 24.04, Ubuntu 26.04 and Debian 13
remains **build-supported**, and for Fedora 44 **partially runtime-certified**.

---

## 9. Verdict

**PACKAGING V1 LIFECYCLE: PARTIAL — NOT CERTIFIED.**

Thirteen gates pass on real evidence: install, manifest, daemon ownership,
runtime directory and socket modes, restart, D-Bus cold activation, LAN
surface, upgrade with trust-store preservation, remove, reinstall, purge, no
orphans, and no root-owned user state. The SM-X620's pairing survived the
entire packaging effort untouched.

Eleven gates are **BLOCKED** and two are **PARTIAL**. Every one of them is
named above with the reason. None was marked as passing on the strength of a
neighbouring measurement, and no gate was reworded to fit what could be
measured.

**This document does not certify that OmniBridge works on an installed Linux
desktop.** It certifies that the packages install, upgrade, remove and purge
correctly without ever touching the user's trust store, and that the daemon's
runtime posture on Fedora 44 is what the unit promises. The graphical half of
the lifecycle is untested, and Packaging v1 cannot be called runtime-certified
until it is.
