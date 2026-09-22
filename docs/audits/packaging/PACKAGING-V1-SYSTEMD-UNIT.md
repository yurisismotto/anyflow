# OmniBridge — Packaging v1, the systemd user unit (P2, gates S1/S2/S3)

| Field | Value |
| --- | --- |
| **Branch** | `fix/systemd-user-unit-runtime-dir-v1` |
| **Base commit** | `6c8eac9` (merge of PR #49, `chore/docs-structure-v1`) |
| **Date** | 2026-09-22 |
| **Scope** | Audit §18 row 1 — Phase 1. Closes **P2**. Moves the unit to `packaging/common/`. |
| **Authority** | [`PACKAGING-V1-READINESS-AUDIT.md`](PACKAGING-V1-READINESS-AUDIT.md) §4.2, §7, §14.1 |
| **Host** | Fedora 44 Workstation, systemd 259.9, GNOME 50.5 Wayland, 16 cores, rustc/cargo 1.98.1 |
| **Product code changed** | **No.** No `.rs`, `.kt` or `.proto` file was modified. |
| **Verdict** | **S1 PASS · S2 PASS · S3 PASS** |

Claims are labelled **MEASURED** (a command was run here and its output is
quoted) or **SOURCE-VERIFIED** (read out of the tree).

---

## 0. Summary

The unit had three defects in four lines, all of them in audit §4.2, and none
of them visible to any test the project had. It now lives at
`packaging/common/omnibridged.service`, it starts on a machine that has never
run OmniBridge, and every hardening directive it had before it still has.

| # | Defect | Fix | Gate |
| --- | --- | --- | --- |
| P2a | No `RuntimeDirectory=`; under `ProtectSystem=strict` the daemon cannot create `$XDG_RUNTIME_DIR/omnibridge`, so the control socket has nowhere to bind | `RuntimeDirectory=omnibridge`, `RuntimeDirectoryMode=0700` | **S2 PASS** |
| P2b | `ReadWritePaths=%h/.local/share/omnibridge` names a path that does not exist on a fresh install; without a `-` prefix that refuses to start the unit | grant the parent, `ReadWritePaths=%h/.local/share` | **S1 PASS** |
| P2c | `StateDirectory=omnibridge` creates `~/.local/state/omnibridge` in a *user* unit — a directory the daemon never opens | removed | **S1 PASS** |
| — | The unit lived under `packaging/fedora/`, where Debian packaging would have grown a second copy | `git mv` to `packaging/common/` | §3 |

`ProtectSystem=strict` is untouched. `ProtectHome=read-only` is untouched.
Sixteen hardening directives are byte-identical and each one is now asserted
by name in `packaging/tests/packaging-checks.sh`, so a future "fix" that
reaches for one of them fails a check instead of passing review.

---

## 1. The change, as a diff of effective directives

Comments are not directives, and a report that diffs whole files hides the
answer in prose. This is the diff with comment and blank lines stripped from
both sides. **MEASURED:**

```console
$ git show HEAD:packaging/fedora/omnibridged.service | grep -vE '^\s*#|^\s*$' > old
$ grep -vE '^\s*#|^\s*$' packaging/common/omnibridged.service > new
$ diff -u old new
@@ -11,8 +11,9 @@
 PrivateTmp=true
 ProtectSystem=strict
 ProtectHome=read-only
-StateDirectory=omnibridge
-ReadWritePaths=%h/.local/share/omnibridge
+RuntimeDirectory=omnibridge
+RuntimeDirectoryMode=0700
+ReadWritePaths=%h/.local/share
 ProtectKernelTunables=true
 ProtectKernelModules=true
 ProtectControlGroups=true
```

Three lines out, three lines in. Nothing else in the unit changed — not
`ExecStart`, not `Restart=`, not one directive in the sandbox.

The full effective unit, **SOURCE-VERIFIED** from
`packaging/common/omnibridged.service`:

```ini
[Unit]
Description=OmniBridge — local device continuity daemon
Documentation=https://github.com/yurisismotto/omnibridge
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/omnibridged
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
RuntimeDirectory=omnibridge
RuntimeDirectoryMode=0700
ReadWritePaths=%h/.local/share
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictNamespaces=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true
MemoryDenyWriteExecute=true
SystemCallArchitectures=native
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources @obsolete
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK

[Install]
WantedBy=default.target
```

---

## 2. Why the grant is the parent and not the leaf

This is the one decision in the change that widens anything, so it gets its
own section rather than a line in a table.

The readiness audit reserved two outcomes in §7.2 and said gate S1 would pick
between them. S1 was run, it picked the second, and the reasoning is worth
stating plainly because "grant the parent" reads like the lazy option and is
not:

* `ReadWritePaths=%h/.local/share/omnibridge` **refuses to start the unit**
  when the path does not exist. Audit §4.2 probe C: `Failed to start transient
  service unit: Invalid ReadWritePaths`. Every fresh install is that machine.
* `ReadWritePaths=-%h/.local/share/omnibridge` starts the unit — the `-`
  prefix means "ignore when absent" — and then the hole is not established
  either, so the daemon's `create_dir_all` lands on a read-only `$HOME` and
  the first run fails with the identity never written. Absent-and-ignored is
  not the same as present-and-writable.
* `ReadWritePaths=%h/.local/share` is the fallback §7.2 reserved. The parent
  exists on every desktop machine that has ever run a single XDG-aware
  application, so the unit starts; the hole is established at a real path, so
  the daemon can create its own directory one level down.

What that costs, stated honestly: the daemon can write anywhere under
`~/.local/share`, not only under `~/.local/share/omnibridge`. What it does
not cost: `ProtectSystem=strict` is unchanged, `ProtectHome=read-only` is
unchanged, and the rest of `$HOME` — `~/.ssh`, `~/.config`, `~/Documents`,
`~/.gnupg` — stays read-only to the daemon. It is a bounded widening inside an
existing restriction, and it is reviewable in one line.

A scriptlet that created `~/.local/share/omnibridge` at install time would
have avoided it. That was rejected: the data directory holds the user's
pairings, a package must never create, own or remove it, and a root scriptlet
creating a directory in a user's `$HOME` is how packages end up leaving
root-owned state behind (gate L26).

---

## 3. One unit, every format

**MEASURED** — the move is recorded as a rename, and the content is unchanged
apart from §1:

```console
$ git status --short
R  packaging/fedora/omnibridged.service -> packaging/common/omnibridged.service
```

Consumers updated in the same commit:

| File | Change |
| --- | --- |
| `packaging/fedora/omnibridge.spec` | `%install` copies from `packaging/common/` |
| `packaging/release/make-source-bundle.sh` | the required-build-input list names the new path, so a bundle missing the unit is a hard failure |
| `packaging/tests/packaging-checks.sh` | asserts the new path, asserts **no** copy survives under `packaging/fedora/`, and asserts the bundle carries it |
| `README.md` | the source-tree map and the manual-install recipe |
| `packaging/fedora/README.md` | now points at `packaging/common/` instead of describing a unit it no longer holds |
| `packaging/common/README.md` | new — what the unit does, why each load-bearing directive is there, and how to re-measure it |

The point of the move is not tidiness. The unit is where
`ProtectSystem=strict`, the syscall filter and `RestrictAddressFamilies` live.
Under `packaging/fedora/`, the obvious way to package Debian in Phase 4 would
have been a second copy — and the first tightening of any of those directives
after that would have landed on one distribution and silently missed the
other.

---

## 4. Gates S1, S2, S3 — how they were measured

Audit §14.1 defines the three gates. They are not a one-off: §14.3 runs them
again on Ubuntu 24.04, Ubuntu 26.04 and Debian 13, and every later change to
the unit has to re-measure them. So they are a committed script,
`packaging/tests/systemd-unit-gates.sh`, not a transcript.

### 4.1 What the harness does, and the two deltas it applies

It installs the real unit file, unmodified, at
`~/.config/systemd/user/omnibridged.service` and starts it under the real user
service manager. It then applies exactly **two** overrides, in a drop-in, and
they are both recorded here because a report that hid them would be worthless:

| Delta | Why |
| --- | --- |
| `ExecStart=` → the built binary | `/usr/bin/omnibridged` is not installed on this host; the RPM that installs it is built in §5. The argument vector is otherwise untouched — **mDNS stays on**, because it is the widest syscall surface the daemon has and S3 is precisely a question about syscalls. |
| `Environment=XDG_DATA_HOME=<throwaway>` | S1 asks what happens when the data directory does **not** exist, which cannot be asked of a machine where it does. |

**No sandbox directive is overridden.** Anything that did would make the run
meaningless.

The throwaway root is created one level under `~/.local/share`, so the
daemon's own directory sits at exactly the depth `~/.local/share/omnibridge`
does relative to the `ReadWritePaths=` grant. The grant is exercised at the
same depth it will be in service, not a shallower one.

### 4.2 The real trust store was not touched, and that is measured

The host has a real, paired identity. The harness digests
`~/.local/share/omnibridge` before and after — content, mode, owner and size —
and fails if anything moved. **MEASURED:**

```console
$ sha256sum ~/.local/share/omnibridge/*
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json
$ stat -c '%a %U %n' ~/.local/share/omnibridge ~/.local/share/omnibridge/*
700 yuri /home/yuri/.local/share/omnibridge
600 yuri /home/yuri/.local/share/omnibridge/identity.key
600 yuri /home/yuri/.local/share/omnibridge/state.json
```

Identical before and after the run — the harness's own final check:

```
The real trust store was not touched
  ok    /home/yuri/.local/share/omnibridge is byte- and mode-identical to before the run
```

Independent corroboration that the daemon really used the throwaway store: the
probe daemon logged `fingerprint=531A 0681 8911 BAC6`, while the host's real
identity is `149F 6B66 AB5D 8526`. Two different identities, so the real key
was never read.

A hand-started development daemon was running on this host and holding TCP
55432 and the control socket. The harness **refuses** to stop a daemon it did
not start; it was stopped by hand before the run and restarted, unchanged,
afterwards. `omnibridge status` after the run still reports
`paired 1 device(s) — SM-X620`.

### 4.3 The run

**MEASURED**, `./packaging/tests/systemd-unit-gates.sh --binary
desktop/target/release/omnibridged`, Fedora 44, systemd 259.9:

```
Static verification
  ok    systemd-analyze verify: clean

Gate S1 — first start with no data directory
  XDG_DATA_HOME=/home/yuri/.local/share/omnibridge-gate-probe.344069
  (data directory .../omnibridge does not exist)
  ok    S1: the unit starts where the data directory does not exist
  ok    S1: the unit is active
  ok    S1: the daemon created identity.key inside a read-only $HOME
  ok    S1: identity.key is 0600
  ok    S1: state.json is 0600
  ok    S1: the data directory is 0700

Gate S2 — runtime directory and control socket
  ok    S2: /run/user/1000/omnibridge exists
  ok    S2: runtime directory mode 0700
  ok    S2: runtime directory owned by yuri
  ok    S2: control.sock exists and is a socket
  ok    S2: control.sock mode 0600
  ok    S2: control.sock owned by yuri

The process itself
  ok    the daemon runs as yuri, not root
  ok    NoNewPrivs is 1 on the running process

Restart (audit L18)
  ok    restart: the socket is recreated at 0600
  ok    restart: the unit is active again

Gate S3 — no sandbox denial in the journal
  36 journal lines for omnibridged.service
  ok    S3: no ProtectSystem, ReadWritePaths, namespace or seccomp denial

Stop, and what systemd takes with it
  ok    systemd removed the runtime directory with the unit
  ok    the data directory survived the stop (state is not systemd's to clean)

The real trust store was not touched
  ok    /home/yuri/.local/share/omnibridge is byte- and mode-identical to before the run

-----------------------------------------------
21 passed, 0 failed
```

### 4.4 S1 — what the journal shows the daemon actually did

The gate is not "the unit did not crash". It is "the daemon reached a working
state from nothing". **MEASURED**, `journalctl --user -u omnibridged`:

```
systemd[4386]: Started omnibridged.service - OmniBridge — local device continuity daemon.
omnibridged: local identity device=8db865a544ebb54273ab5a9b4032f400 name=fedora
             fingerprint=531A 0681 8911 BAC6 key_backing=software
omnibridged: UPower available; this machine will report its own battery
omnibridged: files.v1 ready download_dir=/home/yuri/Downloads/OmniBridge
omnibridged: notifications.v1 ready sink=org.freedesktop.Notifications — gnome-shell 50.5
omnibridged: capabilities registered capabilities=["battery.v1", "clipboard.v1", "files.v1", "notifications.v1"]
omnibridged: listening port=55432 families=IPv4+IPv6 sockets=1
omnibridged: control endpoint ready endpoint=/run/user/1000/omnibridge/control.sock
omnibridge_runtime::mdns: advertising _omnibridge._tcp.local. port=55432 families=IPv4+IPv6
omnibridge_linux::tray: OmniBridge tray item published on the session bus
omnibridge_linux::tray::watcher: registered an OmniBridge tray item with the desktop shell
```

A **new** identity was generated and written — `identity.key` at 0600 inside a
0700 directory — inside a `$HOME` that `ProtectHome=read-only` makes read-only
everywhere else. That is the whole of what S1 asks, and it is the line the old
unit could not have produced.

All four capabilities registered, the TCP listener bound on both families, the
control endpoint came up, mDNS advertised, and the tray item published on the
session bus. Nothing in the sandbox blocked any of it.

### 4.5 S3 — what "no denial" was tested against

Grepping for the word `error` would pass on a unit that never started. The
gate matches the strings this sandbox actually uses to announce a refusal:

```
Read-only file system | Operation not permitted | Permission denied
Failed to set up mount namespace
Failed at step (NAMESPACE|RUNTIME_DIRECTORY|STATE_DIRECTORY|EXEC|SECCOMP)
Invalid ReadWritePaths | bad-system-call | signal=SYS
Failed to create.*director | seccomp
```

None matched across 36 journal lines covering a cold start, a restart and a
stop.

Three `INFO` lines *do* mention `Connection refused (os error 111)`. They are
the clipboard backend reporting that this GNOME 50.5 compositor implements
neither the `wlr`/`ext` data-control protocol nor a reachable Xwayland
display, so clipboard **auto-send** is unavailable while manual send still
works. That is pre-existing, session-dependent, honestly reported by the
daemon, and has nothing to do with the sandbox — the same message appears
outside systemd. It is **not** an S3 failure and is not being quietly counted
as one.

### 4.6 The sandbox was applied, not merely declared

A unit whose directives were silently ignored would look identical in
`systemctl cat`. **MEASURED** on the running process: `NoNewPrivs: 1` in
`/proc/<pid>/status`.

`systemd-analyze security --user omnibridged.service` rates it:

```
→ Overall exposure level for omnibridged.service: 4.7 OK :-)
```

The remaining exposure is what a LAN daemon with a desktop integration is
made of and is deliberate, not unexamined:

| Reported | Why it stays |
| --- | --- |
| `RestrictAddressFamilies=~AF_(INET\|INET6)` 0.3 | OmniBridge is a LAN daemon. This is the product. |
| `PrivateNetwork=` 0.5 | Same. |
| `RestrictAddressFamilies=~AF_UNIX` 0.1 | The control socket, and the session bus for the tray and notifications. |
| `RestrictAddressFamilies=~AF_NETLINK` 0.1 | mDNS needs interface enumeration. |
| `ProtectHome=` 0.1 | Rated against `ProtectHome=yes`. It is `read-only`, deliberately: the daemon must read the user's `$XDG_DATA_HOME`. |
| `CapabilityBoundingSet=~…` (many, 0.1–0.3 each) | A **user** unit cannot set a capability bounding set. The process starts with no capabilities at all, which the report cannot see. |
| `PrivateUsers=`, `RootDirectory=`, `ProtectProc=`, `ProcSubset=`, `DeviceAllow=`, `KeyringMode=` 0.1–0.2 each | Not available, or not meaningful, to a `--user` unit. |
| `UMask=` 0.1 | The daemon sets its own modes explicitly — `identity.key` 0600, the data directory 0700, `control.sock` 0600, all three measured above — rather than relying on a umask. |

Recorded as a debt, deliberately **not** fixed here: `ProtectClock=`,
`ProtectHostname=`, `ProtectKernelLogs=` and `PrivateDevices=` are available to
user units and are not set. Each is plausible and each needs its own
measurement against a live session before it ships — `PrivateDevices=` in
particular interacts with the clipboard and tray paths. Adding four untested
directives inside a branch whose gate is "the unit starts on a fresh install"
is how the next S1 gets introduced. See §7.

---

## 5. The package still contains the unit

Moving a file the spec installs by path is exactly the change that builds a
package with a missing file. **MEASURED** — rebuilt from a source bundle in
`mock 6.8`, `fedora-44-x86_64`, offline:

```console
$ ./packaging/release/make-source-bundle.sh --worktree --output dist
$ rpmbuild --define "_topdir …" -bs SPECS/omnibridge.spec
$ mock -r fedora-44-x86_64 --rebuild SRPMS/omnibridge-0.1.0-2.fc44.src.rpm
…
INFO: Done(…/omnibridge-0.1.0-2.fc44.src.rpm) Config(fedora-44-x86_64) 9 minutes 22 seconds
```

`mock 6.8`, `rpm-build 6.0.2`, `fedora-44-x86_64`, networking disabled during
`%build` — the same configuration the build foundation was certified on.
Artifacts:

```
omnibridge-0.1.0-2.fc44.x86_64.rpm      4.8 MiB
omnibridge-0.1.0-2.fc44.src.rpm          30 MiB
```

`%check` ran inside the buildroot and passed: **997 passed, 0 failed, 23
ignored** across 62 test binaries. The flaky convergence test recorded as debt
12 of the build foundation did not reproduce in this run.

The unit inside the package, **MEASURED** with
`rpm2cpio … | cpio -i --to-stdout ./usr/lib/systemd/user/omnibridged.service`
and comment lines stripped, is byte-for-byte the file in §1 — including
`RuntimeDirectory=omnibridge`, `RuntimeDirectoryMode=0700` and
`ReadWritePaths=%h/.local/share`. The move did not lose it, and the package
does not carry a stale copy.

`packaging/tests/packaging-checks.sh --bundle … --rpm …` — **78 passed, 0
failed**, including the four package-level assertions:

```
Built package: omnibridge-0.1.0-2.fc44.x86_64.rpm
  ok    package contains /usr/bin/omnibridged
  ok    package contains /usr/bin/omnibridge
  ok    package contains /usr/bin/omnibridge-gui
  ok    the user unit landed in a real systemd user directory
  ok    no unexpanded rpm macro in the file list
```

and, in the bundle group, `ok bundle carries packaging/common/omnibridged.service`
— the assertion that would have caught this move breaking the release tarball.

### 5.1 rpmlint, including one error this branch is not fixing

**MEASURED**, `rpmlint 2.8.0`:

```
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridge
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridge-gui
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridged
omnibridge.x86_64: W: no-manual-page-for-binary omnibridge
omnibridge.x86_64: W: no-manual-page-for-binary omnibridge-gui
omnibridge.x86_64: W: no-manual-page-for-binary omnibridged
omnibridge.x86_64: E: file-contains-buildroot /usr/share/doc/omnibridge/docs/audits/packaging/PACKAGING-V1-BUILD-FOUNDATION.md
 1 packages and 0 specfiles checked; 1 errors, 6 warnings, 3 filtered
```

| Finding | Status |
| --- | --- |
| `unstripped-binary-or-object` ×3 | Expected. `%global debug_package %{nil}` — a deliberate v1 choice recorded in audit §19 Q2, not an oversight. |
| `no-manual-page-for-binary` ×3 | True, and out of scope here. Man pages are not in any phase of the plan; recorded so the gap is visible. |
| `E: file-contains-buildroot` | **Pre-existing, reproduced here, and deliberately left for Phase 3.** |

The error is worth stating plainly rather than filtering. `%files` contains
`%doc README.md docs/`, so the RPM ships **178 files** of engineering evidence
in `/usr/share/doc/omnibridge/docs/` — 19 MB uncompressed for a package whose
three binaries are the product. One of those documents,
`PACKAGING-V1-BUILD-FOUNDATION.md`, quotes `mock` transcripts containing
`/builddir/build/BUILD/…` paths, and rpmlint reads a buildroot path inside a
shipped file as an error.

It is a correct complaint about the wrong thing: the document is right to
quote its own build transcript — audit §"Historical documents are evidence"
requires exactly that — and the package is wrong to ship it. The fix is
trimming `%doc`, which the plan already schedules as Phase 3 item 5 ("Fix
`%doc docs/` bloat. Do not ship engineering evidence tree inside RPM").

It is **not** a regression from this branch. `git log` puts
`PACKAGING-V1-BUILD-FOUNDATION.md` in `e571859`, two commits before this one,
and nothing here changes `%files`. Recorded, owned, and left to the phase that
owns `%doc`.

---

## 6. Packaging regression checks

`packaging/tests/packaging-checks.sh` gained a group for the unit. It asserts
the path, that no second copy survives under `packaging/fedora/`, that the
spec installs the common one, the three changed directives by exact value, the
absence of `StateDirectory=`, and each of the sixteen hardening directives by
name. **MEASURED:**

```
The canonical systemd user unit (audit §7.1; gates S1-S3)
  ok    the unit is at packaging/common/omnibridged.service
  ok    no duplicate unit under packaging/fedora/
  ok    the spec installs the common unit
  ok    S2: RuntimeDirectory=omnibridge
  ok    S2: RuntimeDirectoryMode=0700
  ok    S1: ReadWritePaths=%h/.local/share (the parent, so a fresh install can create its data directory)
  ok    no StateDirectory= (it would point at ~/.local/state)
  ok    hardening kept: NoNewPrivileges=true
  ok    hardening kept: PrivateTmp=true
  ok    hardening kept: ProtectSystem=strict
  ok    hardening kept: ProtectHome=read-only
  ok    hardening kept: ProtectKernelTunables=true
  ok    hardening kept: ProtectKernelModules=true
  ok    hardening kept: ProtectControlGroups=true
  ok    hardening kept: RestrictNamespaces=true
  ok    hardening kept: RestrictRealtime=true
  ok    hardening kept: RestrictSUIDSGID=true
  ok    hardening kept: LockPersonality=true
  ok    hardening kept: MemoryDenyWriteExecute=true
  ok    hardening kept: SystemCallArchitectures=native
  ok    hardening kept: SystemCallFilter=@system-service
  ok    hardening kept: SystemCallFilter=~@privileged @resources @obsolete
  ok    hardening kept: RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK
  ok    no User=/Group= — it runs as whoever owns the session
  ok    [Install] WantedBy=default.target (a user unit target)
  ok    ExecStart is the absolute installed path
  ok    no *Directory= directive creates or owns user state
```

Full run, static checks only: **49 passed, 0 failed.**

The last assertion in that group is the one worth keeping: it fails if any
`RuntimeDirectory=`, `StateDirectory=`, `CacheDirectory=`, `LogsDirectory=` or
`ConfigurationDirectory=` ever points into `.local`. That is the mechanism by
which a unit would start creating and — on stop — removing the user's trust
store, which is the failure mode R6 exists to prevent.

---

## 7. What this branch did not do

| # | Left open | Where it belongs |
| --- | --- | --- |
| 1 | `%systemd_user_post` / `_preun` / `_postun` in the spec | Phase 3. `systemd-rpm-macros` is already a `BuildRequires`. |
| 2 | Gate **L3** (autostart survives a logout/login) and **L19**–**L20** | Phase 6. They need a session cycle and a reboot, not a unit file. |
| 3 | `ProtectClock=`, `ProtectHostname=`, `ProtectKernelLogs=`, `PrivateDevices=` | A dedicated hardening pass with its own live-session measurements. §4.6. |
| 4 | The Debian side actually installing this file | Phase 4, which is why the file is where it is. |
| 5 | The gates on Ubuntu 24.04 / 26.04 / Debian 13 | Phase 6. The harness is committed so they are a command, not a rewrite. |

---

## 8. Verdict

**S1 PASS.** The unit starts on a machine where the daemon's data directory
does not exist, and the daemon creates it, writes `identity.key` at 0600 into
a 0700 directory, and reaches a fully registered state — inside a `$HOME` that
is otherwise read-only to it.

**S2 PASS.** `$XDG_RUNTIME_DIR/omnibridge` is `0700` and user-owned;
`control.sock` is `0600` and user-owned. systemd creates the directory and
removes it with the unit.

**S3 PASS.** No `ProtectSystem`, `ReadWritePaths`, namespace or seccomp denial
in the journal across a cold start, a restart and a stop.

**P2 is closed.** `ProtectSystem=strict` was not weakened to get here.
