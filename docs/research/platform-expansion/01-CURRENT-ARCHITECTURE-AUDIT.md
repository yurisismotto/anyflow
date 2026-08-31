# 01 — Current architecture audit

| Field | Value |
| --- | --- |
| **Title** | Current architecture audit: what is portable today, what is not |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Every subsystem in `desktop/`, `android/`, `protocol/`, `packaging/` as it exists on `develop` at commit `f7a0015`. |
| **Decision status** | No decisions. Findings only. |
| **Evidence** | REPO VERIFIED throughout, except where a line is tagged otherwise. No file in the repository was modified to produce this document. |
| **Related documents** | [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), [04](04-LINUX-PORTABILITY.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md), [15](15-CROSS-PLATFORM-CLIPBOARD.md), [17](17-BACKGROUND-EXECUTION-MODEL.md), [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |

---

## 1. Method

Every claim below was produced by reading the tracked sources, not by inference from the
documentation. The three mechanical sweeps that anchor the classification were:

```
grep -rn 'cfg(unix)|cfg(target_os|cfg(target_family|cfg(windows)' --include=*.rs desktop/
grep -rn 'std::os::unix|UnixListener|UnixStream|PermissionsExt|OpenOptionsExt' --include=*.rs desktop/
grep -rn 'env::var_os|env::var' --include=*.rs desktop/
```

**The first sweep returns nothing.** There is not one conditional-compilation attribute in
the entire Rust workspace. This is the single most important structural fact in this
document: AnyFlow does not currently *have* a platform boundary at the build level. It has
Linux code that happens to be the only code. Every portability finding below follows from
that.

The second sweep returns 24 hits across 6 non-test source files. The third returns 12.
That is a small, enumerable surface — which is the good news.

---

## 2. Subsystem classification

Classification vocabulary is the one the sprint brief defines. `PORTABLE CORE` means the
code compiles and behaves correctly on Windows, macOS and iOS as written.
`PLATFORM-ABSTRACTION-READY` means the code is *behind* a trait or an equivalent seam, so a
new platform is a new implementation rather than an edit.

### 2.1 Protocol and wire format

| Subsystem | Files | Class | Notes |
| --- | --- | --- | --- |
| `.proto` schemas | `protocol/proto/anyflow/v1/**` | **PORTABLE CORE** | Language-neutral. Compiled by `prost-build`+`protox` (Rust) and the protobuf Gradle plugin (Kotlin) from the *same* files. |
| Generated Rust types | `desktop/proto/src/lib.rs`, `desktop/proto/build.rs` | **PORTABLE CORE** | `protox` is a pure-Rust protobuf compiler, so the build needs no `protoc` binary and no C toolchain. See [ADR-0004](../../adr/ADR-0004-protocol-buffers.md). This is a real portability asset: a Windows or macOS build inherits it for free. |
| Framing | `desktop/core/src/framing.rs` (62 lines) | **PORTABLE CORE** | 4-byte big-endian length prefix over `tokio::io`. No platform surface. |
| Envelope / sequence / replay | `desktop/core/src/session.rs` | **PORTABLE CORE** | 1586 lines, no `std::os`, no filesystem, no environment. |

**Platform enum is the one protocol-level gap.** `protocol/proto/anyflow/v1/core.proto`
declares:

```protobuf
enum Platform {
  PLATFORM_UNSPECIFIED = 0;
  PLATFORM_ANDROID = 1;
  PLATFORM_LINUX = 2;
}
```

There is no value for Windows, macOS, iOS or iPadOS. Adding them is an additive proto3
enum change: an old peer decodes an unknown value as the raw integer and the field is
presentational only (it selects an icon; it is never an authorization input — see
`desktop/core/src/identity.rs` `device_info()` and `android/.../ui/UiMapping.kt`). This is
the *only* protocol change this research finds unavoidable, and it is the mildest kind
there is. Tracked as **PLAT-DEC-008** in [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md).

### 2.2 Security core

| Subsystem | Files | Class | Notes |
| --- | --- | --- | --- |
| TLS 1.3 + pinning | `desktop/core/src/tls.rs` (387 lines) | **PORTABLE CORE** | `rustls` 0.23 with the `ring` provider, `default-features = false`. No OpenSSL, no system trust store, no platform verifier. TLS13-only version list; both verifiers delegate to `rustls::crypto::verify_tls13_signature`. Compiles unchanged on every rustls-supported target. |
| SPKI fingerprint | `desktop/core/src/fingerprint.rs` (94 lines) | **PORTABLE CORE** | `sha2` + `x509-parser`, pure Rust. |
| Pairing proof | `desktop/core/src/pairing.rs` (274 lines) | **PORTABLE CORE** | HMAC-SHA256 with domain separation, `subtle` for constant-time comparison. |
| QR payload | `desktop/core/src/qr.rs` (121 lines) | **PORTABLE CORE** | |
| Capability registry | `desktop/core/src/capability.rs` (124 lines) | **PORTABLE CORE** | Trait-object registry keyed by string id. The transport contains no capability names. |
| Clipboard policy model | `desktop/core/src/clipboard_policy.rs` (226 lines) | **PORTABLE CORE** | Pure data + rules. |
| Discovery record model | `desktop/core/src/discovery.rs` (122 lines) | **PORTABLE CORE** | TXT build/parse and name sanitisation only. Contains no responder. |
| **Local identity** | `desktop/core/src/identity.rs` (190 lines) | **NEEDS-REFACTOR** | See §3.1. Compiles anywhere; *structurally* blocks hardware-backed keys. |
| **Store / trust store** | `desktop/core/src/store.rs` (380 lines) | **LINUX-SPECIFIC** | See §3.2. The only file in `anyflow-core` that fails to compile off-unix. |

`anyflow-core` is therefore **one file away** from compiling on Windows and macOS, and one
*design change* away from being able to hold a hardware-backed key. That is a much better
starting position than the "Fedora + Android" framing suggests.

### 2.3 Capabilities

| Subsystem | Files | Class | Notes |
| --- | --- | --- | --- |
| `battery.v1` protocol half | `desktop/capabilities/battery/src/lib.rs` | **PORTABLE CORE** | |
| `battery.v1` local source | `desktop/capabilities/battery/src/upower.rs` | **LINUX-SPECIFIC** | Already behind a Cargo feature (`upower = ["dep:zbus"]`, **off by default**) and behind a trait injected via `BatteryCapability::with_local_source`. **This is the template the rest of the codebase should copy.** A Windows or macOS battery source is a new optional dependency and a new impl; nothing above it changes. |
| `files.v1` state machine | `desktop/capabilities/files/src/{lib,transfer,stream,auth,limits}.rs` | **PORTABLE CORE** | 1833+427+363+280+130 lines with no platform surface. Data-stream MAC, filename rules, limits — all portable. |
| `files.v1` filename safety | `desktop/capabilities/files/src/filename.rs` (268 lines) | **NEEDS-REFACTOR** | Portable *code*, but its rule set is POSIX-shaped. See §3.4. |
| `files.v1` destination | `desktop/capabilities/files/src/destination.rs` (440 lines) | **LINUX-SPECIFIC** | `OpenOptionsExt::mode`, `PermissionsExt::from_mode`, `$XDG_DOWNLOAD_DIR`, `user-dirs.dirs`, `$HOME`. |
| `clipboard.v1` logic | `desktop/capabilities/clipboard/src/{lib,dedup,text,limits,policy,redact}.rs` | **PORTABLE CORE** | 943+326+272+110+40+53 lines. Loop suppression, dedup cache, text validation — no platform surface. |
| **`ClipboardBackend` trait** | `desktop/capabilities/clipboard/src/backend/mod.rs` (375 lines) | **PLATFORM-ABSTRACTION-READY** | The best-designed seam in the repository. Its own doc comment already says a macOS or Windows implementation would work unchanged above it. One caveat, in §3.3. |
| Wayland backend | `desktop/capabilities/clipboard/src/backend/wayland.rs` (538 lines) | **WAYLAND-SPECIFIC** | Drives `wl-copy`/`wl-paste` as child processes. |
| X11/XFIXES watch | `desktop/capabilities/clipboard/src/backend/x11.rs` (272 lines) | **X11-SPECIFIC** | Watch only; no read/write. Exists because Mutter implements no data-control protocol ([ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md)). |

### 2.4 Daemon

| Subsystem | Files | Class | Notes |
| --- | --- | --- | --- |
| TCP listener + dual-stack probe | `desktop/daemon/src/listener.rs` (291 lines) | **PORTABLE CORE (behaviour POC REQUIRED)** | Pure `std::net`/`tokio::net`. But `bind_endpoints` *infers* `IPV6_V6ONLY` by binding `[::]` then `0.0.0.0` and reading the error. Windows defaults and `SO_EXCLUSIVEADDRUSE` semantics differ from Linux's `net.ipv6.bindv6only`. Compiles; behaviour unverified → **POC-WIN-01**. |
| mDNS advertisement | `desktop/daemon/src/mdns.rs` (88 lines) | **PORTABLE CORE (POC REQUIRED)** | Uses `mdns-sd`, a pure-Rust responder that the upstream README states "supports macOS, Linux and Windows" (OFFICIAL-ISH: project README, not a distro/vendor doc). Coexistence with a *system* responder on 5353 is the open question → [13](13-CROSS-PLATFORM-DISCOVERY.md). |
| Session/peer state | `desktop/daemon/src/state.rs` (467 lines) | **PORTABLE CORE** | |
| **Control socket transport** | `desktop/daemon/src/server.rs` (1017 lines) | **LINUX-SPECIFIC** | `tokio::net::{UnixListener, UnixStream}` + `PermissionsExt` chmod on the socket path. |
| **Control socket path/uid** | `desktop/daemon/src/control.rs` (379 lines) | **LINUX-SPECIFIC** | `$XDG_RUNTIME_DIR`, `/tmp/anyflow-<uid>` fallback, and `nix_uid()` which parses `/proc/self/status`. The request/response *types* in the same file are portable. |
| Process entry point | `desktop/daemon/src/main.rs` (305 lines) | **LINUX-SPECIFIC** | `tracing_subscriber` configured `.without_time()` "because journald adds its own". Assumes a `systemd --user` supervisor. |

### 2.5 CLI, GUI, packaging

| Subsystem | Files | Class | Notes |
| --- | --- | --- | --- |
| `anyflow` CLI | `desktop/cli/src/main.rs` (684 lines) | **LINUX-SPECIFIC** | `UnixStream::connect` only. Command surface itself is portable. |
| GUI shell + views | `desktop/gui/src/**` (~2500 lines) | **GTK-SPECIFIC** | GTK4 + libadwaita. |
| GUI ↔ daemon client | `desktop/gui/src/client.rs` (203 lines) | **LINUX-SPECIFIC** | Same `UnixStream` dependency as the CLI. |
| GUI resource build | `desktop/gui/build.rs` | **LINUX-SPECIFIC (build-time)** | `glib_build_tools::compile_resources` shells out to `glib-compile-resources`. |
| RPM spec | `packaging/fedora/anyflow.spec` | **FEDORA-SPECIFIC** | Also: it packages `anyflowd` and `anyflow` **only**. The GUI binary, a `.desktop` entry, an icon and an autostart entry are **not packaged at all**. |
| systemd user unit | `packaging/fedora/anyflowd.service` | **LINUX-SPECIFIC** | Heavily hardened (`SystemCallFilter`, `RestrictAddressFamilies`, `ProtectSystem=strict`). Nothing equivalent exists on any other platform; see [17](17-BACKGROUND-EXECUTION-MODEL.md). |

### 2.6 Android (reference implementation)

Android is not a port of the Rust core; it is an independent implementation of the same
protocol in Kotlin. That is deliberate ([ADR-0002](../../adr/ADR-0002-android-native-kotlin.md)) and it
is the most important data point this audit has, because **it proves the protocol is
implementable twice without sharing a line of transport code.**

| Subsystem | File | Notes |
| --- | --- | --- |
| Identity | `identity/DeviceIdentity.kt` | Android Keystore, StrongBox-then-TEE, **key never leaves the TEE**. `KEY_ALIAS = "anyflow-identity-v2"`; v1 keys are deleted because they lacked `DIGEST_NONE` and were unusable for TLS client auth. |
| Pinning | `net/PinnedTrustManager.kt` | The Kotlin twin of `PinnedServerCertVerifier`. |
| Framing / protocol | `net/Framing.kt`, `net/Protocol.kt` | Twins of `framing.rs` / `session.rs`. |
| Discovery | `net/Discovery.kt` | `NsdManager` + `WifiManager.MulticastLock`. |
| Background | `service/ConnectionService.kt` | `connectedDevice` foreground service. |
| Clipboard | `clipboard/*.kt` (7 files) | `ClipboardManager`, `EXTRA_IS_SENSITIVE`, QS tile that *cannot* read the clipboard itself and therefore opens the Activity. |

The cost of that duplication is visible: `Framing`, `Protocol`, `PairingProof`,
`Fingerprint`, `KeyDigests`, `TrustStore`, `ClipboardPolicy` and `Filenames` all exist
twice, and each has its own test file. Whether that cost should be paid a third, fourth and
fifth time is the central question of [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md).

---

## 3. The portability blockers, precisely

### 3.1 BLOCKER-01 — the identity key is a `Vec<u8>` in process memory

`desktop/core/src/identity.rs`:

```rust
pub struct LocalIdentity {
    …
    key_pkcs8_der: Vec<u8>,
    …
}

pub(crate) fn rustls_private_key(&self) -> PrivateKeyDer<'static> {
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_pkcs8_der.clone()))
}
```

and `tls.rs` consumes it through `with_single_cert(...)` / `with_client_auth_cert(...)`.

This is correct for a software key and **structurally incompatible with every
hardware-backed keystore**, because those keystores exist precisely to make
`key_pkcs8_der` unobtainable. A TPM key, a Secure Enclave key and an Android Keystore key
can all *sign*; none of them can be serialised to PKCS#8.

Android already dodges this by not using Rust. Windows and macOS cannot dodge it if they
use the Rust core.

The fix is known and does not touch the protocol: rustls exposes
`rustls::sign::SigningKey` / `rustls::sign::Signer` and
`client::ResolvesClientCert` / `server::ResolvesServerCert` exactly for keys that live in
"an HSM, or another process, or another machine" (OFFICIAL DOC VERIFIED —
`rustls::manual::_03_howto`). The `rustls-cng` crate (published under the **rustls**
GitHub organisation, v0.7.1, targeting rustls ^0.23 — the version this repo pins) already
implements it against Windows CNG, including ECDSA on secp256r1.

Severity: **this is the item that decides how much of Wave 0 there is.** Tracked as
**PLAT-DEC-001** and **PLAT-DEC-004**.

### 3.2 BLOCKER-02 — `store.rs` is the only non-portable file in `anyflow-core`

Three separable problems, in decreasing severity:

1. **Unix mode enforcement.** `harden_dir` (0700) and `require_private_mode` (refuse to
   load a key with mode `& 0o077`) are a *security control*, not incidental. Windows and
   macOS need an equivalent, not a `#[cfg]`-away. On Windows the equivalent is a DACL
   granting only the owning user; there is no `0o700`. Removing the check on Windows would
   be a silent security regression — explicitly forbidden by the sprint's security
   principle.
2. **`write_atomic`** uses `OpenOptionsExt::mode` to create the temp file *already* at the
   final mode, closing the world-readable window. The Windows equivalent must create the
   file with a security descriptor, not chmod after the fact.
3. **Path and hostname.** `default_data_dir()` is `$XDG_DATA_HOME/anyflow` else
   `~/.local/share/anyflow`. `default_device_name()` reads `/etc/hostname` and falls back
   to the literal string `"Fedora"` — which will be a visibly wrong device name on any
   other OS, including other Linux distributions.

### 3.3 BLOCKER-03 — `ClipboardBackend::watch_changes` forbids polling

`backend/mod.rs` states, as a contract on implementors:

> `Err(Unavailable)` is a normal answer … **No implementation may satisfy this by polling.**

On Linux that rule is right and well-argued (ADR-0014). On **macOS it is unsatisfiable**:
AppKit's only public change signal for the general pasteboard is `NSPasteboard.changeCount`,
an integer you compare against the last value you saw. There is no public notification
(OFFICIAL DOC VERIFIED for `changeCount`'s existence and semantics; the *absence* of a
notification API is an argument from the absence of one in AppKit — see
[10](10-MACOS-FEASIBILITY.md), where it is tagged POC REQUIRED rather than asserted).

So either macOS never gets automatic clipboard send, or the contract is amended to
"no implementation may poll *unless the platform offers nothing else, and it declares that
it is polling*". The second is the honest option and costs nothing on Linux. Tracked as
**PLAT-DEC-009**.

### 3.4 BLOCKER-04 — filename sanitisation is POSIX-shaped

`filename.rs` strips path separators and rejects `.`/`..`. It does not know about
Windows-reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`),
about trailing dots and spaces being stripped by the Win32 layer, about `:` opening an
alternate data stream, or about `\` being a separator. A peer sending `CON` or
`report.txt.` or `a:b` is not currently a Linux problem and *is* a Windows one. This is a
security item, not a cosmetic one → [20](20-SECURITY-THREAT-ANALYSIS.md), **SEC-004**.

### 3.5 BLOCKER-05 — local IPC is a Unix domain socket with Unix semantics

`server.rs` binds a `UnixListener` and chmods it; `control.rs` places it under
`$XDG_RUNTIME_DIR` (already 0700 per-user) and derives the uid from `/proc/self/status`.
The CLI and GUI both connect with `tokio::net::UnixStream`.

Windows *has* `AF_UNIX` since Windows 10 build 17063 — but with no ancillary data, so
there is no `SO_PEERCRED` equivalent and no credential passing (OFFICIAL DOC VERIFIED,
Microsoft "AF_UNIX comes to Windows"). And `tokio::net::UnixStream` is `#[cfg(unix)]`
regardless. So this needs a real abstraction with a Named Pipe implementation, not a
recompile.

### 3.6 BLOCKER-06 — `default_download_dir()` is XDG-only

`$XDG_DOWNLOAD_DIR` → `user-dirs.dirs` → `$HOME/Downloads`. On Windows the answer is
`FOLDERID_Downloads` via `SHGetKnownFolderPath`; on macOS it is
`FileManager.urls(for: .downloadsDirectory)`; on iOS there is no such thing at all and the
concept has to be replaced (see [16](16-CROSS-PLATFORM-FILES.md)).

### 3.7 BLOCKER-07 — there is no build-level platform boundary

Because there is not one `cfg(target_os)` in the tree, none of the above can be fixed
"where it is". Fixing them in place would scatter `#[cfg]` through
`store.rs`, `destination.rs`, `server.rs`, `control.rs`, `main.rs`, `client.rs` and the CLI
— seven files, several of them security-critical, each acquiring two untested branches.
That is exactly the outcome [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) is written to
avoid.

---

## 4. Dependency map

```
                       protocol/proto/**  ────────────────┐
                              │                            │
                     (prost-build + protox)      (protobuf-gradle-plugin)
                              │                            │
                       anyflow-proto                  Kotlin protos
                              │                            │
   ┌──────────────────────────┴─────────┐                  │
   │        anyflow-core                │                  │
   │  framing tls session pairing qr    │            android/app
   │  capability fingerprint discovery  │        (independent Kotlin
   │  clipboard_policy                  │         implementation of the
   │  ────────────────────────────────  │         same protocol)
   │  store.rs        ← LINUX-SPECIFIC  │
   │  identity.rs     ← NEEDS-REFACTOR  │
   └──────┬─────────────┬───────────────┘
          │             │
   capability crates    │
   ┌──────────────┐     │
   │ battery      │ upower.rs ← LINUX (feature-gated, trait-injected) ✅ good pattern
   │ files        │ destination.rs ← LINUX
   │ clipboard    │ backend/{wayland,x11}.rs ← LINUX (behind ClipboardBackend) ✅ good pattern
   └──────┬───────┘
          │
   anyflow-daemon
     listener.rs   PORTABLE (behaviour unverified off-Linux)
     mdns.rs       PORTABLE (mdns-sd: Linux/macOS/Windows)
     state.rs      PORTABLE
     control.rs    types PORTABLE / paths LINUX
     server.rs     LINUX (UnixListener)
     main.rs       LINUX (systemd assumptions)
          │
     ┌────┴────┐
  anyflow-cli  anyflow-gui
   UnixStream   UnixStream + GTK4/libadwaita
```

Two things stand out.

**First**, the GUI depends on `anyflow-daemon` (`default-features = false`) purely to reuse
the control-protocol types — deliberately, so "the GUI cannot drift from the socket
contract". That is a good decision that becomes a portability problem the moment
`anyflow-daemon` stops compiling on the target: the GUI inherits every one of the daemon's
Linux dependencies just to get some `serde` structs. Splitting the control *types* out of
`anyflow-daemon` is cheap and unblocks a lot.

**Second**, the arrows into `anyflow-core` are all *upward*. Nothing in core depends on
the daemon, the CLI or the GUI, and `SessionHost` (`core/src/session.rs:130`) is a proper
inversion point: `local_device_info`, `registry`, `lookup_peer`, `pairing_mode_active`,
`verify_pairing_proof`, `confirm_pairing`, `store_peer`, `on_established`, `on_closed`.
The whole protocol is testable in-process because of it. **A second platform can implement
`SessionHost` without touching core.** That is the single biggest asset in the codebase for
this expansion.

---

## 5. What exactly stops the core compiling on Windows / macOS / iOS

Compile-blocking items only. Behavioural gaps are in §3 and in the per-platform documents.

| # | Item | File | Win | macOS | iOS |
| --- | --- | --- | :-: | :-: | :-: |
| 1 | `std::os::unix::fs::OpenOptionsExt` | `core/src/store.rs:334` | ✗ | ✓ | ✓ |
| 2 | `std::os::unix::fs::PermissionsExt` ×2 | `core/src/store.rs:352,367` | ✗ | ✓ | ✓ |
| 3 | `OpenOptionsExt` / `PermissionsExt` ×4 | `capabilities/files/src/destination.rs:34,66,159,263` | ✗ | ✓ | ✓ |
| 4 | `tokio::net::UnixListener/UnixStream` | `daemon/src/server.rs:9` | ✗ | ✓ | ✓ |
| 5 | `tokio::net::UnixStream` | `cli/src/main.rs:15`, `gui/src/client.rs:28` | ✗ | ✓ | ✓ |
| 6 | `zbus` (D-Bus) | `capabilities/battery/src/upower.rs` | ✗ | ✗ | ✗ | 
| 7 | `x11rb` | `capabilities/clipboard/src/backend/x11.rs` | ✗ | ✗ | ✗ |
| 8 | `gtk4` / `libadwaita` / `glib-build-tools` | `gui/**` | ✗ | ~ | ✗ |

Item 6 is already `optional = true` and off by default → not a blocker, just a
`--no-default-features` away.
Item 7 is **not** optional today; `x11rb` is an unconditional dependency of the clipboard
crate. Making it optional is a one-line manifest change plus a `cfg`.
Item 8 only matters for the GUI, which no non-Linux platform will use.

**Net result:** with `store.rs`, `destination.rs`, the control-socket transport, and the
`x11rb` feature gate addressed, `anyflow-proto`, `anyflow-core`, all three capability
crates and most of `anyflow-daemon` compile on `x86_64-pc-windows-msvc` (Rust **Tier 1**)
and `aarch64-apple-darwin` / `aarch64-apple-ios` (Rust **Tier 2**, both rustup-distributed
— OFFICIAL DOC VERIFIED, *The rustc book*, Platform Support).

That is a genuinely small list. It is also why this research recommends doing that work
**once, deliberately, in a Wave 0** rather than as a side effect of the first Windows
sprint.

---

## 6. Runtime assumptions that no `cfg` will find

These compile fine and are wrong anyway. They matter more than §5.

| Assumption | Where | Reality elsewhere |
| --- | --- | --- |
| A supervisor restarts us (`Restart=on-failure`) | `anyflowd.service` | Windows: SCM or nothing. macOS: `launchd` via `SMAppService`. iOS: nothing — the app is killed and not restarted. |
| Logs go to journald, so timestamps are redundant | `daemon/src/main.rs` `.without_time()` | Windows Event Log / a file; macOS `os_log`. Both want timestamps or structured fields. |
| The device name is `/etc/hostname`, else `"Fedora"` | `core/src/store.rs:108` | Windows `GetComputerNameEx`; macOS `SCDynamicStoreCopyComputerName`; iOS `UIDevice.name` (privacy-restricted since iOS 16 — returns a generic model name without an entitlement). |
| A graphical session exists and is identified by `WAYLAND_DISPLAY`/`DISPLAY` | `clipboard/backend/mod.rs:176-185` | Meaningless off Linux. |
| `wl-copy`/`wl-paste` are on `PATH` | `backend/wayland.rs` | No external helper exists or should exist on Windows/macOS. |
| One user per machine, and the socket's directory mode is the whole access control | `daemon/src/control.rs` | Windows fast-user-switching runs several interactive sessions at once → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md). |
| The process runs continuously once started | everywhere | False on iOS by design, and false on Android without the foreground service the app already has. |

---

## 7. What is already genuinely multi-platform

Stated plainly, because the roadmap depends on it:

- **The protocol.** Wire format, framing, envelope, pairing proof, capability model.
  Proven by two independent implementations that interoperate.
- **The security model.** TLS 1.3 + SPKI pinning + explicit pairing + per-capability
  grants. `rustls` needs no system TLS stack; the trust decisions are ours.
- **The cryptographic primitive choice.** ECDSA **P-256** was chosen for Android Keystore
  compatibility ([ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md)). That choice
  turns out to be exactly right for every remaining platform: the Apple Secure Enclave
  supports **only** 256-bit elliptic-curve keys (OFFICIAL DOC VERIFIED,
  `kSecAttrTokenIDSecureEnclave`), and Windows CNG KSPs list ECDSA P-256 first.
  **AnyFlow made the one crypto decision that makes hardware-backed identity possible on
  all five platforms, before knowing it would need to.**
- **Discovery on the wire.** `_anyflow._tcp.local.` with a documented TXT schema
  (`v`, `pv`, `id`, `dn`) is plain DNS-SD; every platform has a stack for it.
- **The capability plugin model.** Adding a platform's partial support is not a protocol
  change.
- **`SessionHost`.** The host-inversion trait that makes a second host implementation a
  normal amount of work.

---

## 8. Findings summary

| ID | Finding | Class | Impact |
| --- | --- | --- | --- |
| AUD-01 | No `cfg(target_os)` anywhere in the workspace | Structural | Wave 0 must create the boundary |
| AUD-02 | `LocalIdentity` owns raw PKCS#8 bytes | NEEDS-REFACTOR | Blocks TPM / Secure Enclave |
| AUD-03 | `store.rs` is the only non-portable core file | LINUX-SPECIFIC | Small, security-sensitive |
| AUD-04 | `ClipboardBackend` is a clean seam | ABSTRACTION-READY | Reuse as-is |
| AUD-05 | `watch_changes` bans polling; macOS can only poll | Contract conflict | Amend contract |
| AUD-06 | Control socket is UDS + unix modes | LINUX-SPECIFIC | Needs an IPC trait |
| AUD-07 | `filename.rs` has no Windows reserved-name rules | Security gap | Fix before any Windows receive |
| AUD-08 | `battery`/`upower` feature-gating is the right pattern | Positive | Copy it |
| AUD-09 | GUI depends on `anyflow-daemon` for types only | Coupling | Split control types out |
| AUD-10 | `Platform` proto enum lacks Windows/macOS/iOS | Protocol gap | Additive, backward-compatible |
| AUD-11 | RPM packages neither the GUI nor a `.desktop` file | Packaging gap | Pre-existing Linux debt |
| AUD-12 | `mdns-sd` already claims Linux/macOS/Windows | Positive | Verify, don't replace |
| AUD-13 | Device-name fallback is the literal `"Fedora"` | Cosmetic-but-visible | Trivial |
