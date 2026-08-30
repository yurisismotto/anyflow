# ADR-0012 — Bulk transfer, and why `MAX_FRAME_LEN` stays at 64 KiB

Status: Accepted

## Context

`MAX_FRAME_LEN` is 64 KiB. Every message in the protocol today — a HELLO, a
pairing proof, a ping, a battery reading — is at most a few hundred bytes, so
the limit is not currently felt.

It will be felt the moment someone implements file transfer. The obvious move
at that point is to raise the limit, and the obvious move is wrong. This ADR
records the decision *before* the pressure exists, so the answer is not
improvised by whoever picks up the file-sharing task.

## Decision

**Keep `MAX_FRAME_LEN` at 64 KiB. Bulk payloads get a separate data stream and
never travel inside an `Envelope`.**

### Why not a bigger frame

The frame limit is the cheapest denial-of-service control in the protocol. The
length prefix is checked *before* a buffer is allocated, so a peer claiming a
4 GiB frame costs a rejection and a closed socket rather than 4 GiB of
resident memory. That property is worth exactly as much as the limit is small.
Raising it to accommodate files would weaken the one control that protects
both a phone and a desktop from an unpaired peer, in order to serve a feature
that does not need it.

### Why not chunk inside capability messages

Chunking a file into 64 KiB `CapabilityMessage`s inside the existing envelope
stream keeps the limit intact but is still the wrong shape:

* Every chunk would pass through the replay guard, the sequence counter and
  the capability dispatch table. That machinery exists for a low-rate stream
  of small control messages; running a multi-gigabyte transfer through it
  makes the per-message cost the transfer's cost.
* The dedup window is a bounded set of recent message ids. A long transfer
  would churn it continuously, degrading the protection it exists to provide.
* There is one envelope stream per session, so a large transfer would
  head-of-line block ping, battery updates and — worse — a revocation taking
  effect. A file transfer must never be able to delay an unpair.
* Protobuf decoding wants the whole message in memory. Chunking avoids the
  worst of that, but the envelope path is still built around
  parse-then-dispatch, not streaming.

### The shape bulk transfer will take

A capability negotiates a transfer over the control stream — metadata, size,
a content hash, and a single-use transfer id — and the bytes then move over a
**separate stream**, opened for that transfer and closed with it.

Binding requirements, recorded now so they are not rediscovered later:

* The data stream is authenticated by the **same pinned identity** as the
  control session. It is not a new trust decision and must not become one.
* The transfer id is single-use and bound to the negotiating session, so a
  data stream cannot be attached to a different peer's transfer.
* The receiver knows the expected size and hash *before* the first byte
  arrives, and enforces both. An oversized or mismatched transfer is aborted,
  not truncated and accepted.
* Revocation cancels in-flight transfers for that peer immediately.
* The control session stays responsive throughout; that is the whole point.

Whether the data stream is a second TLS connection or a multiplexed stream
over the existing one is left open — it is an implementation trade-off between
connection setup cost and multiplexing complexity, and both satisfy the
requirements above. That decision belongs to the file-sharing ADR.

## Consequences

* `MAX_FRAME_LEN` can stay small and keep doing its job.
* File sharing is a larger piece of work than "add a capability", because it
  needs transport work as well. This is the honest cost, surfaced early.
* Any future proposal to raise `MAX_FRAME_LEN` should be read as a signal that
  something is being pushed through the wrong channel.
* A capability that genuinely needs a one-off payload larger than 64 KiB but
  far smaller than a file — a thumbnail, say — should be chunked by the
  capability itself, not granted a bigger frame.
