# OmniBridge — Release Readiness v1, artifact signing foundation

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-signing-foundation-v1` |
| **Baseline** | `105b7ba` (merge of PR #63) |
| **Date** | 2026-09-22 |
| **Scope** | D-2 — RC-SIGN-01, RC-SIGN-02, RC-SIGN-03 |
| **Status** | **Everything that can be built without the key is built and tested. One decision is outstanding and it is not this document's to make: §6.** |

---

## 0. What this phase did and did not do

**Did.** Inventoried what a release actually contains and what each part can be
signed with (§2); set out the trust-model choices with their consequences (§5);
implemented the signing path, the verification path, the CI wiring and the
negative tests (§3, §4); and proved the verifier rejects the eight ways a
release can be wrong.

**Did not.** Generate a production key. Choosing where a signing key lives, who
holds it and how it rotates is a commitment that outlives v1, and a key created
to make a checklist go green is worse than no key: it produces signatures that
verify and mean nothing. **§6 is a required stop.**

**Nothing here changes what today's release produces.** Signing activates when
the `RELEASE_SIGNING_KEY` secret exists and is inert until then, and the run
summary says which happened.

---

## 1. Two different questions, and why provenance is not the answer to both

The release already carries SLSA v1 build provenance —
`actions/attest-build-provenance`, Sigstore-backed, GitHub's OIDC identity,
verifiable with `gh attestation verify`. It is real and it stays.

| Question | Answered by | Answered today? |
| --- | --- | --- |
| *Were these bytes built by OmniBridge's CI from commit X?* | SLSA provenance | **yes** |
| *Does the OmniBridge maintainer stand behind this release?* | a maintainer signature | **no** |

A user with a corrupted download needs the first. A user worried about a
compromised repository, a hostile mirror or a substituted release page needs
the second. Provenance is generated **by the same platform that hosts the
artifacts**, so it cannot be the whole answer to "is this repository telling me
the truth?" — an attacker with push access gets provenance for free.

**Treating provenance as a substitute for signing is the specific error this
document exists to prevent.**

---

## 2. Inventory — what is produced, what is signed, what could be

Fourteen artifacts, as CI run
[`35747130364`](https://github.com/yurisismotto/omnibridge/actions/runs/35747130364)
built them.

| # | Artifact | Signed today | A: package-native | B: checksum/release | C: provenance |
| --- | --- | :-: | --- | :-: | :-: |
| 1 | `omnibridge-0.1.0.tar.gz` | no | n/a | ✔ | ✔ |
| 2 | `omnibridge-0.1.0-vendor.tar.xz` | no | n/a | ✔ | ✔ |
| 3 | `fedora44/omnibridge-0.1.0-3.fc44.src.rpm` | no | `rpmsign` header signature | ✔ | ✔ |
| 4–5 | `fedora44/omnibridge{,-gui}-0.1.0-3.fc44.x86_64.rpm` | no | `rpmsign` | ✔ | ✔ |
| 6–7 | `ubuntu2404/omnibridge{,-gui}_0.1.0-1_amd64.deb` | no | `debsigs`/`dpkg-sig` | ✔ | ✔ |
| 8–9 | `ubuntu2604/…` | no | as above | ✔ | ✔ |
| 10–11 | `debian13/…` | no | as above | ✔ | ✔ |
| 12–14 | `sbom/*.cdx.json` × 3 | no | n/a | ✔ | ✔ |
| — | `SHA256SUMS` (14 entries) | **no** | n/a | **the primary user-facing path** | ✔ |

### 2.1 The four kinds, kept apart

**A — package-native signatures.** `rpmsign` writes a signature into the RPM
header; `rpm -K` checks it at install time against a key the user imported with
`rpmkeys --import`. The Debian equivalent, `debsigs`, exists but is **not** how
third-party `.deb`s are normally distributed: Debian's own trust flows through
`Release`/`InRelease` signatures on repository metadata, and `dpkg` does not
verify per-package signatures by default.

These matter when packages are served from a **repository the user configured**.
OmniBridge has no repository. Its artifacts are downloaded from a GitHub release
page and installed with `dnf install ./file.rpm` or `apt install ./file.deb`,
and in that flow a package-native signature is checked only if the user has
already imported the key — which is the same trust decision as verifying a
detached signature, with more steps and less coverage. **Recommended for later,
with a repository; not the v1 path.**

**B — checksum/release signature.** One detached OpenPGP signature over
`SHA256SUMS`, which itself covers all fourteen files. One trust decision, total
coverage, and it protects the tarballs and the SBOMs that have no package-native
option at all. **This is what §3 implements** and it is what upstream projects
that ship tarballs from a release page overwhelmingly do.

**C — provenance/attestation.** In place, unchanged, and §1 says what it is
worth.

**D — repository metadata signatures.** Nothing to sign: there is no
`repomd.xml` and no `Release` file, because there is no repository. It is listed
so that a later decision to publish one is a known, separate piece of work — and
so nobody reads "signing is done" as covering it.

### 2.2 Tooling, measured on this workstation

| Tool | Present | Consequence |
| --- | :-: | --- |
| `gpg` 2.4.9 | **yes** | option B can be produced and verified here today |
| `rpm` / `rpmkeys` 6.0.2 | **yes** | an RPM signature could be *verified* here |
| `rpmsign` | **no** | option A for RPM cannot be produced here without installing it |
| `debsigs`, `dpkg-sig` | **no** | option A for DEB cannot be produced here |
| `cosign` | **no** | keyless/Sigstore signing would need installing |

This is not a detail. **An emergency signing path that cannot run on the
maintainer's own machine is not a path**, and option B is the only one that runs
here unmodified today.

---

## 3. What is implemented

### 3.1 `packaging/release/sign-release.sh`

Produces `SHA256SUMS.asc`, a detached armoured signature. It refuses more than
it signs:

| Refuses when | Because |
| --- | --- |
| there is no `SHA256SUMS`, or it is empty, or it lists nothing | signing it would vouch for nothing |
| **the manifest does not match the files beside it** | signing a manifest that does not describe the release *launders an unverified artifact into a trusted one* — the single worst thing this script could do |
| a `PGP PRIVATE KEY BLOCK` is present under the release directory | the signature would be published together with the key |
| the named key has no usable secret half | a key moved to an absent smartcard lists fine and cannot sign |
| the signature it just wrote does not verify | a signature nobody checked is a file, not a signature |
| that signature is by a different key than requested | a keyring with several keys must not silently pick one |

It never reads, holds or prints a passphrase. That is asserted by the tests
against the *machinery* — no `--passphrase`, no loopback pinentry, no silent
`read` — because the absence of the code is a stronger guarantee than the
absence of the string.

### 3.2 `packaging/release/verify-release.sh`

The command a user runs. It checks the **signature first and the digests
second**: checking digests first would be checking a download against itself.

**A missing signature is a failure, not a skip.** An attacker who can substitute
artifacts can also delete `SHA256SUMS.asc`, so a verifier that reports "no
signature, checking digests only" and exits 0 gives its strongest answer in
exactly the case it exists to catch. `--allow-unsigned` is opt-in for the
pre-RC period, prints what it is *not* checking, and labels its result
`CHECKED (UNSIGNED)` rather than `VERIFIED`.

With `--fingerprint` it requires a specific key, not merely one the keyring
trusts; without it, it says so rather than implying the stronger check.

### 3.3 CI wiring, inert until there is a key

In `release-artifacts.yml`, after `SHA256SUMS` and before the attestation:

| Step | Behaviour |
| --- | --- |
| *Is a signing key configured?* | reads `secrets.RELEASE_SIGNING_KEY`; an **empty** secret counts as absent, so a blank secret cannot take the signing path and fail inside gpg |
| *Sign SHA256SUMS* | only when configured. Imports through a **pipe** — never a file that could outlive a failure, never an argument visible in `ps` — into a `mktemp -d` `GNUPGHOME` removed by a trap; cross-checks the imported key against `vars.RELEASE_SIGNING_FPR`; signs; then verifies **the way a user would**, against an exported public keyring with the fingerprint pinned |
| *No signing material may reach the artifacts* | runs **whether or not anything was signed**: no PGP or OpenSSH private key block, no `*.gpg`/`*.key`/`secring*` anywhere under `release/` before it is uploaded |
| *Signing status* | says plainly which happened |

**Expected secret and variable names**, so the decision in §6 is the only thing
left to do:

| Name | Kind | Contents |
| --- | --- | --- |
| `RELEASE_SIGNING_KEY` | **secret** | the ASCII-armoured private key |
| `RELEASE_SIGNING_KEY_PASSPHRASE` | **secret**, optional | only if the chosen model uses a passphrase; unused by the current path, which expects an unencrypted key held by GitHub's secret store |
| `RELEASE_SIGNING_FPR` | **variable**, not secret | the full fingerprint. A fingerprint is public by design, and having it as a variable is what lets CI notice a secret rotated to the wrong key |

`packaging/tests/release-signing-tests.sh` runs on every packaging pull request
via `packaging-checks.yml`. It needs no secret — it makes its own — so it runs
on forks.

---

## 4. The negative tests, and the test of the tests

`packaging/tests/release-signing-tests.sh` — **28 checks, 0 failures.**

The key is generated per run into a temporary `GNUPGHOME` deleted on exit, and
its identity cannot be mistaken for a release key:

```
OmniBridge TEST KEY -- DO NOT TRUST <test-key@invalid.example>
```

`.invalid` is reserved by RFC 2606 and can never be a real domain. The tests
assert the marker and the reserved domain, and that `GNUPGHOME` is not the
operator's own keyring.

| # | Case | Required |
| --- | --- | --- |
| 0 | **positive control** — untampered, correctly signed | **accepted** |
| 1 | a modified artifact | rejected |
| 2 | a signature by the wrong key | rejected |
| 3 | a signature by a key the keyring does not hold | rejected |
| 4 | **a manifest regenerated over tampered files** — internally consistent, so only the signature can catch it | rejected |
| 5 | a missing signature | rejected |
| 6 | an artifact named in the manifest but absent | rejected |
| 7 | a truncated signature | rejected |
| 8 | an empty signature file — present, so an existence check would pass it | rejected |
| 9 | a good signature by an unexpected fingerprint | rejected |
| 10 | the signer, given a manifest that does not match its files | **refuses, and writes nothing** |
| 11 | the signer, given a release containing a private key | **refuses** |
| 12 | `--allow-unsigned` | proceeds, warns, and labels the result `UNSIGNED` |
| 13 | no private key material in the signed release directory | asserted |
| 14 | no secret key material in any captured script output | asserted against a real line of the key |
| 15 | no key-shaped file committed to the repository | asserted against `git ls-files` |

**The positive control is what stops this file passing on a verifier that
rejects everything**, and the suite was checked against the opposite failure
too: replacing `verify-release.sh` with `exit 0` — a rubber stamp — makes
**11 of the cases fail**. The tests are not vacuous in either direction.

---

## 5. The trust model, laid out for a decision

Four models are realistic. Each is stated with what it costs when things go
wrong, because that is the part that is easy to skip.

### Option 1 — OpenPGP key, private half in GitHub Actions secrets *(recommended)*

The key is generated once on the maintainer's workstation, the private half is
pasted into `RELEASE_SIGNING_KEY`, the public half is published.

| | |
| --- | --- |
| **Signs** | `SHA256SUMS`, in CI, automatically, on every release build |
| **Custody** | GitHub holds the private key. Anyone with repository admin, or a workflow that can read secrets, can sign |
| **Offline/emergency** | works: the maintainer keeps the same key and can run `sign-release.sh` locally with nothing but `gpg` |
| **Rotation** | generate a new key, publish it, update the secret and the variable, keep the old public key so old releases still verify |
| **Revocation** | the revocation certificate is generated **at the same time as the key** and stored separately; publishing it marks the key dead |
| **Cost when it goes wrong** | a repository compromise is a signing-key compromise. This is the real weakness, and it is the same one almost every project of this size accepts |
| **Effort** | lowest. Works with what is already implemented and with the tooling on this machine |

### Option 2 — OpenPGP key that never touches CI

CI produces artifacts; the maintainer downloads them, verifies the provenance,
signs locally and uploads `SHA256SUMS.asc` to the release.

| | |
| --- | --- |
| **Custody** | the maintainer alone, optionally on a hardware token |
| **Cost when it goes wrong** | a repository compromise does **not** yield the key |
| **Cost the rest of the time** | every release needs a human at a machine with the key; no unattended release |
| **Effort** | the same scripts, run by hand. `RELEASE_SIGNING_KEY` is never created |

### Option 3 — Sigstore keyless (`cosign`)

No long-lived key. Signing uses a short-lived certificate bound to an OIDC
identity and logged in a public transparency log.

| | |
| --- | --- |
| **Custody** | nothing to hold, nothing to lose, nothing to rotate |
| **Cost** | verification needs `cosign`, not `gpg`, which most Linux users do not have; and if the identity is GitHub's, it answers the **same** question the existing provenance already answers — §1's distinction collapses |
| **Effort** | new tooling on both sides; `cosign` is absent here |

### Option 4 — hardware token (YubiKey or similar), signing locally

Option 2 with the key on a device that cannot export it.

| | |
| --- | --- |
| **Custody** | strongest available here. Extraction requires the physical token |
| **Cost** | a lost token with no backup means the key is gone; needs a second token or a securely stored backup, decided **before** the key is made |
| **Effort** | hardware, plus `gpg` smartcard setup. The scripts need no change — `sign-release.sh` already rejects a key whose secret half is a stub, which is exactly the "token not plugged in" case |

### 5.1 What is recommended, and why

**Option 1, with the key generated offline and a revocation certificate made at
the same moment.** It is the only model that signs every release without a human
in the loop, it degrades to Option 2 in an emergency because the maintainer
still holds the key, it verifies with `gpg` which users already have, and it
answers the question provenance cannot. Its weakness — repository compromise
implies key compromise — is real and worth stating in the README rather than
engineering around at this size.

**Option 4 is the right upgrade** once releases are less frequent than the
inconvenience of plugging in a token.

---

## 6. SIGNING DECISION REQUIRED

**This is where the phase stops.** Everything above is implemented, tested and
merged-ready. Nothing below can be chosen on the user's behalf.

### 6.1 The questions

| # | Question | Recommendation |
| --- | --- | --- |
| 1 | Which model? | **Option 1** — OpenPGP key, private half in GitHub Actions secrets |
| 2 | Key type and tool? | **Ed25519**, `gpg` 2.4.9, no expiry on the primary, a signing subkey if you want one |
| 3 | Where does the private key live? | generated **offline** on the workstation; the armoured private half pasted into the `RELEASE_SIGNING_KEY` secret; the local copy and the revocation certificate kept in your password manager or on encrypted offline media |
| 4 | May CI sign directly? | **yes**, under Option 1 — that is the point of it |
| 5 | Recovery and rotation? | revocation certificate generated with the key and stored apart; rotation = new key, publish it, update the secret and the variable, keep the old **public** key so past releases keep verifying |
| 6 | What is published, and where? | the fingerprint in `README.md`, the public key as a release asset and at a stable URL |

### 6.2 The exact commands, for the recommended option

Run on the workstation, **not** in CI. Nothing here is run by this phase.

```console
# 1. the key. Ed25519, no expiry, one uid.
$ gpg --quick-generate-key "OmniBridge Release Signing <yuri.sismotto@gmail.com>" ed25519 sign never

# 2. its fingerprint — this is public, and it is what users check against
$ gpg --fingerprint --with-colons "OmniBridge Release Signing" | awk -F: '/^fpr:/ {print $10; exit}'

# 3. the revocation certificate, BEFORE anything is signed, stored apart from
#    the key. Without it a lost key cannot be marked dead.
$ gpg --output omnibridge-release-revoke.asc --gen-revoke <FPR>

# 4. the public half, to publish
$ gpg --armor --export <FPR> > omnibridge-release-pubkey.asc

# 5. the private half, to paste into the GitHub secret. Treat this file the way
#    you treat the key: it IS the key.
$ gpg --armor --export-secret-keys <FPR> > /tmp/omnibridge-signing-private.asc
$ gh secret set RELEASE_SIGNING_KEY < /tmp/omnibridge-signing-private.asc
$ shred -u /tmp/omnibridge-signing-private.asc

# 6. the fingerprint as a repository VARIABLE, so CI notices a wrong key
$ gh variable set RELEASE_SIGNING_FPR --body '<FPR>'

# 7. prove it locally before trusting CI with it
$ ./packaging/release/sign-release.sh   --dir <release-dir> --key <FPR>
$ ./packaging/release/verify-release.sh --dir <release-dir> --fingerprint <FPR>
```

### 6.3 What must be added to GitHub

| Name | Kind | Value |
| --- | --- | --- |
| `RELEASE_SIGNING_KEY` | **secret** | the output of step 5 |
| `RELEASE_SIGNING_FPR` | **variable** | the fingerprint from step 2 |

Nothing else. No token, no password, no third-party account.

### 6.4 What happens after the answer

RC-SIGN-01 closes when the key exists and its public half is published;
RC-SIGN-02 closes on the first CI run with the secret set, which needs no
workflow edit; RC-SIGN-03 closes when `README.md` carries the verification
recipe, which is written but deliberately unpublished until there is a
fingerprint to put in it.

---

## 7. State of the three gates

| Gate | Before | Now | Remaining |
| --- | --- | --- | --- |
| **RC-SIGN-01** a maintainer key exists, public half published | OPEN | **OPEN** | §6 — the decision, then six commands |
| **RC-SIGN-02** CI signs `SHA256SUMS` | OPEN | **IMPLEMENTED, INERT** — wiring, guards and post-conditions in place; activates on the secret | the secret |
| **RC-SIGN-03** documented verification | OPEN | **IMPLEMENTED, UNPUBLISHED** — `verify-release.sh` exists and is tested; the README recipe needs a real fingerprint | §6 |

**No gate is claimed closed.** RC-SIGN-02 and RC-SIGN-03 have had their
implementable halves done and their remainder named, which is the most this
phase can honestly do without inventing a key.
