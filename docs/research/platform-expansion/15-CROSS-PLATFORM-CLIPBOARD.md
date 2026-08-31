# 15 — Cross-platform clipboard

| Field | Value |
| --- | --- |
| **Title** | One `clipboard.v1`, six backends |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | `ClipboardBackend` per platform, the polling question, loop suppression, sensitive hints, capability metadata. |
| **Decision status** | PROPOSED. **PLAT-DEC-006** (capability metadata) and **PLAT-DEC-009** (polling) OPEN. |
| **Evidence** | REPO VERIFIED for the existing design; OFFICIAL DOC VERIFIED for platform APIs; POC REQUIRED where marked. |
| **Related documents** | [03](03-PLATFORM-CAPABILITY-MATRIX.md), [06](06-KDE-PLASMA-WAYLAND.md), [08](08-WINDOWS-FEASIBILITY.md), [10](10-MACOS-FEASIBILITY.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [../../architecture/CLIPBOARD.md](../../architecture/CLIPBOARD.md), [ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md) |

---

## 1. The good news first

The existing abstraction is right. `capabilities/clipboard/src/backend/mod.rs` already says so
in its own module documentation:

> *"Everything that knows about Wayland, X11 or an external helper lives under this module.
> Above it, the capability deals in `ClipboardText` and a change signal, and would work
> unchanged against a macOS or Windows implementation."*

That claim survives scrutiny. The trait is five methods:

```rust
#[async_trait::async_trait]
pub trait ClipboardBackend: Send + Sync {
    fn id(&self) -> &'static str;
    async fn read_text(&self) -> BackendResult<Option<ClipboardText>>;
    async fn write_text(&self, text: &ClipboardText, sensitive: bool) -> BackendResult<()>;
    fn watch_changes(&self) -> BackendResult<ClipboardWatch>;
    fn watch_availability(&self) -> std::result::Result<(), String>;
    fn describe(&self) -> String;
}
```

and its two most important design decisions turn out to be *more* right cross-platform than
they were on Linux:

**The watch yields `()`, not content.** `ClipboardWatch.changes` is an `mpsc::Receiver<()>`;
the manager then calls `read_text()` itself. On Linux the justification was that XFIXES
`SelectionNotify` carries no data, so a content-carrying watch would have to fabricate it.
Cross-platform it is better still: `WM_CLIPBOARDUPDATE` also carries nothing, and macOS's
`changeCount` is a bare integer. **Every platform's change signal is content-free.** A trait
that had been designed around "the watch gives you the new text" would need reworking for all
three.

**`Err(Unavailable)` is a normal answer.** The capability registers, tells peers `FAILED`
honestly, and reports why locally. That is exactly the behaviour iOS needs, and macOS may need,
and a Linux X11 session needs today.

**Conclusion: keep the trait. Amend one sentence of its contract (§3). Add backends.**

---

## 2. Backends per platform

| Platform | read | write | watch | Sensitive hint | Status |
| --- | --- | --- | --- | --- | --- |
| **Linux Wayland (GNOME)** | `wl-paste` | `wl-copy` | **XFIXES** on Xwayland `CLIPBOARD` | `wl-copy --sensitive` | REPO VERIFIED, shipping |
| **Linux Wayland (KDE etc.)** | `wl-paste` | `wl-copy` | `wl-paste --watch` (data-control) | `wl-copy --sensitive` (Klipper honours it) | Code path exists, **POC-KDE-01** |
| **Linux X11** | ✗ not implemented | ✗ | XFIXES (exists) | — | Gap; **LINUX-006** |
| **Windows** | `GetClipboardData(CF_UNICODETEXT)` | `SetClipboardData` | **`AddClipboardFormatListener` → `WM_CLIPBOARDUPDATE`** | "exclude from clipboard history" formats | POC-WIN-05 |
| **macOS** | `NSPasteboard.string(forType:)` | `clearContents` + `setString` | **`changeCount` polling** | `org.nspasteboard.ConcealedType` (convention) | POC-MAC-05 |
| **Android** | `ClipboardManager` (needs focus) | `setPrimaryClip` | ✗ platform-limited | `EXTRA_IS_SENSITIVE` | REPO VERIFIED, shipping |
| **iOS/iPadOS** | `UIPasteControl` / paste gesture | `UIPasteboard.general.string` | ✗ **platform restriction** | — | POC-IOS-07 |

Notice that Windows is the only platform with a **first-class, event-driven, no-caveat** watch.
Linux needs a compositor protocol or an X11 bridge; macOS must poll; Android and iOS cannot.

---

## 3. The polling amendment

The contract currently states:

> *"`Err(Unavailable)` is a normal answer, not a failure … **No implementation may satisfy this
> by polling.**"*

That rule is correct on Linux, where two event sources exist and polling would be laziness. On
macOS it is unsatisfiable: `NSPasteboard.changeCount` is the documented change-detection
mechanism and AppKit publishes no change notification.

**Proposed amendment (documentation only, in `backend/mod.rs`):**

> No implementation may satisfy this by polling **where the platform offers an event-driven
> source.** Where none exists, a backend may poll, and must then:
> * report the interval in `describe()`, so `anyflow clipboard status` tells the truth;
> * poll only the platform's cheap change *counter*, never the clipboard content;
> * poll only while at least one peer has `auto_send` enabled;
> * stop polling when the last such watch is dropped.

The manager already drops the `ClipboardWatch` when no peer wants auto-send — *"a machine with
no auto-send peer runs no watcher at all"* — so the last two conditions are satisfied by the
existing lifecycle. The change is genuinely a doc-comment plus a macOS implementation.

**Cost of the alternative** — refusing macOS auto-send entirely — is losing the feature AnyFlow
is most identified with on a platform where it works fine. **PLAT-DEC-009, recommended
direction: amend.**

Interval: `changeCount` is a cheap integer read that touches no content. 250–500 ms is the
usual practice. Under App Nap, timers coalesce and the effective interval will be longer, which
is acceptable — a clipboard sync that arrives 2 seconds late is fine; one that never arrives is
not.

---

## 4. Loop suppression across platforms

This is where a naive port breaks, so it is worth being precise about why the current design
survives.

`clipboard.v1` writes to the local clipboard. That write fires the local watch. If the watch
sent it back, two devices would ping-pong forever.

AnyFlow suppresses this **by content**, not by sequence, in
`capabilities/clipboard/src/dedup.rs` and the manager: an inbound update carries a random
16-byte `event_id` and a `content_hash` (SHA-256 over the UTF-8 bytes), and a locally-observed
clipboard whose hash matches a recently-applied one is not re-sent.

Why content-based matters here:

| Platform | Self-write fires our own watch? | Consequence |
| --- | --- | --- |
| Linux Wayland | `wl-copy` changes the selection → yes | Already handled |
| Windows | `SetClipboardData` → **yes**, `WM_CLIPBOARDUPDATE` fires | Handled by the same logic |
| macOS | `setString` increments `changeCount` → **yes** | Handled by the same logic |
| Android | `setPrimaryClip` → yes | Already handled |

A *sequence*-based suppression ("ignore the next N events after a write") would have needed
per-platform tuning and would still race. The content-hash approach needs none.

**But it must be tested per platform, not assumed.** `capabilities/clipboard/tests/loops.rs`
already exists as the Linux test; each new backend should be added to it, or an equivalent
written. **POC-WIN-05** and **POC-MAC-05** both include a loop test for this reason.

One genuine cross-platform hazard: the `origin_device_id` field. `clipboard_v1.proto` documents
it as *"how a receiver can tell a first-hand clipboard event from one that has already
travelled"* and explicitly says it is **not** identity and never an authorization input. With
three or more paired devices, a clip could otherwise circulate. Any new implementation must set
and honour it — it is easy to omit and the failure mode (a slow loop between three devices)
would be confusing.

---

## 5. Text rules are already portable

`capabilities/clipboard/src/text.rs` (272 lines) validates encoding, rejects oversize content
(with `TOO_LARGE` rather than truncation — *"a silently shortened password is worse than a
refused one"*), and normalises. `limits.rs` holds the ceilings.

Nothing in it is platform-specific except one thing worth naming: **line endings.** Windows
clipboards conventionally carry CRLF; Linux and macOS carry LF. A clip copied on Windows and
pasted on Linux arriving with `\r\n` is a real, visible defect.

Options:
- (a) Normalise to LF on send, restore CRLF on write on Windows. Loses fidelity for text that
  genuinely contains `\r`.
- (b) Send verbatim; let each platform's paste target deal with it.
- (c) Normalise on *write* only, per platform, and never touch what is on the wire.

**Recommendation: (c).** The protocol stays byte-transparent (which matters — the hash is over
the bytes, and dedup depends on it), and the Windows backend converts LF→CRLF on write and
CRLF→LF on read. That keeps the normalisation at the platform boundary, which is where the
trait is. **WIN-009.** Note this interacts with `content_hash`: normalisation must happen
*inside* the backend, after hashing on send and before hashing on receive, or dedup will
mismatch. Worth an explicit test.

---

## 6. Receive when the device cannot apply

`ClipboardOutcome` already has `PENDING_USER`:

> *"Accepted, but the receiver's policy needs a human to apply it. The clip is held in memory,
> briefly, and never written to disk."*

and `ClipboardApply { device }` exists on the control socket to apply it later. This was
designed for a Linux user with `auto_receive` off. **It is exactly the mechanism iOS needs** —
a clip that arrives while the app is foreground but auto-apply is not desired, or a clip queued
by the desktop for the next time the phone appears.

**No protocol change is needed for the iOS clipboard model.** That is a significant finding: the
most constrained platform fits the existing capability because the capability was designed
around consent rather than around automation.

One extension worth considering, on the **desktop** side and needing no protocol change: let
the desktop hold the most recent clip for a peer that is *not currently connected*, and offer it
on reconnect. This makes the iOS foreground-only model feel intentional. It is local state, it
must be bounded and in-memory only (never on disk — the existing rule), and it must expire.
**UX-008.**

---

## 7. Capability metadata — the deferred protocol question

### The problem

`clipboard.v1` is all-or-nothing. `core/src/capability.rs`: *"There is no negotiation of a
version within a capability: two ids either match or they don't."*

So an iOS peer advertising `clipboard.v1` is indistinguishable, to a Linux peer's UI, from
another Linux box. The user will enable `auto_send` for it and nothing will ever arrive.

### Options

| Option | Mechanism | Assessment |
| --- | --- | --- |
| **(a)** Capability metadata in `HELLO` | New field, e.g. `map<string, CapabilityProperties>` alongside `repeated string capabilities` | Correct in principle; a real protocol change |
| **(b)** Infer from `DeviceInfo.platform` | No protocol change | **Rejected.** `platform` is attacker-supplied and is currently used only for an icon. Making it an input to behaviour turns a cosmetic field into a semi-security one |
| **(c)** UI-only inference | Do not offer `auto_send` for a peer that has never sent an automatic clip | No protocol change; slightly magical; degrades gracefully |
| **(d)** Do nothing | The toggle exists and does nothing on some peers | Poor, but survivable until iOS exists |

### Recommendation

**Defer. Do not implement now. Design (a) so it is ready, and ship (c) as the interim.**

If (a) is ever implemented, the obligations are non-negotiable:

- **Backward compatibility.** proto3 additive field; an old peer ignores it. An old peer must
  keep working exactly as today.
- **Fail-closed.** Absent metadata must mean *"assume nothing extra"*, never *"assume
  everything"*. A peer that omits `can_receive_auto` must be treated as not supporting it —
  which is the safe direction and also the one that makes old peers behave as they do now.
- **Not an authorization input.** Metadata is a *self-report*, exactly like `sensitive_hint` and
  `origin_device_id`. It may shape UI and may suppress pointless traffic. It must never widen
  what a peer is allowed to do. The grant model in `store.rs::TrustedPeer::allows` stays the
  only authority.
- **Versioning.** If it changes the meaning of `HELLO`, `PROTOCOL_VERSION_MAX` moves, and
  `TrustedPeer::last_protocol_version` — which exists *"so a future release can detect and
  refuse a silent downgrade"* — becomes relevant for the first time.
- **Capability ids stay stable.** `clipboard.v1` remains `clipboard.v1`. Metadata is not a
  version bump.

Candidate fields, for design only:

```protobuf
// PROPOSED. NOT FOR IMPLEMENTATION IN THIS SPRINT.
message CapabilityProperties {
  bool can_send_manual    = 1;
  bool can_send_auto      = 2;
  bool can_receive        = 3;
  bool can_auto_apply     = 4;
  bool background_receive = 5;
}
```

**PLAT-DEC-006.** Status OPEN. Blocking? Only for iOS, and only for polish rather than
function.

---

## 8. What must not become platform-specific

Stated because it is the brief's compatibility principle and the temptation is real:

**There must be no `clipboard.windows.v1`, `clipboard.macos.v1` or `clipboard.ios.v1`.**

Every platform difference found in this research is expressible as:
- a different `ClipboardBackend` implementation (an internal detail no peer sees), or
- an existing `ClipboardOutcome` value (`PENDING_USER`, `FAILED`, `REJECTED_POLICY`), or
- local per-peer policy in `ClipboardPolicy` (already per-peer, already local, already
  `#[serde(default)]`-safe), or
- optional, additive, fail-closed metadata (§7, deferred).

None of it requires a new capability id. `clipboard.v1` as designed already carries the whole
cross-platform product.

---

## 9. Sensitive hint per platform

`sensitive_hint` is defined as *"a HINT, in both directions … not an ACL and not a
cryptographic property"*.

| Platform | Mechanism | Real effect |
| --- | --- | --- |
| Linux Wayland | `wl-copy --sensitive` | Clipboard managers skip history. **On KDE, Klipper actually honours it** |
| Windows | Register "ExcludeClipboardContentFromMonitorProcessing" / "CanIncludeInClipboardHistory" formats | Skips Win+V history and Cloud Clipboard. **EXTERNAL VERIFICATION REQUIRED** on exact format names |
| macOS | `org.nspasteboard.ConcealedType` | Community convention honoured by several managers; **not** an Apple API |
| Android | `ClipDescription.EXTRA_IS_SENSITIVE` | The system hides the clip preview |
| iOS | — | No equivalent found |

Windows is the platform where the hint matters most, because **Cloud Clipboard can upload a
clip to Microsoft's servers**. For a product whose first principle is local-first, a received
password ending up in a cloud clipboard is a bad outcome — even though it is the user's own
setting and their own account. Setting the exclusion formats is not optional politeness on
Windows; it is the difference between honouring the product's promise and not.
**WIN-008**, and recorded in [20](20-SECURITY-THREAT-ANALYSIS.md).

---

## 10. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **ARCH-005** | Amend the `ClipboardBackend` polling contract | Wave 0, low effort |
| **WIN-003** | Windows `ClipboardBackend` | High |
| **WIN-008** | Sensitive-clip exclusion formats | Medium |
| **WIN-009** | CRLF/LF normalisation at the backend boundary, hash-safe | Medium |
| **MAC-005** | macOS `ClipboardBackend` with declared polling | High |
| **IOS-003** | `UIPasteControl` send + foreground receive | Medium |
| **LINUX-006** | X11 read/write backend | Medium |
| **KDE-001** | Name the missing data-control protocol in `clipboard status` | Low |
| **UX-008** | Desktop holds the latest clip for a disconnected peer | Medium |
| **PROTO-001** | Design (not implement) `CapabilityProperties` | Low, design only |
