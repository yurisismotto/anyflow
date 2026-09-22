#!/usr/bin/env bash
#
# Lightweight checks over the packaging tree and, when one is given, over a
# generated source bundle and a built RPM.
#
# These are the packaging equivalent of the MSRV guard in
# .github/workflows/linux-distro-compat.yml: cheap assertions that catch the
# specific ways this tree has already been observed to drift. Each one names
# the defect it is standing guard over.
#
#   ./packaging-checks.sh                       # static checks only
#   ./packaging-checks.sh --bundle dist         # ...plus the bundle in dist/
#   ./packaging-checks.sh --rpm path/to.rpm     # ...plus a built package
#
# No absolute path is baked in: every path is derived from the repository root
# or passed as an argument, so this runs the same in a checkout, a container
# and CI.

set -euo pipefail

ROOT="$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
SPEC="$ROOT/packaging/fedora/omnibridge.spec"
VENDOR_CONFIG="$ROOT/packaging/fedora/cargo-vendor-config.toml"

UNIT="$ROOT/packaging/common/omnibridged.service"

BUNDLE_DIR=""
RPM_FILE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --bundle) BUNDLE_DIR="${2:?--bundle needs a directory}"; shift 2 ;;
        --rpm) RPM_FILE="${2:?--rpm needs a file}"; shift 2 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

# Listings go to files rather than through a pipe. `grep -q` exits on its
# first match, which hands the writing end of a pipe a SIGPIPE, which
# `set -o pipefail` then reports as a failed check — a false negative that
# only appears once the listing outgrows the 64 KiB pipe buffer, which is
# exactly the kind of test that lies quietly for months.
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-checks.XXXXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

PASS=0
FAIL=0
pass() { printf '  ok    %s\n' "$*"; PASS=$((PASS + 1)); }
fail() { printf '  FAIL  %s\n' "$*"; FAIL=$((FAIL + 1)); }
group() { printf '\n%s\n' "$*"; }

# --------------------------------------------------------------------------
group "Version synchronisation (audit §13.1)"
# --------------------------------------------------------------------------
workspace_version="$(
    awk '
        /^\[workspace\.package\]/ { in_section = 1; next }
        /^\[/                     { in_section = 0 }
        in_section && /^[[:space:]]*version[[:space:]]*=/ {
            gsub(/.*=[[:space:]]*"|".*/, ""); print; exit
        }
    ' "$ROOT/desktop/Cargo.toml"
)"
spec_version="$(awk '/^Version:/ { print $2; exit }' "$SPEC")"

if [ -z "$workspace_version" ]; then
    fail "desktop/Cargo.toml [workspace.package] version is unreadable"
elif [ "$workspace_version" = "$spec_version" ]; then
    pass "spec Version: $spec_version matches the workspace version"
else
    fail "spec says $spec_version, desktop/Cargo.toml says $workspace_version"
fi

# --------------------------------------------------------------------------
group "MSRV synchronisation (audit P1)"
# --------------------------------------------------------------------------
msrv="$(awk -F'"' '/^rust-version/ { print $2; exit }' "$ROOT/desktop/Cargo.toml")"
spec_rust="$(awk '/^BuildRequires:[[:space:]]*rust[[:space:]]*>=/ { print $NF; exit }' "$SPEC")"
if [ "$msrv" = "$spec_rust" ]; then
    pass "spec BuildRequires: rust >= $spec_rust matches rust-version = $msrv"
else
    fail "spec requires rust >= ${spec_rust:-<none>}, workspace MSRV is $msrv"
fi

# --------------------------------------------------------------------------
group "Build-fatal defects B1/B2/B3 stay fixed"
# --------------------------------------------------------------------------
for br in pkgconf-pkg-config gtk4-devel libadwaita-devel glib2-devel; do
    if grep -qE "^BuildRequires:[[:space:]]+$br( |$)" "$SPEC"; then
        pass "B1: BuildRequires $br"
    else
        fail "B1: the spec builds omnibridge-gui without BuildRequires: $br"
    fi
done

# The tray tests raise their own dbus-daemon rather than mocking the bus, so
# %check needs the binary even though nothing in %build does.
if grep -qE '^BuildRequires:[[:space:]]+dbus-daemon( |$)' "$SPEC"; then
    pass "B1: BuildRequires dbus-daemon (the tray suites need a real bus)"
else
    fail "B1: %check raises a dbus-daemon that the buildroot will not have"
fi

if grep -q '^BuildRequires:[[:space:]]*systemd-rpm-macros' "$SPEC"; then
    pass "B2: BuildRequires systemd-rpm-macros"
else
    fail "B2: %{_userunitdir} is used without BuildRequires: systemd-rpm-macros"
fi

if grep -q '%{_userunitdir}' "$SPEC"; then
    pass "B2: the user unit path stays a macro"
else
    fail "B2: %{_userunitdir} was replaced by a hardcoded path"
fi

if grep -qE 'cargo (build|test).*--offline' "$SPEC"; then
    pass "B3: cargo runs --offline"
else
    fail "B3: cargo may reach crates.io during the build"
fi
if grep -qE 'cargo (build|test).*--locked' "$SPEC"; then
    pass "B3: cargo runs --locked"
else
    fail "B3: the build may update Cargo.lock"
fi
if grep -q '^Source1:' "$SPEC" && grep -q 'cargo-vendor-config.toml' "$SPEC"; then
    pass "B3: a vendor tarball and its cargo config are wired into %prep"
else
    fail "B3: no vendored source is unpacked, so an isolated build cannot resolve"
fi

# --------------------------------------------------------------------------
group "Vendor config describes an offline-only source (audit B3)"
# --------------------------------------------------------------------------
if grep -q 'replace-with = "vendored-sources"' "$VENDOR_CONFIG"; then
    pass "crates-io is replaced by vendored-sources"
else
    fail "crates-io is not replaced in $VENDOR_CONFIG"
fi
if grep -qE '^directory = "vendor"' "$VENDOR_CONFIG"; then
    pass "the vendor directory is relative, so the config is location-independent"
else
    fail "the vendor directory is absent or absolute in $VENDOR_CONFIG"
fi
if grep -qE '^\[source\."(git|sparse)\+' "$VENDOR_CONFIG"; then
    fail "a network source is configured in $VENDOR_CONFIG"
else
    pass "no git or sparse registry source is configured"
fi

# --------------------------------------------------------------------------
group "The package installs everything it builds (audit P7)"
# --------------------------------------------------------------------------
for bin in omnibridged omnibridge omnibridge-gui; do
    if grep -qE "^%\{_bindir\}/$bin$" "$SPEC"; then
        pass "%files lists $bin"
    else
        fail "$bin is built but never appears in %files"
    fi
done
for pkg in omnibridge-daemon omnibridge-cli omnibridge-gui; do
    if grep -q -- "-p $pkg" "$SPEC"; then
        pass "%build names $pkg explicitly"
    else
        fail "%build does not name $pkg"
    fi
done

# --------------------------------------------------------------------------
group "The canonical systemd user unit (audit §7.1; gates S1-S3)"
# --------------------------------------------------------------------------
# One unit, every format. The file used to live under packaging/fedora/, which
# is where a Debian package would have grown a second copy and where a
# hardening change would have landed on one distribution and missed the other.
if [ -f "$UNIT" ]; then
    pass "the unit is at packaging/common/omnibridged.service"
else
    fail "packaging/common/omnibridged.service is missing"
fi
if [ -e "$ROOT/packaging/fedora/omnibridged.service" ]; then
    fail "a second copy of the unit survives under packaging/fedora/"
else
    pass "no duplicate unit under packaging/fedora/"
fi
if grep -qE '^install .*packaging/common/omnibridged\.service' "$SPEC"; then
    pass "the spec installs the common unit"
else
    fail "the spec does not install packaging/common/omnibridged.service"
fi

if [ -f "$UNIT" ]; then
    # Directives, not comments: every check below reads the file with comment
    # and blank lines stripped, so prose describing a directive can never be
    # mistaken for the directive itself.
    grep -vE '^[[:space:]]*(#|$)' "$UNIT" > "$SCRATCH/unit.directives"

    # P2, first defect: without this the daemon cannot create
    # $XDG_RUNTIME_DIR/omnibridge under ProtectSystem=strict and has nowhere
    # to bind control.sock.
    if grep -qx 'RuntimeDirectory=omnibridge' "$SCRATCH/unit.directives"; then
        pass "S2: RuntimeDirectory=omnibridge"
    else
        fail "S2: RuntimeDirectory=omnibridge is absent; the control socket has no directory"
    fi
    if grep -qx 'RuntimeDirectoryMode=0700' "$SCRATCH/unit.directives"; then
        pass "S2: RuntimeDirectoryMode=0700"
    else
        fail "S2: RuntimeDirectoryMode is not 0700"
    fi

    # P2, second and third defects. The grant is the PARENT: measured, the
    # leaf does not exist on a fresh install, and the '-' prefix that would
    # let the unit start does not then let the daemon create it.
    if grep -qx 'ReadWritePaths=%h/.local/share' "$SCRATCH/unit.directives"; then
        pass "S1: ReadWritePaths=%h/.local/share (the parent, so a fresh install can create its data directory)"
    else
        fail "S1: ReadWritePaths= is not the settled %h/.local/share"
    fi
    if grep -q '^StateDirectory=' "$SCRATCH/unit.directives"; then
        fail "StateDirectory= is back; in a user unit it creates ~/.local/state, which the daemon never opens"
    else
        pass "no StateDirectory= (it would point at ~/.local/state)"
    fi

    # The sandbox. These are the lines a well-meaning fix for a start-up
    # failure reaches for first, so each one is named rather than counted.
    for directive in \
        'NoNewPrivileges=true' \
        'PrivateTmp=true' \
        'ProtectSystem=strict' \
        'ProtectHome=read-only' \
        'ProtectKernelTunables=true' \
        'ProtectKernelModules=true' \
        'ProtectControlGroups=true' \
        'RestrictNamespaces=true' \
        'RestrictRealtime=true' \
        'RestrictSUIDSGID=true' \
        'LockPersonality=true' \
        'MemoryDenyWriteExecute=true' \
        'SystemCallArchitectures=native' \
        'SystemCallFilter=@system-service' \
        'SystemCallFilter=~@privileged @resources @obsolete' \
        'RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK'
    do
        if grep -qxF "$directive" "$SCRATCH/unit.directives"; then
            pass "hardening kept: $directive"
        else
            fail "hardening weakened or removed: $directive"
        fi
    done

    # A user unit, and it must stay one. User=/Group= in a user unit is not
    # even legal, but naming root anywhere is the mistake worth catching.
    if grep -qE '^(User|Group)=' "$SCRATCH/unit.directives"; then
        fail "the unit sets User=/Group=; it is a --user unit and must not"
    else
        pass "no User=/Group= — it runs as whoever owns the session"
    fi
    if grep -qx 'WantedBy=default.target' "$SCRATCH/unit.directives"; then
        pass "[Install] WantedBy=default.target (a user unit target)"
    else
        fail "the unit is not installed into default.target"
    fi
    if grep -qx 'ExecStart=/usr/bin/omnibridged' "$SCRATCH/unit.directives"; then
        pass "ExecStart is the absolute installed path"
    else
        fail "ExecStart is not /usr/bin/omnibridged"
    fi
    # The trust store is not the unit's to own. ReadWritePaths grants the
    # parent and stops there; anything that named the leaf as a directory to
    # create or clean would be creating package-owned user state.
    if grep -qE '^(RuntimeDirectory|StateDirectory|CacheDirectory|LogsDirectory|ConfigurationDirectory)=.*\.local' "$SCRATCH/unit.directives"; then
        fail "a *Directory= directive points into the user's data tree"
    else
        pass "no *Directory= directive creates or owns user state"
    fi
fi

# --------------------------------------------------------------------------
group "No maintainer script touches user state (audit R6)"
# --------------------------------------------------------------------------
# state.json and identity.key are the trust store. A scriptlet that removed
# them would destroy a user's pairings silently, and it would look like
# tidiness in review. The grep is the guard.
grep -nE '(\.local/share|\$HOME|%\{_sharedstatedir\}|~/)' "$SPEC" \
    | grep -vE '^[0-9]+:#' > "$SCRATCH/state-refs" || true
if [ -s "$SCRATCH/state-refs" ]; then
    fail "the spec references a user-state path outside a comment"
else
    pass "no user-state path is referenced outside comments"
fi

# --------------------------------------------------------------------------
if [ -n "$BUNDLE_DIR" ]; then
group "Generated source bundle"
# --------------------------------------------------------------------------
src_tarball="$(find "$BUNDLE_DIR" -maxdepth 1 -name 'omnibridge-*.tar.gz' | head -1)"
vendor_tarball="$(find "$BUNDLE_DIR" -maxdepth 1 -name 'omnibridge-*-vendor.tar.xz' | head -1)"

if [ -n "$src_tarball" ]; then pass "source tarball: $(basename "$src_tarball")"
else fail "no source tarball in $BUNDLE_DIR"; fi
if [ -n "$vendor_tarball" ]; then pass "vendor tarball: $(basename "$vendor_tarball")"
else fail "no vendor tarball in $BUNDLE_DIR"; fi

if [ -n "$src_tarball" ]; then
    tar -tzf "$src_tarball" > "$SCRATCH/src.list"

    # The first four are build inputs and the icon is read by
    # `desktop/gui/build.rs`. U2 is neither: it is here because
    # `make-source-bundle.sh` used to carry a special-case exclusion for it,
    # from when it was an untracked file at the repository root. It is now a
    # tracked document under `docs/`, and asserting that the bundle ships it
    # is what would catch that exclusion being reintroduced.
    for required in \
        desktop/Cargo.lock \
        desktop/Cargo.toml \
        packaging/fedora/omnibridge.spec \
        packaging/fedora/cargo-vendor-config.toml \
        packaging/common/omnibridged.service \
        docs/design/assets/omnibridge-app-icon.svg \
        docs/audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
    do
        if grep -qE "^omnibridge-[^/]+/$required$" "$SCRATCH/src.list"; then
            pass "bundle carries $required"
        else
            fail "bundle is missing $required"
        fi
    done

    # docs/design/assets is not documentation as far as the build is
    # concerned: desktop/gui/build.rs reads the app icon out of it.
    for forbidden in \
        '\.git/' 'desktop/target/' 'android/build/' 'android/[^/]*/build/' \
        '\.apk$' '\.aab$' '\.key$' '\.pem$' '\.jks$' '\.keystore$' \
        'state\.json$' 'trust-store\.json$' 'local\.properties$'
    do
        if grep -qE "$forbidden" "$SCRATCH/src.list"; then
            fail "bundle contains a forbidden path matching /$forbidden/"
        else
            pass "bundle has no $forbidden"
        fi
    done
fi

if [ -n "$vendor_tarball" ]; then
    tar -tJf "$vendor_tarball" > "$SCRATCH/vendor.list"
    if grep -qE '^vendor/' "$SCRATCH/vendor.list"; then
        pass "vendor tarball unpacks to vendor/"
    else
        fail "vendor tarball does not have a vendor/ root"
    fi
    checksums="$(grep -c '\.cargo-checksum\.json$' "$SCRATCH/vendor.list" || true)"
    if [ "$checksums" -gt 0 ]; then
        pass "all $checksums vendored crates carry a cargo checksum"
    else
        fail "no .cargo-checksum.json in the vendor tarball"
    fi
fi
fi

# --------------------------------------------------------------------------
if [ -n "$RPM_FILE" ]; then
group "Built package: $(basename "$RPM_FILE")"
# --------------------------------------------------------------------------
if command -v rpm >/dev/null; then
    rpm -qpl "$RPM_FILE" 2>/dev/null > "$SCRATCH/rpm.list"
    for bin in omnibridged omnibridge omnibridge-gui; do
        if grep -qE "/usr/bin/$bin$" "$SCRATCH/rpm.list"; then
            pass "package contains /usr/bin/$bin"
        else
            fail "package is missing /usr/bin/$bin"
        fi
    done
    if grep -qE 'systemd/user/omnibridged\.service$' "$SCRATCH/rpm.list"; then
        pass "the user unit landed in a real systemd user directory"
    else
        fail "omnibridged.service is not under a systemd user directory"
    fi
    if grep -q '%{_userunitdir}' "$SCRATCH/rpm.list"; then
        fail "an unexpanded %{_userunitdir} is in the package (B2 regression)"
    else
        pass "no unexpanded rpm macro in the file list"
    fi
else
    fail "rpm is not installed; cannot inspect $RPM_FILE"
fi
fi

printf '\n%s\n' "-----------------------------------------------"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
