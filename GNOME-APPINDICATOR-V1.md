# GNOME AppIndicator Compatibility v1

**Branch** `feature/gnome-appindicator-v1` · **Base** `develop` after PR #37
(KDE StatusNotifier v1) · **Date** 2026-09-18

---

## Summary

The sprint's first job was to find out whether AnyFlow needed a GNOME tray
backend at all. It does not.

GNOME Shell has no tray. What GNOME users install is an extension —
**AppIndicator and KStatusNotifierItem Support**,
`appindicatorsupport@rgcjonas.gmail.com` — which owns
`org.kde.StatusNotifierWatcher` and draws whatever registers with it. That is
the same well-known name, the same `org.kde.StatusNotifierItem` interface and
the same `com.canonical.dbusmenu` menu that PR #37 already implements. So the
KDE StatusNotifier implementation **is** the GNOME AppIndicator
implementation.

**No product code changed.** No dependency was added. `desktop/Cargo.lock` is
byte-identical. What this sprint produced is evidence: an audit of the
extension's own source at the version Fedora ships, a compatibility table with
a citation per row, 18 new deterministic tests against a host modelled on that
source, and a set of physical measurements against the live daemon.

One gate was, at first writing, genuinely blocked: the extension was **not
installed** on this machine, and installing it (approved, done, from the
Fedora repository) turned out not to be enough — GNOME Shell scans extension
directories only at startup, and on Wayland the Shell cannot be restarted
without a logout.

**That logout has now happened, and §28–§38 are the real-host certification.**
The extension is loaded and `ACTIVE`, `org.kde.StatusNotifierWatcher` is owned
by the real `gnome-shell` process, AnyFlow's item is the one item registered
with it, the Flow A is drawn in the primary top panel, and the menu the real
extension built from AnyFlow's DBusMenu has exactly the three expected rows.
**P1–P18 all pass. Still no product code changed.**

---

## 1. Baseline

```
$ git branch --show-current
feature/gnome-appindicator-v1

$ git log -5 --oneline
5195692 Merge pull request #37 from yurisismotto/feature/kde-statusnotifier-v1
cc2218c feat(linux): add KDE StatusNotifier integration
0b2e319 Merge pull request #36 from yurisismotto/feature/android-branding-files-ux-v1
0b82581 feat(android): add Flow A branding and Files UX
d48e86e Merge pull request #35 from yurisismotto/fix/android-clipboard-truthfulness-v1

$ git merge-base --is-ancestor 5195692 HEAD && echo yes
yes

$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md      # unrelated, untouched

$ git diff --check
(clean)
```

PR #37 is in ancestry. The untracked `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was
neither read nor staged.

---

## 2. GNOME environment

| Fact | Value |
| --- | --- |
| Distribution | Fedora Linux 44 (Workstation Edition) |
| Kernel | 7.1.9-200.fc44.x86_64 |
| `gnome-shell --version` | **GNOME Shell 50.4** |
| `XDG_CURRENT_DESKTOP` | `GNOME` |
| `XDG_SESSION_DESKTOP` | `gnome` |
| `XDG_SESSION_TYPE` | **`wayland`** |
| GNOME Shell process | pid 4714, started **8 Sep 2026 18:20:48** |

That last row is the one that decided §15 at first writing. The Shell had been
running for ten days; the extension package was installed the same day.

> **Superseded by §28.** The machine has since been logged out and back in.
> The Shell described above (pid 4714) is gone; the session is now served by
> pid 2752281, started 18 Sep 11:40:58, with the extension loaded. The
> post-logout environment is recorded in §28.

---

## 3. Installed and enabled tray extensions

### Before this sprint

```
$ gnome-extensions list
dash-to-dock@micxgx.gmail.com
multi-monitors-bar@frederykabryan
apps-menu@…  background-logo@…  launch-new-instance@…  places-menu@…  window-list@…

$ gnome-extensions list --enabled
dash-to-dock@micxgx.gmail.com
multi-monitors-bar@frederykabryan
background-logo@fedorahosted.org
```

**No AppIndicator/SNI extension was installed.** This was verified three ways
rather than inferred from the list, as §3 requires:

1. `gnome-extensions list` — no `appindicatorsupport@…`.
2. A grep for `statusnotifier|appindicator` across **both** extension
   directories returned exactly one hit:
   `multi-monitors-bar@frederykabryan/mirroredIndicatorButton.js:871`. That
   file is a hardcoded list of extension *role names* the extension treats as
   "problematic" for icon sizing on GNOME < 49. It registers nothing, owns
   nothing, and contains neither `RegisterStatusNotifierItem` nor any
   `own_name` call. **It is not an SNI host.**
3. `busctl --user list` showed no name resembling a watcher.

### §18 — extension conflicts

One SNI host, at most, can own the well-known name. There are **no** competing
SNI/AppIndicator hosts installed or enabled on this machine, so there is
nothing to conflict. No user extension was disabled, and no user extension
setting was changed by this sprint — see §24.

### After the approved install

Installing was explicitly approved, from the Fedora repository, no manual
tarball:

```
$ pkexec dnf install -y gnome-shell-extension-appindicator
…
[6/6] Instalando gnome-shell-extension-…
Concluído!

$ rpm -q gnome-shell-extension-appindicator
gnome-shell-extension-appindicator-64-1.fc44.noarch
```

| Check | Result |
| --- | --- |
| Version installed | **64-1.fc44** — upstream `v64`, the version audited in §5 |
| UUID | **`appindicatorsupport@rgcjonas.gmail.com`** ✓ |
| `shell-version` in its metadata | `["45","46","47","48","49","50"]` |
| Compatible with GNOME Shell 50.4? | **Yes** — `50` is listed |

`dnf` also pulled in `libappindicator-gtk3`, `libdbusmenu` and
`libdbusmenu-gtk3`. Those are dependencies **of the extension package**, on
the host side of the bus. AnyFlow gained nothing — see §17.

---

## 4. Actual StatusNotifier watcher owner

```
$ busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus GetNameOwner s org.kde.StatusNotifierWatcher
Call failed: The name does not have an owner
```

**`org.kde.StatusNotifierWatcher` has no owner** — before the install, and
still after it.

Attempting to enable it, without restarting or killing the Shell:

```
$ gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
A extensão "appindicatorsupport@rgcjonas.gmail.com" não existe     # exit 2

$ gdbus call … org.gnome.Shell.Extensions.EnableExtension "appindicatorsupport@…"
(false,)

$ gdbus call … org.gnome.Shell.Extensions.GetExtensionInfo "appindicatorsupport@…"
(@a{sv} {},)
```

The running Shell does not know the extension exists. GNOME Shell enumerates
extension directories when it starts, and this Shell started ten days before
the directory appeared. On Wayland the Shell cannot be restarted in place.

**Therefore: a logout/login is required, and this is the point the sprint was
told to stop at.** Nothing was restarted, nothing was killed, and nothing was
logged out.

Crucially, `enabled-extensions` was **not** modified by the failed attempt:

```
$ gsettings get org.gnome.shell enabled-extensions
['background-logo@fedorahosted.org', 'dash-to-dock@micxgx.gmail.com',
 'multi-monitors-bar@frederykabryan']
```

The user's extension state is exactly as it was.

> **Superseded by §29.** After the logout the same name **is** owned, by
> `:1.14` = `gnome-shell` pid 2752281, and `RegisterStatusNotifierItem`
> succeeds. See §29.

---

## 5. Upstream extension audit

Audited from source, not from prose: the upstream `v64` tree
(`github.com/ubuntu/gnome-shell-extension-appindicator`, tag `v64`), which is
the version Fedora 44 packages as `64-1.fc44`.

### How it accepts a registration

`statusNotifierWatcher.js:207` `RegisterStatusNotifierItemAsync` reads its
argument two ways — a leading `/` is an object path with the bus name taken
from the sender; anything matching `DBusUtils.BUS_ADDRESS_REGEX`
(`dbusUtils.js:22`) is a **well-known name**, which it then resolves to the
unique name via `GetNameOwner` before using `/StatusNotifierItem` as the path.

AnyFlow sends `org.kde.StatusNotifierItem-<pid>-1`, which matches that regex
(the `$` binds only to the second alternative, so the first is an unanchored
"something-dot-something" search). This is the same form KDE's own client
sends, and it is the branch the extension was written for.

### What it does with a duplicate

`_ensureItemRegistered` (`:134`) looks the item up by `Util.indicatorId`
(`util.js:33`) and, if it is already held, calls `item.reset()` and returns.
A repeat registration is never a second icon.

### The brute-force scan

`_seekStatusNotifierItems` (`:148`) runs `tools/busAnalyzer.js` two seconds
after the watcher starts, walking every bus name and introspecting it from `/`
downwards for `org.kde.StatusNotifierItem`. It exists because some
applications never re-register when the extension is toggled. It de-duplicates
against the same `indicatorId`, so it cannot double an item that registered
normally.

### Readiness and visibility

* `_checkIfReady` (`appIndicator.js:480`) requires a name owner, a **non-empty
  `Id`**, and a **non-empty `Menu`**. An item missing either never gets an icon.
* `indicatorStatusIcon.js:321`: `this.visible = status !== 'Passive'`.

### Feature detection

`appIndicator.js:456` introspects the item and sets
`supportsActivation = !!lookup_method('Activate')` and
`_hasAyatanaSecondaryActivate = !!lookup_method('XAyatanaSecondaryActivate')`.
Both answers change behaviour that no property expresses.

### What its XML deliberately omits

`interfaces-xml/StatusNotifierItem.xml` **comments out `ToolTip`** with the
note *"We disable this as we don't support tooltip"*, and comments out every
`New…` signal. `ItemIsMenu` is declared but the string does not occur in any
`.js` file.

`interfaces-xml/DBusMenu.xml` declares `TextDirection`, `IconThemePath`,
`EventGroup` and `AboutToShowGroup` — **none of which is read or called
anywhere in the extension's JavaScript.** Verified by grep across every `.js`
in the tree. This matters: the KDE sprint chose not to implement `EventGroup`
or `AboutToShowGroup`, and that choice is safe on GNOME too.

---

## 6. Native SNI vs libappindicator — the architectural decision

> **Decision: A.** The existing AnyFlow `StatusNotifierItem` + `DBusMenu` is
> already fully consumable by GNOME's AppIndicator/KStatusNotifierItem
> extension. No compatibility change is required, and no second tray backend
> is required.

Every method the extension calls, AnyFlow implements. Every property the
extension reads, AnyFlow publishes, with the right signature and a value that
produces the intended behaviour. Every method AnyFlow omits, the extension
either never calls or feature-detects and routes around.

§7's dependency gate was therefore never opened. `libappindicator`,
`libayatana-appindicator`, `ksni` and GTK tray libraries were **not** added,
not evaluated for packaging, and not needed — because the gate's precondition
("real evidence proves the native SNI implementation cannot work with the
GNOME extension") is false.

---

## 7. Compatibility table

Evidence is a file and line in upstream `v64`, or a live measurement against
`anyflowd` on this session's bus.

### `org.kde.StatusNotifierItem`

| AnyFlow | GNOME extension expectation | Compatible | Evidence |
| --- | --- | --- | --- |
| bus name `org.kde.StatusNotifierItem-<pid>-1` | matches `BUS_ADDRESS_REGEX`, resolved via `GetNameOwner` | ✅ | `dbusUtils.js:22`, `statusNotifierWatcher.js:217` |
| object at `/StatusNotifierItem` | `DEFAULT_ITEM_OBJECT_PATH` | ✅ | `statusNotifierWatcher.js:41` |
| objects exported **before** the name is claimed | registration is verified by introspection | ✅ | `appIndicator.js:448`; PR #37 already ordered it this way |
| `Category` = `ApplicationStatus` | `s`, read into the proxy | ✅ | XML:7; live `GetAll` |
| `Id` = `io.github.yurisismotto.anyflow` | **must be non-empty** or no icon | ✅ | `appIndicator.js:480` |
| `Title` = `AnyFlow` | `s` | ✅ | XML:9 |
| `Status` = **`Active`** | hidden iff `Passive` | ✅ | `indicatorStatusIcon.js:321` |
| `WindowId` = `0` | declared, unused | ✅ | XML:11 |
| `Menu` = `/MenuBar` | **must be non-empty** or no icon | ✅ | `appIndicator.js:480` |
| `IconName` = `io.github.yurisismotto.anyflow` | `new Gio.ThemedIcon({name})` | ✅ | `appIndicator.js:1157` |
| `IconThemePath` = `""` | falsy ⇒ skip private theme lookup | ✅ | `appIndicator.js:1271`, `:1345` |
| `IconPixmap` = empty | only consulted if the name yields nothing | ✅ | `appIndicator.js:1476` |
| Overlay / Attention names and pixmaps empty | optional | ✅ | XML:28–43 |
| `ItemIsMenu` = `false` | **never read** | ⚪ ignored | absent from every `.js` |
| `ToolTip` published | **never read** — commented out of its XML | ⚪ ignored | XML:50–54 |
| `Activate(ii)` | feature-detected; **double** click | ✅ | `appIndicator.js:456`, `indicatorStatusIcon.js:388` |
| `SecondaryActivate(ii)` | middle click, after the Ayatana probe fails | ✅ | `appIndicator.js:834` |
| `XAyatanaSecondaryActivate(u)` **absent** | probed; absence ⇒ fall back | ✅ | `appIndicator.js:457`, `:825` |
| `ContextMenu(ii)` no-op | **never called** | ✅ | absent from every `.js` |
| `Scroll(is)` no-op | called on smooth scroll | ✅ | `indicatorStatusIcon.js:453` |
| `ProvideXdgActivationToken(s)` | called before `Activate` and `SecondaryActivate` | ✅ | `appIndicator.js:775` |
| `New…` signals declared, never emitted | proxy listens; nothing changes | ✅ | `appIndicator.js:181` |

### `com.canonical.dbusmenu`

| AnyFlow | GNOME extension expectation | Compatible | Evidence |
| --- | --- | --- | --- |
| `Version` = `3` | `u` | ✅ | XML:3 |
| `Status` = `normal` | `s` | ✅ | XML:5 |
| `TextDirection` **absent** | declared, **never read** | ✅ | grep: no use |
| `IconThemePath` (menu) **absent** | declared, **never read** | ✅ | grep: no use |
| `GetLayout(0, -1, ['type','children-display'])` | phase one | ✅ | `dbusMenu.js:354`; **measured live** |
| `GetGroupProperties(ids, [])` | phase two | ✅ | `dbusMenu.js:301`; **measured live** |
| `GetProperty` | declared | ✅ | XML:23 |
| `AboutToShow(0)` → `false` | called, guarded, failures cached | ✅ | `dbusMenu.js:508`, `:894`; **measured live** |
| `EventGroup` **absent** | declared, **never called** | ✅ | grep: no use |
| `AboutToShowGroup` **absent** | declared, **never called** | ✅ | grep: no use |
| `Event(id,"clicked",<i 0>,t)` | a selection | ✅ | `dbusMenu.js:682` |
| `Event(0,"opened"/"closed",<i 0>,0)` ignored | brackets every menu display | ✅ | `dbusMenu.js:963`, `:966` |
| root `children-display` = `submenu` | **required** to descend | ✅ | `dbusMenu.js:590`; **measured live** |
| `label`:`s`, `enabled`:`b`, `visible`:`b` | `MandatedTypes`; a mismatch is **silently dropped** | ✅ | `dbusMenu.js:74`; **measured live** |
| `type` omitted | defaults to `standard` | ✅ | `dbusMenu.js:90` |

### Live verification of the menu contract

Run against the running `anyflowd`, using the extension's exact call shapes:

```
$ gdbus call … com.canonical.dbusmenu.GetLayout -- 0 -1 "['type','children-display']"
(uint32 1, (0, {'children-display': <'submenu'>},
 [<(1, @a{sv} {}, @av [])>, <(2, @a{sv} {}, @av [])>, <(3, @a{sv} {}, @av [])>]))

$ busctl … GetGroupProperties aias 4 0 1 2 3 0
… 0 1 "children-display" s "submenu"
  1 3 "enabled" b true "visible" b true "label" s "Quick Panel"
  2 3 "visible" b true "enabled" b true "label" s "Files"
  3 3 "enabled" b true "visible" b true "label" s "Settings"

$ busctl … AboutToShow i 0
b false
```

Phase one correctly returns the shape with the requested properties only;
phase two fills in the labels with the mandated types. This is the whole menu
handshake, answered exactly as the extension expects.

---

## 8. Implementation changes

**None.** No file under `desktop/platform-linux/src/`, `desktop/daemon/src/`,
`desktop/gui/src/`, `desktop/gui/data/` or `desktop/proto/` was modified.

The sprint brief said not to force a code change merely because this is called
a sprint. The evidence supports doing nothing, so nothing was done.

Two decisions were **re-examined** against real host evidence and deliberately
left as they are — see §10 and §13 below. That is a different thing from not
looking.

---

## 9. Icon behaviour

`IconName` is `io.github.yurisismotto.anyflow`, a *theme name*, and
`IconThemePath` is empty. That combination takes the extension down the branch
at `appIndicator.js:1345` → `:1157`, which returns `new Gio.ThemedIcon({name})`
— handing the name to St, which resolves it through the session's icon theme
at the size and scale it is actually drawing.

The canonical mark remains `docs/design/assets/app-icon.svg`, installed
unmodified by `install-desktop-metadata.sh`, and it is present on this machine:

```
/home/yuri/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
/home/yuri/.local/share/applications/io.github.yurisismotto.anyflow.desktop
/home/yuri/.local/share/dbus-1/services/io.github.yurisismotto.anyflow.service
```

**No `IconPixmap` was added**, and no GNOME-specific fork of the Flow A
artwork was created. §10 permits adding pixmaps only if real GNOME evidence
shows `IconName` failing with the theme metadata correctly installed; there is
no such evidence, and there cannot be until the panel actually draws the icon.

Whether GNOME recolours it symbolically, and how the Flow A reads at panel
size, is host behaviour that **P3/P4 will record** after the logout. The
extension's own lookup flags honour the Shell theme's `icon-style`
(`appIndicator.js:1108`), so a symbolic rendering would be the host's choice
and not an AnyFlow failure.

---

## 10. Click semantics — measured from the host, not assumed from KDE

This is where GNOME differs most from Plasma, and the difference was read out
of `indicatorStatusIcon.js:418` rather than guessed.

For an item with `supportsActivation === true` **and** a non-empty menu —
which is AnyFlow — the extension behaves as follows:

| Gesture | What GNOME sends | What AnyFlow does |
| --- | --- | --- |
| **single left click** | *nothing on the bus* — after the double-click timeout it calls `menu.toggle()` and opens the DBusMenu | the menu is drawn from `/MenuBar` |
| **double left click** | `ProvideXdgActivationToken` then **`Activate(x,y)`**; the pending menu is cancelled | **Quick Panel** |
| **middle click** | `ProvideXdgActivationToken` then `SecondaryActivate(x,y)` | deliberately nothing — §13 |
| **right click** | *nothing on the bus* — `menu.toggle()` | the menu is drawn from `/MenuBar` |
| **scroll** | `Scroll(delta,'horizontal'\|'vertical')` | deliberately nothing |

Two consequences worth stating plainly:

* **On GNOME the primary click opens the menu, not the Quick Panel.** On
  Plasma, with `ItemIsMenu = false`, the primary click is `Activate`. The
  extension never reads `ItemIsMenu`, so the same item behaves differently on
  the two desktops — and §12 says that is acceptable and native. It is also
  fine for the product: every surface stays reachable, the Quick Panel by one
  click plus one, or by a double click.
* **KDE semantics were not forced onto GNOME.** No code exists that asks which
  desktop is running, and none was added.

---

## 11. DBusMenu compatibility

Covered by the table in §7 and the live transcript beneath it. In summary: the
root id is `0`, it carries `children-display: submenu` (without which the
extension treats the root as a leaf and draws an empty menu), the three rows
carry `label`/`enabled`/`visible` with the mandated types, the layout revision
is a constant `1`, `AboutToShow` truthfully answers `false`, and the four
event verbs the extension sends — `opened`, `closed`, `clicked`, `hovered` —
resolve to exactly one action for exactly one of them.

Nothing the extension requires is missing. Nothing was added on suspicion:
`EventGroup`, `AboutToShowGroup`, `TextDirection`, menu `IconThemePath`,
per-row `icon-name`/`icon-data`, `toggle-type` and `toggle-state` are all
absent from AnyFlow and all unused by the extension.

---

## 12. Tooltip difference

The extension comments `ToolTip` out of its own interface XML with the note
*"we don't support tooltip"*. **GNOME will not show AnyFlow's tooltip. That is
not an AnyFlow failure and nothing was redesigned because of it.**

The tooltip still has to be safe, because it is a public property of a public
object that any process on the session bus can read without a click. Measured
on the live daemon:

```
'ToolTip': <('', @a(iiay) [], 'AnyFlow', 'One flow. Any device.')>
```

Product name and tagline. No filename, no clipboard content, no notification,
no peer name, no fingerprint, no address.

---

## 13. SecondaryActivate — re-decided, and left a no-op

The KDE sprint left `SecondaryActivate` empty because no real host had ever
been observed sending it. That reason has now expired: GNOME's extension
demonstrably maps middle click to it (`indicatorStatusIcon.js:423`).

**Decision: Option A — keep the no-op.** Not by inertia, and not merely
because the event exists, but for a reason the new evidence supplies:

* On GNOME, a **single** click already opens a menu containing all three
  surfaces, and a **double** click already opens the Quick Panel. Every
  surface is at most two clicks away by two different routes.
* A middle click mapped to Quick Panel would duplicate `Activate`. Mapped to
  Settings it would be arbitrary, undiscoverable, and inconsistent with
  Plasma, where the same gesture would then have to mean something too.
* A middle click is a gesture people make by accident on a trackpad. The
  correct behaviour for an accident is nothing.

This is now recorded as a decision rather than a debt. `n14` pins it: the
event arrives, a token is offered, and no window opens.

---

## 14. ContextMenu

`ContextMenu(x,y)` is **never called** by the extension — the string does not
appear in any of its JavaScript. It draws the menu itself from
`com.canonical.dbusmenu`. The no-op therefore stays, `anyflowd` still creates
no popup of its own, and there is still no GTK dependency in
`daemon` or `platform-linux`. `n15` asserts the menu is reachable in full
without `ContextMenu` ever being called.

---

## 15. GApplication activation — certified

The activation chain does not involve the tray extension at all: the extension
draws an icon, and the icon's events reach the same `/MenuBar` and
`/StatusNotifierItem` objects that any caller can reach. So this was certified
**now**, physically, against the live daemon on the real session bus, by
sending exactly the events GNOME sends.

**Cold** — no `anyflow-gui` process existed:

```
$ pgrep -c -x anyflow-gui          →  0
$ gdbus call … com.canonical.dbusmenu.Event 1 "clicked" "<int32 0>" 0
()
$ pgrep -a -x anyflow-gui
2744804 /home/yuri/.local/bin/anyflow-gui --gapplication-service
$ cat /proc/2744804/cgroup
0::/user.slice/…/app.slice/dbus-:1.2-io.github.yurisismotto.anyflow@10.service
```

The GUI was started **by the message bus**, into its own transient unit — not
as a child of `anyflowd`, and therefore not inside the daemon's sandbox. AT-SPI
confirmed a real window: `APP: anyflow-gui → WINDOW: 'AnyFlow' role: frame`.

**Warm** — the same process, three surfaces:

| Menu row | Event | Processes after | Result |
| --- | --- | --- | --- |
| Quick Panel | `Event(1,"clicked")` | 1 | window `AnyFlow` |
| Settings | `Event(3,"clicked")` | **1** | second window `AnyFlow Settings` |
| Files | `Event(2,"clicked")` | **1** | `AnyFlow Settings` becomes `ACTIVE` |

One process throughout. **No second long-lived GUI, no daemon child-spawn.**

**Non-resident lifecycle** — closing the GUI does not take the tray with it:

```
$ pkill -x anyflow-gui ; sleep 3
$ pgrep -c -x anyflow-gui                          →  0
$ busctl --user list | grep StatusNotifierItem
org.kde.StatusNotifierItem-2692440-1  2692440 anyflowd  yuri :1.1542 …
$ gdbus call … Properties.Get org.kde.StatusNotifierItem Title
(<'AnyFlow'>,)
$ busctl --user status org.kde.StatusNotifierItem-2692440-1
PID=2692440   Comm=anyflowd
```

The item is owned by `anyflowd`, survives the GUI, and still answers.

### A finding to verify after the logout

`MenuItemFactory._onActivate` (`dbusMenu.js:677`) calls
`indicator.provideActivationToken(timestamp)` on the **item** immediately
before sending the menu's `clicked` event. AnyFlow's menu path passes `None`
for the token deliberately (`menu.rs`), on the stated reasoning that "the
shell never offers a token on the menu object" — which is true of the *menu
object* but not of the interaction as a whole.

Measured consequence in this session: **none.** The Settings window became
`ACTIVE` when opened from the menu without a token. It is recorded here so
that P6–P8 check it again in a genuinely cold, unfocused case. **No code was
changed to pre-empt it**, because a change made to compensate for an
unmeasured effect is exactly what this sprint was told not to do.

---

## 16. Watcher lifecycle

Unchanged and still event-driven: one `NameOwnerChanged` subscription for one
name, subscribed *before* the first `GetNameOwner` so a shell that finishes
starting in between arrives as a signal rather than falling into the gap. No
timer, no polling, no retry loop.

Deterministically certified against the GNOME-modelled host — `n1`–`n5`,
`n18`: registration when the host is already there, when it appears later,
when it is replaced, and silence afterwards. Real-host lifecycle is part of
the blocked physical set.

The extension's own lifecycle matches what that loop expects: `enable()`
creates the watcher and owns the name (`extension.js:87`), `disable()`
destroys it and unowns it (`:61`) — one owner change each way.

---

## 17. Dependency review

```
$ git diff --stat desktop/Cargo.lock
(no output)
```

**`desktop/Cargo.lock` is unchanged.** No `[[package]]` block was added,
removed or altered.

`desktop/platform-linux/Cargo.toml` is unchanged. Its dependency set is still
`anyflow-core`, `anyflow-control`, `anyflow-proto`, `async-trait`, `tokio`,
`tracing`, plus the optional `zbus` and `futures-lite` behind the `tray`
feature — both of which were already in the resolved graph before PR #37.

Not added, and not needed: `libappindicator`, `libayatana-appindicator`,
ayatana Rust bindings, `ksni`, any GTK tray library. `n17` now fails the build
if any of those names appears in the crate's manifest, so this stays true
after the sprint stops watching.

The `libappindicator-gtk3` / `libdbusmenu` / `libdbusmenu-gtk3` RPMs that
`dnf` installed are dependencies of the **GNOME extension package**. They are
host-side, they are not linked by anything AnyFlow builds, and they do not
appear in any AnyFlow manifest or lockfile.

**Portable Windows boundary:** untouched. No portable crate was modified; the
tray is behind `platform-linux`'s `tray` feature as before.

---

## 18. Privacy and security

Unchanged, and re-verified for the surface this sprint is about.

TLS 1.3, SPKI pinning, pairing proof, the trust store, per-peer grants,
clipboard privacy, notification privacy, file safety, no cloud, no telemetry —
none of these has any code path in common with what changed, because **no
product code changed**.

What the tray exposes was measured directly. Everything readable on both
objects is one of a closed set of constants:

```
"" · io.github.yurisismotto.anyflow · AnyFlow · One flow. Any device. ·
ApplicationStatus · Active · normal · submenu · Quick Panel · Files · Settings
```

`n16` asserts exactly that set over `GetAll` on the item plus
`GetGroupProperties` on the menu, recursing into every container. No clipboard
text, no filename, no path, no URI, no notification content, no IP address, no
fingerprint, no pairing material.

The menu also carries **no authority**: no pairing, granting, revoking,
sending or quitting. A decision about who may read this machine is not
reachable from a menu drawn by another process.

The activation path still cannot be steered by a message: `TrayAction` is a
three-variant enum with no payload, and the `GAction` names are `&'static str`
in `model.rs`. A D-Bus caller can send any string it likes; it is compared to
one constant and dropped.

---

## 19. Protocol impact

**None.** `desktop/proto/` untouched. No change to the network protocol, TLS
framing, pairing, capability negotiation, `files.v1`, `clipboard.v1`,
`notifications.v1` or `battery.v1`.

```
$ git diff --name-only | grep -E 'proto/|tls|pairing|capability'
(no output)
```

This is purely local shell integration, and it did not even need that.

---

## 20. Stock GNOME behaviour — certified

This is the state the machine is in right now, and it was observed over 69
minutes of a real session with no watcher on the bus.

```
$ ps -o pid,etime,time,%cpu -p 2692440
2692440  01:09:12  00:00:03  0.0
```

**Three seconds of CPU in sixty-nine minutes.** A finer measurement over a
49-second window: **2 ticks — 0.02 CPU-seconds, ≈0.04%.** There is no busy
loop, which is what having no timer at all buys.

The daemon's complete log for that period is **16 lines**:

```
INFO anyflowd: local identity device=795fec… name=Fedora fingerprint=DF65 D3E4 BA28 EDF9
INFO anyflowd: UPower available; this machine will report its own battery
INFO anyflowd: files.v1 ready download_dir=/home/yuri/Downloads/AnyFlow …
INFO …clipboard…: clipboard backend backend="wl-clipboard" available=true …
INFO anyflowd: clipboard.v1 ready backend=wl-clipboard; …
INFO …notifications…: notification server server=gnome-shell vendor=GNOME version=50.4 …
INFO anyflowd: notifications.v1 ready sink=org.freedesktop.Notifications … available=true
INFO anyflowd: capabilities registered capabilities=["battery.v1","clipboard.v1","files.v1","notifications.v1"]
INFO anyflowd: listening port=55432 families=IPv4+IPv6 sockets=1
INFO anyflowd: control endpoint ready endpoint=/run/user/1000/anyflow/control.sock
INFO anyflow_runtime::mdns: advertising _anyflow._tcp.local. port=55432 families=IPv4+IPv6
INFO anyflow_linux::tray: AnyFlow tray item published on the session bus item=org.kde.StatusNotifierItem-2692440-1
INFO anyflow_linux::tray::watcher: no desktop tray host on this session; the AnyFlow tray item is
     published and will register itself if one appears
INFO anyflow_runtime::listener: session established device=6532889e… peer=573C CB84 DA6C 993B …
```

Against §8's checklist:

| Requirement | Result | Evidence |
| --- | --- | --- |
| `anyflowd` starts | ✅ | running 69 min |
| networking starts | ✅ | `tcp LISTEN *:55432` |
| mDNS starts | ✅ | `udp *:5353` ×4 fds, `_anyflow._tcp.local.` |
| control socket starts | ✅ | `srw------- /run/user/1000/anyflow/control.sock` |
| files works | ✅ | `files.v1 ready`; granted to a live peer |
| clipboard works | ✅ | `clipboard.v1 ready backend=wl-clipboard` |
| notifications works | ✅ | `notifications.v1 ready … available=true` |
| no busy loop | ✅ | 0.02 CPU-s / 49 s |
| no warning spam | ✅ | **zero** `warn`/`error` lines |
| no fake "tray ready" claim | ✅ | the line says the item *is published and will register if a host appears* — which is exactly true |

A live Android peer stayed connected throughout, on stock GNOME with no tray
host at all:

```
SM-X620  6532889e…  fingerprint 573C CB84 DA6C 993B
   connected yes · state connected · last frame 32s ago
   granted battery.v1, clipboard.v1, files.v1
```

**No watcher ≠ AnyFlow failure.** No XEmbed hack, no `GtkStatusIcon`, no
hidden GTK window, no Shell private API, no extension injection was added or
considered.

---

## 21. Automated tests

### Counts

| Suite | Before | After |
| --- | --- | --- |
| `tray_model.rs` (T1–T16) | 15 | **15** — unchanged |
| `tray_identity.rs` | 5 | **5** — unchanged |
| `tray_dbus.rs` (D1–D15, Plasma host) | 19 | **19** — unchanged |
| `tray_gnome.rs` (N1–N18, GNOME host) | — | **18 new** |
| **Tray total** | 39 | **57** |

Full workspace, `cargo test --workspace -j 2`:

```
63 test binaries · 971 passed · 0 failed · 23 ignored
```

Display-gated GUI tests, `cargo test -p anyflow-gui -- --ignored --test-threads=1`:
`1 passed; 0 failed`. The 23 ignored elsewhere are the pre-existing
real-clipboard and live-system-bus suites, unrelated to the tray.

### Every KDE test preserved

`tray_dbus.rs`'s 19 tests were run before and after the fixture extraction,
with identical names and identical results. Nothing was deleted, renamed or
weakened.

### The new suite — `platform-linux/tests/tray_gnome.rs`

A fake host whose every behaviour is modelled on the extension's source, with
the file and line cited at each one: it resolves the well-known name to a
unique name before touching the item, introspects for `Activate` and
`XAyatanaSecondaryActivate`, enforces `_checkIfReady`'s `Id`/`Menu`
preconditions, applies `Status !== 'Passive'`, and collapses a repeat
registration into a reset.

| Test | Gate | What it pins |
| --- | --- | --- |
| `n1` | D1, G7 | host already present ⇒ exactly one registration, keyed by the well-known name |
| `n2` | D1 | host appears later ⇒ exactly one |
| `n3` | D11, G8 | host replaced ⇒ exactly one new registration |
| `n4` | D10, G6 | host disappears ⇒ daemon and item survive |
| `n5` | D9 | a repeat registration takes the reset path, never a second icon |
| `n6` | D2 | ready and visible by the extension's own rules |
| `n7` | D2 | `Activate` found, Ayatana variant correctly absent |
| `n8` | D3, G11, G12 | theme name, no theme path, **no pixmaps** |
| `n9` | D4 | the brute-force `/`-downwards scan finds the item once, and not a second one |
| `n10` | D5 | the two-phase menu fetch |
| `n11` | D5 | every property matches `MandatedTypes` — a mismatch would be three blank rows |
| `n12` | D6–D8 | `AboutToShow`→`opened`→`clicked`→`closed`; one action, the right one |
| `n13` | — | looking at the menu and closing it opens nothing |
| `n14` | §13 | middle click offers a token and does nothing; no stale token leaks |
| `n15` | §14 | the menu is reachable without `ContextMenu` ever being called |
| `n16` | D12 | the closed string set over everything the extension can read |
| `n17` | G13 | **no tray-backend dependency** in the manifest |
| `n18` | D13 | after registering, the item says nothing further |

`n9` earned its keep during development: a first version searched the
introspection document as flat text and "found" the interface at `/` too,
because zbus serves the whole subtree inline. The extension does not make that
mistake — it asks `Gio.DBusNodeInfo.lookup_interface`, which is depth-aware —
so the test now counts tag depth the same way, and additionally asserts the
walk really visited `/`, `/MenuBar` and `/StatusNotifierItem`.

### §21 gate matrix

| | Gate | Where |
| --- | --- | --- |
| G1 | SNI identity unchanged | `t12`, `d7`, `n6` |
| G2 | DBusMenu layout unchanged | `t8`–`t10`, `n10` |
| G3 | no arbitrary action names | `t7`, `t16` |
| G4 | no private content in properties | `t11`, `d15`, `n16` |
| G5 | no host-specific state to build the item | no host branch exists in `src/tray/` |
| G6 | missing watcher non-fatal | `d2`, `n4`, and §20's live session |
| G7 | active watcher registers exactly once | `d1`, `n1`, `n5` |
| G8 | owner replacement re-registers once | `d5`, `n3` |
| G9 | no polling/timer introduced | `d14_the_source_contains_no_timer_at_all`, `n18` |
| G10 | no new resident GUI | `the_daemon_starts_the_tray_and_does_not_race_it_against_anything`; §15's one-process measurement |
| G11 | `IconName` is the official `APP_ID` | `t12`, `n8` |
| G12 | no `IconPixmap` | `d7`, `n8` |
| G13 | no libappindicator/ayatana dependency | `n17`, unchanged `Cargo.lock` |
| G14 | no GNOME-specific authority actions | `MENU` is the same three rows; `t7` |
| G15 | no platform protocol changes | §19 |

---

## 22. Physical tests

Certified **now**, because the activation chain never needed the extension:

| Gate | Result | Evidence |
| --- | --- | --- |
| P6 Quick Panel opens | ✅ PASS | §15 cold activation |
| P7 Files opens the right surface | ✅ PASS | §15, `AnyFlow Settings` becomes `ACTIVE` |
| P8 Settings opens | ✅ PASS | §15 |
| P9 GUI cold activation | ✅ PASS | started by the bus into its own transient unit |
| P10 GUI existing-instance activation | ✅ PASS | three activations, one process |
| P11 closing the GUI does not remove the tray item | ✅ PASS | item survives `pkill -x anyflow-gui` |
| P12 the daemon owns the tray item | ✅ PASS | `PID=2692440 Comm=anyflowd` |
| P15 no private content in the menu | ✅ PASS | live `GetGroupProperties`; `n16` |
| P16 no registration loop in the log | ✅ PASS | 16 lines in 69 min; one info line about the absent host |
| P17 Android/tablet reconnect still works | ✅ PASS | SM-X620 connected, last frame 32 s |
| P18 files/clipboard/notifications unaffected | ✅ PASS | all four capabilities ready |

These seven were **blocked at first writing**, until the Shell had been
restarted by a logout/login, because every one of them needs the extension
actually loaded and drawing. **The logout has happened and all seven now
pass** — the measurements are in §31 and §34:

| Gate | Result | Where |
| --- | --- | --- |
| P1 icon appears in the GNOME top panel | ✅ PASS | §31 — `[menu] 'AnyFlow'` at `(2269,1728,36,32)` on the primary panel |
| P2 one icon only | ✅ PASS | §31 — 9 panel menus in the Shell tree, exactly 1 named AnyFlow |
| P3 Flow A renders recognisably | ✅ PASS | §31 — resolves to the Flow A SVG; rendered at 16–64 px |
| P4 no generic/missing icon | ✅ PASS | §31 — control name falls back to `image-missing`, AnyFlow's does not |
| P5 the indicator exposes Quick Panel / Files / Settings | ✅ PASS | §31 — exactly 3 enabled `menu item`s, in English, from DBusMenu |
| P13 daemon restart recreates exactly one icon | ✅ PASS | §34 — watcher `1 → 0 → 1`, one registration, no duplicate |
| P14 no device connected still leaves a healthy indicator | ✅ PASS | §34 — `Status Active`, no attention icon, menu intact |

Also re-certified with the extension in the path, and P6–P12 and P15–P18
re-measured against the real host: §36.

No screenshots exist, because this machine has no screenshot path at all —
`org.gnome.Shell.Screenshot` and `Introspect` both answer `AccessDenied`,
`Eval` is off, and `import -window root` cannot work on a rootless Xwayland.
All three refusals were re-tested after the logout. §31 uses the Shell's own
AT-SPI tree instead, which is a stronger statement about what is on the panel
than a photograph would be.

---

## 23. Systemd

The packaged unit `packaging/fedora/anyflowd.service` still carries
`ProtectSystem=strict` with **no `RuntimeDirectory=` grant**, so it cannot
create `/run/user/1000/anyflow/control.sock`:

```
$ grep -c RuntimeDirectory packaging/fedora/anyflowd.service
0
```

This is the pre-existing packaging defect PR #37 discovered. **It was not
fixed here**, as instructed.

**SYSTEMD PACKAGED-UNIT TEST = BLOCKED BY KNOWN PACKAGING DEBT.**

The sandbox was not weakened, and nothing in this sprint required running the
daemon under it: no product code changed, so the sandbox's interaction with
the tray is exactly what PR #37 certified. The security directives are intact
and unmodified — `NoNewPrivileges`, `ProtectSystem=strict`,
`ProtectHome=read-only`, `MemoryDenyWriteExecute`,
`SystemCallFilter=@system-service` minus `@privileged @resources @obsolete`,
`RestrictAddressFamilies` to four families.

---

## 24. Optional user guidance and the packaging boundary

Truthfully, and without making the extension a runtime dependency:

> **GNOME requires a StatusNotifier/AppIndicator shell extension to display
> the AnyFlow tray icon.** Stock GNOME Shell has no system tray, and AnyFlow
> cannot add one. AnyFlow runs completely normally without it — every feature
> works, and the tray icon is the only thing that is absent.
>
> On Fedora: `sudo dnf install gnome-shell-extension-appindicator`, then
> enable **AppIndicator and KStatusNotifierItem Support** in the Extensions
> app. On Debian/Ubuntu the package is
> `gnome-shell-extension-appindicator`. A newly installed extension is picked
> up the next time GNOME Shell starts; on a Wayland session that means logging
> out and back in.

The protocol authority is `org.kde.StatusNotifierWatcher`, **not** a
particular extension package. No code checks for a UUID, a package name, or a
desktop environment in order to decide whether AnyFlow works — and none was
added. Any host that owns that name gets the item.

**Packaging boundary respected.** This sprint packaged nothing, installed no
extension into AnyFlow's own packaging, and added no `Requires`, `Recommends`
or `Suggests`. AnyFlow's RPM/DEB must not silently modify GNOME Shell
extensions, and does not. A later packaging sprint may add a *Recommends* or
*Suggests* where distro conventions allow; that decision is not taken here.

The one install that happened was a human-approved `dnf install` on this
development machine, for certification purposes only. It is not part of any
AnyFlow package.

---

## 25. Files changed

```
$ git status --short
 M desktop/platform-linux/tests/tray_dbus.rs
?? desktop/platform-linux/tests/common/
?? desktop/platform-linux/tests/tray_gnome.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md          # unrelated, untouched

$ git diff --stat
 desktop/platform-linux/tests/tray_dbus.rs | 114 +++-------------------
 1 file changed, 10 insertions(+), 104 deletions(-)
```

| File | Kind | What |
| --- | --- | --- |
| `desktop/platform-linux/tests/tray_gnome.rs` | **new** | N1–N18 against the GNOME-modelled host |
| `desktop/platform-linux/tests/common/mod.rs` | **new** | the private-bus fixture, extracted so both suites share one bus definition |
| `desktop/platform-linux/tests/tray_dbus.rs` | modified | fixture moved out; **every test body byte-identical**, all 19 still passing |
| `GNOME-APPINDICATOR-V1.md` | **new** | this report |

**No product source file was modified.** Tests and documentation only.

### Final git status, after the real-host certification

Unchanged by §28–§38, because that work only measured the running system and
wrote into this report:

```
$ git branch --show-current
feature/gnome-appindicator-v1

$ git status --short
 M desktop/platform-linux/tests/tray_dbus.rs
?? GNOME-APPINDICATOR-V1.md
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md          # unrelated, untouched
?? desktop/platform-linux/tests/common/
?? desktop/platform-linux/tests/tray_gnome.rs

$ git diff --check
(clean)

$ git diff --stat
 desktop/platform-linux/tests/tray_dbus.rs | 114 +++-------------------
 1 file changed, 10 insertions(+), 104 deletions(-)

$ git status --porcelain desktop/*/src/ desktop/proto/ desktop/Cargo.lock android/
(no output)
```

That last command is the code-change gate: **no product source, no protocol
file and no lockfile is modified.** Nothing was staged, committed, pushed, or
opened as a PR. `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was neither read nor
modified — it still carries its 15 Sep mtime and checksum
`ca26a92d683a05b954544cbf685305e2`.

To be precise about what was measured when, because "nothing changed" would
be the wrong summary:

* **Test files did change earlier in this sprint** — `tray_gnome.rs` and
  `tests/common/mod.rs` are new, and `tray_dbus.rs` had its fixture extracted.
  That is the diff shown above.
* **§26's software gates were run over those changes**, not over an untouched
  tree: **971 passed, fmt PASS, clippy PASS, display-gated GUI PASS.**
* **During the post-logout continuation — the real-host physical
  certification in §28–§38 — no source file and no test file changed.** That
  work only observed the running system and wrote into this report.
* **The suite was therefore not re-run after the physical certification**, and
  deliberately so: re-running it would have re-measured exactly the same code
  the gates above already covered.
* **No product source file changed at any point in the sprint.** Every change
  is a test file or documentation.

---

## 26. CI expectation

`desktop/**` changes, so the normal desktop matrix is expected to run: Debian
13, Ubuntu 24.04, Ubuntu 26.04, MSRV 1.88, Windows portable core, and desktop
fmt + clippy. No workflow was weakened or modified.

The changes are tests only. The new suite needs `dbus-daemon`, exactly as
`tray_dbus.rs` already does, so it runs wherever that one does and is skipped
by the same `#![cfg(feature = "tray")]` elsewhere. The Windows portable core
job is unaffected — no portable crate was touched.

Local gates, all green:

```
cargo fmt --all --check                                             ✅
cargo test --workspace -j 2               971 passed, 0 failed      ✅
cargo clippy --locked --workspace --all-targets --all-features
    -j 2 -- -D warnings                   no warnings               ✅
cargo test -p anyflow-gui -- --ignored --test-threads=1   1 passed  ✅
```

---

## 27. Remaining debts

Recorded, not fixed — all out of scope for this sprint:

1. **`packaging/fedora/anyflowd.service` has no `RuntimeDirectory=`** while
   using `ProtectSystem=strict`, so the packaged unit cannot create its
   control socket. Pre-existing, found by PR #37.
2. ~~**Real GNOME host certification is one logout away.** P1–P5, P13, P14.~~
   **CLOSED.** The logout happened; P1–P5, P13 and P14 all pass against the
   real GNOME Shell. See §28–§36.
3. **The menu path does not consume the activation token GNOME offers**
   (§15). Re-certified against the real extension in §33: the token is
   provably not forwarded, and all three surfaces still open **and take
   focus** from cold. **No measured harm, and no code changed.** One latent
   corner remains recorded and unfixed: a token left in the item's slot by a
   menu click would be forwarded by a later `Activate` from a host that does
   not provide its own token first. Neither GNOME nor Plasma is such a host,
   so it is unreachable on both certified desktops.
4. **KDE physical certification** remains blocked — no Plasma session exists
   on this machine. Unchanged by this sprint.
5. Unrelated and untouched: `BRAND.md` staleness, Android lint, incoming-file
   timeout shown as Declined, HelloAck supported/effective mismatch, auto
   reconnect, `battery.v1` update freshness.

---

## 28. Post-login environment — the extension actually loaded

The machine was logged out and back in. This section and everything after it
was measured **after** that, against the real GNOME Shell that now has the
extension in it.

| Fact | Before the logout | After the logout |
| --- | --- | --- |
| `gnome-shell --version` | GNOME Shell 50.4 | **GNOME Shell 50.4** |
| `XDG_CURRENT_DESKTOP` | `GNOME` | **`GNOME`** |
| `XDG_SESSION_TYPE` | `wayland` | **`wayland`** |
| GNOME Shell pid | 4714, started 8 Sep 18:20 | **2752281, started 18 Sep 11:40:58** |
| RPM | `gnome-shell-extension-appindicator-64-1.fc44` | **unchanged — 64-1.fc44** |
| Extension known to the Shell | **no** | **yes** |
| Extension state | n/a (`GetExtensionInfo` → `{}`) | **`ACTIVE`** |

```
$ gnome-extensions info appindicatorsupport@rgcjonas.gmail.com
appindicatorsupport@rgcjonas.gmail.com
  Nome: AppIndicator and KStatusNotifierItem Support
  Caminho: /usr/share/gnome-shell/extensions/appindicatorsupport@rgcjonas.gmail.com
  Habilitada: Sim
  Estado: ACTIVE
```

`GetExtensionInfo` over the Shell's own D-Bus agrees: `'state': <1.0>`, i.e.
`ENABLED`.

**`gnome-extensions enable` was not run by this sprint.** The extension was
already enabled when it was first checked after login, so the brief's
conditional enable step never fired. `enabled-extensions` has gained the UUID
since §4's reading:

```
$ gsettings get org.gnome.shell enabled-extensions
['background-logo@fedorahosted.org', 'dash-to-dock@micxgx.gmail.com',
 'multi-monitors-bar@frederykabryan', 'appindicatorsupport@rgcjonas.gmail.com']
```

The RPM ships only the extension's own settings schema
(`org.gnome.shell.extensions.appindicator.gschema.xml`) and no enablement
override, so that change was made outside this sprint, at or after login. No
other extension was touched: the three that were enabled before are all still
enabled, and none was disabled.

### The logout also replaced the session bus — and AnyFlow said so

Worth recording because it was measured rather than assumed. The user bus was
rebuilt by the logout: the broker that had served the session since 8 Sep
(pid 1298) was replaced by a new one (pid 2752136, started 11:40:58) now
listening on `/run/user/1000/bus`.

The `anyflowd` from before the logout **survived the logout as a process** and
handled the disappearance of its bus truthfully:

```
WARN monitor_name_lost{name=org.kde.StatusNotifierItem-2692440-1}:
     zbus::connection: Failed to parse `NameLost` signal: I/O error: failed to read from socket
INFO anyflow_linux::tray: the session bus closed; the AnyFlow tray item is gone
```

It did not claim a tray it no longer had, it did not spin, and it kept the
Android session running across the whole logout. Restarting the daemon put it
on the new bus. That restart is the only reason a restart was needed, and it
is a property of the logout, not a defect.

---

## 29. The real watcher

```
$ busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus GetNameOwner s org.kde.StatusNotifierWatcher
s ":1.14"

$ busctl --user call … GetConnectionUnixProcessID s ":1.14"
u 2752281

$ ps -o pid,comm,args -p 2752281
2752281 gnome-shell /usr/bin/gnome-shell --mode=user
```

| Fact | Value |
| --- | --- |
| Unique owner of `org.kde.StatusNotifierWatcher` | **`:1.14`** |
| That connection's process | **`gnome-shell`, pid 2752281** |
| Extension UUID | `appindicatorsupport@rgcjonas.gmail.com` |
| Extension state | **`ACTIVE`** (`state: 1.0`) |
| Installed RPM | **`gnome-shell-extension-appindicator-64-1.fc44.noarch`** |
| GNOME Shell | **50.4** |
| Session | **Wayland** |
| `IsStatusNotifierHostRegistered` | `true` |
| `ProtocolVersion` | `0` |

The watcher is owned by the **real GNOME Shell process**, not by a test
fixture. Every measurement below went through it.

```
$ busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
      org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems
as 1 "org.kde.StatusNotifierItem-2765240-1"

$ busctl --user status org.kde.StatusNotifierItem-2765240-1
PID=2765240   Comm=anyflowd   CommandLine=./target/debug/anyflowd --log info
```

**One registered item. It is AnyFlow's. `anyflowd` owns it.**

---

## 30. The real handshake, captured on the bus

This is §7's compatibility table stopping being a prediction. `dbus-monitor`
was running across a daemon restart, so the whole conversation between the
real extension and the real daemon was recorded. Reading `:1.14` as
gnome-shell and `:1.168` as `anyflowd`:

```
method call  :1.168 -> org.kde.StatusNotifierWatcher  /StatusNotifierWatcher
                       org.kde.StatusNotifierWatcher.RegisterStatusNotifierItem
                       string "org.kde.StatusNotifierItem-2761191-1"
method call  :1.14  -> :1.168   /StatusNotifierItem  org.freedesktop.DBus.Properties.GetAll
method call  :1.14  -> :1.168   /StatusNotifierItem  org.freedesktop.DBus.Introspectable.Introspect
method call  :1.14  -> :1.168   /StatusNotifierItem  org.freedesktop.DBus.Properties.Get
method call  :1.14  -> :1.168   /MenuBar             com.canonical.dbusmenu.GetLayout
method call  :1.14  -> :1.168   /MenuBar             com.canonical.dbusmenu.AboutToShow
method call  :1.14  -> :1.168   /MenuBar             com.canonical.dbusmenu.GetGroupProperties
```

Every call §5 and §7 predicted, in the predicted order, and nothing else. The
`Introspect` is the feature detection at `appIndicator.js:456`; AnyFlow
answers it with a document containing `Activate` and not
`XAyatanaSecondaryActivate`, which is the combination the extension was
written for.

The payloads, as they crossed the bus:

```
GetLayout(0, -1, ["type","children-display"])
  → uint32 1, (0, {'children-display': <'submenu'>}, [ <(1,…)>, <(2,…)>, <(3,…)> ])

AboutToShow(0)
  → boolean false

GetGroupProperties([1,2,3], [])
  → 1: {'enabled': true, 'label': "Quick Panel", 'visible': true}
    2: {'enabled': true, 'label': "Files",       'visible': true}
    3: {'enabled': true, 'label': "Settings",    'visible': true}
```

Revision `1`, root `children-display: submenu`, three rows with the mandated
types, `AboutToShow` truthfully `false`. **The real host asked exactly what the
modelled host in `tray_gnome.rs` asks, and got exactly the same answers.**

---

## 31. P1–P5 — the icon on the real panel

### How the indicator was observed

GNOME Shell will not describe its own UI here: `org.gnome.Shell.Screenshot`,
`org.gnome.Shell.Introspect.GetWindows` and `org.gnome.Shell.Eval` were all
re-tested in this session and all three still refuse (`AccessDenied`,
`AccessDenied`, `(false, '')`). What *does* work is **AT-SPI**: gnome-shell
publishes its own actor tree as an accessible application, so the panel can be
read directly from the Shell rather than inferred.

### P1 — the icon appears in the GNOME top panel ✅ PASS

```
[menu] 'AnyFlow'  extents=(2269, 1728, 36, 32)
                  states=['enabled','focusable','sensitive','showing','visible']
```

A GNOME `PanelMenu.Button` is exposed to AT-SPI with role `menu`, and its
accessible name is the SNI `Title`. So this node **is** the AnyFlow panel
indicator: 36×32 points, `showing` and `visible`.

That it is in the *top panel of the primary monitor* is not an assumption —
the monitor layout was read from Mutter:

```
LOGICAL MONITOR connector=eDP-1  model=CSW  pos=(572,1728)  scale=1.0  primary=true
LOGICAL MONITOR connector=HDMI-1 model=SAM  pos=(0,0)       scale=1.25 primary=false
```

The primary monitor's origin is `(572,1728)`; the panel bar runs
`(572,1728,1920,32)`; the AnyFlow indicator sits at `x=2269` inside it, in the
right-hand status area, immediately left of the keyboard and system menus
(`Teclado` at 2305, `Sistema` at 2355).

### P2 — exactly one AnyFlow indicator ✅ PASS

Every `role=menu` node in the entire Shell tree was enumerated:

```
  name=''                       ext=(1451, 0, 170, 32)      mapped=True
  name=''                       ext=(2891, 0, 40, 32)       mapped=True
  name=''                       ext=(2931, 0, 141, 32)      mapped=True
  name=''                       ext=(1449, 1728, 167, 32)   mapped=True
  name='AnyFlow'                ext=(2269, 1728, 36, 32)    mapped=True
  name='Clique de permanência'  mapped=False
  name='Acessibilidade'         mapped=False
  name='Teclado'                ext=(2305, 1728, 50, 32)    mapped=True
  name='Sistema'                ext=(2355, 1728, 137, 32)   mapped=True

total menus=9   AnyFlow menus=1
```

**One.** The secondary monitor's bar, drawn by the user's
`multi-monitors-bar` extension, carries no AnyFlow indicator, so not even a
mirrored copy exists.

One confound was chased down rather than waved away. A name search for
"AnyFlow" returns **three** nodes, which would look like duplication. Two of
them are `label`s, not indicators, and their siblings identify them
immediately:

```
siblings: ['Contatos','Meteorologia','Relógios','Mapas','Fedora Media Writer', …]
siblings: ['Ajuda','Analisador de uso de disco','AnyFlow','Arquivos','Boxes', …]
```

They are **app-grid icons in the overview** — AnyFlow's installed `.desktop`
entry, listed alphabetically among every other installed application, and
duplicated per monitor by `multi-monitors-bar`. Both are unmapped
(`extents = INT_MIN`). Neither is a tray item.

### P3 — Flow A is recognisable ✅ PASS · P4 — not generic, missing or broken ✅ PASS

There is no screenshot path on this machine (§31 opening paragraph), so the
icon was certified the way the Shell itself resolves it, plus a faithful
offline render.

**What the Shell resolves.** `IconName` is `io.github.yurisismotto.anyflow`
with an empty `IconThemePath`, so the extension builds
`new Gio.ThemedIcon({name})` and St resolves it through the session icon
theme. Reproducing that lookup from the Shell's own vantage point — a fresh
`Gtk.IconTheme` on the default display:

```
theme name: Adwaita
has_icon: True
  size=16 scale=1 -> ~/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
  size=16 scale=2 -> …/io.github.yurisismotto.anyflow.svg
  size=24 scale=2 -> …/io.github.yurisismotto.anyflow.svg
  size=32 scale=2 -> …/io.github.yurisismotto.anyflow.svg
  size=48 scale=1 -> …/io.github.yurisismotto.anyflow.svg

control (a name that does not exist) -> /usr/share/icons/Adwaita/scalable/status/image-missing.svg
```

That control line is what makes P4 a measurement instead of an assertion: a
name the theme cannot resolve **does** fall back to `image-missing`, and
AnyFlow's does not, at any panel size or scale.

**That the resolved file is the Flow A**, not the superseded Flowing A:

```
$ diff <installed svg> docs/design/assets/app-icon.svg      → identical
$ grep -o 'd="[^"]*"' <installed svg>   | head -2
d="anyflow-appicon"
d="M 53 56 C 50 41 43 22 32 8 C 22 21 15 36 10 47 C 7.5 52.5 11.5 56.5 15.5 52 …"
$ grep -o 'd="[^"]*"' docs/design/assets/logo-flow-a.svg | head -2
d="anyflow-flow-a"
d="M 53 56 C 50 41 43 22 32 8 C 22 21 15 36 10 47 C 7.5 52.5 11.5 56.5 15.5 52 …"
```

The ribbon path is **byte-identical to `logo-flow-a.svg`**, and does not match
`logo-flowing-a.svg`. `desktop/gui/tests/brand_assets.rs::the_primary_application_icon_is_the_flow_a`
already pins this in CI.

**How it reads at panel size.** Rendered with ImageMagick 7.1.2, which links
the same librsvg GTK uses, at 16/24/32/64 px on a dark panel ground and a
light one:

> `flow-a-panel-sheet.png` — the ribbon A is legible at every size. At 16 px
> the apex, the sweep and the tail are all still distinguishable; the plate
> stays a rounded square; the teal→indigo gradient survives. The 16 px render
> is not blank (`mean=0.382`, `alpha_mean=0.960`).

The Shell drew it into a 16×16 box inside the 36×32 indicator, which the
accessible tree confirms: `[panel] ext=(2279, 1736, 16, 16)`.

This is the same file, at the same sizes, through the same renderer — but it
is a render, not a photograph of the panel. Stated plainly so the reader can
weigh it.

### P5 — the indicator exposes exactly Quick Panel / Files / Settings ✅ PASS

The real extension built a real popup menu from AnyFlow's DBusMenu, and it is
in the Shell's accessible tree:

```
container child count: 3
container children roles: ['menu item', 'menu item', 'menu item']

  'Quick Panel'  states=['enabled','focusable','sensitive','visible']
  'Files'        states=['enabled','focusable','sensitive','visible']
  'Settings'     states=['enabled','focusable','sensitive','visible']
```

**Exactly three rows, all enabled, nothing else** — no Quit, no About, no
separator, no submenu.

There is a detail here that settles the provenance beyond argument: this
session's GNOME is in Portuguese, and every other Shell menu in the same tree
reads `Configurações`, `Suspender`, `Encerrar sessão…`. These three rows are
in **English**, because they did not come from GNOME. They came over
`com.canonical.dbusmenu` from `anyflowd`, and the extension rendered the
strings it was given.

---

## 32. How GNOME v64 actually presents the item

Read out of the **installed** package — `/usr/share/gnome-shell/extensions/…`,
the exact code running in pid 2752281 — rather than from upstream prose:

| Gesture | `indicatorStatusIcon.js` (installed v64) | What reaches AnyFlow | What AnyFlow does |
| --- | --- | --- | --- |
| **single left click** | `:440` `this._waitForDoubleClick()` → `:402` `this.menu.toggle()` after the double-click timeout | nothing on the bus | the DBusMenu opens |
| **double left click** | `:386` `_updateClickCount(event) === 2` → `this._indicator.open(x, y, time)` → `:802` `provideActivationToken` then `:803` `ActivateAsync` | `ProvideXdgActivationToken`, then `Activate(x,y)` | **Quick Panel** |
| **right click** | `:443` `this.menu.toggle()` | nothing on the bus | the DBusMenu opens |
| **middle click** | `:423` `BUTTON_MIDDLE` → `secondaryActivate(...)` → `:821` `provideActivationToken` then `SecondaryActivate` | `ProvideXdgActivationToken`, then `SecondaryActivate(x,y)` | deliberately nothing — §13 |
| **touch** | `:412` `TOUCH_BEGIN` → `this.menu.toggle()` | nothing on the bus | the DBusMenu opens |
| **scroll** | `:452` smooth scroll → `this._indicator.scroll(dx, dy)` | `Scroll(delta, 'horizontal'\|'vertical')` | deliberately nothing |

This confirms §10 against the installed binary, and adds the touch row, which
§10 did not have.

**The pointer gestures themselves could not be synthesised.** This was tested,
not assumed: `Atspi.generate_mouse_event(2287, 1744, 'b1c')` returns `True`
and produces **no** bus traffic, because the AT-SPI path reaches Xwayland and
the GNOME panel is a Wayland-native Shell actor. `org.gnome.Mutter.RemoteDesktop`
remains inhibited and there is no `ydotool`/`uinput` path. So what is certified
here is: the gesture→D-Bus mapping, from the installed source; and the
D-Bus→AnyFlow behaviour, measured live with the extension loaded. What is not
certified is a human fingertip, and the report does not claim it.

**No AnyFlow code was changed because GNOME's click semantics differ from
KDE's.** On GNOME the primary click opens the menu; on Plasma it activates.
Both are native to their host, both reach all three surfaces, and no code in
`src/tray/` asks which desktop is running.

---

## 33. The activation-token finding, re-certified — ✅ PASS, no code change

§15 recorded that GNOME calls `ProvideXdgActivationToken` on the **item**
immediately before a DBusMenu click (`dbusMenu.js:677`), while AnyFlow's menu
dispatch (`menu.rs:76`) passes `None`. This was re-tested with the real
extension loaded.

### The mechanism, confirmed on the bus

The two objects are separate. `item.rs:266` stores the token; `item.rs:100`
takes it on `Activate`/`SecondaryActivate`; `menu.rs:76` dispatches with
`None` and never reads the slot. To prove that rather than assert it, a marked
token was provided exactly as GNOME provides one, then a menu row was clicked:

```
→ ProvideXdgActivationToken("anyflow-cert-marker-MENU")   on /StatusNotifierItem
→ Event(1, "clicked", <int32 0>, 0)                        on /MenuBar

observed on the bus:
  ActivateAction("quick-panel", [], [])        ← platform-data EMPTY
```

The token was not forwarded. A later `Activate` on the item then showed where
it had gone:

```
  ActivateAction("quick-panel", [], [ {"activation-token": "anyflow-cert-marker-MENU"} ])
```

So the finding is exactly as described: **the menu path forwards no token, and
the token GNOME provides for a menu click stays in the item's slot.** In
normal GNOME use it is harmlessly overwritten, because the extension provides
a fresh token immediately before every `Activate` and `SecondaryActivate`
(`appIndicator.js:802`, `:821`).

### The measured consequence: none

The question §5 asks is whether the three surfaces open **and focus** without
a token. Measured from cold — no `anyflow-gui` process — for each row, with
the extension loaded:

| Row | `ActivateAction` | platform-data | Window | Focused? |
| --- | --- | --- | --- | --- |
| Quick Panel | `quick-panel` | empty | `AnyFlow` | **ACTIVE** ✅ |
| Files | `transfers` | empty | `AnyFlow Settings` | **ACTIVE** ✅ |
| Settings | `settings` | empty | `AnyFlow Settings` | **ACTIVE** ✅ |

And warm, with the GUI already running, also from the menu with no token:
`AnyFlow Settings` became `ACTIVE` immediately.

A control rules out the alternative explanation that cold launches simply
always focus here for unrelated reasons — `gapplication launch`, a path with
nothing to do with the tray, behaves the same. And AnyFlow is not doing
anything unusual to earn it: the GUI is started **by the message bus** into
its own transient unit (`dbus-:1.2-io.github.yurisismotto.anyflow@N.service`),
exactly as §15 measured.

**No focus failure was measured. Per the brief, PASS, and no code was
changed.** No speculative activation-token fix was made.

One observation is recorded for completeness rather than buried: the very
first cold trial of the evening produced a window that was `showing` and
`visible` but never `active`, while the Shell's own stage held the active
state — which is what happens when the overview is open. It did not reproduce
in any of the five controlled cold trials that followed. It is reported
because it happened, not because it is believed to be a defect.

A second observation, also recorded rather than acted on: a token left in the
item's slot by a menu click would be forwarded by a subsequent `Activate` from
a host that does **not** provide its own token first. GNOME and Plasma both
do, so this is unreachable on either certified host. It is noted in §35's
debts, not fixed here — fixing it would be exactly the speculative change the
brief forbids.

---

## 34. P13 and P14 on the real host

### P13 — daemon restart ✅ PASS

With the extension active and one indicator on the panel, only the AnyFlow
daemon was stopped and started. GNOME Shell was **not** restarted.

| Step | Watcher `RegisteredStatusNotifierItems` | Panel indicators | Menu rows |
| --- | --- | --- | --- |
| before | `1 "org.kde.StatusNotifierItem-2757865-1"` | 1 at `(2269,1728,36,32)` | Quick Panel / Files / Settings |
| daemon stopped | **`0`** | **0** | — |
| daemon restarted | `1 "org.kde.StatusNotifierItem-2761191-1"` | **1** at `(2269,1728,36,32)` | Quick Panel / Files / Settings |

The old item **disappeared** from the real watcher and from the real panel
tree; exactly one new item appeared; **no duplicate remained**. And the
registration happened exactly once for the new daemon:

```
$ grep -c 'member=RegisterStatusNotifierItem'   (across the restart window)
1
```

### P14 — no connected device ✅ PASS

Reached without revoking or unpairing anything: the tablet's Wi-Fi was turned
off over USB `adb` (`svc wifi disable`), and the daemon was then restarted so
no session could be held open. All five pairings survived
(`paired 5 device(s)`), and the trust store was not touched.

With **zero** connected peers:

```
RegisteredStatusNotifierItems   as 1 "org.kde.StatusNotifierItem-2765240-1"
AnyFlow panel indicators:       1 [(2269, 1728, 36, 32)]
AnyFlow menu rows:              ['Files', 'Quick Panel', 'Settings']

Status              'Active'          ← not NeedsAttention
IconName            'io.github.yurisismotto.anyflow'
AttentionIconName   ''
AttentionMovieName  ''
OverlayIconName     ''
ToolTip             ('', [], 'AnyFlow', 'One flow. Any device.')
```

* the indicator remains present and healthy ✅
* `Status` is **not** abused as `NeedsAttention` ✅ — and cannot be: the
  source sets it as a constant, and `model.rs:87` records that `Passive` "is
  never sent" either
* the menu is still Quick Panel / Files / Settings, all `enabled` ✅
* no flashing or error state: `AttentionIconName` and `AttentionMovieName`
  are both empty, so the extension has nothing to flash with ✅

The daemon was also honest about the peer it had lost. Before the restart it
reported the stale session as exactly that, rather than claiming health:

```
SM-X620  6532889e…   connected yes   state stale
```

Wi-Fi was then restored, and the connection came back **by itself** — see
P17 in §36.

---

## 35. Watcher lifecycle, certified by an accident

The most valuable evidence of this sprint was not planned. Partway through,
**the screen locked and was unlocked** — `screenShield.js` / `unlockDialog.js`
in the journal at 11:53:58 and 11:54:32. GNOME Shell disables user extensions
while the lock screen is up, so the appindicator extension called `disable()`,
which unowns `org.kde.StatusNotifierWatcher`, and on unlock called `enable()`,
which owns it again:

```
11:53:58  NameOwnerChanged  org.kde.StatusNotifierWatcher  ':1.14' -> ''
11:54:32  NameOwnerChanged  org.kde.StatusNotifierWatcher  ''     -> ':1.14'
```

AnyFlow's own log across that window, unprompted and unassisted:

```
15: INFO …tray::watcher: registered an AnyFlow tray item with the desktop shell
17: INFO …tray::watcher: the desktop tray host went away; the AnyFlow tray item
                          stays published and will re-register when one returns
18: INFO …tray::watcher: registered an AnyFlow tray item with the desktop shell
```

A real host vanished and came back, and AnyFlow **kept its item published,
said so truthfully, and re-registered exactly once** — 34 seconds later, at
the instant the name returned, which also demonstrates the loop is
event-driven rather than polled. That is gates **G6, G7, G8** and tests
`d2`/`d5`/`n3`/`n4` certified against the real GNOME Shell by a real event
rather than a fixture.

---

## 36. Remaining physical regression, with the extension in the path

| Gate | Result | Evidence measured now |
| --- | --- | --- |
| P6 Quick Panel opens | ✅ PASS | cold → `ActivateAction("quick-panel")` → window `AnyFlow` **ACTIVE** |
| P7 Files opens the correct surface | ✅ PASS | cold → `ActivateAction("transfers")` → `lib.rs:523` `show_settings(Some(Page::Files))` → window `AnyFlow Settings` **ACTIVE** |
| P8 Settings opens | ✅ PASS | cold → `ActivateAction("settings")` → window `AnyFlow Settings` **ACTIVE** |
| P9 GUI cold activation | ✅ PASS | 5 cold trials; each started by the bus into `dbus-:1.2-io.github.yurisismotto.anyflow@N.service` |
| P10 existing-instance activation | ✅ PASS | 4 activations in a row, `gui pids` stayed `2763571`, count **1** |
| P11 closing the GUI leaves the tray item | ✅ PASS | after `pkill -x anyflow-gui`: item on bus **1**, watcher list **1**, `Title` still answers, panel indicator still **1** |
| P12 the daemon owns the tray item | ✅ PASS | `PID=2765240 Comm=anyflowd` |
| P15 no private content | ✅ PASS | closed string set below |
| P16 no registration-loop spam | ✅ PASS | §37 |
| P17 SM-X620 reconnect healthy | ✅ PASS | Wi-Fi restored → reconnected unaided within 30 s, `state connected`, `last frame 23s ago`, `battery 79% (23s old)`, grants intact |
| P18 files/clipboard/notifications unaffected | ✅ PASS | all four capabilities `ready`; battery flowing from the peer |

**P15 in full.** Every string readable on both objects by any process on the
session bus — `GetAll` on the item, `GetAll` on the menu, `GetLayout`, and
`GetGroupProperties` over every id — is one of:

```
"" · Active · AnyFlow · ApplicationStatus · AttentionIconName ·
AttentionIconPixmap · AttentionMovieName · Category · children-display ·
enabled · Files · IconName · IconPixmap · IconThemePath · Id ·
io.github.yurisismotto.anyflow · ItemIsMenu · label · Menu · /MenuBar ·
normal · One flow. Any device. · OverlayIconName · OverlayIconPixmap ·
Quick Panel · Settings · Status · submenu · Title · ToolTip · Version ·
visible · WindowId
```

Property names and a closed set of constants. **No filename, no path, no URI,
no clipboard text, no notification content, no peer name, no device id, no
fingerprint, no IP address, no battery level** — all of which the daemon
does know, and none of which the tray exposes.

---

## 37. Log audit on the real host

The certification daemon's complete log, over a period that included a
registration, a real watcher loss and recovery, a no-peer window and a
reconnect:

```
16 lines total
 0 WARN
 0 ERROR
 1 × "registered an AnyFlow tray item with the desktop shell"
```

No duplicate registrations. No watcher loop. No D-Bus failures. No activation
failures. The two tray lines contain only the item's bus name — no clipboard,
file or notification content anywhere in them.

**No busy loop**, measured directly from `/proc`:

```
delta ticks over 30 s: 4     →  0.04 CPU-seconds  ≈ 0.13 %
```

**Bus quiet at rest**: with the indicator drawn and the item registered, the
last 200 lines of the session-bus trace contain **1** line mentioning the
AnyFlow item or its menu. The extension is not polling it and it is not
polling the extension.

### One thing the Shell did log

For completeness, because it is real and it names the extension:

```
gnome-shell[2752281]: Object Gjs_appindicatorsupport_rgcjonas_gmail_com_indicatorStatusIcon_IndicatorStatusIcon
  (0x…), has been already disposed — impossible to access it. This might be caused by
  the object having been destroyed from C code using something such as destroy(),
  dispose(), or remove() vfuncs.
```

Four occurrences, each coinciding exactly with an `IndicatorStatusIcon` being
destroyed — three when the AnyFlow item's bus name went away on a daemon stop
(11:48:18, 11:51:12, 12:01:59) and one when the extension itself was disabled
by the screen lock (11:53:58).

Assessment: this is a **GJS disposal-ordering warning inside the extension's
own JavaScript**, on its teardown path. It is not an AnyFlow D-Bus failure —
no call from AnyFlow failed, no call to AnyFlow failed, and after every one of
those four events the panel ended up with exactly one correct indicator and
the correct three-row menu. There is nothing in the SNI or DBusMenu contract
AnyFlow could do differently to prevent a host from touching its own disposed
object. **Recorded as an upstream observation, not as an AnyFlow defect, and
not acted on.**

---

## 38. Evidence index

Bus and tree evidence captured this session, outside the repository (nothing
was added to the working tree):

| Evidence | What it shows |
| --- | --- |
| `p13.log` | full session-bus trace across the P13 restart, the lock/unlock watcher cycle and every activation |
| `gnome-cert2.log` | the daemon log containing the watcher-lost / re-registered pair |
| `p14.log` | the no-peer daemon's complete 16-line log |
| `pre-logout-daemon.log` | the pre-logout daemon's truthful "the session bus closed" lines |
| `flow-a-panel-sheet.png` | the Flow A rendered at 16/24/32/64 px on dark and light panel grounds |

No screenshot of the panel exists, because this machine has no screenshot path
at all — `org.gnome.Shell.Screenshot` refuses, `Introspect` refuses, `Eval` is
off, and `import -window root` cannot work on a rootless Xwayland. All three
refusals were re-tested this session rather than carried over from the earlier
report. The AT-SPI extents, states and names in §31 are the substitute, and
they come from the Shell itself.

---

## 39. Final gate matrix

| Gate | Verdict | Certified against |
| --- | --- | --- |
| EXISTING SNI GNOME COMPATIBILITY | **PASS** | real extension v64 in `gnome-shell` pid 2752281 — §30 |
| REAL WATCHER REGISTRATION | **PASS** | owner `:1.14` = gnome-shell; `RegisteredStatusNotifierItems` = AnyFlow's item — §29 |
| GNOME INDICATOR VISIBILITY | **PASS** | one `[menu] 'AnyFlow'` at `(2269,1728,36,32)` on the primary panel — §31 |
| FLOW A GNOME RENDERING | **PASS** | theme resolves to the Flow A SVG; `image-missing` control negative — §31 |
| DBUSMENU ON GNOME | **PASS** | the real extension's own `GetLayout`/`AboutToShow`/`GetGroupProperties`, captured on the bus — §30; three rendered rows — §31 |
| GNOME CLICK SEMANTICS | **PASS** | installed v64 handler mapped gesture-by-gesture — §32 |
| ACTIVATION TOKEN | **PASS** — no code change | token provably not forwarded, all three surfaces still focus from cold — §33 |
| QUICK PANEL ACTIVATION | **PASS** | cold + warm, extension loaded — §33, §36 |
| FILES ACTIVATION | **PASS** | `transfers` → `show_settings(Some(Page::Files))` — §36 |
| SETTINGS ACTIVATION | **PASS** | cold + warm — §36 |
| GUI COLD ACTIVATION | **PASS** | 5 cold trials, bus-activated transient unit — §36 |
| GUI NON-RESIDENT LIFECYCLE | **PASS** | item and panel indicator survive `pkill -x anyflow-gui` — §36 |
| DAEMON RESTART | **PASS** | watcher `1 → 0 → 1`, exactly one registration, no duplicate — §34 |
| NO CONNECTED DEVICE | **PASS** | `Status Active`, no attention icon, menu intact — §34 |
| WATCHER LIFECYCLE | **PASS** | real lock/unlock took the host away and brought it back; one re-registration — §35 |
| STOCK GNOME NON-REGRESSION | **PASS** | §20 (no host) and §36–§37 (host present) |
| NO NEW TRAY DEPENDENCY | **PASS** | `Cargo.lock` unchanged; `n17` |
| SECURITY / PRIVACY | **PASS** | closed string set re-measured on the real host — §36 |
| PORTABLE WINDOWS BOUNDARY | **PASS** | no portable crate touched |
| DESKTOP TEST GATES | **PASS** | 971 passed, fmt, clippy, display-gated GUI — unchanged, no source touched |

---

## Verdict

> ### GNOME APPINDICATOR V1:
> ### NO PRODUCT CODE CHANGE REQUIRED — HOST INTEGRATION CERTIFIED

Both halves are now earned. **No product code change was required**: not one
file under `desktop/*/src/`, `desktop/proto/` or `android/` was modified, and
`desktop/Cargo.lock` is byte-identical. **And host integration is certified**:
a real GNOME Shell 50.4 on Wayland, running the Fedora `v64` AppIndicator
extension, owns `org.kde.StatusNotifierWatcher`, has AnyFlow's item as the one
item registered with it, draws the Flow A in the primary top panel, and
renders a three-row menu it built from AnyFlow's own DBusMenu. P1–P18 all
pass.

**This is not an implementation change.** PR #37 supplied the implementation.
What this sprint supplied is evidence — an audit, 18 new deterministic tests,
and now a physical certification against the real host.

Two things are worth carrying forward from the real host that no fixture had
produced. A screen lock took the watcher away mid-sprint and gave it back, and
AnyFlow kept its item published, said so truthfully, and re-registered exactly
once (§35). And the activation-token finding was re-tested rather than
patched: the token is provably not forwarded on the menu path, and all three
surfaces still open **and take focus** from cold, so nothing was changed
(§33).

**The KDE StatusNotifier implementation is also the GNOME AppIndicator
implementation. No parallel tray backend is required. Duplication is not
progress.**
