# OmniBridge — UI guidelines

How the [brand](BRAND.md) is applied on each platform, and the rules that are
not negotiable.

---

## The rules that outrank the mockups

The design reference is the visual source of truth. These five rules outrank
it, and each of them changed something the reference showed.

### 1. Colour never carries meaning alone

Roughly one man in twelve cannot separate the cyan of "connected" from the
amber of "not responding", and both are claims about whether what is on screen
is true right now.

Every status is **a dot, an icon and a word**, and the word carries the
meaning. Both platforms declare the vocabulary in one place —
`OmniBridgeStatus` (Kotlin) and `widgets::Status` (Rust) — with all three
required by the type, so a status cannot be added as a colour and nothing
else.

### 2. Text uses the corrected accents, never the brand hues

Bridge Cyan is 2.40 : 1 on white. See [the two-family rule](BRAND.md#the-two-family-rule).
Tests on both platforms fail if a text accent drops below AA — on the
background as well as on a card, and on the elevated dark surface as well as
the plain one, because those are the harder of each pair and were the ones
not previously checked.

### 3. The fingerprint is never abbreviated for balance

It is the string a person compares against another screen, and that comparison
is the entire reason pairing has no man-in-the-middle window. It is shown in
full, in monospace, selectable, grouped in fours. Aesthetics do not get a vote.

### 4. Nothing is stored to make a screen look better

The reference shows a **clipboard history** panel and a persistent transfer
**History** tab. OmniBridge has neither, by design: clipboard text never reaches
disk and the control socket carries no clip content at all — a pending clip is
size, hash prefix and age.

Both were removed rather than faked. What replaced them says plainly what is
and is not kept:

> *"OmniBridge keeps no clipboard history. A received clip waits in memory with a
> five-minute expiry and is gone once applied, dismissed or expired."*

Transfers are listed for the current daemon run, under a line saying so.

### 5. Security wording is the project's own, not marketing

The reference labels the connection **"End-to-end encrypted."** The product
says:

> **Secure connection** · *Direct connection · TLS 1.3, pinned · local network*

with the detail in a tooltip: TLS 1.3 with ALPN `omnibridge/1`, mutually
authenticated, pinned to the key approved at pairing, no relay and no cloud.
That is precise and checkable. Reaching for a phrase whose meaning does not
exactly match is how a security claim quietly becomes untrue.

---

## Status language

| Status | Meaning | Light | Dark | Icon |
|---|---|---|---|---|
| Connected | A live session exists | `#10747E` | `#3DC9D7` | link |
| Available | Paired, no session right now | `#445CDD` | `#90A1FF` | device |
| Connecting | Handshaking or retrying | `#445CDD` | `#90A1FF` | activity |
| Transferring | Bytes moving | `#445CDD` | `#90A1FF` | send |
| Success | Finished cleanly | `#10747E` | `#3DC9D7` | check |
| Not responding | Session silent long enough to be history | `#B45309` | `#FBBF24` | warning |
| Failed | Terminal error | `#DC2626` | `#F87171` | warning |
| Disconnected | Deliberately not connected; also cancelled | `#64718B` | `#7C88A3` | link-off |
| Revoked | Trust withdrawn | `#DC2626` | `#F87171` | shield-off |

The four brand-derived rows moved with the palette. **Amber and red did
not**, and that is the rule rather than an oversight: a warning means the
same thing whatever the identity is.

Two distinctions worth keeping:

- **Available ≠ Disconnected.** A paired device with no session is resting, not
  faulty. "Disconnected" reads as a problem.
- **Not responding ≠ Connected.** A session that has gone quiet means anything
  the device last told us is history. The daemon models this as `Stale`, and
  collapsing it into "connected" is what once let a dead session keep showing
  a live battery percentage.
- **Cancelled is not Failed.** A cancellation is a decision, and it is not
  drawn in red.

---

## Components

Both platforms carry the same vocabulary under the same names.

| Concept | Android (`ui/components`) | Desktop (`gui/src/widgets.rs`) |
|---|---|---|
| Card | `OmniBridgeCard` | `card()` |
| Section heading | `OmniBridgeSectionLabel` | `section_label()` |
| Status | `OmniBridgeStatusBadge` | `status_badge()` |
| Primary action | `OmniBridgePrimaryButton` | `cta_button()` |
| Secondary action | `OmniBridgeSecondaryButton` | `secondary_button()` |
| Destructive action | `OmniBridgeDestructiveButton` | `destructive_button()` |
| Permission row | `OmniBridgeCapabilityRow` | `policy_switch()` |
| Device | `OmniBridgeDeviceCard` | `device_card()` |
| Transfer | `OmniBridgeTransferCard` | `transfer_card()` |
| Security notice | `OmniBridgeSecurityNotice` | `security_notice()` |
| Empty state | `OmniBridgeEmptyState` | `empty_state()` |
| Tinted icon | `OmniBridgeIconTile` | `icon_tile()` |
| Fingerprint | `OmniBridgeFingerprint` | `fingerprint()` |
| Brand mark | `OmniBridgeGradientMark` / `OmniBridgeBrandMark` | `brand_mark()` / `brand_logo()` |
| Progress | `OmniBridgeProgressBar` | `progress()` |

### Connection state has one home per screen

**For a given peer or session, one screen shows its connection state and its
primary Connect/Disconnect actions in exactly one place.** That place is the
canonical surface for the peer — the device card on Android, the device row
on the desktop. Nothing else on the same screen repeats the badge, and
nothing else on the same screen offers a second Connect or Disconnect for the
same session.

Two controls that appear to drive one session are not a tidiness problem.
They invite a reading that is simply false — that disconnecting in one place
might leave the other still connected — and there is no way for a person to
tell from the screen which of the two is authoritative. The Devices screen
carried exactly this for a while: the selected device card said `Connected`
with a `Disconnect`, and a separate "Connection" section underneath said
`Connected to fedora` with its own `Connect` and `Disconnect`.

**A secondary notice may still appear**, and should, when it carries state the
canonical surface genuinely cannot express. A status badge has room for a
word, not a sentence:

- a retry countdown and the reason for it;
- the text of a terminal connection error;
- guidance when several peers are paired and none is selected, which is not a
  property of any single row — the answer is "choose one".

Such a notice carries **words only**. No status badge, no Connect, no
Disconnect. If a notice would say something the canonical surface already
says, it should not be shown at all: both front ends decide this in one
testable place rather than in a composable or a widget tree.

This is a rule about *presentation*, not about state. Every connection state
the coordinator models stays modelled; what the rule constrains is how many
places on one screen are allowed to draw it.

### Cards

One surface, one hairline border, 16 px radius, 16 px padding. Depth comes
from the border, not a shadow. A card that is also a button keeps the card
look and gains a hover state.

### Buttons

- **Primary** — pill, CTA gradient, white label, 48 dp minimum height. Disabled
  is a flat neutral, never a faded gradient: translucent gradient reads as
  "still tappable, just pretty".
- **A disabled control always says why.** Greying something out states that it
  cannot be used; it never states what would make it usable, and the person
  is left guessing at a screen that has the answer. Both front ends now carry
  the reason next to the control — `UiMapping.sendClipboardUi` on Android,
  `views::clipboard::send_blocked_reason` on the desktop — and both are unit
  tested to produce one for every unusable state.
- **Secondary** — pill, surface fill, hairline border.
- **Destructive** — pill, **outlined** red, never filled, and never placed
  adjacent to a confirming button. On the desktop it also takes a confirmation
  dialog naming the device.

### Toggles

A permission row is icon, title, one-line description, switch. The description
is not filler: every one of these rows changes what another machine may do
with this one, and "Clipboard" alone does not say whether it means sending,
receiving or both.

A switch that cannot work is not shown. Where the platform forbids something —
Android cannot read the clipboard in the background — the UI states the
reason instead of offering a dead toggle.

A switch whose precondition is off is **disabled**, not silently ignored:
"Apply automatically" greys out when "Allow receiving" is off.

### Progress

Track is `surface-sunken`, fill is the brand gradient, 6 px, fully rounded.
Unknown progress travels a short segment rather than asserting a percentage
that would be a lie. A percentage is always accompanied by a status word.

---

## Desktop — GTK4 / libadwaita

Native, not a web view. It honours the system font, the system colour scheme
and the system's accessibility settings because it is built from the
platform's own widgets.

**Structure.** `AdwApplicationWindow` → `AdwToolbarView` (`AdwHeaderBar` with
the mark + wordmark) → `AdwNavigationSplitView` (sidebar + content) → a
`GtkStack` of pages, with a status strip pinned along the bottom.

**Navigation.** Dashboard · Files · Clipboard · Devices · Trusted peers ·
Settings. The selected row is a tinted rounded rectangle in the corrected
blue.

**Sidebar footer.** Network state and this computer's own name and short
fingerprint. The fingerprint is there, not buried in Settings, because it is
what someone reads out while pairing from the other side.

**Responsiveness.** An `AdwBreakpoint` at 700 sp collapses the split view
rather than squeezing the content. Minimum window 360 × 420.

**Data.** Pages re-render wholesale from daemon state on a 2 s poll. With this
much data that is cheaper in bugs than a diffing layer, and it makes what is
on screen a pure function of what the daemon last said. The control socket
cannot push; a D-Bus front end would get signals instead, as
`daemon/src/control.rs` already anticipates.

**Theme.** The stylesheet is rebuilt and swapped when
`AdwStyleManager::is_dark` changes — dark is a derived palette, so it cannot
be a filter over the light one.

---

## Android — Compose / Material 3

Material semantics are kept, not replaced. OmniBridge should look like
**OmniBridge on Android**, not like a foreign design language pasted onto the
platform: a `Switch` still behaves and reads as an Android switch, a dialog
still sits where Android puts one.

**Dynamic colour is deliberately off.** The palette carries meaning — teal is
"connected", amber is "not responding", red is "revoked". Letting the
wallpaper recolour that recolours the status language with it.

**Structure.** `Scaffold` with a top app bar and a bottom `NavigationBar`:
Devices · Activity · Settings. Detail screens (peer, send clipboard) live
under the Devices tab and are reached by tapping; back returns to the tab.
Navigation state is `rememberSaveable`, so a rotation lands where it was.

**Home.** Anything waiting on a decision — an incoming file, a held clip —
sits above the device list, because it is the only thing on the screen with a
deadline.

**Activity.** In-flight work and what finished since the app started. No
History tab; see rule 4.

**Quick actions** grey out rather than disappearing when they cannot run, so
the row does not reflow every time a session drops.

**Touch targets** are 48 dp minimum even where the reference draws a control
smaller: the visual size is the reference's, the target is Android's.

### The Send clipboard screen

Hero, destination, content, action, security — in that order, because that is
the order the questions occur in: *what am I doing*, *where is it going*,
*what exactly is going*, *do it*, *is this safe*.

The destination line under the device name is its **short fingerprint**. It
is deliberately not a platform description: `DeviceInfo` on the wire carries
`device_id` and `device_name` and nothing else, so no operating system or
form factor ever reaches the other side. Three screens used to print a
literal `"Desktop · Linux"` there, which labelled every paired device Linux
whether it was or not. What the product does know is the pinned identity, and
that is worth the line — it tells two computers called `fedora` apart.

The security footer states four facts — *Direct connection · TLS 1.3 ·
Pinned identity · Local network* — and is announced to a screen reader as one
sentence rather than four fragments. The desktop does **not** repeat this on
its Clipboard page: it has a persistent status strip that says the same
thing, and a per-page footer there would be Android's solution to a problem
GTK does not have.

#### The preview itself

The Send clipboard screen shows the text about to leave, because seeing it is
how a person notices they are about to send the wrong thing.

A clip the source app marked `EXTRA_IS_SENSITIVE` is **not** shown. That flag
exists precisely so that surfaces like this one do not render a password into
a shoulder-surfable box; the size and the destination are enough to decide
with. Neither case is logged or persisted — the preview lives in composition
and dies with the screen.

---

## Layout and responsiveness

Both platforms cap the content column rather than letting cards grow to the
width of the window. A card stretched across a tablet or a maximised desktop
window is not a wide layout — it is a narrow layout that has been pulled, and
it reads as one: a row with a title at the far left and a switch at the far
right makes the eye travel the whole width to connect two things that belong
together.

| | Rule |
|---|---|
| **Android** | `Modifier.omniBridgeContentColumn()` — fills below 640 dp, caps and centres above it. A modifier rather than a wrapper, so it applies to a `LazyColumn` without making it eager. |
| **Desktop** | `AdwBreakpoint` at 700 sp collapses the split view rather than squeezing the content. Minimum window 360 × 420. |

The cap is a layout constraint and nothing else. Neither platform branches on
width to show *different content*: a tablet and a phone show the same screen,
and only the margins differ.

## Motion

Movement means flow: something travelling from one device to another. Short,
eased, never looping for decoration.

| Token | ms | Use |
|---|---|---|
| instant | 90 | Press feedback |
| fast | 160 | Page crossfade |
| normal | 240 | Progress, state change |
| slow | 400 | Entrances |
| ribbon pulse | 2200 | One pass of an indeterminate sweep |

**Reduced motion is honoured.** Android reads
`Settings.Global.ANIMATOR_DURATION_SCALE`; a duration used raw instead of
through `motionDuration()` is a bug. An indeterminate progress bar parks
instead of sweeping.

---

## Accessibility

| Requirement | How |
|---|---|
| Contrast | Every text token clears AA on its own surface; asserted by tests on both platforms |
| Not colour alone | Every status is dot + icon + word |
| Touch targets | 48 dp minimum on Android |
| Keyboard | Full tab order on the desktop; a 2 px focus ring that survives tinted cards |
| Screen readers | Semantic roles throughout; decorative artwork explicitly hidden |
| Grouped announcements | A status badge announces once as a phrase, not three times as fragments |
| Fingerprints | Announced in four-character groups rather than as one unpronounceable run |
| Reduced motion | Honoured on Android; the desktop's only animation is a 160 ms crossfade |

Decorative elements — the ribbon flourish behind a device card, the transfer
hero illustration, the QR mark — are marked as presentation and carry no
state. A card must never need its artwork to say it is connected.

---

## What must never be traded for UX

Restating the sprint's own boundary, because it is the reason several things
above look the way they do:

- no hiding a warning that a person needs;
- no removing or shortening a fingerprint;
- no auto-granting a capability, auto-enabling clipboard, or auto-accepting a
  file;
- no clipboard history, and nothing persisted merely to feed a screen;
- no loosening of TLS, and no cloud, account or telemetry.

The UI adapts to the product's rules. Never the other way round.
