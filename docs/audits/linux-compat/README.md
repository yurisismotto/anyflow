# Ubuntu / Debian compatibility — the U series

Three documents, in order. Together they are the record of making the Linux
desktop stack build and run on stock Ubuntu LTS and Debian Stable, and then
proving it on real installations of both.

| Doc | Branch | What it is | Verdict |
| --- | --- | --- | --- |
| [U0](LINUX-UBUNTU-DEBIAN-COMPAT-U0.md) | `research/linux-debian-ubuntu-compat-v1` | read-only audit and implementation plan, from code and measured probes | implementation ready — no blocker |
| [U1](LINUX-UBUNTU-DEBIAN-COMPAT-U1.md) | `feature/linux-debian-ubuntu-compat-v1` | implementation of the U0 delta, and nothing else | delta closed |
| [U2](LINUX-UBUNTU-DEBIAN-COMPAT-U2.md) | certification of `9792330` | certification on real Ubuntu and Debian hosts | certified, with defects recorded |

U0 corrects the earlier desk research in two places — the GTK floor (§14.2) and
the MSRV (§20.3). The MSRV correction is why `desktop/Cargo.toml` and the
packaging spec pin the version they do.

U2 certifies the code **as it was at `9792330`, with its defects present**. It
is deliberately not amended. The defects it found were fixed afterwards, on
separate branches, and those remediation records live in
[`docs/reports/linux/`](../../reports/linux/) as the `U2-HARDENING-*` series.
