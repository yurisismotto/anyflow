# OmniBridge rebrand — remaining `anyflow` occurrence audit

**Branch:** `feature/omnibridge-rebrand-v1`
**Date:** 2026-09-21
**Scope:** every case-insensitive occurrence of `anyflow` left in the working
tree after the rebrand, classified.

**Third run — official artwork integrated.** The approved OmniBridge assets were
supplied and installed on 2026-09-21, and the twelve AnyFlow-era artwork files
plus the two Android brand drawables were deleted rather than relabelled. The
`BLOCKED_VISUAL_ASSET` category that dominated the previous two runs is
therefore **gone, not reduced**: there is no longer any active product artwork
carrying the old identity, and the count fell from 69 files to 57.

```console
$ git ls-files -co --exclude-standard -z \
  | while IFS= read -r -d '' f; do [ -f "$f" ] && printf '%s\0' "$f"; done \
  | xargs -0 env LC_ALL=C grep -ail anyflow | sort -u | wc -l
57
```

`LC_ALL=C` and `-a` matter: the two committed DER certificates contain NUL
bytes, and in a UTF-8 locale `grep` silently reports no match on them. A plain
`grep -ril anyflow` misses **both**, which is worth recording because a future
acceptance gate written the obvious way would miss them too.

**Zero of the 57 files is an unexplained active-product occurrence, and zero is
active artwork.** Each falls into one of the five allowed categories below,
listed with its per-file count (`grep -aoic anyflow`).

No secret, key, token, pairing proof or clipboard/notification content appears
in this document.

---

## Summary

| Category | Files |
|---|---|
| 1 — historical evidence (31 root reports + 2 sprint reports + 4 research docs + ADR-0011) | 38 |
| 2 — rename / migration documentation | 6 |
| 3 — intentional legacy documentation, in code comments | 2 |
| 4 — intentional negative / legacy tests | 6 |
| 5 — pending GitHub repository URL (3) + frozen DER vectors (2) | 5 |
| **UNEXPLAINED ACTIVE-PRODUCT OCCURRENCES** | **0** |
| **ACTIVE ANYFLOW ARTWORK** | **0** |
| **Total, unique files** | **57** |

---

## 1 — Historical evidence *(38 files)*

Reports of work genuinely performed under the AnyFlow name, quoting console
output from those runs. They keep their original wording: rewriting them to say
"OmniBridge" would falsify evidence for the sake of a clean `grep`.

* the **31** root-level certification reports (KDE Plasma, GNOME, notifications
  N0–N6, clipboard, Wave 0, the Linux-compatibility U0–U2 set, the UX and
  hardening sprints, the visual-identity report);
* `docs/sprints/wave-0-platform-abstraction.md` (76) and
  `docs/sprints/files-v1.md` (23);
* `docs/research/platform-expansion/` — `21-POC-MASTER-PLAN.md` (1),
  `25-IMPLEMENTATION-BACKLOG.md` (1), `28-WAVE-0-IMPLEMENTATION-SPEC.md` (2),
  `README.md` (1);
* `docs/adr/ADR-0011-project-naming-and-wire-identifiers.md` (28), kept because
  it records the earlier *Fedroid Bridge → AnyFlow* rename whose reasoning
  ADR-0018 reuses.

---

## 2 — Rename / migration documentation *(6 files)*

These exist **in order to** name the former identity. An occurrence here is the
point of the file.

| File | Count | Why |
|---|---|---|
| `docs/MIGRATION-ANYFLOW-TO-OMNIBRIDGE.md` | 44 | The migration note itself |
| `docs/adr/ADR-0018-rename-to-omnibridge.md` | 43 | The decision record and the full identifier table |
| `OMNIBRIDGE-REBRAND-REMAINDER-AUDIT.md` | 17 | This file |
| `OMNIBRIDGE-REBRAND-V1-VISUAL-CLOSURE.md` | 24 | The sprint report for the rebrand |
| `README.md` | 4 | The "Renamed." notice and the pending repository URL |
| `docs/design/BRAND.md` | 4 | Previous name; what the withdrawn blocker said; why the UI gradient tokens still differ; why the mark needs no small cut |

---

## 3 — Intentional legacy documentation, in code comments *(2 files)*

| File | Count | Why |
|---|---|---|
| `android/app/src/main/java/…/identity/DeviceIdentity.kt` | 5 | Why the Keystore alias namespace **restarts** at `omnibridge-identity-v1` instead of continuing `anyflow-identity-v2`, and why the AnyFlow-era legacy-alias sweep was removed rather than renamed |
| `desktop/gui/src/widgets.rs` | 1 | Why `brand_mark` needs no heavier small-size cut: the AnyFlow mark was *stroked* and thinned out below 24 px; the OmniBridge mark is filled and does not |

Neither is a string the product ever renders.

---

## 4 — Intentional negative / legacy tests *(6 files)*

Each names the dead identity in order to assert its **absence**. Deleting the
word would delete the guarantee.

| File | Count | Asserts |
|---|---|---|
| `desktop/core/tests/wire_identity.rs` | 2 | No pre-rename name survives in the ALPNs, mDNS type or QR prefix |
| `android/app/src/test/…/WireIdentityTest.kt` | 1 | The Kotlin mirror of the same contract |
| `desktop/proto/tests/namespace.rs` | 1 | The protobuf package of all six schemas |
| `desktop/gui/tests/brand_assets.rs` | 3 | `no_active_asset_carries_the_pre_rename_identity`, and that no icon path names a retired mark |
| `android/app/src/test/…/BrandingResourcesTest.kt` | 1 | `no launcher resource still draws a retired mark` |
| `android/app/src/androidTest/…/DeviceIdentityTest.kt` | 4 | That `anyflow-identity-v1/v2` are **not** present in this app's Keystore |

---

## 5 — Content that genuinely must remain *(5 files)*

### GitHub repository URL, pending the manual repository rename *(3)*

| File | Count |
|---|---|
| `desktop/Cargo.toml` | 1 |
| `packaging/fedora/omnibridge.spec` | 1 |
| `packaging/fedora/omnibridged.service` | 1 |

`github.com/yurisismotto/anyflow` is still the live URL. Changing these before
the repository is renamed would point them at a 404. The rename is a deliberate
manual post-merge step.

### Frozen cross-language test vectors *(2, one occurrence each)*

`protocol/testdata/identity-a.der` and `identity-b.der` carry
`CN=anyflow:<device-id>` in their certificate subjects. Their **SPKI
fingerprints are pinned as known-answer constants in both languages**, and their
purpose is to prove that the two implementations derive the same fingerprint
from the same bytes. The CN inside is incidental and is never parsed — trust is
SPKI-pinned.

Regenerating them would rotate two keypairs and churn four known-answer
constants across two languages, replacing verified cross-language evidence for
no behavioural gain. The code that *generates* new certificates was changed:
`desktop/core/src/identity.rs` and Android's `DeviceIdentity.kt` both emit
`CN=omnibridge:<device-id>`, so every identity created from this commit onward
carries the new name.

**Follow-up, if a reviewer prefers consistency over frozen evidence:**
`cargo run -p omnibridge-core --example gen_test_vectors`, then update the two
Rust constants and the two Kotlin ones together.

---

## Artwork — the category that closed

The previous two runs classified 14 files as **BLOCKED_VISUAL_ASSET**: twelve
SVGs under `docs/design/assets/` and two Android brand drawables, all still
carrying AnyFlow titles, `aria-label`s and gradient ids. All fourteen are now
**deleted**, and the approved OmniBridge artwork is installed in their place:

| Removed | Replaced by |
|---|---|
| `app-icon.svg` | `omnibridge-app-icon.svg` |
| `logo-flow-a.svg`, `-mono`, `-small` | `omnibridge-mark.svg` |
| `logo-flowing-a.svg`, `-mono` | `omnibridge-mark.svg` |
| `icon-flowing-ribbon.svg`, `-mono` | `omnibridge-mark.svg` / `omnibridge-mark-mono.svg` |
| `ribbon-connection.svg` | `omnibridge-mark.svg` |
| `logo-lockup.svg` | `omnibridge-logo-lockup.svg` |
| `wordmark.svg`, `wordmark-mono.svg` | `omnibridge-wordmark.svg` |
| `res/drawable/logo_flow_a.xml` | `res/drawable/logo_omnibridge_mark.xml` |
| `res/drawable/logo_flowing_ribbon.xml` | `res/drawable/logo_omnibridge_mark.xml` |

`docs/design/assets/` now contains six files, all installed byte-for-byte as
supplied. A scan of every one of them for `anyflow`, `flow a`, `flow-a`,
`flow_a`, `one flow` and `fedroid` returns nothing, and that scan is a test
(`no_active_asset_carries_the_pre_rename_identity`) rather than a one-off.

---

## What is provably clean

Every active product identifier was checked **by test rather than by grep**:

* control ALPN, data ALPN, mDNS service type and QR prefix — asserted as
  literals in `desktop/core/tests/wire_identity.rs` **and**
  `android/app/src/test/…/WireIdentityTest.kt`;
* the protobuf package and `java_package` of all six schemas — asserted over
  compiled descriptors in `desktop/proto/tests/namespace.rs`;
* the desktop application id, `.desktop` `Icon=`, D-Bus service `Name=`/`Exec=`,
  `StatusNotifierItem` `Id` and `IconName`, and the installer — cross-checked by
  `desktop/platform-linux/tests/tray_identity.rs` and `brand_assets.rs`;
* the Android `applicationId`, every manifest component and the notification
  listener component — read back out of the **built APK** with `aapt2`;
* the six HMAC domain separators — pinned indirectly by the recomputed
  cross-language known-answer vectors;
* **and now the artwork**: every platform derivative is asserted to carry the
  canonical outline of `docs/design/assets/omnibridge-mark.svg` byte for byte,
  on both front ends, so a redrawn or retraced icon fails the build.
