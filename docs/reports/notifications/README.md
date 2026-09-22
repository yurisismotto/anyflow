# `notifications.v1` — sprint reports

Android → Linux notification mirroring, built in numbered waves. Read in order;
each wave's report states the baseline commit it started from and what the
previous wave had not yet done.

| Wave | Doc | What it added | Outcome |
| --- | --- | --- | --- |
| research | [Research report](NOTIFICATIONS-V1-RESEARCH-REPORT.md) | architecture, protocol, privacy model, plan | spec ready — 1 blocker, 3 P0 questions open |
| decision | [Decision report](NOTIFICATIONS-V1-DECISION-REPORT.md) | closed the blocker and the P0 questions; ran both PoCs | implementation ready |
| N0 | [N0](NOTIFICATIONS-V1-N0-REPORT.md) | protocol schema, capability contracts, ADRs, architecture docs | PASS — no runtime yet |
| N1 | [N1](NOTIFICATIONS-V1-N1-REPORT.md) | the Android notification source adapter | PASS — source half only |
| N2 | [N2](NOTIFICATIONS-V1-N2-REPORT.md) | the Linux notification sink | PASS — end to end on hardware |
| N3 | [N3](NOTIFICATIONS-V1-N3-REPORT.md) | consent, privacy and the two applications' UI | PASS, partially executed — USB dropped mid-run, marked as such |
| N4 | [N4](NOTIFICATIONS-V1-N4-REPORT.md) | dismissal synchronisation | PASS |
| N5 | [N5](NOTIFICATIONS-V1-N5-REPORT.md) | hardening, recovery, soak and edge cases | PASS — two lifecycle defects corrected |

The final gate is not here: it is
[`docs/certification/notifications/NOTIFICATIONS-V1-N6-FINAL-CERTIFICATION.md`](../../certification/notifications/NOTIFICATIONS-V1-N6-FINAL-CERTIFICATION.md).

The specification these waves implement is
[`docs/research/notifications-v1/`](../../research/notifications-v1/), and the
decisions are ADR-0015, ADR-0016 and ADR-0017.

N3's verdict is honest about being partially executed. That is deliberate and it
was not later back-filled; N6 re-ran the hardware gates with its own evidence.
