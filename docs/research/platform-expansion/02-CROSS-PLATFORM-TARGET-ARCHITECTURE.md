# 02 — Cross-platform target architecture

| Field | Value |
| --- | --- |
| **Title** | Target architecture for Linux + Windows + macOS + Android + iOS/iPadOS |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Where the code boundary between shared and native should fall, and why. Crate layout, FFI strategy, host inversion, what deliberately stays duplicated. |
| **Decision status** | **PROPOSED.** Nothing here is approved. Every option carries its evidence level. |
| **Evidence** | REPO VERIFIED for all statements about current code; OFFICIAL DOC VERIFIED for toolchain/tier claims; HYPOTHESIS for effort estimates. |
| **Related documents** | [01](01-CURRENT-ARCHITECTURE-AUDIT.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md), [17](17-BACKGROUND-EXECUTION-MODEL.md), [18](18-UI-PLATFORM-STRATEGY.md), [22](22-IMPLEMENTATION-ROADMAP.md), [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) |

---

## 1. The question, stated properly

The sprint brief offers a candidate diagram: one Rust core, three desktop platform adapters,
two mobile clients. The brief also says, correctly, not to assume it is right.

It is *mostly* right, and it is wrong in one specific and important place. The rest of this
document is about where.

The framing that produces the right answer is not "how much code can we share?" It is:

> **For each subsystem: if this is implemented twice, what is the cost of the two copies
> disagreeing?**

Sort by that, and the architecture writes itself.

| Subsystem | Cost if two implementations disagree | Verdict |
| --- | --- | --- |
| Wire format, framing, envelope | Silent corruption or a hang between two OmniBridge devices | **Share** |
| Pairing proof construction | A pairing that succeeds when it should fail. Security-fatal. | **Share** |
| SPKI pinning / cert verification | Accepting the wrong device. Security-fatal. | **Share** |
| Replay guard, sequence rules | Exploitable divergence | **Share** |
| Capability negotiation + grant checks | A peer using a capability it was never granted | **Share** |
| Clipboard dedup / loop suppression | An infinite clipboard loop between two devices | **Share** |
| `files.v1` transfer state machine + stream MAC | A stream authenticated to the wrong transfer | **Share** |
| Filename sanitisation rules | Path traversal | **Share** |
| Size/rate limits | Inconsistent DoS surface | **Share** |
| Reading a clipboard | Nothing. The two platforms genuinely differ. | **Don't share** |
| Writing a file to disk | Nothing | **Don't share** |
| Storing a key | Nothing — and sharing it is *actively harmful*, §5 | **Don't share** |
| Drawing a button | Nothing; sharing it makes the product worse | **Don't share** |
| Staying alive in the background | Nothing; the platforms have nothing in common | **Don't share** |

The list of "share" items is exactly the set that is already in `omnibridge-core` and the
non-backend halves of the capability crates. **The existing crate boundaries are already in
the right place.** The expansion does not need a re-architecture. It needs the boundary that
already exists conceptually to be made real at the build level, plus one new seam (identity
signing) that does not exist yet.

---

## 2. Recommended architecture

```
                       protocol/proto/**            ← one schema, all platforms
                              │
        ┌─────────────────────┴──────────────────────┐
        │                                             │
   omnibridge-proto (Rust)                        Kotlin protos (Android)
        │
   ┌────┴──────────────────────────────────────────────────┐
   │  omnibridge-core            PORTABLE, no_std-ish policy:  │
   │    framing session tls pairing qr fingerprint          │
   │    capability discovery clipboard_policy               │
   │    identity  ──► Signer trait  (NEW)                   │
   │    store     ──► StateStore + SecretFile traits (NEW)  │
   └────┬──────────────────────────────────────────────────┘
        │
   ┌────┴────────────────────────────────────────┐
   │  capability crates (protocol halves only)    │
   │    battery   ── BatterySource trait          │  ✅ pattern already exists
   │    files     ── FileSink trait      (NEW)    │
   │    clipboard ── ClipboardBackend    ✅ exists │
   └────┬────────────────────────────────────────┘
        │
   ┌────┴──────────────────────────────────────┐
   │  omnibridge-runtime  (NEW; today's daemon     │
   │  minus its Linux assumptions)              │
   │    listener  mdns  state  SessionHost impl │
   │    ── ControlTransport trait      (NEW)    │
   └────┬───────────────┬───────────────┬───────┘
        │               │               │
  omnibridge-linux    omnibridge-windows   omnibridge-apple      ← platform adapter crates
   uds, xdg,        named pipe,       keychain, UDS,
   0600 modes,      DPAPI/ACL,        NSPasteboard,
   wl-clipboard,    CNG/TPM,          SecureEnclave,
   upower           Win32 clipboard,  IOKit power
                    WinRT DNS-SD?
        │               │               │
   ┌────┴───┐      ┌────┴────┐     ┌────┴──────┐
   │omnibridged│      │OmniBridge  │     │OmniBridge.app│
   │ + CLI  │      │Agent.exe│     │  (agent)  │
   └────┬───┘      └────┬────┘     └────┬──────┘
        │               │               │
   GTK4/libadwaita   WinUI 3        SwiftUI/AppKit          ← native UI, IPC to agent
        GUI          desktop app        app


   Android (Kotlin)                     iOS / iPadOS (Swift)
   independent impl, stays              ── open question, §6 ──
   Kotlin (PLAT-DEC-010)                Rust core via UniFFI is the
                                        leading candidate
```

Five deltas from today. Nothing else moves.

| Δ | Change | Why |
| --- | --- | --- |
| **Δ1** | `identity.rs` gains a `Signer` trait; the PKCS#8 bytes become *one implementation* of it | Unblocks TPM and Secure Enclave. [01 §3.1] |
| **Δ2** | `store.rs` splits into portable state logic + a `SecretFile`/`StateStore` platform trait | Removes the only non-portable file in core, **without** dropping the mode checks |
| **Δ3** | `omnibridge-daemon` splits into `omnibridge-runtime` (portable) + `omnibridge-linux` (adapter); control-protocol *types* move to their own crate | Lets the GUI and future UIs depend on types without inheriting a platform |
| **Δ4** | `files` gains a `FileSink`, `clipboard`'s `x11rb` becomes optional | Same pattern as `battery`/`upower`, which already works |
| **Δ5** | A `ControlTransport` trait with UDS and Named Pipe implementations | Windows AF_UNIX exists but carries no credentials |

---

## 3. Alternatives considered and rejected

### Alt-A — `#[cfg(target_os)]` in place, no new crates

Cheapest to start. Rejected.

Seven files would acquire conditional branches (`store.rs`, `destination.rs`, `server.rs`,
`control.rs`, `main.rs`, `client.rs`, `cli/main.rs`), three of which are security-critical.
Every `#[cfg]` arm not matching the CI host is **unbuilt and untested**, and CI on Linux
would not even compile the Windows branch of the trust store. For a codebase whose defining
quality is that its dangerous parts are tested (`core/tests/`, `daemon/tests/`,
`capabilities/*/tests/`), that is a regression in kind, not degree. Adapter crates keep the
platform code compiled and tested by the platform's own CI job.

Keep `#[cfg]` for *small, local* choices inside an adapter. Do not use it as the boundary.

### Alt-B — one Rust core, all five platforms consume it via FFI (including Android)

Maximises sharing. Rejected as a *goal*, deferred as an *option*.

Android today has a complete, tested Kotlin implementation with hardware-backed Keystore
identity wired through `javax.net.ssl` and Conscrypt. Replacing it with Rust-over-JNI would:

- re-solve the hardest problem on that platform (a non-exportable TEE key driving a TLS
  client certificate) in a harder way, through a Rust `SigningKey` calling back into JNI
  calling `Signature.sign()`;
- throw away ~20 passing test files;
- deliver nothing the user can see.

The rule this research proposes: **rewrite a working platform implementation only when the
duplication has actually caused a defect, not because a diagram looks tidier.**
Tracked as **PLAT-DEC-010**, recommended direction *keep Kotlin*, status OPEN.

### Alt-C — no shared core at all; five native implementations

This is what Android + Linux already is, extended. Rejected for Windows and macOS: it means
implementing the pairing proof and the pinning verifier three more times, and those are the
two places where a bug is a security vulnerability rather than a defect. Two independent
implementations is a healthy cross-check. Five is a liability.

### Alt-D — Electron / Tauri / a web UI for the desktops

Rejected for Electron; see [18](18-UI-PLATFORM-STRATEGY.md) for the full argument. In
summary: OmniBridge's value is *deep OS integration* (clipboard ownership, tray/menu-bar
presence, login startup, share targets, native file pickers, notifications). A web layer
adds a runtime between the product and every one of those, and the UI is not where the
duplication cost lies — §1 shows the expensive duplication is all below the UI.

Tauri is a closer call because its core is already Rust; it is not dismissed, but it does
not solve the clipboard/agent/keystore problems, which are the actual work.

---

## 4. Answers to the brief's specific questions

**Should the Rust core be a library?** It already is. `omnibridge-core` is a library with no
global state and no I/O policy, and `SessionHost` (`core/src/session.rs:130`) is a proper
inversion point. Nothing needs inventing; two traits need adding (Δ1, Δ2).

**Should the daemon be cross-platform?** The *code* yes, the *concept* no. `listener.rs`,
`mdns.rs` and `state.rs` are portable now. But "daemon" is a Linux word, and on Windows a
Service is the wrong shape (Session 0 has no clipboard — [08](08-WINDOWS-FEASIBILITY.md)),
on macOS it is a `SMAppService` login agent, and on iOS it does not exist. So: one portable
`omnibridge-runtime` **library**, hosted by a per-platform process with a per-platform
lifetime. The abstract name for that process is defined in
[17](17-BACKGROUND-EXECUTION-MODEL.md) as the **OmniBridge Agent**.

**Should `omnibridge-core` be separated from `omnibridge-linux`?** Yes, but the split that matters
is not `core`/`linux` — core is already almost clean. It is
`omnibridge-daemon` → `omnibridge-runtime` + `omnibridge-linux`, plus extracting the control-protocol
types so the GUI stops depending on the daemon for `serde` structs
([01 §4](01-CURRENT-ARCHITECTURE-AUDIT.md)).

**What logic stays in Rust?** Everything in the "share" column of §1: framing, TLS and
pinning, pairing, session lifecycle and replay, capability negotiation and grants, clipboard
policy/dedup/loop-suppression, transfer state machine, stream MAC, filename rules, limits.

**What logic stays native?** Key storage, clipboard read/write/watch, filesystem writes and
the file picker, notifications, power/battery source, discovery *if* the platform stack
proves better than `mdns-sd`, background/lifecycle, login registration, and all UI.

**How is Rust exposed to Swift?** Two candidates, both real:
- **UniFFI** (Mozilla) — generates Swift *and* Kotlin from one interface definition;
  production-proven in `mozilla/application-services` for exactly this
  "one Rust core, two mobile platforms" shape (OFFICIAL DOC VERIFIED, UniFFI user guide).
  Chosen if iOS uses the Rust core, because it keeps a future Kotlin-consumes-Rust option
  open at no extra cost.
- **A hand-written C ABI + a Swift wrapper** — smaller dependency, more work, and it
  hand-rolls the async story.

Recommended direction: **UniFFI**, POC REQUIRED (**POC-IOS-04**). The hard part is not the
binding generator, it is that the Secure Enclave signer must be *called back into Swift from
Rust* during the TLS handshake, synchronously — see §5.

**How is Rust exposed to Windows UI?** It is not, directly. WinUI 3 talks to the **agent
process** over the `ControlTransport` (a Named Pipe), exactly as `omnibridge-gui` talks to
`omnibridged` today over a Unix socket. This preserves the property the current design already
has — the UI can crash, restart or be absent, and the session survives — and it avoids a
C#↔Rust FFI entirely. Recommended direction: *named pipe IPC, no FFI*. (Research v1 cited this as "PLAT-DEC-007", which
is a drafting error — [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md) defines PLAT-DEC-007 as
Flatpak viability. The IPC seam belongs to **PLAT-DEC-001**; see
[27 §2](27-ARCHITECTURE-DECISION-CLOSEOUT.md).)

**What is not worth sharing?** UI. Notification presentation. Anything whose two
implementations disagreeing costs nothing. And — stated explicitly because it is
counter-intuitive — **key storage**, which is next.

---

## 5. The one place "share more" is the wrong instinct

`LocalIdentity` currently holds `key_pkcs8_der: Vec<u8>` and hands it to rustls.

The tempting move, when adding Windows and macOS, is to keep that: generate a P-256 key in
Rust, persist it as PKCS#8 with tight file permissions, and have one identical code path on
every desktop. It would work. It would be less code. It would be **a silent security
downgrade on every platform that has a TPM or a Secure Enclave**, and the sprint's security
principle forbids exactly that.

So the seam has to be inverted:

```rust
// PROPOSED. Not implemented. Illustrative only.
pub trait IdentitySigner: Send + Sync + std::fmt::Debug {
    fn certificate_der(&self) -> &CertificateDer<'static>;
    fn fingerprint(&self) -> Fingerprint;
    fn sign(&self, scheme: SignatureScheme, message: &[u8]) -> Result<Vec<u8>>;
    fn supported_schemes(&self) -> &[SignatureScheme];
    /// Honest self-description for the UI and for `omnibridge status`.
    fn backing(&self) -> KeyBacking; // Software | Tpm | SecureEnclave | Keystore(StrongBox|Tee)
}
```

wired into rustls through `sign::SigningKey` + `ResolvesClientCert`/`ResolvesServerCert` —
the extension points rustls documents for keys that live "in an HSM, or in another process,
or perhaps another machine" (OFFICIAL DOC VERIFIED). `rustls-cng` (rustls org, v0.7.1,
rustls ^0.23 — the pinned version) already does precisely this for Windows CNG with ECDSA
secp256r1, which turns the Windows half from research into integration.

Three consequences worth stating now:

1. **`KeyBacking` must be surfaced, not hidden.** If a machine has no usable TPM and falls
   back to a software key, the user and the *peer's* user should be able to see that. The
   protocol change to advertise it, if any, is deliberately deferred — see
   [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md) and **PLAT-DEC-004**.
2. **`sign()` is synchronous and blocking**, because rustls's `Signer::sign` is. A Secure
   Enclave signature that requires user presence (Touch ID) cannot be done inside a TLS
   handshake. Therefore OmniBridge's Apple identity key must be created **without**
   `.userPresence` access control. That is a real design constraint, not a preference →
   [12](12-APPLE-SECURITY-AND-INTEGRATION.md).
3. Identity generation and identity *use* separate cleanly. `rcgen` still issues the
   self-signed certificate on platforms with software keys; on TPM/Enclave platforms the
   certificate must be built around a public key the platform gives us, which `rcgen`
   supports via a remote-key-pair path. **POC-WIN-03** / **POC-MAC-03** must prove this end
   to end, not in parts.

---

## 6. iOS is the architecture's real open question

Every other platform gets the same answer: portable Rust core, native adapter, native UI.
iOS resists it for reasons that are not about Rust:

- Rust builds fine for `aarch64-apple-ios` (Tier 2, rustup-distributed).
- But an iOS app **cannot hold a socket open in the background** ([11](11-IOS-IPADOS-FEASIBILITY.md)),
  so the `session.rs` model of "a long-lived authenticated session with a liveness timer"
  describes something iOS will not let exist.
- And the Secure Enclave signer has to be reachable *synchronously from Rust during a
  handshake*, which means a Swift callback across the FFI on rustls's thread.

Three options, none free:

| Option | Shares | Cost | Verdict |
| --- | --- | --- | --- |
| **iOS-1** Rust core via UniFFI, Swift adapter | pairing, pinning, framing, capability logic | FFI + a Swift→Rust signer callback + reworking session lifetime for suspend | **Recommended**, POC REQUIRED |
| **iOS-2** Full Swift implementation (Android's model) | nothing | Third implementation of the pairing proof and the pinning verifier | Rejected unless iOS-1's POCs fail |
| **iOS-3** No iOS client; iPadOS only, or nothing | — | — | The honest fallback if background limits make the product not worth shipping |

The decision cannot be made from documentation. **POC-IOS-06** (measure exactly how long a
TLS session survives backgrounding, and what the user-visible failure looks like) is the
gate. Tracked as **PLAT-DEC-005**.

---

## 7. What this architecture explicitly does not do

- It does not change the protocol, beyond the additive `Platform` enum values (**PROPOSED**,
  [23](23-RISKS-OPEN-QUESTIONS-AND-DECISIONS.md)).
- It does not change TLS, pinning, pairing, or the trust model.
- It does not unify the UI.
- It does not force capability parity. A platform advertises what it can do; see
  [03](03-PLATFORM-CAPABILITY-MATRIX.md).
- It does not rewrite Android.
- It does not introduce a cloud, a relay or an account, on any platform, for any reason —
  including to make iOS look like a desktop.

---

## 8. Sequencing

Δ1–Δ5 are **Wave 0** and are the only refactor this expansion needs. They are worth doing
before the first Windows line of code, because doing them afterwards means doing them twice.
Their acceptance criterion is precise and testable on Linux alone:

> `cargo build … --target x86_64-pc-windows-msvc` succeeds on a Linux CI host, and the entire
> existing test suite still passes on Linux with no behavioural change.

**Corrected by the verification sprint.** `ring` *"currently requires a C (but not C++) toolchain"*
and, for Windows targets, the Visual Studio 2022 "Desktop development with C++" workload
(upstream `BUILDING.md`, V-10). MSVC libraries are not redistributable onto a Linux runner, so the
Linux cross-compile gate **does not work as written**. The corrected gate runs `cargo check` on a
**Windows CI runner**, and excludes `omnibridge-runtime` (whose `mdns-sd` Windows behaviour is V-12,
still open). See [28 §10.3](28-WAVE-0-IMPLEMENTATION-SPEC.md).

The gate is still *mechanical*, which is what makes Wave 0 finishable — and a green compile check
is a **boundary regression test, not runtime certification**.
