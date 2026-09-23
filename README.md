# OmniBridge

**One bridge. Any device.**

Open-source, local-first device continuity.

> **Renamed.** This project was called **AnyFlow** (*One flow. Any device.*)
> until it was renamed to **OmniBridge** before the public v1.0.0 release.
> The rename went all the way down — binaries, application ids, ALPN, mDNS,
> the QR prefix, the protobuf namespace and the pairing-proof domain — with no
> compatibility aliases, because there was no released version to stay
> compatible with. **An existing AnyFlow build cannot talk to an OmniBridge
> build and a development pairing must be redone once.**
> See [ADR-0018](docs/adr/ADR-0018-rename-to-omnibridge.md) for the decision
> and [the migration note](docs/migrations/MIGRATION-ANYFLOW-TO-OMNIBRIDGE.md) for what
> to do about an existing checkout or test device.
>
> The repository has since been renamed too, and now lives at
> `github.com/yurisismotto/omnibridge`.
>
> The certification reports in this repository were written under the old
> name and keep their original wording; they are evidence, not documentation.
> Their AnyFlow naming — and the old repository URLs in the CI run and issue
> links they cite — is preserved deliberately.

No required cloud. No vendor lock-in. No telemetry by default.

Devices find each other on the local network, authenticate with pinned public
keys over TLS 1.3, and only after an explicit, human-confirmed pairing.

> **Status: v1.0.0.** The certified foundation — identity, discovery,
> pairing, authenticated transport, ping/pong — carries four capabilities:
> `battery.v1`, `files.v1`, `clipboard.v1` and `notifications.v1`. **Media
> control and browser integration are not implemented**; the architecture is
> built to receive them, and that is all.
>
> **`notifications.v1` is implemented and certified.** It is an optional,
> off-by-default Android notification mirror, requiring both the Android OS
> notification-access grant *and* a separate per-peer grant, with no history,
> no cloud and no telemetry. The decision is
> [ADR-0015](docs/adr/ADR-0015-notification-access.md), with
> [ADR-0016](docs/adr/ADR-0016-notification-identity.md) and
> [ADR-0017](docs/adr/ADR-0017-capability-roles.md); it is specified in
> [docs/research/notifications-v1/](docs/research/notifications-v1/) and
> certified in
> [NOTIFICATIONS-V1-N6-FINAL-CERTIFICATION.md](docs/certification/notifications/NOTIFICATIONS-V1-N6-FINAL-CERTIFICATION.md).
>
> Clipboard sharing is, precisely: **automatic desktop → Android sync**
> (opt-in, per device) and **manual Android → desktop send**. It is not
> "automatic bidirectional clipboard", and saying so would be wrong: Android
> 10+ refuses clipboard reads to an app without input focus, and OmniBridge uses
> none of the techniques that defeat that. See
> [docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md).

## Principles

1. Local-first. The LAN is the only transport.
2. No cloud service, no account, no telemetry.
3. Nothing in plaintext. TLS 1.3 only.
4. Identity is a public key — never an IP, hostname or MAC address.
5. Discovery is not trust. Reachability grants nothing.
6. Explicit pairing, confirmed by a human, with key pinning afterwards.
7. Every feature is a separately granted capability.
8. No root, no accessibility service, no ADB, no hidden permissions. A
   privileged Android capability is acquired only for a named, user-visible
   feature, through the platform's own API for it, with separately revocable
   consent — and never to defeat a restriction that protects the user
   ([ADR-0015](docs/adr/ADR-0015-notification-access.md)).
9. Logs never contain user content.

## Layout

```
omnibridge/
├── protocol/proto/            Wire format — compiled by BOTH implementations
├── desktop/                   Rust workspace
│   ├── proto/                 Generated protobuf types
│   ├── core/                  Identity, pairing, TLS, framing, session
│   ├── capabilities/battery/  battery.v1
│   ├── capabilities/files/    files.v1 — transfers, filename safety, stream auth
│   ├── capabilities/clipboard/ clipboard.v1 — text rules, policy, loop suppression
│   ├── capabilities/notifications/ notifications.v1 — mirror, roles, redaction
│   ├── daemon/                omnibridged
│   ├── cli/                   omnibridge
│   └── gui/                   omnibridge-gui — GTK4 / libadwaita
├── android/                   Kotlin + Compose app
├── browser-extension/         (placeholder)
├── packaging/common/          the systemd user unit and the cargo vendor config
├── packaging/fedora/          RPM spec, firewalld service
├── packaging/debian/          debhelper packaging for Debian and Ubuntu
└── docs/                      see docs/README.md for the full taxonomy
    ├── adr/                   ADR-0001 … ADR-0018
    ├── architecture/          OVERVIEW.md, PROTOCOL.md, FILES.md, CLIPBOARD.md, NOTIFICATIONS.md
    ├── design/                BRAND.md, UI-GUIDELINES.md, tokens.json, assets/
    ├── security/              THREAT_MODEL.md
    ├── research/              cross-platform expansion, notifications.v1
    ├── audits/                readiness and gap analyses, by area
    ├── certification/         PASS/FAIL gates and their evidence, by area
    ├── reports/               sprint and hardening reports, by area
    └── migrations/            AnyFlow → OmniBridge
```

Root Markdown is limited to this file and
[AGENTS.md](AGENTS.md); every other document lives under
[docs/](docs/README.md), which explains where a new one belongs.

## Running on Linux

### Which distributions, and what "supported" means for each

These are not all the same claim, and the difference is worth reading before
choosing one:

| Distribution | Status | What that means |
| --- | --- | --- |
| **Fedora 41+** (developed and certified on 44) | **Runtime certified** | Every capability has been exercised on real hardware in a real GNOME Wayland session, through six certification waves |
| **Ubuntu 24.04 LTS** | **Runtime certified** | The packages CI publishes, installed on a real desktop with a real GNOME Wayland session, and paired with a physical Android phone over the LAN |
| **Ubuntu 26.04 LTS** | **Runtime certified** | same |
| **Debian 13 trixie** | **Runtime certified**, with one documented exception | same, except that manual clipboard **sending** cannot work on its compositor — see [Known limitations](#known-limitations) |

These three moved from "build-supported" to "runtime certified" in
[lifecycle closure](docs/certification/linux/RELEASE-LIFECYCLE-CLOSURE-V1.md)
and [peer-gate closure](docs/certification/linux/RELEASE-PEER-GATES-CLOSURE-V1.md).
**23 of 26 lifecycle gates are certified on all three** — clean install, the
installed file manifest, autostart, the launcher, D-Bus activation into a live
session, the tray, mDNS, the listening port, restart, logout/login, reboot,
remove, reinstall, purge and the trust store surviving all of it — plus
Android discovery, file transfer and notification mirroring against a physical
SM-X620 on the same LAN. Two gates are N/A with a stated reason (there is one
published build per target, so there is nothing to upgrade *from*), and one is
partial on Debian 13 alone.

The first gate that ran found a defect that made the packages **unusable** on
both Ubuntu releases — `omnibridged.service` could not start at all — and it
was fixed before certification continued. That is the argument for doing this
rather than shipping on a green build matrix.

What is still **not** claimed, on any distribution: sending a file or a
clipboard **from** the phone to the desktop is untested end to end, because
driving it needs a human at the phone's document picker. The code paths are
exercised by the in-process suite; they have not been measured on hardware.

**Ubuntu 22.04 LTS and Debian 12 bookworm cannot run the GUI** and are not
targets: their libadwaita (1.2) and GTK (4.8) predate the APIs the interface is
built from. The daemon and CLI need no GTK at all.

### Prerequisites

Three things, and only the third is distribution-specific in any interesting
way:

* **A C compiler.** `ring` compiles C and assembly for the TLS primitives.
  Nothing else in the tree needs one — the daemon and the CLI link no C
  library at all.
* **A Rust toolchain, 1.88 or newer.** See the table below; this is the one
  place where a distribution's own package may not be enough.
* **GTK 4.12+ and libadwaita 1.5+**, for the GUI only. Every supported
  distribution ships enough (Ubuntu 24.04 is exactly at the libadwaita floor).

It does **not** need `protobuf-compiler` — the schema is compiled by `protox`,
in pure Rust ([ADR-0004](docs/adr/ADR-0004-protocol-buffers.md)) — and it does
**not** need OpenSSL, Avahi, libdbus or libX11 development packages. Transport
security is `rustls`/`ring`, D-Bus is `zbus` (a pure-Rust implementation),
mDNS is `mdns-sd` (its own responder, not an Avahi client) and the Xwayland
clipboard watch is `x11rb` with its own connection backend.

**Fedora**

```bash
sudo dnf install gcc pkgconf-pkg-config rust cargo
sudo dnf install gtk4-devel libadwaita-devel glib2-devel   # GUI only
sudo dnf install wl-clipboard                              # clipboard, at runtime
sudo dnf install upower                                    # battery.v1, optional
```

**Ubuntu 24.04 / 26.04 and Debian 13**

```bash
sudo apt install gcc libc6-dev pkg-config
sudo apt install libgtk-4-dev libadwaita-1-dev   # GUI only; this also brings
                                                 # in glib-compile-resources
sudo apt install wl-clipboard                    # clipboard, at runtime
sudo apt install upower                          # battery.v1, optional
```

The package names differ; the runtime binary OmniBridge actually looks for is
called `wl-copy` on all of them, and the package carrying it is called
`wl-clipboard` on all of them.

### The Rust toolchain, per distribution

**OmniBridge requires Rust ≥ 1.88.** That number is not a preference: the
committed `Cargo.lock` contains crates (`time`, `rcgen`, `zbus`) that declare
it, so an older toolchain fails in Cargo's resolver before compiling a line of
OmniBridge. The distribution's own `rustc` package is **not** required — it is
simply the most convenient source when it is new enough.

| Distribution | Its default `rustc` | Enough? | What to use |
| --- | --- | --- | --- |
| Fedora 44 | 1.98 | **yes** | `dnf install rust cargo` |
| Ubuntu 26.04 LTS | 1.93.1 | **yes** | `apt install rustc cargo` |
| Ubuntu 24.04 LTS | 1.75 | **no** | a newer versioned toolchain from Ubuntu's own archive — `apt install rustc-1.91 cargo-1.91` — or [rustup](https://rustup.rs). Note that `rustc-1.82` is also in the archive and is **not** enough |
| Debian 13 trixie | 1.85.1 | **no** | `trixie-backports` (`rustc` 1.94.1), or [rustup](https://rustup.rs) |

### Build and run

```bash
cd desktop
cargo build --release
cargo test --workspace          # 981 tests

./target/release/omnibridged   # foreground, or install the user unit
```

As a service. The unit is a **user** unit — the identity key lives 0600 in
your `$XDG_DATA_HOME` and the control socket in your `$XDG_RUNTIME_DIR`, so
nothing here wants root. It is distribution-neutral and lives in
`packaging/common/`, which is the one copy every package format installs:

```bash
install -Dm0644 packaging/common/omnibridged.service \
    ~/.config/systemd/user/omnibridged.service
systemctl --user enable --now omnibridged.service
```

Then:

```bash
omnibridge status              # identity, port, capabilities, live connections
omnibridge pair                # opens a pairing window and prints a QR code
omnibridge devices             # paired devices
omnibridge ping <device>       # round-trip over the live session
omnibridge unpair <device>     # revoke; takes effect immediately
```

File transfer is a separately granted capability and is **never** granted
automatically — writing a file to your disk is a side effect
([ADR-0008](docs/adr/ADR-0008-capability-architecture.md)):

```bash
omnibridge grant <device> files.v1     # allow file transfer with this device
omnibridge send <device> ~/photo.jpg   # offer a file; streams progress
omnibridge transfers                   # everything since the daemon started
omnibridge cancel <transfer-prefix>    # stop one mid-flight
omnibridge revoke <device> files.v1    # withdraw; stops transfers already running
```

Received files land in `<XDG downloads>/OmniBridge`. An existing name is never
overwritten — `photo.jpg` becomes `photo (1).jpg`. See
[docs/architecture/FILES.md](docs/architecture/FILES.md).

Clipboard sharing is likewise never granted automatically — a device that can
write your clipboard can also see what you paste next:

```bash
omnibridge grant <device> clipboard.v1        # allow clipboard sharing
omnibridge clipboard status                   # what works here, and per-device policy
omnibridge clipboard send <device>            # send the current clipboard, now
omnibridge clipboard send <device> --sensitive  # ask the phone to mark it sensitive
omnibridge clipboard apply <device>           # apply a clip that is waiting
omnibridge clipboard auto-send <device> on    # push every local copy to that device
omnibridge clipboard auto-receive <device> on # apply its clips as they arrive
```

Granting is one decision; automation is another. A freshly granted device can
send and receive **by hand**, and both automatic directions start **off** —
`auto-send` means everything you copy leaves this machine, and `auto-receive`
means that device can replace what you are about to paste. Clipboard content
is never written to disk and never logged, at any level. See
[docs/architecture/CLIPBOARD.md](docs/architecture/CLIPBOARD.md).

The daemon has no terminal, so it cannot prompt: it **declines** incoming
files and logs why. `omnibridged --accept-files-without-asking` is the documented
escape hatch for an unattended test rig.

`<device>` is a device id or a fingerprint prefix of at least 8 characters. An
ambiguous prefix is an error, never a guess.

The daemon never needs root.

## Running on Android

Needs JDK 21 and Android SDK platform 35. See
[android/README.md](android/README.md). Build with
`cd android && ./gradlew :app:assembleDebug`.

## Pairing

1. On the computer: `omnibridge pair`. A QR code appears; it is valid for 120
   seconds
   and works once.
2. On the phone: **Scan pairing code**.
3. The phone pins the computer's key *from the QR*, before opening a socket —
   so the first connection is already authenticated and there is no
   man-in-the-middle window.
4. The phone proves it holds the pairing code, bound to both identities and to
   a fresh nonce.
5. The computer shows the phone's fingerprint. **Check it matches the phone's
   screen**, then accept.
6. Both sides store the other's public key. The token is destroyed.

Afterwards the phone reconnects on its own using the stored identities. A
network change does not require re-pairing.

## Verifying a release download

Every OmniBridge release ships a `SHA256SUMS` covering all fourteen artifacts,
and a detached OpenPGP signature over that manifest. Checking the signature
first and the digests second is the only order that helps: checking digests
first is checking a download against itself.

**The key to trust.** One fingerprint, and it does not change when the
maintainer's email does:

```
OmniBridge Release Signing Key
primary  F545DC184E909192C3FB6F6E64963019E731BE07
```

The primary is **certify-only**; releases are signed by its `sign`-only subkey
`E8EDE4706F067739A8D3A8B74C48CB81694FD134`, which expires 2028-09-22. Verify
against the **primary** fingerprint above — that is the long-term anchor, and
the verifier resolves the subkey for you.

**Where the key is.** The armoured public key is
[`packaging/release/omnibridge-release-pubkey.asc`](packaging/release/omnibridge-release-pubkey.asc)
in this repository, which gives it a stable URL that does not change with the
release:

```
https://raw.githubusercontent.com/yurisismotto/omnibridge/main/packaging/release/omnibridge-release-pubkey.asc
```

It is also attached to every release page as `omnibridge-release-pubkey.asc`.
Both copies are the same key, and neither is a substitute for checking the
fingerprint printed above — a key fetched over HTTPS from a repository is
still a key somebody could have replaced, and the fingerprint is what makes
the check mean something.

```bash
# 1. import the published public key into a keyring of its own
curl -fsSLO https://raw.githubusercontent.com/yurisismotto/omnibridge/main/packaging/release/omnibridge-release-pubkey.asc
gpg --homedir ./ob-verify --import omnibridge-release-pubkey.asc
gpg --homedir ./ob-verify --export > ob-release.gpg

# 2. check the signature, then the files
./packaging/release/verify-release.sh \
    --dir <the downloaded release directory> \
    --keyring ob-release.gpg \
    --fingerprint F545DC184E909192C3FB6F6E64963019E731BE07
```

A run that succeeds prints `VERIFIED` and the number of files checked. Anything
else is a failure — in particular a **missing** `SHA256SUMS.asc` is a failure,
not a skip, because anyone who can substitute an artifact can also delete the
signature. `--allow-unsigned` exists, says exactly what it is not checking, and
reports its result as `CHECKED (UNSIGNED)`.

The releases also carry SLSA build provenance, which is a different claim and
not a substitute:

```bash
gh attestation verify <artifact> -R yurisismotto/omnibridge
```

Provenance answers *"was this built by OmniBridge's CI, from which commit?"*.
The signature answers *"does the maintainer stand behind this release?"*. A
green provenance check is not a maintainer signature.

The evidence behind this identity — custody, the signed set, independent
verification and the negative tests — is in
[docs/certification/release/RELEASE-SIGNING-CLOSURE-V1.md](docs/certification/release/RELEASE-SIGNING-CLOSURE-V1.md).

## Security

Read [docs/security/THREAT_MODEL.md](docs/security/THREAT_MODEL.md).

Short version: identity is a hardware-backed P-256 key (Android Keystore /
StrongBox on the phone; a 0600 file the daemon refuses to start without on the
desktop). Transport is TLS 1.3 with mutual authentication and SPKI pinning.
Pairing uses a 160-bit single-use token proved via HMAC bound to both
identities and a server nonce. Replay is stopped by strictly increasing
sequence numbers and message-id de-duplication — never by timestamps, because
clocks disagree.

There is no build flag, debug variant or test helper anywhere in this
repository that disables certificate validation.

## Testing

```bash
cd desktop && cargo test --workspace              # 981 tests
cd android && ./gradlew :app:testDebugUnitTest    # 771 tests

# Touches the real system clipboard, so it is opt-in:
cd desktop && cargo test -p omnibridge-capability-clipboard --test real_backend \
    -- --ignored --test-threads=1                 # 9 tests

# On a connected Android device:
cd android && ./gradlew :app:connectedDebugAndroidTest   # 102 tests
```

The Rust suite includes end-to-end pairing over real TLS on loopback and a
hostile-client suite that replays envelopes, duplicates message ids, rewinds
sequence numbers, claims another device's fingerprint and skips the handshake.

The Android suite covers the same wire rules on the Kotlin side, and checks
pinning against real certificates emitted by the desktop implementation
(`protocol/testdata/`).

Three known-answer vectors are asserted by **both** suites, so the
implementations cannot drift apart silently:

* the pairing proof and confirmation HMACs,
* the SPKI fingerprints of the shared certificate fixtures, and
* the `clipboard.v1` content hash — SHA-256 over the UTF-8 bytes — along with
  the text rules around it, since a clip one platform sends and the other
  refuses is a bug rather than a policy.

## Known limitations

* **Widening a capability grant takes effect on the next connection.** A
  session's effective capability set is fixed at handshake time, so after
  `omnibridge grant … files.v1` the phone must reconnect. *Narrowing* is
  immediate, including against a transfer already running — the asymmetry
  fails in the safe direction, but it is a rough edge.
* **No resume.** A transfer interrupted by a disconnect fails and its partial
  file is deleted. The receiver already knows the expected size and digest, so
  resume is tractable, but it needs durable partial state that this version
  deliberately does not keep.
* **One file per share.** `ACTION_SEND_MULTIPLE` is registered so OmniBridge
  appears for multi-select, but only the first item is sent.
* The trust store's persistence path is not covered by the local JVM unit
  tests: it needs a real `Context` and `filesDir`. Its pure logic is tested,
  and `ClipboardPersistenceTest` now covers the file I/O on a device.
* **Automatic Android → desktop clipboard is not implemented, and will not be.**
  Android 10+ refuses clipboard reads to an app without input focus, and every
  way around it is forbidden or user-hostile. Android → desktop is a deliberate
  action: the Send clipboard button, the Quick Settings tile, or sharing text
  to OmniBridge. Verified on an SM-X620 (Android 16): background read REFUSED,
  focused read ALLOWED, background `setPrimaryClip` APPLIED.
* **Sending the desktop clipboard needs a session that can read it, and some
  cannot.** Reading a selection this process does not own requires either the
  `wlr`/`ext-data-control` protocol or a reachable Xwayland. GNOME implements
  neither protocol, so it depends on the Xwayland fallback — which is present
  on Ubuntu 24.04 and 26.04, where sending works, and **not usable on Debian 13
  trixie**, where it does not. Both automatic *and* manual sending are affected,
  because both read the selection the same way. **Receiving a clipboard from the
  phone is unaffected** on every distribution: writing a clip needs no such
  protocol. `omnibridge clipboard status` reports `auto-send`, `manual send` and
  `receiving` separately, so the answer for your session is printed rather than
  guessed.
* **The desktop clipboard also needs an unlocked session.** On GNOME Wayland,
  `wl-copy` and `wl-paste` block behind the lock screen rather than failing.
  Every call is bounded by a timeout and reported as such, so nothing hangs —
  but clipboard sync does not work while the screen is locked. A lock is one
  cause of that timeout and the bullet above is another; the error names both
  rather than assuming.
* **Sensitive clips are refused where `wl-copy` cannot mark them.** A clip the
  phone marks as a password or other secret is written with `wl-copy
  --sensitive`, which tells clipboard managers to keep it out of their
  history. That option arrived in wl-clipboard 2.3.0, and Ubuntu 24.04,
  Ubuntu 26.04 and Debian 13 all ship 2.2.1 — so on those three, **OmniBridge
  refuses such a clip rather than writing it unmarked**, because an unmarked
  password silently persisted in a history file is the worse outcome. Ordinary
  clipboard sharing is unaffected. `omnibridge clipboard status` and the GUI's
  clipboard page both say so up front rather than at the moment a password
  fails to arrive. OmniBridge decides this by asking `wl-copy --help` for the
  option, never by reading its version — Fedora's `2.2.1^git…` has the flag
  and Debian's `2.2.1` does not, with the same version string.
* **Clipboard auto-send needs a compositor that can report clipboard changes.**
  GNOME implements neither wlr- nor ext-data-control, so OmniBridge watches via
  XFIXES on the Xwayland `CLIPBOARD` selection instead (ADR-0014). Without a
  reachable Xwayland there is no watcher — and, as the bullet above says,
  **no manual send either**, because both read the selection the same way.
  This bullet used to claim auto-send "degrades to manual sending"; it does
  not, and `omnibridge clipboard status` now reports the two separately
  instead of inferring one from the other.
* The desktop private key is protected by filesystem permissions, not by
  hardware. TPM2 sealing is the top security debt
  ([ADR-0006](docs/adr/ADR-0006-device-identity-and-pairing.md)).
* mDNS advertises a stable device id and name, which is a modest tracking
  signal on untrusted networks (threat model, T18). `--no-mdns` is the blunt
  workaround; a per-network toggle is the proper fix.

## License

Apache-2.0. See [LICENSE](LICENSE).
