# ADR-0001 — Monorepo structure

**Status:** Accepted · 2026-08-29

## Context

AnyFlow is two independent implementations — Kotlin on Android, Rust on
Fedora — that must agree exactly on a wire protocol, a pairing MAC
construction and a fingerprint definition. Any drift between them shows up as
"pairing mysteriously fails", which is among the hardest classes of bug to
diagnose because both sides look correct in isolation.

Future components (a GTK4 GUI, a browser extension, more platforms) multiply
the problem.

## Decision

One repository, with the protocol definitions as a shared top-level directory
that both builds compile directly:

```
anyflow/
├── protocol/proto/          # single source of truth, compiled by both sides
├── desktop/                 # Rust workspace
│   ├── proto/  core/  control/  runtime/  platform-linux/
│   ├── daemon/  cli/  gui/  capabilities/
├── android/                 # Gradle project
├── browser-extension/       # placeholder
├── packaging/fedora/
└── docs/{architecture,security,adr}/
```

The Rust side is a Cargo workspace rooted at `desktop/`, with crate
directories named for their role (`daemon/`, `cli/`) rather than prefixed
(`anyflow-daemon/`), so the tree matches the intended layout while staying
idiomatic Cargo.

Wave 0 added `control/`, `runtime/` and `platform-linux/` **alongside** what
was already there, rather than reshuffling everything under a `crates/`
directory. The alternative was considered and rejected as churn with no
functional gain: `desktop/` already contains only Rust, and every path in every
existing document points at the current layout. See
[the Wave 0 sprint report](../sprints/wave-0-platform-abstraction.md).

`android/app/build.gradle.kts` points its proto source set at
`../../protocol/proto`, and `desktop/proto/build.rs` compiles the same files.
Neither side owns a copy.

## Alternatives

**Separate repositories per component.** Normal for two languages, and it
would let each release independently. Rejected: the protocol would have to be
versioned and published as an artifact before either side could use it, which
adds a release cycle to every protocol change during the phase when protocol
changes are most frequent. Drift becomes possible the moment there are two
copies.

**Monorepo with a duplicated `.proto` per module.** Simpler build wiring.
Rejected for the same reason: two copies drift.

**Git submodule for the protocol.** Gets a single source of truth without a
monorepo. Rejected: submodules make a cross-cutting change a multi-commit,
multi-repo dance, and contributors get the version skew wrong constantly.

## Consequences

* A protocol change is one commit that touches both implementations, and CI
  can prove both still build.
* Contributors need both toolchains to build everything, though each module
  builds alone.
* Release tagging must handle two artifacts with different cadences (an APK
  and an RPM). Not yet solved; noted as a debt.

## Security implications

* Positive: the wire format cannot silently diverge between implementations.
  A mismatched pairing MAC would be a security-relevant bug, and this
  structure makes it a compile-time concern rather than a runtime surprise.
* Positive: security documentation lives beside the code it describes, so a
  change to `pairing.rs` and a change to the threat model are reviewable in
  one diff.
* Negative: a single repository is a single supply-chain target. Mitigation is
  process — signed commits and review — not structure.
