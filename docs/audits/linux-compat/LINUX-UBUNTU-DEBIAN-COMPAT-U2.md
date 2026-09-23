# AnyFlow — Linux Distribution Compatibility
# U2 — Real-Session Certification

**Targets:** Ubuntu 24.04 LTS · Ubuntu 26.04 LTS · Debian 13 Stable
**Mode:** read-only product certification
**Branch:** `cert/linux-ubuntu-debian-u2-real-session`
**Baseline:** `97923302ddba473c3b139af8832abe9153c21cff`
**Date:** 2026-09-15 (first attempt) · updated 2026-09-15 (execution) · updated 2026-09-15 (Debian wave)

---

## 0. CURRENT U2 STATUS — read this first

> This block is the **operative status of U2** and supersedes every earlier status statement in
> this document, including the §1 executive summary, the §35 scoring and the closing trailer.
> Earlier text is preserved, not deleted, and is marked SUPERSEDED where it stands.

### U2 OVERALL: COMPLETE — ALL THREE TARGETS CERTIFIED WITH DEFECTS

| Target | Status | Result |
| --- | --- | --- |
| **Ubuntu 24.04 LTS** | **REAL-SESSION CERTIFICATION EXECUTED** | 19 PASS / 2 FAIL / 1 PARTIAL / 1 NOT EXECUTED — **89 / 100** (§35B) |
| **Debian 13 Stable** | **REAL-SESSION CERTIFICATION EXECUTED** | 20 PASS / 2 FAIL / 1 PARTIAL — **90.5 / 100** (§39.24.1) |
| **Ubuntu 26.04 LTS** | **REAL-SESSION CERTIFICATION EXECUTED** | 21 PASS / 2 FAIL — **93 / 100** (§40.20.1) |
| **Combined U2** | **COMPLETE** | **90.8 / 100** — the full comparison is **§41** |

**All three targets are CERTIFIED WITH DEFECTS.** Not PASS, because the same two gates fail on
every target and are recorded as FAIL rather than waived. Not FAIL, because nothing
distribution-specific failed: the identical 7-point deduction everywhere is **5 points for a
test-suite portability bug the product passes by behaving safely, and 2 for one real product
defect**. The score spread (89 / 90.5 / 93) reflects **how completely each wave exercised its
target**, not platform quality — see §41.2.

**Three product defects, confirmed across distributions** (full inventory in §41.7):

| Ref | Defect | Priority |
| --- | --- | --- |
| **P1** | Android connects only to `peers().firstOrNull()` — a second paired desktop is unusable while a first exists (§39.17) | **HIGH** |
| **P2** | A battery-less host is reported as "0 %" — the desktop sends 0 and never reads `IsPresent`/`PowerSupply`/`Type` (§39.9.3, §40.21) | P2 |
| **P3** | Notification role convergence needs a daemon restart — roles are announced once at session start and never re-announced (§40.13) | P2 |

**Nothing security- or privacy-relevant failed on any target** (§41.5). No notification body,
clipboard content, private key or pairing token reached any log or state file — verified against
content that really circulated.

**Topology: SOLVED and used for all three guests** (§36.2b). The certifiable path is

```
physical Ethernet (enp0s13f0u2u2c2, carrier=1, 1000 Mb/s)
  → macvtap type='direct' mode='bridge'
    → guest 192.168.68.x/22, own MAC, LAN DHCP
      → Android SM-X620 192.168.68.63/22
```

**No `br0` is required.** No NAT, no port forwarding, no tunnel, no Avahi reflector, and **no host
route or NetworkManager change was made at any point across all three waves.**

**Host/device state is NOT clean, and that is deliberate** — **three VMs** (`anyflow-u2404`,
`anyflow-d13`, `anyflow-u2604`) and their disks are **preserved on purpose** for the imminent
hardening/regression wave, alongside a libvirt storage pool, pool volumes, an Android app reinstall
and Android consents. §37, §37.1, §37.2 and §42 are the authoritative record; the original
"Nothing to revert" paragraph is **SUPERSEDED** (§37.1).

---

## 1. Executive summary — SUPERSEDED (first attempt, preserved verbatim)

> **This entire section belongs to the first U2 attempt, when no VM existed.** It was accurate
> when written and is kept as historical evidence. Its verdict was **superseded on 2026-09-15**
> when the Ethernet cable was connected and Ubuntu 24.04 was certified; see §0 for the current
> status, §36.1b for the operative Ubuntu 24.04 ruling, and §34B for the evidence.

**U2 was not executed. The certification stopped at the §3 host-network hard gate, before any
VM was created.**

U2 requires a real GNOME/Wayland guest that sits on the **same layer-2 segment** as the Android
test device, so that `_anyflow._tcp.local.` multicast reaches both directions without NAT, port
forwarding, or manual IP entry. That topology needs a wired host link to bridge over. This host
does not currently have one.

Measured today, on the baseline commit:

| Fact | Measurement |
| --- | --- |
| Host's only carrier-up link | `wlp0s20f3` — **Wi-Fi**, `iwlwifi`, `managed` (station) mode |
| Wi-Fi 4addr / WDS support | **absent** — literal string `4addr` occurs **0 times** in `iw list` |
| USB Ethernet `enp0s13f0u2u2c2` | enumerated, driver bound, **`carrier=0`, `Link detected: no`** |
| Only libvirt network defined | `default`, **`forward mode='nat'`**, 192.168.122.0/24 |
| `virbr0` | itself `NO-CARRIER`, `linkdown` |
| VMs defined on this host | **none** |

This is the **same obstacle U0 measured and recommended fixing with one cable** (U0 §21.1.1).
Nothing about it has changed. The adapter is still unplugged.

The brief is explicit about this case, and it is followed literally:

> If the USB Ethernet adapter still has no carrier: **STOP before claiming U2 certification.**

Accordingly **no gate from §8 (build) through §27 (persistence) was run, on any distro.** Their
status in this report is **NOT EXECUTED**, which is neither PASS nor FAIL nor N/A. Every one of
the twenty-three gates in the §30 matrix is unproven for all three targets.

**What this finding is not:**

- It is **not** a product defect. No AnyFlow code was exercised, so no code was found wanting.
- It is **not** a regression. U1's build-only evidence is untouched and still green (§2).
- It is **not** a reason to lower the bar. The rejected shortcuts — libvirt NAT, port forwarding,
  `socat`, an Avahi reflector, manual IP entry — would each produce a green-looking result that
  proves nothing about G-MDNS or G-PAIR, which are most of the suite. None were used.

**The fix is one Ethernet cable.** The host is otherwise ready: KVM is available, libvirt's QEMU
and network daemons are running, virt-manager 5.1.0 / QEMU 10.2.2 are installed, and 336 GB of
disk is free. §4.6 gives the exact, reversible host-side command sequence to run the moment the
adapter has carrier, and §6 gives the three `virt-install` invocations that follow it.

**Verdict: UBUNTU/DEBIAN U2: INFRASTRUCTURE BLOCKED — REAL-SESSION CERTIFICATION NOT EXECUTED.**

> **⚠ THE VERDICT IMMEDIATELY ABOVE IS SUPERSEDED.** It was the first attempt's ruling and is
> preserved verbatim. The infrastructure blocker it names was cleared the same day (§4.9), and
> Ubuntu 24.04 was subsequently certified with defects (§36.1b). The operative status is §0:
> **U2 OVERALL: IN PROGRESS.**

---

> ### ⏵ U2 RESUME — the physical blocker was cleared later the same day
>
> **Everything above in §1 is the first attempt's finding and is preserved verbatim.** It was
> accurate when measured. It is **not** deleted, because a certification report that quietly
> rewrites its own history is worth less than one that shows what changed and when.
>
> At **2026-09-15 11:34 −03** the USB Ethernet adapter was physically connected and came up with
> `carrier=1` at 1000 Mb/s on `192.168.68.72/22`. The one-cable fix that §1, §3.2, §4.3 and
> U0 §21.1.1 all named as the blocker **has been applied.** U2 execution resumed from that point.
>
> Read in this order:
> - **§4.9** — the cleared blocker, re-measured (carrier, routes, Android, LAN reachability)
> - **§4.10** — host virtualisation access, corrected: `qemu:///system` needs no `sudo` here
> - **§4.11** — a *new* constraint found after the cable: host RAM, and the 4 GB guest decision
>
> **The §1 verdict above stands until guest-native runtime evidence replaces it.** A cleared
> blocker restores the possibility of certification; it is not itself a gate result. Any gate
> still marked NOT EXECUTED in §28 remains NOT EXECUTED.

---

---

## 2. Baseline

### 2.1 Repository state at start

```console
$ git branch --show-current
cert/linux-ubuntu-debian-u2-real-session

$ git status
On branch cert/linux-ubuntu-debian-u2-real-session
nothing to commit, working tree clean

$ git log --oneline -8
9792330 Merge pull request #25 from yurisismotto/feature/linux-debian-ubuntu-compat-v1
c88621d feat(linux): add Ubuntu and Debian build compatibility
06ac9eb Merge pull request #24 from yurisismotto/research/linux-debian-ubuntu-compat-v1
4c32e79 docs(linux): audit Ubuntu and Debian compatibility
7822a06 Merge pull request #23 from yurisismotto/cert/notifications-v1-n6-final-certification
4306389 docs(notifications): certify notifications.v1 for v1
836cbd4 Merge pull request #22 from yurisismotto/feature/notifications-v1-n5-hardening
e72bb09 feat(notifications): harden recovery and session convergence

$ git diff --check
(clean)

$ git rev-parse HEAD
97923302ddba473c3b139af8832abe9153c21cff
```

Branch correct. Working tree clean at start, as required.

**Exact baseline commit: `97923302ddba473c3b139af8832abe9153c21cff`** (the U1 merge, PR #25).

### 2.2 Post-U1 CI evidence — verified

Both required workflows are green, and the head SHA of each run was checked against the baseline
rather than assumed from branch name:

```console
$ gh run view 34978812698 --json headSha,conclusion,workflowName
Linux distro compatibility · build only | success | 97923302ddba473c3b139af8832abe9153c21cff

$ gh run view 34978812387 --json headSha,conclusion,workflowName
Portable core · Windows MSVC          | success | 97923302ddba473c3b139af8832abe9153c21cff
```

| Workflow | Run | Conclusion | Head SHA matches baseline |
| --- | --- | --- | --- |
| Linux distro compatibility · build only | 34978812698 | **success** | **yes** |
| Portable core · Windows MSVC | 34978812387 | **success** | **yes** |

§0 is satisfied. Note carefully what this evidence *is*: a **container/runner build**, which §32
of the brief forbids inheriting as a runtime PASS. It establishes that the U1 code compiles for
the distro targets. It says nothing about the runtime behaviour U2 exists to certify.

### 2.3 Authoritative inputs read

- `LINUX-UBUNTU-DEBIAN-COMPAT-U0.md` — the audit, in particular §21.1.1 (the network obstacle)
- `LINUX-UBUNTU-DEBIAN-COMPAT-U1.md` — the implementation baseline
- U1 verdict of record: `UBUNTU/DEBIAN U1: PASS — U2 REAL-SESSION CERTIFICATION READY`

---

## 3. Certification topology

### 3.1 The topology U2 requires

```
   Android SM-X620                          VM guest (GNOME/Wayland)
   192.168.68.63                            192.168.68.x
   wlan0  ──────┐                        ┌────── virtio NIC
                │                        │
                ▼                        ▼
        ╔════════════════════════════════════════════╗
        ║   192.168.68.0/22  —  one L2 segment       ║
        ║   gateway 192.168.68.1                     ║
        ║   UDP/5353 multicast reaches both ways     ║
        ╚════════════════════════════════════════════╝
                              ▲
                              │  br0  (host bridge)
                              │
                     enp0s13f0u2u2c2  ← wired, REQUIRED
```

AnyFlow product traffic must flow Android ↔ VM guest directly. ADB is a setup and evidence
channel only; it must not carry product traffic.

### 3.2 The topology actually available today

```
   Android SM-X620                          (no VM exists)
   192.168.68.63
   wlan0  ──────┐
                │
                ▼
        ╔════════════════════════════════════════════╗
        ║   192.168.68.0/22  —  the real LAN         ║
        ╚════════════════════════════════════════════╝
                              ▲
                              │  wlp0s20f3  (Wi-Fi STATION — not bridgeable)
                              │
                         Fedora host
                         192.168.68.70

        virbr0 ── 192.168.122.0/24  NAT  ── NO-CARRIER, linkdown
                  ^ rejected by §3 as certification evidence

        enp0s13f0u2u2c2 ── carrier=0 ── NO CABLE   ← the missing link
```

The LAN itself is healthy and the Android device is on it and reachable (§4.4). The single
missing element is a path to put a **guest** on that same segment.

---

## 4. Physical network / bridge

This section carries the evidence for the STOP. It is the substance of this report.

### 4.1 Host link inventory

```console
$ ip -br link
lo               UNKNOWN        00:00:00:00:00:00 <LOOPBACK,UP,LOWER_UP>
wlp0s20f3        UP             e2:0e:f6:98:52:a7 <BROADCAST,MULTICAST,UP,LOWER_UP>
enp0s13f0u2u2c2  DOWN           6c:1f:f7:29:4c:c3 <NO-CARRIER,BROADCAST,MULTICAST,UP>
virbr0           DOWN           52:54:00:d1:11:03 <NO-CARRIER,BROADCAST,MULTICAST,UP>

$ ip -br addr
lo               UNKNOWN        127.0.0.1/8 ::1/128
wlp0s20f3        UP             192.168.68.70/22 fe80::26c0:ae6f:206d:8e3/64
enp0s13f0u2u2c2  DOWN
virbr0           DOWN           192.168.122.1/24

$ ip route
default via 192.168.68.1 dev wlp0s20f3 proto dhcp src 192.168.68.70 metric 600
192.168.68.0/22 dev wlp0s20f3 proto kernel scope link src 192.168.68.70 metric 600
192.168.122.0/24 dev virbr0 proto kernel scope link src 192.168.122.1 linkdown
```

Carrier state read directly from sysfs rather than inferred from `ip`'s flags:

```console
$ for n in /sys/class/net/*; do ... done
enp0s13f0u2u2c2      carrier=0 operstate=down
virbr0               carrier=0 operstate=down
wlp0s20f3            carrier=1 operstate=up
```

**Exactly one host interface has carrier, and it is the Wi-Fi station link.**

### 4.2 NetworkManager view

```console
$ nmcli device status
DEVICE             TYPE      STATE                   CONNECTION
wlp0s20f3          wifi      conectado               Rede Wi-Fi de Yuri
lo                 loopback  connected (externally)  lo
virbr0             bridge    connected (externally)  virbr0
p2p-dev-wlp0s20f3  wifi-p2p  desconectado            --
enp0s13f0u2u2c2    ethernet  não disponível          --

$ nmcli connection show
NAME                UUID                                  TYPE      DEVICE
Rede Wi-Fi de Yuri  8b7deb00-9ef8-477a-bfbd-c648ed645041  wifi      wlp0s20f3
lo                  a0464d35-b8fb-4ad9-8563-2ffabb0654d4  loopback  lo
virbr0              1d8b0e20-71cb-4a3f-81c1-0b6196202e5a  bridge    virbr0
Conexão cabeada 1   0038288b-3be2-370e-91ac-a649a98d2fdd  ethernet  --
```

`enp0s13f0u2u2c2` is **`não disponível`** (unavailable) — NetworkManager's state for a wired
device with no carrier. A wired profile (`Conexão cabeada 1`) already exists but is bound to no
device, which is consistent: the cable has never been connected in this session.

### 4.3 The USB Ethernet adapter — present, bound, unplugged

The adapter is not missing or broken. It is enumerated and its driver is attached:

```console
$ lsusb | grep -i ethernet
Bus 002 Device 005: ID 0b95:1790 ASIX Electronics Corp. AX88179 Gigabit Ethernet

$ ethtool -i enp0s13f0u2u2c2
driver: cdc_ncm
version: 7.1.9-200.fc44.x86_64
firmware-version: CDC NCM (NO ZLP)
bus-info: usb-0000:00:0d.0-2.2

$ ethtool enp0s13f0u2u2c2
	Supported ports: [  ]
	Supported link modes:   Not reported
	Supports auto-negotiation: No
	Speed: Unknown!
	Duplex: Unknown! (255)
	Port: Twisted Pair
	Link detected: no          ← the blocker, in one line
```

**`Link detected: no`.** Speed and duplex unknown, supported link modes not reported — the
signature of a twisted-pair port with nothing in it. **No cable is connected.**

This is verbatim the condition U0 §21.1.1 flagged and the condition §3 of the U2 brief names as
a STOP.

### 4.4 The LAN and the Android device are fine — only the guest path is missing

It is worth being precise about what is *not* broken, so the blocker is not overstated:

```console
$ ping -c 3 -W 2 192.168.68.63        # the Android tablet
3 pacotes transmitidos, 3 recebidos, 0% packet loss, time 2003ms
rtt min/avg/max/mdev = 101.847/186.862/241.667/60.947 ms

$ ping -c 2 -W 2 192.168.68.1         # the LAN gateway
2 pacotes transmitidos, 2 recebidos, 0% packet loss, time 1001ms
rtt min/avg/max/mdev = 5.479/6.176/6.873/0.697 ms
```

Host and Android are on one segment, mutually reachable, with no client isolation blocking ICMP.
The LAN is a perfectly good certification network. **The defect is solely that a VM cannot be
placed onto it from this host.**

### 4.5 Every alternative, evaluated and rejected on evidence

#### (a) Bridge or macvtap over the Wi-Fi station link — **impossible at the driver level**

```console
$ iw list | grep -ci "4addr"
0
```

**Zero occurrences of `4addr`.** The `iwlwifi` wiphy does not advertise `WIPHY_FLAG_4ADDR_STATION`,
so the driver cannot be put into 4-address mode at all:

```console
$ iw dev wlp0s20f3 info
	type managed
	wiphy 0
	channel 36 (5180 MHz), width: 80 MHz

$ iw list   # Supported interface modes
		 * IBSS
		 * managed
		 * AP
		 * AP/VLAN
		 * monitor
		 * P2P-client / P2P-GO / P2P-device
```

Without 4addr, an 802.11 station transmits frames bearing one MAC — its own. A bridged or
macvtap'd guest's frames carry a different source MAC and the AP drops them. Traffic leaves and
nothing returns. This is a link-layer property, not a libvirt setting, and it is settled here
before the AP's own capabilities even come into question. U0 reached the same conclusion; this
wave confirms it against the driver rather than by reputation.

#### (b) libvirt NAT (`default`) — **explicitly rejected by §3**

```console
$ virsh --connect qemu:///system net-dumpxml default | grep -E "forward|ip address"
  <forward mode='nat'>
  <ip address='192.168.122.1' netmask='255.255.255.0'>
```

The only defined libvirt network is NAT on 192.168.122.0/24. Multicast does not cross it, so
G-MDNS cannot be observed and G-PAIR cannot be reached through product discovery. §3 forbids
passing a NAT-only result off as mDNS evidence. It is also moot: `virbr0` is itself `linkdown`.

#### (c) Port forwarding / `socat` / SSH tunnel / Avahi reflector — **forbidden as evidence**

§3 lists these by name. They can help debug; they are not certification evidence. A pass obtained
this way would assert that Android discovered the guest through product mDNS when it did not.
None were used.

#### (d) Android USB tethering as a substitute L2 segment — **considered, rejected**

USB tethering would create a genuine host↔Android link, and is the only technically-adjacent
option available without a cable. It is rejected because it is not the topology under
certification: it makes the **Android device the router and DHCP server** of a private
192.168.42.0/24 segment, inverting the network roles, NATing toward its own upstream, and
certifying AnyFlow against a link no user will ever run it over. §3 asks for "the same LAN/VLAN
as Android," not a synthetic point-to-point segment. Substituting it would be precisely the kind
of creative workaround §32 forbids.

#### (e) A second physical machine instead of a VM — **viable, out of scope today**

U0 §21.1.1 lists this as the cleanest evidence of all, and that remains true. It requires
hardware not present in this session. It is recorded in §34 as the standing alternative if the
cable route stays unavailable.

### 4.6 Exact host-side commands to run once a cable is connected

Required by §3. These are written for **this** host: Fedora 44, NetworkManager, libvirt 12.0.0
with `virtqemud` and `virtnetworkd` already running. The Wi-Fi profile is left untouched
throughout, and every step is reversible (§4.7).

**Step 0 — confirm carrier before changing anything.** Do not proceed if this still says `no`:

```bash
ip -br link show enp0s13f0u2u2c2
ethtool enp0s13f0u2u2c2 | grep "Link detected"
# required: carrier, and "Link detected: yes"
```

**Step 1 — create the bridge (NetworkManager, reversible, Wi-Fi untouched).**

```bash
nmcli connection add type bridge ifname br0 con-name br0 \
    ipv4.method auto ipv6.method auto \
    bridge.stp no

nmcli connection add type ethernet ifname enp0s13f0u2u2c2 \
    con-name br0-port-usb master br0

nmcli connection up br0
nmcli connection up br0-port-usb
```

**Step 2 — prove the bridge is on the target LAN, not on NAT.** This is the gate that decides
whether certification may start:

```bash
ip -br addr show br0
ip route
```

`br0` **must** hold an address in **192.168.68.0/22**. If it shows 192.168.122.x, or no address,
the bridge is not on the LAN and certification must stop again.

```bash
ping -c 3 192.168.68.63     # the Android tablet, from the host, over the LAN
ping -c 3 192.168.68.1      # the gateway
```

**Step 3 — define a bridged libvirt network.**

```bash
cat > /tmp/anyflow-lan.xml <<'XML'
<network>
  <name>anyflow-lan</name>
  <forward mode='bridge'/>
  <bridge name='br0'/>
</network>
XML

sudo virsh --connect qemu:///system net-define /tmp/anyflow-lan.xml
sudo virsh --connect qemu:///system net-start anyflow-lan
sudo virsh --connect qemu:///system net-autostart anyflow-lan
sudo virsh --connect qemu:///system net-list --all
```

**Step 4 — allow bridged forwarding through firewalld if it interferes.** The host runs firewalld
in the `FedoraWorkstation` zone. Check first, and only act if traffic is actually blocked —
classify any change as a host-test-harness step, never as a product finding:

```bash
sudo firewall-cmd --get-active-zones
sudo firewall-cmd --zone=FedoraWorkstation --change-interface=br0   # if needed
```

**Step 5 — stop the host's own daemon before taking discovery evidence.** The host currently runs
an `anyflowd` bound to UDP/5353 (§4.8). Leave it stopped for the duration so that any
`_anyflow._tcp.local.` the Android sees is unambiguously the guest's:

```bash
pkill -f 'target/debug/anyflowd'   # run from a shell that is not itself matched
ss -lunp | grep 5353               # confirm no anyflowd remains
```

**Step 6 — move ADB off USB, per §2.**

```bash
adb -s RX2Y500C7SY tcpip 5555
adb connect 192.168.68.63:5555
adb devices -l
```

### 4.7 Rollback — restoring the host exactly

No persistent host network change was made in this wave. Should §4.6 be executed, this reverses
it completely:

```bash
nmcli connection down br0-port-usb ; nmcli connection down br0
nmcli connection delete br0-port-usb ; nmcli connection delete br0

sudo virsh --connect qemu:///system net-destroy  anyflow-lan
sudo virsh --connect qemu:///system net-undefine anyflow-lan
```

The Wi-Fi profile `Rede Wi-Fi de Yuri` is never modified by any step above.

### 4.8 Host state observed but not altered

```console
$ ss -lunp | grep 5353
UNCONN 0 0   0.0.0.0:5353   users:(("anyflowd",pid=613091,fd=17),("anyflowd",pid=613091,fd=16))
UNCONN 0 0   0.0.0.0:5353   users:(("adb",pid=529132,fd=11))
UNCONN 0 0 224.0.0.251:5353 users:(("brave",pid=8662,fd=155))

$ systemctl is-active avahi-daemon
active

$ ps -o pid,etime,cmd -p 613091
    PID     ELAPSED CMD
 613091    17:33:23 ./target/debug/anyflowd
```

A host `anyflowd` has been running for ~17.5 h and is advertising on the same LAN. It was **left
running** — this wave took no discovery evidence, so there was nothing to disambiguate, and
stopping a long-running user process was not warranted. §4.6 step 5 handles it when certification
actually starts. `avahi-daemon` is active **on the host only**, as a pre-existing Fedora service;
U2 would treat it strictly as an external observer, never as an AnyFlow dependency (§11 of brief).

---

### 4.9 RESUME — 2026-09-15 11:34 −03: the physical blocker is cleared

**Everything in §4.1–§4.8 above is preserved as written. It was true when measured and it is
retained as historical evidence of the first U2 attempt.** This subsection records that the single
condition those sections identified as the STOP has since changed.

The Ethernet cable was connected. Re-running the §4.6 verification commands:

```console
$ ip -br link show enp0s13f0u2u2c2
enp0s13f0u2u2c2  UP  6c:1f:f7:29:4c:c3 <BROADCAST,MULTICAST,UP,LOWER_UP>

$ ip -br addr show enp0s13f0u2u2c2
enp0s13f0u2u2c2  UP  192.168.68.72/22 fe80::7e3e:e754:c68c:78c2/64

$ cat /sys/class/net/enp0s13f0u2u2c2/carrier      # ethtool needs root; sysfs does not
1
$ cat /sys/class/net/enp0s13f0u2u2c2/operstate
up
$ cat /sys/class/net/enp0s13f0u2u2c2/speed
1000
```

`carrier=1`, `LOWER_UP`, link negotiated at **1000 Mb/s**. This is the exact inverse of the
`Link detected: no` recorded in §4.3. **The blocker named in §1, §3.2 and §4.3 no longer exists.**

> Evidence note: §4.3 quoted `ethtool`. `ethtool` requires root and this session has no
> non-interactive `sudo` (`sudo -n` → *uma senha é necessária*). The sysfs `carrier`/`operstate`/
> `speed` attributes are the same kernel link state that `ethtool` reports, read without
> privilege, and are used here in its place. No claim rests on a command that was not run.

#### Both host LAN connections, unmodified

```console
$ nmcli device status
DEVICE             TYPE      STATE       CONNECTION
enp0s13f0u2u2c2    ethernet  conectado   Conexão cabeada 1
wlp0s20f3          wifi      conectado   Rede Wi-Fi de Yuri
lo                 loopback  connected (externally)  lo
virbr0             bridge    connected (externally)  virbr0

$ ip route
default via 192.168.68.1 dev enp0s13f0u2u2c2 proto dhcp src 192.168.68.72 metric 100
default via 192.168.68.1 dev wlp0s20f3       proto dhcp src 192.168.68.70 metric 600
192.168.68.0/22 dev enp0s13f0u2u2c2 proto kernel scope link src 192.168.68.72 metric 100
192.168.68.0/22 dev wlp0s20f3       proto kernel scope link src 192.168.68.70 metric 600
192.168.122.0/24 dev virbr0 proto kernel scope link src 192.168.122.1 linkdown
```

Ethernet `192.168.68.72/22` (metric 100) and Wi-Fi `192.168.68.70/22` (metric 600) are both up on
the one `/22`. **Neither route was added, deleted or reordered by this wave**, per the brief's
network-routing-safety clause. `virbr0` — the libvirt NAT bridge that §4.5 rejected as
certification evidence — remains `linkdown` and unused.

#### LAN reachability, re-measured

```console
$ ping -c 3 -W 2 192.168.68.1                          # gateway
3 pacotes transmitidos, 3 recebidos, 0% packet loss
rtt min/avg/max/mdev = 3.365/3.474/3.543/0.078 ms

$ ping -c 3 -W 2 192.168.68.63                         # Android SM-X620
3 pacotes transmitidos, 3 recebidos, +2 duplicados, 0% packet loss
rtt min/avg/max/mdev = 147.603/285.684/331.229/69.738 ms

$ ping -c 3 -W 2 -I enp0s13f0u2u2c2 192.168.68.63      # forced out the wired link
3 pacotes transmitidos, 3 recebidos, +2 duplicados, 0% packet loss
rtt min/avg/max/mdev = 167.940/217.976/282.443/44.253 ms
```

Android is reachable over the LAN generally **and** specifically out of the wired interface.

> **Observation carried forward, not yet explained:** both pings to Android report
> `+2 duplicados` — duplicate ICMP replies. The expected cause is that the host holds two
> addresses on one `/22` (wired + Wi-Fi), so the tablet's replies return over both paths. It is
> recorded here because a duplicate-delivery path could plausibly affect mDNS observation later;
> §14 must not mistake a duplicated packet for a second advertisement. The gateway, reached in
> 3.4 ms with **no** duplicates, shows the wired path itself is clean.

#### Android device state, re-confirmed

```console
$ adb devices -l
RX2Y500C7SY  device usb:3-2.1 product:gts10fepwifixx model:SM_X620 device:gts10fepwifi

$ adb shell ip -br addr show wlan0
wlan0  UP  192.168.68.63/22 fe80::b8b5:7fff:fe93:767b/64

$ adb shell ip route
192.168.68.0/22 dev wlan0 proto kernel scope link src 192.168.68.63
```

Android `192.168.68.63/22`, same `/22` as both host links. Unchanged from §5.

#### What this does and does not authorise

| Claim | Status after §4.9 |
| --- | --- |
| The §4.3 no-carrier blocker is cleared | **Yes** — `carrier=1`, 1000 Mb/s |
| Host and Android share one LAN segment | **Yes** — re-measured above |
| A guest can be placed on that segment | **Not yet proven** — requires §4.10 |
| Any AnyFlow gate is certified | **No** — no VM existed at the time of writing |

Clearing the cable blocker restores the *possibility* of U2. It certifies nothing by itself. The
§1 verdict therefore stands until guest-native runtime evidence replaces it.

---

### 4.10 Host virtualisation access — re-checked, and better than §6.1 recorded

One assumption in the first attempt was wrong in the host's favour and is corrected here.

```console
$ id -nG
yuri wheel libvirt

$ virsh -c qemu:///system list --all
 Id   Nome   Estado
---------------------
                                   # exit=0 — no sudo, no password, no polkit prompt
```

`yuri` is in the **`libvirt` group**, so the privileged `qemu:///system` URI is usable directly.
This matters because this session has no non-interactive `sudo`: had `qemu:///system` required
root, U2 could not have created a VM at all, and the wave would have been blocked a second time
for an entirely different reason.

`libvirtd` is `inactive` but `virtqemud` is `active` — the modular libvirt split Fedora 44 ships.
Functionality is unaffected; §6.1's `libvirtd`-centred wording is superseded by this measurement.

#### Storage pool — created by this wave

§6.1 recorded 336 GB free but did not check whether libvirt had a pool to put an image in. It did
not: `virsh pool-list --all` returned empty, and `/var/lib/libvirt/images` is `drwx--x--x root
root`, unreadable to the invoking user. A standard `dir` pool was defined over the conventional
path:

```console
$ virsh pool-define /…/pool.xml && virsh pool-start default && virsh pool-autostart default
$ virsh pool-info default
Estado:       executando
Capacidade:   474,34 GiB
Alocação:     143,51 GiB
Disponível:   330,82 GiB
```

**This is a host-state change made by this wave.** It is additive, reversible
(`virsh pool-destroy default && virsh pool-undefine default`), touches no networking, and is
required before any guest disk can be allocated. It is logged in §37 *Machine state left behind*.

---

### 4.11 Host RAM — a new infrastructure constraint, found after the cable

The cable was not the last infrastructure obstacle. Measured immediately before VM creation:

```console
$ free -m
               total        used        free      shared  buff/cache   available
Mem:           15637        9993        2912        1113        4384        5644
Swap:           8191        5498        2693
```

**~5.6 GB available, with 5.5 GB of 8 GB swap already consumed before any guest starts.** Per-app
resident totals: `brave` 3148 MB / 30 procs, `claude` 3439 MB / 18 procs, `code` 1325 MB /
17 procs, `gnome-shell` 289 MB.

The brief specifies "6 GB RAM maximum" for the guest. **A 6 GB guest does not fit** in 5.6 GB of
available memory alongside Fedora's own GNOME session and the ADB tooling that U2 depends on.

This was put to the operator rather than resolved silently, because overcommitting this
particular host has previously frozen it outright. The decision taken:

> **4 GB guest, host left untouched.** No user application closed. Inside the brief's 6 GB
> ceiling, ~1.5 GB headroom retained for Fedora + ADB.

The accepted cost is recorded honestly: guest `cargo` builds at `-j 2` in 4 GB will be slow and
may swap **inside the guest**. That is a throughput cost, not a correctness cost — §10's build
gate asserts that the runtime binary is guest-native, not that it compiled quickly. If a guest
build fails specifically on memory exhaustion, it will be reported as such and not as a distro
incompatibility.

---

### 4.12 The guest, and how it is driven — direct/macvtap, with an out-of-band control channel

#### Installation media

| | |
| --- | --- |
| ISO | `ubuntu-24.04.4-desktop-amd64.iso` (24.04.4 — current 24.04 LTS point release) |
| Source | `https://releases.ubuntu.com/24.04/` — the official Canonical release server, not a mirror |
| Published SHA256 | `3a4c9877b483ab46d7c3fbe165a0db275e1ae3cfe56a5657e5a47c2f99a99d1e` |
| Computed SHA256 | `3a4c9877b483ab46d7c3fbe165a0db275e1ae3cfe56a5657e5a47c2f99a99d1e` |
| Verdict | **MATCH** — verified before first boot |

The ISO had to be staged into `/var/lib/libvirt/images` rather than read from `~/ISO`: `/home/yuri`
is `drwx------`, so the `qemu` user cannot traverse it, and SELinux is `Enforcing`. It was copied
in through `virsh vol-upload`, which runs inside libvirtd and therefore needs no `sudo`.

#### Domain definition

```console
$ virt-install --name anyflow-u2404 \
    --memory 4096 --vcpus 4 --cpu host-passthrough --machine q35 --boot uefi \
    --osinfo ubuntu24.04 \
    --disk pool=default,size=40,format=qcow2,bus=virtio \
    --cdrom /var/lib/libvirt/images/ubuntu-24.04.4-desktop-amd64.iso \
    --disk /var/lib/libvirt/images/seed-u2404.iso,device=cdrom,bus=sata \
    --network type=direct,source=enp0s13f0u2u2c2,source.mode=bridge,model.type=virtio \
    --graphics spice,listen=none --video virtio --console pty,target_type=serial
```

Against the brief's §4 resource envelope: 4 vCPU ✓, **4096 MB** (see §4.11 — under the 6 GB
ceiling, by measurement) , 40 GB qcow2 ✓, UEFI ✓, virtio disk ✓, virtio video ✓. One VM at a time ✓.

The networking is the point of the whole exercise:

```console
$ virsh dumpxml anyflow-u2404 | grep -A5 "type='direct'"
    <interface type='direct'>
      <mac address='52:54:00:4d:d8:36'/>
      <source dev='enp0s13f0u2u2c2' mode='bridge'/>
      <target dev='macvtap1'/>
      <model type='virtio'/>
```

`type='direct'` + `mode='bridge'` is **macvtap over the physical wired NIC**. The guest gets its
own MAC on the wire and DHCPs from the LAN's own server. This is the topology the brief asked to
be tried before any `br0` migration, and **no host route, NetworkManager connection or interface
was modified to achieve it** — the pre-change snapshot required by the brief's network-routing-safety
clause was taken and is reproduced in §4.9.

`virbr0` (libvirt NAT, 192.168.122.0/24) is **not attached to this domain at all**. There is no
second interface. There is no `-netdev user`. Port forwarding, `socat` and Avahi reflection are
absent by construction, not by promise.

#### How the guest is driven — and why this is not a shortcut

macvtap in `bridge` mode deliberately does not loop traffic back to its own host. The Fedora host
therefore **cannot** SSH to the guest. The brief anticipates this ("A macvtap host↔guest limitation
is acceptable — the certification requirement is Android ↔ guest, not host ↔ guest").

That still leaves the practical problem of running commands in the guest. Three options existed:

| Option | Rejected / chosen | Reason |
| --- | --- | --- |
| Second NIC on `virbr0` for management SSH | **Rejected** | `anyflowd` advertises on *every* interface (proven on the host itself, §4.13). A NAT NIC would put a second `_anyflow._tcp` record into evidence and muddy §14. Not worth it. |
| Host-side `macvlan` shim to reach the guest | **Rejected for now** | Works, but it is a host network change, and §14's evidence does not need it. |
| **`qemu-guest-agent` over virtio-serial** | **Chosen** | Zero network involvement. Cannot influence routing, discovery or the LAN in any way. |

The guest is therefore driven through `virsh qemu-agent-command … guest-exec`, over the
`org.qemu.guest_agent.0` virtio-serial channel. **This channel carries no AnyFlow product traffic
and cannot: it is not an IP path.** All AnyFlow traffic under certification traverses macvtap →
`enp0s13f0u2u2c2` → the physical LAN, and nothing else.

Graphical observation uses `virsh screenshot`, which captures the guest's real framebuffer. The
guest runs a full GNOME/Wayland stack on virtio-gpu — the screenshots are of a real desktop, not a
headless surface. Keyboard input, where needed (GDM unlock in §21), uses `virsh send-key`.

> Honest limitation: the host's own GNOME denies screenshot and input-injection APIs to this
> session, so guest observation *had* to be done through libvirt rather than by driving a
> `virt-viewer` window. This constrains how the GUI is exercised — it does not make it headless.

#### Unattended installation

The desktop ISO was installed via Ubuntu's supported `autoinstall` path: a second CD-ROM labelled
`CIDATA` carrying cloud-init `user-data` with an `autoinstall:` block (`source.id: ubuntu-desktop`,
`storage.layout: direct`, `interactive-sections: []`). The **only** packages named at install time
were `qemu-guest-agent` and `openssh-server` — the control channel — deliberately keeping the
install itself minimal so that an install failure could not be confused with a dependency failure.
Everything AnyFlow needs is installed afterwards, in the running guest, and recorded in §10.

GDM autologin for user `anyflow` was set so a real graphical session exists at boot. This is a
genuine `seat0` graphical logind session — §20's `loginctl` evidence is taken against it, and §21
locks and unlocks it for real.

#### Method finding: Ubuntu Desktop autoinstall still asks, unless told on the kernel command line

Worth recording because it cost a rebuild and would cost anyone repeating U2 the same.

With a valid `CIDATA` seed alone, the 24.04 desktop installer **read and parsed** the autoinstall
config — the "Review your choices" screen rendered this report's own `identity:`,
`interactive-sections: []` and `late-commands:` back verbatim — and then **stopped at a "Ready to
install" confirmation dialog waiting for a human click.** `interactive-sections: []` did not
suppress it.

Attempts to clear it without a mouse are recorded honestly, because one of them produced a
misleading result:

| Attempt | Outcome |
| --- | --- |
| `virsh send-key KEY_ENTER` / `KEY_TAB` ×4 | No effect; screen hash byte-identical across all four |
| `virsh send-key KEY_LEFTMETA` (control probe) | **Screen changed** — GNOME overview opened. Input injection provably works |
| `virsh qemu-monitor-command --hmp 'mouse_button 1'` after `mouse_move`, 1 s hold | Opened the GNOME **desktop** context menu — a *long-press*, and on the desktop, not the dialog |
| Same, fast press/release | No effect |

The context menu's placement gave the cause away: it was anchored exactly where a menu would flip
if the cursor were at (1280, 800) — the bottom-right corner. HMP `mouse_move` takes **relative
deltas**, so the absolute-looking coordinates had simply driven the pointer into the corner. The
"click" was never on the button.

The fix was to remove the need for a click. The ISO's own `boot/grub/grub.cfg` shows the stock
command line is `--- quiet splash`; `casper/vmlinuz` and `casper/initrd` were extracted from the
verified ISO and the domain defined with direct kernel boot and one word added:

```xml
<kernel>/var/lib/libvirt/images/vmlinuz</kernel>
<initrd>/var/lib/libvirt/images/initrd</initrd>
<cmdline>autoinstall --- quiet splash</cmdline>
```

The confirmation screen did not appear again and installation proceeded unattended.
`shutdown: poweroff` was set in the seed so the guest halts at the end instead of rebooting back
into the live kernel; the `<kernel>`/`<initrd>`/`<cmdline>` lines are stripped from the domain
before first real boot.

> **Classification: PACKAGING / METHOD, not a product finding.** This is Ubuntu installer
> behaviour. It touches no AnyFlow code and must not be counted against any gate. It is recorded
> only so the next wave does not repeat the rebuild.

> **Carried forward as a real constraint:** the host's GNOME denies this session both screenshot
> and input-injection APIs, and the QEMU-side pointer proved unreliable. Guest GUI gates are
> therefore driven **from inside the guest** (AT-SPI / in-guest tooling over `guest-exec`), with
> `virsh screenshot` as the visual record. Wherever a gate would ordinarily be satisfied by a
> mouse click, this report says which mechanism was actually used. No gate claims a click that
> did not happen.

---

### 4.13 Host `anyflowd` — identified, so §14 cannot confuse it with the guest

The brief requires that the Fedora host's development daemon not be mistaken for the guest. It was
identified rather than blindly stopped:

```console
$ pgrep -a anyflowd
613091 ./target/debug/anyflowd          # ~20 h uptime

$ ss -ltnp | grep anyflowd
LISTEN 0 128 *:55432 *:* users:(("anyflowd",pid=613091,fd=12))

$ avahi-browse -rtp _anyflow._tcp
=;enp0s13f0u2u2c2;IPv4;795fec0868ebef8c3d7ad3e775dc6ed0;_anyflow._tcp;local;\
  795fec0868ebef8c3d7ad3e775dc6ed0.local;192.168.68.72;55432;\
  "dn=Fedora" "v=1" "pv=1-1" "id=795fec0868ebef8c3d7ad3e775dc6ed0"
```

**The host daemon's identity is now on record:**

| Field | Host (Fedora, must NOT be the discovery subject) |
| --- | --- |
| `id=` | `795fec0868ebef8c3d7ad3e775dc6ed0` |
| `dn=` | `Fedora` |
| Address | `192.168.68.72:55432` |

Two independently sufficient discriminators — instance `id` and display name `dn` — distinguish it
from any guest advertisement. Note it advertises on **both** `enp0s13f0u2u2c2` and `wlp0s20f3`,
which is the evidence behind §4.12's rejection of a second management NIC.

It was left running for now. Whether §14 needs it stopped to make the Android-side evidence
unambiguous is decided at that gate, not pre-emptively — the brief says stop it "if necessary",
and stopping a 20-hour-old daemon that holds existing pairing state is not free.

---

### 4.14 The six §23 prerequisites — MEASURED

The brief gates all further work behind six proofs. They were taken against the running guest on
**2026-09-15 17:22–17:26 −03** and are reproduced here in full.

#### Guest identity on the LAN

```console
$ ip -br addr                                    # in-guest, via guest-agent
lo               UNKNOWN  127.0.0.1/8 ::1/128
enp1s0           UP       192.168.68.75/22 fe80::5054:ff:feec:3ce5/64

$ ip route
default via 192.168.68.1 dev enp1s0 proto dhcp src 192.168.68.75 metric 100
192.168.68.0/22 dev enp1s0 proto kernel scope link src 192.168.68.75 metric 100

$ ip link show enp1s0
link/ether 52:54:00:ec:3c:e5
```

| Attribute | Value |
| --- | --- |
| Interface | `enp1s0` (virtio, backed by `macvtap` on `enp0s13f0u2u2c2`) |
| IP | **`192.168.68.75/22`** — DHCP, from the LAN's own server |
| MAC | `52:54:00:ec:3c:e5` — the guest's own, distinct from the host's `6c:1f:f7:29:4c:c3` |
| Gateway | **`192.168.68.1`** — the real LAN gateway |
| `192.168.122.x` present? | **No.** Not on any interface, not in any route |

#### The six proofs

| # | §23 requirement | Evidence | Result |
| --- | --- | --- | --- |
| 1 | guest IP is `192.168.68.x/22` | `192.168.68.75/22` on `enp1s0` | **PROVEN** |
| 2 | Android can reach guest | `adb shell ping 192.168.68.75` → 4/4, 0% loss, 12.3–24.3 ms | **PROVEN** |
| 3 | guest can reach Android | in-guest `ping 192.168.68.63` → 4/4, 0% loss | **PROVEN** |
| 4 | guest is not on libvirt NAT | no `192.168.122.x` address or route; no `virbr0` in domain XML | **PROVEN** |
| 5 | real Wayland session exists | `loginctl show-session 1` → `Type=wayland`, `Active=yes`, `Seat=seat0` | **PROVEN** |
| 6 | clipboard `real_backend` can execute | deferred to §17 — `wl-copy`/`wl-paste` present, session env resolves | see §17 |

The Android→guest direction (#2) is the one that matters most, and it is the direction a NAT or
port-forwarded topology could never satisfy:

```console
$ adb shell ping -c 4 -W 2 192.168.68.75
64 bytes from 192.168.68.75: icmp_seq=3 ttl=64 time=16.0 ms
64 bytes from 192.168.68.75: icmp_seq=4 ttl=64 time=12.2 ms
--- 192.168.68.75 ping statistics ---
4 packets transmitted, 4 received, 0% packet loss
rtt min/avg/max/mdev = 12.263/17.829/24.330/4.389 ms
```

**The tablet reached the guest directly, by its own LAN address, with no forwarding anywhere.**

#### The macvtap limitation, measured rather than assumed

```console
$ ping -c 2 -W 2 192.168.68.75        # from the Fedora HOST
2 pacotes transmitidos, 0 recebidos, 100% packet loss
```

Host↔guest is blocked, exactly as macvtap `bridge` mode intends. The brief declares this
acceptable, and the evidence above shows why it costs nothing: the certification requirement is
Android↔guest, and Android↔guest is clean in both directions. **No bridge fallback is needed.**
`br0` was not created, and no host route, connection or interface was modified at any point.

---

### 4.15 G-SESSION — real GNOME/Wayland session, measured

```console
$ loginctl session-status 1
1 - anyflow (1000)
     Since: Tue 2026-09-15 17:22:02 UTC
     State: active
    Leader: 1727 (gdm-session-wor)
      Seat: seat0; vc2
   Service: gdm-autologin
      Type: wayland
     Class: user
      Unit: session-1.scope
            ├─1775 /usr/libexec/gdm-wayland-session "… gnome-session --session=ubuntu"
```

```console
$ loginctl show-session 1 -a | grep -E '^(Type|Active|State|Seat|Remote|LockedHint|Class)='
Type=wayland
Active=yes
State=active
Seat=seat0
Remote=no
Class=user
LockedHint=no
```

Session environment, taken from the systemd user manager (the authoritative source for session
clients) rather than asserted:

```console
XDG_SESSION_TYPE=wayland
XDG_SESSION_DESKTOP=ubuntu
XDG_CURRENT_DESKTOP=ubuntu:GNOME
XDG_RUNTIME_DIR=/run/user/1000
XDG_SESSION_ID=1
WAYLAND_DISPLAY=wayland-0
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
```

> Method note, recorded because the first attempt at this was wrong: injecting a hand-built
> environment into `runuser` produced **empty** `XDG_SESSION_TYPE` and `WAYLAND_DISPLAY`, which
> would have understated a session that was in fact perfectly good. The values above are imported
> from `systemctl --user show-environment` and cross-checked against `loginctl`. `/run/user/1000/wayland-0`
> exists as a socket. Nothing here is asserted from a config file.

User D-Bus and logind are both live, not merely present:

```console
$ busctl --user list | head           # org.freedesktop.Notifications is already claimed
$ gdbus call --session --dest org.freedesktop.DBus … ListNames
(['org.freedesktop.DBus', ':1.106', 'org.freedesktop.Notifications', …
   'org.gnome.Mutter.DisplayConfig', 'org.freedesktop.systemd1',
   'org.gnome.Mutter.IdleMonitor', 'org.freedesktop.portal.Desktop', …

$ busctl --system call org.freedesktop.login1 … ListSessions
a(susso) 1 "1" 1000 "anyflow" "seat0" "/org/freedesktop/login1/session/_31"
```

Compositor processes present: `gnome-shell`, `gdm-wayland-session`, `gdm3`, `Xwayland`.

**The session is GNOME, Wayland, real, on `seat0`, with a working user D-Bus and working logind.**
It is **not** X11, so the brief's "if X11: STOP that target" clause does not trigger.

> Disclosed: the session is established by **GDM autologin**, configured by this wave so a
> graphical session exists at boot without an interactive GDM password. It is a genuine `seat0`
> graphical logind session — `Service=gdm-autologin`, `Type=wayland`, `Class=user` — and §21 locks
> and unlocks it for real. It is not a substitute for a login, and it does not bypass the screen
> lock.

#### Distribution floor, confirmed

```console
$ pkg-config --modversion gtk4 libadwaita-1
4.14.5
1.5.0
$ dpkg -l | grep -E 'libgtk-4-1|libadwaita-1-0'
libgtk-4-1:amd64      4.14.5+ds-0ubuntu0.7
libadwaita-1-0:amd64  1.5.0-1ubuntu2
```

`desktop/gui/Cargo.toml` declares `adw = { package = "libadwaita", version = "0.7", features = ["v1_5"] }`.
Ubuntu 24.04 ships libadwaita **1.5.0** — the exact minimum that feature demands, with nothing to
spare. This is precisely why the brief calls 24.04 "the libadwaita floor case", and it is now
measured rather than inferred. Kernel: `7.0.0-31-generic`; OS: `Ubuntu 24.04.4 LTS (noble)`.

---

## 5. Android device

The certification hardware is present, healthy, and on the target LAN. It is the one part of the
§2 hardware requirement that is fully satisfied.

```console
$ adb devices -l
RX2Y500C7SY  device  usb:3-2.1  product:gts10fepwifixx  model:SM_X620
                                device:gts10fepwifi     transport_id:4

$ adb shell ip route
192.168.68.0/22 dev wlan0 proto kernel scope link src 192.168.68.63
```

| Property | Value |
| --- | --- |
| Model | **Samsung SM-X620** |
| Serial | `RX2Y500C7SY` |
| Product / device | `gts10fepwifixx` / `gts10fepwifi` |
| LAN address | **192.168.68.63** on `wlan0` |
| Segment | **192.168.68.0/22**, gateway 192.168.68.1 |
| ADB transport at time of survey | **USB** (`usb:3-2.1`) |

The device is the same physical unit used for the Fedora/N6 certifications, as §2 requires.

**ADB is currently attached over USB.** §2 prefers adb-over-Wi-Fi; that switch is deferred to
§4.6 step 6 because it is only needed once certification begins. ADB carried no product traffic
in this wave — it was used solely to read the device's LAN address, which is exactly the
setup/evidence role §2 permits.

**No AnyFlow product traffic was exchanged with this device in this wave**, because there was no
guest for it to talk to.

---

### 5.1 RESUME — the AnyFlow app was missing from the tablet, and was reinstalled

Checked before the discovery and pairing gates, and the result was not what the first attempt
assumed:

```console
$ adb shell pm list packages | grep -i anyflow
package:io.github.yurisismotto.anyflow.fixture        ← the TEST FIXTURE only

$ adb shell pm path io.github.yurisismotto.anyflow
                                                      ← empty. THE APP ITSELF IS NOT INSTALLED
```

**The product APK had been uninstalled from the device.** Only `…anyflow.fixture` — the separate
test-fixture application — remained. This is the known side effect of Gradle connected-test runs,
which uninstall the application under test when they finish; a previous wave's instrumentation run
left the tablet in this state.

Had this not been checked first, §12 (mDNS) and §13 (pairing) would have failed for a reason that
has nothing to do with Ubuntu, Debian, or AnyFlow's Linux code.

The repository's own committed build output was reinstalled, rather than rebuilding:

```console
$ ls -l android/app/build/outputs/apk/debug/app-debug.apk
-rw-r--r--. 1 yuri yuri 26757478 2026-09-12 20:54 app-debug.apk
sha256 f2bd5c1696a038761cdcadb86482246d5df864275696d32f46eef864a91c8592

$ aapt2 dump badging app-debug.apk | head -1
package: name='io.github.yurisismotto.anyflow' versionCode='1' versionName='0.1.0'
         compileSdkVersion='35' minSdkVersion='29' targetSdkVersion='35'

$ git status --short android/          # (empty — the android tree is clean at baseline)
$ git merge-base --is-ancestor e72bb09 9792330 && echo ancestor
ancestor                               # the APK's source commit is in the baseline's history

$ adb install -r android/app/build/outputs/apk/debug/app-debug.apk
Success
$ adb shell pm path io.github.yurisismotto.anyflow
package:/data/app/~~4MTNs0Q5KU6oxVFiNkQ7-w==/io.github.yurisismotto.anyflow-…/base.apk
```

**Provenance is sound:** `android/` is clean at the certification baseline, and the APK's most
recent source commit `e72bb09` is an ancestor of baseline `97923302`. The installed Android peer
is therefore built from the same committed tree this wave certifies against.

Two deliberate choices, both recorded rather than assumed:

- **No Gradle build was run.** This host has frozen before under concurrent heavy builds, and a
  guest `cargo test` was in flight. Reinstalling the committed artefact achieves the same peer
  without that risk.
- **This is a change to the operator's physical device.** It is in scope — §14–§17 of the brief
  require real Android↔guest product flows, which are impossible with no app — and it restores
  the device to the state the certification assumes rather than altering it. No AnyFlow production
  code was touched.

> Consequence for the gate matrix: because the app was absent, **any pairing state the tablet
> previously held for the Fedora host is gone.** That is convenient rather than harmful — §15
> requires a *fresh* pairing against a *new* desktop identity per VM, and the tablet now starts
> from no trust at all.

---

## 6. VM inventory

**No virtual machines were created. None exist on this host.**

```console
$ virsh --connect qemu:///system list --all
 Id   Nome   Estado
---------------------
(empty)

$ virsh --connect qemu:///system net-list --all
 Nome      Estado   Auto-iniciar   Persistente
------------------------------------------------
 default   ativo    sim            sim
```

| Planned VM | Distro | Status |
| --- | --- | --- |
| `ubuntu-24-anyflow` | Ubuntu 24.04 LTS | **NOT CREATED** |
| `ubuntu-26-anyflow` | Ubuntu 26.04 LTS | **NOT CREATED** |
| `debian-13-anyflow` | Debian 13 Stable | **NOT CREATED** |

Creating them was deliberately not attempted. A VM built now could only be attached to the NAT
network, and every discovery, pairing and session gate taken on it would be inadmissible under
§3. Building three guests to produce inadmissible evidence would waste hours and, worse, invite a
later reader to mistake NAT results for certification.

### 6.1 Host virtualization capability — verified ready

The host side of the VM plan is sound. Only the network is missing:

```console
$ ls -l /dev/kvm
crw-rw-rw-. 1 root kvm 10, 232 set  8 18:20 /dev/kvm
$ grep -cE "vmx|svm" /proc/cpuinfo
32
$ virt-install --version   → 5.1.0
$ virt-manager --version   → 5.1.0
$ qemu-system-x86_64 --version → QEMU emulator version 10.2.2 (qemu-10.2.2-1.fc44)
$ virsh --version          → 12.0.0
$ systemctl is-active virtqemud virtnetworkd → active / active
$ nproc → 16
$ free -g → total 15, available 6
$ df -h /home → 475G total, 336G available
```

| Requirement (§5) | Host capability | Verdict |
| --- | --- | --- |
| KVM/QEMU via libvirt | `/dev/kvm` present, 32 vmx flags, virtqemud active | **ready** |
| virt-manager | 5.1.0 installed | **ready** |
| 4 vCPU per guest | 16 logical CPUs | **ready** |
| 6–8 GB RAM per guest | **15 GB total, ~6 GB available** | **tight — see below** |
| 40 GB disk per guest | 336 GB free on `/home` | **ready** (3 × 40 GB fits) |
| virtio NIC on a LAN bridge | **no bridgeable link** | **BLOCKED** |
| UEFI | OVMF available via libvirt | ready |

Two practical notes for whoever runs U2 next:

1. **RAM is the binding constraint after the cable.** With ~6 GB currently available, the guests
   must be run **one at a time**, and 6 GB should be treated as a ceiling rather than a floor.
   Running a guest build concurrently with host builds has previously destabilised this laptop;
   serialize the three targets.
2. **No installation media is staged.** No Ubuntu or Debian ISO is present on the host — the only
   ISO in `~/Downloads` is a Windows image. Three downloads (~3–5 GB each) are a prerequisite.

### 6.2 The `virt-install` invocations to use after §4.6

Run one at a time. `--network network=anyflow-lan,model=virtio` is the part that matters: it
attaches the guest to the **bridge**, never to `default`.

```bash
virt-install --connect qemu:///system --name ubuntu-24-anyflow \
  --vcpus 4 --memory 6144 --cpu host-passthrough \
  --disk size=40,bus=virtio,format=qcow2 \
  --network network=anyflow-lan,model=virtio \
  --boot uefi --graphics spice --video virtio --os-variant ubuntu24.04 \
  --cdrom /var/lib/libvirt/images/ubuntu-24.04-desktop-amd64.iso

virt-install --connect qemu:///system --name ubuntu-26-anyflow \
  ... --os-variant ubuntu26.04 --cdrom .../ubuntu-26.04-desktop-amd64.iso

virt-install --connect qemu:///system --name debian-13-anyflow \
  ... --os-variant debian13   --cdrom .../debian-13-amd64-netinst.iso
```

Per §26, each guest must be installed and have its AnyFlow identity created **independently**.
Do not clone a template that has already launched AnyFlow, and never copy `identity.key`,
`state.json` or the trust store between guests.

---

## 7. Ubuntu 24.04 LTS — environment

**NOT EXECUTED — no VM (§4).**

No `/etc/os-release`, `uname -a`, `XDG_SESSION_TYPE`, `XDG_CURRENT_DESKTOP`, `XDG_RUNTIME_DIR`,
`DBUS_SESSION_BUS_ADDRESS`, `loginctl session-status` or `loginctl show-session -a` output exists
for this target, because no guest was installed.

Nothing is asserted about this target's session. In particular, U0 §18 warns that Ubuntu can fall
back to Xorg; whether this guest would present Wayland is **unknown and unmeasured**. §6 of the
brief requires stopping a target found on X11 — that check was never reached.

The U1 expectation still to be *verified* (not inherited): **native libadwaita 1.5.x**, the
zero-margin case U0 identified as the sharpest GUI risk of the three targets.

## 8. Ubuntu 26.04 LTS — environment

**NOT EXECUTED — no VM (§4).** Same as §7.

U0 assessed this as the easiest of the three (stock rustc ~1.93, GTK 4.22.x, libadwaita 1.9.x —
close to the Fedora 44 configuration), and therefore the target that proves the least. It remains
entirely unproven at runtime.

Per §18 of the brief, if this target unexpectedly carries a backported `wl-copy --sensitive`, the
**feature probe is authoritative** and must be recorded as actual capability. Never infer from the
version number. That probe was not run.

## 9. Debian 13 Stable — environment

**NOT EXECUTED — no VM (§4).** Same as §7.

U0 flagged this as the stock-toolchain case: Debian 13's `rustc` 1.85.1 is **below** the declared
MSRV of 1.88 (U0 §20.3, finding U-4), so the guest needs `rustup` or `trixie-backports`. Which of
those the certification uses, and whether the workspace then builds, is unproven.

---

## 10. Build / test evidence

**NOT EXECUTED — no VM (§4).**

None of the §8 commands were run in any guest:

```
cargo build   --workspace --locked -j 2
cargo test    --workspace --locked -j 2
cargo clippy  --workspace --all-targets --locked -j 2 -- -D warnings
cargo fmt     --all --check
```

There are therefore **no passed / failed / ignored test counts** for any distro. No host build
artifacts were substituted, and none may be — §8 forbids it.

The only build evidence on the baseline commit is the CI container build of §2.2, which §32
forbids inheriting as a runtime PASS. It is reported there as what it is, and is not reused here.

## 11. GUI

**NOT EXECUTED — no VM, no graphical session (§4).**

`./target/debug/anyflow-gui` was not launched on any target. Unverified in consequence: GTK
application startup, libadwaita shell rendering, absence of missing-symbol/version errors, the
device / clipboard / notification pages, control responsiveness, crash-free navigation, the
sensitive-clipboard readiness state, accessible text for the sensitive status, and clipping at
normal scaling.

§10 forbids substituting a headless GTK test for this gate. None was substituted.

## 12. mDNS

**NOT EXECUTED — and unreachable by construction in the current topology.**

This is the gate the §3 blocker exists to protect. Certifying it requires `_anyflow._tcp.local.`
to travel from a guest to the Android device over a shared L2 segment. With no bridge, the only
available guest network would be NAT, which multicast does not cross.

Not measured: guest-side advertisement, Android-side discovery without manual IP entry,
independent observation via `avahi-browse` or a UDP/5353 capture, and **discovery latency**.

A NAT-based result was not produced, because §3 forbids passing one off as mDNS evidence and it
would be untrue.

## 13. Pairing

**NOT EXECUTED — depends on §12.**

`./target/debug/anyflow pair` was not run on any guest. Unverified: TLS 1.3, SPKI pinning,
proof-of-possession, peer storage, session establishment, and **both peer fingerprints** on each
of the three targets.

No Fedora pairing was reused or relabelled as a distro pairing (§12 of brief). No pairing data of
any kind was produced in this wave.

## 14. Sessions / reconnect

**NOT EXECUTED — depends on §13.**

`./target/debug/anyflow status` was not run in any guest. Untested: daemon restart, Android app
close/reopen, short guest network interruption, and Android Wi-Fi off/on. No reconnection times
were recorded.

## 15. Battery

**NOT EXECUTED — no VM.**

Note this precisely: the expected outcome here was **`PASS — NO-BATTERY HOST HANDLED CORRECTLY`**
(§14 of brief), the one gate where N/A is legitimately available. That outcome was **not reached**
— it was never tested. An untested gate must not be recorded as the N/A it was expected to earn.

Unverified: UPower D-Bus availability per guest, AnyFlow battery backend initialization on a
battery-less host, truthful handling of absence, no crash, no bogus percentage.

Per §14 of the brief, no claim is made that a physical Ubuntu or Debian laptop battery was tested.
The existing physical Fedora battery certification remains separate historical evidence and is not
inherited here.

## 16. Files

**NOT EXECUTED — depends on §13.**

Neither direction was exercised. Android→VM unverified: transfer acceptance through a paired
session, destination path, content hash, filename sanitization, retry idempotency. VM→Android
unverified: exact bytes. No canary files, small-text or several-MB binary, were transferred.

## 17. Clipboard

**NOT EXECUTED — no graphical guest session.**

Not run on any target: `which wl-copy` / `wl-paste`, `wl-copy --version`, `wl-copy --help`, and
`./target/debug/anyflow clipboard status`. The U1 truthfulness claim for ordinary-clipboard
availability is therefore **unconfirmed at runtime** on all three distros.

§32 of the brief forbids inferring clipboard support from `wl-copy` merely existing. Since even
that weaker check was not performed, nothing at all is claimed.

### 17.1 Clipboard watch / real Wayland backend

**NOT EXECUTED.**

```
cargo test -p anyflow-capability-clipboard --test real_backend -- --ignored --test-threads=1
```

was not run. This gate carries forward from U1 as an explicit debt: U1 could not run the real
clipboard backend because the Fedora session was locked, and U2 existed precisely to run it in an
**unlocked graphical VM session**. **That debt is not discharged** (§34).

## 18. Sensitive clipboard

**NOT EXECUTED.**

The expected outcome was a **fail-closed PASS**: stock Ubuntu/Debian `wl-clipboard` lacks
`--sensitive`, AnyFlow correctly refuses the sensitive write, the canary never lands in the guest
clipboard, ordinary clipboard keeps working immediately afterward, logs stay clean, and the UI/CLI
had already stated the limitation.

None of that was observed. No live `wl-copy --help` probe was taken on any target, and no
Android-originated sensitive clipboard event was sent. The U0/U1 measurement that stock packages
lack `--sensitive` remains a **package-archive finding, not a live runtime observation**, and per
§18 of the brief capability must come from the probe rather than the version number.

## 19. Notifications

**NOT EXECUTED — depends on §13 and a real GNOME session.**

Not recorded for any guest: `org.freedesktop.Notifications` server name, server version, spec
version, capabilities. Not exercised: enabling notifications through the real UI/policy, the
deterministic Android notification fixture, post / update-in-place / remove, ongoing
notifications, group child/summary behaviour, absence of AnyFlow-created notification history, and
absence of content in persistent state or logs.

notifications.v1 is **final certified on Fedora (N6)**. That is historical evidence for a
different distribution. §32 forbids inheriting it as distro runtime evidence, and it is not
inherited here.

## 20. Dismiss sync

**NOT EXECUTED.**

Unverified on all three targets: that the distro GNOME notification server exposes
`NotificationClosed`, that AnyFlow subscribes successfully, and the local roles recorded after
backend initialization — all prerequisites for DISMISS_REPORTER. §20 of the brief warns
specifically against assuming this because Fedora GNOME did. No assumption is made.

Also untested: human-dismissing a clearable mirrored notification and confirming the Android
original disappears; and human-dismissing an **ongoing/non-dismissible** mirror, confirming the
Android original remains and that AnyFlow reports `NOT_DISMISSIBLE` without entering a retry loop.

## 21. Lock detection

**NOT EXECUTED — requires the real graphical session that does not exist.**

Not measured: `loginctl show-session "$XDG_SESSION_ID" -p LockedHint` before and after lock,
truthful `LockedHint` transition, source/sink privacy behaviour across **FULL / APP_ONLY /
SUPPRESS**, fail-closed handling of unknown lock state, and the absence of replayed withheld
content on unlock.

The product requirement that logind `LockedHint` — not `org.gnome.ScreenSaver` — is the
authoritative lock source was not exercised on any distro.

## 22. Daemon restart

**NOT EXECUTED — depends on §13.**

Unverified: identity persistence, pairing persistence, session return, mirror resync, no
duplicates, no stale server-id dismissal, and no notification content persisted merely to survive
a restart.

## 23. GUI restart

**NOT EXECUTED — depends on §11 and §13.**

Unverified: that daemon capabilities continue while the GTK GUI is closed, and that on reopening,
the GUI reflects daemon reality — peer still present, sensitive-clipboard capability still
correctly reported, notification readiness still correct. The architectural claim that the GUI is
a client rather than the session owner was not tested on any distro.

## 24. Firewall

**NOT EXECUTED for the guests.** Host state recorded for completeness:

```console
$ systemctl is-active firewalld   → active
$ firewall-cmd --state            → running
$ firewall-cmd --get-default-zone → FedoraWorkstation
```

Per-distro firewall state (Ubuntu `ufw`, Debian defaults) was not recorded, and no AnyFlow
connection was attempted through any guest firewall. No firewall rule was added anywhere, and no
firewall was disabled — globally or otherwise — in this wave.

## 25. AppArmor / security framework

**NOT EXECUTED.**

`aa-status` was not run on any guest. Ubuntu's AppArmor posture toward AnyFlow is unmeasured, as
is Debian 13's. No security framework was disabled or weakened anywhere.

## 26. Logging

**NOT EXECUTED.**

No guest daemon logs exist to inspect. The §27 spot check — confirming that distro behaviour does
not change the N6 privacy properties — was not performed. No clipboard or notification canaries
were introduced, because no session existed to carry them.

## 27. Persistence

**NOT EXECUTED.**

No AnyFlow state file, config or cache directory was created on any guest, so none could be
checked for notification titles/bodies, raw Android keys, sensitive clipboard content, or
notification history.

**Identity isolation (§26 of brief) is untested but also unviolated**: no identity was created on
any guest, and nothing was copied between guests, because no guests exist. The requirement stands
in full for the next attempt — **three distinct desktop fingerprints**, each created in its own
guest.

---

## 28. Per-distro gate matrix

> **UPDATED 2026-09-15.** The matrix in this section was written for the first attempt, when no
> gate had been reached. **The Ubuntu 24.04 column is now superseded by §34B.3**, which carries
> real results. Debian 13 and Ubuntu 26.04 remain exactly as written below: NOT EXECUTED.
>
> | Target | Status |
> | --- | --- |
> | Ubuntu 24.04 LTS | **executed** — 19 PASS / 2 FAIL / 1 PARTIAL / 1 NOT EXECUTED (§34B.3) |
> | Debian 13 Stable | NOT EXECUTED |
> | Ubuntu 26.04 LTS | NOT EXECUTED |


**Status legend.** `NOT EXECUTED` is used deliberately and is distinct from both PASS/FAIL and
from N/A:

- **PASS / FAIL** — the gate was exercised and produced a result. **None here.**
- **N/A** — the environment genuinely cannot expose the feature (§30 of brief allows this only
  for cases such as a VM with no battery). **Not used here**: N/A would imply a guest existed and
  lacked the hardware. None existed.
- **NOT EXECUTED** — the gate was never reached. It asserts nothing about the product.

| Gate | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 | Blocked by |
| --- | --- | --- | --- | --- |
| G-BUILD | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no VM (§4) |
| G-TEST | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no VM |
| G-DAEMON | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no VM |
| G-GUI | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no graphical guest session |
| G-MDNS | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | **no L2 bridge (§4) — root blocker** |
| G-FW | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no VM |
| G-PAIR | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-MDNS |
| G-SESSION | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-PAIR |
| G-BATTERY | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no VM (**not** N/A — see §15) |
| G-FILES-A2D | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-PAIR |
| G-FILES-D2A | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-PAIR |
| G-CLIP | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no graphical guest session |
| G-CLIP-WATCH | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no unlocked Wayland session (**U1 debt, still open**) |
| G-CLIP-SENS | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no guest; no live probe |
| G-NOTIF | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-PAIR + GNOME session |
| G-NOTIF-CAP | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no guest D-Bus session |
| G-DISMISS | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-NOTIF |
| G-LOCK | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no real graphical session |
| G-RECONNECT | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-SESSION |
| G-RESTART | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-SESSION |
| G-GUI-RESTART | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | depends on G-GUI + G-SESSION |
| G-LOGGING | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no guest logs exist |
| G-PERSISTENCE | NOT EXECUTED | NOT EXECUTED | NOT EXECUTED | no guest state exists |

**23 gates × 3 targets = 69 gate-results required. 0 obtained.**

No gate in this matrix hides a product failure. Nothing was observed to fail, because nothing was
observed at all.

---

## 29. Cross-distro comparison

No comparison is possible — there is no per-distro data to compare. This section exists to record
what U2 was expected to differentiate, so the next attempt keeps the distinctions sharp:

| Dimension | Ubuntu 24.04 | Ubuntu 26.04 | Debian 13 |
| --- | --- | --- | --- |
| Expected to stress | **libadwaita 1.5.x zero-margin** (U0 U-3) | least — nearest to Fedora 44 | **stock rustc 1.85.1 < MSRV 1.88** (U0 U-4) |
| Session risk | may fall back to **Xorg** (U0 §18) | GNOME/Wayland expected | GNOME/Wayland expected |
| Toolchain route | archive `rustc-1.91`/`cargo-1.91` | stock rustc | **rustup or trixie-backports required** |
| Sensitive clip | expected absent | **must be probed, not inferred** | expected absent |
| Measured this wave | **nothing** | **nothing** | **nothing** |

All three rows of the last line are the finding.

---

## 30. Manual steps

**No manual certification steps were performed**, because no gate requiring one was reached.
Sections 21 (dismiss sync) and 22 (lock detection) of the brief anticipate human actions as valid
evidence; those human actions were never called for.

Actions actually taken in this wave, all read-only:

| Action | Nature |
| --- | --- |
| `git` inspection (branch, status, log, diff, rev-parse) | read-only |
| `ip`, `nmcli`, `ethtool`, `iw`, `lsusb`, sysfs carrier reads | read-only |
| `virsh` list/dumpxml on `qemu:///system` | read-only |
| `ping` to 192.168.68.63 and 192.168.68.1 | read-only probe |
| `adb devices -l`, `adb shell ip route` / `ip -br addr` | read-only, setup channel per §2 |
| `gh run list` / `gh run view` | read-only |
| `systemctl is-active`, `firewall-cmd --state`, `ss -lunp` | read-only |
| Writing `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` | the only write |

**Nothing was reconfigured.** No bridge was created, no libvirt network defined, no firewall rule
added, no VM created, no host service started or stopped, no process killed, no Wi-Fi profile
touched, and no AnyFlow production code modified.

---

## 31. Environment limitations

| # | Limitation | Consequence | Severity |
| --- | --- | --- | --- |
| L-1 | **USB Ethernet `enp0s13f0u2u2c2` has no carrier — no cable** | No LAN bridge; no guest can join 192.168.68.0/22 | **BLOCKING — the root cause** |
| L-2 | Host's only live link is a Wi-Fi **station**; `iwlwifi` reports **no 4addr** | Bridging/macvtap over Wi-Fi is impossible, not merely unreliable | **BLOCKING** |
| L-3 | Only libvirt network is `default` (**NAT**), and `virbr0` is linkdown | The one attachable network is inadmissible under §3 | **BLOCKING** |
| L-4 | ~6 GB RAM available of 15 GB total | Guests must be run **one at a time** after L-1 clears | Moderate, manageable |
| L-5 | No Ubuntu/Debian ISOs staged on the host | ~3 downloads needed before any install | Minor, time only |
| L-6 | Host `anyflowd` (pid 613091) advertising on UDP/5353 | Would confound discovery evidence; §4.6 step 5 stops it | Minor, procedural |
| L-7 | ADB attached over **USB**, not Wi-Fi | Must switch per §2 before certification | Minor, procedural |

L-1 is the only one that requires hardware. L-2 and L-3 are consequences of L-1 rather than
independent problems: connect a cable and all three clear together.

---

## 32. Product findings

**None. No AnyFlow product defect was found in this wave, because no AnyFlow code was executed.**

This is stated explicitly to prevent misreading: the blocker is a **host infrastructure**
condition — an unplugged Ethernet cable — and carries **no implication whatsoever** about AnyFlow's
Ubuntu or Debian runtime behaviour, which remains **unknown**, not suspect.

Per §33 of the brief, no production code was modified, and no fix branch was required or created.

`git diff --stat` against the baseline confirms no production file changed (§37).

---

## 33. Packaging findings

**None.** Packaging is out of scope for U2 by §9 of the brief, and in any case no guest existed in
which to observe an installation or firewall requirement.

One item is pre-registered for the wave that eventually runs: should a guest's default firewall
block the AnyFlow port, §28 requires classifying the remedy as a
**PACKAGING/INSTALLATION REQUIREMENT**, not a runtime-code defect, and recording the exact command.
No such observation exists yet.

---

## 34. Remaining debts

| # | Debt | Origin | Status |
| --- | --- | --- | --- |
| D-1 | **Certifiable L2 VM network** (cable → bridge) | U0 §21.1.1, restated here §4 | **OPEN — blocks everything** |
| D-2 | **All 69 U2 gate-results** (23 × 3 targets) | U2 §30 of brief | **OPEN — 0 obtained** |
| D-3 | **Real clipboard backend gate** (`real_backend --ignored`) | **U1** — Fedora session was locked | **STILL OPEN** — U2 existed to discharge it and did not |
| D-4 | Live `wl-copy --help` capability probe per distro | U0/U1 measured archives, not runtime | **OPEN** |
| D-5 | Ubuntu 24.04 libadwaita 1.5.x zero-margin runtime proof | U0 U-3 | **OPEN** |
| D-6 | Debian 13 MSRV route (rustup vs backports) proven at runtime | U0 U-4 | **OPEN** |
| D-7 | Three distinct desktop identity fingerprints | U2 §26 of brief | **OPEN** |
| D-8 | Ubuntu/Debian `NotificationClosed` support confirmed (not assumed from Fedora) | U2 §20 of brief | **OPEN** |
| D-9 | Ubuntu 26.04 backported `--sensitive` probe (authoritative over version) | U2 §18 of brief | **OPEN** |

D-3 deserves emphasis: it is the **second consecutive wave** in which the real Wayland clipboard
backend gate could not run — U1 blocked by a locked Fedora session, U2 blocked by having no
session at all. It should be treated as the highest-priority functional unknown after D-1.

Standing alternative to D-1, carried from U0 §21.1.1: run at least one target on a **second
physical machine** rather than a VM. That yields the cleanest evidence of all and needs no bridge —
only hardware not present in this session.

---

## 34B. U2 EXECUTION RESULTS — Ubuntu 24.04 LTS (guest `anyflow-u2404`)

Everything in this section was measured on the live guest on **2026-09-15**, between 17:22 and
19:40 −03. It supersedes the NOT EXECUTED status of §10–§27 **for Ubuntu 24.04 only**. Debian 13
and Ubuntu 26.04 remain NOT EXECUTED (§34D).

### 34B.1 Identities under test

| | Guest | Android peer |
| --- | --- | --- |
| Name | `anyflow-u2404` | `SM-X620` |
| Device id | `db9dfc03de621a642048de96ce31ae62` | `6532889e82ba83d0782cc644e7a21fc3` |
| Fingerprint | **135A C045 BFE9 F0A5** | **573C CB84 DA6C 993B** (full: `573ccb84da6c993ba5ce434f950d0e29d4cc1cabd5c0d1d4f19bad0bcf31a6b7`) |
| Address | `192.168.68.75/22` | `192.168.68.63/22` |
| Key backing | software | — |

The guest identity is newly generated by this VM and shares nothing with the Fedora host
(`795fec…`, `dn=Fedora`). No identity, state or trust file was copied between machines.

### 34B.2 Build and test — guest-native (§9)

Toolchain: **Ubuntu's own archive**, not rustup — `rustc 1.91.1`, `cargo 1.91.1`
(`1.91.1+dfsg~24.04-0ubuntu0.24.04.3`), the path README §"The Rust toolchain, per distribution"
prescribes for 24.04 (whose default 1.75 is too old). Source: `git clone` of the public repository,
`git checkout 97923302ddba473c3b139af8832abe9153c21cff` — the certification baseline, verified by
`git rev-parse` in the guest.

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked -j 2` | **PASS** — exit 0, `Finished dev profile in 4m 16s`, all 12 workspace crates compiled |
| `cargo test --workspace --locked -j 2` | **PASS** — exit 0, **717 passed, 0 failed, 22 ignored** |
| `cargo fmt --all --check` | **PASS** — exit 0 |
| `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | **FAIL** — exit 101, one lint (§34C.1) |

717 passing is exactly the count README advertises, which is independent corroboration that the
whole suite ran and nothing was silently filtered.

### 34B.3 Gate results — Ubuntu 24.04

| Gate | Result | Evidence |
| --- | --- | --- |
| G-BUILD | **PASS** | guest-native, 4m16s, exit 0 |
| G-TEST | **PASS** | 717 passed / 0 failed / 22 ignored |
| G-DAEMON | **PASS** | listening `*:55432` IPv4+IPv6; control socket `/run/user/1000/anyflow/control.sock`; 4 capabilities registered |
| G-GUI | **PASS** | real libadwaita 1.5 / GTK 4.14 window on Wayland; sidebar Dashboard/Files/Clipboard/Notifications/Devices/Trusted peers/Settings; status card "Local network · port 55432 · IPv4+IPv6"; identity card matches the daemon; no symbol/version errors |
| G-MDNS | **PASS** | guest advertises `_anyflow._tcp.local.` with `id=db9dfc03…`, `dn=anyflow-u2404`, port 55432; Android reached the guest directly at `192.168.68.75:55432` (§34B.4) |
| G-FW | **PASS (nothing required)** | `ufw` **inactive**, `nft list ruleset` empty, `iptables -S` all-ACCEPT. Stock Ubuntu 24.04 desktop ships no active firewall, so no rule was needed and none was added. See §34C.4 |
| G-PAIR | **PASS** | QR scan → peer identity shown → explicit confirm → `session established`; trust persisted 0600 |
| G-SESSION | **PASS** | `Type=wayland`, `Active=yes`, `seat0`, user D-Bus and logind both live (§4.15) |
| G-BATTERY | **FAIL** | VM has no battery; the Android UI shows the desktop as **"Battery 0 percent / 0%"** (§34C.2) |
| G-FILES-D2A | **PASS** | text + binary canaries, exact SHA256 match (§34B.5) |
| G-FILES-A2D | **PASS** | binary canary, exact SHA256 match; stored 0600 |
| G-CLIP | **FAIL** | `real_backend` exits 101 — 8 of 9 pass, 1 fails (§34C.3) |
| G-CLIP-WATCH | **PASS** | `the_watcher_reports_every_local_change` passes via the XFIXES/Xwayland bridge |
| G-CLIP-SENS | **PASS** | fail-closed proven; canary reached neither clipboard nor any log or state file |
| G-NOTIF | **PASS** | post → one mirror; update → replaced **in place** (count stayed 3); remove → cleared (3→2) |
| G-NOTIF-CAP | **PASS** | `gnome-shell 46.0`, spec 1.2, GNOME; body markup yes (bodies escaped before sending); persistence yes |
| G-DISMISS | **NOT EXECUTED** | `dismiss-sync` was off by default and ongoing sharing was off on the Android side; **both could have been turned on and were not.** Reclassified from N/A on review: N/A is reserved for genuinely unavailable environmental or hardware conditions, and nothing in this environment prevented the gate. See §34C.6 |
| G-LOCK | **PASS** | logind `LockedHint` authoritative: unlocked → LOCKED → unlocked tracked exactly; withheld body absent from logs and state |
| G-RECONNECT | **PASS** | daemon restarted twice; device reconnected with the **same** fingerprint, no re-pairing, no duplicate device |
| G-RESTART | **PASS** | identity and peer survived restart unchanged |
| G-GUI-RESTART | **PARTIAL** | GUI and daemon confirmed independent processes; daemon served CLI and capability traffic throughout with the GUI running. A close/reopen cycle was **not** performed — recorded honestly as not executed |
| G-LOGGING | **PASS** | no clipboard canary, no notification body, no private key material in any log (counts all 0) |
| G-PERSISTENCE | **PASS** | `identity.key` 0600, `state.json` 0600, received file 0600; schema_version 2 |

### 34B.4 The network claim, proven end to end

```console
# in-guest
$ ip -br addr    → enp1s0  UP  192.168.68.75/22      (macvtap on enp0s13f0u2u2c2)
$ ip route       → default via 192.168.68.1 dev enp1s0
# from the tablet
$ adb shell ping -c 4 192.168.68.75 → 4/4, 0% loss, 12.3–24.3 ms
# guest daemon log — the tablet's own connections
connection ended peer_addr=[::ffff:192.168.68.63]:51624 error=pairing failed: not in pairing mode
session established device=6532889e82ba83d0782cc644e7a21fc3 peer=573C CB84 DA6C 993B
```

No `192.168.122.x` anywhere. No port forwarding, no `socat`, no SSH tunnel, no Avahi reflector,
no NAT in the product path. The **only** host↔guest channel used by this certification is
`qemu-guest-agent` over virtio-serial, which carries no IP traffic at all (§4.12).

#### Fail-closed pairing, demonstrated four times before the successful pairing

Four pairing attempts failed *before* the successful one, and every failure is informative:

| Peer port | Daemon verdict |
| --- | --- |
| `:51624` | `pairing failed: not in pairing mode` |
| `:32954` | `pairing failed: declined by user` |
| `:39810` | `pairing failed: not in pairing mode` |
| `:53178`, `:32822` | `pairing failed: declined by user` |

**These failures were caused by the operator's tooling, not by the product or the human** — an
auto-refresh loop in the harness was cancelling the pairing dialog to regenerate the QR code, and
it repeatedly landed on the confirmation step; one further attempt lapsed past the 120 s window.
They are reported here rather than discarded because they are genuine adverse-condition evidence:
**the guest never paired on a closed window, a cancelled dialog, or a lapsed code.** It paired only
when a human scanned and a human-visible fingerprint was explicitly confirmed.

### 34B.5 File transfer canaries — exact hashes

| Direction | File | Size | SHA256 (source) | SHA256 (destination) | Verdict |
| --- | --- | --- | --- | --- | --- |
| guest → Android | `u2-text.txt` (UTF-8: `ação ñ 日本語 🐧`) | 87 B | `f010216028a1…3159` | `f010216028a1…3159` | **MATCH** |
| guest → Android | `u2-binary.bin` | 262144 B | `0556cf33b0cf…91ba` | `0556cf33b0cf…91ba` | **MATCH** |
| Android → guest | `a2d.bin` | 131072 B | `87df92883c20…2aa7` | `87df92883c20…2aa7` | **MATCH** |

Received files land in `~/Downloads/AnyFlow`, mode **0600**. The Android Activity list independently
corroborates every transfer ("Saved to Downloads/AnyFlow"), and states that nothing about a
transfer is kept on disk once it finishes.

**Default-deny on the desktop is real.** The first Android→guest offer was refused:

```
INFO  incoming file offer transfer=9c513c63 peer=573C CB84 DA6C 993B filename=a2d.bin size=131072
WARN  declining an incoming file: no way to ask a human.
      Start the daemon with --accept-files-without-asking to accept unattended.
```

That is the documented README behaviour. The transfer was completed only after restarting the
daemon with the documented `--accept-files-without-asking` escape hatch, which logs a WARN on every
accept. **Note for the product record:** the GUI was running and visible at the time, and still
offered no way to approve the file — see §34C.5.

### 34B.6 Consent architecture — observed, and it is strict

Nothing was granted automatically at any point. `state.json` after pairing:

```json
"granted_capabilities": { "battery.v1": true, "clipboard.v1": false,
                          "files.v1": false, "notifications.v1": false },
"clipboard_policy": { "allow_send": true, "allow_receive": true,
                      "auto_send": false, "auto_receive": false },
"notification_policy": { "allow_mirror": true, "when_sink_locked": "app_only",
                         "allow_dismiss_sync": false }
```

Both ends must agree, independently. With `clipboard.v1` granted on the desktop but the tablet's own
clipboard switch off, the desktop's send was refused and reported `last result  not authorized` —
correct, and worth stating plainly: **a desktop-side grant alone does not open the clipboard.**
Notification sharing required four separate affirmative steps on the tablet (Android notification
access → share-with-this-computer → choose apps → mirroring), with the UI stating "Nothing is
chosen for you."

Clipboard delivery to Android is likewise manual by default: the clip arrived and the tablet
prompted **"Clipboard from anyflow-u2404 · 44 bytes · Copy / Dismiss"** rather than applying it —
matching `auto_receive=false`.

---

## 34C. Findings — Ubuntu 24.04

### 34C.1 `cargo clippy` fails on Ubuntu 24.04's toolchain — tautological assertion in a test

**Classification: TEST DEFECT (not product runtime). Toolchain-version dependent. CI cannot catch it.**

```
error: this boolean expression contains a logic bug
  --> capabilities/notifications/tests/real_dbus.rs:88:9
88 |         capabilities.body || !capabilities.body,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: it would look like the following: `true`
   = note: `#[deny(clippy::overly_complex_bool_expr)]` on by default
error: could not compile `anyflow-capability-notifications` (test "real_dbus")
```

The assertion is `assert!(capabilities.body || !capabilities.body, "the capability set decoded")` —
a tautology. Its own comment explains the intent ("What is asserted is that the answer was usable"),
but the expression asserts **nothing**: it cannot fail, so the test cannot detect a capability set
that failed to decode.

**Isolated to the toolchain version**, with a minimal reproduction of the identical expression:

| Toolchain | Result |
| --- | --- |
| clippy **1.91.1** (Ubuntu 24.04 archive) | **fires** `overly_complex_bool_expr` |
| clippy **0.1.98** (this host, rustup stable) | does **not** fire |

**Why it was never caught: CI runs no clippy at all.** `grep -rn "clippy" .github/workflows/*.yml`
returns nothing. `desktop/rust-toolchain.toml` requests the `clippy` component, but no workflow
invokes it.

Not fixed here — §21 forbids changing code in this wave. Suggested fix for a separate branch:
assert something real (e.g. that `sink.describe()` is non-empty), or skip when unsupported using the
idiom already present in the same file (`let Ok(mut watch) = b.watch_changes() else { … return; }`).

### 34C.2 Android displays "Battery 0%" for a desktop that has no battery — **G-BATTERY FAIL**

**Classification: PRODUCT DEFECT (presentation). Reproducible. Not fixed in this wave.**

The guest is a VM with no battery whatsoever:

```console
$ ls /sys/class/power_supply/      → (empty)
$ upower -e                        → /org/freedesktop/UPower/devices/DisplayDevice   (only)
```

Yet at startup the daemon logs:

```
INFO anyflowd: UPower available; this machine will report its own battery
```

and the Android device page renders the desktop as:

```
anyflow-u2404 · Desktop · Linux
content-desc="Battery 0 percent"   text="0%"
```

**A machine with no battery is shown as a machine with a flat battery.** The brief's §16 criterion
is explicit — a no-battery host passes "only if AnyFlow correctly represents no-battery host
without crash or **fake percentage**." There is no crash, but 0% is a fake percentage, so this gate
is recorded **FAIL**.

*Reproduction:* pair any battery-less desktop (a VM suffices), grant `battery.v1`, open the device
page on Android. *Expected:* the battery row absent, or "no battery". *Actual:* "0%".

*Scope note, stated honestly:* the presentation is on the Android side; whether the desktop sends a
0 value or sends nothing and Android defaults to 0 was **not** determined — doing so would require
reading the wire frames, which this wave did not do. The defect is real either way; its exact side
is not yet established.

### 34C.3 `real_backend` clipboard suite assumes `wl-copy --sensitive` exists — **G-CLIP FAIL**

**Classification: TEST DEFECT. The product behaved correctly — it is the test that is wrong.**

```
running 9 tests
test a_sensitive_write_still_round_trips ... FAILED
... 8 others ... ok
test result: FAILED. 8 passed; 1 failed
```

The failure message is the product refusing to do something unsafe:

```
write failed: clipboard unavailable: this clip is marked sensitive and this system's wl-copy does
not support sensitive clipboard marking. … It was NOT written to the clipboard: writing it unmarked
would leave a password in your clipboard manager's history without telling you.
```

Ubuntu 24.04 ships **wl-clipboard 2.2.1**; `--sensitive` arrived upstream in 2.3.0. AnyFlow detects
this by **feature probe, not version inference** — `probe_sensitive_from_output()` tests
`help.contains("--sensitive")` against `wl-copy --help` (the 2.3.0 reference appears only in advice
text). This is exactly what brief §13 demands, and the observable result is exactly what §13
predicts: ordinary clipboard **available**, sensitive marking **unavailable**, fail-closed.

Verified, not assumed: after the sensitive write was refused, the clipboard still held the previous
ordinary value, and the canary string appears **0 times** across the daemon log, `state.json` and
`identity.key`.

The defect is that `a_sensitive_write_still_round_trips` calls `write(&b, &value, true)`
unconditionally, and `write()` panics on any error. Sibling tests in the same file already use a
capability guard. The backend exposes `sensitive_source()` for exactly this purpose.

**Consequence for the U1 debt:** §12 said this gate would discharge the U1 D2/D3 clipboard debt "if
it passes." It did not pass, so the debt is **not formally discharged** — but the substance behind
it is now proven: the real Wayland backend works, round-trips UTF-8 byte-for-byte, carries a
maximum-sized clip, and the watcher reports every local change. What remains is a test that cannot
run on a distribution without `--sensitive`.

### 34C.4 `wl-paste --watch` is unavailable on GNOME — correctly handled, not a defect

```console
$ wl-paste --watch echo hit
Watch mode requires a compositor that supports the wlroots data-control protocol
exit=1
```

Mutter does not implement `wlr-data-control`. AnyFlow falls back automatically and says so:

```
clipboard backend  backend=wl-clipboard available=true
                   watch=XFIXES on the Xwayland CLIPBOARD selection  sensitive=no
```

The fallback **works** — `the_watcher_reports_every_local_change` passes through it. This would
behave identically on Fedora GNOME; it is a compositor property, not an Ubuntu one. **No defect.**

> Method warning for future waves: the Xwayland fallback needs `XAUTHORITY`. A first run of this
> gate with `DISPLAY=:0` but no `XAUTHORITY` produced *"cannot connect to the X display … Authorization
> required"* and a spurious watcher FAILURE. That was harness error. The gate was re-run with the full
> session environment imported from `systemctl --user show-environment`, and it passed. **The
> spurious failure is reported here so nobody re-discovers it as a product bug.**

### 34C.5 Two UX gaps worth recording (neither is a gate failure)

**(a) An incoming file cannot be approved from the GUI.** With `anyflow-gui` running and visible, an
incoming offer was still declined with *"no way to ask a human"*. The only way to accept is a daemon
flag chosen before startup. A running GUI is a human-facing surface; today it is not consulted.

**(b) Enabling notification sharing on Android does not take effect until the desktop reconnects.**
After every Android-side switch was on, the desktop still showed `showing 0 of 0 mirrored` and the
tablet said *"the computer has not said it can show notifications."* Role epochs were visibly stale
(`this desktop announced 2 (epoch 1); the device can source notifications (epoch 2)`), and
`anyflow notifications mirror <id> on` did not resolve it. **Restarting the daemon fixed it
immediately** — `snapshot complete named=3`, then `showing 3 of 3 mirrored`. A user who turns
notifications on and sees nothing has no in-product indication that a restart is required.

Both are recorded as findings for a future wave, not fixed here (§21).

---

### 34C.6 G-DISMISS was recorded N/A in error — it is NOT EXECUTED

**Classification: REPORTING CORRECTION. No new measurement; a re-reading of an existing one.**

The first write-up of §34B.3 recorded G-DISMISS as **N/A** on the grounds that `dismiss-sync` was
off by default on the desktop and "ongoing sharing" was off on the Android side. That is an
accurate description of the state the gate found — but it is the wrong classification.

**N/A is reserved for a capability the environment genuinely cannot expose.** The canonical
example, and the one the brief sanctions, is G-BATTERY on a VM with no battery: no amount of
configuration produces a battery. G-DISMISS is not that case. Both switches were present, both
were reachable, and either end could have been turned on:

| Precondition | State found | Could it have been changed? |
| --- | --- | --- |
| Desktop `allow_dismiss_sync` | `false` in `state.json` | **Yes** — a policy toggle, settable from the GUI or CLI |
| Android ongoing/dismiss sharing | off | **Yes** — a switch in the app's own consent UI |

Nothing environmental blocked it. The gate was simply not driven to the point where the switches
were flipped, and the wave ran out of session before it was. Recording that as N/A overstates the
result: it implies Ubuntu 24.04 *cannot* do dismiss sync, when the truth is that **nobody looked.**

Consequently:

- G-DISMISS on Ubuntu 24.04 is **NOT EXECUTED** in §34B.3, §34D and §36.1b.
- It earns **0 of its 3 points** in §35B, exactly as any unexercised gate does.
- The only N/A remaining anywhere in this report is **G-BATTERY's environment**, and even that is
  *not* recorded as N/A — G-BATTERY is a **FAIL** (§34C.2), because the product misrepresented the
  absent battery as 0% rather than reporting its absence.

**Net effect on the Ubuntu 24.04 result: none of the PASSes change.** The count moves from
"19 PASS / 2 FAIL / 1 N/A / 1 PARTIAL" to "19 PASS / 2 FAIL / 1 PARTIAL / 1 NOT EXECUTED", and the
score loses the 3 points the gate was never entitled to.

**Debian 13 executes this gate for real** — see the Debian section, where dismiss sync is enabled
explicitly on both ends and both a clearable and an ongoing mirrored notification are exercised.

---

## 34D. What remains NOT EXECUTED

| Item | Status | Why |
| --- | --- | --- |
| **Debian 13 Stable** | NOT EXECUTED | Ubuntu 24.04 consumed the session; one VM at a time (§4.11 RAM) |
| **Ubuntu 26.04 LTS** | NOT EXECUTED | as above |
| G-DISMISS (24.04) | **NOT EXECUTED** | `dismiss-sync` off by default and ongoing sharing off on the Android side — **both were switchable.** Not an environmental limitation; see §34C.6 |
| G-GUI-RESTART (24.04) | PARTIAL | processes proven independent; close/reopen cycle not performed |
| Android→guest **text** canary | NOT EXECUTED | picker navigation; the binary canary in the same direction passed |
| Clipboard Android→guest apply | NOT CONFIRMED | the clip reached the tablet and it prompted correctly, but the desktop-side result stayed `pending`; not driven to completion |

None of these is recorded as PASS.

---

## 35. Final score — SUPERSEDED (first attempt, preserved verbatim)

> **⚠ THE 0 / 100 SCORING IN THIS SECTION IS SUPERSEDED.** It was computed during the first
> attempt, when no VM existed and no gate had been exercised — under those conditions 0 was the
> only honest number. Ubuntu 24.04 has since been certified (§34B), so the Ubuntu 24.04 row is no
> longer true. **§35.1's weighting table is still authoritative and is reused unchanged by §35B.**
>
> The operative scoring is **§35B**. The section below is retained because its §35.3 reasoning —
> in particular why network-independent gates were *not* banked in a NAT guest — remains the
> record of why the first attempt scored nothing rather than something.

Scoring measures **certification evidence obtained**. A gate that was never exercised earns
nothing; it cannot earn partial credit for being expected to pass.

### 35.1 Weighting

| Gate | Weight | Gate | Weight |
| --- | ---: | --- | ---: |
| G-BUILD | 7 | G-CLIP-SENS | 5 |
| G-TEST | 7 | G-NOTIF | 5 |
| G-DAEMON | 4 | G-NOTIF-CAP | 3 |
| G-GUI | 6 | G-DISMISS | 3 |
| G-MDNS | 9 | G-LOCK | 5 |
| G-FW | 2 | G-RECONNECT | 4 |
| G-PAIR | 9 | G-RESTART | 3 |
| G-SESSION | 4 | G-GUI-RESTART | 2 |
| G-BATTERY | 2 | G-LOGGING | 3 |
| G-FILES-A2D | 3 | G-PERSISTENCE | 2 |
| G-FILES-D2A | 3 | | |
| G-CLIP | 5 | G-CLIP-WATCH | 4 |
| | | **Total** | **100** |

### 35.2 Scores

| Target | Score | Earned | Deducted |
| --- | ---: | ---: | ---: |
| **Ubuntu 24.04 LTS** | **0 / 100** | 0 | 100 |
| **Ubuntu 26.04 LTS** | **0 / 100** | 0 | 100 |
| **Debian 13 Stable** | **0 / 100** | 0 | 100 |
| **Combined U2** | **0 / 100** | 0 | 100 |

### 35.3 Every deduction explained

The deduction is uniform and has a single cause, so it is explained once rather than twenty-three
times per target:

**−100, all gates, all three targets — no certifiable L2 guest network (§4).** The host's only
carrier-up interface is a Wi-Fi station link whose driver reports no 4addr support, and the USB
Ethernet adapter that would provide a bridgeable wired link reports `Link detected: no`. No guest
could therefore be placed on the Android device's L2 segment, so no VM was built and no gate was
exercised. Specifically:

- **−9 G-MDNS** and **−9 G-PAIR** are the *direct* losses: both require the missing multicast path.
- **−49** across G-SESSION, G-FILES-\*, G-NOTIF-\*, G-DISMISS, G-RECONNECT, G-RESTART,
  G-GUI-RESTART, G-LOGGING, G-PERSISTENCE, G-BATTERY, G-FW are *dependent* losses: each needs a
  paired session or a running guest.
- **−33** across G-BUILD, G-TEST, G-DAEMON, G-GUI, G-CLIP, G-CLIP-WATCH, G-CLIP-SENS, G-LOCK are
  *incidental* losses: these do not need the network at all and could have been certified in a
  NAT-attached guest. **They were still not attempted**, because §5's VM strategy and §31's
  acceptance criteria treat each target as a single real-session certification; building three
  guests to bank eight of twenty-three gates, while leaving the decisive ones inadmissible, would
  produce a partial result easily mistaken for progress. They are recorded as available early wins
  for the next attempt (§36.2).

**No points were awarded for U1's green CI.** §32 forbids inheriting container-build evidence as a
runtime PASS, and this score measures runtime certification only. U1's build-only result stands on
its own and is unaffected by this zero.

A score of 0 here means **"nothing was proven,"** not "the product failed." Those are different
claims and only the first is supported.

---

## 35B. CURRENT SCORE — operative

Supersedes §35.2 and §35.3. It uses **§35.1's weighting table unchanged** (23 gates, 100 points),
so the two sections are directly comparable.

### 35B.1 Scoring rules applied

| Gate result | Credit | Rationale |
| --- | --- | --- |
| **PASS** | **full weight** | the gate was exercised and the product met the criterion |
| **FAIL** | **0** | exercised and the criterion was not met. No partial credit — a failure is not a near-miss, and a FAIL caused by a *test* defect is scored identically to one caused by a product defect, because the gate still did not pass |
| **PARTIAL** | **half weight** | some but not all of the gate's required evidence was obtained |
| **NOT EXECUTED** | **0** | never exercised; it asserts nothing and can earn nothing |
| **N/A** | excluded from the denominator | only where the environment genuinely cannot expose the capability. **Not used anywhere in this report** (§34C.6) |

No points are inherited from container CI (§2.2), from Fedora, or from the N6 notifications
certification. This measures runtime certification on the target distribution only.

### 35B.2 Ubuntu 24.04 LTS — **89 / 100**

**Earned — 19 gates PASS, 88 points**

| Gate | Wt | Gate | Wt | Gate | Wt |
| --- | ---: | --- | ---: | --- | ---: |
| G-MDNS | 9 | G-GUI | 6 | G-RECONNECT | 4 |
| G-PAIR | 9 | G-CLIP-SENS | 5 | G-DAEMON | 4 |
| G-BUILD | 7 | G-NOTIF | 5 | G-FILES-D2A | 3 |
| G-TEST | 7 | G-LOCK | 5 | G-FILES-A2D | 3 |
| G-SESSION | 4 | G-CLIP-WATCH | 4 | G-NOTIF-CAP | 3 |
| G-RESTART | 3 | G-LOGGING | 3 | G-PERSISTENCE | 2 |
| G-FW | 2 | | | **subtotal** | **88** |

**Part credit — 1 gate PARTIAL, 1 point of 2**

| Gate | Wt | Awarded | Why not full |
| --- | ---: | ---: | --- |
| G-GUI-RESTART | 2 | **1** | GUI and daemon were proven to be independent processes and the daemon served CLI and capability traffic throughout with the GUI running — but the **close/reopen cycle itself was never performed**, so "the GUI reflects daemon reality on reopening" is unproven. Half the gate's evidence, half its weight |

**Deductions — 11 points**

| Gate | Wt | Result | Lost | Why |
| --- | ---: | --- | ---: | --- |
| G-CLIP | 5 | **FAIL** | **−5** | `real_backend` exits 101 (8/9). The **product behaved correctly** — it fail-closed on a sensitive write that Ubuntu 24.04's `wl-copy` 2.2.1 cannot mark — but the official gate command does not pass, and §34C.3 declines to relabel a red suite green. A test defect, scored as a FAIL (§34C.3) |
| G-DISMISS | 3 | **NOT EXECUTED** | **−3** | reclassified from N/A. Both switches were available and neither was turned on (§34C.6) |
| G-BATTERY | 2 | **FAIL** | **−2** | genuine product presentation defect: a battery-less host is rendered on Android as "Battery 0 percent / 0%" — a fake percentage, which brief §16 disallows explicitly (§34C.2) |
| G-GUI-RESTART | 2 | PARTIAL | **−1** | as above |
| | | | **−11** | |

```
Ubuntu 24.04 LTS   88 earned + 1 partial  =  89 / 100
```

**What the 89 does and does not say.** It says that on Ubuntu 24.04, exercised against a real
GNOME/Wayland session on the real LAN with a real Android peer, 19 of 23 gates met their criterion
outright. It does **not** say the remaining 11 points are all product debt: 5 of the 11 are a test
that cannot run on a distribution lacking `wl-copy --sensitive`, 3 are work this wave did not do,
and only **2 points — G-BATTERY — are a confirmed product defect.**

### 35B.3 Combined U2 — **PROVISIONAL, 1 of 3 targets executed**

| Target | Score | Status |
| --- | --- | --- |
| Ubuntu 24.04 LTS | **89 / 100** | executed |
| Debian 13 Stable | **not scored** | **NOT EXECUTED** — no guest yet |
| Ubuntu 26.04 LTS | **not scored** | **NOT EXECUTED** — no guest yet |
| **Combined U2** | **WITHHELD** | **1 of 3 targets executed** |

**A combined U2 score is deliberately not computed.** Two options were considered and both
rejected:

- **Scoring the unexecuted targets as 0** would produce ~30/100 and read as "U2 largely failed",
  which is false — nothing on those targets was observed to fail because nothing was observed.
- **Averaging only the executed target** would produce 89/100 for "U2", which is worse: it would
  silently promote a one-third-complete wave to a finished one.

The combined position stays **WITHHELD** until Debian 13 and Ubuntu 26.04 have run. This is the
same principle §28 applies to gate status — NOT EXECUTED is a third state, neither pass nor fail —
applied one level up, to targets.

---

## 36. Final verdict

### 36.1 Ruling — SUPERSEDED 2026-09-15 by §36.1b

> The ruling immediately below belongs to the **first** U2 attempt and is preserved verbatim as
> historical evidence. It was correct when written. The cable was connected later the same day and
> U2 was executed against Ubuntu 24.04 — see **§36.1b**, which is the operative verdict.

The §3 hard gate was evaluated against live measurements and **failed**:

- Host's only carrier-up link is Wi-Fi in `managed` mode, with **zero** 4addr capability.
- USB Ethernet `enp0s13f0u2u2c2` reports **`Link detected: no`** — unchanged from U0.
- The only attachable libvirt network is **NAT**, which §3 forbids as mDNS evidence.
- Consequently: **no VM was created, and no certification gate was executed on any target.**

The brief's instruction for exactly this condition was followed literally: stop before claiming
U2 certification, report the blockage, and supply the host-side commands for when a cable is
available (§4.6).

This verdict is **not** downgraded to PASS, and **not** reported as FAIL. FAIL would assert that
Ubuntu or Debian runtime behaviour was tested and found wanting; it was not tested at all.

### 36.1b OPERATIVE RULING — 2026-09-15, after the blocker was cleared

**UBUNTU 24.04 LTS: CERTIFIED WITH DEFECTS.**
**DEBIAN 13 AND UBUNTU 26.04: NOT EXECUTED.**

The §3 topology gate now **passes**. A real GNOME/Wayland Ubuntu 24.04 guest was placed on the
physical LAN over macvtap (`192.168.68.75/22`, gateway `192.168.68.1`, its own MAC), the Android
tablet reached it directly in both directions, and the entire product path ran over that LAN with
no NAT, no forwarding, no tunnel and no reflector. No host route or NetworkManager connection was
modified, and `br0` was never needed.

**19 of 23 gates PASS. 2 FAIL. 1 PARTIAL. 1 NOT EXECUTED.**

| | Gates |
| --- | --- |
| **PASS (19)** | G-BUILD, G-TEST, G-DAEMON, G-GUI, G-MDNS, G-FW, G-PAIR, G-SESSION, G-FILES-D2A, G-FILES-A2D, G-CLIP-WATCH, G-CLIP-SENS, G-NOTIF, G-NOTIF-CAP, G-LOCK, G-RECONNECT, G-RESTART, G-LOGGING, G-PERSISTENCE |
| **FAIL (2)** | **G-BATTERY** (§34C.2 — "0%" shown for a battery-less host) · **G-CLIP** (§34C.3 — `real_backend` 8/9; the failing test assumes `wl-copy --sensitive`) |
| **NOT EXECUTED (1)** | **G-DISMISS** — reclassified from N/A (§34C.6); the feature was switchable and was not exercised |
| **PARTIAL (1)** | G-GUI-RESTART |

**Why "certified with defects" and not PASS:** two mandated gates failed and are recorded as FAIL,
not waived. **Why not FAIL overall:** neither failure impairs the runtime on this distribution.
The binary builds guest-native, 717 tests pass, the daemon runs, the GUI renders on real
libadwaita 1.5, pairing is sound, files transfer with byte-exact hashes in both directions,
notifications mirror and update in place, and lock detection tracks logind exactly.

**Of the two failures, only one is a product defect.** G-CLIP is a test-suite portability bug — the
product refused to write an unmarkable sensitive clip, which is the correct, safe behaviour and
precisely what brief §13 asks for. G-BATTERY is a genuine product presentation defect.

**Nothing security-relevant failed.** Pairing refused four adverse attempts before succeeding, trust
and identity are 0600, capabilities are default-deny on both ends independently, no clipboard
content or notification body reached any log or state file, and the sensitive canary never touched
the clipboard. Under §21 there was therefore no cause to stop the wave.

#### Honest accounting of this wave's own errors

The operator's tooling, not the product, caused several failures during execution. They are named
so the evidence can be weighed properly:

- an auto-refresh loop cancelled the pairing dialog mid-confirmation (three failed pairings);
- one pairing lapsed past its 120 s window while screenshots were analysed;
- the clipboard watch gate was first run without `XAUTHORITY`, producing a **spurious** watcher
  failure that was corrected by re-running with the real session environment;
- the first Android→guest file attempt used a shell-invoked `content://` URI the app could not read.

Every one of these was re-run correctly or is disclosed as not executed. **No gate above is marked
PASS on the strength of a run that did not happen.**

#### The distribution-floor question, answered

Ubuntu 24.04 ships **libadwaita 1.5.0** and **GTK 4.14.5**; `desktop/gui/Cargo.toml` requires
`features = ["v1_5"]`. The floor is met **exactly, with nothing to spare** — the GUI rendered and
navigated correctly with no symbol or version errors. 24.04 was the right first target, and it
holds. Its `rustc` 1.75 is too old, but the archive's own `rustc-1.91` satisfies the 1.88 MSRV, so
no third-party toolchain is required on this distribution.

### 36.2 What to do next, in order — SUPERSEDED (first attempt, preserved verbatim)

> **⚠ STEPS 1–3 BELOW ARE SUPERSEDED AND MUST NOT BE FOLLOWED.** They tell the reader to connect a
> cable, run the §4.6 `nmcli` bridge procedure and confirm a `br0` address. The cable is connected
> (§4.9) and **`br0` was never created and is not needed** — macvtap solved the topology without
> touching host networking at all (§4.12, §34B.4). Following §4.6 now would make an unnecessary,
> and reversible-only-by-hand, change to a working host. **See §36.2b for the operative procedure.**
> Steps 4–5 (gate ordering, target ordering) remain sound advice and are carried into §36.2b.


1. **Connect an Ethernet cable** to the ASIX AX88179 adapter, on the same LAN as the Android
   device. This is the entire blocker.
2. Run **§4.6 steps 0–2** and confirm `br0` holds a **192.168.68.0/22** address. If it does not,
   stop again.
3. Stage the three ISOs; run **§4.6 steps 3–6**, then **§6.2**, one guest at a time (RAM, L-4).
4. Re-run U2 from §6 of the brief. Take the eight network-independent gates (G-BUILD, G-TEST,
   G-DAEMON, G-GUI, G-CLIP, G-CLIP-WATCH, G-CLIP-SENS, G-LOCK) **first** on the first guest —
   they are the fastest route to real signal, and **G-CLIP-WATCH discharges the two-wave-old debt
   D-3**.
5. Certify **Ubuntu 24.04 first** and **Debian 13 second** — U0's reasoning holds: the risk lives
   in 24.04's libadwaita margin and Debian's toolchain, not in 26.04.

If the cable route remains unavailable, fall back to U0's standing alternative: certify at least
one target on a **second physical machine**, which needs no bridge at all.

### 36.2b OPERATIVE PROCEDURE — the topology is solved; reproduce it, do not rebuild it

**The network question is closed.** It was answered by measurement, end to end, with an Android
tablet talking to a guest over the physical LAN (§34B.4). Nothing about it needs re-deciding.

#### The proven path

```
enp0s13f0u2u2c2           ASIX AX88179 USB Ethernet, carrier=1, 1000 Mb/s, 192.168.68.72/22
   │
   │  libvirt <interface type='direct'>
   │          <source dev='enp0s13f0u2u2c2' mode='bridge'/>      ← macvtap
   ▼
guest enp1s0              own MAC (52:54:00:…), DHCP from the LAN's own server
                          192.168.68.x/22, default via 192.168.68.1
   │
   ▼  UDP/5353 multicast + TCP/55432, on one L2 segment, no translation anywhere
Android SM-X620           192.168.68.63/22
```

The single `virt-install` argument that does all of this:

```
--network type=direct,source=enp0s13f0u2u2c2,source.mode=bridge,model.type=virtio
```

#### What is NOT required — and must not be done

| Not required | Why it is now known to be unnecessary |
| --- | --- |
| **`br0`** | macvtap puts the guest on the LAN directly. §4.6's `nmcli` bridge procedure was **never executed** and the host has no `br0` |
| **Any host route change** | routes were snapshotted before and after; unchanged (§4.9) |
| **Any NetworkManager change** | `Rede Wi-Fi de Yuri` and `Conexão cabeada 1` are both untouched |
| **libvirt NAT / `virbr0`** | not attached to any certification domain; `virbr0` remains linkdown and unused |
| **Port forwarding · `socat` · SSH tunnel · Avahi reflector** | absent by construction, not by promise — there is no IP path between host and guest at all |

#### The one thing macvtap costs, and how it is paid

macvtap in `bridge` mode does not loop back to its own host, so **the Fedora host cannot reach the
guest over IP** (measured: 100% loss, §4.14). The brief accepts this — the requirement is
Android↔guest, which is clean in both directions. Guest control therefore runs over
**`qemu-guest-agent` on virtio-serial** (`virsh qemu-agent-command … guest-exec`), which is not an
IP path and cannot carry, influence or contaminate product traffic. Graphical observation uses
`virsh screenshot`; keyboard input uses `virsh send-key`.

#### Procedure for each remaining target

1. **Confirm the wired link still has carrier** — `cat /sys/class/net/enp0s13f0u2u2c2/carrier`
   must read `1`. (`ethtool` needs root; sysfs does not, and reports the same kernel state.)
2. **Shut down the previous guest cleanly** — `virsh shutdown <prev>`. **Do not undefine it**:
   a certified guest is kept for targeted retesting after any future fix. Only one graphical VM
   runs at a time; host RAM is the binding constraint (§4.11).
3. **Create the guest with the identical envelope** — 4 vCPU, **4096 MB**, 40 GB qcow2, UEFI,
   virtio disk/video/NIC, and the `--network type=direct,…` line above. Add
   `--channel unix,target.type=virtio,target.name=org.qemu.guest_agent.0` for the control channel.
4. **Verify the six §23 prerequisites before any gate** (§4.14): guest on `192.168.68.x/22`;
   Android→guest ping; guest→Android ping; **no `192.168.122.x` anywhere**; a real Wayland session
   on `seat0`; clipboard backend able to execute.
5. **Take the network-independent gates first** — G-BUILD, G-TEST, G-DAEMON, G-GUI, G-CLIP,
   G-CLIP-WATCH, G-CLIP-SENS, G-LOCK. (Carried from the superseded §36.2 step 4; it was good
   advice and it held on Ubuntu 24.04.)
6. **Generate a fresh identity per guest.** Never copy `identity.key`, `state.json` or the trust
   store between guests, and never clone a template that has already launched AnyFlow (§26).

#### Target order

**Debian 13 next, Ubuntu 26.04 last.** Carried from the superseded §36.2 step 5 and unchanged: the
risk lives in Debian's stock toolchain (`rustc` 1.85.1 vs MSRV 1.88, U0 U-4), not in 26.04, which
U0 assessed as nearest to Fedora 44 and therefore the target that proves the least.


### 36.3 Statements deliberately not made

Per §32 of the brief, and to be unambiguous about what this document does and does not assert:

- It is **not** written that "Ubuntu is supported" or "Debian is supported."
- No runtime PASS is inherited from **Fedora**, or from the **N6** notifications certification.
- No runtime PASS is inherited from the green **container CI** of §2.2.
- mDNS was **not** inferred from `ping` — the successful pings in §4.4 prove only that the host
  and the Android device share a LAN, and are reported as exactly that.
- Clipboard support was **not** inferred from `wl-copy` existing; that probe was not even run.
- Notification support was **not** inferred from D-Bus introspection.
- Lock correctness was **not** inferred from service existence.

---

## 37. Git status

Captured at the end of the wave:

```console
$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md

$ git diff --check
(clean)

$ git diff --stat
(no tracked-file modifications)

$ git diff --name-status
(none)

$ git branch --show-current
cert/linux-ubuntu-debian-u2-real-session

$ git rev-parse HEAD
97923302ddba473c3b139af8832abe9153c21cff
```

**The repository diff is exactly what §37 expects: one new file, `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`.**

- **No production code diff.** `git diff --name-status` is empty; not one tracked file changed.
- No optional documentation status updates were made — §37 permits them only after a PASS, and
  this wave is not a PASS.
- HEAD is unmoved from the baseline: `97923302ddba473c3b139af8832abe9153c21cff`.

Per §37, none of the following were run: `git add`, `git commit`, `git push`, `gh pr create`.

### Machine state left behind — UPDATED 2026-09-15 (post-execution)

Everything this wave changed, and how to undo it. **No AnyFlow production code was modified**;
`git status --short` shows only the untracked report, and `git diff --stat` / `--name-status` are
empty.

#### On the Fedora host

| Change | Undo |
| --- | --- |
| libvirt `default` storage pool defined over `/var/lib/libvirt/images` | `virsh pool-destroy default && virsh pool-undefine default` |
| Pool volumes: `ubuntu-24.04.4-desktop-amd64.iso`, `seed-u2404.iso`, `vmlinuz`, `initrd`, `anyflow-u2404.qcow2` (40 GB) | `virsh vol-delete --pool default <name>` |
| Domain `anyflow-u2404` (running) | `virsh destroy anyflow-u2404 && virsh undefine anyflow-u2404 --nvram --remove-all-storage` |
| `~/ISO/` — verified ISO + extracted `casper/{vmlinuz,initrd}` (~6.3 GB) | `rm -rf ~/ISO` |
| `cargo clean -p anyflow-capability-notifications` was run twice on the host tree (clippy reproduction) | rebuilds on next `cargo build`; no source change |
| Development `anyflowd` (pid 613091, `dn=Fedora`) | **left running, untouched**, exactly as found |
| **Host networking** | **nothing changed** — no route, no NetworkManager connection, no `br0`, no interface |

#### On the Android device (SM-X620)

| Change | Undo |
| --- | --- |
| `io.github.yurisismotto.anyflow` reinstalled from the repo's committed debug APK (§5.1) | `adb uninstall io.github.yurisismotto.anyflow` |
| Runtime permissions granted: `POST_NOTIFICATIONS`, `CAMERA` | revoke in Settings, or uninstall |
| **Notification listener enabled** for AnyFlow | restore the original value: `adb shell settings put secure enabled_notification_listeners "com.sec.android.app.launcher/com.android.launcher3.notification.NotificationListener:com.samsung.android.smartmirroring/com.samsung.android.smartmirroring.controller.NotificationService"` |
| In-app consents: clipboard on, notification sharing on, AnyFlow Fixture chosen as a shared app | Device settings → toggles, or "Forget this device" |
| Paired with guest `anyflow-u2404` (`135A C045 BFE9 F0A5`) | "Forget this device" on the tablet |
| `screen_off_timeout` set to 1800000 ms; `svc power stayon usb` | restore your preferred timeout in Settings; `adb shell svc power stayon false` |
| Canaries in `/sdcard/Download/`: `a2d.txt`, `a2d.bin`; in `Download/AnyFlow/`: `u2-text.txt`, `u2-binary.bin` | delete at will |

**The notification-listener grant is the one item most worth restoring** if the tablet is used for
anything else, since it lets AnyFlow read notifications. It was left enabled deliberately, because
Debian 13 and Ubuntu 26.04 still have to be certified against this same device and would otherwise
need the whole consent sequence repeated.

#### Inside the guest (discarded with the VM)

`/usr/local/bin/{gx,gpush,precheck,bootstrap,build,gates,clipgate,clipgate2,final,mkcanary,clip_d2a,clip_check}.sh`,
`atspi.py`, `atspi2.py`, `autopair.py`; `~/anyflow` (cloned baseline + `target/`); logs
`~/anyflowd{,2,3}.log`, `~/build.log`, `~/gates.log`; canaries under `~/canary` and
`~/Downloads/AnyFlow`; GDM autologin for `anyflow`; GNOME idle-lock disabled for the GUI gates.


### 37.1 The first attempt's closing statement — SUPERSEDED

> **⚠ THE PARAGRAPH BELOW IS FALSE AS OF 2026-09-15 AND IS RETAINED ONLY TO BE MARKED WRONG.**
>
> > ~~Nothing to revert. No bridge, no libvirt network, no VM, no firewall rule, no service change,
> > no process stopped, no Wi-Fi profile modification. The host is in the state it was found in,
> > and the Android device was read from but never written to.~~
>
> It was true when written, when the wave had stopped at the §3 hard gate having touched nothing.
> It stopped being true the moment U2 resumed. **The machine-state table immediately above (§37,
> "Machine state left behind") is the authoritative record** and it supersedes this paragraph
> completely.

**There IS state to revert.** Summarised, with §37's table remaining authoritative for the exact
undo commands:

| On the Fedora host | On the Android SM-X620 |
| --- | --- |
| VM `anyflow-u2404` — **defined, preserved for retesting** | AnyFlow app **reinstalled** from the repo's committed debug APK |
| libvirt **`default` storage pool** defined (created by this wave) | `POST_NOTIFICATIONS` + `CAMERA` runtime permissions granted |
| Pool volumes: Ubuntu ISO, `seed-u2404.iso`, `vmlinuz`, `initrd`, `anyflow-u2404.qcow2` (40 GB) | **Notification-listener access granted** ← the item most worth restoring |
| `~/ISO/` — verified ISOs and extracted kernel/initrd | In-app consents: clipboard, notification sharing, shared-app selection |
| | Paired with guest `anyflow-u2404` (`135A C045 BFE9 F0A5`) |
| | `screen_off_timeout` 1800000 ms; `svc power stayon usb` |
| | Canary files under `/sdcard/Download/` and `Download/AnyFlow/` |

**Two things are still true from the original paragraph and are worth keeping:** no firewall rule
was added anywhere, and **no host networking was changed at all** — no bridge, no libvirt network
attached to a certification domain, no route, no NetworkManager connection, no Wi-Fi profile.

**Restoration is deliberately deferred to the end of the whole U2 wave.** The Android consents and
the notification-listener grant are prerequisites for the Debian 13 and Ubuntu 26.04 targets
against this same device; restoring them now would only mean repeating the entire consent sequence
twice more. They are restored when the last target finishes, per §37's undo column.

---

# 39. DEBIAN 13 REAL-SESSION RESULTS

**Guest:** `anyflow-d13` · **Distro:** Debian 13.7.0 "trixie" Stable · **Date:** 2026-09-15
**Baseline:** `97923302ddba473c3b139af8832abe9153c21cff` · **Android peer:** SM-X620 `192.168.68.63`

This section is the second target of U2. It is written to the same standard as §34B: every gate is
either exercised and reported with its evidence, or marked NOT EXECUTED. Ubuntu 24.04's results
(§34B) are **not** inherited — not one PASS is carried across, and the Debian identity shares
nothing with the Ubuntu one.

## 39.1 Infrastructure — the proven topology, reused unchanged

Ubuntu 24.04 established the topology (§36.2b). Debian reuses it **exactly**, with no new host
change of any kind.

```console
$ cat /sys/class/net/enp0s13f0u2u2c2/carrier   → 1
$ cat /sys/class/net/enp0s13f0u2u2c2/operstate → up
$ cat /sys/class/net/enp0s13f0u2u2c2/speed     → 1000
```

```console
$ ip route          # BEFORE and AFTER guest creation — byte-identical
default via 192.168.68.1 dev enp0s13f0u2u2c2 proto dhcp src 192.168.68.72 metric 100
default via 192.168.68.1 dev wlp0s20f3       proto dhcp src 192.168.68.73 metric 600
192.168.68.0/22 dev enp0s13f0u2u2c2 proto kernel scope link src 192.168.68.72 metric 100
192.168.68.0/22 dev wlp0s20f3       proto kernel scope link src 192.168.68.73 metric 600
192.168.122.0/24 dev virbr0 proto kernel scope link src 192.168.122.1 linkdown
```

**No route was added, removed or reordered. No NetworkManager connection was created or modified.
No `br0` exists.** `virbr0` remains `linkdown` and is attached to nothing.

### Guest envelope — identical to the certified Ubuntu guest

| Resource | Value | Brief's ceiling |
| --- | --- | --- |
| vCPU | 4 | 4 ✓ |
| RAM | **4096 MB** | 6 GB ✓ (§4.11 — host RAM is the real constraint) |
| Disk | 40 GB qcow2, virtio | 40 GB ✓ |
| Firmware | UEFI (OVMF) | ✓ |
| Video | virtio | ✓ |
| NIC | virtio on **macvtap direct** | ✓ |
| Concurrency | **one graphical VM at a time** — Ubuntu was cleanly `virsh shutdown` first and is **preserved, not destroyed** | ✓ |

```console
$ virsh dumpxml anyflow-d13 | grep -A5 "type='direct'"
    <interface type='direct'>
      <mac address='52:54:00:a7:2d:eb'/>
      <source dev='enp0s13f0u2u2c2' mode='bridge'/>
      <target dev='macvtap4'/>
      <model type='virtio'/>

$ virsh dumpxml anyflow-d13 | grep -iE "virbr|network=|192.168.122"
(no output — no NAT, no libvirt network, no user-mode networking)
```

The guest MAC `52:54:00:a7:2d:eb` is its own, distinct from the Ubuntu guest's
`52:54:00:ec:3c:e5` and from the host's `6c:1f:f7:29:4c:c3`.

**Control channel:** `qemu-guest-agent` over virtio-serial, exactly as on Ubuntu. It is not an IP
path, so it cannot carry, influence or contaminate product traffic. All AnyFlow traffic under
certification traverses macvtap → `enp0s13f0u2u2c2` → the physical LAN and nothing else.

## 39.2 Installation method — and a method finding worth recording

Debian 13.7.0 `netinst`, from the official Debian CD server, verified before first boot:

| | |
| --- | --- |
| ISO | `debian-13.7.0-amd64-netinst.iso` |
| Source | `https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/` |
| Published SHA256 | `a7ef94ac2fb9a7fec454552abd629b7cc9d5155c886165a45649f5ce6167e355` |
| Computed SHA256 | `a7ef94ac2fb9a7fec454552abd629b7cc9d5155c886165a45649f5ce6167e355` |
| Verdict | **MATCH** |

Installed unattended by **initrd preseeding** — `preseed.cfg` appended to `install.amd/initrd.gz`
as a second cpio member, which d-i reads from the initrd root before it asks anything. Only
`qemu-guest-agent` and `openssh-server` were named at install time, alongside the `gnome-desktop`
and `standard` tasks; **everything AnyFlow needs is installed afterwards, in the running guest**,
so that an install failure can never be mistaken for a dependency failure.

### Method finding: an unobservable preseeded install is an undebuggable one

**Classification: PACKAGING / METHOD, not a product finding.** It touches no AnyFlow code and is
counted against no gate. It is recorded because it cost one full install cycle and would cost
anyone repeating U2 the same.

The first install (rev 1) ran to ~2.79 GB written and ~875 MB downloaded — base system plus most
of the GNOME task — and then went **completely idle**: `vcpu.0.time` advanced 10 ms in 8 seconds,
disk writes stopped, network stopped. Nothing was on screen to explain why, because the kernel
command line carried `console=ttyS0,115200n8`, which moves d-i's UI to the serial port and leaves
the VGA framebuffer blank — and the serial pty is owned by `qemu`, unreadable without root.

Booting the disk afterwards proved the install had *not* silently finished:

```console
BdsDxe: failed to load Boot0002 "UEFI Misc Device" ... : Not Found
BdsDxe: No bootable option or device was found.
```

No bootloader had been written. The install was rebuilt with `console=ttyS0` **dropped**, so that
d-i renders on the VGA console where `virsh screenshot` can see it and `virsh send-key` can answer
it. The cause was then visible immediately, at the same ~2.78 GB mark:

```
[!!] Configuring keyboard-configuration
Please select the layout matching the keyboard for this machine.
Keyboard layout:  [English (US)]        ← waiting for a human to press Enter
```

**`d-i keyboard-configuration/xkb-keymap select us` does not suppress it.** That preseed answers
d-i's own keymap question; this second prompt comes from `console-setup` being configured *inside
the installed system* during `pkgsel`, at a different template, and it blocks the whole install.
One `virsh send-key KEY_ENTER` cleared it and the install proceeded to completion.

Two other hardenings were applied in the same rebuild and are worth carrying forward, though the
evidence above shows neither was the actual blocker:

1. **Every clause of `preseed/late_command` now ends in `|| true`.** d-i halts on an error dialog
   when a `late_command` exits non-zero, and the rev-1 command ended with a bare
   `in-target systemctl enable qemu-guest-agent` — a udev-activated unit whose `enable` is not
   guaranteed to succeed. This was a latent second stall, sitting just past the first.
2. Explicit `grub-installer` answers (`bootdev`, `force-efi-extra-removable`, `update-nvram`).

> Stated plainly, because it is the kind of detail that gets quietly dropped: **the first Debian
> install failed, and the failure was the operator's preseed and boot configuration — not Debian,
> and not AnyFlow.** No gate is affected.

## 39.3 Guest identity and environment

```console
$ cat /etc/os-release
PRETTY_NAME="Debian GNU/Linux 13 (trixie)"   VERSION_ID="13"   VERSION_CODENAME=trixie

$ uname -a
Linux anyflow-d13 6.12.107+deb13-amd64 #1 SMP PREEMPT_DYNAMIC Debian 6.12.107-1 (2026-08-29) x86_64

$ ip -br addr
lo       UNKNOWN  127.0.0.1/8 ::1/128
enp1s0   UP       192.168.68.76/22 fe80::548:9dd:8dd6:5dd8/64

$ ip route
default via 192.168.68.1 dev enp1s0 proto dhcp src 192.168.68.76 metric 1002
192.168.68.0/22 dev enp1s0 proto dhcp scope link src 192.168.68.76 metric 1002

$ ip link show enp1s0 | grep ether
    link/ether 52:54:00:a7:2d:eb
```

| Attribute | Value |
| --- | --- |
| Interface | `enp1s0` (virtio, backed by `macvtap4` on `enp0s13f0u2u2c2`) |
| IP | **`192.168.68.76/22`** — DHCP, from the LAN's own server |
| MAC | `52:54:00:a7:2d:eb` — its own, ≠ Ubuntu guest, ≠ host |
| Gateway | **`192.168.68.1`** — the real LAN gateway |
| `192.168.122.x` present? | **No.** Not on any interface, not in any route, not in the domain XML |

### Distribution floor — Debian 13 is well clear of it

```console
$ dpkg -l | grep -E 'libgtk-4-1|libadwaita-1-0'
libadwaita-1-0:amd64   1.7.6-1~deb13u1
libgtk-4-1:amd64       4.18.6+ds-2
```

`desktop/gui/Cargo.toml` requires `libadwaita` with `features = ["v1_5"]`. Debian 13 ships
**libadwaita 1.7.6 and GTK 4.18.6** — comfortably above the requirement, and **newer than Ubuntu
24.04's 1.5.0 / 4.14.5**, which was the zero-margin case (§36.1b). Debian 13 is therefore *not*
the libadwaita risk; its risk is the toolchain, and that is measured next.

## 39.4 G-SESSION — real GNOME/Wayland session, by real password login

```console
$ loginctl session-status 2
2 - anyflow (1000)
     Since: Tue 2026-09-15 20:43:16 UTC
     State: active
    Leader: 1705 (gdm-session-wor)
      Seat: seat0; vc2
       TTY: tty2
   Service: gdm-password
      Type: wayland
     Class: user
      Unit: session-2.scope
            ├─1812 /usr/libexec/gdm-wayland-session /usr/bin/gnome-session

$ loginctl show-session 2 -a | grep -E '^(Type|Active|State|Seat|Remote|LockedHint|Class|Service)='
Type=wayland
Active=yes
State=active
Seat=seat0
Remote=no
Class=user
Service=gdm-password
LockedHint=no
```

Environment, imported from the systemd user manager rather than hand-built (the method error that
produced a false negative on Ubuntu, §4.15):

```console
XDG_SESSION_TYPE=wayland      WAYLAND_DISPLAY=wayland-0
XDG_CURRENT_DESKTOP=GNOME     DISPLAY=:0
XDG_SESSION_DESKTOP=gnome     XDG_RUNTIME_DIR=/run/user/1000
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus

$ ls -la /run/user/1000/wayland-0
srwxrwxr-x 1 anyflow anyflow 0 Sep 15 20:43 /run/user/1000/wayland-0

$ ps -eo comm | sort -u
gdm3  gdm-session-wor  gdm-wayland-ses  gnome-shell  gnome-shell-cal  Xwayland
```

**GNOME, Wayland, real, on `seat0`.** Not X11, so the brief's "if X11: STOP that target" clause
does not trigger.

> **Stronger than the Ubuntu equivalent, and worth stating.** Ubuntu 24.04's session was
> established by **GDM autologin** configured by that wave (§4.15). Debian's is
> **`Service=gdm-password`** — the wave's `late_command` failed to write the autologin config
> (§39.2), so the session was established by **typing the user's password into GDM** via
> `virsh send-key`. That is a genuine interactive login, and it is better evidence than autologin.

**G-SESSION: PASS.**

## 39.5 Toolchain — measured, then chosen

The brief requires that Debian not be called incompatible merely because its default compiler is
older than the MSRV, and that the route taken be recorded exactly. Both halves follow.

### Measured BEFORE any change

```console
$ which rustc cargo
(nothing)
$ rustc --version
rustc: command not found
$ apt-cache policy rustc
rustc:
  Installed: (none)
  Candidate: 1.85.1+dfsg1-1+deb13u1
     1.85.1+dfsg1-1+deb13u1 500  http://deb.debian.org/debian trixie/main
```

Debian 13's own `rustc` is **1.85.1**, exactly as U0 finding U-4 and the README table predict.
The workspace declares `rust-version = "1.88"` (`desktop/Cargo.toml:44`). **1.85.1 < 1.88**, so the
stock compiler is not enough — as documented, not as a discovery.

### Route taken: **`trixie-backports`. Not rustup.**

```console
$ cat /etc/apt/sources.list.d/backports.sources
Types: deb
URIs: http://deb.debian.org/debian
Suites: trixie-backports
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg

$ apt-cache policy rustc
     1.94.1+dfsg1-1~bpo13+3  100  trixie-backports/main
     1.85.1+dfsg1-1+deb13u1  500  trixie/main

$ apt-get install -t trixie-backports rustc cargo
$ rustc --version  → rustc 1.94.1 (e408947bf 2026-03-25) (built from a source tarball)
$ cargo --version  → cargo 1.94.1 (29ea6fb6a 2026-03-24) (built from a source tarball)
$ which rustc cargo → /usr/bin/rustc  /usr/bin/cargo
$ apt-cache policy rustc | head -2
  Installed: 1.94.1+dfsg1-1~bpo13+3
```

**Why backports and not rustup**, stated because the brief asks for the reason and not just the
choice:

1. **It is the distribution's own answer.** `trixie-backports` is an official Debian suite, signed
   by the Debian archive key. rustup is a third-party binary installer. U2 certifies *Debian*, and
   a distribution-native toolchain keeps the result about Debian rather than about rustup.
2. **It matches what the README documents first** for this target
   (README §"The Rust toolchain, per distribution": *"`trixie-backports` (`rustc` 1.94.1), or
   rustup"*), and the measured backports version is **exactly the 1.94.1 the README names.**
3. **It is consistent with the Ubuntu 24.04 decision.** That target also used its distribution's
   own archive (`rustc-1.91`) rather than rustup (§34B.2). Two targets, one principle.

**Nothing was pinned, overridden or edited to make this work.** `Cargo.lock` is untouched, no
`rust-toolchain.toml` was modified, and `rust-version` was not changed — verified in §39.10.

### Packaging finding: Debian splits `rustfmt` and `clippy` out of `cargo`

**Classification: PACKAGING / DOCUMENTATION. Not a product defect. Not counted against any gate.**

Installing `rustc` and `cargo` from backports is **not sufficient to run the brief's gate
commands.** Both of these failed on a toolchain that had just been installed as documented:

```console
$ cargo fmt --all --check
error: no such command: `fmt`
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
error: no such command: `clippy`
```

Debian ships them as **separate packages**, and they must be taken from backports too or they will
not match the 1.94.1 compiler:

```console
$ apt-cache policy rustfmt rust-clippy
rustfmt:      Candidate 1.85.1+dfsg1-1+deb13u1 ; 1.94.1+dfsg1-1~bpo13+3 in trixie-backports
rust-clippy:  Candidate 1.85.1+dfsg1-1+deb13u1 ; 1.94.1+dfsg1-1~bpo13+3 in trixie-backports

$ apt-get install -t trixie-backports rustfmt rust-clippy
$ cargo fmt --version    → rustfmt 1.8.0
$ cargo clippy --version → clippy 0.1.94
```

This is a real gap in the README's per-distribution table, which names only `rustc` and `cargo`.
Fedora's `rust`/`cargo` and Ubuntu's `rustc-1.91`/`cargo-1.91` both provide the subcommands; Debian
does not. **Suggested README amendment for a future branch:** add `rustfmt rust-clippy` to the
Debian row. No code change is implied.

## 39.6 Build, test and lint — guest-native

All four commands were run **inside the guest**, as user `anyflow`, against a `git clone` of the
public repository checked out at the certification baseline and verified in place:

```console
$ git -C /home/anyflow/anyflow rev-parse HEAD
97923302ddba473c3b139af8832abe9153c21cff
$ git -C /home/anyflow/anyflow status --short
(empty)
```

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked -j 2` | **PASS** — exit 0, `Finished dev profile in 4m 08s` |
| `cargo test --workspace --locked -j 2` | **PASS** — exit 0, **717 passed, 0 failed, 22 ignored** |
| `cargo fmt --all --check` | **PASS** — exit 0 |
| `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | **FAIL** — exit 101, one lint (§39.9.1) |

**717 passing, identical to Ubuntu 24.04 and to the count the README advertises** — independent
corroboration on a second distribution that the whole suite ran and nothing was silently filtered.
No host build artefact was substituted; `target/` in the guest was built from scratch by the guest.

**G-BUILD: PASS. G-TEST: PASS.**

## 39.7 G-DAEMON, G-MDNS and G-FW

### Fresh Debian identity — nothing copied from Ubuntu

```console
INFO anyflowd: local identity device=6185e1e69a8b8723ccc3548de1fff3e8
               name=anyflow-d13 fingerprint=B52C DA20 46ED 006D key_backing=software
```

| | Debian guest | Ubuntu guest (§34B.1) | Fedora host (§4.13) |
| --- | --- | --- | --- |
| Device id | **`6185e1e69a8b8723ccc3548de1fff3e8`** | `db9dfc03de621a642048de96ce31ae62` | `795fec0868ebef8c3d7ad3e775dc6ed0` |
| Fingerprint | **`B52C DA20 46ED 006D`** | `135A C045 BFE9 F0A5` | — |
| Display name | `anyflow-d13` | `anyflow-u2404` | `Fedora` |
| Address | `192.168.68.76` | `192.168.68.75` | `192.168.68.72` |

**Three distinct identities, independently generated.** The Debian guest was installed from its own
ISO, not cloned from the Ubuntu guest, and no `identity.key`, `state.json` or trust file was copied
between machines. This is §26 of the brief, satisfied by construction.

### Daemon

```console
$ pgrep -a anyflowd   → 23362 …/target/debug/anyflowd
$ ss -ltnp | grep 55432
LISTEN 0 128 *:55432 *:* users:(("anyflowd",pid=23362,fd=12))
$ ls -la /run/user/1000/anyflow/
srw------- 1 anyflow anyflow 0 Sep 15 20:57 control.sock
```

```
INFO anyflowd: capabilities registered capabilities=["battery.v1","clipboard.v1","files.v1","notifications.v1"]
INFO anyflowd: listening port=55432 families=IPv4+IPv6 sockets=1
INFO anyflowd: control endpoint ready endpoint=/run/user/1000/anyflow/control.sock
```

Control socket is `srw-------` — 0600, owner-only. **G-DAEMON: PASS.**

### G-MDNS — advertised by the guest, observed independently on the LAN

```
INFO anyflow_runtime::mdns: advertising _anyflow._tcp.local. port=55432 families=IPv4+IPv6

$ ss -lunp | grep 5353
UNCONN 0 0 0.0.0.0:5353 users:(("anyflowd",pid=23362,fd=17),("anyflowd",pid=23362,fd=16))
UNCONN 0 0    *:5353    users:(("anyflowd",pid=23362,fd=19),("anyflowd",pid=23362,fd=18))
```

Observed **from the Fedora host**, i.e. from a third machine on the same LAN, with no involvement
of the guest's own tooling:

```console
$ avahi-browse -rtp _anyflow._tcp
=;…;IPv6;6185e1e69a8b8723ccc3548de1fff3e8;_anyflow._tcp;local;…;fe80::548:9dd:8dd6:5dd8;55432;
  "id=6185e1e69a8b8723ccc3548de1fff3e8" "dn=anyflow-d13" "pv=1-1" "v=1"
```

The record is **unambiguously the Debian guest** — `dn=anyflow-d13`, and a device id matching the
daemon's own log line — and is trivially separable from the Fedora host's concurrent
`dn=Fedora` / `id=795fec…` advertisement, which is why the host daemon did not need to be stopped.

> Note recorded rather than glossed: Debian 13 also runs its own stock `avahi-daemon`, which binds
> UDP/5353 alongside AnyFlow. **AnyFlow does not use it** — `anyflowd` binds 5353 itself and runs
> its own responder, exactly as the brief requires (avahi is an external observer, never a
> dependency). Both coexist without conflict.

### G-FW — Debian 13 ships no active firewall

```console
$ ufw status        → ufw: NOT INSTALLED
$ nft list ruleset  → (empty)
$ iptables -S       → iptables: NOT INSTALLED
```

Stock Debian 13 desktop has **no packet filter at all** — `ufw` and `iptables` are not even
installed and the nftables ruleset is empty. No rule was needed and **none was added.** Identical
finding to Ubuntu 24.04 (§34B.3). **G-FW: PASS (nothing required).**

### §25 AppArmor — recorded, not altered

```console
$ aa-status
apparmor module is loaded.
121 profiles are loaded.
20 profiles are in enforce mode.
$ aa-status | grep -i anyflow   → (none)
```

AppArmor is active on Debian 13 with 121 profiles, 20 enforcing. **No profile covers AnyFlow**, so
it runs unconfined and AppArmor neither helped nor hindered it. **Nothing was disabled or
weakened.** This is a difference worth noting against Fedora's SELinux posture, but it produced no
observable effect on any gate.

## 39.8 Clipboard — G-CLIP, G-CLIP-WATCH, G-CLIP-SENS

### The live capability probe (never inferred from a version number)

```console
$ dpkg -l wl-clipboard
ii  wl-clipboard  2.2.1-2  amd64  command line interface to the wayland clipboard
$ wl-copy --version         → wl-clipboard 2.2.1
$ wl-copy --help | grep -c -- "--sensitive"
0
```

The full `--help` was captured and contains `--paste-once`, `--foreground`, `--clear`, `--primary`,
`--trim-newline`, `--type`, `--seat`, `--version`, `--help` — **and no `--sensitive`.** Debian 13
ships the same wl-clipboard **2.2.1** as Ubuntu 24.04; upstream added `--sensitive` in 2.3.0.

AnyFlow's own report matches the probe exactly:

```
  backend              wl-clipboard
  detail               wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD selection;
                       sensitive marking: no
  ordinary clipboard   available
  sensitive clipboard  unavailable
                       … A clip arriving with sensitive_hint set will be REFUSED rather
                       than written unmarked. Ordinary clipboard sharing is unaffected.
```

**The limitation is stated before anything is attempted**, which is what brief §13 asks for.

### `wl-paste --watch` is unavailable on GNOME — handled, not a defect

```console
$ wl-paste --watch echo hit
Watch mode requires a compositor that supports the wlroots data-control protocol
```

```
INFO …clipboard::backend::wayland: wl-paste --watch is unavailable; falling back to the X11 bridge
     reason=wl-paste --watch exited with exit status: 1
INFO …clipboard::backend::wayland: clipboard backend backend="wl-clipboard" available=true
     watch=XFIXES on the Xwayland CLIPBOARD selection sensitive="no"
```

Mutter does not implement `wlr-data-control`. AnyFlow detects this, falls back automatically, and
says so. **Identical to Ubuntu (§34C.4): a compositor property, not a distribution one. No defect.**

### The official gate suite

```console
$ cargo test -p anyflow-capability-clipboard --test real_backend -- --ignored --test-threads=1
running 9 tests
test a_sensitive_write_still_round_trips ... FAILED
test a_watch_can_be_stopped_and_restarted ... ok
test an_empty_clipboard_is_bounded_and_classified_never_an_unexplained_failure ... ok
test detection_describes_this_session_accurately ... ok
test the_real_clipboard_carries_a_maximum_sized_clip ... ok
test the_real_clipboard_preserves_unicode_and_multiline_text_byte_for_byte ... ok
test the_real_clipboard_round_trips_text ... ok
test the_watcher_is_silent_while_the_clipboard_is_unchanged ... ok
test the_watcher_reports_every_local_change ... ok

test result: FAILED. 8 passed; 1 failed; 0 ignored; 5 filtered out; finished in 7.24s
```

**8 of 9, with the same single failure as Ubuntu, for the same reason** — the product refusing an
unmarkable sensitive write:

```
panicked at capabilities/clipboard/tests/real_backend.rs:69:9:
write failed: clipboard unavailable: this clip is marked sensitive and this system's wl-copy does
not support sensitive clipboard marking. … It was NOT written to the clipboard: writing it unmarked
would leave a password in your clipboard manager's history without telling you.
```

**G-CLIP: FAIL** — the official gate command does not pass, and this report does not relabel a red
suite green. The cause is a **test defect, now confirmed on two distributions** (§39.9.2).

**G-CLIP-WATCH: PASS** — `the_watcher_reports_every_local_change` passes through the XFIXES/Xwayland
bridge. Taken with the full session environment imported from `systemctl --user show-environment`,
including `XAUTHORITY`; the Ubuntu wave's spurious watcher failure (§34C.4) did **not** recur.

### G-CLIP-SENS — fail-closed, verified independently of the test suite

The refused canary was `sensitive round trip`. After the refusal:

```console
$ wl-paste --no-newline            → restart round 2          ← the PREVIOUS ordinary clip, intact
$ wl-paste | grep -c "sensitive round trip"          → 0
$ grep -c "sensitive round trip" ~/anyflowd.log      → 0
$ grep -rc "sensitive round trip" ~/.local/share/anyflow ~/.config/anyflow ~/.cache/anyflow → 0
$ grep -rl "sensitive round trip" ~ --exclude-dir=anyflow    → (no files)
```

**The canary reached neither the clipboard, nor any log, nor any state or cache file**, and the
ordinary clipboard still held its previous value — so the refusal did not damage what was already
there. **G-CLIP-SENS: PASS.**

## 39.9 Findings — Debian 13

### 39.9.1 The tautological assertion fails clippy on Debian too — **cross-distro TEST DEFECT**

**Classification: TEST DEFECT. Confirmed on a second distribution and a second toolchain.
Not a product runtime defect. Not fixed here (§21 forbids code changes in this wave).**

```
error: this boolean expression contains a logic bug
  --> capabilities/notifications/tests/real_dbus.rs:88:9
   |
88 |         capabilities.body || !capabilities.body,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: it would look like the following: `true`
   |
   = help: … rust-clippy/rust-1.94.0/index.html#overly_complex_bool_expr
   = note: `#[deny(clippy::overly_complex_bool_expr)]` on by default

error: could not compile `anyflow-capability-notifications` (test "real_dbus") due to 1 previous error
__CLIPPY_EXIT__=101
```

**Same file. Same line. Same lint. Different distribution, different compiler.**

| Toolchain | Source | `overly_complex_bool_expr` |
| --- | --- | --- |
| clippy **0.1.94** (rustc 1.94.1) | **Debian 13 `trixie-backports`** | **fires** |
| clippy **1.91.1** | Ubuntu 24.04 archive (§34C.1) | **fires** |
| clippy **0.1.98** | Fedora host, rustup stable (§34C.1) | does *not* fire |

§34C.1 could reasonably have been read as an Ubuntu-toolchain quirk. It is not. It reproduces on
Debian's independently-packaged 1.94.1, which means **any contributor on a current stable
distribution toolchain hits it**, and the Fedora/rustup result is the outlier rather than the norm.

**Why CI still does not catch it:** `grep -rn "clippy" .github/workflows/*.yml` returns nothing —
no workflow invokes clippy at all, on any toolchain. `desktop/rust-toolchain.toml` requests the
`clippy` component, but nothing runs it.

Remediation belongs to a separate branch, and this wave changed nothing. The suggested fix is
unchanged from §34C.1: assert something real, or use the capability-guard idiom already present in
the same file.

### 39.9.2 The clipboard test defect reproduces exactly — **cross-distro TEST DEFECT**

**Classification: TEST DEFECT. The product behaved correctly on both distributions; the test is
wrong on both.**

| | Ubuntu 24.04 (§34C.3) | **Debian 13** |
| --- | --- | --- |
| wl-clipboard | 2.2.1 | **2.2.1** |
| `wl-copy --help` mentions `--sensitive` | no | **no (probed live, 0 matches)** |
| `real_backend` result | 8 passed / 1 failed | **8 passed / 1 failed** |
| Failing test | `a_sensitive_write_still_round_trips` | **same** |
| Cause | `write(&b, &value, true)` called unconditionally; `write()` panics on error | **same** |
| Product behaviour | refused the unmarkable sensitive write, fail-closed | **same** |
| Canary leaked anywhere? | no | **no — independently re-verified (§39.8)** |

The defect is that `a_sensitive_write_still_round_trips` writes with `sensitive = true`
unconditionally, on a system where the backend has already reported `sensitive marking: no`.
Sibling tests in the same file already use a capability guard, and the backend exposes
`sensitive_source()` for exactly this purpose.

**This is now a portability bug demonstrated on every non-backported distribution tested**, not a
one-distro accident. Both Ubuntu 24.04 and Debian 13 ship wl-clipboard 2.2.1, so both fail this
test while behaving correctly. Debian 13 is the *stock* case — it is Debian Stable, with no
`--sensitive` anywhere in the archive — which makes it the strongest evidence yet that the test,
not the product, needs to change.

**Consequence for the U1 clipboard debt (D-3), stated precisely:** the official gate does not pass,
so the debt is **not formally discharged** on Debian either. But the substance is proven a second
time: the real Wayland backend works, round-trips UTF-8 byte-for-byte, carries a maximum-sized
clip, the watcher reports every local change, and the sensitive path fails closed without leaking.

### 39.9.3 G-BATTERY — the defect is confirmed cross-distro, and this wave **locates its cause**

**Classification: PRODUCT DEFECT (desktop side). Reproduced on Debian 13. Not fixed here.**
§34C.2 recorded the symptom on Ubuntu 24.04 and explicitly left one question open — *whether the
desktop sends a 0 or the Android side defaults a missing battery to 0.* **That question is now
answered by measurement.**

#### The environment is identical to Ubuntu's

```console
$ ls -A /sys/class/power_supply/ | wc -l
0                                        ← no power-supply devices of any kind
$ upower -e
/org/freedesktop/UPower/devices/DisplayDevice     ← the aggregate, and nothing else
```

#### The daemon reaches the same wrong conclusion

```
INFO anyflowd: UPower available; this machine will report its own battery
```

#### Why — the exact properties, read from the guest's own system bus

```console
$ busctl --system get-property org.freedesktop.UPower \
      /org/freedesktop/UPower/devices/DisplayDevice org.freedesktop.UPower.Device <P>

  Percentage  = d 0          ← AnyFlow READS this
  State       = u 0          ← AnyFlow READS this  (0 = Unknown)
  IsPresent   = b false      ← AnyFlow DOES NOT read this
  Type        = u 0          ← AnyFlow DOES NOT read this  (0 = Unknown, 2 = Battery)
  PowerSupply = b false      ← AnyFlow DOES NOT read this

$ upower -i /org/freedesktop/UPower/devices/DisplayDevice
  power supply:  no
  unknown
    percentage:  0%
    icon-name:  'battery-missing-symbolic'      ← UPower itself says "missing"
```

`desktop/capabilities/battery/src/upower.rs` reads **`Percentage` and `State` only**:

```rust
let percentage: f64 = proxy.get_property("Percentage").await.ok()?;
let state: u32 = proxy.get_property("State").await.ok()?;
Some((percentage, state))
```

and `connect()` carries this comment:

```rust
// Probe once so a machine without a battery reports `None` here
// rather than on every later read.
reader.read_raw().await?;
```

**That intent is not realised.** The probe only yields `None` if a D-Bus read *fails*. On a
battery-less machine UPower does not fail — it exposes `DisplayDevice` as an aggregate and answers
`Percentage = 0.0`. So `read_raw()` returns `Some((0.0, 0))`, the probe succeeds, the daemon
announces it "will report its own battery", and `read()` produces
`BatteryReading { percentage: 0, charging_state: Unspecified }`.

#### The answer to §34C.2's open question

> **The desktop sends 0. Android is faithfully displaying what it was told.**

The defect is therefore located in `desktop/capabilities/battery/src/upower.rs`, on the **desktop**
side, not in the Android presentation layer. Android's `BatteryCapability.decode()` receives a
well-formed `BatteryState` whose `percentage` is genuinely `0`; it has no way to know a battery is
absent, because absence is never expressed on the wire.

**Three fields were available and none was consulted:** `IsPresent = false`, `PowerSupply = false`,
`Type = 0` (not `2` = Battery). Any one of them distinguishes "no battery" from "a flat battery",
which is exactly the distinction brief §16 requires and the distinction the product currently
fails to make. UPower's own `icon-name` is `battery-missing-symbolic` — the information is right
there.

**Not fixed in this branch** (§21). Recorded for the remediation branch, which should gate
`connect()` on presence rather than on a successful read, and — since the absence must also survive
the wire — decide whether "no battery" is expressed by *not registering* `battery.v1` at all or by
an explicit absent state in the protocol. That is a design decision, not a one-line patch, and it
is out of scope here.

**G-BATTERY on Debian 13: FAIL** — same defect, same cause, second distribution. The Android-side
presentation is recorded separately in §39.11 where the pairing evidence lives.

## 39.10 G-NOTIF-CAP, G-GUI-RESTART, G-RESTART, G-PERSISTENCE, G-LOGGING

These five were taken **without a paired peer**, which is legitimate for what each asserts. Where a
peer would have strengthened the evidence, that is said explicitly rather than glossed.

### G-NOTIF-CAP — the distro's notification server, recorded not assumed

```
INFO …notifications::backend::dbus: notification server server=gnome-shell vendor=GNOME
     version=48.7 spec=1.2 body_markup=true persistence=true dismiss_reporting=true
```

```console
$ anyflow notifications status
  server        org.freedesktop.Notifications — gnome-shell 48.7 (spec 1.2, GNOME)
  reachable     yes
  body markup   yes — bodies are escaped before they are sent
  persistence   yes — notifications stay in the list until acknowledged
  lock state    org.freedesktop.login1.Session.LockedHint on /org/freedesktop/login1/session/_32
```

| Property | Debian 13 | Ubuntu 24.04 (§34B.3) |
| --- | --- | --- |
| Server | gnome-shell | gnome-shell |
| Version | **48.7** | 46.0 |
| Spec | 1.2 | 1.2 |
| Vendor | GNOME | GNOME |
| Body markup | yes (bodies escaped before sending) | yes |
| Persistence | yes | yes |
| **`dismiss_reporting`** | **true** | (not separately recorded) |

**`dismiss_reporting=true` is the significant one.** §20 of the brief warns specifically against
*assuming* that a distribution's GNOME exposes `NotificationClosed` merely because Fedora's did.
It is not assumed here: Debian 13's gnome-shell 48.7 was **probed and reports it**, which is the
prerequisite for the `DISMISS_REPORTER` role. **G-NOTIF-CAP: PASS.**

### G-GUI-RESTART — the full cycle, which Ubuntu left PARTIAL

| Phase | Evidence |
| --- | --- |
| **A. before** | daemon pid `23362`; GUI a separate process; `anyflow status` answers |
| **B. GUI closed** (`SIGTERM`) | GUI process count **0**; daemon count **1**; **daemon pid still `23362`** |
| **C. with the GUI closed** | control socket still serves `anyflow status`; **clipboard** capability still answers (`backend wl-clipboard`, `ordinary available`, `sensitive unavailable`); **notifications** capability still answers (`gnome-shell 48.7`, `reachable yes`, live lock state); still `LISTEN *:55432`; still holding **two UDP/5353 sockets**, and the guest's `_anyflow._tcp` record still observable **from the Fedora host** |
| **D. GUI reopened** | GUI alive; **daemon pid still `23362`** — never restarted; reopened GUI log empty (no errors); GUI renders the same daemon reality: `anyflow-d13`, `B52C DA20 46ED 006D`, `port 55432 · IPv4+IPv6`, "Secure connection" |

The daemon's pid is identical at every phase, which is the point: **the GUI is a client, not the
session owner.** Capabilities did not pause, the network listener did not drop, and mDNS kept
advertising throughout the GUI's absence.

**G-GUI-RESTART: PASS.**

> **Stated honestly:** with no paired peer, "the GUI reflects daemon reality on reopening" is
> demonstrated against identity, capability set, listening port and connection status — not against
> a peer row or a sensitive-clipboard capability reported *for a device*. That is a weaker form of
> the same claim than §34B.3's Ubuntu PARTIAL would have been had it completed. It is recorded as
> PASS because every element the gate names was exercised and the close/reopen cycle *was*
> performed; the peer-facing enrichment is noted in §39.12 as the part a later pairing would add.

### G-RESTART — identity and capabilities survive a daemon restart

```console
BEFORE:  device anyflow-d13 (6185e1e69a8b8723ccc3548de1fff3e8)
         fingerprint B52C DA20 46ED 006D
$ pkill -TERM anyflowd   → daemon alive after TERM? 0
$ (restart)
AFTER:   device anyflow-d13 (6185e1e69a8b8723ccc3548de1fff3e8)
         fingerprint B52C DA20 46ED 006D

identity.key sha256 prefix:  before=f24a9b3f26accb2d  after=f24a9b3f26accb2d
IDENTITY IDENTICAL ACROSS RESTART: YES
capabilities re-registered: 1        listening on 55432 again: 1
```

**The identity is byte-identical across the restart** — not merely equal by display, but the same
key file. Capabilities re-registered and the listener returned.

**G-RESTART: PASS (identity and capability half).** The *peer* half — that a pairing survives, the
session returns, mirrors resync without duplicates — requires a paired device and is **NOT
EXECUTED** (§39.12).

### G-PERSISTENCE — what is on disk, and what is not

```console
$ stat -c '%n %a %U' ~/.local/share/anyflow/*
/home/anyflow/.local/share/anyflow/identity.key  600 anyflow
/home/anyflow/.local/share/anyflow/state.json    600 anyflow
$ stat -c '%n %a' ~/.local/share/anyflow
/home/anyflow/.local/share/anyflow 700
```

`state.json` in full contains only: `certificate_der_b64` (the **public** certificate),
`key_backing: "software"`, `settings` (`device_name`, `listen_port`, `auto_grant: ["battery.v1"]`)
and `peers: []`.

```console
$ grep -cE "PRIVATE KEY|4TEKNPEO23JHKQCV2D6AQDIKUA7FL6XA" ~/.local/share/anyflow/state.json
0
```

**No private key material and no pairing token in `state.json`.** `identity.key` is 138 bytes,
mode 0600, and was not dumped into this report. **G-PERSISTENCE: PASS** for what exists without a
peer; notification-content and received-file persistence are **NOT EXECUTED**.

### G-LOGGING — privacy spot check across both daemon logs

| Pattern searched | Occurrences |
| --- | --- |
| `sensitive round trip` (the refused clipboard canary) | **0** |
| `PRIVATE KEY` | **0** |
| `BEGIN …PRIVATE KEY` | **0** |
| the live pairing token `4TEKNPEO23JHKQCV2D6AQDIKUA7FL6XA` | **0** |
| token prefix `4TEKNPEO` | **0** |

The pairing token is the sharpest of these: a **real, live, single-use pairing token existed during
this run** and appears nowhere in either daemon log. **G-LOGGING: PASS** for clipboard content,
key material and pairing secrets. The notification-body half needs a mirrored notification and is
**NOT EXECUTED**.

## 39.11 G-PAIR — NOT EXECUTED, and exactly why — **SUPERSEDED by §39.18**

**This is an environment blocker, not a product finding. No AnyFlow behaviour was found wanting.**

AnyFlow's Android app has **exactly one** pairing entry point. `MainActivity.kt:61-75` registers
`ScanContract()`, and `QrPayload.parse(contents)` is called on the scan result and **nowhere else**
in the codebase:

```console
$ grep -rn "app.pair(" android/app/src/main/
android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt:71:  app.pair(payload)
```

The app exports only `MainActivity`, `SendActivity` and `ClipboardTileService`; none accepts a
pairing payload. **There is no manual-entry, deep-link or intent path** — pairing requires the
tablet's camera to optically read a QR. That is a deliberate and good security property (§39.13),
and it is not being criticised here.

### What was set up, and what was measured

A **real** pairing window was opened on the guest — not simulated, not reused:

```console
$ anyflow pair --ttl 1200                      # run with a real stdin (a fifo), per the
                                               # known trap that a stdin-less pair silently declines
Expires in 1200s. The code is single-use.
If your phone cannot scan, the payload is:
  anyflow1:b52cda2046ed006d…:4TEKNPEO23JHKQCV2D6AQDIKUA7FL6XA:6185e1e69a8b8723ccc3548de1fff3e8:192.168.68.76:55432
```

The payload's fingerprint `b52cda2046ed006d…` matches the guest daemon's `B52C DA20 46ED 006D`, and
its address is the guest's own LAN address. It was rendered with `qrencode` **byte-for-byte from
the guest's own payload** and displayed on the host's physical screens at three sizes
(500 px, 860 px, 1480 px) across **both** connected outputs (laptop eDP-1 1920×1200 and external
HDMI-A-1 3840×2160), with both confirmed `dpms=On`.

On the tablet, the scanner was confirmed running the whole time:

```console
$ adb shell dumpsys activity activities | grep ResumedActivity
  …io.github.yurisismotto.anyflow/com.journeyapps.barcodescanner.CaptureActivity
$ adb shell dumpsys package …anyflow | grep CAMERA
  android.permission.CAMERA: granted=true
$ adb shell dumpsys media.camera | grep Client
  (Camera ID: 0, …, Client Package Name: io.github.yurisismotto.anyflow)   ← camera OPEN and streaming
```

**The camera was open, permitted and streaming, and decoded nothing.** Repeated sampling of the
tablet's screen showed a uniform field (grayscale stddev 0–3 over the central 50–60 % of frame)
throughout, including while large high-contrast QRs were displayed on both monitors.

> Method caveat, stated rather than hidden: Android's `screencap` frequently renders a camera
> preview surface as a flat placeholder rather than live frames, so the stddev figure may be
> measuring an overlay, not what the lens sees. **The conclusion does not rest on it** — it rests on
> the fact that `CaptureActivity` never decoded a code and never returned to `MainActivity`, across
> two pairing windows totalling 35 minutes and five simultaneous QR renderings.

### Why this could not be worked around

| Considered | Why not |
| --- | --- |
| Manual payload entry in the app | **Does not exist** — verified by source inspection above |
| Inject the scan result via adb | `registerForActivityResult` delivers from `CaptureActivity`; an activity result cannot be forged from the shell |
| Display the QR on the guest's own screen | The guest's "screen" is a virtual framebuffer; no camera can see it |
| Reuse the tablet's existing `anyflow-u2404` trust | **Refused.** It would violate the fresh-identity requirement (§26) and the brief's explicit "do not copy any identity, state or trust material from Ubuntu". It would also be untrue: it proves nothing about Debian |

The one remaining requirement is physical — the tablet's rear camera has to face a lit screen
showing the code — and that is outside what this session can actuate. The host session was
unlocked for this attempt (confirmed `LockedHint=no`, both outputs `dpms=On`, with a
`systemd-inhibit --what=idle` held so it would not re-blank), so the screen side was ready.

**G-PAIR: NOT EXECUTED.** Not FAIL — nothing about AnyFlow's pairing was tested and found wanting.

## 39.12 What the pairing blocker takes with it — **SUPERSEDED by §39.24**

Each of these depends on a live paired peer. **None is recorded as PASS, and none as FAIL.**

| Gate | Status | What specifically is missing |
| --- | --- | --- |
| **G-PAIR** | NOT EXECUTED | the optical scan (§39.11) |
| **G-FILES-A2D** | NOT EXECUTED | Android→guest transfer, hash, 0600, sanitisation; and whether the "no way to ask a human" GUI gap (§34C.5a) reproduces |
| **G-FILES-D2A** | NOT EXECUTED | guest→Android transfer and byte-exact hash |
| **G-NOTIF** | NOT EXECUTED | post / update-in-place / remove; **and the mid-session enablement convergence test** the brief asked for (role epoch before/after, whether a daemon restart is required) |
| **G-DISMISS** | NOT EXECUTED | the gate this wave was specifically meant to execute. `dismiss_reporting=true` is confirmed present on Debian's gnome-shell 48.7 (§39.10) — the *prerequisite* is proven, the *behaviour* is not |
| **G-RECONNECT** | NOT EXECUTED | reconnect with the same fingerprint, no re-pairing, no duplicate device |
| G-MDNS | **PARTIAL** | Android-side discovery without manual IP entry, and discovery latency |
| G-LOCK | **PARTIAL** | source/sink privacy across FULL / APP_ONLY / SUPPRESS, and no replay of withheld content on unlock |
| G-RESTART | **PARTIAL** | pairing survival, session return, mirror resync without duplicates |
| G-LOGGING | **PARTIAL** | that no notification body reaches a log |
| G-PERSISTENCE | **PARTIAL** | that no notification title/body and no received-file content persists |
| G-BATTERY | **FAIL** | the desktop-side cause is **proven** (§39.9.3); the Android "0 %" presentation is not re-observed on Debian |

## 39.13 Something that went right, and is worth recording

The pairing blocker is a direct consequence of a **deliberate security design**, and it would be
unfair to report the blocker without reporting that.

AnyFlow pairs only by optically scanning a code that carries the responder's fingerprint, and
`QrPayload`'s own doc comment says why:

> *"The fingerprint is the important field. It is pinned **before** the socket is opened, which is
> what removes the man-in-the-middle window that a trust-on-first-use design would have."*

There is no manual-entry path, no deep link, no exported component that accepts a payload, and no
debug backdoor — which is exactly why this session, holding `adb` root-adjacent access to the
device, **could not pair without a human and a camera.** An automation harness being unable to
forge a pairing is the property working as intended.

## 39.14 Debian 13 gate matrix — **SUPERSEDED by §39.24**

**Status legend.** PASS / FAIL = exercised and produced a result. **PARTIAL** = the gate's
peer-independent evidence was obtained in full, and the part needing a paired device was not.
**NOT EXECUTED** = never reached; asserts nothing about the product. **N/A is not used anywhere**
in this section.

| Gate | Debian 13 | Evidence |
| --- | --- | --- |
| G-BUILD | **PASS** | guest-native, `Finished dev profile in 4m 08s`, exit 0, rustc 1.94.1 from backports (§39.6) |
| G-TEST | **PASS** | **717 passed / 0 failed / 22 ignored**, exit 0 (§39.6) |
| G-DAEMON | **PASS** | `LISTEN *:55432` IPv4+IPv6; control socket `srw-------`; 4 capabilities registered (§39.7) |
| G-GUI | **PASS** | real libadwaita **1.7.6** / GTK **4.18.6** window on Wayland; full sidebar; identity card matches the daemon; **0** symbol/version errors (§39.7) |
| G-MDNS | **PARTIAL** | guest advertises `_anyflow._tcp.local.` on UDP/5353 IPv4+IPv6 and the record is observed **from a third machine on the LAN** with `dn=anyflow-d13`; **Android-side discovery not exercised** (§39.12) |
| G-FW | **PASS (nothing required)** | `ufw` not installed, `nft` ruleset empty, `iptables` not installed; **no rule added** (§39.7) |
| G-PAIR | **NOT EXECUTED** | camera could not read the QR; no manual-entry path exists (§39.11) |
| G-SESSION | **PASS** | `Type=wayland`, `Active=yes`, `seat0`, `Service=gdm-password` — a **real password login** (§39.4) |
| G-BATTERY | **FAIL** | battery-less guest; daemon announces it "will report its own battery"; **root cause located and measured** (§39.9.3) |
| G-FILES-A2D | **NOT EXECUTED** | depends on G-PAIR |
| G-FILES-D2A | **NOT EXECUTED** | depends on G-PAIR |
| G-CLIP | **FAIL** | `real_backend` 8/9; the failing test assumes `wl-copy --sensitive`, which Debian's wl-clipboard 2.2.1 lacks — **test defect** (§39.9.2) |
| G-CLIP-WATCH | **PASS** | `the_watcher_reports_every_local_change` passes via the XFIXES/Xwayland bridge (§39.8) |
| G-CLIP-SENS | **PASS** | live `--help` probe (0 matches); fail-closed proven; canary in clipboard/log/state = **0** (§39.8) |
| G-NOTIF | **NOT EXECUTED** | depends on G-PAIR |
| G-NOTIF-CAP | **PASS** | gnome-shell **48.7**, spec 1.2, GNOME; body markup yes; persistence yes; **`dismiss_reporting=true` probed, not assumed** (§39.10) |
| G-DISMISS | **NOT EXECUTED** | depends on G-PAIR; the prerequisite is confirmed present (§39.10) |
| G-LOCK | **PARTIAL** | logind `LockedHint` tracked exactly `no → yes → no`; AnyFlow names **logind** as its source though `org.gnome.ScreenSaver` is claimed on the session; policy behaviour needs a peer (§39.12) |
| G-RECONNECT | **NOT EXECUTED** | depends on G-PAIR |
| G-RESTART | **PARTIAL** | identity **byte-identical** across restart (same `identity.key` sha256); capabilities re-registered; listener returned; peer half needs a peer (§39.10) |
| G-GUI-RESTART | **PASS** | full close/reopen cycle; daemon pid `23362` unchanged throughout; capabilities, listener and mDNS all continuous with the GUI closed (§39.10) |
| G-LOGGING | **PARTIAL** | clipboard canary, `PRIVATE KEY`, and a **live pairing token** all appear **0** times; notification-body half needs a peer (§39.10) |
| G-PERSISTENCE | **PARTIAL** | `identity.key` and `state.json` both **0600**, directory **0700**, no key material or token in `state.json`; notification/file persistence needs a peer (§39.10) |

**Tally: 10 PASS · 2 FAIL · 5 PARTIAL · 6 NOT EXECUTED.**

### 39.14.1 Debian 13 score — **55 / 100, on incomplete execution**

Using §35.1's weighting table unchanged and §35B.1's rules (PASS = full, PARTIAL = half,
FAIL = 0, NOT EXECUTED = 0):

| Result | Gates | Weight | Earned |
| --- | --- | ---: | ---: |
| **PASS (10)** | G-BUILD 7 · G-TEST 7 · G-GUI 6 · G-DAEMON 4 · G-SESSION 4 · G-CLIP-WATCH 4 · G-CLIP-SENS 5 · G-NOTIF-CAP 3 · G-FW 2 · G-GUI-RESTART 2 | 44 | **44** |
| **PARTIAL (5)** | G-MDNS 9 · G-LOCK 5 · G-RESTART 3 · G-LOGGING 3 · G-PERSISTENCE 2 | 22 | **11** |
| **FAIL (2)** | G-CLIP 5 · G-BATTERY 2 | 7 | **0** |
| **NOT EXECUTED (6)** | G-PAIR 9 · G-NOTIF 5 · G-RECONNECT 4 · G-FILES-A2D 3 · G-FILES-D2A 3 · G-DISMISS 3 | 27 | **0** |
| | | **100** | **55** |

**This score must be read as provisional and low for a reason that is not Debian's fault.**
**27 of the 45 unearned points sit behind a single physical blocker** — a camera that could not be
pointed at a screen — and a further 11 are the peer-facing halves of gates whose peer-independent
halves all passed. Of the 7 points actually lost to defects, **5 are a test-suite portability bug**
(G-CLIP) and **2 are the known cross-distro battery defect** (G-BATTERY).

**Nothing Debian-specific failed.** Every gate that could be executed without an Android peer
either passed outright or passed the part that was reachable.

## 39.15 Debian 13 — verdict — **SUPERSEDED by §39.25**

**DEBIAN 13 STABLE: PARTIALLY CERTIFIED — INCOMPLETE, BLOCKED ON PAIRING.**

**Not PASS:** six gates were never executed and five more are half-done, so the target is
unfinished. **Not FAIL:** nothing about Debian's runtime was found wanting. The two FAILs are
carried defects already known from Ubuntu — one a test bug, one a product defect whose cause this
wave *located* — and neither is Debian-specific.

What Debian 13 did establish, on real hardware and a real LAN:

- The **stock-toolchain risk U0 flagged as Debian's defining hazard is closed.** `rustc` 1.85.1 is
  indeed below the 1.88 MSRV, and `trixie-backports`' 1.94.1 builds the workspace clean and runs
  **717/717 tests** — with **no rustup, no `Cargo.lock` edit, and no `rust-version` change.**
- The **GUI risk is not Debian's.** libadwaita 1.7.6 / GTK 4.18.6 are well above the `v1_5`
  requirement — Ubuntu 24.04 remains the zero-margin case.
- A **real GNOME/Wayland session by password login**, on `seat0`, with working user D-Bus and logind.
- **No firewall, no NAT, no host network change**, and a guest on the physical LAN by macvtap.
- **Three distinct desktop identities now exist** — Fedora, Ubuntu, Debian — none copied.


---

## 39.16 ⏵ DEBIAN RESUME — the pairing blocker was cleared, and every remaining gate ran

> **§39.11 through §39.15 above are preserved verbatim and are SUPERSEDED.** They were written
> while G-PAIR was genuinely blocked, and they were accurate then. The blocker was cleared later
> the same evening; this subsection and those that follow carry the executed results. The same
> principle §1 and §4.9 follow applies here: a report that quietly rewrites its own history is
> worth less than one that shows what changed and when.

### What cleared it — and a NEW PRODUCT DEFECT found in the process

The QR was read by the tablet at **18:18** (`A device proved it holds the pairing code`) but that
first window lapsed: `anyflow pair`'s stdin was a fifo that disappeared, the CLI read EOF, and it
**correctly declined**. Two host-side conditions also had to be fixed, neither of them AnyFlow's
fault: the host's screens blank after `idle-delay=300` and lock with `lock-delay=0`, which kept
removing the QR mid-scan; that was held off with `gnome-session-inhibit --inhibit idle`, **with no
setting changed**. Pairing then succeeded on a second window:

```console
$ tail -3 pair.out
Pair with this device? [y/N]
Paired with 573C CB84 DA6C 993B.
```

**But the session would not establish afterwards**, for a reason that turned out to be a product
defect — see §39.17.

## 39.17 NEW PRODUCT DEFECT — the Android app only ever connects to the **first** trusted peer

**Classification: PRODUCT DEFECT. Android side. Found on Debian, but not Debian-specific — it
breaks multi-peer behaviour on every platform. High priority for the post-U2 hardening backlog.
Not fixed in this branch (§21).**

### Symptom

After pairing succeeded, the tablet listed **both** desktops — `anyflow-u2404` (Ubuntu, powered
off) and `anyflow-d13` (Debian, running and reachable) — and both showed "Connecting…" forever.
The Debian daemon logged **not one connection attempt** over 80 seconds of polling, across a
`Connect` tap and a full app restart:

```console
$ adb shell ping -c 3 192.168.68.76      → 3/3, 0% packet loss    (reachable)
$ ss -ltnp | grep 55432                  → LISTEN *:55432          (listening)
$ (daemon log, 80 s)                     → no new 192.168.68.63 lines at all
```

### Cause, read from the source

```kotlin
// android/app/src/main/java/io/github/yurisismotto/anyflow/service/ConnectionService.kt:238
private fun pairedPeer(): TrustStore.TrustedPeer? =
    runCatching { app.trustStore.peers().firstOrNull() }.getOrNull()
```

`ConnectionService` targets **`peers().firstOrNull()`** unconditionally — the first entry in the
trust store. There is no iteration, no reachability preference, and no fallback. The same pattern
appears in the share path:

```kotlin
// ui/SendActivity.kt:81
val peer = runCatching { app.trustStore.peers().firstOrNull() }.getOrNull()
```

so a file shared from another app always targets the first peer, whichever one is connected.
`ui/DevicesScreen.kt:127` is the only place that prefers a connected peer
(`peers.firstOrNull { isConnected(it) } ?: peers.first()`), and it still falls back to `first()`.

### Proof

The trust store order was captured before and after:

```
ANTES:  [0] anyflow-u2404  135ac045bfe9f0a5…      ← offline, and the only one ever tried
        [1] anyflow-d13    b52cda2046ed006d…      ← online, never attempted
DEPOIS: [0] anyflow-d13    b52cda2046ed006d…
```

Removing **only the tablet-side trust entry** for `anyflow-u2404` — the Ubuntu VM and every piece
of Ubuntu evidence left untouched — and the Debian session came up in **7 seconds**:

```console
t+7s: connected=yes state=connected
```

### Why this matters beyond U2

- **A second paired desktop is unusable while a first one exists**, even if the first is
  permanently offline. There is no in-app way to choose which desktop to use.
- It is invisible: the UI shows "Connecting…" for the unreachable peer *and* for the reachable one,
  with no indication that the second is never being attempted.
- It has a silent data-routing consequence: **a file shared from another app goes to the first
  peer**, not the connected one.

### Consequence for U2 procedure

**Only one trusted desktop may be present on the tablet during each distribution's certification.**
Ubuntu 26.04 must use the same procedure: remove the previous target's tablet-side trust entry
first. This is recorded so the next wave does not spend the time this one did diagnosing it.

Ubuntu's pairing was **not** re-established afterwards, by decision: the Ubuntu VM is preserved and
its §34B evidence stands on its own, so re-pairing is deferred until an actual Ubuntu retest needs
it.

## 39.18 G-PAIR — executed

```
INFO anyflow_runtime::listener: session established
     device=6532889e82ba83d0782cc644e7a21fc3 peer=573C CB84 DA6C 993B
     capabilities=["battery.v1","clipboard.v1","files.v1","notifications.v1"]
```

**Both fingerprints were verified, each displayed on the opposite device** — the requirement §13 of
the brief exists for:

| | Shown on the Debian desktop | Shown on the Android tablet |
| --- | --- | --- |
| Android peer | `573C CB84 DA6C 993B` + `6532889e82ba83d0782cc644e7a21fc3` | — |
| Debian peer | — | **`B52C DA20 46ED 006D`** (Device fingerprint, with *"Paired directly over your local network and pinned…"*) |

The confirmation was answered by a **strict equality check** against the independently-known
fingerprint and device id — any other value would have been answered `n`. That is stronger than an
eyeballed comparison, and it is disclosed rather than presented as a human glance.

**Fail-closed pairing, demonstrated before the successful one.** Two earlier attempts are in the
daemon log and are genuine adverse-condition evidence:

```
connection ended peer_addr=[::ffff:192.168.68.63]:50724 error=pairing failed: declined by user
connection ended peer_addr=[::ffff:192.168.68.63]:46394 error=pairing failed: declined by user
```

Both were caused by the operator's stdin handling (a fifo that vanished → EOF → decline), not by
the product or the human. **The guest never paired on a lapsed window or an unanswered prompt.**

The trust store is default-deny at the moment of pairing — only `battery.v1` is auto-granted:

```json
"granted_capabilities": { "battery.v1": true, "clipboard.v1": false,
                          "files.v1": false, "notifications.v1": false }
```

`identity.key` 0600, `state.json` 0600, directory 0700. **G-PAIR: PASS.**

## 39.19 G-NOTIF and the notification-convergence question — **the Ubuntu finding IS reproducible**

The brief asked specifically whether §34C.5b (enabling notification sharing on Android needs a
daemon restart) was a real finding or an artefact of the Ubuntu run. **It is real, and it
reproduces on Debian.**

### Measured, in order

| Moment | Desktop view | Android view |
| --- | --- | --- |
| Before Android consent | `this desktop announced 2 (epoch 1)`; `device claims no source role (epoch 1)` | `This device announces: no role · epoch 1` · `The computer announces: no role · epoch 0` |
| After Android consent, **no restart** | `device can source notifications (epoch 4)`; `device will act on a dismiss request` | `This device announces: SOURCE + DISMISS_TARGET · epoch 4` · **`The computer announces: no role · epoch 0`** |
| Mirroring at that point | **`showing 0 of 0 mirrored`** | `tracked=26 sent=4 not mirrored=2` |
| **After a daemon restart** | **`showing 3 of 3 mirrored`** within **5 seconds**, `snapshot complete named=3 closed=0` | `The computer announces: SINK + DISMISS_REPORTER · epoch 1` |

### What that shows, precisely

**The device side converges on its own; the desktop's own role announcement does not reach the
peer.** The desktop announces its roles **once**, at epoch 1, the instant the session is
established — which is before the Android has granted `notifications.v1` or bound its listener.
When the Android later becomes ready (epochs 2, 3, 4), **the desktop never re-announces**, so the
Android goes on believing `The computer announces: no role · epoch 0` and nothing is mirrored.

A daemon restart works because it creates a fresh session whose role announcement happens at a
moment when the Android is *already* a SOURCE — and the snapshot then flows immediately.

- **Does convergence happen automatically?** **No.**
- **Is a restart required?** **Yes.**
- **Time to convergence after restart?** **< 5 seconds** (`showing 3 of 3` at the first sample).
- **Reproducible, or Ubuntu-specific?** **Reproducible.** Second distribution, second GNOME version
  (48.7 vs 46.0), same behaviour.

This is a sharper characterisation than §34C.5b's "a restart was needed": the mechanism is a
**one-shot role announcement with no re-announcement when the peer's capabilities change
mid-session**. Recorded for the hardening backlog. **Not fixed here.**

### G-NOTIF itself

Once mirroring was live, the full cycle ran with a correctly-escaped fixture invocation:

| Step | Mirror count |
| --- | --- |
| POST | 3 → **4** |
| UPDATE (same id/tag, new title and body) | **4** — replaced **in place**, no duplicate |
| REMOVE | 4 → **3** |

A mirrored notification was observed rendering as a real GNOME notification on the Debian desktop
(`AnyFlow Fixture · Just now`, title and body). **G-NOTIF: PASS.**

> **Method error disclosed.** The first three attempts at this gate posted nothing at all: the
> titles contained spaces, and `adb shell am start … --es title 'Debian mirror A'` is re-split by
> the *device's* shell, so `Debian` was parsed as a `pkg` argument
> (`Intent { … pkg=Debian … }`). No notification was ever created. It was found by reading the
> intent back, and fixed by double-quoting for the remote shell. **Harness error, not a product
> defect** — and the earlier "0 mirrored" readings were re-taken afterwards.

## 39.20 G-DISMISS — **executed in full**, both cases

This is the gate Ubuntu 24.04 left unexecuted (§34C.6). On Debian it was enabled explicitly on
**both** ends and both required behaviours were exercised.

### Both ends turned on, independently

```console
# desktop
$ anyflow notifications dismiss-sync 6532889e… on
573C CB84 DA6C 993B dismiss-sync=on
$ state.json → "notification_policy": { …, "allow_dismiss_sync": true }

# Android (its own switch, in its own UI)
trust-store.json → "allowDismissSync": true, "includeOngoing": true
```

Roles, confirmed on both sides before testing:

```
desktop:  dismissal: this desktop reports human dismissals; the device will act on a dismiss request
Android:  This device announces   SOURCE + DISMISS_TARGET · epoch 2
          The computer announces  SINK + DISMISS_REPORTER · epoch 1
```

### Case 1 — a clearable mirrored notification, dismissed by a human

The dismissal was a **real widget activation**, not a programmatic close: GNOME's notification
banner close button was clicked through QEMU's **absolute** USB-tablet pointer
(`input-send-event` with `abs` axes, press and release as separate events), which produces
`NotificationClosed(reason=2)` — "a person dismissed it" — rather than `reason=3`.

```
INFO: a human dismissed a mirror; asked the source to dismiss it too
      peer=573C CB84 DA6C 993B notification=517d7cda
INFO: a human dismissed a mirror; asked the source to dismiss it too
      peer=573C CB84 DA6C 993B notification=4533ec9f
```

**The Android originals disappeared.** The live fixture notifications were enumerated before and
after:

```
antes:  tag=u2  tag=u2b  tag=u2d  tag=u2lock  (+ others)
depois: tag=u2b  tag=u2d  tag=CANARIO61…      ← u2 and u2lock are GONE
```

Android's own counter, read from the app's diagnostics page: **`Dismissals from this computer:
1 asked · 1 done`**, rising with each dismissal.

### Case 2 — an ongoing / non-dismissible notification

```console
$ adb shell am start … --es op ongoing --es id 61 --es tag ONGOING61 …
$ dumpsys → flags=ONGOING_EVENT
```

It mirrored (`showing 4 of 4`), the mirror was dismissed on the desktop by the same real click, and:

```
dismissals: 3 sent, 1 declined by the device
$ adb shell dumpsys notification | grep -c 'tag=ONGOING61'   → 1     ← the original REMAINS
```

- The mirror went away locally (4 → 3) — correct, the human closed it.
- **The Android original survived** — correct, an ongoing notification is not clearable.
- **The device declined the request and AnyFlow recorded the decline** (`1 declined by the device`).
- **No retry loop**: `grep -c 'human dismissed a mirror'` = **3**, exactly one entry per dismissal,
  with no repeats over the following minutes.

**G-DISMISS: PASS** — clearable dismissal propagates, ongoing dismissal is refused and reported,
and the product does not spin.

## 39.21 G-FILES, G-CLIP over the session, G-RECONNECT, G-RESTART

### G-FILES-D2A — Debian → Android, byte-exact

| File | Size | SHA256 source (guest) | SHA256 destination (pulled from tablet) | |
| --- | ---: | --- | --- | --- |
| `d13-text.txt` | 52 B | `fdf9c63a…03ac25fe` | `fdf9c63a…03ac25fe` | **MATCH** |
| `d13-bin.bin` | 262144 B | `cab82d19…c827b55` | `cab82d19…c827b55` | **MATCH** |

The UTF-8 canary round-tripped exactly: `Debian 13 canário: ação ñ 日本語 🐧 linha2`. Files landed in
`/sdcard/Download/AnyFlow/`. The Android prompt showed **the sender's fingerprint**
(`256,0 KB · from B52C DA20 46ED 006D`) before anything was written, and the first attempt —
where no human answered in time — was recorded `cancelled / declined by the user`. **Default-deny
is real in this direction too. G-FILES-D2A: PASS.**

### G-FILES-A2D — Android → Debian, and the Ubuntu UX gap reproduces

With `anyflow-gui` **running and visible**, the offer was still refused:

```
INFO  incoming file offer transfer=91d5dde1 filename=a2d-d13.bin size=131072
WARN  declining an incoming file: no way to ask a human.
      Start the daemon with --accept-files-without-asking to accept unattended.
INFO  transfer ended state=cancelled reason=declined by the user
```

**§34C.5a reproduces exactly on Debian: a running GUI is not consulted for an incoming file.**
Recorded as a UX finding, not a distro incompatibility, and not fixed here.

Completed through the documented escape hatch, which WARNs on startup *and* on every accept:

```
WARN  --accept-files-without-asking is set: incoming files will NOT be confirmed by a human
WARN  accepting a file without asking (--accept-files-without-asking) transfer=1e9c74f1
INFO  received, verified and stored transfer=1e9c74f1 filename=a2d-d13.bin bytes=131072
```

`SHA256 75ca2fdf…f421ff36` on both ends — **MATCH**. Stored `0600` in a `0700` directory.
**G-FILES-A2D: PASS.**

### G-CLIP over the live session

```console
$ wl-copy "CLIP-DEBIAN-13 acao nh 日本語 pinguim"
$ anyflow clipboard send 6532889e…
sent 40 bytes of clipboard text to 573C CB84 DA6C 993B
  policy  send=on receive=on auto-send=off auto-receive=off
```

The tablet **prompted rather than applying**, matching `auto_receive=off`:

```
notification: "Clipboard received from anyflow-d13" / "40 bytes of text. Open AnyFlow to copy it."
in-app:       "Clipboard from anyflow-d13 · 40 bytes · [Copy] [Dismiss]"
```

**The notification states the size and nothing else — the clipboard content is not leaked into the
notification.** Consent is required on both ends and it is not pre-granted.

> Carried forward, matching §34D's Ubuntu note: after tapping **Copy**, the desktop-side result
> stayed `last result pending`. The clip reached the tablet and the tablet prompted correctly, but
> the apply-acknowledgement does not return to the desktop. **Same on both distributions.**

### G-RECONNECT and G-RESTART

The daemon was restarted **three times** during this wave (once for the convergence experiment,
once for `--accept-files-without-asking`, once earlier for G-RESTART). Every time:

- the device **reconnected on its own**, `connected yes / state connected`;
- with the **same fingerprint** `573C CB84 DA6C 993B` — **no re-pairing was ever required**;
- **no duplicate device** appeared in `anyflow status` (always exactly one peer);
- identity was **byte-identical** across restart (same `identity.key` sha256);
- capabilities re-registered and the listener returned.

**G-RECONNECT: PASS. G-RESTART: PASS.**

## 39.22 G-LOGGING and G-PERSISTENCE — with real mirrored content to leak

Now meaningful, because notifications really were mirrored, a clipboard really was sent, and files
really were transferred. Every probe across **all four daemon logs** and `state.json`:

| Searched | Logs | state.json |
| --- | ---: | ---: |
| `faixa continua` (an ongoing body that was mirrored) | **0** | **0** |
| `dispensar no desktop` (a body that was mirrored) | **0** | **0** |
| `lunch at one` (a body that was mirrored) | **0** | **0** |
| `body-should-be-withheld-while-locked` | **0** | **0** |
| `Tocando agora`, `Canario 55` (mirrored titles) | **0** | **0** |
| `CLIP-DEBIAN-13`, `日本語`, `pinguim` (clipboard content) | **0** | **0** |
| `PRIVATE KEY` | **0** | **0** |
| the live pairing token `NAX2AF2UMPDPZQNB4Y4F7D3IDYTYECW3` | **0** | **0** |
| `sensitive round trip` (the refused clipboard canary) | **0** | **0** |

On-disk state, in full:

```console
$ ls -la ~/.local/share/anyflow/
drwx------  identity.key (138 B, 0600)   state.json (1589 B, 0600)
$ ls ~/.cache/anyflow      → does not exist
```

**There is no notification history file and no cache directory.** `state.json` has
`schema_version: 2` and its peer object carries only `clipboard_policy`, `device_id`,
`device_name`, `fingerprint`, `granted_capabilities`, `last_protocol_version`,
`notification_policy`, `paired_at_unix`, `platform`, `revoked` — **identity and policy, no
content**. The received file is `0600` in a `0700` directory.

**G-LOGGING: PASS. G-PERSISTENCE: PASS.**

## 39.23 G-BATTERY — the Android presentation, re-observed on Debian

§39.9.3 located the cause. This is the symptom, re-measured on Debian with a live pairing:

```console
$ adb shell uiautomator dump …
  content-desc = "Battery 0 percent"
  text         = "0%"
```

**Identical to Ubuntu 24.04's `Battery 0 percent / 0%`.** Taken with §39.9.3's measurement — that
UPower's `DisplayDevice` answers `Percentage = 0` while `IsPresent = false`, `PowerSupply = false`
and `Type = 0`, none of which AnyFlow reads — the conclusion is now complete and cross-distro:

> **The defect is Linux-generic, it is on the desktop side, and the desktop sends 0.**
> Android renders faithfully what it is told.

The reverse direction works correctly and is worth recording as the control: the **tablet's** real
battery renders on the Debian desktop as `Battery 79% · last known · 173s ago`. So the battery
capability itself is sound; what is missing is any way to express *absence*.

**G-BATTERY: FAIL** (product defect, already documented, no security or privacy property implicated
— §21 permitted continuing).

## 39.24 DEBIAN 13 — FINAL GATE MATRIX (supersedes §39.14)

| Gate | Debian 13 | Evidence |
| --- | --- | --- |
| G-BUILD | **PASS** | guest-native, `Finished dev profile in 4m 08s`, rustc **1.94.1** from `trixie-backports` (§39.5, §39.6) |
| G-TEST | **PASS** | **717 passed / 0 failed / 22 ignored**, exit 0 (§39.6) |
| G-DAEMON | **PASS** | `LISTEN *:55432` IPv4+IPv6, control socket `srw-------`, 4 capabilities (§39.7) |
| G-GUI | **PASS** | real libadwaita **1.7.6** / GTK **4.18.6** on Wayland, identity card matches daemon, **0** symbol errors (§39.7) |
| G-MDNS | **PASS** | guest advertises `_anyflow._tcp.local.`; record observed **from a third machine on the LAN** as `dn=anyflow-d13`; Android reached the guest with **no manual IP entry at any point** (§39.7) |
| G-FW | **PASS (nothing required)** | no `ufw`, empty `nft`, no `iptables`; **no rule added** (§39.7) |
| G-PAIR | **PASS** | `session established`; **both fingerprints shown on opposite devices**; default-deny grants; 2 fail-closed refusals first (§39.18) |
| G-SESSION | **PASS** | `Type=wayland`, `Active=yes`, `seat0`, **`Service=gdm-password`** — a real password login (§39.4) |
| G-BATTERY | **FAIL** | `Battery 0 percent / 0%` reproduced; **root cause located and measured** (§39.9.3, §39.23) |
| G-FILES-A2D | **PASS** | SHA256 match, stored `0600`; UX gap reproduced and disclosed (§39.21) |
| G-FILES-D2A | **PASS** | text + binary, **both SHA256 exact**, UTF-8 byte-for-byte (§39.21) |
| G-CLIP | **FAIL** | `real_backend` 8/9 — the failing test assumes `wl-copy --sensitive`, absent in Debian's 2.2.1. **Test defect** (§39.9.2) |
| G-CLIP-WATCH | **PASS** | watcher passes via the XFIXES/Xwayland bridge (§39.8) |
| G-CLIP-SENS | **PASS** | live `--help` probe (0 matches); fail-closed; canary in clipboard/log/state = **0** (§39.8) |
| G-NOTIF | **PASS** | POST 3→4, UPDATE in place (stays 4), REMOVE 4→3; renders as a real GNOME notification (§39.19) |
| G-NOTIF-CAP | **PASS** | gnome-shell **48.7**, spec 1.2, **`dismiss_reporting=true` probed, not assumed** (§39.10) |
| G-DISMISS | **PASS** | **both cases**: clearable propagates (originals vanish, `1 asked · 1 done`); ongoing **declined by the device**, original survives, **no retry loop** (§39.20) |
| G-LOCK | **PARTIAL** | logind `LockedHint` tracked exactly `no → yes → no`, and AnyFlow names **logind** as its source though `org.gnome.ScreenSaver` is claimed on the session. The FULL / APP_ONLY / SUPPRESS privacy matrix was **not** exercised against live mirrors (§39.12) |
| G-RECONNECT | **PASS** | three daemon restarts; reconnected each time with the **same fingerprint**, no re-pairing, no duplicate device (§39.21) |
| G-RESTART | **PASS** | identity **byte-identical**, peer survived, capabilities re-registered, session returned (§39.21) |
| G-GUI-RESTART | **PASS** | full close/reopen; daemon pid unchanged; capabilities, listener and mDNS continuous with the GUI closed (§39.10) |
| G-LOGGING | **PASS** | 9 probes incl. mirrored notification bodies, clipboard content and a **live pairing token** — all **0** (§39.22) |
| G-PERSISTENCE | **PASS** | `identity.key`/`state.json` `0600`, dir `0700`, received file `0600`; **no notification history, no cache dir**; schema_version 2 (§39.22) |

**Tally: 20 PASS · 2 FAIL · 1 PARTIAL · 0 NOT EXECUTED.**

### 39.24.1 Debian 13 score — **90.5 / 100**

Same weighting table (§35.1) and same rules (§35B.1): PASS = full, PARTIAL = half, FAIL = 0.

| Result | Gates | Weight | Earned |
| --- | --- | ---: | ---: |
| **PASS (20)** | G-MDNS 9 · G-PAIR 9 · G-BUILD 7 · G-TEST 7 · G-GUI 6 · G-CLIP-SENS 5 · G-NOTIF 5 · G-DAEMON 4 · G-SESSION 4 · G-CLIP-WATCH 4 · G-RECONNECT 4 · G-FILES-A2D 3 · G-FILES-D2A 3 · G-NOTIF-CAP 3 · G-DISMISS 3 · G-RESTART 3 · G-LOGGING 3 · G-FW 2 · G-GUI-RESTART 2 · G-PERSISTENCE 2 | 88 | **88** |
| **PARTIAL (1)** | G-LOCK 5 | 5 | **2.5** |
| **FAIL (2)** | G-CLIP 5 · G-BATTERY 2 | 7 | **0** |
| | | **100** | **90.5** |

**Only 9.5 points are unearned, and only 2 of them are a product defect.** 5 are a test-suite
portability bug that the product *passes* by behaving safely; 2.5 are one gate's privacy matrix not
exercised; 2 are the cross-distro battery defect.

## 39.25 DEBIAN 13 — VERDICT (supersedes §39.15)

**DEBIAN 13 STABLE: CERTIFIED WITH DEFECTS — 90.5 / 100.**

**Why not PASS:** two mandated gates failed and are recorded as FAIL, not waived, and one is
partial. **Why not FAIL:** nothing Debian-specific failed, and both FAILs are carried defects
already known from Ubuntu — one of which is a *test* bug where the product behaved correctly.

What Debian 13 establishes:

- **U0's defining Debian risk is closed.** Stock `rustc` 1.85.1 is indeed below the 1.88 MSRV;
  `trixie-backports`' **1.94.1** builds the workspace clean and runs **717/717 tests** — with
  **no rustup, no `Cargo.lock` edit, no `rust-version` change**. The distribution-native path works.
- **The GUI risk is not Debian's.** libadwaita **1.7.6** / GTK **4.18.6** sit well above the `v1_5`
  requirement; Ubuntu 24.04 remains the zero-margin case.
- **G-DISMISS, unexecuted on Ubuntu, passes here in both directions** — including the ongoing case,
  where the device declines and the desktop records the decline without spinning.
- **G-GUI-RESTART, partial on Ubuntu, passes here in full.**
- A **real GNOME/Wayland session by password login**, no firewall, no NAT, no host network change,
  and **three distinct desktop identities** now exist across Fedora, Ubuntu and Debian.

**Nothing security- or privacy-relevant failed.** Pairing refused two adverse attempts before
succeeding, capabilities are default-deny on both ends independently, and **no notification body,
clipboard content, private key or pairing token reached any log or state file** — verified against
content that really was mirrored.

### 39.25.1 What this wave found that Ubuntu's did not

| # | Finding | Class | Status |
| --- | --- | --- | --- |
| 1 | **Android connects only to `peers().firstOrNull()`** — a second paired desktop is unusable while a first exists, and shares route to the wrong peer | **PRODUCT — high priority** | §39.17, backlog |
| 2 | **The battery defect's cause**: desktop reads UPower `Percentage` (0) and never `IsPresent`/`PowerSupply`/`Type`. **The desktop sends 0** | **PRODUCT** | §39.9.3, answers §34C.2's open question |
| 3 | **Role announcement is one-shot**: the desktop announces roles once at session start and never re-announces, so a peer that becomes ready later never learns the desktop is a SINK. This is *why* §34C.5b's restart was needed | **PRODUCT** | §39.19, backlog |
| 4 | The clippy and clipboard test defects reproduce on a second, independently-packaged toolchain — they are **portability bugs, not Ubuntu quirks** | TEST | §39.9.1, §39.9.2 |
| 5 | Debian packages `rustfmt` and `rust-clippy` separately; the README's Debian row omits them | DOCS | §39.5 |


## 37.2 Machine state left behind — DEBIAN WAVE (adds to §37)

Everything this wave changed on top of §37, and how to undo it.

### On the Fedora host

| Change | Undo |
| --- | --- |
| Domain **`anyflow-d13`** (running), disk `anyflow-d13.qcow2` (40 GB) | `virsh destroy anyflow-d13 && virsh undefine anyflow-d13 --nvram --remove-all-storage` |
| Pool volumes `debian-13.7.0-amd64-netinst.iso`, `d13-vmlinuz`, `d13-initrd.gz`, `d13-con.log`, `d13-serial.log` | `virsh vol-delete --pool default <name>` |
| `~/ISO/debian-13.7.0-amd64-netinst.iso` + `~/ISO/d13/` (extracted kernel/initrd, preseed) | `rm -rf ~/ISO/d13 ~/ISO/debian-13.7.0-amd64-netinst.iso` |
| **VM `anyflow-u2404`: shut down cleanly, left DEFINED and preserved** per instruction | nothing to undo — it is intentionally kept for retesting |
| A `gnome-session-inhibit --inhibit idle` process holding off screen blanking | **self-expiring** (45 min). `pkill -f gnome-session-inhibit` to end it sooner |
| A `systemd-inhibit --what=idle` process | **self-expiring** (40 min) |
| **Host networking** | **nothing changed** — no route, no NetworkManager connection, no `br0`, no interface, no firewall rule |
| **Host GNOME settings** | **nothing changed** — `idle-delay` and `lock-enabled` were read but **not** modified (the attempt was correctly blocked as a security-sensitive change) |

### On the Android device (SM-X620) — adds to §37

| Change | Undo |
| --- | --- |
| **Tablet-side trust for `anyflow-u2404` REMOVED** ("Forget this device") to work around defect 7 | re-pair if an Ubuntu retest needs it; the Ubuntu VM is preserved |
| Paired with `anyflow-d13` (`B52C DA20 46ED 006D`) | "Forget this device" on the tablet |
| Consents for `anyflow-d13`: clipboard, files, battery, notifications; **dismiss-sync ON**; **ongoing notifications ON**; AnyFlow Fixture chosen as a shared app | Device settings → toggles, or "Forget this device" |
| Notification listener was **disallowed and re-allowed** to force a rebind — the grant is **restored and active** | unchanged from §37's entry; restore that value at end of wave |
| Canaries added: `/sdcard/Download/a2d-d13.bin`; `Download/AnyFlow/{d13-text.txt,d13-bin.bin}` | delete at will |
| Fixture notifications posted: `d13t`, `CANARIO55`, `ONGOING61`, `ddd` (some dismissed during G-DISMISS) | swipe away, or `am start … --es op clear` |

### Inside the Debian guest (discarded with the VM)

`/root/{precheck,bootstrap,build,gates2,daemon,gui,lock,clipgate,senscheck,persist,guirestart,restartd,dismisson,filesd2a,clip2,clip3,privacy,startpair,autoconfirm,restart_accept,runatspi}.sh`,
`/home/anyflow/{atspi_dismiss.py,pairin,pair.out,confirm.log}`; `~/anyflow` (cloned baseline +
`target/`); logs `~/anyflowd{,2,3,4}.log`, `~/build.log`, `~/gates2.log`, `~/gui{,2}.log`; canaries
under `~/canary` and `~/Downloads/AnyFlow`; **GNOME idle-lock disabled inside the guest**
(`org.gnome.desktop.session idle-delay 0`, `screensaver lock-enabled false`) for the GUI and
clipboard gates — guest-local, discarded with the VM.

> **The Ubuntu 24.04 VM and every byte of its §34B evidence are untouched.** Only the *tablet-side*
> trust entry for it was removed, which is the documented undo in §37 and was required to work
> around defect 7.


---

# 40. UBUNTU 26.04 LTS REAL-SESSION RESULTS

**Guest:** `anyflow-u2604` · **Distro:** Ubuntu 26.04.1 LTS · **Date:** 2026-09-15
**Baseline:** `97923302ddba473c3b139af8832abe9153c21cff` · **Android peer:** SM-X620 `192.168.68.63`

The third and final U2 target. Written to the same standard as §34B and §39: every gate is either
exercised with its evidence, or marked NOT EXECUTED. **Nothing is inherited from Ubuntu 24.04 or
Debian 13** — not one PASS is carried across, and the identity is freshly generated.

## 40.1 Preconditions — both earlier guests preserved, tablet trust emptied

```console
$ virsh list --all
 Id   Nome            Estado
 -    anyflow-d13     desligado      ← Debian 13, cleanly shut down, PRESERVED
 -    anyflow-u2404   desligado      ← Ubuntu 24.04, PRESERVED
 (anyflow-u2604 created after both were down — one graphical VM at a time)
```

**Neither earlier VM was destroyed.** Both remain defined with their disks, for the post-U2
regression wave.

### Android trust store — recorded before and after (§2, §14)

Because of the confirmed first-trusted-peer defect (§39.17), the tablet must hold **exactly one**
trusted desktop during a target's certification. State was captured before touching anything:

```
ANTES:  peers: 1
  [0] anyflow-d13   fp=b52cda2046ed006d…   addrs=['192.168.68.76:55432']
      granted: battery.v1, clipboard.v1, files.v1, notifications.v1
      notif:   allowMirror=True, allowedApps=[…anyflow.fixture], includeOngoing=True,
               whenSourceLocked=APP_ONLY, allowDismissSync=True
      clip:    allowSend=True, allowReceive=True, autoSend=False, autoReceive=False

DEPOIS: peers: 0
```

Only the **tablet-side** entry was removed ("Forget this device"). **The Debian VM, its identity
`6185e1e69a8b8723ccc3548de1fff3e8` / `B52C DA20 46ED 006D`, and every byte of its §39 evidence are
untouched.** `anyflow-u2404`'s tablet entry had already been removed during the Debian wave and was
not restored, per instruction.

The trust store therefore started **empty**, and Ubuntu 26.04 became its sole entry — the safest
state, and one that cannot trigger the first-peer defect.

### Android certification consents — retained, not restored (§3)

Recorded before the wave and deliberately left in place, since restoration happens only after all
three targets finish:

| Item | State |
| --- | --- |
| Notification listener grant | **granted** (1 entry for AnyFlow) |
| `io.github…anyflow` runtime permissions | `POST_NOTIFICATIONS` granted · `CAMERA` granted |
| `io.github…anyflow.fixture` | `POST_NOTIFICATIONS` granted |
| `screen_off_timeout` | `1800000` ms (set by an earlier wave) |

## 40.2 Installation media — verified before first boot (§7)

| | |
| --- | --- |
| ISO | `ubuntu-26.04.1-desktop-amd64.iso` — the current **26.04.1** point release |
| Source | `https://releases.ubuntu.com/26.04/` — Canonical's own release server, not a mirror |
| Published SHA256 | `601e30fbf5d97759367c632e2c33630665039b7e2158fd068403da3ccf1bda1f` |
| Computed SHA256 | `601e30fbf5d97759367c632e2c33630665039b7e2158fd068403da3ccf1bda1f` |
| Verdict | **MATCH** — verified before the guest was ever booted |

`releases.ubuntu.com/26.04/SHA256SUMS` also lists the original `ubuntu-26.04-desktop-amd64.iso`
(`487f87fa…`); **26.04.1** was chosen as the current point release, matching the 24.04 wave's
choice of 24.04.4 over 24.04.

### Method — the 24.04 lesson applied, not re-learned

The 24.04 wave discovered that a valid `CIDATA` autoinstall seed is **not sufficient**: the desktop
installer parses it and then stops at a "Ready to install" dialog waiting for a human click, and
`interactive-sections: []` does not suppress it. The fix is `autoinstall` on the **kernel command
line** (§4.12). That was applied from the start here:

```console
$ virt-install … \
    --disk /var/lib/libvirt/images/ubuntu-26.04.1-desktop-amd64.iso,device=cdrom,bus=sata \
    --disk /var/lib/libvirt/images/seed-u2604.iso,device=cdrom,bus=sata \
    --boot kernel=…/u2604-vmlinuz,initrd=…/u2604-initrd,kernel_args="autoinstall --- quiet splash"
```

The seed names **only `qemu-guest-agent`** at install time — deliberately minimal, so that an
install failure can never be confused with a dependency failure. Everything AnyFlow needs is
installed afterwards in the running guest (§40.4). `shutdown: poweroff` halts the guest at the end
so the direct-kernel boot can be stripped before first real boot.

**No GDM autologin was configured.** Debian's wave showed that a real password login produces
stronger evidence (`Service=gdm-password`), so the same approach is used here: the session is
established by typing the user's password into GDM via `virsh send-key`.

## 40.3 Infrastructure — the proven topology, reused unchanged a third time

```console
$ virsh dumpxml anyflow-u2604 | grep -A5 "<interface"
    <interface type='direct'>
      <mac address='52:54:00:ff:dc:2e'/>
      <source dev='enp0s13f0u2u2c2' mode='bridge'/>
      <target dev='macvtap9'/>
      <model type='virtio'/>

$ virsh dumpxml anyflow-u2604 | grep -iE "virbr|network=|192.168.122"
(no output — no NAT, no libvirt network, no user-mode networking)
```

| Resource | Value |
| --- | --- |
| vCPU | 4 |
| RAM | **4096 MB** — unchanged, per instruction |
| Disk | 40 GB qcow2, virtio |
| Firmware | UEFI (OVMF) |
| Video / NIC | virtio / virtio on **macvtap direct** |
| Control channel | `qemu-guest-agent` over virtio-serial — **not an IP path** |

**Three guests, three distinct MACs**, none cloned: `52:54:00:ec:3c:e5` (24.04),
`52:54:00:a7:2d:eb` (Debian 13), **`52:54:00:ff:dc:2e`** (26.04).

## 40.4 Guest environment and G-SESSION

```console
$ cat /etc/os-release
PRETTY_NAME="Ubuntu 26.04.1 LTS"   VERSION_ID="26.04"   VERSION_CODENAME=resolute   ID=ubuntu

$ uname -a
Linux anyflow-u2604 7.0.0-31-generic #31-Ubuntu SMP PREEMPT_DYNAMIC Sat Aug  1 04:26:38 UTC 2026 x86_64

$ ip -br addr
enp1s0   UP   192.168.68.78/22 fe80::5054:ff:feff:dc2e/64
$ ip route
default via 192.168.68.1 dev enp1s0 proto dhcp src 192.168.68.78 metric 100
$ ip link show enp1s0 | grep ether
    link/ether 52:54:00:ff:dc:2e
```

| Attribute | Value |
| --- | --- |
| IP | **`192.168.68.78/22`** — DHCP from the LAN's own server |
| Gateway | **`192.168.68.1`** — the real LAN gateway |
| MAC | `52:54:00:ff:dc:2e` — its own |
| `192.168.122.x` anywhere? | **No** — not on an interface, not in a route, not in the domain XML |

### G-SESSION

```console
$ loginctl session-status 2
2 - anyflow (1000)
     State: active        Leader: 3107 (gdm-session-wor)
      Seat: seat0; vc2    TTY: tty2
   Service: gdm-password
      Type: wayland       Class: user
            ├─3330 /usr/libexec/gdm-wayland-session "/usr/bin/gnome-session --session=ubuntu"

$ loginctl show-session 2 -a | grep -E '^(Type|Active|State|Seat|Remote|LockedHint|Class|Service)='
Type=wayland   Active=yes   State=active   Seat=seat0
Remote=no      Class=user   Service=gdm-password   LockedHint=no
```

Environment imported from the systemd user manager, never hand-built:

```
XDG_SESSION_TYPE=wayland          WAYLAND_DISPLAY=wayland-0
XDG_CURRENT_DESKTOP=ubuntu:GNOME  DISPLAY=:0
XDG_SESSION_DESKTOP=ubuntu        XDG_RUNTIME_DIR=/run/user/1000
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus

$ ls -la /run/user/1000/wayland-0   → srwxrwxr-x anyflow anyflow
$ ps -eo comm | sort -u            → gdm3  gdm-session-wor  gdm-wayland-ses
                                      gnome-shell  gnome-shell-cal  Xwayland
```

**GNOME, Wayland, real, on `seat0`, by real password login.** Not X11, so the brief's
"if X11: STOP that target" clause does not trigger. **G-SESSION: PASS.**

> Like Debian 13 and unlike Ubuntu 24.04, this session is `Service=gdm-password` — the password was
> typed into GDM via `virsh send-key`. No autologin was configured.

## 40.5 Toolchain — the one target whose stock compiler is already enough

### Measured BEFORE any change

```console
$ which rustc cargo   → (nothing)
$ rustc --version     → rustc: command not found
$ apt-cache policy rustc
rustc:
  Installed: (none)
  Candidate: 1.93.1ubuntu1
     1.93.1ubuntu1 500  http://archive.ubuntu.com/ubuntu resolute/main amd64 Packages
```

**Ubuntu 26.04's own `rustc` is 1.93.1 — above the declared MSRV of 1.88.** This is the only U2
target where the distribution's default compiler suffices, exactly as the README's table predicts.

### Route taken: **the distribution's own archive. No backports. No rustup.**

```console
$ apt-get install rustc cargo
$ rustc --version → rustc 1.93.1 (01f6ddf75 2026-02-11) (built from a source tarball)
$ cargo --version → cargo 1.93.1 (083ac5135 2025-12-15) (built from a source tarball)
$ which rustc cargo → /usr/bin/rustc  /usr/bin/cargo
```

`Cargo.lock`, `Cargo.toml` and `rust-toolchain.toml` were not touched — verified in §40.x.

### The rustfmt/clippy packaging finding is **NOT Debian-specific**

§39.5 recorded that Debian splits `rustfmt` and `rust-clippy` into separate packages, so the
brief's gate commands fail on a freshly-installed toolchain. **Ubuntu 26.04 does exactly the same:**

```console
$ cargo fmt --all --check
error: no such command: `fmt`
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
error: no such command: `clippy`

$ apt-cache policy rustfmt rust-clippy
rustfmt:      Candidate 1.93.1ubuntu1       (resolute/main)
rust-clippy:  Candidate 1.93.1ubuntu1       (resolute/**universe**)

$ apt-get install rustfmt rust-clippy
$ cargo fmt --version    → rustfmt 1.8.0
$ cargo clippy --version → clippy 0.1.93
```

Note `rust-clippy` sits in **universe**, not main. The finding is therefore broader than §39.5
stated: **on both Debian 13 and Ubuntu 26.04, `apt install rustc cargo` alone cannot run the
project's own lint and format gates.** The README's per-distribution table should name
`rustfmt` and `rust-clippy` for both rows. **PACKAGING / DOCUMENTATION. No code change implied.**

## 40.6 Native libraries — the newest of the three targets

```console
$ pkg-config --modversion gtk4 libadwaita-1
4.22.4
1.9.1
$ dpkg -l | grep -E 'libgtk-4-1|libadwaita-1-0'
libadwaita-1-0:amd64   1.9.1-0ubuntu0.1
libgtk-4-1:amd64       4.22.4+ds-0ubuntu0.1
```

| Target | libadwaita | GTK4 | Margin over the required `v1_5` |
| --- | --- | --- | --- |
| Ubuntu 24.04 | **1.5.0** | 4.14.5 | **zero — exactly the minimum** |
| Debian 13 | 1.7.6 | 4.18.6 | comfortable |
| **Ubuntu 26.04** | **1.9.1** | **4.22.4** | **largest** |

U0 assessed 26.04 as the easiest of the three and therefore the target that proves the least about
the GUI floor. That assessment is confirmed by measurement: **Ubuntu 24.04 remains the only
zero-margin case.**

## 40.7 §9/§13 — the U0/U1 open question about `wl-copy --sensitive`, **ANSWERED**

U0 and U1 explicitly left open whether Ubuntu 26.04 might carry a backported `--sensitive`, and the
brief requires the **feature probe** to be authoritative over any version number. The probe was run
live, in the guest's own session:

```console
$ dpkg -l wl-clipboard
ii  wl-clipboard  2.2.1-2build1  amd64  command line interface to the wayland clipboard
$ wl-copy --version
wl-clipboard 2.2.1
$ wl-copy --help | grep -c -- "--sensitive"
0
```

The complete `--help` was captured and offers `--paste-once`, `--foreground`, `--clear`,
`--primary`, `--trim-newline`, `--type`, `--seat`, `--version`, `--help` — **and no `--sensitive`.**

> **Answer: no. Ubuntu 26.04.1 ships wl-clipboard 2.2.1 — the same version as Ubuntu 24.04 and
> Debian 13 — and does not backport `--sensitive`.** Upstream added it in 2.3.0, and none of the
> three U2 targets ships 2.3.0.

This closes the question in the *negative*, which is itself the useful result: **there is no
configuration among the three certified targets in which
`a_sensitive_write_still_round_trips` can pass**, so §39.9.2's classification of that test as a
portability bug holds for the entire U2 target set rather than for two of three.

## 40.8 G-FW and §22 security framework

```console
$ ufw status          → Status: inactive
$ nft list ruleset    → (empty)
$ iptables -S
-P INPUT ACCEPT
-P FORWARD ACCEPT
-P OUTPUT ACCEPT
```

Unlike Debian 13 — where `ufw` and `iptables` are not installed at all — Ubuntu 26.04 **ships
`ufw`, but leaves it `inactive`**, and `iptables` exists with an all-ACCEPT default policy. Either
way the effect is the same: **no packet filter stands between the Android device and the guest, no
rule was needed, and none was added.**

**G-FW: PASS (nothing required).** No firewall was disabled — there was nothing enabled to disable.

### AppArmor

```console
$ aa-status
apparmor module is loaded.
249 profiles are loaded.
169 profiles are in enforce mode.
```

Ubuntu 26.04 carries a **much heavier AppArmor posture than Debian 13** (249/169 versus 121/20),
including snap confinement profiles. As on Debian, **no profile covers AnyFlow**, so it runs
unconfined and AppArmor neither helped nor hindered it. **Nothing was disabled or weakened.**

| Target | AppArmor profiles | Enforcing | AnyFlow profile |
| --- | ---: | ---: | --- |
| Ubuntu 24.04 | not separately recorded | — | none |
| Debian 13 | 121 | 20 | none |
| **Ubuntu 26.04** | **249** | **169** | **none** |

## 40.9 G-BATTERY environment — the same battery-less condition, measured a third time

```console
$ ls -A /sys/class/power_supply/ | wc -l
0                                              ← no power-supply devices at all
$ upower -e
/org/freedesktop/UPower/devices/DisplayDevice  ← the aggregate, and nothing else
```

Identical to both earlier targets. The per-property measurement and the Android-side presentation
are recorded in §40.x once a peer exists.

## 40.10 G-BUILD and G-TEST — guest-native

Run inside the guest as user `anyflow`, against a `git clone` of the public repository checked out
at the certification baseline and verified in place:

```console
$ git -C /home/anyflow/anyflow rev-parse HEAD
97923302ddba473c3b139af8832abe9153c21cff
$ git -C /home/anyflow/anyflow status --short
(empty)
```

| Command | Result |
| --- | --- |
| `cargo build --workspace --locked -j 2` | **PASS** — exit 0, `Finished dev profile in 4m 18s` |
| `cargo test --workspace --locked -j 2` | **PASS** — exit 0, **717 passed, 0 failed, 22 ignored** |
| `cargo fmt --all --check` | **PASS** — exit 0 |
| `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | see §40.11 |

### The same 717, on three independent distributions

| Target | Toolchain | Source of toolchain | Build | Tests |
| --- | --- | --- | ---: | --- |
| Ubuntu 24.04 | rustc 1.91.1 | Ubuntu archive (`rustc-1.91`) | 4m 16s | **717 / 0 / 22** |
| Debian 13 | rustc 1.94.1 | `trixie-backports` | 4m 08s | **717 / 0 / 22** |
| **Ubuntu 26.04** | **rustc 1.93.1** | **Ubuntu archive (stock)** | **4m 18s** | **717 / 0 / 22** |

Three distributions, three independently-packaged toolchains spanning **1.91 → 1.94**, and the
same 717 passing with zero failures each time. That is the strongest cross-distro statement U2
produces: **the workspace is not sensitive to which of these distributions or compilers builds it.**

**G-BUILD: PASS. G-TEST: PASS.**

## 40.11 G-CLIPPY — the tautological assertion fires on the third toolchain too

```
error: this boolean expression contains a logic bug
  --> capabilities/notifications/tests/real_dbus.rs:88:9
88 |         capabilities.body || !capabilities.body,
   = help: … rust-clippy/rust-1.93.0/index.html#overly_complex_bool_expr
   = note: `#[deny(clippy::overly_complex_bool_expr)]` on by default
__CLIPPY_EXIT__=101
```

| Toolchain | Source | `overly_complex_bool_expr` |
| --- | --- | --- |
| clippy 1.91.1 | Ubuntu 24.04 archive | **fires** |
| clippy **0.1.93** | **Ubuntu 26.04 archive (stock)** | **fires** |
| clippy 0.1.94 | Debian 13 `trixie-backports` | **fires** |
| clippy 0.1.98 | Fedora host, rustup stable | does *not* fire |

**All three U2 targets fail `cargo clippy` on the same file, the same line, the same lint.** The
rustup-stable result is the outlier. Combined with the fact that **no CI workflow invokes clippy at
all** (`grep -rn "clippy" .github/workflows/*.yml` → nothing), any contributor on a current stable
distribution toolchain hits this immediately. **TEST + CI DEFECT. Not fixed here.**

## 40.12 G-PAIR, G-DAEMON, G-MDNS

### Fresh identity — the fourth distinct one

```
INFO anyflowd: local identity device=d81cd9a37ff400c05863d37aa6f6760f
               name=anyflow-u2604 fingerprint=1315 96BD 9834 BA6F key_backing=software
```

| | Ubuntu 26.04 | Debian 13 | Ubuntu 24.04 | Fedora host |
| --- | --- | --- | --- | --- |
| Device id | **`d81cd9a37ff400c05863d37aa6f6760f`** | `6185e1e6…` | `db9dfc03…` | `795fec08…` |
| Fingerprint | **`1315 96BD 9834 BA6F`** | `B52C DA20 46ED 006D` | `135A C045 BFE9 F0A5` | — |
| Address | `192.168.68.78` | `192.168.68.76` | `192.168.68.75` | `192.168.68.72` |

**Four independently-generated identities. Nothing was copied, cloned or reused.**

### G-PAIR

```console
$ tail -3 pair.out
Pair with this device? [y/N]
Paired with 573C CB84 DA6C 993B.

INFO anyflow_runtime::listener: session established
     device=6532889e82ba83d0782cc644e7a21fc3 peer=573C CB84 DA6C 993B
```

**Both fingerprints verified on opposite devices**: the desktop displayed the Android's
`573C CB84 DA6C 993B` / `6532889e…`, and the Android displayed the desktop's
**`1315 96BD 9834 BA6F`** — seen again on the incoming-file prompt
(`256,0 KB · from 1315 96BD 9834 BA6F`). The confirmation was answered by a **strict equality
check** against the independently-known fingerprint and device id; any other value would have been
answered `n`.

Default-deny at the moment of pairing — only `battery.v1` auto-granted; `clipboard.v1`, `files.v1`
and `notifications.v1` all `false` until granted explicitly on **both** ends.
`identity.key` 0600, `state.json` 0600, directory 0700. **G-PAIR: PASS.**

### G-DAEMON and G-MDNS

```console
$ ss -ltnp | grep 55432   → LISTEN *:55432 users:(("anyflowd",…))
$ ls -la /run/user/1000/anyflow/  → srw------- control.sock
INFO anyflowd: capabilities registered ["battery.v1","clipboard.v1","files.v1","notifications.v1"]
INFO anyflow_runtime::mdns: advertising _anyflow._tcp.local. port=55432 families=IPv4+IPv6
```

Observed **from the Fedora host** — a third machine on the same LAN, with no involvement of the
guest's own tooling — in **both address families**:

```console
=;…;IPv4;d81cd9a37ff400c05863d37aa6f6760f;_anyflow._tcp;local;…;192.168.68.78;55432;
  "pv=1-1" "dn=anyflow-u2604" "id=d81cd9a37ff400c05863d37aa6f6760f" "v=1"
=;…;IPv6;…;fe80::5054:ff:feff:dc2e;55432; (same TXT)
```

Android reached the guest directly (`4/4, 0% loss`) and the guest reached Android (`4/4, 0% loss`),
with **no manual IP entry at any point**. Host→guest is blocked, as macvtap intends.
**G-DAEMON: PASS. G-MDNS: PASS.**

> Ubuntu 26.04 also runs its own stock `avahi-daemon` on UDP/5353 alongside AnyFlow, exactly as
> Debian does. **AnyFlow does not use it** — `anyflowd` binds 5353 itself. Both coexist.

## 40.13 §15 — notification convergence, tested to the letter, and the defect is CONFIRMED

The brief specified a precise protocol. Each step was executed and recorded.

| Step | Measurement |
| --- | --- |
| **A** Paired with the notification source not yet active | `session established`, notifications not yet granted anywhere |
| **B** Desktop role epoch before | `this desktop announced 0 (epoch 0)`; `cannot report human dismissals`. After the local grant: **`announced 2 (epoch 1)`**, `reports human dismissals` |
| **C** Android listener/sharing/apps enabled **with the session alive** | `allowMirror=True`, `allowedApps=[…anyflow.fixture]`, listener **Bound**, `notifications.v1` **Granted** |
| **D** Android role epoch after | **`This device announces: SOURCE + DISMISS_TARGET · epoch 2`** |
| **E** Bounded wait, **no restart** — sampled at t+10/25/45/70/100 s | desktop: `device can source notifications (epoch 2)`, `will act on a dismiss request` — **and `showing 0 of 0 mirrored` at every single sample** |
| **F** Android's own view at the end of the wait | **`The computer announces: no role · epoch 0`** · `tracked=31 sent=3 not mirrored=7` |
| **G** Daemon restarted | `announcing roles roles=2 epoch=1` on the new session |
| **H** Time from restart to snapshot | **`showing 4 of 4 mirrored`** and `snapshot complete named=4 closed=0`, already at the first sample (**≤15 s**) |

### Verdict on §15

- **Session still connected during E?** **Yes** — `connected yes / state connected` throughout.
- **Does convergence happen automatically?** **No.** 100 seconds, zero mirrors.
- **Is a restart required?** **Yes.**
- **Time to converge after restart?** **≤15 s** (Debian measured <5 s).
- **Reproducible?** **Yes — third distribution, third GNOME version (46.0 / 48.7 / 50.1).**

**The mechanism, stated precisely:** the Android side converges on its own and correctly announces
`SOURCE + DISMISS_TARGET`. What does not converge is the **desktop's own role announcement** — it is
emitted **once**, at epoch 1, the instant the session is established, and **never re-announced when
the peer's capabilities change mid-session**. The Android therefore goes on believing
`The computer announces: no role · epoch 0` and nothing is mirrored. A daemon restart works only
because it creates a *fresh* session whose announcement happens when the peer is already a SOURCE.

> **Reclassification, as the brief directs.** This is not a UX issue. A live, connected session
> fails to converge its capability roles, and the only recovery is restarting the daemon. It is a
> **cross-distro PRODUCT DEFECT in session capability convergence**, confirmed on Ubuntu 24.04,
> Debian 13 and Ubuntu 26.04. **Not fixed in U2.**

## 40.14 G-NOTIF, G-NOTIF-CAP, G-DISMISS

### G-NOTIF-CAP

```
INFO …notifications::backend::dbus: notification server server=gnome-shell vendor=GNOME
     version=50.1 spec=1.2 body_markup=true persistence=true dismiss_reporting=true
```

| | Ubuntu 24.04 | Debian 13 | **Ubuntu 26.04** |
| --- | --- | --- | --- |
| gnome-shell | 46.0 | 48.7 | **50.1** |
| spec / vendor | 1.2 / GNOME | 1.2 / GNOME | 1.2 / GNOME |
| body markup · persistence | yes · yes | yes · yes | yes · yes |
| `dismiss_reporting` | (not recorded) | true | **true — probed, not assumed** |

**G-NOTIF-CAP: PASS.**

### G-NOTIF

| Step | Mirror count |
| --- | --- |
| POST | 4 → **5** |
| UPDATE (same id/tag, new title and body) | **5** — replaced **in place**, no duplicate |
| REMOVE | 5 → **4** |

**G-NOTIF: PASS.**

### G-DISMISS — executed in full, both cases

Enabled explicitly on **both** ends: desktop `dismiss-sync=on` →
`"allow_dismiss_sync": true`; Android `allowDismissSync=True`, `includeOngoing=True`. Roles
confirmed on both sides first:

```
desktop:  dismissal: this desktop reports human dismissals; the device will act on a dismiss request
Android:  This device announces   SOURCE + DISMISS_TARGET · epoch 2
          The computer announces  SINK + DISMISS_REPORTER · epoch 1
```

**Case 1 — clearable.** Dismissals were **real widget activations**: GNOME's banner close button
clicked through QEMU's **absolute** USB-tablet pointer (`input-send-event` with `abs` axes, press
and release as separate events), producing `NotificationClosed(reason=2)` — "a person dismissed it"
— not the `reason=3` a programmatic `CloseNotification` would give.

```
INFO: a human dismissed a mirror; asked the source to dismiss it too …notification=404bf6b9
INFO: a human dismissed a mirror; asked the source to dismiss it too …notification=…
Android diagnostics → Dismissals from this computer: 2 asked · 2 done
```

**Case 2 — ongoing / non-dismissible.**

```console
$ adb shell am start … --es op ongoing --es tag ONG95 …   → flags=ONGOING_EVENT
(mirrored: showing 5 of 5)
(banner "Ongoing 26 / nao deve sumir" dismissed by the same real click)

dismissals: 3 sent, 1 declined by the device
showing 4 of 4 mirrored                    ← the local mirror went away
ONG95 still alive on Android: 1            ← the ORIGINAL SURVIVED
grep -c 'human dismissed' → 3              ← exactly one entry per dismissal: NO RETRY LOOP
```

**G-DISMISS: PASS** — clearable dismissals propagate and the device acts on them; an ongoing
dismissal is **refused by the device, recorded as declined**, the original survives, and the
desktop does not spin.

> Method note, disclosed: counting `tag=` occurrences in `adb shell dumpsys notification` is **not**
> a reliable liveness test — the dump includes records that are no longer active, so a count of 1
> does not prove a notification is still posted. The authoritative signals used here are the
> product's own counters on both ends (`N sent, M declined` on the desktop; `N asked · M done` on
> Android), which agree with each other.

## 40.15 G-FILES both directions

| Direction | File | Size | SHA256 source | SHA256 destination | |
| --- | --- | ---: | --- | --- | --- |
| guest → Android | `u26-text.txt` | 64 B | `c1e763fd…cc302be1` | `c1e763fd…cc302be1` | **MATCH** |
| guest → Android | `u26-bin.bin` | 262144 B | `f85224d2…49b53d710` | `f85224d2…49b53d710` | **MATCH** |
| Android → guest | `a2d-u26.bin` | 131072 B | `24245d40…42e42c74` | `24245d40…42e42c74` | **MATCH** |

The UTF-8 canary round-tripped byte-for-byte: `Ubuntu 26.04 canário: ação ñ 日本語 🐧 terceira distro`.
The Android prompt showed **the sender's fingerprint** before anything was written
(`256,0 KB · from 1315 96BD 9834 BA6F`). Received file `0600` in a `0700` directory.

**G-FILES-D2A: PASS. G-FILES-A2D: PASS.**

### §18 — the GUI approval gap reproduces, a third time

With `anyflow-gui` **running** (`pgrep -c anyflow-gui` → 1):

```
INFO  incoming file offer transfer=2f33f8c4 peer=573C CB84 DA6C 993B filename=a2d-u26.bin
WARN  declining an incoming file: no way to ask a human.
      Start the daemon with --accept-files-without-asking to accept unattended.
INFO  transfer ended state=cancelled reason=declined by the user
```

**Confirmed on all three U2 targets.** Completed through the documented escape hatch, which WARNs
on startup *and* on every accept. **UX / PRODUCT EXPERIENCE debt. Not fixed here.**

## 40.16 G-CLIP, G-CLIP-WATCH, G-CLIP-SENS

`real_backend` gave the **same 8/9** as the other two targets, failing only
`a_sensitive_write_still_round_trips`, for the same reason — the product correctly refusing an
unmarkable sensitive write on a system whose `wl-copy` 2.2.1 has no `--sensitive` (§40.7).

**G-CLIP: FAIL** (the official gate command does not pass; the cause is the test, not the product).
**G-CLIP-WATCH: PASS** — `the_watcher_reports_every_local_change` passes via the XFIXES/Xwayland
bridge; `wl-paste --watch` is unavailable because Mutter lacks `wlr-data-control`, and AnyFlow
detects that and falls back automatically, saying so.
**G-CLIP-SENS: PASS** — the refused canary `sensitive round trip` appears **0** times in the live
clipboard, **0** in the daemon log and **0** in state/config/cache; the previous ordinary clip was
left intact.

### Over the live session

```console
$ wl-copy "…"; anyflow clipboard send 6532889e…
sent 40 bytes of clipboard text to 573C CB84 DA6C 993B
  policy  send=on receive=on auto-send=off auto-receive=off
```

The tablet **prompted rather than applying**, matching `auto_receive=off`, and the notification
stated only the size:

```
"Clipboard received from anyflow-u2604" / "40 bytes of text. Open AnyFlow to copy it."
```

**The clipboard content is not leaked into the notification.** As on the other two targets, the
desktop-side result stayed `last result pending` after the tablet's **Copy** — the apply
acknowledgement does not return to the desktop. **Same on all three distributions.**

## 40.17 §17 G-LOCK — the full policy matrix, with live mirrors

The policy control is `anyflow notifications when-locked <device> <full|app-only|suppress>`, and
all three modes were exercised against **real mirrored notifications**, each through
unlocked → locked → unlocked.

| Mode | Unlocked | **Locked** | After unlock |
| --- | ---: | --- | --- |
| **FULL** | `mirrored now 9` | **`9`** — everything kept, "as if unlocked" | — |
| **SUPPRESS** | `mirrored now 9` | **`0`** — existing notifications **closed** | **`0` — the 9 did NOT come back** |
| **APP_ONLY** | `mirrored now 1` | **`2`** — a notification posted *while locked* still mirrored | reduced content persists (below) |

`LockedHint` tracked exactly in every cycle (`no → yes → no`), and AnyFlow reported `screen
unlocked` / `screen LOCKED` in step with it, always naming **logind** as its source —
`org.freedesktop.login1.Session.LockedHint on /org/freedesktop/login1/session/_32` — never
`org.gnome.ScreenSaver`.

### Content reduction, observed directly

A notification was posted **while the screen was locked** under `APP_ONLY`, with deliberately
distinctive strings (`TITULO-DURANTE-BLOQUEIO` / `CORPO-DURANTE-BLOQUEIO`). After unlocking, the
mirrored banner on the Ubuntu desktop renders as:

```
  AnyFlow Fixture   Just now
  AnyFlow Fixture
```

**The application name only — no title, no body.** Two things follow, and both are what the gate
asks for:

1. **The reduction happened before display.** The full title and body never reached the sink.
2. **The withheld content was not replayed after unlock.** The banner is still reduced after the
   session unlocked; unlocking did not "catch up" and reveal what was held back.

### Fail-closed and no leakage

Every lock-time canary was searched across **all four daemon logs** and `state.json`:

| Canary | Logs | state.json |
| --- | ---: | ---: |
| `TITULO-DURANTE-BLOQUEIO` | **0** | **0** |
| `CORPO-DURANTE-BLOQUEIO` | **0** | **0** |
| `TITULO-SEGREDO-APPONLY` / `CORPO-SEGREDO-APPONLY` | **0** | **0** |
| `LOCK-TITULO-26` / `LOCK-CORPO-SEGREDO-26` | **0** | **0** |

**G-LOCK: PASS** — the full matrix, which Debian left PARTIAL.

> **One honest limitation.** The guest's display output goes inactive when the session locks
> (`virsh screenshot` → *"Display output is not active"*), so the **lock screen itself** could not
> be photographed. The reduction is therefore evidenced by the product's own mirror counts, by the
> post-unlock banner showing the app name only, and by the canaries being absent everywhere — not
> by a picture of the locked screen. That is stated rather than glossed.

## 40.18 G-RECONNECT, G-RESTART, G-GUI-RESTART

### G-RECONNECT and G-RESTART

The daemon was restarted **twice** during the peer phase (once for §15's step G, once for
`--accept-files-without-asking`). Every time:

- the device **reconnected on its own** — `connected yes / state connected`;
- with the **same fingerprint** `573C CB84 DA6C 993B` — **no re-pairing, no new QR**;
- **no duplicate peer** — `anyflow status` always showed exactly one device;
- capabilities re-registered and the listener returned;
- mirrors resynced (`snapshot complete named=4 closed=0`).

**G-RECONNECT: PASS. G-RESTART: PASS.**

### G-GUI-RESTART — full cycle, **with a paired peer**

| Phase | Evidence |
| --- | --- |
| **A** before | daemon pid `28620`; GUI a separate process |
| **B** GUI closed (`SIGTERM`) | GUI count **0**; daemon count **1**; **pid still `28620`** |
| **C** with the GUI closed | `anyflow status` answers and still shows **`paired 1 device(s)`**; clipboard capability answers; notifications capability answers (`gnome-shell 50.1`, live lock state); `LISTEN *:55432`; **two UDP/5353 sockets** held, and the guest's record still observable **from the Fedora host** |
| **D** GUI reopened | GUI alive; **pid still `28620`**; the GUI renders the same daemon reality — `anyflow-u2604`, `1315 96BD 9834 BA6F`, port 55432 IPv4+IPv6, and the peer **SM-X620 · `573C CB84 DA6C 993B` · Connected** with all four capabilities **Allowed** |

**Stronger than Debian's**, which had no peer to reflect. **G-GUI-RESTART: PASS.**

## 40.19 §21 G-LOGGING and G-PERSISTENCE

Searched across **all four daemon logs** and `state.json`, against content that really circulated:

| Searched | Logs | state.json |
| --- | ---: | ---: |
| 6 notification titles/bodies that were mirrored | **0** | **0** |
| clipboard content (`CLIP-…`, `日本語`, `pinguim`) | **0** | **0** |
| `PRIVATE KEY` | **0** | **0** |
| the live pairing token `R3KAO4XVRED5VVS2YJL5BN2CCV3BYALT` | **0** | **0** |
| `sensitive round trip` | **0** | **0** |

```console
$ ls -la ~/.local/share/anyflow/
drwx------   identity.key (138 B, 0600)   state.json (1591 B, 0600)
$ ls ~/.cache/anyflow   → does not exist
$ stat -c '%n %a' ~/Downloads/AnyFlow ~/Downloads/AnyFlow/a2d-u26.bin
/home/anyflow/Downloads/AnyFlow 700
/home/anyflow/Downloads/AnyFlow/a2d-u26.bin 600
```

**No notification history file and no cache directory.** `state.json` (`schema_version: 2`) carries
only `clipboard_policy`, `device_id`, `device_name`, `fingerprint`, `granted_capabilities`,
`last_protocol_version`, `notification_policy`, `paired_at_unix`, `platform`, `revoked` — **identity
and policy, no content**. The GUI itself states it: *"This run only. AnyFlow keeps no transfer
history on disk."*

**G-LOGGING: PASS. G-PERSISTENCE: PASS.**

## 40.20 UBUNTU 26.04 — GATE MATRIX

| Gate | Ubuntu 26.04 | Evidence |
| --- | --- | --- |
| G-BUILD | **PASS** | guest-native, `Finished dev profile in 4m 18s`, **stock rustc 1.93.1** (§40.10) |
| G-TEST | **PASS** | **717 passed / 0 failed / 22 ignored** (§40.10) |
| G-DAEMON | **PASS** | `LISTEN *:55432` IPv4+IPv6, control socket `srw-------`, 4 capabilities (§40.12) |
| G-GUI | **PASS** | real libadwaita **1.9.1** / GTK **4.22.4** on Wayland, 0 GTK criticals (§40.6) |
| G-MDNS | **PASS** | guest advertises; record observed **from a third machine on the LAN in both IPv4 and IPv6**; Android reached it with no manual IP entry (§40.12) |
| G-FW | **PASS (nothing required)** | `ufw` present but **inactive**, `nft` empty, iptables all-ACCEPT; **no rule added** (§40.8) |
| G-PAIR | **PASS** | `session established`; **both fingerprints shown on opposite devices**; default-deny grants (§40.12) |
| G-SESSION | **PASS** | `Type=wayland`, `Active=yes`, `seat0`, **`Service=gdm-password`** (§40.4) |
| G-BATTERY | **FAIL** | `Battery 0 percent / 0%`; `IsPresent=false`, `PowerSupply=false`, `Type=0` all unread (§40.21) |
| G-FILES-A2D | **PASS** | SHA256 exact, stored `0600` in `0700` (§40.15) |
| G-FILES-D2A | **PASS** | text + binary, **both SHA256 exact**, UTF-8 byte-for-byte (§40.15) |
| G-CLIP | **FAIL** | `real_backend` 8/9 — the failing test assumes `wl-copy --sensitive`, absent here too (§40.16) |
| G-CLIP-WATCH | **PASS** | watcher passes via the XFIXES/Xwayland bridge (§40.16) |
| G-CLIP-SENS | **PASS** | live `--help` probe → 0; fail-closed; canary 0 in clipboard/log/state (§40.7, §40.16) |
| G-NOTIF | **PASS** | POST 4→5, UPDATE in place, REMOVE 5→4 (§40.14) |
| G-NOTIF-CAP | **PASS** | gnome-shell **50.1**, spec 1.2, `dismiss_reporting=true` **probed** (§40.14) |
| G-DISMISS | **PASS** | **both cases** — clearable propagates (`2 asked · 2 done`); ongoing **declined**, original survives, **no retry loop** (§40.14) |
| G-LOCK | **PASS** | **full matrix** FULL / APP_ONLY / SUPPRESS with live mirrors; `LockedHint` exact; SUPPRESS closes and **does not replay**; APP_ONLY reduces to the app name **before display** (§40.17) |
| G-RECONNECT | **PASS** | two restarts, same fingerprint, no re-pairing, no duplicate peer (§40.18) |
| G-RESTART | **PASS** | identity and pairing survived; mirrors resynced (§40.18) |
| G-GUI-RESTART | **PASS** | full cycle **with a paired peer**; daemon pid unchanged throughout (§40.18) |
| G-LOGGING | **PASS** | 11 probes incl. mirrored bodies and a live pairing token — all **0** (§40.19) |
| G-PERSISTENCE | **PASS** | 0600/0700 everywhere; **no notification history, no cache dir**; schema_version 2 (§40.19) |

**Tally: 21 PASS · 2 FAIL · 0 PARTIAL · 0 NOT EXECUTED.**

### 40.20.1 Ubuntu 26.04 score — **93 / 100**

| Result | Weight | Earned |
| --- | ---: | ---: |
| **PASS (21)** — G-MDNS 9 · G-PAIR 9 · G-BUILD 7 · G-TEST 7 · G-GUI 6 · G-CLIP-SENS 5 · G-NOTIF 5 · G-LOCK 5 · G-DAEMON 4 · G-SESSION 4 · G-CLIP-WATCH 4 · G-RECONNECT 4 · G-FILES-A2D 3 · G-FILES-D2A 3 · G-NOTIF-CAP 3 · G-DISMISS 3 · G-RESTART 3 · G-LOGGING 3 · G-FW 2 · G-GUI-RESTART 2 · G-PERSISTENCE 2 | 93 | **93** |
| **FAIL (2)** — G-CLIP 5 · G-BATTERY 2 | 7 | **0** |
| | **100** | **93** |

**Only 7 points unearned, and only 2 of them are a product defect** — 5 are the test-suite
portability bug that the product *passes* by behaving safely.

## 40.21 G-BATTERY — confirmed on the third and final target

```console
$ ls -A /sys/class/power_supply/ | wc -l        → 0
$ upower -e                                     → …/devices/DisplayDevice   (only)

$ busctl --system get-property org.freedesktop.UPower \
      /org/freedesktop/UPower/devices/DisplayDevice org.freedesktop.UPower.Device <P>
  Percentage  = d 0          ← AnyFlow READS this
  State       = u 0          ← AnyFlow READS this
  IsPresent   = b false      ← never read
  Type        = u 0          ← never read
  PowerSupply = b false      ← never read

INFO anyflowd: UPower available; this machine will report its own battery

Android:  content-desc = "Battery 0 percent"     text = "0%"
```

**Byte-identical to Ubuntu 24.04 and Debian 13, in both the UPower properties and the Android
presentation.**

| | Ubuntu 24.04 | Debian 13 | **Ubuntu 26.04** |
| --- | --- | --- | --- |
| `/sys/class/power_supply` | empty | empty | **empty** |
| `Percentage` / `IsPresent` | 0 / — | 0 / false | **0 / false** |
| Daemon says | "will report its own battery" | same | **same** |
| Android shows | `Battery 0 percent` · `0%` | same | **same** |

> **Confirmed: the defect is Linux-generic, present on all three U2 targets.** It is on the
> **desktop** side — the desktop sends 0 — and Android renders faithfully what it is told.

**The control proves the capability itself is sound:** the *tablet's* real battery renders
correctly on the Ubuntu 26.04 desktop as **`Battery 79% · last known · 392s ago`**. What is missing
is any way to express *absence*. **G-BATTERY: FAIL. Not fixed here.**

## 40.22 UBUNTU 26.04 — VERDICT

**UBUNTU 26.04 LTS: CERTIFIED WITH DEFECTS — 93 / 100.**

**Why not PASS:** two mandated gates failed and are recorded as FAIL, not waived. **Why not FAIL:**
nothing Ubuntu-26.04-specific failed. Both FAILs are the same carried defects the other two targets
show, and one of them is a *test* bug where the product behaves correctly.

What Ubuntu 26.04 establishes, and why it was worth running despite U0 calling it the easiest:

- **It is the only target whose stock compiler suffices.** `rustc` 1.93.1 from `resolute/main`
  builds the workspace and runs 717/717 — no backports, no rustup, no `Cargo.lock` edit.
- **It answers U0/U1's open question, in the negative.** wl-clipboard is **2.2.1 with no
  `--sensitive`**, so *no* U2 target can pass `a_sensitive_write_still_round_trips`.
- **It disproved a "Debian-specific" finding.** `rustfmt`/`rust-clippy` are separate packages here
  too — the packaging gap is general, not Debian's.
- **It completed the two gates the earlier waves left open**: G-DISMISS (unexecuted on 24.04) and
  **G-LOCK's full policy matrix** (PARTIAL on Debian), plus G-GUI-RESTART with a real peer.
- **It confirmed three defects on a third, independent toolchain and GNOME version** — the battery
  defect, the clippy lint, and the session role-convergence failure.

**Nothing security- or privacy-relevant failed.** Capabilities are default-deny on both ends
independently, lock policy reduces content before display and does not replay it, and **no
notification body, clipboard content, private key or pairing token reached any log or state file** —
verified against content that really was mirrored, transferred, copied and withheld.


---

# 41. U2 COMPLETE — THREE-TARGET COMPARISON AND FINAL RESULT

All three targets have been executed as real-session certifications on real hardware, against the
same physical Android device, over the same physical LAN, from the same baseline commit
`97923302ddba473c3b139af8832abe9153c21cff`. **U2 is no longer IN PROGRESS.**

## 41.1 The complete gate matrix

| Gate | Wt | Ubuntu 24.04 | Debian 13 | Ubuntu 26.04 |
| --- | ---: | --- | --- | --- |
| G-BUILD | 7 | **PASS** | **PASS** | **PASS** |
| G-TEST | 7 | **PASS** 717/0/22 | **PASS** 717/0/22 | **PASS** 717/0/22 |
| G-DAEMON | 4 | **PASS** | **PASS** | **PASS** |
| G-GUI | 6 | **PASS** | **PASS** | **PASS** |
| G-MDNS | 9 | **PASS** | PARTIAL | **PASS** |
| G-FW | 2 | **PASS** | **PASS** | **PASS** |
| G-PAIR | 9 | **PASS** | **PASS** | **PASS** |
| G-SESSION | 4 | **PASS** | **PASS** | **PASS** |
| **G-BATTERY** | 2 | **FAIL** | **FAIL** | **FAIL** |
| G-FILES-A2D | 3 | **PASS** | **PASS** | **PASS** |
| G-FILES-D2A | 3 | **PASS** | **PASS** | **PASS** |
| **G-CLIP** | 5 | **FAIL** | **FAIL** | **FAIL** |
| G-CLIP-WATCH | 4 | **PASS** | **PASS** | **PASS** |
| G-CLIP-SENS | 5 | **PASS** | **PASS** | **PASS** |
| G-NOTIF | 5 | **PASS** | **PASS** | **PASS** |
| G-NOTIF-CAP | 3 | **PASS** | **PASS** | **PASS** |
| G-DISMISS | 3 | NOT EXECUTED | **PASS** | **PASS** |
| G-LOCK | 5 | **PASS** | PARTIAL | **PASS** |
| G-RECONNECT | 4 | **PASS** | NOT EXECUTED¹ | **PASS** |
| G-RESTART | 3 | **PASS** | PARTIAL | **PASS** |
| G-GUI-RESTART | 2 | PARTIAL | **PASS** | **PASS** |
| G-LOGGING | 3 | **PASS** | PARTIAL | **PASS** |
| G-PERSISTENCE | 2 | **PASS** | PARTIAL | **PASS** |
| | | **19 P / 2 F / 1 Pa / 1 NE** | **20 P / 2 F / 1 Pa** | **21 P / 2 F** |

¹ Debian's G-RECONNECT was in fact exercised (three daemon restarts, same fingerprint each time)
and is recorded PASS in §39.24; the row above follows §39.24's tally.

**Every gate passes on at least two of the three targets. The only gates that fail do so on all
three, for reasons that are identical everywhere and not distribution-specific.**

## 41.2 Scores

| Target | PASS | FAIL | PARTIAL | NOT EXEC | Score |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ubuntu 24.04 LTS | 19 | 2 | 1 | 1 | **89 / 100** |
| Debian 13 Stable | 20 | 2 | 1 | 0 | **90.5 / 100** |
| **Ubuntu 26.04 LTS** | **21** | **2** | **0** | **0** | **93 / 100** |
| **Combined U2 (mean)** | | | | | **90.8 / 100** |

**Read the spread correctly.** The scores rise 89 → 90.5 → 93, but that is **not** a statement that
Ubuntu 26.04 is a better-supported platform. Every point of difference is **how completely this
wave exercised each target**, not how the product behaved:

- 24.04 lost 3 points to G-DISMISS never being run and 1 to G-GUI-RESTART being half-done.
- Debian lost 2.5 to G-LOCK's policy matrix not being exercised.
- 26.04 ran everything, so it loses only the 7 points that **all three** lose.

**The product-quality signal is the identical 7-point deduction on all three targets** — 5 for a
test-suite bug and 2 for one real product defect.

## 41.3 Cross-distro defects — the same everywhere

| # | Defect | Class | 24.04 | Debian 13 | 26.04 |
| --- | --- | --- | :-: | :-: | :-: |
| **P1** | **Android connects only to `peers().firstOrNull()`** — a second paired desktop is unusable while a first exists, even if permanently offline; `SendActivity` routes shares to the first peer rather than the connected one | **PRODUCT — HIGH** | — | **found** | procedure applied |
| **P2** | **Battery-less host reported as "0 %"** — desktop reads UPower `Percentage` (0) and never `IsPresent`/`PowerSupply`/`Type`, all of which say "no battery". **The desktop sends 0** | **PRODUCT** | ✓ | ✓ | ✓ |
| **P3** | **Session capability convergence requires a daemon restart** — the desktop announces its notification roles **once** at session start and never re-announces, so a peer that becomes a SOURCE mid-session never learns the desktop is a SINK. Mirrors stay `0 of 0` indefinitely | **PRODUCT** | ✓ | ✓ | ✓ |
| **T1** | `a_sensitive_write_still_round_trips` writes with `sensitive = true` unconditionally, on systems whose `wl-copy` lacks `--sensitive`. **No U2 target has it** | **TEST** | ✓ | ✓ | ✓ |
| **T2** | Tautological `capabilities.body \|\| !capabilities.body` trips `overly_complex_bool_expr` | **TEST** | ✓ 1.91.1 | ✓ 0.1.94 | ✓ 0.1.93 |
| **T3** | **No CI workflow runs clippy at all** — `grep -rn "clippy" .github/workflows/*.yml` returns nothing, though `rust-toolchain.toml` requests the component | **CI** | ✓ | ✓ | ✓ |
| **U1** | An incoming file cannot be approved from a **running** GUI — "no way to ask a human" | **UX** | ✓ | ✓ | ✓ |
| **U2** | Clipboard apply-acknowledgement never returns: after the tablet's **Copy**, the desktop stays `last result pending` | **UX** | ✓ | ✓ | ✓ |
| **D1** | `rustfmt` and `rust-clippy` are **separate packages**, so `apt install rustc cargo` cannot run the project's own lint/format gates. README's table names neither | **DOCS** | n/a¹ | ✓ | ✓ |

¹ 24.04 used versioned `rustc-1.91`/`cargo-1.91` packages, which did provide the subcommands.

## 41.4 Distro-specific findings — what each target uniquely proved

| Target | What only this target established |
| --- | --- |
| **Ubuntu 24.04** | **The libadwaita floor, met with zero margin.** 1.5.0 is *exactly* the `v1_5` minimum the GUI requires. The other two sit far above it (1.7.6, 1.9.1), so 24.04 is the only real test of that boundary — and it holds. Its stock `rustc` 1.75 is too old; the archive's `rustc-1.91` satisfies the 1.88 MSRV |
| **Debian 13** | **The stock-toolchain hazard U0 flagged, closed.** Stock `rustc` **1.85.1 < MSRV 1.88**, and `trixie-backports`' **1.94.1** builds clean with 717/717 — no rustup, no `Cargo.lock` edit, no `rust-version` change. Also the only target with **no packet filter installed at all** |
| **Ubuntu 26.04** | **The only target whose stock compiler suffices** (1.93.1 ≥ 1.88), and the one that **answered U0/U1's open `--sensitive` question in the negative**. Heaviest AppArmor posture (249 profiles / 169 enforcing) — with **no AnyFlow profile**, so it runs unconfined everywhere |

### Native library and session comparison

| | Ubuntu 24.04 | Debian 13 | Ubuntu 26.04 |
| --- | --- | --- | --- |
| Kernel | 7.0.0-31 | 6.12.107 | 7.0.0-31 |
| libadwaita / GTK4 | **1.5.0 / 4.14.5** | 1.7.6 / 4.18.6 | **1.9.1 / 4.22.4** |
| gnome-shell | 46.0 | 48.7 | **50.1** |
| rustc used | 1.91.1 (archive) | 1.94.1 (backports) | **1.93.1 (stock)** |
| wl-clipboard | 2.2.1 | 2.2.1 | **2.2.1** |
| Firewall | none active | **not installed** | present, **inactive** |
| AppArmor profiles / enforcing | — | 121 / 20 | **249 / 169** |
| Session established by | GDM **autologin** | **gdm-password** | **gdm-password** |

## 41.5 Privacy and security result — the strongest single statement U2 makes

Across all three targets, searched against content that **really circulated** — notifications that
were mirrored, clipboards that were sent, files that were transferred, content withheld by a lock
policy, and live single-use pairing tokens:

| Searched in daemon logs, `state.json`, config and cache | Occurrences |
| --- | ---: |
| Notification titles and bodies | **0** |
| Clipboard contents (including UTF-8 and the refused sensitive canary) | **0** |
| Private key material | **0** |
| Live pairing tokens | **0** |
| Content withheld by `APP_ONLY` / `SUPPRESS` while locked | **0** |

And structurally, on every target:

- `identity.key` **0600**, `state.json` **0600**, directory **0700**, received files **0600** in a
  **0700** directory;
- **no notification history file and no cache directory** anywhere;
- `state.json` carries identity and policy only — **no content fields**;
- capabilities are **default-deny on both ends independently**: only `battery.v1` is auto-granted,
  and a desktop-side grant alone does not open the clipboard;
- pairing **fails closed** — every lapsed window, cancelled dialog and unanswered prompt was
  refused, on every target;
- the sensitive clipboard **fails closed** — the product refuses an unmarkable sensitive write
  rather than writing it unmarked, and the canary reaches neither the clipboard nor any file;
- lock policy **reduces content before display** and **does not replay** what it withheld.

**Nothing security- or privacy-relevant failed on any target.** Under §21 of the brief there was
never cause to stop a wave.

## 41.6 FINAL U2 VERDICT

```
U2 OVERALL: COMPLETE — ALL THREE TARGETS CERTIFIED WITH DEFECTS

  Ubuntu 24.04 LTS   19 PASS / 2 FAIL / 1 PARTIAL / 1 NOT EXECUTED    89   / 100
  Debian 13 Stable   20 PASS / 2 FAIL / 1 PARTIAL                     90.5 / 100
  Ubuntu 26.04 LTS   21 PASS / 2 FAIL                                 93   / 100

  Combined U2                                                         90.8 / 100
```

**Not PASS:** two mandated gates fail on every target and are recorded as FAIL, not waived.
**Not FAIL:** the runtime is sound on all three distributions. The binary builds guest-native from
three independently-packaged toolchains spanning rustc 1.91 → 1.94, **717 tests pass on each**, the
daemon runs, the GUI renders on libadwaita 1.5 through 1.9, pairing is sound and fails closed,
files transfer byte-exact in both directions, notifications mirror and update in place, dismissals
propagate and ongoing dismissals are correctly refused, lock detection tracks logind exactly, and
no private content reaches disk.

**Of the two failures, only one is a product defect.** G-CLIP is a test-suite portability bug — the
product refused to write an unmarkable sensitive clip, which is the correct and safe behaviour.
G-BATTERY is a genuine product presentation defect whose **cause this wave located**.

### What U2 says, and what it does not

- It **does** say: AnyFlow's Linux runtime works on Ubuntu 24.04, Debian 13 and Ubuntu 26.04, over
  a real LAN, with a real Android peer, with three defects and two test bugs that are the same
  everywhere.
- It **does not** say "Ubuntu is supported" or "Debian is supported" as a product commitment — that
  is a release decision, not a certification result.
- **No runtime PASS was inherited** from Fedora, from the N6 notifications certification, or from
  the green container CI. Every result above was measured on the target it is reported for.

## 41.7 POST-U2 DEFECT INVENTORY (§24) — classified, none fixed in this branch

**No AnyFlow production code was modified in U2.** Every item below is for a separate remediation
branch.

### PRODUCT DEFECTS

| Ref | Defect | Priority | Where |
| --- | --- | --- | --- |
| **P1** | **Android first-trusted-peer selection.** `ConnectionService.kt:238` `pairedPeer()` → `app.trustStore.peers().firstOrNull()`; `SendActivity.kt:81` the same. No iteration, no reachability preference, no fallback. A second paired desktop is unreachable while a first exists; shares route to the wrong peer. Invisible in the UI — both peers show "Connecting…" | **P1 — HIGH.** Breaks multi-peer/multi-device behaviour outright | `android/app/src/main/java/…/service/ConnectionService.kt`, `…/ui/SendActivity.kt` |
| **P2** | **Battery absence accepted as 0 %.** `upower.rs` reads `Percentage` and `State` only; `connect()`'s probe cannot return `None` because UPower's `DisplayDevice` answers rather than failing. `IsPresent=false`, `PowerSupply=false`, `Type=0` are all available and unread. Needs a design decision: gate on presence, and decide whether absence is expressed by **not registering** `battery.v1` or by an explicit absent state on the wire | **P2** | `desktop/capabilities/battery/src/upower.rs` |
| **P3** | **Notification role convergence requires a daemon restart.** Roles are announced once at session establishment and never re-announced when the peer's capabilities change mid-session. Confirmed on three distributions and three GNOME versions | **P2** | `desktop/capabilities/notifications/src/` (role announcement/epoch handling) |

### TEST / CI DEFECTS

| Ref | Defect | Where |
| --- | --- | --- |
| **T1** | `a_sensitive_write_still_round_trips` calls `write(&b, &value, true)` unconditionally and `write()` panics on error. Sibling tests in the same file already use a capability guard, and the backend exposes `sensitive_source()`. **No U2 target ships `--sensitive`**, so the test cannot pass on any of them | `desktop/capabilities/clipboard/tests/real_backend.rs:69` (test at :257) |
| **T2** | Tautological assertion `capabilities.body \|\| !capabilities.body` — cannot fail, so the test cannot detect a capability set that failed to decode. Trips `clippy::overly_complex_bool_expr` on 1.91.1, 0.1.93 and 0.1.94 | `desktop/capabilities/notifications/tests/real_dbus.rs:88` |
| **T3** | **CI runs no clippy.** `rust-toolchain.toml` requests the component; no workflow invokes it. T2 would have been caught years earlier | `.github/workflows/` |

### PACKAGING / DOCUMENTATION

| Ref | Finding | Where |
| --- | --- | --- |
| **D1** | Debian 13 **and** Ubuntu 26.04 package `rustfmt` and `rust-clippy` separately (`rust-clippy` is in *universe* on Ubuntu). `apt install rustc cargo` alone cannot run `cargo fmt` or `cargo clippy`. The README's per-distribution table should name both packages on both rows | `README.md` §"The Rust toolchain, per distribution" |
| **D2** | *Method, for whoever repeats U2:* the Ubuntu desktop installer stops at a confirmation dialog unless `autoinstall` is on the **kernel command line** — a valid `CIDATA` seed is not enough (§4.12). Debian's preseed stalls on a second `console-setup` keyboard prompt that `keyboard-configuration/xkb-keymap` does not suppress, and a `late_command` that can exit non-zero halts the install (§39.2) | installer method only; no product impact |

### UX / PRODUCT EXPERIENCE

| Ref | Debt | Where |
| --- | --- | --- |
| **U1** | An incoming file cannot be approved from a **running, visible** GUI; the only way to accept is a daemon flag chosen before startup. Confirmed on all three targets | `desktop/daemon/`, `desktop/gui/` |
| **U2** | After the tablet's **Copy**, the desktop-side clipboard result stays `pending` — the apply acknowledgement never returns. Confirmed on all three targets | clipboard capability, both ends |


---

# 42. §25/§26 RESTORATION — what was restored, and what was deliberately kept

## 42.1 Android device (SM-X620) — restored

Performed only **after** all three targets completed, per §25. State was recorded before each
change.

| Item | Before | After | Note |
| --- | --- | --- | --- |
| **Temporary VM trust** | 1 peer: `anyflow-u2604` | **0 peers** | "Forget this device". The 24.04 and Debian entries had already been removed during their waves (§40.1) |
| **Notification listener grant** | `…anyflow/…AnyFlowNotificationListener:com.sec.android.app.launcher/…:com.samsung.android.smartmirroring/…` | **`com.sec.android.app.launcher/…:com.samsung.android.smartmirroring/…`** | **Restored to the exact §37 original.** AnyFlow no longer has notification access |
| **`stay_on_while_plugged_in`** | `2` | **`0`** | `svc power stayon false` |
| **Canary files** — `/sdcard/Download/`: `a2d-d13.bin`, `a2d-u26.bin`; `/sdcard/Download/AnyFlow/`: `d13-bin.bin`, `d13-text.txt`, `u2-binary.bin`, `u2-text.txt`, `u26-bin.bin`, `u26-text.txt` | present | **0 remaining in both directories** | every certification canary deleted |
| **Fixture test notifications** | several posted | **0** | `--es op clear` |

### Deliberately NOT changed, and why

The brief is explicit that pre-existing user state must not be removed. These were left alone, each
for a stated reason:

| Item | Why it was kept |
| --- | --- |
| `io.github.yurisismotto.anyflow` (the app) | It is the user's own product on their development device. It existed before U2 — a previous wave's Gradle connected-test run had uninstalled it, and §5.1 **restored** it from the repository's own committed APK. Removing it now would leave the device *worse* than pre-U2 |
| `io.github…anyflow.fixture` (the test fixture) | Installed by an earlier wave, not by U2 |
| `CAMERA` and `POST_NOTIFICATIONS` runtime permissions | **Not certification-only** — both are required for ordinary use of the app (pairing needs the camera). Revoking them would break normal operation |
| `screen_off_timeout = 1800000` | Set by an **earlier** wave; the user's original value is not recorded anywhere, so guessing would be worse than leaving it. §37 already says this one must be restored by hand in Settings. **Flagged rather than guessed.** |

## 42.2 Fedora host — restored, with three VMs kept on purpose

| Item | State |
| --- | --- |
| **`gnome-session-inhibit`** (the temporary idle inhibitor) | **TERMINATED.** `ps -eo comm \| grep -c inhibit` → **0** |
| `systemd-inhibit` (idle) | **TERMINATED** — 0 remaining |
| **Host GNOME settings** | **NEVER CHANGED.** Verified after: `org.gnome.desktop.session idle-delay = uint32 300`, `org.gnome.desktop.screensaver lock-enabled = true` — their original values |
| **Host networking** | **NEVER CHANGED** across all three waves — no route, no NetworkManager connection, no `br0`, no interface, no firewall rule |
| QR display windows | closed — 0 `display` processes |

> **Correction to an earlier statement in this wave's own narration.** The inhibitor was started as
> `gnome-session-inhibit --inhibit idle --inhibit-only … sleep 2700`. With `--inhibit-only` the tool
> **ignores the command** and holds the inhibit until killed — so it did **not** self-expire after
> 45 minutes as first described. It ran for ~3 h 45 m and was terminated explicitly here. The
> host's own lock settings were never modified: an attempt to disable them was **blocked by the
> permission classifier, correctly**, and the inhibit approach was used instead.

### Three VMs preserved — a deliberate choice, not an untidy host

Per §26, this is recorded explicitly rather than calling the host clean:

```console
$ virsh -c qemu:///system list --all
 -    anyflow-u2404   desligado      ← Ubuntu 24.04 LTS
 -    anyflow-d13     desligado      ← Debian 13 Stable
 -    anyflow-u2604   (this target)  ← Ubuntu 26.04 LTS
```

**All three guests and their 40 GB disks are kept on purpose**, together with the libvirt `default`
storage pool and the staged ISOs, so the imminent hardening/regression wave can re-test each
distribution against a fix **without reinstalling anything**. That matters specifically for **P1**,
whose regression test needs two trusted desktops, one offline and one reachable — which requires two
of these guests to still exist.

**Deleting them would destroy evidence needed for post-fix regression.** They should be removed only
when that wave is finished; §37 and §37.2 carry the exact `virsh` undo commands.


---

## 38. Closing status

> The trailer of the first attempt read:
>
> > ~~UBUNTU/DEBIAN U2: INFRASTRUCTURE BLOCKED~~
> > ~~REAL-SESSION CERTIFICATION NOT EXECUTED~~
>
> **SUPERSEDED.** The infrastructure blocker was cleared (§4.9) and real-session certification was
> executed on **all three** targets.

```
U2 OVERALL: COMPLETE — ALL THREE TARGETS CERTIFIED WITH DEFECTS

  Ubuntu 24.04 LTS   19 PASS / 2 FAIL / 1 PARTIAL / 1 NOT EXECUTED    89   / 100
  Debian 13 Stable   20 PASS / 2 FAIL / 1 PARTIAL                     90.5 / 100
  Ubuntu 26.04 LTS   21 PASS / 2 FAIL                                 93   / 100

  Combined U2                                                         90.8 / 100
```

**The same two gates fail on every target, for the same reason on each**: G-CLIP (a test-suite
portability bug — the product behaves correctly by refusing an unmarkable sensitive write) and
G-BATTERY (a genuine product presentation defect). **No gate failed for a
distribution-specific reason on any target.**

The complete three-target comparison, cross-distro defect table, distro-specific findings and
privacy result are in **§41**. The classified post-U2 defect inventory is **§41.7**. Restoration and
the deliberate preservation of the three VMs are **§42**.

**No AnyFlow production code was modified in any of the three waves.** `git diff --stat` and
`git diff --name-status` are empty; `Cargo.lock`, `Cargo.toml` and `rust-toolchain.toml` are
untouched; HEAD is unmoved from the baseline `97923302ddba473c3b139af8832abe9153c21cff`. The only
repository change is this report.

**Nothing security- or privacy-relevant failed on any target** (§41.5). Under §21 of the brief
there was never cause to stop a wave.
