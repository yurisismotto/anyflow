# Wave 0 — POC-CORE-04 · Phase 1 readiness (pre-push)

**Date:** 2026-08-31 · **Branch:** `feature/core-platform-abstraction-v1` · **No commit, no push.**

**Status: POC-CORE-04 READY TO RUN** — not PASS. Nothing has executed on a Windows runner yet.

> **Phase 1 document, superseded 2026-09-01.** The gate has since run and passed:
> [run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649) on
> `cfd33f6`, `host: x86_64-pc-windows-msvc`, 12/12 steps green. This file is retained as the
> pre-push readiness record; the result lives in
> [the sprint report](docs/sprints/wave-0-platform-abstraction.md) §17.

---

## 1. Existing CI audit

There is **no CI in this repository**. Not stale, not disabled — absent.

```
$ ls .github/
ls: cannot access '.github/': No such file or directory
$ git ls-files | grep -i '^\.github'
(nothing)
$ find . -maxdepth 3 -name '*.yml' -o -name '*.yaml'
(nothing)
```

This is by design. `28-WAVE-0-IMPLEMENTATION-SPEC.md` §15 forbids a CI change in the same commit
as the refactor: *"CI-001 lands after, on green."* Wave 0 has now landed green locally
(`WAVE-0-LOCAL-CERTIFICATION-REPORT.md`), so CI-001 is due.

There was therefore no existing workflow to extend and no matrix to join. A new, small,
permanent workflow was created. Nothing was duplicated.

## 2. Workflow chosen

`.github/workflows/portable-windows-msvc.yml` — one job, `portable-msvc`, on `windows-latest`.

Named **CI-001** in the file header, scoped to exactly what §10.3 defines and nothing else.

## 3. Files changed

| File | Change |
| --- | --- |
| `.github/workflows/portable-windows-msvc.yml` | **new**, 284 lines |
| `WAVE-0-POC-CORE-04-READINESS.md` | **new**, this document (optional for the commit) |

**No Rust source, no Cargo manifest, no protobuf, no protocol, no TLS code, no Android file was
touched.** `git status` for `desktop/` is byte-identical to the pre-existing Wave 0 worktree.

## 4. Exact MSVC commands the job runs

Working directory `desktop/`, shell `pwsh`.

**The gate — verbatim from §10.3, plus `--locked`:**

```
cargo check --locked --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-proto -p anyflow-core -p anyflow-control \
  -p anyflow-capability-clipboard -p anyflow-capability-files -p anyflow-capability-battery
```

**Strengthening — codegen and link, where `ring`'s MSVC objects actually have to work:**

```
cargo build --locked --no-default-features --target x86_64-pc-windows-msvc  -p …(same six)
```

**Test targets:**

```
cargo test --locked --no-run --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-proto -p anyflow-control \
  -p anyflow-capability-clipboard -p anyflow-capability-files -p anyflow-capability-battery

cargo test --locked --no-run --no-default-features --target x86_64-pc-windows-msvc \
  -p anyflow-core --lib --test identity_seam --test pairing --test portable_boundary --test protocol
```

**Toolchain proof** (fails the job on anything but MSVC):

```
rustc -Vv ; cargo -V ; rustup show active-toolchain ; rustup target list --installed
host: x86_64-pc-windows-msvc   ← asserted by string equality; -gnu is rejected explicitly
```

## 5. Portable package list — verified against the workspace

`desktop/Cargo.toml` members were read, not assumed. The six portable crates exist under these
exact names:

`anyflow-proto`, `anyflow-core`, `anyflow-control`, `anyflow-capability-clipboard`,
`anyflow-capability-files`, `anyflow-capability-battery`.

`anyflow-runtime` is **excluded**, per §10.3: it depends on `mdns-sd`, whose Windows behaviour is
V-12 / POC-WIN-02. `anyflow-linux`, `anyflow-daemon`, `anyflow-cli`, `anyflow-gui` are adapters
and binaries, out of scope.

A step asserts via `cargo metadata` that all six still exist, so a rename or deletion fails the
gate rather than silently shrinking it.

## 6. Dependency-boundary check

Measured on the **resolved graph**, not grepped from source — because Wave 0 already hit exactly
this once: in POC-CORE-01 the first attempt failed when Cargo feature unification switched
`unix-fs` back on through a capability crate's default dependency on `anyflow-core`, which a
source grep would have missed.

```
cargo tree --locked --no-default-features --target x86_64-pc-windows-msvc -e normal \
  -p …(the six) --prefix none --format '{p} [{f}]'
```

The job then fails if either:

- a **package name** in the graph is one of `anyflow-linux`, `anyflow-daemon`, `anyflow-runtime`,
  `anyflow-gui`, `anyflow-cli`, `x11rb`, `x11rb-protocol`, `zbus`, `zbus_names`, `gtk4`,
  `gtk4-sys`, `libadwaita`, `libadwaita-sys`, `glib`, `glib-sys`, `gdk4`, `gio`, `gobject-sys`,
  `wayland-client`, `wayland-backend`, `smithay-client-toolkit`, `mdns-sd`, `nix`, `libc`,
  `rustix`; or
- an `anyflow-*` node carries the feature `unix-fs`, `linux-backends` or `upower`.

Matching is on the package name only — matching whole lines would false-positive, because `tokio`
carries a feature literally called `libc`.

**Dry-run against the real resolver, locally, with the MSVC triple** (`cargo tree --target` filters
`cfg` without needing the target installed):

```
87 packages · 0 forbidden crates · 0 forbidden features

anyflow-core                 []      ← unix-fs OFF
anyflow-capability-clipboard []      ← linux-backends OFF
anyflow-capability-files     []      ← unix-fs OFF
anyflow-capability-battery   []      ← upower OFF
ring        v0.17.14 [alloc,default,dev_urandom_fallback]
rustls      v0.23.43 [ring,std]
tokio       v1.53.1  [… mio, socket2, windows-sys]      ← windows-sys, not libc
windows-sys v0.61.2  [Win32_Networking_WinSock, Win32_System_Pipes, …]
```

`libc` does not appear as a package; `mio`/`socket2` resolve their Windows backends. This is the
graph the runner will build.

## 7. Why this is a valid POC-CORE-04

| Official criterion (21 §POC-CORE-04, 28 §10.3) | How it is met |
| --- | --- |
| **Question** — does `cargo check` succeed for the six portable crates on `x86_64-pc-windows-msvc`? | That exact command is the gate step |
| **`--no-default-features`** | Present on every cargo invocation |
| **On a Windows runner** (a Linux cross-compile is not a valid gate: `ring` needs MSVC, V-10) | `runs-on: windows-latest`, and the job fails unless `rustc -Vv` reports host `x86_64-pc-windows-msvc` |
| **`anyflow-runtime` deliberately excluded** | Excluded; the reason is in the file header |
| **Failure = any `std::os::unix` leak or unexpected transitive Unix-only dependency** | The compile catches the first; the `cargo tree` step catches the second |
| **Output = the gate that becomes CI-001** | This *is* CI-001: permanent, on PRs to `main`/`develop` |
| **`rustls` + `ring` + MSVC path proven** | `ring` is unchanged and in the graph; `cargo build` forces its codegen; a step lists the MSVC objects it produced |

No criterion was invented and none was substituted. `ring` was not replaced, no crypto backend was
changed, and no GNU build is offered as evidence.

**What it does not prove:** that AnyFlow runs on Windows. Compile-check is a boundary regression
test; runtime certification is Wave 5+.

## 8. Expected GitHub trigger

```yaml
pull_request:   branches: [main, develop]        paths: desktop/**, this workflow
push:           branches: [main, develop, 'feature/**']   paths: desktop/**, this workflow
workflow_dispatch:
```

- **Now:** pushing `feature/core-platform-abstraction-v1` matches `feature/**` **and** the
  `desktop/**` path filter (the Wave 0 refactor is in that commit), so the run starts on push —
  no PR needed to obtain the POC-CORE-04 evidence.
- **Permanently:** every PR into `develop` or `main` that touches `desktop/**` must pass it. A
  future PR cannot break the Windows boundary silently.
- `workflow_dispatch` is there for re-running the gate on demand once the workflow reaches the
  default branch.
- `concurrency` cancels superseded runs on the same ref. `permissions: contents: read`.

**Fail-closed:** no `continue-on-error` anywhere; no warning-only step. The job fails if MSVC
compile fails, if a Linux crate or feature reaches the portable set, if a portable package
disappears, or if the `unsafe_code` policy regresses.

**No cache.** The repo has no CI and therefore no established cache action; adding a third-party
action to a first workflow is a supply-chain decision, not a Wave 0 one. Simplicity first.

## 9. The one scope call: two `anyflow-core` test targets

`cargo test --no-run --no-default-features` was run locally against all six crates. Five compile
every test target. `anyflow-core` fails on exactly two:

| Test target | Why it cannot compile with `unix-fs` off |
| --- | --- |
| `core/tests/identity_and_store.rs` | uses `Store::open(dir)`, `platform::unix_fs` |
| `core/tests/identity_states.rs` | uses `Store::open(dir)`, `Store::probe_identity_at` |

Those functions **do not exist** when the feature is off. They are tests *of the Unix filesystem
adapter* — the 0600-mode refusal, `probe_identity_at`, XDG paths. Under §6's A/B/C classification
this is **C: not part of the official POC**, whose minimal scope is the library compile check.
The library code they exercise is already correctly feature-gated (`core/src/platform/unix_fs.rs`,
the sole exception recorded in `core/tests/portable_boundary.rs`); it is only the *test files*
that are unconditional.

**Nothing was hidden and nothing was weakened.** No assertion was changed, no `cfg` was added, no
`required-features` was written into a production manifest. The two targets are named explicitly
in the workflow, and a preceding guard step compares `core/tests/*.rs` against the recorded set of
six files and fails if it differs — so a *new* core test cannot slip past the Windows gate
unnoticed.

*Optional follow-up, deliberately not done here:* declaring `required-features = ["unix-fs"]` on
those two `[[test]]` targets would let the gate name all six crates uniformly. It is a production
manifest change, so it is a separate decision, not something to slip into a CI sprint.

## 10. Local regression

Per §16: **no production file changed**, so the hardware suite was not repeated and Android was
not rebuilt (nothing shared with Android changed).

What was run locally, all read-only and all with `--locked`:

| Command | Result |
| --- | --- |
| `cargo check --locked --no-default-features -p …(six)` (Linux proxy for the gate) | exit 0 |
| `cargo test --locked --no-run --no-default-features -p …(six)` | 5 crates OK; `anyflow-core` fails on the two adapter tests above — the finding in §9 |
| `cargo test --locked --no-run --no-default-features -p anyflow-core --lib --test identity_seam --test pairing --test portable_boundary --test protocol` | exit 0 — 5 executables |
| `cargo tree --locked --no-default-features --target x86_64-pc-windows-msvc -e normal …` | 87 packages, boundary clean |
| YAML parse of the workflow | OK — 11 steps, `runs-on: windows-latest`, 3 triggers |
| Forbidden-list dry run against the resolved graph | 0 hits |

Every cargo invocation used `--locked`, so `Cargo.lock` could not be modified.

## 11. Git status

```
$ git branch --show-current
feature/core-platform-abstraction-v1

$ git diff --check
(clean)

$ git diff --stat
38 files changed, 2096 insertions(+), 323 deletions(-)

staged (pre-existing renames):        5
modified, unstaged:                  38
untracked paths:                     14   (16 files with -uall)
```

Unchanged from the Wave 0 precheck except for the two new untracked files in §3.

### Secret / artifact audit

| Pattern | Finding |
| --- | --- |
| `*.key`, `*.pem`, `*.p12`, `*.pfx`, `*.jks`, `*.keystore` | none, tracked or untracked (`-uall`) |
| `*.apk`, `*.aab` | none |
| `state.json`, `identity.key`, trust stores | none |
| logs, temporary binaries, test captures | none — all under the scratchpad, outside the repo |
| `git check-ignore` on the new workflow | not ignored; it will be tracked |

The `.gitignore` already covers `*.key`, `state.json`, `trust-store.json`, `*.apk`, `*.aab`.

## 12. Commit recommendation

The Windows runner compiles `desktop/`, so the **whole Wave 0 implementation must be in the
commit** — the workflow alone would have nothing to check.

**Required:**

```
git add -A desktop/ docs/ .github/workflows/portable-windows-msvc.yml
```

That is: the 5 already-staged renames, the 38 modified files, the 12 Wave 0 untracked paths
(`desktop/control/`, `desktop/runtime/`, `desktop/platform-linux/`, `desktop/core/src/platform/`,
`desktop/core/src/secret_store.rs`, `desktop/capabilities/files/src/sink.rs`, the three new
`core/tests/*.rs`, `docs/sprints/wave-0-platform-abstraction.md`) and the new workflow.

**Your call, not required by the gate:** `WAVE-0-LOCAL-CERTIFICATION-REPORT.md` and this file.

Suggested message:

```
ci: add the Windows MSVC portable-core gate (CI-001, POC-CORE-04)

Wave 0's portable boundary has only ever been checked from Linux: a source
grep (core/tests/portable_boundary.rs) and an x86_64-pc-windows-gnu
cross-compile (POC-CORE-01). Neither is evidence for MSVC — ring's C
toolchain and CRT differ, and MSVC libraries cannot be redistributed onto a
Linux runner (V-10). That is why POC-CORE-04 exists separately, and it has
never been executed.

This adds the job 28-WAVE-0-IMPLEMENTATION-SPEC.md §10.3 specifies, on a
GitHub-hosted windows-latest runner: cargo check --no-default-features
--target x86_64-pc-windows-msvc over the six portable crates, with
anyflow-runtime deliberately excluded (mdns-sd on Windows is V-12).

Beyond the specified check it also builds (codegen and link, where ring's
MSVC objects have to work), compiles the portable test targets, and asserts
the boundary on the resolved dependency graph rather than on source — the
failure mode POC-CORE-01 actually hit was Cargo feature unification
switching unix-fs back on, which a grep cannot see.

No production code changes. No protocol, protobuf, TLS or Android change.
No crypto backend substitution: ring stays. A -gnu host fails the job.

A green run proves the boundary held. It does not mean AnyFlow runs on
Windows; that is runtime certification and belongs to Wave 5+.

Refs: POC-CORE-04, CI-001, ARCH-010
```

**Do not tag this as certification.** POC-CORE-04 stays NOT EXECUTED until the run is green.

## 13. After the push — Phase 2

1. Watch the run on `feature/core-platform-abstraction-v1`.
2. If green: capture runner OS image, `rustc -Vv`, `cargo -V`, host triple, the exact commands,
   and the run ID/URL.
3. Update `21-POC-MASTER-PLAN.md` (POC-CORE-04 → **PASS** with that evidence),
   `docs/sprints/wave-0-platform-abstraction.md` §POC-CORE-04 (NOT EXECUTED → PASS),
   `docs/research/platform-expansion/README.md`, and the Wave 0 gate table.
4. Re-evaluate the gate set. If G6, G10 and POC-CORE-01/02/03/04 are all PASS →
   **WAVE 0 CERTIFIED**. Otherwise NOT CERTIFIED with the blocker named.

---

# POC-CORE-04 READY TO RUN → **PASS (2026-09-01)**

As written at Phase 1: *"Not PASS. No Windows runner has executed anything."* The gate was
written, fail-closed, and scoped to the official criteria — and it has since run green on
`windows-2025-vs2026`. **POC-CORE-04 PASS**;
[run 33465365649](https://github.com/yurisismotto/anyflow/actions/runs/33465365649).
