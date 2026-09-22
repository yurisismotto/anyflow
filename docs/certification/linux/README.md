# Real-host Linux desktop certification

Tray and shell integration certified on actual desktop sessions, one directory
per desktop environment. Both were run on real hosts; neither was simulated.

| Environment | Doc | Verdict |
| --- | --- | --- |
| GNOME | [GNOME AppIndicator compatibility v1](gnome/GNOME-APPINDICATOR-V1.md) | PASS — no GNOME-specific backend needed, and no product code changed |
| KDE Plasma | [KDE Plasma real host certification v1](kde/KDE-PLASMA-REAL-CERTIFICATION-V1.md) | PASS — Part I tray/shell, Part II the six Android↔KDE capability debts |

The GNOME document's finding is the reason there is one tray implementation
rather than two: GNOME Shell has no tray of its own, and the extension GNOME
users install owns `org.kde.StatusNotifierWatcher` — the same well-known name,
interface and menu protocol the KDE backend already speaks.

The implementation those certifications exercise is reported in
[`docs/reports/linux/KDE-STATUSNOTIFIER-V1.md`](../../reports/linux/KDE-STATUSNOTIFIER-V1.md).
