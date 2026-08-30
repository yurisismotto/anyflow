# ADR-0003 — Rust desktop daemon

**Status:** Accepted · 2026-08-29

## Context

The desktop component is a long-running network service that parses
attacker-controlled bytes from the local network, holds a private key, and
must run unattended for weeks. It has to work headless, start under
`systemd --user`, and never need root.

## Decision

Rust, `tokio` for async, running as a `systemd --user` service. The workspace
is split so the protocol is testable without any of the daemon:

| Crate | Role |
| --- | --- |
| `fedroid-proto` | Generated protobuf types only |
| `fedroid-core` | Identity, pairing, TLS, framing, session, capability registry. No I/O policy, no globals. |
| `fedroid-daemon` | mDNS, listener, control socket, `SessionHost` implementation |
| `fedroid-cli` | `fedroid` — talks to the daemon, holds no keys and no protocol logic |
| `fedroid-capability-battery` | `battery.v1` |

`unsafe_code = "forbid"` at the workspace level.

The CLI reaches the daemon over a Unix socket in `$XDG_RUNTIME_DIR` carrying
newline-delimited JSON.

## Alternatives

**C or C++.** Rejected outright. A memory-unsafe parser exposed to the local
network is the exact shape of bug this project cannot afford.

**Go.** Memory-safe, excellent networking, easy static binaries. Reasonable
choice. Rejected on the margin: Rust's enums and exhaustive matching model a
protocol state machine more precisely, and `rustls` gives finer control over
the certificate verification path than Go's `tls` package — which matters a
lot when the whole trust model is a custom verifier (ADR-0007). The GC pause
is irrelevant here; the type system was the deciding factor.

**Python.** Fastest to write. Rejected: packaging a Python daemon with native
TLS dependencies on Fedora is worse than shipping one binary, and the runtime
cost is real for something always resident.

**D-Bus instead of a Unix socket for the control interface.** More idiomatic
for a Linux desktop service and the likely path for the GUI. Rejected as the
*starting* point: it would make the daemon impossible to test without a
session bus, and the CLI is the only client today. The control protocol is
deliberately thin so a D-Bus front end can be added beside it.

**Secret Service (gnome-keyring) for key storage.** Rejected — see ADR-0006.

## Consequences

* One dependency-light binary; the release build needs only a Rust toolchain.
* `fedroid-core` has no global state and takes its host application as a
  trait, so the entire protocol runs in-process in tests over real TLS.
* Contributors need Rust. Compile times are moderate.
* A D-Bus interface will have to be added later for the GTK4 GUI, alongside
  rather than instead of the socket.

## Security implications

* Positive: no memory-safety class of bug in the network parser, and `unsafe`
  is forbidden rather than discouraged.
* Positive: the daemon needs no privileges. It binds a port above 1024, writes
  only under `$XDG_DATA_HOME`, and refuses to start if its key file is
  group- or world-readable.
* Positive: the CLI holds no keys, so a bug there cannot leak an identity.
* Negative: the control socket is an attack surface for a malicious local
  process running *as the same user*. That process could already read the key
  file, so it does not widen the boundary — but note that peers on the network
  cannot reach the socket at all, and no protocol message maps to a control
  command.
