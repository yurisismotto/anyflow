# 13 — Cross-platform discovery

| Field | Value |
| --- | --- |
| **Title** | Keeping `_anyflow._tcp.local.` working on five platforms |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | mDNS/DNS-SD stacks per platform, TXT records, address families, interface and network changes, interoperability testing. |
| **Decision status** | PROPOSED |
| **Evidence** | REPO VERIFIED for AnyFlow's record and behaviour; OFFICIAL DOC VERIFIED for platform APIs; POC REQUIRED for every coexistence claim. |
| **Related documents** | [04](04-LINUX-PORTABILITY.md), [08](08-WINDOWS-FEASIBILITY.md), [10](10-MACOS-FEASIBILITY.md), [11](11-IOS-IPADOS-FEASIBILITY.md), [20](20-SECURITY-THREAT-ANALYSIS.md) |

---

## 1. The invariant

Whatever each platform uses internally, the following must not change, because today's Android
client depends on it and an expansion must not break a shipped device:

| Element | Value | Source |
| --- | --- | --- |
| Service type | `_anyflow._tcp.local.` | `core/src/lib.rs::SERVICE_TYPE` |
| Instance name | the **device id** (128-bit hex), not the human name | `daemon/src/mdns.rs` |
| Host name | `{device_id}.local.` | `daemon/src/mdns.rs` |
| Port | advertised in SRV; `DEFAULT_PORT` 55432 is only a default | `core/src/lib.rs` |
| TXT `v` | `"1"` — TXT schema version | `core/src/discovery.rs` |
| TXT `pv` | `"{min}-{max}"` protocol version range | `core/src/discovery.rs` |
| TXT `id` | device id, lowercase hex, ≤64 chars, hex-validated on parse | `core/src/discovery.rs` |
| TXT `dn` | device name, control chars stripped, ≤64 chars | `core/src/discovery.rs` |
| **Not** in TXT | the identity fingerprint | deliberate — §7 |

The instance name being the *device id* rather than the device name is a small decision that
pays off across platforms: DNS-SD instance names must be unique on the link, and
"two machines called `fedora`" — or, now, "two machines called `MacBook Pro`" — is the common
case. It also means a Windows machine with two logged-in users advertises two distinct
instances with no collision handling needed ([09 §7](09-WINDOWS-SECURITY-AND-INTEGRATION.md)).

---

## 2. Stack per platform

| Platform | Role | Stack | Status |
| --- | --- | --- | --- |
| **Linux** | advertise | `mdns-sd` (in-process, pure Rust) | REPO VERIFIED, shipping |
| **Android** | browse | `NsdManager` + `WifiManager.MulticastLock` | REPO VERIFIED, shipping |
| **Windows** | advertise | `mdns-sd` preferred; `DnsServiceRegister` (windns.h, Windows 10+) as fallback | POC-WIN-02 |
| **macOS** | advertise | `mdns-sd` attempted; `DNSServiceRegister` (dnssd C API) as the budgeted fallback | POC-MAC-02 |
| **iOS/iPadOS** | browse | `NWBrowser` (Network.framework). **Not** `mdns-sd` | POC-IOS-01 |

Two deliberate asymmetries:

- **iOS uses the platform API, everyone else uses the Rust crate.** iOS is the one place where
  running an own responder is both technically hostile (`mDNSResponder` owns 5353) and
  procedurally hostile (local-network privacy is wired into Network.framework). Reusing the
  crate there would buy nothing and risk App Review.
- **macOS is expected to need the fallback.** `mDNSResponder` is the strictest system responder
  of the three desktops. [10 §4](10-MACOS-FEASIBILITY.md) budgets for the platform API rather
  than hoping.

---

## 3. Port 5353 coexistence — the cross-cutting risk

Every desktop platform already runs a system mDNS responder:

| Platform | System responder | Notes |
| --- | --- | --- |
| Linux | `avahi-daemon` (or `systemd-resolved` with mDNS enabled) | AnyFlow has coexisted with it through certification on Fedora |
| Windows | Built-in mDNS (behind the DNS-SD APIs) | Untested with a second responder |
| macOS | `mDNSResponder` | Historically the least tolerant |

mDNS is designed for multiple responders on a *link*, but multiple responders in one *host*
raises three concrete issues:

1. **Binding UDP 5353.** Requires `SO_REUSEADDR` (and on some systems `SO_REUSEPORT`) plus
   multicast group membership. Whether `mdns-sd` sets these appropriately on each OS is exactly
   what the POCs must show.
2. **Hostname conflict.** `mdns.rs` registers `{device_id}.local.`. Since the device id is
   128 bits of randomness, a *name* collision with the system responder's hostname is
   impossible — but conflict *probing* still happens and both responders may answer for
   different names on the same interface.
3. **Interface tracking.** `.enable_addr_auto()` lets `mdns-sd` follow address changes so the
   record stays correct "across Wi-Fi/dock changes without a restart". This uses `AF_NETLINK`
   on Linux — which is why the systemd unit's `RestrictAddressFamilies` includes it — and needs
   an equivalent on Windows (`NotifyIpInterfaceChange`) and macOS (`SCNetworkReachability` or
   the system responder's own handling). **If this silently stops working on a new platform,
   discovery degrades after the first Wi-Fi change and looks like "AnyFlow randomly stops
   finding my computer".**

**Recommendation:** treat "does a second responder coexist" as a **release gate** on each new
desktop platform, not as a detail. It is the single most likely cause of a "works on my
machine, not on yours" report.

---

## 4. Address families

`daemon/src/listener.rs` gets this unusually right and the reasoning should be preserved
verbatim on other platforms:

> *"The daemon listens on every family it advertises, and advertises only what it listens on.
> Getting that wrong is not cosmetic: an mDNS record with an `AAAA` for a daemon bound to
> `0.0.0.0` sends the phone to an address that refuses every connection, and the phone has no
> way to tell that from the computer being asleep."*

`bind_endpoints` discovers whether one `[::]` socket also serves IPv4 **by experiment**,
because `IPV6_V6ONLY`'s default comes from `net.ipv6.bindv6only` and is not safe to assume. It
binds `[::]` first, then `0.0.0.0`, and reads the outcome:

- v4 bind succeeds → the v6 socket was v6-only; both sockets together cover both families
- v4 bind fails with `AddrInUse` → the v6 socket is dual-stack
- v6 bind fails outright → IPv4 only

`mdns::Advertisement::publish` then takes `Families` and calls `daemon.disable_interface(...)`
so the record matches reality.

**On Windows, `IPV6_V6ONLY` defaults to enabled and `AddrInUse` semantics differ**
(`SO_EXCLUSIVEADDRUSE`, and Windows permits some binds Linux rejects). The probe will probably
still produce a correct answer — it is empirical — but "probably" is not good enough for the
mechanism that decides what gets advertised. **POC-WIN-01 must assert the resulting `Families`
value explicitly**, not just that the daemon started.

macOS is BSD-derived and defaults `IPV6_V6ONLY` on, so the probe should find the two-socket
case, matching a common Linux configuration.

---

## 5. Client-side address handling

Android's `Endpoints.kt` is the reference and is better than the naive approach. It:

- takes **all** addresses from the record (A + multiple AAAA, plus a remembered last-known one);
- orders them **IPv4 → routable IPv6 → zoned link-local IPv6 → IPv4 link-local → hostnames**;
- **drops undialable addresses** rather than sorting them: loopback, unspecified, multicast,
  out-of-range ports, and — the case that caused a real defect — a link-local IPv6 address with
  no zone index;
- caps attempts at `MAX_PER_ROUND = 4`, explicitly because *"a hostile or broken responder can
  publish an unbounded number of records, and 'try them all' would be a battery attack"*;
- deliberately does **not** implement Happy Eyeballs, because racing sockets on a battery
  device is the wrong trade.

**Any iOS client must reproduce this behaviour, not reinvent it.** The ordering rationale
(IPv4 first because consumer mesh Wi-Fi often carries only link-local IPv6 between access
points) is empirical knowledge that took a defect to learn, and `Endpoints.kt` is pure
functions over strings with a full test file — so it is also the most portable piece of Kotlin
in the repository. It is a candidate for the shared Rust core if iOS uses one
(**ARCH-007**).

Note that the port always comes from the SRV record; no client hardcodes 55432. Confirmed:
`grep -rn "55432" android/app/src/main/java/` returns nothing.

---

## 6. Network change, sleep, VPN, multiple interfaces

| Event | Behaviour needed | Current state |
| --- | --- | --- |
| Wi-Fi → Ethernet (dock) | Re-advertise with new addresses | `enable_addr_auto()` on Linux; **verify per platform** |
| Sleep / resume | Re-register; peers must re-discover | Linux: untested across suspend. **Add to every platform POC** |
| Wi-Fi network switch | Old records must not persist | mDNS TTLs plus the goodbye packet on `Drop` |
| **VPN active** | A full-tunnel VPN can capture multicast or change the default route; a split-tunnel usually leaves the LAN alone | **Untested anywhere.** A common real-world configuration and a likely support issue. **POC.** |
| Multiple interfaces | Advertise on all usable ones; the client must handle per-interface link-local scoping | `IfKind` filtering exists for families, not per-interface. `Endpoints.kt` handles zones |
| Firewall | Blocks 5353 or 55432 | Differs by platform → [07 §7](07-LINUX-PACKAGING.md), [08 §10](08-WINDOWS-FEASIBILITY.md) |
| **Client isolation / AP isolation** | Guest Wi-Fi that blocks peer-to-peer traffic makes AnyFlow silently non-functional | Not detected today. A diagnostic ("found the device but cannot connect") would help. **UX-005** |

The VPN and AP-isolation rows are the two most likely causes of a user reporting "it doesn't
work" with nothing wrong in the logs, and neither is currently detected or explained. Worth a
diagnostic, on every platform.

---

## 7. Security properties that must not drift

From `core/src/discovery.rs` and the threat model, restated because each new platform is an
opportunity to lose them:

1. **Discovery is not trust.** Everything in a record is attacker-controlled. A spoofed record
   leads to a TCP connection that fails the pinned-key check — a non-event. No platform's
   discovery API may be allowed to feed an authorization decision.
2. **The fingerprint is never published.** Deliberate: *"so a passive observer cannot enumerate
   the trust graph."* Any platform API that helpfully offers to publish more must be told not to.
3. **`dn` is untrusted.** `sanitize_device_name` strips control characters and caps at 64 —
   because device names are rendered in notifications and terminals, where a control sequence
   could forge UI. Every platform's UI must sanitize on *display*, not rely on the publisher.
4. **`id` is validated on parse** (`s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit())`).
5. **Publishing a stable `id` and `dn` is an accepted, documented privacy cost** — an observer
   in a café can tell the same laptop came back. The documented mitigation is a per-network
   toggle to suppress advertisement, which **does not exist yet** and becomes more important as
   AnyFlow ships on laptops that travel. **UX-006.**

Point 5 is worth elevating: it is the only place where the expansion makes an existing accepted
risk *worse* (more devices, more networks, more travel) without any code change.

---

## 8. Interoperability matrix to prove

The point of the whole document. Every cell is a real pairing that must work.

| Advertiser ↓ / Browser → | Android (`NsdManager`) | iOS (`NWBrowser`) |
| --- | --- | --- |
| **Linux** (`mdns-sd`) | ✅ shipping | POC-IOS-01 |
| **Windows** (`mdns-sd` or `DnsServiceRegister`) | **POC-WIN-02** | POC |
| **macOS** (`mdns-sd` or `dnssd`) | **POC-MAC-02** | POC |

The two bold cells are the important ones: they prove a **new** desktop is visible to the
**existing shipped** Android app with **no app change**. If either fails, the expansion has
broken backward compatibility, which is the one outcome that is not acceptable.

Proposed harness (**POC-DISC-01**): one machine per desktop platform on one link, one Android
device and one iOS device, and a checklist —
*service appears; TXT keys parse; both address families resolve; the connection completes a
handshake; the record disappears within the TTL after the daemon stops; the record is correct
again after a Wi-Fi change and after suspend/resume.*

`daemon/examples/fake_phone.rs` already exists as a scriptable client and is the natural basis
for automating the desktop-side half.

---

## 9. Recommended direction

1. **Do not change the wire record.** Not the service type, not the TXT keys, not the instance
   naming.
2. **Reuse `mdns-sd` on Windows; expect and budget the platform fallback on macOS; use
   `NWBrowser` on iOS.**
3. **Make 5353 coexistence a release gate per desktop platform.**
4. **Assert `Families` explicitly in the Windows and macOS bring-up POCs**, not just "it
   started".
5. **Port `Endpoints.kt`'s ordering and filtering logic to any new client**, ideally by moving
   it into the shared core.
6. **Add diagnostics for VPN and AP isolation** — the two invisible failure modes.
7. **Implement the advertisement-suppression toggle** the threat model already promises, before
   AnyFlow is on laptops in cafés at scale.
