# Release certification

The wave-level verdicts on whether OmniBridge is ready to be released, as
opposed to whether any one piece of it works.

| Doc | What it is | Verdict |
| --- | --- | --- |
| [v1.0.0 GA Release](V1.0.0-GA-RELEASE.md) | the released version itself — promoted, tagged, built from an immutable commit, signed, published and verified again from the public release | **OMNIBRIDGE V1.0.0 GA: RELEASED WITH EXPLICIT NON-BLOCKING DEBTS** |
| [RC Certification v1](RC-CERTIFICATION-V1.md) | the release candidate itself — one immutable commit, built, signed, verified and installed | **RC V1: CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS** |
| [Release Readiness v1 — final](RELEASE-READINESS-V1-FINAL.md) | the current verdict over the thirteen RC conditions | **READY FOR RC WITH EXPLICIT NON-BLOCKING DEBTS** — 13/13 |
| [Release Signing Closure v1](RELEASE-SIGNING-CLOSURE-V1.md) | the production signing identity, the signed artifact set, and the evidence for both | **D-2: PASS** |
| [Release Readiness v1](RELEASE-READINESS-V1.md) | the original thirteen-condition audit, 2026-09-22 | **BLOCKED — on D-2 alone**, superseded 2026-09-23, kept as the record |

Read them in that order if you want the answer, and in reverse if you want to
see how it was reached.

Behind a row: [lifecycle closure](../linux/RELEASE-LIFECYCLE-CLOSURE-V1.md) and
[peer-gate closure](../linux/RELEASE-PEER-GATES-CLOSURE-V1.md) for the gates,
[security evidence closure](../security/SECURITY-EVIDENCE-CLOSURE-V1.md) for
SEC-LOG-03 and notification privacy, and
[the signing foundation](../../audits/release/RELEASE-SIGNING-FOUNDATION-V1.md)
for the design the closure implements.

**All thirteen conditions now hold.** The thirteenth was signing, and it was
open by decision rather than oversight: the audit of 2026-09-22 refused to
reclassify it while no production key existed, and recorded the wave BLOCKED
for a day rather than write something more comfortable. It closed on
2026-09-23 the only way it could — a real key, a real artifact set from one
immutable commit, independent verification, and negative tests against that
real signed set.

Eleven non-blocking debts are carried into RC, listed explicitly in the final
document's §3, and **through GA unchanged** — see the GA document's §11, which
records why D-12 is not closed by publishing SBOMs once and why D-14 is not
closed by a favourable observation.

**v1.0.0 is released.** `main` carries it at
`67cbf8b169b294b358efc857aa18a305860e14a3`, the `v1.0.0` tag points there, and
<https://github.com/yurisismotto/omnibridge/releases/tag/v1.0.0> publishes the
signed artifact set. What is still **not** authorised: Google Play, any Android
production signing material, and any package repository.
