# OmniBridge — the systemd user unit could not start on Ubuntu

| Field | Value |
| --- | --- |
| **Branch** | `fix/systemd-user-unit-capabilities-v1` |
| **Baseline** | `9c76719` (merge of PR #59, Release Readiness v1 baseline) |
| **Date** | 2026-09-22 |
| **Found by** | Release Readiness v1, phase R1 — the first execution of lifecycle gate **L1** on a real installed desktop |
| **Severity** | **Release-blocking on Ubuntu 24.04 LTS and Ubuntu 26.04 LTS.** The daemon cannot start from the package at all. Fedora 44 and Debian 13 are unaffected. |
| **Product code changed** | **No.** No `.rs`, `.kt` or `.proto` file was modified. One line was removed from one `.service` file. |

---

## 1. What happens

On Ubuntu 24.04 and Ubuntu 26.04, with the **shipped** `omnibridge_0.1.0-1_amd64.deb`
installed, the user unit never reaches `ExecStart`:

```console
$ systemctl --user start omnibridged.service
$ systemctl --user status omnibridged.service
● omnibridged.service - OmniBridge — local device continuity daemon
     Loaded: loaded (/usr/lib/systemd/user/omnibridged.service; disabled; preset: enabled)
     Active: activating (auto-restart) (Result: exit-code)
    Process: 3727 ExecStart=/usr/bin/omnibridged (code=exited, status=218/CAPABILITIES)

$ journalctl --user -u omnibridged -n 3
(ibridged)[3727]: omnibridged.service: Failed to drop capabilities: Operation not permitted
systemd[1130]: omnibridged.service: Main process exited, code=exited, status=218/CAPABILITIES
systemd[1130]: omnibridged.service: Failed with result 'exit-code'.

$ ls /run/user/1000/omnibridge
ls: cannot access '/run/user/1000/omnibridge': No such file or directory
```

`Restart=on-failure` then retries forever, so the unit sits in
`activating (auto-restart)` and nothing ever listens on 55432. There is no
control socket, so the CLI, the GUI and the tray all have nothing to connect
to.

**The binary is fine.** `/usr/bin/omnibridged --version` prints
`omnibridged 0.1.0` on the same machine. The daemon never runs.

---

## 2. Blast radius — measured, one distribution at a time

The probe is a minimal user unit carrying `ProtectKernelModules=true` and
`ExecStart=/bin/true`, plus a control run of the same unit with the directive
deleted. The control must start, or the probe proves nothing.

| Distribution | systemd | Probe | Control | Shipped unit |
| --- | --- | --- | --- | --- |
| Fedora 44 Workstation | `259.9-1.fc44` | **starts** | starts | **starts** |
| Debian 13 trixie | `257.13-1~deb13u1` | **starts** | starts | **starts** |
| **Ubuntu 24.04 LTS** | `255.4-1ubuntu8.17` | **FAILS 218/CAPABILITIES** | starts | **FAILS** |
| **Ubuntu 26.04 LTS** | `259.5-0ubuntu3.4` | **FAILS 218/CAPABILITIES** | starts | **FAILS** |

**It is not a systemd version boundary.** Debian 13's 257.13 works and Ubuntu
26.04's 259.5 does not; Fedora 44 and Ubuntu 26.04 are both systemd 259 and
differ. Nor is it the capability sets, which are byte-identical on every host
measured:

```console
# user manager, Fedora 44 and Debian 13 and Ubuntu 26.04 alike
CapInh: 0000000800000000   CapPrm: 0000000800000000
CapEff: 0000000800000000   CapBnd: 000001ffffffffff
```

---

## 3. Why

`ProtectKernelModules=yes` implies `CapabilityBoundingSet=~CAP_SYS_MODULE`.

Changing a bounding set requires `CAP_SETPCAP`. A **user** manager has none —
`prctl(PR_CAPBSET_DROP, CAP_SYS_MODULE)` returns `EPERM` for uid 1000 on
**every** distribution measured, including the two where the unit starts:

```console
# identical result on Fedora 44 and on Debian 13
uid=1000 prctl(PR_CAPBSET_DROP, CAP_SYS_MODULE) -> -1 errno=1 (EPERM)
```

So on the distributions where the unit *does* start, systemd is not calling
that syscall directly — it applies the restriction from inside an unprivileged
**user namespace**, where it holds the capability. Measured on Debian 13, the
spawned process really does come out restricted:

```console
CapBnd: 000001fffffeffff     # 0x1ffffffffff with bit 16 (CAP_SYS_MODULE) cleared
```

Where an unprivileged user namespace cannot be created, that route is closed
and the direct `PR_CAPBSET_DROP` fails. Ubuntu restricts unprivileged user
namespaces by AppArmor policy; Debian 13 does not even expose the
`kernel.apparmor_restrict_unprivileged_userns` sysctl, and Fedora uses SELinux.

**A bisect confirms it is this directive and only this directive.** One drop-in
per candidate, unit restarted after each, on Ubuntu 24.04:

```
ProtectKernelTunables=false -> exit-code, activating
ProtectKernelModules=false -> success,   active      <-- sole cause
ProtectControlGroups=false -> exit-code, activating
RestrictNamespaces=false   -> exit-code, activating
```

---

## 4. The fix, and why it is not a weakening

`ProtectKernelModules=true` is removed from
[`packaging/common/omnibridged.service`](../../../packaging/common/omnibridged.service).
Nothing else in the unit changed.

Packaging v1 was explicit that this list must not be raided for a start-up fix:

> `ProtectSystem=strict` was **not weakened** to achieve this. Sixteen
> hardening directives are byte-identical to before and each is now asserted by
> name, so a future fix for a start-up failure cannot quietly reach for one.

That guard did its job — it is why this change is argued rather than made. The
argument is that in a **user** unit this particular directive protects nothing
that is not already protected twice, **MEASURED**:

| Claim | Measurement |
| --- | --- |
| The module syscalls are already blocked | `systemd-analyze syscall-filter @system-service` does **not** list `init_module`, `finit_module` or `delete_module`. `SystemCallFilter=@system-service` is an **allow-list**, so all three are refused regardless of any capability. |
| There is no capability to use anyway | an unprivileged process has `CapEff: 0000000000000000` — no `CAP_SYS_MODULE`. |
| It cannot be re-gained | `NoNewPrivileges=true` is set, so the bounding set cannot grow across an `execve`. |

So the directive's only observable effect in this unit is on the distributions
where it makes the unit unstartable.

**The honest cost, recorded rather than omitted:**

```
→ Overall exposure level for omnibridged.service: 5.0 MEDIUM
```

Packaging v1 recorded **4.7 OK**. `systemd-analyze security` scores the
directive's presence, not its effect, so removing it costs 0.3 on that scale
while changing nothing a process can actually do. A unit that does not start
scores nothing at all.

---

## 5. What stops it coming back

Two guards, because a static one alone is what let this ship.

**Static** — `packaging/tests/packaging-checks.sh` moves the directive from the
"must be kept" list to an explicit negative assertion, alongside the two
directives that name the capability machinery outright:

```
ok    no ProtectKernelModules= — a user manager cannot apply it, and it is redundant here
ok    no CapabilityBoundingSet= — …
ok    no AmbientCapabilities= — …
```

`packaging-checks.sh`: **95 passed, 0 failed** (was 93; one assertion removed,
three added).

**Runtime** — `packaging/tests/lifecycle-gates.sh`, added by Release Readiness
v1 phase R1, asserts that the unit **actually starts** on each supported
distribution, that a control socket appears, and that a process exists. That is
the gate whose absence let a unit that cannot start reach a certified package
set.

---

## 6. Re-verification on the unaffected host

`packaging/tests/systemd-unit-gates.sh` against the fixed unit, Fedora 44,
three consecutive runs: **21 passed, 0 failed** each time. S1, S2, S3, L18 and
the trust-store guard all hold.

```
ok    S1: the unit starts where the data directory does not exist
ok    S2: /run/user/1000/omnibridge exists, mode 0700, owned by yuri
ok    S2: control.sock mode 0600, owned by yuri
ok    S3: no ProtectSystem, ReadWritePaths, namespace or seccomp denial
ok    restart: the socket is recreated at 0600
ok    /home/yuri/.local/share/omnibridge is byte- and mode-identical to before the run
```

> **One flaky first run, recorded because it is a harness gap and not a
> product one.** The very first invocation reported 17 passed, 1 failed,
> immediately after the development daemon had been killed. The gate refuses to
> run when an `omnibridged` *process* exists but does not check that **TCP
> 55432 is free**, and a just-killed daemon still holds it. Three runs after the
> port was released were clean. The missing precondition is carried into
> Release Readiness phase R4.

The real trust store was digested before and after every run and is
byte-identical: `1914f6cd…ca21` / `a539ea91…e6ce`.

---

## 7. What this means for the packaged artifacts

**The `0.1.0-1` `.deb` files in CI run `35730680143` are not shippable to
Ubuntu.** They carry the broken unit. The artifacts must be rebuilt from a
commit containing this fix before any Ubuntu lifecycle gate can be certified
against them.

Packaging v1's verdict is not reopened: it certified that the **packages**
install, upgrade, remove and purge correctly, and they still do — this defect
is in what happens *after* installation, which is precisely the half that
[its §11](../../certification/linux/PACKAGING-V1-FINAL-CERTIFICATION.md) marked
BLOCKED and did not claim.

---

## 8. Verdict

**FIXED.** One directive removed, argued rather than assumed, with the
protection it nominally provided shown to be present twice over by
measurement. Two Ubuntu LTS releases go from *the daemon cannot start* to *the
daemon starts*. A static guard stops it returning and a runtime guard now
asserts the property that matters — that the unit starts — on every supported
distribution rather than only on the one the maintainer happens to run.
