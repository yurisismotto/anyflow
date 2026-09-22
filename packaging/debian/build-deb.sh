#!/usr/bin/env bash
#
# Builds OmniBridge's .deb packages in a disposable container, offline.
#
#   ./build-deb.sh --image docker.io/library/debian:trixie dist
#   ./build-deb.sh --image docker.io/library/ubuntu:24.04 dist --output out/u24
#
# `dist` is a directory produced by packaging/release/make-source-bundle.sh.
#
# Why a container, and what it proves
# -----------------------------------
# The same three properties `mock` gives the RPM:
#
#   * the buildroot installs **nothing by name** — only what `apt-get build-dep`
#     derives from debian/control — so an under-declared Build-Depends fails
#     the build instead of being silently covered by the host's packages;
#   * `cargo` runs `--locked --offline` against a vendored tree with CARGO_HOME
#     inside the build directory, so a network fetch is impossible rather than
#     merely unnecessary;
#   * it builds as a normal user, never root, which is what `%check`'s
#     permission tests require — `omnibridge-core`'s store tests chmod a
#     directory to 0000 and assert the read comes back PermissionDenied, and
#     root has CAP_DAC_OVERRIDE and reads it anyway.
#
# The Rust floor
# --------------
# 1.88, and two of the three targets cannot meet it from their stock archive.
# This script resolves that per distribution, the same way debian/control
# declares it — see packaging/debian/README.source. Nothing here lowers the
# floor.

set -euo pipefail

IMAGE=""
BUNDLE=""
OUTPUT=""
SKIP_TESTS=0

die() { printf '\nbuild-deb: %s\n' "$*" >&2; exit 2; }

while [ $# -gt 0 ]; do
    case "$1" in
        --image)  IMAGE="${2:?--image needs a container image}"; shift 2 ;;
        --output) OUTPUT="${2:?--output needs a directory}"; shift 2 ;;
        --skip-tests) SKIP_TESTS=1; shift ;;
        -h|--help) sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) die "unknown argument: $1" ;;
        *) BUNDLE="$1"; shift ;;
    esac
done

[ -n "$IMAGE" ] || die "pass --image"
[ -n "$BUNDLE" ] || die "pass the bundle directory produced by make-source-bundle.sh"
[ -d "$BUNDLE" ] || die "not a directory: $BUNDLE"
command -v podman >/dev/null || die "podman is required"

ROOT="$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
BUNDLE="$(cd -- "$BUNDLE" && pwd)"
OUTPUT="${OUTPUT:-$ROOT/dist-deb}"
mkdir -p "$OUTPUT"

SRC_TARBALL="$(find "$BUNDLE" -maxdepth 1 -name 'omnibridge-*.tar.gz' ! -name '*vendor*' | head -1)"
VENDOR_TARBALL="$(find "$BUNDLE" -maxdepth 1 -name 'omnibridge-*-vendor.tar.xz' | head -1)"
[ -n "$SRC_TARBALL" ] || die "no source tarball in $BUNDLE"
[ -n "$VENDOR_TARBALL" ] || die "no vendor tarball in $BUNDLE"

VERSION="$(basename "$SRC_TARBALL" | sed 's/^omnibridge-//; s/\.tar\.gz$//')"

printf '\n==> %s, OmniBridge %s\n' "$IMAGE" "$VERSION"
printf '    source %s\n    vendor %s\n' "$(basename "$SRC_TARBALL")" "$(basename "$VENDOR_TARBALL")"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-deb.XXXXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/in"
cp -- "$SRC_TARBALL" "$VENDOR_TARBALL" "$STAGE/in/"
# The packaging tree travels separately from the bundle: it is what is being
# tested, and taking it from the checkout is what lets a change be built before
# it is committed.
cp -r -- "$ROOT/packaging/debian" "$STAGE/in/debian-tree"
rm -f "$STAGE/in/debian-tree/build-deb.sh"

cat > "$STAGE/in/build.sh" <<'INSIDE'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# Deliberately NOT named VERSION: `/etc/os-release` defines VERSION, and
# sourcing it below would silently overwrite it. It did, once, and the build
# went looking for `omnibridge-13 (trixie).tar.gz`.
OB_VERSION="$1"
SKIP_TESTS="$2"

step() { printf '\n--> %s\n' "$*"; }

step "Base tools"
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
    build-essential devscripts equivs lintian ca-certificates xz-utils >/dev/null

# --------------------------------------------------------------------------
# The Rust floor, per distribution. Nothing below lowers it; each branch makes
# a toolchain of at least 1.88 available and then gets out of the way.
# --------------------------------------------------------------------------
step "Rust toolchain"
# Read in a subshell so that only the one field wanted crosses back out.
DISTRO_ID="$(. /etc/os-release && printf '%s' "$ID")"
rust_ok() { command -v rustc >/dev/null 2>&1 && \
    dpkg --compare-versions "$(rustc --version | awk '{print $2}')" ge 1.88; }

if [ "$DISTRO_ID" = debian ]; then
    # trixie's stock rustc is 1.85.1. Backports carries 1.94.1.
    echo 'deb http://deb.debian.org/debian trixie-backports main' \
        > /etc/apt/sources.list.d/backports.list
    apt-get update -qq
    apt-get install -y -qq -t trixie-backports rustc cargo >/dev/null
else
    # Ubuntu: stock rustc where it is new enough, rustc-1.91 where it is not.
    apt-get install -y -qq rustc cargo >/dev/null || true
    if ! rust_ok; then
        apt-get install -y -qq rustc-1.91 cargo-1.91 >/dev/null
        export PATH="/usr/lib/rust-1.91/bin:$PATH"
    fi
fi
rustc --version
cargo --version
rust_ok || { echo "the available rustc is below the 1.88 floor"; exit 1; }

# --------------------------------------------------------------------------
step "Unpack the bundle"
# --------------------------------------------------------------------------
mkdir -p /build && cd /build
tar -xzf "/in/omnibridge-$OB_VERSION.tar.gz"
cd "omnibridge-$OB_VERSION"
# The vendored crates, and the config that points cargo at them. From here on
# every dependency resolves out of this tree.
tar -xf "/in/omnibridge-$OB_VERSION-vendor.tar.xz" -C desktop
install -Dm0644 packaging/common/cargo-vendor-config.toml desktop/.cargo/config.toml
# The packaging under test, not whatever the tarball happened to carry.
rm -rf debian
cp -r /in/debian-tree debian
chmod +x debian/rules

# --------------------------------------------------------------------------
step "Build dependencies, derived from debian/control and nothing else"
# --------------------------------------------------------------------------
# mk-build-deps builds a package whose dependencies are exactly control's
# Build-Depends, then installs it. Nothing is installed by name here, so an
# under-declared build dependency fails the build.
mk-build-deps --install --remove \
    --tool 'apt-get -o Debug::pkgProblemResolver=yes --no-install-recommends -y' \
    debian/control >/dev/null

# --------------------------------------------------------------------------
step "Build as a normal user"
# --------------------------------------------------------------------------
# %check's permission tests are meaningless as root: they chmod a directory to
# 0000 and assert PermissionDenied, and root has CAP_DAC_OVERRIDE.
# No fixed uid: Ubuntu 24.04 and later ship an `ubuntu` user already holding
# 1000, so `useradd -u 1000` fails there. It failed silently behind a
# `|| true`, and the next line then died with `chown: invalid user: builder`.
# Letting the system pick a free uid sidesteps the collision entirely, and the
# uid is of no consequence — what matters is only that the build is not root.
if ! id -u builder >/dev/null 2>&1; then
    useradd -m builder
fi
id builder
chown -R builder /build
if [ "$SKIP_TESTS" = 1 ]; then
    export DEB_BUILD_OPTIONS="nocheck"
fi
runuser_or_su() {
    if command -v runuser >/dev/null 2>&1; then runuser -u builder -- "$@"
    else su builder -c "$(printf '%q ' "$@")"; fi
}
runuser_or_su env \
    PATH="$PATH" \
    DEB_BUILD_OPTIONS="${DEB_BUILD_OPTIONS:-}" \
    dpkg-buildpackage -b -us -uc

# --------------------------------------------------------------------------
step "lintian"
# --------------------------------------------------------------------------
cd /build
# Informational rather than fatal: lintian's tag set is tuned for the Debian
# archive and this is a third-party package. Every tag it does emit is recorded
# in the report rather than suppressed here.
lintian --no-tag-display-limit ./*.changes 2>&1 | tee /out/lintian.txt || true

cp -- ./*.deb /out/ 2>/dev/null || true
ls -l /out/*.deb
INSIDE

mkdir -p "$OUTPUT"
podman run --rm \
    -v "$STAGE/in:/in:ro,z" \
    -v "$OUTPUT:/out:z" \
    "$IMAGE" bash /in/build.sh "$VERSION" "$SKIP_TESTS"

printf '\nPackages in %s\n' "$OUTPUT"
