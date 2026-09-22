# Linux packaging audits

| Doc | What it is | Verdict |
| --- | --- | --- |
| [Readiness audit](PACKAGING-V1-READINESS-AUDIT.md) | audit of the RPM spec, systemd unit, firewall and toolchain against what a real Fedora package needs | defects B1–B3, P1–P7, S1, R6 recorded |
| [Build foundation](PACKAGING-V1-BUILD-FOUNDATION.md) | the implementation that made the RPM build at all, offline, in `mock` | B1, B2, B3, P1 closed; B4 found and closed |
| [systemd user unit](PACKAGING-V1-SYSTEMD-UNIT.md) | the unit moved to `packaging/common/` and measured against a fresh install | **S1 PASS, S2 PASS, S3 PASS**; P2 closed |
| [D-Bus activation](PACKAGING-V1-DBUS-ACTIVATION.md) | the daemon repairing its own desktop activation after an install into a live session | implemented and measured on a live `dbus-broker`; P4's session half closed |
| [Fedora integration](PACKAGING-V1-FEDORA-INTEGRATION.md) | the RPM as a real installed desktop product: subpackage split, desktop metadata, lifecycle macros, firewall, `%doc` trim | P7, P5, R10, Q3 closed; two flaky `%check` tests fixed |
| [Debian and Ubuntu](PACKAGING-V1-DEBIAN-UBUNTU.md) | the first debhelper packaging this project has had, built on all three targets | **build-verified** on Debian 13, Ubuntu 24.04 and 26.04; not runtime-certified |

The readiness audit is the authority the packaging tree cites by section number:
`packaging/fedora/omnibridge.spec`, `packaging/fedora/README.md`,
`packaging/common/README.md` and `packaging/release/make-source-bundle.sh` all
reference it. Keep those references working if either document moves again.

Most of these documents are re-runnable rather than only readable:

| Script | What it re-measures |
| --- | --- |
| `packaging/tests/packaging-checks.sh` | every static claim in the build foundation, the unit report and the integration report, plus the manifests of built packages |
| `packaging/tests/systemd-unit-gates.sh` | gates S1, S2 and S3 against the real unit, on whatever machine it is run on |
| `packaging/tests/install-smoke.sh` | install, remove, reinstall, **purge**, and above all that no transaction touches the user's trust store — `.rpm` and `.deb` alike |
| `packaging/debian/build-deb.sh` | builds the `.deb` packages offline in a disposable container, as a non-root user, and runs lintian |

That matters for Phase 6: the same verdicts have to be reached again on Ubuntu
24.04, Ubuntu 26.04 and Debian 13, and a script can be re-run where a
transcript cannot.
