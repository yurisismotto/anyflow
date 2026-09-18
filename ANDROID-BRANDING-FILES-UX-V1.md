# AnyFlow — Android Branding + Files UX v1

**Branch** `feature/android-branding-files-ux-v1`
**Base** `develop` after PR #35 (Android Clipboard Truthfulness v1)
**Hardware** Samsung SM-X620 (Android 16 / API 36, One UI 8.0) + Fedora host
**Date** 2026-09-17 → 2026-09-18
**Status** not committed, not pushed, no PR opened

Two goals, both met:

* **A.** The Android launcher, splash and in-app identity now wear the
  official **Flow A**, generated from the same SVG the desktop uses.
* **B.** Android has a **Files** surface that shows what moved, says what
  actually happened to it, and can open a finished file when — and only when
  — Android still legitimately lets it.

---

## 1. Baseline

```
git branch --show-current   feature/android-branding-files-ux-v1
git status --short          ?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
git log -5 --oneline        d48e86e Merge pull request #35 …clipboard-truthfulness-v1
                            96f004c fix(clipboard): make Android delivery feedback truthful
                            1c6e09f Merge pull request #34 …revoked-device-cleanup-v1
                            ccab7e5 fix(ci): align revoked cleanup tests and Windows guard
                            e50a758 feat(trust): add revoked device cleanup tombstones
git diff --check            clean
```

`git merge-base --is-ancestor 96f004c HEAD` → **PR #35 is in ancestry.**
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not read, written or staged.

---

## 2. Pre-change Android branding

| Thing | Before | Verdict |
|---|---|---|
| `android:icon` | `@mipmap/ic_launcher` | fine, points at an adaptive icon |
| `android:roundIcon` | `@mipmap/ic_launcher_round` | fine |
| `mipmap-anydpi-v26/ic_launcher.xml` | `<background>`+`<foreground>`+`<monochrome>` | structurally complete already |
| `drawable/ic_launcher_foreground.xml` | **the Flowing Ribbon** | **stale mark** |
| `drawable/ic_launcher_monochrome.xml` | the Flowing Ribbon | stale mark |
| `@color/ic_launcher_background` | `#0F172A` Ink | correct |
| adaptive / round / themed support | all three present | correct |
| bitmap mipmaps | none — `anydpi-v26` only, `minSdk = 29` | correct; every supported device gets the vector |
| app label | `@string/app_name` = "AnyFlow" | correct |
| splash | no custom splash; Android 12+ default | correct, inherits the icon |
| in-app institutional mark | `logo_flowing_a.xml` on Settings/About | **stale mark** |

So the Android icon *architecture* was already right. What was wrong was the
**artwork inside it**: Android was still wearing the mark the product had
moved off.

### Where the drift came from

Commit `5bcda90 feat(desktop): add quick panel and AnyFlow branding` (PR #33)
added `logo-flow-a.svg`, `logo-flow-a-mono.svg`, `logo-flow-a-small.svg` and
**rewrote `app-icon.svg`** from the ribbon geometry to the Flow A:

```diff
-  <g transform="translate(256,256) scale(6.6) translate(-32,-32)">
-    <path d="M14 44 C 30 44, 24 21, 50 22" … />
-    <circle cx="14" cy="44" r="7" …/><circle cx="50" cy="22" r="7" …/>
+  <g transform="translate(254,258) scale(6.2) translate(-32,-32)">
+    <path d="M 53 56 C 50 41 43 22 32 8 …" stroke-width="8" …/>
```

The desktop followed and pinned it
(`desktop/gui/tests/brand_assets.rs::the_primary_application_icon_is_the_flow_a`).
Android did not follow, and nothing was checking, because an app icon's
correctness is normally established by somebody glancing at a home screen.

### A note on BRAND.md

`docs/design/BRAND.md` still says, under **Misuse**, "do not put the Flowing A
on an app icon". That rule is about the **Flowing A** (`logo-flowing-a.svg`),
a *different, older* mark from the **Flow A** (`logo-flow-a.svg`). BRAND.md
was not updated by PR #33 and its Assets table does not list the Flow A files
at all. This sprint does **not** edit BRAND.md — correcting the brand book is
a decision for whoever owns the brand, not a side effect of an Android sprint
— but the staleness is recorded as a debt in §30.

---

## 3. Flow A source asset

Source of truth: **`docs/design/assets/logo-flow-a.svg`**, 64-unit grid.

```
d            M 53 56 C 50 41 43 22 32 8 C 22 21 15 36 10 47
             C 7.5 52.5 11.5 56.5 15.5 52 C 22 43.5 33 38 45 37.5
             C 50 37.3 54 38 57 39
stroke-width 8, round caps, round joins, fill none
gradient     (10,56) → (54,8), #16B8A6 → #4F7CFF → #8B5CF6
```

One path, one subpath — the ribbon never lifts. That is why it survives
monochrome, and why it converts to a VectorDrawable without loss.

**Geometric suitability for an adaptive icon**, computed from the path itself
(cubics flattened at 4000 points/segment, stroke counted):

| Measurement | Value |
|---|---|
| stroked bounding box | 55.776 × 56.000 of the 64 grid |
| minimum enclosing circle of the ink | centre (32.81, 36.24), **r = 32.25** |

An adaptive icon guarantees the inner **72 × 72** square of a 108 canvas, and
for a round mask only a **66-unit circle** about the centre (r = 33). The Flow
A's ink radius is 32.25 — it fits the stricter of the two **at its native
size**. The mark is geometrically ideal for this; nothing had to be shrunk.

`app-icon.svg` was *not* used as the Android source: it is a 512 tile with its
own rounded-rect ground, and an adaptive icon supplies its own ground. The
canonical **path** is the shared thing, which is exactly what the test pins.

---

## 4. Adaptive icon architecture

```
mipmap-anydpi-v26/ic_launcher.xml        ─┬─ background @color/ic_launcher_background (#0F172A Ink)
mipmap-anydpi-v26/ic_launcher_round.xml  ─┘   foreground @drawable/ic_launcher_foreground
                                              monochrome @drawable/ic_launcher_monochrome
```

Both mipmap XMLs are unchanged. Only the two drawables were rewritten.

`drawable/ic_launcher_foreground.xml`:

```xml
<vector … android:viewportWidth="108" android:viewportHeight="108">
    <group android:translateX="21.2" android:translateY="17.8">
        <path android:pathData="M 53 56 C 50 41 …"
              android:strokeWidth="8" strokeLineCap="round" strokeLineJoin="round">
            <aapt:attr name="android:strokeColor"><gradient …/></aapt:attr>
```

**The group scales by exactly 1 and only translates.** That is the design
decision worth defending: at scale 1 the numbers in the drawable are the
numbers in the SVG — stroke 8 here, stroke 8 there — so "the stroke weight is
what makes the marks a family" (BRAND.md) is preserved by arithmetic rather
than by anyone remembering to rescale it.

The translate `(21.2, 17.8)` puts the ink's minimum-enclosing-circle centre on
the canvas centre (54, 54) — **not** the bounding-box centre, because the A's
apex and its out-flung crossbar terminal are not symmetrical. Placing the
enclosing circle instead of the box is what buys the clearance:

| Mask | Limit | Actual | Slack |
|---|---|---|---|
| 72 × 72 square safe zone | ink within [18, 90] | [21.80, 82.20] | **+3.80** |
| 66-unit round safe zone | r ≤ 33 | r = 32.29 | **+0.71** |

Both are **recomputed by the test** from the drawable's own path and group —
not trusted from the comment. Editing the icon and getting the placement wrong
breaks the build, not the home screen.

No SVG feature was lost in conversion: the path is `M` + `C` only, the caps
and joins map 1:1, and the linear gradient maps to `<gradient>` with the same
user-space endpoints. There is nothing to document as an approximation, and
the canonical SVG remains the source.

---

## 5. Monochrome / themed icon

`drawable/ic_launcher_monochrome.xml` is the foreground with the paint
replaced and **nothing else changed** — same path, same stroke width, same
group translate — so a themed launcher draws the same mark in the same place
rather than a second drawing of it that can drift.

It names exactly one colour (`#FF000000`) and no gradient. Android tints this
layer by taking its **alpha** and painting the system's own colour through it;
a gradient would be discarded, so declaring one would be a file claiming a
colour it never gets to keep. Asserted:

* no `<gradient>` anywhere;
* none of `#16B8A6`, `#4F7CFF`, `#8B5CF6` present;
* the set of distinct colours in the file has size **1**.

---

## 6. Manifest and splash changes

**Manifest: no change was needed and none was made.** `android:icon`,
`android:roundIcon` and `android:label` already pointed at the right
resources; only the artwork behind them was stale.

**Splash: no change, deliberately.** `targetSdk = 35`, so Android 12+ draws
the system splash from the launcher icon over the theme's `windowBackground`
(Paper `#F8FAFC` light / `#0A0F1C` dark, both already the app's own colours).
No custom splash Activity was created and none should be.

The one thing to get right was the icon, and it is right for free: the system
splash shows the **inner two thirds** of the adaptive foreground, which is the
same 72-of-108 the square safe zone already answers for. Certified on hardware
in §23 (I5). Startup is unchanged — nothing was added to the start path.

---

## 7. Pre-change file-transfer state model

`FileTransferManager` is created in `AnyFlowApp.onCreate()` and holds
`ConcurrentHashMap<String, Transfer>` keyed by lowercase-hex transfer id. It
publishes two `StateFlow`s: `visible` (all transfers) and `pendingOffers`.

Per transfer, **before** this sprint:

| Field | Internal `Transfer` | Published `TransferUi` |
|---|---|---|
| transfer id | ✔ | ✔ |
| filename (sanitized) | ✔ | ✔ |
| size, bytes moved | ✔ | ✔ |
| direction (`sending`) | ✔ | ✔ |
| state + failure reason | ✔ | ✔ |
| **peer fingerprint** | ✔ | ✘ |
| **MIME type** | ✔ | ✘ |
| **source URI** (outgoing) | ✔ | ✘ |
| **received file URI** | `pending` — **set to `null` on completion** | ✘ |
| **name actually saved** | read into a local, then discarded | ✘ |
| completion time / order | ✘ | ✘ |

Receive destination: `MediaStore.Downloads`, `RELATIVE_PATH =
Download/AnyFlow`, written with `IS_PENDING = 1`, hash-checked, then published
by clearing `IS_PENDING`. No storage permission is held or needed.

### The ten audit questions

1. **What information exists after a transfer finishes?** Id, filename, size,
   bytes, direction, terminal state, failure reason. Internally also the peer
   and MIME type. Nothing about *where the file went*.
2. **Which parts are only in memory?** All of it.
3. **What survives process restart?** Of the transfer list, **nothing**. Of
   the data, the received *file* — it is in public `Download/AnyFlow` and
   survives even uninstall.
4. **Does AnyFlow own received bytes, or another provider?** MediaStore holds
   the row; AnyFlow is its **owning package**, so it can read it back with no
   grant and no permission. The bytes live in public Downloads, not in app
   storage.
5. **Does it retain a usable `content://` URI for outgoing files?** It retains
   the `Uri` for the life of the in-memory transfer. Whether it is *usable* is
   a separate question — see 6.
6. **Does permission survive the sending Activity?** **Not reliably, and never
   guaranteed.** Two paths, two lifetimes: a Sharesheet `ACTION_SEND` grant
   belongs to `SendActivity` and dies with it; an in-app `OpenDocument` grant
   lives as long as `MainActivity`'s **task**. Neither is persistable unless
   taken, and nothing takes it. Certified empirically in §24 (F11/F12).
7. **Could a received file be opened?** No. The URI was discarded at
   completion, so there was nothing to open with.
8. **Could a sent source file be reopened?** No — the URI existed internally
   but was never surfaced.
9. **Is there a FileProvider?** **No.** No `<provider>` of any kind in the
   manifest.
10. **What would require new persistence?** Anything surviving process
    restart. See §9 — it was not needed and was not added.

---

## 8. Transfer metadata availability

`TransferUi` was widened to carry what a Files row needs, and deliberately not
more:

```kotlin
data class TransferUi(
    transferId, peer: Fingerprint, filename, sizeBytes, bytesTransferred,
    sending, state, failure,
    savedName: String?,          // what MediaStore actually called it
    mimeType: String,
    hasOpenTarget: Boolean,      // structural: something was retained
    accessLost: Boolean,         // a check looked, and it was gone
    sequence: Long,              // monotonic: created
    settledSequence: Long?,      // monotonic: ended
)
```

**No `Uri` is in it, in either direction.** The screen is built entirely from
this type, so anything in it can reach a row, a log line or a screenshot — and
a `content://` URI routinely embeds a document id or an account. What the
screen needs is whether opening is *possible*, which is a boolean. The URI
stays inside the manager and is fetched at the instant of a tap.

No SHA-256 either: the digest is how the bytes were verified, not something a
person needs, and a hash of a file someone chose is an identifier for it.

Two ordinals rather than a clock. `System.currentTimeMillis()` can repeat or
go backwards, and two files offered in the same millisecond — which the share
sheet's multi-select does routinely — would have no defined order at all.

Ordering also *changed*: `publishState()` used to sort by **filename**, which
put two unrelated transfers next to each other because they happened to share
a name, and moved a row when an unrelated one arrived. It now sorts by
`sequence`.

---

## 9. Persistence decision

**No new persistence. Nothing is written to disk. Decision-gate branch (A).**

Completed transfer information already stays in application memory for the
life of the process, which is what the brief's preferred path asks for. The
Files UX is built on that truth and on nothing else.

This is also the product's existing, deliberate position — the screen it
replaces carried the comment *"AnyFlow has no history to show … a History tab
would either be permanently empty or would have to be fed by starting to store
what the product deliberately does not store."* Adding a store would have
reversed a stated product decision to make a list look longer.

Two things make session-scoped honest rather than a shortfall:

* **the file is not the list.** A received file is in `Download/AnyFlow` and
  outlives the app entirely. "The list is gone" and "your file is gone" are
  very different pieces of news, and the UI says the first while denying the
  second;
* **the UI labels it**, in two places, in resources:
  * empty state — *"Files you send or receive appear here while they are
    moving, and stay listed until AnyFlow closes."*
  * under Recent — *"Transfers from this session. AnyFlow keeps no history of
    what you send or receive — received files stay in Downloads/AnyFlow."*

Nothing is called "history" anywhere in the product.

Because nothing is persisted, the "never persist" list is satisfied
structurally rather than by policy: no file contents, no hashes, no pairing
material, no challenge, no proof, no TLS material, no paths, no clipboard
content, and no second AnyFlow-owned copy of anything.

---

## 10. Files screen architecture

```
FileTransferManager.visible : StateFlow<List<TransferUi>>
            │
            ▼
FilesMapping.build(transfers, peers) ──► FilesUi(active, recent)   ← pure, JVM-testable
            │                                    │
            │                              FileRow (no Uri, no path)
            ▼                                    ▼
FilesScreen (Compose)  ── renders ────────► AnyFlowTransferCard
            │
            └─ onOpenTransfer(transferId) ──► MainActivity.openTransfer
                                                   │
                                          FileTransferManager.resolveOpen(id)
                                                   │  (probes the platform NOW)
                                          Ready(uri, mime) │ Unavailable(OpenAction)
```

Nothing on the screen decides whether a transfer succeeded, what it is called,
whose it is, or whether it can be opened. `FilesMapping` does, in plain
functions over plain data — which is what lets 31 assertions run in
milliseconds on the JVM instead of on a tablet whose instrumented run destroys
the pairing.

New files:

| File | Role |
|---|---|
| `ui/FilesScreen.kt` | the Compose surface (replaces `ActivityScreen.kt`) |
| `ui/FilesMapping.kt` | rows, status vocabulary, ordering, peer labels — pure |
| `files/OpenAction.kt` | the Open decision seam — pure |
| `files/MimeTypes.kt` | media-type sanitising — pure |

---

## 11. Active / recent mapping

```
Files
 ├ Active   incoming offers awaiting an answer (accept/reject card)
 │          + transfers in flight            ordered by `sequence` desc
 └ Recent   terminal transfers                ordered by `settledSequence` desc
            + the session-scope note
```

Two different ordinals on purpose: `sequence` is when a transfer *started*,
`settledSequence` when it *ended*, and those are not the same order — a large
file offered first routinely finishes last.

Status vocabulary, every value derived from a state the machine actually
reaches:

| Shown | From |
|---|---|
| Waiting | `OFFERED` / `WAITING_ACCEPT` |
| Sending / Receiving | `TRANSFERRING`, `VERIFYING` + direction |
| Sent / Received | `COMPLETED` + direction |
| Declined | `CANCELLED` + `DECLINED_BY_USER` |
| Cancelled | `CANCELLED` + anything else |
| Timed out | `FAILED` + `TIMED_OUT` |
| Disconnected | `FAILED` + `TRANSPORT` |
| Failed | `FAILED` + anything else |

Nothing consults bytes transferred, whether a file exists, or how long ago it
was. A transfer that moved every byte and then failed its hash check is
`FAILED` and stays that way.

Each row shows direction (in **words**, plus an icon and accent), display
name, `From <device>` / `To <device>`, size, status, and an action when one is
genuinely available.

---

## 12. Transfer identity

**`TransferId`, everywhere, and nothing else.** Rows are keyed by
`UiMapping.transferKey(transferId)`. `FilesMapping` contains no comparison
against a filename — asserted by a source check (T19) that forbids
`filename ==`, `displayName ==`, `filename.equals` and
`first { it.filename`, because UX-DEBT-01 was exactly
`transfers.filter { it.filename == name }` and a Files screen is the obvious
place for it to come back.

Proven on hardware: two transfers of `photo-note.txt`, one **Sent** with an
Open action and one **Declined** with none, side by side as two rows. The log
says the same thing — `offering photo-note.txt … as f4afcc7b` and
`… as 3ed9f8d5`.

The Open action is keyed to the transfer id as well: `onOpenTransfer` takes an
id and nothing else — not a `Uri`, not a filename.

---

## 13. Peer identity

Fingerprint-based. `FileRow.peerHex` is the identity; `FileRow.peerLabel` is
display only and nothing routes on it.

Where two trusted computers share a display name, the label becomes
`"<name> · <short fingerprint>"` — the same presentation the pairing screen
compares — and **never an IP address**, which names a position on a network
rather than a machine and changes on its own. Asserted in T4, including that
no label contains `192.168`.

A peer that is not in the trust list at all — revoked mid-transfer — still
gets a row, labelled with its short fingerprint. Dropping it would be hiding a
transfer that really happened.

---

## 14. Received-file Open path

The received file is a `MediaStore.Downloads` item **AnyFlow itself inserted**.
Two consequences:

* AnyFlow can read it back with **no grant and no permission** — being the
  owning package is enough;
* it is already a `content://` URI in public storage. There is nothing to copy
  and nothing to wrap.

The one change needed was to **stop throwing it away**. `receiveBytes` used to
do `transfer.pending = null` after publishing; it now also records
`transfer.openUri = pending.uri` and `transfer.savedName = actualName`.

That the URI is retained *only after publication* is load-bearing: before that
line it is a pending row no other app can see and that `finish()` deletes, and
offering to open unverified bytes is precisely what `IS_PENDING` exists to
prevent. `finish()` was extended to clear `openUri` when it discards a pending
entry, so a failed hash check cannot leave a row pointing at bytes that did
not verify.

`savedName` matters in practice: MediaStore renames a duplicate rather than
overwriting, and a row showing the *offered* name would send someone hunting
for a file that is not there. Certified — the second copy displayed as
**`pasture-map (1).txt`**.

Reachability is checked by asking MediaStore for the cheapest column there is
(`_ID`). A deleted item returns no row. Nothing is read and no stream opened.

---

## 15. Sent-file Open path

The outgoing URI is whatever the person shared in. AnyFlow holds the `Uri`,
which is **not** the same as holding permission — so the grant is re-checked
with `checkUriPermission(uri, myPid, myUid, FLAG_GRANT_READ_URI_PERMISSION)`
at the moment of the tap, and again whenever the Files screen appears.

When the grant is gone the row reports **`SourceUnavailable`** — deliberately
not "file missing". The file is almost certainly still on the device and
AnyFlow simply may not look at it; telling someone it is missing would send
them hunting for something that is not lost.

**Nothing is ever recovered by name.** There is no filename lookup, no path
reconstruction, no display-name match. Certified in the sharpest possible
form: after the grant lapsed, `/sdcard/Download/photo-note.txt` **still existed
on disk**, and AnyFlow still correctly withdrew Open — because what it checks
is the permission, not the file.

`takePersistableUriPermission` is **not called anywhere**. The product does not
need a cosmetic row to survive forever, and taking a persistable grant to
achieve that would be acquiring a long-lived capability for a UI detail.

---

## 16. URI permission semantics

| Route | Grant lifetime | Open works |
|---|---|---|
| received file (MediaStore, AnyFlow-owned) | none needed | until the file is deleted |
| in-app picker (`ACTION_OPEN_DOCUMENT`) | `MainActivity`'s **task** | while that task lives |
| Sharesheet (`ACTION_SEND`) | `SendActivity` | while it is on screen |

Nothing is taken persistably; nothing is requested beyond read; the grant
handed onward to a viewer is `FLAG_GRANT_READ_URI_PERMISSION` for **one URI**,
for the life of that activity. Read, not write — opening a file is not
permission to change it.

The decision seam is a five-value sealed type carrying **no payload at all**:

```kotlin
OpenAction.Available | SourceUnavailable | FileMissing | NoViewer | NotApplicable
```

The brief sketches this as `Available(uri, mime)`. It carries neither, and the
difference is load-bearing twice: a URI in the type the *row* holds is how one
reaches a log or a screenshot; and a URI captured when the row was drawn is a
stale answer by the time it is tapped. `Available` means "structurally
openable" — the transfer really succeeded and something was really retained —
which is exactly the condition the button is drawn on. T21 asserts every
variant is a payload-free object, and that `OpenAction.kt` names no `Uri`, no
`path` and no `ByteArray`.

`NoViewer` is produced only by an actual attempt. Predicting it would need a
package-manager query per row and would still be answering a different
question from the one `startActivity` asks.

---

## 17. MIME handling

Re-derived at the moment of opening, most-trustworthy source first:
`ContentResolver.getType(uri)` → the type recorded on the transfer →
`application/octet-stream`.

**A peer's MIME type is not used as given.** `FileOffer.mime_type` is
attacker-controlled and was previously only length-bounded, which was fine
while nothing downstream acted on it. Opening changes that: the type is what
Android resolves an *application* with, making it the one offer field that
selects code to run. `MimeTypes.sanitize` therefore drops parameters, refuses
anything that is not exactly `type/subtype`, refuses wildcards (a peer does not
get to decide the chooser should offer everything), and validates both halves
against RFC 6838 restricted-name characters. Anything else becomes the
fallback.

It does **not** sniff content and does **not** judge safety. "It ends in .apk"
is not a security control and neither is "it ends in .txt"; Android's own
installer consent guards an install, and duplicating that judgement here would
only make AnyFlow's version the one that is wrong. Opening hands the URI to
`ACTION_VIEW` through a chooser and lets the platform ask.

Nothing is executed. Certified: the received `.txt` resolved to `text/plain`
and Android offered its normal readers.

---

## 18. FileProvider review

**None introduced. `FILEPROVIDER SAFETY: NOT_APPLICABLE`** — and that is the
right outcome, not an omission.

A FileProvider exists to share a file an app owns *privately*. AnyFlow owns no
such file: received files go to public `Download/AnyFlow` via MediaStore, and
outgoing files belong to other apps and arrive as grants. Adding a provider
would have meant either copying a received file into app storage just to hand
it back out — a second AnyFlow-owned copy of the user's data, which §8 of the
brief forbids — or exposing app-private paths that hold the trust store and
identity material.

The manifest contains no `<provider>` of any kind. Nothing about the app's
storage surface widened in this sprint.

---

## 19. Accessibility

* **Direction is a word**, not an arrow's rotation: `SENT` / `RECEIVED` drive
  `From <device>` / `To <device>` text; the glyph and accent colour are
  decoration on top.
* **State is a word.** Status labels come from resources; `AnyFlowStatus`
  supplies only the colour, so someone who cannot separate the tones still
  reads the difference between "Declined" and "Failed".
* **Each row is one sentence** to a screen reader — `"pasture-map (1).txt,
  Received, From Fedora, 91 B"` — via `semantics(mergeDescendants = true)`
  rather than four announced fragments. The description carries the **full**
  name even when the visible label is ellipsised.
* **Open and Cancel name their file.** Every row's button reads "Open"; the
  content description is `Open <filename>`, so the action is unambiguous when
  navigating by button. Same for `Stop transferring <filename>`.
* **Touch targets** use the design system's `MinTouchTarget` via
  `AnyFlowTextButton`.
* **Progress** keeps `AnyFlowProgressBar`'s existing hand-written progress
  semantics.
* **Empty state** is a titled, announced block, not a blank screen.
* Textual app identity is untouched — the launcher label is still
  `@string/app_name`, asserted by a branding test.

---

## 20. Localization

The repo's newer screens already use string resources (87 `stringResource`
call sites); the screen replaced here did not. Every user-facing string on the
Files surface is now in `res/values/strings.xml` — 31 new entries covering
Files, Transfers, Active, Recent, Sent, Received, Sending, Receiving, Waiting,
Declined, Cancelled, Timed out, Disconnected, Failed, Open, File unavailable,
No app can open this file, `From <device>`, `To <device>`, the two scope
notes, the empty state, and the three Open-failure sentences.

`Tab.label` changed from `String` to `@StringRes Int`, because the enum is
constructed before any Composable runs and a literal there was the one piece
of navigation text a translator could not reach. Two strings were added for
the other tabs so the enum is consistent.

The strings block carries the same reviewable rules as the notification
consent copy: a status word must name a state the machine can prove, and
**there is no format specifier anywhere that a URI or path could be
substituted into**.

**Only `values/` exists.** No locale is claimed that the repository does not
have; nothing here asserts translation coverage.

---

## 21. Automated tests

**764 unit tests, 0 failures, across 50 classes.** 43 are new.

### `BrandingResourcesTest` — 12 tests (B1–B8)

| # | Test | Covers |
|---|---|---|
| B1 | every adaptive icon layer resolves to a resource that exists | background/foreground/monochrome resolve to real files or colours |
| B2 | both launcher icons declare a monochrome layer | themed-icon support |
| B3 | the manifest names launcher icons that exist | `icon` / `roundIcon` resolve |
| B4 | no launcher resource still draws the ribbon | + `logo_flowing_a.xml` is gone and unreferenced in all Kotlin |
| B5 | the launcher foreground carries the canonical Flow A geometry | `pathData` == the SVG's `d`; stroke width matches |
| B5 | the mark stays inside every launcher mask | **recomputes** the flattened stroked ink against both safe zones |
| B5 | the icon carries no text | |
| B6 | the monochrome layer names no colour of its own | no gradient, no brand hex, exactly one colour |
| B6 | the coloured foreground uses the brand gradient and nothing else | three stops in order; Ink background |
| B7 | drawn, not downloaded and not photographed | vector only, no `<bitmap>`, no external URL, no image library in Gradle |
| B8 | no build variant overrides the application icon | no `src/debug`/`src/release` manifest or res, no `manifestPlaceholders` |
| — | the app label is the product name and comes from resources | |

The geometry test parses the drawable's own path and `<group>`, rejects any
command other than `M`/`C`, asserts one subpath, refuses a non-default pivot
and a non-uniform scale, flattens every cubic at 400 points and measures the
result. No screenshot comparison is used anywhere.

### `FilesUxTest` — 31 tests (T1–T25, plus)

T1–T2 direction · T3 same filename → two rows · T4 same-name peers stay
distinct · T5–T6 success only from real terminal success · T7–T11 each ending
keeps its name · T12 progress does not duplicate a row · T13 deterministic
most-recent ordering (both ordinals) · T14 clearing removes only finished rows
· T15–T18 the four Open answers · T19 no filename lookup (source assertion) ·
T20 no peer-name identity · T21 the decision carries no content and no URI
(reflection + source) · T22 no `content://`, `file://`, `/storage/`, `/data/`
in a rendered row, and no URI/path field on the type · T23 MIME fallback,
wildcard refusal, header-injection refusal, normalisation · T24 three distinct
truthful failure sentences · T25 100 % of bytes moved still cannot promote a
failure.

Plus: every terminal state maps to a terminal status and no live one does;
an unchecked target is not assumed broken; nothing unfinished is openable;
the Files screen offers no "Send again" (T26–T28 rationale, §30); an offer
card is withdrawn when its transfer ends; and a logging check that the four
new files log nothing at all and that the one new log line names only an id
prefix and a direction.

---

## 22. androidTest compile gate

```
JAVA_HOME=~/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=~/Android/Sdk \
  ./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest
```

**BUILD SUCCESSFUL.**

The gate earned its place: it caught `NotificationUiFixtures.kt` failing to
construct `MainActions` after two actions were added — source-set drift of
exactly the kind this gate exists to stop, invisible to `testDebugUnitTest`
and to `assembleDebug`. Fixed in the same sprint; no connected test was run.

---

## 23. Physical branding evidence — SM-X620

Installed with `adb install -r`. **The app was never uninstalled**; the trust
store (`files/trust-store.json`, 5188 bytes, 4 peer records) was verified
present and intact before and after.

Packaged-APK verification (`aapt2 dump xmltree` on the built APK):

```
application: label='AnyFlow' icon='res/mipmap-anydpi-v26/ic_launcher.xml'
adaptive-icon: background @0x7f020009  foreground @0x7f040019  monochrome @0x7f04001a
foreground: viewport 108×108, translateX=21.2, translateY=17.8, strokeWidth=8
            pathData "M 53 56 C 50 41 43 22 32 8 C 22 21 15 36 10 47 …"
```

| # | Check | Result |
|---|---|---|
| I1 | launcher shows the Flow A | **PASS** — app drawer page 2, labelled "AnyFlow" |
| I2 | round/adaptive mask renders correctly | **PASS** — One UI squircle, mark whole |
| I3 | icon is not clipped | **PASS** — apex, left foot and crossbar terminal all inside with visible margin |
| I4 | launcher/search entry uses AnyFlow branding | **PASS** — drawer, taskbar and Recents card all show the Flow A; label "AnyFlow" |
| I5 | splash appearance is coherent | **PASS** — cold start shows the Flow A on its Ink squircle, centred, over the app's own background |
| I6 | themed/monochrome Flow A | **PASS** — `colortheme_app_icon` flipped to 1: the mark renders white on the wallpaper-derived ground, same geometry, nothing clipped. **Setting restored to 0** and verified |
| I7 | Quick Settings tile branding | **PASS, unchanged** — the tile declares `android:icon="@drawable/ic_clipboard"`, a purpose-specific glyph. It never used the app icon, so §25's "update only if it legitimately uses the stale app icon" does not apply. A QS tile must be a flat monochrome glyph; the Flow A would be wrong there |

Screenshots captured (scratchpad, not committed): app drawer, cropped icon,
cold-start splash, themed icon, Recents card. An offline render of the
drawable under circle / squircle / rounded-square / full masks plus the themed
cut was also produced from the XML's own geometry as a cross-check.

---

## 24. Physical transfers evidence — Fedora + SM-X620

Real LAN, real daemon, no VM, no emulator. Test files were a few dozen bytes
each; contents are not reproduced here.

> The desktop daemon fail-closes: with no GUI provider attached it declines
> every incoming offer. For the runs that needed the desktop to *accept*, it
> was restarted with `--accept-files-without-asking` and **restored afterwards
> to its original `anyflowd --log info` invocation**. That flag changes the
> desktop's consent only; no Android behaviour under test was affected.

| # | Check | Result |
|---|---|---|
| F1 | Fedora → Android success | **PASS** — `pasture-map.txt`, SHA-256 matched host exactly |
| F2 | Android → Fedora success | **PASS** — `photo-note.txt`, SHA-256 matched, `stored at ~/Downloads/AnyFlow/photo-note.txt` |
| F3 | incoming decline | **PASS** — Reject → `Declined`, no Open; **no file written** to `Download/AnyFlow` |
| F4 | outgoing failure / disconnect | **PASS** — daemon killed while an offer was pending → `Disconnected`, distinct from Failed and Declined |
| F5 | same filename sent twice as separate transfers | **PASS** — two `photo-note.txt` rows, `f4afcc7b` and `3ed9f8d5` |
| F6 | Files updates without app restart | **PASS** — offer, Waiting, progress, completion, decline and disconnect all arrived live |
| F7 | received row reaches Received | **PASS** |
| F8 | sent row reaches Sent | **PASS** |
| F9 | Open received file launches a viewer | **PASS** — Android's chooser ("Abrir com") with text/plain readers |
| F10 | opening does not expose a `file://` URI | **PASS** — chooser intent `clip={text/plain {U(content)}} flg=0x10800001`: a **content** URI with `FLAG_GRANT_READ_URI_PERMISSION` |
| F11 | outgoing Open works while the grant is valid | **PASS** — same content-URI chooser for the sent file |
| F12 | outgoing permission unavailable → reports unavailable | **PASS** — see below |
| F13 | same-name rows remain distinct | **PASS** — one `Sent` with Open, one `Declined` without, side by side |
| F14 | peer displayed correctly | **PASS** — "From Fedora" / "To Fedora"; share sheet showed fingerprint `DF65 D3E4 BA28 EDF9` |
| F15 | no content/URI leaks in logcat | **PASS** — 0 matches for `content://`, `file://`, `/storage/`, `/sdcard/` across the whole buffer |

### F12 in detail — the sharpest result

The `MainActivity` task was dismissed from Recents while the **foreground
service kept the process alive** (pid unchanged, `isForeground=true`). So the
transfer rows survived and the task-scoped URI grant did not. On reopening:

* `↑ photo-note.txt — Sent` — **Open withdrawn**, the historical status
  preserved;
* `↓ pasture-map.txt — Received — [Open]` — **still openable**, because
  AnyFlow owns that MediaStore row and needs no grant.

The file `/sdcard/Download/photo-note.txt` **still existed on disk throughout**.
AnyFlow still refused to offer Open — because it checks the permission, not
the file. That is the whole of §10 of the brief demonstrated in one screen.

The log recorded it without leaking anything:

```
I FileTransfer: open target for f4afcc7b is no longer reachable (sent)
I FileTransfer: open target for 3ed9f8d5 is no longer reachable (sent)
```

### A defect found and fixed during certification

Killing the daemon while an offer was on screen left the accept/reject card in
**Active** above a row already reading **Disconnected** — two statements about
one file, one false. `_pending` was cleared only when the person answered or
the 120-second timeout fired; `onSessionEnded → finish` never touched it.

Pre-existing, and never dangerous — `transition` refuses to leave a terminal
state, so answering a dead offer failed closed — but untrue for up to two
minutes, and the Files screen shows both halves at once, which is where it
became visible. `finish()` now withdraws any pending offer for the transfer it
is ending and releases the coroutine waiting on it. Re-certified on hardware:
the card is gone and only the `Disconnected` row remains. Covered by a test.

---

## 25. Process-restart evidence

```
adb shell am force-stop io.github.yurisismotto.anyflow
```

| Question | Answer |
|---|---|
| which rows remain? | **none** |
| which disappear? | **all** |
| does this match the chosen V1 model? | **yes** — §9, session-scoped by design |
| is it labelled? | **yes** — the empty state reads *"Files you send or receive appear here while they are moving, and stay listed until AnyFlow closes."* |
| does Open still work? | not applicable — there is no row to open from |
| does an outgoing source URI permission survive? | **no.** Nothing is taken persistably, so every grant dies with the process |
| do the received **files** survive? | **yes** — `pasture-map.txt` and `pasture-map (1).txt` both still in `Download/AnyFlow`, SHA-256 verified against the host after the restart |
| does the pairing survive? | **yes** — all four trust-store records intact, grants unchanged |

Nothing in the product calls this volatile list "history".

---

## 26. Security and privacy

No regression, and two narrowings.

* **No new permission, no new component, no new exported surface.** The
  manifest diff is empty.
* **No FileProvider** — §18.
* **No persistable URI permission** is taken anywhere.
* **Path-traversal protections untouched.** `Filenames.sanitize` and the
  MediaStore `RELATIVE_PATH` destination are unchanged; the Files UX never
  constructs a path, never accepts a display name as one, and never resolves
  a URI to a filesystem location. Opening uses the exact retained URI for that
  exact transfer id.
* **Unverified bytes stay unreachable.** `openUri` is set only *after*
  `IS_PENDING` is cleared, and `finish()` clears it when it discards a pending
  entry — so a failed hash check cannot leave a row offering to open the bytes
  that failed.
* **Narrowing 1 — peer MIME types are now sanitised** before reaching an
  Intent (§17). Previously the peer's string was only length-bounded.
* **Narrowing 2 — a stale offer card can no longer outlive its transfer**
  (§24).
* No analytics, no telemetry, no cloud sync, no thumbnails, no background
  indexing, no content inspection. Nothing is written to disk at all.

---

## 27. Logging audit

Asserted by test: `FilesMapping.kt`, `FilesScreen.kt`, `OpenAction.kt` and
`MimeTypes.kt` contain **no logging statement of any kind**.

One new log line exists, in `markAccessLost`:

```
open target for <id prefix 8> is no longer reachable (sent|received)
```

A transfer-id prefix and a direction. No URI, no path, no filename, no
fingerprint — asserted, with the surrounding comment stripped first so prose
explaining the rule is not mistaken for a breach of it.

Full-buffer sweep on hardware after every scenario: **0** occurrences of
`content://`, `file://`, `/storage/` or `/sdcard/` from this app.

Pre-existing, unchanged, and reported for completeness: `offer()` and
`onOffer()` log a *display name* (`offering photo-note.txt (37B) as f4afcc7b`).
That is a filename, not a URI or a path, and for incoming files it is the
already-sanitized one. This sprint adds no filename logging and removes none.

---

## 28. Protocol impact

**None. No protobuf or `files.v1` wire change.** `protocol/` and `desktop/`
are untouched — `git status` on both is empty.

Everything the Files surface needs was already either in the offer
(`filename`, `size_bytes`, `mime_type`) or derivable locally (the MediaStore
item AnyFlow itself inserted; the source URI the person shared in; the peer
fingerprint the session authenticated). Nothing required asking the other side
for more, so the no-protocol-change gate was never approached.

Desktop gates were not run, and did not need to be: no shared or protocol code
was touched, and burning the RAM to compile the Rust workspace for ceremony
would have contradicted §0.

---

## 29. Files changed

**Modified (15)**

```
android/app/src/main/res/drawable/ic_launcher_foreground.xml      Flow A, gradient
android/app/src/main/res/drawable/ic_launcher_monochrome.xml      Flow A, flat
android/app/src/main/res/drawable/logo_flowing_ribbon.xml         comment: no longer the app icon
android/app/src/main/res/values/strings.xml                       +31 Files strings, +2 tab labels
android/app/src/main/java/.../files/FileTransferManager.kt        open targets, ordinals, resolveOpen, stale-offer fix
android/app/src/main/java/.../ui/Navigation.kt                    Activity → Files; @StringRes tab labels
android/app/src/main/java/.../ui/AnyFlowShell.kt                  routes to FilesScreen
android/app/src/main/java/.../ui/MainState.kt                     onOpenTransfer, onRefreshOpenTargets
android/app/src/main/java/.../ui/MainActivity.kt                  openTransfer / launchViewer / failure messages
android/app/src/main/java/.../ui/TransferViews.kt                 IncomingOfferCard takes a peer label
android/app/src/main/java/.../ui/components/Cards.kt              statusLabel, rowDescription, action slot, cancel description
android/app/src/main/java/.../ui/components/Indicators.kt         brand mark → Flow A
android/app/src/test/java/.../SendRetryTest.kt                    fixture gains a peer
android/app/src/test/java/.../NotificationNavigationTest.kt       Screen.Activity → Screen.Files
android/app/src/androidTest/java/.../NotificationUiFixtures.kt    two new actions
```

**Added (7)**

```
android/app/src/main/res/drawable/logo_flow_a.xml
android/app/src/main/java/.../files/OpenAction.kt
android/app/src/main/java/.../files/MimeTypes.kt
android/app/src/main/java/.../ui/FilesMapping.kt
android/app/src/main/java/.../ui/FilesScreen.kt
android/app/src/test/java/.../BrandingResourcesTest.kt
android/app/src/test/java/.../FilesUxTest.kt
```

**Deleted (2)**

```
android/app/src/main/java/.../ui/ActivityScreen.kt      replaced by FilesScreen
android/app/src/main/res/drawable/logo_flowing_a.xml    superseded by logo_flow_a.xml
```

### Totals

| Set | Count |
|---|---|
| modified (tracked) | 15 |
| deleted (tracked) | 2 |
| **tracked total — what `git diff` reports** | **17** |
| new, untracked | 7 |
| **code / tests / resources changed by this sprint** | **24** |
| plus this report, once versioned | **25** |

`git diff --stat` reports **17 files, 683 insertions, 186 deletions**. That
figure counts only what git is already tracking, so it excludes the seven new
files — which is why the sprint's real footprint is 24, and 25 with this
report. The seven new files are listed above and are visible in
`git status --short` as `??`; none of them is ignored.

---

## 30. Remaining debts

1. **`docs/design/BRAND.md` is stale.** Its Assets table omits
   `logo-flow-a*.svg`, and its Misuse rule ("do not put the Flowing A on an
   app icon") reads as forbidding what PR #33 deliberately did with the
   *Flow A*. Not touched here — a brand-book correction belongs with whoever
   owns the brand. Worth a short follow-up.
2. **No "Send again" on the Files screen (T26–T28 not applicable).**
   Deliberate. Retry lives on the share-sheet surface where the source grant
   is still live; on Files that grant is normally gone — which is what F12
   proves — so a retry there would either fail or need a persistable
   permission the product does not take. Left out rather than faked, and
   asserted absent by test.
3. **An incoming offer that times out is recorded as `DECLINED_BY_USER`.**
   Pre-existing: `awaitApproval`'s `withTimeoutOrNull(…) ?: false` cannot
   distinguish "said no" from "never answered", so a 2-minute timeout displays
   as **Declined** rather than **Timed out**. Android can currently only show
   `Timed out` for a timeout the *peer* reports. Fixing it means a second
   failure reason at the approval seam — small, but it changes what is sent on
   the wire, so it was out of scope here.
4. **Android lint is not a gate** — see §31. Unchanged by this sprint.
5. **`AnyFlowStatus`'s labels are still hard-coded English.** The Files screen
   routes around this (its status words come from resources and
   `AnyFlowStatus` supplies only colour), but the shared enum remains
   unlocalised for the other screens that use it.
6. The Files screen re-probes open targets on entry and on list change. A file
   deleted by another app *while the screen is already open and idle* is not
   noticed until the next entry — Android offers no callback for it, and
   polling was rejected per §15.

---

## 31. Git status

```
$ git branch --show-current
feature/android-branding-files-ux-v1

$ git status --short
 M android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/files/FileTransferManager.kt
 D android/app/src/main/java/io/github/yurisismotto/anyflow/ui/ActivityScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/AnyFlowShell.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainState.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/Navigation.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/TransferViews.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/components/Cards.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/components/Indicators.kt
 M android/app/src/main/res/drawable/ic_launcher_foreground.xml
 M android/app/src/main/res/drawable/ic_launcher_monochrome.xml
 D android/app/src/main/res/drawable/logo_flowing_a.xml
 M android/app/src/main/res/drawable/logo_flowing_ribbon.xml
 M android/app/src/main/res/values/strings.xml
 M android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationNavigationTest.kt
 M android/app/src/test/java/io/github/yurisismotto/anyflow/SendRetryTest.kt
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? android/app/src/main/java/io/github/yurisismotto/anyflow/files/MimeTypes.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/files/OpenAction.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/FilesMapping.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/FilesScreen.kt
?? android/app/src/main/res/drawable/logo_flow_a.xml
?? android/app/src/test/java/io/github/yurisismotto/anyflow/BrandingResourcesTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/FilesUxTest.kt

$ git diff --check
(clean)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untouched and still untracked. Nothing
was committed, pushed, or opened as a PR. `git add .` was never used.

---

## 32. Gate matrix

| Gate | Result |
|---|---|
| ANDROID FLOW A BRANDING | **PASS** |
| ADAPTIVE ICON | **PASS** |
| THEMED / MONOCHROME ICON | **PASS** |
| FILES SCREEN | **PASS** |
| ACTIVE TRANSFER LIVE UPDATE | **PASS** |
| RECENT TRANSFER MODEL | **PASS** (session-scoped, labelled as such) |
| TRANSFER IDENTITY SAFETY | **PASS** |
| RECEIVED FILE OPEN | **PASS** |
| SENT FILE OPEN | **PASS** |
| URI PERMISSION SAFETY | **PASS** |
| FILEPROVIDER SAFETY | **NOT_APPLICABLE** (none introduced; none needed) |
| PROCESS RESTART BEHAVIOUR | **PASS** |
| ACCESSIBILITY | **PASS** |
| SECURITY / PRIVACY REGRESSION | **PASS** (no regression; two narrowings) |
| ANDROID TEST GATES | **PASS** |

**Build gates, run sequentially, never concurrently with Cargo:**

```
:app:testDebugUnitTest        BUILD SUCCESSFUL   764 tests, 0 failures
:app:assembleDebug            BUILD SUCCESSFUL
:app:assembleDebugAndroidTest BUILD SUCCESSFUL
```

**Android lint was not run and is not claimed to pass.** `:app:lintDebug` is
deliberately excluded from this repository's CI, documented in
`.github/workflows/android-ci.yml` ("Android lint is NOT a gate here"), where
it currently reports 1 pre-existing error and 47 warnings. This sprint did not
change that decision and did not add to the count knowingly.

---

## 33. Final verdict

### ANDROID BRANDING + FILES UX V1: **PASS**

Against the stated conditions:

* **Flow A is actually used by Android** — launcher, round mask, themed icon,
  splash, in-app identity, and the Recents card. Generated from
  `logo-flow-a.svg`, pinned to it by a geometry test, and photographed on the
  SM-X620.
* **The Files surface truthfully represents transfer state** — every status
  word is derived from the state machine and from nothing else, and the two
  kinds of evidence behind that claim are deliberately kept apart:
  * **hardware** certified the documented F1–F15 scenarios on the SM-X620,
    which produced and read **Sent**, **Received**, **Declined** and
    **Disconnected** on screen, along with Open, the `content://` chooser, and
    the loss of an outgoing URI grant;
  * **the JVM suite (T1–T25+)** covers the *complete* mapping, including
    **Cancelled**, **Timed out**, **Failed** and every remaining state — each
    asserted exhaustively over `TransferState` × direction × `FailureReason`,
    including that 100 % of bytes moved still cannot promote a failure.

  Cancelled, Timed out and generic Failed were **not** reproduced physically;
  they are asserted in the suite. Claiming otherwise would be the kind of
  untruth this whole sprint is about.
* **Received files can be opened safely when available** — MediaStore's own
  `content://` URI, a chooser, a read grant for one item, a sanitised media
  type, and a truthful sentence for each of the four ways it does not happen.
* **Sent files are never recovered by filename or path** — proven in the one
  case that settles it: the file still on disk, the grant gone, Open correctly
  withdrawn.
* **No dangerous URI permission or FileProvider expansion** — no provider, no
  persistable grant, no new permission, an empty manifest diff.
* **No file content is persisted for history** — nothing is persisted at all,
  and the UI says so in the two places a person would otherwise be misled.
* **No security regression** — plus peer MIME sanitisation and the stale-offer
  fix.

Stopping here for review. Nothing committed, nothing pushed, no PR opened.
