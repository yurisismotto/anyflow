# 21 — PoC master plan

| Field | Value |
| --- | --- |
| **Title** | Small, independent proofs of concept that unblock decisions |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Every PoC this research says is required before a decision can be made or a wave can start. |
| **Decision status** | PROPOSED. **No PoC in this document has been implemented and none may be implemented in this sprint.** |
| **Evidence** | n/a — this document *creates* evidence requirements. |
| **Related documents** | [22](22-IMPLEMENTATION-ROADMAP.md), [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md), and the per-platform documents each PoC cites |

---

## 1. Rules for every PoC here

1. **Small and independent.** One question each. A PoC that needs another PoC to finish first is
   two PoCs, unless the dependency is explicit below.
2. **Throwaway.** PoC code is not the implementation and must not be merged into `develop`. It
   lives on its own branch or in a scratch repository.
3. **A PoC that proves the pieces separately proves nothing.** The identity PoCs in particular
   must end in a completed handshake **against the existing Linux `omnibridged`**, not against a
   test double. This rule exists because OmniBridge has already been bitten by exactly that class
   of error: Android v1 keystore keys were generated without `DIGEST_NONE` and were "unusable
   for TLS client authentication" — a fact only a real handshake would have revealed, and by
   then the authorisations were immutable.
4. **Failure is a result.** A PoC that fails has done its job. Record the failure in the result
   placeholder and let the decision it feeds change.
5. **Security is a first-class criterion.** A PoC that "works" by disabling pinning, exporting a
   key, or accepting an unauthenticated peer has failed.
6. `daemon/examples/fake_phone.rs` already exists as a scriptable protocol client and is the
   default counterparty for desktop-side PoCs.

---

## 2. Summary

| ID | Title | Blocks | Duration | Env |
| --- | --- | --- | --- | --- |
| POC-CORE-01 | Cross-compile the portable crates | Wave 0 exit | 2 d | Linux |
| POC-CORE-02 | `IdentitySigner` seam, behaviour-preserving | PLAT-DEC-001 | 3 d | Linux |
| POC-CORE-03 | `ControlTransport` seam | Wave 0 exit | 2 d | Linux |
| POC-LINUX-01 | Debian 13 build + run | Wave 2 | 1 d | Debian VM |
| POC-LINUX-02 | Ubuntu 24.04 + 26.04 build + run | Wave 2 | 1 d | Ubuntu VMs |
| POC-LINUX-03 | Flatpak viability | PLAT-DEC-007 | 3 d | Fedora |
| POC-LINUX-04 | Linux TPM2 signer | LINUX-007 | 3 d | Fedora + TPM |
| POC-KDE-01 | Plasma Wayland clipboard | Wave 3 | 1 d | Plasma VM |
| POC-KDE-02 | GTK GUI on Plasma | Wave 3 | 1 d | Plasma VM |
| POC-WIN-01 | Compile + bind on Windows | Wave 5 | 2 d | Win 11 |
| POC-WIN-02 | DNS-SD discovery | Wave 5 | 2 d | Win 11 + Android |
| POC-WIN-03 | CNG/TPM P-256 identity | PLAT-DEC-001 | 3 d | Win 11 + TPM |
| POC-WIN-04 | TLS + SPKI pinning via `rustls-cng` | Wave 5 | 2 d | Win 11 + Linux |
| POC-WIN-05 | Clipboard listener + loop suppression | Wave 6 | 2 d | Win 11 |
| POC-WIN-06 | User-session agent lifecycle | PLAT-DEC-002 | 3 d | Win 11, 2 users |
| POC-WIN-07 | WinUI ↔ agent named pipe | PLAT-DEC-007 | 3 d | Win 11 |
| POC-WIN-08 | Firewall Private/LocalSubnet | Wave 6 | 1 d | Win 11 |
| POC-WIN-09 | MSIX package lifecycle | Wave 6 | 2 d | Win 11 |
| POC-MAC-01 | Compile + test on macOS | Wave 7 | 1 d | Apple Silicon |
| POC-MAC-02 | Bonjour interoperability | Wave 7 | 2 d | Mac + Android |
| POC-MAC-03 | Secure Enclave P-256 identity | PLAT-DEC-004 | 3 d | Apple Silicon |
| POC-MAC-04 | TLS + pinning via a custom signer | PLAT-DEC-004 | 3 d | Mac + Linux |
| POC-MAC-05 | `NSPasteboard` sync | PLAT-DEC-009 | 2 d | Mac |
| POC-MAC-06 | `SMAppService` login agent | Wave 7 | 2 d | Mac |
| POC-MAC-07 | SwiftUI ↔ agent IPC | Wave 8 | 2 d | Mac |
| POC-IOS-01 | Bonjour discovery | Wave 9 | 1 d | iPhone/iPad |
| POC-IOS-02 | Local network permission | Wave 9 | 1 d | iPhone |
| POC-IOS-03 | Pairing | Wave 9 | 2 d | iPhone + Linux |
| POC-IOS-04 | UniFFI + Enclave TLS identity | PLAT-DEC-005 | 4 d | iPhone + Linux |
| POC-IOS-05 | Foreground lifecycle | Wave 9 | 2 d | iPhone |
| POC-IOS-06 | **Background suspension measurement** | **PLAT-DEC-005** | 2 d | iPhone, real device |
| POC-IOS-07 | Clipboard manual send/receive | Wave 9 | 1 d | iPhone |
| POC-IOS-08 | Share Extension | Wave 9 | 2 d | iPhone |
| POC-IOS-09 | Files send/receive | Wave 9 | 2 d | iPhone |
| POC-DISC-01 | Five-platform discovery interop | Wave 5+ | 2 d | full lab |

**Total ≈ 71 engineer-days**, spread across waves. Not a single sprint.

Durations are HYPOTHESIS. They assume the environment already exists; setting up a Windows VM
with a working TPM, or an Apple developer account, is not counted.

---

## 3. The three PoCs that decide the most

If only three were run, these are they:

| ID | Why |
| --- | --- |
| **POC-IOS-06** | Decides whether an iOS product is worth building at all (PLAT-DEC-005). Cheap, needs only a device and a Linux daemon, and is the only way to learn what backgrounding actually costs |
| **POC-WIN-04** | Proves the "portable Rust core + hardware key" architecture end to end on the first new platform. If it fails, [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) needs rethinking before any platform work starts |
| **POC-KDE-01** | Cheapest high-value result in the whole plan. One afternoon on a Plasma VM turns KDE from "probably fine" into a certified platform, or into a known, specific problem with a named cause |

---

## 4. The PoCs

Each entry: **Q** question · **H** hypothesis · **E** environment · **S** scope · **✓** success ·
**✗** failure · **🔒** security · **⏱** duration · **⇢** dependency · **R** result.

---

> **⚠ UPDATED by the verification sprint (2026-08-31).** Two PoCs added (**POC-CORE-04**,
> **POC-LINUX-05**), four narrowed or requestioned, and all 37 reprioritised into
> P0/P1/P2/P3 classes in [27 §7](27-ARCHITECTURE-DECISION-CLOSEOUT.md).
>
> **The headline result: only four PoCs are P0, all four are Wave 0's own acceptance gates, and
> none of them blocks Wave 0 from starting.** Research v1's framing — 35 PoCs, ≈71 engineer-days,
> none implemented — read as if the whole programme were gated on them. It is not.
>
> | Change | PoC |
> | --- | --- |
> | **Narrowed** — its central question is answered from primary sources (V-01, V-02); what remains is the Xwayland fallback on Plasma | POC-KDE-01 |
> | **Split** — `mdns-sd` coexistence (V-12) stays P1; the `DnsServiceRegister` A/AAAA half (V-03) becomes contingent, run only if `mdns-sd` fails | POC-WIN-02 |
> | **Requestioned** — no longer "does anything prompt?" (V-06 says yes, by default, from macOS 15.4) but *"does `changeCount` polling **alone** trigger the access alert, or only the subsequent content read?"* | POC-MAC-05 |
> | **Narrowed** — TN3179 answers most of it; what remains is whether the listen-only path avoids the prompt, and whether the multicast entitlement is needed for a single declared service type | POC-IOS-02 |
> | **NEW** — `cargo check --target x86_64-pc-windows-msvc` on a **Windows runner** for the portable crates (a Linux cross-compile cannot work: `ring` needs MSVC, V-10) · **PASS 2026-09-01** | **POC-CORE-04** |
> | **NEW** — confirm on real Debian 13 / Ubuntu 24.04 / 26.04 that `wl-copy --sensitive` fails as predicted, and that the probe detects it | **POC-LINUX-05** |
>
> No PoC is deleted.

### POC-CORE-01 — Cross-compile the portable crates

- **Q** After the Wave 0 seams exist, do `omnibridge-proto`, `omnibridge-core`, the three capability crates and `omnibridge-runtime` compile for a non-Unix target from a Linux host?
- **H** Yes, once `store.rs`, `destination.rs`, the control transport and the `x11rb` feature gate are addressed ([01 §5](01-CURRENT-ARCHITECTURE-AUDIT.md)).
- **E** Linux + `rustup target add x86_64-pc-windows-gnu` (cheaper proxy than `-msvc`, which needs MSVC libraries).
- **S** Build only. No Windows implementations — the platform trait impls may be `unimplemented!()` stubs.
- **✓** `cargo build --target x86_64-pc-windows-gnu -p …` succeeds for all six crates; the full Linux test suite still passes with zero behavioural change.
- **✗** A portable crate still needs a platform impl to compile → the seam is in the wrong place.
- **🔒** No security control may be `#[cfg]`-ed away to make this pass. If a check has no Windows equivalent yet, it becomes a `todo!()` in the stub, not a deletion.
- **⏱** 2 d · **⇢** ARCH-002/003/004/005/006 · **R** **PASS — 2026-08-31.** All six portable crates *built* (not merely checked) for `x86_64-pc-windows-gnu` from Fedora, `ring` included, via `mingw64-gcc` and Fedora's `rust-std-static-x86_64-pc-windows-gnu` in a scratch sysroot. No security control was `#[cfg]`-ed away and no stub was written: the platform code is behind a Cargo feature. The first attempt failed usefully — Cargo feature unification switched `unix-fs` back on through a capability crate's default dependency on `omnibridge-core`, which a grep-only gate would have missed. [Evidence](../../reports/foundation/wave-0-platform-abstraction.md)

### POC-CORE-02 — `IdentitySigner` seam is behaviour-preserving

- **Q** Can `LocalIdentity` be refactored behind a signer trait with rustls's `ResolvesClientCert`/`ResolvesServerCert`, with no change to Linux behaviour?
- **H** Yes. rustls documents this as the supported extension point.
- **E** Linux.
- **S** Introduce the trait; make the existing file-backed key one implementation; route `tls.rs` through the resolvers.
- **✓** Every existing test passes unchanged, including `core/tests/identity_and_store.rs` (which asserts a 0644 key is refused) and the full `daemon/tests/e2e.rs`. Interop with the shipping Android app still works.
- **✗** rustls's resolver path changes handshake behaviour, or the mode check cannot be expressed on the trait.
- **🔒** `require_private_mode`'s hard failure must survive verbatim. This is the single highest-risk refactor in Wave 0 because it touches the code that decides identity.
- **⏱** 3 d · **⇢** none · **R** **PASS — 2026-08-31.** Every existing test passes unmodified, `require_private_mode`'s hard failure verbatim included. Beyond the success criterion, four complete pinned TLS 1.3 handshakes now run from a provider holding no key bytes: server-side, client-side, both-ends non-exportable, and a rejection case asserting the failure comes from `PinnedServerCertVerifier` during the handshake. **Android interop is not covered** — no device was attached (G6). [Evidence](../../reports/foundation/wave-0-platform-abstraction.md)

### POC-CORE-03 — `ControlTransport` seam

- **Q** Can the control socket be abstracted so a named-pipe implementation is possible, without changing the CLI/GUI protocol?
- **H** Yes — `server.rs` is ~1000 lines of which only the accept loop is Unix-specific.
- **E** Linux.
- **S** Trait over listener/stream; UDS implementation; extract control types into their own crate so `omnibridge-gui` stops depending on `omnibridge-daemon`.
- **✓** `daemon/tests/control.rs` passes unchanged; the GUI builds without the daemon crate.
- **✗** The protocol turns out to depend on Unix semantics (it should not — it is newline-delimited JSON).
- **🔒** The new crate must not weaken the "local only, never reachable from the network" property stated in `control.rs`.
- **⏱** 2 d · **⇢** none · **R** **PASS — 2026-08-31.** `daemon/tests/control.rs` passes unmodified; `cargo tree -p omnibridge-gui | grep -c omnibridge-daemon` → 0, likewise for the CLI. The trait carries the `BindError::AlreadyOwned` contract the Windows named-pipe mitigation needs, and the Linux implementation honours it. A latent defect was fixed on the way: `bind` used to unlink a live socket. [Evidence](../../reports/foundation/wave-0-platform-abstraction.md)

---

### POC-LINUX-01 — Debian 13 trixie

- **Q** Does OmniBridge build from archive packages and run on Debian stable?
- **H** Yes. rustc 1.85 ≥ 1.82; GTK 4.18.6 ≥ 4.12; libadwaita 1.7.6 ≥ 1.5 (OFFICIAL DOC VERIFIED).
- **E** Debian 13 VM, GNOME Wayland.
- **S** Build daemon, CLI and GUI. Pair with a phone. Send a file. Sync a clipboard. Check Avahi coexistence and the default firewall.
- **✓** All four work; `omnibridge clipboard status` reports a working watch source.
- **✗** Any build failure from archive packages; mDNS not visible to the phone.
- **🔒** Confirm `require_private_mode` behaves identically on a different filesystem/umask.
- **⏱** 1 d · **⇢** none · **R** _pending_

### POC-LINUX-02 — Ubuntu 24.04 LTS and 26.04 LTS

- **Q** Same, on both Ubuntu LTS releases — and does the **libadwaita 1.5.0 floor** actually hold on noble?
- **H** Yes on both, but 24.04 needs a `rustc-1.82`+ package rather than the default `rustc` 1.75, and its libadwaita 1.5.0 is exactly at our floor with zero margin.
- **E** Ubuntu 24.04 and 26.04 VMs, GNOME Wayland.
- **S** As POC-LINUX-01, plus: confirm the GUI links and runs against libadwaita **1.5.0** specifically.
- **✓** Both build and run; the noble GUI starts and the pairing `AdwAlertDialog` renders.
- **✗** A libadwaita symbol newer than 1.5 is required → the floor has silently moved and CI must pin it (**CI-003**).
- **🔒** —
- **⏱** 1 d · **⇢** none · **R** _pending_

### POC-LINUX-03 — Flatpak viability

- **Q** Can OmniBridge work as a Flatpak, and at what permission cost?
- **H** Networking works with `--share=network` (OmniBridge does not use `.local` NSS resolution, which is the documented Flatpak gap); the blockers are `--socket=x11` on GNOME and a second-class CLI.
- **E** Fedora + GNOME Wayland, `flatpak-builder`.
- **S** Manifest bundling `wl-clipboard`; `--share=network --socket=wayland --socket=x11 --filesystem=xdg-download`; Background portal autostart.
- **✓** mDNS visible to a phone; clipboard read/write/watch works; a file lands in the real Downloads folder; the key-mode check passes in `~/.var/app`.
- **✗** Multicast on 5353 does not work inside the sandbox, or the XFIXES watch cannot reach Xwayland.
- **🔒** Note every permission required; a Flatpak needing `--socket=x11` + `--share=network` + `--filesystem=` is close to unsandboxed and should be described that way.
- **⏱** 3 d · **⇢** none · **R** _pending_

### POC-LINUX-04 — Linux TPM2 signer

- **Q** Can an unprivileged Linux user hold a non-exportable P-256 identity key in a TPM 2.0 and use it for TLS?
- **H** Technically yes via `tss-esapi` or `tpm2-pkcs11`; the practical obstacle is unprivileged access to `/dev/tpmrm0` (group membership), which may make it undeployable for OmniBridge's "no root" model.
- **E** Fedora with TPM 2.0.
- **S** Key generation, certificate around the public key, a `SigningKey`, a handshake.
- **✓** Handshake completes against the existing daemon; the whole flow works as a non-root user with no manual sysadmin step.
- **✗** Root or manual group configuration is required → the feature is not deployable as-is; document and defer.
- **🔒** Must not require loosening TPM device permissions system-wide.
- **⏱** 3 d · **⇢** POC-CORE-02 · **R** _pending_

---

### POC-KDE-01 — Plasma Wayland clipboard

- **Q** On Plasma, which watch source does `detect_watch_source()` pick, and does auto-send work — including on Ubuntu 26.04 LTS with wl-clipboard 2.2.1?
- **H** `WatchSource::DataControl` on a distro with wl-clipboard ≥ 2.3. On Ubuntu LTS (2.2.1) it may fail if KWin has dropped `wlr-data-control` in favour of `ext-data-control` ([05 §5.3](05-DEBIAN-UBUNTU-COMPATIBILITY.md)).
- **E** Two VMs: Fedora KDE (newer wl-clipboard) and Kubuntu 26.04 (2.2.1). Plasma Wayland.
- **S** Run the existing daemon. Record `omnibridge clipboard status`. Copy on the desktop → does the phone receive? Copy on the phone → does the desktop apply? Then test the XFIXES fallback by forcing it.
- **✓** DataControl selected on at least one distro; auto-send works both ways; `wl-copy --sensitive` causes Klipper to skip the entry.
- **✗** Neither DataControl nor XFIXES works on Plasma → KDE has no auto-send and needs mitigation (b) or (c) from [06 §4](06-KDE-PLASMA-WAYLAND.md).
- **🔒** Confirm **PRIMARY is never touched**, including with Klipper's clipboard↔selection sync enabled — and record what happens when it *is* enabled ([06 §5](06-KDE-PLASMA-WAYLAND.md)).
- **⏱** 1 d · **⇢** none · **R** _pending_

### POC-KDE-02 — GTK GUI on Plasma

- **Q** Does the GTK4/libadwaita GUI behave acceptably on Plasma?
- **H** Runs correctly; looks foreign. Dark-mode following and the file chooser depend on `xdg-desktop-portal-kde`.
- **E** Plasma Wayland VM, with and without the KDE portal.
- **S** Launch, navigate every view, open the file picker, trigger the pairing `AdwAlertDialog`, toggle the system dark mode, test HiDPI.
- **✓** No crashes, no unreadable text, no broken layout; dark mode follows when the portal is installed.
- **✗** The pairing dialog is unusable or the theme is illegible → **PLAT-DEC-003** needs revisiting sooner.
- **🔒** The pairing dialog must display the short fingerprint legibly. An unreadable fingerprint is a security defect, not a cosmetic one.
- **⏱** 1 d · **⇢** none · **R** _pending_

---

### POC-WIN-01 — Compile and bind on Windows

- **Q** Does the workspace build for `x86_64-pc-windows-msvc`, and does `bind_endpoints` produce a correct `Families` value?
- **H** Builds after Wave 0. The `IPV6_V6ONLY` probe is empirical so it should work, but Windows defaults `V6ONLY` on and has different `AddrInUse`/`SO_EXCLUSIVEADDRUSE` semantics ([13 §4](13-CROSS-PLATFORM-DISCOVERY.md)).
- **E** Windows 11, MSVC Build Tools, Windows SDK.
- **S** Build; run the portable tests; call `bind_endpoints` and **assert the resulting `Families`**.
- **✓** Builds; portable tests pass; `Families` matches what the sockets actually accept, verified by connecting over both families.
- **✗** `Families` claims a family the listener does not serve → the exact defect the module docs warn about, and discovery would advertise an unreachable address.
- **🔒** Advertising an address that refuses connections is indistinguishable from the machine being asleep — a reliability problem that reads as a security one.
- **⏱** 2 d · **⇢** POC-CORE-01 · **R** _pending_

### POC-WIN-02 — DNS-SD discovery

- **Q** Can a Windows machine advertise `_omnibridge._tcp.local.` such that the **existing, unmodified** Android app finds and connects to it?
- **H** Yes with `mdns-sd` (README claims Windows support). Fallback: `DnsServiceRegister`, which carries an unresolved question about whether it publishes A/AAAA records.
- **E** Windows 11 + an Android device on one Wi-Fi network.
- **S** Advertise with `mdns-sd`. Check with `dns-sd -B` from a Mac or `avahi-browse` from Linux. Then browse from the Android app.
- **✓** The service appears with correct TXT keys; the Android app lists it and dials it; coexistence with the Windows built-in responder on 5353 is stable across a restart.
- **✗** Cannot bind 5353, or the record lacks address records → fall back to the platform API and re-run.
- **🔒** Confirm the identity fingerprint is **not** published in TXT.
- **⏱** 2 d · **⇢** POC-WIN-01 · **R** _pending_

### POC-WIN-03 — CNG / TPM ECDSA P-256 identity

- **Q** Can a non-exportable P-256 key be created in the Microsoft Platform Crypto Provider and wrapped in an OmniBridge-shaped certificate?
- **H** Yes. CNG lists ECDSA P-256; the provider is documented as non-extractable.
- **E** Windows 11 with TPM 2.0, and a second machine/VM **without** one.
- **S** `NCryptOpenStorageProvider(MS_PLATFORM_CRYPTO_PROVIDER)` → `NCryptCreatePersistedKey` → `NCryptFinalizeKey`; export the public key; build a self-signed certificate around it with `rcgen`; compute `Fingerprint::from_certificate_der`.
- **✓** Key created and non-exportable (verify export **fails**); certificate parses; the SPKI fingerprint is stable across restarts. On the no-TPM machine the fallback provider is used and **reported**.
- **✗** P-256 unsupported by the provider, or `rcgen` cannot build a certificate around an external public key.
- **🔒** Attempt to export the key and confirm it fails. Confirm the fallback is *visible*, never silent (**X5**).
- **⏱** 3 d · **⇢** POC-WIN-01 · **R** _pending_

### POC-WIN-04 — TLS + SPKI pinning with a CNG key

- **Q** Can a TPM-held key complete OmniBridge's mutually-authenticated, pinned TLS 1.3 handshake with the **existing Linux daemon**?
- **H** Yes, via `rustls-cng`'s `CngSigningKey` in a `ResolvesClientCert`/`ResolvesServerCert`.
- **E** Windows 11 (TPM) + a Fedora machine running `omnibridged`.
- **S** Wire the signer into `client_config`/`server_config`; pair; exchange a PING/PONG; then a `battery.v1` message.
- **✓** Handshake completes in **both directions**; both sides' pins match; a deliberately wrong pin is rejected.
- **✗** Signature scheme or encoding mismatch → the whole "portable core + hardware key" architecture needs rethinking before any platform work.
- **🔒** **The negative test is mandatory:** a wrong pin must fail. A PoC that only proves the happy path would not catch a verifier that returns `Ok(())`, which `tls.rs` names as "the single most dangerous thing in this codebase".
- **⏱** 2 d · **⇢** POC-WIN-03, POC-CORE-02 · **R** _pending_

### POC-WIN-05 — Clipboard listener and loop suppression

- **Q** Does `AddClipboardFormatListener` satisfy `ClipboardBackend`, and does the existing content-hash dedup suppress self-echo?
- **H** Yes on both. `WM_CLIPBOARDUPDATE` is a content-free signal, which is exactly what `ClipboardWatch` yields.
- **E** Windows 11, with clipboard history and Cloud Clipboard **enabled**.
- **S** Message-only window; read/write `CF_UNICODETEXT`; watch; run the scenario from `capabilities/clipboard/tests/loops.rs`; test CRLF handling; test the sensitive-exclusion formats.
- **✓** Copy on Windows → phone receives. Phone copy → Windows applies. **No loop.** A sensitive clip does not appear in Win+V. CRLF↔LF round-trips without corrupting the content hash.
- **✗** A loop occurs, or `OpenClipboard` contention causes hangs.
- **🔒** Verify no clipboard content reaches the log (the Linux test `logging.rs` is the model). Verify the sensitive clip does not reach Cloud Clipboard (**X7**).
- **⏱** 2 d · **⇢** POC-WIN-01 · **R** _pending_

### POC-WIN-06 — User-session agent lifecycle

- **Q** Does a per-session agent survive lock, unlock, sleep, resume, network change and fast user switching?
- **H** Yes, with the caveat that Windows offers no supervisor — a crash is terminal until next login.
- **E** Windows 11 with **two** user accounts.
- **S** Start at login; lock/unlock; sleep/resume; Wi-Fi→Ethernet; switch users and confirm two agents with two identities and two pipes; confirm the port fallback works when both want 55432.
- **✓** All of the above; user B cannot reach user A's pipe; both advertise distinct instances.
- **✗** Only one user can run OmniBridge, or clipboard access breaks after a lock/unlock cycle.
- **🔒** **Explicitly attempt** to open user A's pipe from user B's session and confirm it is denied (**X3**).
- **⏱** 3 d · **⇢** POC-WIN-01 · **R** _pending_

### POC-WIN-07 — WinUI ↔ agent named pipe

- **Q** Can a WinUI 3 app drive the agent over a per-SID named pipe, and is the pipe safe against squatting?
- **H** Yes. The control protocol is newline-delimited JSON and toolkit-agnostic.
- **E** Windows 11, Windows App SDK.
- **S** Pipe server with `FILE_FLAG_FIRST_PIPE_INSTANCE` and a per-SID DACL; a minimal WinUI client showing status and devices; **a hostile squatter process that tries to pre-create the pipe name**.
- **✓** The app works; the squatter fails to create the pipe when the agent is running, and the client detects a wrong server when the agent is not.
- **✗** The squatter succeeds → **X2** is unmitigated and must be fixed before shipping.
- **🔒** The squatter test is the point of this PoC, not an extra.
- **⏱** 3 d · **⇢** POC-WIN-06 · **R** _pending_

### POC-WIN-08 — Firewall rules

- **Q** Do Private + `LocalSubnet` rules permit LAN use and genuinely block a public network?
- **H** Yes; Microsoft recommends exactly this scoping for home/small-business apps.
- **E** Windows 11 on a private network and on a network marked Public.
- **S** Create the two rules; connect from a phone; switch the profile to Public and retry.
- **✓** Works on Private; **fails on Public**; the rules are removed cleanly.
- **✗** Traffic passes on Public → the scoping is wrong.
- **🔒** The negative test is the deliverable (**X13**).
- **⏱** 1 d · **⇢** POC-WIN-01 · **R** _pending_

### POC-WIN-09 — MSIX lifecycle

- **Q** Does MSIX give install, start-at-login, update, uninstall and firewall-rule removal?
- **H** Yes for the first four; the firewall step may need an external installer, since MSIX restricts system changes.
- **E** Windows 11.
- **S** Package; `windows.startupTask Enabled="true"`; install; reboot; update to a new version; uninstall.
- **✓** Starts at login without a consent dialog (packaged desktop apps do not prompt); update preserves identity and pairings; uninstall prompts about data and removes firewall rules.
- **✗** Firewall rules cannot be created from MSIX → fall back to an MSI/Inno installer.
- **🔒** An update must **never** regenerate the identity (**X5**). Uninstall must not leave rules behind (**WIN-011**).
- **⏱** 2 d · **⇢** POC-WIN-06 · **R** _pending_

---

### POC-MAC-01 — Compile and test on macOS

- **Q** Does the workspace build and pass its portable tests on `aarch64-apple-darwin`?
- **H** Yes with fewer changes than Windows — the existing Unix code in `store.rs` and `destination.rs` is correct on macOS as written.
- **E** Apple Silicon Mac, Xcode CLT.
- **S** Build; run the full suite except the Linux-graphical clipboard tests.
- **✓** Builds; tests pass; `require_private_mode` behaves identically.
- **✗** Tier-2 target issues, or a POSIX assumption that differs on Darwin.
- **🔒** —
- **⏱** 1 d · **⇢** POC-CORE-01 · **R** _pending_

### POC-MAC-02 — Bonjour interoperability

- **Q** Can `mdns-sd` advertise alongside `mDNSResponder`? If not, does the `dnssd` C API work from Rust?
- **H** `mdns-sd` may fail to bind 5353 — macOS is the strictest of the three desktops. Budget for the fallback.
- **E** Mac + Android device + a Linux machine running Avahi.
- **S** Try `mdns-sd`; verify with `dns-sd -B`; browse from Android. On failure, implement `DNSServiceRegister`.
- **✓** The Android app finds and connects to the Mac; the record survives sleep/resume and a Wi-Fi change.
- **✗** Both routes fail → macOS discovery needs Network.framework and a Swift bridge.
- **🔒** No fingerprint in TXT.
- **⏱** 2 d · **⇢** POC-MAC-01 · **R** _pending_

### POC-MAC-03 — Secure Enclave identity

- **Q** Can a Secure Enclave P-256 key be created without user presence and wrapped in an OmniBridge certificate?
- **H** Yes. The Enclave supports only P-256, which is exactly what OmniBridge uses.
- **E** Apple Silicon Mac.
- **S** `SecKeyCreateRandomKey` with `kSecAttrTokenIDSecureEnclave` and `.privateKeyUsage` (**no** `.userPresence`); extract the public key; build a certificate; compute the fingerprint.
- **✓** Key created; export **fails**; no biometric prompt on signing; fingerprint stable across reboots.
- **✗** A prompt appears on every signature → the access-control flags are wrong and must be corrected *before* any key is used in production, because they are immutable after creation.
- **🔒** The "no user presence" property is the whole point — verify by signing repeatedly with the screen locked and unlocked.
- **⏱** 3 d · **⇢** POC-MAC-01 · **R** _pending_

### POC-MAC-04 — TLS + pinning with an Enclave key

- **Q** Can a Secure Enclave key complete OmniBridge's pinned mutual TLS 1.3 handshake against the **existing Linux daemon**?
- **H** Yes, via a custom `rustls::sign::SigningKey` over `SecKeyCreateSignature` with `.ecdsaSignatureMessageX962SHA256`.
- **E** Mac + a Fedora machine running `omnibridged`.
- **S** Write the signer; wire it in; pair; PING/PONG; a `battery.v1` exchange.
- **✓** Handshake completes both directions; pins match; **a wrong pin is rejected**.
- **✗** Signature verification fails → almost certainly the digest trap ([12 §4](12-APPLE-SECURITY-AND-INTEGRATION.md), trap 1). This is the exact class of error that made OmniBridge's Android v1 keys unusable.
- **🔒** Negative pin test mandatory. Do **not** work around a failure by exporting the key.
- **⏱** 3 d · **⇢** POC-MAC-03, POC-CORE-02 · **R** _pending_

### POC-MAC-05 — `NSPasteboard` sync

- **Q** **First:** does reading `NSPasteboard.general` from a signed, notarized, unsandboxed background agent prompt the user on current macOS? **Then:** does `changeCount` polling satisfy the (amended) `ClipboardBackend` contract?
- **H** Reading does not prompt for an unsandboxed Developer-ID agent, but this is the least certain claim in the macOS analysis and must be measured first.
- **E** macOS, current release, agent signed and notarized.
- **S** Read/write/poll; loop-suppression scenario; measure CPU cost of polling at 250 ms and 500 ms, and under App Nap.
- **✓** No prompt (or exactly one, at first use); no loop; CPU cost negligible; polling stops when no peer wants auto-send.
- **✗** A prompt on every read → macOS auto-send is not viable and the capability must be manual-only there.
- **🔒** Poll only `changeCount`, never content. Verify no content reaches the log.
- **⏱** 2 d · **⇢** POC-MAC-01 · **R** _pending_

### POC-MAC-06 — `SMAppService` login agent

- **Q** Does an `SMAppService`-registered login-item agent register reliably and survive the lifecycle?
- **H** Yes, but registration is known to be finicky — reports exist of `registerAndReturnError` succeeding without registering, and of code-signing-related failures.
- **E** macOS: a clean install, an upgraded system, and an app update.
- **S** Register; verify it appears in System Settings → Login Items; log out/in; sleep/resume; change network; update the app and confirm it still runs.
- **✓** Registers in all three scenarios; survives everything; the user can disable it visibly.
- **✗** Registration silently fails on any scenario → an alternative (a classic `LaunchAgent` plist) is needed.
- **🔒** The agent must not be registered as a `LaunchDaemon` (root, no user context).
- **⏱** 2 d · **⇢** POC-MAC-01 · **R** _pending_

### POC-MAC-07 — SwiftUI ↔ agent IPC

- **Q** Can a SwiftUI app drive the agent over a Unix domain socket, reusing `server.rs`?
- **H** Yes — macOS is the platform where the existing IPC transfers wholesale.
- **E** macOS.
- **S** Agent hosts the existing control server at an Application Support path; SwiftUI client shows status, devices and transfers.
- **✓** Works; the socket is not reachable by another user.
- **✗** Sandboxing (if later adopted) forces XPC → record the cost.
- **🔒** Verify the socket's containing directory is user-only.
- **⏱** 2 d · **⇢** POC-MAC-01 · **R** _pending_

---

### POC-IOS-01 — Bonjour discovery

- **Q** Does `NWBrowser` find `_omnibridge._tcp` published by the existing Linux daemon, with parseable TXT keys?
- **H** Yes.
- **E** iPhone/iPad + a Fedora machine running `omnibridged`.
- **S** `NWBrowser` with `NSBonjourServices = ["_omnibridge._tcp"]`; parse `v`/`pv`/`id`/`dn`; resolve endpoints.
- **✓** Service found; TXT parsed; both address families resolved; the `Endpoints.kt` ordering rules reproduce sensibly.
- **✗** TXT keys unavailable through `NWBrowser`'s API surface.
- **🔒** Discovery grants nothing; a spoofed record must lead only to a failed handshake.
- **⏱** 1 d · **⇢** none · **R** _pending_

### POC-IOS-02 — Local network permission

- **Q** When is the prompt shown, what does denial look like in code, and what is the recovery path?
- **H** Prompted at first browse or first local connect; denial is unrecoverable in-app; recovery is Settings.
- **E** iPhone, fresh install, and a device where the permission was previously denied.
- **S** Trigger the prompt; deny; observe the error; implement a "open Settings" affordance.
- **✓** The prompt appears once; denial is detectable and produces an accurate message with a working Settings deep link.
- **✗** Denial is indistinguishable from "no devices found" → the UX would be inexplicable.
- **🔒** The usage string must be honest and specific — App Review and user trust both depend on it.
- **⏱** 1 d · **⇢** POC-IOS-01 · **R** _pending_

### POC-IOS-03 — Pairing

- **Q** Can an iOS client complete the full pairing flow against the existing `omnibridged`?
- **H** Yes; the pairing proof is portable Rust and the QR payload format is fixed.
- **E** iPhone + Fedora daemon.
- **S** QR scan; `PairRequest` with the HMAC proof; human confirmation on the desktop; trust-store persistence on both sides.
- **✓** Pairing completes; the desktop lists the phone; the phone reconnects after a restart.
- **✗** Any divergence in proof construction.
- **🔒** The short fingerprint must be displayed on both screens for comparison. A rejected pairing must not persist anything.
- **⏱** 2 d · **⇢** POC-IOS-04 · **R** _pending_

### POC-IOS-04 — UniFFI + Secure Enclave TLS identity

- **Q** Can the Rust core run on iOS via UniFFI, with a Secure Enclave signer called back **from Rust into Swift** during the handshake?
- **H** Yes, but this is the highest-risk piece of the whole Apple strategy: a synchronous Swift callback on a rustls thread.
- **E** iPhone + Fedora daemon.
- **S** UniFFI surface for connect/pair/send; a Swift callback implementing the signer; a full pinned mutual handshake.
- **✓** Handshake completes; no deadlock; no main-actor violation; a wrong pin is rejected.
- **✗** The callback deadlocks or cannot be made `Send`-safe → reconsider iOS-1 vs iOS-2 in [02 §6](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md).
- **🔒** Negative pin test mandatory. Errors crossing the FFI must carry no user content.
- **⏱** 4 d · **⇢** POC-MAC-04 (the macOS signer is ~90% of this) · **R** _pending_

### POC-IOS-05 — Foreground lifecycle

- **Q** How fast is discover→connect→handshake→usable on reopen?
- **H** Fast enough to feel instant if the last-known address is cached.
- **E** iPhone on Wi-Fi.
- **S** Measure cold launch, warm reopen, after a network change, after airplane-mode toggle.
- **✓** Warm reopen under ~2 s to a usable session.
- **✗** Multi-second reconnects on every foreground → the product feels broken regardless of correctness.
- **🔒** A cached address is a hint, never trust — the pin still decides.
- **⏱** 2 d · **⇢** POC-IOS-03 · **R** _pending_

### POC-IOS-06 — Background suspension measurement ★

- **Q** Exactly how long does an established session survive after backgrounding, on a real device, on battery? What does the desktop observe?
- **H** Seconds to tens of seconds; the desktop sees a hang, not a clean close, until liveness expires.
- **E** iPhone, **physical device, on battery, not attached to Xcode** — the debugger changes suspension behaviour and would invalidate the result.
- **S** Establish a session; background the app; measure time to socket death with and without `beginBackgroundTask`; observe the desktop's `DeviceState` transition; repeat with the screen locked and under memory pressure.
- **✓** A number, and a description of the desktop-side failure mode.
- **✗** n/a — **any** result is the deliverable. This PoC cannot fail; it can only be skipped.
- **🔒** Confirm that a half-open session on the desktop cannot be exploited: `session.rs`'s liveness and `on_closed(peer, session_id)` handling must clean up correctly.
- **⏱** 2 d · **⇢** POC-IOS-05 · **R** _pending_
- **Note** This PoC decides **PLAT-DEC-005** and therefore whether Wave 9 happens at all.

### POC-IOS-07 — Clipboard manual send/receive

- **Q** Does `UIPasteControl` avoid the paste prompt, and does foreground receive work?
- **H** Yes to both — `UIPasteControl` is one of the documented non-prompting paths.
- **E** iPhone.
- **S** A `UIPasteControl` "send clipboard" button; a received clip written to `UIPasteboard` while foreground; the `PENDING_USER` flow for a clip that arrives without auto-apply.
- **✓** No prompt on the `UIPasteControl` path; receive works; `PENDING_USER` + `ClipboardApply` behave as designed.
- **✗** A prompt appears anyway → send becomes a two-tap flow and the UX must say so.
- **🔒** No automatic clipboard reading, ever. No UI element implying it exists.
- **⏱** 1 d · **⇢** POC-IOS-03 · **R** _pending_

### POC-IOS-08 — Share Extension

- **Q** Can a Share Extension reach the identity and trust store and send a file?
- **H** Yes, via an App Group + a keychain access group; but extension lifetime is short.
- **E** iPhone.
- **S** Extension; App Group container; keychain access group on the Enclave key; send a small and a large file.
- **✓** Small file completes; the extension reads the trust store **read-only**; a large file's failure is graceful and explained.
- **✗** The extension cannot use the Enclave key → the access group must be set at key creation, which is immutable — so this must be settled before any key ships.
- **🔒** Extension must not write the trust store and must not be able to pair (**SEC-007**).
- **⏱** 2 d · **⇢** POC-IOS-04 · **R** _pending_

### POC-IOS-09 — Files send and receive

- **Q** What does `files.v1` mean on iOS in practice?
- **H** Foreground-only both ways; received files land in the app container and are visible in the Files app.
- **E** iPhone/iPad.
- **S** Send via the document picker; receive to `Documents/`; enable `LSSupportsOpeningDocumentsInPlace`; background the app mid-transfer.
- **✓** Both directions work in the foreground; files appear in the Files app; a mid-transfer suspension fails cleanly with a clear message and no partial file presented as complete.
- **✗** A partial file is left looking complete → a correctness bug, since `destination.rs`'s atomic-promotion invariant would have been lost.
- **🔒** The "no file with the final name ever contains partial data" invariant must hold on iOS too.
- **⏱** 2 d · **⇢** POC-IOS-05 · **R** _pending_

---

### POC-CORE-04 — Windows-target compile check 🆕

| Field | Value |
| --- | --- |
| **Question** | After Wave 0, does `cargo check` succeed for the six portable crates on `x86_64-pc-windows-msvc`? |
| **Why docs cannot answer** | `ring` requires MSVC and a C toolchain (V-10, upstream `BUILDING.md`), so a Linux cross-compile is not a valid gate. Whether the portable crates are otherwise clean is only knowable by compiling |
| **Input** | The post-Wave-0 workspace |
| **Minimal scope** | `cargo check -p omnibridge-proto -p omnibridge-core -p omnibridge-control -p omnibridge-capability-{clipboard,files,battery} --no-default-features --target x86_64-pc-windows-msvc` |
| **Success** | Clean. `omnibridge-runtime` deliberately excluded — its `mdns-sd` Windows behaviour is V-12 |
| **Failure** | Any `std::os::unix` leak, or an unexpected transitive Unix-only dependency |
| **Output** | The gate that becomes CI-001 |
| **VM?** | ✅ GitHub-hosted `windows-latest` has MSVC |
| **Hardware?** | None beyond a CI runner |
| **Blocks** | Wave 0 acceptance (G3), CI-001 |
| **Result** | ✅ **PASS — 2026-09-01.** Green on a GitHub-hosted `windows-2025-vs2026` runner (Windows Server 2025 10.0.26100), `rustc 1.98.0` with **`host: x86_64-pc-windows-msvc`**, `cargo 1.98.0`, Visual Studio Enterprise 2026 18.9.12112.369. The specified `cargo check --no-default-features --target x86_64-pc-windows-msvc` over the six portable crates is clean; the same six also **build**, and their portable test targets link — 14 MSVC executables. `ring` was compiled by MSVC (`ring_core_0_17_14_.lib`), not substituted. The boundary was additionally asserted on the **resolved** dependency graph (87 packages, no platform crate, no `unix-fs`/`linux-backends`/`upower`), because the failure POC-CORE-01 actually hit was Cargo feature unification, which a source grep cannot see. `omnibridge-runtime` excluded as specified. 12/12 steps `success`, no `continue-on-error`. **This is CI-001**, permanent on PRs to `main`/`develop`. [Run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649) · commit `cfd33f6` · [full evidence](../../reports/foundation/wave-0-platform-abstraction.md) |
| **Class** | **P0 — architecture blocking** |

### POC-LINUX-05 — `wl-copy --sensitive` on real Debian/Ubuntu 🆕

| Field | Value |
| --- | --- |
| **Question** | Does `wl-copy --sensitive` fail as predicted on wl-clipboard 2.2.1, and does a probe detect it reliably? |
| **Why docs cannot answer** | The source proves the flag is absent and that `exit(1)` follows; what needs measuring is OmniBridge's end-to-end behaviour and the probe's reliability |
| **Input** | Debian 13, Ubuntu 24.04, Ubuntu 26.04; a paired Android device sending a `sensitive_hint` clip |
| **Minimal scope** | Send a sensitive clip to each; observe. Then run the proposed probe |
| **Success** | The failure reproduces; the probe detects 2.2.1 without false positives; a 2.3.0 system is unaffected |
| **Failure** | The probe is unreliable, or the failure differs from prediction |
| **Output** | Validates LINUX-010 and PLAT-DEC-013 |
| **VM?** | ✅ needs a real Wayland session, so a VM not a container |
| **Hardware?** | An Android device for the end-to-end half |
| **Blocks** | LINUX-010, PLAT-DEC-013, Wave 2 |
| **Class** | **P1 — platform blocking** |

### POC-DISC-01 — Five-platform discovery interoperability

- **Q** Does every advertiser reach every browser, over both address families, across network changes?
- **H** Yes; all five speak plain DNS-SD.
- **E** The full lab ([22 §7](22-IMPLEMENTATION-ROADMAP.md)) on one link.
- **S** The matrix in [13 §8](13-CROSS-PLATFORM-DISCOVERY.md), plus: record disappears within the TTL after shutdown; correct after a Wi-Fi change; correct after suspend/resume; behaviour with a VPN active; behaviour on an AP-isolated guest network.
- **✓** Every cell passes; the two negative environments (VPN, AP isolation) produce a *diagnosable* failure rather than silence.
- **✗** Any new desktop is invisible to the **existing shipped** Android app → backward compatibility is broken, which is not acceptable.
- **🔒** Confirm no fingerprint in TXT anywhere; confirm `dn` sanitisation on every client.
- **⏱** 2 d · **⇢** POC-WIN-02, POC-MAC-02, POC-IOS-01 · **R** _pending_
