# Linux packaging audits

| Doc | What it is | Verdict |
| --- | --- | --- |
| [Readiness audit](PACKAGING-V1-READINESS-AUDIT.md) | audit of the RPM spec, systemd unit, firewall and toolchain against what a real Fedora package needs | defects B1–B3, P1–P7, S1, R6 recorded |
| [Build foundation](PACKAGING-V1-BUILD-FOUNDATION.md) | the implementation that made the RPM build at all, offline, in `mock` | B1, B2, B3, P1 closed; B4 found and closed |
| [systemd user unit](PACKAGING-V1-SYSTEMD-UNIT.md) | the unit moved to `packaging/common/` and measured against a fresh install | **S1 PASS, S2 PASS, S3 PASS**; P2 closed |
| [D-Bus activation](PACKAGING-V1-DBUS-ACTIVATION.md) | the daemon repairing its own desktop activation after an install into a live session | implemented and measured on a live `dbus-broker`; P4's session half closed |

The readiness audit is the authority the packaging tree cites by section number:
`packaging/fedora/omnibridge.spec`, `packaging/fedora/README.md`,
`packaging/common/README.md` and `packaging/release/make-source-bundle.sh` all
reference it. Keep those references working if either document moves again.

Two of these documents are re-runnable rather than only readable.
`packaging/tests/packaging-checks.sh` re-asserts the static claims in the build
foundation and the unit report; `packaging/tests/systemd-unit-gates.sh`
re-measures gates S1, S2 and S3 on whatever machine it is run on, which is how
the same verdicts get reached again on Ubuntu and Debian in Phase 6.
