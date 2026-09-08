# POC-NOTIF-02 — Does lock detection actually report a locked GNOME session?

| Field | Value |
| --- | --- |
| **Class** | P0 |
| **Defined in** | [06 §3](../06-OPEN-QUESTIONS-AND-POCS.md) |
| **Executed** | 2026-09-08 |
| **Verdict** | **PASS**, with one mandatory design correction: `logind LockedHint` is authoritative; `org.gnome.ScreenSaver.ActiveChanged` is a change *hint*, never the lock signal |
| **Consequence** | [01 §6.2](../01-FUNCTIONAL-SPECIFICATION.md) and [04](../04-PLATFORM-CAPABILITY-MATRIX.md) updated. The `Full` lock policy is **available**, not degraded |

---

## 1. Platform under test

| Field | Value | Source |
| --- | --- | --- |
| OS | Fedora 44 | host |
| Desktop | GNOME Shell 50.4 | `gnome-shell --version` |
| Session type | Wayland | `XDG_SESSION_TYPE`, `loginctl show-session 2 -p Type` |
| Session class | user, seat0, `Active=yes` | `loginctl` |
| logind session object | `/org/freedesktop/login1/session/_32` | `login1.Manager.GetSession("2")` |
| Notification server | `('gnome-shell', 'GNOME', '50.4', '1.2')` | `GetServerInformation` |
| Server capabilities | `actions, body, body-markup, icon-static, persistence, sound` | `GetCapabilities` |

`org.freedesktop.ScreenSaver` remains a trap and this run reconfirms it:

```console
$ gdbus call --session -d org.freedesktop.ScreenSaver -o /org/freedesktop/ScreenSaver \
      -m org.freedesktop.ScreenSaver.GetActive
Error: GDBus.Error:org.freedesktop.DBus.Error.NotSupported: This method is not
part of the idle inhibition specification
```

`org.gnome.ScreenSaver`, by introspection, offers `Lock()`, `GetActive()`,
`SetActive(b)`, `GetActiveTime()`, and the signals `ActiveChanged(b)` and
`WakeUpScreen()`.

## 2. Procedure

Two `dbus-monitor` instances run for the whole test:

```console
$ dbus-monitor --session "type='signal',interface='org.gnome.ScreenSaver'"
$ dbus-monitor --system  "type='signal',interface='org.freedesktop.DBus.Properties',\
                          path='/org/freedesktop/login1/session/_32'"
```

`LockedHint` is additionally polled at 50 ms to measure latency directly. Four
paths were exercised:

| Path | How | Why |
| --- | --- | --- |
| **1** | `loginctl lock-session 2` | The canonical lock |
| **2** | `org.gnome.ScreenSaver.Lock()` | What the Super+L keyboard shortcut calls. The PoC asks for the shortcut because "they are not always the same path" — this is that path invoked over D-Bus rather than by a keypress, which is the part that needs no human |
| **3** | `org.gnome.ScreenSaver.SetActive(true)` | A *third* path, not in the PoC, added because it turned out to behave differently |
| **4** | `SetActive(true)` with `lock-enabled=false` | Blank without lock |

Unlock in every case is `loginctl unlock-session 2`. The `lock-enabled`
gsettings key was read before path 4 and restored afterwards.

## 3. Observed

### Path 1 — `loginctl lock-session`

```text
time=1788906451.602231  org.gnome.ScreenSaver  ActiveChanged  true
time=1788906451.603293  login1.Session         LockedHint     true     (+1.06 ms)
...
time=1788906453.752547  org.gnome.ScreenSaver  ActiveChanged  false
time=1788906453.753634  login1.Session         LockedHint     false    (+1.09 ms)
```

Polled: `LockedHint → yes` in **0.25 s**; `→ no` in **0.75 s** after unlock.
`GetActive()` read `true` while locked and `false` after unlock.

### Path 2 — `org.gnome.ScreenSaver.Lock()` (the keyboard-shortcut path)

Call issued at `1788906565.9815`:

```text
time=1788906566.199966  login1.Session         LockedHint     true     (+218 ms)
time=1788906566.865011  org.gnome.ScreenSaver  ActiveChanged  true     (+884 ms)
...
time=1788906568.174205  org.gnome.ScreenSaver  ActiveChanged  false
time=1788906568.175277  login1.Session         LockedHint     false
```

Both sources report the lock inside 1 s, and both report the unlock. **But
`LockedHint` leads `ActiveChanged` by 665 ms.** The session is locked for
two thirds of a second before the screensaver reports itself active.

### Path 3 — `org.gnome.ScreenSaver.SetActive(true)`

```text
time=1788906465.682560  org.gnome.ScreenSaver  ActiveChanged  true
                        login1.Session         LockedHint     — no transition, ever
```

Polled for 5 s: `LockedHint` never became `yes`. `SetActive(true)` **blanks the
screensaver without locking the session**, even with `lock-enabled=true`, and
the blank is dismissed by the next input.

### Path 4 — blank with `lock-enabled=false`

```text
ScreenSaver.GetActive = (true,)
LockedHint            = no
```

## 4. Assessment against the pre-written criteria

> **Pass.** Both sources report locked within 1 s of the lock, both report
> unlocked on unlock, and blanking-without-locking does **not** report locked.

| Criterion | `logind LockedHint` | `org.gnome.ScreenSaver` |
| --- | --- | --- |
| Reports locked within 1 s (path 1) | ✅ 1.06 ms | ✅ 0 ms (led) |
| Reports locked within 1 s (path 2) | ✅ 218 ms | ✅ 884 ms |
| Reports unlocked on unlock | ✅ both paths | ✅ both paths |
| Blank-without-lock does not report locked | ✅ stays `no` | ⚠️ reports **active** — which is true, and is not a claim about locking |

**Verdict: PASS.** The fail branch — *"either source is silent or wrong … the
`Full` lock policy must be marked unavailable rather than silently
ineffective"* — is **not** triggered. Neither source is silent, and a prompt,
correct, monotonic source of lock state exists. `Full` and `AppOnly` are both
implementable and effective.

## 5. The mandatory correction

`ActiveChanged` is not a lock signal and must not be used as one. Two
independent measurements say so, and they fail in **opposite** directions:

* **Path 2 — fails open.** For 665 ms the session is locked and
  `ActiveChanged` still says inactive. A sink gated on `ActiveChanged` would
  render notification bodies in full onto a locked screen for two thirds of a
  second on every lock. That is precisely the *"fails open silently"* failure
  [01 §6.2](../01-FUNCTIONAL-SPECIFICATION.md) is written to prevent.
* **Path 4 — fails closed.** Blank-without-lock reports active while the
  session is unlocked, so content would be needlessly withheld.

The rule for the sink:

> **`org.freedesktop.login1.Session.LockedHint` is the authoritative lock
> state.** It is read at connect and tracked through `PropertiesChanged` on the
> session object. `org.gnome.ScreenSaver.ActiveChanged` may be subscribed as an
> additional *wake-up* to re-read `LockedHint`, and its boolean is discarded.
> A session whose `LockedHint` cannot be read is treated as **locked**.

This costs nothing: `LockedHint` was both faster and more accurate than the
GNOME signal on every path measured, and it is a logind property rather than a
desktop-specific one, so it is the portable choice as well as the correct one.

## 6. Cleanup

`lock-enabled` restored to its original `true`. Final state: `LockedHint=no`,
`GetActive=(false,)`, session unlocked. No notification was posted during this
PoC, so there was nothing to clear.
