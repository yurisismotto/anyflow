# Sprint report — `files.v1`

**Branch:** `feature/file-transfer-v1` · **Working tree:** uncommitted, as instructed

> ## Certification: **NOT CERTIFIED**
>
> 14 of 16 gates pass. FILE-02 and FILE-03 require real hardware, and no
> Android device is attached — `adb devices` is empty. Those two gates are
> **unexecuted, not inferred**.

---

## 1. Executive summary

| | |
| --- | --- |
| Rust tests | **193** passed, 0 failed |
| Android tests | **149** passed, 0 failed |
| Gates passing | **14 / 16** |
| Gates blocked | **2** |

`files.v1` is implemented on both sides, documented, and tested. File bytes
travel on a dedicated TLS 1.3 data stream authenticated by an HMAC over a
single-use challenge; `MAX_FRAME_LEN` is untouched at 64 KiB and no file byte
enters an `Envelope`. Both transfer directions were verified end to end
against the *real* `anyflowd` binary and the real `anyflow` CLI, with SHA-256
confirmed on both sides.

All 18 security cases from the brief (F1–F18) are covered by executed tests
that drive the real capability over a real TLS data stream with real pinning.
Nothing was stubbed to make a rejection happen.

**Why this is not certified.** FILE-02 and FILE-03 require Android ↔ Fedora
transfers on physical hardware. No device is attached, so those gates were not
run. Every other gate was executed rather than reasoned about, and per the
brief's rule, an unexecuted security or integrity gate blocks certification.

**Ready to finish.** Attach the Galaxy Tab S10 FE+ (`SM-X620`) and the two
remaining gates can be run in one pass — the APK, the instrumented tests and
the daemon are all built and green.

---

## 2. Architecture

Two channels, and nearly everything else follows from keeping them apart. This
is the distinction the brief asked to be documented explicitly.

```
  control channel                             authenticated data stream
  ALPN "anyflow/1"                            ALPN "anyflow-data/1"
  ────────────────────────────────────        ──────────────────────────────
  length-prefixed Envelopes, ≤ 64 KiB         two protobuf frames (≤ 4 KiB)
  replay guard, sequence, dispatch            no replay guard, no dispatch
  carries DECISIONS                           carries BYTES
  one per session                             one per transfer, then closed

  FILE_OFFER    metadata, size, sha256        DataStreamAuth  {id, mac}
  FILE_ACCEPT   + challenge (when receiving)  DataStreamReady {status}
  FILE_READY    + challenge (when sending)    <raw bytes> exactly size_bytes
  FILE_REJECT / FILE_CANCEL / FILE_FAILED
  FILE_COMPLETE the receiver's verdict
```

Both arrive at the **same listener on the same port** with the same mutual
authentication and the same pinned identities. ALPN decides which is which
during the handshake, before a single application byte. A connection
negotiating neither is dropped — treating an absent ALPN as "probably control"
would hand the handshake path to any client that omitted it, so it fails
closed.

Sharing the port means file transfer inherits the listener, the mDNS record,
the dual-stack binding and the connection limit that were already certified.
The brief's "no second discovery mechanism" falls out for free rather than
needing discipline.

### The phone always dials

The direction of the *bytes* is independent of who opens the *socket*. An
Android app is not a stable listener, and making it one would mean a second
foreground service, a second port and an inbound attack surface on the device
holding the user's photos.

| Transfer | Offers | Approves | Dials | Writes bytes |
| --- | --- | --- | --- | --- |
| Android → Fedora | phone | desktop | **phone** | phone |
| Fedora → Android | desktop | phone | **phone** | desktop |

Recorded as `docs/adr/ADR-0013-file-transfer-data-stream.md`, which resolves
the question ADR-0012 deliberately left open. Full specification:
`docs/architecture/FILES.md`.

---

## 3. Control messages

Capability id `files.v1`, payload a `FileControl` oneof carried in
`CapabilityMessage.payload`, which the transport treats as opaque bytes.

| Message | Sent by | Carries |
| --- | --- | --- |
| `FILE_OFFER` | sender | `transfer_id`, filename, `size_bytes`, `mime_type`, `sha256`, timestamp |
| `FILE_ACCEPT` | receiver | `transfer_id`, + challenge when the receiver is the stream acceptor |
| `FILE_READY` | stream acceptor | `transfer_id` + challenge, when the acceptor is the *sender* |
| `FILE_REJECT` | receiver | `transfer_id`, reason |
| `FILE_CANCEL` | either | `transfer_id`, reason |
| `FILE_COMPLETE` | receiver | `transfer_id` — the receiver's verdict, after verification |
| `FILE_FAILED` | either | `transfer_id`, reason |

### Two deliberate refinements to the brief's list

**`FILE_READY` was added**, with justification. The challenge cannot ride on
the offer: an offer exists before anyone has agreed to anything, so a challenge
present there would let a peer open the data stream and start moving bytes
*before the receiving human accepted*. But in the Fedora → Android direction
the desktop is the sender, so the challenge cannot ride on `FILE_ACCEPT`
either. `FILE_READY` exists solely to carry it in that case.

The unifying rule: *the peer that accepts data streams issues the challenge in
whichever control message it sends at the moment the transfer becomes active.*
A second benefit falls out — by the time a dialer holds a challenge, the
acceptor has already moved the transfer into its transferring state, so there
is no race where a legitimate stream arrives too early and is refused.

**`FileOffer` has no path field at all**, relative or absolute, and no
`stream_challenge`. Both absences are load-bearing: the first means nothing a
peer sends can name a location on the receiver; the second makes the early-dial
case unrepresentable rather than merely discouraged.

### Transfer id

16 cryptographically random bytes, single-use, and deliberately **not** derived
from `message_id`. Their lifecycles differ: a message id is meaningful for one
frame and is garbage-collected by the dedup window, while a transfer id must
stay meaningful across many frames and two connections. Reusing it would tie a
transfer's identity to the replay window's bookkeeping.

---

## 4. Authenticated data stream

TLS answers "which *device* is on this socket". It does not answer "which
*transfer* is this socket for", and a device may have several in flight. The
brief explicitly forbids treating `transfer_id` as a sufficient bearer token,
so the stream is authenticated explicitly:

```text
mac = HMAC-SHA256(
    key = stream_challenge,              # 32 random bytes, single-use
    msg = "anyflow/files.v1/data-stream/v1"
          || len_prefixed(acceptor_identity_fingerprint)
          || len_prefixed(dialer_identity_fingerprint)
          || len_prefixed(transfer_id))
```

Deliberately the same construction as the existing pairing proof: a standard
MAC, a versioned domain separator, every field length-prefixed so two different
field splits cannot produce the same message. No new cryptography was invented,
and `tls.rs` gained no new verifier — only a second ALPN string.

| Binding | Stops |
| --- | --- |
| challenge is random, single-use, sent only inside the control session's TLS | transfer-id guessing, replay, bearer-token use |
| acceptor fingerprint | a proof captured against one machine reused against another |
| dialer fingerprint | a different device replaying a captured proof |
| transfer id | a stream authorized for one transfer attached to another |

The acceptor **also** requires that the TLS peer certificate on the data
connection is the same pinned identity that negotiated the transfer. The MAC is
defence in depth on that check, never a substitute. Refusals are generic on the
wire, so a prober cannot distinguish "no such transfer" from "bad MAC".

### A design that was tried and rejected

The first attempt derived the stream key from a TLS exporter (RFC 5705) —
elegant, and needs no extra message. It was abandoned because **it cannot be
implemented on Android**: the JDK exposes no keying-material exporter and
Conscrypt's is not public API. A design the phone cannot implement is not a
design. A random challenge sent inside the control session's TLS is exactly as
strong for this purpose.

---

## 5. State machine

Explicit, in one transition table per language, with exactly one function that
calls it. State is never derived from a socket, a file on disk or a log line.

```
              ┌──────────┐                  ┌───────────────┐
 sender ──────│ OFFERED  │    receiver ─────│ WAITING_ACCEPT│
              └────┬─────┘                  └───────┬───────┘
                   │ peer accepted                  │ human said yes
                   └──────────────┬─────────────────┘
                                  ▼
                          ┌───────────────┐
                          │ TRANSFERRING  │   bytes moving
                          └───────┬───────┘
           sender: FILE_COMPLETE  │  receiver: all bytes in
                  ┌───────────────┴────────────┐
                  ▼                            ▼
           ┌────────────┐              ┌─────────────┐
           │ COMPLETED  │◄─────────────│  VERIFYING  │ sha256 matches
           └────────────┘              └─────────────┘

 every live state may also reach FAILED or CANCELLED
```

Three rules carry most of the safety:

* **Nothing leaves a terminal state.** That single rule makes a duplicate
  `FILE_COMPLETE` (F17), a late cancel, and a second data stream for a finished
  transfer all uninteresting.
* **A receiver has no path from "last byte" to "done" that skips `VERIFYING`.**
* **A sender never completes itself.** Only the receiver's `FILE_COMPLETE`
  completes a send — declaring success when our own write finished would report
  a file as delivered that the other side may have discarded.

Both implementations are tested against the same table (`transfer.rs` tests;
`TransferStateTest.kt`), so the two devices cannot disagree about whether a
transfer is live.

---

## 6. Permission and grant model

`files.v1` follows the existing capability model exactly and is **never
auto-granted**. `auto_grant` remains `["battery.v1"]`: writing a file to
someone's disk is a side effect, and ADR-0008 requires those to be explicit.

```bash
anyflow grant  <device> files.v1    # allow
anyflow revoke <device> files.v1    # withdraw, immediately
```

On Android, a per-computer toggle on the device card does the same.
Authorization is checked **on the receiving side**, from the receiver's own
trust store, never inferred from what the sender advertised — and it is asked
at four separate moments:

1. the session layer filters capability messages against the grant set fixed at
   handshake time;
2. `files.v1` re-checks when an offer arrives;
3. it re-checks *after* the human answers, because a pairing can be revoked
   while a prompt sits on screen;
4. it re-checks when a data stream authenticates, and periodically while one
   runs.

> **A rough edge, documented rather than hidden.** Widening a grant takes
> effect on the *next connection*, because a session's effective capability set
> is fixed at handshake time. Narrowing takes effect *immediately*, including
> against a transfer already running. The asymmetry fails in the safe
> direction, but after `anyflow grant … files.v1` the phone must reconnect.

---

## 7. Android send flow

`Gallery / Files / Browser → Share → AnyFlow → choose device → Send`. A
dedicated `SendActivity` handles `ACTION_SEND`, so the launcher activity stays
clean.

* The `content://` URI is read through `ContentResolver` and is **never
  resolved to a filesystem path**. Turning a content URI into a path is the
  classic Android mistake: the path is often wrong, often unreadable, and on a
  `FileProvider` URI from another app it can be turned into a traversal. There
  is no `getPath` anywhere in `files.v1`.
* The display name from another app is treated exactly like one from the
  network — same sanitizer.
* The stream is read **twice**: once to compute SHA-256, once to send. The
  offer must carry the digest before the first byte moves, and buffering the
  file to hash it once is exactly what FILE-13 forbids. A provider whose
  content changes between passes produces a hash mismatch on the receiver,
  which fails the transfer rather than delivering something that does not match
  its own digest.
* No storage permission is declared. The temporary read grant that arrives with
  the intent is strictly less access than any storage permission would be.

`ACTION_SEND_MULTIPLE` is registered so AnyFlow appears for multi-select and
sends the first item, saying so. Full batching is documented as the next
increment, as the brief permits.

---

## 8. Android receive flow

`MediaStore.Downloads` with `RELATIVE_PATH = Download/AnyFlow`. On API 29+ —
this app's floor — that needs **no storage permission at all**. No
`MANAGE_EXTERNAL_STORAGE`, no `WRITE_EXTERNAL_STORAGE`, no path.

`IS_PENDING` is the whole integrity story on Android, and it maps exactly onto
the desktop's temp-then-verify-then-promote model. A pending item is invisible
to every other app, so:

1. insert with `IS_PENDING = 1`, stream the bytes in;
2. hash while writing;
3. compare against the offer;
4. **only then** clear `IS_PENDING`, which publishes it.

A file that fails its hash is deleted while still invisible. There is no window
in which unverified bytes appear as a finished download. The user sees an
accept/reject card carrying the sanitized filename, the size and the sender's
short fingerprint, then a progress bar, then the outcome.

---

## 9. Fedora send flow

```bash
anyflow send <device> <file>      # offers, then streams live progress
anyflow transfers                  # everything since the daemon started
anyflow cancel <transfer-prefix>   # stop one mid-flight
```

`<device>` is a device id or a fingerprint prefix of at least 8 characters,
resolved by the existing `resolve_device`. **An ambiguous prefix is an error,
never a guess** — sending a file to the wrong device because a prefix matched
two of them is not a failure mode worth having. `anyflow cancel` follows the
same rule with a 4-character minimum.

The daemon hashes the file (streamed, bounded buffer), generates the transfer
id, sends `FILE_OFFER`, and streams `TransferProgress` events back over the
control socket so the CLI renders a live progress bar without polling. Only the
basename is transmitted.

---

## 10. Fedora receive flow

Destination is `<XDG downloads>/AnyFlow`, resolved without hardcoding a home
directory: `$XDG_DOWNLOAD_DIR` if absolute, else `XDG_DOWNLOAD_DIR` parsed from
`user-dirs.dirs` (so a localised Downloads folder is honoured), else
`$HOME/Downloads`. Overridable with `anyflowd --download-dir`.

Bytes go to a hidden `.anyflow-<id>.part` **inside that directory**, not
`/tmp`. That is not tidiness: `rename(2)` is atomic only within one filesystem
and `/tmp` is routinely a different one, so a temp file next to its destination
is what makes the final promotion a genuine atomic rename.

> **Receiver approval on Fedora.** The daemon has no terminal — it runs under
> `systemd --user` — so it cannot prompt. It **declines and logs** rather than
> inventing a silent yes. `anyflowd --accept-files-without-asking` is the
> documented escape hatch for an unattended test rig; it warns on startup and
> on every accepted file. A desktop GUI prompt and a per-device "always allow"
> are the planned real answers.

---

## 11. Filesystem safety

A peer-supplied filename is a *hint about what to call the file*, never an
instruction about where to put it. Rust and Kotlin implement the same rules,
asserted by the same test cases on both sides so the two cannot quietly
diverge.

| Input | Result |
| --- | --- |
| `../../etc/passwd` | `passwd` |
| `..\..\windows\system32\cmd.exe` | `cmd.exe` |
| `/etc/shadow` | `shadow` |
| `safe.txt\0.sh` | `safe.txt.sh` |
| `a\nb.txt` | `ab.txt` |
| `evil.txt.` | `evil.txt` |
| `..` `.` `/` `""` `CON.txt` | **rejected** |
| 400 characters | capped at 255 bytes, extension kept, cut on a character boundary |

What is stripped and why: everything up to the last separator (**both** `/` and
`\`, so a Windows-shaped path cannot smuggle a component past a Unix-only
split); control characters including NUL (a NUL truncates a C string, so a name
passing a Rust check could mean something else to a syscall); trailing dots and
spaces (silently trimmed by Windows and SMB, which makes `evil.txt.` and
`evil.txt` the same file there); `.` and `..`; Windows device names.

A leading dot is deliberately *allowed* — `.gitignore` is an ordinary filename
landing in a dedicated directory. A name that sanitizes to nothing is
**rejected, never renamed**: inventing a name for a file whose own name was
hostile would hide the attack from the person being attacked.

Every file is created with `O_EXCL` at mode 0600 in a 0700 directory, so a
symlink planted in the download directory causes the open to fail rather than
be followed. Duplicate names are numbered — `photo (1).jpg` — by *creating* the
reservation, not testing-then-creating, which closes the race in which two
concurrent transfers both see a name free and the second destroys the first.

---

## 12. Integrity model

The offer carries SHA-256 over the whole file, so the receiver knows the
expected digest **before the first byte arrives**. That is what makes the check
a verification rather than a comparison against whatever turned up.

1. bytes stream into a temp file (or invisible pending row), hashed as written;
2. exactly `size_bytes` are accepted — no more, no fewer;
3. the digest is compared;
4. only then is the file promoted to a name the user can see.

On failure the partial file is deleted. A file that fails its hash is never, at
any instant, visible under its final name. Two disagreements are caught
separately from a hash mismatch because they have a clearer cause:
**truncation** (EOF before `size_bytes`) and **overrun** (anything after it).
The receiver never reads past the declared size, so an overflow cannot reach
the file even transiently.

---

## 13. Cancellation

Either side, at any point before a terminal state. A cancel is polled between
chunks by the copy loop, so it lands *while bytes are moving* rather than at
the end of the file. It closes the data stream, deletes the partial file, moves
the transfer to `CANCELLED` — distinct from `FAILED`, so "you pressed cancel"
is never displayed as an error — and is propagated to the peer over the control
session, which kept working throughout. That last point is the two-channel
split earning its keep.

Cancelling an already-terminal transfer is a no-op, not an error. Verified by
`f13_cancelling_mid_transfer_stops_it_and_deletes_the_partial_file`, which
cancels a genuinely mid-copy transfer and asserts the peer receives the
cancellation.

---

## 14. Disconnect behaviour

If the control session dies mid-transfer, the transfer **fails**. There is no
resume in v1, as the brief permits.

This needed explicit handling and is the single most important consequence of
choosing two connections: **a data stream does not die when its control session
does**. It is a separate TCP connection and would happily keep running, or
leave a transfer in `TRANSFERRING` forever holding a partial file and a
concurrency slot. A per-transfer reaper notices the session channel has closed,
ends the transfer with reason `TRANSPORT`, and deletes the partial file.
Nothing sits in a live state indefinitely.

Resume is documented as future work: the receiver already knows the expected
size and digest, so it is tractable, but it needs durable partial state that
this version deliberately does not keep.

---

## 15. Revocation behaviour

Unpairing a device, or withdrawing its `files.v1` grant, stops its in-flight
transfers **immediately**, by two independent mechanisms:

* **Deterministic** — `anyflow unpair` and `anyflow revoke` call into the
  transfer manager directly, *before* the session is torn down, so the peer
  still receives the cancellation.
* **Backstop** — the reaper re-asks the authorizer for every active transfer,
  so a grant that disappears by any other route is noticed within a second.

A revoked peer's partial file is deleted like any other, and it cannot open a
new data stream: the grant check runs before the transfer record is even looked
at, so a revoked peer learns nothing about which transfer ids exist. Both
mechanisms are separately tested
(`f15_revoking_a_peer_mid_transfer_stops_it_immediately`,
`f15_a_transfer_whose_grant_disappears_is_reaped`).

---

## 16. IPv4 / IPv6

The data stream dials the same address and port the control session used, on
the listener already certified for both families. There is no second discovery
mechanism, no second port, and no address-family logic of its own — which is
precisely why this was cheap.

`a_transfer_works_over_both_address_families` runs a full transfer over IPv4
loopback and again over IPv6 loopback against a dual-stack listener, verifying
the received bytes each time.

---

## 17. Abuse limits

Every bound lives in one file (`limits.rs`) with its rationale. The rule they
follow: a limit exists to stop a hostile or broken peer consuming something
unbounded, and is set high enough that an honest user never meets it.

| Limit | Default | Why |
| --- | --- | --- |
| Concurrent transfers per peer | 4 | bounds descriptors, temp files, stream tasks |
| Maximum file size | 16 GiB | configurable; streaming does not care about size, so a low limit would only block real use |
| Filename | 255 B | `NAME_MAX`; counted in bytes, not characters |
| MIME type | 128 B | longest real IANA type is well under 100 |
| Accept timeout | 120 s | long enough to pick up a phone |
| Stream-open timeout | 30 s | both peers are already connected |
| Stream idle timeout | 60 s | bounds silence, not duration, so a large file is not penalised |
| Verdict timeout | 600 s | the receiver still has to hash and store a large file |
| Data-stream frame | 4 KiB | checked before allocation, like `MAX_FRAME_LEN` |
| Control-send timeout | 2 s | see §22 — bounds a handler's wait for outbound queue room |
| Copy buffer | 64 KiB | what a transfer costs in memory, whatever the file's size |
| Duplicate-name attempts | 999 | |

---

## 18. Files changed

`git diff --stat`: **27 files modified, +2003 / −44**, plus 14 new paths.
**No commits and no pushes were made**, per the brief.

### New

| Path | What |
| --- | --- |
| `protocol/proto/…/files_v1.proto` | control messages + data-stream handshake |
| `desktop/capabilities/files/` | new crate: `lib.rs`, `auth.rs`, `filename.rs`, `destination.rs`, `stream.rs`, `transfer.rs`, `limits.rs` |
| `desktop/daemon/tests/files.rs` | 35 integration tests, F1–F18 |
| `android/…/files/` | `FileTransferManager`, `DataStream`, `StreamAuth`, `Filenames`, `TransferState`, `Downloads`, `SharedFile` |
| `android/…/capability/FilesCapability.kt` | the capability handler |
| `android/…/ui/SendActivity.kt`, `TransferViews.kt` | Sharesheet target and transfer UI |
| 4 × Android unit test files, 1 × instrumented | filenames, stream auth, state machine, copy loop, MediaStore |
| `docs/adr/ADR-0013-…md`, `docs/architecture/FILES.md` | decision record and specification |

### Modified — foundation touched only where additive

* `core/src/lib.rs`, `core/src/tls.rs` — added `ALPN_DATA_PROTOCOL`,
  `data_stream_client_config`, `negotiated_protocol`. **No verifier changed**;
  the server config now advertises both ALPNs.
* `daemon/src/listener.rs` — one `match` on negotiated ALPN, failing closed.
* `daemon/src/{state,control,server,main}.rs`, `cli/src/main.rs` — transfer
  manager wiring, `grant`/`revoke`/`send`/`transfers`/`cancel`.
* `android/…/net/PinnedTrustManager.kt` — `harden()` gained an optional ALPN
  parameter. `PeerConnection.kt` — exposes `remoteAddress`.
* `daemon/tests/e2e.rs` — two assertions updated: the advertised capability set
  genuinely grew, and the test now also asserts that advertising `files.v1`
  does *not* grant it.

---

## 19. Rust tests

> `cargo fmt --all --check` clean · `cargo build --workspace` clean ·
> `cargo test --workspace` **193 passed, 0 failed** ·
> `cargo clippy --workspace --all-targets` zero warnings.

Baseline before this sprint was 100 tests; `files.v1` adds 93 (58 unit in the
new crate, 35 integration). The security matrix maps to executed tests:

| Case | Test |
| --- | --- |
| F1 no grant | refused at the transport layer, *and* a withdrawn grant refused by the capability itself within a live session |
| F2 unknown peer | `f2_a_stranger_cannot_open_a_data_stream` |
| F3 wrong identity | another paired device, holding the *correct* challenge and a forged MAC, is refused on TLS identity |
| F4 guessed id | right id, right peer, right TLS — four wrong MACs, all refused; the legitimate dialer's chance survives |
| F5 reused id | duplicate offer refused; replayed auth frame refused (challenge consumed) |
| F6 expired transfer | real reaper timeout; a late dialer with a correct MAC still gets nothing |
| F7 / F8 traversal & absolute | 6 hostile names, each stored as a basename inside the download dir; canary file outside untouched; no subdirectory created |
| F9 SHA mismatch | same length, different bytes → `Integrity`, nothing promoted, partial deleted |
| F10 / F12 truncated | short stream fails, nothing left behind |
| F11 oversized | 6000 bytes for a 1000-byte offer → refused; exactly 1000 ever accepted |
| F13 cancellation | mid-copy cancel; decline; unanswered offer times out |
| F14 disconnect | session dropped mid-copy with the data stream held open → `FAILED`, partial deleted |
| F15 revocation | deterministic path and reaper backstop, separately |
| F16 oversized metadata | 40 KB filename, long MIME, 31-byte hash, empty hash, `u64::MAX` size |
| F17 duplicate complete | three completions + a late cancel; state unchanged |
| F18 isolation | two concurrent transfers intact; cancelling one leaves the other |

---

## 20. Android tests

> `./gradlew :app:testDebugUnitTest` **149 passed, 0 failed** ·
> `:app:assembleDebug` clean · `:app:assembleDebugAndroidTest` clean, no
> warnings.

Baseline was 105; `files.v1` adds 44 across `FilenamesTest` (15),
`StreamAuthTest` (10), `TransferStateTest` (7), `DataStreamCopyTest` (8), plus
4 rebalanced.

### Cross-language contracts

Two constructions are pinned as checked contracts rather than left as two
implementations that happen to agree:

* **The data-stream MAC.** `StreamAuthTest` asserts the same vector as the Rust
  `auth::tests` —
  `dc03a57c8cc240452a3d109540a529b7c4172bd3dce2d983a2d8bfcd37fa1f58`. If either
  changes without the other, the phone and desktop can no longer open a data
  stream, and both suites fail loudly.
* **The filename sanitizer.** Both suites assert the same 15 cases, so one
  device can never accept a name the other refuses.

### Instrumented (not mocked)

The brief asks not to rely on mocks where real Android behaviour matters.
`DownloadsTest` exercises the real MediaStore: that a pending entry is
genuinely invisible to a query until published, that a discarded entry leaves
neither a visible file nor an orphan pending row, that a duplicate name is
renamed rather than overwritten, and that a ~13 MiB file streams through
without being held in memory. These are platform behaviours; a mock would only
assert that the mock behaves as assumed, which is worth nothing — the entire
Android integrity story rests on `IS_PENDING` really working as documented.
**Compiled and packaged, not yet run: needs a device.**

---

## 21. Hardware tests

The brief asks that test tiers be clearly distinguished. They are:

| Tier | What ran | Status |
| --- | --- | --- |
| JVM unit | 149 Android tests: sanitizer, MAC, state machine, copy loop | **Executed** |
| Rust unit + integration | 193 tests, incl. 35 over real TLS data streams with real pinning | **Executed** |
| Fake peer | real `anyflowd` + real `anyflow` CLI + `fake_phone`, both directions, SHA-256 verified | **Executed** |
| Android instrumented | `DownloadsTest` against real MediaStore | **No device** |
| Real hardware | Android ↔ Fedora with a real photo | **No device** |

### Fake-peer run (executed)

Against the real daemon binary, not the in-process harness. `fake_phone` was
extended to play the dialer role. All 18 checks passed, twice (before and after
the §22 hardening):

* `files.v1` not auto-granted after pairing; sending to an ungranted device
  refused
* `anyflow grant` works; granting an unimplemented capability refused
* phone → desktop: 3 MiB file, name, size and SHA-256 all match; no `.part`
  left behind
* duplicate name → `holiday photo (1).jpg`, original untouched
* desktop → phone: 2.4 MiB file, SHA-256 matches, CLI progress bar to 100%
* `anyflow transfers` lists all three; `anyflow revoke` then blocks further
  sends

> **Not done, and not inferred.** `adb devices` is empty, so nothing ran on the
> Galaxy Tab S10 FE+ (`SM-X620`). Untested on hardware: the Sharesheet intent
> path, real `ContentResolver` reads from Gallery/Files, MediaStore writes, the
> accept/reject UI, Wi-Fi drop mid-transfer, and the TEE-backed identity
> presenting itself on a second TLS connection. `fake_phone` is a test client
> and must never be reported as a phone.

---

## 22. Bugs found

### 1 — Early-dial window in my own first protocol draft

The first draft put `stream_challenge` in `FileOffer`. In the Fedora → Android
direction that would have let the phone open the data stream and pull the file
down *before its user accepted*, defeating receiver approval. Caught while
writing the flow, before any code shipped. Fixed by removing the field entirely
and adding `FILE_READY`; the type now makes the case unrepresentable.

### 2 — TLS exporter design was not implementable on Android

The initial stream-authentication design used an RFC 5705 keying-material
exporter. Android exposes no such API. Caught before implementation; replaced
with the challenge-based MAC.

### 3 — Latent deadlock hazard in the session loop (hardened, not proven)

A capability's `on_message` is awaited *inside* the session's `select!` loop,
and that same loop drains the outbound queue. A handler that waits indefinitely
to enqueue a reply would block the only task that could make room.
`battery.v1` can never reach this — it never replies from `on_message`.
`files.v1` replies to every malformed offer, so it can.

I bounded the send (`try_send`, then a 2-second fallback). **Honest scope:** I
could not construct an input that actually reaches the stuck state — the test
passes with and without the fix, because each session runs in its own task, so
a wedged session harms only that peer. This is *hardening against a hazard
visible in the code*, not a fix for a demonstrated failure, and the test's doc
comment says so. The underlying coupling is in the foundation's `run_session`;
I did not restructure it, since that is a larger and riskier change than the
evidence warrants.

### 4 — Two stale test assertions (expected)

`e2e.rs` asserted the negotiated set was exactly `["battery.v1"]`. The set
genuinely grew. Updated, and strengthened to also assert that advertising
`files.v1` does not grant it.

**No bugs were found in the certified foundation.** Its code was changed only
additively.

---

## 23. Security audit

### Unchanged (FILE-16)

* TLS 1.3 only. No verifier was modified. `PinnedServerCertVerifier`,
  `RecordingClientCertVerifier` and `PinnedTrustManager` are byte-identical.
* Both `verify_tls13_signature` implementations still delegate to the real
  check. No code path disables validation.
* SPKI pinning unchanged. Data streams use the *same* pinned verifier via a
  shared helper — `client_config` and `data_stream_client_config` differ in one
  field: the ALPN string.
* Session resumption still disabled; `MAX_FRAME_LEN` still 64 KiB.

### New attack surface, and what closes it

* **A second inbound connection type.** Routed by ALPN immediately after the
  handshake, failing closed on anything unrecognised. It shares the existing
  connection semaphore, so a peer cannot double its descriptor budget by
  opening data streams.
* **The data-stream auth frame** is the least-trusted input in the capability —
  from a peer that completed TLS but proved nothing else. Capped at 4 KiB,
  checked before allocation, refused generically.
* **Peer-supplied filenames** — covered in §11; threat model T8 upgraded from
  "not applicable" to a full mitigation list.
* **New threat entries** T19 (a data stream opened by the wrong peer or for the
  wrong transfer) and T20 (a transfer that never ends) added to the threat
  model with mitigations and residual risk.

### Logging

Audited: no log statement anywhere in `files.v1` contains a challenge, file
content, or a raw peer-supplied filename. `StreamChallenge`'s `Debug` prints
`<redacted>` and it is zeroed on drop. `TransferId`'s `Display` prints only 8
hex characters, so reaching for the obvious formatter cannot put a full id in a
log. Only *sanitized* filenames are logged. Failure reasons sent to a peer come
from a fixed enum, so a local path or errno cannot escape through that channel.

### Android permissions

No permission was added. No `MANAGE_EXTERNAL_STORAGE`, no
`WRITE_EXTERNAL_STORAGE`, no `READ_MEDIA_*`. Receiving uses MediaStore
(permission-free on API 29+); sending uses the temporary read grant that
arrives with the share intent.

---

## 24. Limitations

* **Not run on physical hardware.** The blocking gap.
* **Widening a grant needs a reconnect.** Narrowing is immediate.
* **No resume.** An interrupted transfer fails and its partial file is deleted.
* **One file per share.** `ACTION_SEND_MULTIPLE` sends the first item only.
* **No directories.** The offer has no path field by design.
* **The Fedora daemon cannot prompt.** It declines by default; the unattended
  flag is the current workaround.
* **No "always allow from this trusted device".** Approval is per-transfer, as
  the brief specified for v1.
* **The two-pass read on Android** reads a shared file twice. Correct and
  memory-flat, but a provider whose content changes between passes causes a
  hash failure rather than a retry.
* **Transfer records are in-memory only** and bounded. No transfer history is
  written to disk — deliberate, consistent with the store's no-history policy.

---

## 25. Technical debt

1. **The `on_message` / outbound-queue coupling in `run_session`** (§22, item
   3). Bounded from the capability side; the structural fix is to give the
   session a separate writer task, as the Android client already has. Worth
   doing before a second replying capability lands.
2. **Grant changes need a reconnect to widen.** `CAPABILITY_ANNOUNCE` exists on
   the wire and is currently accepted-and-ignored; a re-negotiation path driven
   by the *local* grant store (never the peer's assertion) would close this.
3. **No desktop approval mechanism.** The daemon needs a way to ask — D-Bus
   notification with actions, or an `anyflow recv` foreground command.
4. **The Android manager is one large class.** ~700 lines doing state, control
   and stream work. The Rust side is split across six modules; the Kotlin side
   should follow.
5. **The Rust reaper polls at 1 Hz.** Fine at this scale, but event-driven
   cancellation on session close would be tighter.
6. **Timeouts are partly hardcoded.** Two are in `FilesConfig`; the rest are
   constants.
7. **TPM2 sealing of the desktop key** remains the top pre-existing debt,
   inherited from ADR-0006.

---

## 26. Gate status FILE-01 … FILE-16

| Gate | What | Status |
| --- | --- | --- |
| FILE-01 | Capability negotiation | ✅ PASS |
| FILE-02 | Android → Fedora on hardware | ⛔ **BLOCKED** |
| FILE-03 | Fedora → Android on hardware | ⛔ **BLOCKED** |
| FILE-04 | SHA-256 integrity | ✅ PASS |
| FILE-05 | Path traversal protection | ✅ PASS |
| FILE-06 | Unauthorized peer rejection | ✅ PASS |
| FILE-07 | Wrong identity rejection | ✅ PASS |
| FILE-08 | Cancellation | ✅ PASS |
| FILE-09 | Disconnect cleanup | ✅ PASS |
| FILE-10 | Duplicate filename handling | ✅ PASS |
| FILE-11 | Revocation handling | ✅ PASS |
| FILE-12 | IPv4 / IPv6 | ✅ PASS |
| FILE-13 | No whole-file buffering | ✅ PASS |
| FILE-14 | Rust tests | ✅ PASS |
| FILE-15 | Android tests | ✅ PASS |
| FILE-16 | TLS / pinning unchanged | ✅ PASS |

**FILE-02 and FILE-03 are unexecuted, not failed.** Both directions pass
against the real daemon binary with SHA-256 verification, but by a Rust test
client rather than an Android device. The brief is explicit that a gate must
not be inferred, so the sprint is reported as **NOT CERTIFIED** until the
tablet is attached and those two run.

---

## 27. Recommendation for the next sprint

### First: close this one

Attach the Galaxy Tab S10 FE+, install the APK, and run FILE-02 and FILE-03
plus the hardware matrix the brief lists — cancel, Wi-Fi drop mid-transfer,
reject, same-name collision, hostile filename. Also run
`connectedDebugAndroidTest` for `DownloadsTest`. This is a single session's
work; nothing is expected to fail, but nothing should be claimed until it runs.

### Then: `clipboard.v1`, and pay one debt first

Clipboard is the natural next capability and it is small — text, a size cap, no
new transport. But it is the second capability that *replies from*
`on_message`, which makes debt item 1 (§25) worth clearing first: give the
session a dedicated writer task, mirroring what the Android client already
does. That is a contained change to `run_session`, and doing it before a second
replying capability is much cheaper than after.

Clipboard also raises a policy question this sprint deferred and clipboard
cannot: **per-peer auto-accept**. Nobody will approve every clipboard sync by
hand, so "always allow from this trusted device" has to exist. Designing it for
clipboard and retrofitting it to `files.v1` is the right order — the grant
model is already the place it belongs.

### Deliberately not next

Resume, directory transfer and multi-file batching are all real, all wanted,
and none of them are as valuable as a second capability proving the
architecture is genuinely additive. They are `files.v2` material.

---

*AnyFlow · `files.v1` · branch `feature/file-transfer-v1` · working tree
uncommitted, as instructed. Fedora peer fingerprint and device ids in this
report come from disposable test daemons, not the developer's own identity.*
