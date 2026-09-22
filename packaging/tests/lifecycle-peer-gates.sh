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

# A capability exercise that has not answered in three minutes is hung, not
# slow. The default 15-minute ceiling turns a hang into a run that simply
# stops, with the last line printed looking exactly like a pass.
GA_EXEC_TIMEOUT="${GA_EXEC_TIMEOUT:-180}"

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
guest_fpr="$(printf '%s\n' "$status_out" | sed -n 's/^ *fingerprint *//p' | head -1)"
[ -n "$guest_dev_name" ] && [ -n "$guest_dev_id" ] && [ -n "$guest_fpr" ] \
    || abort "could not read the guest's device name, id and fingerprint from 'omnibridge status'"
ok "the guest under test is '$guest_dev_name' ($guest_dev_id), fingerprint $guest_fpr"

# The identity must belong to THIS guest and no other. Two guests sharing a
# device id would mean a cloned trust store, and every peer assertion below
# would be measuring the wrong machine.
guest_host="$(gx 'hostname' | tr -d '[:space:]')"
case "$guest_dev_name" in
    "$guest_host") ok "the advertised device name matches the guest's hostname ($guest_host)" ;;
    *) abort "the daemon advertises '$guest_dev_name' but the guest's hostname is '$guest_host'" ;;
esac
{
  echo "distro        $DISTRO"
  echo "domain        $DOMAIN"
  echo "hostname      $guest_host"
  echo "device_name   $guest_dev_name"
  echo "device_id     $guest_dev_id"
  echo "fingerprint   $guest_fpr"
  echo "guest_ip      $guest_ip"
  echo "phone         $phone_model  $PHONE_IP"
  echo "recorded_at   $(date -u +%FT%TZ)"
} | save "29-peer-identity.txt"

# No stale peer may be carried in from an earlier distribution's run.
phone_pairs_before="$(ob devices 2>&1)"
printf '%s\n' "$phone_pairs_before" | save "31-guest-devices-before.txt"
stale="$(printf '%s\n' "$phone_pairs_before" | grep -c 'paired *yes' || true)"
if [ "${stale//[[:space:]]/}" -gt 1 ] 2>/dev/null; then
    abort "the guest already has ${stale} paired devices; this run cannot tell which peer a capability result came from"
fi

# ---------------------------------------------------------------------------
# Is the phone already paired with THIS guest? Decided from the guest's own
# trust store, before anything on the phone is inspected, so a re-run after a
# successful scan does not trip the "must be absent" baseline below.
ALREADY_PAIRED=0
if printf '%s\n' "$phone_pairs_before" | grep -qi "$phone_model"; then
    ALREADY_PAIRED=1
fi

section "L12 — baseline: what the phone lists BEFORE this guest is paired"
# ---------------------------------------------------------------------------
# The Android app lists PAIRED desktops; it has no browse list of unpaired
# ones, because pairing is by QR and the payload carries the address. So
# "the desktop appears" is asserted after pairing -- and it only means
# something if the desktop was demonstrably absent beforehand. That baseline
# is taken here.
dump_ui() {
    "${ADB[@]}" shell uiautomator dump /sdcard/ob-ui.xml >/dev/null 2>&1 || return 1
    "${ADB[@]}" shell cat /sdcard/ob-ui.xml 2>/dev/null
}
ui_texts() {
    printf '%s' "$1" | grep -o 'text="[^"]*"' | sed 's/text="//;s/"$//' | grep -v '^$' | sort -u
}

"${ADB[@]}" shell am force-stop "$APP_PKG" >/dev/null 2>&1
sleep 2
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 10
ui_before="$(dump_ui || true)"
[ -n "${ui_before//[[:space:]]/}" ] \
    || abort "uiautomator returned an empty hierarchy; a 'the phone sees it' claim would rest on nothing"
printf '%s\n' "$ui_before" | save "32-L12-phone-ui-before.xml"
n_texts="$(ui_texts "$ui_before" | grep -c . || true)"
[ "${n_texts:-0}" -ge 5 ] 2>/dev/null \
    || abort "the phone's UI hierarchy carries only ${n_texts} text nodes; that is not a real dump"
ok "L12: the phone's UI dump is real ($n_texts distinct text nodes)"

if [ "$ALREADY_PAIRED" = "1" ]; then
    printf '%s' "$ui_before" | grep -qi "$guest_dev_name" \
        && ok "L12: the phone is already paired with '$guest_dev_name' from an earlier run of this script, and lists it" \
        || abort "the guest's trust store names the phone, but the phone does not list '$guest_dev_name'; the two disagree about who is paired"
elif printf '%s' "$ui_before" | grep -qi "$guest_dev_name"; then
    abort "the phone lists '$guest_dev_name' but the guest's trust store has no pairing for $phone_model; a stale entry would make every gate below unattributable"
else
    ok "L12: the phone does NOT list '$guest_dev_name' yet — the baseline is clean"
fi

# The host Fedora daemon's pairing is the one this wave must never disturb.
# It is recorded here and checked again at the end.
if printf '%s' "$ui_before" | grep -qi 'fedora'; then
    HOST_PAIRING_PRESENT=1
    ok "L12: the tablet's existing pairing with the host 'fedora' daemon is present before this run"
else
    HOST_PAIRING_PRESENT=0
    na "L12: no 'fedora' entry on the tablet before this run; nothing to preserve"
fi

# ---------------------------------------------------------------------------
section "Pairing the phone with the packaged desktop"
# ---------------------------------------------------------------------------
if [ "$ALREADY_PAIRED" = "1" ]; then
    ok "the phone is already paired with this guest; no operator action needed"
else
    payload_file="$EVIDENCE/34-pair-payload.txt"
    # `omnibridge pair` prints "Pair with this device? [y/N]" and reads the
    # answer from STDIN with a 60 s timeout. Backgrounded with no stdin it
    # reads EOF instantly and declines -- and the only trace is one line in the
    # daemon journal:
    #
    #   connection ended peer_addr=... error=pairing failed: declined by user
    #
    # The operator sees the scanner succeed on the phone and nothing else. This
    # is Packaging v1's `bash -s` with no stdin, in a new place: a command that
    # exits 0 having silently answered "no" on the operator's behalf.
    #
    # A BOUNDED number of confirmations is supplied -- not `yes` -- so the
    # harness accepts the pairing it asked for and nothing more. Which peer was
    # actually accepted is asserted by fingerprint and platform below rather
    # than assumed.
    gu "printf 'y\ny\ny\n' | nohup omnibridge pair --ttl $PAIR_TTL > /tmp/ob-pair.txt 2>&1 &" >/dev/null 2>&1
    sleep 6
    payload="$(gx 'sed -n "s/^ *\(omnibridge1:.*\)$/\1/p" /tmp/ob-pair.txt | head -1' | tr -d '[:space:]')"
    [ -n "$payload" ] \
        || abort "'omnibridge pair' produced no omnibridge1: payload in the guest; nothing can be displayed to scan"
    case "$payload" in omnibridge1:*) : ;; *) abort "pairing payload has an unexpected shape: $payload" ;; esac
    printf '%s\n' "$payload" > "$payload_file"
    ok "pairing payload obtained from the guest (${#payload} bytes, expires in ${PAIR_TTL}s)"

    qr_png="$EVIDENCE/35-pair-qr.png"
    command -v qrencode >/dev/null 2>&1 || abort "qrencode is not installed on the host; the QR cannot be shown"
    qrencode -o "$qr_png" -s 20 -m 4 "$payload" || abort "qrencode failed"
    [ -s "$qr_png" ] || abort "the rendered QR is empty"
    qr_px="$(file "$qr_png" | sed -n 's/.*PNG image data, \([0-9]*\) x .*/\1/p')"
    ok "QR rendered at ${qr_px}x${qr_px} px: $qr_png"
    # Close any image viewer first. This is not tidiness: GNOME's viewer runs
    # as a D-Bus service, and opening a second file while a window is already
    # up does not reliably raise or replace it. An operator then scans
    # whatever was already on screen -- which is how a run was lost to a stale
    # QR from an earlier attempt, with the app correctly rejecting a payload
    # nobody meant to present.
    pkill -x loupe >/dev/null 2>&1 || true
    pkill -x eog >/dev/null 2>&1 || true
    sleep 1
    ( setsid loupe "$qr_png" >/dev/null 2>&1 & ) \
        || ( setsid xdg-open "$qr_png" >/dev/null 2>&1 & ) || true
    sleep 3
    if pgrep -x loupe >/dev/null 2>&1; then
        ok "image viewer relaunched on this run's QR (any earlier QR window was closed first)"
    else
        na "no image viewer process detected; the operator must open the PNG by hand"
    fi

    # The ASCII rendering the CLI itself prints, kept as evidence and as a
    # fallback the operator can scan from a terminal if no viewer appears.
    gx 'cat /tmp/ob-pair.txt' > "$EVIDENCE/35b-pair-qr.txt" 2>/dev/null || true

    # Open the phone's scanner. `am start` CANNOT do this and must not be
    # relied on: PairingCaptureActivity is exported="false" -- correctly, and
    # SEC-ANDROID-01 depends on it staying that way -- so adb is refused with
    # `SecurityException: Permission Denial: ... not exported`. The button is
    # tapped instead, at coordinates read from the live view hierarchy.
    "${ADB[@]}" shell am force-stop "$APP_PKG" >/dev/null 2>&1
    sleep 2
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
    sleep 8
    pair_ui="$(dump_ui || true)"
    [ -n "${pair_ui//[[:space:]]/}" ] || abort "cannot read the phone's view hierarchy to find the pairing button"
    tap_xy="$(printf '%s' "$pair_ui" | tr '>' '\n' \
        | grep -i 'content-desc="Pair a new device"' \
        | sed -n 's/.*bounds="\[\([0-9]*\),\([0-9]*\)\]\[\([0-9]*\),\([0-9]*\)\]".*/\1 \2 \3 \4/p' \
        | head -1 \
        | awk '{print int(($1+$3)/2), int(($2+$4)/2)}')"
    [ -n "$tap_xy" ] \
        || abort "the phone's view hierarchy has no 'Pair a new device' control; the scanner cannot be opened"
    # shellcheck disable=SC2086
    "${ADB[@]}" shell input tap $tap_xy >/dev/null 2>&1
    sleep 5
    top="$("${ADB[@]}" shell dumpsys activity activities 2>/dev/null | sed -n 's/.*topResumedActivity=ActivityRecord{[^ ]* [^ ]* \([^ ]*\).*/\1/p' | head -1)"
    case "$top" in
        *PairingCaptureActivity*) ok "the phone's pairing scanner is open (tapped at $tap_xy)" ;;
        *) abort "tapping at $tap_xy did not open the scanner; the phone is showing '$top'" ;;
    esac

    cat <<OPBLOCK

=========================================================
OPERATOR ACTION REQUIRED — scan the pairing QR
=========================================================
  Target machine   the HOST screen (Fedora 44) and the $phone_model tablet
  What to do       the tablet's OmniBridge pairing scanner is ALREADY OPEN
                   (verified: topResumedActivity is PairingCaptureActivity).
                   A QR has been opened on this screen in the image viewer.
                   Point the tablet's camera at it. If no window appeared,
                   open this file by hand and scan it:

                     $qr_png

                   That is the only manual step in this run.
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

devs_after="$(ob devices 2>&1)"
printf '%s\n' "$devs_after" | save "36-guest-devices-after-pair.txt"

# Exactly one paired peer, and it must be the phone. More than one and no
# capability result below could be attributed; none and the pairing did not
# take however green the scan looked.
n_paired="$(printf '%s\n' "$devs_after" | grep -c 'paired *yes' || true)"
[ "${n_paired//[[:space:]]/}" = "1" ] \
    || abort "the guest reports ${n_paired//[[:space:]]/} paired devices; exactly one is required to attribute a result"
ok "the guest has exactly one paired peer"

peer_id="$(printf '%s\n' "$devs_after" | sed -n "s/^ *$phone_model *\([0-9a-f]\{16,\}\).*/\1/p" | head -1)"
[ -n "$peer_id" ] || abort "paired, but the peer's device id could not be read back"
peer_platform="$(printf '%s\n' "$devs_after" | sed -n 's/^ *platform *//p' | head -1 | tr -d '[:space:]')"
peer_fpr="$(printf '%s\n' "$devs_after" | sed -n 's/^ *fingerprint *//p' | head -1)"
[ "$peer_platform" = "android" ] \
    || abort "the paired peer reports platform '$peer_platform', not android; this is not the phone under test"
[ -n "$peer_fpr" ] || abort "the paired peer has no fingerprint"
ok "the paired peer is $phone_model ($peer_id), platform android, fingerprint $peer_fpr"

# The peer's fingerprint must NOT be the host Fedora daemon's, which is the
# one pairing this wave must never disturb. They are different devices; if the
# two ever matched, something has been cloned.
case "$peer_fpr" in
    "$guest_fpr") abort "the peer's fingerprint equals the guest's own; the trust store is not describing two machines" ;;
    *) ok "the peer's fingerprint differs from the guest's own, as it must" ;;
esac

{ echo "peer_id      $peer_id"; echo "peer_fpr     $peer_fpr"; echo "guest_id     $guest_dev_id"; echo "guest_fpr    $guest_fpr"; } | save "36b-peer-binding.txt"

# ---------------------------------------------------------------------------
section "Proving the phone is pointed at THIS guest"
# ---------------------------------------------------------------------------
# Everything the phone sends goes to whichever desktop the app has selected.
# With four paired desktops on this tablet by the end of the wave, sending to
# the wrong one and reading the right one's journal is a way to pass three
# gates while measuring nothing.
"${ADB[@]}" shell am force-stop "$APP_PKG" >/dev/null 2>&1
sleep 2
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 10
sel_ui="$(dump_ui || true)"
[ -n "${sel_ui//[[:space:]]/}" ] || abort "uiautomator returned an empty hierarchy; the selected peer cannot be verified"
printf '%s\n' "$sel_ui" | save "37-phone-ui-after-pair.xml"

if printf '%s' "$sel_ui" | grep -qi "$guest_dev_name"; then
    ok "L12: the packaged desktop '$guest_dev_name' now appears on the phone, and did not before"
else
    abort "the phone's UI does not name '$guest_dev_name'; it is not pointed at the guest under test"
fi

# "existing pairing untouched" -- the other half of L12.
if [ "${HOST_PAIRING_PRESENT:-0}" = "1" ]; then
    if printf '%s' "$sel_ui" | grep -qi 'fedora'; then
        ok "L12: the tablet's pairing with the host 'fedora' daemon survived pairing with the guest"
    else
        notok "L12: the tablet's existing 'fedora' pairing is no longer listed after pairing with the guest"
    fi
fi

# The guest's short fingerprint is shown beside the selected device. Matching
# it is what distinguishes "a desktop called anyflow-d13 is listed" from "the
# desktop this run is testing is the selected one".
guest_fpr_head="$(printf '%s' "$guest_fpr" | awk '{print $1, $2}')"
if printf '%s' "$sel_ui" | grep -qF "$guest_fpr_head"; then
    ok "the phone shows this guest's fingerprint ($guest_fpr_head), so the selected peer is the guest under test"
else
    notok "the phone does not show this guest's fingerprint ($guest_fpr_head); the selected peer may be another desktop"
fi

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
#
# `wl-copy` does NOT exit: it stays alive to own the Wayland selection for as
# long as the clip is offered. Run in the foreground it never returns, and the
# guest-exec call blocks until its timeout -- the run simply stops, with the
# last line printed looking like a pass. Detach it and let it keep the
# selection, then verify by reading the clipboard back.
gu "setsid wl-copy '$SENT_CLIP' >/dev/null 2>&1 </dev/null & sleep 1" >/dev/null 2>&1
sleep 2
gx 'pgrep -x wl-copy >/dev/null' \
    || abort "no wl-copy process owns the selection after the copy; the clipboard holds nothing to send"
ok "L14: a detached wl-copy owns the Wayland selection"
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
# `--clear` releases the selection and exits; `wl-copy ''` would hang for the
# same reason the copy above does.
gu "wl-copy --clear" >/dev/null 2>&1
sleep 1
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
