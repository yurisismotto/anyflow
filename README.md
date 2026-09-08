# AnyFlow

**One flow. Any device.**

Open-source, local-first device continuity.

No required cloud. No vendor lock-in. No telemetry by default.

Devices find each other on the local network, authenticate with pinned public
keys over TLS 1.3, and only after an explicit, human-confirmed pairing.

> **Status: `clipboard.v1` Sprint.** On top of the certified foundation —
> identity, discovery, pairing, authenticated transport, ping/pong,
> `battery.v1` and `files.v1` — this adds **text clipboard sharing**.
> Notifications, media control and browser integration are **not**
> implemented; the architecture is built to receive them, and that is all.
>
> **`notifications.v1` is approved in design only.** This release contains
> **no notification listener and no notification code**. The design — an
> optional, off-by-default Android notification mirror, requiring both the
> Android OS notification-access grant *and* a separate per-peer grant, with no
> history, no cloud and no telemetry — is recorded in
> [ADR-0015](docs/adr/ADR-0015-notification-access.md) and specified in
> [docs/research/notifications-v1/](docs/research/notifications-v1/).
>
> Clipboard sharing is, precisely: **automatic Fedora → Android sync**
> (opt-in, per device) and **manual Android → Fedora send**. It is not
> "automatic bidirectional clipboard", and saying so would be wrong: Android
> 10+ refuses clipboard reads to an app without input focus, and AnyFlow uses
> none of the techniques that defeat that. See
> [docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md).

## Principles

1. Local-first. The LAN is the only transport.
2. No cloud service, no account, no telemetry.
3. Nothing in plaintext. TLS 1.3 only.
4. Identity is a public key — never an IP, hostname or MAC address.
5. Discovery is not trust. Reachability grants nothing.
6. Explicit pairing, confirmed by a human, with key pinning afterwards.
7. Every feature is a separately granted capability.
8. No root, no accessibility service, no ADB, no hidden permissions. A
   privileged Android capability is acquired only for a named, user-visible
   feature, through the platform's own API for it, with separately revocable
   consent — and never to defeat a restriction that protects the user
   ([ADR-0015](docs/adr/ADR-0015-notification-access.md)).
9. Logs never contain user content.

## Layout

```
anyflow/
├── protocol/proto/            Wire format — compiled by BOTH implementations
├── desktop/                   Rust workspace
│   ├── proto/                 Generated protobuf types
│   ├── core/                  Identity, pairing, TLS, framing, session
│   ├── capabilities/battery/  battery.v1
│   ├── capabilities/files/    files.v1 — transfers, filename safety, stream auth
│   ├── capabilities/clipboard/ clipboard.v1 — text rules, policy, loop suppression
│   ├── daemon/                anyflowd
│   ├── cli/                   anyflow
│   └── gui/                   (placeholder — GTK4/Libadwaita, later)
├── android/                   Kotlin + Compose app
├── browser-extension/         (placeholder)
├── packaging/fedora/          systemd user unit, RPM spec
└── docs/
    ├── architecture/          OVERVIEW.md, PROTOCOL.md, FILES.md
    ├── security/              THREAT_MODEL.md
    ├── research/              cross-platform expansion, notifications.v1
    └── adr/                   ADR-0001 … ADR-0015
```

## Running on Fedora

Needs a Rust toolchain and a C compiler (`sudo dnf install gcc`). It does
**not** need `protobuf-compiler`: the schema is compiled by `protox`, in pure
Rust ([ADR-0004](docs/adr/ADR-0004-protocol-buffers.md)).

```bash
cd desktop
cargo build --release
cargo test --workspace          # 192 tests

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

File transfer is a separately granted capability and is **never** granted
automatically — writing a file to your disk is a side effect
([ADR-0008](docs/adr/ADR-0008-capability-architecture.md)):

```bash
anyflow grant <device> files.v1     # allow file transfer with this device
anyflow send <device> ~/photo.jpg   # offer a file; streams progress
anyflow transfers                   # everything since the daemon started
anyflow cancel <transfer-prefix>    # stop one mid-flight
anyflow revoke <device> files.v1    # withdraw; stops transfers already running
```

Received files land in `<XDG downloads>/AnyFlow`. An existing name is never
overwritten — `photo.jpg` becomes `photo (1).jpg`. See
[docs/architecture/FILES.md](docs/architecture/FILES.md).

Clipboard sharing is likewise never granted automatically — a device that can
write your clipboard can also see what you paste next:

```bash
anyflow grant <device> clipboard.v1        # allow clipboard sharing
anyflow clipboard status                   # what works here, and per-device policy
anyflow clipboard send <device>            # send the current clipboard, now
anyflow clipboard send <device> --sensitive  # ask the phone to mark it sensitive
anyflow clipboard apply <device>           # apply a clip that is waiting
anyflow clipboard auto-send <device> on    # push every local copy to that device
anyflow clipboard auto-receive <device> on # apply its clips as they arrive
```

Granting is one decision; automation is another. A freshly granted device can
send and receive **by hand**, and both automatic directions start **off** —
`auto-send` means everything you copy leaves this machine, and `auto-receive`
means that device can replace what you are about to paste. Clipboard content
is never written to disk and never logged, at any level. See
[docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md).

The daemon has no terminal, so it cannot prompt: it **declines** incoming
files and logs why. `anyflowd --accept-files-without-asking` is the documented
escape hatch for an unattended test rig.

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
cd desktop && cargo test --workspace              # 292 tests
cd android && ./gradlew :app:testDebugUnitTest    # 206 tests

# Touches the real system clipboard, so it is opt-in:
cd desktop && cargo test -p anyflow-capability-clipboard --test real_backend \
    -- --ignored --test-threads=1                 # 9 tests

# On a connected Android device:
cd android && ./gradlew :app:connectedDebugAndroidTest   # 21 tests
```

The Rust suite includes end-to-end pairing over real TLS on loopback and a
hostile-client suite that replays envelopes, duplicates message ids, rewinds
sequence numbers, claims another device's fingerprint and skips the handshake.

The Android suite covers the same wire rules on the Kotlin side, and checks
pinning against real certificates emitted by the desktop implementation
(`protocol/testdata/`).

Three known-answer vectors are asserted by **both** suites, so the
implementations cannot drift apart silently:

* the pairing proof and confirmation HMACs,
* the SPKI fingerprints of the shared certificate fixtures, and
* the `clipboard.v1` content hash — SHA-256 over the UTF-8 bytes — along with
  the text rules around it, since a clip one platform sends and the other
  refuses is a bug rather than a policy.

## Known limitations

* **`files.v1` has not run against a physical phone.** It is exercised end to
  end against the real `anyflowd` binary over real TLS, in both directions,
  with SHA-256 verification — but by `fake_phone`, which is a test client and
  must never be reported as a phone. The Android send and receive paths
  (Sharesheet intent handling, `ContentResolver` reads, MediaStore writes) are
  covered by unit and instrumented tests but have not been run on a device.
* **Widening a capability grant takes effect on the next connection.** A
  session's effective capability set is fixed at handshake time, so after
  `anyflow grant … files.v1` the phone must reconnect. *Narrowing* is
  immediate, including against a transfer already running — the asymmetry
  fails in the safe direction, but it is a rough edge.
* **No resume.** A transfer interrupted by a disconnect fails and its partial
  file is deleted. The receiver already knows the expected size and digest, so
  resume is tractable, but it needs durable partial state that this version
  deliberately does not keep.
* **One file per share.** `ACTION_SEND_MULTIPLE` is registered so AnyFlow
  appears for multi-select, but only the first item is sent.
* The trust store's persistence path is not covered by the local JVM unit
  tests: it needs a real `Context` and `filesDir`. Its pure logic is tested,
  and `ClipboardPersistenceTest` now covers the file I/O on a device.
* **Automatic Android → Fedora clipboard is not implemented, and will not be.**
  Android 10+ refuses clipboard reads to an app without input focus, and every
  way around it is forbidden or user-hostile. Android → Fedora is a deliberate
  action: the Send clipboard button, the Quick Settings tile, or sharing text
  to AnyFlow. Verified on an SM-X620 (Android 16): background read REFUSED,
  focused read ALLOWED, background `setPrimaryClip` APPLIED.
* **The desktop clipboard needs an unlocked session.** On GNOME Wayland,
  `wl-copy` and `wl-paste` block behind the lock screen rather than failing.
  Every call is bounded by a timeout and reported as such, so nothing hangs —
  but clipboard sync does not work while the screen is locked.
* **Clipboard auto-send needs a compositor that can report clipboard changes.**
  GNOME implements neither wlr- nor ext-data-control, so AnyFlow watches via
  XFIXES on the Xwayland `CLIPBOARD` selection instead (ADR-0014). Without
  Xwayland there is no watcher and `auto-send` degrades to manual sending,
  which `anyflow clipboard status` reports.
* The desktop private key is protected by filesystem permissions, not by
  hardware. TPM2 sealing is the top security debt
  ([ADR-0006](docs/adr/ADR-0006-device-identity-and-pairing.md)).
* mDNS advertises a stable device id and name, which is a modest tracking
  signal on untrusted networks (threat model, T18). `--no-mdns` is the blunt
  workaround; a per-network toggle is the proper fix.
* No GUI yet. The daemon and CLI are the whole desktop surface.

## License

Apache-2.0. See [LICENSE](LICENSE).
