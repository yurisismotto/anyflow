# `clipboard.v1`

Text clipboard sharing between a Linux desktop and an Android device.

## What this is, stated accurately

```text
Automatic  desktop → Android   clipboard sync   (opt-in, per device)
Manual     Android → desktop   clipboard send   (a person taps a button)
```

It is **not** "automatic bidirectional clipboard", and saying so would be
wrong rather than merely optimistic. Android 10 and later refuse
`getPrimaryClip` to an app that does not have input focus, so an ordinary
Android app cannot watch its own clipboard. Every technique that defeats that
restriction — an `AccessibilityService`, becoming the default IME, an
invisible focus-stealing activity, `READ_LOGS`, root — is either forbidden by
Play policy, hostile to the user, or both. OmniBridge uses none of them and
therefore does not have background clipboard reading on Android. See
[Android's limitation](#the-android-limitation) below.

## Scope

**Text only.** There is deliberately no field in the schema for an image, a
file, a URI or HTML. A clipboard that carries a file is a file transfer with a
worse consent story, and [`files.v1`](FILES.md) already exists for that.

Also out of scope, and absent rather than disabled: clipboard history, cloud
clipboard, cross-internet relay, and remote commands.

## One channel

Everything travels as the opaque `payload` of a `CapabilityMessage` on the
**existing control session** (ALPN `omnibridge/1`). No second socket, no second
listener, no data stream.

```text
┌──────────────────────────────────────────┐
│ clipboard.v1     ClipboardControl        │  ≤ 32 KiB of UTF-8
├──────────────────────────────────────────┤
│ session          Envelope, replay guard  │
├──────────────────────────────────────────┤
│ framing          u32 length + protobuf   │  MAX_FRAME_LEN = 64 KiB
├──────────────────────────────────────────┤
│ TLS 1.3          mutual auth, SPKI pinned│
└──────────────────────────────────────────┘
```

`files.v1` needed a second connection because a multi-gigabyte file cannot fit
in a 64 KiB frame and must not delay a cancel. A clipboard is small by nature,
so the split would buy nothing and cost a second authenticated connection to
reason about.

## Protocol

```protobuf
ClipboardUpdate {
  bytes  event_id          = 1;  // exactly 16 CSPRNG bytes, per event
  string origin_device_id  = 2;  // where the copy happened; NOT identity
  string text_utf8         = 3;  // ≤ MAX_CLIPBOARD_TEXT_BYTES
  bytes  content_hash      = 4;  // SHA-256 over the UTF-8 bytes, 32 bytes
  bool   sensitive_hint    = 5;  // a presentation hint, never an ACL
  int64  timestamp_unix_ms = 6;  // informational only
}

ClipboardResult { bytes event_id = 1; ClipboardOutcome outcome = 2; }

ClipboardControl { oneof body { update = 1; result = 2; } }
```

### `event_id`

16 cryptographically random bytes, generated per clipboard event, and
deliberately **not** the envelope's `message_id` — the same rule `files.v1`
follows for `transfer_id`. A message id is meaningful for one frame and is
aged out by the session's replay window; a clipboard event id must stay
meaningful for a de-duplication cache that outlives both the frame and,
potentially, the connection. Two ids with different lifetimes must not be the
same field.

An id whose length is not 16 is refused and **not answered**: there is nothing
coherent to correlate a reply with, and echoing an attacker-chosen id back
buys nothing.

### `content_hash`

SHA-256 over the UTF-8 bytes of `text_utf8`. It exists for three things:

* de-duplication,
* loop suppression,
* diagnostics that must not log the content.

It is **not authentication**. TLS 1.3 with pinned identities and the
`clipboard.v1` grant are what make a message trustworthy; a matching hash from
an unauthorized peer still gets nowhere. It is compared with an ordinary `!=`
rather than in constant time, on purpose — there is no secret here, and a
constant-time comparison would imply an authority it does not have.

A **mismatched** hash is refused (`INVALID_TEXT`): the sender is broken or is
trying something, and either way the `(hash, content)` pair it asked us to
remember would poison de-duplication. An **absent** hash is accepted, because
the field is optional and a minimal peer should still interoperate.

### `ClipboardOutcome`

Every value is safe to send to a peer. It says what happened in a vocabulary
the sender can act on, and nothing about the receiver's local state — no
exception text, no path, and never any part of the clipboard content,
including content the receiver already held.

| Outcome | Meaning |
| --- | --- |
| `APPLIED` | Written to the receiver's system clipboard |
| `PENDING_USER` | Accepted, held in memory, waiting for a human to apply it |
| `DUPLICATE` | This `event_id` was already handled; nothing happened twice |
| `NOT_AUTHORIZED` | No `clipboard.v1` grant here |
| `REJECTED_POLICY` | Granted, but this direction is off for this peer |
| `REJECTED_SENSITIVE` | The receiver declines clips marked sensitive |
| `TOO_LARGE` | Past the ceiling; use `files.v1` for bulk content |
| `INVALID_TEXT` | Malformed, NUL-bearing, or hash mismatch |
| `FAILED` | The receiver's clipboard backend refused or was unavailable |

## Limits

| Limit | Value | Why |
| --- | --- | --- |
| `MAX_CLIPBOARD_TEXT_BYTES` | 32 KiB | See below |
| `EVENT_ID_LEN` | 16 bytes | 128 bits of CSPRNG output |
| `CONTENT_HASH_LEN` | 32 bytes | SHA-256 |
| `EVENT_CACHE_ENTRIES` / TTL | 256 / 5 min | De-duplication, bounded twice over |
| `SUPPRESSION_ENTRIES` / TTL | 64 / 10 s | Loop suppression, short-lived on purpose |
| `PENDING_CLIP_TTL` | 5 min | Held clips do not outlive the reason they exist |
| `BACKEND_TIMEOUT` | 5 s | A locked GNOME session blocks `wl-copy` for ever |

### Why 32 KiB, and why it is not the frame limit

`clipboard.v1` is control-plane data: an update travels inside an `Envelope`,
so it is bounded by `MAX_FRAME_LEN` (64 KiB) whether we say so or not. Setting
the text ceiling *at* the frame limit would mean a legal-by-our-rules clip
could not be framed at all, because the envelope also carries a 16-byte message
id, a 32-byte hash, an event id, a device id, protobuf tags and length
prefixes.

32 KiB leaves roughly 32 KiB of headroom — far more than the few hundred bytes
of overhead actually needed. The margin is deliberate: it should absorb a
future field without becoming a wire-format change.

It is also generous for what a clipboard *is*: roughly 32,000 ASCII characters
or 8,000 CJK characters — a long shell command, a stack trace, a whole config
file. Content past that is a document, and a document is what `files.v1` is
for, which is exactly what `TOO_LARGE` tells the sender.

Raising it is not free: it would have to be raised on both platforms at once,
and it must never approach `MAX_FRAME_LEN`.

**Oversized content is never truncated.** A truncated password, key or command
is not a degraded version of the original — it is a different, wrong value that
looks plausible.

## UTF-8 and NUL

Accepted: any non-empty valid UTF-8 within the size limit and free of U+0000.
Nothing is normalised, trimmed or re-encoded. ASCII, accented Latin (`olá`,
`ñandú`), CJK, emoji including flags and ZWJ sequences, tabs, CR, LF, CRLF and
significant leading/trailing whitespace all pass through byte-for-byte. A
clipboard that "helpfully" rewrote content would corrupt exactly the payloads a
user is least able to notice.

**NUL (U+0000) is refused** on both platforms, and this is a decision rather
than a consequence — U+0000 is valid UTF-8 and valid in both a Rust `String`
and a Kotlin `String`. It is refused because it cannot be carried *faithfully*
end to end: the X11 `UTF8_STRING` and `text/plain` conventions do not carry it,
and any consumer along the path that touches a C string API truncates at the
first NUL, silently. Silent truncation is the one failure a clipboard must
never have, and refusing is the only behaviour that is identical on both
platforms and visible to the user.

Empty text is refused too: it carries no information, and accepting it would
hand a peer a way to blank the local clipboard.

The rules live in exactly two places — `text.rs` and `ClipboardText.kt` — and
both suites pin the same vectors, including the SHA-256 of `"hello"`.

## Grant and policy: two different questions

```text
              clipboard.v1 grant          ← never automatic (ADR-0008)
                     │
       ┌─────────────┼─────────────┐
       │             │             │
  allow_send   allow_receive   (per peer, local only)
       │             │
   auto_send    auto_receive
```

* The **grant** lives in the trust store next to `files.v1`'s and answers "may
  this device speak clipboard with me at all?". It is what capability
  negotiation filters on. It is **not** in `auto_grant`: a device that can
  write your clipboard can also see what you paste next.
* The **policy** answers "and in which directions, and how automatically?".

Collapsing them would mean either a grant that quietly enables automatic
two-way sync, or four capability ids. Neither is right.

### Defaults

| Flag | Default | Why |
| --- | --- | --- |
| `allow_send` | **on** | Manual send is what the explicit grant was for |
| `allow_receive` | **on** | Manual receive is what the explicit grant was for |
| `auto_send` | **off** | Everything you copy would leave the machine |
| `auto_receive` | **off** | A peer could replace what you are about to paste |

Reaching these defaults already required `omnibridge grant <device>
clipboard.v1`, which is never automatic — so the two `allow_*` flags are what
the person just asked for, not a silent widening. The two `auto_*` flags are
the ones that must never turn themselves on, and they do not.

Concretely, with a fresh grant and nothing else:

* copying locally sends nothing anywhere;
* a peer's update is accepted, held in memory and reported `PENDING_USER` — it
  does not touch the system clipboard;
* `omnibridge clipboard send <device>` works, because a human asked.

`auto_send` and `auto_receive` are each *contained* by their direction:
`may_auto_send() == allow_send && auto_send`. Turning a direction off cannot be
defeated by a stale automatic flag.

### A peer can never set its own policy

Every flag is decided and stored locally. There is no protocol message that
changes any of them — the schema has no such message, so the rule is an
absence rather than a check that could be inverted.

### Widening needs a reconnect; narrowing does not

This asymmetry is the foundation's, not this capability's:

* the **effective** capability set is fixed at handshake time, so *adding*
  `clipboard.v1` to it requires a new handshake;
* every inbound message is re-checked against the trust store, so *removing*
  it bites at once — including on a session that is already established, and
  including for the outbound watcher, which is told to re-read its peer list.

The dangerous half is the one that is immediate.

## Loop prevention

The loop that must not happen:

```text
  desktop clipboard changes
       │
       ▼  watcher  ──────────► CLIPBOARD_UPDATE ──────► Android
                                                            │
                                              setPrimaryClip
                                                            │
                                     Android's watcher fires
                                                            │
       ◄─────────────────── CLIPBOARD_UPDATE ◄──────────────┘
       │
  write to desktop clipboard →  watcher fires again  →  for ever
```

Two independent mechanisms break it, and both are needed.

**`EventCache`** makes *the same event* idempotent: a retry, a duplicate or a
replay of one update is not applied or answered twice. It is global rather than
per-peer, so replaying one peer's event id through another peer is also caught.
Bounded by count *and* by age.

**`SuppressionCache`** makes *a locally applied remote clip* invisible to the
local watcher. Applying a remote clip arms an entry keyed on the content hash
**before** the write — on a healthy desktop the change notification can arrive
while `write_text` is still returning.

The entry is **single-use and short-lived**, and that is what makes it
different from `if new_text == old_text`:

* a remote clip applied locally suppresses **exactly one** subsequent watcher
  event — the one it caused;
* copying that same text again by hand afterwards is a new event and is sent
  normally;
* an entry whose watcher event never arrives expires in 10 seconds rather than
  sitting there swallowing a later copy;
* a *failed* apply releases the entry immediately, because no echo is coming.

A naive content comparison is wrong in both directions: it suppresses a
legitimate re-copy, and it fails to suppress a genuine loop where two peers
happen to produce equal content from different events.

## No relay

A clip received from peer A is never forwarded to peer B.

This is enforced by **absence**: there is no code path anywhere that takes a
`ClipboardUpdate` from one peer and sends it to another. The only function that
turns a clipboard change into outbound traffic is `on_local_change`, and its
input is the *local clipboard*, never an inbound message.

What could still create a relay is the loop *through* the clipboard — A's clip
is written locally, the watcher sees a change, and it goes out to B. That is
what the suppression cache stops, and the check happens before any peer is
considered.

## Ordering

`timestamp_unix_ms` is informational only and is never an input to an
authorization, expiry, de-duplication or ordering decision. Within a session,
ordering is the protocol's. Across peers, **v1 is last-accepted-wins**: the
most recent update that passes every check is the one on the clipboard. That is
an explicit decision, not an accident — a distributed clock would be a large
mechanism for a problem ("two people copied something at the same moment on
two devices") that resolves itself the next time either of them copies.

## Persistence

**Clipboard content is never written to disk.** Not to `state.json`, not to the
trust store, not to a log, not to SQLite, not to preferences, not to transfer
history, not to telemetry, not to a crash report.

What *is* persisted is the per-peer policy — four booleans — which is settings,
not content.

The two caches hold ids, hashes and monotonic timestamps. A held
(`PENDING_USER`) clip is the only clipboard text that lives longer than one
call, it lives in memory only, it is dropped when the session ends, and it
expires after five minutes regardless.

Monotonic clocks throughout (`tokio::time::Instant`,
`SystemClock.elapsedRealtime`): expiry must not be steerable by a clock change.

## Logging

Clipboard content never reaches a log, at any level. What is logged instead is
the peer's short fingerprint, the event-id prefix, the content-hash prefix, the
byte count and the outcome — enough to correlate two devices' logs, useless to
anyone reading one.

This is enforced by three things rather than by discipline:

* `ClipboardText` has a **hand-written** `Debug`/`toString` that prints
  `bytes` and a hash prefix. A `derive`d one would put content into every
  `tracing` field using `?value`, every panic message and every test failure.
* All diagnostics go through `redact.rs` / `Redact.kt`.
* `tests/logging.rs` runs full flows — applied, sensitive, duplicate,
  oversized, invalid, held-then-applied, backend failure — under a `tracing`
  subscriber capturing at **TRACE**, with canary strings, and asserts the
  canaries never appear. `RUST_LOG=trace` is what someone runs when something
  is wrong, and that is the worst moment to spill a password into a file they
  are about to attach to a bug report.

## The Linux clipboard backend

```text
ClipboardBackend
    read_text()      → Option<ClipboardText>
    write_text(text, sensitive)
    watch_changes()  → a change *signal*, not content
```

Everything that knows about Wayland or X11 lives under `backend/`. Above it the
capability deals in `ClipboardText` and would work unchanged on another
platform.

### The watch is a signal, not content

`ClipboardWatch` yields `()`, and the manager then calls `read_text()` itself.
That extra step is load-bearing three times:

* the two available change sources on Linux report different things — an XFIXES
  `SelectionNotify` carries no data at all — so a content-carrying watch would
  have to fabricate one of them;
* clipboard content never flows through the notification plumbing, so there is
  one place that reads it and one place to audit;
* a burst of changes collapses naturally, because reading after the fact yields
  the clipboard's *current* state rather than a queue of stale ones.

### Read and write: `wl-clipboard`

`wl-copy` and `wl-paste`, the standard implementation, packaged everywhere.
Reading and writing a Wayland clipboard is not a function call — a client that
*offers* the clipboard must stay alive to serve every paste request that
follows — and running a Wayland event loop inside a headless daemon to
reimplement that would buy nothing.

Two properties of these tools shape every call, and both were found by running
them rather than by reading them:

* **`wl-copy` daemonises and inherits its parent's stdio.** A parent that hands
  it a pipe and then waits for EOF waits for ever, because the forked child
  holds the write end open for as long as it owns the clipboard. Every spawn
  gives it `/dev/null` for stdout and stderr.
* **These tools block, rather than fail, when the compositor will not serve
  them.** On a **locked** GNOME session `wl-copy` and `wl-paste` wait
  indefinitely for a seat that is never granted. Every invocation is wrapped in
  `BACKEND_TIMEOUT` with `kill_on_drop`, so a locked screen costs one killed
  child rather than one leaked child per attempt, for ever.

Nothing builds a shell command. The text goes to the child's **stdin** and
never appears in an `argv`, so there is no quoting to get wrong, nothing for a
`$(…)` in a clipboard to expand into, and no clipboard content in
`/proc/<pid>/cmdline` where any process on the machine could read it.

**Dependency:** the `wl-clipboard` package, which is what it is called on
Fedora, Ubuntu and Debian alike. Its absence is detected once at startup and
reported by `omnibridge clipboard status`, rather than failing at the first use.
The runtime message names the missing binaries and the package and stops
there: OmniBridge does not know which package manager the machine has, and a
wrong guess is worse than none. Per-distribution install commands live in
[the README](../../README.md#running-on-linux), where they can be correct.

### Sensitive marking is a separate capability from the clipboard itself

`wl-copy --sensitive` asks clipboard managers to keep a clip out of their
history. It is a **distinct capability** from having a working clipboard, and
the difference is not hypothetical:

| Distribution | `wl-clipboard` | ordinary clipboard | `--sensitive` |
| --- | --- | --- | --- |
| Fedora 44 | `2.2.1^git20251124` | yes | **yes** — a post-2.2.1 snapshot carrying the flag |
| Ubuntu 24.04 LTS | `2.2.1-1build1` | yes | **no** |
| Ubuntu 26.04 LTS | `2.2.1-2build1` | yes | **no** |
| Debian 13 trixie | `2.2.1-2` | yes | **no** |

Note the second column: all four print the identical string `wl-clipboard
2.2.1`, and they do not behave identically. **That is why OmniBridge probes
`wl-copy --help` for the option rather than parsing `--version`** — a `>= 2.3`
version test would reject Fedora's working build and accept the three that
cannot do it. The version number is offered to users as guidance for choosing
a build; it is never the test.

Where the capability is absent, a clip the phone marked sensitive is
**refused** rather than written unmarked (PLAT-DEC-013): an unmarked password
persisted in a clipboard-history manager without the user being told is worse
than a visible failure. Ordinary clipboard sharing is untouched. Because the
refusal only happens at the moment somebody copies a password — the worst
possible moment to learn about it — the state is reported up front and
separately from the backend's own availability, by both
`omnibridge clipboard status` and the GUI's clipboard page:

```text
  ordinary clipboard   available
  sensitive clipboard  unavailable
                       this system's wl-copy does not support sensitive
                       clipboard marking. …
```

### Watching: XFIXES, because GNOME has no data-control

`wl-paste --watch` needs the compositor to implement
`zwlr_data_control_manager_v1` or `ext_data_control_manager_v1`. **Mutter
implements neither**, so on the desktop this Sprint certifies, the native
Wayland answer to "tell me when the clipboard changes" does not exist:

```console
$ wl-paste --type text/plain --watch /usr/bin/echo
Watch mode requires a compositor that supports the data-control protocol
```

What does exist is Xwayland. Mutter mirrors the Wayland clipboard onto the X11
`CLIPBOARD` selection so X11 applications can paste, and taking ownership of an
X11 selection generates an XFIXES `SelectionNotify` to every client that asked
for one. That is a supported, public X11 mechanism, it fires for changes made
by *Wayland-native* applications, and it costs one idle socket and no polling.

Only the `CLIPBOARD` atom is selected for. `PRIMARY` — the selection that
fills merely by dragging the mouse over text — is never watched, because
synchronising it would transmit text the user never asked to copy.

Backend detection resolves, once, at startup:

1. `wl-paste --watch` if data-control is available (sway, Hyprland, KWin);
2. otherwise XFIXES on Xwayland (GNOME);
3. otherwise nothing — reported honestly, with `auto_send` degrading to manual
   sending. **No polling fallback**, ever.

See [ADR-0014](../adr/ADR-0014-clipboard-change-notification.md).

### The watcher is supervised

* It **starts only when needed**: with no `auto_send` peer there is no watcher,
  no child process and no X connection.
* It **stops when the last one goes**: a revocation bumps a policy epoch, the
  loop re-reads and the watch is dropped.
* It **restarts if the backend dies**, with exponential backoff from 1 s to
  60 s, so a compositor restart is survivable and a permanently broken backend
  costs one attempt a minute rather than a spin.
* It **never polls**.
* It **cannot kill the daemon**: everything it touches is a handled `Result`.

### X11

The XFIXES watcher is already the seed of a native X11 backend. Read and write
for a pure X11 session are **not implemented** in this Sprint; a session with
`DISPLAY` but no `WAYLAND_DISPLAY` gets an `Unsupported` backend that says
exactly that, rather than failing at the first call with a worse message.

## The Android limitation

`ClipboardCapabilities.AUTO_SEND_SUPPORTED = false`, as a constant the UI, the
tests and this document all read.

Since Android 10 (API 29), `getPrimaryClip` returns null unless the calling app
has input focus or is the default IME. OmniBridge is a normal app. It does **not**:

* declare an `AccessibilityService`;
* ask to become the default IME;
* hold `READ_LOGS`, `SYSTEM_ALERT_WINDOW` or any hidden permission;
* use reflection against private APIs;
* open an invisible activity to steal focus;
* poll in the background hoping to catch a moment of focus;
* require root, Magisk, or ADB at runtime.

So there is no supported way to observe the clipboard in the background, and
the app says so instead of offering a toggle that would quietly do nothing.

**Writing is not restricted the same way.** `setPrimaryClip` has no focus
requirement, which is precisely what makes automatic desktop → Android sync
possible while automatic Android → desktop sync is not.

### Where a read happens

Every clipboard read originates from a button on a **resumed Activity**:

| Entry point | Reads the clipboard? | Notes |
| --- | --- | --- |
| **Send clipboard** button | Yes | The Activity has focus |
| Quick Settings tile | No | Opens the Activity, which then reads |
| Sharesheet (`text/plain`) | **No** | The text arrives in `EXTRA_TEXT` |
| Connection service | Never | It has no window and must not try |

The Sharesheet case is worth stating: a `text/plain` share carries its text in
the intent, so it never touches the system clipboard and the focus rule does
not apply. It is the one path where text reaches a computer without the person
first copying it. An intent carries no `EXTRA_IS_SENSITIVE`, so shared text is
sent **without** the sensitive hint — honest rather than convenient, since
guessing from the content would be exactly the heuristic password detector this
project refuses to build.

### Receiving on Android

`setPrimaryClip` with two `ClipDescription` extras, both **presentation
hints** that nothing depends on:

* `EXTRA_IS_REMOTE_DEVICE` (API 34+) — tells the system the clip came from
  another device, which suppresses the "copied" toast for something the person
  did not copy;
* `EXTRA_IS_SENSITIVE` (API 33+), when the update is marked sensitive — asks
  the system to hide the preview and asks clipboard managers not to keep it.

Neither is a cryptographic mechanism or an ACL, and neither is ever an
authorization input.

With `auto_receive` **off** (the default) the clip is held in memory and
offered through a notification titled *"Clipboard received from &lt;computer&gt;"*
with a **Copy** action. The notification carries the computer's name, the byte
count and whether the clip was marked sensitive — **never the text**, in any
form: a notification is shown on the lock screen and is readable by any
notification listener the user has installed. The action opens the app rather
than applying in place, so the person can see where the clip came from before
it lands on their clipboard.

## Sensitive clipboards

Android → desktop already requires a deliberate tap. When the platform marks a
clip `EXTRA_IS_SENSITIVE`, OmniBridge asks **again**, naming the destination:

```text
This clipboard is marked sensitive
The app you copied from marked this text as sensitive — a password, a
recovery code, or similar.
Send 12 bytes to lima?
                                              [Cancel]  [Send]
```

The dialog shows the size and the destination, never a preview: rendering the
text would put a password on a screen that is also visible over the shoulder,
and it buys nothing, since the person just copied it.

**The desktop has no equivalent signal.** Wayland offers no reliable way to ask
whether a clip is sensitive, and inventing a heuristic password detector would
be a guess dressed up as a security control. So:

* desktop → Android auto-sync is opt-in per device, with the consequence
  stated in the CLI help: *everything you copy* goes to that device;
* `omnibridge clipboard send <device> --sensitive` lets a person mark one clip by
  hand, which sets `EXTRA_IS_SENSITIVE` on Android;
* applying a sensitive clip on the desktop uses `wl-copy --sensitive`, so
  desktop clipboard managers skip it.

## Clear behaviour

The local clipboard is **not** cleared after syncing. Android has its own
expiry policy in modern versions and there is no requirement to replace it. A
future `clear after N minutes` is possible; it is not v1.

## CLI

```console
omnibridge clipboard status
omnibridge clipboard send <device> [--sensitive]
omnibridge clipboard apply <device>
omnibridge clipboard allow <device> send|receive on|off
omnibridge clipboard auto-send <device> on|off
omnibridge clipboard auto-receive <device> on|off
```

A device is named by its device id or by an unambiguous fingerprint prefix of
at least 8 characters. **An ambiguous prefix is an error, never a guess** —
sending a password to the wrong device because a prefix matched two of them is
not a failure mode worth having.

`omnibridge clipboard status` keeps three facts visibly separate, because
collapsing them is how a person comes to believe sync is running when it is
not:

* whether `clipboard.v1` is **granted**;
* what the per-direction **policy** permits;
* whether this **session can technically do it** — on GNOME, `auto-send` is
  reported as *NOT supported here*, and a peer with `auto_send` on gets an
  explicit warning that nothing is being pushed.

It reports cache sizes so bounded growth is observable rather than merely
claimed, and it describes a held clip by size, hash prefix, origin and age —
**never a preview**. The control socket never carries clipboard content.

## Android UI

On each computer's card:

* **Share clipboard with this computer** — the `clipboard.v1` grant;
* **Receive clipboard from this computer** — `allow_receive`;
* **Apply received clipboard automatically** — `auto_receive`, with a line
  saying what each state means;
* **Send clipboard to this computer** — `allow_send`;
* **Send clipboard** — the button, enabled only when granted, allowed and
  connected;
* the reason auto-send is absent, rather than a toggle that would do nothing.

The screen observes `TrustStore.peersFlow` and the sync's `pendingClips` /
`lastOutcome` flows. The previous release read the peer list once during
composition and never again, so a grant changed elsewhere stayed invisible;
every mutator now republishes.

## Quick Settings tile

A `TileService` has no input focus — the panel belongs to System UI — so
`getPrimaryClip` returns null inside `onClick`. The tile therefore does the one
supported thing: it brings OmniBridge to the foreground with an explicit
`ACTION_SEND_CLIPBOARD`, and the Activity, which does have focus, reads the
clipboard and shows the destination.

The tile resolves its target **without guessing**: with exactly one eligible
computer the send starts, and with none or several the screen simply opens so
the person picks — the same rule the CLI follows for an ambiguous prefix. It
reports `STATE_UNAVAILABLE` when nothing is set up, rather than looking ready.

`startActivityAndCollapse(Intent)` throws on API 34+; both the `Intent` and
`PendingIntent` forms are handled, because this app supports API 29 upward.

## Testing

| Suite | What it proves |
| --- | --- |
| `capabilities/clipboard/src/*` unit tests | Text rules, policy, caches, backend detection |
| `capabilities/clipboard/tests/security.rs` | CLIP-SEC-01 … 18 against the manager |
| `capabilities/clipboard/tests/loops.rs` | Convergence, with a hard budget on messages delivered |
| `capabilities/clipboard/tests/logging.rs` | No content in logs, captured at TRACE |
| `daemon/tests/clipboard.rs` | The whole capability over real TLS, real pinning, real trust store |
| Android `Clipboard*Test` (JVM) | The same rules, against a fake store |
| Android `ClipboardInstrumentedTest` | The **real** `ClipboardManager` on a device |

The loop suite is worth a note. It wires real managers together through their
session channels and delivers messages until a full sweep produces nothing. A
converging system runs dry in two or three sweeps; a looping one never does —
so the budget is checked on **every delivery**, not per sweep. A loop
amplifies, so a per-sweep check would let one sweep grow to billions of
messages before it was tested. A failure is a count, not a hang.

The Android JVM suite uses a fake `ClipboardTarget` for the *storage* only. It
makes no claim about how Android behaves: every assertion about
`getPrimaryClip`, `setPrimaryClip`, `EXTRA_IS_SENSITIVE` and
`EXTRA_IS_REMOTE_DEVICE` lives in the instrumented suite, on a real device,
because a mock that agreed with our assumptions would prove only that we are
self-consistent.
