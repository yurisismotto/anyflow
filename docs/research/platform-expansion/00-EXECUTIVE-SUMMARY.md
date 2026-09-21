# 00 — Executive summary

| Field | Value |
| --- | --- |
| **Title** | Cross-platform expansion: consolidated findings |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Consolidated conclusions from documents [01](01-CURRENT-ARCHITECTURE-AUDIT.md)–[25](25-IMPLEMENTATION-BACKLOG.md). |
| **Decision status** | **No decision is Approved.** All recommendations are PROPOSED, POC REQUIRED or OPEN. |
| **Evidence** | Mixed; each claim carries its level in the source document. |
| **Related documents** | [README](README.md) — the index |

---

> **⚠ UPDATED by the verification sprint (2026-08-31).** Every conclusion below was tested against
> primary sources. **The headline survived; three specifics did not.** See
> [26 §14](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) for the full OLD → NEW → WHY table.
>
> | Was | Now |
> | --- | --- |
> | "12 items EXTERNAL VERIFICATION REQUIRED" | **8 VERIFIED, 2 partial, 2 still open, 0 refuted.** Neither open item blocks Wave 0 |
> | PLAT-DEC-001 POC REQUIRED | **READY FOR RFC.** The rustls seam is verified against 0.23.43; the refactor is **two call sites** |
> | "`filename.rs` knows nothing about Windows" | **REFUTED.** Device names, trailing dots and `\` are present and tested. Real gaps: `:` (ADS) and `U+202E` (bidi — **a Linux defect today**) |
> | "`wl-copy --sensitive` ✅ identical on Plasma" | **REFUTED.** It does not exist before wl-clipboard 2.3.0, so `sensitive_hint` clips **fail** on Debian 13 and every current Ubuntu LTS |
> | R-03 silent identity regeneration = future hardware risk | **A present-tense defect.** `Path::exists()` returns `false` on any metadata error, and `initialize()` then **overwrites `state.json`**, destroying the trust store |
> | macOS = "feasible with one compromise (polling)" | Polling confirmed — **and access is user-gated from macOS 15.4**, plus local-network privacy from macOS 15 |
> | Not previously noticed | **`unsafe_code = "forbid"` is workspace-wide** and blocks every future platform adapter |
>
> **Wave 0 is READY** — specification in [28](28-WAVE-0-IMPLEMENTATION-SPEC.md).

## 1. The headline

**OmniBridge is in a much better position for cross-platform expansion than its own documentation
suggests, and the reason is a series of decisions already made for other reasons.**

Four in particular:

1. **ECDSA P-256 was chosen for Android Keystore compatibility** ([ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md)).
   It turns out to be the *only* algorithm supported by the Apple Secure Enclave (which accepts
   256-bit EC keys and nothing else) and a first-class algorithm in Windows CNG. Ed25519 — the
   better primitive on paper, and the one a greenfield project would have picked — would have
   made hardware-backed identity impossible on three of the four hardware keystores OmniBridge will
   ever care about.
2. **`rustls` + `ring` instead of OpenSSL or a platform TLS stack.** The pinning verifiers, the
   TLS-1.3-only version list and the proof-of-possession check compile unchanged on every
   target. The single most security-critical code in the product does not need porting.
3. **`SessionHost`** (`core/src/session.rs:130`) already inverts every host dependency out of the
   protocol. A new platform implements a trait; it does not fork the session layer.
4. **The capability plugin model.** The transport contains no capability names, so a platform
   with partial support is not a protocol change.

Against that, one structural gap:

> **There is not a single `cfg(target_os)` in the entire Rust workspace.** OmniBridge does not have
> a platform boundary — it has Linux code that happens to be the only code. Creating that
> boundary is the whole of Wave 0, and it is small: **one file in `omnibridge-core`, one file in
> the files capability, and the control-socket transport.**

---

## 2. Feasibility by platform

| Platform | Feasibility | Effort | Main blockers | Priority | Confidence |
| --- | --- | --- | --- | --- | --- |
| **Linux (Debian/Ubuntu)** | **Very high** | S–M | Archive `rustc` on Ubuntu LTS; nothing else | **1st** | **High** — archive versions verified |
| **Linux (KDE Plasma)** | **Very high** | S | One question: does the packaged `wl-clipboard` speak the protocol KWin offers? | **2nd** | Medium-high |
| **Windows 10/11** | **High** | L | None structural. Filename rules and named-pipe hardening are must-fix defects | **3rd** | Medium-high — every hard problem has a first-party API, and `rustls-cng` already exists |
| **macOS** | **High, with one compromise** | L | Clipboard watching must poll; Developer ID + notarization are mandatory and need a Mac | **4th** | Medium |
| **iOS / iPadOS** | **Constrained** | XL | **Platform restriction:** the app is suspended shortly after backgrounding and its sockets may be reclaimed | **5th** | **Low** — gated on POC-IOS-06 |
| **Android** | Shipping | — | — | — | — |

### Highly viable

**Debian, Ubuntu and KDE Plasma.** Same binary, same protocol, different distribution and
desktop. Debian 13 (GTK 4.18.6, libadwaita 1.7.6, rustc 1.85) and Ubuntu 26.04 LTS
(4.22.4 / 1.9.0 / 1.93.1) exceed every requirement. **Ubuntu 24.04 LTS sits exactly on the
libadwaita 1.5 floor with zero margin** — supported, and one careless API call away from being
dropped.

**Windows.** Every hard problem has a documented first-party answer, and two are *better* than on
Linux: `AddClipboardFormatListener` is a real event-driven clipboard watch (Linux needs a
compositor protocol or an Xwayland bridge), and a named pipe can *authenticate* its caller
(a Unix socket relies on directory permissions).

### Partially viable

**macOS.** One genuine compromise: `NSPasteboard.changeCount` polling is the only change-detection
mechanism AppKit offers, and the current `ClipboardBackend` contract states "No implementation
may satisfy this by polling". That contract needs one sentence amended. The larger cost is not
technical: **Developer ID, notarization and a Mac in CI are mandatory**, not optional.

### Structurally constrained

**iOS / iPadOS.** Not "hard" — *restricted*. Four capability rows in
[03](03-PLATFORM-CAPABILITY-MATRIX.md) are hard platform restrictions:
automatic clipboard send, automatic clipboard receive, an always-connected session, and
background execution. iOS suspends the app shortly after backgrounding and may reclaim its
sockets while suspended (OFFICIAL DOC VERIFIED).

The honest product is **a foreground companion**: open it, it reconnects in about a second,
shows what is waiting, and lets you send from anywhere via the Share Sheet. That is coherent and
useful. It is not "your clipboard follows you around", and the UI must never imply that it is.

**The temptation to reach for APNs and a relay server to simulate an always-on connection is
rejected on principle** — it would put a third party in the trust path of a product whose first
principle is that there isn't one.

---

## 3. What is already multi-platform, and what needs adapters

### Already portable (no work)

- Wire format, framing, envelope, replay guard, sequence rules.
- TLS 1.3 configuration, SPKI pinning, both custom verifiers, proof of possession.
- Pairing proof construction and verification; the QR payload.
- Capability registry and negotiation; grant checks.
- `clipboard.v1` text rules, dedup and loop suppression.
- `files.v1` transfer state machine, data-stream MAC, limits.
- The discovery record model and TXT schema.
- `SessionHost` — the host-inversion seam.
- The design-token system, which is already tested against one JSON file on two platforms.

### Needs an adapter (the whole of the work)

| Concern | Seam | Status |
| --- | --- | --- |
| Clipboard read/write/watch | `ClipboardBackend` | **Already exists.** Well-designed. Add backends |
| Local battery source | `BatterySource` + a Cargo feature | **Already exists.** The template for the rest |
| Identity key storage and signing | `IdentitySigner` | **Must be created.** The one real refactor |
| State and secret-file storage | `StateStore` / `SecretFile` | Must be created |
| File destination and safe writing | `FileSink` | Must be created |
| Local IPC | `ControlTransport` | Must be created |
| Agent lifetime | per-platform host process | Concept, not code |
| UI | native per platform | By design |

---

## 4. The single deepest finding

`core/src/identity.rs` holds the private key as `key_pkcs8_der: Vec<u8>` and hands it to rustls.

That is correct for a software key and **structurally impossible for any hardware key**, whose
entire purpose is that those bytes cannot be obtained. Android already avoids this by not using
Rust; Windows and macOS cannot, if they use the shared core.

The fix is documented by rustls itself: implement `sign::SigningKey` and install it through
`ResolvesClientCert`/`ResolvesServerCert` — the extension point for keys living "in an HSM, or
in another process, or perhaps another machine". **`rustls-cng`, published under the rustls
GitHub organisation and targeting the rustls version this repository already pins, implements
exactly this for Windows CNG including ECDSA secp256r1.** The Apple equivalent does not exist
and must be written.

Everything else in the expansion is ordinary engineering. This is the piece that decides whether
[02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) is right, which is why **POC-WIN-04** is one of
the three PoCs that matter most.

---

## 5. Recommended order, and why

**Consolidate Linux → Windows → macOS → iOS**, with a core-seams wave in front.

| Wave | What | Size |
| --- | --- | --- |
| **0** | Core portability seams (identity signer, state store, file sink, control transport, crate split) | M |
| **1** | Linux portability — `gethostname`, `.desktop`, icons, autostart, package the GUI | S |
| **2** | Debian / Ubuntu | S–M |
| **3** | KDE Plasma | S |
| **4** | Linux packaging and distribution | S–M |
| **5** | Windows foundation — discovery, TPM identity, filename safety, named pipe | L |
| **6** | Windows capabilities and UI — clipboard, tray, WinUI, firewall, MSIX | L |
| **7** | macOS foundation — Enclave signer, `SMAppService`, notarization | L |
| **8** | macOS capabilities and UI | M–L |
| **9** | iOS / iPadOS constrained client | XL |

≈ **27–42 weeks** of engineering plus ≈ **71 engineer-days** of PoCs (HYPOTHESIS).

Two properties of this ordering are deliberate:

- **Waves 0–4 are a complete, shippable outcome.** At their end OmniBridge can honestly say it
  supports *Linux*, not *Fedora*. Nothing after that point is required for it. Given one
  maintainer and five platforms, having a legitimate stopping point is a feature.
- **Windows before macOS**, despite macOS needing fewer code changes, because `rustls-cng` makes
  Windows the cheapest place to prove the riskiest part of the architecture — and macOS then
  inherits a proven pattern instead of pioneering one.

### Best return per unit of effort

1. **KDE Plasma** — one afternoon of certification for a whole desktop environment, and
   plausibly a *better* clipboard than GNOME gets.
2. **Debian / Ubuntu** — packaging work only; multiplies the addressable users.
3. **Wave 0** — pays for itself three times over and is the difference between three clean ports
   and three messy ones.

---

## 6. PoCs that block decisions

Thirty-five PoCs are specified in [21](21-POC-MASTER-PLAN.md). Three decide the most:

| PoC | Decides | Why it is first |
| --- | --- | --- |
| **POC-IOS-06** — background suspension measurement | **PLAT-DEC-005**: whether an iOS product is worth building | Cheap (a device and a Linux daemon), and the only way to learn what backgrounding actually costs. Must run on a physical device, on battery, not attached to Xcode |
| **POC-WIN-04** — TLS + SPKI pinning with a TPM-held key | **PLAT-DEC-001**: whether the shared-core architecture works | If it fails, [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) needs rethinking *before* any platform work begins |
| **POC-KDE-01** — Plasma clipboard | Wave 3, and a real Ubuntu-LTS risk | Cheapest high-value result in the plan |

A rule applied throughout: **an identity PoC that proves the pieces separately proves nothing.**
Each must end in a completed handshake against the existing Linux daemon, with a negative test
confirming a wrong pin is rejected. OmniBridge has already been bitten by this class of error —
Android v1 Keystore keys were generated without `DIGEST_NONE` and were unusable for TLS client
authentication, a fact only a real handshake would have revealed, and by then the keystore
authorisations were immutable.

---

## 7. Major risks

| # | Risk | Why it matters |
| --- | --- | --- |
| 1 | **Wave 0 regresses identity or the trust store** | The refactor touches the code that decides trust. Mitigation: the existing tests are the specification and **no test may be modified to make a refactor pass** |
| 2 | **Filename rules omitted on Windows** | Reserved device names, alternate data streams (`a:b`) and trailing dots are currently unhandled, and `files.v1` accepts filenames from a paired-but-possibly-hostile peer. This is a defect, not a gap |
| 3 | **Silent identity regeneration** | If a TPM or Enclave becomes unreadable, the "no key yet" path would generate a new identity and break every pairing while looking like a hiccup |
| 4 | **iOS background limits make the product not worth shipping** | A legitimate outcome, and better discovered by POC-IOS-06 than after Wave 9 |
| 5 | **Scope creep — five platforms, one maintainer** | Mitigated by Waves 0–4 being a complete outcome |
| 6 | **Cloud or push creeping in to "fix" iOS** | Would change what OmniBridge is. Rejected explicitly and on the record |
| 7 | **libadwaita floor drifts above 1.5** | Silently drops Ubuntu 24.04 LTS, supported to 2029. Invisible without a CI job |
| 8 | **No Mac available** | Waves 7–9 cannot ship at all without one, plus an Apple Developer Program membership |

Full register: [23 §2](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md).

---

## 8. Open architectural decisions

Twelve, none decided here. The five that matter most:

| ID | Question | Recommended direction | Status |
| --- | --- | --- | --- |
| **PLAT-DEC-001** | One Rust core, or per-platform transports? | Portable core + native adapters | POC REQUIRED |
| **PLAT-DEC-002** | Windows agent or service? | **Agent.** Session 0 has no clipboard | PROPOSED (strong) |
| **PLAT-DEC-004** | Secure Enclave strategy | Custom `SigningKey`, **no user presence** | POC REQUIRED |
| **PLAT-DEC-005** | What do we promise on iOS? | Foreground companion. **No cloud, no push** | POC REQUIRED |
| **PLAT-DEC-009** | May a clipboard backend poll? | Amend the contract; declared polling only where no event source exists | PROPOSED |

---

## 9. What does not change

The compatibility and security principles hold, and this research found no reason to bend them:

- **No platform-specific protocols.** There must be no `clipboard.windows.v1`,
  `clipboard.macos.v1` or `files.ios.v1`. Every platform difference found is expressible as a
  different backend, an existing `ClipboardOutcome` value, local per-peer policy, or optional
  fail-closed metadata that is **deferred, not proposed for implementation**.
- **One protocol change is unavoidable and it is the mildest kind there is:** adding
  `PLATFORM_WINDOWS`, `PLATFORM_MACOS`, `PLATFORM_IOS` and `PLATFORM_IPADOS` to a proto3 enum
  whose field is presentational and never an authorization input. **PROPOSED**, not implemented.
- **The security model is untouched.** TLS 1.3 only; SPKI pinning as the sole identity check;
  hostnames and IPs are never identity; explicit human-confirmed pairing; discovery grants
  nothing; per-capability grants re-checked per message; `files.v1` never auto-granted; no
  clipboard content in logs; local-first, no account, no telemetry.
- **The expansion changes where keys live and how local IPC is guarded. It changes nothing about
  who is trusted or why.** That is the single acceptance criterion for the security half of this
  work.
- **No false parity.** A platform advertises only what it can do; the UI offers only what is
  advertised; a capability that is impossible is *absent*, not present-and-broken.

---

## 10. What this sprint produced

Twenty-six numbered documents plus an index, and no code. 81 backlog items, 35 PoC specifications, 12 open decisions, 16
risks, and 12 items explicitly flagged **EXTERNAL VERIFICATION REQUIRED** rather than guessed
([24 §7](24-SOURCE-BIBLIOGRAPHY.md)).

Nothing in `desktop/`, `android/`, `protocol/`, `packaging/` or `.github/` was modified.

## 11. What the verification sprint added

Three further documents, still no code:

- **[26 — External verification closeout](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)** — all twelve
  `V-nn` items taken to primary sources; two Research v1 claims refuted; four new defects and one
  architectural blocker found in the repository.
- **[27 — Architecture decision closeout](27-ARCHITECTURE-DECISION-CLOSEOUT.md)** — all twelve
  `PLAT-DEC` revisited plus three new ones. **10 READY FOR RFC, 3 RECOMMENDED, 2 POC REQUIRED, 0
  OPEN.** Risks re-scored; 37 PoCs reprioritised, of which **four are P0 and none blocks Wave 0
  from starting**.
- **[28 — Wave 0 implementation spec](28-WAVE-0-IMPLEMENTATION-SPEC.md)** — an implementable
  specification: six seams (and six rejected), nine PRs, ≈17 engineer-days, twelve acceptance
  gates, and **no hardware the project does not already have**.

**Declaration: WAVE 0 READY.**
