# 14 — Cross-platform identity and key storage

| Field | Value |
| --- | --- |
| **Title** | Where the identity key lives on each platform, and how it reaches TLS |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Key generation, storage, hardware backing, non-exportability, TLS integration, fallbacks. |
| **Decision status** | PROPOSED. **PLAT-DEC-001**, **PLAT-DEC-004** and **PLAT-DEC-012** OPEN. |
| **Evidence** | REPO VERIFIED for the current model; OFFICIAL DOC VERIFIED for each platform's keystore; POC REQUIRED for every non-Android hardware path. |
| **⚠ Verification update** | The rustls seam is now **documentation-verified** against 0.23.43, and the refactor is **two call sites** ([26 §10, §11.3](26-EXTERNAL-VERIFICATION-CLOSEOUT.md)). Three constraints were added: `Signer::sign` receives an **unhashed** message; ECDSA output must be X9.62 **DER** (Apple returns it, Windows CNG does not); the Secure Enclave **cannot import an existing key**, so software→hardware migration re-pairs every peer (**PLAT-DEC-015**). Identity failure states are specified in [28 §7](28-WAVE-0-IMPLEMENTATION-SPEC.md). |
| **Related documents** | [01](01-CURRENT-ARCHITECTURE-AUDIT.md), [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md), [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [12](12-APPLE-SECURITY-AND-INTEGRATION.md), [20](20-SECURITY-THREAT-ANALYSIS.md), [ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md) |

---

## 1. What must not change

- **The identity is an ECDSA P-256 key pair.**
- **The identity is the SPKI**, not the certificate. `Fingerprint = SHA-256(DER SubjectPublicKeyInfo)`.
  A certificate can be re-issued from the same key without breaking a pairing.
- **The certificate is a self-signed envelope**, with no SANs, so nobody can reintroduce
  name-based trust.
- **Trust comes from pinning that fingerprint at pairing time.**
- **The device id is 128 random bits**, not derived from any hardware serial, because
  *"a device id must not be a stable cross-install tracking handle"*.

All REPO VERIFIED (`core/src/identity.rs`, `core/src/fingerprint.rs`). This document proposes
no change to any of it.

---

## 2. The P-256 decision, revisited five years early

[ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md) chose P-256 over Ed25519 for one
reason: Android Keystore offers hardware-backed EC P-256 essentially everywhere, and Ed25519
keystore support is neither universal nor usable for TLS client authentication through
`javax.net.ssl`.

Checking that decision against the platforms it did not consider:

| Platform | Hardware store | P-256 | Ed25519 |
| --- | --- | --- | --- |
| Android | Keystore (StrongBox / TEE) | ✅ | ⚠️ not universal, not TLS-usable |
| **Apple Secure Enclave** | Enclave | ✅ | ❌ **The Enclave supports *only* 256-bit EC keys** (OFFICIAL DOC VERIFIED) |
| **Windows TPM** | Microsoft Platform Crypto Provider | ✅ | ❌ TPM 2.0's mandatory ECC curve set does not include Ed25519 |
| Linux TPM2 (future) | TPM 2.0 | ✅ | ❌ same |

**Ed25519 would have made hardware-backed identity impossible on three of the four hardware
stores OmniBridge will ever care about.** A decision made for Android compatibility turns out to
be the only decision that works everywhere. It should be recorded as such, and it should not be
revisited.

---

## 3. Platform table

| Platform | API | Hardware-backed | ECDSA P-256 | Non-exportable | TLS-usable | Fallback | PoC |
| --- | --- | :-: | :-: | :-: | --- | --- | --- |
| **Linux** | File: `identity.key`, mode 0600 in a 0700 dir, mode **enforced at load** | ❌ | ✅ | ❌ | direct PKCS#8 → rustls | *(is the fallback)* | — |
| Linux (future) | TPM 2.0 via `tpm2-tss` / a PKCS#11 provider | ✅ | ✅ | ✅ | `SigningKey` | file key | **POC-LINUX-04** |
| **Windows** | CNG `MS_PLATFORM_CRYPTO_PROVIDER` (TPM) | ✅ | ✅ | ✅ | **`rustls-cng` `CngSigningKey`** | `MS_KEY_STORAGE_PROVIDER` + DPAPI | POC-WIN-03/04 |
| **macOS** | Secure Enclave via `SecKeyCreateRandomKey` / CryptoKit | ✅ | ✅ (only) | ✅ | custom `SigningKey` → `SecKeyCreateSignature` | Keychain `…ThisDeviceOnly` | POC-MAC-03/04 |
| **Android** | Keystore, StrongBox → TEE | ✅ | ✅ | ✅ | `KeyManager` / Conscrypt (**shipping**) | none needed | — |
| **iOS/iPadOS** | Secure Enclave | ✅ | ✅ (only) | ✅ | same as macOS | Keychain | POC-IOS-04 |

The asymmetry is stark and worth stating plainly: **after this expansion, Linux would be the
only OmniBridge platform without a hardware-backed identity.**

---

## 4. The structural blocker

`core/src/identity.rs`:

```rust
pub struct LocalIdentity {
    key_pkcs8_der: Vec<u8>,   // ← the whole problem
}
pub(crate) fn rustls_private_key(&self) -> PrivateKeyDer<'static> { … }
```

consumed by `tls.rs` via `with_single_cert` / `with_client_auth_cert`.

Correct for a software key. **Structurally impossible for any hardware key**, whose entire
purpose is that those bytes cannot be obtained.

Android sidesteps it by not using Rust: `DeviceIdentity.kt` holds a `PrivateKey` handle that
Conscrypt can sign with but nothing can read.

### The fix

rustls documents the extension point for keys that live "in an HSM, or in another process, or
perhaps another machine": implement `rustls::sign::SigningKey`, return a `rustls::sign::Signer`
from `choose_scheme`, and install it through `ResolvesClientCert` / `ResolvesServerCert`
(OFFICIAL DOC VERIFIED, `rustls::manual::_03_howto`).

```rust
// PROPOSED. Wave 0. Not implemented.
pub trait IdentitySigner: Send + Sync + std::fmt::Debug {
    fn certificate_der(&self) -> &CertificateDer<'static>;
    fn fingerprint(&self) -> Fingerprint;
    fn signing_key(&self) -> Arc<dyn rustls::sign::SigningKey>;
    /// Per-platform equivalent of `require_private_mode`. See 09 §3.
    fn verify_protection(&self) -> Result<()>;
    fn backing(&self) -> KeyBacking;
}

pub enum KeyBacking {
    SoftwareFile,                 // Linux today
    SoftwareOsProtected,          // DPAPI / Keychain (no hardware)
    Tpm,                          // Windows Platform Crypto Provider, future Linux TPM2
    SecureEnclave,                // macOS / iOS
    AndroidKeystore { strongbox: bool },
}
```

`LocalIdentity` becomes one implementation (`SoftwareFile`), not the only shape.

**`verify_protection()` is the part that is easy to forget and must not be.** Today
`Store::open` refuses to start if the key file is group- or world-readable — a hard error, not
a warning. Without an equivalent on the trait, the Windows software fallback would ship with no
protection check at all, which is precisely the silent weakening the sprint forbids.

---

## 5. Per-platform notes

### 5.1 Windows — the easy one

`rustls-cng` (rustls GitHub org, v0.7.1, rustls ^0.23 — the pinned version) already implements
`CngSigningKey` over an `NCryptKey`, supports ECDSA secp256r1, and exists for exactly this
purpose (OFFICIAL DOC VERIFIED, docs.rs). The Windows path is integration, not invention.

`MS_PLATFORM_CRYPTO_PROVIDER` is documented as ensuring "private keys are securely stored and
cannot be extracted, even by malicious software", and is incompatible with exportable export
policies — which is the guarantee OmniBridge wants.

Follow Android's pattern for provider selection: attempt the hardware provider, catch the
failure, fall back. `DeviceIdentity.kt` does this for StrongBox with a comment explaining why a
capability query is not enough — *"StrongBox is absent on many devices and throws only at
generation time, so the fallback has to be a catch rather than a capability query."* The same is
true of the TPM.

### 5.2 macOS / iOS — the one that must be written

No `rustls-secure-enclave` exists. Details, traps and the user-presence constraint are in
[12 §3–4](12-APPLE-SECURITY-AND-INTEGRATION.md). The headline constraints:

- Create the key **without** user presence — `Signer::sign` is synchronous and handshakes are
  unattended.
- `.ecdsaSignatureMessageX962SHA256` (message, not digest) — the double-hashing trap that
  already cost OmniBridge its Android v1 keys in a different form.
- Certificate must be built around an externally-held public key; verify the embedded SPKI is
  byte-identical to what `Fingerprint::from_certificate_der` extracts.

### 5.3 Linux — the platform left behind

Current: a 0600 file in a 0700 directory, mode enforced at load. `store.rs` explains why the
Secret Service was rejected: *"a `systemd --user` daemon can start before any keyring is
unlocked, and a daemon that blocks on a locked keyring at boot is worse than useless."*
That reasoning is sound and is unaffected by this expansion.

TPM 2.0 is the documented follow-up in ADR-0006. With `IdentitySigner` in place it becomes a
new implementation rather than a refactor. Two viable routes: `tpm2-tss` via
`tss-esapi`, or a PKCS#11 provider (`tpm2-pkcs11`) behind a generic PKCS#11 `SigningKey`. The
PKCS#11 route has a bonus: it would also cover smartcards and YubiKeys, which some users would
value more than the TPM.

**Recommendation: do not block the expansion on Linux TPM2, but schedule it explicitly
(POC-LINUX-04) so Linux does not silently become the weakest platform.** Note the practical
obstacle: unlike Windows and Apple, a Linux TPM is often not accessible to an unprivileged
user without `tss` group membership or a resource-manager configuration — which may make it
undeployable for the exact "no root required" model OmniBridge has.

### 5.4 Android — leave it alone

Shipping, hardware-backed, StrongBox-preferring, with the v1→v2 alias migration already handled.
There is nothing to gain and a working implementation to lose. **PLAT-DEC-010: keep Kotlin.**

---

## 6. Should a peer be told how a key is stored?

Tempting: add `key_backing` to `DeviceInfo` so a phone can show "this computer's key is in a
TPM".

**Recommendation: no, not in v1.** Reasons:

1. **It is a self-report.** A malicious or compromised peer claims whatever it likes. It is
   therefore not a security property, only a UI decoration — and a UI decoration that *looks*
   like a security property is worse than none.
2. **It is a protocol change** with the usual obligations (versioning, backward compatibility,
   fail-closed semantics), for zero enforcement value.
3. **The local user's own backing is the one that matters**, and that needs no protocol —
   `omnibridge status` and the UI can show it directly.

If it is ever revisited, it must be as *advisory display only*, never an input to any
authorization or policy decision, and it must be listed as such in the threat model.
**PLAT-DEC-012**, recommended direction: **local display only, no protocol change.**

---

## 7. Fallback policy

The rule, from the sprint's security principle:

> If a platform cannot offer a hardware-backed key, define the fallback explicitly. Do not
> pretend equivalence.

Concretely:

| Requirement | Detail |
| --- | --- |
| Attempt order | Hardware first, always. Catch failure; do not query capability |
| Fallback protection | Non-exportable **as far as the platform allows**: DPAPI user-scope on Windows, `…WhenUnlockedThisDeviceOnly` on Apple, 0600/0700 with enforcement on Linux |
| Visibility | `omnibridge status` reports the backing. The UI shows it. First-run says it |
| Silence | **Never** silently downgrade. A machine that had a TPM key and cannot reach the TPM must **fail to start**, not quietly generate a software key — that would be an identity change, breaking every pairing, presented as a hiccup |
| Re-keying | Never automatic. A key change is a new identity and invalidates all pairings; it must be an explicit user action with an explicit warning |
| Logging | Log the backing at startup, exactly as Android logs `isStrongBoxBacked` |

That fourth row is the one that could cause real damage. `Store::open` currently generates a
fresh identity on first run — the same code path that would run if a key became unreadable.
On Linux with a file that is a clear error; with a TPM the "unreadable" case is much more
likely (BIOS reset, TPM clear, OS reinstall, VM migration). **The hardware paths must
distinguish "no key yet" from "key exists but is unusable" and refuse to auto-regenerate in the
second case.** **SEC-009**, high priority.

---

## 8. Migration

Existing Linux installs have a software key and existing pairings. Introducing `IdentitySigner`
must not disturb them:

1. Wave 0's refactor is **behaviour-preserving**: the file-backed implementation does exactly
   what `LocalIdentity` does now, with the same mode checks. The existing tests
   (`core/tests/identity_and_store.rs`, including the "refuses a 0644 key" case) are the
   regression suite.
2. **No automatic migration to a TPM.** If Linux TPM2 support ships later, it is opt-in and
   generates a *new* identity, which means re-pairing. Say so.
3. `store.rs`'s `SCHEMA_VERSION` exists for exactly this and is currently 1. If the trust store
   gains a key-backing field, bump it and handle the old shape.

---

## 9. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **ARCH-002** | `IdentitySigner` trait; `LocalIdentity` becomes its file-backed implementation | **Wave 0, high** |
| **SEC-002** | `verify_protection()` per platform | High |
| **SEC-009** | Distinguish "no key" from "key unusable"; never auto-regenerate over an existing identity | **High** |
| **WIN-002** | CNG + `rustls-cng` implementation | High |
| **MAC-001** | `AppleSigningKey` over `SecKeyCreateSignature` | High |
| **LINUX-007** | Linux TPM2 / PKCS#11 signer | Medium |
| **UX-007** | Show key backing in `omnibridge status`, the UI, and first-run | Medium |
