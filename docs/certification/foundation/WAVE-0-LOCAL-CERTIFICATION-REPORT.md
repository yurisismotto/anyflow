# Wave 0 — Final Local Certification Closeout

**Date:** 2026-08-31 · **Branch:** `feature/core-platform-abstraction-v1` · **No commit, no push.**

**Final status: WAVE 0 LOCAL GATES CLOSED**

G6 and G10 both passed on real hardware. `POC-CORE-04` (Windows MSVC runner) remains open and
is handled separately — Wave 0 is therefore **not** declared overall CERTIFIED.

---

## 1. Precheck

| Check | Result |
| --- | --- |
| `git branch --show-current` | `feature/core-platform-abstraction-v1` ✅ |
| `git diff --check` | clean ✅ |
| `git status` | 5 staged renames (pre-existing), 38 modified, 12 untracked — unchanged ✅ |
| Staging | nothing added ✅ |
| `adb devices -l` | `RX2Y500C7SY … device` (not `unauthorized`) ✅ |
| Fedora graphical session | `Active=yes`, `Type=wayland`, `LockedHint=no` ✅ |
| `connectedDebugAndroidTest` before G6 | not run ✅ |

---

## 2. Android hardware under test

| Field | Value |
| --- | --- |
| Model | Samsung **SM-X620** (`gts10fepwifi`) |
| Android | **16**, SDK **36** |
| Serial | `RX2Y500C7SY` |
| LAN address | `192.168.68.62` |
| App | `io.github.yurisismotto.anyflow` v0.1.0, **unmodified** |
| APK SHA-256 | `b08834d4a7130b5ba9b091e917b4946e9a057444ec1f142aa4ddc1ea7fdafa99` |

`git status --porcelain android/` was empty — Android source unchanged at commit `a0bb214`.

**Reproducibility check:** after `./gradlew clean` and a full rebuild, the APK SHA-256 was
**identical** to the one installed for G6. The G6 run therefore used exactly what the unchanged
source produces.

---

## 3. Post-Wave-0 daemon identity

| Field | Value |
| --- | --- |
| HEAD | `16aa7f0` |
| Uncommitted worktree diff SHA-256 | `76b4f20ebf1994449d1c1b7b61102e237372a6aebffe2fe1ddfff4f1cdbdd1bc` |
| Binary | `desktop/target/debug/anyflowd` |
| Binary SHA-256 | `3512c6c1c21765ed3c385f772d7494890658f3f9e4c232baa135b67ea0af8a7a` |
| Fedora identity | `795fec0868ebef8c3d7ad3e775dc6ed0` · `DF65 D3E4 BA28 EDF9` · `key_backing=software` |
| Listening | port 55432, IPv4+IPv6, mDNS `_anyflow._tcp.local.` |

**Proof it is the post-Wave-0 binary, not the old one.** The binary's debug symbols contain
types from crates that did not exist before the refactor:

```
anyflow_runtime::state::DaemonState
anyflow_runtime::listener::accept_loop
anyflow_runtime::server::run<anyflow_linux::UnixControlListener>
anyflow_control::transport::BindError
anyflow_control::ClipboardPeerReport
```

Pre-Wave-0 all of this lived under `anyflow_daemon::`. The daemon was force-rebuilt
(`touch daemon/src/main.rs && cargo build -p anyflow-daemon`) immediately before the run.

**State migration on start:** the real store was `schema_version: 1` (a genuine pre-Wave-0
store). The daemon loaded it and reported the **same** `device_id` and fingerprint with
`key_backing=software`. Migration to v2 happened in place on first write.

---

## 4. G6 — battery.v1 · **PASS**

| Evidence | Value |
| --- | --- |
| Fedora received | `battery 70% (Charging, 40s old)` |
| Device ground truth (`adb dumpsys battery`) | `level: 71`, `USB powered: true` |
| Reverse direction | Android UI showed Fedora at **96%** |

The one-point difference is the battery ticking 70 → 71 during the reporting interval. Real
capability traffic in both directions, not inferred from tests.

---

## 5. G6 — clipboard.v1 Android → Fedora · **PASS**

1. Ordinary text copied in **Chrome** (a normal Android app) from a `<textarea>`, via the
   system context-menu "Copiar".
2. AnyFlow opened → **Send clipboard** → confirm screen showed `66 bytes` and
   `Direct connection · TLS 1.3, pinned` → **Send to Fedora**.
3. Daemon: `clipboard update … outcome="pending"` (auto-receive off, as configured).
4. `anyflow clipboard apply` → `applied 66 bytes to the clipboard`.
5. Pasted into a **real GTK4 application** (`Gdk.Clipboard.read_text_async` into a
   `Gtk.TextView` — the same path `gnome-text-editor` uses for Ctrl+V).

| | bytes | SHA-256 |
| --- | --- | --- |
| Sent | 66 | `0105ded8b4a941314f45fe0757f320ad382dcc10b60e9877f88297ee4c1cf5a9` |
| Pasted in GTK4 app | 66 | `0105ded8b4a941314f45fe0757f320ad382dcc10b60e9877f88297ee4c1cf5a9` |

**Byte-identical.** Payload included a decomposed `e`+U+0301, `Ω`, and `日本語`.

---

## 6. G6 — clipboard.v1 Fedora → Android · **PASS**

1. `anyflow clipboard send` → `sent 67 bytes … (sensitive=false)`.
2. Android notification: *"Clipboard received from Fedora — 67 bytes of text. Open AnyFlow to
   copy it."* — **byte count only, no content**.
3. Notification **Copy** action → app → pending-clip card *"Clipboard from Fedora / 67 bytes"* →
   **Copy** → toast *"Copied 67 bytes to your clipboard."*
4. Pasted into **Chrome** on the device.

| | bytes | SHA-256 |
| --- | --- | --- |
| Sent | 67 | `4bc2baedfe6399418fdba3e64a263d9df382ae5f748cd7ed861df79c5a38e41a` |
| Pasted in Chrome | 67 | `4bc2baedfe6399418fdba3e64a263d9df382ae5f748cd7ed861df79c5a38e41a` |

**Byte-identical.** Payload included `naïve`, `€`, `한국어`.

---

## 7. G6 — files.v1 Android → Fedora · **PASS** (real Sharesheet)

Path: **Files app → long-press → Compartilhar → Android system Sharesheet → "Send with
AnyFlow" → Send.** The confirm dialog showed the filename, target, and the pinned fingerprint
`DF65 D3E4 BA28 EDF9`.

| Field | Value |
| --- | --- |
| Transfer | `78798b48` |
| State | `completed` |
| Size | **11221 bytes** (exact) |
| SHA-256 | `8af08d11b933d2d0ac0e35e812f814bb462575880f9b376291a9514bd090f08a` (exact) |
| Destination | `/home/yuri/Downloads/AnyFlow/g6-sheet.txt` |

Daemon: `incoming file offer` → accepted → `received, verified and stored … bytes=11221`.

A second transfer over the in-app SAF picker (`g6-a2f.txt`, 22910 bytes,
`25cdb1d8…`) also completed byte-identically.

---

## 8. G6 — files.v1 Fedora → Android · **PASS**

`anyflow send` from the CLI. Android showed an in-app **"Incoming file / g6-f2a.txt / 14,8 KB ·
from DF65 D3E4 BA28 EDF9"** card requiring **Accept** — receiver approval is enforced on
Android.

| Field | Value |
| --- | --- |
| Transfer | `08273754` |
| CLI | `100% 14.8 KiB / 14.8 KiB` → `Sent. g6-f2a.txt` |
| Daemon | `sent; awaiting the receiver's verdict` → `the peer confirmed it stored the file` |
| App log | `received and verified 08273754 as g6-f2a.txt` |
| Size on device | **15110 bytes** (exact) |
| SHA-256 on device | `42501125730bdcad16618f6161f7213689c0e435e1ced4327bcc7f6e338a00a5` (exact) |
| Destination | `/storage/emulated/0/Download/AnyFlow/g6-f2a.txt` |
| MediaStore publication | `_display_name=g6-f2a.txt, _size=15110` ✅ |

---

## 9. G6 verdict — **PASS**

| Requirement | Result |
| --- | --- |
| 1. Daemon starts | ✅ |
| 2. Existing state migration remains valid | ✅ schema 1 loaded, same device id + fingerprint |
| 3. Android discovers / reconnects | ✅ mDNS advertised; reconnected after drops and after instrumentation |
| 4. Legitimate pairing / re-pairing | ✅ fresh Android identity, QR scanned, explicit confirm |
| 5. Session establishes | ✅ |
| 6. TLS 1.3 negotiated | ✅ `TLSv1.3 / TLS_AES_256_GCM_SHA384`; TLS 1.2 refused with alert 70 |
| 7. SPKI pinning enforced | ✅ live cert SPKI SHA-256 == advertised fingerprint, exactly |
| 8. Effective capabilities correct | ✅ `["battery.v1","clipboard.v1","files.v1"]` |
| battery / clipboard ×2 / files ×2 | ✅ all with real traffic |

The unmodified Android application interoperates with the refactored daemon.

**Pairing detail.** The previous Android identity no longer existed (app not installed), so a
legitimate fresh pairing was performed: new identity `532a8ced416c25ace89a25d915f3c0f4`,
fingerprint `4E0D C7A0 AC32 F8AF`. The phone's own Device screen displayed Fedora's fingerprint
as `DF65 D3E4 BA28 EDF9` — matching the desktop exactly — with the text *"Paired directly over
your local network and pinned to this key. A different key cannot impersonate it."*

---

## 10. wl-clipboard capability detected

Established empirically, not assumed from the distro name:

```
$ wl-copy --version   →  wl-clipboard 2.2.1   (wl-clipboard-2.2.1^git20251124.e808203-2.fc44)
$ wl-copy --help      →      --sensitive      Hint that the content is sensitive.
```

Runtime detection agreed:

```
clipboard backend backend="wl-clipboard" available=true
  watch=XFIXES on the Xwayland CLIPBOARD selection  sensitive="yes"
```

→ **SUPPORTED BACKEND CASE.** Support had not disappeared; the real sensitive round trip was
executed rather than simulated.

*Note:* `wl-paste --watch` is unavailable because GNOME/Mutter does not implement the
`wlr-data-control` protocol, and the backend correctly falls back to the XFIXES/Xwayland bridge
and says so in the log. Correct, honest behaviour — not a defect.

---

## 11. G10 — sensitive Fedora → Android · **PASS**

```
$ anyflow clipboard send --sensitive …
sent 40 bytes of clipboard text to 4E0D C7A0 AC32 F8AF (marked sensitive)

daemon: clipboard update sent … event=022aea65 bytes=40 sensitive=true
```

| Check | Result |
| --- | --- |
| Hint transmitted | ✅ `sensitive=true` on the wire |
| Android received the hint | ✅ notification: *"40 bytes of text… **Marked sensitive.**"* |
| Sensitive handling in-app | ✅ card: *"40 bytes · **marked sensitive**"* |
| No content in UI | ✅ full UI-tree dump contained none of the secret |
| Content integrity | ✅ 40 bytes, `46e0393086bd096cda2cdb292701bd12de2d6332665f87a322a0c99c150f7db6` — byte-identical when pasted into Chrome |

---

## 12. G10 — sensitive Android → Fedora · **PASS**

The Android clipboard carried a genuine `EXTRA_IS_SENSITIVE` mark. The app's own read detected
it independently:

- **Clipboard preview masked**: `••••••••••••` — the content was *not* rendered
- *"This clipboard is marked sensitive"* / *"The app you copied from marked this text as sensitive"*
- A **second explicit confirmation**: *"Send 40 bytes to Fedora?"* — byte count only, no content
- The UI-tree dump leaked none of the secret

Fedora side, after `anyflow clipboard apply`:

```
$ pgrep -a wl-copy
94329 wl-copy --type text/plain;charset=utf-8 --sensitive
```

**Direct proof the backend used its supported sensitive mechanism.**

| | bytes | SHA-256 |
| --- | --- | --- |
| Expected | 40 | `46e0393086bd096cda2cdb292701bd12de2d6332665f87a322a0c99c150f7db6` |
| Pasted back on Fedora | 40 | `46e0393086bd096cda2cdb292701bd12de2d6332665f87a322a0c99c150f7db6` |

**Byte-identical, privacy hint semantics intact.**

**Supported harness confirmation.** `ClipboardTarget.kt` states that every assertion about
`EXTRA_IS_SENSITIVE` lives in an instrumented test on a real device. Those ran and passed on the
SM-X620:

- `a_sensitive_clip_from_another_app_is_detected`
- `extra_is_sensitive_is_set_only_when_the_clip_is_marked_sensitive`
- `extra_is_remote_device_is_set_on_api_34_and_above`
- `setPrimaryClip_applies_a_remote_clip`

---

## 13. G10 verdict — **PASS**

Full round trip on real hardware with the real backend: content hashes match in both
directions, sensitive metadata is carried and honoured at both ends, no content in any log, no
persistence.

Backing test evidence — all 9 previously-blocked `real_backend` tests now pass on an unlocked
seat:

```
test result: ok. 8 passed; 0 failed  (7.08s)   [+ the 9th, run separately: ok, 0.86s]
  a_sensitive_write_still_round_trips ... ok
  detection_describes_this_session_accurately ... ok
  the_real_clipboard_preserves_unicode_and_multiline_text_byte_for_byte ... ok
  …
```

---

## 14. Locked-seat regression · **PASS**

Plain `wl-copy` verified working while unlocked first. Then the desktop was deliberately locked
once (`loginctl lock-session 2`, `LockedHint=yes`) and an apply attempted:

| Property | Result |
| --- | --- |
| No hang | ✅ bounded at **5.0 s** (`BACKEND_TIMEOUT`) |
| Clear error | ✅ *"the clipboard did not respond in time. On GNOME Wayland this normally means the session is locked: wl-copy and wl-paste cannot obtain a seat behind the lock screen."* |
| Pending clip retained | ✅ `caches 2 event id(s)`, `last result pending` |
| No content logged | ✅ the `WARN` line carries the error only |
| Recovery | ✅ unlocked → `applied 40 bytes`, again via `wl-copy … --sensitive` |

Session unlocked afterwards (`LockedHint=no`). This is regression evidence, not a requirement
that clipboard work behind the GNOME lock.

---

## 15. State migration regression · **PASS**

Run against a **copy**; the real store was backed up first and never destructively tested.

| Property | Before | After |
| --- | --- | --- |
| `schema_version` | 1 | **2** |
| `device_id` | `795fec0868ebef8c3d7ad3e775dc6ed0` | identical |
| Certificate | — | identical |
| Fingerprint | `DF65 D3E4 BA28 EDF9` | identical |
| `key_backing` | *(absent)* | `software` |
| Peers | 4 | 4, ids identical |
| Revocations | 2 revoked | retained, still denied |
| Grants | per-peer | identical |
| Clipboard policies | per-peer | identical |
| Settings | — | identical |
| `identity.key` | `567137cb…` | **byte-identical** |

### Failure cases — no failure other than true NOT_FOUND created a new state

| Case | State reported | New state created? |
| --- | --- | --- |
| unreadable `state.json` | `IDENTITY_LOST` | No |
| unreadable key | `IDENTITY_LOST` | No |
| malformed `state.json` | `IDENTITY_CORRUPTED` | No |
| 0644 key | `IDENTITY_CORRUPTED` + `chmod 600` remedy | No |
| future schema (v99) | `IDENTITY_CORRUPTED`, refuses downgrade | No |
| unknown `key_backing` | `IDENTITY_CORRUPTED` | No |
| bad certificate base64 | `IDENTITY_CORRUPTED` | No |
| state without key | `IDENTITY_LOST` | No |
| key without state | `IDENTITY_LOST` | No |
| traversable (0755) data dir | *hardened to 0700*, files byte-identical | No |
| **empty dir (true NOT_FOUND)** | — | **Yes (correct)** |

The traversable-dir case is deliberate and unit-tested (`harden_makes_a_loose_directory_private`):
a loose *directory* is tightened, a loose *key* is refused.

---

## 16. Identity regression · **PASS** — 52 tests

| Suite | Count |
| --- | --- |
| `core/tests/identity_and_store.rs` | 24 |
| `core/tests/identity_seam.rs` | 8 |
| `core/tests/identity_states.rs` | 20 |

Named properties, each with the test that proves it:

| Property | Test |
| --- | --- |
| private-key-byte-free provider signer | `a_non_exportable_identity_completes_a_pinned_handshake_as_the_{server,client}`, `both_ends_non_exportable_still_works` |
| software Linux identity | `store_generates_an_identity_on_first_run_and_reloads_it` |
| TLS client signer | `…_as_the_client` |
| TLS server signer | `…_as_the_server` |
| pinned rejection stays pinned rejection | `pinning_still_rejects_a_different_identity_through_the_resolver_path` |
| temporary failure does not regenerate | `a_transient_io_failure_is_temporarily_unavailable_and_writes_nothing` |
| corrupted vs lost stay distinct | `a_malformed_state_file_is_corrupted_and_never_overwritten`, `a_key_with_no_state_beside_it_is_fatal_rather_than_a_first_run` |
| no silent identity replacement | `every_fault_leaves_the_identity_intact_and_only_absence_creates`, `only_not_created_may_create` |

---

## 17. Filename regression · **PASS** — 23 tests

`RESERVED_STEMS` (verified in source) covers every named case:

```
con, prn, aux, nul, com1…com9, lpt1…lpt9, conin$, conout$
WINDOWS_ILLEGAL = < > : " | ? *
```

| Named input | Covered by |
| --- | --- |
| `:` and ADS-shaped names | `a_colon_cannot_survive_and_no_alternate_data_stream_can_be_named` |
| U+202E | `a_right_to_left_override_cannot_disguise_an_extension` |
| Unicode `Cf` | `invisible_format_characters_go_even_when_they_are_not_bidi`, `every_bidi_control_goes_not_only_the_famous_one` |
| CON / PRN / AUX / NUL | `windows_device_names_are_rejected` (`CON`, `con.txt`, `NUL.jpg`, `lpt9.tar.gz`) |
| CONIN$ / CONOUT$ | `the_console_handles_are_reserved_too` (also `NUL:`, `con:`) |
| COM1–COM9 / LPT1–LPT9 | `RESERVED_STEMS` + `windows_device_names_are_rejected` |
| slash / backslash | `a_unix_traversal…`, `a_windows_traversal…`, `the_result_never_contains_a_separator` |
| trailing dots / spaces | `trailing_dots_and_spaces_go` |

**No protocol change.**

---

## 18. Control transport regression · **PASS**

| Check | Result |
| --- | --- |
| `anyflow-linux` Unix control socket tests | 6/6 ✅ |
| `AlreadyOwned` behaviour | `binding_twice_reports_already_owned_and_not_a_generic_io_error` ✅ |
| Stale socket replaced | `a_stale_socket_file_is_replaced_rather_than_refused` ✅ |
| Owner-only mode | `a_bound_socket_is_owner_only` ✅ |
| CLI control requests | ✅ exercised throughout (`status`, `devices`, `pair`, `grant`, `send`, `transfers`, `clipboard …`) |
| GUI control requests | ✅ live GUI connected to `wayland-0`; **53 distinct socket inodes in 14 s** vs ~7 steady — a new control connection per 2 s refresh. GUI unit tests 11/11 |

### `server::bind` never unlinks a live AnyFlow listener — verified live

With the real daemon holding `/run/user/1000/anyflow/control.sock`, a second daemon was started:

```
Error: the local control endpoint is already owned by another process:
  /run/user/1000/anyflow/control.sock is accepting connections. AnyFlow will not bind an
  alternative name, because a client that has to search for the agent can be answered by
  whatever squatted on the first one

inode/mtime before: 523 1788222059
inode/mtime after : 523 1788222059      → LIVE LISTENER PRESERVED
```

The original daemon kept serving.

---

## 19. Portable-boundary regression · **PASS**

```
$ cargo build --no-default-features \
    -p anyflow-proto -p anyflow-core -p anyflow-control \
    -p anyflow-capability-clipboard -p anyflow-capability-files -p anyflow-capability-battery
    Finished `dev` profile in 4.82s
```

`core/tests/portable_boundary.rs` — 7/7:

- `the_portable_crates_declare_unsafe_code_forbidden`
- `the_adapter_and_binary_crates_at_least_deny_unsafe_code`
- `no_crate_actually_uses_unsafe_today`
- `the_portable_crates_name_no_platform_outside_a_feature_gated_module`
- `the_security_critical_files_carry_no_platform_arm_at_all`
- `each_exception_is_actually_behind_a_feature`
- `every_declared_exception_still_exists`

The source boundary is clean of Linux-specific `std::os` imports where forbidden.

**No Windows MSVC claim is made from Linux.** `POC-CORE-04` is handled separately.

---

## 20. Rust final counts

Run sequentially, in the required order:

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | exit 0 — clean |
| `cargo build --workspace` | exit 0 |
| `cargo test --workspace` | exit 0 — **366 passed, 0 failed, 9 ignored** (40 suites) |
| `cargo clippy --workspace --all-targets` | exit 0 — **0 warnings, 0 errors** |

The 9 ignored are the `real_backend` clipboard tests. Run separately against the real unlocked
seat: **9 passed, 0 failed**.

**Total Rust tests executed: 375 passed, 0 failed.**

> The GUI crate needs `gtk4-devel`, which is not installed system-wide; the documented no-root
> prefix (`. ~/.local/gtk4-prefix/ENV.sh`) was sourced for every cargo invocation.

---

## 21. Android JVM counts

```
./gradlew clean                        exit=0
./gradlew testDebugUnitTest            exit=0
./gradlew :app:assembleDebug           exit=0
./gradlew :app:assembleDebugAndroidTest exit=0
```

**22 classes · 232 tests · 0 failures · 0 errors · 0 skipped → 232 PASSED.**

---

## 22. Android instrumented counts

```
./gradlew :app:connectedDebugAndroidTest   exit=0
Starting 21 tests on SM-X620 - 16
Finished 21 tests on SM-X620 - 16          (0 skipped) (0 failed)
BUILD SUCCESSFUL
```

**21 tests · 0 failures · 0 skipped → 21 PASSED**, across
`ClipboardInstrumentedTest` (9), `ClipboardPersistenceTest` (3), `DeviceIdentityTest` (4),
`DownloadsTest` (5). Including:

- `thePrivateKeyIsNotExportable`
- `theIdentityKeyCanProduceATls13CertificateVerifySignature`
- `clipboard_content_is_never_written_to_the_trust_store_or_anywhere_else`
- `a_sensitive_clip_from_another_app_is_detected`

**The Android Keystore identity survived instrumentation.** The pairing
(`532a8ced…` / `4E0D C7A0 AC32 F8AF`) remained valid and re-established a session with all
three capabilities afterwards. No re-pair was needed and no private material was copied.

---

## 23. TLS / SPKI audit — unchanged

| Control | Evidence |
| --- | --- |
| TLS 1.3 only | `TLS13_ONLY = &[&rustls::version::TLS13]`, applied at both `tls.rs:293` (client) and `:344` (server). Live: negotiated `TLSv1.3 / TLS_AES_256_GCM_SHA384`; `-tls1_2` refused with **alert 70 (protocol_version)** |
| SPKI SHA-256 | Live cert SPKI digest `df65d3e4ba28edf920beaf8e41701bbc00563a6448f301ff64cc8d140e14334a` == the fingerprint the daemon advertises — **exact match** |
| ECDSA P-256 | Live cert: `Public Key Algorithm: id-ecPublicKey`, `NIST CURVE: P-256`, `ecdsa-with-SHA256` |
| Explicit pairing | Single-use QR token + `ConfirmRequest` with fingerprint shown on both ends |
| Unknown peer fail-closed | Live: `peer sent no certificates` (anonymous TLS probes), `pairing failed: not in pairing mode` |
| Capability grants | Independent of what a peer advertises; enforced per session |
| Revocation | 2 revoked peers retained and denied: *"unavailable — this device's pairing was revoked"* |
| No trust from mDNS | `runtime/src/mdns.rs` never touches the trust store |
| No trust-all | 0 hits for `danger_accept_invalid`, `NoCertificateVerification`, `insecure`, `allow_invalid`. One production `dangerous()` — installing the pinned verifier |
| No cleartext protocol | Every accept goes through `TlsAcceptor` under `TLS_HANDSHAKE_TIMEOUT` |
| No private key export introduced | `private_key_pkcs8_der()` is **pre-existing** (present in `HEAD`). Wave 0 *added* `signing_key() -> Arc<dyn SigningKey>` — an opaque signer returning no key bytes. The export surface **shrank** |

**Protocol unchanged:** `git status --porcelain protocol/` empty; `desktop/proto/src/` untouched.
Only `desktop/proto/Cargo.toml` changed, and only to replace `[lints] workspace = true` with a
per-crate `unsafe_code = "forbid"` (ARCH-010).

---

## 24. Logging / persistence audit — clean

Searched the full daemon log for every payload used this run — `hunter2`, `9f1c7b`, `3d8e5a`,
`café`, `привет`, `7f3a9c`, `b2e5d1`, `c4a7f2`, `日本語`, `한국어`, `SENTINEL`:

**0 occurrences of every one.**

Every clipboard log line is metadata only:

```
clipboard update      peer=4E0D…  event=424af60e  outcome="pending"
clipboard update sent peer=4E0D…  event=72baabf6  bytes=67  sensitive=false
clipboard update sent peer=4E0D…  event=022aea65  bytes=40  sensitive=true
```

`state.json`: 0 occurrences of any payload. Top-level keys are
`certificate_der_b64, device_id, key_backing, peers, schema_version, settings` — no clipboard
content field; peers carry `clipboard_policy` (direction flags) only.

Android surfaces show byte counts and never content: notification *"40 bytes of text… Marked
sensitive."*, card *"40 bytes · marked sensitive"*, send prompt *"Send 40 bytes to Fedora?"*,
and the sensitive preview is masked to `••••••••••••`.

---

## 25. New bugs found

### B-1 — A failed Sharesheet offer leaves the dialog stuck on "Sending…" forever (real bug)

`SendActivity.startSend` discards the `Result`:

```kotlin
private fun startSend(peer: TrustStore.TrustedPeer, uri: Uri) {
    ConnectionService.start(this)
    lifecycleScope.launch {
        app.files.offer(peer.fingerprint, uri)   // ← no .onFailure
    }
}
```

`FileTransferManager.offer` can fail for five distinct reasons (not authorized, not connected,
too many transfers, no safe name, `measure()` throws). Every one is swallowed: the button stays
on **"Sending…"** indefinitely with no toast and nothing in logcat. Observed twice with a URI
the app could not open. Every sibling path (`startTextSend`, `applyClip`, `sendClipboard`) has
an `.onFailure` that surfaces a toast — this one is the outlier.

*Suggested fix:* add `.onFailure { showError(it.message ?: "Could not send that file.") }`.

### B-2 — `real_backend` test calls `wl-copy --clear` unbounded, so it hangs forever on a locked seat (test-only)

`an_empty_clipboard_is_bounded_and_classified_never_an_unexplained_failure` uses a blocking
`std::process::Command::status()` with no timeout, unlike the production backend which wraps
every invocation in `BACKEND_TIMEOUT` with `kill_on_drop`. With the seat unavailable,
`wl-copy --clear` never returns and the test hangs indefinitely instead of failing with the
clear, actionable message the other 8 tests give. This is what blocked G10's documented step
until the seat was unlocked.

Confirmed narrow: on an **unlocked** seat the same test passes in 0.86 s. Production code is
correctly defended; only the test harness is exposed.

*Suggested fix:* wrap the clear in the same timeout the backend uses, and skip with the existing
"could not clear the clipboard; skipping" branch on timeout.

### Observations (not defects)

- **O-1** Capability grants take effect on the **next** session. After `anyflow grant`, the
  Android UI kept the old capability set (`capabilities=["battery.v1"]`) until reconnect. Correct
  and safe, but it reads as "clipboard is broken" until the session is re-established.
- **O-2** The Android per-peer **Clipboard** permission defaults **off** ("Nothing is granted
  automatically") — deliberate, but it means a desktop-side grant alone is not sufficient.
- **O-3** `wl-paste --watch` is unavailable on GNOME (no `wlr-data-control`); the backend falls
  back to the XFIXES/Xwayland bridge and logs the reason. Correct.
- **O-4** Environment: the GNOME idle lock (`idle-delay 300`, `lock-enabled true`) fires during
  long test runs and breaks clipboard tests mid-suite. `loginctl unlock-session 2` recovers it.
  A settings change to suppress this was attempted and declined by policy; no desktop settings
  were modified.
- **O-5** An untracked stray file sits in the repo root:
  `": add cross-platform expansion research and roadmap\""` (1698 bytes, a list of doc paths).
  It is debris from an earlier session's malformed `git commit -m`. **Left untouched.**

---

## 26. Files changed

Unchanged from precheck — this closeout modified **no source files**.

```
38 files changed, 2096 insertions(+), 323 deletions(-)
```

Plus the 5 pre-existing staged renames (0 insertions, 0 deletions) and 12 untracked paths
(`desktop/control/`, `desktop/runtime/`, `desktop/platform-linux/`, `desktop/core/src/platform/`,
`desktop/core/src/secret_store.rs`, `desktop/capabilities/files/src/sink.rs`, three new
`core/tests/*.rs`, `docs/sprints/wave-0-platform-abstraction.md`, and the stray file in O-5).

### Artifact / secret audit

| Pattern | Finding |
| --- | --- |
| `key`, `pem`, `p12`, `pfx`, `jks`, `keystore` | Only **source** files (`DeviceIdentity.kt`, `identity.rs`, …) and docs |
| `identity` files | `protocol/testdata/identity-{a,b}.der` — verified **X.509 certificates (public)**, pre-existing since `1887f1d` |
| `state.json`, trust stores | None in the tree; `.gitignore` covers `*.key`, `state.json`, `trust-store.json` |
| APK / AAB | None tracked or untracked; `.gitignore` covers `*.apk`, `*.aab` |
| Logs, temporary binaries, test captures | None — all under `/tmp` scratchpad, outside the repo |

Test artefacts left outside the repo, for your awareness: `~/Downloads/AnyFlow/g6-a2f.txt` and
`g6-sheet.txt` (received during G6). Files pushed to the device were removed. A backup of the
pre-migration real store is at
`…/scratchpad/backup-real-store/{state.json,identity.key}.orig`.

---

## 27. Git status (final)

```
$ git branch --show-current
feature/core-platform-abstraction-v1

$ git diff --check
(clean)

$ git status
Changes to be committed:
        renamed: desktop/daemon/src/control.rs  -> desktop/control/src/lib.rs
        renamed: desktop/daemon/src/listener.rs -> desktop/runtime/src/listener.rs
        renamed: desktop/daemon/src/mdns.rs     -> desktop/runtime/src/mdns.rs
        renamed: desktop/daemon/src/server.rs   -> desktop/runtime/src/server.rs
        renamed: desktop/daemon/src/state.rs    -> desktop/runtime/src/state.rs
Changes not staged for commit:  38 modified
Untracked files:                12 paths
```

**No commit. No push. Nothing additionally staged.** The five rename entries were left exactly
as they were.

---

## 28. Wave 0 gate table

| Gate | Scope | Result |
| --- | --- | --- |
| **G1** | Schema fixture test edits reviewed | ✅ Accepted — required by `SCHEMA_VERSION`/`key_backing` migration; no assertion weakened |
| **G6** | Unmodified Android app ↔ post-Wave-0 daemon on real hardware | ✅ **PASS** |
| **G10** | `sensitive_hint` hardware round trip, unlocked clipboard seat | ✅ **PASS** |
| — | Locked-seat regression | ✅ PASS — bounded, clear, clip retained, no leak |
| — | State migration regression (v1 → v2, 11 failure cases) | ✅ PASS |
| — | Identity regression (52 tests) | ✅ PASS |
| — | Filename regression (23 tests) | ✅ PASS |
| — | Control transport + live listener ownership | ✅ PASS |
| — | Portable boundary (`--no-default-features`, 7 tests) | ✅ PASS |
| — | Rust full regression | ✅ 375 passed / 0 failed; fmt + clippy clean |
| — | Android JVM | ✅ 232 passed / 0 failed |
| — | Android instrumented | ✅ 21 passed / 0 failed |
| — | Security audit | ✅ Every control unchanged; export surface shrank |
| — | Protocol | ✅ Unchanged |
| **PLAT-DEC-013** | Fail-closed sensitive behaviour | ✅ Not exercised as a refusal — this backend **supports** `--sensitive`, so the supported path ran. Ordinary writes functional throughout |
| **POC-CORE-04** | Windows MSVC runner | ✅ **PASS — 2026-09-01**, on a GitHub-hosted `windows-2025-vs2026` runner, `host: x86_64-pc-windows-msvc` ([run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649)). Evidence: [sprint report §17](../../reports/foundation/wave-0-platform-abstraction.md) |

---

# WAVE 0 LOCAL GATES CLOSED

Both formal blockers this run was scoped to close — **G6** and **G10 partial** — are closed with
real hardware evidence, and every local regression passes.

> **Superseded on 2026-09-01.** The one item this report left open, `POC-CORE-04`, has since
> passed on a real Windows MSVC runner. **Wave 0 is now CERTIFIED** — see
> [the sprint report](../../reports/foundation/wave-0-platform-abstraction.md) §§17, 24, 26, which is the
> authoritative gate record. This document remains the evidence for **G6** and **G10**.
