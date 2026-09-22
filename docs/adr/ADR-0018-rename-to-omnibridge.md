# ADR-0018 — Rename to OmniBridge

Status: Accepted. **Supersedes the identifier tables of
[ADR-0011](ADR-0011-project-naming-and-wire-identifiers.md).**

## Context

The project was called **AnyFlow** — *One flow. Any device.* It is being
renamed to **OmniBridge** — *One bridge. Any device.* — before the public
v1.0.0 release, ahead of Packaging, Android signing and Google Play.

There has been **no public v1 release**. Nothing is installed anywhere except
development checkouts and the certification hardware, so there is no
compatibility contract to honour. This is the last moment at which public and
runtime identifiers can be changed without them becoming commitments.

ADR-0011 made exactly this decision once before, when *Fedroid Bridge* became
AnyFlow, and recorded its reasoning: keeping a dead product name alive inside
an ALPN string or an HMAC domain separator "would leave a permanent,
inexplicable artefact in a security-relevant constant." That reasoning applies
unchanged, so this ADR follows the same policy rather than inventing a new one.
The precedent is also visible in the tree: after the ADR-0011 rename, the only
surviving occurrence of `fedroid` anywhere in the repository was ADR-0011
itself.

## Decision

Rename everything, including the wire identifiers and the cryptographic domain
separators, **with no compatibility aliases and no dual-stack support**.

### Product, binaries and packages

| Thing | Was | Is |
| --- | --- | --- |
| Product | `AnyFlow` | **`OmniBridge`** |
| Tagline | `One flow. Any device.` | **`One bridge. Any device.`** |
| CLI binary | `anyflow` | **`omnibridge`** |
| Daemon binary | `anyflowd` | **`omnibridged`** |
| GUI binary | `anyflow-gui` | **`omnibridge-gui`** |
| systemd user unit | `anyflowd.service` | **`omnibridged.service`** |
| RPM package | `anyflow` | **`omnibridge`** |
| Rust crates | `anyflow-*` | **`omnibridge-*`** |
| Gradle root project | `AnyFlow` | **`OmniBridge`** |

### Identity

| Thing | Was | Is |
| --- | --- | --- |
| Desktop application id | `io.github.yurisismotto.anyflow` | **`io.github.yurisismotto.omnibridge`** |
| Android applicationId / namespace | `io.github.yurisismotto.anyflow` | **`io.github.yurisismotto.omnibridge`** |
| Notification fixture applicationId | `…anyflow.fixture` | **`…omnibridge.fixture`** |
| Android Keystore identity alias | `anyflow-identity-v2` | **`omnibridge-identity-v1`** |
| Android notification-secret alias | `anyflow-notification-secret-v1` | **`omnibridge-notification-secret-v1`** |
| Certificate subject CN | `anyflow:<device-id>` | **`omnibridge:<device-id>`** |

The desktop application id is one string used as the GApplication id, the
D-Bus name, the `.desktop` basename, the `Icon=` key, the icon-theme name, the
`StatusNotifierItem` `Id` and its `IconName`. It stays one string.

The Keystore alias namespace **restarts at v1**. It is not a continuation:
`anyflow-identity-v{1,2}` lived in the Android Keystore of
`io.github.yurisismotto.anyflow`, and an app cannot reach another app's
keystore entries. Nothing is migrated and nothing is destroyed. The
AnyFlow-era "delete the broken v1 key" sweep is therefore removed rather than
renamed — under the new applicationId it could only ever have deleted a key
this app had just written.

### Wire and cryptographic identifiers

These are protocol constants. Changing them **is** a protocol change, made
deliberately as one, and it takes effect on both peers in the same commit.

| Identifier | Was | Is |
| --- | --- | --- |
| Control ALPN | `anyflow/1` | **`omnibridge/1`** |
| Data-stream ALPN | `anyflow-data/1` | **`omnibridge-data/1`** |
| mDNS service type | `_anyflow._tcp.local.` | **`_omnibridge._tcp.local.`** |
| QR payload prefix | `anyflow1:` | **`omnibridge1:`** |
| Protobuf package | `anyflow.v1`, `anyflow.v1.capabilities` | **`omnibridge.v1`, `omnibridge.v1.capabilities`** |
| Protobuf `java_package` | `io.github.yurisismotto.anyflow.proto[.capabilities]` | **`io.github.yurisismotto.omnibridge.proto[.capabilities]`** |
| Pairing proof domain | `anyflow/pairing-proof/v1` | **`omnibridge/pairing-proof/v1`** |
| Pairing confirm domain | `anyflow/pairing-confirm/v1` | **`omnibridge/pairing-confirm/v1`** |
| `files.v1` data-stream domain | `anyflow/files.v1/data-stream/v1` | **`omnibridge/files.v1/data-stream/v1`** |
| `notifications.v1` id domain | `anyflow/notifications.v1/id/v1` | **`omnibridge/notifications.v1/id/v1`** |
| `notifications.v1` group domain | `anyflow/notifications.v1/group/v1` | **`omnibridge/notifications.v1/group/v1`** |
| `notifications.v1` content domain | `anyflow/notifications.v1/content/v1` | **`omnibridge/notifications.v1/content/v1`** |

**The TCP port stays 55432.** It carries no brand and changing it would only
invalidate firewall documentation.

### State and runtime paths

| Path | Was | Is |
| --- | --- | --- |
| Desktop state | `$XDG_DATA_HOME/anyflow`, `~/.local/share/anyflow` | **`…/omnibridge`** |
| Control socket | `$XDG_RUNTIME_DIR/anyflow/control.sock` | **`…/omnibridge/control.sock`** |
| Fallback runtime dir | `/tmp/anyflow-<uid>` | **`/tmp/omnibridge-<uid>`** |
| Desktop download subdir | `~/Downloads/AnyFlow` | **`~/Downloads/OmniBridge`** |
| Android download subdir | `Download/AnyFlow` | **`Download/OmniBridge`** |

Permissions are unchanged: state directory `0700`, identity and state files
`0600`, received files `0600`.

OmniBridge writes **only** to OmniBridge paths. Existing AnyFlow state is left
exactly where it is and is never read, moved or deleted. There is no migration
subsystem, because there is no released installation to migrate.

## What deliberately did not change

The rename touched **names only**. Specifically unchanged, and asserted by the
existing suites rather than assumed:

* TLS 1.3 only, ECDSA P-256 identities, SPKI SHA-256 pinning;
* the fingerprint construction, the proof construction and its big-endian
  32-bit length prefixing, the HMAC-SHA256 primitive, the 160-bit single-use
  token, its TTL and attempt limit, and the human confirmation gate;
* framing, envelope layout, sequence and anti-replay semantics, trust
  semantics, capability negotiation and per-peer grants;
* **every protobuf field number and field type.** This is a namespace rename,
  not a schema change.

The *values* of the six domain separators changed, so every cross-language
known-answer vector that consumes one was recomputed — independently, from the
construction as documented, not by copying what the implementation now emits.
The same script reproduces the committed AnyFlow-era vectors byte-for-byte
from the old domain strings, which is what demonstrates that the construction
itself did not move.

## Consequences

* **Every development pairing breaks, once.** A peer speaking `anyflow/1` and
  one speaking `omnibridge/1` fail during the TLS handshake, which is the
  intended failure mode: loud, immediate and unambiguous. Both ends must be
  rebuilt from this commit and re-paired. See
  [the migration note](../migrations/MIGRATION-ANYFLOW-TO-OMNIBRIDGE.md).
* **Android treats this as a different app.** A new `applicationId` means a
  new install, a new keystore, a new identity, and notification-listener
  access granted again by hand. The old app is not uninstalled by anything
  here.
* The dead name is gone from every active product path, so a `grep` for
  `anyflow` outside the classified remainder is unambiguously a bug. The
  remainder is enumerated in `OMNIBRIDGE-REBRAND-REMAINDER-AUDIT.md`.
* New: `desktop/core/tests/wire_identity.rs`, `desktop/proto/tests/namespace.rs`
  and `android/app/src/test/.../WireIdentityTest.kt` pin these identities as
  literals on both sides. ADR-0011's rename had no such test and needed one.

## Not decided here

* **The visual identity.** No official OmniBridge mark was supplied, so none
  was adopted or invented; the AnyFlow artwork is retained as a labelled
  placeholder. See `docs/design/BRAND.md`.
* **The GitHub repository name.** `github.com/yurisismotto/anyflow` is
  unchanged and still referenced by the crate metadata, the RPM spec and the
  systemd unit. Renaming the repository is a manual post-merge operation.
  *Post-merge update, 2026-09-21: that manual operation has since been carried
  out. The repository is now `github.com/yurisismotto/omnibridge`, and the
  three metadata references were updated to match. The old URL is retained
  where it appears inside historical evidence — CI run links, issue links and
  the certification reports — which is left as written.*
* **Packaging.** `omnibridged.service` exists as a source template only. This
  build still installs no user unit and no autostart entry; the D-Bus service
  file activates the *GUI*. That remains the Packaging sprint's work, with the
  live-bus `ReloadConfig` requirement from
  `KDE-PLASMA-REAL-CERTIFICATION-V1.md` §40 attached to it.
