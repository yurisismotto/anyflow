# OmniBridge — Production Release Signing Closure v1

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-signing-provisioning-v1` |
| **Baseline** | `14e958d` — develop after PR #68 |
| **Date** | 2026-09-23 |
| **Scope** | D-2: the four conditions on the production-signing gate, [RELEASE-SIGNING-FOUNDATION-V1.md](../../audits/release/RELEASE-SIGNING-FOUNDATION-V1.md) §8.9 |
| **Verdict** | **D-2: PASS** |

This document records **public** material only: fingerprints, digests, commands
and their output. No passphrase, no secret-key export, no backup content, no
revocation-certificate content and no physical location of any medium appears
here or anywhere in this repository.

---

## 0. The four conditions, and what closed each

The gate was not "a key exists". §8.9 set four conditions and required all of
them, and R5 was told in as many words not to weaken them because provisioning
was happening separately. Each row below points at the section that measured it.

| | Condition | Measured in | |
| --- | --- | --- | :-: |
| 1 | the production signing identity is provisioned | §1 | ✅ |
| 2 | a **real** release artifact set is signed with it | §2, §3 | ✅ |
| 3 | those signatures are verified **independently of the signing step** | §4 | ✅ |
| 4 | the negative verification tests pass against that real signed set | §5 | ✅ |

**D-2: PASS.** No partial pass was available and none was taken.

---

## 1. The identity

### 1.1 Public material

| Field | Value |
| --- | --- |
| **UID** | `OmniBridge Release Signing Key` |
| **Primary fingerprint** | `F545DC184E909192C3FB6F6E64963019E731BE07` |
| **Signing subkey fingerprint** | `E8EDE4706F067739A8D3A8B74C48CB81694FD134` |
| **Primary algorithm / capability** | Ed25519, **certify only** (`C`), **no expiry** |
| **Subkey algorithm / capability** | Ed25519, **sign only** (`S`) |
| **Created** | 2026-09-23 |
| **Subkey expiry** | **2028-09-22** — 63 072 000 s = **730 days** |

The UID carries **no email address**. That is §8.1's decision and it is the
reason the identity survives a change of the maintainer's personal address
without the published fingerprint moving.

```console
$ gpg --homedir <public-only> --list-keys --with-subkey-fingerprints
pub   ed25519 2026-09-23 [C]
      F545DC184E909192C3FB6F6E64963019E731BE07
uid           OmniBridge Release Signing Key
sub   ed25519 2026-09-23 [S] [expires: 2028-09-22]
      E8EDE4706F067739A8D3A8B74C48CB81694FD134
```

The colon listing is what proves the capability split rather than the pretty
one. Field 12 is lowercase for the key's **own** capability and uppercase for
the aggregate over the key and its subkeys:

```
pub:-:255:22:64963019E731BE07:1790134877:::-:::cSC:::::ed25519:::0:
sub:-:255:22:4C48CB81694FD134:1790134877:1853206877:::::s:::::ed25519::
```

`c` on the primary and `s` on the subkey: the primary **cannot sign data**, so
a compromised signing subkey is revoked and replaced without the published
fingerprint changing. `1853206877 − 1790134877 = 63 072 000`, the two years
§8.4 requires.

### 1.2 Custody — measured, not asserted

| Claim | How it was checked | Result |
| --- | --- | :-: |
| primary secret **absent** from the operational workstation | `gpg --list-secret-keys` shows `sec#`, **and** no file exists for the primary's keygrip | ✅ |
| signing subkey secret **present** | `ssb`, **and** its keygrip file exists | ✅ |
| encrypted offline backup exists, **restore-tested** | operator procedure, before the key was used | ✅ |
| revocation certificate exists, on **separate** offline media | operator procedure | ✅ |
| temporary MASTER keyring destroyed, both media unmounted | operator procedure | ✅ |
| the private key **never touched CI** | §6 | ✅ |

```console
$ gpg --list-secret-keys F545DC184E909192C3FB6F6E64963019E731BE07
sec#  ed25519 2026-09-23 [C]
      F545DC184E909192C3FB6F6E64963019E731BE07
uid           OmniBridge Release Signing Key
ssb   ed25519 2026-09-23 [S] [expires: 2028-09-22]
      E8EDE4706F067739A8D3A8B74C48CB81694FD134
```

**`sec#` is a label; the keygrip file is the fact.** A listing can be believed
only so far, so the absence was checked where the secret would actually live:

| Keygrip | Belongs to | `~/.gnupg/private-keys-v1.d/<grip>.key` |
| --- | --- | --- |
| `33264D673C141E44F964927D0E44683F7091A853` | primary | **absent** |
| `94BA07D37A587654C5D1AF505ECB31A6AB7C1785` | signing subkey | present |

This is the custody model §8.3 describes. It is **not** what §8.6's commands
established — they leave the primary's secret in `~/.gnupg` and stop — and that
section now carries a dated superseding note saying so. The gap is also a test:
`release-signing-tests.sh` builds an operational keyring holding only the
signing subkey and requires that adding a UID and changing the primary's expiry
both **fail** from it.

**What is published:** the primary fingerprint and the armoured public key.
**What never is:** everything else.

---

## 2. The artifact set — one immutable commit

| Field | Value |
| --- | --- |
| **Commit** | `14e958d6211a3c659eab11dd730c4f5bcda0afab` |
| **Branch** | `develop`, after PR #68 |
| **Workflow run** | [35815768243](https://github.com/yurisismotto/omnibridge/actions/runs/35815768243) |
| **Trigger** | `workflow_dispatch` with `rev=14e958d…` |
| **Started / finished** | 2026-09-23T03:48:04Z → 04:05:48Z |
| **Jobs** | **7 of 7 success** |
| **Version** | 0.1.0 |

The workflow resolves what it is asked for and refuses anything that is not a
commit SHA. Its own log records the resolution, and that line is what anchors
every later claim about this run:

```
requested : 14e958d6211a3c659eab11dd730c4f5bcda0afab
resolved  : 14e958d6211a3c659eab11dd730c4f5bcda0afab
version   : 0.1.0
```

`--worktree` was **not** used anywhere. Nothing was built from a dirty tree.

### 2.1 Inventory — 14 artifacts, with the digests that were signed

These are the fourteen lines of the `SHA256SUMS` that carries the production
signature. They are reproduced verbatim.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `omnibridge-0.1.0.tar.gz` | 2 778 746 | `ec1e37ffbb7803b49fea303ca8740926d9d87ac94bdf537860e246dfb5c146ad` |
| `omnibridge-0.1.0-vendor.tar.xz` | 28 433 536 | `b3e64028e0d79fd9761fe691f3bb7faadb6ac2d1bc1eb10112857807bf93098a` |
| `fedora44/omnibridge-0.1.0-3.fc44.src.rpm` | 31 246 034 | `807d990cf8b02993cf323f553ca885ad34335803f77388cceac7743040702d51` |
| `fedora44/omnibridge-0.1.0-3.fc44.x86_64.rpm` | 3 677 971 | `07ab9e3dc546b75cac547daa2e6c92f083b991d254a8b01ca990c7827577620d` |
| `fedora44/omnibridge-gui-0.1.0-3.fc44.x86_64.rpm` | 692 357 | `2252cdb45c423605284a76c2c8fd3cedcebba058ee72b0ce83ae839f8eb723fb` |
| `ubuntu2404/omnibridge_0.1.0-1_amd64.deb` | 4 124 128 | `140313e075cfa55c6997c5294cd084c7cfda3d83804b47134933f4dffde6b81d` |
| `ubuntu2404/omnibridge-gui_0.1.0-1_amd64.deb` | 679 586 | `e271efd6606c5aeb0fcc27ea969bbf97b97e7e78a20a3fd05a9173c4d3b1f355` |
| `ubuntu2604/omnibridge_0.1.0-1_amd64.deb` | 4 074 044 | `41fd050c97d6d9440a1875fefc431bf40cf092a1e1ee37ccd96ebf1fe1f5ad40` |
| `ubuntu2604/omnibridge-gui_0.1.0-1_amd64.deb` | 672 052 | `a39706e00818890191765f8e9e5f423315ccf8463d356eae5860093b5d068be3` |
| `debian13/omnibridge_0.1.0-1_amd64.deb` | 3 759 904 | `7093b4f6a0f0f8533b20086e692a3d9c73f215ce53716f4c2fac7e4b1a91f10a` |
| `debian13/omnibridge-gui_0.1.0-1_amd64.deb` | 619 384 | `5a4eddb913375d1cae12442db5dd43bc5accf3c1ecea3e6639a5a45f1c995562` |
| `sbom/omnibridge-0.1.0-omnibridge_bin.cdx.json` | 204 953 | `e8d89074e2867ec9e8696f782af633c925d2164651f4e2dbf0ebfc53f9c2085f` |
| `sbom/omnibridge-0.1.0-omnibridged_bin.cdx.json` | 233 131 | `770988a2923ae96849f3483bb92502b7d390a51501429d1803bae7ac78b00e97` |
| `sbom/omnibridge-0.1.0-omnibridge-gui_bin.cdx.json` | 253 052 | `eb8593718caa71726f072746da9622f89670d9a767c212cd3a1f53a162cecf0a` |

| Manifest | SHA-256 |
| --- | --- |
| `SHA256SUMS` (14 entries) | `7aeff7785964b34ecbb01ac898d9e467f827ba52e6747b38a8b5240b3c5dc9d4` |
| `SHA256SUMS.asc` (228 bytes) | `0b09012503ecc606bd8a30abaa2f0e92b97d95e7a36c487a9887e58683f9b8f7` |

Digest of the whole signed directory, taken before and after the negative
tests ran over it: `cefd823119ee58cbaec2cee8fda13c7a4338c2d8d6197e032f0d484ae6a2e137`.

The three distributions ship **the same filename** — `omnibridge_0.1.0-1_amd64.deb`
— with three different digests. That is asserted by the workflow rather than
hoped for, and §5 turns it into a substitution test.

### 2.2 SLSA provenance — verified BEFORE anything was signed

Provenance was checked first, because a signature over an artifact whose origin
has not been established launders it.

```console
$ gh attestation verify <each artifact> -R yurisismotto/omnibridge
```

**15 of 15 verified, 0 failed** — the fourteen artifacts and `SHA256SUMS`.
Re-run **after** signing, unchanged: 15 of 15. `SHA256SUMS.asc` is produced
locally and is correctly not attested; CI never saw it.

The predicate binds the artifacts to the commit, not merely to the repository:

| Field | Value |
| --- | --- |
| `buildType` | `https://actions.github.io/buildtypes/workflow/v1` |
| `resolvedDependencies[0].digest.gitCommit` | **`14e958d6211a3c659eab11dd730c4f5bcda0afab`** |
| `workflow.path` | `.github/workflows/release-artifacts.yml` |
| `runDetails.metadata.invocationId` | `…/actions/runs/35815768243/attempts/1` |

### 2.3 SBOM evidence stays attached to this set

All three SBOMs are named in the **signed** manifest, so the signature covers
them; each one's provenance names the same commit; and each parses as CycloneDX
and describes the graph that shipped.

| SBOM | Format | Components | `rustls` |
| --- | --- | ---: | --- |
| `omnibridge_bin` | CycloneDX 1.3 | 175 | 0.23.45 |
| `omnibridged_bin` | CycloneDX 1.3 | 196 | 0.23.45 |
| `omnibridge-gui_bin` | CycloneDX 1.3 | 207 | 0.23.45 |

`rustls 0.23.45` is at or above the RUSTSEC-2026-0285 fix, which is the
security certification's claim carried forward onto *these* bytes.

---

## 3. The production signature

Produced **locally**, by `packaging/release/sign-release.sh`, on the maintainer
workstation. The signature surface is the approved v1 one: a single detached
armoured signature over `SHA256SUMS`, which itself covers all fourteen
artifacts. No package-native RPM or DEB signing was invented; §2.1 of the
foundation document says why, and nothing about it changed.

```console
$ ./packaging/release/sign-release.sh --dir <release> \
      --key F545DC184E909192C3FB6F6E64963019E731BE07
sign-release: manifest lists 14 file(s)
sign-release: every listed file is present and matches its digest
sign-release: no private key material is present in the release directory
sign-release: signing with F545DC184E909192C3FB6F6E64963019E731BE07
sign-release: wrote <release>/SHA256SUMS.asc (228 bytes)
sign-release: the signature verifies against SHA256SUMS
sign-release: the signature is by subkey E8EDE4706F067739A8D3A8B74C48CB81694FD134
              of the requested key F545DC184E909192C3FB6F6E64963019E731BE07
SIGNED  <release>/SHA256SUMS.asc
```

The order matters and is the script's whole argument: the manifest is verified
against the files **before** anything is signed, and the signature is verified
**after** it is written. A signature over something the script did not check
would be worse than no signature.

**The requested identity was the PRIMARY fingerprint.** GnuPG resolved that to
the signing subkey by itself — `usando a subchave 4C48CB81694FD134 em vez da
chave principal 64963019E731BE07` — which is exactly the behaviour a
certify-only primary is for, and the reason the verifier had to learn to accept
a published primary fingerprint against a subkey signature.

| Signature packet | Value |
| --- | --- |
| algorithm | 22 (EdDSA) |
| issuer key id | `4C48CB81694FD134` (the signing subkey) |
| digest algorithm | 10 (SHA-512) |
| created | 2026-09-23 |
| class | `0x00`, detached over binary |

### 3.1 CI signing remains off, deliberately

`RELEASE_SIGNING_KEY` was **not** configured, and `RELEASE_SIGNING_FPR` was
**not** added. Neither was created to make a gate go green. The CI signing path
stays implemented and inert; §6 is the evidence that it stayed inert on this
very run.

---

## 4. Independent verification

Verification is independent in the sense that matters: a **separate keyring
built from the published public export**, holding no secret, with the
fingerprint pinned — not a re-run of the signing step's own check.

### 4.1 The verifier holds no secret

| Check | Result |
| --- | :-: |
| secret key files under the verifier's `private-keys-v1.d` | **0** |
| `gpg --list-secret-keys` in that home | empty |
| public keys imported from the published export | 2 (primary + subkey) |

The verifier script asserts this itself and refuses a `--keyring` that carries
secret material, so the property holds for any user who follows the documented
recipe and not only for this run.

### 4.2 The verification

```console
$ ./packaging/release/verify-release.sh --dir <release> \
      --keyring <public-only>.gpg \
      --fingerprint F545DC184E909192C3FB6F6E64963019E731BE07
verify: SHA256SUMS lists 14 file(s)
verify: checking against 1 key(s) imported from <public-only>.gpg,
        in a private keyring holding no secret material
verify: good signature by subkey E8EDE4706F067739A8D3A8B74C48CB81694FD134
        of primary key F545DC184E909192C3FB6F6E64963019E731BE07
verify: the signing key is subkey E8EDE4706F067739A8D3A8B74C48CB81694FD134
        of the expected primary key F545DC184E909192C3FB6F6E64963019E731BE07
verify: all 14 file(s) match their digests

VERIFIED  <release>
  14 file(s), signed by E8EDE4706F067739A8D3A8B74C48CB81694FD134
```

| Required proof | Result |
| --- | :-: |
| verifier `GNUPGHOME` contains no secret key material | ✅ |
| public **primary** fingerprint matches | ✅ |
| detached `SHA256SUMS` signature verifies | ✅ |
| every hash in `SHA256SUMS` verifies | ✅ 14/14 |
| all expected artifacts present | ✅ 14/14 |
| SLSA provenance still verifies | ✅ 15/15 |
| SBOM evidence still attached to this set | ✅ §2.3 |

### 4.3 Why this could not have been done with `--keyring` alone

This workstation runs gpg with `use-keyboxd`, and under it
`--no-default-keyring --keyring FILE` is **silently ignored**: gpg prints a Note
on stderr, exits 0, and answers out of the user's own keyring. Both directions
were measured on gpg 2.4.9 before the verifier was changed —

* **false PASS**: a release reported `VERIFIED` against a keyring that did not
  contain the signing key at all, because the user's own keyring did, while the
  script printed "checking against \<file\>";
* **false FAIL**: a genuine release, with the correct keyring, on a keyboxd host
  that had not imported the key: *"This is what a substituted release looks
  like"*.

There is no per-invocation escape — gpg rejects `--no-use-keyboxd`. The
verifier now imports into a private `GNUPGHOME` and checks there, which is also
what makes §4.1 true. **Verified independently of the host keyring**: the same
verification was re-run with `GNUPGHOME` pointed at an empty keyboxd home that
had never seen the key, and it still reported `VERIFIED` with all 14 digests
matching. The old implementation gave a false FAIL in exactly that position.

---

## 5. Negative tests against the real signed set

`packaging/tests/release-signing-production-tests.sh`, run against **copies**
of the real signed release. **47 passed, 0 failed.**

Existing coverage in `release-signing-tests.sh` is over a fixture with
throwaway keys; it is the right shape for CI and cannot answer condition 4,
which names the real set. This harness is a certification instrument, run by a
person, not a CI job — CI has neither the key nor the artifacts.

**Every case proves its own precondition before claiming a rejection.** A
verifier that rejected everything would pass a suite that skipped that step, so
each case takes a fresh copy, requires that copy to verify *before* it is
damaged, requires the damage to have landed, and only then requires rejection.

| Test | Invalid condition, proved to exist | Result |
| --- | --- | :-: |
| **SIGN-NEG-01** | one byte of `ubuntu2404/omnibridge_0.1.0-1_amd64.deb` changed at offset 512; digest moved `140313e0…` → `3fd71e0b…`, **file size unchanged** so only the digest can catch it | **rejected** |
| **SIGN-NEG-02** | first digest in `SHA256SUMS` altered by one hex character, manifest still 14 well-formed lines | **rejected by the SIGNATURE**, before any digest was trusted |
| **SIGN-NEG-03a** | keyring holding only an unrelated key `F35C0A2E…` | **rejected** |
| **SIGN-NEG-03b** | correct keyring, unrelated **expected fingerprint** — the substitution case | **rejected** |
| **SIGN-NEG-04** | `SHA256SUMS.asc` removed; the remaining release is **internally consistent**, so only the missing signature can catch it | **rejected** |
| **SIGN-NEG-05** | `ubuntu2404/omnibridge_0.1.0-1_amd64.deb` replaced by Debian 13's package of the **same filename, different bytes** | **rejected** |
| **SIGN-NEG-06** | no private material anywhere publishable | **clean**, §5.1 |
| **SIGN-NEG-07** | production key never entered CI | **clean**, §6 |

SIGN-NEG-02's second assertion is the one worth keeping: the failure must come
from the signature, not from a digest mismatch. Checking digests before
provenance would be checking a download against itself.

### 5.1 SIGN-NEG-06 — and the scanner that had to be proved twice

A scan that finds nothing has two explanations and only one is good news. So
the scanner was first shown to **find a real armoured private key** — a
throwaway identity exported in full into a scratch tree; the production key was
never touched.

It also had to be shown *not* to fire on a header with no payload. Every guard
that hunts for leaked keys must contain the string it hunts for, so
`sign-release.sh`, `release-signing-tests.sh`, this harness and the release
workflow all carry `-----BEGIN PGP PRIVATE KEY BLOCK-----` as a grep pattern. A
header-only scan reports four leaked keys in a tree that has none — a FAIL that
is about the test rather than the product, which is the half of the AGENTS.md
rule that is easy to miss. A hit therefore requires a long base64 line within
six lines of the header.

| Control | Expected | Result |
| --- | --- | :-: |
| a **real** exported private key (451 bytes) planted in a scratch tree | found | ✅ |
| a guard's own pattern literal, header with no payload | **not** reported | ✅ |

With both controls holding, the scan is worth reporting:

| Target | Result |
| --- | :-: |
| the signed release artifact set | clean |
| the artifact set exactly as CI produced it | clean |
| the tracked repository tree at `HEAD` (602 files) | clean |
| the working tree under `packaging/` | clean |
| the documentation tree | clean |
| the published source archive, unpacked (601 files) | clean |
| the full GitHub Actions log (11 834 lines) | clean |

---

## 6. The production private key never entered CI

| Evidence | Result |
| --- | :-: |
| repository Actions **secrets** | **0** |
| repository Actions **variables** | **0** |
| repository **environments** | **0** |
| repository owner type | `User` — no organisation secrets are possible |
| `RELEASE_SIGNING_KEY` in the run's step environment | empty |
| the run emitted `##[notice]No RELEASE_SIGNING_KEY secret is configured` | yes |
| the run's *Signing status* step reported the release unsigned | yes |
| `imported a signing key` anywhere in the log | **absent** |
| the production primary fingerprint anywhere in the log | **absent** |
| `SHA256SUMS.asc` in the artifact set CI produced | **absent** |
| `SHA256SUMS` in that set | present — so the absence above is a choice, not a failed download |

**The two absence claims are anchored.** Before concluding anything from what
the log lacks, the log is required to contain `resolved  : 14e958d…` — the line
this very run wrote about this very commit. A log from another run would answer
"absent" to every question, including the ones whose true answer is "present".

`##[notice]` is a runtime marker. The workflow's own source is echoed into the
log verbatim, so searching for the message alone would match the un-run `echo`
that produces it; only the rendered notice proves the branch was taken.

---

## 7. Regression suites

| Suite | Result |
| --- | :-: |
| `packaging/tests/release-signing-tests.sh` | **54 passed, 0 failed** |
| `packaging/tests/release-signing-production-tests.sh` | **47 passed, 0 failed** |
| `packaging/tests/packaging-checks.sh` (static, incl. H1/H2/H3) | **101 passed, 0 failed** |
| `packaging/tests/harness-selftests.sh` | **40 passed, 0 failed** |
| the product suite, in CI at `14e958d`, across four container builds | **4 132 passed, 0 failed** |

The five fixes provisioning produced are each re-proved by name:

| | Fix | Proved by |
| --- | --- | --- |
| 1 | verifier isolation from keyboxd / the default keyring | `verify-release.sh verifies inside a private GNUPGHOME it built from --keyring`; the false-PASS and false-FAIL cases |
| 2 | no `find \| grep -q` false-green key-material guard | packaging-checks H1 over every harness; the CI guard captures output and tests it |
| 3 | certify-only primary + signing subkey support | `sign-release.sh signs by the PRIMARY fingerprint although the primary's secret is absent` |
| 4 | verification by the **published primary** fingerprint | `a subkey-only signature verifies against the published primary fingerprint` |
| 5 | rejection by an unrelated fingerprint | `a master+subkey signature against an unrelated fingerprint: rejected` |

### 7.1 Two defects found in the harnesses while running them

Recorded because the project's own rule is that a harness defect is worth as
much as a product defect, and because both produced a wrong answer.

**A header is not a key.** The first version of `SIGN-NEG-06` searched for the
armoured header alone and reported leaked keys in four files that contain it as
a *grep pattern*. That is a FAIL measured against a precondition the harness
invented. Fixed by requiring a base64 payload, and by adding the negative
control in §5.1 that would catch the regression.

**A megabyte does not fit in `need_nonempty`.** `SIGN-NEG-07` slurped the
1.4 MB CI log into a shell variable and passed it to `need_nonempty`, whose
first act is `${text//[[:space:]]/}`. Bash pattern substitution at that size
does not finish: **measured at over ten minutes with no result** before it was
killed. The primitive is correct for the few-kilobyte journal captures it was
written for. The section now greps the file, which AGENTS.md endorses in the
same breath as the here-string form and which cannot lose a match to SIGPIPE
either.

---

## 8. Gate status

| Gate | Before | Now |
| --- | --- | --- |
| **RC-SIGN-01** key exists, public half published | OPEN | **CLOSED** — §1 |
| **RC-SIGN-02** every release carries a valid `SHA256SUMS.asc`, produced locally | IMPLEMENTED, INERT | **CLOSED** for this set — §3. The equivalence §8.9 defined is the one met: the key never touches CI |
| **RC-SIGN-03** documented verification | IMPLEMENTED, UNPUBLISHED | **CLOSED** — the recipe and the real fingerprint are now in `README.md` |

```
D-2: PASS
```

---

## 9. What this does **not** claim

* **Not a release.** No tag exists, no GitHub Release was published, nothing was
  merged to `main`. This set was built to be signed and verified, and that is
  all it was built for.
* **Not package-native signing.** RPM and DEB header signatures remain
  uninvented and unneeded for v1 — foundation §2.1.
* **Not a substitute for provenance, nor replaced by it.** §1 of the foundation
  document holds: provenance answers *"was this built by OmniBridge's CI from
  commit X?"*, the signature answers *"does the maintainer stand behind it?"*,
  and a green `gh attestation verify` must not be read as a maintainer
  signature.
* **Not a claim about the backup media.** The encrypted backup, its restore
  test and the revocation certificate are recorded here as operator
  attestations. This document did not inspect them, and deliberately holds
  nothing that would help anyone who found them.
