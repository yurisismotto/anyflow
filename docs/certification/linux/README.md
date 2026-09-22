# Real-host Linux desktop certification

Tray and shell integration certified on actual desktop sessions, one directory
per desktop environment. Both were run on real hosts; neither was simulated.

| Environment | Doc | Verdict |
| --- | --- | --- |
| GNOME | [GNOME AppIndicator compatibility v1](gnome/GNOME-APPINDICATOR-V1.md) | PASS — no GNOME-specific backend needed, and no product code changed |
| KDE Plasma | [KDE Plasma real host certification v1](kde/KDE-PLASMA-REAL-CERTIFICATION-V1.md) | PASS — Part I tray/shell, Part II the six Android↔KDE capability debts |
| Packaging lifecycle | [Packaging v1 lifecycle certification](PACKAGING-V1-LIFECYCLE-CERTIFICATION.md) | **PARTIAL — 13 of 26 gates certified, 11 BLOCKED, 2 partial.** Not a runtime certification |
| Packaging v1, final | [Packaging v1 final certification](PACKAGING-V1-FINAL-CERTIFICATION.md) | **CERTIFIED WITH EXPLICIT NON-BLOCKING DEBTS** — the packages, not the desktop runtime |

The GNOME document's finding is the reason there is one tray implementation
rather than two: GNOME Shell has no tray of its own, and the extension GNOME
users install owns `org.kde.StatusNotifierWatcher` — the same well-known name,
interface and menu protocol the KDE backend already speaks.

The implementation those certifications exercise is reported in
[`docs/reports/linux/KDE-STATUSNOTIFIER-V1.md`](../../reports/linux/KDE-STATUSNOTIFIER-V1.md).

The lifecycle document is the odd one out and says so in its own title line:
the two above were run on real desktop sessions, and it could not be. Two
environmental blockers stopped it — no root password, and user-session QEMU's
SLIRP networking, which accepts no inbound connection and carries no mDNS
multicast, so a VM built without root can never be discovered by the phone.
Every blocked gate is named with its reason; none was marked as passing on the
strength of a neighbouring measurement.

Read the final certification first if you want one document: it is the
read-mostly audit over everything above and every packaging audit, and its §16
says plainly what is certified, what is not, and which two debts must close
before a public release.
