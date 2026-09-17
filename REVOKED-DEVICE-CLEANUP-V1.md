# Revoked Device Cleanup v1

Secure tombstones and "Remove from list" on Desktop and Android.

Branch `feature/revoked-device-cleanup-v1`, on top of `develop` at 5541140
(Quick Panel + Branding v1, PR #33).

---

## 1. Baseline

```
$ git branch --show-current
feature/revoked-device-cleanup-v1
$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md        (untouched, not staged)
$ git log -3 --oneline
5541140 Merge pull request #33 from yurisismotto/feature/quick-panel-branding-v1
5bcda90 feat(desktop): add quick panel and AnyFlow branding
4bebff6 Merge pull request #32 from yurisismotto/fix/ux-debt-retry-repair-scanner-insets
$ git diff --check
(clean)
```

The Quick Panel merge is the immediate ancestor.

**Real starting state on the certification host.** `~/.local/share/anyflow/state.json`
held 15 peer records: 11 revoked and visible, 4 trusted. Fourteen of the fifteen
were called `SM-X620`. No record had a `hidden` key, because none existed — which
made this a genuine migration case rather than a simulated one.

Byte-exact backup taken before any change:
`…/scratchpad/state.json.pre-sprint`, md5 `8ff91b5b0240b78205ec0192d32b1291`.

The tablet's own store was backed up the same way (`tablet-trust-store.pre.json`,
md5 `50b65fce6e21c94084bb1bf38059860f`): 4 trusted peers, none with a `revoked`
or `hiddenFromUi` key.

---

## 2. Desktop trust-store audit (pre-change)

### Where everything lives

| Concern | Location |
| --- | --- |
| Persisted peer record | `desktop/core/src/store.rs` — `TrustedPeer` |
| Fingerprint / SPKI pin | `TrustedPeer.fingerprint`, `core/src/fingerprint.rs` |
| Revoked state | `TrustedPeer.revoked: bool` |
| Device id, display name, platform | `TrustedPeer.device_id` / `device_name` / `platform` |
| Grants | `TrustedPeer.granted_capabilities: BTreeMap<String, bool>` |
| Capability policy | `clipboard_policy`, `notification_policy` |
| Timestamps | `paired_at_unix`, `last_protocol_version` |
| Addresses / discovery | **not stored per peer** — mDNS only, `core/src/discovery.rs` |
| Revocation | `Store::revoke_peer` |
| Pairing / re-pair | `Store::add_peer`, `runtime/src/state.rs::store_peer` |
| Schema / migration | `SCHEMA_VERSION = 2`, `StateFile`, `classify()` |
| GUI device lists | `gui/src/views/devices.rs`, `views/peers.rs`, `panel/model.rs` |
| Selected peer | `gui/src/selection.rs` → `$XDG_CONFIG_HOME/anyflow/gui.json` |
| Session admission | `core/src/session.rs::accept`, via `SessionHost::lookup_peer` |
| Peer control requests | `control/src/lib.rs`, handled in `runtime/src/server.rs` |

### A. What "revoked" meant

`revoked: bool` on the record, set by `revoke_peer`, which also cleared
`granted_capabilities` but **left `clipboard_policy` and `notification_policy`
as the user had set them**. `Store::trusted_peer` filtered revoked records out;
`Store::peer_record` returned them. `TrustedPeer::allows` returned false for any
revoked peer regardless of grants.

### B. Fields the refusal actually depends on

Exactly two: `fingerprint` (the map key and the pinned identity) and `revoked`.
`DaemonState::lookup_peer` reads `peer_record(fp)` and answers
`PeerStatus::Revoked` on `p.revoked` alone. Nothing else in the record
participates in the decision.

### C. Fields that are UI/history only

`device_id`, `device_name`, `platform`, `paired_at_unix`,
`last_protocol_version`, and — once the grant is gone — both policy objects.

### D. What deleting a revoked record did

Turned `REVOKED` into `UNKNOWN`. This is observable and not cosmetic
(`core/src/session.rs:512-600`):

| | outside a pairing window | inside one |
| --- | --- | --- |
| **Revoked** | `HelloAck{status: REJECTED}`, empty capability list, `Err(NotAuthorized)` | treated as unknown (`treat_as_unknown`) |
| **Unknown** | `HelloAck{status: PAIRING_REQUIRED}` + this daemon's advertised capabilities, then `Err(NotInPairingMode)` | full pairing path |

Both are refusals, so deletion was not an *immediate* hole. What it destroyed
was the distinction: the same key stopped being a device the owner had thrown
out and became a stranger, greeted differently, attributable to nothing, and
with no record anywhere that a decision had ever been made about it.

### E. Could a previously revoked fingerprint reconnect as unknown?

Only if its record was deleted. Nothing in the desktop deleted records — there
was no delete path at all. So the answer before this sprint was "no, because
the situation could not arise", not "no, because it is prevented".

### F. How a fresh pairing re-enabled a revoked fingerprint

`store_peer` builds a whole new `TrustedPeer` from the handshake and calls
`add_peer`, which inserts into a `BTreeMap` keyed on fingerprint — replacing the
record wholesale. Grants are the negotiated set intersected with
`settings.auto_grant` (which is `["battery.v1"]`). Certified already by
`daemon/tests/e2e.rs::a_revoked_device_can_pair_again_when_the_owner_opens_a_new_window`.

### G. Could grants or policies survive a re-pair?

Grants: no — `revoke_peer` cleared them and `add_peer` replaces the record.
Policies: **yes, they survived a revoke** (not a re-pair — the fresh record
carries defaults). A revoked record kept `allow_mirror: true` and the user's
clipboard directions. Inert, because every authorizer asks `trusted_peer` first,
but it was stale consent sitting on disk. Fixed here (§8).

### H. Stale in-memory peer state after a mutation

No. Every authorizer (`FilesAuthorizer`, `ClipboardAuthorizer`,
`NotificationAuthorizer`, `SessionHost::lookup_peer`) locks and re-reads the
store on every question. `do_unpair` additionally tears down sessions, cancels
transfers, clears renegotiation, bumps the clipboard epoch and closes
notification mirrors.

---

## 3. Android trust-store audit (pre-change)

### The brief's premise did not hold

> "A revoked device remains visible indefinitely in the device/peer lists on
> both desktop and Android."

This was true of the desktop and **false of Android, because Android had no
revoked state at all.** `TrustStore.TrustedPeer` had no `revoked` field, and the
only destructive action — `PeerDetailScreen`'s "Forget this device" — called
`TrustStore.removePeer`, which *deleted* the record:

```kotlin
fun removePeer(fingerprint: Fingerprint) {
    if (selectedPeerHex == fingerprint.toHex()) clearSelectedPeer()
    writePeers(peers().filterNot { it.fingerprint.contentEquals(fingerprint) })
}
```

It also had no confirmation dialog: one tap destroyed the pairing.

So on Android this sprint had to *build* the lifecycle rather than extend it.
That is a larger change than the brief anticipated, and it is a security
improvement rather than only a UX one — see §5.

### Where everything lives

| Concern | Location |
| --- | --- |
| Peer store | `store/TrustStore.kt` |
| Persisted record | `TrustStore.TrustedPeer`, JSON in `filesDir/trust-store.json` |
| Fingerprint / SPKI pin | `identity/Fingerprint.kt`, `net/PinnedTrustManager.kt` |
| Revoked state | **absent** |
| Pairing recovery | `TrustedPeer.mergePairing`, `upsertPairedPeer`, `pairing/PairingGate.kt` |
| Sensitive capabilities | `capability/SensitiveCapabilities.NEVER_AUTO_GRANTED` |
| Selected peer | `TrustStore.selectedPeerFlow`, `store/PeerTarget.kt` |
| Connection coordinator | `net/ConnectionCoordinator.kt`, `service/ConnectionService.kt` |
| Device UI | `ui/DevicesScreen.kt`, `ui/PeerDetailScreen.kt`, `ui/MainState.kt` |
| Forget action | `MainActivity.rememberMainActions().onForget` |
| Grants and policies | `setGrant`, `setClipboardPolicy`, `setNotificationPolicy` |

### The asymmetry that shapes the Android threat model

**Android never listens.** There is no `ServerSocket` anywhere in `app/src/main`;
`PeerConnection` only dials. So "the peer reconnecting" on Android means *this
phone dialling that computer*. A tombstone on Android therefore cannot be about
closing an inbound door — there isn't one. What it has to prevent is:

1. the phone dialling a computer it no longer trusts;
2. a later pairing of the same key silently inheriting the grants, the
   clipboard directions and the notification app list from the relationship the
   person ended;
3. the record vanishing with no account of what happened to it.

Deletion prevented (2) by accident — `mergePairing` saw `existing == null` — and
prevented neither (3) nor any attribution.

### The hardened invariants that had to survive

`mergePairing`'s existing contract (UX-DEBT-02, `PairingRecoveryTest`):

* same fingerprint with an existing record → facts refresh, decisions persist,
  grants unioned;
* `grantable = paired.grantedCapabilities - SensitiveCapabilities.NEVER_AUTO_GRANTED`
  applied in **every** branch, so a pairing can never invent a sensitive grant;
* a different fingerprint → new-peer semantics, inheriting nothing;
* a failed pairing never reaches `mergePairing`, so it mutates nothing.

All four are preserved. One condition was **added**: see §12.

---

## 4. Pre-change revoke / delete behaviour, side by side

| | Desktop (before) | Android (before) |
| --- | --- | --- |
| Withdraw trust | `anyflow unpair` / "Revoke this device" → `revoked = true` | "Forget this device" → record **deleted** |
| Confirmation | yes, `AdwAlertDialog` | **none** |
| Row afterwards | stays, badged *Revoked*, forever | gone |
| Record afterwards | kept | gone |
| Grants afterwards | cleared | gone with the record |
| Policies afterwards | **kept** | gone with the record |
| Same key pairs again | recognised as a return | met as a stranger |
| Remove from list | did not exist | did not exist |

---

## 5. Threat model

What "Remove from list" must not become:

1. **A silent downgrade to unknown.** The user asks to tidy a screen; if that
   deletes the record, the key is greeted with `PAIRING_REQUIRED` and the
   daemon's capability list instead of `REJECTED`, and the revocation has no
   trace. *Mitigation:* the record is replaced, never removed; `peer_record`
   still returns it and `lookup_peer` still answers `Revoked`.
2. **A back door to revoking.** If removing worked on a trusted device it would
   be an unconfirmed revoke wearing a tidy-up label. *Mitigation:* the store
   returns `HideOutcome::NotRevoked` and the UI never draws the action for a
   trusted device.
3. **A grant resurrector.** A hidden record is a *better* hiding place for a
   stale grant than a deleted one: invisible and still inherited. *Mitigation:*
   the tombstone carries no grants and no policies at all; on Android
   `mergePairing` additionally treats any revoked record as a new peer.
4. **A name-based bulk action.** Fourteen of fifteen records on the real host
   are called `SM-X620`. Matching on a name would remove the wrong device.
   *Mitigation:* the control request takes a full fingerprint hex and nothing
   else; the bulk path collects fingerprints from the store, never names.
5. **A silent re-target.** Clearing the chosen device and picking a survivor
   would send the next file somewhere the person did not choose. *Mitigation:*
   the choice is cleared only on a fingerprint match, and nothing chooses a
   replacement.
6. **A network dependency.** *Mitigation:* the operation is local on both
   platforms. No protobuf changed, no message is sent, no session is required.

---

## 6. Chosen tombstone representation

One additive boolean beside the existing one, on both platforms.

**Desktop** — `TrustedPeer.hidden: bool`, `#[serde(default)]`:

```
revoked=false hidden=false   TRUSTED
revoked=true  hidden=false   REVOKED_VISIBLE
revoked=true  hidden=true    REVOKED_TOMBSTONE
revoked=false hidden=true    impossible — read as revoked=true (fail closed)
```

**Android** — `TrustedPeer.revoked` and `TrustedPeer.hidden`, persisted as
`"revoked"` and `"hiddenFromUi"`, **written only when true** so a store of
ordinary trusted computers is byte-identical to what a build without this
feature wrote.

No parallel store. The existing record expresses both concepts, and keeping
them in one place is what lets `peer_record` stay the single answer to "who is
this key?".

**Reading is where the invariant is enforced,** not writing: `hidden` implies
`revoked`, decided on load (`Store::load`, `TrustedPeer.fromJson`) rather than
believed from the file. A hand-edited or truncated store that says "off the
list and still trusted" is read as revoked.

---

## 7. Minimum retained fields, and why

| Field | Kept | Why |
| --- | --- | --- |
| `fingerprint` | yes | It *is* the identity — the pinned SPKI the TLS session is checked against and the key `lookup_peer` is asked about. Without it there is no revocation. |
| `revoked` | yes | The whole decision. `lookup_peer` reads this and nothing else. |
| `hidden` | yes | Presentation. Distinguishes a tombstone from a revoked row the user has not dealt with yet. |

That is the complete list. Nothing else is needed to keep refusing a key.

---

## 8. Removed metadata, and why

Both platforms, on removal:

| Field | Becomes | Why |
| --- | --- | --- |
| `device_id` / `deviceId` | `""` | History. Also stops any selector naming the tombstone. |
| `device_name` / `deviceName` | `""` | A display name for something no longer displayed. |
| `platform` | `0` | History. |
| `paired_at_unix` | `0` | History of a relationship that ended. |
| `last_protocol_version` (desktop) | `0` | History. |
| `granted_capabilities` | empty | A grant that is not stored cannot come back on a re-pair. |
| `addresses` (Android) | empty | A convenience for dialling something the phone decided not to dial. |
| `clipboard_policy` | `DENIED` | See below. |
| `notification_policy` | `DENIED` | See below. |

**`DENIED`, not `default()`.** Functionally identical — every authorizer asks
about trust first — but a *default* policy serialises as `allow_send: true` and
`allow_mirror: true`, and a tombstone whose line in the trust store reads
`allow_mirror: true` is a sentence an auditor has to reason their way out of.
On Android it matters more than cosmetically: `NotificationPolicy()`'s
`knownApps` is the cache of every application installed on the phone — 97
package names on the certification tablet. `DENIED` empties it. Measured on
hardware: `known 97` before revoking, `known 0` after (§20).

`revoke_peer` / `asRevoked()` now apply the same reset, so the policies go at
the moment the person says no rather than at the moment they tidy up. That
closes audit finding **G** from §2.

---

## 9. Desktop UX

The peer-management surface in this application is the **Trusted peers** page
(`gui/src/views/peers.rs`), which is where "Revoke this device" already lives;
the page labelled *Settings* is this computer's own identity. The action was
added where revocation already is.

A revoked card now reads:

```
SM-X620                                              [ Revoked ]
Device fingerprint
1B27 07BA 7D28 9DA2 9CE7 7A6F 29D1 3026 …
Device id ca10b2bf9504eda1585f6feb2c2fecd9
Revoked. This device cannot connect until it pairs again.
──────────────────────────────────────────────
[ Remove from list ]
```

There is **no "Pair again" button**, deliberately. Re-pairing is
`anyflow pair` opening a window with a fresh single-use token and a human
confirming a fingerprint; this page has no way to invoke that flow, and a
button that only navigated elsewhere would be architecture invented to match a
mock-up. Recorded as a debt (§24).

Confirmation:

```
Remove SM-X620 from the list?
The device will stay revoked and cannot reconnect unless you pair it again.
                                                    [ Cancel ]  [ Remove ]
```

Cancel is the default and the close response. Nothing says delete, erase, key,
certificate, blacklist or unrevoke.

After confirmation the row disappears on the next poll, in the same daemon
process; Settings stays usable; the Quick Panel is unaffected (it already
filtered revoked devices, `panel/model.rs:887`); and if the removed fingerprint
was the chosen device the choice is cleared with nothing put in its place.

A CLI verb was added alongside, because a fingerprint-addressed local operation
should be scriptable and it is what made the physical evidence below
reproducible:

```
anyflow remove-from-list <full-fingerprint-hex>
anyflow remove-from-list --all
```

---

## 10. Desktop bulk cleanup

Offered only when **more than one** visible revoked device exists, drawn from
the same set the action operates on — so "nothing to remove" and "no button"
are one fact rather than two that could disagree.

```
Revoked devices
11 revoked device(s) are still listed here. Removing them from the
list does not un-revoke them.
[ Remove all revoked devices ]
```

```
Remove 11 revoked devices from the list?
They will remain revoked and cannot reconnect unless paired again.
Devices you still trust are not affected.
                                            [ Cancel ]  [ Remove 11 ]
```

`Store::hide_all_revoked_peers` builds the whole new peer map and writes the
document **once**, committing to memory only if the write succeeded
(`Store::commit`). The trust store is a single JSON document, so a per-device
loop that failed halfway would leave the file describing a state nobody asked
for.

Rules, all tested: only revoked *visible* records; trusted records untouched;
existing tombstones not rewritten; no name matching anywhere; the choice
cleared only if it named one of the removed fingerprints.

---

## 11. Android UX

`DevicesScreen` now draws `state.listedPeers` (trusted + revoked-visible)
instead of `state.peers` (trusted only). A revoked row loses its capability
chips and its Connect button and gains a sentence:

```
[ Revoked ]
anyflow-u2604
Desktop · Linux
This device can no longer connect.
```

Tapping it opens a detail screen that is **not** the ordinary one with switches
greyed out — a row of disabled switches invites "can I turn these back on?",
and the answer (pair it again from a fresh code) is not something a switch can
say. It shows the state, the still-pinned fingerprint, and one action:

```
[ Revoked ]   anyflow-u2604
This device can no longer connect.

Security
Device fingerprint   1315 96BD 9834 BA6F
This key is still pinned and still refused. Pairing again means
scanning a new code and checking the fingerprint on both screens.

[ Remove from list ]
```

```
Remove anyflow-u2604 from the list?
This device will stay revoked and cannot reconnect unless you pair it again.
                                                [ Cancel ]   [ Remove ]
```

`Material3 AlertDialog`, the pattern already used on the phone. Cancel is the
dismiss action and the tap-outside action.

**"Forget this device" became "Revoke this device"** and gained a confirmation.
That is the change that gives Android the lifecycle at all: the button used to
delete on a single tap. What was one destructive action is now two deliberate
ones, matching the desktop's vocabulary.

No bulk action on Android. The screen has no multi-select idiom and inventing
one would be the "new interaction language" the brief rules out; the phone also
does not accumulate revoked computers the way a desktop accumulates revoked
phones.

All new strings are in `res/values/strings.xml` (`device_revoke_*`,
`device_remove_from_list_*`, `device_revoked_*`, `action_cancel`) and reached
through `stringResource`. The project ships one locale (`values`, plus
`values-night` for colours); no other locale exists to fall out of step.

---

## 12. Selection semantics

The rule, stated once per platform as a pure function and tested there:

```rust
// desktop — gui/src/selection.rs
pub fn forget_if(&self, fingerprint: &str) -> bool
```
```kotlin
// android — TrustStore.Companion
fun selectionAfterRemoval(selectedHex: String?, removedHex: String): String?
```

Both compare the fingerprint hex and do exactly one thing: clear, or leave
alone. Neither can produce a *new* selection — there is no path from a removal
to a choice, which is what "does not auto-select another peer" means at the
layer that decides.

With the choice gone, resolution is the existing one:

* desktop `Target::resolve` — one trusted peer → `OnlyTrustedPeer`, several →
  `MustChoose`, and a stored choice naming nothing → `MustChoose { stale_choice: true }`;
* Android `PeerTarget.resolve` — `OnlyTrustedPeer` / `MustChoose` / `NoTrustedPeer`.

Each platform's documented one-peer convenience is untouched. Nothing new was
invented for the removal itself.

Measured on hardware: pointing the tablet's stored choice at a tombstoned
fingerprint and restarting the app produced *"Choose a device above to send
to."* and *"Several devices are paired. Use Connect on the one you want."* — it
neither dialled the tombstone nor picked a survivor (§20).

---

## 13. Re-pair semantics

Unchanged protocol, unchanged ceremony. A tombstoned key takes the same path a
revoked one always took: refused outright with no pairing window open; inside a
window it must present a proof of a token generated seconds earlier and a human
must confirm the fingerprint.

* **Desktop.** `store_peer` builds a fresh `TrustedPeer` from the handshake with
  `hidden: false`; `add_peer` replaces the tombstone in a map keyed on
  fingerprint, so recovery yields exactly one row. Grants are the negotiated set
  intersected with `auto_grant` (`battery.v1`), so nothing from the old
  relationship returns.
* **Android.** `mergePairing` gained one condition:

  ```kotlin
  if (existing == null || existing.revoked || !existing.fingerprint.contentEquals(paired.fingerprint))
  ```

  A revoked record now takes the **new-peer** branch. Without it, a tombstone
  would have been a better hiding place for a stale grant than a deleted record
  was. This does **not** regress UX-DEBT-02: a *desktop-side* revoke leaves the
  phone's own record untrevoked, so the person's local decisions still survive
  somebody else's revoke — only their own revoke erases them. Pinned by
  `a desktop side revoke still leaves this phone's decisions intact`.

No "unrevoke" endpoint exists on either platform. There is no way to move a
record out of `revoked` except a completed pairing.

---

## 14. SensitiveCapabilities analysis

`SensitiveCapabilities.NEVER_AUTO_GRANTED = { clipboard.v1, notifications.v1 }`
is untouched. The non-escalation boundary is still applied once, inside
`mergePairing`, to both branches.

This sprint makes it *harder* to escalate, in two places:

1. a revoked record now inherits nothing at all on re-pair, so the union that
   `NEVER_AUTO_GRANTED` guards has an empty left-hand side;
2. `asRevoked()` clears the policies as well as the grants, so there is no
   stored `allowMirror` or clipboard direction for a later pairing to find.

Verified on the desktop after a real tombstone recovery: `clipboard.v1` and
`notifications.v1` were not present in the record at all, `files.v1` was
`false`, `battery.v1` was `true` (the auto-grant policy).

---

## 15. Active-session analysis

`do_unpair`'s teardown was **extracted**, not duplicated:

```rust
async fn enforce_revocation(state: &Arc<DaemonState>, fingerprint: &Fingerprint)
```

It cancels in-flight `files.v1` transfers (a separate TCP connection that would
otherwise outlive the control session), shuts the session down, drops it,
clears any renegotiation, bumps the clipboard auto-send epoch and closes
notification mirrors. `do_unpair`, `do_hide_revoked` and `do_hide_all_revoked`
all call it. Removing from the list is not expected to find a session — revoking
took it down — but "which of the two kill paths ran?" is not a question a
revocation should ever raise.

Authorizers continue to fail closed independently: each re-reads the store on
every question, and a tombstone is never a `trusted_peer`.

Certified by `a_live_session_does_not_survive_a_removal`: revoke with a live
session up, remove from the list, session gone, and the same client's
reconnection refused with `NotAuthorized`.

---

## 16. Persistence and migration

**Desktop.** `SCHEMA_VERSION` stays **2**. `hidden` is `#[serde(default)]`, so
a record without it reads as visible. Bumping the version would make older
builds refuse the file for a flag they can safely ignore.

Migration behaviour, exactly:

* every existing revoked peer stays **VISIBLE**. Nothing is hidden for anybody;
* trusted peers, their grants and both policies are preserved byte-for-byte;
* no cryptographic pin is touched;
* the daemon does not rewrite `state.json` on load. **Measured:** the new build
  opened the 15-record store and the file's md5 was unchanged.

**Android.** `SCHEMA_VERSION` stays **1**, the same reasoning `selectedPeer`
used. Both flags are optional and written only when true, so a file containing
only trusted computers is byte-identical to what the previous build wrote — a
downgrade sees nothing new. **Measured:** `adb install -r` of the new APK over
the old one left `trust-store.json` md5-identical, with all four peers trusted,
visible and still selected.

**Failing safely.**

* desktop: an unparseable `state.json` is already a refusal to start
  (`classify` → `IDENTITY_CORRUPTED`), and the store is never wiped to simplify
  anything;
* both: `hidden && !revoked` is read as revoked;
* Android: a record with a missing or malformed fingerprint is dropped rather
  than guessed — a record with no pinned key is not an identity;
* desktop: `Store::commit` restores the previous peer map if the write fails, so
  memory cannot get ahead of the disk in the permissive direction.

---

## 17. Local IPC changes

**No device-to-device protocol change. No protobuf change.** Nothing in
`protocol/proto` was touched and no message crosses the network for this
feature.

The **local control IPC** (`anyflow-control`, the Unix socket between the CLI/GUI
and the daemon) gained two additive request variants:

```rust
Request::HideRevokedDevice { fingerprint: String }
Request::HideAllRevokedDevices
```

Call it **local control IPC v2 (additive)**. Compatibility:

* an older CLI or GUI against a new daemon is unaffected — no existing variant,
  field or response changed;
* a new GUI against an older daemon gets `malformed request: unknown variant`,
  which the GUI surfaces as an error and does not treat as success;
* the socket is local-only, in `XDG_RUNTIME_DIR`, unreachable from the network,
  and no peer message can reach it.

`HideRevokedDevice` takes a **full fingerprint hex**, not the `device` selector
every other device-addressed request takes. A selector is right for something a
person types and wrong here: the record being addressed has had its device id
and name cleared, and the operation must act on one cryptographic identity
rather than on whatever shares a name with it. Malformed input is refused
without touching anything.

`resolve_device` was narrowed to `listed_peers` and given an empty-needle guard,
so no CLI selector — a former device id, a fingerprint prefix, or the empty
string against a tombstone's now-empty device id — can name a tombstone.

---

## 18. Unit / JVM tests

**Desktop — 34 new (898 total passing, 0 failures).**

`desktop/core/tests/revoked_tombstone.rs` — 16:

| # | Test |
| --- | --- |
| D3, D4 | `a_revoked_device_is_listed_until_it_is_removed_and_never_after` |
| D5 | `a_tombstone_is_still_a_record_and_still_revoked` |
| D2 | `a_trusted_device_cannot_be_removed_from_the_list` |
| | `removing_a_device_that_is_not_there_is_an_error_not_a_write` |
| | `removing_a_tombstone_again_is_a_no_op` |
| D6 | `removing_one_device_leaves_a_same_named_device_alone` |
| D9 | `a_tombstone_keeps_the_key_and_nothing_else` |
| D10 | `revoking_clears_the_grants_and_the_policies_that_go_with_them` |
| D11, D12 | `pairing_again_over_a_tombstone_restores_one_visible_trusted_record` |
| D13, D14 | `a_tombstone_grants_nothing_to_a_different_fingerprint` |
| D15 | `a_tombstone_survives_a_restart` |
| D16 | `a_store_without_the_field_reads_every_revoked_device_as_visible` |
| D17 | `a_record_that_says_hidden_but_not_revoked_is_read_as_revoked` |
| | `the_schema_version_is_unchanged_because_the_field_is_additive` |
| D18, D19 | `bulk_removal_touches_revoked_visible_devices_and_only_those` |
| D20 | `bulk_removal_with_nothing_revoked_writes_nothing` |

`desktop/daemon/tests/revoked_cleanup.rs` — 12, over the real control socket and
a real TLS 1.3 handshake with real pinning:

| # | Test |
| --- | --- |
| D1, D4 | `a_revoked_device_is_listed_until_it_is_removed` |
| D2 | `a_trusted_device_cannot_be_removed_from_the_list` |
| | `a_malformed_fingerprint_is_refused_without_touching_anything` |
| **D5** | `a_device_removed_from_the_list_is_still_refused_at_the_door` |
| D6 | `removing_one_device_leaves_a_same_named_device_listed` |
| D11, D12 | `a_fresh_pairing_brings_a_removed_device_back_exactly_once` |
| D10 | `a_failed_pairing_leaves_the_tombstone_exactly_as_it_was` |
| D13, D14 | `a_different_key_is_a_new_device_and_the_tombstone_gives_it_nothing` |
| D18, D19 | `bulk_removal_takes_the_revoked_and_leaves_the_trusted` |
| D20 | `bulk_removal_with_nothing_revoked_says_so_and_changes_nothing` |
| **D21** | `a_live_session_does_not_survive_a_removal` |
| | `a_tombstone_cannot_be_named_by_any_device_selector` |

`gui/src/selection.rs` — 4 (D7, D8) and `gui/src/panel/model/tests.rs` — 2 (D8 at
the resolution layer).

**Android — 29 new (688 total passing, 0 failures).**
`app/src/test/.../RevokedDeviceCleanupTest.kt`:

| # | Covered by |
| --- | --- |
| A1 | `a revoked device offers remove from list and nothing else` |
| A2 | `a trusted device never offers remove from list`, `a revoked device is never offered connect` |
| A5 | `a tombstone survives being written and read back`, `a revoked but visible record survives…` |
| A6 | `a revoked computer is not in the trusted set the coordinator resolves over`, `a stale choice naming a revoked computer resolves to nothing` |
| A7 | `removing the chosen computer clears the choice`, `removing another computer leaves the choice alone`, `removing never chooses a replacement`, `with the choice cleared and several computers left the app asks` |
| A8 | `removing one computer leaves a same named computer untouched` |
| A9 | `pairing again after a revoke restores trust and no old grant` |
| A10 | `a failed pairing leaves the tombstone exactly as it was` |
| A11 | `pairing again after a revoke…` (asserts every member of `NEVER_AUTO_GRANTED`) |
| A12 | `a tombstone gives nothing to a different fingerprint` |
| A13 | `pairing again over a visible revoked record inherits nothing either` |
| A14 | `pairing over a tombstone yields one row, not two` |
| — | migration, fail-closed, privacy: `a record written before this sprint reads as trusted and visible`, `an ordinary trusted record is written exactly as an older build wrote it`, `a record that says hidden but not revoked is read as revoked`, `a record with no usable fingerprint is dropped rather than guessed`, `nothing in the persisted record could hold content` |
| — | regression guard: `a desktop side revoke still leaves this phone's decisions intact` |

**A3 and A4 are not headless.** `TrustStore(context)` needs a `Context` and a
`filesDir`, and this project has no Robolectric; a Compose UI test would have to
be instrumented, and running `connectedDebugAndroidTest` uninstalls the
application, which would have destroyed the tablet's pairing and every piece of
physical evidence in §20. They were certified by hand on the tablet instead
(§20, P11a/P11b), and the *rules* behind them — which actions a row offers — are
covered headlessly by A1/A2.

To make that possible, the Android rules were factored into pure functions
(`TrustedPeer.fromJson`/`toJson`/`asRevoked`/`asTombstone`/`mergePairing`,
`TrustStore.upserted`/`trustedOf`/`listedOf`/`selectionAfterRemoval`,
`UiMapping.deviceActions`/`deviceStateDescription`) so the screens and the tests
read the same rule rather than two copies of it.

---

## 19. GUI / Compose tests

`cargo test -p anyflow-gui -- --ignored --test-threads=1` — the one
display-gated widget-tree test passes unchanged.

The real GUI evidence for this feature is the AT-SPI capture in §20: the tree
was read out of the running application on the certification host, and the
counts (11 `Remove from list`, 4 `Revoke this device`, 1 bulk) are the
assertions D1 and D2 make, on the real trust store.

No Compose UI test was added, for the uninstall reason in §18.

---

## 20. Physical evidence — Fedora host and SM-X620 tablet

Real hardware throughout. No VM was booted. Builds were serialised, `-j 2` for
cargo and `--no-daemon --max-workers=2` for Gradle.

### Desktop — Fedora 44, daemon `target/debug/anyflowd`, real `~/.local/share/anyflow`

| Step | Result |
| --- | --- |
| **P1** at least one revoked visible peer exists | **PASS** — 15 listed: 11 revoked, 4 trusted, 14 of them named `SM-X620` |
| migration | **PASS** — the new build read a store with no `hidden` key anywhere; `state.json` md5 unchanged (`8ff91b5b…`), all 11 revoked peers still **visible** |
| a11y, before | **PASS** — AT-SPI: 11 `[button] 'Remove from list'`, 4 `[button] 'Revoke this device'`, 1 `[button] 'Remove all revoked devices'`, heading `'Revoked devices'`, `'11 revoked device(s) are still listed here…'` |
| confirmation wording | **PASS** — `[alert] 'Remove SM-X620 from the list?'` / `'The device will stay revoked and cannot reconnect unless you pair it again.'` / `[button] 'Cancel'` `[button] 'Remove'` |
| Cancel changes nothing | **PASS** — `state.json` md5 unchanged after pressing Cancel |
| **P2** remove one from the desktop list | **PASS** — pressed through the GUI via AT-SPI; `1B27 07BA` gone |
| **P3** no daemon restart | **PASS** — daemon pid `2320176` before and after; 15 → 14 listed, 11 → 10 revoked |
| **P6** selection cleared, not redirected | **PASS** — `gui.json` had `selected_peer = 1b2707ba…`; after removal the key is **absent**. Nothing was chosen in its place |
| tombstone on disk | **PASS** — `device_id ""`, `device_name ""`, `platform 0`, `paired_at_unix 0`, `granted_capabilities {}`, `revoked true`, `hidden true` |
| **P4** trusted SM-X620 unaffected | **PASS** — `573C CB84` stayed `paired yes / connected yes`, grants `battery.v1, clipboard.v1, files.v1`, live session throughout |
| **P5** removed fingerprint still rejected | **PASS** — see below |
| **P7** fresh secure re-pair of the tombstoned identity | **PASS** — see below |
| **P8** it returns exactly once | **PASS** — 1 row for that fingerprint, 16 records before and after |
| **P9** no sensitive grant silently restored | **PASS** — `clipboard.v1` and `notifications.v1` not present at all; `files.v1 false`; `battery.v1 true` |
| bulk removal | **PASS** — `[alert] 'Remove 10 revoked devices from the list?'` → 15 listed → **5**, 10 revoked → **0**, 5 paired unchanged, 16 records and 11 tombstones retained, same daemon pid |
| **D19** on real records | **PASS** — all 4 originally-trusted records compared field by field against the pre-sprint backup: `device_id`, `device_name`, `platform`, `paired_at_unix`, `granted_capabilities`, `last_protocol_version`, `revoked`, both policies — **unchanged** |
| **D20** on real records | **PASS** — with 0 revoked devices the bulk button is not drawn at all, and no `Remove from list` button exists anywhere; 5 `Revoke this device` remain |
| audit log | **PASS** — 11 lines `removed a revoked device from the list; the revocation stands peer=<short fingerprint>`. No token, proof, key, clipboard or notification content anywhere in the log |

**P5 / P7 / P8 / P9 in detail.** The eleven pre-existing revoked fingerprints
are dead identities from previous app installations — no device still holds
those keys, so none of them can attempt a reconnection. To exercise the gate
with a client that *does* hold its key, the repository's own
`daemon/examples/fake_phone` was used: a real TLS 1.3 client speaking the real
protocol with real pinning, on the real host, against the live daemon and the
live trust store. It is the dev tool, not the tablet — stated plainly rather
than dressed up.

```
# paired for real: token, proof, and a human-equivalent confirmation
fake phone identity: 8D0D 2032 042A 7F9E
TLS established and server identity pinned
session established with Fedora (DF65 D3E4 BA28 EDF9)
$ anyflow pair …  →  Pair with this device? [y/N]  →  Paired with 8D0D 2032 042A 7F9E.

# revoked, then reconnecting — REVOKED_VISIBLE
$ anyflow unpair 8d0d…            revoked 8D0D 2032 042A 7F9E
$ fake_phone connect              TLS established and server identity pinned
                                  Error: peer is not authorized

# removed from the list — REVOKED_TOMBSTONE
$ anyflow remove-from-list 8d0d…  removed 8D0D 2032 042A 7F9E from the list; it stays revoked
$ anyflow devices | grep 8D0D     (nothing)

# P5 — the same client, the same key, the same path
$ fake_phone connect              TLS established and server identity pinned
                                  Error: peer is not authorized
```

TLS still completes and the pin still holds; it is the **application-layer
admission** that refuses, which is the right shape — the transport was not
weakened to produce the rejection.

```
# P7 — the ordinary pairing ceremony, nothing bypassed
$ anyflow pair --ttl 600 → fake_phone pair <payload> → confirmed by hand
session established with Fedora

# P8 — exactly one row for that identity, 16 records before and after
# P9 — granted: battery.v1 true, files.v1 false, clipboard.v1 and
#      notifications.v1 absent entirely
```

### Android — SM-X620 (the certification tablet; the briefs' "Galaxy S25")

APK installed with `adb install -r`, same debug signer
(`744a1108…e9771e0c`), so `filesDir` and the Keystore key were preserved.

| Step | Result |
| --- | --- |
| migration | **PASS** — `trust-store.json` md5 unchanged (`50b65fce…`) across the update; 4 peers, all trusted and visible, `selectedPeer` preserved |
| **P10** create a revoked desktop identity visible in Android | **PASS** — `Revoke this device` → `Revoke anyflow-u2604?` → Revoke; record became `revoked true / hidden false`, grants `[]`, addresses `0`, `knownApps` **97 → 0** |
| **A3** Cancel on the revoke dialog | **PASS** — trust store md5 unchanged |
| revoked row presentation | **PASS** — badge `Revoked` (a word, with a content-desc), name, `This device can no longer connect.`, **no capability chips and no Connect button**, while the three trusted rows all keep theirs |
| revoked detail screen | **PASS** — state, fingerprint `1315 96BD 9834 BA6F`, `This key is still pinned and still refused…`, one action `Remove from list`; no permission switches, no second Revoke |
| **P11a** Cancel on the remove dialog | **PASS** — `Remove anyflow-u2604 from the list?` / `This device will stay revoked and cannot reconnect unless you pair it again.`; Cancel left the store md5 unchanged (**A3**) |
| **P11b / A4** Remove | **PASS** — record became `deviceName ""`, `deviceId ""`, `revoked true`, `hidden true`, grants `[]`; **4 records before and after — nothing deleted** |
| **P12** row disappears without an app restart | **PASS** — gone from the list immediately |
| **A5** persistence across a restart | **PASS** — `am force-stop` + relaunch: the tombstone is still in the file and still absent from the list; the *revoked-visible* `anyflow-d13` is still listed and still badged `Revoked` |
| **P13 / A6** normal reconnect blocked | **PASS** — the app's stored choice was pointed at the tombstoned fingerprint and the app restarted. It resolved to **MustChoose**: *"Choose a device above to send to."* and *"Several devices are paired. Use Connect on the one you want."* It neither dialled the tombstone nor selected a survivor |
| **A7** selection cleared when the selected peer is revoked | **PASS** — selected `anyflow-d13` through its own Connect button (`selectedPeer = b52cda20…`), revoked it, and `selectedPeer` became `null` — cleared, not redirected; with two trusted peers left the app asks |
| trusted peers unaffected | **PASS** — `Fedora` and `anyflow-u2404` kept their grants and policies throughout; re-selecting Fedora reconnected immediately (`Connected to Fedora`, desktop reports `connected yes`, battery 79%) |
| **P14 / P15 / P16** fresh QR re-pair on Android | **BLOCKED** — see below |

**Why P14–P16 are BLOCKED, honestly.** Recovering a tombstoned identity on
Android needs an optical QR scan: `QrPayload.parse` has exactly one call site
(`ui/PairingScanner.kt`), reached only from the zxing `CaptureActivity` result,
and the manifest declares **no deep-link intent filter** for the `anyflow1:`
scheme — verified, not assumed. So the payload cannot be delivered over `adb`;
a person must point the tablet's rear camera at the code, and the tablet has a
PIN lock.

There is a second obstacle that matters more. The only identity the tablet
could re-pair with is **Fedora**, this host — the other three peers are VM
desktops, and §19 forbids booting VMs. Testing recovery would therefore mean
revoking and tombstoning the *working* Fedora pairing and leaving the tablet
disconnected until a person scanned a fresh code. That is a destructive,
outward-facing change to the user's device that I am not going to make
unasked.

The same lifecycle *is* certified end to end on real hardware with a real
handshake — on the desktop, at P5/P7/P8/P9 — and the Android half of it
(`mergePairing` over a tombstone, no duplicate row, no sensitive grant, nothing
inherited) is covered headlessly by A9–A14. What remains unproven is
specifically the Android QR + camera leg.

---

## 21. Accessibility

**Desktop.** Captured from the running application over AT-SPI:

```
[grouping] '' desc='SM-X620, revoked. This device can no longer connect.'
  …
  [grouping] 'Revoked'
    [label] 'Revoked'
  [label] 'Revoked. This device cannot connect until it pairs again.'
  [button] 'Remove from list'
      desc='Removes SM-X620 from the list. It stays revoked and cannot
            reconnect unless you pair it again.'
[button] 'Remove all revoked devices'
    desc='Removes 11 revoked device(s) from the list. They stay revoked.
          Devices you still trust are not affected.'
```

* the destructive action has a text label, not an icon;
* the revoked state is a word, three times over (badge, sentence, card
  description) — never colour alone;
* the card carries the device name *and* the state in one description, so they
  are announced together as well as separately;
* the confirmation is an `AdwAlertDialog`: focusable, keyboard-operable, with
  Cancel as both the default response and the close response.

Two accessibility bugs were found and fixed by taking this evidence rather than
assuming it. `gtk::accessible::Property::Label` on a `Button::with_label` is
**silently ignored** — the longer string never reached the bus — so the extra
context is now a `Description`, which is additive and leaves the accessible
*name* matching the visible text (what voice control needs). And a plain
`gtk::Box` is role `generic`, which AT-SPI does not surface at all, so the card
now sets `AccessibleRole::Group` before its description; before that line the
property was being dropped on the floor.

**Android.**

* the badge renders the word `Revoked` with a matching `content-desc`;
* `This device can no longer connect.` is a real text node on both the row and
  the detail screen;
* the capability chips already state allowed/not allowed in words;
* the revoked row exposes no Connect button rather than a disabled one;
* `AlertDialog` is the platform's own: focusable, dismissable, Cancel on the
  tap-outside path;
* the row also sets `stateDescription` from `UiMapping.deviceStateDescription`,
  which is additive (it does not replace the children the N3 work fixed).
  **Honest limit:** `uiautomator dump` does not serialise `stateDescription`, so
  that one property was not independently observed on the device. The string
  itself is unit-tested; its delivery is one line of wiring, unverified.

---

## 22. Privacy and security review

**What a tombstone retains:** one public fingerprint and two booleans. Nothing
else. Verified against the real `state.json` after the bulk cleanup: of 11
tombstones, **none** has a grant and **none** has a name.

**What it does not persist**, checked rather than asserted: addresses (desktop
never had per-peer addresses; Android's are cleared), clipboard data,
notification data — including the `knownApps` cache, 97 package names on the
tablet, now emptied at revoke time — file history, QR tokens, proofs, ephemeral
challenges.

**Audit logging.** Existing trust and revocation events are unchanged. The new
operation logs one line per device at INFO: the action and a shortened public
fingerprint, the same shape `do_unpair` already used. No new telemetry system,
no analytics, no cloud, no network call.

**Regression gates, explicitly checked:**

| | Status |
| --- | --- |
| TLS 1.3 | untouched; `fake_phone` completed the handshake against the live daemon before being refused |
| SPKI pinning | untouched; "server identity pinned" on every attempt, including the refused ones |
| Pairing proof-of-possession | untouched; recovery required a live token and a hand confirmation |
| QR freshness / single-use token | untouched; a second window issued a new token |
| Fingerprint-based routing | strengthened — the new request takes a full fingerprint and `resolve_device` can no longer name a tombstone |
| SensitiveCapabilities | strengthened (§14) |
| Files approval | untouched |
| Notification grants / policy | strengthened — policies are now cleared at revoke |
| Clipboard grants | strengthened, same reason |
| Fail-closed unknown/revoked | strengthened — `hidden && !revoked` reads as revoked; `Store::commit` rolls back a failed write |

No gate weakened.

---

## 23. Quality gates

```
$ cargo fmt --all --check                                          CLEAN
$ cargo test --workspace -j 2                                      898 passed, 0 failed (59 suites)
$ cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings
                                                                   0 warnings
$ cargo test -p anyflow-gui -- --ignored --test-threads=1          1 passed
$ ./gradlew --no-daemon --max-workers=2 :app:testDebugUnitTest :app:assembleDebug
                                                                   688 passed, 0 failed; BUILD SUCCESSFUL
```

`connectedDebugAndroidTest` was **not** run: this sprint adds no instrumented
test, and running it uninstalls the application, which would have destroyed the
tablet's pairing and the §20 evidence. No cargo and Gradle build ran
concurrently.

---

## 24. Files changed

```
 android/app/src/main/java/.../store/TrustStore.kt              | 423 ++++++++++---
 android/app/src/main/java/.../ui/DevicesScreen.kt              |  38 +-
 android/app/src/main/java/.../ui/MainActivity.kt               |  23 +-
 android/app/src/main/java/.../ui/MainState.kt                  |  45 ++-
 android/app/src/main/java/.../ui/PeerDetailScreen.kt           | 166 +++++-
 android/app/src/main/java/.../ui/UiMapping.kt                  |  55 ++-
 android/app/src/main/java/.../ui/components/Cards.kt           |  21 +-
 android/app/src/main/res/values/strings.xml                    |  24 ++
 desktop/cli/src/main.rs                                        |  30 ++
 desktop/control/src/lib.rs                                     |  23 ++
 desktop/core/src/store.rs                                      | 212 +++++++-
 desktop/core/tests/identity_and_store.rs                       |   1 +
 desktop/daemon/examples/fake_phone.rs                          |   1 +
 desktop/gui/src/panel/model/tests.rs                           |  47 +++
 desktop/gui/src/selection.rs                                   |  92 +++++
 desktop/gui/src/views/mod.rs                                   |  12 +
 desktop/gui/src/views/peers.rs                                 | 172 +++++++
 desktop/runtime/src/server.rs                                  | 186 +++++--
 desktop/runtime/src/state.rs                                   |  20 +-
 19 files changed, 1473 insertions(+), 118 deletions(-)

new:
 android/app/src/test/java/.../RevokedDeviceCleanupTest.kt      (29 tests)
 desktop/core/tests/revoked_tombstone.rs                        (16 tests)
 desktop/daemon/tests/revoked_cleanup.rs                        (12 tests)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was not touched and is not staged.

---

## 25. Remaining debts

1. **Android QR re-pair of a tombstone is uncertified on hardware** (§20). It
   needs a person with the tablet and a temporary sacrifice of the working
   Fedora pairing. Everything except the camera leg is covered.
2. **No "Pair again" button** on the desktop revoked card. Re-pairing is a
   daemon-side window plus a human confirmation, and the Trusted peers page has
   no way to invoke that flow; a button that only navigated elsewhere would be
   invented architecture. A real affordance would mean letting the GUI open a
   pairing window scoped to one fingerprint.
3. **`stateDescription` on the Android device card is unverified on device**
   (§21) — `uiautomator dump` does not serialise it.
4. **A3/A4 are not headless on Android** (§18). Robolectric, or a Compose test
   in a variant that does not uninstall the app, would close this.
5. **No bulk "Remove all revoked" on Android.** Deliberate (§11), reconsider if
   phones start accumulating revoked computers.
6. **The desktop's chosen device still lives in `gui.json`**, not the daemon —
   a pre-existing debt from the Quick Panel sprint, unchanged here. It is why
   clearing the choice happens in the GUI rather than beside the trust-store
   write.
7. **Two of the tablet's stale VM pairings are now revoked** (`anyflow-u2604`
   tombstoned, `anyflow-d13` revoked-visible) as a direct result of the physical
   certification the brief asked for. They are dead identities for powered-off
   VMs. A full undo is `adb push` of
   `…/scratchpad/tablet-trust-store.pre.json` over
   `files/trust-store.json`; the Keystore key never changed, so the records come
   back trusted.

---

## 26. Git

```
$ git branch --show-current
feature/revoked-device-cleanup-v1

$ git status --short
 M android/app/src/main/java/io/github/yurisismotto/anyflow/store/TrustStore.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/DevicesScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainState.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PeerDetailScreen.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/UiMapping.kt
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/components/Cards.kt
 M android/app/src/main/res/values/strings.xml
 M desktop/cli/src/main.rs
 M desktop/control/src/lib.rs
 M desktop/core/src/store.rs
 M desktop/core/tests/identity_and_store.rs
 M desktop/daemon/examples/fake_phone.rs
 M desktop/gui/src/panel/model/tests.rs
 M desktop/gui/src/selection.rs
 M desktop/gui/src/views/mod.rs
 M desktop/gui/src/views/peers.rs
 M desktop/runtime/src/server.rs
 M desktop/runtime/src/state.rs
?? REVOKED-DEVICE-CLEANUP-V1.md
?? android/app/src/test/java/io/github/yurisismotto/anyflow/RevokedDeviceCleanupTest.kt
?? desktop/core/tests/revoked_tombstone.rs
?? desktop/daemon/tests/revoked_cleanup.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md

$ git diff --check
(clean)
```

Nothing staged, nothing committed, nothing pushed, no PR opened.

---

## 27. Acceptance gates

```
DESKTOP REVOKED CLEANUP .................. PASS
ANDROID REVOKED CLEANUP .................. PASS
REVOCATION TOMBSTONE SECURITY ............ PASS
FRESH RE-PAIR RECOVERY ................... PASS (desktop, on hardware)
                                           BLOCKED (Android QR leg — §20)
SELECTION SAFETY ......................... PASS
SENSITIVE GRANT NON-ESCALATION ........... PASS
PERSISTENCE / MIGRATION .................. PASS
ACCESSIBILITY ............................ PASS
SECURITY / PRIVACY REGRESSION ............ PASS
```

**REVOKED DEVICE CLEANUP V1: PASS**

No security-blocking failure. The one BLOCKED item is a hardware-access
limitation on one leg of one gate — an optical QR scan needing a person at a
PIN-locked tablet, and a working pairing I declined to break unasked — not a
defect and not a weakened invariant. The same recovery path is certified end to
end on real hardware on the desktop, and its Android rules are pinned
headlessly.

---

## 28. PR #34 CI follow-up

Two CI failures on PR #34, fixed here. Neither was a product defect, and
**nothing in §1–§27 changed** — no production code, no security invariant, no
physical evidence.

### Android — `:app:compileDebugAndroidTestKotlin`

**What passed already.** `:app:assembleDebug` and `:fixture:assembleDebug` were
green. The application builds.

**Root cause, and it is mine.** The instrumented source set still referenced the
pre-sprint API. This sprint deliberately did not *run*
`connectedDebugAndroidTest` — it uninstalls the app and would have destroyed the
tablet's pairing and the §20 evidence — but I wrongly let that decision stand in
for compiling it too. `assembleDebugAndroidTest` compiles instrumented tests
**without installing or running anything on the tablet**, so there was never a
reason to skip it. It is now part of the local gate list.

**No hard-delete API was restored.** `TrustStore.removePeer` stays gone. Every
call site was moved to the real lifecycle — `revokePeer` then `hideRevokedPeer`
— which is what those tests actually needed: a peer that holds no grant, no
policy and no row.

| File | Change |
| --- | --- |
| `ClipboardPersistenceTest.kt` | Two cleanup sites moved to a shared `cleanUp()` helper that revokes and then removes from the list. `a_forgotten_computer_leaves_no_clipboard_policy_behind` renamed to `revoking_a_computer_scrubs_its_clipboard_policy_and_the_tombstone_keeps_it_denied` and re-aimed at the real rule. |
| `NotificationHardwareGateTest.kt` | `tearDown` revokes and then removes the synthetic `0x7e` peer. |
| `NotificationUiFixtures.kt` | `state()` gained the real `listedPeers`; `Recorder.actions()` replaced `onForget` with `onRevoke` and `onRemoveFromList`. |

The renamed clipboard test is a **stronger** claim than the one it replaces.
The old one asserted "the policy is gone because the whole record is gone",
which says nothing about a record that stays. It now asserts that revoking
*scrubs* the grants and the clipboard policy from the record itself — read back
through `peerRecord`, from a freshly opened store, so the assertion is against
the file — and that the tombstone keeps answering `DENIED` afterwards. That is
the invariant that matters now precisely *because* the record survives: a stale
`autoReceive` on a revoked peer is consent waiting to be resurrected by a
re-pair.

`NotificationUiFixtures.state()` needed `listedPeers` to be a **real value**,
not an empty placeholder. `MainUiState.peerByHex` now resolves against the
listed set, so an empty list would have handed every consent screen a null peer
and rendered "This device is no longer paired" instead of the thing under test —
a compile fix that silently voided the tests. It defaults to the same single
trusted peer as `peers`, with a parameter for a future revoked-row test.

The two `Recorder` lambdas **record** rather than no-op. No consent screen
should ever ask to revoke a computer, and a recorder is what lets a test say so;
an empty lambda would hide it.

**The two "Cannot infer type for parameter" errors were cascading**, as
suspected — not a separate defect. Both failing tests are
`fun … = runBlocking { … }` whose block *ended* on a `store.removePeer(peer)`
call; with that reference unresolved Kotlin could not infer the lambda's return
type. Both disappeared once the API calls were corrected, with no separate fix.
That also explains why the reported line numbers were 57 and 141 and not 118 —
the middle test ends on an `assertEquals`, so its block type was still
inferable.

**Result:** `:app:assembleDebugAndroidTest` → BUILD SUCCESSFUL.

### Windows MSVC — the classification guard

**What passed already.** `cargo check` and `cargo build`
`--locked --no-default-features --target x86_64-pc-windows-msvc` both completed
for the full portable package set. This was **not** a portability regression.

The job stopped at *"Core test files unchanged (guard for the exclusion
below)"*, because `desktop/core/tests/` gained `revoked_tombstone.rs` and the
guard requires every new core test target to be classified deliberately. That
is the guard doing exactly its job.

**Classification: unix-fs adapter.** Established by inspection *and* measurement
rather than assumed:

* every case in the file goes through `Store::open(tempdir)`, and several read
  and rewrite `state.json` directly to exercise schema migration, the
  hidden-implies-revoked fail-closed rule and survival across a restart;
* `Store::open` is `#[cfg(feature = "unix-fs")]` (`core/src/store.rs`);
* compiling it with the feature off fails with
  `E0599: no associated function or constant named 'open' found for struct
  anyflow_core::store::Store` — run locally to confirm, not inferred.

So it belongs beside `identity_and_store` and `identity_states`, and **not** in
the portable `--no-default-features` test set. The portable half of this
feature — what a tombstone does to admission — lives in `anyflow-daemon`'s
`revoked_cleanup` suite, which is outside this job's crate set in any case.

**Workflow change** (`.github/workflows/portable-windows-msvc.yml`), one step,
nothing weakened:

1. `'revoked_tombstone.rs'` added to the explicit `$expected` core test-file
   guard;
2. the comment above it corrected from "`anyflow-core` has **two**" to
   "**three**", naming `identity_and_store`, `identity_states` and
   `revoked_tombstone`;
3. the reason recorded inline, including the measured `E0599`;
4. `revoked_tombstone` **not** added to the portable
   `cargo test --no-run --no-default-features` list;
5. the guard itself untouched — no wildcard, no skip, no relaxation.

**Honest limit:** a local Fedora run proves nothing about the MSVC runner. The
change here is a *classification*, and the Windows job remains the certification
source; it has to go green after push before this is settled.

### Gates after the fix

```
:app:assembleDebugAndroidTest                    BUILD SUCCESSFUL
:app:testDebugUnitTest :app:assembleDebug        688 passed, 0 failed; BUILD SUCCESSFUL
cargo fmt --all --check                          CLEAN
cargo test --workspace -j 2                      898 passed, 0 failed (59 suites)
cargo clippy --locked --workspace --all-targets --all-features -j 2 -- -D warnings
                                                 0 warnings
```

`connectedDebugAndroidTest` was not run and the tablet app was not reinstalled.
