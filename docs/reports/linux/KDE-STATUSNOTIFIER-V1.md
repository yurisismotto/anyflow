# AnyFlow — KDE StatusNotifier v1

**Branch** `feature/kde-statusnotifier-v1`
**Base** `develop` after merged PR #36 (Android Branding + Files UX)
**Host** Fedora 44, GNOME 50.4 / Wayland, rustc 1.98.0
**Date** 2026-09-18
**Status** not committed, not pushed, no PR opened

Native KDE Plasma tray integration, spoken directly to the session bus:
`org.kde.StatusNotifierItem` and `com.canonical.dbusmenu`, owned by
`anyflowd`, driving the GApplication actions `anyflow-gui` has exported since
the Quick Panel sprint.

**KDE STATUSNOTIFIER V1: IMPLEMENTED — PHYSICAL KDE CERTIFICATION BLOCKED**

There is no KDE Plasma session on this machine; none was installed and none was
simulated. Everything else — the whole protocol, the whole watcher lifecycle,
the tray's identity and D-Bus metadata, and the cold GUI activation the tray
depends on — was measured on a real session bus against a spec-faithful
watcher, and the deterministic suite covers the rest.

---

## 1. Baseline

```
git branch --show-current   feature/kde-statusnotifier-v1
git status --short          ?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
git log -5 --oneline        0b2e319 Merge pull request #36 …android-branding-files-ux-v1
                            0b82581 feat(android): add Flow A branding and Files UX
                            d48e86e Merge pull request #35 …android-clipboard-truthfulness-v1
                            96f004c fix(clipboard): make Android delivery feedback truthful
                            1c6e09f Merge pull request #34 …revoked-device-cleanup-v1
git diff --check            clean
```

`git merge-base --is-ancestor 0b82581 HEAD` → **PR #36 is in ancestry.**
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not read, written or staged.

---

## 2. Current GUI lifecycle, as audited

`desktop/gui/src/lib.rs` documents, and the running binary confirms:

* one `adw::Application`, id `io.github.yurisismotto.anyflow`, flags
  `HANDLES_COMMAND_LINE`;
* two surfaces — Quick Panel (`panel/`) and Settings (`views/`) — that are two
  windows of one process, neither owning the other;
* *"the agent is `anyflowd` and stays the only long-lived process AnyFlow
  runs."*

That last line is the constraint this sprint had to honour, and it is why the
design below puts the tray in the daemon. `anyflowd` is a `systemd --user`
service holding the TCP listener, the mDNS record, the trust store and every
capability; the GUI is a thing a person opens.

Measured, rather than read: started bare, `anyflow-gui` owns the bus name,
exports one window object, and exits when it goes idle in service mode
(§6).

---

## 3. The GApplication action seam, introspected

With `anyflow-gui` running:

```console
$ gdbus introspect --session --dest io.github.yurisismotto.anyflow \
      --object-path /io/github/yurisismotto/anyflow
  interface org.gtk.Actions        List / Describe / DescribeAll / Activate / SetState
  interface org.gtk.Application    Activate / Open / CommandLine
  interface org.freedesktop.Application
        Activate(a{sv})
        Open(as, a{sv})
        ActivateAction(s action-name, av parameter, a{sv} platform-data)

$ … org.gtk.Actions.DescribeAll
({'quick-panel': (true, signature '', @av []),
  'settings':    (true, '', []),
  'transfers':   (true, '', [])},)
```

| Question | Answer |
|---|---|
| **A.** can another same-user process activate `app.quick-panel`? | **Yes.** `ActivateAction("quick-panel", [], {})` raised the Quick Panel; a second window object appeared under `/…/window/` and the process count stayed at **one** |
| **B.** Settings? | **Yes** |
| **C.** Transfers? | **Yes** |
| **D.** what if `anyflow-gui` is not running? | before this sprint: `org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable` |
| **E.** is `DBusActivatable=true` enough for a cold start? | **No** — see D. It is a promise read by things that launch *desktop entries*; the message bus reads a *service file* |
| **F.** did a `.service` activation file exist? | **No.** Nothing matching `anyflow` in `~/.local/share/dbus-1/services` or `/usr/share/dbus-1/services` |
| **G.** what metadata is required? | `[D-BUS Service]` with `Name=` and an **absolute** `Exec=` |

All three actions are parameterless (`signature ''`), which is what lets a tray
call them by name without knowing a variant type — a property the GUI's own
documentation says it preserved for exactly this caller.

---

## 4. Tray owner decision

**`anyflowd` owns the StatusNotifierItem.** Implemented in `anyflow-linux`
(`desktop/platform-linux/src/tray/`), the Linux desktop-shell adapter.

```
anyflowd  ──owns──►  org.kde.StatusNotifierItem-<pid>-1
 (always)              /StatusNotifierItem   (SNI)
                       /MenuBar              (DBusMenu)
                             │ click
                             ▼
                    session D-Bus → io.github.yurisismotto.anyflow
                                      org.freedesktop.Application.ActivateAction
                                             │
                                    anyflow-gui — started by the bus if it is
                                    not running, gone again when idle
```

Proven on the live session bus: the name
`org.kde.StatusNotifierItem-2692440-1` is owned by pid 2692440, which
`busctl --user list` names `anyflowd`, and the `ActivateAction` calls the tray
makes are sent by that same unique name (§19).

The rejected alternative was keeping `anyflow-gui` resident to hold a GTK tray
icon. It is easier and it would have made AnyFlow a product with two resident
processes, one of which exists only to draw an icon — reversing a stated
architectural position as a side effect of a UI feature.

No second daemon, no hidden GTK window, no autostart copy of `anyflow-gui`, no
GTK or KDE dependency anywhere near `anyflowd`.

---

## 5. Why the GUI stays non-resident

Because `--gapplication-service` makes the bus start it *for a window* and
GApplication ends it when there is no window left. Measured on this build:

```console
$ ./target/debug/anyflow-gui --gapplication-service &
$ busctl --user list | grep anyflow
io.github.yurisismotto.anyflow   2672702  anyflow-gui …          # name owned
$ gdbus introspect … /io/github/yurisismotto/anyflow/window
node                                                              # no windows
$ sleep 12; pgrep anyflow-gui
(gui exited — inactivity timeout)
```

Ten seconds of idleness and it is gone. Across every scenario in §19 and §20 —
four cold activations, three menu rows, repeated clicks — the process count
never exceeded **one**, and after `pkill -x anyflow-gui` the daemon, the item
and the registration were all still there and a further click started a fresh
GUI.

---

## 6. Cold D-Bus activation audit

The gap and the fix, both measured.

**Before** — no service file, no cold start:

```console
$ pkill -x anyflow-gui
$ gdbus call --session --dest io.github.yurisismotto.anyflow … ActivateAction quick-panel '[]' '{}'
Error: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable
```

**After** — with `io.github.yurisismotto.anyflow.service` installed:

```console
$ pgrep -x anyflow-gui          # nothing
$ gdbus call … ActivateAction quick-panel '[]' '{}'
()
$ pgrep -a anyflow-gui
2677193 /home/yuri/.local/bin/anyflow-gui --gapplication-service
$ ps -o ppid= -p 2677193        # 4428 — the systemd user manager
$ cat /proc/2677193/cgroup
0::/user.slice/…/app.slice/dbus-:1.2-io.github.yurisismotto.anyflow@0.service
   → window: 'AnyFlow'          # the Quick Panel, and only that
```

Two findings worth recording:

* **the bus does not watch its service directories.** Fedora 44 runs
  dbus-broker; immediately after the file was written the activation still
  failed, and the identical call succeeded the moment `ReloadConfig` was sent.
  The installer now sends it, or a development install would silently work
  only after the next login.
* **`--gapplication-service` is load-bearing.** Without it the bus starts the
  binary with no arguments — a bare launch, which opens Settings — and then the
  activation message arrives and opens the Quick Panel as well. Someone who
  clicked "Quick Panel" would get two windows. With it, the cold start opened
  exactly one, and it was the right one.

`settings` and `transfers` were cold-activated the same way, each producing one
`AnyFlow Settings` window and one process.

---

## 7. Activation metadata changes

One new file and one changed script. No identity string was invented; the one
that already existed is reused.

**`desktop/gui/data/io.github.yurisismotto.anyflow.service.in`**

```ini
[D-BUS Service]
Name=io.github.yurisismotto.anyflow
Exec=@BINDIR@/anyflow-gui --gapplication-service
```

A template, because a service file's `Exec` must be absolute and the only
honest source of an absolute path is the install prefix. **Nothing about
AnyFlow's identity is derived** — the bus name is the same literal as
everywhere else; only the directory is substituted.

**`desktop/gui/tools/install-desktop-metadata.sh`** now installs it into
`$prefix/share/dbus-1/services/`, substituting `@BINDIR@` from **`$prefix`**
and never from `$destdir`; removes it on `--uninstall`; and sends
`ReloadConfig` in live-session mode only.

Both prefixes were exercised:

```console
# development
$ ./install-desktop-metadata.sh
installed ~/.local/share/dbus-1/services/io.github.yurisismotto.anyflow.service
Exec=/home/yuri/.local/bin/anyflow-gui --gapplication-service

# packaging-style, into a staging root
$ ./install-desktop-metadata.sh --prefix /usr --destdir <tmp>
<tmp>/usr/share/applications/io.github.yurisismotto.anyflow.desktop
<tmp>/usr/share/dbus-1/services/io.github.yurisismotto.anyflow.service
<tmp>/usr/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
Exec=/usr/bin/anyflow-gui --gapplication-service
grep -r 'Sandbox|/home/yuri|<tmp>' <tmp>/ → clean
```

`/home/yuri` appears in the development install because the development prefix
*is* `~/.local`; it cannot appear in a packaged one, and the test refuses an
`Exec` containing `/home/`, `Sandbox`, `target/debug`, `..` or `~` in the
template itself.

Alignment is pinned by tests in both crates (§17, §21): the bus name, the
desktop entry's basename and `Icon=`, the service file's basename and `Name=`,
the icon in `hicolor`, the GtkApplication `APP_ID` and the tray's `IconName`
are one string, and the three action names are read out of `anyflow-gui`'s own
public constants rather than restated.

---

## 8. StatusNotifier spec evidence

Derived from the files the shell's own code is generated from, fetched during
this sprint and read — not recalled.

| Source | What it settled |
|---|---|
| `frameworks/kstatusnotifieritem/src/org.kde.StatusNotifierItem.xml` | every item property, method and signal, with signatures |
| `frameworks/kstatusnotifieritem/src/org.kde.StatusNotifierWatcher.xml` | the watcher interface |
| `frameworks/kstatusnotifieritem/src/kstatusnotifieritemdbus_p.cpp` | the item's bus-name shape and object path |
| `frameworks/kstatusnotifieritem/src/kstatusnotifieritem.cpp` | registration, `/MenuBar`, `QDBusServiceWatcher` on owner change |
| `frameworks/kstatusnotifieritem/src/kstatusnotifieritem.h` | the `ItemStatus` and `ItemCategory` enum spellings |
| `plasma/plasma-workspace/statusnotifierwatcher/statusnotifierwatcher.cpp` | what Plasma's watcher actually does with a registration |
| `plasma/plasma-workspace/libdbusmenuqt/com.canonical.dbusmenu.xml` | the menu interface Plasma is generated from |
| `plasma/plasma-workspace/libdbusmenuqt/dbusmenuimporter.cpp` | which menu calls Plasma actually makes |
| `plasma/plasma-workspace/applets/systemtray/systemtraymodel.cpp` | that `Id` is the tray's persistent configuration key |

### The contract

```
watcher bus name       org.kde.StatusNotifierWatcher
watcher object path    /StatusNotifierWatcher
registration           RegisterStatusNotifierItem(s service)
                       — a leading '/' means "object path, service = sender";
                         anything else means "bus name, path = /StatusNotifierItem"
watcher properties     RegisteredStatusNotifierItems as
                       IsStatusNotifierHostRegistered b
                       ProtocolVersion i
watcher signals        StatusNotifierItemRegistered(s)
                       StatusNotifierItemUnregistered(s)
                       StatusNotifierHostRegistered() / …Unregistered()

item bus name          org.kde.StatusNotifierItem-<pid>-<n>
item object path       /StatusNotifierItem
item interface         org.kde.StatusNotifierItem
properties             Category s · Id s · Title s · Status s · WindowId i
                       IconThemePath s · Menu o · ItemIsMenu b
                       IconName s · IconPixmap a(iiay)
                       OverlayIconName s · OverlayIconPixmap a(iiay)
                       AttentionIconName s · AttentionIconPixmap a(iiay)
                       AttentionMovieName s · ToolTip (sa(iiay)ss)
methods                ProvideXdgActivationToken(s)
                       ContextMenu(ii) · Activate(ii) · SecondaryActivate(ii)
                       Scroll(i s)
signals                NewTitle · NewIcon · NewAttentionIcon · NewOverlayIcon
                       NewMenu · NewToolTip · NewStatus(s)

menu property          Menu = /MenuBar   (KDE uses /NO_DBUSMENU when absent)
menu interface         com.canonical.dbusmenu
properties             Version u · Status s
methods                Event(i s v u) · GetProperty(i s)→v
                       GetLayout(i i as)→(u, (ia{sv}av))
                       GetGroupProperties(ai as)→a(ia{sv})
                       AboutToShow(i)→b
signals                ItemsPropertiesUpdated(a(ia{sv}) a(ias))
                       LayoutUpdated(u i) · ItemActivationRequested(i u)
```

Two behaviours of Plasma's watcher shaped the implementation:

* **it verifies.** `RegisterStatusNotifierItem` builds a proxy at the item's
  path and calls `isValid()`, which introspects. So the object is exported
  **before** the bus name is requested, closing the window in which an item
  could be rejected for being empty. The fake watcher in the test suite does
  the same check, so a regression here fails a test rather than a home screen.
* **it de-duplicates.** A repeat registration of the same service and path is a
  no-op. That is a safety net, not a licence — a shell that *did* honour it
  would show two AnyFlow icons — so the implementation registers once per
  watcher owner (§13).

### Compliance is asserted, not asserted-to

Two tests introspect the live objects and compare **every** member and
signature against the tables above:

```
the_status_notifier_item_interface_is_the_one_kde_declares      ok
the_menu_interface_is_the_one_plasmas_importer_is_generated_from ok
```

`EventGroup` and `AboutToShowGroup` are **not** implemented: they are not in
the file Plasma is generated from and no Plasma caller would use them, so
implementing them would be implementing a guess.

---

## 9. Item identity and properties

Read off the live object on the session bus:

```console
$ gdbus introspect --session --dest org.kde.StatusNotifierItem-2692440-1 \
      --object-path /StatusNotifierItem --only-properties
      readonly s  Category   = 'ApplicationStatus'
      readonly s  Id         = 'io.github.yurisismotto.anyflow'
      readonly s  Title      = 'AnyFlow'
      readonly s  Status     = 'Active'
      readonly s  IconName   = 'io.github.yurisismotto.anyflow'
      readonly a(iiay) IconPixmap = []
      readonly s  IconThemePath = ''
      readonly o  Menu       = '/MenuBar'
      readonly b  ItemIsMenu = false
      readonly i  WindowId   = 0
      readonly (sa(iiay)ss) ToolTip = ('', [], 'AnyFlow', 'One flow. Any device.')
      readonly s  OverlayIconName = ''        AttentionIconName = ''
      readonly s  AttentionMovieName = ''     (all pixmap arrays empty)
```

* **`Id` is the application id.** Plasma's system tray uses `Id` as the
  configuration key for whether the user has pinned or hidden an item
  (`systemtraymodel.cpp` reads it into `m_shownItems` / `m_hiddenItems`), so it
  must be the same string on the next login and it must be ours alone — a
  collision means inheriting somebody else's "hidden".
* **`Title` is the product name**, because a person reads it.
* **`Category` is `ApplicationStatus`.** `SystemServices` is tempting —
  `anyflowd` genuinely is a background service — but the category describes the
  *icon*, and this icon is the entry point to an application with windows,
  which is exactly what KDE documents `ApplicationStatus` as.
* **`ItemIsMenu` is false**, so a left click is the Quick Panel and not the
  menu. AnyFlow has a meaningful primary action; `true` is for items that do
  not.
* **`WindowId` is 0.** The item belongs to a daemon, which has no window.
  Claiming an id would point the shell at somebody else's.

One tray item per daemon process, named `org.kde.StatusNotifierItem-<pid>-<n>`
exactly as KDE's own client names its items. D6 asserts the watcher sees
exactly one and that the name has that shape; a daemon restart produced one
unregistration and one registration, never two live items (§13).

---

## 10. Icon strategy

`IconName` only. No `IconPixmap`, no `IconThemePath`.

The name resolves through the session's own theme, which is what makes the
Flow A crisp at whatever size and scale the shell is drawing without AnyFlow
knowing anything about the display. Verified through a real theme lookup:

```console
$ Gtk.IconTheme.has_icon("io.github.yurisismotto.anyflow")            → True
$ …lookup_icon(…).get_file().get_path()
  /home/yuri/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
$ cmp <that file> docs/design/assets/app-icon.svg                     → identical
```

The installed icon is byte-identical to the canonical `app-icon.svg`, whose
path data begins `M 53 56 C 50 41 43 22 32 8 …` — the Flow A the desktop brand
test pins. The tray does not know where that file is, and the icon was not
altered to suit it.

A pixmap would be a second copy of the mark, rasterised at a size guessed by
the sender, that the brand documentation does not know exists. **No KDE
evidence exists that `IconName` is insufficient** when the metadata is
correctly installed, and a suspicion is not evidence, so none was added.

What this section establishes is that AnyFlow *asks for the right icon by the
right name, and that the name resolves* — the identity half of
`FLOW A TRAY IDENTITY`, and it is PASS. It does not establish that Plasma draws
that icon in a system tray; no Plasma session was available to draw it, so that
half stays BLOCKED.

---

## 11. DBusMenu model

```
root (id 0)   children-display = submenu
 ├ 1  Quick Panel   enabled, visible
 ├ 2  Files         enabled, visible
 └ 3  Settings      enabled, visible
```

Live, over the session bus:

```console
$ gdbus call … --object-path /MenuBar --method com.canonical.dbusmenu.GetLayout 0 1 "[]"
(uint32 1, (0, {'children-display': <'submenu'>},
  [<(1, {'enabled': <true>, 'label': <'Quick Panel'>, 'visible': <true>}, @av [])>,
   <(2, {'enabled': <true>, 'label': <'Files'>,       'visible': <true>}, @av [])>,
   <(3, {'enabled': <true>, 'label': <'Settings'>,    'visible': <true>}, @av [])>]))

$ … GetAll com.canonical.dbusmenu   →  {'Version': <uint32 3>, 'Status': <'normal'>}
```

**The ids are literals.** Deriving an id from a position in the array would
compile, would work, and would silently rewire every id the moment somebody
inserted a row — while a shell that had already fetched the layout went on
sending the old ones. T9 pins `[(1, "Quick Panel"), (2, "Files"), (3,
"Settings")]` as a golden, T10 asserts the source never indexes the menu by
position, and T8 asserts no row claims the root's id.

An unknown id is refused safely: `Event` on a made-up id does nothing at all,
and `GetLayout` on one returns `InvalidArgs` rather than a plausible empty menu
— a host that got an empty menu back for an invented id would have been told
something false.

`Status` is `normal`, always. The other defined value, `notice`, asks the host
to make the menu prominent; it is the menu's version of `NeedsAttention` and is
refused for the same reason.

What is deliberately **absent**: Pair, Grant, Revoke, Send clipboard, Send
file, Quit. The first five carry authority — they decide who may read this
machine, and a decision that large belongs on a surface where the person can
see what they are deciding about. "Quit AnyFlow" is absent for a different
reason: the only thing it could honestly quit is `anyflowd`, and stopping the
continuity service from a tray menu is not closing a window, it is turning the
product off. A test refuses a menu label containing any of those words.

---

## 12. Tray action model

```rust
pub enum TrayAction { QuickPanel, Files, Settings }   // no payload, 1 byte

TrayAction::QuickPanel.gapplication_action() == "quick-panel"
TrayAction::Files     .gapplication_action() == "transfers"
TrayAction::Settings  .gapplication_action() == "settings"
```

**No action name can come from a D-Bus message.** The shell sends an integer
and a verb; the verb is compared against one constant and dropped, the integer
is looked up in a table of three, and the name that reaches
`ActivateAction` is a `&'static str` from a `match` over three variants. T7
asserts `size_of::<TrayAction>() == 1` — a variant carrying a `String` could not
be one byte — and that the produced name set is exactly those three.

`Files` maps to `transfers` because those are the two surfaces' own names: the
row a person reads says "Files", and the action `anyflow-gui` has exported
since the Quick Panel sprint is `transfers`. The enum exists so neither has to
change to suit the other; a test reads `ACTION_TRANSFERS` out of the GUI's
source rather than restating it.

### Interaction

| Gesture | Behaviour |
|---|---|
| left click / `Activate` | Quick Panel. Always, with no state consulted; the coordinates are ignored, because a daemon telling a window manager where to put a window is how an application places itself wrongly on the one monitor arrangement nobody tested |
| right click | Plasma draws the DBusMenu at `/MenuBar` itself |
| `ContextMenu(x,y)` | **no-op.** Plasma does not call it; the only alternative is a daemon popping up a window of its own, which needs a GUI toolkit in `anyflowd` |
| `SecondaryActivate` (middle click) | **no-op, deliberately.** The specification does not say what it means and shells disagree. The honest options were "open Settings" and "do nothing", and choosing the first would mean shipping an interaction no real KDE session has ever run. Recorded as a debt, not finished |
| `Scroll` | **no-op.** A wheel event is something a pointer does on the way past. AnyFlow has no small reversible continuous quantity to bind it to — everything it could change is a device, a grant or a transfer |
| `ProvideXdgActivationToken` | stored, forwarded once as `platform-data["activation-token"]`, then **taken**: a spent token would be a stale credential on the next click |

All three no-ops were driven over the real bus and started nothing (§20).

---

## 13. Watcher lifecycle

Purely event-driven. `NameOwnerChanged`, filtered to one name, and nothing
else: **no timer, no poll, no backoff, no retry counter** — asserted by test
over all five tray source files.

```
subscribe to NameOwnerChanged(org.kde.StatusNotifierWatcher)   ← before the first look
GetNameOwner                                                    ← the race is closed
  owned    → register, remember the owner's unique name
  no owner → one info line, and wait

owner appears      → register, unless already registered with that same owner
owner disappears   → forget it; the item stays published and named
owner replaced     → register with the new one, because the remembered owner differs
```

Holding the **owner** rather than a boolean is what makes the replacement case
work: a shell can be replaced in a single `NameOwnerChanged` where both the old
and the new owner are non-empty, and a boolean would read that as "still
registered" while the new shell never heard of this item.

On the live session bus, with a spec-faithful watcher started and killed three
times:

```
INFO tray: AnyFlow tray item published on the session bus item=org.kde.StatusNotifierItem-2691111-1
INFO tray::watcher: registered an AnyFlow tray item with the desktop shell
INFO tray::watcher: the desktop tray host went away; the AnyFlow tray item stays published …
INFO tray::watcher: registered an AnyFlow tray item with the desktop shell
INFO tray::watcher: the desktop tray host went away; …
INFO tray::watcher: registered an AnyFlow tray item with the desktop shell
INFO tray::watcher: the desktop tray host went away; …
INFO tray::watcher: registered an AnyFlow tray item with the desktop shell
```

One registration per shell, one line each. Eight idle seconds with no host
produced **zero** further lines.

Daemon restart, watched by a watcher that implements the unregistration half:

```
REGISTERED    org.kde.StatusNotifierItem-2690778-1/StatusNotifierItem
UNREGISTERED  org.kde.StatusNotifierItem-2690778-1/StatusNotifierItem
REGISTERED    org.kde.StatusNotifierItem-2691111-1/StatusNotifierItem
RegisteredStatusNotifierItems → ['org.kde.StatusNotifierItem-2691111-1/StatusNotifierItem']
```

One item, recreated — not two.

---

## 14. Task supervision

```rust
let _tray = anyflow_linux::tray::spawn(ActivatorChoice::SessionBus);
…
tokio::select! {
    r = net => r??,                 // the network listener
    r = ctl => r??,                 // the control server
    _ = tokio::signal::ctrl_c() => …
}
```

**The tray is not in that `select!`.** Everything in it is load-bearing and the
first to finish ends the process; a tray icon is not in that class. If the
session has no tray host, or the shell restarts, or the item cannot be
published at all, the right outcome is a log line and a daemon that goes on
moving files. A test parses the daemon's `select!` block and fails if "tray"
appears inside it.

A panic is not silent either. A bare `tokio::spawn` swallows a panic into a
`JoinHandle` nobody awaits, so `spawn` runs a supervisor task whose only job is
to await the inner handle and say so if it died that way. "The tray stopped
working and nothing was written down" is the failure mode that costs one task
to remove.

The session bus going away ends the follow loop normally; the supervisor logs
once and returns. Nothing reconnects, because a session bus that has gone away
is a session that is ending.

---

## 15. systemd sandbox review

Validated under a unit whose hardening is **byte-identical** to
`packaging/fedora/anyflowd.service` (`diff` of everything but `Description` and
`ExecStart` — empty), run as a throwaway `systemd --user` unit and removed
afterwards. No repository file was changed and no hardening directive was
weakened.

```
daemon   cgroup /user.slice/…/app.slice/anyflowd-sandboxprobe.service
         NoNewPrivs: 1   Seccomp: 2 (filtered)
         ProtectHome=read-only  ProtectSystem=strict  MemoryDenyWriteExecute=yes
         RestrictAddressFamilies=AF_INET AF_INET6 AF_NETLINK AF_UNIX

INFO tray: AnyFlow tray item published on the session bus item=…-2691970-1
INFO tray::watcher: registered an AnyFlow tray item with the desktop shell
```

Then one tray click, from inside that sandbox:

```
gui pid 2692122   ppid 4428  ← the systemd user manager, NOT anyflowd
gui cgroup /user.slice/…/app.slice/dbus-:1.2-io.github.yurisismotto.anyflow@9.service
gui NoNewPrivs: 0   Seccomp: 0        ← the sandbox is NOT inherited
pstree anyflowd → threads only, no child processes at all
window: 'AnyFlow'
```

That is the whole of §4 of the brief demonstrated in one measurement. Nothing
is spawned: a test forbids `Command::new`, `std::process::Command`, `sh -c`,
`xdg-open` and `gtk-launch` anywhere in the tray.

| Requirement | Result |
|---|---|
| session D-Bus reachable under the unit | **yes** — `AF_UNIX` was already allowed; nothing was added |
| SNI registration works under the unit | **yes** |
| GUI activation is not a sandboxed child | **yes** — different parent, different cgroup, `NoNewPrivs: 0` |
| extra filesystem write required by the tray | **none** |
| new syscall / address family required | **none** |
| hardening weakened | **none** |

### A pre-existing packaging defect found on the way

`packaging/fedora/anyflowd.service` **cannot bind the control socket as
written**:

```
Error: could not bind the local control endpoint: Read-only file system (os error 30)
anyflowd.service: Main process exited, code=exited, status=1/FAILURE
```

`ProtectSystem=strict` makes `$XDG_RUNTIME_DIR` read-only and the unit grants
no writable path there, so `$XDG_RUNTIME_DIR/anyflow/control.sock` cannot be
created. The fix is `RuntimeDirectory=anyflow` (plus `ReadWritePaths=%t/anyflow`),
which is a *grant*, not a weakening — every hardening directive stays.

**Not fixed here.** It has nothing to do with the tray, packaging is explicitly
a later sprint, and §33 forbids widening the branch. The probe unit carried
that one added line so the sandbox could be measured at all; the repository's
unit is untouched. Recorded as debt 1.

---

## 16. Dependency review

**No new crate.** `cargo tree` and the lock file both say so — the diff to
`desktop/Cargo.lock` is **two lines**, and neither adds a `[[package]]`:

```diff
 [[package]]
 name = "anyflow-linux"
 dependencies = [
+ "futures-lite",
+ "zbus",
```

`zbus 5.19.0` and `futures-lite 2` were already resolved and already built for
`anyflowd`: `anyflow-capability-notifications` takes zbus for the freedesktop
notification server and logind, `anyflow-capability-battery` takes it for
UPower, and the daemon enables both. Making the edge direct in `anyflow-linux`
is architectural honesty — the crate really does use a D-Bus client now — not
new supply-chain surface. Same options as the existing two:
`default-features = false, features = ["tokio"]`, because zbus's own executor
would be a second runtime inside a process that already has one.

`ksni` and every other tray crate was **not** considered necessary and none was
added: zbus was already there, the protocol is two interfaces, and the whole
implementation is 1,517 lines including its documentation.

### The feature boundary is real, not decorative

`anyflow-linux` gained `tray`, on by default; `anyflow-gui` and `anyflow-cli`
set `default-features = false`. Measured per crate:

```
cargo tree -p anyflow-gui    -e normal | grep -c zbus   → 0
cargo tree -p anyflow-cli    -e normal | grep -c zbus   → 0
cargo tree -p anyflow-daemon -e normal | grep -c zbus   → 6
cargo tree -p anyflow-daemon … | grep anyflow-linux     → default,tray
```

Built alone, the GUI and the CLI pull no zbus at all. (A whole-workspace build
unifies features and compiles `anyflow-linux` once with `tray` on; that is how
unification works and it is not what the declaration is for.)

---

## 17. Pure tests — `tray_model.rs`, 15 tests

| # | Test |
|---|---|
| T1 | a primary click is the Quick Panel and only that — the function takes no argument, so there is no state it could consult |
| T2–T4 | each menu row maps to the surface it names, and to the right `GAction` |
| T5 | an unknown menu id resolves to no action — including the root, `i32::MIN` and `i32::MAX` |
| T6 | only `clicked` is a decision — `hovered`, `opened`, `closed`, `x-kde-*`, `"Clicked"`, `"clicked "` and `""` all do nothing |
| T7 | the action type has no room for a name from the bus — `size_of == 1`, a closed name set, and a source check |
| T8 | menu ids are unique and none is the root's |
| T9 | menu ids are the values they have always been (golden) |
| T10 | the layout is deterministic and never indexed by position |
| T11 | every string the tray can publish is one of eight constants; the model mentions no peer, fingerprint, clipboard, filename, device name, battery or notification, and the single word "transfer" appears only as the action name |
| T12 | the icon name and the item id are the application id, and the icon name is not a path, a URL or a filename |
| T13 | the status is `Active`; `NeedsAttention` does not appear outside a comment explaining why |
| T14 | `Scroll`, `SecondaryActivate` and `ContextMenu` have literally empty bodies |
| T15 | nothing about the menu depends on an activation having worked — `TrayMenu` holds exactly one field |
| T16 | no tray decision is made by a label, a fingerprint, a peer or a position |
| — | the menu is exactly the three rows, and no label carries authority or stops the daemon |

Plus three unit tests in the crate: the object path is the application id
mechanically, an obviously wrong activation token is not forwarded, and both
object paths parse.

---

## 18. Private D-Bus integration tests — `tray_dbus.rs`, 19 tests

Every test raises its **own** `dbus-daemon` from a configuration written by the
test, with **no service directories at all**. Nothing in these tests can reach
the developer's session, and above all nothing can start the real
`anyflow-gui`; a test that expected D-Bus activation to rescue it fails rather
than quietly succeeding for the wrong reason.

The fake watcher serves `org.kde.StatusNotifierWatcher` with the signatures
from KDE's own XML and — like Plasma — **verifies** a registration by reading
the item's properties back over a second connection before accepting it. An
item that owned its name but exported no object would be refused here exactly
as Plasma refuses it.

| # | Test | Result |
|---|---|---|
| D1 | registers when the watcher is already there | **PASS** — and the watcher recorded `<name>/StatusNotifierItem`, so the bus-name branch was taken |
| D2 | an absent watcher is not an error — the item still answers, the task still lives | **PASS** |
| D3 | a watcher that appears later gets a registration | **PASS** |
| D4 | the watcher going away does not take the item with it | **PASS** — still owned, still `Active` |
| D5 | a shell that comes back gets exactly one new registration, and it stays one | **PASS** |
| D6 | the watcher sees exactly one AnyFlow item, named `org.kde.StatusNotifierItem-<pid>-<n>` | **PASS** |
| D7 | every item property has the type and value the spec requires — sixteen signatures, checked through `GetAll` | **PASS** |
| D8 | a left click asks for the Quick Panel and nothing else; coordinates change nothing; `SecondaryActivate`, `ContextMenu` and eight `Scroll` calls add nothing | **PASS** |
| D9–D11 | each menu row opens the surface it names — ids read back out of the layout, as Plasma does | **PASS** |
| D12 | six invented ids and six wrong verbs do nothing, and a real click still works afterwards | **PASS** |
| D13 | a shell that announces itself repeatedly is registered with once | **PASS** |
| D14 | with no watcher, the item sends **nothing at all** — a bus monitor recorded zero messages from it in two seconds | **PASS** |
| D14b | the source contains no `sleep`, `interval`, `Duration::from`, `retry`, `backoff` or `loop {` | **PASS** |
| D15 | every string on both objects is one of fifteen constants, and none looks like a path or an address | **PASS** |
| — | an activation token is forwarded once and then forgotten | **PASS** |
| — | an activator that always fails changes nothing: still registered, same rows, same ids, still `Active`, every click still attempted | **PASS** |
| — | the rest of the menu protocol Plasma uses: `Version`, `Status`, `AboutToShow`, depth 0, a refused parent, filtered and unfiltered `GetGroupProperties`, `GetProperty` | **PASS** |
| — | the item interface is the one KDE declares (28 members, exact signatures) | **PASS** |
| — | the menu interface is the one Plasma's importer is generated from (10 members) | **PASS** |

---

## 19. GApplication activation tests

Driven against the **real** session bus with the real daemon, and read off the
bus rather than out of a log. `dbus-monitor` on
`interface='org.freedesktop.Application'` while the three menu rows and one
`Activate` were invoked:

```
sender=:1.1480 -> destination=io.github.yurisismotto.anyflow  member=ActivateAction
   string "quick-panel"        ← menu id 1
   string "transfers"          ← menu id 2
   string "settings"           ← menu id 3
   string "quick-panel"        ← Activate

$ busctl --user list | grep :1.1480
:1.1480  2689451  anyflowd  …
org.kde.StatusNotifierItem-2689451-1  2689451  anyflowd  …
```

The sender is `anyflowd`, and it is the same connection that owns the item.

**A. GUI already running** — `ActivateAction("quick-panel")` produced a second
window object under `/…/window/` and the AT-SPI window list showed
`AnyFlow Settings` and `AnyFlow` side by side, with `pgrep -c anyflow-gui`
still **1**. No second resident instance. Same for `settings` and `transfers`.

**B. GUI not running** — every one of the four activations above was performed
with **zero** `anyflow-gui` processes beforehand; each started one through
D-Bus activation, in its own transient systemd unit, and opened exactly one
window: `AnyFlow` for `quick-panel`, `AnyFlow Settings` for `settings` and for
`transfers`.

**The daemon and the tray survive the GUI.** After `pkill -x anyflow-gui`:
the daemon is alive, `org.kde.StatusNotifierItem-2689451-1` is still owned, the
watcher still lists it, and a further `Activate` started a fresh GUI.

Both prefixes were exercised for the metadata (§7).

**The Files chain, link by link.** This is what `FILES ACTIVATION` passes on:

```
DBusMenu Event(2, "clicked")      measured — the row labelled "Files"
    -> TrayAction::Files          a table of three literal ids (T2–T4, D10)
    -> GApplication "transfers"   measured — dbus-monitor, string "transfers"
    -> Settings activated         measured — cold, one process, window observed
```

`ACTION_TRANSFERS` really is `"transfers"`: a test reads it out of
`anyflow-gui`'s own source rather than restating it.

**Recorded separately, and not part of that PASS.** Which *page* Settings
opened on could not be inspected: this session's accessibility tree exposes
only the frame of that window, so nothing could read the selected page. That
`app.transfers` opens Settings on `Page::Files` rests on the GUI's own
pre-existing unit tests. The window was observed; the page was not, and this
report claims nothing visual about it in either direction.

---

## 20. GNOME non-regression

Fedora 44, GNOME 50.4, Wayland. `org.kde.StatusNotifierWatcher` has no owner
on this session and never will.

| # | Check | Result |
|---|---|---|
| G1 | `anyflowd` starts normally | **PASS** — identity, UPower, files.v1, clipboard.v1, notifications.v1, all four capabilities, listener on 55432, control endpoint, mDNS |
| G2 | the absent watcher is non-fatal | **PASS** — one info line: *"no desktop tray host on this session; the AnyFlow tray item is published and will register itself if one appears"* |
| G3 | no busy loop, no warning spam | **PASS** — **two** tray lines in the whole run; 20 seconds of idling added none. D14 measured zero bus messages in two seconds |
| G4 | the GUI and Quick Panel still start normally | **PASS** — repeatedly, hot and cold |
| G5 | files / clipboard / notifications unaffected | **PASS** — the tablet (SM-X620) established a session mid-test with `battery.v1, clipboard.v1, files.v1`; `anyflow status` reports 5 paired devices |
| G6 | no fake tray icon is claimed to exist | **PASS** — the item is published on the bus, which is true, and the log says there is no host to show it |

No GNOME extension was added, and no AppIndicator path was implemented. That
is the next sprint.

---

## 21. KDE physical environment availability

```console
$ echo "$XDG_CURRENT_DESKTOP"                GNOME
$ echo "$XDG_SESSION_TYPE"                   wayland
$ busctl --user list | grep -i StatusNotifierWatcher
(nothing)
$ which plasmashell startplasma-wayland      (no plasma)
$ rpm -qa | grep -i plasma-workspace         (nothing)
$ virsh list --all                           (no VMs)
```

**No KDE Plasma session exists on this machine, and none was installed.**
`KDE REAL-SHELL CERTIFICATION = BLOCKED.` No Plasma session was installed,
and none was simulated to stand in for one: the fake watcher in §13 and §18
exercises the *protocol*, and a protocol peer is not a shell. Nothing in this
report treats it as evidence that Plasma renders anything.

---

## 22. KDE physical evidence

**None.** K1–K15 were not run: there is no Plasma shell to run them on.

What *was* certified on real hardware, against a spec-faithful watcher on the
real session bus, is every part of K1–K15 that is not Plasma drawing pixels:

| KDE gate | Status here |
|---|---|
| K1 daemon running, GUI not | **measured** (§19 B) |
| K2 tray item appears | **measured as a registration**, not as a picture |
| K3/K4 correct Flow A icon, not generic | **identity and metadata measured** — the name resolves through the real hicolor theme to the canonical `app-icon.svg` (§10). **Rendering BLOCKED**: whether Plasma draws it has not been seen |
| K5 left click opens the Quick Panel | **measured** |
| K6 cold-starts the GUI through D-Bus activation | **measured** |
| K7 GUI leaves, tray remains because the daemon owns it | **measured**, by killing the GUI rather than by closing a window |
| K8 the menu shows exactly the three rows | **measured** through `GetLayout` |
| K9 each row opens the right surface | **measured** (bus capture, §19) |
| K10 no duplicate long-lived GUI | **measured** — never more than one process |
| K11 daemon restart → one item | **measured** (§13) |
| K12 shell restart / re-registration | **measured**, three times |
| K13 no paired device still leaves a healthy item | **measured** — `Status` is a constant and consults nothing |
| K14 no file/clipboard/notification content in tooltip or menu | **measured** (D15, §23) |
| K15 no registration loop or errors in the journal | **measured** |

No screenshots were taken and none would have added anything: there is no
Plasma tray to photograph.

---

## 23. Privacy and logging audit

Everything the tray will put on the session bus, gathered by *asking* the live
objects rather than by listing what someone remembered:

```
ApplicationStatus · io.github.yurisismotto.anyflow · AnyFlow · Active
One flow. Any device. · Quick Panel · Files · Settings · submenu
label · enabled · visible · children-display · normal · ""
```

Fifteen strings, all compile-time constants. D15 reads both interfaces'
`GetAll` plus the entire menu layout, walks every nested value, and fails on
anything not in that list — and separately on anything containing `/home/`,
`/tmp/`, `content://`, `file://`, `://` or `192.168`.

**The tooltip is the tagline.** What a tooltip could usefully say — "connected
to Yuri's phone", "sending holiday-photos.zip" — is exactly what must not be
there: `ToolTip` is a public property of a public object on the session bus,
readable by every process in the session, at any time, without a click. The
tray is the one AnyFlow surface whose contents leave the process without
anyone asking, so the only safe contents are the ones that were already public.
**No device display name is on the bus at all.**

Logging: four lines exist, all `info`, and the only variable in any of them is
the tray's own bus name.

```
AnyFlow tray item published on the session bus  item=org.kde.StatusNotifierItem-<pid>-1
registered an AnyFlow tray item with the desktop shell  item=…
the desktop tray host went away; the AnyFlow tray item stays published …
no desktop tray host on this session; …
```

Two more can appear and carry a `GAction` name (one of three constants) and a
D-Bus **error name**: a zbus error's `Display` includes the message the far end
wrote, and the far end is another process on the session bus, so
`describe_bus_error` reduces it to the error name and nothing else.

Unchanged and unexamined by this sprint: TLS 1.3, SPKI pinning, the pairing
proof, the trust store, per-peer grants, clipboard privacy, notification
privacy, file path protections. No new permission, no new component, no new
exported network surface, no cloud, no telemetry. The trust store survived
every restart in this sprint — `anyflow status` reports the same 5 paired
devices, and the tablet reconnected on its own mid-run.

---

## 24. Portability impact

`anyflow-linux` is not in the portable set, and the portable set does not reach
it:

```
anyflow-proto / anyflow-core / anyflow-control /
anyflow-capability-{clipboard,files,battery,notifications}
    → occurrences of "anyflow-linux" in the dependency tree: 0, all seven
cargo check --locked --no-default-features -p <all seven>   → Finished
cargo tree -p anyflow-capability-notifications --no-default-features | grep -c zbus → 0
cargo tree -p anyflow-capability-battery       --no-default-features | grep -c zbus → 0
```

No SNI code can compile into the Windows portable set: it is in a crate that
set does not depend on, behind a feature, in a module the feature removes
entirely. `core/tests/portable_boundary.rs` needed no new exception, and
`portable-windows-msvc.yml` needed no change. The Windows guard was not
weakened.

---

## 25. Protocol impact

**None.** `protocol/` is untouched. No protobuf, no TLS framing, no capability
negotiation, no pairing, no `files.v1`, `clipboard.v1`, `battery.v1` or
`notifications.v1` change. The tray is a local Linux shell surface and asks no
peer for anything; everything it publishes is a constant in this repository.

`git status` on `protocol/` and `android/` is empty.

---

## 26. Files changed

**Modified (8)**

```
desktop/Cargo.lock                                two dependency-edge lines, no new package
desktop/platform-linux/Cargo.toml                 the `tray` feature; zbus + futures-lite
desktop/platform-linux/src/lib.rs                 the module, and why it lives here
desktop/daemon/src/main.rs                        one supervised spawn, outside the select!
desktop/gui/Cargo.toml                            default-features = false on anyflow-linux
desktop/cli/Cargo.toml                            default-features = false on anyflow-linux
desktop/gui/tools/install-desktop-metadata.sh     installs the activation entry; ReloadConfig
desktop/gui/tests/brand_assets.rs                 +3 tests for the activation metadata
```

**Added (10)**

```
desktop/gui/data/io.github.yurisismotto.anyflow.service.in     60
desktop/platform-linux/src/tray/mod.rs                        245
desktop/platform-linux/src/tray/model.rs                      252
desktop/platform-linux/src/tray/activate.rs                   200
desktop/platform-linux/src/tray/item.rs                       354
desktop/platform-linux/src/tray/menu.rs                       317
desktop/platform-linux/src/tray/watcher.rs                    149
desktop/platform-linux/tests/tray_model.rs                    471
desktop/platform-linux/tests/tray_dbus.rs                    1349
desktop/platform-linux/tests/tray_identity.rs                 211
```

`git diff --stat` reports **8 files, 315 insertions, 12 deletions** — tracked
files only, so it excludes the ten new ones. The sprint's real footprint is
**18**, and 19 with this report.

---

## 27. Remaining debts

1. **`packaging/fedora/anyflowd.service` cannot bind the control socket.**
   `ProtectSystem=strict` leaves `$XDG_RUNTIME_DIR` read-only and the unit
   grants nothing there, so `anyflowd` fails at start-up under it with
   `Read-only file system (os error 30)`. Pre-existing, found while validating
   the sandbox, **not fixed** — packaging is a later sprint and §33 forbids
   widening. The fix is `RuntimeDirectory=anyflow` plus
   `ReadWritePaths=%t/anyflow`, both grants rather than weakenings.
2. **`SecondaryActivate` is a no-op.** Middle click is unspecified and shells
   disagree. "Open Settings" was the plausible alternative and would have meant
   shipping an interaction no real KDE session has run. Revisit with Plasma in
   front of you.
3. **No KDE physical certification.** K1–K15 remain unrun. The deterministic
   and live-bus evidence covers the protocol and the lifecycle; it cannot cover
   Plasma drawing an icon, honouring `ItemIsMenu`, or placing the menu.
4. **`Status` is a constant.** Truthful for every state v1 has, and the design
   argues it should stay one — but if AnyFlow ever gains a state a tray should
   reflect, `NewStatus` is declared and never emitted, and that is the seam.
5. **GNOME shows no tray item.** By design here: GNOME has no
   `StatusNotifierWatcher` without an extension, and AppIndicator compatibility
   is explicitly the next sprint. Nothing in this branch pretends otherwise.
6. **The GUI's accessibility tree exposes only the frame** on this session, so
   "Settings opened on the Files page" could not be read visually (§19). The
   chain is proven link by link instead.

Recorded but untouched, per §33: `docs/design/BRAND.md` staleness, Android
lint, the incoming-file timeout shown as `Declined`, HelloAck
supported-vs-effective capabilities, app-start reconnect, `battery.v1`
state-change updates.

---

## 28. Git status

```
$ git branch --show-current
feature/kde-statusnotifier-v1

$ git status --short
 M desktop/Cargo.lock
 M desktop/cli/Cargo.toml
 M desktop/daemon/src/main.rs
 M desktop/gui/Cargo.toml
 M desktop/gui/tests/brand_assets.rs
 M desktop/gui/tools/install-desktop-metadata.sh
 M desktop/platform-linux/Cargo.toml
 M desktop/platform-linux/src/lib.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? desktop/gui/data/io.github.yurisismotto.anyflow.service.in
?? desktop/platform-linux/src/tray/
?? desktop/platform-linux/tests/

$ git diff --check
(clean)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untouched and still untracked. Nothing
was committed, pushed, or opened as a PR. `git add .` was never used.

---

## 29. Quality gates

```
cargo fmt --all --check                                  CLEAN
cargo test --workspace -j 2                              953 passed, 0 failed, 23 ignored
cargo clippy --locked --workspace --all-targets \
      --all-features -j 2 -- -D warnings                 CLEAN
cargo test -p anyflow-gui -- --ignored --test-threads=1  1 passed, 0 failed
cargo test -p anyflow-linux -- --test-threads=1          48 passed, 0 failed
```

**45 of those 953 are new**: 15 (`tray_model`) + 19 (`tray_dbus`) + 5
(`tray_identity`) + 3 (crate unit tests) + 3 (`brand_assets`). The baseline was
908.

The D-Bus suite is run with `--test-threads=1` above, but does not need it:
each test owns its own `dbus-daemon` and shares no fixture with any other.

Cargo work was run sequentially at `-j 2` throughout. No Gradle was run, no
Android build was run, and no VM was booted.

---

## 30. Gate matrix

| Gate | Result |
|---|---|
| SNI SPEC COMPLIANCE | **PASS** — introspection compared member-by-member against KDE's own XML |
| WATCHER REGISTRATION | **PASS** |
| WATCHER REAPPEAR / RE-REGISTER | **PASS** — three cycles on the live bus, one registration each |
| DBUSMENU | **PASS** |
| QUICK PANEL ACTIVATION | **PASS** |
| SETTINGS ACTIVATION | **PASS** |
| FILES ACTIVATION | **PASS** — the whole chain proven: DBusMenu *Files* → `TrayAction::Files` → `"transfers"` → Settings activated (see the note below) |
| GUI COLD DBUS ACTIVATION | **PASS** |
| GUI NON-RESIDENT LIFECYCLE | **PASS** |
| DAEMON SUPERVISION | **PASS** |
| SYSTEMD SANDBOX | **PASS** — no hardening weakened; a pre-existing unit defect recorded, not fixed |
| GNOME NON-REGRESSION | **PASS** |
| FLOW A TRAY IDENTITY | **PASS** — item identity, `IconName` and the D-Bus metadata (see the note below) |
| KDE REAL-SHELL CERTIFICATION | **BLOCKED** — no Plasma session on this machine; none installed, none simulated |
| SECURITY / PRIVACY | **PASS** |
| PORTABLE WINDOWS BOUNDARY | **PASS** |
| DESKTOP TEST GATES | **PASS** |

### Two rows that carry a boundary

Both are **PASS**, and in both cases a second, narrower thing is **BLOCKED**.
Keeping the two apart is the whole point of the distinction; collapsing either
one in either direction would make this report say something it did not
measure.

**FLOW A TRAY IDENTITY — PASS, for identity and metadata.** What passed is that
the item *is* AnyFlow and asks for AnyFlow's mark: `Id` and `IconName` are both
`io.github.yurisismotto.anyflow`, `Title` is `AnyFlow`, the D-Bus activation
entry, the desktop entry's `Icon=`, the installed `hicolor` file and the
GtkApplication `APP_ID` are the same string, and a real theme lookup resolves
that name to a file byte-identical to the canonical `app-icon.svg` (§10).
**What remains BLOCKED is the visual rendering**: whether Plasma actually draws
the Flow A in its system tray has not been seen, because there is no KDE Plasma
session available on which to see it. A resolved icon name is not a drawn icon.

**FILES ACTIVATION — PASS, for the chain.** Every link was measured:
a `com.canonical.dbusmenu` `Event(2, "clicked", …)` on the row labelled *Files*
maps to `TrayAction::Files`, which resolves to the GApplication action name
`"transfers"` — captured on the live session bus by `dbus-monitor` as
`ActivateAction` with `string "transfers"` (§19) — and Settings was activated,
cold, with the window observed. **Recorded separately, and not part of that
PASS:** the specific page Settings opened on could not be inspected, because
this session's accessibility tree exposes only the frame of that window. That
is a limitation of the inspection, and it is **not** a claim of visual
certification in either direction — `app.transfers` opening Settings on
`Page::Files` rests on the GUI's own pre-existing unit tests, not on anything
seen here.

---

## 31. Final verdict

### KDE STATUSNOTIFIER V1: IMPLEMENTED — PHYSICAL KDE CERTIFICATION BLOCKED

* **The daemon owns the tray.** `anyflowd` publishes the item; `anyflow-gui`
  gained no new lifetime, no autostart and no residency, and the bus starts it
  for a window and lets it go.
* **The protocol is derived, not remembered.** Every property, method, signal
  and signature came out of the files KDE and Plasma generate their own code
  from, and two tests compare the live objects against those tables
  member-by-member.
* **The lifecycle is event-driven.** No timer, no poll, no backoff, no retry
  counter — asserted in the source and measured on a bus monitor at zero
  messages per two idle seconds.
* **Nothing about anybody is on the bus.** Fifteen constant strings, all of
  them already public, gathered by asking the live objects rather than by
  listing.
* **The sandbox holds.** The GUI a tray click starts has a different parent, a
  different cgroup, `NoNewPrivs: 0` and `Seccomp: 0`; `anyflowd` spawns no child
  process at all.
* **GNOME is not worse.** Two log lines, no spam, no fake icon claimed, every
  capability unaffected, the paired tablet reconnecting mid-run.
* **Nothing new was taken on, and nothing shared was touched.** No new crate —
  the lock's diff is two dependency-edge lines and no `[[package]]`. No
  protocol or protobuf change: `protocol/` and `android/` are untouched, and
  nothing the tray publishes was asked of a peer.
* **The gates hold.** 953 workspace tests pass, 0 fail, **45 of them new**;
  `cargo fmt --all --check` clean; `cargo clippy --locked --workspace
  --all-targets --all-features -- -D warnings` clean; the display-gated GUI
  tests pass; GNOME non-regression passes.

And the honest half, stated once more so it cannot be read past: **no KDE
Plasma session existed on this machine, none was installed, and none was
simulated.** So nothing here claims that Plasma draws the Flow A, honours
`ItemIsMenu`, or places the menu where it should. The tray's *identity* — `Id`,
`IconName`, and the D-Bus metadata that carries them — is PASS and was
measured; the *rendering* of that identity by a real shell is BLOCKED, and the
two are not the same claim. That is the one thing a real shell has to say, and
it has not said it yet.

One thing was found and deliberately left alone:
`packaging/fedora/anyflowd.service` cannot bind the control socket under its
own hardening (§15, debt 1). It is recorded as a discovered debt and **not
fixed on this branch** — it predates this work, it has nothing to do with the
tray, and packaging is a later sprint.

Stopping here for review. Nothing committed, nothing pushed, no PR opened.
