# AnyFlow protocol — v1

## Invariants

These hold for every version of this protocol. A change to any of them is a
new major version and a new threat-model review.

1. **Identity is a public key.** Never an IP address, hostname, MAC address or
   device id.
2. **Discovery is not trust.** Being findable and being reachable grant
   nothing.
3. **No plaintext.** Every byte after TCP connect is inside TLS 1.3.
4. **Advertising is not authorization.** What a peer says it supports and what
   it is allowed to use are separate decisions, made by separate parties.
5. **Clocks are not trusted.** `timestamp_unix_ms` is informational only.
6. **Capability payloads are opaque to the transport.**

## Layers

```
┌──────────────────────────────────────────┐
│ capabilities        battery.v1, files.v1 │  own schemas, own versions
├──────────────────────────────────────────┤
│ session             HELLO, PAIR_*, PING  │  state machine, replay guard
├──────────────────────────────────────────┤
│ framing             u32 length + proto   │  bounded before allocation
├──────────────────────────────────────────┤
│ TLS 1.3             mutual auth, pinned  │  identity lives here
├──────────────────────────────────────────┤
│ TCP                 port 55432 (default) │
└──────────────────────────────────────────┘
```

A second kind of connection shares that same port and the same four lower
layers, and skips the top two:

```
┌──────────────────────────────────────────┐
│ raw bytes           exactly size_bytes   │  one transfer, then closed
├──────────────────────────────────────────┤
│ stream auth         MAC over a challenge │  which transfer, not just which peer
├──────────────────────────────────────────┤
│ TLS 1.3             mutual auth, pinned  │  the SAME pinned identities
├──────────────────────────────────────────┤
│ TCP                 the same port        │
└──────────────────────────────────────────┘
```

Which one a connection is, is decided by **ALPN** during the handshake, before
a single application byte:

| ALPN | Connection |
| --- | --- |
| `anyflow/1` | control session — HELLO, pairing, capability messages |
| `anyflow-data/1` | a `files.v1` data stream — one transfer's bytes |

A connection that negotiates neither is dropped. Treating an absent ALPN as
"probably a control session" would hand the handshake path to any client that
omitted it, so this fails closed. See
[ADR-0013](../adr/ADR-0013-file-transfer-data-stream.md).

## Framing

```
┌────────────┬──────────────────────────┐
│ length u32 │ Envelope (protobuf)      │
│ big-endian │ ≤ 65536 bytes            │
└────────────┴──────────────────────────┘
```

The length is validated before allocation. `length == 0` and
`length > MAX_FRAME_LEN` are both protocol violations that close the
connection.

## Envelope

| Field | Type | Notes |
| --- | --- | --- |
| `protocol_version` | `uint32` | Fixed after HELLO_ACK; a change closes the connection |
| `message_id` | `bytes` | Exactly 16 random bytes; de-duplicated |
| `sequence` | `uint64` | Per-connection, strictly increasing from 1 |
| `timestamp_unix_ms` | `int64` | **Informational only** |
| `correlation_id` | `bytes` | The `message_id` being replied to |
| `body` | `oneof` | Exactly one message |

## Handshake

```
Phone                                        Fedora
  │                                             │
  │──── TCP + TLS 1.3 (mutual, ALPN anyflow/1) ─│
  │     phone pins the desktop SPKI             │
  │     desktop proves possession of its key    │
  │     desktop learns the phone's SPKI         │
  │                                             │
  │──── HELLO ─────────────────────────────────▶│
  │     device info, version range, caps        │  fingerprint in HELLO must
  │                                             │  equal the TLS peer's
  │◀─── HELLO_ACK ──────────────────────────────│
  │     TRUSTED | PAIRING_REQUIRED | …          │
  │                                             │
  │  if TRUSTED ────────────────────────────────▶ established
  │                                             │
  │  if PAIRING_REQUIRED (+ 32-byte nonce)      │
  │──── PAIR_REQUEST (HMAC proof) ─────────────▶│
  │                                             │ verify, consume token,
  │                                             │ ask the human
  │◀─── PAIR_RESPONSE (confirmation MAC) ───────│
  │     phone verifies the confirmation         │
  │                                             │
  └──── established ────────────────────────────┘
```

### Version negotiation

`chosen = min(peer_max, local_max)`; valid if `chosen ≥ max(peer_min,
local_min)`, otherwise `HELLO_ACK{VERSION_UNSUPPORTED}` and close. The
negotiated version is fixed for the connection.

### What an unauthenticated peer may send

`HELLO`, and then `PAIR_REQUEST` — and the latter only while a pairing window
is open. **Anything else closes the connection**, including a `PING`.

## Pairing proof

```
proof        = HMAC-SHA256(token, "anyflow/pairing-proof/v1"
                                  ‖ len32(responder_fp) ‖ responder_fp
                                  ‖ len32(initiator_fp) ‖ initiator_fp
                                  ‖ len32(nonce)        ‖ nonce)

confirmation = HMAC-SHA256(token, "anyflow/pairing-confirm/v1" ‖ …)
```

* `token` — 20 raw bytes (the base32 in the QR, decoded)
* `responder_fp` / `initiator_fp` — 32 raw fingerprint bytes
* `nonce` — 32 bytes from `HELLO_ACK.pairing_nonce`
* `len32` — big-endian `uint32`

Length-prefixing every field prevents a concatenation ambiguity. The domain
separators prevent a proof being replayed as a confirmation. Both sides
compare in constant time.

This is a cross-language contract. `PairingProofTest` on Android and
`desktop/core/tests/pairing.rs` both pin it.

## Replay protection

| Layer | Stops |
| --- | --- |
| TLS 1.3 records | Off-path injection, reordering, replay |
| `sequence` strictly increasing | A hostile or broken peer rewinding |
| `message_id` de-duplication (1024-entry window) | A fresh sequence with a reused id |
| Single-use token + server nonce | Pairing replay across sessions and devices |

A violation is fatal. A correct peer never produces one.

## QR payload

```
anyflow1:<responder-fingerprint-hex>:<token-base32>:<device-id>:<addr>[,<addr>…]
```

Maximum 512 bytes. The fingerprint is the load-bearing field: it is pinned
before the socket opens, which is what removes the man-in-the-middle window.
Addresses are hints; an unparseable one is dropped rather than failing the
code.

## Capability messages

```protobuf
CapabilityMessage { string capability_id = 1; bytes payload = 2; }
```

The transport never parses `payload`. Effective capabilities are
`mutually supported ∩ granted by the local trust store`, re-checked per
message. An un-negotiated id gets a **non-fatal** `ERROR` and the session
continues.

### `files.v1`

Capability id `files.v1`, payload a `FileControl` message. Control messages
only: **no file byte ever enters an `Envelope`**, which is what lets
`MAX_FRAME_LEN` stay at 64 KiB (ADR-0012).

| Message | Sent by | Carries |
| --- | --- | --- |
| `FILE_OFFER` | sender | `transfer_id`, filename, `size_bytes`, `mime_type`, `sha256`, timestamp |
| `FILE_ACCEPT` | receiver | `transfer_id`, and the stream challenge when the receiver is the stream acceptor |
| `FILE_READY` | stream acceptor | `transfer_id` + challenge, when the acceptor is the *sender* |
| `FILE_REJECT` | receiver | `transfer_id`, reason |
| `FILE_CANCEL` | either | `transfer_id`, reason |
| `FILE_COMPLETE` | receiver | `transfer_id` — the receiver's verdict, after verification |
| `FILE_FAILED` | either | `transfer_id`, reason |

`transfer_id` is 16 cryptographically random bytes, single-use, and
deliberately **not** derived from `message_id`: a message id is meaningful for
one frame and is garbage-collected by the dedup window, while a transfer id
must stay meaningful across many frames and two connections.

`FileOffer` has **no path field**, relative or absolute, and no stream
challenge. The absent path means nothing a peer sends can name a location on
the receiver; the absent challenge means a data stream cannot be opened before
the receiving human has accepted.

The data stream's first two frames use the same 4-byte big-endian length
prefix, capped at 4 KiB and checked before allocation:

```
DataStreamAuth  { protocol_version, transfer_id, mac }   dialer → acceptor
DataStreamReady { status, reason }                       acceptor → dialer
<raw bytes>     exactly size_bytes                       in the negotiated direction
```

```text
mac = HMAC-SHA256(
    key = stream_challenge,
    msg = "anyflow/files.v1/data-stream/v1"
          || len_prefixed(acceptor_identity_fingerprint)
          || len_prefixed(dialer_identity_fingerprint)
          || len_prefixed(transfer_id))
```

Same construction as the pairing proof. The acceptor additionally requires that
the TLS peer certificate on the data connection is the same pinned identity
that negotiated the transfer; the MAC is defence in depth on that check, not a
replacement for it.

Full specification: [FILES.md](FILES.md).

### `battery.v1`

```protobuf
BatteryState {
  uint32 percentage        = 1;  // 0..=100, validated on receipt
  ChargingState charging_state = 2;
  int64 timestamp_unix_ms  = 3;  // display only
}
```

Held in memory only, dropped on disconnect. Never persisted.

## mDNS

`_anyflow._tcp.local.`, TXT: `v=1`, `pv=1-1`, `id=<hex>`, `dn=<name>`.
The fingerprint is **not** published. See ADR-0005.
