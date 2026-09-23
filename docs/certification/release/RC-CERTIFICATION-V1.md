# OmniBridge — Release Candidate Certification v1

| Field | Value |
| --- | --- |
| **Branch** | `feature/rc-certification-v1` |
| **RC candidate** | `ed7c17a69cbf55fcbb044249f13f90a8c57e8c10` — `develop` after PR #69 |
| **Previously certified commit** | `14e958d` — the baseline of [Release Readiness v1 final](RELEASE-READINESS-V1-FINAL.md) |
| **Date** | 2026-09-23 |
| **Host** | Fedora 44, gpg 2.4.9 (`use-keyboxd`), podman 5.8.7 |
| **Mode** | **Narrow final-candidate validation.** The Packaging v1, lifecycle, peer-gate, security and physical-Android matrices were **inherited, not re-run** — §2 is the evidence that entitles this document to inherit them |
| **Verdict** | **RC V1: CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS** |

---

## 0. What this document is, and what it deliberately is not

It certifies **one immutable artifact set**, built by CI from one commit,
signed with the production identity, verified from a keyring holding no
secret, and installed from the packages themselves.

It is **not** a re-run of the distribution, lifecycle or Android matrices.
Re-running them would have burned hours to reproduce a known answer, and §2
proves the answer is known: **every product and packaging tree at the
candidate is byte-identical to the commit those matrices certified.**

Inheriting evidence is only honest if the inheritance is *earned*. §2 is that
argument, and it is made at two independent levels — the git object graph and
the built artifacts — because a claim this load-bearing should not rest on one.

---

## 1. The candidate

| Field | Value |
| --- | --- |
| **Commit** | `ed7c17a69cbf55fcbb044249f13f90a8c57e8c10` |
| **Version** | 0.1.0 |
| **Workflow run** | [35880858165](https://github.com/yurisismotto/omnibridge/actions/runs/35880858165) |
| **Trigger** | `workflow_dispatch`, `rev=ed7c17a6…` |
| **Started / finished** | 2026-09-23T15:20:45Z → 15:39:04Z |
| **Jobs** | **7 of 7 success** |
| **Working tree at dispatch** | clean; `git status --porcelain` empty, `git diff --check` clean |

The run's own log carries the line that anchors every later claim about it:

```
requested : ed7c17a69cbf55fcbb044249f13f90a8c57e8c10
resolved  : ed7c17a69cbf55fcbb044249f13f90a8c57e8c10
version   : 0.1.0
```

`--worktree` appears **0 times** in the 11 817-line log. Nothing was built
from a dirty tree.

---

## 2. Nothing that previous certifications measured has changed

This is the section the whole wave turns on.

### 2.1 At the git object level

`git diff --name-status 14e958d..ed7c17a` returns eleven files: the release
workflow, `README.md`, six documents, the **verifier**, and two **test**
harnesses. To turn that from an eyeballed file list into a proof, the tree
objects themselves were compared — a git tree hash is a Merkle root over the
entire subtree, so equality is byte-equality of every file beneath it:

| Tree | `14e958d` → `ed7c17a` | Object |
| --- | :-: | --- |
| `desktop/` — all product code | **IDENTICAL** | `f5cb156220fbf6ca8e027709d48f9a2d1557a258` |
| `android/` | **IDENTICAL** | `25d8c6b87b5f59e4e6a0b7495492609e65178410` |
| `protocol/` | **IDENTICAL** | `620a23520197767c0353b8cfb924f674978070fc` |
| `packaging/fedora/` | **IDENTICAL** | `20f49949da9218bab092bcf2d8bbc14eefee1605` |
| `packaging/debian/` | **IDENTICAL** | `2795f5c708de51f8da794fda49dc05509ea070a1` |
| `packaging/common/` — the systemd unit | **IDENTICAL** | `4423d5b36d64f29bdda58afd1505301930250221` |

`make-source-bundle.sh` and `sign-release.sh` are unchanged too.

**The one change to the builder is inert with respect to artifact content.**
`.github/workflows/release-artifacts.yml` changed in exactly one place: the
post-build *"No signing material may reach the artifacts"* guard, from
`find … | grep -q .` to a captured-output test. That guard **inspects** the
release directory after it is assembled; it produces nothing. The change
removes the SIGPIPE false-green AGENTS.md forbids, so it makes the gate
strictly stricter.

### 2.2 At the artifact level

The git argument would be enough on its own. It was corroborated anyway, by
unpacking the candidate's packages and the previously certified ones and
comparing every payload file:

| Package | Payload files differing vs. the certified set |
| --- | --- |
| `fedora44/omnibridge-gui-…x86_64.rpm` | **none — the payload is byte-identical in full** |
| `fedora44/omnibridge-…x86_64.rpm` | **1** — `usr/share/doc/omnibridge/README.md` |
| `ubuntu2404/omnibridge_…_amd64.deb` | **1** — `usr/share/doc/omnibridge/README.md.gz` |
| `ubuntu2604/omnibridge_…_amd64.deb` | **1** — `usr/share/doc/omnibridge/README.md.gz` |
| `debian13/omnibridge_…_amd64.deb` | **1** — `usr/share/doc/omnibridge/README.md.gz` |

In every case the file manifests are identical and the differing file is the
shipped README. **`usr/bin/omnibridge`, `usr/bin/omnibridged`,
`usr/bin/omnibridge-gui`, `usr/lib/systemd/user/omnibridged.service`, the
hicolor icon, the `.desktop` entry, the D-Bus activation file and the AppStream
metainfo are byte-identical to the artifacts the lifecycle and peer-gate
closures certified.**

Three of the four GUI `.deb`s are byte-identical as whole files, which is why
the core packages' single differing file is legible as what it is: a
documentation change, not a product change.

The SBOMs agree in substance as well as shape:

| SBOM | Format | Components | `rustls` | Dependency graph vs. certified |
| --- | --- | ---: | --- | --- |
| `omnibridge_bin` | CycloneDX 1.3 | 175 | 0.23.45 | **identical** |
| `omnibridged_bin` | CycloneDX 1.3 | 196 | 0.23.45 | **identical** |
| `omnibridge-gui_bin` | CycloneDX 1.3 | 207 | 0.23.45 | **identical** |

### 2.3 What that entitles this document to inherit

| Inherited evidence | Source | Why inheritance is sound |
| --- | --- | --- |
| Packaging v1 — the packages | [final certification](../linux/PACKAGING-V1-FINAL-CERTIFICATION.md) | `packaging/fedora`, `packaging/debian`, `packaging/common` byte-identical |
| 23 / 26 lifecycle gates on Ubuntu 24.04, Ubuntu 26.04, Debian 13 | [lifecycle closure](../linux/RELEASE-LIFECYCLE-CLOSURE-V1.md), [peer-gate closure](../linux/RELEASE-PEER-GATES-CLOSURE-V1.md) | the installed binaries and the unit are byte-identical |
| Physical Android smoke — SM-X620, Android 16 | [peer-gate closure](../linux/RELEASE-PEER-GATES-CLOSURE-V1.md) | `android/` and `desktop/` byte-identical; no protocol change |
| Security certification, SEC-LOG-03, L16 privacy | [security certification](../security/SECURITY-CERTIFICATION-V1.md), [evidence closure](../security/SECURITY-EVIDENCE-CLOSURE-V1.md) | product code byte-identical; `cargo audit` re-measured clean at the candidate — §6 |
| GNOME / KDE tray certification | [gnome](../linux/gnome/GNOME-APPINDICATOR-V1.md), [kde](../linux/kde/KDE-PLASMA-REAL-CERTIFICATION-V1.md) | GUI payload byte-identical |

**No physical Android device was exercised in this wave, and none is claimed.**
The SM-X620 results stand as the peer-gate closure recorded them.

---

## 3. The RC artifact set — 14 artifacts, exact

`SHA256SUMS` carries **14 entries**; the release tree holds **14 artifacts**;
the two agree exactly, with no unlisted extra file and nothing listed missing.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `omnibridge-0.1.0.tar.gz` | 2 804 547 | `9da39844f118087d4ff91672cf7fa74ce5be5ad19a5d1f01a2444dda1fbb9792` |
| `omnibridge-0.1.0-vendor.tar.xz` | 28 435 120 | `c9c9daf20511f3d7eeddf063e521cf8c057574f9edc1ec528ec54823d82038f4` |
| `fedora44/omnibridge-0.1.0-3.fc44.src.rpm` | 31 274 458 | `578e65a45b1dc3ebef3bb1430fa1db6ca42c5f480474a4938ba7f8b7916e5496` |
| `fedora44/omnibridge-0.1.0-3.fc44.x86_64.rpm` | 3 678 692 | `c65c29cc1e6280bf0467b8d56fffef0c935454e5e60921e723884f9ac505bcde` |
| `fedora44/omnibridge-gui-0.1.0-3.fc44.x86_64.rpm` | 692 357 | `35fe87739ace668bdd576722a5e86a6255081298eb761f73fce1a623a4f6dc11` |
| `ubuntu2404/omnibridge_0.1.0-1_amd64.deb` | 4 124 978 | `00e3016b7ff92a6ea74472ed9f7c7319bab7b2699abe3e4b1e03ce1bf49b3774` |
| `ubuntu2404/omnibridge-gui_0.1.0-1_amd64.deb` | 679 586 | `e271efd6606c5aeb0fcc27ea969bbf97b97e7e78a20a3fd05a9173c4d3b1f355` |
| `ubuntu2604/omnibridge_0.1.0-1_amd64.deb` | 4 074 946 | `e184257f68488c37f79570c5646b193ea82ad34e56eb3b3b58353d7588e3f854` |
| `ubuntu2604/omnibridge-gui_0.1.0-1_amd64.deb` | 672 052 | `a39706e00818890191765f8e9e5f423315ccf8463d356eae5860093b5d068be3` |
| `debian13/omnibridge_0.1.0-1_amd64.deb` | 3 760 816 | `4e77d27ec62b97835c55fd73baab870127aa88e973bb13311216159627434f0a` |
| `debian13/omnibridge-gui_0.1.0-1_amd64.deb` | 619 384 | `5a4eddb913375d1cae12442db5dd43bc5accf3c1ecea3e6639a5a45f1c995562` |
| `sbom/omnibridge-0.1.0-omnibridge_bin.cdx.json` | 204 953 | `d70cc12977fd5d04c6ab278e33b987ac720a9450ee8155987026cb3d91e48a86` |
| `sbom/omnibridge-0.1.0-omnibridged_bin.cdx.json` | 233 131 | `cf4112e259f9ef0c3a02db10a22fd09b7644d712338b1b6424dd87aa3b26b520` |
| `sbom/omnibridge-0.1.0-omnibridge-gui_bin.cdx.json` | 253 052 | `96d34638f5fb4028e1b73017ac23579037791e96e2857446e812056e540e1547` |

| Manifest | Bytes | SHA-256 |
| --- | ---: | --- |
| `SHA256SUMS` (14 entries) | — | `7a8432e07572d9fe7dbbcae35e3a1fd4135c2b683b4e9e58d7f4b0454183aa10` |
| `SHA256SUMS.asc` | 228 | `5c8b2ecea09db76523993f02f5c0d4841c80a26f5cf1388db987c8557e43f4e0` |

Digest over the whole signed set (all 16 files), taken before and after the
negative suite ran: `7ab76ffe6ed1f6345e3f23b3237e72432112a7e4dadc8e6881739ba69bbd6a4d`.

### 3.1 SLSA provenance — verified before anything was signed

**15 of 15 verified, 0 failed** (the 14 artifacts and `SHA256SUMS`), and
re-verified **after** signing: unchanged, 15 of 15. The predicate binds the
artifacts to the candidate commit, not merely to the repository:

| Field | Value |
| --- | --- |
| `buildType` | `https://actions.github.io/buildtypes/workflow/v1` |
| `resolvedDependencies[0].digest.gitCommit` | **`ed7c17a69cbf55fcbb044249f13f90a8c57e8c10`** |
| `workflow.path` | `.github/workflows/release-artifacts.yml` |
| `runDetails.metadata.invocationId` | `…/actions/runs/35880858165/attempts/1` |

`SHA256SUMS.asc` is correctly **not** attested: it is produced locally and CI
never saw it. That absence is a property of the custody model, not a gap.

---

## 4. The production signature

Produced **locally**, in the foreground, by the maintainer, with
`packaging/release/sign-release.sh`. Signing order is the script's whole
argument and was unchanged: the manifest is verified against the files
**before** anything is signed, and the signature is verified **after** it is
written.

| Field | Value |
| --- | --- |
| **Requested identity** | `F545DC184E909192C3FB6F6E64963019E731BE07` — the published **primary** |
| **Actual signer** | `E8EDE4706F067739A8D3A8B74C48CB81694FD134` — the sign-only subkey |
| **Signature surface** | `SHA256SUMS.asc`, detached, armoured — the approved v1 surface, unchanged |
| **Packet** | algo 22 (EdDSA), keyid `4C48CB81694FD134`, digest algo 10 (SHA-512), sigclass `0x00` |

Custody was re-measured at the candidate, where the secret would actually live
rather than in a listing that can only be believed so far:

| Keygrip | Belongs to | `~/.gnupg/private-keys-v1.d/<grip>.key` |
| --- | --- | --- |
| `33264D673C141E44F964927D0E44683F7091A853` | primary | **ABSENT** |
| `94BA07D37A587654C5D1AF505ECB31A6AB7C1785` | signing subkey | present |

Colon listing: `pub … cSC` with own capability `c`, `sub … s`. The primary
still **cannot sign data**; GnuPG delegated to the subkey by itself.

---

## 5. Independent verification — public material only

Run with `GNUPGHOME` pointed at a **freshly created home that had never seen
the key**, which is the exact position where the pre-fix verifier gave a false
FAIL on this keyboxd host.

```console
$ GNUPGHOME=<fresh> ./packaging/release/verify-release.sh --dir <rc> \
      --keyring <public-only>.gpg \
      --fingerprint F545DC184E909192C3FB6F6E64963019E731BE07
verify: SHA256SUMS lists 14 file(s)
verify: checking against 1 key(s) imported from <public-only>.gpg,
        in a private keyring holding no secret material
verify: good signature by subkey E8EDE4706F067739A8D3A8B74C48CB81694FD134
        of primary key F545DC184E909192C3FB6F6E64963019E731BE07
verify: all 14 file(s) match their digests

VERIFIED  <rc>
  14 file(s), signed by E8EDE4706F067739A8D3A8B74C48CB81694FD134
```

| Required proof | Result |
| --- | :-: |
| verifier `GNUPGHOME` holds **zero** secret key files | ✅ 0 |
| `gpg --list-secret-keys` in that home | ✅ empty |
| published **primary** fingerprint matches | ✅ |
| signature is by the expected **subkey of that primary** | ✅ |
| `SHA256SUMS.asc` verifies | ✅ |
| `SHA256SUMS` covers the entire expected inventory | ✅ 14/14, no extra, none missing |
| every hash matches | ✅ 14/14 |
| SLSA provenance still verifies | ✅ 15/15 |
| no private key material in the RC | ✅ §6 |

The exported public keyring is 534 bytes and contains no armoured private
block. The verifier imports into a private `GNUPGHOME` of its own — the
host keyring cannot answer for it.

---

## 6. Security and negative checks

### 6.1 Production signing negatives, against this signed set

`packaging/tests/release-signing-production-tests.sh`, run against **copies**
of the real signed RC: **47 passed, 0 failed.** Every case proves its own
precondition before claiming a rejection, and the harness proved it left the
evidence untouched — the set digest is `7ab76ffe…` before and after.

| Test | Result |
| --- | :-: |
| **SIGN-NEG-01** one byte changed, size preserved | rejected |
| **SIGN-NEG-02** `SHA256SUMS` altered, still well-formed | **rejected by the SIGNATURE**, before any digest was trusted |
| **SIGN-NEG-03a/b** unrelated keyring / unrelated expected fingerprint | rejected |
| **SIGN-NEG-04** signature removed, release internally consistent | rejected |
| **SIGN-NEG-05** artifact swapped for another distro's same-named package | rejected |
| **SIGN-NEG-06** no private material anywhere publishable | clean |
| **SIGN-NEG-07** production key never entered CI | clean |

SIGN-NEG-07 is **anchored**: the log is first required to contain
`resolved  : ed7c17a6…`, the line this run wrote about this commit, so an
"absent" answer cannot come from reading the wrong log.

### 6.2 The production key never entered CI

| Evidence | Result |
| --- | :-: |
| repository Actions **secrets** / **variables** / **environments** | **0 / 0 / 0** |
| repository owner type | `User` — no organisation secrets are possible |
| the run emitted the **rendered** `##[notice]No RELEASE_SIGNING_KEY secret is configured` | yes |
| `imported a signing key` anywhere in the log | absent |
| the production primary fingerprint anywhere in the log | absent |
| `SHA256SUMS.asc` in the set CI produced | **absent** |
| `SHA256SUMS` in that set | present — so the absence above is a choice, not a failed download |

### 6.3 No secret material is published

| Target | Result |
| --- | :-: |
| the signed RC artifact set | clean |
| the set exactly as CI produced it | clean |
| the tracked repository tree at HEAD (604 files) | clean |
| the published source archive, unpacked (604 files) | clean |
| the full workflow log (11 817 lines) | clean |
| revocation certificate in the RC set or the repo | **none** |
| backup / key-shaped file in the RC set | **none** |
| GNUPGHOME, gpg-agent state, trustdb, `private-keys-v1.d` tracked | **none** |

The scanner's two controls both hold, so a clean result means something: it
**finds** a real planted 451-byte armoured private key, and it does **not**
fire on a guard's own pattern literal with no payload.

### 6.4 Dependency audit, re-measured at the candidate

`cargo audit --deny warnings` — **exit 0, clean**, 282 dependencies against
1 267 advisories. `rustls 0.23.45` in the lockfile and in all three SBOMs, at
or above the **RUSTSEC-2026-0285** fix.

---

## 7. Installed-artifact smoke

Deliberately narrow: enough to prove the candidate's packages install and
behave, **not** a re-run of the distribution matrix. Every package was first
proved to be in the signed manifest with a matching digest, so what was
installed is provably part of the certified set.

`packaging/tests/install-smoke.sh`, real package managers, throwaway
containers that have never seen OmniBridge:

| Image | Format | Result |
| --- | --- | :-: |
| `registry.fedoraproject.org/fedora:44` | RPM | **25 passed, 0 failed** |
| `docker.io/library/ubuntu:24.04` | DEB | **27 passed, 0 failed** |
| `docker.io/library/debian:trixie` | DEB | **27 passed, 0 failed** |

**79 passed, 0 failed.** Covered on each: install exit 0 with no scriptlet
error; all eight expected files present, including `omnibridge`, `omnibridged`
and `omnibridge-gui`; the D-Bus `Exec` is the absolute installed path and
points at an executable; the installed unit carries the S1/S2 directives and
keeps `ProtectSystem=strict`; the unit is installed **disabled** and installing
started **no daemon**; remove, reinstall and (on DEB) purge all leave the
user's trust store byte- and mode-identical; no package-owned file survives
removal; every file in the user's state is owned by the user.

**What this is not.** A container has no `systemd --user` manager, no session
bus, no compositor and no tray host. It says nothing about autostart at login,
GUI launch, D-Bus cold activation or tray integration — gates L3 and L6–L9.
Those are inherited from the lifecycle and peer-gate closures under §2, on the
strength of the installed binaries and the unit being byte-identical, and are
**not** claimed here.

---

## 8. Automated suite results

| Suite | Passed | Failed | Ignored / N/A |
| --- | ---: | ---: | --- |
| product suite, **inside the RC build** (288 test binaries, 4 container builds) | **4 132** | **0** | 96 ignored |
| `release-signing-production-tests.sh` — against this signed RC | **47** | **0** | — |
| `release-signing-tests.sh` — fixture suite | **54** | **0** | — |
| `packaging-checks.sh` — static, incl. H1/H2/H3 | **101** | **0** | — |
| `harness-selftests.sh` | **40** | **0** | — |
| `install-smoke.sh` × 3 images | **79** | **0** | L13/L17 not exercised — §9 |
| `cargo audit --deny warnings` | clean | — | 282 deps, 1 267 advisories |
| CI release artifact workflow | 7 jobs | 0 | — |
| **Total measured this wave** | **4 453** | **0** | |

Not re-run, and inherited under §2: `lifecycle-gates.sh` and
`lifecycle-peer-gates.sh` on the three guests, and the physical SM-X620 smoke.

---

## 9. Known N/A items

| Item | Classification | Reason |
| --- | :-: | --- |
| **L13 / L17** upgrade and trust-across-upgrade | **N/A — preserved** | no older *published* version exists to upgrade from. Unchanged from the lifecycle closure; not inferred from the uninstall/reinstall result, which is a different gate |
| **L14** clipboard auto-send on Debian 13 | **PARTIAL — inherited** | the product's own contract; Debian 13 does not reach the Xwayland XFIXES fallback. Finding F-2, already evidenced |
| **phone → guest** file transfer | **NOT EXECUTED — inherited** | needs the app's own document picker, i.e. a human. adb cannot hand the app a readable URI |
| Android background clipboard | **not claimed** | Android forbids it |
| physical Android smoke this wave | **not executed** | inherited under §2; no device was exercised and nothing is claimed |

---

## 10. Debt disposition — D-4 … D-14

The eleven debts were assessed at `14e958d` against trees that are
**byte-identical** at the candidate. None of them can have changed by
construction, and none is promoted.

| Debt | Disposition | Basis |
| --- | :-: | --- |
| **D-4** filenames in logs | **REMAINS NON-BLOCKING** | `desktop/` and `android/` byte-identical; the known cross-platform filename-logging evidence is **preserved unchanged**, and the implementation has not moved |
| **D-5** no man pages | **REMAINS NON-BLOCKING** | packaging trees byte-identical; packaging polish |
| **D-6** no AppStream screenshots | **REMAINS NON-BLOCKING** | store presentation, not a release gate |
| **D-7** no `-debuginfo` / `-dbgsym` | **REMAINS NON-BLOCKING** | deliberate |
| **D-8** `cargo deny` not configured | **REMAINS NON-BLOCKING** | no `deny.toml` exists; licence policy still undecided. **Not closed** — this wave ran `cargo audit`, which is a different instrument |
| **D-9** no Debian source package | **REMAINS NON-BLOCKING** | no Debian upload planned for v1 |
| **D-10** x86_64 only | **REMAINS NON-BLOCKING** | stated scope |
| **D-11** external IPv6 unscanned | **REMAINS NON-BLOCKING** | scope |
| **D-12** no SBOM publication path | **REMAINS NON-BLOCKING** | the three SBOMs ship inside the **signed** manifest and are provenance-bound; publishing them as first-class release assets is still undone |
| **D-13** CI debug vs `%check` release | **REMAINS NON-BLOCKING** | build-profile difference, understood |
| **D-14** byte-identical rebuilds unproven | **REMAINS NON-BLOCKING — deliberately not closed** | see below |

**D-14, explicitly.** This wave observed something relevant: two independent CI
runs, from two different commits, produced **byte-identical** bytes for three
of four GUI `.deb`s and for every payload file except the one input that
genuinely changed. That is evidence, and it is recorded. **It is not a
reproducibility guarantee and D-14 is not closed on it.** One observed pair of
runs on one runner image is not the property D-14 names, which is that
*rebuilds are reproducible in general* — that needs a declared build
environment, a rebuild instrument and a policy, none of which exists. Closing a
debt on a favourable coincidence is exactly the kind of green tick over nothing
this project's rules forbid.

**Blocking debts: none. Promoted to blocker: none. Resolved in RC: none.**

---

## 11. Release-candidate invariants

| Invariant | Status |
| --- | :-: |
| artifact source commit immutable | ✅ `ed7c17a6…`, resolved and logged |
| CI build green | ✅ 7 / 7 |
| artifact inventory exact | ✅ 14, manifest and tree agree, no extra |
| SLSA provenance valid | ✅ 15 / 15, bound to the commit |
| `SHA256SUMS` correct | ✅ 14 / 14 |
| production signature valid | ✅ by subkey of the published primary |
| public-only verification valid | ✅ zero secret files in the verifier |
| the exact signed set was tested | ✅ digests checked against the manifest before install |
| packages clean-install on supported distributions | ✅ RPM + DEB, 79 / 0 |
| installed RC behaves | ✅ files, unit, D-Bus, state semantics |
| physical Android peer smoke | **inherited** — §2, §9; not executed this wave |
| release-signing negatives pass | ✅ 47 / 0 |
| no secret signing material published | ✅ §6.3 |
| debt disposition complete | ✅ §10 |
| blocking failures | **none** |

---

## 12. Verdict

```
RC candidate      : ed7c17a69cbf55fcbb044249f13f90a8c57e8c10
artifact set      : 14 artifacts + SHA256SUMS + SHA256SUMS.asc
provenance        : 15/15
signature         : E8EDE4706F067739A8D3A8B74C48CB81694FD134
                    of primary F545DC184E909192C3FB6F6E64963019E731BE07
independent verify: VERIFIED, 14/14, from a keyring holding no secret
measured this wave: 4 453 passed, 0 failed
blocking defects  : none

RC V1: CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS
```

Eleven non-blocking debts, **D-4 to D-14**, are carried forward unchanged and
listed in §10. The qualifier is not a hedge on the gates — every RC gate
passed. It records that eleven things remain outstanding, none of them
silently.

---

## 13. What this does not authorise

This document ends at RC certification. None of the following has happened, and
none is implied:

* `develop` has **not** been merged to `main`;
* **no** `v1.0.0` tag exists, and no RC tag was invented — project policy does
  not require one, and the artifacts are already bound to an immutable commit
  by provenance;
* **no** GitHub Release has been published;
* **no** package repository has been published;
* **no** Play Console release has been started, and **no** Android production
  signing material exists;
* **General Availability is not claimed.**

The signed artifact set certified here is evidence. Publishing it is a
deliberate act that belongs to the release wave that follows.
