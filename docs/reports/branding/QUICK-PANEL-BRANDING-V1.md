# AnyFlow — Quick Panel + Branding v1

**Sprint closeout · 17 September 2026 · branch `feature/quick-panel-branding-v1`**

| | |
|---|---|
| **Status** | **QUICK PANEL + BRANDING V1: PASS** · **pre-PR polish: PASS** |
| Branch | `feature/quick-panel-branding-v1` |
| Base | `4bebff6` (merge of #32, `fix/ux-debt-retry-repair-scanner-insets`) |
| Commits made | **none** — no commit, no push, no PR |
| Working tree at close | 15 modified · 8 new paths · 1 untouched historical file |
| Tests | **864** workspace (was 786) · `anyflow-gui` **113** (was 39) + 19 display-gated sections |
| Gates | `fmt` clean · clippy **0 warnings** · all gates PASS |

**Sprint closeout, plus a pre-PR polish added on 17 September 2026** — two
items, §§32–39: recent file transfers in the Quick Panel (QP-POLISH-01) and the
generic-application-icon defect (BRAND-POLISH-01). Everything in §§1–31 is the
original certification and is unchanged; the polish sections are additive and
the running totals above include both.

---

## 1. Scope

Two tightly related scopes, both delivered:

**QP-1 — Desktop Quick Panel.** A compact Libadwaita surface that answers "is
AnyFlow running, which devices are connected, what is their battery, which one
am I sending to, and can I send something now" in a few seconds, with Send
File and Send Clipboard bound to the existing `files.v1` and `clipboard.v1`
paths.

**BRAND-1 — AnyFlow visual identity v1.** A new primary mark — the Flow A —
replacing the teal-to-violet ribbon-and-dots as the application icon, plus the
desktop metadata that makes it resolve by name.

Nothing from the exclusion list was implemented. See §30.

---

## 2. Baseline

```
4bebff6 Merge pull request #32 from yurisismotto/fix/ux-debt-retry-repair-scanner-insets
782ba58 fix(android): harden retry repair and scanner UX
932234a Merge pull request #31 from yurisismotto/fix/ux-hardening-file-approval-qr-orientation
```

Working tree at open: one untracked historical file,
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`. It was **not** read into, modified, staged
or touched at any point.

---

## 3. Existing GUI architecture (audited before any change)

| Piece | Where | What it was |
|---|---|---|
| Application | `gui/src/lib.rs` | One `adw::Application`, id `io.github.yurisismotto.anyflow`, built one window in `connect_activate` |
| Window | `lib.rs::build_window` | `AdwNavigationSplitView`, 7-page sidebar, statusbar |
| Pages | `gui/src/views/` | dashboard, files, clipboard, notifications, devices, peers, settings |
| Control client | `gui/src/client.rs` | Same NDJSON socket the CLI uses; `send` (one-shot), `pair` (stream), `watch_file_offers` (stream) |
| Poll | `lib.rs` | 5 requests per 2 s, committed as one `DaemonState`; pages redraw only on a slice that changed |
| Approval | `gui/src/approval.rs` | `install(&window)` — attached to **the** window |
| Tokens/widgets | `theme.rs`, `widgets.rs` | Two-family palette (identity vs AA-corrected text), status vocabulary that is never colour alone |
| Marks | `docs/design/assets/` | `icon-flowing-ribbon.svg` (product/app icon), `logo-flowing-a.svg` (institutional), compiled into a GResource |
| Desktop metadata | — | **none.** No `.desktop` file, no icon-theme entry, no window icon name |

Three things the audit found that shaped the work:

1. **There was no concept of a chosen device.** `views/dashboard.rs` picked a
   destination with
   `connected.iter().find(|d| d.granted_capabilities.contains("files.v1"))` —
   list position with a filter in front of it. This is the desktop instance of
   the defect U2 P1 fixed on Android at three call sites, and the P1 report
   records that the trust store's order is not even stable (a successful
   connection appends the peer). Fixed here; see §6.
2. **The approval provider belonged to a window.** With two windows, hanging it
   on either means closing that one silently stops the machine being able to
   accept a file. Moved to the application; see §11.
3. **The primary mark was a line between two dots.** At 16 px it reduces to two
   blobs and a hairline, and it has no letterform to hold on to. Replaced;
   see §14.

---

## 4. Quick Panel architecture

```
anyflow-gui                          one process, one application id
├── App                              lib.rs — owns everything that outlives a window
│   ├── DaemonState  (Rc<RefCell>)   one poll, shared
│   ├── Selection    (Rc)            the chosen device, persisted
│   ├── poll          2 s timer      installed once, at startup
│   ├── ApprovalHandle               the daemon's file-approval provider
│   ├── actions       app.quick-panel · app.settings
│   ├── QuickPanel?   panel/         the everyday surface
│   └── SettingsWindow?  views/      the management surface
└── panel::model                     pure — no GTK — every decision the panel makes
```

The split that matters is the last line. `panel/model.rs` is 1 056 lines of
plain data and functions with no GTK in it; `panel/mod.rs` is 1 047 lines that
draw the result and turn clicks back into the requests the model composed. A
button is handed either an `Action::Ready { fingerprint, peer_name }` or an
`Action::Blocked { reason }` and has nothing else to go on, so the widget layer
**cannot grow a routing rule of its own**.

The panel is a window on the same `GtkApplication` as Settings. It is not a
daemon, does not start one, holds no state Settings cannot see, and adds no
authority: every button ends in a control request `anyflow` could make by hand.

### Layout as built

```
┌────────────────────────────────────┐   380 px wide, height follows content
│ (A) AnyFlow                     ⚙  │   AdwWindowTitle + subtitle
│     One flow. Any device.          │
│ ┌────────────────────────────────┐ │
│ │ ◉  SM-X620                  ✓  │ │   radio · name · Selected
│ │    Connected · 79% · Not charging│   state and battery in words
│ │    Clipboard · Files           │ │   what is usable right now
│ ├────────────────────────────────┤ │
│ │ ○  SM-X620                     │ │   muted tile, no capabilities
│ │    Offline                     │ │
│ │    Available when connected    │ │
│ └────────────────────────────────┘ │
│ [ Send file ]  [ Send clipboard ]  │
│  ● Transferring  Sending x.txt·42% │   only while in flight
│ ────────────────────────────────── │
│  Notifications                 Off │
│  Clipboard                      On │
│  Files                          On │
│ ────────────────────────────────── │
│ [ Open AnyFlow Settings ]          │
└────────────────────────────────────┘
```

Measured on the real desktop: **380 × 607 px** with four devices and a live
session, **380 × 214 px** with the service down.

---

## 5. Panel state model

All of it in `panel::model`, all of it tested without a display.

```rust
Health      Available | Reaching | Unavailable { headline }
Link        Connected | Stale | Offline
Battery     Present { percent, status, stale } | Absent | Unavailable
Capability  { granted, live }          usable() = granted && live
PeerCard    { fingerprint, fingerprint_short, device_id, name, platform,
              link, selected, battery, files, clipboard, notifications }
Target      Selected(fp) | OnlyTrustedPeer(fp) | MustChoose { stale_choice } | NoTrustedPeer
Action      Ready { fingerprint, peer_name } | Blocked { reason }
StatusLine  { value: On|Off|Unavailable, detail }
TransferLine{ filename, outgoing, peer_name, percent }
PanelModel  { health, this_device, peers, target, send_file, send_clipboard,
              files, clipboard, notifications, transfer }
```

Three states rather than two, twice over, and both times because the middle
one is a different situation with a different fix:

* `Health::Reaching` — the first poll is still in flight. Drawing that as a
  failure would make the panel flash an error every time it opened.
* `Capability { granted, live }` — "you never allowed this" and "this session
  did not negotiate it" fail for unrelated reasons. Only `usable()` enables an
  action; the row's tooltip states which of the two is missing.

Revoked devices are filtered out of the panel entirely: they are not trusted,
so they are not an everyday surface, and recovering one is a Settings job.

---

## 6. Multi-peer routing

**A destination is a fingerprint.** Never a display name, never a list index,
never an address. `Target::resolve` is the desktop half of Android's
`store/PeerTarget.kt` and has the same four outcomes:

| Situation | Outcome |
|---|---|
| No trusted peer | `NoTrustedPeer` — nothing to send to |
| Choice stored, still names a trusted peer | `Selected(fp)` |
| No choice, exactly one trusted peer | `OnlyTrustedPeer(fp)` — the convenience case |
| No choice, several peers | `MustChoose { stale_choice: false }` — the panel asks |
| Choice stored, no longer names a trusted peer | `MustChoose { stale_choice: true }` |

The only thing read out of the peer slice is a fingerprint comparison and a
length. Display order is a *separate* concern: rows are sorted connected-first
then by name then by fingerprint, precisely so they do not swap places under
the pointer when a session comes up — which the P1 report measured happening.

### One deliberate divergence from Android, documented

Android's rule 7 lets a stale choice fall through to the one-peer convenience.
This does not. A stored fingerprint that has gone means a device that was
unpaired or re-paired, and quietly re-aiming that person's Send button at
whatever is left is the "fallback to another peer" the whole model exists to
refuse. It costs one click and the panel says why:

> The device you had chosen is no longer paired. Choose one above.

### Where the choice lives

`$XDG_CONFIG_HOME/anyflow/gui.json`, one additive key, schema 1:

```json
{ "schema": 1, "selected_peer": "573ccb84da6c993b…a6b7" }
```

One public fingerprint hex. No name, no address, no content. The value is
validated as hex on the way in *and* on the way out, because it becomes a
device selector on the control socket. Anything unreadable, unparseable,
missing, of the wrong JSON type or not hex reads as "nobody has chosen" — the
state that **asks**, never the state that sends.

Deliberately not `$XDG_DATA_HOME/anyflow`: that is the trust store, whose
0700/0600 modes are verified on load, and a GUI preference has no business
inside a directory with that contract.

It is an **application** preference, not a panel one. The Settings dashboard
reads and writes the same `Selection`, and its own "Send a file…" quick action
is now `PanelModel::send_file` — the very value the panel's button carries. The
list-position destination in `views/dashboard.rs` is gone.

The daemon would be the better long-term owner, so the CLI and a future tray
would agree without anyone writing a file. That needs a new control request and
this sprint adds no protocol, so it is recorded as debt (§28).

---

## 7. Battery semantics

`Battery` has three states and the middle one is why the type exists. U2 P2
fixed a desktop that rendered *no battery* as `0%`; `battery.v1` expresses "no
battery" by sending **no frame at all** (`LocalBatterySource::read → None`), so
a missing reading has to stay missing all the way to the screen.

| Daemon says | Panel model | Rendered |
|---|---|---|
| `battery: Some(r)` | `Present { percent, status, stale }` | `79% · Not charging`, or `0%` at zero |
| `None`, live session, `battery.v1` negotiated | `Absent` | `No battery reported` |
| `None`, offline, or `battery.v1` not negotiated | `Unavailable` | omitted from the row; `Battery unavailable` when asked |

* **Zero is a real reading.** `Present { percent: 0 }` renders as `0%`.
* **Absence is never a number.** `an_absent_battery_never_renders_as_zero`
  asserts the rendered label contains no `%` at all.
* **Absent ≠ Unavailable.** Different words, asserted different.
* A reading arriving over a `Stale` session is forced to `stale: true` and
  labelled `79% (last known)` — history, not the present.

Observed live on the tablet across a 2-hour session, a grant-triggered
reconnect and a daemon restart: `79% · Not charging` when fresh,
`79% (last known)` past the 120 s staleness floor, `Battery unavailable` when
the peer went offline. Never `0%`.

---

## 8. Files quick action

Uses the existing path and adds nothing:

```
button click
  → GtkFileDialog::open()                      the platform's own chooser
  → model::send_file_request(&action, path)    Request::Send { device: <fingerprint>, path }
  → client::send(...)                          the same socket the CLI uses
```

* No new transfer implementation, no new authorization model, **no wire change**.
* The GUI is never on the data path: it hands the daemon a path, and the daemon
  streams the bytes over the session it already holds. No byte of the file
  enters this process.
* The receiving end's approval still applies in full — verified in §24.
* A blocked action is disabled with the reason in the tooltip *and* the
  accessible description: `Files are not enabled for SM-X620.`,
  `This session with SM-X620 has not negotiated file transfer.`,
  `SM-X620 is offline.`

Progress is `PanelModel::transfer` — **in-flight transfers only**. A finished
transfer disappears from the panel rather than accumulating, so the panel
cannot become the file history the design forbids. The completed list stays in
Settings, where it is scoped to the daemon run and says so.

---

## 9. Clipboard quick action

```
button click
  → model::send_clipboard_request(&action)     Request::ClipboardSend { device: <fingerprint>, sensitive: false }
  → client::send(...)
```

* The local clipboard is read **by the daemon**, and only because a person
  pressed the button. Nothing about the clip enters this process, is shown
  here, or is kept anywhere.
* `sensitive` is `false` and is not the panel's to set: asking a receiver to
  treat a clip as a secret is a claim about the clip, and someone pressing a
  general-purpose Send button has not made it. The Settings clipboard page
  takes the same position.
* Preconditions, each with its own sentence: service available · a destination
  chosen · peer connected · `clipboard.v1` granted · negotiated on this session
  · `clipboard.v1` enabled on this computer · a working backend ·
  `allow_send` on.

### A real-hardware finding, fixed

The panel first said *"Clipboard sent to SM-X620"* and the tablet had silently
dropped every clip — the grant existed on the desktop and not on the Android
side. `ClipboardSend` is answered as soon as the frame is on the session (that
is all the sending end can know at the time); the receiver's verdict arrives
afterwards as `last_outcome`.

The panel now reports it:

> Last clip sent: not authorized by SM-X620.

Quiet outcomes (`applied`, `pending`, `duplicate`) are not news and are not
shown. Covered by `a_refusal_by_the_receiving_device_is_reported`.

---

## 10. Notifications status

State only. **No notification content, ever.**
`NotificationsStatusReport` has no field that can carry a title, a body or an
application name, so the row is structurally incapable of becoming a history.

| Condition | Row |
|---|---|
| `!enabled` | `Unavailable` — not enabled on this computer |
| `!available` | `Unavailable` — no notification server in this session |
| no destination | `Unavailable` — no device selected |
| `!granted` or `revoked` | `Off` — not enabled for *name* |
| `!allow_mirror` | `Off` — mirroring is turned off for *name* |
| granted + mirroring | `On` |

The row follows **grant and policy**, never the fact that a device advertised
the capability: `the_notifications_row_reflects_grant_and_policy` pins the case
where `notifications.v1` is negotiated and live on the session while
`allow_mirror` is off — the row says `Off`.

Lock policy is appended when it is not `full`: *"Hidden while this screen is
locked."* / *"Only the app name is shown while this screen is locked."* A peer
that is connected but not sourcing is stated too.

---

## 11. Clipboard and Files status rows

**Clipboard.** The one place the Android platform limit has to be told
truthfully. The two directions are separate settings and only one can ever be
automatic:

* *this computer → the device* may be automatic, if this session can observe
  clipboard changes at all (`watch_available`; GNOME cannot);
* *the device → this computer* covers only what arrives. A modern Android
  cannot read its own clipboard in the background, so nothing arrives unless a
  person asks it to on the phone, and `auto_receive` decides what happens to a
  clip **after** it lands.

Rendered live on this machine:

> Sends this computer's clipboard only when you press Send clipboard. Clips
> from SM-X620 are held until you apply them. SM-X620 sends only when you ask
> it to there.

The word *sync* does not appear anywhere in the panel, and a test asserts it
does not. The tablet's own settings screen says the same thing in its own
words — *"Android does not let an ordinary app read the clipboard in the
background, so clips are sent when you tap Send clipboard"* — so the two ends
agree.

**Files.** `Available. Files SM-X620 sends still need your approval here.` /
`Available once SM-X620 is connected.` / `Files are not enabled for SM-X620.`
No file history in the panel.

---

## 12. Application and window lifecycle

| Rule | How it is true |
|---|---|
| Closing the panel does not stop `anyflowd` | Different process; the panel never touches it |
| Closing the panel does not stop the poll | The poll belongs to `App`, installed at startup |
| Closing the panel does not detach file approval | The `ApprovalHandle` belongs to `App` |
| Closing either window does not revoke trust | Nothing in the GUI writes the trust store |
| Reopening reconnects to current state | The panel is rebuilt from the live `DaemonState` |
| Daemon restart under an open panel recovers | The existing 2 s poll and the existing 2 s approval re-attach; no new strategy |
| No stale state after a restart | Every row is a pure function of the newest poll |

The approval provider moved from the window to the application. Dropping it —
which happens when the application exits — returns the daemon to declining
every offer, which is the safe direction and the behaviour a desktop with no
GUI has always had. It is parented to whichever window is in front when an
offer arrives; with no window there is none to ask over and the offer is left
unanswered, which the daemon resolves as a decline.

All verified on hardware — §24, steps 8–13.

---

## 13. The KDE activation seam (prepared, **not** implemented)

Two application actions, and a command line that is forwarded to the running
instance:

```
app.quick-panel        present the Quick Panel
app.settings           present the Settings window

anyflow-gui                  Settings — the existing behaviour, unchanged
anyflow-gui --quick-panel    the Quick Panel
anyflow-gui --page files     Settings, on one page
```

`GApplication` exports its action group on the session bus as
`org.gtk.Actions`, and the `.desktop` file declares `DBusActivatable=true`, so
a future StatusNotifierItem can raise the panel **by name** without linking
against this crate or knowing one thing about how the panel is built. The
`.desktop` file also exposes it as a launcher shortcut
(`[Desktop Action quick-panel]`), which is a third caller of the same seam.

`HANDLES_COMMAND_LINE` is load-bearing: without it a second
`anyflow-gui --quick-panel` would activate the running instance and the running
instance would never see the flag.

**No StatusNotifierItem is implemented. No KDE dependency is taken.** The seam
is all this sprint owes it.

Repeated invocation presents the window that exists rather than opening a
second one — a tray cannot be trusted not to click twice
(`the_quick_panel_is_created_once_and_presented_again`).

---

## 14. Brand direction

> **AnyFlow — One flow. Any device.**

The product carried two marks: a filled "A" for institutional use, and a
teal-to-violet ribbon between two dots as the product and application icon.
The ribbon-and-dots was the weaker half and is no longer the primary mark:

* at 16 px it reduces to two blobs and a hairline;
* it has no letterform to hold on to;
* "a line between two dots" is the most crowded space in this category.

**The Flow A** replaces it. One continuous ribbon that draws an A: up the right
leg to the apex, down the left leg, around a hook at the foot, and out again as
the crossbar. Continuity is the brand idea and here it is literal — the mark is
a single unbroken path, so there is nothing to come apart at small sizes and
nothing that depends on colour to be read.

One mark now, everywhere. The two-node ribbon survives only where it is
actually about a connection: the empty-state illustration, which is the job it
was always best at.

It does not resemble the Bluetooth or Wi-Fi glyph, KDE, Nearby Share, Phone
Link, AirDrop, or an infinity symbol. It reads as a letter first.

---

## 15. Logo construction

64-unit grid, 8-unit stroke, round caps and joins — the same grid, weight and
terminals as the existing family.

```
M 53 56                                   right foot
C 50 41  43 22  32 8                      up the right leg to the apex
C 22 21  15 36  10 47                     down the left leg
C 7.5 52.5  11.5 56.5  15.5 52            the hook at the foot
C 22 43.5  33 38  45 37.5                 the crossbar, flowing right
C 50 37.3  54 38  57 39                   out again
```

One `<path>`, one `M`, no lifts. Pinned by
`the_flow_a_is_one_continuous_ribbon`.

Gradient axis `(10,56) → (54,8)`, teal → blue → violet, the same axis the
family already used, so teal lands on the hook and violet on the apex and the
tail — the mark reads as flow.

**How it was chosen.** Four candidates were drawn, rendered at 16/24/32/64/128
and looked at; three more refinements followed. The rejected shapes are worth
recording because each failed at a size, not in principle: a strict
single-stroke A with the crossbar returning under the apex read as "Λ with a
tail"; another read as a "4". The tail-right form with a bottom-left hook was
the only one that still read as an **A** at 16 px in monochrome.

---

## 16. Brand assets

| File | Bytes | What |
|---|---|---|
| `docs/design/assets/logo-flow-a.svg` | 813 | Canonical. 64-grid, brand gradient |
| `docs/design/assets/logo-flow-a-mono.svg` | 522 | Monochrome, `stroke="currentColor"` |
| `docs/design/assets/logo-flow-a-small.svg` | 804 | Same geometry, stroke 9.5, for ≤ 24 px |
| `docs/design/assets/app-icon.svg` | 818 | 512 px rounded square on Ink, replaced |
| `desktop/gui/data/io.github.yurisismotto.anyflow.desktop` | — | Desktop entry + Quick Panel action |

All original, project-owned, hand-written vectors. No stock artwork, no
licensed material, no embedded font, no raster.

**No duplicated artwork.** `build.rs` derives the icon-theme copy —
`icons/scalable/apps/io.github.yurisismotto.anyflow.svg` — from the canonical
`app-icon.svg` into `OUT_DIR` at build time, so the second location a
`GtkIconTheme` needs cannot drift from the first.

`widgets::brand_mark(size)` picks the `-small` cut below 24 px: the canonical
8-unit stroke lands at 2.00 px at 16 px, which works but is only just over the
line, and the heavier cut lands at 2.38 px.

The old `icon-flowing-ribbon.svg` and `logo-flowing-a.svg` are left in place
untouched — Android's design-token test resources read that directory, and
deleting a mark is not this sprint's business.

---

## 17. Colour and system-theme behaviour

Libadwaita stays authoritative. Brand colour is spent on identity and nothing
else:

* the **mark** carries the gradient — and nothing else in the panel does;
* `Status::Connected`'s AA-corrected accent tints the "Connected" line and an
  `On` value. That is the existing semantic vocabulary, not a brand statement;
* an offline device's tile is `af-tile-neutral` — **neutral grey**;
* success, warning and error follow the existing accessible semantic tokens,
  which clear WCAG AA on their own surface and are pinned by `theme::tests`.

No new colour was introduced and no hex appears in the panel code.

The panel is theme-aware by construction: it uses the existing `theme` module,
whose light and dark sheets are swapped on `AdwStyleManager::dark` (dark is a
derived palette, not an inversion). Nothing is white-only, and no state is
carried by colour alone — every status is a word, and selection is a radio
**plus** the word "Selected" **plus** the row's accessible name.

The mono mark inherits `currentColor` and was rendered on both Paper `#F8FAFC`
and near-Ink `#0A0F1C` at all five sizes; it is legible on both.

---

## 18. Desktop metadata

There was none. Added:

```ini
[Desktop Entry]
Name=AnyFlow
Icon=io.github.yurisismotto.anyflow
Exec=anyflow-gui
DBusActivatable=true
Categories=Network;FileTransfer;
Actions=quick-panel;

[Desktop Action quick-panel]
Name=Quick Panel
Exec=anyflow-gui --quick-panel
```

`desktop-file-validate`: **clean, no warnings.**

The application id, the `GtkApplication` id, the D-Bus name and the icon name
are deliberately the same string. GNOME and KDE both match a window to its
launcher by that id and search an icon theme by it, so a mismatch is exactly
what makes a running window show a grey fallback square.

`install_icons()` adds the compiled-in `icons/` prefix to the default
`GtkIconTheme` and sets the default window icon name, so a build run straight
out of the source tree shows the right icon without anything installed under
`/usr/share/icons`.

Nothing about package identity changed. No binary, crate, protocol, package id
or Android application id was renamed. `packaging/fedora/anyflow.spec` was not
touched — it does not install the GUI today, and packaging is out of scope;
recorded as debt (§28).

---

## 19. Pure-model tests — 32, no display needed

`panel::model::tests`, covering every item of the required list:

| # | Test |
|---|---|
| A | `zero_peers_produces_no_target_and_no_action` |
| B | `one_connected_peer_is_the_target_without_being_chosen` |
| C | `one_offline_peer_appears_but_offers_no_live_action` |
| D | `several_peers_and_no_choice_asks_instead_of_guessing` |
| E | `the_choice_maps_to_the_same_peer_in_any_order` (three orderings) |
| F | `a_stale_choice_is_refused_rather_than_repointed` |
| G | `a_present_battery_at_zero_renders_as_a_real_zero` |
| H | `an_absent_battery_never_renders_as_zero` |
| I | `an_unavailable_battery_is_distinct_from_an_absent_one` |
| J | `the_file_action_needs_a_live_authorized_capability` |
| K | `the_clipboard_action_needs_every_precondition` |
| L | `the_notifications_row_reflects_grant_and_policy` |
| M | `an_offline_peer_cannot_receive_a_quick_action` |
| N | `a_display_name_collision_does_not_affect_routing` |
| O | `an_unreachable_daemon_produces_a_safe_state` |

Plus: `an_unanswered_first_poll_is_not_an_error`,
`a_revoked_device_is_not_in_the_panel_or_the_target_set`,
`rows_are_ordered_for_reading_not_for_routing`,
`a_stale_session_marks_its_battery_reading_as_history`,
`the_clipboard_row_states_each_direction_truthfully`,
`a_refusal_by_the_receiving_device_is_reported`,
`every_blocked_reason_is_a_sentence_not_an_error_code`,
`a_row_announces_its_state_in_words`.

And `selection::tests` — **7** — covering round-trip persistence, clearing,
rejection of anything that is not fingerprint hex, corrupt/foreign files
reading as "no choice", and the assertion that the stored file holds *nothing
but* the fingerprint and a schema number.

---

## 20. Action-routing tests — inside the 32 above

| # | Requirement | Test |
|---|---|---|
| A + B | Both actions target the selected fingerprint | `both_actions_target_the_selected_fingerprint` — asserted on the `Request` itself |
| C | Switching the peer switches the destination | `switching_the_selected_peer_switches_the_destination` |
| D | A display name never chooses | `a_display_name_never_chooses_the_destination` — names swapped between two peers, destination unmoved |
| E | List ordering never chooses | `list_ordering_never_chooses_the_destination` |
| F | No stale fallback | `a_stale_choice_never_falls_back_to_another_peer` — with one peer left **and** with several |
| G | No peer → no send | `with_no_peer_there_is_no_send` |
| H | One-peer convenience stays safe | `the_one_peer_convenience_stays_safe` |
| I | Revalidated by the lower layer | `a_quick_action_carries_only_a_fingerprint_for_the_daemon_to_recheck` |
| J | Failure does not move the selection | `a_blocked_action_does_not_move_the_selection` |

`send_file_request` / `send_clipboard_request` are pure functions in the model,
and the widget calls them and sends what they return — which is what lets "the
button sends to the selected fingerprint" be asserted on the request rather
than on a widget.

No lower-level cryptographic test was duplicated.

---

## 21. GTK and application-action tests — 12 display-gated sections

GTK binds itself to the thread that initialised it and libtest gives every
`#[test]` a thread, so all of these are *sections* called from the single
`views::display_gate` entry point, each failing with its own name:

```console
cargo test -p anyflow-gui -- --ignored --test-threads=1     # 1 test, 12 new sections, ok
```

**Panel (7):** row states everything in words · a disabled action says why · the
selected row is the chosen peer and says so · choosing a row stores that row's
fingerprint · one peer is not forced through a selector · an unreachable daemon
draws no live actions · the panel always offers the way into Settings.

**Application (5):** both surfaces reachable as actions (asserted on
`list_actions()`, and both parameterless) · the panel is created once and
presented again · closing the panel leaves the state object and the Settings
window · both windows belong to one application · opening a surface starts no
daemon (neither the poll nor the approval attachment is created by a window).

`App::for_test` installs actions and nothing else. The production path also
starts the poll and becomes the daemon's approval provider; a test must do
neither, because it would talk to whatever daemon the developer has running and
take the approval role away from their real session.

No window-coordinate assertions anywhere.

---

## 22. SVG validation — 8 tests

`desktop/gui/tests/brand_assets.rs`, running in CI with no display:

| Test | What it refuses |
|---|---|
| `every_asset_is_a_self_contained_svg` | no namespace, no `viewBox`, more than one root |
| `no_asset_reaches_outside_itself` | `<image>`, `data:image`, `href=`, `<use>`, `<script>`, `@font-face`, `<font>`, `.ttf`, `.woff`, `<text>`, `font-family`, any `http` other than the SVG namespace, `/home/`, `file://`, `c:\`, Inkscape/Sodipodi state |
| `every_paint_reference_resolves_inside_its_own_file` | a `url(#id)` with no matching `id` — which paints black silently |
| `gradient_ids_are_unique_across_the_family` | two marks in one binary sharing an id |
| `every_stroke_survives_the_smallest_size_it_is_drawn_at` | a stroke under one pixel at the size that asset is actually drawn at |
| `the_flow_a_is_one_continuous_ribbon` | more than one `<path>`, more than one `M`, a missing round cap, a filled mark |
| `the_monochrome_cut_inherits_its_colour` | any hard-coded colour in the mono cut |
| `the_app_icon_is_a_drawn_icon_with_its_own_ground` | a bare stroke on transparency, which vanishes on a dark shell panel |

Assets are **classified**, not lumped: the marks answer for 16 px;
`ribbon-connection.svg` is the empty-state illustration, drawn at 200 × 72 and
never as an icon, so holding it to an icon's floor would measure it against a
size it is never asked to be. A new or deleted asset fails the list.

Stroke widths at 16 px, measured (`stroke-width × group scale ÷ viewBox width ×
16`): `logo-flow-a` **2.00 px**, `logo-flow-a-small` **2.38 px**, `app-icon`
**1.55 px**, `icon-flowing-ribbon` **1.75 px**.

**Visual inspection.** All marks rendered locally through **librsvg 2.62.3** —
the same renderer GTK uses — at 16/24/32/64/128 px, as gradient, as monochrome
on Paper, as monochrome on near-Ink, and as the app icon. The letter reads at
every size in every form. Inspection renders were kept out of the repository.

---

## 23. Local quality results

Sequential, as required, `-j 2` throughout.

```console
$ cargo fmt --all --check                       clean
$ cargo test -p anyflow-gui -p anyflow-control   91 passed · 0 failed · 1 ignored
$ cargo test --workspace -j 2                   838 passed · 0 failed
$ cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings
                                                 exit 0 · zero warnings
$ cargo test -p anyflow-gui -- --ignored --test-threads=1
                                                 1 passed (12 new sections)
$ desktop-file-validate …anyflow.desktop         clean
```

`anyflow-gui`: **39 → 91** tests, plus 12 display-gated sections.
Workspace: **786 → 838**.

No Gradle was run: no Android file changed. `docs/design/assets` is an Android
**unit-test** resource directory (so `DesignTokensTest` can read
`tokens.json`), not a shipped asset source, and no Android test reads any SVG.

Builds were kept strictly sequential; no VM was booted.

---

## 24. Real Fedora + tablet physical test

Fedora 44, GNOME on Wayland, live `anyflowd`, real SM-X620 over the LAN.
Four trusted peers, **all named "SM-X620"** — a genuine display-name collision,
which made this a better test than a contrived one.

| # | Step | Result |
|---|---|---|
| 1 | Daemon running | ✅ pid 1991415, port 55432 |
| 2 | Tablet connected | ✅ `573C CB84 DA6C 993B`, `battery.v1`/`clipboard.v1`/`files.v1` granted |
| 3 | Open Quick Panel | ✅ `anyflow-gui --quick-panel`, 380 × 620 |
| 4 | Tablet appears once | ✅ one row per trusted peer, four rows, no duplicates, no revoked records |
| 5 | Correct selected device | ✅ with no choice: **no row selected**, "Choose a device above", both actions disabled |
| 6 | Battery matches the daemon | ✅ `Connected · 79% · Not charging`, and `79% (last known)` once stale |
| 7 | Capability state truthful | ✅ `Clipboard · Files`; tooltip `Notifications: not enabled` |
| 8 | Send Clipboard | ✅ pressed via AT-SPI; daemon accepted; tablet showed **"Clipboard from Fedora · 22 bytes · [Copy] [Dismiss]"** — 22 bytes is exactly the marker sent. Byte count only, no content, on both ends |
| 9 | Send File opens a native chooser | ✅ the platform chooser opened, titled "Send a file", via `xdg-desktop-portal` |
| 10–12 | File reaches Android, hash correct | ✅ 5 536-byte file offered to the **fingerprint the panel stored**; tablet prompted "Incoming file · anyflow-quickpanel-test.txt · 5,4 KB · from DF65 D3E4 BA28 EDF9"; accepted; pulled back and compared: `d5487dab…e85a` on both ends, `cmp` byte-identical |
| 13 | Switch device with several peers | ✅ chose the **offline** peer; target moved to it by fingerprint |
| 14 | Offline peer action disabled | ✅ `sensitive=False`, accessible description `SM-X620 is offline.` — and the *connected* device sitting directly above it did **not** become a fallback |
| 15 | Open Settings from the panel | ✅ same process, two frames: `AnyFlow` 380 × 607 and `AnyFlow Settings` 1000 × 680 |
| 16 | Close the panel | ✅ |
| 17 | Daemon and session survive | ✅ daemon pid unchanged, tablet still `connected`, Settings still open, GUI process unchanged |
| 18 | Reopen the panel | ✅ `anyflow-gui --quick-panel` from a terminal **forwarded to the running instance** — still one pid — and raised the panel |
| 19 | State is fresh | ✅ live state, and the stored choice survived the close/reopen |

**Beyond the required list:**

* **Daemon killed under an open panel.** The panel collapsed to
  *"AnyFlow service is not available / AnyFlow keeps trying. Your devices stay
  paired, and nothing is lost."*, dropped every device row and every action,
  and shrank to 380 × 214. No socket path, no `errno` on screen.
* **Daemon restarted under the open panel.** Recovered on its own with no
  reload, using the existing 2 s poll. Devices came back offline with
  `Clipboard: enabled, not available on this session` — the granted-but-not-live
  distinction — and no stale "connected" survived.
* **Incoming file with only the panel open.** The tablet sent the file back.
  The existing approval dialog appeared — *"Incoming file / SM-X620 wants to
  send you a file. / Verified device · 573C CB84 DA6C 993B / [Decline]
  [Accept]"* — parented to the Quick Panel. Accepted; stored at
  `~/Downloads/AnyFlow/`; SHA-256 identical. The application-scoped approval
  provider works and the panel neither weakened nor bypassed it.
* **Choice made from Settings, reflected in the panel with no reload** — one
  application, one selection.
* **What was stored**, after choosing from the Settings device card:
  `{"schema":1,"selected_peer":"573ccb84…a6b7"}` — the full fingerprint, not a
  device id, not a name, not an index.

No VM was booted. The three other trusted peers are preserved VM identities and
correctly showed as offline.

### The one link not driven by hand

Step 9 verified the chooser opens; steps 10–12 verified the transfer end to end
using the exact request the panel composes (`Request::Send` carrying the exact
fingerprint the panel had stored). The **chooser → path** handoff itself could
not be driven headlessly on this machine: the session is Wayland with no input
injection available, and the portal chooser exposes neither an action on its
file rows nor an editable search entry to AT-SPI. That handoff is two lines —
`GtkFileDialog::open`'s callback and `path.to_string_lossy()` — and everything
downstream of it is covered by `both_actions_target_the_selected_fingerprint`.
It is stated rather than glossed.

---

## 25. Accessibility evidence

Captured from the **running** application over AT-SPI on the real session.
Verbatim, with private content necessarily absent because the panel holds none:

```text
=== WINDOW: 'AnyFlow'  role=frame ===
    extents: 380 x 607 at (0,0)
[frame] name="AnyFlow"
  [list box] name="Devices. Choose which one AnyFlow sends to."
    [list item] name="SM-X620, selected device, connected, battery 79% · Not charging,
                      available: Clipboard, Files"
      [radio button] name="Send to SM-X620"
      [label] name="SM-X620"
      [label] name="Connected · 79% · Not charging"
      [label] name="Clipboard · Files"  desc="Clipboard: available
                                              Files: available
                                              Notifications: not enabled
                                              Fingerprint: 573C CB84 DA6C 993B"
      [grouping] name="Selected device"
        [label] name="Selected"
    [list item] name="SM-X620, offline, no capabilities available"
      [radio button] name="Send to SM-X620"
      [label] name="Offline"
      [label] name="Available when connected"  desc="Clipboard: not enabled
                                                     Files: not enabled
                                                     Notifications: not enabled
                                                     Fingerprint: 3B38 1925 F8A6 E49D"
    [button] name="Send file to SM-X620"
    [button] name="Send clipboard to SM-X620"
    [grouping] name="Notifications: Off"  desc="Notifications are not enabled for SM-X620."
    [grouping] name="Clipboard: On"       desc="Sends this computer's clipboard only when you
                                                press Send clipboard. Clips from SM-X620 are
                                                held until you apply them. SM-X620 sends only
                                                when you ask it to there."
    [grouping] name="Files: On"           desc="Available. Files SM-X620 sends still need your
                                                approval here."
    [button] name="Open AnyFlow Settings"
```

With the offline peer chosen:

```text
'Send file'      sensitive=False  desc='SM-X620 is offline.'
'Send clipboard' sensitive=False  desc='SM-X620 is offline.'
```

With the connected peer chosen: `sensitive=True`, and the button's accessible
name becomes `Send file to SM-X620` — the destination is in the name, not only
in a caption.

Checklist:

* **Keyboard-operable** — every control is a focusable GTK control. Device
  choice is a `GtkCheckButton` in radio mode, operated with Space; actions are
  buttons.
* **Sensible tab order** — devices, then actions, then status, then Settings;
  document order, no overrides.
* **Accessible names** — every control has one; the icon-only Settings gear is
  named `Open AnyFlow Settings`.
* **Selected device announced** — in the row's accessible name
  (`selected device`), in the radio's state, and in the word "Selected".
* **Connected/offline announced** — in words, in the row name and on the row.
* **Battery announced** — `battery 79% · Not charging`, never a bare number,
  and omitted rather than faked when unavailable.
* **Disabled actions expose why** — accessible `Description` and tooltip.
* **No colour-only meaning** — every status is a word; selection is a radio
  *and* an icon *and* a word *and* the accessible name.

### Two accessibility defects found and one GTK limitation recorded

Both were found by reading the accessibility tree of the running panel, not by
reasoning about the code.

1. **A device announced itself as selected before anyone had chosen.** A
   `GtkListBox` in `SelectionMode::Single` selects a row by itself when the
   list first takes keyboard focus — which a freshly opened panel does — and
   GTK reports that as a `SELECTED` state. The panel was simultaneously saying
   "choose a device above" with both actions correctly disabled. Nothing routed
   wrongly (a destination comes from the stored fingerprint, never from a
   widget), but a screen reader was being told the opposite of what the screen
   said. `unselect_all()` does not hold — the focus handler selects again.
   **Fixed** by taking the selection state away from the widget entirely:
   `SelectionMode::None`, and `State::Selected` set from the model on every row.
2. **An activatable `GtkListBoxRow` exposes no AT-SPI action** — measured,
   `get_n_actions() == 0` — so the row was something an assistive technology
   could read and could not operate. **Fixed** by making the choice a radio
   button, which is a first-class keyboard control.
3. **`GtkCheckButton` also reports zero AT-SPI actions** on GTK 4.20 here,
   while `GtkButton` correctly reports `click`. The radio remains focusable and
   Space-operable, so keyboard access is intact, but an AT that drives by
   action cannot toggle it. This is a GTK-side gap, not an AnyFlow one, and is
   recorded as debt (§28).

**Screenshots were not captured.** This is a GNOME Wayland session: the
GNOME screenshot portal is denied to this environment and the Xwayland root is
not composited, so `import -window root` produces nothing. Screenshots are
evidence rather than pass criteria, and the accessibility tree above is the
stronger record. The *brand* marks were rendered offline through librsvg at
every size and inspected visually (§22).

---

## 26. Privacy and security audit

**Privacy — the panel is not a history.**

| Content | Where it can appear | Where it cannot |
|---|---|---|
| Clipboard text | Nowhere. The daemon reads the clipboard; no clip crosses the control socket and none enters this process | The panel, any log, any file |
| Notification title/body/app | Nowhere. `NotificationsStatusReport` has no field that could carry one | Structurally impossible |
| File contents | Nowhere. The GUI hands the daemon a path; the bytes never enter this process | — |
| Filename | Only on an **in-flight** transfer line, and in the chooser and the approval dialog the files policy already allows | No completed-transfer list, no history |

Nothing is persisted but one public fingerprint hex and a schema number, in
`$XDG_CONFIG_HOME/anyflow/gui.json`, asserted by
`the_stored_file_holds_nothing_but_the_fingerprint`. No OTP is exposed — no
notification content reaches the panel at all. No telemetry, no analytics, no
cloud, no crash-reporting SDK; no dependency was added to the crate.

**Security — no model change.** Preserved, and none of it touched:

TLS 1.3 · SPKI pinning · proof-of-possession · per-peer grants ·
`SensitiveCapabilities.NEVER_AUTO_GRANTED` · fingerprint-based routing ·
incoming file approval · notification filtering · lock fail-closed.

The Quick Panel is a view and controller over already-authorized operations. It
grants nothing. `Action::Ready` means "the daemon last said this was possible"
and the daemon re-checks every grant when the request arrives —
`FilesAuthorizer::is_authorized` reads the trust store fresh, including against
a stream already mid-copy — which is exactly why this layer is allowed to work
from a two-second-old poll.

The one place a value crosses from the GUI into the daemon is the stored
fingerprint, and it is validated as hex both when written and when read.

Incoming file approval was **strengthened, not weakened**: it now survives a
window closing, and was verified live with only the Quick Panel open.

---

## 27. Files changed

**New (7):**

| Path | Lines | What |
|---|---|---|
| `desktop/gui/src/panel/model.rs` | 1 056 | The panel's decisions, pure, no GTK |
| `desktop/gui/src/panel/model/tests.rs` | 908 | 32 model and routing tests |
| `desktop/gui/src/panel/mod.rs` | 1 047 | The panel widgets + 7 display-gated sections |
| `desktop/gui/src/selection.rs` | 289 | The persisted fingerprint choice + 7 tests |
| `desktop/gui/tests/brand_assets.rs` | 299 | 8 SVG validation tests |
| `desktop/gui/data/io.github.yurisismotto.anyflow.desktop` | 31 | Desktop entry + Quick Panel action |
| `docs/design/assets/logo-flow-a{,-mono,-small}.svg` | — | The Flow A |

**Modified (11):**

| Path | Δ | What |
|---|---|---|
| `desktop/gui/src/lib.rs` | +1042/−… | Application architecture: actions, command line, app-owned poll/selection/approval, `build_settings`, 5 launch tests + 5 window sections |
| `desktop/gui/src/views/dashboard.rs` | +157/−… | Destination is the shared model, not list position; "Use this device" / "Selected for quick actions" |
| `desktop/gui/src/views/mod.rs` | +64 | `Pages` carries the selection; `choose_peer`, `set_redraw`, `redraw_now` |
| `desktop/gui/src/approval.rs` | +28 | `install(parent_resolver)` — application-scoped |
| `desktop/gui/src/widgets.rs` | +37 | `brand_mark` → the Flow A, with the small cut below 24 px; `brand_logo` removed |
| `desktop/gui/build.rs` | +24 | Derives the icon-theme copy from the canonical app icon |
| `desktop/gui/data/anyflow.gresource.xml` | +13 | New marks and the `icons/` theme prefix |
| `docs/design/assets/app-icon.svg` | +12/−12 | The Flow A on the Ink ground |
| `desktop/gui/src/views/settings.rs` | +5 | About uses the one mark |
| `desktop/gui/src/views/{clipboard,notifications}.rs` | +6 | Test-only: `Pages::new` takes a selection |

`git diff --stat`: **11 files, 1 079 insertions, 309 deletions**, plus 7 new
paths. `git diff --check`: clean.

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` — untouched, unstaged, still untracked.
`git add .` was never used.

---

## 28. Remaining debts

**Carried forward, untouched** (none blocked this work):

* UX-DEBT-04 — phone physical orientation coverage
* UX-DEBT-05 — revoked peer card recovery affordance
* UX-DEBT-06 — stale desktop peer records. *Visible in this sprint:* the live
  daemon holds 11 revoked SM-X620 records. The panel filters them out, so it is
  unaffected; Settings still lists them.
* UX-DEBT-07 — PairingGate physical-race evidence
* Android lint
* Two cosmetic inline hex conversions
* `anyflow pair` terminal fallback printing the QR payload

**Newly discovered, recorded rather than fixed:**

| # | Debt |
|---|---|
| QP-DEBT-01 | The chosen device is a GUI preference. The daemon should own it so the CLI and a future tray agree without anyone writing a file — needs one new control request, which this sprint's no-protocol rule excludes. |
| QP-DEBT-02 | `GtkCheckButton` reports zero AT-SPI actions on GTK 4.20 while `GtkButton` reports `click`. Keyboard access is intact; an AT driving by action cannot toggle the device choice. Upstream. |
| QP-DEBT-03 | No GTK-*symbolic* cut of the mark. GTK recolours a symbolic icon by forcing `fill`, which turns a stroked glyph into a blob, so the mono cut is stroke-based and is not a `-symbolic` asset. A KDE tray that wants symbolic recolouring will need the stroke outlined to a filled path — best done by a committed generator, not by hand. |
| QP-DEBT-04 | `packaging/fedora/anyflow.spec` installs neither `anyflow-gui`, the `.desktop` file, nor the hicolor icon. Packaging is out of scope; the metadata is now in the tree ready for it. |
| QP-DEBT-05 | No AppStream `metainfo.xml`. Needed for a software-centre listing; packaging-adjacent. |
| QP-DEBT-06 | The `Send clipboard` toast says "sent" when the frame is on the session. The receiver's verdict now reaches the status row (§9), but the toast itself is still optimistic by one round trip. |
| QP-DEBT-07 | Screenshot evidence is unavailable in this environment (Wayland; portal denied). A visual regression pass needs a session where capture is permitted. |

---

## 29. Git state at close

```console
$ git branch --show-current
feature/quick-panel-branding-v1

$ git status --short
 M desktop/gui/build.rs
 M desktop/gui/data/anyflow.gresource.xml
 M desktop/gui/src/approval.rs
 M desktop/gui/src/lib.rs
 M desktop/gui/src/views/clipboard.rs
 M desktop/gui/src/views/dashboard.rs
 M desktop/gui/src/views/mod.rs
 M desktop/gui/src/views/notifications.rs
 M desktop/gui/src/views/settings.rs
 M desktop/gui/src/widgets.rs
 M docs/design/assets/app-icon.svg
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? desktop/gui/data/io.github.yurisismotto.anyflow.desktop
?? desktop/gui/src/panel/
?? desktop/gui/src/selection.rs
?? desktop/gui/tests/
?? docs/design/assets/logo-flow-a-mono.svg
?? docs/design/assets/logo-flow-a-small.svg
?? docs/design/assets/logo-flow-a.svg

$ git diff --check
(clean)

$ git diff --stat
 11 files changed, 1079 insertions(+), 309 deletions(-)
```

Nothing staged, nothing committed, nothing pushed, no PR opened.

---

## 30. Scope check

Verified **not** implemented on this branch:

| Excluded | Status |
|---|---|
| KDE StatusNotifier | Not implemented. Activation seam only; no KDE dependency |
| GNOME extension | Not implemented |
| AppIndicator backend | Not implemented |
| Windows tray / macOS menu bar | Not implemented |
| RPM/DEB packaging | Not touched. `anyflow.spec` unchanged |
| Android visual redesign | No Android source file changed |
| Android lint cleanup | Not touched |
| Battery hotplug | Not touched |
| Notification queue recovery | Not touched |
| Concurrent multi-peer sessions | Not touched |
| Protocol / protobuf changes | **None.** `protocol/` untouched; `anyflow-control` untouched |
| Cloud / telemetry | None. No dependency added |

> **Superseded in part by the pre-PR polish.** The row above was true at the
> §31 close and is left as written. It is **no longer true of the branch**:
> the polish added two additive fields to `anyflow-control`'s `TransferReport`
> (`seq`, `failure_code`). `protocol/` is still untouched and there is still no
> protobuf or device-to-device wire change — but `anyflow-control` is no longer
> untouched. See §32.2 for the exact scope of what changed.

CI workflows were not redesigned. The touched area is `desktop/gui/**` plus
`docs/design/assets/**`, which `desktop-quality.yml` and
`linux-distro-compat.yml` classify on `desktop/**` and pick up unchanged. The
new test target `desktop/gui/tests/brand_assets.rs` is **not** covered by
`portable-windows-msvc.yml`'s classification guards, which watch `core/tests`
and `capabilities/notifications/tests`: it is a GUI-crate target reading desktop
artwork and is outside the portable boundary by construction. No guard needed
updating and no Windows portability was weakened.

---

## 31. Verdict

```
QUICK PANEL + BRANDING V1:     PASS
QUICK PANEL STATE MODEL:       PASS
MULTI-PEER ACTION ROUTING:     PASS
FILES QUICK ACTION:            PASS
CLIPBOARD QUICK ACTION:        PASS
PANEL LIFECYCLE:               PASS
ACCESSIBILITY:                 PASS
ANYFLOW BRAND V1:              PASS
SECURITY / PRIVACY REGRESSION: PASS
```

None of the Quick Panel failure conditions applies: routing never reads a
display name or a list position; a stale selection reaches nobody; battery
absence never renders as `0%`; closing either window leaves the daemon and the
session untouched; the panel keeps no content history; and Files and Clipboard
go through the existing grants, re-checked by the daemon on arrival.

None of the Branding failure conditions applies: the mark reads as an **A** at
16 px, works in pure monochrome on light and on dark, contains no external
reference, raster or font blob, and the application id, `GtkApplication` id,
D-Bus name and icon name are one consistent string.

---

# Pre-PR polish — 17 September 2026

Two items, scoped and closed. Sections 1–31 above are the original
certification and were not rewritten.

---

## 32. QP-POLISH-01 — Recent transfers

### 32.1 The audit came first, and changed the design

The brief said not to invent state names. The daemon's actual vocabulary,
read out of `capabilities/files/src/transfer.rs`, is smaller than the brief's
example list:

| | Values |
|---|---|
| `TransferState` | `offered` · `waiting_accept` · `transferring` · `verifying` · **`completed`** · **`failed`** · **`cancelled`** |
| Terminal | those last three, and only those |
| `Direction` | `sending` · `receiving` |
| `FailureReason` | 12 variants, including `DeclinedByUser`, `TimedOut`, `Transport`, `CancelledByUser` |

So there is **no `declined` state and no `timed_out` state** — "declined" and
"timed out" are *reasons* attached to `cancelled` and `failed`. A panel that
wants to say `Declined` has to read the reason, not the state.

Two things stood in the way, and both were the same kind of problem: the
daemon holds the fact and the control socket was throwing it away.

**1. "Newest first" was not derivable.** `TransferManager` keys transfers by
`TransferId` — 128 random bits — in a `BTreeMap`, and `snapshot()` iterates
its values. The order the GUI receives is therefore *the order of a random
number*. Measured on the live daemon before any change:

```console
$ anyflow transfers
  227eae14  sending -> SM-X620      AGENTS.md
  a1fbf0ae  receiving <- SM-X620    anyflow-quickpanel-test.txt
```

`22… < a1…`, and that is the whole reason for the order. `TransferRecord`
already carries `seq` — *"Creation order, so the oldest finished records can be
dropped first. A transfer id is random and therefore says nothing about age"* —
and it was not on the snapshot.

The alternative was to have the GUI remember the order transfers appeared in
across polls. That was rejected: it cannot order transfers that finished
before the panel opened, which is the ordinary case, and it would have meant
labelling an arbitrary order "newest first". Sorting by list position is the
defect this whole project exists to refuse.

**2. "Declined" was only available as prose.** `TransferReport::failure` is
`FailureReason::as_str()` — `"declined by the user"`, `"timed out"`. Branching
on that would make rewording a user-facing sentence a silent behaviour change
in another crate, and `anyflow-gui` depends only on `anyflow-control`, so it
cannot see the enum to pin the strings against.

### 32.2 What was added below the GUI

Two additive fields on the local control socket. Stated precisely, because the
distinction matters and is easy to blur:

* **No protobuf change.** `protocol/` is untouched; no `.proto` file, message or
  enum was added, removed or renumbered.
* **No Android ↔ desktop network protocol or wire change.** Nothing that
  crosses the TLS session between devices was altered — not a frame, not a
  field, not a negotiation.
* **No `files.v1` transport or state-machine change.** `TransferState`, its
  transition table, `FailureReason` as a set, and every byte path are exactly
  as they were. `FailureReason::code()` is a new *projection* of an existing
  variant, not a new variant.
* **The local desktop control IPC schema DID evolve, additively.**
  `TransferReport` gained two fields — `seq` and `failure_code`. That is a
  change to the contract between `anyflowd` and its own local front ends
  (`anyflow`, `anyflow-gui`) over the Unix control socket, and it is a real
  schema change, not a no-op.
* **Both new fields are `#[serde(default)]`**, so a newer front end parses an
  older daemon's report rather than failing on it. This was not a theoretical
  precaution: it was exercised by accident against the live old daemon, below.

So: a **local control-contract evolution**, not a `files.v1`
transport/state-machine protocol change, and not a device-to-device wire
change. Both fields expose a fact the daemon already held and was discarding on
the way out:

| Crate | Change |
|---|---|
| `capabilities/files` | `TransferSnapshot.seq` (from the record's existing counter); `FailureReason::code()` → a stable token; `FailureReason::ALL` |
| `control` | `TransferReport.seq` and `TransferReport.failure_code`, both `#[serde(default)]`; new `transfer_state`, `transfer_failure`, `transfer_direction` token modules |
| `runtime` | carries both through `transfer_report`; three new tests pinning the two vocabularies to each other |

`failure` (prose) and `failure_code` (token) now sit side by side on purpose:
the CLI and the Settings transfer list show the sentence verbatim, and the
panel branches on the token. Rewording one cannot move the other.

The token set lives in `anyflow-control` because that crate is the contract
between the agent and its front ends, and a front end may not depend on a
capability crate to learn it. `desktop/runtime` is the only crate that can see
both halves, so that is where the correspondence is asserted — exhaustively,
in both directions, so neither a new `FailureReason` nor an orphaned token
passes:

```rust
every_failure_reason_is_a_token_the_control_protocol_names
every_transfer_state_is_a_token_the_control_protocol_names   // incl. is_terminal agreement
both_directions_are_named
```

**The version skew was then observed for real.** Before the daemon was
restarted, the *old* `anyflowd` was still running against the *new* GUI:

```json
{"seq": null, "filename": "AGENTS.md", "direction": "sending", "state": "completed", "failure_code": null}
```

`#[serde(default)]` absorbed both absences, the panel drew the rows, and the
order fell back to the deterministic filename tie-break rather than failing to
parse the report. That is the behaviour the existing `ClipboardStatusReport`
comment asks for, confirmed by accident on live hardware.

### 32.3 The recent-transfer model

All of it in `panel::model`, all of it pure, all of it tested with no display.

```rust
Outcome   Sent | Received | Declined | Cancelled | TimedOut | Disconnected | Failed

RecentTransfer {
    filename,                  // sanitised by the daemon
    peer_name,                 // the TRUST STORE's name for this transfer's peer
    peer_fingerprint_short,    // short form only, and only in the description
    outgoing,                  // from the transfer, never guessed
    outcome,
    sort_key,                  // the daemon's seq — ordering only, never shown
}

RECENT_LIMIT = 3
```

Mapping, from the daemon's terminal state and its failure **token**:

| Daemon says | Panel says |
|---|---|
| `completed` + `sending` | **Sent** |
| `completed` + `receiving` | **Received** |
| any terminal + `declined_by_user` | **Declined** |
| any terminal + `cancelled_by_user`, or a bare `cancelled` | **Cancelled** |
| `failed` + `timed_out` | **Timed out** |
| `failed` + `transport` | **Disconnected** |
| any terminal + any other reason | **Failed** |

`Disconnected` earns its own word because the brief asked for transport to be
named *if represented*, and it is: `FailureReason::Transport`. A reason this
build does not recognise reads as `Failed` — never as a success, which is the
safe direction for a newer agent.

Three decisions worth stating:

* **Direction is never guessed.** `outgoing(direction)` returns
  `Option<bool>` from the daemon's own two tokens, and a transfer whose
  direction this build does not recognise is **not shown at all**. Every row
  reads "to" or "from" somebody, so a guess would not be a vaguer label — it
  would be a false statement about where a file went. (This also removed a
  latent inaccuracy: the previous code matched `"outgoing"`, a value the
  daemon has never sent.)
* **The one-word status does not say who refused.** Outgoing + declined means
  the peer said no; incoming + declined means this computer did. Same word,
  opposite events — so the *description* carries it: `SM-X620 declined this
  file.` against `This computer declined … from SM-X620.`
* **The tie-break keeps the order total**, so two rows cannot swap places
  under the pointer between polls.

### 32.4 Why it is not a history

| | |
|---|---|
| Storage added | **None.** The rows are read from the daemon's in-memory list for the current run |
| Restart | Empties it. Verified live: a freshly restarted daemon answers *"no transfers since the daemon started"* and the panel draws no section |
| Cap | 3, `truncate`d in the model |
| In-flight | Excluded — it has its own progress line, and cannot appear twice |
| Empty | No section, no heading, no button |
| Carried | A filename, a peer name, a direction, an outcome |
| **Not** carried | file contents, hash, size, mime type, stored path, transfer id, full fingerprint |

`a_recent_row_carries_no_content_hash_or_identifier` asserts the last row of
that table against *every string the model can produce* and against the
struct's own `Debug` output, so a row cannot grow one of them by someone
drawing more of it later.

The short fingerprint is the one identifier kept, in the accessible
description and the tooltip only, never on the visible row. It is what
disambiguates two devices with the same display name for someone who cannot
see the screen, and it is already what the panel's peer rows expose.

### 32.5 Multi-peer safety

A recent row's identity is **the transfer's**, not the selection's.
`recent_transfers(state)` is handed the daemon state and nothing else — not
`chosen`, not `target` — so there is no value in scope it could re-label a
historical row with.

| Rule | How it is true |
|---|---|
| The peer shown is the peer that participated | `device_name` and `fingerprint_short` come off the `TransferReport` |
| Reading history is not a routing decision | `app.transfers` calls `show_settings(Some(Page::Files))` and touches no selection |
| The list is not a routing authority | `Target::resolve` never sees it |
| A stale choice still asks | Even when a recent transfer names a peer that does exist |

Asserted by `a_display_name_collision_does_not_affect_transfer_identity` (two
peers both named `SM-X620`; the row is byte-identical whichever is selected),
`a_recent_transfer_does_not_change_the_selected_peer`, and the display-gated
`opening_the_transfer_list_does_not_change_the_chosen_device`.

### 32.6 View all transfers

```
app.transfers        present Settings on its Transfers page
```

A third **parameterless** application action rather than a parameter on
`app.settings`, which stays parameterless so it can go on a button, a
keybinding or the exported `org.gtk.Actions` interface without a caller having
to know a variant type. It presents the Settings window that already exists,
on the page that already exists; it builds no second window, starts no second
process, and creates no new Files screen. A tray item would be its second
caller and needs nothing new.

The button carries `app.transfers` as its `action-name` — it has no click
handler of its own, which is what the display-gated section asserts.

### 32.7 Accessibility

Rows are **static**, because there is nothing useful for clicking one to do:
this polish adds no file-manager action, and a control that looks live and is
not is worse than a label. The one control is a real `GtkButton`, the only
widget this application has measured as reliably operable by an assistive
technology.

Captured from the running panel over AT-SPI, verbatim:

```text
[heading] 'Recent transfers'
[grouping] 'Received AGENTS.md.txt from SM-X620'
    desc='AGENTS.md.txt arrived from SM-X620. Device 573C CB84 DA6C 993B.
          Open AnyFlow Settings for the full list.'
  [image] 'From'
  [label] 'AGENTS.md.txt'
  [label] 'From SM-X620 · Received'
[grouping] 'Sent recent-accepted.txt to SM-X620'
    desc='recent-accepted.txt reached SM-X620. Device 573C CB84 DA6C 993B. …'
  [image] 'To'
  [label] 'recent-accepted.txt'
  [label] 'To SM-X620 · Sent'
[grouping] 'Declined recent-sent.txt to SM-X620'
    desc='SM-X620 declined this file. Device 573C CB84 DA6C 993B. …'
  [image] 'To'
  [label] 'recent-sent.txt'
  [label] 'To SM-X620 · Declined'
[button] 'View all transfers'
    desc='Opens the Transfers page in AnyFlow Settings. It does not change
          which device you send to.'
```

The direction appears three times and never only as an arrow: in the glyph, in
the word *To*/*From* on the row, and as a sentence in the accessible name. The
arrow image is labelled and non-focusable so a screen reader does not say the
direction twice.

Outcome is a word, never colour alone; the colour is the existing semantic
`Connected`/`Disconnected` token, not a brand statement.

---

## 33. Recent transfers — real hardware

Fedora 44, GNOME 50.4 on Wayland, GTK 4.22.4, live `anyflowd`, real SM-X620
(`573C CB84 DA6C 993B`) over the LAN, four trusted peers all named `SM-X620`.
No VM booted. The daemon was restarted onto the new build first, which is why
its transfer list starts empty.

| # | Step | Result |
|---|---|---|
| 1 | Daemon restarted on the new build | ✅ pid 2234093; `no transfers since the daemon started` |
| 2 | Tablet reconnected by itself | ✅ `connected`, `battery.v1`/`clipboard.v1`/`files.v1` granted |
| 3 | Quick Panel opened with nothing finished | ✅ **no heading, no rows, no button** — and the window measured **380 × 620**, against 686 with the section present, so it is absent rather than empty |
| 4 | Device chosen through the Settings card | ✅ `{"schema":1,"selected_peer":"573ccb84…a6b7"}` |
| 5 | File sent Fedora → tablet, **refused** on the tablet | ✅ daemon: `cancelled` / `declined by the user`; panel: **`To SM-X620 · Declined`**, described as *"SM-X620 declined this file"* — the outgoing sense, not "this computer declined" |
| 6 | File sent Fedora → tablet, **accepted** on the tablet | ✅ tablet prompted *"Incoming file · recent-accepted.txt · 77 B · from DF65 D3E4 BA28 EDF9"*; daemon `completed`; panel: **`To SM-X620 · Sent`** |
| 7 | File sent tablet → Fedora, approved on Fedora | ✅ the existing approval dialog appeared and was accepted; stored at `~/Downloads/AnyFlow/AGENTS.md.txt`; panel: **`From SM-X620 · Received`** |
| 8 | Newest first, by the daemon's counter | ✅ see below |
| 9 | A fourth transfer | ✅ daemon holds **4**, panel draws **3** — the oldest row dropped |
| 10 | `View all transfers` pressed over AT-SPI | ✅ `n_actions=1`, sensitive; the **existing** Settings → Transfers page was selected and shown |
| 11 | The selection after opening history | ✅ **unchanged**: `573ccb84…a6b7` before and after |
| 12 | Panel closed and reopened | ✅ one process throughout; the rows and the choice both came back |

**Ordering, against real `seq` values.** The daemon's own numbering and what
the panel drew:

```text
daemon (seq desc)                               panel, top to bottom
  4  recent-fourth.txt    sending   completed     Sent recent-fourth.txt to SM-X620
  3  AGENTS.md.txt        receiving completed     Received AGENTS.md.txt from SM-X620
  2  recent-accepted.txt  sending   completed     Sent recent-accepted.txt to SM-X620
  1  recent-sent.txt      sending   cancelled     (dropped — the cap is 3)
        failure_code: declined_by_user
```

Three rows for four transfers, newest first, oldest dropped, and
`failure_code` present on the **local control socket** — not on any
device-to-device wire, which carries nothing new (§32.2).

**The division of labour, observed.** The same four transfers on the Settings
page at the same moment carried everything the panel leaves out — `39 B · to
SM-X620`, `3.8 KB · from SM-X620`, the daemon's own sentence *"declined by the
user"*, and `Saved to /home/yuri/Downloads/AnyFlow/AGENTS.md.txt`. None of
that appears on a panel row.

**Not driven by hand.** Catching an in-flight transfer in the panel and
confirming it is not *also* drawn as an outcome needs a transfer slow enough to
observe; these were 39–3 800 bytes on a LAN. The case is covered by
`an_in_flight_transfer_is_not_duplicated_into_the_recent_list`, which asserts
it for **every** non-terminal state, and by the Settings page showing
`from SM-X620 · waiting_accept` for a transfer the panel had given no outcome
at the same moment (step 7). It is stated rather than glossed.

Two harness notes, for whoever repeats this: a `GtkButton` reached over AT-SPI
during the 2 s poll's redraw can be destroyed before `do_action` lands — the
device-choice press needed one retry — and reading the widget tree immediately
after a press reads it before GTK's main loop has run the action.

---

## 34. BRAND-POLISH-01 — the generic icon: root cause

The previous sprint added the canonical marks, `app-icon.svg`, the `.desktop`
file, the `GtkApplication` id and a compiled-in icon theme, and GNOME still
drew a generic square. The brief said not to assume the fix was another SVG.
It was not; **no artwork was wrong**. The chain that resolves a window to an
icon was broken in two places, both outside the process.

### 34.1 What was measured, not assumed

**The app_id was already correct.** From the client's own Wayland traffic:

```console
$ WAYLAND_DEBUG=1 anyflow-gui --quick-panel
 -> xdg_toplevel#78.set_title("AnyFlow")
 -> xdg_toplevel#78.set_app_id("io.github.yurisismotto.anyflow")
```

**There is no window-icon channel on this session at all.** Every global
Mutter 50.4 advertises, filtered for one:

```console
$ grep -oE 'global\([0-9]+, "[a-z_0-9]+"' wl-debug.log | grep -i icon
(nothing)
```

No `xdg_toplevel_icon_manager_v1`, and xdg-shell has no equivalent of X11's
`_NET_WM_ICON`. In 1 572 lines of protocol log the client sends **no**
icon-related request of any kind. So `gtk::Window::set_default_icon_name` —
which the previous sprint relied on — has nowhere to send anything under
Wayland, and the compiled-in `GtkIconTheme` resource path is private to the
AnyFlow process and invisible to GNOME Shell, which is a different program.

**Both remaining links were missing.** Asked of the live session:

```python
Gio.DesktopAppInfo.new("io.github.yurisismotto.anyflow.desktop")
  -> TypeError: constructor returned NULL
```
```console
$ find ~/.local/share/icons /usr/share/icons -name '*anyflow*'
(nothing)
```

`~/.local/share/applications/` held only `mimeapps.list` and
`mimeinfo.cache`. The `.desktop` file and the icon existed **only inside the
source tree and inside the binary**.

### 34.2 The root cause, stated

On Wayland the shell derives a window's icon entirely outside the application:

```text
xdg_toplevel.set_app_id("io.github.yurisismotto.anyflow")   <- the application
    -> the .desktop file with that id, from XDG_DATA_DIRS    <- the session
        -> its Icon= name
            -> that name in the SHELL's icon theme
```

Only the first line was ever ours, and it was already right. The second and
third steps both missed, so GNOME Shell had no launcher to match the window
to, fell back to a window-backed app, and — having no icon from the client
either — drew a generic glyph. No error is produced anywhere in that path.

### 34.3 Why it was missed: the X11/Wayland asymmetry

The previous approach was not wrong in general — it is right on X11, and that
is exactly why it looked correct. Measured under Xwayland on the same build:

```console
$ xprop -id 0xe00004 WM_CLASS _NET_WM_ICON
WM_CLASS(STRING) = "anyflow-gui", "anyflow-gui"
_NET_WM_ICON(CARDINAL) =        Icon (48 x 48):    [the Flow A, correctly rendered]
```

On X11 GTK resolves the default icon name through the in-process theme and
attaches the result to the window itself, so the compiled-in icon reaches the
shell in-band and works with nothing installed. On Wayland that transport does
not exist. One mechanism, two very different outcomes.

That measurement also produced a **second, separate defect**: the X11
`WM_CLASS` is `anyflow-gui` — GTK takes a Wayland app_id from the
`GApplication` id but an X11 `WM_CLASS` from the program name — so a shell
matching an X11 window would look for `anyflow-gui.desktop` and find nothing.
Fixed with `StartupWMClass=anyflow-gui`, which is the key that exists for
precisely this, and which is now pinned by a test.

---

## 35. Icon integration — the fix

### 35.1 One installer, two callers

`desktop/gui/tools/install-desktop-metadata.sh` installs the two files the
session needs:

```console
$ ./desktop/gui/tools/install-desktop-metadata.sh --link-binary "$PWD/desktop/target/debug/anyflow-gui"
installed /home/yuri/.local/share/applications/io.github.yurisismotto.anyflow.desktop
installed /home/yuri/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
linked    /home/yuri/.local/bin/anyflow-gui -> …/desktop/target/debug/anyflow-gui
```

Both files are installed **verbatim**. Nothing is rewritten, generated or
templated, which is what makes it impossible for a development run and a
package to disagree about the application's identity. `--link-binary` is what
makes `Exec=anyflow-gui` resolvable from a source tree without the desktop
file having to say anything different from what a package will ship.

Ready for the packaging sprint by construction — same script, same two files,
different prefix, and `DESTDIR` so it never writes into a live `/usr`:

```console
$ ./install-desktop-metadata.sh --prefix /usr --destdir "$RPM_BUILD_ROOT"
```

It refuses a relative prefix, validates the entry with
`desktop-file-validate` *before* installing it (a malformed entry is ignored
silently, which looks exactly like the script not having run), skips the cache
refresh in `DESTDIR` mode, contains no `sudo`/`pkexec`/`doas`, and has
`--uninstall`. Round-trip verified into a throwaway prefix.

### 35.2 What the application does *not* do

`install_icons()` was **kept** — it is correct and load-bearing on X11, and
the in-process theme is what the About dialog and the header mark use — but its
documentation was wrong in a way that caused this defect, claiming it made the
icon resolve "in the window list and the switcher". It now states the Wayland
chain, which of its links belong to this process (one), and where the rest
happens.

The application installs **nothing** at startup. An application that writes
into a person's home directory because they ran it is doing something they did
not ask for, and the brief forbids it. `the_installer_is_safe_to_run_unprivileged`
asserts it against the *code* of `lib.rs`, `main.rs` and `build.rs` with
comments stripped — so the source can keep explaining the installer at length
without the test mistaking an explanation for a call — and also that none of
the three spawns a process at all.

### 35.3 No new artwork

The Flow A is unchanged: same file, same geometry, same gradient. No second
logo, no raster, no geometry fix — GNOME accepts the SVG directly, so no PNG
was needed and none was added.
`the_primary_application_icon_is_the_flow_a` asserts the app icon's path data
is *identical* to the canonical mark's, by geometry rather than by filename,
and that nothing — the resource list, `build.rs`, the installer — names the old
ribbon mark as the application icon.

---

## 36. Icon — real Fedora/GNOME evidence

Fedora 44, GNOME Shell 50.4, Wayland, GTK 4.22.4.

**The chain, re-asked after the install.** Both queries that returned nothing
before:

```console
1. desktop file found by app_id : True          (was: constructor returned NULL)
   Name                         : AnyFlow
   Icon= resolves to            : io.github.yurisismotto.anyflow
   Exec                         : anyflow-gui
   actions                      : ['quick-panel']
2. icon name in a plain theme   : True          (was: absent everywhere)
   resolved file                : /home/yuri/.local/share/icons/hicolor/
                                  scalable/apps/io.github.yurisismotto.anyflow.svg
```

Step 2 was asked from a **separate process with a fresh `GtkIconTheme` and no
access to AnyFlow's compiled-in resources** — deliberately, because that is
GNOME Shell's exact position. The name resolves to a real file on disk.

**The application half, on a clean restart** (§19's sequence). The old GUI was
terminated and the new build started as plain `anyflow-gui` from `PATH` —
i.e. exactly what the installed `Exec=anyflow-gui` runs:

| # | Step | Result |
|---|---|---|
| 1 | Old GUI terminated | ✅ 0 processes |
| 2 | Started via the installed launcher path | ✅ `/home/yuri/.local/bin/anyflow-gui`, pid 2238786 |
| 3 | Quick Panel opened | ✅ `xdg_toplevel#78.set_app_id("io.github.yurisismotto.anyflow")` |
| 4 | Settings opened | ✅ `xdg_toplevel#108.set_app_id("io.github.yurisismotto.anyflow")` |
| 5 | Both surfaces, one identity | ✅ **identical** app_id on both toplevels |
| 6 | Panel closed, daemon and process survive | ✅ gui pid unchanged, daemon pid unchanged |
| 7 | Panel reopened | ✅ `xdg_toplevel#80.set_app_id("io.github.yurisismotto.anyflow")`, still one process |
| 8 | Identity stable across close/reopen | ✅ all three toplevels, one string |

**X11 half:** `WM_CLASS = "anyflow-gui"`, matched by `StartupWMClass`, and a
correct 48 × 48 `_NET_WM_ICON` carrying the Flow A.

### What automated inspection could **not** observe

Everything above is measured. None of it is a *rendered pixel*, because this
environment denies every programmatic way to look at one:

```console
org.gnome.Shell.Screenshot.Screenshot              AccessDenied
org.gnome.Shell.Introspect.GetWindows              GetWindows is not allowed
org.gnome.Shell.Introspect.GetRunningApplications  GetRunningApplications is not allowed
org.gnome.Shell.Eval                               (false, '')     — unsafe-mode off
```

`import -window root` produces nothing either: this is Wayland and the
Xwayland root is not composited. Enabling GNOME's unsafe mode on the
developer's live session was not done.

So what automation established is precisely this and no more: **every link of
the resolution chain now resolves, each measured independently on the live
session** — the app_id the window declares, the desktop file found by exactly
that id, and the icon name resolving to a real file from the shell's own
vantage point. Whether GNOME then *draws* that file was outside what any tool
here could answer. It was answered by the operator instead; see below.

A repeatable, machine-checked visual regression pass still needs a session
where capture is permitted, which remains recorded as QP-DEBT-07.

### Operator-observed visual confirmation

**Evidence class: OPERATOR-OBSERVED.** This is a human observation reported by
the operator on their own Fedora/GNOME Wayland session. It is recorded
separately from everything above for that reason — it was not produced,
captured or checked by any tool in this environment, and it is not
machine-reproducible here.

The operator visually confirmed that **GNOME renders the AnyFlow Flow A as the
application icon for the running AnyFlow application**, in place of the generic
application glyph that was the defect.

* This **closes the visual gap** that automated inspection could not observe —
  the last link in the chain, from "the shell can resolve the icon file" to
  "the shell draws it", is now confirmed rather than inferred.
* Taken with the measured evidence above, the whole chain is accounted for:
  the application declares the app_id (measured), the session resolves that
  app_id to the desktop entry and its `Icon=` name to a real SVG (measured),
  and GNOME draws the Flow A from it (operator-observed).
* **No screenshot was captured.** None exists, in this report or elsewhere in
  the repository. The screenshot portal is still denied here (above), and
  nothing about this confirmation should be read as image evidence.
* The confirmation is recorded at the granularity the operator gave it — the
  application icon GNOME shows for the running application. Individual shell
  surfaces are not separately certified here, and the automated evidence above
  is unchanged by it.

---

## 37. Polish — development run behaviour

```console
# once, by hand, never by the application:
$ ./desktop/gui/tools/install-desktop-metadata.sh \
      --link-binary "$PWD/desktop/target/debug/anyflow-gui"

# then, as before:
$ ./desktop/target/debug/anyflow-gui --quick-panel     # works, icon resolves
$ anyflow-gui --quick-panel                            # via ~/.local/bin
```

* **No root**, nothing written outside `~/.local`, nothing in `/usr`.
* **Not at startup**, and not on every launch — one explicit command.
* **Reversible** — `--uninstall`.
* Rebuilding the binary needs no reinstall: `~/.local/bin/anyflow-gui` is a
  symlink to the build output.
* A window already open when the metadata is installed may keep the generic
  icon until it is reopened. The script says so.

---

## 38. Polish — tests and gates

`anyflow-gui`: **91 → 113**. Workspace: **838 → 864**. Display-gated
sections: **12 → 19**.

**Recent transfers — 16 model tests** (`panel::model::tests`, no display):

| Brief | Test |
|---|---|
| A | `zero_terminal_transfers_shows_no_recent_section` (empty, unanswered, and in-flight-only) |
| B | `a_completed_outgoing_transfer_reads_as_sent_to_the_peer` |
| C | `a_completed_incoming_transfer_reads_as_received_from_the_peer` |
| D | `a_declined_transfer_reads_as_declined` (both directions, and who refused) |
| E | `a_failed_transfer_reads_as_failed` (5 reasons; asserts the daemon's prose reaches nothing) |
| F | `a_timeout_and_a_dropped_connection_are_named_apart` (+ cancelled, + reasonless terminals) |
| G | `at_most_three_recent_transfers_are_shown` |
| H | `recent_transfers_are_newest_first` (handed over in scrambled order) |
| I | `an_in_flight_transfer_is_not_duplicated_into_the_recent_list` (every non-terminal state) |
| J | `a_display_name_collision_does_not_affect_transfer_identity` |
| K | `a_recent_transfer_does_not_change_the_selected_peer` |
| L | `view_all_targets_the_settings_transfers_page` |
| M | `a_recent_row_carries_no_content_hash_or_identifier` |

Plus `the_recent_list_is_only_the_current_daemon_run`,
`a_transfer_with_an_unrecognised_direction_is_not_shown`,
`every_outcome_is_a_calm_word_rather_than_an_error_code`.

**Display-gated, +7 sections:** no section when nothing finished · a row states
direction and outcome in words · three rows at most, newest first · `View all
transfers` is a button carrying `app.transfers` · a row is a label and not a
control · `app.transfers` opens the existing Settings window on its Transfers
page and not a second one · opening the transfer list does not change the
chosen device.

**Icon — 6 tests, extending `brand_assets.rs` rather than duplicating it**
(the existing eight already prove the artwork is a self-contained,
font-free, raster-free, legible-at-16 px vector):

| Brief | Test |
|---|---|
| A, C | `the_application_id_names_the_desktop_entry_and_its_icon` |
| B, I | `every_layer_asks_for_the_same_icon_name` — Rust, ini and shell agree |
| D | `the_desktop_entry_launches_this_binary` (+ `DBusActivatable`, + `StartupWMClass`) |
| E | `the_icon_is_compiled_in_at_an_icon_theme_path` |
| K | `the_primary_application_icon_is_the_flow_a` |
| — | `the_installer_is_safe_to_run_unprivileged` |
| J | `settings_and_the_panel_belong_to_one_application`, extended to assert one identity |
| F–H, L | already proven by the original eight |

The application id is **read out of `src/lib.rs`** rather than restated in the
test: a test that restated it would agree with itself while disagreeing with
the application, which is the failure being guarded against.

**Vocabulary — 4 tests below the GUI:** three in `runtime` pinning
`FailureReason`/`TransferState`/`Direction` to the control tokens
exhaustively in both directions, and `every_failure_reason_has_a_distinct_machine_code`
in the files capability.

```console
$ cargo fmt --all --check                            clean
$ cargo test -p anyflow-gui -p anyflow-control -j 2  113 passed · 0 failed
$ cargo test --workspace -j 2                        864 passed · 0 failed
$ cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings
                                                     exit 0 · zero warnings
$ cargo test -p anyflow-gui -- --ignored --test-threads=1
                                                     1 passed (19 sections)
$ desktop-file-validate …anyflow.desktop             clean, no warnings
$ install-desktop-metadata.sh --prefix <tmp> [--uninstall]
                                                     round-trip clean
```

Sequential throughout, `-j 2`. No Gradle, no Android build, no VM.

---

## 39. Polish — files changed, debts, verdict

### Files

**New (1):**

| Path | Lines | What |
|---|---|---|
| `desktop/gui/tools/install-desktop-metadata.sh` | 158 | Installs the desktop entry and icon into a prefix. The whole Wayland icon mechanism, and what packaging will reuse |

**Modified, by this polish (7):**

| Path | What |
|---|---|
| `desktop/capabilities/files/src/transfer.rs` | `FailureReason::code()`, `::ALL`, and a test that the tokens are distinct tokens |
| `desktop/capabilities/files/src/lib.rs` | `TransferSnapshot.seq`, from the record's existing counter |
| `desktop/control/src/lib.rs` | `TransferReport.seq` and `.failure_code`; the `transfer_state`/`transfer_failure`/`transfer_direction` token modules |
| `desktop/runtime/src/server.rs` | carries both fields through; three vocabulary-correspondence tests |
| `desktop/gui/src/panel/model.rs` | `Outcome`, `RecentTransfer`, `recent_transfers`, `outgoing`; `PanelModel::recent` |
| `desktop/gui/src/panel/mod.rs` | the section and its rows; 5 display-gated sections |
| `desktop/gui/src/lib.rs` | `ACTION_TRANSFERS`; corrected `install_icons` documentation; `selected_page`; 2 display-gated sections |
| `desktop/gui/data/…anyflow.desktop` | `StartupWMClass=anyflow-gui`, measured |
| `desktop/gui/src/views/notifications.rs` | test fixture: the new fields, and a direction the daemon actually sends |

`git diff --stat` for the branch as a whole: **15 files, 1 530 insertions,
311 deletions**, plus 8 new paths. `git diff --check`: clean.

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` — untouched, unstaged, still untracked.
`git add .` was never used. Nothing staged, committed or pushed; no PR.

### New debts

| # | Debt |
|---|---|
| QP-DEBT-08 | **Settings → Transfers is still unordered.** It renders in the order the daemon's map yields, which is the order of a random transfer id — observed live: `recent-fourth`, `recent-sent`, `AGENTS.md.txt`, `recent-accepted`. `seq` now exists on the report, so sorting it is a one-line change; it was left alone because reordering a certified page is outside this polish. |
| QP-DEBT-09 | **Settings badges a declined transfer "Disconnected".** Pre-existing: `views/files.rs` maps `cancelled` to `Status::Disconnected`, whose label is that word. The panel says `Declined` for the same transfer, so the two surfaces disagree. `failure_code` is what a fix would read. |
| QP-DEBT-10 | `views/files.rs` still branches on `"rejected"` and `"outgoing"` — neither is a value the daemon has ever sent. Dead arms, now that `transfer_state`/`transfer_direction` name the real set. |
| QP-DEBT-11 | **No file-manager action anywhere.** Deliberately not added to the panel per the brief; Settings shows `Saved to <path>` as text with nothing to open it. Recorded as future UX debt, as the brief directs. |
| QP-DEBT-12 | The chosen device is still a GUI preference (QP-DEBT-01 stands). `seq` and `failure_code` show the additive-control-field pattern a daemon-owned selection would use. |

QP-DEBT-07 (no screenshot capture in this environment) no longer blocks
*confirmation* of the application icon — the operator confirmed it visually
(§36) — but it still blocks a **machine-checked visual regression pass** for
it. Nothing in CI or in this environment can assert the drawn icon; the
automated tests assert the resolution chain, and the drawn result rests on an
operator observation that would have to be repeated by hand.

### Verdict

```
QUICK PANEL + BRANDING V1:     PASS   (unchanged, §31)
RECENT TRANSFERS:              PASS   (measured)
APPLICATION ICON INTEGRATION:  PASS   (measured chain + operator-observed render)
```

None of the recent-transfer failure conditions applies: at most three rows are
shown, measured live against a daemon holding four; nothing is persisted, and a
daemon restart empties the list; no content, hash, size, path or id reaches the
presentation model, asserted against every string it can produce; the list
cannot move the selection, verified live before and after opening it; it is
never consulted for routing; and direction comes from the transfer, with an
unrecognised direction drawn not at all rather than guessed.

None of the icon failure conditions applies, and the evidence for it is of two
distinct classes, kept apart deliberately:

* **Measured, automated** (§36): every link of the resolution chain resolves on
  the live session, each verified independently — the app_id the window
  declares, the desktop entry found by exactly that id, and its `Icon=` name
  resolving to a real SVG from a process with no access to AnyFlow's own
  resources. Both surfaces declare one identical app_id, measured on three
  toplevels. The installed files are byte-identical to the ones packaging will
  ship, so identity cannot diverge; the fix needs no root and nothing in
  `/usr`; and no second logo was introduced — the app icon's geometry is
  asserted identical to the canonical Flow A.
* **Operator-observed** (§36): the operator visually confirmed on the real
  Fedora/GNOME session that GNOME renders the Flow A as the application icon
  for the running AnyFlow application, rather than the generic glyph. This is
  the one step no tool here could observe, and it is what closes
  *"GNOME still displays a generic icon on a surface that should resolve the
  app"* as a failure condition. **No screenshot was captured**; this is a human
  observation, not image evidence, and it is not machine-reproducible in this
  environment (QP-DEBT-07).

---

*No commit, no push, no PR. Awaiting review.*
