# U2 HARDENING — P2: Battery Absence / Fake 0%

**Branch:** `fix/u2-p2-battery-absence`
**Base:** `9343328` (develop, with P1 multi-peer merged)
**Date:** 2026-09-16
**Scope:** the single U2 defect P2. Nothing else.

> `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is **not** modified by this branch. It remains
> untracked evidence for baseline `9792330`, describing the code as it was certified —
> with the defect present. This report is the separate remediation record.

---

## 1. Defect of record

A Linux machine with **no battery at all** was advertised to Android as a battery at 0%,
and the phone rendered `Battery 0 percent` / `0%` on that computer's card.

Reproduced live in this session on `anyflow-u2404`, against a daemon built from the
certified baseline, before any fix was loaded:

```text
anyflow-u2404:  ls -A /sys/class/power_supply/   → (0 entries)
anyflowd (baseline): "UPower available; this machine will report its own battery"
tablet logcat:  BatteryCapability: battery update from 135A C045 BFE9 F0A5: 0%
tablet UI:      Connected | anyflow-u2404 | Desktop · Linux | Battery 0 percent | 0%
```

The Android side was not inventing the value. The desktop sent `percentage = 0`.

---

## 2. Root cause

**A successful D-Bus call was treated as proof that a battery exists.**

UPower publishes `/org/freedesktop/UPower/devices/DisplayDevice` on *every* machine,
battery or not, and it answers every property read on a battery-less host. The old reader
asked for exactly two properties:

```rust
let percentage: f64 = proxy.get_property("Percentage").await.ok()?;
let state: u32      = proxy.get_property("State").await.ok()?;
Some((percentage, state))
```

Both reads succeed on a battery-less machine and yield `0.0` and `0`. Neither
`IsPresent`, `PowerSupply` nor `Type` — the three properties UPower provides for exactly
this question — was consulted anywhere in the tree.

The `connect()` probe carried a comment stating the intent ("so a machine without a
battery reports `None` here"), but implemented it as `read_raw().await?` — a call that
cannot fail on a machine with UPower running, whether or not that machine has a battery.
The intent was documented and never realised.

---

## 3. UPower evidence

Captured this session, one guest at a time. All three battery-less hosts agree, across
three different UPower versions, so the rule is not distro-specific:

| Host | UPower | `/sys/class/power_supply` | `Type` | `IsPresent` | `PowerSupply` | `Percentage` | `State` | `IconName` |
|---|---|---|---|---|---|---|---|---|
| `anyflow-u2404` (Ubuntu 24.04) | 1.90.3 | **0 entries** | `0` | `false` | `false` | `0.0` | `0` | `battery-missing-symbolic` |
| `anyflow-d13` (Debian 13) | 1.90.9 | **0 entries** | `0` | `false` | `false` | `0.0` | `0` | `battery-missing-symbolic` |
| `anyflow-u2604` (Ubuntu 26.04.1) | 1.91.1 | **0 entries** | `0` | `false` | `false` | `0.0` | `0` | `battery-missing-symbolic` |
| **Fedora host** (Lenovo, real `BAT0`) | system | `ADP0 BAT0 …` | **`2`** | **`true`** | **`true`** | **`77.0`** | `5` | `battery-full-charging-symbolic` |

UPower itself calls the battery-less case missing — `battery-missing-symbolic`, and
`DisplayDevice` is the *only* device on the bus (`upower -e` lists nothing else). The
aggregate exists; the battery does not.

---

## 4. Presence rule chosen

```rust
pub fn presence(device: &DisplayDevice) -> BatteryPresence {
    if device.kind == UPOWER_KIND_BATTERY && device.is_present && device.power_supply {
        BatteryPresence::Present
    } else {
        BatteryPresence::Absent
    }
}
```

A `DisplayDevice` is a real system battery **iff all three** hold:

| Property | Value required | Why this one |
|---|---|---|
| `Type` | `2` (`UP_DEVICE_KIND_BATTERY`) | The aggregate reports `UNKNOWN` when it stands in for nothing. Also excludes `LINE_POWER`. |
| `IsPresent` | `true` | UPower's direct statement that the cell is installed — a laptop with its battery pulled still reports `Type = Battery`. |
| `PowerSupply` | `true` | Separates a battery that powers the machine from a cell in a peripheral (mouse, headset, the `hidpp_battery_*` entries on the Fedora host). |

**What the rule deliberately does not consult:**

- **`Percentage`.** Presence is never inferred from charge. `percentage == 0` means a flat
  battery, and silencing it would delete the reading a person most needs. This is §2.C of
  the brief and the single thing the fix may not be.
- **`State`.** *Presence and charge state are different questions.* A present battery may
  sit at `UP_DEVICE_STATE_UNKNOWN` while a charge controller settles. Test B4 pins this: a
  battery at 55% with `State = 0` is still `Present`, and only its charging state is
  `UNSPECIFIED`.
- **`IconName`.** `battery-missing-symbolic` is a correct signal but it is a presentation
  string, not a structured property. The three above already answer the question.

A fourth outcome is modelled separately: **unreadable**. If the system bus or a property
read fails, that is "we could not ask", not "there is no battery" — see §6.

---

## 5. Architecture before

```text
UPowerReader::connect()
  └─ zbus::Connection::system()
  └─ read_raw()  ──►  get_property("Percentage")   ─┐ both succeed on a
                      get_property("State")         ─┘ battery-less host
  └─ Some(reader)                    ← ALWAYS, whenever UPower is running

daemon: if let Some(upower) = connect() → with_local_source(upower)
        log "UPower available; this machine will report its own battery"

on_peer_connected → src.read() → Some(BatteryReading { percentage: 0, … })
                              → encode → wire → Android renders "Battery 0 percent"
```

## 6. Architecture after

```text
UPowerReader::detect() -> LocalBattery
  └─ system bus unreachable ──────────────────────► Unavailable
  └─ read_display_device()  (Type, IsPresent, PowerSupply, Percentage, State)
        ├─ None (a property read failed) ─────────► Unavailable
        └─ Some(device) ─► presence(&device)
                             ├─ Present ──────────► Present(reader)
                             └─ Absent ───────────► Absent

daemon: match detect()
   Present(u)  → with_local_source(u)   "…this machine will report its own battery"
   Absent      → no source              "UPower available; no system battery present;
                                          battery.v1 is receive-only"
   Unavailable → no source              "no local battery source; battery.v1 is receive-only"

on_peer_connected → self.local is None → NO FRAME SENT
                  → Android has nothing to render → no battery pill at all
```

`battery.v1` stays registered in **all three** cases — see §8.

**Runtime semantics (§6 of the brief), stated explicitly:**

- Presence is **probed once at daemon start**, in `detect()`. This is where the daemon
  decides what to log and whether to install a source.
- Presence is **also re-checked on every read**: `LocalBatterySource::read` calls
  `read_display_device()` and passes the result through `reading_of`, which re-applies the
  presence rule. A battery removed mid-run therefore stops being reported rather than
  freezing at its last value.
- **A read is only taken on `on_peer_connected`**, so in practice the re-check happens once
  per session. There is no polling loop and none was added.
- **"Temporarily unreadable" is never "definitely absent."** An unreadable property yields
  no reading (`None` — say nothing) and does **not** uninstall the source, so a later
  session can succeed. The daemon's log distinguishes the two cases, which is the only
  place the distinction is visible to a person. This is deliberately not a hotplug
  architecture; see debt 1.

---

## 7. Files changed

| File | Δ | What |
|---|---|---|
| `capabilities/battery/src/upower.rs` | +215 / −54 | `DisplayDevice` snapshot type; `presence`; `reading_of`; `LocalBattery`; `detect()` replaces `connect()`; `map_state` completed. |
| `capabilities/battery/src/lib.rs` | +17 / −3 | Re-exports the presence API; documents what `LocalBatterySource: None` means. |
| `daemon/src/main.rs` | +24 / −6 | Three-way `match` on `detect()`; the misleading log line is no longer emitted on a battery-less host. |
| `capabilities/battery/tests/battery_presence.rs` | **new**, 452 | 23 tests. No D-Bus. |
| `capabilities/battery/tests/real_upower.rs` | **new**, 47 | 1 `#[ignore]`d probe of real hardware. |
| `android/.../test/BatteryReadingTest.kt` | **new**, 94 | 5 tests. **No Android production code changed.** |

```text
git diff --stat:  3 files changed, 202 insertions(+), 54 deletions(-)   (+ 3 new test files)
```

### Declared beyond the minimum

`map_state` gained the two `PENDING_*` states:

```rust
5 => ChargingState::NotCharging,   // UP_DEVICE_STATE_PENDING_CHARGE
6 => ChargingState::Discharging,   // UP_DEVICE_STATE_PENDING_DISCHARGE
```

Previously both fell through to `Unspecified`. This is declared rather than buried because
it goes beyond "presence": the brief's §14 requires that on the positive physical case the
charging state *remains correct*, and the physical Fedora host sits permanently at
`State = 5` because it has a charge threshold enabled. Without this the only real battery
available would have reported its state as unknown. Two lines, in the function already
being rewritten, covered by test B6, and it cannot affect presence.

---

## 8. Protocol impact

**None. No protobuf change, no wire change, no new protocol state, and `SCHEMA_VERSION` is
untouched.**

Per §4 of the brief, the existing architecture was checked first, and it already expresses
absence truthfully: `BatteryCapability.local: Option<Arc<dyn LocalBatterySource>>`. A
machine with no battery simply has no source, sends no `BatteryState` frame, and the peer
has nothing to render. The bug was never that absence could not be expressed — it was that
`detect()` never concluded absence.

**`battery.v1` is still registered on a battery-less desktop, deliberately.** The
capability is bidirectional and registration is what lets this machine *receive* the
phone's battery — the common case, since most desktops have no battery and every phone
does. Dropping the capability to signal "I have no battery" would have broken the far more
useful direction to fix the less useful one. Verified physically in §16 and pinned by tests
C6, C7 and C8.

---

## 9. Tests added

**`battery_presence.rs` — 23 tests, no system bus.** The D-Bus call is separated from the
decision, and the tests drive the decision with the exact property shapes real machines
produce (§3).

*Present batteries — none of these may regress:*

| # | Test | Covers |
|---|---|---|
| B1 | `present_battery_discharging_is_reported` | 79%, discharging (brief §9.1) |
| B2 | `present_battery_charging_is_reported` | 79%, charging (§9.2) |
| B3 | `present_battery_at_zero_percent_is_still_a_battery` | **mandatory §8** — `IsPresent/PowerSupply/Type=Battery` at 0% is a battery at 0% |
| B4 | `present_battery_with_unknown_state_is_still_present` | `State=Unknown` does not hide a battery (§9.4, §3) |
| B5 | `present_battery_full` | 100%, fully charged |
| B6 | `present_battery_pending_charge_is_not_charging` | `State=5`, the physical host's steady state |

*Absent batteries — each is a shape a real machine produces:*

| # | Test | Covers |
|---|---|---|
| A1 | `battery_less_machine_reports_nothing` | the certified VM aggregate (§9.5) |
| A2 | `is_present_false_is_absent` | `IsPresent=false` alone (§9.5) |
| A3 | `power_supply_false_is_absent` | `PowerSupply=false` alone (§9.6) |
| A4 | `non_battery_kinds_are_absent` | `Type` ∈ {Unknown, LinePower, Mouse} (§9.7) |
| A5 | `absence_does_not_depend_on_the_percentage` | a battery-less host reading 50% is still absent |

*Malformed and bounds (§9.8, §9.10):*

| # | Test | Covers |
|---|---|---|
| M1 | `non_finite_percentage_reports_nothing` | `NaN`/±∞ — a `NaN as u32` saturates to **0**, so the naive path turns a broken sensor into a confident "0%" |
| M2 | `percentages_are_clamped_to_the_wire_range` | >100 and <0 clamp, pre-existing behaviour preserved |
| M3 | `percentages_round_to_nearest` | 78.6 → 79 |
| M4 | `unknown_state_value_keeps_the_battery` | a future `State` value does not hide a battery |

*The capability seam (§10) — absence silences sending and changes nothing about receiving:*

| # | Test | Covers |
|---|---|---|
| C1 | `a_machine_with_a_battery_sends_on_connect` | battery present → `battery.v1` frame on the wire |
| C2 | `a_flat_battery_is_still_announced` | **§8 end to end** — a real 0% reaches the wire |
| C3 | `a_battery_less_machine_sends_no_frame` | **the defect** — no frame at all |
| C4 | `no_local_source_sends_no_frame` | the shape the daemon builds for `Absent`/`Unavailable`, and for a build without the feature |
| C5 | `an_unreadable_source_sends_no_frame` | **§9.9** — D-Bus unreadable never becomes 0% either |
| C6 | `a_battery_less_machine_still_receives_the_peers_battery` | **§10/§15 independence** |
| C7 | `a_machine_with_no_source_still_receives` | same, with no source at all |
| C8 | `the_capability_is_advertised_either_way` | `battery.v1` is registered regardless |

**`real_upower.rs` — 1 test, `#[ignore]`d.** An opt-in probe of the machine it runs on,
for physical regression runs. Never part of the unit gate; needs `--ignored` to run.

**`BatteryReadingTest.kt` — 5 tests.** The phone's half of the contract, at the existing
`BatteryCapability.decode` seam. A frame that *does* arrive saying 0% is a real flat
battery and must decode as 0; absence is expressed by the frame not arriving, never by a
sentinel inside one. **No Android production code was changed** — see §11.

### Mutation check

The suite was verified to fail against both wrong implementations, not merely to pass
against the right one:

```text
MUTATION 1 — the pre-fix code (presence never consulted):
  7 failed: battery_less_machine_reports_nothing, is_present_false_is_absent,
            power_supply_false_is_absent, non_battery_kinds_are_absent,
            absence_does_not_depend_on_the_percentage,
            a_battery_less_machine_sends_no_frame,
            a_battery_less_machine_still_receives_the_peers_battery

MUTATION 2 — the forbidden `if percentage <= 0 { None }`:
  6 failed: present_battery_at_zero_percent_is_still_a_battery,
            a_flat_battery_is_still_announced,
            percentages_are_clamped_to_the_wire_range,
            absence_does_not_depend_on_the_percentage,
            power_supply_false_is_absent, non_battery_kinds_are_absent
```

The shortcut the brief forbids is caught by name.

---

## 10. Desktop test results

```text
$ cargo test -p anyflow-capability-battery --locked
   battery_presence.rs        23 passed   0 failed   0 ignored

$ cargo test -p anyflow-capability-battery --features upower --locked
   battery_presence.rs        23 passed   0 failed   0 ignored
   real_upower.rs              0 passed   0 failed   1 ignored

$ cargo test -p anyflow-capability-battery -p anyflow-daemon \
             -p anyflow-core -p anyflow-runtime --locked -j 2
   TOTAL                     335 passed   0 failed   1 ignored
     core: protocol 12 · pairing 21 · identity_and_store 24 · identity_states 20 ·
           identity_seam 8 · notifications_protocol 47 · portable_boundary 9 · lib 25
     daemon: e2e 18 · files 36 · notifications 36 · clipboard 21 · wire 10 ·
             sessions 6 · listen 5 · control 4
     runtime: lib 10       battery: 23

$ cargo fmt --all --check
   (clean)

$ cargo clippy -p anyflow-capability-battery --all-targets --locked
   0 warnings
$ cargo clippy -p anyflow-capability-battery --all-targets --features upower --locked
   0 warnings
$ cargo clippy -p anyflow-daemon --all-targets --locked
   0 warnings
```

Both feature states are clean: a `now_ms` dead-code warning that the first draft introduced
in the no-feature build was found by clippy and gated before this was recorded.

The known unrelated `real_dbus` tautological-assertion clippy failure in the notifications
crate was **not** touched and is not counted here — out of scope per §20.

### Real hardware probe

```text
$ cargo test -p anyflow-capability-battery --features upower --test real_upower \
      --locked -- --ignored --nocapture

PRESENT percentage=77 charging_state=NotCharging
test result: ok. 1 passed
```

Taken on the Fedora host while `/sys/class/power_supply/BAT0/capacity` read `77` and
`status` read `Not charging`. See §15.

---

## 11. Android test results

```text
$ ./gradlew --offline :app:testDebugUnitTest
TOTAL   tests=562   passed=562   failed=0   skipped=0
  BatteryReadingTest  5   (new)
  MultiPeerRoutingTest 13   PeerTargetTest 18   UiMappingTest 31   (P1, still green)
```

Baseline was 557 (the P1 total); this branch adds **5**.

**No Android production code changed**, and per §11 of the brief none was manufactured. The
app already models absence correctly and this was confirmed by reading the path end to end:

```kotlin
BatteryCapability.remote : AtomicReference<Reading?>   // null until a frame arrives
  onPeerDisconnected → remote.set(null)                // cleared when a session ends
MainActivity:234   remoteBatteryPercent = app.battery.remoteReading()?.percentage
DevicesScreen:118  batteryPercent = if (connected) state.remoteBatteryPercent else null
Cards.kt:137       if (batteryPercent != null) { AnyFlowBatteryPill(...) }
```

No frame → `null` → **no pill is composed at all**, which is §7's preferred behaviour
("no battery percentage row/icon for that desktop") and needs no new UX.

The stale path was also checked rather than assumed. `PeerConnection.close()` calls
`onPeerDisconnected` for every negotiated capability from a `finally` block, so the value
is cleared when a session ends, including on a P1 retarget. **Verified physically**: with
the baseline daemon showing `Battery 0 percent`, tapping Disconnect removed the pill
immediately, and reconnecting to the fixed daemon never brought it back (§12). Retargeting
across machines was also exercised — u2404 → d13 → u2604 — with no value carried over.

---

## 12. Ubuntu 24.04 physical regression → **PASS**

`anyflow-u2404`, `192.168.68.75`, fingerprint `135A C045 BFE9 F0A5` — the preserved U2
identity. Both binaries were built in the guest from the same checkout, so this is a true
A/B on one machine, in one session.

**Before — baseline `9792330`, the defect reproduced live:**

```text
daemon:  UPower available; this machine will report its own battery
logcat:  ConnectionService: CONNECT_SUCCESS round=96 endpoint=192.168.68.75:55432
         ConnectionService: SESSION_START
         BatteryCapability: battery update from 135A C045 BFE9 F0A5: 0%
tablet:  Connected | anyflow-u2404 | Desktop · Linux | Battery 0 percent | 0%
```

**After — the same guest, the fixed binary, nothing else changed:**

```text
daemon:  UPower available; no system battery present; battery.v1 is receive-only
         capabilities registered capabilities=["battery.v1", "clipboard.v1",
                                               "files.v1", "notifications.v1"]
         session established peer=573C CB84 DA6C 993B
logcat:  CONNECT_SUCCESS → SESSION_START       (no `battery update` line at all)
tablet:  Connected | anyflow-u2404 | Desktop · Linux | Clipboard, not allowed |
         Files, allowed | Battery, allowed | Disconnect
```

- `/sys/class/power_supply` — 0 entries; `DisplayDevice` present; presence properties all negative (§3). ✔
- No `Battery N percent`, no `N%` anywhere on the Devices screen. ✔
- `Battery, allowed` remains — that is the **grant chip**, not a reading, and it is untouched. ✔
- Trust intact: `paired yes`, `granted battery.v1, clipboard.v1, files.v1, notifications.v1`. ✔
- Session established and held; no crash, no reconnect storm. ✔

One procedural note recorded for honesty: the first "after" attempt appeared to still show
0%, and the cause was the test harness, not the code — the baseline binary had been renamed,
so `pkill -x anyflowd` did not match it, it kept the port, and the fixed daemon exited with
`Address already in use`. Once the right process was stopped the fixed daemon took the port
and the result above is what followed.

---

## 13. Debian 13 physical regression → **PASS**

`anyflow-d13`, `192.168.68.59`, fingerprint `B52C DA20 46ED 006D`. UPower **1.90.9** — a
different version from Ubuntu 24.04's, which is why this is worth running separately.

```text
/sys/class/power_supply:  0 entries
DisplayDevice: Type=0  IsPresent=false  PowerSupply=false  Percentage=0.0  State=0
               IconName='battery-missing-symbolic'

daemon:  UPower available; no system battery present; battery.v1 is receive-only

logcat:  13:44:17  TARGET_CHANGED peer=B52C DA20 46ED 006D
         13:44:17  RETRY_CANCELLED reason=woken early
         13:44:22  CONNECT_ATTEMPT round=9 endpoint=192.168.68.59:55432 index=0 of=2
         13:44:22  CONNECT_SUCCESS → SESSION_START
         (no `battery update` line)

tablet:  Connected | anyflow-d13 | Desktop · Linux | … | Disconnect
         no "Battery N percent", no "N%"
```

This retarget also carries the stale check: the tablet came here **from** `anyflow-u2404`,
which minutes earlier had been displaying `Battery 0 percent` under the baseline daemon. No
value carried across.

Desktop side: `paired yes · connected yes · state connected · granted battery.v1,
clipboard.v1, files.v1, notifications.v1 · battery 79% (NotCharging, 22s old)` — the
tablet's battery, arriving normally at a desktop that has none of its own.

---

## 14. Ubuntu 26.04 physical regression → **PASS**

`anyflow-u2604`, `192.168.68.78`, fingerprint `1315 96BD 9834 BA6F`. Ubuntu 26.04.1 LTS,
UPower **1.91.1** — the third distinct version.

```text
/sys/class/power_supply:  0 entries
DisplayDevice: Type=0  IsPresent=false  PowerSupply=false  Percentage=0.0  State=0

daemon:  UPower available; no system battery present; battery.v1 is receive-only

logcat:  13:47:35  TARGET_CHANGED peer=1315 96BD 9834 BA6F
         13:47:40  CONNECT_ATTEMPT round=16 endpoint=192.168.68.78:55432 index=0 of=2
         13:47:40  CONNECT_SUCCESS → SESSION_START
         (no `battery update` line)

tablet, all three cards at once:
         Available :: anyflow-u2404
         Available :: anyflow-d13
         Connected :: anyflow-u2604
         no "Battery N percent" and no "N%" anywhere on the screen
```

**Stability (§13.7):** re-checked after ~6 minutes of continuous session —
`connected yes · state connected · last frame 37s ago`, daemon uptime 367 s, and
`grep -cE "panic|ERROR"` over the whole daemon log returned **0**.

All three guests were run **sequentially**, one at a time, ballooned at runtime with
`virsh setmem` (never `--config`). Each guest's checkout was returned to a clean
`9792330` afterwards and every domain was shut down cleanly; the persisted domain XML was
not touched.

---

## 15. Real Fedora battery — positive regression → **PASS**

The physical host, a Lenovo laptop with a real `BAT0`. No power-management setting was
changed.

**Recorded before the test:**

```text
$ ls /sys/class/power_supply
ADP0  BAT0  hidpp_battery_2  hidpp_battery_3  ucsi-source-psy-USBC000:001

$ cat /sys/class/power_supply/BAT0/status    → Not charging
$ cat /sys/class/power_supply/BAT0/capacity  → 77
$ cat /sys/class/power_supply/BAT0/present   → 1

UPower DisplayDevice:
  Type=2  IsPresent=true  PowerSupply=true  Percentage=77.0  State=5
  Energy=43.22  EnergyFull=55.82  IconName='battery-full-charging-symbolic'
```

Note `hidpp_battery_2/3` — a mouse and a keyboard, both with cells in them. They are
exactly what `PowerSupply` exists to exclude, and `DisplayDevice` correctly reports
`PowerSupply = true` for the system battery.

**The fixed backend against that hardware, through the full public path
(`UPowerReader::detect()` → `LocalBatterySource::read()` — the same two calls the daemon
makes):**

```text
PRESENT percentage=77 charging_state=NotCharging
```

- The real battery **is** detected — `LocalBattery::Present`, no false negative. ✔
- The percentage is real and matches `/sys` exactly: **77** vs `capacity = 77`. ✔
- The charging state is correct: `NotCharging` vs `status = Not charging`. `State = 5`
  (`PENDING_CHARGE`) is this machine's steady state because it holds a charge threshold,
  and it is the case the §7 `map_state` addition exists for — without it this would have
  read `Unspecified`. ✔

**The daemon on the same host:**

```text
$ anyflowd
INFO anyflowd: local identity name=Fedora fingerprint=DF65 D3E4 BA28 EDF9
INFO anyflowd: UPower available; this machine will report its own battery
INFO anyflowd: capabilities registered capabilities=["battery.v1", "clipboard.v1",
                                                     "files.v1", "notifications.v1"]
```

**Limitation, recorded as §14 of the brief permits.** The final Android *presentation*
check on this host was not performed. Pairing needs an optical QR scan with the tablet's
rear camera past its PIN lock — a step only a person can take — and the brief makes that
check conditional on it being practical. Desktop-to-desktop pairing is not a substitute:
the desktop CLI can only *show* a pairing code, never scan one. What was verified instead
is the whole desktop path on real hardware (above) plus a real TLS session against this
daemon driven by the `fake_phone` example, which negotiated `["battery.v1", "files.v1"]`
and completed a ping/pong; the example installs no tracing subscriber, so it does not print
the frame it received. The renderer on the far side is the part this branch did not change,
and its behaviour was proven on the three VMs in §12–§14.

Cleanup: the temporary `fake_phone` peer was revoked from the host's trust store
(`revoked C026 A789 84CD 354E`) and the second daemon instance and its data directory were
removed.

---

## 16. Android → desktop battery smoke (§15) → **PASS**

The direction that must **not** be affected by local battery absence. Read from each
battery-less desktop while the tablet was connected to it:

| Desktop | `anyflow status` |
|---|---|
| `anyflow-u2404` | `battery 79% (NotCharging, 42s old)` |
| `anyflow-d13` | `battery 79% (NotCharging, 22s old)` |
| `anyflow-u2604` | `battery 79% (NotCharging, 21s old)` |

All three also reported `capabilities battery.v1, clipboard.v1, files.v1,
notifications.v1` and `granted battery.v1, …` — the capability is still advertised,
negotiated and granted on a machine that has no battery of its own. Exactly the
independence §10 requires, and the reason §8 keeps registration unconditional.

(The `STALE` marking seen on a longer-held session is the phone's push cadence — the
pre-existing battery-freshness debt noted in the P1 report, out of scope per §20.)

---

## 17. Privacy / logging (§16)

- **No new persisted state of any kind.** No battery history, no new file, no schema change.
  The reading remains in memory only and is dropped on disconnect.
- **Net new log statements: one.** Three `tracing::info!` lines replace two; the added one
  is `"UPower available; no system battery present; battery.v1 is receive-only"`.
- **All three are fixed strings.** No interpolation, no device name, no address, no
  fingerprint, no percentage. The diff contains no new `println!`, `eprintln!`, `Log.*` or
  `Toast`.
- Battery percentages continue to be logged at **debug** only, unchanged.
- No secrets, pairing tokens, private keys, notification content or clipboard content are
  touched by this branch. No telemetry, no network behaviour, no cloud.

---

## 18. Source audit (§17)

```text
$ grep -RIn --exclude-dir=target -E "UPower|upower|DisplayDevice|IsPresent|PowerSupply|power_supply" \
      desktop android browser-extension packaging
```

Every hit accounted for:

| Site | Status |
|---|---|
| `capabilities/battery/src/upower.rs` | The single reader. The only `get_property("Percentage")` in the tree, and it is behind the presence rule. |
| `daemon/src/main.rs:122` | The single consumer, now a three-way `match`. |
| `capabilities/battery/Cargo.toml`, `daemon/Cargo.toml`, `runtime/Cargo.toml` | Feature plumbing (`upower`). No logic. |
| `control/src/lib.rs`, `cli/Cargo.toml`, `gui/Cargo.toml` | Comments naming UPower as a dependency reason. No logic. |
| `packaging/fedora/anyflow.spec:20` | `Recommends: upower` — a weak dependency, and the comment beside it already says the daemon simply does not report a battery without it. Correct as written. |

**Second-path check — is anything still equating "UPower exists" with "a battery exists"?**
No. There is exactly one path from D-Bus to a `BatteryReading`, and it passes through
`presence()`.

**The receiving/reporting side was audited too**, and it already uses omission rather than a
zero sentinel, on both platforms:

| Site | Shape |
|---|---|
| `control/src/lib.rs:502,543` | `battery: Option<BatteryReport>` |
| `runtime/src/server.rs:246` | `if is_connected { battery_report(...) } else { None }` |
| `gui/src/views/dashboard.rs:111` | `if let Some(battery) = &device.battery` |
| `cli/src/main.rs:439` | `if let Some(b) = &d.battery` |
| `ui/DevicesScreen.kt:118` + `Cards.kt:137` | `Int?`, pill composed only when non-null |

No component anywhere renders a default `0`.

---

## 19. Remaining debts

None blocking.

1. **Presence is not hotplug-aware.** It is decided at daemon start and re-checked on each
   read, and a read is only taken on connect. A battery inserted into a running,
   already-connected machine is therefore not noticed until the next session. Deliberately
   not widened into a hotplug architecture per §6 of the brief; UPower's
   `PropertiesChanged` signal is the obvious future seam.
2. **`Unavailable` and `Absent` differ only in the log.** Both produce a receive-only
   capability, which is the right behaviour today. If a desktop UI ever wants to say "your
   battery could not be read" as distinct from "this machine has none", the enum already
   carries the distinction and only the presentation is missing.
3. **`UP_DEVICE_STATE_EMPTY` (3) still maps to `NotCharging`.** Pre-existing, preserved
   deliberately rather than churned: the wire enum has no `Empty`, and the percentage
   already conveys it. Noted because it was read closely during this work.
4. **The Android presentation check on the real Fedora battery is outstanding**, for the
   reason in §15. It needs one QR scan by a person.

---

## 20. Scope check (§20)

Verified **not** fixed, changed, or touched by this branch: notification capability
convergence; clipboard sensitive test harness; the notifications `real_dbus` clippy
assertion; CI clippy; Android CI; Debian packaging docs; incoming-file GUI approval; QR
scanner camera orientation; trust-store ordering cosmetics; branding / logo / palette;
Quick Panel; KDE.

`git diff --name-status` lists three files, all of them on the battery path. The only
change beyond the three files the brief implies is the `map_state` completion inside
`upower.rs`, declared in §7.

**P1 remains green:** `MultiPeerRoutingTest` 13, `PeerTargetTest` 18, `UiMappingTest` 31,
all passing in the 562-test Android run, and P1's routing was exercised repeatedly on
hardware during this run (three retargets across three desktops, all honoured).

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` is untracked, unmodified, unstaged, and its historical
results are quoted here only as the baseline they were — never rewritten as if they had
used the fix.

---

## 21. Git

```text
$ git status --short
 M desktop/capabilities/battery/src/lib.rs
 M desktop/capabilities/battery/src/upower.rs
 M desktop/daemon/src/main.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? U2-HARDENING-P2-BATTERY-ABSENCE.md
?? android/app/src/test/java/io/github/yurisismotto/anyflow/BatteryReadingTest.kt
?? desktop/capabilities/battery/tests/

$ git diff --check
(clean)

$ git diff --stat
 desktop/capabilities/battery/src/lib.rs    |  17 ++-
 desktop/capabilities/battery/src/upower.rs | 215 +++++++++++++++++++++++------
 desktop/daemon/src/main.rs                 |  24 +++-
 3 files changed, 202 insertions(+), 54 deletions(-)

$ git diff --name-status
M	desktop/capabilities/battery/src/lib.rs
M	desktop/capabilities/battery/src/upower.rs
M	desktop/daemon/src/main.rs
```

Nothing was staged, committed, pushed, or opened as a PR.

---

## 22. Verdict

**NEGATIVE side.** All three preserved U2 Linux guests — Ubuntu 24.04, Debian 13 and
Ubuntu 26.04, across UPower 1.90.3, 1.90.9 and 1.91.1 — have no physical battery, still
publish a `DisplayDevice` that answers every property read, and are no longer treated as a
battery. None of them sends a `battery.v1` frame, and the tablet renders no percentage and
no battery icon for any of them. On Ubuntu 24.04 this was shown as a same-session A/B
against a binary built from the certified baseline, which produced `Battery 0 percent` on
the tablet minutes earlier.

**POSITIVE side.** The Fedora host's real `BAT0` is still detected, still reports its real
percentage (77, matching `/sys` exactly), and its charging state is correct. A battery that
is physically present and genuinely flat is still reported as 0% — asserted at the presence
rule, at the wire encoding, and on the Android decode side, and the suite fails by name
against `if percentage <= 0 { None }`.

Neither side was waived.

```text
U2 P2 BATTERY ABSENCE REMEDIATION: PASS
BATTERYLESS HOST NO LONGER REPORTED AS 0%
REAL BATTERY REGRESSION PASS
```
