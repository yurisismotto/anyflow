# 04 — Linux portability: from "Fedora" to "Linux"

| Field | Value |
| --- | --- |
| **Title** | Turning a Fedora implementation into a Linux platform |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Everything OmniBridge assumes about Linux, separated into *Linux requirement* vs. *Fedora implementation detail*. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for every assumption; OFFICIAL DOC VERIFIED for distro package versions. |
| **Related documents** | [01](01-CURRENT-ARCHITECTURE-AUDIT.md), [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md), [06](06-KDE-PLASMA-WAYLAND.md), [07](07-LINUX-PACKAGING.md), [17](17-BACKGROUND-EXECUTION-MODEL.md) |

---

## 1. The core claim

Almost nothing in OmniBridge is Fedora-specific. What exists is a *Fedora-shaped assumption
set*, and it is short. The certification was done on Fedora + GNOME Wayland
(`CLIPBOARD-V1-CERTIFICATION.md`), which is why the docs read as Fedora-only; the code is
broader than its documentation.

The honest exception is the **GUI's minimum GTK/libadwaita versions**, which are a real
constraint on which distributions can run OmniBridge at all. That is [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md).

---

## 2. Assumption inventory

| # | Assumption | Where | Linux requirement or Fedora detail? |
| --- | --- | --- | --- |
| A1 | `rustc >= 1.82`, edition 2021 | `desktop/Cargo.toml`, `packaging/fedora/omnibridge.spec` | **Linux requirement**, but a *distro-version* constraint. §3 |
| A2 | A C toolchain (`gcc`) at build time | `omnibridge.spec` `BuildRequires: gcc` | **Fedora detail, and probably wrong.** §4 |
| A3 | No `protoc` needed (protox) | `desktop/proto/Cargo.toml` | **Linux-neutral asset.** Applies to every platform. |
| A4 | `rustls`+`ring`, no OpenSSL | `core/Cargo.toml` | **Linux-neutral asset.** §5 |
| A5 | mDNS via the in-process `mdns-sd` responder, not Avahi | `daemon/src/mdns.rs` | **Linux-neutral asset**, with a coexistence question. §6 |
| A6 | `$XDG_DATA_HOME` / `$XDG_RUNTIME_DIR` / `$XDG_DOWNLOAD_DIR` | `store.rs`, `control.rs`, `destination.rs` | **Linux requirement.** Correctly implemented. §7 |
| A7 | `/etc/hostname` for the device name, fallback `"Fedora"` | `store.rs:108` | **Fedora detail.** §7 |
| A8 | `/proc/self/status` for the uid | `control.rs:373` | **Linux requirement** (procfs). Acceptable; noted. |
| A9 | systemd, and a `systemd --user` session | `omnibridged.service` | **Mostly Linux requirement**, with a real non-systemd tail. §8 |
| A10 | GTK 4.12 + libadwaita 1.5 | `gui/Cargo.toml` features `v4_12`, `v1_5` | **Linux requirement, and the binding one.** §9 |
| A11 | Wayland, detected by `WAYLAND_DISPLAY` | `clipboard/backend/mod.rs:176` | **Linux requirement**, incomplete for X11. §10 |
| A12 | `wl-copy` / `wl-paste` on `PATH` | `backend/wayland.rs` | **Linux requirement**, an external runtime dependency. §10 |
| A13 | Mutter implements no data-control protocol | ADR-0014, `backend/wayland.rs` | **GNOME detail**, not Linux. → [06](06-KDE-PLASMA-WAYLAND.md) |
| A14 | UPower on D-Bus for the local battery | `capabilities/battery/src/upower.rs` | **Linux requirement**, already optional. |
| A15 | RPM is the packaging format | `packaging/fedora/` | **Fedora detail.** → [07](07-LINUX-PACKAGING.md) |
| A16 | No `.desktop` file, no icon, no autostart, GUI unpackaged | `omnibridge.spec` `%files` | **Gap on all Linux.** §11 |

---

## 3. A1 — Rust version

`rust-version = "1.82"` in the workspace; `rust-toolchain.toml` says `channel = "stable"`, so
a developer with `rustup` always satisfies it. Distro builds are the constraint:

| Distro | `rustc` in the archive | Meets 1.82? |
| --- | --- | --- |
| Fedora (current) | tracks stable closely | ✅ |
| Debian 12 bookworm | 1.63.0 | ❌ |
| Debian 13 trixie (stable) | 1.85.0 | ✅ |
| Debian 13 trixie-backports | 1.94.1 | ✅ |
| Debian forky / sid | 1.95.0 | ✅ |
| Ubuntu 22.04 LTS | default 1.75; `rustc-1.82` available | ⚠️ named package only |
| Ubuntu 24.04 LTS | default 1.75; `rustc-1.82` … `rustc-1.91` available | ⚠️ named package only |
| Ubuntu 26.04 LTS | 1.93.1 | ✅ |

(OFFICIAL DOC VERIFIED — packages.debian.org / packages.ubuntu.com, accessed 2026-08-31.)

The Ubuntu LTS case is the interesting one: the *default* `rustc` is too old, but the archive
carries versioned `rustc-1.8x` packages. A Debian/Ubuntu build recipe must
`Build-Depends: rustc-1.82 | rustc (>= 1.82)` rather than plain `rustc`, or use the upstream
toolchain. Recorded as **LINUX-002** in [25](25-IMPLEMENTATION-BACKLOG.md).

**Recommendation:** keep `rust-version` as a real MSRV and raise it only deliberately. It is
the one number that decides whether a distro can build OmniBridge from its own archive.

---

## 4. A2 — is `gcc` actually needed?

`omnibridge.spec` declares `BuildRequires: gcc`. The dependency tree suggests it may not be:

- `rustls` uses the **ring** provider. `ring` historically needs a C compiler *and* an
  assembler for its primitives. This is the likely reason `gcc` is there.
- `x11rb` is configured `default-features = false, features = ["xfixes"]` — its own comment
  says this is *"Pure Rust (x11rb's own connection backend), so it adds no C toolchain
  requirement to a build that had none."* Which implies the author believed the build had
  none.
- `protox` removes the `protoc` requirement, explicitly.
- `glib-build-tools` shells out to `glib-compile-resources` — a *tool* dependency, not a
  compiler one, and only for the GUI.

So `gcc` is probably present for `ring` alone. **This matters** because it is the difference
between "OmniBridge builds with a Rust toolchain" and "OmniBridge needs a full C toolchain on every
platform, including a Windows MSVC install". It is cheap to settle empirically and nobody
should guess:

```
# Verification, not part of this sprint:
cargo build --release -p omnibridge-daemon   # in a container with rust but no cc
```

If `ring` is the only reason, the alternative is `rustls`'s `aws-lc-rs` provider (also needs
C) or the pure-Rust `rustls-rustcrypto` (not a rustls-project-supported provider, and swapping
the crypto provider is a security-relevant change that this sprint must not propose lightly).
**Recommendation: keep `ring`, document the C-toolchain requirement honestly, and stop
treating it as a Fedora packaging quirk.** Tracked as **LINUX-001**.

---

## 5. A4 — rustls instead of OpenSSL is a portability asset

Worth stating because it is the kind of decision that is invisible until it saves a month.

`core/Cargo.toml`: `rustls = { default-features = false, features = ["std", "ring"] }`, and
`daemon/src/main.rs` installs the provider explicitly *"so that the choice of backend is
visible in the source and cannot be changed by a transitive dependency's feature flag."*

Consequences for the expansion:
- No dependency on the distro's OpenSSL version, its ABI breaks, or its policy files
  (Fedora's crypto-policies could otherwise disable something under us).
- No system trust store is consulted — OmniBridge's trust *is* the pin, so there is nothing to
  port.
- The same TLS stack on Windows and macOS, which means the pinning verifier is proven once.

The one thing to watch: `rustls` moves faster than a distro release. A distro packaging
OmniBridge with `cargo build --locked` gets `Cargo.lock`'s versions; a distro that unbundles
Rust crates (Debian does, for `librust-*` packages) may not. That is a real Debian packaging
consideration → [05 §6](05-DEBIAN-UBUNTU-COMPATIBILITY.md).

---

## 6. A5 — mDNS without Avahi

`mdns-sd` runs its own responder thread. **OmniBridge does not use Avahi, and does not depend on
it.** That is unusual for a Linux desktop application and it is the right call for a
cross-platform product: `daemon/src/mdns.rs` works the same way on every OS the crate
supports (upstream README: *"supports macOS, Linux and Windows"*).

Two consequences that are Linux-specific and must be checked:

1. **Coexistence with the system responder.** Nearly every Linux desktop runs Avahi bound to
   UDP 5353. Two responders on one host can both work (mDNS is designed for multiple
   responders on a link, and `SO_REUSEADDR`/`SO_REUSEPORT` allow the bind) but it is not
   free: duplicate-name probing, conflicting `.local` hostname claims
   (`mdns.rs` registers `"{device_id}.local."`), and reflector behaviour on some networks.
   The daemon has run this way on Fedora + Avahi through certification, which is meaningful
   evidence — but it is one distro's Avahi configuration. **POC-LINUX-01/02** should
   explicitly check discovery on Debian and Ubuntu with their default Avahi.
2. **Firewalld/ufw.** Fedora's firewalld default zone blocks inbound TCP on 55432 and, in
   some configurations, mDNS. Debian ships no firewall enabled by default; Ubuntu ships `ufw`
   installed but inactive. So a first-run experience that "just works" on Ubuntu may need a
   firewalld rule on Fedora — the opposite of what one would guess. Recorded as **LINUX-004**.

---

## 7. A6/A7 — XDG is right, `/etc/hostname` is not

The XDG handling is genuinely good and needs no change. `destination.rs` resolves Downloads
through `$XDG_DOWNLOAD_DIR` → `user-dirs.dirs` → `$HOME/Downloads`, explicitly so a localised
desktop gets `~/Transferências` rather than a hardcoded English path, and it refuses to run a
shell to expand the config file. `store.rs` uses `$XDG_DATA_HOME` else `~/.local/share`.
`control.rs` uses `$XDG_RUNTIME_DIR` with a `/tmp/omnibridge-<uid>` fallback it creates 0700.

The device name is the exception:

```rust
fn default_device_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok() … .unwrap_or_else(|| "Fedora".to_string())
}
```

Two problems. `/etc/hostname` is not universally the live hostname — systemd's transient
hostname (`hostnamectl --transient`) and a DHCP-supplied hostname can differ from the file,
and containers frequently have neither. And the fallback string is a distribution name, which
will read as wrong on Debian, on KDE, and absurd on Windows.

**Recommendation:** `gethostname(2)` (via `rustix` or `nix`, or `sd_booted`-free plain libc)
with a neutral fallback such as `"OmniBridge Desktop"`, and on other platforms the native call
(`GetComputerNameExW`, `SCDynamicStoreCopyComputerName`). Trivial; user-visible on the peer's
screen. **LINUX-003**.

---

## 8. A9 — systemd, and the honest size of the non-systemd tail

`omnibridged.service` is a `systemd --user` unit and is unusually well hardened:
`NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=read-only` with one `ReadWritePaths`,
`SystemCallFilter=@system-service` minus `@privileged @resources @obsolete`,
`RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK`, `MemoryDenyWriteExecute`.

That hardening is a genuine security control and it is **not portable to anything** — not to
other init systems, not to Windows, not to macOS. Losing it silently on another platform
would violate the sprint's security principle, so [20](20-SECURITY-THREAT-ANALYSIS.md) tracks
"platform sandboxing parity" as an explicit risk rather than pretending it transfers.

For Linux itself:

- **systemd distros** (Fedora, Debian, Ubuntu, openSUSE, Arch): the unit works as-is. The
  only per-distro question is whether `%h` and `StateDirectory=` behave identically, which
  they should.
- **Non-systemd** (Devuan, Artix, Void, Alpine, Gentoo/OpenRC, Chimera): no `systemd --user`,
  therefore no supervisor and no `$XDG_RUNTIME_DIR` guarantee. The daemon itself needs
  nothing from systemd — it is an ordinary unprivileged process — so the fallback is an XDG
  autostart entry, which also covers "user has no systemd but has a desktop".

**Recommendation:** ship **both** a systemd user unit and an XDG autostart `.desktop` file,
and make the daemon tolerate a missing `$XDG_RUNTIME_DIR` (it already does). Do **not** invest
in OpenRC/runit service files until someone asks. Recorded as **LINUX-005**, **PKG-003**.

`RestrictAddressFamilies` deserves a note for the expansion: it includes `AF_NETLINK`, which
`mdns-sd` needs to watch interface changes (`enable_addr_auto`). Any future tightening of
that list will silently break discovery across a Wi-Fi/dock change — the exact failure the
`Advertisement::publish` doc comment warns about.

---

## 9. A10 — the GTK/libadwaita floor is the real portability limit

`desktop/gui/Cargo.toml`:

```toml
gtk = { package = "gtk4", version = "0.9", features = ["v4_12"] }
adw = { package = "libadwaita", version = "0.7", features = ["v1_5"] }
```

In `gtk4-rs`/`libadwaita-rs`, a `vX_Y` feature means "compile against, and require at runtime,
at least that version". So the declared floor is **GTK 4.12 and libadwaita 1.5**
(REPO VERIFIED).

The API inventory backs it up. Widgets actually used:

- libadwaita: `AlertDialog`, `Dialog`, `NavigationSplitView`, `NavigationPage`, `ToolbarView`,
  `Breakpoint`, `BreakpointCondition`, `LengthUnit`, `HeaderBar`, `StyleManager`,
  `ApplicationWindow`, `Application`, `ResponseAppearance`.
- GTK: `FileDialog`, `Picture`, `DrawingArea`, `ListBox`, `Stack`, `ProgressBar`, `Switch`,
  `CssProvider`, `AccessibleRole`.

`AdwAlertDialog` and `AdwDialog` are the **libadwaita 1.5** adaptive-dialog generation that
replaced `AdwMessageDialog` (OFFICIAL DOC VERIFIED — libadwaita 1.5 release notes and the
"Migrating to Adaptive Dialogs" guide). `AdwNavigationSplitView`, `AdwNavigationPage`,
`AdwToolbarView` and `AdwBreakpoint` are the 1.4 adaptive generation. `GtkFileDialog` is
GTK 4.10.

So **libadwaita 1.5 is the binding floor and it is load-bearing** — `AlertDialog` is used for
the pairing confirmation, which is a security-critical UI element, not a decoration. Lowering
the floor would mean rewriting that dialog against `AdwMessageDialog` (1.2+), which would
widen distro support by exactly one Debian release. Analysed in
[05 §5](05-DEBIAN-UBUNTU-COMPATIBILITY.md); recommendation there is **do not lower it**.

The daemon and CLI have **no GTK dependency at all**. A distro that cannot satisfy the GUI's
floor can still ship `omnibridged` + `omnibridge` and get every capability except the graphical
interface. That is a genuinely useful degradation path and the packaging should be built to
allow it (separate `omnibridge` and `omnibridge-gui` binary packages) → [07](07-LINUX-PACKAGING.md).

---

## 10. A11/A12 — display server and the clipboard helper

`backend/mod.rs::detect()` branches on `WAYLAND_DISPLAY`, then reports X11 as explicitly
unimplemented:

> *"this is an X11 session; clipboard.v1 currently implements the Wayland backend only (the
> X11 watch exists, read/write does not)"*

This is honest and correct behaviour — the capability registers, answers peers with `FAILED`,
and tells the local user exactly why. But it does mean **OmniBridge's clipboard does not work on
an X11 session at all**, which still matters:

- Ubuntu's GNOME session on hardware with the proprietary NVIDIA driver has historically
  defaulted to Xorg;
- KDE Plasma still ships an X11 session;
- remote desktops (X2Go, some VNC/RDP setups) are X11;
- `XDG_SESSION_TYPE=x11` inside a VM without Wayland support — which is exactly what a CI
  runner or a test VM looks like.

The last one matters for this expansion specifically: **a test lab built from VMs may not be
able to exercise the clipboard at all** unless Wayland works in the VM. See
[21](21-POC-MASTER-PLAN.md) and the hardware notes in [22](22-IMPLEMENTATION-ROADMAP.md).

Completing the X11 backend is not large — `x11rb` is already a dependency and the XFIXES watch
already exists; what is missing is selection ownership (read/write), which is the same "stay
alive as the selection owner" problem `wl-copy` solves on Wayland. Recorded as **LINUX-006**,
priority medium, and *not* on the critical path for Windows or macOS.

`wl-clipboard` as a runtime dependency: `omnibridge.spec` does **not** declare it, not even as a
`Recommends:` (only `Recommends: upower` is there). The backend degrades gracefully and names
the Fedora package in its error message — which is friendly on Fedora and unhelpful on
Debian. Two fixes, both small: declare the dependency in each package, and make the message
name the *binary* rather than a distro-specific install command. **PKG-004**.

---

## 11. A16 — the desktop-integration gap

`%files` in the RPM spec contains: `omnibridged`, `omnibridge`, `omnibridged.service`, `LICENSE`,
`README.md`, `docs/`. That is all.

Missing, on **every** Linux distribution, not just Fedora:

| Missing | Consequence |
| --- | --- |
| `omnibridge-gui` binary | The GUI cannot be installed from a package at all |
| `.desktop` entry | No application menu entry, no icon, no Sharesheet-equivalent |
| Icon theme install (`hicolor`) | No icon anywhere |
| AppStream metainfo (`.metainfo.xml`) | Invisible to GNOME Software / KDE Discover |
| XDG autostart entry | GUI/daemon does not start at login without `systemctl --user enable` |
| `wl-clipboard` dependency | Clipboard silently unavailable on a fresh install |

This is pre-existing Linux debt, not expansion work — but it is on the critical path for
"Linux is a platform, not a dev machine", and it is cheap. [07](07-LINUX-PACKAGING.md) covers
it; **PKG-001** and **PKG-002** in the backlog.

---

## 12. Recommended direction

1. Replace `/etc/hostname` + `"Fedora"` with `gethostname` + a neutral fallback. (**LINUX-003**)
2. Settle the `gcc` question empirically and document the answer. (**LINUX-001**)
3. Make `x11rb` an optional feature, so non-Linux builds do not pull it. (**ARCH-004**)
4. Ship `.desktop`, icon, AppStream metainfo and an autostart entry; package the GUI.
   (**PKG-001**, **PKG-002**)
5. Split packaging into `omnibridge` (daemon+CLI) and `omnibridge-gui`, so distros below the
   libadwaita floor can still ship the useful half. (**PKG-005**)
6. Declare `wl-clipboard` per distro and make the "not installed" message distro-neutral.
   (**PKG-004**)
7. Verify Avahi coexistence and default firewall behaviour on Debian and Ubuntu.
   (**POC-LINUX-01**, **POC-LINUX-02**)
8. Complete the X11 read/write backend — medium priority, off the critical path.
   (**LINUX-006**)

None of these change the protocol, the security model, or any capability.
