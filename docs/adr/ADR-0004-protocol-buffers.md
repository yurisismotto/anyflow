# ADR-0004 — Protocol Buffers, compiled with protox

**Status:** Accepted · 2026-08-29

## Context

Two implementations in two languages must agree on a wire format that will
grow for years. Requirements: an explicit schema, forward and backward
compatibility as fields are added, small messages, and mature codegen for both
Kotlin/JVM and Rust.

## Decision

Protocol Buffers 3, with `.proto` files in `protocol/proto/` compiled by both
sides (ADR-0001). Android uses `protobuf-javalite`; Rust uses `prost`.

**The Rust build compiles the schema with `protox`, a pure-Rust protobuf
compiler, rather than invoking a system `protoc`.**

## Alternatives

**JSON.** Human-readable and needs no codegen. Rejected: no schema, so both
sides re-derive the format from prose, and a typo becomes a runtime bug. Also
larger on the wire and slower to parse, which matters on a phone radio.

**CBOR / MessagePack.** Compact and schema-optional. Rejected for the same
reason as JSON: schema-optional means schema-in-your-head.

**Cap'n Proto or FlatBuffers.** Zero-copy parsing. Rejected: the performance
advantage is irrelevant for messages measured in tens of bytes, and the
tooling maturity on Android is much thinner.

**protobuf-java (full) on Android instead of javalite.** Rejected: the full
runtime brings reflection and a much larger method count for features
(descriptors, text format) this app never uses.

**System `protoc` via `prost-build`'s default path.** The conventional
approach. Rejected: it makes `dnf install protobuf-compiler` a prerequisite
for building a security daemon from a clean checkout, and version skew between
a developer's `protoc` and CI's is a real source of confusing diffs.

**`protoc-bin-vendored`.** Removes the system dependency by shipping a
prebuilt binary inside a crate. Rejected: downloading and executing a
third-party binary during the build of a security-sensitive project is a
supply-chain wart we would rather not have.

`protox` compiles `.proto` to a descriptor set in pure Rust, which
`prost-build` then consumes. Verified: the workspace builds with no `protoc`
installed anywhere.

## Consequences

* `cargo build` works on a clean machine with only a Rust toolchain.
* Field numbers become permanent. Adding is safe; renumbering or reusing a
  number is not, and reviewers must watch for it.
* An older peer receiving an unknown field ignores it, which is what makes
  additive protocol evolution possible.
* `protox` is a smaller project than `protoc`. If it stops being maintained,
  the fallback is checking in generated code plus a regeneration script — a
  contained change to one `build.rs`.

## Security implications

* Positive: an explicit schema means every field has a declared type, so
  malformed input fails in the parser rather than deep in application code.
  `malformed_protobuf_body_is_refused` covers this.
* Positive: no `protoc` binary is downloaded or executed at build time.
* Caveat: protobuf is not self-delimiting and does not bound message size on
  its own. Framing does that; see ADR-0010.
* Caveat: proto3 cannot distinguish "absent" from "default" for scalars. The
  protocol never relies on that distinction for a security decision —
  `pairing_nonce` is checked for exact length, not for presence.
