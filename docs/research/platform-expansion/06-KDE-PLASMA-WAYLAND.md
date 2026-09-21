# 06 — KDE Plasma / Wayland

| Field | Value |
| --- | --- |
| **Title** | OmniBridge on KDE Plasma Wayland |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | Clipboard, GUI integration, notifications, autostart, Xwayland interaction on Plasma. Whether GTK stays. |
| **Decision status** | PROPOSED. **PLAT-DEC-003** (GTK on KDE vs. a future Qt frontend) stays OPEN. |
| **Evidence** | REPO VERIFIED for OmniBridge's behaviour; OFFICIAL DOC VERIFIED for protocol deprecation and archive versions; **POC REQUIRED** for the central question in §4. |
| **Related documents** | [04](04-LINUX-PORTABILITY.md), [05](05-DEBIAN-UBUNTU-COMPATIBILITY.md), [15](15-CROSS-PLATFORM-CLIPBOARD.md), [18](18-UI-PLATFORM-STRATEGY.md) |

---

## 1. Summary

KDE Plasma is the **cheapest expansion target OmniBridge has** — cheaper than Debian, far
cheaper than Windows. It is the same binary, the same protocol, the same trust store, on a
different compositor. There is no port.

There is exactly one substantive technical question (§4) and one product question (§7), and
the technical one is *good news that needs confirming* rather than a risk to mitigate: on
Plasma, OmniBridge should be able to use the **proper Wayland clipboard-change protocol** that
GNOME denies it, and drop the Xwayland bridge entirely.

---

## 2. What already exists in the code for KDE

This is not speculative work. `capabilities/clipboard/src/backend/wayland.rs` already
contains a three-way watch-source detector:

```rust
pub enum WatchSource {
    /// `wl-paste --watch`, which needs the compositor to implement
    /// `zwlr_data_control_manager_v1` or `ext_data_control_manager_v1`.
    /// sway, Hyprland and KWin do; Mutter does not.
    DataControl,
    /// XFIXES on the Xwayland `CLIPBOARD` selection.
    X11Fixes,
    /// No event-driven source.
    None(String),
}

fn detect_watch_source() -> WatchSource {
    match probe_data_control() {
        Ok(()) => return WatchSource::DataControl,
        Err(why) => { …fall through… }
    }
    match super::x11::probe() {
        Ok(()) => WatchSource::X11Fixes,
        Err(why) => WatchSource::None(…),
    }
}
```

**KWin is named in the source as a compositor that implements data-control.** The
`DataControl` branch has never run on the certification machine (Fedora + GNOME), because
Mutter forces the X11 fallback. On Plasma it should be the branch that runs.

`probe_data_control()` is itself a nice piece of design worth noting, because it is what
makes this cheap: rather than opening a Wayland connection to ask the compositor what it
implements, it runs `wl-paste --watch` briefly and observes whether it dies immediately
(protocol missing) or stays alive (protocol present). That means **OmniBridge does not need to
know which protocol KWin speaks — `wl-clipboard` decides, and OmniBridge reads the outcome.**

---

## 3. Clipboard on Plasma: how it works

| Operation | Mechanism on Plasma | Same as GNOME? |
| --- | --- | --- |
| **read** | `wl-paste --no-newline --type text/plain;charset=utf-8` | ✅ identical |
| **write** | `wl-copy --type text/plain;charset=utf-8`, text on stdin | ✅ identical |
| **write sensitive** | `wl-copy --sensitive` | ⚠️ **Requires wl-clipboard ≥ 2.3.0.** On 2.2.1 the flag does not exist and `wl-copy` exits 1, so the clip **fails entirely** — see [26 §5.1](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). Where it *is* available, Plasma is the best case: Klipper honours the `x-kde-passwordManagerHint` it sets |
| **watch** | `wl-paste --watch` over `ext-data-control-v1` (expected) | ❌ **different, and better** |

Read and write need no work at all: they are `wl-clipboard` invocations that do not depend on
the compositor implementing data-control (they use the ordinary `wl_data_device` path with a
seat). Everything already written about them in
[ADR-0014](../../adr/ADR-0014-clipboard-change-notification.md) and
[CLIPBOARD.md](../../architecture/CLIPBOARD.md) applies unchanged.

**`wl-copy --sensitive` becomes materially more useful on Plasma.** On GNOME there is no
built-in clipboard history to skip. Plasma ships **Klipper**, which does keep history — so a
password sent from a phone with `sensitive_hint` set genuinely avoids being persisted in a
history list. This is a case where OmniBridge's existing "hint, not enforcement" design pays off
on a platform it was not written for.

---


> **⚠ Updated by the verification sprint (2026-08-31).** See
> [26 — External verification closeout](26-EXTERNAL-VERIFICATION-CLOSEOUT.md) for the primary
> sources and [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) for the resulting decisions.
> **§4 is resolved** from KWin's git history and wl-clipboard's release record. **§3 is partly wrong**: `wl-copy --sensitive` does not exist before wl-clipboard 2.3.0, so on Plasma with a 2.2.1 package the sensitive path **fails**, it does not degrade.

## 4. The one real question: which data-control protocol, and does the packaged wl-clipboard speak it?

`wlr-data-control-unstable-v1` has been superseded by `ext-data-control-v1` in
wayland-protocols, and upstream marks the wlr version deprecated and "not intended for
production use" (OFFICIAL DOC VERIFIED, wayland-protocols). KWin has a merge request porting
its implementation to `ext-data-control` (KDE `plasma/kwin` MR !6606).

**RESOLVED (V-01, V-02).** From primary sources:

- wl-clipboard **2.3.0** (released **2026-03-22**) added `ext-data-control-v1`, contributed by
  @zzag — stated verbatim in the upstream release body. Release 2.2.1 predates it (2023-08-27)
  and its source tree contains only `wlr-data-control-unstable-v1.xml`.
- KWin's commit history is unambiguous: *"Support wlr-data-control with the same impl as
  ext-data-control"* (2024-11-22) → *"Port to ext-data-control"* (2025-04-10) → **"Drop
  wlr-data-control support overlay"** (2025-04-12). That commit is absent from `v6.4.6` and
  present from `v6.4.90`, so **the overlay disappears in Plasma 6.5**.
- `zwlr_data_control` appears **zero times** in KWin `master`.

Now cross that with what distributions ship (OFFICIAL DOC VERIFIED, 2026-08-31):

| Distro | wl-clipboard |
| --- | --- |
| Debian 13 trixie (stable) | 2.2.1 |
| Debian forky / sid | **2.3.0** |
| Ubuntu 24.04 / 25.10 / **26.04 LTS** | 2.2.1 |
| Ubuntu stonking (devel) | **2.3.0** |
| Fedora current | `2.2.1^git20251124.e808203` — **has both protocols and `--sensitive`** (V-11) |

So there is a plausible and testable failure mode:

> **On Ubuntu 26.04 LTS + Plasma, KWin offers only `ext-data-control-v1`, the packaged
> `wl-clipboard` 2.2.1 speaks only `wlr-data-control-unstable-v1`, `probe_data_control()`
> fails, and OmniBridge silently falls back to the Xwayland XFIXES bridge — or to no watch at
> all.**

Three things make this worth taking seriously rather than dismissing:

1. It affects the **current LTS**, the release most users will be on.
2. OmniBridge's failure mode is graceful but *quiet*: `detect_watch_source()` logs and degrades.
   The user sees "auto-send unavailable" in `omnibridge clipboard status` and has no way to know
   the cause is a package version.
3. If KWin kept a `wlr-data-control` binding for compatibility, none of this happens and KDE
   is simply the best clipboard platform OmniBridge has.

**This is the entire content of POC-KDE-01.** It is one afternoon on a Plasma VM and it
decides whether KDE auto-send ships in Wave 3 or waits.

Mitigation if the failure is real, in preference order:
- **(a)** Nothing in OmniBridge. Depend on `wl-clipboard >= 2.3` in the packaging and let the
  distro's version decide; degrade honestly, and make the status message say *which* protocol
  was missing rather than a generic sentence.
- **(b)** Speak `ext-data-control-v1` in-process with a Wayland client crate, removing the
  `wl-clipboard` dependency for the *watch* path only (read/write still need a seat, so the
  helper stays). Real work; it also removes the "is wl-clipboard installed" failure entirely.
- **(c)** Rely on the Xwayland XFIXES bridge on Plasma too. Works only if Plasma's Xwayland
  clipboard bridging is active and mirrors selections — **POC-KDE-01** should measure this as
  a fallback even if (a) succeeds.

Recommendation: **(a) now, (b) only if (a) proves insufficient in practice.**

---

## 5. CLIPBOARD vs PRIMARY on Plasma

The brief is explicit and the code already complies: OmniBridge synchronises the ordinary
`CLIPBOARD` selection and must never touch `PRIMARY`.

Plasma raises the stakes because it makes PRIMARY more visible: Plasma has a
*"Synchronize contents of the clipboard and the selection"* Klipper setting, and mouse-select
fills PRIMARY constantly.

Verified in the code:
- `wl-paste`/`wl-copy` default to `CLIPBOARD`; the `--primary` flag is never passed anywhere
  in `backend/wayland.rs`.
- `backend/x11.rs` watches the `CLIPBOARD` atom via XFIXES, not `PRIMARY`.
- The trait contract in `backend/mod.rs` states the rule explicitly:
  *"The CLIPBOARD selection, never PRIMARY … Synchronising it would transmit text the user
  never asked to copy."*

**One Plasma-specific hazard follows from this that does not exist on GNOME:** if a user turns
on Klipper's clipboard↔selection synchronisation, then *selecting* text with the mouse writes
it into CLIPBOARD, OmniBridge's watch fires legitimately, and every mouse selection is sent to
the phone. OmniBridge is behaving correctly; the desktop changed what CLIPBOARD means.

This cannot and should not be "fixed" in OmniBridge — it is the user's Klipper setting. It
should be **documented**, and `omnibridge clipboard status` on Plasma is the natural place to
mention it. **UX-004** in the backlog. It is also a mild privacy consideration worth a line in
[20](20-SECURITY-THREAT-ANALYSIS.md).

---

## 6. Notifications, autostart, D-Bus

| Concern | Plasma | Difference from GNOME |
| --- | --- | --- |
| Notifications | `org.freedesktop.Notifications` on the session bus, served by `plasma-workspace` | None. Same freedesktop spec. OmniBridge does not implement notifications yet on either. |
| Autostart | XDG autostart: `~/.config/autostart/*.desktop`, honoured by `plasma-session` | None. Same spec. |
| systemd user units | Plasma 6 supports a systemd-managed startup and `systemd --user` is standard on all target distros | None |
| Session bus for UPower | `org.freedesktop.UPower` on the **system** bus; `zbus` reaches it identically | None |
| File chooser | GTK's own dialog, unless `xdg-desktop-portal-kde` is installed and the app is portal-aware | **Difference**; see §7 |
| Klipper | A real clipboard-history manager | Makes `--sensitive` more useful (§3) |

Nothing here needs new code. The autostart entry OmniBridge does not yet ship
([04 §11](04-LINUX-PORTABILITY.md)) works identically on both desktops when it exists.

**There is no KDE-specific D-Bus interface OmniBridge should use.** It is worth stating because
the instinct is to reach for `org.kde.klipper.klipper`. Don't: it is a Klipper-private
interface, it does not exist if the user disabled Klipper, and it would create a KDE-only
clipboard path parallel to the portable `ClipboardBackend`. The `wl-paste --watch` route works
on Plasma, sway, Hyprland and every other data-control compositor with one code path.

---

## 7. Does GTK stay on KDE?

**Yes. PLAT-DEC-003, recommended direction: GTK stays; revisit only on evidence.**

What actually happens when a GTK4/libadwaita app runs on Plasma:

| Aspect | Reality | Severity |
| --- | --- | --- |
| Does it run? | Yes, with `gtk4` + `libadwaita` installed | — |
| Does it look native? | No. libadwaita hard-codes the Adwaita stylesheet and ignores GTK themes by design. It will look like a GNOME app on a Breeze desktop. | **Cosmetic, permanent** |
| Dark mode | `AdwStyleManager` follows `org.freedesktop.appearance color-scheme` via the settings portal — Plasma exposes this through `xdg-desktop-portal-kde` | Works **if** the portal is installed |
| File chooser | `gtk::FileDialog` shows GTK's chooser unless the portal is used | Noticeable; `GTK_USE_PORTAL=1` or portal-aware code fixes it |
| Window decorations | Client-side (CSD), so no Breeze titlebar | Cosmetic, and a known Plasma complaint |
| Icons, accent colour | Adwaita's, not Breeze's | Cosmetic |
| Accessibility | AT-SPI, shared | Fine |

So: **functionally fine, visually foreign.**

The case *for* a Qt/Kirigami frontend is real — it would look right, get Breeze, get KDE's
file dialog and server-side decorations. The case *against* it, for now:

1. It is a **third** desktop GUI (GTK + WinUI + SwiftUI + Qt) for one desktop environment,
   before Windows and macOS have any GUI at all.
2. The GUI is a thin client. `gui/src/client.rs` is 203 lines over a JSON-lines socket; the
   views are presentation. There is no shared logic to lose — which cuts both ways, but it
   means the *cost* of a Qt frontend is bounded and can be paid later by anyone, including a
   contributor.
3. Nothing about the product is broken by a foreign-looking window. The daemon, the clipboard
   and files all work.
4. OmniBridge's visual identity (Flowing A, Flowing Ribbon, Palette A —
   [BRAND.md](../../design/BRAND.md)) is deliberately its own; a GTK app on Plasma is
   "OmniBridge-looking", not "broken-looking".

**Condition to revisit:** if KDE becomes a primary target with real users, or if a
contributor offers a Kirigami frontend, or if the portal-dependent bits (dark mode, file
chooser) prove unreliable in practice. Until then the effort belongs on Windows.

Two cheap mitigations to do regardless, both in [18](18-UI-PLATFORM-STRATEGY.md):
- Recommend `xdg-desktop-portal-kde` in the packaging so dark mode and the file chooser
  behave (**PKG-007**).
- Make `gtk::FileDialog` portal-aware so KDE users get KDE's file picker (**KDE-003**).

---

## 8. Xwayland interference

Only relevant if the X11 fallback is used on Plasma — i.e. if §4 goes badly.

Plasma runs Xwayland and bridges selections between the Wayland and X11 clipboards, as Mutter
does, but the two compositors' bridging implementations are independent. OmniBridge's X11 watch
(`backend/x11.rs`) connects to `$DISPLAY` and subscribes to XFIXES
`SelectionNotify` on `CLIPBOARD`. On GNOME this is a certified, working path. On Plasma it is
**unverified** and belongs in **POC-KDE-01** as the fallback measurement.

Two Plasma-specific risks if the fallback is used:
- If Plasma's bridge only mirrors on demand (when an X client asks for the selection) rather
  than eagerly, XFIXES may not fire on every Wayland-side copy.
- On a Plasma session started without Xwayland (increasingly possible), `$DISPLAY` is unset
  and `x11::probe()` correctly returns `Unavailable` — so the fallback simply does not exist,
  and the §4 question becomes decisive rather than merely important.

---

## 9. PoCs and backlog

| ID | Question |
| --- | --- |
| **POC-KDE-01** | Does `wl-paste --watch` work on Plasma Wayland with the distro-packaged `wl-clipboard`? Which protocol is negotiated? What happens on Ubuntu 26.04 (wl-clipboard 2.2.1)? Does the XFIXES fallback work on Plasma? |
| **POC-KDE-02** | Does the GTK4/libadwaita GUI run correctly on Plasma? Dark mode following, file chooser, decorations, HiDPI, and the pairing `AdwAlertDialog` in particular. |

| Backlog | Item |
| --- | --- |
| **KDE-001** | Make `omnibridge clipboard status` name the missing data-control protocol explicitly |
| **KDE-002** | Document the Klipper clipboard↔selection sync hazard |
| **KDE-003** | Portal-aware file chooser |
| **PKG-007** | Recommend `xdg-desktop-portal-kde` |

---

## 10. Conclusion

KDE Plasma requires **no new OmniBridge code** and should be treated as a *certification target*,
not a port. The work is: run the existing binary on Plasma, confirm which watch source it
picks, confirm the GUI behaves, write down what differs, and ship it.

If §4 resolves favourably, Plasma becomes the **best** Linux desktop for OmniBridge's clipboard —
a standard Wayland protocol instead of an Xwayland bridge, plus a clipboard manager that
honours the sensitive hint. That is a more interesting outcome than "KDE also works".
