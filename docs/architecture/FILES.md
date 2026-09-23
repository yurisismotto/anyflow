# `files.v1` — file transfer

Secure file sharing between a phone and a desktop. This document is the
specification; [ADR-0013](../adr/ADR-0013-file-transfer-data-stream.md)
records *why* the transport looks like this, and
[ADR-0012](../adr/ADR-0012-bulk-transfer-and-frame-limit.md) records why file
bytes are not allowed anywhere near an `Envelope`.

## The one thing to understand first

There are **two channels**, and almost everything else follows from keeping
them apart.

```
  control session                             data stream
  ALPN "omnibridge/1"                            ALPN "omnibridge-data/1"
  ────────────────────────────────────        ──────────────────────────────
  length-prefixed Envelopes, ≤ 64 KiB         DataStreamAuth   (≤ 4 KiB)
  replay guard, sequence numbers              DataStreamReady  (≤ 4 KiB)
  capability dispatch                         <raw bytes>  exactly size_bytes
  one per session                             one per transfer, then closed

  FILE_OFFER    metadata, size, sha256
  FILE_ACCEPT   + challenge (when receiving)
  FILE_READY    + challenge (when sending)
  FILE_REJECT / FILE_CANCEL / FILE_FAILED
  FILE_COMPLETE the receiver's verdict
```

The control session carries **decisions**; the data stream carries **bytes**.
No file byte ever enters an `Envelope`, so `MAX_FRAME_LEN` stays at 64 KiB and
keeps being the cheap denial-of-service control it was designed to be.

The practical payoff: because a copy runs on its own connection, the control
session stays responsive throughout. A cancel, a `PING`, or — most
importantly — an unpair all take effect *during* a multi-gigabyte transfer
rather than after it.

## Roles: who dials, who accepts

**The phone always dials. The desktop always accepts.** In both directions of
transfer.

An Android app is not a stable listener, and making it one would mean a second
foreground service, a second port and an inbound attack surface on the device
holding the user's photos. So the direction of the *bytes* is independent of
who opens the *socket*:

| Transfer | Offers | Approves | Dials the stream | Writes bytes |
| --- | --- | --- | --- | --- |
| Android → desktop | phone | desktop | **phone** | phone |
| Desktop → Android | desktop | phone | **phone** | desktop |

The peer that accepts data streams (always the desktop) is the one that issues
the stream challenge.

## Flow

### Android → desktop

```
phone                                          desktop
  │ FILE_OFFER {id, name, size, sha256} ─────────►│
  │                                               │ grant? metadata ok? slot free?
  │                                               │ ask the human
  │◄──────── FILE_ACCEPT {id, challenge} ─────────│ state: TRANSFERRING
  │                                               │ (temp file opened)
  │ ══ TLS "omnibridge-data/1" ═════════════════════►│
  │ DataStreamAuth {id, mac} ────────────────────►│ verify identity + MAC
  │◄──────────────── DataStreamReady {READY} ─────│ challenge consumed
  │ ═══════════ size_bytes of file ═════════════►│ hash while writing
  │ (half-close)                                  │ state: VERIFYING
  │                                               │ sha256 matches → promote
  │◄──────────────── FILE_COMPLETE {id} ──────────│ state: COMPLETED
```

### Desktop → Android

Identical, except the desktop is the sender, so the challenge cannot ride on
`FILE_ACCEPT` — the desktop is not the accepter. It sends `FILE_READY`
instead, and the phone still dials.

```
desktop                                        phone
  │ FILE_OFFER {id, name, size, sha256} ─────────►│  ← no challenge here
  │                                               │  ask the human
  │◄──────────────── FILE_ACCEPT {id} ────────────│  (pending entry opened)
  │ state: TRANSFERRING                           │
  │ FILE_READY {id, challenge} ──────────────────►│
  │◄══ TLS "omnibridge-data/1" ══════════════════════│
  │◄─────────────── DataStreamAuth {id, mac} ─────│
  │ DataStreamReady {READY} ─────────────────────►│
  │ ═══════════ size_bytes of file ══════════════►│ hash while writing
  │◄──────────────── FILE_COMPLETE {id} ──────────│ verified → published
```

## Why `FileOffer` has no challenge field

An offer exists before anyone has agreed to anything. A challenge present at
that point would let a peer open the data stream and start moving bytes before
the receiving human accepted. The field's absence makes that unrepresentable
rather than merely discouraged.

A second, quieter benefit: by the time a dialer holds a challenge, the acceptor
has *already* moved the transfer into `TRANSFERRING`. There is no race in
which a legitimate stream arrives before the acceptor is ready and is refused.

## Capability and grants

Capability id: `files.v1`. It follows the existing model
([ADR-0008](../adr/ADR-0008-capability-architecture.md)) exactly.

**It is never auto-granted.** `auto_grant` remains `["battery.v1"]`: writing a
file to someone's disk is a side effect, and ADR-0008 requires those to be
explicit. On the desktop:

```bash
omnibridge grant <device> files.v1     # allow
omnibridge revoke <device> files.v1    # withdraw, immediately
```

On the phone, a per-computer toggle on the device card does the same.

Authorization is checked **on the receiving side**, from the receiver's own
trust store, and never inferred from what the sender advertised. It is asked
at four separate moments:

1. the session layer filters capability messages against the grant set fixed
   at handshake time;
2. `files.v1` re-checks when an offer arrives;
3. it re-checks after the human answers, because a pairing can be revoked
   while a prompt sits on screen;
4. it re-checks when a data stream authenticates, and periodically while one
   runs.

Widening a grant takes effect on the **next connection**, because the session's
effective capability set is fixed at handshake time. Narrowing takes effect
**immediately**, including against a transfer already in flight. That asymmetry
is deliberate and it fails in the safe direction.

## Data-stream authentication

TLS answers "which device is on this socket". It does not answer "which
transfer is this socket for", and a device may have several in flight. The
tempting shortcut — let the dialer name a `transfer_id` and treat it as proof
— would make that id a bearer token, and bearer tokens leak into logs and
crash reports.

```text
mac = HMAC-SHA256(
    key = stream_challenge,              # 32 random bytes, single-use
    msg = "omnibridge/files.v1/data-stream/v1"
          || len_prefixed(acceptor_identity_fingerprint)
          || len_prefixed(dialer_identity_fingerprint)
          || len_prefixed(transfer_id))
```

Same construction as the pairing proof: a standard MAC, a versioned domain
separator, every field length-prefixed. What each binding buys:

| Binding | Stops |
| --- | --- |
| challenge is random, single-use, and only ever sent inside the control session's TLS | transfer-id guessing, replay, using an id as a bearer token |
| acceptor fingerprint | a proof captured against one machine being used against another |
| dialer fingerprint | a different device replaying a captured proof |
| transfer id | a stream authorized for one transfer being attached to another |

The acceptor **also** requires that the TLS peer certificate on the data
connection is the same pinned identity that negotiated the transfer. The MAC
is defence in depth on that, not a substitute.

Refusals are generic on the wire: a dialer that guessed learns only that it did
not work, never which check failed or whether the id existed.

## State machine

```
                    ┌──────────┐                    ┌───────────────┐
     sender ────────│ OFFERED  │      receiver ─────│ WAITING_ACCEPT│
                    └────┬─────┘                    └───────┬───────┘
                         │ peer accepted                    │ human said yes
                         └──────────────┬───────────────────┘
                                        ▼
                                ┌───────────────┐
                                │ TRANSFERRING  │  bytes moving
                                └───────┬───────┘
                 sender: FILE_COMPLETE  │  receiver: all bytes in
                        ┌───────────────┴────────────┐
                        ▼                            ▼
                 ┌────────────┐              ┌─────────────┐
                 │ COMPLETED  │◄─────────────│  VERIFYING  │ sha256 matches
                 └────────────┘              └─────────────┘
```

Every live state can also reach `FAILED` or `CANCELLED`. **Nothing leaves a
terminal state** — that single rule is what makes a duplicate `FILE_COMPLETE`,
a late cancel, and a second data stream for a finished transfer all
uninteresting.

State is never derived from a socket, a file on disk or a log line. There is
one transition table (`TransferState::can_transition_to` in Rust,
`TransferState.canTransitionTo` in Kotlin) and one function that calls it.

A receiver has no path from "last byte arrived" to "done" that skips
`VERIFYING`. A sender never completes itself: only the receiver's
`FILE_COMPLETE` completes a send, because declaring success when our own write
finished would report a file as delivered that the other side may have thrown
away.

## Integrity

The offer carries SHA-256 over the whole file, so the receiver knows the
expected digest **before the first byte arrives**. That is what makes the check
a verification rather than a comparison against whatever turned up.

1. bytes stream into a temp file, hashed as they are written;
2. exactly `size_bytes` are accepted — no more, no fewer;
3. the digest is compared;
4. only then is the file promoted to a name the user can see.

On failure the partial file is deleted. A file that fails its hash is never,
at any instant, visible under its final name.

Two disagreements are caught separately from a hash mismatch, because they
have a clearer cause:

* **truncation** — EOF before `size_bytes`;
* **overrun** — anything written after `size_bytes`. The receiver never reads
  past the declared size, so the overflow cannot reach the file even
  transiently.

## Filesystem safety

A peer-supplied filename is a *hint about what to call the file*, never an
instruction about where to put it. `filename::sanitize` (Rust) and
`Filenames.sanitize` (Kotlin) are the same rules, asserted by the same test
cases on both sides:

| Input | Result |
| --- | --- |
| `../../etc/passwd` | `passwd` |
| `..\..\windows\system32\cmd.exe` | `cmd.exe` |
| `/etc/shadow` | `shadow` |
| `safe.txt\0.sh` | `safe.txt.sh` |
| `a\nb.txt` | `ab.txt` |
| `evil.txt.` | `evil.txt` |
| `..`, `.`, `/`, `` , `CON.txt` | **rejected** |
| 400 characters | capped at 255 bytes, extension kept, cut on a character boundary |

What is stripped, and why:

* **everything up to the last separator** — both `/` and `\`, so a
  Windows-shaped path cannot smuggle a component past a Unix-only split;
* **control characters including NUL** — a NUL truncates a C string, so a name
  that passes a Rust or Kotlin check could mean something else to a syscall;
  control characters also forge terminal and notification output;
* **trailing dots and spaces** — harmless on Linux, silently trimmed by
  Windows and SMB, which makes `evil.txt.` and `evil.txt` the same file there;
* **`.` and `..`** — they name directories;
* **Windows device names** — `CON`, `NUL`, `LPT1`; cheap, and a received file
  is routinely synced onward.

A leading dot is deliberately *allowed*: `.gitignore` is an ordinary filename
landing in a dedicated directory where a dotfile means nothing.

A name that sanitizes to nothing is **rejected**, never renamed. Inventing a
name for a file whose own name was hostile would hide the attack from the
person being attacked.

The offer format has **no path field at all**, relative or absolute. There is
nothing a peer could set that would name a location on the receiver.

## Desktop destination

`<XDG downloads>/OmniBridge`, resolved without hardcoding a home directory:

1. `$XDG_DOWNLOAD_DIR` if absolute;
2. `XDG_DOWNLOAD_DIR` from `${XDG_CONFIG_HOME:-$HOME/.config}/user-dirs.dirs`,
   which is where `xdg-user-dirs` records a localised Downloads folder;
3. `$HOME/Downloads`.

Overridable with `omnibridged --download-dir`.

Received bytes go to a hidden `.omnibridge-<id>.part` **inside that directory**,
not `/tmp`. That is not tidiness: `rename(2)` is atomic only within one
filesystem and `/tmp` is routinely a different one, so a temp file next to its
destination is what makes the final promotion a genuine atomic rename.

Every file is created with `O_EXCL` at mode 0600, in a 0700 directory.
`O_EXCL` is what makes this safe against a symlink planted in the download
directory: the open fails rather than following it.

**Duplicate names.** `photo.jpg`, then `photo (1).jpg`, then `photo (2).jpg`,
up to 999. The name is reserved by *creating* it with `O_EXCL`, not by testing
for existence and then creating it — the test-then-create form has a window in
which two concurrent transfers both see the name free and the second destroys
the first. Nothing is ever overwritten.

## Android destination

`MediaStore.Downloads` with `RELATIVE_PATH = Download/OmniBridge`. On API 29+ —
this app's floor — that needs **no storage permission at all**. There is no
`MANAGE_EXTERNAL_STORAGE`, no `WRITE_EXTERNAL_STORAGE`, and no path anywhere.

`IS_PENDING` is the whole integrity story on Android, and it maps exactly onto
the desktop's temp-then-verify-then-promote model: a pending item is invisible
to every other app, so bytes are streamed in, hashed, compared, and only then
is `IS_PENDING` cleared. A file that fails its hash is deleted while still
invisible.

Duplicate names are handled by MediaStore itself, which renames rather than
overwrites — the same convention, verified by an instrumented test rather than
assumed.

## Android send: the Sharesheet

`Gallery / Files / Browser → Share → OmniBridge → Send`.

`SendActivity` handles `ACTION_SEND`. The `content://` URI is read through the
`ContentResolver` and **never resolved to a filesystem path** — turning a
content URI into a path is the classic Android mistake: the path is often
wrong, often unreadable, and on a `FileProvider` URI from another app it can be
turned into a traversal. The resolver respects the temporary read grant that
came with the intent and works for providers with no file behind them at all.

The display name from another app is treated exactly like one from the
network: same sanitizer.

The stream is read **twice** — once to compute the SHA-256, once to send —
because the offer must carry the digest before the first byte moves. That
costs one extra read and keeps memory flat. A provider whose content changes
between the passes produces a hash mismatch on the receiver, which fails the
transfer rather than delivering something that does not match its own digest.

`ACTION_SEND_MULTIPLE` is registered so OmniBridge appears for multi-select, and
sends the first item, saying so. Full batching is the documented next
increment (see *Not in this version*).

## Desktop send: the CLI

```bash
omnibridge send <device> <file>
omnibridge transfers
omnibridge cancel <transfer-id-prefix>
```

`<device>` is a device id or a fingerprint prefix of at least 8 characters. An
ambiguous prefix is an **error**, never a guess — sending a file to the wrong
device because a prefix matched two of them is not a failure mode worth
having. `omnibridge cancel` follows the same rule with a 4-character minimum.

## Receiver approval

Required by default, on both sides. There is no global auto-accept.

**Android** shows an accept/reject card carrying the sanitized filename, the
size and the sender's short fingerprint.

**The desktop** daemon has no terminal of its own — it runs under
`systemd --user` — so it cannot prompt. It **declines and logs**, rather than
inventing a silent yes. `omnibridged --accept-files-without-asking` is the
documented escape hatch for an unattended test rig; it warns on startup and on
every accepted file. A desktop GUI, and a per-device "always allow from this
trusted device", are the planned real answers (see *Not in this version*).

## Cancellation

Either side, at any point before a terminal state. A cancel:

* is polled between chunks by the copy loop, so it lands while bytes are
  moving rather than at the end of the file;
* closes the data stream;
* deletes the partial file (or the pending MediaStore row);
* moves the transfer to `CANCELLED`, distinct from `FAILED` so that "you
  pressed cancel" is never displayed as an error;
* is propagated to the peer over the control session, which kept working
  throughout — the whole point of the two-channel split.

A cancel of an already-terminal transfer is a no-op, not an error.

## Disconnect

If the control session dies mid-transfer, the transfer **fails**. There is no
resume in v1.

This needs saying explicitly because it is not automatic: the data stream is a
*separate* TCP connection and does not break when the control session does. A
per-transfer reaper notices that the session channel has closed and ends the
transfer, deletes the partial file, and sets `FAILED` with reason
`TRANSPORT`. Nothing is left in `TRANSFERRING` forever.

Resume is a plausible future increment — the pieces are there, since the
receiver already knows the expected size and digest — but it needs a durable
record of partial state, which this version deliberately does not keep.

## Revocation

Unpairing a device, or withdrawing its `files.v1` grant, stops its in-flight
transfers **immediately**, by two independent mechanisms:

* **deterministic**: `omnibridge unpair` and `omnibridge revoke` call into the
  transfer manager directly, before the session is torn down, so the peer
  still receives the cancellation;
* **backstop**: the reaper re-asks the authorizer for every active transfer,
  so a grant that disappears by any other route is noticed within a second.

A revoked peer's partial file is deleted like any other.

## IPv4 / IPv6

The data stream dials the same address and port the control session used, on
the listener that was already certified for both families
([ADR-0005](../adr/ADR-0005-lan-discovery-mdns.md)). There is no second
discovery mechanism, no second port, and no address-family logic of its own.

## Limits

Defaults, all configurable where it makes sense:

| Limit | Default | Why |
| --- | --- | --- |
| concurrent transfers per peer | 4 | bounds open descriptors, temp files and stream tasks |
| maximum file size | 16 GiB (`--max-file-mib`) | streaming does not care about size, so a low limit would only block real use; this bounds "how much disk can one transfer fill" |
| filename | 255 bytes | `NAME_MAX`; counted in bytes, not characters |
| MIME type | 128 bytes | the longest real IANA type is well under 100 |
| accept timeout | 120 s | long enough to pick up a phone |
| stream-open timeout | 30 s | both peers are already connected; this reclaims a transfer someone agreed to and abandoned |
| stream idle timeout | 60 s | bounds silence, not total duration, so a large file is not penalised |
| verdict timeout | 600 s | the receiver still has to hash and store a large file |
| data-stream handshake frame | 4 KiB | checked before allocation, like `MAX_FRAME_LEN` |
| copy buffer | 64 KiB | what a transfer costs in memory, whatever the file's size |
| duplicate-name attempts | 999 | |

## Memory

A transfer costs one 64 KiB buffer and one file descriptor, on both sides, in
both directions, whatever the file's size. Nothing anywhere holds a whole
file: not the copy loop, not the digest computation, not the offer path. This
is a property of the loops rather than a configured limit.

## Logging

**May appear:** truncated transfer id (8 hex characters), *sanitized*
filename, size, byte counts, state, truncated peer fingerprint, failure
reasons.

**Must never appear:** the stream challenge (it is a MAC key), file content,
absolute paths on the *peer*, private keys, pairing tokens, or the **raw**
peer-supplied filename — that last one is attacker-controlled and could forge
log lines.

`TransferId`'s `Display` prints the truncated form, so reaching for the
obvious formatter cannot put a full id in a log. `StreamChallenge`'s `Debug`
prints `<redacted>` and it is zeroed on drop.

Failure reasons sent to a peer come from a fixed enum, so a path or an errno
string cannot escape through that channel.

## Not in this version

Recorded so they are choices rather than oversights:

* **Resume after a disconnect.** Needs a durable record of partial state.
* **`ACTION_SEND_MULTIPLE` batching.** Needs a queue, per-item progress, and a
  partial-failure story.
* **Directories.** The offer has no path field by design; sending a tree needs
  a relative-name field and a matching sanitizer for each component.
* **"Always allow files from this trusted device."** The per-peer setting the
  sprint anticipated. Approval is per-transfer for now.
* **A desktop GUI prompt.** The daemon cannot ask a human today, so it
  declines.
* **Bandwidth limiting and transfer queueing.**
