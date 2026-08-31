# 11 — iOS / iPadOS feasibility

| Field | Value |
| --- | --- |
| **Title** | AnyFlow on iOS and iPadOS, honestly scoped |
| **Status** | Research / Draft |
| **Last reviewed** | 2026-08-31 |
| **Scope** | What an iOS AnyFlow client can and cannot be. Discovery, permission, pairing, TLS, lifecycle, background, clipboard, files, share extension. |
| **Decision status** | PROPOSED. **PLAT-DEC-005** (iOS background expectations) is the gating decision and is OPEN. |
| **Evidence** | OFFICIAL DOC VERIFIED where Apple documentation was retrievable; several key pages are JS-rendered and could not be fetched — marked **EXTERNAL VERIFICATION REQUIRED**. |
| **Related documents** | [03](03-PLATFORM-CAPABILITY-MATRIX.md), [12](12-APPLE-SECURITY-AND-INTEGRATION.md), [15](15-CROSS-PLATFORM-CLIPBOARD.md), [16](16-CROSS-PLATFORM-FILES.md), [17](17-BACKGROUND-EXECUTION-MODEL.md) |

---

## 1. Verdict, stated before the detail

**An iOS AnyFlow can exist and can be genuinely useful. It cannot be an Android AnyFlow, and
no amount of engineering will change that.**

The whole platform reduces to one sentence:

> **iOS suspends the app shortly after it leaves the foreground, and while suspended the
> system may reclaim the app's sockets.**

(OFFICIAL DOC VERIFIED — Apple's background-execution documentation and TN2277: *"while the
app is suspended the system may choose to reclaim resources out from underneath a network
socket used by the app, thereby closing the network connection represented by that socket"*,
and *"iOS puts strict limits on background execution, and its default behavior is to suspend
your app shortly after the user has moved it to the background"*.)

Everything AnyFlow's Android client does — a persistent authenticated session, a foreground
service, clipboard watching, background file receive — depends on the process continuing to
run. On iOS it does not.

So the design question is not *"how do we keep the session alive?"* It is
**"what is a good product when the session only exists while the app is open?"** That question
has a decent answer, and §9 gives it.

The trap this document exists to prevent: reaching for push notifications and a relay server to
simulate an always-on connection. That would introduce a cloud dependency into a product whose
first principle is that there isn't one. **Rejected explicitly in §6.4.**

---

## 2. Toolchain

| Item | Finding |
| --- | --- |
| Rust target | `aarch64-apple-ios` is **Tier 2**, rustup-distributed. `aarch64-apple-ios-sim` for the simulator (OFFICIAL DOC VERIFIED) |
| Static library | Rust builds a staticlib; linked into the app |
| Bindings | **UniFFI** generates Swift bindings from one interface definition, and is production-proven for exactly this shape in `mozilla/application-services` (OFFICIAL DOC VERIFIED, UniFFI user guide). Alternative: a hand-written C ABI + Swift wrapper |
| Build host | Mac + Xcode required |
| Distribution | App Store or TestFlight only. No sideloading path worth planning around |

Compiling the core is not the hard part and should not be treated as the risk.

---

## 3. Local network permission — the first gate

iOS 14 introduced local network privacy. An app that talks to devices on the LAN must declare:

```xml
<key>NSLocalNetworkUsageDescription</key>
<string>AnyFlow finds your paired computer on this Wi-Fi network.</string>
<key>NSBonjourServices</key>
<array>
  <string>_anyflow._tcp</string>
</array>
```

(OFFICIAL DOC VERIFIED — Apple requires both keys; *"Apps that use Bonjour must also declare
the services they browse, using the `NSBonjourServices` key"*, and a missing declaration
produces a `NoAuth` error.)

Behavioural consequences that shape the UX:

- The user is prompted **once**, at the first local-network operation.
- If they deny, **the app cannot re-prompt**. Recovery is Settings → Privacy & Security →
  Local Network. AnyFlow must detect denial and say exactly that, with a deep link to Settings.
- The service type must be listed literally, and `_anyflow._tcp` matches what
  `core/src/lib.rs` already advertises (`SERVICE_TYPE = "_anyflow._tcp.local."`). **No protocol
  change needed.**
- The prompt fires on browsing *and* on connecting to a local address — so it cannot be
  deferred past pairing.

Apple's TN3179 is the authoritative reference and could not be retrieved through this
session's tooling. **EXTERNAL VERIFICATION REQUIRED** for the finer points: exactly which
operations trigger the prompt, and how denial surfaces to `NWBrowser` versus a raw socket.

**POC-IOS-02** exists to measure this first, because a denied permission with no in-app
recovery is a support burden that must be designed for, not discovered.

---

## 4. Discovery

AnyFlow's direction is fixed: the desktop advertises, the mobile client browses
([ADR-0005](../../adr/ADR-0005-lan-discovery-mdns.md), and `daemon/src/mdns.rs`). iOS inherits the
browse role, matching Android's `NsdManager`.

| Option | Assessment |
| --- | --- |
| **`NWBrowser`** (Network.framework) | Apple's current API; integrates with local-network privacy; the recommended route |
| `NSNetServiceBrowser` | Deprecated |
| `mdns-sd` in Rust | **Do not.** It would bind 5353 alongside `mDNSResponder` on a platform that is strict about it, and it would sidestep the local-network privacy machinery in a way Apple is unlikely to reward at review |

**Recommendation: `NWBrowser` in Swift, handing resolved endpoints down to the Rust core.**
This is one of the few places where the platform API is clearly right and the shared code is
clearly wrong. The core already accepts addresses from outside — `core/src/discovery.rs`
defines the record model but contains no responder or browser, so there is a natural seam.

---

## 5. TLS and identity

`rustls` + `ring` compiles for iOS; the pinning verifiers are unchanged.

Identity is the Secure Enclave, which supports **only** P-256 — the algorithm AnyFlow already
uses (OFFICIAL DOC VERIFIED; see [10 §5.1](10-MACOS-FEASIBILITY.md) for why this is fortunate).
The same custom `rustls::sign::SigningKey` written for macOS should work on iOS, since
`SecKeyCreateSignature` is available on both. **The macOS work is ~90% of the iOS identity
work** — an argument for sequencing macOS before iOS.

The user-presence constraint from [10 §5.2](10-MACOS-FEASIBILITY.md) applies with equal force:
no Face ID/Touch ID gate on the identity key, because `Signer::sign` is called synchronously
inside a handshake. `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` + `.privateKeyUsage`.

---

## 6. Background execution — the constraint, in detail

### 6.1 What actually happens

| Phase | State | Session |
| --- | --- | --- |
| App foreground | Running | ✅ Alive |
| User switches app / locks | Background, briefly running | ⚠️ Alive for seconds |
| Shortly after | **Suspended** — no code runs | ❌ Dead. The socket may be reclaimed at any moment |
| Memory pressure | **Terminated** | ❌ Gone |
| User reopens | Relaunched or resumed | Must re-discover, re-connect, re-handshake |

`UIApplication.beginBackgroundTask` buys a bounded grace period (historically on the order of
tens of seconds, not guaranteed) to *finish* something — enough to complete an in-flight
clipboard send or close cleanly, not enough to keep a session.

### 6.2 Background modes, honestly assessed

`UIBackgroundModes` values and whether AnyFlow may legitimately use them:

| Mode | Legitimate for AnyFlow? |
| --- | --- |
| `audio` | **No.** Abuse. Would be rejected and deserves to be. |
| `location` | **No.** AnyFlow has no location purpose and declares no location permission on Android either — the manifest calls that out explicitly. |
| `voip` | **No.** AnyFlow is not a VoIP app. This was the classic keep-alive abuse and Apple closed it. |
| `bluetooth-central` / `bluetooth-peripheral` | **No** — unless AnyFlow one day genuinely uses BLE, which is a different product decision |
| `external-accessory` | No |
| `fetch` (Background App Refresh) | **Maybe, marginally.** Opportunistic, system-scheduled, no guaranteed timing. Could poll a paired desktop occasionally. Very weak. |
| `processing` (`BGProcessingTask`) | **Maybe.** Long tasks when charging and idle. Wrong shape for clipboard, possibly usable for a deferred file transfer |
| `remote-notification` | Requires **APNs** → §6.4 |

**Conclusion: no background mode legitimately supports an always-connected LAN session, and
AnyFlow must not pretend otherwise.** The exact current list of values could not be fetched
from Apple in this session — **EXTERNAL VERIFICATION REQUIRED** — but the conclusion does not
depend on the edges of the list.

### 6.3 What *is* possible

- **Foreground:** everything. Full session, files both ways, clipboard both ways (subject to
  §7), battery, pairing.
- **Brief background:** finish an in-flight operation via `beginBackgroundTask`.
- **On next foreground:** reconnect, receive whatever the desktop queued.
- **`URLSession` background transfers:** genuinely continue while suspended — but they are
  HTTP(S) against a URL, not AnyFlow's protobuf-over-TLS-1.3 session. Using them would mean a
  second, HTTP-shaped protocol with its own authentication. **Rejected** for v1; noted in
  [16](16-CROSS-PLATFORM-FILES.md) as the only mechanism that could ever give iOS background
  file transfer, at a protocol cost that is not currently worth paying.

### 6.4 Push / cloud — rejected, with the reasoning recorded

APNs could wake the app when the desktop has something to send. It would require:

- an APNs-capable server (Apple does not deliver device-to-device pushes on the LAN);
- the desktop reaching that server, i.e. **internet dependency for a LAN product**;
- a device token registry, i.e. **an account or a persistent identifier held by a third party**;
- and, since a push payload is delivered via Apple, a new party in the trust path.

That contradicts *local-first, no required cloud, no telemetry*, which is not a preference but
the product's identity.

**Recommendation: no APNs, no relay, no cloud in v1, and not as a way to make iOS resemble a
desktop.** If it is ever revisited it must be an explicit, opt-in, separately-designed feature
with its own threat model — never a quiet dependency. Recorded as **PLAT-DEC-005**, with the
recommended direction being *accept the foreground-only model*.

---

## 7. Clipboard

| Direction | Feasibility |
| --- | --- |
| **iOS → desktop, manual** | ✅ With `UIPasteControl`, or a paste into an AnyFlow text field, or the Share Sheet with selected text |
| **iOS → desktop, automatic** | ❌ **Platform restriction.** Reading requires either a paste gesture or a prompt, and there is no background execution to watch from |
| **Desktop → iOS, applied while foreground** | ✅ `UIPasteboard.general.string = …`; **writing has never required permission** |
| **Desktop → iOS, applied while background** | ❌ Not running |
| **Desktop → iOS, applied on next foreground** | ✅ The realistic design |

The read prompt is precise: it "is generated whenever your app accesses the pasteboard
directly, that is, not going through the Paste menu command, the keyboard shortcut, or
`UIPasteControl`" (OFFICIAL DOC VERIFIED). `UIPasteControl` is a system button the user taps,
which hands the app the pasteboard contents without a prompt — **exactly the right primitive
for "send my clipboard to my computer"**, because it makes the user's intent explicit, which
is what AnyFlow wants anyway.

`UIPasteboard.detectPatterns(for:)` can test for patterns *without* triggering the notification
— useful for showing "you have a URL copied" affordances, and **not** a way to read content.
Do not use it to work around consent.

Android is the precedent for handling this well: `ClipboardTileService` cannot read the
clipboard (no input focus since Android 10) so it opens the Activity and lets the human
confirm. The iOS design is the same shape with a different button.

**The UI must not offer an "automatic clipboard sync" toggle on iOS.** Per the product rule in
[03 §5](03-PLATFORM-CAPABILITY-MATRIX.md): a capability the platform forbids is absent, not
present-and-broken.

---

## 8. Files

| Direction | Mechanism |
| --- | --- |
| **Send** | Share Sheet (an `NSExtension` share extension, or the main app as a share target) and `UIDocumentPickerViewController`. Mirrors Android's `SendActivity` + `ACTION_SEND` |
| **Receive** | Written into the app's container; surfaced in the Files app via `UISupportsDocumentBrowser` / `LSSupportsOpeningDocumentsInPlace`, or "Save to Files" via the Share Sheet |
| **Background receive** | ❌ Not running |
| **Resume after suspension** | ❌ Not with the current protocol |

There is **no user-visible Downloads directory on iOS**, so `destination.rs`'s entire XDG
resolution chain has no meaning. `files.v1` needs no protocol change — an offer carries a
filename and never a path, which is exactly right here — but the *destination* concept must be
replaced by "the app's Documents directory, exposed through the Files app".

A Share Extension raises a real architectural wrinkle: extensions run in **a separate process**
with their own sandbox. To send a file from the Share Sheet it needs access to the identity
key and the trust store, which means an **App Group** shared container and a Keychain access
group — and a Secure Enclave key created with the right access group. This is not hard but it
must be designed at the start, not retrofitted. → [12](12-APPLE-SECURITY-AND-INTEGRATION.md).

Details in [16](16-CROSS-PLATFORM-FILES.md).

---

## 9. What an honest iOS AnyFlow looks like

Given all of the above, the product is:

> **A foreground companion.** Open AnyFlow, it finds your computer in about a second,
> reconnects, and shows what is waiting. Send a file from anywhere with the Share Sheet. Send
> your clipboard with one tap. Receive whatever your computer sent while you were away, the
> moment you open the app.

That is a coherent, useful product. It is **not** "your clipboard follows you around", and the
marketing, the UI and the onboarding must never imply that it is.

Concretely:
- **Do not** show "Connected" as a persistent state. Show "Connected" only while it is true.
- **Do** make reconnect fast enough to feel like it never disconnected — that is where the
  engineering effort should go, not into fighting suspension.
- **Do** let the desktop queue clips and offers for a peer that is not currently connected, so
  the next foreground is productive. This is a **desktop-side** feature and needs no protocol
  change — the desktop already holds pending clips in memory for
  `CLIPBOARD_OUTCOME_PENDING_USER`.
- **Do** use the Share Sheet aggressively. It is the one place iOS gives an app first-class
  entry from anywhere in the system, and it maps perfectly onto AnyFlow's manual model.

### 9.1 iPadOS

Materially the same as iOS, with three differences that all favour it:

- Multitasking (Stage Manager, Split View) means the app is **visible and unsuspended for far
  longer** in normal use, so the foreground-only model bites less.
- Larger screen — the macOS-style layout is viable.
- Hardware keyboard: `⌘V` is a paste gesture, which is one of the routes that avoids the
  pasteboard prompt.

**iPadOS is the better first Apple-mobile target of the two**, and it costs almost nothing
extra once iOS exists.

---

## 10. PoCs

| ID | Question |
| --- | --- |
| **POC-IOS-01** | `NWBrowser` finds `_anyflow._tcp` published by the existing Linux daemon; TXT keys parse; addresses resolve. |
| **POC-IOS-02** | Local network permission: when is it prompted, what does denial look like in code, what is the recovery UX? |
| **POC-IOS-03** | Full pairing against the existing `anyflowd`: QR scan, proof, confirmation, trust store persistence. |
| **POC-IOS-04** | Rust core via UniFFI + a Secure Enclave `SigningKey` completing a mutual TLS 1.3 handshake with SPKI pinning. |
| **POC-IOS-05** | Foreground lifecycle: connect, disconnect, reconnect, network change, app switch and return — measure reconnect time. |
| **POC-IOS-06** | **Background suspension measurement.** How long does a TLS session survive backgrounding, on a real device, on battery, with and without `beginBackgroundTask`? Does the desktop see a clean close or a hang? **This is the gate for PLAT-DEC-005.** |
| **POC-IOS-07** | Clipboard: `UIPasteControl` send, foreground receive, and confirmation that no prompt appears on the `UIPasteControl` path. |
| **POC-IOS-08** | Share Extension with an App Group + Keychain access group reaching the identity and trust store. |
| **POC-IOS-09** | Files send and receive; Files-app visibility; behaviour when the app is suspended mid-transfer. |

---

## 11. Recommended direction

1. **Scope iOS as a foreground companion.** Write it down in the product docs before writing
   code, so nobody has to discover it from a bug report.
2. **Sequence iOS after macOS** — the Secure Enclave signer, the UniFFI surface and the SwiftUI
   patterns are shared, and macOS is the cheaper place to get them wrong.
3. **`NWBrowser` for discovery**, Rust for everything security-critical.
4. **UniFFI**, subject to POC-IOS-04.
5. **`UIPasteControl` for clipboard send.** No automatic clipboard on iOS, ever, and no toggle
   suggesting otherwise.
6. **Share Sheet as the primary send entry point**, with the App Group designed in from day one.
7. **No APNs. No relay. No cloud.**
8. **iPadOS ships alongside iOS** and may be the more compelling of the two.
9. **Run POC-IOS-06 before committing to the platform at all.** If a session cannot survive even
   an app-switch-and-return without a jarring reconnect, the product may not be worth building
   — and that is a legitimate outcome for this research to have made visible.
