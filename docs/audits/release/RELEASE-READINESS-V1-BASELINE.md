# OmniBridge — Release Readiness v1, baseline audit

| Field | Value |
| --- | --- |
| **Branch** | `feature/release-readiness-v1` |
| **Baseline** | `026d47b` (merge of PR #58, Packaging v1 final certification) |
| **Date** | 2026-09-22 |
| **Host** | Fedora 44 Workstation, GNOME 50.5 Wayland, `uid=1000(yuri) groups=yuri,wheel,libvirt` |
| **Mode** | **Read-only.** No product code, packaging file, workflow or historical document was modified. No VM was started, no package installed, no host network changed. |
| **Verdict** | **RELEASE READINESS V1: BASELINE ESTABLISHED — 3 release-blocking debts open, 1 of them newly reclassified as reachable** |

This audit answers one question: *what stands between OmniBridge and Release
Candidate certification, and which of it can this environment actually close?*

It does **not** close anything. Everything below is either quoted from an
existing report or measured read-only on this host today.

---

## 0. Executive summary

Packaging v1 closed with **`PACKAGING V1: CERTIFIED WITH EXPLICIT NON-BLOCKING
DEBTS`** and named two debts that block a public release — **D-1** (eleven
lifecycle gates never ran) and **D-2** (nothing is signed) — plus **D-3**
(three distributions have no runtime gate at all), which blocks the *claim of
support* rather than the release itself.

This baseline re-measured the environment those debts were declared against.
**One of the two stated causes of D-1 is no longer true, and was arguably never
true.**

| Finding | Consequence |
| --- | --- |
| **N-1** | **`qemu:///system` is usable without root, and always was.** Lifecycle certification blocker **B-2** says *"Bridged or macvtap networking needs root."* The invoking user is in the **`libvirt`** group, so `virsh -c qemu:///system` succeeds with no `sudo`, no password and no polkit prompt — measured in §3.1. The privileged half is done by `virtqemud`, not by the caller. |
| **N-2** | **Three LAN-capable VMs are already defined, installed and powered off.** `anyflow-d13`, `anyflow-u2404` and `anyflow-u2604` each carry `<interface type='direct'><source dev='enp0s13f0u2u2c2' mode='bridge'/>` — **macvtap over the wired NIC**, the exact topology B-2 says is unreachable — plus an `org.qemu.guest_agent.0` virtio-serial channel and 20–25 GB of allocated disk. §3.2. |
| **N-3** | **The real LAN blocker is a cable, not a privilege.** `enp0s13f0u2u2c2` reports `Link detected: no`. The host's only carrier is Wi-Fi, over which macvtap cannot work. This is the same blocker [U2 §4.3](../linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md) hit and cleared *with one cable* on 2026-09-15. §3.3. |
| **N-4** | **Root inside a guest substitutes for root on the host for most of D-1.** U2 established the method: drive the guest through `virsh qemu-agent-command … guest-exec` (root, no credentials, no IP path) and observe it with `virsh screenshot` against the real framebuffer. Six of the eleven blocked gates need no LAN at all and become reachable **today, with no operator action**. §5. |
| **N-5** | **The two published gate counts disagree with the gate table they summarise.** §4.1. The enumerated table is 15 PASS · 2 PARTIAL · 9 BLOCKED; the executive summary says 13 · 2 · 11. Both are in the source document and the difference is reconcilable — but it is not arithmetic, it is a reclassification, and it was never stated as one. |

**Nothing here contradicts a measurement.** Every gate the lifecycle
certification measured still stands. What changed is the *reason* eleven gates
did not run: it was recorded as a privilege wall, and it is in fact one
physical cable plus a method that had already been proven two waves earlier in
this same repository.

---

## 1. Scope and method

### 1.1 Documents read in full

| Document | What was taken from it |
| --- | --- |
| [`certification/linux/PACKAGING-V1-FINAL-CERTIFICATION.md`](../../certification/linux/PACKAGING-V1-FINAL-CERTIFICATION.md) | the verdict, the fourteen debts D-1…D-14, the signing status §15 |
| [`certification/linux/PACKAGING-V1-LIFECYCLE-CERTIFICATION.md`](../../certification/linux/PACKAGING-V1-LIFECYCLE-CERTIFICATION.md) | the 26-gate matrix, blockers B-1 and B-2, per-distribution applicability |
| [`certification/security/SECURITY-CERTIFICATION-V1.md`](../../certification/security/SECURITY-CERTIFICATION-V1.md) | the 16 SEC gates, gap G-1, finding F-1, §13's eight exclusions |
| [`audits/packaging/PACKAGING-V1-RELEASE-CI.md`](../packaging/PACKAGING-V1-RELEASE-CI.md) | the 14-artifact set, the immutable-input proof, gates RC-SIGN-01…03 |
| [`audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U0.md`](../linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U0.md) §21.1.1 | the Wi-Fi bridging impossibility and the ranked options that follow from it |
| [`audits/linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`](../linux-compat/LINUX-UBUNTU-DEBIAN-COMPAT-U2.md) §4.9–§4.12 | the cable being cleared, `qemu:///system` without root, the guest-agent method |

### 1.2 What "read-only" meant here

Every command in §3 is an enumeration. `virsh list`, `virsh dumpxml`,
`virsh pool-list`, `ip`, `ethtool`, `adb devices`, `gdbus introspect`,
`sha256sum`. **No domain was started.** No `virsh` command that mutates a
domain, pool, volume or network was issued. The trust store was hashed, not
touched.

---

## 2. The three release-blocking debts, as Packaging v1 left them

Quoted from [`PACKAGING-V1-FINAL-CERTIFICATION.md`](../../certification/linux/PACKAGING-V1-FINAL-CERTIFICATION.md) §14:

| # | Debt | Blocks a public release? |
| --- | --- | --- |
| **D-1** | Eleven lifecycle gates never ran — the graphical half | **Yes**, for calling any distribution runtime-certified |
| **D-2** | Artifacts are unsigned | **Yes** |
| **D-3** | Ubuntu 24.04, Ubuntu 26.04 and Debian 13 have no runtime gate at all | Yes, for claiming support beyond "it builds" |

The other eleven debts (**D-4** … **D-14**) are each recorded as **not**
blocking a public release, and this audit **does not reopen any of them**.
They are carried forward unchanged and listed in §9 so that the RC readiness
audit inherits a complete ledger rather than a filtered one.

**SEC-LOG-03** is not among D-1…D-14 because Security Certification v1 records
it as closed. §6 explains why it is nevertheless on this wave's list.

---

## 3. Environment re-measurement — the part that changes the plan

### 3.1 Privilege

```console
$ sudo -n true
sudo: uma senha é necessária          # B-1 still holds: no non-interactive root on the host

$ id
uid=1000(yuri) gid=1000(yuri) grupos=1000(yuri),10(wheel),985(libvirt)

$ virsh -c qemu:///system list --all
 Id   Nome              Estado
 -    anyflow-d13       desligado
 -    anyflow-f44-kde   desligado
 -    anyflow-u2404     desligado
 -    anyflow-u2604     desligado
                                       # exit 0 — no sudo, no password, no polkit prompt
```

**B-1 is confirmed and unchanged.** There is no non-interactive `sudo` on this
host, so nothing can be installed into Fedora 44 itself.

**B-2 as written is contradicted.** Bridged and macvtap networking do not need
the *caller* to be root; they need `virtqemud` to be root, and it is. U2 §4.10
measured exactly this on 2026-09-15 and recorded it as *"one assumption in the
first attempt was wrong in the host's favour."* The lifecycle certification,
written seven days later, restated the superseded assumption.

### 3.2 The staged guests

```console
$ virsh -c qemu:///system dumpxml anyflow-u2404 | sed -n '/<interface/,/<\/interface>/p'
    <interface type='direct'>
      <mac address='52:54:00:ec:3c:e5'/>
      <source dev='enp0s13f0u2u2c2' mode='bridge'/>
      <model type='virtio'/>
    </interface>
```

| Domain | Interface | Guest agent | Disk allocated | Firmware |
| --- | --- | --- | --- | --- |
| `anyflow-d13` | macvtap bridge on `enp0s13f0u2u2c2` | **`org.qemu.guest_agent.0`** | 20.5 GB / 40 GB | UEFI secboot, q35 |
| `anyflow-u2404` | macvtap bridge on `enp0s13f0u2u2c2` | **`org.qemu.guest_agent.0`** | 25.7 GB / 40 GB | UEFI secboot, q35 |
| `anyflow-u2604` | macvtap bridge on `enp0s13f0u2u2c2` | **`org.qemu.guest_agent.0`** | 23.2 GB / 40 GB | UEFI secboot, q35 |
| `anyflow-f44-kde` | bridge `br-anyflow-kde` — **the bridge does not exist** | **absent** | 34.4 GB / 32 GB | i440fx |

The first three are Debian 13, Ubuntu 24.04 and Ubuntu 26.04 with real
installed desktops, a control channel, and the correct network model already
written into their domain XML. They have simply never been started for a
packaging gate.

`anyflow-f44-kde` is the exception and is **not** usable as-is: its bridge was
a transient software AP built by the KDE certification
([`kde/KDE-PLASMA-REAL-CERTIFICATION-V1.md`](../../certification/linux/kde/KDE-PLASMA-REAL-CERTIFICATION-V1.md) §831–§835),
it was torn down, and the domain has no guest agent.

### 3.3 The cable

```console
$ ip -br addr
lo               UNKNOWN  127.0.0.1/8 ::1/128
wlp0s20f3        UP       192.168.68.73/22 fe80::26c0:ae6f:206d:8e3/64
enp0s13f0u2u2c2  DOWN
virbr0           DOWN     192.168.122.1/24

$ ethtool enp0s13f0u2u2c2 | grep -i 'link detected'
	Link detected: no
```

The host's only carrier is **Wi-Fi**. U0 §21.1.1 states the constraint and it
has not changed: *"802.11 associates one MAC per client, and most access
points drop frames whose source MAC is not the associated station's — so a
Linux bridge or a macvtap over `wlp0s20f3` will see the guest's traffic leave
and nothing come back. This is not a libvirt setting that can be changed; it
is how the link layer works."*

U0 ranked the options and U2 executed the recommended one. Re-stated here
because it is the single decision this wave turns on:

| Option | Verdict, unchanged from U0 §21.1.1 |
| --- | --- |
| **Plug `enp0s13f0u2u2c2` into the LAN** | **Recommended.** Already present, already wired into three domains, just has no carrier |
| macvtap in bridge mode on the wired NIC | **Already configured** (§3.2) — this *is* the topology waiting for the cable |
| A second physical machine instead of a VM | Cleanest evidence of all |
| libvirt NAT + port forwarding | **Rejected** — multicast does not cross NAT, so mDNS and pairing gates would prove nothing |
| Wi-Fi bridge via 4-address / WDS | **Rejected** — depends on AP support that is usually absent |

### 3.4 Observation and input

| Capability | State | Unlocks |
| --- | --- | --- |
| `virsh screenshot` | available — captures the guest's real framebuffer | **L6, L9** visual gates, inside a guest |
| `virsh send-key` | available | guest keyboard input where a gate needs it |
| `org.gnome.Shell.Screenshot` on the **host** session bus | **present** (`Screenshot`, `ScreenshotWindow`, `ScreenshotArea` introspected) | host-side visual evidence, if a host gate ever becomes installable |
| `adb` | `RX2Y500C7SY  device  model:SM_X620` — attached | physical-Android gates |
| `grim`, `gnome-screenshot`, `spectacle`, `wtype`, `ydotool`, `xdotool` | **all absent** | — host-side capture must go through the GNOME Shell D-Bus API |

### 3.5 Resources

```console
$ free -m
               total        used        free      shared  buff/cache   available
Mem:           15637        6989         341         999        9702        8648
$ df -h /var
/dev/mapper/luks-…  475G  304G  169G  65% /
```

**8.6 GB available**, against the 5.6 GB U2 had to plan around. A 4 GB guest —
U2's measured choice — fits with headroom. 169 GB free disk. **One graphical
VM at a time** remains the operating rule.

### 3.6 Product state, untouched

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json

$ pgrep -a omnibridged
691427 ./target/release/omnibridged --log info

$ rpm -q omnibridge omnibridge-gui
o pacote omnibridge não está instalado
o pacote omnibridge-gui não está instalado
```

Both digests are **identical to the values recorded before Packaging v1 Phase
1** and re-verified at its close. Pairing with the SM-X620 is intact. The
running daemon is the development build; **no OmniBridge package is installed
on this host**, which is why every host-level packaged gate is blocked.

---

## 4. D-1 — the lifecycle gate matrix

### 4.1 Reconciling the published counts (finding N-5)

The lifecycle certification's gate table, counted mechanically:

```console
$ awk '/^## 1\. Gate matrix/,/^### Per-distribution/' …LIFECYCLE….md \
    | grep -E '^\| \*?\*?L[0-9]+' | …
PASS              15    (L1 L2 L4 L5 L7 L11 L13 L17 L18 L21 L22 L23 L24 L25 L26)
PARTIAL            2    (L10 L12)
BLOCKED            9    (L3 L6 L8 L9 L14 L15 L16 L19 L20)
TOTAL             26
```

The same document's summary line says **13 PASS · 2 PARTIAL · 11 BLOCKED**,
and its header says **13 of 26 certified, 13 BLOCKED**. Three counts, one
table.

They reconcile, and the reconciliation is in blocker **B-1**'s own scope line:
*"L1–L3, L6, L8, L9, L14–L16, L19, L20 as **packaged-system** gates"* — eleven
gates, including **L1 and L2**. L1 and L2 passed **in containers** and the
document says plainly that *"a container is not a desktop."* Counted as
container gates they pass; counted as packaged-system gates they are blocked.

**This baseline adopts the stricter reading — eleven gates to close** — because
it is the one the release brief uses and the one that matches what a user
installing a `.deb` on a desktop actually exercises. The distinction is
recorded rather than smoothed over: it is a reclassification, not an error in
any measurement.

### 4.2 The eleven gates, their prerequisites, and their classification

| Gate | What it requires | Root needed | LAN needed | Visual | Phone | **Class** | Reachable now? |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **L1** clean install on a real system | install the package as root on an installed desktop | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |
| **L2** installed files manifest | as L1, then enumerate owned paths | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |
| **L3** daemon autostart after login | L1, then `systemctl --user enable` + a login cycle | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |
| **L6** GUI launches from the application menu | L1, then click the entry and see the OmniBridge icon | ✔ *(guest)* | — | **✔** | — | **HUMAN-OPERATOR** → *substitutable by `virsh screenshot`* | **Yes — in a guest** |
| **L8** activation immediately after install into a **live** session | L1 into an already-running session, then activate over the session bus | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |
| **L9** tray integration — one correct icon, 3 menu items | L1 + a tray host + somebody looking | ✔ *(guest)* | — | **✔** | — | **HUMAN-OPERATOR** → *substitutable by `virsh screenshot`* | **Yes — in a guest** |
| **L14** clipboard smoke | L1 + a paired peer on the same L2 segment | ✔ *(guest)* | **✔** | — | **✔** | **LAN/VM + PHYSICAL-ANDROID** | **No — needs the cable** |
| **L15** files smoke | L1 + a paired peer | ✔ *(guest)* | **✔** | — | **✔** | **LAN/VM + PHYSICAL-ANDROID** | **No — needs the cable** |
| **L16** notifications smoke | L1 + a paired peer | ✔ *(guest)* | **✔** | — | **✔** | **LAN/VM + PHYSICAL-ANDROID** | **No — needs the cable** |
| **L19** logout / login | L1, then a full session cycle | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |
| **L20** reboot | L1, then reboot the certifying machine | ✔ *(guest)* | — | — | — | **ROOT-PRIVILEGE** | **Yes — in a guest** |

And the two **PARTIAL** gates:

| Gate | Why it is partial | What would make it full | **Class** | Reachable now? |
| --- | --- | --- | --- | --- |
| **L10** mDNS discovery | advertised and measured, but by the **development** daemon, not a packaged install | the same measurement against a packaged install on the LAN | **LAN/VM** | **No — needs the cable** |
| **L12** Android physical discovery | the phone is paired and connected, but to the **development** daemon | discovery and pairing against a packaged install on the LAN | **LAN/VM + PHYSICAL-ANDROID** | **No — needs the cable** |

**Six of the eleven blocked gates — L1, L2, L3, L6, L8, L9 — plus L19 and L20,
that is eight of eleven, need no LAN.** They need an installed desktop with
root, which is precisely what an already-installed guest provides through
`guest-exec`. They are reachable **with no operator action at all**.

The remaining three (**L14, L15, L16**) and both PARTIAL gates (**L10, L12**)
need the guest to be visible to the phone on one L2 segment. That is the
cable, and only the cable.

### 4.3 D-3 — the per-distribution runtime matrix

The lifecycle certification's applicability table shows **no distribution has
a complete column.** Ubuntu 24.04, Ubuntu 26.04 and Debian 13 have *no* host
runtime evidence; Fedora 44 has host evidence for L4, L5, L7, L11, L18 only.

The three staged guests are exactly those three distributions. Closing D-1
inside them closes D-3 for them in the same pass. **Fedora 44 as a *packaged*
runtime target has no guest** — `anyflow-f44-kde` is KDE, has no guest agent,
and its network no longer exists. Fedora's host-level gates were taken on this
workstation and cannot be extended without B-1.

---

## 5. What R1 can close, and what it cannot

| Scenario | Gates closed | Operator action |
| --- | --- | --- |
| **A — nothing changes** | L1, L2, L3, L6, L8, L9, L19, L20 in up to three guests; D-3 for the non-LAN half | **none** |
| **B — the Ethernet cable is plugged in** | **A**, plus L10, L12, L14, L15, L16 against a packaged install; D-3 in full for the three guest distributions | **one cable** |
| **C — a host root password is supplied** | **B**, plus the Fedora 44 packaged-desktop column on this workstation | cable + password |

**Scenario B closes D-1 as written.** Scenario C is what it would take to also
call *Fedora 44* runtime-certified from a package rather than from a
development build.

### 5.1 OPERATOR ACTION REQUIRED — LAN carrier for the certification guests

> **Target machine** — the Fedora 44 host, `192.168.68.73/22`.
>
> **Action** — connect the USB Ethernet adapter `enp0s13f0u2u2c2`
> (MAC `6c:1f:f7:29:4c:c3`, altname `enx6c1ff7294cc3`) to the **same physical
> LAN the SM-X620 is on** — the network serving `192.168.68.0/22`. A cable
> from the adapter to the router or to any switch on that network.
>
> **Expected observation**
> ```console
> $ ethtool enp0s13f0u2u2c2 | grep -i 'link detected'
> 	Link detected: yes
> $ ip -br addr show enp0s13f0u2u2c2
> enp0s13f0u2u2c2  UP  192.168.68.x/22 …
> ```
>
> **Evidence to paste back** — the output of both commands above.
>
> **How to verify it succeeded** — `Link detected: yes`, and an address in
> `192.168.68.0/22` (not `169.254.x.x`, which would mean carrier but no DHCP).
>
> **Rollback** — unplug the cable. Nothing is configured, no NetworkManager
> profile is edited, no route is added. The three guest domains already name
> this interface, so no domain XML changes either.
>
> **Why it is needed** — L10, L12, L14, L15 and L16 require the guest and the
> phone to share one L2 segment so that mDNS multicast and inbound TCP 55432
> both flow. Wi-Fi cannot carry a second MAC (§3.3); NAT cannot carry
> multicast. There is no software substitute that produces admissible evidence.

**R1 does not wait for this.** Scenario A runs first and closes eight gates.

### 5.2 OPERATOR ACTION — optional, host root

> **Target machine** — the Fedora 44 host.
>
> **Action** — supply the `sudo` password, or grant a scoped NOPASSWD rule, if
> and only if Fedora 44 is to be certified as a *packaged* desktop rather than
> as a development host.
>
> **Not required** for D-1 as written. Recorded because §4.3 shows it is the
> only route to a complete Fedora 44 runtime column, and because omitting it
> would leave the RC audit unable to explain why Fedora's column differs.

---

## 6. SEC-LOG-03 — what is closed and what is not

### 6.1 The gate as written

> **SEC-LOG-03** — *file content absent from logs.*

Security Certification v1 §1 records it **PASS WITH FINDING**, and §0 records
how it got there: gap **G-1**, *"SEC-LOG-03 had no evidence anywhere.
`clipboard.v1` and `notifications.v1` each got a log-privacy canary when they
were built; `files.v1` never did."* The gap was closed with a new test,
`daemon/tests/file_log_privacy.rs`.

### 6.2 What that test proves

**In-process, at `TRACE`.** §7 states the method: *"capturing every `tracing`
event at `TRACE`, which is strictly more than `journalctl` ever shows."* The
assertion is `sec_log_03_file_content_never_reaches_the_log` — the canary and a
24-byte prefix of it absent across a 400×-repeated payload large enough to
cross the copy buffer. The capture carries a non-vacuity guard.

That is a genuine, strong test and it is **not** being reopened.

### 6.3 What it does not prove, and why this wave still lists it

Security Certification v1 §13 item 7 names the exclusion itself:

> | 7 | Live `adb logcat` sentinel run; end-to-end clipboard/file/notification exercises on hardware | §11.4 — belongs with the Phase 6 lifecycle certification, which drives the capabilities on real hardware. |

Phase 6 is the lifecycle certification, and **L15 (files smoke) is one of the
eleven gates that never ran.** The deferral therefore has no destination: the
phase it was deferred to could not execute it.

So the honest statement of the gap is narrower than "SEC-LOG-03 has no
evidence", and narrower than the brief's phrasing, and it is this:

| Layer | Evidence | State |
| --- | --- | --- |
| `tracing` events, in-process, `TRACE` | `daemon/tests/file_log_privacy.rs` | **closed** |
| **`journalctl --user -u omnibridged` after a real transfer** | — | **open** |
| **Android `logcat` after a real transfer** | — | **open** |
| A real file crossing the wire between two real devices | — | **open** — it is gate L15 |

**R2's job is the bottom three rows**, with high-entropy sentinels in the
filename and, separately, in the file *content*, and with preconditions that
prove the transfer actually happened and the capture window actually covered
it. An empty `journalctl` or `logcat` capture must fail, not pass.

### 6.4 Finding F-1 is in scope for a decision, not for a silent change

`capabilities/files/src/lib.rs:913` and `:1705` log `filename = %filename` at
`info`. That is **debt D-4**, explicitly non-blocking, and
`sec_log_03_the_filename_is_logged_and_that_is_a_recorded_finding`
characterises it so it cannot drift.

**This matters to R2's method rather than to its verdict:** a filename sentinel
*is expected to appear* in the journal, and a harness that searched for both
sentinels and failed on either would fail for the wrong reason. The two
sentinels must be asserted in opposite directions — **filename present**
(confirming the capture is real and covers the operation), **content absent**
(the gate). That is also the strongest available non-vacuity guard, because it
proves the capture window caught this specific transfer.

---

## 7. D-2 — artifact signing, current state

### 7.1 What is produced, and what is signed

**Nothing is signed.** `SHA256SUMS` carries no detached signature; no RPM is
`rpmsign`-ed; no `.deb` carries a `_gpgorigin`; no repository metadata exists
to sign.

The 14 artifacts of CI run
[`35730680143`](https://github.com/yurisismotto/omnibridge/actions/runs/35730680143),
as the release CI audit measured them:

| # | Artifact | Signed today | Package-native signature available? |
| --- | --- | --- | --- |
| 1 | `omnibridge-0.1.0.tar.gz` | no | n/a — detached signature only |
| 2 | `omnibridge-0.1.0-vendor.tar.xz` | no | n/a — detached signature only |
| 3 | `fedora44/omnibridge-0.1.0-3.fc44.src.rpm` | no | **yes** — `rpmsign`, header signature |
| 4–5 | `fedora44/omnibridge{,-gui}-0.1.0-3.fc44.x86_64.rpm` | no | **yes** — `rpmsign` |
| 6–11 | `ubuntu2404/`, `ubuntu2604/`, `debian13/` × `omnibridge{,-gui}_0.1.0-1_amd64.deb` | no | **yes** — `debsigs`/`dpkg-sig`, though third-party `.deb` practice is usually a signed checksum file |
| 12–14 | `sbom/*.cdx.json` × 3 | no | n/a — detached signature only |
| — | `SHA256SUMS` (14 entries) | **no** | n/a — **this is the primary user-facing verification path** |

**Tooling present on this host:** `gpg`. **Absent:** `rpmsign`, `debsigs`,
`dpkg-sig`, `cosign`. R3 must account for that — CI tooling is a separate
question from workstation tooling, but an emergency local signing path that
cannot run on the maintainer's own machine is not a path.

### 7.2 The three open gates, unchanged

| Gate | Status | Needs |
| --- | --- | --- |
| **RC-SIGN-01** a maintainer key exists, public half published | **OPEN** | a key, and somewhere to publish the fingerprint |
| **RC-SIGN-02** CI signs `SHA256SUMS` | **OPEN** | the private half as a secret, or an external signing service |
| **RC-SIGN-03** the README documents verification | **OPEN** | RC-SIGN-01 first |

### 7.3 What must not be confused with signing

**SLSA v1 provenance exists and verifies** — Sigstore-backed, GitHub OIDC
identity, `gh attestation verify` bound to the workflow and the commit. The
final certification states the distinction and it is restated here because R3
turns on it:

> It proves *which workflow built these bytes from which commit*. It does
> **not** prove a human vouched for the release.

Provenance answers *"was this built by OmniBridge's CI from commit X?"*.
A maintainer signature answers *"does the OmniBridge maintainer stand behind
this release?"*. A user with a corrupted download needs the first; a user
worried about a compromised repository or a hostile mirror needs the second.

### 7.4 The decision that is not ours to make

Generating a production signing key fixes **key custody** — who holds it,
where, how it rotates, what happens when it is lost. That is a trust-model
decision with consequences long after v1. The brief forbids inventing one and
this audit agrees: **R3 builds every part of the path that can exist without
the secret, and stops at `SIGNING DECISION REQUIRED`.**

---

## 8. Consolidated classification

Every open item, classified as the brief requires.

| Item | Blocking? | HUMAN-OPERATOR | ROOT-PRIVILEGE | LAN/VM | PHYSICAL-ANDROID | SIGNING | AUTOMATABLE | OTHER |
| --- | --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| **D-1 L1** clean install | **yes** | | **✔** guest | | | | | |
| **D-1 L2** manifest | **yes** | | **✔** guest | | | | | |
| **D-1 L3** autostart after login | **yes** | | **✔** guest | | | | | |
| **D-1 L6** launcher entry | **yes** | ✔ *(screenshot)* | **✔** guest | | | | | |
| **D-1 L8** activation after install | **yes** | | **✔** guest | | | | | |
| **D-1 L9** tray icon + menu | **yes** | ✔ *(screenshot)* | **✔** guest | | | | | |
| **D-1 L14** clipboard smoke | **yes** | | ✔ guest | **✔** | **✔** | | | |
| **D-1 L15** files smoke | **yes** | | ✔ guest | **✔** | **✔** | | | |
| **D-1 L16** notifications smoke | **yes** | | ✔ guest | **✔** | **✔** | | | |
| **D-1 L19** logout / login | **yes** | | **✔** guest | | | | | |
| **D-1 L20** reboot | **yes** | | **✔** guest | | | | | |
| **D-1 L10** mDNS from a packaged install | **yes** | | ✔ guest | **✔** | | | | |
| **D-1 L12** Android discovery vs packaged | **yes** | | ✔ guest | **✔** | **✔** | | | |
| **D-2** RC-SIGN-01 maintainer key | **yes** | **✔** decision | | | | **✔** | | |
| **D-2** RC-SIGN-02 CI signs `SHA256SUMS` | **yes** | | | | | **✔** | **✔** wiring | |
| **D-2** RC-SIGN-03 documented verification | **yes** | | | | | **✔** | **✔** | |
| **D-3** per-distribution runtime matrix | **yes** *(for the support claim)* | | ✔ guest | **✔** | | | | |
| **SEC-LOG-03** journal evidence | no *(gate already PASS)* | | ✔ guest | **✔** | **✔** | | **✔** harness | |
| **SEC-LOG-03** logcat evidence | no *(as above)* | | | | **✔** | | **✔** harness | |
| **Harness fail-loud rule** (R4) | no | | | | | | **✔** | |
| **AGENTS.md policy** (R4.3) | no | | | | | | **✔** | |
| **N-5** published counts vs table | no | | | | | | | **✔** record |
| **D-4** F-1 filenames logged | no | **✔** decision | | | | | | |
| **D-5** no man pages | no | | | | | | **✔** | |
| **D-6** no AppStream screenshots | no | **✔** | | | | | | |
| **D-7** no `-debuginfo` / `-dbgsym` | no | | | | | | | **✔** deliberate |
| **D-8** `cargo deny` not configured | no | **✔** licence policy | | | | | **✔** wiring | |
| **D-9** no Debian source package | no | | | | | | **✔** | |
| **D-10** x86_64 only | no | | | | | | | **✔** scope |
| **D-11** external IPv6 unscanned | no | | | **✔** | | | | **✔** network |
| **D-12** no SBOM publication path | no | | | | | | **✔** | |
| **D-13** CI debug vs `%check` release | no | | | | | | **✔** | |
| **D-14** byte-identical rebuilds unproven | no | | | | | | | **✔** aspiration |

**No new blocker is invented here.** Every blocking row traces to D-1, D-2 or
D-3 as Packaging v1 recorded them. The SEC-LOG-03 rows are marked
non-blocking because the gate is already **PASS**; they are on this wave's
list because §6.3 shows its deferred half has nowhere else to go.

---

## 9. Debts carried forward unchanged

**D-4** … **D-14** are restated in §8 and are **not reopened by this wave**.
Two are worth flagging to the RC audit because a later phase touches their
subject matter without closing them:

* **D-8** (`cargo deny` not configured) — R2.2 runs `cargo deny check` *if it
  is configured*. If it is still absent, R2 records that, and does not
  configure a licence policy on the project's behalf.
* **D-12** (no SBOM publication path) — R3 inventories the SBOMs as signing
  candidates. Signing an artifact that goes nowhere is still worth doing; it
  does not close D-12.

---

## 10. Verdict

# RELEASE READINESS V1: BASELINE ESTABLISHED

**Three debts block a public release: D-1, D-2 and D-3.** All three are exactly
where Packaging v1 left them. Nothing measured has regressed, and the trust
store and pairing this project has carried since Phase 1 are byte-identical.

**The material change is in D-1's cost, not its content.** It was recorded as
requiring a root password and a LAN-capable VM that could not be built without
one. Measured today: `qemu:///system` needs no root, three LAN-capable guests
are already built and powered off with the correct macvtap topology and a
working control channel, and the method for driving and observing them was
proven in this repository two waves ago. **Eight of the eleven blocked gates
need no operator action whatsoever.** The remaining three, and both PARTIAL
gates, need one Ethernet cable.

**D-2 is untouched and must stay untouched until a person decides.** The
infrastructure can be built without the secret; the key custody model cannot be
chosen without the maintainer.

**SEC-LOG-03 is not a false pass.** Its in-process evidence is real and stands.
Its journal-and-logcat half was deferred to a phase that then could not run,
and that is the gap R2 closes.

**What this audit does not claim.** That any lifecycle gate has been closed —
none has, and no domain was started. That the cable will work when plugged in —
U2 measured it working on 2026-09-15 and the topology has not changed, but
that is a citation, not a measurement taken today. That D-2 can be closed at
all in this wave.
