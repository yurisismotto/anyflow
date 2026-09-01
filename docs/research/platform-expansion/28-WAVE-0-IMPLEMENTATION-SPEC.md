# 28 — Wave 0 implementation specification

## Core platform abstraction

| Field | Value |
| --- | --- |
| **Title** | Wave 0 — Core platform abstraction: an implementable specification |
| **Status** | **IMPLEMENTED AND CERTIFIED** — `feature/core-platform-abstraction-v1`, commit `cfd33f6`. **WAVE 0 CERTIFIED 2026-09-01**: all 12 acceptance gates and all four P0 PoCs pass. See [the sprint report](../../sprints/wave-0-platform-abstraction.md) |
| **Last reviewed** | 2026-08-31 |
| **Sprint** | `research/platform-expansion-verification-v1` |
| **Scope** | The refactor that makes AnyFlow's Rust workspace portable, without adding a platform |
| **Decision status** | Depends on PLAT-DEC-001, -009, -012, -014 — all **READY FOR RFC** ([27](27-ARCHITECTURE-DECISION-CLOSEOUT.md)) |
| **Evidence** | Repository at `7bb0cc4`; rustls 0.23.43 documentation; [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) |
| **Related documents** | [01](01-CURRENT-ARCHITECTURE-AUDIT.md) · [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) · [26](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) · [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) · [22](22-IMPLEMENTATION-ROADMAP.md) |

---

## 1. Objective

> Give AnyFlow a **real platform boundary at the build level**, and fix the three defects that a
> refactor would otherwise carry forward into every platform that inherits the code.

Measured by one sentence:

> After Wave 0, adding a platform means **writing an adapter crate**. It never means editing
> `anyflow-core`, `tls.rs`, `session.rs`, or a capability crate's protocol half.

Today that is false. There is not one `cfg(target_os)` in the workspace — verified again this
sprint, the sweep still returns nothing. AnyFlow does not have a platform boundary; it has Linux
code that happens to be the only code.

Wave 0 is smaller than Research v1 estimated, for a reason established in
[26 §11.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md): **the identity seam is two call sites.**

---

## 2. Non-goals

Stated first, because Wave 0's value depends on not growing.

- **No new platform.** No Windows, macOS, iOS or KDE-specific functionality. Not one line of
  `anyflow-windows`.
- **No protocol change.** No `.proto` edit, no `PROTOCOL_VERSION_MAX` move. `PLATFORM_WINDOWS`
  stays PROPOSED (PLAT-DEC-008).
- **No TLS, pairing, pinning or capability-negotiation behaviour change.** The seam changes *where
  the signing key comes from*, never what is signed or how it is verified.
- **No UI work.** GTK stays exactly as it is.
- **No CI change in the same commit as the refactor.** CI-001 lands after, on green.
- **No hardware backing.** No TPM, no Enclave, no Keystore. Wave 0 builds the seam and ships
  exactly one implementation of it: the software key that exists today.
- **No cosmetic renaming.** `anyflow-core` keeps its name. Churn is a cost, not a deliverable.

**The Wave 0 completion test is a negative one:** at the end, AnyFlow on Fedora and the Android app
behave identically to before, and `git log` shows no behavioural change — only a boundary.

---

## 3. Current problems, precisely

Seven items. Five are portability; three are defects; one is both.

| # | Problem | Evidence | Kind |
| --- | --- | --- | --- |
| **P1** | `LocalIdentity` owns `key_pkcs8_der: Vec<u8>` and hands it to rustls. Structurally incompatible with every hardware keystore | `identity.rs:47`, `tls.rs:288,342` | portability |
| **P2** | `store.rs` uses `OpenOptionsExt`/`PermissionsExt`; `default_data_dir()` is XDG-only; `default_device_name()` reads `/etc/hostname`, falling back to the literal `"Fedora"` | `store.rs:108-112, 319-326, 334, 352, 367` | portability |
| **P3** | `Store::open()` conflates "no key" with "key unreadable" and **overwrites `state.json`**, destroying the trust store | `store.rs:143-149` | **defect** |
| **P4** | `filename.rs` lets `:` (alternate data streams) and Unicode `Cf` characters (bidi spoofing) through | `filename.rs:66,73` | **defect** |
| **P5** | The control socket is `UnixListener` + chmod; paths from `$XDG_RUNTIME_DIR`; uid from `/proc/self/status` | `server.rs:9`, `control.rs` | portability |
| **P6** | `destination.rs` uses Unix modes and XDG download dirs | `destination.rs:34,66,159,263` | portability |
| **P7** | `unsafe_code = "forbid"` workspace-wide. `forbid` cannot be locally overridden; every adapter needs `unsafe` | `desktop/Cargo.toml` | **blocker** |

Plus two couplings that make the boundary awkward:

- **C1** — `anyflow-gui` and `anyflow-cli` both depend on `anyflow-daemon` (`default-features = false`)
  solely to reuse the control-protocol types. Deliberate, and good — but it means the GUI inherits
  every Linux dependency of the daemon to obtain some `serde` structs.
- **C2** — `Platform::Linux` is hardcoded at `store.rs:154` and `store.rs:188`. The platform identity
  is decided by the storage layer.

**Why fixing these in place is rejected.** It would put `#[cfg]` arms into `store.rs`,
`destination.rs`, `server.rs`, `control.rs`, `main.rs`, `client.rs` and `cli/main.rs` — seven files,
three of them security-critical. Every arm not matching the CI host is unbuilt and untested. For a
codebase with **309 tests** whose defining quality is that its dangerous parts are tested, that is a
regression in kind. See [02 §3](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), Alt-A.

---

## 4. Target architecture

Evolution, not replacement. **`anyflow-core` keeps its name, its path and its public API.**

```
                      protocol/proto/**            (untouched)
                             │
                      anyflow-proto                (untouched)
                             │
   ┌─────────────────────────┴──────────────────────────────┐
   │  anyflow-core                     PORTABLE — no std::os │
   │    framing session tls pairing qr fingerprint           │
   │    capability discovery clipboard_policy                │
   │                                                          │
   │    identity.rs  ──► trait IdentityProvider      (NEW)   │
   │    store.rs     ──► trait SecretStore           (NEW)   │
   │                     + portable state/trust logic         │
   │    unsafe_code = "forbid"          (KEPT)               │
   └────┬────────────────────────────────────────────────────┘
        │
   ┌────┴──────────────────────────────────────────┐
   │  capability crates — protocol halves only      │
   │    battery   ── BatterySource      ✅ exists    │
   │    clipboard ── ClipboardBackend   ✅ exists    │
   │    files     ── FileSink           (NEW)       │
   │    unsafe_code = "forbid"          (KEPT)      │
   └────┬──────────────────────────────────────────┘
        │
   ┌────┴───────────────────────┐   ┌──────────────────────────┐
   │  anyflow-control    (NEW)  │   │  anyflow-runtime   (NEW) │
   │  control request/response  │◄──┤  today's daemon minus    │
   │  types only. No I/O.       │   │  its Linux assumptions   │
   │  Breaks C1.                │   │  ── trait ControlTransport│
   └────┬───────────────────────┘   └────────┬─────────────────┘
        │                                     │
        │              ┌──────────────────────┴──────────┐
        │              │  anyflow-linux           (NEW)  │
        │              │  UDS transport, XDG paths,      │
        │              │  0600/0700 modes, /etc/hostname,│
        │              │  file-backed identity           │
        │              │  unsafe_code = "deny"           │
        │              └──────────────────────┬──────────┘
        │                                     │
   ┌────┴──────┬──────────────────┐   ┌───────┴────┐
   │anyflow-cli│  anyflow-gui     │   │  anyflowd  │
   └───────────┴──────────────────┘   └────────────┘
```

**Five new crates' worth of names, but only three are new code**: `anyflow-control` is a *move*,
`anyflow-runtime` and `anyflow-linux` are a *split* of today's `anyflow-daemon`.

### Why these seams and no others

The brief is explicit: do not create abstractions because they look elegant. Each seam below is
justified by **at least one verified platform difference**, and the ones that failed that test are
listed after.

| Seam | Verified difference that requires it |
| --- | --- |
| **`IdentityProvider`** | Enclave and TPM keys cannot be exported as PKCS#8 — Apple: *"Not having a mechanism to transfer plain-text key data into or out of the Secure Enclave is fundamental to its security"* ([26 §13](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)) |
| **`SecretStore`** | Unix modes have no Windows equivalent; a DACL is not `0o600`. Removing the check on Windows would be a silent security regression |
| **`ControlTransport`** | `tokio::net::UnixStream` is `#[cfg(unix)]`; Windows `AF_UNIX` carries no credentials; named pipes need an explicit DACL because the **default grants Everyone + anonymous read** ([26 §9.2](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)) |
| **`FileSink`** | `FOLDERID_Downloads` vs `$XDG_DOWNLOAD_DIR`; iOS has no Downloads concept at all |
| **`ClipboardBackend`** | ✅ **already exists.** Reuse unchanged |
| **`BatterySource`** | ✅ **already exists.** The template |

**Seams considered and rejected for Wave 0:**

| Rejected seam | Why |
| --- | --- |
| `DiscoveryBackend` | `mdns-sd` claims Linux/macOS/Windows. Until V-12 shows it fails, a trait would abstract over one implementation. **Add it when a second exists**, not before |
| `NotificationBackend` | AnyFlow implements no notifications on any platform. Nothing to abstract |
| `RuntimeLifecycle` | The differences (systemd / `Run` key / `SMAppService`) live in the **host process**, not in library code. A trait would have one method and no callers |
| `DestinationResolver` | Subsumed by `FileSink`. Two traits for one concern |
| `FileSystemBackend` | Too broad. `FileSink` names what `files.v1` actually needs |
| `LocalIpcBackend` | Same thing as `ControlTransport`. One name, not two |

**Rule applied:** a seam earns its place when a *verified* platform difference makes one
implementation impossible, not when a second implementation is merely imaginable.

---

## 5. Crate and module boundaries

| Crate | Status | Contents | `unsafe_code` |
| --- | --- | --- | --- |
| `anyflow-proto` | unchanged | generated types | forbid |
| `anyflow-core` | **modified** | protocol, TLS, pairing, capability, discovery model, clipboard policy; `IdentityProvider` + `SecretStore` traits; portable state/trust logic | **forbid** |
| `anyflow-capability-{battery,clipboard,files}` | **modified** | protocol halves; `FileSink` added; `x11rb` feature-gated | **forbid** |
| `anyflow-control` | **NEW (move)** | control request/response types; `serde` derives. No I/O, no tokio | forbid |
| `anyflow-runtime` | **NEW (split)** | listener, mdns, state, `SessionHost` impl, `ControlTransport` trait | forbid |
| `anyflow-linux` | **NEW (split)** | UDS transport, XDG paths, mode checks, `/etc/hostname`, file identity, wl-clipboard/X11 wiring | **deny** |
| `anyflow-daemon` (`anyflowd`) | **modified** | thin binary: compose runtime + linux adapter | deny |
| `anyflow-cli`, `anyflow-gui` | **modified** | depend on `anyflow-control`, **not** `anyflow-daemon` | deny |

**Why not `crates/anyflow-*`?** The alternative layout in the brief (`crates/anyflow-core`,
`crates/platform-linux`, …) is a full directory reshuffle for no functional gain. `desktop/` already
contains only Rust; `desktop/core`, `desktop/daemon`, `desktop/capabilities/*` are established in
[ADR-0001](../../adr/ADR-0001-monorepo-structure.md); every path in every existing document points
there. **Minimise churn: add `desktop/control`, `desktop/runtime`, `desktop/platform-linux`
alongside what exists.**

### The `unsafe_code` change (P7)

Today, `desktop/Cargo.toml`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
```

`forbid` **cannot** be relaxed by an inner `#[allow]`. Every adapter will need `unsafe` (Win32, CNG,
`Security.framework`, IOKit). The fix must be shaped so it **cannot** weaken the security core:

- Remove `unsafe_code` from `[workspace.lints.rust]`.
- Add `unsafe_code = "forbid"` explicitly to `anyflow-core`, `anyflow-proto`, `anyflow-control`,
  `anyflow-runtime` and the three capability crates.
- Adapter and binary crates get `unsafe_code = "deny"`, so any `unsafe` needs a deliberate,
  reviewable `#[allow]` with a justification comment.

**The wrong fix is to relax the workspace default to `deny`.** That silently drops the guarantee
from the crates where it matters most. Filed as **ARCH-010**.

---

## 6. Traits and seams

Illustrative. **Not implementation.** Names to be settled in the RFC.

### 6.1 `IdentityProvider` — the one that matters

Derived from what `tls.rs` and `store.rs` actually consume, not from a wish list.

```rust
// anyflow-core. PROPOSED — illustrative only.

/// How the private key is protected. Reported locally; never on the wire
/// (PLAT-DEC-012 — an unverifiable self-report is not a security property).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyBacking {
    Software,
    Tpm,
    SecureEnclave,
    Keystore { strongbox: bool },
}

/// Everything the protocol needs from a device identity.
///
/// Deliberately does NOT expose the private key. That is the whole point:
/// a TPM, Secure Enclave or Keystore key can sign and can never be exported.
pub trait IdentityProvider: Send + Sync + std::fmt::Debug {
    fn device_id(&self) -> &str;
    fn device_name(&self) -> &str;
    fn platform(&self) -> anyflow_proto::v1::Platform;   // fixes C2

    fn certificate_der(&self) -> &CertificateDer<'static>;
    fn fingerprint(&self) -> Fingerprint;

    /// The rustls signing handle. For the software backing this wraps the
    /// PKCS#8 key exactly as today; for a hardware backing it calls the
    /// platform API. `SigningKey::choose_scheme` is where P-256 is asserted.
    fn signing_key(&self) -> Arc<dyn rustls::sign::SigningKey>;

    fn backing(&self) -> KeyBacking;

    /// The equivalent of `require_private_mode` for this backing:
    /// 0600 on Unix, a per-user DACL on Windows, trivially Ok for hardware.
    fn verify_protection(&self) -> Result<()>;
}
```

**Why `signing_key()` and not `sign()`.** Research v1 sketched
`fn sign(&self, scheme, message) -> Result<Vec<u8>>`. Verification against rustls 0.23.43 shows that
is the wrong shape: rustls needs `Arc<dyn SigningKey>`, whose `choose_scheme(&[SignatureScheme])`
returns a `Box<dyn Signer>` **per handshake**. Flattening that into one `sign()` would force
`anyflow-core` to reimplement scheme negotiation — re-solving, less well, something rustls already
does. **Return the rustls type.**

Three details from the rustls docs the implementations must honour
([26 §10](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)):

1. **`Signer::sign` is synchronous and non-async.** No `await`, no user-presence prompt.
2. **The message is *not* pre-hashed.** For `ECDSA_NISTP256_SHA256` the implementation must SHA-256
   then sign. Apple's `.ecdsaSignatureMessageX962SHA256` and Android's `SHA256withECDSA` do this
   internally; Windows CNG's `NCryptSignHash` does not and must be given the digest.
3. **ECDSA output must be X9.62 DER `SEQUENCE { INTEGER r, INTEGER s }`.** Apple returns DER;
   CNG returns raw IEEE-P1363 and must be converted (`rustls-cng` already does this).

**Wave 0 ships exactly one implementation**, in `anyflow-linux`:

```rust
pub struct FileIdentity { /* … cert, PKCS#8 bytes, fingerprint … */ }
// signing_key() → the same rustls software key path as today
// backing()     → KeyBacking::Software
// verify_protection() → today's require_private_mode
```

### 6.2 The `tls.rs` change, exactly

The entire portability change to the most security-critical file in the repository:

```rust
// core/src/tls.rs — server_config(), currently line 288
-        .with_single_cert(
-            vec![identity.certificate_der().clone()],
-            identity.rustls_private_key(),
-        )
-        .map_err(Error::Tls)?;
+        .with_cert_resolver(Arc::new(SingleCertResolver::new(identity)));

// core/src/tls.rs — client_config_with_alpn(), currently line 342
-        .with_client_auth_cert(
-            vec![identity.certificate_der().clone()],
-            identity.rustls_private_key(),
-        )
-        .map_err(Error::Tls)?;
+        .with_client_cert_resolver(Arc::new(SingleCertResolver::new(identity)));
```

Verified against docs.rs for **rustls 0.23.43**, the pinned version:

```rust
// WantsServerCert
pub fn with_cert_resolver(self, cert_resolver: Arc<dyn ResolvesServerCert>) -> ServerConfig
// WantsClientCert
pub fn with_client_cert_resolver(self, r: Arc<dyn ResolvesClientCert>) -> ClientConfig
```

**Note both return the config directly, not a `Result`** — so `.map_err(Error::Tls)?` is removed on
both lines. The resolver wraps `CertifiedKey::new(cert_chain, provider.signing_key())`, and
`CertifiedKey` holds `Arc<dyn SigningKey>` with no PKCS#8 anywhere.

Everything else in `tls.rs` — `TLS13_ONLY`, `PinnedServerCertVerifier`,
`RecordingClientCertVerifier`, `verify_tls13_signature`, ALPN, `send_tls13_tickets = 0`,
`peer_fingerprint` — **is untouched**. That is the property that makes Wave 0 acceptable at all: the
pinning verifiers, which `tls.rs` itself calls "the single most dangerous thing in this codebase",
are not in the diff.

### 6.3 `SecretStore`

```rust
pub trait SecretStore: Send + Sync {
    fn read_secret(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn write_secret(&self, name: &str, data: &[u8]) -> Result<()>;
    fn read_state(&self) -> Result<Option<Vec<u8>>>;
    fn write_state(&self, data: &[u8]) -> Result<()>;
    /// Enforces the platform's private-storage guarantee. Must be a hard
    /// error, never a warning: 0700 on Unix, an owner-only DACL on Windows.
    fn harden(&self) -> Result<()>;
}
```

The **policy** — schema version, refusing a newer schema, atomic write-then-rename, JSON shape,
trust-store semantics — stays portable in `anyflow-core`. Only the *bytes-to-storage* step moves.

`write_atomic`'s security property must survive the move: the temp file is created **already at the
final mode**, so there is no window in which the key is world-readable. The trait's contract must
say so, or a Windows implementation will chmod-after-the-fact and reintroduce the window.

### 6.4 `ControlTransport`

```rust
pub trait ControlTransport: Send + Sync {
    type Listener: ControlListener;
    /// Binds the local control endpoint.
    ///
    /// MUST fail, distinguishably, when the endpoint name is already owned by
    /// another process. Implementations MUST NOT fall back to a different name:
    /// a fallback is what an attacker wants, because clients would then have to
    /// search. See R-06 and the Windows FILE_FLAG_FIRST_PIPE_INSTANCE policy.
    fn bind(&self) -> Result<Self::Listener, BindError>;
}

pub enum BindError {
    /// Another process already owns this endpoint. Fatal. Never retry.
    AlreadyOwned { detail: String },
    Io(std::io::Error),
}
```

`AlreadyOwned` exists in Wave 0 **even though Linux barely needs it**, because it is the shape the
Windows named-pipe mitigation requires, and retrofitting a new error variant into a trait after
adapters exist is exactly the churn Wave 0 is meant to prevent.

### 6.5 `FileSink`

```rust
pub trait FileSink: Send + Sync {
    fn default_directory() -> PathBuf where Self: Sized;
    fn open_temp(&self, id: TransferId) -> io::Result<(File, PathBuf)>;
    fn reserve(&self, name: &str) -> io::Result<PathBuf>;
    fn promote(&self, temp: &Path, name: &str) -> io::Result<PathBuf>;
}
```

The invariants stay in the capability crate and are **not** the adapter's to reinterpret:
a dedicated directory; the destination is never peer-influenced; `create_new` reservation so a
racing transfer cannot take the name; atomic rename so no partial file ever bears the final name;
`filename::sanitize` applied before anything touches the filesystem.

### 6.6 What stays in the portable core

Audited, not assumed. Everything below has **no** platform surface and moves nowhere:

`framing.rs` · `session.rs` (1586 lines, no `std::os`, no filesystem, no environment) ·
`tls.rs` (minus the two resolver lines) · `pairing.rs` · `qr.rs` · `fingerprint.rs` ·
`capability.rs` · `discovery.rs` (record model only — contains no responder) ·
`clipboard_policy.rs` · the `files.v1` state machine, `stream.rs`, `auth.rs`, `limits.rs` ·
`filename.rs` (**protocol-global by decision**, PLAT-DEC-014) · the clipboard `dedup`, `text`,
`limits`, `policy`, `redact` modules · `SessionHost` — already the right seam, unchanged.

---

## 7. Identity failure semantics

The brief asks for these explicitly, and [26 §11.2](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) shows the
current code gets it wrong **today, on Linux**.

| State | Meaning | Action |
| --- | --- | --- |
| `IDENTITY_NOT_CREATED` | No state file **and** no key. Genuine first run | Create. The **only** state that may generate a key |
| `IDENTITY_AVAILABLE` | Key present, protection verified, backing matches the recorded expectation | Proceed |
| `IDENTITY_TEMPORARILY_UNAVAILABLE` | Backing exists but is not usable now — device locked before first unlock, TPM busy | **Retry with backoff. Never regenerate.** Report "identity locked", not "not paired" |
| `IDENTITY_HARDWARE_UNAVAILABLE` | `state.json` records a hardware backing; the hardware is absent or the key is gone (TPM cleared, Enclave key invalidated) | **Fatal.** Refuse to start. Name the cause and the remedy. **Never regenerate** |
| `IDENTITY_CORRUPTED` | Key present but unparseable, or its public key does not match the stored certificate | **Fatal.** Refuse to start |
| `IDENTITY_LOST` | `state.json` present, key absent or unreadable | **Fatal.** Refuse to start. **This is the case the current code gets wrong** |

**The load-bearing rule:**

> A new identity may be generated **only** from `IDENTITY_NOT_CREATED`. Every other failure is
> fatal. `state.json` must never be written by a path that has not first established that the key
> situation is `NOT_CREATED` or `AVAILABLE`.

**Required code changes** (`core/src/store.rs`):

1. Replace `Path::exists()` with `Path::try_exists()` and handle the error. `exists()` returns
   `false` for `EACCES`, a broken symlink, or a non-traversable parent — which is why an unreadable
   key currently routes into `initialize()`.
2. Classify into the six states above before choosing a branch.
3. `initialize()` must be reachable only from `IDENTITY_NOT_CREATED`.
4. Add `key_backing` to `state.json`. Without a **recorded expectation** there is no way to
   distinguish "this device never had hardware backing" from "the hardware backing is gone" — and
   that distinction is the entire safety property (PLAT-DEC-015).
5. Bump `SCHEMA_VERSION`. Reading an older file yields `KeyBacking::Software`, which is correct for
   every identity that exists today.

Schema compatibility is one-way by existing design: `load()` already refuses a `schema_version`
newer than it supports, *"refusing to downgrade and risk losing trust records"*. That is the right
behaviour and Wave 0 relies on it.

---

## 8. Migration plan

Nine steps, each independently reviewable, each leaving the tree green. **Order matters:** the two
defects are fixed *before* the code that contains them is moved, so the move is a pure refactor.

| Step | Change | Test gate |
| --- | --- | --- |
| **1** | **P7** — move `unsafe_code` from workspace to per-crate | Full suite; `cargo clippy` clean |
| **2** | **P3** — fix identity failure semantics in `store.rs`; add `key_backing`; bump schema | `identity_and_store.rs` (24 tests) unmodified + **new** fault-injection tests |
| **3** | **P4** — SEC-004: add `:`, Unicode `Cf`, `CONIN$`/`CONOUT$`, `<>"|?*` to `filename.rs` | `filename.rs` unit tests + `files.rs` (36 tests) + **new** adversarial cases |
| **4** | Extract `anyflow-control` (types only); repoint CLI and GUI | GUI + CLI build **without** `anyflow-daemon`; `control.rs` (4 tests) unmodified |
| **5** | Introduce `IdentityProvider` + `KeyBacking`; `FileIdentity` in `anyflow-linux`; swap the two `tls.rs` lines to resolvers | `protocol.rs` (12), `pairing.rs` (21), `e2e.rs` (18), `sessions.rs` (6) **unmodified**; Android interop unchanged |
| **6** | Introduce `SecretStore`; move Unix modes/XDG into `anyflow-linux`; fix **C2** (`Platform` from the provider) | `identity_and_store.rs` unmodified |
| **7** | Introduce `FileSink`; move `destination.rs` platform bits to `anyflow-linux` | `files.rs` (36 tests) unmodified |
| **8** | Split `anyflow-daemon` → `anyflow-runtime` + `anyflow-linux`; `ControlTransport` with the UDS impl | `control.rs`, `listen.rs` (5), `wire.rs` (10) unmodified |
| **9** | Feature-gate `x11rb`; `anyflow-core` and capability crates carry no `std::os::unix` | Compile gate (§10) |

**Step 5 is the risky one** and is deliberately placed after the two defect fixes, so that if it is
reverted the security improvements stay.

**Steps 2 and 3 are shippable on their own.** If Wave 0 is abandoned after step 3, AnyFlow is
strictly better than it is today: two real defects fixed, no architectural change. That is a
deliberate property of this ordering.

---

## 9. Compatibility constraints

Absolute, and each is testable:

| # | Constraint |
| --- | --- |
| **CC-1** | **No wire change.** No `.proto` edit; `PROTOCOL_VERSION_MAX` unchanged; no new field |
| **CC-2** | **The unmodified Android app must pair, connect and transfer with the post-Wave-0 daemon.** The regression check for the whole wave |
| **CC-3** | **Existing `state.json` files load.** A pre-Wave-0 install upgrades in place: same identity, same `device_id`, same fingerprint, same peer list, no re-pairing |
| **CC-4** | **Same identity file format.** `identity.key` stays PKCS#8 at 0600. Wave 0 changes *how the key is reached*, not what is on disk |
| **CC-5** | **No test may be modified to make the refactor pass.** If a test fails, the refactor is wrong. Adding tests is required; changing one is a review stop |
| **CC-6** | **`anyflow status` output stays stable**, except for an added key-backing line |
| **CC-7** | **The systemd unit and the RPM keep working**, unchanged, with binaries at the same paths |

CC-5 deserves emphasis. The 309 existing tests are the specification of behaviour Wave 0 must
preserve. A refactor that needs a test edited has changed behaviour, which is exactly what Wave 0
must not do.

---

## 10. Testing strategy

### 10.1 Preserve every existing gate

**309 tests** (209 integration across 16 files, 100 unit) must pass **unmodified**:

| Suite | Tests | Proves |
| --- | :-: | --- |
| `core/tests/identity_and_store.rs` | 24 | Identity persistence, mode enforcement, trust store |
| `core/tests/pairing.rs` | 21 | Pairing proof, domain separation, constant-time compare |
| `core/tests/protocol.rs` | 12 | Envelope, sequence, replay |
| `daemon/tests/files.rs` | 36 | Transfer state machine, filenames, limits |
| `daemon/tests/clipboard.rs` | 21 | Dedup, loop suppression, policy |
| `daemon/tests/e2e.rs` | 18 | Full pair → connect → transfer |
| `daemon/tests/wire.rs` | 10 | Byte-level framing |
| `daemon/tests/sessions.rs` | 6 | Liveness, overlapping sessions |
| `daemon/tests/listen.rs` | 5 | Dual-stack bind |
| `daemon/tests/control.rs` | 4 | Control socket protocol |
| `capabilities/clipboard/tests/security.rs` | 30 | Redaction, limits, policy |
| `capabilities/clipboard/tests/{loops,logging,real_backend}.rs` | 22 | Loop suppression, no content in logs, real backend |

### 10.2 New tests Wave 0 must add

| Area | Test |
| --- | --- |
| **Identity failure semantics** | Six cases from §7. Each fatal case must refuse to start **and leave `state.json` byte-identical** |
| **Filename hardening** | `a:b` → rejected/sanitised; `photo\u{202E}gnp.exe` → `Cf` stripped; `CONIN$` → rejected; `x\|y.txt`, `q?.txt` → handled |
| **`IdentityProvider`** | A test-double provider drives a full pinned handshake, proving nothing needs PKCS#8 |
| **`KeyBacking` round-trip** | Written to `state.json`, read back; an older file yields `Software` |
| **`ControlTransport::AlreadyOwned`** | Binding twice returns `AlreadyOwned`, not a generic IO error |
| **Portable-crate purity** | `anyflow-core` and the capability crates contain no `std::os::unix` — grep-based, in CI |

### 10.3 The compile gate

The acceptance criterion Research v1 proposed —
`cargo build --target x86_64-pc-windows-msvc` on a Linux host — **does not work**, and this sprint
established why. `ring` *"currently requires a C (but not C++) toolchain"*, and for Windows targets
*"Build Tools for Visual Studio 2022 … The 'Desktop development with C++' workflow must be
installed"* ([26 §V-10](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). MSVC libraries are not redistributable
onto a Linux runner.

**Corrected gate:**

```
# On a Windows CI runner (GitHub-hosted windows-latest has MSVC):
cargo check -p anyflow-proto -p anyflow-core -p anyflow-control \
            -p anyflow-capability-clipboard -p anyflow-capability-files \
            -p anyflow-capability-battery \
            --no-default-features --target x86_64-pc-windows-msvc
```

**`anyflow-runtime` is deliberately excluded** from the first gate. It depends on `mdns-sd`, whose
Windows behaviour is V-12/POC-WIN-02 — unresolved and not Wave 0's problem. Adding it later is a
one-line CI change.

> **Executed 2026-09-01 — PASS.** This gate is now
> `.github/workflows/portable-windows-msvc.yml` (CI-001), green on a GitHub-hosted
> `windows-2025-vs2026` runner with `host: x86_64-pc-windows-msvc`:
> [run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649),
> commit `cfd33f6`. Evidence — runner, toolchain, exact commands, dependency graph and `ring`'s
> MSVC objects — is recorded once, in
> [the sprint report §17](../../sprints/wave-0-platform-abstraction.md).

**Distinguish clearly**, because conflating them is how a project convinces itself a platform works:

| | Proves |
| --- | --- |
| **Compile check** | No `std::os::unix` leaked into a portable crate. **A boundary regression test, nothing more** |
| **Runtime certification** | The software behaves correctly on that platform. Requires a real machine, and is **Wave 5+**, never Wave 0 |

A green Windows `cargo check` says nothing about whether AnyFlow works on Windows. It says the
boundary held.

### 10.4 Hardware and environment

| Environment | Purpose | Needed for Wave 0? |
| --- | --- | :-: |
| Fedora + GNOME (current) | Full suite, clipboard certification | ✅ **yes** |
| Physical Android device | CC-2 interop regression | ✅ **yes** |
| Windows CI runner | §10.3 compile gate | ✅ **yes** (CI-hosted, free) |
| Debian/Ubuntu containers | Wave 2 | ❌ |
| Plasma VM | Wave 3 | ❌ |
| A Mac | Waves 7–9 | ❌ |
| Physical iOS device | Wave 9 | ❌ |

**Wave 0 needs no hardware the project does not already have.** This is a real property of the plan,
not an accident: it is what makes Wave 0 startable now. (Note: the Android device available for
testing is an SM-X620 tablet, not the phone the earlier briefs name — sufficient for CC-2.)

---

## 11. Security invariants

The single acceptance criterion for the security half:

> **Wave 0 changes where keys live and how local IPC is guarded. It changes nothing about who is
> trusted or why.**

Concretely, each independently checkable:

| # | Invariant |
| --- | --- |
| **SI-1** | TLS 1.3 only. `TLS13_ONLY` unchanged |
| **SI-2** | SPKI pinning remains the sole identity check. Both verifiers unchanged |
| **SI-3** | Hostnames and IPs are never identity. No SAN validation reintroduced |
| **SI-4** | Pairing stays explicit and human-confirmed. `pairing.rs` untouched |
| **SI-5** | Per-capability grants re-checked per message. `capability.rs` untouched |
| **SI-6** | `files.v1` is never auto-granted |
| **SI-7** | No clipboard content in logs. `logging.rs` still passes |
| **SI-8** | **The private-key protection check is never weakened.** `verify_protection()` must be a hard error on every backing. A platform without an equivalent does not get a pass — it gets an implementation |
| **SI-9** | **`initialize()` is reachable only from `IDENTITY_NOT_CREATED`** (§7) |
| **SI-10** | **Filename sanitisation is protocol-global.** No `#[cfg]` in `filename.rs` (PLAT-DEC-014) |
| **SI-11** | `unsafe_code = "forbid"` retained on `anyflow-core`, `anyflow-proto`, `anyflow-control`, `anyflow-runtime` and all three capability crates |
| **SI-12** | Session resumption stays disabled (`send_tls13_tickets = 0`) — full handshakes are where pinning lives |

---

## 12. Acceptance gates

Wave 0 is done when **all** of these hold:

| # | Gate | How verified |
| --- | --- | --- |
| **G1** | 309 existing tests pass, **unmodified** | `cargo test --workspace` + `git diff --stat` on `tests/` shows additions only |
| **G2** | New tests from §10.2 pass | `cargo test --workspace` |
| **G3** | Windows compile gate passes for the six portable crates | §10.3, Windows runner |
| **G4** | `anyflow-core` and the capability crates contain no `std::os::unix` | grep gate in CI |
| **G5** | GUI and CLI build without `anyflow-daemon` | `cargo tree -p anyflow-gui \| grep -c anyflow-daemon` → 0 |
| **G6** | **The unmodified Android app pairs, connects, sends and receives** | Manual, on the physical device |
| **G7** | A pre-Wave-0 `state.json` upgrades in place; same fingerprint, same peers, no re-pairing | Fixture + manual |
| **G8** | Identity fault injection: key deleted / `chmod 000` / dir non-traversable → refuses to start, `state.json` byte-identical | New tests |
| **G9** | `anyflow status` reports `KeyBacking::Software` | Manual |
| **G10** | Fedora hardware smoke: pair, clipboard both directions, file both directions, `sensitive_hint` | Manual, on the certification machine |
| **G11** | `cargo clippy --workspace --all-targets` clean; `unsafe_code` lints as specified | CI |
| **G12** | No `.proto` file changed; `PROTOCOL_VERSION_MAX` unchanged | `git diff --stat protocol/` empty |

**G1, G6 and G12 are the ones that cannot be waived.** They are, respectively: behaviour unchanged,
interoperability unchanged, protocol unchanged.

---

## 13. Files likely affected

Estimated from the audit. **No file below was modified in this sprint.**

| File | Change | Risk |
| --- | --- | :-: |
| `desktop/Cargo.toml` | `unsafe_code` per-crate; new members | Low |
| `desktop/core/src/identity.rs` | `IdentityProvider`, `KeyBacking`; `LocalIdentity` becomes the software impl | **High** |
| `desktop/core/src/tls.rs` | **Two lines** → cert resolvers | **High** |
| `desktop/core/src/store.rs` | Split portable/platform; §7 failure semantics; `key_backing`; schema bump | **High** |
| `desktop/core/src/lib.rs` | Re-exports | Low |
| `desktop/capabilities/files/src/filename.rs` | SEC-004 additions | **High** (security) |
| `desktop/capabilities/files/src/destination.rs` | `FileSink`; Unix bits move | Medium |
| `desktop/capabilities/clipboard/Cargo.toml` | `x11rb` optional | Low |
| `desktop/capabilities/clipboard/src/backend/mod.rs` | Contract wording (PLAT-DEC-009) | Low |
| `desktop/daemon/src/{control,server,main,state,listener,mdns}.rs` | Split across `anyflow-control` / `anyflow-runtime` / `anyflow-linux` | Medium |
| `desktop/cli/src/main.rs`, `desktop/gui/src/client.rs` | Depend on `anyflow-control`; use `ControlTransport` | Medium |
| `desktop/{control,runtime,platform-linux}/` | **New** | Medium |
| `desktop/*/tests/**` | **Additions only** (CC-5) | — |

**Not touched:** `protocol/**`, `android/**`, `.github/**`, `packaging/**` (until step 9 confirms
binary paths), `desktop/gui/src/views/**`, `docs/adr/**`, `docs/security/THREAT_MODEL.md`.

---

## 14. Risks and rollback

| Risk | Mitigation |
| --- | --- |
| **R-01 — the refactor regresses identity or trust** | CC-5: no test may be modified. Step 5 isolated and independently revertable. G1 + G6 + G7 |
| **Scope creep into Wave 1** | Non-goals in §2 are enumerated so a reviewer can point at them |
| **The `SigningKey` shape is wrong** | Verified against rustls 0.23.43 docs, not assumed. Step 5's own test uses a double, so the seam is proved before any adapter |
| **The schema bump breaks an existing install** | G7 is a hard gate. `load()` already refuses newer schemas |
| **`unsafe_code` relaxed too broadly** | SI-11 names the crates that keep `forbid` |
| **A platform difference is discovered mid-wave** | Wave 0 adds no platform, so there is nothing to discover. Any such finding belongs to Wave 5+ |

**Rollback.** Each of the nine steps is a separate PR on a single branch. Steps 1–3 are pure
improvements and never roll back. Steps 4–9 are structural; reverting any one leaves the tree green
because each step's gate is the existing test suite. **The worst case is stopping after step 3**,
which still delivers both defect fixes.

**Branch plan** — one branch, nine PRs, per
[22 §5](22-IMPLEMENTATION-ROADMAP.md)'s convention:

```
feature/wave0-platform-seams-v1
  PR 1  chore: scope unsafe_code lint per crate            (step 1)
  PR 2  fix: never regenerate identity over an unusable key (step 2)  ← ships alone
  PR 3  fix: harden filename sanitisation                   (step 3)  ← ships alone
  PR 4  refactor: extract anyflow-control                   (step 4)
  PR 5  refactor: IdentityProvider seam                     (step 5)  ← the risky one
  PR 6  refactor: SecretStore seam                          (step 6)
  PR 7  refactor: FileSink seam                             (step 7)
  PR 8  refactor: split runtime and linux adapter           (step 8)
  PR 9  chore: feature-gate x11rb; add compile gate         (step 9)
```

PRs 2 and 3 are marked "ships alone" deliberately: they are defect fixes with no architectural
content and should not wait for the refactor to land.

---

## 15. CI implications

**No CI change in this sprint, and none in the same commit as the refactor.** Design only.

| Job | When | Proves |
| --- | --- | --- |
| **CI-001** — Windows compile gate (§10.3) · ✅ **landed and green**, `.github/workflows/portable-windows-msvc.yml` | **After** Wave 0 lands green | A `std::os::unix` call in a portable crate fails the PR |
| **CI-002** — portable-crate purity grep | With CI-001 | Cheap backstop for CI-001 |
| **CI-003** — GUI against Ubuntu 24.04 libadwaita 1.5 | Wave 2 | The floor is a contract, not a coincidence (R-11) |
| macOS compile gate | Wave 7 | Deferred — needs a macOS runner |
| `clipboard-session` VM job | Wave 2–3 | Containers cannot host a Wayland compositor with a seat |

**Cross-compiling is not runtime certification.** CI-001 proves the boundary held. It does not
prove AnyFlow runs on Windows, and the roadmap must never treat a green CI-001 as Windows support.

---

## 16. Effort

**HYPOTHESIS.** Estimates are the least evidence-backed thing in this document.

| Step | Estimate |
| --- | --- |
| 1 — unsafe_code scoping | 0.5 d |
| 2 — identity failure semantics | 2 d |
| 3 — filename hardening | 1.5 d |
| 4 — extract `anyflow-control` | 1 d |
| 5 — `IdentityProvider` | **3 d** |
| 6 — `SecretStore` | 2 d |
| 7 — `FileSink` | 1.5 d |
| 8 — runtime/linux split | 3 d |
| 9 — x11rb gate + compile gate | 1 d |
| Interop + smoke (G6, G7, G10) | 1.5 d |
| **Total** | **≈ 17 engineer-days** |

Lower than Research v1's "M" for Wave 0, and the reason is
[26 §11.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md): the identity refactor is two call sites, and the
rustls seam is documentation-verified rather than PoC-gated. The two defect fixes (steps 2–3, 3.5 d)
are new work Research v1 did not know about, so the net change is smaller than it looks.

---

## 17. Wave 0 readiness

Against the brief's six criteria:

| # | Criterion | Status |
| --- | --- | :-: |
| 1 | Architecture boundary sufficiently defined | ✅ §4, §5 — six seams, each justified by a verified platform difference; six rejected |
| 2 | Identity seam sufficiently defined | ✅ §6.1, §6.2 — verified against rustls 0.23.43; the change is two call sites |
| 3 | No essential external-verification blocker open | ✅ Two remain (V-08, V-12); neither touches Wave 0 |
| 4 | P0 risks for the refactor have defined mitigation | ✅ R-01 (CC-5, G1/G6/G7), R-08 (§7), R-07 (§10.2), R-18 (§5) |
| 5 | Spec is implementable | ✅ Nine steps, named files, gates per step, branch plan |
| 6 | No critical Wave 0 decision blocked by an unrun PoC | ✅ PLAT-DEC-001/-009/-012/-014 all READY FOR RFC. The four P0 PoCs are Wave 0's *acceptance gates*, not prerequisites |

**Criterion 6 is the one that changed.** Research v1 had PLAT-DEC-001 at POC REQUIRED, which would
have blocked Wave 0 on POC-WIN-04 — a PoC needing Windows hardware and a TPM. Verifying the rustls
seam against its own documentation removes that dependency: the architecture question is answered,
and POC-WIN-04 now proves *Windows integration* in Wave 5, where it belongs.

### Implementation outcome — 2026-08-31

Wave 0 was implemented against this specification on
`feature/core-platform-abstraction-v1`. Full evidence:
**[docs/sprints/wave-0-platform-abstraction.md](../../sprints/wave-0-platform-abstraction.md)**.

| | |
| --- | --- |
| Rust tests | 375 passed, 0 failed (366 + 9 `real_backend` on an unlocked seat; baseline 303) |
| Android tests | 232 JVM + 21 instrumented (SM-X620), 0 failed; `android/**` unchanged |
| Official gates | **12 of 12 PASS** (§12) |
| P0 PoCs | POC-CORE-01 **PASS** · -02 **PASS** · -03 **PASS** · -04 **PASS** ([run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649)) |
| Status | **WAVE 0 CERTIFIED — 2026-09-01** |

**What the spec got right.** The identity refactor was two call sites, exactly
as §6.2 predicted; `rustls::sign::SingleCertAndKey` implements both resolver
traits, so no custom resolver was needed. Nine steps, nine landings, no
protocol change, no `.proto` edit, no Android edit.

**Three amendments the implementation forced**, recorded so the next wave
inherits the corrected version rather than this one:

1. **The Unix filesystem adapter stayed in `anyflow-core`**, behind the
   default-on `unix-fs` feature, rather than moving to `anyflow-linux`. §5
   places it in the adapter, but `Store::open(dir)` is called from
   `core/tests/identity_and_store.rs`, and **CC-5** forbids editing a test to
   make the refactor pass. This specification's own compile gate is written
   `--no-default-features`, which is exactly the shape the feature provides,
   so the boundary the gate checks is unchanged.
2. **`ControlTransport` lives in `anyflow-control`, not `anyflow-runtime`.**
   §6.4 places it in the runtime, but `anyflow-gui` and `anyflow-cli` need the
   client half of the endpoint, and reaching it through the runtime would have
   re-created audit finding **C1** — the GUI inheriting mDNS, UPower and every
   capability crate for some struct definitions. The trait needs only tokio's
   `AsyncRead`/`AsyncWrite`, so `anyflow-control` stays dependency-light.
3. **A seventh trait, `IdentityBackend`, was added.** Not a seventh concern:
   it is the *creation* half of `IdentityProvider`, split out because
   `create()` is where hardware differs most. Folding it in would have forced
   an `export_secret()` method, reintroducing exportability into the one trait
   that exists to remove it.

**One defect this specification did not know about**, found while moving the
control socket: `server::bind` removed an existing socket file
unconditionally, reasoning that *"a second live daemon would have failed its
own port bind first"* — but the port bind happens **after** it in `main`. Fixed
with a live-owner probe and `BindError::AlreadyOwned`, which §6.4 already
required for the Windows named-pipe mitigation.

**Two existing tests were modified**, against CC-5, both because §7 items 4–5
*mandate* adding `key_backing` and bumping `SCHEMA_VERSION`, and those two
tests are the ones that notice a schema change. Neither assertion changed. Both
are itemised in the sprint report §8.

### Declaration

**WAVE 0 READY.** *(Implemented; see above.)*

With one condition that is a sequencing note rather than a blocker:

> **Steps 2 and 3 (the two defect fixes) should land before or with step 5.** Both live in code the
> refactor moves. Fixing them after the move means fixing them in a new abstraction that was built
> around the broken behaviour, and it means the `state.json`-destroying path exists in two
> structures instead of one.
