# OmniBridge — Packaging v1, Build Foundation (B1 + B2 + B3)

| Field | Value |
| --- | --- |
| **Branch** | `feature/packaging-build-foundation-v1` |
| **Base commit** | `7caecbe` (merge of PR #47, `feature/linux-packaging-v1`) |
| **Date** | 2026-09-22 |
| **Scope** | Audit §18 row 2 — Phase 2, "make the RPM build at all". Closes **B1, B2, B3, P1**. |
| **Authority** | `PACKAGING-V1-READINESS-AUDIT.md`, plus the resolved S1 decision `ReadWritePaths=%h/.local/share`. |
| **Host** | Fedora 44 Workstation, rustc/cargo 1.98.1, podman 5.x, 16 cores |
| **Product code changed** | **No.** No `.rs`, `.kt` or `.proto` file was modified. |
| **Verdict** | **PACKAGING BUILD FOUNDATION: READY FOR COMMIT** |

Claims are labelled **MEASURED** (a command was run here and its output is
quoted), **SOURCE-VERIFIED** (read out of the tree) or **DEFERRED**.

---

## 0. Summary

The RPM builds. It had never been built before — the audit MEASURED that no
`rpmbuild` or `mock` existed on the development host, which is how three
build-fatal defects survived in a 73-line spec.

All three are closed, and a fourth was found in the process:

| # | Defect | Status |
| --- | --- | --- |
| **B1** | GUI build dependencies undeclared | **Fixed**, and proved by a buildroot that installs only what the spec declares |
| **B2** | `%{_userunitdir}` undefined | **Fixed**; the macro now expands to `/usr/lib/systemd/user` |
| **B3** | Cargo reaches crates.io in a network-isolated buildroot | **Fixed**; vendored source bundle, `--offline`, built with no network namespace at all |
| **P1** | Spec declared `rust >= 1.82`, MSRV is `1.88` | **Fixed**, and guarded |
| **B4** *(new)* | `%check` raises a real `dbus-daemon` the buildroot did not have | **Fixed**; see §2.4 |

Produced, offline, from a bare Fedora 44 container with no network namespace:

```
omnibridge-0.1.0-2.fc44.x86_64.rpm      x86_64    4.2 MiB
omnibridge-0.1.0-2.fc44.src.rpm         src        29 MiB
```

carrying `omnibridged`, `omnibridge` **and** `omnibridge-gui` — the last of
which the previous spec compiled and then threw away.

`%check` ran in full inside that isolated buildroot: **65 suites, 997
assertions, 0 failures, 23 ignored.**

The one gap this report originally left open — that the build had never been
put through real `mock` — was closed on 2026-09-22. Fedora's canonical
`fedora-44-x86_64` buildroot rebuilds the SRPM offline and passes the same
`%check`, with nothing in the packaging changed to make it do so. **§17.**

---

## 1. B1 — GUI build dependencies

### Root cause

SOURCE-VERIFIED. `%build` ran `cargo build --release --locked` over the whole
workspace. `omnibridge-gui` is a workspace member (`desktop/Cargo.toml:11`),
so it was always being compiled. `BuildRequires` named `rust`, `cargo` and
`gcc` and nothing else.

The GUI's C-library dependencies are not resolved by cargo. They are resolved
by `system-deps`, which shells out to `pkg-config` from each `-sys` crate's
build script, and by `desktop/gui/build.rs`, which runs
`glib-compile-resources` through `glib-build-tools`. None of that was
declared, so the build died in `desktop/gui` before installing anything.

### How the fix was derived

Not by guessing. MEASURED, in three steps:

1. Read `[package.metadata.system-deps]` out of every locked `-sys` crate,
   which is the table `system-deps` actually consults:

   | crate | pkg-config module |
   | --- | --- |
   | `gtk4-sys`, `gdk4-sys`, `gsk4-sys` | `gtk4` |
   | `libadwaita-sys` | `libadwaita-1` |
   | `glib-sys` / `gio-sys` / `gobject-sys` | `glib-2.0`, `gio-2.0`, `gobject-2.0` |
   | `pango-sys` | `pango` |
   | `graphene-sys` | `graphene-gobject-1.0` |
   | `gdk-pixbuf-sys` | `gdk-pixbuf-2.0` |

2. Map each module's `.pc` file to its owning package with `rpm -qf`:

   ```
   gtk4                  -> gtk4-devel
   libadwaita-1          -> libadwaita-devel
   glib-2.0/gio-2.0/...  -> glib2-devel      (also owns glib-compile-resources)
   pango                 -> pango-devel      | pulled in transitively
   graphene-gobject-1.0  -> graphene-devel   | by gtk4-devel, so not
   gdk-pixbuf-2.0        -> gdk-pixbuf2-devel| declared separately
   /usr/bin/pkg-config   -> pkgconf-pkg-config
   ```

3. Prove the set is sufficient **and** minimal by building in a container that
   installed nothing by name — only what `dnf builddep` derives from the
   spec's own `BuildRequires`. An under-declared dependency fails that build
   rather than being quietly satisfied by the developer's host.

### Fix

```spec
BuildRequires:  pkgconf-pkg-config
BuildRequires:  gtk4-devel >= 4.12
BuildRequires:  libadwaita-devel >= 1.5
BuildRequires:  glib2-devel
```

The two floors are the ones `desktop/gui/Cargo.toml` already enforces through
cargo features and documents as measured: `v4_12` for
`CssProvider::load_from_string`, `v1_5` for `adw::Dialog` /
`adw::AlertDialog`. Stating them in the spec makes a buildroot that is too old
fail in `dnf` with a legible message instead of deep inside a `-sys` crate's
build script.

MEASURED in the buildroot: `gtk4 4.22.5`, `libadwaita-1 1.9.4`,
`glib-2.0 2.88.3`, `glib-compile-resources` at `/usr/bin/glib-compile-resources`.

---

## 2. B2 — systemd RPM macros

### Root cause

MEASURED, on the host:

```console
$ rpm --eval '%{_userunitdir}'
%{_userunitdir}
$ rpm -q systemd-rpm-macros
package systemd-rpm-macros is not installed
```

The macro is owned by `systemd-rpm-macros` and the spec did not require it.
Unexpanded, `%install` would create a directory whose name is the literal
string `%{_userunitdir}` and `%files` would fail on the path that is missing.

### Fix

```spec
BuildRequires:  systemd-rpm-macros
```

The path stays a macro. Hardcoding `/usr/lib/systemd/user` would have made the
symptom go away while throwing away the thing the macro is for — being right
on a distribution that moves it.

MEASURED in the buildroot, before `rpmbuild` was invoked:

```console
### rpm --eval '%{_userunitdir}' -> /usr/lib/systemd/user
```

and in the finished package:

```
/usr/lib/systemd/user/omnibridged.service
```

### 2.4 B4 — a second missing build dependency, found by actually running %check

Not on the audit's list, and it only appears once the build gets far enough to
run tests. `platform-linux/tests/tray_dbus.rs` and `tray_gnome.rs` do not mock
the bus: each test raises its **own** private `dbus-daemon` with no service
directories, publishes a real StatusNotifierItem and drives a fake watcher
built from KDE's own interface signatures. The fixture shells out by name and
says so itself — `platform-linux/tests/common/mod.rs:68`:

```rust
.expect("dbus-daemon should be installed");
```

MEASURED: 18 of 19 tests in that suite failed in 0.03 s in a buildroot without
the binary. The audit's `%check` analysis (§5.2) listed the suites that are
already `#[ignore]`d and concluded PKG-006 was largely discharged; it did not
catch this one, because these tests are not display-requiring — they bring
their own bus, they just need the program that provides it.

Fix: `BuildRequires: dbus-daemon`.

The whole set of external binaries the suite shells out to was then
enumerated rather than patched one failure at a time — `/bin/sh`, `id`,
`loginctl`, `dbus-daemon`. The first three were already in the buildroot.

---

## 3. B3 — offline Cargo build

### Root cause

SOURCE-VERIFIED and MEASURED. `cargo build --locked` reads the committed
lockfile but still **downloads** all 270 crates from crates.io. There was no
`vendor/` directory and no `.cargo/config.toml` anywhere in the tree. `mock`
disables networking during `%build`; Debian `buildd` has no network at all.
No official package could be produced from the spec as written.

`Source0: %{name}-%{version}.tar.gz` made it worse in a quiet way: nothing in
the repository has ever produced that tarball. There was no release script, no
CI job, no `make dist`.

### Fix — four parts

**1. A vendored second source.**

```spec
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz
```

**2. `%prep` wires the replacement in.**

```spec
tar -xf %{SOURCE1} -C desktop
install -Dpm0644 packaging/fedora/cargo-vendor-config.toml desktop/.cargo/config.toml
```

**3. The cargo config is committed, not generated into the bundle.**
`packaging/fedora/cargo-vendor-config.toml` replaces `crates-io` with the
vendored directory. It is committed because `cargo vendor`'s emitted block is
*derived from the dependency graph*: add one `git = "…"` dependency and cargo
emits an extra `[source."git+https://…"]` stanza, silently, and the package
starts needing a network again with nothing in review to show for it. The
release script diffs what cargo emitted against the committed file and refuses
to build a bundle if they disagree.

**4. `%build` and `%check` cannot reach out even if they wanted to.**

```spec
export CARGO_HOME=%{_builddir}/%{name}-cargo-home
cargo build --release --locked --offline -p … -p … -p …
```

`CARGO_HOME` is redirected into the build tree so no `~/.cargo/config.toml`
belonging to whoever runs `rpmbuild` can put a registry back. With that plus
`--offline`, a fetch is not merely unnecessary, it is impossible.

### Evidence

The build ran in a container started with `--network=none`. MEASURED from
inside it, before `rpmbuild`:

```console
### building as: uid=1000(builder) gid=1000(builder) groups=1000(builder)
### network interfaces:
  lo:
### crates.io reachable?
  no DNS resolution
```

`lo` is the only interface. There is no route, no DNS and no proxy. The build
then compiled 270 crates and ran the full test suite to completion.

---

## 4. Source-bundle design

`packaging/release/make-source-bundle.sh`, 322 lines.

### Output

Two tarballs, per audit §13.3, not one combined archive:

| File | Role |
| --- | --- |
| `omnibridge-<V>.tar.gz` | `Source0` — the upstream source |
| `omnibridge-<V>-vendor.tar.xz` | `Source1` — 270 vendored crates |
| `omnibridge-<V>-SOURCES.sha256` | checksums over both |

They are separate so `Source0` stays a pristine upstream tarball: that is what
an RPM expects, what a Debian `3.0 (quilt)` `.orig.tar.gz` must be (audit Q5),
and what lets the vendor tarball be regenerated for a security rebuild without
reissuing the source. Together they are the bundle; neither is useful alone.

MEASURED: 2.3 MiB source, 28 MiB vendor (343 MiB uncompressed).

### Two source modes

* `--rev <tag>` (default `HEAD`) — `git archive` of a pinned revision. It
  cannot pick up an untracked file, a build output or a developer's local
  state, because git does not know about any of them. **This is the release
  path.**
* `--worktree` — copies tracked *and* untracked-but-not-ignored files from the
  working tree, so a packaging change can be proved to build before it is
  committed. It prints a warning saying it is not the release path. This
  sprint used it, because the brief forbids staging or committing; the
  evidence below was produced from a bundle made this way.

### Exclusions

Enforced on the staged tree in **both** modes, so the guarantee does not
depend on which one ran. Anything matched is removed, the tree is re-scanned,
and a survivor is a hard failure rather than a warning.

`.git`, `desktop/target`, `android/**/build`, `android/.gradle`,
`android/.kotlin`, `android/local.properties`, `*.apk`, `*.aab`, `*.key`,
`*.pem`, `*.p12`, `*.pfx`, `*.jks`, `*.keystore`, `id_rsa*`, `id_ed25519*`,
`state.json`, `trust-store.json`, `.vscode`, `*.swp`, `*.rs.bk`, and
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`.

MEASURED on this branch: the only thing removed was the U2 document, which is
untracked and would otherwise have been swept up by `--worktree`.

`protocol/testdata/*.der` is **deliberately not excluded**. Those two files are
X.509 *certificates* — public, no private half — used as cross-language test
vectors by `desktop/core/tests/identity_and_store.rs` and by the Android unit
tests. `%check` fails without them.

`docs/` is **not** trimmed, for a reason that is easy to miss:
`desktop/gui/build.rs` reads `../../docs/design/assets/omnibridge-app-icon.svg`
and copies it into `OUT_DIR` under the icon-theme name. A bundle that dropped
`docs/` to save space would compile until it did not. The script asserts that
file is present, along with `desktop/Cargo.lock`, `desktop/Cargo.toml`, the
spec, the unit, the vendor config, `protocol/proto` and `LICENSE`.

### Determinism

MEASURED byte-identical across two back-to-back runs over an unchanged tree:

```
run 1: 52af81ec70fb612b25478412d3da6932cdf4ed02952944d85a18f63fe6993fb1  omnibridge-0.1.0.tar.gz
run 2: 52af81ec70fb612b25478412d3da6932cdf4ed02952944d85a18f63fe6993fb1  omnibridge-0.1.0.tar.gz
run 1: e591169e50943a94926fd74b61f04a9091428a364c943dcf67e88fd55c3a5e27  omnibridge-0.1.0-vendor.tar.xz
run 2: e591169e50943a94926fd74b61f04a9091428a364c943dcf67e88fd55c3a5e27  omnibridge-0.1.0-vendor.tar.xz
```

`SOURCE_DATE_EPOCH` comes from the archived commit's committer date (an
external value wins). `tar --sort=name --mtime=@$SOURCE_DATE_EPOCH --owner=0
--group=0 --numeric-owner`. `gzip -n`.

One non-obvious part, MEASURED rather than assumed: **`xz` output depends on
the core count of the machine that ran it** unless `--block-size` is set.

| invocation | sha256 (first 16) |
| --- | --- |
| `-T0 -6` (16 cores) | `43ac388364b4d447` |
| `-T1 -6` | `002e8099c1d43692` |
| `-T0 --block-size=16MiB -6` | `4dd319c67805bace` |
| `-T2 / -T3 / -T4 / -T8`, same block size | `4dd319c67805bace` |
| `-T1 --block-size=16MiB -6` | `58c18af874f5e2e7` |

So the script pins `--block-size=16MiB` and floors the thread count at 2,
because a single thread takes xz's non-blocked encoder path and disagrees with
every other count.

A sharper confirmation came from packaging files being edited between builds:
across **four** generations the source tarball hash tracked every edit
(`52af81ec…` → `8a215432…` → `c6b0d044…` → `666640ae…`) while the vendor
tarball hash never moved off `e591169e…`, because the locked dependency graph
never moved either. The bundle's two halves change independently and only when
their own inputs do, which is the property a security rebuild needs.

### Failure modes

All four MEASURED firing, with the message quoted:

| Trigger | Message |
| --- | --- |
| spec/workspace version drift | `version drift: desktop/Cargo.toml says '0.2.0', …spec says '0.1.0'. desktop/Cargo.toml is authoritative — fix the spec.` |
| version unreadable | `could not read [workspace.package] version from desktop/Cargo.toml` |
| `Cargo.lock` inconsistent | `Cargo.lock is inconsistent with the workspace manifests. Run 'cargo update --workspace --offline' or commit the lockfile change; a package build cannot resolve this for you.` |
| vendor config drift | prints the unified diff, then: `cargo vendor emitted a source configuration that packaging/fedora/cargo-vendor-config.toml does not describe. A dependency outside crates.io (a git or path source) was probably added…` |

Vendoring failure is the fifth: `cargo vendor`'s own stderr is echoed and the
script dies with `cargo vendor failed`.

---

## 5. Version source

**`desktop/Cargo.toml` `[workspace.package] version` is authoritative.**
The spec's `Version:` stays a literal copy, and is *asserted* rather than
generated.

Generating it was considered and rejected on a mechanical ground: rpm cannot
read a TOML file at spec-parse time, and an SRPM does not carry `Cargo.toml`
at the point `Version:` is needed. A `%(…)` shell expansion would work in a
developer's checkout and break in the SRPM that is supposed to be the
reproducible artifact.

So the copy is guarded in two places, which is what audit §13.1 asks for:

* `make-source-bundle.sh` refuses to produce a bundle when the two disagree —
  no bundle, no package;
* `packaging/tests/packaging-checks.sh` asserts it **without building
  anything**, so CI can gate on it the same way `linux-distro-compat.yml`
  already gates the MSRV.

The same check covers P1: the spec's `BuildRequires: rust >= 1.88` is asserted
equal to `rust-version` in `desktop/Cargo.toml`. That is the drift that
produced the false `1.82` claim in the first place — the MSRV had a CI guard,
the spec did not, so the spec drifted instead.

No release/versioning framework was invented. There is no tag yet, and none
was created.

---

## 6. RPM BuildRequires — final

```spec
BuildRequires:  rust >= 1.88          # P1: measured MSRV, was 1.82
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config    # B1
BuildRequires:  gtk4-devel >= 4.12    # B1
BuildRequires:  libadwaita-devel >= 1.5  # B1
BuildRequires:  glib2-devel           # B1 (also glib-compile-resources)
BuildRequires:  systemd-rpm-macros    # B2
BuildRequires:  dbus-daemon           # B4, %check only
```

Still deliberately **absent**: `protobuf-compiler`. The build compiles the
schema with `protox` in pure Rust (ADR-0004) and the spec's comment saying so
is accurate and was kept.

Resolved in the buildroot, MEASURED:

```
rust                   rust-1.98.1-1.fc44.x86_64
cargo                  cargo-1.98.1-1.fc44.x86_64
gcc                    gcc-16.2.1-2.fc44.x86_64
pkgconf-pkg-config     pkgconf-pkg-config-2.5.1-1.fc44.x86_64
gtk4-devel             gtk4-devel-4.22.5-2.fc44.x86_64
libadwaita-devel       libadwaita-devel-1.9.4-1.fc44.x86_64
glib2-devel            glib2-devel-2.88.3-1.fc44.x86_64
systemd-rpm-macros     systemd-rpm-macros-259.9-1.fc44.noarch
dbus-daemon            dbus-daemon-1.16.2-1.fc44.x86_64
```

738 packages total in the buildroot, every one of them pulled by
`dnf builddep` from the spec.

---

## 7. Build commands

```bash
# 1. bundle
./packaging/release/make-source-bundle.sh --worktree --output dist

# 2. buildroot: installs nothing by name, only what the spec declares
podman build -t omnibridge-buildroot:f44 -f - . <<'CONTAINERFILE'
FROM registry.fedoraproject.org/fedora:44
RUN dnf -y install rpm-build rpmlint dnf-plugins-core && dnf clean all
COPY omnibridge.spec /tmp/omnibridge.spec
RUN dnf -y builddep /tmp/omnibridge.spec && dnf clean all
RUN useradd -m -u 1000 builder && mkdir -p /build /out && chown builder /build /out
USER builder
CONTAINERFILE

# 3. build, with no network namespace at all
podman run --network=none -v ./dist:/sources:ro,z -v ./out:/out:z \
    omnibridge-buildroot:f44 bash -c '
        mkdir -p /build/{SOURCES,SPECS,BUILD,RPMS,SRPMS,BUILDROOT}
        cp /sources/*.tar.* /build/SOURCES/
        tar -xzOf /build/SOURCES/omnibridge-0.1.0.tar.gz \
            omnibridge-0.1.0/packaging/fedora/omnibridge.spec > /build/SPECS/omnibridge.spec
        rpmbuild --define "_topdir /build" -ba /build/SPECS/omnibridge.spec
        find /build/RPMS /build/SRPMS -name "*.rpm" -exec cp {} /out/ \;'
```

The spec is read **out of the source tarball**, not out of the checkout, so
what was built is what the bundle ships.

### Why a container and not mock

`mock` is the audit's preferred environment (§13.6) and it was **not
available when the work below was done**. MEASURED on the host at that time:
`mock`, `rpmbuild`, `rpmlint` and `rpmdev-setuptree` were all absent, and
`sudo` requires a password, so none of them could be installed.

**This limitation is now closed rather than reported.** `mock` has since been
installed on this host and the same SRPM was rebuilt in the canonical Fedora 44
buildroot: see **§17**. The container evidence below stands as it was
measured, and everything it claimed was confirmed there.

The container is the strongest equivalent available here, and on two of the
three properties that matter it is at least as strict as mock:

| property | mock | this container |
| --- | --- | --- |
| clean buildroot per build | yes | yes — fresh `fedora:44`, nothing carried over |
| network off during build | yes, during `%build` | **stronger** — no network namespace for the whole run, including `%prep` and `%check` |
| only declared deps present | yes | yes — `dnf builddep` on the spec, nothing installed by name |
| unprivileged builder | yes (`mockbuild`) | yes (`builder`, uid 1000) |
| Fedora buildroot package set | authoritative (`build` group) | close but not identical |

The last row is the honest gap: mock installs Fedora's canonical minimal
buildroot, and this container starts from the `fedora:44` image, which is not
byte-identical to it. A dependency that the image happens to carry and the
mock buildroot does not would be missed here. That gap has since been
measured and found empty: mock's canonical buildroot is **489** packages
against the container's 738, and the smaller, authoritative set builds the
package (§17.1, §17.10).

The unprivileged-builder row was not a free choice — see §13.

---

## 8. Offline-build evidence

MEASURED, from the run that produced the artifacts.

```console
### building as: uid=1000(builder) gid=1000(builder) groups=1000(builder)
### network interfaces:
  lo:
### crates.io reachable?
  no DNS resolution
### spec taken from inside the source tarball:
b2308131dd6d236aec76fd0d7ee8ad29be98663d9c95d5ffafbc723e42c9ae24  /build/SPECS/omnibridge.spec
### rpm --eval '%{_userunitdir}' -> /usr/lib/systemd/user
### rpmbuild starting 2026-09-22T04:57:56Z
…
Wrote: /build/SRPMS/omnibridge-0.1.0-2.fc44.src.rpm
Wrote: /build/RPMS/x86_64/omnibridge-0.1.0-2.fc44.x86_64.rpm
### rpmbuild finished 2026-09-22T05:06:50Z
```

Eight minutes fifty-four seconds, start to finish, for 270 crates, three
binaries and the whole test suite.

No line of the 1,700-line build log contains `Downloading`, `Updating
crates.io index`, or a fetch error. The only occurrence of the string
`crates.io` is the probe printed above, which failed to resolve.

Cargo resolved every dependency out of `desktop/vendor`, and the daemon, the
CLI and the GUI all compiled:

```
Compiling omnibridge-gui v0.1.0 (…/desktop/gui)
Compiling omnibridge-daemon v0.1.0 (…/desktop/daemon)
Compiling omnibridge-cli v0.1.0 (…/desktop/cli)
```

---

## 9. Artifacts

| File | Arch | Size |
| --- | --- | --- |
| `omnibridge-0.1.0-2.fc44.x86_64.rpm` | `x86_64` | 4,451,776 B (4.2 MiB) |
| `omnibridge-0.1.0-2.fc44.src.rpm` | `src` | 30,868,506 B (29 MiB) |

Installed size 17,008,332 B. Signature: none — GPG signing is audit Q6 and is
a calendar item, not this sprint's.

`Release` went `1` → `2`. `0.1.0-1` never produced an artifact anywhere, so
reusing it for the first package that actually builds would have put two
different things under one version-release, and `rpmlint` flags the duplicate
changelog entry besides.

**The RPM was not installed on the host.** The brief and the audit both put
lifecycle installation behind later gates.

---

## 10. Package manifest

`rpm -qpl omnibridge-0.1.0-2.fc44.x86_64.rpm`:

```
/usr/bin/omnibridge
/usr/bin/omnibridge-gui
/usr/bin/omnibridged
/usr/lib/systemd/user/omnibridged.service
/usr/share/doc/omnibridge
/usr/share/doc/omnibridge/README.md
/usr/share/doc/omnibridge/docs          (+ 116 paths beneath it)
/usr/share/licenses/omnibridge
/usr/share/licenses/omnibridge/LICENSE
```

All three binaries present, as intended. `omnibridge-gui` is the one the
previous spec built and discarded (audit P7); installing the binary is the
floor, and the desktop integration around it is Phase 3.

`/usr/lib/systemd/user/omnibridged.service` is the B2 fix visible in the
output: the macro expanded, and no path in the package is a literal
`%{_userunitdir}`.

Auto-generated `Requires` picked up the GUI's runtime libraries without being
told to — `libgtk-4.so.1`, `libadwaita-1.so.0`, `libpango-1.0.so.0`,
`libgdk_pixbuf-2.0.so.0`, `libcairo.so.2`, `libgio/gobject/glib-2.0.so.0`.
`Recommends: upower` survived, correctly weak.

The 116 paths under `%doc docs/` are audit §12's bloat finding. Left alone
deliberately: it is Phase 3 item 5, it is not build-fatal, and trimming it
here would mix a content decision into a build fix.

---

## 11. rpmlint

Run inside the buildroot, rpmlint 2.8.0.

### Binary package — 0 errors, 6 warnings

```
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridge
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridge-gui
omnibridge.x86_64: W: unstripped-binary-or-object /usr/bin/omnibridged
omnibridge.x86_64: W: no-manual-page-for-binary omnibridge
omnibridge.x86_64: W: no-manual-page-for-binary omnibridge-gui
omnibridge.x86_64: W: no-manual-page-for-binary omnibridged
1 packages and 0 specfiles checked; 0 errors, 6 warnings, 3 filtered, 0 badness
```

Both warning classes are known and accepted for v1: `unstripped-binary` is
the direct consequence of `%global debug_package %{nil}`, which audit Q2 says
to keep and revisit; man pages are a later phase.

### Source package — 1 error, 2 warnings

```
omnibridge.src: E: spelling-error ('systemd', … -> systems, system, system d)
omnibridge.spec: W: invalid-url Source0: omnibridge-0.1.0.tar.gz
omnibridge.spec: W: invalid-url Source1: omnibridge-0.1.0-vendor.tar.xz
```

The "error" is rpmlint's dictionary not knowing the word *systemd*. It is not
a defect and the text is correct.

`invalid-url` is inherent to a locally generated tarball and is what every
vendored Rust package in Fedora reports. It resolves when the tarballs are
published against a tag, which is the CI phase.

Five `macro-in-comment` warnings appeared on the first run, from `%build`,
`%install`, `%files` and `%check` written in the spec's own explanatory
comments. They were fixed by escaping (`%%`) and the spec was rebuilt; the
listing above is the clean result.

---

## 12. Files changed

No product code. Nothing outside `packaging/` except one `.gitignore` entry.

| File | Change |
| --- | --- |
| `packaging/fedora/omnibridge.spec` | **modified**, +122/−10 — B1, B2, B4, B3, P1; GUI installed; explicit build targets; `Release` 1→2 and a changelog entry |
| `packaging/fedora/cargo-vendor-config.toml` | **new**, 34 lines — the committed offline source replacement |
| `packaging/release/make-source-bundle.sh` | **new**, 322 lines — the source-bundle tool |
| `packaging/tests/packaging-checks.sh` | **new**, 272 lines — the packaging assertions |
| `packaging/fedora/README.md` | **modified**, +193/−5 — bundle generation, offline build, prerequisites, artifact locations, the unit's phase boundary |
| `.gitignore` | **modified**, +5 — ignore `/dist/` |

Deliberately **not** touched: `packaging/fedora/omnibridged.service` (§14),
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`, every `.rs`, `.kt` and `.proto`, every
workflow, and every historical certification report.

---

## 13. Validation performed

### Source validation, as the brief specifies

```console
$ cd desktop && cargo metadata --locked --no-deps --format-version 1 > /dev/null
OK
$ cargo fmt --all --check
OK (no diff)
```

### Packaging checks

`packaging/tests/packaging-checks.sh` — **50 passed, 0 failed** with a bundle
and the built RPM supplied. It covers exactly the list the brief asks for:
version synchronisation, MSRV synchronisation, each B1/B2/B3 fix still being
in place, the vendor config describing an offline-only source, vendor config
presence in the generated bundle, every built binary appearing in `%files`,
the required binaries appearing in the built package's manifest, forbidden
paths being absent from the bundle, no unexpanded macro reaching the package,
and a standing grep for any maintainer script growing a reference to a
user-state path (audit R6 — the one that would destroy a user's trust store).

No absolute path is baked into it. Every path is derived from the repository
root or passed as an argument.

One bug in it is worth recording, because it is the kind that makes a test
suite worse than useless. Two vendor-tarball checks were reporting `FAIL`
against a tarball that was in fact correct. The cause was
`printf … | grep -q`: `grep -q` exits on its first match, the writer gets
SIGPIPE, and under `set -o pipefail` the pipeline's status is 141, so the
`if` takes the failing branch. It only fires once the listing outgrows the
64 KiB pipe buffer, which is why the smaller source-bundle checks passed and
the 270-crate vendor listing did not — a check that passes on small inputs
and silently inverts on large ones. Every such site was rewritten to grep a
file instead of a pipe.

### `%check` inside the isolated buildroot

**65 suites, 997 assertions, 0 failures, 23 ignored.** This is the first time
the suite has ever run inside a package build; the audit MEASURED that no
`rpmbuild` existed on the development host.

Two of the audit's R1 predictions did **not** materialise: the daemon suite
binds TCP and the runtime suite touches mDNS, and both were fine with only
`lo` present.

### Two failures that were diagnosed rather than worked around

**Root cannot run the store tests.** The first build failed in
`omnibridge-core --lib`: `an_unreadable_secret_is_permission_denied_and_never_absence`
and `an_unreadable_state_file_is_permission_denied_and_never_absence`. Both
chmod a directory to `0000` and assert the read comes back `PermissionDenied`
rather than "no key at all" — the Wave 0 defect they exist to hold shut. Root
has `CAP_DAC_OVERRIDE` and reads the file anyway.

That was a defect in **my harness**, not in the package: `mock` and `koji`
both build as an unprivileged user. The container now adds a `builder` user
and both tests pass. It is written up in `packaging/fedora/README.md`, because
anyone hand-rolling `rpmbuild` as root will hit it.

**One flaky test, in product test code, left alone.**
`the_mid_session_convergence_path_logs_no_notification_content`
(`daemon/tests/notifications.rs:1942`) failed once with *"nothing was
captured, so this test proves nothing"* — its own guard against being
vacuous. It attaches a scoped `tracing` subscriber and asserts no notification
content reaches the log; the capture came back empty.

It did not reproduce. MEASURED, **18 consecutive passes**: 6 on the host
(release, full binary), 5 in the identical container image with
`--network=none`, 3 running it alone there, and 5 more under a 2-CPU quota to
test a timing hypothesis. It then passed again in the successful build. The
one failure coincided with heavy concurrent load from other work on this
machine.

It is **not** caused by anything in this sprint — no packaging change touches
it, and CI has never exercised it because `.github/workflows` runs
`cargo test --locked` in **debug**, while `%check` runs `--release`. Fixing it
means editing `.rs`, which §12 of the brief puts out of scope. Recorded as a
debt in §15 with the evidence, not silenced, and `%check` was not weakened to
hide it.

### Bundle determinism

Two independent runs, byte-identical (§4).

---

## 14. The systemd unit — deliberately untouched, and why

The brief's §8 is conditional on what the audit schedules. The audit schedules
this elsewhere, so the unit was left alone.

* Audit §17 **Phase 1** step 3: *"Move it to
  `packaging/common/omnibridged.service` — one unit, two formats."*
* Audit §18 **row 1**: branch `fix/systemd-user-unit-runtime-dir-v1`, Phase 1,
  closes **P2**, merge gate *"S1, S2, S3 measured and quoted in the PR."*
* Audit §18 **row 2**: branch `fix/fedora-spec-buildable-v1`, Phase 2, closes
  **P1, B1, B2, B3**, merge gate *"mock builds offline; SRPM attached."*

This sprint is row 2. The unit migration is row 1 — a different phase, a
different branch, and a different merge gate.

Three further reasons this is the right call rather than a technicality:

1. The brief itself says *"Do NOT claim S2/S3 certification yet."* Shipping
   the S1 decision without the S2/S3 measurements that are supposed to
   accompany it is exactly what row 1's merge gate exists to prevent.
2. The defects are not build-fatal. The unit is installed as data; the RPM
   builds and its file list is correct whatever the unit contains. Nothing in
   B1, B2 or B3 is blocked by it.
3. It keeps a packaging-build PR reviewable as a packaging build, instead of
   turning it into a systemd-hardening review.

The S1 outcome is recorded where the next branch will need it — in
`packaging/fedora/README.md`, alongside a plain statement that the unit as
shipped does **not** yet start on a fresh install, so nobody reads the
current file as certified. The settled decision is
`ReadWritePaths=%h/.local/share` (the parent, the fallback audit §7.2 reserved
for exactly the measured outcome), together with `RuntimeDirectory=omnibridge`,
`RuntimeDirectoryMode=0700`, dropping `StateDirectory=`, and
`ProtectSystem=strict` / `ProtectHome=read-only` untouched.

---

## 15. Debts deliberately deferred

| # | Debt | Why deferred | Where it belongs |
| --- | --- | --- | --- |
| 1 | The unit fix and its move to `packaging/common/` | §14 | Audit branch 1, Phase 1 |
| 2 | `omnibridge-gui` subpackage split | Phase 3 item 1; the binary ships in the main package meanwhile | Phase 3 |
| 3 | `.desktop`, hicolor icon, D-Bus service file | Phase 3 item 2, via `install-desktop-metadata.sh` which already takes `--prefix`/`--destdir` | Phase 3 |
| 4 | `%systemd_user_post` / `_preun` / `_postun` | Phase 3 item 3. `systemd-rpm-macros` is now a `BuildRequires`, so the macros are available when that branch starts | Phase 3 |
| 5 | firewalld service XML | Phase 3 item 4 | Phase 3 |
| 6 | `%doc` trimmed to `README.md`; `Suggests: wl-clipboard` | Phase 3 item 5 | Phase 3 |
| 7 | AppStream metainfo | Audit Q3 | Phase 3 |
| 8 | D-Bus activation self-heal (P4) | Product code; the brief forbids it in this branch | Branch 3b, only if Q1 = yes |
| 9 | DEB packaging | Out of scope by instruction | Phase 4 |
| 10 | CI artifact pipeline, `SHA256SUMS`, SBOM, provenance, GPG | Out of scope by instruction | Phase 5 |
| ~~11~~ | ~~**Re-run the build under real `mock`**~~ | **CLOSED 2026-09-22** — mock 6.8, `fedora-44-x86_64`, offline, `%check` passed | §17 |
| 12 | **The flaky convergence test** | Product `.rs`; out of scope by §12 of the brief | Needs an owner; see §13 |
| 13 | **CI runs tests in debug; `%check` runs release** | Discovered here, not fixed here | Phase 5 |
| 14 | No git tag exists, so the release path is unexercised | Tagging is a release decision | Phase 5 |
| 15 | Byte-identical *package* rebuild | Audit §13.5 calls it an aspiration; the *source bundle* is deterministic today | Later |

---

## 16. Next phase — recommendation

**Do audit branch 1 next, not branch 3.**

The plan lists branch 1 (the unit) before branch 2 (the build). Branch 2 ran
first here because it was the harder blocker and nothing downstream could
start without it. That inversion is now paid off, and branch 1 is both small
and already decided: S1 has been answered, the target unit is written out in
audit §7.1, and the only work left is applying it, moving it to
`packaging/common/`, and measuring S2 and S3.

Doing it before branch 3 also matters for sequencing: Phase 3 adds
`%systemd_user_post` and friends, and enabling lifecycle macros around a unit
that does not start on a fresh install would produce a package that installs
cleanly and then fails the first time a user runs the documented one-liner.

Two things to fold into whichever branch goes next:

1. ~~**Re-run the offline build under real `mock`**~~ — **done, §17.** mock
   6.8 rebuilt the SRPM in `fedora-44-x86_64` offline, `%check` passed, and
   branch 2's merge gate (audit §18 row 2, gate S4) is measured. What remains
   unexercised is the *tag-based* release path: the bundle was made with
   `--worktree` because the packaging change is uncommitted (§15 debt 14).
2. **Give the flaky test an owner** (§13, debt 12). It is one test in product
   code, it is now on the package build's critical path, and CI cannot see it
   because CI tests in debug.

Not recommended before those: D-Bus self-heal, DEB, firewall lifecycle,
certification, artifact CI.

---

## 17. REAL MOCK CLOSURE — 2026-09-22

The one claim §7 could not make. `mock` is now installed on this host, and the
same SRPM was rebuilt in Fedora's canonical buildroot. **It builds, and its
own `%check` passes there.**

Nothing in the spec, the release script, the vendor config or the checks was
changed to make this pass. The artifacts below came out of the implementation
exactly as §12 records it — the spec inside the SRPM hashes to
`b2308131dd6d236aec76fd0d7ee8ad29be98663d9c95d5ffafbc723e42c9ae24`, the same
file as `packaging/fedora/omnibridge.spec` in the working tree.

### 17.1 Environment

| Field | Value |
| --- | --- |
| mock | `6.8` — `mock-6.8-1.fc44.noarch` |
| configs | `mock-core-configs-45.1-1.fc44.noarch` |
| config used | `/etc/mock/fedora-44-x86_64.cfg` → `templates/fedora-branched.tpl` |
| root | `fedora-44-x86_64` (`/var/lib/mock/fedora-44-x86_64`) |
| chroot setup | `install @buildsys-build` (the distro default, untouched) |
| package manager | `dnf5` |
| bootstrap image | `registry.fedoraproject.org/fedora:44` |
| `rpmbuild_networking` | `False` (mock's default) |
| `use_host_resolv` | `False` (mock's default) |
| build user | `uid=1000(mockbuild) gid=135(mock)` |
| rpmlint | `2.8.0` — the same version the container run used |

**No custom buildroot was created and no configuration was relaxed.** MEASURED:

```console
$ rpm -V mock-core-configs | grep -E 'fedora-44|fedora-branched'
                                      # no output: the config is as shipped
$ rpm -V mock | grep site-defaults
                                      # no output: site-defaults.cfg as shipped
$ ls ~/.config/mock
ls: cannot access '/home/yuri/.config/mock': No such file or directory
```

The buildroot mock assembled is **489 packages**: 186 for the minimal
`@buildsys-build` group, then 303 more that `dnf5 builddep` resolved from the
SRPM itself —

```
['/usr/bin/dnf5', 'builddep', '--installroot', '/var/lib/mock/fedora-44-x86_64/root/',
 '--releasever', '44', '…/omnibridge-0.1.0-2.fc44.src.rpm', …]
```

— which is the authoritative form of the B1 question. Nothing was installed by
name, and nothing was installed by hand. All nine declared `BuildRequires`
resolved:

```
rust                   rust-1.98.1-1.fc44.x86_64
cargo                  cargo-1.98.1-1.fc44.x86_64
gcc                    gcc-16.2.1-2.fc44.x86_64
pkgconf-pkg-config     pkgconf-pkg-config-2.5.1-1.fc44.x86_64
gtk4-devel             gtk4-devel-4.22.5-2.fc44.x86_64
libadwaita-devel       libadwaita-devel-1.9.4-1.fc44.x86_64
glib2-devel            glib2-devel-2.88.3-1.fc44.x86_64
systemd-rpm-macros     systemd-rpm-macros-259.9-1.fc44.noarch
dbus-daemon            dbus-daemon-1:1.16.2-1.fc44.x86_64
```

### 17.2 Inputs

The bundle was generated from the **current worktree**, not from a tag:
`make-source-bundle.sh --worktree`, because the packaging implementation under
test is uncommitted and the brief forbids staging it. **This is not the
tag-based release path** and no claim is made that it is; the release path
(`--rev <tag>`) remains unexercised, as §15 debt 14 records.

| Artifact | SHA-256 | Size |
| --- | --- | --- |
| `omnibridge-0.1.0.tar.gz` (Source0) | `5e90d67189aec222fe04cc5623dda73ffeab24056c9adbdd8428783264afc439` | 2,403,076 B |
| `omnibridge-0.1.0-vendor.tar.xz` (Source1) | `e591169e50943a94926fd74b61f04a9091428a364c943dcf67e88fd55c3a5e27` | 28,440,100 B |
| `omnibridge-0.1.0-2.fc44.src.rpm` (input SRPM) | `8bfad39f916bf3c7368c68078bbc3ffa635b72af7d1436594fea174f9ab7b0e3` | 30,869,665 B |

The vendor tarball hash is the same `e591169e…` §4 recorded across four
generations: the locked dependency graph has not moved. The source tarball
hash tracks the packaging edits made since, exactly as that section predicts.

SRPM verified before it was handed to mock: `Version: 0.1.0`,
`Release: 2.fc44`, `Source0` and `Source1` both present and byte-identical to
the bundle, `packaging/fedora/cargo-vendor-config.toml` present inside
Source0, and the contained spec identical to the one in the tree.

### 17.3 Build

```bash
./packaging/release/make-source-bundle.sh --worktree --output "$OUT/dist"
tar -xzOf "$OUT/dist/omnibridge-0.1.0.tar.gz" \
    omnibridge-0.1.0/packaging/fedora/omnibridge.spec > "$OUT/rpmbuild/SPECS/omnibridge.spec"
rpmbuild --define "_topdir $OUT/rpmbuild" -bs "$OUT/rpmbuild/SPECS/omnibridge.spec"

mock -r fedora-44-x86_64 --resultdir="$OUT/mock-result" \
     --rebuild "$OUT/rpmbuild/SRPMS/omnibridge-0.1.0-2.fc44.src.rpm"
```

The spec is taken out of the source tarball rather than the checkout, so what
was built is what the bundle ships.

MEASURED:

```console
INFO: Done(…/omnibridge-0.1.0-2.fc44.src.rpm) Config(fedora-44-x86_64) 10 minutes 56 seconds
### mock exit: 0
### mock rebuild finished 2026-09-22T05:24:40Z
```

Inside it, `%prep` unpacked the vendored source and installed the committed
cargo config, and `%build` compiled all three binaries offline:

```
+ tar -xf /builddir/build/SOURCES/omnibridge-0.1.0-vendor.tar.xz -C desktop
+ install -Dpm0644 packaging/fedora/cargo-vendor-config.toml desktop/.cargo/config.toml
+ export CARGO_HOME=/builddir/build/BUILD/omnibridge-0.1.0-build/omnibridge-cargo-home
+ cargo build --release --locked --offline -p omnibridge-daemon -p omnibridge-cli -p omnibridge-gui
   Compiling omnibridge-gui v0.1.0 (…/desktop/gui)
   Compiling omnibridge-daemon v0.1.0 (…/desktop/daemon)
   Compiling omnibridge-cli v0.1.0 (…/desktop/cli)
    Finished `release` profile [optimized] target(s) in 4m 02s
…
Wrote: /builddir/build/RPMS/omnibridge-0.1.0-2.fc44.x86_64.rpm
```

`%install` expanded the B2 macro, as the file list below confirms:

```
+ install -Dpm0644 packaging/fedora/omnibridged.service \
    /builddir/build/BUILD/omnibridge-0.1.0-build/BUILDROOT/usr/lib/systemd/user/omnibridged.service
```

### 17.4 Offline evidence

Three independent lines, none of which required weakening `--offline`.

**1. The build log.** MEASURED over all 114,667 bytes of
`mock-result/build.log`:

```console
$ grep -icE 'downloading|updating crates\.io index|crates\.io index|failed to download' build.log
0
```

The only two lines in the whole log matching `dns` at all are the crate name
`mdns-sd v0.15.2` and the test `mdns_service_type_is_omnibridge_tcp`. There is
no `Downloading`, no index update and no fetch error, because `--offline`
makes a fetch impossible and the vendored `Source1` made it unnecessary.

**2. mock's own isolation.** `rpmbuild.py:109` passes
`unshare_net=not rpmbuild_networking` into the child, and `rpmbuild_networking`
is `False` by default (`mockbuild/config.py:98`); `util.py:723` calls
`condUnshareNet` → `unshare(CLONE_NEWNET)` in the pre-exec, before
`systemd-nspawn --resolv-conf=off` is entered. `%prep`, `%build`, `%install`
and `%check` all ran inside that empty namespace.

**3. A direct probe of that namespace.** `mock --chroot` takes the same
`unshare_net=self.private_network` path as the build (`backend.py:400-407`),
so what it sees is what `%build` saw. MEASURED:

```console
--- whoami ---
uid=1000(mockbuild) gid=135(mock) groups=135(mock)
--- interfaces (/proc/net/dev) ---
    lo:       0       0    0    0 …
--- /etc/resolv.conf ---
                                   (empty)
--- crates.io ---
no DNS resolution
--- default route ---
lo	00000000	0100007F …
```

`lo` is the only interface, `/etc/resolv.conf` is empty, and `crates.io` does
not resolve.

### 17.5 `%check`

Run in full inside mock, not skipped and not weakened:

```
+ cargo test --release --locked --offline
```

**65 suites, 997 assertions, 0 failures, 23 ignored** — identical to the
container result, aggregated from the 65 `test result:` lines in the build log.

Two specific results are worth naming:

* `the_mid_session_convergence_path_logs_no_notification_content` — the flaky
  test of §13 — **passed** (`build.log:1270`). No rerun was needed and none was
  performed: this is the first and only mock run, reported as it happened. That
  makes 19 consecutive passes on record against the single failure. It stays
  open as §15 debt 12; a test that failed once and has not reproduced is not
  the same thing as a test that is known good.
* `an_unreadable_secret_is_permission_denied_and_never_absence` and
  `an_unreadable_state_file_is_permission_denied_and_never_absence` **passed**,
  which is the independent confirmation that mock built as an unprivileged
  user. Under root's `CAP_DAC_OVERRIDE` they fail, which is the trap §13
  documents for anyone hand-rolling `rpmbuild`.

### 17.6 Artifacts

Result directory: `mock-result/` (kept outside the repository — nothing was
added to the working tree by this run).

| File | Arch | Size | SHA-256 |
| --- | --- | --- | --- |
| `omnibridge-0.1.0-2.fc44.x86_64.rpm` | `x86_64` | 4,450,011 B | `871bea848f2d78d91bb20e22c346ca567d74dab7df84d7a64a8e228432595922` |
| `omnibridge-0.1.0-2.fc44.src.rpm` (mock-rebuilt) | `src` | 30,868,873 B | `0050e6c366532e3c670860502144ecb1f8ab53c96a1ec0b08fdf4c12c1cceb9a` |

NEVRA: **`omnibridge-0.1.0-2.fc44.x86_64`**, no epoch, `License: Apache-2.0`,
`Buildhost: fedora`, installed size 17,020,620 B, unsigned.

The mock-rebuilt SRPM differs in bytes from the one fed to it — mock reruns
`rpmbuild -bs` inside the chroot — but its payload does not: Source0, Source1
and the spec inside it hash to `5e90d671…`, `e591169e…` and `b2308131…`, the
same three files that went in. The sources round-tripped untouched.

**The RPM was not installed on the host.**

### 17.7 Package manifest

`rpm -qpl omnibridge-0.1.0-2.fc44.x86_64.rpm` (116 paths under
`/usr/share/doc/omnibridge/docs/` elided):

```
/usr/bin/omnibridge
/usr/bin/omnibridge-gui
/usr/bin/omnibridged
/usr/lib/systemd/user/omnibridged.service
/usr/share/doc/omnibridge
/usr/share/doc/omnibridge/README.md
/usr/share/doc/omnibridge/docs
/usr/share/licenses/omnibridge
/usr/share/licenses/omnibridge/LICENSE
```

All four required paths are present: the three binaries and the user unit at
the expanded `%{_userunitdir}`. No path in the package is a literal
`%{_userunitdir}`.

### 17.8 rpmlint

Same rpmlint (2.8.0), run on the mock-produced artifacts. **The container
result reproduced exactly** — no new finding, nothing that mock exposed and
the container hid.

| Class | Finding | Verdict |
| --- | --- | --- |
| WARNING ×3 | `unstripped-binary-or-object` on each binary | **deferred debt** — the direct consequence of `%global debug_package %{nil}`, which audit Q2 says to keep and revisit |
| WARNING ×3 | `no-manual-page-for-binary` | **deferred debt** — man pages are a later phase |
| ERROR ×1 | `spelling-error ('systemd' → systems, system, system d)` | **false positive** — rpmlint's dictionary does not know the word; the text is correct and was not changed |
| WARNING ×2 | `invalid-url Source0 / Source1` | **false positive for now** — inherent to a locally generated tarball, as for every vendored Rust package in Fedora; it resolves when the tarballs are published against a tag |

Totals: binary `0 errors, 6 warnings, 3 filtered, 0 badness`; source
`1 errors, 2 warnings, 4 filtered, 1 badness`. Nothing correct was changed to
silence a dictionary.

### 17.9 Packaging checks

`packaging/tests/packaging-checks.sh --bundle <dist> --rpm <mock RPM>` —
**50 passed, 0 failed**, against the mock-produced package. No check was
weakened, skipped or adjusted for mock; the RPM group is the same one the
container artifact passed, pointed at a different file.

### 17.10 Were the container findings confirmed or contradicted?

**Confirmed, on every point that mattered, and the one honest gap §7 reported
is closed.**

| Claim from the container run | Under real mock |
| --- | --- |
| The declared `BuildRequires` are sufficient | **Confirmed, and more strongly.** The container's `fedora:44` image carried 738 packages; mock's canonical buildroot carried 489. The smaller, authoritative set built it. The gap §7 named — "a dependency the image happens to carry and the mock buildroot does not" — did not exist |
| `%{_userunitdir}` expands (B2) | Confirmed — `/usr/lib/systemd/user/omnibridged.service` in the package |
| The build is offline (B3) | Confirmed by three independent lines, §17.4 |
| `dbus-daemon` is needed and sufficient for `%check` (B4) | Confirmed — the tray suites passed |
| All three binaries ship (P7) | Confirmed — §17.7 |
| `%check`: 65 / 997 / 0 / 23 | Confirmed, exactly |
| The store tests need an unprivileged builder | Confirmed — mock builds as `mockbuild` and they passed |
| rpmlint: 6 warnings binary, 1 error + 2 warnings source | Confirmed, identical |

One new fact, not a contradiction: the two environments do **not** produce a
byte-identical package. Container 4,451,776 B / installed 17,008,332 B; mock
4,450,011 B / installed 17,020,620 B. Different buildroots, different build
paths, different `BUILDHOST`. That is §15 debt 15 — the *source bundle* is
deterministic today, the *package* is not, and audit §13.5 already files that
as an aspiration rather than a v1 gate.

### 17.11 What this closes

Audit §18 row 2's merge gate reads *"mock builds offline; SRPM attached"*, and
audit §14.1 gate **S4** reads *"`mock` builds the SRPM offline — build log; no
network access."* Both are now MEASURED rather than argued. Audit risk **R1**
(`%check` fails under mock because the daemon suite binds TCP and the runtime
suite touches mDNS in a network-isolated chroot) **did not materialise**: both
suites passed with only `lo` present, which the container run predicted and
mock has now confirmed in the environment the risk was written about.

---

# PACKAGING BUILD FOUNDATION: READY FOR COMMIT

B1, B2, B3 and P1 are closed, with a fourth build-fatal defect (B4,
`dbus-daemon` for `%check`) found and closed alongside them. The RPM builds
offline in an isolated buildroot, its full test suite passes there, and it
ships the GUI binary it used to discard.

The single limitation this report used to carry — **`mock` was not available
on this host** — is **closed, not reported**. On 2026-09-22, mock 6.8 rebuilt
the SRPM in Fedora's own `fedora-44-x86_64` buildroot, unmodified and
unrelaxed: 489 packages resolved from the spec's own `BuildRequires`, no
network for `%prep`/`%build`/`%install`/`%check`, no crate downloaded, and
**65 suites / 997 assertions / 0 failures / 23 ignored**. Every container
finding was confirmed there and none was contradicted. Audit §18 row 2's merge
gate and gate S4 are measured. **§17.**

What is still *not* claimed: the tag-based release path. The bundle under test
was built with `--worktree`, because the packaging implementation it proves is
uncommitted (§15 debt 14), and the package is not yet byte-reproducible across
environments (§15 debt 15).

No product code was changed.
