# `notifications.v1` — N6: final certification

**Branch:** `cert/notifications-v1-n6-final-certification`
**Baseline:** `836cbd4` — N5 merged (PR #22), working tree clean at the start.
**Mode:** read-only audit and certification. No feature was added, nothing was
refactored, no protocol was touched, and nothing was committed, pushed or
opened as a PR.

---

## 1. Executive summary

`notifications.v1` was audited against every promise N0–N5 made, re-run against
its whole deterministic suite, and exercised once more on the certification
hardware. **No P0 and no BLOCKER was found.** Two minor findings are recorded,
both pre-existing, neither blocking, and neither a security or correctness
defect.

What N6 establishes with its **own** evidence, gathered today:

| | Result |
| --- | --- |
| Rust workspace | **703 passed, 0 failed**, 22 ignored; `fmt` clean; clippy **0 warnings** |
| Android JVM | **523 / 0 / 0 / 0** — `skipped = 0` |
| Android connected, on SM-X620 | **see §26** |
| Real D-Bus gates | **7 of 10 executed and passed**; 3 declined (§28) |
| Real logind lock gate | **2 passed** |
| GTK widget-tree gate (N3 accessibility) | **1 passed** |
| N6 regression soak, real GNOME 50.4 | **302 s, 2652 cycles**, 0 phantom dismisses, bounded, converged |
| Hardware mid-session grant convergence | **PASS — 6.18 s**, no manual disconnect |
| Hardware reconnect / resync | **PASS** — `snapshot complete named=3 closed=0`, no grace expiry |
| Hardware revocation | **PASS** — 3 mirrors closed immediately, nothing mirrored afterwards |
| Persistence, both sides | **PASS** — zero notification content or history |
| Logging, both sides, live | **PASS** — canary sweep clean, capture non-vacuous |
| Protocol immutability | **PASS** — `protocol/` is byte-identical to N0's commit |

The design's central claim survives the audit intact: **every question is asked
per peer, per message, against the pinned TLS identity and the local trust
store; a role is never an authorization input; and the one message that can
change state on the phone has no field capable of carrying anything but an
opaque 16-byte name.**

### The two findings

| # | Finding | Class | Blocking? |
| --- | --- | --- | --- |
| F1 | One user-facing string in the Android notification settings screen is a Kotlin literal rather than a string resource ([NotificationSettingsScreen.kt:112](android/app/src/main/java/io/github/yurisismotto/anyflow/ui/NotificationSettingsScreen.kt#L112)) | i18n hygiene, introduced N3 | No |
| F2 | On One UI 8, `settings get secure enabled_notification_listeners` does not reflect a grant made with `cmd notification allow_listener` — the *read* is unreliable, not only the write N5 found | test-environment | No |

Neither touches the wire, the trust chain, privacy, or any product behaviour a
user can reach. Both are recorded in §33.

---

## 2. Baseline

```console
$ git branch --show-current
cert/notifications-v1-n6-final-certification
$ git status
On branch cert/notifications-v1-n6-final-certification
nothing to commit, working tree clean
$ git log --oneline -6
836cbd4 Merge pull request #22 from yurisismotto/feature/notifications-v1-n5-hardening
e72bb09 feat(notifications): harden recovery and session convergence
85f8424 Merge pull request #21 from yurisismotto/feature/notifications-v1-n4-dismiss-sync
778ac05 ci: classify N4 notification tests and avoid duplicate runs
b7531d0 feat(notifications): implement dismiss synchronization
8d8948a Merge pull request #20 from yurisismotto/feature/notifications-v1-n3-consent-ui
$ git diff --check
(clean)
```

**Certification hardware.** Samsung SM-X620 (`gts10fepwifi`), Android 16 /
One UI 8, serial `RX2Y500C7SY`, over USB, on `192.168.68.63`. Desktop: Fedora
44, GNOME Shell 50.4 on Wayland, `192.168.68.70`. Probed live by the daemon at
startup:

```
notification server  gnome-shell 50.4 (spec 1.2, GNOME)
                     body_markup=true persistence=true dismiss_reporting=true
lock source          org.freedesktop.login1.Session.LockedHint
                     on /org/freedesktop/login1/session/_32
```

Device identity for this wave: `3B38 1925 F8A6 E49D`, device id
`a1ea1a3e6ef54a7fabdafd45e95e4d2d`. Desktop identity `DF65 D3E4 BA28 EDF9`,
unchanged from N5.

---

## 3. Certification scope

**Read in full before any conclusion was drawn:** ADR-0015, ADR-0016,
ADR-0017, `docs/architecture/NOTIFICATIONS.md`,
`protocol/proto/anyflow/v1/capabilities/notifications_v1.proto`, and the N0–N5
reports.

**Audited as code, not as prose:** the portable contract
(`desktop/core/src/notifications.rs`), the Linux sink
(`desktop/capabilities/notifications/`), the runtime's grant and convergence
path (`desktop/runtime/`), and the Android source
(`android/app/src/main/java/.../notifications/`) end to end from
`NotificationListenerService` callback to wire and back.

**Not done, deliberately:** no feature added, no refactor, no protocol change,
no post-v1 work. The only files this wave writes are this report and the two
documentation corrections named in §32.

---

## 4. N0–N5 history

| Wave | Subject | Verdict | Certified |
| --- | --- | --- | --- |
| N0 | Schema, portable contract, ADR-0016, ADR-0017 | PASS | `26952da` |
| N1 | Android source, listener, identity derivation, filter | PASS | — |
| N2 | Linux sink, `NotificationSink` seam, D-Bus, lock source | PASS | — |
| N3 | Grants, per-app filter, consent UI both ends, CLI | PASS | `98e8b70` |
| N4 | Dismissal synchronisation and echo suppression | PASS | `b7531d0` |
| N5 | Mid-session grant convergence, grace, bounds, soak, fixture | PASS | `e72bb09` |

N6 re-verified each wave's load-bearing claims rather than trusting the reports;
where a claim is reproduced with N6's own evidence it is marked **N6**, and
where the certified historical evidence stands it is marked **N1–N5** and the
distinction is never blurred.

---

## 5. Final requirements matrix

Every major requirement from N0–N5. **Impl** cites the code that enforces it,
**Auto** the test that fails if it stops being true, **HW** the hardware
evidence and which wave produced it.

| # | Requirement | Origin | Implementation evidence | Automated test evidence | Hardware evidence | Status | Debt / limitation |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Identity model: a message is from the pinned TLS fingerprint, never from a claim | ADR-0016 §7 | `NotificationManager.peers: HashMap<Fingerprint, PeerSlot>`; `origin_device_id` only ever copied into an outbound `DismissRequest` | `two_peers_sending_the_same_identity_do_not_collide`, `an_empty_snapshot_from_one_peer_closes_only_its_own_mirrors` | N6 — two live sessions keyed by `3B38 1925 F8A6 E49D` | **PASS** | — |
| 2 | Roles announced, never assumed | ADR-0017 §1 | `PeerRoles::default()` empty; `apply_upsert` step 2 | `sink.rs`, `hardening.rs` role suite | N6 — `roles=0 epoch=1` then `roles=2 epoch=2` observed live | **PASS** | — |
| 3 | Role epochs strictly monotonic; a stale announcement can never re-widen | ADR-0017 §4 | `PeerRoles::apply` refuses `epoch <= self.epoch` and `epoch == 0` | `the_epoch_table_holds_in_order`, `epoch_zero_is_refused_even_as_the_first_announcement` | N6 — epoch 1 → 2 within one connection, restart at 1 across one | **PASS** | — |
| 4 | Source/sink asymmetry; Linux never acts on an inbound dismiss | ADR-0017 §1 | `handle_control` Dismiss arm answers `RejectedRole` | `dismiss.rs` | N1–N5 | **PASS** | — |
| 5 | HMAC notification identity, 16 bytes, derived not random | ADR-0016 §1–§3 | `NotificationSecret` (Keystore `HmacSHA256`, non-exportable); `NotificationIdentity` | `NotificationIdentityTest`, `notifications_protocol.rs` | N1–N5 | **PASS** | — |
| 6 | Raw Android key never leaves the source | ADR-0016 §1 | `SourceIdMap` in-memory; key referenced only inside `handleDismiss` | `no_failure_path_renders_notification_content_or_a_platform_key` | N6 — `0\|io.github` absent from both logs and both stores | **PASS** | — |
| 7 | Content is never identity | ADR-0016 §11 | `content_hash` used only for de-dup; mirror keyed on `NotificationId` | `notification_upsert_has_exactly_the_approved_fields` | N6 — update in place kept `1 of 1` | **PASS** | one-sided by design |
| 8 | Upsert is idempotent; posted and updated are one message | schema | `apply_upsert` replaces in place via `replaces` | `five_hundred_updates_to_one_identity_stay_one_mirror` | **N6** — update kept one mirror | **PASS** | — |
| 9 | Remove carries no reason | schema | `NotificationRemove{ notification_id, origin_device_id }` | `the_small_messages_have_exactly_the_approved_fields` | N1–N5 | **PASS** | — |
| 10 | Snapshot is active-state resync, never history | ADR-0015 §1.8 | `Snapshot` holds `BTreeSet<NotificationId>` and nothing else | `a_complete_snapshot_removes_what_it_did_not_name` + 6 failure cases | **N6** — `snapshot complete named=3 closed=0` after a real Wi-Fi outage | **PASS** | — |
| 11 | Grant model: per peer, never auto-granted | ADR-0015 §4 | `auto_grant = ["battery.v1"]`; Android subtracts `NotificationsCapability.ID` at pairing | `renegotiate.rs`, `daemon/tests/notifications.rs` | **N6** — `pair grants=battery.v1,files.v1` on real hardware | **PASS** | — |
| 12 | Deny-by-default app filter | ADR-0015 §5 | `NotificationPolicy.allowedApps = emptySet()` | `NotificationFilterTest` | N6 — only the named fixture mirrored | **PASS** | — |
| 13 | Work profile off by default | ADR-0015 §8 | `includeWorkProfile = false` | `NotificationFilterTest` | N1–N5 | **PASS** | detection debt (§33 D) |
| 14 | Ongoing off by default | ADR-0015 | `includeOngoing = false` | `NotificationFilterTest` | **N6** — enabled deliberately, then mirrored | **PASS** | — |
| 15 | Source lock privacy, reduced before encoding | ADR-0015 §7 | `NotificationSource` reduces before `NotificationWire.encode` | `a locked phone transmits the app label and no content` | N1–N5 | **PASS** | — |
| 16 | Sink lock privacy, reduced before D-Bus | ADR-0015 §7 | `Presentation::resolve` → `build_mirror` | `a_locked_session_shows_the_app_name_and_no_body` | N1–N5 | **PASS** | — |
| 17 | Unknown lock state = locked | ADR-0015 §7 | `UnknownLock::is_locked → true`; `read_locked_hint` returns `true` on every error | `an_unknown_lock_state_is_locked_whichever_way_it_was_reached` | **N6** — `real_lock` agrees with `loginctl` live | **PASS** | one session (§33 C) |
| 18 | OS permission and AnyFlow grant are separate and both required | ADR-0015 §1.4, ADR-0017 §6 | `NotificationAccess.isGranted` vs `policyFor(peer)`, three call sites | `NotificationReadinessTest` | **N6** — listener allowed while grant absent mirrored nothing | **PASS** | — |
| 19 | Desktop receive policy per peer | ADR-0015 §4 | `DaemonState::policy_for` → `trusted_peer().allows()` | `daemon/tests/notifications.rs` | **N6** — `anyflow notifications mirror/when-locked/dismiss-sync` | **PASS** | — |
| 20 | Android dismiss policy, default off | ADR-0015 §6 | `allowDismissSync = false`; `NotificationDismissRules.screen` | `NotificationDismissRulesTest` | N6 — trust store read back `allowDismissSync` | **PASS** | — |
| 21 | Desktop dismiss policy, default off | ADR-0015 §6 | `may_sync_dismissals`; `maybe_request_dismissal` | `dismiss.rs` 2×2 matrix | **N6** — still `off` immediately after the grant | **PASS** | — |
| 22 | Only a human dismissal propagates (reason 2 only) | ADR-0015 §6 | `CloseReason::is_human_dismissal` — one `matches!` on one variant | `only reason two is a human dismissal`; N6 soak: 0 dismiss requests in 2652 cycles | N5 (person required) | **PASS** | needs a person (§33 D) |
| 23 | Clearability re-checked live at the source | ADR-0015 §6 | `decideClearable(activeNotification(key))` | `NotificationDismissRulesTest` | N5 — `1 sent, 1 declined` for an ongoing | **PASS** | — |
| 24 | Echo suppression | N4 | `NotificationEcho`, armed before the cancel | `NotificationEchoTest` | N1–N5 | **PASS** | — |
| 25 | A mid-session grant converges with no manual reconnect | N5 / ADR-0017 §3 | `renegotiate::decide` + `do_grant` | `renegotiate.rs` (10), `daemon/tests/notifications.rs` (9) | **N6 — 6.18 s on hardware** | **PASS** | — |
| 26 | Reconnect grace bounded: > 0 and finite | ADR-0015, NOTIFICATIONS.md | `limits::RECONNECT_GRACE = 60 s` | `the_shipped_grace_is_greater_than_zero_and_finite` + 11 cases | **N6** — mirrors survived a 20 s outage | **PASS** | — |
| 27 | A replaced session's detach cannot clear the live one | N5 | `superseded_sessions` counter, spent not permanent | `a_replaced_sessions_detach_cannot_close_the_live_sessions_mirrors`, `a_genuine_disconnect_after_a_displacement_still_clears_the_screen` | **N6** — `replaced by a newer session` with **no** grace expiry 75 s later | **PASS** | — |
| 28 | Role state is per connection | ADR-0017 §4 / N5 | `detach_session` resets `local_roles` and `peer_roles` | `a_peer_with_no_session_claims_no_roles` | N6 | **PASS** | — |
| 29 | Queue bounds: 256 per peer, terminal removes never silently dropped | N5 | `limits::MAX_QUEUED_WORK`, `QueueStats.high_water` | `removals_under_pressure_converge_and_any_loss_is_counted` | **N6 soak — peak queue 4, dropped_terminal 0** | **PASS** | — |
| 30 | Mirror bound: exactly 200 per peer | N5 | `limits::MAX_MIRRORS_PER_PEER` | `five_hundred_notifications_never_exceed_the_mirror_ceiling`, `the_two_hundred_and_first_mirror_evicts_exactly_one` | N6 soak — peak 8, 0 evictions | **PASS** | — |
| 31 | Multi-peer isolation | N2–N5 | per-peer slot, queue, mirror table, counters | 11 isolation tests (§19) | N1–N5 | **PASS** | — |
| 32 | Failure injection converges safely on both ends | N5 | `MemorySink` seams; `BreakableListener` | 13 Android + 10 desktop injections | N1–N5 | **PASS** | — |
| 33 | Logging privacy at every level including TRACE | ADR-0015 §1.15 | `redact.rs`; every Android log site is a class, enum, count or id prefix | `logging.rs` (11), `NotificationLoggingCanaryTest` (13) | **N6 — both live captures clean** | **PASS** | — |
| 34 | Persistence privacy: no content, no history, either side | ADR-0015 §1.8/§1.14 | no schema field on either store | `no_notification_content_is_written_to_disk` | **N6 — full key inventory of both stores, §21** | **PASS** | — |
| 35 | Accessibility: no N3 regression | N3 | per-page render, stable widget identity | `the_notifications_page_widget_tree` (+6 re-render cases) | **N6 — live Android semantics dump, §23** | **PASS** | desktop l10n (§33 B) |
| 36 | Android lifecycle: bound only while a granted peer is connected | ADR-0015 §3 | `requestRebind` / `requestUnbind` driven by live peer state | `NotificationSourceTest` | **N6 — bind at the grant, unbind on outage, rebind on restore, in logcat** | **PASS** | `requestRebind` stickiness (§33 D) |
| 37 | Desktop backend lifecycle: loss narrows roles, recovery re-widens with a newer epoch | N5 | `set_available`, `spawn_platform_pumps` | `losing_and_regaining_the_server_narrows_then_rewidens_with_a_new_epoch` | deterministic (§33 D) | **PASS** | GNOME cannot restart on Wayland |
| 38 | Protocol immutability | N0 | — | `notifications_schema.rs` (17 descriptor tests) | — | **PASS** | `protocol/` byte-identical since `26952da` |
| 39 | No actions, replies, `PendingIntent`, history | ADR-0015 §1.11–13, §11 | absence of a field, not a check | `the_dismiss_primitive_has_no_remote_execution_path`, `the_control_envelope_has_no_escape_hatch` | **N6 source search, §32** | **PASS** | — |

---

## 6. Architecture

The shape N0 designed is the shape that shipped. One channel — the existing
authenticated control session — carrying a `NotificationControl` of at most
8 KiB inside `CapabilityMessage.payload`. No second socket, no data stream, no
image, no icon.

The ownership boundaries in `docs/architecture/NOTIFICATIONS.md` were checked
against code rather than read:

| Concern | Owner in code | Verified at |
| --- | --- | --- |
| Which notifications leave the phone | `NotificationFilter.screen` then `.decideForPeer` | source only; the sink has no request path |
| What a notification is called | `NotificationIdentity` (source) + `contract::validate_*` (schema) | sink treats the id as opaque `NotificationId` |
| Who a message is from | `Fingerprint`, the key of `NotificationManager.peers` | `origin_device_id` is never read for authorization |
| Whether a peer may use the capability | `DaemonState::policy_for` → `trusted_peer().allows()`, re-asked per message | `handle_control` **and** `apply_upsert` ask separately |
| What a peer can physically do | `PeerRoles`, per connection, reset on detach | never consulted as authorization |
| Whether a notification can be dismissed at all | `NotificationDismissRules.decideClearable`, re-read live | sink may only ask |
| Timers | `limits.rs`, sink-local | `no_wire_message_carries_a_timing_value` |

**Ordering.** The snapshot design depends on in-order delivery, and the three
points that provide it are unchanged: TLS over TCP, one reader awaiting
`on_message` before taking the next, one writer draining a single FIFO. The
constraint that places on the capability — a **single ordered producer** — is
met on both ends: the desktop by one worker task per `PeerSlot`, the Android
side by one ordered event loop. Both are still pinned by
`inbound_capability_messages_reach_the_handler_in_wire_order` and
`replies_keep_their_order_under_queue_pressure`, green in N6's run.

---

## 7. Protocol

### Immutability — **PASS**

```console
$ git log --format=%H --diff-filter=A -1 -- protocol/proto/anyflow/v1/capabilities/notifications_v1.proto
26952da6568b35bcf927edcfc08b600d52748360        # the N0 commit that added it
$ git diff --stat 26952da HEAD -- protocol/
(empty)
```

**`protocol/` has not changed by a single byte since the commit that introduced
the notifications schema.** Not in N1, N2, N3, N4 or N5. This is stronger than
the `git diff -- protocol/` the brief asks for, which only compares against the
working tree.

### Contract audit against the implementation

| Message | Field | Rule | Enforced at |
| --- | --- | --- | --- |
| `NotificationUpsert` | `notification_id` | **exactly** 16 bytes | `NotificationId::from_bytes` — `try_into` on `[u8; 16]`, no truncation path |
| | `origin_device_id` | 32 lowercase hex | `check_device_id`; uppercase **refused**, not folded |
| | `app_id` / `app_label` | ≤ 255 / ≤ 128 bytes, no NUL | `check_text` |
| | `title` / `body` | ≤ 512 / ≤ 4096 bytes, no NUL | `check_text` |
| | `group_id` | absent **or** exactly 8 | `check_optional_digest` |
| | `content_hash` | absent **or** exactly 32 | `check_optional_digest` |
| `NotificationRemove` | — | id + origin only; **no reason code** | schema shape |
| `DismissRequest` | — | id + origin only | schema shape |
| `NotificationResult` | `notification_id` | exactly 16 | `NotificationId::from_bytes` |
| `SyncMarker` | `sync_id` | exactly 16 | `validate_sync_marker` |
| `NotificationRoles` | `epoch` | ≠ 0, strictly increasing | `validate_roles`, `PeerRoles::apply` |
| whole message | — | ≤ 8 KiB, checked **first** | `validate_control` |

The Android side agrees constant for constant (`NotificationLimits`:
16/16/8/32/32, 255/128/512/4096, 8 KiB), and `isDeviceId` applies the same
lowercase-hex rule with the same refusal-not-repair discipline. Both toolchains
compile the same `.proto` and assert the same canonical encoded vector byte for
byte.

**Roles remain claims.** There is no code path from a received
`NotificationRoles` to the trust store on either end; `PeerRoles` exposes
`has`, `epoch`, `is_empty` and `iter`, and no mutator a peer could reach.

**`Upsert` is idempotent**; **`Remove` has no transmitted reason**;
**`DismissRequest` carries no action, intent, payload, free text or reply** —
and cannot, because it has two fields. **No field can carry a `PendingIntent`,
`RemoteInput`, action index, reply or bundle**: `notifications_schema.rs` has
17 descriptor-level tests that fail if a field is added, if a field name hints
at a prohibited capability, if any `bytes` field appears that is not one of the
four fixed-width identifiers, if the control envelope grows an escape hatch, or
if a field number is retired or reused.

**The snapshot is an active-state resync, not a history**: `Snapshot` holds a
`BTreeSet<NotificationId>` and a `sync_id`, and has no field that could hold
text, a timestamp or a digest.

---

## 8. Identity

**`notification_id = HMAC-SHA256(secret, "anyflow/notifications.v1/id/v1" ||
len32(key) || key)[0..16]`**, derived at the source.

* **The secret lives in the Android Keystore as a non-exportable `HmacSHA256`
  key.** That is stronger than ADR-0016 §4 asks for, and stronger in a way that
  matters: the 32 bytes never exist in the app's heap, so there is nothing for
  a log, a crash dump or a backup to leak. There is no accessor that returns
  them.
* **`NotificationSecret` refines ADR-0016 §5 and documents the refinement.**
  ADR-0016 says a *missing or unreadable* secret is regenerated. The
  implementation distinguishes the two: **absent** → regenerated
  (`REGENERATED`, logged at warn as a reason code with a generation counter);
  **present but unreadable** → *left alone*, capability inert, `UNAVAILABLE`,
  no `SOURCE` role announced. Replacing a live secret on one failed read would
  invalidate every mirror for no reason. The fail-forward property ADR-0016
  wanted still holds for the case it was about. **N6 accepts this as a
  strengthening, not a deviation**, because it is documented at the type and
  pinned by `NotificationSecretTest`.
* **N6 hardware evidence:** `AnyFlowApp: notification secret LOADED
  generation=0` — after a reinstall the secret was loaded, **not** silently
  regenerated, and the `notificationSecretGeneration` counter in the trust
  store is the thing that would say otherwise.
* **The map is memory-only.** `SourceIdMap` is a private field of
  `NotificationSource` bounded at `MAX_TRACKED_NOTIFICATIONS = 512`, rebuilt
  from `getActiveNotifications()` on `onListenerConnected`, and never
  serialised — there is no write path to the trust store, which N6 confirmed by
  reading back every key the store holds (§21).

**The mirror namespace is `(authenticated peer fingerprint, notification_id)`.**
`NotificationManager.peers` is keyed by `Fingerprint`, each `PeerSlot` owns its
own `MirrorTable`, and `origin_device_id` is stored on a mirror entry for one
purpose only: to be copied back into a `DismissRequest`. A peer that lies in it
addresses nothing but its own mirrors.

---

## 9. Roles and epochs — **PASS**

`PeerRoles::apply` is eleven lines and every refusal is in them:

```rust
if announcement.epoch == 0        { return Err(UnsetEpoch) }
if announcement.epoch <= self.epoch { return Err(StaleEpoch { .. }) }
self.roles = announcement.roles.iter().filter_map(|r| Role::from_wire(*r)).collect();
self.epoch = announcement.epoch;
```

`<=`, not `<`: **equal is refused too**, so a duplicate of the current epoch
carrying a different set cannot take effect. `Role::from_wire` maps unknown and
`UNSPECIFIED` to `None`, so an unknown role is **dropped without discarding the
set**.

The adversarial table (`the_epoch_table_holds_in_order`), re-run in N6:

| roles in | epoch | source after | dismiss-target after | epoch after |
| --- | --- | --- | --- | --- |
| `{SOURCE}` | 0 | no | no | 0 |
| `{SOURCE}` | 1 | yes | no | 1 |
| `{SOURCE, DT}` | 1 | yes | no | 1 |
| `{SOURCE, DT}` | 2 | yes | yes | 2 |
| `{}` | 3 | no | no | 3 |
| `{SOURCE, DT}` | 2 | **no** | **no** | 3 |
| `{SOURCE}` | 3 | no | no | 3 |
| `{SOURCE}` | 4 | yes | no | 4 |
| `{SOURCE, DT}` | `u32::MAX` | yes | yes | MAX |
| `{}` | `u32::MAX` | yes | yes | MAX |

> **The critical acceptance invariant — a stale role announcement can NEVER
> restore authority a newer announcement removed — holds**, at row 6 and at the
> `u32::MAX` ceiling, where a further announcement is refused rather than
> accepted stale.

Plus `epoch_zero_is_refused_even_as_the_first_announcement`,
`an_unknown_role_is_ignored_without_discarding_the_set`, and
`re_widening_a_role_restores_no_content` — a widening says what the peer can do
now, not what it once sent.

**Live in N6**, on the certification hardware, within one connection:

```
announcing roles peer=3B38 1925 F8A6 E49D roles=2 epoch=1   (desktop: SINK + DISMISS_REPORTER)
peer roles       peer=3B38 1925 F8A6 E49D roles=0 epoch=1   (phone: listener not yet bound)
peer roles       peer=3B38 1925 F8A6 E49D roles=2 epoch=2   (phone: SOURCE + DISMISS_TARGET)
```

and, across the Wi-Fi outage, both sides restarting at epoch 1 — a **correct
reset**, which a naive global-monotonicity assertion would have called a
failure.

### Role lifetime — **PASS**

`detach_session` resets `local_roles` and `peer_roles`. The gap N5 closed was
the session rebuilt *without* `notifications.v1` negotiated, which never calls
`attach_session`, so the previous session's roles survived it. N6 re-ran
`a_peer_with_no_session_claims_no_roles`: **green**. Nothing could ever have
flowed — the grant is re-checked per message and roles are never an
authorization input — what it cost was the truth of the one screen a person
reads when working out why their notifications stopped.

---

## 10. Consent model — **PASS**

Three permissions, three holders, three revocation surfaces, and **N6 observed
all three states apart on hardware**:

| Gate | Held by | N6 observation |
| --- | --- | --- |
| Android notification access | the OS | granted via `cmd notification allow_listener`; **nothing was sent to anyone** until the peer grant existed |
| The peer grant | the local trust store, per peer | `pair grants=battery.v1,files.v1` — pairing did **not** grant `notifications.v1` |
| The announced roles | each peer, about itself | phone announced `roles=0` while access was held but the listener unbound |

**Pairing does not grant notifications.** Verified in code on both ends —
desktop `auto_grant = ["battery.v1"]`, Android
`connection.negotiatedCapabilities.toSet() - ClipboardCapability.ID -
NotificationsCapability.ID` — and **verified live**: the harness asserts it, and
N6's real pairing printed `pair grants=battery.v1,files.v1`.

**Enabling mirroring does not enable dismiss sync.** N6 checked this at the
exact moment it would be easiest to get wrong — immediately after the grant
converged:

```console
$ anyflow notifications status
      notifications.v1 granted
      mirror on    when locked app-only   dismiss-sync off      ← still off
```

Turning it on took a second, separate, deliberate command. This is ADR-0015 §6
holding on real hardware, not in a test.

**Readiness is two types, not one enum.** `NotificationReadiness` (8 states,
each with a different fix) and a separate dismiss-readiness type, because
mirroring can be `READY` while dismissal sync is unavailable — the state N2
measured. Collapsing them would force the UI to lie about whichever half was
worse. N6 confirms both types still exist and neither has absorbed the other.

**Desktop consent surface**, exercised in N6:
`anyflow notifications status | mirror | when-locked | dismiss-sync`, plus the
GTK page whose widget tree is gated by test. The status output names, per peer:
the grant, the mirror switch, the lock policy, the dismiss switch, the mirror
count, **both sides' roles with their epochs**, and the dismissal readiness as
a separate line — never collapsed into one "ready".

---

## 11. Android source — **PASS**

Traced end to end, callback to wire.

**The callback thread does almost nothing.** `onNotificationPosted` drops
AnyFlow's own package **first, before extraction**, copies a handful of fields
into a plain `PlatformNotification` and returns. No protobuf, no HMAC, no
package-manager lookup, no I/O, no peer enumeration. Every `extras` read is
`runCatching`-wrapped, because `extras` is a `Bundle` an arbitrary application
filled in and a hostile one must produce a dropped notification, never an
exception on the phone's main thread. A failure returns `null`, which drops —
fail closed.

**The filter order is the specification**, and it is the order in the file:

1. **own package** — a hard rule with no policy field that could enable it;
2. **`VISIBILITY_SECRET`** — unconditional, at the source;
3. **`IMPORTANCE_NONE`** — a notification the phone does not show its owner
   must not become a desktop banner;
4. the peer's grant and mirroring switch;
5. work-profile and ongoing switches, each independent of the app list;
6. the **per-app allow-list, deny by default**;
7. the lock policy, when it says suppress.

Rules 1–3 are peer-independent and run once, so a notification that must never
leave the device is dropped once rather than once per peer.

**Defaults, read from the code:** `allowedApps = emptySet()`,
`includeWorkProfile = false`, `includeOngoing = false`,
`whenSourceLocked = APP_ONLY`, `allowDismissSync = false`, and
`NotificationPolicy.DENIED` — what an unknown, forgotten or ungranted peer
resolves to — is every switch off with `whenSourceLocked = SUPPRESS`.

**No heuristic anywhere.** No OTP regex, no keyword list, no app-category
guess. System packages are denied for the same reason every other package is:
the allow-list starts empty. AnyFlow does not classify a package as "system"
and then trust the classification.

**Lock reduction happens before encoding**, so withheld content never exists on
the wire. **Unknown lock = locked**, and `NotificationAccess.isGranted` reads
the platform on every question with `.getOrDefault(false)`.

**The raw key never leaves.** `SourceIdMap` is in-memory, bounded at 512,
rebuilt on connect. The platform key is passed to `cancel()` and referenced
nowhere else — not logged, not persisted, not put in an answer, not counted.

**Callbacks do not block and the single ordered producer is intact**: one event
loop owns everything outbound, and `NotificationQueue` coalesces per identity
into a **slot** rather than a queue, which is what keeps a removal from
overtaking the upsert it refers to.

### Every content-bearing log site on the Android notification path

All 22 were enumerated and read. Every one carries a **class name, an enum, a
count, or `NotificationRedact.idPrefix`** — 8 hex characters of an
already-opaque HMAC. Representative:

```kotlin
Log.w(TAG, "could not read a notification: ${e.javaClass.simpleName}")   // type only
Log.w(TAG, "cancelNotification refused: ${e.javaClass.simpleName}")      // type only
Log.d(TAG, "not mirrored: $reason")                                      // DropReason enum
Log.i(TAG, "snapshot sent: $sent of ${active.size} active")              // counts
Log.i(TAG, "dismiss refused: ${verdict.reason} for ${idPrefix(...)}")    // enum + prefix
```

The discipline is deliberate and stated at each site: *"a platform exception
message from the notification manager can name the notification"*. **No
`println`, no `printStackTrace`, and no `Throwable.message` interpolation
anywhere on the notification path.** N6 confirms zero hits for all three.

---

## 12. Linux sink — **PASS**

Traced from `handle_control` to `org.freedesktop.Notifications.Notify`.

**The gates, in the order they run**, and the fact that two of them run twice
is the point:

```
decode → validate_control (ceiling FIRST, then every field bound)
       → slot(peer)                      ← keyed on the PINNED fingerprint
       → policy_for(peer)                ← the grant, asked fresh  [1st]
       → per-body dispatch, enqueue
                 ↓  the peer's own worker
       → policy_for(peer)                ← the grant, asked fresh  [2nd]
       → peer_roles.has(Role::Source)    ← the peer's own claim
       → privacy_or_default == Secret ?  ← close the mirror, refuse
       → lock.is_locked()                ← read from the platform, NOT cached
       → Presentation::resolve           ← Suppress closes what is on screen
       → build_mirror                    ← withheld content is ABSENT, not flagged
       → sink.display(...)
```

> *"`handle_control` asked before queueing; this asks before displaying, and the
> two are different moments — a revocation in between must win."*

**Server ids are memory-only.** `ServerId` lives in `MirrorTable` inside
`PeerState`, and nothing writes it anywhere. **The mirror table is bounded** at
200 per peer, per peer rather than globally, so a flooding peer cannot evict
another peer's notifications.

**Backend loss invalidates server ids**, and recovery re-announces with a
strictly greater epoch — `losing_and_regaining_the_server_narrows_then_rewidens_with_a_new_epoch`.

**`CloseNotification` cleanup cannot become a remote dismiss.** The only path
from a close to a `DismissRequest` runs through
`closed.reason.is_human_dismissal()`, which is
`matches!(self, Self::Dismissed)` — deliberately a single `matches!` on a
single variant rather than a list of exclusions, because *"a rule written as
'everything except expiry' grows a hole the day a fifth reason is added, and
the hole would clear somebody's phone."* A stale server id finds no mirror and
produces nothing.

**No content is persisted, and there is no history.** `build_mirror` produces
what the backend will display and drops the rest; nothing on the write path
touches the trust store.

### Desktop log audit

`redact.rs` is the single funnel and its rule has **no debug override**: title,
body, app label and app id are never rendered, *"not at trace level, not behind
a feature flag, not in a panic message."* N6 grepped every `tracing::` site in
the capability for `title|body|text|app_id|app_label|platform_key`: **zero
hits**. What is emitted instead is `redact::id_prefix` (8 hex chars),
`peer.to_display_short()`, an outcome enum, a reason class and counts. A
`zbus::Error` is rendered by `class()` as one of a fixed set of words, because
*"a D-Bus error message is written by the other end and is not this process's to
forward into a log."*

---

## 13. App filtering — **PASS**

Deny by default, and the deny is an **empty set**, not a rule that could be
inverted. `policy.allowsApp(pkg)` is `allowMirror && packageName in
allowedApps`.

N6 hardware: with `allowedApps = ["io.github.yurisismotto.anyflow.fixture"]`,
the fixture mirrored and nothing else on a device holding 30+ active
notifications did. AnyFlow's own package is dropped by a hard rule at two
points — the listener callback and `NotificationFilter.screen` — with no
setting that reaches either.

The trust store records the filter as **configuration**, which is what it is:

```json
"notificationPolicy": {
  "allowMirror": true, "allowedApps": ["io.github.yurisismotto.anyflow.fixture"],
  "allowDismissSync": true, "includeOngoing": true, "includeWorkProfile": false,
  "knownApps": [], "whenSourceLocked": "APP_ONLY"
}
```

A package name a person chose in a picker. No notification, no id, no content.

---

## 14. Lock / privacy — **PASS**

| | `FULL` | `APP_ONLY` | `SUPPRESS` | unknown |
| --- | --- | --- | --- | --- |
| **Android source** | full content encoded | app label only, **reduced before encoding** | dropped entirely, `DropReason.LOCK_SUPPRESSED` | treated as locked |
| **Linux sink** | full | app name, body absent from the `Mirror` | mirror closed and none created | `UnknownLock::is_locked → true` |

**The reduction happens before the content crosses the relevant boundary** on
both ends. On the source that boundary is the encoder, so withheld content
never exists on the wire. On the sink it is `build_mirror`, and withheld content
is **absent from the result rather than present-and-flagged** — *"there is no
downstream step that could forget to apply a flag, because there is no flag."*

**Fail-closed is by type, not by check.** `UnknownLock::is_locked()` returns
`true`; `LogindLock::read_locked_hint()` returns `true` on a proxy failure and
`true` on a property-read failure, logging the error *class*. The lock is read
from the platform **per notification** rather than from a cached answer.
`an_unknown_lock_state_is_locked_whichever_way_it_was_reached` covers
unlocked→unknown, locked→unknown, and an unknown source that claims "unlocked"
underneath.

**Unlock does not replay.** There is nothing to replay:
`unlocking_does_not_restore_content_the_sink_never_kept` and
`a_dismissal_does_not_restore_content_that_was_reduced` both hold, and
`mirror.rs` records that restoring on unlock *"was considered and rejected: it
would be a notification history."*

**N6 live:** `real_lock` agreed with `loginctl` on the running session, and the
daemon read `LockedHint` from
`/org/freedesktop/login1/session/_32` at startup.

---

## 15. Resync / reconnect — **PASS**

### Deterministic (N6 run)

| Case | Test | Result |
| --- | --- | --- |
| short disconnect, within grace | `a_reconnect_inside_the_grace_keeps_every_mirror` | mirrors kept, same objects |
| beyond grace | `a_disconnect_longer_than_the_grace_closes_the_mirrors` | both closed, once each |
| peer never returns | `a_peer_that_never_returns_leaves_no_mirrors_and_no_content` | 0 mirrors, report content-free |
| old timer vs new session | `an_expired_grace_from_an_old_session_cannot_close_the_new_ones_mirrors` | generation check holds |
| incomplete snapshot | `an_incomplete_snapshot_is_abandoned_and_removes_nothing` | removes nothing |
| `END` without `BEGIN` | `an_end_with_no_begin_removes_nothing` | refused |
| mismatched `sync_id` | `an_end_for_a_different_snapshot_removes_nothing…` | refused, snapshot survives |
| complete snapshot | `a_complete_snapshot_removes_what_it_did_not_name` | converges |
| stale server ids | `a_close_for_a_stale_server_id_produces_no_dismiss_request` | no dismiss |
| server restart | `a_snapshot_after_a_restart_converges_on_the_new_server` | converges |
| `BEGIN` left open by a departed peer | `a_snapshot_left_open_by_a_departed_peer_is_still_cleared_by_the_grace` | no ghost |

### Hardware (N6's own run)

Three fixture notifications live, plus Android's own auto-group summary — **four
`StatusBarNotification`s, three mirrors** after the ongoing/clearable set
settled. A real Wi-Fi interruption:

```console
$ adb shell svc wifi disable        # 20 s, inside the 60 s grace
      showing 3 of 3 mirrored        ← nothing was cleared
$ adb shell svc wifi enable
      showing 3 of 3 mirrored        ← nothing was duplicated
```

The daemon, across the reconnect:

```
session established   … capabilities=["battery.v1", "notifications.v1"]
replaced by a newer session; closing the old one   session=3
announcing roles      roles=2 epoch=1
peer roles            roles=0 epoch=1
peer roles            roles=2 epoch=2
snapshot complete     named=3 closed=0
```

**No duplicates, no ghosts, no lost terminal removals, no stale-server-id
dismissal** — and the reconciliation named exactly what the tablet held.

### The N5 replaced-session defect: reproduced, and confirmed fixed

The `replaced by a newer session` line above is the **exact condition** that
produced N5's worst defect: the displaced session's `on_peer_disconnected`
arriving after the live session had attached, clearing the live session's
outbound sender and arming a grace timer carrying the *new* generation — which
closed four visible mirrors sixty seconds later on a healthy session.

N6 waited **75 seconds past the replacement**, past the whole 60 s grace
window:

```console
=== any grace expiry or mass close? ===
  NONE — the live session's mirrors were not touched
      showing 3 of 3 mirrored
```

The fix — `superseded_sessions`, a counter that is **spent** on the next detach
rather than a permanent exemption — is pinned in both directions by
`a_replaced_sessions_detach_cannot_close_the_live_sessions_mirrors` and
`a_genuine_disconnect_after_a_displacement_still_clears_the_screen`, both green
in N6's run. **This is N6's own hardware evidence, not N5's.**

---

## 16. Dismiss synchronization — **PASS**

The whole chain, traced in code:

```
  a human closes the mirror on GNOME
        │  NotificationClosed(server_id, reason)
        ▼
  note_closed → mirrors.forget_server_id(id)          ← a LIVE mirror lookup
        │       reason.is_human_dismissal()           ← REASON 2 ONLY
        ▼
  maybe_request_dismissal
        │  policy_for(peer).may_sync_dismissals()     ← desktop policy, asked NOW
        │  peer_roles.has(DISMISS_TARGET)             ← the peer will act
        │  local_roles.announced(DISMISS_REPORTER)    ← we said we report
        ▼
  queued on the peer's own worker → send_dismiss → DismissRequest{id, origin}
        │
        ▼  ─────────────── the phone ───────────────
  NotificationDismissRules.screen
        │  16-byte id            → else refuse, DO NOT ANSWER
        │  32-lc-hex origin      → else INVALID
        │  notifications.v1 grant→ else NOT_AUTHORIZED
        │  canDismiss (listener + access + secret) → else REJECTED_ROLE
        │  allowMirror && allowDismissSync → else REJECTED_POLICY
        │  origin == this device → else INVALID
        ▼  only now:
  idMap.platformKey(id)              → null ⇒ UNKNOWN_NOTIFICATION
  activeNotification(platformKey)    → live re-read, not the stale flag
  decideClearable                    → !clearable || ongoing ⇒ NOT_DISMISSIBLE
  echo.arm(...)                      → BEFORE the cancel, never after
  cancelNotification(mappedKey)      ← the one call
```

**There is exactly one production `cancelNotification` call site in the whole
product**: `AnyFlowNotificationListener.kt:92`, inside the `ListenerControl.cancel`
seam, reachable only through `NotificationDismissRules`. N6 re-verified by
search: every other occurrence of the word in production source is a doc
comment.

**Four independent yeses, none derived from another**, and the grant is asked
*now* rather than reused from the upsert that put the notification on screen.

**The message is not a lookup oracle.** The `SourceIdMap` is not consulted until
grant, role and policy have all passed, so a peer failing an earlier gate never
reaches the lookup; and a wrong guess is indistinguishable from a notification
already gone.

**Clearability is re-read live.** `decideClearable` consults the *current*
`PlatformNotification`, not the `dismissible` flag the desktop holds, and
refuses on `!clearable || ongoing` — **both**, not one implying the other,
*"because a rule that relies on one implying the other is a rule that breaks on
the release where it stops."*

**Absent from production, confirmed by N6 search:**
`cancelAllNotifications` · `snoozeNotification` · `RemoteInput` ·
`PendingIntent` remote execution · action invocation · reply · open-app ·
arbitrary package/id/tag cancellation. Every textual hit for these terms on the
notifications path is **prose asserting the absence**. The ten real
`PendingIntent` code sites are `ConnectionService`'s own foreground-service
notification, the clipboard quick-settings tile, and `clipboard.v1`'s own
"clip received" notification — all AnyFlow posting **its own** notifications,
all pre-existing, none reachable from any inbound `notifications.v1` message,
and all covered by the own-package drop so they never mirror either.

**Echo suppression** (`NotificationEcho`, 64 entries, 10 s TTL) is armed
*before* the cancel, because `onNotificationRemoved` can arrive on the main
thread while `cancelNotification` is still returning; and it is **released** if
the cancel fails, so a failed cancel cannot swallow a later genuine removal.

**What N6 did not re-run.** The end-to-end human dismissal needs a person to
click a GNOME banner: Mutter input injection is denied in this environment and
GNOME Shell exposes no notification banner to AT-SPI, so neither
`ANYFLOW_HUMAN_DISMISS` gate nor a hardware human-dismiss could be driven. That
evidence is **N5's, and is cited as N5's** (§10 of the N5 report: a clearable
one honoured at `2 sent, 1 declined`, an ongoing one refused at
`1 sent, 1 declined`, with two distinct derived-id prefixes distinguishing
them). N6 adds: `dismiss.rs` (33 tests) and `NotificationDismissRulesTest` are
green, and the N6 soak sent **0 `DismissRequest`s across 2652 cycles** in which
nobody dismissed anything — the negative half of the same property, measured
today.

---

## 17. Mid-session grant convergence — **PASS**

The mechanism was **not changed in N6**. It was re-audited and re-measured.

`renegotiate::decide` is a pure function of four values:

```rust
if !granted                                    { Withdrawn }          // narrowing never reconnects
else if session.is_none()                      { NoSession }          // nothing is frozen
else if session.negotiated.contains(capability){ AlreadyNegotiated }  // nothing to do
else if requested == Some(session.id)          { AlreadyRequested }   // the coalescing point
else                                           { Reconnect }
```

The bound is **one request per session** — no clock, nothing to tune. The
decision and the recording happen inside one lock, which is what makes the bound
hold when two control clients flip the same switch at once. The rule names **no
capability**: `files.v1` and `clipboard.v1` froze identically, and the
correction fixes the capability model rather than one instance of it.

### N6's own hardware measurement

A session deliberately established **before** any notifications grant:

```console
$ anyflow notifications status
    SM-X620 (3B38 1925 F8A6 E49D)
      notifications.v1 NOT granted
      roles: this desktop announced 0 (epoch 0); the device claims no source role (epoch 0)

# the daemon's own view of that session:
INFO session established … peer=3B38 1925 F8A6 E49D capabilities=["battery.v1"]
```

The single user action, and nothing else — **no manual disconnect at any
point**:

```console
$ anyflow grant a1ea1a3e6ef54a7fabdafd45e95e4d2d notifications.v1
granted notifications.v1 for 3B38 1925 F8A6 E49D (reconnecting the device so it takes effect now)

CONVERGED after 6.18 s — no manual disconnect

    SM-X620 (3B38 1925 F8A6 E49D)
      notifications.v1 granted
      roles: this desktop announced 2 (epoch 1); the device can source notifications (epoch 2)
      dismissal: this desktop reports human dismissals; the device will act on a dismiss request
```

The daemon log, which is the primary evidence:

```
INFO anyflow_runtime::state:    granted a capability this session cannot use; ending it so
                                the device reconnects and negotiates again
                                peer=3B38 1925 F8A6 E49D session=2 capability="notifications.v1"
INFO anyflow_runtime::listener: session established … capabilities=["battery.v1", "notifications.v1"]
INFO anyflow_capability_notifications: announcing roles … roles=2 epoch=1
INFO anyflow_capability_notifications: peer roles … roles=2 epoch=2
INFO anyflow_capability_notifications: snapshot complete … named=0 closed=0
```

**6.18 s** from the final user action to a usable notification session, on the
same hardware class N5 measured 7.35 s on. Both are the same mechanism; N6's is
N6's.

And immediately afterwards, in logcat, the other half of ADR-0015 §3:

```
NotificationListeners: Enabling component io.github.yurisismotto.anyflow/.notifications.AnyFlowNotificationListener
NotificationListeners: binding: Intent { act=android.service.notification.NotificationListenerService … }
AnyFlowApp: notification secret LOADED generation=0
NotificationListeners: 0 notification listener service connected
```

— then `Disabling component` when Wi-Fi dropped, and `Enabling` again when it
returned. **"AnyFlow reads your notifications only while a granted computer is
connected" is structurally true and was observed being true.**

### Deterministic coverage (N6 run)

`renegotiate.rs` — 10/10, the brief's §4 table row for row:

```
a_widening_past_the_session_requests_exactly_one_reconnect   ok   (A)
b_a_burst_of_writes_produces_one_reconnect                   ok   (B)
c_a_session_already_asked_is_not_asked_again                 ok   (C)
d_no_session_means_no_request_and_no_retry                   ok   (D)
e_withdrawing_a_grant_never_reconnects                       ok   (E)
f_rapid_toggling_is_bounded_by_the_reconnects_it_causes      ok   (F)
g_a_grant_the_session_already_negotiated_is_inert            ok   (G)
a_replacement_session_is_judged_on_its_own_capabilities      ok
requests_are_per_peer                                        ok
decision_names_are_distinct_and_carry_nothing                ok
```

plus `daemon/tests/notifications.rs` — **36 passed**. Cases D and G are
asserted as *decisions* rather than absences, so a future change that starts
dialling from the desktop fails a test rather than shipping.

**Revocation does not reconnect**, and N6 confirmed it on hardware: withdrawing
the grant produced `closed every mirror for a peer … closed=3 reason="revoked"`
and **no** "ending it so the device reconnects" line.

---

## 18. Bounds / backpressure — **PASS**

| Bound | Value | Where | N6 evidence |
| --- | --- | --- | --- |
| desktop mirrors per peer | **200** | `limits::MAX_MIRRORS_PER_PEER` | `five_hundred_notifications_never_exceed_the_mirror_ceiling`; the 201st evicts **exactly one** |
| desktop work queue per peer | **256** | `limits::MAX_QUEUED_WORK` | N6 soak **peak 4**, 0 evictions |
| Android event queue | **256** | `NotificationLimits.EVENT_QUEUE_CAPACITY` | `NotificationQueueTest` |
| Android `SourceIdMap` | **512** | `MAX_TRACKED_NOTIFICATIONS`, LinkedHashMap, `retainOnly` on snapshot | `NotificationIdentityTest` |
| echo suppression cache | **64**, 10 s TTL | `NotificationEcho.CAPACITY` / `TTL_MS` | `NotificationEchoTest` |
| snapshot entries | **100** | `MAX_SNAPSHOT_ENTRIES` | contract |
| backend call timeout | **5 s** | `limits::BACKEND_TIMEOUT` | a wedged server cannot stall the session |

**Terminal removes are not silently dropped.** The desktop queue evicts the
oldest **non-terminal** item first; a removal is dropped only when the queue is
full *of removals*, and when that happens it is **counted**
(`dropped_terminal`), **logged at warn** (*"a lost removal is a notification
that stays on a screen"*), and the peer is answered `RATE_LIMITED` naming the
identity. `removals_under_pressure_converge_and_any_loss_is_counted` drove 300
removals under pressure: **0 dropped**.

**Same-identity coalescing cannot swallow a terminal removal.** Coalescing is a
per-identity *slot* holding the latest upsert, and a removal is a different work
kind that does not coalesce into it — which is also what keeps a removal from
overtaking the upsert it refers to. `a burst keeps one identity and the removal
still arrives last` (N1) and `five_hundred_updates_to_one_identity_stay_one_mirror`
both hold.

**`QueueStats.high_water` exists because `pending` is a sample**, and sampling a
queue one task fills while another drains measures the scheduler rather than the
bound.

### N6's own soak — high-water marks measured today

Against the **real** GNOME Shell 50.4 notification server and the **real**
logind lock source:

```
soak finished after 302s, 2652 cycles
  peak mirrors        8
  peak queue depth    4          (bound 256)
  final mirrors       0
  coalesced           2652
  queue evictions     0
  dropped terminal    0
  mirror evictions    0
  role announcements  205
  results sent        2632
  dismiss requests    0
```

Every assertion held at **every cycle**, not only at the end: no phantom
dismiss, no unbounded growth, no reconnect storm (204 deliberate reconnects,
each producing exactly one role announcement), epochs correct in both directions
(strictly increasing within a connection, restarting at 1 across one), and a
final empty snapshot took everything off the screen.

> **This is N6's run, and it is shorter than N5's on purpose.** N5's certified
> 30-minute / 15819-cycle in-process soak and its 15-minute / 70-cycle hardware
> soak are **historical evidence and are not restated here as if N6 re-ran
> them**. N6 ran a 302-second regression of the same gate to confirm the
> property still holds against the current tree, and says so. See §28 for what
> a plain `--ignored` run of `real_dbus` does and does not prove.

---

## 19. Multi-peer isolation — **PASS**

Re-run in N6 (all inside the 703):

| Property | Test |
| --- | --- |
| the same `notification_id` from two peers does not collide | `two_peers_sending_the_same_identity_do_not_collide` |
| one peer's grant cannot authorize another | `an_ungranted_peer_displays_nothing` |
| one peer's policy never applies to another | `one_peers_policy_never_applies_to_another` |
| one peer's allow-list does not affect another | `an_app_allowed_for_one_computer_is_not_mirrored_to_another` |
| one peer's dismiss setting does not affect another | `enabling dismiss sync for one computer does not enable it for another` |
| revoking one leaves the other mirroring | `revoking_one_peer_leaves_the_other_mirroring` |
| reconnecting one leaves the other's mirrors alone | `reconnecting_one_peer_leaves_the_others_mirrors_alone` |
| role epochs are per peer | `a_role_epoch_is_scoped_to_one_peer` (A narrows at 9000, B stays at 1) |
| counters are per peer | `the_counters_are_per_peer` |
| an **empty** snapshot from one peer closes only its own mirrors | `an_empty_snapshot_from_one_peer_closes_only_its_own_mirrors` |
| echo suppression for one does not hide a genuine remove from another | `other peers are still told about a dismissal they did not ask for` |
| a peer with no role does not silence one with a role | `a_peer_that_announced_no_role_does_not_silence_the_one_that_did` |

The isolation is **structural**, not a set of checks: `NotificationManager`
holds `HashMap<Fingerprint, Arc<PeerSlot>>`, and each slot owns its own queue,
worker task, mirror table, snapshot state, roles, counters and outbound sender.
**There is no shared global notification authority to get wrong.**

The strongest case is the empty snapshot: a peer sending `BEGIN`…`END` naming
nothing removes every mirror *it* holds and cannot touch another peer's.

---

## 20. Failure handling — **PASS**

### Android (`NotificationHardeningTest`, 13 tests)

A `BreakableListener` whose every method can be made to **throw**, not merely
return `null` — because `NotificationListenerService` throws `SecurityException`
when the binding is torn down underneath the caller, which is exactly what
happens the instant someone revokes notification access in Settings.

| Injection | Result |
| --- | --- |
| `activeNotifications()` returns null | **no snapshot at all** — "I could not ask" must never be encoded as an empty snapshot, which the sink would honour by removing every mirror |
| `activeNotifications()` throws | same; the single ordered producer survives and the next event is handled normally |
| `activeNotification()` throws mid-dismiss | fails closed — nothing cancelled, outcome not `REMOVED` |
| `cancel()` throws | answered **once**, never retried; the echo entry is released |
| `activePackages()` throws | empty set, no crash, recovers |
| keystore unavailable mid-session | role narrows, emission stops, secret **not replaced** |
| policy revoked / peer ungranted | dropped before encoding |
| queue pressure | bounded, terminal removal still arrives last |
| source-map miss | `UNKNOWN_NOTIFICATION`, not an error |
| role unavailable | `REJECTED_ROLE` |

And the property none may break:
`no_failure_path_renders_notification_content_or_a_platform_key` drives every
seam in turn over one session, then searches the status, the queue description
and every non-upsert message for the canaries.

### Desktop

| Injection | Result |
| --- | --- |
| `Notify` fails | reported, not claimed displayed; the identity is remembered with **no** server id so the retry creates rather than replaces; one mirror, not two |
| `CloseNotification` fails | the entry is dropped anyway and the next snapshot reconciles — the dangerous direction, because a failed close leaves something on a screen |
| `NotificationClosed` subscription absent | `DISMISS_REPORTER` is **not announced** |
| backend disappears | roles narrow; recovery re-announces with a strictly newer epoch |
| lock source unavailable | `UnknownLock` reports locked **by type**, not by a check a caller could forget |
| peer send fails / channel gone | the worker drains rather than blocking — `a_dead_outbound_channel_does_not_stall_the_worker` |
| queue full | bounded, counted, terminal items preserved |
| snapshot malformed | `a_malformed_snapshot_marker_does_not_wedge_the_worker` — a bad-width `sync_id` opens no snapshot |
| stale close signal | no `DismissRequest` |
| worker task dies | it does not — every injection is followed by an assertion that the next message is still handled, in order |

`no_failure_path_puts_content_in_a_report` is the desktop counterpart.

**Every error path: fails closed, stays bounded, does not crash the long-lived
service, does not leak content, does not widen authority.** The seams are
asymmetric on purpose — `set_display_failure` vs `set_close_failure` — because
a display that fails shows nothing, which is visible and safe, while a close
that fails leaves something on a screen, which is neither.

The N5 fix that `reports_dismissals()` is gated on **observed** close events
rather than the backend's own claim still stands
(`a_sink_that_cannot_observe_closes_does_not_claim_dismiss_reporting`): a role
is a statement about what this device can physically do, and announcing
`DISMISS_REPORTER` with no stream to observe would offer the user a switch that
could never do anything.

---

## 21. Persistence — **PASS**

Run **after** a full hardware notification flow with distinctive canaries live
on both screens, and **before** the connected suite's teardown.

### Desktop

```console
$ ls ~/.local/share/anyflow/
identity.key   state.json          # and nothing else
$ ls ~/.config/anyflow/ ~/.cache/anyflow/
(neither exists)
$ ls /run/user/1000/anyflow/
control.sock
```

**Every distinct key in `state.json`, enumerated exhaustively** — a full
recursive walk that yields a path for every scalar, **including `false`-valued
keys**, so nothing hides behind a falsy value:

```
/certificate_der_b64                      /peers[]/granted_capabilities/…
/device_id                                /peers[]/last_protocol_version
/key_backing                              /peers[]/notification_policy/allow_dismiss_sync
/schema_version                           /peers[]/notification_policy/allow_mirror
/settings/auto_grant[]                    /peers[]/notification_policy/when_sink_locked
/settings/device_name                     /peers[]/paired_at_unix
/settings/listen_port                     /peers[]/platform
/peers[]/clipboard_policy/…               /peers[]/revoked
/peers[]/device_id · device_name · fingerprint
```

The `notification_policy` object holds **exactly three fields** —
`allow_mirror`, `when_sink_locked`, `allow_dismiss_sync`. No allow-list, no
ids, no server ids, no history, no journal, no content, **and no shape any of
them could take**. `settings.auto_grant` is `["battery.v1"]`.

Canary sweep over every byte of `state.json` and `identity.key` — the N6
canaries (`N6MIRROR4471KESTREL`, `N6ONGOING4471PANGOLIN`,
`quokka-marmoset-4471`, `saola-okapi-4471`, tags `n6mir`/`n6ong`, the raw-key
prefix `0|io.github`, the fixture package) plus `history`, `journal`,
`server_id`: **no hit**. `identity.key` mode `600` in a `700` directory —
existence and mode only; the bytes were never read or printed.

### Android — **PASS**

```console
$ adb shell run-as io.github.yurisismotto.anyflow find . -type f
./files/trust-store.json                (689 B)
```

**One file.** `databases/`, `no_backup/`, `app_webview/` do not exist;
`cache/`, `code_cache/` and `shared_prefs/` are empty; the external app
directory `/sdcard/Android/data/…` holds nothing.

Every distinct key in it:

```
/deviceId                    /peers[]/grantedCapabilities[]
/deviceName                  /peers[]/notificationPolicy/allowDismissSync
/schemaVersion               /peers[]/notificationPolicy/allowMirror
/peers[]/addresses[]         /peers[]/notificationPolicy/allowedApps[]
/peers[]/clipboardPolicy/…   /peers[]/notificationPolicy/includeOngoing
/peers[]/deviceId            /peers[]/notificationPolicy/includeWorkProfile
/peers[]/deviceName          /peers[]/notificationPolicy/whenSourceLocked
/peers[]/fingerprint         /peers[]/pairedAtUnix
```

(plus `knownApps`, empty.) Canary sweep over every byte:

```
NO TITLE, BODY, TAG, RAW KEY, DERIVED ID, SERVER ID OR HISTORY FOUND
```

**What is stored is trust, configuration and policy, and nothing else.**
`allowedApps` is a package name a person chose in a picker.
`notificationSecretGeneration` is a counter; the secret itself lives in the
Keystore, is non-exportable, and never reaches a file.

**No dismiss history, no notification history, no event journal, on either
side.** Structurally, not by promise: neither store has a field that could hold
one, `Snapshot` has no field that could hold text, and `MirrorTable` is
in-memory.

---

## 22. Logging — **PASS**

### Deterministic

`logging.rs` (11 tests) and `NotificationLoggingCanaryTest` (13 tests) run the
whole path under a `TRACE` subscriber and search the capture. Both green in
N6's run. They prove coverage **twice** — that the path under test actually ran,
*and* that the subscriber was attached while it did — because either check alone
can pass for the wrong reason, which is a mistake N5 made once and corrected.

### Live, on the certification hardware, in N6

**Desktop** — the daemon's own journal, 28 lines, 3547 bytes:

```
canaries searched: N6MIRROR4471KESTREL · N6ONGOING4471PANGOLIN
                   quokka-marmoset-4471 · saola-okapi-4471
                   kestrel · pangolin · quokka · marmoset · saola · okapi
                   n6mir · n6ong (tags) · anyflow.fixture (package)
                   0|io.github (raw platform key prefix)
DESKTOP LOG: CLEAN
```

The capture is **non-vacuous**: it contains the convergence line itself, the
role announcements, both snapshot completions and the revocation, so the
assertion is about a log that covers the path.

What it *does* carry, and may: a peer fingerprint prefix
(`peer=3B38 1925 F8A6 E49D`), enums, counts and capability ids.

**Android** — 21 AnyFlow lines in logcat:

```
ANDROID ANYFLOW LOG: CLEAN
```

And, separately, the **fixture's own** discipline held live — it logs the
operation, the id and the tag, never the title or the body, because it runs in
the same sessions as the canaries:

```
AnyFlowFixture: op=post id=71 tag=n6mir channel=anyflow-fixture
AnyFlowFixture: op=post id=72 tag=n6ong channel=anyflow-fixture
```

**One honest limit.** The daemon ran at its ordinary `INFO` level, so the live
desktop capture contains no `debug`-level per-upsert lines. The `TRACE`-level
evidence is the deterministic suite's, which is where it belongs — a live
`TRACE` capture would prove less, not more, because it could not be
exhaustive.

---

## 23. Accessibility — **PASS**

### Desktop (N3 regression gate) — re-run in N6

```console
$ cargo test -p anyflow-gui --lib -- --ignored --test-threads=1
test views::notifications::tests::the_notifications_page_widget_tree ... ok
test result: ok. 1 passed; 0 failed
```

At the shipped `REFRESH_SECS = 2`, its six re-render cases still hold:
unchanged daemon state yields **the same widget objects** (so a screen reader's
focus and AT-SPI handles survive a refresh), a changed state updates the
affected controls, activation after many refreshes still reaches the handler,
the switch is still keyboard-activatable, the dismiss switch still follows the
daemon, and a change on another page does not disturb this one. **No full-page
rebuild has returned.**

> Note for the next runner, carried forward from N5 because it is still true:
> `cargo test -p anyflow-gui -- --ignored` runs the *binary* target and finds
> zero tests. The gate lives in the lib —
> `cargo test -p anyflow-gui --lib -- --ignored --test-threads=1`.

### Android — live semantics tree, N6

Dumped from the running app on the tablet. Merged rows expose a **truthful
state description** rather than a bare label:

```
content-desc='Clipboard, not allowed'    text='Clipboard'
content-desc='Files, allowed'            text='Files'
content-desc='Battery, allowed'          text='Battery'
content-desc='Disconnected'              text='Disconnected'
content-desc='Pair a new device'
```

The capability chips announce *label + state*, not label alone — which is the
N3 property. In the notification settings source, `Role.Button` and
`Role.RadioButton` are set on the real controls, headings carry
`semantics { heading() }`, and decorative glyphs are given
`contentDescription = null` so they are not announced twice. Toggle state is
exposed through the toggle's own semantics rather than through a separate label.

**Live AT-SPI on the desktop was not inspected**: Mutter input injection and
GNOME screenshotting are both denied in this environment. The widget-tree gate
is the substitute and it is a stronger one for regression purposes, because it
asserts object identity across refreshes, which an AT-SPI snapshot cannot.

---

## 24. Fixture safety — **PASS**

`android/fixture/` — a separate Gradle module, audited structurally and by
building both APKs.

**It cannot reach the AnyFlow APK, and nothing depends on it:**

```console
$ grep -rn "fixture" android/app/build.gradle.kts
(no dependency — only an unrelated comment about cross-language fixtures)
$ grep -rn "anyflow.fixture" android/app/src/main/
(none in production source)
$ unzip -l app-debug.apk | grep -ci fixture
0
```

**Its permission list is the whole point:**

```console
$ aapt2 dump badging fixture-debug.apk
package: name='io.github.yurisismotto.anyflow.fixture' versionCode='1'
uses-permission: name='android.permission.POST_NOTIFICATIONS'
uses-permission: name='io.github.yurisismotto.anyflow.fixture.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION'
launchable-activity: name='…fixture.FixtureActivity' label='AnyFlow Fixture'
```

**No `INTERNET`** — so there is nowhere for anything it displays to go. No
storage, contacts, location, camera, microphone, `QUERY_ALL_PACKAGES`, listener
service or foreground service. The second entry is a signature-level permission
androidx defines for itself.

**It cannot control AnyFlow internals.** Its exported surface is one activity
that posts, updates, removes, groups or clears **its own** notifications. It
holds no permission that would let it read anything, and it has no IPC to
AnyFlow. The `exported="true"` is stated rather than hidden: any app on the
device can make the fixture post a notification with text of its choosing — but
not as anybody else, not over a network it does not have, and not reading
anything, so the surface is equivalent to the caller posting its own
notification.

**And it keeps the same logging discipline as the product** (§22), verified
live.

### For comparison, the AnyFlow APK's own permissions

```
INTERNET · ACCESS_NETWORK_STATE · CHANGE_WIFI_MULTICAST_STATE ·
CHANGE_NETWORK_STATE · FOREGROUND_SERVICE ·
FOREGROUND_SERVICE_CONNECTED_DEVICE · POST_NOTIFICATIONS · CAMERA
```

**Every privilege ADR-0015 §2 refuses is absent**:
`BIND_ACCESSIBILITY_SERVICE`, `QUERY_ALL_PACKAGES`, `SYSTEM_ALERT_WINDOW`,
`READ_LOGS`, `MANAGE_EXTERNAL_STORAGE`, location. `CAMERA` is the pre-existing
QR-pairing permission (ADR-0006) and is not on that list.

`BIND_NOTIFICATION_LISTENER_SERVICE` correctly does **not** appear as a
`uses-permission` — it is declared on the `<service>`, which is what stops any
other app binding it, exactly as ADR-0015's Consequences require:

```xml
<service android:name=".notifications.AnyFlowNotificationListener"
         android:exported="false"
         android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE">
  <meta-data android:name="android.service.notification.default_autobind"
             android:value="false" />
```

---

## 25. Android JVM — **PASS**

```console
$ cd android
$ JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
    ./gradlew :app:testDebugUnitTest --rerun-tasks
BUILD SUCCESSFUL in 25s
```

Read out of the JUnit XML under `app/build/test-results/testDebugUnitTest/`,
**not** out of `BUILD SUCCESSFUL`:

```
tests=523   failures=0   errors=0   skipped=0
```

**`skipped = 0`.**

---

## 26. Android connected — **PASS**

Run **last**, after every persistence and logging sweep, because the suite
uninstalls the app and the test package when it finishes and would otherwise
take the evidence with it (N3 §F9).

Its assumptions were prepared **deterministically** with the N5 fixture before
the run — app installed, listener allowed via
`cmd notification allow_listener`, `POST_NOTIFICATIONS` granted, device
unlocked and verified (`deviceLocked=0`, `showing=false`), and the fixture
posting the exact title the gate looks for.

```console
$ JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk \
    ./gradlew :app:connectedDebugAndroidTest
Starting 102 tests on SM-X620 - 16
…
BUILD SUCCESSFUL in 2m 43s
```

From the JUnit XML under `app/build/outputs/androidTest-results/connected/`:

```
tests=102   failures=0   errors=0   skipped=0
```

**Green on the first attempt with `skipped = 0`** — which is the point of the
fixture existing. N5's first attempt produced a green build with one silently
skipped hardware gate, and the correction was to move that gate off
`com.android.shell` onto a fixture whose precondition a run can *create*. N6 is
the first wave to benefit from that, and it did.

---

## 27. Rust regression — **PASS**

```console
$ cd desktop
$ cargo fmt --all --check                                   (clean)
$ cargo build --workspace --locked -j 2                     Finished
$ cargo test  --workspace --locked -j 2
   passed=703   failed=0   ignored=22
$ cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
   0 errors, 0 warnings
```

Of the 703, the notification suites are: `sink.rs` 60, `hardening.rs` 40,
`dismiss.rs` 33, `logging.rs` 11, `daemon/tests/notifications.rs` 36,
`renegotiate.rs` 10, `notifications_schema.rs` 17, plus the portable contract's
own tests in `anyflow-core`.

**No warning of any kind was emitted.**

---

## 28. Real D-Bus / lock / GTK — **PASS**, stated precisely

```console
$ cargo test -p anyflow-capability-notifications --test real_dbus -- --ignored --test-threads=1
   10 passed; 0 failed        … in 0.25s
$ cargo test -p anyflow-capability-notifications --test real_lock -- --ignored --test-threads=1
   2 passed; 0 failed
$ cargo test -p anyflow-gui --lib -- --ignored --test-threads=1
   1 passed; 0 failed
```

> **Ten "passed" in `real_dbus` is not ten gates.** N6 re-ran with
> `--nocapture` to say exactly which executed:
>
> ```
> a_human_dismissal_end_to_end_produces_exactly_one_dismiss_request … SKIPPED: set ANYFLOW_HUMAN_DISMISS=1
> a_human_dismissal_on_this_desktop_is_reported_as_reason_two       … SKIPPED: set ANYFLOW_HUMAN_DISMISS=1
> a_thirty_minute_soak_stays_bounded_and_converges                  … SKIPPED: set ANYFLOW_SOAK=1
> ```
>
> **7 of the 10 executed and passed. 3 declined and returned immediately.** The
> 0.25-second wall time is the proof. This is N5's remaining debt item 11,
> confirmed rather than inherited, and it is why this report does not write "10
> real D-Bus gates passed".

The **7 that actually ran**, against the live GNOME Shell 50.4 server:
`the_real_server_answers_and_says_what_it_can_do`,
`a_notification_is_created_replaced_in_place_and_closed`,
`closing_an_unknown_id_is_a_success`,
`the_server_reports_our_own_close_with_reason_three`,
`a_body_reaches_the_server_escaped_and_nothing_is_left_behind`,
`a_low_urgency_notification_is_accepted`,
`the_real_session_can_report_human_dismissals`.

The **2 human-dismiss gates** need a person to close a banner and could not be
driven here (§33 D). Their evidence is **N5's**, cited as N5's.

The **soak** was then run explicitly by N6 with `ANYFLOW_SOAK=1
ANYFLOW_SOAK_SECS=300`, and its result is reported in §18 as a **302-second N6
regression run**, never as a re-run of N5's 1802-second certification soak.

`real_lock` (2 gates) agreed with `loginctl` on the live session. The GTK gate
is §23.

---

## 29. N6 hardware smoke

Performed on the SM-X620, which was available and stable throughout. Recorded
exactly, including what was **not** done.

| # | Step | Result |
| --- | --- | --- |
| 1 | install the current build | **done** — app, androidTest and fixture APKs |
| 2 | pair | **done** — real TLS 1.3, real SPKI pin, real proof. Confirmed on the desktop only after the offered fingerprint `3B38 1925 F8A6 E49D` was checked against the one the device reported from its own `DeviceIdentity`. `pair grants=battery.v1,files.v1` |
| 3 | enable notifications | **done** — OS listener access, then the phone-side grant and policy |
| 4 | select the fixture app | **done** — `allowedApps=["…anyflow.fixture"]`, read back from the trust store |
| 5 | mirror a clearable notification | **done** — `showing 1 of 1 mirrored` |
| 6 | update it | **done** — still `1 of 1`; the upsert replaced in place |
| 7 | human dismiss on Fedora | **NOT DONE** — needs a person; see below |
| 8 | verify the Android original removed | **NOT DONE** — depends on 7 |
| 9 | post an ongoing notification | **done** — `flags=ONGOING_EVENT` on Android, `showing 3 of 3 mirrored` |
| 10 | human-dismiss that mirror | **NOT DONE** — depends on 7 |
| 11 | verify the source refuses and the original remains | **NOT DONE** — depends on 7 |
| 12 | Wi-Fi off/on once | **done** — 20 s outage |
| 13 | verify reconnect / resync | **done** — `snapshot complete named=3 closed=0`, no grace expiry across 75 s |
| 14 | revoke the notifications grant | **done** — `closed every mirror for a peer closed=3 reason="revoked"` |
| 15 | verify no further mirroring | **done** — a notification posted afterwards stayed on Android and **never crossed**: `showing 0 of 0 mirrored` |

**Steps 7, 8, 10 and 11 were not performed, and nothing in this report claims
them.** They need a human to click a GNOME notification banner; Mutter input
injection is denied in this environment and GNOME Shell exposes no banner to
AT-SPI. The corresponding certified evidence is **N5 §10 and §8 step 14–15**,
and it is cited as N5's throughout. What N6 substitutes is the deterministic
dismissal suite (33 + Android rules tests) and the soak's negative property
(**0 `DismissRequest`s in 2652 cycles** with nobody dismissing anything).

**Not re-run, deliberately:** N5's 30-minute in-process soak and 15-minute
hardware soak. §28 of the brief asks for exactly that restraint, and §18 marks
the boundary.

### The pairing path, stated honestly

The QR is camera-only, so pairing went through
`HostDrivenCertificationHarness`: the desktop minted a real single-use token,
`anyflow pair` printed the payload that token encodes, and the device's own
`QrPayload.parse` → `AnyFlowApp.pair` ran it. Everything after the parse is the
product's real code path. **The only step skipped is turning pixels into that
string. No camera scan is claimed.**

---

## 30. Other capability regression — **PASS**

`notifications.v1` did not damage anything.

| Capability | Evidence |
| --- | --- |
| `battery.v1` | **N6 hardware** — `battery 79% (NotCharging, 8s old)` on the live session, with `notifications.v1` running beside it; auto-granted set still exactly `["battery.v1"]` |
| `files.v1` | granted at pairing as designed (`pair grants=battery.v1,files.v1`); its suites are inside the 703 and the 523; the desktop chip read *"Files, allowed"* on the device |
| `clipboard.v1` | withheld at pairing as designed; chip read *"Clipboard, not allowed"*; `clipboard.v1` suites green in both runs; the Wayland backend initialised normally in the same daemon |
| transport / session | `inbound_capability_messages_reach_the_handler_in_wire_order` and `replies_keep_their_order_under_queue_pressure` green |
| pairing / trust store | a full pair, grant, revoke cycle executed on hardware with 11 peer records intact |

None of these was recertified from zero, and none needed to be.

---

## 31. Windows CI readiness

`.github/workflows/portable-windows-msvc.yml` audited.

**No duplicate push + PR execution on a feature branch.** Triggers are
`pull_request` on `[main, develop]` and `push` on `[main, develop]`. A push to
`cert/notifications-v1-n6-final-certification` matches neither; its PR fires
`pull_request` once; the eventual merge to `main` fires `push` once. Plus a
`concurrency` group keyed on `github.ref` with `cancel-in-progress`.

**`hardening.rs` is deliberately classified portable**, and the workflow says
why in a comment: everything it exercises runs against the
`MemorySink`/`MemoryLock` pair, which is the whole reason the `NotificationSink`
seam exists, and a Windows sink will want every one of those assertions
unchanged.

**The classification guard is strict and was not weakened.** It is an exact set
comparison — `Compare-Object $expected $actual` fails on **any** difference,
added or removed:

```pwsh
$expected = @('dismiss.rs','hardening.rs','logging.rs','real_dbus.rs','real_lock.rs','sink.rs')
```

A new test file in that directory fails the job until someone classifies it.
The same discipline guards `core/tests`.

**Linux-only tests stay feature-gated**: `real_dbus.rs:36` and `real_lock.rs:30`
are whole-file `#![cfg(feature = "linux-dbus")]`, so they compile to empty
binaries with the feature off — which is why they need no exclusion in the
`--no-run` step.

**Verified locally in N6:**

```console
$ cargo test --locked --no-run --no-default-features -p anyflow-capability-notifications
  Executable tests/dismiss.rs      ✓
  Executable tests/hardening.rs    ✓
  Executable tests/logging.rs      ✓
  Executable tests/real_dbus.rs    ✓   (empty with the feature off)
  Executable tests/real_lock.rs    ✓   (empty with the feature off)
  Executable tests/sink.rs         ✓
```

The dependency-boundary step measures the **resolved graph**, not source greps,
and forbids both Linux crates and unified platform features — the trap Wave 0
actually fell into once. The ARCH-010 `unsafe` policy step keeps
`unsafe_code = "forbid"` on the notifications crate.

**Remote Windows PASS is not claimed.** That evidence comes only after the
certification PR is pushed and GitHub Actions runs the job.

---

## 32. Protocol / security guards

### Protocol guard — **PASS**

```console
$ git diff -- protocol/
(empty)
$ git diff --stat 26952da HEAD -- protocol/     # 26952da = the N0 commit that added the schema
(empty)
```

`notifications_v1.proto` is **byte-identical to the file N0 committed.**

### Forbidden-capability search over production source — **PASS**

Searched `android/app/src/main` and `desktop/*/src`,
`desktop/capabilities/*/src`. Every hit classified:

| Term | Hits | Classification |
| --- | --- | --- |
| `RemoteInput` | 3 | **prose asserting absence** (3 doc comments) |
| `ActionInvoked` | 1 | **prose** — `dbus.rs` explaining there is no action for it to be about |
| `snoozeNotification` | 0 | absent |
| `cancelAllNotifications` | 0 | **absent from production**; present only in the test-only fixture, clearing *its own* notifications |
| `snooze` | 1 | **prose** asserting absence |
| `notification database` | 0 | absent |
| `notification history` | 6 | 5 **prose** asserting absence + 1 GUI string telling the user there is none, and a test asserting that string |
| `reply` / `replies` | 117 | every **code** hit is a `zbus::Error::InvalidReply` variant; the rest are doc comments |
| `open app` | 0 | absent |
| `PendingIntent` | 13 | **3 prose** on the notifications path; **10 code** hits, all AnyFlow's *own* notifications — `ConnectionService` foreground-service notification, `ClipboardTileService` quick-settings tile, `clipboard.v1`'s "clip received" notification. All pre-existing, none reachable from any inbound `notifications.v1` message, and all covered by the own-package drop so none can mirror |

**No new production remote-control path beyond exact dismissal exists.** The
only remote effect in the capability remains:

```
DismissRequest → mapped notification_id → local in-memory reverse lookup
              → live clearability re-check → cancelNotification(mappedKey)
```

with **exactly one** production call site
([AnyFlowNotificationListener.kt:92](android/app/src/main/java/io/github/yurisismotto/anyflow/notifications/AnyFlowNotificationListener.kt#L92)).

### Schema regression guard

`desktop/proto/tests/notifications_schema.rs` — 17 descriptor-level tests that
fail if the message set changes, if `NotificationUpsert` gains or loses a
field, if a field name hints at a prohibited capability, if a `bytes` field
appears that is not one of the four fixed-width identifiers, if any message
gains an open-ended container, if `DismissRequest` gains a remote-execution
path, if the control envelope gains an escape hatch, if a field number is
retired or reused, or if an enum value changes.

### Documentation consistency

`docs/architecture/NOTIFICATIONS.md` was audited claim by claim against the
tree. Every factual statement it makes about N0–N5 behaviour is **accurate** —
the cfg-gating of `real_dbus.rs`/`real_lock.rs`, the classification guard's
six-file set, the portable-boundary listing, the descriptor regression test, the
three ordering guarantees and their two pinning tests, the `limits.rs`
tunables, and the convergence description. Three statements had gone stale only
because N6 happened, and were corrected — status line, the certification row,
and the wave-table row. Nothing else in the document, and **no ADR**, was
touched: accepted ADR history is not rewritten.

**One inconsistency inside N5's own report is noted and left alone**: §29
records `passed=701` while its §37 acceptance table records `703 passed`. The
current tree reports **703**. N5's report is accepted history and N6 does not
edit it; the correct number is recorded here.

---

## 33. Remaining debts, classified

**A) blocks `notifications.v1` v1 — none.**

**B) general V1 platform work, not a notifications defect**

| Debt | Origin | Note |
| --- | --- | --- |
| No desktop localization framework; all GTK/CLI copy is English | N3-8 | Platform-wide; every capability's desktop surface has it. Creating one is not N6 work |
| One user-facing Android string is a literal, not a resource — `NotificationSettingsScreen.kt:112`, *"This device is no longer paired."* | **N6 F1** | The only user-facing literal in the three notification screens, which otherwise draw on **83 distinct string resources**; it is a fallback shown when a peer is forgotten while its settings screen is open |
| `IdentityReset` has no caller | N1-6 | The secret's identity-reset rotation (ADR-0016 §5) is implemented but nothing invokes the reset yet |
| `knownApps` is a second per-peer package list | N3-5 | Data-model tidiness; it is empty in practice and holds configuration, not content |

**C) post-v1 enhancement**

| Debt | Origin |
| --- | --- |
| KDE Plasma not certified (the sink is portable; only GNOME is proven) | N2-6 |
| The lock detector resolves one logind session | N2-9 |
| Windows / macOS sinks and sources | by design |
| Play policy question OQ-09 — notification access is a restricted Play area | N1-10 / ADR-0015. **Blocks no release today**: AnyFlow ships from GitHub |
| Work-profile *installation* detection | N3-7 |
| One UI ignores `EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME` | N3-6 |

**D) test-environment limitation**

| Limitation | Status in N6 |
| --- | --- |
| A human dismissal cannot be driven here — Mutter input injection denied, GNOME exposes no banner to AT-SPI | **Recurred.** Steps 7/8/10/11 of the smoke and the two `ANYFLOW_HUMAN_DISMISS` gates were not run. Evidence is N5's, cited as N5's |
| GNOME Shell 50.4 on Wayland cannot restart without ending the session, so a live server restart is untestable | **Recurred.** Covered deterministically by three tests against a fake that renumbers from 1, which reproduces the dangerous half faithfully |
| Pairing skips the camera; the harness drives the real path minus the ZXing decode | **Recurred.** No camera scan is claimed anywhere |
| `am instrument` force-stops the app under test, killing a live session | **Recurred and worked around.** The harness was used only for setup, before the session that had to survive |
| USB / adb instability on the tablet | **Did not recur.** The device stayed on USB for the whole wave; no reboot was needed |
| A stuck `NotificationShade` holding focus device-wide | **Did not recur.** No focus-dependent step was needed |
| `settings put secure enabled_notification_listeners` is not enough on One UI 8 | **Confirmed, and sharpened — N6 F2.** `cmd notification allow_listener` *worked*, and `settings get secure enabled_notification_listeners` **still reported the old value**. The **read** is unreliable too, not only the write. `dumpsys notification \| grep -A2 "Allowed notification listeners"` is the reliable read, and `cmd notification disallow_listener` the reliable undo. Worth writing down, because both the setting-write and the setting-read look like they worked |
| The soak is `#[ignore]`d **and** env-gated, so a plain `--ignored` run reports it passed after an immediate return | **Confirmed precisely** — §28. Ten passing tests in `real_dbus` is seven gates plus three that declined |
| `requestRebind` stickiness never independently provoked | Still open. N6 observed ordinary bind/unbind/rebind behaving correctly across a grant and a Wi-Fi outage, but did not provoke the sticky case |
| One UI app-sleep over a long idle (POC-NOTIF-04) | **Still not tested.** N6 ran no idle test, and 302 seconds of *active* soak is not one. Unchanged from N5 |

**Not silently promoted to a blocker:** none of B or C. **Not excused as
debt:** nothing in this list is a notifications security or correctness defect;
F1 is an i18n gap on a fallback string and F2 is a property of One UI's shell
tooling.

---

## 34. Final risk review

Every answer cites code, a test, or evidence gathered in N6.

| Question | Answer | Evidence |
| --- | --- | --- |
| Can an **unpaired** device inject a notification? | **No.** | It cannot complete the TLS handshake against a pinned identity; `lookup_peer` returns `PeerStatus::Unknown` and no session is built. No session, no `PeerSlot`, no path to `handle_control` |
| Can an **ungranted paired** peer inject one? | **No.** | `policy_for` returns `DENIED` for a peer without the grant, and the check runs twice — once in `handle_control` before queueing, once in `apply_upsert` before displaying. **N6 hardware:** with the listener allowed but the desktop grant absent, `showing 0 of 0 mirrored` |
| Can a peer **spoof another peer's** mirror namespace? | **No.** | Mirrors are keyed `(Fingerprint, NotificationId)` and the fingerprint is the pinned TLS identity, not `origin_device_id`. `an_empty_snapshot_from_one_peer_closes_only_its_own_mirrors` is the strongest form |
| Can a peer dismiss an **arbitrary** Android notification? | **No.** | `DismissRequest` has two fields and neither is a package, id or tag. The id must be one this device *itself* put in its in-memory `SourceIdMap`; the origin must be this device; and the map is not consulted until grant, role and policy have all passed |
| Can **expiration** dismiss the phone's notification? | **No.** | `is_human_dismissal` is `matches!(self, Self::Dismissed)` — reason 2 only, a single variant rather than a list of exclusions. **N6 soak: 0 `DismissRequest`s in 2652 cycles** |
| Can a **stale role epoch** re-widen authority? | **No.** | `apply` refuses `epoch <= self.epoch`; row 6 of `the_epoch_table_holds_in_order` is exactly this attack. And a role is not authority in the first place |
| Can notification content **survive on disk**? | **No.** | Full recursive key inventory of both stores in §21; neither has a field that could hold it. Android writes exactly one file, 689 bytes, of trust and policy |
| Can notification content appear in **AnyFlow logs**? | **No.** | `redact.rs` with no debug override; all 22 Android log sites read and classified; 24 deterministic canary tests at `TRACE`; **both live N6 captures clean and non-vacuous** |
| Can enabling mirroring **silently enable dismiss sync**? | **No.** | Separate fields, separate defaults, separate readiness types, and `may_sync_dismissals` requires both. **N6 hardware:** `dismiss-sync off` immediately after the grant converged |
| Can a **work-profile** notification leak by default? | **No.** | `includeWorkProfile = false`, checked at the source before encoding and independently of the app list |
| Can a **locked source** transmit full content under `AppOnly`? | **No.** | The reduction happens before `NotificationWire.encode`, so the content never exists on the wire. Unknown lock resolves to locked |
| Can a **backend restart** turn an old close into a dismiss? | **No.** | A stale server id finds no live mirror in `forget_server_id`, so `maybe_request_dismissal` is never reached — `a_close_for_a_stale_server_id_produces_no_dismiss_request` |
| Can **reconnect create duplicate mirrors**? | **No.** | Ids are derived and survive a source restart, so the snapshot reconciles in place. **N6 hardware:** `snapshot complete named=3 closed=0` after a real Wi-Fi outage, mirrors `3 of 3` throughout |
| Can a **capability grant become stuck** until a manual reconnect? | **No — and this is the availability answer, not a security one.** | The grant ends the frozen session; the phone's own coordinator redials on its ordinary transient backoff and the new `HELLO` recomputes the intersection. **N6 hardware: 6.18 s, no manual disconnect.** The bound is one request per session, so it converges without a clock and cannot storm |
| Can a **terminal remove disappear** under queue pressure? | **Not silently.** | Non-terminal items are evicted first; a removal is dropped only when the queue is full *of removals*, and then it is counted, logged at `warn`, and the peer is answered `RATE_LIMITED`. 300 removals under pressure: **0 dropped**. **N6 soak: `dropped_terminal 0`** |

---

## 35. Production-readiness decision

`notifications.v1` is **ready for AnyFlow V1**.

The judgement rests on four things the audit confirmed rather than assumed.

**The dangerous directions are closed by shape, not by check.** There is no
field in `DismissRequest` that could carry an action, so no future change can
widen it without changing the schema and tripping seventeen descriptor tests.
There is no field in either trust store that could hold notification content,
so "no history" is not a promise anyone has to keep. `is_human_dismissal` is
one `matches!` on one variant, so a fifth close reason cannot grow a hole. The
allow-list starts empty, so "deny by default" needs no rule that could be
inverted.

**Every authority is re-derived, per peer, per message.** The grant is asked
twice on the display path and again before a dismissal is sent. The lock is read
from the platform per notification. Clearability is re-read from the live
notification rather than trusted from the flag the desktop holds. A role is
never an input to any of it.

**The failure modes were found by running the thing, and are fixed.** N5's worst
defect — mirrors closing sixty seconds after a session replacement, on a healthy
session — was invisible to every deterministic test and needed a real Wi-Fi
outage on a real device to appear. **N6 reproduced the exact triggering
condition on hardware and watched the fix hold for 75 seconds past the grace
window.**

**What is not proven is stated, not glossed.** The human-dismiss path needs a
person and N6 could not drive one; that evidence is N5's and is labelled N5's
everywhere it appears. The desktop notification-server restart cannot be
executed on Wayland and is covered deterministically. The camera scan is not
claimed. One UI app-sleep over a long idle is still untested. None of these is a
security gap; each is a verification limit, recorded where it bites.

### Score: **93 / 100**

| Area | Score | Reasoning for the deduction |
| --- | --- | --- |
| Architecture / protocol | **10 / 10** | Byte-frozen since N0 across five waves. Asymmetry solved by a pattern (ADR-0017) rather than a special case. Six bodies and no escape hatch, guarded at descriptor level |
| Security / identity | **10 / 10** | Pinned fingerprint is the only identity; roles are structurally incapable of being authorization; one `cancelNotification` call site; the reverse path is in-memory and gated behind five checks that run in an order chosen so the message cannot become a lookup oracle |
| Privacy | **10 / 10** | Reduction before encoding; no content on disk or in logs at any level, verified by exhaustive key inventory and live canary sweep on both sides; the secret is non-exportable so there is nothing to leak |
| Correctness | **10 / 10** | 703 Rust + 523 JVM + 102 instrumented, zero failures, zero skips. Adversarial epoch table, snapshot failure matrix, and every bound measured rather than sampled |
| Reliability / recovery | **9 / 10** | Convergence measured at 6.18 s; resync exact; the replaced-session defect fixed and re-confirmed on hardware. **−1:** a live notification-server restart remains unexecutable on this platform, and `requestRebind` stickiness was watched but never provoked |
| Consent / UX | **10 / 10** | Three permissions never collapsed; two readiness types because they fail apart; deny-by-default proven on hardware; mirroring demonstrably did not enable dismissal |
| Accessibility | **9 / 10** | The N3 widget-identity gate holds at the shipped refresh interval; Android merged rows announce truthful state. **−1:** live desktop AT-SPI could not be inspected in this environment |
| Testing | **9 / 10** | Deterministic coverage is excellent and the failure injections throw rather than return null, which is the harder and more honest fake. **−1:** three `real_dbus` entries report "passed" while declining to run — a convention that is documented but still reads as a green gate in CI output |
| Hardware validation | **8 / 10** | Pairing, convergence, mirroring, update-in-place, ongoing, resync across a real outage, revocation, persistence and logging all exercised on the certification device today. **−2:** the human-dismiss path could not be driven here, so its hardware evidence is N5's; and long-idle app-sleep is still untested |
| Portability / maintainability | **9 / 10** | The portable set compiles for MSVC with a strict classification guard and a resolved-graph dependency boundary; `hardening.rs` is deliberately portable so a Windows sink inherits it. **−1:** remote Windows CI evidence does not exist yet, only local `--no-default-features` compilation |
| Documentation | **9 / 10** | ADRs are decision records that actually decide; the code's comments carry the reasoning at the site that needs it. **−1:** one hardcoded user-facing string, and N5's report contradicts itself on a test count |

**Total: 93 / 100.** Every deduction is a verification or environment limit or a
piece of hygiene — none is a defect in the shipped behaviour of
`notifications.v1`.

---

## 36. Git status

Nothing was added, committed, pushed or opened as a PR. `HEAD` is still
`836cbd4`.

```console
$ git status --short
 M docs/architecture/NOTIFICATIONS.md
?? NOTIFICATIONS-V1-N6-FINAL-CERTIFICATION.md

$ git diff --check
(clean)

$ git diff --stat
 docs/architecture/NOTIFICATIONS.md | 10 +++++-----
 1 file changed, 5 insertions(+), 5 deletions(-)

$ git diff --name-status
M       docs/architecture/NOTIFICATIONS.md

$ git log --oneline -1
836cbd4 Merge pull request #22 …          ← HEAD unmoved; nothing staged
```

**There is no production code diff**, which is the expected outcome of a
read-only certification. The only changes are this report and five lines of
`NOTIFICATIONS.md` — three factual statements that went stale because N6
happened (the status line, the "what exists" heading, the certification and
wave-table rows). No ADR was touched, no `.proto`, no Kotlin, no Rust.

**Artefact audit** — `*.apk *.aab *.key *.pem *.p12 *.pfx *.jks *.keystore
*.log`, QR images, UI dumps, `state.json`, trust stores, `identity.key`,
`target/`, `build/`: **none present in the diff.** Every working file used for
the hardware gates — pairing payloads, logcat captures, the pulled trust store,
UI dumps, soak output — lives in the session scratchpad, outside the
repository. No secret key material was printed at any point; `identity.key` was
checked for existence and mode only.

### Machine state left behind

* **Tablet:** `enabled_notification_listeners` verified **byte-identical** to
  the value captured before this wave. `io.github.yurisismotto.anyflow` and its
  test package are **uninstalled** — the connected suite removed them, as it
  always does. `io.github.yurisismotto.anyflow.fixture` remains installed and
  inert with **zero active notifications**, exactly as N6 found it;
  `adb uninstall io.github.yurisismotto.anyflow.fixture` removes it. The device
  is unlocked, on USB, DND off.
* **Desktop:** `anyflowd` running at the ordinary `INFO` level with the same
  trust store plus the one peer this wave paired (`3B38 1925 F8A6 E49D`, its
  `notifications.v1` grant **revoked** at the end of the smoke). The GNOME
  session is clean — the soak's final empty snapshot took everything off the
  screen, and the revocation closed the last three mirrors.
* Nothing this wave posted is still on either screen.

---

## 37. Acceptance

| §37 criterion | Result |
| --- | --- |
| no P0 / BLOCKER | **PASS** — §33, two non-blocking findings |
| protocol contract remains coherent | **PASS** — §7, byte-frozen since `26952da` |
| trust / grant / policy / role chain fail-closed | **PASS** — §12, §34 |
| notification content not persisted or logged | **PASS** — §21, §22 |
| dismiss remains exact-notification-only | **PASS** — §16, one call site |
| reconnect / resync correct | **PASS** — §15, hardware and deterministic |
| mid-session grant converges automatically | **PASS** — §17, **6.18 s on hardware** |
| bounds and multi-peer isolation proven | **PASS** — §18, §19 |
| Android JVM passes with `skipped=0` | **PASS** — 523 / 0 / 0 / 0 |
| connected hardware suite passes with `skipped=0` | **PASS** — 102 / 0 / 0 / 0, first attempt |
| Rust passes | **PASS** — 703 passed, 0 failed, 0 warnings |
| real Linux gates pass | **PASS** — 7 of 10 executed (§28), 2 lock, 1 GTK |
| N3 accessibility regression green | **PASS** — §23 |
| N4 dismissal regression green | **PASS** — §16 |
| N5 hardening regressions green | **PASS** — §15, §17, §9 |
| protocol unchanged except documentation | **PASS** — §36, zero production diff |
| no forbidden remote-control functionality | **PASS** — §32, every hit classified |


---

## Appendix A — i18n / user-text audit (brief §9)

**Android.** The three `notifications.v1` screens —
`NotificationSettingsScreen`, `NotificationUiMapping` and `AppPickerScreen` —
draw on **83 distinct string resources**. Exactly one user-facing string is a
Kotlin literal (finding F1, §33 B), introduced in N3 (`98e8b70`) and **not new
in N6**. No new inline user-facing notification text was added by this wave,
because this wave added no product code.

**Desktop.** AnyFlow has **no desktop localization framework**, and that remains
true. It is recorded here as a **platform-wide** debt (§33 B) rather than a
`notifications.v1` one: the GTK GUI and the CLI are English-only for every
capability. N6 did **not** create an i18n system, per the brief.

**Notification content is never translated.** It is the user's own data,
rendered as the source produced it, and there is no code path that would
transform it — `build_mirror` only ever *removes* content, and `text.rs` only
sanitises and length-caps. A sink that rewrote a notification's text to
simulate a feature the desktop lacks is explicitly forbidden by the schema's
own `Progress` comment, for the same reason.

---

## Appendix B — what N6 ran, at a glance

| Gate | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --all --check` | clean |
| Rust build | `cargo build --workspace --locked -j 2` | ok |
| Rust tests | `cargo test --workspace --locked -j 2` | **703 / 0**, 22 ignored |
| Rust lints | `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | **0 warnings** |
| Real D-Bus | `--test real_dbus -- --ignored` | **7 executed**, 3 declined |
| Real lock | `--test real_lock -- --ignored` | **2 / 2** |
| GTK widget tree | `-p anyflow-gui --lib -- --ignored` | **1 / 1** |
| N6 soak | `ANYFLOW_SOAK=1 ANYFLOW_SOAK_SECS=300 … soak` | **302 s, 2652 cycles** |
| Portable compile | `cargo test --no-run --no-default-features -p anyflow-capability-notifications` | 6 targets |
| Android JVM | `./gradlew :app:testDebugUnitTest --rerun-tasks` | **523 / 0 / 0 / 0** |
| Android connected | `./gradlew :app:connectedDebugAndroidTest` | **102 / 0 / 0 / 0** |
| Named regressions | `renegotiate`, `hardening`, `daemon/tests/notifications` | 10, 40, 36 |
| Hardware smoke | §29 | 11 of 15 steps; 4 need a person |

---

## NOTIFICATIONS.V1 FINAL CERTIFICATION: PASS
## NOTIFICATIONS.V1 READY FOR ANYFLOW V1
