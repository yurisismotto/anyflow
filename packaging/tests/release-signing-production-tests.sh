#!/usr/bin/env bash
# release-signing-production-tests.sh — the negative tests, run against the
# REAL signed release artifact set rather than a fixture.
#
# WHY THIS EXISTS BESIDE release-signing-tests.sh
# -----------------------------------------------
# That suite proves the scripts behave, using throwaway keys and a fixture
# release built in a temporary directory. It is the right shape for CI: it
# needs no production key and no artifact download, so it runs on every PR.
#
# It cannot answer the question this file answers. "Does verification reject a
# tampered OmniBridge release, signed by the production identity, over the
# artifacts GitHub Actions actually built?" is a different claim, and a fixture
# cannot make it. The four conditions on the production-signing gate --
# RELEASE-SIGNING-FOUNDATION-V1.md §8.9 -- name the real set explicitly:
#
#   2. a REAL release artifact set is signed with it;
#   4. the negative verification tests pass against THAT real signed set.
#
# So this harness is handed the real thing, and is a certification instrument:
# it is run once per signed release, by a human, with the evidence transcribed
# into docs/certification/release/RELEASE-SIGNING-CLOSURE-V1.md. It is not
# wired into CI, because CI has neither the key nor the artifacts.
#
# THE REAL SET IS NEVER MUTATED
# -----------------------------
# Every destructive case runs against a COPY. The directory passed in is read,
# and its digest is taken before and after to prove it: a harness that
# corrupted the evidence it was certifying would be worse than no harness.
#
# EVERY CASE PROVES ITS PRECONDITION
# ----------------------------------
# AGENTS.md: a FAIL measured against a precondition the harness itself
# destroyed says nothing about the product. A verifier that rejects everything
# passes every negative test ever written. So each case here does three things
# in order:
#
#   1. asserts the COPY verifies BEFORE it is damaged -- the positive control,
#      per case, not once at the top;
#   2. asserts the damage actually landed -- the byte changed, the file is
#      gone, the digests now differ;
#   3. only then requires the verifier to reject it.
#
# Step 1 is the one that is easy to skip and the one that carries the weight.

set -uo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$HERE/../.." && pwd)"
# shellcheck source=packaging/tests/lib/assert.sh
. "$HERE/lib/assert.sh"

VERIFY="$ROOT/packaging/release/verify-release.sh"

DIR=""; PUBKEY=""; FPR=""; RUNLOG=""; CIDIR=""; COMMIT=""
usage() {
    cat >&2 <<USAGE
usage: $0 --dir SIGNED_RELEASE_DIR --pubkey FILE --fingerprint FPR \\
          --run-log FILE --ci-dir DIR --commit SHA

  --dir          the real, signed release directory (artifacts, SHA256SUMS,
                 SHA256SUMS.asc). Read only; never modified.
  --pubkey       the PUBLIC half of the production key. Public material only;
                 the harness refuses a file carrying a secret.
  --fingerprint  the published PRIMARY fingerprint users are told to check.
  --run-log      the full GitHub Actions log of the run that built --ci-dir,
                 captured with 'gh run view <id> --log'.
  --ci-dir       the artifact set exactly as CI produced it, before any local
                 signing. Proves CI shipped no signature and held no key.
  --commit       the immutable commit SHA the artifacts were built from. Used
                 to anchor --run-log to THIS build before anything is
                 concluded from what that log lacks.
USAGE
    exit 2
}
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) DIR="$2"; shift 2 ;;
        --pubkey) PUBKEY="$2"; shift 2 ;;
        --fingerprint) FPR="$2"; shift 2 ;;
        --run-log) RUNLOG="$2"; shift 2 ;;
        --ci-dir) CIDIR="$2"; shift 2 ;;
        --commit) COMMIT="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done
[ -n "$DIR" ] && [ -n "$PUBKEY" ] && [ -n "$FPR" ] && [ -n "$RUNLOG" ] \
    && [ -n "$CIDIR" ] && [ -n "$COMMIT" ] || usage

PASS=0; FAIL=0; declare -a FAILED=()
ok()    { PASS=$(( PASS + 1 )); printf 'ok    %s\n' "$*"; }
notok() { FAIL=$(( FAIL + 1 )); FAILED+=("$*"); printf 'not ok  %s\n' "$*"; }
section(){ printf '\n== %s ==\n' "$*"; }
die()   { printf '\nPRECONDITION FAILED: %s\n' "$*" >&2; exit 3; }

need_tool gpg sha256sum find git tar || die "the harness cannot run without its tools"
[ -x "$VERIFY" ] || die "verify-release.sh is not executable at $VERIFY"
[ -d "$DIR" ]    || die "--dir '$DIR' is not a directory"
[ -f "$PUBKEY" ] || die "--pubkey '$PUBKEY' does not exist"
[ -s "$RUNLOG" ] || die "--run-log '$RUNLOG' is empty; a search over it would prove nothing"
[ -d "$CIDIR" ]  || die "--ci-dir '$CIDIR' is not a directory"

# The public key must be public. If it carries a secret, every later claim in
# the closure document about "no secret material" is false, and the person
# running this is about to publish the opposite of what they think.
SCAN="$(mktemp -d)"; chmod 700 "$SCAN"
gpg --homedir "$SCAN" --batch --quiet --import <"$PUBKEY" >/dev/null 2>&1 \
    || die "--pubkey '$PUBKEY' is not a keyring gpg can read"
nsec="$(find "$SCAN/private-keys-v1.d" -type f -name '*.key' 2>/dev/null | wc -l)"
[ "${nsec:-0}" -eq 0 ] || die "--pubkey '$PUBKEY' carries SECRET key material; it is not a public key"
gpgconf --homedir "$SCAN" --kill gpg-agent >/dev/null 2>&1 || true
rm -rf "$SCAN"

WORK="$(mktemp -d -t omnibridge-prod-signing.XXXXXXXX)" || die "no temp dir"
trap 'rm -rf "$WORK"' EXIT INT TERM
printf 'work: %s\n' "$WORK"

# A fingerprint of the whole real set, taken now and re-taken at the end. The
# real directory is evidence; this harness must leave it byte-identical.
realsum() { find "$DIR" -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1; }
REAL_BEFORE="$(realsum)"

# A throwaway identity that has nothing to do with OmniBridge, for NEG-03.
STRANGER="$WORK/stranger-home"; mkdir -p "$STRANGER"; chmod 700 "$STRANGER"
gpg --homedir "$STRANGER" --batch --quiet --pinentry-mode loopback --passphrase '' \
    --quick-generate-key "Unrelated Key — not OmniBridge" ed25519 sign never >/dev/null 2>&1 \
    || die "could not generate the unrelated key NEG-03 needs"
STRANGER_FPR="$(gpg --homedir "$STRANGER" --with-colons --list-keys | awk -F: '/^fpr:/ {print $10; exit}')"
[ -n "$STRANGER_FPR" ] || die "the unrelated key has no fingerprint"
[ "$STRANGER_FPR" != "$FPR" ] || die "the unrelated key collided with the production fingerprint"
gpg --homedir "$STRANGER" --batch --export "$STRANGER_FPR" > "$WORK/stranger-pub.gpg"
[ -s "$WORK/stranger-pub.gpg" ] || die "the unrelated public key could not be exported"
gpgconf --homedir "$STRANGER" --kill gpg-agent >/dev/null 2>&1 || true

verify_copy() { "$VERIFY" --dir "$1" --keyring "$PUBKEY" --fingerprint "$FPR" >"$2" 2>&1; }

# Make a copy, and REQUIRE it to verify before anything is done to it. This is
# the per-case positive control: without it, a rejection below would prove
# only that copying broke something.
fresh_copy() {
    local dest="$1"
    cp -r "$DIR" "$dest" || die "could not copy the real set to $dest"
    verify_copy "$dest" "$dest.control.log" \
        || die "the untouched COPY at $dest does not verify; every rejection below would be vacuous. $(tail -2 "$dest.control.log")"
}

# ---------------------------------------------------------------------------
section "Positive control — the real signed set verifies as published"
# ---------------------------------------------------------------------------
if "$VERIFY" --dir "$DIR" --keyring "$PUBKEY" --fingerprint "$FPR" >"$WORK/real.log" 2>&1; then
    ok "the REAL signed release verifies against the published primary fingerprint"
else
    printf '%s\n' "$(cat "$WORK/real.log")" >&2
    die "the real signed set does not verify; there is nothing to run negative tests against"
fi
n_entries="$(grep -c . <"$DIR/SHA256SUMS" || true)"
if [ "${n_entries:-0}" -ge 14 ]; then
    ok "SHA256SUMS covers $n_entries artifacts"
else
    notok "SHA256SUMS covers only ${n_entries:-0} artifacts; the real set should carry at least 14"
fi
sig_line="$(grep -E 'good signature by' "$WORK/real.log" || true)"
if need_nonempty "the verifier's signature line" "$sig_line"; then
    ok "the verifier reported the signer: ${sig_line#verify: }"
else
    notok "the verifier printed no signature line, so 'VERIFIED' rests on nothing readable"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-01 — one byte of a real artifact is modified"
# ---------------------------------------------------------------------------
T1="$WORK/neg01"; fresh_copy "$T1"
ok "NEG-01 control: the untouched copy verifies"
TARGET="$(find "$T1/ubuntu2404" -name '*.deb' -type f | sort | head -1)"
[ -n "$TARGET" ] || die "NEG-01 found no .deb to damage in the real set"
REL="${TARGET#$T1/}"
before_hash="$(sha256sum "$TARGET" | cut -d' ' -f1)"
# One byte, in place, at a fixed offset. Not an append: a truncation or a
# trailing byte is a cruder change than a real substitution would make.
printf '\xff' | dd of="$TARGET" bs=1 seek=512 count=1 conv=notrunc status=none \
    || die "NEG-01 could not write the byte"
after_hash="$(sha256sum "$TARGET" | cut -d' ' -f1)"
if need_ran "NEG-01's one-byte edit to $REL" "$before_hash" "$after_hash"; then
    ok "NEG-01 precondition: $REL changed ($before_hash -> $after_hash)"
else
    notok "NEG-01's edit did not change the file; the rejection below would be vacuous"
fi
if [ "$(stat -c%s "$TARGET")" = "$(stat -c%s "$DIR/$REL")" ]; then
    ok "NEG-01 precondition: the damaged artifact is the same SIZE, so only the digest can catch it"
else
    notok "NEG-01 changed the file size; a weaker check than the digest could have caught it"
fi
if verify_copy "$T1" "$WORK/neg01.log"; then
    notok "SIGN-NEG-01: a modified artifact VERIFIED"
else
    ok "SIGN-NEG-01: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg01.log" | head -1 | cut -c1-140)"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-02 — SHA256SUMS is modified"
# ---------------------------------------------------------------------------
T2="$WORK/neg02"; fresh_copy "$T2"
ok "NEG-02 control: the untouched copy verifies"
m_before="$(sha256sum "$T2/SHA256SUMS" | cut -d' ' -f1)"
# Flip one hex digit of the FIRST recorded digest. The manifest stays
# well-formed, so this is caught by the signature rather than by a parse error.
first_line="$(head -1 "$T2/SHA256SUMS")"
first_digest="${first_line%% *}"
flipped="$(printf '%s' "$first_digest" | sed 's/^./0/; s/^00/01/')"
[ "$flipped" != "$first_digest" ] || flipped="$(printf '%s' "$first_digest" | sed 's/^./f/')"
sed -i "1s/$first_digest/$flipped/" "$T2/SHA256SUMS" || die "NEG-02 could not edit the manifest"
m_after="$(sha256sum "$T2/SHA256SUMS" | cut -d' ' -f1)"
if need_ran "NEG-02's edit to SHA256SUMS" "$m_before" "$m_after"; then
    ok "NEG-02 precondition: SHA256SUMS changed ($first_digest -> $flipped on line 1)"
else
    notok "NEG-02 did not change SHA256SUMS; the rejection below would be vacuous"
fi
if [ "$(wc -l <"$T2/SHA256SUMS")" = "$(wc -l <"$DIR/SHA256SUMS")" ]; then
    ok "NEG-02 precondition: the manifest is still well-formed, $(wc -l <"$T2/SHA256SUMS") lines"
else
    notok "NEG-02 malformed the manifest; the signature check is not what would be exercised"
fi
if verify_copy "$T2" "$WORK/neg02.log"; then
    notok "SIGN-NEG-02: a modified SHA256SUMS VERIFIED"
else
    ok "SIGN-NEG-02: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg02.log" | head -1 | cut -c1-140)"
fi
if grep -qF 'signature on SHA256SUMS is not good' "$WORK/neg02.log"; then
    ok "SIGN-NEG-02 was caught by the SIGNATURE, before any digest was trusted"
else
    notok "SIGN-NEG-02 was not caught by the signature; the manifest was trusted before its provenance was settled"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-03 — verification against an unrelated public key"
# ---------------------------------------------------------------------------
T3="$WORK/neg03"; fresh_copy "$T3"
ok "NEG-03 control: the untouched copy verifies against the PRODUCTION key"
ok "NEG-03 precondition: an unrelated key $STRANGER_FPR exists and is not $FPR"
# (a) the stranger's keyring cannot vouch for the signature at all.
if "$VERIFY" --dir "$T3" --keyring "$WORK/stranger-pub.gpg" --fingerprint "$FPR" >"$WORK/neg03a.log" 2>&1; then
    notok "SIGN-NEG-03a: the release VERIFIED against a keyring holding only an unrelated key"
else
    ok "SIGN-NEG-03a: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg03a.log" | head -1 | cut -c1-140)"
fi
# (b) the correct keyring, but the user checks the WRONG fingerprint. This is
#     the substitution case: a valid signature by a key that is not ours.
if "$VERIFY" --dir "$T3" --keyring "$PUBKEY" --fingerprint "$STRANGER_FPR" >"$WORK/neg03b.log" 2>&1; then
    notok "SIGN-NEG-03b: the release VERIFIED against an unrelated expected fingerprint"
else
    ok "SIGN-NEG-03b: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg03b.log" | head -1 | cut -c1-140)"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-04 — SHA256SUMS.asc is removed"
# ---------------------------------------------------------------------------
T4="$WORK/neg04"; fresh_copy "$T4"
ok "NEG-04 control: the untouched copy verifies, so a signature was there to remove"
[ -f "$T4/SHA256SUMS.asc" ] || die "NEG-04 found no signature to remove"
rm -f "$T4/SHA256SUMS.asc"
if [ ! -e "$T4/SHA256SUMS.asc" ]; then
    ok "NEG-04 precondition: SHA256SUMS.asc is gone"
else
    notok "NEG-04 could not remove the signature"
fi
if verify_copy "$T4" "$WORK/neg04.log"; then
    notok "SIGN-NEG-04: an unsigned release VERIFIED"
else
    ok "SIGN-NEG-04: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg04.log" | head -1 | cut -c1-140)"
fi
# The digests still all match. An unsigned release is internally consistent,
# which is exactly why "no signature" must not be a skip.
if ( cd "$T4" && sha256sum -c --quiet SHA256SUMS >/dev/null 2>&1 ); then
    ok "SIGN-NEG-04: the stripped release is internally consistent, so ONLY the missing signature can catch it"
else
    notok "SIGN-NEG-04: the stripped release fails its own digests; the missing signature is not what was measured"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-05 — an artifact is replaced, same filename, different bytes"
# ---------------------------------------------------------------------------
# The realistic substitution: another distribution's package of the SAME name.
# All three DEB targets build 'omnibridge_<V>-1_amd64.deb', and the workflow
# asserts the three differ -- so this swap is a genuinely different binary
# under a filename SHA256SUMS already covers.
T5="$WORK/neg05"; fresh_copy "$T5"
ok "NEG-05 control: the untouched copy verifies"
VICTIM="$(find "$T5/ubuntu2404" -name 'omnibridge_*_amd64.deb' -type f ! -name '*gui*' | sort | head -1)"
DONOR="$(find "$T5/debian13"   -name 'omnibridge_*_amd64.deb' -type f ! -name '*gui*' | sort | head -1)"
[ -n "$VICTIM" ] && [ -n "$DONOR" ] || die "NEG-05 needs a same-named .deb in ubuntu2404 and debian13"
[ "$(basename "$VICTIM")" = "$(basename "$DONOR")" ] \
    || die "NEG-05's two packages are not same-named: $(basename "$VICTIM") vs $(basename "$DONOR")"
ok "NEG-05 precondition: both distributions ship the filename $(basename "$VICTIM")"
v_before="$(sha256sum "$VICTIM" | cut -d' ' -f1)"
d_hash="$(sha256sum "$DONOR" | cut -d' ' -f1)"
if [ "$v_before" != "$d_hash" ]; then
    ok "NEG-05 precondition: the donor is genuinely different bytes ($d_hash != $v_before)"
else
    notok "NEG-05's donor is byte-identical to the victim; the swap would change nothing"
fi
cp -f "$DONOR" "$VICTIM" || die "NEG-05 could not perform the swap"
v_after="$(sha256sum "$VICTIM" | cut -d' ' -f1)"
if need_ran "NEG-05's substitution" "$v_before" "$v_after"; then
    ok "NEG-05 precondition: the artifact was replaced, under its own name"
else
    notok "NEG-05's swap did not change the file; the rejection below would be vacuous"
fi
if [ "$(basename "$VICTIM")" = "$(basename "$DONOR")" ] && [ -f "$VICTIM" ]; then
    ok "NEG-05 precondition: the filename in SHA256SUMS is unchanged, so only the digest can catch it"
fi
if verify_copy "$T5" "$WORK/neg05.log"; then
    notok "SIGN-NEG-05: a substituted artifact VERIFIED"
else
    ok "SIGN-NEG-05: rejected — $(grep -E 'VERIFICATION FAILED' "$WORK/neg05.log" | head -1 | cut -c1-140)"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-06 — no private material anywhere it could be published"
# ---------------------------------------------------------------------------
# A scanner that finds nothing has two explanations, and only one of them is
# good news. So the scanner is FIRST shown to catch a planted key; only then is
# its silence on the real targets worth reporting.
# The one header this shell still needs by name, to prove the canary below is
# a real armoured key before the scan is trusted. The other forms the scanner
# recognises -- plain, ENCRYPTED, OPENSSH, RSA, EC, DSA -- live in the regex in
# scan_tree, which is the only thing that reads them. They were once also shell
# variables here; when scan_tree moved to Python they became dead, and the
# linter said so (SC2034). Deleting them was the fix. A suppression directive
# would have kept four copies of one list, three of which nothing reads and
# none of which anything checks.
PGP_MARK='-----BEGIN PGP PRIVATE KEY BLOCK-----'

# A HEADER IS NOT A KEY
# ---------------------
# Searching for the header alone is wrong in this repository, and wrong in the
# direction that wastes a person's afternoon. Every guard that looks for leaked
# key material must CONTAIN the string it looks for, so
# packaging/release/sign-release.sh, packaging/tests/release-signing-tests.sh,
# this file and .github/workflows/release-artifacts.yml all carry
# `-----BEGIN PGP PRIVATE KEY BLOCK-----` as a grep pattern. A header-only scan
# reports four leaked keys in a tree that has none, and a FAIL that is about
# the test rather than the product sends someone hunting a defect that is not
# there -- the second half of the AGENTS.md rule.
#
# What makes an armoured block a KEY is the base64 payload after the header. So
# a hit requires a long base64 line within six lines of the header. Measured
# both ways below: the guards' pattern literals are not reported, and a real
# exported key is.
scan_tree() {  # prints every path under $1 that holds a real armoured private key
    python3 - "$1" <<'PYEOF'
import os, re, sys
root = sys.argv[1]
hdr = re.compile(r'-----BEGIN (?:PGP |OPENSSH |ENCRYPTED |RSA |EC |DSA )?PRIVATE KEY(?: BLOCK)?-----')
b64 = re.compile(r'^[A-Za-z0-9+/=]{40,}$')
for dirpath, dirnames, filenames in os.walk(root):
    dirnames[:] = [d for d in dirnames if d != '.git']
    for fn in filenames:
        fp = os.path.join(dirpath, fn)
        try:
            if os.path.getsize(fp) > 64 * 1024 * 1024:
                continue
            with open(fp, encoding='utf-8', errors='replace') as fh:
                lines = fh.read().splitlines()
        except (OSError, ValueError):
            continue
        for i, line in enumerate(lines):
            if hdr.search(line) and any(b64.match(x.strip()) for x in lines[i + 1:i + 7]):
                print(fp)
                break
PYEOF
}

# The positive control is a REAL exported private key, not a mock-up. The
# throwaway identity generated for NEG-03 is exported here in full and planted
# in a scratch tree; nothing of the production key is touched. A scanner that
# cannot find an actual armoured secret key has nothing to say about the trees
# it calls clean.
CANARY="$WORK/canary"; mkdir -p "$CANARY/nested"
gpg --homedir "$STRANGER" --batch --pinentry-mode loopback --passphrase '' \
    --armor --export-secret-keys "$STRANGER_FPR" > "$CANARY/nested/planted.asc" 2>/dev/null
if ! grep -qF -- "$PGP_MARK" "$CANARY/nested/planted.asc"; then
    die "the canary is not a real armoured private key, so the positive control below would prove nothing"
fi
ok "NEG-06 precondition: a REAL armoured private key ($(wc -c <"$CANARY/nested/planted.asc") bytes) was planted in a scratch tree"
canary_hits="$(scan_tree "$CANARY")"
if need_nonempty "the canary scan" "$canary_hits" && contains "$canary_hits" "planted.asc"; then
    ok "NEG-06 positive control: the scanner FINDS a real planted private key"
else
    die "the scanner did not find a real key it was handed; its silence on the real targets would mean nothing"
fi
# And the other half of the control: the guards' own pattern literals, which
# are headers with no payload, must NOT be reported. Without this, the scan
# would be useless in exactly this repository.
GUARDFILE="$WORK/guard-shaped"; mkdir -p "$GUARDFILE"
printf "if grep -rlq -- '%s' release; then echo leak; fi\n" "$PGP_MARK" > "$GUARDFILE/guard.sh"
if [ -z "$(scan_tree "$GUARDFILE")" ]; then
    ok "NEG-06 negative control: a guard's own pattern literal is NOT mistaken for a key"
else
    notok "NEG-06: the scanner reports a header with no payload as a key; every tree below would fail spuriously"
fi

scan_and_report() {  # label, tree
    local label="$1" tree="$2" hits n
    hits="$(scan_tree "$tree")"
    n="$(grep -c . <<<"$hits" || true)"
    [ -n "${hits//[[:space:]]/}" ] || n=0
    if [ "$n" -eq 0 ]; then
        ok "SIGN-NEG-06: no private key material in $label"
    else
        notok "SIGN-NEG-06: $n file(s) carrying private key material in $label: $(tr '\n' ' ' <<<"$hits")"
    fi
}

scan_and_report "the release artifact set" "$DIR"
scan_and_report "the artifact set exactly as CI produced it" "$CIDIR"

# Tracked files, read from git rather than from the working tree: an ignored
# file is not published, and a tracked one is.
TRACKED="$WORK/tracked"; mkdir -p "$TRACKED"
( cd "$ROOT" && git archive --format=tar HEAD ) | tar -x -C "$TRACKED" \
    || die "could not materialise the tracked tree; NEG-06 cannot inspect what would be published"
n_tracked="$(find "$TRACKED" -type f | wc -l)"
if [ "${n_tracked:-0}" -gt 100 ]; then
    ok "NEG-06 precondition: the tracked tree materialised, $n_tracked files"
else
    die "the tracked tree holds only ${n_tracked:-0} files; the scan would be vacuous"
fi
scan_and_report "the tracked repository tree at HEAD" "$TRACKED"

# The working tree too -- including what is staged but not yet committed, which
# is what a push would carry.
scan_and_report "the working tree (excluding .git and build output)" "$ROOT/packaging"
scan_and_report "the documentation tree" "$ROOT/docs"

# The source bundle a user downloads, unpacked.
BUNDLE="$(find "$DIR" -maxdepth 1 -name '*.tar.gz' -type f | sort | head -1)"
[ -n "$BUNDLE" ] || die "NEG-06 found no source archive in the real set"
SRC="$WORK/src"; mkdir -p "$SRC"
tar -xzf "$BUNDLE" -C "$SRC" || die "could not unpack $BUNDLE"
n_src="$(find "$SRC" -type f | wc -l)"
if [ "${n_src:-0}" -gt 100 ]; then
    ok "NEG-06 precondition: the source archive unpacked, $n_src files from $(basename "$BUNDLE")"
else
    die "the source archive yielded only ${n_src:-0} files; the scan would be vacuous"
fi
scan_and_report "the published source archive" "$SRC"

# The workflow log. This is where a key would show up if CI had ever held one.
LOGDIR="$WORK/logscan"; mkdir -p "$LOGDIR"; cp "$RUNLOG" "$LOGDIR/run.log"
log_hits="$(scan_tree "$LOGDIR")"
if [ -z "${log_hits//[[:space:]]/}" ]; then
    ok "SIGN-NEG-06: no private key header in the workflow log ($(wc -l <"$RUNLOG") lines)"
else
    notok "SIGN-NEG-06: the workflow log carries armoured private key material"
fi

# ---------------------------------------------------------------------------
section "SIGN-NEG-07 — the production private key never entered CI"
# ---------------------------------------------------------------------------
# THE LOG IS GREPPED AS A FILE, NOT SLURPED INTO A VARIABLE
# ---------------------------------------------------------
# A CI log is megabytes -- 1.4 MB for this run. `need_nonempty` starts with
# `${text//[[:space:]]/}`, and bash's pattern substitution over a string that
# size does not finish in any useful time: MEASURED at over ten minutes with no
# result before it was killed. The primitive is right for the few-KB journal
# captures it was written for and wrong here, so this section greps the file
# instead. That is also the form AGENTS.md endorses -- `grep -q P <<<"$var"`
# **or grep the file** -- and it cannot lose a match to SIGPIPE either.
log_lines="$(wc -l <"$RUNLOG")"
if [ "${log_lines:-0}" -ge 200 ]; then
    ok "NEG-07 precondition: the workflow log is $log_lines lines, so an 'absent' result over it is not vacuous"
else
    die "the workflow log holds only ${log_lines:-0} lines; every 'absent' claim below would be vacuous"
fi
# The anchor. Before claiming anything is missing from this log, prove the log
# is the build of THIS commit -- AGENTS.md's window rule. A log from some other
# run would answer "absent" to every question below, including the ones whose
# true answer is "present".
if grep -qF -- "resolved  : $COMMIT" "$RUNLOG"; then
    ok "NEG-07 anchor: the log is the build that resolved $COMMIT, the commit under test"
else
    die "the log does not record a build of $COMMIT, so nothing can be concluded from what it lacks"
fi
# `##[notice]` is a GitHub Actions RUNTIME marker. The workflow's own source is
# echoed into the log verbatim, so a plain search for the message would match
# the un-run `echo` that produces it; only the rendered notice proves the
# branch was taken.
if grep -qF -- '##[notice]No RELEASE_SIGNING_KEY secret is configured' "$RUNLOG"; then
    ok "SIGN-NEG-07: CI emitted the runtime notice that no signing key was configured"
else
    notok "SIGN-NEG-07: the log carries no rendered 'no signing key' notice; CI may have taken the signing path"
fi
# And the consequence, printed by a later step at runtime rather than echoed.
if grep -qF -- 'SHA256SUMS is NOT signed. Signing needs a private key this' "$RUNLOG"; then
    ok "SIGN-NEG-07: the run's Signing status step reported the release unsigned"
else
    notok "SIGN-NEG-07: the run did not report the release unsigned"
fi
if ! grep -qF -- 'imported a signing key' "$RUNLOG"; then
    ok "SIGN-NEG-07: CI never imported a signing key"
else
    notok "SIGN-NEG-07: the log says CI imported a signing key"
fi
# The production fingerprint itself must not appear: neither the primary nor
# the signing subkey was ever known to CI.
if ! grep -qF -- "$FPR" "$RUNLOG"; then
    ok "SIGN-NEG-07: the production primary fingerprint does not appear in the CI log"
else
    notok "SIGN-NEG-07: the production primary fingerprint appears in the CI log"
fi
# CI shipped no signature. That is the observable consequence of holding no key.
if [ ! -e "$CIDIR/SHA256SUMS.asc" ]; then
    ok "SIGN-NEG-07: the artifact set CI produced carries NO SHA256SUMS.asc"
else
    notok "SIGN-NEG-07: CI produced a SHA256SUMS.asc, so CI signed something"
fi
if [ -f "$CIDIR/SHA256SUMS" ]; then
    ok "NEG-07 precondition: CI did produce SHA256SUMS, so its absence of .asc is a choice and not an empty download"
else
    notok "NEG-07: the CI artifact set has no SHA256SUMS either; the download may be incomplete"
fi

# ---------------------------------------------------------------------------
section "The real evidence set was not modified by this harness"
# ---------------------------------------------------------------------------
REAL_AFTER="$(realsum)"
if [ "$REAL_BEFORE" = "$REAL_AFTER" ]; then
    ok "the real signed set is byte-identical to how it was found ($REAL_AFTER)"
else
    notok "THIS HARNESS MODIFIED THE EVIDENCE: $REAL_BEFORE -> $REAL_AFTER"
fi

printf '\n-----------------------------------------------\n'
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
if [ "$FAIL" -gt 0 ]; then printf '\nFailed:\n'; for g in "${FAILED[@]}"; do printf '  %s\n' "$g"; done; fi
[ "$FAIL" -eq 0 ]
