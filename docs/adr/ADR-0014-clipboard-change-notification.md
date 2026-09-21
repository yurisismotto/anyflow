# ADR-0014 — Detecting clipboard changes on the Linux desktop

**Status:** Accepted · 2026-08-30

## Context

`clipboard.v1` offers optional, per-device automatic Fedora → Android sync. To
push a clip as it is copied, the daemon has to know *when* the local clipboard
changes. Reading it on a timer is not acceptable: the sprint's own rule is no
busy polling, and a daemon that spawns a helper process once a second to ask
"has it changed yet?" is exactly the thing that makes a background service
worth uninstalling.

The obvious answer on Wayland is `wl-paste --watch`, from the standard
`wl-clipboard` package. It needs the compositor to implement one of two
protocols: `zwlr_data_control_manager_v1` (wlroots) or the newer
`ext_data_control_manager_v1`. sway, Hyprland and KWin implement one or both.

**Mutter implements neither.** Verified on the certification target:

```console
$ rpm -q mutter gnome-shell
mutter-50.4-1.fc44.x86_64
gnome-shell-50.4-1.fc44.x86_64

$ wl-paste --version
wl-clipboard 2.2.1          # supports wlr- and ext- data-control

$ wl-paste --type text/plain --watch /usr/bin/echo
Watch mode requires a compositor that supports the data-control protocol
```

So on GNOME — the desktop this Sprint certifies — the native Wayland answer to
"tell me when the clipboard changes" does not exist for an ordinary
application. Reading and writing work fine; only *watching* does not.

That is a real constraint, not a packaging gap: it is a deliberate Mutter
position, since data-control lets any client read every clipboard change
without user interaction. OmniBridge is not entitled to route around a
compositor's privacy decision.

The options were:

1. **Poll `wl-paste` on a timer.** Rejected outright. It spawns a process per
   tick, it is explicitly forbidden by the sprint, and it would still miss
   changes between ticks.
2. **Ship without auto-send on GNOME.** Honest, and the fallback if nothing
   else works — but it drops the headline feature on the only desktop we test.
3. **Use the Xwayland clipboard bridge.** Mutter mirrors the Wayland clipboard
   onto the X11 `CLIPBOARD` selection so X11 applications can paste. Taking
   ownership of an X11 selection generates an XFIXES `SelectionNotify` to
   every client that asked for one.

Option 3 was measured before it was chosen. A 60-line `x11rb` probe registered
`XFixesSelectSelectionInput` on `CLIPBOARD` and watched while three
*Wayland-native* `wl-copy` invocations ran:

```text
CLIPBOARD owner window = 0x400004
XFIXES version 5.0
  [1] XFIXES SelectionNotify: selection=381 owner=0x400004
  [2] XFIXES SelectionNotify: selection=381 owner=0x400004
  [3] XFIXES SelectionNotify: selection=381 owner=0x400004
total XFIXES selection events: 3
```

Three copies, three events, event-driven, no polling.

## Decision

**Reads and writes go through `wl-clipboard` against the real Wayland
clipboard. Change notification is a separate, pluggable source, chosen once at
startup: `wl-paste --watch` where data-control exists, XFIXES on Xwayland
otherwise, and nothing — reported honestly — if neither is available.**

### The watch yields a signal, not content

`ClipboardWatch` delivers `()`. The manager then calls `read_text()` itself.
This is what makes the two sources interchangeable at all — an XFIXES
`SelectionNotify` carries no data — and it has two further consequences worth
having: clipboard content never flows through the notification plumbing, so
there is exactly one place that reads it and one place to audit; and a burst of
changes collapses naturally, because a read after the fact yields the
clipboard's *current* state rather than a queue of stale ones.

The data-control implementation runs `wl-paste --watch /usr/bin/echo` rather
than the more obvious `--watch cat`. `echo` ignores its stdin and prints one
newline, so each change is one unambiguous byte on our pipe. `cat` would stream
every clip through our stdout with no delimiter — ambiguous framing for any
clip containing a newline, and clipboard content somewhere it does not need to
be.

### Only `CLIPBOARD`, never `PRIMARY`

The XFIXES watcher selects on the `CLIPBOARD` atom alone. X11 and Wayland both
have a second, implicit selection filled by merely dragging the mouse over
text. Synchronising it would transmit text the user never asked to copy.

### This is not an X11 backend

The XFIXES watcher never reads or writes selection data. It is a notification
source; the clipboard it observes is the Wayland one, and reads and writes go
to the Wayland one. A native X11 backend for a pure X11 session is future work,
and this module is the seed of its watch half.

## Consequences

**Auto-send works on GNOME**, which is the point.

**It depends on Xwayland running.** If `DISPLAY` is unset — a GNOME session
built without Xwayland — detection falls through to "no watch source", which is
reported by `omnibridge clipboard status` and degrades to manual sending. Nothing
breaks; a feature is simply unavailable and says so.

**It works better elsewhere.** On sway, Hyprland or KWin the first branch is
taken and no X connection is opened at all.

**It adds `x11rb`.** Pure Rust with its own connection backend, so it adds no C
toolchain requirement to a build that had none — which matters, because
building from a clean Fedora checkout with nothing but a Rust toolchain is a
standing constraint (ADR-0004). It is compiled with `default-features = false`
plus `xfixes`.

**The watcher owns an OS thread.** The X11 API is blocking and
`wait_for_event` has no timeout, so parking a tokio worker in it would starve
whatever else that worker was scheduled to run. Stopping it needs an actual X
event, so the guard sends its own window a `ClientMessage` from a second handle
on the connection — the standard idiom for interrupting an X event loop.

**Detection is a startup cost.** Probing `wl-paste --watch` means spawning it
and waiting 400 ms to see whether it survives; there is no way to ask a
compositor "do you implement data-control" without a Wayland connection of our
own, so we ask the tool that already knows. Doing it once at startup — rather
than per command — is also what lets `omnibridge clipboard status` tell the user
what will and will not work *before* they turn a flag on.

**A GNOME change would simplify this.** If Mutter ships
`ext_data_control_manager_v1`, the first branch starts being taken with no code
change, and the X11 module reverts to being only the seed of a future X11
backend.

## Notes

An unrelated but load-bearing discovery from the same investigation, recorded
here because it shaped the backend's process handling: on a **locked** GNOME
session, `wl-copy` and `wl-paste` do not fail — they **block indefinitely**,
waiting for a seat the compositor will not grant. Every backend invocation is
therefore bounded by `BACKEND_TIMEOUT` with `kill_on_drop`, so a locked screen
costs one killed child rather than one leaked child per attempt, for ever.
`wl-copy` additionally daemonises while inheriting its parent's stdio, so it is
always given `/dev/null` for stdout and stderr — a parent that handed it a pipe
and waited for EOF would wait until the next copy, possibly hours later.
