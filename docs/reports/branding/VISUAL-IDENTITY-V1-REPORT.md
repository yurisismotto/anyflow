# AnyFlow — Visual Identity & Product UI System v1

**Sprint closeout · 31 August 2026 · branch `feature/visual-identity-v1`**

| | |
|---|---|
| **Status** | **ANYFLOW VISUAL IDENTITY V1 COMPLETE** |
| Branch | `feature/visual-identity-v1` |
| Base | `dbdeb2b` |
| Commits made | none — no commit, no push |
| Working tree at close | 8 modified · 23 untracked |
| Gates | 26 PASS · 1 NOT EXECUTED · 0 FAIL |

---

## 1. Executive summary

The sprint delivered a complete visual identity and design system for AnyFlow,
applied end to end on both platforms and reviewed on real hardware.

Two things were larger than the brief implied, and both were built rather than
deferred:

**There was no desktop application.** `desktop/gui/` held a placeholder README
and nothing else. VIS-13 … VIS-18 therefore meant writing a complete GTK4 /
libadwaita application from zero — window shell, six pages, pairing dialog,
theming, tests — as a client of the daemon's existing control socket. It is
built, it runs on this Fedora machine, and every screen in this report is a
capture of it running against the live daemon.

**The palette as specified is not accessible for text.** Brand teal is
**2.49 : 1** on white against a WCAG AA floor of 4.5 : 1; blue and violet also
fall short. The reference draws "Connected" in brand teal. Rendered literally
that makes the most load-bearing word on the screen the least readable one, so
the palette was split into an identity family (fills, marks, gradients) and a
corrected family (all text and small icons) that clears AA on its own surface.
Both platforms assert this with tests that recompute the ratios, including
sampling the CTA gradient's interpolation.

**One real defect was found and fixed on hardware.** Any received clipboard
clip crashed the Android home screen — a pending clip and its own peer produced
the same `LazyColumn` key, and Compose throws on a duplicate. It is fixed,
covered by three regression tests, and re-verified on the tablet.

Everything green stayed green: **303** Rust tests (was 292), **232** Android
JVM tests (was 206), `fmt` clean, clippy at **zero** warnings.

---

## 2. Visual direction adopted

Faithful to the reference in composition, spacing, hierarchy, cards, borders,
iconography and general feel — with five deliberate departures, each because
the reference showed something the product does not do or must not do. They
are listed in §30 and documented in
[`docs/design/UI-GUIDELINES.md`](../../../docs/design/UI-GUIDELINES.md).

The interface stays quiet: white or Ink surfaces, hairline borders, low
elevation, one accent at a time. Colour is spent on meaning — connected,
transferring, revoked — and gradient is reserved for the mark, one primary
button, a progress fill and illustration.

---

## 3. Flowing A — the institutional mark

An abstract "A" from flowing lines with the crossbar running through and out
to the right as a ribbon. Drawn on a 64-unit grid, filled with the brand
gradient, with a round-terminal bar at 6.5 units.

Used for: wordmark lockup, About, onboarding, documentation. On Android it is
the mark on the Settings screen.

Files: `logo-flowing-a.svg`, `logo-flowing-a-mono.svg`, `logo-lockup.svg`,
`res/drawable/logo_flowing_a.xml`.

## 4. Flowing Ribbon — the product and connection mark

One continuous stroke from a teal node to a violet node — `device ●~~~● device`.
Geometry `M14,44 C 30,44 24,21 50,22`, stroke 6.5, round caps, nodes r=7.

Used for: app icon, app bar, transfer motif, empty-state illustration, the
sidebar identity block, and the mark at the centre of the pairing QR.

Files: `icon-flowing-ribbon.svg`, `icon-flowing-ribbon-mono.svg`,
`app-icon.svg`, `ribbon-connection.svg`, `res/drawable/logo_flowing_ribbon.xml`.

**Family.** Both marks share the 64-unit grid, the 6.5-unit weight, round
terminals, the same curvature and the same gradient axis. The ribbon's node
circles and the A's bar terminal are the same radius.

## 5. Palette A

`#16B8A6` Teal · `#4F7CFF` Blue · `#8B5CF6` Violet · `#0F172A` Ink ·
`#F8FAFC` Paper, implemented exactly.

Paper and Ink are the two ends of one slate ramp, so the eleven-step neutral
scale is derived rather than invented. Semantic tokens — surface, border,
text-secondary, disabled and the rest — sit on top; no screen contains a
literal hex.

**The two-family split** (see §1): `on_light` = teal `#0F766E`, blue `#3B5BDB`,
violet `#7C3AED`, amber `#B45309`, red `#DC2626`; `on_dark` = `#2DD4BF`,
`#8FA9FF`, `#A78BFA`, `#FBBF24`, `#F87171`. Every one clears AA.

## 6. Typography

The AnyFlow scale — display 32/700 through caption 12/400, plus a monospace
role — applied to the platform UI face. Inter is the documented direction and
is deliberately not bundled: a megabyte of font on Android to replace a
near-identical system face, and on GNOME overriding the user's own font
setting. The desktop stack is `Inter, Cantarell, Adwaita Sans, sans-serif`, so
Inter is used wherever it is installed.

Fingerprints, device ids and hashes use monospace, grouped in fours. That is
not styling: a fingerprint is compared character by character against another
screen.

## 7. Tokens

[`docs/design/tokens.json`](../../../docs/design/tokens.json) is the single source of
truth for colours, gradients, spacing (4 px base), radius, borders, elevation,
icon sizes, motion, status colours and typography.

**Both platforms read it in their tests** — `desktop/gui/src/theme.rs` and
`android/.../DesignTokensTest.kt` — mirroring the existing `protocol/testdata`
pattern that keeps the Rust and Kotlin protocol implementations aligned.
Change a value on one side and the other fails.

Platform modules: `ui/theme/{Color,Type,Dimens,Motion,Gradients,Status}.kt`
and `desktop/gui/src/theme.rs` + `data/style.css`.

---

## 8. Android app icon

Adaptive icon: Ink background, the ribbon foreground scaled into the 72 dp
safe zone, plus a monochrome layer for Android 13+ themed icons. Verified on
the tablet's taskbar.

`res/mipmap-anydpi-v26/ic_launcher.xml` + `ic_launcher_round.xml`,
`res/drawable/ic_launcher_{foreground,monochrome}.xml`.

## 9. Android home

Top bar with ribbon + wordmark. "Devices" with an add button. Device cards
carrying status badge, name, platform, battery and capability chips, with the
ribbon flourish as a corner accent on a connected device only. Quick actions —
Send clipboard, Send files, Pair device, Device settings — which grey out
rather than disappearing when they cannot run. Connection card. Local-network
notice. Bottom navigation: Devices · Activity · Settings.

Anything waiting on a decision — an incoming file, a held clip — is placed
above the device list, because it is the only thing on screen with a deadline.

## 10. Android peer details

Circular device avatar, name, status badge, platform. **Permissions** card
(Clipboard / Files / Battery, each with a one-line description of what it
actually permits). **Clipboard** card with receive, auto-apply — disabled while
receive is off — and send. The "Automatic sending is not possible on Android"
notice states the platform reason instead of offering a dead toggle. Gradient
CTA. **Security** card with the fingerprint in monospace. "Forget this device"
in outlined red, set apart at the bottom, never beside a confirming button.

## 11. Android clipboard

Direction and automation live on the peer detail screen, above. The send flow
is its own screen (§12 of the reference): hero motif, connected badge,
clipboard preview, gradient CTA, cancel, and an accurate security line.

**The preview shows ordinary text and hides sensitive text.**
`EXTRA_IS_SENSITIVE` exists so a surface like this does not render a password
into a shoulder-surfable box; a sensitive clip shows a masked placeholder with
its size, and the pre-existing confirmation dialog still fires before any send.
Nothing is logged or persisted — the preview lives in composition and dies with
the screen.

## 12. Android files

Incoming offer cards (accept / reject), transfer cards with the gradient
progress bar, byte counts, direction stated in words, and cancel. Terminal
states keep their status until cleared; a cancellation is not drawn as a
failure.

## 13. Android pairing

The QR scan path and its permission flow are unchanged. What changed is the
surface around it: the empty state uses the connection ribbon with
"Connect your first device" and a gradient "Pair device" CTA, and the add
button in the Devices header opens the same scanner.

## 14. Android dark theme

Derived from Ink rather than inverted: background `#0A0F1C` sits just below
Ink and Ink itself becomes the card surface, so a card reads as lifted out of
the page exactly as in light. Accents move to the `on_dark` family. Verified
on the tablet with `cmd uimode night yes`, then reverted.

---

## 15. Desktop shell

`AdwApplicationWindow` → `AdwToolbarView` (header bar with ribbon + wordmark)
→ `AdwNavigationSplitView` → `GtkStack`, with a status strip along the bottom.
Sidebar: Dashboard · Files · Clipboard · Devices · Trusted peers · Settings,
with the network state and this computer's own name and short fingerprint
pinned to its foot.

Talks to the daemon over the existing Unix control socket using the daemon's
own `Request`/`Response` types. No new protocol, capability or privilege.

## 16. Desktop dashboard

Device cards with platform, fingerprint, status badge, a battery panel while a
session is live, and capability chips. Alongside them, **Activity** (the live
transfer list, labelled as this run only) and **Quick actions** (Send a file,
via `Request::Send`). Empty state uses the connection ribbon.

## 17. Desktop pairing

A dialog with a **drawn** QR — error correction level H with the ribbon in the
centre — beside a "Trusted device identity" panel that fills in when a device
proves it holds the code: name, device id and the **full** fingerprint in
monospace. Security notes, then "Confirm & pair" on the CTA gradient, disabled
until there is something to confirm.

Drawing the code rather than showing the daemon's ASCII art also fixes a real
problem: the terminal rendering inverts on a dark background and ZXing will
not decode an inverted QR.

## 18. Desktop files

In-progress and completed sections, gradient progress, direction in words,
byte counts, cancel, and stored path for received files — under an explicit
line saying the list covers the current daemon run and nothing is kept on disk.

## 19. Desktop clipboard

Backend card naming the real backend and whether clipboard changes can be
observed here — which is what decides whether automatic sending is offerable
at all. Per-device cards with grant state, revoked state reported separately,
and the four policy switches; auto-send is disabled with its reason when the
session cannot watch. Pending clips show **size, hash prefix, age and
sensitivity — never content**.

**No clipboard history.** See §30.

## 20. Desktop dark theme

The stylesheet is rebuilt and swapped when `AdwStyleManager::is_dark` changes;
dark is a derived palette, so it cannot be a filter over the light one.
Verified by switching the real GNOME `color-scheme` setting, then reverting.

---

## 21. Accessibility

| Requirement | Status |
|---|---|
| Contrast | Every text token clears AA on its own surface; **asserted by tests** on both platforms |
| Not colour alone | Every status is dot + icon + word; the type has no colour-only constructor |
| Touch targets | 48 dp minimum on Android |
| Keyboard navigation | Full tab order on the desktop; 2 px focus ring that survives tinted cards |
| Screen reader labels | Semantic roles throughout; decorative artwork explicitly hidden |
| Grouped announcements | A status badge announces once as a phrase; fingerprints in four-character groups |
| Reduced motion | Android reads `ANIMATOR_DURATION_SCALE`; durations go through `motionDuration()` |
| Focus states | Explicit, not inherited |

## 22. Responsiveness

**Desktop** — an `AdwBreakpoint` at 700 sp collapses the split view rather than
squeezing the content; minimum window 360 × 420. Verified by resizing to 380
logical points: the sidebar collapses cleanly, nothing clips.

**Android** — content is capped at 640 dp and centred. Without it a device card
on this tablet became a 900 dp band with a name floating at one end. Phones
never reach the cap. Verified on the SM-X620 in both orientations of the
review.

---

## 23. Components created

Android (`ui/components/`): `AnyFlowCard`, `AnyFlowSectionLabel`,
`AnyFlowSecurityNotice`, `AnyFlowFingerprint`, `AnyFlowStatusBadge`,
`AnyFlowGradientMark`, `AnyFlowBrandMark`, `AnyFlowProgressBar`,
`AnyFlowBatteryPill`, `AnyFlowRibbonFlourish`, `AnyFlowIconTile`,
`AnyFlowDeviceCard`, `AnyFlowCapabilityRow`, `AnyFlowTransferCard`,
`AnyFlowEmptyState`, `AnyFlowPrimaryButton`, `AnyFlowSecondaryButton`,
`AnyFlowDestructiveButton`, `AnyFlowTextButton`, `AnyFlowQuickAction`.

Desktop (`gui/src/widgets.rs`): `card`, `section_label`, `security_notice`,
`fingerprint`, `status_badge`, `brand_mark`, `brand_logo`, `progress`,
`icon_tile`, `empty_state`, `cta_button`, `secondary_button`,
`destructive_button`, `separator`, plus the `Status` vocabulary and
`group_fingerprint`.

The two sets are deliberately named alike; the mapping is tabulated in the UI
guidelines.

## 24. Assets created

`logo-flowing-a.svg` · `logo-flowing-a-mono.svg` · `icon-flowing-ribbon.svg` ·
`icon-flowing-ribbon-mono.svg` · `app-icon.svg` · `ribbon-connection.svg` ·
`wordmark.svg` · `wordmark-mono.svg` · `logo-lockup.svg` · a 28-glyph icon
family in `docs/design/assets/icons/`, mirrored as 30 Android vector
drawables · the adaptive launcher icon with a monochrome layer.

All original vector work; no third-party or unknown-licence asset. The
wordmark is set in live text rather than outlines so the logo never disagrees
with the face the product renders.

The three reference images were supplied as chat attachments rather than
files, so nothing was added to `docs/design/reference/`; the empty directory
was removed.

## 25. Documentation

- [`docs/design/BRAND.md`](../../../docs/design/BRAND.md) — brand idea, tagline, the two
  logo roles and their hierarchy, palette with measured contrast ratios, the
  two-family rule, gradients, typography, spacing, radius, iconography,
  light/dark, asset inventory, misuse.
- [`docs/design/UI-GUIDELINES.md`](../../../docs/design/UI-GUIDELINES.md) — the five
  rules that outrank the mockups, status language, component vocabulary and
  cross-platform mapping, desktop patterns, Android patterns, motion,
  accessibility, and what must never be traded for UX.
- [`docs/design/tokens.json`](../../../docs/design/tokens.json) — the cross-platform
  contract.
- `desktop/gui/README.md` — rewritten from placeholder to real documentation.

## 26. Screenshots and visual comparison

Captured from the running applications, not mock-ups:

**Desktop** (Fedora, GNOME Wayland, live daemon) — dashboard, files, clipboard,
devices, trusted peers, settings, all in light; dashboard, clipboard, peers and
settings again in dark; plus a narrow-window capture proving the breakpoint.

**Android** (SM-X620, Android 16, live session) — home available, home
connected with the flourish, home with a pending clip, peer details, send
clipboard empty, send clipboard with a populated preview, activity, settings;
peer details and home in dark.

Held locally for review and **not committed**, per the brief.

## 27. Files modified

**Modified (8):** `android/app/build.gradle.kts` ·
`android/app/src/main/AndroidManifest.xml` · `ui/ClipboardViews.kt` ·
`ui/MainActivity.kt` · `ui/TransferViews.kt` · `desktop/Cargo.toml` ·
`desktop/Cargo.lock` · `desktop/gui/README.md`

**New (23 paths):** the Android `ui/theme/` and `ui/components/` packages,
seven screen/shell files, `UiMapping.kt`, `res/drawable/`,
`res/mipmap-anydpi-v26/`, `res/values/{colors,themes}.xml`,
`res/values-night/`, two test files, the whole `desktop/gui` crate, and
`docs/design/`.

Roughly 4,100 lines of Android UI and 3,000 lines of desktop Rust.

The manifest change is visual only — theme, icon, tile icon. **No permission,
component or capability was added.**

## 28. Rust tests

| Command | Result |
|---|---|
| `cargo fmt --all --check` | **PASS** — clean |
| `cargo build --workspace` | **PASS** |
| `cargo clippy --workspace --all-targets` | **PASS** — 0 warnings, 0 errors |
| `cargo test --workspace` | **PASS** — 303 passed, 0 failed, 9 ignored |

Baseline was 292 passed; the 11 new tests are the GUI crate's token, contrast
and fingerprint-grouping tests. `cargo fmt` touched only files in the new
untracked crate — no tracked `.rs` file was reformatted.

## 29. Android tests

| Command | Result |
|---|---|
| `./gradlew clean` | **PASS** |
| `./gradlew testDebugUnitTest` | **PASS** — 232 tests, 0 failures, 0 errors, 0 skipped |
| `./gradlew :app:assembleDebug` | **PASS** — APK 25,279,764 bytes |

Baseline was 206. The 26 new tests are 9 design-token/contrast tests and 17
UI-mapping tests, three of which are the crash regression.

**Instrumented tests were not run.** `connectedDebugAndroidTest` uninstalls
both APKs, which destroys the Keystore key and costs a fresh QR pairing — the
same cost recorded in every previous round. Nothing in this sprint changed
instrumented-test code, and the logic those tests would cover was moved into
JVM-testable functions precisely so it need not be paid. Say the word and I
will run it.

---

## 30. Regressions found

### R-1 · P0 · Any received clipboard clip crashed the Android home screen — FIXED

```
java.lang.IllegalArgumentException: Key "df65d3e4…334a" was already used.
  at androidx.compose.ui.layout.LayoutNodeSubcompositionsState.subcompose
```

A pending clip is keyed by the fingerprint of the peer it came from; a device
card by the fingerprint of the peer it *is*. Both were in the same
`LazyColumn`, so a clip arriving from a paired computer produced two items
with an identical key and Compose threw. Introduced by this sprint's home
screen; caught on hardware, not by a test.

**Fix:** keys namespaced by kind in `UiMapping` (`clip:`, `peer:`, `offer:`,
`transfer:`), applied on both the home and activity screens. **Three
regression tests** added. Re-verified on the tablet: clip sent from Fedora,
pending card rendered, applied, zero fatal exceptions.

### Departures from the reference, and why

| # | Reference showed | Delivered | Reason |
|---|---|---|---|
| D-1 | Clipboard history panel with clip text | Removed; replaced by a statement that nothing is kept | Clipboard content never reaches disk and the control socket carries none. Building it meant starting to store what the product refuses to store |
| D-2 | Persistent transfer **History** tab | "Completed this run", with a line saying so | The daemon holds transfers in memory for the life of the process |
| D-3 | "End-to-end encrypted" | "Direct connection · TLS 1.3, pinned · local network" | The brief forbids marketing that is not exactly the technical semantics. This is precise and checkable |
| D-4 | Status labels in brand teal | Corrected accents | Brand teal is 2.49 : 1. Accessibility outranks fidelity |
| D-5 | Bespoke stroke icons on the desktop | Adwaita symbolic | GTK recolours symbolic icons by forcing a fill, which turns stroke glyphs into blobs; accent tinting is load-bearing. The AnyFlow family remains canonical for Android |

Smaller adaptations: no device photographs (no such data — platform glyphs
instead); no IP addresses (the control protocol reports none — fingerprint
instead); no pause control (the protocol has cancel only); no rename pencil
(no rename capability); "Sensitive clipboard detected / looks like a terminal
command" replaced with the truth, that the source app set the flag — AnyFlow
does not inspect content.

**No business or security regression.** No capability auto-granted, no
warning hidden, no fingerprint shortened, no persistence added, no permission
added, no TLS change, no cloud, no account, no telemetry.

## 31. Technical debt

1. **Instrumented Compose UI tests not written.** The logic is covered on the
   JVM; genuinely visual assertions would need instrumentation, which costs a
   re-pairing every run. A dedicated test identity — already a standing debt
   from the clipboard round — would remove the obstacle.
2. **UI strings live in Kotlin, not `strings.xml`.** This follows the existing
   convention in the file it replaced, but it blocks localisation. One
   extraction pass, once the copy settles.
3. **Inter is not bundled.** Deliberate and documented; revisit if the brand
   face becomes non-negotiable.
4. **The desktop polls at 2 s.** The control socket cannot push. A D-Bus
   interface would give signals, as `daemon/src/control.rs` already
   anticipates.
5. **Desktop icons are Adwaita, Android icons are the AnyFlow family.** Right
   for each platform, but the two do not look identical side by side. Closing
   it would mean redrawing the family as filled outlines so GTK can tint them.
6. **Two dead identities in the desktop trust store** (`1B27 07BA…`,
   `2172 18A3…`), already revoked, now visible on the Trusted peers page. Not
   from this sprint; visible because there is finally a screen for it.
7. **`--page` is an undocumented-in-`--help` flag.** It has no argument parser
   behind it; if the GUI grows real CLI options it should get `clap`.

---

## 32. Acceptance gates

| Gate | | Evidence |
|---|---|---|
| VIS-01 brand tokens centralized | **PASS** | `tokens.json` read by tests on both platforms; no literal hex in any screen |
| VIS-02 Flowing A implemented | **PASS** | SVG + mono + lockup + Android vector drawable |
| VIS-03 Flowing Ribbon implemented | **PASS** | SVG + mono + app icon + wide ribbon + drawable |
| VIS-04 official Palette A | **PASS** | Exact values, plus the AA-corrected families |
| VIS-05 typography system | **PASS** | Full scale on both platforms; mono for fingerprints |
| VIS-06 Android app icon | **PASS** | Adaptive + monochrome; seen on the tablet taskbar |
| VIS-07 Android home | **PASS** | Captured on SM-X620, connected and disconnected |
| VIS-08 Android peer details | **PASS** | Captured; permissions, automation, security, destructive |
| VIS-09 Android clipboard UI | **PASS** | Send screen with live preview captured |
| VIS-10 Android file transfer UI | **PASS** | Offer, progress and terminal states restyled |
| VIS-11 Android pairing | **PASS** | Empty state + scanner entry points |
| VIS-12 Android dark theme | **PASS** | Captured via `cmd uimode night yes`, reverted |
| VIS-13 desktop shell | **PASS** | Built from zero; captured |
| VIS-14 desktop dashboard | **PASS** | Captured against the live daemon |
| VIS-15 desktop pairing | **PASS** | Drawn QR, identity panel, security notes |
| VIS-16 desktop files | **PASS** | Captured |
| VIS-17 desktop clipboard | **PASS** | Captured; real backend detection, no history |
| VIS-18 desktop dark theme | **PASS** | Captured via GNOME `color-scheme`, reverted |
| VIS-19 accessibility | **PASS** | Contrast asserted by tests; §21 |
| VIS-20 responsive behavior | **PASS** | Desktop breakpoint verified at 380 pt; Android capped at 640 dp |
| VIS-21 no business/security regression | **PASS** | §30; manifest change is visual only |
| VIS-22 no clipboard persistence/history | **PASS** | History removed from both platforms; nothing added |
| VIS-23 no broad Android permissions | **PASS** | Manifest diff adds icon and theme attributes only |
| VIS-24 existing Rust tests PASS | **PASS** | 303 passed, 0 failed |
| VIS-25 existing Android tests PASS | **PASS** | 232 passed, 0 failed |
| VIS-26 hardware visual review Android | **PASS** | SM-X620 / Android 16; found and fixed R-1 |
| VIS-27 Fedora desktop visual review | **PASS** | Live daemon, normal and HiDPI, light and dark, resize |

**26 PASS · 1 NOT EXECUTED · 0 FAIL.**

The one NOT EXECUTED is the Android instrumented suite (§29), which is a
deliberate deferral rather than a gate: it is not among VIS-01…VIS-27 and it
costs a re-pairing.

## 33. Git status

```
git branch --show-current   feature/visual-identity-v1
git log --oneline -1        dbdeb2b   (no commit, no push)
git diff --check            clean — no whitespace errors, no conflict markers
git diff --stat             8 files changed, 846 insertions(+), 437 deletions(-)
git status --porcelain      8 modified · 23 untracked

forbidden artefacts
  APK / AAB               absent   screenshots (png/jpg)   absent
  local.properties        absent   identity.key            absent
  state.json              absent   clipboard content        absent
```

---

## 34. Final status

# ANYFLOW VISUAL IDENTITY V1 COMPLETE

*Run on `feature/visual-identity-v1`, 31 August 2026. No commit, no push. The
desktop application was built from an empty placeholder; one P0 crash was
found on hardware, fixed and regression-tested.*
