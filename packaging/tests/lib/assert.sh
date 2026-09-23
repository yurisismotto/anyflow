#!/usr/bin/env bash
# assert.sh — the fail-loud primitives, one per false-green this project has
# actually suffered.
#
# THE RULE
# --------
#   A test, certification gate or harness must fail loudly when its measurement
#   precondition is absent. A PASS with zero observed evidence is INVALID --
#   and so is a FAIL measured against a precondition the harness destroyed.
#
# WHY THIS FILE IS SHORT, AND WHY IT IS NOT A FRAMEWORK
# -----------------------------------------------------
# Every function here exists because a specific harness in this repository
# reported the wrong answer, and the comment on each one names that occurrence.
# Nothing is here because it seemed generally wise. A general framework would
# have to guess at failure modes; this file only has to remember them.
#
# Twenty-nine false results are on record across four waves:
#
#   Packaging v1        7  an absent `runuser`, an empty tracing capture, a glob
#                          that skipped a gate, `bash -s` with no stdin, a subuid
#                          that could not write, a relative path read as a named
#                          volume, six packages built with one shipped
#   Lifecycle closure   9  PACKAGING-V1 -> RELEASE-LIFECYCLE-CLOSURE-V1.md §5
#   Peer-gate closure  10  RELEASE-PEER-GATES-CLOSURE-V1.md §5
#   Evidence closure    1  `grep -q` behind a pipe under pipefail
#   Signing foundation  2  a signing subkey's fingerprint is not its primary's
#
# USAGE
# -----
# Each function prints a diagnostic and returns non-zero. It does not exit, so
# the caller chooses:
#
#     need_tool qrencode || abort "the QR cannot be rendered"
#     contains "$capture" "transfer=$id" || notok "the window missed the transfer"
#
# Every one is exercised by packaging/tests/harness-selftests.sh, which asserts
# that it REJECTS its failure mode and ACCEPTS the good case. A primitive that
# only ever returns success would pass a suite that only checked the good case.

# Guard against double-sourcing: these are plain function definitions and
# re-sourcing is harmless, but the flag makes "did the harness load this?"
# answerable, which packaging-checks.sh asks statically.
# shellcheck disable=SC2034  # read by packaging-checks.sh H2, not by this file
export OMNIBRIDGE_ASSERT_SH=1

_af() { printf 'assert: FAILED: %s\n' "$*" >&2; return 1; }

# ---------------------------------------------------------------------------
# need_tool TOOL...
#
# Packaging v1: `install-smoke.sh` called `runuser`, which was not installed in
# the container. The call failed, the harness did not look, and the gate passed
# having run nothing. Absent tooling is the cheapest false green there is.
# ---------------------------------------------------------------------------
need_tool() {
    local t missing=()
    for t in "$@"; do command -v "$t" >/dev/null 2>&1 || missing+=("$t"); done
    [ "${#missing[@]}" -eq 0 ] || _af "required tool(s) not installed: ${missing[*]}"
}

# ---------------------------------------------------------------------------
# need_nonempty LABEL TEXT [MIN_LINES]
#
# Packaging v1: the notification canary's tracing capture recorded nothing, and
# the sentinel search over it passed. Lifecycle closure: L16's journal held one
# line. An empty capture answers every question with "absent", including the
# ones whose true answer is "present".
# ---------------------------------------------------------------------------
need_nonempty() {
    local label="$1" text="$2" min="${3:-1}" n
    [ -n "${text//[[:space:]]/}" ] || { _af "$label is empty; a search over it would prove nothing"; return 1; }
    n="$(grep -c . <<<"$text" || true)"
    [ "${n:-0}" -ge "$min" ] 2>/dev/null \
        || _af "$label holds $n line(s), fewer than the $min this gate needs to be non-vacuous"
}

# ---------------------------------------------------------------------------
# contains LABEL_TEXT PATTERN   /   absent LABEL_TEXT PATTERN
#
# Evidence closure: `printf '%s' "$capture" | grep -q PATTERN` missed a string
# the capture contained in **225 of 300 runs**. `grep -q` exits on its first
# match, the producer dies of SIGPIPE, and `set -o pipefail` turns that into
# "no match". On a privacy gate written "absent is a pass", that is a PASS on a
# real leak.
#
# A here-string is not a pipeline, so pipefail cannot apply: 0 misses in the
# same 300 runs. Never reintroduce the pipe; packaging-checks.sh H1 fails CI
# on it.
# ---------------------------------------------------------------------------
contains() { grep -qF -- "$2" <<<"$1"; }
contains_re() { grep -qE -- "$2" <<<"$1"; }
absent()   { ! grep -qF -- "$2" <<<"$1"; }

# ---------------------------------------------------------------------------
# need_window_covers LABEL CAPTURE ANCHOR
#
# Lifecycle closure §5 defects 2 and 9, and peer-gate closure §5 defect 19: a
# capture window that does not correspond to the operation under test. The S3
# grep matched a previous session's failures; the L15 grep matched a previous
# run's successful transfer; the session grep characterised a live connection
# from one that had already ended.
#
# The fix is not a better timestamp. It is an ANCHOR the product itself wrote
# about THIS operation -- a transfer id, an InvocationID, a filename the app
# logged -- whose presence proves the window caught the right thing. Note the
# asymmetry this enforces: the anchor must be PRESENT before any claim that
# something else is ABSENT can mean anything.
# ---------------------------------------------------------------------------
need_window_covers() {
    local label="$1" capture="$2" anchor="$3"
    need_nonempty "$label" "$capture" || return 1
    contains "$capture" "$anchor" \
        || _af "$label does not contain '$anchor', so the window does not cover the operation under test; any 'absent' result over it is vacuous"
}

# ---------------------------------------------------------------------------
# need_exact_count LABEL ACTUAL EXPECTED
#
# Packaging v1: six packages were built and one was shipped, and every gate
# downstream measured the one. A count that is merely non-zero is not a count.
# ---------------------------------------------------------------------------
need_exact_count() {
    local label="$1" actual="${2//[[:space:]]/}" expected="$3"
    [ "${actual:-x}" = "$expected" ] \
        || _af "$label is ${actual:-<unreadable>}, expected exactly $expected"
}

# ---------------------------------------------------------------------------
# need_glob DIR PATTERN EXPECTED
#
# Packaging v1: a glob that matched nothing made L17 skip its whole group in
# silence. A glob is a question; zero matches is an answer, and it is usually
# the wrong one.
# ---------------------------------------------------------------------------
need_glob() {
    local dir="$1" pattern="$2" expected="$3" n
    [ -d "$dir" ] || { _af "need_glob: '$dir' is not a directory"; return 1; }
    n="$(find "$dir" -maxdepth 1 -name "$pattern" -type f 2>/dev/null | grep -c . || true)"
    [ "${n:-0}" -eq "$expected" ] 2>/dev/null \
        || _af "expected exactly $expected file(s) matching '$pattern' in $dir, found ${n:-0}"
}

# ---------------------------------------------------------------------------
# need_stdin_answer CMD_DESCRIPTION
#
# Packaging v1: `bash -s` with no stdin. Lifecycle closure §5 defect 4:
# `omnibridge pair` backgrounded with no stdin read EOF from its `[y/N]` prompt
# and DECLINED the pairing, exiting 0. Two operator scans were lost to it.
#
# The lesson generalises past stdin: a command that prompts and is given
# nothing does not hang, it takes the default -- and the default is usually
# "no". So the harness must supply a BOUNDED answer and then assert the
# outcome it wanted, never assume the exit code meant consent.
#
# This checks the half that can be checked mechanically: that the fd is not a
# closed or empty stream before an interactive command is started.
# ---------------------------------------------------------------------------
need_stdin_answer() {
    local what="$1"
    if [ -t 0 ]; then return 0; fi
    if [ ! -r /dev/stdin ]; then
        _af "$what will read EOF from its prompt and take the default answer, which is usually 'no'"
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# need_writable PATH AS_USER
#
# Packaging v1: a subuid that could not write. The gate's setup silently
# produced nothing and the assertions ran against an empty directory.
# ---------------------------------------------------------------------------
need_writable() {
    local path="$1" as_user="${2:-}" probe
    probe="$path/.omnibridge-write-probe.$$"
    if [ -n "$as_user" ]; then
        runuser -u "$as_user" -- sh -c "touch '$probe' 2>/dev/null && rm -f '$probe'" \
            || { _af "$as_user cannot write to $path; anything staged there would be silently absent"; return 1; }
    else
        ( touch "$probe" 2>/dev/null && rm -f "$probe" ) \
            || { _af "cannot write to $path; anything staged there would be silently absent"; return 1; }
    fi
}

# ---------------------------------------------------------------------------
# need_abs_path LABEL PATH
#
# Packaging v1: a relative path was read as a named volume by the container
# runtime, so the mount pointed at an empty volume instead of the source tree,
# and every file assertion inside measured nothing.
# ---------------------------------------------------------------------------
need_abs_path() {
    local label="$1" path="$2"
    case "$path" in
        /*) : ;;
        *) _af "$label must be an absolute path, got '$path' -- a relative path is read as a named volume by podman/docker and mounts an empty volume" ; return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# need_ran LABEL MARKER_BEFORE MARKER_AFTER
#
# Peer-gate closure §5 defects 10 and 11: the harness force-stopped the app it
# was about to measure, dropping the TLS session and snoozing the notification
# listener -- then reported the gates as product failures. The operation never
# ran, and nothing said so.
#
# Two observations of the same counter, taken around the operation, with the
# change asserted. A counter that did not move means the operation did not
# happen; that is a statement about the harness until proved otherwise.
# ---------------------------------------------------------------------------
need_ran() {
    local label="$1" before="${2//[[:space:]]/}" after="${3//[[:space:]]/}"
    [ -n "$before" ] && [ -n "$after" ] \
        || { _af "$label: a before/after observation is missing, so nothing can be said about whether the operation ran"; return 1; }
    [ "$before" != "$after" ] \
        || _af "$label did not change across the operation ($before -> $after); the operation under test did not run"
}

# ---------------------------------------------------------------------------
# need_delta LABEL BEFORE AFTER EXPECTED_DELTA
#
# Peer-gate closure §5 defect 14, and L16: `mirrored now` counts what is
# CURRENTLY mirrored, not a running total. An older entry expiring as a new one
# arrives leaves it unchanged, so `after > before` is false on a notification
# that was mirrored perfectly -- and `after != before` would accept two
# arrivals as one. The expected delta is stated and required exactly.
# ---------------------------------------------------------------------------
need_delta() {
    local label="$1" before="${2//[[:space:]]/}" after="${3//[[:space:]]/}" want="$4"
    case "$before$after" in
        *[!0-9]*|'') _af "$label: before='$before' after='$after' are not both numbers"; return 1 ;;
    esac
    [ "$(( after - before ))" -eq "$want" ] \
        || _af "$label changed by $(( after - before )), expected exactly $want ($before -> $after)"
}
