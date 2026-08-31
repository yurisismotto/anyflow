# 05 — Debian / Ubuntu compatibility

| Field | Value |
| --- | --- |
| **Title** | Which Debian and Ubuntu releases can run and build AnyFlow |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Archive-version audit against the API surface AnyFlow actually uses. Build feasibility, runtime feasibility, packaging, CI. |
| **Decision status** | PROPOSED |
| **Evidence** | OFFICIAL DOC VERIFIED — `packages.debian.org` and `packages.ubuntu.com`, accessed 2026-08-31. REPO VERIFIED for the API inventory. |
| **Related documents** | [04](04-LINUX-PORTABILITY.md), [06](06-KDE-PLASMA-WAYLAND.md), [07](07-LINUX-PACKAGING.md), [19](19-PACKAGING-AND-DISTRIBUTION.md) |

---

## 1. Method

The brief is explicit: do not pick minimum versions by opinion. So the order is

1. inventory the GTK/libadwaita API the code actually calls;
2. find the version each API was introduced in;
3. read the archives for what each release ships;
4. compare.

Step 1 is REPO VERIFIED (`grep -rhno "adw::[A-Za-z_]*" desktop/gui/src`). Step 2 is
OFFICIAL DOC VERIFIED from libadwaita/GTK documentation. Step 3 is OFFICIAL DOC VERIFIED
from the distributions' own package databases. Nothing here is inferred from a blog post.

---

## 2. What the GUI actually requires

### 2.1 Declared floor

```toml
gtk = { package = "gtk4",      version = "0.9", features = ["v4_12"] }
adw = { package = "libadwaita", version = "0.7", features = ["v1_5"] }
```

In the gtk-rs family a `vX_Y` feature gates the bindings to that API level and requires that
version of the C library at build and run time. **Declared floor: GTK 4.12, libadwaita 1.5.**

### 2.2 Floor implied by the code

| API used | Introduced in | Where |
| --- | --- | --- |
| `adw::AlertDialog` | **libadwaita 1.5** | `gui/src/views/pairing.rs` — pairing confirmation |
| `adw::Dialog` | **libadwaita 1.5** | dialog base |
| `adw::NavigationSplitView` | libadwaita 1.4 | `gui/src/lib.rs` |
| `adw::NavigationPage` | libadwaita 1.4 | `gui/src/lib.rs`, views |
| `adw::ToolbarView` | libadwaita 1.4 | shell |
| `adw::Breakpoint`, `BreakpointCondition`, `LengthUnit` | libadwaita 1.4 | adaptive layout |
| `adw::StyleManager`, `HeaderBar`, `ApplicationWindow` | ≤ 1.0 | — |
| `gtk::FileDialog` | **GTK 4.10** | `gui/src/views/files.rs` |
| `gtk::Picture`, `DrawingArea`, `ListBox`, `Stack`, `ProgressBar`, `Switch`, `CssProvider` | ≤ 4.0 | — |

So the **code-implied** floor is libadwaita 1.5 / GTK 4.10, and the **declared** floor is
libadwaita 1.5 / GTK 4.12. The GTK gap (4.10 vs 4.12) appears to be a conservative pin
rather than a requirement; the libadwaita floor is real and is set by `AdwAlertDialog`.

This matters, so state it plainly: **the single API that decides Debian/Ubuntu support is
`AdwAlertDialog`, and it is used for the pairing confirmation dialog** — the screen where a
human compares a fingerprint and accepts a device. It is not a cosmetic widget.

---

## 3. Archive versions

OFFICIAL DOC VERIFIED, accessed 2026-08-31.

### Debian

| Suite | Status | GTK 4 | libadwaita | rustc | wl-clipboard |
| --- | --- | --- | --- | --- | --- |
| bullseye | oldoldstable | — | — | 1.48.0 | 2.0.0 |
| bookworm | oldstable | 4.8.3 | 1.2.2 | 1.63.0 | 2.1.0 |
| **trixie** | **stable** | **4.18.6** | **1.7.6** | **1.85.0** | **2.2.1** |
| trixie-backports | — | — | — | 1.94.1 | — |
| forky | testing | 4.22.4 | 1.9.2 | 1.95.0 | **2.3.0** |
| sid | unstable | 4.22.4 | 1.9.2 | 1.95.0 | 2.3.0 |

### Ubuntu

| Release | Status | GTK 4 | libadwaita | rustc (default) | wl-clipboard |
| --- | --- | --- | --- | --- | --- |
| 22.04 jammy | LTS | 4.6.9 | 1.1.0 (1.1.7 in -updates) | 1.75.0 | 2.0.0 |
| **24.04 noble** | **LTS** | **4.14.5** | **1.5.0** | 1.75.0 | **2.2.1** |
| 25.10 questing | interim | 4.20.1 | 1.8.0 | 1.85.1 | 2.2.1 |
| **26.04 resolute** | **LTS** | **4.22.4** | **1.9.0** (1.9.1 -updates) | **1.93.1** | 2.2.1 |
| stonking | devel | 4.23.2 | 1.10~beta.1 | 1.93.1 | **2.3.0** |

Ubuntu 22.04 and 24.04 also carry versioned `rustc-1.82` … `rustc-1.91` packages alongside
the default `rustc`.

---

## 4. Compatibility matrix

| Distribution | Version | GTK 4 | libadwaita | Rust feasible | daemon | CLI | GUI | clipboard | discovery | Status |
| --- | --- | --- | --- | --- | :-: | :-: | :-: | --- | --- | --- |
| Debian | 12 bookworm | 4.8.3 | 1.2.2 | ❌ 1.63 | ⚠️ | ⚠️ | ❌ | n/a | ⚠️ | **NOT SUPPORTED** |
| Debian | 13 trixie | 4.18.6 | 1.7.6 | ✅ 1.85 | ✅ | ✅ | ✅ | ✅ wl-clip 2.2.1 | POC | **PRIMARY TARGET** |
| Debian | forky/sid | 4.22.4 | 1.9.2 | ✅ | ✅ | ✅ | ✅ | ✅ 2.3.0 | POC | **SUPPORTED (moving)** |
| Ubuntu | 22.04 LTS | 4.6.9 | 1.1.0 | ⚠️ named pkg | ⚠️ | ⚠️ | ❌ | n/a | ⚠️ | **DAEMON-ONLY at best** |
| Ubuntu | 24.04 LTS | 4.14.5 | **1.5.0** | ⚠️ named pkg | ✅ | ✅ | ✅ **exactly at floor** | ✅ 2.2.1 | POC | **SUPPORTED (fragile)** |
| Ubuntu | 25.10 | 4.20.1 | 1.8.0 | ✅ 1.85 | ✅ | ✅ | ✅ | ✅ | POC | **SUPPORTED** |
| Ubuntu | 26.04 LTS | 4.22.4 | 1.9.0 | ✅ 1.93 | ✅ | ✅ | ✅ | ✅ | POC | **PRIMARY TARGET** |

⚠️ for bookworm/jammy daemon+CLI means: buildable *only* with a non-archive Rust toolchain
(rustup or a backport). The daemon has no GTK dependency, so it is not blocked by libadwaita
— only by `rustc`.

---


> **⚠ Updated by the verification sprint (2026-08-31).** See
> [26 — External verification closeout](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) for the primary
> sources and [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) for the resulting decisions.
> **§5.3 is now resolved**, and a **new P0 defect** was found: `wl-copy --sensitive` does not exist before wl-clipboard 2.3.0, so `sensitive_hint` clips **fail outright** on Debian 13 and every current Ubuntu LTS ([26 §5.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). MSRV is **1.82**, so Ubuntu 24.04 needs the named `rustc-1.82` package.

## 5. The three findings that matter

### 5.1 Ubuntu 24.04 LTS sits exactly on the libadwaita floor

Noble ships **libadwaita 1.5.0**. AnyFlow requires **1.5**. There is zero margin.

This is a supported configuration and it is also a fragile one: any future use of a
libadwaita 1.6+ API — `AdwSpinner`, `AdwBottomSheet`, newer `AdwToolbarView` properties,
anything from the 1.6/1.7/1.8 generations — silently drops Ubuntu 24.04 LTS, whose standard
support runs to 2029. Nobody will notice at review time, because the developer's Fedora has
1.7 or newer.

**Recommendation:** make the floor a *contract*, not a coincidence — set
`features = ["v1_5"]` deliberately (already true), state the policy in
`desktop/gui/README.md`, and add a CI job that builds the GUI against Ubuntu 24.04's
libadwaita. Raising the floor then becomes a decision someone makes on purpose. **CI-003**.

### 5.2 Debian 12 and Ubuntu 22.04 are out, and lowering the floor buys almost nothing

Bookworm has libadwaita 1.2.2; jammy has 1.1.0. Reaching them means rewriting the adaptive
shell for the pre-1.4 widget set — `AdwLeaflet`/`AdwFlap` instead of
`AdwNavigationSplitView`, `AdwMessageDialog` instead of `AdwAlertDialog`, and no
`AdwBreakpoint`. That is a second GUI, not a compatibility shim.

And the payoff is temporary. Debian 12 is oldstable already. Ubuntu 22.04's standard support
ends April 2027.

**Recommendation: do not lower the floor.** Offer bookworm/jammy users the daemon and CLI
only (which need no GTK), and let the GUI require trixie / 24.04+. This is the argument for
splitting the packages — §7.

### 5.3 wl-clipboard 2.2.1 is the KDE risk, and it is an LTS-wide one

Ubuntu 24.04, 25.10 **and 26.04 LTS** all ship `wl-clipboard` **2.2.1**. `ext-data-control-v1`
support landed in **2.3.0**, which is in Debian forky/sid and Ubuntu "stonking" only.

Meanwhile KWin has ported its data-control implementation to `ext-data-control-v1`
(KWin MR !6606, replacing `wlr-data-control-unstable-v1`, which upstream wayland-protocols
marks deprecated).

If KWin no longer offers `wlr-data-control`, then on **Ubuntu 26.04 LTS + Plasma**
`probe_data_control()` in `backend/wayland.rs` fails, and `detect_watch_source()` falls
through to the Xwayland XFIXES bridge — which may or may not work on a Plasma Wayland
session. Automatic clipboard send is what is at stake.

Whether KWin kept a compatibility `wlr-data-control` binding was the crux. **RESOLVED
(V-01, V-02 — primary sources):**

- `ext-data-control-v1` support landed in **wl-clipboard 2.3.0**, released **2026-03-22**
  (upstream release body).
- KWin **dropped** its `wlr-data-control` compatibility overlay in commit `764b723`
  (2025-04-12), first shipped in **Plasma 6.5**. KWin ≤ 6.4 offers both.

Cross-referenced against the archives, **exactly one supported configuration is broken**:

| Distribution | KWin | wl-clipboard | Auto-send |
| --- | --- | --- | :-: |
| Debian 13 trixie | 6.3.6 | 2.2.1 | ✅ via `wlr` |
| Ubuntu 24.04 LTS | 5.27.11 | 2.2.1 | ✅ via `wlr` |
| Ubuntu 25.10 | 6.4.5 | 2.2.1 | ✅ via `wlr` |
| **Ubuntu 26.04 LTS** | **6.6.4** | **2.2.1** | ❌ **no common protocol** |
| Debian forky/sid, Ubuntu stonking | 6.7.4 | 2.3.0 | ✅ via `ext` |

Full matrix and sources: [26 §6](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). **POC-KDE-01 is narrowed**
to measuring the Xwayland XFIXES fallback on Plasma, which documentation cannot settle.

Note that GNOME on Ubuntu is unaffected: on GNOME the code already knows `wl-paste --watch`
will not work and uses the X11 bridge.

---

## 6. Build dependencies for a Debian package

Derived from the RPM spec plus the crate graph. Not tested — **POC-LINUX-01** exists to test
exactly this.

```
Build-Depends:
  debhelper-compat (= 13),
  dh-cargo,
  cargo,
  rustc (>= 1.82) | rustc-1.82,
  pkg-config,
  # for `ring` — confirm whether it is actually needed, see 04 §4
  gcc,
  # GUI only:
  libgtk-4-dev (>= 4.12),
  libadwaita-1-dev (>= 1.5),
  libglib2.0-dev-bin,   # glib-compile-resources, used by gui/build.rs

Depends (daemon):  ${shlibs:Depends}, ${misc:Depends}
Recommends:        upower
Suggests:          wl-clipboard          # daemon degrades honestly without it
Depends (gui):     libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5)
```

Two Debian-specific issues that do not arise on Fedora:

1. **Vendored vs. unbundled crates.** Debian policy prefers `librust-*` archive packages over
   vendored sources. AnyFlow depends on `rustls 0.23`, `tokio 1.53`, `prost 0.14`,
   `rcgen 0.14`, `mdns-sd 0.15`, `x11rb 0.14`, `gtk4 0.9`, `libadwaita 0.7` and more. The
   probability that trixie's `librust-*` set satisfies all of those simultaneously is low,
   and the security-relevant ones (`rustls`, `rcgen`) are exactly the ones where a version
   substitution matters. Realistically the first Debian package will vendor
   (`cargo build --locked` with `Cargo.lock`), which is acceptable for a
   third-party/PPA/OBS package but is a hurdle for official Debian inclusion.
   **This is a packaging-strategy decision, not a technical blocker** → **PLAT-DEC-011**.
2. **`%check` equivalent.** The RPM runs `cargo test --release --locked`. Several tests need
   a graphical session (`capabilities/clipboard/tests/real_backend.rs`) and must be excluded
   or gated on a build-time environment variable in a buildd chroot. **PKG-006**.

---

## 7. Package split proposal

| Binary package | Contents | Depends |
| --- | --- | --- |
| `anyflow` | `anyflowd`, `anyflow`, systemd user unit, docs | none beyond libc |
| `anyflow-gui` | `anyflow-gui`, `.desktop`, icons, AppStream metainfo | `anyflow (= same version)`, GTK ≥ 4.12, libadwaita ≥ 1.5 |

Rationale: the GTK floor is the only thing excluding a distribution, and it excludes only the
GUI. A bookworm or jammy user with a rustup toolchain can have a fully working AnyFlow with
the CLI. The same split is proposed for RPM in [07](07-LINUX-PACKAGING.md).

---

## 8. Proposed CI (design only — no GitHub Actions change in this sprint)

The brief forbids touching CI now. This is the design for later.

| Job | Container / image | Proves |
| --- | --- | --- |
| `build-debian-stable` | `debian:trixie` | Archive `rustc` 1.85 builds daemon+CLI+GUI |
| `build-ubuntu-lts-2404` | `ubuntu:24.04` + `rustc-1.82` | The libadwaita **1.5.0 floor** still holds |
| `build-ubuntu-lts-2604` | `ubuntu:26.04` | Current LTS |
| `build-fedora` | `fedora:latest` | Existing baseline |
| `msrv` | `rust:1.82` | MSRV is real |
| `test-headless` | any | Full suite minus the graphical clipboard tests |
| `clipboard-session` | VM, **not** a container | The graphical tests; needs a real Wayland session |

The last row is the important one: containers cannot host a Wayland compositor with a seat,
so clipboard certification cannot be a container job. It is a VM job or a hardware job. That
constraint shapes the whole test-lab recommendation in
[22](22-IMPLEMENTATION-ROADMAP.md).

---

## 9. Recommended direction

1. **Primary Debian/Ubuntu targets: Debian 13 trixie and Ubuntu 26.04 LTS.** Both are current,
   both exceed every floor comfortably.
2. **Ubuntu 24.04 LTS: supported, with a CI job pinning the libadwaita 1.5 floor.**
3. **Debian 12 / Ubuntu 22.04: daemon + CLI only, best-effort, no GUI.** Do not lower the
   libadwaita floor for them.
4. Split into `anyflow` and `anyflow-gui`.
5. Treat wl-clipboard 2.2.1 on **Ubuntu 26.04 LTS + Plasma** as a confirmed auto-send gap
   (cause known; see above). Report it upstream to Ubuntu — 2.3.0 in `resolute-updates` fixes
   this *and* `--sensitive` for every Ubuntu user.
6. **NEW, P0 (LINUX-010):** handle `wl-copy --sensitive` being unavailable before wl-clipboard
   2.3.0. It affects Debian 13 and **every** current Ubuntu LTS, on **every** desktop, and today
   makes `sensitive_hint` clips fail with no explanation. Probe, fail closed, name the remedy
   (PLAT-DEC-013).
7. **Do not express this as `wl-clipboard >= 2.3` in packaging.** Fedora ships
   `2.2.1^git20251124`, which *has* both features — a version dependency would exclude a working
   system. Probe at runtime; *recommend* 2.3 in packaging.
6. Defer official Debian archive inclusion; ship a `.deb` from CI first.
