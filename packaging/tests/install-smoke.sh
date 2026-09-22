#!/usr/bin/env bash
#
# Installs built OmniBridge packages in a throwaway container and measures what
# the install, the removal and the reinstall actually did.
#
#   ./install-smoke.sh --image registry.fedoraproject.org/fedora:44 out/*.rpm
#   ./install-smoke.sh --image docker.io/library/debian:trixie out/*.deb
#
# The container is always disposable: it is created for the run and removed
# afterwards, so there is nothing to keep and no flag to keep it with.
#
# What this is, and what it is not
# --------------------------------
# It is a **package-lifecycle** test: the file manifest, the scriptlets, and
# above all the promise that no transaction touches the user's trust store. It
# runs the real package manager against the real artifacts, as root, in a
# container that has never seen OmniBridge.
#
# It is **NOT** desktop certification, and it must never be quoted as any. A
# container has no `systemd --user` manager, no session bus, no compositor and
# no tray host. Nothing here says anything about whether the launcher appears,
# whether the GUI starts, whether D-Bus cold activation works, or whether the
# daemon autostarts at login. Those are gates L3 and L6-L9 and they need a real
# graphical session.
#
# Why a container and not a VM
# ----------------------------
# For the gates it *does* cover, a container is the stronger instrument: it is
# a machine that has never had OmniBridge on it, it is destroyed afterwards,
# and it can be re-run in ninety seconds. The gates it cannot cover are not
# gates a VM would cover better — they need a graphical login, which is a
# different measurement entirely.
#
# The user's trust store
# ----------------------
# The central assertion. A fake identity is planted as a non-root user before
# the package is removed, and its digest and mode are compared afterwards. If
# any maintainer script ever grows a line that removes `~/.local/share/
# omnibridge`, this is what fails.

set -euo pipefail

IMAGE=""
PACKAGES=()

while [ $# -gt 0 ]; do
    case "$1" in
        --image) IMAGE="${2:?--image needs a container image}"; shift 2 ;;
        -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
        *) PACKAGES+=("$1"); shift ;;
    esac
done

die() { printf '\ninstall-smoke: %s\n' "$*" >&2; exit 2; }

[ -n "$IMAGE" ] || die "pass --image"
[ "${#PACKAGES[@]}" -gt 0 ] || die "pass at least one package file"
command -v podman >/dev/null || die "podman is required"

for p in "${PACKAGES[@]}"; do
    [ -f "$p" ] || die "not a file: $p"
done

# The format decides which package manager the script inside the container
# reaches for. Derived from the artifacts rather than from the image name, so
# a mismatch is caught here instead of halfway through a transaction.
case "${PACKAGES[0]}" in
    *.rpm) FORMAT=rpm ;;
    *.deb) FORMAT=deb ;;
    *) die "cannot tell the package format from ${PACKAGES[0]}" ;;
esac

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-smoke.XXXXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/pkgs"
cp -- "${PACKAGES[@]}" "$STAGE/pkgs/"

printf '\n==> %s packages, %s\n' "$FORMAT" "$IMAGE"
for p in "${PACKAGES[@]}"; do printf '    %s\n' "$(basename "$p")"; done

# --------------------------------------------------------------------------
# The script that runs inside. Written here rather than mounted from the
# repository so that what ran is visible in this file.
# --------------------------------------------------------------------------
cat > "$STAGE/inside.sh" <<'INSIDE'
set -uo pipefail

PASS=0
FAIL=0
pass() { printf '  ok    %s\n' "$*"; PASS=$((PASS + 1)); }
fail() { printf '  FAIL  %s\n' "$*"; FAIL=$((FAIL + 1)); }
group() { printf '\n%s\n' "$*"; }

FORMAT="$1"
APP_ID="io.github.yurisismotto.omnibridge"

# --------------------------------------------------------------------------
group "Install"
# --------------------------------------------------------------------------
if [ "$FORMAT" = rpm ]; then
    dnf -y install /pkgs/*.rpm > /tmp/install.log 2>&1
else
    apt-get update -qq > /tmp/install.log 2>&1
    apt-get -y install /pkgs/*.deb >> /tmp/install.log 2>&1
fi
rc=$?
if [ "$rc" -eq 0 ]; then
    pass "the packages installed (exit 0)"
else
    fail "install failed (exit $rc)"
    tail -30 /tmp/install.log | sed 's/^/        /'
fi
# A scriptlet that failed but did not fail the transaction is still a defect.
if grep -qiE 'scriptlet (failed|error)|warning: %(post|pre)' /tmp/install.log; then
    fail "a scriptlet reported a problem"
    grep -iE 'scriptlet (failed|error)|warning: %(post|pre)' /tmp/install.log | sed 's/^/        /'
else
    pass "no scriptlet error"
fi

# --------------------------------------------------------------------------
group "The files that must be there (audit §12, R8)"
# --------------------------------------------------------------------------
for f in \
    /usr/bin/omnibridged \
    /usr/bin/omnibridge \
    /usr/bin/omnibridge-gui \
    /usr/lib/systemd/user/omnibridged.service \
    "/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "/usr/share/applications/$APP_ID.desktop" \
    "/usr/share/dbus-1/services/$APP_ID.service" \
    "/usr/share/metainfo/$APP_ID.metainfo.xml"
do
    if [ -e "$f" ]; then pass "installed: $f"; else fail "missing: $f"; fi
done

# --------------------------------------------------------------------------
group "Desktop metadata is valid on the installed system"
# --------------------------------------------------------------------------
exec_line="$(grep '^Exec=' "/usr/share/dbus-1/services/$APP_ID.service" 2>/dev/null || true)"
if [ "$exec_line" = "Exec=/usr/bin/omnibridge-gui --gapplication-service" ]; then
    pass "P5: the D-Bus Exec is the absolute installed path"
else
    fail "P5: D-Bus Exec is '${exec_line:-<none>}'"
fi
# It must point at something that is actually there — an absolute path to a
# file the package forgot to ship would look correct and start nothing.
target="$(printf '%s' "$exec_line" | sed 's/^Exec=//; s/ .*//')"
if [ -x "$target" ]; then
    pass "the D-Bus Exec target exists and is executable"
else
    fail "the D-Bus Exec target '$target' is not an executable file"
fi
if command -v desktop-file-validate >/dev/null 2>&1; then
    if desktop-file-validate "/usr/share/applications/$APP_ID.desktop" 2>/tmp/dfv; then
        pass "the installed desktop entry validates"
    else
        fail "the installed desktop entry does not validate"
        sed 's/^/        /' /tmp/dfv
    fi
fi

# --------------------------------------------------------------------------
group "The systemd user unit (audit §4.3)"
# --------------------------------------------------------------------------
unit=/usr/lib/systemd/user/omnibridged.service
if grep -qx 'RuntimeDirectory=omnibridge' "$unit" \
   && grep -qx 'ReadWritePaths=%h/.local/share' "$unit" \
   && grep -qx 'ProtectSystem=strict' "$unit"; then
    pass "the installed unit carries the S1/S2 directives and keeps ProtectSystem=strict"
else
    fail "the installed unit is not the hardened one"
fi
# Shipped disabled: a global enable would raise a LAN listener for every
# account on the machine.
if [ -e /etc/systemd/user/default.target.wants/omnibridged.service ] \
   || [ -e /usr/lib/systemd/user/default.target.wants/omnibridged.service ]; then
    fail "R7: the unit was enabled globally by the install"
else
    pass "R7: the unit is installed disabled"
fi

# --------------------------------------------------------------------------
group "Nothing runs, and nothing runs as root"
# --------------------------------------------------------------------------
# The package starts no daemon. It cannot: a root scriptlet has no route to a
# user's service manager, and it must not try.
if pgrep -x omnibridged >/dev/null 2>&1; then
    fail "installing the package started omnibridged"
    ps -o user,pid,cmd -C omnibridged | sed 's/^/        /'
else
    pass "installing the package started no daemon"
fi

# --------------------------------------------------------------------------
group "%doc is documentation, not the evidence tree (R10)"
# --------------------------------------------------------------------------
docs_shipped="$(find /usr/share/doc/omnibridge -type f 2>/dev/null | wc -l)"
if [ -d /usr/share/doc/omnibridge/docs ]; then
    fail "R10: the docs/ evidence tree is installed ($docs_shipped files under doc/)"
else
    pass "R10: no docs/ tree installed ($docs_shipped file(s) under doc/)"
fi

# --------------------------------------------------------------------------
group "The firewall was shipped and not touched (audit §9)"
# --------------------------------------------------------------------------
if [ "$FORMAT" = rpm ]; then
    if [ -e /usr/lib/firewalld/services/omnibridge.xml ]; then
        pass "the firewalld service definition is installed"
    else
        fail "the firewalld service definition is missing"
    fi
else
    if [ -e /usr/lib/firewalld/services/omnibridge.xml ]; then
        fail "a .deb shipped firewalld metadata; audit §9.3 says it must not"
    else
        pass "no firewalld metadata in the .deb (audit §9.3)"
    fi
fi

# --------------------------------------------------------------------------
group "User state survives remove and reinstall (audit §11, R6, gates L21-L23)"
# --------------------------------------------------------------------------
# A user who has paired a phone. The bytes are fake; the paths, modes and the
# promise about them are the real thing.
id -u tester >/dev/null 2>&1 || useradd -m tester
DATA=/home/tester/.local/share/omnibridge
# Created as root and then chowned, rather than through runuser or su. Neither
# is present in a minimal Fedora image, and the first version of this script
# used runuser: it failed, the fixture was never created, and every assertion
# below then compared nothing to nothing and reported PASS. The end state is
# identical either way — what matters is the files, their modes and their
# owner, not which uid called mkdir.
mkdir -p "$DATA"
printf 'PRETEND-PRIVATE-KEY\n' > "$DATA/identity.key"
printf '{"schema":1,"peers":["SM-X620"]}\n' > "$DATA/state.json"
chmod 700 "$DATA"
chmod 600 "$DATA/identity.key" "$DATA/state.json"
chown -R tester:tester /home/tester/.local

# The guard that was missing. An absent fixture makes every assertion in this
# group vacuously true, which is worse than a failure because it reports PASS.
if [ -s "$DATA/identity.key" ] && [ -s "$DATA/state.json" ] \
   && [ "$(stat -c '%U' "$DATA/identity.key")" = tester ]; then
    pass "trust-store fixture planted, owned by $(stat -c '%U' "$DATA/identity.key")"
else
    fail "the trust-store fixture was not created; the assertions below would prove nothing"
fi

before="$(sha256sum $DATA/identity.key $DATA/state.json; stat -c '%a %U %n' $DATA $DATA/identity.key $DATA/state.json)"
# And the fingerprint itself must be non-empty, for the same reason.
if printf '%s' "$before" | grep -q identity.key; then
    pass "the before-fingerprint names the trust store"
else
    fail "the before-fingerprint is empty; nothing below can be compared"
fi

if [ "$FORMAT" = rpm ]; then
    dnf -y remove omnibridge omnibridge-gui > /tmp/remove.log 2>&1
else
    apt-get -y remove omnibridge omnibridge-gui > /tmp/remove.log 2>&1
fi
rc=$?
if [ "$rc" -eq 0 ]; then pass "remove succeeded (exit 0)"; else fail "remove failed (exit $rc)"; fi

after_remove="$(sha256sum $DATA/identity.key $DATA/state.json 2>/dev/null; stat -c '%a %U %n' $DATA $DATA/identity.key $DATA/state.json 2>/dev/null)"
if [ "$before" = "$after_remove" ]; then
    pass "L21/L23: the trust store is byte- and mode-identical after remove"
else
    fail "L21/L23: REMOVE CHANGED THE USER'S TRUST STORE"
    diff <(printf '%s\n' "$before") <(printf '%s\n' "$after_remove") | sed 's/^/        /'
fi

# L25: nothing package-owned is left behind.
left=""
for f in /usr/bin/omnibridged /usr/bin/omnibridge /usr/bin/omnibridge-gui \
         /usr/lib/systemd/user/omnibridged.service \
         "/usr/share/applications/$APP_ID.desktop" \
         "/usr/share/dbus-1/services/$APP_ID.service" \
         "/usr/share/metainfo/$APP_ID.metainfo.xml" \
         "/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg" \
         /usr/lib/firewalld/services/omnibridge.xml
do
    [ -e "$f" ] && left="$left $f"
done
if [ -n "$left" ]; then
    fail "L25: package files survived removal:$left"
else
    pass "L25: no package-owned file survived removal"
fi

# Reinstall, and the same state must still be there.
if [ "$FORMAT" = rpm ]; then
    dnf -y install /pkgs/*.rpm > /tmp/reinstall.log 2>&1
else
    apt-get -y install /pkgs/*.deb > /tmp/reinstall.log 2>&1
fi
rc=$?
if [ "$rc" -eq 0 ]; then pass "L22: reinstall succeeded"; else fail "L22: reinstall failed (exit $rc)"; fi

after_reinstall="$(sha256sum $DATA/identity.key $DATA/state.json 2>/dev/null; stat -c '%a %U %n' $DATA $DATA/identity.key $DATA/state.json 2>/dev/null)"
if [ "$before" = "$after_reinstall" ]; then
    pass "L22/L23: the trust store is unchanged after reinstall"
else
    fail "L22/L23: REINSTALL CHANGED THE USER'S TRUST STORE"
fi

# Debian only: purge is the transaction most likely to grow a destructive
# postrm, so it gets its own assertion.
if [ "$FORMAT" = deb ]; then
    apt-get -y purge omnibridge omnibridge-gui > /tmp/purge.log 2>&1
    rc=$?
    if [ "$rc" -eq 0 ]; then pass "L24: purge succeeded"; else fail "L24: purge failed (exit $rc)"; fi
    after_purge="$(sha256sum $DATA/identity.key $DATA/state.json 2>/dev/null; stat -c '%a %U %n' $DATA $DATA/identity.key $DATA/state.json 2>/dev/null)"
    if [ "$before" = "$after_purge" ]; then
        pass "L24: the trust store is unchanged after PURGE"
    else
        fail "L24: PURGE DESTROYED THE USER'S TRUST STORE"
    fi
fi

# L26: no root-owned file anywhere in the user's OmniBridge state.
rooted="$(find /home/tester/.local/share/omnibridge ! -user tester 2>/dev/null)"
if [ -n "$rooted" ]; then
    fail "L26: root-owned files in the user's state:"
    printf '%s\n' "$rooted" | sed 's/^/        /'
else
    pass "L26: every file in the user's state is owned by the user"
fi

printf '\n%s\n' "-----------------------------------------------"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
INSIDE

podman run --rm \
    -v "$STAGE/pkgs:/pkgs:ro,z" \
    -v "$STAGE/inside.sh:/inside.sh:ro,z" \
    -e DEBIAN_FRONTEND=noninteractive \
    "$IMAGE" bash /inside.sh "$FORMAT"
