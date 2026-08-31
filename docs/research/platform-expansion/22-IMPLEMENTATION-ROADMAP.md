# 22 — Implementation roadmap

| Field | Value |
| --- | --- |
| **Title** | Waves, dependencies, gates and the lab they need |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Ordering of the expansion, derived from dependencies rather than from platform popularity. Branch names, gates, hardware, CI. |
| **Decision status** | PROPOSED. Effort estimates are HYPOTHESIS. |
| **Evidence** | Ordering is derived from [01](01-CURRENT-ARCHITECTURE-AUDIT.md)–[21](21-POC-MASTER-PLAN.md). |
| **Related documents** | [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), [21](21-POC-MASTER-PLAN.md), [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md), [25](25-IMPLEMENTATION-BACKLOG.md) |


---

> **⚠ UPDATED by the verification sprint (2026-08-31).**
>
> **The wave order is confirmed, not changed.** Every ordering argument survived verification, and
> two got stronger:
>
> - **Wave 0 first** — reinforced. The identity seam is now documentation-verified against
>   rustls 0.23.43 rather than PoC-gated, and it turns out to be **two call sites**
>   ([26 §11.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). Wave 0 also now carries **two defect fixes**
>   that must not be refactored around.
> - **Windows before macOS** — reinforced. `rustls-cng` was verified in source (rustls org, active,
>   rustls 0.23, P-256, P1363→DER conversion with tests), while the Apple signer still has to be
>   written. Windows remains the cheapest place to prove the riskiest part of the architecture.
>
> **Changes to the plan:**
>
> | Item | Change |
> | --- | --- |
> | **Wave 0 size** | ≈**17 engineer-days**, nine PRs ([28 §16](28-WAVE-0-IMPLEMENTATION-SPEC.md)). Smaller than "M" — the identity refactor is two call sites — but it absorbs two new defect fixes |
> | **Wave 0 gate** | The proposed Linux cross-compile **does not work**: `ring` needs MSVC (V-10). Corrected to `cargo check` on a **Windows CI runner**, excluding `anyflow-runtime` |
> | **Wave 0 hardware** | **None beyond what the project already has** — Fedora, an Android device, and a CI Windows runner |
> | **Wave 2** | Gains **LINUX-010 (P0)**: `sensitive_hint` clips fail on Debian 13 and every current Ubuntu LTS |
> | **Wave 3** | **Cheaper.** POC-KDE-01's central question is answered; only the Xwayland fallback needs measuring |
> | **Wave 5** | Gains **WIN-010** (message-only window + message pump for `AddClipboardFormatListener`) |
> | **Wave 7** | Gains **MAC-010** (do not exit on network failure) and the macOS **local-network permission** (applies from macOS 15; `launchd` agents get no exemption). **Developer ID moves from a release requirement to a PoC prerequisite** |
> | **Wave 8** | macOS auto-send is **user-gated** from macOS 15.4, not merely polling-based |
> | **PoCs** | 37 specified. **Four are P0 and all are Wave 0 acceptance gates — none blocks Wave 0 from starting** ([27 §7](27-ARCHITECTURE-DECISION-CLOSEOUT.md)) |
>
> **Waves 0–4 remain a complete, shippable outcome**, and that is now a stronger claim: three of the
> four defects this programme has found are fixed inside Waves 0–2.

---
## 1. What determines the order

Not platform popularity. Three things:

1. **Architectural dependency.** Wave 0's seams are needed by every platform. Doing them once
   is cheaper than doing them three times badly.
2. **Cost of being wrong.** The identity signer is the riskiest piece of the architecture, and
   Windows is the cheapest place to prove it, because `rustls-cng` already exists.
3. **Cost per unit of value.** KDE and Debian/Ubuntu need almost no new code and turn "runs on
   the developer's Fedora" into "runs on Linux". That is the highest ratio in the plan and it
   comes before any new operating system.

The resulting order is **consolidate Linux → Windows → macOS → iOS**, with Wave 0 in front of
all of it.

Why macOS after Windows, despite macOS needing fewer code changes: the macOS *release* path
requires an Apple Developer Program membership, a Mac in CI, and notarization
([12 §7](12-APPLE-SECURITY-AND-INTEGRATION.md)); and the Secure Enclave signer must be written
from scratch, whereas Windows has `rustls-cng`. Windows is the cheaper proving ground for the
architecture, and macOS then inherits a proven pattern.

---

## 2. Waves

### Wave 0 — Core portability seams

| | |
| --- | --- |
| **Objective** | Create the platform boundary that does not exist today, without changing behaviour on Linux |
| **Dependencies** | none |
| **Branch** | `feature/core-platform-abstraction-v1` |
| **Deliverables** | `IdentitySigner` (ARCH-002); `StateStore`/`SecretFile` split of `store.rs`; `FileSink` (ARCH-006); `ControlTransport` (ARCH-003); `anyflow-daemon` → `anyflow-runtime` + `anyflow-linux`; control types extracted into their own crate; `x11rb` made optional (ARCH-004); `ClipboardBackend` polling contract amended (ARCH-005) |
| **Gates** | **Zero behavioural change on Linux.** The entire existing suite passes unmodified, including `core/tests/identity_and_store.rs`, `daemon/tests/e2e.rs` and the clipboard tests. Interop with the shipping Android app is unchanged. `cargo build --target x86_64-pc-windows-gnu` succeeds for the portable crates |
| **PoCs** | POC-CORE-01, -02, -03 |
| **Hardware** | Linux dev machine |
| **Risks** | This wave touches identity and the trust store. A regression here is a security regression. Mitigation: the existing tests are unusually good and must be treated as the specification — **no test may be modified to make a refactor pass** |
| **Effort** | **M** (~2–3 weeks) |

### Wave 1 — Linux portability

| | |
| --- | --- |
| **Objective** | Turn Fedora-specific details into Linux behaviour |
| **Dependencies** | Wave 0 |
| **Branch** | `feature/linux-portability-v1` |
| **Deliverables** | `gethostname` instead of `/etc/hostname`+`"Fedora"` (LINUX-003); settle the `gcc`/`ring` question (LINUX-001); `.desktop`, icons, AppStream metainfo (PKG-001); package the GUI (PKG-002); XDG autostart (PKG-003/LINUX-005); declare `wl-clipboard` (PKG-004); split `anyflow`/`anyflow-gui` (PKG-005); firewalld service definition, not auto-enabled (PKG-008) |
| **Gates** | Fresh Fedora install: menu entry, icon, starts at login, clipboard works, file received, no manual `systemctl` step required |
| **PoCs** | none |
| **Hardware** | Fedora VM |
| **Risks** | Low. Mostly packaging content |
| **Effort** | **S** (~1 week) |

### Wave 2 — Debian / Ubuntu

| | |
| --- | --- |
| **Objective** | Two more distribution families, no new code |
| **Dependencies** | Wave 1 |
| **Branch** | `feature/debian-packaging-v1` |
| **Deliverables** | `debian/` packaging (PKG-009); `rustc-1.82`-aware build-deps (LINUX-002); release tarball job (PKG-010); container CI jobs for trixie / 24.04 / 26.04 |
| **Gates** | Builds and runs on Debian 13, Ubuntu 24.04 and Ubuntu 26.04; the **libadwaita 1.5.0 floor holds on noble** (CI-003) |
| **PoCs** | POC-LINUX-01, POC-LINUX-02 |
| **Hardware** | Debian and Ubuntu VMs |
| **Risks** | The libadwaita floor moving without anyone noticing. Mitigated by the noble CI job |
| **Effort** | **S–M** (~1–2 weeks) |

### Wave 3 — KDE Plasma

| | |
| --- | --- |
| **Objective** | Certify Plasma; resolve the data-control question |
| **Dependencies** | Wave 2 |
| **Branch** | `feature/kde-wayland-v1` |
| **Deliverables** | POC-KDE-01/02 results; name the missing protocol in `clipboard status` (KDE-001); document the Klipper sync hazard (KDE-002); portal-aware file chooser (KDE-003); recommend `xdg-desktop-portal-kde` (PKG-007) |
| **Gates** | Clipboard works both ways on Plasma, or the reason is documented with a named cause and a mitigation; the GUI is usable, and the pairing dialog is legible |
| **PoCs** | POC-KDE-01, POC-KDE-02 |
| **Hardware** | Fedora KDE VM **and** Kubuntu 26.04 VM (the wl-clipboard 2.2.1 case) |
| **Risks** | The wl-clipboard/KWin protocol mismatch ([05 §5.3](05-DEBIAN-UBUNTU-COMPATIBILITY.md)). Cheap to discover, possibly awkward to fix |
| **Effort** | **S** (~1 week) |

### Wave 4 — Linux packaging and distribution

| | |
| --- | --- |
| **Objective** | Users can install AnyFlow without building it |
| **Dependencies** | Waves 1–3 |
| **Branch** | `feature/linux-distribution-v1` |
| **Deliverables** | Signed RPM repo (COPR); signed DEB repo; release tarballs + checksums; optionally POC-LINUX-03's Flatpak verdict |
| **Gates** | A user on Fedora, Debian or Ubuntu installs from a repo and pairs a phone without touching a compiler |
| **PoCs** | POC-LINUX-03 (optional) |
| **Hardware** | VMs |
| **Effort** | **S–M** |

> **Waves 0–4 are the point at which AnyFlow can honestly say it supports "Linux" rather than
> "Fedora".** If the expansion stopped here it would still be a significant improvement, and
> nothing after this point is required for it. That is a deliberate property of the ordering.

### Wave 5 — Windows foundation

| | |
| --- | --- |
| **Objective** | A Windows agent that pairs, discovers and holds a session |
| **Dependencies** | Wave 0 (hard) |
| **Branch** | `feature/windows-foundation-v1` |
| **Deliverables** | Windows adapter crate; `mdns-sd` on Windows (WIN-001); CNG/TPM identity (WIN-002); `rustls-cng` signer; Windows `FileSink` (WIN-005); named-pipe `ControlTransport` (SEC-003); **Windows-safe filename rules (SEC-004)**; ephemeral-port fallback (WIN-007); `Platform` proto enum value |
| **Gates** | The **existing, unmodified** Android app discovers, pairs with and transfers a file to a Windows machine, with a TPM-backed identity; a wrong pin is rejected |
| **PoCs** | POC-WIN-01 … -04, POC-WIN-06 |
| **Hardware** | Windows 11 with TPM 2.0 (**physical or a VM with a vTPM**), plus an Android device |
| **Risks** | `rcgen` around an external public key; `Families` probe behaviour; TPM absent in some VMs |
| **Effort** | **L** (~4–6 weeks) |

### Wave 6 — Windows capabilities and UI

| | |
| --- | --- |
| **Objective** | Clipboard, notifications, tray, UI, installer |
| **Dependencies** | Wave 5 |
| **Branches** | `feature/windows-clipboard-v1`, `feature/windows-ui-v1` |
| **Deliverables** | Windows `ClipboardBackend` (WIN-003); sensitive-exclusion formats (WIN-008); CRLF handling (WIN-009); toasts (WIN-004); tray; WinUI 3 app; firewall rules; MSIX + winget; process mitigations (WIN-006); uninstall removes rules (WIN-011) |
| **Gates** | Clipboard both ways with no loop; a sensitive clip stays out of Win+V; Public-profile firewall genuinely blocks; MSIX installs, starts at login, updates preserving identity, and uninstalls cleanly |
| **PoCs** | POC-WIN-05, -07, -08, -09 |
| **Hardware** | Windows 11 with two user accounts; a network switchable between Private and Public |
| **Risks** | Named-pipe squatting (**must** be closed); MSIX firewall restrictions |
| **Effort** | **L** (~4–6 weeks) |

### Wave 7 — macOS foundation

| | |
| --- | --- |
| **Objective** | A macOS agent that pairs, discovers and holds a session |
| **Dependencies** | Wave 0; ideally Wave 5 (the signer pattern) |
| **Branch** | `feature/macos-foundation-v1` |
| **Deliverables** | macOS adapter crate; discovery (`mdns-sd` or `dnssd`); `AppleSigningKey` (MAC-001); Enclave key policy (MAC-002); certificate around an external key (MAC-003); `SMAppService` agent (MAC-007); `.app` skeleton; signing + notarization pipeline (MAC-004) |
| **Gates** | The existing Android app pairs with a Mac using a Secure-Enclave-backed identity; the agent survives logout/login and sleep/wake; a notarized build runs on a clean Mac with no Gatekeeper warning |
| **PoCs** | POC-MAC-01 … -04, POC-MAC-06 |
| **Hardware** | **Apple Silicon Mac (required)**, Apple Developer Program membership, Android device |
| **Risks** | The signature-digest trap; `SMAppService` flakiness; notarization pipeline setup |
| **Effort** | **L** (~4–6 weeks) |

### Wave 8 — macOS capabilities and UI

| | |
| --- | --- |
| **Objective** | Clipboard, files, menu bar, UI, distribution |
| **Dependencies** | Wave 7 |
| **Branches** | `feature/macos-clipboard-v1`, `feature/macos-ui-v1` |
| **Deliverables** | `NSPasteboard` backend with declared polling (MAC-005); quarantine on received files (SEC-006); notifications (MAC-006); `MenuBarExtra`; SwiftUI app; DMG + Homebrew Cask |
| **Gates** | Clipboard both ways with no loop and no repeated prompting; files land in Downloads; the DMG installs and the Cask works |
| **PoCs** | POC-MAC-05, POC-MAC-07 |
| **Hardware** | Mac |
| **Risks** | Pasteboard privacy prompting (POC-MAC-05 measures it first) |
| **Effort** | **M–L** (~3–5 weeks) |

### Wave 9 — iOS / iPadOS constrained client

| | |
| --- | --- |
| **Objective** | A foreground companion, honestly scoped |
| **Dependencies** | Waves 7–8; **and POC-IOS-06's result** |
| **Branch** | `feature/ios-foundation-v1` |
| **Deliverables** | UniFFI surface; `NWBrowser` discovery; local-network permission UX; Enclave identity; pairing; foreground session; `UIPasteControl` clipboard (IOS-003); Share Extension (IOS-005); Files-app visibility (IOS-004); App Group + keychain group (IOS-002); TestFlight |
| **Gates** | Pairs with the existing Linux daemon; foreground clipboard and files both ways; **no UI element implies background operation** |
| **PoCs** | POC-IOS-01 … -09 |
| **Hardware** | iPhone **and** iPad (physical — the simulator invalidates POC-IOS-06), Mac, Apple Developer Program |
| **Risks** | The Swift↔Rust signer callback; App Review; and the possibility that the product is not worth shipping — a legitimate outcome |
| **Effort** | **XL** (~6–10 weeks) |

### Wave X — cross-cutting, any time after Wave 0

Not a wave; items with no platform dependency that should not wait:

- **SEC-004** Windows-safe filename rules — **do this in Wave 0 or Wave 5, not later**
- **SEC-009** never auto-regenerate an identity over an unusable key
- **UX-006** mDNS advertisement suppression toggle
- **UX-005** VPN / AP-isolation diagnostics
- **CI-004/005** `cargo audit`, SBOM, release checksums
- **LINUX-006** X11 clipboard read/write
- **PROTO-001/002** design-only work on capability metadata and `files.v2`

---

## 3. Dependency graph

```
Wave 0  Core seams
  ├────────────────────────────────────────────┐
  │                                            │
Wave 1  Linux portability                   Wave 5  Windows foundation
  │                                            │
Wave 2  Debian/Ubuntu                       Wave 6  Windows capabilities + UI
  │
Wave 3  KDE                                 Wave 7  macOS foundation  ←(pattern from W5)
  │                                            │
Wave 4  Linux distribution                  Wave 8  macOS capabilities + UI
                                               │
                                            Wave 9  iOS/iPadOS  ←(gated on POC-IOS-06)
```

Waves 1–4 and 5–6 are independent after Wave 0 and can run in parallel if there are two people.
Waves 7–9 are strictly sequential: each inherits patterns from the one before.

---

## 4. Effort summary

| Wave | Size | Rough weeks | Confidence |
| --- | --- | --- | --- |
| 0 Core seams | M | 2–3 | Medium-high — the scope is enumerable ([01 §5](01-CURRENT-ARCHITECTURE-AUDIT.md)) |
| 1 Linux portability | S | 1 | High |
| 2 Debian/Ubuntu | S–M | 1–2 | High — archive versions are known |
| 3 KDE | S | 1 | Medium — one unresolved question |
| 4 Linux distribution | S–M | 1–2 | Medium |
| 5 Windows foundation | L | 4–6 | Medium |
| 6 Windows capabilities/UI | L | 4–6 | Medium |
| 7 macOS foundation | L | 4–6 | Medium-low — the signer is unproven |
| 8 macOS capabilities/UI | M–L | 3–5 | Medium |
| 9 iOS/iPadOS | XL | 6–10 | **Low** — gated on POC-IOS-06 |

**Total ≈ 27–42 weeks of engineering**, plus ~71 engineer-days of PoCs ([21](21-POC-MASTER-PLAN.md)),
excluding lab setup, credential acquisition and review latency. HYPOTHESIS throughout.

---

## 5. Branch naming

Proposed for later use. **None of these branches is created in this sprint.**

```
feature/core-platform-abstraction-v1     Wave 0
feature/linux-portability-v1             Wave 1
feature/debian-packaging-v1              Wave 2
feature/kde-wayland-v1                   Wave 3
feature/linux-distribution-v1            Wave 4
feature/windows-foundation-v1            Wave 5
feature/windows-clipboard-v1             Wave 6
feature/windows-ui-v1                    Wave 6
feature/macos-foundation-v1              Wave 7
feature/macos-clipboard-v1               Wave 8
feature/macos-ui-v1                      Wave 8
feature/ios-foundation-v1                Wave 9
poc/<poc-id>                             throwaway PoC branches, never merged
```

This matches the existing convention visible in the repository's history
(`feature/clipboard-v1`, `feature/file-transfer-v1`, `feature/visual-identity-v1`).

---

## 6. Certification gates

Every wave ends with the same four questions, in this order:

1. **Does the existing Android app still work, unmodified?** Backward compatibility with a
   shipped client is the one non-negotiable.
2. **Do all existing tests still pass, unmodified?** A test changed to accommodate a refactor is
   a specification changed without a decision.
3. **Is every security control either present or explicitly recorded as an accepted gap?**
   No silent weakening ([20](20-SECURITY-THREAT-ANALYSIS.md)).
4. **Does the UI advertise only what the platform can do?** ([03 §5](03-PLATFORM-CAPABILITY-MATRIX.md))

Plus the per-wave gate in §2.

---

## 7. Hardware and test lab

| Environment | VM sufficient? | Why |
| --- | --- | --- |
| Fedora GNOME Wayland | ✅ | Reference platform |
| Fedora KDE Plasma | ✅ | Clipboard needs a real Wayland session — **a container will not do**; a VM will |
| Debian 13 | ✅ | |
| Ubuntu 24.04 LTS | ✅ | Pins the libadwaita floor |
| Ubuntu 26.04 LTS / Kubuntu 26.04 | ✅ | The wl-clipboard 2.2.1 case |
| Windows 11 (x64) | ✅ **with a vTPM** | Hyper-V/QEMU can present a vTPM; verify the Platform Crypto Provider is actually available |
| Windows 11 with a **physical TPM** | ❌ **Physical required** | A vTPM is not the same trust story; at least one real machine for final certification |
| Windows 11 ARM64 | ⚠️ | Cross-compile in CI; a physical device only if ARM64 is promoted beyond secondary |
| **macOS Apple Silicon** | ❌ **Physical required** | Secure Enclave, notarization, App Store Connect. Apple licensing also constrains macOS VMs |
| macOS Intel | ⚠️ optional | Only while the universal binary is kept |
| **iPhone** | ❌ **Physical required** | POC-IOS-06 is invalid on the simulator, and invalid attached to Xcode |
| **iPad** | ❌ Physical | Multitasking behaviour differs meaningfully |
| **Android phone** | ❌ Physical | Already the case; note the device under test is an SM-X620 tablet, not the phone the briefs name |
| Second Android device | ⚠️ | For multi-peer clipboard and `origin_device_id` loop testing |
| A router where the Wi-Fi profile can be set Public/Private, and a guest network with AP isolation | ❌ Physical | POC-WIN-08 and POC-DISC-01's negative tests |

**Minimum viable lab: 5 VMs + 1 Windows machine with a real TPM + 1 Apple Silicon Mac +
1 iPhone + 1 iPad + 1 Android device.** The Mac is the single most expensive prerequisite and it
gates Waves 7–9 entirely.

---

## 8. CI implications (design only — no CI change this sprint)

| Can be automated in CI | Requires hardware/manual |
| --- | --- |
| Compile for every target | Clipboard read/write/watch on a real session |
| All portable unit and integration tests | Pairing with a real phone |
| Container builds (Fedora, Debian, Ubuntu) | TPM / Secure Enclave key generation |
| MSRV check (`rust:1.82`) | mDNS across a real network |
| `cargo clippy`, `cargo fmt`, `cargo audit` | Sleep/resume, network change |
| Package building (RPM, DEB, MSIX) | Notarization (needs credentials, though the *step* is scriptable) |
| Signing (with CI secrets) | App Review |
| Android unit tests | Android instrumented tests (device or emulator) |

Runners needed: `ubuntu-latest`, `windows-latest`, `macos-latest`. The last is the one with a
cost and a queue.

**The single most valuable CI addition** is a matrix build across `x86_64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc` and `aarch64-apple-darwin` for the *portable* crates, gated from Wave 0
onward. It turns "someone accidentally added a Unix-only call to `anyflow-core`" from a
discovery made months later into a failed PR. **CI-001.**

Second most valuable: the Ubuntu 24.04 libadwaita-floor job (**CI-003**), because that failure
is otherwise invisible until a user reports it.

---

## 9. What could change this order

| If… | Then… |
| --- | --- |
| POC-WIN-04 fails | Stop. Re-examine [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) before starting any platform |
| POC-IOS-06 shows sessions die in under a few seconds with an ugly desktop-side failure | Consider dropping Wave 9, or reducing it to iPadOS only |
| POC-KDE-01 shows KDE clipboard is broken on Ubuntu LTS | Wave 3 grows a mitigation; it does not block anything else |
| A contributor appears with macOS or Windows expertise | Waves 5–6 and 7–8 can run in parallel; nothing else changes |
| Apple Developer Program membership is not obtained | Waves 7–9 cannot ship at all. **Start this early** |
| Users ask for Windows loudly | The order already puts Windows first among new platforms |
