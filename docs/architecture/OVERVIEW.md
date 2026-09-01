# Architecture overview

## Shape of the system

```
        Fedora                                    Android
┌───────────────────────┐                 ┌───────────────────────┐
│  anyflow (CLI)        │                 │  Compose UI           │
│        │              │                 │        │              │
│  unix socket, JSON    │                 │  ConnectionService    │
│        │              │                 │  (connectedDevice FGS)│
│  anyflowd             │                 │        │              │
│  ┌─────────────────┐  │   TLS 1.3       │  ┌──────────────────┐ │
│  │ session         │◀─┼─────────────────┼─▶│ PeerConnection   │ │
│  │ capabilities    │  │   mutual auth   │  │ capabilities     │ │
│  │ trust store     │  │   SPKI pinned   │  │ trust store      │ │
│  │ identity (file) │  │                 │  │ identity(Keystore│ │
│  └─────────────────┘  │                 │  └──────────────────┘ │
│  mDNS responder       │◀── DNS-SD ──────│  NsdManager browse    │
└───────────────────────┘                 └───────────────────────┘
```

The phone always initiates. The desktop always listens. See ADR-0005.

## Rust crates

| Crate | Depends on | Role |
| --- | --- | --- |
| `anyflow-proto` | — | Generated protobuf types |
| `anyflow-core` | proto | Identity, pairing, TLS, framing, session, capability registry |
| `anyflow-capability-battery` | core, proto | `battery.v1` |
| `anyflow-capability-files` | core, proto | `files.v1`: transfer state machine, filename safety, data-stream auth |
| `anyflow-capability-clipboard` | core, proto | `clipboard.v1`: text rules, policy, loop suppression, Wayland/X11 backend |
| `anyflow-control` | — | The CLI/GUI ↔ agent contract: request/response types and the `ControlTransport` seam. No I/O |
| `anyflow-runtime` | core, control, battery, files, clipboard | The AnyFlow Agent, minus the platform: mDNS, listener, state, control server, `SessionHost` |
| `anyflow-linux` | core, control | The Linux adapter: Unix-socket control endpoint, XDG paths, 0600/0700 modes, store composition |
| `anyflow-daemon` | runtime, linux | `anyflowd` — composes the two and adds a `main` |
| `anyflow-cli` | control, linux | `anyflow` |
| `anyflow-gui` | control, linux | `anyflow-gui` |

`anyflow-core` has no global state and no I/O policy. Everything it needs from
the host arrives through the `SessionHost` trait, which is why the whole
protocol can be tested in-process over real TLS with no daemon, no filesystem
and no human.

## The platform boundary

Since Wave 0 the workspace has an explicit one. `anyflow-proto`,
`anyflow-core`, `anyflow-control` and the three capability crates are
**portable**: built with `--no-default-features` they contain no `std::os`, no
environment assumption and no filesystem assumption, and they compile for a
non-Unix target. Their host implementations live behind default-on Cargo
features, in modules that a `cargo test` gate
(`core/tests/portable_boundary.rs`) keeps enumerated.

Adding a platform means writing an adapter crate alongside `anyflow-linux`. It
does not mean editing `anyflow-core`, `tls.rs`, `session.rs` or a capability
crate's protocol half — and the same gate is what keeps that true.

Four seams carry the boundary, each justified by a verified platform
difference rather than by symmetry: `IdentityProvider` (a TPM or Secure
Enclave key can sign and can never be exported), `SecretStore` (a DACL is not
`0o600`), `ControlTransport` (`UnixStream` is `#[cfg(unix)]`, and a Windows
named pipe's default DACL grants Everyone read), and `FileSink`
(`FOLDERID_Downloads` is not `$XDG_DOWNLOAD_DIR`). `ClipboardBackend` and
`BatterySource` already existed and were the template.

`unsafe_code` is declared per crate: `forbid` on the portable crates, `deny`
on the adapters and binaries — because `forbid` cannot be relaxed locally, and
an adapter that one day needs FFI must not be able to weaken the core to get
it. No crate uses `unsafe` today, and a test asserts it.

See [the Wave 0 sprint report](../sprints/wave-0-platform-abstraction.md).

## Where trust is decided

There is exactly one place per direction, on purpose.

| Question | Answered by |
| --- | --- |
| Is this the desktop I paired with? | `PinnedServerCertVerifier` / `PinnedTrustManager` |
| Does this client hold the key it claims? | `verify_tls13_signature` (both verifiers) |
| Is this peer trusted? | `SessionHost::lookup_peer` → the trust store |
| May this peer use this capability? | `granted_capabilities`, re-checked per message |
| Which *transfer* is this data stream for? | `files.v1` MAC over a single-use challenge (ADR-0013) |
| Should this stranger become trusted? | Valid pairing proof **and** a human |
| Should this file be written to my disk? | A human, per transfer |

Nothing else grants anything. Not the network, not mDNS, not a device id, not
a device name.

## Extending it

Adding a capability touches four things and none of them are the transport:

1. A `.proto` under `protocol/proto/anyflow/v1/capabilities/`.
2. `impl Capability` in a new crate under `desktop/capabilities/`.
3. `class … : Capability` under `android/app/src/main/java/.../capability/`.
4. Register it, and decide its grant policy — **not** in `auto_grant` unless
   it is read-only with no side effects.

Adding a protocol message means a new `oneof` variant and a new arm in each
session loop. An old peer that receives one closes the connection with a
protocol violation, which is why the version range in `HELLO` exists.
