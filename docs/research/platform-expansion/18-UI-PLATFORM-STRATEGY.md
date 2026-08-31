# 18 — UI platform strategy

| Field | Value |
| --- | --- |
| **Title** | Native UI per platform, one visual identity |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | UI toolkit per platform, how the brand survives five toolkits, why not Electron, why not a web UI. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for the existing design system; OFFICIAL DOC VERIFIED for toolkit availability. |
| **Related documents** | [06](06-KDE-PLASMA-WAYLAND.md), [08](08-WINDOWS-FEASIBILITY.md), [10](10-MACOS-FEASIBILITY.md), [../../design/BRAND.md](../../design/BRAND.md), [../../design/UI-GUIDELINES.md](../../design/UI-GUIDELINES.md) |

---

## 1. The strategy in one line

**Native toolkit per platform; one design-token source of truth; one visual identity; one
control protocol underneath.**

Not "one UI codebase". The duplication analysis in
[02 §1](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) puts UI firmly in the *don't share* column:
the cost of two UIs disagreeing is zero, and the cost of one UI feeling foreign on four
platforms is the whole product experience.

---

## 2. Toolkit per platform

| Platform | Toolkit | Rationale |
| --- | --- | --- |
| **Linux** | GTK4 + libadwaita | Shipping. Correct on GNOME; foreign but functional on KDE ([06 §7](06-KDE-PLASMA-WAYLAND.md)) |
| **Windows** | **WinUI 3** (Windows App SDK) | Microsoft's current desktop stack; SDK 2.4.0 stable; works packaged and unpackaged; Windows 10 1809+ (OFFICIAL DOC VERIFIED). WPF is an acceptable fallback |
| **macOS** | **SwiftUI**, AppKit where SwiftUI is thin | `MenuBarExtra` for the menu-bar item; AppKit for `NSOpenPanel` and anything SwiftUI does not cover |
| **Android** | Jetpack Compose | Shipping |
| **iOS/iPadOS** | SwiftUI, UIKit where needed | `UIPasteControl` is UIKit; shares patterns and much code with the macOS app |

Every one of these is a **client of the AnyFlow Agent**, not a host of it. The Linux GUI already
works this way: `gui/src/client.rs` is 203 lines over a newline-delimited JSON Unix socket, and
the GUI depends on `anyflow-daemon` *only* to reuse the request/response types so it "cannot
drift from the socket contract". That property should be preserved everywhere:

- the UI can be closed, crash, or never be installed, and the session continues;
- there is no Rust↔C#/Swift FFI for the UI at all;
- a platform's CLI comes free from the same protocol.

(One caveat noted in [01 §4](01-CURRENT-ARCHITECTURE-AUDIT.md): the GUI currently inherits the
whole daemon crate to get those types. Extracting a small `anyflow-control` types crate is
Wave 0 work — **ARCH-003**.)

---

## 3. The identity travels; the widgets do not

AnyFlow already has a cross-platform design system, and it is unusually rigorous. From
`docs/design/tokens.json`:

> *"Canonical AnyFlow design tokens. This file is the single source of truth for both platforms
> and is READ BY TESTS on each side, the same way `protocol/testdata` keeps the Rust and Kotlin
> protocol implementations from drifting."*

There is a `DesignTokensTest.kt` on Android and a `theme.rs` on the GTK side, both checked
against the same JSON. That mechanism extends to two more platforms with no new invention:
generate (or hand-mirror, with a test) a Swift `Color` table and a WinUI `ResourceDictionary`
from `tokens.json`, and add the corresponding token test.

**What travels:**

| Element | How it survives a new toolkit |
| --- | --- |
| **Flowing A** (institutional mark) | SVG in `docs/design/assets/`; About screens, wordmark lockups |
| **Flowing Ribbon** (app mark) | SVG; app icon on each platform, in each platform's icon geometry |
| **Palette A** | `tokens.json` → per-platform token table + a test |
| The `brand.*` vs `on_light.*`/`on_dark.*` split | **Must be carried verbatim.** Brand teal is 2.49:1 on white and *may never be used for a text label*; the corrected hues exist for that. A new platform that flattens the two families reintroduces a WCAG failure |
| "The interface stays quiet" | A principle, not a widget: neutral surfaces, hairline borders, one accent at a time |
| The flow metaphor | Empty states, transfer motifs — always the same curve |

**What does not travel, and should not:**

navigation patterns, control shapes, typography scale, motion curves, dialog conventions, and
above all the *chrome*. `AdwNavigationSplitView` on Linux, `NavigationView` on WinUI,
`NavigationSplitView` on SwiftUI, `NavigationSuiteScaffold` on Compose — same information
architecture, four native expressions.

The existing `UI-GUIDELINES.md` should gain a short "per-platform expression" section rather
than being rewritten. **UX-001.**

---

## 4. Why not Electron

The brief asks for an extremely convincing analysis before recommending it. Here is why it is
not recommended:

1. **The duplication is not in the UI.** §1 of
   [02](02-CROSS-PLATFORM-TARGET-ARCHITECTURE.md) enumerates what is expensive to duplicate —
   pairing proofs, pinning, framing, replay, dedup, transfer state. None of it is UI. Electron
   would share the cheap half and leave the expensive half untouched.
2. **AnyFlow's value is OS integration**, and every integration point needs native code
   regardless: clipboard ownership and watching, tray/menu-bar presence, login registration,
   share targets, native file pickers, notifications with actions, and keystore access. With
   Electron, all of that still gets written per platform — plus a Node/Chromium layer, plus the
   IPC to reach it.
3. **It contradicts the product.** A local-first, no-telemetry, small-footprint background agent
   that ships a browser engine per install and holds ~150 MB of RAM to show a device list is a
   contradiction users notice.
4. **Security surface.** A Chromium runtime in a process that holds the identity key and the
   trust store is a large addition to a threat model whose current attack surface is
   "a TLS listener and a Unix socket".
5. **Accessibility and platform conventions** are approximated rather than inherited.

**Tauri** deserves a fairer hearing, since its core is already Rust and its footprint is far
smaller. It is not dismissed. But it solves the *packaging-a-webview* problem, not the
clipboard/agent/keystore problems, which are the actual work — and it would still leave the
Linux GUI needing a rewrite away from a GTK/libadwaita implementation that already exists and
works. **Revisit only if maintaining three native UIs proves to be the bottleneck**, which it
will not be for a long time: the GUI is a thin client over a JSON protocol.

**And no web UI merely to share code.** A localhost web interface would put an HTTP server in
the agent, create a new authentication problem (any local process could reach it), and give up
every native affordance. The control protocol already gives code sharing where it is free:
the *contract*, not the rendering.

---

## 5. Shared UI surface that is worth sharing

Not zero. These are worth doing once:

| Item | How |
| --- | --- |
| **Design tokens** | `tokens.json` + a per-platform test. **Already the pattern** |
| **Iconography** | `docs/design/assets/icons/*.svg` — 24 icons already exist and are already mirrored into Android drawables and a GTK GResource. Adding a Swift asset catalogue and a WinUI resource set is mechanical |
| **Copy strings** | Error and status wording (`FailureKind.kt` / `UiMapping.kt` on Android) is where platforms drift most visibly. A shared string table would keep "what went wrong" identical everywhere |
| **The control protocol** | Types generated or mirrored from one definition; the CLI, GUI, WinUI app and SwiftUI app all speak it |
| **State→display mapping** | `UiMapping.kt` and `views/mod.rs` both turn peer/transfer state into user-facing labels. This is *logic*, not rendering, and is a candidate for the shared core |

The last row is worth pursuing: "is this device connected, stale, revoked, or merely
discovered" is a decision with real rules (`BATTERY_STALE_AFTER_SECS`, `DeviceState`,
revocation), and four independent renderings of those rules will disagree.
**ARCH-010.**

---

## 6. Platform-specific UI surfaces

Each platform has one or two entry points that are not a window and that carry
disproportionate value:

| Platform | Surface | Value |
| --- | --- | --- |
| Linux | GNOME Shell / Plasma tray, notifications | Presence without a window |
| Windows | **Tray icon**, toast notifications with actions, jump list | The tray is where a background agent lives on Windows |
| macOS | **Menu-bar item** (`MenuBarExtra`), notification centre | Same role as the Windows tray |
| Android | **Quick Settings tile** (shipping), Sharesheet, ongoing notification | `ClipboardTileService` is a good example of designing around a platform limit rather than fighting it |
| iOS | **Share Sheet**, widgets, Shortcuts | The Share Sheet is the primary entry point given the foreground-only model |

Android's QS tile is instructive and should be the model for the others: the tile cannot read
the clipboard (no input focus since Android 10), so it opens the Activity and lets the human
confirm — *"That is the supported flow, not a workaround for one."*

**Windows tray and macOS menu bar should be treated as required, not optional.** An agent with
no visible presence is an agent users distrust, disable, or forget they installed.

---

## 7. Accessibility

The Linux GUI already uses `gtk::AccessibleRole` in several places, which is more than most
projects start with. Each platform must carry its own:

| Platform | Requirement |
| --- | --- |
| Linux | AT-SPI roles and labels (partially done) |
| Windows | UI Automation — WinUI provides it if controls are used properly and `AutomationProperties` are set |
| macOS/iOS | VoiceOver labels; Dynamic Type |
| Android | TalkBack; content descriptions |

And one cross-platform rule already encoded in `tokens.json`: **contrast**. The `on_light.*` /
`on_dark.*` families exist precisely so accent hues meet WCAG AA as text. Any new platform token
table must be tested against the same thresholds, not eyeballed.

---

## 8. Recommended direction

1. **Native toolkit per platform**: GTK4/libadwaita, WinUI 3, SwiftUI, Compose, SwiftUI.
2. **UI is always a client of the Agent** over the control protocol. No FFI for UI.
3. **`tokens.json` stays the single source of truth**; every platform gets a token test.
4. **Brand marks and icons ship as SVG** from `docs/design/assets/` and are converted per
   platform, as they already are for Android and GTK.
5. **No Electron.** Revisit Tauri only if three native UIs become the bottleneck.
6. **No web UI for code sharing.**
7. **Tray (Windows) and menu bar (macOS) are required surfaces.**
8. **Keep GTK on KDE** for now ([06 §7](06-KDE-PLASMA-WAYLAND.md)); a Kirigami frontend stays a
   welcome contribution rather than a plan.
9. **Extract state→display mapping** into shared logic before it exists in four places.
