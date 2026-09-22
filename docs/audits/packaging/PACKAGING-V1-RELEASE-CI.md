# OmniBridge — Packaging v1, release artifact CI

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-artifacts-ci-v1` |
| **Baseline** | `70dff64` (merge of PR #56, lifecycle certification) |
| **Date** | 2026-09-22 |
| **Workflow** | `.github/workflows/release-artifacts.yml` — CI-007 |
| **Evidence run** | [`35730680143`](https://github.com/yurisismotto/omnibridge/actions/runs/35730680143) — **7 of 7 jobs green**, artifacts downloaded and inspected |
| **Product code changed** | **No.** No `.rs`, `.kt` or `.proto` file was modified. |
| **Verdict** | **RELEASE PIPELINE WORKING — 14 artifacts, unsigned. Signing is a release gate, §7.** |

Claims are **MEASURED** (a command was run here and its output is quoted) or
**SOURCE-VERIFIED**.

---

## 0. Summary

Every Linux artifact OmniBridge ships is now produced by CI from **one
immutable git revision**, in network-isolated builds, on a machine that is not
a developer's workstation.

**MEASURED** — downloaded from the run and listed exactly as CI assembled it:

```
omnibridge-0.1.0.tar.gz                              2570.9 KiB
omnibridge-0.1.0-vendor.tar.xz                      27763.5 KiB
fedora44/omnibridge-0.1.0-3.fc44.src.rpm            30367.7 KiB
fedora44/omnibridge-0.1.0-3.fc44.x86_64.rpm          3590.3 KiB
fedora44/omnibridge-gui-0.1.0-3.fc44.x86_64.rpm       675.9 KiB
ubuntu2404/omnibridge_0.1.0-1_amd64.deb              4025.5 KiB
ubuntu2404/omnibridge-gui_0.1.0-1_amd64.deb           663.4 KiB
ubuntu2604/omnibridge_0.1.0-1_amd64.deb              3976.7 KiB
ubuntu2604/omnibridge-gui_0.1.0-1_amd64.deb           656.1 KiB
debian13/omnibridge_0.1.0-1_amd64.deb                3669.4 KiB
debian13/omnibridge-gui_0.1.0-1_amd64.deb             604.5 KiB
sbom/omnibridge-0.1.0-omnibridge_bin.cdx.json         200.1 KiB
sbom/omnibridge-0.1.0-omnibridged_bin.cdx.json        227.7 KiB
sbom/omnibridge-0.1.0-omnibridge-gui_bin.cdx.json     247.1 KiB
SHA256SUMS                                          14 entries
```

**No `v1.0.0` was published. No release tag was created.** Publishing is a
deliberate human act; this pipeline proves the artifacts can be produced, not
that they should be.

**Five defects were found by building and then inspecting.** Every one of them
exited 0 while doing nothing or losing something, and none was visible from a
green tick — §6.

---

## 1. The immutable input

`make-source-bundle.sh` has two modes and only one is a release path.
`--worktree` bundles whatever is lying in a checkout, **including uncommitted
and untracked files**, and its own output warns so. It exists to validate a
packaging change before it is committed.

The workflow never uses it. It always passes `--rev`, and always a **resolved
commit SHA** rather than the string it was given:

```yaml
rev="$(git rev-parse --verify "${requested}^{commit}")"
case "$rev" in
  [0-9a-f]*) ;;
  *) echo "::error::'$requested' did not resolve to a commit SHA"; exit 1 ;;
esac
```

A branch name moves; a SHA does not. Every downstream job builds from the
resolved value, so the six package artifacts are demonstrably from the same
source bytes — the bundle is built **once** and the RPM and three DEB jobs
consume that one artifact.

No tag is needed for this and none is created. When `v0.1.0` eventually
exists, `push: tags: ['v*']` builds from it; the mechanism does not change.

---

## 2. Offline, non-root, nothing installed by name

Both package builds now have a script, and the two are deliberately the same
shape so the formats are reasoned about identically:

| | RPM | DEB |
| --- | --- | --- |
| script | `packaging/fedora/build-rpm.sh` **(new)** | `packaging/debian/build-deb.sh` |
| buildroot derived from | `dnf builddep` over the spec | `mk-build-deps` over `debian/control` |
| network during the build | **`--network=none`** | `--locked --offline`, `CARGO_HOME` in the tree |
| build user | `builder`, never root | `builder`, never root |

`build-rpm.sh` exists because `mock` needs the `mock` group and a privileged
helper that a GitHub runner does not have. `mock` remains the right instrument
for a Fedora packager and `packaging/fedora/README.md` still says so — this is
the path that works in CI, not a replacement.

**Why non-root matters here and is not a detail:** `%check` runs the full
suite, and `omnibridge-core`'s store tests chmod a directory to `0000` and
assert the read comes back `PermissionDenied`. Root has `CAP_DAC_OVERRIDE` and
reads it anyway, so a root build would pass a test that proves nothing.

**MEASURED**, locally before the workflow was written: `build-rpm.sh` produced
an SRPM and two RPMs with `%check` reporting **1030 passed, 0 failed** inside
the buildroot, and `packaging-checks.sh --rpm --rpm` reporting **110 passed, 0
failed**.

---

## 3. SBOM

One CycloneDX document **per shipped binary**, via `--describe binaries`. That
is more useful than a single roll-up: a consumer asking *"what is in
`omnibridged`"* gets the daemon's closure without the GUI's GTK stack in it.

**MEASURED**, locally:

| SBOM | Format | Components |
| --- | --- | --- |
| `omnibridge_bin` (CLI) | CycloneDX 1.3 | 175 |
| `omnibridged_bin` (daemon) | CycloneDX 1.3 | 196 |
| `omnibridge-gui_bin` | CycloneDX 1.3 | 207 |

The daemon's SBOM names **`rustls 0.23.45`** — the version Security
Certification v1 moved to when it found RUSTSEC-2026-0285. That is a useful
cross-check that the SBOM describes the graph that actually shipped, and the
workflow asserts it: an SBOM naming a rustls below the fix **fails the job**.

The generation step also fails if `cargo-cyclonedx` writes nothing, if a
document has fewer than 50 components, or if no SBOM names rustls at all —
because a file of the right shape containing nothing is the failure mode worth
guarding against.

---

## 4. Checksums and the three-distribution problem

**MEASURED** — `SHA256SUMS`, 14 entries, paths rather than bare names:

```
a93a45585623972b…  debian13/omnibridge_0.1.0-1_amd64.deb
258b88976b176cb7…  ubuntu2404/omnibridge_0.1.0-1_amd64.deb
c3777c44f77af593…  ubuntu2604/omnibridge_0.1.0-1_amd64.deb
```

Three **different** binaries under one filename. All three targets build
`omnibridge_0.1.0-1_amd64.deb`, and they are not interchangeable — Ubuntu
26.04's does not even carry `init-system-helpers` in `Depends`, because its
debhelper version-gates that dependency differently.

The release layout keeps them in per-distribution subdirectories. The
canonical filename is **preserved rather than mangled**, because that is what
`dpkg -i` and an apt repository expect; the directory carries the distinction.

Two assertions stop this regressing:

* every artifact group is **counted** — 2 tarballs, 3 RPM, 2 DEB per
  distribution, 3 SBOMs — and a shortfall fails the job;
* the three same-named `.deb` sets must be **byte-different**, because
  identical bytes would mean one distribution's build had been reused for
  another.

---

## 5. Provenance

**MEASURED**, `gh attestation verify` against a downloaded RPM, exit 0:

```
verified   : https://slsa.dev/provenance/v1
workflow   : …/.github/workflows/release-artifacts.yml@refs/pull/57/merge
sourceRepo : https://github.com/yurisismotto/omnibridge
```

`actions/attest-build-provenance` records **what was built, from which commit,
by which workflow**, backed by Sigstore's transparency log and GitHub's OIDC
identity. It needs **no secret of ours**, which is exactly why it is usable
here — and exactly why it is not a substitute for a maintainer's signature
(§7).

---

## 6. Five defects found by building it, and by inspecting what it built

None was visible from a green tick. Four of them **exited 0 while doing
nothing**, which by this point in Packaging v1 is a recognisable class rather
than a coincidence.

### 6.1 `podman run` without `-i` does not attach stdin

`bash -s < script` read nothing, exited 0, and `podman commit` captured a
pristine image. It surfaced two steps later as `unable to find user builder` —
a long way from the cause. The prepare stage now ends with `id builder` and
`rpmbuild --version`, so it **proves it ran**.

### 6.2 A rootless subuid cannot write to a bind mount

The RPM build succeeded, wrote its packages, and then:

```
cp: cannot create regular file '/out/omnibridge-0.1.0-3.fc44.x86_64.rpm': Permission denied
```

Under rootless podman the build user maps to a subuid with no write access to
a host directory. Artifacts are extracted with `podman cp` instead, and
producing zero `.rpm` is now an explicit failure rather than an empty
directory.

### 6.3 A relative `--output` is a named volume, not a bind mount

The worst of the five, because everything was green. All three `.deb` builds
succeeded and **listed the packages they had just written**:

```
-rw-r--r-- 1 root root 3757480 /out/omnibridge_0.1.0-1_amd64.deb
…
##[error]No files were found with the provided path: out/*.deb
```

`podman -v` treats a **non-absolute** source as a named volume. The workflow
passed `--output out`, so every artifact went into a podman volume called
`out` and the workspace stayed empty. It never appeared locally because every
local invocation happened to pass an absolute path.

Both scripts now resolve `--output` with `cd … && pwd` before it reaches
podman. Verified by reproducing the exact failing case — `--output relout` —
which now lands two `.deb` files in the workspace.

### 6.4 Six packages built, one shipped

Found only by **downloading the artifacts**, which is why the brief asks for
that rather than for a green tick. The first fully-green run assembled a
release directory holding **one** `.deb` set where six had been built, and a
`SHA256SUMS` listing **10 files where 14 existed**. §4 is the fix.

### 6.5 `cargo cyclonedx` does not accept `--locked`

The flag does not exist on it. Found by running the tool locally before wiring
it in, rather than discovering it in CI.

### 6.6 And one that was not a silent failure

The very first run failed loudly and correctly:

```
error: no matching package named `prost` found
note: offline mode (via `--offline`) can sometimes cause surprising resolution failures
```

`make-source-bundle.sh` opens by checking the lockfile with
`cargo metadata --locked --offline`, which needs the crates already in the
local cargo cache. On a developer's machine they are; on a fresh runner they
are not.

The fix is `cargo fetch --locked` in the workflow, **not** dropping
`--offline` from the script: that flag is what makes the check prove the
*vendored* build will resolve, and weakening it to make CI green would have
removed the guarantee the check exists for. The script's own error message now
points at `cargo fetch`, because "no matching package named prost" on a clean
checkout is a long way from the actual problem.

---

## 7. Signing — a release gate, not a gap to paper over

**`SHA256SUMS` is NOT GPG-signed.**

Signing needs a private key. This repository has none, and inventing one for a
CI run would produce a signature that means nothing — worse than no signature,
because it looks like assurance. The brief was explicit: *do not invent signing
secrets; if artifact signing requires credentials, record as an RC/release
gate.*

| Gate | Status | What it needs |
| --- | --- | --- |
| **RC-SIGN-01** — a maintainer GPG key exists and its public half is published | **OPEN** | a key, and somewhere to publish the fingerprint |
| **RC-SIGN-02** — `SHA256SUMS` is signed with it in CI | **OPEN** | the private half as a repository secret, or an external signing service |
| **RC-SIGN-03** — the README documents how a user verifies a download | **OPEN** | RC-SIGN-01 first |

Audit §19 **Q6** already records this as *"calendar time, not engineering
time — start now"*, and that remains the right characterisation: generating a
key and deciding where its fingerprint lives are decisions, not work.

What exists in the meantime is real but different in kind: build provenance
(§5) proves **which workflow built these bytes from which commit**. It does
not prove a human vouched for the release.

---

## 8. What this branch did not do

| # | Not done | Why |
| --- | --- | --- |
| 1 | Publish `v1.0.0`, create a release tag, or open a GitHub Release | Explicitly out of scope, and a human decision besides |
| 2 | Sign anything | §7 |
| 3 | Build a Debian **source** package (`.dsc` + `.orig.tar.gz`) | `dpkg-buildpackage -b` builds binaries only. `debian/source/format` is `3.0 (quilt)` per audit Q5; a source package needs the vendor tree inside the orig tarball and is a separate decision |
| 4 | Reproducible *package* builds | The **source bundle** is byte-reproducible for a given revision and is asserted so. Byte-identical RPM/DEB rebuilds are audit §13.5's stated aspiration, not a v1 requirement |
| 5 | `aarch64` or any second architecture | Audit §13.9. The pipeline is a matrix and adding a row is mechanical, but nothing has certified OmniBridge on another architecture |
| 6 | Publish the SBOM anywhere, or wire it into a vulnerability feed | It is produced and attested; where it goes is a distribution decision |

---

## 9. Verdict

**RELEASE PIPELINE WORKING.**

Fourteen artifacts, built by CI from one immutable commit, in builds with no
network, by a non-root user, in buildroots derived from the packaging metadata
and nothing else. Checksummed, attested with SLSA provenance, and verified by
downloading them and re-running the packaging assertions against what actually
came out.

The pipeline found five defects in itself before it produced anything
trustworthy, four of which reported success while losing or doing nothing.
Each is now either impossible or loud.

**Nothing is signed**, and that is the one substantive thing between this and
a publishable release. It is recorded as three open RC gates with what each
needs, not as a footnote.
