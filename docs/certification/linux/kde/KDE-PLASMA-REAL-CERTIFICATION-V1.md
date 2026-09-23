# AnyFlow — KDE Plasma Real Host Certification v1

**Branch:** `test/kde-plasma-certification-v1`
**Base:** `develop` after merged PR #38 (GNOME AppIndicator real-host certification)
**Date:** 2026-09-18 (Part I) · 2026-09-21 (Part II)
**Verdict:** **PASS.** No KDE-specific product defect was found and no product source
file was changed. Part I certified the tray/shell half on a real Plasma host; Part II
(§32–§43) closed the six remaining Android↔KDE capability debts — notifications N1–N7,
the Android → KDE accepted file path, clipboard both directions, the connected Quick
Panel, a controlled lock/unlock cycle and a full Plasma session restart. Final matrix in
§42, verdict in §43.

---

## 1. Baseline

```console
$ git branch --show-current          test/kde-plasma-certification-v1
$ git status --short                 ?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
$ git diff --check                   (clean)
$ git log -5 --oneline
81cd4d6 Merge pull request #38 from yurisismotto/feature/gnome-appindicator-v1
aa2a898 test(linux): certify GNOME AppIndicator compatibility
5195692 Merge pull request #37 from yurisismotto/feature/kde-statusnotifier-v1
cc2218c feat(linux): add KDE StatusNotifier integration
0b2e319 Merge pull request #36 from yurisismotto/feature/android-branding-files-ux-v1
```

PR #38 is in ancestry. `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not read, modified or staged.

**What this sprint had to close.** `KDE-STATUSNOTIFIER-V1.md` §21–§22 recorded
`KDE REAL-SHELL CERTIFICATION = BLOCKED` — "No KDE Plasma session exists on this machine,
and none was installed… the fake watcher in §13 and §18 exercises the *protocol*, and a
protocol peer is not a shell." Everything below is against a real Plasma shell.

---

## 2. VM provenance

| Item | Value |
|---|---|
| Source | `https://dl.fedoraproject.org/pub/fedora/linux/releases/44/KDE/x86_64/iso/` (official Fedora infrastructure) |
| Image | `Fedora-KDE-Desktop-Live-44-1.7.x86_64.iso` |
| Size | 3 368 683 520 bytes (exact match to manifest) |
| SHA256 | `c8295961d4c41adbf785a31a17c21a971d3b7415fda72dcad0c11c49577bf03a` — `sha256sum -c` → **SUCESSO** |
| Manifest | `Fedora-KDE-44-1.7-x86_64-CHECKSUM`, GPG clearsigned |
| Signature | **Valid** — "Fedora (44) <fedora-44-primary@fedoraproject.org>", key `36F6 12DC F27F 7D1A 48A8 35E4 DBFC F71C 6D9F 90A6` |
| Architecture | x86_64 |

No third-party mirror was used. The copy staged into the libvirt pool was re-verified
byte-for-byte after upload (same SHA256).

**VM:** `anyflow-f44-kde` — did not exist beforehand (`virsh list --all` showed only the
pre-existing, powered-off `anyflow-d13`, `anyflow-u2404`, `anyflow-u2604`, which stayed off
for the whole run). Created with 4 vCPU, 4 GiB RAM, 32 GiB qcow2, `domain type='kvm'`,
`cpu host-passthrough`.

### Installation note (environment, not product)

Fedora 44 **Live** media refuses kickstart. Anaconda states it outright:

> *"Configuration not supported — Kickstart is not supported on Live ISO installs, please
> use netinstall or standard ISO. This installation will continue interactively."*

Because the certification LAN carries no route off-segment by design (§6), a netinstall was
not an option. The guest was therefore installed by copying the live root filesystem to
`/dev/vda1` and completing the install (fstab, users, bootloader, non-live initramfs) from
inside the live session. Three traps are worth recording for the packaging sprint:

* the live rootfs BLS entries point at Fedora's **kiwi build tree**
  (`/root/var/lib/mock/f44-kiwi-build-…/image-root/boot/vmlinuz-…`) and must be rewritten to
  `/boot/vmlinuz-<kver>`;
* `/etc/plasmalogin.conf` ships `Autologin User=liveuser`, which silently falls back to the
  greeter once `liveuser` is removed — Fedora 44 KDE uses **plasmalogin**, not SDDM, so an
  `sddm.conf.d` drop-in is ignored;
* Fedora 44 KDE's only session is Wayland (`/usr/share/wayland-sessions/plasma.desktop`;
  `/usr/share/xsessions/` is empty), which is what the brief prefers anyway.

None of this touches AnyFlow.

---

## 3. Fedora KDE version

```console
$ cat /etc/os-release
NAME="Fedora Linux"
VERSION="44 (KDE Plasma Desktop Edition)"
VERSION_ID=44
VARIANT="KDE Plasma Desktop Edition"

$ uname -a
Linux anyflow-kde 6.19.10-300.fc44.x86_64 #1 SMP PREEMPT_DYNAMIC
Wed Mar 25 18:23:49 UTC 2026 x86_64 GNU/Linux
```

---

## 4. Plasma version

Measured from the running session, not inferred from package names:

```console
$ plasmashell --version      plasmashell 6.6.4
$ kwin_wayland --version     kwin 6.6.4
```

Corroborating RPM versions:

```console
plasma-workspace-6.6.4-1.fc44.x86_64
kwin-6.6.4-2.fc44.x86_64
kf6-kcoreaddons-6.25.0-1.fc44.x86_64      (KDE Frameworks 6.25.0)
qt6-qtbase-6.10.2-2.fc44.x86_64           (Qt 6.10.2)
```

---

## 5. Session type

```console
$ loginctl show-session 10 -p Type -p Class -p Active -p State -p Desktop -p Seat -p Remote
Seat=seat0
Remote=no
Desktop=KDE
Type=wayland
Class=user
Active=yes
State=active
```

Read from the live `plasmashell` process environment (`/proc/<pid>/environ`), so it is the
session the shell actually runs in:

```
XDG_CURRENT_DESKTOP=KDE
XDG_SESSION_DESKTOP=KDE
XDG_SESSION_TYPE=wayland
DESKTOP_SESSION=/usr/share/wayland-sessions/plasma.desktop
KDE_FULL_SESSION=true
KDE_SESSION_VERSION=6
WAYLAND_DISPLAY=wayland-0
```

**Real KDE Plasma 6, real graphical session, Wayland.**

---

## 6. VM networking

The host has **no wired carrier** (the AX88179 USB NIC is present but `NO-CARRIER`,
`Link detected: no`) and no Ethernet cable was available, so the macvtap model used by the
earlier distro VMs was not reachable. Certifying through NAT, a relay, socat, a proxy, port
forwarding, an mDNS reflector or adb forwarding was explicitly out of scope, so a **real L2
test LAN was built on the host's own Wi-Fi radio**.

### Radio feasibility (audited read-only before any change)

| # | Item | Finding |
|---|---|---|
| 1 | Interface | `wlp0s20f3`, `phy0`, Intel Raptor Lake CNVi, `iwlwifi` |
| 2 | Interface combinations | `#{managed}<=1, #{AP,P2P-client,P2P-GO}<=1, #{P2P-device}<=1, total<=3, #channels<=1` |
| 3 | Concurrent managed + AP | **Supported** (AP is in `Supported interface modes`; combination 2 admits managed+AP+P2P-device = 3) |
| 4 | Same channel required | **Yes — `#channels <= 1`** |
| 5 | Channel at audit time | **100 (5500 MHz)**, 40 MHz, BSSID `60:33:4b:e2:a2:14` |
| 6 | AP vif while connected | Creatable, but **cannot beacon on ch 100** |
| 7 | Tooling | `dnsmasq` 2.92, `iw`, `wpa_supplicant`, `bridge` present; **`hostapd` absent** |

The blocker was regulatory, not combinatorial. The radio is self-managed
(`country BR: DFS-UNSET`) and channel 100 reports:

```
* 5500.0 MHz [100] (22.0 dBm) (no IR, radar detection)
```

`no IR` forbids initiating radiation, i.e. beaconing. All of 5 GHz UNII-1 (36–48) was
likewise `(no IR)`. **2.4 GHz channels 1–13 carry no such flag.** The same SSID had a strong
2.4 GHz BSS on **channel 11** (`60:33:4B:E2:A2:13`, signal 85) from the same Time Capsule, so
moving the managed link there satisfied `#channels <= 1` without leaving the network.

### Topology as built

```
br-anyflow-kde  192.168.77.1/24   (host: DHCP only — NO gateway, NO DNS, NO NAT)
├── ap-anyflow   AP vif on phy0, ch 11, WPA2-PSK/CCMP, ap_isolate=0
│                 └── SM-X620        192.168.77.36
└── vnet3 (tap)  anyflow-f44-kde NIC
                  └── anyflow-kde    192.168.77.97
```

Requested evidence, item by item:

1. **Host managed interface** — `wlp0s20f3`, SSID `Rede Wi‑Fi de Ida`, BSSID
   `60:33:4b:e2:a2:13`, **ch 11 (2462 MHz)**, −51 dBm, IP 192.168.0.118 (host stayed online).
2. **AP interface** — `ap-anyflow`, `type AP`, MAC `02:d5:5d:a9:63:c5`, hostapd 2.11
   (`AP-ENABLED`), run in the foreground only, **never enabled at boot**
   (`systemctl is-enabled hostapd` → `disabled`).
3. **Effective channel of both** — **11** for the managed link and the AP.
4. **Bridge / tap** — `br-anyflow-kde`; members `ap-anyflow` and `vnet3`.
5. **VM IP** — `192.168.77.97` (DHCP, MAC `52:54:00:aa:dc:11`).
6. **SM-X620 IP** — `192.168.77.36` (DHCP, lease name `Tab-S10-FE-de-Yuri`).
7. **Direct Android↔VM connectivity** —
   ```console
   (tablet) $ ping -c 3 192.168.77.97
   3 packets transmitted, 3 received, 0% packet loss
   rtt min/avg/max/mdev = 11.038/13.735/15.258/1.914 ms
   ```
   and, at the application layer, the daemon logged the tablet reaching it directly:
   `session established device=6532889e… peer=573C CB84 DA6C 993B`, with
   `ss -tnp` on the guest showing the single flow
   `192.168.77.97:55432 ↔ 192.168.77.36:53664`. mDNS ran natively over the bridge
   (`advertising _anyflow._tcp.local. port=55432`).
8. **Absence of NAT** —
   * `net.ipv4.ip_forward = 0` (verified before *and* after every step; firewalld's zone
     assignment flipped it to 1 once and it was immediately set back to 0);
   * no `masquerade`/`snat`/`dnat` statement anywhere in `nft list ruleset`;
   * dnsmasq serves **DHCP only** — `port=0`, `dhcp-option=3` and `dhcp-option=6` both empty;
   * consequently the tablet's routing table has **no default route at all**:
     ```console
     (tablet) $ ip route
     192.168.77.0/24 dev wlan0 proto kernel scope link src 192.168.77.36
     ```

   With forwarding off and no gateway offered, traffic cannot leave the segment. Android and
   the KDE guest are on one L2 broadcast domain and talk to each other directly.

**Host changes made, all reversible, none permanent** — see §28 for the restore procedure.
The Wi-Fi profile was pinned to the 2.4 GHz BSSID; `hostapd` was installed (`dnf`, with
approval) and run in the foreground; `br-anyflow-kde` was placed in firewalld's `libvirt`
zone **at runtime only** (`--permanent` list is empty) so the host's DHCP server could
answer — without that, DISCOVERs were dropped by the `FedoraWorkstation` zone.

---

## 7. Real watcher owner

No fake or private watcher was used anywhere in this report.

```console
$ busctl --user list | grep -i StatusNotifier
org.kde.StatusNotifierHost-2428     2428 plasmashell  anyflow :1.41  user@1000.service
org.kde.StatusNotifierWatcher       2403 kded6        anyflow :1.37  user@1000.service

$ busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
        org.freedesktop.DBus GetNameOwner s org.kde.StatusNotifierWatcher
s ":1.37"

$ busctl --user call … GetConnectionUnixProcessID s org.kde.StatusNotifierWatcher
u 2403

$ busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
        org.kde.StatusNotifierWatcher IsStatusNotifierHostRegistered
b true
```

| Property | Value |
|---|---|
| Well-known name | `org.kde.StatusNotifierWatcher` |
| Unique owner | `:1.37` |
| Owning process | **PID 2403, `kded6`** (KDE's own daemon), unit `user@1000.service` |
| Registered host | `org.kde.StatusNotifierHost-2428` → **PID 2428, `plasmashell`** |
| `IsStatusNotifierHostRegistered` | `true` |

---

## 8. SNI registration

```console
$ busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
        org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems
as 2 ":1.74/StatusNotifierItem" "org.kde.StatusNotifierItem-4325-1/StatusNotifierItem"
```

Two items are registered; **exactly one of them is AnyFlow.** The other was identified
rather than assumed:

```console
$ busctl --user get-property :1.74 /StatusNotifierItem org.kde.StatusNotifierItem Id
s "Xwayland Video Bridge_pipewireToXProxy"
$ … GetConnectionUnixProcessID s :1.74   →   u 2702   →   /usr/bin/xwaylandvideobridge
```

i.e. a stock Fedora KDE component, not a duplicate AnyFlow item.

AnyFlow's item, owned by `anyflowd` (PID 4325):

| Property | Value | Gate |
|---|---|---|
| Object path | `/StatusNotifierItem` | required path |
| `Id` | `io.github.yurisismotto.anyflow` | official APP_ID |
| `Title` | `AnyFlow` | |
| `Category` | `ApplicationStatus` | |
| `Status` | `Active` | truthful |
| `IconName` | `io.github.yurisismotto.anyflow` | resolves through APP_ID |
| `ItemIsMenu` | `false` | |
| `Menu` | `/MenuBar` | required path |
| `WindowId` | `0` | daemon owns it, no window |

Daemon log at registration:

```
anyflow_linux::tray: AnyFlow tray item published on the session bus
                     item=org.kde.StatusNotifierItem-4325-1
anyflow_linux::tray::watcher: registered an AnyFlow tray item with the desktop shell
                     item=org.kde.StatusNotifierItem-4325-1
```

No duplicate item, one registration.

---

## 9. Visual tray evidence

Captured from the running Plasma panel (`virsh screenshot`, cropped and magnified):

* **K1 — the AnyFlow tray icon appears** in the Plasma system tray, between the
  notifications bell and the volume icon. **PASS**
* **K2 — exactly one AnyFlow tray item.** One icon on screen; one AnyFlow entry on the bus
  (§8). **PASS**
* **K3 — the Flow A is recognisable.** The drawn mark is a single continuous stroked ribbon
  forming an "A" — the **Flow A** geometry (`logo-flow-a.svg`, the current identity mark and
  app icon), not the superseded filled "Flowing A" letterform. **PASS**
* **K4 — not missing, not generic.** Full-colour teal→violet gradient mark on the panel's
  badge, clearly distinct from the monochrome system icons; no generic/placeholder glyph.
  **PASS**
* **K5 — rendering acceptable at Plasma tray size.** Crisp at the panel's native icon size;
  the stroke and the crossbar terminal stay legible. **PASS**
* **K6 — no KDE-specific artwork was required.** The same `io.github.yurisismotto.anyflow`
  name installed by the repository installer resolves through the hicolor theme. **PASS**

Plasma also rendered the item's tooltip from the SNI `ToolTip` property, which is independent
confirmation that the shell is reading AnyFlow's own metadata:

> **AnyFlow** — *One flow. Any device.*

This is the gate the previous sprint had to leave open ("whether Plasma draws it has not been
seen"). It has now been seen.

---

## 10. Flow A result

**PASS**, and this time for rendering as well as identity. Identity/metadata were re-verified
here (`Id`, `IconName`, the installed hicolor SVG, the tooltip), and the mark is drawn by a
real Plasma shell (§9). The installer used was the repository-supported one — no icon or
`.desktop` file was hand-copied:

```console
$ ./install-desktop-metadata.sh --link-binary ~/.local/bin/anyflow-gui
installed /home/anyflow/.local/share/applications/io.github.yurisismotto.anyflow.desktop
installed /home/anyflow/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg
installed /home/anyflow/.local/share/dbus-1/services/io.github.yurisismotto.anyflow.service
```

---

## 11. Click semantics

All of the following were driven by **real pointer input inside the guest** (uinput →
libinput → KWin), not by calling AnyFlow's methods directly. Pointer placement was closed-loop
(measured against the live cursor, confirmed by Plasma's own tooltip appearing on the item).

| Gate | Expectation | Result |
|---|---|---|
| **K7** | primary/left click performs the native expected action — Quick Panel | **PASS** — the Quick Panel opened |
| **K8** | context/right click exposes exactly Quick Panel, Files, Settings | **PASS** — Plasma rendered exactly those three rows, nothing else |
| **K9** | Quick Panel opens | **PASS** (§23) |
| **K10** | Files opens the correct Settings/Files surface | **PASS** — opened the Settings window on **Files → Transfers** |
| **K11** | Settings opens | **PASS** — with a window already open, Settings raises the existing window rather than opening a second one |
| **K12** | cold activation with no `anyflow-gui` process | **PASS** — see the finding below |
| **K13** | warm activation reuses one process | **PASS** — PID stayed `9683` across Quick Panel → Files → Settings; never more than one process |
| **K14** | closing the GUI leaves the tray item alive | **PASS** — closing the last window ended `anyflow-gui`; `anyflowd` (4325) stayed and its item stayed `Active` and registered |

Interaction semantics were **not** modified to suit Plasma; the same primary-click-opens-the-
Quick-Panel behaviour certified on GNOME was exercised unchanged.

### Finding (environment, not a product defect) — cold D-Bus activation needs the bus to rescan

The first cold left click did reach AnyFlow. Plasma delivered `Activate`, the daemon mapped it
to the Quick Panel action, and the *activation* failed:

```
anyflow_linux::tray::item: could not present the AnyFlow window the tray asked for
                           action="quick-panel"
                           reason=org.freedesktop.DBus.Error.ServiceUnknown
```

Cause, measured rather than guessed: the session bus started at **19:01:34** and the D-Bus
service file was installed at **19:14:03**, so the bus had never scanned it —
`busctl --user list --activatable` did not contain the name. After one
`org.freedesktop.DBus.ReloadConfig` the name became activatable and the same cold click
started `anyflow-gui --gapplication-service` and opened the Quick Panel.

The click routing, the tray action mapping, the service file and its `Exec` were all correct;
what was missing was a bus rescan after a mid-session install. **This is a packaging
requirement, not a KDE defect**, and it is the single most useful thing this sprint hands to
the Linux packaging sprint: an RPM/deb that drops
`io.github.yurisismotto.anyflow.service` into a live session must ensure the bus picks it up
(a fresh login does it; so does a reload). AnyFlow's own behaviour when activation fails is
correct and quiet — one truthful log line, no crash, no retry storm.

---

## 12. DBusMenu

Plasma consumes `com.canonical.dbusmenu` on `/MenuBar`. Layout read from the live bus:

```console
$ busctl --user call org.kde.StatusNotifierItem-4325-1 /MenuBar \
        com.canonical.dbusmenu GetLayout iias 0 10 0
u(ia{sv}av) 1 0 1 "children-display" s "submenu"
  3 (ia{sv}av) 1 3 "enabled" b true "label" s "Quick Panel" "visible" b true 0
    (ia{sv}av) 2 3 "enabled" b true "label" s "Files"       "visible" b true 0
    (ia{sv}av) 3 3 "enabled" b true "label" s "Settings"    "visible" b true 0

$ … get-property … com.canonical.dbusmenu Version   →  u 3
$ … get-property … com.canonical.dbusmenu Status    →  s "normal"
```

Exactly three rows — **Quick Panel, Files, Settings** — all enabled and visible, and the
rendered Plasma menu matched the protocol exactly (§11, K8).

Absent, as required: **no Pair, no Revoke, no Grant, no Quit daemon, no Send clipboard, no
Send file.** The tray remains a navigation surface, not an authority surface. **PASS**

---

## 13. GUI cold/warm activation

* **Cold** — with zero `anyflow-gui` processes, a tray click started
  `/home/anyflow/.local/bin/anyflow-gui --gapplication-service` (PID 9683) through D-Bus
  activation and presented the Quick Panel. **PASS** (after the bus rescan described in §11).
* **Warm** — Quick Panel → Files → Settings all reused **PID 9683**. At no point did a second
  GUI process exist. **PASS**
* **Non-resident** — closing the last AnyFlow window ended the process within seconds, while
  `anyflowd` and the tray item continued. **PASS**

---

## 14. Android pairing

Physical **SM-X620, Android 16** (serial `RX2Y500C7SY`, `ro.product.model=SM-X620`), joined to
the certification SSID over the air (`anyflow-cert`, −31 dBm) and addressed `192.168.77.36`.
A **fresh** pairing was performed — the desktop's trust store started empty
(`paired 0 device(s)`) and no identity or trust state was copied from any other machine.

| Gate | Result |
|---|---|
| **A1** discovery works on LAN | **PASS** — mDNS `_anyflow._tcp.local.` advertised over the bridge; the tablet reached `192.168.77.97:55432` directly |
| **A2** QR pairing works | **PASS** — optical scan of the QR rendered from the daemon's own payload |
| **A3** TLS/SPKI trust succeeds | **PASS** — "A device proved it holds the pairing code"; proof-of-possession completed and the SPKI fingerprint pinned |
| **A4** device becomes connected | **PASS** — `state connected` on the desktop, "Connected to anyflow-kde" on the tablet |
| **A5** daemon and GUI show the correct peer | **PASS** — daemon lists the peer; the tablet names `anyflow-kde` and shows the desktop's fingerprint `ADD6 9AA1 613A 38D7` on its incoming-file card |
| **A6** reconnect after normal disconnect | **PASS** — reconnected after a peer-initiated disconnect, after a capability grant forced a re-handshake, and after a full daemon restart (§20) |

```console
$ anyflow status
  device      anyflow-kde (0a665c55d7f8a153fdf002b5a8878706)
  fingerprint ADD6 9AA1 613A 38D7
  paired      1 device(s)
  devices:
    SM-X620  6532889e82ba83d0782cc644e7a21fc3
       platform    android
       fingerprint 573C CB84 DA6C 993B
       paired      yes
       connected   yes
       state       connected
       granted     battery.v1, clipboard.v1, files.v1, notifications.v1
```

The pairing token/proof is deliberately not reproduced in this report.

**Operational note.** The first scan was consumed and **declined** because the confirmation
prompt read no answer — `anyflow pair` needs an answer already queued on a durable stdin. The
code is single-use, so that attempt was spent and a second window was opened with the answer
pre-loaded. This is a test-harness lesson, not a product behaviour: the CLI's default
(decline when unanswered) is the safe one.

---

## 15. Files

| Gate | Result |
|---|---|
| **F1** Android → KDE transfer | **NOT CERTIFIED** — offer reached the desktop and was held correctly, but was never accepted; see below |
| **F2** KDE → Android transfer | **PASS** — `Sent. kde-to-android.txt`, 86.5 KiB, 100 % |
| **F3** byte integrity / SHA256 | **PASS** — `ac46dd6c656eae353810f50305c2e048f45bcc51c5268124f8fa005b3d30e015` identical on both sides |
| **F4** KDE incoming approval works | **NOT CERTIFIED** — the approval dialog was never observed (guest rendering degraded, §22) |
| **F5** decline works | **PASS** — an unaccepted offer ends `cancelled / reason: declined by the user` |
| **F6** transfer appears correctly in GUI | **NOT CERTIFIED** |
| **F7** no duplicate transfer | **PASS** — each attempt is one distinct id (`548a848f`, `70e751da`, `00716e17`, `eedaff81`); no duplicates |
| **F8** received file lands in the configured location | **PASS (Android side)** — landed in `/sdcard/Download/AnyFlow/`; the desktop's configured `download_dir=/home/anyflow/Downloads/AnyFlow` was never exercised because F1 did not complete |
| **F9** KDE file manager can open/show the received file | **NOT RUN** |
| **F10** tray unaffected during transfer | **PASS** — the item stayed registered and `Active` throughout |

What *was* proven about the Android → KDE direction: the offer crosses the LAN and is
correctly presented to the desktop's approval rendezvous, twice —

```
anyflow_capability_files: incoming file offer transfer=00716e17 peer=573C CB84 DA6C 993B
                          filename=pasture-map.txt size=91
anyflow_capability_files: incoming file offer transfer=eedaff81 peer=573C CB84 DA6C 993B
                          filename=recent-fourth.txt size=39
```

and the first one sat at `state waiting_accept` — i.e. the daemon held it pending a human
decision rather than auto-accepting — before ending `cancelled / declined by the user` when no
approval arrived. That is fail-closed behaviour working as designed. It is **not** a
certification of F1/F4, because the accepted path was never completed.

Contents of the transferred files are not reproduced here. All were small and harmless.

---

## 16. Clipboard

The actual KDE Wayland backend, recorded from the daemon's own announcement:

```
anyflow_capability_clipboard::backend::wayland: clipboard backend
    backend="wl-clipboard" available=true
    watch=wl-paste --watch (data-control) sensitive="yes"
```

```console
$ anyflow clipboard status
  backend              wl-clipboard
  ordinary clipboard   available
  sensitive clipboard  available
  auto-send            supported on this session
  devices:
    SM-X620  …  policy  send=on receive=on auto-send=off auto-receive=off
```

**`wl-clipboard` remains the backend on KDE Wayland**, using the `data-control` protocol, with
sensitive-content marking supported.

| Gate | Result |
|---|---|
| **C1** KDE → Android manual send | **PARTIAL** — the desktop sent it (`sent 22 bytes of clipboard text`; `clipboard update sent … bytes=22 sensitive=false`); arrival was **not** confirmed on the Android side |
| **C2** Android → KDE manual send | **NOT RUN** |
| **C3** content arrives correctly | **NOT CERTIFIED** |
| **C4** no false "sent" before peer verdict | **NOT CERTIFIED** on KDE |
| **C5** grant disabled → send blocked truthfully | **PASS** |
| **C6** grant enabled → session converges | **PASS** |
| **C7** no clipboard content in logs/report | **PASS** |

**C5** is worth quoting, because it is exactly the truthfulness property the sprint cares
about. With `clipboard.v1 NOT granted`, the send was refused — no optimistic success:

```console
$ anyflow clipboard send 6532889e82ba83d0782cc644e7a21fc3
error: this device is not allowed to send clipboard text to that peer.
       Grant it with `anyflow grant <device> clipboard.v1`.
```

**C6**: granting `clipboard.v1` re-handshook the session ("reconnecting the device so it takes
effect now") and the new session negotiated the full set —
`capabilities=["battery.v1", "clipboard.v1", "files.v1", "notifications.v1"]`.

No clipboard content appears in this report or in the daemon log; the log records only a byte
count, an event id and a sensitivity flag.

---

## 17. Notifications

**NOT RUN.** N1–N7 were not exercised.

What *was* established, and is the KDE-specific half of the answer the sprint wanted:

```
anyflow_capability_notifications::backend::dbus: notification server
    server=Plasma vendor=KDE version=6.6.4 spec=1.2
    body_markup=true persistence=true dismiss_reporting=true

anyflow_capability_notifications::backend::logind: lock state from logind LockedHint
    session=/org/freedesktop/login1/session/_310 locked=false

anyflowd: notifications.v1 ready
    sink=org.freedesktop.Notifications — Plasma 6.6.4 (spec 1.2, KDE)
    lock=org.freedesktop.login1.Session.LockedHint on /org/freedesktop/login1/session/_310
    available=true
```

So the real KDE notification service **is** `org.freedesktop.Notifications` as implemented by
**Plasma 6.6.4** (spec 1.2), it advertises `persistence` and `dismiss_reporting` — both
relevant to N2/N3 — and AnyFlow takes its lock signal from **logind `LockedHint`**, not from a
GNOME-specific source. The session also announced notification roles to the peer
(`announcing roles peer=573C CB84 DA6C 993B roles=2 epoch=1`), with the tablet reporting
`roles=0` at that moment.

None of that substitutes for N1–N7, which remain to be run.

---

## 18. Battery

| Gate | Result |
|---|---|
| **B1** Android battery visible on KDE AnyFlow | **PASS** |
| **B2** value plausible/current | **PASS** |
| **B3** absence/stale state truthful | **PASS** |

```console
$ anyflow status
       granted     battery.v1
       battery     79% (NotCharging, 42s old)
```

79 % matched the tablet's own indicator at the same moment, the charging state was correct
(the tablet was on USB data but not charging), and the reading is **explicitly aged**
("42s old") rather than presented as instantaneous. On the desktop side the daemon was equally
truthful about its own absence of a battery:

```
anyflowd: UPower available; no system battery present; battery.v1 is receive-only
```

No attempt was made to fix battery.v1 live-change debt, per the brief.

---

## 19. Peer disconnect / reconnect

**PASS.** Exercised three ways:

1. **Peer-initiated disconnect** — the tablet showed `Available / Not connected`; the desktop
   reported `connected no / state disconnected` with `last seen` ageing. Tapping *Connect*
   restored `connected yes` with all four grants and a fresh battery reading.
2. **Grant-forced reconnect** — granting `clipboard.v1` dropped and re-established the session,
   which then negotiated the enlarged capability set.
3. **Daemon restart** — §20.

Throughout, the tray item stayed present and `Status` stayed `Active`; no `NeedsAttention` was
used to signal peer state, and the menu remained available.

---

## 20. Daemon restart

Measured against the **real Plasma watcher**:

| Stage | `RegisteredStatusNotifierItems` | AnyFlow items |
|---|---|---|
| before | `:1.74/…` + `org.kde.StatusNotifierItem-4325-1/…` | **1** |
| daemon stopped | `:1.74/…` only | **0** |
| daemon restarted (PID 19596) | `:1.74/…` + `org.kde.StatusNotifierItem-19596-1/…` | **1** |

* no duplicate icon;
* re-registration occurred **exactly once** —
  `grep -c "registered an AnyFlow tray item"` on the new daemon's log → `1`;
* Android reconnected to the restarted daemon within 15 s, with all four grants intact;
* the GUI remained independently activatable.

**PASS.**

---

## 21. Plasma session lifecycle

**BLOCKED — not run.** No hot `plasmashell` restart, logout/login or VM reboot was performed
after the tray gates, so no claim is made about re-registration across a new Plasma session.
This gate is explicitly *not* inferred from the daemon-restart result in §20, which exercises
the opposite direction (item goes away and returns while the shell stays up).

The watcher's own resilience was previously certified at protocol level in
`KDE-STATUSNOTIFIER-V1.md` §13 (three watcher-reappearance cycles, one registration each);
that remains protocol evidence, not a real-session lifecycle result.

---

## 22. Screen lock

**PARTIAL.** The session locked twice on idle during the run, so real lock behaviour was
observed, but the gate was not executed as a controlled lock/unlock cycle.

Observed, and relevant to policy:

* **An incoming file offer that arrives while the session is locked is not silently accepted.**
  Transfer `00716e17` was held at `waiting_accept` across the lock and then ended
  `cancelled / declined by the user`. Fail-closed, which is the correct direction.
* **The watcher/item relationship survived the lock** — after unlocking, `kded6` still owned
  `org.kde.StatusNotifierWatcher` (PID 2403) and AnyFlow's item was still registered and
  `Active`. Plasma did **not** remove or recreate the watcher across lock/unlock.
* The Android session was unaffected by the lock and remained connected.

Not established: whether notification content is redacted on the KDE lock screen (that is
N4, and §17 was not run).

**Test-environment note.** Unlocking proved awkward: `kwriteconfig6 … Autolock false` did not
take effect for the running locker, typed passwords did not reach the greeter reliably, and
`loginctl unlock-session 10` cleared logind's `LockedHint` while leaving the KDE greeter on
screen. What worked was dismissing `kscreenlocker_greet` directly and then holding the locker
off with `kde-inhibit --screenSaver`. None of this involves AnyFlow.

---

## 23. Quick Panel

Certified **physically** on the real Plasma session, from a cold tray click. Observed content,
with no peer paired at that moment:

* **Flow A branding** — the mark and the wordmark, with *"One flow. Any device."* ✔
* **Connection state** — truthful: *"No device is paired yet. Pair one from AnyFlow Settings."* ✔
* **Files / Clipboard / Notifications status rows** — each present and each reading
  `Unavailable`, which was correct with no peer ✔
* **Send file** — present and **disabled**, i.e. the action respected the (absent) negotiated
  capability rather than offering an optimistic button ✔
* **Send clipboard** — present ✔
* **Settings** — *"Open AnyFlow Settings"* ✔

The **Files** row of the tray menu opened the Settings window on **Files → Transfers**, which
showed *"No transfers yet — Files you send or receive appear here while they are moving, and
stay listed until the daemon restarts"*, the identity footer `anyflow-kde ADD6 9AA1 613A 38D7`,
and a status line reading *"Secure connection · Local network · port 55432 · IPv4+IPv6"*.

**Not certified:** the peer-connected rendering of the Quick Panel — battery row, selected
peer, *View transfers* — because the guest's rendering degraded (§22) before the panel was
re-opened with the tablet connected. No optimistic-clipboard-success regression was observed
in what was seen, but the panel's connected state was not visually re-checked.

---

## 24. Privacy / security

| Property | Result |
|---|---|
| TLS 1.3 | **unchanged** — `desktop/core/src/tls.rs`: `static TLS13_ONLY: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13]`, configs built with `TLS13` only |
| SPKI pinning | **unchanged** — `desktop/core/src/fingerprint.rs` fingerprints the SPKI, not the certificate; `qr.rs`: "The scanning device pins this SPKI" |
| Proof-of-possession pairing | **exercised live** — "A device proved it holds the pairing code", single-use code, confirmation-gated |
| Fingerprint peer authority | **exercised live** — the desktop names the peer by `573C CB84 DA6C 993B`; the tablet's incoming-file card names the desktop by `ADD6 9AA1 613A 38D7` |
| Per-peer grants | **exercised live** — ungranted clipboard refused (C5); grants applied per device and renegotiated on reconnect |
| Notification lock policy | **not exercised** (§17); the lock source is logind `LockedHint` |
| File traversal protections | not re-exercised; no source change |
| Clipboard privacy | **PASS** — no clipboard content in logs (byte count + event id + sensitivity flag only) |
| No cloud | **PASS** — `ss -tnp` showed exactly one established connection, `192.168.77.97:55432 ↔ 192.168.77.36:53664`, the LAN peer. Nothing else. |
| No telemetry | **PASS** — same evidence; and the test LAN has no route off-segment at all, so any outbound attempt would have failed visibly |
| KDE shell integration is local session D-Bus only | **PASS** — the tray item, menu and activation all live on `unix:path=/run/user/1000/bus`; the daemon listens only on 55432 |

On-disk state permissions on the guest:

```console
$ ls -la ~/.local/share/anyflow/
drwx------.  anyflow anyflow   .
-rw-------.  anyflow anyflow   identity.key
-rw-------.  anyflow anyflow   state.json
```

---

## 25. Logs

Daemon log for the whole certified run is **20 lines**. Audited for leakage:

* `grep -inE "password|secret|token|proof|private key|BEGIN .*KEY|clipboard content|body="`
  → **no matches**;
* no clipboard text, no file contents, no notification body, no full sensitive URI, no pairing
  token or proof, no private key;
* **no watcher-registration loop** — `grep -c "registered an AnyFlow tray item"` → `1` per
  daemon lifetime;
* **no repeated error spam** — the most repeated line in the whole run appeared twice (the
  `ServiceUnknown` activation line of §11, once per click before the bus rescan);
* failures are reported once, truthfully, and at `INFO`.

The journal contains no AnyFlow errors; the only AnyFlow-adjacent entries are the sshd
sessions used to drive the guest.

---

## 26. Code changes

**NONE.** No product source file was created, modified, staged or committed. The only
repository change is this report.

No KDE behaviour required a product change. The one activation failure observed (§11) was
traced to the session bus not having rescanned its service directory after a mid-session
metadata install, and cleared with `ReloadConfig`; AnyFlow's own handling of that failure was
correct and quiet. Per §25 of the brief, nothing was "silently fixed" during certification.

---

## 27. Files changed

```
KDE-PLASMA-REAL-CERTIFICATION-V1.md   (new, this report)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` remains untracked and untouched.

---

## 28. Remaining debts

> **Superseded by Part II (2026-09-21).** All six certification debts listed below
> were executed and closed in §32–§42; the host-state table at the end of this
> section was superseded by the rebuild in §32 and the restore in §44.

**Certification debts left by this sprint** (all are "not run", none is a known defect):

1. **Notifications N1–N7 on KDE** — the highest-value gap. The sink is identified
   (Plasma 6.6.4, spec 1.2, `persistence`/`dismiss_reporting` advertised) but no notification
   was mirrored, updated, dismissed or lock-tested.
2. **Files F1/F4/F6/F9** — the Android → KDE accepted path and the KDE approval dialog.
3. **Clipboard C2/C3/C4**, and confirmation of C1 on the Android side.
4. **Plasma session lifecycle (§21)** — logout/login or reboot, then re-registration.
5. **Screen lock (§22)** as a controlled cycle, including notification redaction (N4).
6. **Quick Panel with a peer connected (§23)** — battery row, selected peer, View transfers.

**Product debts observed, not fixed:**

* none new. battery.v1 live-change debt was left alone, per the brief.

**Packaging input (for the next sprint):**

* the D-Bus activation file must be visible to the session bus at install time — see §11;
* AnyFlow needs `mdns` and `55432/tcp` open. The guest was installed with
  `firewall --enabled --service=mdns --port=55432:tcp`, so a package should carry the
  equivalent firewalld service rather than expecting the user to open it;
* Fedora 44 KDE's display manager is **plasmalogin**, not SDDM, if any packaging touches
  autostart or session files.

**Host state currently left in place** (the certification LAN is still up so the work can be
resumed without re-pairing):

| Change | Restore |
|---|---|
| Wi-Fi pinned to 2.4 GHz BSSID `60:33:4B:E2:A2:13` (ch 11) | `nmcli connection modify "Rede Wi‑Fi de Ida" 802-11-wireless.bssid ""` then `nmcli connection up …` (original: bssid unset, band unset, channel 0 — it had associated to `60:33:4b:e2:a2:14` on ch 100) |
| `hostapd` running in the foreground | `pkill -x hostapd` — the unit is `disabled` and was never enabled |
| `dnsmasq` serving DHCP on the bridge | `pkill` that instance (the system `dnsmasq.service` remains `disabled`/`inactive`) |
| `ap-anyflow` vif and `br-anyflow-kde` bridge | `ip link del ap-anyflow; ip link del br-anyflow-kde` |
| `br-anyflow-kde` in firewalld zone `libvirt` | runtime-only; `firewall-cmd --reload` or reboot clears it (the permanent interface list is empty) |
| `hostapd` and `guestfs-tools` packages installed | `dnf remove` if unwanted |
| `net.ipv4.ip_forward` | currently **0**, which is the original value |
| VM `anyflow-f44-kde` | left defined and running; the pre-existing `anyflow-d13`, `anyflow-u2404`, `anyflow-u2604` were never started |

---

## 29. Git status

```console
$ git branch --show-current
test/kde-plasma-certification-v1

$ git status --short
?? KDE-PLASMA-REAL-CERTIFICATION-V1.md
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md

$ git diff --check
(clean)

$ git diff --stat
(no tracked file modified)

$ git diff --name-status
(no tracked file modified)
```

Nothing staged, nothing committed, nothing pushed, no PR opened.

---

## 30. Automated tests

No source or test file changed, so the workspace was not re-run for ceremony. The existing
green baseline recorded in `KDE-STATUSNOTIFIER-V1.md` §29 remains applicable:

```
cargo test --workspace -j 2                              953 passed, 0 failed, 23 ignored
cargo test -p anyflow-gui -- --ignored --test-threads=1     1 passed, 0 failed
cargo test -p anyflow-linux -- --test-threads=1            48 passed, 0 failed
```

The binaries certified here were built from this branch with
`cargo build --release -j 2 -p anyflow-daemon -p anyflow-cli -p anyflow-gui` (3 m 49 s, no
warnings surfaced in the tail), against Fedora 44's own GTK4 4.22 / libadwaita 1.9 — the same
ABI the guest runs.

---

## 31. Final matrix

| Gate | Result |
|---|---|
| FEDORA KDE REAL HOST | **PASS** |
| PLASMA WAYLAND SESSION | **PASS** |
| REAL STATUSNOTIFIER WATCHER | **PASS** |
| SNI REGISTRATION | **PASS** |
| ONE TRAY ITEM ONLY | **PASS** |
| FLOW A VISUAL RENDERING | **PASS** |
| KDE PRIMARY CLICK | **PASS** |
| KDE DBUSMENU | **PASS** |
| QUICK PANEL | **PASS** (unpaired state; peer-connected state not re-checked) |
| FILES SURFACE | **PASS** |
| SETTINGS SURFACE | **PASS** |
| GUI COLD ACTIVATION | **PASS** |
| GUI NON-RESIDENT LIFECYCLE | **PASS** |
| ANDROID DISCOVERY | **PASS** |
| ANDROID PAIRING | **PASS** |
| FILES ANDROID → KDE | **NOT CERTIFIED** |
| FILES KDE → ANDROID | **PASS** |
| CLIPBOARD ANDROID → KDE | **NOT RUN** |
| CLIPBOARD KDE → ANDROID | **PARTIAL** |
| NOTIFICATIONS ANDROID → KDE | **NOT RUN** |
| BATTERY | **PASS** |
| PEER RECONNECT | **PASS** |
| DAEMON RESTART | **PASS** |
| PLASMA SESSION RESTART/LOGIN | **BLOCKED** |
| SCREEN LOCK/UNLOCK | **PARTIAL** |
| SECURITY / PRIVACY | **PASS** |
| NO PRODUCT CODE CHANGE REQUIRED | **PASS** |

### Verdict (Part I — superseded)

> **Superseded by §43.** The verdict below was correct at the end of Part I, when
> the Android↔KDE capability gates had not been executed. Part II executed them
> all; the certification verdict is **PASS** (§43).

### KDE PLASMA REAL CERTIFICATION V1: INCOMPLETE — NOT PASS *(Part I only)*

The sprint's PASS bar is "the real Plasma host **and** the critical Android↔KDE flows all
pass". The real Plasma host half is done and is strong: a genuine Fedora KDE 44 / Plasma 6.6.4
Wayland session, a real `kded6`-owned `StatusNotifierWatcher` with `plasmashell` as host, one
truthful AnyFlow item, the Flow A actually drawn in the panel, real pointer clicks producing
the Quick Panel and a three-row DBusMenu with no authority actions, correct cold/warm GUI
activation and a non-resident GUI, and a clean daemon-restart re-registration. Pairing, battery
and desktop→phone file transfer with byte-exact integrity all pass against the physical
SM-X620 over a real, relay-free L2 LAN.

What is missing is not a failure but an absence: **notifications were not exercised at all**,
the Android → KDE file path was never accepted, and clipboard was only half covered. Those are
squarely "critical Android↔KDE flows", so the honest verdict cannot be PASS.

**This is explicitly not a FAIL in the sense of §28 of the brief** — no gate failed on KDE, and
no KDE-specific defect was found. Nothing here justifies a defect sprint, and no product code
was changed.

### Roadmap consequence

KDE is **not yet cleared as a non-blocker**. The cheapest path to closing it is a short
continuation, not a new sprint: the VM, the no-NAT test LAN, the pairing and the tooling are
all still standing, so §28's six certification debts can be executed directly against the
running environment. Packaging should not start until notifications in particular have been
run on this host, since notifications are the capability most likely to differ between GNOME
and Plasma and the one this sprint learned the least about.

---

# Part II — Continuation run (2026-09-21)

This part closes the six certification debts §28 left open. It was run against the
same VM and the same pairing; **no device was re-paired and no product source file
was changed.**

## 32. State of the environment at the start of Part II

The brief for this run assumed "the VM, AP, bridge and existing pairing are already
valid and standing". **They were not.** The host had been rebooted and moved to a
different network in the interval, so:

| Thing | Expected | Found |
|---|---|---|
| VM `anyflow-f44-kde` | running | **powered off** (`desligado`) |
| `br-anyflow-kde` | up | **absent** |
| `ap-anyflow` / `hostapd` / `dnsmasq` | running | **absent** |
| Host Wi-Fi | `Rede Wi‑Fi de Ida`, ch 11 | **`Rede Wi-Fi de Yuri`, ch 36 (5 GHz)** |
| Pairing (desktop trust store) | intact | **intact** |

The test LAN was therefore **rebuilt to the same documented design** (§6) rather than
redesigned: one radio running managed + AP, a bridge carrying the AP vif and the VM
tap, dnsmasq serving DHCP only, no NAT, no relay. Only the two parameters that the
new router forces were changed:

* the managed link was pinned to the **2.4 GHz sibling BSS of the same SSID**,
  `08:8A:F1:2B:4B:B2`, **channel 7 (2442 MHz)** — 2.4 GHz 1–13 carry no `no IR` flag,
  so the AP may beacon there, exactly as §6 reasoned for ch 11;
* the AP's PSK was re-set (the previous one was not recorded anywhere), keeping the
  SSID `anyflow-cert` that the tablet already had saved.

### Topology as rebuilt

```
br-anyflow-kde  192.168.77.1/24   (host: DHCP only — NO gateway, NO DNS, NO NAT)
├── ap-anyflow   AP vif on phy0, ch 7, WPA2-PSK/CCMP, ap_isolate=0, hostapd 2.11
│                 └── SM-X620        192.168.77.60
└── vnet0 (tap)  anyflow-f44-kde NIC
                  └── anyflow-kde    192.168.77.97
```

`bridge link show` listed exactly `ap-anyflow` and `vnet0`; the DHCP lease file named
`anyflow-kde` and `Tab-S10-FE-de-Yuri`.

**No-NAT re-proved on the tablet itself:**

```console
(tablet) $ ip route
192.168.77.0/24 dev wlan0 proto kernel scope link src 192.168.77.60

(tablet) $ ping -c1 -W2 1.1.1.1
connect: Network is unreachable
```

`net.ipv4.ip_forward = 0` before and after (the firewalld `libvirt` zone assignment
flips it to 1; it was set back and re-checked, as §6 records).

### Pairing was not redone

```console
$ anyflow devices
  SM-X620   6532889e82ba83d0782cc644e7a21fc3
  platform android   fingerprint 573C CB84 DA6C 993B
  paired yes   granted battery.v1 clipboard.v1 files.v1 notifications.v1
```

`state.json` held exactly **one** peer, `revoked:false`, `hidden:false`, matching the
fingerprint §14 recorded. The tablet reconnected on its own.

## 33. Test harness for this run

The previous run's guest access died with the host's `/tmp` (tmpfs). Access was
re-established **without** creating credentials: the guest is driven by
`virsh send-key` into a Konsole and read back from `virsh screenshot`, and the pointer
is driven by QEMU's own absolute input events.

> **Correction to an earlier finding.** `KDE-STATUSNOTIFIER-V1`-era notes said QEMU's
> mouse "does not reach a Plasma Wayland session at all". That is true of HMP
> `mouse_move` (relative deltas). **QMP `input-send-event` with `abs` axes works
> exactly**, pixel-for-pixel:
>
> ```console
> $ virsh qemu-monitor-command anyflow-f44-kde \
>     '{"execute":"input-send-event","arguments":{"events":[
>        {"type":"abs","data":{"axis":"x","value":22271}},
>        {"type":"abs","data":{"axis":"y","value":31444}}]}}'
> ```
>
> lands on the tray icon (1024×768 screen, axes 0–32767) and Plasma raises the
> AnyFlow tooltip. Every click in Part II — the notification ✕, **Accept** on the file
> approval, the tray, the Quick Panel — is a real pointer click delivered this way.
> `ydotool` was *not* used: `--absolute` pins the cursor at the origin on this guest.

**Prerequisites enabled on the Android side** (consent steps a person performs; none
of them is a notification *policy* field):

* the notification listener was granted with `cmd notification allow_listener`
  (`settings put` is not enough on One UI). The app reads the grant at start, so the
  app was restarted once for it to observe `Allowed`;
* **Share notifications with this computer** was turned on, and the **AnyFlow Fixture**
  app was chosen in the app picker (1 of 98);
* the **Clipboard** capability toggle was turned on for C1–C3.

**Deliberately not changed:** `allow_mirror`, `when_sink_locked` (`app-only`) and
`allow_dismiss_sync` (`off`) — the three fields that are the notification policy — were
left exactly as found on both ends, and every N-gate below was judged against them.

**Temporary, and restored:**

| Change | Restored |
|---|---|
| Plasma `plasmanotifyrc PopupTimeout=120000` (to try to hold a popup open) | key **deleted**; `kreadconfig6` → empty. It never took effect anyway — popups kept closing at ≈5.5 s |
| Android **Apply automatically** turned on once, to read the received clip back | turned **off** again; screenshot confirms "Received text waits in a notification until you tap Copy" |

## 34. Notifications — N1…N7

The sink, as the product itself reports it:

```console
$ anyflow notifications status
  server        org.freedesktop.Notifications — Plasma 6.6.4 (spec 1.2, KDE)
  reachable     yes
  body markup   yes — bodies are escaped before they are sent
  persistence   yes — notifications stay in the list until acknowledged
  lock state    org.freedesktop.login1.Session.LockedHint on …/session/_31
  screen        unlocked
  SM-X620 (573C CB84 DA6C 993B)
    mirror on    when locked app-only    dismiss-sync off
    roles: this desktop announced 2 (epoch 1); the device can source notifications (epoch 2)
```

and on the tablet: `This device announces SOURCE + DISMISS_TARGET · epoch 2`,
`The computer announces SINK + DISMISS_REPORTER · epoch 1`.

Evidence was taken from a `dbus-monitor` trace of `org.freedesktop.Notifications` in
the guest session — i.e. from what actually crossed the desktop's notification bus.

### N1 — an Android notification appears on KDE — **PASS**

A fixture post rendered as a real Plasma popup: app name **AnyFlow Fixture** with the
Flow A icon, title and body as sent. `anyflow notifications status` reported
`mirrored now 2 / showing 2 of 2 mirrored` at the time.

*Environmental note:* Plasma's **Do Not Disturb** was active at the start of the run,
which suppressed the first popups while still mirroring them. That is Plasma's setting,
not AnyFlow's behaviour, and it is why the first captures showed no banner.

### N2 — an update replaces, it does not duplicate — **PASS**

Post, then update the same Android id 2 s later (inside the popup's life):

```
method call … member=Notify   uint32 0    "N2-Original-8803"  "…-first"
method call … member=Notify   uint32 14   "N2-Updated-8803"   "…-second"
signal      … member=NotificationClosed   uint32 14   uint32 1
```

The second `Notify` carries **`replaces_id = 14`** — the server id Plasma gave the
first — so one notification existed and was updated in place. It later closed once,
by expiry.

A first attempt used a 6 s gap and produced `replaces_id = 0` twice. That is **not** a
defect: the trace shows the first notification had already been closed (`reason 1`)
1.2 s earlier, so there was nothing live to replace. Correct behaviour, and worth
recording because it is easy to misread.

### N3 — dismissal semantics — **PASS**

The protocol distinguishes the two, and Plasma reports both:

| Event | `NotificationClosed` |
|---|---|
| popup timed out | id 16, **reason 1 — Expired** |
| **✕ clicked by a real pointer click** | id 17, **reason 2 — Dismissed** |

Under the current rule (`allow_dismiss_sync = false`; on the tablet *"Off. Dismissing a
notification on that computer leaves this one alone."*) the desktop must **not** ask the
phone to dismiss. After one genuine dismissal and many expiries the tablet's own counter
read:

```
Dismissals from this computer     0 asked · 0 done
tracked=16 sent=22 not mirrored=0
```

So a user dismissal was reported according to the current product rules — which is to
say, not forwarded — and **expiry was never treated as a user dismissal.**

### N4 — lock policy — **PASS**

The session was locked with `loginctl lock-session 1` (no greeter hacks; an unattended
`loginctl unlock-session` was armed first so the run could not strand). While locked a
fixture notification with a distinctive title *and* body was posted from the tablet.

* AnyFlow saw the lock: the status captured 30 s in reported `screen  LOCKED`.
* The lock screen itself showed **no notification content** at all.
* On the notification bus, for the whole locked period:

```console
BODYHITS=0        # the body token never appeared
TITLEHITS=0       # the title never appeared
…
string "AnyFlow Fixture"
string ""          # summary
string ""          # body
```

That is precisely `when_sink_locked = app_only`: the computer was told *which app*
notified and nothing else. **No sensitive content was leaked contrary to policy.**

* After unlocking, mirroring resumed in full — the next post rendered with its title
  and body intact.

### N5 — no permanent AnyFlow notification history — **PASS**

After a run that mirrored, updated, dismissed, expired and lock-redacted notifications:

```console
$ ls -la ~/.local/share/anyflow/
-rw-------. 1 anyflow anyflow  138 Sep 18 19:14 identity.key
-rw-------. 1 anyflow anyflow 1613 Sep 18 20:08 state.json
$ ls -d ~/.cache/anyflow ~/.config/anyflow
ls: cannot access … No such file or directory  (both)
```

Both files still carry their **Sep 18** mtimes — nothing was written. No history file
was created anywhere. The GUI says the same thing of transfers: *"This list covers the
current daemon run. AnyFlow keeps no transfer history on disk."*

### N6 — AnyFlow's own package stays excluded — **PASS**

Two independent proofs:

* **By construction** — searching the app picker for "anyflow" returns only
  **AnyFlow Fixture** (`…anyflow.fixture`). AnyFlow's own package cannot be chosen.
* **At runtime** — AnyFlow posted two of its own notifications on the tablet during the
  run ("Clipboard received from anyflow-kde", "Connected to anyflow-kde"). Neither
  reached the desktop: `grep -c 'Clipboard received' <bus trace>` → **0**.

The same trace also shows that **no** unchosen app crossed the bus — the tablet's own
shade held personal notifications throughout, and only the fixture's strings appear.

### N7 — no notification content in AnyFlow logs — **PASS**

```console
LOGBODY=0     # body tokens in ~/nd.log
LOGTITLE=0    # titles, and clipboard tokens, in ~/nd.log
LOGLINES=29
```

The daemon logs capability negotiation, session and tray events only. (This is also why
mirrored notifications leave no trace in the daemon log — an absence that initially
looks like a failure to mirror and is not.)

## 35. Files — Android → KDE

The daemon was run **without** `--accept-files-without-asking`; the approval rendezvous
was exercised for real, with the GUI attached.

| Gate | Result |
|---|---|
| **F4** approval visibly presented and acceptable | **PASS** |
| **F1** transfer completes | **PASS** |
| **F6** transfer appears correctly in the GUI | **PASS** |
| **F8** lands in the configured download directory | **PASS** |
| **F9** open/show from the product surface | **N/A — the surface has no such action on any platform** |

The dialog:

> **Incoming file** — SM-X620 wants to send you a file.
> **anyflow-kde-cert.txt** · 77 B · text/plain
> Verified device · 573C CB84 DA6C 993B
> [ Decline ] [ Accept ]

**Accept** was clicked with a real pointer click. The Quick Panel then showed
*Recent transfers — anyflow-kde-cert.txt, From SM-X620 · Received*, and the Settings
Transfers page showed *Done — Saved to /home/anyflow/Downloads/AnyFlow/anyflow-kde-cert.txt*
at 100 %.

**Byte integrity:**

```console
(tablet)  fbd46b4ea29a62379ef314aad4809c4f1bdf10f9bee15c58734d3a84b29d88b3  /sdcard/Download/anyflow-kde-cert.txt
(KDE)     fbd46b4ea29a62379ef314aad4809c4f1bdf10f9bee15c58734d3a84b29d88b3  /home/anyflow/Downloads/AnyFlow/anyflow-kde-cert.txt
```

Identical, 77 bytes, and the received file is mode `-rw-------`.

**On F9.** `desktop/gui/src/views/files.rs` renders the stored path as a *caption*
(`widgets::caption(&format!("Saved to {stored}"))`) — there is no open, reveal or
"show in folder" action in the transfers view, and no `xdg-open`/`FileManager1` call
anywhere in `desktop/`. So the gate's "where supported" clause applies: **the product
surface does not support it, on KDE or anywhere else.** This is not a KDE gap and not a
regression. The received file itself opens normally from the KDE desktop — `xdg-open`
opened it in KWrite showing the exact payload.

## 36. Clipboard

Confirmed backend, as the brief asks:

```console
$ anyflow clipboard status
  backend              wl-clipboard
  detail               wl-clipboard; watch: wl-paste --watch (data-control); sensitive marking: yes
  policy               send=on receive=on auto-send=off auto-receive=off
```

**wl-clipboard** and **data-control** — unchanged. Android→desktop stayed the explicit
path; no background auto-read was tested, and the tablet states why it cannot exist:
*"Android does not let an ordinary app read the clipboard in the background, so clips
are sent when you tap Send clipboard."*

Test text was non-sensitive and is **not reproduced here**; sizes are given instead.

| Gate | Result |
|---|---|
| **C1** KDE → Android, arrival confirmed on Android | **PASS** — 20 bytes sent; the tablet raised *"Clipboard received from anyflow-kde — 20 bytes of text"* with a **Copy** action, and answered `PENDING_USER` |
| **C2** Android → KDE, arrival confirmed on the KDE clipboard | **PASS** — sent from the tablet's **Send clipboard** screen (which previewed the clip and its 21-byte size); on KDE `anyflow clipboard apply` reported *applied 21 bytes to the clipboard* and `wl-paste` returned it |
| **C3** content integrity both directions | **PASS** — byte-exact both ways |
| **C4** no false success before the peer verdict | **PASS** |

**C3 method.** For Android→KDE the KDE clipboard was first loaded with a different
local filler, so the post-transfer `wl-paste` could only match if the text really
travelled; it matched exactly, 21 bytes. For KDE→Android the tablet's **Clipboard
preview** was read back and showed the 19-byte token that KDE had sent, character for
character.

**C4 is the strongest result of the three.** The desktop's "sent N bytes" is a
statement about transmission; the *acceptance* is reported separately, and it tracked
the peer's real answer through three different states in one session:

| Peer state | Android log | `anyflow clipboard status` |
|---|---|---|
| clipboard capability **off** | `outcome=NOT_AUTHORIZED` | `last result  not authorized` |
| on, apply-automatically off | `outcome=PENDING_USER` | `last result  pending` |
| on, apply-automatically on | — | `last result  applied` |

No optimistic success at any point. The tablet's own send screen behaves the same way:
with an empty clipboard it **disables** "Send to anyflow-kde" and says *"There is
nothing to send"* rather than claiming a send.

## 37. Quick Panel with the peer connected

Opened by a real click on the tray icon, with SM-X620 connected:

| Item | Result |
|---|---|
| Selected peer | **SM-X620** |
| Connected state | truthful — `Connected`; after the reboot it showed `Offline / Available when connected` until the peer returned |
| Battery row, current/stale | **truthful both ways** — `Connected · 72% · Charging` when fresh, `Connected · 44% (last known)` / `72% (last known)` once the reading aged |
| Files capability | **On** (and `Off` while the peer was offline) |
| Clipboard capability | **On** |
| Notifications capability | **On** |
| Send file | enabled when connected, **disabled (greyed) when offline** |
| Send clipboard | enabled |
| View transfers | **present** — *"View all transfers"* appears once a transfer exists, tooltip *"Opens the Transfers page in AnyFlow Settings. It does not change which device you send to."* |
| Settings | **Open AnyFlow Settings** (header control tooltip confirms) |
| Flow A branding | **present** — the mark plus *"AnyFlow — One flow. Any device."* |

No optimistic clipboard success regression was observed (§36, C4).

## 38. Controlled lock / unlock

One deliberate cycle, `loginctl lock-session` → `loginctl unlock-session`.

**Before:** watcher `org.kde.StatusNotifierWatcher` owned by **kded6 (1579)**; host
`org.kde.StatusNotifierHost-1622` owned by **plasmashell (1622)**; AnyFlow item
`org.kde.StatusNotifierItem-2904-1` owned by **anyflowd (2904)**; session `Id=1`,
`Active=yes`, `LockedHint=no`; peer connected.

**During:** N4 executed (§34) — `screen LOCKED`, app-name-only, no leak. **Plasma's real
behaviour, recorded rather than assumed: the watcher, the host and the AnyFlow item all
stayed registered and unchanged across the lock** — Plasma does not tear the tray down
when the session locks, so there is nothing for AnyFlow to re-register.

**After:** `ITEMS=1` — exactly one AnyFlow item, **no duplicate**; the same three names
with the same owners; `connected yes / state connected` — the Android session survived
the lock untouched; tray icon, tooltip and the three-row menu all still functional.

## 39. Plasma session lifecycle

**Logout → login was not usable, and the reason is recorded rather than worked around.**
`/etc/plasmalogin.conf` has `[Autologin] User=anyflow`, `Session=plasma.desktop`, and
`#Relogin=false` — commented out, so the default (`false`) applies: after a logout
plasmalogin shows the greeter and does not re-login. No account password was available
to this run, so a logout would have stranded the session. The brief's accepted
alternative was used: a **clean `systemctl reboot`**. `plasmashell` was **not**
hot-killed.

After the new session came up (autologin):

| Check | Result |
|---|---|
| `org.kde.StatusNotifierWatcher` exists | **yes** |
| Ownership truthful | **kded6 (1552)** owns the watcher; **plasmashell (1600)** owns `StatusNotifierHost-1600` — both new pids |
| `io.github.yurisismotto.anyflow` activatable | **yes, with no `ReloadConfig`** — see §40 |
| anyflowd registers exactly once | **yes** — `grep -c 'registered an AnyFlow tray item'` → **1** |
| Exactly one tray icon | **yes** — `ITEMS=1`, `org.kde.StatusNotifierItem-2874-1` (anyflowd 2874) |
| Flow A resolves | **yes** — icon drawn, tooltip *"AnyFlow / One flow. Any device."* |
| Tray menu works | **yes** — Quick Panel · Files · Settings |
| Cold GUI activation | **yes** — no GUI process existed; the tray click activated it and the Quick Panel opened, truthfully showing the peer `Offline` |
| Android reconnects | **yes, on its own** — the tablet showed *"Connected to anyflow-kde"* without intervention, and the panel went to `Connected · 72% · Charging` |
| Pairing/trust intact | **yes** — same single peer, same fingerprint, no re-pair |
| Mirroring after reboot | **yes** — a fresh fixture notification rendered in full |

This build installs **no systemd user unit and no XDG autostart entry** for `anyflowd`,
so the daemon was started by hand in the new session, exactly as it was in the old one.
That is a packaging gap, not a KDE finding — see §40.

## 40. D-Bus activation — Packaging requirement (not fixed here)

Packaging was **not** touched in this sprint. The finding from §11 is preserved and now
**classified as a mandatory Packaging requirement**, with one new measurement that
sharpens it.

**The finding.** A D-Bus service file installed into an **already running** session is
invisible to that session's bus until the bus re-reads its service directories, i.e.
until `org.freedesktop.DBus.ReloadConfig` is called (or the user logs out and back in).
Activation by name fails until then, while the identical call against an
already-running instance succeeds.

**New measurement.** In the **freshly booted** session of §39, before anything AnyFlow
was started:

```console
$ busctl --user list --activatable | grep -i anyflow
io.github.yurisismotto.anyflow   …  (activatable)
```

The bus picked the file up at session start on its own. So the `ReloadConfig` need is
**specific to install/upgrade into a live session** — which is exactly when a package
manager does its work.

**Requirement for the packaging sprint.** A package install or upgrade must leave the
**live** user bus able to activate

```
io.github.yurisismotto.anyflow
```

without the user logging out. Calling `org.freedesktop.DBus.ReloadConfig` on the user
bus in the package's post-install step is the obvious discharge. Two adjacent items
belong to the same sprint and are recorded here, not fixed:

* `Exec=` in the service file must be an absolute path (the bus does not search `PATH`),
  so it has to be substituted at install time from the prefix — the guest's copy reads
  `Exec=/home/anyflow/.local/bin/anyflow-gui --gapplication-service`;
* **the daemon has no unit.** The service file activates the *GUI*; its own comments
  say `anyflowd` is expected to be "running as a user service". Nothing in this build
  installs that service, so on a fresh login there is no tray icon until someone starts
  `anyflowd` by hand. Packaging must ship the daemon's user unit.

## 41. Code changes in Part II

**None.** No file under `desktop/`, `android/`, `protocol/` or `packaging/` was opened
for writing. `git status --short` still shows only the two untracked reports, and
`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was neither read nor modified.

**No product defect was found on KDE in Part II.** Two things that could be mistaken for
defects are explained above and are not: the `replaces_id = 0` pair in §34 (the first
notification had already expired) and the absence of mirrored notifications from the
daemon log in §34 (that absence is N7 passing).

## 42. Final matrix — Part II

| Gate | Result |
|---|---|
| N1 notification appears on KDE | **PASS** |
| N2 update replaces, no duplicate | **PASS** |
| N3 dismissal semantics (dismiss vs expiry) | **PASS** |
| N4 lock policy, no leak, resumes after unlock | **PASS** |
| N5 no permanent notification history | **PASS** |
| N6 AnyFlow's own package excluded | **PASS** |
| N7 no notification content in logs | **PASS** |
| F1 Android → KDE completes | **PASS** |
| F4 approval presented and accepted | **PASS** |
| F6 transfer shown correctly in the GUI | **PASS** |
| F8 lands in the download directory | **PASS** |
| F8 SHA256 byte integrity | **PASS** |
| F9 open/show from the product surface | **N/A — not implemented on any platform** |
| C1 KDE → Android, confirmed on Android | **PASS** |
| C2 Android → KDE, confirmed on KDE clipboard | **PASS** |
| C3 content integrity both directions | **PASS** |
| C4 no false success before peer verdict | **PASS** |
| Clipboard backend `wl-clipboard` + `data-control` | **PASS** |
| QUICK PANEL, PEER CONNECTED | **PASS** |
| CONTROLLED LOCK / UNLOCK | **PASS** |
| PLASMA SESSION LIFECYCLE (clean reboot) | **PASS** |
| ANDROID RECONNECT AFTER SESSION RESTART | **PASS** |
| PAIRING/TRUST PRESERVED THROUGHOUT | **PASS** |
| NO KDE-SPECIFIC PRODUCT CODE CHANGE REQUIRED | **PASS** |

## 43. Verdict

### KDE PLASMA REAL CERTIFICATION V1: PASS

Every critical gate this sprint set passes. Specifically, and explicitly:

* **No KDE-specific product code change is required.** No product source file was
  changed in either part of this certification, and no KDE-specific defect was found.
* **Fedora KDE 44 / Plasma 6.6.4 / Wayland is certified** — a real, installed Plasma
  Wayland session on real hardware-backed virtualisation, not a protocol stand-in.
* **Real physical SM-X620 cross-device flows are certified** — notifications, files,
  clipboard, battery and pairing all exercised against the physical tablet over a real,
  relay-free, NAT-free L2 LAN.
* **Real StatusNotifier / DBusMenu / GUI lifecycle is certified** — a genuine
  `kded6`-owned watcher with `plasmashell` as host, exactly one truthful AnyFlow item
  across lock, unlock and a full session restart, a three-row DBusMenu, correct cold
  GUI activation and a non-resident GUI.

The one gate not marked PASS, **F9**, is marked **N/A** rather than failed: the product
has no open/show action for a received transfer on any platform, so there is nothing
KDE-specific to certify. It is a feature gap for a future sprint, recorded here, and it
does not block this verdict.

### Carried forward, not fixed here

* **Packaging (mandatory)** — §40: the live user bus must be able to activate
  `io.github.yurisismotto.anyflow` after an install or upgrade without a logout; the
  `Exec=` path must be substituted at install time; and **the daemon needs a user
  service unit**, which this build does not ship.
* **Product (minor, not KDE-specific)** — no open/reveal action for received files
  (§35, F9).
* `battery.v1` live-change debt, untouched, as the original brief directed.

### Roadmap consequence

**KDE is cleared as a non-blocker.** Packaging may now start; it should carry §40 as an
acceptance item.

## 44. Restore of the temporary host changes

Everything that did **not** need host root is restored:

| Change | State |
|---|---|
| VM `anyflow-f44-kde` | **cleanly powered off** (`systemctl poweroff` in the guest), **still defined** for future regression testing. Not removed. |
| Plasma `plasmanotifyrc PopupTimeout` | **restored** — key deleted, `kreadconfig6` returns empty |
| Android **Apply automatically** (clipboard) | **restored to off** — "Received text waits in a notification until you tap Copy" |
| Android notification policy (`allow_mirror`, `when_sink_locked`, `allow_dismiss_sync`) | **never changed** |
| SM-X620 Wi-Fi | **back on its normal network** — `Rede Wi-Fi de Yuri`, `192.168.68.63/22`, `ping 1.1.1.1` → 0 % loss, ~22 ms. **Internet restored.** |
| Temporary `anyflow-cert` network on the tablet | **forgotten** (it was created for this run with a throwaway PSK) |
| `net.ipv4.ip_forward` | **0** — the original value |
| Guest instrumentation (`dbus-monitor`, `ydotoold`) | gone with the guest reboot/poweroff; nothing persists |
| `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` | untouched — mtime still 2026-09-15 |

**Host-side teardown: executed.** It needed an interactive polkit authentication, which
arrived late in the run; once authorised, `lan-down.sh` completed cleanly:

```console
hostapd stopped
dnsmasq(temp) stopped
br-anyflow-kde removed
firewalld runtime reloaded
--- final state ---
net.ipv4.ip_forward = 0
Device "br-anyflow-kde" does not exist.
hostapd gone
disabled          # systemctl is-enabled hostapd — never enabled at boot
inactive          # systemctl is-active dnsmasq  — system unit untouched
FedoraWorkstation (default)
  interfaces: wlp0s20f3
```

| Item | State |
|---|---|
| temporary `hostapd` | **stopped**; unit still `disabled`, never enabled at boot |
| temporary `dnsmasq` instance | **stopped** — `pgrep -x dnsmasq` empty, nothing listening on `:67`, `/run/anyflow-cert` removed; the system `dnsmasq.service` is still `inactive` |
| `br-anyflow-kde` | **removed** |
| firewalld runtime association | **cleared** — active zones are back to `FedoraWorkstation (default)` with `wlp0s20f3` only; the `--permanent` list was always empty |
| `net.ipv4.ip_forward` | **0**, the original value |
| NAT | none — no `masquerade`/`snat`/`dnat` was ever written |
| Wi-Fi profile | **restored** — `bssid --`, `band --`, `channel 0`, i.e. exactly the original settings; the host re-associated to a 5 GHz BSS (ch 36), `192.168.68.73/22`, `ping 1.1.1.1` 0 % loss |
| `ap-anyflow` vif | **removed** — the first delete raced hostapd's release of the interface and left it behind `DOWN` and unbridged; a second removal cleared it. `ip link show ap-anyflow` → *Device "ap-anyflow" does not exist.* |

**Restore verified, final state:**

```console
$ ip -br link show | grep -E 'anyflow|br-anyflow'      (no output — both absent)
$ pgrep -x hostapd ; pgrep -x dnsmasq                  (no output — both stopped)
$ systemctl is-enabled hostapd                         disabled
$ systemctl is-active  dnsmasq                         inactive
$ sysctl net.ipv4.ip_forward                           net.ipv4.ip_forward = 0
$ firewall-cmd --get-active-zones
FedoraWorkstation (default)
  interfaces: wlp0s20f3
$ nmcli -f 802-11-wireless.{bssid,band,channel} con show "Rede Wi-Fi de Yuri"
802-11-wireless.bssid:    --        802-11-wireless.band: --        802-11-wireless.channel: 0
$ ip -br addr show wlp0s20f3
wlp0s20f3  UP  192.168.68.73/22 …
```

The host is back exactly as it was found: no bridge, no AP vif, no hostapd, no dnsmasq,
no firewalld association, forwarding off, and an unpinned Wi-Fi profile on its normal
address. The VM remains defined and powered off; `hostapd` and `guestfs-tools` remain
installed, as the brief directed.

This is an environment restore only. It has no bearing on the verdict in §43: all
certification evidence was collected before any teardown began.
