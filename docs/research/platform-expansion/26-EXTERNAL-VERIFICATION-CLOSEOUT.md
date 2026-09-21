# 26 — External verification closeout

| Field | Value |
| --- | --- |
| **Title** | External verification closeout for the platform expansion research |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Sprint** | `research/platform-expansion-verification-v1` |
| **Scope** | Every item Research v1 flagged **EXTERNAL VERIFICATION REQUIRED**, plus the claims this sprint's re-audit of the repository refuted or sharpened. |
| **Decision status** | No decisions. Evidence only. Decisions are in [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md). |
| **Evidence** | Primary sources throughout: upstream git history, distribution package databases, vendor API documentation, and this repository's source at `7bb0cc4`. |
| **Related documents** | [24](24-SOURCE-BIBLIOGRAPHY.md) §7 defines the V-list · [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) · [28](28-WAVE-0-IMPLEMENTATION-SPEC.md) |

---

## 1. How the inventory was derived

The sprint brief said not to trust the count of twelve. It was re-derived rather than assumed.

```
grep -o "EXTERNAL VERIFICATION REQUIRED" *.md | wc -l     →  21
```

Twenty-one raw occurrences, not twelve. Reading each one in context resolves the discrepancy:

| Kind | Count | What they are |
| --- | --- | --- |
| **Distinct registered items** | **12** | `V-01` … `V-12`, defined in [24 §7](24-SOURCE-BIBLIOGRAPHY.md) |
| Inline restatements of a registered item | 9 | Document-header evidence lines (00, 10, 11, 12, README, 24 ×3) and prose that points at a V-item without naming it |

Every one of the nine inline occurrences was traced to a registered item:

| Occurrence | Maps to |
| --- | --- |
| `03:187`, `11:88` | V-05 (TN3179) |
| `05:162`, `06:101` | V-01 / V-02 (data-control) |
| `08:89` | V-03 (`DnsServiceRegister`) |
| `08:244`, `09:236`, `15:280`, `20:103` | V-04 (clipboard exclusion formats) |
| `10:164` | V-08 (`org.nspasteboard.ConcealedType`) |
| `10:195` | V-06 (`NSPasteboard.AccessBehavior`) |
| `11:164` | V-07 (`UIBackgroundModes`) |
| `12:255` | V-09 (Share Extension local network) |

**So N = 12 is correct, but only because [24 §7](24-SOURCE-BIBLIOGRAPHY.md) is the canonical
register.** A reader grepping the corpus would get 21 and be misled. The register is the source
of truth; this document closes it out.

Other marker counts, re-derived the same way, for [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md):

| Marker | Raw occurrences | Distinct registered items |
| --- | --- | --- |
| `POC REQUIRED` | 31 | **35** PoC specifications in [21](21-POC-MASTER-PLAN.md) |
| `HYPOTHESIS` | 8 | — (an evidence level, not a register) |
| `PROPOSED` | 39 | — |
| `OPEN` | 33 | 12 decisions + 11 `Q-nn` questions |
| `BLOCKED` | 2 | none — both are vocabulary definitions |

---

## 2. Source policy applied in this sprint

Per the brief, a claim is only **VERIFIED** against a primary source. Two techniques mattered:

1. **Upstream git, not release notes.** Where release notes and issue threads disagreed
   (V-01, V-02), the answer was taken from the project's own commit history and source tree via
   the GitHub API. That is the publisher's own record, not a secondary account of it.
2. **Apple's documentation JSON API.** Research v1 could not verify five Apple claims because
   `developer.apple.com` renders through JavaScript. Those pages are backed by a JSON endpoint:

   ```
   https://developer.apple.com/tutorials/data/documentation/<path>.json
   ```

   It returns the same authored content the SPA displays, including full technote bodies and
   complete symbol lists. **This unblocked V-05, V-06 and V-07**, which Research v1 had to leave
   open. It is recorded here as method, so a future sprint does not repeat the dead end.

No Reddit, Stack Overflow or blog post is cited as evidence for any conclusion below.

---

## 3. Closeout table

Status vocabulary: **VERIFIED** · **REFUTED** · **PARTIALLY VERIFIED** · **STILL OPEN** ·
**BLOCKED**.

| ID | Status | One-line result |
| --- | --- | --- |
| V-01 | **VERIFIED** | `ext-data-control-v1` landed in wl-clipboard **2.3.0** (2026-03-22) |
| V-02 | **VERIFIED** | KWin dropped `wlr-data-control` in **Plasma 6.5**; ≤ 6.4 has both |
| V-03 | **PARTIALLY VERIFIED** | The API carries host/IPv4/IPv6 fields; on-the-wire A/AAAA publication is undocumented |
| V-04 | **VERIFIED** | Three formats, exact names and semantics confirmed — including one Research v1 missed |
| V-05 | **VERIFIED** | TN3179 retrieved in full; local network privacy also applies to **macOS 15+** |
| V-06 | **VERIFIED** | `NSPasteboard.AccessBehavior`, macOS **15.4**, four cases, **default is ask** |
| V-07 | **VERIFIED** | Authoritative list obtained; none of the eleven values fits OmniBridge |
| V-08 | **STILL OPEN** | Community convention; adoption breadth not establishable from a primary source |
| V-09 | **PARTIALLY VERIFIED** | Extensions share the container app's privilege; a background-undetermined edge case exists |
| V-10 | **VERIFIED** | `ring` upstream: "currently requires a C (but not C++) toolchain" |
| V-11 | **VERIFIED** | Fedora ships `2.2.1^git20251124.e808203`, which **is** feature-complete |
| V-12 | **STILL OPEN** | Cannot be settled from documentation; correctly a PoC |

**Counts: VERIFIED 8 · PARTIALLY VERIFIED 2 · STILL OPEN 2 · REFUTED 0 · BLOCKED 0.**

No V-item was refuted. Two repository claims made *by* Research v1 were — see §5.

---

## 4. Item-by-item

### V-01 — Which wl-clipboard release added `ext-data-control-v1`

| Field | Value |
| --- | --- |
| **Original claim** | "gained support at some point around release 2.3; release notes and issue #242 could not be reconciled" ([06 §4](06-KDE-PLASMA-WAYLAND.md)) |
| **Original document** | 05, 06 |
| **Why verification was needed** | It decides whether Plasma gets an event-driven clipboard on a given distribution |
| **Source consulted** | `github.com/bugaevc/wl-clipboard` — release list and release body via the GitHub API; source trees at tags `v2.2.1` and `v2.3.0`; `src/wl-copy.c` at both tags. **Primary (publisher's own repository).** Accessed 2026-08-31 |
| **Result** | **VERIFIED** |

Evidence, verbatim from the `v2.3.0` release body:

> This is an incremental release. It contains:
> * Various bug & issue fixes.
> * **Support for the `ext-data-control-v1` protocol**, kindly contributed by @zzag.
> * Some support for the `wl-copy --sensitive` flag, and `CLIPBOARD_STATE=sensitive` hint in
>   `wl-paste --watch` mode. There are both implemented through `x-kde-passwordManagerHint`,
>   and should be compatible with (at least) Klipper and KeePassXC.

Release dates from the same API: **v2.3.0 → 2026-03-22**; **v2.2.1 → 2023-08-27**.

**Impact.** Larger than the question asked, because the same release note settles a second
matter nobody had asked about — see [§5.1](#51-refuted--wl-copy---sensitive-is-not-available-on-any-current-debianubuntu-stable).

### V-02 — Does KWin still expose `wlr-data-control-unstable-v1`?

| Field | Value |
| --- | --- |
| **Original claim** | Unknown; "the crux" of the KDE risk ([06 §4](06-KDE-PLASMA-WAYLAND.md), Q-02) |
| **Original document** | 05, 06 |
| **Why verification was needed** | If KWin kept a compatibility binding, the whole Ubuntu-LTS KDE risk evaporates |
| **Source consulted** | `github.com/KDE/kwin` — full `master` tree listing, `src/wayland/datacontroldevicemanager_v1.{h,cpp}`, commit history for that path, and tag-ancestry comparisons. **Primary (upstream source).** Accessed 2026-08-31 |
| **Result** | **VERIFIED — the compatibility binding is gone** |

Evidence:

1. `DataControlDeviceManagerV1InterfacePrivate` derives from
   `QtWaylandServer::ext_data_control_manager_v1` **and nothing else**. The header comment reads:
   *"DataControlDeviceManagerV1Interface corresponds to the Wayland interface
   `ext_data_control_manager_v1`."*
2. The string `zwlr_data_control` appears **zero times** in the `master` tree.
3. Commit history for that file gives the exact sequence:

   | Date | Commit subject |
   | --- | --- |
   | 2024-11-22 | Support wlr-data-control with the same impl as ext-data-control |
   | 2025-04-10 | wayland/datacontrol: Port to ext-data-control |
   | **2025-04-12** | **wayland/datacontrol: Drop wlr-data-control support overlay** |

4. Tag ancestry of the drop commit `764b723`: **not** in `v6.3.6`, `v6.4.0`, `v6.4.6`;
   **present** in `v6.4.90`, `v6.4.91`, `v6.5.0`.

**Conclusion: KWin ≤ 6.4.x offers both protocols. KWin ≥ 6.5.0 offers only
`ext-data-control-v1`.**

**Impact.** Combined with V-01 and the archive versions below, the failure Research v1
hypothesised is **real, and its boundaries are now exact** — see [§6](#6-the-kde-clipboard-matrix-resolved).

### V-03 — Does `DnsServiceRegister` publish A/AAAA records?

| Field | Value |
| --- | --- |
| **Original claim** | A Microsoft Q&A thread suggests it may not ([08 §4](08-WINDOWS-FEASIBILITY.md), Q-04) |
| **Source consulted** | learn.microsoft.com — `DnsServiceRegister`, `DNS_SERVICE_INSTANCE`. **Primary (vendor).** Accessed 2026-08-31 |
| **Result** | **PARTIALLY VERIFIED** |

`DNS_SERVICE_INSTANCE` carries `pszHostName`, `ip4Address` (`IP4_ADDRESS*`) and `ip6Address`
(`IP6_ADDRESS*`), documented as "the service-associated IPv4/IPv6 address". So the API
*accepts* addresses and the capability plainly exists. **What the documentation never states is
whether the responder emits A/AAAA records on the wire, or what happens when the pointers are
`NULL`.** That cannot be closed from documentation.

Two facts were verified and are useful regardless:

- Minimum supported client: **Windows 10, desktop apps only**.
- *"The registration is tied to the lifetime of the calling process. If the process goes away,
  the service will be automatically deregistered."* — the same semantics OmniBridge's
  `Advertisement`/`Drop` pair implements today.

**Impact: reduced priority.** This only matters if `mdns-sd` is *replaced* on Windows. Since
`mdns-sd` publishes its own address records, V-03 is a question about a fallback path, not the
primary one. Stays with **POC-WIN-02**, demoted from a blocking question to a contingent one.

### V-04 — Windows clipboard-history / Cloud Clipboard exclusion formats

| Field | Value |
| --- | --- |
| **Original claim** | `"ExcludeClipboardContentFromMonitorProcessing"` and/or `"CanIncludeInClipboardHistory"`, exact names unconfirmed |
| **Original document** | 08, 09, 15, 20 (WIN-008) |
| **Source consulted** | learn.microsoft.com — *Clipboard Formats*, §"Cloud Clipboard and Clipboard History Formats". **Primary (vendor).** Accessed 2026-08-31 |
| **Result** | **VERIFIED — and there are three formats, not two** |

Verbatim:

> - **ExcludeClipboardContentFromMonitorProcessing** : Place any data on the clipboard in this
>   format to prevent all clipboard formats being included in the clipboard history or
>   synchronized to the user's other devices.
> - **CanIncludeInClipboardHistory** : Place a serialized **DWORD** value of zero on the
>   clipboard in this format to prevent all clipboard formats being included in the clipboard
>   history, or place a value of one instead to explicitly request that the clipboard item be
>   included in the clipboard history. **This does not affect synchronization to the user's
>   other devices.**
> - **CanUploadToCloudClipboard** : Place a serialized **DWORD** value of zero on the clipboard
>   in this format to prevent all clipboard formats being synchronized to the user's other
>   devices, or place a value of one instead to explicitly request that the clipboard item be
>   synchronized to other devices. **This does not affect the local device's clipboard history.**

All three are obtained through `RegisterClipboardFormat`.

**Impact on WIN-008.** Research v1 named two of three and did not have the semantics. The
correct implementation for a `sensitive_hint` clip sets **all three**:
`ExcludeClipboardContentFromMonitorProcessing` (blanket), plus `CanIncludeInClipboardHistory`=0
and `CanUploadToCloudClipboard`=0 explicitly — because the two DWORD formats are documented as
covering *disjoint* halves, and relying on the blanket format alone leaves no defence if its
behaviour narrows. WIN-008 moves from "verify the names first" to **specified and implementable**.

### V-05 — TN3179 *Understanding local network privacy*

| Field | Value |
| --- | --- |
| **Original claim** | Content unknown; "the authoritative reference and could not be retrieved" ([11 §4](11-IOS-IPADOS-FEASIBILITY.md), [03](03-PLATFORM-CAPABILITY-MATRIX.md), Q-06) |
| **Source consulted** | `developer.apple.com/tutorials/data/documentation/technotes/tn3179-understanding-local-network-privacy.json`. **Primary (vendor).** Accessed 2026-08-31 |
| **Result** | **VERIFIED — retrieved in full** |

The findings exceed the question. Four are decision-relevant.

**(a) It applies to macOS, from macOS 15.** Verbatim platform table: iOS 14, iPadOS 14,
**macOS 15**, visionOS 1; tvOS and watchOS not supported. Research v1 treated local-network
privacy as an iOS concern. **It is a macOS concern too**, and Wave 7 must budget for it.

**(b) Listening does not require the privilege; connecting and Bonjour do.** Verbatim:

| Operation | Local network access required |
| --- | --- |
| Making an outgoing TCP connection | **yes** |
| **Listening for and accepting incoming TCP connections** | **no** |
| Sending a UDP unicast / multicast / broadcast | yes |
| Receiving an incoming UDP unicast | no |
| Receiving an incoming UDP multicast | yes |

and *"All Bonjour operations require local network access"* — registering, browsing, resolving.

**(c) The macOS agent does not get the daemon exemption.** Verbatim:

> macOS automatically allows local network access by: Any daemon started by `launchd`; Any
> program running as root; Command-line tools run from Terminal or over SSH…
> **The exception for `launchd` daemons doesn't apply to `launchd` agents.**

OmniBridge's macOS process must be a **per-user agent** (it needs the user's pasteboard and login
session — the same reason Windows needs an agent, [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md)
PLAT-DEC-002). So it **will** face the Local Network prompt. Two consequences:

- Use `SMAppService`, or set `AssociatedBundleIdentifiers` in the `launchd` plist, so macOS
  attributes the access to the app and shows a meaningful name in the alert.
- **Sign with a real Developer ID.** Verbatim: *"Local network privacy tracks the identity of
  your program using its code signature… To ensure that local network privacy reliably tracks
  the identity of your macOS program, sign it with an Apple-issued code-signing identity."*
  This makes signing a **technical** requirement, not only a distribution one — it changes the
  answer to a question the brief asked explicitly ([§8](#8-macos-signing-required-for-execution-or-for-distribution)).

**(d) A documented Apple bug constrains our error handling.** Verbatim:

> macOS fails to display the local network alert when a process with a very short lifespan
> performs a local network operation (FB16131937). For example, if you create a `launchd` agent
> that performs a local network operation and immediately exits when that fails, macOS won't
> display the local network alert. **To work around this, update your code to not exit
> immediately after a local network operation fails.**

OmniBridge's daemon currently treats a bind failure as fatal. On macOS that behaviour would produce
an agent that can never obtain the permission it needs — a permanent, silent failure. This is a
**named requirement on the macOS adapter**, new in this sprint (MAC-010, [25](25-IMPLEMENTATION-BACKLOG.md)).

Also recorded: the iOS Simulator does not support local network privacy (test on device);
macOS has **no way to reset** the privilege (FB14944392), so PoCs need VM snapshots or fresh
user accounts; macOS keeps the state **per user account**; `AllowedEthernetLocalNetworkAddresses`
/ `AllowedWiFiLocalNetworkAddresses` (macOS 15.5+) exist for CI hosts.

### V-06 — `NSPasteboard.AccessBehavior`

| Field | Value |
| --- | --- |
| **Original claim** | "introduced version, enum cases, Info.plist key, whether reads prompt" all unknown ([10 §6.2](10-MACOS-FEASIBILITY.md), Q-06) |
| **Source consulted** | `…/appkit/nspasteboard/accessbehavior-swift.enum.json` and `…/appkit/nspasteboard.json`. **Primary (vendor).** Accessed 2026-08-31 |
| **Result** | **VERIFIED** |

- **Introduced: macOS 15.4.**
- **Four cases**, verbatim abstracts:
  - `.default` — *"The default behavior for the General pasteboard is to ask upon programmatic
    access. All other pasteboards default to always allow access. If an app has never triggered
    a pasteboard access alert, its General pasteboard will report [default] behavior… Once
    programmatic pasteboard access triggers the first pasteboard access alert, the state
    automatically changes to [ask]."*
  - `.ask` — *"The system will notify the user and ask for permission before granting pasteboard
    access. However, access that is both **user originated and paste related** will always be
    allowed, and will not result in a notification."*
  - `.alwaysAllow` — *"automatically allow all pasteboard access, without notifying the user."*
  - `.alwaysDeny` — *"automatically deny all pasteboard access, without notifying the user."*
- **No Info.plist key exists.** `accessBehavior` is a read-only property; the value is set by
  the user in System Settings, per app, and only after the app has triggered an alert. An app
  cannot declare or request it.

**Impact — this is the most consequential macOS finding of the sprint.** OmniBridge's automatic
clipboard *send* is by definition programmatic, non-user-originated access to the General
pasteboard. On macOS 15.4+ the default outcome is an **alert**, and silent operation requires
the user to have chosen `.alwaysAllow` in System Settings afterwards.

Note what is *not* affected: reading `changeCount` is metadata, not content, and the
documentation ties the alert to pasteboard *access*. Whether a `changeCount` read alone trips
the alert is the one part still unmeasured — and it is now the sharpest possible question for
**POC-MAC-05**, which changes from "does anything prompt?" to "does `changeCount` polling alone
prompt, or only the subsequent content read?"

macOS clipboard auto-send is therefore **user-gated on 15.4+**, not merely "polling-based". That
is a product statement, and [03](03-PLATFORM-CAPABILITY-MATRIX.md) is updated accordingly.

### V-07 — Authoritative `UIBackgroundModes` list

| Field | Value |
| --- | --- |
| **Original claim** | "The exact current list of values could not be fetched… the conclusion does not depend on the edges" ([11 §6.2](11-IOS-IPADOS-FEASIBILITY.md), Q-07) |
| **Source consulted** | `…/bundleresources/information-property-list/uibackgroundmodes.json`. **Primary (vendor).** Accessed 2026-08-31 |
| **Result** | **VERIFIED — and the conclusion holds** |

The eleven values: `audio`, `location`, `voip`, `fetch`, `remote-notification`,
`external-accessory`, `bluetooth-central`, `bluetooth-peripheral`, `processing`,
`push-to-talk`, `nearby-interaction`.

**None describes "hold a TCP/TLS session open to a LAN peer."** The near misses and why they are
misses: `voip` is gated on PushKit/CallKit with an incoming-call UI; `processing` is a
system-scheduled opportunistic `BGProcessingTask`, not a socket; `fetch` is a brief
opportunistic wake; `remote-notification` requires APNs, which
[27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) PLAT-DEC-005 rejects on principle.

Research v1's hedge is discharged: the conclusion did not depend on the edges, **and the edges
now confirm it**.

### V-08 — Is `org.nspasteboard.ConcealedType` honoured widely enough?

| Field | Value |
| --- | --- |
| **Result** | **STILL OPEN** |

`org.nspasteboard.ConcealedType` is a community convention published at nspasteboard.org, not an
Apple API — Research v1 said so and that remains correct. The open half of the question is
*adoption breadth*, and that is not the sort of claim any primary source establishes: there is
no vendor, no specification body and no registry. Surveying individual clipboard-manager
repositories would be evidence about those repositories, not about the convention.

**Recommendation: retire the question rather than keep it open indefinitely.** The cost of
setting the type is one line; the benefit is non-zero and unmeasurable; it is a *hint*, exactly
like `wl-copy --sensitive`, and `clipboard.v1` already defines `sensitive_hint` as advisory. Set
it, document it as best-effort, and do not gate anything on it. Reclassified in
[27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) as a non-decision.

### V-09 — Does an iOS Share Extension inherit local-network permission?

| Field | Value |
| --- | --- |
| **Source consulted** | TN3179, §"App extension considerations". **Primary (vendor).** |
| **Result** | **PARTIALLY VERIFIED** |

Verbatim:

> **In general, app extensions share the Local Network privilege state of their container app.**
> Some app extension types are assumed to be running in the background. If such an extension
> performs a local network operation while its Local Network privilege is undetermined, the
> system denies that operation as it would for an iOS app running in the background…
> If your app has app extensions, add the [`NSLocalNetworkUsageDescription`] and
> [`NSBonjourServices`] properties to the **app's** `Info.plist`, not to the app extension's.

The general answer is **yes, it inherits** — which is what Research v1 hoped. The residual is the
*undetermined* case: if the user has never granted the privilege and first use is via the Share
Extension, the operation is denied with no alert. That is a real product path (install, share
straight from Photos, never open the app) and it must be designed for: the app has to establish
the privilege in the foreground during onboarding.

Not fully closed because "some app extension types are assumed to be running in the background"
does not enumerate which, and whether a Share Extension is among them is unstated.
**POC-IOS-08** narrows accordingly.

### V-10 — Is `ring` the reason the RPM spec needs `gcc`?

| Field | Value |
| --- | --- |
| **Source consulted** | `github.com/briansmith/ring` — `BUILDING.md`. **Primary (upstream).** Accessed 2026-08-31 |
| **Result** | **VERIFIED** (for the "is `ring` a reason" half) |

Verbatim: **"*ring* currently requires a C (but not C++) toolchain."**

So `gcc` in the RPM spec is not vestigial. Whether it is the *only* reason still needs the
container build (Q-01 / **POC-LINUX-01**), but the question changes from "is a C toolchain
needed at all" to "is anything *else* also needed", which is a much smaller question.

**Two consequences Research v1 did not draw**, both from the same page:

- **Windows builds need MSVC.** Verbatim: *"For Windows targets, 'Build Tools for Visual Studio
  2022'… The 'Desktop development with C++' workflow must be installed."* And for ARM64:
  *"the Visual Studio Build Tools 'VS 2022 C++ ARM64 build tools' and 'clang' components must be
  installed."*
- **Therefore the Wave 0 cross-compile gate cannot be a bare `cargo check --target
  x86_64-pc-windows-msvc` on a Linux host** — `ring` will not build without the MSVC toolchain
  and sysroot. [28](28-WAVE-0-IMPLEMENTATION-SPEC.md) specifies the gate accordingly, and it is
  the reason Wave 0's acceptance criterion is stated in terms of a
  *Windows runner* rather than a Linux cross-compile.

### V-11 — Fedora's `wl-clipboard` version

| Field | Value |
| --- | --- |
| **Source consulted** | The certification machine itself: `rpm -q wl-clipboard`, `wl-copy --version`, `wl-copy --help`, and `strings` on the installed binaries. **Primary (the artefact under test).** 2026-08-31 |
| **Result** | **VERIFIED — with a trap** |

```
wl-clipboard-2.2.1^git20251124.e808203-2.fc44.x86_64
```

The version *string* says 2.2.1. The *binary* does not behave like 2.2.1:

- `wl-copy --help` lists `--sensitive`; `echo test | wl-copy --sensitive` exits 0.
- `strings` on both `wl-copy` and `wl-paste` shows **`ext_data_control_*` and
  `zwlr_data_control_*`** interface names.

Fedora packages a **post-2.2.1 git snapshot** (2025-11-24) that already carries the features
released as 2.3.0.

**Impact — this refutes a packaging instinct.** [06 §4](06-KDE-PLASMA-WAYLAND.md) mitigation (a)
was *"depend on `wl-clipboard >= 2.3` in the packaging"*. On Fedora that dependency
**would exclude a package that has the features**, because `2.2.1^git20251124 < 2.3`. A
version-number dependency is the wrong instrument here. The right one is a **capability probe**,
which the code already performs — `probe_data_control()` runs `wl-paste --watch` and observes
whether it survives. The packaging should express a *recommendation*, and the runtime should keep
deciding by probe. Recorded as a correction to PKG-00x and KDE-001 in
[25](25-IMPLEMENTATION-BACKLOG.md).

### V-12 — Can `mdns-sd` bind 5353 alongside a system responder?

| Field | Value |
| --- | --- |
| **Result** | **STILL OPEN — correctly** |

Nothing in `mdns-sd`'s documentation, Apple's, or Microsoft's states what happens when a second
responder binds 5353 on a host that already runs `mDNSResponder` (macOS) or the Windows
responder. Socket-option behaviour (`SO_REUSEADDR`/`SO_REUSEPORT` semantics differing per OS)
makes this exactly the class of question documentation cannot answer.

Remains **POC-WIN-02 / POC-MAC-02**. This is a correct use of a PoC and no attempt was made to
manufacture a documentary answer.

---

## 5. Claims refuted by this sprint's repository re-audit

The brief said the previous report is not the only source, and that the current code is. Two
Research v1 claims did not survive contact with the code.

### 5.1 REFUTED — `wl-copy --sensitive` is not available on any current Debian/Ubuntu stable

[06 §3](06-KDE-PLASMA-WAYLAND.md) states, for "write sensitive":

> `wl-copy --sensitive` — ✅ identical, and *more effective* — Klipper is a real clipboard-history
> manager that honours the hint

**This is wrong on every Debian and Ubuntu release currently supported.** Chain of primary
evidence:

1. `--sensitive` was added in wl-clipboard **2.3.0** (V-01, release body).
2. The `v2.2.1` source tree confirms the absence directly: the string `sensitive` appears
   **0 times** in `src/wl-copy.c`, and its `long_options` table is
   `{version, help, primary, trim-newline, paste-once, foreground, clear, type, seat}`.
3. Its unknown-option path is not lenient:
   ```c
   default:
       /* getopt has already printed an error message */
       print_usage(stderr, argv[0]);
       exit(1);
   ```
4. OmniBridge passes the flag unconditionally when the clip is sensitive
   (`backend/wayland.rs:263-267`), and treats a non-zero exit as a hard failure
   (`backend/wayland.rs:310-315`): `Err(BackendError::Failed("wl-copy exited with …"))`.
5. Archive versions (packages.debian.org / packages.ubuntu.com, 2026-08-31):
   Debian trixie **2.2.1-2**; Ubuntu noble **2.2.1-1build1**, questing **2.2.1-2**,
   resolute (26.04 LTS) **2.2.1-2build1**. Only Debian forky/sid and Ubuntu stonking have 2.3.0.

**Therefore: on Debian 13 and on every current Ubuntu LTS, receiving a clip with
`sensitive_hint` set fails outright.** Not degrades — fails.

Three things make this more serious than the KDE data-control issue:

- It is **desktop-independent**. GNOME, KDE, sway — all of them. It is a `wl-copy` argument, not
  a compositor protocol.
- It affects the **most security-sensitive** clipboard path there is: the one carrying passwords.
- It is **invisible on the development machine**, because Fedora's snapshot has the flag (V-11).
  This is precisely the Fedora-monoculture blind spot Wave 1 exists to remove.

The failure direction is at least the safe one: fail-closed. OmniBridge does not paste the password
without the hint; it refuses. That is the right choice and should be preserved. But the user sees
an unexplained failure for exactly one class of clip.

**This is a new defect, not a gap.** Filed as **LINUX-010 (P0 for Wave 2)** in
[25](25-IMPLEMENTATION-BACKLOG.md), with the fix shape: probe `--sensitive` support once (as
`probe_data_control()` already does for the watch path), and on an older `wl-copy` either fall
back to a plain write **with the user told the hint could not be honoured**, or refuse with a
message that names the cause and the remedy. Which of the two is a product decision, raised as
**PLAT-DEC-013** in [27](27-ARCHITECTURE-DECISION-CLOSEOUT.md).

### 5.2 REFUTED — `filename.rs` already implements most of the Windows rules

[01 §3.4](01-CURRENT-ARCHITECTURE-AUDIT.md) (BLOCKER-04) and AUD-07 state:

> It does not know about Windows-reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
> `LPT1`–`LPT9`), about trailing dots and spaces being stripped by the Win32 layer, or about `\`
> being a separator.

Reading `capabilities/files/src/filename.rs` at `7bb0cc4`, **all three are already handled**:

| Claimed missing | Actually present |
| --- | --- |
| Reserved device names | `RESERVED_STEMS` — `con prn aux nul com1‥com9 lpt1‥lpt9`, matched case-insensitively on the pre-first-dot stem |
| Trailing dots and spaces | `.trim_end_matches([' ', '.', '\t'])` |
| `\` as a separator | `raw.rsplit(['/', '\\'])` |

with tests for each (`windows_device_names_are_rejected`, `trailing_dots_and_spaces_go`,
`a_windows_traversal_keeps_only_the_basename`). The module's own doc comment explains the intent:
*"Nothing here runs on Windows today, but a received file lands in a directory that is routinely
shared over SMB or synced, and this costs one table lookup."*

**R-07 and SEC-004 were therefore overstated.** The residual gaps are real but different, and one
of them is not a Windows issue at all — see [§7](#7-filename-safety-the-actual-residual-gaps).

---

## 6. The KDE clipboard matrix, resolved

V-01 + V-02 + the archives give the complete answer that POC-KDE-01 was going to spend an
afternoon discovering. The PoC is not cancelled — it still measures the Xwayland fallback and the
GUI — but its central question is now answered from primary sources.

| Distribution | KWin | wl-clipboard | `wlr` offered | `ext` offered | wl-clipboard speaks | **Auto-send** | **`--sensitive`** |
| --- | --- | --- | :-: | :-: | --- | :-: | :-: |
| Debian 13 trixie | 6.3.6 | 2.2.1 | ✅ | ✅ | wlr only | ✅ via `wlr` | ❌ |
| Debian forky / sid | 6.7.4 | 2.3.0 | ❌ | ✅ | both | ✅ via `ext` | ✅ |
| Ubuntu 24.04 LTS | 5.27.11 | 2.2.1 | ✅ | ❌ | wlr only | ✅ via `wlr` | ❌ |
| Ubuntu 25.10 | 6.4.5 | 2.2.1 | ✅ | ✅ | wlr only | ✅ via `wlr` | ❌ |
| **Ubuntu 26.04 LTS** | **6.6.4** | **2.2.1** | **❌** | ✅ | **wlr only** | ❌ **BROKEN** | ❌ |
| Ubuntu stonking | 6.7.4 | 2.3.0 | ❌ | ✅ | both | ✅ via `ext` | ✅ |
| Fedora 44 | — (GNOME here) | 2.2.1^git | — | — | both | n/a (Mutter) | ✅ |

Sources: KWin tag ancestry (V-02); wl-clipboard tags and source (V-01); packages.debian.org and
packages.ubuntu.com for `kwin-wayland` and `wl-clipboard`, accessed 2026-08-31; the local machine
for Fedora (V-11).

**Findings.**

1. **Exactly one configuration is broken, and it is the newest LTS.** Ubuntu 26.04 LTS pairs
   KWin 6.6 (which dropped `wlr`) with wl-clipboard 2.2.1 (which only speaks `wlr`). No common
   protocol → `probe_data_control()` fails → fallback to the Xwayland XFIXES bridge, whose
   behaviour on Plasma is still unmeasured.
2. **The window is closing on its own.** Ubuntu will pick up wl-clipboard 2.3.0 in a later cycle;
   `stonking` already has it. This is a transient distribution-packaging mismatch, not an
   architectural problem — which is an argument for mitigation (a) (document, probe, degrade
   honestly) over mitigation (b) (implement `ext-data-control` in-process).
3. **The right lever is an Ubuntu bug report**, not OmniBridge code. wl-clipboard 2.3.0 in
   resolute-updates fixes both this *and* `--sensitive` for every Ubuntu user, not just OmniBridge's.
4. **Debian is fine throughout**, in both stable and testing, for auto-send.
5. `--sensitive` is broken far more widely than auto-send — see §5.1.

---

## 7. Filename safety: the actual residual gaps

Re-derived from the code, and confirmed by simulating `sanitize()` against adversarial inputs.
The simulation reproduces the function's logic exactly (split on `/` and `\`, drop `Cc`
characters, `trim_start`, `trim_end_matches([' ', '.', '\t'])`, reject `.`/`..`, reject reserved
stems, truncate).

| Input | `sanitize()` returns | Verdict |
| --- | --- | --- |
| `CON`, `con.txt`, `NUL.jpg`, `lpt9.tar.gz`, `..\..\x\CON` | `None` | ✅ handled |
| `evil.txt.`, `evil.txt   `, `evil.txt . . ` | `evil.txt` | ✅ handled |
| `../../etc/passwd`, `..\..\windows\system32\cmd.exe` | basename only | ✅ handled |
| **`report:hidden.txt`** | **`report:hidden.txt`** | ❌ **alternate data stream** |
| **`a:b`** | **`a:b`** | ❌ **alternate data stream** |
| **`photo\u{202E}gnp.exe`** | **unchanged** | ❌ **bidi spoofing — all platforms** |
| **`file\u{200B}.txt`** | **unchanged** | ❌ invisible character |
| `CONIN$`, `CONOUT$` | unchanged | ❌ reserved on Windows, absent from the table |
| `x\|y.txt`, `q?.txt`, `<a>.txt` | unchanged | ⚠️ invalid on Windows → create fails |
| `con .txt` | `con .txt` | ⚠️ benign (Win32 does not trim mid-name) |

Two of these deserve to be separated from the rest.

**The colon is a real Windows security gap.** `a:b` on NTFS creates an alternate data stream `b`
on file `a`. The file the user sees is `a`, apparently empty; the payload is invisible to
Explorer and to most tooling. `destination.rs::reserve()`'s defence-in-depth check
(`path.parent() != Some(self.dir)`) does **not** catch it, because `C:\dir\a:b` has parent
`C:\dir`. This confirms the *severity* Research v1 assigned to SEC-004 while correcting its
*content*.

**The bidi override is a cross-platform gap that exists on Linux today.** `char::is_control()`
matches Unicode general category **Cc** only. `U+202E RIGHT-TO-LEFT OVERRIDE` is category **Cf**,
and is therefore not filtered (confirmed: `unicodedata.category('\u202E') == 'Cf'`). A file named
`invoice\u{202E}cod.exe` displays in most file managers as `invoiceexe.doc`. This is a
**present-tense defect on the certification platform**, not a future Windows concern — and it was
not in Research v1 at all.

Both are folded into a rewritten **SEC-004** in [25](25-IMPLEMENTATION-BACKLOG.md), and the
protocol-global-versus-destination-specific question is decided in
[27](27-ARCHITECTURE-DECISION-CLOSEOUT.md) as **PLAT-DEC-014**.

Also confirmed, and worth recording because it removes a worry: **case-insensitive collisions are
already safe.** `reserve()` opens with `create_new(true)` (`O_EXCL`), so on NTFS or APFS a
colliding `Report.txt` returns `AlreadyExists` and the loop numbers up to `Report (1).txt`. No
overwrite is possible. No work needed.

---

## 8. macOS signing: required for execution, or for distribution?

The brief asked for this distinction explicitly. Evidence now separates them cleanly.

| Requirement | Needed to **run** locally | Needed for **trusted distribution** | Source |
| --- | :-: | :-: | --- |
| Ad-hoc / unsigned binary | ✅ runs (with Gatekeeper override) | ❌ | — |
| **Developer ID signature** | **✅ required in practice** | ✅ required | TN3179 §Build-time considerations |
| Notarization | ❌ | ✅ required | Notarizing macOS software |
| Hardened runtime | ❌ | ✅ required for notarization | Notarizing macOS software |

The middle row is the change. Research v1 filed Developer ID under distribution. TN3179 makes it
a **runtime correctness** requirement for OmniBridge specifically:

> Local network privacy tracks the identity of your program using its code signature. This
> presents a challenge on macOS, which allows for unsigned code and ad hoc signed code… To ensure
> that local network privacy reliably tracks the identity of your macOS program, **sign it with
> an Apple-issued code-signing identity**. … Local network privacy uses your main executable UUID
> as part of its implementation. If your main executable has no UUID, or shares a UUID with other
> programs, local network privacy may behave weirdly.

An unsigned OmniBridge agent may have its Local Network grant attributed unstably across rebuilds —
which for a product whose entire function is local networking means the permission silently
resets. **R-09 (Developer ID paperwork) therefore rises in severity**: it gates the macOS PoCs,
not just the macOS release.

---

## 9. Windows: items verified beyond the V-list

These were not on the V-list but the brief named them. All are primary-source verified.

### 9.1 Session 0 — why the clipboard cannot live in a service

learn.microsoft.com, *Interactive Services*, verbatim:

> **Services cannot directly interact with a user as of Windows Vista.**
> By default, services use a **noninteractive window station** and cannot interact with the user.
> **All services run in Terminal Services session 0.**

`NoInteractiveServices` defaults to **1** on modern Windows, so `SERVICE_INTERACTIVE_PROCESS` is
inert. The Windows clipboard is a per-window-station object; a service's noninteractive station is
not the logged-in user's `WinSta0`. **A service cannot see the user's clipboard.** PLAT-DEC-002
is settled by first-party documentation.

Microsoft's own recommended shape is the one OmniBridge already has on Linux:

> Create a separate hidden GUI application and use `CreateProcessAsUser`… Design the GUI
> application to communicate with the service through some method of interprocess communication
> (IPC), for example, **named pipes**. … **Note that IPC can expose your service interfaces over
> the network unless you use an appropriate access control list (ACL).** … If this service runs
> on a multiuser system, add the application to `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`
> so that it is run in each session. If the application uses named pipes for IPC, the server can
> distinguish between multiple user processes by **giving each pipe a unique name based on the
> session ID**.

The last sentence is first-party confirmation of the per-session pipe naming that
[09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) proposed by reasoning.

### 9.2 Named pipes — the default ACL is unsafe

learn.microsoft.com, `CreateNamedPipeA`, verbatim on `lpSecurityAttributes`:

> If *lpSecurityAttributes* is **NULL**, the named pipe gets a default security descriptor and the
> handle cannot be inherited. The ACLs in the default security descriptor for a named pipe grant
> full control to the LocalSystem account, administrators, and the creator owner. **They also
> grant read access to members of the Everyone group and the anonymous account.**

**OmniBridge must never pass `NULL`.** The default exposes the control plane to Everyone and to
anonymous. This is a concrete, mandatory requirement, and it is a stronger statement than
[09](09-WINDOWS-SECURITY-AND-INTEGRATION.md) makes.

On squatting, `FILE_FLAG_FIRST_PIPE_INSTANCE`, verbatim:

> If you attempt to create multiple instances of a pipe with this flag, creation of the first
> instance succeeds, but creation of the next instance fails with **ERROR_ACCESS_DENIED**.

Read carefully, this is **detection, not prevention**. It cannot stop a hostile process that
created the name *first*; what it guarantees is that in that case **our** create fails. The
mitigation is therefore a *policy on the failure*, and it must be stated as one:

> On `ERROR_ACCESS_DENIED` from `CreateNamedPipe` with `FILE_FLAG_FIRST_PIPE_INSTANCE`, the agent
> must **abort with an error naming the squatter**. It must never retry, and must never fall back
> to a different pipe name — a fallback name is exactly what an attacker wants, because the CLI
> and GUI would then have to search for it.

Also verified and required: **`PIPE_REJECT_REMOTE_CLIENTS`** (0x00000008) — *"Connections from
remote clients are automatically rejected"* — without which a named pipe is reachable over SMB.
And: pipe names are **case-insensitive**, max 256 characters, which matters for the per-SID naming
scheme.

### 9.3 Clipboard watching is genuinely event-driven

learn.microsoft.com, *Using the Clipboard*, verbatim:

> There are three ways of monitoring changes to the clipboard… **New programs should use clipboard
> format listeners or the clipboard sequence number.**
> A clipboard format listener is a **window** which has registered to be notified… A window
> registers by calling **AddClipboardFormatListener**. When the contents of the clipboard change,
> the window is posted a **WM_CLIPBOARDUPDATE** message.

and, on the sequence number: *"Note that this is a **not** a notification method and **should not
be used in a polling loop**."*

**Two consequences.** First, Windows genuinely satisfies the existing no-polling contract, so
PLAT-DEC-009's exception is macOS-only. Second — and this is a design constraint Research v1 did
not record — `AddClipboardFormatListener` **requires an `HWND` and a message pump**. The Windows
agent therefore cannot be a pure console/tokio process; it must own a message-only window and run
a message loop alongside the async runtime. That is a real structural requirement on the Windows
adapter, filed as **WIN-010**.

---

## 10. rustls and `rustls-cng`: the identity seam is documentation-verified

The brief asked whether signing can be provided through a trait without PKCS#8 bytes. It can, and
the exact shape is now pinned to the version this repository uses.

**Verified against docs.rs for rustls 0.23.43 — the version in `desktop/Cargo.lock`:**

```rust
pub trait SigningKey: Debug + Send + Sync {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>>;
    fn algorithm(&self) -> SignatureAlgorithm;
    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> { ... }   // provided
}

pub trait Signer: Debug + Send + Sync {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
    fn scheme(&self) -> SignatureScheme;
}

pub struct CertifiedKey {
    pub cert: Vec<CertificateDer<'static>>,
    pub key:  Arc<dyn SigningKey>,
    pub ocsp: Option<Vec<u8>>,
}
```

Three details that decide the adapter implementations, all from the official docs:

1. **`sign()` is synchronous and non-async.** Confirms the constraint Research v1 derived: an
   Apple Enclave key must be created **without** `.userPresence`, because a Touch ID prompt cannot
   occur inside a handshake. PLAT-DEC-004's sub-decision stands.
2. **The message is *not* pre-hashed.** Verbatim: *"`message` is not pre-hashed by the caller; the
   implementer must apply the appropriate hash function based on the selected scheme."* For
   `ECDSA_NISTP256_SHA256` the implementer must SHA-256 then sign. This maps cleanly onto:
   - Apple `SecKeyCreateSignature(key, .ecdsaSignatureMessageX962SHA256, data)` — the *Message*
     variants hash internally ✅
   - Android `Signature.getInstance("SHA256withECDSA")` — hashes internally ✅ (and is exactly
     why the v1 Keystore keys created with `DIGEST_NONE` were unusable)
   - Windows CNG `NCryptSignHash` — takes a **pre-computed hash**, so the adapter must hash first
3. **ECDSA output must be X9.62 DER `SEQUENCE { INTEGER r, INTEGER s }`**, not raw `r‖s`. Apple and
   Android return DER; **Windows CNG returns IEEE-P1363 raw** and must be converted.

**`CertifiedKey::new(cert_chain, Arc<dyn SigningKey>)` requires no PKCS#8 bytes**, and the builder
seam exists at exactly the states `tls.rs` already uses:

```rust
// ServerConfig, WantsServerCert
pub fn with_cert_resolver(self, cert_resolver: Arc<dyn ResolvesServerCert>) -> ServerConfig
// ClientConfig, WantsClientCert
pub fn with_client_cert_resolver(self, r: Arc<dyn ResolvesClientCert>) -> ClientConfig
```

Note both return the config **directly, not a `Result`** — so the `.map_err(Error::Tls)?` at
`tls.rs:292` and `tls.rs:346` disappears. A small thing, but it is the kind of detail that turns a
"should be easy" into a compile error at the wrong moment.

### `rustls-cng` — classification

| Question | Answer | Evidence |
| --- | --- | --- |
| Under the rustls org? | **Yes** — `rustls/rustls-cng` | GitHub API |
| Maintained? | **Yes** — last push **2026-08-10**, three weeks before this sprint; not archived; 0 open issues | GitHub API |
| rustls 0.23 compatible? | **Yes** — `rustls = { version = "0.23", default-features = false, features = ["std"] }` at tag `v0.7.1` | `Cargo.toml` @ `v0.7.1` |
| Implements the signer? | **Yes** — `impl Signer for CngSigner`, `impl SigningKey for CngSigningKey` | `src/signer.rs` @ `v0.7.1` |
| Handles P-256? | **Yes** — `256 => &[SignatureScheme::ECDSA_NISTP256_SHA256]` | `src/signer.rs` |
| Handles the DER problem? | **Yes** — `p1363_to_der()` via `CryptEncodeObjectEx`/`X509_ECC_SIGNATURE`, with unit tests covering high-bit and stripped-zero cases | `src/signer.rs` |
| License | MIT/Apache-2.0 — compatible with OmniBridge's Apache-2.0 | `Cargo.toml` |

**Two integration constraints Research v1 did not surface:**

- **Default features pull `aws-lc-rs`**: `default = ["logging", "tls12", "aws-lc-rs"]`. OmniBridge
  uses `ring` everywhere. The dependency must be
  `rustls-cng = { version = "0.7", default-features = false, features = ["ring"] }` — otherwise the
  build drags in a second crypto provider with a heavier toolchain requirement.
- **The default branch is `dev`, not `master`.** `master` is a stale orphan pinned to
  `rustls = "0.20"` / `version = "0.1.0"`. Anyone auditing this crate by browsing the default
  GitHub view of `master` will reach a wrong conclusion. Pin to the **tag**.

**Classification per the brief's vocabulary:**

| Layer | Classification |
| --- | --- |
| rustls `SigningKey`/`Signer`/resolver seam | **OFFICIAL PLATFORM VERIFIED** |
| `rustls-cng` as a working CNG bridge for P-256 | **THIRD-PARTY LIBRARY VERIFIED** |
| End-to-end: TPM-held key → handshake → SPKI pin against the Linux daemon | **POC STILL REQUIRED** (POC-WIN-04) |

The third row is not pessimism. Nothing in any document proves that a certificate issued around a
non-exportable public key produces an SPKI byte-identical to what `Fingerprint::from_certificate_der`
extracts, and that is the whole trust anchor.

---

## 11. New findings from the repository re-audit

Beyond §5, the code review surfaced items absent from Research v1.

### 11.1 `unsafe_code = "forbid"` is workspace-wide

`desktop/Cargo.toml`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
```

`forbid` — not `deny` — **cannot be overridden by an inner `#[allow]`**. Every platform adapter
OmniBridge will write needs `unsafe`: Win32 FFI, CNG, `Security.framework`, `SecKeyCreateSignature`,
IOKit. `windows-sys` and `objc2`-family calls are unsafe by construction.

This is a **Wave 0 blocker nobody had noticed**, and it is trivially fixable — but only if it is
fixed deliberately. The lint must become per-crate rather than workspace-wide, so that
`omnibridge-core` and the capability crates keep `forbid` (which is a genuine security property worth
protecting) while adapter crates get `deny` with narrowly-scoped `#[allow]`. Doing it the other way
round — relaxing the workspace default — would silently drop the guarantee from the security core.
Filed as **ARCH-010 (P0, Wave 0)**.

### 11.2 `Store::open()` can silently destroy the trust store — today, on Linux

This sharpens R-08 from a future hardware risk into a **present-tense defect**.

```rust
// core/src/store.rs:143-149
if state_path.exists() && key_path.exists() {
    Self::load(dir, &state_path, &key_path)
} else {
    Self::initialize(dir, &state_path, &key_path)   // new identity + EMPTY peer list
}
```

and `initialize()` calls `write_key()` **and `write_state()`**, overwriting `state.json`.

`std::path::Path::exists()` returns `false` for *any* failure to read metadata, not just absence —
including `EACCES` and a broken symlink. So:

| Situation | Current behaviour | Correct behaviour |
| --- | --- | --- |
| Genuine first run (neither file) | initialize | ✅ initialize |
| `state.json` present, `identity.key` deleted | **initialize — overwrites `state.json`, wiping every paired peer** | refuse to start |
| `identity.key` present but metadata unreadable | **initialize — same destruction** | refuse to start |
| `identity.key` present, mode 0644 | `load` → `require_private_mode` errors ✅ | ✅ correct already |

The last row shows the code already knows how to fail correctly when it takes the `load` path. The
bug is that a missing-or-unreadable key **routes around** that path entirely.

The realistic trigger is not exotic: a partial restore from backup, a botched `rsync`, a
half-completed migration, or a permissions accident on the data directory. The result is a new
identity, an empty trust store, and every paired device silently broken — presented to the user as
if OmniBridge had simply started fresh.

**This must be fixed in Wave 0**, before any hardware backing exists to make it worse. SEC-009 is
rewritten in [25](25-IMPLEMENTATION-BACKLOG.md) to cover the software case first, and
[28 §7](28-WAVE-0-IMPLEMENTATION-SPEC.md) defines the identity failure states.

### 11.3 The identity seam is smaller than Research v1 estimated

Every consumer of the private key, workspace-wide:

| Location | Use |
| --- | --- |
| `core/src/tls.rs:288` | `.with_single_cert(…, identity.rustls_private_key())` |
| `core/src/tls.rs:342` | `.with_client_auth_cert(…, identity.rustls_private_key())` |
| `core/src/store.rs:209` | `write_atomic(path, self.identity.private_key_pkcs8_der(), 0o600)` |
| `core/tests/identity_and_store.rs:73` | test asserts the key round-trips |

`rustls_private_key()` is `pub(crate)` with **two** call sites, both in `tls.rs`, four lines apart
in shape. `private_key_pkcs8_der()` is `pub` with one production call site, in the persistence
path — which is exactly where it *should* remain for the software backing.

**The refactor is two call sites, not a rewrite.** This raises confidence in Wave 0's size
materially and is reflected in [28](28-WAVE-0-IMPLEMENTATION-SPEC.md).

### 11.4 `Platform::Linux` is hardcoded in the store, not derived

`store.rs:154` and `store.rs:188` both pass `omnibridge_proto::v1::Platform::Linux` literally, in
`initialize()` and `load()` respectively. So the platform identity is decided by the *storage*
layer. Any platform adapter must be able to supply it, which makes this a Wave 0 seam item
(small, but it would otherwise be discovered mid-Wave-5). Filed as **ARCH-011**.

### 11.5 `x11rb` may not actually be a compile blocker

[01 §5](01-CURRENT-ARCHITECTURE-AUDIT.md) lists `x11rb` as compile-blocking on Windows, macOS and
iOS. But the manifest pins it as `default-features = false, features = ["xfixes"]`, which is
x11rb's **pure-Rust** connection backend — and the crate's own comment in
`capabilities/clipboard/Cargo.toml` says so: *"Pure Rust (x11rb's own connection backend), so it
adds no C toolchain requirement."* Pure-Rust code with no `std::os::unix` usage will generally
compile for `*-pc-windows-msvc`; it simply will not *work*.

This sprint did not build for those targets, so the claim is **neither confirmed nor refuted** —
it is downgraded from "compile blocker" to "**to be settled by the Wave 0 compile gate**". The
reason to feature-gate `x11rb` is unchanged and still good (dead weight, and honest capability
reporting), but it should not be presented as a hard compile error until a build says so. It is
unconditional in the manifest today, which Research v1 got right.

---

## 12. Bibliography delta

Every source first consulted in this sprint, for [24](24-SOURCE-BIBLIOGRAPHY.md). All accessed
**2026-08-31**. P = primary.

| Publisher | Title / artefact | Locator | Supports | P |
| --- | --- | --- | --- | :-: |
| bugaevc | wl-clipboard releases; `v2.3.0` body; `src/wl-copy.c` @ `v2.2.1`, `v2.3.0` | `github.com/bugaevc/wl-clipboard` | V-01, §5.1 | ✅ |
| KDE | KWin `datacontroldevicemanager_v1.{h,cpp}`; commit history; tag ancestry of `764b723` | `github.com/KDE/kwin` | V-02 | ✅ |
| Debian | `kwin-wayland`, `wl-clipboard` across suites | `packages.debian.org` | §6 | ✅ |
| Canonical | `kwin-wayland`, `wl-clipboard` across releases | `packages.ubuntu.com` | §6 | ✅ |
| Fedora / local | `wl-clipboard-2.2.1^git20251124.e808203-2.fc44` | machine under test | V-11 | ✅ |
| Microsoft | Clipboard Formats — Cloud Clipboard and Clipboard History Formats | learn.microsoft.com | V-04 | ✅ |
| Microsoft | Interactive Services (Session 0) | learn.microsoft.com | §9.1, PLAT-DEC-002 | ✅ |
| Microsoft | `CreateNamedPipeA` | learn.microsoft.com | §9.2, SEC-003 | ✅ |
| Microsoft | Using the Clipboard — format listeners | learn.microsoft.com | §9.3, WIN-010 | ✅ |
| Microsoft | `DnsServiceRegister`, `DNS_SERVICE_INSTANCE` | learn.microsoft.com | V-03 | ✅ |
| Apple | TN3179 Understanding local network privacy (full text, JSON API) | developer.apple.com | V-05, V-09, §8 | ✅ |
| Apple | `NSPasteboard.AccessBehavior`; `NSPasteboard` full symbol list | developer.apple.com | V-06, PLAT-DEC-009 | ✅ |
| Apple | `UIBackgroundModes` | developer.apple.com | V-07 | ✅ |
| Apple | Protecting keys with the Secure Enclave | developer.apple.com | PLAT-DEC-004 | ✅ |
| Apple | `SecKeyCreateSignature`, `ecdsaSignatureMessageX962SHA256` | developer.apple.com | PLAT-DEC-004 | ✅ |
| Apple | `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` | developer.apple.com | §13 | ✅ |
| rustls | `SigningKey`, `Signer`, `CertifiedKey`, `ConfigBuilder` @ 0.23.43 | docs.rs | §10, Wave 0 | ✅ |
| rustls | `rustls-cng` `Cargo.toml` + `src/signer.rs` @ `v0.7.1`; repo metadata | github.com/rustls/rustls-cng | §10 | ✅ |
| briansmith | `ring` `BUILDING.md` | github.com/briansmith/ring | V-10 | ✅ |

---

## 13. Apple Secure Enclave: verified constraints

Not on the V-list, but the brief made it a central question and Research v1's Apple evidence was
partial. From *Protecting keys with the Secure Enclave*, verbatim:

> **Works only with NIST P-256 elliptic curve keys.** These keys can only be used for creating and
> verifying cryptographic signatures, or for elliptic curve Diffie-Hellman key exchange…
>
> **Can't encode preexisting keys. You must use the Secure Enclave to create the keys.** Not having
> a mechanism to transfer plain-text key data into or out of the Secure Enclave is fundamental to
> its security.
>
> Requires hardware support. Only iOS devices with an A7 or later processor, or a Mac with the Touch
> Bar and Touch ID or with an M1 or later processor support this feature.
>
> By specifying the [`.privateKeyUsage`] flag, you make the private key available for use in signing
> and verification operations inside the Secure Enclave. **Without the flag, key generation still
> succeeds, but signing operations that attempt to use it fail.**

Five consequences, three of them new:

1. **P-256-only confirms OmniBridge's most important accidental decision.** ADR-0006 chose P-256 for
   Android Keystore. It is the *only* algorithm the Enclave accepts. Verified, not inferred.
2. **Non-exportability is confirmed as fundamental**, which is the whole justification for
   replacing `key_pkcs8_der`.
3. **NEW — there is no migration path from a software key to an Enclave key.** Because preexisting
   keys cannot be imported, a device that starts with a software identity and later gains hardware
   backing must generate a **new identity**, which means **re-pairing every peer**. This is a
   product decision, not an implementation detail, and it must be made before the first Apple
   release ships a software fallback. Raised as **PLAT-DEC-015**.
4. **NEW — `.privateKeyUsage` is the same trap that already bit OmniBridge once.** Key generation
   succeeds and signing fails later, exactly as the Android v1 `DIGEST_NONE` keys did — and, as
   there, the access-control flags are immutable after creation. MAC-002 must cover it explicitly.
5. **NEW — Intel Macs without a T1/T2 have no Secure Enclave.** A software fallback on macOS is
   not optional, which makes the fallback *policy* (§11.2, PLAT-DEC-015) load-bearing rather than
   theoretical.

On accessibility class: the documentation recommends
`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, but qualifies it — *"generally preferred unless
your app operates in the background."* OmniBridge's agent does. The documented alternative,
`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, is *"recommended for items that need to be
accessed by background applications"* and *"do not migrate to a new device"* — which is precisely
the correct property for a device identity. **Recommendation: `AfterFirstUnlockThisDeviceOnly`,
`.privateKeyUsage`, no `.userPresence`.** Immutable after creation, so it must be right first time.

---

## 14. What this closeout changes

| Area | Research v1 | After verification |
| --- | --- | --- |
| KDE clipboard | "One question about wl-clipboard versions" | Answered. One broken configuration: **Ubuntu 26.04 LTS**, with exact cause |
| `wl-copy --sensitive` | "✅ identical, more effective on Plasma" | **Broken on Debian 13 and every Ubuntu LTS**. New P0 |
| Fedora wl-clipboard | Unknown | `2.2.1^git20251124` — has the features despite the version string. **Do not use a version dependency** |
| Windows exclusion formats | Two names, unverified | Three names, semantics verified. WIN-008 implementable |
| Windows agent vs service | PROPOSED (strong) | **Settled by first-party doc.** Also: needs an HWND + message pump |
| Named pipe security | "FIRST_PIPE_INSTANCE + per-SID DACL" | Default ACL grants **Everyone + anonymous read**. FIRST_PIPE_INSTANCE is *detection*; the policy on failure is the mitigation |
| macOS pasteboard | "Polling is the only mechanism" | Confirmed — **and** access is user-gated from macOS 15.4 |
| macOS local network | Not considered | **Applies from macOS 15.** Agents do not get the daemon exemption |
| macOS Developer ID | Distribution requirement | **Also a runtime correctness requirement** |
| iOS background modes | "Couldn't fetch the list" | Verified. Eleven values, none fits |
| iOS local network | "TN3179 unavailable" | Retrieved. **Listening does not need the privilege; Bonjour does** |
| Secure Enclave | P-256 only | Confirmed — plus **no import path**, `.privateKeyUsage` trap, Intel Macs excluded |
| rustls signer | "Documented by rustls itself" | Verified against 0.23.43, including unhashed-message and DER-encoding requirements |
| `rustls-cng` | "Already solves the hard part" | Verified in source. Plus: use `ring` not default features; pin the **tag**, not `master` |
| `filename.rs` | "Knows nothing about Windows" | **Refuted.** Real gaps are `:` (ADS) and `U+202E` (bidi, all platforms) |
| Identity regeneration | Future hardware risk | **Present-tense defect on Linux.** `Path::exists()` conflates absent with unreadable |
| `unsafe_code` | Not mentioned | **`forbid` workspace-wide** blocks every platform adapter |
| Identity refactor size | "The one real refactor" | **Two call sites** in `tls.rs` |

---

## 15. Verification scorecard

```
Research v1 external items (derived from 24 §7, not assumed):   12

  VERIFIED ................................................ 8
  PARTIALLY VERIFIED ...................................... 2   (V-03, V-09)
  STILL OPEN .............................................. 2   (V-08, V-12)
  REFUTED ................................................. 0
  BLOCKED ................................................. 0

Additional claims tested against the repository:              2 REFUTED (§5)
New defects discovered:                                       4 (§5.1, §7 ×2, §11.2)
New architectural blockers discovered:                        1 (§11.1)
```

Neither remaining open item blocks Wave 0. V-08 is a cosmetic hint with no dependent decision;
V-12 is a Wave 5/7 discovery question that is correctly a PoC. Both are outside Wave 0's scope
entirely — see [28 §14](28-WAVE-0-IMPLEMENTATION-SPEC.md).
