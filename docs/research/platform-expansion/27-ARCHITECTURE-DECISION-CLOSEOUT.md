# 27 — Architecture decision closeout

| Field | Value |
| --- | --- |
| **Title** | Decision closeout: every `PLAT-DEC` revisited against verified evidence |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Sprint** | `research/platform-expansion-verification-v1` |
| **Scope** | The twelve decisions in [23 §1](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md), three new ones the verification forced, the sixteen risks, and the PoC plan. |
| **Decision status** | **Nothing here is Approved or Accepted.** The strongest status this sprint may assign is **READY FOR RFC**. |
| **Evidence** | [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) throughout, plus the repository at `7bb0cc4`. |
| **Related documents** | [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) — the original register · [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) · [28](28-WAVE-0-IMPLEMENTATION-SPEC.md) |

---

## 1. Status vocabulary

Per the brief, `APPROVED` and `ACCEPTED` are **not available** in this sprint. We are not writing
ADRs yet.

| Status | Meaning |
| --- | --- |
| **READY FOR RFC** | Evidence is sufficient. Someone can write the ADR without doing more research |
| **RECOMMENDED** | A direction is recommended, but a PoC must confirm it before an ADR |
| **POC REQUIRED** | The decision genuinely cannot be made until a PoC runs |
| **OPEN** | No recommendation yet |
| **REJECTED OPTION** | Applies to an option within a decision, not to the decision |

---

## 2. Summary

| ID | Decision | v1 status | **New status** | Δ |
| --- | --- | --- | --- | :-: |
| PLAT-DEC-001 | Portable Rust core vs. per-platform | POC REQUIRED | **READY FOR RFC** | ⬆ |
| PLAT-DEC-002 | Windows agent vs. service | PROPOSED | **READY FOR RFC** | ⬆ |
| PLAT-DEC-003 | GTK on KDE vs. Qt frontend | OPEN | **READY FOR RFC** | ⬆ |
| PLAT-DEC-004 | Secure Enclave strategy | POC REQUIRED | **RECOMMENDED** | ⬆ |
| PLAT-DEC-005 | What we promise on iOS | POC REQUIRED | **POC REQUIRED** | = |
| PLAT-DEC-006 | Capability metadata in the protocol | OPEN | **RECOMMENDED** | ⬆ |
| PLAT-DEC-007 | Flatpak viability | POC REQUIRED | **POC REQUIRED** | = |
| PLAT-DEC-008 | `Platform` enum values | PROPOSED | **READY FOR RFC** | ⬆ |
| PLAT-DEC-009 | May a clipboard backend poll? | PROPOSED | **READY FOR RFC** | ⬆ |
| PLAT-DEC-010 | Android → Rust core? | OPEN | **READY FOR RFC** (keep Kotlin) | ⬆ |
| PLAT-DEC-011 | Debian vendored vs. unbundled | PROPOSED | **READY FOR RFC** | ⬆ |
| PLAT-DEC-012 | Do peers learn key backing? | PROPOSED | **READY FOR RFC** | ⬆ |
| **PLAT-DEC-013** | `wl-copy --sensitive` unavailable | — | **READY FOR RFC** | 🆕 |
| **PLAT-DEC-014** | Filename rules: global or per-destination | — | **READY FOR RFC** | 🆕 |
| **PLAT-DEC-015** | Software→hardware identity migration | — | **RECOMMENDED** | 🆕 |

**Ten READY FOR RFC · three RECOMMENDED · two POC REQUIRED · zero OPEN.**

A note on the numbering conflict inherited from Research v1: **[02 §4](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md)
refers to "PLAT-DEC-007" for the Windows UI↔agent IPC question, while
[23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) defines PLAT-DEC-007 as Flatpak viability.** The
register is authoritative; doc 02's reference is a drafting error and is corrected to point at
PLAT-DEC-001, which is where the IPC seam actually lives. No new ID is minted for it.

---

## 3. The decisions

### PLAT-DEC-001 — One portable Rust core, or per-platform implementations?

| Field | Value |
| --- | --- |
| **Question** | Do pairing, pinning, framing and session logic exist once in Rust, or once per platform? |
| **Options** | (a) portable core + native adapters; (b) native reimplementation per platform; (c) hybrid — Rust desktops, native mobile |
| **v1 evidence** | `SessionHost` already inverts host dependencies; core is "one file from compiling off-Unix" |
| **v1 status** | POC REQUIRED — gated on the identity signer working at all |

**New evidence.** The gate has been removed by documentation rather than by a PoC.

1. rustls 0.23.43 — the pinned version — exposes `SigningKey`/`Signer` and
   `with_cert_resolver`/`with_client_cert_resolver` at exactly the builder states `tls.rs` uses.
   `CertifiedKey::new(certs, Arc<dyn SigningKey>)` needs no PKCS#8 bytes
   ([26 §10](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)).
2. The consumer surface is **two `pub(crate)` call sites** (`tls.rs:288`, `tls.rs:342`), not a
   diffuse dependency ([26 §11.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)).
3. `rustls-cng` v0.7.1, under the rustls org, actively maintained, rustls 0.23, ECDSA P-256,
   including the IEEE-P1363→DER conversion Windows requires — verified in source.
4. Apple's `SecKeyCreateSignature` with `.ecdsaSignatureMessageX962SHA256` is synchronous, hashes
   internally, and returns X9.62 DER — matching `Signer::sign`'s contract exactly.

The architectural question "can a non-exportable key drive our TLS stack through a trait?" is
**answered yes, from primary sources, for both hardware platforms**. What remains unproven is
*integration*, not *architecture* — and integration failures do not invalidate (a); they are
ordinary bugs.

**Two new constraints** that must be in the RFC:

- `unsafe_code = "forbid"` is workspace-wide and `forbid` cannot be locally overridden. Every
  adapter needs `unsafe`. The lint must move to per-crate scope, keeping `forbid` on
  `omnibridge-core` and the capability crates ([26 §11.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)).
- `ring` requires a C toolchain, and MSVC on Windows. The Wave 0 acceptance gate cannot be a bare
  Linux cross-compile ([26 §V-10](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)).

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** — portable core + native adapters. (c) survives as the *outcome* for Android via PLAT-DEC-010, not as a separate architecture |
| **Rejected options** | **(b) REJECTED** — five implementations of the pairing proof and the pinning verifier is a liability, not a cross-check |
| **Risks** | R-01 (Wave 0 regresses identity/trust). Mitigation: existing tests are the specification; no test may be modified to make the refactor pass |
| **PoC dependency** | **None for the decision.** POC-CORE-01/02 become Wave 0 *acceptance gates*, not prerequisites |
| **Implementation dependency** | ARCH-001, ARCH-002, ARCH-003, ARCH-010 |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-002 — Windows agent or Windows Service?

| Field | Value |
| --- | --- |
| **Options** | (a) user-session agent; (b) Windows Service; (c) both |
| **v1 status** | PROPOSED (strong) |

**New evidence — first-party and decisive.** learn.microsoft.com, *Interactive Services*:

> **Services cannot directly interact with a user as of Windows Vista.**
> By default, services use a **noninteractive window station**…
> **All services run in Terminal Services session 0.**

The Windows clipboard is a per-window-station object. A service's noninteractive station is not
the logged-in user's `WinSta0`, and `NoInteractiveServices` defaults to 1, so
`SERVICE_INTERACTIVE_PROCESS` is inert. **(b) cannot read the user's clipboard.** This is no longer
an inference from architecture; it is the vendor saying so.

Microsoft's recommended shape is the one OmniBridge already implements on Linux: a per-session
process, talking to the rest over named-pipe IPC, registered per-session via `Run`, with per-session
pipe names. Two verbatim lines matter for [28](28-WAVE-0-IMPLEMENTATION-SPEC.md):

> Note that IPC can expose your service interfaces over the network unless you use an appropriate
> access control list (ACL).
> …the server can distinguish between multiple user processes by giving each pipe a unique name
> based on the session ID.

**New constraint.** `AddClipboardFormatListener` requires an `HWND` and a message pump
([26 §9.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). The agent must own a message-only window and run
a message loop beside the tokio runtime. Filed as WIN-010. This *reinforces* (a): a message pump
needs an interactive window station, which Session 0 does not provide.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** — user-session agent |
| **Rejected options** | **(b) REJECTED** — cannot access the clipboard, verified. **(c) REJECTED** — adds a privileged Session 0 process and a cross-session channel to buy "runs while logged out", which a device-continuity product does not need |
| **Risks** | R-06 (named-pipe squatting) — see PLAT-DEC-014's neighbour, SEC-003 |
| **PoC dependency** | None for the decision. POC-WIN-06 validates lifecycle; POC-WIN-05 validates the message pump |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-003 — GTK on KDE, or a Qt/Kirigami frontend?

| Field | Value |
| --- | --- |
| **v1 status** | OPEN |

**New evidence.** No new *toolkit* evidence — but the KDE picture is no longer uncertain, and that
was the reason the decision was open. [26 §6](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) resolves the
clipboard question from primary sources: KDE works everywhere except one distribution
configuration, for a reason that is a packaging mismatch and is self-correcting.

The case for GTK is therefore stronger than in v1, and for a reason v1 could not have known: **the
real KDE problems turned out not to be GUI problems at all.** They were `wl-clipboard` version
problems, which a Kirigami frontend would not have touched. Spending the first KDE effort on a
second GUI would have addressed none of the three defects this sprint found.

The revisit conditions from [06 §7](06-KDE-PLASMA-WAYLAND.md) stand unchanged and are good ones.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** — GTK stays. KDE is a **certification target, not a port** |
| **Rejected options** | **(b) REJECTED FOR NOW**, not on merit — revisit if KDE gains real users, or a contributor offers Kirigami. **(c) REJECTED** — a toolkit-neutral Linux GUI is a third codebase pretending to be a saving |
| **Risks** | Cosmetic only. libadwaita ignores GTK themes by design, so foreign appearance on Plasma is permanent |
| **PoC dependency** | POC-KDE-02 validates; it does not decide |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-004 — Secure Enclave integration strategy

| Field | Value |
| --- | --- |
| **Options** | (a) custom `SigningKey` over `SecKeyCreateSignature`; (b) TLS in Swift with Network.framework, pinning reimplemented; (c) software key in the Keychain |
| **v1 status** | POC REQUIRED |

**New evidence** ([26 §13](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)), all verbatim from Apple:

- *"Works only with NIST P-256 elliptic curve keys"* — OmniBridge's algorithm, verified not inferred.
- *"Not having a mechanism to transfer plain-text key data into or out of the Secure Enclave is
  fundamental to its security"* — confirms the seam is mandatory.
- `SecKeyCreateSignature` is synchronous; `.ecdsaSignatureMessageX962SHA256` hashes internally and
  returns X9.62 DER — matching `Signer::sign`'s "message is not pre-hashed" contract and its
  expected output encoding **exactly**. This is the single most encouraging technical fact in the
  Apple half of the expansion: no re-encoding layer is needed, unlike Windows.
- *"By specifying the [`.privateKeyUsage`] flag… Without the flag, key generation still succeeds,
  but signing operations that attempt to use it fail."*

Three findings change the shape of the decision without changing its direction:

1. **`.userPresence` is confirmed impossible**, not merely inadvisable — `Signer::sign` is
   synchronous and non-async in 0.23.43. The sub-decision v1 called "settled by physics" is now
   settled by documentation.
2. **`.privateKeyUsage` is the Android `DIGEST_NONE` trap again**: generation succeeds, signing
   fails later, flags immutable. MAC-002 must name it.
3. **Intel Macs without T1/T2 have no Enclave.** So **(c) is not an alternative to (a) — it is a
   mandatory companion to it.** That reframes (c) from "fallback we hope not to use" into "a code
   path that will certainly run on real hardware", which raises the importance of PLAT-DEC-015.

**Why still RECOMMENDED and not READY FOR RFC.** Every *component* is verified, but the one thing
no document establishes is that a certificate built around an Enclave public key yields an SPKI
byte-identical to what `Fingerprint::from_certificate_der` extracts. That is the trust anchor, and
OmniBridge has already been burned by exactly this class of "the pieces worked separately" error on
Android. POC-MAC-03 + POC-MAC-04 must end in a completed pinned handshake.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** custom `SigningKey`, with **(c)** as a mandatory, visible fallback. Key created `AfterFirstUnlockThisDeviceOnly` + `.privateKeyUsage`, **no** `.userPresence` |
| **Rejected options** | **(b) REJECTED** — a second implementation of the pinning verifier, which `tls.rs` itself calls "the single most dangerous thing in this codebase" |
| **Risks** | R-02 (signature-encoding detail) — reduced, since Apple returns the encoding rustls wants. New: no software→hardware migration path (PLAT-DEC-015) |
| **PoC dependency** | **POC-MAC-03, POC-MAC-04** — must end in a real handshake plus a negative wrong-pin test |
| **Status** | **RECOMMENDED** |

### PLAT-DEC-005 — What do we promise on iOS?

| Field | Value |
| --- | --- |
| **Options** | (a) foreground companion; (b) APNs + relay; (c) no iOS client; (d) iPadOS only |
| **v1 status** | POC REQUIRED |

**New evidence.**

- **V-07 closed.** The eleven `UIBackgroundModes` values are `audio`, `location`, `voip`, `fetch`,
  `remote-notification`, `external-accessory`, `bluetooth-central`, `bluetooth-peripheral`,
  `processing`, `push-to-talk`, `nearby-interaction`. None describes a LAN control session. v1's
  hedge — "the conclusion does not depend on the edges" — is discharged.
- **V-05 closed, with one genuinely encouraging result.** TN3179's operation table says
  **"Listening for and accepting incoming TCP connections: no"** — an iOS app that only *listens*
  needs no Local Network privilege. It is Bonjour (register/browse/resolve) and *outgoing*
  connections that do. This does not rescue background operation, but it means the permission
  story is narrower than feared, and it shapes what POC-IOS-02 should measure.
- Multicast entitlement: required for arbitrary service types and for browsing all types. OmniBridge
  browses one fixed type declared in `NSBonjourServices`, so it is probably **not** needed —
  probably, because TN3179 does not state the boundary precisely. POC-IOS-01 must confirm.
- Share Extension inherits the container app's privilege (V-09), with an undetermined-state edge
  that onboarding must handle.

**Why this stays POC REQUIRED, and why that is correct.** Everything above is about *permission*.
The decision turns on *duration* — how long a TLS session actually survives backgrounding on a real
device, on battery, not attached to Xcode. No Apple document states it, because it is not a
contract; it is an emergent property of the scheduler. **POC-IOS-06 remains the gate**, and it is
the right kind of unknown to spend a PoC on.

The documentation has, however, closed off every alternative to measuring: there is no background
mode to apply for, and no entitlement that changes the answer.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** foreground companion — pending POC-IOS-06 |
| **Rejected options** | **(b) REJECTED ON PRINCIPLE.** It puts a third party in the trust path of a product whose first principle is that there is not one. Not a technical rejection and not revisitable on technical grounds — see §5. **(d)** is a legitimate reduced scope, not a rejection |
| **Risks** | R-03 (limits make it not worth shipping) — a legitimate outcome. R-13 (cloud creeps in) |
| **PoC dependency** | **POC-IOS-06** (blocking), POC-IOS-01/02 (scoping) |
| **Status** | **POC REQUIRED** — and it blocks Wave 9 only |

### PLAT-DEC-006 — Capability metadata in the protocol?

| Field | Value |
| --- | --- |
| **Question** | Should `can_send_manual` / `can_send_auto` / `can_receive` / `can_auto_apply` / `background_receive` be inferred locally, declared as metadata, negotiated, or a combination? |
| **Options** | (a) `CapabilityProperties` in `HELLO`; (b) infer from `DeviceInfo.platform`; (c) UI-only inference; (d) nothing |
| **v1 status** | OPEN, "defer" |

**New evidence** makes the problem concrete rather than hypothetical, and it is no longer only
about iOS:

| Platform | A capability it advertises but cannot always deliver | Source |
| --- | --- | --- |
| iOS | auto-send, auto-receive, background session | V-07 |
| **macOS 15.4+** | **auto-send — user-gated by `NSPasteboard.AccessBehavior`, default ask** | V-06 |
| **Ubuntu 26.04 LTS + Plasma** | **auto-send — no common data-control protocol** | [26 §6](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) |
| **Debian 13 / all Ubuntu LTS** | **`sensitive_hint` honouring — `wl-copy` too old** | [26 §5.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) |

The last two are the important ones. **A capability's availability is not a property of the
platform; it is a property of the running installation.** Two Ubuntu 26.04 machines, one on GNOME
and one on Plasma, differ. The same machine differs before and after a `wl-clipboard` upgrade.

This kills option (b) outright — already rejected in v1, now for a second and stronger reason — and
it substantially weakens (a). A static `CapabilityProperties` map declared at `HELLO` would either
be recomputed per connection (in which case it is dynamic state on a handshake, with all the
staleness that implies) or be wrong.

**Recommended: (D) — a combination, but not the one the brief's option list suggests.**

1. **Local runtime capability detection is the primary mechanism, and it already exists.**
   `detect_watch_source()` probes rather than assumes; `probe_data_control()` runs the real tool
   and observes. That pattern — *ask the system, do not infer from a label* — is correct and should
   be extended (to `--sensitive` support, and to `NSPasteboard.accessBehavior` on macOS).
2. **Surface it locally, in `omnibridge clipboard status` and the UI.** A user must be able to learn
   that auto-send is off *and why*, naming the missing protocol or the System Settings toggle.
3. **Do not put it on the wire yet.** Design (a) so it is ready, keep it deferred.

**Why deferring is the right call and not a dodge.** The problem the wire format would solve is
"the *peer's* user sees a toggle that cannot work". That is real but second-order, and every
platform difference found in two sprints is expressible as an existing `ClipboardOutcome` value or
local policy. Meanwhile the *local* half — a user who cannot find out why their own machine is not
auto-sending — is a live defect on Ubuntu 26.04 today, needs no protocol change, and is
KDE-001/LINUX-010 in the backlog.

If (a) is ever implemented, the constraints from v1 stand and are unchanged: additive proto3 field,
**fail-closed** (absent ⇒ assume nothing extra), never an authorization input, capability ids
unchanged, `PROTOCOL_VERSION_MAX` moves.

| Field | Value |
| --- | --- |
| **Recommended** | **(D) combination** — local runtime detection + honest local reporting now; wire metadata designed, deferred |
| **Rejected options** | **(b) REJECTED** — `platform` is attacker-supplied and presentational; making it behavioural would be a security regression. **(d) REJECTED** — doing nothing leaves the Ubuntu 26.04 defect unexplained to users |
| **Risks** | R-14 (forced parity produces broken toggles) |
| **Backward compatibility** | No protocol change ⇒ no compatibility surface. Old peers unaffected |
| **Status** | **RECOMMENDED** (design only; maximum permitted this sprint, and it still depends on POC-MAC-05 for the macOS row) |

### PLAT-DEC-007 — Flatpak viability

| Field | Value |
| --- | --- |
| **v1 status** | POC REQUIRED |

**New evidence** — none directly, but two adjacent results matter.

The `--sensitive` finding ([26 §5.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)) shows OmniBridge's
clipboard path depends on the *version of an external binary on `PATH`*. Inside a Flatpak sandbox
that binary is the runtime's, not the host's — which cuts both ways: the runtime version is
**predictable** (an advantage), but it is **not the host's**, so a Flatpak OmniBridge could honour
`sensitive_hint` on a host whose own `wl-copy` is too old, or fail on a host where it would have
worked. Either way it is a *third* wl-clipboard version to reason about.

Combined with v1's finding that the GNOME clipboard watch needs `--socket=x11`, the case for
"experimental, not primary" strengthens. OmniBridge is closer to a system agent than an application,
and its clipboard path is unusually sensitive to what is on `PATH`.

| Field | Value |
| --- | --- |
| **Recommended** | **(b)** — experimental alongside RPM/DEB. Not primary |
| **Rejected options** | **(a) REJECTED** — a background agent with a `--socket=x11` requirement and a second-class CLI is not a good primary channel |
| **PoC dependency** | **POC-LINUX-03** — extended to also record which `wl-clipboard` the runtime provides |
| **Status** | **POC REQUIRED** |

### PLAT-DEC-008 — Add `Platform` enum values for Windows, macOS, iOS, iPadOS?

| Field | Value |
| --- | --- |
| **v1 status** | PROPOSED |

**New evidence.** Re-audit confirms the premise and adds one thing v1 missed: `Platform::Linux` is
**hardcoded** at `store.rs:154` and `store.rs:188`, in both `initialize()` and `load()`. The value
is decided by the storage layer, so adding enum values is necessary but not sufficient — the
adapter must be able to supply it. Small, and much cheaper to fix in Wave 0 than to discover in
Wave 5. Filed as **ARCH-011**.

The compatibility analysis is unchanged and correct: proto3 decodes unknown enum values as the raw
integer, and the field is presentational (`UiMapping.kt`, `views/`), never an authorization input.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** add `PLATFORM_WINDOWS`, `PLATFORM_MACOS`, `PLATFORM_IOS`, `PLATFORM_IPADOS`. iPadOS gets its own value — the capability profiles genuinely differ (closes Q-10) |
| **Rejected options** | **(b) REJECTED** — reusing `UNSPECIFIED` makes a new desktop indistinguishable from a broken one. **(c) REJECTED** — a free-text field invites parsing |
| **Risks** | None. Additive; an old peer shows a generic icon |
| **Implementation dependency** | ARCH-011. **Not implemented in this sprint** — no protobuf was touched |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-009 — May a clipboard backend poll?

| Field | Value |
| --- | --- |
| **v1 status** | PROPOSED |

**New evidence, and it converts an argument-from-absence into a documented negative.**

Research v1 could only say "AppKit publishes no change notification" as an inference. This sprint
scanned the **complete `NSPasteboard` symbol reference** (the full class JSON, 98 KB — every
member, not a rendered page) for `notification`, `observ`, `kvo`, `publisher`, `didchange`:
**zero occurrences of any of them.** Combined with `changeCount`'s own documentation — *"record the
value of `changeCount`… and compare it with a later value"* — the conclusion is now evidenced
rather than assumed.

The contrast with Windows sharpens the rule rather than dissolving it. Microsoft explicitly says of
its sequence number: *"this is **not** a notification method and **should not be used in a polling
loop**"* — and offers `AddClipboardFormatListener` instead. So the platforms divide cleanly:

| Platform | Event source | Polling |
| --- | --- | --- |
| Linux (data-control) | `wl-paste --watch` | forbidden |
| Linux (X11/Xwayland) | XFIXES | forbidden |
| Windows | `WM_CLIPBOARDUPDATE` | forbidden — vendor says so |
| Android | `ClipboardManager` listener | forbidden |
| **macOS** | **none exists** | **permitted, declared** |

The rule's *purpose* — do not burn power on a timer when an event source exists — is preserved
exactly. The amendment names the one platform where the antecedent is false.

**A new condition, from V-06.** On macOS 15.4+, programmatic pasteboard access is user-gated
(`.default` ⇒ ask). So the amended contract must require a backend that polls to **also** report
when it is polling *and being denied* — otherwise macOS auto-send fails silently, which is the
failure mode [26 §6](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) criticises on Ubuntu. Proposed contract
text for `backend/mod.rs`:

> No implementation may satisfy this by polling **unless the platform provides no event-driven
> change source at all**, in which case the implementation must declare that it polls, must poll
> the cheapest available change indicator rather than reading content, and must report a distinct
> outcome when the platform denies it access rather than reporting "no change".

macOS satisfies this: `changeCount` is a cheap integer, and content is read only when it moves.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** amend the contract, as above |
| **Rejected options** | **(b) REJECTED** — macOS manual-only is a worse product for no benefit. **(c) REJECTED absolutely** — no private API |
| **Risks** | Low. A documentation change in `backend/mod.rs`, **not** a protocol change |
| **PoC dependency** | **POC-MAC-05** — but its question has changed. It must now measure *whether `changeCount` polling alone triggers the access alert, or only the subsequent content read* |
| **Status** | **READY FOR RFC** for the contract amendment. The macOS *product* claim (auto-send silent by default) stays gated on POC-MAC-05 |

### PLAT-DEC-010 — Does Android move to the Rust core?

| Field | Value |
| --- | --- |
| **v1 status** | OPEN, recommendation strong |

**New evidence** strengthens "keep Kotlin" from a cost argument to a technical one.

`Signer::sign` receives an **unhashed** message and must apply the scheme's hash itself
([26 §10](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). On Android that maps to
`Signature.getInstance("SHA256withECDSA")`, which hashes internally — *the exact thing OmniBridge's v1
Keystore keys could not do*, because they were created with `DIGEST_NONE`, and the authorisations
are immutable. A Rust-over-JNI signer would re-enter that minefield to reach a place Kotlin already
occupies safely, with hardware-backed Keystore identity already wired through Conscrypt and ~20
passing test files.

The rule v1 proposed is a good one and this sprint found no reason to bend it: **rewrite a working
platform implementation only when the duplication has actually caused a defect.** Two sprints of
auditing have found no protocol divergence between the Rust and Kotlin implementations.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** keep Kotlin |
| **Rejected options** | **(b) REJECTED** — re-solves the hardest problem on that platform in a harder way and delivers nothing a user can see. **(c) REJECTED** — a per-capability language split is the worst of both |
| **Open sub-item** | Q-11 (`Endpoints.kt` ordering into the shared core) stays open, decided in Wave 9 (ARCH-007) |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-011 — Debian packaging: vendored or unbundled?

| Field | Value |
| --- | --- |
| **v1 status** | PROPOSED |

**New evidence.** Two facts firm this up.

- `ring` requires a C toolchain (V-10), so `gcc` in `Build-Depends` is confirmed necessary, not
  vestigial. One less unknown in the `debian/control` draft.
- MSRV is **1.82** (`desktop/Cargo.toml`, `rust-version.workspace = true`). Against the archives:
  Debian trixie `rustc` 1.85 ✅; Ubuntu 24.04 default `rustc` **1.75** ❌ but the versioned
  `rustc-1.82`…`rustc-1.91` packages exist ✅; Ubuntu 26.04 `rustc` 1.93.1 ✅. So Ubuntu 24.04
  builds only against a *named* toolchain package — which is exactly what v1's ⚠️ meant, now tied
  to a concrete MSRV read from the manifest rather than to an assumption.

The vendoring argument is unchanged and remains right: the probability that trixie's `librust-*`
set simultaneously satisfies `rustls 0.23`, `rcgen 0.14`, `tokio 1.53`, `prost 0.14`,
`mdns-sd 0.15`, `gtk4 0.9` and `libadwaita 0.7` is low, and the security-critical ones are exactly
where a substitution matters most.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** vendor with `Cargo.lock`, ship a `.deb` from CI. `Build-Depends: rustc (>= 1.82) \| rustc-1.82` |
| **Rejected options** | **(b) DEFERRED, not rejected** — revisit only if a Debian maintainer takes it on |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-012 — Do peers learn how a key is stored?

| Field | Value |
| --- | --- |
| **v1 status** | PROPOSED — local display only |

**New evidence** makes the *local* half more important without changing the wire answer.

Because the Enclave cannot import preexisting keys, and because Intel Macs have no Enclave at all,
a real population of OmniBridge installs will run software-backed keys on machines the user assumes
are hardware-backed. The local display therefore stops being a nicety.

The argument against putting it on the wire is unchanged and correct: it is an **unverifiable
self-report**. A peer claiming `key_backing = SecureEnclave` proves nothing — the claim is made by
the same software that would be lying. A UI decoration that looks like a security property is worse
than no decoration.

| Field | Value |
| --- | --- |
| **Recommended** | **(b)** local display only. Show backing in `omnibridge status` and the UI; no protocol change |
| **Rejected options** | **(a) REJECTED** — unverifiable self-report presented as a security property |
| **Implementation dependency** | ARCH-002 must expose `KeyBacking`; SEC-002 verifies protection per backing |
| **Status** | **READY FOR RFC** |

---

## 4. New decisions forced by the verification

### PLAT-DEC-013 — What happens when `wl-copy` cannot honour `sensitive_hint`? 🆕

| Field | Value |
| --- | --- |
| **Question** | On wl-clipboard < 2.3.0, `wl-copy --sensitive` exits 1 and the clip is not written at all. Fail-closed with an explanation, or fall back to a plain write with a warning? |
| **Why it exists** | [26 §5.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) — affects Debian 13 and every current Ubuntu LTS, on every desktop |
| **Options** | (a) probe once; on old `wl-copy`, refuse the clip and tell the user why; (b) probe once; write without the flag and warn prominently; (c) leave as-is (unexplained failure); (d) require wl-clipboard ≥ 2.3 in packaging |

**Evidence.** `wl-copy` 2.2.1's option table has no `sensitive` entry and its unknown-option path
is `print_usage(stderr); exit(1)`. OmniBridge maps a non-zero exit to `BackendError::Failed`
(`backend/wayland.rs:310-315`). The current behaviour is therefore (c) by accident: the clip fails,
and nothing explains it.

**Recommended: (a).** Reasoning:

- The current *direction* of failure is right. A `sensitive_hint` clip is, by construction, the one
  the user least wants persisted in Klipper's history. Silently downgrading to a plain write —
  option (b) — turns a visible failure into an invisible privacy regression, which is the exact
  trade the security principle forbids.
- What is wrong is the *explanation*, not the outcome. The user must be told: the clip was refused
  because this system's `wl-copy` is too old to mark it sensitive, and the remedy is a
  `wl-clipboard` upgrade.
- The probe belongs beside `probe_data_control()`, which already establishes the pattern of asking
  the tool rather than assuming.

**(d) is rejected on the strength of V-11**: Fedora ships `2.2.1^git20251124` which *has* the flag,
so a `>= 2.3` dependency would exclude a working system. Version numbers are the wrong instrument;
the probe is the right one. Packaging should *recommend* wl-clipboard ≥ 2.3 and say why.

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** — probe, fail closed, explain, name the remedy |
| **Rejected options** | **(b) REJECTED** — silent privacy downgrade. **(c) REJECTED** — status quo defect. **(d) REJECTED** — refuted by V-11 |
| **Risks** | R-17 (new) |
| **Implementation dependency** | LINUX-010 (P0, Wave 2) |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-014 — Filename sanitisation: protocol-global or destination-specific? 🆕

| Field | Value |
| --- | --- |
| **Question** | The brief asks it directly. Should `filename.rs` apply one conservative rule set everywhere, or rules chosen per destination filesystem? |
| **Options** | (a) protocol-global conservative — one rule set, all platforms; (b) destination-specific — `#[cfg]` or runtime per target filesystem; (c) global floor + destination-specific additions |

**Evidence** ([26 §7](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)):

- The current code is **already** (a), deliberately, and its doc comment says why: *"Nothing here
  runs on Windows today, but a received file lands in a directory that is routinely shared over SMB
  or synced, and this costs one table lookup."*
- Research v1's claim that the Windows rules are missing is **refuted** — device names, trailing
  dots/spaces and `\` are all handled and tested.
- The genuine gaps are `:` (alternate data streams) and `U+202E` (bidi override). **The second is
  not a Windows issue at all** — a bidi-spoofed filename misleads a user on Linux exactly as well.

**Recommended: (a), unchanged — and the evidence for it is stronger than the brief assumed.**

Three arguments, in increasing order of force:

1. **A received file does not stay on the receiving filesystem.** It gets synced, shared over SMB,
   copied to a USB stick, or backed up to a NAS. Sanitising for the filesystem the bytes first land
   on optimises for the least interesting moment in the file's life.
2. **Destination-specific rules put a security control behind a `#[cfg]` arm.**
   [02 §3](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) rejects that pattern for exactly the right
   reason: an arm that does not match the CI host is unbuilt and untested. `filename.rs` has 15
   test functions today; splitting it per-platform would leave most of them running on one target.
3. **The one gap that matters most is platform-independent.** If bidi override were treated as "a
   display concern" and handled per-destination, it would be fixed nowhere, because no single
   platform owns it.

**(c) is rejected as unnecessary rather than wrong.** No evidence in two sprints found a rule that
is *harmful* on one platform and *required* on another. Every rule found is either universally safe
or universally desirable. (c) would add a mechanism with no current use.

**Resulting rule set** (rewritten SEC-004, applied everywhere, no `#[cfg]`):

| Rule | Status |
| --- | --- |
| Strip to basename on `/` and `\` | ✅ present |
| Reject `.` and `..` | ✅ present |
| Strip Unicode category **Cc** | ✅ present |
| Trim trailing dots, spaces, tabs | ✅ present |
| Reject `CON PRN AUX NUL COM1-9 LPT1-9` | ✅ present |
| Cap at `MAX_FILENAME_BYTES` on a char boundary, keep extension | ✅ present |
| **Reject or strip `:`** | ❌ **add — alternate data streams** |
| **Strip Unicode category Cf (`U+202E`, `U+200B`, `U+FEFF`, …)** | ❌ **add — spoofing, all platforms** |
| **Add `CONIN$`, `CONOUT$` to reserved stems** | ❌ **add** |
| **Reject or replace `< > " \| ? *`** | ❌ **add — invalid on Windows** |
| Case-insensitive collision handling | ✅ already safe — `create_new` + numbering |

| Field | Value |
| --- | --- |
| **Recommended** | **(a)** protocol-global conservative, with the four additions above |
| **Rejected options** | **(b) REJECTED** — puts a security control behind untested `#[cfg]` arms and optimises for the wrong moment. **(c) REJECTED** — no evidence any rule needs to differ |
| **Risks** | R-07 (rewritten — severity confirmed, content corrected) |
| **Implementation dependency** | SEC-004, **P0, Wave 0** — moved earlier because the bidi gap is a present-tense Linux defect, not a future Windows one |
| **Compatibility** | `files.v1` unchanged. Sanitisation is receiver-local; no wire change, no version bump |
| **Status** | **READY FOR RFC** |

### PLAT-DEC-015 — Software→hardware identity migration, and the fallback policy 🆕

| Field | Value |
| --- | --- |
| **Question** | Apple's Enclave cannot import an existing key. What happens to a device that ships with a software identity and later gains hardware backing — and what happens when a hardware identity becomes inaccessible? |
| **Why it exists** | Verbatim: *"Can't encode preexisting keys. You must use the Secure Enclave to create the keys."* Plus: Intel Macs have no Enclave, so software keys will exist in the field |

**The two situations must never be confused**, and this is the heart of the brief's fail-safe
requirement:

| Situation | Meaning | Correct response |
| --- | --- | --- |
| **Never supported** | No TPM, no Enclave, no Keystore on this device | Software key. Legitimate. Show the backing honestly |
| **Supported, not yet used** | Hardware exists; identity predates support | **Offer** migration. Never automatic — it re-pairs every peer |
| **Was hardware, now inaccessible** | TPM cleared, Enclave locked, Keystore invalidated | **Refuse to start.** Never regenerate |

**Recommended policy:**

1. **Record the backing at creation**, in `state.json`, alongside the identity. Without a recorded
   expectation there is no way to tell "never had hardware" from "lost hardware" — and that
   distinction is the entire safety property.
2. **A recorded hardware backing that cannot be opened is a hard error.** Refuse to start. Name the
   cause and the remedy. Do not fall back to software, and do not generate a new identity — either
   would silently break every pairing while looking like a hiccup.
3. **Software→hardware migration is user-initiated, never automatic**, and the UI must state
   plainly that it creates a new identity and requires re-pairing every device. Because the Enclave
   cannot import, there is no honest way to make this transparent, so it must be explicit.
4. **Never silently downgrade hardware→software.** If hardware creation fails on first run, say so
   and let the user choose.

This subsumes the brief's software-key-fallback question. The distinction it asks for —
"device never supported hardware backing" vs. "previous hardware identity disappeared" — is
implementable **only** if (1) is done, which is why (1) is the load-bearing item and belongs in
Wave 0's `state.json` schema, before any hardware backing exists to need it.

| Field | Value |
| --- | --- |
| **Recommended** | As above. Record backing; unusable-recorded-hardware is fatal; migration is explicit; no silent downgrade |
| **Rejected options** | **Automatic migration REJECTED** — re-pairs every peer without consent. **Silent software fallback REJECTED** — a security downgrade the user cannot see |
| **Risks** | R-08 (rewritten — now a present-tense defect, [26 §11.2](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)) |
| **Implementation dependency** | SEC-009 (P0, Wave 0), ARCH-002 |
| **PoC dependency** | Fault injection per backing, in POC-WIN-03 and POC-MAC-03 |
| **Status** | **RECOMMENDED** — the *policy* is settled; per-backing detection of "inaccessible" needs the PoCs |

---

## 5. Decisions on principle, restated

Two rejections in this register are **not technical** and must not be revisited on technical
grounds. Recording them plainly so a future sprint does not quietly reopen them:

- **PLAT-DEC-005(b) — APNs + relay for iOS.** Rejected because it puts a third party in the trust
  path of a product whose first principle is that there is not one. A better relay design does not
  change this. If iOS background limits make the product not worth shipping, the answer is
  **not to ship it**, not to add a cloud.
- **PLAT-DEC-009(c) — private clipboard API on macOS.** Rejected absolutely. A product whose value
  is trustworthiness does not ship a private API in the clipboard path.

**On APNs specifically**, per the brief's request for a documented position, default state
**DEFERRED**:

| Question | Answer |
| --- | --- |
| What would it solve? | Waking a suspended iOS app so a clip could arrive without the user opening it |
| What would it *not* solve? | Everything else. The app still cannot hold a socket; a push is a wake, not a session. Payload limits mean the clip itself still needs a fetch |
| External infrastructure? | **Yes** — an APNs-authorised server OmniBridge would have to operate |
| Impact on local-first | Fatal. A LAN-only product would require an internet round trip through infrastructure the maintainer runs |
| Impact on privacy | The relay learns device identities, timing and volume — a metadata channel that does not exist today |
| Could it ever be optional? | Only as an opt-in that is off by default and whose absence changes nothing. No such design has been produced, and none is requested |
| **Status** | **DEFERRED.** Not scheduled, not designed, not a fallback for POC-IOS-06 |

---

## 6. Risk closeout

Sixteen risks reviewed. Three were named as defects; all three change materially.

| ID | v1 | **Now** | Change |
| --- | --- | --- | --- |
| R-01 | Med / Critical | Med / Critical | Unchanged. Mitigation strengthened in [28](28-WAVE-0-IMPLEMENTATION-SPEC.md) |
| R-02 | Med / High | **Low / High** | ⬇ Apple returns X9.62 DER, the encoding rustls wants — no conversion layer. Windows needs one, and `rustls-cng` already has it, tested |
| R-03 | High / Med | High / Med | Unchanged. POC-IOS-06 still the gate |
| R-04 | Med / Med | Med / Med | Unchanged (V-12 still open) |
| R-05 | Med / Low–Med | **Confirmed / Low–Med** | Cause identified exactly: Ubuntu 26.04 LTS only, KWin ≥ 6.5 × wl-clipboard 2.2.1 |
| R-06 | Low / High | **Med / High** | ⬆ The default named-pipe ACL grants **Everyone + anonymous read**. Higher likelihood of a mistake than assumed |
| R-07 | Med / High | **Low / High** | ⬇ likelihood — most rules already exist. Content corrected: `:` and `U+202E`, not device names |
| R-08 | Med / High | **High / Critical** | ⬆⬆ **Present-tense defect on Linux**, not a future hardware risk |
| R-09 | Med / High | **Med / High+** | ⬆ Developer ID now also a *runtime* requirement (local network privacy) |
| R-10 | Med / High | Med / High | Unchanged |
| R-11 | High / Med | High / Med | Unchanged. CI-003 still the mitigation |
| R-12 | High / High | High / High | Unchanged. Waves 0–4 remain a complete outcome |
| R-13 | Med / Critical | Med / Critical | Unchanged. §5 records the rejection |
| R-14 | Med / Med | **Med / Med+** | Broadened — macOS 15.4 and Ubuntu 26.04 are now parity-gap platforms too, not just iOS |
| R-15 | Low / Med | Low / Med | Unchanged |
| R-16 | Med / Med | Med / Med | Unchanged |
| **R-17** 🆕 | — | **Confirmed / Med** | `sensitive_hint` clips fail on Debian 13 and every Ubuntu LTS |
| **R-18** 🆕 | — | **Confirmed / High** | `unsafe_code = "forbid"` blocks every platform adapter |
| **R-19** 🆕 | — | **Med / Med** | macOS clipboard auto-send is user-gated (15.4+) and may be silently denied |

### The three defects, treated concretely

**R-08 — silent identity regeneration.** *Now the most severe item in the register.*

| Field | Value |
| --- | --- |
| **Failure scenario** | `identity.key` is deleted or becomes unreadable (partial restore, botched `rsync`, permissions accident). `Path::exists()` returns `false` for *any* metadata error, so `Store::open()` takes the `initialize()` branch, generates a fresh identity, and **overwrites `state.json`, destroying the peer list**. Presented to the user as a fresh start |
| **Required mitigation** | Distinguish absent from unreadable. Use `try_exists()`; treat `state.json` present + key absent/unreadable as **fatal**. Record `key_backing`; a recorded hardware backing that will not open is fatal. Never write `state.json` from a path that did not first prove the key situation |
| **Where it belongs** | `core/src/store.rs` — `Store::open`, `initialize`, `load` |
| **Wave** | **0** |
| **Test** | Fault injection: (i) key deleted, state present; (ii) key `chmod 000`; (iii) data dir non-traversable. Each must refuse to start and leave `state.json` **byte-identical** |
| **Blocking severity** | **P0.** A refactor that moves this code without fixing it would bake the defect into the new abstraction |

**R-07 — filename semantics.** Severity confirmed, content corrected.

| Field | Value |
| --- | --- |
| **Failure scenario** | (i) A paired-but-hostile peer offers `a:b`; on NTFS this creates an alternate data stream `b` on file `a` — invisible in Explorer, and `destination.rs`'s parent-directory check does not catch it because `C:\dir\a:b` has parent `C:\dir`. (ii) A peer offers `invoice\u{202E}cod.exe`, which displays as `invoiceexe.doc` **on Linux today** |
| **Required mitigation** | SEC-004 as rewritten in PLAT-DEC-014 |
| **Where** | `capabilities/files/src/filename.rs`, protocol-global |
| **Wave** | **0** (moved from 5 — the bidi half is a current Linux defect) |
| **Test** | New cases in `filename.rs` tests and `daemon/tests/files.rs` |
| **Blocking severity** | **P0** for the bidi rule; P1 for the Windows-only rules |

**R-06 — named-pipe squatting.** Likelihood raised; mitigation made precise.

| Field | Value |
| --- | --- |
| **Failure scenario** | (i) A local process creates `\\.\pipe\omnibridge-…` before the agent starts; the CLI/GUI connect to it and hand over control-plane traffic. (ii) The agent passes `NULL` for `lpSecurityAttributes`, and the default DACL grants **read to Everyone and to anonymous** |
| **Required mitigation** | Explicit `SECURITY_ATTRIBUTES` with a DACL granting only the owning user's SID — **never `NULL`**. `PIPE_REJECT_REMOTE_CLIENTS`. `FILE_FLAG_FIRST_PIPE_INSTANCE`, and on `ERROR_ACCESS_DENIED` **abort naming the squatter — never retry, never fall back to another name**. Per-session pipe name (Microsoft's own guidance). Verify the caller with `GetNamedPipeClientProcessId` |
| **Where** | `omnibridge-windows` adapter, `ControlTransport` impl |
| **Wave** | **5** |
| **Test** | POC-WIN-07 with a hostile squatter started first; assert the agent refuses to start |
| **Blocking severity** | **P0 for Wave 5.** Not a Wave 0 item — but the `ControlTransport` trait designed in Wave 0 must not make the safe implementation awkward. [28 §6](28-WAVE-0-IMPLEMENTATION-SPEC.md) requires a fallible bind returning a distinguishable "name already owned" error |

---

## 7. PoC plan hardening

Thirty-five PoCs reviewed. The verification answered several outright and sharpened others.

### 7.1 Retired or absorbed

| PoC | Disposition | Why |
| --- | --- | --- |
| POC-KDE-01 | **Narrowed, not retired** | Its central question is answered from primary sources ([26 §6](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). What remains genuinely unmeasurable: does the Xwayland XFIXES fallback work on Plasma? Reduced from ~2 days to ~half a day |
| POC-WIN-02 | **Split** | The `mdns-sd`-coexistence half (V-12) stays P1. The `DnsServiceRegister` A/AAAA half (V-03) becomes contingent — run only if `mdns-sd` fails |
| POC-MAC-05 | **Requestioned** | No longer "does anything prompt?" but "does `changeCount` polling alone trigger the access alert, or only the content read?" — a sharper and cheaper measurement |
| POC-IOS-02 | **Narrowed** | TN3179 answers most of it. What remains: does the *listen-only* path really avoid the prompt, and is the multicast entitlement needed for a single declared service type? |

No PoC is deleted. Three shrink, one splits.

### 7.2 New PoCs the verification requires

| ID | Question | Why docs cannot answer | Blocks |
| --- | --- | --- | --- |
| **POC-CORE-04** | Does `cargo check --target x86_64-pc-windows-msvc` (on a Windows runner) succeed for the portable crates after Wave 0? | `ring` needs MSVC; nothing states whether the portable crates are otherwise clean | Wave 0 gate, CI-001 |
| **POC-LINUX-05** | On Debian 13 / Ubuntu 24.04 / 26.04, does `wl-copy --sensitive` fail as predicted, and does the probe correctly detect it? | Confirms the §5.1 defect on real systems and validates the fix | LINUX-010, PLAT-DEC-013 |

### 7.3 Prioritisation

**P0 — ARCHITECTURE BLOCKING.** Must complete before or during Wave 0.

| PoC | Blocks | Can run in a VM? | Hardware? |
| --- | --- | :-: | --- |
| **POC-CORE-01** — cross-compile the portable crates | Wave 0 acceptance | ✅ | A Windows runner (CI is enough) |
| **POC-CORE-02** — `IdentitySigner` seam is behaviour-preserving | ARCH-002, R-01 | ✅ | none |
| **POC-CORE-03** — `ControlTransport` seam | ARCH-003 | ✅ | none |
| **POC-CORE-04** 🆕 — Windows-target compile check | CI-001 | ✅ | Windows runner |

**Four.** All are Wave 0's own acceptance gates, all run on Linux plus a CI Windows runner, none
needs a physical device, and none blocks the *decision* to start Wave 0 — they gate its completion.
This is the answer to the brief's "probably few": **no PoC must run before Wave 0 begins.**

**P1 — PLATFORM BLOCKING.** Before the relevant platform wave.

| PoC | Blocks wave | Decision |
| --- | :-: | --- |
| POC-LINUX-05 🆕 | 2 | PLAT-DEC-013 |
| POC-LINUX-01, POC-LINUX-02 | 2 | PLAT-DEC-011 |
| POC-KDE-01 (narrowed) | 3 | — |
| POC-WIN-03, **POC-WIN-04** | 5 | PLAT-DEC-004 pattern |
| POC-WIN-07 | 5 | R-06 |
| POC-MAC-03, **POC-MAC-04** | 7 | PLAT-DEC-004 |
| POC-MAC-05 (requestioned) | 8 | PLAT-DEC-009 macOS half |
| **POC-IOS-06** | 9 | **PLAT-DEC-005** |

**P2 — VALIDATION.** POC-WIN-01/05/06/08/09, POC-MAC-01/02/06/07, POC-IOS-01/03/05/07/08/09,
POC-KDE-02, POC-DISC-01.

**P3 — OPTIMIZATION.** POC-LINUX-03 (Flatpak), POC-LINUX-04 (Linux TPM2).

### 7.4 Linux TPM2 — explicitly not a Wave 0 blocker

The brief asks which future path is plausible. Assessment, unimplemented:

| Path | Viability |
| --- | --- |
| **`tpm2-tss` via a Rust binding + custom `SigningKey`** | **Most plausible.** Same seam as Windows and Apple. The `IdentityProvider` trait pays for it |
| **PKCS#11 + a custom `SigningKey`** | Plausible; more indirection, better hardware coverage (also reaches smartcards/YubiKeys) |
| OpenSSL provider | **Does not apply.** rustls does not consume OpenSSL providers. Mentioned only to close it off |
| Platform keyring (`gnome-keyring`/`kwallet`) | Different problem — encrypts a software key at rest; the key is still exportable to our process. A *hardening*, not hardware backing |

Classified **POC REQUIRED (P3)**. Linux TPM2 must not gate Wave 0 or Waves 1–4: Linux already has a
working software identity, and the seam that would enable TPM2 later is the same one Wave 0 builds
for Windows and Apple. **Building the seam is Wave 0; using it on Linux is optional forever.**

---

## 8. Decisions READY FOR RFC

Ten, in the order an RFC author should take them:

| # | ID | Title | Wave |
| --- | --- | --- | :-: |
| 1 | PLAT-DEC-001 | Portable Rust core + native adapters | 0 |
| 2 | PLAT-DEC-009 | Clipboard backends may poll where no event source exists | 0 |
| 3 | PLAT-DEC-014 | Protocol-global conservative filename rules | 0 |
| 4 | PLAT-DEC-012 | Key backing is displayed locally, never advertised | 0 |
| 5 | PLAT-DEC-013 | `sensitive_hint` fails closed with an explanation | 2 |
| 6 | PLAT-DEC-011 | Debian vendors; ships from CI | 2 |
| 7 | PLAT-DEC-003 | GTK stays; KDE is a certification target | 3 |
| 8 | PLAT-DEC-002 | Windows: user-session agent, not a service | 5 |
| 9 | PLAT-DEC-008 | Additive `Platform` enum values | 5 |
| 10 | PLAT-DEC-010 | Android stays Kotlin | — |

**1–4 are Wave 0's charter.** They are the only ones an RFC must land before implementation starts.
5–10 can be written any time before their wave.

**This is not a request to write those RFCs now.** The brief is explicit that they are reviewed
first.

## 9. Decisions that still depend on a PoC

Five. Two have status **POC REQUIRED** (no recommendation can be made without the measurement);
three are **RECOMMENDED** (the direction is settled, and a PoC must confirm one component before an
ADR).

| ID | Status | What the PoC decides | Gate | Wave |
| --- | --- | --- | --- | :-: |
| PLAT-DEC-005 | **POC REQUIRED** | Whether an iOS product is worth building at all | POC-IOS-06 | 9 |
| PLAT-DEC-007 | **POC REQUIRED** | Whether Flatpak is a usable channel | POC-LINUX-03 | 4 |
| PLAT-DEC-004 | RECOMMENDED | That an Enclave-issued certificate's SPKI matches what the fingerprint code extracts | POC-MAC-03/04 | 7 |
| PLAT-DEC-015 | RECOMMENDED | How "hardware inaccessible" is detected per backing | POC-WIN-03, POC-MAC-03 | 5, 7 |
| PLAT-DEC-006 | RECOMMENDED | The macOS row of the capability table | POC-MAC-05 | 8 |

**None of these blocks Wave 0.** The earliest is Wave 4; the two that could change the architecture
(004, 005) belong to Waves 7 and 9, and both inherit a seam Wave 0 will already have proved on
Linux with a software key.
