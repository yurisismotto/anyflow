# 19 — Packaging and distribution

| Field | Value |
| --- | --- |
| **Title** | Consolidated packaging, signing, update and architecture matrix |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | All platforms. Formats, signing, updates, CI artifacts, CPU architectures, uninstall and data retention. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for current packaging; OFFICIAL DOC VERIFIED for signing/notarization requirements. |
| **Related documents** | [07](07-LINUX-PACKAGING.md), [08 §11](08-WINDOWS-FEASIBILITY.md), [10 §10](10-MACOS-FEASIBILITY.md), [12 §7](12-APPLE-SECURITY-AND-INTEGRATION.md), [22](22-IMPLEMENTATION-ROADMAP.md) |

---

## 1. Consolidated matrix

| Platform | Primary | Secondary | Experimental | Not recommended |
| --- | --- | --- | --- | --- |
| **Fedora / RHEL / openSUSE** | RPM | tarball | Flatpak | AppImage |
| **Debian / Ubuntu** | DEB | tarball | Flatpak | Snap, AppImage |
| **Other Linux** | tarball | — | Flatpak | — |
| **Windows** | MSIX | winget | MSI / Inno installer | Microsoft Store (deferred) |
| **macOS** | signed + notarized DMG | Homebrew Cask | — | PKG, Mac App Store (deferred) |
| **Android** | APK (GitHub Releases) | Play Store (later) | — | — |
| **iOS/iPadOS** | App Store | TestFlight | — | *(no alternative exists)* |

---

## 2. Signing and trust

| Platform | Requirement | Cost / friction |
| --- | --- | --- |
| Linux RPM/DEB | GPG signing of packages/repos | Free; needs a key and a repo (COPR / OBS / a static repo) |
| Linux Flatpak | Flathub signs | Free; Flathub review |
| **Windows** | **Authenticode**, and MSIX must be signed | **Paid certificate.** Unsigned binaries trigger SmartScreen warnings that stop most users |
| **macOS** | **Developer ID + notarization, mandatory** | **Apple Developer Program, annual fee.** Since macOS 10.15 all Developer-ID software built after 2019-06-01 must be notarized (OFFICIAL DOC VERIFIED) |
| **iOS** | Apple Developer Program; App Review | Same membership; plus review latency |
| Android | Upload/app signing key | Free; key custody matters |

**Two of the five platforms cannot be distributed at all without paying and without a
platform-specific build machine.** That is the largest non-engineering constraint in this
expansion and it belongs in planning, not in a release-week surprise:

- macOS and iOS need **a Mac** for `codesign`, `notarytool`, `stapler`, Xcode and App Store
  Connect.
- Windows needs a code-signing certificate and, for reliable MSIX work, a Windows build agent.

---

## 3. CPU architectures

| Platform | Architecture | Recommendation | Rust tier |
| --- | --- | --- | --- |
| Linux | `x86_64` | **Required** | Tier 1 |
| Linux | `aarch64` | **Yes** — Raspberry Pi, ARM laptops, and cheap CI | Tier 1 |
| Linux | riscv64 | No | Tier 2 |
| Windows | `x86_64` | **Required** | **Tier 1** |
| Windows | `aarch64` | **Yes, secondary.** Windows on ARM is mainstream enough now | Tier 2 |
| macOS | `aarch64` (Apple Silicon) | **Required** | Tier 2 |
| macOS | `x86_64` (Intel) | **Universal binary while it is nearly free** | Tier 2 |
| Android | `arm64-v8a` | **Required** | n/a (Kotlin) |
| Android | `armeabi-v7a`, `x86_64` | x86_64 for the emulator only | n/a |
| iOS | `aarch64` | **Required** | Tier 2 |
| iOS Simulator | `aarch64-apple-ios-sim` | For development | Tier 2 |

(Tiers OFFICIAL DOC VERIFIED, *The rustc book*, Platform Support.)

On macOS Intel: build a universal binary (`lipo`) for as long as it costs one extra `cargo
build --target`. Drop it when the CI time stops being worth it — that is a metrics decision, not
a principle. Note that Android is pure Kotlin today, so no Rust targets are involved
([PLAT-DEC-010](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md)); that changes only if Android ever
consumes the Rust core.

---

## 4. Update strategy

| Platform | Mechanism | In-app updater? |
| --- | --- | --- |
| Linux RPM/DEB | `dnf` / `apt` from a repo | **No** |
| Linux tarball | Manual | No |
| Linux Flatpak | Flathub | No |
| Windows MSIX | App Installer / winget upgrade | **No** |
| Windows MSI | Manual or winget | No |
| macOS DMG | Manual, or Homebrew Cask | **No in v1.** Sparkle is the community standard if needed later |
| Android APK | Manual; Play Store later | No |
| iOS | App Store | n/a |

**Recommendation: no in-app auto-updater on any platform in v1.** An auto-updater in a process
that holds the identity key and the trust store is a high-value target
([20](20-SECURITY-THREAT-ANALYSIS.md), W6), and every platform already has a supported update
channel. A *notification* that a newer version exists is acceptable — but note that even a
version check is a network call to a server, which is a small dent in "no required cloud", so
it must be **opt-in and off by default**. **UX-009.**

---

## 5. Uninstall and data retention

An area where products routinely behave badly, and AnyFlow holds cryptographic identity and a
trust store — so it matters more than usual.

| Platform | Binaries | Config/state | Recommendation |
| --- | --- | --- | --- |
| Linux RPM/DEB | Removed by the package manager | `$XDG_DATA_HOME/anyflow` **survives** | Correct default. `apt purge` / an explicit `anyflow reset` removes it |
| Windows MSIX | Removed; per-user virtualised state removed with the package | `%LOCALAPPDATA%\AnyFlow` | **Prompt: "also remove your device identity and paired devices?"** |
| Windows MSI | Removed | Same | Same prompt |
| macOS DMG | Drag to Trash | `~/Library/Application Support/AnyFlow` **survives** | Ship an "AnyFlow → Reset identity" menu item; document the path |
| Android | Uninstall removes app data | Keystore key **is destroyed** | Correct: the identity cannot survive, so pairings are dead. Say so in the UI |
| iOS | Same | Same | Same |

Three rules:

1. **Never silently destroy an identity.** Losing it invalidates every pairing on every peer,
   and those peers will show a *revoked-looking* device with no explanation.
2. **Never silently retain it either.** A user who uninstalls expects their keys gone. Ask.
3. **The Windows agent must remove its firewall rules on uninstall.** A leftover inbound rule
   for a deleted program is exactly the kind of debris a security-conscious user will
   (correctly) hold against the product. **WIN-011.**

There is a related asymmetry worth documenting: on Android and iOS the identity is
hardware-bound and dies with the app; on the desktops it is a file (or a TPM/Enclave key) that
outlives an uninstall. Two different, both-defensible behaviours that will confuse users unless
the UI explains them.

---

## 6. CI artifacts (design only — no CI change this sprint)

| Job | Runner | Artifact | Signing |
| --- | --- | --- | --- |
| `linux-x86_64` | ubuntu-latest | tarball, RPM, DEB | GPG |
| `linux-aarch64` | ubuntu-latest (cross) or ARM runner | tarball | GPG |
| `windows-x64` | windows-latest | MSIX, MSI | Authenticode |
| `windows-arm64` | windows-latest (cross) | MSIX | Authenticode |
| `macos-universal` | **macos-latest** | .app, DMG | Developer ID + notarize + staple |
| `android` | ubuntu-latest | APK, AAB | Upload key |
| `ios` | **macos-latest** | .ipa | App Store Connect |

Secrets required: GPG key, Authenticode certificate + password, Apple Developer ID certificate
+ App Store Connect API key, Android keystore. **All of them are release-blocking and none is
an engineering task** — obtaining them takes calendar time and should start early.

---

## 7. Reproducibility and supply chain

| Practice | Status |
| --- | --- |
| `Cargo.lock` committed | ✅ REPO VERIFIED |
| `cargo build --locked` in the RPM | ✅ REPO VERIFIED |
| `cargo test --locked` in `%check` | ✅ (needs a headless gate — [07](07-LINUX-PACKAGING.md), PKG-006) |
| Gradle version catalog pinned | ⚠️ `libs.versions.toml` carries a comment that the pins *"have NOT been resolved"* against a real SDK — a pre-existing item, unrelated to this expansion but worth closing before adding platforms |
| Publish checksums per release | **Missing.** Add |
| SBOM (`cargo auxiliary`/`cargo-cyclonedx`) | **Missing.** Worth having for a security-positioned product |
| Dependency review / `cargo audit` in CI | **Missing.** Should exist before the dependency surface triples |

`cargo audit` and an SBOM matter more after this expansion than before it: `rustls-cng`, Apple
security bindings, `windows-sys` and a UniFFI toolchain all get added to a currently tight
dependency set. **CI-004**, **CI-005**.

---

## 8. Recommended direction

1. **System packages first on Linux** (RPM, DEB), tarball everywhere, Flatpak experimental.
2. **MSIX + winget on Windows**, x64 and ARM64, Authenticode-signed.
3. **Signed, notarized DMG + Homebrew Cask on macOS.** Defer the App Store.
4. **App Store + TestFlight on iOS.** There is no alternative.
5. **No in-app auto-updaters.** Optional, opt-in update *notification* at most.
6. **Ask before deleting identity on uninstall; remove firewall rules.**
7. **Start the signing-credential paperwork early** — it is calendar time, not engineering time.
8. **Add checksums, SBOM and `cargo audit` before the dependency surface grows.**
