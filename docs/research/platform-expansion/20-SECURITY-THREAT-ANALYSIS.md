# 20 — Cross-platform threat analysis

| Field | Value |
| --- | --- |
| **Title** | New and changed threats introduced by the platform expansion |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Threats that the existing threat model does not cover because it was written for Fedora + Android. |
| **Decision status** | Research only. **This document does not amend `docs/security/THREAT_MODEL.md`** and must not be treated as having done so. |
| **Evidence** | REPO VERIFIED for existing mitigations; OFFICIAL DOC VERIFIED for platform behaviour; HYPOTHESIS where marked. |
| **Related documents** | [../../security/THREAT_MODEL.md](../../security/THREAT_MODEL.md), [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [12](12-APPLE-SECURITY-AND-INTEGRATION.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |

---

## 1. Relationship to the official threat model

`docs/security/THREAT_MODEL.md` scopes itself to the foundation sprint and defines attackers
A1–A6 (passive/active on Wi-Fi, malicious app on the phone, malicious local user on the
desktop, brief physical access, a previously-paired-now-hostile device), with root/kernel
compromise, a malicious build, and TEE hardware attacks explicitly out of scope.

**That attacker model needs no change.** The expansion adds no new *class* of adversary. What
it adds is new *surface* for the existing ones, and it changes what "the desktop" means.

Two attackers get materially more interesting:

- **A4 (malicious local user on the desktop).** On Linux this is a second Unix account and the
  0700 directory answers it. On Windows, fast user switching means A4 may be *concurrently
  logged in with an interactive desktop of their own*, and the named-pipe namespace is
  machine-global. See §3.
- **A6 (previously paired, now hostile).** Unchanged in kind, but the filename attack surface
  triples when Windows semantics enter the picture. See §4.

---

## 2. Threat register

Format: existing mitigation → what changes → recommended mitigation → how it is verified.

### X1 — Windows Session 0 / service misdesign

| | |
| --- | --- |
| **Risk** | A Windows Service implementation would run as `LocalSystem` outside the interactive session, requiring a privileged process and a cross-session IPC channel — a large new attack surface for a product whose current surface is one TLS listener and one Unix socket |
| **Existing** | n/a |
| **Mitigation** | **Do not build a service.** User-session agent, unprivileged ([08 §5](08-WINDOWS-FEASIBILITY.md)) |
| **Verify** | Architecture review; POC-WIN-06 |

### X2 — Named-pipe squatting

| | |
| --- | --- |
| **Risk** | A hostile local process creates `\\.\pipe\AnyFlow\…` before the agent does. The UI connects to it and hands over control commands — pair, grant, send clipboard |
| **Existing** | Linux is immune: the socket path lives in a 0700 per-user directory |
| **Mitigation** | `FILE_FLAG_FIRST_PIPE_INSTANCE`; per-SID DACL; UI verifies the server process token; per-SID pipe name |
| **Verify** | POC-WIN-07 must include a hostile-squatter test, not just a happy path |
| **Severity** | **High.** This is a bug, not an accepted risk |

### X3 — Windows multi-user identity confusion

| | |
| --- | --- |
| **Risk** | A machine-wide identity would let user B's session receive clips paired to user A's phone |
| **Existing** | Implicit single-user assumption throughout the Linux daemon |
| **Mitigation** | Per-user identity, per-user trust store, per-user agent, per-user pipe. Document that "pairing with the computer" means pairing with *that person's account* ([09 §7](09-WINDOWS-SECURITY-AND-INTEGRATION.md)) |
| **Verify** | POC-WIN-06 with two concurrent sessions |

### X4 — Filename attacks with Windows semantics

| | |
| --- | --- |
| **Risk** | Reserved device names, alternate data streams (`a:b`), trailing dots/spaces, `\` as a separator — from a paired-but-hostile peer (A6) |
| **Existing** | `filename.rs` sanitises POSIX separators and `.`/`..`; tested |
| **Mitigation** | **SEC-004** — extend the rule set, apply on **all** platforms, extend the existing tests |
| **Verify** | Unit tests in `filename.rs`; wire tests in `daemon/tests/files.rs` |
| **Severity** | **High.** Blocks any Windows receive path |

### X5 — Silent key-backing downgrade

| | |
| --- | --- |
| **Risk** | A TPM or Enclave becomes unavailable (BIOS reset, TPM clear, VM migration, OS reinstall). The agent takes the "no key yet" path and generates a **new software identity**, breaking every pairing while presenting as a hiccup |
| **Existing** | `Store::open` generates a fresh identity on first run — the same code path |
| **Mitigation** | **SEC-009** — distinguish "no key" from "key exists but unusable"; refuse to start in the second case; never auto-regenerate over an existing identity; always report the backing |
| **Verify** | Fault-injection test per platform |
| **Severity** | **High.** Silent, user-visible as an inexplicable un-pairing |

### X6 — Loss of the systemd sandbox

| | |
| --- | --- |
| **Risk** | The Linux daemon runs under `ProtectSystem=strict`, `SystemCallFilter`, `RestrictAddressFamilies`, `MemoryDenyWriteExecute`. Windows and macOS agents have no equivalent, so a memory-safety bug (in a dependency; the workspace itself is `unsafe_code = "forbid"`) has more room |
| **Existing** | `anyflowd.service` |
| **Mitigation** | Windows: process mitigation policies (dynamic-code prohibition, CFG, signature policy). macOS: hardened runtime without JIT entitlements. **Record as an accepted, named gap; do not claim parity** |
| **Verify** | Documentation review; **SEC-005** |

### X7 — Cloud Clipboard / clipboard history exfiltration

| | |
| --- | --- |
| **Risk** | A clip AnyFlow writes on Windows is uploaded to the user's Microsoft account by Cloud Clipboard, or retained in Win+V history. A local-first product silently touching a cloud |
| **Existing** | `sensitive_hint` is honoured on Linux (`wl-copy --sensitive`) and Android (`EXTRA_IS_SENSITIVE`) |
| **Mitigation** | **WIN-008** — set all three exclusion formats (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`=0, `CanUploadToCloudClipboard`=0) for sensitive clips; document the general behaviour. **VERIFIED (V-04)** — names and semantics confirmed, WIN-008 implementable |
| **Verify** | POC-WIN-05 with clipboard history enabled |
| **Note** | This is the user's own setting, but AnyFlow is the thing putting data there |

### X8 — macOS pasteboard polling and privacy

| | |
| --- | --- |
| **Risk** | Polling `NSPasteboard` may trigger user-facing prompts, or read content the user never intended to share; and a background agent reading the pasteboard on a timer is exactly the pattern macOS privacy features exist to expose |
| **Existing** | The current contract forbids polling |
| **Mitigation** | Poll only `changeCount` (an integer, no content) and read content only when it changes **and** a peer has `auto_send` on; stop polling when no peer wants it; declare the interval in `describe()` |
| **Verify** | **POC-MAC-05 must first measure whether reading prompts at all** |

### X9 — iOS extension isolation and the shared keychain group

| | |
| --- | --- |
| **Risk** | A keychain access group shared between the app and its Share Extension widens who can use the identity key. Two processes writing the trust store concurrently can corrupt it |
| **Existing** | n/a |
| **Mitigation** | **SEC-007** — Share Extension is **read-only** with respect to the trust store; it may use an existing pairing, never create one. Pairing stays in the main app, where the human is |
| **Verify** | POC-IOS-08 |

### X10 — Installer and update compromise

| | |
| --- | --- |
| **Risk** | A tampered installer, or a compromised update channel, replaces the agent — which holds the identity key and the trust store |
| **Existing** | RPM/dnf signing on Fedora |
| **Mitigation** | Authenticode + MSIX signing; Developer ID + notarization; **no in-app auto-updater**; publish checksums; SBOM; `cargo audit` in CI |
| **Verify** | Release-process review; **CI-004**, **CI-005** |

### X11 — Code-signing key compromise

| | |
| --- | --- |
| **Risk** | With signing on four platforms, the signing keys become the highest-value asset in the project — higher than any single device identity |
| **Existing** | n/a |
| **Mitigation** | CI secrets, not developer machines; hardware tokens where the platform supports them; separate keys per platform; a documented revocation plan |
| **Verify** | Process, not code. **SEC-010** |

### X12 — DLL search-order hijacking (Windows)

| | |
| --- | --- |
| **Risk** | A planted DLL in the application directory is loaded by the agent |
| **Mitigation** | Install under `Program Files` (admin-writable only); `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`; avoid relative `LoadLibrary` |
| **Verify** | POC-WIN-06 |

### X13 — Firewall misconfiguration

| | |
| --- | --- |
| **Risk** | A rule scoped to Public or `0.0.0.0/0` exposes the listener on café and hotel networks. Windows' own "allow through firewall" prompt defaults to including Public |
| **Existing** | Linux: no rule is created at all (Fedora's firewalld may in fact *block* AnyFlow — the opposite problem) |
| **Mitigation** | Installer creates Private + `LocalSubnet` rules explicitly; never rely on the prompt; remove rules on uninstall (**WIN-011**) |
| **Verify** | POC-WIN-08, including a negative test on a Public network |

### X14 — mDNS advertisement as a tracking signal

| | |
| --- | --- |
| **Risk** | A stable `id` and `dn` on an untrusted network let an observer recognise the same laptop returning. `discovery.rs` already documents this as an accepted cost, with a per-network suppression toggle as the promised mitigation |
| **Existing** | The toggle **does not exist** |
| **Mitigation** | **UX-006** — implement it. Expansion makes it worse: more devices, more laptops, more networks |
| **Verify** | Feature test |
| **Note** | The only case where the expansion degrades an already-accepted risk with no code change |

### X15 — Platform IPC spoofing, generally

| | |
| --- | --- |
| **Risk** | The control channel authorises pairing, grants, sends and revocations. Any local process reaching it owns the trust relationship |
| **Existing** | Unix: 0700 directory |
| **Mitigation** | Windows: X2. macOS unsandboxed: Unix socket in a user-only directory (the existing model transfers). macOS sandboxed: XPC with code-signing requirements. **Never localhost TCP on any platform** |
| **Verify** | Per-platform IPC POCs |

### X16 — VPN / AP isolation as a silent failure

| | |
| --- | --- |
| **Risk** | Not a security threat but a security-adjacent one: users blame the product, disable protections, or work around it in unsafe ways when it fails silently |
| **Mitigation** | **UX-005** — detect "discovered but cannot connect" and say so |
| **Verify** | Manual, on a guest network |

---

## 3. What does **not** change

The core security model is unaffected by every platform in this study:

| Property | Still true on Windows/macOS/iOS? |
| --- | --- |
| TLS 1.3 only, TLS 1.2 not compiled in | ✅ same rustls code |
| SPKI pinning is the sole identity check | ✅ |
| Hostnames and IPs are never identity | ✅ — certificates carry no SANs by design |
| Proof of possession via `verify_tls13_signature` | ✅ — and the hardware signers make it *stronger* |
| Explicit, human-confirmed pairing with a nonce-bound HMAC proof | ✅ |
| Discovery grants nothing | ✅ |
| Per-capability grants, re-checked per message | ✅ |
| `files.v1` never auto-granted | ✅ |
| No clipboard content in logs (test-enforced) | ✅ — the test is in portable code |
| No cloud, no account, no telemetry | ✅ — and explicitly re-affirmed against the iOS push temptation ([11 §6.4](11-IOS-IPADOS-FEASIBILITY.md)) |
| Revocation keeps the record so a device cannot silently re-pair | ✅ |

**The expansion changes where keys live and how local IPC is guarded. It changes nothing about
who is trusted or why.** Preserving that is the single acceptance criterion for the whole
security half of this research.

---

## 4. Severity ranking

| Rank | Threat | Type |
| --- | --- | --- |
| 1 | **X4** filename attacks with Windows semantics | Bug — blocks Windows receive |
| 2 | **X5** silent key-backing downgrade | Bug — silent identity loss |
| 3 | **X2** named-pipe squatting | Bug — local privilege/control escalation |
| 4 | **X10/X11** installer, update and signing-key compromise | Process |
| 5 | **X3** Windows multi-user identity confusion | Design |
| 6 | **X13** firewall misconfiguration | Design + installer |
| 7 | **X7** Cloud Clipboard exfiltration | Design + platform behaviour |
| 8 | **X6** loss of the systemd sandbox | Accepted gap |
| 9 | **X9** iOS extension isolation | Design |
| 10 | **X14** mDNS tracking | Pre-existing, worsened |

Ranks 1–3 are **defects** and must be closed before the relevant platform ships publicly.
Ranks 4–10 are design and process items to be planned, not fixed in code alone.

---

## 5. What to do with this document

It is research. When the expansion moves from planning to implementation:

1. `docs/security/THREAT_MODEL.md` should be **amended in its own change**, informed by this
   document, not replaced by it.
2. Its scope line ("the foundation Sprint") is already out of date — clipboard and files have
   shipped since. Updating that scope is a separate, overdue task.
3. Each platform's ADR should carry its own security section, as
   [ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md) and
   [ADR-0007](../../adr/ADR-0007-tls-transport-and-pinning.md) do.
