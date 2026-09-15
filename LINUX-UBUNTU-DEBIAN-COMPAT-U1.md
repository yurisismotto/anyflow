# AnyFlow — Linux distribution compatibility
# U1: Ubuntu / Debian compatibility implementation

**Branch:** `feature/linux-debian-ubuntu-compat-v1`
**Baseline:** `06ac9eb` — U0 audit merged (PR #24), working tree clean at the start.
**Authoritative input:** `LINUX-UBUNTU-DEBIAN-COMPAT-U0.md` (verdict: *implementation ready*).
**Mode:** implementation of the U0 delta only. Nothing committed, nothing pushed, no PR.

---

## 1. Baseline

```console
$ git branch --show-current
feature/linux-debian-ubuntu-compat-v1
$ git status --short
(clean)
$ git log --oneline -3
06ac9eb Merge pull request #24 from yurisismotto/research/linux-debian-ubuntu-compat-v1
4c32e79 docs(linux): audit Ubuntu and Debian compatibility
7822a06 Merge pull request #23 from yurisismotto/cert/notifications-v1-n6-final-certification
$ git diff --check
(clean)
```

**Host:** Fedora 44, kernel 7.1.9-200.fc44, GNOME Shell 50.4 on Wayland, rustc/cargo 1.98.0,
GTK 4.22.4 / libadwaita 1.9.3 from a session-local devel prefix, podman 5.8.4.

---

## 2. U0 findings addressed

| # | U0 finding | What U1 did | Where |
| --- | --- | --- | --- |
| **U-1** | `wl-copy --sensitive` absent on all three targets; the refusal is only discovered when a password fails to arrive | Kept the fail-closed refusal unchanged. Split "clipboard available" from "sensitive marking available" through the backend seam, the control protocol, the CLI and the GUI, so the gap is stated **before** the first sensitive clip. Six new capability tests plus a security regression | §13–§18 |
| **U-2** | The one product string naming a distribution: *"Fedora: `sudo dnf install wl-clipboard`"* | Replaced with a distro-neutral message naming the binaries and the package. Audited: **zero** package-manager commands and **zero** distribution names remain in any production string literal | §17 |
| **U-3** | Ubuntu 24.04's libadwaita is exactly 1.5.0 against a 1.5 floor, unguarded | The `ubuntu:24.04` CI row compiles the GUI against that distribution's own libadwaita, plus a guard that goes red if the image stops supplying 1.5.x — so the row cannot quietly stop being a gate | §7, §11 |
| **U-4** | Declared MSRV 1.82 is false; the lockfile needs 1.88 | `rust-version = "1.88"`, measured not chosen, with the reasoning at the declaration. An MSRV CI job pins exactly that number | §5, §6, §24 |
| **U-5** | No Linux CI job at all | `.github/workflows/linux-distro-compat.yml`: an MSRV job plus a three-row distro matrix | §11, §12 |
| **U-6** | README and architecture docs equate Linux with Fedora | Rewritten as a Linux section with per-distribution prerequisites, toolchains and honest support status; five architecture documents de-Fedora'd where "Fedora" meant "the platform" | §19 |
| **U-7** | The GTK floor is genuinely 4.12 (`CssProvider::load_from_string`) | Floor preserved, and the exact API that sets it recorded beside the Cargo feature | §10 |
| **U-8** | No `.desktop` / AppStream file | **Deferred**, deliberately — packaging wave | §20 |

---

## 3. Scope

**In:** L1 (true MSRV), L2 (sensitive-clipboard capability model, CLI and GUI), L3 (Linux
distro CI + MSRV gate), L4 (distro-neutral runtime remediation), L5 (de-Fedora the generic
documentation).

**Out, and verified out:** no protocol change, no Android change, no KDE work, no packaging
(`.deb`, `.rpm`, `.desktop`, AppStream, systemd units, firewall, AppArmor, SELinux), no
architectural redesign, no new dependency, no `Cargo.lock` change.

---

## 4. Files changed

```
 README.md                                             | 125 +++++-
 desktop/Cargo.toml                                    |  25 +-       L1
 desktop/capabilities/clipboard/src/backend/mod.rs     |  44 +++      L2
 desktop/capabilities/clipboard/src/backend/wayland.rs |  34 +-       L2, L4
 desktop/capabilities/notifications/tests/real_dbus.rs |  18 +-       consequence of L1 (§24)
 desktop/cli/src/main.rs                               |  59 ++-      L2
 desktop/control/src/lib.rs                            |  17 +        L2
 desktop/gui/Cargo.toml                                |  18 ++       floors documented
 desktop/gui/src/views/clipboard.rs                    | 352 ++++++   L2
 desktop/gui/src/views/mod.rs                          |  29 ++       test-harness fix (§25)
 desktop/gui/src/views/notifications.rs                |  12 +-       test-harness fix (§25)
 desktop/runtime/src/server.rs                         |   7 +        L2
 docs/architecture/{CLIPBOARD,FILES,NOTIFICATIONS,OVERVIEW,PROTOCOL}.md |  86 +-  L5
 .github/workflows/linux-distro-compat.yml             | new          L3
 desktop/capabilities/clipboard/tests/sensitive_capability.rs | new   L2 tests
```

17 modified, 2 new. **`desktop/Cargo.lock` is unchanged.** The only non-comment line added
to any manifest in the tree is `rust-version = "1.88"`.

Two files need a word of explanation, because a reviewer will ask why they are here:

* **`capabilities/notifications/tests/real_dbus.rs`** — seven `x % n == 0` rewritten as
  `x.is_multiple_of(n)`. Not opportunistic tidying: see §24.
* **`gui/src/views/{mod,notifications}.rs`** — a GTK test-harness constraint that adding a
  second display test exposed. See §25.

---

## 5. MSRV correction

`desktop/Cargo.toml` now declares `rust-version = "1.88"`, at the single authoritative
`[workspace.package]` level. All twelve crates inherit it through
`rust-version.workspace = true`; there is no second or conflicting declaration anywhere in
the tree.

The number was **re-measured** on this branch rather than taken from U0:

```console
$ cargo metadata --format-version 1 --locked   # highest rust_version over all locked packages
    1.88.0  time 0.3.55 / time-core 0.1.9 / time-macros 0.2.32
      1.88  rcgen 0.14.10
    1.87.0  wasip2 1.0.4
      1.87  zbus 5.19.0 / zbus_macros / zbus_names / zcheapstr / zvariant / zvariant_derive / zvariant_utils
    1.85.0  cfg-expr 0.20.9 / deranged 0.5.8

EFFECTIVE MSRV OF THE COMMITTED Cargo.lock : 1.88.0
locked crates demanding more than 1.82     : 36
```

**U0 said 34; this measurement says 36.** The difference is a counting choice, not a
disagreement: the 36 includes two crates that are only ever resolved for non-Linux targets
(`wasip2`, and one of its dependents). The floor itself — 1.88.0, set by `time` — is
identical, and it is what the declaration follows. No dependency was downgraded and
`Cargo.lock` was not touched.

The reasoning lives at the declaration rather than only in this report: which crates set the
floor, that Debian 13's 1.85.1 and Ubuntu 24.04's `rustc-1.82` were measured failing on it,
why downgrading `zbus`/`rcgen`/`time` was rejected, and which CI job enforces the number.

**`desktop/rust-toolchain.toml` was left at `channel = "stable"`.** It is a developer
convenience and does not conflict with a declared MSRV — but it does interact with CI in a
way that cost a probe to find, recorded in §12.

---

## 6. Cargo evidence

```console
$ git status --short -- desktop/Cargo.lock
(nothing — unchanged)

$ git diff -- protocol/
(empty)

$ git diff -- '*/Cargo.toml' | grep '^+' | grep -v '^+#'
+rust-version = "1.88"
```

No dependency was added. U0 proved OpenSSL, Avahi, libdbus and libX11 unnecessary
(`rustls`/`ring`, `mdns-sd`, `zbus`, `x11rb`), and nothing in this wave contradicted that:
every container compile below ran without any of them, and without `protoc`.

---

## 7. Ubuntu 24.04 LTS

The load-bearing row, because its libadwaita sits exactly on the floor.

```
########## Ubuntu 24.04.4 LTS ##########
gtk4          4.14.5
libadwaita-1  1.5.0
glib-2.0      2.80.0
glib-compile-resources: /usr/bin/glib-compile-resources
toolchain:    rustc 1.88.0 (6b00bc388 2025-06-23) / cargo 1.88.0

cargo metadata --locked                 OK: lockfile resolves
cargo check --locked -p anyflow-daemon  Finished in 1m 01s
cargo check --locked -p anyflow-cli     Finished in    21s
cargo check --locked -p anyflow-gui     Finished in    55s   ← against libadwaita 1.5.0
cargo check --locked --workspace        Finished in    49s
cargo test  --locked (portable set)     325 passed · 0 failed · 9 ignored · 4 filtered
```

No PPA, no backport, no third-party repository, no bundled newer library: `libgtk-4-dev` and
`libadwaita-1-dev` came from the image's own archive. The GUI therefore compiled against
**libadwaita 1.5.0 with nothing to spare**, which is the property the CI row exists to keep
true.

Re-measured independently of U0, and confirming it:

```console
$ apt-cache policy rustc          Candidate: 1.75.0+dfsg0ubuntu1-0ubuntu7.4     ← below the floor
$ apt-cache search '^rustc-1\.'   rustc-1.74 … rustc-1.85, rustc-1.89, rustc-1.91
```

So Ubuntu 24.04's **own archive** can supply a working toolchain (`rustc-1.91`), and the
`rustc-1.82` package that the old declared MSRV implied would be enough is not.

---

## 8. Ubuntu 26.04 LTS

```
########## Ubuntu 26.04.1 LTS ##########
gtk4          4.22.4
libadwaita-1  1.9.1
glib-2.0      2.88.0
toolchain:    rustc 1.88.0 / cargo 1.88.0

cargo metadata --locked                 OK
cargo check --locked -p anyflow-daemon  Finished in 1m 01s
cargo check --locked -p anyflow-cli     Finished in    22s
cargo check --locked -p anyflow-gui     Finished in    55s
cargo check --locked --workspace        Finished in    49s
cargo test  --locked (portable set)     325 passed · 0 failed · 9 ignored · 4 filtered
```

The easiest of the three, as U0 predicted: a GTK stack essentially identical to the Fedora
certification host's.

---

## 9. Debian 13 trixie

```
########## Debian GNU/Linux 13 (trixie) ##########
gtk4          4.18.6
libadwaita-1  1.7.6
glib-2.0      2.84.4
toolchain:    rustc 1.88.0 / cargo 1.88.0

cargo metadata --locked                 OK
cargo check --locked -p anyflow-daemon  Finished in 1m 01s
cargo check --locked -p anyflow-cli     Finished in    22s
cargo check --locked -p anyflow-gui     Finished in    55s
cargo check --locked --workspace        Finished in    46s
cargo test  --locked (portable set)     325 passed · 0 failed · 9 ignored · 4 filtered
```

And the finding that made U-4 matter, re-measured:

```console
$ apt-cache policy rustc          Candidate: 1.85.1+dfsg1-1+deb13u1     ← below the 1.88 floor
```

Debian Stable's GTK stack is comfortably above the GUI's floors and its stock `rustc` is
below the lockfile's. The two facts stay separate in the documentation, exactly as U0 asked.

---

## 10. GTK and libadwaita floors

Unchanged, and now explained where they are declared
([gui/Cargo.toml](desktop/gui/Cargo.toml)):

| Floor | The API that sets it | Site |
| --- | --- | --- |
| `gtk4` **v4_12** | `gtk::CssProvider::load_from_string` — `#[cfg(feature = "v4_12")]`; at v4_10 the theme installer does not compile | [gui/src/lib.rs](desktop/gui/src/lib.rs) |
| `libadwaita` **v1_5** | `adw::Dialog` | [views/pairing.rs](desktop/gui/src/views/pairing.rs) |
| `libadwaita` **v1_5** | `adw::AlertDialog` | [views/peers.rs](desktop/gui/src/views/peers.rs) |

Neither was lowered, and no synthetic version checker was added: `system-deps` already
enforces both against the distribution's pkg-config metadata, and the Ubuntu 24.04 CI row is
what proves the libadwaita half is still honoured by a real archive. The comment names the
ungated `load_from_data(&str)` fallback for the GTK floor — recorded as available, not
recommended, since every supported target is ≥ 4.14.

---

## 11. Linux CI design

`.github/workflows/linux-distro-compat.yml`, two jobs.

**Job `msrv`** — the regression gate for §5. Installs exactly the declared toolchain, asserts
the workflow's pinned number still matches `desktop/Cargo.toml` (so the two cannot drift into
testing a version nothing claims), then `cargo metadata --locked`, `cargo check --locked` for
the three binaries, and `cargo check --locked --workspace --all-targets`. If a future
dependency bump raises the real floor, this job is what goes red — and its comment says
explicitly that the fix is to identify the crate that moved it, not to bump `rust-version`.

**Job `distro`** — three container rows: `ubuntu:24.04`, `ubuntu:26.04`, `debian:trixie`.
Each one:

1. records the distribution identity;
2. installs **only** U0's measured minimum — `gcc libc6-dev pkg-config libgtk-4-dev
   libadwaita-1-dev` (plus `ca-certificates curl git` for the toolchain and the checkout
   action, labelled as CI tools so they are not mistaken for product dependencies) — from
   that image's own archive, with no PPA, backport, third-party repository or bundled
   library;
3. prints `gtk4`, `libadwaita-1` and `glib-2.0` versions as evidence, fails loudly if
   `glib-compile-resources` is not present after `libgtk-4-dev` (U0 measured that it is), and
   on the 24.04 row **fails if libadwaita is no longer 1.5.x** — because at that point the
   row would still be green while no longer testing the floor it exists for;
4. installs Rust pinned to the declared MSRV, and verifies the pin actually took;
5. runs `cargo metadata --locked`, then `cargo check --locked` for `anyflow-daemon`,
   `anyflow-cli` and **`anyflow-gui`** separately, then the whole workspace;
6. runs the portable and Linux-generic test suites;
7. prints what the row proved — and what it did not.

**What it claims.** The workflow header, and the final step of every row, say **BUILD
COMPATIBILITY** and state that it is *not* desktop runtime certification: no compositor, no
session bus, no logind session, no notification server, no link-local network. Clipboard,
notifications, lock detection, GUI rendering and mDNS are named as things a container may
never be cited for.

**Native libraries versus the Rust toolchain.** The header sets out why the rows use the
distribution's own GTK and a separately installed Rust: whether a given distribution's
default `rustc` package is new enough is a documentation question, answered per distribution
in the README, and making it a CI failure would turn this matrix red for a reason unrelated
to AnyFlow's source.

**Deliberately not added:** a `cargo`/apt cache. A three-row GTK matrix is not cheap, and a
cache would help — but `actions/cache` inside a container adds a failure mode that is harder
to read than the cost it saves. Recorded as a cost to revisit, not an oversight.

---

## 12. CI trigger audit

Reviewed as if it were someone else's PR (§27). Verified mechanically, not by reading:

| Check | Result |
| --- | --- |
| no `feature/**` push trigger | **ok** — the only occurrence of `feature/` in the file is the comment saying why |
| PR + push on `[main, develop]` only | **ok** |
| path filters | `desktop/**` (which covers `desktop/Cargo.toml` and `Cargo.lock`) and the workflow itself |
| `workflow_dispatch` | **present** |
| `concurrency` + `cancel-in-progress` | **present**, keyed on `github.ref` |
| `continue-on-error` | **never used as a key** — the only occurrence is a comment saying it is not |
| `fail-fast: false` | present, so one red row does not hide the others; it does **not** soften the result |
| allow-failure rows | **none**; Ubuntu 24.04, Ubuntu 26.04 and Debian 13 are all mandatory |
| GUI compilation | **mandatory on every row** |
| `--locked` | on all 9 `cargo check` / `test` / `metadata` invocations across both jobs — verified by parsing the YAML, not by grep |
| secrets | **none used** |
| `sudo` inside a container | **none** — the three `sudo` lines are in the `msrv` job, which runs on a non-root GitHub runner |
| `permissions` | `contents: read` |
| package cache behaviour | `apt-get update` then `--no-install-recommends`; no third-party repository is ever enabled |

**One trap found by running it, not by reading it.** `desktop/rust-toolchain.toml` pins
`channel = "stable"`, and rustup honours that file over its own default. The first local
container probe of this workflow reported installing 1.88.0 and then compiled on **1.98.1**.
`RUSTUP_TOOLCHAIN` is now set in the distro rows (it outranks the file), the `msrv` job uses
`rustup run`, which bypasses it, and both jobs assert the version they actually got. Without
this the whole matrix would have proved nothing about the MSRV while looking green.

---

## 13. Clipboard capability model

U0's §11 asked for "clipboard backend available" and "sensitive clipboard marking available"
to stop being one bit. They now are three separate questions, asked and reported separately,
through the concepts that already existed — no second policy store was invented.

**Backend seam** ([backend/mod.rs](desktop/capabilities/clipboard/src/backend/mod.rs)) —
`ClipboardBackend` gains `availability()` beside the existing `sensitive_support()` and
`watch_availability()`. All three are pure predicates answered from the single startup probe;
none performs I/O, so `status` stays free of side effects. `WaylandBackend::availability()`
reports the helper binaries only; `Unsupported` reports its reason; `MemoryBackend` gains a
matching switch for tests.

**Control protocol** ([control/src/lib.rs](desktop/control/src/lib.rs)) —
`ClipboardStatusReport` gains `backend_available: bool`, `#[serde(default = "default_true")]`
like `sensitive_available` beside it, so a version skew between `anyflow` and the agent cannot
fail to parse a status report over a display field.

**Daemon** ([runtime/src/server.rs](desktop/runtime/src/server.rs)) — asks all three and
reports all three.

The reason they must stay apart is concrete rather than theoretical, and the comments say so:
Ubuntu 24.04, Ubuntu 26.04 and Debian 13 are all "ordinary yes, sensitive no". One bit would
have to either call ordinary mirroring broken, which it is not, or claim sensitive clips will
be written, which they will not.

---

## 14. Sensitive clipboard behaviour

**Unchanged, and deliberately so.** The write path still refuses a `sensitive_hint` clip when
the backend cannot mark it (PLAT-DEC-013). Nothing was made to silently downgrade, nothing
guesses from a package version, and `--sensitive` is never dropped to force a write through.

Detection is still a **capability probe**: `probe_sensitive()` reads `wl-copy --help` for the
option. The `--version` output is never consulted — proved by test, not by inspection (§15,
cases E and F). U0's reason holds: Fedora's `2.2.1^git20251124` carries the flag and Ubuntu's
and Debian's `2.2.1` do not, with the same version string.

What changed is **when the user learns about it**: the state is now reported up front by both
the CLI and the GUI, rather than only inside the failure message of the first password that
does not arrive.

---

## 15. CLI UX

`anyflow clipboard status`, rendered by a real daemon on this host:

```console
Clipboard
  backend              wl-clipboard
  detail               wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD selection; sensitive marking: yes
  ordinary clipboard   available
  sensitive clipboard  available
  auto-send            supported on this session
  caches               0 event id(s), 0 suppression entr(ies)
```

And the Ubuntu/Debian case — same host, same daemon, a `wl-copy` whose `--help` has no
`--sensitive`:

```console
Clipboard
  backend              wl-clipboard
  detail               wl-clipboard; watch: XFIXES on the Xwayland CLIPBOARD selection; sensitive marking: no
  ordinary clipboard   available
  sensitive clipboard  unavailable
                       this system's wl-copy does not support sensitive clipboard marking.
                       Install a wl-clipboard build whose wl-copy accepts `--sensitive`
                       (upstream added it in 2.3.0; some distributions backport it into an
                       earlier version).
                       A clip arriving with sensitive_hint set will be REFUSED rather
                       than written unmarked. Ordinary clipboard sharing is unaffected.
  auto-send            supported on this session
  caches               0 event id(s), 0 suppression entr(ies)
```

That is the shape U0 §12 asked for. Note the third line of the remedy: the version is offered
as guidance for choosing a build and explicitly not as the test, because a backport makes the
number unreliable. No package manager is named.

---

## 16. GUI UX

The clipboard page's "This computer" card gains a `SensitiveState` row
([views/clipboard.rs](desktop/gui/src/views/clipboard.rs)), built from the existing
`widgets::security_notice` — the same component the page already uses. The page was not
redesigned and no new component was introduced.

Three states rather than a boolean, because "there is no clipboard at all" and "the clipboard
works but cannot mark a clip" lead somewhere different: collapsing them would send a user with
no `wl-clipboard` hunting for a newer `wl-clipboard` version.

| State | Title | What the body says |
| --- | --- | --- |
| `Available` | *Sensitive clipboard: available* | marked clips are written and stay out of history |
| `NotMarkable` | *Sensitive clipboard: unavailable* | **ordinary clipboard sharing works normally** first; then that this desktop's `wl-copy` has no `--sensitive`; then that AnyFlow refuses such a clip because an unmarked password would sit in a clipboard manager's history without the user being told; then what to install |
| `NoClipboard` | *Sensitive clipboard: unavailable* | the question is moot, and the line above already says what is missing |

Accessibility, per U0 §13:

* **the state is in the words** — `title()` says "available" or "unavailable" outright, so
  nothing depends on seeing the tint or the icon;
* **no icon-only explanation** — icon, title and body are all present, and a test asserts
  every state has a title naming the answer and a body over 40 characters;
* **AT-SPI** — the notice carries `AccessibleRole::Group` and an explicit `Label` combining
  title and detail, so it is announced as one phrase rather than as fragments. Asserted by the
  display-requiring widget-tree test, which passed on this GNOME session.
* **not alarming for ordinary operation** — `NoClipboard` deliberately does *not* wear the
  caution tint, because the missing clipboard is already reported on the line above and saying
  it twice in warning colours overstates one fault into two. A test pins that a working
  desktop is never told about a refusal that cannot happen.

---

## 17. Distro-neutral remediation

**Before** ([wayland.rs](desktop/capabilities/clipboard/src/backend/wayland.rs)):

> `wl-copy/wl-paste not found on PATH. Install the wl-clipboard package (Fedora: `sudo dnf install wl-clipboard`).`

**After:**

> `wl-copy/wl-paste not found on PATH. Install the wl-clipboard package.`

The `--sensitive` remedy was rewritten in the same spirit (§15). The package is called
`wl-clipboard` on Fedora, Ubuntu and Debian alike, which is what makes this cheap to say once
and correctly. Package-manager commands moved to the README, per distribution, where they can
be right.

**Audit of production source** — every `.rs` under `desktop/*/src` and
`desktop/capabilities/*/src`, scanning **string literals only** with line comments stripped:

```
distribution names in a production string literal : 0
package-manager commands in a production string literal : 0
```

The only matches in the whole sweep are the forbidden-term lists inside the new
`#[cfg(test)]` assertion that enforces this rule, which is the intended result.

---

## 18. Security regression

U0 §16 required proof that the content of a refused sensitive clip reaches no process — by
measuring bytes, not by asserting an error string.

[`tests/sensitive_capability.rs`](desktop/capabilities/clipboard/tests/sensitive_capability.rs)
writes real shell scripts named `wl-copy` and `wl-paste` into a temporary directory, puts that
directory on `PATH`, and lets `WaylandBackend::detect()` find them exactly as it would find
the real tools. Every invocation appends its `argv` — and, for a real copy, its stdin between
markers — to a log file. The process boundary is therefore inside the test rather than mocked
away, which is the only way the question can be answered at all.

```rust
let err = backend.write_text(&text(CANARY), true).await.expect_err(…);
assert!(matches!(err, BackendError::Unavailable(_)));

assert!(!log.contains(CANARY));                       // no content anywhere in the log
assert_eq!(fakes.stdin_bytes(), "");                  // ZERO content bytes to any child
assert!(copies.is_empty());                           // wl-copy was not invoked at all
```

**Result: PASS.** Zero content bytes reached any child process, and no unmarked `wl-copy` was
spawned — the only `wl-copy` invocation on the refused path is the capability probe's
`--help`.

The measurement is not vacuous: the *supported* case in the same file asserts
`stdin_bytes().contains(CANARY)` and passes, so the capture mechanism demonstrably works and
an empty capture means something.

**`PATH` note.** The first version of these tests prepended the fake directory to `PATH`. The
"missing tool" cases then found this host's real `wl-clipboard` and passed for the wrong
reason. `PATH` is now **replaced**, with `#!/bin/sh` resolved by the kernel and the one
external command spelled `/bin/cat`. Recorded because a passing test that proves nothing is
worse than a failing one.

---

## 19. Documentation changes

**README.md** — *"Running on Fedora"* became *"Running on Linux"*:

* a support-status table distinguishing **runtime certified** (Fedora) from
  **build-supported — compatibility target** (Ubuntu 24.04, Ubuntu 26.04, Debian 13), with
  "build-supported" defined in the text as *compiles and passes its portable tests*, and
  explicitly **not** *discovery, pairing, clipboard, notifications or lock detection have been
  observed working there*;
* Ubuntu 22.04 and Debian 12 named as out of scope, with the reason;
* prerequisites, and what is **not** needed — no `protobuf-compiler`, no OpenSSL, no Avahi, no
  libdbus, no libX11 — with the reason for each;
* per-distribution package lists derived from U0 §6's probes. Fedora names in the Fedora
  block, Debian names in the Ubuntu/Debian block; nothing copied across;
* **Rust ≥ 1.88**, with a per-distribution toolchain table: Fedora 1.98 yes, Ubuntu 26.04
  1.93.1 yes, Ubuntu 24.04 1.75 no → `rustc-1.91` from its own archive (and a note that
  `rustc-1.82` is *not* enough), Debian 13 1.85.1 no → backports or rustup. The text says the
  distribution's own compiler package is convenient, not mandatory;
* the GTK 4.12 / libadwaita 1.5 floors, and `wl-clipboard`;
* a Known-limitations entry for the sensitive-clip gap naming all three affected
  distributions, that ordinary sharing is unaffected, and that the probe reads `--help` and
  never the version.

**docs/architecture/** — `CLIPBOARD.md` (title, the "what this is" block, the loop diagram,
the backend section heading, the dependency line, and a new section on sensitive marking as a
capability distinct from the clipboard, with the measured four-distribution table),
`OVERVIEW.md` and `NOTIFICATIONS.md` (diagram labels), `PROTOCOL.md` and `FILES.md` (flow
labels and headings).

**Deliberately preserved (§19 of the brief).** Every "Fedora" that names *the host evidence
was gathered on* stays: `NOTIFICATIONS.md`'s certification header and its N6 row, and every
`NOTIFICATIONS-V1-*`, `CLIPBOARD-V1-*` and `WAVE-0-*` report. **No ADR was touched.** The rule
applied while editing: *"Fedora was the certification host"* stays, *"Fedora is the platform"*
goes.

Two README lines were corrected as a direct consequence rather than as tidying: the layout
tree called `gui/` a placeholder and Known limitations said *"No GUI yet"*, both of which the
new section — which documents the GUI's GTK floors — would have flatly contradicted. The
desktop test count in the same code block was updated to the figure measured in §25.

---

## 20. Packaging boundary

Nothing added: no `.deb`, no `.rpm` change, no `.desktop`, no AppStream metainfo, no systemd
unit installation, no firewall file, no AppArmor profile, no SELinux policy, no install
script. `packaging/` is untouched — verified by `git status`. U0's **U-8** stays deferred and
was not fixed opportunistically.

One consequence of holding that line is recorded as a debt in §26:
`packaging/fedora/anyflow.spec:12` still says `BuildRequires: rust >= 1.82`, which is now
known to be false. U0's L1 named that file; the U1 brief's §22 forbids `.rpm` changes. The
brief won, and the README says in prose that the unit file is distribution-neutral and only
its directory is Fedora-named.

---

## 21. KDE boundary

Nothing. No `WatchSource` variant, no compositor probe, no Plasma branch, no KDE dependency,
no change to notification, clipboard or lock behaviour for KWin. The `wlr`/`ext` data-control
question is untouched. KDE remains an independent wave after Ubuntu/Debian certification.

---

## 22. Fedora regression

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check` | **clean** |
| `cargo build --workspace --locked -j 2` | **Finished**, no warnings |
| `cargo test --workspace --locked -j 2` | **717 passed · 0 failed · 22 ignored** |
| the same suite at `HEAD` (`06ac9eb`) | **703 passed · 0 failed · 22 ignored** |
| `cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings` | **clean** |

703 is exactly the figure N6 certified, so the baseline is the certified one and the delta is
**+14 tests, no regression**. Ignored count is unchanged at 22 (§25 explains why adding an
ignored test left it there).

Live behaviour on this host was checked with a real daemon on an isolated `XDG_RUNTIME_DIR`
and a spare port: it registered `battery.v1`, `clipboard.v1`, `files.v1` and
`notifications.v1`, resolved the GNOME notification server and the logind session as before,
and reported `sensitive marking: yes`. The host's own `anyflowd` was left running and
untouched; the test daemon was stopped and its state directory removed.

---

## 23. Container evidence

`podman` 5.8.4, each image's own archive, repository mounted read-only, one distribution at a
time. The probe script mirrors the CI workflow step for step.

| | Ubuntu 24.04.4 | Ubuntu 26.04.1 | Debian 13 trixie |
| --- | --- | --- | --- |
| `gtk4` | 4.14.5 | 4.22.4 | 4.18.6 |
| `libadwaita-1` | **1.5.0** | 1.9.1 | 1.7.6 |
| `glib-2.0` | 2.80.0 | 2.88.0 | 2.84.4 |
| `glib-compile-resources` after `libgtk-4-dev` | ✓ `/usr/bin` | ✓ | ✓ |
| toolchain used | rustc/cargo **1.88.0** | **1.88.0** | **1.88.0** |
| stock archive `rustc` | 1.75 ✗ | (not re-probed) | 1.85.1 ✗ |
| `cargo metadata --locked` | ✓ | ✓ | ✓ |
| `cargo check -p anyflow-daemon` | ✓ | ✓ | ✓ |
| `cargo check -p anyflow-cli` | ✓ | ✓ | ✓ |
| **`cargo check -p anyflow-gui`** | **✓** | **✓** | **✓** |
| `cargo check --workspace` | ✓ | ✓ | ✓ |
| portable + Linux-generic tests | 325 · 0 · 9 ign · 4 filt | 325 · 0 · 9 · 4 | 325 · 0 · 9 · 4 |

**This is compilation and dependency evidence. It is not certification** — no compositor, no
session bus, no logind session, no notification server, no link-local network took part.

Two honest caveats:

* The probe's "stock rustc" line is unreliable after the first row, because the three runs
  share one rustup home which is on `PATH`. Ubuntu 24.04's and Debian 13's stock versions were
  therefore re-measured separately with `apt-cache policy` in a clean container (§7, §9);
  Ubuntu 26.04's 1.93.1 is U0's figure, not re-measured here, and is above the floor either
  way.
* The 4 filtered-out tests per row are the root-privilege exclusions explained in §25.

---

## 24. Rust 1.88 evidence

Run explicitly, against the committed lockfile, exactly as the CI `msrv` job runs it:

```console
declared rust-version : 1.88
toolchain in use      : rustc 1.88.0 (6b00bc388 2025-06-23)

cargo metadata --locked                       OK: lockfile resolves
cargo check --locked -p anyflow-daemon -p anyflow-cli -p anyflow-gui
                                              Finished in 1m 44s
cargo check --locked --workspace --all-targets
                                              Finished in 19s
MSRV GATE DONE
```

**Rust 1.88 compiles the committed tree — every crate and every test target.** The declared
MSRV was not raised beyond what the lockfile demands, and nothing was downgraded to reach it.
Each of the three distro rows additionally compiled on 1.88.0 against its own GTK stack (§23),
so the number is enforced in four places rather than asserted in one.

### One consequence of telling the truth about the MSRV

Declaring 1.88 turned `cargo clippy -- -D warnings` red in
`capabilities/notifications/tests/real_dbus.rs`, with seven `manual_is_multiple_of` errors.

This looked like a pre-existing failure and is not one. Clippy gates that lint on the declared
`rust-version`, because `u64::is_multiple_of` stabilised in 1.87; at `rust-version = "1.82"`
the lint is suppressed. Isolated by bisecting the manifest line itself:

```console
rust-version = "1.82"  →  cargo clippy … --test real_dbus  →  0 errors
rust-version = "1.88"  →  cargo clippy … --test real_dbus  →  8 errors
```

So the seven sites are a direct consequence of L1 and were fixed here — a mechanical,
semantics-preserving rewrite to `cycle.is_multiple_of(n)`, which the MSRV gate above confirms
compiles on 1.88.0. They are the only reason a notifications test file appears in this diff.

Recorded at length because the first reading — "pre-existing, out of scope, report it" —
would have been wrong, and would have left the clippy gate red in a wave whose whole point is
that a declaration should match reality.

---

## 25. Tests

**Fedora host, exact counts, no rounding:**

| Suite | Passed | Failed | Ignored |
| --- | --- | --- | --- |
| `cargo test --workspace --locked` (this branch) | **717** | **0** | **22** |
| the same at `HEAD` `06ac9eb` (baseline) | 703 | 0 | 22 |
| `cargo test -p anyflow-capability-clipboard` (focused) | **85** | 0 | 9 |
| ↳ of which `tests/sensitive_capability.rs` (new) | **7** | 0 | 0 |
| `cargo test -p anyflow-gui` | **37** | 0 | 1 |
| ↳ of which new `SensitiveState` tests | **7** | 0 | 0 |
| `cargo test -p anyflow-gui -- --ignored --test-threads=1` (needs a display) | **1** | 0 | 0 |
| `cargo test -p anyflow-capability-clipboard --test real_backend -- --ignored` | 2 | **7** | 0 |

**+14 tests, 0 regressions.**

### The six cases U0 §15 asked for, and the security case

All seven are in `tests/sensitive_capability.rs` and all drive the real `WaylandBackend`
through real fake binaries on `PATH` (§18):

| Case | Test | Result |
| --- | --- | --- |
| **A** wl-copy present, `--sensitive` supported | `a_supported_wl_copy_offers_both_ordinary_and_sensitive_clipboard` | ordinary **available**, sensitive **available**, `--sensitive` passed, content delivered |
| **B** present, unsupported | `an_unsupported_wl_copy_keeps_the_ordinary_clipboard_and_refuses_sensitive_clips` | ordinary **available**, sensitive **unavailable**, ordinary write works, sensitive write **refused** |
| **C** wl-copy missing | `a_missing_wl_copy_makes_the_whole_clipboard_backend_unavailable` | backend **unavailable**, names the binary and the package |
| **D** wl-paste missing | `a_missing_wl_paste_is_reported_truthfully_rather_than_as_a_working_clipboard` | read and watch both **unavailable** and say which tool is missing; sensitive not claimed despite a capable `wl-copy` |
| **E** old-looking version, has the flag | `an_old_looking_version_that_has_the_flag_is_treated_as_supported` | **supported**; asserts `--version` was never invoked |
| **F** new-looking version, no flag | `a_new_looking_version_without_the_flag_is_treated_as_unsupported` | **unsupported**; refused; asserts `--version` was never invoked |
| **§16** | `a_refused_sensitive_clip_reaches_no_process_at_all` | **zero content bytes** to any child; no unmarked `wl-copy` spawned |

No test parses a version as the source of truth, and two of them assert that the production
code does not either — by checking the fake binaries' invocation log, which records every
`argv` the backend used.

### The GUI display test, and why `views/mod.rs` is in the diff

Adding a second `#[ignore]`d display test broke the existing one. GTK is initialised once per
**process** and binds to the thread that did it, while libtest gives every `#[test]` its own
thread — so the second display test does not fail an assertion, it panics inside
`gtk::Stack::new` with a message about threads and nothing about the page. Measured, not
feared: each test passed alone and the pair failed together.

`views/notifications.rs` already documented this constraint for the nine sections inside its
own test. The fix extends the same idea one level up: each page now contributes a *section*,
and `views::display_gate::every_page_widget_tree` is the single `#[ignore]`d test that calls
them in sequence on one thread. Sections still fail with their own names. No module was made
public to satisfy the harness, and
`cargo test -p anyflow-gui -- --ignored --test-threads=1` — the command the repository already
documents — is green again.

### Two exclusions, both recorded rather than hidden

* **The real-clipboard gate could not be run.** `real_backend` reported 7 failures, all
  `TimedOut`. The cause is environmental and the test file documents it: on GNOME, `wl-copy`
  and `wl-paste` block waiting for a seat the compositor will not grant behind a lock screen.
  Confirmed two ways — `loginctl` reports `LockedHint=yes` for this session, and the identical
  failure reproduces at clean `HEAD`. **Not a U1 regression**, and it is recorded as not done
  rather than inherited. It needs an unlocked session and belongs to the U2 run (gate
  G-CLIP). The backend did detect correctly throughout: `sensitive marking: yes`.
* **Four `anyflow-core` tests are skipped in containers.** They chmod a directory to 0000 and
  assert the read is `PermissionDenied` rather than "absent" — the Wave 0 defect they exist to
  prevent. A container job runs as root, which has `CAP_DAC_OVERRIDE`, so they fail for a
  reason unrelated to the distribution. They are excluded **by full name**, and the workflow
  has a guard that fails if the number of tests reaching for mode 0000 ever changes, so a
  fifth cannot be silently skipped. They run normally on every developer machine and on the
  certification host, where they pass.

---

## 26. Remaining debts

| # | Debt | Why it is still here |
| --- | --- | --- |
| **D1** | `packaging/fedora/anyflow.spec:12` still declares `BuildRequires: rust >= 1.82`, now known false | U0's L1 named this file; the U1 brief §22 forbids `.rpm` changes. **The brief won.** This is a one-line fix and should be the first thing the packaging wave does — or an explicit exception to §22 if it should not wait |
| **D2** | The real-clipboard Fedora gate was not run | The session is locked (§25). Needs an unlocked session; folds into U2's G-CLIP |
| **D3** | No CI cache for the cargo registry or apt | A three-row GTK matrix is not cheap. `actions/cache` inside a container adds a failure mode harder to read than the cost it saves (§11). Revisit if the job's duration becomes a problem |
| **D4** | Ubuntu 26.04's stock `rustc` (1.93.1) is U0's figure, not re-measured here | Above the floor either way, so nothing turns on it |
| **D5** | `packaging/fedora/` tree name, `.desktop` + AppStream (U-8), firewall recipes, the `StateDirectory` / `ReadWritePaths` inconsistency | packaging wave, exactly as U0 deferred them |
| **D6** | Everything runtime on Ubuntu and Debian | **U2.** Discovery, pairing, clipboard, notifications, lock detection, reconnect and the unit have never run on those distributions. The documentation says "build-supported", not "supported", for precisely this reason |
| **D7** | Anything KDE | its own wave |

---

## 27. Git status

```console
$ git status --short
 M README.md
 M desktop/Cargo.toml
 M desktop/capabilities/clipboard/src/backend/mod.rs
 M desktop/capabilities/clipboard/src/backend/wayland.rs
 M desktop/capabilities/notifications/tests/real_dbus.rs
 M desktop/cli/src/main.rs
 M desktop/control/src/lib.rs
 M desktop/gui/Cargo.toml
 M desktop/gui/src/views/clipboard.rs
 M desktop/gui/src/views/mod.rs
 M desktop/gui/src/views/notifications.rs
 M desktop/runtime/src/server.rs
 M docs/architecture/CLIPBOARD.md
 M docs/architecture/FILES.md
 M docs/architecture/NOTIFICATIONS.md
 M docs/architecture/OVERVIEW.md
 M docs/architecture/PROTOCOL.md
?? .github/workflows/linux-distro-compat.yml
?? desktop/capabilities/clipboard/tests/sensitive_capability.rs

$ git diff --check
(clean)

$ git diff --stat
 17 files changed, 756 insertions(+), 70 deletions(-)

$ git diff -- protocol/
(empty)

$ git status --short -- desktop/Cargo.lock protocol/ android/ packaging/ docs/adr/
(empty — all untouched)
```

**Artefact audit** — `target/`, `build/`, `*.deb`, `*.rpm`, `*.log`, `*.apk`, keys,
certificates, container output, temporary apt files, state or trust files: **none present** in
the working tree. Every probe artefact (the probe scripts, container logs, the shared cargo
and rustup homes, the per-distribution target directories, the isolated daemon's state
directory) lives outside the repository and was removed or left under
`~/.cache/anyflow-u1-probe`. No secret key material was read or printed.

**Nothing was added, committed, pushed, or opened as a PR.** `HEAD` is still `06ac9eb`.

### Machine state left behind

Four container images were already present and were reused; none was left running (every
probe used `--rm`). No package was installed on the host. `desktop/target/` was written to, as
any build does. One note worth recording for whoever runs U2: the first container probe used
the session scratchpad under `/tmp` for cargo and rustup homes and filled that tmpfs. The
probe now keeps them on disk under `~/.cache/anyflow-u1-probe` — a GTK matrix needs several
gigabytes per distribution.

---

## 28. Final verdict

Against §30's acceptance list:

| Criterion | Result |
| --- | --- |
| real MSRV declared as 1.88 | **yes** — measured from the lockfile, single declaration, reasoning recorded |
| Rust 1.88 compiles the current lockfile | **yes** — whole workspace, all targets, `--locked` |
| Ubuntu 24.04 native GTK/libadwaita GUI compiles | **yes** — against libadwaita **1.5.0** |
| Ubuntu 26.04 native GTK/libadwaita GUI compiles | **yes** |
| Debian 13 native GTK/libadwaita GUI compiles | **yes** |
| daemon compiles on all three | **yes** |
| CLI compiles on all three | **yes** |
| distro CI exists | **yes** — `linux-distro-compat.yml` |
| Ubuntu 24.04 a required CI row | **yes**, with a guard that keeps it meaningful |
| Debian 13 a required CI row | **yes** |
| no duplicate feature-push CI | **yes** — PR + `[main, develop]` push only |
| ordinary clipboard still functional where supported | **yes** — asserted by test and unchanged in code |
| sensitive clipboard still fail-closed | **yes** — and proved by byte count, not by error string |
| capability gap visible before failure | **yes** — CLI and GUI, both shown live |
| feature detection does not use version parsing | **yes** — and two tests assert `--version` is never invoked |
| runtime remediation distro-neutral | **yes** — zero distribution names and zero package-manager commands in production strings |
| Fedora remains green | **yes** — 717 passed, 0 failed, against a 703 baseline |
| protocol unchanged | **yes** — empty diff |
| no KDE work | **yes** |
| no packaging work | **yes** |
| no P0 / BLOCKER | **yes** |

Two things are worth stating plainly rather than leaving in a table. **The sensitive-clipboard
gap is not fixed and cannot be** — it is a property of the wl-clipboard those distributions
ship. What U1 changed is that a user now learns about it on a status page instead of when a
password fails to arrive, and that the refusal is proved to leak nothing. And **nothing in
this wave ran on an Ubuntu or Debian desktop session.** Every runtime claim about those
distributions is still unproven, which is why the documentation says *build-supported* and not
*supported*.

---

# UBUNTU/DEBIAN U1: PASS
# U2 REAL-SESSION CERTIFICATION READY
