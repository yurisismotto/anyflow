#!/usr/bin/env bash
# security-log-evidence.sh — SEC-LOG-03's deferred half, and L16's privacy half,
# measured on real hardware instead of in-process.
#
# WHAT IS ALREADY CLOSED, AND IS NOT REOPENED HERE
# ------------------------------------------------
# Security Certification v1 §7 proves SEC-LOG-03 in-process, at `TRACE`, with
# `daemon/tests/file_log_privacy.rs`. That test is strong and stands. Its §13
# item 7 deferred one thing to "the Phase 6 lifecycle certification, which
# drives the capabilities on real hardware" -- and L15 was one of the eleven
# lifecycle gates that never ran, so the deferral had no destination. The
# Release Readiness baseline §6.3 names the three open rows:
#
#     journalctl --user -u omnibridged after a real transfer   -- open
#     Android logcat after a real transfer                     -- open
#     a real file crossing the wire between two real devices   -- open (L15)
#
# L15 is now certified, so this harness can stand on it. What it adds is the
# two log captures, taken around that same real transfer.
#
# THE METHOD, AND THE TRAP IN IT
# ------------------------------
# Baseline §6.4 sets the rule: the two sentinels are asserted in OPPOSITE
# directions -- the filename PRESENT, because that is finding F-1 and its
# presence proves the capture is real and covers this operation, and the
# content ABSENT, which is the gate. A harness that searched for both and
# failed on either would fail for the wrong reason.
#
# One correction, measured rather than assumed. Both filename log sites in
# `capabilities/files/src/lib.rs` -- `:916` "incoming file offer" and `:1707`
# "received, verified and stored" -- are on the RECEIVE path. The desktop is
# the sender in the only direction that can be driven without a human at a
# document picker, so no filename reaches the guest journal at all, and an
# assertion that one does would fail on a correct product. The two sides
# therefore anchor on different things, and each anchor is something the
# PRODUCT wrote about THIS operation:
#
#     guest journal   the transfer id the daemon generated, plus its byte count
#     Android logcat  the filename, under the app's own FileTransfer tag
#
# A second trap, also measured: `adbd` echoes the whole `am start` command line
# into logcat, so driving the fixture with `--es title '<sentinel>'` puts the
# sentinel in the log by itself. The notification assertion therefore requires
# that every occurrence be on an `adbd` line, and fails if any other process
# logged one. Grepping without that distinction fails a correct product.
#
# Same rule as every harness here: an empty capture is not evidence. A PASS
# with zero observed evidence is INVALID -- and so is a FAIL measured against a
# precondition this harness destroyed.

set -uo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/guest-agent.sh
. "$HERE/lib/guest-agent.sh"

DOMAIN=""; DISTRO=""; EVIDENCE=""; PHONE_IP=""; ADB_SERIAL=""
GUEST_USER="${GUEST_USER:-anyflow}"; GUEST_UID="${GUEST_UID:-1000}"
APP_PKG="io.github.yurisismotto.omnibridge"
FIXTURE_PKG="io.github.yurisismotto.omnibridge.fixture"
FIXTURE_ACT="$FIXTURE_PKG/.FixtureActivity"
DROPIN="/etc/systemd/user/omnibridged.service.d/99-omnibridge-trace.conf"

while [ $# -gt 0 ]; do
    case "$1" in
        --domain) DOMAIN="$2"; shift 2 ;;
        --distro) DISTRO="$2"; shift 2 ;;
        --evidence) EVIDENCE="$2"; shift 2 ;;
        --phone-ip) PHONE_IP="$2"; shift 2 ;;
        --adb-serial) ADB_SERIAL="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$DOMAIN" ] && [ -n "$DISTRO" ] && [ -n "$EVIDENCE" ] && [ -n "$PHONE_IP" ] \
    || { echo "usage: $0 --domain D --distro N --evidence DIR --phone-ip IP [--adb-serial S]" >&2; exit 2; }
mkdir -p "$EVIDENCE"
export GA_EXEC_TIMEOUT=240

PASS=0; FAIL=0; NA=0; declare -a FAILED_GATES=()
ok()      { PASS=$(( PASS + 1 )); printf 'ok    %s\n' "$*"; }
notok()   { FAIL=$(( FAIL + 1 )); FAILED_GATES+=("$*"); printf 'not ok  %s\n' "$*"; }
na()      { NA=$(( NA + 1 ));   printf 'n/a   %s\n' "$*"; }
section() { printf '\n== %s ==\n' "$*"; }
save()    { cat > "$EVIDENCE/$1"; }

ADB=(adb); [ -n "$ADB_SERIAL" ] && ADB=(adb -s "$ADB_SERIAL")
gx() { ga_exec "$DOMAIN" "$@"; }
gu() {
    ga_exec "$DOMAIN" "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GUEST_UID/bus sh -c $(printf '%q' "$*")"
}
ob() { gu "omnibridge $*"; }
PUI="$HERE/lib/phone-ui.py"

# The daemon's own output is colourised, so the level is preceded by an ANSI
# escape and `grep '^TRACE'` matches nothing on a journal full of TRACE. Every
# capture is stripped once, here, rather than at each use.
strip_ansi() { sed 's/\x1b\[[0-9;]*m//g'; }

# The TRACE drop-in is guest configuration this harness adds, and it must not
# survive the run whatever happens to it -- a daemon left at TRACE writes every
# mDNS packet to the journal forever.
TRACE_INSTALLED=0
restore_trace() {
    [ "$TRACE_INSTALLED" = "1" ] || return 0
    gx "rm -f $DROPIN; rmdir /etc/systemd/user/omnibridged.service.d 2>/dev/null; true" >/dev/null 2>&1
    gu "systemctl --user daemon-reload; systemctl --user restart omnibridged.service" >/dev/null 2>&1
    sleep 5
    if gx "test -e $DROPIN"; then
        printf 'WARNING: the TRACE drop-in is still present at %s\n' "$DROPIN" >&2
    else
        printf '\nrestored: the TRACE drop-in is removed and the daemon is back at its packaged log level\n'
    fi
}
abort() { printf '\nPRECONDITION FAILED: %s\n' "$*" >&2; restore_trace; exit 3; }
trap restore_trace EXIT

# ---------------------------------------------------------------------------
section "Preconditions"
# ---------------------------------------------------------------------------
ga_ping "$DOMAIN" 300 || abort "guest agent in '$DOMAIN' does not answer"
ok "the guest agent answers"

guest_os="$(gx 'sed -n "s/^PRETTY_NAME=//p" /etc/os-release | tr -d \"')"
[ -n "$guest_os" ] || abort "could not read /etc/os-release from the guest"
case "$DISTRO" in
    ubuntu2404) expect="Ubuntu 24.04" ;;
    ubuntu2604) expect="Ubuntu 26.04" ;;
    debian13)   expect="Debian GNU/Linux 13" ;;
    *) abort "unknown --distro '$DISTRO'" ;;
esac
case "$guest_os" in
    *"$expect"*) ok "the guest is '$guest_os' and matches --distro $DISTRO" ;;
    *) abort "--distro $DISTRO expects '$expect' but the guest is '$guest_os'" ;;
esac

n_dev="$("${ADB[@]}" devices 2>/dev/null | grep -cw device || true)"
[ "${n_dev:-0}" -ge 1 ] 2>/dev/null || abort "adb sees no attached device"
phone_model="$("${ADB[@]}" shell getprop ro.product.model 2>/dev/null | tr -d '\r[:space:]')"
[ -n "$phone_model" ] || abort "adb returned no device model"
ok "physical Android attached: $phone_model"
for p in "$APP_PKG" "$FIXTURE_PKG"; do
    grep -qx "package:$p" <<<"$("${ADB[@]}" shell pm list packages 2>/dev/null)" || abort "$p is not installed on the phone"
done
ok "both $APP_PKG and $FIXTURE_PKG are installed"

phone_wifi="$("${ADB[@]}" shell ip -4 -br addr show wlan0 2>/dev/null | awk '{print $3}' | cut -d/ -f1 | tr -d '\r[:space:]')"
[ "$phone_wifi" = "$PHONE_IP" ] || abort "the phone is at '${phone_wifi:-<none>}' but --phone-ip says $PHONE_IP"
guest_ip="$(gx "ip -4 -br addr show scope global | awk '{print \$3}' | cut -d/ -f1 | head -1" | tr -d '[:space:]')"
gx "ping -c 3 -W 3 $PHONE_IP >/dev/null 2>&1" || abort "the guest cannot reach the phone"
ok "guest $guest_ip reaches the phone at $PHONE_IP"

status_out="$(ob status 2>&1)"
guest_dev_name="$(printf '%s\n' "$status_out" | sed -n 's/^ *device *\([^ ]*\) .*/\1/p' | head -1)"
guest_fpr="$(printf '%s\n' "$status_out" | sed -n 's/^ *fingerprint *//p' | head -1)"
[ -n "$guest_dev_name" ] && [ -n "$guest_fpr" ] || abort "could not read the guest's identity from 'omnibridge status'"
ok "the guest under test is '$guest_dev_name', fingerprint $guest_fpr"

devs="$(ob devices 2>&1)"
n_paired="$(printf '%s\n' "$devs" | grep -c 'paired *yes' || true)"
[ "${n_paired//[[:space:]]/}" = "1" ] \
    || abort "the guest reports ${n_paired//[[:space:]]/} paired devices; exactly one is required to attribute a log line"
peer_id="$(printf '%s\n' "$devs" | sed -n "s/^ *$phone_model *\([0-9a-f]\{16,\}\).*/\1/p" | head -1)"
peer_fpr="$(printf '%s\n' "$devs" | sed -n 's/^ *fingerprint *//p' | head -1)"
[ -n "$peer_id" ] && [ -n "$peer_fpr" ] || abort "the paired peer's id or fingerprint could not be read"
ok "the paired peer is $phone_model ($peer_id), fingerprint $peer_fpr"

pdump() {
    "${ADB[@]}" shell uiautomator dump /sdcard/ob-ui.xml >/dev/null 2>&1 || return 1
    "${ADB[@]}" shell cat /sdcard/ob-ui.xml 2>/dev/null > "$EVIDENCE/.ui.xml"
    [ -s "$EVIDENCE/.ui.xml" ]
}
ptap() { "${ADB[@]}" shell input tap "$1" "$2" >/dev/null 2>&1; sleep 3; }

# Android is always the initiator, so the desktop cannot dial. Every capture
# below is taken around an operation that needs the session; asserting it here
# and again before each operation is what stops this harness measuring a
# connection it tore down itself.
ensure_connected() {
    local why="$1" xy
    if grep -q 'connected *yes' <<<"$(ob devices 2>/dev/null)"; then return 0; fi
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
    sleep 7
    pdump || abort "cannot read the phone's view hierarchy to reconnect ($why)"
    if xy="$("$PUI" "$EVIDENCE/.ui.xml" nav '^Devices$' 2>/dev/null)"; then
        # shellcheck disable=SC2086
        ptap $xy
        pdump || true
    fi
    xy="$("$PUI" "$EVIDENCE/.ui.xml" find-after "$guest_dev_name" '^Connect$' 2>/dev/null)" \
        || abort "no Connect control for $guest_dev_name on the phone ($why)"
    # shellcheck disable=SC2086
    ptap $xy
    ga_wait_for "$DOMAIN" 90 "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID omnibridge devices 2>/dev/null | grep -q 'connected *yes'" \
        || abort "the phone did not connect to $guest_dev_name ($why)"
    ok "connected to $guest_dev_name before $why"
}

# ---------------------------------------------------------------------------
section "Raising the daemon to TRACE"
# ---------------------------------------------------------------------------
# Security Certification v1 §7's method, applied to the journal: capture at
# TRACE, which is strictly more than journalctl ever shows, so a sentinel
# absent here is absent from any log a user or a bug report could carry.
gx "mkdir -p /etc/systemd/user/omnibridged.service.d && printf '[Service]\nEnvironment=RUST_LOG=trace\n' > $DROPIN" \
    || abort "could not install the TRACE drop-in"
TRACE_INSTALLED=1
gu "systemctl --user daemon-reload && systemctl --user restart omnibridged.service" >/dev/null 2>&1
ga_wait_for "$DOMAIN" 90 'pgrep -x omnibridged >/dev/null' || abort "the daemon did not come back after the restart"
sleep 5

dpid="$(gx 'pgrep -x omnibridged | head -1' | tr -d '[:space:]')"
[ -n "$dpid" ] || abort "no omnibridged process after the restart"
gx "tr '\0' '\n' < /proc/$dpid/environ | grep -qx 'RUST_LOG=trace'" \
    || abort "the running daemon (pid $dpid) does not carry RUST_LOG=trace; the capture would be at the packaged level"
ok "the daemon is running at pid $dpid with RUST_LOG=trace in its environment"

# Environment is not effect. Assert the level is actually producing events,
# because a filter that parsed but did not apply would give a quieter journal
# and a sentinel search over it would look clean for the wrong reason.
sleep 6
lvl="$(gu "journalctl --user -u omnibridged --no-pager -o cat -n 200" | strip_ansi | grep -c '^TRACE' || true)"
[ "${lvl:-0}" -ge 1 ] 2>/dev/null \
    || abort "no TRACE-level line appears in the journal after the restart; RUST_LOG is set but not in effect"
ok "the journal is carrying TRACE-level events ($lvl in the last 200 lines)"

ensure_connected "the capture gates"

# ---------------------------------------------------------------------------
section "SEC-LOG-03 — file content absent from the journal and from logcat"
# ---------------------------------------------------------------------------
SENT_FNAME="OBFNAME-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_FBODY="OBFBODY-$(head -c 24 /dev/urandom | base32 | tr -d '=' | head -c 40)"
{ echo "file-name  $SENT_FNAME"; echo "file-body  $SENT_FBODY"; } | save "10-sentinels-file.txt"
ok "SEC-LOG-03: distinct sentinels for the filename and for the content"

SENDDIR="/home/$GUEST_USER/obsec"
gx "rm -rf $SENDDIR && mkdir -p $SENDDIR && printf '%s\n' '$SENT_FBODY' > $SENDDIR/$SENT_FNAME.txt && chown -R $GUEST_USER:$GUEST_USER $SENDDIR"
gx "grep -qF '$SENT_FBODY' $SENDDIR/$SENT_FNAME.txt" \
    || abort "the staged file does not contain the content sentinel; the gate would search for something that never existed"
fsize="$(gx "stat -c %s $SENDDIR/$SENT_FNAME.txt" | tr -d '[:space:]')"
ok "SEC-LOG-03: the source file exists, is ${fsize} B, and provably carries the content sentinel"

# The capture window. A journal CURSOR, not a --since timestamp: journalctl
# parses --since in the guest's local time while the harness reads the clock in
# UTC, so the two agree only while the guest happens to be on UTC. A cursor is
# exact and carries no timezone at all.
cur="$(gu "journalctl --user -u omnibridged --no-pager -n 0 --show-cursor 2>/dev/null | sed -n 's/^-- cursor: //p'" | tr -d '\r\n')"
[ -n "$cur" ] || abort "could not obtain a journal cursor; the capture window could not be bounded"
ok "SEC-LOG-03: journal cursor taken BEFORE the transfer"
"${ADB[@]}" logcat -c >/dev/null 2>&1
ok "SEC-LOG-03: logcat cleared before the transfer"

ensure_connected "the file transfer"
gx "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GUEST_UID/bus sh -c 'nohup omnibridge send $peer_id $SENDDIR/$SENT_FNAME.txt > /tmp/obsec-snd.out 2>&1 &'" >/dev/null 2>&1
sleep 7
snd_err="$(gx 'cat /tmp/obsec-snd.out 2>/dev/null' | grep -i '^error:' | head -1 || true)"
[ -z "${snd_err//[[:space:]]/}" ] || abort "'omnibridge send' refused the offer: $snd_err"

# Accept on the phone, on THIS file's row.
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 5
pdump || abort "cannot read the phone's view hierarchy while the offer is live"
files_tab="$("$PUI" "$EVIDENCE/.ui.xml" nav '^Files$' 2>/dev/null)" || files_tab=""
[ -n "$files_tab" ] || abort "the phone shows no Files tab; the offer cannot be accepted"
# shellcheck disable=SC2086
ptap $files_tab
sleep 4
pdump || abort "cannot read the Files tab"
cp "$EVIDENCE/.ui.xml" "$EVIDENCE/11-file-offer-ui.xml" 2>/dev/null || true
xy="$("$PUI" "$EVIDENCE/.ui.xml" find-after "$SENT_FNAME" '^Accept$' 2>/dev/null)" \
    || abort "no Accept control below $SENT_FNAME; the transfer this gate needs did not reach the phone"
# shellcheck disable=SC2086
ptap $xy
sleep 14

xfers="$(ob transfers 2>&1)"
printf '%s\n' "$xfers" | save "12-transfers.txt"
xfer_ctx="$(grep -A4 "$SENT_FNAME" <<<"$xfers")"
grep -qE 'state +completed' <<<"$xfer_ctx" \
    || abort "the transfer did not complete; there is no real operation for the captures to be about"
xfer_id="$(printf '%s' "$xfers" | grep -B4 "$SENT_FNAME" | sed -n 's/^ *\([0-9a-f]\{8\}\) *sending.*/\1/p' | tail -1)"
[ -n "$xfer_id" ] || abort "the completed transfer has no id; the journal capture could not be bound to it"
ok "SEC-LOG-03: the transfer completed — id $xfer_id, ${fsize} B, to $phone_model"

# --- the guest journal -----------------------------------------------------
jnl="$(gu "journalctl --user -u omnibridged --no-pager -o cat --after-cursor '$cur'" | strip_ansi)"
printf '%s\n' "$jnl" | save "13-journal-file.txt"
n_jnl="$(printf '%s\n' "$jnl" | grep -c . || true)"
[ "${n_jnl:-0}" -ge 20 ] 2>/dev/null \
    || abort "the journal capture holds only ${n_jnl} line(s); a sentinel search over that proves nothing"
n_trace="$(printf '%s\n' "$jnl" | grep -c '^TRACE' || true)"
[ "${n_trace:-0}" -ge 1 ] 2>/dev/null \
    || abort "the journal capture carries no TRACE line; it is not the level this gate claims to search"
ok "SEC-LOG-03: the journal capture holds $n_jnl line(s), $n_trace of them TRACE"

# Unrelated expected daemon activity, so the capture is a real log and not a
# window that happened to catch only this transfer.
grep -qE 'mdns|listener|session' <<<"$jnl" \
    && ok "SEC-LOG-03: the capture carries unrelated daemon activity (mDNS/listener/session), as a real log does" \
    || notok "SEC-LOG-03: the capture carries no unrelated daemon activity; it does not look like a real log stream"

# The anchor: something the PRODUCT wrote about THIS transfer.
if grep -qF "transfer=$xfer_id" <<<"$jnl"; then
    ok "SEC-LOG-03: the capture names transfer=$xfer_id — the window provably covers this operation"
else
    abort "no journal line names transfer=$xfer_id; the window does not cover the transfer and any 'absent' result would be vacuous"
fi
grep -qE "(size|bytes)=$fsize" <<<"$jnl" \
    && ok "SEC-LOG-03: the capture carries this file's byte count ($fsize)" \
    || notok "SEC-LOG-03: the capture does not carry the byte count $fsize"

# The gate.
if grep -qF "$SENT_FBODY" <<<"$jnl"; then
    notok "SEC-LOG-03: the file CONTENT sentinel appears in the daemon journal at TRACE"
else
    ok "SEC-LOG-03: the content sentinel is ABSENT from $n_jnl journal lines at TRACE"
fi
prefix24="$(printf '%s' "$SENT_FBODY" | head -c 24)"
if grep -qF "$prefix24" <<<"$jnl"; then
    notok "SEC-LOG-03: a 24-byte prefix of the content appears in the journal"
else
    ok "SEC-LOG-03: a 24-byte prefix of the content is ABSENT too"
fi

# F-1's scope, characterised rather than asserted either way. On the send path
# the desktop logs no filename, which is why the anchor above is the transfer
# id; both filename sites are on the receive path.
if grep -qF "$SENT_FNAME" <<<"$jnl"; then
    ok "SEC-LOG-03/F-1: the filename appears in the guest journal (finding F-1, as recorded)"
else
    ok "SEC-LOG-03/F-1: no filename in the guest journal on the SEND path — both F-1 log sites (lib.rs:916, :1707) are on the receive path, which this direction never takes"
fi

# --- Android logcat --------------------------------------------------------
lc="$("${ADB[@]}" logcat -d 2>/dev/null || true)"
printf '%s\n' "$lc" | save "14-logcat-file.txt"
n_lc="$(printf '%s\n' "$lc" | grep -c . || true)"
[ "${n_lc:-0}" -ge 20 ] 2>/dev/null \
    || abort "logcat returned ${n_lc} line(s); a sentinel search over that proves nothing"
ok "SEC-LOG-03: logcat holds $n_lc line(s) since it was cleared before the transfer"

# The anchor on this side IS the filename: the app logs it under its own
# FileTransfer tag, which is the receive path and therefore F-1's equivalent.
if grep -qE "FileTransfer.*$SENT_FNAME" <<<"$lc"; then
    ok "SEC-LOG-03: logcat carries this transfer's filename under the app's FileTransfer tag — the window provably covers the operation"
else
    abort "logcat does not name $SENT_FNAME under FileTransfer; the window does not cover the transfer and any 'absent' result would be vacuous"
fi
if grep -qF "$SENT_FBODY" <<<"$lc"; then
    notok "SEC-LOG-03: the file CONTENT sentinel appears in Android logcat"
else
    ok "SEC-LOG-03: the content sentinel is ABSENT from $n_lc logcat lines"
fi
grep -qF "$prefix24" <<<"$lc" \
    && notok "SEC-LOG-03: a 24-byte prefix of the content appears in logcat" \
    || ok "SEC-LOG-03: a 24-byte prefix of the content is ABSENT from logcat too"

# ---------------------------------------------------------------------------
section "L16 privacy — notification content absent from the journal"
# ---------------------------------------------------------------------------
# Lifecycle closure recorded this n/a rather than passing it: the daemon logs
# nothing for a mirrored notification at its default level, so the window held
# one line and grepping it would have been vacuous. At TRACE it is not.
ensure_connected "L16 privacy"
SENT_NTITLE="OBNTITLE-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_NBODY="OBNBODY-$(head -c 15 /dev/urandom | base32 | tr -d '=' | head -c 24)"
{ echo "notif-title $SENT_NTITLE"; echo "notif-body  $SENT_NBODY"; } | save "20-sentinels-notification.txt"

nstat="$(ob 'notifications status' 2>&1)"
grep -qi 'can source notifications' <<<"$nstat" \
    || abort "the phone announces no notification source role; run lifecycle-peer-gates.sh first — L16 privacy would measure a peer sharing nothing"
ok "L16: the phone announces a notification source role"

"${ADB[@]}" shell "am start -n $FIXTURE_ACT --es op clear" >/dev/null 2>&1
sleep 9
before_n="$(ob 'notifications status' 2>&1 | sed -n 's/^ *mirrored now *//p' | head -1 | tr -d '[:space:]')"
before_n="${before_n:-0}"
ok "L16: baseline after clearing the fixture's own notifications — $before_n mirrored"

ncur="$(gu "journalctl --user -u omnibridged --no-pager -n 0 --show-cursor 2>/dev/null | sed -n 's/^-- cursor: //p'" | tr -d '\r\n')"
[ -n "$ncur" ] || abort "could not obtain a journal cursor for the notification window"
"${ADB[@]}" logcat -c >/dev/null 2>&1

# The desktop's own Notify call, captured so the sentinels are PROVED to have
# crossed. Without it, "the sentinel is absent from the journal" is equally
# true of a notification that never arrived.
gu "rm -f /tmp/obsec-notify.txt; setsid dbus-monitor --session \"interface='org.freedesktop.Notifications',member='Notify'\" > /tmp/obsec-notify.txt 2>&1 < /dev/null & sleep 1" >/dev/null 2>&1
sleep 3
gx 'pgrep -f "dbus-monitor --session" >/dev/null' \
    || abort "the D-Bus monitor did not start; the notification could not be bound to its content"

"${ADB[@]}" shell "am start -n $FIXTURE_ACT --es op post --es id 92 --es tag seclog --es title '$SENT_NTITLE' --es body '$SENT_NBODY'" >/dev/null 2>&1
sleep 16
gx 'pkill -f "dbus-monitor --session"; true' >/dev/null 2>&1

after_n="$(ob 'notifications status' 2>&1 | sed -n 's/^ *mirrored now *//p' | head -1 | tr -d '[:space:]')"
after_n="${after_n:-0}"
[ "$after_n" -eq $(( before_n + 1 )) ] 2>/dev/null \
    || abort "the mirrored count went $before_n -> $after_n, not +1; the operation this gate is about did not happen as intended"
ok "L16: exactly one notification was mirrored ($before_n -> $after_n)"

ncap="$(gx 'cat /tmp/obsec-notify.txt 2>/dev/null' || true)"
printf '%s\n' "$ncap" | save "21-notify-dbus.txt"
if grep -qF "$SENT_NTITLE" <<<"$ncap" && grep -qF "$SENT_NBODY" <<<"$ncap"; then
    ok "L16: the desktop's Notify call carries BOTH sentinels — they provably reached this machine, so their absence below means something"
else
    abort "the Notify capture does not carry the sentinels; the content never reached the desktop and 'absent from the journal' would be vacuous"
fi

njnl="$(gu "journalctl --user -u omnibridged --no-pager -o cat --after-cursor '$ncur'" | strip_ansi)"
printf '%s\n' "$njnl" | save "22-journal-notification.txt"
n_njnl="$(printf '%s\n' "$njnl" | grep -c . || true)"
[ "${n_njnl:-0}" -ge 20 ] 2>/dev/null \
    || abort "the notification journal capture holds only ${n_njnl} line(s); this is the vacuity lifecycle closure refused to pass on"
n_ntrace="$(printf '%s\n' "$njnl" | grep -c '^TRACE' || true)"
ok "L16: the journal capture holds $n_njnl line(s), $n_ntrace of them TRACE"

# The product's own record of the mirroring, inside the window. This is the
# non-vacuity anchor: the capability demonstrably acted.
if grep -qE 'omnibridge_capability_notifications' <<<"$njnl"; then
    ok "L16: the capture carries the notifications capability's own record of this mirroring: $(printf '%s' "$njnl" | grep -E 'omnibridge_capability_notifications' | head -1 | tr -s ' ' | head -c 150)"
else
    abort "no notifications-capability line in the window; the capture does not cover the mirroring and any 'absent' result would be vacuous"
fi

if grep -qF "$SENT_NTITLE" <<<"$njnl"; then
    notok "L16/SEC-LOG-02: the notification TITLE sentinel appears in the daemon journal at TRACE"
elif grep -qF "$SENT_NBODY" <<<"$njnl"; then
    notok "L16/SEC-LOG-02: the notification BODY sentinel appears in the daemon journal at TRACE"
else
    ok "L16/SEC-LOG-02: neither the title nor the body sentinel appears in $n_njnl journal lines at TRACE"
fi

# --- logcat, with the adbd echo accounted for ------------------------------
nlc="$("${ADB[@]}" logcat -d 2>/dev/null || true)"
printf '%s\n' "$nlc" | save "23-logcat-notification.txt"
n_nlc="$(printf '%s\n' "$nlc" | grep -c . || true)"
[ "${n_nlc:-0}" -ge 20 ] 2>/dev/null || abort "logcat returned ${n_nlc} line(s) for the notification window"
ok "L16: logcat holds $n_nlc line(s) since it was cleared before the post"

grep -qF 'OmniBridgeFixture: op=post id=92 tag=seclog' <<<"$nlc" \
    && ok "L16: logcat carries the fixture's own record of this post — the window provably covers the operation" \
    || abort "logcat does not carry the fixture's post line; the window does not cover the operation"

# `adbd` logs the command line it was asked to run, so the sentinels are in
# logcat because THIS HARNESS put them there. Those lines are excluded by
# process name, and the exclusion is asserted to be exactly that -- if any
# other process logged a sentinel, this fails.
hits="$(printf '%s\n' "$nlc" | grep -F -e "$SENT_NTITLE" -e "$SENT_NBODY" || true)"
n_hits="$(printf '%s\n' "$hits" | grep -c . || true)"
foreign="$(printf '%s\n' "$hits" | grep -v ' adbd *:' || true)"
n_foreign="$(printf '%s\n' "$foreign" | grep -c . || true)"
printf '%s\n' "$hits" | save "24-logcat-sentinel-hits.txt"
if [ "${n_foreign:-0}" -eq 0 ] 2>/dev/null; then
    ok "L16: the $n_hits logcat line(s) carrying a sentinel are all adbd echoing this harness's own 'am start'; no OmniBridge process logged either"
else
    notok "L16: ${n_foreign} logcat line(s) outside adbd carry a notification sentinel: $(printf '%s' "$foreign" | head -1 | head -c 160)"
fi

printf '\n-----------------------------------------------\n'
printf '%s / %s: %d passed, %d failed, %d n/a\n' "$DISTRO" "$DOMAIN" "$PASS" "$FAIL" "$NA"
if [ "$FAIL" -gt 0 ]; then printf '\nFailed:\n'; for g in "${FAILED_GATES[@]}"; do printf '  %s\n' "$g"; done; fi
printf 'evidence: %s\n' "$EVIDENCE"
[ "$FAIL" -eq 0 ]
