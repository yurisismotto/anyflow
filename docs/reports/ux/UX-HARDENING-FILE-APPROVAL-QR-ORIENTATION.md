# AnyFlow — UX Hardening

## Incoming File Approval + Pairing Scanner Orientation

Branch: `fix/ux-hardening-file-approval-qr-orientation`
Date: 2026-09-16
Host: Fedora 44, Linux 7.1.9-200.fc44.x86_64
Hardware under test: Samsung SM-X620 tablet, Android 16 (API 36), One UI 8.0

---

## 1. Scope

Two UX debts identified during U2 certification, and nothing else.

| Id | Defect | Outcome |
| --- | --- | --- |
| UX-1 | A desktop with a graphical session had no way to approve an incoming file, so certification had to run the daemon with `--accept-files-without-asking` | **PASS** |
| UX-2 | ANDROID-UX-ORIENTATION-01 — the pairing QR scanner forced landscape | **PASS** |

Not implemented, and verified absent from the diff in §27: branding, logo, palette,
Quick Panel, KDE StatusNotifier/tray, trust-store ordering cosmetics, Android lint
cleanup, RPM/DEB packaging, battery hotplug, notification queue recovery, concurrent
multi-peer sessions, protocol redesign.

No protobuf or wire change was needed. `files.v1` already expresses decline
(`FileCancel` with `DECLINED_BY_USER`) and that is what a Decline sends.

---

## 2. Baseline

Branch state at start, unchanged from the brief's expectation:

```
fix/ux-hardening-file-approval-qr-orientation
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
c0aac3a Merge pull request #30 from yurisismotto/docs/u2-test-ci-final-evidence
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` was neither read into the diff, modified, nor
staged. `git add .` was not used. No commit, push or PR was created.

---

## 3. UX-1 — defect of record

From `U2-HARDENING-P1-MULTIPEER.md` §408 and `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`
§1815: when the tablet offered a file to a normally-started desktop daemon, the
daemon refused, correctly and loudly:

```
WARN declining an incoming file: no way to ask a human. Start the
     daemon with --accept-files-without-asking to accept unattended.
```

To complete a transfer and validate routing and hashes, the daemon had to be
restarted with `--accept-files-without-asking`. That override is a legitimate
development and headless mode. It is not the graphical desktop experience.

---

## 4. UX-1 — root cause

**Exact root cause: `files.v1` had the approval seam but the desktop shipped no
implementation of it that could reach a person, and the daemon is a
`systemd --user` process with no terminal of its own.**

`anyflow-capability-files` defines the seam (`capabilities/files/src/lib.rs:75`):

```rust
#[async_trait::async_trait]
pub trait TransferApproval: Send + Sync {
    async fn confirm_receive(&self, offer: &IncomingOffer) -> bool;
}
```

`desktop/daemon/src/main.rs` had exactly two implementations of it, and the choice
between them was a command-line flag:

* `ConsoleApproval` — the default. Logged the line above and returned `false`. Its
  own doc comment named the gap: *"A desktop GUI and a `anyflow recv` command are
  the planned ways to answer; until one exists, `--accept-files-without-asking` is
  the documented escape hatch for a test rig."*
* `AcceptEverything` — selected by `--accept-files-without-asking`. Returned `true`.

So the missing thing was not a policy, a protocol message, or a security control.
It was a **rendezvous**: something that could carry the question from the daemon to
a graphical process and carry one boolean back. The GTK application was already a
client of the daemon's control socket, and the control socket already had exactly
the shape needed — `Request::Pair` opens a stream, the daemon pushes
`Event::ConfirmRequest`, and the client answers `Request::Confirm` on the same
connection. Nothing had been built on that precedent for files.

---

## 5. Existing files architecture (audited before changing anything)

Traced end to end. Nothing in this chain was altered except where §6 says so.

```
Android SendActivity (share sheet, EXTRA_STREAM)
  → authenticated control session (TLS 1.3, ALPN "anyflow/1", SPKI-pinned)
  → FileControl{Offer}                      capabilities/files/src/lib.rs on_offer()
      ├─ TransferId::from_bytes            malformed id → protocol error, no reply
      ├─ authorized(peer)                  FilesAuthorizer → trust store, fresh
      ├─ transfer-id reuse check           single-use ids
      ├─ validate_offer()                  sha256 len, filename len, mime len,
      │                                    filename::sanitize, max_file_bytes
      ├─ MAX_CONCURRENT_TRANSFERS_PER_PEER = 4
      └─ record inserted as WaitingAccept, then spawned:
  → await_approval()
      ├─ timeout(backstop, approval.confirm_receive(offer))   ← THE SEAM
      ├─ re-check authorized(peer) after the human
      ├─ destination.open_temp(id)
      ├─ StreamChallenge::generate()       acceptor role
      ├─ transition(Transferring)
      └─ FileControl{Accept} carrying the challenge
  → separate TLS data stream (ALPN "anyflow-data/1")
      ├─ DataStreamAuth: transfer_id + HMAC over the single-use challenge
      └─ raw bytes, exactly size_bytes, 64 KiB buffer
  → FileSink → sha256 over the whole file → promote to Downloads/AnyFlow
  → FileControl{Complete} to Android
```

Answers to the specific questions the brief asked:

| Question | Answer |
| --- | --- |
| Where is the `FileOffer` decoded? | `TransferManager::on_offer`, `capabilities/files/src/lib.rs` |
| Where are filename/size/identity validated? | `validate_offer` + `filename::sanitize`, before any resource is committed |
| Where does the daemon decide approval is needed? | Always. `await_approval` is spawned for every authorized offer; there is no bypass path |
| Where was `--accept-files-without-asking` checked? | `daemon/src/main.rs`, choosing which `TransferApproval` to construct |
| What abstraction represented "ask a human"? | `TransferApproval::confirm_receive` — already correct, with no implementation that could reach one |
| Did a GUI callback/channel exist? | No. This sprint adds one |
| Are daemon and GUI separate processes? | Yes |
| What IPC seam already existed? | `anyflow-control`: newline-delimited JSON over a Unix socket in `$XDG_RUNTIME_DIR`. `Pair`/`Confirm` was the precedent for a daemon-asks-client exchange |
| How is timeout/cancel/disconnect represented? | The reaper (`reap_once`, 1 s tick): `session.is_closed()` → `Transport`; `now >= deadline` → `TimedOut`; `!authorized` → `Revoked`. Plus `TransferState::can_transition_to` refusing illegal transitions |
| How does a decline reach Android? | `fail()` → `FileCancel{DECLINED_BY_USER}` (declines are cancellations, not failures) |
| What survives a restart? | Nothing. Transfers are in-memory for the daemon's life; nothing about a transfer is written to disk |
| How are concurrent offers handled? | Up to 4 per peer, each with its own spawned `await_approval`, so several prompts can be outstanding at once |

---

## 6. UX-1 — approval architecture

Three additions and no new data path. The daemon still depends on no toolkit; the
GUI still adds no protocol, capability or privilege of its own.

```
                     ┌──────────────── anyflowd ────────────────┐
  Android ── TLS ──▶  │ files.v1                                 │
                     │   on_offer → await_approval               │
                     │        │ confirm_receive(offer)           │
                     │        ▼                                  │
                     │   FileApproval  (runtime/src/approval.rs) │
                     │        │ pending: BTreeMap<TransferId,_>  │
                     │        │ provider: Option<mpsc::Sender>   │
                     │        ▼                                  │
                     │   control server                          │
                     │   run_file_approval_session()             │
                     └──────────────────┬───────────────────────┘
                                        │ $XDG_RUNTIME_DIR/anyflow/control.sock
                       WatchFileOffers  │  ndjson
                       FileOfferRequest │
                       FileOfferWithdrawn
                       FileDecision     │
                     ┌──────────────────┴───────────────────────┐
                     │ anyflow-gui                              │
                     │   client::watch_file_offers()            │
                     │   approval::install() → AdwAlertDialog   │
                     └──────────────────────────────────────────┘
```

### 6.1 `anyflow-runtime::approval::FileApproval` (new)

The rendezvous, and the only new abstraction. It implements `TransferApproval`.

* Pending offers are keyed by `TransferId` — 128 random bits, minted by the sender,
  single-use. A decision names one **exactly**, never a prefix.
* `attach()` registers a provider and returns an epoch plus a bounded receiver
  (`PROVIDER_QUEUE = 32`, eight peers' worth of simultaneous prompts).
  Attaching **replaces** any previous provider, for the reason `begin_pairing`
  replaces a pairing window: a desktop session can be restarted, and a stale
  attachment that refused the live one would leave the machine unable to accept
  files until the daemon was restarted. The displaced provider's pending questions
  are **dropped**, which reads as a decline.
* `detach(epoch)` checks the epoch first, so a session that was already displaced
  cannot unhook its replacement — the same mistake `unregister_session` avoids with
  session ids.
* `decide(id, accept)` removes the entry as it answers it, which is what makes a
  duplicate or reversed decision inert by construction rather than by a check
  someone has to remember.
* `withdraw(id)` removes without answering, for a transfer that ended under a
  prompt.
* `unattended: bool` holds `--accept-files-without-asking`. It is set once at
  startup and **cannot be changed by any control message**: an override a local
  client could switch on would not be an override, it would be a bypass.

A `PendingGuard` removes the entry on `Drop`. This is load-bearing: `files.v1`
wraps `confirm_receive` in a timeout, and a timeout *drops the future* rather than
telling it anything. Without the guard a provider could "answer" minutes later a
question the reaper had already closed.

### 6.2 `anyflow-control` (extended)

New requests, events and one report type. No existing type changed shape.

```rust
Request::WatchFileOffers                        // become the approval provider
Request::FileDecision { transfer, accept }      // answer one prompt, full hex id

Event::FileApprovalReady { unattended }         // attached; and whether it matters
Event::FileOfferRequest(FileOfferRequest)       // the question
Event::FileOfferWithdrawn { transfer_id, reason } // it stopped being answerable

pub struct FileOfferRequest {
    transfer_id, device_name, device_id,
    fingerprint, fingerprint_short,
    filename, size_bytes, mime_type,
}
```

There is no field on `FileOfferRequest` that could hold a byte of the file, the
stream challenge, or any part of the session's key material.

### 6.3 `anyflow-runtime::server::run_file_approval_session` (new)

Mirrors `run_pair_session`. It subscribes to the transfer event stream **before**
attaching, so a transfer that ends between the two cannot slip through
unwithdrawn, then selects over three sources:

* an offer from the seam → check the transfer is still live (`snapshot_one`), then
  emit `FileOfferRequest` and record the id in an `open` set;
* a line from the client → if it parses as `FileDecision`, apply it **only** if the
  id is in this session's `open` set. Anything else on the connection is ignored
  rather than answered, so a stray request cannot stand in for a decision;
* a terminal `TransferEvent` for an open prompt → `withdraw` + `FileOfferWithdrawn`.

On exit it `detach`es, which declines whatever is still pending.

The device name on a prompt comes from the **trust store**, keyed by the
authenticated fingerprint — never from the offer. A peer that renamed itself to
match another of your devices changes nothing about what the dialog says.

### 6.4 `anyflow-gui` (extended)

`client::watch_file_offers` holds one long-lived connection with a 2 s re-attach
loop, because the daemon is a user unit that can be restarted under a running
window and "silently stops asking" is indistinguishable, from the user's side,
from the defect this surface exists to fix. `approval::install` owns one
`AdwAlertDialog` per pending offer, keyed by transfer id.

Both the update callback and each dialog's response closure hold the handle
**weakly**. A strong reference either way would be a cycle — the handle would never
drop, the stream would never close, and the window would stay attached after it was
gone.

The handle is owned by the window, not by `build_window`'s scope: `build_window`
returns immediately, and a handle dropped there would detach the moment the window
appeared.

### 6.5 Two small corrections inside `files.v1`

Both are consequences of the prompt now taking *human* time, where before it
returned instantly in both directions.

1. **The accept timeout is derived from configuration, not from the constant, and
   is deliberately later than the reaper's.** `await_approval` used
   `tokio::time::timeout(ACCEPT_TIMEOUT, …)` — the 120 s constant — while the
   reaper bounds `WaitingAccept` by `config.accept_timeout`. A host that set a
   longer `accept_timeout` would have had its prompts cut short by a bound it never
   set, and reported as `DeclinedByUser` rather than `TimedOut`.

   Naively switching to `config.accept_timeout` made the two bounds land on the
   same instant and which reason the peer heard became a race — caught by the new
   integration test, not by inspection. The fix is
   `accept_timeout + APPROVAL_BACKSTOP_GRACE` (5 s, more than one `REAP_INTERVAL`
   tick), so the reaper always wins and the peer always hears `TimedOut` for a
   prompt nobody touched. The backstop exists only so a provider that never answers
   cannot leak a task.

2. **A refused post-approval transition cleans up its temp file.** If the reaper
   ends a transfer while the prompt is on screen, a late Accept opens a temp file
   and *then* finds the state machine refuses `Transferring`. `clean_up_temp` had
   already run, on a record that did not yet have a path to clean, so the `.part`
   file was left in the download directory — unverified peer-supplied data. The
   stale-accept path now cleans up explicitly.

`TransferId::from_hex` was added (exact, case-insensitive, rejecting prefixes and
signed forms) because the control protocol carries the id as hex.

---

## 7. UX-1 — security invariants

| Invariant | How it holds |
| --- | --- |
| The GUI does not grant trust | Nothing in `approval.rs` or `FileApproval` touches the trust store. Approval is per-offer, stored nowhere, and the next offer from the same device asks again |
| Approval binds to the authenticated offer | Pending entries are keyed by `TransferId`; `decide` resolves the named entry and no other. `pending_peer` records the TLS-authenticated fingerprint |
| A decision for offer A cannot approve offer B | Structural: there is no code path that resolves an entry other than the one named. Tested at both the unit and socket level |
| A decision for peer A cannot approve peer B | Transfer ids are per-transfer and single-use, so two peers can never collide on one. Tested with two paired peers |
| Source is not identified by display name | The prompt carries the full and short fingerprint; the name shown is the trust store's for that fingerprint |
| No "always accept" persisted | Not implemented, by design |
| No notification-based auto-actions | None added |
| TLS 1.3 / SPKI pinning / proof-of-possession | Untouched |
| Per-peer grants | Untouched. `FilesAuthorizer` is still asked fresh before the prompt and again after it |
| One-time data-stream challenge / HMAC | Untouched, and never leaves the daemon |
| Filename/path sanitation, size limits, idempotency | Untouched |
| GUI is not the data plane | The only things crossing the socket are the metadata in `FileOfferRequest` and a boolean back |
| The override is not reachable from a client | `unattended` is constructor-only; a provider attaching to an unattended daemon is told so and changes nothing |

---

## 8. UX-1 — UI behaviour

```
┌─────────────────────────────────────────┐
│  Incoming file                          │
│  SM-X620 wants to send you a file.      │
│                                         │
│  ux1-accept.txt                         │
│  518.7 KB · text/plain                  │
│  Verified device · 573C CB84 DA6C 993B  │
│                                         │
│            [ Decline ]  [ Accept ]      │
└─────────────────────────────────────────┘
```

Captured live from the accessibility tree during the real-device run:

```
 alert 'Incoming file'
     label 'Incoming file'
     label 'SM-X620 wants to send you a file.'
     label 'ux1-accept.txt'
     label '518.7 KB · text/plain'
     label 'Verified device · 573C CB84 DA6C 993B'
     button 'Decline'
     button 'Accept'
```

`AdwAlertDialog`, the existing Libadwaita idiom (`views/peers.rs` uses the same
type for revocation). No custom window chrome. English strings written directly,
following the current desktop convention — there is no i18n strategy in
`anyflow-gui` today and this sprint does not invent one.

Not displayed: file contents, any preview, the stream challenge, session keys,
pairing tokens.

Accessibility:

* `set_default_response("decline")` — Enter declines.
* `set_close_response("decline")` — Escape and the window-manager close decline.
* Buttons carry their labels as accessible names (`button 'Decline'`,
  `button 'Accept'` above).
* The filename label is `selectable(true)` and wraps.
* The detail column carries one accessible label — *"ux1-accept.txt, 518.7 KB, from
  the verified device 573C CB84 DA6C 993B"* — so assistive technology announces the
  essentials together rather than as three loose labels.
* **No response is given a colour appearance.** The Adwaita convention would tint
  Accept as `Suggested`; a consent gate should not nudge, and the two buttons must
  not differ by colour alone.

---

## 9. UX-1 — timeout / disconnect / exit behaviour

| State | Behaviour | Where it is enforced |
| --- | --- | --- |
| Peer disconnects while the prompt is open | Reaper sees `session.is_closed()` → `Transport`; the session withdraws the prompt, the dialog closes, a later Accept resolves nothing | `reap_once` + `run_file_approval_session` + `decide` returning `Unknown` |
| Offer expires | Reaper deadline → `TimedOut`; prompt withdrawn; stale Accept inert | `deadline_for(WaitingAccept)`, strictly before the approval backstop |
| GUI exits or crashes | Socket closes → `detach` → pending senders dropped → `confirm_receive` reads a closed channel as **decline**. Never an auto-accept | `FileApproval::detach` |
| A second provider attaches | The first's pending questions are dropped (declined); the first session is told `replaced` | `FileApproval::attach` |
| Daemon exits | The GUI's stream ends → `ApprovalUpdate::Detached` → every dialog closes. No fake actionable prompt survives | `client::watch_file_offers` |
| Duplicate decision | The entry is removed as it is answered, so the second finds nothing | `FileApproval::decide` |
| Decline | Terminal. `FileCancel{DECLINED_BY_USER}`, and the state machine refuses any later transition | `fail()` + `can_transition_to` |
| Provider stops reading | Queue fills at 32 → the offer is never queued → decline, rather than an unbounded backlog of questions | `try_send` on a bounded channel |

Every one of these is tested without a `sleep` on the decision path: the
deterministic tests drive the state directly, and the integration tests wait on
events with a bound rather than on wall-clock guesses.

---

## 10. UX-1 — deterministic test results

### 10.1 `anyflow-runtime::approval` unit tests (15)

```
running 15 tests
test approval::tests::with_no_provider_attached_an_offer_is_declined ... ok
test approval::tests::the_explicit_override_still_accepts_without_asking ... ok
test approval::tests::a_provider_cannot_turn_the_override_off ... ok
test approval::tests::a_provider_that_accepts_produces_an_acceptance ... ok
test approval::tests::a_provider_that_declines_produces_a_decline ... ok
test approval::tests::a_decision_for_one_offer_does_not_resolve_another ... ok
test approval::tests::a_decision_for_one_peer_does_not_resolve_anothers_offer ... ok
test approval::tests::a_second_decision_on_the_same_offer_is_inert ... ok
test approval::tests::an_unknown_transfer_id_answers_nothing ... ok
test approval::tests::a_withdrawn_offer_declines_and_cannot_be_accepted_afterwards ... ok
test approval::tests::dropping_the_question_removes_it_so_a_late_accept_finds_nothing ... ok
test approval::tests::detaching_a_provider_declines_what_it_was_being_asked ... ok
test approval::tests::a_stale_detach_does_not_unhook_the_live_provider ... ok
test approval::tests::replacing_a_provider_declines_the_old_ones_questions ... ok
test approval::tests::a_provider_that_stops_reading_declines_rather_than_queueing_forever ... ok

test result: ok. 15 passed; 0 failed
```

### 10.2 `desktop/daemon/tests/file_approval.rs` — over the real control socket (11)

Real TLS, real pinning, real MAC, real reaper, real `FileSink`. The approval
provider is a Unix-socket client speaking exactly what the GTK application speaks.
**No test in this file starts a daemon with `--accept-files-without-asking`.**

```
running 11 tests
test with_no_provider_attached_the_desktop_still_declines ... ok
test attaching_a_provider_does_not_decide_anything_on_its_own ... ok
test accepting_at_the_prompt_completes_the_transfer_through_the_existing_sink ... ok
test declining_at_the_prompt_leaves_no_file_and_tells_the_phone ... ok
test a_provider_that_hangs_up_mid_prompt_declines_rather_than_accepting ... ok
test a_decision_for_one_offer_never_resolves_another ... ok
test a_decision_for_one_peer_never_resolves_anothers_offer ... ok
test a_repeated_or_reversed_decision_changes_nothing ... ok
test a_decision_for_an_offer_the_provider_was_never_shown_is_ignored ... ok
test a_peer_that_disconnects_withdraws_the_prompt_and_a_late_accept_does_nothing ... ok
test an_unanswered_prompt_expires_and_cannot_be_accepted_afterwards ... ok

test result: ok. 11 passed; 0 failed
```

### 10.3 Coverage against the brief's A–O

| | Requirement | Covered by |
| --- | --- | --- |
| A | Unattended default declines | `with_no_provider_attached_an_offer_is_declined`, `with_no_provider_attached_the_desktop_still_declines` |
| B | Override retains existing semantics | `the_explicit_override_still_accepts_without_asking`, `a_provider_cannot_turn_the_override_off` |
| C | Provider accepts one offer | `a_provider_that_accepts_produces_an_acceptance`, `accepting_at_the_prompt_…` |
| D | Provider declines one offer | `a_provider_that_declines_produces_a_decline`, `declining_at_the_prompt_…` |
| E | Closing the prompt declines | `a_provider_that_hangs_up_mid_prompt_declines_rather_than_accepting`; in the GUI, `set_close_response(DECLINE)` |
| F | Offer A cannot resolve offer B | `a_decision_for_one_offer_does_not_resolve_another` (unit + socket) |
| G | Peer A cannot resolve peer B | `a_decision_for_one_peer_does_not_resolve_anothers_offer` (unit + socket, two paired peers) |
| H | Disconnect invalidates the prompt | `a_withdrawn_offer_…`, `a_peer_that_disconnects_withdraws_the_prompt_…` |
| I | Timeout invalidates the prompt | `dropping_the_question_removes_it_…`, `an_unanswered_prompt_expires_…` |
| J | Duplicate Accept harmless | `a_second_decision_on_the_same_offer_is_inert`, `a_repeated_or_reversed_decision_changes_nothing` |
| K | Duplicate Decline harmless | same two |
| L | Stale Accept after Decline does nothing | same two |
| M | Transfer proceeds through the existing sink | `accepting_at_the_prompt_completes_the_transfer_through_the_existing_sink` (SHA-256 compared, no `.part` left) |
| N | Declined transfer creates no completed file | `declining_at_the_prompt_…`, `with_no_provider_attached_…` |
| O | GUI absence never becomes implicit acceptance | `with_no_provider_attached_…` (both levels) |

Two further tests beyond the list, because the states exist and are reachable:
`attaching_a_provider_does_not_decide_anything_on_its_own` (attachment is not
consent) and `a_decision_for_an_offer_the_provider_was_never_shown_is_ignored`
(a client cannot answer a question it was not asked).

GTK-level tests are limited to what is stable and meaningful: the two pure
functions behind the dialog (`sizes_are_rendered_for_a_human_to_weigh`,
`the_safe_response_is_the_default_and_the_close_response`). The dialog's actual
behaviour is proved on the real desktop in §11–12, driven through AT-SPI.

---

## 11. UX-1 — real Decline regression

Setup — every element real, nothing stubbed:

* daemon: `./target/debug/anyflowd --log info`. Verified from `/proc/<pid>/cmdline`
  and by the absence of the override's startup warning in the log. **The override
  was never used at any point in this sprint's physical testing.**
* GUI: `./target/debug/anyflow-gui`, a normal graphical session.
* Android: SM-X620, Android 16, freshly paired to this desktop by optical QR scan,
  `files.v1` granted by hand.
* Share path: the **real system Sharesheet** (`android/com.android.internal.app.ResolverActivity`),
  choosing "Send with AnyFlow", then `SendActivity` → Send.

Result:

```
daemon: incoming file offer transfer=345b112f peer=573C CB84 DA6C 993B
                            filename=ux1-evidence.txt size=354128

dialog: alert 'Incoming file'
          'SM-X620 wants to send you a file.'
          'ux1-evidence.txt'
          '345.8 KB · text/plain'
          'Verified device · 573C CB84 DA6C 993B'
          [Decline] [Accept]

→ Decline pressed

daemon: transfer ended transfer=345b112f state=cancelled
                       reason=declined by the user
Android: "declined"
desktop: no ux1-evidence.txt in ~/Downloads/AnyFlow
         no .part and no .anyflow-* artefact
```

Against §13's PASS criteria: offer reached the desktop ✓; approval UI appeared ✓;
filename matched ✓; size matched (354 128 B = 345.8 KB) ✓; source identity correct
— fingerprint `573C CB84 DA6C 993B` is the tablet's, and the name came from the
trust store ✓; Decline → Android received the decline ✓; no completed file ✓; the
daemon was never started with automatic accept ✓.

**Real Decline: PASS.**

---

## 12. UX-1 — real Accept regression + SHA256

Same setup. A second file, because of the debt recorded in §26: after a declined
transfer, sharing the *same* filename again shows the old outcome row instead of a
Send button.

```
daemon: incoming file offer transfer=b84b453c peer=573C CB84 DA6C 993B
                            filename=ux1-accept.txt size=531187

dialog: 'ux1-accept.txt'  '518.7 KB · text/plain'
        'Verified device · 573C CB84 DA6C 993B'

→ Accept pressed

daemon: received, verified and stored transfer=b84b453c
                                      filename=ux1-accept.txt bytes=531187
Android: "Sent"
```

SHA-256, source against destination:

```
378944a5d36f884efc4219c63a4f13548f5aabad326162464eff55773cafde7c  (source, sent from the tablet)
378944a5d36f884efc4219c63a4f13548f5aabad326162464eff55773cafde7c  /home/yuri/Downloads/AnyFlow/ux1-accept.txt
```

Identical. `~/Downloads/AnyFlow` holds no `.part` or `.anyflow-*` artefact. The
file was written by the existing `FileSink` to the existing destination; no byte of
it passed through the GUI process.

**Real Accept: PASS.**

A prior rehearsal against `examples/fake_phone` — a real TLS peer with real pinning
— exercised the same three paths (decline, accept, disconnect-under-prompt) before
the device was involved, and matched.

---

## 13. UX-2 — defect of record

ANDROID-UX-ORIENTATION-01: opening the pairing QR scanner forced the device into
landscape, whatever the user was doing.

The device's own `RotationHistory` from the pre-fix build, on 2026-09-15, records it
three times in one session:

```
09-15 16:00:57.418 ROTATION_0 to ROTATION_90
  source=ActivityRecord{… anyflow/com.journeyapps.barcodescanner.CaptureActivity t2328}
         SCREEN_ORIENTATION_LANDSCAPE
  mode=USER_ROTATION_FREE user=ROTATION_0 sensor=ROTATION_0
09-15 16:01:01.194 ROTATION_90 to ROTATION_0
  source=ActivityRecord{… anyflow/.ui.MainActivity t2328} SCREEN_ORIENTATION_UNSPECIFIED
```

Read it carefully: `user=ROTATION_0 sensor=ROTATION_0` — the user's setting said
portrait and the sensor said portrait — and the app forced `SCREEN_ORIENTATION_LANDSCAPE`
anyway. The screen snapped back to portrait the instant `MainActivity` returned.

---

## 14. UX-2 — root cause

**Exact root cause: two independent causes, neither of them in AnyFlow's own code,
and fixing either one alone still leaves the scanner locked.**

### Cause 1 — the dependency's manifest, imported by the merger

`com.journeyapps:zxing-android-embedded:4.3.0` ships this in its own
`AndroidManifest.xml` (extracted from the AAR in the Gradle cache and read
directly):

```xml
<activity
    android:name="com.journeyapps.barcodescanner.CaptureActivity"
    android:clearTaskOnLaunch="true"
    android:screenOrientation="sensorLandscape"
    android:stateNotNeeded="true"
    android:theme="@style/zxing_CaptureTheme"
    android:windowSoftInputMode="stateAlwaysHidden" />
```

`ScanContract` launches `CaptureActivity` by default. AnyFlow declared no scanner
activity of its own, so the manifest merger imported `sensorLandscape` verbatim.
Nobody in this repository ever asked for landscape, and nothing in this repository
could have been edited to stop it.

### Cause 2 — the library's runtime default

`CaptureManager.initializeFromIntent` (disassembled from the AAR's `classes.jar`):

```
35: ldc  "SCAN_ORIENTATION_LOCKED"
37: iconst_1                                  ← default TRUE
38: invokevirtual android/content/Intent.getBooleanExtra:(Ljava/lang/String;Z)Z
45: ifeq 52
49: invokevirtual lockOrientation:()V
```

and `lockOrientation()` reads the *current* display rotation and configuration and
calls `activity.setRequestedOrientation()` with a single hard value — `LANDSCAPE`
(0), `REVERSE_LANDSCAPE` (8), `PORTRAIT` (1) or `REVERSE_PORTRAIT` (9).

`ScanOptions.setOrientationLocked` only writes the extra when it is called, and
AnyFlow never called it. So the extra was absent, the default was `true`, and the
activity was pinned to whatever orientation it launched in — which, because of
cause 1, was always landscape. Even the 180° flip was blocked.

---

## 15. UX-2 — scanner architecture before

```
MainActivity
  registerForActivityResult(ScanContract())        ← launches CaptureActivity
  launchScanner():
      ScanOptions()
        .setDesiredBarcodeFormats(QR_CODE)
        .setPrompt("Point at the QR code shown by `anyflow pair`")
        .setBeepEnabled(false)
                                                  ← setOrientationLocked never called
  result callback:
      val contents = result.contents ?: return    ← cancellation silently swallowed
      QrPayload.parse(contents) ?: showError(…)

AndroidManifest.xml
  (no scanner activity declared → library's sensorLandscape imported)
```

Audited and confirmed absent from AnyFlow's own sources: no `screenOrientation`
anywhere in the app manifest, no `requestedOrientation`, no
`setRequestedOrientation`, no custom scanner Activity or subclass, no
`configChanges`, no orientation-related intent extra, no Compose state tied to
configuration.

---

## 16. UX-2 — scanner architecture after

Both causes addressed, and the two fixes cross-reference each other so neither can
be removed alone in ignorance.

### Manifest (`android/app/src/main/AndroidManifest.xml`)

```xml
<activity
    android:name="com.journeyapps.barcodescanner.CaptureActivity"
    android:screenOrientation="unspecified"
    tools:replace="android:screenOrientation" />
```

`unspecified` is the platform default and the only value that means *"not ours to
decide"*: Android applies the user's own orientation policy. Deliberately **not**
`fullSensor` or `fullUser`, which the library's README suggests — those force sensor
rotation and would override a person's rotation lock, which is the same defect
pointing the other way. `tools:replace` is required; without it the merger sees two
conflicting values for one attribute and fails the build. Every other attribute the
library sets is inherited unchanged, confirmed in the merged manifest (§17).

No new activity class was introduced. A `CaptureActivity` subclass would have worked
too, but would have left the library's landscape-locked declaration in the merged
manifest as dead weight.

### New `ui/PairingScanner.kt`

```kotlin
fun options(): ScanOptions = ScanOptions()
    .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
    .setPrompt(PROMPT)
    .setBeepEnabled(false)
    .setOrientationLocked(false)     // the library's default is the opposite

sealed interface Outcome { Cancelled; NotAnyFlowCode; Pair(payload) }
fun outcomeOf(contents: String?): Outcome
```

Both halves were extracted from `MainActivity` so they can be tested on the JVM —
`ScanOptions` is a plain Java builder over a map of intent extras and needs no
device. `Outcome` is a sealed set rather than a nullable payload so "the user backed
out" and "that code was not ours" cannot be collapsed into one silent `return`;
`MainActivity`'s callback is now a `when` over it.

### Camera lifecycle

`configChanges` is deliberately **not** declared, leaving the library's own tested
path: Android reconfigures the activity on rotation and `CameraPreview`'s
`RotationListener` re-frames the preview. Measured on the device (§21): the
`ActivityRecord` identity is *unchanged* across a rotation and the task holds
exactly two activities throughout — `MainActivity` and one `CaptureActivity`. No
duplicate scanner, no second pairing result, and `dumpsys window` reports
`mCameraAppInfoSet={}` — camera released — after cancel.

### Permissions

No permission change. `CAMERA` was already declared and is still requested at the
moment of use. No new permission was requested, and camera permission handling was
not touched.

---

## 17. UX-2 — orientation semantics

The merged manifest that actually ships
(`app/build/intermediates/merged_manifests/debug/processDebugManifest/AndroidManifest.xml`):

```xml
<activity
    android:name="com.journeyapps.barcodescanner.CaptureActivity"
    android:clearTaskOnLaunch="true"
    android:screenOrientation="unspecified"
    android:stateNotNeeded="true"
    android:theme="@style/zxing_CaptureTheme"
    android:windowSoftInputMode="stateAlwaysHidden" />
```

`unspecified`, with the library's other attributes intact. The only remaining
occurrence of the string `sensorLandscape` anywhere in the merged manifest is inside
the explanatory comment that documents what went wrong.

The live activity, read off the device with the scanner open:

```
overrideOrientation=SCREEN_ORIENTATION_UNSPECIFIED
requestedOrientation=SCREEN_ORIENTATION_UNSPECIFIED
```

in both portrait and landscape. Before the fix this was `SCREEN_ORIENTATION_LANDSCAPE`.

So: AnyFlow does not own the user's orientation during a pairing scan. Rotation
locked to portrait stays portrait; rotation unlocked follows the device; nothing is
forced against an OS orientation lock.

---

## 18. UX-2 — Android tests

`android/app/src/test/.../PairingScannerOrientationTest.kt`, 11 tests, JVM only —
no Robolectric added.

```
A  the scan request explicitly unlocks the orientation
A  the scan request carries no orientation forcing configuration
A  the scan request is still a silent scan with the same prompt
B  the scanner activity is declared unspecified and overrides the library
B  no activity in the manifest forces or overrides an orientation
C  no production code locks the screen orientation
C  no production code re-locks the scanner orientation
D  a cancelled scan is cancelled and not a failed pairing
E  a valid code flows into the existing pairing parser
E  an arbitrary uri is still not a pairing code
E  a rejected outcome carries none of the scanned text
```

Mapped to the brief's A–E:

* **A** — `PairingScanner.options().moreExtras` must contain
  `SCAN_ORIENTATION_LOCKED = false`. Asserted as *present and false*, not merely
  absent, because the library's default is `true`: absence **is** the defect.
* **B** — the app manifest is parsed and the `CaptureActivity` declaration checked
  for `screenOrientation="unspecified"` **and** `tools:replace`. A second test
  asserts the whole manifest contains exactly one `screenOrientation` declaration
  and that its value is `unspecified`, so `fullSensor`/`fullUser` or a lock on any
  other activity fails the build.
* **C** — every shipping `.kt` is scanned for `setRequestedOrientation`,
  `SCREEN_ORIENTATION_*` and `setOrientationLocked(true)`.
* **D** — `outcomeOf(null) == Cancelled`.
* **E** — a well-formed code produces `Pair` with the parsed fingerprint, token and
  device id; an arbitrary URI, a foreign scheme, an empty string and a non-hex
  fingerprint all produce `NotAnyFlowCode`.

Both static checks strip comments before scanning. That is not incidental: the first
run of these tests **failed**, because the manifest comment says `sensorLandscape`
and `PairingScanner`'s KDoc says `setRequestedOrientation` in order to explain the
defect. A check that cannot tell prose from code pushes the next person towards
deleting the explanation.

The plus-one beyond the list (`a rejected outcome carries none of the scanned text`)
pins that a rejected code's `toString` never carries the pairing token.

Full Android suite: **592 tests, 0 failures, 0 errors, 0 skipped.**

---

## 19. UX-2 — physical portrait tests

Device: **Samsung SM-X620, Android 16 (API 36), One UI 8.0.** Native display
1800×2880. Every reading below is from `dumpsys`, `uiautomator` or `screencap` on
the device, not from a screenshot judged by eye.

### Test 1 — device portrait, rotation unlocked → scanner portrait

Setting: `accelerometer_rotation=1`, `user_rotation=0`, display `ROTATION_0`.

```
focus     = anyflow/com.journeyapps.barcodescanner.CaptureActivity
rotation  = ROTATION_0
config    = … xlrg … port …  mDisplayRotation=ROTATION_0 mRotation=ROTATION_0
requestedOrientation = SCREEN_ORIENTATION_UNSPECIFIED
screencap = 1800 x 2880  (portrait)
RotationHistory: no new entry
```

**PASS.** Before the fix this produced `ROTATION_0 to ROTATION_90` sourced from
`CaptureActivity SCREEN_ORIENTATION_LANDSCAPE`.

### Test 4 — cancel/back returns cleanly

```
KEYCODE_BACK →
  focus   = anyflow/io.github.yurisismotto.anyflow.ui.MainActivity
  records = 1                        (the scanner's ActivityRecord is gone)
  camera  = mCameraAppInfoSet={}     (released)
  screen  = 1800 x 2880              (still portrait)
  no "not a AnyFlow pairing code" toast; no pairing occurred
```

**PASS.**

### Test 7 — successful QR scan in portrait → pairing succeeds

A genuine optical scan of a `qrencode`-rendered payload on the Fedora screen, with
the tablet held in portrait, auto-rotate on. Both sides had no prior trust for each
other.

```
desktop: A device proved it holds the pairing code:
           name        SM-X620
           device id   6532889e82ba83d0782cc644e7a21fc3
           fingerprint 573C CB84 DA6C 993B
         → confirmed → "Paired with 573C CB84 DA6C 993B."

tablet trust store: Fedora df65d3e4ba28edf9 added; selectedPeer = Fedora
RotationHistory: no entry between 21:42:15 and 21:50 — nothing rotated
```

**PASS.**

The operator's first impression during this scan was that something had rotated. The
instrumentation shows otherwise — no rotation event, display `ROTATION_0` throughout,
`requestedOrientation=SCREEN_ORIENTATION_UNSPECIFIED` — and on a second look at the
scanner the operator confirmed it was upright and correct. Recorded because it was
reported, and resolved against the device's own trace rather than waved away.

---

## 20. UX-2 — physical landscape tests

### Test 5 — scanner started in landscape → opens landscape

Rotation locked landscape (`accelerometer_rotation=0`, `user_rotation=1`), scanner
launched from a landscape `MainActivity`:

```
focus     = anyflow/com.journeyapps.barcodescanner.CaptureActivity
rotation  = ROTATION_90
requestedOrientation = SCREEN_ORIENTATION_UNSPECIFIED
screencap = 2880 x 1800  (landscape)
records   = 2            (MainActivity + one CaptureActivity)
```

**PASS.**

### Test 8 — successful QR scan in landscape → pairing succeeds

Run twice, because the first attempt was less rigorous than it looked.

**8a — auto-rotate on, tablet physically turned to landscape.** Decoded, and the
pairing flow resumed into a working authenticated session. But the tablet was
*already* paired with this desktop from test 7, so the Android side took its
already-trusted path and this resolved as a reconnect rather than fresh trust.
Recorded as such rather than claimed as more than it was.

**8b — both sides cleared first.** Fedora revoked on the desktop *and* forgotten on
the tablet (the three preserved U2 VM peers untouched), rotation locked to landscape,
scanner open, fresh 900 s token:

```
desktop: A device proved it holds the pairing code:
           name        SM-X620
           device id   6532889e82ba83d0782cc644e7a21fc3
           fingerprint 573C CB84 DA6C 993B
         → confirmed → "Paired with 573C CB84 DA6C 993B."

RotationHistory: no new entry during the scan (last entry 22:14:45,
                 the operator's own rotation lock, sourced from MainActivity)
```

A genuine fresh trust establishment from a landscape scan, with the orientation
never moving. **PASS.**

One attempt between 8a and 8b failed for an unrelated reason and is recorded for
honesty: the 900 s pairing window expired while the tablet's "Forget this device"
screens were being navigated, so that scan read a dead token and the desktop
reported `Pairing window expired`. Re-staged with a fresh token, it succeeded.

---

## 21. UX-2 — rotation-while-open test

Rotation driven with the scanner open and a prompt-free camera preview running.

### Portrait → landscape → portrait, rotation lock moved under the scanner

```
before            rotation=ROTATION_0   focus=CaptureActivity   records=2
user_rotation=1 → rotation=ROTATION_90  focus=CaptureActivity   records=2
user_rotation=0 → rotation=ROTATION_0   focus=CaptureActivity   records=2
unlock (accel=1) → rotation=ROTATION_0  focus=CaptureActivity   records=2
```

The scanner followed every change. Throughout, the task held exactly two activities
and the `CaptureActivity` `ActivityRecord` identity was **unchanged** (`227793907`)
across the portrait↔landscape cycle — the activity was reconfigured, not duplicated.
Only the window handle changed.

### Sensor-driven rotation, auto-rotate on, operator turning the tablet

This is the decisive pair of entries:

```
09-16 22:04:22.554 ROTATION_0 to ROTATION_90
  source=ActivityRecord{… anyflow/com.journeyapps.barcodescanner.CaptureActivity t2380}
         SCREEN_ORIENTATION_UNSPECIFIED
  mode=USER_ROTATION_FREE user=ROTATION_0 sensor=ROTATION_90
09-16 22:04:24.994 ROTATION_90 to ROTATION_0
  source=ActivityRecord{… anyflow/com.journeyapps.barcodescanner.CaptureActivity t2380}
         SCREEN_ORIENTATION_UNSPECIFIED
  mode=USER_ROTATION_FREE user=ROTATION_0 sensor=ROTATION_0
```

Same device, same app, same activity class as the §13 defect trace. Two differences:
`SCREEN_ORIENTATION_UNSPECIFIED` where it said `SCREEN_ORIENTATION_LANDSCAPE`, and
`sensor=ROTATION_90` — the rotation came from the person turning the tablet, not from
the app forcing it. The operator confirmed the preview stayed usable and the code
scanned in that orientation.

**Tests 2 and 3: PASS.**

### Phone form factor

Only the tablet was available, so this is by design and test rather than by a second
device: the fix removes orientation ownership entirely rather than choosing a
different orientation, so there is no form-factor-dependent branch left to differ.
The two static tests (§18 B and C) assert exactly that — no `screenOrientation`
other than `unspecified` anywhere in the manifest, and no `setRequestedOrientation`
in any shipping source — which holds on any screen size. Recorded as a residual
gap in §26.

---

## 22. UX-2 — rotation-lock test

### Test 6 — system rotation lock ON in portrait → scanner stays portrait

```
accelerometer_rotation=0  user_rotation=0
focus     = anyflow/com.journeyapps.barcodescanner.CaptureActivity
rotation  = ROTATION_0
screencap = 1800 x 2880 (portrait)
records   = 2
```

**PASS.** And the converse, which matters just as much: with the lock set to
landscape (`user_rotation=1`) the scanner opened and ran landscape and stayed there
through a full scan (§20 test 8b), with no rotation event of its own.

This is why `unspecified` was chosen over `fullSensor`/`fullUser`. Either of those
would have passed test 1 and failed this one.

---

## 23. Pairing regression

The QR contract, its validation, and every trust decision are unchanged by this
sprint.

* Format still `anyflow1:<fingerprint-hex>:<token-base32>:<device-id>:<addr>[,…]`.
  `QrPayload.parse` was not modified.
* Schema/version, fingerprint/SPKI identity, token length (20 bytes), device-id
  charset and length, and the `MAX_LENGTH` bound are all still enforced in
  `QrPayload.parse` and nowhere else. `PairingScanner.outcomeOf` delegates to it.
* Proof-of-possession, trust establishment and TLS pinning: untouched. The
  desktop-side `verify_pairing_proof` and `confirm_pairing` paths were not changed.
* Arbitrary URI scanning still cannot pair — asserted for four rejected forms in
  §18 E, and observed on the device: a scan of a code for a peer whose token had
  expired produced `Pairing window expired` rather than a pairing.
* Full QR payloads are not logged by the app. `PairingScanner.Outcome` deliberately
  carries nothing for a rejected code, and §18 pins that the token cannot appear in
  its `toString`.
* No camera frame is persisted. No new permission.

Three genuine optical pairings were completed during this sprint (portrait fresh,
landscape reconnect, landscape fresh), plus one deliberate expired-token rejection.
**Pairing security regression: PASS.**

Preserved U2 certification state: the tablet's three VM peers — `anyflow-u2604`,
`anyflow-d13`, `anyflow-u2404`, each with `battery.v1` + `files.v1` — were present
before this sprint and are present, unmodified, after it. Only the Fedora⇄tablet
pairing created during this sprint was added, revoked and re-created.

---

## 24. Privacy / logging audit

The daemon log for the whole physical session was audited with ANSI escapes
stripped. The GUI wrote nothing at all (0 bytes).

| Must not be logged | Result |
| --- | --- |
| File body / contents | A 40-byte fragment of the accepted file's content appears **0** times in either log |
| Full transfer ids | **0**. Every one is the 8-hex display form: `transfer=142180fa`, `345b112f`, `631c4e7f`, `8eda5558`, `b84b453c` |
| Full fingerprints | **0** 64-hex strings. Every peer is the short display form: `peer=573C CB84 DA6C 993B` |
| QR payload / pairing secret | **0** occurrences of `token`, `challenge`, `hmac`, `private` at any level |
| One-time file challenge / HMAC | **0**. The challenge never leaves the daemon and is cleared on any terminal transition |
| Private key | **0** |
| Clipboard content | **0** |
| Notification content | **0** |

Three 32-hex strings do appear. All three are **device ids**
(`795fec…` local, `6532889e…` tablet, `51881e3b…` the rehearsal peer) — non-secret
identifiers the daemon has always logged at startup and on session establishment.
Not transfer ids, not key material.

Filenames: the log carries the **sanitized** name (`filename=ux1-accept.txt`). This
is the pre-existing `files.v1` policy — `on_offer` has always logged it, with a
comment explaining that the raw peer-supplied name is never logged at any level
because it is attacker-controlled and could forge log lines. The new decline path in
`FileApproval` logs exactly what the old `ConsoleApproval` logged, no more.

No approval history is kept, anywhere, at any level. No cloud. No telemetry.

One pre-existing behaviour worth naming, unchanged by this sprint and outside its
scope: `anyflow pair` prints the QR payload — including the single-use token — to
the operator's own terminal, as a documented fallback for a phone that cannot scan
("If your phone cannot scan, the payload is: …"). That is terminal output for a
person, not a persistent log, and the token is single-use with a TTL. Recorded as an
observation, not a finding of this sprint.

---

## 25. CI / local quality results

Run sequentially. Cargo and Gradle were never run at the same time; no emulator was
started; no VM was booted; no `swapoff`, cache drop, kernel or power-management
change was made.

### Desktop

```
cargo fmt --all --check                                          clean
cargo test -p anyflow-capability-files -p anyflow-runtime        pass
cargo test -p anyflow-daemon --test file_approval                11 passed
cargo test -p anyflow-daemon --test files                        36 passed
cargo test --workspace                                           786 passed,
                                                                 0 failed, 23 ignored
cargo clippy --locked --workspace --all-targets --all-features
      -j 2 -- -D warnings                                        clean (exit 0)
```

`-j 2` was accepted in that position by this Cargo, so the brief's command ran
verbatim.

Two clippy findings on my own new code were fixed rather than allowed:
`chunks_exact_to_as_chunks` in `TransferId::from_hex` (rewritten as an indexed loop
with an explicit ASCII-hex guard, which also closes a `from_str_radix` sign-form
hole — `+a` no longer parses as `0a`), and `never_loop` in a test.

### Android

```
./gradlew --no-daemon --max-workers=2 :app:assembleDebug :fixture:assembleDebug
                                                                 BUILD SUCCESSFUL
./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest
                                                                 BUILD SUCCESSFUL
./gradlew --no-daemon --max-workers=2 :app:testDebugUnitTest --rerun
                                                                 592 tests,
                                                                 0 failures, 0 errors
```

`connectedAndroidTest` was **not** run. It uninstalls the app, which would have
destroyed the pairing and the U2 trust state this sprint had to preserve.

Android lint remains a separately recorded debt and was not touched.

### Workflow files

**No workflow file was modified.** Checked against the two classification guards in
`.github/workflows/portable-windows-msvc.yml`:

* the `core/tests` file-list guard — `desktop/core/tests` is unchanged;
* the `capabilities/notifications/tests` file-list guard — unchanged.

The new test targets are `desktop/runtime/src/approval.rs` (unit tests inside a
crate that is explicitly *excluded* from the portable set) and
`desktop/daemon/tests/file_approval.rs` (the daemon crate, also outside the portable
set). Neither is in a guarded directory and neither is a portable-core or
notifications test, so no workflow change is required or permitted.

`anyflow-control` *is* in the portable set and gained new types. They are plain
`serde` structs and enums with no new dependency, so `cargo test -p anyflow-control
--no-run --no-default-features --target x86_64-pc-windows-msvc` is unaffected; the
crate compiles clean under `--all-features` locally and has no platform-conditional
code.

---

## 26. Files changed

### Modified (15)

| File | What |
| --- | --- |
| `desktop/control/src/lib.rs` | `WatchFileOffers`, `FileDecision`, `FileApprovalReady`, `FileOfferRequest`, `FileOfferWithdrawn` |
| `desktop/runtime/src/lib.rs` | `pub mod approval` |
| `desktop/runtime/src/state.rs` | `file_approval` field + `with_file_approval` |
| `desktop/runtime/src/server.rs` | `run_file_approval_session`, `apply_file_decision`, `file_offer_request`; two dispatch arms |
| `desktop/daemon/src/main.rs` | builds one `FileApproval` and shares it; `AcceptEverything` and `ConsoleApproval` deleted |
| `desktop/daemon/src/lib.rs` | re-exports `approval` |
| `desktop/capabilities/files/src/lib.rs` | accept backstop derived from config and placed after the reaper; stale-accept temp cleanup |
| `desktop/capabilities/files/src/limits.rs` | `APPROVAL_BACKSTOP_GRACE` |
| `desktop/capabilities/files/src/transfer.rs` | `TransferId::from_hex` + 3 tests |
| `desktop/cli/src/main.rs` | new `Event` variants in two exhaustive matches |
| `desktop/gui/src/client.rs` | `watch_file_offers`, `ApprovalUpdate`, `ApprovalHandle`, re-attach loop |
| `desktop/gui/src/lib.rs` | `pub mod approval`; installs the provider for the window's lifetime |
| `desktop/daemon/tests/common/mod.rs` | `ApprovalMode`, `start_with_approval_provider`, `file_approval` field |
| `android/app/src/main/AndroidManifest.xml` | `tools:` namespace; `CaptureActivity` re-declared `unspecified` with `tools:replace` |
| `android/app/.../ui/MainActivity.kt` | routes through `PairingScanner`; `when` over `Outcome` |

### Added (5)

| File | What |
| --- | --- |
| `desktop/runtime/src/approval.rs` | `FileApproval` + 15 unit tests |
| `desktop/gui/src/approval.rs` | the `AdwAlertDialog` prompt + 2 unit tests |
| `desktop/daemon/tests/file_approval.rs` | 11 socket-level integration tests |
| `android/app/.../ui/PairingScanner.kt` | scan request + outcome |
| `android/app/src/test/.../PairingScannerOrientationTest.kt` | 11 tests |

```
15 files changed, 734 insertions(+), 90 deletions(-)   (tracked)
5 files added (untracked)
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` remains untracked, unmodified and unstaged.

---

## 27. Remaining debts

Newly discovered during this sprint, recorded rather than fixed:

1. **UX-DEBT-01 — a declined share offers no retry.** `SendActivity` replaces the
   Send button with the transfer's outcome row whenever `transfers` holds a sending
   transfer with the same filename. After a decline, re-sharing the *same* file
   shows "declined" and no way to try again for the life of the app's transfer list.
   Observed directly; §12 used a second file because of it.
2. **UX-DEBT-02 — a desktop-side revoke leaves the phone unable to re-pair.** With
   the desktop revoked but the tablet still trusting it, scanning a fresh QR made
   the tablet take its already-paired connect path; the desktop logged *"a revoked
   device is pairing again; it must prove the new token and be confirmed by hand"*
   and then *"timed out waiting for PAIR_REQUEST"*. Recovery required "Forget this
   device" on the tablet. Pre-existing and unrelated to either UX defect.
3. **UX-DEBT-03 — the scanner's prompt strip is clipped in portrait.**
   `zxing_status_view` lays out at `[623,2842][1176,2880]`, inside the navigation
   bar's `[0,2784][1800,2880]`. The portrait path had never been exercised before
   this sprint, so this layout was never visible. Cosmetic; the preview and the
   decode are unaffected.
4. **UX-DEBT-04 — orientation is verified on a tablet only.** Phone form factor is
   covered by design and by static test (§21), not by a second device.
5. Pre-existing and untouched: Android lint; `anyflow pair` printing the payload to
   the operator's terminal (§24).

### Scope check

Verified absent from the diff: branding, new logo, palette redesign, Quick Panel,
KDE tray/StatusNotifier, packaging, Android lint cleanup, trust-store ordering
cosmetics, notification role queue recovery, battery hotplug, concurrent sessions,
protocol/protobuf change, workflow changes.

---

## 28. Git status

```
$ git status --short
 M android/app/src/main/AndroidManifest.xml
 M android/app/src/main/java/io/github/yurisismotto/anyflow/ui/MainActivity.kt
 M desktop/capabilities/files/src/lib.rs
 M desktop/capabilities/files/src/limits.rs
 M desktop/capabilities/files/src/transfer.rs
 M desktop/cli/src/main.rs
 M desktop/control/src/lib.rs
 M desktop/daemon/src/lib.rs
 M desktop/daemon/src/main.rs
 M desktop/daemon/tests/common/mod.rs
 M desktop/gui/src/client.rs
 M desktop/gui/src/lib.rs
 M desktop/runtime/src/lib.rs
 M desktop/runtime/src/server.rs
 M desktop/runtime/src/state.rs
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
?? android/app/src/main/java/io/github/yurisismotto/anyflow/ui/PairingScanner.kt
?? android/app/src/test/java/io/github/yurisismotto/anyflow/PairingScannerOrientationTest.kt
?? desktop/daemon/tests/file_approval.rs
?? desktop/gui/src/approval.rs
?? desktop/runtime/src/approval.rs

$ git diff --check
(clean)
```

Nothing staged. Nothing committed. Nothing pushed. No PR.

---

## 29. Verdict

Both defects are resolved. Neither is waived.

UX-1 fails if any of these hold; none does:

* normal GUI still requires `--accept-files-without-asking` — no: two real
  transfers, one declined and one accepted with matching SHA-256, on a daemon whose
  `/proc/<pid>/cmdline` carried only `--log info`;
* closing the prompt accepts — no: `set_close_response("decline")`, plus a
  hang-up-mid-prompt integration test;
* the wrong offer can be approved — no: keyed by single-use `TransferId`, exact
  match only, tested across offers and across peers;
* the GUI becomes the file data plane — no: metadata out, one boolean back;
* no decision defaults to accept — no: six distinct routes to "no answer" all
  produce a decline, each tested.

UX-2 fails if any of these hold; none does:

* scanner still forces landscape — no: `unspecified` in the merged manifest,
  `SCREEN_ORIENTATION_UNSPECIFIED` on the live activity, `setOrientationLocked(false)`
  in the request;
* scanner ignores the system rotation lock — no: verified locked portrait and locked
  landscape, including through a full scan;
* rotation launches a duplicate scanner or pairing result — no: one `ActivityRecord`,
  identity unchanged across rotation, two activities in the task throughout, camera
  released on cancel;
* portrait or landscape scan breaks pairing — no: fresh trust established by optical
  scan in each orientation.

```
UX HARDENING: PASS
INCOMING FILE APPROVAL UI: PASS
HEADLESS DEFAULT REMAINS FAIL-CLOSED
QR SCANNER ORIENTATION: PASS
PAIRING SECURITY REGRESSION: PASS
```
