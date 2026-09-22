# Wave 0 — POC-CORE-04 · Phase 2 final certification

**Date:** 2026-09-01 · **Branch:** `feature/core-platform-abstraction-v1` · **No commit, no push.**

> # POC-CORE-04 PASS
> # WAVE 0 CERTIFIED

---

## 1. Certification-candidate commit

```
cfd33f6fa5bb525fa1a650574f6fa870fdac2708
refactor: establish cross-platform core platform boundary
Yuri Converso Sismotto · Tue Sep 1 00:11:31 2026 -0300
```

`git rev-parse HEAD` == `git rev-parse origin/feature/core-platform-abstraction-v1`. Local and
remote are the same commit, and it is the commit CI-001 ran against — verified by `headSha`, not
assumed. It carries the whole Wave 0 refactor (55 files) **and**
`.github/workflows/portable-windows-msvc.yml`, which is why a single run can be evidence for the
gate.

## 2. GitHub Actions evidence — the real run

| | |
| --- | --- |
| **Run ID** | `33465365649` |
| **URL** | https://github.com/yurisismotto/anyflow/actions/runs/33465365649 |
| **Workflow** | `.github/workflows/portable-windows-msvc.yml` — "Portable core · Windows MSVC" |
| **Job** | `cargo check · x86_64-pc-windows-msvc` (`99723987185`) |
| **Event / branch** | `push` on `feature/core-platform-abstraction-v1` |
| **Head SHA** | `cfd33f6fa5bb525fa1a650574f6fa870fdac2708` |
| **Conclusion** | **`success`** |
| **Duration** | 2026-09-01 03:12:26Z → 03:14:51Z (2 m 23 s) |
| **Steps** | **12 of 12 `success`** — none skipped, none `continue-on-error`, no conditional step |

It is the only workflow run in the repository. Nothing was re-run, and no earlier failure was
superseded.

**Log audit.** The full 139 KB job log was downloaded and searched: **0** `##[error]`/`##[warning]`
annotations, **0** rustc warnings, **0** `error[E…]` / `could not compile` / `failed` lines. The
workflow file contains no `continue-on-error` and no `if:` guard, so no step could pass by being
skipped.

### Runner

| | |
| --- | --- |
| Runner image | `windows-2025-vs2026`, version `20260824.214.3` |
| Operating system | Microsoft Windows Server 2025 · `10.0.26100` · Datacenter |
| Runner agent | `2.337.0`; image provisioner `20260819.586` |
| C toolchain | **Visual Studio Enterprise 2026**, `18.9.12112.369` (via `vswhere`) |

### Toolchain — the MSVC assertion

```
rustc 1.98.0 (88d9e12ae 2026-08-18)
binary: rustc
commit-hash: 88d9e12ae178fab0fb5cc050a94da85685d449ea
commit-date: 2026-08-18
host: x86_64-pc-windows-msvc
release: 1.98.0
LLVM version: 22.1.8

cargo 1.98.0 (797e8a9bc 2026-08-05)

active toolchain: stable-x86_64-pc-windows-msvc
                  (overridden by 'D:\a\anyflow\anyflow\desktop\rust-toolchain.toml')

targets installed: i686-pc-windows-msvc, x86_64-pc-windows-gnu, x86_64-pc-windows-msvc

effective host triple: x86_64-pc-windows-msvc
```

The job asserts the host triple by string equality and exits 1 on anything else, with an explicit
`::error::A -gnu toolchain is NOT an acceptable substitute for POC-CORE-04.` It also fails if
`x86_64-pc-windows-msvc` is absent from the installed targets. **The `-gnu` target is present on
the image but was not used** — the host is MSVC and every cargo invocation names
`--target x86_64-pc-windows-msvc`.

## 3. Exact commands executed on the runner

Working directory `desktop/`, shell `pwsh` (PowerShell 7).

**The gate — 28 §10.3 verbatim, plus `--locked`:**

```
cargo check --locked --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-proto -p anyflow-core -p anyflow-control \
  -p anyflow-capability-clipboard -p anyflow-capability-files -p anyflow-capability-battery
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 07s
```

**Codegen and link — where `ring`'s MSVC objects actually have to work:**

```
cargo build --locked --no-default-features --target x86_64-pc-windows-msvc -p …(same six)
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.09s
```

**Test targets:**

```
cargo test --locked --no-run --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-proto -p anyflow-control \
  -p anyflow-capability-clipboard -p anyflow-capability-files -p anyflow-capability-battery
→ Finished `test` profile in 20.68s — 9 executables

cargo test --locked --no-run --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-core --lib --test identity_seam --test pairing --test portable_boundary --test protocol
→ Finished `test` profile in 12.04s — 5 executables
```

Every invocation used `--locked`; `Cargo.lock` could not drift on the runner.

## 4. Portable package set — six crates

`anyflow-proto`, `anyflow-core`, `anyflow-control`, `anyflow-capability-clipboard`,
`anyflow-capability-files`, `anyflow-capability-battery`.

A `cargo metadata` step asserted all six still exist before the gate ran:
`portable set present: anyflow-proto, anyflow-core, anyflow-control, anyflow-capability-clipboard,
anyflow-capability-files, anyflow-capability-battery`. A rename or deletion fails the job rather
than silently shrinking the gate.

`anyflow-runtime` **deliberately excluded**, per 28 §10.3 — `mdns-sd`'s Windows behaviour is
V-12 / POC-WIN-02. `anyflow-linux`, `anyflow-daemon`, `anyflow-cli`, `anyflow-gui` are adapters and
binaries, out of scope.

## 5. Dependency boundary — measured on the resolved graph

```
boundary clean: 87 packages, no platform crate, no platform feature.
```

Measured with `cargo tree --locked --no-default-features --target x86_64-pc-windows-msvc -e normal`,
**not** grepped from source — because the failure POC-CORE-01 actually hit was Cargo feature
unification switching `unix-fs` back on through a capability crate's default dependency on
`anyflow-core`, which a source grep cannot see.

- All four `anyflow-*` portable nodes carry `[]` — `unix-fs`, `linux-backends` and `upower` all off.
- No forbidden package: no `anyflow-linux`/`-daemon`/`-runtime`/`-gui`/`-cli`, no `x11rb`, `zbus`,
  `gtk4`, `libadwaita`, `glib`, `wayland-*`, `smithay-*`, `mdns-sd`, `nix`, `rustix`, **no `libc`**.
- `libc` appears only as a **feature name** on `tokio v1.53.1`, which is why the check matches
  package names rather than whole lines.
- Windows backends resolved as expected: `windows-sys v0.61.2` (`Win32_Networking_WinSock`,
  `Win32_System_Pipes`, `Win32_Storage_FileSystem`, …), `mio v1.2.2`, `socket2 v0.6.5`.

**87 packages is exactly the count the Linux dry run predicted.** The graph the runner built is the
graph that was reviewed pre-push.

## 6. `ring` / MSVC

```
D:\a\anyflow\anyflow\desktop\target\x86_64-pc-windows-msvc\debug\build\ring-ea9757050f2935d9\out\ring_core_0_17_14_.lib
D:\a\anyflow\anyflow\desktop\target\x86_64-pc-windows-msvc\debug\build\ring-ea9757050f2935d9\out\ring_core_0_17_14__test.lib
```

`ring v0.17.14 [alloc, default, dev_urandom_fallback]` and `rustls v0.23.43 [ring, std]` are in the
resolved graph, and `ring`'s C sources were compiled to MSVC `.lib` objects under the MSVC target
directory by Visual Studio Enterprise 2026's toolchain.

**`ring` was not replaced and no crypto backend was substituted.** This is the specific thing
POC-CORE-01's `-gnu` result could not establish (V-10: different C runtime, different `ring` build
path, MSVC libraries not redistributable onto a Linux host).

## 7. Test compile result

14 MSVC test executables linked under `target\x86_64-pc-windows-msvc\debug\deps\`:

| Crate | Executables |
| --- | --- |
| `anyflow-proto` | lib |
| `anyflow-control` | lib |
| `anyflow-capability-battery` | lib |
| `anyflow-capability-clipboard` | lib, `logging`, `loops`, `real_backend`, `security` |
| `anyflow-capability-files` | lib |
| `anyflow-core` | lib, `identity_seam`, `pairing`, `portable_boundary`, `protocol` |

`--no-run`: they are compiled and linked, not executed. That is the PoC's scope — a compile-check
boundary test, not a Windows runtime test.

**The two excluded `anyflow-core` test targets** — `identity_and_store.rs` and `identity_states.rs`
— test the Unix filesystem adapter itself (`Store::open`, `platform::unix_fs`,
`probe_identity_at`), which does not exist when `unix-fs` is off. A preceding guard step compares
`core/tests/*.rs` against the recorded set of six files and **fails the job if it differs**, so a
new core test cannot slip past the Windows gate unnoticed. No assertion was changed, no `cfg` was
added, no `required-features` was written into a production manifest.

## 8. `unsafe_code` policy (ARCH-010)

```
unsafe policy intact: forbid in the portable/security crates, deny in the adapters.
```

`forbid` verified in `proto`, `core`, `control`, `runtime`, `capabilities/{clipboard,files,battery}`;
`deny` in `platform-linux`, `daemon`, `cli`, `gui`.

## 9. POC-CORE-04 verdict

Against the official criteria — [21 §POC-CORE-04](../../../docs/research/platform-expansion/21-POC-MASTER-PLAN.md)
and [28 §10.3](../../../docs/research/platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md), **not redefined**:

| Official criterion | Evidence |
| --- | --- |
| **Q** — does `cargo check` succeed for the six portable crates on `x86_64-pc-windows-msvc`? | Yes. That exact command, step 6, `Finished` in 1 m 07 s |
| **Minimal scope** — `--no-default-features`, the six named crates, that target | Exactly those, on every invocation |
| **On a Windows runner** (a Linux cross-compile is not a valid gate — `ring` needs MSVC, V-10) | `windows-2025-vs2026`; job asserts `host: x86_64-pc-windows-msvc` |
| **Success = clean**, `anyflow-runtime` deliberately excluded | Clean: 0 errors, 0 warnings. `anyflow-runtime` excluded |
| **Failure = any `std::os::unix` leak or unexpected transitive Unix-only dependency** | Neither: the compile catches the first, the resolved-graph check the second (87 packages, 0 forbidden) |
| **Output = the gate that becomes CI-001** | This *is* CI-001 — permanent, fail-closed, on PRs to `main`/`develop` |
| **VM?** — GitHub-hosted `windows-latest` has MSVC | Confirmed: VS Enterprise 2026 present, `ring` built by it |

> ## **POC-CORE-04 = PASS** — 2026-09-01

## 10. Wave 0 gate table — final

Official gates, [28 §12](../../../docs/research/platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md):

| # | Gate | Result | Evidence |
| --- | --- | --- | --- |
| **G1** | 309 existing tests pass, **unmodified** | ⚠️ **PASS, accepted** — 2 schema-fixture edits | Sprint §8. `SCHEMA_VERSION` bump + `key_backing` are mandated by the spec's own §7; no assertion weakened, both declared |
| **G2** | new tests from §10.2 pass | ✅ PASS — +63 | Sprint §18 |
| **G3** | Windows compile gate for the six portable crates | ✅ **PASS** | **Run 33465365649**, `-msvc` on a Windows runner |
| **G4** | no `std::os::unix` in `anyflow-core` / capability crates | ✅ PASS | `core/tests/portable_boundary.rs` (7 tests) + the runner's graph check |
| **G5** | GUI and CLI build without `anyflow-daemon` | ✅ PASS | `cargo tree \| grep -c anyflow-daemon` → 0 for both |
| **G6** | **unmodified Android app pairs, connects, sends, receives** | ✅ **PASS** | SM-X620: pair, `battery.v1`, `clipboard.v1` both ways, `files.v1` both ways (real Sharesheet); 21 instrumented tests |
| **G7** | pre-Wave-0 `state.json` upgrades in place | ✅ PASS | Real schema-1 store: same `device_id`, same fingerprint `DF65 D3E4 BA28 EDF9`, same 4 peers, revocations and grants intact |
| **G8** | identity fault injection refuses; `state.json` byte-identical | ✅ PASS — 20 tests | Sprint §9, §10 |
| **G9** | `anyflow status` reports `KeyBacking::Software` | ✅ PASS — `key  software-backed` | Sprint §18 |
| **G10** | Fedora hardware smoke incl. `sensitive_hint` | ✅ **PASS** | Re-run on an unlocked seat; `sensitive_hint` both directions; 9/9 `real_backend` |
| **G11** | clippy clean; `unsafe_code` lints as specified | ✅ PASS — 0 warnings | Locally, and re-asserted on the runner |
| **G12** | no `.proto` changed; `PROTOCOL_VERSION_MAX` unchanged | ✅ PASS | `git diff --stat` on `protocol/` and `*.proto` between `16aa7f0` and `cfd33f6`: **empty** |

**12 of 12 PASS. 0 FAIL. 0 NOT EXECUTED.** The three unwaivable gates — G1, G6, G12 — all pass.

P0 PoCs: **POC-CORE-01 PASS · POC-CORE-02 PASS · POC-CORE-03 PASS · POC-CORE-04 PASS.**

Regression, re-verified against the versioned Wave 0 documents:

| Suite | Result | Where recorded |
| --- | --- | --- |
| Rust | **375 passed / 0 failed** (366 + the 9 `real_backend` since run) | Sprint §1 records 366 + 9 ignored; the 9 ran on an unlocked seat |
| Android JVM | **232 passed / 0 failed** | Sprint §18 |
| Android instrumented | **21 passed / 0 failed / 0 skipped** (SM-X620) | Hardware closeout; now folded into sprint §18 |
| `cargo fmt` / `clippy` | clean / 0 warnings | Sprint §1, §24 |
| TLS / SPKI | unchanged | Sprint §20 |
| state migration v1→v2 | PASS | Sprint §18 (G7) |
| identity / no-silent-regeneration | PASS | Sprint §10 |
| filename hardening | PASS | Sprint §12 |
| control transport | PASS | Sprint §13, POC-CORE-03 |
| portable boundary | PASS | Sprint §5; runner graph check |
| protocol | unchanged (0 `.proto` files) | Verified by `git diff` at certification |

## 11. Certification

> # **WAVE 0 CERTIFIED** — 2026-09-01
> Commit `cfd33f6` · branch `feature/core-platform-abstraction-v1`

Every condition of the certification rule holds: G6 PASS, G10 PASS, POC-CORE-01/02/03/04 all PASS,
and no other official gate is FAIL or NOT EXECUTED.

**Remaining blockers: none.**

**What certification means.** The Wave 0 refactor is behaviour-preserving on Fedora, interoperable
with the unmodified Android app, protocol-identical, and its portable core compiles, links and
builds its tests under a real MSVC toolchain.

**What it does not mean.** It does **not** mean AnyFlow runs on Windows. No platform was added in
Wave 0. CNG, named pipes, `mdns-sd` coexistence (V-12) and notifications.v1 are Wave 5+. A green
CI-001 must never be read as Windows support.

## 12. Open debts — not blockers

| # | Item | Status |
| --- | --- | --- |
| **B-1** | Sharesheet stuck on "Sending…" after an offer failure (Android, real bug) | Tracked: [issue #12](https://github.com/yurisismotto/anyflow/issues/12), P2. **Not fixed** — an Android change would break `android/**` = 0 files, which G6 depends on |
| **B-2** | `real_backend` calls `wl-copy --clear` unbounded; hangs on a locked seat (test-only) | Tracked: [issue #13](https://github.com/yurisismotto/anyflow/issues/13), P3 |

Both were already registered as GitHub issues before this phase; both are recorded in sprint §23
items 8–9. Neither is a Wave 0 gate and neither contaminates the certification.

Also carried forward (sprint §23): `anyflow-runtime` outside the portable gate (deliberate, V-12);
the Unix FS adapter living in `anyflow-core` behind a feature (CC-5); no dependency cache in
CI-001 (a first workflow adding a third-party action is a supply-chain decision, deferred).

## 13. Documents changed

**Modified (tracked):**

| File | Change |
| --- | --- |
| `docs/sprints/wave-0-platform-abstraction.md` | §17 POC-CORE-04 NOT EXECUTED → **PASS** with the full evidence record (canonical); §18 Android interop and clipboard round-trip closed; §1, §23, §24, §25, §26 → **CERTIFIED** |
| `docs/research/platform-expansion/21-POC-MASTER-PLAN.md` | POC-CORE-04 gains a **Result** row |
| `docs/research/platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md` | Status → IMPLEMENTED AND CERTIFIED; §10.3 records the executed gate; implementation-outcome table; CI-001 row |
| `docs/research/platform-expansion/README.md` | Wave 0 → CERTIFIED; PoC row → 4/4 PASS; doc-28 status |
| `docs/research/platform-expansion/25-IMPLEMENTATION-BACKLOG.md` | CI-001 → DONE |

**Modified (untracked working-tree reports):** `WAVE-0-LOCAL-CERTIFICATION-REPORT.md` (POC-CORE-04
OPEN → PASS; closing banner superseded) and `WAVE-0-POC-CORE-04-READINESS.md` (Phase 1 banner
marked superseded). **New:** this file.

**No production file was touched.** No Rust, no Cargo manifest, no protobuf, no protocol, no TLS,
no Android, and **no change to the workflow** — it is green and was left alone.

## 14. Git status — final

```
$ git branch --show-current
feature/core-platform-abstraction-v1

$ git diff --check
(clean)

$ git diff --stat
 docs/research/platform-expansion/21-POC-MASTER-PLAN.md      |   3 +-
 docs/research/platform-expansion/25-IMPLEMENTATION-BACKLOG.md |   2 +-
 docs/research/platform-expansion/28-WAVE-0-IMPLEMENTATION-SPEC.md |  22 +-
 docs/research/platform-expansion/README.md                  |  13 +-
 docs/sprints/wave-0-platform-abstraction.md                 | 230 +++++++++-----
 5 files changed, 179 insertions(+), 91 deletions(-)

$ git status --porcelain -- desktop/ android/ protocol/ .github/
(empty)
```

Documentation only. **No `git add`. No commit. No push.**

### One thing the maintainer must decide

`WAVE-0-LOCAL-CERTIFICATION-REPORT.md` — the sole evidence for **G6** and **G10** — is still
**untracked**. The versioned certification now links to it. Until it is committed, the
certification chain is not self-contained in the repository. Committing it (and this file) with the
doc updates closes that gap; it is the maintainer's call, not a gate.
