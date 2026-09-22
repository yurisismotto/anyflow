#!/usr/bin/env bash
#
# Runs pre-gates S1, S2 and S3 from
# docs/audits/packaging/PACKAGING-V1-READINESS-AUDIT.md §14.1 against the real
# systemd user unit, on the machine it is run on.
#
#   S1  the unit starts on a machine where the daemon's data directory does
#       not exist — i.e. every fresh install — and the daemon creates it
#   S2  $XDG_RUNTIME_DIR/omnibridge is 0700 and control.sock is 0600, both
#       owned by the invoking user
#   S3  the journal for the unit carries no sandbox denial
#
# Why a script and not a transcript
# ---------------------------------
# These three gates are not a one-off. Audit §14.3 runs them again on Ubuntu
# 24.04, Ubuntu 26.04 and Debian 13, and every later change to the unit has to
# re-measure them. A transcript in a report cannot be re-run on a different
# distribution; this can.
#
# Disposable state, always
# ------------------------
# The gates are measured against a throwaway XDG_DATA_HOME, never the real
# trust store. That is not squeamishness: S1's whole question is what happens
# when the data directory does NOT exist, which cannot be asked of a machine
# where it does. The throwaway directory is created one level under
# ~/.local/share, exactly mirroring the real ~/.local/share/omnibridge, so the
# ReadWritePaths= grant is exercised at the same depth it will be in service.
#
# The real ~/.local/share/omnibridge is digested before and after and the run
# fails if anything moved. Nothing in here deletes it, on any path, including
# the error paths.
#
# Usage
# -----
#   ./systemd-unit-gates.sh                        # unit from the repo, /usr/bin/omnibridged
#   ./systemd-unit-gates.sh --binary <path>        # test a build that is not installed
#   ./systemd-unit-gates.sh --unit <path>          # test a specific unit file
#   ./systemd-unit-gates.sh --keep                 # leave the unit installed for inspection
#
# It needs no root and refuses to run as root.

set -euo pipefail

ROOT="$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel 2>/dev/null || echo "")"
UNIT_SRC="${ROOT:+$ROOT/packaging/common/omnibridged.service}"
BINARY="/usr/bin/omnibridged"
KEEP=0
UNIT_NAME="omnibridged.service"

while [ $# -gt 0 ]; do
    case "$1" in
        --unit)   UNIT_SRC="${2:?--unit needs a file}"; shift 2 ;;
        --binary) BINARY="${2:?--binary needs a file}"; shift 2 ;;
        --keep)   KEEP=1; shift ;;
        -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
done

die()  { printf '\nsystemd-unit-gates: %s\n' "$*" >&2; exit 2; }
note() { printf '  %s\n' "$*"; }
group(){ printf '\n%s\n%s\n' "$*" "$(printf '%.0s-' $(seq ${#1}))"; }

PASS=0
FAIL=0
pass() { printf '  ok    %s\n' "$*"; PASS=$((PASS + 1)); }
fail() { printf '  FAIL  %s\n' "$*"; FAIL=$((FAIL + 1)); }

[ "$(id -u)" -ne 0 ] || die "refusing to run as root: omnibridged is a user service"
[ -n "${XDG_RUNTIME_DIR:-}" ] || die "XDG_RUNTIME_DIR is unset; there is no user session to measure"
[ -n "$UNIT_SRC" ] && [ -f "$UNIT_SRC" ] || die "no unit file: pass --unit <path>"
[ -x "$BINARY" ] || die "no daemon binary at $BINARY: pass --binary <path>"
systemctl --user show-environment >/dev/null 2>&1 || die "no systemd --user manager in this session"

# --------------------------------------------------------------------------
# The real trust store is read-only to this script. Digest it up front so the
# final check can prove that, rather than assert it.
# --------------------------------------------------------------------------
REAL_DATA="${XDG_DATA_HOME:-$HOME/.local/share}/omnibridge"
fingerprint_real() {
    if [ -d "$REAL_DATA" ]; then
        # Modes and ownership matter as much as content: a run that left the
        # key world-readable would be a failure even with identical bytes.
        find "$REAL_DATA" -maxdepth 1 -type f -printf '%m %u %s %P\n' 2>/dev/null | sort
        find "$REAL_DATA" -maxdepth 1 -type f -exec sha256sum {} + 2>/dev/null | sort -k2
    else
        echo "ABSENT"
    fi
}
REAL_BEFORE="$(fingerprint_real)"

# --------------------------------------------------------------------------
# Nothing may already own the port, the socket or the unit name: the gates
# measure a first start, and a second daemon would fail its bind for reasons
# that have nothing to do with the unit.
# --------------------------------------------------------------------------
if pgrep -x omnibridged >/dev/null 2>&1; then
    die "omnibridged is already running (pid $(pgrep -x omnibridged | tr '\n' ' ')).
Stop it first — including a hand-started development build — and start it again
afterwards. This script will not stop a daemon it did not start."
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-gates.XXXXXXXX")"
UNIT_DIR="$HOME/.config/systemd/user"
UNIT_DST="$UNIT_DIR/$UNIT_NAME"
DROPIN_DIR="$UNIT_DIR/$UNIT_NAME.d"
DROPIN="$DROPIN_DIR/zz-gate-harness.conf"
HAD_UNIT=0
[ -e "$UNIT_DST" ] && HAD_UNIT=1

# The disposable data root. One level under ~/.local/share so that the
# daemon's own directory sits exactly where the real one does relative to the
# ReadWritePaths= grant.
PROBE_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/omnibridge-gate-probe.$$"

cleanup() {
    status=$?
    systemctl --user stop "$UNIT_NAME" >/dev/null 2>&1 || true
    if [ "$KEEP" -eq 0 ]; then
        [ "$HAD_UNIT" -eq 0 ] && rm -f "$UNIT_DST"
        rm -f "$DROPIN"
        rmdir "$DROPIN_DIR" 2>/dev/null || true
        systemctl --user daemon-reload >/dev/null 2>&1 || true
    fi
    # The probe root, and only the probe root. The name is built from this
    # script's own PID and is re-checked here so no edit can widen it into
    # something that removes real state.
    case "$PROBE_ROOT" in
        *"/omnibridge-gate-probe."*) rm -rf "$PROBE_ROOT" ;;
    esac
    rm -rf "$SCRATCH"
    exit $status
}
trap cleanup EXIT

# --------------------------------------------------------------------------
group "The unit under test"
# --------------------------------------------------------------------------
note "unit   $UNIT_SRC"
note "binary $BINARY"
mkdir -p "$UNIT_DIR" "$DROPIN_DIR"
install -m0644 "$UNIT_SRC" "$UNIT_DST"

# The two deltas, and they are the only two. Both are recorded in the report
# that cites this run.
#
#   ExecStart   the gates may run against a build that is not installed at
#               /usr/bin. The argument vector is otherwise untouched: mDNS
#               stays on, because it is the widest syscall surface the daemon
#               has and S3 is exactly a question about syscalls.
#   XDG_DATA_HOME  the disposable data root described in the header.
#
# No sandbox directive is overridden. Anything that did would make the run
# meaningless.
mkdir -p "$PROBE_ROOT"
cat > "$DROPIN" <<EOF
[Service]
ExecStart=
ExecStart=$BINARY
Environment=XDG_DATA_HOME=$PROBE_ROOT
EOF
systemctl --user daemon-reload

# --------------------------------------------------------------------------
group "Static verification"
# --------------------------------------------------------------------------
if systemd-analyze verify "$UNIT_DST" > "$SCRATCH/verify.txt" 2>&1; then
    pass "systemd-analyze verify: clean"
else
    fail "systemd-analyze verify reported problems"
    sed 's/^/        /' "$SCRATCH/verify.txt"
fi
if [ -s "$SCRATCH/verify.txt" ]; then
    note "verify output:"
    sed 's/^/        /' "$SCRATCH/verify.txt"
fi

# --------------------------------------------------------------------------
group "Gate S1 — first start with no data directory"
# --------------------------------------------------------------------------
PROBE_DATA="$PROBE_ROOT/omnibridge"
[ ! -e "$PROBE_DATA" ] || die "the probe data directory already exists; refusing"
note "XDG_DATA_HOME=$PROBE_ROOT (data directory $PROBE_DATA does not exist)"

CURSOR="$(journalctl --user -n0 --show-cursor 2>/dev/null | sed -n 's/^-- cursor: //p')"

if systemctl --user start "$UNIT_NAME" 2>"$SCRATCH/start.err"; then
    pass "S1: the unit starts where the data directory does not exist"
else
    fail "S1: systemctl --user start failed"
    sed 's/^/        /' "$SCRATCH/start.err"
fi

# The socket is the daemon's own readiness signal: it appears after the
# identity is on disk. Polling for it beats sleeping for a guess.
SOCK="$XDG_RUNTIME_DIR/omnibridge/control.sock"
for _ in $(seq 100); do
    [ -S "$SOCK" ] && break
    sleep 0.1
done

state="$(systemctl --user is-active "$UNIT_NAME" || true)"
if [ "$state" = "active" ]; then
    pass "S1: the unit is active"
else
    fail "S1: the unit is $state"
fi

if [ -f "$PROBE_DATA/identity.key" ]; then
    pass "S1: the daemon created identity.key inside a read-only \$HOME"
else
    fail "S1: identity.key was not created — the ReadWritePaths grant did not reach it"
fi
for f in identity.key state.json; do
    if [ -f "$PROBE_DATA/$f" ]; then
        m="$(stat -c '%a' "$PROBE_DATA/$f")"
        if [ "$m" = "600" ]; then
            pass "S1: $f is 0600"
        else
            fail "S1: $f is $m, expected 600"
        fi
    fi
done
if [ -d "$PROBE_DATA" ]; then
    m="$(stat -c '%a' "$PROBE_DATA")"
    if [ "$m" = "700" ]; then
        pass "S1: the data directory is 0700"
    else
        fail "S1: the data directory is $m, expected 700"
    fi
fi

# --------------------------------------------------------------------------
group "Gate S2 — runtime directory and control socket"
# --------------------------------------------------------------------------
RUNDIR="$XDG_RUNTIME_DIR/omnibridge"
if [ -d "$RUNDIR" ]; then
    pass "S2: $RUNDIR exists"
    read -r mode owner _ <<EOF
$(stat -c '%a %U %n' "$RUNDIR")
EOF
    if [ "$mode" = "700" ]; then
        pass "S2: runtime directory mode 0700"
    else
        fail "S2: runtime directory mode $mode, expected 700"
    fi
    if [ "$owner" = "$(id -un)" ]; then
        pass "S2: runtime directory owned by $owner"
    else
        fail "S2: runtime directory owned by $owner, expected $(id -un)"
    fi
else
    fail "S2: $RUNDIR does not exist"
fi
if [ -S "$SOCK" ]; then
    pass "S2: control.sock exists and is a socket"
    read -r mode owner _ <<EOF
$(stat -c '%a %U %n' "$SOCK")
EOF
    if [ "$mode" = "600" ]; then
        pass "S2: control.sock mode 0600"
    else
        fail "S2: control.sock mode $mode, expected 600"
    fi
    if [ "$owner" = "$(id -un)" ]; then
        pass "S2: control.sock owned by $owner"
    else
        fail "S2: control.sock owned by $owner, expected $(id -un)"
    fi
else
    fail "S2: $SOCK is missing"
fi

# --------------------------------------------------------------------------
group "The process itself"
# --------------------------------------------------------------------------
MAINPID="$(systemctl --user show -p MainPID --value "$UNIT_NAME" || echo 0)"
if [ "${MAINPID:-0}" -gt 0 ]; then
    puser="$(ps -o user= -p "$MAINPID" | tr -d ' ')"
    if [ "$puser" != "root" ] && [ "$puser" = "$(id -un)" ]; then
        pass "the daemon runs as $puser, not root"
    else
        fail "the daemon runs as $puser"
    fi
    # Proof the sandbox is applied, not merely declared: a unit whose
    # directives were silently ignored would look identical in `systemctl cat`.
    if grep -q 'NoNewPrivs:[[:space:]]*1' "/proc/$MAINPID/status"; then
        pass "NoNewPrivs is 1 on the running process"
    else
        fail "NoNewPrivs is not set on the running process"
    fi
else
    fail "the unit has no MainPID"
fi

# --------------------------------------------------------------------------
group "Restart (audit L18)"
# --------------------------------------------------------------------------
if systemctl --user restart "$UNIT_NAME" 2>"$SCRATCH/restart.err"; then
    for _ in $(seq 100); do [ -S "$SOCK" ] && break; sleep 0.1; done
    if [ -S "$SOCK" ] && [ "$(stat -c '%a' "$SOCK")" = "600" ]; then
        pass "restart: the socket is recreated at 0600"
    else
        fail "restart: the socket did not come back correctly"
    fi
    if [ "$(systemctl --user is-active "$UNIT_NAME" || true)" = "active" ]; then
        pass "restart: the unit is active again"
    else
        fail "restart: the unit did not come back"
    fi
else
    fail "restart failed"
    sed 's/^/        /' "$SCRATCH/restart.err"
fi

# --------------------------------------------------------------------------
group "Gate S3 — no sandbox denial in the journal"
# --------------------------------------------------------------------------
if [ -n "${CURSOR:-}" ]; then
    journalctl --user -u "$UNIT_NAME" --after-cursor "$CURSOR" --no-pager \
        > "$SCRATCH/journal.txt" 2>/dev/null || true
else
    journalctl --user -u "$UNIT_NAME" --no-pager > "$SCRATCH/journal.txt" 2>/dev/null || true
fi
note "$(wc -l < "$SCRATCH/journal.txt") journal lines for $UNIT_NAME"

# Each pattern is a way this sandbox actually announces a refusal. Matching on
# the strings rather than on "error" keeps an unrelated warning from failing
# the gate and an unrelated info line from passing it.
DENIALS='Read-only file system|Operation not permitted|Permission denied|Failed to set up mount namespace|Failed at step (NAMESPACE|RUNTIME_DIRECTORY|STATE_DIRECTORY|EXEC|SECCOMP)|Invalid ReadWritePaths|bad-system-call|signal=SYS|Failed to create.*director|seccomp'
if grep -qE "$DENIALS" "$SCRATCH/journal.txt"; then
    fail "S3: the journal contains a sandbox denial"
    grep -nE "$DENIALS" "$SCRATCH/journal.txt" | sed 's/^/        /'
else
    pass "S3: no ProtectSystem, ReadWritePaths, namespace or seccomp denial"
fi
if grep -qE 'panicked|FATAL|error' "$SCRATCH/journal.txt"; then
    note "non-fatal lines worth reading:"
    grep -nE 'panicked|FATAL|error' "$SCRATCH/journal.txt" | sed 's/^/        /'
fi
sed 's/^/        /' "$SCRATCH/journal.txt"

# --------------------------------------------------------------------------
group "Sandbox report"
# --------------------------------------------------------------------------
systemd-analyze security --user "$UNIT_NAME" > "$SCRATCH/security.txt" 2>&1 || true
tail -1 "$SCRATCH/security.txt" | sed 's/^/  /'
note "full report follows"
sed 's/^/        /' "$SCRATCH/security.txt"

# --------------------------------------------------------------------------
group "Stop, and what systemd takes with it"
# --------------------------------------------------------------------------
systemctl --user stop "$UNIT_NAME" || true
for _ in $(seq 50); do [ -e "$RUNDIR" ] || break; sleep 0.1; done
if [ -e "$RUNDIR" ]; then
    fail "the runtime directory outlived the unit"
else
    pass "systemd removed the runtime directory with the unit"
fi
if [ -f "$PROBE_DATA/identity.key" ]; then
    pass "the data directory survived the stop (state is not systemd's to clean)"
else
    fail "the data directory did not survive the stop"
fi

# --------------------------------------------------------------------------
group "The real trust store was not touched"
# --------------------------------------------------------------------------
REAL_AFTER="$(fingerprint_real)"
if [ "$REAL_BEFORE" = "$REAL_AFTER" ]; then
    pass "$REAL_DATA is byte- and mode-identical to before the run"
else
    fail "$REAL_DATA CHANGED during the run"
    diff <(printf '%s\n' "$REAL_BEFORE") <(printf '%s\n' "$REAL_AFTER") | sed 's/^/        /' || true
fi

printf '\n%s\n' "-----------------------------------------------"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
