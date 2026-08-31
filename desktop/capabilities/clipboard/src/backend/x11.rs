//! Clipboard-change notification via XFIXES.
//!
//! # Why this exists on a Wayland sprint
//!
//! GNOME is the target desktop, and Mutter implements neither
//! `zwlr_data_control_manager_v1` nor `ext_data_control_manager_v1`. Without
//! one of those, `wl-paste --watch` cannot run at all — it exits immediately
//! with *"Watch mode requires a compositor that supports the data-control
//! protocol"*. So on the exact desktop this Sprint certifies, the native
//! Wayland answer to "tell me when the clipboard changes" does not exist.
//!
//! What does exist is Xwayland. Mutter mirrors the Wayland clipboard onto the
//! X11 `CLIPBOARD` selection so that X11 applications can paste, and taking
//! ownership of an X11 selection generates an XFIXES `SelectionNotify` to
//! every client that asked for one. That notification is a supported, public
//! X11 mechanism, it is delivered for changes made by *Wayland-native*
//! applications, and it costs one idle socket.
//!
//! This is a notification source, not an X11 clipboard implementation. It
//! never reads or writes selection data: reads and writes go through
//! `wl-copy`/`wl-paste` against the real Wayland clipboard. The same code is
//! what a future native X11 backend would build its watch on, which is the
//! other reason it is shaped as its own module.
//!
//! # What is deliberately not watched
//!
//! Only the `CLIPBOARD` atom. `PRIMARY` — the selection that fills merely by
//! dragging the mouse over text — is never selected for, because
//! synchronising it would transmit text the user never asked to copy.
//!
//! See ADR-0014.

use std::sync::Arc;

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{self, ConnectionExt as _, CreateWindowAux, EventMask, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use super::{BackendError, BackendResult, ClipboardWatch};

/// Minimum XFIXES version that has selection notifications. They arrived in
/// XFIXES 1.0; asking for 5.0 keeps us on the version every modern server
/// offers, and the reply tells us what we actually got.
const XFIXES_MAJOR: u32 = 5;
const XFIXES_MINOR: u32 = 0;

/// Checks that an XFIXES clipboard watch can be established here.
///
/// Called once, at backend detection, so that `anyflow clipboard status` can
/// state the truth before anything is attempted. Everything it opens is
/// closed again.
pub fn probe() -> Result<(), String> {
    if std::env::var_os("DISPLAY").is_none() {
        return Err(
            "DISPLAY is not set, so there is no Xwayland server to observe \
             the clipboard through"
                .to_string(),
        );
    }

    let (conn, _screen) =
        x11rb::connect(None).map_err(|e| format!("cannot connect to the X display: {e}"))?;

    let version = conn
        .xfixes_query_version(XFIXES_MAJOR, XFIXES_MINOR)
        .map_err(|e| format!("XFIXES query failed: {e}"))?
        .reply()
        .map_err(|_| "the X server does not provide the XFIXES extension".to_string())?;

    if version.major_version < 1 {
        return Err(format!(
            "XFIXES {}.{} is too old for selection notifications",
            version.major_version, version.minor_version
        ));
    }

    Ok(())
}

/// Starts an XFIXES watch on the `CLIPBOARD` selection.
///
/// The X11 API is synchronous and blocking, so the watch owns a dedicated OS
/// thread rather than a tokio task: parking a runtime worker in
/// `wait_for_event` would starve whatever else that worker was scheduled to
/// run.
pub fn watch_clipboard() -> BackendResult<ClipboardWatch> {
    let (conn, screen_num) = x11rb::connect(None)
        .map_err(|e| BackendError::Unavailable(format!("cannot connect to the X display: {e}")))?;
    let conn = Arc::new(conn);

    let screen = conn
        .setup()
        .roots
        .get(screen_num)
        .ok_or_else(|| BackendError::Failed("the X display has no usable screen".into()))?;
    let root = screen.root;
    let root_visual = screen.root_visual;

    conn.xfixes_query_version(XFIXES_MAJOR, XFIXES_MINOR)
        .map_err(|e| BackendError::Failed(format!("XFIXES query failed: {e}")))?
        .reply()
        .map_err(|_| BackendError::Unavailable("the X server has no XFIXES extension".into()))?;

    let clipboard = conn
        .intern_atom(false, b"CLIPBOARD")
        .map_err(|e| BackendError::Failed(format!("InternAtom failed: {e}")))?
        .reply()
        .map_err(|e| BackendError::Failed(format!("InternAtom failed: {e}")))?
        .atom;

    // An unmapped 1x1 window. It is never drawn and never takes focus; it
    // exists only as the destination XFIXES delivers events to.
    let window = conn
        .generate_id()
        .map_err(|e| BackendError::Failed(format!("could not allocate an X id: {e}")))?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        root_visual,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(|e| BackendError::Failed(format!("could not create the watch window: {e}")))?;

    conn.xfixes_select_selection_input(
        window,
        clipboard,
        // SET_SELECTION_OWNER is the one that fires on every copy. The other
        // two catch an owner that goes away, which is also a clipboard
        // change worth noticing.
        SelectionEventMask::SET_SELECTION_OWNER
            | SelectionEventMask::SELECTION_WINDOW_DESTROY
            | SelectionEventMask::SELECTION_CLIENT_CLOSE,
    )
    .map_err(|e| BackendError::Failed(format!("could not select selection input: {e}")))?;

    conn.flush()
        .map_err(|e| BackendError::Failed(format!("could not flush the X connection: {e}")))?;

    let (tx, rx) = tokio::sync::mpsc::channel(64);

    // A private atom so the stop message cannot be confused with anything a
    // real application sends.
    let stop_atom = conn
        .intern_atom(false, b"_ANYFLOW_CLIPBOARD_WATCH_STOP")
        .map_err(|e| BackendError::Failed(format!("InternAtom failed: {e}")))?
        .reply()
        .map_err(|e| BackendError::Failed(format!("InternAtom failed: {e}")))?
        .atom;

    let thread_conn = Arc::clone(&conn);
    let handle = std::thread::Builder::new()
        .name("anyflow-clipboard-x11".to_string())
        .spawn(move || {
            loop {
                let event = match thread_conn.wait_for_event() {
                    Ok(event) => event,
                    Err(e) => {
                        // The X server went away — Xwayland restarted, or the
                        // session ended. Returning ends the watch; the
                        // supervisor above restarts it with backoff.
                        tracing::debug!(error = %e, "X clipboard watch connection ended");
                        break;
                    }
                };

                match event {
                    Event::XfixesSelectionNotify(notify) if notify.selection == clipboard => {
                        // Blocking send would park this thread on a full
                        // queue while the manager is busy; a dropped signal
                        // costs nothing because the manager reads the
                        // clipboard's current state, not a backlog.
                        if tx.try_send(()).is_err() && tx.is_closed() {
                            break;
                        }
                    }
                    Event::ClientMessage(msg) if msg.type_ == stop_atom => break,
                    _ => {}
                }
            }
        })
        .map_err(|e| BackendError::Failed(format!("could not start the watch thread: {e}")))?;

    Ok(ClipboardWatch::new(
        rx,
        "XFIXES (Xwayland CLIPBOARD)",
        Box::new(X11WatchGuard {
            conn,
            window,
            stop_atom,
            handle: Some(handle),
        }),
    ))
}

/// Stops the watch thread and tears down its X resources.
///
/// The thread is blocked in `wait_for_event`, which has no timeout and no
/// cancellation. Waking it needs an actual X event, so the guard sends the
/// window a `ClientMessage` from this side of the connection: with an empty
/// event mask, X delivers it to the client that created the destination
/// window, which is us. That is the standard idiom for interrupting an X
/// event loop, and it is why the guard holds a handle to the connection.
struct X11WatchGuard {
    conn: Arc<RustConnection>,
    window: xproto::Window,
    stop_atom: xproto::Atom,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for X11WatchGuard {
    fn drop(&mut self) {
        let event =
            xproto::ClientMessageEvent::new(32, self.window, self.stop_atom, [0u32, 0, 0, 0, 0]);
        let _ = self
            .conn
            .send_event(false, self.window, EventMask::NO_EVENT, event);
        let _ = self.conn.flush();

        if let Some(handle) = self.handle.take() {
            // If the thread has already exited — the connection dropped, say
            // — this returns at once. If it is wedged in a way the stop
            // message cannot reach, joining would hang the daemon's
            // shutdown, so the destroy below happens first and closing the
            // connection is what finally releases it.
            let _ = self.conn.destroy_window(self.window);
            let _ = self.conn.flush();
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe must answer, one way or the other, without panicking — on a
    /// developer's desktop, in a container, and in CI where there is no X
    /// server at all.
    #[test]
    fn probe_reports_rather_than_panics() {
        match probe() {
            Ok(()) => {
                assert!(
                    std::env::var_os("DISPLAY").is_some(),
                    "a successful probe implies a display"
                );
            }
            Err(why) => assert!(!why.is_empty(), "a failure must say why"),
        }
    }

    /// Without a display the answer must be the specific, actionable one
    /// rather than a connection error further down.
    #[test]
    fn a_missing_display_is_reported_precisely() {
        // Not `set_var`: mutating the process environment is unsound in a
        // threaded test binary. The condition is checked instead.
        if std::env::var_os("DISPLAY").is_none() {
            let why = probe().expect_err("no DISPLAY means no watch");
            assert!(why.contains("DISPLAY"), "unexpected reason: {why}");
        }
    }
}
