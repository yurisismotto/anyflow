# Desktop GUI — placeholder

Planned: GTK4 + Libadwaita, talking to the daemon over the same interface the
CLI uses.

Nothing here yet, deliberately. The daemon and `fedroid` cover this Sprint's
acceptance criteria, and a GUI built before the protocol settled would be
rework.

When it arrives it will almost certainly want a D-Bus interface rather than
the JSON control socket, so that it can receive signals instead of polling.
The control protocol in `desktop/daemon/src/control.rs` is deliberately thin
so a D-Bus front end can be added beside it rather than replacing it. See
[ADR-0003](../../docs/adr/ADR-0003-rust-desktop-daemon.md).
