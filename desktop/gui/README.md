# `anyflow-gui` — desktop front end

GTK4 + libadwaita. A native Linux desktop application, deliberately not an
Electron app and not an embedded web view.

## What it is

A **client of the daemon**, and nothing more. It speaks the same
newline-delimited JSON control protocol the `anyflow` CLI speaks, over the
same Unix socket in `$XDG_RUNTIME_DIR/anyflow/control.sock`, using the very
same `Request`/`Response` types from `anyflow-daemon`. Sharing those types is
the point: if the socket contract changes, this crate stops compiling.

It adds no protocol, no capability and no privilege. Everything on screen is
something the daemon already reports, and every action is a request the CLI
can make too.

## Layout

```text
src/
  lib.rs      application, window, sidebar, refresh loop
  theme.rs    design tokens + the token/contrast tests
  client.rs   the control-socket client
  widgets.rs  the component vocabulary
  views/      one module per page
data/
  style.css   the structural stylesheet
  *.svg       brand artwork, compiled in via GResource
```

Pages re-render wholesale from daemon state on a two-second poll. The control
socket cannot push — see the note in `daemon/src/control.rs` about a future
D-Bus front end receiving signals instead. Until that exists, polling
metadata the daemon already holds in memory is the honest option, and
rebuilding is cheaper in bugs than a diffing layer at this data volume.

## Building

Needs the GTK4 and libadwaita development packages, and
`glib-compile-resources` on `PATH` (the build script runs it):

```bash
sudo dnf install gtk4-devel libadwaita-devel glib2-devel
cargo build -p anyflow-gui
```

If you cannot install system-wide, the devel packages can be unpacked into a
prefix and pointed at with `PKG_CONFIG_PATH` and `PATH` — `dnf download
--resolve` needs no root, and the runtime `.so` files are already present on
any GNOME system.

## Running

```bash
anyflowd &            # the daemon must be running
cargo run -p anyflow-gui
```

`--page <dashboard|files|clipboard|devices|peers|settings>` opens straight to
one page. It exists so the UI can be driven without a pointer — for a
screenshot pass, or simply to land where you meant to.

## Tests

```bash
cargo test -p anyflow-gui
```

The token tests read `docs/design/tokens.json` — the same file the Android
side's `DesignTokensTest` reads — and fail if this crate's palette drifts from
it. They also recompute every contrast ratio the brand documentation quotes,
including sampling the CTA gradient's interpolation, so the accessibility
claims are executable rather than asserted.

## Design

[`docs/design/BRAND.md`](../../docs/design/BRAND.md) and
[`docs/design/UI-GUIDELINES.md`](../../docs/design/UI-GUIDELINES.md).
