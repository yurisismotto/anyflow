#!/usr/bin/env bash
# guest-agent.sh — drive a libvirt guest through qemu-guest-agent, and fail
# loudly when the channel, the guest or the command is not really there.
#
# Why this exists: the Linux lifecycle gates need root on an installed desktop.
# This host has no non-interactive sudo, but `qemu:///system` is reachable
# (the invoking user is in the `libvirt` group), so root *inside a guest* is
# available. The virtio-serial channel carries no IP traffic and therefore
# cannot influence discovery, pairing or anything else under certification.
#
# Every function here obeys one rule, which the release brief states and
# packaging v1 learned seven times the hard way:
#
#     A PASS with zero observed evidence is INVALID.
#
# So: no function returns success on an empty capture, an absent channel, a
# domain that is not running, or a command that never started.

set -o pipefail

GA_CONNECT="${GA_CONNECT:-qemu:///system}"
GA_EXEC_TIMEOUT="${GA_EXEC_TIMEOUT:-900}"

ga_die() { printf 'guest-agent: FATAL: %s\n' "$*" >&2; return 1; }

_ga_virsh() { virsh -c "$GA_CONNECT" "$@"; }

# ga_require_domain DOMAIN — the domain exists and is running.
ga_require_domain() {
    local dom="$1" state
    [ -n "$dom" ] || { ga_die "ga_require_domain: no domain given"; return 1; }
    _ga_virsh dominfo "$dom" >/dev/null 2>&1 \
        || { ga_die "domain '$dom' is not defined on $GA_CONNECT"; return 1; }
    state="$(_ga_virsh domstate "$dom" 2>/dev/null)"
    case "$state" in
        running|executando) : ;;
        *) ga_die "domain '$dom' is not running (state: ${state:-unknown})"; return 1 ;;
    esac
}

# ga_require_channel DOMAIN — the domain XML actually declares the agent
# channel. A guest with no channel makes every later agent call fail in a way
# that is easy to misread as "the guest is slow".
ga_require_channel() {
    local dom="$1"
    _ga_virsh dumpxml "$dom" 2>/dev/null | grep -q "org.qemu.guest_agent.0" \
        || { ga_die "domain '$dom' declares no org.qemu.guest_agent.0 channel"; return 1; }
}

# ga_ping DOMAIN [TIMEOUT] — block until the agent answers, or fail.
ga_ping() {
    local dom="$1" timeout="${2:-180}" waited=0
    ga_require_domain "$dom" || return 1
    ga_require_channel "$dom" || return 1
    while [ "$waited" -lt "$timeout" ]; do
        if _ga_virsh qemu-agent-command "$dom" '{"execute":"guest-ping"}' >/dev/null 2>&1; then
            return 0
        fi
        sleep 3; waited=$(( waited + 3 ))
    done
    ga_die "guest agent in '$dom' did not answer guest-ping within ${timeout}s"
    return 1
}

# ga_exec DOMAIN COMMAND... — run COMMAND as root in the guest via /bin/sh -c.
# Stdout of the guest command goes to stdout; stderr to stderr; the guest's
# exit code becomes this function's exit code.
#
# Preconditions asserted, each with its own message:
#   * the domain is running and has the channel;
#   * guest-exec returned a real PID;
#   * guest-exec-status reported `exited` before the deadline;
#   * the status payload carried an exitcode field at all.
ga_exec() {
    local dom="$1"; shift
    local cmd="$*"
    [ -n "$cmd" ] || { ga_die "ga_exec: empty command for '$dom'"; return 1; }
    ga_require_domain "$dom" || return 1

    local req pid resp exited code out err waited=0
    req="$(jq -cn --arg c "$cmd" \
        '{execute:"guest-exec",arguments:{path:"/bin/sh",arg:["-c",$c],"capture-output":true}}')"

    resp="$(_ga_virsh qemu-agent-command "$dom" "$req" 2>&1)" \
        || { ga_die "guest-exec rejected by '$dom': $resp"; return 1; }
    pid="$(printf '%s' "$resp" | jq -r '.return.pid // empty' 2>/dev/null)"
    [ -n "$pid" ] \
        || { ga_die "guest-exec in '$dom' returned no pid (response: $resp)"; return 1; }

    while [ "$waited" -lt "$GA_EXEC_TIMEOUT" ]; do
        resp="$(_ga_virsh qemu-agent-command "$dom" \
            "$(jq -cn --argjson p "$pid" '{execute:"guest-exec-status",arguments:{pid:$p}}')" 2>&1)" \
            || { ga_die "guest-exec-status failed for pid $pid in '$dom': $resp"; return 1; }
        exited="$(printf '%s' "$resp" | jq -r '.return.exited // false' 2>/dev/null)"
        [ "$exited" = "true" ] && break
        sleep 2; waited=$(( waited + 2 ))
    done
    [ "$exited" = "true" ] \
        || { ga_die "command did not exit within ${GA_EXEC_TIMEOUT}s in '$dom': $cmd"; return 1; }

    code="$(printf '%s' "$resp" | jq -r '.return.exitcode // empty' 2>/dev/null)"
    # `// empty` also swallows a real 0, so distinguish the two explicitly.
    if [ -z "$code" ]; then
        printf '%s' "$resp" | jq -e 'has("return") and (.return|has("exitcode"))' >/dev/null 2>&1 \
            && code=0 \
            || { ga_die "guest-exec-status carried no exitcode in '$dom' (response: $resp)"; return 1; }
    fi

    out="$(printf '%s' "$resp" | jq -r '.return["out-data"] // empty' 2>/dev/null)"
    err="$(printf '%s' "$resp" | jq -r '.return["err-data"] // empty' 2>/dev/null)"
    [ -n "$out" ] && printf '%s' "$out" | base64 -d 2>/dev/null
    [ -n "$err" ] && printf '%s' "$err" | base64 -d 2>/dev/null >&2
    return "$code"
}

# ga_exec_ok DOMAIN COMMAND... — ga_exec, but non-zero is fatal and the output
# is echoed so a failure is never silent.
ga_exec_ok() {
    local dom="$1"; shift
    local out rc
    out="$(ga_exec "$dom" "$@" 2>&1)"; rc=$?
    if [ "$rc" -ne 0 ]; then
        printf 'guest-agent: FATAL: command failed (exit %d) in %s: %s\n' "$rc" "$dom" "$*" >&2
        printf '%s\n' "$out" >&2
        return "$rc"
    fi
    printf '%s' "$out"
}

# ga_exec_nonempty DOMAIN COMMAND... — succeeds only if the command succeeded
# AND produced output. This is the anti-vacuous primitive: a gate that greps a
# log must not pass because the log was empty.
ga_exec_nonempty() {
    local dom="$1"; shift
    local out rc
    out="$(ga_exec "$dom" "$@")"; rc=$?
    if [ "$rc" -ne 0 ]; then
        ga_die "command failed (exit $rc) in $dom: $*"; return "$rc"
    fi
    if [ -z "${out//[[:space:]]/}" ]; then
        ga_die "command succeeded but produced NO output in $dom, which this gate treats as no evidence: $*"
        return 1
    fi
    printf '%s' "$out"
}

# ga_wait_for DOMAIN TIMEOUT COMMAND... — poll until COMMAND exits 0.
ga_wait_for() {
    local dom="$1" timeout="$2"; shift 2
    local waited=0
    while [ "$waited" -lt "$timeout" ]; do
        ga_exec "$dom" "$@" >/dev/null 2>&1 && return 0
        sleep 3; waited=$(( waited + 3 ))
    done
    ga_die "condition never became true within ${timeout}s in '$dom': $*"
    return 1
}

# ga_put DOMAIN LOCAL REMOTE — copy a local file into the guest over the
# virtio-serial channel, then verify it by digest. No network is involved.
ga_put() {
    local dom="$1" local_path="$2" remote="$3"
    [ -f "$local_path" ] || { ga_die "ga_put: '$local_path' is not a file"; return 1; }
    local want got handle b64 chunk
    want="$(sha256sum "$local_path" | cut -d' ' -f1)"

    handle="$(_ga_virsh qemu-agent-command "$dom" \
        "$(jq -cn --arg p "$remote" '{execute:"guest-file-open",arguments:{path:$p,mode:"wb"}}')" 2>&1 \
        | jq -r '.return // empty')"
    [ -n "$handle" ] || { ga_die "ga_put: could not open '$remote' in '$dom'"; return 1; }

    # 64 KiB of base64 per write. The ceiling is the kernel's MAX_ARG_STRLEN,
    # which is 128 KiB for a single argv entry -- a 128 KiB chunk fails with
    # "argument list too long" before the agent ever sees it.
    b64="$(base64 -w0 "$local_path")"
    local off=0 len=${#b64} step=65536
    while [ "$off" -lt "$len" ]; do
        chunk="${b64:$off:$step}"
        _ga_virsh qemu-agent-command "$dom" \
            "$(jq -cn --argjson h "$handle" --arg d "$chunk" \
               '{execute:"guest-file-write",arguments:{handle:$h,"buf-b64":$d}}')" >/dev/null 2>&1 \
            || { ga_die "ga_put: write failed at offset $off"; return 1; }
        off=$(( off + step ))
    done
    _ga_virsh qemu-agent-command "$dom" \
        "$(jq -cn --argjson h "$handle" '{execute:"guest-file-close",arguments:{handle:$h}}')" >/dev/null 2>&1

    got="$(ga_exec "$dom" "sha256sum '$remote' 2>/dev/null | cut -d' ' -f1" | tr -d '[:space:]')"
    [ "$got" = "$want" ] \
        || { ga_die "ga_put: digest mismatch for '$remote' (want $want, got ${got:-<none>})"; return 1; }
    return 0
}
