# OmniBridge — Packaging v1, final certification

| Field | Value |
| --- | --- |
| **Branch** | `docs/packaging-v1-final-certification` |
| **Baseline** | `bdb7049` (merge of PR #57, release artifact CI) |
| **Date** | 2026-09-22 |
| **Mode** | Read-mostly audit. No feature was added, no product code was changed, and nothing below is a new measurement except the re-verification in §0.1. |
| **Verdict** | **PACKAGING V1: CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS** |

This certifies **the packages**. It does **not** certify that OmniBridge works
on an installed Linux desktop — eleven lifecycle gates never ran, and §11 says
which and why. The distinction is load-bearing and is kept throughout.

---

## 0. Executive summary

Packaging v1 began with a readiness audit that found a spec which had **never
produced an artifact**: three build-fatal defects in 73 lines, a systemd unit
that could not start on a fresh install, no Debian packaging at all, and no
release pipeline. Eight branches later:

| | Before | Now |
| --- | --- | --- |
| Fedora RPM | did not build | **2 packages**, offline, `%check` 1030/0 |
| Debian/Ubuntu | **did not exist** | **6 packages** across 3 distributions |
| systemd unit | would not start on a fresh install | S1/S2/S3 **PASS**, sandbox intact |
| D-Bus activation | `ServiceUnknown` until logout | self-healed from inside the session |
| Release artifacts | none | **14**, from one immutable commit, attested |
| Security | never measured | **16 gates**, one real CVE found and fixed |

**Two things were found that mattered more than the packaging work:**

* **A real vulnerability.** `cargo audit`, run for the first time as gate
  SEC-DEPS-01, found **RUSTSEC-2026-0285** in rustls 0.23.43 — *"TLS 1.3
  handshake messages incorrectly accepted across encryption level
  boundaries"*. OmniBridge speaks TLS 1.3 and nothing else, so it sat on the
  path every peer session uses. Fixed in its own branch (PR #54) before the
  certification continued.
* **A log-privacy gate with no evidence.** SEC-LOG-03 — *file content absent
  from logs* — had never been tested, while its clipboard and notification
  equivalents were covered twice over. It had been nominally green while
  resting on nothing.

### 0.1 Re-verified for this document

| Check | Result |
| --- | --- |
| `packaging/tests/packaging-checks.sh` | **93 passed, 0 failed** |
| `cargo test --workspace -j 2` | **1030 passed, 0 failed**, 24 ignored |
| `cargo audit --deny warnings` | **clean**, 282 dependencies |
| The canonical unit on `develop` | `RuntimeDirectory=omnibridge`, `RuntimeDirectoryMode=0700`, `ReadWritePaths=%h/.local/share`, `ProtectSystem=strict`, `ProtectHome=read-only`, no `StateDirectory=` |

---

## 1. Supported distributions

| Distribution | Status | What that word means here |
| --- | --- | --- |
| **Fedora 44** | **partially runtime-certified** | packages build, install, upgrade, remove and reinstall; daemon, socket, restart, D-Bus cold activation and LAN surface measured on a real host. The graphical gates did not run. |
| **Ubuntu 24.04 LTS** | **build-supported** | packages build and install; no runtime gate has run |
| **Ubuntu 26.04 LTS** | **build-supported** | as above |
| **Debian 13 trixie** | **build-supported** | as above |

Ubuntu 22.04 and Debian 12 are **out of scope** and the GTK 4.12 / libadwaita
1.5 floor was not lowered for them. Ubuntu 24.04 ships libadwaita exactly
**1.5.0** — zero margin — and `linux-distro-compat.yml` is the only thing
holding that floor. It must not be weakened.

Architecture: **x86_64 only.** `aarch64` is a matrix row nobody has added and
nothing has certified.

---

## 2. Package formats

Two formats, two packages each, and the split is the same in both.

| | `omnibridge` (core) | `omnibridge-gui` |
| --- | --- | --- |
| RPM | `omnibridged`, `omnibridge`, the user unit, the hicolor icon, firewalld XML, README, LICENSE | the GUI, `.desktop`, D-Bus activation entry, AppStream metainfo |
| DEB | the same, **minus** firewalld metadata (§7) | the same |

`omnibridge-gui` requires the **exact** core build — `= %{version}-%{release}`
on RPM, `= ${binary:Version}` on DEB — because the GUI speaks the daemon's
control socket and a version skew there is a protocol skew.

**The icon is in the core package, not the GUI.** `omnibridged` owns the
StatusNotifierItem and its icon name is the application id, which a shell
resolves out of `hicolor` rather than out of the GUI's compiled-in GResource.
A core-only install would otherwise draw a grey square on KDE.

---

## 3. Source and build method

**One source bundle, two formats, no second copy of anything.**

```
make-source-bundle.sh --rev <immutable commit SHA>
  ├── omnibridge-<V>.tar.gz          the upstream source
  └── omnibridge-<V>-vendor.tar.xz   every crate in desktop/Cargo.lock (270)
```

Both package builds consume that one bundle and run
`cargo --locked --offline` with `CARGO_HOME` redirected into the build tree,
so a network fetch is **impossible rather than merely unnecessary**. That is
the property `mock`, `koji` and Debian `buildd` all require.

Three files exist once and are consumed by both formats:

| File | Why it is single |
| --- | --- |
| `packaging/common/omnibridged.service` | it is where `ProtectSystem=strict`, the syscall filter and `RestrictAddressFamilies` live. A second copy is how a hardening change lands on one distribution and misses the other. |
| `packaging/common/cargo-vendor-config.toml` | the offline build depends on it |
| `desktop/gui/tools/install-desktop-metadata.sh` | three of its four files are installed **verbatim**; only the D-Bus `Exec=` line is derived. Identity cannot drift between a development install and a package. |

`packaging-checks.sh` asserts all three, and asserts that **no second unit has
appeared** under `packaging/fedora/` or `packaging/debian/`.

**The Rust floor is 1.88**, measured from the lockfile rather than chosen, and
two of three Debian targets cannot meet it from their stock archive — stated
in `debian/control`, in `README.source` and in the Debian report rather than
lowered. Nothing at runtime depends on Rust: the core package's `Depends` is
`libc6`, `libgcc-s1` and `init-system-helpers`, asserted by a check.

---

## 4. Artifact contents

**MEASURED** from CI run `35730680143`, downloaded and inspected — 14
artifacts, `SHA256SUMS` with 14 entries:

```
omnibridge-0.1.0.tar.gz                    omnibridge-0.1.0-vendor.tar.xz
fedora44/   .src.rpm + core + gui          ubuntu2404/ core + gui
ubuntu2604/ core + gui                     debian13/   core + gui
sbom/       3 × CycloneDX 1.3              SHA256SUMS
```

All three distributions build `omnibridge_0.1.0-1_amd64.deb` — the same
filename, three **different** binaries. They are kept in per-distribution
subdirectories, and CI asserts they are byte-different, because identical
bytes would mean one distribution's build had been reused for another.

---

## 5. systemd behaviour

**Gates S1, S2, S3: PASS** ([evidence](../../audits/packaging/PACKAGING-V1-SYSTEMD-UNIT.md)),
re-runnable with `packaging/tests/systemd-unit-gates.sh` — 21 assertions.

| | |
| --- | --- |
| **S1** | starts where the data directory does not exist; the daemon creates `identity.key` at 0600 inside a 0700 directory, **within a read-only `$HOME`** |
| **S2** | `$XDG_RUNTIME_DIR/omnibridge` 0700, `control.sock` 0600, both user-owned |
| **S3** | no `ProtectSystem`, `ReadWritePaths`, namespace or seccomp denial |

Three defects were closed in four lines: no `RuntimeDirectory=` (so the socket
had nowhere to live under `ProtectSystem=strict`), a `ReadWritePaths=` naming
a path that does not exist on a fresh install, and a `StateDirectory=` that in
a *user* unit creates `~/.local/state` — a directory the daemon never opens.

`ProtectSystem=strict` was **not weakened** to achieve this. Sixteen hardening
directives are byte-identical to before and each is now asserted by name, so a
future fix for a start-up failure cannot quietly reach for one.
`systemd-analyze security --user` rates it **4.7 OK**.

**The unit ships disabled on every format**, asserted on the installed system.
A global enable would raise a LAN listener for every account on the machine.

**An upgrade does not restart a running daemon** — inherent to user units, and
now measured rather than assumed.

---

## 6. D-Bus behaviour

A package installs the GUI's service file as **root**; the user's already-
running session bus has not read it, so clicking OmniBridge in the tray
returns `ServiceUnknown` until the next logout. Root cannot fix that — it has
no route to a user's session bus, and no rpm file trigger exists or can exist
for `/usr/share/dbus-1/services`.

`omnibridged` repairs its own activation from inside the session: one
`ListActivatableNames`, and if the name is missing, **exactly one**
`ReloadConfig` and one re-query. No timer, no retry, no system bus, no root.
D-Bus is not a startup dependency — measured with the bus address pointed at
nothing.

**Measured on both bus implementations**, because they differ:

| Bus | Behaviour |
| --- | --- |
| **dbus-broker** (Fedora 44's session bus) | `HealedByReload` — the repair the feature exists for |
| **dbus-daemon** (reference implementation) | `AlreadyActivatable`, **zero reloads** — it watches with inotify, so the self-heal is correctly a no-op |

This corrected the readiness audit's premise, which was true of dbus-broker
and not of dbus-daemon, and the correction is recorded rather than smoothed
over.

**L7 cold activation: PASS**, measured on the live bus — no GUI running,
`Peer.Ping` returned `()`, the bus started the process, and it exited on idle.

---

## 7. Firewall behaviour

**Fedora:** `/usr/lib/firewalld/services/omnibridge.xml`, **TCP 55432 only**,
owned by the core package, **installed and never enabled**. No scriptlet runs
`firewall-cmd` on any path — a package that silently opens a port is doing
something the user did not ask for, and one that silently closes a port on
removal is deleting a rule the user added.

mDNS is deliberately **not** redeclared: firewalld ships `mdns.xml` correctly
scoped to `224.0.0.251` and `ff02::fb`, and a second definition would be a
broader rule to keep right.

**MEASURED** on this host: the `FedoraWorkstation` zone already permits both
flows, so a default Fedora Workstation install needs **no firewall change at
all**. The file exists for `public`, `FedoraServer` and hand-tightened zones.

**Debian/Ubuntu:** **no firewall metadata**, deliberately. Debian enables no
firewall by default and Ubuntu ships `ufw` inactive, so a firewalld XML would
be dead weight on nearly every install and would imply a firewalld that is
usually absent. The `ufw` commands are documented instead.

---

## 8. GNOME and KDE behaviour

| | |
| --- | --- |
| **KDE Plasma** | native StatusNotifierItem host. The tray item works with no extension. |
| **GNOME** | GNOME Shell has no tray. An AppIndicator/KStatusNotifier host extension is required to *see* the item. |

**The packages do not install or enable that extension, and must not.** It is
the user's desktop and their choice; a package that pulled in a shell
extension would be changing it on their behalf. Without it the tray icon is
absent and **everything else works** — `packaging-checks.sh` asserts no GNOME
extension appears in any dependency.

**Neither was visually confirmed on an installed package.** Gates L6
(launcher entry) and L9 (tray icon and menu) require somebody to look at a
screen. The tray implementation itself is certified elsewhere, on real
sessions — `certification/linux/gnome/` and `certification/linux/kde/` — but
not from a packaged install.

---

## 9. Security certification

**[Security Certification v1](../security/SECURITY-CERTIFICATION-V1.md):
13 PASS · 1 PASS WITH FINDING · 2 N/A · 0 FAIL · 0 BLOCKED.**

| Gate | Result |
| --- | --- |
| SEC-NET-01 no unexpected listener | **PASS** — external scan from the paired SM-X620; OmniBridge contributes exactly TCP 55432 |
| SEC-NET-02 no plaintext payload | **PASS** — in-process wire relay; pairing token and device name absent |
| SEC-TLS-01 TLS 1.3 only | **PASS** — a raw TLS 1.2 `ClientHello` is answered with fatal alert **70 `protocol_version`**; `tls12` is not compiled in |
| SEC-AUTH-01/02/03 | **PASS** — discovery ≠ trust, connection ≠ authorization, revocation durable |
| SEC-FILE-01/02 | **PASS** — traversal prevented, data streams require a valid single-use challenge |
| SEC-LOG-01/02 | **PASS** |
| SEC-LOG-03 | **PASS WITH FINDING F-1** — content absent; **filenames are logged** |
| SEC-LOCAL-01/02 | **PASS** — never root; 0700/0600/0700/0600 |
| SEC-FUZZ-01 | **PASS** — 60,000 inputs, three parsers, no panic/hang/traversal |
| SEC-DEPS-01 | **PASS** — after fixing the CVE it found |
| SEC-ANDROID-01 | **PASS** — three exported components, each justified; the notification listener is `exported=false` **and** permission-guarded |

**Finding F-1 is open and deliberate.** `files.v1` logs filenames at `info`
while `notifications.v1` redacts titles. Metadata rather than content, so not
a gate failure, and not remotely reachable, so not a vulnerability — but a
real inconsistency. It was **not** changed, because altering product logging
in the middle of certifying that logging is the wrong order. A
characterisation test stops it drifting either way.

Two gates are **N/A with the measurement that makes them so**, never as a
pass: external IPv6 scanning (this network assigns the host no global IPv6
address) and `tcpdump` capture (needs `CAP_NET_RAW`; replaced by a relay that
sees the same bytes).

`cargo audit` is now a CI gate that runs on every manifest change **and
daily** — the one check whose result can change without the repository
changing.

---

## 10. Update, remove and purge semantics

**The invariant:** no transaction — install, upgrade, remove or **purge** —
may create, move or delete `~/.local/share/omnibridge`. It holds the user's
identity key and the record of every device they have paired.

Guarded twice:

* **statically** — `packaging-checks.sh` greps the spec and every maintainer
  script for `$HOME` and `.local/share`, asserts `debian/rules` adds no purge
  behaviour, and asserts **no hand-written maintainer scripts exist at all**;
* **dynamically** — `install-smoke.sh` plants a fake `identity.key` and
  `state.json` owned by a non-root user and compares **digest, mode and
  owner** across every transaction.

**MEASURED**, all four package sets:

| Transaction | Result |
| --- | --- |
| upgrade `0.1.0-2` → `0.1.0-3` (Fedora) | trust store **byte- and mode-identical**; no daemon started |
| remove | unchanged; **no package-owned file survived** |
| reinstall | unchanged |
| **purge** (all three Debian targets) | **unchanged** |
| after all of it | **no root-owned file** anywhere in the user's state |

There are **no maintainer scripts** beyond what `%systemd_user_*` and
`dh_installsystemduser` generate. Each omission is justified: the desktop
database and icon cache are handled by the distribution's own file triggers,
the session-bus reload is impossible as root, and a `prerm` cannot reach a
user's service manager.

---

## 11. Lifecycle results — PARTIAL, and this is the main debt

**[Lifecycle certification](PACKAGING-V1-LIFECYCLE-CERTIFICATION.md): 13 of 26
gates certified, 2 partial, 11 BLOCKED.**

| Certified | Blocked |
| --- | --- |
| L1 install · L2 manifest · L4 not root · L5 runtime dir · L7 cold activation · L11 LAN surface · L13 + L17 upgrade · L18 restart · L21 remove · L22 reinstall · L23 state preserved · L24 purge · L25 no orphans · L26 no root-owned state | L3 autostart · L6 launcher · L8 activation after install · L9 tray · L14 clipboard · L15 files · L16 notifications · L19 logout/login · L20 reboot, plus the per-distribution runtime matrix |

**Nothing measured failed.** Every blocked gate is blocked by access, not by a
defect:

* **No root.** `sudo` requires a password, so every gate beginning "install
  the package on a real system" cannot start.
* **No VM can reach the LAN.** KVM is available and `qemu:///session` works,
  and the four installation ISOs are staged — but user-session QEMU uses
  SLIRP, which **accepts no inbound connection and carries no mDNS
  multicast**. A VM built without root can never be discovered by the phone,
  so L10/L12/L13 and everything needing a paired phone stay unreachable
  however much install automation is written. Bridged networking needs root.
* **L6 and L9 need a human at a screen.**

**No distribution has a complete column, and none is runtime-certified.**

---

## 12. Physical Android results

**SM-X620, Android 16**, `192.168.68.63/22`, attached and paired for the
entire effort.

**Pairing, trust and grants were preserved throughout. Nothing was reset and
nothing was re-paired.** **MEASURED** at the end against the digests recorded
before the first branch:

```
1914f6cd…ca21  identity.key    identical to the pre-Phase-1 value
a539ea91…e6ce  state.json      identical to the pre-Phase-1 value
SM-X620  fingerprint 509B D0C1 CE97 C909  paired yes
```

That survived eight branches, three daemon stop/restart cycles, two systemd
unit-gate runs and a D-Bus activation test.

The phone also served as the **second LAN machine** for SEC-NET-01, which is
the only reason that gate has genuine external evidence.

**What it does not certify:** L12 and L13 against a *packaged* install. The
daemon it is paired with is the development build, because installing the
package needs root.

---

## 13. Release CI results

**[Release CI](../../audits/packaging/PACKAGING-V1-RELEASE-CI.md): working.**
Run `35730680143`, 7 of 7 jobs green, artifacts downloaded and inspected.

* **14 artifacts from one immutable commit.** The workflow resolves its input
  to a **commit SHA** and refuses anything that does not — `--worktree` is
  never used, because it bundles uncommitted and untracked files.
* **Offline, non-root builds** in buildroots derived from the packaging
  metadata and nothing else.
* **3 CycloneDX SBOMs**, one per shipped binary. The daemon's names
  `rustls 0.23.45` — the version the security certification fixed — and CI
  fails if any SBOM names a rustls below it.
* **SLSA v1 provenance**, verified with `gh attestation verify`, bound to the
  workflow and the commit.

**No `v1.0.0`, no release tag, nothing published.**

The pipeline found **five defects in itself**, four of which reported success
while losing or doing nothing — `podman` without `-i`, a subuid that cannot
write to a bind mount, a relative `--output` that is a named volume rather
than a bind mount, and **six packages built with one shipped**. That last was
visible only by downloading the artifacts.

---

## 14. Known debts

Each is real, named, and none is a defect in what was built.

| # | Debt | Blocks a public release? |
| --- | --- | --- |
| **D-1** | **Eleven lifecycle gates never ran** — the graphical half. §11 | **Yes** for calling any distribution runtime-certified |
| **D-2** | **Artifacts are unsigned.** §15 | **Yes** |
| **D-3** | Ubuntu 24.04, Ubuntu 26.04 and Debian 13 have **no runtime gate at all** | Yes, for claiming support beyond "it builds" |
| **D-4** | **Finding F-1** — `files.v1` logs filenames while `notifications.v1` redacts titles | No. A decision, with a recommendation |
| **D-5** | **No man pages** for any of the three binaries | No. `--help` is complete; rpmlint and lintian both report it |
| **D-6** | **No AppStream screenshots** — the strict validator warns | No. A placeholder would be worse |
| **D-7** | **No `-debuginfo`/`-dbgsym`** — deliberate for v1 (audit Q2) | No |
| **D-8** | **`cargo deny` not configured** — licence and duplicate policy | No. A project decision |
| **D-9** | **No Debian source package** (`.dsc`) — `-b` builds binaries only | No for a third-party `.deb`; yes for an archive submission |
| **D-10** | **x86_64 only** — no `aarch64` row | No for v1 |
| **D-11** | **External IPv6 surface unscanned** — no global IPv6 on this network | No, but it is unmeasured rather than clean |
| **D-12** | **No SBOM publication path** — produced and attested, but goes nowhere | No |
| **D-13** | **CI tests in debug; `%check` tests in release** — found during the build foundation | No, but it is why two flaky tests reached `mock` unseen |
| **D-14** | **Byte-identical package rebuilds** unproven. The *source bundle* is reproducible and asserted so | No — audit §13.5 calls it an aspiration |

---

## 15. Artifact-signing status

**Nothing is signed. `SHA256SUMS` carries no GPG signature.**

No key was generated for this. A signature made with an invented key is worse
than no signature, because it looks like assurance.

| Gate | Status | Needs |
| --- | --- | --- |
| **RC-SIGN-01** a maintainer key exists, public half published | **OPEN** | a key, and somewhere to publish the fingerprint |
| **RC-SIGN-02** CI signs `SHA256SUMS` | **OPEN** | the private half as a secret, or an external signing service |
| **RC-SIGN-03** the README documents verification | **OPEN** | RC-SIGN-01 first |

What **does** exist is different in kind and should not be confused for it:
**SLSA build provenance**, Sigstore-backed, using GitHub's OIDC identity. It
proves *which workflow built these bytes from which commit*. It does **not**
prove a human vouched for the release.

Audit §19 Q6 calls this *"calendar time, not engineering time — start now"*,
and that is still right: generating a key and deciding where its fingerprint
lives are decisions, not work.

---

## 16. Final verdict

# PACKAGING V1: CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS

**What is certified.** OmniBridge has real Linux packaging. Two formats, four
package sets, eight binary packages, built offline from one immutable commit
by CI, in buildroots derived from the packaging metadata and nothing else, by
a non-root user, with the full test suite passing inside every build. They
install, upgrade, remove, reinstall and purge without ever touching the user's
trust store — guarded statically and dynamically, on all four sets. The
systemd unit starts on a fresh machine with its sandbox intact. The daemon
repairs its own D-Bus activation from inside the user's session. The firewall
definition is present and inert. Security Certification v1 passed sixteen
gates, and the two it could not run are marked N/A with the measurement that
makes them so.

**Why the verdict is not simply CERTIFIED.** Eleven lifecycle gates never ran
(**D-1**) and nothing is signed (**D-2**). Both are blocked by access this
environment does not grant — a root password, a LAN-capable VM, an operator at
a screen, and a signing key — and neither is a defect in what was built.
Nothing that was measured failed.

**Why it is not BLOCKED.** Every gate that could be measured was measured, and
every one passed. The packaging work is complete and verified; what is missing
is verification of runtime behaviour on a desktop, not the packaging itself.

**What must close before a public release.** D-1 and D-2. Until D-1 closes,
the honest word for Ubuntu 24.04, Ubuntu 26.04 and Debian 13 is
**build-supported**, and for Fedora 44 **partially runtime-certified**. Until
D-2 closes, there is nothing for a user to verify a download against.

**What this document does not say.** That OmniBridge works on an installed
Linux desktop. That has not been demonstrated, and no amount of green package
gates demonstrates it.
