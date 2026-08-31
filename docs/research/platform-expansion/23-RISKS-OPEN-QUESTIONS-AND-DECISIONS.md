# 23 — Risks, open questions and decision register

| Field | Value |
| --- | --- |
| **Title** | Decision register and risk register for the platform expansion |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Every architectural decision this research surfaces but does not make. |
| **Decision status** | **No decision in this document is Accepted or Approved.** All are PROPOSED, POC REQUIRED, or OPEN, as the sprint requires. |
| **Evidence** | Per decision. |
| **Related documents** | all |

---

## 1. Decision register

> **⚠ SUPERSEDED STATUSES.** Every decision below was revisited against verified evidence in
> **[27 — Architecture decision closeout](27-ARCHITECTURE-DECISION-CLOSEOUT.md)**, which is now
> authoritative for status. Summary: **9 READY FOR RFC · 3 RECOMMENDED · 3 POC REQUIRED · 0 OPEN**,
> plus three new decisions (**PLAT-DEC-013/014/015**) the verification forced. The text below is
> retained as the original statement of each question.
>
> Note also a drafting error inherited from Research v1:
> [02 §4](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) cites "PLAT-DEC-007" for the Windows UI↔agent
> IPC question. **This register is authoritative** — PLAT-DEC-007 is Flatpak viability; the IPC
> seam belongs to PLAT-DEC-001.

Status vocabulary: **PROPOSED** (a direction is recommended, awaiting a decision) ·
**POC REQUIRED** (cannot be decided from documentation) · **OPEN** (no recommendation yet).

---

### PLAT-DEC-001 — One Rust core, or per-platform transport implementations?

- **Why it matters** Decides whether pairing, pinning and framing are implemented twice (as
  today, Rust + Kotlin) or five times. Those are the functions where a divergence is a
  vulnerability, not a defect.
- **Options** (a) portable Rust core + native adapters everywhere new; (b) native
  reimplementation per platform, as Android did; (c) hybrid — Rust for desktops, native for
  mobile.
- **Evidence** REPO VERIFIED: `anyflow-core` is already host-inverted via `SessionHost` and is
  one file from compiling off-Unix ([01 §5](01-CURRENT-ARCHITECTURE-AUDIT.md)). OFFICIAL DOC
  VERIFIED: `x86_64-pc-windows-msvc` Tier 1, `*-apple-darwin`/`*-apple-ios` Tier 2.
- **Recommended** **(a)**, contingent on the identity signer working.
- **Status** **POC REQUIRED** — POC-CORE-02, POC-WIN-04.
- **Blocking?** **Yes.** Everything after Wave 0.
- **PoC** POC-CORE-01/02, POC-WIN-04, POC-MAC-04.

### PLAT-DEC-002 — Windows agent or Windows Service?

- **Why it matters** Determines whether the clipboard can work at all on Windows, and how large
  the privileged attack surface is.
- **Options** (a) user-session agent; (b) Windows Service; (c) both.
- **Evidence** OFFICIAL DOC VERIFIED: services run in Session 0, which is non-interactive and
  has no user desktop; Interactive Services Detection was removed in Windows 10 1803. The
  clipboard is per-window-station.
- **Recommended** **(a).** (b) cannot access the user's clipboard; (c) adds a privileged process
  and a cross-session IPC channel for a capability — running while nobody is logged in — that a
  device-continuity product does not need.
- **Status** **PROPOSED**, strong.
- **Blocking?** Yes, for Wave 5.
- **PoC** POC-WIN-06.

### PLAT-DEC-003 — GTK on KDE, or a future Qt/Kirigami frontend?

- **Why it matters** Whether KDE is a certification target or a second GUI codebase.
- **Options** (a) GTK everywhere on Linux; (b) add Kirigami for Plasma; (c) a toolkit-neutral
  Linux GUI.
- **Evidence** REPO VERIFIED: the GUI is a 203-line client over a JSON socket plus views — there
  is no shared logic to lose. libadwaita hard-codes Adwaita and ignores GTK themes by design, so
  a foreign appearance on Plasma is permanent, not fixable.
- **Recommended** **(a)** for now. Revisit if KDE gains real users or a contributor offers
  Kirigami.
- **Status** **OPEN.**
- **Blocking?** No.
- **PoC** POC-KDE-02.

### PLAT-DEC-004 — Secure Enclave integration strategy

- **Why it matters** Decides whether Apple platforms get hardware-backed identity, and whether
  the FFI shape is viable.
- **Options** (a) custom `rustls::sign::SigningKey` over `SecKeyCreateSignature`; (b) do TLS in
  Swift with Network.framework and reimplement pinning there; (c) software key in the Keychain.
- **Evidence** OFFICIAL DOC VERIFIED: the Enclave supports only P-256, which AnyFlow already
  uses; rustls documents `SigningKey` for exactly this. No `rustls-secure-enclave` crate exists —
  it must be written. (b) would mean a second implementation of the pinning verifier, which
  `tls.rs` calls "the single most dangerous thing in this codebase".
- **Recommended** **(a)**, with **(c)** as an explicit, visible fallback. Reject (b).
- **Status** **POC REQUIRED** — POC-MAC-03, POC-MAC-04.
- **Blocking?** Yes, for Waves 7–9.
- **Sub-decision, already settled by physics:** the key must be created **without** user
  presence, because `Signer::sign` is synchronous and handshakes are unattended. This is
  immutable after key creation, so it must be right the first time.

### PLAT-DEC-005 — What do we promise on iOS?

- **Why it matters** Decides whether an iOS product exists, and prevents a cloud dependency
  being introduced to simulate one.
- **Options** (a) foreground companion, no background claims; (b) APNs + a relay to wake the
  app; (c) no iOS client; (d) iPadOS only.
- **Evidence** OFFICIAL DOC VERIFIED: iOS suspends shortly after backgrounding and may reclaim
  sockets while suspended. No `UIBackgroundModes` value legitimately covers a LAN control
  session.
- **Recommended** **(a).** **(b) is rejected on principle** — it contradicts local-first, no
  required cloud, no third party in the trust path — and must not be reintroduced as a quiet
  implementation detail. (d) is a reasonable reduced scope.
- **Status** **POC REQUIRED** — POC-IOS-06 measures the actual cost and decides whether (a) is a
  good enough product.
- **Blocking?** Yes, for Wave 9 — and Wave 9 only.

### PLAT-DEC-006 — Capability metadata in the protocol?

- **Why it matters** Without it, an iOS peer advertising `clipboard.v1` is indistinguishable
  from a Linux desktop, and users will enable automation that can never fire.
- **Options** (a) `CapabilityProperties` map in `HELLO`; (b) infer from `DeviceInfo.platform`;
  (c) UI-only inference from observed behaviour; (d) nothing.
- **Evidence** REPO VERIFIED: capability ids "either match or they don't"; `platform` is
  attacker-supplied and currently only selects an icon.
- **Recommended** **Defer.** Ship **(c)** as the interim; design **(a)** so it is ready.
  **Reject (b)** — it would turn a cosmetic, attacker-controlled field into a behavioural input.
- **Status** **OPEN**, design only. If ever implemented: additive proto3 field, **fail-closed**
  (absent ⇒ assume nothing extra), never an authorization input, capability ids unchanged, and
  `PROTOCOL_VERSION_MAX` moves.
- **Blocking?** No.

### PLAT-DEC-007 — Flatpak viability

- **Why it matters** Whether Flatpak can be a supported Linux channel or only an experiment.
- **Options** (a) primary; (b) experimental alongside RPM/DEB; (c) not supported.
- **Evidence** OFFICIAL DOC VERIFIED: Flatpak has no `.local` NSS resolution and no mDNS portal.
  **But AnyFlow does not resolve `.local` names** — it runs its own responder and dials the IP
  addresses in the record, so the documented gap may not apply. The real costs are
  `--socket=x11` (required for the GNOME clipboard watch) and a second-class CLI.
- **Recommended** **(b)**, POC-gated.
- **Status** **POC REQUIRED** — POC-LINUX-03.
- **Blocking?** No.

### PLAT-DEC-008 — Add `Platform` enum values for Windows, macOS, iOS?

- **Why it matters** It is the only protocol change this research finds unavoidable.
- **Options** (a) add `PLATFORM_WINDOWS`, `PLATFORM_MACOS`, `PLATFORM_IOS`, `PLATFORM_IPADOS`;
  (b) reuse `PLATFORM_UNSPECIFIED`; (c) a free-text field.
- **Evidence** REPO VERIFIED: proto3 enums decode unknown values as the raw integer; the field
  is presentational (`UiMapping.kt`, `views/`) and is never an authorization input.
- **Recommended** **(a).** Additive, backward-compatible; an old peer shows a generic icon.
  Reject (b) — it would make new desktops indistinguishable from broken ones. Reject (c) — a
  free-text field invites parsing.
- **Status** **PROPOSED.** **Not implemented in this sprint.**
- **Blocking?** Yes for polish in Wave 5; not for function.
- **Open sub-question** whether iPadOS is its own value or reports `PLATFORM_IOS`. Recommendation:
  its own value, since the capability profiles genuinely differ.

### PLAT-DEC-009 — May a clipboard backend poll?

- **Why it matters** Decides whether macOS gets automatic clipboard send.
- **Options** (a) amend the contract to allow declared polling where no event source exists;
  (b) keep the ban and give macOS manual-only clipboard; (c) private API.
- **Evidence** REPO VERIFIED: `backend/mod.rs` currently states "No implementation may satisfy
  this by polling." OFFICIAL DOC VERIFIED: `NSPasteboard.changeCount` is the documented change
  mechanism; AppKit publishes no change notification.
- **Recommended** **(a).** The rule's purpose — do not burn power on a timer when an event
  source exists — is preserved, because on macOS none exists. Reject (c) absolutely.
- **Status** **PROPOSED.** A documentation change in `backend/mod.rs`, **not** a protocol change.
- **Blocking?** Yes, for Wave 8.
- **PoC** POC-MAC-05.

### PLAT-DEC-010 — Does Android move to the Rust core?

- **Why it matters** It is the largest available "increase code sharing" move, and the one with
  the worst risk/reward.
- **Options** (a) keep Kotlin; (b) migrate transport/security to Rust via JNI/UniFFI; (c) new
  capabilities in Rust, existing ones in Kotlin.
- **Evidence** REPO VERIFIED: a complete, tested Kotlin implementation with hardware-backed
  Keystore identity wired through Conscrypt, plus ~20 test files. Migration would re-solve the
  hardest problem on that platform in a harder way and deliver nothing users can see.
- **Recommended** **(a).** Rewrite a working platform implementation only when the duplication
  has actually caused a defect.
- **Status** **OPEN**, recommendation strong.
- **Blocking?** No.

### PLAT-DEC-011 — Debian packaging: vendored or unbundled crates?

- **Why it matters** Decides whether official Debian archive inclusion is realistic.
- **Options** (a) vendor with `Cargo.lock`, ship from CI / a PPA / OBS; (b) unbundle to
  `librust-*` and pursue archive inclusion.
- **Evidence** OFFICIAL DOC VERIFIED: Debian policy prefers unbundled crates. AnyFlow depends on
  `rustls 0.23`, `rcgen 0.14`, `tokio 1.53`, `prost 0.14`, `mdns-sd 0.15`, `gtk4 0.9`,
  `libadwaita 0.7` and more; the security-critical ones are exactly where a version substitution
  matters most.
- **Recommended** **(a)** first. Revisit (b) only if a Debian maintainer takes it on.
- **Status** **PROPOSED.**
- **Blocking?** No.

### PLAT-DEC-012 — Do peers learn how a key is stored?

- **Why it matters** Tempting UI feature; poor security property.
- **Options** (a) add `key_backing` to `DeviceInfo`; (b) local display only.
- **Evidence** It is an unverifiable self-report. A UI decoration that looks like a security
  property is worse than none.
- **Recommended** **(b).** No protocol change. Show the *local* backing in `anyflow status` and
  the UI.
- **Status** **PROPOSED.**
- **Blocking?** No.

---

## 2. Risk register

> **⚠ UPDATED.** Re-scored in [27 §6](27-ARCHITECTURE-DECISION-CLOSEOUT.md), where the three
> defects (R-06, R-07, R-08) get concrete failure scenarios, mitigations, owning wave and tests.
> Headline changes: **R-08 rises to High/Critical** — it is a **present-tense defect on Linux**,
> not a future hardware risk. **R-02 and R-07 fall.** Three new risks: **R-17** (`--sensitive`
> unavailable), **R-18** (`unsafe_code = "forbid"` blocks adapters), **R-19** (macOS clipboard
> access user-gated).

| ID | Risk | Likelihood | Impact | Mitigation | Owner doc |
| --- | --- | --- | --- | --- | --- |
| R-01 | Wave 0 introduces a regression in identity or the trust store | Medium | **Critical** | Existing tests are the spec; **no test may be modified to make a refactor pass**; POC-CORE-02 | [22](22-IMPLEMENTATION-ROADMAP.md) |
| R-02 | The Apple signer fails on a signature-encoding detail | Medium | High | POC-MAC-04 ends in a real handshake, not a unit test | [12](12-APPLE-SECURITY-AND-INTEGRATION.md) |
| R-03 | iOS background limits make the product not worth shipping | **High** | Medium (scope) | POC-IOS-06 first; be willing to ship iPadOS only, or nothing | [11](11-IOS-IPADOS-FEASIBILITY.md) |
| R-04 | `mdns-sd` cannot coexist with `mDNSResponder` on macOS | Medium | Medium | Budget the `dnssd` fallback into Wave 7 rather than treating it as a surprise | [13](13-CROSS-PLATFORM-DISCOVERY.md) |
| R-05 | KDE auto-send broken on Ubuntu LTS by wl-clipboard 2.2.1 | Medium | Low–Medium | POC-KDE-01; depend on wl-clipboard ≥ 2.3 in packaging; improve the status message | [06](06-KDE-PLASMA-WAYLAND.md) |
| R-06 | Named-pipe squatting ships unmitigated | Low | **High** | `FILE_FLAG_FIRST_PIPE_INSTANCE` + per-SID DACL; POC-WIN-07 includes a hostile squatter | [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| R-07 | Windows filename rules omitted; path/ADS attack from a hostile peer | Medium | **High** | SEC-004 in Wave 0 or 5, applied on **all** platforms | [16](16-CROSS-PLATFORM-FILES.md) |
| R-08 | Silent identity regeneration after a TPM/Enclave becomes unreadable | Medium | **High** | SEC-009 — distinguish "no key" from "key unusable"; refuse to start | [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) |
| R-09 | Apple Developer Program / signing credentials not obtained in time | Medium | High (schedule) | Start the paperwork before Wave 7 | [19](19-PACKAGING-AND-DISTRIBUTION.md) |
| R-10 | No Mac available for CI | Medium | High | Waves 7–9 are blocked without one. Identify the machine before committing to them | [22](22-IMPLEMENTATION-ROADMAP.md) |
| R-11 | libadwaita floor drifts above 1.5 unnoticed, dropping Ubuntu 24.04 LTS | **High** | Medium | CI-003: build the GUI against noble's libadwaita | [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md) |
| R-12 | Scope creep: five platforms, one maintainer | **High** | High | Waves 0–4 are a complete, shippable outcome on their own. Stopping there is a success | [22](22-IMPLEMENTATION-ROADMAP.md) |
| R-13 | Cloud/push creeps in to "fix" iOS | Medium | **Critical (product identity)** | PLAT-DEC-005 records the rejection and the reasoning | [11](11-IOS-IPADOS-FEASIBILITY.md) |
| R-14 | Capability parity is forced, producing broken toggles on constrained platforms | Medium | Medium | The product rule in [03 §5](03-PLATFORM-CAPABILITY-MATRIX.md) is a certification gate | [03](03-PLATFORM-CAPABILITY-MATRIX.md) |
| R-15 | Sandboxing gap on Windows/macOS misrepresented as parity | Low | Medium | SEC-005: record it as an accepted, named gap | [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) |
| R-16 | Two responders on one host cause intermittent, hard-to-diagnose discovery failures | Medium | Medium | Make 5353 coexistence a release gate per platform | [13](13-CROSS-PLATFORM-DISCOVERY.md) |

---

## 3. Open questions not yet decisions

Things this research could not settle and that are not yet shaped as decisions.

| # | Question | Why it is open | Next step |
| --- | --- | --- | --- |
| ~~Q-01~~ | Is a C toolchain genuinely required (`ring`)? | **ANSWERED (V-10)** — upstream: *"ring currently requires a C (but not C++) toolchain."* Whether anything *else* also needs `gcc` remains for POC-LINUX-01 |
| ~~Q-02~~ | Does KWin still expose `wlr-data-control`? | **ANSWERED (V-02)** — no, from Plasma 6.5. Both up to 6.4 |
| ~~Q-03~~ | Which wl-clipboard release added `ext-data-control`? | **ANSWERED (V-01)** — 2.3.0, 2026-03-22 |
| Q-04 | Does `DnsServiceRegister` publish A/AAAA records? | **PARTIAL (V-03)** — the API accepts addresses; on-wire behaviour undocumented. Contingent on `mdns-sd` failing first → POC-WIN-02 |
| ~~Q-05~~ | Exact Windows clipboard-format names? | **ANSWERED (V-04)** — three formats, semantics confirmed |
| Q-06 | Does macOS prompt when a background agent reads the pasteboard? | **REFRAMED (V-06)** — yes by default from macOS 15.4. Residual: does `changeCount` polling *alone* prompt? → POC-MAC-05 |
| ~~Q-07~~ | Authoritative `UIBackgroundModes` list? | **ANSWERED (V-07)** — eleven values; none fits |
| Q-08 | Does an iOS Share Extension inherit the app's local-network permission? | **PARTIAL (V-09)** — in general yes; the background-undetermined case remains → POC-IOS-08 |
| Q-09 | Should the desktop hold a pending clip for a disconnected peer, and for how long? | Product question, not technical | Design during Wave 9 (UX-008) |
| Q-10 | Should iPadOS be its own `Platform` enum value? | Capability profiles differ; identity does not | Decide with PLAT-DEC-008 |
| Q-11 | Is `Endpoints.kt`'s ordering logic worth moving into the shared Rust core? | It is pure, tested, and would otherwise be written a third time for iOS | ARCH-007, decide in Wave 9 |

---

## 4. Explicitly not decided here

For the avoidance of doubt, and because the sprint forbids it:

- No decision is Accepted or Approved.
- `docs/security/THREAT_MODEL.md` is **not** amended.
- No ADR is created, superseded or edited.
- No protocol change is implemented. `PLATFORM_WINDOWS` and friends remain **PROPOSED**.
- No branch from [22 §5](22-IMPLEMENTATION-ROADMAP.md) is created.
- No PoC is implemented.
