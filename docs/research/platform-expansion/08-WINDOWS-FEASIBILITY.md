# 08 — Windows feasibility

| Field | Value |
| --- | --- |
| **Title** | AnyFlow on Windows 10 and 11 |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Rust target, TLS, discovery, identity, clipboard, filesystem, agent-vs-service, UI, firewall, packaging. |
| **Decision status** | PROPOSED. **PLAT-DEC-002** (agent vs. service) has a strong recommended direction; **PLAT-DEC-007** (UI↔agent IPC) likewise. |
| **Evidence** | OFFICIAL DOC VERIFIED throughout from `learn.microsoft.com` unless marked otherwise. REPO VERIFIED for AnyFlow's own behaviour. |
| **Related documents** | [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [13](13-CROSS-PLATFORM-DISCOVERY.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md), [17](17-BACKGROUND-EXECUTION-MODEL.md), [18](18-UI-PLATFORM-STRATEGY.md) |

---

## 1. Verdict

**Windows is highly feasible and is the strongest expansion target after Linux
consolidation.** Every hard problem has a documented first-party API, and two of them —
hardware-backed identity and event-driven clipboard watching — are *better* supported on
Windows than on Linux.

The work is real but it is engineering, not research: no blocker was found that requires
a design compromise.

The single decision that shapes everything is §5: **AnyFlow on Windows is a user-session
agent, not a Windows Service.** Everything else follows from it.

---

## 2. Toolchain and build

| Item | Finding | Evidence |
| --- | --- | --- |
| Rust target | `x86_64-pc-windows-msvc` is **Tier 1** — "guaranteed to work", official binaries, automated tests | OFFICIAL DOC VERIFIED, *The rustc book*, Platform Support |
| ARM64 | `aarch64-pc-windows-msvc` is Tier 2; `arm64ec-pc-windows-msvc` also exists | OFFICIAL DOC VERIFIED |
| MSVC toolchain | Required for the `-msvc` targets (Build Tools + Windows SDK). The `-gnu` targets exist but mixing them with CNG/WinRT bindings is friction for no gain | — |
| C compiler | `ring` needs one; on Windows that is MSVC + NASM. See [04 §4](04-LINUX-PORTABILITY.md) — settle it there, it applies here | HYPOTHESIS |
| `protoc` | **Not needed.** `protox` is pure Rust | REPO VERIFIED |
| Win32/WinRT bindings | `windows` / `windows-sys` crates (Microsoft-published) | — |

**POC-WIN-01** is simply: does the workspace compile for `x86_64-pc-windows-msvc` after the
Wave 0 changes, and do the portable tests pass. It is a gate, not an experiment.

---

## 3. TLS

Nothing to do. `rustls` + `ring`, TLS 1.3 only, custom pinning verifiers — none of it
consults a system trust store or a platform TLS stack, so it behaves identically on Windows
(`core/src/tls.rs`, REPO VERIFIED). **Schannel is not used and should not be.**

The only Windows-specific TLS work is feeding rustls a key it cannot see — §6.

---

## 4. Discovery

AnyFlow's requirement is narrow: **keep advertising `_anyflow._tcp.local.` with the existing
TXT schema** (`v`, `pv`, `id`, `dn`) so today's Android client finds a Windows desktop with
no client change.

Two routes:

### 4.1 Reuse `mdns-sd` (recommended)

The crate's own README states it "supports macOS, Linux and Windows". `daemon/src/mdns.rs`
would then work unchanged, including `IfKind` family filtering and `enable_addr_auto()`.

Consequence: **AnyFlow runs its own mDNS responder on Windows**, alongside whatever the OS
does. Windows has a built-in mDNS responder (used by the DNS-SD APIs below). Two responders on
one host is legal mDNS but the interaction needs measuring: port 5353 binding with
`SO_REUSEADDR`, duplicate-name probing, and whether Windows' responder answers for a name
`mdns-sd` is also claiming (`{device_id}.local.`).

### 4.2 Use the platform DNS-SD API

Windows has first-party DNS-SD, in two flavours:

| API | Surface | Minimum |
| --- | --- | --- |
| `DnsServiceRegister` / `DnsServiceBrowse` / `DnsServiceDeRegister` (`windns.h`, `dnsapi.lib`) | Win32 | **Windows 10**, desktop apps only (OFFICIAL DOC VERIFIED) |
| `Windows.Networking.ServiceDiscovery.Dnssd` (`DnssdServiceInstance`, `DnssdServiceWatcher`) | WinRT | Windows 10 |

`DnsServiceRegister` is asynchronous and, notably, **"the registration is tied to the lifetime
of the calling process. If the process goes away, the service will be automatically
deregistered."** (OFFICIAL DOC VERIFIED.) That is exactly the semantics AnyFlow's
`Advertisement`/`Drop` pair implements by hand today, so the models match.

One caveat is **PARTIALLY VERIFIED (V-03)**. `DNS_SERVICE_INSTANCE` carries `pszHostName`,
`ip4Address` and `ip6Address` — documented as "the service-associated IPv4/IPv6 address" — so the
capability exists. But Microsoft never documents whether A/AAAA records are emitted on the wire,
or what a `NULL` address pointer does. If the platform API registers `SRV`/`TXT` but not the
address records, an Android client resolving the instance gets no address — a silent, total
discovery failure. `mdns-sd` publishes its own address records, so route 4.1 does not have this
problem, which **demotes V-03 from a blocking question to a contingent one**.

**Recommendation: try `mdns-sd` first (POC-WIN-02), keep the platform API as the fallback.**
It reuses certified code, keeps one implementation across Linux/Windows/macOS, and avoids the
A/AAAA question entirely. Explicitly: **do not require Apple's Bonjour for Windows to be
installed separately.** Neither route needs it, and depending on a third-party service for
core discovery would be a regression against a product principle.

Interoperability with Avahi (Linux), `NsdManager` (Android) and Bonjour (macOS/iOS) is
[13](13-CROSS-PLATFORM-DISCOVERY.md); the wire format is plain DNS-SD in all cases.

---

## 5. Agent vs. Service — the decision

**Recommendation: a per-user, user-session agent. Not a Windows Service. Not both.**

### 5.1 Why not a service

Windows isolates services in **Session 0**, which is non-interactive and has no desktop
(OFFICIAL DOC VERIFIED). Interactive Services were deprecated and the Interactive Services
Detection service was removed in Windows 10 1803. The consequences for AnyFlow are decisive:

| AnyFlow needs | In Session 0 |
| --- | --- |
| Read/write the user's clipboard | **Impossible.** The clipboard is per-window-station; Session 0's is not the user's. |
| `AddClipboardFormatListener` on a message-pump window | **Impossible.** No interactive desktop. |
| The user's Downloads folder | Wrong profile — `LocalSystem`'s, not the user's. |
| A pairing confirmation prompt | **Impossible.** No UI. |
| One identity per human on a shared PC | A service is one process for the machine → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |

Every one of those is a *core* AnyFlow function. `clipboard.v1` alone settles it.

### 5.2 Why not both

A hybrid — service for the network, agent for the clipboard — is worse than either. It adds a
privileged process, a cross-session IPC channel that must be authenticated (a real attack
surface, [20](20-SECURITY-THREAT-ANALYSIS.md)), and an ambiguity about which process owns the
identity key and the trust store. The only thing it buys is running while nobody is logged in
— and AnyFlow is a *device-continuity* product: with nobody logged in there is no clipboard to
sync, no Downloads folder to write to, and no human to confirm anything.

This also matches AnyFlow's Linux design exactly: `anyflowd` is a `systemd --user` unit
running unprivileged as the user, explicitly *"needs no root, no capabilities, and no
system-wide state"* (`daemon/src/main.rs`, REPO VERIFIED). The Windows agent is the same
architecture spelled in a different OS's vocabulary. That is a strong signal it is right.

### 5.3 Shape of the agent

```
AnyFlowAgent.exe   — one per interactive user session
  ├── tokio runtime: TLS listener, mdns responder, capability handlers  (portable Rust)
  ├── a hidden message-only window (HWND_MESSAGE) for WM_CLIPBOARDUPDATE
  ├── Named Pipe server: \\.\pipe\AnyFlow\<user SID>\control
  ├── tray icon: status, quick actions, "Open AnyFlow"
  └── identity key: CNG, Microsoft Platform Crypto Provider (TPM) when available
```

Started at login. Not elevated. Not `LocalSystem`. The install is per-machine (binaries in
`Program Files`), the *run* is per-user, and state lives in `%LOCALAPPDATA%\AnyFlow`.

The message-only window is worth calling out: it is why the agent must be in the interactive
session and why it must pump messages, but it is *not* a visible window and does not make
AnyFlow a GUI app. The tray icon does that, and the tray icon is optional.

### 5.4 Start at login

| Mechanism | Notes | Verdict |
| --- | --- | --- |
| MSIX `windows.startupTask` extension | For a packaged desktop app, `Enabled="true"` in the manifest starts it at login **with no consent dialog**; the user can still disable it in Task Manager → Startup (OFFICIAL DOC VERIFIED) | **Recommended** for the MSIX build |
| `HKCU\…\CurrentVersion\Run` | Classic, works for unpackaged installs, visible in Task Manager → Startup | Fallback for the plain installer |
| Task Scheduler at logon | More capable; more surprising, harder to uninstall cleanly | Not recommended |
| Startup folder shortcut | Fragile | No |

MSIX startup is the notable one: unlike UWP, a packaged *desktop* app does not need
`RequestEnableAsync` and does not prompt. Good UX; also a reason to be conservative and honest
about it in the installer.

---

## 6. Identity: CNG and the TPM

AnyFlow's identity is **ECDSA P-256** (`core/src/identity.rs`, REPO VERIFIED), chosen for
Android Keystore compatibility. Windows CNG lists ECDSA P-256 among the supported algorithms
for its key storage providers (OFFICIAL DOC VERIFIED).

| Provider | Constant | Backing | Use |
| --- | --- | --- | --- |
| Microsoft **Platform** Crypto Provider | `MS_PLATFORM_CRYPTO_PROVIDER` | **TPM** — "private keys are securely stored and cannot be extracted, even by malicious software" | **Preferred** |
| Microsoft Software Key Storage Provider | `MS_KEY_STORAGE_PROVIDER` | Software, DPAPI-protected | Fallback |
| Microsoft Smart Card KSP | `MS_SMART_CARD_KEY_STORAGE_PROVIDER` | Smart card | Not applicable |

(OFFICIAL DOC VERIFIED, "CNG Key Storage Providers".)

The Platform Crypto Provider is documented as incompatible with exportable export policies —
which is the point: the key is non-exportable by construction, exactly like an Android
Keystore key.

### 6.1 How it reaches rustls

This is the part that looked hard and is not. `rustls` documents extension points for keys
that live "in an HSM, or in another process, or perhaps another machine":
implement `rustls::sign::SigningKey` (returning a `rustls::sign::Signer`) and plug it in via
`ResolvesClientCert` / `ResolvesServerCert` (OFFICIAL DOC VERIFIED, `rustls::manual::_03_howto`).

And **it is already implemented for CNG**: the `rustls-cng` crate, published under the
**rustls** GitHub organisation — v0.7.1, targeting rustls **^0.23**, which is the version this
repository pins — provides `CngSigningKey` built from an `NCryptKey`, with support for
"non-exportable private certificate chains from the Windows certificate store" and ECDSA on
secp256r1 (OFFICIAL DOC VERIFIED, docs.rs).

So the Windows identity path is: `NCryptOpenStorageProvider(MS_PLATFORM_CRYPTO_PROVIDER)` →
`NCryptCreatePersistedKey(NCRYPT_ECDSA_P256_ALGORITHM)` → `NCryptFinalizeKey` → wrap in
`CngSigningKey` → hand to a `ResolvesClientCert`. The `IdentitySigner` seam from
[02 §5](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) is what makes it fit.

Note that `rustls` `Signer::sign` is **blocking**. A TPM signature is fast and requires no
user presence, so this is fine on Windows — unlike the Apple case
([12](12-APPLE-SECURITY-AND-INTEGRATION.md)).

### 6.2 Certificate

AnyFlow's certificate is a self-signed envelope for a raw public key; trust is the SPKI pin
(`core/src/fingerprint.rs`). With a TPM key, `rcgen` cannot generate the key — it must build
the certificate *around* a public key the TPM gives us and have the TPM sign the TBS. `rcgen`
supports a remote-key-pair path for this. **POC-WIN-03 must prove key-gen → certificate →
`Fingerprint::from_certificate_der` → a completed handshake against the existing Linux daemon,
end to end.** Proving the pieces separately proves nothing.

### 6.3 Fallback when there is no usable TPM

Not every machine has TPM 2.0 exposed, and enterprise policy or a VM can remove it. The
fallback is `MS_KEY_STORAGE_PROVIDER` with `NCRYPT_ALLOW_EXPORT_FLAG` **not** set and a DPAPI
user-scoped key.

The rule from the sprint's security principle, restated: **the fallback must be explicit and
visible, never silent.** `anyflow status` must report the key backing, the first-run flow
should state it, and [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) covers whether a peer
should be told (recommendation: not in v1, and not without a deliberate protocol decision).

---

## 7. Clipboard

Windows has the best clipboard API of the three desktops for AnyFlow's purposes.

| Operation | API | Notes |
| --- | --- | --- |
| **watch** | `AddClipboardFormatListener(hwnd)` → `WM_CLIPBOARDUPDATE` (0x031D) | A genuine event, no polling. `RemoveClipboardFormatListener` on shutdown. Needs a window and a message pump. OFFICIAL DOC VERIFIED. |
| **read** | `OpenClipboard` → `GetClipboardData(CF_UNICODETEXT)` → `GlobalLock` → `CloseClipboard` | UTF-16; convert to UTF-8 for `ClipboardText`. Must handle `OpenClipboard` failing because another process holds it — retry with backoff, never block forever. |
| **write** | `OpenClipboard` → `EmptyClipboard` → `SetClipboardData(CF_UNICODETEXT, hMem)` → `CloseClipboard` | Ownership transfers to the system. |
| **sensitive hint** | Register and set **all three**: `ExcludeClipboardContentFromMonitorProcessing` (any data), `CanIncludeInClipboardHistory` = DWORD 0, `CanUploadToCloudClipboard` = DWORD 0 | **VERIFIED (V-04)**, learn.microsoft.com *Clipboard Formats*. Research v1 named two of three. The two DWORD formats cover **disjoint** halves — history and device sync respectively — and each explicitly "does not affect" the other, so setting all three is belt-and-braces rather than redundant. All are obtained via `RegisterClipboardFormat`. WIN-008 is now implementable. |

Mapping onto the existing trait is direct: `ClipboardBackend::{read_text, write_text,
watch_changes, watch_availability, describe}` all have natural Windows implementations, and
`watch_changes` returns a *signal* (`mpsc::Receiver<()>`) which is exactly what
`WM_CLIPBOARDUPDATE` is — a notification with no payload. **The trait's core design decision
turns out to fit Windows better than it fits Linux.**

Two Windows-specific hazards:

- **Loop suppression.** AnyFlow's dedup (`capabilities/clipboard/src/dedup.rs`) already
  suppresses echoes by content hash and `event_id`, and that is content-based, so it carries
  over. But `SetClipboardData` *will* fire our own `WM_CLIPBOARDUPDATE`, so the backend must
  also expect and tolerate a self-triggered event. Because suppression is content-hash based
  rather than sequence based, this should be handled already — **and must be tested, not
  assumed** (POC-WIN-05).
- **Cloud Clipboard.** If the user has clipboard sync across their Microsoft account enabled,
  a clip AnyFlow writes can be uploaded to Microsoft. That is the user's setting, not
  AnyFlow's behaviour, but it is worth documenting for a local-first product, and it is
  precisely why the sensitive-hint format matters. → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md),
  [20](20-SECURITY-THREAT-ANALYSIS.md).

---

## 8. Filesystem, downloads, notifications

| Concern | Windows answer |
| --- | --- |
| Downloads folder | `SHGetKnownFolderPath(FOLDERID_Downloads)`. Replaces the XDG chain in `destination.rs`. |
| Data directory | `%LOCALAPPDATA%\AnyFlow` (`FOLDERID_LocalAppData`). Replaces `$XDG_DATA_HOME/anyflow`. |
| Key file protection | No `0600`. Use a DACL granting only the owning user SID, or DPAPI (`CryptProtectData`, `CRYPTPROTECT_UI_FORBIDDEN`) — and preferably neither, because with a TPM key there is no key file. Note the `require_private_mode` check must gain a Windows equivalent, not be skipped. → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| Atomic write | `MoveFileEx(..., MOVEFILE_REPLACE_EXISTING)`. `std::fs::rename` on Windows already replaces. |
| `O_EXCL` equivalent | `CreateFile` with `CREATE_NEW`. Windows has no symlink-following problem of the same shape, but reparse points exist — use `FILE_FLAG_OPEN_REPARSE_POINT` semantics deliberately. |
| **Filename sanitisation** | **Must be extended.** Reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, with or without an extension), trailing dots/spaces, `:` (alternate data streams), `\` as a separator, and `<>"|?*`. `filename.rs` handles none of these. **Security item — SEC-004.** |
| Notifications | Toast notifications via `Windows.UI.Notifications` / the App SDK's `AppNotificationManager`. Needs an AUMID, which packaged (MSIX) apps get for free and unpackaged apps must register. |

---

## 9. UI

**Recommendation: WinUI 3 on the Windows App SDK**, talking to the agent over the named pipe.

| Option | Assessment |
| --- | --- |
| **WinUI 3 / Windows App SDK** | Microsoft's current desktop UI stack. Latest stable **2.4.0**; runs on Windows 10 1809+ and Windows 11; works in **unpackaged** apps as well as packaged (OFFICIAL DOC VERIFIED). Native look, Fluent, dark mode, accessibility. C# or C++. |
| WPF | Mature, stable, huge ecosystem, but the older design language | Acceptable fallback |
| Win32/Comctl | Only sane for the tray icon | Used for exactly that |
| Electron / web | Rejected — [18](18-UI-PLATFORM-STRATEGY.md) | No |
| Rust-native (egui, iced, slint) | No native Windows feel; accessibility gaps | No |

Windows 10 1809 as WinUI's floor is comfortable: Windows 10 mainstream support has ended for
most consumer SKUs, so the practical target is Windows 11 with Windows 10 22H2 as a courtesy.

The UI is a **client of the agent**, exactly as `anyflow-gui` is a client of `anyflowd` over a
Unix socket today (`gui/src/client.rs`, 203 lines of newline-delimited JSON). Keeping that
shape means:
- no Rust↔C# FFI at all;
- the session survives the UI closing or crashing;
- the control protocol stays the single contract, so a Windows CLI is free.

**PLAT-DEC-007: Named Pipe.** Not localhost TCP (any local process could connect, and it would
show up in firewall prompts), not Windows App Services (packaged-app-only and awkward for a
non-store install), not AF_UNIX (it exists since Windows 10 build 17063 but carries **no
ancillary data**, so there is no credential passing — OFFICIAL DOC VERIFIED — and
`tokio::net::UnixStream` is `cfg(unix)` anyway). A named pipe with a DACL restricted to the
owning user's SID is the direct analogue of the 0700-directory model, and it is the one
mechanism that can *authenticate the caller* — see [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md).

---

## 10. Firewall

The listener needs an inbound rule. Microsoft's own guidance for home/small-business
applications is to restrict the remote address rather than the port scope: *"it is best to
modify the remote address restriction to specify 'Local Subnet' only"* (OFFICIAL DOC VERIFIED).

Proposed rules:

```powershell
New-NetFirewallRule -DisplayName "AnyFlow (TCP 55432)" `
  -Direction Inbound -Program "C:\Program Files\AnyFlow\AnyFlowAgent.exe" `
  -Protocol TCP -LocalPort 55432 `
  -Profile Private -RemoteAddress LocalSubnet -Action Allow

New-NetFirewallRule -DisplayName "AnyFlow mDNS (UDP 5353)" `
  -Direction Inbound -Program "C:\Program Files\AnyFlow\AnyFlowAgent.exe" `
  -Protocol UDP -LocalPort 5353 `
  -Profile Private -RemoteAddress LocalSubnet -Action Allow
```

Rules:
- **Private profile only.** Never Public — a café network is exactly where AnyFlow must not
  accept connections.
- **`RemoteAddress LocalSubnet`.** Never `0.0.0.0/0`.
- **Program-scoped**, full path (wildcards are not supported in application rules —
  OFFICIAL DOC VERIFIED).
- Domain profile: leave off by default; a managed network is the admin's decision.

Installer experience: creating a firewall rule needs elevation, so the installer asks once.
The alternative — letting Windows show its own "allow this app through the firewall?" prompt
on first bind — is friendlier but yields a rule scoped by whatever the user clicked, and the
default in that dialog includes Public. **Recommendation: create the rules explicitly in the
installer, scoped as above; do not rely on the prompt.** Removal on uninstall is mandatory.

---

## 11. Packaging

| Option | Assessment |
| --- | --- |
| **MSIX** | Clean install/uninstall, `windows.startupTask`, an AUMID for toasts, per-user or per-machine, real update story. Requires signing (a trusted cert, or sideload trust). **Recommended primary.** |
| **winget** | Not a format — a manifest pointing at an MSIX or an installer. Cheap and high-value for discoverability. **Recommended secondary.** |
| Plain installer (MSI / Inno / NSIS) | Maximum control (firewall rules, per-machine service-like install). Needed if MSIX proves too restrictive for the firewall step. **Fallback.** |
| Microsoft Store | Later. Store policy and the local-network/background story need review first. **Deferred.** |

Architectures: **x64 primary, ARM64 secondary** (Windows on ARM is now mainstream enough to
matter, and `aarch64-pc-windows-msvc` is Tier 2). Both need signing.

Code signing is required for a serious Windows distribution: unsigned binaries get SmartScreen
warnings that will stop most users. This is a cost item, not a technical one, and it is shared
with the macOS notarization requirement → [19](19-PACKAGING-AND-DISTRIBUTION.md).

---

## 12. PoCs

| ID | Question |
| --- | --- |
| **POC-WIN-01** | Does the workspace compile for `x86_64-pc-windows-msvc`, and do the portable tests pass? Does `listener.rs`'s `IPV6_V6ONLY` probe behave sanely on Windows? |
| **POC-WIN-02** | Does `mdns-sd` advertise `_anyflow._tcp.local.` on Windows such that the existing Android app finds it and connects? Coexistence with the built-in responder on 5353. |
| **POC-WIN-03** | CNG + Microsoft Platform Crypto Provider: create a non-exportable ECDSA P-256 key, build a certificate around it, compute the SPKI fingerprint. |
| **POC-WIN-04** | `rustls-cng` `CngSigningKey` + `ResolvesClientCert`: complete a mutually-authenticated TLS 1.3 handshake against the existing Linux `anyflowd`, with SPKI pinning on both sides. |
| **POC-WIN-05** | `AddClipboardFormatListener` read/write/watch with loop suppression against the existing dedup logic. |
| **POC-WIN-06** | A user-session agent that survives lock, unlock, sleep, resume and fast user switching. |
| **POC-WIN-07** | WinUI 3 ↔ agent over a named pipe with a per-SID DACL. |
| **POC-WIN-08** | Firewall rules scoped Private + LocalSubnet; verify a Public network genuinely blocks inbound. |
| **POC-WIN-09** | MSIX package with `windows.startupTask`; install, start at login, update, uninstall, data removal. |

---

## 13. Recommended direction

1. **User-session agent.** No Windows Service. (**PLAT-DEC-002**)
2. **Reuse the portable Rust core** behind the Wave 0 seams; no second protocol implementation.
3. **`mdns-sd` first**, platform DNS-SD as fallback.
4. **TPM-backed identity via CNG + `rustls-cng`**, with an explicit, visible software fallback.
5. **`AddClipboardFormatListener`** for the watch — the trait fits.
6. **WinUI 3 over a named pipe.** No FFI.
7. **Private + LocalSubnet firewall rules**, created by the installer, removed on uninstall.
8. **MSIX + winget**, x64 and ARM64, signed.
9. **Extend `filename.rs` for Windows reserved names *before* any Windows build can receive a
   file.** This is the one item that is a security bug rather than a feature gap.
