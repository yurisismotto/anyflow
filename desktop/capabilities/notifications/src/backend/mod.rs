//! The platform boundary.
//!
//! Everything that knows about D-Bus, freedesktop, GNOME or logind lives
//! under this module. Above it, the capability deals in [`Mirror`],
//! [`SinkError`] and a close signal, and would work unchanged against a
//! Windows toast adapter or a `UNUserNotificationCenter` one.
//!
//! # Why the seam is here and not in `anyflow-core`
//!
//! Wave 0 declined to create a `NotificationSink` before anything implemented
//! one, on the grounds that an abstraction with nothing on either side of it
//! is a guess about a shape. N2 is the first implementation, so N2 is where
//! the seam belongs — and the shape below is what writing the D-Bus sink
//! actually taught, not what was sketched in advance. Two things changed from
//! the sketch in `docs/architecture/NOTIFICATIONS.md`:
//!
//! * `display` takes `replaces: Option<ServerId>` and returns a `ServerId`,
//!   because the server is permitted to answer with an id that is **not** the
//!   one it was given. The specification only promises the id is preserved
//!   when `replaces_id` is non-zero, and a caller that assumed it would be
//!   preserved would silently lose track of the notification on any server
//!   that behaves differently. So the returned id is always stored.
//! * `capabilities()` appears, because whether a body is escaped is a question
//!   only the backend can answer (`body-markup`), and answering it in the
//!   capability would mean the capability knowing about freedesktop.
//!
//! # What is deliberately absent
//!
//! **No actions.** There is no `add_action`, no button, no callback carrying a
//! chosen index. `notifications_v1.proto` has no field that could carry one,
//! and this trait has no method that could invoke one, so the guarantee is the
//! shape of the interface rather than a check a later change could invert.
//!
//! **No content in an error.** [`SinkError`] carries a *class* and, at most, a
//! platform error *name*. Nothing constructed here can be printed into a log
//! and reveal what a notification said.

/// The Linux backends. Feature-gated: with `linux-dbus` off, this module is
/// the two traits and their in-memory implementations, and nothing here names
/// a bus, a desktop or an operating system.
#[cfg(feature = "linux-dbus")]
pub mod dbus;
#[cfg(feature = "linux-dbus")]
pub mod logind;

use std::sync::Arc;

use tokio::sync::mpsc;

/// The notification server's own handle for one displayed notification.
///
/// A `u32` because that is what freedesktop uses, and wrapping it in a newtype
/// would buy nothing here — but note what it is **not**: it is not stable
/// across a server restart, it is not an AnyFlow identity, and a peer never
/// sees one. The mapping from the opaque remote identity to this local number
/// is `MirrorTable`'s, and it is memory-only.
pub type ServerId = u32;

/// Everything a backend needs in order to display one notification.
///
/// Built by the capability *after* validation, authorization, policy and the
/// lock reduction, so a backend never has to make a decision. In particular a
/// backend never sees a title or a body that policy withheld: the reduction
/// happens before this struct is built, so what is withheld does not exist
/// here to be mishandled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mirror {
    /// The application's name, already sanitized and length-capped.
    pub app_name: String,
    /// The one-line summary, already sanitized. Empty is legal: a
    /// notification with no title is normal on Android.
    pub summary: String,
    /// The body, already sanitized and — where the backend advertised
    /// `body-markup` — already escaped.
    pub body: String,
    /// How loudly to present it. Never [`Urgency::Critical`] in v1.
    pub urgency: Urgency,
    /// Whether the **source** reduced this notification before sending it.
    ///
    /// Carried so a backend may present a mirror as reduced rather than as
    /// complete. It is never an instruction to reconstruct anything: what was
    /// withheld is not here.
    pub redacted: bool,
}

/// The freedesktop urgency hint, with the one value v1 never emits removed
/// from reach.
///
/// `Critical` is deliberately not a variant. On GNOME a critical notification
/// does not auto-dismiss, and Android `IMPORTANCE_HIGH` — which every chat app
/// sets — would therefore become a banner the user has to click away for every
/// message received. **A mirror must never be more intrusive than the
/// notification it mirrors.** Making the value unrepresentable is stronger
/// than a comment saying not to use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
}

impl Urgency {
    /// The byte the freedesktop `urgency` hint carries.
    pub fn as_hint(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
        }
    }
}

/// Why a notification server call did not do what was asked.
///
/// Coarse on purpose. These reach the local operator through the daemon log
/// and `anyflow notifications status`, and a reduced form of the *class* — not
/// the message — reaches a peer as a [`anyflow_proto::v1::capabilities::NotificationOutcome`].
/// They never carry notification content, and the `Failed` variant carries a
/// platform error *name* rather than a formatted message for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkError {
    /// There is no notification server on this session at all, or the one
    /// there was has gone away.
    ///
    /// Distinct from `Failed` because it is the state that narrows the `SINK`
    /// role: this desktop cannot display anything for anyone, and saying so is
    /// more useful to a peer than failing every notification individually.
    Unavailable(String),
    /// The server was there and refused, or the call errored.
    Failed(String),
    /// The call did not finish within [`crate::limits::BACKEND_TIMEOUT`].
    ///
    /// Its own variant because a wedged server is a different operational
    /// situation from an absent one, and because it is the case the whole
    /// worker-task design exists to survive.
    TimedOut,
}

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "no notification server: {why}"),
            Self::Failed(why) => write!(f, "the notification server refused: {why}"),
            Self::TimedOut => f.write_str("the notification server did not answer in time"),
        }
    }
}

impl std::error::Error for SinkError {}

/// A short, stable class name for a log line. Never the message.
impl SinkError {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "unavailable",
            Self::Failed(_) => "failed",
            Self::TimedOut => "timed-out",
        }
    }
}

pub type SinkResult<T> = std::result::Result<T, SinkError>;

/// Why the desktop closed a notification we posted.
///
/// The four freedesktop reasons, preserved exactly, because the distinction
/// between them is load-bearing for N4 and collapsing it now would make that
/// wave's most important rule unimplementable:
///
/// * **[`Dismissed`] is the only one a human performed.** It is the only one
///   that may ever become a `DismissRequest` to the source.
/// * **[`Expired`] must never dismiss anything on the phone.** A desktop
///   banner timing out is not a decision, and treating it as one would clear
///   somebody's phone every time they walked away from their desk.
/// * **[`Closed`] is our own `CloseNotification` coming back**, and acting on
///   it would be an immediate self-inflicted loop.
/// * **[`Undefined`] carries no information**, so it cannot justify an action
///   on another device.
///
/// **N4 acts on exactly one of them, and only outwards.** A
/// [`Dismissed`] close for a mirror this process still holds becomes one
/// `DismissRequest` to the peer that sourced it, subject to both ends' policy
/// and the peer's `DISMISS_TARGET` role; every other reason produces no remote
/// effect whatsoever. The local effect is the same for all four: the mirror's
/// entry is dropped — because the server invalidates the id *before* the
/// signal is sent, so a later `replaces_id` naming it would create a second
/// notification rather than update the first.
///
/// [`Dismissed`]: CloseReason::Dismissed
/// [`Expired`]: CloseReason::Expired
/// [`Closed`]: CloseReason::Closed
/// [`Undefined`]: CloseReason::Undefined
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Expired,
    Dismissed,
    Closed,
    Undefined,
}

impl CloseReason {
    /// The freedesktop reason codes, exhaustive and small.
    pub fn from_wire(value: u32) -> Self {
        match value {
            1 => Self::Expired,
            2 => Self::Dismissed,
            3 => Self::Closed,
            _ => Self::Undefined,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::Dismissed => "dismissed",
            Self::Closed => "closed",
            Self::Undefined => "undefined",
        }
    }

    /// Whether a human performed this close.
    ///
    /// **The only `true` in this function is the only thing in the whole
    /// capability that may cause a `DismissRequest`.** It is deliberately a
    /// single `matches!` on a single variant rather than a list of exclusions:
    /// a rule written as "everything except expiry" grows a hole the day a
    /// fifth reason is added, and the hole would clear somebody's phone.
    pub fn is_human_dismissal(&self) -> bool {
        matches!(self, Self::Dismissed)
    }
}

/// One notification the desktop closed, as observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closed {
    pub id: ServerId,
    pub reason: CloseReason,
}

/// What a notification server says it can do.
///
/// Read once, at connect. Only the parts the sink actually branches on are
/// modelled: everything else the server advertises is either irrelevant to a
/// text mirror (`sound`, `icon-static`) or something v1 refuses to use
/// (`actions`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SinkCapabilities {
    /// The server parses markup in the body, so the body must be escaped
    /// before it is sent. GNOME advertises this.
    pub body_markup: bool,
    /// The server renders a body at all. A server without it gets a summary
    /// and nothing else, rather than a body silently dropped into a void.
    pub body: bool,
    /// Notifications stay in a list until acknowledged, rather than only
    /// appearing as a transient banner. GNOME advertises this, which is why
    /// duplicate suppression here is a visible requirement and not a tidy one.
    pub persistence: bool,
    /// Whether this backend can positively identify a **human** dismissal of a
    /// notification it displayed.
    ///
    /// The one field here that is not read from the server's own
    /// `GetCapabilities`, because it is not a property of the server alone: it
    /// is true when a close signal actually reaches this process *and* its
    /// reason distinguishes a person closing a notification from a banner
    /// timing out or from our own `CloseNotification` returning. For
    /// freedesktop that is `NotificationClosed` plus reason 2
    /// ([`CloseReason::Dismissed`]).
    ///
    /// **This is the single input to the `DISMISS_REPORTER` role**, and it
    /// defaults to `false` so that a backend which has not answered the
    /// question claims nothing. A desktop that cannot tell a human dismissal
    /// from an expiry must never announce that it will report one: a peer that
    /// believed the claim would have its notifications cleared every time a
    /// banner timed out on a screen nobody was looking at.
    pub dismiss_reporting: bool,
}

/// Display, replace and close notifications on this desktop.
///
/// Implementations must honour three rules that the signatures cannot state:
///
/// * **A close that finds nothing is a success.** The specification says a
///   server sends an error for an unknown id; GNOME sends nothing at all
///   (HOST VERIFIED — three closes, one signal, no errors). Either way the
///   notification is gone, which is what was asked for, so
///   [`close`](NotificationSink::close) must return `Ok` in both cases. A sink
///   that reported the spec-compliant error as a failure would log one on
///   every dismissal against dunst or mako.
/// * **The returned id is authoritative.** Never assume the id handed to
///   `replaces` comes back.
/// * **Bounded.** Every call must complete or fail within
///   [`crate::limits::BACKEND_TIMEOUT`]; none may block indefinitely. The
///   capability enforces this with its own timeout as well, because a
///   guarantee that lives only in an implementation is a guarantee one
///   implementation can forget.
#[async_trait::async_trait]
pub trait NotificationSink: Send + Sync {
    /// Short, stable name for diagnostics, e.g. `"freedesktop"`.
    fn id(&self) -> &'static str;

    /// One line describing what is actually there, for
    /// `anyflow notifications status`.
    fn describe(&self) -> String;

    /// What this server can do. Read once at connect; never re-negotiated.
    fn capabilities(&self) -> SinkCapabilities;

    /// Whether the server is reachable right now.
    ///
    /// The answer decides whether this device announces the `SINK` role at
    /// all: a desktop with no notification server cannot display anything, and
    /// claiming otherwise would make a peer send content into a void.
    async fn availability(&self) -> SinkResult<()>;

    /// Creates or replaces one notification, returning the server's id for it.
    ///
    /// `replaces` names an id this sink previously returned. Passing one that
    /// the server has already invalidated is not an error: the server creates
    /// a new notification instead, and the returned id — which the caller
    /// stores — is what keeps the two ends in step.
    async fn display(&self, mirror: &Mirror, replaces: Option<ServerId>) -> SinkResult<ServerId>;

    /// Closes one notification. A `no such id` answer is a success.
    async fn close(&self, id: ServerId) -> SinkResult<()>;

    /// A stream of notifications the desktop closed.
    ///
    /// `None` from an implementation that cannot observe closes; the
    /// capability then simply never learns about them, which costs it a
    /// mirror-table entry per user dismissal and nothing else.
    ///
    /// Called once. The receiver is handed over rather than cloned, so a
    /// second call returns `None` — there is one stream and one consumer.
    fn closed_events(&self) -> Option<mpsc::Receiver<Closed>>;

    /// A stream of availability changes, if the platform can report them.
    ///
    /// `true` means a server is present. This is what makes the `SINK` role
    /// narrow and widen without polling, and it is what turns a GNOME Shell
    /// restart from a silent failure into an observed event.
    fn availability_events(&self) -> Option<mpsc::Receiver<bool>>;
}

// ---------------------------------------------------------------------------
// Lock state
// ---------------------------------------------------------------------------

/// Whether this desktop session is locked.
///
/// A separate seam from [`NotificationSink`] because they are separate
/// questions with separate platform answers: on Linux the notification server
/// is on the *session* bus and the lock state is a *system* bus property of a
/// logind session object. A sink that answered both would be two adapters
/// wearing one trait.
///
/// # The one rule
///
/// **Anything other than a confident "unlocked" is locked.** A D-Bus error, a
/// missing service, a session object that cannot be resolved, a timeout — all
/// of them answer `true`. A privacy control that fails open is not a control,
/// and this is the seam where that is decided rather than in each caller.
#[async_trait::async_trait]
pub trait LockSource: Send + Sync {
    /// Short, stable name for diagnostics, e.g. `"logind"`.
    fn id(&self) -> &'static str;

    /// One line describing where the answer comes from.
    fn describe(&self) -> String;

    /// Whether the session is locked **right now**.
    ///
    /// Implementations must return `true` when they do not know.
    async fn is_locked(&self) -> bool;

    /// A stream of lock-state changes, if the platform can report them.
    ///
    /// The capability uses this to re-post its mirrors reduced the moment the
    /// screen locks. An implementation that cannot report changes returns
    /// `None`, and the lock policy then applies only to notifications that
    /// arrive after the lock — which is a real degradation, and is why the
    /// status report says which source is in use.
    fn lock_events(&self) -> Option<mpsc::Receiver<bool>>;
}

// ---------------------------------------------------------------------------
// In-memory implementations
// ---------------------------------------------------------------------------

/// A notification server that exists only in this process.
///
/// Used by the capability's own suite and by the daemon integration tests, so
/// that every rule about mirrors, replacement, closing and convergence is
/// proved deterministically and without a desktop session. It behaves like the
/// **specification**, not like GNOME: closing an unknown id is a success but
/// is recorded, and `display` returns a fresh id unless `replaces` names a
/// notification it still holds — which is the behaviour a spec-literal server
/// like dunst would show, and therefore the behaviour worth testing against.
pub struct MemorySink {
    inner: std::sync::Mutex<MemorySinkState>,
    closed_tx: mpsc::Sender<Closed>,
    closed_rx: std::sync::Mutex<Option<mpsc::Receiver<Closed>>>,
    availability_tx: mpsc::Sender<bool>,
    availability_rx: std::sync::Mutex<Option<mpsc::Receiver<bool>>>,
}

#[derive(Default)]
struct MemorySinkState {
    next_id: ServerId,
    live: std::collections::BTreeMap<ServerId, Mirror>,
    /// Every `display` call, in order, as `(replaces, mirror)`.
    displays: Vec<(Option<ServerId>, Mirror)>,
    /// Every `close` call, in order.
    closes: Vec<ServerId>,
    /// The id the last successful `display` returned.
    last_id: Option<ServerId>,
    failure: Option<SinkError>,
    capabilities: SinkCapabilities,
    available: bool,
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

impl MemorySink {
    pub fn new() -> Self {
        let (closed_tx, closed_rx) = mpsc::channel(64);
        let (availability_tx, availability_rx) = mpsc::channel(64);
        Self {
            inner: std::sync::Mutex::new(MemorySinkState {
                next_id: 1,
                capabilities: SinkCapabilities {
                    // The GNOME set, because that is what the certification
                    // target advertises and a fake that is easier than the
                    // real thing tests the wrong code.
                    body_markup: true,
                    body: true,
                    persistence: true,
                    // And GNOME does deliver `NotificationClosed` with reason
                    // 2 for a human close, HOST VERIFIED in N2 and re-proved
                    // in N4's `real_dbus` gate.
                    dismiss_reporting: true,
                },
                available: true,
                ..MemorySinkState::default()
            }),
            closed_tx,
            closed_rx: std::sync::Mutex::new(Some(closed_rx)),
            availability_tx,
            availability_rx: std::sync::Mutex::new(Some(availability_rx)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemorySinkState> {
        // A poisoned mutex here means a test panicked while holding it; the
        // useful failure is that panic, not a second one from this line.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every `display` call so far, as `(replaces, mirror)`.
    pub fn displays(&self) -> Vec<(Option<ServerId>, Mirror)> {
        self.lock().displays.clone()
    }

    /// Every `close` call so far, in order.
    pub fn closes(&self) -> Vec<ServerId> {
        self.lock().closes.clone()
    }

    /// The id the last successful `display` returned.
    ///
    /// The server's number, not a count of calls: the two differ as soon as a
    /// replacement or a restart is involved, and a test that assumed they were
    /// the same would be asserting against its own arithmetic.
    pub fn last_server_id(&self) -> Option<ServerId> {
        self.lock().last_id
    }

    /// How many notifications this server currently holds.
    pub fn live_count(&self) -> usize {
        self.lock().live.len()
    }

    /// The notification currently held under `id`, if any.
    pub fn live(&self, id: ServerId) -> Option<Mirror> {
        self.lock().live.get(&id).cloned()
    }

    /// Makes every subsequent call fail.
    pub fn set_failure(&self, failure: Option<SinkError>) {
        self.lock().failure = failure;
    }

    pub fn set_capabilities(&self, capabilities: SinkCapabilities) {
        self.lock().capabilities = capabilities;
    }

    /// Simulates a human closing a notification on the desktop, or the server
    /// expiring one.
    pub async fn user_closes(&self, id: ServerId, reason: CloseReason) {
        self.lock().live.remove(&id);
        let _ = self.closed_tx.send(Closed { id, reason }).await;
    }

    /// The notification server dies — a GNOME Shell crash or restart.
    ///
    /// Everything it was holding is gone and **no `NotificationClosed` signal
    /// is emitted**: a shell that died did not tell anybody what it had. Its
    /// numbering also starts again, which is what makes a stale id *dangerous*
    /// rather than merely useless — id 1 may well be valid again and belong to
    /// somebody else's notification. The fake reproduces both faithfully
    /// rather than helpfully, because they are the cases the recovery has to
    /// survive.
    pub async fn go_away(&self) {
        {
            let mut state = self.lock();
            state.live.clear();
            state.next_id = 1;
            state.available = false;
            state.failure = Some(SinkError::Unavailable("gone".into()));
        }
        let _ = self.availability_tx.send(false).await;
    }

    /// A notification server acquires the name.
    pub async fn come_back(&self) {
        {
            let mut state = self.lock();
            state.available = true;
            state.failure = None;
        }
        let _ = self.availability_tx.send(true).await;
    }
}

#[async_trait::async_trait]
impl NotificationSink for MemorySink {
    fn id(&self) -> &'static str {
        "memory"
    }

    fn describe(&self) -> String {
        "in-memory notification server (tests only)".to_string()
    }

    fn capabilities(&self) -> SinkCapabilities {
        self.lock().capabilities.clone()
    }

    async fn availability(&self) -> SinkResult<()> {
        let state = self.lock();
        if let Some(f) = &state.failure {
            return Err(f.clone());
        }
        if state.available {
            Ok(())
        } else {
            Err(SinkError::Unavailable("no server".into()))
        }
    }

    async fn display(&self, mirror: &Mirror, replaces: Option<ServerId>) -> SinkResult<ServerId> {
        let mut state = self.lock();
        if let Some(f) = &state.failure {
            return Err(f.clone());
        }
        state.displays.push((replaces, mirror.clone()));

        // The spec-literal behaviour: an id the server still holds is replaced
        // atomically and keeps its number; one it does not hold is gone, and
        // this is a new notification with a new number.
        let id = match replaces {
            Some(id) if state.live.contains_key(&id) => id,
            _ => {
                let id = state.next_id;
                state.next_id = state.next_id.saturating_add(1);
                id
            }
        };
        state.live.insert(id, mirror.clone());
        state.last_id = Some(id);
        Ok(id)
    }

    async fn close(&self, id: ServerId) -> SinkResult<()> {
        let mut state = self.lock();
        if let Some(f) = &state.failure {
            return Err(f.clone());
        }
        state.closes.push(id);
        // Removing something that is not there is a success, and is recorded
        // so a test can assert that the *call* happened.
        state.live.remove(&id);
        Ok(())
    }

    fn closed_events(&self) -> Option<mpsc::Receiver<Closed>> {
        self.closed_rx.lock().ok()?.take()
    }

    fn availability_events(&self) -> Option<mpsc::Receiver<bool>> {
        self.availability_rx.lock().ok()?.take()
    }
}

/// A lock source a test can steer.
///
/// The default is **unlocked**, because a suite whose every assertion ran
/// against a locked session would prove the reduction path and nothing else.
/// [`set_unknown`](MemoryLock::set_unknown) is the important one: it is how the
/// fail-closed rule is tested rather than trusted.
pub struct MemoryLock {
    inner: std::sync::Mutex<MemoryLockState>,
    tx: mpsc::Sender<bool>,
    rx: std::sync::Mutex<Option<mpsc::Receiver<bool>>>,
}

#[derive(Default)]
struct MemoryLockState {
    locked: bool,
    /// The platform cannot answer. Must resolve to locked.
    unknown: bool,
}

impl Default for MemoryLock {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryLock {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(64);
        Self {
            inner: std::sync::Mutex::new(MemoryLockState::default()),
            tx,
            rx: std::sync::Mutex::new(Some(rx)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryLockState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Locks or unlocks the session and fires the change signal.
    pub async fn set_locked(&self, locked: bool) {
        self.lock().locked = locked;
        let _ = self.tx.send(locked).await;
    }

    /// Makes the source unable to answer. It must then report locked.
    pub fn set_unknown(&self, unknown: bool) {
        self.lock().unknown = unknown;
    }
}

#[async_trait::async_trait]
impl LockSource for MemoryLock {
    fn id(&self) -> &'static str {
        "memory"
    }

    fn describe(&self) -> String {
        "in-memory lock state (tests only)".to_string()
    }

    async fn is_locked(&self) -> bool {
        let state = self.lock();
        // The rule, in the one place it is decided.
        state.unknown || state.locked
    }

    fn lock_events(&self) -> Option<mpsc::Receiver<bool>> {
        self.rx.lock().ok()?.take()
    }
}

/// The sink a machine with no notification server gets.
///
/// **Reports unavailable, always.** It is deliberately not a no-op that
/// pretends to succeed: an accepting sink that displayed nothing would make
/// this device announce the `SINK` role and then swallow every notification a
/// phone sent it, which is worse than saying so — the phone would have no way
/// to know its notifications were going nowhere. Reporting unavailable makes
/// the role narrow, which is exactly the state ADR-0017 designed roles to
/// express.
pub struct NoSink;

#[async_trait::async_trait]
impl NotificationSink for NoSink {
    fn id(&self) -> &'static str {
        "none"
    }

    fn describe(&self) -> String {
        "no notification server on this session".to_string()
    }

    fn capabilities(&self) -> SinkCapabilities {
        SinkCapabilities::default()
    }

    async fn availability(&self) -> SinkResult<()> {
        Err(SinkError::Unavailable("no notification server".into()))
    }

    async fn display(&self, _mirror: &Mirror, _replaces: Option<ServerId>) -> SinkResult<ServerId> {
        Err(SinkError::Unavailable("no notification server".into()))
    }

    async fn close(&self, _id: ServerId) -> SinkResult<()> {
        // A close against a server that does not exist has already achieved
        // what it asked for: there is nothing on any screen.
        Ok(())
    }

    fn closed_events(&self) -> Option<mpsc::Receiver<Closed>> {
        None
    }

    fn availability_events(&self) -> Option<mpsc::Receiver<bool>> {
        None
    }
}

/// A lock source for a platform with no lock at all.
///
/// **Reports locked, always.** It is not a stub that says "unlocked because we
/// do not know"; it is the fail-closed answer written down as a type, so that
/// composing the capability on a platform whose lock state has not been
/// implemented yet produces reduced notifications rather than a silent privacy
/// hole. A platform that genuinely has no lock screen would need a deliberate
/// implementation saying so, and reviewing that is the point.
pub struct UnknownLock;

#[async_trait::async_trait]
impl LockSource for UnknownLock {
    fn id(&self) -> &'static str {
        "unknown"
    }

    fn describe(&self) -> String {
        "no lock-state source on this platform; treated as locked".to_string()
    }

    async fn is_locked(&self) -> bool {
        true
    }

    fn lock_events(&self) -> Option<mpsc::Receiver<bool>> {
        None
    }
}

/// Convenience alias for the shared sink the capability holds.
pub type SharedSink = Arc<dyn NotificationSink>;
/// Convenience alias for the shared lock source the capability holds.
pub type SharedLock = Arc<dyn LockSource>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urgency_has_no_critical_variant_to_reach_for() {
        assert_eq!(Urgency::Low.as_hint(), 0);
        assert_eq!(Urgency::Normal.as_hint(), 1);
        // There is deliberately no third arm. If one is ever added, the
        // reasoning on `Urgency` has to be revisited first.
    }

    #[test]
    fn close_reasons_map_exactly_and_unknown_is_undefined() {
        assert_eq!(CloseReason::from_wire(1), CloseReason::Expired);
        assert_eq!(CloseReason::from_wire(2), CloseReason::Dismissed);
        assert_eq!(CloseReason::from_wire(3), CloseReason::Closed);
        assert_eq!(CloseReason::from_wire(4), CloseReason::Undefined);
        assert_eq!(CloseReason::from_wire(99), CloseReason::Undefined);
    }

    #[test]
    fn only_a_human_dismissal_is_a_human_dismissal() {
        assert!(CloseReason::Dismissed.is_human_dismissal());
        for other in [
            CloseReason::Expired,
            CloseReason::Closed,
            CloseReason::Undefined,
        ] {
            assert!(
                !other.is_human_dismissal(),
                "{other:?} must never dismiss anything on the source"
            );
        }
    }

    #[test]
    fn a_backend_that_says_nothing_claims_no_dismiss_reporting() {
        // The default is what a new adapter gets, and what `NoSink` returns.
        // It must be the fail-closed answer: no `DISMISS_REPORTER` role.
        assert!(!SinkCapabilities::default().dismiss_reporting);
        assert!(!NoSink.capabilities().dismiss_reporting);
    }

    #[test]
    fn the_fake_desktop_reports_dismissals_like_gnome_does() {
        assert!(MemorySink::new().capabilities().dismiss_reporting);
    }

    #[tokio::test]
    async fn an_unknown_lock_source_reports_locked() {
        assert!(UnknownLock.is_locked().await);
    }

    #[tokio::test]
    async fn a_lock_source_that_cannot_answer_reports_locked() {
        let lock = MemoryLock::new();
        assert!(!lock.is_locked().await);
        lock.set_unknown(true);
        assert!(lock.is_locked().await, "unknown must fail closed");
    }
}
