# U2 Post-Certification Hardening — Test / CI

**Branch:** `fix/u2-test-ci-hardening`
**Base:** `develop` @ `70a30e3`
**Date:** 2026-09-16
**Status:** local pass; GitHub Actions evidence pending (§20)

---

## 1. Scope

This branch addresses the four remaining U2 **test / CI** defects and nothing
else.

| ID | Defect | Outcome |
|----|--------|---------|
| TC1 | `clipboard` `real_backend` test assumed `wl-copy --sensitive` support unconditionally | **Fixed** — capability-branching contract test |
| TC2 | `notifications` `real_dbus` tautological assertion (`x \|\| !x`) | **Fixed** — replaced with three real invariants |
| TC3 | Desktop clippy was not a GitHub CI gate | **Implemented** — `.github/workflows/desktop-quality.yml` |
| TC4 | `android/**` changes had no GitHub CI gate at all | **Implemented** — `.github/workflows/android-ci.yml` |

**Product behaviour is unchanged.** No file under any `src/` directory was
modified. The complete change set is three desktop **test** files, one Android
**test** fixture, and two new workflow files (§14).

P1 (Android multi-peer routing), P2 (Linux battery absence) and P3 (notification
role convergence) are untouched and their suites still pass (§16).

### Explicitly not addressed

Debian packaging/documentation, incoming-file GUI approval, QR scanner camera
orientation, trust-store visual ordering, the notification lost-role queue
recovery debt, battery hotplug, branding, palette, Quick Panel, KDE, and
application UX redesign — all out of scope by instruction, and none was touched.

---

## 2. Baseline

```
$ git branch --show-current
fix/u2-test-ci-hardening

$ git log --oneline -3
70a30e3 Merge pull request #28 from yurisismotto/fix/u2-p3-notification-role-convergence
2ffa945 fix(notifications): converge roles during live sessions
72188c6 Merge pull request #27 from yurisismotto/fix/u2-p2-battery-absence

$ git status --short
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md
```

`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` — the historical U2 evidence — was untracked
at the start of this branch and is **untracked, unmodified and unstaged** at the
end of it (§21). Its mtime is unchanged at `2026-09-15 23:16`.

### Host

| | |
|---|---|
| OS | Fedora 44, Linux 7.1.9 |
| Session | GNOME Wayland (`wayland-0`), gnome-shell 50.4, notification spec 1.2 |
| Rust | `rustc 1.98.0` / `clippy 0.1.98` |
| wl-clipboard | `2.2.1` — **with** `--sensitive` (Fedora's `2.2.1^git…` snapshot) |
| JDK (local) | Temurin 21.0.12.1 (CI uses 17 — §11) |
| Android SDK | platform 35, local `cmdline-tools/latest` |

Per instruction, **no U2 VM was booted**, no emulator was started, and
`connectedAndroidTest` was never invoked. `anyflow-u2404`, `anyflow-d13` and
`anyflow-u2604` remain powered off and preserved.

---

## 3. TC1 — the clipboard `real_backend` defect

### What U2 observed

On Ubuntu 24.04, Ubuntu 26.04 and Debian 13 — all shipping wl-clipboard 2.2.1
without `--sensitive` — this test failed:

```rust
/// `wl-copy --sensitive` must not corrupt the content it marks.
async fn a_sensitive_write_still_round_trips() {
    let value = ClipboardText::validate("sensitive round trip").expect("valid");
    write(&b, &value, true).await;      // panics on Err
    assert_eq!(read(&b).await.expect("text").as_str(), "sensitive round trip");
}
```

### Root cause

The test asserted a **host capability**, not a **product contract**.

The product was already correct on those distributions, and correct in the
direction that matters:

* ordinary clipboard available — `availability() == Ok(())`;
* sensitive capability reported unavailable — `sensitive_support()` is `Err`;
* sensitive write refused **fail-closed** with `BackendError::Unavailable`;
* **zero** sensitive bytes handed to an unmarked `wl-copy`.

That is exactly PLAT-DEC-013: refusing is right, because silently writing a
`sensitive_hint` clip unmarked turns a visible failure into an invisible privacy
regression.

The capability model already distinguished the two questions — `availability()`
and `sensitive_support()` are separate trait methods, documented as separate on
purpose. The test simply did not ask the second one. It hard-asserted the Fedora
answer.

The consequence is worse than a red tick: a gate that goes red for *correct*
fail-closed behaviour trains the next person to ignore it.

---

## 4. TC1 — corrected semantics

`a_sensitive_write_still_round_trips` is replaced by
**`a_sensitive_write_honours_this_backend_s_advertised_capability`**
(`desktop/capabilities/clipboard/tests/real_backend.rs`).

It asserts the product contract, which is the same sentence on every host:

> `sensitive_support()` is a promise, and `write_text(_, true)` keeps it in
> **both** directions.

The test matches on `(sensitive_support(), write_text(_, true))`:

| `sensitive_support()` | write outcome | verdict |
|---|---|---|
| `Ok(())` | `Ok(())` | **pass** — and the content must round-trip byte for byte, hash included |
| `Err(why)` | `Err(Unavailable)` | **pass** — and four further assertions run (below) |
| `Ok(())` | `Err(e)` | **fail** — status promises sensitive clips on a machine where they do not arrive |
| `Err(why)` | `Ok(())` | **fail** — either the probe is wrong or the clip went out unmarked |

On the **unsupported** branch it asserts, in order:

1. the error is the typed capability error `BackendError::Unavailable`, not a
   generic `Failed` (before Wave 0 this surfaced as `wl-copy exited with 1`);
2. the refusal carries a non-empty reason, so `anyflow clipboard status` has
   something to print;
3. the refusal does **not** contain the content it refused;
4. **no sensitive byte reached the clipboard** — an ordinary sentinel is written
   first, and after the refusal the clipboard must still hold that sentinel. An
   implementation that fell back to an unmarked `wl-copy` would have replaced it
   with the canary;
5. the ordinary clipboard still works afterwards — the capability *degrades*, it
   does not break.

Two properties of this design are deliberate and load-bearing:

* **Unsupported is an asserted state, never a skipped one.** There is no early
  `return`. An unsupported host runs strictly *more* assertions than a supported
  one, because the fail-closed path is the one with a privacy consequence.
* **Nothing is hard-coded.** No distribution name, no `wl-clipboard` version, no
  version string. The branch is chosen by `backend.sensitive_support()`, which is
  the backend's own probe of the installed tool. This matters because the version
  number is the *wrong* instrument — Fedora's `2.2.1^git…` has the flag and
  Debian's plain `2.2.1` does not.

### The companion fix: this contract is now gated on every PR

`real_backend.rs` is `#[ignore]`d — it needs a Wayland session and is asked for
by name during a certification run. **No CI job ever executes it.** The contract
it gates was therefore ungated between certifications, which is how TC1 could sit
in the tree at all.

So the same sentence was added to
`desktop/capabilities/clipboard/tests/sensitive_capability.rs`, which is *not*
ignored and which the Linux distro matrix already runs on every pull request. A
shared helper `assert_sensitive_contract()` drives both fakes:

* `the_sensitive_contract_holds_where_the_flag_exists` — must take the supported
  branch;
* `the_sensitive_contract_holds_where_the_flag_is_missing` — must take the
  refusal branch.

Because that suite drives a real process boundary (`#!/bin/sh` fakes on `PATH`),
it measures the fail-closed property one level deeper than `real_backend` can:
**stdin bytes actually delivered to the child process**, plus the assertion that
no `wl-copy` was spawned for the refused clip at all.

### A pre-existing break found and fixed while doing this

`sensitive_capability.rs` imports `backend::wayland::WaylandBackend` and uses
`std::os::unix::fs::PermissionsExt`, but carried **no `cfg` gate**. It therefore
cannot compile with `--no-default-features` — which is precisely what
`portable-windows-msvc.yml` does:

```
$ git stash push -- capabilities/*/tests     # the untouched U2 baseline
$ cargo test --locked --no-run --no-default-features -p anyflow-capability-clipboard
error[E0432]: unresolved import `anyflow_capability_clipboard::backend::wayland`
note: the item is gated behind the `linux-backends` feature
```

**The portable Windows gate is red on `develop` today**, and was before this
branch. The fix is the classification `real_dbus.rs` already uses — a whole-file
`cfg` so the target compiles to an empty test binary when the feature is off:

```rust
#![cfg(all(unix, feature = "linux-backends"))]
```

Verified: with the gate, `cargo test --no-run --no-default-features` builds every
test target in both capability crates (§18). With default features on Linux, all
nine tests still run.

---

## 5. TC1 — mutation evidence

All mutations were temporary, run one at a time, and **reverted**; `git diff`
over `src/` is empty (§21).

The unsupported host was simulated faithfully rather than mocked: a wrapper
`wl-copy` early on `PATH` that filters `--sensitive` out of `--help` and rejects
the flag the way 2.2.1 rejects an unknown option (`print_usage(stderr); exit 1`),
and otherwise `exec`s the real `/usr/bin/wl-copy`. So the compositor, the
clipboard and the round trip are all real; only the advertised capability is
Ubuntu-shaped.

### M1 — the old assertion against an unsupported host → **FAILS** ✔

*Reproduces the U2 failure exactly.*

```
test mutation_m1_the_old_unconditional_assertion ... FAILED
panicked at real_backend.rs:74:
write failed: clipboard unavailable: this clip is marked sensitive and this
system's wl-copy does not support sensitive clipboard marking. […] It was NOT
written to the clipboard: writing it unmarked would leave a password in your
clipboard manager's history without telling you.
```

The corrected test, same host, same run: **passes**.

```
test a_sensitive_write_honours_this_backend_s_advertised_capability ... ok
backend: wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD selection;
         sensitive marking: no
sensitive marking on this host: unsupported (…)
```

### M2 — unsafe fallback: `sensitive=false`, payload forwarded anyway → **FAILS** ✔

`wayland.rs` mutated to downgrade silently instead of refusing:

```rust
// MUTATION M2: silent downgrade
if self.sensitive.as_result().is_ok() { command.arg("--sensitive"); }
```

Corrected `real_backend` test, unsupported host:

```
test a_sensitive_write_honours_this_backend_s_advertised_capability ... FAILED
sensitive_support() said this backend CANNOT mark a clip (…) and the write
succeeded anyway.
Either the probe is wrong, or the clip was written unmarked — and an unmarked
write of a sensitive clip leaves a password in the desktop's clipboard history
without telling anyone.
```

And — the part that matters for CI — the **non-ignored** suite catches it too,
on any machine, with no compositor:

```
test the_sensitive_contract_holds_where_the_flag_is_missing ... FAILED
test an_unsupported_wl_copy_keeps_the_ordinary_clipboard_and_refuses_… ... FAILED
test a_refused_sensitive_clip_reaches_no_process_at_all ... FAILED
test a_new_looking_version_without_the_flag_is_treated_as_unsupported ... FAILED
test result: FAILED. 5 passed; 4 failed
```

### M3 — `sensitive=true` but the marked write fails unexpectedly → **FAILS** ✔

`wayland.rs` mutated to inject a failure after the flag is added. Supported host
(the real Fedora `wl-copy`):

```
test a_sensitive_write_honours_this_backend_s_advertised_capability ... FAILED
sensitive_support() said this backend can mark a clip, and the marked write
failed: clipboard operation failed: injected M3 failure
```

### Not a skip

Explicitly checked: on the unsupported host the corrected test executes the full
refusal branch and reports `ok` — it does not print a skip message and return.
The `eprintln!` line naming the host's capability is evidence, not an escape
hatch.

---

## 6. TC2 — the `real_dbus` / clippy defect

`desktop/capabilities/notifications/tests/real_dbus.rs`, in
`the_real_server_answers_and_says_what_it_can_do`:

```rust
// Not asserted as a fixed set: `GetCapabilities` is the server's answer,
// not ours, and a suite that required GNOME's exact list would fail on
// KDE for no reason. What is asserted is that the answer was usable.
assert!(
    capabilities.body || !capabilities.body,
    "the capability set decoded"
);
```

The *comment* is right — requiring GNOME's exact list would be wrong, because
`body`, `body-markup` and `persistence` are all optional in the freedesktop
specification. The *assertion* is a tautology and proves nothing whatsoever.

**A single meaningless assertion was the only tautology in the crate.** A sweep
of all desktop tests for equivalent shapes (`x || !x`, `assert!(true)`,
`matches!(…, _)`, `#[allow]`, `black_box`, `let _ =` used to silence a lint)
found no other instance in the notifications surface. The three `let _ =` uses in
`dismiss.rs` / `logging.rs` discard drain results and are legitimate. Nothing was
broadened into a refactor.

### On the clippy half

U2 saw this rejected by clippy on distro toolchains. It was **not** reproduced
locally: restoring the expression and running `clippy 0.1.98` produced no
diagnostic, because the expression sits inside an `assert!` macro expansion and
whether `nonminimal_bool` fires there has varied between releases.

That is reported as measured, not as assumed — and it does not change the fix.
The assertion had to go because it asserted nothing (proved in §7), not because a
particular clippy version disliked it. After the change the test contains no
tautological expression at all, so no clippy version can have an opinion about it.

---

## 7. TC2 — the corrected assertion

Three invariants replace the tautology. Each is a promise **this code** makes,
not one the desktop makes — so each holds on GNOME, KDE, dunst and mako alike,
with no capability the specification does not guarantee being asserted.

### 1. The capability query completed, and was parsed faithfully

This is what the old assertion was reaching for and could not express.
`DbusSink::connect` calls:

```rust
let advertised: Vec<String> = proxy.call("GetCapabilities", &())
    .await.unwrap_or_else(|_| Vec::new());
```

A failed query is, from inside the returned struct, **indistinguishable** from a
server that advertises nothing. So the test asks the server again over a
connection of its own and compares token by token:

```rust
let advertised = advertised_capabilities().await;
for (name, parsed) in [("body", capabilities.body),
                       ("body-markup", capabilities.body_markup),
                       ("persistence", capabilities.persistence)] {
    assert_eq!(parsed, advertised.iter().any(|c| c == name), …);
}
```

Nothing about GNOME's list is required — only that whatever *this* server says,
the sink recorded exactly that. A sink that substring-matched (`body` inside
`body-markup`), dropped the query, or invented a capability fails on every
desktop.

### 2. `capabilities()` is pure and stable

`NotificationSink::capabilities` is documented "read once at connect; never
re-negotiated". Two calls must be equal — a sink that re-queried the bus here
would make the role announcement depend on *when* it was asked.

### 3. `dismiss_reporting` reflects the subscription, not the advertisement

This is the assertion with teeth. `dismiss_reporting` is the **single input to
the `DISMISS_REPORTER` role** and is deliberately not read from
`GetCapabilities` — the spec has no capability string for "I will tell you why a
notification closed". What decides it is whether this process actually holds a
`NotificationClosed` subscription:

```rust
let closed = sink.closed_events();
assert_eq!(capabilities.dismiss_reporting, closed.is_some(), …);
assert!(sink.closed_events().is_none(), "handed over exactly once");
```

The second line pins the documented one-stream-one-consumer handover; a second
reader would split close signals between two halves of the capability and lose
dismissals at random.

### Live run

```
$ cargo test -p anyflow-capability-notifications --test real_dbus \
    -- --ignored --test-threads=1 the_real_server_answers
server: org.freedesktop.Notifications — gnome-shell 50.4 (spec 1.2, GNOME)
capabilities: SinkCapabilities { body_markup: true, body: true,
                                 persistence: true, dismiss_reporting: true }
GetCapabilities (fresh query): ["actions", "body", "body-markup",
                                "icon-static", "persistence", "sound"]
test the_real_server_answers_and_says_what_it_can_do ... ok
```

### Mutation evidence

**N1 — the swallowed capability query.** `dbus.rs` mutated so the
`GetCapabilities` result is dropped, exactly as a transient bus error would drop
it today — silently, into an all-false set. Both assertions run side by side:

```
test mutation_the_old_tautological_assertion ... ok        <-- PROVES NOTHING
  capabilities: SinkCapabilities { body_markup: false, body: false,
                                   persistence: false, dismiss_reporting: true }

test the_real_server_answers_and_says_what_it_can_do ... FAILED
  assertion `left == right` failed: the sink's `body` disagrees with what this
  server advertises (["actions","body","body-markup","icon-static",
  "persistence","sound"]) — the capability set is parsed, not decorative
    left: false
   right: true
```

The old assertion passed against a **completely broken capability set**. That is
the clearest possible statement of what it was worth.

**N2 — the role announced with no subscription.** `dbus.rs` mutated so the close
match rule is dropped and `dismiss_reporting: true` is announced anyway:

```
test mutation_the_old_tautological_assertion ... ok
test the_real_server_answers_and_says_what_it_can_do ... FAILED
  dismiss_reporting must reflect the close-signal subscription this process
  actually holds, not what the server advertises
    left: true      right: false
```

That is the dangerous direction: announcing `DISMISS_REPORTER` without a
subscription would clear a peer's notifications every time a banner timed out on
a screen nobody was looking at.

**Recorded limitation, honestly.** A first attempt at N2 wired `dismiss_reporting`
to an *arbitrary advertised string* (`actions`). That mutation was **not** caught,
because on this GNOME session `actions` is advertised and the subscription also
succeeded, so the two values coincided. Invariant 3 asserts *agreement*; a
miswiring that happens to agree on a given host is not detected on that host. The
direction with a user-visible consequence — over-claiming the role — is caught,
which is the property worth having. This is stated rather than papered over.

All mutations reverted; `git diff` over `src/` is empty.

---

## 8. Desktop CI architecture

**New file:** `.github/workflows/desktop-quality.yml` — *"Desktop quality ·
fmt + clippy"* (CI-003).

A dedicated workflow rather than extra rows on the already-expensive distro
matrix: `linux-distro-compat.yml` runs three containers plus an MSRV job, and
bolting a lint pass onto each would multiply the cost of the thing that already
costs the most, to answer a question that only needs asking once.

| | |
|---|---|
| Runner | `ubuntu-24.04` (same as the existing MSRV job) |
| Triggers | `pull_request` + `push` on `[main, develop]`, paths `desktop/**` and this workflow; `workflow_dispatch` |
| Concurrency | `desktop-quality-${{ github.ref }}`, `cancel-in-progress: true` |
| Permissions | `contents: read` |
| Actions | `actions/checkout@v5` only — consistent with the repository |
| Native deps | `gcc libc6-dev pkg-config libgtk-4-dev libadwaita-1-dev` |
| Timeout | 45 minutes |

The native dependency list is not invented: it is the measured minimum already
justified in `linux-distro-compat.yml` (U0 §6) — no `libssl-dev` (transport is
rustls/ring), no `libdbus-1-dev` (zbus 5 is pure Rust), no `libx11-dev` (x11rb
brings its own connection), no `protobuf-compiler` (protox compiles in-process).

`push` is deliberately **not** on `feature/**`, matching the discipline already
written into both existing desktop workflows: a feature branch with an open PR
would otherwise run everything twice for one push.

### Gates

```yaml
- run: cargo metadata --locked --format-version 1 > /dev/null
- run: cargo fmt --all --check
- run: cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Plus two guard steps that fail loudly rather than drifting:

* **toolchain identity** — prints `rustup show active-toolchain`, `rustc -Vv`,
  `cargo fmt --version`, `cargo clippy --version`;
* **the pinned channel is still a channel** — fails if
  `desktop/rust-toolchain.toml` is ever pinned to a fixed version, because at
  that moment this job silently stops answering the question it exists to answer
  (§9).

`--locked` on every invocation: CI must lint what is committed. `Cargo.lock` was
**not modified** by this branch.

---

## 9. Desktop clippy policy

### Why `--all-features`, measured rather than assumed

Every feature declared in `desktop/` was enumerated before choosing:

| crate | features |
|---|---|
| `anyflow-core` | `default = ["unix-fs"]` |
| `anyflow-capability-files` | `default = ["unix-fs"]` |
| `anyflow-capability-clipboard` | `default = ["linux-backends"]` |
| `anyflow-capability-notifications` | `default = ["linux-dbus"]` |
| `anyflow-runtime`, `anyflow-daemon` | `default = ["upower"]` |
| `anyflow-capability-battery` | `default = []`, `upower = ["dep:zbus"]` |
| `proto`, `control`, `platform-linux`, `cli`, `gui` | none |

Every feature is **additive and Linux-enabling**; none excludes another. So
`--all-features` is precisely the default set plus `battery/upower` — which the
daemon turns on anyway. It is the **shipped Linux desktop configuration**, not an
invalid combination assembled to please a linter. `--all-features` is therefore
valid and is used.

It is also what makes the `linux-dbus` half of the notifications crate —
`backend/dbus.rs`, `backend/logind.rs`, and the `real_dbus` / `real_lock` test
targets — reachable by clippy at all. With the feature off those compile to empty
binaries, and **the TC2 defect would have been invisible to the gate meant to
catch it.**

`--all-targets` is equally load-bearing: both U2 test defects live in `tests/`,
and a clippy run that skipped test targets would have missed both.

The opposite direction, `--no-default-features`, is deliberately *not* this job's
business: the portable boundary is measured on a Windows runner by
`portable-windows-msvc.yml`, which is the only place it means anything.

### Stable, not MSRV — and the two are kept apart

This job runs **stable**, which is what `desktop/rust-toolchain.toml` pins and
therefore what a contributor sees locally. It answers:

> Does the current supported development toolchain consider this repository
> warning-free?

The MSRV gate stays where it is — the `msrv` job in `linux-distro-compat.yml`,
pinned to exactly the declared `rust-version = "1.88"` — and answers the other
question:

> Does the declared minimum compiler build the project?

Keeping them apart matters in both directions. Clippy's lint set changes between
releases, so pinning this job to the MSRV would freeze the lints at whatever 1.88
knew and defeat the purpose. And a new lint appearing on stable is a reason to
fix the code, never a reason to raise `rust-version`.

**`rust-version` was not raised.** `desktop/Cargo.toml` is unmodified. The
distinction is written into the workflow header, as instructed.

---

## 10. Android CI architecture

**New file:** `.github/workflows/android-ci.yml` — *"Android · build + unit
tests"* (CI-004).

Before this file, a pull request touching only `android/**` ran **zero** GitHub
checks; both desktop workflows are filtered to `desktop/**`. U2 P1 demonstrated
the consequence directly (§12).

| | |
|---|---|
| Runner | `ubuntu-24.04` |
| Triggers | `pull_request` + `push` on `[main, develop]`, paths `android/**` and this workflow; `workflow_dispatch` |
| Concurrency | `android-ci-${{ github.ref }}`, `cancel-in-progress: true` |
| Permissions | `contents: read` |
| Actions | `actions/checkout@v5`, `actions/setup-java@v4` (with its own first-party `cache: gradle`) |
| Timeout | 45 minutes |

**No emulator. No device. `connectedAndroidTest` is never invoked.** The header
says so explicitly, in the same voice the existing workflows use to refuse to be
cited as runtime certification.

### Gates

```yaml
- run: ./gradlew --no-daemon --max-workers=2 :app:assembleDebug :fixture:assembleDebug
- run: ./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest
- run: ./gradlew --no-daemon --max-workers=2 :app:testDebugUnitTest
```

1. **`:app:assembleDebug`** — the compilation gate. `:fixture:assembleDebug` is
   included because `settings.gradle.kts` includes that module beside `:app` and
   *nothing else ever compiles it*.
2. **`:app:assembleDebugAndroidTest`** — compiles the instrumented test
   **sources** without running them. This needs no Android runtime and is the
   step that would have caught the break described in §16.
3. **`:app:testDebugUnitTest`** — the JVM suites.

Plus a non-gating `What ran` step that parses the JUnit XML and prints every
suite with its counts, so a reviewer sees the P1/P2/P3 regression suites in the
log without downloading an artifact. It was extracted from the YAML and executed
verbatim against real results to confirm the heredoc survives GitHub's block-
scalar dedent (§18).

`--no-daemon` because a CI runner is discarded after one build. `--max-workers=2`
is passed explicitly even though `android/gradle.properties` already sets it, so
a change to that file cannot silently make CI behave differently from a local run.

---

## 11. Android SDK / JDK derivation

Nothing is pinned from memory. Every version is read out of the repository, and
two steps re-derive it at run time and fail if it has moved.

| Derived | Source | Value |
|---|---|---|
| AGP | `android/gradle/libs.versions.toml` | `8.8.0` |
| Kotlin | `android/gradle/libs.versions.toml` | `2.1.0` |
| Gradle | `android/gradle/wrapper/gradle-wrapper.properties` | `8.11.1`, SHA-256 pinned |
| `compileSdk` | `android/app/build.gradle.kts` | `35` |
| `targetSdk` / `minSdk` | `android/app/build.gradle.kts` | `35` / `29` |
| Java level | `sourceCompatibility = JavaVersion.VERSION_17`, `kotlinOptions.jvmTarget = "17"` | **17** |
| Test task | AGP's own | `:app:testDebugUnitTest` |

**JDK 17** is used because that is the level the module declares it compiles
against, and the minimum AGP 8.8 accepts. A guard step re-reads both declarations
and fails if they disagree with the installed JDK:

```
sourceCompatibility : 17
kotlin jvmTarget    : 17
installed JDK       : 17
```

**SDK platform 35** is installed by parsing `compileSdk` out of the build file at
run time — not hard-coded — and through the SDK's **own `sdkmanager`**, not by
downloading an archive from anywhere. The step fails if the parse yields nothing,
so the answer can never silently default.

**Gradle wrapper.** `gradle-wrapper.properties` already pins
`distributionSha256Sum` and sets `validateDistributionUrl=true`, so a substituted
distribution fails the build rather than running. That is why no separate
wrapper-validation action is introduced; the workflow prints those three lines as
evidence instead.

**Local runs used JDK 21** (the only JDK installed on this host besides a
too-new system 25). CI uses 17. The difference is recorded rather than hidden;
the declared compile level is 17 in both cases, and GitHub Actions is the
authoritative environment for the gate.

### Android lint — debt, not a gate

Audited as instructed. `:app:lintDebug` currently reports **1 error and 47
warnings**. The single error:

```
app/src/main/java/io/github/yurisismotto/anyflow/ui/ClipboardTileService.kt:82:
Error: TileService#startActivityAndCollapse(Intent) is deprecated.
Use TileService#startActivityAndCollapse(PendingIntent) instead.
[StartActivityAndCollapseDeprecated]
```

**The code is already correct.** It branches on
`Build.VERSION.SDK_INT >= UPSIDE_DOWN_CAKE` and passes a `PendingIntent` on API
34+, where the `Intent` overload throws. The flagged line is the API 29–33
branch — and on those releases the `PendingIntent` overload **does not exist**.
The lint check flags the call site without reading the version gate.

So there is no source fix to make. The only routes to a green `lintDebug` are:

* `@Suppress("StartActivityAndCollapseDeprecated")` at the call site — a
  suppression, not a fix;
* a `lint-baseline.xml` — which hides this error *and* the next one;
* dropping the pre-34 branch — a Quick Settings tile regression on Android 10
  through 13, i.e. a product change, and "Quick Panel" is explicitly out of scope
  for this branch.

None qualifies as "a real small source compatibility fix, clearly correct and
independently testable". **Lint therefore did not become a gate**, no global
suppression or baseline was added, and the reasoning above is written into the
workflow file itself so the next person inherits the audit rather than repeating
it.

---

## 12. Android test execution — P1/P2/P3 regression

No test count is hard-coded in the workflow. The task is authoritative: adding a
suite needs no workflow edit, and deleting one cannot be hidden by a number
nobody updated.

Observed total, from a forced fresh run (`--rerun`) on this host:

```
41 suites, 581 tests, 0 failures, 0 errors, 0 skipped
```

The named regression suites all ran and all passed:

| Wave | Suite | tests |
|---|---|---|
| P1 | `MultiPeerRoutingTest` | 13 |
| P1 | `PeerTargetTest` | 18 |
| P1 | `UiMappingTest` | 31 |
| P2 | `BatteryReadingTest` | 5 |
| P3 | `NotificationConvergenceTest` | 19 |
| P3 | `NotificationSourceTest` | 60 |
| P3 | `NotificationRolesTest` | 19 |
| P3 | `NotificationHardeningTest` | 13 |
| P3 | `NotificationsProtocolTest` | 20 |

All nine report `0 failures, 0 errors`.

---

## 13. Workflow permissions and security

| check | desktop-quality | android-ci |
|---|---|---|
| `permissions` | `contents: read` | `contents: read` |
| write permissions | none | none |
| `secrets.*` / `GITHUB_TOKEN` | none | none |
| keystore / signing material | n/a | none — debug builds use the locally generated debug keystore |
| ADB keys | n/a | none |
| artifact upload | none | none |
| `curl` / `wget` of a binary | none | none |
| third-party actions | none | none |
| actions used | `actions/checkout@v5` | `actions/checkout@v5`, `actions/setup-java@v4` |
| concurrency `cancel-in-progress` | yes | yes |

Both workflows are read-only and publish nothing. The Android SDK platform is
obtained through the SDK's own `sdkmanager`; the Gradle distribution through the
repository's SHA-256-pinned wrapper. No executable is downloaded from an
unverified URL anywhere.

Action pinning follows the existing repository convention (major version tags,
`actions/checkout@v5` as already used by both existing workflows).

---

## 14. Path-filter matrix

Simulated against the actual parsed YAML of all four workflows:

| scenario | workflows triggered |
|---|---|
| Android-only PR | `android-ci` |
| Desktop-only PR | `desktop-quality`, `linux-distro-compat`, `portable-windows-msvc` |
| Mixed Android + desktop PR | all four |
| Documentation-only PR | **none** |
| Change to `desktop-quality.yml` | `desktop-quality` only |
| Change to `android-ci.yml` | `android-ci` only |
| Change to `portable-windows-msvc.yml` | `portable-windows-msvc` only |
| Change to `linux-distro-compat.yml` | `linux-distro-compat` only |

Every requirement of §14 holds. Each workflow-file change triggers **only its own
workflow** — no duplicate triggers, and no matrix runs twice. All four
`concurrency.group` values are distinct, and all four have identical `pull_request`
and `push` path filters.

---

## 15. Files changed

### Modified (4 — all test code, no production source)

```
M  android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt   | 14 +
M  desktop/capabilities/clipboard/tests/real_backend.rs                                        | 181 ++-
M  desktop/capabilities/clipboard/tests/sensitive_capability.rs                                | 146 ++
M  desktop/capabilities/notifications/tests/real_dbus.rs                                       | 101 ++-
   4 files changed, 432 insertions(+), 10 deletions(-)
```

### Added (2)

```
?? .github/workflows/desktop-quality.yml
?? .github/workflows/android-ci.yml
```

### Deliberately untouched

* **`LINUX-UBUNTU-DEBIAN-COMPAT-U2.md`** — still untracked, unmodified,
  unstaged, unrenamed. `git add .` was never used.
* `.github/workflows/portable-windows-msvc.yml` and
  `.github/workflows/linux-distro-compat.yml` — byte-identical to `develop`.
* Every `src/` directory in the repository.
* `desktop/Cargo.toml`, `desktop/Cargo.lock`, `android/gradle/libs.versions.toml`,
  `android/gradle.properties`.

### The Android test fixture change

`NotificationUiFixtures.kt` did not compile. Found because the new
`assembleDebugAndroidTest` gate ran it for the first time:

```
e: NotificationUiFixtures.kt:95:9 No value passed for parameter 'selectedPeerHex'.
```

P1 added `selectedPeerHex` to `MainUiState` — the explicit-target model that
replaced `peers.first()` routing — and the instrumented fixture was never
updated, because nothing in CI or in a normal `./gradlew test` compiles
`androidTest` sources. **This is TC4's own thesis, demonstrated on itself.**

The fix is test-only and minimal: a `selectedPeerHex: String? = null` parameter
threaded through. `null` is the honest value and not a shortcut — this fixture
builds a state with exactly **one** trusted peer, and one candidate resolves to
`PeerTarget.Resolution.OnlyTrustedPeer` whether or not a choice was ever made. So
the consent screens under test get a target without the fixture asserting
anything about routing, which is `PeerTargetTest`'s subject. It is a parameter
rather than a hard-coded `null` so a future multi-peer consent test can state its
own choice.

No production Kotlin was touched, and no instrumented test was executed.

---

## 16. Focused local test results

Run sequentially, one command at a time, `cargo -j 2`, Gradle
`--no-daemon --max-workers=2`, host memory checked before each heavy step. No
build was run concurrently with another. No VM was booted. No Gradle or Kotlin
daemon was left behind (verified with `pgrep` afterwards: none).

### A — clipboard, focused

```
$ cargo test --locked -p anyflow-capability-clipboard --test sensitive_capability -- --test-threads=1
running 9 tests
  a_missing_wl_copy_makes_the_whole_clipboard_backend_unavailable ... ok
  a_missing_wl_paste_is_reported_truthfully_rather_than_as_a_working_clipboard ... ok
  a_new_looking_version_without_the_flag_is_treated_as_unsupported ... ok
  a_refused_sensitive_clip_reaches_no_process_at_all ... ok
  a_supported_wl_copy_offers_both_ordinary_and_sensitive_clipboard ... ok
  an_old_looking_version_that_has_the_flag_is_treated_as_supported ... ok
  an_unsupported_wl_copy_keeps_the_ordinary_clipboard_and_refuses_sensitive_clips ... ok
  the_sensitive_contract_holds_where_the_flag_exists ... ok          <-- new
  the_sensitive_contract_holds_where_the_flag_is_missing ... ok      <-- new
test result: ok. 9 passed; 0 failed
```

Real-session gate, supported host (`wl-copy` **with** `--sensitive`):

```
$ cargo test --locked -p anyflow-capability-clipboard --test real_backend -- --ignored --test-threads=1
test result: ok. 9 passed; 0 failed; 0 ignored
```

Real-session gate, **unsupported** host (Ubuntu/Debian-shaped `wl-copy` wrapper,
real compositor):

```
test a_sensitive_write_honours_this_backend_s_advertised_capability ... ok
sensitive marking on this host: unsupported (…)
```

### B — notifications, focused

```
$ cargo test --locked -p anyflow-capability-notifications --test real_dbus \
    -- --ignored --test-threads=1 the_real_server_answers a_notification_is closing_an_unknown
test the_real_server_answers_and_says_what_it_can_do ... ok
test a_notification_is_created_replaced_in_place_and_closed ... ok
test closing_an_unknown_id_is_a_success ... ok
test result: ok. 3 passed; 0 failed
```

### Both capability crates, whole suites

```
$ cargo test --locked -p anyflow-capability-clipboard -p anyflow-capability-notifications
passed=300  failed=0  ignored=21
```

The 21 ignored are the real-session gates in `real_backend`, `real_dbus` and
`real_lock`, which are `#[ignore]`d by design and were run separately above (the
remaining ones need a human dismissal or a soak flag).

### Portable boundary — the §4 companion fix, verified

```
$ cargo test --locked --no-run --no-default-features \
    -p anyflow-capability-clipboard -p anyflow-capability-notifications
Finished `test` profile
  Executable tests/sensitive_capability.rs   <-- now builds; was E0432 on the baseline
  Executable tests/real_backend.rs
  Executable tests/real_dbus.rs
  … (13 targets, all built)
```

---

## 17. Android JVM result

```
$ ./gradlew --no-daemon --max-workers=2 :app:testDebugUnitTest --rerun
BUILD SUCCESSFUL in 12s

41 suites, 581 tests, 0 failures, 0 errors, 0 skipped
```

```
$ ./gradlew --no-daemon --max-workers=2 :app:assembleDebug :fixture:assembleDebug
BUILD SUCCESSFUL

$ ./gradlew --no-daemon --max-workers=2 :app:assembleDebugAndroidTest
BUILD SUCCESSFUL in 21s          (after the NotificationUiFixtures.kt fix; see §15)
```

Toolchain for these runs: Gradle 8.11.1, AGP 8.8.0, Kotlin 2.1.0, compileSdk 35,
JDK 21 locally / JDK 17 in CI (§11).

---

## 18. Desktop clippy result

```
$ rustc -V
rustc 1.98.0 (88d9e12ae 2026-08-18) (Fedora 1.98.0-1.fc44)
$ cargo clippy -V
clippy 0.1.98 (88d9e12ae1 2026-08-18)

$ cargo fmt --all --check
(clean)

$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    Checking anyflow-proto / -core / -control / -runtime / -linux / -daemon
             / -cli / -gui / -capability-{battery,clipboard,files,notifications}
    Finished `dev` profile in 36.60s
(no diagnostics)
```

Twelve crates, every target, every feature, warnings denied — clean. Formatting
was applied once during this branch (`cargo fmt --all`) and the result re-checked.

---

## 19. Workflow static validation

No CI simulator was installed — no `act`, no Docker. Validation was static plus
targeted execution of the scripts themselves, using the `PyYAML` already present
on the host (nothing was installed for this).

| check | result |
|---|---|
| YAML parses (`yaml.safe_load`) | both files ✔ |
| job names, step counts | `desktop-quality`: 1 job / 9 steps · `android-ci`: 1 job / 10 steps ✔ |
| `pull_request` paths == `push` paths | both ✔ |
| `workflow_dispatch` present | both ✔ |
| `permissions: contents: read` | both ✔ |
| `concurrency` + `cancel-in-progress` | both ✔, distinct groups |
| `working-directory` | job-level `desktop` / `android`; one deliberate step-level `.` for `apt-get` ✔ |
| path-filter coherence across all 4 workflows | §14 ✔ |
| no secrets / write perms / arbitrary downloads | §13 ✔ |

Every derivation expression was **executed against the real repository**, not
merely read:

```
sourceCompatibility=17  jvmTarget=17        (Android Java-level guard)
compileSdk=35                               (Android SDK platform derivation)
agp=8.8.0  kotlin=2.1.0                     (Android summary step)
channel=stable -> PASS (moving channel)     (desktop channel drift guard)
```

The Android `What ran` step was extracted from the parsed YAML and run verbatim
against `app/build/test-results/`, confirming the embedded Python heredoc
survives GitHub's block-scalar dedent and produces the 41-suite / 581-test table.

GitHub Actions remains the source of truth (§20).

---

## 20. Existing workflows preserved

```
$ git diff --name-only .github/
(empty)
```

`linux-distro-compat.yml` and `portable-windows-msvc.yml` are **byte-identical**
to `develop`.

The P3 guard is intact and verified programmatically — the expected list is
compared against the actual directory contents:

| guard | directory | match |
|---|---|---|
| Core test files unchanged | `desktop/core/tests` | ✔ |
| Notification test files unchanged (classification) | `desktop/capabilities/notifications/tests` | ✔ |

`convergence.rs` is still in the notifications classification list and was not
removed or weakened. The expected set is exactly:

```
convergence.rs, dismiss.rs, hardening.rs, logging.rs,
real_dbus.rs, real_lock.rs, sink.rs
```

which matches the directory exactly.

**No desktop test file was added or renamed by this branch**, so no guard update
was required. `real_dbus.rs` remains `#![cfg(feature = "linux-dbus")]` and still
compiles to an empty binary with the feature off — verified in §16. The new
clipboard tests live inside the existing `sensitive_capability.rs`, which is now
classified honestly for the first time (§4).

---

## 21. Remaining CI / test debts

Recorded, not fixed — each deliberately out of scope for this branch.

1. **Android lint is not a gate.** 1 error, 47 warnings. The error is a false
   positive on a correctly version-gated `startActivityAndCollapse` call, and
   every route to green is a suppression, a baseline, or a Quick Settings product
   change. Full audit in §11. Making it a gate means deciding the tile question
   on its merits first.

2. **`portable-windows-msvc.yml` has no clipboard test-file classification
   guard.** It guards `core/tests` and `capabilities/notifications/tests` but not
   `capabilities/clipboard/tests` — which is exactly why the
   `sensitive_capability.rs` misclassification (§4) went unnoticed until the
   portable build broke. Adding a third guard block mirroring the existing two
   would prevent the recurrence. Not done here: modifying that workflow was
   outside this branch's remit beyond preserving it, and the underlying break is
   now fixed.

3. **`real_backend.rs` and `real_dbus.rs` remain `#[ignore]`d and CI-unreachable.**
   TC1's contract is now mirrored into a CI-runnable suite (§4); TC2's is not, and
   cannot easily be — a notification server is required. The `linux-dbus` code
   path is at least *compiled and linted* by the new desktop gate (§9).

4. **Instrumented Android tests are compiled but never executed in CI.** By
   instruction: no emulator, no device. Their behaviour is certified on hardware.

5. **The desktop clippy gate lints one feature configuration.** `--all-features`
   is the shipped Linux product (§9); the `--no-default-features` portable
   configuration is compile-checked on Windows but not clippy-checked anywhere.

6. **Invariant 3 of the corrected TC2 test detects disagreement, not every
   miswiring.** A mutation that happens to agree on a given host is not caught on
   that host (§7). The direction with a user-visible consequence is caught.

7. **The `GetCapabilities` error is still swallowed in production.**
   `DbusSink::connect` uses `.unwrap_or_else(|_| Vec::new())`, so a failed query
   is indistinguishable from an empty advertisement inside the sink. The new test
   detects it from outside; the production seam was not changed, because this
   branch does not change product behaviour.

---

## 22. Git

```
$ git status --short
 M android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt
 M desktop/capabilities/clipboard/tests/real_backend.rs
 M desktop/capabilities/clipboard/tests/sensitive_capability.rs
 M desktop/capabilities/notifications/tests/real_dbus.rs
?? .github/workflows/android-ci.yml
?? .github/workflows/desktop-quality.yml
?? LINUX-UBUNTU-DEBIAN-COMPAT-U2.md

$ git diff --check
(clean — no whitespace errors, no conflict markers)

$ git diff --stat
 .../yurisismotto/anyflow/NotificationUiFixtures.kt |  14 ++
 .../capabilities/clipboard/tests/real_backend.rs   | 181 ++++++++++++++++++++-
 .../clipboard/tests/sensitive_capability.rs        | 146 +++++++++++++++++
 .../capabilities/notifications/tests/real_dbus.rs  | 101 +++++++++++-
 4 files changed, 432 insertions(+), 10 deletions(-)

$ git diff --name-status
M	android/app/src/androidTest/java/io/github/yurisismotto/anyflow/NotificationUiFixtures.kt
M	desktop/capabilities/clipboard/tests/real_backend.rs
M	desktop/capabilities/clipboard/tests/sensitive_capability.rs
M	desktop/capabilities/notifications/tests/real_dbus.rs
```

Nothing was staged, committed, pushed, or turned into a pull request.
`git add .` was never used. `LINUX-UBUNTU-DEBIAN-COMPAT-U2.md` remains untracked
and unmodified.

---

## 23. Pre-push verdict

```
U2 TEST/CI HARDENING: LOCAL PASS
TC1 CLIPBOARD CAPABILITY-AWARE TEST: PASS
TC2 NOTIFICATION CLIPPY TEST DEFECT: PASS
DESKTOP CLIPPY CI: IMPLEMENTED — AWAITING GITHUB
ANDROID CI: IMPLEMENTED — AWAITING GITHUB
```

This is **not** a final pass. Two of the four deliverables are GitHub Actions
workflows that have never executed on GitHub, and no claim about them is made
beyond static validation and local execution of their individual commands.

---

## POST-PUSH CI EVIDENCE — PENDING

*Nothing below this line may be filled in until the workflows have actually run
on GitHub. Do not fabricate run IDs.*

| workflow | run ID | conclusion | duration |
|---|---|---|---|
| Desktop quality · fmt + clippy | _pending_ | _pending_ | _pending_ |
| Android · build + unit tests | _pending_ | _pending_ | _pending_ |
| Linux distro compatibility (regression) | _pending_ | _pending_ | _pending_ |
| Portable core · Windows MSVC (regression) | _pending_ | _pending_ | _pending_ |

**To confirm after the first PR run:**

- [ ] `desktop-quality` goes green on a desktop-only change.
- [ ] `android-ci` goes green on an Android-only change, and is the **only**
      workflow that runs for one.
- [ ] `android-ci` reports `41 suites, 581 tests, 0 failures` (or a number the
      task itself produces — the workflow hard-codes none).
- [ ] The Java-level guard, the `compileSdk` derivation and the
      `rust-toolchain.toml` channel guard all pass on a clean runner.
- [ ] `sdkmanager` is present at the assumed path on `ubuntu-24.04`, and
      `platforms;android-35` installs.
- [ ] `portable-windows-msvc` goes **green** — it is red on `develop` today, and
      §4's classification fix is the reason to expect it to recover. If it is
      still red, the remaining cause is something this branch did not find.
- [ ] A documentation-only PR triggers none of the four.

**Verdict to be issued only after the above:** _pending_
