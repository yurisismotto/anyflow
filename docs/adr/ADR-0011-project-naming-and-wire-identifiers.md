# ADR-0011 — Project naming and wire identifiers

Status: Accepted

## Context

The project was originally called *Fedroid Bridge*. That name encoded two
assumptions the architecture does not actually make: that the desktop is
Fedora, and that the product is a "bridge" between exactly two devices. The
protocol is neither Fedora-specific nor two-device-specific — it is a
capability-negotiating, identity-pinned session protocol that happens to have
a Fedora daemon and an Android client as its first two implementations.

The project is now **AnyFlow** — *One flow. Any device.*

The old name was not only in prose. It was in crate names, binary names, the
Android package, the protobuf package, and — most importantly — in values that
go on the wire and into cryptographic computations.

## Decision

Rename everything, including the wire identifiers, with no compatibility
aliases.

There is no public release and no deployed installation, so there is nothing
to stay compatible with. Keeping `fedroid` alive inside a domain separator or
an ALPN string purely to avoid touching the protocol would leave a permanent,
inexplicable artefact in a security-relevant constant. A reader three years
from now should not have to learn a dead product name to understand why the
HMAC input starts the way it does.

### Product and binaries

| Thing | Value | Note |
| --- | --- | --- |
| Product | `AnyFlow` | |
| Daemon binary | `anyflowd` | |
| CLI binary | `anyflow` | `anyflow status`, `pair`, `devices`, `ping`, `unpair` |
| systemd user unit | `anyflowd.service` | see below |
| RPM package | `anyflow` | |

**Why `anyflowd.service` and not `anyflow.service`.** The unit supervises the
daemon binary, and `anyflow` is a separate, user-facing CLI that is *not* a
service. Naming the unit after the binary it runs means `systemctl --user
status anyflowd`, `ps`, and the journal all show one identifier, and it
removes the trap where `systemctl --user start anyflow` and `anyflow status`
look like the same noun but are not.

### Wire and cryptographic identifiers

These are protocol constants. Changing them **is** a protocol change, and was
made deliberately as one.

| Identifier | Value |
| --- | --- |
| ALPN | `anyflow/1` |
| mDNS service type | `_anyflow._tcp.local.` |
| QR payload prefix | `anyflow1:` |
| Pairing proof domain | `anyflow/pairing-proof/v1` |
| Pairing confirm domain | `anyflow/pairing-confirm/v1` |
| Certificate subject CN | `anyflow:<device-id>` |
| Protobuf package | `anyflow.v1`, `anyflow.v1.capabilities` |
| Android package / applicationId | `io.github.yurisismotto.anyflow` |
| Android Keystore alias | `anyflow-identity-v1` |

**QR prefix.** `anyflow1:` keeps the scheme tag and its version in one token,
so a future `anyflow2:` payload is *recognised and rejected* by a v1 parser
rather than misparsed. The previous form was `fedroidb1:`, where the `b`
stood for "bridge"; that letter carried no version information and is gone.

**Domain separators.** These are HMAC inputs. `anyflow/pairing-proof/v1`
carries its own `/v1`, independent of the QR version and of the protobuf
package version, because the three can legitimately move at different times.

**Android namespace.** `io.github.yurisismotto.anyflow` was derived from the
repository's actual owner (`git remote -v` →
`github.com/yurisismotto/anyflow`), not invented. The `io.github.<owner>`
form is stable as long as the GitHub account exists, which avoids a second
forced rename later; the project owns no DNS domain, so a `dev.` or `com.`
namespace would have been a claim it cannot back.

## What deliberately did not change

The rename touched names only. Framing, the certificate and key formats, the
fingerprint construction, the proof construction and its length prefixing, the
anti-replay rules, sequence semantics, trust semantics and capability
negotiation are all byte-for-byte as they were. The cross-language known-answer
vectors in `desktop/core/tests/pairing.rs` and `PairingProofTest.kt` exist to
keep it that way, and were introduced during this rename precisely because the
existing tests could not have detected a divergence.

## Consequences

* Anyone with a pre-rename build must re-pair. Given there is no release, this
  affects development checkouts only.
* The dead name is gone from the codebase entirely, so a future `grep` for it
  returning a hit is unambiguously a bug.
* One more thing to get right when adding a capability: its id lives in the
  `anyflow.v1.capabilities` protobuf package.
