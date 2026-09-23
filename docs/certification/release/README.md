# Release certification

The wave-level verdicts on whether OmniBridge is ready to be released, as
opposed to whether any one piece of it works.

| Doc | What it is | Verdict |
| --- | --- | --- |
| [Release Readiness v1](RELEASE-READINESS-V1.md) | the thirteen RC conditions, each measured | **BLOCKED — on D-2 alone: artifacts are unsigned** |

Read it after the two documents it rests on, if you want the detail behind a
row: [lifecycle closure](../linux/RELEASE-LIFECYCLE-CLOSURE-V1.md) and
[peer-gate closure](../linux/RELEASE-PEER-GATES-CLOSURE-V1.md) for the gates,
[security evidence closure](../security/SECURITY-EVIDENCE-CLOSURE-V1.md) for
SEC-LOG-03 and notification privacy, and
[the signing foundation](../../audits/release/RELEASE-SIGNING-FOUNDATION-V1.md)
for the one thing still open.

**Twelve of the thirteen conditions hold.** The thirteenth is signing, and it
is open by decision rather than by oversight: every part of the path that can
exist without a production key exists and is tested, and the gate stays shut
until a real release is signed and independently verified.
