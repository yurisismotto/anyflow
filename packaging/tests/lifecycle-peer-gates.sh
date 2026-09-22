#!/usr/bin/env bash
# lifecycle-peer-gates.sh — lifecycle gates L12, L14, L15 and L16, which need a
# real second device: the physical Android peer, on the same LAN segment as the
# guest under test.
#
# These four are the gates no amount of container or VM work substitutes for.
# They need mDNS multicast and inbound TCP 55432 to flow between two real
# network stacks, which is why the guest is on macvtap over the wired NIC and
# not on libvirt NAT.
#
# The one step that cannot be automated is pairing: OmniBridge pairs by
# scanning a QR code with the phone's camera, and the Android app has no
# manual-entry path (checked: there is no text field and no deep link). This
# script therefore renders the guest's pairing payload as a QR on the HOST's
# screen, opens the phone's pairing screen over adb, and stops with an explicit
# OPERATOR ACTION REQUIRED block. It does not mark anything PASS while waiting.
#
# Same rule as every other harness here: a gate whose precondition is absent
# fails loudly. A PASS with zero observed evidence is INVALID.

set -uo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/guest-agent.sh
. "$HERE/lib/guest-agent.sh"

DOMAIN=""; DISTRO=""; EVIDENCE=""; PHONE_IP=""; ADB_SERIAL=""; PAIR_TTL=240
GUEST_USER="${GUEST_USER:-anyflow}"; GUEST_UID="${GUEST_UID:-1000}"
APP_PKG="io.github.yurisismotto.omnibridge"

while [ $# -gt 0 ]; do
    case "$1" in
        --domain) DOMAIN="$2"; shift 2 ;;
        --distro) DISTRO="$2"; shift 2 ;;
        --evidence) EVIDENCE="$2"; shift 2 ;;
        --phone-ip) PHONE_IP="$2"; shift 2 ;;
        --adb-serial) ADB_SERIAL="$2"; shift 2 ;;
        --pair-ttl) PAIR_TTL="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$DOMAIN" ] && [ -n "$DISTRO" ] && [ -n "$EVIDENCE" ] && [ -n "$PHONE_IP" ] \
    || { echo "usage: $0 --domain D --distro N --evidence DIR --phone-ip IP [--adb-serial S]" >&2; exit 2; }

mkdir -p "$EVIDENCE"
PASS=0; FAIL=0; NA=0; declare -a FAILED_GATES=()
ok()      { PASS=$(( PASS + 1 )); printf 'ok    %s\n' "$*"; }
notok()   { FAIL=$(( FAIL + 1 )); FAILED_GATES+=("$*"); printf 'not ok  %s\n' "$*"; }
na()      { NA=$(( NA + 1 ));   printf 'n/a   %s\n' "$*"; }
section() { printf '\n== %s ==\n' "$*"; }
abort()   { printf '\nPRECONDITION FAILED: %s\n' "$*" >&2; exit 3; }
save()    { cat > "$EVIDENCE/$1"; }

ADB=(adb); [ -n "$ADB_SERIAL" ] && ADB=(adb -s "$ADB_SERIAL")
gx() { ga_exec "$DOMAIN" "$@"; }
gu() {
    ga_exec "$DOMAIN" "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GUEST_UID/bus sh -c $(printf '%q' "$*")"
}
ob() { gu "omnibridge $*"; }

# ---------------------------------------------------------------------------
section "Preconditions"
# ---------------------------------------------------------------------------
ga_ping "$DOMAIN" 300 || abort "guest agent in '$DOMAIN' does not answer"
ok "the guest agent answers"

n_dev="$("${ADB[@]}" devices 2>/dev/null | grep -cw device || true)"
[ "${n_dev:-0}" -ge 1 ] 2>/dev/null || abort "adb sees no attached device; every phone gate would measure nothing"
phone_model="$("${ADB[@]}" shell getprop ro.product.model 2>/dev/null | tr -d '\r[:space:]')"
phone_rel="$("${ADB[@]}" shell getprop ro.build.version.release 2>/dev/null | tr -d '\r[:space:]')"
[ -n "$phone_model" ] || abort "adb returned no device model"
ok "physical Android attached: $phone_model, Android $phone_rel"

"${ADB[@]}" shell pm list packages 2>/dev/null | grep -qx "package:$APP_PKG" \
    || abort "$APP_PKG is not installed on the phone"
ok "$APP_PKG is installed on the phone"

phone_wifi="$("${ADB[@]}" shell ip -4 -br addr show wlan0 2>/dev/null | awk '{print $3}' | cut -d/ -f1 | tr -d '\r[:space:]')"
[ "$phone_wifi" = "$PHONE_IP" ] \
    || abort "the phone is at '${phone_wifi:-<none>}' but --phone-ip says $PHONE_IP"
ok "the phone is on the LAN at $phone_wifi"

guest_ip="$(gx "ip -4 -br addr show scope global | awk '{print \$3}' | cut -d/ -f1 | head -1" | tr -d '[:space:]')"
[ -n "$guest_ip" ] || abort "the guest has no global IPv4 address"
gx "ping -c 3 -W 3 $PHONE_IP >/dev/null 2>&1" || abort "the guest cannot reach the phone"
ok "guest $guest_ip reaches the phone at $PHONE_IP"

gx 'pgrep -x omnibridged >/dev/null' || abort "omnibridged is not running in the guest"
ok "omnibridged is running in the guest"

status_out="$(ob status 2>&1)"
[ -n "${status_out//[[:space:]]/}" ] || abort "'omnibridge status' produced no output in the guest"
printf '%s\n' "$status_out" | save "30-guest-status-before.txt"
guest_dev_name="$(printf '%s\n' "$status_out" | sed -n 's/^ *device *\([^ ]*\) .*/\1/p' | head -1)"
guest_dev_id="$(printf '%s\n' "$status_out"   | sed -n 's/.*device *[^ ]* *(\([0-9a-f]*\)).*/\1/p' | head -1)"
[ -n "$guest_dev_name" ] && [ -n "$guest_dev_id" ] \
    || abort "could not read the guest's device name/id from 'omnibridge status'"
ok "the guest advertises as '$guest_dev_name' ($guest_dev_id)"

# The phone's existing pairing must not be disturbed by anything here.
phone_pairs_before="$(ob devices 2>&1)"
printf '%s\n' "$phone_pairs_before" | save "31-guest-devices-before.txt"

# ---------------------------------------------------------------------------
section "L12 — the phone discovers the packaged desktop"
# ---------------------------------------------------------------------------
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 8
dump_ui() {
    "${ADB[@]}" shell uiautomator dump /sdcard/ob-ui.xml >/dev/null 2>&1 || return 1
    "${ADB[@]}" shell cat /sdcard/ob-ui.xml 2>/dev/null
}
ui=""
for _ in 1 2 3 4 5 6; do
    ui="$(dump_ui || true)"
    printf '%s' "$ui" | grep -qi "$guest_dev_name" && break
    sleep 10
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
done
[ -n "${ui//[[:space:]]/}" ] \
    || abort "uiautomator returned an empty hierarchy; a 'the phone sees it' claim would rest on nothing"
printf '%s\n' "$ui" | save "32-L12-phone-ui.xml"
ui_texts="$(printf '%s' "$ui" | grep -o 'text="[^"]*"' | sed 's/text="//;s/"$//' | grep -v '^$' | sort -u)"
n_texts="$(printf '%s\n' "$ui_texts" | grep -c . || true)"
ok "L12: the phone's UI hierarchy carries $n_texts distinct text nodes (a real dump, not an empty one)"

if printf '%s' "$ui" | grep -qi "$guest_dev_name"; then
    ok "L12: the phone lists the packaged desktop '$guest_dev_name'"
else
    notok "L12: '$guest_dev_name' does not appear in the phone's device list"
    printf '%s\n' "$ui_texts" | head -40 | save "33-L12-ui-texts.txt"
fi

# ---------------------------------------------------------------------------
section "Pairing the phone with the packaged desktop"
# ---------------------------------------------------------------------------
already="$(ob devices 2>&1 | grep -ci "$phone_model" || true)"
if [ "${already//[[:space:]]/}" -ge 1 ] 2>/dev/null; then
    ok "the phone is already paired with this guest; no operator action needed"
else
    payload_file="$EVIDENCE/34-pair-payload.txt"
    gu "nohup omnibridge pair --ttl $PAIR_TTL > /tmp/ob-pair.txt 2>&1 &" >/dev/null 2>&1
    sleep 6
    payload="$(gx 'sed -n "s/^ *\(omnibridge1:.*\)$/\1/p" /tmp/ob-pair.txt | head -1' | tr -d '[:space:]')"
    [ -n "$payload" ] \
        || abort "'omnibridge pair' produced no omnibridge1: payload in the guest; nothing can be displayed to scan"
    case "$payload" in omnibridge1:*) : ;; *) abort "pairing payload has an unexpected shape: $payload" ;; esac
    printf '%s\n' "$payload" > "$payload_file"
    ok "pairing payload obtained from the guest (${#payload} bytes, expires in ${PAIR_TTL}s)"

    qr_png="$EVIDENCE/35-pair-qr.png"
    command -v qrencode >/dev/null 2>&1 || abort "qrencode is not installed on the host; the QR cannot be shown"
    qrencode -o "$qr_png" -s 14 -m 4 "$payload" || abort "qrencode failed"
    [ -s "$qr_png" ] || abort "the rendered QR is empty"
    ( xdg-open "$qr_png" >/dev/null 2>&1 & ) || true
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.PairingCaptureActivity" >/dev/null 2>&1 \
        || "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1

    cat <<OPBLOCK

=========================================================
OPERATOR ACTION REQUIRED — scan the pairing QR
=========================================================
  Target machine   the HOST screen (Fedora 44) and the $phone_model tablet
  What to do       a QR code has been opened on this screen ($qr_png).
                   On the tablet, the OmniBridge pairing scanner has been
                   started over adb. Point the tablet's camera at the QR.
  Pairing with     $guest_dev_name ($guest_dev_id) at $guest_ip  [$DISTRO guest]
  Expected         the tablet shows the pairing confirmation and the guest's
                   device list gains an entry for $phone_model
  Time limit       ${PAIR_TTL}s -- the code is single-use and then expires
  If it expires    re-run this script; it opens a fresh window
  Rollback         'omnibridge unpair $phone_model' in the guest. The guest is
                   a throwaway VM; the tablet's pairing with the HOST daemon
                   (fedora) is NOT touched by this and must not be reset.
=========================================================

OPBLOCK
    waited=0
    while [ "$waited" -lt "$PAIR_TTL" ]; do
        if ob devices 2>/dev/null | grep -qi "$phone_model"; then break; fi
        sleep 5; waited=$(( waited + 5 ))
    done
    if ob devices 2>/dev/null | grep -qi "$phone_model"; then
        ok "the phone paired with $guest_dev_name after ${waited}s"
    else
        printf '\nPAIRING NOT COMPLETED within %ss. L14, L15 and L16 are left BLOCKED rather than marked passing.\n' "$PAIR_TTL" >&2
        notok "pairing with the guest did not complete; L14/L15/L16 BLOCKED"
        printf '\n%s / %s: %d passed, %d failed, %d n/a\n' "$DISTRO" "$DOMAIN" "$PASS" "$FAIL" "$NA"
        exit 4
    fi
fi

peer_id="$(ob devices 2>&1 | sed -n "s/^ *$phone_model *\([0-9a-f]\{16,\}\).*/\1/p" | head -1)"
[ -n "$peer_id" ] || abort "paired, but the peer's device id could not be read back"
ok "peer device id is $peer_id"
ob devices 2>&1 | save "36-guest-devices-after-pair.txt"

# Sentinels. High-entropy so that a match cannot be a pre-existing string and
# an absence cannot be luck. Each gate gets its own.
SENT_CLIP="OBCLIP-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_FILE_NAME="OBNAME-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_FILE_BODY="OBBODY-$(head -c 24 /dev/urandom | base32 | tr -d '=' | head -c 40)"
SENT_NOTIF="OBNOTIF-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
{ echo "clip      $SENT_CLIP"; echo "file-name $SENT_FILE_NAME"; echo "file-body $SENT_FILE_BODY"; echo "notif     $SENT_NOTIF"; } | save "37-sentinels.txt"

# ---------------------------------------------------------------------------
section "L14 — clipboard, both directions"
# ---------------------------------------------------------------------------
clip_status="$(ob clipboard status 2>&1)"
[ -n "${clip_status//[[:space:]]/}" ] || abort "'omnibridge clipboard status' produced no output"
printf '%s\n' "$clip_status" | save "40-L14-clipboard-status.txt"
ok "L14: 'omnibridge clipboard status' answers ($(printf '%s\n' "$clip_status" | grep -c .) line(s))"

# The gate asks specifically that status is HONEST about --sensitive. On
# Ubuntu/Debian wl-clipboard is 2.2.1 and has no --sensitive; the correct
# behaviour is to say so, not to pretend.
wlc_ver="$(gx 'wl-copy --version 2>&1 | head -1' || true)"
has_sensitive="$(gx 'wl-copy --help 2>&1 | grep -c -- "--sensitive" || true' | tr -d '[:space:]')"
if [ "${has_sensitive:-0}" = "0" ]; then
    if printf '%s' "$clip_status" | grep -qiE 'sensitive'; then
        ok "L14: wl-copy has no --sensitive (${wlc_ver:-unknown}) and status says so"
    else
        notok "L14: wl-copy has no --sensitive and status does not mention it (U-1 regression)"
    fi
else
    ok "L14: wl-copy supports --sensitive (${wlc_ver:-unknown})"
fi

ob "grant $peer_id clipboard.v1" >/dev/null 2>&1
ob "clipboard allow $peer_id" >/dev/null 2>&1

# guest -> phone
gu "wl-copy '$SENT_CLIP'" >/dev/null 2>&1
sleep 1
guest_clip="$(gu 'wl-paste -n 2>/dev/null' | tr -d '[:space:]')"
[ "$guest_clip" = "$SENT_CLIP" ] \
    || abort "the guest clipboard does not hold the sentinel after wl-copy; the send direction would measure nothing"
ok "L14: the guest clipboard holds the sentinel before sending"
"${ADB[@]}" logcat -c >/dev/null 2>&1
send_out="$(ob "clipboard send $peer_id" 2>&1)"; send_rc=$?
printf '%s\n' "$send_out" | save "41-L14-send-out.txt"
[ "$send_rc" -eq 0 ] \
    && ok "L14: guest -> phone clipboard send exited 0" \
    || notok "L14: guest -> phone clipboard send exited $send_rc: $send_out"
sleep 6
lc="$("${ADB[@]}" logcat -d -t 4000 2>/dev/null || true)"
[ -n "${lc//[[:space:]]/}" ] \
    || abort "adb logcat returned nothing; the phone-side half of L14 cannot be asserted"
n_lc="$(printf '%s\n' "$lc" | grep -c . || true)"
ok "L14: captured $n_lc logcat line(s) after the send (a real capture)"
if printf '%s' "$lc" | grep -qiE 'clipboard'; then
    ok "L14: the phone logged clipboard activity after the guest sent"
else
    notok "L14: no clipboard activity on the phone after the guest sent"
fi

# phone -> guest
gu "wl-copy ''" >/dev/null 2>&1
"${ADB[@]}" shell am start -a android.intent.action.SEND -t text/plain \
    --es android.intent.extra.TEXT "$SENT_CLIP" \
    -n "$APP_PKG/.ui.SendActivity" >/dev/null 2>&1
sleep 10
back="$(gu 'wl-paste -n 2>/dev/null' || true)"
if printf '%s' "$back" | grep -qF "$SENT_CLIP"; then
    ok "L14: phone -> guest clipboard arrived; the sentinel is in the guest clipboard"
else
    notok "L14: phone -> guest clipboard did not arrive (guest clipboard: '${back:0:60}')"
fi

# ---------------------------------------------------------------------------
section "L15 — files, each way"
# ---------------------------------------------------------------------------
ob "grant $peer_id files.v1" >/dev/null 2>&1
gx "mkdir -p /tmp/obsend && printf '%s\n' '$SENT_FILE_BODY' > /tmp/obsend/$SENT_FILE_NAME.txt && chown -R $GUEST_USER /tmp/obsend"
gx "test -s /tmp/obsend/$SENT_FILE_NAME.txt" \
    || abort "the file to send does not exist in the guest; L15 would measure nothing"
ok "L15: source file staged in the guest with a unique name and a unique body"

"${ADB[@]}" logcat -c >/dev/null 2>&1
fsend="$(ob "send $peer_id /tmp/obsend/$SENT_FILE_NAME.txt" 2>&1)"; fsend_rc=$?
printf '%s\n' "$fsend" | save "42-L15-send-out.txt"
[ "$fsend_rc" -eq 0 ] \
    && ok "L15: guest -> phone file send exited 0" \
    || notok "L15: guest -> phone file send exited $fsend_rc: $fsend"
sleep 12
phone_hit="$("${ADB[@]}" shell "find /sdcard/Download /sdcard/Documents -iname '*${SENT_FILE_NAME}*' 2>/dev/null | head -5" 2>/dev/null | tr -d '\r')"
if [ -n "${phone_hit//[[:space:]]/}" ]; then
    ok "L15: the file arrived on the phone at $phone_hit"
else
    lc2="$("${ADB[@]}" logcat -d -t 4000 2>/dev/null || true)"
    if printf '%s' "$lc2" | grep -qF "$SENT_FILE_NAME"; then
        ok "L15: the phone logged the transfer of $SENT_FILE_NAME (scoped storage hides the path from adb)"
    else
        notok "L15: no trace of $SENT_FILE_NAME on the phone, in storage or in logcat"
    fi
fi

# phone -> guest, and the mode the gate names
dl="/home/$GUEST_USER/Downloads/OmniBridge"
gx "rm -f $dl/*${SENT_FILE_NAME}* 2>/dev/null; true"
"${ADB[@]}" shell "mkdir -p /sdcard/Download && printf '%s' '$SENT_FILE_BODY' > /sdcard/Download/$SENT_FILE_NAME.txt" >/dev/null 2>&1
"${ADB[@]}" shell am start -a android.intent.action.SEND -t text/plain \
    --eu android.intent.extra.STREAM "file:///sdcard/Download/$SENT_FILE_NAME.txt" \
    -n "$APP_PKG/.ui.SendActivity" >/dev/null 2>&1
sleep 15
landed="$(gx "find $dl -iname '*${SENT_FILE_NAME}*' 2>/dev/null | head -1" | tr -d '[:space:]')"
if [ -n "$landed" ]; then
    ok "L15: phone -> guest file landed at $landed"
    mode="$(gx "stat -c '%a %U' '$landed'" | tr -d '\n')"
    [ "$mode" = "600 $GUEST_USER" ] \
        && ok "L15: it is 0600 and owned by $GUEST_USER" \
        || notok "L15: the received file is '$mode', the gate requires '600 $GUEST_USER'"
    gx "grep -qF '$SENT_FILE_BODY' '$landed'" \
        && ok "L15: the received file carries the sentinel body (content is intact)" \
        || notok "L15: the received file does not contain the sentinel body"
else
    notok "L15: phone -> guest file did not land in $dl (this direction needs an in-app approval on the tablet)"
fi

# ---------------------------------------------------------------------------
section "L16 — notification mirroring, and no content in the journal"
# ---------------------------------------------------------------------------
ob "grant $peer_id notifications.v1" >/dev/null 2>&1
ob "notifications allow $peer_id" >/dev/null 2>&1 || true
jnl_mark="$(gu 'date -u +%Y-%m-%d\ %H:%M:%S' | tr -d '\n')"
"${ADB[@]}" shell cmd notification post -S bigtext -t "$SENT_NOTIF" obgate "$SENT_NOTIF body" >/dev/null 2>&1
sleep 10
notif_list="$(ob "notifications list" 2>&1 || true)"
printf '%s\n' "$notif_list" | save "43-L16-notifications.txt"
if printf '%s' "$notif_list" | grep -qF "$SENT_NOTIF"; then
    ok "L16: the notification was mirrored to the packaged desktop"
elif printf '%s' "$notif_list" | grep -qiE 'notification'; then
    notok "L16: the desktop lists notifications but not the sentinel one"
else
    notok "L16: no mirrored notification observed on the desktop"
fi

# The privacy half. This must not pass on an empty journal.
jnl="$(gu "journalctl --user -u omnibridged --no-pager --since '$jnl_mark' 2>/dev/null" || true)"
if [ -z "${jnl//[[:space:]]/}" ]; then
    jnl="$(gu 'journalctl --user -u omnibridged --no-pager -n 500 2>/dev/null' || true)"
fi
[ -n "${jnl//[[:space:]]/}" ] \
    || abort "the daemon journal is empty for the window that covers the notification; 'no content in the journal' would be a vacuous PASS"
n_jnl="$(printf '%s\n' "$jnl" | grep -c . || true)"
ok "L16: captured $n_jnl journal line(s) covering the notification (non-vacuous)"
printf '%s\n' "$jnl" | save "44-L16-journal.txt"
if printf '%s' "$jnl" | grep -qF "$SENT_NOTIF"; then
    notok "L16/SEC-LOG-02: the notification sentinel appears in the daemon journal"
else
    ok "L16/SEC-LOG-02: the notification content is absent from $n_jnl journal lines"
fi

# ---------------------------------------------------------------------------
section "The host's own pairing must be untouched"
# ---------------------------------------------------------------------------
after_devs="$(ob devices 2>&1)"
printf '%s\n' "$after_devs" | save "45-guest-devices-final.txt"
ok "guest device list recorded; the HOST daemon's trust store is a separate file and was never opened by this script"

printf '\n-----------------------------------------------\n'
printf '%s / %s: %d passed, %d failed, %d n/a\n' "$DISTRO" "$DOMAIN" "$PASS" "$FAIL" "$NA"
if [ "$FAIL" -gt 0 ]; then printf '\nFailed:\n'; for g in "${FAILED_GATES[@]}"; do printf '  %s\n' "$g"; done; fi
printf 'evidence: %s\n' "$EVIDENCE"
[ "$FAIL" -eq 0 ]
