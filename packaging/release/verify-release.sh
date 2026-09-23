#!/usr/bin/env bash
# verify-release.sh — the command a user runs before installing anything.
#
# It answers two questions in the only order that is useful:
#
#   1. did the OmniBridge maintainer sign this release's manifest?
#   2. are the files beside it the files that manifest describes?
#
# Checking the digests first and the signature afterwards would be checking a
# download against itself. So the signature is checked first, and the manifest
# is only trusted once it is known to be the maintainer's.
#
# WHY A MISSING SIGNATURE IS A FAILURE AND NOT A SKIP
# ---------------------------------------------------
# An attacker who can substitute artifacts can also delete `SHA256SUMS.asc`.
# A verifier that reports "no signature found, checking digests only" and exits
# 0 therefore gives its strongest answer -- "verified" -- in exactly the case
# it is meant to catch. `--allow-unsigned` exists for the pre-RC period when
# there is genuinely no key yet; it is opt-in, it prints a warning that says
# what is not being checked, and it is never the default.

set -uo pipefail

DIR=""; KEYRING=""; EXPECT_FPR="${OMNIBRIDGE_SIGNING_FPR:-}"; ALLOW_UNSIGNED=0
usage() {
    cat >&2 <<USAGE
usage: $0 --dir RELEASE_DIR [--keyring FILE] [--fingerprint FPR] [--allow-unsigned]

  --dir           directory holding the artifacts, SHA256SUMS and SHA256SUMS.asc
  --keyring       a keyring holding ONLY the expected public key; without it,
                  the user's default gpg keyring is used
  --fingerprint   the full fingerprint the signature must carry; defaults to
                  \$OMNIBRIDGE_SIGNING_FPR. Without one, any key the keyring
                  trusts is accepted, which is weaker and is said so.
  --allow-unsigned  proceed when there is no signature at all. Prints what is
                  not being checked. Not the default, and never in CI.
USAGE
    exit 2
}
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) DIR="$2"; shift 2 ;;
        --keyring) KEYRING="$2"; shift 2 ;;
        --fingerprint) EXPECT_FPR="$2"; shift 2 ;;
        --allow-unsigned) ALLOW_UNSIGNED=1; shift ;;
        -h|--help) usage ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done
[ -n "$DIR" ] || usage

die() { printf '\nVERIFICATION FAILED: %s\n' "$*" >&2; exit 3; }
say() { printf 'verify: %s\n' "$*"; }

[ -d "$DIR" ] || die "'$DIR' is not a directory"
MANIFEST="$DIR/SHA256SUMS"
SIG="$DIR/SHA256SUMS.asc"
[ -f "$MANIFEST" ] || die "no SHA256SUMS in '$DIR'; there is nothing to verify against"
[ -s "$MANIFEST" ] || die "SHA256SUMS is empty"
n_entries="$(grep -c . <"$MANIFEST" || true)"
[ "${n_entries:-0}" -ge 1 ] 2>/dev/null || die "SHA256SUMS lists no files"
say "SHA256SUMS lists $n_entries file(s)"

# ---------------------------------------------------------------------------
# 1. the signature
# ---------------------------------------------------------------------------
if [ ! -f "$SIG" ]; then
    if [ "$ALLOW_UNSIGNED" = "1" ]; then
        cat >&2 <<'WARN'

WARNING: this release carries no SHA256SUMS.asc, and --allow-unsigned was
given. The digests below prove the download is internally consistent. They
prove NOTHING about who produced it: anyone who can replace an artifact can
replace SHA256SUMS to match. Do not use this mode to accept a release you
obtained from anywhere but the official source.

WARN
    else
        die "no SHA256SUMS.asc in '$DIR'. An unsigned release is not verified. Pass --allow-unsigned only if you understand that this checks the download against itself."
    fi
else
    command -v gpg >/dev/null 2>&1 || die "gpg is not installed; the signature cannot be checked"
    [ -s "$SIG" ] || die "SHA256SUMS.asc is empty"

    GPG=(gpg --batch --status-fd 3)
    if [ -n "$KEYRING" ]; then
        [ -f "$KEYRING" ] || die "--keyring '$KEYRING' does not exist"
        [ -s "$KEYRING" ] || die "--keyring '$KEYRING' is empty; it holds no key to check against"

        # WHY THIS IMPORTS INTO A PRIVATE KEYRING INSTEAD OF PASSING
        # `--no-default-keyring --keyring FILE`
        # ----------------------------------------------------------
        # Those two options are SILENTLY IGNORED when gpg is configured with
        # `use-keyboxd` -- one line in ~/.gnupg/common.conf, present by default
        # on Fedora 44. gpg prints only
        #
        #     Note: Specified keyrings are ignored due to option "use-keyboxd"
        #
        # on stderr, exits normally, and answers out of the user's OWN keyring
        # instead. Both directions were measured on gpg 2.4.9 before this was
        # changed:
        #
        #   * FALSE PASS -- a release verified `VERIFIED` against a --keyring
        #     that did not contain the signing key at all, because the user's
        #     own keyring did. The narrowing this flag advertises never
        #     happened, and the script said "checking against <file>" while it
        #     was doing nothing of the kind;
        #   * FALSE FAIL -- a genuine release, with the CORRECT keyring, on a
        #     keyboxd host that had not imported the key, was reported as
        #     "This is what a substituted release looks like". That is how a
        #     verifier teaches people to ignore it.
        #
        # There is no way to switch keyboxd back off for one invocation: gpg
        # rejects `--no-use-keyboxd` as an invalid option. A private GNUPGHOME
        # is therefore the only construction that makes "checked against
        # exactly this keyring" a true statement -- and it carries the property
        # release evidence needs anyway: the verifier provably holds no secret
        # key.
        ISOHOME="$(mktemp -d)" || die "could not create a private keyring directory"
        chmod 700 "$ISOHOME"
        trap 'GNUPGHOME="$ISOHOME" gpgconf --kill gpg-agent >/dev/null 2>&1; rm -rf "$ISOHOME"' EXIT INT TERM
        gpg --homedir "$ISOHOME" --batch --quiet --import <"$KEYRING" >/dev/null 2>&1 \
            || die "nothing could be imported from --keyring '$KEYRING'; gpg cannot read it as a keyring"

        # An empty import must not be mistaken for a strict check. A keyring
        # holding no key rejects every signature, which looks identical to
        # catching a bad one and means nothing.
        n_keys="$(gpg --homedir "$ISOHOME" --batch --with-colons --list-keys 2>/dev/null | grep -c '^pub:' || true)"
        [ "${n_keys:-0}" -ge 1 ] \
            || die "--keyring '$KEYRING' yielded no public key. Verifying against an empty keyring rejects everything, which is not the same as checking anything."

        # A verifier must hold no secret. If the keyring carried any, the
        # person running this is about to publish evidence that proves the
        # opposite of what they think.
        n_sec="$(find "$ISOHOME/private-keys-v1.d" -type f -name '*.key' 2>/dev/null | wc -l)"
        [ "${n_sec:-0}" -eq 0 ] \
            || die "--keyring '$KEYRING' carried secret key material; a verifier must hold none"

        GPG+=(--homedir "$ISOHOME")
        say "checking against $n_keys key(s) imported from $KEYRING, in a private keyring holding no secret material"
    fi

    status="$("${GPG[@]}" --verify "$SIG" "$MANIFEST" 3>&1 1>/dev/null 2>/dev/null)" || true
    case "$status" in
        *GOODSIG*) : ;;
        *) die "the signature on SHA256SUMS is not good$( [ -n "$KEYRING" ] && printf ' for %s' "$KEYRING" ). gpg said: $(printf '%s' "$status" | tr '\n' ' ' | head -c 200)" ;;
    esac
    validsig="$(printf '%s\n' "$status" | awk '/VALIDSIG/ {print; exit}')"
    sig_fpr="$(awk '{print $3}'  <<<"$validsig")"
    pri_fpr="$(awk '{print $NF}' <<<"$validsig")"
    [ -n "$sig_fpr" ] || die "the signature verified but carried no fingerprint; refusing to report success"
    if [ -n "$pri_fpr" ] && [ "$pri_fpr" != "$sig_fpr" ]; then
        say "good signature by subkey $sig_fpr of primary key $pri_fpr"
    else
        say "good signature by $sig_fpr"
    fi

    if [ -n "$EXPECT_FPR" ]; then
        # Case-insensitive, and spaces stripped, because a fingerprint is
        # copied from a web page as often as from a terminal.
        #
        # The PRIMARY fingerprint is accepted as well as the signing one. What
        # a project publishes, and what a user is told to check, is the primary
        # key's fingerprint -- but a certify-only master signs nothing, so the
        # signature carries its SUBKEY's fingerprint instead. Comparing only
        # the signing key would tell a user with the correct fingerprint that
        # their release was substituted.
        norm() { printf '%s' "$1" | tr -d '[:space:]' | tr 'a-f' 'A-F'; }
        want="$(norm "$EXPECT_FPR")"
        got="$(norm "$sig_fpr")"
        got_pri="$(norm "${pri_fpr:-}")"
        if [ "$want" = "$got" ] || { [ -n "$got_pri" ] && [ "$want" = "$got_pri" ]; }; then
            if [ "$want" = "$got_pri" ] && [ "$got" != "$got_pri" ]; then
                say "the signing key is subkey $got of the expected primary key $got_pri"
            else
                say "the signing key is the expected one ($got)"
            fi
        else
            die "the signature is by $got (primary ${got_pri:-unknown}), but $want was expected. This is what a substituted release looks like."
        fi
    else
        say "NOTE: no expected fingerprint was given, so any key this keyring trusts would pass. Pass --fingerprint for the stronger check."
    fi
fi

# ---------------------------------------------------------------------------
# 2. the files
# ---------------------------------------------------------------------------
# Only now, with the manifest's provenance settled, are the digests worth
# checking. Every listed file must be present: `sha256sum -c` reports a missing
# file as a failure, and --quiet keeps the output to what went wrong.
missing=0
while read -r _ path; do
    [ -n "$path" ] || continue
    [ -e "$DIR/$path" ] || { printf 'verify: MISSING  %s\n' "$path" >&2; missing=$(( missing + 1 )); }
done <"$MANIFEST"
[ "$missing" -eq 0 ] || die "$missing file(s) named in SHA256SUMS are not present"

( cd "$DIR" && sha256sum -c --quiet SHA256SUMS ) \
    || die "at least one file does not match its digest in SHA256SUMS"
say "all $n_entries file(s) match their digests"

if [ -f "$SIG" ]; then
    printf '\nVERIFIED  %s\n  %s file(s), signed by %s\n' "$DIR" "$n_entries" "${sig_fpr:-unknown}"
else
    printf '\nCHECKED (UNSIGNED)  %s\n  %s file(s) are internally consistent; nothing about their origin was verified\n' "$DIR" "$n_entries"
fi
