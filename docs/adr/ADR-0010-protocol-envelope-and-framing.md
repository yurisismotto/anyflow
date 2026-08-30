# ADR-0010 — Protocol envelope and framing

**Status:** Accepted · 2026-08-29

## Context

The brief asks for an envelope with a protocol version, message id, timestamp,
message type, payload and a correlation id, using robust identifiers and
without depending on synchronized clocks.

Protobuf is not self-delimiting, so a framing layer is needed regardless.

## Decision

### Framing

4-byte big-endian length, then the encoded `Envelope`. `MAX_FRAME_LEN` is
64 KiB and **the length is checked before any buffer is allocated**.

### Envelope

```protobuf
message Envelope {
  uint32 protocol_version = 1;
  bytes  message_id       = 2;  // 16 random bytes
  uint64 sequence         = 3;  // per-connection, strictly increasing
  int64  timestamp_unix_ms= 4;  // informational ONLY
  bytes  correlation_id   = 5;
  oneof body { Hello ... CapabilityMessage ...; }
}
```

Two departures from the brief, both deliberate:

**A `oneof` instead of `messageType` + `payload`.** A `oneof` *is* a
type-and-payload pair, but it makes it structurally impossible for the tag to
disagree with the bytes, which removes a whole class of confused-parsing bugs.
Capability traffic still uses an opaque payload, inside `CapabilityMessage`,
because capabilities own their own schemas (ADR-0008).

**A `sequence` field, added.** The brief asked to avoid depending on
synchronized clocks. `timestamp_unix_ms` is therefore informational only and
is never an input to a security decision; `sequence` plus `message_id`
de-duplication does the work a timestamp window would otherwise be asked to
do.

### Identifiers

`message_id` is 16 random bytes from the OS CSPRNG — deliberately **not**
UUIDv7, which embeds a timestamp and would reintroduce both a clock dependency
and a small leak of device clock state.

## Alternatives

**Varint length prefix.** Saves three bytes. Rejected: fixed-width is simpler
to bound, and the parser must read a fixed header before it can trust
anything.

**Newline-delimited or self-describing framing.** Rejected: binary payloads
need escaping, which is an entire class of bug for no benefit.

**A larger frame limit.** Rejected for now: nothing in v1 exceeds a few
hundred bytes. When `files.v1` arrives it will chunk, and the limit can be
raised deliberately with that change rather than pre-emptively.

**UUIDv7 for message ids.** Rejected as above.

**Sequence numbers only, without id de-duplication.** Rejected: they catch
different things. A peer can send a fresh sequence number with a reused id,
which de-duplication catches and the sequence check does not
(`a_duplicate_message_id_terminates_the_session`).

## Consequences

* Framing is trivial to implement identically in both languages, and is.
* The 64 KiB limit will need raising for file transfer. That is a one-line,
  deliberate change with a matching change on the other side.
* Every message costs 4 bytes of framing plus roughly 40 bytes of headers.
  Irrelevant at this frequency.

## Security implications

* The length check happens **before** allocation, so a peer claiming a 4 GiB
  frame gets an error rather than an allocation
  (`oversized_length_prefix_is_refused_without_allocating`).
* On the Android side the signed `readInt()` is widened to a long before
  comparison, so a length with the high bit set reads as a huge positive
  number and is rejected rather than as a negative one that slips past a
  `> MAX` check.
* A truncated frame is reported as a clean close, not as garbage
  (`truncated_frame_reports_closed_not_garbage`).
* `sequence` and `message_id` give replay protection that does not depend on
  clocks. A violation closes the connection rather than being skipped: a
  well-behaved peer never produces one, so it means a bug or an attack.
* The `oneof` means an unexpected message type on an established session is
  detected structurally and closes the connection.
