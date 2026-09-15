# AnyFlow — Linux distribution compatibility
# U0: Ubuntu / Debian compatibility audit and implementation plan

**Branch:** `research/linux-debian-ubuntu-compat-v1`
**Baseline:** `7822a06` — `notifications.v1` final certification merged (PR #23), working
tree clean at the start.
**Mode:** read-only audit, targeted probes, implementation plan. No feature was added,
nothing was refactored, no protocol was touched, and nothing was committed, pushed or
opened as a PR. No production code change is left behind.

---

## 1. Executive summary

The current AnyFlow Linux desktop stack was audited against stock **Ubuntu LTS** and
**Debian Stable**, GNOME on Wayland. The audit was done from code and from measured probes
rather than from the previous desk research, and it **corrects that research in two
places** — the GTK floor (§14.2) and, more consequentially, the MSRV (§20.3).

**There is no blocker.** Ubuntu 24.04 LTS, Ubuntu 26.04 LTS and Debian 13 trixie can each
build the daemon, the CLI **and the GTK GUI**, verified by compiling the workspace inside
each distribution against its own GTK stack (§20.4). The delta is small enough for **one
implementation wave**, and none of it is architectural.

One qualification belongs in the first paragraph rather than buried: **the GTK side needs
nothing at all, and the Rust toolchain side does.** Every target's GTK and libadwaita are
adequate as shipped. But the committed `Cargo.lock` needs **rustc 1.88** while the manifest
claims 1.82, so **Debian Stable's stock `rustc` 1.85.1 cannot build AnyFlow** and Ubuntu
24.04 needs `rustc-1.91` rather than the `rustc-1.82` the declared MSRV implies. Both
distributions can supply a working toolchain from their own archives or backports; what is
wrong today is the declaration (U-4).

The reason the delta is small is visible in three measurements:

| Measurement | Result |
| --- | --- |
| External binaries the whole product executes | **exactly two** — `wl-copy`, `wl-paste` |
| D-Bus names the whole product uses | **all four are `org.freedesktop.*`** — zero `org.gnome.*` |
| Native C libraries `anyflowd` and `anyflow` link against | **none** (§6) |
| OpenSSL anywhere in the resolved dependency graph | **none** |
| systemd coupling in the runtime (`sd_notify`, socket activation, watchdog) | **none** |
| User-facing strings in the product that name a distribution | **exactly one** (§4) |

### The findings

| # | Finding | Class | Blocking? |
| --- | --- | --- | --- |
| **U-1** | `wl-copy --sensitive` does not exist on Ubuntu 24.04, Ubuntu 26.04 **or** Debian 13 — all three ship wl-clipboard 2.2.1, the flag landed in 2.3.0. Every clip Android marks `sensitive_hint` is therefore **refused** on all three targets. Fedora works only because its build is a post-2.2.1 git snapshot that carries the flag. | Distro capability gap; the refusal is deliberate and fail-closed | **No** — but it is the one user-visible functional difference and needs a decision |
| **U-2** | The only string in the product that names a distribution tells the user to run `sudo dnf install wl-clipboard` ([wayland.rs:133](desktop/capabilities/clipboard/src/backend/wayland.rs#L133)) — wrong on both new targets | UX correctness | No |
| **U-3** | Ubuntu 24.04 LTS ships libadwaita **exactly 1.5.0** against a declared floor of **1.5**. Zero margin, and nothing in the repo guards it | Regression risk | No |
| **U-4** | **The declared MSRV is false.** `desktop/Cargo.toml` says `rust-version = "1.82"`; the committed `Cargo.lock` actually requires **rustc 1.88** (`time`, `rcgen` at 1.88; `zbus`/`zvariant` at 1.87 — 34 locked crates demand more than 1.82). **Debian Stable's stock `rustc` 1.85.1 therefore cannot build AnyFlow**, and neither can Ubuntu 24.04's `rustc-1.82`. Found by compiling, not by reading archives (§20.3) | Build correctness | **No** — every target has a toolchain that works; the *claim* is what is broken |
| **U-5** | There is **no Linux CI job at all**. `.github/workflows/` contains only the Windows MSVC portability job | Process gap | No |
| **U-6** | `README.md`, `docs/architecture/` and `packaging/` say "Fedora" where they now mean "Linux" | Documentation | No |
| **U-7** | The GTK floor is genuinely **4.12**, not the "conservative pin" the earlier research called it. The API is `CssProvider::load_from_string` | Correction to prior research | No |
| **U-8** | No `.desktop` file exists anywhere in the repo, so the `desktop-entry` hint AnyFlow sends with every mirrored notification resolves to nothing | Packaging, pre-existing, all distros | No |

### The scope boundary that matters

Debian 12 bookworm (GTK 4.8.3, libadwaita 1.2.2, rustc 1.63) and Ubuntu 22.04 LTS cannot
build the GUI, and reaching them would mean a second GUI rather than a shim (§14.4). Both
are **oldstable/superseded**, and both are **out of scope for V1** — stated as a decision,
not discovered as a failure.

---

## 2. Baseline

```console
$ git branch --show-current
research/linux-debian-ubuntu-compat-v1
$ git status
On branch research/linux-debian-ubuntu-compat-v1
nothing to commit, working tree clean
$ git log --oneline -10
7822a06 Merge pull request #23 from yurisismotto/cert/notifications-v1-n6-final-certification
4306389 docs(notifications): certify notifications.v1 for v1
836cbd4 Merge pull request #22 from yurisismotto/feature/notifications-v1-n5-hardening
e72bb09 feat(notifications): harden recovery and session convergence
85f8424 Merge pull request #21 from yurisismotto/feature/notifications-v1-n4-dismiss-sync
778ac05 ci: classify N4 notification tests and avoid duplicate runs
b7531d0 feat(notifications): implement dismiss synchronization
8d8948a Merge pull request #20 from yurisismotto/feature/notifications-v1-n3-consent-ui
98e8b70 feat(android,desktop): add notifications.v1 consent and privacy UX
4d33205 Merge pull request #19 from yurisismotto/feature/notifications-v1-n2-linux-sink
$ git diff --check
(clean)
```

**Audit host.** Fedora 44, kernel 7.1.9-200.fc44, GNOME Shell 50.4 on Wayland,
rustc/cargo 1.98.0, GTK 4.22.4 and libadwaita 1.9.3 from a session-local devel prefix,
podman 5.8.4, libvirt 12.0.0 / QEMU 10.2.2 / virt-manager / GNOME Boxes all present.

**Note on the brief's baseline wording.** The brief says `notifications.v1` is "merged into
develop". In this repository it is merged to **`main`** as PR #23 (`7822a06`); there is no
`develop` branch. Recorded so the baseline is unambiguous.

---

## 3. Current architecture

Twelve crates. Classification is from the manifests and the resolved dependency graph, not
from the names.

| Crate | Role | Native deps | Class |
| --- | --- | --- | --- |
| `anyflow-proto` | generated schema | none (`protox` is pure Rust) | **PORTABLE** |
| `anyflow-core` | identity, TLS, trust store, discovery contract | none | **PORTABLE** (`unix-fs` feature adds the POSIX store) |
| `anyflow-control` | control-socket contract | none | **PORTABLE** |
| `anyflow-runtime` | the agent: sessions, grants, renegotiation, mDNS | none | **PORTABLE** |
| `anyflow-linux` | Unix socket transport, XDG paths, file store | none | **LINUX-GENERIC** |
| `anyflow-daemon` | `anyflowd` binary | **none** | **LINUX-GENERIC** |
| `anyflow-cli` | `anyflow` binary | **none** | **LINUX-GENERIC** |
| `anyflow-gui` | GTK4 + libadwaita | **gtk4, libadwaita and their stack** | **LINUX-GENERIC**, GTK-bound |
| `…-capability-battery` | `battery.v1` | `zbus` (pure Rust) behind `upower` | **PORTABLE** core / **LINUX-GENERIC** backend |
| `…-capability-clipboard` | `clipboard.v1` | `x11rb` (pure Rust) behind `linux-backends` | **PORTABLE** core / **LINUX-GENERIC** backend |
| `…-capability-files` | `files.v1` | none | **PORTABLE** (`unix-fs` adds POSIX) |
| `…-capability-notifications` | `notifications.v1` | `zbus` (pure Rust) behind `linux-dbus` | **PORTABLE** core / **LINUX-GENERIC** backend |

**Nothing is FEDORA-SPECIFIC.** `packaging/fedora/` is the only **PACKAGING-ONLY** tree,
and it is out of this wave's scope.

**GNOME-SPECIFIC needs a more careful answer than yes or no.** No production code names
GNOME — there is not one `org.gnome.*` string in the tree (§9), and the notification sink
adapts to whatever `GetCapabilities` reports. But one *runtime path* is chosen because of
GNOME: since Mutter implements neither `zwlr_data_control_manager_v1` nor
`ext_data_control_manager_v1`, the clipboard change watch always falls through to the
Xwayland XFIXES bridge (ADR-0014, §8). The code reaches that path by **probing**, not by
detecting GNOME, which is why it is portable — but a reader should know that on every GNOME
distribution the fallback *is* the path, and that the primary path has never run in
production. Classified **LINUX-GENERIC, GNOME-shaped**.

The Wave 0 platform abstraction is what makes this true: every platform touch is behind a
seam (`NotificationSink`, `LockSource`, `ClipboardBackend`, `SecretStore`, `FileSink`,
`ControlTransport`) with an in-memory implementation beside it, and every platform
dependency is an **optional Cargo feature** rather than a mandatory one.

---

## 4. Fedora-specific assumptions

The sweep covered `fedora`, `dnf`, `rpm`, `rpm-ostree`, `selinux`, `restorecon`,
`semanage`, `firewalld`, `firewall-cmd`, `/usr/lib64`, `/usr/libexec`,
`/usr/lib/systemd`, `polkit` and `apparmor` across all production Rust
(`desktop/*/src`, `desktop/capabilities/*/src`).

```
fedora : 11      dnf : 1      everything else : 0
```

Every one of the 12 hits was read. **Eleven are prose** — module documentation recording
where a measurement was taken, or explaining why a rule exists.

**Exactly one is product surface:**

| # | Location | What it is |
| --- | --- | --- |
| **U-2** | [wayland.rs:130-135](desktop/capabilities/clipboard/src/backend/wayland.rs#L130-L135) | The remediation text shown when `wl-copy`/`wl-paste` are missing: *"Install the wl-clipboard package (Fedora: `sudo dnf install wl-clipboard`)."* |

A multiline-tolerant sweep for `dnf install`, `apt install`, `apt-get install`,
`systemctl --user` and `loginctl` inside string literals across the whole product returns
**three** hits in total: the one above, a `systemctl --user start anyflowd.service`
suggestion in the CLI's daemon-unreachable error ([main.rs:238](desktop/cli/src/main.rs#L238)),
and one line of module prose. The CLI string assumes systemd, which is true on Fedora,
Ubuntu and Debian alike, so it is correct on every V1 target.

**Two things that look Fedora-specific and are not:**

* `default_device_name()` ([unix_fs.rs:231-246](desktop/core/src/platform/unix_fs.rs#L231-L246))
  reads `/etc/hostname` then `/proc/sys/kernel/hostname`, with `"AnyFlow Desktop"` as the
  fallback. The comment records that the fallback *used* to be a distribution name and was
  removed precisely because *"a user who sees 'Fedora' on their phone while pairing a
  Debian laptop is being told something false."* Both sources exist identically on Ubuntu
  and Debian.
* `probe_sensitive()` ([wayland.rs:472-496](desktop/capabilities/clipboard/src/backend/wayland.rs#L472-L496))
  refuses to parse `wl-copy --version` and parses `--help` instead, because *"Fedora's
  `2.2.1^git20251124` has the flag and Debian's `2.2.1` does not."* That is a
  **capability** probe rather than a version check, and it is the right shape for exactly
  the cross-distro problem U-1 describes.

**No SELinux, firewalld, polkit, `/usr/lib64`, `/usr/libexec` or RPM assumption exists in
the product.**

---

## 5. Linux-generic assumptions

Everything the product assumes about Linux is POSIX or freedesktop, and all of it holds
identically on Ubuntu and Debian:

| Assumption | Where | Ubuntu / Debian |
| --- | --- | --- |
| `/proc/self/status` for the uid | [platform-linux/src/lib.rs:83](desktop/platform-linux/src/lib.rs#L83), [logind.rs:292](desktop/capabilities/notifications/src/backend/logind.rs#L292) | identical |
| `/etc/hostname`, `/proc/sys/kernel/hostname` | [unix_fs.rs:236](desktop/core/src/platform/unix_fs.rs#L236) | identical |
| Unix domain socket in `$XDG_RUNTIME_DIR` | [platform-linux/src/lib.rs:72-76](desktop/platform-linux/src/lib.rs#L72-L76) | identical (`/run/user/$UID`) |
| `O_EXCL` + mode 0600 + same-directory `rename(2)` | [destination.rs:87-165](desktop/capabilities/files/src/destination.rs#L87-L165) | identical |
| 0700 directories, 0600 key files, mode verification on load | [unix_fs.rs](desktop/core/src/platform/unix_fs.rs), [store.rs](desktop/core/src/store.rs) | identical |
| TCP 55432, unprivileged | [core/src/lib.rs:39](desktop/core/src/lib.rs#L39) | identical; no capability or privileged port needed |
| `_anyflow._tcp.local.` over its own multicast socket | [runtime/src/mdns.rs](desktop/runtime/src/mdns.rs) | identical |

**AnyFlow's own source contains no raw `libc`, no `nix`, no `syscall()` and no `ioctl`.**
The only occurrence of the word `libc` in production source is a comment explaining why it
is *not* used. (`libc` is of course present *transitively*, through `socket2`, `mio` and
`if-addrs`; the claim is about AnyFlow's own code, where a platform-specific syscall would
be a portability liability.)

---

## 6. Native and build dependencies

Measured from the **resolved dependency graph**, not from source greps.

```console
$ cargo tree --workspace --locked -e normal,build | grep -iE "openssl|native-tls|libssl"
(no output)
```

**There is no OpenSSL anywhere in the Linux application stack.** Transport security is
`rustls` + `ring` throughout, as ADR-0007 requires. No accidental dependency has appeared.

Every `-sys` crate in the entire workspace:

```
gdk4-sys  gdk-pixbuf-sys  gio-sys  glib-sys  gobject-sys
graphene-sys  gsk4-sys  gtk4-sys  libadwaita-sys  pango-sys      ← the GUI only
linux-raw-sys                                                    ← pure Rust (rustix)
```

Per binary:

| Binary | Native `-sys` dependencies |
| --- | --- |
| `anyflowd` | **`linux-raw-sys` only** — i.e. no C library at all |
| `anyflow` (CLI) | **`linux-raw-sys` only** |
| `anyflow-gui` | the ten GTK-stack crates above |

This is stronger than it looks, and it is worth naming why:

* **`zbus` 5 is a pure-Rust D-Bus implementation.** `libdbus-1-dev` is **not** required to
  build, and `libdbus` is not linked at runtime. All three D-Bus consumers (notifications
  sink, logind lock, UPower battery) inherit that.
* **`x11rb` 0.14 with `default-features = false`** uses its own `RustConnection`. The
  Xwayland XFIXES clipboard watch needs **no `libX11`, no `libxcb`** at build or run time.
* **`mdns-sd` is a pure-Rust mDNS responder.** **Avahi is not a build or link dependency.**
* **`protox` + `prost_build::skip_protoc_run()`** ([proto/build.rs](desktop/proto/build.rs))
  means **no system `protoc`**. Confirmed in every container probe.
* A **C compiler is still required**, because `ring` compiles C and assembly. That is the
  one thing `BuildRequires: gcc` in the Fedora spec is actually for.
* `anyflow-gui`'s build script runs **`glib-compile-resources`**
  ([gui/build.rs](desktop/gui/build.rs)) — a build-time *binary*, not a library.

### Package names per distribution

Recorded for the later packaging wave. **No manifest is created here.**

| Need | Fedora | Ubuntu / Debian |
| --- | --- | --- |
| Rust ≥ 1.82 | `rust`, `cargo` | `rustc`, `cargo` (trixie / 26.04) or `rustc-1.8x`,`cargo-1.8x` (24.04) or rustup |
| C compiler (for `ring`) | `gcc` | `gcc`, `libc6-dev` (or `build-essential`) |
| pkg-config | `pkgconf-pkg-config` | `pkg-config` |
| GTK 4 headers (**GUI only**) | `gtk4-devel` | `libgtk-4-dev` |
| libadwaita headers (**GUI only**) | `libadwaita-devel` | `libadwaita-1-dev` |
| `glib-compile-resources` (**GUI only**) | `glib2-devel` | **no separate package needed** — `libgtk-4-dev` alone pulls `libgio-2.0-dev-bin`, which ships it at `/usr/bin/glib-compile-resources` (measured on trixie) |
| clipboard helpers (**runtime**) | `wl-clipboard` | `wl-clipboard` |
| battery (**runtime, optional**) | `upower` | `upower` |
| user D-Bus session (**runtime**) | present with systemd | **`dbus-user-session`** — a hard dependency of `gnome-core` and `ubuntu-desktop-minimal`, confirmed |

**No `protobuf-compiler` on any distribution.** **No `libssl-dev` on any distribution.**
**No `libdbus-1-dev`, `libx11-dev` or `libxcb1-dev` on any distribution.**

---

## 7. External runtime commands

Every process the product spawns, found by searching all of production for
`Command::new` and `process::Command`:

| Executable | Purpose | Fatal if missing? | Fallback | Fedora | Ubuntu 24.04 / 26.04 | Debian 13 |
| --- | --- | --- | --- | --- | --- | --- |
| **`wl-copy`** | write the clipboard; probe `--sensitive` via `--help` | **No** | backend reports `Unsupported` with the reason; daemon still starts, capability still registers and answers honestly | `wl-clipboard` | `wl-clipboard` `/usr/bin/wl-copy` ✓ measured | ✓ measured |
| **`wl-paste`** | read the clipboard; `--watch` for change events; probe data-control | **No** | falls through to the Xwayland XFIXES watch, then to "watch unavailable, manual send still works" | `wl-clipboard` | ✓ measured | ✓ measured |

**That is the complete list.** The product does **not** invoke `loginctl`, `systemctl`,
`journalctl`, `xdg-open`, `gio`, `notify-send`, `gdbus`, `dbus-send`, `nmcli`, `hostname`,
`ip`, `ss`, `xclip` or `xsel`. Every textual hit for those names in production source is a
doc comment.

Two absolute paths are passed **as arguments** to `wl-paste --watch`: `/usr/bin/true`
(the data-control probe) and `/usr/bin/echo` (the watch itself, chosen so clipboard
content never enters AnyFlow's pipe). Both were verified present at exactly those paths in
Ubuntu 24.04, Ubuntu 26.04 and Debian 13 — all three are merged-`/usr` systems.

---

## 8. Clipboard

### What the backend actually does

`detect_linux()` ([backend/mod.rs:220-240](desktop/capabilities/clipboard/src/backend/mod.rs#L220-L240))
branches on `WAYLAND_DISPLAY`. On a Wayland session it builds `WaylandBackend`, which
probes three things once, at construction:

1. **tools** — are `wl-copy` and `wl-paste` on `PATH`;
2. **watch source** — `wl-paste --watch` first, then the **Xwayland XFIXES** bridge, then
   none;
3. **`--sensitive` support** — by reading `wl-copy --help`.

Each probe's answer is reported by `anyflow clipboard status` rather than discovered at
first use.

**On GNOME the watch is always the Xwayland bridge.** Mutter implements neither
`zwlr_data_control_manager_v1` nor `ext_data_control_manager_v1`, so `wl-paste --watch`
exits immediately and the code falls through to XFIXES on the Xwayland `CLIPBOARD`
selection (ADR-0014). **This is GNOME behaviour, not Fedora behaviour** — it will be
identical on Ubuntu GNOME and Debian GNOME.

**X11 read/write is not implemented on any distribution.** A pure-X11 session gets
`Unsupported` with an explicit message. That is a capability boundary, not a distro gap.

### U-1 — the one real functional difference

`wl-copy --sensitive` was added in **wl-clipboard 2.3.0**. Measured today:

| Distribution | package version | `wl-copy --version` says | `wl-copy --help` contains `--sensitive` |
| --- | --- | --- | --- |
| **Fedora 44** (the certification host) | `2.2.1^git20251124.e808203-2.fc44` | `wl-clipboard 2.2.1` | **YES** — measured on the host |
| **Ubuntu 24.04 LTS** | `2.2.1-1build1` | `wl-clipboard 2.2.1` | **NO** — measured |
| **Ubuntu 26.04 LTS** | `2.2.1-2build1` | `wl-clipboard 2.2.1` | **NO** — measured |
| **Debian 13 trixie** | `2.2.1-2` | `wl-clipboard 2.2.1` | **NO** — measured |
| Debian 12 bookworm | `2.1.0-0.1+b1` | — | NO |

Note the middle column: **all four report the identical version string `wl-clipboard
2.2.1`, and they do not behave identically.** Fedora's is a post-2.2.1 git snapshot that
carries the backported flag. This is precisely why `probe_sensitive()` reads `--help`
rather than parsing `--version`, and it is a small piece of design that this audit can now
confirm was correct for the right reason.

What happens then is deliberate and documented at the call site
([wayland.rs:315-341](desktop/capabilities/clipboard/src/backend/wayland.rs#L315-L341)):
the write is **refused**, with

> *"this clip is marked sensitive and … It was NOT written to the clipboard: writing it
> unmarked would leave a password in your clipboard manager's history without telling
> you."*

The comment records that writing it unmarked with a warning *"was considered and
rejected … silently downgrading turns a visible failure into an invisible privacy
regression"* (PLAT-DEC-013).

**So this is not a defect — it is fail-closed behaviour meeting a distro that cannot do
the thing.** But the consequence on both new targets is concrete: **every clip a password
manager copies on the phone** (Android sets `EXTRA_IS_SENSITIVE`, which becomes
`sensitive_hint` on the wire) **is refused on Ubuntu and Debian and accepted on Fedora.**
That is a user-visible behavioural difference between supported platforms and it needs a
decision, not just a note. See work item **L2**.

### Truthful error surfacing

Confirmed: `WatchSource::describe()` and `SensitiveSupport::describe()` exist precisely so
`anyflow clipboard status` can state the degradation before the user hits it, and the
`Unsupported` backend exists *"so the capability can still be registered, still answer
peers with an honest `FAILED`, and still report why … rather than the daemon refusing to
start."* Nothing here is Fedora-specific. The only thing that is wrong on Ubuntu/Debian is
the **wording** of the missing-tools remedy (U-2).

---

## 9. Notifications

**`notifications.v1` depends on the freedesktop interface, not on GNOME and not on
Fedora.** Measured, not asserted.

Every D-Bus name in the entire product:

```
org.freedesktop.Notifications      /org/freedesktop/Notifications
org.freedesktop.DBus               /org/freedesktop/DBus
org.freedesktop.login1             /org/freedesktop/login1
org.freedesktop.UPower             /org/freedesktop/UPower/devices/DisplayDevice
```

**There is not one `org.gnome.*` name in production code.** The only mention of
`org.gnome.ScreenSaver` in the tree is POC-NOTIF-02 prose explaining why it was **rejected**
as a lock source.

| Requirement | Implementation | Class |
| --- | --- | --- |
| `org.freedesktop.Notifications` | `DEST`/`PATH`/`IFACE` constants, spec-standard | **portable across any FDO server** |
| `GetServerInformation` | the connect probe; its answer is what `anyflow notifications status` prints | portable |
| `GetCapabilities` | read at connect; `body`, `body-markup`, `persistence` taken from the reply, never assumed | portable |
| `Notify` | the standard 8-argument call; `actions` deliberately empty; `urgency` and `desktop-entry` hints | portable |
| `CloseNotification` | standard | portable |
| `NotificationClosed` | subscribed; **`dismiss_reporting` is set from whether the subscription succeeded**, not from a capability string | portable, and self-correcting |
| `replaces_id` | `replaces.unwrap_or(0)` — 0 means "new", any other value replaces in place | portable |

The capability set is **read from the server at runtime**, so a server with a different
capability list produces different, truthful behaviour rather than a wrong assumption.
`dismiss_reporting` is the sharpest example: it is `closed_rx.is_some()`, i.e. *"did this
process actually get the signal subscription"*, so a session where the match rule cannot
be installed announces no `DISMISS_REPORTER` and the feature honestly reports itself
unavailable.

**Classification: portable across GNOME distros.** Ubuntu 24.04 ships gnome-shell 46.0,
Ubuntu 26.04 ships 50.1, Debian 13 ships 48.7 — against Fedora 44's 50.4. All four are the
same notification server implementation, and the code reads its capabilities rather than
hard-coding them. **Nothing here needs a change.** It does need a real-session test, because
"the code adapts" is a claim about behaviour and only a session can confirm it (§21).

One packaging-adjacent observation (**U-8**): `Notify` sends the hint
`desktop-entry = "io.github.yurisismotto.anyflow"`, and **neither a `.desktop` file nor an
AppStream `metainfo.xml` exists anywhere in the repository** — verified by a tree-wide
search. GNOME uses that hint to resolve the app's name and icon on the banner, and the GUI
registers the same string as its `adw::Application` id. So today the banner falls back to
the raw `app_name` and the GUI has no launcher entry. This is **pre-existing and identical
on Fedora**, it is not a distro compatibility issue, and it belongs to the packaging wave —
where a `.deb` will want both files anyway.

---

## 10. Lock detection

`LogindLock` ([backend/logind.rs](desktop/capabilities/notifications/src/backend/logind.rs))
reads `org.freedesktop.login1.Session.LockedHint` on the **system** bus.

`resolve_session()` tries three candidates in order:

1. **`Manager.GetUser(uid)` → `User.Display`** — logind's own name for the user's primary
   graphical session. The uid comes from `/proc/self/status`, not from `libc::getuid`,
   because the crate forbids `unsafe`.
2. `Manager.GetSessionByPID(self)` — correct when the daemon runs inside the session
   scope, absent under `systemd --user`.
3. `Manager.GetSession($XDG_SESSION_ID)` — last, because the variable can be stale or
   inherited.

**Every one of those is systemd-logind's standard API.** Assessment per target:

| Question | Ubuntu 24.04 / 26.04 | Debian 13 |
| --- | --- | --- |
| systemd-logind present | yes — systemd 255.4 / 259.5 | yes — systemd 257.13 |
| `User.Display` populated for a GNOME Wayland login | expected yes | expected yes |
| `LockedHint` tracked through `PropertiesChanged` | standard logind behaviour | standard |
| `XDG_SESSION_ID` set by `pam_systemd` | yes | yes |
| `DBUS_SESSION_BUS_ADDRESS` | **not read by AnyFlow at all** — `zbus` resolves the bus itself | same |

**No distro compatibility risk was identified in the code.** Two things to note rather than
fix:

* Debian permits **elogind** as a systemd-logind substitute on non-systemd installs.
  It serves the same `org.freedesktop.login1` interface, and Debian's *default* init for
  GNOME is systemd, so this is not a V1 concern. Recorded for completeness.
* The fail-closed rule is by type, not by check: `read_locked_hint()` returns `true` on
  every error path, and a session that cannot be resolved at all yields `UnknownLock`,
  which reports locked. A distro difference here degrades toward **withholding** content.
  That is the right direction and it is why this section carries no risk item.

Real-session confirmation is still required (§21 gate **G-LOCK**): the session-resolution
logic is the one place where a different display manager (GDM on both targets, so likely
identical) could in principle produce a different `Display` answer.

---

## 11. Discovery / mDNS

`Advertisement::publish` ([runtime/src/mdns.rs](desktop/runtime/src/mdns.rs)) uses
`mdns-sd` 0.15, a **pure-Rust mDNS responder**. The daemon only advertises; Android is
always the initiator.

| Question | Answer | Evidence |
| --- | --- | --- |
| Does it work without Avahi? | **Yes.** `mdns-sd` opens its own multicast socket and speaks mDNS itself | no `avahi` or `dbus` name appears in the discovery path; `mdns-sd` is the only dependency |
| Is Avahi required indirectly? | **No** — not as a library, not over D-Bus | the resolved graph contains no Avahi binding |
| Does Avahi being present break it? | **No** — and Fedora already proves it: Fedora Workstation runs `avahi-daemon` by default and AnyFlow has been certified on it through six waves. `mdns-sd` sets both `SO_REUSEADDR` and `SO_REUSEPORT` (`service_daemon.rs:688-691`, verified in the vendored crate), so two responders coexist on 5353 | Fedora certification history; **still a VM gate on Ubuntu/Debian (G-MDNS)** |
| Interface enumeration | delegated to `mdns-sd` via `.enable_addr_auto()`, so the record follows Wi-Fi/dock changes without a restart | [mdns.rs:66-68](desktop/runtime/src/mdns.rs#L66-L68) |
| IPv4 / IPv6 | the record is restricted to the families the TCP listener actually **bound**, via `daemon.disable_interface(IfKind::IPv6/IPv4)`. Advertising an address the daemon cannot accept on is explicitly treated as worse than advertising nothing | [mdns.rs:41-47](desktop/runtime/src/mdns.rs#L41-L47), [listener.rs](desktop/runtime/src/listener.rs) |
| NetworkManager assumptions | **none** — NetworkManager is never consulted | no `nmcli`, no `org.freedesktop.NetworkManager` |
| Instance naming | the **device id**, not the hostname, because *"two machines called 'fedora' is the common case"* | [mdns.rs:52-55](desktop/runtime/src/mdns.rs#L52-L55) |

Separating the three layers the brief asks about:

* **Code requirement:** none beyond an interface that can send and receive multicast on
  UDP 5353 and a listener on TCP 55432. Both unprivileged.
* **System configuration:** the firewall must permit inbound mDNS (UDP 5353) and TCP 55432.
  Fedora Workstation's default zone already does. **Ubuntu ships `ufw` inactive by
  default and Debian ships no firewall enabled by default**, so both are *expected* to be
  at least as permissive — but a default-state claim is exactly the kind of thing that
  must be measured in a real install rather than inferred, so it is gate **G-FW**.
* **Packaging recommendation (later wave):** ship a firewalld service file for Fedora and
  a documented `ufw allow` recipe for Ubuntu. Not this wave.

---

## 12. Files

`files.v1`'s desktop side is POSIX-plain. Audited against every item the brief names:

| Item | Implementation | Distro risk |
| --- | --- | --- |
| Default destination | `$XDG_DOWNLOAD_DIR` if absolute → `XDG_DOWNLOAD_DIR` parsed out of `${XDG_CONFIG_HOME:-$HOME/.config}/user-dirs.dirs` → `$HOME/Downloads` | **none** — `xdg-user-dirs` is present on both targets |
| XDG directories | full three-step resolution above; only the two forms `xdg-user-dirs-update` actually writes are interpreted, *"since running a shell to expand a config file would be a much larger thing to trust"* | none |
| Permissions | destination forced to 0700 if group/other bits are set; every file created mode 0600 | none |
| Filename sanitization | `sanitize()` strips directories — `/etc/shadow` → `shadow` | none |
| Temp directory | **deliberately not `/tmp`** — the temp file sits **beside** its destination | **none, and this is the portable choice**: `rename(2)` is only atomic within one filesystem, and `/tmp` is a separate `tmpfs` on both targets exactly as on Fedora |
| Atomic rename | `std::fs::rename` within the destination directory | none |
| Linux-specific syscalls | **none** — `OpenOptions::create_new(true).mode(0o600)` is the whole trick; no `libc`, no `renameat2`, no `O_TMPFILE` | none |
| Shell commands | **none** | none |

One localization note that is a **property of the code being correct**, not a risk: on a
non-English Ubuntu or Debian install `xdg-user-dirs` creates a localized download folder
(`Téléchargements`, `Descargas`, …). Because the resolver reads `user-dirs.dirs` rather
than assuming `~/Downloads`, it finds it. The `~/Downloads` fallback is only reached when
that file is absent, and it behaves identically on Fedora.

**No Fedora-specific filesystem assumption exists.**

---

## 13. Battery

`battery.v1`'s Linux backend is **UPower over the system bus, and nothing else**
([capabilities/battery/src/upower.rs](desktop/capabilities/battery/src/upower.rs)).

* Object: `/org/freedesktop/UPower/devices/DisplayDevice`, interface
  `org.freedesktop.UPower.Device`, properties `Percentage` and `State`.
* **No sysfs.** No `/sys/class/power_supply` read anywhere in the tree.
* No `upower(1)` shell-out — *"we read the … bus property directly rather than shelling
  out to `upower(1)`"*.
* The whole backend is behind the optional `upower` Cargo feature (default on for the
  daemon), so a build without it is still a valid receive-only `battery.v1`.

| Situation | Behaviour | Distro difference |
| --- | --- | --- |
| Laptop with a battery | `Percentage` clamped to 0–100, `State` mapped to the wire enum | none — `UPower.Device.State` is a stable freedesktop enum |
| **UPower absent** | `Connection::system()` or the proxy fails → `connect()` returns `None` → the capability is receive-only | none |
| **UPower unavailable mid-session** | each `read()` returns `None`; nothing is reported | none |
| Feature compiled out | `connect()` is a `None` stub | none |
| **Desktop without a battery** | UPower's `DisplayDevice` still answers, with `Percentage = 0.0` and `State = 0`, so AnyFlow reports **0 % / Unspecified** rather than "no battery" | **identical on every distribution** |

The last row is a real behavioural wart, but it is **pre-existing and distro-independent**,
so U0 records it as an observation and does **not** promote it to a distro work item.

`upower` is available as a package on all three distributions (Ubuntu 24.04: 1.90.3;
Ubuntu 26.04: 1.91.1; Debian 13: 1.90.9) and is a `Recommends:`-class dependency — which is
already how the Fedora spec declares it.

---

## 14. GTK / libadwaita GUI

This is the question the brief calls critical, and it is answered by **compiling**, not by
reading documentation.

### 14.1 The declared floor

```toml
gtk = { package = "gtk4",       version = "0.9", features = ["v4_12"] }
adw = { package = "libadwaita", version = "0.7", features = ["v1_5"] }
```

In the gtk-rs family these features are enforced by `system-deps` at build time. Read from
the `-sys` crates' own metadata:

```
gtk4-sys 0.9.6       [package.metadata.system-deps.gtk4.v4_12]        version = "4.12"
libadwaita-sys 0.7.2 [package.metadata.system-deps.libadwaita_1.v1_5] version = "1.5"
```

So the declared floor is **GTK ≥ 4.12** and **libadwaita ≥ 1.5**, and it is a hard
`pkg-config` gate, not a suggestion.

### 14.2 The floor the code actually needs — measured

The declared floor was lowered one step at a time and the GUI recompiled. The manifest was
restored unconditionally afterwards and the working tree verified clean.

```console
$ # baseline, as shipped
$ cargo check -p anyflow-gui --locked        →  exit 0

$ # probe 1: gtk4 v4_12 → v4_10
gui/src/lib.rs:66:36: error[E0599]: no method named `load_from_string` found for
                     struct `CssProvider` in the current scope
                                             →  exit 101

$ # probe 2: libadwaita v1_5 → v1_4
gui/src/views/pairing.rs:23:23: error[E0433]: cannot find `Dialog` in `adw`
gui/src/views/peers.rs:171:23:  error[E0433]: cannot find `AlertDialog` in `adw`
                                             →  exit 101
```

**Both floors are real, and these are the exact APIs that set them:**

| Floor | API | Site |
| --- | --- | --- |
| **GTK 4.12** | `gtk::CssProvider::load_from_string` | [gui/src/lib.rs:66](desktop/gui/src/lib.rs#L66) — the theme installer, called again on every light/dark switch |
| **libadwaita 1.5** | `adw::Dialog` | [gui/src/views/pairing.rs:23](desktop/gui/src/views/pairing.rs#L23) — the pairing confirmation |
| **libadwaita 1.5** | `adw::AlertDialog` | [gui/src/views/peers.rs:171](desktop/gui/src/views/peers.rs#L171) — the forget-device confirmation |

> **U-7 — this corrects the earlier desk research.**
> [05-DEBIAN-UBUNTU-COMPATIBILITY.md §2.2](docs/research/platform-expansion/05-DEBIAN-UBUNTU-COMPATIBILITY.md)
> concluded that *"the GTK gap (4.10 vs 4.12) appears to be a conservative pin rather than a
> requirement"*, on the basis that the highest-versioned **type** used is `gtk::FileDialog`
> (4.10). That inventory was correct about types and missed a **method**:
> `CssProvider::load_from_string` is gated `#[cfg(feature = "v4_12")]`. The GTK floor is a
> requirement. The libadwaita half of that research is confirmed exactly.
>
> The floor is nonetheless **cheap to lower if it ever matters**: `CssProvider::load_from_data(&str)`
> is ungated (deprecated since 4.12, still present in gtk4 0.9) and is a one-line
> substitution. There is no reason to make it today — every V1 target is ≥ 4.14.

### 14.3 What the archives actually ship — measured

Queried with `apt-cache policy` and confirmed with `pkg-config --modversion` after a real
install, inside disposable containers.

| Distribution | `gtk4` | `libadwaita-1` | `glib-2.0` | GUI builds? |
| --- | --- | --- | --- | --- |
| **Ubuntu 24.04.4 LTS** | **4.14.5** | **1.5.0** | 2.80.0 | **YES — libadwaita exactly at the floor** |
| **Ubuntu 26.04.1 LTS** | **4.22.4** | **1.9.1** | 2.88.0 | **YES** |
| **Debian 13 trixie (stable)** | **4.18.6** | **1.7.6** | 2.84.4 | **YES** |
| Debian 12 bookworm (oldstable) | 4.8.3 | 1.2.2 | 2.74.6 | **NO** — both floors missed |
| *(host: Fedora 44)* | *4.22.4* | *1.9.3* | *2.88.3* | *yes* |

### 14.4 Conclusion — §19 answered without hand-waving

> **Can the current AnyFlow GTK4/libadwaita GUI compile against stock Ubuntu LTS?
> YES** — against both 24.04 and 26.04.
> **Against stock Debian Stable? YES** — Debian 13 trixie.
> **Classification: not a P0. Not a blocker. No compatibility refactor is required.**

Two qualifications, both of which become work items rather than blockers:

* **U-3, the margin.** Ubuntu 24.04 LTS has libadwaita **1.5.0** against a **1.5** floor.
  There is no margin at all, and nothing in the repository would catch its loss. The next
  person who reaches for `AdwSpinner` (1.6), `AdwBottomSheet` (1.7) or any 1.6+ property
  will not notice, because the development host has 1.9.3. Ubuntu 24.04's standard support
  runs to 2029. **The fix is a CI job, not a code change** — work item **L3**.
* **Debian 12 and Ubuntu 22.04 stay out.** Reaching libadwaita 1.2 means `AdwLeaflet` and
  `AdwFlap` instead of `AdwNavigationSplitView`, `AdwMessageDialog` instead of
  `AdwAlertDialog`, and no `AdwBreakpoint` — a second adaptive shell, not a shim, for two
  releases that are already superseded. **Recommendation: do not lower the floor.** The
  daemon and CLI need no GTK at all and can be offered to those users independently, which
  is an argument for split packages in the later packaging wave.

---

## 15. systemd and the runtime

The brief asks for runtime and packaging to be separated. They are, cleanly.

### The runtime assumes nothing about systemd

| Coupling searched for | Hits in production |
| --- | --- |
| `sd_notify` / `NOTIFY_SOCKET` | **0** |
| socket activation / `LISTEN_FDS` | **0** |
| watchdog / `WATCHDOG_USEC` | **0** |
| `JOURNAL_STREAM`, `INVOCATION_ID` | **0** |
| a `systemd` crate dependency | **0** |

Every `systemd` string in production Rust is prose. `anyflowd` is an ordinary foreground
process that writes to stderr; it runs the same under a unit, under a terminal, or under
any other supervisor.

**It also never aborts on a missing platform service.** Each backend resolves to a
degraded value rather than an error:

```rust
let notification_sink = match DbusSink::connect().await { Some(s) => …, None => Arc::new(NoSink) };
let notification_lock = match LogindLock::connect().await { Some(l) => …, None => /* UnknownLock */ };
if let Some(upower) = UPowerReader::connect().await { … }
```

So a headless box, an SSH session, a container or a machine without logind all start the
daemon successfully with the affected capabilities honestly reporting themselves
unavailable. **Nothing in §15 needs a change for Ubuntu or Debian.**

### What is packaging, and is therefore out of scope here

`packaging/fedora/anyflowd.service` is a **user** unit (`systemctl --user`), correctly so:
the identity key is 0600 in the user's `$XDG_DATA_HOME` and the control socket is in the
user's `$XDG_RUNTIME_DIR`. Its content is distro-neutral and would work unchanged on both
targets — `ProtectSystem=strict`, `SystemCallFilter=@system-service`,
`RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK`, and
`ExecStart=/usr/bin/anyflowd`, which is the correct path on all three distributions.
`AF_NETLINK` in that list is load-bearing rather than defensive, and it is worth recording
why, because a future tightening could remove it: `mdns-sd` enumerates interfaces through
`if-addrs`, which on non-Apple POSIX opens `socket(AF_NETLINK, SOCK_RAW, NETLINK_ROUTE)`
(`if-addrs-0.14.0/src/posix_not_apple.rs:16`). Dropping `AF_NETLINK` would break discovery
on every distribution at once.

Two small inconsistencies noticed while reading it, recorded for the packaging wave and
deliberately **not** fixed here: it declares both `StateDirectory=anyflow`
(`~/.local/state/anyflow`, which the code never uses) and
`ReadWritePaths=%h/.local/share/anyflow` (which it does); and the tree name
`packaging/fedora/` will need to become distro-neutral once a `.deb` exists.

Lingering (`loginctl enable-linger`) behaves identically on all three.

---

## 16. XDG paths

Every environment variable production code reads, with its count:

```
DISPLAY 4 · HOME 3 · XDG_RUNTIME_DIR 2 · XDG_DOWNLOAD_DIR 2
XDG_SESSION_ID 1 · XDG_DATA_HOME 1 · XDG_CONFIG_HOME 1 · WAYLAND_DISPLAY 1 · PATH 1
```

| Variable | Use | Fallback | Portable? |
| --- | --- | --- | --- |
| `XDG_DATA_HOME` | trust store + identity key → `…/anyflow` | `$HOME/.local/share/anyflow` | ✓ |
| `XDG_RUNTIME_DIR` | control socket → `…/anyflow/control.sock` | `/tmp/anyflow-$UID`, created 0700 | ✓ |
| `XDG_CONFIG_HOME` | locating `user-dirs.dirs` | `$HOME/.config` | ✓ |
| `XDG_DOWNLOAD_DIR` | files destination (env, then `user-dirs.dirs`) | `$HOME/Downloads` | ✓ |
| `XDG_SESSION_ID` | third-choice logind session candidate | the other two candidates | ✓ |
| `WAYLAND_DISPLAY` / `DISPLAY` | clipboard backend selection | honest `Unsupported` | ✓ |
| `HOME` | base for the above | `.` (keeps the daemon alive rather than writing to `/`) | ✓ |
| `PATH` | `which()` for `wl-copy` / `wl-paste` | reports the tools missing | ✓ |

**`XDG_CACHE_HOME` is never used, and there is no cache directory** — matching the N6
finding that `~/.cache/anyflow` does not exist.

**There is no hard-coded home path anywhere.** The only absolute paths in production are
`/proc/self/status`, `/etc/hostname`, `/proc/sys/kernel/hostname`, the
`/tmp/anyflow-$UID` fallback, and `/usr/bin/true` + `/usr/bin/echo` as arguments to
`wl-paste` (§7). Behaviour is identical on Fedora, Ubuntu and Debian.

---

## 17. Security frameworks

| Framework | Does AnyFlow depend on it, or on its absence? |
| --- | --- |
| **SELinux** (Fedora default) | **No.** Zero references. The daemon is unprivileged, writes only under `$HOME`, binds an unprivileged port, and needs no policy module or file-context rule. It has run under enforcing SELinux through six certification waves without one |
| **AppArmor** (Ubuntu default) | **No.** Zero references. AnyFlow ships no profile, so it runs unconfined. Ubuntu 24.04+'s `apparmor_restrict_unprivileged_userns` restriction is irrelevant: **nothing in AnyFlow creates a user namespace** — the sandboxing in the unit file is systemd's, applied by a privileged manager |
| **polkit** | **No.** Zero references. No privileged action is ever requested |
| **Firewall** | Needs inbound UDP 5353 and TCP 55432. Fedora's default zone permits both; Ubuntu's `ufw` is inactive by default and Debian enables no firewall by default. **Gate G-FW** measures this rather than assuming it |
| **User groups** | **None.** No `dialout`, `plugdev`, `input` or any other group membership is required |
| **udev rules** | **None.** No device node is opened |
| **Privileged ports** | **None.** 55432 and 5353 are both unprivileged |
| **Filesystem modes** | 0700 directories, 0600 keys, verified on load — enforced by AnyFlow itself, identically everywhere |

**AnyFlow does not depend on either security framework being disabled, and needs no policy
on either.** The one thing a `.deb` will eventually want is the same thing the RPM wants:
nothing. Should a distribution later demand a confinement profile for a network-listening
user daemon, that is packaging-wave work and is recorded as such.

---

## 18. Ubuntu findings

**Targets: Ubuntu 24.04 LTS (noble) and Ubuntu 26.04 LTS (resolute).** Both matter —
24.04 is supported to 2029 and is the version most users have; 26.04 is current.

| Area | Ubuntu 24.04 LTS | Ubuntu 26.04 LTS |
| --- | --- | --- |
| GTK 4 / libadwaita | 4.14.5 / **1.5.0** — builds, **zero margin** (U-3) | 4.22.4 / 1.9.1 — comfortable |
| Default `rustc` | **1.75** — far below what the lockfile needs | **1.93.1** — works |
| Named Rust packages | `rustc-1.74`, `1.76`…`1.82`, `1.83`, `1.84`, `1.85`, `1.89`, `1.91`. **`rustc-1.82` is *not* enough** (U-4): the lockfile needs **1.88**, so the archive answer is **`rustc-1.91` + `cargo-1.91`**. The archive alone does suffice — rustup is a convenience, not a requirement | n/a |
| `protoc` | not needed | not needed |
| OpenSSL | not needed | not needed |
| wl-clipboard | **2.2.1 — no `--sensitive`** (U-1, measured) | **2.2.1 — no `--sensitive`** (U-1, measured) |
| `wl-copy`/`wl-paste` path | `/usr/bin/` ✓ | `/usr/bin/` ✓ |
| `/usr/bin/true`, `/usr/bin/echo` | present ✓ | present ✓ |
| gnome-shell | 46.0 | 50.1 — the same generation as the Fedora 44 certification host (50.4) |
| systemd | 255.4 | 259.5 |
| `dbus-user-session` | **hard dependency of `ubuntu-desktop-minimal`** ✓ | ✓ |
| upower / xwayland / xdg-user-dirs | 1.90.3 / 23.2.6 / 0.18 available | 1.91.1 / 24.1.10 / 0.19 available |
| Firewall | `ufw` installed, inactive by default — **G-FW** | same |
| Security framework | AppArmor — no dependency either way | same |

**Ubuntu-specific risks worth naming:**

1. **The session may not be Wayland.** Ubuntu is known to select an Xorg session in some
   hardware configurations (classically with the NVIDIA proprietary driver) — *this is
   background knowledge, not something U0 measured, and G-SESSION exists to settle it per
   target*. If it happens, `WAYLAND_DISPLAY` is unset and `detect_linux()` returns the
   `Unsupported` clipboard backend
   with an honest message — *"this is an X11 session; clipboard.v1 currently implements the
   Wayland backend only"*. Mirroring, files, battery and pairing are unaffected. This is a
   **known capability boundary meeting a plausible Ubuntu default**, so the VM matrix must
   record which session type it certified (gate **G-SESSION**).
2. **Clipboard auto-send needs Xwayland.** On GNOME the change watch is the XFIXES bridge,
   which needs `DISPLAY` to point at a running Xwayland. `xwayland` is packaged on both
   releases and is part of a normal GNOME install; a session deliberately configured
   without it loses **auto-send only** (manual send still works, and the daemon says so).
   Gate **G-CLIP-WATCH**.
3. **U-3's fragility is Ubuntu 24.04's alone.** Nothing else on the matrix is within two
   minor versions of a floor.

---

## 19. Debian findings

**Target: Debian 13 trixie — the current Debian Stable.**

| Area | Debian 13 trixie |
| --- | --- |
| GTK 4 / libadwaita | **4.18.6 / 1.7.6** — builds, with margin |
| `rustc` / `cargo` | **1.85.1 / 1.85.1** — above the *declared* MSRV 1.82 but **below the lockfile's real 1.88 floor: the stock archive does NOT suffice** (U-4). Use `trixie-backports` (`rustc` 1.94.1) or rustup |
| `protoc` | not needed |
| OpenSSL | not needed |
| wl-clipboard | **2.2.1 — no `--sensitive`** (U-1, measured) |
| `wl-copy`/`wl-paste` path | `/usr/bin/` ✓ |
| `/usr/bin/true`, `/usr/bin/echo` | present ✓ |
| gnome-shell | 48.7 |
| systemd | 257.13 |
| `dbus-user-session` | **hard dependency of `gnome-core`** ✓ |
| upower / xwayland / xdg-user-dirs | 1.90.9 / 24.1.6 / 0.18 available |
| Firewall | none enabled by default — **G-FW** |
| Security framework | AppArmor available; no dependency either way |

**Debian is the easier of the two targets for the GUI and the harder one for the
toolchain**, and both halves are worth stating plainly. Debian Stable's GTK stack (4.18.6 /
1.7.6) is *newer* than Ubuntu 24.04 LTS's and has real headroom above the libadwaita floor —
so the U-3 fragility is Ubuntu's alone. But Debian's `rustc` 1.85.1 is the **only stock
toolchain on any V1 target that cannot build the committed lockfile**, which makes Debian 13
the target that surfaces U-4. The two facts are independent and must not be collapsed into a
single "Debian is fine" or "Debian is a problem".

**Debian-specific notes:**

1. **Debian 12 bookworm is out** (GTK 4.8.3, libadwaita 1.2.2, rustc 1.63 — all three
   below their floors). It is oldstable. Daemon and CLI could in principle be offered with a
   non-archive toolchain since they need no GTK; the GUI cannot. Recorded as a decision in
   §14.4, not as a failure.
2. **elogind** exists in Debian for non-systemd installs and serves the same
   `org.freedesktop.login1` interface. Debian GNOME's default is systemd. No action.
3. Nothing in the audit produced a single Debian-only code risk.

---

## 20. Compile probes

Disposable containers, `podman` 5.8.4. **A container result is a compilation and
dependency fact. It is explicitly not a desktop certification**, and §21 exists because of
that.

### 20.1 Method

```console
$ podman pull docker.io/library/{ubuntu:24.04,ubuntu:26.04,debian:trixie,debian:bookworm}
$ podman run --rm -v <probe>:/p.sh:ro -v /home/yuri/Sandbox/anyflow:/src:ro \
      docker.io/library/<image> sh /p.sh
```

Four kinds of probe were run: an **archive-version probe** (`apt-cache policy` for every
dependency, then `pkg-config --modversion` after a real install); a **runtime-binary
probe** (`wl-copy --version`, `wl-copy --help`, tool paths, `/usr/bin/true`,
`/usr/bin/echo`); a **toolchain-availability probe** (`apt-cache search '^rustc-1\.'`); and
a **full compile probe** (`cargo metadata --locked`, then `cargo check` of the daemon + CLI
and of the GUI, against the distribution's own `rustc`, `cargo`, GTK and libadwaita).

### 20.2 What the probes established

| Fact | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 | Debian 12 |
| --- | --- | --- | --- | --- |
| `cargo metadata --locked` resolves | ✓ | ✓ | ✓ | — |
| `protoc` needed | **no** | **no** | **no** | — |
| OpenSSL needed | **no** | **no** | **no** | — |
| `glib-compile-resources` present after `libgtk-4-dev` | ✓ `/usr/bin/` | ✓ | ✓ `/usr/bin/` | — |
| `pkg-config gtk4` after install | 4.14.5 | 4.22.4 | 4.18.6 | 4.8.3 |
| `pkg-config libadwaita-1` after install | 1.5.0 | 1.9.1 | 1.7.6 | 1.2.2 |
| `wl-copy --sensitive` | **NO** | **NO** | **NO** | NO |

### 20.3 The MSRV finding — U-4, and it is the sharpest result in this wave

The first full compile probe, using Debian 13's **stock** `rustc` 1.85.1, failed before
reaching a single line of AnyFlow's own code:

```console
--- cargo check -p anyflow-daemon -p anyflow-cli ---
  zbus@5.19.0        requires rustc 1.87
  zvariant@5.15.0    requires rustc 1.87
  zcheapstr@1.1.0    requires rustc 1.87
--- cargo check -p anyflow-gui ---
error: rustc 1.85.1 is not supported by the following packages:
  rcgen@0.14.10      requires rustc 1.88
  time@0.3.55        requires rustc 1.88.0
  time-macros@0.2.32 requires rustc 1.88.0
```

Computed exactly from the committed lockfile:

```console
$ cargo metadata --format-version 1 --locked | (highest rust_version)
      1.88.0  time 0.3.55 / time-core / time-macros
        1.88  rcgen 0.14.10
        1.87  zbus 5.19.0 / zvariant / zbus_names / zvariant_utils / zcheapstr

EFFECTIVE MSRV OF THE COMMITTED Cargo.lock : 1.88
DECLARED workspace rust-version            : 1.82
locked crates demanding more than 1.82     : 34
```

> **`rust-version = "1.82"` in `desktop/Cargo.toml` is not true of the tree as it is
> committed. The real floor is 1.88.**

This is not cosmetic. It is the difference between "Debian Stable builds AnyFlow out of
the box" and "Debian Stable does not":

| Toolchain | Declared MSRV says | Reality | How known |
| --- | --- | --- | --- |
| Debian 13 stock `rustc` **1.85.1** | fine (≥ 1.82) | **fails** | **measured** — the probe above |
| Ubuntu 24.04 archive `rustc-1.82` | fine (= 1.82) | **fails** | inferred: 1.82 < 1.88, by the same rule that failed 1.85.1 |
| **Ubuntu 24.04 archive `rustc-1.91`** | fine | **works** | **measured** — §20.4 |
| **Debian 13 + rustup stable** | fine | **works** | **measured** — §20.4 |
| Ubuntu 26.04 stock `rustc` **1.93.1** | fine | works | inferred: 1.93 ≥ 1.88; not compiled in this wave |
| Debian 13 `trixie-backports` `rustc` 1.94.1 | fine | works | inferred; the backports version is from the earlier research, not re-checked here |

The earlier desk research compared archive `rustc` against the **declared** 1.82 and
therefore recorded Debian 13 as "✅ 1.85" and Ubuntu 24.04 as needing the named
`rustc-1.82` package. Both conclusions are wrong for the tree as committed. That is the
single most valuable thing a compile probe bought over reading archives, and it is why
§25 makes an MSRV declaration and a CI job the first two work items.

Note the two halves are independent and must stay so: this is a **Rust toolchain** floor,
not a GTK floor. Debian 13's GTK 4.18.6 and libadwaita 1.7.6 are comfortably above the
GUI's requirement (§14); its `rustc` is not above the lockfile's.


### 20.4 The workspace compiled, in both distributions, against their own GTK

With an adequate toolchain in place, each distribution built the whole thing — **including
the GUI, against that distribution's own GTK and libadwaita**, not the host's.

**A — Ubuntu 24.04 LTS, the zero-margin case.** Archive toolchain only; no rustup.

```console
########## Ubuntu 24.04.4 LTS ##########
gtk4: 4.14.5   libadwaita-1: 1.5.0
archive rustc: rustc 1.91.1 (ed61e7d7e 2025-11-07)          ← apt install rustc-1.91 cargo-1.91

--- cargo check -p anyflow-daemon -p anyflow-cli ---
    Finished `dev` profile … in 1m 36s

--- cargo check -p anyflow-gui (against THIS distro's GTK/libadwaita) ---
    Checking gtk4 v0.9.7
    Checking libadwaita v0.7.2
    Finished `dev` profile … in 1m 44s
```

**This is the single most important result in the wave.** `anyflow-gui` compiled against
**libadwaita 1.5.0** — the exact version the floor demands, with nothing to spare — using a
toolchain Ubuntu ships in its own archive. U-3 is a *fragility*, not a present defect.

**B — Debian 13 trixie.**

```console
########## Debian GNU/Linux 13 (trixie) ##########
gtk4: 4.18.6   libadwaita-1: 1.7.6
rustup rustc: rustc 1.98.1 (48a229cea 2026-09-01)

--- cargo check -p anyflow-daemon -p anyflow-cli ---
    Finished `dev` profile … in 1m 03s

--- cargo check -p anyflow-gui (against THIS distro's GTK/libadwaita) ---
    Checking libadwaita v0.7.2
    Finished `dev` profile … in 1m 04s
```

**Debian Stable's GTK stack is fine; only its `rustc` was not.** Swapping the toolchain and
changing nothing else took the same tree from the §20.3 failure to a clean build.

**What these two runs do and do not prove.** They prove that the committed tree compiles —
daemon, CLI and GUI — against Ubuntu 24.04's and Debian 13's own GTK, libadwaita, glib and
C toolchain, with no `protoc`, no OpenSSL and no `libdbus`. They **do not** prove the 1.88
figure is the tight lower bound: 1.88 is computed from the lockfile's declared
`rust_version` metadata (§20.3), while the compiles above ran at 1.91 and 1.98.1. Pinning
a CI job to exactly the declared MSRV (L1/L3) is what would make 1.88 an enforced number
rather than a derived one. And **neither run is a certification** — a container has no
session bus, no logind session, no compositor and no notification server. Everything beyond
"it builds" is §21's job.

---

## 21. VM certification matrix

### 21.1 Host and hardware strategy

The development host is Fedora 44 and it already has everything needed:
**libvirt 12.0.0, QEMU 10.2.2, virt-manager and GNOME Boxes are all installed.**

**Recommended: libvirt/QEMU with virt-manager, bridged networking, and adb over Wi-Fi.**
The reasoning is that the hard part is not the VM, it is the LAN:

* AnyFlow's discovery is **mDNS on the local link** and its transport is **TCP to an
  address the phone dials**. Under libvirt's default **NAT** network the VM sits on
  `192.168.122.0/24` behind the host, so mDNS from the phone's Wi-Fi segment never reaches
  it and the phone cannot open a TCP connection to it. **NAT cannot certify discovery or
  pairing**, which is most of the suite.
* A **bridged** interface puts the VM on the same link as the phone. Then mDNS, TCP 55432
  and the whole capability suite work exactly as they do on bare metal, and the phone
  treats the VM as one more computer on the network.
* **USB passthrough is not needed and should be avoided.** Passing the tablet's USB
  through to the guest complicates the host's own `adb` and risks the instability recorded
  in previous waves. **Use `adb` over Wi-Fi from the host** (`adb tcpip 5555` once over
  USB, then `adb connect <phone-ip>:5555`) and drive the phone from the host while the VM
  is just a peer on the LAN. The VM never needs to see the phone over USB — AnyFlow's
  whole protocol is network-based.
* GNOME Boxes is fine for creating the images but defaults to NAT; **virt-manager is the
  tool that exposes bridged networking**, so the VMs should be created or reconfigured
  there.

### 21.1.1 The one real obstacle on this host, measured

```console
$ virsh net-dumpxml default | grep -E "forward|ip address"
  <forward mode='nat'>
  <ip address='192.168.122.1' netmask='255.255.255.0'>

$ ip -br link
lo               UNKNOWN   …
wlp0s20f3        UP        …          ← the only link with carrier: Wi-Fi
enp0s13f0u2u2c2  DOWN      <NO-CARRIER…>   ← USB Ethernet, unplugged
virbr0           DOWN      …
```

**The host's only live interface is Wi-Fi, and a VM cannot reliably be bridged onto a Wi-Fi
interface.** 802.11 associates one MAC per client, and most access points drop frames whose
source MAC is not the associated station's — so a Linux bridge or a macvtap over `wlp0s20f3`
will see the guest's traffic leave and nothing come back. This is not a libvirt setting that
can be changed; it is how the link layer works.

So the certification needs one of these, **decided before the VMs are built**:

| Option | Verdict |
| --- | --- |
| **Plug in the USB Ethernet adapter (`enp0s13f0u2u2c2`) and bridge over it** | **Recommended.** It is already present, just has no carrier. One cable turns the whole plan into the straightforward case, and the phone stays on Wi-Fi on the same LAN |
| macvtap in bridge mode on the wired NIC | Equivalent; note the well-known caveat that host↔guest traffic does not flow, which does not matter here because the *phone* is the peer, not the host |
| Run the daemon under test on a **second physical machine** instead of a VM | Cleanest evidence of all, and worth considering for at least one of the two targets |
| **libvirt NAT + port forwarding** | **Rejected.** Multicast does not cross the NAT, so **G-MDNS and G-PAIR cannot be certified**, and those are most of the suite. A pass obtained this way would prove almost nothing |
| Wi-Fi bridge via 4-address / WDS mode | Rejected — depends on AP support that is usually absent, and debugging it is not this project's work |

One consequence to plan for either way: with a bridge, the **host's own `anyflowd`** and the
VM's will both advertise `_anyflow._tcp.local.` on the same link. That is supported
(instance names are device ids, not hostnames — §11) but the person running the
certification should stop the host daemon to keep the evidence unambiguous.

**VM automation is explicitly not implemented in this wave.**

### 21.2 The two targets

| | **Target A** | **Target B** |
| --- | --- | --- |
| Distribution | **Ubuntu 24.04 LTS** (noble) | **Debian 13 trixie** (stable) |
| Why this one | the LTS most users have; **the zero-margin libadwaita case** (U-3) | current Debian Stable; the stock-`rustc` case (U-4) |
| Desktop | GNOME (default session) | GNOME (default session) |
| Display protocol | **Wayland** — recorded explicitly, because Ubuntu can fall back to Xorg (§18) | **Wayland** |
| Init / session | systemd 255.4, GDM, real graphical user session, **not** a container and **not** `ssh` | systemd 257.13, GDM |
| Image | `ubuntu-24.04.x-desktop-amd64.iso` | `debian-13.x.0-amd64-DVD/netinst` + GNOME task |
| Networking | **bridged** to the host LAN | **bridged** |
| Packages to install | `gcc libc6-dev pkg-config libgtk-4-dev libadwaita-1-dev libglib2.0-dev wl-clipboard upower` | same |
| Rust toolchain | **`rustc-1.91` + `cargo-1.91` from the archive** (proves the archive-only path) | **rustup stable**, *or* `trixie-backports` `rustc` — the stock 1.85.1 will not do (U-4) |
| Phone connectivity | `adb connect` over Wi-Fi from the **host**; the phone reaches the VM over the bridge | same |

A third, optional target — **Ubuntu 26.04 LTS** — is the easiest of the three (stock
`rustc` 1.93.1, GTK 4.22.4, libadwaita 1.9.1, gnome-shell 50.1, i.e. nearly the Fedora 44
configuration). It should be certified **after** A and B, because it proves the least: if
anything is going to break, it breaks on 24.04 or on Debian's toolchain, not there.

### 21.3 Gates, per target

Every gate is run on both A and B. **"Works on Fedora" is never accepted as evidence.**

| Gate | What it proves | How it is judged |
| --- | --- | --- |
| **G-BUILD** | `cargo build --release --locked` for the whole workspace, with the named toolchain | binary produced; the toolchain used is recorded |
| **G-TEST** | `cargo test --workspace --locked` | pass count recorded and compared with the host's certified 703 (N6). U0 did **not** re-run the suite on the host — that figure is N6's and is cited as N6's |
| **G-DAEMON** | `anyflowd` starts in the user session, logs its backends | the startup lines naming the notification server, the lock source and the listener families |
| **G-GUI** | `anyflow-gui` launches, renders, and the pairing + peers dialogs open | **specifically the `AdwDialog`/`AdwAlertDialog` screens** — they are the libadwaita 1.5 APIs, and 24.04 is exactly at 1.5.0 |
| **G-MDNS** | the phone discovers the VM | the device appears in the Android app without a manual address |
| **G-FW** | discovery and TCP 55432 work **with the distribution's default firewall state**, unmodified | recorded as "default state was X, no change made" — or, if a change was needed, exactly which |
| **G-PAIR** | full TLS 1.3 pairing with fingerprint confirmation | fingerprints compared on both screens |
| **G-SESSION** | which session type was certified | `echo $XDG_SESSION_TYPE` = `wayland`, recorded |
| **G-BATTERY** | `battery.v1` on a machine where it is meaningful | a VM has no battery, so this is either run on a laptop guest with `qemu` battery emulation or **recorded as not meaningfully testable in a VM** — do not fake a pass |
| **G-FILES** | `files.v1` both directions, into the **localized** XDG download dir | file arrives, mode 0600, correct directory |
| **G-CLIP** | `clipboard.v1` manual send both directions | byte-identical paste in a real application |
| **G-CLIP-WATCH** | auto-send, i.e. the **Xwayland XFIXES** bridge | `anyflow clipboard status` names the watch source; a copy triggers a send |
| **G-CLIP-SENS** | **U-1** — a `sensitive_hint` clip from the phone | expected: **refused with the explanatory message**. This gate passes by observing the honest refusal, not by the clip arriving |
| **G-NOTIF** | `notifications.v1` mirror, update-in-place, remove, snapshot resync | the N6 smoke steps 1–6, 9, 12–15 |
| **G-NOTIF-CAP** | the sink adapted to *this* server | the `notification server …` log line, with the capabilities read from `GetCapabilities` |
| **G-LOCK** | logind session resolution and `LockedHint` | lock the screen; `anyflow notifications status` and the daemon log agree with `loginctl` |
| **G-RECONNECT** | Wi-Fi outage inside and beyond the grace | mirrors survive a short outage, converge after a long one |
| **G-RESTART** | `systemctl --user restart anyflowd` | the peer reconnects with no manual step |
| **G-UNIT** | the systemd user unit runs under this distribution's systemd with its hardening intact | `systemctl --user status` clean; no `ProtectSystem`/`SystemCallFilter` denial in the journal |

Containers are acceptable evidence for **G-BUILD** only. They are **not** acceptable for
G-DAEMON, G-GUI, G-MDNS, G-FW, G-PAIR, G-CLIP*, G-NOTIF*, G-LOCK, G-RECONNECT, G-RESTART
or G-UNIT — every one of those needs a real graphical session, a real session bus, a real
logind session and a real link.

---

## 22. Distro matrix

Status vocabulary is the brief's. Fedora 44 is the certified reference.

| Component | Fedora 44 | Ubuntu 24.04 LTS | Ubuntu 26.04 LTS | Debian 13 | Required change | Evidence needed |
| --- | --- | --- | --- | --- | --- | --- |
| **Rust daemon** (`anyflowd`) | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none to the code — but see the toolchain row | G-BUILD, G-DAEMON |
| **CLI** (`anyflow`) | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none | G-BUILD |
| **Rust toolchain** (lockfile floor **1.88**) | WORKS AS-IS (1.98) | **stock 1.75 ✗ · `rustc-1.82` ✗ · `rustc-1.91` ✓ from the archive** | stock **1.93.1 ✓** | **stock 1.85.1 ✗** — backports 1.94.1 ✓ or rustup ✓ | **CODE CHANGE REQUIRED**: declare `rust-version = "1.88"` truthfully (L1), then document the working toolchain per distribution | G-BUILD, plus an MSRV CI job (L3) |
| **GUI** (GTK4/libadwaita) | WORKS AS-IS | **LIKELY WORKS — NEEDS REAL TEST** (libadwaita exactly 1.5.0) | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none to compile; **CI job** to keep it true (U-3) | G-BUILD, **G-GUI** |
| **mDNS discovery** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none — pure Rust, no Avahi | G-MDNS, G-FW |
| **TLS / transport** | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | none — rustls/ring, no OpenSSL | G-BUILD |
| **Pairing / trust store** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none | G-PAIR |
| **battery.v1** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none — UPower is freedesktop | G-BATTERY (VM caveat) |
| **files.v1** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none | G-FILES |
| **clipboard Wayland (read/write)** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | **U-2** remediation string | G-CLIP |
| **clipboard sensitive clips** | WORKS AS-IS | **BLOCKED BY DISTRO** (U-1) | **BLOCKED BY DISTRO** (U-1) | **BLOCKED BY DISTRO** (U-1) | decision + truthful surfacing (L2) | G-CLIP-SENS |
| **clipboard auto-send watch** | WORKS AS-IS (XFIXES) | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none; depends on Xwayland | G-CLIP-WATCH |
| **clipboard X11 read/write** | NOT IMPLEMENTED | NOT IMPLEMENTED | NOT IMPLEMENTED | NOT IMPLEMENTED | out of V1 scope on every distro | — |
| **notifications.v1** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none — spec-driven, capabilities read at runtime | G-NOTIF, G-NOTIF-CAP |
| **lock detection (logind)** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | LIKELY WORKS — NEEDS REAL TEST | none | G-LOCK |
| **control socket** | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | none — `$XDG_RUNTIME_DIR` | G-DAEMON |
| **state paths** | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | WORKS AS-IS | none — full XDG | G-FILES |
| **systemd integration** | WORKS AS-IS | **PACKAGING CHANGE REQUIRED** | PACKAGING CHANGE REQUIRED | PACKAGING CHANGE REQUIRED | later wave: distro-neutral unit path + `.deb` | G-UNIT, G-RESTART |
| **firewall** | WORKS AS-IS | LIKELY WORKS — NEEDS REAL TEST (`ufw` inactive by default) | same | LIKELY WORKS — NEEDS REAL TEST (none by default) | later wave: a documented recipe | G-FW |
| **security framework** | WORKS AS-IS (SELinux) | WORKS AS-IS (AppArmor, unconfined) | WORKS AS-IS | WORKS AS-IS | none | G-DAEMON, G-UNIT |
| **Documentation / packaging tree** | WORKS AS-IS | **PACKAGING CHANGE REQUIRED** (U-6) | same | same | de-Fedora the README and build docs | — |

**Debian 12 bookworm and Ubuntu 22.04 LTS: BLOCKER for the GUI, by decision (§14.4).**
Daemon and CLI remain technically reachable with a non-archive toolchain; the GUI does not.
Neither is a V1 target.

---

## 23. KDE scope boundary

Everything below is **out of scope for this wave and must not be solved in it.** Ubuntu and
Debian on **GNOME** come first; KDE is a separate wave with its own certification.

| Not in this wave | Why it is KDE's problem, not Ubuntu/Debian's |
| --- | --- |
| KWin's notification server quirks (`Plasma` implements `org.freedesktop.Notifications` with a different capability set) | the sink already reads `GetCapabilities` at runtime; what changes on Plasma is the *answer*, and that needs a Plasma session to measure |
| KDE clipboard: **`wlr-data-control` vs `ext-data-control-v1`** | prior research established that KWin dropped its `wlr-data-control` overlay in Plasma 6.5 while every current LTS ships wl-clipboard 2.2.1 (which only speaks `wlr`), leaving **Ubuntu 26.04 + Plasma** with no common protocol. **This is a KDE finding, and it is not the same thing as U-1** — U-1 is about `--sensitive` on GNOME, and it bites on every distro in this wave |
| Whether the Xwayland XFIXES fallback works under KWin | on GNOME it is the *primary* watch and is certified; on Plasma it is an untested fallback |
| KDE screen-lock / session semantics | Plasma uses logind `LockedHint` too, but the transitions must be measured, exactly as POC-NOTIF-02 did for GNOME |
| Plasma/Qt GUI integration, theming, tray | the GTK GUI runs on Plasma but has not been looked at |
| `kde-config`, KWallet, Plasma-specific paths | none are referenced today and none should be added here |

**Do not add a `WatchSource` variant, a compositor probe, a KDE branch or a Plasma
workaround in the Ubuntu/Debian wave.** If the Ubuntu/Debian certification uncovers
something that looks like it needs one, record it and hand it to the KDE wave.

---

## 24. Blockers

**None.**

No P0. No BLOCKER for Ubuntu 24.04 LTS, Ubuntu 26.04 LTS or Debian 13 trixie.

Stated precisely, because two findings could be mistaken for blockers:

* **U-4 (MSRV) is not a blocker** — it is a one-line truth correction plus documentation.
  Every V1 target has a toolchain that works: Ubuntu 24.04 `rustc-1.91` from its own
  archive, Ubuntu 26.04 stock, Debian 13 backports or rustup. Nothing must be rewritten,
  and no dependency must be downgraded. What is broken today is the *claim*, not the
  build.
* **U-1 (`--sensitive`) is not a blocker** — it is a distribution capability gap that the
  code already handles fail-closed, with an explanation, and reports in `status`. It
  degrades one privacy-preserving path of one capability. It needs a **decision** and
  honest surfacing, not a fix in the compatibility sense.

The one thing that *is* a hard boundary is a deliberate scope decision rather than a
defect: **Debian 12 and Ubuntu 22.04 cannot run the GUI and will not be supported** (§14.4).

---

## 25. Implementation plan

**One branch.** The delta does not justify splitting: there is no architectural change, no
protocol change and no new platform seam. What the wave actually contains is a manifest
declaration, a status line, a CI workflow, one error string and a documentation pass.
Splitting that across two branches would mean two certification passes over the same VMs
for no benefit.

**Proposed branch: `feature/linux-debian-ubuntu-compat-v1`.**

Ordered by dependency: **L1 before L3**, because the MSRV job has nothing to enforce until
the number is true; **L3 before the §26 certification**, because otherwise the certification
is a one-off rather than something the repository can keep. L2, L4 and L5 are independent
of the others and of each other.

---

### L1 — Declare the true MSRV

| | |
| --- | --- |
| **Problem** | `rust-version = "1.82"` is not true of the committed tree. The lockfile's real floor is **1.88** (`time`, `rcgen` at 1.88; `zbus`/`zvariant` at 1.87), and 34 locked crates demand more than 1.82. Debian 13's stock `rustc` 1.85.1 and Ubuntu 24.04's `rustc-1.82` both fail before compiling any AnyFlow code (U-4, §20.3) |
| **Files** | [desktop/Cargo.toml:21](desktop/Cargo.toml#L21) · [packaging/fedora/anyflow.spec:12](packaging/fedora/anyflow.spec#L12) (`BuildRequires: rust >= 1.82`) · [README.md:73-78](README.md#L73-L78) |
| **Change** | Set `rust-version = "1.88"`. Correct the spec's `BuildRequires`. Replace the README's "Needs a Rust toolchain" with a per-distribution table naming a toolchain that actually works on each target |
| **Rejected alternative** | Pinning `zbus`, `rcgen` and `time` back to keep a 1.82 floor. That means downgrading the D-Bus client all three platform backends share, and the certificate generator, to support a toolchain **no V1 target needs** — Ubuntu 24.04 carries `rustc-1.91` in its own archive and Debian 13 has backports. Trading current security-relevant dependencies for a synthetic compatibility claim is the wrong direction |
| **Test** | A CI job running `cargo check --workspace --locked` on **exactly** the declared MSRV toolchain, so the number is enforced rather than asserted. Without this, the next dependency bump re-creates the defect silently — which is how it got here |
| **Real-environment gate** | G-BUILD on both VM targets |
| **Risk** | **Low.** The one real risk is that 1.88 drifts upward again unnoticed, which is precisely what the MSRV job in L3 prevents |

---

### L2 — Decide and surface the sensitive-clip gap

| | |
| --- | --- |
| **Problem** | `wl-copy --sensitive` (wl-clipboard ≥ 2.3.0) does not exist on Ubuntu 24.04, Ubuntu 26.04 or Debian 13 — all ship 2.2.1 (measured, §8). Every clip Android marks `EXTRA_IS_SENSITIVE` is therefore refused on all three, and accepted on Fedora. The refusal is correct and fail-closed; what is missing is that a user only discovers it at the moment a password fails to arrive (U-1) |
| **Files** | [wayland.rs:315-341](desktop/capabilities/clipboard/src/backend/wayland.rs#L315-L341) and [:472-517](desktop/capabilities/clipboard/src/backend/wayland.rs#L472-L517) · the clipboard status renderer in [cli/src/main.rs](desktop/cli/src/main.rs) · [gui/src/views/clipboard.rs](desktop/gui/src/views/clipboard.rs) · [docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md) |
| **Change** | **Keep the refusal.** Surface the unsupported state **proactively**: `anyflow clipboard status` and the GUI clipboard page should say "sensitive clips: not supported — this system's wl-clipboard is older than 2.3.0" *before* the user tries, rather than only in the failure text. Record it in `CLIPBOARD.md` as a named platform limitation with the exact version requirement |
| **Rejected alternatives** | (a) Writing the clip unmarked with a warning — already considered and rejected at the call site (PLAT-DEC-013): it converts a visible failure into an invisible privacy regression. (b) Implementing a native Wayland clipboard writer so AnyFlow no longer depends on `wl-copy`'s flag — a genuine solution, and far too large for a compatibility wave. If it is ever wanted it needs its own ADR |
| **Test** | Unit tests over `probe_sensitive_from_output` with the **real** `wl-copy --help` text from 2.2.1 and from 2.3.0. The function was deliberately split out from the process spawn for exactly this, and the help text can be captured from the containers used in §20 |
| **Real-environment gate** | **G-CLIP-SENS** — which passes by *observing the honest refusal and the proactive status line*, not by the clip arriving |
| **Risk** | **Low** for the code. The **product** risk is real and belongs to whoever owns the roadmap: on Ubuntu and Debian, AnyFlow will not sync password-manager clips at all until those distributions ship wl-clipboard 2.3.0 |

---

### L3 — A Linux CI matrix, and an MSRV job

| | |
| --- | --- |
| **Problem** | There is **no Linux CI job** — `.github/workflows/` contains only `portable-windows-msvc.yml` (U-5). And Ubuntu 24.04 LTS sits on libadwaita **1.5.0** against a **1.5** floor with zero margin and nothing guarding it; the next use of a 1.6+ API silently drops an LTS supported to 2029, and will not be noticed because the development host has 1.9.3 (U-3) |
| **Files** | `.github/workflows/linux-distro-matrix.yml` (new) |
| **Change** | A container matrix — `ubuntu:24.04` (archive `rustc-1.91`/`cargo-1.91`, GTK 4.14.5, libadwaita **1.5.0**), `ubuntu:26.04`, `debian:trixie` — each running `cargo check --locked` for `anyflow-daemon`, `anyflow-cli` **and `anyflow-gui`**. Plus one **MSRV job** pinning the exact declared toolchain (L1). The 24.04 row is the load-bearing one: it is the only place the libadwaita floor is actually exercised |
| **Conventions to match** | The existing Windows workflow's discipline, which is good and should not be re-invented: `pull_request` + `push` on `[main, develop]` only (so a feature branch does not run twice), and a `concurrency` group keyed on `github.ref` with `cancel-in-progress` |
| **Test** | The job is the test. It fails the day someone reaches for `AdwSpinner` |
| **Real-environment gate** | None — CI is compile evidence only, and the workflow should say so in a comment so a green tick is never mistaken for a desktop certification |
| **Risk** | **Medium**, and it is cost rather than correctness: three containers each building the GTK dependency tree is not a fast job. Mitigations: `cargo check` rather than `build`, a shared cargo registry cache, and running the GUI row only on the three distro images rather than on every combination |

---

### L4 — Distro-neutral remediation text

| | |
| --- | --- |
| **Problem** | The only user-facing string in the product that names a distribution tells every user to run `sudo dnf install wl-clipboard` (U-2) |
| **Files** | [wayland.rs:130-135](desktop/capabilities/clipboard/src/backend/wayland.rs#L130-L135) |
| **Change** | Name the **package**, not one package manager. The package is called `wl-clipboard` on Fedora, Ubuntu and Debian alike, which makes this easy to say once and correctly |
| **Test** | One assertion that the message names the package and no single distribution |
| **Real-environment gate** | Observed incidentally during G-CLIP if the tools are absent; not worth a gate of its own |
| **Risk** | **Trivial** |

---

### L5 — De-Fedora the documentation

| | |
| --- | --- |
| **Problem** | `README.md` has a section called *"Running on Fedora"* whose build instructions say `sudo dnf install gcc`; `docs/architecture/OVERVIEW.md`'s diagram labels the desktop side "Fedora"; `CLIPBOARD.md` and the README describe the product as "Fedora ↔ Android" (U-6) |
| **Files** | [README.md](README.md) · [docs/architecture/OVERVIEW.md](docs/architecture/OVERVIEW.md) · [docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md) |
| **Change** | "Running on Linux", with a per-distribution prerequisites table (from §6) and the toolchain guidance from L1. Replace "Fedora" with "Linux" where it names the *platform* |
| **Explicitly not touched** | **ADRs** — accepted decision records are not rewritten. **Certification reports** (`NOTIFICATIONS-V1-*`, `CLIPBOARD-V1-*`, `WAVE-0-*`) — accepted history, and their "Fedora" references are *true statements about the host the evidence was gathered on*. The distinction to hold onto while editing: *"Fedora was the certification host"* stays; *"Fedora is the platform"* goes |
| **Test** | None mechanical |
| **Real-environment gate** | None |
| **Risk** | **Trivial**, with one judgement call — see the distinction above |

---

### Deferred, deliberately

| Item | Wave |
| --- | --- |
| `.deb` packaging, split daemon/GUI packages, `packaging/` tree rename | **packaging wave** — the brief forbids it here |
| A `.desktop` file **and an AppStream `metainfo.xml`**, so the `desktop-entry` hint resolves and the GUI has a launcher entry (U-8) | **packaging wave** — pre-existing and identical on Fedora |
| `StateDirectory=anyflow` vs `ReadWritePaths=%h/.local/share/anyflow` in the unit (§15) | **packaging wave** |
| Firewall recipes (`firewalld` service file, `ufw allow`) | **packaging wave** |
| Anything KDE (§23) | **KDE wave** |
| A native Wayland clipboard writer that removes the `wl-copy` dependency | its own ADR, if ever |
| `battery.v1` reporting 0 % on a batteryless desktop (§13) | pre-existing, distro-independent; not this wave's |

---

## 26. Acceptance plan for the next wave

The implementation wave is accepted when **both** targets pass **every** gate in §21.3 on a
real graphical session, and the evidence says which distribution produced it.

| Criterion | Ubuntu 24.04 LTS | Debian 13 trixie |
| --- | --- | --- |
| build (`cargo build --release --locked`, workspace) | G-BUILD, archive `rustc-1.91` | G-BUILD, backports or rustup |
| test suite (`cargo test --workspace --locked`) | G-TEST — the pass count is recorded and compared against the host's certified figure (**703 at N6**); a difference is a finding, not a rounding | G-TEST |
| daemon starts in a real user session | G-DAEMON | G-DAEMON |
| GUI launches, **and the `AdwDialog`/`AdwAlertDialog` screens open** | **G-GUI** | G-GUI |
| discovery | G-MDNS **with the default firewall state** (G-FW) | G-MDNS, G-FW |
| pairing | G-PAIR, fingerprints compared on both screens | G-PAIR |
| battery | G-BATTERY, or recorded as not meaningfully testable in a VM | same |
| files | G-FILES, both directions, into the localized XDG dir | G-FILES |
| clipboard | G-CLIP, G-CLIP-WATCH, **G-CLIP-SENS (honest refusal)** | same |
| notifications | G-NOTIF, G-NOTIF-CAP, G-LOCK | same |
| reconnect | G-RECONNECT | G-RECONNECT |
| daemon restart | G-RESTART, G-UNIT | same |

**Two rules the certification must hold to, both learned from the notifications waves:**

1. **No "works because Fedora works".** Every row is evidence gathered on that
   distribution, in that wave, and is labelled with the distribution that produced it. Where
   a gate cannot be driven in the environment, it is recorded as **not done** with the
   reason — never inherited and never implied.
2. **A container is not a session.** G-BUILD may be satisfied by CI. Nothing else may.

---

## 27. Remaining risks

| # | Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- | --- |
| R1 | **Ubuntu 24.04's libadwaita 1.5.0 floor is lost by an unrelated change.** The development host has 1.9.3; nothing today would notice | **High** without L3 | An LTS supported to 2029 silently drops out | **L3's 24.04 CI row** — the single highest-value item in this plan |
| R2 | **The MSRV drifts up again** on the next `cargo update` | High without L3 | The §20.3 defect recurs, and is again found by a distro user rather than by CI | **L1 + L3's MSRV job** |
| R3 | **Ubuntu's session is Xorg, not Wayland** on some hardware, so `clipboard.v1` is unavailable | Medium | Clipboard silently absent on affected installs; everything else fine and the reason is reported honestly | G-SESSION records the session type; documentation states the Wayland requirement for clipboard |
| R4 | **Auto-send depends on Xwayland** being present for the XFIXES bridge | Low–Medium | Auto-send lost; manual send unaffected and the daemon says so | G-CLIP-WATCH; note the dependency in the docs |
| R5 | **U-1 turns into a support burden**: Ubuntu/Debian users report "password copy doesn't work" | Medium–High | Reputational, not technical | L2's proactive status line, plus an explicit line in the README |
| R6 | **mDNS collides with a running `avahi-daemon`** on Ubuntu/Debian | Low — Fedora already runs Avahi and has been certified six times | Discovery fails | G-MDNS on a real desktop install with its default daemons |
| R7 | **The bridged-VM setup is fiddly enough that certification is done over NAT** and quietly proves nothing about discovery | Medium | A green certification that never exercised mDNS or inbound TCP | §21.1 states the requirement; G-MDNS and G-PAIR cannot pass over NAT, so the matrix enforces it |
| R8 | GNOME versions differ across targets (46.0 / 48.7 / 50.1 vs the certified 50.4) | Low | A capability difference the sink already reads at runtime | G-NOTIF-CAP records `GetCapabilities` per target |
| R9 | `battery.v1` cannot be meaningfully exercised in a VM | Certain | One capability's hardware evidence is weaker on the new targets | Record it as not testable rather than faking it; or certify it once on real Ubuntu/Debian hardware later |
| R10 | The libadwaita/GTK floors are correct but **untested against the 1.5.0 runtime** — compiling against 1.5.0 headers is not the same as *running* on 1.5.0 | Medium | A runtime symbol issue on 24.04 only | **G-GUI specifically opens the two `Adw*Dialog` screens**, which are the 1.5 APIs |

---

## 28. Git status

Nothing was added, committed, pushed or opened as a PR. `HEAD` is still `7822a06`.

```console
$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U0.md

$ git diff --check
(clean)

$ git diff --stat
(empty)

$ git diff --name-status
(empty)

$ git log --oneline -1
7822a06 Merge pull request #23 …          ← HEAD unmoved; nothing staged
```

**There is no production code diff**, which is the expected outcome of a read-only audit.
The only change is this report.

**On the one probe that did touch a tracked file.** §14.2 required lowering
`desktop/gui/Cargo.toml`'s declared GTK and libadwaita features and recompiling, because
that is the only way to learn which API sets the floor rather than guessing from a type
inventory. The manifest was copied first, restored unconditionally on every exit path, and
`git status` was checked immediately afterwards and was clean. **No production change is
left behind.**

**Artefact audit** — `*.apk *.deb *.rpm *.key *.pem *.log`, container images, compile
output, `target/` directories: **none present in the diff.** Every working file used for
the probes — the probe scripts, container output, the `cargo metadata` dump and the
manifest backup — lives in the session scratchpad, outside the repository. No secret key
material was read or printed.

### Machine state left behind

* **Host:** four container images pulled (`ubuntu:24.04`, `ubuntu:26.04`, `debian:trixie`,
  `debian:bookworm`); `podman rmi` removes them. No container is still running — every
  probe used `--rm`. No package was installed on the host. `desktop/target/` was written
  to by the §14.2 `cargo check`, as any build does.
* **Repository:** clean apart from this report.

---

## 29. Final verdict

The audit found **no blocker**. Every claim in the summary below is backed by code read in
this wave or by a probe run in it, and where evidence is inherited or inferred it says so.

* **Ubuntu 24.04 LTS, Ubuntu 26.04 LTS and Debian 13 trixie are viable V1 targets.** The
  daemon, the CLI and the GTK GUI all compile on Ubuntu 24.04 and Debian 13 against those
  distributions' **own** GTK and libadwaita (§20.4), including the zero-margin
  libadwaita 1.5.0 case.
* **The architecture needed nothing.** Two external binaries, four D-Bus names all
  `org.freedesktop.*`, no OpenSSL, no `protoc`, no `libdbus`, no `libX11`, no Avahi, no
  systemd coupling, and no C library at all for the daemon and CLI. Wave 0's seams are why.
* **The work is small and it is one branch**: a truthful MSRV, a status line, a CI matrix,
  one error string, and a documentation pass.
* **Two things are genuinely wrong today and both were found by measuring, not by
  reading.** The declared MSRV is false and excludes Debian Stable's stock toolchain
  (U-4, §20.3). And `wl-copy --sensitive` is absent on all three targets, so sensitive
  clips are refused there and accepted on Fedora (U-1, §8).
* **What is not proven is stated as not proven.** Nothing in this wave ran on a real
  Ubuntu or Debian desktop session. Every runtime claim — discovery, pairing, mirroring,
  lock detection, clipboard, reconnect — is marked *LIKELY WORKS — NEEDS REAL TEST* in §22
  and has a named gate in §21.3. A container is not a session, and this report never treats
  one as though it were.

### Next branch and scope

**Branch: `feature/linux-debian-ubuntu-compat-v1`**

| | Scope |
| --- | --- |
| **In** | **L1** truthful MSRV (`rust-version = "1.88"`, spec, README) · **L2** proactive surfacing of the sensitive-clip gap · **L3** Linux CI matrix + MSRV job, with the Ubuntu 24.04 / libadwaita 1.5.0 row as its point · **L4** distro-neutral wl-clipboard remediation string · **L5** de-Fedora the README and architecture docs |
| **Then** | The §21 VM certification of **Ubuntu 24.04 LTS** and **Debian 13 trixie** against the §21.3 gates, on bridged networking (§21.1.1), accepted per §26 |
| **Out** | KDE (§23) · RPM/DEB packaging, installers, `.desktop`/AppStream files, firewall recipes · protocol or Android changes · lowering the libadwaita floor for Debian 12 / Ubuntu 22.04 |

**One thing to settle before the branch opens**, because it is a product decision rather
than an engineering one: **U-1**. On Ubuntu and Debian, AnyFlow will refuse every clip a
password manager copies until those distributions ship wl-clipboard 2.3.0. L2 makes that
honest and visible; it does not make it work. Whether that is acceptable for V1, or whether
it justifies a native Wayland clipboard writer in a later wave, is not U0's call.

---

## Appendix A — what U0 ran, at a glance

Every probe was disposable (`podman run --rm`) or reverted. Nothing was installed on the
host.

| # | Probe | Command (abridged) | What it settled |
| --- | --- | --- | --- |
| P1 | Fedora-assumption sweep | `grep -rniE "fedora\|dnf\|rpm\|selinux\|firewalld\|polkit\|apparmor\|/usr/lib64…" --include='*.rs' desktop/*/src desktop/capabilities/*/src` | 12 hits, 11 prose, **1 product surface** (§4) |
| P2 | External-process inventory | `grep -rnE "Command::new\|process::Command" …` + a multiline sweep of string literals for package-manager hints | **exactly two binaries**, `wl-copy` and `wl-paste` (§7) |
| P3 | D-Bus name inventory | `grep -rnoE '"(org\|/org)[A-Za-z0-9_./]*"' …` | four names, **all `org.freedesktop.*`**, zero `org.gnome.*` (§9) |
| P4 | OpenSSL / native-dependency audit | `cargo tree --workspace --locked -e normal,build`, then per binary | **no OpenSSL**; daemon and CLI link **no C library**; GTK stack is the GUI's alone (§6) |
| P5 | GTK/Adw floor extraction | read `[package.metadata.system-deps]` from `gtk4-sys 0.9.6` and `libadwaita-sys 0.7.2`; read the type-level `#[cfg]` gates from `src/auto/mod.rs` | declared floors are hard `pkg-config` gates: GTK 4.12, libadwaita 1.5 (§14.1) |
| P6 | **GUI floor compile probe** | lower `v4_12`→`v4_10` and `v1_5`→`v1_4` in `gui/Cargo.toml`, `cargo check -p anyflow-gui`, **restore unconditionally** | both floors are **real**; named the three exact APIs; **corrected the prior research** (§14.2, U-7) |
| P7 | Archive-version probe | `podman run … apt-cache policy …` then `pkg-config --modversion` after a real install, on 4 images | the version matrix in §14.3 |
| P8 | Runtime-binary probe | `wl-copy --version`, `wl-copy --help \| grep -- --sensitive`, tool paths, `/usr/bin/true`, `/usr/bin/echo` | **`--sensitive` is absent on all three V1 targets** (§8, U-1) |
| P9 | Ubuntu 24.04 toolchain probe | `apt-cache search '^rustc-1\.'` | `rustc-1.74` … `rustc-1.91` exist in the archive (§18) |
| P10 | **Full compile probe, stock Debian toolchain** | `podman run debian:trixie` → distro `rustc` 1.85.1 + distro GTK → `cargo check` | **failed: the lockfile needs 1.88** (§20.3, U-4) |
| P11 | MSRV extraction | `cargo metadata --format-version 1 --locked` → highest `rust_version` over all locked packages | declared **1.82**, real **1.88**, 34 crates above 1.82 (§20.3) |
| P12 | **Compile probe, adequate toolchains** | Ubuntu 24.04 + archive `rustc-1.91` + GTK 4.14.5/adw **1.5.0**; Debian 13 + rustup stable + GTK 4.18.6/adw 1.7.6 | §20.4 |
| P13 | Host virtualisation survey | `virsh net-dumpxml default`, `ip -br link` | libvirt default is NAT; **the host's only live link is Wi-Fi** (§21.1.1) |
| P14 | CI convention check | read `.github/workflows/portable-windows-msvc.yml` | one workflow, Windows only; its trigger/concurrency discipline is what L3 should copy (§25) |

## Appendix B — what U0 deliberately did not do

* **No KDE work of any kind** (§23).
* **No RPM or DEB packaging, no installer, no `.desktop` file.**
* **No protocol change**, no `.proto` touched, no Android change.
* **No production code change** — the one manifest edit was a reverted probe (§28).
* **No VM was built and no VM automation was written.** §21 designs the matrix; running it
  is the next wave.
* **No re-certification of `notifications.v1`, `clipboard.v1`, `files.v1` or `battery.v1`
  on Fedora.** Their certifications stand; this wave asks only what changes on a different
  distribution.

---

---

# UBUNTU/DEBIAN COMPATIBILITY: IMPLEMENTATION READY
