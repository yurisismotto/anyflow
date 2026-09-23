# 09 — Windows security and integration

| Field | Value |
| --- | --- |
| **Title** | Windows security model, local IPC, multi-user, updates, attack surface |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Everything security-relevant about running OmniBridge on Windows that [08](08-WINDOWS-FEASIBILITY.md) states but does not analyse. |
| **Decision status** | PROPOSED |
| **Evidence** | OFFICIAL DOC VERIFIED from `learn.microsoft.com`; REPO VERIFIED for the Linux controls being mapped. |
| **Related documents** | [08](08-WINDOWS-FEASIBILITY.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md), [20](20-SECURITY-THREAT-ANALYSIS.md), [../../security/THREAT_MODEL.md](../../security/THREAT_MODEL.md) |

---

## 1. The rule this document exists to enforce

> Expansion must not weaken the model. Where a platform cannot provide an equivalent control,
> say so explicitly rather than dropping it quietly.

OmniBridge's Linux security posture rests on controls that have no direct Windows analogue.
This document maps each one, and names the two places where Windows is genuinely *stronger*
and the three where it is genuinely weaker.

---

## 2. Control-by-control mapping

| # | Linux control | Where | Windows equivalent | Verdict |
| --- | --- | --- | --- | --- |
| C1 | Private key file mode 0600, **enforced at load**; refuses to start otherwise | `store.rs::require_private_mode` | With a TPM key there **is no key file**. Software fallback: DACL granting only the user SID, plus DPAPI. An equivalent *enforcement* check must be written — see §3 | **Stronger** (TPM) / **parity** (fallback), if the check is written |
| C2 | Data dir 0700, auto-hardened | `store.rs::harden_dir` | `%LOCALAPPDATA%\OmniBridge` inherits a user-only ACL by default. Must be *verified*, not assumed | Parity, needs code |
| C3 | Atomic write with the final mode from creation | `store.rs::write_atomic` | `CreateFile(CREATE_NEW)` + explicit SD + `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` | Parity |
| C4 | Control socket unreachable by other users (dir 0700 in `$XDG_RUNTIME_DIR`) | `control.rs`, `server.rs` | Named pipe with a DACL restricted to the owning SID, **and `GetNamedPipeClientProcessId` / impersonation to verify the caller** | **Stronger** — Windows can *authenticate* the client, Unix relies on path permissions. §4 |
| C5 | Unprivileged daemon; no root, no capabilities | `daemon/src/main.rs` | Agent runs as the interactive user, not elevated, not `LocalSystem` | Parity |
| C6 | systemd sandbox: `ProtectSystem=strict`, `SystemCallFilter`, `RestrictAddressFamilies`, `MemoryDenyWriteExecute`, … | `omnibridged.service` | **No equivalent.** AppContainer/MSIX-container would be the closest and conflicts with clipboard + firewall + arbitrary Downloads writes | **WEAKER — accepted gap.** §5 |
| C7 | `O_EXCL` defeats a symlink planted in the download dir | `destination.rs` | `CREATE_NEW` gives the same guarantee. Reparse points need a deliberate decision | Parity, needs care |
| C8 | Filename sanitisation | `filename.rs` | **Insufficient.** No reserved device names, no trailing dot/space, no `:`/`\` handling | **WEAKER until fixed.** §6 |
| C9 | One user per session; the daemon is per-user | implicit | Fast user switching means **several interactive sessions at once**. §7 | Needs design |
| C10 | Packages signed by the distro | RPM/dnf | Authenticode + MSIX signing; SmartScreen | Parity |
| C11 | Firewall not touched by the package | (nothing today) | Rules created by the installer, Private + LocalSubnet | **Stronger** — explicit rather than absent |
| C12 | No clipboard content in logs, proven by a test | `capabilities/clipboard/tests/logging.rs` | Same test applies; Windows adds **Cloud Clipboard** as a new exfiltration path outside our control. §8 | New risk |

---

## 3. Key storage and the "no key file" case

With `MS_PLATFORM_CRYPTO_PROVIDER` the private key never exists as bytes we can hold, so C1
becomes vacuous in the good case. That is a genuine improvement over Linux, where
[ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md) explicitly accepts a file-based key
because a `systemd --user` daemon may start before the keyring is unlocked.

The important consequence: **`Store::open()`'s current invariant — "load the key, check its
mode, refuse if loose" — no longer describes the good path.** The Wave 0 `IdentitySigner`
seam ([02 §5](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md)) must therefore carry the *check* as
well as the *signing*:

```rust
// PROPOSED, illustrative.
trait IdentitySigner {
    /// Refuse to start if the stored key is not protected as this platform requires.
    /// Linux: mode check. Windows software fallback: DACL check. TPM/Enclave: trivially Ok.
    fn verify_protection(&self) -> Result<()>;
    fn backing(&self) -> KeyBacking;
}
```

Without this, the Windows software fallback would ship with *no* equivalent of
`require_private_mode` — a silent weakening of exactly the kind the brief forbids.
**SEC-002** in the backlog.

Fallback policy, stated plainly:

- Prefer TPM. Attempt `MS_PLATFORM_CRYPTO_PROVIDER` first, exactly as Android tries StrongBox
  before the TEE (`DeviceIdentity.kt`, REPO VERIFIED — and note it uses `runCatching` because
  StrongBox "throws only at generation time", which is the same pattern needed here).
- Fall back to `MS_KEY_STORAGE_PROVIDER` **without** `NCRYPT_ALLOW_EXPORT_FLAG`.
- **Record which one was used, and show it.** `omnibridge status` and the UI must say
  "hardware-backed (TPM)" or "software key".
- Do **not** re-key silently if a TPM appears later. The identity is the pin; changing it
  breaks every pairing. A re-key must be a user-initiated action with a clear warning.

Whether the *peer* should learn the backing is deferred to
[14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) — recommendation: not in v1, because it is
a protocol change and an unverifiable self-report.

---

## 4. Local IPC: the named pipe

The Linux control socket's security is entirely positional: it lives in `$XDG_RUNTIME_DIR`,
which is mode 0700 and per-user, so no other user can reach the path. `server.rs` additionally
chmods the socket. There is no caller authentication — and none is needed, because the
filesystem already answered the question.

Windows has no `$XDG_RUNTIME_DIR`. The named pipe namespace is machine-global:
`\\.\pipe\<name>` is visible to every session on the box. So the pipe must carry its own
access control:

1. **Name it per-user:** `\\.\pipe\OmniBridge\<user SID>\control`. Not for security — names are
   not secrets — but so that fast user switching gives each session its own agent and pipe
   without collision (§7).
2. **DACL restricted to the owning user's SID**, denying everyone else including
   `Authenticated Users`. This is the direct analogue of the 0700 directory and is the primary
   control.
3. **Verify the caller.** `GetNamedPipeClientProcessId` / `ImpersonateNamedPipeClient` lets
   the server confirm the connecting process's token belongs to the same user. Unix cannot do
   this without `SO_PEERCRED`, and OmniBridge does not do it on Linux. **This is one of the two
   places Windows lets OmniBridge be stronger than its reference platform — take it.**
4. **`FILE_FLAG_FIRST_PIPE_INSTANCE`** on creation, so a hostile process cannot pre-create the
   pipe name and impersonate the agent (a classic named-pipe squatting attack). Without this
   flag, a malicious process that starts first owns the name and the *UI* would connect to it.
   **This is not optional.** **SEC-003.**

Deliberately rejected alternatives:

- **Localhost TCP.** Any local process, in any session, could connect; it needs its own
  authentication layer; and it puts a listening socket in front of the firewall UI for no
  reason.
- **AF_UNIX on Windows.** It exists (Windows 10 build 17063+), but the Windows implementation
  supports **no ancillary data**, so there is no credential passing and no `SO_PEERCRED`
  analogue (OFFICIAL DOC VERIFIED). It would give the *weaker* of the two models.
  `tokio::net::UnixStream` is `cfg(unix)` regardless.
- **Windows App Services.** Packaged-app-only; awkward for a non-Store installation.

---

## 5. The sandboxing gap, stated honestly

`omnibridged.service` is hardened well beyond what most desktop daemons bother with:
`ProtectSystem=strict`, `ProtectHome=read-only` with one writable path, `NoNewPrivileges`,
`RestrictNamespaces`, `MemoryDenyWriteExecute`, `SystemCallArchitectures=native`,
`SystemCallFilter=@system-service` minus `@privileged @resources @obsolete`, and
`RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK`.

**None of this transfers to Windows.** The nearest equivalents:

| Windows mechanism | Why it does not fit |
| --- | --- |
| AppContainer | Would break clipboard access, arbitrary Downloads writes, and the firewall model |
| MSIX container / packaged identity | Provides install/uninstall hygiene and virtualised registry/AppData, not syscall restriction |
| Process mitigation policies (`SetProcessMitigationPolicy`) | **Partially applicable and worth doing**: CFG, ACG/dynamic-code prohibition, no-remote-images, binary signature policy. This is the closest analogue of `MemoryDenyWriteExecute` |
| Restricted tokens / job objects | Would break the clipboard and the message pump |

**Verdict: accept the gap, do not hide it.** Concretely:

- Apply the process mitigations that *are* compatible — dynamic-code prohibition in
  particular, since OmniBridge JITs nothing. (**WIN-006**)
- Record in [20](20-SECURITY-THREAT-ANALYSIS.md) that "the Windows agent is not sandboxed to
  the degree the Linux daemon is" as an accepted risk with a named mitigation set.
- Do **not** claim parity in user-facing material.

---

## 6. Filename handling is a real bug, not a gap

`capabilities/files/src/filename.rs` sanitises for POSIX: it strips separators and rejects
`.`/`..`. Its tests confirm the intent (`sanitize("/etc/shadow") == Some("shadow")`).

On Windows the following peer-supplied filenames are all currently mishandled:

| Input | Windows consequence |
| --- | --- |
| `CON`, `NUL`, `COM1`, `LPT1` (with or without extension) | Reserved device names; the write may target a device rather than a file |
| `report.txt.` / `report.txt ` | Win32 strips trailing dots and spaces → a different file than intended, and a mismatch between the name shown to the user and the name written |
| `notes:secret` | `:` opens an **alternate data stream** on the preceding file — content written where nothing shows in Explorer |
| `a\b\c.txt` | `\` is a separator on Windows and is *not* stripped by POSIX-shaped logic |
| Very long names / paths | `MAX_PATH` behaviour differs from Linux's per-component limit |
| Trailing/leading Unicode that normalises to a separator | Worth a look during implementation |

None of these are exploitable today because no Windows build exists. All of them become
exploitable on the first one, and `files.v1` accepts a filename from a **paired but possibly
misbehaving peer** — which the threat model already treats as in scope.

**SEC-004: extend `filename.rs` with a Windows rule set, applied on every platform.**
Applying it everywhere (rather than under `#[cfg(windows)]`) is deliberate: a file received on
Linux and later copied to a Windows machine or a shared drive carries the problem with it, and
one rule set means one test suite. The existing tests in `filename.rs` and
`daemon/tests/files.rs` are the place to extend.

---

## 7. Multi-user Windows — the question the brief asks directly

> *What happens if two people are logged into the same machine?*

Windows fast user switching keeps **multiple interactive sessions alive simultaneously**. Each
has its own desktop, its own window station, and **its own clipboard**. On Linux this is
possible but rare; on a family Windows PC it is normal.

The design must therefore be:

| Aspect | Decision | Rationale |
| --- | --- | --- |
| Agent instances | **One per interactive user session** | The clipboard is per-session; a shared agent could not serve both |
| Identity | **One identity per user**, not per machine | Pairing is a relationship between a phone and a *person's* desktop. A shared machine identity would let user B's session receive clips paired to user A's phone |
| Trust store | Per user, under `%LOCALAPPDATA%` | Follows the identity |
| Named pipe | Per-SID name + per-SID DACL (§4) | Isolation |
| TCP port 55432 | **Conflict.** Only one process can bind it | §7.1 |
| mDNS | Two responders advertising two instances on one host | Legal; instance names are device ids, so no collision. Two `.local.` hostname claims from one host is the thing to watch |
| Downloads | Each user's own `FOLDERID_Downloads` | Automatic |

### 7.1 The port conflict

Two agents cannot both bind TCP 55432. Options, in order of preference:

1. **Bind ephemeral, advertise the actual port.** The port is already in the DNS-SD SRV
   record, and `core/src/lib.rs` already documents `DEFAULT_PORT` as merely a default:
   *"Advertised over mDNS, so a conflicting deployment can simply use another."* The client
   uses the advertised port (`discovery::parse_discovered` builds `SocketAddr`s from record
   addresses + port). **This design already anticipated the problem.** The only work is making
   the agent fall back gracefully instead of failing to start, and making sure the phone never
   hardcodes 55432. → verify in Android's `Endpoints.kt`.
2. First-come-first-served on 55432, later sessions take an ephemeral port. Same mechanism,
   less predictable.
3. A per-machine broker on 55432 that routes to sessions. Rejected — it is the hybrid
   service model [08 §5.2](08-WINDOWS-FEASIBILITY.md) rejects, with a cross-session IPC
   channel that would need its own trust model.

**Recommendation: option 1**, and note that it improves Linux too (a machine with two logged-in
users has the same conflict there).

### 7.2 The security consequence to state loudly

Because the identity is per-user, **user B on the same PC is a different OmniBridge device from
user A**, with a different fingerprint, requiring its own pairing. That is correct and is what
the threat model implies. It should be said in the docs, because the naive expectation is
"I paired with the computer".

---

## 8. New attack surface introduced by Windows

| # | Surface | Risk | Mitigation |
| --- | --- | --- | --- |
| W1 | Named-pipe squatting | A process that creates the pipe name first impersonates the agent; the UI connects to it and leaks control commands | `FILE_FLAG_FIRST_PIPE_INSTANCE`, per-SID DACL, and the UI verifying the server's process token |
| W2 | Cloud Clipboard | A clip OmniBridge writes is uploaded to the user's Microsoft account — a local-first product silently touching a cloud | Set `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`=0 and `CanUploadToCloudClipboard`=0 for `sensitive_hint` clips; document the general behaviour. **VERIFIED (V-04)** — exact names and semantics confirmed |
| W3 | Clipboard history | Every received clip lands in Win+V history | Same mitigation as W2 |
| W4 | Firewall misconfiguration | A rule scoped to Public/Any exposes the listener on untrusted networks | Installer creates Private+LocalSubnet rules explicitly rather than relying on the Windows prompt, whose default includes Public |
| W5 | Installer tampering / unsigned binaries | SmartScreen bypass, supply-chain | Authenticode + MSIX signing; reproducible CI artifacts; publish hashes |
| W6 | Update channel compromise | A malicious update replaces the agent | Signed MSIX; winget manifests point at signed artifacts; no in-app auto-updater in v1 |
| W7 | DLL search-order hijacking | Loading a planted DLL from the app directory | Install under `Program Files` (admin-writable only); `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`; avoid `LoadLibrary` on relative names |
| W8 | Reserved-name / ADS filenames | §6 | SEC-004 |
| W9 | No syscall sandbox | §5 | Accepted gap + process mitigations |
| W10 | Multi-user identity confusion | §7.2 | Per-user identity, documented |

W1 and W8 are the two that would be *bugs* rather than accepted risks. Both must be closed
before a public Windows build.

---

## 9. What does not change

Worth stating, because it is most of the security model:

- TLS 1.3 only, no downgrade path (`tls.rs` compiles TLS 1.2 out entirely).
- SPKI pinning as the sole identity check; hostnames and IPs remain non-identity.
- Explicit, human-confirmed pairing with a nonce-bound HMAC proof.
- Discovery grants nothing.
- Per-capability grants, re-checked per message; `files.v1` never auto-granted.
- No clipboard content in logs.
- No cloud, no account, no telemetry.

**Windows changes where keys live and how local IPC is guarded. It changes nothing about who
is trusted or why.** That is the property this expansion has to preserve, and it does.

---

## 10. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **SEC-002** | `IdentitySigner::verify_protection()` — a per-platform equivalent of `require_private_mode` | High |
| **SEC-003** | Named pipe: `FILE_FLAG_FIRST_PIPE_INSTANCE`, per-SID DACL, caller verification | High |
| **SEC-004** | Windows-safe filename rules, applied on all platforms | **High — blocks any Windows receive** |
| **WIN-006** | Process mitigation policies (dynamic-code prohibition, CFG, signature policy) | Medium |
| **WIN-007** | Ephemeral-port fallback + confirm the client always uses the advertised port | Medium |
| **WIN-008** | Sensitive-clip exclusion formats for clipboard history / Cloud Clipboard | Medium |
| **SEC-005** | Document the systemd-sandbox gap as an accepted risk in the threat model | Low |
