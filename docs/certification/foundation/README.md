# Wave 0 — core platform abstraction gates

Wave 0 established the cross-platform boundary in `desktop/core`. Its
certification ran in two phases around one CI gate, POC-CORE-04, which had to
prove the workspace builds on Windows MSVC.

| Doc | Date | What it is | Status |
| --- | --- | --- | --- |
| [Local certification closeout](WAVE-0-LOCAL-CERTIFICATION-REPORT.md) | 2026-08-31 | the local gates, G6 and G10 on real hardware | local gates closed; Wave 0 not yet overall certified |
| [POC-CORE-04 Phase 1 readiness](WAVE-0-POC-CORE-04-READINESS.md) | 2026-08-31 | pre-push readiness for the Windows gate — ready to run, not passed | superseded by Phase 2 |
| [POC-CORE-04 Phase 2 final certification](WAVE-0-CERTIFICATION-FINAL.md) | 2026-09-01 | the gate ran and passed on `cfd33f6` | **WAVE 0 CERTIFIED** |

Read them in that order; the Phase 1 document is retained as the pre-push
record and says so in its own header.

The sprint report behind these gates is
[`docs/reports/foundation/wave-0-platform-abstraction.md`](../../reports/foundation/wave-0-platform-abstraction.md),
and the debts closed afterwards are in
[`POST-WAVE0-DEBTS-MICRO-SPRINT.md`](../../reports/foundation/POST-WAVE0-DEBTS-MICRO-SPRINT.md).
