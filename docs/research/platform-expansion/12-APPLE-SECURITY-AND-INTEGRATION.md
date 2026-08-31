# 12 — Apple security and integration

| Field | Value |
| --- | --- |
| **Title** | Keychain, Secure Enclave, entitlements, sandbox, signing, FFI — macOS and iOS together |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | The security-relevant Apple-platform concerns that [10](10-MACOS-FEASIBILITY.md) and [11](11-IOS-IPADOS-FEASIBILITY.md) state but do not analyse. |
| **Decision status** | PROPOSED. **PLAT-DEC-004** OPEN. |
| **Evidence** | OFFICIAL DOC VERIFIED where Apple pages were retrievable; **EXTERNAL VERIFICATION REQUIRED** is marked explicitly and is more common here than in any other document. |
| **Related documents** | [10](10-MACOS-FEASIBILITY.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md), [20](20-SECURITY-THREAT-ANALYSIS.md) |

---

## 1. Why macOS and iOS are one document

The Secure Enclave API, the Keychain API, `SecKeyCreateSignature`, code signing, entitlements
and the FFI shape are the same on both. The differences are the sandbox (mandatory on iOS,
optional on macOS) and the lifecycle. Analysing them together avoids writing the identity
section twice and — more usefully — makes it obvious that **the macOS identity work is most of
the iOS identity work**, which is the strongest argument for sequencing macOS first.

---

## 2. Key storage decision table

| | macOS (Developer ID, unsandboxed) | macOS (App Store, sandboxed) | iOS / iPadOS |
| --- | --- | --- | --- |
| Preferred store | **Secure Enclave** (T2 / Apple Silicon) | Secure Enclave | **Secure Enclave** |
| Fallback | Keychain, `…WhenUnlockedThisDeviceOnly` | Keychain | Keychain (Enclave absent only on very old devices) |
| Key type | ECDSA **P-256** — the only type the Enclave supports | same | same |
| Exportable | **No**, by construction | No | No |
| Access control | `.privateKeyUsage` only. **No user presence** — §3 | same | same |
| Keychain accessibility | `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` | same | same |
| Shared with an extension | n/a | App Group + `kSecAttrAccessGroup` | **App Group + `kSecAttrAccessGroup`** — required for the Share Extension |
| Synced to iCloud Keychain | **Never.** `ThisDeviceOnly` | Never | Never |

The `ThisDeviceOnly` choice is not incidental. AnyFlow's identity is a *device* identity: a
key that synced across a user's devices would make two devices indistinguishable to a peer's
pin, silently breaking the property that pairing binds one device. Enclave keys cannot sync
anyway; the attribute matters for the software fallback.

OFFICIAL DOC VERIFIED: the Secure Enclave supports only 256-bit elliptic-curve private keys —
NIST P-256 — for ECDSA signing and ECDH (`kSecAttrTokenIDSecureEnclave`, `SecureEnclave.P256`).

---

## 3. The user-presence constraint

This is the most important design constraint in the document and it is easy to get wrong in
the appealing direction.

`rustls::sign::Signer::sign` is **synchronous and blocking**. rustls's own guidance says the
mechanism is "designed for keys with low latency access, like in a PKCS#11 provider, Microsoft
CryptoAPI, etc. so is blocking rather than asynchronous" (OFFICIAL DOC VERIFIED).

A Secure Enclave key created with `.userPresence` or `.biometryCurrentSet` requires Face ID,
Touch ID or a passcode **at every signature**. AnyFlow signs during the TLS handshake, and
handshakes happen:

- when the app opens;
- on every reconnect after a network change;
- after sleep/wake;
- when a data stream opens for a file transfer (a **second** TLS connection —
  [ADR-0013](../../adr/ADR-0013-file-transfer-data-stream.md));
- unattended, on a desktop, with the user away.

A biometric prompt in any of those is either impossible (nobody is there) or actively hostile
(a Face ID prompt every time you unlock your laptop).

**Decision: create the identity key with `SecAccessControlCreateWithFlags(..., .privateKeyUsage, ...)`
and no presence requirement.** The key is still non-exportable and still Enclave-bound; it is
protected by the device being unlocked, not by a per-use gesture.

This mirrors Android exactly, which is reassuring: `DeviceIdentity.kt` generates its Keystore
key without `setUserAuthenticationRequired`, for the same reason — a background
`ConnectionService` must be able to reconnect without a human.

**If a future feature genuinely needs user presence** (say, confirming a pairing), it should be
a *separate* Enclave key or an app-level `LAContext` check — never a presence flag on the
identity key. Putting it there would be unrecoverable: keychain access control, like Android
Keystore authorisations, is fixed at creation.

---

## 4. The signer, end to end

The piece of work that does not exist anywhere and must be written:

```
┌─ Rust ─────────────────────────────────────────────┐
│ struct AppleSigningKey { key_ref: SecKey, … }      │
│ impl rustls::sign::SigningKey for AppleSigningKey  │
│   fn choose_scheme(&self, offered) -> Option<Box<dyn Signer>>
│       → ECDSA_NISTP256_SHA256 only                 │
│ impl rustls::sign::Signer                          │
│   fn sign(&self, message) -> Result<Vec<u8>>       │
│       → SecKeyCreateSignature(                     │
│             key,                                   │
│             .ecdsaSignatureMessageX962SHA256,      │
│             message)                               │
│       → returns X9.62 DER (r,s)                    │
│ impl rustls::client::ResolvesClientCert            │
│ impl rustls::server::ResolvesServerCert            │
└────────────────────────────────────────────────────┘
```

Four traps, all of which have bitten this project's Android implementation or its equivalents:

1. **Digest handling.** `.ecdsaSignatureMessageX962SHA256` hashes the message itself. rustls
   hands `sign()` the *unhashed* handshake transcript for TLS 1.3 signatures. Using a
   `…DigestX962SHA256` variant instead would double-hash and every handshake would fail with
   a bad-signature alert. This is the exact class of error that made AnyFlow's Android v1 keys
   unusable — `DeviceIdentity.kt` records that they were generated without
   `KeyProperties.DIGEST_NONE` and *"are therefore unusable for TLS client authentication"*.
2. **Signature encoding.** TLS 1.3 ECDSA signatures are DER `SEQUENCE { r, s }`, which is what
   `SecKeyCreateSignature` returns for the X9.62 algorithms — but verify it byte-for-byte
   against a real rustls peer, not against a unit test that only checks self-consistency.
3. **Certificate generation.** `rcgen` normally generates the key. With an Enclave key it must
   build a certificate around a public key it is *given* and have the Enclave sign the TBS.
   `rcgen` supports a remote-key-pair path; confirm the DER SPKI it embeds is byte-identical to
   what `Fingerprint::from_certificate_der` will later extract, or the pin will not match.
4. **Thread safety.** `SecKey` must be usable from rustls's thread. `SigningKey` is
   `Send + Sync`; the Rust wrapper must uphold that around a Core Foundation object.

**POC-MAC-03 and POC-MAC-04 must be run as one exercise** ending in a completed handshake with
the existing Linux `anyflowd`. Splitting them is how trap 1 survives to production.

---

## 5. Entitlements

### macOS (Developer ID, unsandboxed — recommended)

| Entitlement | Needed? |
| --- | --- |
| `com.apple.security.app-sandbox` | **No** — see §6 |
| Hardened Runtime | **Yes.** Required for notarization |
| `com.apple.security.cs.allow-jit` / `…-unsigned-executable-memory` | **No.** AnyFlow JITs nothing; not requesting them is a security win and a notarization simplification |
| `keychain-access-groups` | Only if a helper or extension shares the keychain |

### macOS (App Store, sandboxed — deferred)

| Entitlement | Needed |
| --- | --- |
| `com.apple.security.app-sandbox` | Yes |
| `com.apple.security.network.server` | Yes — the TLS listener |
| `com.apple.security.network.client` | Yes |
| `com.apple.security.files.downloads.read-write` | Yes — received files |
| `com.apple.security.files.user-selected.read-write` | Yes — the file picker |
| App Group | For the agent↔app channel, which must become XPC under the sandbox |

### iOS

| Item | Value |
| --- | --- |
| `NSLocalNetworkUsageDescription` | Required |
| `NSBonjourServices` | `_anyflow._tcp` |
| `NSCameraUsageDescription` | QR pairing |
| App Group | `group.io.github.yurisismotto.anyflow` — Share Extension ↔ app |
| `keychain-access-groups` | Matching, so the extension reaches the identity |
| `UIBackgroundModes` | **None.** Deliberately empty — [11 §6.2](11-IOS-IPADOS-FEASIBILITY.md) |

The empty `UIBackgroundModes` deserves emphasis: it is a *positive* declaration of the product
model, it makes App Review straightforward, and it is the thing that stops a future
contributor quietly adding `voip` to "fix" the background problem.

---

## 6. Sandbox: the macOS recommendation and its price

**Recommendation: unsandboxed, Developer ID, notarized. Defer the Mac App Store, possibly
permanently.**

What the sandbox costs AnyFlow on macOS:

| Need | Under the App Sandbox |
| --- | --- |
| TLS listener on 55432 | `network.server` — fine |
| mDNS | Fine via Network.framework; **`mdns-sd` binding 5353 is not** |
| Pasteboard polling | Pasteboard access is permitted; the *privacy* behaviour is the unknown ([10 §6.2](10-MACOS-FEASIBILITY.md)) |
| Write to `~/Downloads` | `files.downloads.read-write` — fine |
| Write anywhere else | Security-scoped bookmarks; a real complication for a "choose your download folder" setting |
| Background agent | `SMAppService` login items are constrained in a sandboxed app |
| Unix-socket control channel | Must become **XPC**; the existing `server.rs` no longer applies |

The last two are the expensive ones. Sandboxing turns "reuse `server.rs` with a different path"
into "write an XPC service", and it puts the agent architecture at the mercy of App Store
policy for login items.

Against that, the App Store buys discovery and a familiar install. For a local-first
developer-adjacent utility distributed via Homebrew Cask, that trade is not obviously worth it.

**But the price of deferring must be stated:** unsandboxed still requires Developer ID +
notarization, which requires an Apple Developer Program membership and a Mac in the release
pipeline. There is no free path to a macOS binary users can run without a Gatekeeper fight.

---

## 7. Code signing, notarization, distribution

OFFICIAL DOC VERIFIED:

- Gatekeeper checks for a Developer ID certificate on software distributed outside the App
  Store.
- Since macOS 10.14.5, software signed with a new Developer ID certificate must be notarized.
- Since macOS 10.15, all Developer ID software built after 2019-06-01 must be notarized.
- Notarization is an automated scan producing a ticket, which is then stapled to the artifact.

Pipeline requirements:

```
build (universal) → codesign --options runtime --timestamp (each binary, then the .app)
                  → create DMG → codesign the DMG
                  → xcrun notarytool submit --wait
                  → xcrun stapler staple
```

Every step needs macOS. **A Mac in CI is a hard requirement for macOS releases**, and
[22](22-IMPLEMENTATION-ROADMAP.md) treats it as a hardware prerequisite rather than a detail.

iOS adds App Store Connect, TestFlight and App Review. App Review considerations for AnyFlow:
the local-network usage string must be honest and specific; an empty `UIBackgroundModes` avoids
the most common rejection category for "sync" apps; and the pasteboard behaviour must not look
like surveillance.

---

## 8. FFI between Rust and Swift

| Concern | Guidance |
| --- | --- |
| Binding generator | **UniFFI** — production-proven for one-Rust-core-two-mobile-platforms (OFFICIAL DOC VERIFIED). Keeps a future Kotlin-consumes-Rust option open at no cost |
| Alternative | Hand-written C ABI + Swift wrapper. Fewer dependencies, more work, worse async story |
| **Callbacks Swift ← Rust** | The hard part. The Enclave signer means **Rust calls Swift synchronously on a rustls thread**. UniFFI supports callback interfaces; the constraint is that this callback must not touch the main actor, must not `await`, and must not deadlock |
| Async | AnyFlow's core is `tokio`-based. The Rust side should own its runtime and expose a *blocking* or *callback* surface to Swift rather than trying to bridge `Future` to Swift concurrency |
| Error surface | Errors crossing the FFI must not carry user content — the same rule `core/src/error.rs` and the `Error` protobuf message already enforce (*"MUST NOT contain user content … or secrets"*) |
| Memory | UniFFI handles it; hand-rolled FFI must not free across the boundary |
| Threading | rustls calls the signer on its own thread. `SecKey` use must be safe there |

**The signer callback is the single highest-risk piece of the Apple FFI** and should be the
first thing built, not the last. If it does not work, the whole "Rust core on Apple platforms"
strategy needs revisiting — which is why **POC-MAC-04** is a decision gate and not a task.

---

## 9. Extension isolation (iOS Share Extension)

A Share Extension runs in a **separate process** with its own sandbox. To send a file it needs:

| Resource | Mechanism |
| --- | --- |
| Identity key | Keychain **access group** shared with the app; the Enclave key must be created with that `kSecAttrAccessGroup` |
| Trust store | **App Group** shared container |
| Network | **PARTIALLY VERIFIED (V-09).** TN3179: *"In general, app extensions share the Local Network privilege state of their container app"*, and the `NSLocalNetworkUsageDescription` / `NSBonjourServices` keys go in **the app's** `Info.plist`, not the extension's. Residual: an extension "assumed to be running in the background" is denied without an alert while the privilege is *undetermined*, so onboarding must establish it in the foreground |
| Lifetime | Short. Extensions are killed aggressively; a large transfer will not finish in one |

Security consequences worth recording in [20](20-SECURITY-THREAT-ANALYSIS.md):

- A shared keychain access group **widens** who can use the identity key from "the app" to "the
  app and its extensions". That is acceptable — they ship in one signed bundle from one
  developer — but it must be a deliberate, documented decision, not a side effect.
- Two processes writing the trust store concurrently is a real hazard. `store.rs` already
  writes atomically via temp-file-plus-rename; the shared-container version needs the same
  discipline plus coordination (`NSFileCoordinator`), or the extension should be **read-only**
  with respect to the trust store. **Recommendation: extension is read-only. It may use an
  existing pairing; it may never create one.** That also keeps pairing — the one operation that
  requires a human — in the main app where the human is. **SEC-007.**

---

## 10. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **MAC-001** | `AppleSigningKey`: `rustls::sign::SigningKey` over `SecKeyCreateSignature` | High |
| **MAC-002** | Enclave key creation with `.privateKeyUsage` and no user presence | High |
| **MAC-003** | Certificate generation around an externally-held public key; verify SPKI byte-identity | High |
| **MAC-004** | Notarization pipeline (`codesign` → `notarytool` → `stapler`) | High |
| **IOS-002** | App Group + keychain access group for the Share Extension | Medium |
| **SEC-006** | Set `com.apple.quarantine` on received files | Medium |
| **SEC-007** | Share Extension is read-only against the trust store | Medium |
| **SEC-008** | Document the shared keychain access group as an accepted widening | Low |
