# Security certification

| Doc | What it is | Verdict |
| --- | --- | --- |
| [Security Certification v1](SECURITY-CERTIFICATION-V1.md) | the sixteen SEC gates measured against the real build, plus an external scan from the paired SM-X620 | **13 PASS · 1 PASS WITH FINDING · 2 N/A · 0 FAIL** |
| [Security evidence closure v1](SECURITY-EVIDENCE-CLOSURE-V1.md) | SEC-LOG-03's deferred `journalctl`/`logcat` rows and L16's privacy half, on real hardware at `TRACE` | **CLOSED on Ubuntu 24.04 and Debian 13 · 0 advisories · one harness defect that could have passed a real leak** |

It found two things worth knowing before reading anything else:

* **a real vulnerability** — RUSTSEC-2026-0285 in rustls, on the TLS 1.3 path
  every peer session uses. Fixed in its own branch (PR #54) before the
  certification continued, and `cargo audit` is now a CI gate that runs daily
  as well as on every manifest change;
* **a gate with no evidence** — SEC-LOG-03 (file content absent from logs) had
  never been tested, while the clipboard and notification equivalents were
  covered twice over. Closed by `daemon/tests/file_log_privacy.rs`.

The one open finding, **F-1**, is that `files.v1` logs filenames at `info`
while `notifications.v1` redacts titles. It is metadata rather than content, so
it is not a gate failure, but it is a real inconsistency and §7.4 carries the
recommendation. A characterisation test stops the behaviour changing in either
direction without somebody deciding to.

The evidence closure adds two things to that finding, both measured: both
desktop log sites are on the **receive** path, so a desktop that only sends
logs no filename at all; and the **Android app makes the same choice**, logging
`FileTransfer: incoming <name>`. Whatever is decided about F-1 is therefore a
decision about two implementations.

## Re-running it

Most of the evidence is automated and is part of `cargo test`:

| Gate | Where |
| --- | --- |
| SEC-TLS-01, SEC-NET-02, SEC-AUTH-01 | `desktop/daemon/tests/security_certification.rs` |
| SEC-FUZZ-01 | `desktop/core/tests/parser_fuzz.rs`, `desktop/capabilities/files/tests/filename_fuzz.rs` |
| SEC-LOG-01/02/03 | `capabilities/{clipboard,notifications}/tests/logging.rs`, `daemon/tests/{notification,file}_log_privacy.rs` |
| SEC-DEPS-01 | `.github/workflows/security-audit.yml` |
| SEC-LOCAL-01/02 | `packaging/tests/systemd-unit-gates.sh` |
| SEC-LOG-03 on hardware, L16 privacy | `packaging/tests/security-log-evidence.sh` — needs a guest, a paired phone and the notification fixture |

The host measurements in §2 and §8, and the external scan from the phone, are
transcripts rather than scripts: they depend on a LAN, a paired device and a
running daemon.
