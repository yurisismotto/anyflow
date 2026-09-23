# OmniBridge — Packaging v1, D-Bus activation self-heal (P4)

| Field | Value |
| --- | --- |
| **Branch** | `feature/dbus-activation-self-heal-v1` |
| **Base commit** | `c788d96` (merge of PR #50, `fix/systemd-user-unit-runtime-dir-v1`) |
| **Date** | 2026-09-22 |
| **Scope** | Audit §8.2 — the one part of P4 packaging cannot discharge. Decision **Q1: approved**. |
| **Authority** | [`PACKAGING-V1-READINESS-AUDIT.md`](PACKAGING-V1-READINESS-AUDIT.md) §4.4, §8 |
| **Host** | Fedora 44 Workstation, GNOME 50.5 Wayland, **dbus-broker 37-8.fc44**, rustc/cargo 1.98.1 |
| **Product code changed** | **Yes** — this phase is product code by design. One new module, one call site. No protocol, trust, pairing or capability change. |
| **Verdict** | **IMPLEMENTED AND MEASURED** — including on a real `dbus-broker` session |

Claims are labelled **MEASURED** (a command was run here and its output is
quoted) or **SOURCE-VERIFIED** (read out of the tree).

---

## 0. Summary

A package installs the GUI's D-Bus service file as **root**. The user's
*already running* session bus has not read it, so until the next logout,
clicking OmniBridge in the tray on a correctly installed machine returns
`org.freedesktop.DBus.Error.ServiceUnknown`.

Root cannot fix that — it has no route to a user's session bus, and the audit
MEASURED that no rpm file trigger exists or can exist for
`/usr/share/dbus-1/services`. `omnibridged` can: it runs as the user, in the
session, and already holds a session-bus connection.

```
omnibridged starts
   │
   ├─ ListActivatableNames ──── name present ──► done. No reload. No log line above debug.
   │
   └─ name absent
          │
          ├─ ReloadConfig  (exactly once, ever)
          │
          └─ ListActivatableNames ─┬─ present ──► HealedByReload
                                   └─ absent  ──► StillMissing. Stop.
```

One new module, `desktop/platform-linux/src/activation.rs`, 8 unit tests
against a counting fake and 12 integration tests against real message buses,
one of which runs on the developer's live `dbus-broker` session.

**The headline measurement**, on the real Fedora 44 session bus:

```console
$ cargo test -p omnibridge-linux --test dbus_activation -- --ignored --nocapture
the real session bus reported: HealedByReload
test the_real_session_bus_heals_a_freshly_installed_name ... ok
```

---

## 1. What was found on the way, and why it changed the tests

**This is the most useful thing in this document.** The audit's premise —
*"a service file installed into an already running session is invisible to
that session's bus until `ReloadConfig` is called"* — is **implementation-
specific**, and the first version of this suite failed because of it.

**MEASURED** on this host:

| Bus | Version | A file written into a service directory that **existed** when the bus started |
| --- | --- | --- |
| **dbus-broker** | `dbus-broker-37-8.fc44` — **what a Fedora 44 session actually runs** | **not** visible until `ReloadConfig` |
| **dbus-daemon** | `dbus-daemon-1.16.2-1.fc44` — the reference implementation | visible **immediately**; it watches its service directories with inotify |

```console
$ systemctl --user status dbus.service | head -2
● dbus-broker.service - D-Bus User Message Bus
$ strings /usr/bin/dbus-daemon | grep -ci inotify
7
```

The audit measured Fedora, so it measured `dbus-broker`, and its conclusion is
correct for the platform it was written about. But the integration suite spawns
its own private `dbus-daemon` — which is the right thing to spawn, because it
must never touch the developer's session — and `dbus-daemon`'s inotify made the
first version of the premise test fail with *"the bus noticed the file on its
own"*.

**What this does not change.** The feature is correct on both:

* on `dbus-broker` it is the repair the audit specified;
* on `dbus-daemon` the name is already there by the time the daemon looks, so
  `ensure_activatable` returns `AlreadyActivatable` and **issues no
  `ReloadConfig` at all**. A no-op, which is exactly what it should be.

**What it changed in the tests.** A directory that does not exist cannot be
watched by *either* implementation, so with the service directory missing at
bus start-up the two agree: absent, then present after exactly one
`ReloadConfig`. **MEASURED:**

```console
$ # servicedir configured but not created; bus started; file written afterwards
$ busctl --address=$ADDR ... ListActivatableNames | grep -c test.example.Name
0
$ gdbus call --address $ADDR ... org.freedesktop.DBus.ReloadConfig
()
$ busctl --address=$ADDR ... ListActivatableNames | grep -c test.example.Name
1
```

That is what the fixture now does, and it is **not a contrivance**:
`/usr/share/dbus-1/services` genuinely does not exist on a machine where
nothing has ever shipped a D-Bus service, and the OmniBridge package is then
the thing that creates it.

One test, `either_bus_implementation_ends_up_activatable_and_neither_is_misreported`,
deliberately uses a directory that *did* exist, asserts only the property true
of both — *the name is activatable afterwards, and the outcome never falsely
claims a repair* — and **prints** which path this bus took rather than
asserting it. A property of the bus is not a property of OmniBridge.

Had the suite been written only against `dbus-daemon` with a pre-existing
directory, it would have passed on Fedora by accident and proved nothing about
the case the feature exists for.

---

## 2. The implementation

### 2.1 The policy, in one function with no I/O

**SOURCE-VERIFIED**, `platform-linux/src/activation.rs`:

```rust
pub async fn ensure_activatable(bus: &dyn SessionBusControl, name: &str) -> Activation {
    let names = match bus.list_activatable_names().await {
        Ok(names) => names,
        Err(why) => return Activation::Unavailable(Unavailable::ListFailed(why)),
    };
    if names.iter().any(|n| n == name) {
        return Activation::AlreadyActivatable;
    }

    // The one reload. There is no loop around this and no second call below
    // it; the function returns on every path after it.
    if let Err(why) = bus.reload_config().await {
        return Activation::Unavailable(Unavailable::ReloadFailed(why));
    }

    match bus.list_activatable_names().await {
        Ok(names) if names.iter().any(|n| n == name) => Activation::HealedByReload,
        Ok(_) => Activation::StillMissing,
        Err(why) => Activation::Unavailable(Unavailable::RecheckFailed(why)),
    }
}
```

"At most one reload" is a property of something a reviewer can read in twenty
lines, rather than of a call graph. There is no timer, no retry, no loop and no
second entry point.

The comparison is `==`. Not `contains`, not `starts_with`:
`io.github.yurisismotto.omnibridge` and
`io.github.yurisismotto.omnibridge.Devel` are different applications.

### 2.2 The seam

`SessionBusControl` has **two** methods and no third is possible without
noticing:

```rust
#[async_trait::async_trait]
pub trait SessionBusControl: Send + Sync {
    async fn list_activatable_names(&self) -> Result<Vec<String>, String>;
    async fn reload_config(&self) -> Result<(), String>;
}
```

The production implementation is `zbus::fdo::DBusProxy` over
`zbus::Connection::session()`. Errors are reduced to the bus **error name**
before they are returned — the far end is another process on the session bus,
and its message text is not ours to put in a journal.

### 2.3 The call site

**SOURCE-VERIFIED**, `daemon/src/main.rs`, immediately after the tray:

```rust
tokio::spawn(async {
    let outcome = omnibridge_linux::activation::self_heal_desktop_activation().await;
    omnibridge_linux::activation::log(&outcome);
});
```

Spawned rather than awaited, and deliberately **not** in the daemon's
`select!` — the same discipline the tray already follows. A bus that is slow to
answer is not a reason for the network listener to start late, and nothing
downstream depends on the answer.

### 2.4 Feature gating

`desktop-activation`, default-on, alongside `tray`. `omnibridge-gui` and
`omnibridge-cli` both take `omnibridge-linux` with `default-features = false`
and therefore get neither: the agent owns the session-bus integration, and a
GUI that could reload the bus's configuration would be a boundary worth
defending that nobody had defended.

It implies `tray` for exactly one reason, recorded in `Cargo.toml`: it needs
`DESKTOP_APP_ID`, the single definition of the application's identity. A second
copy of that string is the one thing the identity discipline in
`tray/model.rs` exists to prevent.

---

## 3. Every constraint in the brief, and where it is held shut

| Constraint | How |
| --- | --- |
| **User session bus only** | `Connection::session()`. Asserted by `this_crate_never_opens_the_system_bus`, which walks **every** `.rs` in the crate — not just the file that makes the call today — and fails if the system-bus constructor appears anywhere. |
| **`ReloadConfig` at most once** | Structural (§2.1), and asserted by a counting fake in every unit test. |
| **Never poll** | No timer, no interval, no loop. The module owns no `tokio::time`. |
| **Never require root** | It is a method call on the caller's own session bus. |
| **Never change protocol** | No `.proto`, no wire type, no envelope. |
| **Never change trust or grants** | No store access, no peer state, no capability. The module's whole surface is two bus calls and an enum. |
| **D-Bus never mandatory for startup** | §5.4 — MEASURED on the real binary with no bus at all. |
| **Best effort** | Every path returns a value. Nothing returns `Err` to the daemon. |
| **Never start anything** | `ReloadConfig` re-reads configuration; it activates nothing. The suite's own service files name `Exec=/bin/true` so that even a future test that *did* activate would start nothing. |

---

## 4. The test matrix

The brief names seven cases. All seven, plus six more the implementation
invited.

| # | Case from the brief | Test(s) | Kind |
| --- | --- | --- | --- |
| 1 | already visible | `already_visible_does_nothing_at_all` · `a_session_that_already_knows_the_name_is_left_alone` | fake · real bus |
| 2 | missing → reload → visible | `missing_then_reload_then_visible` · `a_package_installed_into_a_live_session_is_healed` · **`the_real_session_bus_heals_a_freshly_installed_name`** | fake · real bus · **live dbus-broker** |
| 3 | missing → reload → still missing | `missing_then_reload_then_still_missing_stops` · `a_machine_with_no_desktop_application_is_not_pretended_to_be_healthy` | fake · real bus |
| 4 | `ReloadConfig` fails | `a_refused_reload_is_not_retried` | fake |
| 5 | user bus unavailable | `a_dead_bus_is_reported_and_not_fatal` · §5.4 | real bus · real binary |
| 6 | exact-name matching | `the_name_must_match_exactly` · `an_exact_match_is_found_wherever_it_sits_in_the_list` · `a_similar_name_on_the_bus_does_not_count_as_this_one` · `the_real_name_is_found_even_beside_a_near_miss` · `the_self_heal_asks_for_the_name_the_service_file_declares` | fake · real bus · source |
| 7 | reload count ≤ 1 | asserted by `bus.reloads()` in **every** fake-bus test | fake |

Added beyond the brief:

| Test | What it holds shut |
| --- | --- |
| `a_bus_that_cannot_be_listed_is_not_reloaded` | reloading a bus we could not even query would be acting on a guess |
| `a_failed_recheck_is_reported_rather_than_guessed` | a reload that succeeded and a re-query that failed is not a repair |
| `a_running_bus_does_not_see_a_service_file_it_never_scanned` | the premise itself, re-measured on every run |
| `either_bus_implementation_ends_up_activatable_and_neither_is_misreported` | §1 — the broker/daemon difference, without asserting a bus's behaviour as if it were ours |
| `the_production_adapter_reuses_the_connection_it_is_given` | the adapter does not open a second connection |
| `this_crate_never_opens_the_system_bus` | the security property, over the whole crate |

`the_name_must_match_exactly` runs five near misses, each of which contains the
name as a substring, is a prefix of it, or differs only in case:

```
io.github.yurisismotto.omnibridge.Devel
io.github.yurisismotto.omnibridg
io.github.yurisismotto.OmniBridge
xio.github.yurisismotto.omnibridge
io.github.yurisismotto.omnibridge2
```

### 4.1 What the suite is not allowed to touch

Every private bus is raised by the fixture with a service directory the fixture
owns. Nothing reads `DBUS_SESSION_BUS_ADDRESS`, and nothing can start the
developer's `omnibridge-gui`.

The one test that *does* use the live session is `#[ignore]`d, so `%check` in a
buildroot and CI on a headless runner both skip it — the same discipline the
rest of this crate's session-dependent tests follow. It writes exactly one
file, `io.github.yurisismotto.omnibridge.SelfHealProbe.service`, with
`Exec=/bin/true`, and removes it and reloads the bus from a `Drop` guard so
that a failed assertion still puts the session back. It never touches
OmniBridge's own service file or the trust store. **MEASURED** afterwards:

```console
$ ls ~/.local/share/dbus-1/services/
io.github.yurisismotto.anyflow.service
io.github.yurisismotto.omnibridge.service
$ busctl --user list --activatable | grep -c SelfHealProbe
0
```

---

## 5. Gate results

### 5.1 `cargo fmt --all --check`

Clean. (It was not on the first attempt; the code was reformatted, not the
check relaxed.)

### 5.2 `cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings`

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.65s
```

No warning, in production or test code, across the workspace.

### 5.3 `cargo test --workspace -j 2`

```
1016 passed; 0 failed; 23 ignored
```

Up from **997** on the previous branch — the 19 new tests, of which 1 is the
`#[ignore]`d live-session one. Nothing that passed before was changed to make
something new pass.

### 5.4 The daemon starts with no session bus at all

**MEASURED** on the real binary, with `DBUS_SESSION_BUS_ADDRESS` pointed at a
path that does not exist:

```console
$ env XDG_RUNTIME_DIR=$R DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/bus \
      timeout 6 ./target/debug/omnibridged --data-dir … --port 55499 --no-mdns --log debug
 INFO omnibridge_capability_notifications::backend::dbus: no session bus; notifications.v1 has no sink on this session
 INFO omnibridged: listening port=55499 families=IPv4+IPv6 sockets=1
 INFO omnibridged: control endpoint ready endpoint=/tmp/ob-nobus.j0La/omnibridge/control.sock
 INFO omnibridge_linux::tray: no tray integration on this session reason=no session bus (bus error)
DEBUG omnibridge_linux::activation: could not check D-Bus activation; continuing reason=no session bus (bus error)
exit=124   # the timeout fired, i.e. the daemon stayed up for the whole window
```

The listener bound, the control endpoint came up, and the self-heal said so at
`debug` and got out of the way. **D-Bus is not a startup dependency.**

### 5.5 Log levels

Only one outcome is above `debug`, and it is the one where something was
actually changed:

| Outcome | Level | Why |
| --- | --- | --- |
| `AlreadyActivatable` | `debug` | every freshly booted session and every restart after the first. Nothing happened. |
| **`HealedByReload`** | **`info`** | a package was installed into this live session and the bus was repaired. Worth a line. |
| `StillMissing` | `debug` | usually just means `omnibridge-gui` is not installed. |
| `Unavailable(..)` | `debug` | headless, `ssh`, container. Normal. |

---

## 6. What this branch did not do

| # | Left open | Where it belongs |
| --- | --- | --- |
| 1 | Shipping the `.service` file in the RPM at all | **Phase 3.** The self-heal repairs a file the package does not yet install; until then it correctly reports `StillMissing` on a packaged machine. |
| 2 | Gate **L8** — activation immediately after a real `dnf install` into a live session | Phase 6. Needs the Phase 3 package. §5 here proves the mechanism; L8 proves the product. |
| 3 | Gate **L7** — cold activation opening the Quick Panel | Phase 6. Needs a graphical session. |
| 4 | Anything on the system bus | Nowhere. It is forbidden and asserted against. |

---

## 7. Verdict

**IMPLEMENTED AND MEASURED.**

The mechanism works on the bus implementation that needs it (`dbus-broker`,
measured live: `HealedByReload`) and is a correct no-op on the one that does
not (`dbus-daemon`, measured: `AlreadyActivatable`, zero reloads). It touches
the user session bus only, reloads at most once, matches the name exactly,
never polls, never needs root, changes no protocol or trust state, and does not
make D-Bus a condition of the daemon starting.

The audit's premise turned out to be narrower than it was written, and that is
recorded in §1 rather than smoothed over.
