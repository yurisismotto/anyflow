# AnyFlow — Cross-platform expansion research

**One flow. Any device.**
**One protocol. One security model. Multiple platform adapters.**

| Field | Value |
| --- | --- |
| **Status** | Research / Draft — planning only, no implementation |
| **Last reviewed** | 2026-08-31 |
| **Branch** | `research/platform-expansion-v1` |
| **Scope** | Linux (generic, Fedora, Debian, Ubuntu, GNOME, KDE Plasma, Wayland, X11), Windows 10/11, macOS, Android, iOS/iPadOS |
| **Decision status** | **Nothing here is Approved.** All decisions are PROPOSED, POC REQUIRED, or OPEN |

---

## What this is

The answer to one question:

> *What architecture does AnyFlow need in order to support Linux multi-distro, GNOME, KDE
> Plasma, Windows, macOS, Android and iOS/iPadOS without duplicating protocol, security and
> product logic?*

It is research, an architecture audit, a feasibility study and a plan. **No source file outside
this directory was modified.** No protocol, TLS, pairing or capability behaviour was changed. No
PoC was implemented. No branch was created. Nothing was committed.

**Start with [00 — Executive summary](00-EXECUTIVE-SUMMARY.md).**

---

## Dashboard

| Area | Status | One-line finding |
| --- | --- | --- |
| **Linux portability** | **RESEARCHED** | Almost nothing is Fedora-specific. The real constraint is the GTK 4.12 / libadwaita 1.5 floor, and it only affects the GUI |
| **Debian / Ubuntu** | **READY** | Debian 13 and Ubuntu 26.04 exceed every requirement. **Ubuntu 24.04 LTS sits exactly on the libadwaita 1.5 floor** |
| **KDE Plasma** | **POC REQUIRED** | The code already has a KWin-capable clipboard path that GNOME cannot use. One question about `wl-clipboard` versions |
| **Windows** | **RESEARCHED** | Highly feasible. Agent, not service. `rustls-cng` already solves the hard part |
| **macOS** | **RESEARCHED** | Feasible with one contract amendment (polling) and one hard cost (notarization + a Mac) |
| **iOS / iPadOS** | **POC REQUIRED** | Structurally constrained. A foreground companion, or nothing. Gated on POC-IOS-06 |
| **Packaging** | **RESEARCHED** | System packages first. Flatpak experimental, not primary — AnyFlow is closer to a system agent than an application |
| **Security** | **RESEARCHED** | The model survives intact. Three items are defects that must be fixed before a platform ships |
| **PoCs** | **POC REQUIRED** | 35 specified, ≈71 engineer-days, none implemented |

Status vocabulary: **READY** (evidence complete, work can be planned) · **RESEARCHED**
(analysis complete, some verification outstanding) · **POC REQUIRED** (a decision cannot be made
from documentation) · **BLOCKED** · **DEFERRED**.

---

## Index

| # | Document | Purpose | Status | Evidence | Depends on | Wave |
| --- | --- | --- | --- | --- | --- | --- |
| 00 | [Executive summary](00-EXECUTIVE-SUMMARY.md) | Consolidated findings and priorities | Research / Draft | Mixed | 01–25 | — |
| 01 | [Current architecture audit](01-CURRENT-ARCHITECTURE-AUDIT.md) | What is portable today; the exact compile blockers | Research / Draft | **REPO VERIFIED** | — | 0 |
| 02 | [Target architecture](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) | Where the shared/native boundary belongs, and alternatives rejected | Research / Draft | REPO + OFFICIAL DOC | 01 | 0 |
| 03 | [Platform capability matrix](03-PLATFORM-CAPABILITY-MATRIX.md) | What each platform can actually do | Research / Draft | Mixed, per cell | 01 | all |
| 04 | [Linux portability](04-LINUX-PORTABILITY.md) | Fedora detail vs. Linux requirement | Research / Draft | REPO + OFFICIAL DOC | 01 | 1 |
| 05 | [Debian / Ubuntu compatibility](05-DEBIAN-UBUNTU-COMPATIBILITY.md) | Archive-version audit against the API surface used | **READY** | **OFFICIAL DOC VERIFIED** | 04 | 2 |
| 06 | [KDE Plasma / Wayland](06-KDE-PLASMA-WAYLAND.md) | Clipboard, GUI, and whether GTK stays | POC REQUIRED | REPO + OFFICIAL DOC | 04, 05 | 3 |
| 07 | [Linux packaging](07-LINUX-PACKAGING.md) | RPM, DEB, tarball, Flatpak, AppImage, Snap | Research / Draft | REPO + OFFICIAL DOC | 04, 05 | 4 |
| 08 | [Windows feasibility](08-WINDOWS-FEASIBILITY.md) | Toolchain, discovery, identity, clipboard, agent, UI, packaging | Research / Draft | **OFFICIAL DOC VERIFIED** | 02 | 5–6 |
| 09 | [Windows security and integration](09-WINDOWS-SECURITY-AND-INTEGRATION.md) | Control mapping, IPC, multi-user, attack surface | Research / Draft | OFFICIAL DOC + REPO | 08 | 5–6 |
| 10 | [macOS feasibility](10-MACOS-FEASIBILITY.md) | Rust, Bonjour, Enclave, pasteboard, agent, UI, packaging | Research / Draft | OFFICIAL DOC (partial) | 02 | 7–8 |
| 11 | [iOS / iPadOS feasibility](11-IOS-IPADOS-FEASIBILITY.md) | What an iOS client can honestly be | POC REQUIRED | OFFICIAL DOC (partial) | 10 | 9 |
| 12 | [Apple security and integration](12-APPLE-SECURITY-AND-INTEGRATION.md) | Keychain, Enclave, entitlements, sandbox, signing, FFI | Research / Draft | OFFICIAL DOC (partial) | 10, 11 | 7–9 |
| 13 | [Cross-platform discovery](13-CROSS-PLATFORM-DISCOVERY.md) | Keeping `_anyflow._tcp.local.` working everywhere | Research / Draft | REPO + OFFICIAL DOC | 01 | 5, 7, 9 |
| 14 | [Identity and key storage](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) | Where the key lives, and how it reaches TLS | Research / Draft | REPO + OFFICIAL DOC | 02, 09, 12 | 0, 5, 7 |
| 15 | [Cross-platform clipboard](15-CROSS-PLATFORM-CLIPBOARD.md) | One `clipboard.v1`, six backends | Research / Draft | REPO + OFFICIAL DOC | 03, 06, 08, 10, 11 | 0, 6, 8, 9 |
| 16 | [Cross-platform files](16-CROSS-PLATFORM-FILES.md) | `files.v1` on five platforms | Research / Draft | REPO + OFFICIAL DOC | 03, 11 | 0, 5, 9 |
| 17 | [Background execution model](17-BACKGROUND-EXECUTION-MODEL.md) | The AnyFlow Agent: one concept, five lifetimes | Research / Draft | REPO + OFFICIAL DOC | 08, 10, 11 | all |
| 18 | [UI platform strategy](18-UI-PLATFORM-STRATEGY.md) | Native toolkits, one identity, no Electron | Research / Draft | REPO + OFFICIAL DOC | 06, 08, 10 | 6, 8, 9 |
| 19 | [Packaging and distribution](19-PACKAGING-AND-DISTRIBUTION.md) | Consolidated formats, signing, updates, architectures | Research / Draft | OFFICIAL DOC VERIFIED | 07, 08, 10, 12 | 4, 6, 8, 9 |
| 20 | [Cross-platform threat analysis](20-SECURITY-THREAT-ANALYSIS.md) | New surface for existing attackers | Research / Draft | REPO + OFFICIAL DOC | 09, 12, 14 | all |
| 21 | [PoC master plan](21-POC-MASTER-PLAN.md) | 35 PoCs, none implemented | **POC REQUIRED** | n/a — creates requirements | 01–20 | all |
| 22 | [Implementation roadmap](22-IMPLEMENTATION-ROADMAP.md) | Waves, gates, hardware, CI | Research / Draft | Derived | 01–21 | — |
| 23 | [Risks, questions, decisions](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) | Decision register — 12 open decisions, 16 risks | Research / Draft | Per decision | 01–22 | — |
| 24 | [Source bibliography](24-SOURCE-BIBLIOGRAPHY.md) | Every source, with access dates and the verification list | Research / Draft | n/a — is the trail | — | — |
| 25 | [Implementation backlog](25-IMPLEMENTATION-BACKLOG.md) | 81 items, 18 of them P0 | Research / Draft | Per item | 01–23 | all |

---

## Reading paths

**"Should we do this at all?"**
→ [00](00-EXECUTIVE-SUMMARY.md) → [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) → [22](22-IMPLEMENTATION-ROADMAP.md)

**"I am about to start Wave 0."**
→ [01](01-CURRENT-ARCHITECTURE-AUDIT.md) → [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) → [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) → [25](25-IMPLEMENTATION-BACKLOG.md) (ARCH-\*, SEC-\*)

**"I am implementing Windows."**
→ [08](08-WINDOWS-FEASIBILITY.md) → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) → [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) → [15](15-CROSS-PLATFORM-CLIPBOARD.md) → [21](21-POC-MASTER-PLAN.md) (POC-WIN-\*)

**"I am implementing macOS or iOS."**
→ [10](10-MACOS-FEASIBILITY.md) → [11](11-IOS-IPADOS-FEASIBILITY.md) → [12](12-APPLE-SECURITY-AND-INTEGRATION.md) → [17](17-BACKGROUND-EXECUTION-MODEL.md)

**"I am packaging for a distribution."**
→ [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md) → [07](07-LINUX-PACKAGING.md) → [19](19-PACKAGING-AND-DISTRIBUTION.md)

**"I care about the security story."**
→ [20](20-SECURITY-THREAT-ANALYSIS.md) → [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) → [12](12-APPLE-SECURITY-AND-INTEGRATION.md) → [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md)

---

## Conventions

**Evidence levels.** Every substantive conclusion carries one:

| Level | Meaning |
| --- | --- |
| **REPO VERIFIED** | Read from this repository's source at commit `f7a0015` |
| **OFFICIAL DOC VERIFIED** | From vendor, upstream-project or distribution documentation, cited in [24](24-SOURCE-BIBLIOGRAPHY.md) |
| **POC REQUIRED** | Cannot be settled from documentation |
| **HYPOTHESIS** | Reasoned, unverified. Effort estimates are all of this kind |
| **BLOCKED** | Cannot proceed without an external input |
| **EXTERNAL VERIFICATION REQUIRED** | A source could not be retrieved in this session. **Listed in full in [24 §7](24-SOURCE-BIBLIOGRAPHY.md)** — twelve items, none of them guessed |
| **NOT APPLICABLE** | — |

**Identifiers.**

| Prefix | Meaning | Where defined |
| --- | --- | --- |
| `PLAT-DEC-nnn` | Architectural decision | [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |
| `R-nn` | Risk | [23 §2](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |
| `Q-nn` | Open question, not yet a decision | [23 §3](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |
| `X-nn` | Cross-platform threat | [20](20-SECURITY-THREAT-ANALYSIS.md) |
| `V-nn` | External verification needed | [24 §7](24-SOURCE-BIBLIOGRAPHY.md) |
| `AUD-nn` | Audit finding | [01 §8](01-CURRENT-ARCHITECTURE-AUDIT.md) |
| `POC-*` | Proof of concept | [21](21-POC-MASTER-PLAN.md) |
| `ARCH/LINUX/KDE/PKG/WIN/MAC/IOS/SEC/PROTO/UX/CI-nnn` | Backlog item | [25](25-IMPLEMENTATION-BACKLOG.md) |

---

## Constraints this research operated under

Recorded so a future reader knows what was and was not permitted.

- Branch `research/platform-expansion-v1` only. No other branch created.
- No new platform support implemented.
- Protocol, TLS, pairing and existing capabilities untouched.
- Daemon, GUI, Android app and clipboard backend read only.
- No refactor. No commit. No push. No CI change.
- `docs/security/THREAT_MODEL.md` **not** amended — [20](20-SECURITY-THREAT-ANALYSIS.md) is
  research that would inform a future amendment, made in its own change.
- The only change permitted, and the only change made: Markdown documentation under
  `docs/research/platform-expansion/`.

---

## Where the existing documentation lives

This research reads from, and defers to, the project's existing documents:

- [`docs/architecture/`](../../architecture/) — OVERVIEW, PROTOCOL, CLIPBOARD, FILES
- [`docs/security/THREAT_MODEL.md`](../../security/THREAT_MODEL.md)
- [`docs/adr/`](../../adr/) — ADR-0001 … ADR-0014
- [`docs/design/`](../../design/) — BRAND, UI-GUIDELINES, `tokens.json`

Where this research and an existing document disagree, **the existing document is authoritative
and this one is a proposal.**
