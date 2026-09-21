//! `clipboard.v1` — text clipboard sharing between a phone and a desktop.
//!
//! # One channel
//!
//! Everything here travels as the opaque `payload` of a `CapabilityMessage`
//! on the existing control session. There is no second socket, no second
//! listener, and no data stream: a clipboard is small by nature, so the
//! `files.v1` split would buy nothing and would cost a second authenticated
//! connection to reason about. The ceiling that keeps this honest is
//! [`limits::MAX_CLIPBOARD_TEXT_BYTES`], set well below `MAX_FRAME_LEN`.
//!
//! # What this capability is careful about, and where that lives
//!
//! | Property | Enforced by |
//! | --- | --- |
//! | only a paired device can speak at all | TLS 1.3 + SPKI pinning (`omnibridge_core::tls`) |
//! | only an explicitly *granted* device may use the clipboard | [`ClipboardAuthorizer`], re-asked per message |
//! | direction and automation are separate from the grant | [`ClipboardPolicy`] |
//! | a peer cannot widen its own policy | there is no protocol message that sets one |
//! | applying a remote clip does not echo it back | [`dedup::SuppressionCache`] |
//! | a remote clip is never forwarded to a third device | no code path does it — see [`ClipboardManager::on_local_change`] |
//! | the same event is never applied twice | [`dedup::EventCache`] |
//! | text is text, and bounded | [`text::ClipboardText`] |
//! | content never reaches a log, a store or a `Debug` | [`redact`], and `ClipboardText`'s hand-written `Debug` |
//!
//! # What is never here
//!
//! No clipboard history, no persistence of content, no image or file
//! clipboard, no relay, and no path that writes clipboard text anywhere
//! except the system clipboard of the machine it belongs to.

pub mod backend;
pub mod dedup;
pub mod limits;
pub mod policy;
pub mod redact;
pub mod text;

use std::collections::HashMap;
use std::sync::Arc;

use rand::TryRngCore;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::Instant;

use omnibridge_core::capability::{Capability, CapabilityContext, OutboundMessage};
use omnibridge_core::error::{Error, Result};
use omnibridge_core::Fingerprint;
use omnibridge_proto::v1::capabilities as pb;
use omnibridge_proto::Message;

use backend::{BackendError, ClipboardBackend};
use dedup::{EventCache, SuppressionCache};
use limits::*;
pub use policy::{ClipboardAuthorizer, ClipboardPolicy};
pub use text::{ClipboardText, TextRejection};

pub const CAPABILITY_ID: &str = "clipboard.v1";

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// What happened to one inbound update. Mirrors the protobuf enum, so the
/// wire vocabulary and the internal one cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    PendingUser,
    Duplicate,
    NotAuthorized,
    RejectedPolicy,
    RejectedSensitive,
    TooLarge,
    InvalidText,
    Failed,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::PendingUser => "pending",
            Self::Duplicate => "duplicate",
            Self::NotAuthorized => "not authorized",
            Self::RejectedPolicy => "rejected by policy",
            Self::RejectedSensitive => "rejected as sensitive",
            Self::TooLarge => "too large",
            Self::InvalidText => "invalid text",
            Self::Failed => "failed",
        }
    }

    fn to_proto(self) -> pb::ClipboardOutcome {
        match self {
            Self::Applied => pb::ClipboardOutcome::Applied,
            Self::PendingUser => pb::ClipboardOutcome::PendingUser,
            Self::Duplicate => pb::ClipboardOutcome::Duplicate,
            Self::NotAuthorized => pb::ClipboardOutcome::NotAuthorized,
            Self::RejectedPolicy => pb::ClipboardOutcome::RejectedPolicy,
            Self::RejectedSensitive => pb::ClipboardOutcome::RejectedSensitive,
            Self::TooLarge => pb::ClipboardOutcome::TooLarge,
            Self::InvalidText => pb::ClipboardOutcome::InvalidText,
            Self::Failed => pb::ClipboardOutcome::Failed,
        }
    }

    fn from_proto(value: pb::ClipboardOutcome) -> Self {
        match value {
            pb::ClipboardOutcome::Applied => Self::Applied,
            pb::ClipboardOutcome::PendingUser => Self::PendingUser,
            pb::ClipboardOutcome::Duplicate => Self::Duplicate,
            pb::ClipboardOutcome::NotAuthorized => Self::NotAuthorized,
            pb::ClipboardOutcome::RejectedPolicy => Self::RejectedPolicy,
            pb::ClipboardOutcome::RejectedSensitive => Self::RejectedSensitive,
            pb::ClipboardOutcome::TooLarge => Self::TooLarge,
            pb::ClipboardOutcome::InvalidText => Self::InvalidText,
            pb::ClipboardOutcome::Unspecified | pb::ClipboardOutcome::Failed => Self::Failed,
        }
    }
}

/// Why a local send did not happen. Local-only: never sent to a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    /// No `clipboard.v1` grant, or `allow_send` is off for this peer.
    NotPermitted,
    /// The peer has no live session.
    NotConnected,
    /// The clipboard is empty, or holds something that is not text.
    NothingToSend,
    /// The local clipboard text is not usable — oversized, or NUL-bearing.
    Rejected(TextRejection),
    /// The platform clipboard could not be read.
    Backend(BackendError),
    /// The session's outbound queue did not accept the message in time.
    SessionBusy,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPermitted => f.write_str(
                "this device is not allowed to send clipboard text to that peer. \
                 Grant it with `omnibridge grant <device> clipboard.v1`.",
            ),
            Self::NotConnected => f.write_str("that device is not currently connected"),
            Self::NothingToSend => f.write_str("the clipboard is empty, or does not contain text"),
            Self::Rejected(r) => write!(f, "{r}"),
            Self::Backend(e) => write!(f, "{e}"),
            Self::SessionBusy => f.write_str("the session did not accept the message in time"),
        }
    }
}

/// A clip accepted but not yet applied, because `auto_receive` is off.
///
/// Held in memory, bounded to one per peer, and dropped after
/// [`PENDING_CLIP_TTL`]. It is the desktop's equivalent of Android's
/// "Clipboard received — Copy" notification.
struct PendingClip {
    text: ClipboardText,
    sensitive: bool,
    origin_device_id: String,
    received_at: Instant,
}

/// A pending clip, described without its content. Safe to print.
#[derive(Debug, Clone)]
pub struct PendingClipInfo {
    pub peer: Fingerprint,
    pub bytes: usize,
    pub hash_prefix: String,
    pub sensitive: bool,
    pub origin_device_id: String,
    pub age_secs: u64,
}

// ---------------------------------------------------------------------------
// The manager
// ---------------------------------------------------------------------------

/// All `clipboard.v1` state and policy for this device.
///
/// One instance per daemon. It owns the platform backend, the two caches, the
/// per-peer session senders, and the supervised watcher.
pub struct ClipboardManager {
    backend: Arc<dyn ClipboardBackend>,
    authorizer: RwLock<Option<Arc<dyn ClipboardAuthorizer>>>,

    /// This device's own id, stamped into outbound updates as
    /// `origin_device_id`.
    local_device_id: String,

    /// Handled event ids. Global rather than per-peer, so that replaying one
    /// peer's event through another peer is also caught.
    events: Mutex<EventCache>,
    /// Content this device wrote on a peer's behalf, awaiting its own echo.
    suppression: Mutex<SuppressionCache>,

    /// Live control sessions, for sends this device initiates.
    sessions: RwLock<HashMap<Fingerprint, mpsc::Sender<OutboundMessage>>>,

    /// Accepted-but-unapplied clips, one per peer.
    pending: Mutex<HashMap<Fingerprint, PendingClip>>,

    /// Bumped whenever a grant or a policy changes, so the watcher can start
    /// or stop without polling the trust store.
    policy_epoch: tokio::sync::watch::Sender<u64>,

    /// The most recent outcome reported by a peer, for the CLI. No content.
    last_results: Mutex<HashMap<Fingerprint, Outcome>>,
}

impl ClipboardManager {
    pub fn new(
        backend: Arc<dyn ClipboardBackend>,
        local_device_id: impl Into<String>,
    ) -> Arc<Self> {
        let (policy_epoch, _) = tokio::sync::watch::channel(0);
        Arc::new(Self {
            backend,
            authorizer: RwLock::new(None),
            local_device_id: local_device_id.into(),
            events: Mutex::new(EventCache::default()),
            suppression: Mutex::new(SuppressionCache::default()),
            sessions: RwLock::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            policy_epoch,
            last_results: Mutex::new(HashMap::new()),
        })
    }

    /// Wires the grant/policy source.
    ///
    /// Separate from [`new`] for the same reason `files.v1`'s is: the daemon
    /// state is the authorizer and it needs the manager, so one of the two
    /// has to exist first.
    ///
    /// [`new`]: Self::new
    pub async fn set_authorizer(&self, authorizer: Arc<dyn ClipboardAuthorizer>) {
        *self.authorizer.write().await = Some(authorizer);
        self.policy_changed();
    }

    pub fn backend(&self) -> &Arc<dyn ClipboardBackend> {
        &self.backend
    }

    /// Tells the watcher that a grant or policy may have changed.
    ///
    /// Called by the daemon after every grant, revocation and policy edit.
    /// This is what makes "revoking stops future sync immediately" true for
    /// the *outbound* direction as well as the inbound one: the watcher
    /// re-reads the peer list and stops entirely when nobody wants it.
    pub fn policy_changed(&self) {
        self.policy_epoch.send_modify(|epoch| *epoch += 1);
    }

    async fn policy_for(&self, peer: &Fingerprint) -> ClipboardPolicy {
        match self.authorizer.read().await.as_ref() {
            Some(authorizer) => authorizer.policy_for(peer).await,
            // No authorizer wired is a programming error, not a peer's doing.
            // Failing closed is the only safe reading of "we cannot tell".
            None => ClipboardPolicy::DENIED,
        }
    }

    async fn auto_send_peers(&self) -> Vec<Fingerprint> {
        match self.authorizer.read().await.as_ref() {
            Some(authorizer) => authorizer.auto_send_peers().await,
            None => Vec::new(),
        }
    }

    /// Records the outbound channel for a peer's live session.
    pub async fn attach_session(&self, peer: Fingerprint, session: mpsc::Sender<OutboundMessage>) {
        self.sessions.write().await.insert(peer, session);
    }

    /// Forgets a peer's session and everything held on its behalf.
    ///
    /// The pending clip goes with it: a clip that a disconnected device sent,
    /// which the user never applied, has no reason to stay in memory.
    pub async fn detach_session(&self, peer: &Fingerprint) {
        self.sessions.write().await.remove(peer);
        self.pending.lock().await.remove(peer);
        self.last_results.lock().await.remove(peer);
    }

    // -----------------------------------------------------------------------
    // Inbound
    // -----------------------------------------------------------------------

    /// Handles one decoded `clipboard.v1` payload from `peer`.
    ///
    /// Returns the reply to send, if any. A `ClipboardResult` never produces
    /// a reply of its own — answering an answer is how two peers build a
    /// message loop out of nothing.
    pub async fn handle_control(
        &self,
        peer: Fingerprint,
        peer_device_id: &str,
        payload: &[u8],
    ) -> Result<Option<OutboundMessage>> {
        let control = pb::ClipboardControl::decode(payload)
            .map_err(|_| Error::Protocol("malformed clipboard.v1 payload"))?;

        match control.body {
            Some(pb::clipboard_control::Body::Update(update)) => {
                let event_id = update.event_id.clone();
                let outcome = self.handle_update(peer, peer_device_id, update).await;

                tracing::info!(
                    peer = %peer.to_display_short(),
                    event = %redact::event_prefix(&event_id),
                    outcome = outcome.as_str(),
                    "clipboard update"
                );

                // An update whose event id is unusable cannot be answered:
                // there is nothing to correlate the answer with, and echoing
                // an attacker-chosen id back is not useful either.
                if event_id.len() != EVENT_ID_LEN {
                    return Ok(None);
                }
                Ok(Some(encode_result(&event_id, outcome)))
            }

            Some(pb::clipboard_control::Body::Result(result)) => {
                let outcome = Outcome::from_proto(
                    pb::ClipboardOutcome::try_from(result.outcome)
                        .unwrap_or(pb::ClipboardOutcome::Unspecified),
                );
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    event = %redact::event_prefix(&result.event_id),
                    outcome = outcome.as_str(),
                    "peer reported a clipboard outcome"
                );
                self.last_results.lock().await.insert(peer, outcome);
                Ok(None)
            }

            // A body this build does not know, or none at all. Not fatal:
            // a newer peer may add one, and the session must survive it.
            None => Ok(None),
        }
    }

    /// The decision path for one inbound update.
    ///
    /// The order of the checks is deliberate. Authorization comes before
    /// anything is parsed for meaning, and de-duplication comes *after*
    /// validation so that a malformed replay cannot consume a cache slot.
    async fn handle_update(
        &self,
        peer: Fingerprint,
        peer_device_id: &str,
        update: pb::ClipboardUpdate,
    ) -> Outcome {
        // 1. Grant. Asked fresh, so a revocation from a second ago is already
        //    in force on a session that is already established.
        let policy = self.policy_for(&peer).await;
        if policy.is_denied() {
            return Outcome::NotAuthorized;
        }

        // 2. Direction.
        if !policy.allow_receive {
            return Outcome::RejectedPolicy;
        }

        // 3. Shape. An id of the wrong length cannot be de-duplicated, so it
        //    is refused rather than normalised.
        if update.event_id.len() != EVENT_ID_LEN {
            return Outcome::InvalidText;
        }

        // 4. Size, before anything is copied anywhere. `text_utf8` is already
        //    valid UTF-8 — protobuf refuses to decode a `string` field that
        //    is not — so the encoding check on the wire is structural, and
        //    what remains is our own rules.
        if update.text_utf8.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return Outcome::TooLarge;
        }

        let text = match ClipboardText::validate(update.text_utf8) {
            Ok(t) => t,
            Err(TextRejection::TooLarge { .. }) => return Outcome::TooLarge,
            Err(_) => return Outcome::InvalidText,
        };

        // 5. The hash is a consistency check on a peer, not authentication:
        //    a mismatch means the sender is broken or is trying something,
        //    and either way the pair (hash, content) it asked us to remember
        //    would poison de-duplication. Compared with `!=` rather than in
        //    constant time on purpose — there is no secret here, and
        //    pretending otherwise would suggest the hash carries authority
        //    it does not have.
        if !update.content_hash.is_empty() && update.content_hash.as_slice() != text.hash() {
            return Outcome::InvalidText;
        }

        // 6. De-duplication and replay. Idempotent by construction: a repeat
        //    is answered coherently and nothing else happens.
        if !self.events.lock().await.admit(&update.event_id) {
            return Outcome::Duplicate;
        }

        // 7. Automation. With `auto_receive` off the clip is accepted but not
        //    written, so a paired-but-hostile device cannot replace what the
        //    user is about to paste.
        let origin = if update.origin_device_id.is_empty() {
            peer_device_id.to_string()
        } else {
            omnibridge_core::discovery::sanitize_device_name(&update.origin_device_id)
        };

        if !policy.may_auto_receive() {
            let mut pending = self.pending.lock().await;
            pending.insert(
                peer,
                PendingClip {
                    text,
                    sensitive: update.sensitive_hint,
                    origin_device_id: origin,
                    received_at: Instant::now(),
                },
            );
            return Outcome::PendingUser;
        }

        self.apply(&text, update.sensitive_hint, &origin).await
    }

    /// Writes a remote clip to the local clipboard, arming loop suppression
    /// first.
    ///
    /// The order matters and is the whole mechanism: the suppression entry
    /// must exist *before* the write, because on a healthy desktop the change
    /// notification can arrive while `write_text` is still returning.
    async fn apply(&self, text: &ClipboardText, sensitive: bool, origin: &str) -> Outcome {
        self.suppression.lock().await.arm(*text.hash(), origin);

        match self.backend.write_text(text, sensitive).await {
            Ok(()) => Outcome::Applied,
            Err(e) => {
                // The write did not happen, so the echo will not either.
                // Releasing the entry keeps a genuine local copy of the same
                // text from being swallowed later.
                self.suppression.lock().await.take(text.hash());
                tracing::warn!(error = %e, "could not apply a clipboard update");
                Outcome::Failed
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pending clips
    // -----------------------------------------------------------------------

    /// Every clip waiting for a human, described without its content.
    pub async fn pending_clips(&self) -> Vec<PendingClipInfo> {
        let mut pending = self.pending.lock().await;
        let now = Instant::now();
        pending.retain(|_, clip| now.duration_since(clip.received_at) < PENDING_CLIP_TTL);
        pending
            .iter()
            .map(|(peer, clip)| PendingClipInfo {
                peer: *peer,
                bytes: clip.text.len(),
                hash_prefix: redact::hash_prefix(clip.text.hash()),
                sensitive: clip.sensitive,
                origin_device_id: clip.origin_device_id.clone(),
                age_secs: now.duration_since(clip.received_at).as_secs(),
            })
            .collect()
    }

    /// Applies the clip a peer sent while `auto_receive` was off.
    ///
    /// The grant is re-checked here, not just when the clip arrived: a clip
    /// accepted a minute ago from a device that has since been revoked must
    /// not still be applicable.
    pub async fn apply_pending(&self, peer: &Fingerprint) -> std::result::Result<usize, SendError> {
        let policy = self.policy_for(peer).await;
        if !policy.allow_receive {
            return Err(SendError::NotPermitted);
        }

        // Taken out under the lock, and put back if the write fails. Removing
        // it up front and only *then* trying is the obvious shape and it is
        // wrong: the most likely failure here is a locked screen, which is
        // transient — and discarding the clip because of it would mean the
        // person has to go back to the other device and send it again. Found
        // on hardware, doing exactly that.
        let clip = {
            let mut pending = self.pending.lock().await;
            match pending.remove(peer) {
                Some(clip)
                    if Instant::now().duration_since(clip.received_at) < PENDING_CLIP_TTL =>
                {
                    clip
                }
                // Absent or expired are the same answer to a caller.
                _ => return Err(SendError::NothingToSend),
            }
        };

        let bytes = clip.text.len();

        // `apply` reports an outcome, which is the vocabulary a *peer* gets.
        // A local caller deserves better: the backend's own error says
        // whether the session is locked, whether wl-clipboard is missing, or
        // whether something else went wrong — and that is the difference
        // between "unlock your screen" and a shrug. So the write is repeated
        // here rather than being reduced to a yes/no.
        self.suppression
            .lock()
            .await
            .arm(*clip.text.hash(), &clip.origin_device_id);
        match self.backend.write_text(&clip.text, clip.sensitive).await {
            Ok(()) => Ok(bytes),
            Err(e) => {
                // Nothing was written, so no echo is coming; releasing the
                // entry keeps a later local copy of the same text sendable.
                self.suppression.lock().await.take(clip.text.hash());
                tracing::warn!(error = %e, "could not apply a held clipboard clip");
                // Put it back, so the person can simply try again once the
                // screen is unlocked. Its original `received_at` is kept, so
                // this cannot extend a clip's lifetime past its TTL by
                // retrying.
                self.pending.lock().await.insert(*peer, clip);
                self.publish_pending().await;
                Err(SendError::Backend(e))
            }
        }
    }

    /// Re-publishes the pending list after a mutation.
    async fn publish_pending(&self) {
        // `pending_clips` already expires and describes; calling it keeps the
        // one definition of "what is pending" in one place.
        let _ = self.pending_clips().await;
    }

    // -----------------------------------------------------------------------
    // Outbound
    // -----------------------------------------------------------------------

    /// Sends the current local clipboard to one peer, by explicit request.
    ///
    /// This is the manual path — `omnibridge clipboard send <device>` — and it
    /// is deliberately not gated on `auto_send`: a human asking is a
    /// different act from a watcher firing, and only the second one needs the
    /// automatic opt-in.
    pub async fn send_current_clipboard(
        &self,
        peer: &Fingerprint,
        sensitive: bool,
    ) -> std::result::Result<usize, SendError> {
        let policy = self.policy_for(peer).await;
        if !policy.allow_send {
            return Err(SendError::NotPermitted);
        }

        let text = match self.backend.read_text().await {
            Ok(Some(text)) => text,
            Ok(None) => return Err(SendError::NothingToSend),
            Err(e) => return Err(SendError::Backend(e)),
        };

        let bytes = text.len();
        let event = self.new_event(text, sensitive);
        self.deliver(peer, &event).await?;
        Ok(bytes)
    }

    /// Sends explicit text to one peer, bypassing the local clipboard.
    ///
    /// Used by tests and by any caller that already holds the text. The
    /// policy check is identical: nothing skips it.
    pub async fn send_text(
        &self,
        peer: &Fingerprint,
        text: ClipboardText,
        sensitive: bool,
    ) -> std::result::Result<usize, SendError> {
        let policy = self.policy_for(peer).await;
        if !policy.allow_send {
            return Err(SendError::NotPermitted);
        }
        let bytes = text.len();
        let event = self.new_event(text, sensitive);
        self.deliver(peer, &event).await?;
        Ok(bytes)
    }

    /// Builds one clipboard event.
    fn new_event(&self, text: ClipboardText, sensitive: bool) -> LocalEvent {
        LocalEvent {
            event_id: random_event_id(),
            text,
            sensitive,
            origin_device_id: self.local_device_id.clone(),
        }
    }

    /// Puts one event on one peer's session.
    async fn deliver(
        &self,
        peer: &Fingerprint,
        event: &LocalEvent,
    ) -> std::result::Result<(), SendError> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(peer).cloned()
        };
        let Some(session) = session else {
            return Err(SendError::NotConnected);
        };

        let message = OutboundMessage {
            capability_id: CAPABILITY_ID.to_string(),
            payload: encode_update(event).encode_to_vec(),
        };

        // Bounded, for the reason `CONTROL_SEND_TIMEOUT` documents: a peer
        // that has stopped reading must not be able to park this call — which
        // may be running inside the session's own dispatch loop — for ever.
        match tokio::time::timeout(CONTROL_SEND_TIMEOUT, session.send(message)).await {
            Ok(Ok(())) => {
                tracing::info!(
                    peer = %peer.to_display_short(),
                    event = %redact::event_prefix(&event.event_id),
                    bytes = event.text.len(),
                    sensitive = event.sensitive,
                    "clipboard update sent"
                );
                Ok(())
            }
            Ok(Err(_)) => Err(SendError::NotConnected),
            Err(_) => Err(SendError::SessionBusy),
        }
    }

    /// The local clipboard changed.
    ///
    /// # No relay, by absence
    ///
    /// This is the only function that turns a clipboard change into outbound
    /// traffic, and its input is the *local* clipboard — never an inbound
    /// message. There is deliberately no code path anywhere in this crate
    /// that takes a `ClipboardUpdate` from peer A and sends it to peer B: the
    /// no-relay rule is not a check that could be inverted by a flag, it is a
    /// function that does not exist.
    ///
    /// What could still create a relay is the loop through the clipboard
    /// itself — A's clip is written locally, the watcher sees a change, and
    /// it goes out to B. That is what the suppression cache stops, and the
    /// check happens here, before any peer is considered.
    pub async fn on_local_change(&self) {
        let text = match self.backend.read_text().await {
            Ok(Some(text)) => text,
            // Empty, or not text. Not every clipboard change is a clip we can
            // carry, and that is not an error.
            Ok(None) => return,
            Err(e) => {
                // One failed read must not kill the watcher: a locked screen
                // is temporary and the next change should still be seen.
                tracing::debug!(error = %e, "could not read the clipboard after a change");
                return;
            }
        };

        if let Some(origin) = self.suppression.lock().await.take(text.hash()) {
            tracing::debug!(
                origin = %origin,
                hash = %redact::hash_prefix(text.hash()),
                "suppressed the echo of a clip applied from a peer"
            );
            return;
        }

        let peers = self.auto_send_peers().await;
        if peers.is_empty() {
            return;
        }

        // One event id for one clipboard event, shared across peers: two
        // devices receiving the same clip should agree that it is the same
        // clip, which is what makes de-duplication work if they also talk to
        // each other.
        let event = self.new_event(text, false);
        for peer in peers {
            if let Err(e) = self.deliver(&peer, &event).await {
                tracing::debug!(
                    peer = %peer.to_display_short(),
                    error = %e,
                    "could not push the clipboard to a peer"
                );
            }
        }
    }

    /// The last outcome each peer reported, for `omnibridge clipboard status`.
    pub async fn last_results(&self) -> HashMap<Fingerprint, Outcome> {
        self.last_results.lock().await.clone()
    }

    /// Cache sizes, for the bounded-growth test and for diagnostics.
    pub async fn cache_sizes(&self) -> (usize, usize) {
        let events = self.events.lock().await.len();
        let suppression = self.suppression.lock().await.len();
        (events, suppression)
    }

    // -----------------------------------------------------------------------
    // The watcher
    // -----------------------------------------------------------------------

    /// Runs the local clipboard watcher for as long as some peer wants it.
    ///
    /// Supervision, in one place:
    ///
    /// * **It starts only when needed.** With no `auto_send` peer there is no
    ///   watcher, no child process and no X connection — the daemon is doing
    ///   nothing rather than watching for nobody.
    /// * **It stops when the last one goes.** A revocation bumps the policy
    ///   epoch, the loop re-reads, and the watch is dropped.
    /// * **It restarts if the backend dies**, with exponential backoff, so a
    ///   compositor restart is survivable and a permanently broken backend
    ///   costs one attempt a minute rather than a spin.
    /// * **It never polls.** Both sources are event-driven; when neither is
    ///   available the watcher waits for a policy change and reports why.
    /// * **It cannot kill the daemon.** Everything it touches is a `Result`
    ///   handled here.
    pub fn spawn_watcher(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager = Arc::clone(self);
        tokio::spawn(async move { manager.run_watcher().await })
    }

    async fn run_watcher(self: Arc<Self>) {
        let mut epoch = self.policy_epoch.subscribe();
        let mut backoff = WATCH_RESTART_MIN_BACKOFF;

        loop {
            // Idle until at least one peer wants automatic sending.
            while self.auto_send_peers().await.is_empty() {
                if epoch.changed().await.is_err() {
                    return; // the manager is gone
                }
            }

            let mut watch = match self.backend.watch_changes() {
                Ok(watch) => {
                    tracing::info!(
                        source = watch.source,
                        "clipboard auto-send is on; watching for local changes"
                    );
                    backoff = WATCH_RESTART_MIN_BACKOFF;
                    watch
                }
                Err(BackendError::Unavailable(why)) => {
                    // Not transient. Retrying on a timer would be a spin
                    // against a platform that has already answered, so wait
                    // for the situation to change instead.
                    tracing::warn!(
                        reason = %why,
                        "clipboard auto-send is enabled but this session cannot \
                         report clipboard changes; sending by hand still works"
                    );
                    if epoch.changed().await.is_err() {
                        return;
                    }
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        retry_in_secs = backoff.as_secs(),
                        "could not start the clipboard watcher"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(WATCH_RESTART_MAX_BACKOFF);
                    continue;
                }
            };

            loop {
                tokio::select! {
                    signal = watch.changes.recv() => match signal {
                        Some(()) => self.on_local_change().await,
                        None => {
                            // The backend ended: the helper died, the X
                            // connection dropped, the compositor restarted.
                            tracing::info!("the clipboard watcher stopped; restarting it");
                            break;
                        }
                    },

                    changed = epoch.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if self.auto_send_peers().await.is_empty() {
                            tracing::info!(
                                "no device wants clipboard auto-send any more; \
                                 stopping the watcher"
                            );
                            break;
                        }
                    }
                }
            }

            // Dropping the watch releases the child process or the X
            // connection before the next attempt, so a restart never leaves
            // two watchers running.
            drop(watch);
        }
    }
}

/// One locally originated clipboard event, before it is encoded per peer.
struct LocalEvent {
    event_id: Vec<u8>,
    text: ClipboardText,
    sensitive: bool,
    origin_device_id: String,
}

/// 16 cryptographically random bytes.
///
/// Not the envelope's `message_id`, and not derived from it: see the comment
/// on `ClipboardUpdate.event_id`. A CSPRNG failure returns an empty id, which
/// fails the length check at both ends rather than producing a predictable
/// one.
fn random_event_id() -> Vec<u8> {
    let mut buf = [0u8; EVENT_ID_LEN];
    if rand::rngs::OsRng.try_fill_bytes(&mut buf).is_err() {
        return Vec::new();
    }
    buf.to_vec()
}

fn encode_update(event: &LocalEvent) -> pb::ClipboardControl {
    pb::ClipboardControl {
        body: Some(pb::clipboard_control::Body::Update(pb::ClipboardUpdate {
            event_id: event.event_id.clone(),
            origin_device_id: event.origin_device_id.clone(),
            text_utf8: event.text.as_str().to_string(),
            content_hash: event.text.hash().to_vec(),
            sensitive_hint: event.sensitive,
            timestamp_unix_ms: now_unix_ms(),
        })),
    }
}

fn encode_result(event_id: &[u8], outcome: Outcome) -> OutboundMessage {
    let control = pb::ClipboardControl {
        body: Some(pb::clipboard_control::Body::Result(pb::ClipboardResult {
            event_id: event_id.to_vec(),
            outcome: outcome.to_proto() as i32,
        })),
    };
    OutboundMessage {
        capability_id: CAPABILITY_ID.to_string(),
        payload: control.encode_to_vec(),
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The capability
// ---------------------------------------------------------------------------

/// The `clipboard.v1` handler.
///
/// Thin on purpose, exactly as `files.v1`'s is: it decodes nothing and
/// decides nothing beyond routing, so every policy question has one answer in
/// one place.
pub struct ClipboardCapability {
    manager: Arc<ClipboardManager>,
}

impl ClipboardCapability {
    pub fn new(manager: Arc<ClipboardManager>) -> Self {
        Self { manager }
    }

    pub fn manager(&self) -> Arc<ClipboardManager> {
        Arc::clone(&self.manager)
    }
}

#[async_trait::async_trait]
impl Capability for ClipboardCapability {
    fn id(&self) -> &str {
        CAPABILITY_ID
    }

    async fn on_peer_connected(&self, ctx: &CapabilityContext) -> Result<()> {
        // Recorded so a send can be started from the CLI or the watcher,
        // rather than only in reply to an inbound message.
        self.manager
            .attach_session(ctx.peer, ctx.outbound.clone())
            .await;
        // A new session may be the one an auto-send peer was waiting for.
        self.manager.policy_changed();
        Ok(())
    }

    async fn on_message(&self, ctx: &CapabilityContext, payload: &[u8]) -> Result<()> {
        let reply = self
            .manager
            .handle_control(ctx.peer, &ctx.peer_device_id, payload)
            .await?;

        if let Some(reply) = reply {
            // The same bound as `deliver`, and for the same reason: this runs
            // inside the session's dispatch loop.
            match tokio::time::timeout(CONTROL_SEND_TIMEOUT, ctx.outbound.send(reply)).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => tracing::debug!("the session ended before the result was queued"),
                Err(_) => tracing::warn!("timed out queueing a clipboard result"),
            }
        }
        Ok(())
    }

    async fn on_peer_disconnected(&self, peer: &Fingerprint) -> Result<()> {
        self.manager.detach_session(peer).await;
        Ok(())
    }
}
