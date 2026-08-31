//! The platform boundary.
//!
//! Everything that knows about Wayland, X11 or an external helper lives under
//! this module. Above it, the capability deals in [`ClipboardText`] and a
//! change signal, and would work unchanged against a macOS or Windows
//! implementation.
//!
//! # Why the watch is a *signal*, not content
//!
//! [`ClipboardWatch`] yields `()`, and the manager then calls
//! [`ClipboardBackend::read_text`] itself. That looks like an extra step and
//! is load-bearing three times over:
//!
//! * the two available change sources on Linux report different things — an
//!   XFIXES `SelectionNotify` carries no data at all — so a content-carrying
//!   watch would have to fabricate one of them;
//! * clipboard content never flows through the notification plumbing, so
//!   there is one place that reads it and one place to audit;
//! * a burst of changes collapses naturally, because reading after the fact
//!   yields the clipboard's *current* state rather than a queue of stale
//!   ones.

pub mod wayland;
pub mod x11;

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::text::ClipboardText;

/// Why a clipboard operation did not happen.
///
/// Deliberately coarse. These strings reach the local operator through the
/// CLI and the daemon log, never a peer, and they never carry content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The platform cannot do this at all — no compositor, no helper binary,
    /// no supported protocol. Carries what is missing and, where there is
    /// one, how to fix it.
    Unavailable(String),
    /// The operation was attempted and failed.
    Failed(String),
    /// The operation did not finish within [`crate::limits::BACKEND_TIMEOUT`].
    ///
    /// Its own variant because on GNOME this is the *normal* signal that the
    /// session is locked, not an error worth alarming about: `wl-copy` and
    /// `wl-paste` need a seat and a serial that the compositor will not grant
    /// behind a lock screen, and they wait rather than fail.
    TimedOut,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "clipboard unavailable: {why}"),
            Self::Failed(why) => write!(f, "clipboard operation failed: {why}"),
            Self::TimedOut => f.write_str(
                "the clipboard did not respond in time. On GNOME Wayland this \
                 normally means the session is locked: wl-copy and wl-paste \
                 cannot obtain a seat behind the lock screen.",
            ),
        }
    }
}

impl std::error::Error for BackendError {}

pub type BackendResult<T> = std::result::Result<T, BackendError>;

/// A live subscription to clipboard changes.
///
/// Dropping it stops the watch and releases whatever the platform side was
/// holding — a child process, an X11 connection, a thread. The manager keeps
/// exactly one of these alive for as long as at least one peer has
/// `auto_send` on, and drops it otherwise, so a machine with no auto-send
/// peer runs no watcher at all.
pub struct ClipboardWatch {
    /// Fires once per clipboard change. Carries no content: see the module
    /// docs.
    pub changes: mpsc::Receiver<()>,
    /// What is doing the watching, for `anyflow clipboard status`.
    pub source: &'static str,
    /// Dropped last, stopping the platform side.
    _guard: Box<dyn Send + Sync>,
}

impl ClipboardWatch {
    pub fn new(
        changes: mpsc::Receiver<()>,
        source: &'static str,
        guard: Box<dyn Send + Sync>,
    ) -> Self {
        Self {
            changes,
            source,
            _guard: guard,
        }
    }
}

impl std::fmt::Debug for ClipboardWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardWatch")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// Read, write and watch the system clipboard.
///
/// Implementations must honour two rules that are not expressible in the
/// signature:
///
/// * **The CLIPBOARD selection, never PRIMARY.** X11 and Wayland both have a
///   second, implicit selection that is filled by merely *selecting* text
///   with the mouse. Synchronising it would transmit text the user never
///   asked to copy, which is why every call below names the ordinary
///   clipboard explicitly and no code path touches the primary selection.
/// * **Bounded.** Every operation must complete or fail within
///   [`crate::limits::BACKEND_TIMEOUT`]; none may block indefinitely.
#[async_trait::async_trait]
pub trait ClipboardBackend: Send + Sync {
    /// Short, stable name for diagnostics, e.g. `"wl-clipboard"`.
    fn id(&self) -> &'static str;

    /// The current clipboard text, or `None` when the clipboard is empty or
    /// holds something that is not text.
    ///
    /// A clipboard holding an image is not an error: `clipboard.v1` is text
    /// only and a non-text clipboard is simply nothing to send.
    async fn read_text(&self) -> BackendResult<Option<ClipboardText>>;

    /// Replaces the clipboard contents with `text`.
    ///
    /// `sensitive` asks the platform to mark the clip as such where it can —
    /// on Wayland, `wl-copy --sensitive`, which desktop clipboard managers
    /// use to skip storing an entry in history. It is a hint to the desktop,
    /// exactly as `EXTRA_IS_SENSITIVE` is a hint on Android: it is not
    /// enforcement and nothing may depend on it.
    async fn write_text(&self, text: &ClipboardText, sensitive: bool) -> BackendResult<()>;

    /// Starts watching for changes.
    ///
    /// `Err(Unavailable)` is a normal answer, not a failure: it means this
    /// platform cannot report clipboard changes to an ordinary application,
    /// and the caller must degrade to manual sending rather than poll. No
    /// implementation may satisfy this by polling.
    fn watch_changes(&self) -> BackendResult<ClipboardWatch>;

    /// Whether [`watch_changes`] can succeed on this session.
    ///
    /// A pure predicate answered from what was probed at startup: it must not
    /// perform I/O and must not start a watcher. `anyflow clipboard status`
    /// needs to tell the user whether `auto_send` will work *before* they
    /// turn it on, and finding out by starting a helper process would be a
    /// side effect in a status command.
    ///
    /// [`watch_changes`]: Self::watch_changes
    fn watch_availability(&self) -> std::result::Result<(), String>;

    /// One line describing what this backend can actually do here, for
    /// `anyflow clipboard status`. Must not perform I/O.
    fn describe(&self) -> String {
        self.id().to_string()
    }
}

/// Picks the best backend for the running session.
///
/// Order is by what the session *is*, not by what happens to be installed:
/// a Wayland session gets the Wayland backend even if an Xwayland display is
/// also present, because the Wayland clipboard is the real one there and the
/// X11 view of it is a bridge.
pub fn detect() -> Arc<dyn ClipboardBackend> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland {
        return Arc::new(wayland::WaylandBackend::detect());
    }

    // No Wayland display. A pure X11 session is not this Sprint's target and
    // has no read/write implementation yet, so say so precisely rather than
    // returning something that fails at the first call with a worse message.
    Arc::new(Unsupported {
        why: if std::env::var_os("DISPLAY").is_some() {
            "this is an X11 session; clipboard.v1 currently implements the \
             Wayland backend only (the X11 watch exists, read/write does not)"
                .to_string()
        } else {
            "no graphical session found (neither WAYLAND_DISPLAY nor DISPLAY \
             is set), so there is no clipboard to share"
                .to_string()
        },
    })
}

/// The backend for a session that has no clipboard we can use.
///
/// It exists so the capability can still be registered, still answer peers
/// with an honest `FAILED`, and still report *why* in `anyflow clipboard
/// status` — rather than the daemon refusing to start or the capability
/// silently vanishing from the advertised set.
pub struct Unsupported {
    why: String,
}

impl Unsupported {
    pub fn new(why: impl Into<String>) -> Self {
        Self { why: why.into() }
    }
}

#[async_trait::async_trait]
impl ClipboardBackend for Unsupported {
    fn id(&self) -> &'static str {
        "unsupported"
    }

    async fn read_text(&self) -> BackendResult<Option<ClipboardText>> {
        Err(BackendError::Unavailable(self.why.clone()))
    }

    async fn write_text(&self, _text: &ClipboardText, _sensitive: bool) -> BackendResult<()> {
        Err(BackendError::Unavailable(self.why.clone()))
    }

    fn watch_changes(&self) -> BackendResult<ClipboardWatch> {
        Err(BackendError::Unavailable(self.why.clone()))
    }

    fn watch_availability(&self) -> std::result::Result<(), String> {
        Err(self.why.clone())
    }

    fn describe(&self) -> String {
        format!("unsupported ({})", self.why)
    }
}

// ---------------------------------------------------------------------------
// Test backend
// ---------------------------------------------------------------------------

/// An in-memory clipboard, for tests.
///
/// Behaves like a real one in the ways that matter to the logic above it: a
/// write is observable by a read, and a write fires the watch. It is in the
/// library rather than a test module because the daemon's integration tests
/// need it too, and a second copy would be a second set of semantics.
pub struct MemoryBackend {
    inner: std::sync::Mutex<MemoryState>,
}

#[derive(Default)]
struct MemoryState {
    text: Option<ClipboardText>,
    /// Every write, for assertions. Holds content: this type is for tests.
    writes: Vec<(String, bool)>,
    watchers: Vec<mpsc::Sender<()>>,
    /// When set, every operation fails with this. Lets a test drive the
    /// "backend went away" path without a real compositor.
    failure: Option<BackendError>,
    watch_supported: bool,
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(MemoryState {
                watch_supported: true,
                ..MemoryState::default()
            }),
        }
    }

    /// Simulates a local copy by a human: sets the text and fires the watch.
    pub fn user_copies(&self, text: &str) {
        let validated = ClipboardText::validate(text).expect("test text must be valid");
        let senders = {
            let mut state = self.lock();
            state.text = Some(validated);
            state.watchers.clone()
        };
        for tx in senders {
            let _ = tx.try_send(());
        }
    }

    /// The clipboard's current text, if any.
    pub fn current(&self) -> Option<String> {
        self.lock().text.as_ref().map(|t| t.as_str().to_string())
    }

    /// Every `write_text` call, in order, as `(text, sensitive)`.
    pub fn writes(&self) -> Vec<(String, bool)> {
        self.lock().writes.clone()
    }

    /// Makes every subsequent operation fail.
    pub fn set_failure(&self, failure: Option<BackendError>) {
        self.lock().failure = failure;
    }

    /// Makes `watch_changes` report the platform as unable to watch.
    pub fn set_watch_supported(&self, supported: bool) {
        self.lock().watch_supported = supported;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
        // A poisoned mutex here means a test panicked while holding it; the
        // useful failure is that panic, not a second one from this line.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait::async_trait]
impl ClipboardBackend for MemoryBackend {
    fn id(&self) -> &'static str {
        "memory"
    }

    async fn read_text(&self) -> BackendResult<Option<ClipboardText>> {
        let state = self.lock();
        if let Some(f) = &state.failure {
            return Err(f.clone());
        }
        Ok(state.text.clone())
    }

    async fn write_text(&self, text: &ClipboardText, sensitive: bool) -> BackendResult<()> {
        let senders = {
            let mut state = self.lock();
            if let Some(f) = &state.failure {
                return Err(f.clone());
            }
            state.text = Some(text.clone());
            state.writes.push((text.as_str().to_string(), sensitive));
            state.watchers.clone()
        };
        // A real clipboard cannot tell a local write from any other change,
        // and neither does this one: the watch fires. That is exactly the
        // event loop suppression has to swallow.
        for tx in senders {
            let _ = tx.try_send(());
        }
        Ok(())
    }

    fn watch_changes(&self) -> BackendResult<ClipboardWatch> {
        let mut state = self.lock();
        if !state.watch_supported {
            return Err(BackendError::Unavailable("watching is disabled".into()));
        }
        if let Some(f) = &state.failure {
            return Err(f.clone());
        }
        let (tx, rx) = mpsc::channel(64);
        state.watchers.push(tx);
        Ok(ClipboardWatch::new(rx, "memory", Box::new(())))
    }

    fn watch_availability(&self) -> std::result::Result<(), String> {
        if self.lock().watch_supported {
            Ok(())
        } else {
            Err("watching is disabled".into())
        }
    }
}
