# AnyFlow — Threat Model

Scope: the foundation Sprint (identity, discovery, pairing, authenticated
transport, `battery.v1`). Clipboard, file transfer, notifications and browser
integration are **not** implemented and are out of scope; where a decision
here exists to make those safe later, it is called out.

## 1. What we are protecting

| Asset | Why it matters |
| --- | --- |
| Device identity private keys | Compromise means permanent impersonation of a trusted device |
| The trust store | Deciding who is trusted; tampering grants access |
| Pairing tokens | Momentary authority to become trusted |
| Battery telemetry | Low sensitivity, but still a signal about a person's habits |
| *(future)* clipboard, files, notifications | High sensitivity — passwords, 2FA codes, documents |

## 2. Attackers we design against

| # | Attacker | Capability |
| --- | --- | --- |
| A1 | Passive on the same Wi-Fi | Reads all frames, sees mDNS |
| A2 | Active on the same Wi-Fi | Spoofs mDNS/ARP, injects, blocks, MITM |
| A3 | Malicious app on the phone | Runs unprivileged in its own sandbox |
| A4 | Malicious local user on the desktop | Unprivileged, another account on the box |
| A5 | Someone with brief physical access | Can photograph a screen, briefly hold the phone |
| A6 | A previously paired device now hostile | Holds a valid identity and a granted capability |

Explicitly **out of scope**: a root/kernel compromise of either endpoint, a
malicious build of the software itself, and hardware attacks on the TEE. Once
the attacker is inside the trust boundary, this design does not claim to
help.

## 3. Threats and mitigations

### T1 — Attacker on the same Wi-Fi reads traffic

*A1.* Everything after TCP connect is inside TLS 1.3, negotiated with a
forward-secret ECDHE suite. Only TLS 1.3 is compiled in: the Rust side builds
its configs with `TLS13` only and both `verify_tls12_signature`
implementations return an error, and the Android side sets
`enabledProtocols = ["TLSv1.3"]` on every socket.

**Residual:** traffic volume and timing are visible. A watcher can tell that a
phone and a computer are talking, and roughly when. We do not pad.

### T2 — mDNS spoofing

*A2.* Anyone on the link can publish a record claiming any name, id or
address. Discovery is therefore treated as a routing hint and **never** as an
authorization input. A spoofed record leads to a TCP connection whose TLS
handshake fails the pinned-key check. `docs/architecture/PROTOCOL.md` states
this as an invariant; `discovery.rs` and `Discovery.kt` both carry the same
comment so it cannot be quietly forgotten.

**Residual:** an attacker can deny discovery by flooding records. The phone
also remembers the last working address, so a denial is not total.

### T3 — Man-in-the-middle during pairing

*A2, A5.* This is the moment everything hinges on, because it is the only
point where trust is created.

The QR code carries the desktop's SPKI fingerprint. The phone pins it
**before** opening a socket, so the very first TLS handshake is already
authenticated. There is no trust-on-first-use window for a MITM to occupy.
The transfer is out-of-band (camera → screen), so a network attacker cannot
alter it.

In the reverse direction, the desktop authenticates the phone by requiring an
HMAC proof of the pairing token bound to both fingerprints and a server nonce,
then requires a human to confirm the phone's fingerprint.

**Residual:** a user who scans a QR code displayed by an attacker's machine
will pair with that machine. Nothing in software fixes this; the confirmation
prompt showing the fingerprint is the mitigation, and it depends on the user
looking.

### T4 — Replay

*A1, A2.* Three layers:

1. TLS 1.3 record protection stops an off-path attacker replaying anything.
2. Within a connection, `sequence` must strictly increase and `message_id`
   must not have been seen in a 1024-entry window. A violation closes the
   connection rather than being skipped, because a well-behaved peer never
   produces one.
3. A pairing proof is bound to a single-use server nonce and to both
   identities, and the token is consumed on first successful use.

`timestamp_unix_ms` is deliberately **not** used for replay decisions. A phone
and a desktop routinely disagree by seconds, and a timestamp window would
either be trivially wide or would break for honest users.

Covered by `desktop/daemon/tests/wire.rs`
(`a_replayed_envelope_terminates_the_session`,
`a_duplicate_message_id_terminates_the_session`,
`a_non_increasing_sequence_number_terminates_the_session`).

### T5 — Compromise of a previously paired device

*A6.* A trusted device can use exactly the capabilities it has been granted,
and nothing more:

* capability grants are stored per peer and are independent of what the peer
  advertises — a device announcing `clipboard.v1` gets it only if the store
  says so;
* authorization is re-checked per message, not once at connect;
* `anyflow unpair <device>` sets `revoked`, clears the grants, **and tears
  down the live session immediately** rather than waiting for the next
  reconnect;
* this Sprint has no remote command execution, no filesystem access and no
  input injection, so the blast radius of a compromised peer is a battery
  percentage.

**Residual:** until the user revokes, a compromised peer can read whatever it
was granted. There is no automatic detection of a compromised peer.

### T6 — Brute forcing the pairing token

*A2.* The token is 160 bits from the OS CSPRNG. Beyond that: a 120-second
window, single use, and the whole session aborts after 3 failed proofs. An
online guessing attack gets three tries and then a visible failure.

**Residual:** none of practical concern. The realistic attack on pairing is
social (T3), not cryptographic.

### T7 — Compromise of a pairing token

*A5.* Someone photographs the QR from across the room. Mitigations: the window
is short, the token is single-use, and completing pairing still requires a
human to accept the *attacker's* fingerprint on the desktop. If the legitimate
phone pairs first, the token is already consumed
(`a_captured_proof_cannot_be_replayed_by_another_device`).

**Residual:** an attacker who is faster than the user and whose device the
user then accepts at the prompt will pair. The prompt is the last line.

### T8 — Malicious files, path traversal

*Not applicable in this Sprint*: there is no file transfer. Recorded here
because the decisions that make it tractable were taken now — capabilities are
separately granted, payloads are opaque to the transport and validated at the
capability boundary, and `MAX_FRAME_LEN` is 64 KiB. When `files.v1` arrives it
must: write only inside a dedicated directory, never trust a peer-supplied
path or filename, resolve and re-check the final path after canonicalisation,
and refuse symlinks and absolute paths.

### T9 — URL scheme attacks

*Not applicable in this Sprint*: `open-url.v1` does not exist. When it does, it
must allowlist schemes (`http`, `https` only to start), never hand a URL
straight to a system intent or `xdg-open`, and require explicit user
confirmation for anything else.

### T10 — Clipboard contents (passwords, tokens, 2FA codes)

*Not applicable in this Sprint*: no clipboard capability exists. The
groundwork: clipboard content will never be persisted, never logged at any
level, and will be a separately granted capability that is **off by default**
(`auto_grant` contains only `battery.v1`, which is read-only telemetry with no
side effects).

### T11 — Leakage through logs

*A3, A4.* Logs are the easiest way to undo every other control here.

* No message payload is ever logged. Capability handlers log an error's *type*
  and never its contents.
* `PairingToken`'s `Debug` prints `PairingToken(<redacted>)`;
  `LocalIdentity`'s prints `<redacted>` for the key; `QrPayload.toString()`
  redacts the token. Each has a test.
* The scanned QR text is never echoed to the screen on a parse failure.
* Fingerprints and peer addresses *are* logged: both are public values and are
  needed to diagnose a connection.
* The daemon logs to stderr, which journald captures under the user's own
  session.

### T12 — Protocol downgrade

*A2.* Version negotiation happens inside TLS, between two pinned identities,
so an off-path attacker cannot influence it. The negotiated version is fixed
for the connection and any envelope carrying a different one closes the
session (`changing_the_protocol_version_mid_connection_terminates_the_session`).
`last_protocol_version` is recorded per peer so a future release can refuse a
version below a previously observed floor.

**Residual:** with only v1 in existence there is nothing to downgrade *to*.
The per-peer floor is stored but not yet enforced — an honest limitation, not
a claim.

### T13 — Lost or stolen device

*A5.* The phone's private key is in the Keystore, hardware-backed and
non-exportable, so it cannot be extracted from a stolen device to impersonate
it elsewhere. The identity is excluded from cloud backup and device transfer,
so a restored backup cannot carry it either. Recovery is: revoke from the
desktop (`anyflow unpair`), which is effective immediately for live sessions
and permanently for future ones.

**Residual:** an unlocked stolen phone can use its granted capabilities until
revoked. We deliberately do not require biometric unlock per connection: the
connection must re-establish from a pocket, and a design that fights that
would just be turned off.

### T14 — Revocation

*A6.* Covered under T5. Revoked records are kept rather than deleted, so a
revoked device reconnecting is attributable instead of appearing as a
stranger, and cannot silently re-pair without the user noticing.

### T15 — Local privilege boundaries on the desktop

*A4.* The private key is written 0600 inside a 0700 directory, and the daemon
**refuses to start** if the key is group- or world-readable rather than
warning and continuing (`store_refuses_to_load_a_world_readable_private_key`).
The control socket lives in `$XDG_RUNTIME_DIR`, which is 0700 and per-user,
and is chmod 0600 as well. The daemon runs as the user, never as root.

**Residual:** the key is protected by filesystem permissions, not by hardware.
A TPM2-sealed key is the follow-up; see ADR-0006.

### T16 — Resource exhaustion

*A2.* An unauthenticated peer on the LAN can open sockets. Bounds: 32
concurrent connections, a 10-second TLS handshake timeout, a 15-second
protocol handshake timeout, a 64 KiB frame limit checked *before* allocation,
and a 64-byte cap on ping payloads.

### T17 — Malicious peer-supplied strings

*A6.* Device names arrive from the network and are rendered in a terminal and
in notifications. Both sides strip control characters and cap the length
(`sanitize_device_name`, `TrustStore.sanitizeDeviceName`) so a name cannot
forge UI or corrupt a terminal.

### T18 — Privacy of the mDNS advertisement

*A1.* The daemon publishes a stable device id and a device name on every
network it joins. A passive observer on a café network can tell that the same
machine came back. This is an accepted cost: the alternative — dialling every
discovered service and completing a TLS handshake to find out who it is — is
worse for battery and noisier.

The identity **fingerprint is deliberately not published**, so an observer
cannot enumerate who trusts whom. `--no-mdns` disables advertising entirely
today; a per-network toggle is the proper fix and is on the roadmap.

## 4. Assumptions

1. The OS CSPRNG is sound on both platforms.
2. TLS 1.3 as implemented by *ring* and by Conscrypt/BoringSSL is sound.
3. The Android Keystore keeps a non-exportable key non-exportable.
4. The user's desktop account is not already compromised.
5. The user looks at the fingerprint on the confirmation prompt. This one is
   the weakest link in the whole model and we know it.

## 5. Verification

Security-relevant behaviour is covered by tests that fail loudly rather than
by prose. See `desktop/core/tests/pairing.rs`,
`desktop/daemon/tests/wire.rs` and `desktop/daemon/tests/e2e.rs`. Notably:
an unpaired device gets nothing, a different certificate is rejected by the
verifier, a revoked device is refused, a token pairs exactly one device, and a
replay closes the session.

**Never**, in tests or in a debug build, disable certificate validation. There
is no code path in this repository that does, and adding one would invalidate
this entire document.
