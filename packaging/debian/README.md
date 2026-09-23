# Debian and Ubuntu packaging

Targets: **Ubuntu 24.04 LTS**, **Ubuntu 26.04 LTS**, **Debian 13 trixie**.
Ubuntu 22.04 and Debian 12 are out of scope and the GTK / libadwaita floor is
not lowered for them.

`README.source` is the file a *builder* needs — the Rust floor, the offline
build, and what is deliberately absent. This file is the map.

## Two packages, the same two as the RPM

| `omnibridge` | `omnibridge-gui` |
| --- | --- |
| `/usr/bin/omnibridged` | `/usr/bin/omnibridge-gui` |
| `/usr/bin/omnibridge` | `/usr/share/applications/…omnibridge.desktop` |
| `/usr/lib/systemd/user/omnibridged.service` | `/usr/share/dbus-1/services/…omnibridge.service` |
| `/usr/share/icons/hicolor/scalable/apps/…omnibridge.svg` | `/usr/share/metainfo/…omnibridge.metainfo.xml` |

`omnibridge-gui` depends on `omnibridge (= ${binary:Version})` — the exact
build, because the GUI speaks the daemon's control socket and a version skew
there is a protocol skew.

The icon is in the **core** package for the same reason it is on Fedora:
`omnibridged` owns the StatusNotifierItem and a shell resolves its icon name
out of `hicolor`, so a core-only install would otherwise draw a grey square.

## Nothing here is a second copy

Two files that could easily have been duplicated are not:

* **the systemd unit.** `debian/rules` installs
  `packaging/common/omnibridged.service`, the same file `%install` copies. It
  is where `ProtectSystem=strict`, the syscall filter and the address-family
  restriction live, and a second copy is how a hardening change lands on one
  distribution and misses the other.
* **the desktop metadata.** `debian/rules` runs
  `desktop/gui/tools/install-desktop-metadata.sh --prefix /usr --destdir
  debian/tmp` — the identical invocation the spec uses. Three of the four
  files are installed verbatim and only the D-Bus service file's `Exec=` line
  is derived, so a `.deb` and an `.rpm` cannot disagree about the
  application's identity.

`packaging/tests/packaging-checks.sh` asserts both, and asserts that no second
unit file has appeared under `packaging/debian/`.

## No maintainer scripts

None are written by hand, and each omission is deliberate — the table is in
`README.source`. The one that matters most:

> **No script may create, move or delete `~/.local/share/omnibridge`, on any
> path, including `purge`.**

That directory holds the user's identity key and the record of every device
they have paired. `packaging/tests/packaging-checks.sh` greps every maintainer
script for `$HOME` and `.local/share` and fails on a match;
`packaging/tests/install-smoke.sh` plants a fake trust store and compares its
digest, mode and owner across `remove`, `reinstall` and `purge`.

## Building

```bash
./packaging/release/make-source-bundle.sh --rev v1.0.0 --output dist
./packaging/debian/build-deb.sh --image docker.io/library/debian:trixie dist
./packaging/debian/build-deb.sh --image docker.io/library/ubuntu:24.04  dist
./packaging/debian/build-deb.sh --image docker.io/library/ubuntu:26.04  dist
```

`build-deb.sh` is the Debian counterpart of building the RPM in `mock`, and it
is a container for the same three reasons:

* the buildroot installs **nothing by name** — only what `mk-build-deps`
  derives from `debian/control` — so an under-declared `Build-Depends` fails
  the build instead of being covered by the host;
* `cargo` runs `--locked --offline` against a vendored tree with `CARGO_HOME`
  inside the build directory, so a network fetch is impossible;
* it builds as a **normal user**. `omnibridge-core`'s store tests chmod a
  directory to `0000` and assert the read comes back `PermissionDenied`; root
  has `CAP_DAC_OVERRIDE` and reads it anyway, so a root build would pass a
  test that proves nothing.

## Two traps this packaging already fell into

Both are fixed, and both are recorded because neither is obvious.

**`dh_clean` deletes `*.orig` anywhere in the tree.** `cargo vendor` writes a
`Cargo.toml.orig` beside each of the 270 vendored crates and names it in that
crate's `.cargo-checksum.json`. The clean step that runs before every build
was silently removing all of them, and cargo stopped with `failed to calculate
checksum of: …/vendor/anyhow-1.0.104/Cargo.toml.orig`. `debian/rules` now
overrides `dh_clean` with `-X.orig`.

**Automatic `-dbgsym` packages were empty.** debhelper generated
`omnibridge-dbgsym` and `omnibridge-gui-dbgsym`, and lintian reported every
file in them as `debug-file-with-no-debug-symbols`: the release profile emits
no debug info, so they carried nothing. `dh_strip --no-automatic-dbgsym` turns
them off, which also makes this consistent with the RPM's deliberate
`%global debug_package %{nil}` (audit Q2).

## lintian

Run automatically by `build-deb.sh` and recorded, not suppressed. What remains
on a clean build:

| Tag | Why it stays |
| --- | --- |
| `initial-upload-closes-no-bugs` ×2 | It expects the changelog to close an ITP bug in the Debian BTS. This is a third-party package and there is no ITP. |
| `no-manual-page` ×3 | True. Man pages do not exist for any of the three binaries and are in no phase of the plan; `--help` is complete for all of them. The RPM reports the same gap. |

lintian is informational here rather than a gate: its tag set is tuned for the
Debian archive, and this is not an archive submission. Every tag it emits is
in the report.
