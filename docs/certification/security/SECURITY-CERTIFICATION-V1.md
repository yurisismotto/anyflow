# OmniBridge — Security Certification v1

| Field | Value |
| --- | --- |
| **Branch** | `feature/security-certification-v1` |
| **Baseline** | `1a12ffa` (merge of PR #53), plus `5d40112` — the rustls fix this certification itself produced |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, systemd 259.9, GNOME 50.5 Wayland, rustc/cargo 1.98.1, firewalld `FedoraWorkstation` zone |
| **Second LAN device** | **SM-X620, Android 16**, `192.168.68.63/22`, paired throughout. Pairing, trust and grants were preserved; nothing was reset. |
| **Verdict** | **13 PASS · 1 PASS WITH FINDING · 2 N/A · 0 FAIL · 0 BLOCKED** |

Claims are **MEASURED** (a command was run here and its output is quoted),
**TESTED** (an automated test asserts it, named so it can be re-run) or
**SOURCE-VERIFIED**.

---

## 0. Executive summary

The certification found **one real vulnerability** and **two gaps in the
evidence base**. All three are closed.

| # | What was found | Disposition |
| --- | --- | --- |
| **V-1** | **RUSTSEC-2026-0285** — rustls 0.23.43, *"TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries"*, CVSS 5.3 | **FIXED** in its own branch before the certification continued — PR #54. `cargo audit` is now a CI gate that also runs daily. |
| **G-1** | **SEC-LOG-03 had no evidence anywhere.** `clipboard.v1` and `notifications.v1` each got a log-privacy canary when they were built; `files.v1` never did. | **CLOSED** — `daemon/tests/file_log_privacy.rs`. |
| **G-2** | **Nothing had ever asserted that TLS 1.2 is refused.** Both configs declare `TLS13_ONLY` and every test that speaks TLS asks for 1.3, so the refusal was never exercised. | **CLOSED** — a hand-built TLS 1.2 `ClientHello` over a raw socket. |

One **finding** is recorded and not fixed, deliberately:

| # | Finding | Why it is recorded rather than changed |
| --- | --- | --- |
| **F-1** | **Filenames reach the journal.** `capabilities/files/src/lib.rs:913` and `:1705` log `filename = %filename` at `info`, so `medical-results-2026.pdf` appears in `journalctl`. `notifications.v1` redacts titles; `files.v1` does not redact names. | It is metadata, not content, so it is not a SEC-LOG-03 failure, and it is not remotely reachable so it is not a vulnerability. But it is a real inconsistency between two capabilities, and changing product logging in the middle of certifying it is the wrong order. §11.4 carries the recommendation; a characterisation test stops it changing silently either way. |

**No gate was weakened to make anything pass.** Where the environment could
not support a gate as written, the gate is marked **N/A** with the
measurement that establishes why, never as a pass.

---

## 1. Gate results

| Gate | Verdict | Evidence |
| --- | --- | --- |
| **SEC-NET-01** no unexpected OmniBridge listener | **PASS** | §2 — full local enumeration plus an external scan from the paired SM-X620 |
| **SEC-NET-02** no plaintext payload in the capture | **PASS** | §3 — in-process wire relay; sentinels absent |
| **SEC-TLS-01** TLS 1.3 only | **PASS** | §4 — raw TLS 1.2 `ClientHello` answered with fatal alert 70 `protocol_version` |
| **SEC-AUTH-01** unknown peer cannot access capabilities | **PASS** | §5 |
| **SEC-AUTH-02** grants enforced per capability | **PASS** | §5 |
| **SEC-AUTH-03** revoked peer remains revoked | **PASS** | §5 |
| **SEC-FILE-01** traversal prevented | **PASS** | §6 — plus 20,000 generated hostile names |
| **SEC-FILE-02** data stream requires valid authentication/challenge | **PASS** | §6 |
| **SEC-LOG-01** clipboard content absent from logs | **PASS** | §7 |
| **SEC-LOG-02** notification content absent from logs | **PASS** | §7 |
| **SEC-LOG-03** file content absent from logs | **PASS WITH FINDING** | §7 — content absent; **filenames are logged**, finding F-1 |
| **SEC-LOCAL-01** daemon not root | **PASS** | §8 |
| **SEC-LOCAL-02** identity/state/socket permissions correct | **PASS** | §8 |
| **SEC-FUZZ-01** no parser crash/panic in a bounded campaign | **PASS** | §9 — 60,000 inputs, three parsers |
| **SEC-DEPS-01** no unresolved High/Critical dependency vulnerability | **PASS** | §10 — and it found V-1 on its first run |
| **SEC-ANDROID-01** no inappropriate exported sensitive component | **PASS** | §11 |
| SEC-1 external **IPv6** sweep | **N/A** | §2.3 — this network assigns no global IPv6 address to the host |
| SEC-3 packet capture with `tcpdump` | **N/A** | §3.1 — needs `CAP_NET_RAW`; replaced by an in-process relay that sees the same bytes |

---

## 2. SEC-NET-01 — network surface

### 2.1 Local enumeration

**MEASURED**, `ss -lntup`, with the daemon running:

```
tcp  LISTEN  *:55432            users:(("omnibridged",pid=344259,fd=12))
udp  UNCONN  0.0.0.0:5353       users:(("omnibridged",pid=344259,fd=18),(…,fd=17))
udp  UNCONN  *:5353             users:(("omnibridged",pid=344259,fd=20),(…,fd=19))
udp  UNCONN  127.0.0.1:38287    users:(("omnibridged",pid=344259,fd=14))
```

Four sockets. Three are expected: **TCP 55432**, the product listener, and
**UDP 5353** on both address families, which is mDNS.

The fourth needed explaining rather than waving through, and it is **not**
OmniBridge's. **SOURCE-VERIFIED**, `mdns-sd-0.15.2/src/service_daemon.rs:237`:

```rust
let signal_addr = SocketAddrV4::new(LOOPBACK_V4, 0);
let signal_sock = UdpSocket::bind(signal_addr)
```

`ServiceDaemon::new()` binds a loopback UDP socket on an OS-assigned port as a
**self-pipe**, so its mio event loop can be woken alongside the mDNS sockets.
It is bound to `127.0.0.1`, reachable from nothing off-host, and is an
implementation detail of a dependency rather than a listener OmniBridge
introduced.

### 2.2 External scan, from a second machine on the LAN

The brief asks for a scan from a second LAN machine. There is one: the paired
**SM-X620** at `192.168.68.63/22`, same subnet as the host at
`192.168.68.73/22`.

**MEASURED** — every TCP port the host listens on locally, tested from the
phone:

| Port | Reachable from the LAN? | What it is |
| --- | --- | --- |
| **55432** | **yes** | **OmniBridge** |
| 5355 | yes | LLMNR, `systemd-resolved` |
| 27500 | yes | not OmniBridge — unattributed by `ss`, so not owned by uid 1000 |
| 53, 631, 5037, 14639, 42257, 44751 | no | loopback-bound (resolved, CUPS, adb, VS Code) |

And a sweep of ports that must not be open — 21, 22, 23, 25, 80, 139, 443,
445, 3389, 8080, 8443 — found **none reachable**.

**OmniBridge contributes exactly one LAN-reachable TCP port, 55432.**

> A container was tried first and **proved nothing**: rootless podman's pasta
> networking gives the container a copy of the host's addresses and routes, so
> a scan from inside it does not reach the host as an external peer. It
> reported "0 ports open in 1.07 seconds" for a 65535-port scan, which is the
> shape of a scan that never arrived. It is recorded here because that result
> would have looked like a clean pass.

### 2.3 IPv6 — N/A on this network

**MEASURED**: the host has `::1/128` and `fe80::26c0:ae6f:206d:8e3/64` and no
global IPv6 address, although an IPv6 default route is advertised. There is no
global address to scan, so the IPv6 half of SEC-1 is **N/A here** — not
skipped, and not passed. It must be re-run on a network with global IPv6
before OmniBridge claims anything about its IPv6 surface.

The daemon does bind IPv6: `listening port=55432 families=IPv4+IPv6`.

### 2.4 Firewall

**MEASURED**, `firewall-cmd --list-all`, zone `FedoraWorkstation` (default,
active):

```
services: dhcpv6-client samba-client ssh
ports: 1025-65535/udp 1025-65535/tcp
```

This **confirms readiness-audit §4.6**: Fedora Workstation's default zone
already permits TCP 55432 and UDP 5353, so nothing needs opening on a default
install. The packaged firewalld service definition exists for `public`,
`FedoraServer` and hand-tightened zones, and is **installed and never
enabled** — asserted by `packaging/tests/packaging-checks.sh`.

**The package opened no port.** No scriptlet runs `firewall-cmd` on any path.

---

## 3. SEC-NET-02 / SEC-3 — traffic capture

### 3.1 Why a relay and not `tcpdump`

`tcpdump` needs `CAP_NET_RAW` and there is no passwordless root on this host.
The substitute is **not weaker**: an in-process TCP relay sits between the
client and the daemon and records **every byte in both directions** — the same
bytes a capture would show, deterministically, with no privilege and on every
CI runner.

### 3.2 The measurement

**TESTED**, `daemon/tests/security_certification.rs::sec_net_02_no_sensitive_value_crosses_the_socket_in_plaintext`:

A pairing exchange is driven through the relay with two sentinels — the
**pairing token**, which is a real secret exchanged during that very
handshake, and the device name. Neither appears in the captured bytes.

The test refuses to pass vacuously:

* it asserts more than 512 bytes crossed, so an exchange that never happened
  fails rather than passes;
* it asserts the first byte is `0x16`, the TLS handshake content type, so a
  capture of something other than the session fails;
* a separate control, `the_wire_search_finds_a_value_that_is_really_there`,
  proves the substring matcher works on a buffer that does contain the needle.

---

## 4. SEC-TLS-01 — TLS 1.3 only

Nothing had ever asserted this (gap **G-2**). Both configs declare
`TLS13_ONLY` and every test that speaks TLS asks for 1.3, so the *refusal* was
never exercised. A declaration is not a refusal.

### 4.1 A downgrade cannot even be built with our own client

**MEASURED**: `core/Cargo.toml` takes
`rustls = { default-features = false, features = ["std", "ring"] }`. The
`tls12` feature is off, so `rustls::version::TLS12` **does not exist in this
build** — naming it does not compile. The downgrade code is not merely
refused, it was never compiled in.

That is stronger than a refusal, but it is a property of *our* client, and the
gate is about what the **server** does when a stranger offers 1.2. A stranger
is not using our client.

### 4.2 A raw TLS 1.2 `ClientHello`

**TESTED**, `sec_tls_01_a_raw_tls12_client_hello_is_refused`: a `ClientHello`
is built byte by byte with `client_version = 0x0303` and, deliberately, **no
`supported_versions` extension** — which is exactly how a real 1.2-only peer
announces itself and the only way to reach the server's version negotiation
from below.

**MEASURED** response:

```
SEC-TLS-01: the server replied with content type 0x15, alert level 2 description 70
```

A **fatal alert, `protocol_version`**. Not a timeout, not a reset, not a
`ServerHello` — the specific, correct refusal.

The control, `sec_tls_01_control_a_tls13_client_is_accepted`, pairs
successfully against the same server, so the refusal is about the version and
not about the server being broken.

---

## 5. SEC-AUTH-01/02/03 — discovery is not trust

These were covered before this certification by suites written alongside the
features they guard. Duplicating them here would produce a second, weaker copy
that can drift, so they are cited by name and were re-run.

| Property | Tests |
| --- | --- |
| An unpaired peer cannot establish a session at all | `security_certification.rs::sec_auth_01_an_unpaired_peer_cannot_establish_a_session` **(new)** |
| A server presenting an unpinned identity is refused | `sec_auth_01_a_wrong_server_fingerprint_is_refused` **(new)** |
| A client with no certificate is refused in the handshake | `wire.rs::a_client_without_a_certificate_is_rejected_during_the_handshake` |
| A spoofed fingerprint is refused | `wire.rs::trusted_raw_connection` and neighbours |
| **SEC-AUTH-01** an ungranted peer reaches no capability | `clipboard/security.rs::clip_sec_01_an_ungranted_peer_is_refused`, `files.rs::f1_an_offer_from_a_peer_without_a_grant_never_reaches_the_capability` |
| **SEC-AUTH-02** grants are per capability, and follow the pinned identity | `clip_sec_03_policy_follows_the_pinned_identity_not_the_claimed_device_id`, `clip_sec_13_no_inbound_message_can_widen_local_policy`, `f1_a_desktop_cannot_offer_to_a_peer_it_has_not_granted` |
| **SEC-AUTH-03** revocation is durable and immediate | `clip_sec_02_a_revoked_peer_is_refused_even_with_a_permissive_stored_policy`, `clip_sec_16_revocation_stops_clipboard_traffic_on_a_live_session`, `revoked_tombstone.rs` (16 tests, incl. `a_tombstone_survives_a_restart`, `a_tombstone_grants_nothing_to_a_different_fingerprint`), `revoked_cleanup.rs` (12 tests) |
| Replay and proof-of-possession | `pairing.rs` (21 tests: `a_valid_proof_is_accepted_once`, `a_token_cannot_be_used_twice`, `a_proof_for_a_previous_nonce_is_rejected`, `brute_force_is_cut_off_after_a_few_attempts`), `wire.rs::a_replayed_envelope_terminates_the_session`, `a_duplicate_message_id_terminates_the_session`, `a_non_increasing_sequence_number_terminates_the_session` |

**`discovery != trust`** and **`connection != authorization`** both hold: an
mDNS-discovered peer with no pairing cannot complete a session, and a paired
peer with no grant reaches no capability.

---

## 6. SEC-FILE-01/02 — file transfer abuse

| Property | Tests |
| --- | --- |
| **SEC-FILE-01** a hostile filename cannot escape the download directory | `files.rs::f7_f8_a_hostile_filename_cannot_escape_the_download_directory`, plus **20,000 generated names** — §9 |
| **SEC-FILE-02** a data stream requires a valid session and challenge | `files.rs::f2_a_stranger_cannot_open_a_data_stream`, `f3_another_paired_device_cannot_attach_to_someone_elses_transfer`, `f4_a_guessed_transfer_id_is_not_a_bearer_token`, `f5_a_challenge_is_single_use_so_a_second_stream_is_refused`, `f5_a_transfer_id_cannot_be_reused_for_a_second_offer`, `f6_a_transfer_nobody_dialled_expires_and_stops_being_usable` |
| A second file with the same name is numbered, never overwritten | `files.rs::a_second_file_with_the_same_name_is_numbered_not_overwritten` |

The generated campaign (§9) adds the property the hand-written cases cannot
state exhaustively: for **every** accepted name, joining it to a base
directory stays under that base **and adds exactly one path component**. That
second half is what a traversal has to defeat, and it is asserted 13,580 times
per run.

---

## 7. SEC-LOG-01/02/03 — log privacy

All three are proved by **capturing every `tracing` event at `TRACE`**, which
is strictly more than `journalctl` ever shows: the daemon runs at `info` by
default, so a canary that is absent at `TRACE` is absent from the journal.

| Gate | Suite | Verdict |
| --- | --- | --- |
| **SEC-LOG-01** clipboard | `capabilities/clipboard/tests/logging.rs`, and `clip_sec_14_no_debug_or_display_rendering_carries_content` | **PASS** |
| **SEC-LOG-02** notifications | `capabilities/notifications/tests/logging.rs`, `daemon/tests/notification_log_privacy.rs` | **PASS** |
| **SEC-LOG-03** files | `daemon/tests/file_log_privacy.rs` **(new — gap G-1)** | **PASS WITH FINDING** |

Every one of these captures carries a **non-vacuity guard**: a capture that
recorded nothing fails rather than passes. That is not theoretical — the
notifications canary was intermittently recording nothing inside `mock`, which
Phase 3 root-caused to a `tracing` callsite-interest race and fixed by
isolation. The new files canary is in its own binary for the same reason, and
its header says so.

### 7.4 Finding F-1 — filenames reach the journal

**SOURCE-VERIFIED**, `capabilities/files/src/lib.rs`:

```rust
:913   tracing::info!(transfer = %id, peer = …, filename = %filename, size = …,
                      "incoming file offer");
:1705  tracing::info!(transfer = %id, filename = %plan.filename, bytes = …,
                      "received, verified and stored");
```

File **content** never appears — `sec_log_03_file_content_never_reaches_the_log`
asserts the whole canary and a 24-byte prefix of it are absent across a
400×-repeated payload large enough to cross the copy buffer. But the
**filename** does, at `info`, so it lands in `journalctl` and in any bug report
that attaches it.

Why this is a finding and not a failure: SEC-LOG-03 is about *content*, and a
filename is metadata. Why it is a finding at all: `notifications.v1`
deliberately redacts titles, and a filename is closer to a notification title
than to a transfer id. `documents/medical-results-2026.pdf` is not obviously
less sensitive than a message subject.

**Recommendation, for a decision rather than a silent change:** log the
filename at `debug` and a redacted form at `info`, matching how notifications
already treat a title. Not done here, because changing product logging in the
middle of certifying that logging is the wrong order, and because the debugging
value of the current behaviour is real.

`sec_log_03_the_filename_is_logged_and_that_is_a_recorded_finding`
characterises the present behaviour so it cannot change in either direction
without somebody deciding to.

---

## 8. SEC-LOCAL-01/02 / SEC-6 — local hardening

**MEASURED**, with the daemon running:

```console
$ ps -eo user,pid,cmd | grep [o]mnibridged
yuri  344259  ./target/release/omnibridged --log info
```

**SEC-LOCAL-01 PASS** — no `omnibridged` process runs as root, and none can:
the unit is a `systemd --user` unit, the package starts nothing, and no
maintainer script can reach a user's service manager.

**MEASURED** permissions:

```console
$ stat -c '%a %U:%G %n' …
700 yuri:yuri /home/yuri/.local/share/omnibridge
600 yuri:yuri /home/yuri/.local/share/omnibridge/identity.key
600 yuri:yuri /home/yuri/.local/share/omnibridge/state.json
700 yuri:yuri /run/user/1000/omnibridge
600 yuri:yuri /run/user/1000/omnibridge/control.sock
```

**SEC-LOCAL-02 PASS** — all five match the required modes exactly, all owned
by the user. `find … ! -user $USER` over the data directory, the runtime
directory and the downloads directory returned **nothing**: no root-owned file
anywhere in OmniBridge's user state.

The unit's sandbox was certified separately and is re-runnable:
`packaging/tests/systemd-unit-gates.sh` measures gates S1/S2/S3 against the
real unit, and `systemd-analyze security --user omnibridged.service` rates it
**4.7 OK**. Every remaining exposure is itemised in
[`PACKAGING-V1-SYSTEMD-UNIT.md`](../../audits/packaging/PACKAGING-V1-SYSTEMD-UNIT.md) §4.6 —
most of it is `CapabilityBoundingSet=`, which a `--user` unit cannot set and
which the process therefore starts with empty anyway.

---

## 9. SEC-FUZZ-01 — bounded parser campaign

### 9.1 What it is, stated precisely

**Seeded, bounded, structured random testing. Not coverage-guided fuzzing.**
`cargo-fuzz` drives libFuzzer, libFuzzer needs a nightly toolchain, and there
is no `rustup` on this host. Calling it fuzzing without that sentence would be
the word doing work it has not earned.

What it costs: no coverage feedback, so it will not find a path gated behind a
specific magic number. What it still buys: every input reaches a **real**
parser with no mock in between; the generators are **shaped** rather than
uniform, so they get past the first byte and into the paths where parser bugs
live; and it is **deterministic** — a failure prints its seed and reruns
identically.

### 9.2 The campaign

**MEASURED**, `cargo test -- --nocapture`:

```
SEC-FUZZ-01 framing:      20000 inputs — 421 decoded, 11745 rejected, 7834 truncated
SEC-FUZZ-01 filenames:    20000 inputs — 13580 accepted, 6420 refused
SEC-FUZZ-01 device names: 20000 inputs — 19213 non-empty results
```

**60,000 inputs across three parsers. No panic, no hang, no traversal.**

Every campaign asserts it reached **more than one outcome**, so a generator
that degenerated into producing only rejects — and therefore tested only the
first branch — fails rather than passes.

| Parser | Where | Why it is network-facing |
| --- | --- | --- |
| `framing::read_envelope` | `core/tests/parser_fuzz.rs` | the **first** thing an unauthenticated peer reaches, before the handshake has decided anything |
| protobuf decode | same, via wire-plausible envelopes with real tags and varints | reached through framing |
| `filename::sanitize` | `capabilities/files/tests/filename_fuzz.rs` | its output becomes a **path on the receiving machine** |
| `discovery::sanitize_device_name` | `core/tests/parser_fuzz.rs` | arrives over mDNS from anything on the LAN, **before** any pairing, and is rendered in a GUI and written to a log |

Corpus shapes: length prefixes that lie about the body; wire-plausible
protobuf corrupted by bit flips; truncation mid-body; incomplete prefixes;
boundary values around `MAX_FRAME_LEN`; absolute paths; 40-deep `../` runs;
4096-character names; arbitrary Unicode including RTL override, fullwidth stop,
fraction slash, BOM and zero-width space; lossy UTF-8 from random bytes.

Two properties are asserted beyond "it returned":

* **an enormous declared length is refused before allocating.**
  `read_envelope` does `vec![0u8; len]` *after* the bound check;
  `sec_fuzz_01_an_enormous_declared_length_is_refused_before_allocating`
  asserts the order by supplying only the four length bytes for
  `u32::MAX` and timing the refusal.
* **a device name cannot carry a newline** — it reaches the journal, and a
  newline there is log injection.

### 9.3 Duration and reproducibility

The campaign runs in **≈1.2 s** and is part of `cargo test`, which is the only
way a fuzz harness stays honest: one that runs only when somebody remembers is
one that never runs. Seeds are fixed constants in the source; the corpus is
regenerated identically on every run, so there is no corpus directory to keep
and a finding is reproduced by running the test.

---

## 10. SEC-DEPS-01 — dependencies and supply chain

### 10.1 The vulnerability this gate found

**MEASURED**, first run:

```
Crate:    rustls
Version:  0.23.43
Title:    TLS 1.3 handshake messages incorrectly accepted across
          encryption level boundaries
ID:       RUSTSEC-2026-0285
Severity: 5.3 (medium)
Solution: Upgrade to >=0.23.45
```

Directly relevant, not incidental: OmniBridge speaks TLS 1.3 and nothing else,
so the advisory sits on the path **every peer session uses**.

SEC-9's letter requires action only on High/Critical, and this is Medium. It
was fixed anyway — a patch bump in the library the whole trust model rests on,
where an evidence-backed exception would cost more to justify than the fix
costs to apply. Per the brief, the certification **stopped**, the fix went in
its own branch (**PR #54**, merged as `5d40112`), and the floor was raised in
`core/Cargo.toml` rather than only in the lockfile so resolution cannot drift
back below it.

**MEASURED** after the fix: `cargo audit --deny warnings` — clean, 282
dependencies, 0 vulnerabilities and 0 unmaintained-crate warnings.

### 10.2 It is now a standing gate

`.github/workflows/security-audit.yml` runs `cargo audit --deny warnings` on
every change to a manifest or the lockfile, **and daily on a schedule**. The
schedule is the point: this is the one check in the repository whose result can
change *without the repository changing*, and a new advisory against an
unchanged lockfile is exactly what a push-triggered job cannot see.

`--deny warnings` also fails on unmaintained and unsound advisories. An
accepted finding would go in `.cargo/audit.toml` with its id and a written
reason, in the repository and reviewable — never as a flag hidden in the
workflow.

`cargo deny` was **not** run: it is not configured in this repository, and
adding a licence and duplicate-crate policy is a project decision rather than a
security measurement. Recorded as a debt.

### 10.3 Secrets

**MEASURED** over the tracked tree: no private key blocks, no API-key
patterns, no AWS or GitHub or Slack token shapes, and **no `.key`, `.pem`,
`.p12`, `.pfx`, `.jks` or `.keystore` file tracked by git**.

The release bundle enforces the same list independently:
`make-source-bundle.sh` removes those extensions plus `state.json`,
`trust-store.json`, `id_rsa*` and `id_ed25519*`, then **re-checks** and fails
hard if anything survived. `packaging/tests/packaging-checks.sh` asserts the
built bundle carries none of them.

`protocol/testdata/*.der` is deliberately present and is **not** a finding:
those are X.509 *certificates* — public halves only — used as cross-language
test vectors by both the Rust and Android suites.

SBOM generation is **not** implemented (`cargo-cyclonedx` is absent). Recorded
as a debt; it belongs with the release artifacts in Phase 7.

---

## 11. SEC-8 / SEC-ANDROID-01 — Android

Static analysis against the source. MobSF was not available and is not
required: every claim below is read out of the manifest or the Kotlin, which is
what a scanner's findings would have had to be verified against anyway.

### 11.1 Exported components — three, each justified

| Component | Exported | Guard |
| --- | --- | --- |
| `.ui.MainActivity` | **true** | `LAUNCHER`. Required for the app to be startable; accepts no data. |
| `.ui.SendActivity` | **true** | `ACTION_SEND` share target. Required for the sharesheet. Receives a URI carrying a **temporary read grant for one item** — strictly less access than any storage permission, which is why none is declared. |
| `.ui.ClipboardTileService` | **true** | `android:permission="android.permission.BIND_QUICK_SETTINGS_TILE"` — **only the system holds it.** |

**Not exported**, and both are the sensitive ones:

| Component | Exported | Guard |
| --- | --- | --- |
| `.service.ConnectionService` | **false** | the daemon-equivalent; nothing outside the app can bind it |
| `.notifications.OmniBridgeNotificationListener` | **false** | *and* `android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE"` |

The notification listener — the component with access to every notification on
the device — is `exported="false"` **and** permission-guarded, which is the
more restrictive of the two accepted patterns.

**SEC-ANDROID-01 PASS.**

### 11.2 Permissions, backup, cleartext

**MEASURED**, the complete list: `INTERNET`, `ACCESS_NETWORK_STATE`,
`CHANGE_WIFI_MULTICAST_STATE`, `CHANGE_NETWORK_STATE`, `FOREGROUND_SERVICE`,
`FOREGROUND_SERVICE_CONNECTED_DEVICE`, `POST_NOTIFICATIONS`, `CAMERA`.

* **No storage permission of any kind.** Files arrive as URIs with temporary
  grants.
* **No location permission** — notable, because mDNS-using apps frequently
  request one.
* `CAMERA` is for QR pairing.

| Control | State |
| --- | --- |
| `android:allowBackup` | **false** |
| `dataExtractionRules` | every domain excluded from **both** cloud backup and device transfer |
| `usesCleartextTraffic` | not declared; `network_security_config.xml` sets `cleartextTrafficPermitted="false"` app-wide |
| `FileProvider` / any `<provider>` | **none declared** — no exported content-provider surface |

The backup exclusion is reasoned rather than reflexive: the identity private
key is in the Android Keystore and non-exportable, so a restored backup could
never contain a usable identity, and copying the trust store without it would
produce a confusing half-state.

### 11.3 PendingIntent, Keystore, logging

* **PendingIntent:** 3 × `FLAG_IMMUTABLE`, **0 × `FLAG_MUTABLE`**.
* **Keystore:** `KeyGenParameterSpec` in `AndroidKeyStore`, StrongBox
  attempted with a logged TEE fallback.
* **Logging:** 70 `Log.*` calls. None interpolates a notification title or
  body, clipboard text, a token or key material. The content-adjacent ones
  emit exception **class names** (`e.javaClass.simpleName`), a shortened
  fingerprint, or a battery percentage. The one line naming a secret logs its
  **state enum and generation count**, with a comment saying so.

### 11.4 Runtime

`adb` confirmed the device (`SM-X620`, Android 16) attached and on the LAN at
`192.168.68.63`, and it served as the external scanning vantage point in §2.2.
**Pairing, trust and grants were preserved throughout; nothing was reset or
re-paired.**

A full `adb logcat` sentinel run belongs with the live capability exercises in
Phase 6, where clipboard, files and notifications are driven end to end on
hardware. The static analysis above is what this phase can establish.

---

## 12. SEC-7 — D-Bus

| Property | Evidence |
| --- | --- |
| The self-heal is **user session bus only** | `platform-linux/tests/dbus_activation.rs::this_crate_never_opens_the_system_bus` walks **every** `.rs` in the crate and fails if the system-bus constructor appears anywhere — not just in the file that makes the call today |
| No system-bus privilege surface introduced | The daemon connects with `zbus::Connection::session()`. The self-heal makes exactly two calls, `ListActivatableNames` and `ReloadConfig`, at most one reload per start |
| `ReloadConfig` starts nothing | It re-reads the bus's own configuration. The integration suite's service files name `Exec=/bin/true` so that even a test that did activate would start nothing |
| D-Bus is not a startup dependency | MEASURED: the daemon run with `DBUS_SESSION_BUS_ADDRESS` pointing at a nonexistent path bound its listener, brought up its control socket, logged at `debug` and stayed up |

Full detail: [`PACKAGING-V1-DBUS-ACTIVATION.md`](../../audits/packaging/PACKAGING-V1-DBUS-ACTIVATION.md).

---

## 13. What this certification does not cover

| # | Not covered | Why |
| --- | --- | --- |
| 1 | External **IPv6** scanning | §2.3 — no global IPv6 address on this network. **N/A**, to be re-run elsewhere. |
| 2 | `tcpdump`/Wireshark capture | §3.1 — needs `CAP_NET_RAW`. Replaced by a relay that sees the same bytes. |
| 3 | Coverage-guided fuzzing | §9.1 — no nightly toolchain. Replaced by a seeded bounded campaign, labelled as such. |
| 4 | `cargo deny` | §10.2 — not configured; licence policy is a project decision. Debt. |
| 5 | SBOM | §10.3 — `cargo-cyclonedx` absent. Belongs with release artifacts, Phase 7. |
| 6 | MobSF | §11 — unavailable. Static analysis was done against source directly. |
| 7 | Live `adb logcat` sentinel run; end-to-end clipboard/file/notification exercises on hardware | §11.4 — belongs with the Phase 6 lifecycle certification, which drives the capabilities on real hardware. |
| 8 | Penetration testing by a human adversary | Out of scope for an automated certification. |

---

## 14. Verdict

**SECURITY CERTIFICATION V1: PASS WITH ONE RECORDED FINDING.**

Thirteen gates pass, one passes with finding **F-1**, two are **N/A** with the
measurement that makes them so, and none fails or is blocked.

The certification did its job: it found a **real vulnerability** in the TLS
library the entire trust model depends on, and it found that **one of its own
three log-privacy gates had never had any evidence**. Both are closed, and
`cargo audit` is now a daily CI gate rather than something run by hand once a
release.

What is *not* claimed: that OmniBridge has been penetration-tested by a human,
that its IPv6 surface has been scanned, or that coverage-guided fuzzing has
been run. Those are named in §13 with the reason, and the word "certified" does
not extend to them.
