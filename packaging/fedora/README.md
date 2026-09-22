# Fedora packaging

## systemd user unit

The unit is **not** here. It is `packaging/common/omnibridged.service`, one
file that the RPM and the Debian packaging both install, so a hardening change
cannot land on one distribution and miss the other. `packaging/common/README.md`
explains what it does and why each directive is there; `%install` copies it
into `%{_userunitdir}`.

The RPM ships it **disabled**. `systemctl --global preset` consults
`/usr/lib/systemd/user-preset/`, Fedora 44 ships no line naming
`omnibridged.service`, so the preset leaves it off — which is the intended
outcome, not an accident (audit §4.3). The user turns it on:

```bash
systemctl --user enable --now omnibridged.service
```

## RPM

`omnibridge.spec` builds `omnibridged`, `omnibridge` and `omnibridge-gui`,
installs all three and the user unit. `%check` runs the full test suite as
part of the build, so a package that fails its own security tests does not
get built.

It does **not** need `protobuf-compiler` — the build compiles the schema with
`protox` in pure Rust (ADR-0004).

### The package is built from a source bundle, not from a checkout

The spec takes two sources:

| Source | File | What it is |
| --- | --- | --- |
| `Source0` | `omnibridge-<version>.tar.gz` | the upstream source |
| `Source1` | `omnibridge-<version>-vendor.tar.xz` | every crate in `desktop/Cargo.lock`, vendored |

The second one exists because `mock`, `koji` and Debian `buildd` all build
with networking switched off, and `cargo build --locked` reads the committed
lockfile but still *downloads* all 270 crates. Without a vendored tarball no
official package can be produced at all — that was defect B3 of
`docs/audits/packaging/PACKAGING-V1-READINESS-AUDIT.md` §5.1.

`%prep` unpacks the vendor tarball into `desktop/vendor` and copies
`cargo-vendor-config.toml` to `desktop/.cargo/config.toml`, which replaces
the `crates-io` source with that directory. `%build` and `%check` then run
`cargo --locked --offline` with `CARGO_HOME` redirected into the build tree,
so no `~/.cargo/config.toml` belonging to whoever runs `rpmbuild` can quietly
put a registry back.

The vendor tree is **not** committed. 343 MB of third-party source does not
belong in the repository; it is generated at release time from the lockfile,
which is what is committed and what is authoritative.

### Generating the bundle

```bash
# the release path: pinned to a tag, cannot pick up an untracked file
./packaging/release/make-source-bundle.sh --rev v0.1.0

# validating a packaging change before it is committed
./packaging/release/make-source-bundle.sh --worktree
```

Artifacts land in `dist/` unless `--output` says otherwise:

```
dist/omnibridge-0.1.0.tar.gz            Source0
dist/omnibridge-0.1.0-vendor.tar.xz     Source1
dist/omnibridge-0.1.0-SOURCES.sha256    checksums over both
```

`dist/` is a build output. Do not commit it.

The script refuses to produce a bundle when `Cargo.lock` no longer resolves
`--locked`, when vendoring fails, when the version cannot be read, when
`desktop/Cargo.toml` and the spec's `Version:` disagree, or when
`cargo vendor` emits a source configuration that
`cargo-vendor-config.toml` does not describe — which is what adding a
dependency outside crates.io would do, and which would silently make the
package need a network again.

Both tarballs are deterministic: same revision in, same bytes out. That needs
a fixed `SOURCE_DATE_EPOCH` (taken from the archived commit), `tar
--sort=name` with uid/gid 0, `gzip -n`, and an explicit `xz --block-size`,
without which xz output depends on how many cores the release host has.

### Version: one source

`desktop/Cargo.toml` `[workspace.package] version` is authoritative. rpm
cannot read a TOML file at spec-parse time, and an SRPM does not carry
`Cargo.toml` when `Version:` is needed, so the spec's copy stays literal — but
it is not unguarded. Both `make-source-bundle.sh` and
`packaging/tests/packaging-checks.sh` refuse when the two disagree, the
second without building anything, so CI can gate on it the way
`linux-distro-compat.yml` already gates the MSRV.

### Build prerequisites

`BuildRequires` is the list, and it is the list a builder should install from
rather than a prose copy that can drift:

```bash
sudo dnf builddep packaging/fedora/omnibridge.spec
```

Three groups, each there for a reason the spec names inline: the Rust
toolchain at the measured MSRV (**1.88**, from `desktop/Cargo.toml`);
`systemd-rpm-macros`, without which `%{_userunitdir}` does not expand and
`%files` fails on a directory called `%{_userunitdir}`; and the GTK 4 /
libadwaita development packages, without which `omnibridge-gui` does not
compile — it is a workspace member, so it was always being built, just never
with its dependencies declared.

### Building offline

```bash
./packaging/release/make-source-bundle.sh --rev v0.1.0 --output ~/rpmbuild/SOURCES
cp packaging/fedora/omnibridge.spec ~/rpmbuild/SPECS/
rpmbuild -bs ~/rpmbuild/SPECS/omnibridge.spec
mock -r fedora-44-x86_64 --rebuild ~/rpmbuild/SRPMS/omnibridge-0.1.0-2.fc44.src.rpm
```

`mock` disables networking during `%build` by default, which is the point:
if the package builds there it builds on `koji`.

**Do not build this package as root.** `%check` will fail if you do, and the
failure is real rather than an artefact: `omnibridge-core`'s store tests chmod
a directory to `0000` and assert the read comes back `PermissionDenied` rather
than "no key at all", which is the Wave 0 defect they exist to hold shut. Root
has `CAP_DAC_OVERRIDE` and reads the file anyway. `mock` and `koji` both build
as an unprivileged user, so this only bites a hand-rolled `rpmbuild` as root —
which Fedora tells you not to do for other reasons too.

Where `mock` is not available, a container is the equivalent and is how the
offline build was proved for this branch. Three things in it matter: the
build runs with `--network=none`, so a Cargo fetch cannot merely be slow, it
is impossible; the buildroot installs nothing by name, only what
`dnf builddep` derives from the spec, so an under-declared `BuildRequires`
fails the build instead of being silently covered by the host's own packages;
and it builds as a normal user, like mock.

```bash
podman build -t omnibridge-buildroot:f44 -f - . <<'CONTAINERFILE'
FROM registry.fedoraproject.org/fedora:44
RUN dnf -y install rpm-build rpmlint dnf-plugins-core && dnf clean all
COPY packaging/fedora/omnibridge.spec /tmp/omnibridge.spec
RUN dnf -y builddep /tmp/omnibridge.spec && dnf clean all
RUN useradd -m -u 1000 builder && mkdir -p /build /out && chown builder /build /out
USER builder
CONTAINERFILE

podman run --rm --network=none \
    -v "$PWD/dist":/sources:ro,z -v "$PWD/out":/out:z \
    omnibridge-buildroot:f44 bash -c '
        mkdir -p /build/{SOURCES,SPECS,RPMS,SRPMS,BUILD,BUILDROOT}
        cp /sources/*.tar.* /build/SOURCES/
        tar -xzOf /build/SOURCES/omnibridge-*.tar.gz \
            "omnibridge-*/packaging/fedora/omnibridge.spec" > /build/SPECS/omnibridge.spec
        rpmbuild --define "_topdir /build" -ba /build/SPECS/omnibridge.spec
        find /build/RPMS /build/SRPMS -name "*.rpm" -exec cp {} /out/ \;'
```

### Checking the packaging

```bash
./packaging/tests/packaging-checks.sh                      # static
./packaging/tests/packaging-checks.sh --bundle dist        # ...and the bundle
./packaging/tests/packaging-checks.sh --rpm out/omnibridge-0.1.0-2.fc44.x86_64.rpm
```

Cheap assertions over the things this tree has already been observed to get
wrong: version and MSRV drift between the spec and the workspace, a GUI build
dependency going missing again, `%{_userunitdir}` being hardcoded to dodge the
macro, `--offline`/`--locked` being dropped, a binary that is built and then
not shipped, a forbidden path reaching the source bundle, and any maintainer
script growing a reference to a user-state path.

### Not done yet

This is the build foundation, not the finished package. Still open, and
scheduled: the `omnibridge-gui` subpackage split, the `.desktop` entry, the
hicolor icon and D-Bus activation file (all Phase 3, via
`desktop/gui/tools/install-desktop-metadata.sh`, which already takes
`--prefix` and `--destdir`); `%systemd_user_post`/`_preun`/`_postun`;
firewalld metadata; and trimming `%doc` down from the whole of `docs/`.
The unit file's own defects are **closed**: it moved to `packaging/common/`
and gates S1, S2 and S3 passed against it —
`docs/audits/packaging/PACKAGING-V1-SYSTEMD-UNIT.md`.
