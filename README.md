# AnyFlow

**One flow. Any device.**

Open-source, local-first device continuity.

No required cloud. No vendor lock-in. No telemetry by default.

Devices find each other on the local network, authenticate with pinned public
keys over TLS 1.3, and only after an explicit, human-confirmed pairing.

> **Status: foundation Sprint.** Identity, discovery, pairing, authenticated
> transport, ping/pong and `battery.v1` are implemented and tested. Clipboard,
> file transfer, notifications, media control and browser integration are
> **not** implemented — the architecture is built to receive them, and that is
> all.
>
> Both modules build and their test suites pass. Pairing has **not** yet been
> exercised against a physical Android device; see
> [Known limitations](#known-limitations).

## Principles

1. Local-first. The LAN is the only transport.
2. No cloud service, no account, no telemetry.
3. Nothing in plaintext. TLS 1.3 only.
4. Identity is a public key — never an IP, hostname or MAC address.
5. Discovery is not trust. Reachability grants nothing.
6. Explicit pairing, confirmed by a human, with key pinning afterwards.
7. Every feature is a separately granted capability.
8. No root, no accessibility service, no ADB, no hidden permissions.
9. Logs never contain user content.

## Layout

```
anyflow/
├── protocol/proto/            Wire format — compiled by BOTH implementations
├── desktop/                   Rust workspace
│   ├── proto/                 Generated protobuf types
│   ├── core/                  Identity, pairing, TLS, framing, session
│   ├── capabilities/battery/  battery.v1
│   ├── daemon/                anyflowd
│   ├── cli/                   anyflow
│   └── gui/                   (placeholder — GTK4/Libadwaita, later)
├── android/                   Kotlin + Compose app
├── browser-extension/         (placeholder)
├── packaging/fedora/          systemd user unit, RPM spec
└── docs/
    ├── architecture/          OVERVIEW.md, PROTOCOL.md
    ├── security/              THREAT_MODEL.md
    └── adr/                   ADR-0001 … ADR-0010
```

## Running on Fedora

Needs a Rust toolchain and a C compiler (`sudo dnf install gcc`). It does
**not** need `protobuf-compiler`: the schema is compiled by `protox`, in pure
Rust ([ADR-0004](docs/adr/ADR-0004-protocol-buffers.md)).

```bash
cd desktop
cargo build --release
cargo test --workspace          # 79 tests

./target/release/anyflowd   # foreground, or install the user unit
```

As a service:

```bash
install -Dm0644 packaging/fedora/anyflowd.service \
    ~/.config/systemd/user/anyflowd.service
systemctl --user enable --now anyflowd.service
```

Then:

```bash
anyflow status              # identity, port, capabilities, live connections
anyflow pair                # opens a pairing window and prints a QR code
anyflow devices             # paired devices
anyflow ping <device>       # round-trip over the live session
anyflow unpair <device>     # revoke; takes effect immediately
```

`<device>` is a device id or a fingerprint prefix of at least 8 characters. An
ambiguous prefix is an error, never a guess.

The daemon never needs root.

## Running on Android

Needs JDK 21 and Android SDK platform 35. See
[android/README.md](android/README.md). Build with
`cd android && ./gradlew :app:assembleDebug`.

## Pairing

1. On Fedora: `anyflow pair`. A QR code appears; it is valid for 120 seconds
   and works once.
2. On the phone: **Scan pairing code**.
3. The phone pins the computer's key *from the QR*, before opening a socket —
   so the first connection is already authenticated and there is no
   man-in-the-middle window.
4. The phone proves it holds the pairing code, bound to both identities and to
   a fresh nonce.
5. Fedora shows the phone's fingerprint. **Check it matches the phone's
   screen**, then accept.
6. Both sides store the other's public key. The token is destroyed.

Afterwards the phone reconnects on its own using the stored identities. A
network change does not require re-pairing.

## Security

Read [docs/security/THREAT_MODEL.md](docs/security/THREAT_MODEL.md).

Short version: identity is a hardware-backed P-256 key (Android Keystore /
StrongBox on the phone; a 0600 file the daemon refuses to start without on the
desktop). Transport is TLS 1.3 with mutual authentication and SPKI pinning.
Pairing uses a 160-bit single-use token proved via HMAC bound to both
identities and a server nonce. Replay is stopped by strictly increasing
sequence numbers and message-id de-duplication — never by timestamps, because
clocks disagree.

There is no build flag, debug variant or test helper anywhere in this
repository that disables certificate validation.

## Testing

```bash
cd desktop && cargo test --workspace     # 81 tests
cd android && ./gradlew :app:testDebugUnitTest   # 63 tests
```

The Rust suite includes end-to-end pairing over real TLS on loopback and a
hostile-client suite that replays envelopes, duplicates message ids, rewinds
sequence numbers, claims another device's fingerprint and skips the handshake.

The Android suite covers the same wire rules on the Kotlin side, and checks
pinning against real certificates emitted by the desktop implementation
(`protocol/testdata/`).

Two known-answer vectors are asserted by **both** suites, so the
implementations cannot drift apart silently:

* the pairing proof and confirmation HMACs, and
* the SPKI fingerprints of the shared certificate fixtures.

## Known limitations

* **Pairing has never run against a physical phone.** Both sides build, both
  test suites pass, and the desktop end-to-end suite pairs over real TLS
  between two processes — but no Android hardware has been in the loop yet, so
  the on-device behaviour of the Keystore, the foreground service and mDNS
  browsing is unverified. `desktop/daemon/examples/fake_phone.rs` is a test
  client, not a phone, and must never be reported as one.
* The trust store's persistence path is not covered by the local JVM unit
  tests: it needs a real `Context` and `filesDir`. Its pure logic is tested;
  the file I/O is not.
* The desktop private key is protected by filesystem permissions, not by
  hardware. TPM2 sealing is the top security debt
  ([ADR-0006](docs/adr/ADR-0006-device-identity-and-pairing.md)).
* mDNS advertises a stable device id and name, which is a modest tracking
  signal on untrusted networks (threat model, T18). `--no-mdns` is the blunt
  workaround; a per-network toggle is the proper fix.
* No GUI yet. The daemon and CLI are the whole desktop surface.

## License

Apache-2.0. See [LICENSE](LICENSE).
