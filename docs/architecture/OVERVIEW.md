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
| `anyflow-daemon` | core, battery, files, clipboard | mDNS, listener, control socket, `SessionHost` |
| `anyflow-cli` | daemon (types only) | `anyflow` |

`anyflow-core` has no global state and no I/O policy. Everything it needs from
the host arrives through the `SessionHost` trait, which is why the whole
protocol can be tested in-process over real TLS with no daemon, no filesystem
and no human.

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
