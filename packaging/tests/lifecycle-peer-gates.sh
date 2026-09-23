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
# shellcheck source=lib/assert.sh
. "$HERE/lib/assert.sh"

DOMAIN=""; DISTRO=""; EVIDENCE=""; PHONE_IP=""; ADB_SERIAL=""; PAIR_TTL=240
GUEST_USER="${GUEST_USER:-anyflow}"; GUEST_UID="${GUEST_UID:-1000}"
APP_PKG="io.github.yurisismotto.omnibridge"
# The notification fixture, and NOT `com.android.shell`. OmniBridge's app
# picker offers apps a person can open from their home screen, plus apps that
# happen to be notifying right now -- and com.android.shell has no launcher
# entry, so it is invisible in the picker until it is already notifying,
# which is the ordering problem android/fixture/README.md exists to solve.
# Choosing it therefore silently never happened, and L16 measured a peer that
# announced no source role.
FIXTURE_PKG="io.github.yurisismotto.omnibridge.fixture"
FIXTURE_ACT="$FIXTURE_PKG/.FixtureActivity"
LISTENER="$APP_PKG/$APP_PKG.notifications.OmniBridgeNotificationListener"

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
#
# Assigned unconditionally: lib/guest-agent.sh has already applied its own
# default by the time this runs, so a `${VAR:-180}` here would quietly keep
# the 900 it set and the tightening would never happen.
export GA_EXEC_TIMEOUT=180

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
# Every host tool this run cannot proceed without, named before anything is
# measured. qrencode in particular: without it there is no QR, and the operator
# stands in front of a screen with nothing on it.
need_tool virsh jq adb qrencode python3 || abort "a host tool this harness depends on is missing"
ok "every host tool this harness needs is installed"

ga_ping "$DOMAIN" 300 || abort "guest agent in '$DOMAIN' does not answer"
ok "the guest agent answers"

n_dev="$("${ADB[@]}" devices 2>/dev/null | grep -cw device || true)"
[ "${n_dev:-0}" -ge 1 ] 2>/dev/null || abort "adb sees no attached device; every phone gate would measure nothing"
phone_model="$("${ADB[@]}" shell getprop ro.product.model 2>/dev/null | tr -d '\r[:space:]')"
phone_rel="$("${ADB[@]}" shell getprop ro.build.version.release 2>/dev/null | tr -d '\r[:space:]')"
[ -n "$phone_model" ] || abort "adb returned no device model"
ok "physical Android attached: $phone_model, Android $phone_rel"

grep -qx "package:$APP_PKG" <<<"$("${ADB[@]}" shell pm list packages 2>/dev/null)" \
    || abort "$APP_PKG is not installed on the phone"
ok "$APP_PKG is installed on the phone"

# L16 needs a notification SOURCE the picker can offer before it has notified.
# Without the fixture the gate cannot be measured at all, so this is a
# precondition and not a gate: failing here is honest, and reporting L16 against
# a peer that announces no source role is not.
grep -qx "package:$FIXTURE_PKG" <<<"$("${ADB[@]}" shell pm list packages 2>/dev/null)" \
    || abort "$FIXTURE_PKG is not installed on the phone; L16 has no notification source it can choose (android/fixture/README.md)"
grep -q 'android.permission.POST_NOTIFICATIONS: granted=true' \
    <<<"$("${ADB[@]}" shell dumpsys package "$FIXTURE_PKG" 2>/dev/null)" \
    || abort "$FIXTURE_PKG does not hold POST_NOTIFICATIONS; it cannot post the notification L16 mirrors"
ok "$FIXTURE_PKG is installed and holds POST_NOTIFICATIONS"

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
if grep -qi "$phone_model" <<<"$phone_pairs_before"; then
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
    grep -qi "$guest_dev_name" <<<"$ui_before" \
        && ok "L12: the phone is already paired with '$guest_dev_name' from an earlier run of this script, and lists it" \
        || abort "the guest's trust store names the phone, but the phone does not list '$guest_dev_name'; the two disagree about who is paired"
elif grep -qi "$guest_dev_name" <<<"$ui_before"; then
    abort "the phone lists '$guest_dev_name' but the guest's trust store has no pairing for $phone_model; a stale entry would make every gate below unattributable"
else
    ok "L12: the phone does NOT list '$guest_dev_name' yet — the baseline is clean"
fi

# The host Fedora daemon's pairing is the one this wave must never disturb.
# It is recorded here and checked again at the end.
if grep -qi 'fedora' <<<"$ui_before"; then
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
        if grep -qi "$phone_model" <<<"$(ob devices 2>/dev/null)"; then break; fi
        sleep 5; waited=$(( waited + 5 ))
    done
    if grep -qi "$phone_model" <<<"$(ob devices 2>/dev/null)"; then
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

if grep -qi "$guest_dev_name" <<<"$sel_ui"; then
    ok "L12: the packaged desktop '$guest_dev_name' now appears on the phone, and did not before"
else
    abort "the phone's UI does not name '$guest_dev_name'; it is not pointed at the guest under test"
fi

# "existing pairing untouched" -- the other half of L12.
if [ "${HOST_PAIRING_PRESENT:-0}" = "1" ]; then
    if grep -qi 'fedora' <<<"$sel_ui"; then
        ok "L12: the tablet's pairing with the host 'fedora' daemon survived pairing with the guest"
    else
        notok "L12: the tablet's existing 'fedora' pairing is no longer listed after pairing with the guest"
    fi
fi

# The guest's short fingerprint is shown beside the selected device. Matching
# it is what distinguishes "a desktop called anyflow-d13 is listed" from "the
# desktop this run is testing is the selected one".
guest_fpr_head="$(printf '%s' "$guest_fpr" | awk '{print $1, $2}')"
if grep -qF "$guest_fpr_head" <<<"$sel_ui"; then
    ok "the phone shows this guest's fingerprint ($guest_fpr_head), so the selected peer is the guest under test"
else
    notok "the phone does not show this guest's fingerprint ($guest_fpr_head); the selected peer may be another desktop"
fi

# ---------------------------------------------------------------------------
section "Configuring the phone for this guest"
# ---------------------------------------------------------------------------
# Pairing establishes trust; it grants nothing. OmniBridge negotiates the
# INTERSECTION of both sides' per-capability grants, so a freshly paired peer
# starts at whatever each end already allows -- measured on the first guest,
# that was `capabilities=["battery.v1"]` and every capability gate below would
# have failed for a reason that is not the gate's subject.
#
# Every control is located by label in the live view hierarchy and bound to
# THIS guest's row, because the tablet lists several desktops and each has its
# own Connect button and its own chips.
PUI="$HERE/lib/phone-ui.py"
[ -x "$PUI" ] || abort "$PUI is missing; the phone cannot be driven"

pdump() {
    "${ADB[@]}" shell uiautomator dump /sdcard/ob-ui.xml >/dev/null 2>&1 || return 1
    "${ADB[@]}" shell cat /sdcard/ob-ui.xml 2>/dev/null > "$EVIDENCE/.ui.xml"
    [ -s "$EVIDENCE/.ui.xml" ]
}
ptap() { "${ADB[@]}" shell input tap "$1" "$2" >/dev/null 2>&1; sleep 3; }
# ptap_label <anchor-regex> <target-regex> -- fails loudly rather than tapping
# empty space, which is indistinguishable from a tap that worked.
ptap_label() {
    local xy
    pdump || { ga_die "could not dump the phone's view hierarchy"; return 1; }
    xy="$("$PUI" "$EVIDENCE/.ui.xml" find-after "$1" "$2" 2>/dev/null)" || {
        ga_die "no '$2' below '$1' on the phone"; return 1; }
    # shellcheck disable=SC2086
    ptap $xy
}

# ensure_connected WHY -- the session must exist when the operation runs.
#
# `am force-stop` is used above to reach a known screen, and it does two things
# beyond that: it drops the TLS session (the guest logs `connection ended ...
# Connection reset by peer`) and it SNOOZES the notification listener. A gate
# that then sends a file gets `protocol violation: that device is not
# connected`, and a gate that posts a notification mirrors nothing -- both
# reported as product failures, both measured against a connection this harness
# had just torn down itself.
#
# Android is always the initiator, so the desktop cannot dial: the session is
# restored by tapping Connect on the guest's own row. Called before every
# capability operation, and it aborts rather than letting a gate run unconnected.
ensure_connected() {
    local why="$1" xy
    if grep -q 'connected *yes' <<<"$(ob devices 2>/dev/null)"; then return 0; fi
    # Bring the list forward WITHOUT a force-stop -- that is the thing that
    # broke the session in the first place.
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
    sleep 6
    pdump || abort "cannot read the phone's view hierarchy to reconnect ($why)"
    if xy="$("$PUI" "$EVIDENCE/.ui.xml" find '^Devices$' 2>/dev/null)"; then
        # shellcheck disable=SC2086
        ptap $xy
        pdump || true
    fi
    ptap_label "$guest_dev_name" '^Connect$' || abort "could not tap Connect for $guest_dev_name ($why)"
    ga_wait_for "$DOMAIN" 90 "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID omnibridge devices 2>/dev/null | grep -q 'connected *yes'" \
        || abort "the phone did not reconnect to $guest_dev_name ($why); the gate below would measure a torn-down session"
    ok "reconnected to $guest_dev_name before $why"
}

# rebind_listener -- Android unbinds ("snoozes") a NotificationListenerService
# whose app has been force-stopped, and a snoozed listener receives nothing.
# The app's own process cannot un-snooze itself from adb, but toggling the
# approval does, because that is what re-evaluates the binding.
#
# The approval string is read first and asserted to come back BYTE-IDENTICAL:
# this setting is the operator's, it lists two other listeners that have
# nothing to do with OmniBridge, and a run that widened or narrowed it would be
# changing the device it is certifying.
rebind_listener() {
    local before after
    before="$("${ADB[@]}" shell settings get secure enabled_notification_listeners 2>/dev/null | tr -d '\r')"
    case "$before" in
        *"$APP_PKG"*) : ;;
        *) abort "OmniBridge's notification listener is not approved on the phone; L16 cannot be measured" ;;
    esac
    printf '%s\n' "$before" | save "39-listener-approval-before.txt"
    "${ADB[@]}" shell cmd notification disallow_listener "$LISTENER" >/dev/null 2>&1
    sleep 3
    "${ADB[@]}" shell cmd notification allow_listener "$LISTENER" >/dev/null 2>&1
    sleep 6
    after="$("${ADB[@]}" shell settings get secure enabled_notification_listeners 2>/dev/null | tr -d '\r')"
    [ "$after" = "$before" ] \
        || abort "the phone's approved-listener list changed across the rebind; it must be restored exactly (before: $before / after: $after)"
    grep -q "ComponentInfo{$APP_PKG/$APP_PKG.notifications.OmniBridgeNotificationListener}" \
        <<<"$("${ADB[@]}" shell dumpsys notification 2>/dev/null)" \
        || abort "OmniBridge's notification listener is not among the live listeners after the rebind"
    ok "the notification listener is bound again, and the approval list is byte-identical"
}

# 1. The three capability chips, on the guest's own row. The permission screen
#    is opened unconditionally -- the notification opt-in lives on it too, and
#    only entering it when the clipboard happens to be off is how the second
#    half of this configuration silently never runs.
"${ADB[@]}" shell am force-stop "$APP_PKG" >/dev/null 2>&1
sleep 2
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 8
pdump || abort "cannot read the phone's view hierarchy"
if ptap_label "$guest_dev_name" 'Clipboard, (allowed|not allowed)'; then
    pdump || abort "cannot read the permission screen"
    "$PUI" "$EVIDENCE/.ui.xml" find 'Permissions' >/dev/null 2>&1 \
        || abort "tapping the chip did not open the permission screen for $guest_dev_name"
    ok "opened the permission screen for $guest_dev_name"
    for cap in Clipboard Files Battery; do
        pdump || abort "cannot read the permission screen"
        st="$("$PUI" "$EVIDENCE/.ui.xml" state-after 'Permissions' "^$cap"'$' 2>/dev/null || echo unknown)"
        if [ "$st" = "unchecked" ]; then
            xy="$("$PUI" "$EVIDENCE/.ui.xml" toggle-after 'Permissions' "^$cap"'$' | awk '{print $1, $2}')"
            # shellcheck disable=SC2086
            ptap $xy
            ok "granted $cap to $guest_dev_name on the phone"
        else
            ok "$cap already granted to $guest_dev_name on the phone ($st)"
        fi
    done
else
    abort "could not open the permission screen for $guest_dev_name from the device list"
fi

# 2. Notification sharing: a separate, deliberately per-app opt-in, on its own
#    screen reached from the Notifications row of the permission screen.
#
#    That row is identified by its STATUS text, and the status has THREE
#    spellings, one per state the peer can be in:
#
#      Off             sharing has never been enabled for this desktop
#      No apps chosen  sharing is on, but nothing is selected yet
#      N of M apps     sharing is on and N apps are selected
#
#    The row's own label is the bare word "Notifications", which also appears
#    in the body copy around it, so it cannot be used. Matching only one
#    spelling is how this step twice looked at the wrong thing: `n/a` on a
#    re-run of an already-configured peer, and a hard stop on a freshly paired
#    one. All three are matched, and no match aborts rather than shrugging.
pdump || abort "cannot read the phone's view hierarchy"
if ! "$PUI" "$EVIDENCE/.ui.xml" find 'Share notifications with this computer' >/dev/null 2>&1; then
    nrow="$("$PUI" "$EVIDENCE/.ui.xml" find '^(Off|On|No apps chosen|[0-9]+ of [0-9]+ apps)$' 2>/dev/null)" || nrow=""
    [ -n "$nrow" ] \
        || abort "the permission screen for $guest_dev_name shows no notification-sharing row ('Off', 'No apps chosen' or 'N of M apps'); L16 cannot be configured"
    # shellcheck disable=SC2086
    ptap $nrow
    pdump || abort "the notification-sharing screen did not open"
fi
if "$PUI" "$EVIDENCE/.ui.xml" find 'Share notifications with this computer' >/dev/null 2>&1; then
    st="$("$PUI" "$EVIDENCE/.ui.xml" state-after 'Notifications' 'Share notifications with this computer' 2>/dev/null || echo unknown)"
    if [ "$st" = "unchecked" ]; then
        xy="$("$PUI" "$EVIDENCE/.ui.xml" toggle-after 'Notifications' 'Share notifications with this computer' | awk '{print $1, $2}')"
        # shellcheck disable=SC2086
        ptap $xy
        ok "enabled notification sharing with $guest_dev_name"
    else
        ok "notification sharing with $guest_dev_name is already on ($st)"
    fi
    # Choose exactly ONE app -- the fixture, which is what posts the test
    # notification. Selecting every app would share the operator's whole
    # notification stream with a throwaway VM, and choosing com.android.shell
    # cannot work: it has no launcher entry, so the picker cannot offer it
    # until it is already notifying.
    #
    # The app is searched for by its PACKAGE name, which is unique and which
    # the picker shows under each label. `shell` matched nothing here and the
    # miss was reported as one failed check while L16 went on to measure a peer
    # that announced no source role -- three more failures, none of them real.
    #
    # The picker is entered UNCONDITIONALLY. A peer that already has an app
    # chosen is not thereby configured for this gate: Debian 13 arrived at
    # this point reading "1 of 99 apps" from an earlier wave, the chosen app
    # was not the fixture, and the fixture's notification was therefore shared
    # with nobody. The count says something is chosen; only the fixture's own
    # tick says the thing this gate posts from is chosen.
    pdump || true
    apps_row="$("$PUI" "$EVIDENCE/.ui.xml" find '^(No app chosen|[0-9]+ of [0-9]+ apps)$' 2>/dev/null)" || apps_row=""
    if [ -n "$apps_row" ]; then
        # shellcheck disable=SC2086
        ptap $apps_row
        pdump || abort "the app picker did not open"
        "$PUI" "$EVIDENCE/.ui.xml" find 'Search apps' >/dev/null 2>&1 \
            || abort "the app picker has no search field; the notification source cannot be chosen"
        xy="$("$PUI" "$EVIDENCE/.ui.xml" find 'Search apps')"
        # shellcheck disable=SC2086
        ptap $xy
        "${ADB[@]}" shell input text "$FIXTURE_PKG" >/dev/null 2>&1
        sleep 4
        pdump || abort "cannot read the app picker after searching for $FIXTURE_PKG"
        cp "$EVIDENCE/.ui.xml" "$EVIDENCE/39b-L16-app-picker.xml" 2>/dev/null || true
        # Anchored on 'Clear all', not on 'Choose apps': the text typed into
        # the search field IS the package name, so it is itself the first node
        # matching $FIXTURE_PKG below the screen title -- and a search box has
        # no checkbox on its row, so the lookup found nothing and the run
        # stopped saying the picker does not offer the fixture, with the
        # fixture plainly listed below. 'Clear all' sits between the search
        # field and the first result in every state of this screen.
        xy="$("$PUI" "$EVIDENCE/.ui.xml" toggle-after '^Clear all$' "$FIXTURE_PKG" 2>/dev/null | awk '{print $1, $2}')" || xy=""
        [ -n "$xy" ] \
            || abort "$FIXTURE_PKG is not offered by the phone's app picker; L16 has no source it can choose"
        st="$("$PUI" "$EVIDENCE/.ui.xml" toggle-after '^Clear all$' "$FIXTURE_PKG" | awk '{print $3}')"
        if [ "$st" = "checked" ]; then
            ok "$FIXTURE_PKG is already the chosen notification source"
        else
            # shellcheck disable=SC2086
            ptap $xy
            pdump || abort "cannot read the app picker after ticking $FIXTURE_PKG"
            st="$("$PUI" "$EVIDENCE/.ui.xml" toggle-after '^Clear all$' "$FIXTURE_PKG" | awk '{print $3}')"
            [ "$st" = "checked" ] \
                || abort "ticking $FIXTURE_PKG did not take (still $st); the choice must be verified, not assumed"
            ok "chose $FIXTURE_PKG as the only shared notification source, and the tick is confirmed"
        fi
        # The count the picker itself reports. "No app chosen" must be gone:
        # reading the absence of that string is what distinguishes a choice
        # that took from a tap that landed on nothing.
        chosen="$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null | grep -E '^[0-9]+ of [0-9]+ apps$' | head -1)"
        [ -n "$chosen" ] \
            || abort "the picker does not report a chosen-app count; the choice cannot be confirmed"
        ok "the phone reports '$chosen' shared with $guest_dev_name, the fixture among them"
        # Back out of the picker so the caller returns to a known screen.
        "${ADB[@]}" shell input keyevent KEYCODE_BACK >/dev/null 2>&1
        sleep 3
    else
        abort "the notification screen for $guest_dev_name offers no app-choice row; the fixture cannot be selected and L16 would measure a peer sharing nothing"
    fi
else
    abort "the phone's notification-sharing control was not reachable; L16 would go on to measure a peer that announces no source role"
fi
"${ADB[@]}" shell am force-stop "$APP_PKG" >/dev/null 2>&1
sleep 2
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 8

# 2b. That was the LAST force-stop of the run, and every one of them snoozed
#     the notification listener. Rebind it here, before the session is opened,
#     so L16 is not measuring a listener Android has unbound.
rebind_listener

# 3. The guest's own grants, and its notification mirror opt-in.
for cap in clipboard.v1 files.v1 notifications.v1; do
    ob "grant $peer_id $cap" >/dev/null 2>&1
done
granted="$(ob devices 2>&1 | sed -n 's/^ *granted *//p' | head -1)"
for cap in battery.v1 clipboard.v1 files.v1 notifications.v1; do
    case "$granted" in
        *"$cap"*) ok "the guest grants $cap to the phone" ;;
        *) notok "the guest does NOT grant $cap (granted: $granted)" ;;
    esac
done
ob "notifications mirror $peer_id on" >/dev/null 2>&1

# 4. Connect. The desktop cannot dial: Android is always the initiator, so the
#    session only exists once the phone is told to connect.
#
state="$(ob devices 2>&1 | sed -n 's/^ *state *//p' | head -1 | tr -d '[:space:]')"
if [ "$state" != "connected" ]; then
    ptap_label "$guest_dev_name" '^Connect$' || abort "could not tap Connect for $guest_dev_name"
    ga_wait_for "$DOMAIN" 90 "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID omnibridge devices 2>/dev/null | grep -q 'connected *yes'" \
        || abort "the phone did not connect to $guest_dev_name; no capability gate below could run"
fi
ok "the phone is connected to $guest_dev_name"

# Which 'session established' line describes the session the gates will use.
#
# An unbounded `grep 'session established' | tail -1` picks a line from a
# session that has since ENDED -- including the `capabilities=["battery.v1"]`
# one the phone opens before the grants are made -- and characterises this
# connection from it. A `--since` window is no better: it goes empty whenever
# the session was already up when this phase started, and an empty capture is
# not evidence either.
#
# So the line is bound to the LIVE session instead: of every session event in
# the journal, the most recent one must be this device's establishment. If a
# `connection ended` came after it, the line describes a session that is gone.
sess_events="$(gu "journalctl --user -u omnibridged --no-pager -n 500 2>/dev/null | grep -E 'session established|connection ended'")"
[ -n "${sess_events//[[:space:]]/}" ] \
    || abort "no session events at all in the guest journal; the connection cannot be characterised"
printf '%s\n' "$sess_events" | save "38b-session-events.txt"
negotiated="$(printf '%s\n' "$sess_events" | tail -1)"
case "$negotiated" in
    *"session established"*"device=$peer_id"*) ok "the newest session event is this peer's establishment, so it describes the live session" ;;
    *"session established"*) abort "the newest session event names a different device than $peer_id: $negotiated" ;;
    *) abort "the newest session event is a disconnection, so no live session backs the gates below: $negotiated" ;;
esac
printf '%s\n' "$negotiated" | save "38-session-established.txt"
for cap in clipboard.v1 files.v1 notifications.v1; do
    case "$negotiated" in
        *"$cap"*) ok "the session negotiated $cap" ;;
        *) notok "the session did NOT negotiate $cap -- its gate would fail for the wrong reason" ;;
    esac
done

# Sentinels. High-entropy so that a match cannot be a pre-existing string and
# an absence cannot be luck. Each gate gets its own.
SENT_CLIP="OBCLIP-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_FILE_NAME="OBNAME-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_FILE_BODY="OBBODY-$(head -c 24 /dev/urandom | base32 | tr -d '=' | head -c 40)"
SENT_NOTIF="OBNOTIF-$(head -c 12 /dev/urandom | base32 | tr -d '=' | head -c 20)"
SENT_NOTIF_BODY="OBNBODY-$(head -c 15 /dev/urandom | base32 | tr -d '=' | head -c 24)"
{ echo "clip       $SENT_CLIP"; echo "file-name  $SENT_FILE_NAME"; echo "file-body  $SENT_FILE_BODY"; echo "notif      $SENT_NOTIF"; echo "notif-body $SENT_NOTIF_BODY"; } | save "37-sentinels.txt"

# ---------------------------------------------------------------------------
section "L14 — clipboard, both directions"
# ---------------------------------------------------------------------------
clip_status="$(ob clipboard status 2>&1)"
[ -n "${clip_status//[[:space:]]/}" ] || abort "'omnibridge clipboard status' produced no output"
printf '%s\n' "$clip_status" | save "40-L14-clipboard-status.txt"
ok "L14: 'omnibridge clipboard status' answers ($(printf '%s\n' "$clip_status" | grep -c .) line(s))"

# The gate asks specifically that status is HONEST about --sensitive. On
# Ubuntu/Debian wl-clipboard is 2.2.1 and has no --sensitive; saying so is the
# pass, and silently accepting a clip Android marked sensitive would be the
# failure.
wlc_ver="$(gx 'wl-copy --version 2>&1 | head -1' || true)"
has_sensitive="$(gx 'wl-copy --help 2>&1 | grep -c -- "--sensitive" || true' | tr -d '[:space:]')"
if [ "${has_sensitive:-0}" = "0" ]; then
    if grep -qiE 'sensitive clipboard *unavailable' <<<"$clip_status"; then
        ok "L14: wl-copy has no --sensitive (${wlc_ver:-unknown}) and status says 'unavailable'"
    else
        notok "L14: wl-copy has no --sensitive and status does not say so (U-1 regression)"
    fi
else
    ok "L14: wl-copy supports --sensitive (${wlc_ver:-unknown})"
fi

# Can this session read its own selection at all? That is a property of the
# compositor, not of OmniBridge: GNOME implements neither wlr-data-control nor
# ext-data-control, so no client can read a selection it does not own. The
# daemon reports this itself, and the gate is classified from the product's own
# contract rather than forced either way.
if grep -qiE 'auto-send +NOT supported here|watch: unavailable' <<<"$clip_status"; then
    ok "L14: the daemon reports this session cannot observe clipboard changes (its own words, recorded)"
fi

ensure_connected "L14"

gu "setsid wl-copy '$SENT_CLIP' >/dev/null 2>&1 </dev/null & sleep 1" >/dev/null 2>&1
sleep 2
gx 'pgrep -x wl-copy >/dev/null' \
    || abort "no wl-copy process owns the selection; the send direction would measure nothing"
ok "L14: a detached wl-copy owns the Wayland selection"

# The window that must bracket the send, and a phone with NO clipboard receipt
# on it. Both are established BEFORE the operation.
#
# The receipt is not a new notification each time: the app updates one
# notification in place, so its key is identical before and after and a
# "shade gained a notification" test fails on a send that worked. The card is
# dismissed instead, and asserted gone -- then its reappearance is this send's
# receipt and not the previous run's.
clip_mark="$(gx 'date -u "+%Y-%m-%d %H:%M:%S"' | tr -d '\n')"
[ -n "$clip_mark" ] || abort "could not read the guest clock; the L14 capture window could not be bounded"

"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 6
pdump || abort "cannot read the phone's view hierarchy before the clipboard send"
if grep -qiF "Clipboard from" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)"; then
    if xy="$("$PUI" "$EVIDENCE/.ui.xml" find-after 'Clipboard from' '^Dismiss$' 2>/dev/null)"; then
        # shellcheck disable=SC2086
        ptap $xy
        pdump || true
    fi
fi
cp "$EVIDENCE/.ui.xml" "$EVIDENCE/41c-L14-phone-before.xml" 2>/dev/null || true
grep -qiF "Clipboard from $guest_dev_name" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
    && abort "a clipboard receipt from $guest_dev_name is still on the phone before this send; its reappearance afterwards would prove nothing" \
    || ok "L14: the phone carries no clipboard receipt from $guest_dev_name before the send"
sleep 1

send_out="$(gx "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GUEST_UID/bus WAYLAND_DISPLAY=wayland-0 DISPLAY=:0 sh -c 'omnibridge clipboard send $peer_id > /tmp/cs.out 2>&1; echo rc=\$?; cat /tmp/cs.out'")"
printf '%s\n' "$send_out" | save "41-L14-send-out.txt"
send_rc="$(printf '%s' "$send_out" | sed -n 's/^rc=//p' | head -1)"

if [ "$send_rc" = "0" ]; then
    ok "L14: guest -> phone clipboard send exited 0"

    # The CLI's own report must name THIS peer and THIS clip's length. "sent"
    # with no byte count and no fingerprint would be true of any send.
    clip_len="${#SENT_CLIP}"
    grep -qF "$peer_fpr" <<<"$send_out" \
        && ok "L14: the send names the peer under test ($peer_fpr)" \
        || notok "L14: the send does not name the peer's fingerprint; it cannot be attributed"
    grep -qE "sent +$clip_len +bytes" <<<"$send_out" \
        && ok "L14: the send reports exactly $clip_len bytes, the length of this run's sentinel" \
        || notok "L14: the send does not report $clip_len bytes (sentinel length); the payload is not bound to this run"

    # The guest journal, inside the window opened above.
    jc="$(gu "journalctl --user -u omnibridged --no-pager --since '$clip_mark' 2>/dev/null | grep -F 'clipboard update sent'")"
    [ -n "${jc//[[:space:]]/}" ] \
        || abort "no 'clipboard update sent' line in the journal window opened at $clip_mark; the send cannot be corroborated"
    printf '%s\n' "$jc" | save "41d-L14-journal.txt"
    grep -qF "bytes=$clip_len" <<<"$jc" \
        && ok "L14: the guest journal records this send (bytes=$clip_len) inside the bracketing window" \
        || notok "L14: the journal line does not carry bytes=$clip_len"

    # The phone's own half. adb cannot read the Android clipboard back -- this
    # build has no `cmd clipboard` implementation -- so receipt is asserted
    # where the product shows it: the card the app raises, which names the
    # sending desktop and the byte count, on a phone that demonstrably had no
    # such card a moment ago.
    sleep 8
    "${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
    sleep 6
    pdump || abort "cannot read the phone's view hierarchy to check the clipboard receipt"
    cp "$EVIDENCE/.ui.xml" "$EVIDENCE/41f-L14-phone-receipt.xml" 2>/dev/null || true
    if grep -qiF "Clipboard from $guest_dev_name" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)"; then
        ok "L14: the phone shows 'Clipboard from $guest_dev_name' -- receipt attributed to the guest under test"
        grep -qE "^$clip_len bytes$" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
            && ok "L14: the phone reports $clip_len bytes, matching this run's sentinel exactly" \
            || notok "L14: the phone does not report $clip_len bytes for the received clip"
    else
        notok "L14: the phone does not show a clipboard receipt from $guest_dev_name"
    fi
elif grep -qiE 'clipboard did not respond in time' <<<"$send_out"; then
    # The product refuses clearly rather than sending something wrong. That is
    # the documented behaviour of this backend on a compositor with no
    # data-control protocol, so the transfer half is N/A on this session type.
    na "L14: guest -> phone is N/A on this compositor -- $(printf '%s' "$send_out" | sed -n 's/^error: //p' | head -1)"
    # But the status line and the failure disagree, and that is worth naming.
    if grep -qi 'manual send still works' <<<"$clip_status"; then
        notok "L14/FINDING: 'clipboard status' claims 'manual send still works', and manual send failed here with the clipboard timeout. The claim does not hold on this session."
    fi
else
    notok "L14: guest -> phone clipboard send failed unexpectedly: $(printf '%s' "$send_out" | tr '\n' ' ' | head -c 160)"
fi

# phone -> guest. The daemon WRITES the received clip, which needs no
# data-control protocol, so this direction can work where the other cannot.
#
# The selection is released by killing the holder, not by `wl-copy --clear`:
# --clear blocks on this compositor exactly as a copy does, and waiting on it
# stalls the run.
gx 'pkill -x wl-copy; true' >/dev/null 2>&1
sleep 1
ensure_connected "L14 phone -> guest"
# What the daemon had cached BEFORE, so an arrival is a delta and not a
# pre-existing number read after the fact.
caches_before="$(ob clipboard status 2>&1 | sed -n 's/^ *caches *\([0-9]*\) event id(s).*/\1/p' | head -1)"
caches_before="${caches_before:-0}"
recv_mark="$(gx 'date -u "+%Y-%m-%d %H:%M:%S"' | tr -d '\n')"
[ -n "$recv_mark" ] || abort "could not read the guest clock; the phone -> guest window could not be bounded"
sleep 1
if ptap_label "$guest_dev_name" '^Send clipboard$' 2>/dev/null || ptap_label 'Quick actions' '^Send clipboard$' 2>/dev/null; then
    sleep 10
    pdump || abort "cannot read the phone's 'Send clipboard' sheet"
    cp "$EVIDENCE/.ui.xml" "$EVIDENCE/41b-L14-phone-send-ui.xml" 2>/dev/null || true
    sheet="$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null || true)"

    if grep -qiF 'There is nothing to send' <<<"$sheet"; then
        # The product's own answer, and it is the correct one: this harness
        # cannot put anything on the Android clipboard. There is no `cmd
        # clipboard` in this build, and the only other way in is a human
        # copying text in an app. So the direction is not exercised, and the
        # reason is the harness's reach and not the product's behaviour.
        na "L14: phone -> guest NOT EXERCISED. The Android clipboard is empty and the app says so -- 'There is nothing to send. Copy some text, then come back.' adb cannot set the Android clipboard (no 'cmd clipboard' implementation on this build), so no sentinel can be placed for the phone to send. The product is answering correctly; the gate has nothing to measure."
    elif xy="$("$PUI" "$EVIDENCE/.ui.xml" find "^Send to $guest_dev_name\$" 2>/dev/null)"; then
        # shellcheck disable=SC2086
        ptap $xy
        sleep 10
        caches_after="$(ob clipboard status 2>&1 | sed -n 's/^ *caches *\([0-9]*\) event id(s).*/\1/p' | head -1)"
        caches_after="${caches_after:-0}"
        jr="$(gu "journalctl --user -u omnibridged --no-pager --since '$recv_mark' 2>/dev/null | grep -iE 'clipboard'")"
        printf '%s\n' "$jr" | save "41g-L14-recv-journal.txt"
        if [ "$caches_after" -gt "$caches_before" ] 2>/dev/null; then
            ok "L14: phone -> guest arrived; the daemon's clipboard cache grew $caches_before -> $caches_after inside the bracketing window"
        elif [ -n "${jr//[[:space:]]/}" ]; then
            ok "L14: phone -> guest recorded in the guest journal inside the window: $(printf '%s' "$jr" | tr '\n' ' ' | head -c 120)"
        else
            notok "L14: the phone was told to send and the guest observed nothing -- cache $caches_before -> $caches_after and no clipboard line since $recv_mark"
        fi
    else
        notok "L14: the 'Send clipboard' sheet offers neither a send control for $guest_dev_name nor an empty-clipboard message; its state is unknown"
    fi

    # Close the sheet. Leaving it up is not cosmetic: it covers the Files tab,
    # and L15's incoming-file prompt then "never appears" -- four checks
    # failing against a prompt that was drawn behind a sheet this step opened.
    if xy="$("$PUI" "$EVIDENCE/.ui.xml" find '^Cancel$' 2>/dev/null)"; then
        # shellcheck disable=SC2086
        ptap $xy
    else
        "${ADB[@]}" shell input keyevent KEYCODE_BACK >/dev/null 2>&1
        sleep 2
    fi
    pdump || abort "cannot read the phone's view hierarchy after closing the clipboard sheet"
    grep -qiF 'Send to ' <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
        && abort "the 'Send clipboard' sheet is still open; L15's prompt would be drawn behind it" \
        || ok "L14: the clipboard sheet is closed, so L15 can see the phone's own screens"
else
    na "L14: the phone's 'Send clipboard' action was not reachable from the current screen"
fi

# ---------------------------------------------------------------------------
section "L15 — files, each way"
# ---------------------------------------------------------------------------
# The file is staged in the user's HOME, not /tmp. The unit sets
# PrivateTmp=true, so a file written to /tmp outside the service's namespace
# does not exist as far as the daemon is concerned, and the transfer fails with
# "cannot read that file: No such file or directory" -- an error about the
# sandbox that reads like an error about the test.
files_mark="$(gx 'date -u "+%Y-%m-%d %H:%M:%S"' | tr -d '\n')"
[ -n "$files_mark" ] || abort "could not read the guest clock; the L15 capture window could not be bounded"
SENDDIR="/home/$GUEST_USER/obsend"
gx "mkdir -p $SENDDIR && printf '%s\n' '$SENT_FILE_BODY' > $SENDDIR/$SENT_FILE_NAME.txt && chown -R $GUEST_USER:$GUEST_USER $SENDDIR"
gx "test -s $SENDDIR/$SENT_FILE_NAME.txt" \
    || abort "the file to send does not exist in the guest; L15 would measure nothing"
ok "L15: source file staged in the guest's home with a unique name and a unique body"

# NOT a force-stop. Stopping the app here is what tore the session down on the
# first Ubuntu run: `omnibridge send` then answered "protocol violation: that
# device is not connected", and L15's four checks failed against a connection
# the harness had just killed. The app is brought forward instead, and the
# session is asserted immediately before the offer.
"${ADB[@]}" shell am start -n "$APP_PKG/.ui.MainActivity" >/dev/null 2>&1
sleep 6
ensure_connected "L15"

gx "runuser -u $GUEST_USER -- env XDG_RUNTIME_DIR=/run/user/$GUEST_UID DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$GUEST_UID/bus sh -c 'nohup omnibridge send $peer_id $SENDDIR/$SENT_FILE_NAME.txt > /tmp/snd.out 2>&1 &'" >/dev/null 2>&1
sleep 6
# The offer must have been ACCEPTED BY THE DAEMON before the phone is asked to
# show a prompt. A send that never left the guest makes "no incoming-file
# prompt appeared" a statement about the harness, not about the product.
snd_err="$(gx 'cat /tmp/snd.out 2>/dev/null' | grep -i '^error:' | head -1 || true)"
[ -z "${snd_err//[[:space:]]/}" ] \
    || abort "'omnibridge send' refused the offer before the phone was ever involved: $snd_err"
ok "L15: the guest accepted the send with no error; the offer is on the wire"

# The prompt is on the phone's FILES tab, and whatever screen the previous
# gate left up is not it. Go there explicitly: "no incoming-file prompt
# appeared" must mean the product did not draw one, not that the harness was
# looking at another screen.
#
# `nav`, not `find`: "Files" is also a per-device permission row and a
# capability chip on every device card, and `find` answers with the first of
# those. (`find | tail -1` does not help -- `find` prints one line.) The run
# that taught this searched the permission screen for an offer that was on the
# Files tab all along, and reported four L15 failures for it.
pdump || abort "cannot read the phone's view hierarchy to reach the Files tab"
files_tab="$("$PUI" "$EVIDENCE/.ui.xml" nav '^Files$' 2>/dev/null)" || files_tab=""
[ -n "$files_tab" ] || abort "the phone shows no bottom navigation carrying a 'Files' tab; the incoming-file prompt cannot be reached"
# shellcheck disable=SC2086
ptap $files_tab
sleep 4
pdump || abort "cannot read the phone's view hierarchy after opening the Files tab"
grep -qE '^(Active|Received|No files yet)$' <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
    || abort "the Files tab did not open (no Active/Received section); the offer would be looked for on another screen"
ok "L15: the phone's Files tab is open, which is where an incoming offer is drawn"

# The phone asks the human to accept an incoming file. Without the tap the
# transfer is CANCELLED (DECLINED_BY_USER) -- which is the product working,
# and a harness that did not tap would record a product failure.
pdump || abort "cannot read the phone's view hierarchy while the offer is live"
cp "$EVIDENCE/.ui.xml" "$EVIDENCE/42-L15-offer-ui.xml" 2>/dev/null || true
if grep -qF "$SENT_FILE_NAME" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)"; then
    ok "L15: the phone shows an incoming-file prompt naming THIS run's file"
    # The prompt names the sender. Asserting it is what binds this transfer to
    # the guest under test rather than to any other desktop the tablet knows.
    if grep -qiE "from $guest_dev_name" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts)"; then
        ok "L15: the prompt attributes the offer to $guest_dev_name, the machine under test"
    else
        notok "L15: the incoming-file prompt does not attribute the offer to $guest_dev_name"
    fi
    # Bound to THIS file's row. A stale offer left over from an earlier run
    # sits in the same list with its own Accept, and accepting that one would
    # complete a transfer this run never made.
    xy="$("$PUI" "$EVIDENCE/.ui.xml" find-after "$SENT_FILE_NAME" '^Accept$' 2>/dev/null)" || xy=""
    if [ -n "$xy" ]; then
        # shellcheck disable=SC2086
        ptap $xy
        ok "L15: accepted the transfer on the phone, on this file's own row"
    else
        notok "L15: no Accept control below $SENT_FILE_NAME on the incoming-file prompt"
    fi
else
    notok "L15: no incoming-file prompt naming $SENT_FILE_NAME appeared on the phone"
fi
sleep 12

xfers="$(ob transfers 2>&1)"
printf '%s\n' "$xfers" | save "43-L15-transfers.txt"
[ -n "${xfers//[[:space:]]/}" ] || abort "'omnibridge transfers' produced no output"
xfer_ctx="$(grep -A4 "$SENT_FILE_NAME" <<<"$xfers")"
if grep -qE 'state +completed' <<<"$xfer_ctx"; then
    ok "L15: the guest reports the transfer completed"
else
    notok "L15: the guest does not report $SENT_FILE_NAME as completed"
fi
# Bounded to THIS transfer. An unbounded tail of the files-capability lines
# matches a previous run's successful transfer and reports it as this one's --
# the same windowing flaw the S3 journal gate had, in a new place.
xfer_id="$(printf '%s' "$xfers" | grep -B4 "$SENT_FILE_NAME" | sed -n 's/^ *\([0-9a-f]\{8\}\) *sending.*/\1/p' | tail -1)"
if [ -n "$xfer_id" ]; then
    jf="$(gu "journalctl --user -u omnibridged --no-pager --since '$files_mark' 2>/dev/null | grep -F 'transfer=$xfer_id'")"
    need_window_covers "L15: the journal window" "$jf" "transfer=$xfer_id" \
        || abort "no journal line names transfer=$xfer_id; this transfer cannot be corroborated"
    printf '%s\n' "$jf" | save "43b-L15-journal.txt"
    ok "L15: $(printf '%s\n' "$jf" | grep -c .) journal line(s) name transfer=$xfer_id specifically"
    grep -qi 'the peer confirmed it stored the file' <<<"$jf" \
        && ok "L15: the guest journal records the peer confirming it stored THIS transfer" \
        || notok "L15: no 'peer confirmed it stored the file' line for transfer=$xfer_id"
else
    notok "L15: $SENT_FILE_NAME has no transfer id in 'omnibridge transfers'; nothing to corroborate"
fi

# The phone's own view, which is the half a user would check. The tab is found
# by label: a fixed coordinate lands somewhere on any other screen size, and a
# tap that misses looks exactly like a tap that worked.
pdump || abort "cannot read the phone's view hierarchy to reach the Files tab"
if files_xy="$("$PUI" "$EVIDENCE/.ui.xml" nav '^Files$' 2>/dev/null)"; then
    # shellcheck disable=SC2086
    ptap $files_xy
    sleep 4
fi
pdump || true
cp "$EVIDENCE/.ui.xml" "$EVIDENCE/44-L15-phone-files.xml" 2>/dev/null || true
if grep -qF "$SENT_FILE_NAME" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)"; then
    ok "L15: the phone's Files tab lists $SENT_FILE_NAME"
    grep -qiE "From $guest_dev_name" <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
        && ok "L15: the phone attributes it to '$guest_dev_name'" \
        || notok "L15: the phone does not attribute the file to $guest_dev_name"
    grep -qiE '^Received$' <<<"$("$PUI" "$EVIDENCE/.ui.xml" texts 2>/dev/null)" \
        && ok "L15: the phone marks it Received" \
        || notok "L15: the phone does not mark $SENT_FILE_NAME as Received"
else
    notok "L15: the phone's Files tab does not list $SENT_FILE_NAME"
fi

# phone -> guest. Driving this from adb needs a URI the app can read, and a
# file staged by the shell uid in /sdcard is not one: the app answers "that
# file could not be read" even with --grant-read-uri-permission, because the
# file belongs to another uid. Reaching it properly means the app's own
# document picker, which is a human choosing a file. Recorded as NOT EXECUTED
# with the reason rather than failed.
dl="/home/$GUEST_USER/Downloads/OmniBridge"
if gx "test -d $dl"; then
    n_recv="$(gx "find $dl -type f 2>/dev/null | wc -l" | tr -d '[:space:]')"
    modes="$(gx "find $dl -type f -printf '%m %u\n' 2>/dev/null | sort -u")"
    ok "L15: the guest's receive directory exists with ${n_recv} file(s)"
    if [ -n "${modes//[[:space:]]/}" ]; then
        printf '%s\n' "$modes" | save "45-L15-received-modes.txt"
        if grep -qv "^600 $GUEST_USER" <<<"$modes"; then
            notok "L15: a received file is not 0600 $GUEST_USER: $(printf '%s' "$modes" | tr '\n' ' ')"
        else
            ok "L15: every received file is 0600 and owned by $GUEST_USER"
        fi
    fi
else
    na "L15: phone -> guest NOT EXECUTED. adb cannot hand the app a readable URI (a file staged by the shell uid is refused with 'that file could not be read' even with --grant-read-uri-permission); the app's own document picker needs a human. The 0600 assertion has nothing to measure."
fi

# ---------------------------------------------------------------------------
section "L16 — notification mirroring, and no content in the journal"
# ---------------------------------------------------------------------------
ensure_connected "L16"
nstatus_before="$(ob 'notifications status' 2>&1)"
[ -n "${nstatus_before//[[:space:]]/}" ] || abort "'omnibridge notifications status' produced no output"
printf '%s\n' "$nstatus_before" | save "46-L16-status-before.txt"

# Role convergence first. Mirroring with the device claiming no source role
# means nothing will ever arrive, and posting into that is a gate that cannot
# fail for the right reason. It is a PRECONDITION and not a check: the phone
# announces the source role only once an app has been chosen and the listener
# is bound, and both of those are this harness's own doing.
grep -qi 'can source notifications' <<<"$nstatus_before" \
    || abort "the phone announces no notification source role; the app choice or the listener binding above did not take, and L16 would measure that rather than mirroring"
ok "L16: the phone announces a notification source role"

# A clean baseline, taken by clearing the FIXTURE's own notifications and
# nothing else. `mirrored now` is a count of what is currently mirrored, not a
# running total: an older mirrored notification expiring while a new one
# arrives leaves it unchanged, so `after > before` can be false on a
# notification that was mirrored perfectly. Clearing first makes the
# expected delta exactly one, which a coincidence cannot produce.
"${ADB[@]}" shell "am start -n $FIXTURE_ACT --es op clear" >/dev/null 2>&1
sleep 10
nstatus_base="$(ob 'notifications status' 2>&1)"
printf '%s\n' "$nstatus_base" | save "46b-L16-status-baseline.txt"
before_n="$(printf '%s' "$nstatus_base" | sed -n 's/^ *mirrored now *//p' | head -1 | tr -d '[:space:]')"
before_n="${before_n:-0}"
ok "L16: baseline taken after clearing the fixture's own notifications -- $before_n mirrored"

mark="$(gx 'date -u "+%Y-%m-%d %H:%M:%S"' | tr -d '\n')"
[ -n "$mark" ] || abort "could not read the guest clock; the capture window could not be bounded"

# What the desktop's notification server is actually asked to show. The
# mirrored count proves an arrival; this proves WHICH notification arrived,
# because the Notify call carries the summary and the body and both are this
# run's sentinels. The monitor is started BEFORE the post and stopped after --
# a capture opened afterwards would hold nothing and grep clean.
gu "rm -f /tmp/ob-notify-monitor.txt; setsid dbus-monitor --session \"interface='org.freedesktop.Notifications',member='Notify'\" > /tmp/ob-notify-monitor.txt 2>&1 < /dev/null & sleep 1" >/dev/null 2>&1
sleep 3
gx 'pgrep -f "dbus-monitor --session" >/dev/null' \
    || abort "the D-Bus monitor did not start in the guest; the mirrored notification could not be bound to its content"
ok "L16: a D-Bus monitor on org.freedesktop.Notifications.Notify is running before the post"

# Posted by the fixture, not by `cmd notification post`: the picker cannot
# offer com.android.shell, so a notification from it is never shared and the
# gate measures nothing. Quoted as one command -- `adb shell` re-splits the
# line and an unquoted extra is silently mangled.
"${ADB[@]}" shell "am start -n $FIXTURE_ACT --es op post --es id 91 --es tag l16 --es title '$SENT_NOTIF' --es body '$SENT_NOTIF_BODY'" >/dev/null 2>&1
sleep 15
gx 'pkill -f "dbus-monitor --session"; true' >/dev/null 2>&1

# The fixture must actually have posted, or nothing downstream means anything.
grep -q "pkg=$FIXTURE_PKG" <<<"$("${ADB[@]}" shell dumpsys notification 2>/dev/null)" \
    || abort "the fixture posted nothing to the phone's own shade; L16 has no source notification to mirror"
ok "L16: the fixture's notification is on the phone's shade"

nstatus_after="$(ob 'notifications status' 2>&1)"
printf '%s\n' "$nstatus_after" | save "47-L16-status-after.txt"
after_n="$(printf '%s' "$nstatus_after" | sed -n 's/^ *mirrored now *//p' | head -1 | tr -d '[:space:]')"
after_n="${after_n:-0}"
if need_delta "L16: the mirrored count" "$before_n" "$after_n" 1; then
    ok "L16: exactly one notification was mirrored to the packaged desktop ($before_n -> $after_n)"
else
    notok "L16: the mirrored count did not move by exactly the one notification this gate posted ($before_n -> $after_n)"
fi

# Content binding: the desktop was asked to show THIS notification.
notify_cap="$(gx 'cat /tmp/ob-notify-monitor.txt 2>/dev/null' || true)"
n_cap="$(printf '%s\n' "$notify_cap" | grep -c . || true)"
printf '%s\n' "$notify_cap" | save "47b-L16-notify-dbus.txt"
if [ "${n_cap:-0}" -lt 2 ] 2>/dev/null; then
    notok "L16: the D-Bus capture holds ${n_cap} line(s); the mirrored notification cannot be bound to its content"
else
    ok "L16: captured $n_cap line(s) of Notify traffic bracketing the post"
    if grep -qF "$SENT_NOTIF" <<<"$notify_cap" && grep -qF "$SENT_NOTIF_BODY" <<<"$notify_cap"; then
        ok "L16: the desktop's Notify call carries BOTH this run's sentinels -- the notification shown on the guest is the one the phone posted"
    else
        notok "L16: the Notify traffic does not carry this run's sentinels; what was mirrored is not this notification"
    fi
fi

# The privacy half. It is only evidence if the capture is non-empty AND covers
# the operation: grepping an empty journal for a sentinel returns 0 hits and
# proves nothing whatsoever.
jnl="$(gu "journalctl --user -u omnibridged --no-pager --since '$mark' 2>/dev/null" || true)"
n_jnl="$(printf '%s\n' "$jnl" | grep -c . || true)"
printf '%s\n' "$jnl" | save "48-L16-journal.txt"
if [ "${n_jnl:-0}" -lt 2 ] 2>/dev/null; then
    na "L16: the 'no content in the journal' half is NOT asserted here. The daemon logs nothing for a mirrored notification at its default level, so the capture covering the operation holds ${n_jnl} line(s) and a grep over it would pass vacuously. Release Readiness R2 takes this at TRACE with sentinels, which is where it can mean something."
else
    ok "L16: captured $n_jnl journal line(s) covering the notification (non-vacuous)"
    if grep -qF "$SENT_NOTIF" <<<"$jnl"; then
        notok "L16/SEC-LOG-02: the notification TITLE sentinel appears in the daemon journal"
    elif grep -qF "$SENT_NOTIF_BODY" <<<"$jnl"; then
        notok "L16/SEC-LOG-02: the notification BODY sentinel appears in the daemon journal"
    else
        ok "L16/SEC-LOG-02: neither sentinel appears in $n_jnl journal lines"
    fi
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
