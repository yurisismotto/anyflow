#!/usr/bin/env bash
# release-signing-tests.sh — the negative tests for the signing foundation.
#
# A verifier is only worth having if it says NO. Every case below makes one
# thing wrong and requires `verify-release.sh` to fail; the positive control
# requires it to succeed on an untampered release, so that a verifier which
# rejected everything could not pass this file.
#
# THE TEST KEY
# ------------
# Generated per run, into a temporary GNUPGHOME that is deleted on exit. It is
# never committed, never reused, and its user id says what it is in words that
# cannot be mistaken for a release key:
#
#     OmniBridge TEST KEY -- DO NOT TRUST <test-key@invalid.example>
#
# `.invalid` is reserved by RFC 2606 and can never be a real domain. The key
# has no passphrase, which is correct for an ephemeral key that exists for
# ninety seconds and is the reason the tests need no agent.
#
# Nothing here needs, produces or approaches a production key. That decision is
# the user's and is recorded in docs/audits/release/RELEASE-SIGNING-FOUNDATION-V1.md.

set -uo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$HERE/../.." && pwd)"
SIGN="$ROOT/packaging/release/sign-release.sh"
VERIFY="$ROOT/packaging/release/verify-release.sh"

PASS=0; FAIL=0; declare -a FAILED=()
ok()    { PASS=$(( PASS + 1 )); printf 'ok    %s\n' "$*"; }
notok() { FAIL=$(( FAIL + 1 )); FAILED+=("$*"); printf 'not ok  %s\n' "$*"; }
section(){ printf '\n== %s ==\n' "$*"; }
die()   { printf '\nPRECONDITION FAILED: %s\n' "$*" >&2; exit 3; }

command -v gpg >/dev/null 2>&1 || die "gpg is not installed; these tests cannot run"
[ -x "$SIGN" ]   || die "$SIGN is missing or not executable"
[ -x "$VERIFY" ] || die "$VERIFY is missing or not executable"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/omnibridge-signing.XXXXXXXX")"
export GNUPGHOME="$WORK/gnupg"
mkdir -p "$GNUPGHOME"; chmod 700 "$GNUPGHOME"
cleanup() {
    gpgconf --kill gpg-agent >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT

TEST_UID="OmniBridge TEST KEY -- DO NOT TRUST <test-key@invalid.example>"
WRONG_UID="OmniBridge WRONG TEST KEY -- DO NOT TRUST <wrong-key@invalid.example>"

# ---------------------------------------------------------------------------
section "A test-only keyring"
# ---------------------------------------------------------------------------
gen_key() {
    gpg --batch --quiet --passphrase '' --quick-generate-key "$1" ed25519 sign never >/dev/null 2>&1
}
gen_key "$TEST_UID"  || die "could not generate the test key"
gen_key "$WRONG_UID" || die "could not generate the second (wrong) test key"

FPR="$(gpg --batch --with-colons --list-secret-keys "$TEST_UID"  | awk -F: '/^fpr:/ {print $10; exit}')"
WRONG_FPR="$(gpg --batch --with-colons --list-secret-keys "$WRONG_UID" | awk -F: '/^fpr:/ {print $10; exit}')"
[ -n "$FPR" ] && [ -n "$WRONG_FPR" ] && [ "$FPR" != "$WRONG_FPR" ] \
    || die "the two test keys were not generated distinctly"
ok "two ephemeral test keys exist, $FPR and $WRONG_FPR"

# The user id must be unmistakable. A test key that could be read as a release
# key is the one way this file could do harm.
case "$(gpg --batch --list-keys "$FPR" 2>/dev/null)" in
    *"DO NOT TRUST"*) ok "the test key's user id says DO NOT TRUST in words" ;;
    *) notok "the test key's user id does not carry a DO-NOT-TRUST marker" ;;
esac
case "$TEST_UID" in
    *@invalid.example*) ok "the test key's address is under .invalid, which RFC 2606 reserves" ;;
    *) notok "the test key's address is not under a reserved domain" ;;
esac
[ "$GNUPGHOME" != "${HOME:-}/.gnupg" ] \
    && ok "GNUPGHOME is a temporary directory, not the operator's own keyring" \
    || notok "GNUPGHOME points at the operator's real keyring"

# A keyring holding ONLY the good public key, which is what a user would be
# told to download rather than "whatever your gpg already trusts".
gpg --batch --export "$FPR" > "$WORK/omnibridge-release.gpg" 2>/dev/null
[ -s "$WORK/omnibridge-release.gpg" ] || die "could not export the test public key"
ok "exported a public keyring holding only the expected key"

# ---------------------------------------------------------------------------
section "A release directory shaped like the real one"
# ---------------------------------------------------------------------------
# Same shape as the release job builds: two tarballs at the top, three
# per-distribution directories, an sbom directory, and SHA256SUMS over paths
# rather than bare names because the same filename occurs three times.
mkrelease() {
    local d="$1"
    mkdir -p "$d/fedora44" "$d/ubuntu2404" "$d/ubuntu2604" "$d/debian13" "$d/sbom"
    printf 'source tarball %s\n'  "$RANDOM$RANDOM" > "$d/omnibridge-0.0.0-test.tar.gz"
    printf 'vendor tarball %s\n'  "$RANDOM$RANDOM" > "$d/omnibridge-0.0.0-test-vendor.tar.xz"
    printf 'rpm %s\n'             "$RANDOM$RANDOM" > "$d/fedora44/omnibridge-0.0.0-test.x86_64.rpm"
    printf 'deb u2404 %s\n'       "$RANDOM$RANDOM" > "$d/ubuntu2404/omnibridge_0.0.0-test_amd64.deb"
    printf 'deb u2604 %s\n'       "$RANDOM$RANDOM" > "$d/ubuntu2604/omnibridge_0.0.0-test_amd64.deb"
    printf 'deb d13 %s\n'         "$RANDOM$RANDOM" > "$d/debian13/omnibridge_0.0.0-test_amd64.deb"
    printf '{"sbom":"%s"}\n'      "$RANDOM$RANDOM" > "$d/sbom/omnibridge-0.0.0-test.cdx.json"
    ( cd "$d" && find . -type f ! -name 'SHA256SUMS*' -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS )
}
REL="$WORK/release"
mkrelease "$REL"
n="$(grep -c . <"$REL/SHA256SUMS")"
[ "$n" -eq 7 ] || die "the fixture release has $n manifest entries, expected 7"
ok "a fixture release of $n artifacts exists, with SHA256SUMS over paths"

# ---------------------------------------------------------------------------
section "Positive control — an untampered, correctly signed release verifies"
# ---------------------------------------------------------------------------
# Without this, every negative below would also pass on a verifier that simply
# always failed.
"$SIGN" --dir "$REL" --key "$FPR" >"$WORK/sign.log" 2>&1 \
    && ok "sign-release.sh signed the fixture release" \
    || { notok "sign-release.sh failed on a good release: $(tail -1 "$WORK/sign.log")"; }
[ -s "$REL/SHA256SUMS.asc" ] && ok "SHA256SUMS.asc was written" || notok "no SHA256SUMS.asc was written"

if "$VERIFY" --dir "$REL" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR" >"$WORK/verify.log" 2>&1; then
    ok "POSITIVE CONTROL: verify-release.sh accepts the untampered release"
else
    notok "POSITIVE CONTROL FAILED: a good release did not verify: $(tail -2 "$WORK/verify.log" | tr '\n' ' ')"
fi

# ---------------------------------------------------------------------------
section "Negative tests — each must FAIL verification"
# ---------------------------------------------------------------------------
# refute NAME -- runs the verifier and requires a non-zero exit.
refute() {
    local name="$1"; shift
    if "$@" >"$WORK/neg.log" 2>&1; then
        notok "$name: verification SUCCEEDED and must not have"
    else
        ok "$name: rejected — $(grep -m1 'VERIFICATION FAILED' "$WORK/neg.log" | sed 's/.*VERIFICATION FAILED: //' | tr -s ' ' | head -c 150)"
    fi
}

# 1. a modified artifact
T1="$WORK/t1"; cp -r "$REL" "$T1"
printf 'tampered\n' >> "$T1/ubuntu2404/omnibridge_0.0.0-test_amd64.deb"
refute "a modified artifact" "$VERIFY" --dir "$T1" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 2. the wrong key
T2="$WORK/t2"; cp -r "$REL" "$T2"
rm -f "$T2/SHA256SUMS.asc"
"$SIGN" --dir "$T2" --key "$WRONG_FPR" >/dev/null 2>&1 || die "could not sign with the wrong key"
refute "a signature by the wrong key" "$VERIFY" --dir "$T2" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 2b. the wrong key, against a keyring that does not hold it at all
refute "a signature by a key the keyring does not hold" \
    "$VERIFY" --dir "$T2" --keyring "$WORK/omnibridge-release.gpg"

# 3. a modified checksum manifest — digests edited to match a tampered file,
#    which is exactly what an attacker who can replace files would do
T3="$WORK/t3"; cp -r "$REL" "$T3"
printf 'tampered\n' >> "$T3/fedora44/omnibridge-0.0.0-test.x86_64.rpm"
( cd "$T3" && find . -type f ! -name 'SHA256SUMS*' -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS )
( cd "$T3" && sha256sum -c --quiet SHA256SUMS ) \
    && ok "the tampered release is internally consistent, so only the signature can catch it" \
    || notok "the tampered fixture is not internally consistent; the next check would pass for the wrong reason"
refute "a re-generated manifest over tampered files" \
    "$VERIFY" --dir "$T3" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 4. a missing signature
T4="$WORK/t4"; cp -r "$REL" "$T4"; rm -f "$T4/SHA256SUMS.asc"
refute "a missing signature" "$VERIFY" --dir "$T4" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"
# and the escape hatch is opt-in, prints a warning, and says what it did not check
if out="$("$VERIFY" --dir "$T4" --allow-unsigned 2>&1)"; then
    case "$out" in
        *"CHECKED (UNSIGNED)"*) ok "--allow-unsigned proceeds but reports the result as UNSIGNED" ;;
        *) notok "--allow-unsigned proceeded without labelling the result unsigned" ;;
    esac
    case "$out" in
        *"prove NOTHING about who produced it"*) ok "--allow-unsigned warns what is not being checked" ;;
        *) notok "--allow-unsigned printed no warning" ;;
    esac
else
    notok "--allow-unsigned failed on an unsigned but internally consistent release"
fi

# 5. a missing artifact
T5="$WORK/t5"; cp -r "$REL" "$T5"; rm -f "$T5/sbom/omnibridge-0.0.0-test.cdx.json"
refute "an artifact named in the manifest but absent" \
    "$VERIFY" --dir "$T5" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 6. a truncated signature
T6="$WORK/t6"; cp -r "$REL" "$T6"
head -c 60 "$REL/SHA256SUMS.asc" > "$T6/SHA256SUMS.asc"
refute "a truncated signature" "$VERIFY" --dir "$T6" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 7. an empty signature file — present, so a mere existence check would pass it
T7="$WORK/t7"; cp -r "$REL" "$T7"; : > "$T7/SHA256SUMS.asc"
refute "an empty signature file" "$VERIFY" --dir "$T7" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$FPR"

# 8. the right key, but a fingerprint the user was told to expect and did not get
refute "a good signature by an unexpected fingerprint" \
    "$VERIFY" --dir "$REL" --keyring "$WORK/omnibridge-release.gpg" --fingerprint "$WRONG_FPR"

# ---------------------------------------------------------------------------
section "The signer must also refuse"
# ---------------------------------------------------------------------------
# 9. it must not sign a manifest that does not describe the files beside it
T9="$WORK/t9"; cp -r "$REL" "$T9"; rm -f "$T9/SHA256SUMS.asc"
printf 'tampered\n' >> "$T9/debian13/omnibridge_0.0.0-test_amd64.deb"
if "$SIGN" --dir "$T9" --key "$FPR" >"$WORK/s9.log" 2>&1; then
    notok "sign-release.sh signed a manifest that does not match its files"
else
    ok "sign-release.sh refuses to sign a manifest that does not match its files"
fi
[ -f "$T9/SHA256SUMS.asc" ] \
    && notok "a signature was written despite the refusal" \
    || ok "no signature was written when the signer refused"

# 10. it must not sign a release that would publish private key material
T10="$WORK/t10"; cp -r "$REL" "$T10"; rm -f "$T10/SHA256SUMS.asc"
gpg --batch --pinentry-mode loopback --passphrase '' --export-secret-keys --armor "$FPR" \
    > "$T10/sbom/leaked-secret.asc" 2>/dev/null
[ -s "$T10/sbom/leaked-secret.asc" ] || die "could not stage the leaked-secret fixture"
( cd "$T10" && find . -type f ! -name 'SHA256SUMS*' -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS )
if "$SIGN" --dir "$T10" --key "$FPR" >"$WORK/s10.log" 2>&1; then
    notok "sign-release.sh signed a release containing a PGP PRIVATE KEY BLOCK"
else
    ok "sign-release.sh refuses to sign a release that would publish a private key"
fi

# 11. no secret material may reach the release directory or the logs
section "No signing material leaks"
if grep -rlq -- '-----BEGIN PGP PRIVATE KEY BLOCK-----' "$REL" 2>/dev/null; then
    notok "private key material is present in the signed release directory"
else
    ok "no private key material anywhere in the signed release directory"
fi
secret_sample="$(gpg --batch --pinentry-mode loopback --passphrase '' \
                  --export-secret-keys --armor "$FPR" 2>/dev/null | sed -n '3p')"
[ -n "$secret_sample" ] || die "could not sample the secret key for the leak check"
leaks=0
for log in "$WORK"/*.log; do
    [ -f "$log" ] || continue
    if grep -qF -- "$secret_sample" "$log"; then
        notok "a line of secret key material appears in $(basename "$log")"
        leaks=$(( leaks + 1 ))
    fi
done
[ "$leaks" -eq 0 ] && ok "no secret key material in any script output captured by this run"
# The signer must not print a passphrase either. The strongest form of that
# guarantee is that it never handles one, so the MACHINERY is what is asserted
# absent -- `--passphrase`, a loopback pinentry, a silent `read`, a variable
# holding one. Mentioning the word in the usage text is not only allowed, it is
# how the script tells the operator where the passphrase is supposed to live.
sign_code="$(grep -vE '^\s*#' "$SIGN")"
pp_hits="$(grep -nE -- '--passphrase|--pinentry-mode|read -s|read -rs|PASSPHRASE=|passphrase=' <<<"$sign_code" || true)"
if [ -n "${pp_hits//[[:space:]]/}" ]; then
    notok "sign-release.sh handles passphrase material in code: $(head -1 <<<"$pp_hits" | head -c 100)"
else
    ok "sign-release.sh contains no passphrase-handling code at all; the gpg agent or the secret store holds it"
fi
# Nothing test-only may be committed. The keys live in $WORK and nowhere else.
# A here-string, not a pipe: `git ls-files` is long, `grep -q` exits on its
# first match, and under pipefail the SIGPIPE turns a match into a miss -- the
# defect packaging-checks.sh H1 exists to catch, which caught this line.
tracked="$(git -C "$ROOT" ls-files)"
key_shaped="$(grep -E '\.(gpg|asc|key|pem)$' <<<"$tracked" || true)"
if [ -n "${key_shaped//[[:space:]]/}" ]; then
    notok "a key-shaped file is committed to the repository: $(head -3 <<<"$key_shaped" | tr '\n' ' ')"
else
    ok "no key-shaped file is committed to the repository"
fi

# ---------------------------------------------------------------------------
section "A certify-only master with a signing subkey"
# ---------------------------------------------------------------------------
# The structure RELEASE-SIGNING-FOUNDATION-V1.md recommends, and the one that
# caught a real defect before it shipped: gpg signs with the SUBKEY when asked
# for the master, so a check that compared only the signing key's fingerprint
# rejected the recommended layout, and a --fingerprint check against the
# PUBLISHED primary fingerprint told the user their release was substituted.
#
# What a project publishes is the primary fingerprint. What the signature
# carries is the subkey's. Both must be accepted, and everything else must
# still be refused.
SUB_UID="OmniBridge SUBKEY TEST -- DO NOT TRUST <subkey-test@invalid.example>"
gpg --batch --quiet --passphrase '' --quick-generate-key "$SUB_UID" ed25519 cert never >/dev/null 2>&1     || die "could not generate the certify-only master"
MFPR="$(gpg --batch --with-colons --list-secret-keys "$SUB_UID" | awk -F: '/^fpr:/ {print $10; exit}')"
gpg --batch --quiet --passphrase '' --quick-add-key "$MFPR" ed25519 sign 2y >/dev/null 2>&1     || die "could not add the signing subkey"
SFPR="$(gpg --batch --with-colons --list-keys "$MFPR" | awk -F: '/^sub:/{f=1} /^fpr:/{if(f){print $10; exit}}')"
[ -n "$MFPR" ] && [ -n "$SFPR" ] && [ "$MFPR" != "$SFPR" ]     || die "the master and subkey fingerprints were not produced distinctly"
ok "certify-only master $MFPR with signing subkey $SFPR"

# The master must not itself be able to sign data; that is the whole point.
mcap="$(gpg --batch --with-colons --list-keys "$MFPR" | awk -F: '/^pub:/ {print $12; exit}')"
case "$mcap" in
    *c*) ok "the master's own capability is certify (flags: $mcap)" ;;
    *) notok "the master does not carry a certify capability (flags: $mcap)" ;;
esac
case "$mcap" in
    *s*) notok "the master can sign data itself; it was meant to be certify-only (flags: $mcap)" ;;
    *) ok "the master cannot sign data itself, so a compromised signing subkey does not imply a compromised identity" ;;
esac

# GnuPG 2.1+ writes a revocation certificate at key creation. Recommending
# that file is only honest if it is actually there.
[ -s "$GNUPGHOME/openpgp-revocs.d/$MFPR.rev" ]     && ok "gpg pre-generated a revocation certificate at openpgp-revocs.d/$MFPR.rev ($(wc -c <"$GNUPGHOME/openpgp-revocs.d/$MFPR.rev") bytes)"     || notok "no pre-generated revocation certificate for $MFPR; the documented backup step would point at nothing"

SR="$WORK/subkey-release"
mkrelease "$SR"
gpg --batch --export "$MFPR" > "$WORK/subkey-pub.gpg"

"$SIGN" --dir "$SR" --key "$MFPR" >"$WORK/sign-sub.log" 2>&1     && ok "sign-release.sh signs when given the MASTER fingerprint"     || notok "sign-release.sh failed on a master+subkey key: $(tail -1 "$WORK/sign-sub.log")"
grep -q 'by subkey' "$WORK/sign-sub.log"     && ok "sign-release.sh reports that a subkey made the signature"     || notok "sign-release.sh did not report the subkey relationship"

if "$VERIFY" --dir "$SR" --keyring "$WORK/subkey-pub.gpg" --fingerprint "$MFPR" >"$WORK/v-pri.log" 2>&1; then
    ok "verify-release.sh accepts the PUBLISHED primary fingerprint"
else
    notok "verify-release.sh rejected the primary fingerprint, which is what users are told to check: $(tail -1 "$WORK/v-pri.log")"
fi
if "$VERIFY" --dir "$SR" --keyring "$WORK/subkey-pub.gpg" --fingerprint "$SFPR" >/dev/null 2>&1; then
    ok "verify-release.sh also accepts the signing subkey's own fingerprint"
else
    notok "verify-release.sh rejected the subkey fingerprint that made the signature"
fi
# and it must NOT have become permissive in the process
refute "a master+subkey signature against an unrelated fingerprint"     "$VERIFY" --dir "$SR" --keyring "$WORK/subkey-pub.gpg" --fingerprint "$FPR"
T11="$WORK/t11"; cp -r "$SR" "$T11"
printf 'tampered\n' >> "$T11/ubuntu2404/omnibridge_0.0.0-test_amd64.deb"
refute "a modified artifact under a master+subkey signature"     "$VERIFY" --dir "$T11" --keyring "$WORK/subkey-pub.gpg" --fingerprint "$MFPR"

printf '\n-----------------------------------------------\n'
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
if [ "$FAIL" -gt 0 ]; then printf '\nFailed:\n'; for g in "${FAILED[@]}"; do printf '  %s\n' "$g"; done; fi
[ "$FAIL" -eq 0 ]
