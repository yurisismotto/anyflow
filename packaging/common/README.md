# Distribution-neutral packaging

One directory, one file today: `omnibridged.service`.

## Why it is not under `packaging/fedora/`

It used to be, and that was a real problem rather than an untidy one. The unit
is a security artefact — it is where `ProtectSystem=strict`, the syscall filter
and the address-family restriction live. A second copy under
`packaging/debian/` would have been the obvious way to package Debian, and the
day someone tightened one of those directives it would have landed on one
distribution and quietly missed the other.

So there is one file. The RPM's `%install` copies it into `%{_userunitdir}`.
The Debian packaging installs the same path. Neither owns it.

## What the unit does

It runs `omnibridged` as a **user** service. Not a system service, and the
distinction is load-bearing:

* the identity key is `0600` inside a `0700` directory in the user's
  `$XDG_DATA_HOME`, and the trust store beside it says which phones this
  *person* has paired;
* the control socket is `0600` in the user's `$XDG_RUNTIME_DIR`;
* the daemon binds TCP 55432 — above 1024, so no capability is needed.

Running it as root would gain nothing and would put a network-facing parser in
a place where a bug is worth far more to an attacker. Nothing in this
repository installs a system unit, and nothing should.

## The three directives that carry the most weight

```ini
RuntimeDirectory=omnibridge
RuntimeDirectoryMode=0700
ReadWritePaths=%h/.local/share
```

Each of them is there because of something that was measured, not assumed.
The measurements are in
[`../../docs/audits/packaging/PACKAGING-V1-SYSTEMD-UNIT.md`](../../docs/audits/packaging/PACKAGING-V1-SYSTEMD-UNIT.md)
and, before it, §4.2 of the readiness audit.

**`RuntimeDirectory=`** — `ProtectSystem=strict` *is* effective in a systemd
user unit on Fedora 44 / systemd 259; the general caveat in `systemd.exec(5)`
about namespacing and `PrivateUsers=` does not get the daemon off the hook.
Without this directive the daemon cannot `mkdir` in `$XDG_RUNTIME_DIR`, the
control socket has nowhere to live, and `omnibridge status`, the GUI and the
tray all have nothing to connect to. `0700` is the same mode the daemon
applies itself when it is run outside systemd, so the two paths agree.

**`ReadWritePaths=%h/.local/share`** — the *parent*, deliberately, not
`~/.local/share/omnibridge`. Two separate measured reasons:

1. `ReadWritePaths=` without a `-` prefix refuses to start a unit whose path
   does not exist — which is every fresh install; and
2. adding the `-` prefix fixes that and then does **not** punch the hole
   either, so the daemon's first run cannot create its own data directory
   inside a read-only `$HOME`.

Granting the parent is one directory wider than ideal. It stays entirely
inside `ProtectHome=read-only`, it is bounded and reviewable, and it is
**not** a weakening of `ProtectSystem=strict`, which is untouched.

**No `StateDirectory=`** — in a *user* unit `StateDirectory=` maps to
`$XDG_STATE_HOME`, i.e. `~/.local/state`, which the daemon never opens. The
old unit had it. It created an empty directory nobody read and did nothing for
the one that mattered.

## What the unit must never do

The data directory is **not package-owned and not unit-owned**. No
`StateDirectory=`, no `ExecStartPre=` that creates it, nothing in any
maintainer script that touches it. It holds the user's pairings; install,
upgrade, remove and purge all leave it exactly as they found it.
`packaging/tests/packaging-checks.sh` asserts both halves of that.

## Re-measuring it

```bash
./packaging/tests/packaging-checks.sh          # static: the directives are still there
./packaging/tests/systemd-unit-gates.sh        # live: gates S1, S2, S3 on this machine
```

The second one starts the real unit against a throwaway `XDG_DATA_HOME`,
measures the runtime directory and socket modes, reads the journal for sandbox
denials, and fails if the real trust store changed by so much as a mode bit.
It needs no root and refuses to run as root. Audit §14.3 runs it again on
Ubuntu 24.04, Ubuntu 26.04 and Debian 13.

## Enabling it

Packages ship it **disabled**, on every format. A global enable would turn on
a LAN listener for every account on the machine, and the daemon does nothing
until an identity exists and a device is paired, so autostart-before-pairing
buys the user nothing.

```bash
systemctl --user enable --now omnibridged.service
loginctl enable-linger $USER    # optional: keep it running while logged out
```

Lingering is deliberately opt-in. Leaving a network service running after
logout should be something the user chooses.

**An upgrade does not restart a running daemon.** A root scriptlet has no
route to a user's service manager. This is inherent to user units, not a
packaging defect; the new binary is in place and takes effect at the next
`systemctl --user restart omnibridged` or the next login.
