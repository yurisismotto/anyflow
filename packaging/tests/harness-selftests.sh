#!/usr/bin/env bash
# harness-selftests.sh — does the harness fail when it should?
#
# Every certification gate in this repository rests on an assumption nobody had
# tested: that when the thing being measured is absent, the harness says so.
# Twenty-nine times across four waves, it did not. This file tests that
# assumption directly.
#
# Each case does two things, and BOTH matter:
#
#   REJECTS  the primitive returns non-zero on its failure mode
#   ACCEPTS  the primitive returns zero on the good case
#
# Without the second half a primitive hard-coded to `return 1` would pass every
# rejection test here — which is the same vacuity in a new place. Without the
# first half the whole file is decoration.
#
# The failure modes are the ones actually observed, named in
# packaging/tests/lib/assert.sh against the wave that suffered them.

set -uo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/assert.sh
. "$HERE/lib/assert.sh"

PASS=0; FAIL=0; declare -a FAILED=()
ok()      { PASS=$(( PASS + 1 )); printf 'ok    %s\n' "$*"; }
notok()   { FAIL=$(( FAIL + 1 )); FAILED+=("$*"); printf 'not ok  %s\n' "$*"; }
section() { printf '\n== %s ==\n' "$*"; }

# rejects DESC CMD... — the primitive must return non-zero, and must say why.
rejects() {
    local desc="$1"; shift
    local out rc
    out="$("$@" 2>&1)"; rc=$?
    if [ "$rc" -eq 0 ]; then
        notok "REJECTS $desc — returned 0; the harness would have passed on nothing"
    elif [ -z "${out//[[:space:]]/}" ]; then
        notok "REJECTS $desc — returned $rc but printed no diagnostic; a silent failure is hard to act on"
    elif grep -qiE 'syntax error|command not found|no such file' <<<"$out"; then
        # The rejection must come from the PRIMITIVE, not from a shell that
        # could not load it. Measured: on an Ubuntu runner `/bin/sh` is dash,
        # which cannot parse assert.sh's arrays and here-strings, so a case
        # spawned with `sh -c` returned non-zero for a reason that had nothing
        # to do with the thing under test -- and this file recorded it as a
        # pass. A false green inside the suite whose subject is false greens.
        notok "REJECTS $desc — returned $rc because the SHELL failed, not the primitive: $(tr '\n' ' ' <<<"$out" | head -c 110)"
    else
        ok "REJECTS $desc — $(printf '%s' "$out" | sed 's/^assert: FAILED: //' | tr '\n' ' ' | head -c 105)"
    fi
}
# The two kinds are checked differently, on purpose.
#
#   need_*            ASSERTIONS. They must return non-zero AND print why, because
#                     whoever reads the run needs to know what was missing.
#   contains/absent   PREDICATES. They must return the right answer and print
#                     NOTHING, because the caller composes them into its own
#                     message: `contains "$cap" "$s" || notok "the window missed it"`.
#                     Requiring a diagnostic from these would push duplicate text
#                     into every call site.
#
# is_false/is_true check a predicate's answer alone.
is_false() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then
        notok "PREDICATE $desc — answered true, and the truth is false"
    else
        ok "PREDICATE $desc — answers false, as it must"
    fi
}
is_true() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then
        ok "PREDICATE $desc — answers true, as it must"
    else
        notok "PREDICATE $desc — answered false, and the truth is true"
    fi
}

# accepts DESC CMD... — the primitive must return zero on the good case.
accepts() {
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then
        ok "ACCEPTS $desc"
    else
        notok "ACCEPTS $desc — returned non-zero on a case that is fine; a primitive that rejects everything proves nothing"
    fi
}

WORK="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-selftest.XXXXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
section "A missing executable  (Packaging v1: an absent runuser)"
# ---------------------------------------------------------------------------
rejects "a tool that is not installed" \
    need_tool omnibridge-definitely-not-a-real-tool-4f9a
rejects "one absent tool among several present ones" \
    need_tool sh grep omnibridge-definitely-not-a-real-tool-4f9a
accepts "tools that are installed" need_tool sh grep find

# ---------------------------------------------------------------------------
section "An empty capture  (Packaging v1: an empty tracing capture; L16: one line)"
# ---------------------------------------------------------------------------
rejects "an empty capture"            need_nonempty "the journal" ""
rejects "a whitespace-only capture"   need_nonempty "the journal" $'  \n\t\n  '
rejects "a capture below the minimum" need_nonempty "the journal" $'one line' 20
accepts "a capture with real content" need_nonempty "the journal" $'line one\nline two'
accepts "a capture meeting an explicit minimum" \
    need_nonempty "the journal" "$(seq 1 30)" 20

# ---------------------------------------------------------------------------
section "A search that loses a match  (Evidence closure: 225 misses in 300 runs)"
# ---------------------------------------------------------------------------
# The capture is deliberately large and the match deliberately early: that is
# the exact shape that made `printf | grep -q` fail three times in four.
BIG="$(seq 1 4000 | sed 's/^/filler line /')"
BIG="MATCHME-EARLY-SENTINEL
$BIG"
is_true  "contains(), on a sentinel near the start of a 4000-line capture" \
    contains "$BIG" "MATCHME-EARLY-SENTINEL"
is_false "contains(), on a string the capture does not hold" \
    contains "$BIG" "THIS-STRING-IS-NOT-THERE-91af"
is_true  "absent(), on a string that really is absent" \
    absent "$BIG" "THIS-STRING-IS-NOT-THERE-91af"
is_false "absent(), on a string that is present — the privacy-gate direction" \
    absent "$BIG" "MATCHME-EARLY-SENTINEL"

# The regression itself: run the match many times and require it never to be
# lost. Under the old `printf | grep -q` form this failed ~75% of iterations.
miss=0
for _ in $(seq 1 200); do contains "$BIG" "MATCHME-EARLY-SENTINEL" || miss=$(( miss + 1 )); done
[ "$miss" -eq 0 ] \
    && ok "REGRESSION: 200/200 searches of a large capture found a present sentinel (the pipe form missed 75%)" \
    || notok "REGRESSION: $miss of 200 searches LOST a present sentinel; the pipefail/SIGPIPE defect is back"

# ---------------------------------------------------------------------------
section "A window that does not cover the operation  (defects 2, 9, 19)"
# ---------------------------------------------------------------------------
rejects "a window with no anchor for this operation" \
    need_window_covers "the journal" $'unrelated line\nanother unrelated line' "transfer=abc12345"
rejects "an empty window, before any anchor is even looked for" \
    need_window_covers "the journal" "" "transfer=abc12345"
accepts "a window carrying this operation's own anchor" \
    need_window_covers "the journal" $'noise\noffering a file transfer=abc12345 size=48\nnoise' "transfer=abc12345"

# ---------------------------------------------------------------------------
section "A wrong artifact count  (Packaging v1: six built, one shipped)"
# ---------------------------------------------------------------------------
rejects "a count that is wrong"                need_exact_count "installed packages" "1" "2"
rejects "a count that is zero"                 need_exact_count "installed packages" "0" "2"
rejects "a count that could not be read"       need_exact_count "installed packages" "" "2"
accepts "a count that is exactly right"        need_exact_count "installed packages" "2" "2"
accepts "a count with the whitespace guests add" need_exact_count "installed packages" $' 2 \n' "2"

# ---------------------------------------------------------------------------
section "A glob that matched nothing  (Packaging v1: L17 skipped its group)"
# ---------------------------------------------------------------------------
mkdir -p "$WORK/pkgs"
: > "$WORK/pkgs/omnibridge_1.0_amd64.deb"
: > "$WORK/pkgs/omnibridge-gui_1.0_amd64.deb"
rejects "a glob that matches nothing"        need_glob "$WORK/pkgs" '*.rpm' 2
rejects "a glob that matches the wrong number" need_glob "$WORK/pkgs" '*.deb' 3
rejects "a directory that does not exist"    need_glob "$WORK/nope" '*.deb' 2
accepts "a glob that matches exactly the expected count" need_glob "$WORK/pkgs" '*.deb' 2

# ---------------------------------------------------------------------------
section "A relative mount path  (Packaging v1: read as a named volume)"
# ---------------------------------------------------------------------------
rejects "a relative path used as a mount source"  need_abs_path "--pkgdir" "artifacts/fedora44"
rejects "a bare name used as a mount source"      need_abs_path "--pkgdir" "artifacts"
accepts "an absolute mount source"                need_abs_path "--pkgdir" "/tmp/artifacts"

# ---------------------------------------------------------------------------
section "A directory nothing can write to  (Packaging v1: the subuid case)"
# ---------------------------------------------------------------------------
mkdir -p "$WORK/ro" && chmod 500 "$WORK/ro"
if [ "$(id -u)" -eq 0 ]; then
    printf 'n/a   running as root, which can write to a 0500 directory; the unwritable case cannot be staged\n'
else
    rejects "a directory the harness cannot write to" need_writable "$WORK/ro"
fi
accepts "a writable directory" need_writable "$WORK"

# ---------------------------------------------------------------------------
section "An operation that never ran  (defects 10, 11: the session was torn down)"
# ---------------------------------------------------------------------------
rejects "a counter that did not move across the operation" need_ran "mirrored" "3" "3"
rejects "a before/after observation that is missing"       need_ran "mirrored" "3" ""
accepts "a counter that moved"                             need_ran "mirrored" "3" "4"

# ---------------------------------------------------------------------------
section "A delta that is not the one claimed  (defect 14: 'mirrored now' is not a total)"
# ---------------------------------------------------------------------------
rejects "no change where exactly one was expected"   need_delta "mirrored" "3" "3" 1
rejects "two arrivals counted as the one under test" need_delta "mirrored" "3" "5" 1
rejects "a non-numeric observation"                  need_delta "mirrored" "three" "4" 1
accepts "exactly the expected delta"                 need_delta "mirrored" "3" "4" 1
accepts "a delta of one from a cleared baseline"     need_delta "mirrored" "0" "1" 1

# ---------------------------------------------------------------------------
section "A prompt with no stdin  (Packaging v1 bash -s; defect 4: silent decline)"
# ---------------------------------------------------------------------------
# The mechanical half. `omnibridge pair` read EOF from its [y/N] prompt and
# answered "no" while exiting 0, and two operator scans were lost before one
# journal line explained it. What a harness can check before starting such a
# command is that its stdin is not already closed.
# `bash -c`, not `sh -c`: lib/assert.sh declares `#!/usr/bin/env bash` and uses
# arrays and here-strings. On an Ubuntu runner /bin/sh is dash, which cannot
# parse it -- see the note in rejects() for what that cost.
accepts "a command given a real answer on stdin" \
    bash -c '. '"$HERE"'/lib/assert.sh; need_stdin_answer "omnibridge pair" <<<"y"'
rejects "a command whose stdin is closed" \
    bash -c '. '"$HERE"'/lib/assert.sh; exec 0<&-; need_stdin_answer "omnibridge pair"'

printf '\n-----------------------------------------------\n'
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
if [ "$FAIL" -gt 0 ]; then printf '\nFailed:\n'; for g in "${FAILED[@]}"; do printf '  %s\n' "$g"; done; fi
[ "$FAIL" -eq 0 ]
