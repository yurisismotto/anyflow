//! `notifications.v1` — displaying a phone's notifications on this desktop.
//!
//! # What this crate is
//!
//! The **sink half** of `notifications.v1`, and nothing else. It receives
//! `NotificationUpsert`, `NotificationRemove` and `SyncMarker` from a paired,
//! granted peer that has announced itself a `SOURCE`, and it turns them into
//! notifications on this machine's desktop through the [`backend`] seam. It
//! sources nothing: there is no supported way for an ordinary client to
//! observe other applications' notifications on Linux, so v1 does not pretend
//! there is.
//!
//! The wire contract — field limits, identifier widths, the role/epoch
//! reduction, the snapshot bracketing machine, the conservative enum
//! resolution — lives in [`anyflow_core::notifications`] and is **re-exported
//! rather than reimplemented**, so the two ends of the protocol cannot drift.
//!
//! # What this capability is careful about, and where that lives
//!
//! | Property | Enforced by |
//! | --- | --- |
//! | only a paired device can speak at all | TLS 1.3 + SPKI pinning (`anyflow_core::tls`) |
//! | only an explicitly *granted* device may display anything | [`NotificationAuthorizer`], re-asked per message |
//! | a peer can only ever touch its own mirrors | the mirror key is the pinned fingerprint ([`mirror`]) |
//! | a peer cannot widen its own policy | there is no protocol message that sets one |
//! | a peer that never claimed `SOURCE` is not listened to | [`NotificationManager::apply_upsert`] |
//! | a locked screen shows what policy says and no more | the reduction happens **before** the backend call |
//! | unknown lock state is locked | [`backend::LockSource`], in one place |
//! | markup cannot be forged | [`text::escape_markup`], before every body |
//! | an update replaces rather than duplicates | `replaces_id`, from [`mirror::MirrorTable`] |
//! | a wedged notification server cannot stall the session | [`queue`] and one worker task per peer |
//! | content never reaches a log, a store or a `Debug` | [`redact`], and hand-written renderings |
//!
//! # `content_hash` is opaque here, and that is a rule
//!
//! N1 computes `content_hash` at the source. **This sink never recomputes it.**
//! It compares the digest it was sent last for one `(peer, notification_id)`
//! with the digest it is sent now, and does nothing else with it — no
//! canonical encoding, no domain string, no SHA-256 of anything.
//!
//! That is not an omission. A digest only one side computes is a *source-side
//! optimisation*: N1 may change how it is built, or stop sending it, without
//! breaking anybody. The moment a sink recomputes and compares, the
//! construction becomes a wire-format contract that both implementations have
//! to agree on for ever, and a field documented as optional becomes load
//! bearing. So the rule is written here as well as in N1's report:
//! **de-duplicate on the value you were sent.**
//!
//! # What is never here
//!
//! No notification history, in any form: not a file, not a table, not a ring
//! buffer, not a "recent" screen, and nothing in `state.json`. No actions, no
//! buttons, no reply, no URL, no command — `notifications_v1.proto` has no
//! field that could carry one and [`backend::NotificationSink`] has no method
//! that could invoke one. No relay: there is no code path from an inbound
//! message for one peer to an outbound message for another. No dismiss event
//! journal: a dismissal produces one message and two counters, and nothing is
//! written anywhere.
//!
//! # Dismissal synchronisation (N4)
//!
//! One human dismissal on this desktop, one `DismissRequest` to the peer that
//! sourced the notification, and nothing else — no action, no reply, no
//! clear-all, and no other reason for closing a notification travels anywhere.
//!
//! ```text
//!   a person closes a mirror on this desktop
//!         │
//!         ▼  NotificationClosed(server_id, reason = 2)   ← reason 2 ONLY
//!   MirrorTable::forget_server_id  →  Some(entry)        ← still live?
//!         │
//!         ▼  allow_mirror && allow_dismiss_sync (this desktop's policy)
//!         ▼  the peer announced DISMISS_TARGET
//!         ▼  this desktop announced DISMISS_REPORTER
//!   Work::Dismiss  →  the peer's worker  →  DismissRequest{id, origin}
//! ```
//!
//! Five gates, each of which fails closed and each of which is a different
//! question. [`backend::CloseReason::is_human_dismissal`] answers the first;
//! [`mirror::MirrorTable::forget_server_id`] answers the second and is where
//! every race in the design is resolved; the rest are policy, the peer's claim
//! and this device's own claim.
//!
//! The mirror is **already gone locally** by the time the request is sent, so
//! the `NotificationRemove` the source sends after it cancels — if it sends one
//! at all; the source suppresses the echo to the peer that asked — finds
//! nothing and answers `UNKNOWN_NOTIFICATION`. That is convergence, not an
//! error, and it is what stops the loop having a second lap.

pub mod backend;
pub mod limits;
pub mod mirror;
pub mod policy;
pub mod queue;
pub mod redact;
pub mod roles;
pub mod text;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex, Notify, RwLock};
use tokio::time::Instant;

use anyflow_core::capability::{Capability, CapabilityContext, OutboundMessage};
use anyflow_core::error::{Error, Result};
use anyflow_core::notifications::{
    self as contract, NotificationId, PeerRoles, Rejection, Role, Snapshot, SnapshotStep,
};
use anyflow_core::Fingerprint;
use anyflow_proto::v1::capabilities as pb;
use anyflow_proto::Message;

use backend::{
    Closed, LockSource, Mirror, NotificationSink, ServerId, SinkCapabilities, SinkError, Urgency,
};
use mirror::MirrorTable;
pub use policy::{LockPolicy, NotificationAuthorizer, NotificationPolicy};
use queue::{CloseAllReason, Offered, QueueStats, Work, WorkQueue};
use roles::LocalRoles;

/// The canonical capability id, re-exported from the portable contract so
/// there is one spelling of it in the workspace.
pub use anyflow_core::notifications::CAPABILITY_ID;

/// A validated upsert, waiting for its turn on the worker.
///
/// Carries the identity separately from the message because the identity has
/// already been validated and the message has not been re-read since: a caller
/// that had to re-derive the id from the bytes could get a different answer if
/// the bytes were mutated in between, and there would be no reason for the two
/// to disagree except a bug.
#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    pub id: NotificationId,
    pub message: pb::NotificationUpsert,
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// What happened to one inbound message. Mirrors the protobuf enum, so the
/// wire vocabulary and the internal one cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Displayed,
    Removed,
    Duplicate,
    NotAuthorized,
    RejectedPolicy,
    RejectedRole,
    UnknownNotification,
    TooLarge,
    Invalid,
    RateLimited,
    Unavailable,
    Failed,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Displayed => "displayed",
            Self::Removed => "removed",
            Self::Duplicate => "duplicate",
            Self::NotAuthorized => "not authorized",
            Self::RejectedPolicy => "rejected by policy",
            Self::RejectedRole => "rejected: role",
            Self::UnknownNotification => "unknown notification",
            Self::TooLarge => "too large",
            Self::Invalid => "invalid",
            Self::RateLimited => "rate limited",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }

    fn to_proto(self) -> pb::NotificationOutcome {
        match self {
            Self::Displayed => pb::NotificationOutcome::Displayed,
            Self::Removed => pb::NotificationOutcome::Removed,
            Self::Duplicate => pb::NotificationOutcome::Duplicate,
            Self::NotAuthorized => pb::NotificationOutcome::NotAuthorized,
            Self::RejectedPolicy => pb::NotificationOutcome::RejectedPolicy,
            Self::RejectedRole => pb::NotificationOutcome::RejectedRole,
            Self::UnknownNotification => pb::NotificationOutcome::UnknownNotification,
            Self::TooLarge => pb::NotificationOutcome::TooLarge,
            Self::Invalid => pb::NotificationOutcome::Invalid,
            Self::RateLimited => pb::NotificationOutcome::RateLimited,
            Self::Unavailable => pb::NotificationOutcome::Unavailable,
            Self::Failed => pb::NotificationOutcome::Failed,
        }
    }

    fn from_rejection(rejection: &Rejection) -> Self {
        match rejection.outcome() {
            pb::NotificationOutcome::TooLarge => Self::TooLarge,
            _ => Self::Invalid,
        }
    }

    fn from_sink_error(error: &SinkError) -> Self {
        match error {
            SinkError::Unavailable(_) => Self::Unavailable,
            SinkError::Failed(_) | SinkError::TimedOut => Self::Failed,
        }
    }
}

/// How much of a notification reaches the screen.
///
/// Computed from the lock state and the per-peer policy, **before** anything
/// is handed to the backend. There is no "redact on the way out" step further
/// down that could be skipped, and nothing downstream is given a full body
/// plus a flag saying not to use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presentation {
    Full,
    AppOnly,
    Suppress,
}

impl Presentation {
    fn resolve(policy: &NotificationPolicy, locked: bool) -> Self {
        if !locked {
            return Self::Full;
        }
        match policy.when_sink_locked {
            LockPolicy::Full => Self::Full,
            LockPolicy::AppOnly => Self::AppOnly,
            LockPolicy::Suppress => Self::Suppress,
        }
    }

    fn is_reduced(&self) -> bool {
        matches!(self, Self::AppOnly)
    }
}

// ---------------------------------------------------------------------------
// Per-peer state
// ---------------------------------------------------------------------------

struct PeerState {
    /// What this device has told the peer it can do, on this connection.
    local_roles: LocalRoles,
    /// What the peer told us it can do. Recorded; never an authorization
    /// input.
    peer_roles: PeerRoles,
    mirrors: MirrorTable,
    snapshot: Snapshot,
    snapshot_deadline: Option<Instant>,
    connected: bool,
    /// The most recent lock state this peer's mirrors were rendered under.
    locked: bool,
    /// How many `DismissRequest`s this desktop has sent this peer.
    ///
    /// A count, so "one physical dismissal produced exactly one request" is
    /// observable rather than asserted. Not a journal: there is no identity,
    /// no time and no content here, and it dies with the process.
    dismissals_sent: u64,
    /// How many of those the source answered with something other than
    /// `REMOVED` — refused by its policy, not dismissible, already gone.
    ///
    /// Reported because it is the honest answer to "I turned this on and my
    /// phone is not clearing": the desktop asked, and the phone said no.
    dismissals_refused: u64,
    /// How many detaches belong to sessions that have already been replaced.
    ///
    /// A phone that reconnects before this desktop noticed the old socket had
    /// died produces two sessions for one peer. The daemon resolves that by
    /// keeping the newer and shutting the older down — but the *order* the
    /// capability sees is the other way round: `on_peer_connected` for the new
    /// session runs before the displaced one's loop has finished, so the
    /// displaced session's `on_peer_disconnected` arrives **after** the live
    /// session has already attached.
    ///
    /// Without this counter that late detach looked like the live session
    /// ending: it cleared the outbound sender, set `connected = false` and
    /// armed a grace timer carrying the *new* generation — so the generation
    /// check could not catch it, and sixty seconds later every mirror the live
    /// session was showing was closed with `reason="grace expired"` while the
    /// session was up and healthy. Observed on hardware during the N5 §8 gate,
    /// after a Wi-Fi outage:
    ///
    /// ```text
    /// session established … (a newer session)
    /// replaced by a newer session; closing the old one   session=2
    /// closed every mirror for a peer  closed=4  reason="grace expired"
    /// ```
    ///
    /// So an attach that finds the slot already connected records that exactly
    /// one detach is owed to a session that is already over, and the next
    /// detach is spent against it rather than against the live one.
    superseded_sessions: u32,
}

impl PeerState {
    fn new() -> Self {
        Self {
            local_roles: LocalRoles::new(),
            peer_roles: PeerRoles::none(),
            mirrors: MirrorTable::new(),
            snapshot: Snapshot::new(),
            snapshot_deadline: None,
            connected: false,
            locked: false,
            dismissals_sent: 0,
            dismissals_refused: 0,
            superseded_sessions: 0,
        }
    }
}

/// Everything held on one peer's behalf, including across a disconnect.
///
/// The slot outlives the session on purpose: the reconnect grace is the whole
/// reason. A three-second Wi-Fi blip that cleared the desktop and then
/// re-posted everything would, on a `persistence` server like GNOME, fill the
/// notification list with duplicates — which is precisely the "reconnect
/// explodes into duplicates" failure this design exists to avoid.
struct PeerSlot {
    peer: Fingerprint,
    queue: Arc<WorkQueue>,
    wake: Arc<Notify>,
    state: Mutex<PeerState>,
    outbound: RwLock<Option<mpsc::Sender<OutboundMessage>>>,
    shutdown: AtomicBool,
    /// Incremented on every session attach, so a grace timer armed by an
    /// earlier disconnect cannot close the mirrors of the session that
    /// replaced it.
    generation: AtomicU64,
}

impl PeerSlot {
    fn wake(&self) {
        self.wake.notify_one();
    }
}

/// A peer's notification state, described without any content. Safe to print.
#[derive(Debug, Clone)]
pub struct PeerReport {
    pub peer: Fingerprint,
    pub connected: bool,
    /// How many notifications this peer currently has on this screen.
    pub mirrors: usize,
    /// How many of those the notification server has actually accepted.
    pub displayed: usize,
    /// Mirrors evicted because the per-peer ceiling was reached.
    pub evicted: u64,
    /// The roles this device last announced to the peer, and the epoch.
    pub local_roles: usize,
    pub local_epoch: u32,
    /// Whether the peer has claimed it can source notifications.
    pub peer_is_source: bool,
    /// Whether the peer has claimed it will act on a `DismissRequest`.
    ///
    /// Reported separately from [`Self::peer_is_source`] because they narrow
    /// independently and have different fixes: a phone can stop sourcing and
    /// still honour dismissals for what it already sent.
    pub peer_is_dismiss_target: bool,
    pub peer_epoch: u32,
    /// Whether this desktop told the peer it will report human dismissals.
    pub local_reports_dismissals: bool,
    /// `DismissRequest`s sent to this peer, and how many it refused.
    pub dismissals_sent: u64,
    pub dismissals_refused: u64,
    pub snapshot_open: bool,
    pub queue: QueueStats,
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// All `notifications.v1` state for this device.
///
/// One instance per daemon. It owns the platform sink, the lock source, the
/// per-peer mirrors and the per-peer worker tasks.
pub struct NotificationManager {
    sink: Arc<dyn NotificationSink>,
    lock: Arc<dyn LockSource>,
    capabilities: SinkCapabilities,
    authorizer: RwLock<Option<Arc<dyn NotificationAuthorizer>>>,
    peers: RwLock<HashMap<Fingerprint, Arc<PeerSlot>>>,
    /// Whether a notification server is reachable. The single input to the
    /// role announcement.
    available: AtomicBool,
    /// The last lock state observed, so a newly attached peer starts from the
    /// truth rather than from an optimistic default.
    locked: AtomicBool,
    /// How many `NotificationClosed` signals this device has processed.
    ///
    /// A monotonic count of an event that otherwise has **no observable
    /// effect at all** in the cases that matter most: a close for a mirror
    /// that is already gone changes nothing, sends nothing and leaves nothing
    /// to wait on. Every negative test in `tests/dismiss.rs` is about exactly
    /// that shape, and without this they would have to sleep and hope — which
    /// is how a suite comes to pass because the thing it was watching for had
    /// not happened *yet*.
    ///
    /// It is a number. It names no notification, no peer and no reason, it is
    /// not persisted, and it is not in any report a peer can see.
    closes_observed: AtomicU64,
    /// How long a disconnected peer's mirrors stay on screen, in
    /// milliseconds.
    ///
    /// A field rather than the constant read directly, so that the grace can
    /// be *exercised* rather than reasoned about. Its value is
    /// [`limits::RECONNECT_GRACE`] unless a test shortens it, and shortening
    /// it is the only way to run the "disconnect longer than the grace" and
    /// "peer never returns" cases without a suite that takes a minute per
    /// assertion. Sixty seconds of real waiting is not a better test than one
    /// that moves the boundary and crosses it in both directions.
    reconnect_grace_ms: AtomicU64,
    /// A backend claimed it could report dismissals and then handed over no
    /// close stream.
    ///
    /// `SinkCapabilities::dismiss_reporting` is the backend's claim;
    /// `closed_events()` returning `Some` is the claim being true. The Linux
    /// sink derives one from the other and cannot disagree with itself, but a
    /// role is a statement about what this device can *physically do right
    /// now* (ADR-0017 §6), and announcing `DISMISS_REPORTER` with no stream
    /// to observe would be a claim this desktop cannot keep — the phone would
    /// offer its dismiss-sync switch, the person would turn it on, and
    /// nothing would ever happen. So the announcement is gated on what
    /// actually arrived rather than on what was advertised.
    ///
    /// Set only by [`spawn_platform_pumps`](Self::spawn_platform_pumps), so a
    /// manager whose pumps were never started behaves exactly as before.
    close_stream_missing: AtomicBool,
}

impl NotificationManager {
    /// Builds the manager and probes the backend once.
    ///
    /// Probing here rather than lazily is what lets `anyflow notifications
    /// status` report what this session can actually do, and it is what
    /// decides the first role announcement. A desktop with no notification
    /// server is a normal, reportable state — not an error, and not a reason
    /// to fail to start.
    pub async fn new(sink: Arc<dyn NotificationSink>, lock: Arc<dyn LockSource>) -> Arc<Self> {
        let capabilities = sink.capabilities();
        let available = sink.availability().await.is_ok();
        let locked = lock.is_locked().await;

        if !available {
            tracing::info!(
                backend = %sink.id(),
                "no notification server on this session; notifications.v1 is \
                 supported but will announce no SINK role"
            );
        }

        Arc::new(Self {
            sink,
            lock,
            capabilities,
            authorizer: RwLock::new(None),
            peers: RwLock::new(HashMap::new()),
            available: AtomicBool::new(available),
            locked: AtomicBool::new(locked),
            closes_observed: AtomicU64::new(0),
            reconnect_grace_ms: AtomicU64::new(limits::RECONNECT_GRACE.as_millis() as u64),
            close_stream_missing: AtomicBool::new(false),
        })
    }

    /// Wires the grant/policy source.
    ///
    /// Separate from [`new`](Self::new) for the reason `clipboard.v1`'s is:
    /// the daemon state is the authorizer and it needs the manager, so one of
    /// the two has to exist first.
    pub async fn set_authorizer(&self, authorizer: Arc<dyn NotificationAuthorizer>) {
        *self.authorizer.write().await = Some(authorizer);
    }

    pub fn sink(&self) -> &Arc<dyn NotificationSink> {
        &self.sink
    }

    pub fn lock_source(&self) -> &Arc<dyn LockSource> {
        &self.lock
    }

    pub fn capabilities(&self) -> &SinkCapabilities {
        &self.capabilities
    }

    /// Whether a notification server is reachable right now.
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    /// Whether this desktop session is locked, as last observed.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    /// Whether this desktop's backend can positively identify a **human**
    /// dismissal, and therefore whether it announces `DISMISS_REPORTER`.
    ///
    /// Read from the backend once at connect and never re-negotiated, exactly
    /// as the other sink capabilities are. It is a property of the platform,
    /// not of a peer and not of a policy.
    pub fn reports_dismissals(&self) -> bool {
        self.capabilities.dismiss_reporting && !self.close_stream_missing.load(Ordering::Acquire)
    }

    /// How many desktop close signals have been fully processed.
    ///
    /// See [`Self::closes_observed`]'s field documentation: it exists so that
    /// "this close produced no remote effect" can be asserted after the close
    /// has demonstrably been handled, rather than after an arbitrary delay.
    pub fn closes_observed(&self) -> u64 {
        self.closes_observed.load(Ordering::Acquire)
    }

    /// How long a disconnected peer's mirrors stay on screen.
    pub fn reconnect_grace(&self) -> Duration {
        Duration::from_millis(self.reconnect_grace_ms.load(Ordering::Acquire))
    }

    /// Moves the reconnect grace, for tests that need to cross it.
    ///
    /// Not a tunable and not on any control surface: nothing in the daemon
    /// calls this. It exists because the grace's two interesting behaviours
    /// are on opposite sides of a sixty-second boundary, and a suite that
    /// proved only the near side would be proving half the rule. Zero is
    /// rejected — a grace of zero is the duplicate-and-clear churn the grace
    /// exists to prevent, and it must not be reachable even from a test.
    pub fn set_reconnect_grace(&self, grace: Duration) {
        assert!(
            !grace.is_zero(),
            "the reconnect grace is normatively greater than zero"
        );
        self.reconnect_grace_ms
            .store(grace.as_millis() as u64, Ordering::Release);
    }

    async fn policy_for(&self, peer: &Fingerprint) -> NotificationPolicy {
        match self.authorizer.read().await.as_ref() {
            Some(authorizer) => authorizer.policy_for(peer).await,
            // No authorizer wired is a programming error, not a peer's doing.
            // Failing closed is the only safe reading of "we cannot tell".
            None => NotificationPolicy::DENIED,
        }
    }

    // -----------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------

    async fn slot(self: &Arc<Self>, peer: Fingerprint) -> Arc<PeerSlot> {
        if let Some(slot) = self.peers.read().await.get(&peer) {
            return Arc::clone(slot);
        }
        let mut peers = self.peers.write().await;
        if let Some(slot) = peers.get(&peer) {
            return Arc::clone(slot);
        }
        let slot = Arc::new(PeerSlot {
            peer,
            queue: Arc::new(WorkQueue::new()),
            wake: Arc::new(Notify::new()),
            state: Mutex::new(PeerState::new()),
            outbound: RwLock::new(None),
            shutdown: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        });
        peers.insert(peer, Arc::clone(&slot));

        // One worker per peer, for as long as the slot exists. It is the
        // single ordered producer for everything this device sends that peer.
        let manager = Arc::clone(self);
        let worker = Arc::clone(&slot);
        tokio::spawn(async move { manager.run_worker(worker).await });

        slot
    }

    /// Records a peer's live session and starts its role announcement.
    pub async fn attach_session(
        self: &Arc<Self>,
        peer: Fingerprint,
        outbound: mpsc::Sender<OutboundMessage>,
    ) {
        let slot = self.slot(peer).await;
        slot.generation.fetch_add(1, Ordering::AcqRel);
        *slot.outbound.write().await = Some(outbound);

        {
            let mut state = slot.state.lock().await;
            // Attaching over a slot that still believes it is connected means
            // this session is replacing one the daemon has not finished
            // tearing down. Its `on_peer_disconnected` is still to come, and
            // it must not be read as this session ending.
            if state.connected {
                state.superseded_sessions = state.superseded_sessions.saturating_add(1);
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    "attached over a session that has not finished closing; \
                     its detach will be ignored"
                );
            }
            state.connected = true;
            // A fresh connection: the peer has accepted no epoch from us and
            // we have accepted none from it. Both sides start again at 1.
            state.local_roles.reset();
            state.peer_roles = PeerRoles::none();
            // An open snapshot cannot survive the connection it was opened on.
            state.snapshot.abandon();
            state.snapshot_deadline = None;
        }

        slot.queue.offer(Work::AnnounceRoles);
        slot.wake();
    }

    /// Forgets a peer's session, and arms the reconnect grace.
    ///
    /// The mirrors stay. They are closed when the grace expires, or replaced
    /// in place by the reconnect's snapshot, whichever happens first.
    pub async fn detach_session(self: &Arc<Self>, peer: &Fingerprint) {
        let Some(slot) = self.peers.read().await.get(peer).cloned() else {
            return;
        };
        {
            // Spend a superseded session's detach here, before anything is
            // torn down: the live session's outbound sender, its roles and its
            // mirrors all belong to a session that is still running.
            let mut state = slot.state.lock().await;
            if state.superseded_sessions > 0 {
                state.superseded_sessions -= 1;
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    "ignored the detach of a session that had already been \
                     replaced"
                );
                return;
            }
        }

        *slot.outbound.write().await = None;
        let generation = slot.generation.load(Ordering::Acquire);
        {
            let mut state = slot.state.lock().await;
            state.connected = false;
            // An incomplete snapshot must never be applied, and its peer has
            // gone, so it can never be completed either.
            state.snapshot.abandon();
            state.snapshot_deadline = None;
            // Role state is **per connection** (ADR-0017 §4) and the session
            // it described is over, so it is dropped here rather than only
            // being replaced by the next `attach_session`.
            //
            // Replacing it on attach alone was almost enough, and the gap is
            // the one N5 exists to close: a session that is rebuilt *without*
            // this capability negotiated never calls `attach_session` at all,
            // so the previous session's roles survived it — and `anyflow
            // notifications status` went on reporting "the device can source
            // notifications (epoch 2)" for a peer whose current session has
            // no channel to say so on. Nothing could flow, because the grant
            // is re-checked per message; what it cost was the truth of the
            // one screen somebody reads when they are trying to work out why
            // their notifications stopped.
            state.local_roles.reset();
            state.peer_roles = PeerRoles::none();
        }

        let manager = Arc::clone(self);
        let armed = Arc::clone(&slot);
        tokio::spawn(async move {
            tokio::time::sleep(manager.reconnect_grace()).await;
            // The generation check is the whole point: a reconnect during the
            // grace makes this timer's peer a previous session, and closing
            // the live session's mirrors because an old one expired would be
            // exactly the duplicate-and-clear churn the grace exists to stop.
            if armed.generation.load(Ordering::Acquire) != generation {
                return;
            }
            if armed.state.lock().await.connected {
                return;
            }
            armed
                .queue
                .offer(Work::CloseAll(CloseAllReason::GraceExpired));
            armed.wake();
            // Nothing removes the slot here. It holds no content, its mirror
            // table is about to be empty, and keeping it means a peer that
            // reconnects finds its worker already running.
            let _ = &manager;
        });
    }

    /// Closes every mirror a peer holds, now.
    ///
    /// Called when the `notifications.v1` grant is withdrawn or the pairing is
    /// revoked. There is no grace here on purpose: a grace is for a peer that
    /// may come back, and a revoked peer is one the user has just said should
    /// not be on this screen.
    pub async fn revoke_peer(self: &Arc<Self>, peer: &Fingerprint) {
        let Some(slot) = self.peers.read().await.get(peer).cloned() else {
            return;
        };
        slot.queue.offer(Work::CloseAll(CloseAllReason::Revoked));
        slot.wake();
    }

    /// Every peer's notification state, described without content.
    pub async fn peer_reports(&self) -> Vec<PeerReport> {
        let peers: Vec<_> = self.peers.read().await.values().cloned().collect();
        let mut out = Vec::with_capacity(peers.len());
        for slot in peers {
            let state = slot.state.lock().await;
            out.push(PeerReport {
                peer: slot.peer,
                connected: state.connected,
                mirrors: state.mirrors.len(),
                displayed: state
                    .mirrors
                    .iter()
                    .filter(|(_, e)| e.server_id.is_some())
                    .count(),
                evicted: state.mirrors.evicted(),
                local_roles: state.local_roles.count(),
                local_epoch: state.local_roles.epoch(),
                peer_is_source: state.peer_roles.has(Role::Source),
                peer_is_dismiss_target: state.peer_roles.has(Role::DismissTarget),
                peer_epoch: state.peer_roles.epoch(),
                local_reports_dismissals: state.local_roles.announced(Role::DismissReporter),
                dismissals_sent: state.dismissals_sent,
                dismissals_refused: state.dismissals_refused,
                snapshot_open: state.snapshot.is_open(),
                queue: slot.queue.stats(),
            });
        }
        out.sort_by_key(|r| r.peer.to_hex());
        out
    }

    /// How many notifications this desktop is showing on behalf of all peers.
    pub async fn mirror_count(&self) -> usize {
        self.peer_reports().await.iter().map(|r| r.mirrors).sum()
    }

    // -----------------------------------------------------------------------
    // Inbound
    // -----------------------------------------------------------------------

    /// Handles one decoded `notifications.v1` payload from `peer`.
    ///
    /// This runs **on the session's dispatch loop**, so it does exactly what
    /// must happen in order and nothing that could block: decode, validate
    /// against the portable contract, check the grant, and hand the rest to
    /// the peer's worker. No D-Bus call happens here, ever.
    pub async fn handle_control(self: &Arc<Self>, peer: Fingerprint, payload: &[u8]) -> Result<()> {
        let control = pb::NotificationControl::decode(payload)
            .map_err(|_| Error::Protocol("malformed notifications.v1 payload"))?;

        // Every bound the protocol defines, in one call, before anything is
        // read for meaning. A message past the ceiling is refused for being
        // past the ceiling rather than for whichever field came first.
        if let Err(rejection) = contract::validate_control(&control, payload.len()) {
            self.refuse(peer, &control, &rejection).await;
            return Ok(());
        }

        let slot = self.slot(peer).await;

        // The grant, asked fresh. The transport filtered the capability list
        // against the grant when the session was built; a revocation since
        // then would not have been noticed there, and this is where it bites.
        let policy = self.policy_for(&peer).await;

        match control.body {
            Some(pb::notification_control::Body::Roles(announcement)) => {
                self.apply_peer_roles(&slot, &announcement).await;
            }

            Some(pb::notification_control::Body::Upsert(message)) => {
                let id = match contract::validate_upsert(&message) {
                    Ok(id) => id,
                    // Unreachable: `validate_control` already ran. Refusing
                    // again rather than unwrapping keeps the property true if
                    // the two ever diverge.
                    Err(_) => return Ok(()),
                };
                if policy.is_denied() {
                    self.answer(&slot, &id, Outcome::NotAuthorized).await;
                    return Ok(());
                }
                self.enqueue(
                    &slot,
                    Work::Upsert(Box::new(Incoming { id, message })),
                    &peer,
                )
                .await;
            }

            Some(pb::notification_control::Body::Remove(message)) => {
                let id = match contract::validate_remove(&message) {
                    Ok(id) => id,
                    Err(_) => return Ok(()),
                };
                if policy.is_denied() {
                    self.answer(&slot, &id, Outcome::NotAuthorized).await;
                    return Ok(());
                }
                self.enqueue(&slot, Work::Remove(id), &peer).await;
            }

            Some(pb::notification_control::Body::Sync(marker)) => {
                if policy.is_denied() {
                    // A marker names no notification, so there is nothing to
                    // correlate an answer with. It is dropped, and the peer
                    // learns it is ungranted from the upserts it also sent.
                    return Ok(());
                }
                self.enqueue(&slot, Work::Sync(marker), &peer).await;
            }

            Some(pb::notification_control::Body::Dismiss(message)) => {
                // Linux is a sink in v1, and N4 does not change that. A
                // `DismissRequest` asks its receiver to cancel a notification
                // *it* sourced; this device sources none, has never announced
                // `DISMISS_TARGET`, and has no path from any inbound message
                // to closing anything but its own mirrors.
                //
                // Note the direction: N4 makes this desktop a dismiss
                // *reporter*, which is the outbound half. The inbound half
                // stays refused, and the two are separate roles precisely so
                // that implementing one cannot quietly implement the other.
                // Refused, and the session survives: one capability refusing a
                // message must not cost the user everything else.
                let outcome = if policy.is_denied() {
                    Outcome::NotAuthorized
                } else {
                    Outcome::RejectedRole
                };
                if let Ok(id) = contract::validate_dismiss(&message) {
                    self.answer(&slot, &id, outcome).await;
                }
            }

            Some(pb::notification_control::Body::Result(result)) => {
                // A verdict on something this device sent. Logged as an enum
                // name with the opaque id prefix that lets two machines' logs
                // be lined up, and answered with nothing: answering an answer
                // is how two peers build a message loop out of nothing.
                let outcome = pb::NotificationOutcome::try_from(result.outcome)
                    .unwrap_or(pb::NotificationOutcome::Unspecified);
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    notification = %redact::raw_id_prefix(&result.notification_id),
                    outcome = ?outcome,
                    "peer reported a notification outcome"
                );
                // A dismissal the source declined is the one verdict an
                // operator needs a number for: "I turned this on and my phone
                // is not clearing" is answered by the phone having said no.
                //
                // A count, keyed on nothing and holding nothing. Which
                // notification it was is deliberately not retained: that would
                // be a dismiss event journal, which §26 forbids.
                if is_declined_dismissal(outcome) {
                    slot.state.lock().await.dismissals_refused += 1;
                }
            }

            // A body this build does not know, or none at all. `validate_control`
            // already refused an empty one; a future body is not fatal, and the
            // session must survive it.
            None => {}
        }

        Ok(())
    }

    /// Answers a message that failed the portable contract.
    ///
    /// A bad-width `notification_id` is refused **and not answered**: a
    /// `NotificationResult` echoes the id, and a malformed one leaves nothing
    /// coherent to correlate a reply with. Every other rejection is answered,
    /// because a sender that is told `INVALID` can act on it.
    async fn refuse(
        self: &Arc<Self>,
        peer: Fingerprint,
        control: &pb::NotificationControl,
        rejection: &Rejection,
    ) {
        tracing::debug!(
            peer = %peer.to_display_short(),
            rejection = ?rejection,
            "refused a notifications.v1 message"
        );
        if !rejection.is_answerable() {
            return;
        }
        let Some(raw) = named_identity(control) else {
            return;
        };
        let Ok(id) = NotificationId::from_bytes(raw) else {
            return;
        };
        let slot = self.slot(peer).await;
        self.answer(&slot, &id, Outcome::from_rejection(rejection))
            .await;
    }

    /// Offers work to a peer's worker, reporting a full queue to the peer.
    async fn enqueue(self: &Arc<Self>, slot: &Arc<PeerSlot>, work: Work, peer: &Fingerprint) {
        let kind = work.kind();
        // The identity is copied out before the move, so a `RATE_LIMITED`
        // answer can name what it refused.
        let named = match &work {
            Work::Upsert(incoming) => Some(incoming.id.clone()),
            Work::Remove(id) => Some(id.clone()),
            _ => None,
        };

        match slot.queue.offer(work) {
            Offered::Queued | Offered::Coalesced => {}
            Offered::Evicted => {
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    kind,
                    "notification queue is full; dropped the oldest pending update"
                );
            }
            Offered::Dropped => {
                // Only reachable with a full queue of removals. Counted by the
                // queue itself, logged here at warn because a lost removal is
                // a notification that stays on a screen.
                tracing::warn!(
                    peer = %peer.to_display_short(),
                    kind,
                    dropped_terminal = slot.queue.stats().dropped_terminal,
                    "notification queue is full of removals; dropped one"
                );
                if let Some(id) = named {
                    self.answer(slot, &id, Outcome::RateLimited).await;
                }
                return;
            }
        }
        slot.wake();
    }

    /// Records a peer's role announcement.
    async fn apply_peer_roles(
        self: &Arc<Self>,
        slot: &Arc<PeerSlot>,
        announcement: &pb::NotificationRoles,
    ) {
        let (applied, was_source, is_source, epoch) = {
            let mut state = slot.state.lock().await;
            let was_source = state.peer_roles.has(Role::Source);
            let applied = state.peer_roles.apply(announcement);
            (
                applied,
                was_source,
                state.peer_roles.has(Role::Source),
                state.peer_roles.epoch(),
            )
        };

        match applied {
            Ok(()) => tracing::info!(
                peer = %slot.peer.to_display_short(),
                roles = announcement.roles.len(),
                epoch,
                "peer roles"
            ),
            Err(rejection) => {
                tracing::info!(
                    peer = %slot.peer.to_display_short(),
                    rejection = ?rejection,
                    "peer roles refused"
                );
                return;
            }
        }

        // The narrowing direction is the safety-critical one. A phone whose
        // notification access was revoked announces an empty set on the
        // session that is already up; the mirrors it put here can no longer be
        // updated or removed by it, so they come off the screen now rather
        // than at the end of some timer.
        if was_source && !is_source {
            slot.queue
                .offer(Work::CloseAll(CloseAllReason::RoleNarrowed));
            slot.wake();
        }
    }

    /// Sends one `NotificationResult`, through the peer's outbound channel.
    async fn answer(&self, slot: &Arc<PeerSlot>, id: &NotificationId, outcome: Outcome) {
        let Some(outbound) = slot.outbound.read().await.clone() else {
            return;
        };
        let control = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Result(
                pb::NotificationResult {
                    notification_id: id.as_bytes().to_vec(),
                    outcome: outcome.to_proto() as i32,
                },
            )),
        };
        let message = OutboundMessage {
            capability_id: CAPABILITY_ID.to_string(),
            payload: control.encode_to_vec(),
        };
        match tokio::time::timeout(limits::BACKEND_TIMEOUT, outbound.send(message)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => tracing::debug!("the session ended before the result was queued"),
            Err(_) => tracing::warn!("timed out queueing a notification result"),
        }
    }

    // -----------------------------------------------------------------------
    // The worker
    // -----------------------------------------------------------------------

    async fn run_worker(self: Arc<Self>, slot: Arc<PeerSlot>) {
        loop {
            for work in slot.queue.drain() {
                self.process(&slot, work).await;
            }
            if slot.shutdown.load(Ordering::Acquire) {
                return;
            }

            let deadline = slot.state.lock().await.snapshot_deadline;
            match deadline {
                Some(at) => {
                    tokio::select! {
                        _ = slot.wake.notified() => {}
                        _ = tokio::time::sleep_until(at) => self.abandon_snapshot(&slot).await,
                    }
                }
                None => slot.wake.notified().await,
            }
        }
    }

    async fn process(&self, slot: &Arc<PeerSlot>, work: Work) {
        match work {
            Work::AnnounceRoles => self.announce_roles(slot).await,
            Work::Upsert(incoming) => {
                let outcome = self.apply_upsert(slot, &incoming).await;
                tracing::debug!(
                    peer = %slot.peer.to_display_short(),
                    notification = %redact::id_prefix(&incoming.id),
                    outcome = outcome.as_str(),
                    "notification upsert"
                );
                self.answer(slot, &incoming.id, outcome).await;
            }
            Work::Remove(id) => {
                let outcome = self.apply_remove(slot, &id).await;
                tracing::debug!(
                    peer = %slot.peer.to_display_short(),
                    notification = %redact::id_prefix(&id),
                    outcome = outcome.as_str(),
                    "notification removal"
                );
                self.answer(slot, &id, outcome).await;
            }
            Work::Dismiss {
                id,
                origin_device_id,
            } => self.send_dismiss(slot, &id, &origin_device_id).await,
            Work::Sync(marker) => self.apply_sync(slot, &marker).await,
            Work::LockChanged(locked) => self.apply_lock_change(slot, locked).await,
            Work::CloseAll(reason) => self.close_all(slot, reason).await,
        }
    }

    /// Announces this device's roles to one peer, if they have changed.
    ///
    /// # The channel is taken *before* the announcement is recorded
    ///
    /// `LocalRoles::announce` is a state transition, not a query: it records
    /// the set as announced and burns an epoch, and it answers `None` for
    /// every later call with the same set. Recording that and then discovering
    /// there is no outbound channel loses the announcement **permanently** for
    /// that session — this device would go on reporting `announced 2 (epoch
    /// 1)` to a peer that was never told anything, and nothing short of a new
    /// session could put it right. So the sender is taken first and the state
    /// is only advanced once there is somewhere for the message to go.
    async fn announce_roles(&self, slot: &Arc<PeerSlot>) {
        let Some(outbound) = slot.outbound.read().await.clone() else {
            return;
        };
        let available = self.is_available();
        let reporting = self.reports_dismissals();
        let announcement = {
            let mut state = slot.state.lock().await;
            state.local_roles.announce(available, reporting)
        };
        let Some(announcement) = announcement else {
            return;
        };

        tracing::info!(
            peer = %slot.peer.to_display_short(),
            roles = announcement.roles.len(),
            epoch = announcement.epoch,
            "announcing roles"
        );

        let control = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Roles(announcement)),
        };
        let _ = outbound
            .send(OutboundMessage {
                capability_id: CAPABILITY_ID.to_string(),
                payload: control.encode_to_vec(),
            })
            .await;
    }

    /// The decision path for one inbound upsert.
    ///
    /// The order of the checks is deliberate, and it is the same shape
    /// `clipboard.v1` uses: authorization first, then what the peer claims it
    /// is, then what the schema says about the notification itself, then
    /// policy, then de-duplication, and only then anything that touches the
    /// desktop.
    async fn apply_upsert(&self, slot: &Arc<PeerSlot>, incoming: &Incoming) -> Outcome {
        // 1. The grant, asked fresh a second time. `handle_control` asked
        //    before queueing; this asks before displaying, and the two are
        //    different moments — a revocation in between must win.
        let policy = self.policy_for(&slot.peer).await;
        if policy.is_denied() {
            return Outcome::NotAuthorized;
        }

        // 2. The peer's own claim. A device that has not said it can source
        //    notifications is not listened to when it sends one. Absent roles
        //    mean no roles, and that is the fail-closed default that lets a
        //    minimal peer interoperate harmlessly.
        if !slot.state.lock().await.peer_roles.has(Role::Source) {
            return Outcome::RejectedRole;
        }

        // 3. The app's own sensitivity declaration, resolved conservatively:
        //    unspecified is PRIVATE, and a value from the future this build
        //    cannot reason about is SECRET. `SECRET` is never displayed. The
        //    source already refuses to send one, and this sink does not take
        //    its word for it.
        let privacy = contract::privacy_or_default(incoming.message.privacy);
        if privacy == pb::NotificationPrivacy::Secret {
            self.close_mirror(slot, &incoming.id).await;
            return Outcome::RejectedPolicy;
        }

        // 4. The lock state, read from the platform rather than from a cached
        //    answer, and any failure to read it means locked.
        let locked = self.lock.is_locked().await;
        self.locked.store(locked, Ordering::Release);
        let presentation = Presentation::resolve(&policy, locked);

        if presentation == Presentation::Suppress {
            // Nothing is displayed, and anything already on the screen for
            // this identity comes off it.
            self.close_mirror(slot, &incoming.id).await;
            return Outcome::RejectedPolicy;
        }

        let mirror = self.build_mirror(&incoming.message, presentation);

        // 5. Snapshot bookkeeping, and de-duplication.
        let (replaces, duplicate) = {
            let mut state = slot.state.lock().await;
            // Recording happens whether or not the item turns out to be a
            // duplicate: a snapshot names what is *active*, and an unchanged
            // notification is as active as a changed one. Failing to record it
            // would make the reconciliation remove it.
            let step = state.snapshot.upsert(incoming.id.clone());
            debug_assert!(matches!(step, SnapshotStep::Recorded | SnapshotStep::Live));

            match state.mirrors.get(&incoming.id) {
                Some(entry) => {
                    // Identical content, presented the same way, and already
                    // on the screen. Nothing happened twice: no D-Bus call, no
                    // visual change, no re-alert.
                    //
                    // The digest is the SOURCE's. This sink never recomputes
                    // it — see the crate documentation.
                    let same_content = !incoming.message.content_hash.is_empty()
                        && entry.content_hash == incoming.message.content_hash;
                    let same_form = entry.reduced == presentation.is_reduced();
                    let displayed = entry.server_id.is_some();
                    (entry.server_id, same_content && same_form && displayed)
                }
                None => (None, false),
            }
        };

        if duplicate {
            return Outcome::Duplicate;
        }

        // 6. The desktop. `replaces` may name an id the server has already
        //    invalidated; that is not an error, the server creates a new
        //    notification, and the id it returns is the one that gets stored.
        match self.display(&mirror, replaces).await {
            Ok(server_id) => {
                let evicted = {
                    let mut state = slot.state.lock().await;
                    state.mirrors.insert(
                        incoming.id.clone(),
                        Some(server_id),
                        incoming.message.content_hash.clone(),
                        mirror.app_name.clone(),
                        mirror.urgency,
                        presentation.is_reduced(),
                        incoming.message.origin_device_id.clone(),
                    )
                };
                if let Some((_, entry)) = evicted {
                    // The ceiling was reached. Whatever was evicted must come
                    // off the screen too, or it would sit there for ever with
                    // nothing able to update or remove it.
                    if let Some(stale) = entry.server_id {
                        let _ = self.close(stale).await;
                    }
                    tracing::info!(
                        peer = %slot.peer.to_display_short(),
                        cap = limits::MAX_MIRRORS_PER_PEER,
                        "per-peer mirror ceiling reached; closed the oldest"
                    );
                }
                Outcome::Displayed
            }
            Err(error) => {
                self.note_backend_error(&error).await;
                // The identity is remembered with no server id, so the next
                // update for it creates the notification instead of trying to
                // replace one that was never made.
                let mut state = slot.state.lock().await;
                state.mirrors.insert(
                    incoming.id.clone(),
                    None,
                    Vec::new(),
                    mirror.app_name.clone(),
                    mirror.urgency,
                    presentation.is_reduced(),
                    incoming.message.origin_device_id.clone(),
                );
                Outcome::from_sink_error(&error)
            }
        }
    }

    async fn apply_remove(&self, slot: &Arc<PeerSlot>, id: &NotificationId) -> Outcome {
        let policy = self.policy_for(&slot.peer).await;
        if policy.is_denied() {
            return Outcome::NotAuthorized;
        }
        if !slot.state.lock().await.peer_roles.has(Role::Source) {
            return Outcome::RejectedRole;
        }

        let entry = slot.state.lock().await.mirrors.remove(id);
        let Some(entry) = entry else {
            // Not an error. The mirror may have been closed by the user a
            // moment ago, or the notification may never have passed a filter
            // on the way here. Both ends converging on "it is gone" is the
            // correct outcome, and answering it is what makes removal
            // idempotent.
            return Outcome::UnknownNotification;
        };

        let Some(server_id) = entry.server_id else {
            // Known, never displayed — a failed `Notify`, or suppressed by the
            // lock policy. There is nothing on the screen to close.
            return Outcome::Removed;
        };

        match self.close(server_id).await {
            Ok(()) => Outcome::Removed,
            Err(error) => {
                self.note_backend_error(&error).await;
                // The local entry is gone either way: the source has said the
                // notification no longer exists, and holding a mirror for it
                // would mean holding one nothing can ever remove. The answer
                // reports the truth about the backend rather than claiming a
                // success that did not happen.
                Outcome::from_sink_error(&error)
            }
        }
    }

    async fn apply_sync(&self, slot: &Arc<PeerSlot>, marker: &pb::SyncMarker) {
        let step = {
            let mut state = slot.state.lock().await;
            let step = state.snapshot.marker(marker);
            state.snapshot_deadline = match &step {
                SnapshotStep::Opened | SnapshotStep::Restarted => {
                    Some(Instant::now() + limits::SYNC_TIMEOUT)
                }
                SnapshotStep::Complete { .. } => None,
                _ => state.snapshot_deadline,
            };
            step
        };

        match step {
            SnapshotStep::Opened => {
                tracing::debug!(peer = %slot.peer.to_display_short(), "snapshot opened");
            }
            SnapshotStep::Restarted => {
                // A second BEGIN. The open snapshot can never be completed, so
                // it is abandoned rather than applied: an incomplete snapshot
                // that removed mirrors would delete notifications the source
                // never said were gone.
                tracing::info!(
                    peer = %slot.peer.to_display_short(),
                    "a snapshot restarted; the previous one removed nothing"
                );
            }
            SnapshotStep::Complete { named } => {
                let stale = {
                    let state = slot.state.lock().await;
                    state.mirrors.not_named_in(&named)
                };
                for id in &stale {
                    self.close_mirror(slot, id).await;
                }
                tracing::info!(
                    peer = %slot.peer.to_display_short(),
                    named = named.len(),
                    closed = stale.len(),
                    "snapshot complete"
                );
            }
            SnapshotStep::Ignored(rejection) => {
                // An END with no BEGIN, or an END for a different snapshot.
                // Neither removes anything: refusing to remove is the safe
                // direction, and the reconnect grace runs underneath.
                tracing::info!(
                    peer = %slot.peer.to_display_short(),
                    rejection = ?rejection,
                    "snapshot marker ignored; nothing was removed"
                );
            }
            SnapshotStep::Recorded | SnapshotStep::Live => {}
        }
    }

    async fn abandon_snapshot(&self, slot: &Arc<PeerSlot>) {
        let mut state = slot.state.lock().await;
        if !state.snapshot.is_open() {
            state.snapshot_deadline = None;
            return;
        }
        state.snapshot.abandon();
        state.snapshot_deadline = None;
        tracing::info!(
            peer = %slot.peer.to_display_short(),
            "a snapshot never finished; abandoned it and removed nothing"
        );
    }

    /// Re-evaluates every mirror on this peer's behalf after a lock change.
    ///
    /// # Locking reduces; unlocking does not restore
    ///
    /// When the session locks, a mirror already on the screen is re-posted in
    /// its reduced form through `replaces_id`, so the full body stops being
    /// visible without a second notification appearing. GNOME keeps
    /// notifications in a list until they are acknowledged, so leaving the
    /// full text there would defeat the setting for exactly the case it exists
    /// for — the screen the user walked away from.
    ///
    /// When the session unlocks, **nothing happens**. Restoring the full text
    /// would mean this process had kept a title and a body in memory for the
    /// length of the lock, waiting to redisplay them. That is a notification
    /// history by another name, and the design forbids one. The mirror stays
    /// reduced until its source updates it, at which point the new content
    /// arrives and is displayed in full.
    async fn apply_lock_change(&self, slot: &Arc<PeerSlot>, locked: bool) {
        self.locked.store(locked, Ordering::Release);
        slot.state.lock().await.locked = locked;
        if !locked {
            return;
        }

        let policy = self.policy_for(&slot.peer).await;
        let presentation = Presentation::resolve(&policy, true);
        if presentation == Presentation::Full {
            return;
        }

        let affected: Vec<(NotificationId, Option<ServerId>, String, Urgency, bool)> = {
            let state = slot.state.lock().await;
            state
                .mirrors
                .iter()
                .map(|(id, e)| {
                    (
                        id.clone(),
                        e.server_id,
                        e.app_name.clone(),
                        e.urgency,
                        e.reduced,
                    )
                })
                .collect()
        };

        let mut reduced = 0usize;
        for (id, server_id, app_name, urgency, already_reduced) in affected {
            if presentation == Presentation::Suppress {
                self.close_mirror(slot, &id).await;
                reduced += 1;
                continue;
            }
            if already_reduced {
                continue;
            }
            let Some(server_id) = server_id else {
                // Never displayed, so there is nothing on the screen to
                // reduce. The entry is marked so a later update knows.
                if let Some(entry) = slot.state.lock().await.mirrors.get_mut(&id) {
                    entry.reduced = true;
                }
                continue;
            };

            let mirror = Mirror {
                app_name: app_name.clone(),
                summary: app_name,
                body: String::new(),
                urgency,
                redacted: true,
            };
            match self.display(&mirror, Some(server_id)).await {
                Ok(new_id) => {
                    if let Some(entry) = slot.state.lock().await.mirrors.get_mut(&id) {
                        entry.server_id = Some(new_id);
                        entry.reduced = true;
                        // The digest described content that is no longer on
                        // the screen. Clearing it means the next upsert
                        // redisplays rather than being suppressed as a
                        // duplicate of something the user can no longer see.
                        entry.content_hash.clear();
                    }
                    reduced += 1;
                }
                Err(error) => {
                    // The full body may still be on the screen and this
                    // process cannot take it off. Closing it is the
                    // fail-closed answer: a mirror that cannot be reduced must
                    // not stay legible on a locked screen.
                    self.note_backend_error(&error).await;
                    self.close_mirror(slot, &id).await;
                }
            }
        }

        if reduced > 0 {
            tracing::info!(
                peer = %slot.peer.to_display_short(),
                reduced,
                policy = policy.when_sink_locked.as_str(),
                "the session locked; reduced what is on the screen"
            );
        }
    }

    async fn close_all(&self, slot: &Arc<PeerSlot>, reason: CloseAllReason) {
        let entries = {
            let mut state = slot.state.lock().await;
            state.snapshot.abandon();
            state.snapshot_deadline = None;
            state.mirrors.drain()
        };
        if entries.is_empty() {
            return;
        }
        for (_, entry) in &entries {
            if let Some(server_id) = entry.server_id {
                let _ = self.close(server_id).await;
            }
        }
        tracing::info!(
            peer = %slot.peer.to_display_short(),
            closed = entries.len(),
            reason = reason.as_str(),
            "closed every mirror for a peer"
        );
    }

    /// Closes and forgets one mirror, if there is one.
    async fn close_mirror(&self, slot: &Arc<PeerSlot>, id: &NotificationId) {
        let entry = slot.state.lock().await.mirrors.remove(id);
        if let Some(server_id) = entry.and_then(|e| e.server_id) {
            let _ = self.close(server_id).await;
        }
    }

    // -----------------------------------------------------------------------
    // The backend, with a bound on every call
    // -----------------------------------------------------------------------

    async fn display(
        &self,
        mirror: &Mirror,
        replaces: Option<ServerId>,
    ) -> backend::SinkResult<ServerId> {
        match tokio::time::timeout(limits::BACKEND_TIMEOUT, self.sink.display(mirror, replaces))
            .await
        {
            Ok(result) => result,
            Err(_) => Err(SinkError::TimedOut),
        }
    }

    async fn close(&self, id: ServerId) -> backend::SinkResult<()> {
        match tokio::time::timeout(limits::BACKEND_TIMEOUT, self.sink.close(id)).await {
            Ok(result) => result,
            Err(_) => Err(SinkError::TimedOut),
        }
    }

    /// Turns a backend failure into a role change where it deserves one.
    ///
    /// `Unavailable` means this desktop cannot display anything for anybody,
    /// so the `SINK` role narrows and every peer is told — which is more
    /// useful than failing each notification individually and leaving the
    /// sources to guess. A `Failed` or a `TimedOut` is one call going wrong
    /// and changes no claim.
    async fn note_backend_error(&self, error: &SinkError) {
        tracing::warn!(error = error.class(), "a notification backend call failed");
        if matches!(error, SinkError::Unavailable(_)) {
            self.set_available(false).await;
        }
    }

    /// Records that the notification server appeared or went away.
    ///
    /// The role narrows or widens for every peer, with a strictly higher
    /// epoch, on the sessions that are already up. No reconnect is needed and
    /// none is triggered: a role is a statement about what is possible right
    /// now, and the whole reason it is not in `HELLO` is that "right now"
    /// changes.
    pub async fn set_available(&self, available: bool) {
        let previous = self.available.swap(available, Ordering::AcqRel);
        if previous == available {
            return;
        }
        tracing::info!(
            available,
            backend = %self.sink.id(),
            "notification server availability changed"
        );

        let slots: Vec<_> = self.peers.read().await.values().cloned().collect();
        for slot in slots {
            if available {
                // A server that has just come back is not the server that
                // went away, even when it has the same name: gnome-shell
                // restarts its numbering, so an id this process still held
                // could now belong to somebody else's notification. The
                // handles go; the identities stay, and the next upsert or the
                // next snapshot puts the notifications back.
                slot.state.lock().await.mirrors.invalidate_server_ids();
            }
            slot.queue.offer(Work::AnnounceRoles);
            slot.wake();
        }
    }

    /// Records a lock-state change and re-evaluates every peer's mirrors.
    pub async fn set_locked(&self, locked: bool) {
        let previous = self.locked.swap(locked, Ordering::AcqRel);
        if previous == locked {
            return;
        }
        let slots: Vec<_> = self.peers.read().await.values().cloned().collect();
        for slot in slots {
            slot.queue.offer(Work::LockChanged(locked));
            slot.wake();
        }
    }

    /// Records that a notification this device posted has been closed, and —
    /// for a human dismissal alone — asks the source to dismiss it too.
    ///
    /// The server invalidates the id **before** it sends the signal — *"may
    /// not be used in any further communications with the server"* — so the
    /// mirror entry has to go, or a later update for that notification would
    /// name a dead id and silently create a second notification instead of
    /// replacing the first.
    ///
    /// # Why looking the entry up is the whole race story
    ///
    /// `forget_server_id` answers `Some` only when this signal found a mirror
    /// that was **still live**, and everything that removes a mirror for any
    /// other reason removes the entry *before* closing the notification. So
    /// the following all reach this function and all send nothing, with no
    /// timer, no flag and no suppression cache to get wrong:
    ///
    /// * a second signal for the same id — the first consumed the entry;
    /// * a signal for an id a restarted server reissued to somebody else —
    ///   [`mirror::MirrorTable::invalidate_server_ids`] dropped every handle;
    /// * our own `CloseNotification` returning after a `NotificationRemove`,
    ///   a snapshot omission, the lock policy, the per-peer ceiling, a grant
    ///   revocation or the reconnect grace — each removed the entry first;
    /// * a close for a peer that has gone away.
    ///
    /// # And the reason is checked before anything else
    ///
    /// An expiry, a programmatic close and an undefined reason all remove the
    /// entry and send nothing. Only [`CloseReason::Dismissed`] may travel, for
    /// the reason ADR-0015 §6 gives: a desktop banner timing out is not a
    /// decision, and treating one as a decision would clear somebody's phone
    /// every time they walked away from their desk.
    pub async fn note_closed(&self, closed: Closed) {
        let slots: Vec<_> = self.peers.read().await.values().cloned().collect();
        for slot in slots {
            let forgotten = slot.state.lock().await.mirrors.forget_server_id(closed.id);
            let Some((id, entry)) = forgotten else {
                continue;
            };

            tracing::debug!(
                peer = %slot.peer.to_display_short(),
                notification = %redact::id_prefix(&id),
                reason = closed.reason.as_str(),
                "the desktop closed a mirror"
            );

            if closed.reason.is_human_dismissal() {
                self.maybe_request_dismissal(&slot, id, entry.origin_device_id)
                    .await;
            }
            // One server id belongs to at most one peer's mirror, and it has
            // just been consumed. Nothing further to look at.
            break;
        }
        // Last, and unconditionally: the count means "this signal has been
        // fully decided", including the very common decision to do nothing.
        self.closes_observed.fetch_add(1, Ordering::AcqRel);
    }

    /// Decides whether a human dismissal here may become one on the source.
    ///
    /// Four independent answers, all of which must be yes, none of which is
    /// derived from another:
    ///
    /// | Question | Answered by |
    /// | --- | --- |
    /// | may this peer speak notifications with us at all, and is mirroring on? | the trust store, re-asked here |
    /// | did a person turn dismissal sync on for **this** peer? | `allow_dismiss_sync`, off by default |
    /// | will the peer act on a dismiss at all? | its own `DISMISS_TARGET` claim |
    /// | did we tell it we report dismissals? | this device's announced roles |
    ///
    /// The grant is asked *now* rather than reused from the upsert that put
    /// the notification on screen: a person who revoked the grant a second ago
    /// has not authorised a message the desktop is about to send.
    ///
    /// Nothing is sent from here. The request is queued for the peer's own
    /// worker, so it cannot overtake a result or a role announcement already
    /// in flight.
    async fn maybe_request_dismissal(
        &self,
        slot: &Arc<PeerSlot>,
        id: NotificationId,
        origin_device_id: String,
    ) {
        let policy = self.policy_for(&slot.peer).await;
        if !policy.may_sync_dismissals() {
            // The ordinary case, because the setting is off by default. Logged
            // at debug with a reason class and an opaque prefix, so that "I
            // dismissed it and my phone kept it" has an answer in the journal.
            tracing::debug!(
                peer = %slot.peer.to_display_short(),
                notification = %redact::id_prefix(&id),
                reason = "policy",
                "a human dismissal was not sent to the source"
            );
            return;
        }

        let (peer_ready, we_report) = {
            let state = slot.state.lock().await;
            (
                state.peer_roles.has(Role::DismissTarget),
                state.local_roles.announced(Role::DismissReporter),
            )
        };
        if !peer_ready || !we_report {
            tracing::debug!(
                peer = %slot.peer.to_display_short(),
                notification = %redact::id_prefix(&id),
                reason = if peer_ready { "local-role" } else { "peer-role" },
                "a human dismissal was not sent to the source"
            );
            return;
        }

        slot.queue.offer(Work::Dismiss {
            id,
            origin_device_id,
        });
        slot.wake();
    }

    /// Sends one `DismissRequest`, from the peer's own worker.
    ///
    /// Two fields and no third. There is no action index, no intent, no reply
    /// and no free text here **because the message has no field that could
    /// carry one** — the guarantee is the shape of `DismissRequest`, not a
    /// check in this function that a later change could invert.
    ///
    /// A peer that has disconnected between the dismissal and this call gets
    /// nothing: a dismiss is dropped, never queued for a later session
    /// (ADR-0015 §6). By the time the phone is back, its own snapshot is the
    /// truth about what is in its shade.
    async fn send_dismiss(
        &self,
        slot: &Arc<PeerSlot>,
        id: &NotificationId,
        origin_device_id: &str,
    ) {
        let Some(outbound) = slot.outbound.read().await.clone() else {
            tracing::debug!(
                peer = %slot.peer.to_display_short(),
                "the session ended before a dismissal could be sent; it is dropped, not queued"
            );
            return;
        };

        let control = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Dismiss(
                pb::DismissRequest {
                    notification_id: id.as_bytes().to_vec(),
                    origin_device_id: origin_device_id.to_string(),
                },
            )),
        };
        let message = OutboundMessage {
            capability_id: CAPABILITY_ID.to_string(),
            payload: control.encode_to_vec(),
        };

        match tokio::time::timeout(limits::BACKEND_TIMEOUT, outbound.send(message)).await {
            Ok(Ok(())) => {
                slot.state.lock().await.dismissals_sent += 1;
                tracing::info!(
                    peer = %slot.peer.to_display_short(),
                    notification = %redact::id_prefix(id),
                    "a human dismissed a mirror; asked the source to dismiss it too"
                );
            }
            Ok(Err(_)) => {
                tracing::debug!("the session ended before the dismissal was queued")
            }
            Err(_) => tracing::warn!("timed out queueing a dismissal request"),
        }
    }

    /// Builds what the backend will display, from what policy allows.
    ///
    /// Everything the peer sent that policy withholds is **absent from the
    /// result**, not present-and-flagged. There is no downstream step that
    /// could forget to apply a flag, because there is no flag.
    fn build_mirror(&self, message: &pb::NotificationUpsert, presentation: Presentation) -> Mirror {
        let app_name = text::app_name(&message.app_label, &message.app_id);
        let urgency = urgency_for(message.importance);

        match presentation {
            Presentation::Full => {
                let (title, body) =
                    text::presented(&message.title, &message.body, self.capabilities.body_markup);
                Mirror {
                    // A notification with no title is ordinary on Android. The
                    // application's name is the honest headline for one, and
                    // an empty summary would render as a notification with no
                    // visible identity at all.
                    summary: if title.is_empty() {
                        app_name.clone()
                    } else {
                        title
                    },
                    body: if self.capabilities.body {
                        body
                    } else {
                        String::new()
                    },
                    app_name,
                    urgency,
                    redacted: message.redacted,
                }
            }
            // `Suppress` never reaches here — it is refused before a mirror is
            // built — and mapping it to the reduced form rather than to the
            // full one keeps that true even if a future caller forgets.
            Presentation::AppOnly | Presentation::Suppress => Mirror {
                summary: app_name.clone(),
                body: String::new(),
                app_name,
                urgency,
                redacted: true,
            },
        }
    }
}

/// Android importance onto the freedesktop urgency hint.
///
/// `HIGH` maps to **normal**, not to critical. On GNOME a critical
/// notification does not auto-dismiss, and `IMPORTANCE_HIGH` is what every
/// chat application sets — so mapping it up would turn each message into a
/// banner the user has to click away. A mirror must never be more intrusive
/// than the notification it mirrors. An unknown value resolves to `NORMAL`
/// through the portable contract, never to something louder.
fn urgency_for(importance: i32) -> Urgency {
    match contract::importance_or_default(importance) {
        pb::NotificationImportance::Low => Urgency::Low,
        _ => Urgency::Normal,
    }
}

/// Whether a peer's verdict says a dismissal this desktop asked for did not
/// happen.
///
/// # Why no correlation state is needed
///
/// Every `NotificationResult` that reaches this desktop is a verdict on a
/// `DismissRequest`, because a `DismissRequest` is the **only** answerable
/// message this device ever sends. It is sink-only: it sends role
/// announcements (which are not answered), results (answering an answer is
/// refused on both sides, or two peers would build a loop out of nothing), and
/// dismissals. So attributing a result needs no table of outstanding ids —
/// which is exactly the dismiss event journal the design forbids.
///
/// Two outcomes are not refusals:
///
/// * `REMOVED` — the source dismissed it, which is what was asked;
/// * `UNKNOWN_NOTIFICATION` — it was already gone there. Both ends converging
///   on "it is gone" is the correct result, and calling it a refusal would put
///   a number in front of a user that says their phone said no when it did not.
///
/// Everything else is the source declining: its own dismiss-sync policy is
/// off, the notification is ongoing or not clearable, the grant was withdrawn,
/// or the request was refused for a reason this build does not know.
fn is_declined_dismissal(outcome: pb::NotificationOutcome) -> bool {
    !matches!(
        outcome,
        pb::NotificationOutcome::Removed | pb::NotificationOutcome::UnknownNotification
    )
}

/// The `notification_id` a control message names, if it names one.
fn named_identity(control: &pb::NotificationControl) -> Option<&[u8]> {
    match control.body.as_ref()? {
        pb::notification_control::Body::Upsert(m) => Some(&m.notification_id),
        pb::notification_control::Body::Remove(m) => Some(&m.notification_id),
        pb::notification_control::Body::Dismiss(m) => Some(&m.notification_id),
        pb::notification_control::Body::Result(m) => Some(&m.notification_id),
        pb::notification_control::Body::Roles(_) | pb::notification_control::Body::Sync(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Platform event pumps
// ---------------------------------------------------------------------------

impl NotificationManager {
    /// Starts the tasks that turn platform signals into capability state.
    ///
    /// Three streams, all event-driven and none of them polled (ADR-0014):
    /// the desktop closing a notification, the notification server appearing
    /// or disappearing, and the session locking or unlocking. An
    /// implementation that cannot report one of them returns `None` and the
    /// corresponding behaviour degrades in the documented direction — a sink
    /// that cannot see closes keeps a stale mapping until the next update
    /// replaces it, and a lock source that cannot see changes applies the lock
    /// policy only to notifications that arrive after the lock.
    ///
    /// Each stream is handed over once rather than cloned, so calling this
    /// twice starts nothing the second time.
    pub fn spawn_platform_pumps(self: &Arc<Self>) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = Vec::new();

        match self.sink.closed_events() {
            Some(mut closes) => {
                self.close_stream_missing.store(false, Ordering::Release);
                let manager = Arc::clone(self);
                handles.push(tokio::spawn(async move {
                    while let Some(closed) = closes.recv().await {
                        manager.note_closed(closed).await;
                    }
                }));
            }
            None => {
                // Only interesting when the backend said it could: a backend
                // that never claimed the capability is simply a sink, and
                // that is an ordinary, documented configuration.
                if self.capabilities.dismiss_reporting {
                    tracing::warn!(
                        backend = %self.sink.id(),
                        "the notification server advertises close reporting \
                         but handed over no close stream; this desktop will \
                         not announce DISMISS_REPORTER"
                    );
                    self.close_stream_missing.store(true, Ordering::Release);
                }
            }
        }

        if let Some(mut availability) = self.sink.availability_events() {
            let manager = Arc::clone(self);
            handles.push(tokio::spawn(async move {
                while let Some(available) = availability.recv().await {
                    manager.set_available(available).await;
                }
            }));
        }

        if let Some(mut locks) = self.lock.lock_events() {
            let manager = Arc::clone(self);
            handles.push(tokio::spawn(async move {
                while let Some(locked) = locks.recv().await {
                    manager.set_locked(locked).await;
                }
            }));
        }

        handles
    }
}

// ---------------------------------------------------------------------------
// The capability
// ---------------------------------------------------------------------------

/// The `notifications.v1` handler.
///
/// Thin on purpose, exactly as `clipboard.v1`'s and `files.v1`'s are: it
/// decodes nothing and decides nothing beyond routing, so every policy
/// question has one answer in one place.
pub struct NotificationsCapability {
    manager: Arc<NotificationManager>,
}

impl NotificationsCapability {
    pub fn new(manager: Arc<NotificationManager>) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> Arc<NotificationManager> {
        Arc::clone(&self.manager)
    }
}

#[async_trait::async_trait]
impl Capability for NotificationsCapability {
    fn id(&self) -> &str {
        CAPABILITY_ID
    }

    async fn on_peer_connected(&self, ctx: &CapabilityContext) -> Result<()> {
        self.manager
            .attach_session(ctx.peer, ctx.outbound.clone())
            .await;
        Ok(())
    }

    async fn on_message(&self, ctx: &CapabilityContext, payload: &[u8]) -> Result<()> {
        self.manager.handle_control(ctx.peer, payload).await
    }

    async fn on_peer_disconnected(&self, peer: &Fingerprint) -> Result<()> {
        self.manager.detach_session(peer).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_importance_never_becomes_critical() {
        // The variant does not exist, so this is really a test that the
        // mapping does not reach for something louder than normal.
        assert_eq!(
            urgency_for(pb::NotificationImportance::High as i32),
            Urgency::Normal
        );
        assert_eq!(
            urgency_for(pb::NotificationImportance::Normal as i32),
            Urgency::Normal
        );
        assert_eq!(
            urgency_for(pb::NotificationImportance::Low as i32),
            Urgency::Low
        );
    }

    #[test]
    fn an_unknown_importance_resolves_to_normal_not_to_loud() {
        assert_eq!(urgency_for(9999), Urgency::Normal);
        assert_eq!(urgency_for(-1), Urgency::Normal);
        assert_eq!(
            urgency_for(pb::NotificationImportance::Unspecified as i32),
            Urgency::Normal
        );
    }

    #[test]
    fn an_unlocked_session_presents_everything_whatever_the_lock_policy_says() {
        for policy in [LockPolicy::Full, LockPolicy::AppOnly, LockPolicy::Suppress] {
            let p = NotificationPolicy {
                when_sink_locked: policy,
                ..NotificationPolicy::default()
            };
            assert_eq!(Presentation::resolve(&p, false), Presentation::Full);
        }
    }

    #[test]
    fn a_locked_session_follows_the_policy() {
        let cases = [
            (LockPolicy::Full, Presentation::Full),
            (LockPolicy::AppOnly, Presentation::AppOnly),
            (LockPolicy::Suppress, Presentation::Suppress),
        ];
        for (policy, expected) in cases {
            let p = NotificationPolicy {
                when_sink_locked: policy,
                ..NotificationPolicy::default()
            };
            assert_eq!(Presentation::resolve(&p, true), expected);
        }
    }

    #[test]
    fn a_denied_policy_suppresses_even_before_the_grant_is_consulted() {
        // `DENIED` sets `when_sink_locked` to `Suppress` as well as clearing
        // `allow_mirror`, so a caller that read only the lock policy still
        // reaches the safe answer.
        assert_eq!(
            Presentation::resolve(&NotificationPolicy::DENIED, true),
            Presentation::Suppress
        );
    }

    #[test]
    fn only_the_messages_that_name_an_identity_can_be_answered() {
        let roles = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Roles(
                pb::NotificationRoles::default(),
            )),
        };
        assert!(named_identity(&roles).is_none());

        let sync = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Sync(
                pb::SyncMarker::default(),
            )),
        };
        assert!(named_identity(&sync).is_none());

        let upsert = pb::NotificationControl {
            body: Some(pb::notification_control::Body::Upsert(
                pb::NotificationUpsert {
                    notification_id: vec![1; 16],
                    ..pb::NotificationUpsert::default()
                },
            )),
        };
        assert_eq!(named_identity(&upsert), Some(&[1u8; 16][..]));
    }

    #[test]
    fn every_outcome_has_a_wire_value_and_a_name() {
        for outcome in [
            Outcome::Displayed,
            Outcome::Removed,
            Outcome::Duplicate,
            Outcome::NotAuthorized,
            Outcome::RejectedPolicy,
            Outcome::RejectedRole,
            Outcome::UnknownNotification,
            Outcome::TooLarge,
            Outcome::Invalid,
            Outcome::RateLimited,
            Outcome::Unavailable,
            Outcome::Failed,
        ] {
            assert_ne!(
                outcome.to_proto(),
                pb::NotificationOutcome::Unspecified,
                "{outcome:?} must not map to UNSPECIFIED"
            );
            assert!(!outcome.as_str().is_empty());
        }
    }
}
