#!/usr/bin/env bash
# sign-release.sh — produce a detached OpenPGP signature over a release's
# `SHA256SUMS`.
#
# WHAT THIS SIGNS, AND WHY THAT IS THE RIGHT THING TO SIGN
# --------------------------------------------------------
# One signature over `SHA256SUMS`, which itself covers every artifact. A user
# who verifies the signature and then `sha256sum -c`s the manifest has checked
# all fourteen files with one trust decision, and that is the workflow the
# README documents.
#
# It is deliberately NOT a package-native signature. `rpmsign` and `debsigs`
# sign a package so that `rpm`/`dpkg` can check it at install time, which
# matters when packages are served from a repository the user has configured.
# OmniBridge has no repository yet; its artifacts are downloaded from a GitHub
# release page. The detached signature over the manifest is the path that
# actually protects that download. Package-native signing is inventoried in
# docs/audits/release/RELEASE-SIGNING-FOUNDATION-V1.md §2 and is not done here.
#
# WHAT A SIGNATURE IS NOT
# -----------------------
# The release already carries SLSA v1 build provenance, Sigstore-backed, bound
# to the workflow and the commit. That answers "was this built by OmniBridge's
# CI from commit X?". This signature answers a different question — "does the
# OmniBridge maintainer stand behind this release?" — and neither substitutes
# for the other. Do not let a green `gh attestation verify` be read as a
# maintainer signature.
#
# THE RULE
# --------
# A signature produced over something this script did not check is worse than
# no signature: it launders an unverified artifact into a trusted one. So the
# manifest is verified against the files BEFORE anything is signed, and the
# signature is verified after it is written. A precondition that is absent
# fails loudly.

set -uo pipefail

DIR=""; KEY="${OMNIBRIDGE_SIGNING_KEY:-}"; OUT=""
usage() {
    cat >&2 <<USAGE
usage: $0 --dir RELEASE_DIR [--key KEYID_OR_FINGERPRINT] [--output FILE]

  --dir     directory holding the artifacts and their SHA256SUMS
  --key     the signing key; defaults to \$OMNIBRIDGE_SIGNING_KEY
  --output  signature path; defaults to <dir>/SHA256SUMS.asc

Environment:
  OMNIBRIDGE_SIGNING_KEY   key id or fingerprint to sign with
  GNUPGHOME                the keyring to use, as usual for gpg

This script never reads, prints or logs a passphrase. Supply one through the
gpg agent, or use a key with no passphrase held in a secret store.
USAGE
    exit 2
}
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) DIR="$2"; shift 2 ;;
        --key) KEY="$2"; shift 2 ;;
        --output) OUT="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done
[ -n "$DIR" ] || usage
OUT="${OUT:-$DIR/SHA256SUMS.asc}"

die() { printf 'sign-release: FATAL: %s\n' "$*" >&2; exit 3; }
say() { printf 'sign-release: %s\n' "$*"; }

command -v gpg >/dev/null 2>&1 || die "gpg is not installed; nothing can be signed"
[ -d "$DIR" ] || die "'$DIR' is not a directory"
MANIFEST="$DIR/SHA256SUMS"
[ -f "$MANIFEST" ] || die "no SHA256SUMS in '$DIR'; there is no manifest to sign"
[ -s "$MANIFEST" ] || die "SHA256SUMS in '$DIR' is empty; signing it would vouch for nothing"

n_entries="$(grep -c . <"$MANIFEST" || true)"
[ "${n_entries:-0}" -ge 1 ] 2>/dev/null || die "SHA256SUMS lists no files"
say "manifest lists $n_entries file(s)"

# Every listed file must exist and match. Signing a manifest whose digests do
# not describe the files beside it is the one thing this script must never do.
( cd "$DIR" && sha256sum -c --quiet SHA256SUMS ) \
    || die "SHA256SUMS does not verify against the files in '$DIR'; refusing to sign"
say "every listed file is present and matches its digest"

# No private key material may be sitting in the directory about to be released.
if grep -rlq -- '-----BEGIN PGP PRIVATE KEY BLOCK-----' "$DIR" 2>/dev/null; then
    die "a PGP PRIVATE KEY BLOCK is present under '$DIR'; refusing to sign a release that would publish it"
fi
say "no private key material is present in the release directory"

[ -n "$KEY" ] || die "no signing key given (--key or \$OMNIBRIDGE_SIGNING_KEY)"

# The key must exist AND be usable for signing. `gpg --list-secret-keys`
# succeeding is not enough: a key whose secret half is a stub (moved to a
# smartcard that is not present) lists fine and cannot sign.
gpg --batch --list-secret-keys "$KEY" >/dev/null 2>&1 \
    || die "no secret key matching '$KEY' in this keyring (GNUPGHOME=${GNUPGHOME:-\$HOME/.gnupg})"
fpr="$(gpg --batch --with-colons --list-secret-keys "$KEY" 2>/dev/null | awk -F: '/^fpr:/ {print $10; exit}')"
[ -n "$fpr" ] || die "could not read a fingerprint for '$KEY'"
say "signing with $fpr"

rm -f "$OUT"
gpg --batch --yes --armor --detach-sign --local-user "$KEY" --output "$OUT" "$MANIFEST" \
    || die "gpg failed to sign SHA256SUMS"
[ -s "$OUT" ] || die "gpg exited 0 but '$OUT' is empty or missing"
say "wrote $OUT ($(wc -c <"$OUT") bytes)"

# Verify what was just produced, with the same keyring. A signature nobody has
# checked is a file, not a signature.
gpg --batch --verify "$OUT" "$MANIFEST" >/dev/null 2>&1 \
    || die "the signature just written does not verify"
say "the signature verifies against SHA256SUMS"

# And it must be THIS key's signature, not merely a valid one from whatever
# else the keyring happens to hold.
sig_fpr="$(gpg --batch --status-fd 1 --verify "$OUT" "$MANIFEST" 2>/dev/null \
           | awk '/^\[GNUPG:\] VALIDSIG/ {print $3; exit}')"
[ "$sig_fpr" = "$fpr" ] \
    || die "the signature is by '$sig_fpr', not by the requested key '$fpr'"
say "the signature is by the requested key"

printf '\nSIGNED  %s\n  key   %s\n  over  %s (%s entries)\n' "$OUT" "$fpr" "$MANIFEST" "$n_entries"
