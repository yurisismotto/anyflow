# `notifications.v1` — N0 report

> ## Outcome: **NOTIFICATIONS.V1 N0 PASS**
>
> Protocol, capability contracts, ADRs, architecture documentation and tests.
> **No platform listener, no sink, no UI, no runtime.** Nothing mirrors a
> notification, and no peer can tell the capability exists.

| Field | Value |
| --- | --- |
| Wave | N0 — protocol foundation and architecture contracts |
| Branch | `feature/notifications-v1-n0-protocol` |
| Baseline commit | `40e16b4` — *Merge pull request #16 from yurisismotto/research/notifications-v1* |
| Date | 2026-09-08 |
| Committed? | **No.** Nothing added, committed, pushed, or turned into a PR |

---

## 1. Baseline

```console
$ git branch --show-current
feature/notifications-v1-n0-protocol

$ git status --porcelain     # before any change
(empty)

$ git log --oneline -3
40e16b4 Merge pull request #16 from yurisismotto/research/notifications-v1
87229dd docs: specify notifications v1 architecture and security
5f5dc2a Merge pull request #15 from yurisismotto/fix/post-wave0-certification-debts
```

Baseline regression, measured before any edit:

| Suite | Result |
| --- | --- |
| Rust `cargo test --workspace --locked` | **371 passed, 0 failed, 9 ignored** |
| Android `:app:testDebugUnitTest` | **243 tests, 0 failures** |
| `cargo fmt --check` | clean |

Canonical material read before writing anything: ADR-0015, the seven research
documents and two POC records under `docs/research/notifications-v1/`, both
top-level reports, `docs/architecture/{OVERVIEW,PROTOCOL,CLIPBOARD,FILES}.md`,
`docs/security/THREAT_MODEL.md`, the three existing capability `.proto` files,
`anyflow_core::{capability,session,framing,store}`, and the Android
`Capability`/`CapabilityRegistry` pair.

**No discrepancy was found between the approved documentation and the
implementation.** One place where this brief and the merged implementation plan
disagree is resolved in favour of the plan, in §16.

---

## 2. Files changed

Production (5 files, one of them new):

| File | Change |
| --- | --- |
| `protocol/proto/anyflow/v1/capabilities/notifications_v1.proto` | **New.** The capability schema |
| `desktop/proto/build.rs` | +1 line: the new `.proto` in the compile list |
| `desktop/proto/Cargo.toml` | +8: `prost-types`, `protox` as **dev**-dependencies for the schema regression test |
| `desktop/core/src/notifications.rs` | **New.** The portable wire contract |
| `desktop/core/src/lib.rs` | +1 line: `pub mod notifications;` |

Tests (3 new files, one modified):

| File | Change |
| --- | --- |
| `desktop/core/tests/notifications_protocol.rs` | **New.** 42 tests: contract, roles, limits, snapshot, identity, dismissal, compatibility |
| `desktop/proto/tests/notifications_schema.rs` | **New.** 12 descriptor-based schema regression tests |
| `android/app/src/test/java/…/NotificationsProtocolTest.kt` | **New.** 18 JVM tests |
| `desktop/core/src/session.rs` | +114 lines, **entirely inside `mod dispatch_tests`** — the in-order-delivery proof (§11) |

Documentation:

| File | Change |
| --- | --- |
| `docs/adr/ADR-0016-notification-identity.md` | **New** |
| `docs/adr/ADR-0017-capability-roles.md` | **New** |
| `docs/architecture/NOTIFICATIONS.md` | **New** |
| `docs/architecture/PROTOCOL.md` | +66: a `notifications.v1` section beside `clipboard.v1`'s |
| `docs/architecture/OVERVIEW.md` | +14: capability-document index, and the "defined but not implemented" state |
| `docs/adr/README.md` | +2 index rows |
| `desktop/Cargo.lock` | +1 line (`prost-types` added to `anyflow-proto`'s dev deps; no new crate version entered the tree) |

`AndroidManifest.xml` is **unchanged** (§16 below).

---

## 3. Protobuf additions

One new file. **No existing `.proto` was touched**, and the new one imports
nothing — asserted by
`notifications_schema::the_new_schema_changes_no_existing_file`.

### `NotificationRoles`

| # | Type | Field | Limit / semantic | Security relevance |
| --- | --- | --- | --- | --- |
| 1 | `repeated NotificationRole` | `roles` | Complete set, not a delta. Empty is meaningful | **Not an authorization input.** A peer's claim about itself; unknown values ignored, never assumed granted |
| 2 | `uint32` | `epoch` | Strictly monotonic per connection, from 1. 0 = unset, refused | Stops a replayed announcement re-widening a narrowed set — the revocation-safety property |

### `NotificationUpsert` — posted *and* updated, one idempotent message

| # | Type | Field | Limit / semantic | Security relevance |
| --- | --- | --- | --- | --- |
| 1 | `bytes` | `notification_id` | **exactly 16** | Opaque, derived, never the raw Android key. Bad width → refused *and not answered* |
| 2 | `string` | `origin_device_id` | 32 lowercase hex | Display only. **Never identity** — the pinned fingerprint is |
| 3 | `string` | `app_id` | ≤ 255 B | Attacker-controlled; sanitized before display. The one identifier deliberately *not* hidden |
| 4 | `string` | `app_label` | ≤ 128 B | Attacker-controlled; reaches a terminal and a popup (T17) |
| 5 | `string` | `title` | ≤ 512 B | Sensitive user content. Never logged |
| 6 | `string` | `body` | ≤ 4096 B | Sensitive user content. Never logged, never persisted |
| 7 | `int64` | `posted_at_unix_ms` | Informational only | Never an authorization, ordering or expiry input |
| 8 | `NotificationImportance` | `importance` | 4 values | Presentation only. Unknown → `NORMAL`, never louder |
| 9 | `NotificationPrivacy` | `privacy` | 4 values | A **hint, not an ACL**. Unset → `PRIVATE`; unknown → `SECRET` |
| 10 | `NotificationCategory` | `category` | 8 values | Presentation only; an app's own claim, never a boundary |
| 11 | `Progress` | `progress` | Absent unless real | Rate-limiting input |
| 12 | `bool` | `ongoing` | — | Source-side dismissal refusal input |
| 13 | `bool` | `dismissible` | — | The source decides; the sink may only ask |
| 14 | `bytes` | `group_id` | **exactly 8**, or absent | Digest, not the platform group string |
| 15 | `bool` | `group_summary` | — | Presentation |
| 16 | `bool` | `secondary_profile` | — | A boolean, **not** the numeric Android user id — that is a fingerprinting surface |
| 17 | `bool` | `redacted` | — | The source reduced this. Not set for a length truncation |
| 18 | `bytes` | `content_hash` | **exactly 32**, or absent | De-dup and loop detection. **Not authentication, not identity** |

### `NotificationRemove` / `DismissRequest`

Both: `1 bytes notification_id` (exactly 16), `2 string origin_device_id`.

`NotificationRemove` carries **no reason code** — all 23 Android removal reasons
mean the same thing to a mirror, and shipping the reason would leak facts about
the user's device.

`DismissRequest` is the only message that travels sink → source and causes an
effect there. It has **no field capable of carrying an action, an intent, a
reply or a payload**, and that is asserted structurally (§11 below).

### `NotificationResult`, `SyncMarker`, `NotificationControl`

| Message | Fields |
| --- | --- |
| `NotificationResult` | `1 bytes notification_id`, `2 NotificationOutcome outcome` (15 values, all safe to send to a peer) |
| `SyncMarker` | `1 bytes sync_id` (exactly 16), `2 Phase phase` (`UNSPECIFIED`/`BEGIN`/`END`) |
| `NotificationControl` | `oneof body`: `1 roles`, `2 upsert`, `3 remove`, `4 dismiss`, `5 result`, `6 sync`. Six bodies, no seventh, no escape hatch |

**Whole message ≤ 8 KiB encoded**, well under `MAX_FRAME_LEN` (64 KiB) — the
same deliberate headroom `clipboard.v1` reserves.

---

## 4. Capability registry

The repository has **no central registry of capability id strings**: the
registry is `CapabilityRegistry`, keyed by whatever implementations are
registered, and each capability crate exports its own `CAPABILITY_ID` constant
(`anyflow-capability-clipboard` line 58, and the Kotlin `companion object`
`ID`). No parallel registry was invented.

`notifications.v1` follows that convention, with the id placed in
`anyflow_core::notifications::CAPABILITY_ID` because the capability *crate*
(`desktop/capabilities/notifications/`) is an N2 deliverable and does not exist
yet. N2's crate re-exports it.

**Nothing registers it.** `CapabilityRegistry::builder().build()` does not
support it, `advertised()` does not contain it, and it therefore never reaches a
`HELLO`. Asserted on both sides:

* `nothing_advertises_notifications_v1_after_n0` (Rust)
* `notifications v1 is not advertised after N0` (Kotlin)

Unknown-capability behaviour is unchanged: an un-negotiated id still gets a
**non-fatal** `ERROR{UNSUPPORTED_CAPABILITY}` and the session survives
(`desktop/core/src/session.rs`, unmodified).

---

## 5. Role model

`anyflow_core::notifications::{Role, PeerRoles, RolesRejection}`.

* `PeerRoles::default()` is **empty** — absent roles mean no roles, fail closed.
* `apply()` replaces rather than merges: the announcement is the complete set,
  so narrowing is simply not naming a role.
* Epoch strictly monotonic; `0` refused as unset; `<=` refused as stale.
* Unknown role values are filtered out, and the rest of the announcement still
  applies.
* There is **no method that consults or writes a grant**, which is how
  "a role is never an authorization input" is enforced.

Ten tests cover role add, update, removal-by-omission, empty announcement,
stale epoch (both `<` and `==`), epoch 0, unknown future role, and the
capability-without-usable-role case.

Roles are **not** in `HELLO`. `HELLO` is unchanged.

---

## 6. Identity representation

`anyflow_core::notifications::NotificationId` — a newtype over `[u8; 16]`,
constructible only from exactly 16 bytes, opaque, `Ord`/`Hash` so it can key a
map, and with no accessor that interprets its contents.

**The HMAC derivation is not implemented here, deliberately.** It needs the
source device's `device_notification_secret`, which exists only on the source
platform, so it ships in N1 with the Android listener. N0 implements the
protocol type, the exact width, the validation, the refusal semantics and the
tests — and manufactures no Android implementation in portable code. The
boundary is stated in the module doc, in ADR-0016 §12, and in
`docs/architecture/NOTIFICATIONS.md`.

The raw `StatusBarNotification` key is **never transmitted**: there is no schema
field that could hold it, and `sbn`, `status_bar`, `platform_key`, `raw_key`,
`uid` and `user_id` are all on the prohibited-substring list the schema test
enforces.

`origin_device_id` is validated as 32 lowercase hex but is **never trusted as
identity** — `origin_device_id_is_not_identity` proves that two upserts differing
only in a claimed origin resolve to the same identity, so a lying peer gains
nothing, and the sink is documented to key on
`(peer_fingerprint, notification_id)`.

---

## 7. Event model

Exactly the approved minimal model, and no duplicate platform-specific types:

| Concept | Message |
| --- | --- |
| Posted **and** updated | `NotificationUpsert` — one idempotent upsert |
| Removal | `NotificationRemove` |
| Dismissal request | `DismissRequest` |
| Verdict | `NotificationResult` |
| Snapshot begin / end | `SyncMarker{BEGIN}` / `{END}` |
| Snapshot item | An ordinary `NotificationUpsert` inside the bracket |

No Android `Notification`, `StatusBarNotification`, `RemoteViews` or
`PendingIntent` is serialized, and no field could hold one.

---

## 8. Snapshot implementation

`anyflow_core::notifications::{Snapshot, SnapshotStep, SnapshotRejection}` — the
receiving half of the bracket. It holds **identities only**: there is no field
on the type that could hold notification text, asserted by
`the_snapshot_holds_no_content`, which formats the state and checks no fixture
string appears.

| Malformed input | Behaviour | Test |
| --- | --- | --- |
| `END` without `BEGIN` | Refused. Never read as "remove everything" | `end_without_begin_is_refused` |
| Nested `BEGIN` | The open snapshot is **abandoned** without applying; a new one opens. Its later `END` cannot complete anything | `a_nested_begin_abandons_the_open_snapshot_without_applying_it` |
| `END` with a mismatched `sync_id` | Refused; the open snapshot survives and only its own `END` can complete it | `a_mismatched_sync_id_cannot_complete_a_snapshot` |
| Duplicate item | Idempotent (set semantics) | `a_duplicate_item_inside_a_snapshot_is_idempotent` |
| Interrupted / abandoned snapshot | Removes nothing, ever | `an_abandoned_snapshot_never_becomes_authoritative` |
| Unknown future `Phase` | Ignored; cannot complete or abandon | `an_unspecified_phase_cannot_complete_a_snapshot` |
| Upserts **outside** a snapshot | **Allowed** — that is the ordinary live path, not an error | `upserts_outside_a_snapshot_are_live_traffic` |

**The timers are not protocol.** `Snapshot` deliberately has no timer: the
reconnect grace (60 s), the snapshot abandon timeout (30 s),
`MAX_SNAPSHOT_ENTRIES` (100) and `MAX_MIRRORS_PER_PEER` (200) are sink- or
source-local tunables for later waves, per the approved reclassification
([02 §7.5](docs/research/notifications-v1/02-PROTOCOL-AND-EVENT-MODEL.md)). Only
the bracketing and the remove-what-is-not-named rule are on the wire.

Reconnect needs no history: a snapshot is just upserts, and derived ids let the
sink reconcile in place.

---

## 9. Dismissal primitive

Two scalar fields. Idempotent by vocabulary — `UNKNOWN_NOTIFICATION`,
`DUPLICATE` and `NOT_DISMISSIBLE` are answers rather than errors, so a
well-behaved implementation has no reason to retry into a loop.

The "no remote execution path" property is asserted **negatively and
structurally**, not by inspection:

* `the_dismiss_primitive_has_no_remote_execution_path` (descriptors) — exactly
  two fields, of exactly the right types, with no nested type, no nested enum
  and no `oneof`;
* `no_field_name_hints_at_a_prohibited_capability` — 36 banned substrings across
  the whole schema, including `action`, `intent`, `pending`, `invoke`,
  `execute`, `reply`, `remote_input`;
* `the_only_bytes_fields_are_fixed_width_identifiers` — the check a rename
  cannot defeat;
* `the_control_envelope_has_no_escape_hatch` — six bodies, all messages defined
  in this schema, all inside the `oneof`.

The runtime (reason-2 filtering, echo suppression, `cancelNotification`) remains
N4.

---

## 10. Field limits

Implemented in `anyflow_core::notifications`, tested to the boundary:

| Field | Limit | Exact max accepted | Max + 1 refused |
| --- | --- | --- | --- |
| `notification_id` | **exactly 16 B** | ✅ | ✅ (0, 1, 8, 15, 17, 32 all refused) |
| `sync_id` | exactly 16 B | ✅ | ✅ |
| `group_id` | exactly 8 B, or absent | ✅ | ✅ |
| `content_hash` | exactly 32 B, or absent | ✅ | ✅ |
| `origin_device_id` | 32 lowercase hex | ✅ | ✅ (`MalformedDeviceId`) |
| `app_id` | 255 B | ✅ | ✅ |
| `app_label` | 128 B | ✅ | ✅ |
| `title` | 512 B | ✅ | ✅ |
| `body` | 4096 B | ✅ | ✅ |
| Whole message | 8 KiB | ✅ | ✅ (`MessageTooLarge`, checked before any field) |

Also covered: limits are **bytes not characters** (a 4-byte emoji fixture hits
the boundary exactly); invalid UTF-8 never decodes at all (protobuf refuses,
pinned by a hand-built malformed frame); NUL is refused in every string field;
and `identifiers_are_never_truncated_into_collisions` proves two ids sharing a
16-byte prefix are both refused rather than silently merged.

**Truncation semantics are source-side**, as approved: the source truncates
display text on a UTF-8 boundary, the receiver refuses with `TOO_LARGE` and
never repairs. Identifiers are never truncated — that is how collisions are
manufactured. A bad-width `notification_id` is additionally refused **and not
answered** (`Rejection::is_answerable`), because a `NotificationResult` echoes
the id and there would be nothing to correlate a reply with.

---

## 11. Field-set assertion test

`desktop/proto/tests/notifications_schema.rs` — **12 tests, descriptor-based,
not a grep.** It compiles the schema with `protox`, the same compiler
`build.rs` uses, and asserts against `FileDescriptorSet`. A rename, a comment
or a field split across lines cannot hide from it.

It guards: the exact message set; the exact field set of every message; the
33-entry prohibited-substring list; the `bytes`-fields rule; the no-open-ended-
container rule (no `Any`, no map, no foreign type); the dismiss primitive; the
control envelope; every field **number**; the absence of reservations and
number reuse; and every enum vocabulary.

**The guard was verified to actually bite.** Adding
`bytes pending_intent = 3; string reply_text = 4;` to `DismissRequest` failed
four tests with actionable messages, e.g.:

```text
DismissRequest.pending_intent is an unbounded `bytes` field. notifications.v1
carries no binary payload: the only `bytes` fields are the fixed-width
identifiers ["notification_id", "sync_id", "group_id", "content_hash"], each
validated to an exact length.
```

The probe was reverted and the suite is green.

### Ordering (brief §11) — checked, not asserted

The approved design assumes in-order delivery. That assumption was **located in
the transport and proved**, not restated. In `desktop/core/src/session.rs`:

1. TLS 1.3 over TCP is an ordered byte stream.
2. One reader task parses frames sequentially into a bounded FIFO channel, and
   the dispatch loop **awaits** `on_message` before taking the next message, so
   two messages for one capability are never in flight.
3. Exactly one writer task owns the write half and the envelope factory and
   drains a single FIFO channel for capability output.

Point 3 was already pinned by `replies_keep_their_order_under_queue_pressure`.
Point 2 was not, so N0 adds
`inbound_capability_messages_reach_the_handler_in_wire_order` — 64 frames, a
`yield_now()` in the handler to give a concurrent dispatcher every chance to
interleave, and an assertion that the handler observed wire order. The added
capability is named `record.v1`, so **no capability name enters transport
code**.

**No BLOCKER.** One constraint this places on later waves is recorded in
`NOTIFICATIONS.md`: the outbound channel's sender is cloned, so FIFO is
guaranteed *per producer*. N1/N2 must emit notification traffic from a single
ordered producer, which is also why per-identity coalescing is a slot rather
than a queue. No hidden sequence-number architecture was added.

---

## 12. Compatibility tests

| Property | Proved by |
| --- | --- |
| Old peer without `notifications.v1` → battery/files/clipboard unaffected | `negotiation_semantics_are_unchanged_for_existing_capabilities`; the full 426-test suite green with no behavioural change; `session.rs` production code untouched |
| Negotiated only if both sides support it | `CapabilityRegistry::negotiate` is unmodified; the new id is absent from the intersection on both sides |
| Capability present but roles absent → no direction enabled | `a_peer_with_the_capability_but_no_role_enables_no_direction`, `absent_roles_mean_no_roles` |
| Unknown message / field follows existing forward-compatibility rules | `an unknown field does not break decoding`, `an unknown control body is ignored not fatal`, `an unknown role is unrecognized and never granted`, `an unknown privacy value decodes as unrecognized rather than public` (Kotlin); `unknown_enum_values_resolve_conservatively`, `an_unspecified_phase_cannot_complete_a_snapshot` (Rust) |
| No forced reconnect or version failure for legacy peers | No `HELLO` change, no envelope change, no `core.proto` change, no protocol version bump. The new schema imports nothing (`the_new_schema_changes_no_existing_file`) |

---

## 13. Codegen commands

Both toolchains compile **the same file**, from the same directory, and neither
generated output is committed — Rust's goes to `OUT_DIR`, Android's to
`app/build/generated/`.

**Rust** — `protox`, driven by `desktop/proto/build.rs`; the only change is one
line adding the new path to the `files` array:

```console
$ cd desktop && cargo build -p anyflow-proto --locked
```

**Android** — the protobuf Gradle plugin, which already globs
`../../protocol/proto` as a source directory, so **no build file needed
editing**:

```console
$ cd android
$ export JAVA_HOME=$HOME/.local/jdk/jdk-21.0.12.1+1 ANDROID_HOME=$HOME/Android/Sdk
$ ./gradlew :app:testDebugUnitTest --offline    # runs generateDebugProto
```

Nothing was hand-edited. Generated churn: **none possible** — no generated file
is tracked. The generated surface was inspected and contains exactly the 8
messages and 6 enums of the new schema (Rust: `anyflow.v1.capabilities.rs`;
Android: `…/proto/capabilities/Notification*.java`, `DismissRequest.java`,
`Progress.java`, `SyncMarker.java`). No existing generated type changed, because
no existing `.proto` changed.

**Cross-language lockstep** goes further than "both compile it": both sides
assert the same 192-byte canonical encoding of a fully-populated upsert, byte
for byte (`CANONICAL_UPSERT_HEX` / `canonicalUpsertHex`). A field number, wire
type or enum value that drifted on one side fails a test rather than a user.

---

## 14. Rust regression

Run sequentially, from `desktop/`, with the session-local GTK4 devel prefix
sourced (`. ~/.local/gtk4-prefix/ENV.sh` — `anyflow-gui` needs `gtk4.pc`, which
is not installed system-wide on this machine; a pre-existing environment
condition, unrelated to N0).

```console
$ cargo fmt --check                                              # rc=0
$ cargo build --workspace --locked                               # rc=0
$ cargo test --workspace --locked                                # rc=0
$ cargo clippy --workspace --all-targets --locked -- -D warnings # rc=0
```

| | Baseline | After N0 | Delta |
| --- | --- | --- | --- |
| Passed | 371 | **426** | **+55** |
| Failed | 0 | **0** | — |
| Ignored | 9 | **9** | — |

The 55 new tests: 42 in `core/tests/notifications_protocol.rs`, 12 in
`proto/tests/notifications_schema.rs`, 1 in `core/src/session.rs`
(`dispatch_tests`).

Targeted runs:

```console
$ cargo test -p anyflow-core --test notifications_protocol   # 42 passed
$ cargo test -p anyflow-proto --test notifications_schema    # 12 passed
$ cargo test -p anyflow-core --lib session::dispatch_tests   #  6 passed
```

Two clippy findings were raised by the new test code (`type_complexity` on two
function-pointer case tables) and fixed with `SetText` / `SetProbe` aliases. No
production lint was relaxed; `unsafe_code = "forbid"` is untouched and the new
module contains no `unsafe`.

---

## 15. Android regression

```console
$ cd android && ./gradlew :app:testDebugUnitTest --offline
BUILD SUCCESSFUL
```

| | Baseline | After N0 | Delta |
| --- | --- | --- | --- |
| Tests | 243 | **261** | **+18** |
| Failures | 0 | **0** | — |
| Errors | 0 | **0** | — |
| Skipped | 0 | **0** | — |

JDK 21 (`~/.local/jdk/jdk-21.0.12.1+1`) and SDK 35, per the repository's build
configuration. `connectedDebugAndroidTest` was **not** run and is not needed:
no Android production feature is activated, and the new tests are pure JVM
protobuf assertions.

The Android bindings compile from the same schema as the Rust ones, and the
Kotlin `lite` runtime produces byte-identical output for the shared vector.

---

## 16. Manifest guard

```console
$ git diff -- android/app/src/main/AndroidManifest.xml
(empty)
```

**Unchanged, as required.** The only occurrence of the string in the manifest is
still the "Deliberately absent" comment at line 33:

```xml
* no BIND_NOTIFICATION_LISTENER_SERVICE
```

No `<service>`, no `BIND_NOTIFICATION_LISTENER_SERVICE`, no metadata. The
manifest change ADR-0015 authorises is an **N1** deliverable.

### `NotificationSink` — the seam decision (brief §16)

**Resolved to (B): N2 owns it. N0 did not create it.**

This brief invites N0 to consider creating the seam. The merged implementation
plan — which the brief itself names as the authority — places it explicitly with
the first Linux implementation:

> **N2** — *Backend seam* | `desktop/capabilities/notifications/src/backend/mod.rs` — `NotificationSink` trait, mirroring `ClipboardBackend`
> — [05-IMPLEMENTATION-PLAN.md](docs/research/notifications-v1/05-IMPLEMENTATION-PLAN.md)

N0's own deliverable table in the same plan lists the schema, three ADRs, the
architecture document and the doc updates, and nothing else. An abstraction with
no implementation on either side of it is a guess about a shape, and the shape
is exactly what writing the first D-Bus sink will teach.

The future location, the trait sketch and the constraints on it (pure portable
abstraction, zero Linux imports, no D-Bus, no GTK, and `portable_boundary.rs`
must list the new crate) are documented in
`docs/architecture/NOTIFICATIONS.md` §"The platform seams", so N2 does not have
to rediscover them.

---

## 17. Security regression

Unchanged, verified by `git diff` on each file:

| Property | File | State |
| --- | --- | --- |
| TLS 1.3, SPKI pinning | `desktop/core/src/tls.rs` | **Unchanged** |
| Pairing | `desktop/core/src/pairing.rs` | **Unchanged** |
| Identity, no silent regeneration | `desktop/core/src/identity.rs` | **Unchanged** |
| Peer grants, revocation, trust store | `desktop/core/src/store.rs` | **Unchanged** |
| Fingerprints | `desktop/core/src/fingerprint.rs` | **Unchanged** |
| Secret store | `desktop/core/src/secret_store.rs` | **Unchanged** |
| `files.v1` challenge / HMAC | `desktop/capabilities/files/src/lib.rs` | **Unchanged** |
| Clipboard policy, no persistence | `desktop/capabilities/clipboard/src/lib.rs` | **Unchanged** |
| Android manifest | `AndroidManifest.xml` | **Unchanged** |

The only production edits are two `+1` lines (a module declaration and a
`.proto` path), a dev-dependency block, and two new files that no code calls.
The `session.rs` change is `+114/-0`, every line inside `mod dispatch_tests`.

**No notification content persistence was introduced:**

* `desktop/core/src/notifications.rs` contains **no** `tracing::`, `log::`,
  `println!`, `eprintln!` or `dbg!` — verified by grep;
* it derives **no** `Serialize`/`Deserialize` and imports no `serde`;
* it performs **no** file or I/O operation (`std::fs`, `File`, `OpenOptions`:
  none);
* no notification field reaches `state.json` — `store.rs` is untouched and
  mentions nothing notification-related;
* no history structure exists anywhere: the only stateful type, `Snapshot`,
  holds `BTreeSet<NotificationId>` and a `sync_id`, and
  `the_snapshot_holds_no_content` asserts its `Debug` output contains no fixture
  text;
* rejections carry a field *name* and a byte *count*, never a value, so a
  rejection cannot be logged into a leak.

**No fixture body is logged.** No test in any of the three new files prints
anything. Fixture text is obviously synthetic throughout: `FIXTURE TITLE`,
`FIXTURE BODY`, `Fixture App`, `example.fixture.app`, and a device id of
`0123456789abcdef…`. Nothing resembles a real notification, an OTP, or a
credential.

Per POC-NOTIF-01, **no rule anywhere in N0 assumes platform OTP redaction.**
The schema comments, ADR-0016, ADR-0017 and `NOTIFICATIONS.md` each state that
notification content is potentially fully sensitive user data and that platform
redaction is a bonus, never a control. Application filtering and lock policy are
represented (`privacy`, `redacted`, `secondary_profile`) but **not implemented**
— they are N3.

---

## 18. Protocol diff audit

| Check | Result |
| --- | --- |
| No **removed** field | ✅ Nothing existed to remove; nothing reserved (`no_field_number_has_been_retired_or_reused`) |
| No **renumbered** field | ✅ Every number pinned by `field_numbers_are_pinned` |
| No **reused** field number | ✅ Duplicate-number check in the same test |
| No **incompatible type replacement** | ✅ No existing `.proto` was edited at all |
| No envelope / `core.proto` change | ✅ Neither file appears in `git status` |
| No global protocol version bump | ✅ `PROTOCOL_VERSION_MIN`/`MAX` remain 1 |
| Additive only | ✅ One new file that imports nothing and is imported by nothing |
| Old peers unaffected | ✅ The id is never advertised, so it is never negotiated |

Full message-by-message field table: §3 above, and
`docs/architecture/PROTOCOL.md` §`notifications.v1`.

---

## 19. ADR-0016 summary

**`notifications.v1` opaque notification identity** — Accepted.

The transmitted identity is `HMAC-SHA256(device_notification_secret,
"anyflow/notifications.v1/id/v1" || len32(key) || key)[0..16]`. The raw Android
key is never transmitted: it is `userId|pkg|id|tag|uid`, three of whose five
components would arrive at the desktop for every notification of every app for
ever, having never had a destination-side purpose.

Captured: why derived rather than random (a process restart would otherwise
turn a reconnect into a wall of duplicates); the 128-bit truncation rationale
(the id authenticates nothing, and the live population is capped); the secret's
lifetime and the rule that it is **destroyed and regenerated on identity reset
or re-pair**, because otherwise a peer could link the two identities across the
event re-pairing exists to break; that a regenerated secret is a **mirror
reset** reconciled by the ordinary snapshot, never something to map across; that
the sink namespaces by the authenticated pinned fingerprint and never by a
claimed `origin_device_id`; the fail-closed `UNKNOWN_NOTIFICATION` answer for an
unmappable id and why it is not an oracle; the source-side
`notification_id → platform key` map that dismissal needs and that is
reconstructible precisely because the id is derived; what the id hides (`userId`,
`uid`, `id`, `tag`) and what it deliberately does not (**the package name**, via
`app_id`); and that the derivation itself belongs to N1.

It does not duplicate ADR-0015: that ADR decides whether AnyFlow may read
notifications at all; this one decides what one is called once it may.

---

## 20. ADR-0017 summary

**Runtime-narrowable capability roles** — Accepted.

Recorded as an ADR because it is a *pattern*, not a feature: the first time a
capability negotiates anything of its own.

Captured: why roles are not in `HELLO` (it is sent once, it is a flat list of
ids, and it is all-or-nothing per capability — so it cannot express a permission
revoked at 14:32 on a session established at 09:00, and dropping the capability
entirely would also stop the phone accepting dismissals it can still honour);
the rejected "four capability ids" alternative, which ADR-0008 already rejected
for clipboard and which additionally cannot narrow without a reconnect; the
epoch/generation mechanism and the exact failure it prevents; that narrowing and
widening both take effect immediately, and why neither needs a grant's reconnect
discipline; that absent roles mean none; that unknown roles are ignored rather
than assumed; and that a role mismatch answers `REJECTED_ROLE` without closing
the session.

The clause the rest of the ADR protects, tabulated in §6:

> **platform capability ≠ AnyFlow peer grant ≠ role.**

Three different questions, three different deciders, three different blast
radii. A peer cannot widen its own grants by claiming a role; holding the OS
permission grants nothing to any peer; holding a peer grant manufactures no
platform capability.

---

## 21. Architecture document

`docs/architecture/NOTIFICATIONS.md`, linked from `OVERVIEW.md` through a new
capability-document index that marks it **"Protocol only — nothing mirrors a
notification yet"**.

It opens with a what-exists-after-N0 table that says "does not exist" nine
times, and every later section is marked with its owning wave. There are no
implementation claims for code that does not exist.

Covers: the end-to-end shape with each box labelled by wave; ownership
boundaries; the event lifecycle and sink state machine; the role lifecycle
including the revocation sequence; the privacy boundary (reduction happens at
the **source**, before encoding, so withheld content never enters the wire) and
the three non-negotiable rules; the snapshot model with the active-state /
history table and the protocol-vs-tunable distinction; the ordering guarantee
with the three transport points, the tests that pin them and the single-producer
constraint it places on N1/N2; the dismissal model; the platform seams,
including the explicit statement that `NotificationSink` is N2's and its future
location; cross-language lockstep; and the N0-N6 wave boundaries.

---

## 22. Remaining risks and debts

| # | Item | Severity | Notes |
| --- | --- | --- | --- |
| 1 | The wire contract lives in `anyflow-core`, not in the (not-yet-existing) capability crate | **Low — a documented deviation** | The plan's N2 table lists `src/limits.rs`, `src/identity.rs` inside `desktop/capabilities/notifications/`. N0 needs a home for the contract the brief's tests exercise, and the crate is N2's. It sits beside `clipboard_policy.rs` — the existing precedent for a portable capability-adjacent type in core, which the plan itself follows again for `notification_policy.rs`. N2 re-exports rather than reimplements. Moving it later is a rename, not a wire change |
| 2 | Single-ordered-producer constraint on outbound notification traffic | **Low** | The outbound `mpsc` sender is cloned, so FIFO is per-producer. Documented in `NOTIFICATIONS.md`; N1/N2 must emit from one task or one lock. Not a transport defect |
| 3 | The HMAC derivation is unimplemented and untested end to end | **Expected** | N1, by design (ADR-0016 §12). N0 pins the type, the width and the refusal semantics only |
| 4 | No end-to-end test drives `notifications.v1` over a real session | **Expected** | Nothing implements the capability. `desktop/daemon/tests/notifications.rs` is an N2 deliverable |
| 5 | `desktop/gui` needs a session-local GTK4 devel prefix on this machine | **Environment, pre-existing** | `gtk4-devel`/`libadwaita-devel`/`glib2-devel` are not installed system-wide; `. ~/.local/gtk4-prefix/ENV.sh` is required before `cargo build --workspace`. Unrelated to N0, but it will bite the next person |
| 6 | The prohibited-substring list is a heuristic layer | **Low** | It is the weakest of the four schema guards and could be defeated by a field named `x`. The `bytes`-fields rule and the exact field-set assertions are the ones that cannot be, and they run alongside it |
| 7 | The nested-`BEGIN` rule was chosen, not specified | **Low** | The merged spec does not say what a second `BEGIN` means. N0 implements abandon-and-restart because an incomplete snapshot must never remove anything. Documented and tested; if N5 disagrees it is a one-line change with no wire impact |

**No BLOCKER. No P0.**

---

## 23. Git status

```console
$ git status --porcelain
 M desktop/Cargo.lock
 M desktop/core/src/lib.rs
 M desktop/core/src/session.rs
 M desktop/proto/Cargo.toml
 M desktop/proto/build.rs
 M docs/adr/README.md
 M docs/architecture/OVERVIEW.md
 M docs/architecture/PROTOCOL.md
?? android/app/src/test/java/io/github/yurisismotto/anyflow/NotificationsProtocolTest.kt
?? desktop/core/src/notifications.rs
?? desktop/core/tests/notifications_protocol.rs
?? desktop/proto/tests/
?? docs/adr/ADR-0016-notification-identity.md
?? docs/adr/ADR-0017-capability-roles.md
?? docs/architecture/NOTIFICATIONS.md
?? protocol/proto/anyflow/v1/capabilities/notifications_v1.proto

$ git diff --check
(clean)

$ git diff --stat
 desktop/Cargo.lock            |   1 +
 desktop/core/src/lib.rs       |   1 +
 desktop/core/src/session.rs   | 114 ++++++++++++++++++++++++++++++++++++++++++
 desktop/proto/Cargo.toml      |   8 +++
 desktop/proto/build.rs        |   1 +
 docs/adr/README.md            |   2 +
 docs/architecture/OVERVIEW.md |  14 ++++++
 docs/architecture/PROTOCOL.md |  66 ++++++++++++++++++++++++
 8 files changed, 207 insertions(+)
```

**Audit.** Every changed and untracked path is a text file. No secrets, no keys,
no certificates, no `.apk`/`.aab`, no `state.json`, no identity file, no log, no
capture, no build output, no `target/`. This report file is the sixteenth
addition and is itself plain text.

**Nothing was staged, committed, pushed, or turned into a PR.**

---

## 24. Recommended commit message

```text
feat(protocol): define notifications.v1 capability schema and contracts

Adds the notifications.v1 wire protocol, the portable contract that
validates it, and the architecture records behind both. No platform
listener, no sink, no UI, no runtime: nothing registers the capability,
so it never reaches a HELLO and no peer can tell it exists.

Protocol (additive; no envelope, core.proto or version change):
  * notifications_v1.proto — six control bodies, eight messages, six
    enums (five top-level plus SyncMarker.Phase), imported by nothing and importing nothing
  * roles inside the capability rather than in HELLO, with a strictly
    monotonic epoch so a replayed announcement cannot re-widen a set
    that has already narrowed
  * one idempotent upsert for posted-and-updated; removal without a
    reason code; a dismiss primitive with no field capable of carrying
    an action, an intent or a reply
  * BEGIN/END snapshot bracketing for active state; no history exists
    anywhere in the design
  * 16-byte opaque notification identity; the raw Android key is never
    transmitted

Core:
  * anyflow_core::notifications — limits, validation, role/epoch
    reduction, snapshot state machine. Pure functions of decoded
    messages: no I/O, no timers, no policy, no platform, no logging,
    and no field that could hold notification content

Tests (+55 Rust, +18 Android JVM):
  * descriptor-based field-set regression that fails if an action,
    reply, blob, history or media field appears
  * both toolchains assert the same canonical encoding byte for byte
  * proves inbound capability messages reach a handler in wire order,
    the ordering assumption the snapshot design depends on

Docs: ADR-0016 (identity), ADR-0017 (runtime-narrowable roles),
docs/architecture/NOTIFICATIONS.md, PROTOCOL.md and OVERVIEW.md.

AndroidManifest.xml is unchanged. NotificationSink stays with its first
implementation in N2, per the merged implementation plan.
```

---

## 25. Recommendation for N1

**Proceed to N1 — the Android source adapter.** Nothing in N0 blocks it, and
N1 and N2 are independent from here, sharing only the `.proto`.

N1 starts by making the identity derivation real, because it is the only part of
the contract N0 could not implement and everything else in the wave depends on
it:

1. **`device_notification_secret`** — 32 CSPRNG bytes in the existing encrypted
   store, with the ADR-0016 §5 lifecycle wired in from the start. In
   particular, **destroy and regenerate it on identity reset or re-pair**; that
   rule is invisible in the happy path and expensive to retrofit.
2. **The derivation and the id map**, with a cross-language test vector added
   beside the canonical encoding vector this wave established, so the Rust and
   Kotlin derivations are checked against each other rather than each against
   itself.
3. **The listener**, `META_DATA_DEFAULT_AUTOBIND=false`, bound on demand and
   only while a granted peer is connected. Drop AnyFlow's own package **first**,
   before every other check — the foreground-service notification exists on
   every running install and is the shortest path to a mirror-of-a-mirror loop.
4. **Callbacks run on the main thread.** Extract a plain data object and hand it
   to a coroutine; no protobuf, no hashing, no I/O on that thread.
5. **Emit from a single ordered producer** (debt #2). The snapshot semantics N0
   pinned depend on a removal never overtaking the upsert it refers to.

Two things to carry into review, because they are easy to lose:

* `allow_dismiss_sync` defaults **off** (ADR-0015 §6), reversing the research
  recommendation. It is easy to implement from memory of the older text.
* The app filter is **deny-by-default** (ADR-0015 §5), and no OTP, banking or
  2FA heuristic is a security boundary — POC-NOTIF-01 showed the platform's own
  redaction does not fire on the certification target, which is an argument
  *for* deny-by-default, not against it.

N1's exit gate is the honest one already written in the plan: with no peer
granted, `dumpsys notification` shows the listener **not bound**, and no
notification content appears in logcat at any level.

---

**NOTIFICATIONS.V1 N0 PASS**
**N1 READY FOR IMPLEMENTATION**
