# Linux desktop sprint reports

| Doc | What it is | Outcome |
| --- | --- | --- |
| [KDE StatusNotifier v1](KDE-STATUSNOTIFIER-V1.md) | the native Plasma tray implementation — `org.kde.StatusNotifierItem` and `com.canonical.dbusmenu` spoken directly to the session bus | implemented; physical KDE certification blocked at the time, closed later |
| [systemd user unit capabilities fix](SYSTEMD-USER-UNIT-CAPABILITIES-FIX.md) | `ProtectKernelModules=` made the user unit unstartable on both Ubuntu LTS targets — `218/CAPABILITIES` before `ExecStart` | fixed; found by the first real execution of lifecycle gate L1 |

## U2 post-certification hardening

[U2](../../audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md) certified the
code as it stood at `9792330` **with its defects present**, and was deliberately
never amended. Each defect it found was fixed on its own branch, and these are
those remediation records.

| Defect | Doc | What was wrong |
| --- | --- | --- |
| P1 | [Multi-peer selection](U2-HARDENING-P1-MULTIPEER.md) | a tablet trusting two desktops routed connections to the wrong one |
| P2 | [Battery absence](U2-HARDENING-P2-BATTERY-ABSENCE.md) | a machine with no battery was advertised as a battery at 0% |
| P3 | [Notification role convergence](U2-HARDENING-P3-NOTIFICATION-ROLE-CONVERGENCE.md) | enabling notification sharing on a live session did nothing until the daemon restarted |
| test/CI | [Test and CI hardening](U2-HARDENING-TEST-CI.md) | the four remaining U2 test and CI defects |

The real-host certifications that exercise this work are in
[`docs/certification/linux/`](../../certification/linux/).
