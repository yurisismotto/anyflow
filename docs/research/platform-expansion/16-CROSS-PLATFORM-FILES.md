# 16 — Cross-platform files

| Field | Value |
| --- | --- |
| **Title** | `files.v1` on five platforms |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Pickers, destinations, sandboxes, filename safety, atomic writes, background receive, resume. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for the current implementation; OFFICIAL DOC VERIFIED for platform APIs; POC REQUIRED where marked. |
| **⚠ Verification update** | Research v1's claim that `filename.rs` lacks Windows rules is **REFUTED** — device names, trailing dots/spaces and `\` are present and tested. The real gaps are **`:`** (alternate data streams) and **Unicode category `Cf`** (`U+202E` bidi override — a **present-tense Linux defect**). Scope decided as **protocol-global** (PLAT-DEC-014); SEC-004 rewritten and moved to **Wave 0**. Case-insensitive collisions are already safe (`create_new` + numbering). See [26 §7](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). |
| **Related documents** | [09](09-WINDOWS-SECURITY-AND-INTEGRATION.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [20](20-SECURITY-THREAT-ANALYSIS.md), [../../architecture/FILES.md](../../architecture/FILES.md), [ADR-0013](../../adr/ADR-0013-file-transfer-data-stream.md) |

---

## 1. What is already portable

Most of it. `capabilities/files/` is 3400 lines, and the platform-bound part is one 440-line
file.

| Component | File | Portable? |
| --- | --- | --- |
| Transfer state machine | `transfer.rs` (363) | ✅ |
| Data-stream framing and auth | `stream.rs` (427), `auth.rs` (280) | ✅ — HMAC over a single-use challenge, `subtle` constant-time compare |
| Limits | `limits.rs` (130) | ✅ |
| Capability handler | `lib.rs` (1833) | ✅ |
| Filename sanitisation | `filename.rs` (268) | ⚠️ **POSIX-shaped rules** — §4 |
| Destination and writing | `destination.rs` (440) | ❌ Unix modes + XDG |

And the protocol needs nothing: an offer carries a **filename and no path at all**, so a peer
can never influence where a file lands. That single decision is what makes `files.v1` portable
to platforms with radically different filesystems — including iOS, which has no user-visible
filesystem at all.

The second TLS connection for bulk data ([ADR-0013](../../adr/ADR-0013-file-transfer-data-stream.md))
is likewise portable: it is the same port, the same mutual auth, the same pins, distinguished
by ALPN (`anyflow-data/1`). Nothing platform-specific.

---

## 2. Destination per platform

`destination.rs` currently resolves `$XDG_DOWNLOAD_DIR` → `user-dirs.dirs` → `$HOME/Downloads`,
then `AnyFlow/` underneath.

| Platform | Downloads | Data dir | Notes |
| --- | --- | --- | --- |
| **Linux** | XDG chain (as today) | `$XDG_DATA_HOME/anyflow` | Correctly localised — a Brazilian desktop gets `~/Transferências` |
| **Windows** | `SHGetKnownFolderPath(FOLDERID_Downloads)` | `%LOCALAPPDATA%\AnyFlow` | Also correctly localised |
| **macOS** | `FileManager.urls(for: .downloadsDirectory)` | `~/Library/Application Support/AnyFlow` | POSIX modes work as-is |
| **Android** | `MediaStore.Downloads` / SAF | app-private | Already implemented (`files/Downloads.kt`) |
| **iOS/iPadOS** | **No such concept** | app container `Documents/` | §5 |

A `FileSink` trait (Δ4 in [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md)) is the seam:

```rust
// PROPOSED, illustrative.
pub trait FileSink: Send + Sync {
    /// Create an exclusive, owner-only temp file next to the final location.
    fn reserve_temp(&self, transfer: TransferId) -> io::Result<TempHandle>;
    /// Atomically promote to a non-colliding final name; returns where it landed.
    fn promote(&self, temp: TempHandle, name: &str) -> io::Result<PathBuf>;
    /// Human-readable description of where files go, for the UI.
    fn describe(&self) -> String;
}
```

`Destination` becomes the Linux/macOS implementation; Windows and iOS get their own. The
*policy* — dedicated directory chosen by this machine, never influenced by the peer, duplicate
suffixing, atomic promotion — stays in portable code.

---

## 3. Safe writing per platform

`destination.rs`'s three safety properties, and how each transfers:

| Property | Linux/macOS | Windows | iOS |
| --- | --- | --- | --- |
| Temp file **inside the destination dir**, not `/tmp` — because `rename(2)` is only atomic within one filesystem | ✅ as written | ✅ same reasoning; `MoveFileEx` | ✅ app container |
| `O_EXCL` + mode 0600, so a **planted symlink cannot redirect** the write | ✅ as written | `CreateFile(CREATE_NEW)`. Reparse points need a deliberate decision — consider `FILE_FLAG_OPEN_REPARSE_POINT` | ✅ sandboxed |
| Atomic promotion: "at no instant does a file with the final name exist containing partial or unverified data" | `std::fs::rename` | `std::fs::rename` maps to `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` — replaces, unlike POSIX-strict semantics. Verify the placeholder-reservation dance still holds | ✅ |

The `O_EXCL` reasoning is worth preserving verbatim in every implementation, because it is the
non-obvious one: it defends against a *local* attacker who plants a symlink in the download
directory, not against the remote peer. A Windows implementation that used `CREATE_ALWAYS`
instead of `CREATE_NEW` would silently lose it.

---

## 4. Filename safety — the real gap

`filename.rs` sanitises for POSIX. Its tests show the intent:
`sanitize("/etc/shadow") == Some("shadow")`, `sanitize("/etc/") == None`.

The following peer-supplied names are currently mishandled on Windows, and `files.v1` accepts
filenames from a **paired but possibly misbehaving peer** — which the threat model treats as
in scope:

| Input | Consequence on Windows |
| --- | --- |
| `CON`, `NUL`, `AUX`, `PRN`, `COM1`–`COM9`, `LPT1`–`LPT9`, with or without an extension | Reserved device names |
| `report.txt.` / `report.txt ` | Win32 strips trailing dots/spaces → written name ≠ displayed name |
| `notes:secret` | `:` opens an **alternate data stream** — bytes land where Explorer shows nothing |
| `a\b\c.txt` | `\` is a separator and is not stripped by POSIX-shaped logic |
| `<>"\|?*` | Invalid on Windows |
| Very long names | `MAX_PATH` behaviour differs |

macOS adds Unicode normalisation (APFS/HFS+ NFD) and a historical Finder meaning for `:`.

**SEC-004: extend `filename.rs` with a Windows-safe rule set, and apply it on every platform.**

Applying it everywhere rather than under `#[cfg(windows)]` is deliberate:
- a file received on Linux and later copied to Windows or a network share carries the problem;
- one rule set means one test suite, and `filename.rs` plus `daemon/tests/files.rs` already
  have a good one (`daemon/tests/files.rs:930` already tests `/etc/cron.d/anyflow → anyflow`);
- a `#[cfg]`-gated security rule is a rule that is untested on the CI host.

The existing behaviour — sanitise to a safe name, or reject — is the right shape. It just needs
more rules.

---

## 5. iOS: what `files.v1` can mean

iOS has no user-visible filesystem and no Downloads directory. The protocol survives this
unchanged (an offer carries a name, not a path), but the *product* must be redefined.

| Direction | Mechanism | Constraint |
| --- | --- | --- |
| **Send** | Share Sheet (`NSExtension`) or `UIDocumentPickerViewController` | Foreground; user-initiated |
| **Receive** | Written to the app container's `Documents/` | **Only while the app runs** |
| Surfacing received files | `UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace` makes `Documents/` visible in the Files app | The natural equivalent of a Downloads folder |
| "Save to Files" | Share Sheet from within AnyFlow | Lets the user move it wherever they like |
| **Background receive** | ❌ | The app is suspended; sockets may be reclaimed |
| Mid-transfer suspension | ❌ Transfer fails | §6 |

So iOS `files.v1` is: **receive while the app is open; send from anywhere via the Share Sheet.**
That is genuinely useful — "share this photo to my computer" is the single most common thing
people want from a phone — and it should be presented as the feature it is, not as a limited
version of something else.

The Share Extension runs in a separate process and needs an App Group plus a keychain access
group to reach the identity and trust store; and it should be **read-only** with respect to the
trust store ([12 §9](12-APPLE-SECURITY-AND-INTEGRATION.md), **SEC-007**).

---

## 6. Resume — the feature iOS would need and does not have

`files.v1` has no resume. A transfer that is interrupted starts over
([FILES.md](../../architecture/FILES.md)). On Linux and Windows that is a rare annoyance; on iOS it
is structural, because interruption is the normal case.

Options, none for this sprint:

| Option | Assessment |
| --- | --- |
| **(a)** No resume; iOS transfers are foreground-only and restart | **Recommended for v1.** Honest, no protocol change |
| (b) `files.v2` with byte-range resume | A real protocol change: offsets, integrity over partial content, a resume token. Would benefit every platform, and is the right long-term answer |
| (c) `URLSession` background transfer on iOS | Continues while suspended, but is HTTP-shaped — it would mean a second protocol with its own authentication story alongside `anyflow-data/1`. **Rejected**: a parallel transport is exactly the "platform-specific protocol" the compatibility principle forbids |

**Recommendation: (a) now; record (b) as a candidate `files.v2` driven by general value rather
than by iOS.** Note explicitly that (b) must be `files.v2` and not a mutation of `files.v1` —
the version is part of the capability id, and *"a breaking change ships as `battery.v2` and both
can be advertised at once during a migration"* (`core/src/capability.rs`).

---

## 7. Pickers and share integration

| Platform | Send-from picker | Share-target integration |
| --- | --- | --- |
| Linux | `gtk::FileDialog` (GUI), a path argument (CLI) | **None today.** Would be a `.desktop` `MimeType=` entry + a file-manager "Send to" |
| Windows | `IFileOpenDialog` from the UI | Share Target contract (packaged) or a shell verb / "Send to" entry |
| macOS | `NSOpenPanel` / `.fileImporter` | Share Extension (`NSExtension`) |
| Android | SAF | **Shipping** — `SendActivity`, `ACTION_SEND`, `ACTION_SEND_MULTIPLE`, `mimeType="*/*"` |
| iOS | `UIDocumentPickerViewController` | Share Extension |

Android is the reference and the manifest comment explains why `*/*` is safe:
*"It grants the app nothing: the URI arrives with a temporary read permission for that one
item, which is strictly less access than any storage permission would be, and is exactly why no
storage permission is declared anywhere in this manifest."*

**That principle should carry to every platform: take a handle to the one file the user chose,
never a directory grant.** On Windows and macOS that means using the picker's returned handle
(and on a sandboxed macOS, a security-scoped bookmark) rather than requesting broad filesystem
access.

The Linux gap is notable: the desktop with the deepest integration everywhere else has the
weakest share story. **LINUX-008.**

---

## 8. Approval and notifications

`files.v1` is **never auto-granted**
([ADR-0008](../../adr/ADR-0008-capability-architecture.md)) because writing a file is a side effect,
and each transfer additionally needs a human unless `--accept-files-without-asking` is set —
a flag whose own help text says it is *"deliberately awkward to turn on … Intended for
unattended test rigs, not for daily use."*

Per platform, the approval prompt is:

| Platform | Where the human is asked |
| --- | --- |
| Linux | GUI dialog (`AdwAlertDialog`) or the CLI's `Send`/`Transfers` event stream |
| Windows | Toast notification with actions, or the WinUI window |
| macOS | `UNUserNotificationCenter` with actions, or the app window |
| Android | Notification with actions (shipping) |
| iOS | In-app only — the app must be foreground to receive at all, so the prompt is a sheet |

The Windows and macOS cases need an actionable notification, which requires an AUMID on Windows
(free for MSIX packages) and notification authorisation on macOS. Neither is difficult; both are
easy to leave until last and then discover blocks the whole receive flow. **WIN-004**,
**MAC-006**.

---

## 9. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **SEC-004** | Windows-safe filename rules, applied everywhere | **High — blocks Windows receive** |
| **ARCH-006** | `FileSink` trait; `Destination` becomes its POSIX implementation | Wave 0 |
| **WIN-005** | Windows `FileSink` (`FOLDERID_Downloads`, `CREATE_NEW`, `MoveFileEx`) | High |
| **WIN-004** | Actionable toast for transfer approval | Medium |
| **MAC-006** | `UNUserNotificationCenter` approval | Medium |
| **SEC-006** | Set `com.apple.quarantine` on received files | Medium |
| **IOS-004** | Files-app visibility for the app container | Medium |
| **IOS-005** | Share Extension send path | Medium |
| **LINUX-008** | Linux share-target integration | Low |
| **PROTO-002** | Design `files.v2` with resume — design only | Low |
