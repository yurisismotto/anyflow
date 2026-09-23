# OmniBridge — Release Readiness v1, harness hardening

| Field | Value |
| --- | --- |
| **Branch** | `test/release-harness-hardening-v1` |
| **Baseline** | `29ee1cc` (merge of PR #64) |
| **Date** | 2026-09-22 |
| **Scope** | R4 — the recorded false results, turned into refusals that are themselves tested |
| **Verdict** | **29 recorded false results · 10 primitives · 40 self-test cases, each asserting in both directions · 3 static guards in CI · the policy in AGENTS.md** |

---

## 0. What this phase is for

Four waves of certification have produced twenty-nine cases where a harness
gave the wrong answer. Each was fixed where it was found. None of the fixes
stopped the *next* one, because each was a fix to a call site rather than to a
class.

This phase turns the classes into primitives, and — the part that matters —
**tests that the primitives refuse what they are supposed to refuse.** A
fail-loud check that never fails is the same vacuity one level up.

---

## 1. The corpus, read from the source documents

Not summarised from memory. Each row cites where it is recorded.

### 1.1 Packaging v1 — seven

Named in `packaging/tests/lifecycle-gates.sh`'s own header, which is where
they were first written down:

| # | What happened |
| --- | --- |
| 1 | an absent `runuser` — the call failed, nobody looked, the gate passed having run nothing |
| 2 | an empty tracing capture — the sentinel search over it passed |
| 3 | a glob that skipped a gate — L17's whole group, silently |
| 4 | `bash -s` with no stdin |
| 5 | a subuid that could not write — the setup produced nothing and the assertions ran against an empty directory |
| 6 | a relative path read as a named volume — the mount pointed at an empty volume instead of the source tree |
| 7 | six packages built with one shipped |

### 1.2 Lifecycle closure §5 — nine

[`RELEASE-LIFECYCLE-CLOSURE-V1.md` §5](../../certification/linux/RELEASE-LIFECYCLE-CLOSURE-V1.md):

| # | Defect | How it presented |
| --- | --- | --- |
| 1 | `dpkg-query -f "${Status}"` crossed a `sh -c`, where the unescaped `${Status}` expanded to empty | the installed-package count silently became **0** |
| 2 | the S3 journal grep tailed `-n 200` of a **persistent** journal | it matched a *previous* session's failures |
| 3 | a backslash-continued `gdbus` command did not survive `sh -c` | three activation gates failed on `sh: 1: call: not found` |
| 4 | `omnibridge pair` backgrounded with **no stdin** read EOF from its `[y/N]` prompt | it **declined** silently; two operator scans lost |
| 5 | `am start` cannot open `PairingCaptureActivity` — `exported="false"`, correctly | adb was refused and the harness ignored it |
| 6 | the image viewer runs as a D-Bus service, so a second QR did not replace the first | an operator scanned a **stale QR** |
| 7 | `wl-copy` never exits — it owns the selection | the run stalled, last line looking like a pass |
| 8 | `GA_EXEC_TIMEOUT` tightened with `${VAR:-180}` *after* the library applied its 900s default | a hang ran for fifteen minutes |
| 9 | the L15 journal check tailed files-capability lines unbounded | it matched a **previous** run's transfer |

### 1.3 Peer-gate closure §5 — ten

[`RELEASE-PEER-GATES-CLOSURE-V1.md` §5](../../certification/linux/RELEASE-PEER-GATES-CLOSURE-V1.md).
**Seven of these produced a false FAIL**, which is the mirror of a vacuous pass
and just as invalid:

| # | Defect |
| --- | --- |
| 10 | `am force-stop` before L15 **dropped the TLS session** |
| 11 | the same force-stops **snoozed Android's notification listener** |
| 12 | the notification source was `com.android.shell`, which the app picker cannot offer |
| 13 | the picker miss was **one failed check, not a stop**, so L16 ran anyway |
| 14 | `find 'No app chosen'` accepted **any** already-chosen app |
| 15 | the notification row's status has **three** spellings |
| 16 | `toggle-after 'Choose apps' "$FIXTURE_PKG"` matched the **search box**, whose text *is* the package name |
| 17 | the L14 clipboard sheet was **left open**, covering the Files tab |
| 18 | `find '^Files$' \| tail -1` picked a permission row — `find` prints one line, so the tail did nothing |
| 19 | `grep 'session established' \| tail -1` characterised the live session from an **ended** one |

### 1.4 Found while hardening — three

| # | Defect | Where |
| --- | --- | --- |
| 20 | **`grep -q` behind a pipe under `pipefail` loses matches** — 225 of 300 | [evidence closure §5](../../certification/security/SECURITY-EVIDENCE-CLOSURE-V1.md) |
| 21 | `sign-release.sh` rejected a signature made by its own signing **subkey** | [signing foundation §8.8](RELEASE-SIGNING-FOUNDATION-V1.md) |
| 22 | `verify-release.sh --fingerprint` rejected the **published primary** fingerprint | as above |

---

## 2. What the corpus has in common

Reading twenty-nine cases together, they are six shapes, not twenty-nine
problems:

| Shape | Cases |
| --- | --- |
| **the thing that measures is absent** — a tool, a binary, a writable directory | Pkg 1, 5 |
| **the capture is empty or too small to mean anything** | Pkg 2; L closure 1 |
| **the window does not correspond to the operation** | L closure 2, 9; peer 19 |
| **the state the operation needs was destroyed by the harness** | peer 10, 11, 17 |
| **a count, a glob or a delta that is not what it looks like** | Pkg 3, 6, 7; peer 14, 18 |
| **the search itself can be wrong** | #20, #21, #22 |

The last shape is the one nobody looks for, and it is the most dangerous:
everything else is a mistake about *the world*, while this is a mistake about
*the instrument*. A privacy gate written "absent is a pass" turns a lost match
into a pass on a real leak.

---

## 3. `lib/assert.sh` — ten primitives, one per shape

Each function carries, in its own comment, the occurrence that justifies it.
None is there because it seemed generally wise.

| Primitive | Refuses | From |
| --- | --- | --- |
| `need_tool` | a tool that is not installed | Pkg 1 |
| `need_nonempty` | an empty capture, or one below a stated minimum | Pkg 2; L16 |
| `contains` / `absent` | — *predicates* that cannot lose a match | #20 |
| `need_window_covers` | a capture with no anchor the product wrote about **this** run | L closure 2, 9; peer 19 |
| `need_exact_count` | a count that is merely non-zero | Pkg 7; L closure 1 |
| `need_glob` | a glob matching zero, or the wrong number | Pkg 3 |
| `need_stdin_answer` | a prompting command whose stdin is already closed | Pkg 4; L closure 4 |
| `need_writable` | a directory nothing can write to | Pkg 5 |
| `need_abs_path` | a relative mount source | Pkg 6 |
| `need_ran` | a counter that did not move across the operation | peer 10, 11 |
| `need_delta` | a change that is not the one claimed | peer 14 |

Two design choices, both load-bearing:

**Assertions are loud; predicates are silent.** `need_*` print why they failed,
because whoever reads the run needs to know what was missing. `contains` and
`absent` print nothing, because the caller composes them into its own message —
requiring a diagnostic from them would push duplicate text into every call
site. The self-tests check the two kinds differently, on purpose.

**Nothing exits.** Each returns non-zero, so the caller chooses `abort` or
`notok`. A library that exited would decide for a gate whether a missing
precondition is fatal, and sometimes it is not.

---

## 4. `harness-selftests.sh` — does the harness fail when it should?

**40 cases, 0 failures.** Every primitive is asserted in *both* directions:

```
ok  REJECTS a tool that is not installed — required tool(s) not installed: …
ok  ACCEPTS tools that are installed
ok  REJECTS an empty capture — the journal is empty; a search over it would prove nothing
ok  ACCEPTS a capture with real content
ok  REJECTS a window with no anchor for this operation
ok  ACCEPTS a window carrying this operation's own anchor
ok  REJECTS a count that is zero — installed packages is 0, expected exactly 2
ok  REJECTS a glob that matches nothing
ok  REJECTS a relative path used as a mount source
ok  REJECTS a directory the harness cannot write to
ok  REJECTS a counter that did not move across the operation
ok  REJECTS two arrivals counted as the one under test
ok  REJECTS a command whose stdin is closed
```

**The ACCEPTS half is not padding.** A primitive hard-coded to `return 1` would
pass every rejection case in this file. Fifteen acceptance cases stop that, and
CI gate **H3** requires both halves to stay present.

### 4.1 The regression that is a measurement

Defect #20 is re-measured on every run rather than described:

```
ok  REGRESSION: 200/200 searches of a large capture found a present sentinel
    (the pipe form missed 75%)
```

A 4000-line capture with the sentinel near the start — the exact shape that
failed three times in four under `printf | grep -q`. If the pipe form ever
returns, this goes red on the first run.

---

## 5. Adoption — the primitives are used where the defects happened

Not a wholesale rewrite. The call sites that suffered a recorded defect now use
the primitive that names it:

| Harness | Adopted |
| --- | --- |
| `lifecycle-gates.sh` | `need_tool` on the host tools; `need_abs_path` on `--pkgdir`; `need_glob` on the package set; `need_exact_count` on the installed-package count that Pkg-v1 and L-closure-1 both got wrong |
| `lifecycle-peer-gates.sh` | `need_tool` including **qrencode** — without it the operator stands in front of a screen with nothing on it; `need_delta` for L16's mirrored count; `need_window_covers` for L15's journal |
| `security-log-evidence.sh` | `need_tool`; `need_nonempty` on both journal captures; `need_window_covers` on the transfer anchor; `need_delta` on the mirrored count |

The harnesses were smoke-tested against a powered-off guest and abort loudly
with a named precondition rather than crashing or proceeding.

---

## 6. Three static guards, in CI on every packaging pull request

| Gate | Fails when | Verified against |
| --- | --- | --- |
| **H1** | any host-side `\| grep -q` in `packaging/tests/*.sh` | a planted `echo planted \| grep -q planted` — caught. It also caught a real one in the signing tests **before it was committed** |
| **H2** | a guest-driving harness does not load `lib/assert.sh` | the three that must |
| **H3** | the self-tests stop asserting in both directions | requires ≥10 rejection and ≥5 acceptance cases |

H1 exempts a `| grep -q` inside a quoted `gx`/`gu`/`ga_wait_for` command, with
the reason: those run under the guest's `/bin/sh -c`, which does not set
`pipefail`, so the pipeline status is grep's own.

`packaging-checks.sh` — **101 passed, 0 failed.**

---

## 7. AGENTS.md

The rule is now in the file every automated contributor reads first, stated
once and not duplicated: it did not previously appear there at all.

It carries the **symmetry**, which is the half that keeps being missed — *a
FAIL measured against a precondition the harness itself destroyed is equally
invalid* — and the six concrete checks, and the instruction to add a new false
green to `assert.sh` with its measurement rather than patching only the call
site that revealed it.

---

## 8. What this does not do

| | |
| --- | --- |
| **It does not retrofit every assertion** in every harness. The defect sites are adopted; the rest is unchanged, working code. A rewrite would be a larger change with more risk than the problem justifies |
| **It cannot prevent a class nobody has met.** Every primitive is retrospective by construction; that is the design, not a shortfall |
| **`need_stdin_answer` checks the mechanical half only.** That a prompt got *an* answer is checkable; that the answer was the intended one is not — the harness must still assert the outcome it wanted, which is what peer-gate closure's bounded `printf 'y\ny\ny\n'` plus a fingerprint assertion does |
| **Defects 3, 6, 7, 8, 12, 13, 15, 16** are one-site mistakes — a quoting error, a D-Bus viewer, a blocking `wl-copy`, a variable default, four Android UI labels. They are fixed where they were found and are recorded here; a primitive for each would be a framework guessing at a fifth wave |

---

## 9. Verdict

# HARNESS HARDENING COMPLETE — THE REFUSALS ARE TESTED, AND CI KEEPS THEM

Twenty-nine recorded false results are encoded as ten primitives that refuse
them, each naming the wave that suffered it. Forty self-test cases require
every primitive to reject its failure mode **and** to accept the good case, so
neither a primitive that passes everything nor one that fails everything can
survive. Three static guards run on every packaging pull request, and one of
them caught a real defect in this very phase's own new code before it was
committed.

**The single most valuable input, as lifecycle closure predicted, was the
capture-window flaw** — and running the harnesses generalised it twice: from
*the window must bracket the operation* to *the state the operation needs must
survive the harness*, and then to *the search itself must not be able to lose a
match*. All three are now primitives with measurements attached.
