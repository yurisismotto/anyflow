# 07 — Linux packaging

| Field | Value |
| --- | --- |
| **Title** | How AnyFlow should be delivered on Linux |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | RPM, DEB, tarball, Flatpak, AppImage, Snap — assessed against what AnyFlow actually needs from the host. |
| **Decision status** | PROPOSED. **PLAT-DEC-007** (Flatpak viability) is OPEN and POC-gated. |
| **Evidence** | REPO VERIFIED for current packaging; OFFICIAL DOC VERIFIED for Flatpak sandbox behaviour; POC REQUIRED where marked. |
| **Related documents** | [04](04-LINUX-PORTABILITY.md), [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md), [19](19-PACKAGING-AND-DISTRIBUTION.md), [20](20-SECURITY-THREAT-ANALYSIS.md) |

---

## 1. What AnyFlow needs from the host

Packaging formats are usually compared on convenience. That is the wrong axis here. AnyFlow
is a **deep desktop-integration application**: it owns the system clipboard, runs
continuously, binds a listening TCP port, speaks multicast DNS, writes into the user's
Downloads folder, and starts at login. A format that sandboxes any one of those away does not
"work with a caveat" — it removes a capability.

So the comparison axis is: **what does the sandbox take away?**

| Requirement | Why | Sandbox-sensitive? |
| --- | --- | --- |
| Spawn `wl-copy` / `wl-paste` from `PATH` | The only way to read/write a Wayland clipboard | **Yes** — helpers must exist *inside* the sandbox |
| Connect to Xwayland `$DISPLAY` + XFIXES | The GNOME clipboard watch (ADR-0014) | **Yes** — needs `--socket=x11` or fallback |
| Bind TCP 55432, IPv4 + IPv6 | The listener | Yes — `--share=network` |
| Send/receive multicast on UDP 5353 | The in-process `mdns-sd` responder | **Yes**, and subtly — §4 |
| Unix socket in `$XDG_RUNTIME_DIR` | CLI/GUI ↔ daemon control | Yes — path differs inside a sandbox |
| Write to the user's real Downloads dir | `files.v1` receive | **Yes** — the whole point of a portal |
| 0700 dir / 0600 key with mode enforcement | `store.rs` refuses to start otherwise | Yes — must survive the sandbox's filesystem mapping |
| Run at login, in the background, no window | The agent model | **Yes** — needs the Background portal |
| D-Bus to UPower (system bus) | Local battery, optional | Yes — `--system-talk-name` |

Nine requirements, eight sandbox-sensitive. That is the finding that decides this document.

---

## 2. Current state

`packaging/fedora/` contains an RPM spec and a systemd user unit. The spec installs
`anyflowd`, `anyflow` and `anyflowd.service`, plus `LICENSE`, `README.md` and `docs/`.

Gaps, all of them format-independent (repeated from [04 §11](04-LINUX-PORTABILITY.md) because
they are packaging work):

- the **GUI binary is not packaged at all**;
- no `.desktop` entry, so no menu entry and no icon;
- no `hicolor` icon install, though `docs/design/assets/` has the artwork and
  `gui/build.rs` already compiles it into a GResource;
- no AppStream `metainfo.xml`, so AnyFlow is invisible in GNOME Software and KDE Discover;
- no XDG autostart entry — the daemon only starts if the user runs
  `systemctl --user enable --now anyflowd.service`, which `%post` tells them to do;
- `wl-clipboard` is not declared even as a weak dependency, though `upower` is
  (`Recommends: upower`).

Nothing here is hard. All of it is on the critical path for "Linux is a supported platform".

---

## 3. Format-by-format assessment

### 3.1 RPM (Fedora, RHEL, openSUSE)

| Aspect | Assessment |
| --- | --- |
| daemon | ✅ Exists and works |
| autostart | ⚠️ Manual `systemctl --user enable` |
| clipboard | ✅ No sandbox; `wl-clipboard` on host `PATH` |
| network / mDNS | ✅ Unrestricted. ⚠️ firewalld may block inbound 55432 — see §7 |
| D-Bus | ✅ |
| filesystem | ✅ Real `$HOME`, real modes |
| updates | ✅ dnf |
| signing | ✅ Standard RPM/COPR signing |
| desktop integration | ❌ Missing (§2) |
| maintenance | Low |

**Verdict: PRIMARY for Fedora.** The gaps are content, not format.

### 3.2 DEB (Debian, Ubuntu and derivatives)

Same profile as RPM. Build-dependency detail is in [05 §6](05-DEBIAN-UBUNTU-COMPATIBILITY.md).
The only structural difference is Debian's preference for unbundled `librust-*` crates, which
AnyFlow's dependency set makes impractical for a first package (**PLAT-DEC-011**).

**Verdict: PRIMARY for Debian/Ubuntu.** Ship from CI first; pursue archive inclusion later, if
ever.

### 3.3 Tarball / static binary

| Aspect | Assessment |
| --- | --- |
| daemon, clipboard, network, filesystem | ✅ Nothing is restricted |
| autostart | ⚠️ User installs the unit or autostart entry themselves |
| updates | ❌ None |
| signing | ⚠️ Detached signature + checksum only |
| desktop integration | ⚠️ Manual |
| maintenance | Very low |

Genuinely useful for: distributions AnyFlow does not package for, non-systemd distros, test
labs, CI, and users who want to try it without touching the package manager. It should exist
because it costs one CI job.

**Verdict: SECONDARY.** Ship a `.tar.gz` of the release binaries from CI.

### 3.4 Flatpak — the interesting case

Flatpak is the format an application like this is "supposed" to use, and it is the one whose
sandbox collides with the most of §1. Assessing it honestly matters more than picking it.

| Requirement | Under Flatpak |
| --- | --- |
| `wl-copy` / `wl-paste` | **Must be built into the Flatpak.** The host's binaries are not on the sandbox `PATH`. Doable (a small manifest module), but it means AnyFlow ships its own `wl-clipboard`, and a version mismatch with the compositor's protocol (see [06 §4](06-KDE-PLASMA-WAYLAND.md)) becomes AnyFlow's problem rather than the distro's — arguably *better*, since it lets AnyFlow guarantee 2.3.0. |
| Wayland clipboard access | The bundled `wl-copy` needs the Wayland socket: `--socket=wayland`. Should work. **POC.** |
| Xwayland XFIXES fallback | Needs `--socket=x11`, which is a notably broad permission. On GNOME this is the *only* watch source (ADR-0014), so a Flatpak GNOME build cannot do auto-send without it. **This is a real tension: the format's security value is undercut by exactly the permission AnyFlow needs on its primary desktop.** |
| TCP listener | `--share=network` gives host networking. ✅ |
| **mDNS** | The known Flatpak limitation is that **`.local` name resolution does not work in the sandbox** — there is no NSS/Avahi path, and an mDNS portal is still only a discussion upstream (OFFICIAL DOC VERIFIED: flatpak issues #348, #4044; xdg-desktop-portal discussion #1365). **But AnyFlow does not resolve `.local` names.** It runs its own responder (`mdns-sd`) and dials the IP addresses from the DNS-SD record. With `--share=network` the sandbox shares the host network namespace, so multicast on 5353 should work. **This is the single most important unknown about Flatpak for AnyFlow, and it is testable in an hour.** |
| Downloads directory | `--filesystem=xdg-download` grants it. Or the FileTransfer/Documents portal, which would be more idiomatic but is a code change. |
| 0600 key + mode enforcement | The data dir maps to `~/.var/app/<id>/data`. `require_private_mode` should still pass. **POC.** |
| Background / autostart | The **Background portal** (`org.freedesktop.portal.Background`) with `autostart`. This is the supported way and it is a *user-visible permission prompt*, which is arguably correct for a background network daemon. |
| CLI (`anyflow`) | Awkward. `flatpak run --command=anyflow io.github.yurisismotto.AnyFlow status` is not a CLI anyone wants. A wrapper script helps; it is still second-class. |
| UPower | `--system-talk-name=org.freedesktop.UPower`. Optional anyway. |
| Updates, signing, integration | ✅ Flathub does all of this well, and the AppStream metainfo AnyFlow needs anyway is required there. |

**Verdict: EXPERIMENTAL, POC-gated (PLAT-DEC-007).** Flatpak is *plausible* — more plausible
than the "no mDNS in Flatpak" headline suggests, because AnyFlow's discovery does not use the
mechanism that is broken. The blockers are the `--socket=x11` requirement on GNOME and the
second-class CLI, not networking.

The honest framing: Flatpak would be a **distribution convenience for the GUI**, while the
daemon's needs point at a system package. A split — daemon via RPM/DEB, GUI via Flatpak — is
worse than either, because the GUI's whole job is talking to the daemon's socket across a
sandbox boundary. So it is all-or-nothing, and the "all" version needs
`--share=network --socket=wayland --socket=x11 --filesystem=xdg-download` plus a background
permission, which is close to an unsandboxed application wearing a sandbox.

**Recommendation: do not make Flatpak primary. Run POC-LINUX-03 to establish whether it works
at all, because being able to say "yes, with these permissions, and here is why" is worth
having.**

### 3.5 AppImage

| Aspect | Assessment |
| --- | --- |
| Sandbox | None. Everything in §1 works. |
| daemon / autostart | Awkward: a single-file app that must register a login service |
| Updates | ⚠️ Only via AppImageUpdate; usually not wired up |
| Signing | ⚠️ Possible, rarely verified by users |
| GTK/libadwaita bundling | Painful — themes, icons, portals, glib schemas |
| Maintenance | Medium-high for a GUI app |

An AppImage of the **daemon + CLI** would be nearly free and moderately useful. An AppImage of
the **GTK GUI** is a known-difficult exercise for limited benefit.

**Verdict: NOT RECOMMENDED** for the GUI. Optional for daemon+CLI, and the plain tarball
already covers that use case better.

### 3.6 Snap

| Aspect | Assessment |
| --- | --- |
| Sandbox | Similar constraints to Flatpak, with interfaces instead of permissions |
| Relevant interfaces | `network`, `network-bind`, `wayland`, `x11`, `home`, `desktop`, `upower-observe`; **`avahi-observe`/`avahi-control` for mDNS**, which are *Avahi-shaped* and AnyFlow does not use Avahi |
| Daemon support | Genuinely good — snapd supports daemons natively, which is better than Flatpak here |
| Store | Single vendor, and a store account requirement |
| Auto-update | Enforced, which some users dislike |

**Verdict: NOT RECOMMENDED for v1**, absent a specific reason. Snap is technically the best
*daemon* story among sandboxed formats, but the Avahi-shaped mDNS interfaces do not match
AnyFlow's own-responder design, and Ubuntu users are well served by a `.deb`. Revisit only if
Ubuntu adoption makes it worth it.

---

## 4. The mDNS/sandbox question, precisely

Worth isolating, because it is the most quoted objection and the most misunderstood.

The documented Flatpak limitation is **`.local` hostname resolution**: `getaddrinfo("x.local")`
inside a sandbox has no path to Avahi or systemd-resolved, and upstream has no portal for it.

AnyFlow's discovery does **not** work that way:

- `daemon/src/mdns.rs` registers a service with `mdns-sd`, an **in-process responder**, and
  uses `.enable_addr_auto()` so the crate tracks interface addresses itself;
- `core/src/discovery.rs::parse_discovered` builds `SocketAddr`s from the **IP addresses in
  the record**, not from a hostname;
- Android's `NsdManager` likewise resolves to addresses.

So the failing mechanism is one AnyFlow does not use. What AnyFlow needs is: a socket bound to
UDP 5353 that can send and receive multicast on the host's interfaces. `--share=network`
shares the host network namespace, which should provide exactly that.

Unverified parts, which is why this is POC and not a conclusion:
- whether binding 5353 alongside the host's Avahi works from inside the namespace-sharing
  sandbox (`SO_REUSEADDR`/`SO_REUSEPORT` behaviour);
- whether `mdns-sd`'s `AF_NETLINK` interface-change watching works in the sandbox;
- whether `IP_ADD_MEMBERSHIP` on each interface is permitted.

**POC-LINUX-03** answers all three in one sitting. Until then, "Flatpak breaks mDNS" is an
unverified claim about AnyFlow specifically, and this document declines to repeat it as fact.

---

## 5. Recommendation

| Format | Recommendation | Rationale |
| --- | --- | --- |
| **RPM** | **PRIMARY** (Fedora) | Exists; no sandbox; fill the integration gaps |
| **DEB** | **PRIMARY** (Debian/Ubuntu) | Same profile; largest reach |
| **Tarball** | **SECONDARY** | One CI job; covers every other distro and non-systemd |
| **Flatpak** | **EXPERIMENTAL** | POC-gated. Needs broad permissions; CLI is second-class |
| **AppImage** | **NOT RECOMMENDED** | GTK bundling cost, no update path, no daemon story |
| **Snap** | **NOT RECOMMENDED (v1)** | Avahi-shaped interfaces do not match our design |

Stated as the brief asks: **do not choose a universal format because it is popular.** AnyFlow
is closer to a system agent than to an application, and system agents are packaged by the
system.

---

## 6. Package layout (both RPM and DEB)

| Package | Contents | Depends |
| --- | --- | --- |
| `anyflow` | `anyflowd`, `anyflow`, `anyflowd.service`, autostart `.desktop`, docs, licence | libc only; `Recommends: upower`, `Suggests: wl-clipboard` |
| `anyflow-gui` | `anyflow-gui`, `.desktop`, hicolor icons, AppStream metainfo | `anyflow (= version)`, GTK ≥ 4.12, libadwaita ≥ 1.5 |

The split exists so distributions below the libadwaita floor
([05](05-DEBIAN-UBUNTU-COMPATIBILITY.md)) can still ship the daemon.

Files that need to be *authored*, not just installed:

```
packaging/common/io.github.yurisismotto.AnyFlow.desktop
packaging/common/io.github.yurisismotto.AnyFlow.metainfo.xml
packaging/common/anyflow-autostart.desktop          # X-GNOME-Autostart-enabled
packaging/common/icons/hicolor/scalable/apps/io.github.yurisismotto.AnyFlow.svg
```

Artwork already exists in `docs/design/assets/` (`app-icon.svg`, `logo-flowing-a.svg`,
`icon-flowing-ribbon.svg`), and `gui/build.rs` already reads from that directory — so the
icon install is a copy, not a design task.

---

## 7. Firewall, which packaging must not ignore

`anyflowd` binds TCP 55432 and joins multicast 5353. Default host behaviour differs:

| Distro | Default firewall | Inbound 55432 | mDNS |
| --- | --- | --- | --- |
| Fedora | firewalld, **enabled**, zone `FedoraWorkstation` | **Blocked** | The `mdns` service is usually allowed |
| Debian | none enabled | Open | Open |
| Ubuntu | `ufw` installed, **inactive** | Open | Open |

So Fedora — the reference platform — is the one where a fresh install may not accept a
connection from the phone, and Debian/Ubuntu "just work". Whether certification hid this
(because the dev machine already had a rule) is worth checking.

**Recommendation:** ship a firewalld service definition
(`/usr/lib/firewalld/services/anyflow.xml`) in the RPM, and **do not enable it silently**.
Tell the user in `%post`, or better, have `anyflow status` detect an unreachable listener and
say so. Opening a port without consent is the wrong default even for one's own package.
Recorded as **PKG-008**, **LINUX-004**.

This is also the Linux precedent for the Windows firewall discussion in
[09](09-WINDOWS-SECURITY-AND-INTEGRATION.md): scope to the local network, ask before
changing, never open to the world.

---

## 8. PoCs and backlog

| ID | Question |
| --- | --- |
| **POC-LINUX-03** | Does AnyFlow work as a Flatpak? Specifically: multicast on 5353 with `--share=network`; bundled `wl-clipboard` against the host compositor; XFIXES via `--socket=x11`; key-mode enforcement in `~/.var/app`; autostart via the Background portal. |

| Backlog | Item |
| --- | --- |
| **PKG-001** | `.desktop`, icons, AppStream metainfo |
| **PKG-002** | Package the GUI binary |
| **PKG-003** | XDG autostart entry alongside the systemd unit |
| **PKG-004** | Declare `wl-clipboard`; distro-neutral "not installed" message |
| **PKG-005** | Split `anyflow` / `anyflow-gui` |
| **PKG-006** | Gate graphical tests out of buildd/mock `%check` |
| **PKG-007** | Recommend `xdg-desktop-portal-{gtk,kde}` |
| **PKG-008** | firewalld service definition, not auto-enabled |
| **PKG-009** | Debian packaging skeleton (`debian/`), built in CI |
| **PKG-010** | Release tarball job |
