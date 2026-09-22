# AnyFlow → OmniBridge

**AnyFlow was renamed to OmniBridge before the public v1.0.0 release.**
Same product, same protocol design, same security model, new name:

| | Was | Is |
|---|---|---|
| Name | AnyFlow | **OmniBridge** |
| Tagline | One flow. Any device. | **One bridge. Any device.** |

There was no public v1, so the rename was taken as a clean break with no
compatibility aliases. The decision and the full identifier table are in
[ADR-0018](../adr/ADR-0018-rename-to-omnibridge.md).

This note is for **developers with an existing AnyFlow checkout or an AnyFlow
build installed on test hardware**. Nothing here runs automatically, and
nothing in the rebrand deletes anything you already have.

---

## The one-line version

An AnyFlow build and an OmniBridge build **cannot talk to each other**, so
both ends have to be rebuilt from this commit and paired again, once.

---

## Why they cannot talk

The rename went all the way down. Five of the changed identifiers are enough
on their own to stop a session:

| Identifier | AnyFlow | OmniBridge | How it fails |
|---|---|---|---|
| Control ALPN | `anyflow/1` | `omnibridge/1` | TLS handshake fails — no shared protocol |
| Data ALPN | `anyflow-data/1` | `omnibridge-data/1` | a file transfer's second connection is refused |
| mDNS service | `_anyflow._tcp.local.` | `_omnibridge._tcp.local.` | the daemon is simply not discovered |
| QR prefix | `anyflow1:` | `omnibridge1:` | a QR from the other build does not parse |
| Pairing proof domain | `anyflow/pairing-proof/v1` | `omnibridge/pairing-proof/v1` | the proof verifies nowhere |

This is deliberate. A mixed pair fails at the handshake — loudly, immediately
and before any data moves — rather than half-working.

---

## Android

| | Value |
|---|---|
| Old `applicationId` | `io.github.yurisismotto.anyflow` |
| New `applicationId` | **`io.github.yurisismotto.omnibridge`** |
| Old fixture | `io.github.yurisismotto.anyflow.fixture` |
| New fixture | **`io.github.yurisismotto.omnibridge.fixture`** |

Because the `applicationId` changed, **Android treats OmniBridge as a
different app.** It installs alongside AnyFlow rather than over it; both can
sit on the device at once, and each has its own storage, its own Keystore
entries and its own permission grants.

What you will have to do again, by hand:

1. **Uninstall the old app** if you do not want two of them:
   `adb uninstall io.github.yurisismotto.anyflow`
   (and `…anyflow.fixture` for the notification fixture). *Nothing in this
   sprint uninstalls anything for you.*
2. **Install OmniBridge** — `:app:assembleDebug`, then `adb install`.
3. **Grant notification-listener access again.** It is a per-component grant
   and the component moved: it is now
   `io.github.yurisismotto.omnibridge/io.github.yurisismotto.omnibridge.notifications.OmniBridgeNotificationListener`.
   On One UI, `settings put` is not enough — use
   `adb shell cmd notification allow_listener <component>` — and restart the
   app afterwards so it re-reads the grant.
4. **Grant the runtime permissions again** — notifications, camera for the QR
   scanner.
5. **Re-pair.** See below.

### Device identity and the Keystore

The device's ECDSA P-256 identity key is **not** migrated, and cannot be: it
is a non-exportable Android Keystore key, and it lives in the keystore of
`io.github.yurisismotto.anyflow`, which the new app cannot reach. OmniBridge
generates its own on first run under a fresh alias:

| | Alias |
|---|---|
| Old | `anyflow-identity-v2` (and the long-dead `anyflow-identity-v1`) |
| New | **`omnibridge-identity-v1`** |

Nothing exports, re-imports or destroys the old key. It stays in the old app's
keystore and goes away when that app is uninstalled — which is your call, not
the build's.

A new identity means a **new SPKI fingerprint**, so the desktop will see a
device it has never met. That is the correct outcome, not a bug.

The `notifications.v1` HMAC secret moves the same way, for the same reason:
`anyflow-notification-secret-v1` → `omnibridge-notification-secret-v1`, freshly
generated. Mirrored-notification ids are therefore in a new id space, which is
exactly what an identity reset is supposed to do.

### Received files

Received files now land in `Download/OmniBridge` instead of
`Download/AnyFlow`. Anything already in `Download/AnyFlow` stays there — it
survives uninstalling the old app, as it always did, and nothing moves or
deletes it.

---

## Linux / desktop

| | Old | New |
|---|---|---|
| State directory | `~/.local/share/anyflow` | **`~/.local/share/omnibridge`** |
| Control socket | `$XDG_RUNTIME_DIR/anyflow/control.sock` | **`$XDG_RUNTIME_DIR/omnibridge/control.sock`** |
| Fallback runtime dir | `/tmp/anyflow-<uid>` | **`/tmp/omnibridge-<uid>`** |
| Received files | `~/Downloads/AnyFlow` | **`~/Downloads/OmniBridge`** |
| CLI | `anyflow` | **`omnibridge`** |
| Daemon | `anyflowd` | **`omnibridged`** |
| GUI | `anyflow-gui` | **`omnibridge-gui`** |
| Application id | `io.github.yurisismotto.anyflow` | **`io.github.yurisismotto.omnibridge`** |

**OmniBridge writes only to OmniBridge paths.** Your old
`~/.local/share/anyflow/` — `identity.key`, `state.json` and the trust store
in it — is not read, not moved and not deleted. A fresh OmniBridge daemon
starts with an empty trust store and a new desktop identity, so the desktop's
own fingerprint changes too.

There is deliberately no migration subsystem. Writing one for unreleased
development state would be more machinery than the problem deserves.

### Stopping the old build

By hand, and only if it is running:

```console
$ pkill -x anyflowd            # or Ctrl-C in whatever started it
$ anyflow --help               # the old CLI is still on your PATH if you installed it
```

If you installed the old desktop metadata, remove it with the installer that
put it there, from an **AnyFlow-era checkout** — this branch's copy only knows
the new names:

```console
$ ./desktop/gui/tools/install-desktop-metadata.sh --uninstall
```

Left behind otherwise: `~/.local/share/applications/io.github.yurisismotto.anyflow.desktop`,
`~/.local/share/icons/hicolor/scalable/apps/io.github.yurisismotto.anyflow.svg`
and `~/.local/share/dbus-1/services/io.github.yurisismotto.anyflow.service`.
Harmless — they point at a binary that no longer exists — but they will keep
showing an AnyFlow launcher until removed.

When you delete the old state, do it yourself and deliberately:

```console
$ rm -rf ~/.local/share/anyflow        # only when you are sure
```

---

## Re-pairing

Ordinary pairing, with nothing special about it:

1. Build and start the new daemon — `omnibridged`.
2. `omnibridge pair` on the desktop, and **answer the confirmation prompt**;
   it needs a durable stdin and declines by default if nothing answers.
3. Scan the QR with the new Android app. The payload now begins
   `omnibridge1:`.
4. `omnibridge status` should show the peer `connected`, and the grants start
   empty again — `omnibridge grant <device> <capability>` for each one you
   need.

Expect **new fingerprints on both ends**. Any fingerprint written down during
an AnyFlow-era certification run is now historical.

---

## Visual identity, still open

The rebrand shipped **no new artwork**. No official OmniBridge logo or symbol
was supplied and none exists in the repository, so the AnyFlow mark family was
kept as a labelled placeholder rather than relabelled or redrawn. Today the
app icon, launcher icon, tray icon and wordmark are still AnyFlow's.

When the official source asset arrives, these are the derivatives it needs:

* **Android** — adaptive foreground, background (or the `ic_launcher_background`
  colour), monochrome/themed layer, and the launcher entries in
  `res/mipmap-anydpi-v26/`;
* **Linux** — a scalable hicolor SVG published as
  `io.github.yurisismotto.omnibridge.svg` (derived at build time from
  `docs/design/assets/app-icon.svg` by `desktop/gui/build.rs`), plus any raster
  sizes a packaging target asks for;
* **Both** — the wordmark and the mark + wordmark + tagline lockup, which
  currently still read "AnyFlow".

Keep the supplied file as the source of truth in `docs/design/assets/` and
derive everything else from it, as the existing build already does.

---

## Historical documents are not rewritten

Every certification report in this repository — the KDE Plasma, GNOME,
notifications, clipboard, Wave 0 and Linux-compatibility reports, and the
sprint reports under `docs/sprints/` — was written about work genuinely
performed under the AnyFlow name, and quotes console output from those runs.
They keep their original wording. Rewriting them to say "OmniBridge" would
falsify evidence for the sake of a clean `grep`.

`docs/adr/ADR-0011` is kept for the same reason: it records the earlier
*Fedroid Bridge → AnyFlow* rename, whose reasoning ADR-0018 reuses.

The complete, classified list of surviving `anyflow` occurrences is in
`OMNIBRIDGE-REBRAND-REMAINDER-AUDIT.md`.
