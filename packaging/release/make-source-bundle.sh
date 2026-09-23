#!/usr/bin/env bash
#
# Produces the source bundle an official OmniBridge package is built from.
#
# ---------------------------------------------------------------------------
# Why this exists
# ---------------------------------------------------------------------------
#
# `packaging/fedora/omnibridge.spec` declares `Source0: %{name}-%{version}.tar.gz`
# and nothing in the repository ever produced that file. Worse, the spec's
# `%build` ran plain `cargo build --locked`, which reads the committed
# lockfile but still *downloads* all 270 crates from crates.io — and `mock`,
# `koji` and Debian `buildd` all build with networking switched off. No
# official package could be produced at all. That is defect B3 of
# docs/audits/packaging/PACKAGING-V1-READINESS-AUDIT.md §5.1.
#
# This script closes it by emitting the two tarballs the spec now consumes:
#
#   omnibridge-<V>.tar.gz          Source0 — the upstream source
#   omnibridge-<V>-vendor.tar.xz   Source1 — every locked crate, vendored
#
# They are separate on purpose rather than one combined archive. Source0 stays
# a pristine upstream tarball, which is what an RPM expects, what a Debian
# `3.0 (quilt)` `.orig.tar.gz` has to be (audit §19 Q5), and what lets the
# vendor tarball be regenerated for a security rebuild without reissuing the
# source. Together they are the bundle; neither is useful alone.
#
# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------
#
#   ./make-source-bundle.sh                     # from HEAD, into ./dist
#   ./make-source-bundle.sh --rev v0.1.0        # from a tag — the release path
#   ./make-source-bundle.sh --output /tmp/out
#   ./make-source-bundle.sh --worktree          # from uncommitted work; see below
#
# It writes only inside --output and a scratch directory it owns, needs no
# root, and never touches the checkout.

set -euo pipefail

# --------------------------------------------------------------------------
# Paths the bundle must never contain.
#
# Build outputs, local user state, private key material and the repository's
# own history. Matched against the path relative to the bundle root, so
# `desktop/target` matches the tree and not a crate called `target`.
#
# `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` used to be listed here. That entry was
# never about the document's contents: U2 was an *untracked* file lying at the
# repository root, and `--worktree` mode would have swept it into a bundle. It
# is now committed as
# `docs/audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`, so it ships like
# every other versioned report and the special case is obsolete. Do not re-add
# it under the new path: that would drop a tracked file out of the release
# tarball, which is the opposite of what this list is for.
#
# `protocol/testdata/*.der` is deliberately NOT here either. Those two files
# are X.509 *certificates* — public, no private half — used as cross-language
# test vectors by both `desktop/core/tests/identity_and_store.rs` and the
# Android unit tests. `%check` fails without them.
# --------------------------------------------------------------------------
readonly -a FORBIDDEN_GLOBS=(
    '.git' '.git/*'
    'desktop/target' 'desktop/target/*'
    'android/build' 'android/build/*'
    'android/*/build' 'android/*/build/*'
    'android/.gradle' 'android/.gradle/*'
    'android/.kotlin' 'android/.kotlin/*'
    'android/local.properties'
    '*.apk' '*.aab'
    '*.key' '*.pem' '*.p12' '*.pfx' '*.jks' '*.keystore'
    'id_rsa*' 'id_ed25519*'
    'state.json' 'trust-store.json'
    '.vscode' '.vscode/*' '*.swp' '*.rs.bk'
)

die() { printf '%s: error: %s\n' "${0##*/}" "$*" >&2; exit 1; }
note() { printf '  %s\n' "$*" >&2; }
step() { printf '\n==> %s\n' "$*" >&2; }

REV="HEAD"
OUTPUT=""
MODE="git"
KEEP_VENDOR=0

while [ $# -gt 0 ]; do
    case "$1" in
        --rev) REV="${2:?--rev needs a revision}"; shift 2 ;;
        --output) OUTPUT="${2:?--output needs a directory}"; shift 2 ;;
        --worktree) MODE="worktree"; shift ;;
        --keep-vendor) KEEP_VENDOR=1; shift ;;
        -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

command -v git >/dev/null || die "git is required"
command -v cargo >/dev/null || die "cargo is required"
command -v tar >/dev/null || die "tar is required"
command -v xz >/dev/null || die "xz is required"

ROOT="$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
SPEC="$ROOT/packaging/fedora/omnibridge.spec"
VENDOR_CONFIG="$ROOT/packaging/common/cargo-vendor-config.toml"
OUTPUT="${OUTPUT:-$ROOT/dist}"

# --------------------------------------------------------------------------
# 1. Version — one source of truth.
#
# `desktop/Cargo.toml` `[workspace.package] version` is authoritative; the
# spec's `Version:` is a copy that has to agree. rpm cannot read a TOML file
# at spec-parse time (and an SRPM does not carry `Cargo.toml` at that point
# anyway), so the copy stays literal and is *asserted* here rather than
# generated. Audit §13.1. The same assertion runs in
# `packaging/tests/packaging-checks.sh` so CI catches the drift without
# building a bundle.
# --------------------------------------------------------------------------
step "Resolving version"

WORKSPACE_VERSION="$(
    awk '
        /^\[workspace\.package\]/ { in_section = 1; next }
        /^\[/                     { in_section = 0 }
        in_section && /^[[:space:]]*version[[:space:]]*=/ {
            gsub(/.*=[[:space:]]*"|".*/, ""); print; exit
        }
    ' "$ROOT/desktop/Cargo.toml"
)"
[ -n "$WORKSPACE_VERSION" ] || die \
    "could not read [workspace.package] version from desktop/Cargo.toml"

SPEC_VERSION="$(awk '/^Version:/ { print $2; exit }' "$SPEC")"
[ -n "$SPEC_VERSION" ] || die "could not read Version: from $SPEC"

if [ "$WORKSPACE_VERSION" != "$SPEC_VERSION" ]; then
    die "version drift: desktop/Cargo.toml says '$WORKSPACE_VERSION', \
$SPEC says '$SPEC_VERSION'. desktop/Cargo.toml is authoritative — fix the spec."
fi

V="$WORKSPACE_VERSION"
PREFIX="omnibridge-$V"
note "version $V (workspace and spec agree)"

# --------------------------------------------------------------------------
# 2. Determinism anchor.
#
# SOURCE_DATE_EPOCH comes from the archived commit's committer date, so two
# runs over the same revision produce byte-identical tarballs. An externally
# set value wins, which is what a reproducible-build harness expects.
# --------------------------------------------------------------------------
if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
    SOURCE_DATE_EPOCH="$(git -C "$ROOT" log -1 --format=%ct "$REV")" \
        || die "cannot resolve a commit date for '$REV'"
fi
export SOURCE_DATE_EPOCH
note "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"

# --------------------------------------------------------------------------
# 3. Lockfile consistency.
#
# `--locked` at build time fails late and inside a buildroot. Failing here
# instead means a lockfile that no longer matches the manifests can never
# reach a bundle. `--offline` is set too: if this succeeds without a network
# the vendored build will as well.
# --------------------------------------------------------------------------
step "Checking Cargo.lock"
( cd "$ROOT/desktop" && cargo metadata --locked --offline --format-version 1 >/dev/null ) \
    || die "Cargo.lock is inconsistent with the workspace manifests, OR the \
crates it names are not in this machine's cargo cache. On a clean checkout or \
a fresh CI runner, run 'cargo fetch --locked' in desktop/ first. If that does \
not fix it the lockfile really is stale: run 'cargo update --workspace \
--offline' or commit the lockfile change. A package build cannot resolve \
either of these for you."
note "Cargo.lock resolves --locked --offline"

# --------------------------------------------------------------------------
# 4. Stage the source tree.
# --------------------------------------------------------------------------
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-bundle.XXXXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
STAGE="$SCRATCH/$PREFIX"
mkdir -p "$STAGE"

step "Staging source ($MODE mode, rev $REV)"

if [ "$MODE" = "git" ]; then
    # Tag- or commit-pinned and it cannot pick up an untracked file, a build
    # output or a developer's local state, because git does not know about
    # any of them. This is the release path.
    git -C "$ROOT" archive --format=tar "$REV" | tar -x -C "$STAGE"
    note "archived $(git -C "$ROOT" rev-parse --short "$REV") ($(git -C "$ROOT" ls-tree -r --name-only "$REV" | wc -l) files)"
else
    # Validation path: bundles the working tree, including uncommitted edits
    # and files not yet added, so a packaging change can be proved to build
    # before it is committed. NOT the release path — it trusts whatever
    # happens to be lying in the checkout, which is exactly what
    # FORBIDDEN_GLOBS below is there to bound.
    note "WARNING: --worktree bundles uncommitted and untracked files."
    note "         Official artifacts must be built with --rev <tag>."
    local_count=0
    while IFS= read -r -d '' f; do
        mkdir -p "$STAGE/$(dirname -- "$f")"
        cp -p --no-dereference -- "$ROOT/$f" "$STAGE/$f"
        local_count=$((local_count + 1))
    done < <(git -C "$ROOT" ls-files -z --cached --others --exclude-standard)
    note "copied $local_count tracked and untracked files from the working tree"
fi

# --------------------------------------------------------------------------
# 5. Enforce the exclusion list.
#
# Applied to whatever was staged, in both modes, so the guarantee does not
# depend on which one ran. Anything matched is removed and then the tree is
# re-checked; a survivor is a hard failure rather than a warning.
# --------------------------------------------------------------------------
step "Enforcing exclusions"
removed=0
for glob in "${FORBIDDEN_GLOBS[@]}"; do
    while IFS= read -r -d '' victim; do
        rm -rf -- "$victim"
        note "removed ${victim#$STAGE/}"
        removed=$((removed + 1))
    done < <(find "$STAGE" -path "$STAGE/$glob" -print0 2>/dev/null)
done
[ "$removed" -eq 0 ] && note "nothing to remove"

survivors=""
for glob in "${FORBIDDEN_GLOBS[@]}"; do
    found="$(find "$STAGE" -path "$STAGE/$glob" -print 2>/dev/null || true)"
    [ -n "$found" ] && survivors="$survivors$found"$'\n'
done
[ -z "$survivors" ] || die "forbidden paths survived staging:"$'\n'"$survivors"

# The build reads the canonical app icon out of `docs/design/assets`
# (`desktop/gui/build.rs` copies it into OUT_DIR as the icon-theme name), so a
# bundle that trimmed `docs/` to save space would compile until it did not.
for required in \
    desktop/Cargo.toml \
    desktop/Cargo.lock \
    packaging/fedora/omnibridge.spec \
    packaging/common/omnibridged.service \
    packaging/common/cargo-vendor-config.toml \
    packaging/fedora/omnibridge-firewalld.xml \
    protocol/proto \
    docs/design/assets/omnibridge-app-icon.svg \
    desktop/gui/tools/install-desktop-metadata.sh \
    desktop/gui/data/io.github.yurisismotto.omnibridge.desktop \
    desktop/gui/data/io.github.yurisismotto.omnibridge.service.in \
    desktop/gui/data/io.github.yurisismotto.omnibridge.metainfo.xml \
    LICENSE
do
    [ -e "$STAGE/$required" ] || die "the bundle is missing $required"
done
[ -x "$STAGE/desktop/gui/tools/install-desktop-metadata.sh" ] || die \
    "desktop/gui/tools/install-desktop-metadata.sh is not executable in the bundle; \
%install runs it directly and rpmbuild would fail with Permission denied"
note "required build inputs present"

# --------------------------------------------------------------------------
# 6. Vendor the locked dependency graph.
# --------------------------------------------------------------------------
step "Vendoring crates"
VENDOR_DIR="$SCRATCH/vendor"
EMITTED="$SCRATCH/config.emitted.toml"

( cd "$STAGE/desktop" && cargo vendor --locked --versioned-dirs "$VENDOR_DIR" ) \
    > "$EMITTED" 2> "$SCRATCH/vendor.log" \
    || { sed 's/^/    /' "$SCRATCH/vendor.log" >&2; die "cargo vendor failed"; }

crate_count="$(find "$VENDOR_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l)"
[ "$crate_count" -gt 0 ] || die "cargo vendor produced an empty tree"
note "vendored $crate_count crates"

# `cargo vendor` writes the absolute path it was given. Normalise it to the
# relative form the committed config uses, then insist the rest matches
# exactly — see the header of packaging/common/cargo-vendor-config.toml for
# why this comparison is the guard and not a formality.
NORMALISED="$SCRATCH/config.normalised.toml"
sed "s|^directory = \".*\"$|directory = \"vendor\"|" "$EMITTED" > "$NORMALISED"
COMMITTED_BODY="$SCRATCH/config.committed.toml"
grep -v '^[[:space:]]*#' "$VENDOR_CONFIG" | grep -v '^[[:space:]]*$' > "$COMMITTED_BODY"
grep -v '^[[:space:]]*$' "$NORMALISED" > "$NORMALISED.trimmed"

if ! diff -u "$COMMITTED_BODY" "$NORMALISED.trimmed" >"$SCRATCH/config.diff" 2>&1; then
    sed 's/^/    /' "$SCRATCH/config.diff" >&2
    die "cargo vendor emitted a source configuration that \
packaging/common/cargo-vendor-config.toml does not describe. A dependency \
outside crates.io (a git or path source) was probably added — the offline \
build cannot resolve it. Update the committed config deliberately."
fi
note "emitted source config matches packaging/common/cargo-vendor-config.toml"

# --------------------------------------------------------------------------
# 7. Write the tarballs.
#
# --sort=name, a fixed mtime, uid/gid 0 and a pinned format are what make two
# runs over one revision byte-identical. gzip -n keeps the container's own
# timestamp out of the .gz header.
# --------------------------------------------------------------------------
step "Writing tarballs"
mkdir -p "$OUTPUT"
SRC_TARBALL="$OUTPUT/$PREFIX.tar.gz"
VENDOR_TARBALL="$OUTPUT/$PREFIX-vendor.tar.xz"

tar_deterministic() {
    tar --format=gnu --sort=name \
        --mtime="@$SOURCE_DATE_EPOCH" \
        --owner=0 --group=0 --numeric-owner \
        --mode='go-w' \
        "$@"
}

# gzip -n keeps the build host's clock and the input filename out of the .gz
# header, which is otherwise the one byte range that changes between runs.
tar_deterministic -cf - -C "$SCRATCH" "$PREFIX" | gzip -9n > "$SRC_TARBALL"

# --block-size is what makes xz output independent of how many cores the
# release host has. Measured: without it, -T0 on a 16-core machine and -T1 on
# one core produce different bytes from identical input. With it, -T2, -T3,
# -T8 and -T0 all agree. -T1 still does not, because a single thread takes
# xz's non-blocked encoder path, so the thread count is floored at 2 rather
# than left at 0.
xz_threads=$(nproc 2>/dev/null || echo 2)
[ "$xz_threads" -ge 2 ] || xz_threads=2
tar_deterministic -cf - -C "$SCRATCH" vendor \
    | xz -T"$xz_threads" --block-size=16MiB -6 > "$VENDOR_TARBALL"

step "Bundle"
( cd "$OUTPUT" && sha256sum "$(basename "$SRC_TARBALL")" "$(basename "$VENDOR_TARBALL")" \
    | tee "$PREFIX-SOURCES.sha256" )
ls -lh "$SRC_TARBALL" "$VENDOR_TARBALL" | sed 's/^/  /' >&2

if [ "$KEEP_VENDOR" -eq 1 ]; then
    rm -rf "$OUTPUT/vendor"
    mv "$VENDOR_DIR" "$OUTPUT/vendor"
    note "vendor tree kept at $OUTPUT/vendor"
fi

printf '\nSource bundle for %s is in %s\n' "$V" "$OUTPUT" >&2
