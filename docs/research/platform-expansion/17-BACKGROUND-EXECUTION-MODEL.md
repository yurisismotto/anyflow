# 17 — Background execution model

| Field | Value |
| --- | --- |
| **Title** | The AnyFlow Agent: one concept, five lifetimes |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | How AnyFlow stays running on each platform, and what "running" means there. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for Linux and Android; OFFICIAL DOC VERIFIED for Windows/Apple mechanisms. |
| **⚠ Verification update** | The **AnyFlow Agent** concept is confirmed, and two lifetimes are sharpened. **Windows:** a Session 0 service *cannot* see the user's clipboard (*"Services cannot directly interact with a user as of Windows Vista"*, noninteractive window station) — and `AddClipboardFormatListener` needs an `HWND` **and a message pump**, so the agent is a windowed process. **macOS:** *"the exception for `launchd` daemons doesn't apply to `launchd` agents"* — a per-user agent **does** face the Local Network prompt (macOS 15+), and must **not exit** on a network failure (Apple FB16131937). See [26 §9.1, §V-05](26-EXTERNAL-VERIFICATION-CLOSEOUT.md). |
| **Related documents** | [08](08-WINDOWS-FEASIBILITY.md), [10](10-MACOS-FEASIBILITY.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [03](03-PLATFORM-CAPABILITY-MATRIX.md) |

---

## 1. Why this document exists

The word "daemon" appears throughout AnyFlow's code and documentation. It is accurate on Linux
and misleading everywhere else. Carrying it forward would produce, in order: a Windows Service
that cannot see the clipboard, a macOS `LaunchDaemon` running as root with no user pasteboard,
and an iOS design that assumes a process which the OS will not let exist.

**Naming this properly is not cosmetic.** The Windows agent-vs-service decision
([08 §5](08-WINDOWS-FEASIBILITY.md)) follows directly from noticing that "daemon" was a Linux
word, not a universal one.

---

## 2. The abstraction

> **AnyFlow Agent** — the process that owns the device identity, holds the trust store,
> maintains authenticated sessions with paired peers, and hosts the capability handlers. It
> runs **as one specific human user, unprivileged, inside that user's interactive context**,
> for as long as the platform permits.

Three invariants, true on every platform:

1. **Per user, never per machine.** Identity is a relationship between a peer and *a person's*
   device. On a shared Windows PC that means one agent per interactive session
   ([09 §7](09-WINDOWS-SECURITY-AND-INTEGRATION.md)).
2. **Unprivileged.** No root, no `LocalSystem`, no elevation. `daemon/src/main.rs` already
   states this for Linux: *"It needs no root, no capabilities, and no system-wide state."*
3. **In the interactive context.** The clipboard, the Downloads folder and the human are all
   there. This is what rules out Windows Session 0 and macOS `LaunchDaemon`.

What varies is **lifetime**, and that variation is the whole content of §3.

---

## 3. Lifetime per platform

| | Linux | Windows | macOS | Android | iOS/iPadOS |
| --- | --- | --- | --- | --- | --- |
| **Mechanism** | `systemd --user` unit (+ XDG autostart) | User-session agent process | `SMAppService` login-item agent | `connectedDevice` foreground service | **The app itself.** No agent |
| **Lifetime** | Login → logout | Login → logout | Login → logout | While a connection exists or is being established | **Foreground only** |
| **User session** | Required | Required | Required | n/a | n/a |
| **System service?** | ❌ deliberately | ❌ Session 0 has no clipboard | ❌ `LaunchDaemon` has no user context | ❌ | ❌ impossible |
| **Starts at login** | `systemctl --user enable` (manual today) | MSIX `windows.startupTask`, or `HKCU\…\Run` | `SMAppService.register()` | ❌ **deliberately not at boot** | ❌ impossible |
| **Survives screen lock** | ✅ — but the clipboard does not: `wl-copy`/`wl-paste` block for a seat behind the lock screen, which the backend reports as `TimedOut` | ✅ | ✅ | ✅ | n/a |
| **Survives sleep** | ✅ process; sessions must re-establish | ✅ same | ✅ same; App Nap throttles timers | ✅ subject to Doze | n/a |
| **Survives network change** | ✅ `enable_addr_auto()` + reconnect | needs verification | needs verification | ✅ handled in `ConnectionCoordinator` | n/a |
| **OS restrictions** | none | Session 0 isolation | App Nap, TCC, notarization | Doze, App Standby, FGS type rules | **Suspension; sockets reclaimed** |
| **Supervised/restarted** | `Restart=on-failure` | SCM only for services → **none**; must self-handle | `launchd` `KeepAlive` | System-managed | ❌ |

### 3.1 Two rows deserve emphasis

**Linux screen lock.** This is a shipping, certified behaviour that reads as a bug and is not:
`backend/mod.rs` documents `BackendError::TimedOut` as *"on GNOME this is the normal signal
that the session is locked, not an error worth alarming about: `wl-copy` and `wl-paste` need a
seat and a serial that the compositor will not grant behind a lock screen, and they wait rather
than fail."* Every platform needs an equivalent honest answer — Windows and macOS do **not**
block the clipboard behind a lock screen, so they will behave differently, and that difference
should be documented rather than smoothed over.

**Windows has no supervisor.** A Linux daemon gets `Restart=on-failure` for free. A Windows
user-session agent gets nothing: if it crashes it is gone until next login. Options are a
watchdog (another process — more surface), relying on the UI to relaunch it, or simply not
crashing and logging when it does. **Recommendation: no watchdog in v1.** Log to the Event Log,
let the UI offer "start AnyFlow", and treat repeated crashes as the bug they are.
**WIN-010.**

---

## 4. Android is the precedent, not the exception

Worth reading the Android design as a *template for constrained platforms* rather than as
Android-specific work.

`AndroidManifest.xml`, with its own comments:

- `connectedDevice` foreground service type: *"this service exists only while an active
  connection to a paired computer exists or is being established. It is NOT a `dataSync` daemon
  and it is not started at boot."*
- `CHANGE_WIFI_MULTICAST_STATE` is held *"because we genuinely use multicast"* — and the
  manifest explicitly notes that this permission also happens to qualify the service for
  `connectedDevice`, *"not a coincidence or a loophole."*
- A block listing what is deliberately absent: no accessibility service, no notification
  listener, no `QUERY_ALL_PACKAGES`, no location, no `MANAGE_EXTERNAL_STORAGE`.

That is the right posture for every constrained platform: **claim the narrowest capability that
honestly describes what you do, and write down what you refused.** iOS's empty
`UIBackgroundModes` ([12 §5](12-APPLE-SECURITY-AND-INTEGRATION.md)) is the same statement.

---

## 5. Consequences for the shared core

The runtime library must not assume it runs forever. Concretely:

| Assumption in the code today | Cross-platform reality |
| --- | --- |
| A session lives until the socket dies | On iOS it dies whenever the app backgrounds |
| Liveness is measured in minutes (`daemon/tests` use a virtual clock precisely because *"a suite that waited them out in real time would never be run"*) | Reasonable on desktops; too slow for a mobile client that reconnects constantly |
| Reconnect is an exceptional path | On iOS it is **the** path, and its latency is the product's felt quality |
| State is in memory for the process's life | On iOS the process dies; anything not persisted is lost |
| `on_closed(peer, session_id)` distinguishes overlapping sessions | Already correct, and *more* necessary on mobile where reconnects race |

Three specific items for the shared runtime:

1. **Make reconnect fast and cheap.** `Endpoints.kt`'s sequential, capped, ordered dialling
   is the model. Cache the last-known-good address per peer and try it first.
2. **Persist what must survive a kill.** The trust store already does. Anything ephemeral (a
   pending clip, a partial transfer) must either be recoverable or explicitly abandoned — not
   silently half-present.
3. **Keep the desktop side tolerant of a peer that vanishes without a close.** The session
   layer already handles this via liveness, and `on_closed` takes a `SessionId` so a late close
   from a dead session cannot evict a live one — a defect that was clearly found the hard way.
   Mobile makes it routine.

---

## 6. Naming

Recommendation, applied consistently in new code and docs:

| Context | Term |
| --- | --- |
| Abstract concept | **AnyFlow Agent** |
| Linux process | `anyflowd` (keep — it is a daemon there, and changing it breaks users' unit files) |
| Windows process | `AnyFlowAgent.exe` |
| macOS process | the app's bundled login-item helper |
| Android | `ConnectionService` (keep) |
| iOS | *there is no agent* — say so plainly |

Do **not** rename `anyflowd` or the systemd unit. The cost of breaking existing installations
exceeds the benefit of terminological purity, and the Linux name is accurate.

---

## 7. Backlog

| ID | Item | Priority |
| --- | --- | --- |
| **ARCH-003** | Split `anyflow-daemon` into `anyflow-runtime` (portable) + `anyflow-linux` | Wave 0 |
| **ARCH-008** | Document the Agent concept and its per-platform lifetimes in `docs/architecture/` | Low |
| **WIN-010** | Decide crash-recovery policy for the Windows agent (recommendation: none, log it) | Medium |
| **LINUX-005** | XDG autostart entry alongside the systemd unit | Medium |
| **MAC-007** | `SMAppService` registration, tested on clean install / upgrade / app update | High |
| **ARCH-009** | Cache last-known-good address per peer to speed reconnect | Medium |
