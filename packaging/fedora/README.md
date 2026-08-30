# Fedora packaging

## systemd user unit

`anyflowd.service` runs the daemon in the user's session:

```bash
systemctl --user enable --now anyflowd.service
journalctl --user -u anyflowd -f
```

It is a **user** unit, not a system unit, and it must stay that way. The
daemon's whole security posture assumes it runs as the user who owns the
identity: the key file is 0600 in the user's `$XDG_DATA_HOME`, and the control
socket lives in the user's `$XDG_RUNTIME_DIR`. Running it as root would
achieve nothing and would put a network-facing parser in the wrong place.

The unit applies the usual systemd hardening — `NoNewPrivileges`,
`ProtectSystem=strict`, a syscall filter, and `RestrictAddressFamilies` limited
to what a LAN daemon actually needs.

### Lingering

By default a user unit stops when the last session ends. To keep AnyFlow
available while logged out:

```bash
loginctl enable-linger $USER
```

That is a deliberate choice, not a default: leaving a network service running
after logout should be something the user opts into.

## RPM

`anyflow.spec` builds both binaries and installs the user unit. It
needs `rust`, `cargo` and `gcc`, and **not** `protobuf-compiler` — the build
compiles the schema with `protox` in pure Rust (ADR-0004).

`%check` runs the full test suite as part of the build, so a package that
fails its own security tests does not get built.
