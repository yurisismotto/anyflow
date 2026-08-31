# 24 — Source bibliography

| Field | Value |
| --- | --- |
| **Title** | Sources consulted, with access dates and where each was used |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Every external source that supports a conclusion in this research, plus the repository files that support REPO VERIFIED claims. |
| **Decision status** | n/a |
| **Evidence** | This document *is* the evidence trail. |
| **Related documents** | all |

All URLs accessed **2026-08-31** unless noted.

---

## 1. Repository sources (REPO VERIFIED)

The primary source, per the sprint's source hierarchy. State as of branch `develop`,
commit `f7a0015`.

| Area | Files | Used in |
| --- | --- | --- |
| Workspace and toolchain | `desktop/Cargo.toml`, `desktop/rust-toolchain.toml`, all eight crate manifests | 01, 04, 05 |
| Protocol schemas | `protocol/proto/anyflow/v1/{core,envelope}.proto`, `capabilities/{battery,clipboard,files}_v1.proto` | 01, 03, 15, 16, 23 |
| Security core | `desktop/core/src/{tls,identity,fingerprint,pairing,store}.rs` | 01, 02, 09, 12, 14, 20 |
| Session and capability model | `desktop/core/src/{session,capability,discovery,clipboard_policy,framing,qr,error}.rs` | 01, 02, 03, 13, 15 |
| Capabilities | `desktop/capabilities/{battery,clipboard,files}/src/**` | 01, 15, 16 |
| Clipboard platform boundary | `desktop/capabilities/clipboard/src/backend/{mod,wayland,x11}.rs` | 06, 10, 15 |
| Daemon | `desktop/daemon/src/{main,listener,mdns,server,control,state}.rs` | 01, 08, 13, 17 |
| CLI and GUI | `desktop/cli/src/main.rs`, `desktop/gui/src/**`, `desktop/gui/Cargo.toml`, `desktop/gui/build.rs` | 01, 04, 05, 18 |
| Tests (used as specification) | `desktop/core/tests/`, `desktop/daemon/tests/`, `desktop/capabilities/*/tests/` | 21, 22 |
| Android | `android/app/src/main/java/io/github/yurisismotto/anyflow/**`, `AndroidManifest.xml`, `gradle/libs.versions.toml` | 01, 03, 13, 14, 17 |
| Packaging | `packaging/fedora/{anyflow.spec,anyflowd.service}` | 04, 07, 09, 17 |
| Design system | `docs/design/{tokens.json,BRAND.md,UI-GUIDELINES.md}`, `docs/design/assets/**` | 18 |
| Architecture docs | `docs/architecture/{OVERVIEW,PROTOCOL,CLIPBOARD,FILES}.md` | 01, 15, 16 |
| Threat model | `docs/security/THREAT_MODEL.md` | 20 |
| ADRs | `docs/adr/ADR-0001` … `ADR-0014` | throughout |

Notable ADRs relied on: **ADR-0002** (native Kotlin on Android), **ADR-0003** (Rust daemon and
the Unix-socket choice), **ADR-0004** (protobuf via protox, no `protoc`), **ADR-0005** (mDNS, and
the phone-initiates direction), **ADR-0006** (P-256 and the deliberate rejection of the Secret
Service), **ADR-0007** (TLS and pinning), **ADR-0008** (capability grants, no auto-grant for
side-effecting capabilities), **ADR-0013** (the data-stream ALPN), **ADR-0014** (why the GNOME
clipboard watch uses XFIXES).

---

## 2. Rust and crates (docs.rs, official repositories)

| Topic | Title | Publisher | URL | Used in |
| --- | --- | --- | --- | --- |
| Target tiers | Platform Support — The rustc book | Rust Project | https://doc.rust-lang.org/rustc/platform-support.html | 08, 10, 11, 19 |
| Apple targets | `*-apple-darwin`, `*-apple-ios` | Rust Project | https://doc.rust-lang.org/nightly/rustc/platform-support/apple-darwin.html · .../apple-ios.html | 10, 11 |
| Windows targets | `*-pc-windows-msvc` | Rust Project | https://doc.rust-lang.org/rustc/platform-support/windows-msvc.html | 08 |
| External signers | `rustls::manual::_03_howto` — "if your private key resides in a HSM…" | rustls | https://docs.rs/rustls/latest/rustls/manual/_03_howto/ | 02, 08, 12, 14 |
| Signer traits | `rustls::sign::SigningKey`, `sign::Signer`, `sign::CertifiedKey` | rustls | https://docs.rs/rustls/latest/rustls/sign/ | 02, 12, 14 |
| Client cert resolution | `rustls::client::ResolvesClientCert` | rustls | https://docs.rs/rustls/latest/rustls/client/trait.ResolvesClientCert.html | 02, 08, 14 |
| Windows CNG keys | `rustls-cng` — non-exportable keys from the Windows cert store; v0.7.1, rustls ^0.23, ECDSA secp256r1 | rustls org | https://docs.rs/rustls-cng/latest/rustls_cng/ · https://github.com/rustls/rustls-cng | 08, 09, 14 |
| mDNS | `mdns-sd` — "supports macOS, Linux and Windows"; no Avahi/Bonjour dependency | keepsimple1 | https://github.com/keepsimple1/mdns-sd · https://docs.rs/mdns-sd/ | 04, 08, 10, 13 |
| Rust↔Swift/Kotlin FFI | UniFFI user guide; Swift bindings | Mozilla | https://mozilla.github.io/uniffi-rs/ · https://github.com/mozilla/uniffi-rs | 02, 11, 12 |

---

## 3. Linux, freedesktop, GNOME, KDE, Flatpak

| Topic | Title | Publisher | URL | Used in |
| --- | --- | --- | --- | --- |
| libadwaita 1.5 | Libadwaita 1.5 release notes — `AdwDialog`, `AdwAlertDialog` | GNOME (Alice Mikhaylenko) | https://blogs.gnome.org/alicem/2024/03/15/libadwaita-1-5/ | 04, 05 |
| `AdwAlertDialog` | Adw.AlertDialog | GNOME | https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/class.AlertDialog.html | 05 |
| `AdwDialog` | Adw.Dialog (1.5) | GNOME | https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.5/class.Dialog.html | 05 |
| Adaptive dialog migration | Migrating to Adaptive Dialogs | GNOME | https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.5/migrating-to-adaptive-dialogs.html | 05 |
| libadwaita Rust bindings | `libadwaita-rs` — `AlertDialog` | GNOME / world | https://world.pages.gitlab.gnome.org/Rust/libadwaita-rs/stable/latest/docs/libadwaita/struct.AlertDialog.html | 05 |
| KWin data-control | wayland/datacontrol: Port to ext-data-control (MR !6606) | KDE | https://invent.kde.org/plasma/kwin/-/merge_requests/6606 | 03, 05, 06 |
| Protocol deprecation | wlr-data-control-unstable-v1 — deprecated, superseded by ext-data-control-v1 | Wayland Explorer / wayland-protocols | https://wayland.app/protocols/wlr-data-control-unstable-v1 | 06 |
| wl-clipboard ext-data-control | Issue #242 — request for `ext-data-control-v1` support | bugaevc/wl-clipboard | https://github.com/bugaevc/wl-clipboard/issues/242 | 05, 06 |
| wl-clipboard releases | Releases | bugaevc/wl-clipboard | https://github.com/bugaevc/wl-clipboard/releases | 05, 06 |
| Flatpak permissions | Sandbox Permissions | Flatpak | https://docs.flatpak.org/en/latest/sandbox-permissions.html | 07 |
| Flatpak mDNS | No mDNS hostname lookup support (issue #348); mDNS resolution fails (issue #4044) | flatpak/flatpak | https://github.com/flatpak/flatpak/issues/348 · https://github.com/flatpak/flatpak/issues/4044 | 07 |
| Flatpak mDNS portal | Add mDNS local device discovery portal (discussion #1365) | flatpak/xdg-desktop-portal | https://github.com/flatpak/xdg-desktop-portal/discussions/1365 | 07 |

### Distribution archives (version evidence)

| Query | Publisher | URL | Used in |
| --- | --- | --- | --- |
| `libadwaita-1-0` across suites | Debian | https://packages.debian.org/search?keywords=libadwaita-1-0&searchon=names&suite=all&section=all | 05 |
| `libgtk-4-1` across suites | Debian | https://packages.debian.org/search?keywords=libgtk-4-1&searchon=names&suite=all&section=all | 05 |
| `rustc` across suites | Debian | https://packages.debian.org/search?keywords=rustc&searchon=names&suite=all&section=all | 04, 05 |
| `wl-clipboard` across suites | Debian | https://packages.debian.org/search?keywords=wl-clipboard&searchon=names&suite=all&section=all | 05, 06 |
| `libadwaita-1-0` across releases | Ubuntu | https://packages.ubuntu.com/search?keywords=libadwaita-1-0&searchon=names&suite=all&section=all | 05 |
| `libgtk-4-1` across releases | Ubuntu | https://packages.ubuntu.com/search?keywords=libgtk-4-1&searchon=names&suite=all&section=all | 05 |
| `rustc` across releases | Ubuntu | https://packages.ubuntu.com/search?keywords=rustc&searchon=names&suite=all&section=all | 04, 05 |
| `wl-clipboard` across releases | Ubuntu | https://packages.ubuntu.com/search?keywords=wl-clipboard&searchon=names&suite=all&section=all | 05, 06 |

Values recorded in [05 §3](05-DEBIAN-UBUNTU-COMPATIBILITY.md). These are moving targets — the
table should be re-checked whenever a new Debian or Ubuntu release lands.

---

## 4. Microsoft (learn.microsoft.com)

| Topic | Title | URL | Used in |
| --- | --- | --- | --- |
| DNS-SD registration | `DnsServiceRegister` function — Windows 10, `windns.h`, `dnsapi.lib`, registration tied to process lifetime | https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsserviceregister | 08, 13 |
| DNS-SD browsing | `DnsServiceBrowse` function | https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsservicebrowse | 08, 13 |
| DNS-SD (WinRT) | `Windows.Networking.ServiceDiscovery.Dnssd` namespace | https://learn.microsoft.com/en-us/uwp/api/windows.networking.servicediscovery.dnssd | 08 |
| DNS-SD caveat | "Why doesn't DnsServiceRegister create mDNS A/AAAA records?" (Q&A — **secondary**) | https://learn.microsoft.com/en-us/answers/questions/2280653/ | 08, 23 (Q-04) |
| Key storage providers | CNG Key Storage Providers — Software / Smart Card / **Platform (TPM)**; `MS_PLATFORM_CRYPTO_PROVIDER`; ECDSA P256/P384/P521 | https://learn.microsoft.com/en-us/windows/win32/seccertenroll/cng-key-storage-providers | 08, 09, 14 |
| Key creation | `NCryptCreatePersistedKey` | https://learn.microsoft.com/en-us/windows/win32/api/ncrypt/nf-ncrypt-ncryptcreatepersistedkey | 08 |
| CNG overview | Cryptography API: Next Generation | https://learn.microsoft.com/en-us/windows/win32/seccng/cng-portal | 08 |
| TPM | TPM fundamentals; How Windows uses the TPM | https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/tpm-fundamentals | 08, 09 |
| Clipboard watching | `AddClipboardFormatListener`; `WM_CLIPBOARDUPDATE` (0x031D) | https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener · .../dataxchg/wm-clipboardupdate | 08, 15 |
| Clipboard general | Using the Clipboard; Clipboard Functions; Clipboard Notifications | https://learn.microsoft.com/en-us/windows/win32/dataxchg/using-the-clipboard | 08, 15 |
| Session 0 | Interactive Services; Services and Session Zero | https://learn.microsoft.com/en-us/windows/win32/services/interactive-services | 08, 09, 17, 23 |
| Firewall rules | Windows Firewall Rules; Manage Windows Firewall with the command line; `New-NetFirewallRule` — `LocalSubnet`, Private profile, no wildcards in application rules | https://learn.microsoft.com/en-us/windows/security/operating-system-security/network-security/windows-firewall/rules | 08, 09 |
| Startup | `desktop:StartupTask` element; `StartupTask` class — packaged desktop apps do not prompt | https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-desktop-startuptask | 08 |
| WinUI 3 | WinUI 3 — Windows 10 1809+; Windows App SDK downloads and release notes (2.4.0 stable) | https://learn.microsoft.com/en-us/windows/apps/winui/winui3/ · https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads | 08, 18 |
| AF_UNIX | AF_UNIX comes to Windows — build 17063, SOCK_STREAM only, **no ancillary data** | https://devblogs.microsoft.com/commandline/af_unix-comes-to-windows/ | 01, 08, 09 |

---

## 5. Apple (developer.apple.com)

Apple's documentation site renders through JavaScript and several pages could not be retrieved
by this session's fetch tooling. Where only a search-result summary was available, the claim is
marked **EXTERNAL VERIFICATION REQUIRED** in the document that uses it. Those are listed in §7.

| Topic | Title | URL | Used in |
| --- | --- | --- | --- |
| Secure Enclave keys | `kSecAttrTokenIDSecureEnclave` — only 256-bit EC private keys | https://developer.apple.com/documentation/security/ksecattrtokenidsecureenclave | 10, 12, 14 |
| Secure Enclave (CryptoKit) | `SecureEnclave.P256`, `.Signing`, `.KeyAgreement` | https://developer.apple.com/documentation/cryptokit/secureenclave/p256 | 10, 12, 14 |
| Pasteboard change detection | `NSPasteboard.changeCount` | https://developer.apple.com/documentation/appkit/nspasteboard/1533544-changecount | 10, 15 |
| Pasteboard concepts | Pasteboard Concepts (archive) | https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbConcepts.html | 10, 15 |
| Pasteboard privacy | `NSPasteboard.AccessBehavior`; Pasteboard detection patterns | https://developer.apple.com/documentation/appkit/nspasteboard/accessbehavior-swift.enum | 10, 20 |
| Local network privacy | TN3179: Understanding local network privacy | https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy | 03, 11 |
| Local network keys | `NSLocalNetworkUsageDescription`; `NSBonjourServices` | https://developer.apple.com/documentation/bundleresources/information-property-list/nslocalnetworkusagedescription | 11, 12 |
| Local network (WWDC) | Support local network privacy in your app — WWDC20 10110 | https://developer.apple.com/videos/play/wwdc2020/10110/ | 11 |
| Background execution | Configuring background execution modes; `UIBackgroundModes`; About the background execution sequence | https://developer.apple.com/documentation/xcode/configuring-background-execution-modes | 03, 11 |
| Networking + multitasking | TN2277: Networking and Multitasking — sockets reclaimed while suspended | https://developer.apple.com/library/archive/technotes/tn2277/_index.html | 03, 11 |
| Pasteboard on iOS | `UIPasteboard`; `UIPasteControl`; `UIPasteboard.DetectionPattern` | https://developer.apple.com/documentation/uikit/uipastecontrol | 11, 15 |
| Login items | `SMAppService` | https://developer.apple.com/documentation/servicemanagement/smappservice | 10, 17 |
| Notarization | Notarizing macOS software before distribution | https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution | 10, 12, 19 |
| Developer ID | Developer ID — Signing Your Apps for Gatekeeper; New Notarization Requirements (2019-04-10) | https://developer.apple.com/developer-id/ · https://developer.apple.com/news/?id=04102019a | 10, 12, 19 |
| macOS distribution | Distributing software on macOS | https://developer.apple.com/macos/distribution/ | 10, 19 |

Apple Developer Forums threads were consulted for corroboration only (`SMAppService`
registration reliability, `UIPasteControl` behaviour, Secure Enclave key export). They are
**secondary sources** and no architectural conclusion rests on them alone.

---

## 6. Source-quality policy applied

Per the sprint brief, in order:

1. **Repository code** — the basis for every "as it is today" claim (§1).
2. **Official platform documentation** — learn.microsoft.com, developer.apple.com (§4, §5).
3. **Official API/framework documentation** — docs.rs, rustls, UniFFI (§2).
4. **Official distribution documentation** — packages.debian.org, packages.ubuntu.com (§3).
5. **Official upstream projects** — GNOME, KDE, Flatpak, wl-clipboard, mdns-sd (§3).
6. **Secondary sources** — used only for corroboration, never as the sole basis for a decision.
   Where a conclusion would otherwise rest on one, the claim is tagged
   **EXTERNAL VERIFICATION REQUIRED** instead.

---

## 7. EXTERNAL VERIFICATION REQUIRED — the complete list

Everything Research v1 could not confirm from a primary source, with exactly what must be
checked. Nothing here was invented to fill a gap.

> **⚠ CLOSED OUT (2026-08-31).** All twelve were taken to primary sources in
> [26 — External verification closeout](26-EXTERNAL-VERIFICATION-CLOSEOUT.md).
> **8 VERIFIED · 2 PARTIALLY VERIFIED · 2 STILL OPEN · 0 REFUTED · 0 BLOCKED.**
> Neither remaining open item blocks Wave 0. The `Status` column below is the result; the
> evidence is in [26 §4](26-EXTERNAL-VERIFICATION-CLOSEOUT.md).

| # | Claim | Document | Status | Result |
| --- | --- | --- | --- |
| V-01 | Which wl-clipboard release added `ext-data-control-v1` | 05, 06 | **VERIFIED** | **2.3.0**, released 2026-03-22 (upstream release body). 2.2.1 has only `wlr` |
| V-02 | Whether KWin still exposes `wlr-data-control-unstable-v1` | 05, 06 | **VERIFIED** | **Dropped in Plasma 6.5** (commit `764b723`, 2025-04-12). KWin ≤ 6.4 has both |
| V-03 | Whether `DnsServiceRegister` publishes A/AAAA records | 08, 13 | **PARTIALLY VERIFIED** | The struct carries host + IPv4 + IPv6; on-wire publication undocumented. Demoted to a contingent question — `mdns-sd` is the primary path |
| V-04 | Exact Windows clipboard exclusion format names | 08, 09, 15 | **VERIFIED** | **Three** formats: `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory`, `CanUploadToCloudClipboard`. Semantics confirmed |
| V-05 | Full content of TN3179 | 03, 11 | **VERIFIED** | Retrieved in full via the documentation JSON API. Applies to **macOS 15+**; `launchd` agents get no exemption; listening needs no privilege |
| V-06 | `NSPasteboard.AccessBehavior` | 10, 20 | **VERIFIED** | **macOS 15.4**, four cases, **no Info.plist key**, General pasteboard **defaults to ask** |
| V-07 | Authoritative `UIBackgroundModes` list | 11 | **VERIFIED** | Eleven values; none fits a LAN control session. Conclusion confirmed |
| V-08 | Whether `org.nspasteboard.ConcealedType` is widely honoured | 10, 15 | **STILL OPEN** | Not establishable from any primary source. [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) recommends retiring the question: set it, gate nothing on it |
| V-09 | Whether a Share Extension inherits local-network permission | 11, 12 | **PARTIALLY VERIFIED** | *"In general, app extensions share the Local Network privilege state of their container app."* Residual: the background-undetermined case |
| V-10 | Whether `ring` is why the RPM spec requires `gcc` | 04 | **VERIFIED** | Upstream: *"ring currently requires a C (but not C++) toolchain."* Windows needs MSVC — which invalidates the Linux cross-compile gate |
| V-11 | Fedora's current `wl-clipboard` version | 05, 06 | **VERIFIED** | `2.2.1^git20251124.e808203-2.fc44` — **has** both protocols and `--sensitive` despite the version string |
| V-12 | Whether `mdns-sd` can bind 5353 alongside a system responder | 08, 10, 13 | **STILL OPEN** | Correctly a PoC. POC-WIN-02, POC-MAC-02 |

### 7.1 Sources added by the verification sprint

Full table with locators in [26 §12](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). All primary, all
accessed **2026-08-31**:

| Publisher | Artefact | Supports |
| --- | --- | --- |
| bugaevc | wl-clipboard release record; `src/wl-copy.c` @ `v2.2.1` and `v2.3.0` | V-01, the `--sensitive` defect |
| KDE | KWin `datacontroldevicemanager_v1.{h,cpp}`, commit history, tag ancestry | V-02 |
| Debian / Canonical | `kwin-wayland` and `wl-clipboard` across all suites | the KDE matrix |
| Fedora (local) | `wl-clipboard-2.2.1^git20251124.e808203-2.fc44` | V-11 |
| Microsoft | Clipboard Formats; Interactive Services; `CreateNamedPipeA`; Using the Clipboard; `DnsServiceRegister`; `DNS_SERVICE_INSTANCE` | V-03, V-04, PLAT-DEC-002, SEC-003 |
| Apple | TN3179 (full); `NSPasteboard.AccessBehavior`; `NSPasteboard` symbol list; `UIBackgroundModes`; Protecting keys with the Secure Enclave; `SecKeyCreateSignature`; `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` | V-05, V-06, V-07, V-09, PLAT-DEC-004, PLAT-DEC-009 |
| rustls | `SigningKey`, `Signer`, `CertifiedKey`, `ConfigBuilder` @ 0.23.43; `rustls-cng` @ `v0.7.1` | the Wave 0 identity seam |
| briansmith | `ring` `BUILDING.md` | V-10 |

**Method note.** Research v1 could not verify five Apple claims because `developer.apple.com`
renders through JavaScript. Those pages are backed by a JSON endpoint —
`https://developer.apple.com/tutorials/data/documentation/<path>.json` — which returns the same
authored content, including full technote bodies and complete symbol lists. **This unblocked V-05,
V-06 and V-07.** Recorded so a future sprint does not repeat the dead end.

---

## 8. Currency

Everything in §3's distribution tables and §2's crate versions is a moving target. The values
recorded here are correct as of **2026-08-31** and each document carries that date in its
header. Re-verify:

- **distribution package versions** whenever a Debian or Ubuntu release ships;
- **Windows App SDK and rustls/`rustls-cng` versions** before Wave 5;
- **Apple platform behaviour** before Waves 7–9, since Apple changes privacy behaviour at every
  major release and several claims here are already marked for verification;
- **the V-list in §7** before the corresponding wave starts, not at its end.
