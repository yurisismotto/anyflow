# OmniBridge — Threat Model

Scope: the foundation Sprint (identity, discovery, pairing, authenticated
transport, `battery.v1`). Clipboard, file transfer, notifications and browser
integration are **not** implemented and are out of scope; where a decision
here exists to make those safe later, it is called out.

> **`notifications.v1` — approved, not implemented (2026-09-08).**
> **The current release contains no `NotificationListenerService` and no
> notification code of any kind.** A *design* for one has been approved in
> [ADR-0015](../adr/ADR-0015-notification-access.md), which changes the
> permanent position stated in **T26** below. T26 is amended rather than
> replaced: its clipboard reasoning is unaffected and still correct. See **T27**
> for the notification-access position, and
> [docs/research/notifications-v1/03](../research/notifications-v1/03-PRIVACY-SECURITY-THREAT-MODEL.md)
> for the feature's own threat model (T-N01 … T-N16), which becomes part of this
> document when the feature is implemented and not before.

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
* `omnibridge unpair <device>` sets `revoked`, clears the grants, **and tears
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

**Applicable since `files.v1`.** The commitments recorded here before the
feature existed are all met; see [FILES.md](../architecture/FILES.md).

* **Write only inside a dedicated directory.** `<XDG downloads>/OmniBridge` on
  Fedora (mode 0700), `Download/OmniBridge` via MediaStore on Android. The
  directory is chosen by the receiver and cannot be influenced by a peer.
* **Never trust a peer-supplied filename.** `sanitize` reduces it to a bare
  name: everything up to the last `/` **or** `\` is dropped, control
  characters and NUL are stripped, trailing dots and spaces go, `.` and `..`
  and Windows device names are rejected outright, and the result is capped at
  255 bytes on a character boundary. A name that sanitizes to nothing is
  rejected, never renamed — inventing a name would hide the attack.
* **No path is transmitted at all.** `FileOffer` has no path field, relative
  or absolute, so there is nothing a peer could set that names a location.
* **Re-check the final path.** The joined path's parent is asserted to be the
  destination directory before anything is created.
* **Refuse symlinks.** Every file is created with `O_EXCL`, so a symlink
  planted in the download directory causes the open to fail rather than be
  followed. A duplicate name is numbered — `photo (1).jpg` — by *creating* the
  reservation, not by testing-then-creating, which closes the race in which
  two transfers both see a name free.
* **Nothing unverified is ever presented as a file.** Bytes land in a hidden
  `.part` file (or an invisible `IS_PENDING` MediaStore row), are hashed, and
  are promoted only if SHA-256 matches the offer. A failed hash deletes the
  partial file.

Residual risk: OmniBridge does not inspect file *content*. A paired, granted,
human-approved peer can send a file that is malicious when opened. That is out
of scope for a transfer tool, and the mitigations are the grant (off by
default), the per-transfer human approval, and the fact that a received file
is never executed, opened or dispatched on by OmniBridge itself — `mime_type` is
a label and is never an input to a decision.

### T9 — URL scheme attacks

*Not applicable in this Sprint*: `open-url.v1` does not exist. When it does, it
must allowlist schemes (`http`, `https` only to start), never hand a URL
straight to a system intent or `xdg-open`, and require explicit user
confirmation for anything else.

### T10 — Clipboard contents (passwords, tokens, 2FA codes)

**Implemented by `clipboard.v1`.** The clipboard is the most sensitive surface
OmniBridge touches. It routinely holds passwords, API keys, one-time codes, card
numbers, recovery phrases and URLs with tokens in them — usually without the
person consciously deciding to put them there.

* `clipboard.v1` is **never auto-granted**. `auto_grant` still contains only
  `battery.v1`. Granting it is `omnibridge grant <device> clipboard.v1`, or a
  switch on the Android device card.
* The grant is one question; **direction and automation are another**. A
  freshly granted device gets `allow_send`/`allow_receive` on (that is what
  the person just asked for) and `auto_send`/`auto_receive` **off**. So a
  fresh grant sends nothing when you copy, and puts nothing on your clipboard
  when a peer pushes.
* With `auto_receive` off, an accepted clip is held **in memory** and offered
  through a notification or `omnibridge clipboard apply`. It never silently
  replaces what you are about to paste.
* Content is bounded at 32 KiB and **never truncated** — a truncated password
  is a different, plausible-looking, wrong value.
* Nothing is persisted: see T21. Nothing is logged: see T11.

**Residual risk.** A person who grants `clipboard.v1` *and* turns on
`auto_send` for a device has decided that everything they copy goes to that
device. That is a real exposure and the CLI help says so in those words. The
desktop has no reliable way to detect a sensitive clip (unlike Android's
`EXTRA_IS_SENSITIVE`), and we deliberately do **not** invent a heuristic
password detector — a guess dressed up as a security control is worse than an
honest warning.

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
* `files.v1` adds three rules of its own. `StreamChallenge`'s `Debug` prints
  `<redacted>` and it is zeroed on drop — it is a MAC key. `TransferId`'s
  `Display` prints only the first 8 hex characters, so reaching for the
  obvious formatter cannot put a full id in a log. And only the **sanitized**
  filename is ever logged: the raw peer-supplied string is attacker-controlled
  and could forge log lines, so it does not reach a log at any level, nor a
  screen.
* Failure reasons sent to a peer come from a fixed enum, so a local path or an
  errno string cannot escape through that channel.
* `clipboard.v1` adds the strictest rule in the project: **clipboard text
  never reaches a log, at any level, on either platform.** What is logged is
  the peer's short fingerprint, the event-id prefix, the content-hash prefix,
  the byte count and the outcome. `ClipboardText` has a *hand-written*
  `Debug`/`toString` — a derived one would put content into every `tracing`
  field using `?value`, every panic message and every test failure. This is
  proved rather than asserted: `capabilities/clipboard/tests/logging.rs` runs
  every flow (applied, sensitive, duplicate, oversized, invalid,
  held-then-applied, backend failure) under a subscriber capturing at
  **TRACE** with canary strings, and fails if a canary appears. `RUST_LOG=trace`
  is what someone runs when something is wrong, which is the worst moment to
  spill a password into a file they are about to attach to a bug report.
* Nothing built by `clipboard.v1` reaches a shell. Clipboard text goes to
  `wl-copy`'s **stdin**, never an `argv`, so it never appears in
  `/proc/<pid>/cmdline` where any process on the machine could read it.
* A clipboard notification on Android carries the computer's name, the byte
  count and the sensitive flag — **never the text**. A notification is shown
  on the lock screen and is readable by any notification listener the user has
  installed.
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
desktop (`omnibridge unpair`), which is effective immediately for live sessions
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

### T19 — A data stream opened by the wrong peer, or for the wrong transfer

**New with `files.v1`.** File bytes travel on a second TLS connection
(ADR-0013), which raises a question the control session never had: TLS proves
*which device* is on a socket, but not *which transfer* the socket is for.

* An **unknown** peer completes TLS (the listener cannot pin a stranger) and
  is then refused: the grant check runs before the transfer record is even
  looked at, so it learns nothing about which transfer ids exist.
* A **different paired** peer is refused because the transfer's TLS identity
  must equal the one that negotiated it.
* A **guessed transfer id** is worthless: the id is not a bearer token, and
  the dialer must prove a single-use 32-byte challenge with an HMAC bound to
  both fingerprints and the id.
* A **replayed** authentication frame finds the challenge already consumed.
* A **revoked** peer is refused, including mid-copy: the grant is re-checked
  when the stream authenticates and periodically while it runs.

Refusals are generic on the wire, so a prober cannot distinguish "no such
transfer" from "bad MAC".

Residual risk: an attacker who already controls one of the two devices can
read a challenge from the control session's plaintext. This is not a
weakening — on a compromised device they could use the legitimate code path —
and the threat model has never claimed to defend a device against itself.

### T20 — A transfer that never ends

**New with `files.v1`.** A data stream is a separate TCP connection, so it does
*not* die when its control session does. Without handling, a transfer whose
peer vanished would keep running, or sit in `TRANSFERRING` forever while
holding a partial file and a concurrency slot.

A per-transfer reaper ends any transfer whose control session has closed
(`FAILED`, reason `TRANSPORT`), whose state deadline has passed
(`TIMED_OUT`), or whose peer has lost its grant (`REVOKED`). Every path
deletes the partial file. Offers, acceptances, stream opens, idle streams and
the post-send verdict all have explicit bounds, and concurrency is capped per
peer.

### T21 — Clipboard exfiltration by a malicious trusted peer

**New with `clipboard.v1`.** *A3.* A device that was legitimately paired, and
has since been compromised or lent out, is inside TLS and inside the trust
store. What can it get?

* **Nothing, without a `clipboard.v1` grant.** The grant is checked on every
  inbound message, freshly, from the trust store — not from a set captured at
  handshake time. `CLIP-SEC-01`.
* **Nothing after revocation**, on the session it is already holding.
  Narrowing a grant takes effect at once, without a reconnect; widening one
  needs a new handshake. The dangerous half is the immediate one.
  `CLIP-SEC-02`, `CLIP-SEC-16`.
* **It cannot pull.** There is no "give me your clipboard" message in the
  schema. A peer can only *offer*; a clip leaves this device because a local
  watcher fired under a locally-set `auto_send`, or because a person tapped a
  button. `CLIP-SEC-11`.
* **It cannot widen its own permissions.** No protocol message writes a grant
  or a policy flag — the schema has no such message, so this is an absence
  rather than a check that could be inverted. `CLIP-SEC-13`.
* **It cannot impersonate another device.** Authorization keys on the pinned
  TLS identity, never on the `origin_device_id` a peer claims. `CLIP-SEC-03`.

**Residual risk.** A compromised device with `clipboard.v1` granted and
`auto_send` enabled *on the other side* receives whatever that side copies,
for as long as the pairing stands. Revocation is the answer and it is
immediate; there is no way to un-send what already went.

### T22 — Clipboard poisoning

**New with `clipboard.v1`.** *A3.* The inverse of T21: a trusted-but-hostile
peer writing to *your* clipboard. The classic attack is swapping a copied
cryptocurrency address, or a `curl … | sh` line, for one of the attacker's —
and it works precisely because nobody re-reads what they just copied.

* `auto_receive` is **off by default**, so an update is held rather than
  applied. Poisoning requires the person to have turned automatic application
  on for that specific device.
* With it off, applying is an explicit act, and the UI names the source device
  and the size.
* `allow_receive` off refuses the update outright; nothing is even held.
  `CLIP-SEC-12`.
* On Android, an applied clip carries `EXTRA_IS_REMOTE_DEVICE` (API 34+) so
  the system can present it as having come from elsewhere.

**Residual risk.** With `auto_receive` on — which is the setting that makes
desktop → phone sync feel automatic — a trusted peer *can* replace your
clipboard at any moment, including between your copy and your paste. This is
inherent to the feature, it is opt-in per device, and there is no mitigation
short of not enabling it.

### T23 — Clipboard sync loops and multi-peer storms

**New with `clipboard.v1`.** *A3, A4.* Two devices each applying the other's
clip and re-announcing it is an infinite loop that would saturate both radios
and both CPUs — a denial of service reachable by accident, not just by an
attacker.

Two independent mechanisms, both required:

* **`EventCache`** makes one event idempotent. Global rather than per-peer, so
  replaying one peer's `event_id` through another peer is also caught.
  `CLIP-SEC-05`.
* **`SuppressionCache`** makes a locally applied remote clip invisible to the
  local watcher. Armed **before** the write, single-use, and expiring in ten
  seconds so that a stale entry cannot swallow a legitimate re-copy.
  `CLIP-SEC-10`.

Both caches are bounded by count *and* by age, so a flooding peer cannot grow
them. `CLIP-SEC-17`. Convergence is proved by a suite that runs real managers
against each other with a hard budget on messages delivered — checked on every
delivery, because a loop amplifies and a per-sweep check would let one sweep
reach billions of messages before it was tested.

**No relay.** A clip from peer A is never forwarded to peer B. This is
enforced by absence: no code path takes an inbound update and sends it
onward, and the loop *through* the clipboard is what the suppression cache
stops. `CLIP-SEC-09`.

**Residual risk.** A peer that is itself buggy can still send us updates as
fast as its link allows. They are bounded by the session's outbound queue and
answered idempotently, and the liveness probe eventually ends a session whose
peer has stopped reading.

### T24 — Stale, duplicated or replayed clipboard updates

**New with `clipboard.v1`.** *A1, A3.* Beneath the transport's own replay
protection (`sequence`, `message_id`), an `event_id` gives the capability its
own idempotence, with a lifetime that suits it: the transport's dedup window
is scoped to one connection, while a clipboard event must stay recognisable
across a reconnect.

Receiving the same `event_id` twice applies nothing twice, sends nothing
twice, and answers `DUPLICATE`. `timestamp_unix_ms` is informational only and
is never an input to an authorization, expiry or ordering decision — clocks
between a phone and a desktop disagree routinely. Within a session, ordering
is the protocol's; across peers, v1 is explicitly **last-accepted-wins**.

### T25 — Oversized or malformed clipboard updates

**New with `clipboard.v1`.** *A3.* Bounded before anything is allocated or
copied: 32 KiB of UTF-8, an `event_id` of exactly 16 bytes, a `content_hash`
of exactly 32 bytes or absent. Protobuf refuses to decode a `string` field
that is not valid UTF-8, so invalid encodings never reach the capability at
all. NUL-bearing text is refused explicitly and identically on both platforms.

A refusal is **not fatal**: the session survives, because one capability
misbehaving must not cost the user everything else. `CLIP-SEC-06`,
`CLIP-SEC-07`, `CLIP-SEC-18`.

### T26 — Android background clipboard restrictions

**New with `clipboard.v1`.** Not an attack — a platform control we are on the
*receiving* end of, and the design consequence is large enough to belong here.

Android 10+ refuses `getPrimaryClip` to an app without input focus. Every
technique that defeats it (`AccessibilityService`, default IME, an invisible
focus-stealing activity, `READ_LOGS`, root, hidden APIs, reflection) is either
forbidden or user-hostile, and **OmniBridge uses none of them**. The manifest
declares no accessibility service, no `QUERY_ALL_PACKAGES`, no location and no
`SYSTEM_ALERT_WINDOW`.

> **Amended 2026-09-08.** This paragraph originally also read *"no notification
> listener"*, as a permanent statement. It is now qualified by **T27**: the
> approved `notifications.v1` design adds an optional, default-disabled
> `NotificationListenerService`. **Nothing else in this paragraph changes**, and
> the clipboard conclusion below is untouched — notably, a notification listener
> is still never used to work around the clipboard restriction, which is what
> this threat is about.

The consequence is stated rather than hidden: Android → Fedora is a manual
action, `ClipboardCapabilities.AUTO_SEND_SUPPORTED` is `false`, and the UI
explains why instead of offering a toggle that would silently do nothing.

`EXTRA_IS_SENSITIVE` and `EXTRA_IS_REMOTE_DEVICE` are treated strictly as
**presentation hints**. Neither is a cryptographic mechanism, neither is an
ACL, and neither is ever an authorization input — a sensitive clip from an
ungranted peer is still refused, and a sensitive clip from a granted one is
still delivered. What the sensitive hint earns is one more deliberate
confirmation, naming the destination, before a clip leaves the device.

### T27 — Android notification access

**Approved for `notifications.v1`; not implemented.** *Not an attack — a
responsibility, and a change to a published position.*

| | |
| --- | --- |
| **Current release** | Contains **no** `NotificationListenerService`, no `BIND_NOTIFICATION_LISTENER_SERVICE` in the manifest, and no notification code. Nothing described below exists yet |
| **Approved design** | Will declare **one** `<service>`, optional and disabled by default, when `notifications.v1` is implemented |
| **Canonical record** | [ADR-0015](../adr/ADR-0015-notification-access.md) |

**What changes.** `BIND_NOTIFICATION_LISTENER_SERVICE` moves from the manifest's
"deliberately absent" list to the list of permissions OmniBridge holds *and
justifies*, beside `CHANGE_WIFI_MULTICAST_STATE` and
`FOREGROUND_SERVICE_CONNECTED_DEVICE`. As with `BIND_QUICK_SETTINGS_TILE` on the
existing clipboard tile, the permission is held **by the system, not by
OmniBridge**: declaring it is what stops any *other* app from binding our service.

**What does not change.** No accessibility service, no default-IME request, no
`QUERY_ALL_PACKAGES`, no `SYSTEM_ALERT_WINDOW`, no `READ_LOGS`, no
`MANAGE_EXTERNAL_STORAGE`, no location, no root, no hidden APIs, no reflection.
`README.md` principle 8 — *"No root, no accessibility service, no ADB, no hidden
permissions"* — remains true in full.

**The rule that replaced "and it must stay that way".** OmniBridge acquires a
privileged Android capability only when a named, user-visible feature requires
it; only through the platform-sanctioned API for that feature; only with the
user's explicit, separately revocable consent; and **never as a means of
defeating a platform restriction that exists to protect the user**. The last
clause is why T26's answer for the clipboard was to ship a manual send and say
why, and why notification access is a different case: the platform *offers* a
first-class API for it, whose own javadoc names *"bridging to paired devices"*
as the use case.

**Mitigations, all normative
([ADR-0015 §1](../adr/ADR-0015-notification-access.md)).** Notification access
is optional and off by default; requires the Android OS grant **and**,
separately, an explicit per-peer `notifications.v1` grant, neither implying the
other; is independently revocable from either side; fails closed; and is not
required by `battery.v1`, `files.v1` or `clipboard.v1`. The listener is not even
bound unless a granted peer is connected, so an installed-but-unused OmniBridge
reads nothing. It carries **no** history, no cloud sync, no telemetry, no
arbitrary actions, no `PendingIntent`, no reply, no persistence of content and
no logging of content — at any level, including `TRACE`, which is the rule T11
already enforces for the clipboard.

**OmniBridge does not detect sensitive content**, and this is deliberate: no OTP
regex, no keyword list, no banking or 2FA app heuristic. T10's reasoning applies
unchanged — a guess dressed as a security control is worse than an honest
boundary. The control is the deny-by-default per-app allow-list.

**Residual risk. High, and it is the point of the feature.** A person who
enables this is trusting OmniBridge with the most sensitive stream on their phone,
and the platform will not soften that: `POC-NOTIF-01` measured Android 16 /
One UI 8.0 delivering OTP-shaped notifications to an untrusted listener
**entirely unredacted**, so platform OTP redaction is a bonus and never a
control. The trust is repaid by the code being open, by nothing leaving the LAN,
by the listener being unbound whenever no granted peer is connected, and by
every default starting closed.

**Not yet verified:** Google Play policy for notification access. OmniBridge is
distributed from GitHub, so this blocks no release; it must be answered before
any Play submission.

## 4. Assumptions

1. The OS CSPRNG is sound on both platforms.
2. TLS 1.3 as implemented by *ring* and by Conscrypt/BoringSSL is sound.
3. The Android Keystore keeps a non-exportable key non-exportable.
4. The user's desktop account is not already compromised.
5. The user looks at the fingerprint on the confirmation prompt. This one is
   the weakest link in the whole model and we know it.
6. The system clipboard itself is not already being read by another local
   process. On Android the platform enforces this; on a Linux desktop any
   process in the same session can read the clipboard, and `clipboard.v1`
   neither worsens nor can fix that.

## 5. Verification

Security-relevant behaviour is covered by tests that fail loudly rather than
by prose. See `desktop/core/tests/pairing.rs`,
`desktop/daemon/tests/wire.rs` and `desktop/daemon/tests/e2e.rs`. Notably:
an unpaired device gets nothing, a different certificate is rejected by the
verifier, a revoked device is refused, a token pairs exactly one device, and a
replay closes the session.

`files.v1` adds `desktop/daemon/tests/files.rs`, which drives the real
capability over a real TLS data stream with real pinning. Every refusal in T8,
T19 and T20 is asserted there rather than described: an ungranted peer, a
stranger opening a data stream, another paired device attaching to someone
else's transfer, a guessed transfer id, a replayed authentication, a reused
transfer id, a path-traversal filename, an absolute path, a hash mismatch, a
truncated stream, an oversized stream, oversized metadata, a duplicate
`FILE_COMPLETE`, cancellation, a mid-transfer disconnect, a mid-transfer
revocation, and two concurrent transfers staying isolated. The filename rules
and the data-stream MAC are additionally pinned as cross-language contracts:
`android/app/src/test/.../FilenamesTest.kt` and `StreamAuthTest.kt` assert the
same cases and the same MAC vector as the Rust suite, so the two
implementations cannot quietly diverge.

`clipboard.v1` adds four suites. `capabilities/clipboard/tests/security.rs`
asserts CLIP-SEC-01 … 18 against the manager: an ungranted peer, a revoked
peer, a spoofed `origin_device_id`, a replayed and a cross-peer-replayed
`event_id`, an oversized clip, invalid UTF-8, a NUL, a mismatched hash, a
sensitive clip, no relay, no echo, both direction policies, a peer trying to
widen policy, bounded caches, and malformed protobuf.
`capabilities/clipboard/tests/loops.rs` proves convergence with two- and
three-device meshes under a hard delivery budget.
`capabilities/clipboard/tests/logging.rs` proves T11's clipboard rule.
`daemon/tests/clipboard.rs` runs the whole capability over real TLS with real
pinning and the real trust store, including the session-writer regression: a
burst four times the outbound queue depth must be answered in full, both
directions must interleave without wedging, and a saturated session must still
shut down. The Android JVM suite mirrors the text, policy, cache and sync
rules; `ClipboardInstrumentedTest` asserts the **real** `ClipboardManager`,
`EXTRA_IS_SENSITIVE` and `EXTRA_IS_REMOTE_DEVICE` on a device, because a mock
there would prove only that we agree with ourselves.

**Never**, in tests or in a debug build, disable certificate validation. There
is no code path in this repository that does, and adding one would invalidate
this entire document.
