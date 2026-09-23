# OmniBridge — Release Readiness v1

| Field | Value |
| --- | --- |
| **Branch** | `docs/release-rc-readiness-v1` |
| **Baseline** | `128fc10` — develop after PR #67 |
| **Date** | 2026-09-22 |
| **Mode** | **Read-mostly.** No feature was implemented. The only changes are this document and the completion of a documentation correction that the audit itself found — §3.4. |
| **Scope** | the thirteen conditions the release brief sets for Release Candidate readiness |
| **Verdict** | **RELEASE READINESS V1: BLOCKED — on D-2 alone, and D-2 is one decision away** |

---

## 0a. Closure — 2026-09-23

> **DATED CLOSURE NOTE — 2026-09-23, branch `feature/release-signing-provisioning-v1`.**
>
> **D-2 is now PASS, and this document's verdict of BLOCKED is superseded.**
> Everything below it stands unedited, because it was true when it was written
> and the reason it was true is worth keeping: on 2026-09-22 no production key
> existed, and this audit refused to reclassify the gate to make the wave look
> finished.
>
> What changed is not a reclassification. The production identity was
> provisioned on 2026-09-23, a real fourteen-artifact set built by CI from the
> immutable commit `14e958d` was signed with it, the signature was verified
> independently against a public-only keyring, and the negative tests pass
> against that real signed set. Those are the four conditions §3.6 and
> foundation §8.9 set, measured rather than argued.
>
> | Condition 6 — signing | then | now |
> | --- | --- | --- |
> | foundation implemented | ✅ | ✅ |
> | a release actually signed | ❌ | ✅ `SHA256SUMS.asc` over 14 artifacts, by subkey `E8EDE470…` of primary `F545DC18…` |
>
> The other twelve conditions were re-evaluated narrowly rather than re-run:
> nothing in this closure touched hardware, lifecycle or peer certification.
> **13 / 13 now hold.**
>
> Successor documents:
> [RELEASE-SIGNING-CLOSURE-V1.md](RELEASE-SIGNING-CLOSURE-V1.md) for the
> signing evidence, and
> [RELEASE-READINESS-V1-FINAL.md](RELEASE-READINESS-V1-FINAL.md) for the final
> verdict.
>
> **The historical fact is preserved deliberately: Release Readiness v1 was
> BLOCKED on D-2, and it was blocked for a day.**

---

## 0. Executive summary

Twelve of the thirteen RC conditions hold, and are measured rather than
asserted. One does not.

| | Condition | |
| --- | --- | :-: |
| 1 | lifecycle required gates resolved honestly | ✅ |
| 2 | peer matrix complete to the support claim | ✅ |
| 3 | SEC-LOG-03 real evidence | ✅ |
| 4 | L16 privacy evidence | ✅ |
| 5 | no unresolved High/Critical vulnerability | ✅ |
| 6 | **signing foundation implemented** | ✅ implemented — **but no release is signed** |
| 7 | release verification works | ✅ |
| 8 | immutable-source artifact CI valid | ✅ |
| 9 | SLSA provenance valid | ✅ |
| 10 | no required harness can PASS on absent measurement | ✅ |
| 11 | F-2 classified and resolved | ✅ **fixed**, not carried |
| 12 | physical Android pairing preserved | ✅ |
| 13 | support documentation matches measured reality | ✅ *after* §3.4 |

**The one that does not is D-2 — artifacts are unsigned.** Packaging v1 recorded
it as blocking a public release, this audit does not reclassify it, and the
maintainer's instruction on the signing decision was explicit: *"R5 must
explicitly preserve the production-signing gate as OPEN"* and *"do not weaken
the RC-readiness criteria merely because key provisioning is happening
separately."*

So the verdict is **BLOCKED**, and it would be dishonest to write anything
else. What is worth saying alongside it is how narrow the block is: every part
of signing that can exist without the key exists, is tested, and activates on
a key — §3.6.

---

## 1. Where the three Packaging v1 debts stand

| Debt | Blocks a release? | Then | Now |
| --- | :-: | --- | --- |
| **D-1** eleven lifecycle gates never ran | yes | 15 of 26 gates measured | **CLOSED** — 23 of 26 certified on all three distributions, 2 N/A with a stated reason, 1 partial on one distribution |
| **D-2** artifacts are unsigned | **yes** | nothing signed | **OPEN** — foundation built and tested, no key provisioned |
| **D-3** no runtime gate on Ubuntu/Debian | yes | build-supported only | **CLOSED** — all three runtime certified, on the packages CI publishes |

Two of three closed. The third is §3.6.

---

## 2. What this wave did, in order

| Phase | PR | Result |
| --- | --- | --- |
| **R1b** peer-gate closure | [#62](https://github.com/yurisismotto/omnibridge/pull/62) | L12/L14/L15/L16 on both Ubuntu guests, then Debian 13 re-run. **10 harness defects**, 7 of which had produced false FAILs |
| **R2** security evidence | [#63](https://github.com/yurisismotto/omnibridge/pull/63) | SEC-LOG-03's journal and logcat rows, L16's privacy half. Found the **`grep -q` pipefail defect** |
| **R3** signing foundation | [#64](https://github.com/yurisismotto/omnibridge/pull/64) | scripts, CI wiring, 28 negative tests. Stopped at the key |
| **R3b** signing identity | [#65](https://github.com/yurisismotto/omnibridge/pull/65) | the identity design, and **two defects the rehearsal found** |
| **F-2 fix** | [#66](https://github.com/yurisismotto/omnibridge/pull/66) | the product stopped claiming a send path that does not work |
| **R4** harness hardening | [#67](https://github.com/yurisismotto/omnibridge/pull/67) | 29 recorded false results → 10 tested primitives, 3 CI guards |

---

## 3. The thirteen conditions

### 3.1 Lifecycle gates — resolved honestly ✅

**23 of 26 certified on Ubuntu 24.04, Ubuntu 26.04 and Debian 13**, against
CI-published packages on real installed desktops with real graphical sessions.

| | |
| --- | --- |
| **2 N/A** | L13 and L17 — trust preserved across upgrade, and package upgrade. There is one published build per target, so there is nothing to upgrade *from*. Stated, not skipped |
| **1 PARTIAL** | L14's transfer half on **Debian 13 only**. Its compositor implements no data-control protocol and its Xwayland is unreachable, so no client can read a selection it does not own. Certified on both Ubuntu releases, where the Xwayland fallback works |

The word "honestly" is load-bearing, and the wave earned it twice. Lifecycle
closure's first gate found `omnibridged.service` could not start **at all** on
either Ubuntu — eleven blocked gates had been hiding a defect that made half
the supported distributions unusable. And peer-gate closure's first Ubuntu run
failed eight checks of which **none was the product**.

### 3.2 Peer matrix complete to the support claim ✅

| Gate | U24.04 | U26.04 | D13 |
| --- | :-: | :-: | :-: |
| **L12** Android discovery | ✅ | ✅ | ✅ |
| **L14** clipboard | ✅ | ✅ | ⚠ §3.1 |
| **L15** files | ✅ | ✅ | ✅ |
| **L16** notifications | ✅ | ✅ | ✅ |

Bound to identities, not to names: each guest by device id and fingerprint,
the phone as `SM-X620` / `509B D0C1 CE97 C909`, asserted `platform=android`
and asserted distinct from each guest's own fingerprint on every run.

**What is not claimed**, on any distribution: **phone → desktop** file and
clipboard transfers. Driving them needs a human at the phone's document picker
and a sentinel on the Android clipboard that adb cannot set — this build has no
`cmd clipboard`. Recorded as not executed, with the reason, rather than passed.

### 3.3 SEC-LOG-03 ✅ and 3.4 L16 privacy ✅

Measured on two distributions at `TRACE` — strictly more than `journalctl` ever
shows — with the window bounded by a journal **cursor** rather than a
timestamp, and proved to cover the operation by an anchor the **product** wrote
about **this** run.

| | |
| --- | --- |
| **SEC-LOG-03** | a real 48-byte file crosses to the phone. Journal anchor: the transfer id the daemon generated. logcat anchor: the filename, under the app's own `FileTransfer` tag. The content sentinel **and its 24-byte prefix** appear in neither |
| **L16 privacy** | the desktop's own `Notify` call is captured carrying both sentinels — so their absence from the journal means something — and the capability's one line shows the whole contract: peer by fingerprint, notification by **redacted id prefix**, `outcome="displayed"`, no title, no body |

### 3.5 No unresolved High/Critical vulnerability ✅

`cargo audit`: **0 vulnerabilities, 0 warnings**, 282 crates, 1264 advisories.
`rustls` is **0.23.45**, past RUSTSEC-2026-0285. The three `sec_tls_01_*` tests
hold, including the control that stops them passing on a server that refuses
everything. **1033 workspace tests pass, 0 fail.**

`cargo deny` is **not** run — not installed, no `deny.toml`. Debt **D-8**,
gated on a licence policy nobody has decided, and non-blocking in the baseline.

### 3.6 Signing — the one condition that does not hold ❌

**Implemented. Inert. Nothing is signed.**

| | |
| --- | --- |
| **built** | `sign-release.sh`, `verify-release.sh`, CI wiring that activates on a secret with no workflow edit, **38 negative tests** |
| **proved** | a verifier replaced by `exit 0` fails 11 cases; one byte flipped inside a real 4 MB `.deb` is rejected; the 14-artifact set signs and verifies in 0.18 s |
| **not done** | no production key exists |

The maintainer answered the required stop and changed two of its terms: the
private key must **never touch CI**, and the identity must not be bound to a
personal address. The design is
[RELEASE-SIGNING-FOUNDATION-V1.md §8](../../audits/release/RELEASE-SIGNING-FOUNDATION-V1.md).

**This gate stays OPEN until all four hold**, and no part of it may be counted
early:

1. the production signing identity is provisioned;
2. a **real** release artifact set is signed with it;
3. those signatures are verified independently of the signing step;
4. the negative verification tests pass against that real signed set.

Rehearsing the provisioning found **two defects in the scripts** on the exact
key structure the design recommends — signing aborted, and a user holding the
published primary fingerprint would have been told their release was
substituted. Both fixed, five checks added. **That is what the fourth condition
is for:** the tests passing against fixtures did not prove they would pass
against the real thing.

### 3.7 Release verification works ✅

`verify-release.sh` checks the **signature before the digests** — the other
order checks a download against itself — and treats a **missing signature as a
failure, not a skip**. Exercised against the real 14-artifact set and against
**eleven** ways a release can be wrong — a modified artifact, the wrong key, a
key the keyring does not hold, a manifest regenerated over tampered files, a
missing signature, a missing artifact, a truncated signature, an empty
signature file, a good signature by an unexpected fingerprint, and the last two
against a certify-only master with a signing subkey.

### 3.8 Immutable-source artifact CI ✅ and 3.9 SLSA provenance ✅

The full release pipeline ran green four times this wave. Provenance verified
on a real artifact today:

```console
$ gh attestation verify SHA256SUMS --repo yurisismotto/omnibridge
predicateType: https://slsa.dev/provenance/v1
subject      : SHA256SUMS, omnibridge-gui_…deb, omnibridge_…deb
workflow     : …/release-artifacts.yml
repo         : https://github.com/yurisismotto/omnibridge
issuer       : https://token.actions.githubusercontent.com
exit 0

$ gh attestation verify <one byte appended> --repo …
exit 1
```

Verified **and** negatively verified. And restated because R3 turns on it:
provenance is generated by the same platform that hosts the artifacts, so it
answers *"did this CI build these bytes from that commit?"* and **not** *"does
the maintainer stand behind this release?"*. It is not a substitute for §3.6.

### 3.10 No required harness can PASS on absent measurement ✅

29 recorded false results are encoded as 10 primitives that refuse them. 40
self-test cases require each to **reject** its failure mode *and* **accept** the
good case — without the second half, a primitive hard-coded to fail would pass
the suite. Three static guards run on every packaging pull request.

The honest caveat, from that phase's own §8: every primitive is retrospective,
eight of the 29 are one-site mistakes no primitive would have caught, and the
adoption is targeted at the defect sites rather than total.

**What earns this a ✅ rather than a hope** is that the guards have already
caught two live defects: H1 found a `git ls-files | grep -q` in the signing
tests before it was committed, and CI found a **false PASS inside the
self-tests themselves** — a case that passed because dash could not parse the
library, not because the primitive refused.

### 3.11 F-2 — classified, and fixed ✅

**Classification: a product defect in user-facing diagnostics.** Not a security
issue, no data loss, fails closed with exit 1.

Two statements were false, with the session provably unlocked on both sides of
the call:

* *"Clipboard auto-send cannot run; **manual send still works**"* — it does not;
* *"this **normally means the session is locked**"* — it was not, and Xwayland
  was running.

It was **structurally** wrong, not unlucky: both paths read a selection the
process does not own, so the claim was printed in exactly the branch where it
cannot be true. And it was said in **seven** places, not one — four in shipped
strings, three more in a doc comment, an operator message and the README.

**Fixed in [#66](https://github.com/yurisismotto/omnibridge/pull/66)**, not
carried as debt. `clipboard status` now prints `auto-send`, `manual send` and
`receiving` separately, so the answer is reported rather than asserted. A
characterisation test walks the shipped tree and fails on any non-comment line
claiming something "still works" — verified against a planted regression, and
carrying its own non-vacuity guard.

**And verified on the hardware where it was found.** A release build was made
from `develop` (CI run
[`35801876764`](https://github.com/yurisismotto/omnibridge/actions/runs/35801876764),
all seven jobs green), its Debian 13 packages digest-verified inside the guest,
installed over the certified build, and the daemon restarted into the live
session. **9 checks, 0 failures**, with `LockedHint=no` read before *and* after:

```console
# before — the build R1b certified
detail   … Clipboard auto-send cannot run; manual send still works.

# after — the same guest, the same compositor, the same unlocked session
detail        … This session cannot read a selection it does not own, so neither
                automatic nor manual sending can work here. Receiving a clipboard
                from a paired device is unaffected.
auto-send     NOT supported here — this session cannot detect clipboard changes
manual send   NOT supported here — sending reads the selection the same way
              auto-send watches it
receiving     supported — writing a clip needs no data-control protocol

$ omnibridge clipboard send <peer>
error: the clipboard did not respond in time: wl-copy and wl-paste are waiting
for a seat the compositor has not granted. A locked session is one cause; a
session where no client may read a selection it does not own is another. Run
`omnibridge clipboard status` to see which applies here.
rc=1
```

The behaviour is unchanged and correct — it still fails closed. What changed is
that the product now says what is true about it. **This also re-validated the
release pipeline on current `develop`**, which condition 8 needs anyway.

### 3.12 Physical Android pairing preserved ✅

```console
$ sha256sum ~/.local/share/omnibridge/{identity.key,state.json}
1914f6cd54f0d6479f3caedf4d0419b8dbd362a05e4ef612031b6d1153b4ca21  identity.key
a539ea91524065bba5cf774448c901b44d9a593eefb7ba70adf851f28d7fe6ce  state.json
```

Byte-identical to the values recorded before Packaging v1 Phase 1. The tablet
ends the wave paired with four desktops — the three guests and the host
`fedora` daemon at `149F 6B66 AB5D 8526`, listed with its grants exactly as at
the start. Its app data was never cleared.

One phone setting was changed and restored: the approved-listener list, read
before the rebind that unsnoozes the notification listener and asserted
**byte-identical** afterwards. It names two listeners belonging to other apps.

### 3.13 Support documentation matches measured reality ✅ — after a correction

**It did not when this audit started.** The README still described Ubuntu
24.04, Ubuntu 26.04 and Debian 13 as *"build-supported — not yet certified in a
real session"*, which stopped being true at lifecycle closure. The
documentation **understated** what had been measured, which is the same class
of error as overstating it.

Corrected here, because this condition is in R5's remit:

* the three are marked **runtime certified**, with what that means and what is
  still not claimed;
* Debian 13 is marked *certified with one documented exception* rather than
  silently equal to the other two;
* Known limitations now separates **auto-send, manual send and receiving**,
  because the first two fail together and the third does not;
* the README's own instance of the F-2 claim — *"auto-send degrades to manual
  sending"* — is gone.

Two historical documents carried it too. Per `AGENTS.md`, **ADR-0014** and the
**U0 compatibility audit** keep their original text with a dated superseding
note beside it; the decision and the gate verdict in each are unchanged.

---

## 4. Debts, and which of them block

| Debt | Blocks? | State |
| --- | :-: | --- |
| **D-2** artifacts unsigned | **YES** | §3.6 — the only blocker |
| D-4 filenames in logs | no | now known to have an **Android half** as well; a decision about two implementations |
| D-5 no man pages | no | |
| D-6 no AppStream screenshots | no | |
| D-7 no `-debuginfo` / `-dbgsym` | no | deliberate |
| D-8 `cargo deny` not configured | no | licence policy undecided |
| D-9 no Debian source package | no | |
| D-10 x86_64 only | no | scope |
| D-11 external IPv6 unscanned | no | |
| D-12 no SBOM publication path | no | |
| D-13 CI debug vs `%check` release | no | |
| D-14 byte-identical rebuilds unproven | no | aspiration |
| **F-2** | — | **closed**, §3.11 |
| phone → desktop transfers untested | no | needs a human at a picker; stated in the README |
| L14 on Debian 13 | no | a compositor limitation, documented |

---

## 5. Verdict

# RELEASE READINESS V1: BLOCKED

**Blocked on one item: D-2, artifacts are unsigned.** Packaging v1 recorded it
as blocking a public release; nothing in this wave reclassified it; and the
maintainer's answer to the signing stop was to keep the gate open until a real
release is signed and independently verified. Writing anything else would be
manufacturing a READY.

**Everything else is closed, and closed on evidence.** Twenty-three of
twenty-six lifecycle gates on three distributions against CI-published
packages. The four peer gates against a physical phone, bound to fingerprints.
SEC-LOG-03 and L16's privacy half at `TRACE`, with the windows proved to cover
the operations and the content proved to have arrived before its absence was
claimed. Zero advisories. Provenance verified and negatively verified.

**What the wave is really worth** is not the count of green gates. It is that
the first lifecycle gate to run found packages that could not start on half the
supported distributions; that the first Ubuntu peer run failed eight checks of
which none was the product; that a privacy gate's own matcher was losing three
matches in four; that rehearsing the signing procedure broke the scripts before
a real key existed; that the harness self-tests contained a false pass; and
that the product was telling users something false in seven places. **None of
that was found by adding gates. All of it was found by running them and
refusing to accept the first answer.**

**To reach READY FOR RC**, one thing: provision the signing identity per
[§8 of the signing foundation](../../audits/release/RELEASE-SIGNING-FOUNDATION-V1.md),
sign a real release, verify it independently, and run the negative tests
against that signed set. The scripts, the CI wiring and the tests are already
here and already proved against a real 14-artifact set.
