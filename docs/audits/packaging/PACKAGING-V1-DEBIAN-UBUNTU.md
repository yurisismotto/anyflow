# OmniBridge — Packaging v1, Debian and Ubuntu

| Field | Value |
| --- | --- |
| **Branch** | `feature/debian-packaging-v1` |
| **Base commit** | `96e0898` (merge of PR #52, `feature/fedora-packaging-integration-v1`) |
| **Date** | 2026-09-22 |
| **Scope** | Audit §17 Phase 4, §6. First Debian/Ubuntu packaging this project has ever had. |
| **Authority** | [`PACKAGING-V1-READINESS-AUDIT.md`](PACKAGING-V1-READINESS-AUDIT.md) §6, §9.3, §12 |
| **Host** | Fedora 44 Workstation, podman 5.x, 16 cores. Builds ran in disposable containers. |
| **Product code changed** | **No.** No `.rs`, `.kt` or `.proto` file was modified. |
| **Verdict** | **BUILD-VERIFIED ON ALL THREE TARGETS** — not runtime-certified; see §7 |

Claims are labelled **MEASURED** (a command was run here and its output is
quoted) or **SOURCE-VERIFIED** (read out of the tree).

---

## 0. Summary

`git log --all --diff-filter=A -- 'packaging/debian/*' 'debian/*'` returned
nothing before this branch. There is now a debhelper source package that
builds two binary packages on all three targets, offline, from vendored
sources, as a non-root user.

| Target | `rustc` used | route | `%check` equivalent | lintian | install smoke |
| --- | --- | --- | --- | --- | --- |
| **Debian 13 trixie** | 1.94.1 | `trixie-backports` | **1016 passed, 0 failed** | 5 W, 0 E | **27 passed, 0 failed** |
| **Ubuntu 24.04 LTS** | 1.91.1 | `rustc-1.91` from its own archive | **1016 passed, 0 failed** | 5 W, 1 E | **27 passed, 0 failed** |
| **Ubuntu 26.04 LTS** | 1.93.1 | stock | **1016 passed, 0 failed** | 5 W, 1 E | **27 passed, 0 failed** |

```
                    omnibridge      omnibridge-gui
Debian 13            3669 KiB           605 KiB
Ubuntu 24.04         4027 KiB           663 KiB
Ubuntu 26.04         3974 KiB           657 KiB
```

---

## 1. The Rust floor, measured per target

The audit predicted this and every number held. **MEASURED**, each
distribution against its own archive:

| Target | stock `rustc` | meets 1.88? | what satisfies it |
| --- | --- | --- | --- |
| Ubuntu 24.04 LTS | 1.75.0 | **no** | `rustc-1.91` = 1.91.1, in the archive |
| Ubuntu 26.04 LTS | 1.93.1 | **yes** | stock |
| Debian 13 trixie | 1.85.1 | **no** | **nothing in the stock archive** — no versioned `rustc-1.x` exists |
| Debian 13 + backports | — | yes | `trixie-backports` `rustc` 1.94.1 |

`debian/control` therefore declares:

```
Build-Depends: rustc (>= 1.88) | rustc-1.91,
               cargo (>= 1.88) | cargo-1.91,
```

**This is satisfiable on both Ubuntu targets from their own archives and not
on stock Debian 13.** That is stated in `debian/control`, in
`debian/README.source` and here, rather than papered over by lowering the
floor to a version the code does not build against. The brief's instruction was
explicit: *"Do NOT falsely claim stock rustc can build OmniBridge."*

Two practical consequences, both handled:

* On Ubuntu 24.04 `rustc-1.91` installs into `/usr/lib/rust-1.91/bin` and does
  **not** provide `/usr/bin/rustc`. `debian/rules` prepends that directory to
  `PATH` when it exists, which is what lets one rules file serve all three.
* **Nothing reaches the runtime.** `rustc` and `cargo` are build dependencies
  only. **MEASURED** on the built packages:

  ```
  omnibridge:      Depends: libc6 (>= 2.39), libgcc-s1 (>= 4.2), init-system-helpers (>= 1.52)
  omnibridge-gui:  Depends: omnibridge (= 0.1.0-1), libadwaita-1-0 (>= 1.5),
                            libgtk-4-1 (>= 4.12.0), libc6, libcairo2, libglib2.0-0t64, …
  ```

  No relation names Rust, on any of the three. `packaging-checks.sh` asserts
  that, because it is the kind of thing a later `Depends:` edit could quietly
  undo.

  The GUI's floors come from `${shlibs:Depends}` reading the actual ELF
  headers — `libgtk-4-1 (>= 4.12.0)` and `libadwaita-1-0 (>= 1.5)` were not
  written by hand and match the versions `desktop/gui/Cargo.toml` gates
  features on.

---

## 2. Two packages, and nothing duplicated

**MEASURED**, `dpkg-deb -c`:

```
omnibridge                                    omnibridge-gui
/usr/bin/omnibridge                           /usr/bin/omnibridge-gui
/usr/bin/omnibridged                          /usr/share/applications/…omnibridge.desktop
/usr/lib/systemd/user/omnibridged.service     /usr/share/dbus-1/services/…omnibridge.service
/usr/share/icons/hicolor/scalable/apps/…svg   /usr/share/metainfo/…omnibridge.metainfo.xml
/usr/share/doc/omnibridge/README.md.gz        /usr/share/doc/omnibridge-gui/changelog.Debian.gz
/usr/share/doc/omnibridge/changelog.Debian.gz /usr/share/doc/omnibridge-gui/copyright
/usr/share/doc/omnibridge/copyright
```

The same split as the RPM, for the same reasons. The icon is in **core**:
`omnibridged` owns the StatusNotifierItem and a shell resolves its icon name
out of `hicolor`, so a core-only install would otherwise draw a grey square.

Two files that could easily have become second copies did not:

| File | How it stays single |
| --- | --- |
| the systemd unit | `debian/rules` installs `packaging/common/omnibridged.service` — the same file `%install` copies. It is where `ProtectSystem=strict` and the syscall filter live, and a second copy is exactly how a hardening change lands on one distribution and misses the other. |
| the desktop metadata | `debian/rules` runs `install-desktop-metadata.sh --prefix /usr --destdir debian/tmp`, the identical invocation the spec uses. Three of four files are installed verbatim; only the D-Bus `Exec=` line is derived. |

`packaging-checks.sh` asserts both, and asserts that **no** unit file has
appeared under `packaging/debian/`.

`packaging/fedora/cargo-vendor-config.toml` moved to `packaging/common/` in
this branch for the same reason: both formats now consume it.

---

## 3. No maintainer scripts, and the purge guarantee

**None are written by hand.** Each omission is deliberate and is tabulated in
`debian/README.source`: `dh_installsystemduser --no-enable` emits the correct
unit fragments, `desktop-file-utils` and `hicolor-icon-theme` declare dpkg
**triggers** for the two caches, the session-bus reload is impossible as root
and is handled inside the daemon, and a `prerm` cannot reach a user's service
manager.

The one that matters most is the one that must *not* exist:

> **No script may create, move or delete `~/.local/share/omnibridge`, on any
> path, including `purge`.**

The brief asked for automated assertions for this. There are now two layers:

**Static** — `packaging/tests/packaging-checks.sh` greps every maintainer
script for `$HOME` and `.local/share`, asserts `debian/rules` adds no purge
behaviour, and asserts that no hand-written maintainer scripts exist at all:

```
No maintainer script may touch the trust store, on any path (R6)
  ok    no hand-written maintainer scripts at all — debhelper generates them
  ok    debian/rules adds no purge behaviour
```

**Dynamic** — `packaging/tests/install-smoke.sh` plants a fake `identity.key`
and `state.json` owned by a non-root user, then runs the real transactions and
compares digest, mode and owner. **MEASURED**, identically on all three
targets:

```
User state survives remove and reinstall (audit §11, R6, gates L21-L23)
  ok    trust-store fixture planted, owned by tester
  ok    the before-fingerprint names the trust store
  ok    L21/L23: the trust store is byte- and mode-identical after remove
  ok    L25: no package-owned file survived removal
  ok    L22/L23: the trust store is unchanged after reinstall
  ok    L24: purge succeeded
  ok    L24: the trust store is unchanged after PURGE
  ok    L26: every file in the user's state is owned by the user
```

`27 passed, 0 failed` on Debian 13, Ubuntu 24.04 and Ubuntu 26.04.

---

## 4. The build: offline, non-root, nothing installed by name

`packaging/debian/build-deb.sh` is the Debian counterpart of building the RPM
in `mock`, and it is a container for the same three reasons:

1. **Nothing is installed by name.** `mk-build-deps` derives the buildroot from
   `debian/control` alone, so an under-declared `Build-Depends` fails the build
   instead of being silently covered by the host's packages.
2. **`cargo --locked --offline`** against a vendored tree, with `CARGO_HOME`
   redirected into the build directory so no builder's `~/.cargo/config.toml`
   can reintroduce a registry. A network fetch is impossible, not merely
   unnecessary — which is what a buildd requires.
3. **It builds as a normal user.** `omnibridge-core`'s store tests chmod a
   directory to `0000` and assert the read comes back `PermissionDenied`; root
   has `CAP_DAC_OVERRIDE` and reads it anyway, so a root build would pass a
   test that proves nothing.

`override_dh_auto_test` runs the full suite in release. **MEASURED: 1016
passed, 0 failed, 24 ignored on each of the three targets** — the same numbers
`mock` produces for the RPM.

---

## 5. Three defects found by building, and fixed

None of these is visible by reading. Each needed a real `dpkg-buildpackage`.

### 5.1 `dh_clean` deletes every `*.orig` in the tree

The one worth knowing about if you ever package Rust for Debian.

`cargo vendor` writes a `Cargo.toml.orig` beside each of the 270 vendored
crates and **names it in that crate's `.cargo-checksum.json`**. `dh_clean`
removes `*.orig` anywhere in the tree by default, so the clean step that runs
before every build silently deleted 270 files the checksums refer to.
**MEASURED:**

```
error: failed to calculate checksum of:
  /build/omnibridge-0.1.0/desktop/vendor/anyhow-1.0.104/Cargo.toml.orig
Caused by:
  No such file or directory (os error 2)
```

Fixed with `override_dh_clean: dh_clean -X.orig`, and the reason is written
into `debian/rules` beside it. Nothing is lost: the build tree is a fresh
unpack of the source bundle, so there is no stale `.orig` for `dh_clean` to
find that the vendoring did not put there.

### 5.2 Automatic `-dbgsym` packages were empty

debhelper generated `omnibridge-dbgsym` and `omnibridge-gui-dbgsym`, and
lintian reported every file in them:

```
W: omnibridge-dbgsym: debug-file-with-no-debug-symbols [usr/lib/debug/.build-id/…]
```

The release profile emits no debug info, so the packages carried nothing.
Shipping two packages empty of the only thing they exist for is worse than
shipping neither. `dh_strip --no-automatic-dbgsym` turns them off, which also
makes this consistent with the RPM's deliberate `%global debug_package %{nil}`
(audit Q2).

### 5.3 Two bugs in the build harness itself

Both mine, both silent, and both recorded because a harness that fails quietly
is worse than one that fails loudly:

| Bug | Symptom | Fix |
| --- | --- | --- |
| The script sourced `/etc/os-release`, which **defines `VERSION`** and overwrote the script's own | the build looked for `omnibridge-13 (trixie).tar.gz` | read only `ID`, in a subshell; rename the variable `OB_VERSION` |
| `useradd -m -u 1000 builder` behind `\|\| true` | Ubuntu 24.04+ ships a user already holding uid 1000, so it failed silently and the next line died with `chown: invalid user: builder` | let the system pick a free uid; the uid was never the point, only that the build is not root |

---

## 6. lintian

Run by `build-deb.sh` on every build and recorded, not suppressed. It is
**informational rather than a gate**: its tag set is tuned for the Debian
archive and this is a third-party package, not an archive submission.

**MEASURED**, after §5.2:

| Tag | Where | Disposition |
| --- | --- | --- |
| `no-manual-page` ×3 | all three | True. Man pages do not exist and are in no phase of the plan; `--help` is complete for all three binaries. The RPM reports the same gap. Debt. |
| `initial-upload-closes-no-bugs` ×2 | all three | Expects the changelog to close an ITP bug in the Debian BTS. There is no ITP because this is not an archive submission. |
| `E: bad-distribution-in-changes-file unstable` | **Ubuntu only** | Ubuntu's lintian applies Ubuntu-archive rules and wants a series name (`noble`, `resolute`). `unstable` is correct for Debian and is the conventional value for a third-party package built from one source tree for all three targets. Debian's own lintian accepts it. Forking the changelog per distribution to satisfy a linter that does not apply would be worse. |
| `debian-changelog-has-wrong-day-of-week` | — | **Fixed.** The changelog claimed 2026-09-22 was a Monday; it is a Tuesday. |
| `debug-file-with-no-debug-symbols` ×3 | — | **Fixed**, §5.2. |

---

## 7. What this branch did not do

**This is build verification, not runtime certification.** The distinction is
the same one `linux-distro-compat.yml` makes and it matters just as much here.

| # | Not covered | Why |
| --- | --- | --- |
| 1 | **L3, L6–L9, L19, L20** — autostart at login, launcher entry, D-Bus cold activation, tray, logout/login, reboot | A container has no compositor, no session bus, no logind session and no tray host. **Phase 6**, on a real graphical session. |
| 2 | **L10–L16** — mDNS, TCP 55432 reachability, physical Android discovery, pairing preservation, clipboard, files, notifications | Need a LAN and a phone. **Phase 6.** |
| 3 | **L17** — package upgrade | Needs two versions installed in sequence. The smoke covers install, remove, reinstall and purge. **Phase 6.** |
| 4 | A real Debian or Ubuntu **VM** | None exists on this host and installing to the host needs a root password. Containers ran the real `dpkg`/`apt` transactions, which is what the lifecycle gates in §3 need; they cannot substitute for a graphical session. |
| 5 | A **source** package (`.dsc` + `.orig.tar.gz`) | `dpkg-buildpackage -b` builds binaries only. `debian/source/format` is `3.0 (quilt)` per audit Q5, and producing the source package is **Phase 7**, alongside the other release artifacts. |
| 6 | Man pages | §6. Not in any phase of the plan. |

**Nothing here may be cited as evidence that OmniBridge *works* on Debian or
Ubuntu.** It is evidence that it builds correctly from a released source
bundle, that the packages contain what they should, and that no transaction —
including purge — touches the user's trust store.

---

## 8. Verdict

**BUILD-VERIFIED ON ALL THREE TARGETS.**

Debian 13 trixie, Ubuntu 24.04 LTS and Ubuntu 26.04 LTS each build two binary
packages from the same source bundle, offline, as a non-root user, with the
full test suite passing in release (1016/1016) and a buildroot derived from
`debian/control` alone. No package depends on Rust. The systemd unit and the
desktop metadata are the same files the RPM installs, not copies.

The Rust floor is not satisfiable on stock Debian 13, and the packaging says
so in three places rather than lowering it.

A planted trust store survived remove, reinstall and **purge** byte-, mode-
and owner-identical on all three targets, guarded by both a static grep and a
dynamic transaction test.

The three targets remain **build-supported**, not runtime-certified. That word
changes in Phase 6 or it does not change at all.
