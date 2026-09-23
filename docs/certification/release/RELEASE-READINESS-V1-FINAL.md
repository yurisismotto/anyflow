# OmniBridge — Release Readiness v1, final

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-signing-provisioning-v1` |
| **Baseline** | `14e958d` — develop after PR #68 |
| **Date** | 2026-09-23 |
| **Supersedes** | the verdict of [RELEASE-READINESS-V1.md](RELEASE-READINESS-V1.md), not its evidence |
| **Mode** | **Narrow re-evaluation.** No hardware, lifecycle or peer certification was re-run. What was re-measured, and why, is §2 |
| **Verdict** | **RELEASE READINESS V1: READY FOR RC WITH EXPLICIT NON-BLOCKING DEBTS** |

---

## 0. The verdict, and the one thing that changed

**13 / 13 RC conditions satisfied.**

On 2026-09-22 twelve of thirteen held and the thirteenth — signing — did not,
and Release Readiness v1 was recorded **BLOCKED on D-2 alone**. That record
stands; it is not rewritten here, and the day it was blocked is part of the
evidence rather than an embarrassment to be tidied away. The audit was
explicitly told not to reclassify the gate because provisioning was happening
separately, and it did not.

What closed D-2 was not a reclassification. It was the four things foundation
§8.9 demanded, each measured:

1. the production identity provisioned — certify-only primary, sign-only
   subkey, primary secret **absent** from the workstation;
2. a **real** fourteen-artifact set, built by CI from the immutable commit
   `14e958d`, signed locally with it;
3. that signature verified **independently**, against a keyring built from the
   published public key and holding **zero** secret key files;
4. SIGN-NEG-01…07 run against copies of that real signed set — **47 passed,
   0 failed**.

Full evidence:
[RELEASE-SIGNING-CLOSURE-V1.md](RELEASE-SIGNING-CLOSURE-V1.md).

### Why the verdict carries a qualifier

`READY FOR RC` and `READY FOR RC WITH EXPLICIT NON-BLOCKING DEBTS` are both
available, and the second is the accurate one. All thirteen conditions hold —
the qualifier is not a hedge on that. It records that **eleven debts, D-4 to
D-14, are carried into RC**, each classified non-blocking by the audit that
found it and none of them silently. §3 lists them. A bare `READY FOR RC` would
read as "nothing outstanding", and eleven things are outstanding.

---

## 1. The thirteen conditions

| | Condition | Status | Evidence |
| --- | --- | :-: | --- |
| 1 | lifecycle required gates resolved honestly | ✅ | [lifecycle closure](../linux/RELEASE-LIFECYCLE-CLOSURE-V1.md) — 23/26 on all three distributions, 2 N/A with a stated reason, 1 partial on one |
| 2 | peer matrix complete to the support claim | ✅ | [peer-gate closure](../linux/RELEASE-PEER-GATES-CLOSURE-V1.md) |
| 3 | SEC-LOG-03 real evidence | ✅ | [security evidence closure](../security/SECURITY-EVIDENCE-CLOSURE-V1.md) |
| 4 | L16 privacy evidence | ✅ | same |
| 5 | no unresolved High/Critical vulnerability | ✅ | re-measured — §2.1 |
| 6 | **signing foundation implemented, and a release signed** | ✅ | **closed this wave** — closure §1–§5 |
| 7 | release verification works | ✅ | re-measured — §2.2 |
| 8 | immutable-source artifact CI valid | ✅ | re-measured — §2.3 |
| 9 | SLSA provenance valid | ✅ | re-measured — §2.3 |
| 10 | no required harness can PASS on absent measurement | ✅ | re-measured — §2.4 |
| 11 | F-2 classified and resolved | ✅ | fixed in PR #66, not carried |
| 12 | physical Android pairing preserved | ✅ | peer-gate closure, SM-X620 |
| 13 | support documentation matches measured reality | ✅ | re-measured — §2.5 |

---

## 2. What was re-measured, and what was not

The instruction was to re-evaluate narrowly and not to re-run expensive
certification without reason. Conditions 1–4, 11 and 12 rest on hardware,
guests and a physical phone; **nothing in this wave touched the product code**,
so re-running them would have burned hours to reproduce a known answer. The
conditions below were re-measured because this wave changed something that
could plausibly have broken them.

### 2.1 Condition 5 — no unresolved High/Critical vulnerability

Re-measured on **these** bytes rather than inherited. All three SBOMs shipped
in the signed set name `rustls 0.23.45`, at or above the RUSTSEC-2026-0285 fix,
and each SBOM parses as CycloneDX 1.3 with 175 / 196 / 207 components. The SBOMs
are inside the signed manifest, so that claim is covered by the signature.

### 2.2 Condition 7 — release verification works

**This is the condition this wave was most likely to break, and it was broken
when the wave started.** `verify-release.sh` did not verify against the keyring
it claimed to: under `use-keyboxd`, `--no-default-keyring --keyring FILE` is
silently ignored. Measured in both directions on gpg 2.4.9 — a false PASS
against a keyring not holding the signing key, and a false FAIL on a genuine
release. Closure §4.3 has the detail.

It is fixed, and the fix is tested rather than asserted: 54 cases in
`release-signing-tests.sh`, including the two keyboxd regressions with a
positive control ahead of them, and the real signed release verifying from a
`GNUPGHOME` that had never seen the key.

### 2.3 Conditions 8 and 9 — immutable-source CI, and provenance

Re-measured on run
[35815768243](https://github.com/yurisismotto/omnibridge/actions/runs/35815768243):
7 of 7 jobs green, `requested` and `resolved` both
`14e958d6211a3c659eab11dd730c4f5bcda0afab`, `--worktree` not used. Provenance
verified **15 of 15, 0 failed**, before signing and again after, with the
predicate binding `gitCommit 14e958d…` and invocation
`runs/35815768243/attempts/1`.

### 2.4 Condition 10 — no required harness can PASS on absent measurement

Re-measured, and extended. `harness-selftests.sh` 40/0, `packaging-checks.sh`
101/0 including H1 over every harness in the tree.

This wave also found **two harness defects of its own**, both recorded in
closure §7.1 rather than quietly fixed: a key-material scan that mistook a
guard's own grep pattern for a leaked key, and a `need_nonempty` call over a
1.4 MB CI log that did not finish in ten minutes. The first is precisely the
failure this condition exists to catch — a FAIL produced by the harness rather
than the product — and it is now covered by a negative control.

### 2.5 Condition 13 — documentation matches measured reality

Re-measured because this wave changed documentation. Three additions, each
carrying only what was measured:

* `README.md` gained the verification recipe and the **real** primary
  fingerprint, which is what RC-SIGN-03 was waiting for;
* foundation §7, §8.6 and §8.9 gained **dated superseding notes**, with every
  original claim left standing beneath them;
* `RELEASE-READINESS-V1.md` gained a dated closure section, and its BLOCKED
  verdict was left in place.

### 2.6 The product itself did not regress

No product code was changed in this wave. At `14e958d`, inside the four
container builds of run 35815768243, the desktop suite ran to **4 132 passes,
0 failures**.

---

## 3. Debts carried into RC

**Blocking debts: none.** All three from Packaging v1 are closed.

| Debt | Then | Now |
| --- | --- | --- |
| **D-1** eleven lifecycle gates never ran | blocking | **CLOSED** |
| **D-2** artifacts are unsigned | **blocking** | **CLOSED this wave** |
| **D-3** no runtime gate on Ubuntu/Debian | blocking | **CLOSED** |

The eleven below are carried **explicitly**, and are why the verdict is
qualified. None is a blocker and none is new.

| Debt | Non-blocking because |
| --- | --- |
| D-4 filenames in logs | privacy hygiene; has an Android half, so it is a decision about two implementations |
| D-5 no man pages | packaging polish |
| D-6 no AppStream screenshots | store presentation |
| D-7 no `-debuginfo` / `-dbgsym` | deliberate |
| D-8 `cargo deny` not configured | licence policy undecided |
| D-9 no Debian source package | no Debian upload is planned for v1 |
| D-10 x86_64 only | stated scope |
| D-11 external IPv6 unscanned | scope |
| D-12 no SBOM publication path | the SBOMs now ship inside a **signed** manifest; publishing them as a first-class artifact is still undone |
| D-13 CI debug vs `%check` release | build-profile difference, understood |
| D-14 byte-identical rebuilds unproven | an aspiration, not a v1 claim |

---

## 4. What this does not authorise

This document ends at RC **readiness**. It is not permission to release, and
none of the following has happened or is implied:

* `develop` has **not** been merged to `main`;
* **no** `v1.0.0` tag exists;
* **no** GitHub Release has been published;
* **no** Play Console release has been started, and no Android production
  signing material exists;
* RC certification has **not** begun;
* **General Availability is not claimed.**

The signed artifact set in the closure document was built to be signed and
verified. It is evidence, not a release.

---

## 5. Verdict

```
D-2: PASS
13 / 13 RC CONDITIONS SATISFIED
RELEASE READINESS V1: READY FOR RC WITH EXPLICIT NON-BLOCKING DEBTS
```

Eleven non-blocking debts, D-4 to D-14, are carried into RC and listed in §3.
