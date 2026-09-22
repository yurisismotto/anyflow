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
VENDOR_CONFIG="$ROOT/packaging/common/cargo-vendor-config.toml"

UNIT="$ROOT/packaging/common/omnibridged.service"
FIREWALLD="$ROOT/packaging/fedora/omnibridge-firewalld.xml"
DEBIAN="$ROOT/packaging/debian"
GUI_DATA="$ROOT/desktop/gui/data"
APP_ID="io.github.yurisismotto.omnibridge"

BUNDLE_DIR=""
RPM_FILES=()
while [ $# -gt 0 ]; do
    case "$1" in
        --bundle) BUNDLE_DIR="${2:?--bundle needs a directory}"; shift 2 ;;
        --rpm) RPM_FILES+=("${2:?--rpm needs a file}"); shift 2 ;;
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
group "Desktop integration (audit §10; P4, P5, P7)"
# --------------------------------------------------------------------------
# One script installs the desktop entry, the icon, the D-Bus activation entry
# and the AppStream metadata, so a package and a development install cannot
# disagree about the application's identity (R9). The spec must call it rather
# than repeat its four install lines.
if grep -q 'install-desktop-metadata.sh' "$SPEC"; then
    pass "the spec installs desktop metadata with the shared script"
else
    fail "the spec does not call install-desktop-metadata.sh (R9: identity can drift)"
fi
if grep -A2 'install-desktop-metadata\.sh' "$SPEC" | grep -q -- '--destdir'; then
    pass "the metadata installer is given a --destdir"
else
    fail "the metadata installer may write outside the buildroot"
fi

# P5. `Exec` in a D-Bus service file must be absolute; the bus does not search
# PATH. The template's placeholder is what makes that true after substitution,
# and a template that lost it would install a relative Exec.
dbus_template="$GUI_DATA/$APP_ID.service.in"
if grep -qE '^Exec=@BINDIR@/omnibridge-gui' "$dbus_template"; then
    pass "P5: the D-Bus template's Exec is built from @BINDIR@"
else
    fail "P5: $dbus_template does not derive Exec from @BINDIR@"
fi
if grep -qE '^Name='"$APP_ID"'$' "$dbus_template"; then
    pass "the D-Bus template declares the application id verbatim"
else
    fail "the D-Bus template's Name= is not $APP_ID"
fi

# Q3. AppStream metadata exists and agrees with the workspace version. Without
# it OmniBridge is invisible in GNOME Software and KDE Discover.
metainfo="$GUI_DATA/$APP_ID.metainfo.xml"
if [ -f "$metainfo" ]; then
    pass "Q3: AppStream metadata is present"
    meta_version="$(sed -n 's/.*<release version="\([^"]*\)".*/\1/p' "$metainfo" | head -1)"
    if [ "$meta_version" = "$workspace_version" ]; then
        pass "the metainfo release $meta_version matches the workspace version"
    else
        fail "the metainfo says ${meta_version:-<none>}, the workspace says $workspace_version"
    fi
else
    fail "Q3: no AppStream metadata at $metainfo"
fi

# The distribution owns these two caches through its own file triggers
# (MEASURED, audit §4.4). A scriptlet here would be a second, worse copy.
for forbidden_scriptlet in update-desktop-database gtk-update-icon-cache; do
    if grep -qE "^[^#]*$forbidden_scriptlet" "$SPEC"; then
        fail "the spec runs $forbidden_scriptlet; the distro's file triggers already do"
    else
        pass "no $forbidden_scriptlet scriptlet (the distro's file trigger owns it)"
    fi
done

# --------------------------------------------------------------------------
group "Every shipped XML parses"
# --------------------------------------------------------------------------
# Cheap, and not theoretical: the first draft of both files below used '--' as
# a comment underline, which is illegal inside an XML comment and made them
# unparseable. A malformed metainfo file is dropped by the AppStream cache
# builder in silence, and a malformed firewalld service is rejected at load.
for xml in "$FIREWALLD" "$metainfo" ; do
    [ -f "$xml" ] || continue
    if python3 -c 'import sys,xml.dom.minidom; xml.dom.minidom.parse(sys.argv[1])' "$xml" 2>/dev/null; then
        pass "parses: ${xml#"$ROOT"/}"
    else
        fail "does not parse as XML: ${xml#"$ROOT"/}"
    fi
done

# --------------------------------------------------------------------------
group "Firewall: shipped, never enabled (audit §9)"
# --------------------------------------------------------------------------
if grep -q 'firewalld/services/omnibridge.xml' "$SPEC"; then
    pass "the core package installs a firewalld service definition"
else
    fail "no firewalld service definition is installed"
fi
# The whole design in one assertion: a package that runs firewall-cmd is
# opening or closing a port the user did not ask it to.
if grep -nE '^[^#]*firewall-cmd' "$SPEC" > "$SCRATCH/fw" 2>/dev/null && [ -s "$SCRATCH/fw" ]; then
    fail "the spec runs firewall-cmd outside a comment"
    sed 's/^/        /' "$SCRATCH/fw"
else
    pass "no scriptlet runs firewall-cmd, on any path"
fi
if [ -f "$FIREWALLD" ]; then
    # Parsed, not grepped. The file's own comment explains why UDP 5353 is
    # absent, and a grep over the raw text reads that explanation as a
    # declaration — which is exactly the false positive this replaced.
    python3 - "$FIREWALLD" > "$SCRATCH/fw-ports" <<'PYEOF'
import sys, xml.dom.minidom
doc = xml.dom.minidom.parse(sys.argv[1])
for el in doc.getElementsByTagName("port"):
    print(f'{el.getAttribute("protocol")}/{el.getAttribute("port")}')
PYEOF
    declared="$(tr '\n' ' ' < "$SCRATCH/fw-ports" | sed 's/ $//')"
    if [ "$declared" = "tcp/55432" ]; then
        pass "the firewalld service declares exactly tcp/55432 and nothing else"
    else
        fail "the firewalld service declares '$declared'; OmniBridge needs exactly tcp/55432"
    fi
    if grep -q '^udp/5353$' "$SCRATCH/fw-ports"; then
        fail "the firewalld service redeclares mDNS; firewalld ships its own, correctly scoped"
    else
        pass "mDNS is not redeclared (firewalld's own mdns service covers it)"
    fi
fi

# --------------------------------------------------------------------------
group "systemd user lifecycle (audit §7.3, §4.3)"
# --------------------------------------------------------------------------
for macro in systemd_user_post systemd_user_preun systemd_user_postun; do
    if grep -qE "^%$macro omnibridged\.service" "$SPEC"; then
        pass "%$macro is called"
    else
        fail "%$macro is missing; the unit will not be handled on install or removal"
    fi
done
# R7. `systemctl --global enable` would raise a LAN listener for every account
# on the machine. The preset leaves it disabled and that is the decision.
if grep -qE '^[^#]*systemctl --global enable' "$SPEC"; then
    fail "the spec globally enables the unit (R7)"
else
    pass "the unit is not globally enabled"
fi

# --------------------------------------------------------------------------
group "%doc is documentation, not the evidence tree (audit R10)"
# --------------------------------------------------------------------------
doc_line="$(grep -E '^%doc ' "$SPEC" || true)"
if [ -z "$doc_line" ]; then
    fail "the spec ships no %doc at all"
elif printf '%s' "$doc_line" | grep -qE '(^|[[:space:]])docs/?($|[[:space:]])'; then
    fail "%doc ships the docs/ tree: $doc_line"
else
    pass "%doc is $doc_line"
fi

# --------------------------------------------------------------------------
group "Subpackage split (audit §12)"
# --------------------------------------------------------------------------
if grep -q '^%package gui' "$SPEC"; then
    pass "omnibridge-gui is its own subpackage"
else
    fail "the GUI is not split out"
fi
if grep -qE '^Requires:[[:space:]]*%\{name\} = %\{version\}-%\{release\}' "$SPEC"; then
    pass "omnibridge-gui requires the exact core build"
else
    fail "omnibridge-gui does not pin the core package's exact version-release"
fi
if grep -qE '^Suggests:[[:space:]]*wl-clipboard$' "$SPEC"; then
    pass "R5: Suggests wl-clipboard, with no version constraint"
elif grep -qE '^Suggests:[[:space:]]*wl-clipboard' "$SPEC"; then
    fail "R5: wl-clipboard carries a version constraint, which is false on one distro or the other"
else
    fail "wl-clipboard is not suggested"
fi

# --------------------------------------------------------------------------
group "Debian / Ubuntu packaging (audit §6)"
# --------------------------------------------------------------------------
for f in control rules changelog copyright source/format README.source \
         omnibridge.install omnibridge-gui.install; do
    if [ -e "$DEBIAN/$f" ]; then
        pass "debian/$f"
    else
        fail "debian/$f is missing"
    fi
done
if [ -x "$DEBIAN/rules" ]; then
    pass "debian/rules is executable"
else
    fail "debian/rules is not executable; dpkg-buildpackage will refuse it"
fi

# One unit, every format. The Debian packaging must install the SAME file the
# RPM does, not a copy that can drift.
if grep -q 'packaging/common/omnibridged.service' "$DEBIAN/rules"; then
    pass "debian/rules installs the canonical unit from packaging/common/"
else
    fail "debian/rules does not install packaging/common/omnibridged.service"
fi
if [ -e "$DEBIAN/omnibridged.service" ] || [ -e "$DEBIAN/omnibridge.user.service" ]; then
    fail "a second copy of the unit exists under packaging/debian/"
else
    pass "no duplicate unit under packaging/debian/"
fi
# And the same metadata installer, so the .desktop entry, the icon, the D-Bus
# activation entry and the AppStream file cannot differ between formats (R9).
if grep -q 'install-desktop-metadata.sh' "$DEBIAN/rules"; then
    pass "debian/rules installs desktop metadata with the shared script"
else
    fail "debian/rules does not call install-desktop-metadata.sh (R9)"
fi

# The offline build is what makes a buildd build possible.
for flag in --locked --offline; do
    if grep -qE "cargo (build|test).*$flag" "$DEBIAN/rules"; then
        pass "debian/rules runs cargo $flag"
    else
        fail "debian/rules does not run cargo $flag"
    fi
done
if grep -q 'CARGO_HOME' "$DEBIAN/rules"; then
    pass "debian/rules redirects CARGO_HOME into the build tree"
else
    fail "debian/rules lets the builder's ~/.cargo/config.toml reach the build"
fi

# The unit ships disabled on every format (R7).
if grep -q 'dh_installsystemduser --no-enable' "$DEBIAN/rules"; then
    pass "R7: dh_installsystemduser --no-enable"
else
    fail "R7: the Debian package may enable the unit for every user"
fi

# Runtime must not require Rust: rustc/cargo are build dependencies only.
depends_block="$(awk '/^Package: /{p=1} p' "$DEBIAN/control" | grep -E '^(Depends|Recommends|Suggests):' || true)"
if printf '%s' "$depends_block" | grep -qE '\b(rustc|cargo)\b'; then
    fail "a runtime relation names rustc or cargo"
else
    pass "no runtime relation names rustc or cargo"
fi
# The MSRV floor must be stated, and must match the workspace.
if grep -qE "^ *rustc \(>= $msrv\)" "$DEBIAN/control"; then
    pass "Build-Depends states rustc (>= $msrv), matching the workspace MSRV"
else
    fail "Build-Depends does not state rustc (>= $msrv)"
fi

# Audit §9.3: a .deb must not carry firewalld metadata.
if grep -rq 'firewalld' "$DEBIAN"/control "$DEBIAN"/rules "$DEBIAN"/*.install 2>/dev/null; then
    fail "the Debian packaging references firewalld (audit §9.3 says it must not)"
else
    pass "no firewalld metadata in the Debian packaging (audit §9.3)"
fi
# And must not drag in a GNOME Shell extension.
if grep -rqiE 'gnome-shell-extension|appindicator' "$DEBIAN"/control 2>/dev/null; then
    fail "the Debian packaging depends on a GNOME Shell extension"
else
    pass "no GNOME Shell extension dependency"
fi

# --------------------------------------------------------------------------
group "No maintainer script may touch the trust store, on any path (R6)"
# --------------------------------------------------------------------------
# The one that matters most. ~/.local/share/omnibridge holds the identity key
# and every pairing. A postrm that removed it on purge would look like
# tidiness in review and would destroy the user's trust store silently.
scripts_found=0
for f in "$DEBIAN"/*.postinst "$DEBIAN"/*.postrm "$DEBIAN"/*.preinst \
         "$DEBIAN"/*.prerm "$DEBIAN"/postinst "$DEBIAN"/postrm \
         "$DEBIAN"/preinst "$DEBIAN"/prerm; do
    [ -e "$f" ] || continue
    scripts_found=$((scripts_found + 1))
    if grep -nE '(\.local/share|\$HOME|~/)' "$f" | grep -vE '^[0-9]+:[[:space:]]*#' > "$SCRATCH/deb-state"; then
        fail "$(basename "$f") references a user-state path"
        sed 's/^/        /' "$SCRATCH/deb-state"
    else
        pass "$(basename "$f") does not reference user state"
    fi
done
if [ "$scripts_found" -eq 0 ]; then
    pass "no hand-written maintainer scripts at all — debhelper generates them"
fi
# `purge` is the transaction most likely to grow a destructive postrm.
# `grep -n` over one file emits `<line>:<text>` with no filename, so the
# comment filter anchors on the line number alone.
grep -n 'purge' "$DEBIAN/rules" 2>/dev/null \
    | grep -vE '^[0-9]+:[[:space:]]*#' > "$SCRATCH/deb-purge" || true
if [ -s "$SCRATCH/deb-purge" ]; then
    fail "debian/rules mentions purge outside a comment"
    sed 's/^/        /' "$SCRATCH/deb-purge"
else
    pass "debian/rules adds no purge behaviour"
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
        packaging/common/cargo-vendor-config.toml \
        packaging/common/omnibridged.service \
        packaging/fedora/omnibridge-firewalld.xml \
        desktop/gui/tools/install-desktop-metadata.sh \
        desktop/gui/data/io.github.yurisismotto.omnibridge.desktop \
        desktop/gui/data/io.github.yurisismotto.omnibridge.service.in \
        desktop/gui/data/io.github.yurisismotto.omnibridge.metainfo.xml \
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
if [ "${#RPM_FILES[@]}" -gt 0 ]; then
group "Built packages"
# --------------------------------------------------------------------------
if ! command -v rpm >/dev/null; then
    fail "rpm is not installed; cannot inspect the built packages"
else
    # One listing per package, plus a combined one. R8: the exact APP_ID paths
    # are asserted, because packaging drifting from the application id installs
    # a differently-named icon and every desktop draws a grey square.
    : > "$SCRATCH/all.list"
    core_list=""
    gui_list=""
    for rpm_file in "${RPM_FILES[@]}"; do
        name="$(rpm -qp --qf '%{NAME}' "$rpm_file" 2>/dev/null || basename "$rpm_file")"
        listing="$SCRATCH/$name.list"
        rpm -qpl "$rpm_file" 2>/dev/null > "$listing"
        cat "$listing" >> "$SCRATCH/all.list"
        pass "read $name ($(wc -l < "$listing") files)"
        case "$name" in
            omnibridge-gui) gui_list="$listing" ;;
            omnibridge)     core_list="$listing" ;;
        esac
    done

    has() { grep -qxF "$2" "$1"; }

    if [ -n "$core_list" ]; then
        for path in \
            /usr/bin/omnibridged \
            /usr/bin/omnibridge \
            /usr/lib/systemd/user/omnibridged.service \
            "/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
            /usr/lib/firewalld/services/omnibridge.xml
        do
            if has "$core_list" "$path"; then
                pass "core: $path"
            else
                fail "core package is missing $path"
            fi
        done
        # The GUI binary moved out. If it is still here, the split did not
        # happen and the two packages both own it.
        if has "$core_list" /usr/bin/omnibridge-gui; then
            fail "core package still contains /usr/bin/omnibridge-gui"
        else
            pass "core: the GUI binary is not here (it is in omnibridge-gui)"
        fi
        # R10. The evidence tree must not be in the package.
        if grep -q '^/usr/share/doc/omnibridge/docs' "$core_list"; then
            fail "R10: the docs/ evidence tree is in the package ($(grep -c '^/usr/share/doc/omnibridge/docs' "$core_list") files)"
        else
            pass "R10: no docs/ tree in the package"
        fi
        if has "$core_list" /usr/share/doc/omnibridge/README.md; then
            pass "core: README.md is shipped"
        else
            fail "core package ships no README.md"
        fi
    fi

    if [ -n "$gui_list" ]; then
        for path in \
            /usr/bin/omnibridge-gui \
            "/usr/share/applications/$APP_ID.desktop" \
            "/usr/share/dbus-1/services/$APP_ID.service" \
            "/usr/share/metainfo/$APP_ID.metainfo.xml"
        do
            if has "$gui_list" "$path"; then
                pass "gui: $path"
            else
                fail "omnibridge-gui is missing $path"
            fi
        done
    fi

    # P5, against the built package rather than the template: the bus does not
    # search PATH, so a relative Exec here is a launcher that never starts.
    for rpm_file in "${RPM_FILES[@]}"; do
        if rpm -qpl "$rpm_file" 2>/dev/null | grep -q "dbus-1/services/$APP_ID.service"; then
            exec_line="$(rpm2cpio "$rpm_file" 2>/dev/null \
                | cpio -i --to-stdout "./usr/share/dbus-1/services/$APP_ID.service" 2>/dev/null \
                | grep '^Exec=' || true)"
            case "$exec_line" in
                "Exec=/usr/bin/omnibridge-gui --gapplication-service")
                    pass "P5: the packaged D-Bus Exec is the absolute installed path" ;;
                Exec=/*)
                    fail "P5: unexpected absolute Exec: $exec_line" ;;
                *)
                    fail "P5: the packaged D-Bus Exec is not absolute: ${exec_line:-<none>}" ;;
            esac
        fi
    done

    # Audit §12.3: no package may own a path under a user's home.
    if grep -qE '^(/home|/root|/var/home)' "$SCRATCH/all.list"; then
        fail "a package owns a path under a home directory"
        grep -E '^(/home|/root|/var/home)' "$SCRATCH/all.list" | sed 's/^/        /'
    else
        pass "no package owns anything under a home directory"
    fi
    if grep -q '%{_' "$SCRATCH/all.list"; then
        fail "an unexpanded rpm macro is in a package (B2 regression)"
    else
        pass "no unexpanded rpm macro in any file list"
    fi
fi
fi

printf '\n%s\n' "-----------------------------------------------"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
