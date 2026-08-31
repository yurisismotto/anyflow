# 03 — Platform capability matrix

| Field | Value |
| --- | --- |
| **Title** | What each platform can actually do |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Every AnyFlow capability and platform integration point, per target platform. |
| **Decision status** | Informational. Feeds [22](22-IMPLEMENTATION-ROADMAP.md) and the product-semantics rule in §5. |
| **Evidence** | Per-cell. Linux GNOME and Android columns are REPO VERIFIED (shipping code). Every other column is OFFICIAL DOC VERIFIED or POC REQUIRED. |
| **Related documents** | [04](04-LINUX-PORTABILITY.md), [06](06-KDE-PLASMA-WAYLAND.md), [08](08-WINDOWS-FEASIBILITY.md), [10](10-MACOS-FEASIBILITY.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [15](15-CROSS-PLATFORM-CLIPBOARD.md), [17](17-BACKGROUND-EXECUTION-MODEL.md) |

---

## Legend

| Value | Meaning |
| --- | --- |
| **SUP** | Supported. Shipping today, or the platform API is documented and unambiguous. |
| **LIK** | Likely. Documented API exists and fits; no known obstacle; not yet proven for AnyFlow. |
| **POC** | Proof of concept required before anyone plans around it. |
| **LIM** | Possible but materially reduced compared to the reference behaviour. |
| **UNS** | Not supported by our design on this platform. |
| **PR** | **Platform restriction** — the OS forbids it. No amount of engineering changes this. |

Note on "supported today": the Linux GNOME column reflects `develop` at `f7a0015`. Linux KDE
is *the same binary on a different desktop*, which is why its cells differ only where the
desktop differs.

---

## 1. The matrix

| Capability | Linux GNOME | Linux KDE | Windows | macOS | Android | iOS | iPadOS |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| **Discovery** (`_anyflow._tcp.local.` advertise) | SUP | SUP | POC | POC | n/a¹ | n/a¹ | n/a¹ |
| **Discovery** (browse) | n/a¹ | n/a¹ | LIK | LIK | SUP | POC | POC |
| **Pairing** (QR + proof) | SUP | SUP | LIK | LIK | SUP | POC | POC |
| **TLS 1.3 mutual + SPKI pinning** | SUP | SUP | LIK | LIK | SUP | POC | POC |
| **Hardware-backed identity** | UNS² | UNS² | POC | POC | SUP | POC | POC |
| `battery.v1` **receive** | SUP | SUP | LIK | LIK | SUP | LIK | LIK |
| `battery.v1` **send** (own battery) | SUP³ | SUP³ | LIK | LIK | SUP | LIK | LIK |
| `files.v1` **send** | SUP | SUP | LIK | LIK | SUP | LIM⁴ | LIM⁴ |
| `files.v1` **receive** | SUP | SUP | LIK | LIK | SUP | LIM⁵ | LIM⁵ |
| `clipboard.v1` **send** (manual) | SUP | SUP | LIK | LIK | SUP | LIM⁶ | LIM⁶ |
| `clipboard.v1` **receive** (manual apply) | SUP | SUP | LIK | LIK | SUP | LIK | LIK |
| `clipboard.v1` **automatic send** (watch) | SUP⁷ | POC⁸ | LIK⁹ | POC¹⁰ | LIM¹¹ | **PR**¹² | **PR**¹² |
| `clipboard.v1` **automatic receive** (auto-apply) | SUP | SUP | LIK | LIK | LIM¹¹ | **PR**¹³ | **PR**¹³ |
| **Notifications** | SUP¹⁴ | SUP¹⁴ | LIK | LIK | SUP | LIK | LIK |
| **Always-connected session** | SUP | SUP | LIK | LIK | SUP¹⁵ | **PR**¹⁶ | **PR**¹⁶ |
| **Background execution** | SUP | SUP | LIK | LIK | SUP¹⁵ | **PR**¹⁶ | **PR**¹⁶ |
| **Launch at login** | LIK¹⁷ | LIK¹⁷ | LIK | LIK | n/a¹⁸ | **PR** | **PR** |
| **File picker** | SUP | SUP | LIK | LIK | SUP | LIK | LIK |
| **Share integration** | UNS¹⁹ | UNS¹⁹ | POC | LIK | SUP | LIK | LIK |
| **Native UI** | SUP | LIM²⁰ | LIK | LIK | SUP | LIK | LIK |
| **Local-network permission prompt** | none | none | firewall²¹ | POC²² | none²³ | **required**²⁴ | **required**²⁴ |

---

## 2. Notes

Each note is the reason the cell is not the obvious value. Numbers are referenced from the
table above and are stable — cite them as `03/n7`.

**¹ Direction is fixed by design.** `desktop/daemon/src/mdns.rs`: *"The daemon only
advertises; it does not browse. Android is always the initiator."*
([ADR-0005](../../adr/ADR-0005-lan-discovery-mdns.md)). The desktop is a stable listener; the phone
initiates. Windows and macOS inherit "advertise". iOS inherits "browse". A future
desktop↔desktop pairing would need both roles on the desktop side — out of scope here, and
noted in [13](13-CROSS-PLATFORM-DISCOVERY.md).

**² Linux has no hardware-backed identity today, by an explicit decision.**
`core/src/store.rs` documents it: the Secret Service (gnome-keyring) is deliberately *not*
used because *"a `systemd --user` daemon can start before any keyring is unlocked, and a
daemon that blocks on a locked keyring at boot is worse than useless."*
[ADR-0006](../../adr/ADR-0006-device-identity-and-pairing.md) records TPM2 as a follow-up. So on
Linux the current answer is a 0600 file in a 0700 directory, enforced at load
(`require_private_mode` refuses to start otherwise). This is a *deliberate* asymmetry, and
Windows/macOS gaining hardware backing makes Linux the weakest desktop — which is a finding,
not a problem to hide → [14](14-CROSS-PLATFORM-IDENTITY-AND-KEY-STORAGE.md).

**³ Only where UPower is present.** `daemon/src/main.rs` logs *"no local battery source;
battery.v1 is receive-only"* on a desktop tower. Correct behaviour, and the model for every
other platform.

**⁴ iOS send is Share-Sheet-shaped.** A file leaves iOS through the Share Sheet or a document
picker, in the foreground, with an explicit user action. There is no "watch a folder".

**⁵ iOS receive lands in the app's own container**, surfaced through the Files app via
`LSSupportsOpeningDocumentsInPlace` / a Document Provider — not into a system Downloads
folder, which iOS does not have. And it can only happen while the app is running →
[16](16-CROSS-PLATFORM-FILES.md).

**⁶ iOS clipboard *read* prompts the user** unless it goes through the paste menu, a keyboard
shortcut, or `UIPasteControl` (OFFICIAL DOC VERIFIED — the prompt "is generated whenever your
app accesses the pasteboard directly, that is, not going through the Paste menu command, the
keyboard shortcut, or `UIPasteControl`"). So "send my clipboard" on iOS is a
`UIPasteControl` button, not a background read.

**⁷ GNOME auto-send works via the Xwayland XFIXES bridge**, not via Wayland. Mutter implements
neither `zwlr_data_control_manager_v1` nor `ext_data_control_manager_v1`, so `wl-paste --watch`
cannot run there ([ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md), and the comment
in `capabilities/clipboard/Cargo.toml`). The X11 path is a *watch* only; reads and writes still
go through `wl-copy`/`wl-paste`.

**⁸ KDE is POC, not SUP, for a specific and checkable reason.** KWin *does* implement a
data-control protocol — it ported from `wlr-data-control-unstable-v1` to the standardised
`ext-data-control-v1` (KWin MR !6606). So `detect_watch_source()` in `backend/wayland.rs`
should take the `DataControl` branch and never need the X11 fallback. **But** the packaged
`wl-clipboard` must speak the protocol KWin offers, and on Ubuntu 24.04/25.10/26.04 the
packaged version is **2.2.1**, while `ext-data-control` support arrived in wl-clipboard
**2.3.0** (available in Debian forky/sid and Ubuntu "stonking" only). If KWin has dropped
`wlr-data-control`, auto-send on Ubuntu+KDE falls back to Xwayland or fails outright.
This is the single most valuable thing **POC-KDE-01** can answer. See
[06](06-KDE-PLASMA-WAYLAND.md).

**⁹ Windows auto-send is the best-supported of any platform.** `AddClipboardFormatListener`
posts `WM_CLIPBOARDUPDATE` to a window whenever the clipboard changes — a real event, no
polling (OFFICIAL DOC VERIFIED). It needs a message-pump window, which is one more reason the
Windows agent lives in the interactive session ([08](08-WINDOWS-FEASIBILITY.md)).

**¹⁰ macOS can only poll.** `NSPasteboard.changeCount` is the documented mechanism and there
is no public change notification in AppKit. This collides with the current
`ClipboardBackend` contract, which forbids polling — see
[01 §3.3](01-CURRENT-ARCHITECTURE-AUDIT.md) and **PLAT-DEC-009**.

**¹¹ Android's clipboard automation is already limited by the platform** and the app already
handles it correctly: since Android 10, `getPrimaryClip` requires input focus, which is why
`ClipboardTileService` opens `MainActivity` instead of reading the clipboard itself. Marked
LIM rather than PR because the existing app does ship working manual send plus receive.

**¹² iOS cannot watch the clipboard in the background.** `UIPasteboard.changedNotification`
is delivered only while the app is in the foreground and unsuspended, and reads outside a
paste gesture prompt. There is no background clipboard observation, and there should not be:
an app that could silently watch the pasteboard is exactly what the prompt exists to prevent.
**Do not promise this in the UI.**

**¹³ iOS cannot auto-apply a received clip in the background** either, because it is not
running to receive it. When the app *is* foreground, writing to `UIPasteboard` is
unrestricted (writing has never required permission), so "apply on next foreground" is
achievable → [15](15-CROSS-PLATFORM-CLIPBOARD.md) §6.

**¹⁴ Notifications on Linux are currently not implemented in the daemon.** The GUI shows
state; the CLI prints. A desktop notification would go through the freedesktop
`org.freedesktop.Notifications` D-Bus interface. Marked SUP because the platform supports it
and the GUI is present; the *feature* is a backlog item (UX-002).

**¹⁵ Android's always-connected session** is a `connectedDevice` foreground service
(`AndroidManifest.xml`, `service/ConnectionService.kt`) — supported, but at the cost of a
persistent notification, and subject to Doze.

**¹⁶ iOS suspends the app shortly after backgrounding, and may reclaim its sockets while
suspended** (OFFICIAL DOC VERIFIED — Apple's TN2277 and the background-execution
documentation: *"while the app is suspended the system may choose to reclaim resources out
from underneath a network socket used by the app, thereby closing the network connection"*).
No `UIBackgroundModes` value legitimately covers "maintain a LAN control session". This is
the hardest constraint in the whole expansion → [11](11-IOS-IPADOS-FEASIBILITY.md).

**¹⁷ Linux launch-at-login is LIK, not SUP, because nothing ships it.** The systemd unit is
`WantedBy=default.target` and must be enabled by hand (`systemctl --user enable --now`), and
the RPM's `%post` tells the user to do so. There is no XDG autostart entry for the GUI, and
no `.desktop` file at all → [07](07-LINUX-PACKAGING.md).

**¹⁸ Android does not start at boot and deliberately does not:** the manifest comment states
the service *"is NOT a `dataSync` daemon and it is not started at boot."*

**¹⁹ Linux has no share integration.** Android has the Sharesheet (`SendActivity`,
`ACTION_SEND`/`ACTION_SEND_MULTIPLE`, `mimeType="*/*"`). The Linux desktop equivalent would be
a `.desktop` file with `MimeType=` plus a file-manager "Send to" — neither exists. Windows
would use a Share Target or a shell verb; macOS an `NSExtension` Share Extension.

**²⁰ GTK/libadwaita on KDE is LIM for appearance, not function.** It runs; it does not look
like a Plasma application, does not follow Breeze, and gets GTK's file chooser unless the
XDG desktop portal is installed and configured → [06](06-KDE-PLASMA-WAYLAND.md).

**²¹ Windows has no "local network" permission**, but it does have the firewall, and an
inbound rule is mandatory for the listener. The recommendation is a rule scoped to the
**Private** profile with `RemoteAddress LocalSubnet` — Microsoft's own guidance for
"applications and services designed to only be accessed by devices within a home or small
business network" (OFFICIAL DOC VERIFIED). Never `0.0.0.0/0`, never the Public profile.

**²² macOS local-network privacy is a POC because the rules changed.** iOS-style local-network
prompting was extended to Mac apps in recent macOS releases. **RESOLVED (V-05, V-06):**

- TN3179 was retrieved in full. Local network privacy applies to **macOS from macOS 15**, and
  *"the exception for `launchd` daemons doesn't apply to `launchd` agents"* — so AnyFlow's
  per-user macOS agent **does** face the Local Network prompt.
- `NSPasteboard.AccessBehavior` arrived in **macOS 15.4** with four cases. **The General
  pasteboard defaults to asking on programmatic access.** So macOS clipboard **auto-send is
  user-gated**, not merely polling-based.

The residual question — whether reading `changeCount` alone trips the alert, or only the
subsequent content read — is now the sharpened subject of **POC-MAC-05**.
See [26 §V-05, §V-06](26-EXTERNAL-VERIFICATION-CLOSEOUT.md).

**²³ Android needs no local-network permission**, but does need
`CHANGE_WIFI_MULTICAST_STATE` and a held `MulticastLock`, or Wi-Fi hardware filters multicast
with the screen off and discovery silently stops. This is already implemented and documented
in `net/Discovery.kt`.

**²⁴ iOS/iPadOS require `NSLocalNetworkUsageDescription` *and* `NSBonjourServices`**, and the
user is prompted once. Denial is not recoverable in-app — the user must go to Settings. The
service type must be listed literally: `_anyflow._tcp`.

---

## 3. Reading the matrix

Three shapes fall out.

**Desktops converge.** Linux, Windows and macOS differ in *how* (helper process vs. Win32
message vs. polling) but not in *what*: all three can do full bidirectional clipboard,
files both ways, an always-connected session, and login startup. The user-visible product is
the same on all three. macOS's polling and KDE's protocol question are implementation
details, not feature differences.

**Android is already the second-class-by-platform-design case, and it works.** It cannot
watch the clipboard freely, it needs a foreground-service notification, it does not start at
boot. The app ships anyway, and the design absorbed those limits instead of fighting them.
That is the precedent for iOS.

**iOS is different in kind.** Its `PR` cells are not "not yet" — they are "not ever, by
design". Four rows are hard platform restrictions. Any plan that quietly assumes iOS will
behave like a desktop is wrong before it starts.

---

## 4. Where the matrix disagrees with today's protocol

`clipboard.v1` currently has no way to say *"I can receive but I can never automatically
send."* Advertising `clipboard.v1` is all-or-nothing (`core/src/capability.rs`: two ids
"either match or they don't"). An iOS peer that advertises `clipboard.v1` will look, to a
Linux peer's UI, exactly like another Linux box — and the user will turn on `auto_send` for
it and wait forever.

That is not a bug today (there is no iOS client) and it is not a reason to change the
protocol today. It is a reason to *decide deliberately*, before iOS exists, between:

- **(a)** capability metadata in `HELLO`, negotiated and fail-closed;
- **(b)** no protocol change; each side infers from `DeviceInfo.platform`;
- **(c)** no protocol change; the *sending* side's UI simply never offers `auto_send` toggles
  for a peer that has never sent an automatic clip.

Option (b) is the trap — it makes `platform` an input to behaviour, and `platform` is
attacker-supplied. Analysed in [15 §7](15-CROSS-PLATFORM-CLIPBOARD.md), tracked as
**PLAT-DEC-006**. **No change is proposed for implementation in this sprint.**

---

## 5. Product rule this matrix implies

From the brief, restated as a rule for implementers:

> A platform advertises only what it can do, the UI offers only what is advertised, and a
> capability that is impossible on a platform is **absent**, not present-and-broken.

Concretely: the iOS build must not show an "automatic clipboard sync" toggle. The macOS build
must show clipboard watching as available only if the polling backend is actually running. A
Linux GNOME build with no `wl-clipboard` installed already does the right thing — it reports
`Unavailable` with the exact package to install — and that behaviour is the standard for
every other platform.
