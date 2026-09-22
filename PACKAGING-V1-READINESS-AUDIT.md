# OmniBridge — Linux Packaging v1 Readiness Audit

| Field | Value |
| --- | --- |
| **Branch** | `feature/linux-packaging-v1` |
| **Base commit** | `23ca7f6` (= `develop` = `origin/develop` at audit time) |
| **Date** | 2026-09-21 |
| **Type** | READ-ONLY audit and implementation plan. No packaging, source, workflow, documentation or test file was modified. |
| **Audit host** | Fedora 44 Workstation, systemd 259, rustc 1.98.1, firewalld active |
| **Hardware tests** | **None run.** The SM-X620 was not touched. No VM was started. |
| **Scope** | UI Polish v1 closeout; packaging inventory; verification of P1–P10; RPM audit; DEB design; build/release strategy; lifecycle matrix; security review; documentation debts; phased plan. |

Every claim below is labelled:

* **MEASURED** — a command was run during this audit and its output is quoted.
* **SOURCE-VERIFIED** — read directly out of the tree at `23ca7f6`.
* **CITED** — taken from an existing certification report or research document, not re-measured here.
* **UNVERIFIED** — stated as a plan or an expectation, with no evidence yet.

Nothing in this document is presented as certification.

---

## 1. Executive summary

**The UI Polish v1 wave is closed.** All ten Android items and all six desktop
items named in the brief are present in the merged history. The baseline for
Packaging is **PR #46 / merge `23ca7f6`, content commit `6307ea8`**.

**Packaging is not close to shipping, and the gap is larger than the ten
recorded debts suggest.** The audit found three defects that are not on the
P1–P10 list and that each break the build or the install outright:

1. **The RPM does not build.** `%build` runs `cargo build --release --locked`
   over the whole workspace, which includes `omnibridge-gui`. `BuildRequires`
   declares only `rust`, `cargo`, `gcc` — no `gtk4-devel`, no
   `libadwaita-devel`, no `glib2-devel` (for `glib-compile-resources`, which
   `desktop/gui/build.rs` runs). The build fails before it installs anything.
2. **`%{_userunitdir}` is undefined without `BuildRequires:
   systemd-rpm-macros`,** which the spec does not declare. MEASURED on this
   host: `rpm --eval '%{_userunitdir}'` returns the literal string
   `%{_userunitdir}`. `%install` would create a directory of that literal
   name and `%files` would fail.
3. **`cargo build --locked` fetches from crates.io at build time.** Both
   `mock`/`koji` and Debian `buildd` are network-isolated. No vendored source
   archive exists and `desktop/` has no `.cargo/config.toml`. No official
   package can be built from the spec as written.

**The systemd defect is worse than recorded, and was measured rather than
inferred.** P2 was known as "no `RuntimeDirectory=`". It is that, plus a
second independent failure in the same unit, and the first was proved on this
host:

* With `ProtectSystem=strict` and no `RuntimeDirectory=`, a **user** unit
  **cannot** create `$XDG_RUNTIME_DIR/omnibridge` — MEASURED, §4.2. The
  daemon therefore cannot bind its control socket, and the CLI and GUI have
  nothing to talk to.
* `ReadWritePaths=%h/.local/share/omnibridge` with no `-` prefix **refuses to
  start the unit when the path does not exist** — MEASURED, §4.2 — which is
  exactly the state of a machine that has just installed the package.
* `StateDirectory=omnibridge` creates `~/.local/state/omnibridge`, which the
  daemon never opens. Its data directory is `$XDG_DATA_HOME/omnibridge`.

So the packaged unit fails on a fresh install for two reasons, and creates one
directory nobody uses. The fix for the first was also measured working (§4.2).

**Debian/Ubuntu packaging does not exist and never has** — MEASURED:
`git log --all --diff-filter=A -- 'packaging/debian/*' 'debian/*'` is empty.

**One finding contradicts the research documentation and is good news.**
Fedora Workstation's default firewalld zone opens `1025-65535/tcp` **and**
`1025-65535/udp` — MEASURED. TCP 55432 and mDNS on UDP 5353 are already
reachable there. `docs/research/platform-expansion/07-LINUX-PACKAGING.md §7`
states Fedora blocks 55432; that is **wrong for Fedora Workstation** and right
only for the `public` and `FedoraServer` zones. The firewall work is smaller
and more targeted than planned.

**One requirement cannot be met by packaging alone.** P4 asks that D-Bus
activation be usable immediately after install into a live session. The
documented discharge is `org.freedesktop.DBus.ReloadConfig` on the **user
session bus**. An RPM `%post` and a `.deb` `postinst` both run as **root**,
which has no route to a user's session bus, and MEASURED: no package on this
host ships a file trigger for `/usr/share/dbus-1/services` — the mechanism
that exists for `/usr/share/applications` and `/usr/share/icons/hicolor` has
no session-bus equivalent. Discharging P4 requires a small change in
`omnibridged`, which is product code. §8 sets out the design; **§18 flags it
as the one scope decision needing sign-off before Phase 3 starts.**

**Verdict: READY FOR IMPLEMENTATION** (§20), with that one scope decision and
five smaller decisions listed with recommended answers.

---

## 2. UI Polish v1 — merged baseline, confirmed

MEASURED from `git log` / `git show` on `23ca7f6`.

| Field | Value |
| --- | --- |
| **Merge commit** | `23ca7f65a88c1ffd39a39445fd8e716d973c7630` |
| **PR** | #46, `feature/omnibridge-ui-polish-v1` → `develop` |
| **Content commit** | `6307ea8b62ab6641ccaf3c1ee95eb884a339ba2a` |
| **Message** | `feat(ui): polish exchange flow on mobile and desktop` |
| **Author / date** | Yuri Converso Sismotto, Mon 21 Sep 2026 23:28:31 −0300 |
| **Size** | 75 files changed, 3945 insertions(+), 850 deletions(−) |

`feature/linux-packaging-v1` is at that same commit, so the Packaging branch
starts from the approved UI baseline with nothing in between.

### 2.1 Android items — all ten present

| Brief item | Evidence in `6307ea8` |
| --- | --- |
| OmniBridge palette alignment | `ui/theme/Color.kt` (+95/−…), `res/values/colors.xml`, `ui/theme/Gradients.kt`, `ui/theme/Status.kt` |
| Cross-platform token cleanup | `docs/design/tokens.json` rewritten (120 lines changed); read by both `DesignTokensTest.kt` and `desktop/gui/src/theme.rs` |
| Send Clipboard exchange-flow redesign | `ui/SendClipboardScreen.kt` (252 lines changed) + new `SendClipboardUiTest.kt` (295) |
| Send Files exchange-flow visual language | `ui/SendActivity.kt` (430) + new `ui/components/Exchange.kt` (541) + new `ExchangeFlowTest.kt` (694) |
| Duplicate connection controls removed | `ui/DevicesScreen.kt`: the `Connection` section label, `ConnectionCard`, and its own `Connect`/`Disconnect` pair are deleted and replaced by a single `ConnectionNotice`. The diff comment states the defect it fixes: *"the selected device card already showed `Connected` and a `Disconnect`, and then a 'Connection' section underneath showed … its own `Connect` and `Disconnect`."* |
| Devices card decorative line removed | `ui/DevicesScreen.kt`: *"No decorative wave behind the card any more."*; `ui/components/Cards.kt`: *"The ribbon read as a stray tricolour underline floating above the heading."* |
| Files empty-state decorative line removed | `ui/FilesScreen.kt` + `ui/components/Cards.kt`: *"…be the connection ribbon, a thin tricolour stroke that read as a stray…"* |
| Responsive tablet content width | New `ui/theme/Dimens.kt` → `OmniBridgeLayout { contentMax = 640.dp; compactMax = 600.dp }`, applied via `ui/components/Surfaces.kt` |
| Light/dark polish | `ui/theme/Theme.kt`, `ui/theme/Color.kt` (light/dark accent triples), `ui/theme/Gradients.kt` |
| Accessibility improvements | New `DevicesPresentationTest.kt` (150), `ExchangeFlowTest.kt` (694), `SendClipboardUiTest.kt` (295), `DesignTokensTest.kt` (+170); 17 assertions referencing content descriptions, touch targets or contrast |

### 2.2 Desktop items — all six present

| Brief item | Evidence in `6307ea8` |
| --- | --- |
| OmniBridge palette alignment | `desktop/gui/src/theme.rs` (320 lines changed), `desktop/gui/data/style.css` (183) — `TEAL` → `CYAN`, `CTA_GRADIENT` redefined as the `on_light` accent triple |
| Quick Panel polish | `desktop/gui/src/panel/mod.rs` (50) — CSS class `af-quick-panel` → `ob-quick-panel`, header rebuilt |
| Clipboard user-facing language | `desktop/gui/src/views/clipboard.rs` (321) — backend jargon (`wl-clipboard`, `XFIXES on the Xwayland CLIPBOARD selection`) replaced with *"Clipboard changes can/cannot be detected on this computer"* |
| Settings gear consistency | `panel/mod.rs` + `widgets.rs`: one `widgets::SETTINGS_ICON` used by the Quick Panel header, the panel's button and the sidebar row, with a test `the_settings_action_uses_one_icon_everywhere` |
| Light/dark polish | `theme.rs` — separate `on_light` / `on_dark` triples, `brand::DARK` added |
| Accessibility improvements | `theme.rs` contrast assertions: *"All ≥ 4.5:1 on white and Surface"*, *"All ≥ 6.2:1 on Dark and elevated"*, CTA gradient sampled at every interpolation step |

The full visual certification was **not** re-run. History alone was used, as
instructed.

**UI Polish v1 is closed. `23ca7f6` is the final UI baseline for Packaging v1.**

---

## 3. Current packaging inventory

SOURCE-VERIFIED. Every path below was listed, not assumed.

### 3.1 `packaging/` — three files, Fedora only

```
packaging/fedora/omnibridge.spec        73 lines
packaging/fedora/omnibridged.service    39 lines
packaging/fedora/README.md              41 lines
```

`git ls-files packaging` and `find packaging -type f` agree: there is nothing
else. No `packaging/common/`, no `packaging/debian/`, no firewalld XML, no
AppStream metainfo, no man pages, no preset file.

### 3.2 What the spec installs today

| Path | Source |
| --- | --- |
| `%{_bindir}/omnibridged` | `desktop/target/release/omnibridged` |
| `%{_bindir}/omnibridge` | `desktop/target/release/omnibridge` |
| `%{_userunitdir}/omnibridged.service` | `packaging/fedora/omnibridged.service` |
| `%license LICENSE` | repo root |
| `%doc README.md docs/` | repo root — **105 files, 1.6 MB**, including every research and ADR document |

`omnibridge-gui` is built by `%build` (it is a workspace member) and then
**not installed**. No `.desktop`, no icon, no D-Bus service file, no metainfo,
no firewall metadata, no man pages.

### 3.3 Desktop assets that exist and are packaging-ready

| Path | State |
| --- | --- |
| `desktop/gui/data/io.github.yurisismotto.omnibridge.desktop` | Complete. `DBusActivatable=true`, `StartupWMClass=omnibridge-gui`, `Actions=quick-panel`, `Icon=io.github.yurisismotto.omnibridge`. Installed **verbatim** by design. |
| `desktop/gui/data/io.github.yurisismotto.omnibridge.service.in` | D-Bus activation template. `Exec=@BINDIR@/omnibridge-gui --gapplication-service`. |
| `desktop/gui/data/omnibridge.gresource.xml` | Compiled into the binary by `build.rs`. Not an installed file. |
| `desktop/gui/data/style.css` | Compiled in. Not an installed file. |
| `docs/design/assets/omnibridge-app-icon.svg` | **The canonical app icon.** `build.rs` copies it to `icons/scalable/apps/<APP_ID>.svg` in `OUT_DIR`; `install-desktop-metadata.sh` installs the same file to `hicolor/scalable/apps/<APP_ID>.svg`. No redraw anywhere — `desktop/gui/tests/brand_assets.rs` enforces it. |
| `desktop/gui/tools/install-desktop-metadata.sh` | **The installer packaging should reuse.** Already supports `--prefix /usr --destdir "$RPM_BUILD_ROOT"`, substitutes `@BINDIR@` from the prefix (never from the checkout), validates the desktop entry, and **skips** live-session cache refresh in DESTDIR mode. |

### 3.4 Identity constants — one string, enforced by tests

SOURCE-VERIFIED. `APP_ID = "io.github.yurisismotto.omnibridge"` (lowercase) in:

* `desktop/gui/src/lib.rs:71` (GApplication id, default window icon name)
* `desktop/gui/build.rs:16` (icon-theme filename)
* `desktop/platform-linux/src/tray/model.rs:27,47,56` — `DESKTOP_APP_ID`,
  `ICON_NAME`, `ITEM_ID`
* `desktop/gui/tools/install-desktop-metadata.sh:57`
* the `.desktop` basename, its `Icon=`, the `.service.in` basename and `Name=`

> **Correction to the research docs.**
> `docs/research/platform-expansion/07-LINUX-PACKAGING.md §6` and
> `05-DEBIAN-UBUNTU-COMPATIBILITY.md §7` both write
> `io.github.yurisismotto.OmniBridge` with a **capital O and B**, and
> `07 §6` names artwork files (`app-icon.svg`, `logo-flowing-a.svg`,
> `icon-flowing-ribbon.svg`) that **no longer exist**. Those documents predate
> the rebrand. **Packaging must follow the code and the tests, not the
> research prose.** Getting this wrong produces a grey fallback square on
> every desktop.

### 3.5 Runtime paths the package must respect and must not own

SOURCE-VERIFIED.

| Path | Created by | Mode | Contents |
| --- | --- | --- | --- |
| `$XDG_DATA_HOME/omnibridge` → `~/.local/share/omnibridge` | `omnibridged`, first run (`core/src/platform/unix_fs.rs:214`) | `0700` | `identity.key` `0600`, `state.json` `0600` — MEASURED on this host |
| `$XDG_RUNTIME_DIR/omnibridge/control.sock` | `omnibridged` at bind (`platform-linux/src/lib.rs:195-223`) | dir `0700`, socket `0600` | control socket; fallback `/tmp/omnibridge-<uid>` when `XDG_RUNTIME_DIR` is unset |
| `<XDG downloads>/OmniBridge` | `omnibridged` on first accepted file | — | received files |

`state.json` carries `schema_version`, the local identity, **trusted peers**
and **per-capability grants** (`core/src/store.rs:1`). This is the trust store.
**No package may own, create, move or remove any of these.**

### 3.6 Network surface

SOURCE-VERIFIED.

| Fact | Value | Source |
| --- | --- | --- |
| Listen port | TCP **55432** (`DEFAULT_PORT`), overridable with `--port` | `desktop/core/src/lib.rs:39` |
| Bind | `::` then `0.0.0.0` (dual-stack, IPv6 first with v4 fallback) | `desktop/runtime/src/listener.rs:74-114` |
| Discovery | mDNS, service type `_omnibridge._tcp.local.`, UDP **5353** multicast, own responder (`mdns-sd`, not Avahi) | `desktop/core/src/lib.rs:59`, `desktop/runtime/src/mdns.rs` |
| Opt-out | `omnibridged --no-mdns` | `desktop/daemon/src/main.rs:37` |
| Control | Unix socket only, never TCP | `desktop/platform-linux/src/lib.rs:87` |

### 3.7 Build inputs

SOURCE-VERIFIED.

| Fact | Value |
| --- | --- |
| Workspace version | `0.1.0` (`[workspace.package]`, `desktop/Cargo.toml:19`) |
| MSRV | **`rust-version = "1.88"`** (`desktop/Cargo.toml:44`), with a 40-line comment naming `time 0.3.55`, `rcgen 0.14.10`, `zbus 5.19.0` as the crates that set it |
| Lockfile | `desktop/Cargo.lock`, version 4, **282 packages**, committed |
| Vendoring | **none** — no `vendor/`, no `.cargo/config.toml` anywhere (MEASURED) |
| Git tags | **none** (MEASURED: `git tag -l` empty) |
| Native build deps | `gcc`, `libc6-dev`/`glibc-devel`, `pkg-config`; GUI only: `libgtk-4-dev` ≥ 4.12, `libadwaita-1-dev` ≥ 1.5, and `glib-compile-resources` |
| Explicitly **not** needed | `protobuf-compiler`, `libssl-dev`, `libdbus-1-dev`, `libx11-dev`, Avahi |

### 3.8 CI as it stands

SOURCE-VERIFIED. Four workflows, **none of which produces an artifact**.

| Workflow | What it does |
| --- | --- |
| `desktop-quality.yml` | `cargo fmt --check` + `cargo clippy --workspace --all-targets --all-features -D warnings` on stable |
| `linux-distro-compat.yml` | `msrv` job pins rustc 1.88 and asserts it equals the declared `rust-version`; `distro` matrix runs `cargo check` (daemon, CLI, **GUI**) + portable tests in `ubuntu:24.04`, `ubuntu:26.04`, `debian:trixie` containers, against each distribution's own GTK stack |
| `android-ci.yml` | Android build + unit tests |
| `portable-windows-msvc.yml` | portable-core boundary on Windows |

Two things this matters for:

* There is **no Fedora row**, so the primary target is the only one not built in CI.
* The distro rows install a **rustup 1.88**, not the distribution's `rustc`.
  So CI proves *"builds on this distro's GTK with the declared MSRV"* and
  **not** *"builds with this distro's own toolchain"* — which is the question
  a `.deb`'s `Build-Depends` has to answer. See §6.2.

---
## 4. Verified open defects

Each of P1–P10 was re-checked against the tree at `23ca7f6`. None was assumed
still open on the strength of a prior report.

| ID | Status | One-line finding |
| --- | --- | --- |
| **P1** | **OPEN** | Spec declares `rust >= 1.82`; workspace MSRV is `1.88`. |
| **P2** | **OPEN, worse than recorded** | No `RuntimeDirectory=`; **plus** `ReadWritePaths=` at a non-existent path refuses to start; **plus** `StateDirectory=` points somewhere unused. |
| **P3** | **OPEN** | Unit is shipped but never enabled, and no lifecycle macro is called at all. |
| **P4** | **OPEN, and not dischargeable by packaging alone** | No root-side mechanism exists for the session bus. |
| **P5** | **Mechanism EXISTS and is correct; not wired into the spec** | `install-desktop-metadata.sh` already substitutes `@BINDIR@` from the prefix. The spec never calls it. |
| **P6** | **OPEN, but smaller than documented** | Fedora Workstation already opens 55432 and 5353. Only `public`/`FedoraServer` zones need the service file. |
| **P7** | **OPEN** | The GUI is built and then discarded. No `.desktop`, icon, D-Bus service or metainfo is installed. |
| **P8** | **OPEN (documentation only)** | Nothing currently installs or recommends a GNOME extension — which is correct. The debt is that nothing *documents* it in the packaging path either. |
| **P9** | **OPEN by omission** | Nothing in the spec touches user state, which is the right default; but `remove` vs `purge` is undefined because no DEB exists, and upgrade does not restart the running daemon. |
| **P10** | **OPEN** | No manifest exists; §12 proposes one. |

Three further defects were found that are **not** on the P-list and are each
build- or install-fatal. They are **B1–B3** and appear in §5.

### 4.1 P1 — Rust BuildRequires

SOURCE-VERIFIED, both halves:

```
desktop/Cargo.toml:44           rust-version = "1.88"
packaging/fedora/omnibridge.spec:12   BuildRequires:  rust >= 1.82
```

The 1.82 claim is exactly the one `desktop/Cargo.toml`'s own comment records
as having been false: *"1.82 used to stand here and was not true of the tree
as committed."* `.github/workflows/linux-distro-compat.yml` has a gate that
fails if `rust-version` drifts from the pinned MSRV — but nothing checks the
spec, so the spec drifted instead.

**Rule for the fix:** the packaging Rust floor must be *derived from* the
workspace MSRV, never restated. Recommended: `BuildRequires: rust >= 1.88`
with a CI assertion that the number in the spec equals
`sed -n 's/^rust-version *= *"\(.*\)"/\1/p' desktop/Cargo.toml`, mirroring the
guard the MSRV job already applies to itself.

### 4.2 P2 — Runtime directory (MEASURED)

The current unit (`packaging/fedora/omnibridged.service`) contains:

```ini
ProtectSystem=strict
ProtectHome=read-only
StateDirectory=omnibridge
ReadWritePaths=%h/.local/share/omnibridge
```

and **no** `RuntimeDirectory=`. The daemon's socket path is
`$XDG_RUNTIME_DIR/omnibridge/control.sock`, and it creates that directory
itself (`platform-linux/src/lib.rs:195-198`).

`systemd.exec(5)` on this host documents a general caveat that namespacing may
not apply to user units without `PrivateUsers=true`. That caveat would have
made the whole finding moot, so it was **measured rather than believed**.

**Probe A — a user unit with no sandboxing:**

```console
$ systemd-run --user --wait --collect -P --unit=ob-a1 \
    /bin/sh -c 'mkdir -p "$XDG_RUNTIME_DIR/ob-probe" && echo CREATED || echo REFUSED'
$ ls -ld /run/user/1000/ob-probe
drwxr-xr-x. 2 yuri yuri 40 set 21 23:45 /run/user/1000/ob-probe
```

**Probe B — the same thing with the unit's own directives:**

```console
$ systemd-run --user --wait --collect -P --unit=ob-b1 \
    -p ProtectSystem=strict -p ProtectHome=read-only \
    /bin/sh -c 'mkdir -p "$XDG_RUNTIME_DIR/ob-probe2" && echo CREATED || echo REFUSED'
$ ls -ld /run/user/1000/ob-probe2
ls: cannot access '/run/user/1000/ob-probe2': No such file or directory
```

**`ProtectSystem=strict` IS effective in a systemd user unit on Fedora 44 /
systemd 259.** The packaged daemon cannot create its runtime directory. Every
downstream surface — `omnibridge status`, the GUI, the tray — has nothing to
connect to.

**Probe C — the second, independent failure:**

```console
$ systemd-run --user --wait --collect --unit=ob-c1 \
    -p ProtectSystem=strict -p ProtectHome=read-only \
    -p ReadWritePaths=%h/.local/share/omnibridge-does-not-exist /bin/true
Failed to start transient service unit: Invalid ReadWritePaths
```

`systemd.exec(5)`: *"Paths in `ReadWritePaths=` … may be prefixed with `-`, in
which case they will be ignored when they do not exist."* The unit does not
use the prefix, so on a machine where `~/.local/share/omnibridge` has never
been created — i.e. every fresh install — the unit refuses to start. This is
a **second** defect in the same four lines and it was not previously recorded.

**Probe D — the fix, verified:**

```console
$ systemd-run --user --wait --collect --unit=ob-d1 \
    -p ProtectSystem=strict -p ProtectHome=read-only \
    -p RuntimeDirectory=ob-probe3 -p RuntimeDirectoryMode=0700 \
    /bin/sh -c 'touch "$XDG_RUNTIME_DIR/ob-probe3/sock" && echo SOCKET-WRITABLE || echo SOCKET-REFUSED'
SOCKET-WRITABLE

$ # …and the mode systemd actually applies:
$ systemd-run --user --wait --collect --unit=ob-d2 \
    -p ProtectSystem=strict -p RuntimeDirectory=ob-probe4 -p RuntimeDirectoryMode=0700 \
    /bin/sh -c 'stat -c "%a %n" "$XDG_RUNTIME_DIR/ob-probe4"'
700 /run/user/1000/ob-probe4
```

`RuntimeDirectory=omnibridge` + `RuntimeDirectoryMode=0700` under
`ProtectSystem=strict` works, and yields exactly the 0700 the daemon would
have set itself. **`ProtectSystem=strict` is not weakened to obtain this.**

> All four probes were transient units. All probe directories and units were
> removed; `ls -d "$XDG_RUNTIME_DIR"/ob-probe*` and
> `systemctl --user list-units 'ob-*' --all` are both empty. Verified.

**Third finding in the same block.** `systemd.exec(5)` on this host, the
directory table:

```
│ RuntimeDirectory=  │ /run/     │ $XDG_RUNTIME_DIR │ $RUNTIME_DIRECTORY │
│ StateDirectory=    │ /var/lib/ │ $XDG_STATE_HOME  │ $STATE_DIRECTORY   │
```

So `StateDirectory=omnibridge` in a **user** unit creates
`~/.local/state/omnibridge`. The daemon's data directory is
`$XDG_DATA_HOME/omnibridge` = `~/.local/share/omnibridge`. The directive
creates an empty directory nobody reads and does nothing for the one that
matters. See §7 for the full replacement.

### 4.3 P3 — systemd user autostart

SOURCE-VERIFIED. The unit is correct in kind:

* `packaging/fedora/omnibridged.service` has `[Install] WantedBy=default.target`;
* it is installed to `%{_userunitdir}` — the **user** unit directory;
* `packaging/fedora/README.md` states plainly that it must stay a user unit,
  and why.

**No conversion to a system service is proposed anywhere in this document.**

What is missing:

1. **No lifecycle handling at all.** The spec calls no `%systemd_user_post`,
   `%systemd_user_preun` or `%systemd_user_postun`. It prints a `%post`
   banner telling the user to run
   `systemctl --user enable --now omnibridged.service`.
2. **The macros are not even available.** MEASURED:
   `rpm -q systemd-rpm-macros` → not installed;
   `rpm --eval '%{_userunitdir}'` → `%{_userunitdir}` (unexpanded).
   `BuildRequires: systemd-rpm-macros` is mandatory and absent. This is
   defect **B2** in §5.
3. **An upgrade does not restart a running daemon.** A root scriptlet cannot
   reach a user's service manager, so after `dnf upgrade` the old
   `omnibridged` keeps running from the replaced binary until the user
   restarts it or logs out. This is inherent to user units and must be
   documented, not worked around.

**What is appropriate, and the recommendation.** `%systemd_user_post` runs
`systemctl --global preset`, which consults
`/usr/lib/systemd/user-preset/`. MEASURED: Fedora 44 ships
`90-default-user.preset` and `99-default-disable.preset`; with no line naming
`omnibridged.service`, the preset leaves it **disabled**. That is the correct
outcome and calling the macro is still right — it is what makes `%preun`
disable cleanly and keeps the unit directory consistent.

**Recommendation: ship the unit disabled on both RPM and DEB.** Reasons, in
order of weight:

1. A global enable (`systemctl --global enable`) turns on a LAN listener for
   **every** account on the machine, including service accounts that will
   never pair anything.
2. The daemon does nothing until an identity exists and a device is paired, so
   autostart-before-pairing buys the user nothing.
3. It is a one-line, reversible decision the user should make; `%post` and the
   README both say the line.

For Debian the symmetric mechanism is `dh_installsystemduser --no-enable`.
Revisit after the GUI grows a "Start OmniBridge at login" toggle — that is the
right place for this choice to live, not in a package scriptlet.

### 4.4 P4 — D-Bus activation into a live session

SOURCE-VERIFIED and CITED.

The template is at
`desktop/gui/data/io.github.yurisismotto.omnibridge.service.in`, and it is
well-formed:

```ini
[D-BUS Service]
Name=io.github.yurisismotto.omnibridge
Exec=@BINDIR@/omnibridge-gui --gapplication-service
```

| Question from the brief | Answer |
| --- | --- |
| Destination path | `$prefix/share/dbus-1/services/io.github.yurisismotto.omnibridge.service` — `/usr/share/dbus-1/services/…` under a package prefix (`install-desktop-metadata.sh:98,102`) |
| Generated `Exec=` | `/usr/bin/omnibridge-gui --gapplication-service` |
| Install prefix | Passed in as `--prefix`; `@BINDIR@` becomes `$prefix/bin`, never `$destdir`, never a checkout path (`install-desktop-metadata.sh:173`) |
| Is `Exec=` absolute? | **Yes** — see P5 |
| Does install/upgrade reload the live session bus? | **Not from a package.** See below. |

CITED, `KDE-PLASMA-REAL-CERTIFICATION-V1.md §40`: a service file installed
into an **already running** session is invisible to that session's bus until
`org.freedesktop.DBus.ReloadConfig` is called or the user logs out; and in a
**freshly booted** session the bus picked the file up on its own
(`busctl --user list --activatable` listed the name). So the requirement is
specific to install/upgrade into a live session — exactly when a package
manager runs.

MEASURED on this host, the mechanism works and is in use today:

```console
$ busctl --user list --activatable | grep -i omnibridge
io.github.yurisismotto.omnibridge  …  (activatable)

$ cat ~/.local/share/dbus-1/services/io.github.yurisismotto.omnibridge.service
…
Exec=/home/yuri/.local/bin/omnibridge-gui --gapplication-service
```

**Why a package cannot discharge this.** `%post` and `postinst` run as
**root**. Root has no route to an arbitrary user's session bus, and inventing
one (enumerating `/run/user/*/bus` and connecting as root) would be a
privilege boundary violation, not a fix.

MEASURED — the analogous mechanisms that *do* exist, and the one that does not:

```console
$ rpm -q --filetriggers desktop-file-utils
transfiletriggerin  -- /usr/share/applications
update-desktop-database &> /dev/null || :

$ rpm -q --filetriggers gtk4
transfiletriggerin  -- /usr/share/icons/hicolor
gtk-update-icon-cache --force /usr/share/icons/hicolor &>/dev/null || :
```

No package on this host ships a file trigger for
`/usr/share/dbus-1/services`. The session bus has no root-side equivalent,
because it cannot have one.

**Two consequences, both useful.** First: the package needs **no scriptlets**
for the desktop database or the icon cache — the distribution's own triggers
handle both. Do not write them. Second: P4 needs a mechanism that runs as the
right user, in the right session. §8 proposes it.

### 4.5 P5 — Absolute D-Bus `Exec`

**Not a defect in the mechanism. A defect in the spec not using it.**

`install-desktop-metadata.sh:169-176` is explicit:

```bash
sed -e "s|@BINDIR@|$prefix/bin|g" -- "$DBUS_SRC" > "$dbus_dst.tmp"
install -m 0644 -- "$dbus_dst.tmp" "$dbus_dst"
```

`--prefix` is validated absolute (line 92-95), and `$destdir` is deliberately
excluded from the substitution because it is a staging root that will not
exist on the target machine. Running it as
`--prefix /usr --destdir %{buildroot}` yields
`Exec=/usr/bin/omnibridge-gui --gapplication-service`.

No development-tree path can reach the generated file. The requirement is
met the moment the spec calls the script.

### 4.6 P6 — Firewall and LAN discovery (MEASURED; contradicts the research doc)

Network requirements were read from source, not memory — see §3.6: **TCP
55432** inbound, **UDP 5353** multicast for mDNS.

MEASURED on this Fedora 44 Workstation host:

```console
$ firewall-cmd --get-default-zone
FedoraWorkstation
$ firewall-cmd --list-services
dhcpv6-client samba-client ssh
$ firewall-cmd --list-ports
1025-65535/tcp 1025-65535/udp
```

and the shipped zone definition:

```xml
<!-- /usr/lib/firewalld/zones/FedoraWorkstation.xml -->
<port protocol="udp" port="1025-65535"/>
<port protocol="tcp" port="1025-65535"/>
```

**On Fedora Workstation, TCP 55432 and UDP 5353 are already open.** No
firewall change is required there.

The other stock zones, MEASURED from `/usr/lib/firewalld/zones/`:

| Zone | TCP 55432 | UDP 5353 |
| --- | --- | --- |
| `FedoraWorkstation` (Workstation default) | **open** (1025-65535) | **open** (1025-65535) |
| `public` | closed | **open** — ships `<service name="mdns"/>`, and `/usr/lib/firewalld/services/mdns.xml` is `udp/5353` to `224.0.0.251`/`ff02::fb` |
| `FedoraServer` | closed | closed |

> **Correction to `docs/research/platform-expansion/07-LINUX-PACKAGING.md §7`.**
> That table says Fedora inbound 55432 is *"Blocked"*. It is **not blocked on
> Fedora Workstation**, which is the reference platform and the one all six
> certification waves ran on. The claim holds only for `public` and
> `FedoraServer`. The research document also wonders *"whether certification
> hid this (because the dev machine already had a rule)"* — the answer is no:
> the zone's own default port range is why it worked, and `--list-ports`
> above shows no OmniBridge-specific rule.
>
> CITED and consistent: `KDE-PLASMA-REAL-CERTIFICATION-V1.md §28` records the
> KDE **guest** being installed with
> `firewall --enabled --service=mdns --port=55432:tcp`, i.e. a non-Workstation
> profile that did need both opened explicitly.

**Plan — Fedora.** Ship `/usr/lib/firewalld/services/omnibridge.xml`
declaring **TCP 55432 only**. Do **not** redeclare mDNS: firewalld already
ships `mdns.xml`, correctly scoped to the two multicast destinations, and
duplicating it would be a second thing to keep right. Do **not** enable it —
no `firewall-cmd` in any scriptlet, ever. Tell the user the two commands in
`%post` and the README, and have `omnibridge status` report an unreachable
listener so the diagnosis reaches them where the problem shows up.

**Plan — Debian/Ubuntu.** Assume nothing. Debian 13 enables no firewall;
Ubuntu installs `ufw` **inactive** (CITED, `07 §7`; NOT RE-VERIFIED here —
gate L11 measures it). Ship **no** firewall metadata in the `.deb` and
document `sudo ufw allow 55432/tcp` and `sudo ufw allow mdns` for users who
have turned ufw on. A firewalld XML in a `.deb` would be dead weight on the
overwhelming majority of installs.

**Uninstall.** Package removal deletes `omnibridge.xml`. If the user had run
`firewall-cmd --permanent --add-service=omnibridge`, their permanent config
then names a service definition that no longer exists. Firewalld warns about
this. The package **must not** silently edit the user's firewall on removal
to tidy it up — that is the same violation as opening the port, in reverse.
Document the one-line cleanup. Open question **Q4**, §19.

### 4.7 P7 — GUI and desktop integration

SOURCE-VERIFIED. `%install` in the spec installs two binaries and one unit.
`omnibridge-gui` is compiled by `%build` and then thrown away.

| Required | Present in package? |
| --- | --- |
| `omnibridge-gui` binary | **No** |
| `.desktop` entry | **No** (the file exists in the tree) |
| hicolor scalable icon | **No** (the artwork exists and is canonical) |
| D-Bus activation service | **No** (the template exists) |
| GApplication `APP_ID` | Correct in code; irrelevant until the files ship |
| Application resources | Compiled into the binary via GResource — nothing to install |
| AppStream metainfo | **Does not exist anywhere** (MEASURED: no `metainfo`/`appstream` string in `packaging/` or `desktop/gui/`) |

**Icon ownership matters more than it looks.** `omnibridged` — not the GUI —
publishes the StatusNotifierItem, and its `ICON_NAME` is
`io.github.yurisismotto.omnibridge` (`platform-linux/src/tray/model.rs:47`).
A desktop shell resolves that name from the **hicolor** theme, not from the
GUI's compiled-in GResource. So if the icon ships only in `omnibridge-gui`,
an `omnibridge`-only install shows a broken tray icon on KDE. §12 puts the
icon in the **core** package for exactly this reason.

**The icon must derive from `docs/design/assets/omnibridge-app-icon.svg` with
no redraw.** Both existing consumers already do: `build.rs:25-29` copies it,
`install-desktop-metadata.sh:69` installs that same file verbatim, and
`desktop/gui/tests/brand_assets.rs` fails if the asset set changes. Packaging
must install the same file and add no seventh copy.

### 4.8 P8 — GNOME StatusNotifier host

SOURCE-VERIFIED and CITED. OmniBridge publishes a `StatusNotifierItem`
(`desktop/platform-linux/src/tray/`), which KDE Plasma hosts natively and
stock GNOME Shell does not host at all.

**Current state is already compliant.** Nothing in `packaging/` installs,
enables, recommends or depends on a GNOME Shell extension. MEASURED: the spec
has exactly one weak dependency, `Recommends: upower`.

The debt is documentation, and the wording already exists — CITED,
`GNOME-APPINDICATOR-V1.md §24`:

> **GNOME requires a StatusNotifier/AppIndicator shell extension to display
> the OmniBridge tray icon.** Stock GNOME Shell has no system tray, and
> OmniBridge cannot add one. OmniBridge runs completely normally without it —
> every feature works, and the tray icon is the only thing that is absent.

**Strategy for v1**, and the boundary is firm:

* **Never** `Requires:` or `Depends:` a shell extension.
* **Never** install or enable one from a scriptlet.
* **Do not** even use `Recommends:`/`Suggests:` for it. `Recommends` installs
  by default on both dnf and apt, which is the silent install the brief
  forbids. `Suggests:` is closer but still asserts a package name that differs
  between distributions and releases.
* **Do** put the paragraph above in the README's Linux packaging section and
  in the `%description` of the GUI subpackage, naming
  `gnome-shell-extension-appindicator` on both Fedora and Debian/Ubuntu as
  information, not as a dependency.
* **KDE Plasma must keep working with no extension.** Gate L9, §14.

### 4.9 P9 — Upgrade preservation

SOURCE-VERIFIED. What must survive, from §3.5: `~/.local/share/omnibridge/`
containing `identity.key` (0600) and `state.json` (0600) — the local identity,
the trusted-peer set and the per-capability grants.

**Current RPM semantics are accidentally correct.** The spec's `%files` claims
nothing under `%{_userhome}` or `$HOME`, and there is no `%postun` — so
`dnf remove` and `dnf upgrade` both leave user state untouched. That is the
right behaviour and the implementation must keep it deliberately rather than
by omission.

| Operation | RPM | DEB | Correct? |
| --- | --- | --- | --- |
| upgrade | files replaced; `~/.local/share/omnibridge` untouched | same, via dpkg | **Yes** |
| upgrade, running daemon | **not restarted** — a root scriptlet cannot reach a user's service manager | same | Inherent. **Document it**; the user restarts or logs out. |
| `dnf remove` / `apt remove` | binaries + unit + metadata removed; user state kept | same | **Yes** |
| `apt purge` | n/a | dpkg purges **`/etc` conffiles**, not `$HOME` | **Yes — and this is the point** |
| reinstall | user state still there; daemon finds its identity | same | **Yes** |

**The Debian `remove` vs `purge` distinction, stated precisely, because it is
the one place a mistake would destroy an identity.** `purge` removes
configuration files the package registered — conffiles under `/etc` — plus
whatever `postrm purge` deletes. It has **no** built-in reach into `$HOME`.
OmniBridge ships **no** conffiles and needs **none**.

**Therefore the rule for the DEB is: `postrm` must not exist, or if it exists
for another reason, its `purge` branch must do nothing.** A `postrm purge`
that removed `~/.local/share/omnibridge` would destroy the user's
cryptographic identity, invalidating every pairing on every peer and making
this machine look *revoked* to devices that were never told anything — CITED,
`19-PACKAGING-AND-DISTRIBUTION.md §5`, rule 1. If an identity reset is ever
wanted it belongs in an `omnibridge reset` command the user runs knowingly,
never in a package transaction.

Gates L13, L21, L23, L24 and L26 exist to prove this rather than assert it.

### 4.10 P10 — Package content ownership

No manifest exists today. §12 proposes one for RPM and DEB, with an explicit
"not package-owned" list.

---
## 5. Fedora RPM findings

Full audit of `packaging/fedora/omnibridge.spec` (73 lines). Nothing was fixed.

### 5.1 Build-fatal defects

**B1 — GUI build dependencies are missing; the RPM does not build.**
SOURCE-VERIFIED. `%build` is:

```spec
%build
cd desktop
cargo build --release --locked
```

`omnibridge-gui` is a workspace member (`desktop/Cargo.toml:11`), so this
builds it. `desktop/gui/Cargo.toml` requires `gtk4 0.9 (features v4_12)` and
`libadwaita 0.7 (features v1_5)`, both resolved by `system-deps` through
`pkg-config`, and `desktop/gui/build.rs` runs `glib-compile-resources`.
`BuildRequires` lists `rust`, `cargo`, `gcc` and nothing else — no
`pkgconf-pkg-config`, no `gtk4-devel`, no `libadwaita-devel`, no
`glib2-devel`. **The build fails in `desktop/gui`.** This has evidently never
been executed: no `rpmbuild` or `mock` is installed on the development host
(MEASURED).

**B2 — `%{_userunitdir}` is undefined.** MEASURED:

```console
$ rpm --eval '%{_userunitdir}'
%{_userunitdir}
$ rpm -q systemd-rpm-macros
package systemd-rpm-macros is not installed
```

The macro comes from `systemd-rpm-macros`, which the spec does not
`BuildRequires`. Unexpanded, `%install` creates a directory literally named
`%{_userunitdir}` and `%files` fails on the missing path.

**B3 — the build is not offline-capable.** MEASURED: no `vendor/` directory
and no `.cargo/config.toml` anywhere in the tree. `cargo build --locked`
resolves from the committed lockfile but still **downloads** 282 crates from
crates.io. `mock` disables networking during `%build` by default, and Debian
`buildd` has no network at all. **No official package can be produced from
this spec.** §13 sets out the vendored-source-archive fix.

### 5.2 The remaining spec findings

| Area | Finding |
| --- | --- |
| **Stale AnyFlow assumptions** | **None found.** The rebrand (`8c1f16f`, `edcb416`) reached `packaging/` completely: `Name: omnibridge`, `%{_bindir}/omnibridged`, `URL`, `%description`, unit filename and `ExecStart` are all OmniBridge. No `anyflow` string survives in `packaging/`. |
| **Missing GUI files** | §4.7 — binary, `.desktop`, icon, D-Bus service, metainfo: all absent. |
| **Incorrect BuildRequires** | `rust >= 1.82` (P1) plus B1 and B2. Correctly **absent**: `protobuf-compiler` — the comment at line 15-16 citing ADR-0004 is accurate and should be kept. |
| **Runtime dependencies** | Only `Recommends: upower`. Missing: `Suggests: wl-clipboard` (the daemon shells out to `wl-copy`/`wl-paste`, `capabilities/clipboard/src/backend/wayland.rs:48-49`, and degrades honestly without it — so `Suggests`, not `Requires`). GUI subpackage will need `Requires: omnibridge = %{version}-%{release}` and gets its GTK/libadwaita `Requires` automatically from `ldd`-based auto-dependencies. |
| **systemd user macro/lifecycle** | None called (P3). Needs `BuildRequires: systemd-rpm-macros` and `%systemd_user_post` / `%systemd_user_preun` / `%systemd_user_postun`. |
| **Firewall integration** | None (P6). |
| **Architecture assumptions** | `%global debug_package %{nil}` at line 1 suppresses `-debuginfo`/`-debugsource`. Otherwise the spec is arch-neutral and correct: no `%ifarch`, no `/usr/lib64` (which `LINUX-UBUNTU-DEBIAN-COMPAT-U0.md §4` also verified for source). `x86_64` and `aarch64` are both Rust Tier 1. |
| **Version/release source** | `Version: 0.1.0` is **hand-written** and duplicates `desktop/Cargo.toml:19`. Two sources of truth, no guard. MEASURED: **no git tags exist**, so there is no third. |
| **Source tarball assumptions** | `Source0: %{name}-%{version}.tar.gz` with `%autosetup`. **Nothing in the repository produces that tarball** — no release script, no CI job, no `make dist`. |
| **Reproducibility** | `--locked` is used (good). Against it: no vendored sources (B3), no `SOURCE_DATE_EPOCH`, no checksums, no SBOM, `%{dist}`-only `Release`. |
| **Package ownership** | Correct by omission: no user-state path is claimed. `%doc docs/` ships **105 files / 1.6 MB** including every research and ADR document — not a defect, but wrong for a runtime package. |
| **Upgrade/uninstall behaviour** | §4.9 — accidentally correct; no `%postun`; a running user daemon is not restarted on upgrade. |
| **`%check`** | `cargo test --release --locked` over the workspace. Display-requiring tests are already `#[ignore]`d (SOURCE-VERIFIED: `gui/src/views/{clipboard,notifications,mod}.rs`, `capabilities/clipboard/tests/real_backend.rs`, `capabilities/notifications/tests/real_{dbus,lock}.rs`, `capabilities/battery/tests/real_upower.rs`), so PKG-006 is **largely already discharged**. Two residual risks, UNVERIFIED: the daemon suite binds TCP and the runtime suite touches mDNS multicast, both inside a network-isolated mock chroot. Gate in Phase 1. |
| **`%changelog`** | Well-formed; `Sat Aug 29 2026` is genuinely a Saturday (MEASURED). Needs a v1 entry. |

### 5.3 What the spec gets right and must keep

* It is a **user** unit, and `packaging/fedora/README.md` argues the case
  correctly.
* It refuses `protobuf-compiler` and says why, with an ADR reference.
* `--locked` on both build and test.
* `Recommends:` rather than `Requires:` for `upower`, with the reason stated.
* `%check` runs the suite, so a package that fails its own security tests is
  not produced.

---

## 6. Debian / Ubuntu packaging gap

### 6.1 Current state

**None exists, and none ever has.** MEASURED:
`git log --all --oneline --diff-filter=A -- 'packaging/debian/*' 'debian/*'`
returns nothing.

### 6.2 The constraint that shapes the whole design

**Debian 13 trixie's stock `rustc` is 1.85.1. The MSRV is 1.88.** CITED
(`README.md`, `desktop/Cargo.toml:33-39`, `LINUX-UBUNTU-DEBIAN-COMPAT-U0.md
§20.3`), and `desktop/Cargo.toml`'s comment records it as *measured failing*.

| Target | Stock `rustc` | Meets 1.88? | Route |
| --- | --- | --- | --- |
| Ubuntu 24.04 LTS | 1.75 | No | `rustc-1.91` / `cargo-1.91` from Ubuntu's own archive |
| Ubuntu 26.04 LTS | 1.93.1 | **Yes** | stock |
| Debian 13 trixie | 1.85.1 | **No** | `trixie-backports` (`rustc` 1.94.1) or rustup |

So `Build-Depends: rustc (>= 1.88)` is **not satisfiable on stock trixie**,
and `05-DEBIAN-UBUNTU-COMPATIBILITY.md §6`'s proposed
`rustc (>= 1.82) | rustc-1.82` is stale by two floors.

**Recommendation.** Build the official `.deb` artifacts in **CI, with a
pinned rustup toolchain**, from the same immutable tag as the RPM. Declare
`Build-Depends: rustc (>= 1.88) | rustc-1.91, cargo (>= 1.88) | cargo-1.91`
so the source package states the truth, and record in
`debian/README.source` that the published binaries are built with a pinned
toolchain because two of three targets cannot satisfy the floor from their
own archive. This is a third-party `.deb`, not a Debian-archive submission;
that distinction is what makes it acceptable (CITED,
`05 §6` item 1, **PLAT-DEC-011**).

**Runtime** dependencies are unaffected: `rustc` is a build dependency only,
and the binaries need `libc6` plus, for the GUI, `libgtk-4-1 (>= 4.12)` and
`libadwaita-1-0 (>= 1.5)` — all satisfied on all three targets. The
libadwaita **1.5.0** floor on Ubuntu 24.04 has **zero margin**; the existing
`linux-distro-compat.yml` row guards it and must not be weakened.

### 6.3 Proposed tree — DESIGN ONLY, NOT CREATED

Location: **`packaging/debian/`**, matching `packaging/fedora/`. The build
copies or symlinks it to `./debian/` at package time, so the repository root
stays clean. Every file below is justified; nothing speculative is included.

```
packaging/debian/
├── control                       # source + 2 binary packages
├── rules                         # dh $@ + overrides
├── changelog                     # 0.1.0-1
├── copyright                     # DEP-5, Apache-2.0
├── source/format                 # "3.0 (quilt)" or "3.0 (native)"  → Q5, §19
├── omnibridge.install            # core paths
├── omnibridge.user.service       # -> dh_installsystemduser
├── omnibridge-gui.install        # GUI paths
└── README.source                 # why the toolchain is pinned (§6.2)
```

No `debian/compat` file: `debhelper-compat (= 13)` belongs in `Build-Depends`
and the separate file is obsolete.

**`omnibridged.service` is NOT duplicated.** `debian/omnibridge.user.service`
is the name `dh_installsystemduser` expects; `debian/rules` copies
`packaging/fedora/omnibridged.service` into place, or the unit moves to
`packaging/common/` in Phase 2 and both formats read it from there. One unit
file, two packages — the unit is already distribution-neutral, as
`README.md:205-208` states.

**Maintainer scripts: none.** This is deliberate and each omission is
justified:

| Would-be script | Why it is not needed |
| --- | --- |
| `postinst` — enable the unit | `dh_installsystemduser --no-enable` generates the correct fragment. Writing it by hand would be worse. |
| `postinst` — `update-desktop-database` | `desktop-file-utils` declares a dpkg **trigger** on `/usr/share/applications`. |
| `postinst` — `gtk-update-icon-cache` | `hicolor-icon-theme` declares a trigger on `/usr/share/icons/hicolor`. |
| `postinst` — D-Bus `ReloadConfig` | Impossible as root (§4.4). Handled in the daemon (§8). |
| `postrm purge` — remove user data | **Must not exist** (§4.9). |
| `prerm` — stop the daemon | Cannot reach a user's service manager. `dh_installsystemduser` emits the correct no-op. |

`debian/rules`, in outline:

```make
#!/usr/bin/make -f
export DEB_BUILD_MAINT_OPTIONS = hardening=+all
%:
	dh $@
override_dh_auto_build:
	cd desktop && cargo build --release --locked --offline
override_dh_auto_test:
	cd desktop && cargo test --release --locked --offline
override_dh_auto_install:
	install -Dm0755 desktop/target/release/omnibridged     debian/tmp/usr/bin/omnibridged
	install -Dm0755 desktop/target/release/omnibridge      debian/tmp/usr/bin/omnibridge
	install -Dm0755 desktop/target/release/omnibridge-gui  debian/tmp/usr/bin/omnibridge-gui
	desktop/gui/tools/install-desktop-metadata.sh --prefix /usr --destdir debian/tmp
override_dh_installsystemduser:
	dh_installsystemduser --no-enable
```

`--offline` with a vendored source tree is what makes a buildd build possible
(§13). The **same** `install-desktop-metadata.sh` invocation appears in the
spec's `%install`, so the `.desktop`, icon and D-Bus service are byte-identical
across formats and cannot drift.

Targets: **Ubuntu 24.04 LTS, Ubuntu 26.04 LTS, Debian 13 trixie.** GTK ≥ 4.12
and libadwaita ≥ 1.5 hold on all three; Ubuntu 22.04 and Debian 12 remain out
of scope and the floor is not lowered for them.

---

## 7. systemd user-service plan

The unit stays a **user** unit. The daemon stays unprivileged. No system unit
is proposed.

### 7.1 The unit, as it must become

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

# --- the fix for P2 -------------------------------------------------------
# $XDG_RUNTIME_DIR/omnibridge, created by systemd, mode 0700, removed when the
# unit stops. Measured working under ProtectSystem=strict; see the audit §4.2.
RuntimeDirectory=omnibridge
RuntimeDirectoryMode=0700

# --- the data directory ---------------------------------------------------
# $XDG_DATA_HOME/omnibridge holds identity.key and state.json. It is the one
# path outside the runtime directory the daemon writes.
#
# StateDirectory= is NOT used: for a user unit it maps to $XDG_STATE_HOME
# (~/.local/state), which the daemon never opens.
#
# The '-' prefix is load-bearing. Without it a unit whose data directory does
# not yet exist refuses to start, which is every fresh install.  <-- measured
ReadWritePaths=-%h/.local/share/omnibridge

NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
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

Changes from today, and only these: `RuntimeDirectory=` + `RuntimeDirectoryMode=`
added; `StateDirectory=omnibridge` removed; `ReadWritePaths=` gains the `-`
prefix. **`ProtectSystem=strict` is untouched and every other hardening
directive is byte-identical.**

### 7.2 The one thing this does not solve, and how it is closed

With `-%h/.local/share/omnibridge` and `ProtectHome=read-only`, a fresh
install starts the unit — and then the daemon tries to create its data
directory inside a read-only `$HOME`. **UNVERIFIED and it must be measured
first**, because two outcomes are possible:

* the `ReadWritePaths=` hole is punched at the path regardless of existence,
  and the daemon creates the directory through it — nothing more to do; or
* the hole is not established for a missing path, and the first start fails.

If the second: the fix is a `%post`-free, scriptlet-free one — grant
`ReadWritePaths=%h/.local/share` (the parent, which exists for every desktop
user) instead of the leaf. That is one directory wider than ideal and is
still inside `ProtectHome=read-only`, so it is a bounded, reviewable
widening rather than a weakening of `ProtectSystem`.

**Phase 1, gate S1** measures this before anything else is built. It is the
single highest-value measurement left in this plan.

### 7.3 Lifecycle

| Format | Mechanism | Enabled by default? |
| --- | --- | --- |
| RPM | `BuildRequires: systemd-rpm-macros`; `%systemd_user_post omnibridged.service`, `%systemd_user_preun`, `%systemd_user_postun` | **No** — no preset line, so `systemctl --global preset` leaves it disabled (MEASURED, §4.3) |
| DEB | `dh_installsystemduser --no-enable` | **No** |

Documented one-liner, identical on every target:

```bash
systemctl --user enable --now omnibridged.service
```

and, for availability while logged out, the deliberate opt-in
`loginctl enable-linger $USER` — already documented correctly in
`packaging/fedora/README.md §Lingering`.

**Upgrade does not restart a running daemon.** Inherent to user units; a root
scriptlet has no route to a user's service manager. Document it plainly in
the README and prove it in gate L17/L18.

---

## 8. D-Bus activation plan

**Requirement (P4):** after install or upgrade into a **live** session,
`io.github.yurisismotto.omnibridge` must be activatable by name without a
logout or reboot.

### 8.1 What packaging does

`%install` / `override_dh_auto_install` call the existing script:

```bash
desktop/gui/tools/install-desktop-metadata.sh --prefix /usr --destdir "$BUILDROOT"
```

which writes, into the buildroot only:

| Path | Mode | Package |
| --- | --- | --- |
| `/usr/share/applications/io.github.yurisismotto.omnibridge.desktop` | 0644 | `omnibridge-gui` |
| `/usr/share/dbus-1/services/io.github.yurisismotto.omnibridge.service` | 0644 | `omnibridge-gui` |
| `/usr/share/icons/hicolor/scalable/apps/io.github.yurisismotto.omnibridge.svg` | 0644 | `omnibridge` (core — §4.7) |

with `Exec=/usr/bin/omnibridge-gui --gapplication-service` (absolute, P5) and
the desktop entry and icon installed **verbatim** so identity cannot drift
between a development install and a package.

The script's `refresh_caches()` returns early when `--destdir` is set, so no
build machine's caches are touched. On the installed machine the desktop
database and icon cache are refreshed by the distribution's own **file
triggers** (MEASURED, §4.4) — the package writes no scriptlet for either.

### 8.2 The session-bus reload — the part packaging cannot do

Established in §4.4: root has no route to a user's session bus, and no
file-trigger equivalent exists for `/usr/share/dbus-1/services`.

**Recommended mechanism — `omnibridged` self-heals its own activation.**

On startup, after `omnibridged` has its session-bus connection (it already
holds one, for the StatusNotifierItem and for notifications):

1. call `org.freedesktop.DBus.ListActivatableNames`;
2. if `io.github.yurisismotto.omnibridge` is **absent**, call
   `org.freedesktop.DBus.ReloadConfig` **once**, then re-check;
3. log the outcome at `debug` and continue either way. Never retry in a loop,
   never fail startup.

Why this is the right seam, and not a workaround:

* It runs **as the user, in the user's session, on the user's bus** — the only
  context where the call is meaningful or permitted.
* `ReloadConfig` is the bus's own documented mechanism and is exactly what
  `install-desktop-metadata.sh:128-132` already calls for a development
  install, with a comment recording it measured on Fedora 44's dbus-broker.
* It fires at precisely the right moments: fresh install → user enables the
  unit → daemon starts → reload; upgrade → user restarts the daemon → reload.
* It is idempotent, cheap, and a no-op on a freshly booted session where the
  bus already read the directory (CITED, KDE §40).
* It needs **no** new dependency: `zbus` is already a direct dependency of
  `omnibridge-linux` under the `tray` feature.

**This is product code, not packaging.** It touches `omnibridged` and needs
its own test. **§18 carries it as the one scope decision requiring sign-off.**

Rejected alternatives, and why:

| Alternative | Rejected because |
| --- | --- |
| `gdbus call --session` in `%post` | Runs as root; no session bus. |
| Root enumerating `/run/user/*/bus` | A privilege-boundary violation dressed as a fix. |
| An RPM file trigger on `/usr/share/dbus-1/services` | No such trigger exists or can exist for the session bus (MEASURED). |
| A shipped systemd **user** unit that calls `ReloadConfig` at session start | Pointless: the bus already reads the directory at session start (CITED, KDE §40). It would do nothing in the only case that matters. |
| Document "log out and back in" | Explicitly excluded by the brief, and correctly. |

---
## 9. Firewall plan

Grounded in §4.6, where the research documentation's Fedora claim was measured
wrong.

### 9.1 What actually needs opening

| Flow | Protocol / port | Direction |
| --- | --- | --- |
| Peer sessions | TCP **55432** | inbound |
| Discovery | UDP **5353** to `224.0.0.251` / `ff02::fb` | inbound multicast |

Nothing else. No port range, no outbound rule, no forwarding.

### 9.2 Fedora

Ship `/usr/lib/firewalld/services/omnibridge.xml`, owned by the **core**
package (the daemon listens, not the GUI):

```xml
<?xml version="1.0" encoding="utf-8"?>
<service>
  <short>OmniBridge</short>
  <description>Local-first device continuity. Peer sessions on TCP 55432.
  Discovery uses mDNS; enable firewalld's own "mdns" service for that.</description>
  <port protocol="tcp" port="55432"/>
</service>
```

* **TCP 55432 only.** firewalld already ships `mdns.xml`, correctly scoped to
  the two multicast destinations (MEASURED, §4.6). Redeclaring it would be a
  second thing to keep right and a broader rule than the stock one.
* **Never enabled by a scriptlet.** No `firewall-cmd` in `%post`, `%preun` or
  `%postun`, ever.
* On **Fedora Workstation** nothing needs doing at all — the default zone
  already covers both (MEASURED). The file exists for `public` and
  `FedoraServer`.
* `%post` and the README give the exact two commands:

  ```bash
  sudo firewall-cmd --permanent --add-service=omnibridge
  sudo firewall-cmd --permanent --add-service=mdns
  sudo firewall-cmd --reload
  ```

* **Diagnosis where the problem appears:** `omnibridge status` should report
  a listener that is bound but unreachable from the LAN, which is the
  research document's own recommendation (`07 §7`, PKG-008). Nice-to-have for
  v1; carried as **Q4**.

### 9.3 Debian / Ubuntu

Ship **no** firewall metadata. Debian 13 enables no firewall and Ubuntu ships
`ufw` inactive (CITED, `07 §7`; re-measured by gate L11, not by this audit).
Shipping a firewalld XML into a `.deb` would be dead weight on nearly every
install and would imply a firewalld that is usually absent.

Document, for users who have turned `ufw` on:

```bash
sudo ufw allow 55432/tcp
sudo ufw allow mdns
```

### 9.4 Uninstall

Package removal deletes `omnibridge.xml`. A user who had added the service
keeps a permanent firewalld config naming a definition that no longer exists,
and firewalld warns about it.

**The package must not edit the user's firewall on removal.** Silently
removing a rule the user added is the same violation as silently adding one.
Document the cleanup:

```bash
sudo firewall-cmd --permanent --remove-service=omnibridge && sudo firewall-cmd --reload
```

Open question **Q4**, §19: whether `%postun` should *print* that line on
uninstall (not run it). Recommended answer: no — postun output is rarely seen;
put it in the README's uninstall section instead.

---

## 10. Desktop integration plan

| Component | Source | Installed path | Package |
| --- | --- | --- | --- |
| GUI binary | `desktop/target/release/omnibridge-gui` | `/usr/bin/omnibridge-gui` | `omnibridge-gui` |
| Desktop entry | `desktop/gui/data/…omnibridge.desktop`, **verbatim** | `/usr/share/applications/io.github.yurisismotto.omnibridge.desktop` | `omnibridge-gui` |
| D-Bus activation | `…omnibridge.service.in`, `@BINDIR@` → `/usr/bin` | `/usr/share/dbus-1/services/io.github.yurisismotto.omnibridge.service` | `omnibridge-gui` |
| App icon | `docs/design/assets/omnibridge-app-icon.svg`, **verbatim, no redraw** | `/usr/share/icons/hicolor/scalable/apps/io.github.yurisismotto.omnibridge.svg` | **`omnibridge` (core)** |
| App resources | GResource, compiled into the binary by `build.rs` | — | — |
| AppStream metainfo | **does not exist** | `/usr/share/metainfo/io.github.yurisismotto.omnibridge.metainfo.xml` | `omnibridge-gui` |

All of it comes from one invocation of `install-desktop-metadata.sh`, so the
development install and the package produce identical files.

**Why the icon is in the core package.** `omnibridged` owns the
StatusNotifierItem and its `ICON_NAME` is the app id
(`platform-linux/src/tray/model.rs:47`). A shell resolves that name from
**hicolor**, not from the GUI's compiled-in GResource. If the icon shipped
only with `omnibridge-gui`, a core-only install would show a broken tray icon
on KDE. Since `omnibridge-gui` requires `omnibridge = %{version}-%{release}`,
putting the icon in core guarantees it is present in every configuration.

**GApplication `APP_ID`.** `io.github.yurisismotto.omnibridge` — lowercase.
It is the GApplication id, the Wayland `app_id`, the D-Bus name, the
`.desktop` basename, its `Icon=`, the `.service` basename and `Name=`, and
the tray item's `ICON_NAME` and `ITEM_ID`. Tests in `omnibridge-gui` and
`omnibridge-linux` assert every copy agrees. **Do not take the capitalised
spelling from the research documents** (§3.4).

`StartupWMClass=omnibridge-gui` must survive verbatim: GTK derives a Wayland
`app_id` from the GApplication id but an X11 `WM_CLASS` from the program name,
and the desktop entry's own comment records the measurement.

**AppStream metainfo** is the only new file that must be *authored*. It is not
required for the product to work — it makes OmniBridge visible in GNOME
Software and KDE Discover. **Recommended: in scope for v1**, low cost, and
`appstream-util validate-relax` is a cheap CI gate. Carried as **Q3**.

**Man pages** do not exist for `omnibridged` or `omnibridge` (MEASURED: no
`man/` anywhere). The brief says "if currently supported" — they are not.
**Recommended: out of scope for v1**, recorded as a follow-up. `omnibridged
--help` and `omnibridge --help` are complete today.

---

## 11. Upgrade / remove preservation plan

**Invariant: no package, in any transaction, may create, move, modify or
delete anything under `~/.local/share/omnibridge`, `$XDG_RUNTIME_DIR/omnibridge`
or the user's downloads directory.**

| Transaction | RPM | DEB | User state |
| --- | --- | --- | --- |
| install | files placed; unit **not** enabled | same | untouched |
| upgrade | files replaced | dpkg unpack/configure | **preserved** |
| upgrade, daemon running | old process keeps running until the user restarts it or logs out | same | **preserved** |
| `remove` | binaries, unit, desktop metadata, firewalld XML removed | same | **preserved** |
| `purge` | n/a | no conffiles, **no `postrm purge` branch** | **preserved** |
| reinstall | identity found, pairings intact | same | **preserved** |

Enforcement, not assertion:

1. **No `%postun`, no `postrm`.** If one is ever added for another reason, its
   `purge` branch does nothing. A one-line CI grep over `packaging/` asserting
   that no maintainer script mentions `.local/share` or `$HOME` is cheap and
   permanent.
2. **No conffiles.** OmniBridge reads no `/etc` file. Do not create one.
3. **`%files` claims nothing under `$HOME`.** True today; keep it true.
4. **Identity destruction is a user action**, never a package transaction —
   a future `omnibridge reset`, run knowingly (CITED,
   `19-PACKAGING-AND-DISTRIBUTION.md §5` rule 1: *"Never silently destroy an
   identity."*).
5. **Gates L13, L21, L23, L24, L26 prove it** on real installs, including the
   one that matters most: an Android pairing that survives a package upgrade.

---

## 12. Package manifests

### 12.1 `omnibridge` — core (daemon, CLI, agent integration)

| Path | Mode | Notes |
| --- | --- | --- |
| `/usr/bin/omnibridged` | 0755 | daemon |
| `/usr/bin/omnibridge` | 0755 | CLI |
| `/usr/lib/systemd/user/omnibridged.service` | 0644 | `%{_userunitdir}`; shipped **disabled** |
| `/usr/share/icons/hicolor/scalable/apps/io.github.yurisismotto.omnibridge.svg` | 0644 | tray icon — core, not GUI (§10) |
| `/usr/lib/firewalld/services/omnibridge.xml` | 0644 | **RPM only**; not enabled |
| `/usr/share/licenses/omnibridge/LICENSE` | 0644 | `%license`; DEB: `/usr/share/doc/omnibridge/copyright` |
| `/usr/share/doc/omnibridge/README.md` | 0644 | **not** the whole `docs/` tree (§5.2) |

Dependencies: automatic (`libc`) · `Recommends: upower` ·
`Suggests: wl-clipboard`.
Debian: `Depends: ${shlibs:Depends}, ${misc:Depends}` ·
`Recommends: upower` · `Suggests: wl-clipboard`.

### 12.2 `omnibridge-gui` — desktop application

| Path | Mode | Notes |
| --- | --- | --- |
| `/usr/bin/omnibridge-gui` | 0755 | |
| `/usr/share/applications/io.github.yurisismotto.omnibridge.desktop` | 0644 | verbatim |
| `/usr/share/dbus-1/services/io.github.yurisismotto.omnibridge.service` | 0644 | `Exec=/usr/bin/omnibridge-gui --gapplication-service` |
| `/usr/share/metainfo/io.github.yurisismotto.omnibridge.metainfo.xml` | 0644 | to author — **Q3** |

Dependencies: `Requires: omnibridge = %{version}-%{release}` plus automatic
GTK/libadwaita. Debian: `Depends: omnibridge (= ${binary:Version}),
libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5), ${shlibs:Depends},
${misc:Depends}`.

### 12.3 Explicitly NOT package-owned

| Path | Owner | Why |
| --- | --- | --- |
| `~/.local/share/omnibridge/` | the user | identity, trust, grants, state |
| `~/.local/share/omnibridge/identity.key` (0600) | the user | private key |
| `~/.local/share/omnibridge/state.json` (0600) | the user | trusted peers + per-capability grants |
| `$XDG_RUNTIME_DIR/omnibridge/` (0700) | **systemd**, via `RuntimeDirectory=` | removed when the unit stops |
| `$XDG_RUNTIME_DIR/omnibridge/control.sock` (0600) | `omnibridged` | |
| `<XDG downloads>/OmniBridge/` | the user | received files |
| `~/.config/systemd/user/default.target.wants/omnibridged.service` | the user | created by `systemctl --user enable` |

### 12.4 Ownership rules

* Every installed file belongs to **exactly one** package.
* `%{?_unpackaged_files_terminate_build}` stays at its Fedora default, so a
  file `install-desktop-metadata.sh` writes and `%files` forgets is a build
  failure, not a silent omission.
* No package owns a directory it did not create
  (`/usr/share/icons/hicolor/scalable/apps` etc. belong to
  `hicolor-icon-theme`).
* No user runtime or data path appears in any manifest.

---

## 13. Build and release strategy

**Principle: official artifacts are produced by CI, from an immutable tag, in
a network-isolated build, and never from a developer's working tree.** No
cloud runtime service is introduced; this is build-time CI only, which the
brief permits.

### 13.1 Version derivation — one source

MEASURED: the version exists in **two** places today
(`desktop/Cargo.toml:19` and `omnibridge.spec:4`) with no guard, and there
are **no git tags**.

Make `desktop/Cargo.toml` `[workspace.package] version` the single source.
The release script reads it; the spec's `Version:` and `debian/changelog` are
generated or asserted against it; the tag is `v<version>`. A CI gate asserts
all three agree, in the same style as the existing MSRV guard.

### 13.2 Source archive

```bash
git archive --format=tar.gz --prefix=omnibridge-${V}/ v${V} \
    -o omnibridge-${V}.tar.gz
```

Deterministic, tag-pinned, and it cannot pick up `desktop/target/` or any
untracked file.

### 13.3 Vendored dependencies — the fix for B3

```bash
cd desktop && cargo vendor --locked ../vendor
tar --sort=name --mtime="@${SOURCE_DATE_EPOCH}" --owner=0 --group=0 --numeric-owner \
    -cJf omnibridge-${V}-vendor.tar.xz vendor
```

* RPM: `Source1: omnibridge-%{version}-vendor.tar.xz`, unpacked in `%prep`,
  with a generated `desktop/.cargo/config.toml` pointing `crates-io` at it.
* DEB: same tarball in the source package; `debian/rules` uses `--offline`.

Without this, neither `mock` nor `buildd` can build at all (§5.1, B3).

### 13.4 `--locked` everywhere

Already true of `%build` and `%check`. Keep it, add `--offline`, and never
let a package build update `Cargo.lock`.

### 13.5 Reproducibility

| Practice | Now | Plan |
| --- | --- | --- |
| `Cargo.lock` committed | ✅ | keep |
| `--locked` | ✅ | keep, add `--offline` |
| Vendored sources | ❌ | §13.3 — **blocking** |
| `SOURCE_DATE_EPOCH` | ❌ | from the tag's committer date; feeds the vendor tarball and `%changelog` |
| Deterministic tar | ❌ | `--sort=name --mtime --owner=0 --group=0 --numeric-owner` |
| `-debuginfo` | ❌ suppressed by `%global debug_package %{nil}` | **Q2** — keep suppressed for v1, revisit |
| Byte-identical rebuild | ❌ | aspiration, not a v1 gate |

### 13.6 RPM build

`mock -r fedora-44-x86_64 --rebuild omnibridge-<V>-1.fc44.src.rpm` in CI
(container-in-container works with `--isolation=simple`). Adds a **Fedora row**
to CI for the first time. `rpmlint` as a soft gate.

### 13.7 DEB build

`sbuild`/`pbuilder` or a plain container per target, with a pinned rustup
toolchain (§6.2), producing `.deb` for Ubuntu 24.04, Ubuntu 26.04 and
Debian 13. `lintian` as a soft gate.

### 13.8 Checksums, SBOM, provenance

| Artifact | Plan |
| --- | --- |
| Checksums | `SHA256SUMS` over every artifact, published with the release (CITED: currently **missing**, `19 §7`) |
| SBOM | CycloneDX per artifact via `cargo-cyclonedx`, from the same locked graph (CITED: currently **missing**, CI-005) |
| Provenance | GitHub Actions build attestations, tied to the tag and workflow. Build-time only. |
| GPG signing | Needs a key and a decision on hosting. **Calendar item, not an engineering task** (CITED, `19 §2`). **Q6**. |

### 13.9 Architectures

`x86_64` required, `aarch64` yes (both Rust Tier 1; CITED `19 §3`). Nothing in
the tree is arch-specific. `aarch64` artifacts are a CI-capacity question, not
a code question — **out of scope for v1**, and the packaging must not assume
`x86_64` anywhere.

---
## 14. Lifecycle certification matrix

**One VM at a time.** No two graphical VMs are started simultaneously.
Physical Android target: **SM-X620, Android 16**, used only for L12 and L13,
and **existing pairing/trust must be preserved** throughout.

Rows: **Fedora 44** (real host + KDE VM), **Ubuntu 24.04**, **Ubuntu 26.04**,
**Debian 13**. A distribution is runtime-certified only when every gate below
has actually run on it. Until then the honest word stays *build-supported*.

### 14.1 Pre-gates — measured before any package is built

| ID | Gate | Evidence |
| --- | --- | --- |
| **S1** | The revised unit starts on a machine where `~/.local/share/omnibridge` does **not** exist | `systemctl --user status` clean; `identity.key` created; see §7.2 |
| **S2** | `$XDG_RUNTIME_DIR/omnibridge` exists, mode `0700`, `control.sock` `0600` | `stat -c '%a %n'` |
| **S3** | No `ProtectSystem`/`SystemCallFilter` denial in the journal | `journalctl --user -u omnibridged` |
| **S4** | `mock` builds the SRPM offline | build log; no network access |

### 14.2 The lifecycle gates

| ID | Gate | How it is measured | Evidence that counts |
| --- | --- | --- | --- |
| **L1** | Clean install | `dnf install ./omnibridge*.rpm` / `apt install ./omnibridge*.deb` | exit 0, no scriptlet error |
| **L2** | Installed files manifest | `rpm -ql` / `dpkg -L`, both packages | matches §12 exactly; **no path under `$HOME`** |
| **L3** | Daemon autostart after login | enable once, log out, log in | `systemctl --user is-active omnibridged` → `active` |
| **L4** | Runs as the user, not root | `ps -o user,pid,cmd -C omnibridged` | user = the logged-in user; **no root process** |
| **L5** | Control socket runtime directory | `stat -c '%a %U %n' $XDG_RUNTIME_DIR/omnibridge{,/control.sock}` | dir `700`, socket `600`, both user-owned |
| **L6** | GUI launches from the application menu | click the entry in GNOME/KDE | window opens; **OmniBridge icon**, not a grey square |
| **L7** | D-Bus cold activation | `pkill -x omnibridge-gui`; `gdbus call … ActivateAction quick-panel` | Quick Panel opens; no `ServiceUnknown` |
| **L8** | D-Bus activation **immediately after install in a live session** | install without logging out, enable+start the daemon, then L7 | `busctl --user list --activatable` lists the name **with no logout** — the §8 mechanism |
| **L9** | Tray integration | KDE: native. GNOME: with, and **without**, the AppIndicator extension | KDE: one correct icon, 3 menu items. GNOME **without** extension: no tray, **everything else works** |
| **L10** | mDNS discovery | phone sees the desktop | `_omnibridge._tcp.local.` advertised; device listed |
| **L11** | TCP 55432 / firewall | `ss -tlnp`; connect from the phone; `firewall-cmd --list-all` / `ufw status` | reachable; **record whether any firewall change was needed** |
| **L12** | Android physical-device discovery | SM-X620, Android 16 | desktop appears; **existing pairing untouched** |
| **L13** | Pairing/trust preserved across upgrade | `sha256sum` + `stat` of `identity.key`/`state.json` before and after `dnf upgrade`/`apt upgrade`; then a live transfer | **identical digests and modes**; the phone still connects without re-pairing |
| **L14** | Clipboard smoke | send both directions | text arrives; `omnibridge clipboard status` honest about `--sensitive` |
| **L15** | Files smoke | send a file each way | lands in `<downloads>/OmniBridge`, `0600` |
| **L16** | Notifications smoke | mirror one | appears; **no content in the journal** |
| **L17** | Package upgrade | install `0.1.0-1`, then `0.1.0-2` | exit 0; L13 passes; **record that the running daemon was NOT restarted** (§4.9) |
| **L18** | Daemon restart | `systemctl --user restart omnibridged` | socket recreated; **exactly one** tray item |
| **L19** | Logout / login | full session cycle | daemon back; L7 still cold-activates |
| **L20** | Reboot | full reboot | L3, L5, L7, L10 all still pass |
| **L21** | Package remove | `dnf remove` / `apt remove` | binaries/unit/metadata gone; **`~/.local/share/omnibridge` intact, digests unchanged** |
| **L22** | Package reinstall | install again | daemon finds the same identity; phone connects **without re-pairing** |
| **L23** | User state preserved | digests + modes across L17, L21, L22 | unchanged at every step |
| **L24** | Purge semantics (DEB) | `apt purge omnibridge omnibridge-gui` | **`~/.local/share/omnibridge` still intact**; no conffile left |
| **L25** | No orphaned package-owned files | `rpm -Va` / `dpkg -V`; walk §12 paths after removal | nothing left behind |
| **L26** | No root-owned OmniBridge user state | `find ~/.local/share/omnibridge ~/Downloads/OmniBridge $XDG_RUNTIME_DIR/omnibridge ! -user $USER` | **empty** |

### 14.3 Per-distribution applicability

| Gate | Fedora 44 | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 |
| --- | --- | --- | --- | --- |
| L1–L3, L5, L17–L23, L25, L26 | ✔ | ✔ | ✔ | ✔ |
| L4 | ✔ | ✔ | ✔ | ✔ |
| L6–L9 (desktop/tray) | ✔ GNOME + KDE VM | ✔ GNOME | ✔ GNOME | ✔ GNOME |
| L10, L11 | ✔ firewalld | ✔ ufw inactive — **verify** | ✔ | ✔ no firewall — **verify** |
| L12, L13 | ✔ SM-X620 | ✔ SM-X620 | ✔ SM-X620 | ✔ SM-X620 |
| L14 | ✔ | ⚠ `wl-copy --sensitive` absent (2.2.1) — expect an honest refusal, not a failure | ⚠ same | ⚠ same |
| L15, L16 | ✔ | ✔ | ✔ | ✔ |
| L24 | — RPM has no purge | ✔ | ✔ | ✔ |

**No distribution is called runtime-certified until its whole column has
actually run.** This document certifies nothing.

---

## 15. Security checklist

Packaging must not weaken any of the following. Each row names how it is
checked, not merely that it should be.

| Property | Packaging impact | Check |
| --- | --- | --- |
| TLS 1.3 | none — `rustls`/`ring`, statically linked, no OpenSSL | `%check`/`dh_auto_test` runs the transport suite |
| SPKI pinning | none | same suite |
| Proof of possession | none | same suite |
| Local-first, no cloud | **must stay true**: no repo, telemetry or update service is introduced. CI is build-time only. | L11 evidence: `ss -tnp` shows only LAN peers |
| No telemetry | none added | grep the diff |
| Per-peer grants | live in `state.json`, which no package touches | L13, L23 |
| Filesystem permissions | binaries `0755` root:root; metadata `0644`; **no setuid, no setgid, no capabilities** | `rpm -qlv` / `dpkg -c`; `find … -perm /6000` empty |
| Runtime directory mode | `RuntimeDirectoryMode=0700` — MEASURED working (§4.2) | L5 |
| Data directory ownership | created by the daemon, `0700`, user-owned; **never package-owned** | L2, L26 |
| systemd hardening | `ProtectSystem=strict` **not weakened**; only `RuntimeDirectory=` added and `ReadWritePaths=` made tolerant of absence | diff the unit; S3 shows no denial |
| Package script privileges | **no scriptlet does anything privileged.** No `firewall-cmd`, no `systemctl --user`, no `$HOME` access, no network. Only the standard `%systemd_user_*` macros. | review `%pre/%post/%preun/%postun` and every `debian/*.postinst` |
| Firewall changes | **none made by the package.** A definition is shipped; the user enables it. | L11; §9 |
| Daemon privilege | **user unit only.** No system unit, no root path, anywhere. | L4 |
| Identity destruction | **no package transaction may destroy it** | L21, L23, L24 |
| Supply chain | `--locked` + vendored sources + checksums + SBOM + provenance | §13 |

Two specific prohibitions, restated because they are easy to breach by
accident:

1. **Do not weaken `ProtectSystem=strict` to make the socket writable.** The
   measured fix is `RuntimeDirectory=` (§4.2, probe D).
2. **Do not open ports or install a shell extension on the user's behalf.**
   §9, §4.8.

---

## 16. Documentation changes

Active documentation only. **No historical certification report is rewritten.**

| Document | Change | Priority |
| --- | --- | --- |
| `README.md` §Running on Linux | **New "Install" section, before "Build and run".** There is currently **no** package installation documentation at all (MEASURED: no `.rpm`, `.deb` or `dnf install omnibridge` anywhere in the README). Cover: Fedora RPM, Ubuntu 24.04/26.04 and Debian 13 DEB, the `omnibridge` / `omnibridge-gui` split, and `systemctl --user enable --now omnibridged.service`. | **P0** |
| `README.md` §Running on Linux | Demote the current `install -Dm0644 packaging/fedora/omnibridged.service ~/.config/systemd/user/…` recipe to a "from source" subsection, and drop the line calling the Fedora-named directory "a packaging debt" once `packaging/common/` exists. | **P0** |
| `README.md` | Firewall behaviour: **Fedora Workstation needs nothing** (MEASURED, §4.6); `public`/`FedoraServer` need the service enabled; Debian/Ubuntu need nothing unless `ufw` is active. Correct the impression left by the research doc. | **P0** |
| `README.md` §Known limitations | Add the GNOME tray paragraph (§4.8), stating plainly that OmniBridge works fully without the extension. | **P0** |
| `README.md` | Upgrade behaviour: user state is preserved; **a running daemon is not restarted by an upgrade** and the user restarts it or logs out. | **P1** |
| `README.md` | Uninstall behaviour: `remove` and `purge` both keep `~/.local/share/omnibridge`; identity is destroyed only by an explicit user action; the firewalld cleanup line. | **P1** |
| `packaging/fedora/README.md` | Rewrite for the real spec: subpackages, `systemd-rpm-macros`, vendored offline build, firewalld, D-Bus. **Correct the claim that the unit "applies the usual systemd hardening"** to name what a user unit actually enforces, citing the §4.2 measurement. | **P0** |
| `packaging/debian/README.source` | **New.** Why the toolchain is pinned (§6.2); the vendored source archive; why there are no maintainer scripts. | **P0** |
| `desktop/gui/README.md` | Mention `tools/install-desktop-metadata.sh` — it is the mechanism for the desktop entry, icon and D-Bus activation and the README does not mention it at all today. | **P1** |
| `docs/research/platform-expansion/07-LINUX-PACKAGING.md` | Add a dated **verification note** (do not rewrite the research): Fedora Workstation does **not** block 55432 (§4.6), and the app id is lowercase `io.github.yurisismotto.omnibridge` with the named artwork files gone (§3.4). | **P1** |
| `docs/research/platform-expansion/05-DEBIAN-UBUNTU-COMPATIBILITY.md` | Same treatment: `Build-Depends` shows `rustc (>= 1.82)`; the real floor is **1.88** and stock trixie cannot meet it (§6.2). | **P1** |
| `docs/research/platform-expansion/19-PACKAGING-AND-DISTRIBUTION.md` | Note that checksums, SBOM and provenance move from "missing" to §13. | **P2** |
| New: `LINUX-PACKAGING-V1-CERTIFICATION.md` | The lifecycle evidence report, written **only** once §14 has actually run. | **P0 at the end** |

---

## 17. Ordered implementation phases

Each phase ends in a state that can be merged on its own.

### Phase 0 — Decisions (no code)

Resolve Q1–Q6 (§19). **Q1 is the only blocker**: whether `omnibridged` may
gain the ~20-line D-Bus self-heal (§8). Everything else has a recommended
answer that can be adopted as-is.

### Phase 1 — Measure and fix the unit

1. **Gate S1** (§14.1): does the revised unit start where
   `~/.local/share/omnibridge` does not exist? Decide the leaf-vs-parent
   `ReadWritePaths=` question on evidence (§7.2).
2. Apply the §7.1 unit: `RuntimeDirectory=` + `RuntimeDirectoryMode=0700`,
   drop `StateDirectory=`, `-` prefix on `ReadWritePaths=`.
3. Move it to `packaging/common/omnibridged.service` — one unit, two formats.
4. Gates S2, S3.

Closes **P2**. Independently valuable: it also fixes the `~/.config/systemd/user`
recipe the README already documents.

### Phase 2 — Make the RPM build at all

1. `BuildRequires`: `rust >= 1.88` (P1), `systemd-rpm-macros` (B2),
   `pkgconf-pkg-config`, `gtk4-devel`, `libadwaita-devel`, `glib2-devel` (B1).
2. Vendored source archive + `--offline` (B3, §13.3).
3. Release script: tarball + vendor tarball + version assertion (§13.1-13.2).
4. Gate S4: `mock` builds it offline.

Closes **B1, B2, B3, P1**. **Nothing downstream can start until this is green.**

### Phase 3 — Complete the RPM product

1. Split into `omnibridge` + `omnibridge-gui`.
2. Call `install-desktop-metadata.sh --prefix /usr --destdir %{buildroot}`;
   assign the three files per §12. Closes **P5, P7**.
3. `%systemd_user_post/_preun/_postun`; ship disabled. Closes **P3**.
4. `firewalld` service XML, not enabled. Closes **P6** (Fedora).
5. `%doc README.md` only, not `docs/`. `Suggests: wl-clipboard`.
6. AppStream metainfo if Q3 says yes.
7. The §8 daemon self-heal, if Q1 says yes. Closes **P4**.

### Phase 4 — Debian/Ubuntu packaging

`packaging/debian/` per §6.3, no maintainer scripts, `dh_installsystemduser
--no-enable`, targeting Ubuntu 24.04 / 26.04 / Debian 13. Closes **the DEB gap**.

### Phase 5 — CI artifact pipeline

Fedora `mock` row (the first Fedora row in CI), three DEB rows, tag-triggered
release job, `SHA256SUMS`, SBOM, provenance, version/MSRV assertions
(§13.6–13.8). Closes **P10** as an enforced contract rather than a table.

### Phase 6 — Documentation

§16, P0 and P1 rows. Written against what Phases 1-5 actually produced.

### Phase 7 — Lifecycle certification

§14, one VM at a time: Fedora 44 (host + KDE VM), Ubuntu 24.04, Ubuntu 26.04,
Debian 13. SM-X620 for L12/L13 only, pairing preserved. Produces
`LINUX-PACKAGING-V1-CERTIFICATION.md`. Closes **P8, P9** with evidence.

Only after Phase 7 may any distribution's README row change from
*build-supported* to *runtime-certified*.

---

## 18. Recommended branch / sprint breakdown

Sequential. Each branch is a PR into `develop`. `feature/linux-packaging-v1`
(this branch) carries only this audit.

| # | Branch | Scope | Closes | Merge gate |
| --- | --- | --- | --- | --- |
| 1 | `fix/systemd-user-unit-runtime-dir-v1` | Phase 1. Unit fix + move to `packaging/common/`. | **P2** | S1, S2, S3 measured and quoted in the PR |
| 2 | `fix/fedora-spec-buildable-v1` | Phase 2. BuildRequires, vendoring, release script, version guard. | **P1, B1, B2, B3** | `mock` builds offline; SRPM attached |
| 3 | `feature/fedora-rpm-product-v1` | Phase 3 items 1-6. Subpackages, desktop integration, lifecycle macros, firewalld. | **P3, P5, P6, P7, P10** (RPM half) | RPM installs on the real Fedora host; L1, L2, L5, L6 pass |
| 3b | `feature/dbus-activation-self-heal-v1` | Phase 3 item 7. **Product change in `omnibridged`** (§8), with tests. **Only if Q1 = yes.** | **P4** | L8 passes; unit test for the ListActivatableNames→ReloadConfig path |
| 4 | `feature/debian-packaging-v1` | Phase 4. `packaging/debian/`. | DEB gap, **P10** (DEB half) | `.deb` builds and installs on all three targets |
| 5 | `ci/packaging-artifacts-v1` | Phase 5. Fedora + DEB rows, release job, checksums, SBOM, provenance. | build/release strategy | tag build produces every artifact with checksums |
| 6 | `docs/packaging-v1-documentation` | Phase 6. §16 P0 + P1. | documentation debts | README describes what actually ships |
| 7 | `test/linux-packaging-certification-v1` | Phase 7. §14 on all four distributions + SM-X620. | **P8, P9** with evidence | every L-gate run; certification report added |

Branch **3b** is separable on purpose: it is the only one that touches product
code, and keeping it apart means the packaging PRs stay reviewable as
packaging.

Suggested sprints: **1+2** (build works), **3+3b** (Fedora product complete),
**4+5** (Debian + CI), **6+7** (docs + certification).

---

## 19. Risks and open questions

### Decisions needed

| # | Question | Recommendation | Blocking? |
| --- | --- | --- | --- |
| **Q1** | May `omnibridged` gain the ~20-line D-Bus self-heal (§8)? It is product code in a packaging sprint. | **Yes.** It is the only mechanism that runs as the right user on the right bus, it is idempotent, it needs no new dependency, and without it **P4 cannot be met at all**. | **YES — blocks branch 3b** |
| **Q2** | Keep `%global debug_package %{nil}`? | **Keep for v1.** No `-debuginfo` package; document the trade-off. Revisit if Fedora-archive inclusion is ever pursued. | No |
| **Q3** | Author AppStream metainfo for v1? | **Yes.** Small, and without it OmniBridge is invisible in GNOME Software and KDE Discover. Gate with `appstream-util validate-relax`. | No |
| **Q4** | Should `%postun` print firewalld cleanup guidance? | **No.** Put it in the README's uninstall section; postun output is rarely read. | No |
| **Q5** | Debian source format `3.0 (quilt)` or `3.0 (native)`? | **`3.0 (quilt)`** — it matches an upstream tarball plus packaging, and keeps the door open to a proper `.orig.tar.gz` with the vendor tarball alongside. | No |
| **Q6** | GPG signing key and artifact hosting. | GitHub Releases + a detached-signed `SHA256SUMS`. Needs a key; **calendar time, not engineering time** — start now. | No |

### Risks

| # | Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- | --- |
| **R1** | `%check` fails in `mock`: the daemon suite binds TCP and the runtime suite touches mDNS multicast in a network-isolated chroot. | Medium | Build blocked | Measure in Phase 2. If it fails, gate those suites on an env var (PKG-006) — **do not** delete them. Display tests are already `#[ignore]`d. |
| **R2** | `ReadWritePaths=` with `-` does not let the daemon create a missing data directory under `ProtectHome=read-only` (§7.2). | Medium | First start fails | Gate **S1** measures it before any packaging is written. Fallback: grant the parent `~/.local/share`. |
| **R3** | Debian 13's stock `rustc` 1.85.1 cannot build the tree. | **Certain** (§6.2) | Source package not buildable on stock trixie | CI builds with a pinned rustup toolchain; `README.source` states it; `Build-Depends` names `rustc (>= 1.88) \| rustc-1.91`. |
| **R4** | Ubuntu 24.04's libadwaita is exactly **1.5.0** — zero margin. | Low | GUI stops building on the biggest LTS | The existing `linux-distro-compat.yml` guard already fails if the image stops supplying 1.5.x. Do not weaken it. |
| **R5** | `wl-clipboard` 2.2.1 on all three Debian/Ubuntu targets lacks `--sensitive`. | **Certain** (CITED) | Sensitive clips refused | Already handled honestly in product. Packaging must **not** add a version dependency — Fedora's `2.2.1^git…` *has* the flag with the same version string (CITED, `19` verification note). `Suggests:` only. |
| **R6** | A future maintainer adds a `postrm purge` that removes `~/.local/share/omnibridge`. | Low | **Destroys user identity** | §11 rule 1 + a permanent CI grep over `packaging/` for `$HOME` / `.local/share` in any maintainer script. |
| **R7** | Enabling the user unit globally is later adopted for convenience. | Low | A LAN listener on every account | §4.3 records the decision and its reasons; revisit only alongside a GUI "start at login" toggle. |
| **R8** | Packaging drifts from the app id and installs a differently-named icon. | Low | Grey square on every desktop | Only `install-desktop-metadata.sh` installs these files, and `brand_assets.rs` guards the artwork. Add a CI check that `rpm -ql`/`dpkg -L` contains the exact APP_ID paths. |
| **R9** | The `.desktop` or `.service` file is edited by packaging instead of installed verbatim. | Low | Identity drift between dev and package | The script installs both verbatim by design; only `@BINDIR@` is substituted. Keep it that way. |
| **R10** | `docs/` shipped as `%doc` (105 files, 1.6 MB) ends up in the binary package. | Medium | Bloat; research docs presented as user documentation | §12: `%doc README.md` only. |

### Not verified by this audit

* Ubuntu's `ufw`-inactive and Debian's no-firewall defaults (CITED, not re-measured) → **L11**.
* Whether `%check` survives `mock` (**R1**) → Phase 2.
* Whether the `-`-prefixed `ReadWritePaths=` suffices on a fresh machine (**R2**) → **S1**.
* Every runtime behaviour on Ubuntu 24.04, Ubuntu 26.04 and Debian 13 — those
  three remain **build-supported**, not certified.
* The exact body of `%systemd_user_post` on Fedora 44: `systemd-rpm-macros` is
  not installed on this host, so the macro was reasoned about from documented
  behaviour, not read → Phase 2.

---

## 20. Verdict

# PACKAGING V1: READY FOR IMPLEMENTATION

The audit is complete. Every one of P1–P10 was re-checked against the tree at
`23ca7f6` rather than carried over from a report. Three additional
build-fatal defects (B1, B2, B3) were found, and the systemd defect was
measured and its fix verified on this host. Every gap has a named owner
phase, a branch, and a gate that proves it.

**Conditions, all of which are work rather than unknowns:**

1. **Q1 must be answered before branch 3b.** P4 — D-Bus activation usable
   immediately after install into a live session — **cannot be met by
   packaging alone**. It needs ~20 lines in `omnibridged`. That crosses the
   "packaging only" line and is the one decision that needs explicit
   sign-off. If Q1 is *no*, P4 stays open and the honest statement becomes
   "cold tray activation works from the next login", which the brief
   explicitly rejects.
2. **Gate S1 runs first.** It is cheap, it decides one line of the unit, and
   getting it wrong means every fresh install fails to start (R2).
3. **Phase 2 gates everything.** The RPM does not build today. Until `mock`
   produces a package offline, no downstream phase can be tested.
4. **No distribution may be called runtime-certified** until its full §14
   column has run. Fedora is the only one with prior runtime evidence, and
   that evidence is for a source build, not a package.

**Nothing found in this audit blocks starting.** Every blocker is a task with
a plan, a gate and an owner.

---

## Appendix A — Commands run during this audit

Read-only inspection of the repository, plus four transient systemd probes on
the local host. **No repository file was modified. Nothing was staged. No VM
was started. The SM-X620 was not touched.**

| Purpose | Command |
| --- | --- |
| Branch / history | `git branch --show-current`, `git status --short`, `git log --graph`, `git show --stat 6307ea8`, `git show -s 23ca7f6` |
| Debian history | `git log --all --diff-filter=A -- 'packaging/debian/*' 'debian/*'` |
| Inventory | `git ls-files packaging desktop docs`, `find packaging -type f` |
| Source | `cat`/`sed`/`grep` over `packaging/fedora/*`, `desktop/*/Cargo.toml`, `desktop/gui/{build.rs,data/*,tools/*}`, `desktop/platform-linux/src/lib.rs`, `desktop/core/src/{lib.rs,store.rs,platform/unix_fs.rs}`, `desktop/daemon/src/main.rs`, `.github/workflows/*` |
| RPM macros | `rpm --eval '%{_userunitdir}'`, `rpm -q systemd-rpm-macros`, `rpm -q --filetriggers desktop-file-utils gtk4` |
| Firewall | `firewall-cmd --get-default-zone --list-services --list-ports`, `cat /usr/lib/firewalld/{zones,services}/*.xml` |
| systemd | `systemctl --version`, `man 5 systemd.exec`, `cat /usr/lib/systemd/user-preset/*` |
| D-Bus | `busctl --user list --activatable` |
| Probes A-D | `systemd-run --user --wait --collect …` (transient; all units and directories removed and verified gone) |
| Toolchain | `rustc -V`, `cargo -V`, `command -v rpmbuild mock dpkg-buildpackage …` |

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was **not** opened, read, modified or
staged at any point.
