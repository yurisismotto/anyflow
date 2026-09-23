# OmniBridge Rebrand v1 — Visual Closure + Physical Smoke

**Branch:** `feature/omnibridge-rebrand-v1`
**Date:** 2026-09-21
**Verdict:** **OMNIBRIDGE REBRAND V1: READY FOR COMMIT** — the official artwork was supplied on 2026-09-21 and integrated; the S1–S18 physical smoke passed earlier the same day.

> **Run 2 (artwork integration) supersedes the artwork sections below.** Sections 2–5 and 15 recorded the state when no vector source existed. What they describe was resolved when the approved assets arrived: see *§19 — Artwork closure*.

Nothing was committed, pushed or proposed as a PR. The index is empty.

---

## 1 — Index / unstage result

`git reset` (index only). No `--hard`, no `checkout`, no `restore`, no `clean`.

| | Before | After |
|---|---|---|
| Staged entries | 166 | **0** |
| Working-tree entries | 450 | 459 |

The count rises because each `git mv` that was staged as one rename becomes two
working-tree entries once unstaged: the old path as ` D`, the new path inside an
untracked directory. No file content was lost — spot-checked that the renamed
sources exist at their new paths and the old directories are gone.

Final working tree: **277 modified, 166 deleted (old paths), 16 untracked**.
`git diff --check` is clean. `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not touched
(still untracked, mtime 2026-09-15).

---

## 2 — Canonical OmniBridge visual asset source — **BLOCKED**

An approved reference **was** supplied and inspected before anything was derived
from it. It is raster only.

| | |
|---|---|
| File | `~/Downloads/ChatGPT Image 21 de set. de 2026, 12_31_10.png` |
| Format | PNG, 1672 × 941, 8-bit RGB, non-interlaced |
| Chunks | `IHDR`, `caBX`, `IDAT` × 23, `IEND` — no vector payload, no path data |
| Provenance | C2PA manifest in `caBX`: `softwareAgent: ChatGPT`, `version: gpt-image`, `digitalSourceType: …/trainedAlgorithmicMedia`, signed `2026-09-21T05:30:40Z` |
| Vector source | **none** — no `.svg`, `.ai`, `.eps`, `.pdf` or design-tool file exists anywhere on this machine |

The brief permits deriving a canonical asset only where that can be done
"deterministically … without altering the approved design". It cannot be, here:

1. **The pixels are the only original.** The image was synthesised, not rendered
   from geometry. There is no path to recover; a trace would invent control
   points and gradient stops and then present them as approved.
2. **The renderings disagree.** The sheet draws the chosen mark five times —
   hero, horizontal logo and three app icons — and no two share an outline.
3. **The sheet contradicts itself.** Inside the panel marked CHOSEN DIRECTION,
   the tile captioned SYMBOL ONLY shows a plain arch, which is essentially the
   sheet's own *rejected* alternate 01 "Bridge Arc" — not the curled mark the
   hero, the lockup and all three app icons use.
4. **Resolution.** The largest instance is ~250 px tall, with soft gradient
   shading and overlapping translucent ribbons that no flat vector reproduces.

Per the brief the run **stopped** rather than pass a traced approximation off as
canonical. No symbol was invented, redrawn, downloaded or traced.

### What the reference does settle, deterministically

Stated as text in the sheet, so no tracing risk:

| Token | Value |
|---|---|
| Wordmark | OmniBridge |
| Tagline | One bridge. Any device. |
| Typeface | Inter |
| Primary | `#4F6BFF` |
| Bridge Cyan | `#18B8C9` |
| Accent Violet | `#7C5CFC` |
| Dark | `#0B1020` |
| Surface | `#F7F9FC` |

The tagline matches what is already live on every active product surface.

### What unblocks it

One vector file — `.svg`, `.ai`, `.eps` or a Figma/Illustrator export — of the
**curled mark** as drawn in the hero and the app icons. Dropped in as
`docs/design/assets/app-icon.svg`, the existing build derives every other
platform asset from it automatically (see §4).

---

## 3 — Android icon result — **BLOCKED, pipeline verified**

No icon was changed. The pipeline that will consume the approved asset was
verified end to end and is correct:

* `res/mipmap-anydpi-v26/ic_launcher.xml` and `ic_launcher_round.xml` declare
  `background` + `foreground` + **`monochrome`** — the themed-icon slot already
  exists;
* `drawable/ic_launcher_foreground.xml` carries `pathData`/`strokeWidth` that are
  byte-for-byte the `d` and `stroke-width` of `docs/design/assets/logo-flow-a.svg`;
* `BrandingResourcesTest` asserts that equality, so Android and the desktop
  cannot drift apart a curve at a time.

Today that shared path is still the **Flow A**. Swapping the canonical SVG moves
all three platforms at once, which is why no per-density hand-editing is pending.

---

## 4 — Linux icon result — **name correct, content blocked**

`desktop/gui/build.rs` copies `docs/design/assets/app-icon.svg` to
`icons/scalable/apps/io.github.yurisismotto.omnibridge.svg` at build time, so the
**filename and theme path are already APP_ID-correct** and resolve from
`GtkIconTheme`.

The file's *contents* are still AnyFlow's: `aria-label="AnyFlow"`,
`<title>AnyFlow app icon</title>`, gradient id `anyflow-appicon`, Flow A geometry.

One canonical source, three platforms — exactly the single-source-of-truth shape
the brief asks for. Only the source is missing.

---

## 5 — Wordmark / lockup result — **BLOCKED**

`wordmark.svg`, `wordmark-mono.svg` and `logo-lockup.svg` still render the word
**AnyFlow** as live text, and `logo-lockup.svg` still carries *One flow. Any
device.* They were left visibly wrong rather than relabelled, so retained artwork
cannot silently read as an adopted OmniBridge identity.

The wordmark is set in Inter and could be typeset faithfully, but the **lockup
needs the symbol**, so both were held together rather than shipping half a brand.

---

## 6 — Active-brand audit (§4 gate)

Every active product surface presents as OmniBridge. Verified live, not by grep:

| Surface | Value | |
|---|---|---|
| Android app label | `OmniBridge` | ✅ |
| Android system dialogs | "Permitir que o app **OmniBridge** …" | ✅ |
| Android screens | "Run \`omnibridge pair\`…", "OmniBridge talks straight to your computer over TLS 1.3…" | ✅ |
| Android notification channels | "Connection status" / "File transfers" / "Clipboard" — brand-neutral | ✅ |
| Quick Settings tile | "Send clipboard" — brand-neutral | ✅ |
| Linux tray `Id` | `io.github.yurisismotto.omnibridge` | ✅ |
| Linux tray `Title` | `OmniBridge` | ✅ |
| SNI `ToolTip` | `OmniBridge` / `One bridge. Any device.` | ✅ |
| SNI `IconName` | `io.github.yurisismotto.omnibridge` | ✅ |
| `.desktop` | `Name=OmniBridge`, `Icon=io.github.yurisismotto.omnibridge`, `Exec=omnibridge-gui`, `StartupWMClass=omnibridge-gui`, Quick Panel action | ✅ |
| D-Bus service | `Name=io.github.yurisismotto.omnibridge` | ✅ |
| CLI `--help` | "OmniBridge control" | ✅ |
| CLI `--version` | `omnibridge 0.1.0` | ✅ |
| Daemon `--help` | "OmniBridge daemon" | ✅ |

**No active OmniBridge surface presents itself as AnyFlow in text.** The gap is
artwork only: the in-app mark, the empty-state illustration and the launcher icon
are still the AnyFlow ribbon family.

---

## 7 — Android tests / build

```
:app:testDebugUnitTest  :app:assembleDebug  :fixture:assembleDebug
BUILD SUCCESSFUL — exit 0
```

51 test classes, **771 tests, 0 failures, 0 errors, 0 skipped**.

Both APKs built and their identities read back with `aapt2`:
`io.github.yurisismotto.omnibridge` and `io.github.yurisismotto.omnibridge.fixture`.

---

## 8 — Rust fmt / test / clippy / build

Run sequentially, never concurrently with Gradle.

| Step | Exit | |
|---|---|---|
| `cargo fmt --all --check` | 0 | ✅ |
| `cargo test --workspace -j 2` | 0 | **981 passed, 0 failed** |
| `cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings` | 0 | 0 warnings |
| `cargo build --release -j 2 -p omnibridge-daemon -p omnibridge-cli -p omnibridge-gui` | 0 | `omnibridged`, `omnibridge`, `omnibridge-gui` |

---

## 9 — SM-X620 physical smoke, S1–S18

Device `RX2Y500C7SY` (SM-X620). Installed **side-by-side**; the AnyFlow-era app,
its data and its notification-listener grant were all left in place.

```
io.github.yurisismotto.anyflow           ← preserved
io.github.yurisismotto.anyflow.fixture   ← preserved
io.github.yurisismotto.omnibridge        ← new
io.github.yurisismotto.omnibridge.fixture← new
```

| | Check | Result |
|---|---|---|
| S1 | Launches with correct new branding | ⚠️ **text yes, artwork no** — every string is OmniBridge; the mark and empty-state illustration are still AnyFlow's |
| S2 | Identity `io.github.yurisismotto.omnibridge` | ✅ `MainActivity` focus, `aapt2`, and Android's own permission dialogs name "OmniBridge" |
| S3 | Notification listener granted | ✅ in-app status reads *Notification access: Allowed*; AnyFlow's grant preserved alongside |
| S4 | Desktop advertises `_omnibridge._tcp.local.` | ✅ `avahi-browse`, IPv4 + IPv6, port 55432; **no** `_anyflow._tcp` |
| S5 | Android discovers the desktop | ✅ `DISCOVERY round=1 endpoints=1` → `CONNECT_ATTEMPT 192.168.68.73:55432`; device row later re-read *Available* from discovery alone |
| S6 | QR payload uses `omnibridge1:` | ✅ live payload from `omnibridge pair` |
| S7 | Fresh QR pairing | ✅ scanned on the tablet; `Paired with 509B D0C1 CE97 C909` |
| S8 | Proof-of-possession | ✅ *"A device proved it holds the pairing code"* — device id `a8c964c4…`, verified against the tablet's own trust store before confirming |
| S9 | SPKI pinning | ✅ tablet pins `149f6b66…2d5b`; the app's Security panel shows `149F 6B66 AB5D 8526`; `openssl` without a client cert is refused with *certificate required* (alert 116) |
| S10 | Control ALPN `omnibridge/1` | ✅ negotiated on the wire |
| S11 | Data ALPN `omnibridge-data/1` | ✅ negotiated on the wire, and exercised by the S16 transfer |
| S12 | Device becomes connected | ✅ `SESSION_START` / `state connected` |
| S13 | `battery.v1` | ✅ live both ways — desktop reads 80%, tablet logs the desktop's level |
| S14 | Android → desktop clipboard | ✅ `applied 23 bytes to the clipboard`; desktop clipboard verified replaced |
| S15 | Desktop → Android clipboard | ✅ `outcome=PENDING_USER`, then applied on the tablet and previewed back |
| S16 | Small file transfer | ✅ `state completed`; landed in `Download/OmniBridge/`; **SHA-256 match** |
| S17 | Fixture notification → desktop | ✅ `NOTIFICATION_OUTCOME_DISPLAYED`; desktop reports *showing 1 of 1 mirrored* |
| S18 | Disconnect / reconnect | ✅ connected → disconnected → connected in ~10 s, all four grants preserved |

Capability grants had to be made on **both** ends — the desktop with
`omnibridge grant`, the tablet by tapping its own per-device permission
toggles. Until the tablet granted it, an incoming clip was refused
`outcome=NOT_AUTHORIZED`, which is the consent model behaving correctly rather
than a defect. Notification mirroring additionally required picking an app:
turning the permission on shares nothing by itself.

Clipboard, file and notification payloads are deliberately not reproduced here.

### AnyFlow-era state preserved

Verified after the run: both AnyFlow packages still installed, the AnyFlow
notification-listener grant still present, the AnyFlow app's `trust-store.json`
intact, and `~/.local/share/anyflow/` untouched (`identity.key` 2026-08-30,
`state.json` 2026-09-17). The desktop's own data dir moved to
`~/.local/share/omnibridge`, so the OmniBridge identity is genuinely fresh.

---

## 10–12 — Pairing / ALPN / mDNS / QR, and clipboard-file-notification smoke

Pairing completed on the first scan. Desktop `7ca9ec07…` (`149F 6B66 AB5D 8526`)
↔ tablet `a8c964c4…` (`509B D0C1 CE97 C909`).

ALPN was proven on the wire rather than inferred, with `openssl s_client`
against the live daemon:

| Offered ALPN | Result |
|---|---|
| `omnibridge/1` | **negotiated**, then TLS alert 116 *certificate required* — mutual auth intact |
| `omnibridge-data/1` | **negotiated**, same client-cert demand |
| `anyflow/1` | TLS alert 120 *no application protocol* — **refused** |
| `anyflow-data/1` | TLS alert 120 *no application protocol* — **refused** |

That is §7's negative check demonstrated rather than asserted: an AnyFlow-era
peer is turned away at the TLS layer, before a byte of application data, and no
compatibility alias was added to soften it.

| Identifier | Value | Pinned by |
|---|---|---|
| Control ALPN | `omnibridge/1` | `wire_identity.rs` + `WireIdentityTest.kt`, **and observed live** |
| Data ALPN | `omnibridge-data/1` | same, **and observed live** |
| mDNS service | `_omnibridge._tcp.local.` | same, **and observed live** |
| QR prefix | `omnibridge1:` | same, **and observed live** |

`no_pre_rename_identity_survives` asserts none of them contains `anyflow` or
`fedroid`.

---

## 13–14 — Final AnyFlow remainder audit

Binary-safe, and built independently of the previous run: file list from
`git ls-files -co --exclude-standard` (533 files present on disk), scanned with
`env LC_ALL=C grep -ail anyflow`.

> `LC_ALL=C` and `-a` matter. The two committed DER certificates contain NUL
> bytes; in a UTF-8 locale a plain `grep -ril` reports no match on them and
> returns 66 files, missing both.

| Category | Files |
|---|---|
| 1 — historical certification evidence (31 root reports + 2 sprint reports + ADR-0011) | 34 |
| 2 — migration note / decision record / the audit itself | 5 |
| 3 — intentional legacy-development documentation, in code comments | 7 |
| 4 — GitHub repository URL, pending manual repo rename | 8 |
| 5 — content that must remain (14 artwork files + 2 frozen DER vectors) | 16 |
| **Total unique files** | **69** |
| **UNEXPLAINED ACTIVE-PRODUCT OCCURRENCES** | **0** |

This reproduces the prior run's 69 exactly. `OMNIBRIDGE-REBRAND-REMAINDER-AUDIT.md`
was updated with the re-verification and a rewritten §5 artwork section recording
the supplied reference and why it could not be vectorised.

Every active identifier is clean **by test rather than by grep**: ALPN ×2, mDNS
type, QR prefix, the protobuf package of all six schemas, the desktop app id,
`.desktop` `Icon=`, D-Bus `Name=`/`Exec=`, SNI `Id`/`IconName`, the Android
`applicationId` and every manifest component, and the six HMAC domain separators.

---

## 15 — Artwork remainder gate (§9)

Active product artwork **still contains AnyFlow identity**, and is the blocker:

* 12 SVGs under `docs/design/assets/` — `app-icon.svg`, the Flow A and flowing-A
  families, `wordmark{,-mono}.svg`, `logo-lockup.svg`, `ribbon-connection.svg`,
  `icon-flowing-ribbon{,-mono}.svg`;
* 2 Android drawables — `logo_flow_a.xml`, `logo_flowing_ribbon.xml`;
* `logo-lockup.svg` still reads *One flow. Any device.*

All 14 are classified **BLOCKED_VISUAL_ASSET**, kept deliberately unrelabelled.
The 28-glyph UI icon family under `docs/design/assets/icons/` is brand-neutral and
is **not** in this category.

---

## 16 — Historical evidence preservation

Untouched: all 31 root-level certification reports, the 2 sprint reports under
`docs/sprints/`, ADR-0011, and the frozen cross-language DER vectors.
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not opened or modified.

---

## 17 — Git

```
branch                    feature/omnibridge-rebrand-v1
git diff --cached --name-only   (empty)
git diff --check                clean
git status --short              277 M · 166 D · 16 ??
git diff --stat                 443 files changed, 2571 insertions(+), 46047 deletions(-)
```

**Nothing is staged.** No commit, push, PR or repo rename was performed.

---

## 18 — Verdict

**OMNIBRIDGE REBRAND V1: READY FOR COMMIT**

The blocker that stood here — no vector source for the approved symbol — was
cleared when the official artwork was supplied. See §19.

---

## 19 — Artwork closure *(run 2, 2026-09-21)*

Six approved SVGs were supplied and installed **verbatim**, byte-identical to
what was handed over. Nothing was redrawn, traced, simplified or reinterpreted,
and the rejected S/infinity proposal from the previous run was deleted.

| Canonical source | `docs/design/assets/omnibridge-mark.svg` |
|---|---|
| Consumed | `omnibridge-mark.svg`, `-mark-mono`, `-app-icon`, `-android-monochrome`, `-wordmark`, `-logo-lockup` |
| Removed | 12 AnyFlow SVGs + `logo_flow_a.xml` + `logo_flowing_ribbon.xml` |

**Android.** `ic_launcher_foreground.xml`, `ic_launcher_monochrome.xml` and
`logo_omnibridge_mark.xml` are generated from the canonical SVG by script; every
`pathData` is byte-for-byte the SVG's own. The two radial sheens are *not*
approximated: Android composes a `<group>` as `T · R · S` about pivot (0,0),
which is the same product as the SVG's `gradientTransform`, so each sheen is a
unit-radius radial gradient inside a group carrying those exact numbers.
Placement is computed from the outline's 220 vertices — minimum enclosing circle
centre (97.33, 96.90), radius 99.37; at scale 0.32 that is 31.80 against the
33-unit round safe zone. The launcher background moved to the brand Dark
`#0B1020`.

**Linux.** `build.rs` publishes `omnibridge-app-icon.svg` to
`icons/scalable/apps/io.github.yurisismotto.omnibridge.svg`; the installed
hicolor file is byte-identical to the approved app icon. Tray `Id`, `Title`,
`IconName` and `ToolTip` all resolve to the same identity.

**Tests.** Both front ends now assert *geometry*, not filenames:
`every_derived_asset_carries_the_canonical_geometry` and
`every launcher layer carries the canonical OmniBridge outline` re-read
`omnibridge-mark.svg` and compare its ~3.2 kB outline byte for byte, so a
redrawn or retraced derivative fails the build.
`no_active_asset_carries_the_pre_rename_identity` sweeps all six assets for
`anyflow`, `flow a`, `flow-a`, `flow_a`, `one flow` and `fedroid`.

**Audit.** 69 files → **57**. The `BLOCKED_VISUAL_ASSET` category is gone rather
than reduced. **Unexplained active-product occurrences: 0. Active AnyFlow
artwork: 0.**

**Visual smoke.** V1–V5 all pass on the SM-X620 and this desktop; the tablet was
updated in place and neither the pairing nor the four capability grants were
lost.

**One delta, deliberately deferred.** The *brand palette* is now the official
OmniBridge one, but the *UI gradient tokens* are still the AnyFlow-era sweep
(`#16B8A6` → `#4F7CFF` → `#8B5CF6`) because those are code — `tokens.json`,
`Color.kt`, `theme.rs` — pinned by token tests on both front ends. Retuning them
is an interface change rather than artwork integration, and is recorded in
`docs/design/BRAND.md` as a self-contained follow-up.
