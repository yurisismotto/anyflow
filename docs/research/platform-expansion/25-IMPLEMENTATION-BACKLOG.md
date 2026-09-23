# 25 — Implementation backlog

| Field | Value |
| --- | --- |
| **Title** | Every actionable item this research produced |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Findings converted into work items, with source documents and dependencies. |
| **Decision status** | PROPOSED. **Nothing here is implemented in this sprint.** |
| **Evidence** | Each item cites the document that produced it. |
| **Related documents** | [21](21-POC-MASTER-PLAN.md), [22](22-IMPLEMENTATION-ROADMAP.md), [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |

---

## Legend

**Type** — `refactor` · `feature` · `security` · `packaging` · `docs` · `ci` · `design`
> **⚠ UPDATED by the verification sprint (2026-08-31).** Five items added, two rewritten. See
> §"Verification-sprint delta" at the end of this document, and
> [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) / [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md).
> **P0 discipline restated:** P0 means *blocks architecture, security, or a first implementation* —
> not "desirable". Every P0 below is tied to a wave, a decision, a risk, and a PoC where one exists.

**Priority** — P0 blocks a wave · P1 required for the wave to ship · P2 should ship with it ·
P3 opportunistic
**Wave** — from [22](22-IMPLEMENTATION-ROADMAP.md); `X` means no platform dependency

---

## ARCH — architecture and core

| ID | Title | Pri | Wave | Type | Dep | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ARCH-001 | Adapter-crate layout | P0 | 0 | refactor | — | Establish `omnibridge-runtime` + per-platform adapter crates as the platform boundary, instead of scattering `#[cfg]` through seven files | The workspace has a documented boundary; no security-relevant code is behind an untested `#[cfg]` arm | [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) |
| ARCH-002 | `IdentitySigner` trait | P0 | 0 | refactor | ARCH-001 | Replace `LocalIdentity`'s owned PKCS#8 bytes with a signer abstraction wired through rustls `ResolvesClientCert`/`ResolvesServerCert`; the file key becomes one implementation | Every existing test passes unmodified; Android interop unchanged; a hardware signer can be added without touching `tls.rs` | [01 §3.1](01-CURRENT-ARCHITECTURE-AUDIT.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| ARCH-003 | Split `omnibridge-daemon`; extract control types | P0 | 0 | refactor | ARCH-001 | `omnibridge-runtime` (portable) + `omnibridge-linux`; a `ControlTransport` trait; control request/response types in their own crate so the GUI stops depending on the daemon | `daemon/tests/control.rs` passes unmodified; the GUI builds without `omnibridge-daemon` | [01 §4](01-CURRENT-ARCHITECTURE-AUDIT.md), [17](17-BACKGROUND-EXECUTION-MODEL.md) |
| ARCH-004 | Make `x11rb` optional | P1 | 0 | refactor | — | `x11rb` is an unconditional dependency of the clipboard crate; gate it behind a feature so non-Linux builds do not pull it | `cargo build --no-default-features` for the clipboard crate succeeds on a non-Linux target | [01 §5](01-CURRENT-ARCHITECTURE-AUDIT.md) |
| ARCH-005 | Amend the `ClipboardBackend` polling contract | P1 | 0 | docs | — | Permit declared polling where the platform offers no event source; require the backend to report the interval in `describe()` and to poll only a change counter | The doc comment states the exception and its conditions; Linux behaviour unchanged | [10 §6](10-MACOS-FEASIBILITY.md), [15 §3](15-CROSS-PLATFORM-CLIPBOARD.md) |
| ARCH-006 | `FileSink` trait | P0 | 0 | refactor | ARCH-001 | Abstract `destination.rs`'s Unix-mode and XDG specifics; keep the policy (dedicated dir, never peer-influenced, atomic promotion) portable | `daemon/tests/files.rs` passes unmodified | [16 §2](16-CROSS-PLATFORM-FILES.md) |
| ARCH-007 | Move endpoint ordering into the shared core | P2 | 9 | refactor | — | `Endpoints.kt`'s ordering and filtering (IPv4 first, drop unzoned link-local, cap attempts) is pure, tested logic that would otherwise be written a third time for iOS | Rust port with the Kotlin test cases ported alongside | [13 §5](13-CROSS-PLATFORM-DISCOVERY.md) |
| ARCH-008 | Document the OmniBridge Agent concept | P2 | X | docs | — | Add the Agent abstraction and its per-platform lifetimes to `docs/architecture/` | A reader understands why Windows is not a Service and why iOS has no agent | [17](17-BACKGROUND-EXECUTION-MODEL.md) |
| ARCH-009 | Cache last-known-good address per peer | P2 | X | feature | — | Speeds reconnect, which is the felt quality of the product on mobile | Measured reconnect improvement; the cached address is a hint, never trust | [17 §5](17-BACKGROUND-EXECUTION-MODEL.md) |
| ARCH-010 | Share state→display mapping | P3 | X | refactor | — | "Connected / stale / revoked / discovered" has real rules; four independent renderings will disagree | One rule set, consumed by all UIs | [18 §5](18-UI-PLATFORM-STRATEGY.md) |

---

## SEC — security

| ID | Title | Pri | Wave | Type | Dep | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SEC-001 | Per-platform security-control checklist | P1 | X | docs | — | For each platform, record which of the Linux controls is present, equivalent, or an accepted gap. Becomes a certification gate | A checklist exists and is filled in per wave | [09 §2](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [20](20-SECURITY-THREAT-ANALYSIS.md) |
| SEC-002 | `verify_protection()` per platform | P0 | 0 | security | ARCH-002 | The equivalent of `require_private_mode` for every key backing. Without it the Windows software fallback ships with no protection check | Windows DACL check; Linux mode check unchanged; hardware backings trivially pass | [09 §3](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [14 §4](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| SEC-003 | Named pipe hardening | P0 | 5 | security | ARCH-003 | `FILE_FLAG_FIRST_PIPE_INSTANCE`, per-SID DACL, per-SID name, caller verification via `GetNamedPipeClientProcessId` | A hostile squatter process cannot create the pipe name while the agent runs (POC-WIN-07) | [09 §4](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| SEC-004 | **Filename rules — REWRITTEN** | **P0** | **0** | security | — | **Research v1's premise was refuted**: device names, trailing dots/spaces and `\` are already present and tested. Add only what is missing: **`:` (alternate data streams)**, **Unicode category `Cf`** (`U+202E` bidi override, `U+200B`, `U+FEFF` — `is_control()` matches `Cc` only), `CONIN$`/`CONOUT$`, and `<>"\|?*`. **Protocol-global, no `#[cfg]`** (PLAT-DEC-014) | `a:b` rejected; `photo\u{202E}gnp.exe` stripped; `CONIN$` rejected; existing 15 filename tests unmodified | [26 §7](26-EXTERNAL-VERIFICATION-CLOSEOUT.md), [27 PLAT-DEC-014](27-ARCHITECTURE-DECISION-CLOSEOUT.md) |
| SEC-005 | Record the sandbox gap as an accepted risk | P2 | X | docs | — | The systemd hardening has no Windows/macOS equivalent. Say so; do not claim parity | The threat model names the gap and the compensating mitigations | [09 §5](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| SEC-006 | Quarantine received files on macOS | P2 | 8 | security | — | Set `com.apple.quarantine` so a received executable meets Gatekeeper. A genuine improvement over the Linux behaviour | A received `.app` triggers the normal Gatekeeper prompt | [10 §8](10-MACOS-FEASIBILITY.md) |
| SEC-007 | iOS Share Extension is read-only against the trust store | P1 | 9 | security | — | Two processes writing the trust store can corrupt it; and pairing belongs where the human is | The extension can use an existing pairing and cannot create one | [12 §9](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| SEC-008 | Document the shared keychain access group | P3 | 9 | docs | — | Sharing the identity key with an extension widens who can use it — deliberate, but it must be written down | A line in the Apple security notes | [12 §9](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| SEC-009 | **Never auto-regenerate over an unusable key — REWRITTEN, now a present-tense defect** | **P0** | **0** | security | ARCH-002 | **This is broken on Linux today.** `Store::open()` uses `Path::exists()`, which returns `false` on *any* metadata error (`EACCES`, broken symlink, non-traversable parent) — so an unreadable `identity.key` routes into `initialize()`, which generates a new identity **and overwrites `state.json`, destroying the peer list**. Use `try_exists()`; classify into the six states in [28 §7](28-WAVE-0-IMPLEMENTATION-SPEC.md); `initialize()` reachable **only** from `IDENTITY_NOT_CREATED`; record `key_backing` in `state.json` and bump the schema | Fault injection: key deleted / `chmod 000` / dir non-traversable → refuses to start **and leaves `state.json` byte-identical** | [26 §11.2](26-EXTERNAL-VERIFICATION-CLOSEOUT.md), [28 §7](28-WAVE-0-IMPLEMENTATION-SPEC.md), [27 PLAT-DEC-015](27-ARCHITECTURE-DECISION-CLOSEOUT.md) |
| SEC-010 | Signing-key custody plan | P1 | 5 | docs | — | Four platforms means four signing keys, collectively worth more than any device identity. CI secrets, separate keys, a revocation plan | A written custody and revocation procedure | [20 X11](20-SECURITY-THREAT-ANALYSIS.md) |

---

## LINUX

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| LINUX-001 | Settle the `gcc` requirement | P1 | 1 | docs | Determine empirically whether `ring` is the only reason the RPM needs a C toolchain, and document the answer for every platform | A build in a container with rustup and no `cc` either succeeds or fails, and the result is recorded | [04 §4](04-LINUX-PORTABILITY.md) |
| LINUX-002 | `rustc`-version-aware Debian build-deps | P1 | 2 | packaging | Ubuntu LTS's default `rustc` is 1.75; versioned `rustc-1.8x` packages exist | `Build-Depends: rustc-1.82 \| rustc (>= 1.82)` and the package builds on 24.04 | [05 §6](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| LINUX-003 | `gethostname` instead of `/etc/hostname` + `"Fedora"` | P1 | 1 | feature | The current fallback names a distribution and will be visibly wrong everywhere else; `/etc/hostname` is also not always the live hostname | Device name is correct on Fedora, Debian, in a container, and the fallback is neutral | [04 §7](04-LINUX-PORTABILITY.md) |
| LINUX-004 | firewalld default-block diagnosis | P2 | 1 | feature | Fedora blocks inbound 55432 by default; Debian and Ubuntu do not. The reference platform is the one that fails | `omnibridge status` detects an unreachable listener and explains it | [07 §7](07-LINUX-PACKAGING.md) |
| LINUX-005 | XDG autostart entry | P1 | 1 | packaging | Covers non-systemd distributions and users who never run `systemctl --user enable` | OmniBridge starts at login on a systemd and a non-systemd distribution | [04 §8](04-LINUX-PORTABILITY.md) |
| LINUX-006 | X11 clipboard read/write | P2 | X | feature | The XFIXES watch exists; selection ownership (read/write) does not. Affects X11 sessions, some NVIDIA setups, remote desktops and VMs | Clipboard works on a pure X11 session | [04 §10](04-LINUX-PORTABILITY.md) |
| LINUX-007 | Linux TPM2 / PKCS#11 signer | P2 | X | security | Otherwise Linux becomes the only OmniBridge platform without hardware-backed identity | A handshake with a TPM-held key, as an unprivileged user, with no manual sysadmin step | [14 §5.3](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| LINUX-008 | Linux share-target integration | P3 | X | feature | The desktop with the deepest integration elsewhere has the weakest share story | "Send to OmniBridge" appears in a file manager | [16 §7](16-CROSS-PLATFORM-FILES.md) |

---

## KDE

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| KDE-001 | Name the missing data-control protocol in `clipboard status` | P2 | 3 | feature | Today the user sees "auto-send unavailable" with no way to know the cause is a `wl-clipboard` version | The message names the protocol and the package version needed | [06 §4](06-KDE-PLASMA-WAYLAND.md) |
| KDE-002 | Document the Klipper clipboard↔selection hazard | P2 | 3 | docs | With that Klipper setting on, every mouse selection becomes a clipboard event and is synced. OmniBridge is behaving correctly; the desktop changed what CLIPBOARD means | A documented note, surfaced in `clipboard status` on Plasma | [06 §5](06-KDE-PLASMA-WAYLAND.md) |
| KDE-003 | Portal-aware file chooser | P3 | 3 | feature | KDE users get GTK's file dialog unless the portal is used | With `xdg-desktop-portal-kde` installed, the KDE picker appears | [06 §7](06-KDE-PLASMA-WAYLAND.md) |

---

## PKG — packaging

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PKG-001 | `.desktop`, icons, AppStream metainfo | P1 | 1 | packaging | None exist. OmniBridge has no menu entry, no icon and is invisible to GNOME Software / KDE Discover. Artwork already exists in `docs/design/assets/` | Menu entry with the Flowing Ribbon icon; the app appears in a software centre | [04 §11](04-LINUX-PORTABILITY.md), [07 §2](07-LINUX-PACKAGING.md) |
| PKG-002 | Package the GUI binary | P1 | 1 | packaging | The RPM ships only `omnibridged` and `omnibridge` | `dnf install omnibridge-gui` yields a working GUI | [07 §2](07-LINUX-PACKAGING.md) |
| PKG-003 | Ship the autostart entry | P1 | 1 | packaging | Pairs with LINUX-005 | Installed and honoured on GNOME and Plasma | [07 §6](07-LINUX-PACKAGING.md) |
| PKG-004 | Declare `wl-clipboard`; distro-neutral message | P1 | 1 | packaging | Not declared even as a weak dependency, and the "not installed" message names a `dnf` command | Declared per distro; the message names the binary, not a distro command | [04 §10](04-LINUX-PORTABILITY.md) |
| PKG-005 | Split `omnibridge` / `omnibridge-gui` | P1 | 1 | packaging | The GTK floor excludes distributions from the GUI only; the daemon has no GTK dependency | Two binary packages in both RPM and DEB | [05 §7](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| PKG-006 | Gate graphical tests out of build chroots | P1 | 2 | packaging | `%check` runs the full suite, including tests needing a graphical session | Package builds in mock and in a buildd chroot | [05 §6](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| PKG-007 | Recommend `xdg-desktop-portal-{gtk,kde}` | P3 | 3 | packaging | Dark-mode following and the file chooser depend on it | Declared as a recommendation | [06 §7](06-KDE-PLASMA-WAYLAND.md) |
| PKG-008 | firewalld service definition, not auto-enabled | P2 | 1 | packaging | Fedora blocks the listener by default; opening a port without consent is the wrong default | The definition is installed; the user is told how to enable it | [07 §7](07-LINUX-PACKAGING.md) |
| PKG-009 | Debian packaging skeleton | P1 | 2 | packaging | `debian/` built in CI, vendored crates (PLAT-DEC-011) | A `.deb` installs on trixie and on 26.04 | [05 §6](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| PKG-010 | Release tarball job | P2 | 2 | packaging | Covers every distribution OmniBridge does not package for, plus non-systemd | A signed tarball with checksums per release | [07 §3.3](07-LINUX-PACKAGING.md) |

---

## WIN

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WIN-001 | Windows discovery | P0 | 5 | feature | `mdns-sd` preferred; `DnsServiceRegister` fallback. Must coexist with the built-in responder | The **unmodified** Android app finds a Windows machine | [08 §4](08-WINDOWS-FEASIBILITY.md) |
| WIN-002 | CNG/TPM identity via `rustls-cng` | P0 | 5 | security | Non-exportable ECDSA P-256 in `MS_PLATFORM_CRYPTO_PROVIDER`; software fallback, visible | A pinned mutual handshake with the Linux daemon; a wrong pin rejected; export fails | [08 §6](08-WINDOWS-FEASIBILITY.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| WIN-003 | Windows `ClipboardBackend` | P1 | 6 | feature | `AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE`, `CF_UNICODETEXT` read/write | Bidirectional clipboard with no loop; the Linux loop test scenario passes | [08 §7](08-WINDOWS-FEASIBILITY.md), [15](15-CROSS-PLATFORM-CLIPBOARD.md) |
| WIN-004 | Actionable toast for transfer approval | P1 | 6 | feature | `files.v1` needs a human per transfer; an AUMID is required | Accept/decline from the notification | [16 §8](16-CROSS-PLATFORM-FILES.md) |
| WIN-005 | Windows `FileSink` | P0 | 5 | feature | `FOLDERID_Downloads`, `CREATE_NEW`, `MoveFileEx`, per-user ACL | Files land in the real Downloads folder; the atomic-promotion invariant holds | [16 §2](16-CROSS-PLATFORM-FILES.md) |
| WIN-006 | Process mitigation policies | P2 | 6 | security | The closest available analogue of the systemd hardening: dynamic-code prohibition, CFG, binary signature policy | Mitigations applied and verified with a tool | [09 §5](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| WIN-007 | Ephemeral-port fallback | P1 | 5 | feature | Two logged-in users cannot both bind 55432; the port is already advertised in SRV and no client hardcodes it | Two sessions each advertise a working port; the phone connects to both | [09 §7.1](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| WIN-008 | Sensitive-clip exclusion formats | P2 | 6 | security | Keeps a sensitive clip out of Win+V history and Cloud Clipboard — the local-first promise | A clip marked sensitive does not appear in Win+V | [15 §9](15-CROSS-PLATFORM-CLIPBOARD.md), [20 X7](20-SECURITY-THREAT-ANALYSIS.md) |
| WIN-009 | CRLF/LF normalisation at the backend boundary | P2 | 6 | feature | Must happen inside the backend so `content_hash` and dedup stay consistent | Round-trip Windows↔Linux without corruption or a dedup miss | [15 §5](15-CROSS-PLATFORM-CLIPBOARD.md) |
| WIN-010 | Crash-recovery policy | P2 | 6 | design | Windows offers no supervisor for a user-session agent. Recommendation: no watchdog; log it; let the UI relaunch | A written decision and an Event Log entry on crash | [17 §3.1](17-BACKGROUND-EXECUTION-MODEL.md) |
| WIN-011 | Remove firewall rules on uninstall | P1 | 6 | packaging | A leftover inbound rule for a deleted program is exactly the debris a security-conscious user will hold against the product | Rules absent after uninstall | [19 §5](19-PACKAGING-AND-DISTRIBUTION.md) |

---

## MAC

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| MAC-001 | `AppleSigningKey` | P0 | 7 | security | A `rustls::sign::SigningKey` over `SecKeyCreateSignature` with `.ecdsaSignatureMessageX962SHA256`. No equivalent crate exists | A pinned mutual handshake with the Linux daemon; a wrong pin rejected | [12 §4](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| MAC-002 | Enclave key policy: no user presence | **P0** | 7 | security | `Signer::sign` is synchronous; handshakes are unattended. Access control is immutable after key creation, so this must be right the first time | Repeated signing with the screen locked, no prompt, ever | [12 §3](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| MAC-003 | Certificate around an external public key | P0 | 7 | security | `rcgen` must build the certificate for a key it cannot hold; the embedded SPKI must be byte-identical to what the fingerprint code extracts | `Fingerprint::from_certificate_der` matches the Enclave key's SPKI | [12 §4](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| MAC-004 | Notarization pipeline | P1 | 7 | ci | `codesign --options runtime --timestamp` → `notarytool submit --wait` → `stapler staple`. Requires a Mac | A DMG from CI runs on a clean Mac with no Gatekeeper warning | [12 §7](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| MAC-005 | macOS `ClipboardBackend` with declared polling | P1 | 8 | feature | `changeCount` polling, suspended when no peer wants auto-send, interval reported in `describe()` | Bidirectional clipboard, no loop, negligible CPU, no repeated prompts | [10 §6](10-MACOS-FEASIBILITY.md), ARCH-005 |
| MAC-006 | macOS notifications for approval | P1 | 8 | feature | `UNUserNotificationCenter` with actions | Accept/decline a transfer from the notification | [16 §8](16-CROSS-PLATFORM-FILES.md) |
| MAC-007 | `SMAppService` login agent | P0 | 7 | feature | Registration is known to be finicky; must be tested on clean install, upgraded system, and after an app update | Appears in System Settings → Login Items and survives all three scenarios | [10 §7](10-MACOS-FEASIBILITY.md) |

---

## IOS

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| IOS-001 | UniFFI surface for the Rust core | P0 | 9 | refactor | Connect, pair, send, receive — plus a Swift callback for the Enclave signer, called synchronously from a rustls thread. The highest-risk piece of the Apple strategy | POC-IOS-04 completes a handshake with no deadlock | [02 §6](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), [12 §8](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| IOS-002 | App Group + keychain access group | P0 | 9 | security | Required for the Share Extension to reach the identity and trust store. The access group is set at key creation and is immutable, so it must be decided before any key ships | The extension signs with the existing identity | [12 §9](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| IOS-003 | `UIPasteControl` clipboard send | P1 | 9 | feature | The one read path that does not prompt. No automatic clipboard on iOS, and no UI implying it | Send with one tap, no prompt; no auto-send toggle exists | [11 §7](11-IOS-IPADOS-FEASIBILITY.md) |
| IOS-004 | Files-app visibility | P1 | 9 | feature | `UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace` so received files are reachable | Received files appear in the Files app | [16 §5](16-CROSS-PLATFORM-FILES.md) |
| IOS-005 | Share Extension send path | P1 | 9 | feature | The primary entry point given the foreground-only model | Share a photo from Photos to a paired computer | [11 §8](11-IOS-IPADOS-FEASIBILITY.md) |

---

## PROTO — protocol (design only)

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PROTO-001 | Design `CapabilityProperties` metadata | P3 | X | design | Additive, fail-closed, never an authorization input, capability ids unchanged. **Design only** — do not implement | A written proposal with backward-compatibility and migration sections | [15 §7](15-CROSS-PLATFORM-CLIPBOARD.md), PLAT-DEC-006 |
| PROTO-002 | Design `files.v2` with resume | P3 | X | design | Would benefit every platform; driven by general value, not by iOS. Must be `files.v2`, never a mutation of v1 | A written proposal | [16 §6](16-CROSS-PLATFORM-FILES.md) |
| PROTO-003 | Add `Platform` enum values | P1 | 5 | feature | `PLATFORM_WINDOWS`, `PLATFORM_MACOS`, `PLATFORM_IOS`, `PLATFORM_IPADOS`. Additive proto3; an old peer shows a generic icon | An old Android build still connects to a Windows desktop | [23 PLAT-DEC-008](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |

---

## UX

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UX-001 | Per-platform expression section in the UI guidelines | P2 | X | docs | The identity travels; the widgets do not. Document which is which | A section in `UI-GUIDELINES.md` | [18 §3](18-UI-PLATFORM-STRATEGY.md) |
| UX-002 | Desktop notifications on Linux | P2 | X | feature | The daemon implements none today | Transfer offers and clipboard events notify via `org.freedesktop.Notifications` | [03 n14](03-PLATFORM-CAPABILITY-MATRIX.md) |
| UX-003 | Advertise only what the platform supports | **P1** | every | feature | No auto-clipboard toggle on iOS; no watch shown as available when none is running. A certification gate, not a nicety | Per-platform UI audit against the capability matrix | [03 §5](03-PLATFORM-CAPABILITY-MATRIX.md) |
| UX-004 | Document the Klipper sync hazard to users | P3 | 3 | docs | See KDE-002 | User-facing note | [06 §5](06-KDE-PLASMA-WAYLAND.md) |
| UX-005 | VPN / AP-isolation diagnostics | P2 | X | feature | The two most likely causes of "it doesn't work" with nothing in the logs | "Found your computer but cannot reach it — this network may block device-to-device traffic" | [13 §6](13-CROSS-PLATFORM-DISCOVERY.md) |
| UX-006 | mDNS advertisement suppression toggle | P2 | X | feature | The threat model already promises it as the mitigation for the tracking cost; it does not exist, and laptops travel | A per-network or global toggle that stops advertising | [13 §7](13-CROSS-PLATFORM-DISCOVERY.md), [20 X14](20-SECURITY-THREAT-ANALYSIS.md) |
| UX-007 | Show key backing | P2 | 5 | feature | Hardware vs software must be visible in `omnibridge status`, the UI and first-run — never silent | The backing is displayed and logged at startup | [14 §7](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| UX-008 | Desktop holds the latest clip for a disconnected peer | P2 | 9 | feature | Makes the iOS foreground-only model feel intentional. In memory only, bounded, expiring — never on disk | Open the iOS app and the clip from ten minutes ago is offered | [15 §6](15-CROSS-PLATFORM-CLIPBOARD.md) |
| UX-009 | Optional, opt-in update notification | P3 | X | feature | Any version check is a network call, which dents "no required cloud". Off by default | Off by default; explained when enabled | [19 §4](19-PACKAGING-AND-DISTRIBUTION.md) |

---

## CI

| ID | Title | Pri | Wave | Type | Description | Acceptance idea | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CI-001 | Cross-target compile matrix for the portable crates · ✅ **DONE 2026-09-01** — `.github/workflows/portable-windows-msvc.yml`, green on `windows-2025-vs2026` ([run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649)) | **P0** | 0 | ci | Turns "someone added a Unix-only call to `omnibridge-core`" from a discovery made months later into a failed PR | A PR adding `std::os::unix` to a portable crate fails CI | [22 §8](22-IMPLEMENTATION-ROADMAP.md) |
| CI-002 | Windows and macOS runners | P1 | 5 / 7 | ci | Required for platform tests, packaging and — on macOS — signing and notarization | Per-platform jobs run on every PR | [22 §8](22-IMPLEMENTATION-ROADMAP.md) |
| CI-003 | Ubuntu 24.04 libadwaita-floor job | P1 | 2 | ci | Noble ships libadwaita **1.5.0** and OmniBridge requires 1.5 — zero margin. A drift would otherwise be invisible until a user reports it | Using a 1.6+ API fails this job | [05 §5.1](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| CI-004 | `cargo audit` in CI | P2 | X | ci | The dependency surface roughly triples with `rustls-cng`, Apple bindings, `windows-sys` and UniFFI | A known-vulnerable dependency fails the build | [19 §7](19-PACKAGING-AND-DISTRIBUTION.md) |
| CI-005 | SBOM and release checksums | P2 | X | ci | Expected of a security-positioned product | Every release carries an SBOM and checksums | [19 §7](19-PACKAGING-AND-DISTRIBUTION.md) |

---

## Summary

| Category | Items | P0 |
| --- | --- | --- |
| ARCH | 10 | 4 |
| SEC | 10 | 4 |
| LINUX | 8 | 0 |
| KDE | 3 | 0 |
| PKG | 10 | 0 |
| WIN | 11 | 3 |
| MAC | 7 | 4 |
| IOS | 5 | 2 |
| PROTO | 3 | 0 |
| UX | 9 | 0 |
| CI | 5 | 1 |
| **Total** | **81** | **18** |

The eighteen P0 items are the ones that block a wave from starting or shipping. Four of them —
**SEC-004** (filename rules), **SEC-009** (never auto-regenerate an identity), **SEC-002**
(protection checks) and **MAC-002** (no user presence on the Enclave key) — are the ones where
getting it wrong is a security defect rather than a delay, and two of those (MAC-002, IOS-002)
are **irreversible after a key is created**.


---

## Verification-sprint delta (2026-08-31)

Added or rewritten as a result of [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). Every P0 is tied to a
wave, a decision, a risk and a PoC where one applies.

| ID | Title | Pri | Wave | Type | Depends | Description | Acceptance | Decision | Risk | PoC |
| --- | --- | :-: | :-: | --- | --- | --- | --- | --- | --- | --- |
| **ARCH-010** | Scope `unsafe_code` per crate | **P0** | **0** | refactor | — | `unsafe_code = "forbid"` is set workspace-wide, and `forbid` **cannot** be relaxed by an inner `#[allow]`. Every platform adapter needs `unsafe` (Win32, CNG, `Security.framework`, IOKit). Remove it from `[workspace.lints.rust]`; set `forbid` **explicitly** on `omnibridge-core`, `omnibridge-proto`, `omnibridge-control`, `omnibridge-runtime` and the three capability crates; adapters get `deny` | The security core still forbids `unsafe`; an adapter crate can use it with a justified `#[allow]` | PLAT-DEC-001 | **R-18** | — |
| **ARCH-011** | Platform value comes from the identity provider | P1 | 0 | refactor | ARCH-002 | `Platform::Linux` is hardcoded at `store.rs:154` and `store.rs:188`, so the storage layer decides platform identity. Move it behind `IdentityProvider::platform()` | An adapter supplies the value; no literal remains in `store.rs` | PLAT-DEC-008 | — | — |
| **LINUX-010** | Handle `wl-copy --sensitive` being unavailable | **P0** | **2** | security | — | `--sensitive` was added in wl-clipboard **2.3.0**. On 2.2.1 the flag does not exist and `wl-copy` **exits 1**, so `sensitive_hint` clips **fail outright** — on Debian 13 and **every** current Ubuntu LTS, on **every** desktop. Probe once (beside `probe_data_control()`), fail closed, and tell the user the cause and the remedy. **Do not** downgrade silently to a plain write | Probe detects 2.2.1; the failure message names `wl-clipboard` and the upgrade; a 2.3.0 system is unaffected | **PLAT-DEC-013** | **R-17** | **POC-LINUX-05** |
| **WIN-010** | Agent owns a message-only window and pumps messages | P0 | 5 | feature | — | `AddClipboardFormatListener` requires an `HWND`; `WM_CLIPBOARDUPDATE` is *posted to a window*. The agent cannot be a pure console/tokio process — it needs a message-only window and a message loop beside the async runtime. Reinforces the agent-not-service decision: a Session 0 service has no interactive window station | Clipboard changes arrive as `WM_CLIPBOARDUPDATE`; the runtime is not starved | PLAT-DEC-002 | — | POC-WIN-05 |
| **MAC-010** | The macOS agent must not exit on a network failure | P1 | 7 | feature | — | Apple FB16131937: *"macOS fails to display the local network alert when a process with a very short lifespan performs a local network operation… To work around this, update your code to not exit immediately after a local network operation fails."* OmniBridge's daemon treats a bind failure as fatal — on macOS that produces an agent that can never obtain the permission it needs | The agent survives a denied local-network operation long enough for the alert to appear | — | — | POC-MAC-06 |
| **KDE-001** | *(revised)* Name the missing protocol **and** the wl-clipboard version | P1 | 3 | feature | — | `omnibridge clipboard status` must say which data-control protocol was missing *and* which `wl-clipboard` is installed. On Ubuntu 26.04 LTS the cause is KWin 6.6 (`ext` only) × wl-clipboard 2.2.1 (`wlr` only) | The message names both halves | — | R-05 | POC-KDE-01 |
| **PKG-00x** | *(correction)* Do **not** depend on `wl-clipboard >= 2.3` | P2 | 4 | packaging | — | Fedora ships `2.2.1^git20251124`, which **has** both protocols and `--sensitive`. A version dependency would exclude a working system. **Recommend** 2.3 in packaging; decide at runtime by probe | Fedora is not excluded; the recommendation explains why | PLAT-DEC-013 | R-05, R-17 | — |
| **SEC-003** | *(sharpened)* Named-pipe hardening | P0 | 5 | security | ARCH-003 | **The default named-pipe DACL grants read to Everyone and to the anonymous account** — so `lpSecurityAttributes` must **never** be `NULL`. Add `PIPE_REJECT_REMOTE_CLIENTS`. `FILE_FLAG_FIRST_PIPE_INSTANCE` is **detection, not prevention**: on `ERROR_ACCESS_DENIED` the agent must **abort naming the squatter — never retry, never fall back to another name**. Per-session pipe name (Microsoft's own guidance) | A hostile squatter started first causes the agent to refuse to start | PLAT-DEC-002 | **R-06** | POC-WIN-07 |
| **WIN-008** | *(unblocked)* Clipboard-history / Cloud Clipboard exclusion | P1 | 6 | security | — | Set **all three** verified formats for `sensitive_hint` clips: `ExcludeClipboardContentFromMonitorProcessing` (any data), `CanIncludeInClipboardHistory` = DWORD 0, `CanUploadToCloudClipboard` = DWORD 0. The two DWORD formats cover disjoint halves | A sensitive clip appears in neither Win+V history nor Cloud Clipboard | — | X-W2 | POC-WIN-05 |

### P0 audit

The brief asks that no P0 be a merely-desirable improvement. Re-checked:

| P0 | Wave | Blocks | Verdict |
| --- | :-: | --- | --- |
| ARCH-001 Adapter-crate layout | 0 | the boundary itself | ✅ |
| ARCH-002 `IdentityProvider` | 0 | all hardware backing | ✅ |
| ARCH-003 Split daemon; extract control types | 0 | GUI/CLI portability | ✅ |
| ARCH-006 `FileSink` | 0 | `files.v1` off Linux | ✅ |
| **ARCH-010 `unsafe_code` scoping** | 0 | **every adapter** | ✅ new |
| SEC-002 `verify_protection()` | 0 | silent protection gap on Windows | ✅ |
| SEC-004 Filename rules | **0** *(was 0 or 5)* | a present-tense bidi defect | ✅ **moved earlier** |
| SEC-009 No auto-regeneration | 0 | **a live trust-store-destroying defect** | ✅ **severity raised** |
| SEC-003 Named pipe hardening | 5 | Windows control plane | ✅ |
| **LINUX-010 `--sensitive`** | 2 | `sensitive_hint` on Debian/Ubuntu | ✅ new |
| CI-001 Cross-target compile matrix | 0 | boundary regressions | ✅ |
| WIN-001/002/005, MAC-001/002/003/007, IOS-001/002 | 5–9 | their platform's first release | ✅ |

**One demotion.** ARCH-011 (platform value from the provider) was implied as P0-adjacent; it is
**P1** — it must land in Wave 0 for tidiness, but nothing is blocked if it slips to Wave 5.
