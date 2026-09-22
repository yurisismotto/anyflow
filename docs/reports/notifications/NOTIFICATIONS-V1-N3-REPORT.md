# `notifications.v1` — N3: consent, privacy and user experience

**Wave:** N3 · **Branch:** `feature/notifications-v1-n3-consent-ui` ·
**Date:** 2026-09-09

**Baseline commit:** `4d33205`
(*Merge pull request #19 from yurisismotto/feature/notifications-v1-n2-linux-sink*)

A person can now turn Android → Linux notification mirroring on, choose which
applications it covers, and turn it off again, entirely from the two
applications' own screens. No JSON, no `adb`, no CLI. The three permissions
this feature actually has are shown as three things, because they fail as three
things.

> **This report is not final.** The tablet's USB dropped to MTP-only twice
> during the run and needs a physical replug that could not be done from here,
> so §23, §25, §26, §28 and the Android half of §30 are **partially executed**
> and are marked as such. Everything that does not need the device is complete
> and green. The verdict at the end reflects that honestly.

> **Reading order (added 2026-09-10).** This document is cumulative and nothing
> in it has been rewritten. §1–§37 are the original wave report; **FINAL
> CLOSEOUT** (F1–F14) fixed the accessibility blocker and left two gates
> unexecuted; **SECOND FINAL CLOSEOUT** (G1–G14) executes those two gates. The
> operative verdict is **G14**, which supersedes F14. Where a later section
> supersedes an earlier one it says so; the earlier finding is left standing as
> the record of what was true when it was measured.

---

## 1. Baseline and scope

| | |
| --- | --- |
| Base | `4d33205`, N2 merged and certified (**NOTIFICATIONS.V1 N2 PASS**) |
| Canonical design | ADR-0015, ADR-0016, ADR-0017, `docs/architecture/NOTIFICATIONS.md`, `docs/research/notifications-v1/**` (especially [01 §5, §7, §8](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md)), the three previous wave reports |
| Desktop under test | Fedora 44, GNOME Shell **50.4** (spec 1.2), Wayland, `anyflow-daemon` `DF65 D3E4 BA28 EDF9` |
| Phone under test | Samsung **SM-X620**, Android **16** / API 36, One UI **8.0** |
| Commits made | **none** — nothing committed, nothing pushed, no PR |

The brief names `develop` as the base. This branch is a descendant of
`develop`, and `develop` is where the previous notification waves merged — the
N2 PR (**#19**) was merged into `develop`, as were N0 (#17) and N1 (#18). The
baseline commit `4d33205` is unchanged and remains correct.

> **Correction (2026-09-10).** An earlier revision of this section said `main`
> was where the previous waves merged. That was wrong and is corrected above.
> Nothing else in this report depended on it.

Explicitly **not** in this wave, and verified absent below (§33): the dismissal
runtime, notification actions or reply, notification history, any change to the
approved protobuf contract, `QUERY_ALL_PACKAGES`, and `CompanionDeviceManager`.

---

## 2. Files changed

### New — Android

| File | Lines | What it owns |
| --- | ---: | --- |
| `ui/NotificationSettingsScreen.kt` | 539 | The per-computer consent screen: the three gates, the policy, and the diagnostics section |
| `ui/AppPickerScreen.kt` | 393 | The deny-by-default application picker |
| `ui/NotificationUiMapping.kt` | 84 | Readiness → badge, word and sentence. Pure, so the eight states are a test |
| `notifications/NotificationApps.kt` | 156 | Which applications the picker may offer, and in what order. No Android types |
| `notifications/InstalledApps.kt` | 103 | The platform half: `queryIntentActivities`, labels, icons. Every call a binder call |
| `notifications/NotificationReadiness.kt` | 153 | The three gates, and the eight states they produce |
| `res/drawable/ic_notifications.xml`, `ic_search.xml` | 2 files | 24 dp grid, 2 px stroke, matching the existing set |

### New — desktop

| File | Lines | What it owns |
| --- | ---: | --- |
| `gui/src/views/notifications.rs` | 874 | The Notifications page, its readiness model, and 13 tests |

### New — tests

| File | Lines | Tests |
| --- | ---: | ---: |
| `test/…/NotificationAppsTest.kt` | 234 | 13 |
| `test/…/NotificationReadinessTest.kt` | 167 | 13 |
| `test/…/NotificationPolicyStorageTest.kt` | 136 | 10 |
| `test/…/NotificationNavigationTest.kt` | 87 | 7 |
| `androidTest/…/NotificationConsentUiTest.kt` | 371 | 22 |
| `androidTest/…/AppPickerUiTest.kt` | 277 | 17 |
| `androidTest/…/NotificationLoggingCanaryTest.kt` | 437 | 8 |
| `androidTest/…/NotificationUiFixtures.kt` | 138 | — |

### Modified

| File | Change |
| --- | --- |
| `AndroidManifest.xml` | The `<queries>` element for `MAIN`/`LAUNCHER`, with the measurement that justifies it (§7) |
| `notifications/NotificationPolicy.kt` | `knownApps` — the picker's baseline for "N new apps", and a shared reader for stored package lists |
| `notifications/NotificationSource.kt` | `Status.peers` (per-peer role state), `sourceActive`, `activePackages()`, and `ListenerControl.activePackages()` |
| `notifications/AnyFlowNotificationListener.kt` | `activePackages()` on the service seam; `NotificationAccess.settingsIntent()` |
| `ui/MainState.kt` | Three state fields, five actions, and `notificationGates()` |
| `ui/MainActivity.kt` | The actions, the on-resume re-read of the OS permission, and the work-profile probe |
| `ui/Navigation.kt` | Two destinations, and `Screen::parent` — Back walks a declared hierarchy rather than a pushed stack |
| `ui/AnyFlowShell.kt` | The two destinations, and the top bar's back arrow from `parent` |
| `ui/PeerDetailScreen.kt` | The Notifications row on the device card — a state and a way in, deliberately not a switch (§5) |
| `res/values/strings.xml` | 77 strings and one plurals resource, with the two rules that bind the permission copy |
| `app/build.gradle.kts`, `gradle/libs.versions.toml` | Compose UI test dependencies |
| `gui/src/lib.rs`, `views/mod.rs`, `views/dashboard.rs` | The Notifications page, its refresh request, and the fourth capability chip |
| `runtime/src/server.rs` | `do_grant` and `do_notifications_policy` made public, so the suite drives the real handlers |
| `daemon/tests/common/mod.rs`, `daemon/tests/notifications.rs` | Three helpers and five new tests (§31) |
| `test/…/NotificationSourceTest.kt` | The fake listener implements the new seam method |

**`notifications_v1.proto` is unchanged** (§32), and no `.github/` workflow was
touched (§30).

---

## 3. Android UX architecture

The brief's first rule was to add this to the existing per-peer settings rather
than build a parallel section, and that is what happened. The hierarchy is:

```text
Devices  →  Fedora (device card)
              Permissions:  Clipboard · Files · Battery  [switches]
                            Notifications                [state + chevron]
                                 ↓
            Notifications (per computer)
                            Share notifications with this computer  [switch]
                            Notification access  →  Android Settings
                            Apps  →  N of M                         [chevron]
                                 ↓
            Choose apps (the picker)
```

Two decisions are worth defending.

**Notifications is not a switch on the device card.** The other three grants
are one decision each. This one is three — Android's own notification access,
this computer's grant, and which applications — and they are held by different
parties and lost independently. A switch on that card would have to claim one
of them stood for all three, which is the specific failure
[01 §7](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md) says most
products get wrong. So the row shows the **resolved state** — `Off`,
`Needs Android access`, `No apps chosen`, `Ready` — and leads to the screen
where the three are separate.

**Back walks a declared hierarchy.** `Screen::parent` replaced the shell's
"one level of back is all the hierarchy has" rule, which would have dropped
somebody out of the middle of the picker onto the device list. Each destination
names its parent, and the same property survives a rotation or a process death,
because it is a function of the destination rather than of a pushed history
(`NotificationNavigationTest`, 7 tests).

---

## 4. Notification access: the Android permission

A row of its own, with its own state, its own explanation, and one action.

```text
Notification access                                           [ Allowed ]
Allowing Android notification access lets AnyFlow read notifications on
this device. It does not share them with any computer on its own — each
computer has to be allowed separately, and then you choose which apps.
                                                   [ Open Android settings ]
Turn this off in Android settings at any time. Doing so stops every
computer at once.
```

* **AnyFlow never asks for it any other way.** There is no dialog here that
  grants it, no accessibility-service workaround, and no
  `CompanionDeviceManager` association — ADR-0015 §10 records why the last of
  those would be a notification-privacy decision rather than a convenience,
  because a live CDM association switches *off* Android's sensitive-content
  redaction.
* **`NotificationAccess.settingsIntent()`** uses
  `ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS` with
  `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME`, falling back to the
  whole-list action below API 30 (the floor here is 29) or when the detail
  action does not resolve.
* **The state is re-read from the platform, never remembered.** `onResume`
  re-reads it — the only thing this app learns by being resumed is that the
  person came back, *not* that they said yes — and it is re-read again on
  every `NotificationSource.Status` emission, so a revocation while the screen
  is open is picked up too. Both paths are covered by
  `revoking_android_access_while_the_screen_is_open_changes_what_it_says`.
* The button is offered whether or not access is granted, because it is also
  the way back to Android's screen to take it away.

### 4.1 One honest deviation on One UI 8.0

On the certification hardware the detail action lands on the notification-access
**list**, not on AnyFlow's own switch. This is not an AnyFlow bug: a
hand-built intent carrying the exact documented extra behaves identically.

```console
$ adb shell 'am start -a android.settings.NOTIFICATION_LISTENER_DETAIL_SETTINGS \
    --es android.provider.extra.NOTIFICATION_LISTENER_COMPONENT_NAME \
    "io.github.yurisismotto.anyflow/…AnyFlowNotificationListener"'
Starting: Intent { act=…NOTIFICATION_LISTENER_DETAIL_SETTINGS (has extras) }
→ com.android.settings.Settings$NotificationAccessDetailsActivity
→ renders the full app list
```

The person still lands one tap away, on a screen that names AnyFlow. The code
sends what the platform documents; One UI ignores it. Recorded as a debt
(§34.6) rather than worked around, because every workaround for this would
involve a non-public path.

---

## 5. The peer grant

```text
Share notifications with this computer                            [  OFF  ]
Off until you turn it on. Turning it on shares nothing by itself —
you choose which apps next.
```

* **Off on every paired computer**, because `notifications.v1` is withheld at
  pairing on both sides. Measured on the hardware below (§23.1).
* Turning it on writes the **canonical grant** —
  `TrustStore.setGrant(fingerprint, NotificationsCapability.ID, true)` — the
  same call the Clipboard, Files and Battery switches make. There is no second
  permission database, which is why the listener lifecycle, the filter and the
  role announcement all see it immediately.
* Turning it on then calls `NotificationSource.policyChanged()`, which
  re-evaluates the listener binding. Turning it **off** does the same, so the
  listener is released when the last eligible computer goes away rather than
  staying bound and reading.
* **Turning it on selects no application.** That is the single most important
  property of this screen and it is asserted from three directions: the UI test
  `turning_the_switch_on_writes_the_grant_and_selects_no_application`, the
  stored policy read back off the device (§23.2), and the picker's own
  `a_fresh_picker_has_nothing_ticked`.
* An ungranted computer is offered **nothing else**. The lock policy, the pause
  switch and the applications row would all be inert without the grant, and
  offering an inert control is how somebody comes to believe a thing is
  configured when it is not
  (`an_ungranted_computer_is_offered_no_setting_it_could_not_honour`).

---

## 6. The app picker

The largest gap N2 left — *"the only way to name an application is to edit the
phone's trust store"* — and the largest new surface this wave adds.

```text
Only the apps you tick are shared, and their notifications are shared
in full. Nothing is chosen for you.

[ 🔍 Search apps                                                      ]

No app chosen                             [ Select all ]  [ Clear all ]

  ▢  Alexa                com.amazon.dee.app
  ▢  Amazon Shopping      com.amazon.mShop.android.shopping
  ▣  Shell                com.android.shell        Notifying now
  …
```

| Requirement | How |
| --- | --- |
| Default selection empty | `NotificationPolicy.allowedApps` starts empty and nothing in the enable path writes it |
| Explicit per-app enable/disable | One checkbox per row, writing exactly that package |
| Deliberate "Select all" | Present, one tap away, and **confirmed with the number in the sentence** — it is the only control here that can share a hundred applications at once |
| Deliberate "Clear all" | Present, disabled when nothing is chosen |
| Searchable | Label **and** package name, because two applications can carry the same label |
| App label | Resolved locally, falling back to the package name rather than a blank row |
| App icon | Loaded locally, rasterised once off the main thread, never transmitted and never written to disk |
| Package name shown | Under every label, as disambiguation |
| Persisted in the existing policy | `TrustStore.setNotificationPolicy` — no new store |
| Own package never selectable | Removed in `NotificationApps.build`, and dropped again by `NotificationFilter.screen` before any filter runs |
| New apps stay disabled | They are simply not in the set; there is no rule to get wrong |
| Work profile still independent | The app list does not imply the work-profile switch, and vice versa |
| No category or content heuristic | There is none anywhere in the capability, by design (ADR-0015 §5) |

**Ordering is by label and does not change when a row is ticked.** "Allowed
first" was considered and rejected: the row a person is looking for would move
the moment they tapped it.

**A chosen application that later leaves the launcher stays visible.** The
allow-list is a third source for the list precisely so that a package which is
uninstalled, disabled or hidden cannot vanish from the picker while remaining
in the stored policy — otherwise somebody would be sharing something they could
no longer see or switch off.

### 6.1 "N new apps are not being shared"

[01 §5.2](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md) asks
for a passive affordance rather than a prompt, and this implements it with a
new persisted field, `NotificationPolicy.knownApps`: every package the picker
has *shown* this person, recorded when the list is actually displayed.

It is **not** an allow-list — `allowsApp()` does not read it, and
`the_baseline_is_not_an_allow_list` pins that. It is empty until the picker has
been opened once, which is what stops a fresh install announcing that all
ninety-seven of its applications are new. It is passive text and never a
notification: a notification about notifications is the one interruption this
feature must not create.

---

## 7. Package enumeration, and why it needs no `QUERY_ALL_PACKAGES`

The brief required this to be **verified on the reference hardware before the
mechanism was chosen**, and it was — with a throwaway instrumented probe run on
the SM-X620 before a line of the picker existed.

```text
queryIntentActivities(MAIN/LAUNCHER) = 97
getInstalledApplications(0)          = 398
pm list packages                     = 501
labels resolved                      = 97 of 97
icons resolved                       = 40 of 40
```

and, for every package observed posting a notification on that device that day:

```text
posting com.linkedin.android              launchable=true   visible=true  label=LinkedIn
posting com.facebook.katana               launchable=true   visible=true  label=Facebook
posting br.com.quintoandar.inquilinos     launchable=true   visible=true  label=QuintoAndar
posting com.instagram.android             launchable=true   visible=true  label=Instagram
posting com.mercadolibre                  launchable=true   visible=true  label=Mercado Livre
posting com.amazon.mShop.android.shopping launchable=true   visible=true  label=Amazon Shopping
posting com.google.android.googlequicksearchbox launchable=true visible=true label=Google
posting com.google.android.youtube        launchable=true   visible=true  label=YouTube
posting com.android.systemui              launchable=false  visible=true  label=Interface do sistema
posting com.sec.android.daemonapp         launchable=false  visible=true  label=Clima
posting com.android.shell                 launchable=false  visible=true  label=Shell
```

**The decision.** A `<queries>` element for `MAIN`/`LAUNCHER` — the documented,
unrestricted mechanism — makes visible exactly the applications a person sees
on their home screen. That is **97 of 501 packages**, a narrowing of better
than five to one against the restricted permission, and it covered every
application that was actually notifying. Google Play treats
`QUERY_ALL_PACKAGES` as restricted precisely because it is an inventory of
somebody's device; this is not a synonym for it.

**The second source costs nothing.** Packages with no launcher entry — the
system UI, a weather daemon, `com.android.shell` — are *not* made visible by
the element and are not enumerated. They reach the picker only while they are
actually in the notification shade, through the listener that is already bound
and already reading them. `ListenerControl.activePackages()` was added rather
than reusing `activeNotifications()` for exactly one reason: the picker needs
package names, and going through `PlatformNotification` would materialise every
title and body in this process only to throw them away.

The third source is the allow-list itself (§6).

**No `QUERY_ALL_PACKAGES` was added, and there was no point at which it was
needed.** The manifest's deliberately-absent list is unchanged, and the
`<queries>` element carries the measurement above as its justification.

---

## 8. App-filter defaults

| Setting | Default | Where it is enforced |
| --- | --- | --- |
| `allowedApps` | **empty** | `NotificationPolicy.allowsApp` returns false for anything not named |
| A newly installed app | **denied** | It is not in the set. No rule, so no rule to get wrong |
| AnyFlow's own package | **never** | `NotificationFilter.screen` first, `NotificationApps.build` second |
| `includeWorkProfile` | **false** | Independent of the app list |
| `includeOngoing` | **false** | Independent of the app list |
| `whenSourceLocked` | **`APP_ONLY`** | Reduction happens at the source, before encoding |
| `allowDismissSync` | **false** | Stored, inert, and not presented as a control |

Read back off the device immediately after the grant was made from the UI, with
nothing else touched (§23.2):

```json
{"allowMirror": true, "allowedApps": [], "knownApps": [],
 "includeWorkProfile": false, "includeOngoing": false,
 "whenSourceLocked": "APP_ONLY", "allowDismissSync": false}
```

---

## 9. Work profile

Default **off**, and a separate switch from the app list: a person who shares
"Slack" from their personal profile has not thereby asked to share work Slack.

The switch is offered only when the device actually has a second profile
(`UserManager.getUserProfiles().size > 1`, read once — a work profile is not
created while an app is in the foreground). On a device with none, the screen
says so rather than offering an inert control:

```text
Work profile notifications — This device has no work profile.
```

The certification tablet has exactly one profile
(`pm list users` → `UserInfo{0:Yuri C. S.:4c13}`), so **the switch itself could
not be demonstrated on hardware** and the absent-profile copy was what the gate
exercised. Both branches are covered by UI tests
(`a_device_with_no_work_profile_is_told_so_rather_than_offered_an_inert_switch`,
`a_device_with_a_work_profile_is_offered_the_switch_and_it_is_off`), and the
filter rule that work notifications never bypass the app list is pinned by
N1's `the_work_profile_switch_does_not_bypass_the_app_list`.

**What is not claimed:** AnyFlow does not detect that it has been installed
*into* a work profile — there is no public API for `isManagedProfile` without
`MANAGE_USERS`, and guessing from a user id would be a fingerprinting surface
the source already refuses to use. Recorded as a debt (§34.7).

---

## 10. Ongoing notifications

Default **off**, with copy that says what turning it on gets you and what it
does not:

```text
Ongoing notifications                                             [  OFF  ]
Music, navigation and "app is running" notices. Off, because they update
constantly and usually cannot be dismissed. The app still has to be chosen.
```

That last sentence is the requirement, not decoration: **no notification type
bypasses the app allow-list**, and the switch says so where somebody is
deciding.

---

## 11. Source lock policy

```text
When this device is locked
  ( ) Share everything          Titles and text are sent as usual.
  (•) Share the app name only   The computer is told which app notified, and
                                nothing else. Titles and text are removed on
                                this device, before anything is sent.
  ( ) Share nothing             Nothing at all is sent while this device is
                                locked.

Unlocking does not send what was held back. The next update from an app
arrives in full.
```

Default **Share the app name only**, verified on the device as the selected
option. The wording carries the property that makes this the strong form of the
control — *removed on this device, before anything is sent* — rather than
describing it as something the computer does with what it receives.

The last line is the answer to the question this design will otherwise generate
("why did my notifications not come back?"), placed where it is asked. There is
no unlock-restores-content setting and there will not be one: it cannot exist
without holding a title and a body in memory across the lock, which is the
notification history the design forbids.

A policy change writes only that field
(`choosing_a_lock_policy_writes_exactly_that_policy` asserts the app list and
the dismiss-sync flag are untouched) and takes effect on the next notification,
because the source re-reads the policy per notification. Nothing withheld is
replayed.

---

## 12. Desktop UX architecture

A new **Notifications** page in the existing GTK4/libadwaita window, between
Clipboard and Devices, built from `NotificationsStatusReport` — the report N2
already added. No parallel state, no second policy store, and no new control
request: everything on the page is something `anyflow notifications status`
already reports, and every action is one the CLI can already make.

```text
Notifications
┌ This computer ─────────────────────────────────────────────────┐
│ gnome-shell 50.4 (spec 1.2, GNOME)      freedesktop  ● Connected│
│ This computer is unlocked.                                      │
│ Lock state is read from this desktop session. If it cannot be   │
│ read, AnyFlow treats the session as locked.                     │
│ Showing 0 notifications now.                                    │
└─────────────────────────────────────────────────────────────────┘
Devices
┌ SM-X620   769E 7956 7E4C 2402                     ● Not connected┐
│ Set up and waiting. Notifications appear once the device connects│
│ Receive notifications from this device                  [  ON  ] │
│ Show notifications                                      [  ON  ] │
│ When this computer is locked                                     │
│   Show full content / Show app only / Do not show                │
│ Sync dismissals — Not available yet.                             │
└──────────────────────────────────────────────────────────────────┘
Nothing is kept — AnyFlow keeps no notification history…
```

**What is deliberately absent:** any list of notifications, past or present,
and any screen that says one is coming. There is nothing to build one out of
either — and that is asserted, over a real session that displayed a
notification with a distinctive title, by
`the_status_the_desktop_ui_reads_carries_no_notification_content`.

The dashboard's device card gains a fourth capability chip so that a person
looking at it can see every grant a device holds. Omitting the one that carries
their messages would have been the worst omission to make.

---

## 13. Desktop mirror consent

```text
Receive notifications from this device                            [  OFF  ]
Off until you allow it. Allowing it does not change any other permission,
and turning it off closes the notifications already on screen.
```

* Written through `Request::Grant` — the **same request** the Trusted peers
  page uses, so there is one grant and one place it lives.
* Turning it off closes the mirrors already on the screen, because
  `do_grant` calls `notify_notifications_revoked`. A revocation that left the
  last forty notifications up has not withdrawn anything a person can see.
  Driven through the real handler by
  `the_desktop_receive_switch_closes_the_mirrors_it_had_displayed`.
* It takes nothing else with it:
  `the_desktop_receive_switch_leaves_every_other_grant_alone` grants all four
  capabilities, withdraws this one, and asserts the other three and the pairing
  itself survive.
* An ungranted device is offered nothing below it, for the reason the Android
  side is (§5), and says so.

---

## 14. Desktop lock policy

```text
When this computer is locked
  ( ) Show full content   Titles and text appear on the lock screen as usual.
  (•) Show app only       The notification names the app and nothing else. The
                          text is removed before it reaches the notification
                          server, so there is nothing on the locked screen to
                          read.
  ( ) Do not show         Nothing at all is shown while this computer is
                          locked, and anything already on screen is closed.

Unlocking does not bring back text that was withheld: AnyFlow never kept it.
The next update from the app arrives in full.
```

The wire values `full` / `app-only` / `suppress` sit beside their words in one
table, so a label cannot drift from the value it sets — pinned by
`the_lock_choices_are_exactly_the_control_protocols_values`.

The page says lock state comes from "this desktop session"; **`logind` and
`LockedHint` appear nowhere in the normal UX** — which interface answers the
question is a diagnostic, not something to put in front of somebody deciding
what appears on their lock screen.

An **unrecognised** stored value selects nothing rather than falling back to
the first option, because silently selecting "Show full content" for a value
this build does not understand would be the most permissive possible guess
(`an_unrecognised_stored_policy_selects_nothing_rather_than_the_first_option`).

### 14.1 Why this is a `GtkListBox` and not three radio buttons

This is the wave's most surprising finding and it is recorded in full because
the next person will reach for the same API.

Three grouped `GtkCheckButton`s are the obvious way to write an exclusive
choice in GTK4, and that is what this page had first. **Calling
`gtk_check_button_set_group` anywhere on the page stopped the entire
application registering with the AT-SPI registry.** Every individual widget
still created its accessible context — `GTK_DEBUG=accessibility` shows them
being realised — so nothing looked wrong; the application simply never appeared
in `Atspi.get_desktop(0)`, which means **no screen reader could reach any part
of it**.

Bisected against the same build with the grouping removed, and cross-checked
against another GTK4 application (`loupe`) launched from the same shell in the
same instant, which registered every time:

| Build | Registers with AT-SPI |
| --- | --- |
| Pre-N3 `anyflow-gui` | yes |
| N3 with the grouped check buttons | **no** |
| N3 with the notifications page's render disabled | yes |
| N3 with the lock-policy widgets skipped | yes |
| N3 with a single-selection `GtkListBox` | **yes** — 269 accessible nodes |

A single-selection list box is what the window's own sidebar already uses. It
brings the selection state and keyboard navigation with it, and exclusivity
becomes a property of the *data*: the page is redrawn from the daemon on every
refresh, and the daemon holds exactly one policy.

---

## 15. The readiness model

Both platforms compute what is actually happening from the real gates, in one
pure function, and the screen renders it. Neither infers a gate from another.

**Android** — `NotificationGates` → `NotificationReadiness`:

| State | Means | Fix |
| --- | --- | --- |
| `SHARING_OFF` | no peer grant | the switch on this screen |
| `NEEDS_ANDROID_ACCESS` | granted, no OS access | Android Settings |
| `PAUSED` | granted, mirroring switched off | the pause switch |
| `NO_APPS` | granted, nothing chosen | the picker |
| `NOT_CONNECTED` | configured, no session | connect |
| `UNAVAILABLE` | connected, listener not bound or no secret | usually resolves itself |
| `PEER_NOT_RECEIVING` | connected, the computer claims no `SINK` | the computer's own grant |
| `READY` | all three gates open | — |

The order is the specification and it is in two blocks: **what the person can
fix**, then **what the two devices are doing**. `UNAVAILABLE` therefore only
fires while a session is up, which is the only state in which an unbound
listener is an anomaly rather than the designed idle behaviour — with no
granted peer connected the listener is deliberately released, and calling that
"Unavailable" would make the correct state look broken.

**Desktop** — `Readiness::of(peer, available)`: `Revoked`, `NoBackend`,
`NotGranted`, `Paused`, `NotConnected`, `PeerNotSourcing`, `Ready`. A missing
notification server is reported **once**, before anything per-device, because
dressing a machine-wide fault up as something about one device sends people to
check a grant that is fine.

**`PEER_NOT_RECEIVING` / `PeerNotSourcing` exist because N2 measured that
state.** Fedora had granted and announced `SINK`; the tablet had not granted on
its side and so announced `roles=0`; nothing moved. A screen with one switch
would have had to draw that as either "on" or "off", and both would be lies.
Both sides now name it, and both have a test that does
(`a_computer_that_has_not_claimed_it_can_display_is_named_as_such`,
`a_connected_device_that_claims_no_source_role_is_named_as_such`).

`only_the_ready_state_claims_to_be_mirroring` and
`only_ready_claims_to_be_showing_notifications` assert the property that
matters across every variant, and
`every_state_is_reachable_from_some_real_gate_combination` asserts that no
state is dead code and no two collapse into one.

---

## 16. Dismiss sync

**Not presented as a working control, on either platform.**

```text
Sync dismissals
Not available yet. Dismissing a notification on the computer will not
dismiss it here.
```

Rendered as inert text, not as a disabled switch — a switch, even greyed, is
still a thing somebody tries to turn on. `allow_dismiss_sync` stays stored and
`false`, nothing in this wave writes it, and
`dismiss_sync_stays_false_and_is_not_changed_by_anything_else` drives every
other setting and asserts it survives. `there_is_no_working_dismissal_control`
asserts no toggleable node carries those words, and
`there_is_no_control_for_dismissal_sync` asserts the desktop page has exactly
two switches, because a third would be a promise this release cannot keep.

---

## 17. Internationalization

**Android: all 77 new user-facing strings are in `res/values/strings.xml`**,
plus one `<plurals>` for "N new apps" — a plurals resource rather than two
strings picked by an `if`, because the one/other split is English's and a
translator must be able to supply their own categories without the Kotlin
changing.

This is a **change of practice for the Compose surface**, and it is deliberate.
The project had a string-resource mechanism and used it for every platform
surface (notification channels, the share target, the listener label) while
every Compose string was inlined — there were zero `stringResource` calls in
the app. [01 §10](../../../docs/research/notifications-v1/01-FUNCTIONAL-SPECIFICATION.md)
names `strings.xml` as the home for exactly this copy, and permission copy is a
security control that has to be reviewable and translatable as one. Existing
screens were left alone; nothing was regressed, and there is now a precedent.

**Desktop: inline literals, following the existing convention.** The GTK
application has no localization mechanism at all — no gettext, no textdomain,
no catalogs — so §17's condition ("if the project has a
localization/string-resource mechanism for that surface") is not met. Inventing
one for this page would have been a parallel architecture, which the brief
forbids. Recorded as a debt (§34.8).

**Notification content is never translated**, transliterated, normalised or
re-encoded, and `app_label` is resolved in the *source device's* locale — the
tablet's own labels came back as "Interface do sistema" and "Clima" in the
probe above, and that is what a desktop in English would display. No AnyFlow
string interpolates an app label into a translatable sentence in a way that
could let it change the grammar.

---

## 18. Accessibility

**Android.** Every row is one semantics node with one label and one state:
switches carry `contentDescription` from their title; the picker's rows merge
into one node with a `stateDescription` of "Shared" or "Not shared", because a
checkbox whose only signal is a tick is unreadable to anyone who cannot see it;
every interactive row is at least `MinTouchTarget` high; the icon-only search
clear button has a description; section labels are real headings.

One defect was found by reading the device's own accessibility tree during the
gate and fixed: the lock-policy rows announced **twice** — the row as selected
and the radio glyph beside it as unchecked. The glyph is decoration and now
carries `clearAndSetSemantics {}`.

**Desktop.** Every switch has an `accessible::Property::Label`, section labels
are `AccessibleRole::Heading`, the fingerprint is selectable and labelled, and
the status vocabulary is a dot, an icon **and a word** — nothing is
communicated by colour alone, which `every_state_has_a_word_and_a_sentence`
asserts across all seven readiness values.

And the AT-SPI registration defect of §14.1, which was the more serious of the
two: the application exposed correct labels on every widget while being
invisible to assistive technology as a whole.

**A third accessibility problem was found and is *not* fixed**, because it is
Wave-0 architecture rather than this wave's: the desktop pages re-render
wholesale on every control-socket reply — five rebuilds every two seconds as of
this wave. An AT-SPI client that finds a control and then activates it gets a
successful reply and no effect, because the widget it named was destroyed in
between; a screen-reader user loses their place for the same reason. Measured:
activations that never reach the daemon at `REFRESH_SECS = 2` land first time
at 45. Rendering once per cycle instead of once per reply was tried and did
**not** fix it, so the real fix is not rebuilding an unchanged tree — which is a
change to the page architecture the brief's scope rules put outside this wave.
Recorded as a debt (§34.1), with the measurement.

---

## 19. Responsive behaviour

**Android** uses the existing conventions unchanged: the shell caps the content
column at 640 dp and centres it, so the tablet does not render as a stretched
phone, and every new screen is a single scrolling column inside that cap. The
picker is a `LazyColumn`, so a hundred rows cost what ten do. Verified on the
SM-X620's 1600 × 2560 panel in portrait; nothing on the new screens has a fixed
width.

**Desktop** keeps the existing 700 sp breakpoint that collapses the sidebar,
and the Notifications page is built from the same `widgets::card` /
`widgets::row` primitives as every other page, all of which wrap rather than
overflow. Long device names and the fingerprint wrap; nothing sets a minimum
width that could force a horizontal scrollbar.

---

## 20. Android logging canary — NOTIF-SEC-25

N1 left this to N3 and N2 built the desktop twin. `NotificationLoggingCanaryTest`
is the Android half: **8 tests**, each driving a real flow through the real
`NotificationSource` with values no notification would ever contain, then
reading back **everything AnyFlow wrote to logcat** and asserting none of it
came out.

| Flow | Why it is there |
| --- | --- |
| Normal mirroring — post, update, remove | The happy path, and it asserts wire traffic was produced so it cannot pass vacuously |
| Filtered — an app that was never named | The drop is logged; the line must be a reason code and not the package |
| Denied peer — `NotificationPolicy.DENIED`, plus an inbound refusal | The refusal path carries an identifier |
| Lock-reduced — locked, `APP_ONLY` | The label reaches the wire and must not reach the log |
| Suppressed — locked, `SUPPRESS` | |
| A hostile notification — control characters, format specifiers, 4 KiB of padding, the title spliced into the platform key | A parser that quoted what it could not understand would fail here |
| **An error path whose exception message contains every canary** | Where a leak actually hides |
| A snapshot over a whole shade | The bulk path |

Each asserts the capture is **non-empty first**, so a suite that silently
captured nothing cannot report a clean bill of health.

It is an instrumented test, not a JVM one, for a reason worth stating:
`android.util.Log` is a stub on the unit-test classpath and writes nothing, so
a canary suite there would pass by writing to nothing at all. The log is read
through the instrumentation's shell, which holds `READ_LOGS` — **the
application does not, and must not**; AnyFlow declares no `READ_LOGS` and
reading its own log at runtime is not something it should be able to do.

> **Status: written, compiles, not yet executed on hardware** (§30). It is part
> of the connected suite that is blocked on the tablet.

---

## 21. Android UI tests

Deterministic Compose tests, not screenshots. **39 in two files**, driving each
screen as a pure function of `MainUiState` — no daemon, no session, no package
manager, no permission dialog.

`NotificationConsentUiTest` (22):

* the grant is **off** on a paired computer, and merely looking at the screen
  writes nothing;
* an ungranted computer is offered no setting it could not honour;
* turning the switch on writes the grant and **selects no application**;
* Android access is its own row with its own state; a missing one is named and
  never folded into the grant;
* the copy never claims that allowing access shares anything;
* the only route to the permission is Android's own screen, and pressing the
  button grants nothing here;
* a granted computer with no application chosen says so and is **not** "Ready";
* a fully set-up computer reads "Ready";
* a computer that has not claimed `SINK` is named as such;
* a disconnected computer is "Not connected" rather than "Off";
* **revoking Android access while the screen is open changes what it says**;
* the lock policy defaults to app-only, writes exactly what was chosen, and
  states that unlocking restores nothing;
* ongoing is off and says the app list still applies;
* work profile: both branches;
* there is no working dismissal control;
* the privacy copy contains none of `verification code`, `one-time`, `OTP`,
  `automatically hides`, `detects sensitive`;
* no screen lists a notification;
* protocol vocabulary is confined to the Details section.

`AppPickerUiTest` (17): nothing ticked on a fresh picker; opening it records a
baseline and selects nothing; ticking and unticking write exactly one package;
the stored selection is what the rows show; **"Select all" asks first, and
cancelling shares nothing**; "Clear all"; search by label and by package;
an empty search result says so; AnyFlow's own package is not offered **even
when the fixture hands the screen a row for it**; a notifying package is
marked; a withdrawn grant leaves nothing to choose and loads no list; an empty
inventory explains itself and disables both bulk actions.

Plus **43 JVM tests** on the pure logic — the readiness table (13), the
picker's rules (13), policy storage and defaults (10), and navigation and state
restoration (7).

Coverage against the brief's §23 list: every item is present. Three are covered
on the JVM rather than in Compose because that is where the rule lives — "own
package unselectable" and "new package default disabled" are properties of
`NotificationApps`/`NotificationPolicy`, and "process recreation/state
restoration" is a property of `ScreenSaver`.

> **Status: written, compile, not yet executed on hardware** (§30).

---

## 22. Desktop UI tests

**13 tests in `gui/src/views/notifications.rs`**, in two groups.

**Twelve pure tests** on the readiness model and the lock-policy table: every
gate open is Ready; a missing notification server is reported once and not
per-device; a revoked device outranks even a dead backend; ungranted reads
"Off" and not "broken"; mirroring off is "Paused" and not "Off"; a configured
device with no session is "Not connected"; a connected device claiming no
source role is named; **only Ready claims to be showing notifications**; every
state has a word *and* a sentence; the lock choices are exactly the control
protocol's values; the default is app-only.

**One widget-tree test**, `#[ignore]`d and asked for by name (the same
convention the notification capability's `real_dbus` gate uses), which builds
the real page from a fabricated `NotificationsStatusReport` and asserts on the
actual GTK widgets — nine sections in one test, because GTK records the thread
that initialised it and the harness gives each `#[test]` a thread of its own:

* an ungranted device gets exactly one switch, and no lock policy;
* the grant switch shows what the daemon holds, both ways;
* a granted device gets the pause and all three lock choices;
* the lock policy is **one** `ListBox` in `SelectionMode::Single` with the
  stored value selected;
* an unrecognised stored policy selects nothing;
* there is no third switch, and the dismissal row says "Not available yet";
* every switch is announced as a switch, beside words that name it;
* a desktop with no notification server says so once;
* the page renders no notification and says nothing is kept.

```console
$ cargo test -p anyflow-gui -- --ignored --test-threads=1
test views::notifications::tests::the_notifications_page_widget_tree ... ok
test result: ok. 1 passed; 0 failed
```

The brief also asked for "turning off closes mirrors" and "no notification
content in control/status models" as desktop UI tests. Both are asserted where
they are actually decided — against the real daemon handlers the GUI calls —
in §31.

One item on that list has **no test and should not have one**: "empty source
app selection status if that metadata is available". It is not available, and
it must not become available. Which applications a phone shares is the phone's
own configuration; sending it to the desktop would tell a computer the names of
applications whose notifications it is explicitly *not* allowed to see, which
is a small inventory of somebody's device travelling in the wrong direction for
no benefit. The desktop learns the consequence — the peer announces `SOURCE`
and no notifications arrive — and `PeerNotSourcing` is what it says about it.

---

## 23. Android hardware UX evidence

Executed on the SM-X620 by driving the **real UI** with `adb shell input tap`
against widgets located by `uiautomator dump`. Those are pointer events into
the app's own screens: no trust-store edit, no `pm grant`, no CLI.

### 23.1 A clean first run, and what pairing granted

The tablet arrived with no AnyFlow installed at all (Gradle's
`connectedAndroidTest` uninstalls both APKs when it finishes, which took the
trust store and the Keystore identity with it). That produced exactly the
starting state §25 asks for.

After pairing, read straight off the device:

```console
$ adb shell run-as io.github.yurisismotto.anyflow cat files/trust-store.json
  peer: Fedora DF65D3E4BA28EDF9
    granted: ['battery.v1', 'files.v1']
    notificationPolicy: {"allowMirror": true, "allowedApps": [], …}
```

and on the desktop:

```console
$ anyflow notifications status
    SM-X620 (769E 7956 7E4C 2402)
      notifications.v1 NOT granted
```

**Pairing granted `notifications.v1` on neither side.** Item 1 and the
deny-by-default baseline.

The device card then showed:

```text
Notifications                                                    ● Off
Show this device's notifications on the computer               →
```

### 23.2 The consent screen, and the three gates moving one at a time

| # | Requirement | Result |
| --: | --- | --- |
| 1 | Open paired Fedora device | **PASS** |
| 2 | Open Notifications | **PASS** — status `Off`, "This computer cannot see any notification from this device." |
| 3 | See Android notification-access status | **PASS** — `Details` showed access `Not allowed`, listener `Not bound`, grant `Not granted`, `No session` |
| 4 | UI action reaches Android's notification-access settings | **PASS** — landed on `com.android.settings.Settings$NotificationAccessDetailsActivity` (see §4.1 for the One UI deviation) |
| 5 | Enable AnyFlow there | **PASS** — Android's own consent dialog, with its own warning; `enabled_notification_listeners` gained the component |
| 6 | Return to AnyFlow | **PASS** — access re-read on resume: `Allowed`, status moved to `No apps chosen` |
| 7 | Explicitly allow notification sharing with Fedora | **PASS** — status moved `Off` → `Needs Android access` the instant the switch went on |
| 8 | The app list initially shares no new app automatically | **PASS** — see below |
| 9–18 | Choose an app, mirror, disable, re-enable, revoke | **NOT EXECUTED** — blocked on the USB drop |

Immediately after the grant, read off the device with nothing else touched:

```json
granted: ['battery.v1', 'files.v1', 'notifications.v1']
policy:  {"allowMirror": true, "allowedApps": [], "knownApps": [],
          "includeWorkProfile": false, "includeOngoing": false,
          "whenSourceLocked": "APP_ONLY", "allowDismissSync": false}
```

**The grant added exactly one capability and selected exactly zero
applications.** `clipboard.v1` — which pairing also withholds — was not added
either.

And the listener was **approved but not bound**:

```console
$ adb shell dumpsys notification | sed -n '/Live notification listeners/,/Snoozed/p' \
    | grep -c yurisismotto
0
```

`META_DATA_DEFAULT_AUTOBIND=false` honoured, with no eligible peer connected —
ADR-0015 §3, and the same evidence N2 recorded from the other end.

The picker opened with the deny-by-default copy, the search box, `No app
chosen`, `Select all` / `Clear all`, and rows carrying label, package name and
an **unticked** checkbox. **AnyFlow's own package was not among them.**

### 23.3 What is outstanding, and why

The tablet dropped to MTP-only mid-run for the second time. `usbreset` on
`/dev/bus/usb/003/038` re-enumerates it and it comes back MTP-only again; there
is no host-side recovery, and adb-over-Wi-Fi is not enabled on this device. The
AnyFlow Wi-Fi session is unaffected by this, but the app's own UI cannot be
driven without adb.

Outstanding: items 9–18 — choosing `com.android.shell` in the picker (it has no
launcher entry, so it appears only while it is notifying, which needs the
listener bound, which needs the session up), the mirror appearing on Fedora,
disabling and re-enabling that one app, revoking the peer grant, and confirming
the other three grants are untouched.

---

## 24. Desktop hardware UX evidence

Driven through the **accessibility tree** — every action below is one an
assistive technology can perform, which is a stronger claim than a pointer
click.

| # | Requirement | Result |
| --: | --- | --- |
| 1 | Open the AnyFlow GUI | **PASS** |
| 2 | Select the SM-X620 | **PASS** — all paired devices listed with their own state |
| 3 | Locate notification controls | **PASS** — the whole page read back through AT-SPI: 269 accessible nodes, 6 switches, 2 choice lists |
| 4 | Explicitly enable receiving notifications | **PASS** — activating the switch made the daemon report `notifications.v1 granted` |
| 5 | Ready only when the real prerequisites are satisfied | **PARTIAL** — every not-ready state observed (`Off`, `Not connected`, `Revoked`); `Ready` needs a live session |
| 6 | Change the destination lock policy | **PASS** — selecting "Show full content" wrote `when locked full`; selecting "Show app only" wrote it back |
| 7 | The control persists across daemon/GUI restart | **PASS** — see below |
| 8 | Disable notification receiving | **NOT EXECUTED** — see §18/§34.1 |
| 9 | Currently displayed mirrors close | **NOT EXECUTED** — needs a live session |
| 10 | Battery/files/clipboard untouched | **PASS** by test (§31); not re-observed in the GUI |

Persistence, gate 7 — the policy set from the GUI, on disk, and back in the GUI
after restarting **both** processes:

```console
$ cat ~/.local/share/anyflow/state.json | …
{"allow_mirror": true, "when_sink_locked": "full", "allow_dismiss_sync": false}

$ anyflow notifications status
    SM-X620 (769E 7956 7E4C 2402)
      notifications.v1 granted
      mirror on    when locked full   dismiss-sync off

# and the GUI, re-read through AT-SPI after both restarts:
[('Show full content', True), ('Show app only', False), ('Do not show', False)]
```

Gates 4 and 6 were performed with the refresh interval temporarily lengthened,
for the reason in §18: at the shipped two seconds the page is destroyed and
rebuilt underneath the activation. That is a real defect and it is reported as
one rather than hidden — but it means gates 4 and 6 demonstrate that *the
controls are wired correctly*, not that they are comfortable to operate with a
screen reader today.

---

## 25. Full user-driven end-to-end

**NOT EXECUTED.** It needs the tablet, and it is the gate that converts
`notifications.v1` from technically working to actually usable. The path is
staged and every prerequisite is in place: both ends paired, the Android grant
made from the UI, notification access granted from the UI, the desktop grant
made from the GUI, and the daemon running the N3 build at
`--log info,anyflow_capability_notifications=debug`.

What remains is: choose one app in the picker → post from it → watch it appear
on Fedora → update → remove → untick the app → confirm silence → retick →
confirm it returns → revoke on either side → confirm silence.

---

## 26. Logging audit

**Desktop: clean.** The daemon has run the whole session at
`anyflow_capability_notifications=debug` and every notification line it emitted
is a peer fingerprint, an event kind, an outcome name, a count or a platform
identifier — the vocabulary N2 established and this wave did not add to.

**Android: PENDING.** The canary suite (§20) is the audit and is blocked with
the rest of the connected suite. The hardware log sweep against real fixture
strings is blocked with it.

---

## 27. Persistence audit

**New persisted state, in full, this wave:**

| Where | Field | Contents |
| --- | --- | --- |
| Android `trust-store.json` | `peers[].notificationPolicy.knownApps` | Package names the picker has shown this person |

Nothing else. No new field on the desktop. The complete stored notification
policy, read off the device (§23.2), is seven settings and two lists of package
names — **and package names are settings the person chose**, which §28 of the
brief explicitly permits.

`the_stored_form_has_exactly_the_documented_fields` asserts the JSON object has
exactly those seven keys and no others, so adding a field that could hold
something else has to be a deliberate edit to that list.

The desktop half of the audit is unchanged from N2 and re-asserted by
`no_notification_content_is_written_to_disk`, which still passes.

**The hardware sweep for synthetic title/body/tag in the app's private files is
PENDING** with the rest of §25.

---

## 28. Android regression

```console
$ ./gradlew testDebugUnitTest
JVM: tests=447 failures=0 errors=0 skipped=0
```

Up from **404** at the N3 baseline: **+43**, all green, none skipped. Read from
the JUnit XML rather than from "BUILD SUCCESSFUL".

**`connectedDebugAndroidTest`: PENDING.** It is blocked on the tablet, and it
must be run last in any case because it uninstalls both APKs when it finishes.
The suite will need notification access granted before it runs, or N1's
`NotificationHardwareGateTest` will report `assumeTrue` skips — and the brief
requires zero.

---

## 29. Rust regression

Run from `desktop/`, with the documented GTK prefix sourced:

```console
$ cargo fmt --all --check                                        FMT OK
$ cargo build --workspace --locked -j 2                          Finished
$ cargo test --workspace --locked -j 2        590 passed; 0 failed; 18 ignored
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
                                                                 Finished, no warnings
```

Up from **574** at the N3 baseline: **+16** (13 on the desktop notifications
page, 5 new daemon tests, less the one page test that is `#[ignore]`d).

The 18 ignored are the platform gates that must be asked for by name: 9
clipboard `real_backend`, 6 `real_dbus`, 2 `real_lock`, and the new widget-tree
test. **All of the notification ones were executed:**

```console
$ cargo test -p anyflow-capability-notifications --test real_dbus -- --ignored
test result: ok. 6 passed; 0 failed

$ cargo test -p anyflow-capability-notifications --test real_lock -- --ignored
lock source: org.freedesktop.login1.Session.LockedHint on …/session/_32
test result: ok. 2 passed; 0 failed

$ cargo test -p anyflow-gui -- --ignored --test-threads=1
test result: ok. 1 passed; 0 failed
```

`anyflow-core`'s `notifications_protocol` suite is **47 tests, unchanged**;
`anyflow-proto`'s `notifications_schema` descriptor guard is **12, unchanged**.
No portable contract was touched.

---

## 30. Windows MSVC portable CI

**Untouched, and untouchable by this wave.**

```console
$ git diff --stat -- .github/
(empty)
```

N3 changed `anyflow-gui` (a platform application, deliberately not in the
portable set), `anyflow-runtime` (two functions made public, no dependency
change) and two daemon test files. `anyflow-capability-notifications`,
`anyflow-core`, `anyflow-control` and `anyflow-proto` are **unmodified**, so
the portable package set, the forbidden-feature list and the
test-classification guard all still describe exactly what they described at
`4d33205`. `core/tests/portable_boundary.rs` passes unchanged in the run above.

---

## 31. Existing-capability regression

Five new daemon tests, driving the **real control handlers the GUI calls**
rather than reimplementations of them:

| Test | What it pins |
| --- | --- |
| `the_desktop_receive_switch_closes_the_mirrors_it_had_displayed` | Turning the switch off takes what is on the screen off it |
| `the_desktop_receive_switch_leaves_every_other_grant_alone` | `battery.v1`, `files.v1`, `clipboard.v1` and the pairing all survive |
| `changing_a_notification_setting_never_touches_another_capability` | Four policy changes leave every other grant and the clipboard policy identical |
| `pausing_from_the_desktop_closes_mirrors_and_resuming_restores_nothing` | Resuming redisplays nothing this daemon should never have kept; the source's next update arrives in full |
| `the_status_the_desktop_ui_reads_carries_no_notification_content` | The report the page is built from carries no title, body, app id or label — over a session that displayed all four |

`daemon/tests/notifications.rs` is now **23 tests**, all green. `battery.v1`,
`files.v1` and `clipboard.v1` have no source change in this wave and their
suites are green.

**Live smoke tests (§31) are PENDING** with the hardware.

---

## 32. Protocol guard

```console
$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
(empty)
```

N3 is UI and policy. No schema change was needed and none was made.

---

## 33. N4 scope guard

```console
$ git diff -- android/ desktop/ | grep '^+' | grep -E \
    "DismissRequest|cancelNotification|PendingIntent|RemoteInput|addAction"
(no matches)
```

One keyword appears in the new code and it is not the protocol's:
`AppPickerScreen.kt:379` has `onDismissRequest = onDismiss` — Compose's
`AlertDialog` callback for tapping outside the "Select all" confirmation.
There is no `cancelNotification`, no `PendingIntent`, no `RemoteInput`, no
action execution and no reply anywhere in the wave, and Android still announces
`SOURCE` and only `SOURCE`.

---

## 34. Remaining risks and debts

1. **The desktop GUI cannot be reliably operated by assistive technology.**
   Its pages are destroyed and rebuilt five times every two seconds, so an
   AT-SPI activation lands on a widget that no longer exists; measured in §18.
   Wave-0 architecture, made 25% worse by this wave's fifth request. The fix is
   not rebuilding an unchanged tree, which is a page-architecture change beyond
   N3's scope. **This is the most serious thing in this list.**
2. **The Android hardware UX gate is half executed** (§23.3) and the
   user-driven end-to-end (§25), the Android log audit (§26), the device-side
   persistence sweep (§27) and `connectedDebugAndroidTest` (§28) are not
   executed at all. All are blocked on the same physical replug.
3. **`com.android.shell` is a poor test fixture for the picker.** It has no
   launcher entry, so it appears only while it is actually notifying, which
   needs the listener bound, which needs a granted peer connected. A tiny
   fixture app with a launcher entry would make this gate independent of that
   ordering, and N4 will want one.
4. **The USB flakiness is now the dominant cost of this hardware.** It cost
   this wave two full stalls. `adb tcpip 5555` while the cable is up would
   survive it, and enabling that should be the first step of N4's hardware
   session.
5. **`knownApps` is new persisted state.** It is package names, it is
   configuration, and the picker writes it only when it actually shows the
   list — but it is a second list of package names per peer, and a future wave
   should ask whether the "N new apps" affordance earns it.
6. **One UI ignores `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME`** (§4.1). The
   person lands one tap away rather than on AnyFlow's own switch. No workaround
   exists that does not use a non-public path.
7. **"Installed in a work profile" is not detected.** There is no public API
   without `MANAGE_USERS`, and inferring it from a user id would be exactly the
   fingerprinting surface the source refuses to use. A person in that situation
   sees a screen that looks correct and receives nothing.
8. **The desktop GUI has no localization mechanism at all**, so its strings are
   inline. Android's are all externalised now; the desktop's cannot be until
   somebody chooses a mechanism for it.
9. **Pairing cannot be driven from a host**, because the QR goes through the
   camera. This wave completed it by calling the app's own `AnyFlowApp.pair()`
   from a throwaway instrumented helper with a genuine, freshly minted payload —
   the real pairing path minus the camera. That helper must not ship.

**No P0 remains in the shipped code.** Debt 1 is a real accessibility defect in
a shipping UI and is the strongest argument for an early N5 item; debt 2 is
unfinished verification, not a known fault.

---

## 35. Git status

```console
$ git status --short
 M android/app/build.gradle.kts
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/…/notifications/AnyFlowNotificationListener.kt
 M android/app/src/main/java/…/notifications/NotificationPolicy.kt
 M android/app/src/main/java/…/notifications/NotificationSource.kt
 M android/app/src/main/java/…/ui/AnyFlowShell.kt
 M android/app/src/main/java/…/ui/MainActivity.kt
 M android/app/src/main/java/…/ui/MainState.kt
 M android/app/src/main/java/…/ui/Navigation.kt
 M android/app/src/main/java/…/ui/PeerDetailScreen.kt
 M android/app/src/main/res/values/strings.xml
 M android/app/src/test/java/…/NotificationSourceTest.kt
 M android/gradle/libs.versions.toml
 M desktop/daemon/tests/common/mod.rs
 M desktop/daemon/tests/notifications.rs
 M desktop/gui/src/lib.rs
 M desktop/gui/src/views/dashboard.rs
 M desktop/gui/src/views/mod.rs
 M desktop/runtime/src/server.rs
?? (18 new files: 6 Android sources, 2 drawables, 4 JVM tests,
    4 instrumented tests + 1 fixture, 1 desktop view)

$ git diff --check
(clean)
```

Scanned the changed and untracked set for `*.key`, `*.pem`, `*.p12`, `*.pfx`,
`*.jks`, `*.keystore`, `*.apk`, `*.aab`, `*.log`, `*.png`, `state.json`, trust
stores, captures, `target/` and `build/`: **no matches**. The tablet's trust
store, the UI dumps, the pairing QR and every log live in the session
scratchpad and are not in the tree.

**One file must be deleted before this is committed:**
`android/app/src/androidTest/java/…/PairingHelper.kt`, the throwaway pairing
helper of debt 9. It is a hardware-gate tool, not a test.

**Nothing was committed, nothing was pushed, no PR was created.**

### Machine state left behind

* **Fedora.** `anyflowd` is running from `desktop/target/debug/anyflowd` at
  `--log info,anyflow_capability_notifications=debug`, and `anyflow-gui` is
  open on the Notifications page — both the N3 build. The desktop session was
  unlocked with `loginctl unlock-session 2` for the GUI gate and is currently
  unlocked. `toolkit-accessibility` and the a11y bus's `IsEnabled` were both
  toggled while diagnosing §14.1 and both are back to `false`.
* **The tablet's peer `769E 7956 7E4C 2402`** is paired, holds
  `battery.v1` + `notifications.v1` on the desktop and
  `battery.v1` + `files.v1` + `notifications.v1` on the phone, with
  `when_locked = app-only` — the default, restored after the persistence gate
  set it to `full` and back.
* **The tablet** has the N3 debug build and the instrumentation APK installed,
  notification access granted, an empty application allow-list, and no session
  running. Its USB is in MTP-only mode and needs a physical replug before the
  outstanding gates can run.
* Five older `SM-X620` records remain in the desktop's trust store from
  previous waves, two of them revoked. They are untouched.
* The pairing QR (`~/anyflow-pair-qr.png`), the tablet's UI dumps, the daemon
  log and every capture live in the session scratchpad and are **not** in the
  tree.

---

## 36. Recommended commit message

```
feat(android,desktop): the notifications.v1 consent surface

A person can now turn notification mirroring on, choose which apps it
covers, and turn it off again, from the two applications' own screens.
No JSON, no adb, no CLI.

The three permissions this feature has are shown as three things,
because they fail as three things: Android's own notification access,
this computer's grant, and what each side says it can do. A single
switch would have to claim one of them stood for all three, which is
the state N2 measured on hardware and could not have described.
NotificationReadiness resolves the eight states that produces and the
screens render it; the ordering is "what the person can fix" first,
then "what the two devices are doing".

The app picker is deny-by-default and nothing is chosen for you.
Select all exists, one tap away, and asks first with the number in the
sentence. It needs no QUERY_ALL_PACKAGES: a <queries> element for
MAIN/LAUNCHER sees 97 of the 501 packages on the certification tablet
— every application observed notifying on it — and the shade supplies
the handful that have no launcher entry, through the listener that is
already reading them.

Every new Android string is in strings.xml. That is a change of
practice for the Compose surface, and permission copy is the right
place to start: it is a security control and has to be reviewable and
translatable as one.

The desktop's lock policy is a single-selection ListBox rather than
three grouped check buttons, and the reason is worth knowing: calling
gtk_check_button_set_group anywhere on the page stopped the whole
application registering with the AT-SPI registry, while every
individual widget kept exposing its label — an application no screen
reader could reach, that looked entirely correct.

Dismiss sync is inert text on both platforms rather than a disabled
switch, because a switch is still a thing somebody tries to turn on.
notifications_v1.proto is unchanged and no dismissal runtime exists.

+43 Android JVM tests (447 green), +39 instrumented UI tests, 8 for
the NOTIF-SEC-25 logging canary, +16 Rust (590 green), and a GTK
widget-tree test for the page itself.
```

---

## 37. Recommendation for N4

1. **Fix the desktop re-render before adding anything to that page.** N4 adds
   dismissal state to the same screen. Building on a page that destroys itself
   five times every two seconds means building on something a screen-reader
   user cannot operate, and the dismissal switch is the first control on it
   that *does* something to the phone.
2. **`allow_dismiss_sync` is stored, false, and presented as inert text on both
   platforms.** Turning it into a real switch is a one-line change in each UI
   and should be the *last* thing N4 does, not the first — the copy currently
   promises nothing, and it must not start promising before the runtime is
   there.
3. **Only reason 2 may become a `DismissRequest`.** N2 built
   `CloseReason::is_human_dismissal()` for this and N3 did not touch it. The
   UI's contribution is that the switch must say what it does to the phone, in
   the same voice the lock policy uses.
4. **Enable `adb tcpip` at the start of the hardware session.** Two of this
   wave's three stalls were the USB dropping to MTP with no host-side recovery.
5. **Build the fixture app** (debt 3). A one-activity APK with a launcher entry
   that posts on demand would make every picker and mirroring gate independent
   of whether `com.android.shell` happens to be in the shade.
6. **The canary pattern generalises.** `NotificationLoggingCanaryTest` reads
   the log through the instrumentation's shell rather than giving the app
   `READ_LOGS`, and asserts the capture is non-empty before asserting what is
   not in it. N4's dismissal paths — which carry an identity from one device to
   another — should extend it rather than start over.

---

NOTIFICATIONS.V1 N3 FAIL
N4 NOT READY

*Not because anything is known to be broken. Every line of the consent surface
is written, every unit and integration suite is green — 447 Android JVM tests,
590 Rust, 39 instrumented UI tests and 8 logging canaries — the app picker
needs no restricted permission and was measured on the reference hardware
before it was designed, `notifications_v1.proto` is untouched, and no dismissal
runtime exists.*

*It fails because §36's gate requires a connected Android suite green with zero
skips and a user-driven SM-X620 → Fedora flow, and neither has been executed:
the tablet dropped to MTP-only twice and needs a physical replug that cannot be
done from this machine. Nine of the eighteen Android gate items and four of the
ten desktop ones are outstanding. Reporting those as passed would be the one
thing this wave is least entitled to do — its whole subject is not claiming a
permission is in place when it is not.*

*One genuine defect was found and fixed on the way: the desktop application
exposed correct accessible labels on every widget while being invisible to
assistive technology as a whole, because of one call to
`gtk_check_button_set_group`. One more was found and left, with the measurement
that proves it, because fixing it is outside this wave's scope.*

---
---

# FINAL CLOSEOUT — 2026-09-10

Everything above is the **first** closeout attempt and is preserved unchanged
except for the `main`/`develop` correction in §1. That attempt ended
**NOTIFICATIONS.V1 N3 FAIL** for three reasons, all of which are still visible
in it and none of which have been edited away:

* the USB→MTP transition that cut the tablet off mid-wave (§23.3, §35);
* the Android hardware gate that therefore ran only to item 8 (§23.3);
* the desktop re-render / AT-SPI defect discovered while running §24, recorded
  as a debt rather than fixed (§18, §34.1).

This section records what the second pass did. It ends in the same verdict, for
a different and much smaller reason.

---

## F1. The accessibility blocker — root cause

The defect §18 measured was real and the diagnosis in it was incomplete. Two
independent causes compounded:

**Cause 1 — one poll drew five times.** `build_window` issued the refresh
cycle's five control-socket requests and gave *each* of them
`pages.render()` as its completion callback (`desktop/gui/src/lib.rs`). Five
replies, five full redraws, every `REFRESH_SECS = 2`. N3 made this 25% worse by
adding `NotificationsStatus` as the fifth request.

**Cause 2 — every draw destroyed every page.** `Pages::render` called
`widgets::clear` (which unparents every child) and rebuilt *all seven pages*
from scratch, whether or not their data had changed, and whether or not they
were the visible page. This is Wave-0 architecture and `views/mod.rs` documented
it as a deliberate trade: "far cheaper in bugs than a diffing layer".

Together: the `GtkSwitch` a screen reader had located was destroyed and replaced
by a new object up to five times per two seconds. AT-SPI locates and activates
in two separate round trips, so the activation was delivered to an object that
no longer existed — `do_action` returned `True` and nothing happened.

§18 reported that "rendering once per cycle was tried and did **not** fix it".
That is correct and it is why cause 2 matters: fixing only cause 1 leaves a full
teardown every 2 seconds, which is still far faster than a person can act.

---

## F2. The fix

Two changes, both small, both architecture-correct, neither of them a timing
change and neither of them special-cased for accessibility.

**1. One poll → one coherent state update → at most one draw.**
`desktop/gui/src/lib.rs` gained a `Cycle`: the five requests fold their replies
into a pending `DaemonState` seeded from the current one, and the last reply to
arrive commits it and draws **once**. A `generation` counter discards replies
from a superseded poll, so a slow answer cannot commit stale state over a newer
one and a daemon that stops answering cannot wedge the loop.

**2. A page is rebuilt only when its own data changed.**
`Pages::render` now compares the slice of `DaemonState` each page actually reads
against what that page was last drawn from (`Drawn`), and rebuilds only the
pages that differ. Equality is exact rather than hashed: `desktop/control` gained
`PartialEq, Eq` on the ten report types, so two states are equal exactly when
the daemon said the same thing twice.

This gives the property the brief asked for:

```
unchanged data → the widget tree is not touched at all
changed data   → only the pages that read the changed slice are rebuilt
```

**It is not a notifications fix.** Every page gets it. The pages that carry
controls — Notifications, Trusted peers, Clipboard — are where it matters most,
and Notifications benefits completely because `NotificationsStatusReport` has no
elapsed-time field: nothing in it ticks, so a quiet peer produces a byte-identical
report and an untouched page. Devices and Dashboard do show elapsed time
(`silent_secs`, `battery.age_secs`), so they still rebuild as those tick — that
is *changed data*, it is honest, and neither page carries an interactive control.

**A third change fell out of it, and is a real bug fix.** Nine call sites in the
views called `pages.render()` after the daemon answered a Grant or a policy
change, with the comment "Re-read rather than assume: the daemon is the
authority". They did not re-read — they redrew the state from *before* the
change. They now call `Pages::refresh_now()`, which actually asks the daemon
again. The comment was already right; the code now matches it.

`REFRESH_SECS` is **unchanged at 2**. No sleeps were added, no refresh was
disabled, no state is cached indefinitely — an unchanged poll still runs, it
simply has nothing to redraw.

Diffstat for the fix (within the wider N3 diff):

```
desktop/control/src/lib.rs         |  10 +-   (PartialEq/Eq on the report types)
desktop/gui/src/lib.rs             | 178 +++---
desktop/gui/src/views/mod.rs       | 185 ++++++--
desktop/gui/src/views/notifications.rs, clipboard.rs, peers.rs, pairing.rs
                                   |  render() -> refresh_now() at 9 call sites
```

---

## F3. The regression test

Five new assertions in `desktop/gui/src/views/notifications.rs`, inside the
existing `the_notifications_page_widget_tree` group (GTK belongs to the thread
that initialised it, and each `#[test]` gets its own — which is why that group
exists). They drive `Pages::render`, not `render`, because what they are about
is *when* a page is rebuilt.

| Test | What it pins |
| --- | --- |
| `an_unchanged_refresh_leaves_the_control_it_found_alone` | after 10 refreshes carrying a **newly constructed, equal** report each time, `switches()` returns the *same* `GtkSwitch` objects, the choice list is the same object, every switch is still parented and visible, and the stored policy is still selected |
| `an_activation_after_many_refreshes_still_reaches_the_handler` | a probe attached to the switch *before* the refreshes fires exactly once when it is activated afterwards, and that object is still the one on the page |
| `the_switch_is_still_activatable_from_the_keyboard_after_refreshes` | with the page mapped in a real window: still focusable, sensitive, takes focus, and `activate()` (what Space/Enter runs) actually flips it |
| `a_real_change_still_redraws_the_page` | a policy change moves the selection, and losing the grant removes the pause switch — not rebuilding an unchanged tree must not become not rebuilding at all |
| `a_change_on_another_page_does_not_disturb_this_one` | a transfer appearing and clearing on the Files page leaves the Notifications switches identical — the fix is per page, not per application |

**The test was verified to fail on the old behaviour.** Forcing the pre-fix path
(`let first = true`, i.e. always redraw) produces:

```
assertion `left == right` failed: an unchanged refresh destroyed and rebuilt the switch
  left:  Switch { inner: TypedObjectRef { inner: 0x7f90840653f0, type: GtkSwitch } }
  right: Switch { inner: TypedObjectRef { inner: 0x7f908405b0a0, type: GtkSwitch } }
```

Two different `GtkSwitch` pointers — the defect, caught. Restored, the group
passes. The grouped `GtkCheckButton`/AT-SPI-registration assertions of §14.1 are
untouched and still pass; nothing was removed.

One deliberate detail: the keyboard test disables `gtk-enable-animations`.
`GtkSwitch` toggles at the *end* of an animation, and an animation needs frame
callbacks a compositor is free not to send to a window it is not showing. With
animations off the toggle is synchronous, which makes the assertion about the
widget rather than about Mutter's scheduling.

---

## F4. Desktop hardware UX gate, re-run at the shipped cadence

`REFRESH_SECS = 2`, unchanged. Driven entirely through the **accessibility
tree** (`gi.repository.Atspi`) against the real Fedora GNOME session — 295
accessible nodes on the Notifications page.

| # | Requirement | Result |
| --: | --- | --- |
| 1 | Open the AnyFlow GUI | **PASS** |
| 2 | Select the SM-X620 | **PASS** — the `769E 7956 7E4C 2402` card located by fingerprint |
| 3 | Open Notifications | **PASS** — via the sidebar's `Selection.select_child` |
| 4 | Locate *Receive notifications* | **PASS** |
| 5 | Activate it through AT-SPI | **PASS** — first attempt |
| 6 | Daemon state changes | **PASS** — grant flipped, observed in **0.21 s**; the GUI reflected it in **0.16 s** |
| 7 | Change lock policy through AT-SPI | **PASS** — `Selection.select_child` on the choice list |
| 8 | Daemon state changes | **PASS** — `when-locked` written in **0.01 s** |
| 9 | Wait through multiple normal refresh cycles | **PASS** — see below |
| 10 | Activate the controls again | **PASS** — first attempt, **0.22 s** |
| 11 | Restart the GUI, verify persisted state | **PASS** — receive on, "Show full content" still selected |
| 12 | Disable receive notifications through the GUI | **PASS** |
| 13 | Existing mirrors close | **PASS** — one live mirror closed in **0.32 s** |

**Gate 9, the measurement that matters.** An AT-SPI reference to the *Receive
notifications* switch was taken, then held across a 30.1-second soak — **15
refresh cycles at `REFRESH_SECS = 2`** — sampling every 2 s:

```
sampled 14 times over 30.1s: same object 14/14, defunct 0
held switch still readable: name='Receive notifications from this device' checked=True
activate the held switch after 14 cycles -> True, daemon True -> False in 0.22s
```

**The identity check is not vacuous**, and this is the control that proves it. A
real change made *outside* the GUI (`anyflow notifications when-locked … suppress`)
gave the opposite result:

```
external change appeared in the GUI in 1.20s -> selected ['Do not show']
after a real change: same switch object = False | old defunct = True
restored in 1.52s
```

So: unchanged data → the same AT-SPI object, 14/14, none defunct. Changed data →
a new object, the old one defunct, on screen inside **1.2 s** at the shipped
cadence. That is exactly the target property of the brief's §2, measured on
hardware, with no lengthened interval anywhere.

For contrast, §24 of the first closeout had to lengthen the interval to **45
seconds** to land the same activations. It is now **2**.

Gate 13's evidence, from the daemon:

```
INFO anyflow_capability_notifications: closed every mirror for a peer
     peer=769E 7956 7E4C 2402 closed=1 reason="revoked"
```

---

## F5. Android connection stability

`adb devices` at the start showed USB live and the Wi-Fi endpoint `offline`. The
Wi-Fi endpoint was re-armed **before** any gate ran, exactly as the brief asked,
and it earned its keep: the USB interface dropped to a `sec_acm` configuration
(PTP + CDC-ACM, **no adb interface**) twice during the wave.

| Transport | Used for |
| --- | --- |
| **USB** (`RX2Y500C7SY`) | the Gradle runs (`ANDROID_SERIAL`), and the gates before the first drop |
| **Wi-Fi adb** (`192.168.68.63:5555`) | the whole of the items 12–18 recovery after the first USB drop |
| **both** | re-armed after every recovery |

`adb tcpip` does not survive a reboot and is cleared when the USB config flips,
so the fallback has to be re-established after each event rather than set once.
**No AnyFlow transport was changed because of adb** — the AnyFlow session ran
over the same TLS link throughout, and was observed alive across a USB drop.

---

## F6. Android hardware UX items 9–18

Run on the **cold-booted** device (see F9 for why it was rebooted), driven
through the real AnyFlow UI with `uiautomator dump` + `input tap`. adb was used
only to post synthetic notifications and to read evidence; **no trust-store
edit, no adb policy edit, no CLI policy command** touched any of it.

The source app is `com.android.shell`, posting via `cmd notification post`.
It is the safe synthetic source, and §7's own measurement is why it is
legitimate: it has no launcher entry, so it is **not** enumerated by the
`<queries>` element, and it reaches the picker only through
`ListenerControl.activePackages()` while it is actually in the shade. That is
observable in the run — after the reboot cleared the shade, the picker answered
"No app matches that search" until a notification was posted, at which point the
row appeared marked **"Notifying now"**.

| # | Requirement | Result |
| --: | --- | --- |
| 9 | Explicitly choose one safe synthetic/test source app | **PASS** — "Clear all", then `com.android.shell` ticked; the page then read **"1 of 98 apps"** |
| 10 | Post a notification | **PASS** |
| 11 | It appears on Fedora | **PASS** — `notification upsert … outcome="displayed"`, `showing 1 of 1` |
| 12 | Disable that app in the picker | **PASS** |
| 13 | The next notification does **not** appear | **PASS** — no upsert |
| 14 | Re-enable it | **PASS** |
| 15 | The next notification appears | **PASS** — `outcome="displayed"` |
| 16 | Revoke notification sharing for Fedora from the Android UI | **PASS** — and it closed the open mirror: `closed every mirror for a peer … closed=1 reason="peer is no longer a source"` |
| 17 | A further notification does **not** appear | **PASS** — `showing 0 of 0` |
| 18 | battery/files/clipboard grants untouched | **PASS** — see below |

Item 18, both sides, against the baseline captured before the wave:

```
$ diff grants-before.txt grants-after.txt
IDENTICAL — no capability grant changed
```

```
769E 7956 7E4C 2402 ->  granted   battery.v1, notifications.v1     (desktop)
Android-side chips for Fedora: Clipboard, not allowed | Files, allowed | Battery, allowed
```

Every other paired record's grants are byte-identical too, including the five
older `SM-X620` records and the two revoked ones.

**A UI observation, not a defect.** In the app picker the row itself reports
`clickable=false`; the tick is on the `CheckBox` node. Everything is reachable
and correctly labelled — the merged row carries the app's name and the checkbox
carries its state — but the whole row is not the touch target. Worth a look in a
later wave; it did not block anything here.

---

## F7. Full user-driven end-to-end — **PASS, 14/14**

The complete N3 flow, with **no JSON edit, no CLI policy command and no adb
policy edit** anywhere in it. Every policy change below was made by tapping the
real Android UI or by activating a control in the real desktop GUI through
AT-SPI.

| # | Step | Result |
| --: | --- | --- |
| 1 | paired SM-X620 → Android Notifications settings | **PASS** |
| 2 | Android OS notification access granted | **PASS** |
| 3 | peer notification grant turned on from the Android UI | **PASS** |
| 4 | exactly one app selected | **PASS** — "1 of 98 apps" |
| 5 | desktop receive enabled through the GUI | **PASS** |
| 6 | a notification appears on Fedora | **PASS** — `showing 1 of 1` |
| 7 | an update **replaces** it in place | **PASS** — same derived id, still `1 of 1`, no duplicate |
| 8 | removing it on Android **closes** the mirror | **PASS** — dismissed by swiping it out of the shade; `notification removal … outcome="removed"`, `showing 0 of 0` |
| 9 | app unchecked | **PASS** |
| 10 | the next notification stays on Android | **PASS** — `showing 0 of 0` |
| 11 | app re-enabled | **PASS** |
| 12 | the next notification mirrors | **PASS** — `showing 1 of 1` |
| 13 | peer sharing revoked from the Android UI | **PASS** |
| 14 | no further notification mirrors | **PASS** — `showing 0 of 0` |

This is the gate the first closeout could not run at all (§25, "NOT EXECUTED").
It now passes end to end, from a cold-booted device, operated the way a person
would operate it.

One structural detail worth recording, because it is the design working rather
than a workaround: at step 9, unticking `com.android.shell` while it had nothing
in the shade made it disappear from the picker entirely. That is correct — a
package with no launcher entry is enumerable only while it is notifying or
already on the allow-list — and "absent from the list" is itself the
not-shared state.

---

## F8. Android logging canary — NOTIF-SEC-25 — **PASS**

`NotificationLoggingCanaryTest` on the real SM-X620. **All eight tests
executed**; the JUnit XML was parsed rather than trusting `BUILD SUCCESSFUL`:

```
tests=8 failures=0 errors=0 skipped=0
```

```
ok NotificationLoggingCanaryTest.a_normal_mirroring_flow_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_filtered_notification_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_lock_reduced_flow_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_denied_peer_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_suppressed_flow_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_hostile_notification_writes_no_notification_content
ok NotificationLoggingCanaryTest.a_snapshot_over_the_whole_shade_writes_no_notification_content
ok NotificationLoggingCanaryTest.an_error_path_carrying_the_canaries_does_not_render_them
```

Independently of the tests, the **desktop** daemon ran the whole wave at
`--log info,anyflow_capability_notifications=debug` and every notification line
it wrote is of this shape — a derived id prefix and an outcome enum, no title,
no body, no package:

```
DEBUG notification upsert  peer=769E 7956 7E4C 2402 notification=462f95ae outcome="displayed"
DEBUG notification removal peer=769E 7956 7E4C 2402 notification=fef518de outcome="removed"
INFO  closed every mirror for a peer peer=769E 7956 7E4C 2402 closed=1 reason="revoked"
```

Searching the daemon log for the fixture's `7712`, `zebra`, `quokka`,
`AFN3CANARY7712` and the canary titles returns **zero matches**.

> **Attribution.** The Android-side `logcat` sweep is **not** claimed here.
> Gradle's connected run uninstalled the app (F9) and rotated the buffer before
> a clean capture of AnyFlow's own pid could be taken, so the only Android
> logging evidence in this closeout is the eight canary tests, which are
> themselves on-device and which assert exactly this property. SystemUI/OEM
> logging was never conflated with AnyFlow's: the attribution step (filtering
> `logcat` by AnyFlow's pid) is precisely the step that did not complete.

---

## F9. Device persistence audit — **NOT EXECUTED**

This is the gate that fails the wave, and the cause is procedural rather than a
product defect.

`:app:connectedDebugAndroidTest` **uninstalls the app and test packages when it
finishes**. The brief warned about this and told me to run the full connected
suite last. I ran the NOTIF-SEC-25 canaries through
`connectedDebugAndroidTest` with a `-Pandroid.testInstrumentationRunnerArguments.class`
filter — which is the same task, and it uninstalled just the same:

```
$ adb shell pm list packages | grep -i yurisismotto
(nothing — 501 packages, none matching)
```

That removed `/data/data/io.github.yurisismotto.anyflow` and with it **both** the
tablet's trust store and the on-device files the E2E had just produced. The
persistence sweep is defined as "after the hardware E2E, inspect AnyFlow's
private files for the synthetic title / body / tag" — and those files no longer
existed. It cannot be reported as PASS, and it will not be inferred from the
canary tests, which prove a different (if related) property.

**Recovery was attempted and is staged, not finished.** The app was reinstalled
and notification access re-granted through the real Android settings UI, and the
desktop pairing QR was opened in the GUI with the tablet's scanner live. Android
pairing is **camera-QR only** — `MainActivity.requestScan` → ZXing, with no
manual code entry — so it needs a person. The scan was attempted and reached the
daemon after the pairing window's TTL had already lapsed:

```
INFO anyflow_runtime::listener: connection ended peer_addr=[::ffff:192.168.68.63]:51224
     error=pairing failed: not in pairing mode
```

The tablet then dropped off adb again and the session had to end. No new peer
record was created; the trust store still holds the same eight records it held
before.

**What must happen next time, in this order:** pair → configure → E2E →
**persistence sweep** → *then* the full connected suite, last, alone.

---

## F10. Full `connectedDebugAndroidTest` — **NOT EXECUTED**

Only the filtered NOTIF-SEC-25 run (F8) was executed on device: 8/8, skipped=0.
The complete connected suite — which must also cover the new consent-UI tests
and app-picker UI tests — has **not** been run, because after F9 the device is
unpaired and the run would certify a device in a state the feature cannot be
exercised in. `BUILD SUCCESSFUL` is not reported in its place.

---

## F11. Regression totals — all re-run after the accessibility fix

**Rust** (`desktop/`, `-j 2`, `--locked`):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | **clean** |
| `cargo build --workspace --locked -j 2` | **ok** |
| `cargo test --workspace --locked -j 2` | **590 passed, 0 failed, 18 ignored** |
| `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | **clean** |

The 18 ignored are the display- and D-Bus-gated gates, and they were then run
explicitly:

| Explicit gate | Result |
| --- | --- |
| `cargo test -p anyflow-gui -- --ignored` (GTK widget tree, incl. the five new assertions) | **1 passed, 0 failed** |
| `cargo test -p anyflow-capability-notifications -- --ignored` (`real_dbus`) | **6 passed, 0 failed** |
| … (`real_lock`) | **2 passed, 0 failed** |

The real-D-Bus notification gate was run even though the GUI change does not
touch notification backend interaction — the GUI is a control-socket client, and
the change is to render scheduling plus `PartialEq` derives on the control
types.

One clippy finding was introduced by the fix and fixed:
`Rc<RefCell<Option<Rc<dyn Fn()>>>>` tripped `type_complexity` and is now a named
`Refresh` alias.

**Android JVM** (`./gradlew testDebugUnitTest`, forced with `--rerun-tasks`
because the first invocation was `UP-TO-DATE` and would have reported nothing):

```
files=34 tests=447 failures=0 errors=0 skipped=0
```

These are current counts, parsed from the JUnit XML. No count was carried over
from the first closeout.

---

## F12. Guards

**Windows portable CI (§12).** Both are empty, as required:

```
$ git diff -- .github/                                                   (empty)
$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto   (empty)
$ git status --short protocol/ .github/                                  (empty)
```

The accessibility fix required no CI classification change. `desktop/control` is
neither CI nor protocol; the `PartialEq, Eq` derives added there are additive and
change no serialized form.

**Security / N4 (§13).** Re-audited after the change:

* every occurrence of `cancelNotification` and `DismissRequest` in Android
  production sources is a **comment explaining its absence**; the only code hits
  are Compose's unrelated `onDismissRequest` dialog parameter;
* on the desktop, an inbound `DismissRequest` is **refused**, not executed —
  `desktop/capabilities/notifications/src/lib.rs` answers `RejectedRole` (or
  `NotAuthorized`), parsing the id only so the refusal can be addressed;
* `allow_dismiss_sync` defaults false, is stored false, and
  `desktop/core/src/notification_policy.rs` asserts "dismiss sync must never
  default on";
* no `RemoteInput`, no notification action execution, no reply path. The
  `PendingIntent` uses in the tree are the pre-existing foreground-service,
  quick-settings-tile and clipboard notifications, unrelated to mirroring;
* no `CompanionDeviceManager` association, no `QUERY_ALL_PACKAGES` — every hit
  for both is prose explaining why they are absent;
* no history, no content persistence, no content logging (F8).

**`PairingHelper.kt` (§1 of the brief).** **No file was removed, because none
exists.** It is absent from the working tree, from every `androidTest`/`test`
source set, and from `git log --all -- '*PairingHelper*'` (no commits). §35 of
the first closeout listed it as "must be deleted before this is committed"; it
was evidently never written into the tree, or was removed before this session
began. The `androidTest` set was also swept for any equivalent pairing-automation
helper and there is none — the `pairedAtUnix` / `onPair` hits in
`NotificationUiFixtures.kt`, `NotificationHardwareGateTest.kt` and
`ClipboardPersistenceTest.kt` are peer *record* fixtures for UI tests, not
pairing automation.

---

## F13. Git

```
$ git status --short          24 modified, 18 untracked (unchanged from §35,
                              plus desktop/control/src/lib.rs)
$ git diff --check            (clean)
```

Swept the changed and untracked set for `*.apk`, `*.aab`, `*.key`, `*.pem`,
`*.p12`, `*.pfx`, `*.jks`, `*.keystore`, `*.log`, UI dumps, QR images,
`state.json`, trust-store runtime files, `target/` and `build/`, and for
`PairingHelper`: **no matches**. Every capture, UI dump, log and driver script
from this session lives in the session scratchpad and is not in the tree.

**Nothing was committed, nothing was pushed, no PR was created.**

### Machine state left behind

* **Fedora.** `anyflowd` runs from `desktop/target/debug/anyflowd` at
  `--log info,anyflow_capability_notifications=debug`; `anyflow-gui` is open on
  the Notifications page, both the N3 build. The desktop session was unlocked
  with `loginctl unlock-session 2` and is unlocked. `toolkit-accessibility` and
  the a11y bus's `IsEnabled` were both set **true** for the AT-SPI gate and are
  **still true** — the first closeout had left them false.
* **Peer `769E 7956 7E4C 2402`** is restored to exactly its pre-wave settings:
  `notifications.v1` granted, `mirror on`, `when locked app-only` (the shipped
  default — it was moved to `full` for the persistence gate and moved back),
  `dismiss-sync off`. All eight trust-store records are otherwise untouched.
* **The tablet** has the N3 debug build installed with Android notification
  access granted, **but is no longer paired** — the connected-test run wiped its
  app data (F9). It is off adb in a `sec_acm` USB configuration.

---

## F14. Verdict

| Gate | Result |
| --- | --- |
| Desktop accessibility re-render defect fixed | **PASS** |
| Shipped refresh cadence used in certification (`REFRESH_SECS = 2`) | **PASS** |
| AT-SPI controls reliably actionable | **PASS** — 14/14 stable across 15 cycles, activation first time |
| Accessibility regression test, verified to fail on the old behaviour | **PASS** |
| Android hardware UX items 1–18 | **PASS** |
| Desktop hardware UX items 1–13 | **PASS** |
| Full user-driven SM-X620 → Fedora E2E | **PASS** — 14/14 |
| Android NOTIF-SEC-25 executed | **PASS** — 8/8, skipped=0 |
| **Android persistence sweep** | **NOT EXECUTED** |
| **Full `connectedDebugAndroidTest`** | **NOT EXECUTED** |
| Android JVM | **PASS** — 447/0/0/0 |
| Rust | **PASS** — 590/0, clippy and fmt clean |
| Existing capability grants unchanged | **PASS** |
| `notifications_v1.proto` unchanged | **PASS** |
| No N4 dismissal runtime | **PASS** |
| `PairingHelper.kt` removed | **N/A** — it does not exist |
| No blocker / P0 remains | **PASS** — the §18 blocker is fixed |

Fourteen of sixteen. The two that are not executed are both device-state
casualties of running a `connectedDebugAndroidTest` task before the persistence
sweep, and both are recoverable in one sitting once the tablet is paired again.

**The accessibility blocker that failed the first closeout is fixed, tested, and
measured on hardware at the shipped cadence.** But N3 cannot be called PASS with
the persistence sweep and the full connected suite unexecuted, so it is not.

---

# SECOND FINAL CLOSEOUT — EVIDENCE ONLY — 2026-09-10

The previous closeout (F1–F14) fixed the accessibility blocker and left two
gates unexecuted, both casualties of running a `connectedDebugAndroidTest` task
*before* the persistence sweep. This closeout was asked to execute those two
gates in the mandated order and to change nothing else.

Both were executed. **The persistence sweeps pass. The full connected suite
does not**, and the reason is a genuine discovery rather than a device-state
accident: the two consent-UI instrumented classes added in this wave had
**never been executed on hardware** — F10 records the full suite as NOT
EXECUTED — and running them for the first time fails 14 of 82.

**No source file was changed by this closeout.** The changed-file set is
byte-identical to F13: 24 modified, 18 untracked.

---

## G1. Precheck

```
$ git branch --show-current    feature/notifications-v1-n3-consent-ui
$ git diff --check             (clean)
$ git status --short           24 modified, 18 untracked
```

### `PairingHelper.kt` has never existed in this repository

Checked three ways, all empty:

```
$ find android -name 'PairingHelper.kt' -print      (no output)
$ git log --all -- '**/PairingHelper.kt'            (no output — no commit on any ref)
$ find . -name 'PairingHelper*' -not -path './.git/*'   (no output)
```

It is not in the working tree, not in any reachable history, and nowhere in the
repository. The earlier revision that described it as needing deletion was
wrong; F14 already recorded it as **N/A — it does not exist**, and that stands.
Nothing was deleted, and no deletion is claimed.

The `main` / `develop` wording was corrected in §1 and needs no further change.

---

## G2. Pairing precondition — recreated by hand, verified read-only

The maintainer performed a fresh camera-QR pairing. It was verified with
ordinary diagnostics only — **no trust-store JSON was edited, and no temporary
`PairingHelper` was used or needed**:

```
$ anyflow status
  paired      6 device(s)
    SM-X620  a609d09ddac44a63dc77f7df71f3078c
       fingerprint 6820 C730 BD51 2CC8
       paired      yes
       connected   yes        state connected
       granted     battery.v1
```

The Android side was read through the application's own screens. This is a
**new, ninth** peer record on the desktop; the eight that existed before are
untouched.

---

## G3. N3 configuration restored through the real UIs

Every change below was made by tapping the real Android UI (`uiautomator dump`
+ `input tap`) or by activating a real control in the desktop GTK GUI through
AT-SPI. **No JSON edit, no CLI policy command, no `adb` policy edit.**

### Android — `Devices → Fedora → Notifications`

| Setting | How | Result |
| --- | --- | --- |
| Android notification access | already granted, left alone | **Allowed** |
| `notifications.v1` grant to Fedora | tapped *Share notifications with this computer* | **Granted** |
| Exactly one safe synthetic app | *Clear all*, searched `shell`, ticked the row | **“1 of 99 apps”** — `com.android.shell` |
| Source lock policy | untouched (shipped default) | **Share the app name only** = `APP_ONLY` |
| Ongoing notifications | untouched | **off** |
| Work profile | n/a | *“This device has no work profile.”* |

`com.android.shell` again reached the picker only while it was notifying,
marked **“Notifying now”** — the §7 measurement reproducing exactly, with no
`QUERY_ALL_PACKAGES`.

The three gates then read, in the app's own Details section:

```
Android notification access   Allowed
Notification listener         Bound
notifications.v1 grant        Granted
This device announces         SOURCE · epoch 2
```

### Desktop — GUI Notifications page, peer `6820 C730 BD51 2CC8`

| Setting | How | Result |
| --- | --- | --- |
| Receive notifications from this device | AT-SPI `toggle` on the real switch | **on** |
| Show notifications (mirror) | already on once receive was enabled | **on** |
| Destination lock policy | AT-SPI selection in the real list box | **Show app only** = `app_only` |
| Sync dismissals | — | **no control exists** (label only) |

---

## G4. A finding: a grant made mid-session does not announce until a reconnect

Worth recording because it cost real time and is visible to a user, but it is
**not an N3 regression**.

With the TLS session already established *before* either side granted
`notifications.v1`, enabling the grant on Android and enabling receive on the
desktop left both sides blind to each other for as long as they were watched
(three checks over 24 s):

```
Android UI :  This device announces      SOURCE · epoch 2
              The computer announces     no role · epoch 0
Desktop GUI:  Needs attention
              "0 showing of 0 mirrored · this computer announced 0 roles
               (epoch 0) · the device claims no source role (epoch 0)"
```

The desktop's explanatory line — *“Connected, and the device has not said it can
send notifications. Check that AnyFlow on the device shares notifications with
this computer”* — was misleading here, because the device **was** sharing.

Tapping **Disconnect** then **Connect** in the Android UI fixed it immediately
and completely:

```
Desktop GUI:  Connected
              "3 showing of 3 mirrored · this computer announced 1 role
               (epoch 1) · the device can send notifications (epoch 2)"
```

The cause is that capability sets are negotiated at session start, so a
capability granted mid-session has no negotiated channel to announce a role on
until the session is renegotiated. That is N0 protocol behaviour, not something
N3 introduced, and fixing it would mean changing the protocol — explicitly out
of scope here. Logged as a debt: **the desktop's “Needs attention” copy should
distinguish “the device has not said it can send notifications” from “this
session predates the grant; reconnect”.**

---

## G5. Minimal live flow, with fresh canaries

The shade was cleared first, so the flow started from a measured zero
(`0 showing of 0 mirrored`). Canaries for this run, all synthetic and
non-sensitive:

| Field | Value |
| --- | --- |
| title | `AFN3FINAL9317 narwhal` |
| body | `kingfisher tamarind 9317 closeout` |
| updated body | `… closeout updated peregrine`, then `… closeout ibex` |
| tag | `afn3final-9317-tag` |
| raw platform key | `0\|com.android.shell\|2020\|afn3final-9317-tag\|2000` |

**Android source → AnyFlow TLS session → Fedora notification**, all three
observed:

| Step | Evidence |
| --- | --- |
| posted from `com.android.shell` | `cmd notification post` accepted |
| the Android source picked it up | desktop count moved `0 → 1` |
| it crossed the paired TLS session | desktop GUI: **“1 showing of 1 mirrored”** |
| it reached the Fedora notification server | a real `org.freedesktop.Notifications.Notify` method call, `x-shell-sender-pid = 352725` — the `anyflowd` process |
| **the update replaced it** | second `Notify` carried **`replaces_id = 43`** (non-zero) and the count stayed **1 of 1** — replacement in place, no duplicate |
| an unchanged repost | produced **no** `Notify` at all — correctly deduplicated |

### The AppOnly guarantee, observed live rather than argued

The Fedora session was **locked** during the flow (`org.gnome.ScreenSaver
GetActive → true`, `LockedHint=yes`), and the GUI said so: *“This computer is
locked.”* With the destination policy at **Show app only**, the notification
that reached the notification server was:

```
app_name    "Shell"
summary     "Shell"          <- the app's name, not the title
body        ""               <- removed
replaces_id 43
```

Switching the same peer to **Show full content** through the GUI and updating
the notification produced, over the same session:

```
summary     "AFN3FINAL9317 narwhal"
body        "kingfisher tamarind 9317 closeout ibex"
replaces_id 43
```

That pair of observations is worth more than either alone. It proves the full
title and body **did** cross the TLS link and **were** held in the desktop's
memory, and that AppOnly removes them at the D-Bus boundary, exactly as the
GUI's own words promise: *“The text is removed before it reaches the
notification server.”* It also means the persistence sweep below is a real
test — the desktop genuinely had the content it must not persist.

The policy was returned to **Show app only** immediately afterwards.

---

## G6. Android persistence sweep — **PASS**

Executed **before** any `connectedDebugAndroidTest` invocation. This is the
evidence F9 could not produce.

The application's private tree holds **exactly three files**:

| File | Bytes | What it is |
| --- | ---: | --- |
| `files/trust-store.json` | 3566 | AnyFlow's configuration and trust metadata |
| `files/profileInstalled` | 24 | ART baseline-profile marker, written by the platform |
| `shared_prefs/android.app.ActivityThread.IDS.xml` | 108 | the framework's own file (`IDSCount=1`) |

`cache/` and `code_cache/` are **empty**. There is no `databases/`, no
`no_backup/`, and no external app directory at all
(`/sdcard/Android/{data,media,obb}/io.github.yurisismotto.anyflow` — none exist).

**Canary sweep across all three files — zero matches, every token:**

```
AFN3FINAL9317  narwhal  kingfisher  tamarind  peregrine  ibex  9317
afn3final  closeout  Warmup  warmup-picker            -> 0 matches
AFN3CANARY  7712  zebra  quokka  (previous run's)     -> 0 matches
com.android.shell|2020   0|com.android.shell          -> 0 matches
shell_cmd  sbnKey  notificationHistory  postedAt      -> 0 matches
"title"  "body"  "text"  "tag"                        -> 0 matches
```

**Complete leaf-key inventory of `trust-store.json`** — enumerated
exhaustively, including `false`-valued keys (note: `jq 'paths(scalars)'`
silently drops them, which is why this was done structurally instead):

```
schemaVersion                                deviceId
deviceName                                   notificationSecretGeneration
peers[].deviceId                             peers[].deviceName
peers[].fingerprint                          peers[].pairedAtUnix
peers[].grantedCapabilities[]                peers[].addresses[]
peers[].clipboardPolicy.allowSend            peers[].clipboardPolicy.allowReceive
peers[].clipboardPolicy.autoSend             peers[].clipboardPolicy.autoReceive
peers[].notificationPolicy.allowMirror       peers[].notificationPolicy.allowedApps[]
peers[].notificationPolicy.knownApps[]       peers[].notificationPolicy.includeWorkProfile
peers[].notificationPolicy.includeOngoing    peers[].notificationPolicy.whenSourceLocked
peers[].notificationPolicy.allowDismissSync
```

That is the approved set and nothing else. The stored policy, verbatim:

```json
{ "allowMirror": true,
  "allowedApps": ["com.android.shell"],
  "includeWorkProfile": false,
  "includeOngoing": false,
  "whenSourceLocked": "APP_ONLY",
  "allowDismissSync": false }
```

plus `knownApps` — 99 package names, no labels, no content — and
`notificationSecretGeneration: 1`.

**No title. No body. No tag. No raw Android notification key. No notification
history. No payload.** The longest string value in the entire file is 64
characters, the peer's SHA-256 fingerprint hex: there is no room for, and no
instance of, key material or a payload blob. No secret was printed or exported;
the Keystore-held notification secret never appears in the file at all.

---

## G7. Desktop persistence sweep — **PASS**

Locations that exist:

| Path | Bytes | Note |
| --- | ---: | --- |
| `~/.local/share/anyflow/state.json` | 7202 | daemon state and policy |
| `~/.local/share/anyflow/identity.key` | 138 | private key — swept, **never printed** |
| `/run/user/1000/anyflow/control.sock` | 0 | socket |

There is no `~/.config/anyflow` and no `~/.cache/anyflow`.

Same canary set as G6, plus `Shell`, `title`, `body`, `summary`, `payload`,
`history`, `notificationHistory`, `text`, `tag`, `sbn`: **zero matches in
either file.**

**Complete leaf-key inventory of `state.json`:**

```
schema_version  device_id  certificate_der_b64  key_backing
settings.device_name  settings.listen_port  settings.auto_grant[]
peers[].device_id  peers[].device_name  peers[].fingerprint  peers[].platform
peers[].paired_at_unix  peers[].revoked  peers[].last_protocol_version
peers[].granted_capabilities.{battery.v1,clipboard.v1,files.v1,notifications.v1}
peers[].clipboard_policy.{allow_send,allow_receive,auto_send,auto_receive}
peers[].notification_policy.{allow_mirror,allow_dismiss_sync,when_sink_locked}
```

Policy and configuration only — no notification history, no payload, no
notification text of any kind. The certified peer:

```
notification_policy: allow_mirror=true, when_sink_locked='app_only',
                     allow_dismiss_sync=false
```

`allow_dismiss_sync` is **false for all nine peers**. The only string over 80
characters is `certificate_der_b64` (552 chars — the host's own public
certificate), and it was not printed.

Nothing was committed, and no capture left the session scratchpad.

---

## G8. Real-flow logging sweep — **PASS on the Android side**

NOTIF-SEC-25 was *not* re-run on its own; it ran inside the one complete
connected suite (G10) and passed 8/8, skipped=0 again. What is added here is the
**real-flow** sweep against live fixture content, performed before the connected
suite, which F8 flagged as still owed.

The full 205 026-line logcat buffer covering the flow was swept. Hits exist, and
every one is attributable away from AnyFlow:

| Where the canary appears | pid | What it is |
| --- | --- | --- |
| `ExpandableNotifRow`, `InterruptionStateProvider`, `Bubbles`, `View` | 2337 | **Android SystemUI** logging the notification key |
| `HoneySpace.NotificationListener: onNotificationPosted` | 2738 | **Samsung One UI launcher's own** notification listener |
| `adbd` | 29638 | **my own** `adb shell cmd notification post` command line |
| `nativeloader: Extending system_exposed_libraries` | various | false positive — `ibex` is a substring of `libexif.so` |

Restricted to the AnyFlow application's own process — **all 1766 lines from
pid 22644** — every token returns **zero**:

```
AFN3FINAL9317  narwhal  kingfisher  tamarind  peregrine  ibex
afn3final  9317  closeout  com.android.shell|2020  0|com.android.shell
                                              -> 0 hits, all of them
```

This is SystemUI and OEM logging, not AnyFlow logging, and the two are kept
apart here deliberately.

**Honest limit on the desktop half.** The `anyflowd` under test was started by
the maintainer in an interactive terminal; its stdout and stderr go to
`/dev/pts/3`, it writes no log file, and it logs nothing to journald. Its
output could therefore not be replayed in this session, so **no new desktop-side
log sweep was performed.** F8's daemon-log sweep against the previous fixture
stands and is not restated as if it were re-measured.

---

## G9. Accessibility host setting — restored

The certification itself was completed with accessibility enabled (F4). The host
has now been returned to the state it was in before the accessibility
investigation:

```
before:  org.gnome.desktop.interface toolkit-accessibility   true
after:   org.gnome.desktop.interface toolkit-accessibility   false
         org.a11y.Status IsEnabled                           false
         org.a11y.Status ScreenReaderEnabled                 false
```

`anyflow-gui` survived the change and is still running. **No application
accessibility implementation was touched** — the F2 fix is unchanged.

Incidentally corroborating F4 rather than replacing it: this closeout drove the
GUI through AT-SPI for roughly 30 minutes at the shipped `REFRESH_SECS = 2`,
locating the peer card by fingerprint and activating switches and list
selections. Object identity held throughout and every activation took effect
first time. F4's measured 14/14 across 15 cycles remains the citable evidence.

---

## G10. Full `connectedDebugAndroidTest` — **FAIL**

Run **last**, after G1–G9, as the complete task with **no class filter**:

```
$ JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
    ./gradlew :app:connectedDebugAndroidTest
Tests on SM-X620 - 16 failed: There was 14 failure(s).
BUILD FAILED in 2m 47s
```

The JUnit XML was parsed rather than trusting Gradle's summary
(`app/build/outputs/androidTest-results/connected/debug/TEST-SM-X620 - 16-_app-.xml`):

```
tests=82   passed=68   failures=14   errors=0   skipped=0
```

| Class | Tests | Failures | Skipped |
| --- | ---: | ---: | ---: |
| `AppPickerUiTest` | 17 | **7** | 0 |
| `NotificationConsentUiTest` | 22 | **7** | 0 |
| `NotificationLoggingCanaryTest` | 8 | 0 | 0 |
| `NotificationHardwareGateTest` | 10 | 0 | 0 |
| `NotificationSecretInstrumentedTest` | 4 | 0 | 0 |
| `ClipboardInstrumentedTest` | 9 | 0 | 0 |
| `ClipboardPersistenceTest` | 3 | 0 | 0 |
| `DeviceIdentityTest` | 4 | 0 | 0 |
| `DownloadsTest` | 5 | 0 | 0 |

All three N3 instrumented suites did execute:
`NotificationConsentUiTest`, `AppPickerUiTest`, `NotificationLoggingCanaryTest`.
This is one complete unfiltered run; no partial runs were added together.

**`skipped=0` was earned, not assumed.** Every `assumeTrue` gate in
`NotificationHardwareGateTest` was satisfied in advance, which is why it reports
10/10 rather than skipping — the first time that has happened on hardware:

```
./gradlew :app:installDebug :app:installDebugAndroidTest   # in-place, data preserved
adb shell pm grant … POST_NOTIFICATIONS ; … CAMERA         # the focus thief
adb shell cmd notification allow_listener <component>      # already present; re-asserted
adb shell cmd notification post -t 'ANYFLOW-N1-FIXTURE-TITLE' \
    anyflow-n1-fixture 'ANYFLOW-N1-FIXTURE-BODY'
# verified: listener BOUND, fixture active, device awake and unlocked
```

The in-place install preserved both the trust store and the listener approval,
so the persistence sweep in G6 was taken from the fully configured, paired state
and only *then* was the suite allowed to tear it down.

### The 14 failures are test-authoring defects, not product defects

F10 records the full suite as **NOT EXECUTED**, so `AppPickerUiTest` and
`NotificationConsentUiTest` — both added in this wave — had **never run on a
device**. They were executed for the first time here, and they assert against a
semantics tree the product does not have. Grouped by cause:

| # | Cause | Failing tests |
| --: | --- | --- |
| 6 | `assertIsOn()` / `assertIsOff()` on a node matched by `hasText(...)`. The product merges the row and publishes its state as a **`StateDescription`**; `ToggleableState` lives on the inner child, so the assertion cannot see it. | `a_fresh_picker_has_nothing_ticked`, `the_stored_selection_is_what_the_rows_show`, `unticking_an_application_removes_exactly_that_one`, `a_paired_computer_starts_with_notifications_off`, `ongoing_notifications_are_off_and_do_not_bypass_the_app_list`, `a_device_with_a_work_profile_is_offered_the_switch_and_it_is_off` |
| 3 | `performClick()` on the merged row, which is deliberately not the touch target, so nothing is written | `ticking_an_application_adds_exactly_that_one`, `select_all_shares_everything_only_after_it_is_confirmed`, `turning_the_switch_on_writes_the_grant_and_selects_no_application` |
| 1 | `Failed to inject touch input` — target not hittable | `cancelling_select_all_shares_nothing` |
| 1 | `Failed to assert count of nodes` | `anyflows_own_package_is_not_offered` |
| 1 | ambiguous matcher — `Text contains 'Allowed'` matches **2** nodes, because the screen legitimately shows the state twice (the *Notification access* row **and** the *Android notification access* diagnostics row) | `android_access_is_shown_as_its_own_row_with_its_own_state` |
| 2 | scroll/visibility — `performScrollTo() failed`, `component is not displayed` | `a_missing_android_permission_is_named_and_not_folded_into_the_grant`, `protocol_vocabulary_is_confined_to_the_details_section` |

The evidence that the **product** is right and the **test** is wrong is in the
failure dumps themselves. The node the picker actually publishes:

```
Node #835  ContentDescription = '[Banco]'
           StateDescription   = 'Shared'
           Text               = '[Banco, com.example.bank]'
           MergeDescendants   = 'true'   Has 1 child
Selector used: (Text + EditableText contains 'Banco')
```

That is an accessible row: one merged node carrying the app's name, its package
and its sharing state as words a screen reader reads out. The test asked it for
`ToggleableState`, which that node does not carry. §F6 had already spotted the
same structure from the other side — *“the row itself reports
`clickable=false`; the tick is on the `CheckBox` node”* — and filed it as an
observation for a later wave. It is in fact what these 14 assertions are built
on top of.

Independent confirmation that the product behaves correctly, from this session
and from the previous one:

* G3 configured the grant, the picker and the policy **through the real UI** and
  the store recorded exactly `allowedApps: ["com.android.shell"]`, with the
  screen reading *“1 of 99 apps”*.
* G5 mirrored a notification end to end and replaced it in place.
* The user-driven E2E is **14/14** (F7) and Android UX items 9–18 **PASS** (F6).
* `NotificationHardwareGateTest` **10/10** and `NotificationLoggingCanaryTest`
  **8/8**, skipped=0, in this same run.

**Nothing was changed to make these pass.** The brief for this closeout is
evidence-only and forbids refactoring, and choosing the contract these tests
should hold the UI to — assert the merged row's `StateDescription`, or reach the
inner toggleable child, or make the whole row one toggleable target — is a
design decision, and the same one §F6 deferred. Rewriting 14 assertions to match
observed behaviour would also risk turning them into restatements of it. The
diagnosis above is itemised so the fix is mechanical once that call is made.

---

## G11. Local regression — preserved, not re-run

**No source file changed during this closeout**, so the F11 evidence is not
invalidated and is not re-measured:

| Suite | Result | Status |
| --- | --- | --- |
| Android JVM unit tests | **447 passed, 0 failed** | preserved from F11 |
| Rust workspace | **590 passed, 0 failed** | preserved from F11 |
| `cargo fmt --all --check` | clean | preserved from F11 |
| `cargo clippy … -D warnings` | clean | preserved from F11 |

The `git status` set is byte-identical to F13's (24 modified, 18 untracked), and
`git diff --check` is clean. The 14 connected-suite failures are in two
**untracked, uncommitted** test files and required no product change, so no
regression set became stale.

---

## G12. Protocol and N4 guards — re-verified

```
$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
(0 bytes)
$ git status --short protocol/
(no changed files at all under protocol/)
```

| Guard | Result |
| --- | --- |
| `notifications_v1.proto` unchanged | **PASS** — empty diff, and nothing under `protocol/` is modified |
| `cancelNotification` call site | **PASS** — no real invocation anywhere in `android/app/src`; every hit is prose or a rejection comment |
| `DismissRequest` runtime | **PASS** — answered `REJECTED_ROLE`; other hits are Compose's unrelated `onDismissRequest` dialog parameter |
| `RemoteInput` / notification action / reply | **PASS** — none; only prose stating their absence |
| `PendingIntent` remote action | **PASS** — none in the `notifications/` package. The two real uses are AnyFlow's own foreground-service and clipboard notifications, pre-existing and unrelated |
| `allowDismissSync` inert | **PASS** — `false` by default, `false` on disk for all nine peers, and **no runtime consumer**: `dismiss_sync_enabled()` has zero callers in the desktop, and `allowDismissSync` is referenced on Android only in `NotificationPolicy.kt` for storage |
| `QUERY_ALL_PACKAGES` | **PASS** — not declared. The manifest declares only `INTERNET`, `ACCESS_NETWORK_STATE`, `CHANGE_WIFI_MULTICAST_STATE`, `CHANGE_NETWORK_STATE`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_CONNECTED_DEVICE`, `POST_NOTIFICATIONS`, `CAMERA` |
| `CompanionDeviceManager` | **PASS** — no association; only comments recording why |
| Notification history | **PASS** — no `notificationHistory` / `NotificationHistory` symbol anywhere, and none on disk on either side (G6, G7) |
| No dismissal / reply / action control in the desktop GUI | **PASS** — the only two interactive controls on the whole page are *Receive notifications from this device* and *Show notifications*; *Sync dismissals* is a label with no control |

---

## G13. Git

```
$ git status --short      24 modified, 18 untracked   (identical to F13)
$ git diff --check        (clean)
$ git diff --stat         24 files changed, 1293 insertions(+), 90 deletions(-)
```

Swept the modified and untracked set for `*.apk`, `*.aab`, `*.key`, `*.pem`,
`*.p12`, `*.pfx`, `*.jks`, `*.keystore`, `*.log`, QR images, screenshots, UI
dumps, runtime `state.json`, trust-store files, `identity.key`, `target/`,
`build/` and `PairingHelper`: **no matches.** `android/app/build/` and
`desktop/target/` are both gitignored. Every capture, UI dump, logcat, D-Bus
trace and driver script from this session lives in the session scratchpad and
is not in the tree.

**Nothing was added, committed or pushed. No PR was created.**

### Machine state left behind

* **Fedora.** `anyflowd` and `anyflow-gui` still running, both the N3 build.
  `toolkit-accessibility` is back to **false**, as is the a11y bus's
  `IsEnabled`. Peer `6820 C730 BD51 2CC8` keeps `notifications.v1` granted,
  mirror on and `when_sink_locked = app_only`; the desktop's own screen session
  is **locked**, which is how G5 observed the AppOnly redaction live. The other
  eight peer records are untouched, and `allow_dismiss_sync` is false for all
  nine.
* **The tablet** is **unpaired again** — the connected suite uninstalled both
  APKs when it finished, taking the app data with it. That was expected and
  planned for: the persistence sweep (G6) was completed *before* it. Per the
  brief, no attempt was made to recover the pairing afterwards.
* `enabled_notification_listeners` needs no restoration. The only entry this
  closeout touched was AnyFlow's own, which was **already present** in the value
  captured at the start, and which the platform drops with the uninstalled
  package. The Samsung launcher and Smart Mirroring entries were never altered.
* **adb / MTP flakiness recurred and is not fixed.** After the suite finished
  the tablet dropped off adb again; `lsusb` still shows
  `04e8:6860 … (MTP mode)`, so the USB configuration flipped to MTP-only once
  more. `adb reconnect` and a full server restart both failed to recover it — it
  needs a physical replug, which could not be done from here. No claim is made
  that this is fixed, and no post-run device readback was taken.

---

## G14. Verdict

| Gate | Result |
| --- | --- |
| Accessibility blocker remains fixed | **PASS** — F1–F4, host setting now restored (G9) |
| Android UX gate (items 1–18) | **PASS** — preserved from F6 |
| Full user-driven E2E | **PASS** — 14/14, preserved from F7 |
| **Android persistence sweep** | **PASS** — G6, taken before the uninstall |
| **Desktop persistence sweep** | **PASS** — G7 |
| Real-flow logging sweep (Android) | **PASS** — G8, zero AnyFlow-attributed hits |
| NOTIF-SEC-25 | **PASS** — 8/8, skipped=0, inside the one complete run |
| `NotificationHardwareGateTest` | **PASS** — 10/10, skipped=0, first time unskipped |
| `skipped = 0` across the connected suite | **PASS** — G10 |
| **Full `connectedDebugAndroidTest`, one unfiltered run** | **FAIL** — 82 tests, **14 failures**, 0 errors, 0 skipped |
| Android JVM / Rust / fmt / clippy | **PASS** — preserved, no source changed (G11) |
| `notifications_v1.proto` unchanged | **PASS** — G12 |
| N4 runtime absent | **PASS** — G12 |
| `PairingHelper.kt` | **N/A** — has never existed on any ref (G1) |
| No P0 / BLOCKER remains | **PASS** — the F1 blocker is fixed; the 14 failures are in untracked test files, not the product |

Two gates that F14 could not execute are now executed, and both persistence
sweeps pass — the specific evidence the previous closeout was missing. The
sequencing mistake was not repeated: the sweep was taken from the live,
configured, paired state, and the connected suite was allowed to destroy that
state only afterwards.

But the last gate is measured, not inferred, and it does not pass. §13 of the
brief requires a complete unfiltered `connectedDebugAndroidTest` with
`failures=0`, `errors=0`, `skipped=0`. This run gives `skipped=0` and
`errors=0` — and **14 failures**. The product is not implicated: the failures
are in two never-before-executed instrumented test files and are diagnosed
individually in G10. That distinction changes what the fix is, and how small it
is. It does not change the arithmetic.

## NOTIFICATIONS.V1 N3 FAIL
## N4 NOT READY

---

# THIRD FINAL CLOSEOUT — TEST CONTRACT — 2026-09-10

G10 stands as written. It diagnosed 14 failures in one complete unfiltered
`connectedDebugAndroidTest` and declined to fix them, because choosing the
semantics contract the tests should hold the UI to is a design decision and the
brief for that closeout was evidence-only. This closeout makes that call, fixes
the tests, and re-runs the gate. Nothing in G10 is amended or erased.

## H1. Where this started

The run recorded in G10, parsed from the JUnit XML rather than from Gradle's
summary:

```
tests=82   passed=68   failures=14   errors=0   skipped=0
```

| Class | Tests | Failures |
| --- | ---: | ---: |
| `AppPickerUiTest` | 17 | **7** |
| `NotificationConsentUiTest` | 22 | **7** |
| the other seven classes | 43 | 0 |

## H2. The cause categories, as G10 itemised them

| # | Cause | Failing tests |
| --: | --- | --- |
| 6 | `assertIsOn()` / `assertIsOff()` on a node matched by `hasText(...)` — the merged row publishes a `StateDescription`, not a `ToggleableState` | `a_fresh_picker_has_nothing_ticked`, `the_stored_selection_is_what_the_rows_show`, `unticking_an_application_removes_exactly_that_one`, `a_paired_computer_starts_with_notifications_off`, `ongoing_notifications_are_off_and_do_not_bypass_the_app_list`, `a_device_with_a_work_profile_is_offered_the_switch_and_it_is_off` |
| 3 | `performClick()` on a node that is deliberately not the touch target | `ticking_an_application_adds_exactly_that_one`, `select_all_shares_everything_only_after_it_is_confirmed`, `turning_the_switch_on_writes_the_grant_and_selects_no_application` |
| 1 | `Failed to inject touch input` | `cancelling_select_all_shares_nothing` |
| 1 | `Failed to assert count of nodes` | `anyflows_own_package_is_not_offered` |
| 1 | ambiguous matcher — "Allowed" is on the screen twice, legitimately | `android_access_is_shown_as_its_own_row_with_its_own_state` |
| 2 | scroll / visibility | `a_missing_android_permission_is_named_and_not_folded_into_the_grant`, `protocol_vocabulary_is_confined_to_the_details_section` |

## H3. The semantics contract, decided

**A row is what a person perceives. The control inside it is what a person
operates. They are different nodes, and a test must ask each of them the
question it can actually answer.**

Measured on the device, an application row is:

```
Node  ContentDescription = '[Banco]'
      StateDescription   = 'Shared'
      Text               = '[Banco, com.example.bank]'
      MergeDescendants   = 'true'   Has 1 child
        └── Checkbox   ContentDescription = '[Banco]'   ToggleableState = On
```

The one child survives the merge because `Modifier.toggleable` is itself a
merging boundary — which is exactly why the row carries no `ToggleableState`
and why `assertIsOff()` on it could never have worked.

So:

* **accessible state** → assert the merged row's `StateDescription`
  (*Shared* / *Not shared*);
* **interaction and control state** → reach the real `Checkbox` / `Switch` in
  the unmerged tree and assert `ToggleableState` there, and click *it*.

The two are used where each is the point of the test rather than mechanically
doubled up. `the_stored_selection_is_what_the_rows_show` is about what the rows
*show*, so it asserts `StateDescription`; `a_fresh_picker_has_nothing_ticked` is
about the ticks, so it asserts the checkboxes.

**No test tag was added anywhere.** Every control was already addressable by a
stable semantic matcher, because the product already gives each one the row's
name as a `contentDescription` — the same property that stops a screen reader
announcing a contextless "tick box, not checked":

```kotlin
isToggleable() and hasContentDescription(label)   // useUnmergedTree = true
```

`grep -rn "testTag" android/app/src/main/` returns nothing.

**§2 was honoured.** The application row is still not the touch target, the
row is still merged, the checkbox is still the control, and the diagnostics
duplicate of "Allowed" is still on the screen. Nothing was made clickable, and
no click area, toggle behaviour, layout or accessibility semantic was changed.

## H4. What each fix actually does

| Test | Before | After |
| --- | --- | --- |
| the six `assertIsOn`/`assertIsOff` | asked a presentation node for a toggle state | `checkboxFor(...)` / `switchFor(...)` — the real control, unmerged tree |
| the three `performClick` | clicked a label | clicks the control, then asserts the recorded policy or grant write |
| `cancelling_select_all_shares_nothing` | touch injection into the dialog window, which fails on this device | the dialog's own `OnClick` semantics action, scoped by `hasAnyAncestor(isDialog())`; asserts the recorded policy list is *unchanged*, not merely still empty |
| `select_all_shares_everything_only_after_it_is_confirmed` | `onAllNodesWithText("Select all")[1]` — a guess about ordering | the button inside the dialog, by ancestry |
| `anyflows_own_package_is_not_offered` | handed the screen a hand-written row the real inventory can never produce, then required the screen to re-filter it | builds the list through `NotificationApps.build`, where the rule lives, with AnyFlow's package arriving from **all three sources at once** including a stored policy that names it; then asserts **zero picker rows** for it — a row being a node that carries a sharing state, not a string appearing somewhere |
| `android_access_is_shown_as_its_own_row_with_its_own_state` | `hasText("Allowed")`, which matches two nodes | the gate's own status badge, which publishes its state as a `contentDescription`; also asserts the grant switch is independently on, and that the **Details** duplicate is still there |
| `a_missing_android_permission_is_named_and_not_folded_into_the_grant` | ambiguous "Not allowed", then assumed the headline was still on screen after scrolling | the badge, then the grant switch still on, then `performScrollTo()` back to the headline |
| `protocol_vocabulary_is_confined_to_the_details_section` | `onNodeWithText("SOURCE")` exact, but the row reads `SOURCE · epoch 1`; and asserted "Ready" displayed after scrolling past it | asserts the plain-words status first, scrolls through the screen's real container to **Details**, then pins confinement as *each protocol term occurs once, on a Details row* |

One test-side discovery worth recording: the confirmation dialog's dismiss
button is `android.R.string.cancel`, and **this device's framework does not
spell it "Cancel"**. The first corrected run failed on that alone (40/41). The
test now resolves the same resource the screen resolves instead of hard-coding
a word, so it asserts against the product rather than against a device skin.

## H5. The regression test §9 asked for

Two, one per suite, each pinning the architecture rather than restating
observed behaviour:

* `AppPickerUiTest.a_row_is_an_accessible_summary_and_the_checkbox_is_the_control`
  — merged: label, package and `StateDescription` on one node; unmerged:
  **exactly one** toggleable control for that application; the row itself is
  **not** toggleable; and the control is what writes, for exactly that package.
* `NotificationConsentUiTest.a_capability_row_reads_as_a_row_and_the_switch_stays_targetable`
  — title and description are text, the label is **not** a control, there is
  exactly one switch and it carries the row's name, and pressing it writes.

Both document *why* the merged row is not required to expose a
`ToggleableState`: it is what makes the row read as one item to a screen
reader, and a row-sized touch target on a row this tall would make granting by
accident far too easy.

## H6. Files changed

```
android/app/src/androidTest/java/io/github/yurisismotto/anyflow/AppPickerUiTest.kt
android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationConsentUiTest.kt
```

Two files. Both `androidTest`. Both already untracked before this closeout.

## H7. Did production behaviour change?

**No.** No file under `android/app/src/main/`, `desktop/`, or `protocol/` was
opened for writing. No test tag was added, so not even the
"testTags-only, no behavioural path changed" case in §12 applies — there is no
production diff at all. The tracked-file list from `git diff --name-status` is
byte-identical to the one G13 recorded, and the untracked list is unchanged.

Consequently **no previous evidence became stale**: the Android and desktop
persistence sweeps (G6, G7), the user-driven E2E (F7), the UX gates (F6), the
logging sweeps (F8, G8) and the local JVM/Rust/fmt/clippy totals (G11) all
still describe the code that was just tested.

## H8. Targeted run — the two corrected classes

Run first as a fast feedback loop, as §10 allows. Second attempt, after the
`android.R.string.cancel` fix:

```
$ JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
    ./gradlew :app:connectedDebugAndroidTest \
    -Pandroid.testInstrumentationRunnerArguments.class=\
io.github.yurisismotto.anyflow.AppPickerUiTest,\
io.github.yurisismotto.anyflow.NotificationConsentUiTest
Starting 41 tests on SM-X620 - 16
SM-X620 - 16 Tests 41/41 completed. (0 skipped) (0 failed)
BUILD SUCCESSFUL in 1m 11s
```

From the XML — `tests=41 failures=0 errors=0 skipped=0`:

| Class | Result |
| --- | --- |
| `AppPickerUiTest` | **18/18** — the 17 §10 asked for, plus the contract test from §9 |
| `NotificationConsentUiTest` | **23/23** — the 22 §10 asked for, plus the contract test from §9 |

This is a partial run and N3 is **not** called from it.

## H9. Full `connectedDebugAndroidTest` — **PASS**

Hardware preconditions prepared exactly as G10 did, and for the same reason:
`skipped=0` has to be earned, because every gate in
`NotificationHardwareGateTest` skips rather than fails when its precondition is
absent.

```
./gradlew :app:installDebug :app:installDebugAndroidTest    # in-place install
adb shell pm grant … POST_NOTIFICATIONS ; … CAMERA          # the focus thief
adb shell cmd notification allow_listener \
    io.github.yurisismotto.anyflow/…notifications.AnyFlowNotificationListener
adb shell cmd notification post -t 'ANYFLOW-N1-FIXTURE-TITLE' \
    anyflow-n1-fixture 'ANYFLOW-N1-FIXTURE-BODY'
# verified: approval in enabled_notification_listeners, fixture active in
# dumpsys notification, mWakefulness=Awake, mDreamingLockscreen=false
```

Then the complete, unfiltered task:

```
$ JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
    ./gradlew :app:connectedDebugAndroidTest
Starting 84 tests on SM-X620 - 16
SM-X620 - 16 Tests 74/84 completed. (0 skipped) (0 failed)
Finished 84 tests on SM-X620 - 16
BUILD SUCCESSFUL in 2m 48s
```

JUnit XML, parsed rather than trusted from the summary
(`app/build/outputs/androidTest-results/connected/debug/TEST-SM-X620 - 16-_app-.xml`):

```
tests=84   failures=0   errors=0   skipped=0
```

| Class | Tests | Failures | Skipped |
| --- | ---: | ---: | ---: |
| `AppPickerUiTest` | 18 | 0 | 0 |
| `NotificationConsentUiTest` | 23 | 0 | 0 |
| `NotificationHardwareGateTest` | 10 | 0 | 0 |
| `NotificationLoggingCanaryTest` | 8 | 0 | 0 |
| `NotificationSecretInstrumentedTest` | 4 | 0 | 0 |
| `ClipboardInstrumentedTest` | 9 | 0 | 0 |
| `ClipboardPersistenceTest` | 3 | 0 | 0 |
| `DeviceIdentityTest` | 4 | 0 | 0 |
| `DownloadsTest` | 5 | 0 | 0 |
| **Total** | **84** | **0** | **0** |

One complete unfiltered run. No partial runs were added together. The total is
84 rather than G10's 82 because §9's two contract tests were added; the 39
tests §10 named are all present and all pass.

`NOTIF-SEC-25` (8/8) and `NotificationHardwareGateTest` (10/10) are inside this
same run, both with `skipped=0`.

## H10. Scope — JVM and Rust

Per §12: only `androidTest` sources changed, and no test tags were added to any
production source, so no shared production code changed. The Android JVM suite
(447/0) and the Rust suite (590/0) are preserved from G11 and were not re-run —
they describe unchanged code.

```
$ git diff --check
(clean)
```

## H11. Guards

```
$ git diff -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
(empty)
```

`notifications_v1.proto` is **unchanged**. N4's runtime is still absent — no
dismissal path, no `NotificationSink` on Android, and the "Sync dismissals" row
is still inert text with no toggle, which
`there_is_no_working_dismissal_control` now pins from two directions (no
toggleable node with that text, and none carrying it as a `contentDescription`).

## H12. Git

Nothing was committed, pushed, or opened as a PR.

```
$ git status --short
 M android/app/build.gradle.kts
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/AnyFlowNotificationListener.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationPolicy.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationSource.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/AnyFlowShell.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainState.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/Navigation.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PeerDetailScreen.kt
 M android/app/src/main/res/values/strings.xml
 M android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationSourceTest.kt
 M android/gradle/libs.versions.toml
 M desktop/control/src/lib.rs
 M desktop/daemon/tests/common/mod.rs
 M desktop/daemon/tests/notifications.rs
 M desktop/gui/data/style.css
 M desktop/gui/src/lib.rs
 M desktop/gui/src/views/clipboard.rs
 M desktop/gui/src/views/dashboard.rs
 M desktop/gui/src/views/mod.rs
 M desktop/gui/src/views/pairing.rs
 M desktop/gui/src/views/peers.rs
 M desktop/runtime/src/server.rs
?? NOTIFICATIONS-V1-N3-REPORT.md
?? android/app/src/androidTest/java/io/github/yurisismotto/anyflow/AppPickerUiTest.kt
?? android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationConsentUiTest.kt
?? android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationLoggingCanaryTest.kt
?? android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/InstalledApps.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationApps.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/NotificationReadiness.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/AppPickerScreen.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/NotificationSettingsScreen.kt
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/NotificationUiMapping.kt
?? android/app/src/main/res/drawable/ic_notifications.xml
?? android/app/src/main/res/drawable/ic_search.xml
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationAppsTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationNavigationTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationPolicyStorageTest.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationReadinessTest.kt
?? desktop/gui/src/views/notifications.rs

$ git diff --stat | tail -1
 24 files changed, 1293 insertions(+), 90 deletions(-)
```

Identical to G13's, which is the point: this closeout added no production diff.

### Machine state left behind

* The tablet is **unpaired and AnyFlow is uninstalled** — the connected suite
  uninstalls both APKs when it finishes, as G6 had already anticipated by
  taking the persistence sweep first.
* `enabled_notification_listeners` is **byte-identical** to the value captured
  before this closeout began; the approval AnyFlow held went away with the
  uninstall, and the two Samsung listeners are untouched.
* The `com.android.shell` fixture notification (`anyflow-n1-fixture`) is still
  posted. `cmd notification` has no `cancel` subcommand and `pm clear
  com.android.shell` does not remove it; a listener calling
  `cancelNotification(key)` is the only clean removal, and there is no longer a
  listener installed. It is inert and is dismissable by hand from the shade.

## H13. Verdict

| Gate | Result |
| --- | --- |
| Semantics contract chosen and documented | **PASS** — H3, plus two regression tests (H5) |
| Production behaviour unchanged | **PASS** — no production diff at all (H7) |
| No test tag added to production UI | **PASS** — H3 |
| Targeted `AppPickerUiTest` | **PASS** — 18/18 |
| Targeted `NotificationConsentUiTest` | **PASS** — 23/23 |
| **Full `connectedDebugAndroidTest`, one unfiltered run** | **PASS** — 84 tests, 0 failures, 0 errors, 0 skipped |
| `skipped = 0` earned, not assumed | **PASS** — H9 |
| `NotificationHardwareGateTest` | **PASS** — 10/10, skipped=0 |
| NOTIF-SEC-25 | **PASS** — 8/8, skipped=0 |
| Android persistence sweep | **PASS** — preserved from G6, not stale |
| Desktop persistence sweep | **PASS** — preserved from G7, not stale |
| Full user-driven E2E | **PASS** — 14/14, preserved from F7 |
| Android JVM 447/0 · Rust 590/0 | **PASS** — preserved from G11, no source changed |
| `notifications_v1.proto` unchanged | **PASS** — H11 |
| N4 runtime absent | **PASS** — H11 |
| No newly introduced product defect | **PASS** — no product change was made to introduce one |

The one gate that failed in G10 now passes, and it passes without the product
moving. The 14 failures were what G10 said they were: assertions written
against a semantics tree the product does not have, in two files that had never
run on a device. Correcting them meant deciding what the tree *should* be — a
merged row that speaks, with a real control inside it — writing the assertions
against that, and pinning the decision so the next person does not have to
rediscover it from a failure dump.

## NOTIFICATIONS.V1 N3 PASS
## N4 READY FOR IMPLEMENTATION
