# Linux packaging audits

| Doc | What it is | Verdict |
| --- | --- | --- |
| [Readiness audit](PACKAGING-V1-READINESS-AUDIT.md) | audit of the RPM spec, systemd unit, firewall and toolchain against what a real Fedora package needs | defects B1–B3, P1–P7, S1, R6 recorded |
| [Build foundation](PACKAGING-V1-BUILD-FOUNDATION.md) | the implementation that made the RPM build at all, offline, in `mock` | B1, B2, B3, P1 closed; B4 found and closed |

The readiness audit is the authority the packaging tree cites by section number:
`packaging/fedora/omnibridge.spec`, `packaging/fedora/README.md` and
`packaging/release/make-source-bundle.sh` all reference it. Keep those
references working if either document moves again.
