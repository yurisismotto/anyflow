//! Asking a human whether to accept an incoming file.
//!
//! `files.v1` already had the seam — [`TransferApproval`] — and a daemon that
//! could not reach a human already did the safe thing with it: decline, and
//! say so. What it had no implementation of was *reaching* one. The desktop
//! therefore had exactly two behaviours, neither of them a product:
//! refuse every file, or `--accept-files-without-asking` and refuse nothing.
//!
//! This is the missing middle. It is a rendezvous, not a policy:
//!
//! ```text
//!   files.v1  ──confirm_receive(offer)──>  FileApproval  ──offer──>  control
//!                                               │                    session
//!                                               │                      │
//!             <────────── true/false ───────────┴──── decide(id) ──────┘
//! ```
//!
//! # What it deliberately is not
//!
//! * **Not a trust decision.** Saying yes here approves one offer. It grants
//!   nothing, stores nothing, and is not remembered. There is no "always
//!   accept" in this sprint, by design.
//! * **Not a data plane.** The provider is told a filename and a size. Not
//!   one byte of the file, and not the stream challenge, ever passes through
//!   it — the answer it sends back is a single boolean.
//! * **Not a default.** With no provider attached and no explicit override,
//!   every offer is declined. The presence of a GUI *package* is not consent;
//!   the presence of an attached, answering provider is.
//!
//! # Binding a decision to an offer
//!
//! Pending offers are keyed by [`TransferId`] — 128 random bits, minted by
//! the sender and single-use — and a decision names one exactly, never a
//! prefix. That is what makes "a decision for offer A must never approve
//! offer B" structural rather than careful: there is no code path that
//! resolves an entry other than the one named. The peer fingerprint is
//! carried alongside and reported to the provider, so what a human is shown
//! is the authenticated identity rather than a display name a peer chose.
//!
//! # Fail-closed in every direction
//!
//! Every way of *not* getting an answer produces a decline, and each is a
//! different route to the same `false`:
//!
//! | What happens | How it declines |
//! | --- | --- |
//! | no provider attached | [`confirm_receive`] returns `false` immediately |
//! | provider's queue is full | the offer is never queued; `false` |
//! | provider detaches or crashes | its pending entries are dropped, so the `oneshot` closes |
//! | a second provider attaches | the first's entries are dropped the same way |
//! | the offer times out | `files.v1` drops the future; the guard removes the entry |
//! | the daemon exits | the process is gone, and nothing was persisted |
//!
//! [`confirm_receive`]: FileApproval::confirm_receive

use std::collections::BTreeMap;
use std::sync::Mutex;

use anyflow_capability_files::transfer::TransferId;
use anyflow_capability_files::{IncomingOffer, TransferApproval};
use anyflow_core::Fingerprint;
use tokio::sync::{mpsc, oneshot};

/// How many offers may be queued towards a provider before it is considered
/// not to be reading.
///
/// `files.v1` bounds a single peer to four concurrent transfers, so this is
/// eight peers' worth of simultaneous prompts. A queue that filled would mean
/// a provider that had stopped answering, and the safe reading of that is a
/// decline rather than an unbounded backlog of questions.
const PROVIDER_QUEUE: usize = 32;

/// Identifies one attachment of a provider.
///
/// A provider that reconnects gets a new epoch, so the session that is going
/// away cannot detach the one that replaced it — the same mistake
/// `unregister_session` avoids with session ids.
pub type ProviderEpoch = u64;

/// What a provider is asked about one offer.
///
/// Sanitized before it gets here: `filename` has been through
/// `filename::sanitize`, and the fingerprint is the authenticated one from
/// the TLS session rather than anything the peer typed.
pub type ApprovalRequest = IncomingOffer;

/// What [`FileApproval::decide`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The named offer was waiting, and now has its answer.
    Answered,
    /// Nothing was waiting under that id. A duplicate answer, an answer to an
    /// offer that has already timed out, or an id that was never pending.
    /// Never an error: a provider cannot be expected to know that the reaper
    /// reached a transfer a few milliseconds before the human did.
    Unknown,
}

struct Pending {
    /// Carried for reporting and for the invariant that a decision is bound
    /// to an authenticated peer, not to a name.
    peer: Fingerprint,
    reply: oneshot::Sender<bool>,
}

#[derive(Default)]
struct Inner {
    provider: Option<mpsc::Sender<ApprovalRequest>>,
    epoch: ProviderEpoch,
    pending: BTreeMap<TransferId, Pending>,
}

/// The rendezvous between `files.v1` and whoever is able to ask a human.
pub struct FileApproval {
    /// `--accept-files-without-asking`. Set once, at startup, and never
    /// changed from a control message: an override that a local client could
    /// switch on would not be an override, it would be a bypass.
    unattended: bool,
    inner: Mutex<Inner>,
}

impl FileApproval {
    /// Builds the approval seam.
    ///
    /// `unattended` is the `--accept-files-without-asking` flag and keeps its
    /// existing meaning exactly: accept every offer, warn on each one, and
    /// never ask. It is checked before anything else here, so a machine
    /// running with it set behaves identically whether or not a GUI is open.
    pub fn new(unattended: bool) -> Self {
        Self {
            unattended,
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Whether this daemon accepts files without asking.
    ///
    /// Reported to a provider when it attaches so the desktop can say so
    /// rather than sit waiting for prompts that will never come.
    pub fn unattended(&self) -> bool {
        self.unattended
    }

    /// A poisoned lock is not a reason to stop asking about files.
    ///
    /// Nothing here can leave the map inconsistent — every path either
    /// inserts one entry or removes one — so the recovered state is the state
    /// a successful lock would have seen.
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Attaches an approval provider, replacing any previous one.
    ///
    /// Replacing rather than refusing, for the reason `begin_pairing`
    /// replaces a pairing window: the provider is a desktop session, a
    /// desktop session can be restarted, and a stale attachment that refused
    /// the live one would leave the machine unable to accept files until the
    /// daemon was restarted.
    ///
    /// Whatever the displaced provider was still being asked is declined —
    /// the `oneshot` senders are dropped — because those questions were put
    /// to a screen that is no longer there.
    pub fn attach(&self) -> (ProviderEpoch, mpsc::Receiver<ApprovalRequest>) {
        let (tx, rx) = mpsc::channel(PROVIDER_QUEUE);
        let mut inner = self.inner();
        inner.epoch = inner.epoch.wrapping_add(1);
        let epoch = inner.epoch;
        let displaced = inner.provider.replace(tx).is_some();
        // Dropped, not answered: dropping the sender closes the channel, and
        // `confirm_receive` reads a closed channel as a decline. There is no
        // path here that produces a `true` nobody asked for.
        inner.pending.clear();
        if displaced {
            tracing::info!("a new file-approval provider replaced the previous one");
        }
        (epoch, rx)
    }

    /// Detaches a provider, if it is still the current one.
    ///
    /// The epoch check is the whole point: a session that is shutting down
    /// after being displaced must not tear down its replacement.
    pub fn detach(&self, epoch: ProviderEpoch) {
        let mut inner = self.inner();
        if inner.epoch != epoch {
            return;
        }
        inner.provider = None;
        inner.pending.clear();
    }

    /// Answers one pending offer.
    ///
    /// Idempotent by construction: the entry is removed as it is answered, so
    /// a second Accept, a second Decline, or an Accept after a Decline all
    /// find nothing and return [`Decision::Unknown`].
    pub fn decide(&self, transfer: TransferId, accept: bool) -> Decision {
        let pending = self.inner().pending.remove(&transfer);
        match pending {
            Some(pending) => {
                // A closed receiver means `files.v1` already gave up on this
                // offer. Still `Answered`: the entry existed and is gone.
                let _ = pending.reply.send(accept);
                Decision::Answered
            }
            None => Decision::Unknown,
        }
    }

    /// Withdraws a pending offer without answering it.
    ///
    /// For the cases where the *transfer* ended while the question was on
    /// screen: the peer disconnected, the offer expired, the pairing was
    /// revoked. Dropping the sender declines, which is the right outcome even
    /// though nothing is listening any more.
    pub fn withdraw(&self, transfer: TransferId) {
        self.inner().pending.remove(&transfer);
    }

    /// The authenticated peer behind a pending offer, if it is still pending.
    pub fn pending_peer(&self, transfer: TransferId) -> Option<Fingerprint> {
        self.inner().pending.get(&transfer).map(|p| p.peer)
    }

    /// How many offers are waiting on a human. Test and diagnostic use.
    pub fn pending(&self) -> usize {
        self.inner().pending.len()
    }
}

/// Removes a pending entry when the question stops being asked.
///
/// `files.v1` wraps `confirm_receive` in the accept timeout, and a timeout
/// *drops the future* rather than telling it anything. Without this, that
/// would leave an entry in the map that a provider could still "answer"
/// minutes later, on a transfer the reaper had already ended. The `Drop`
/// impl is what makes a dropped question an un-answerable one.
struct PendingGuard<'a> {
    approval: &'a FileApproval,
    transfer: TransferId,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.approval.withdraw(self.transfer);
    }
}

#[async_trait::async_trait]
impl TransferApproval for FileApproval {
    async fn confirm_receive(&self, offer: &IncomingOffer) -> bool {
        if self.unattended {
            // Unchanged from the flag's original implementation, log line
            // included: a test rig that greps for this must keep working.
            tracing::warn!(
                transfer = %offer.transfer_id,
                filename = %offer.filename,
                "accepting a file without asking (--accept-files-without-asking)"
            );
            return true;
        }

        let (reply, rx) = oneshot::channel();

        // Queue and record under one lock, so a provider that detaches cannot
        // slip between the two and leave an entry nobody owns.
        let queued = {
            let mut inner = self.inner();
            match inner.provider.clone() {
                None => Err(NotAsked::NoProvider),
                Some(tx) => match tx.try_send(offer.clone()) {
                    Ok(()) => {
                        inner.pending.insert(
                            offer.transfer_id,
                            Pending {
                                peer: offer.peer,
                                reply,
                            },
                        );
                        Ok(())
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => Err(NotAsked::ProviderBacklogged),
                    Err(mpsc::error::TrySendError::Closed(_)) => Err(NotAsked::NoProvider),
                },
            }
        };

        if let Err(why) = queued {
            // The sanitized filename, never the raw one, and the short
            // fingerprint rather than the full one — the same rule the rest
            // of `files.v1` logs by.
            tracing::warn!(
                transfer = %offer.transfer_id,
                filename = %offer.filename,
                size = offer.size_bytes,
                peer = %offer.peer.to_display_short(),
                reason = why.as_str(),
                "declining an incoming file: no way to ask a human. Open the \
                 AnyFlow desktop application, or start the daemon with \
                 --accept-files-without-asking to accept unattended."
            );
            return false;
        }

        let _guard = PendingGuard {
            approval: self,
            transfer: offer.transfer_id,
        };

        // A dropped sender — provider gone, offer withdrawn — reads as a
        // decline. There is no arm of this that produces an unasked-for yes.
        rx.await.unwrap_or(false)
    }
}

/// Why nobody was asked. Reported in the decline log line so "the GUI is not
/// running" and "the GUI stopped reading" are distinguishable.
#[derive(Debug, Clone, Copy)]
enum NotAsked {
    NoProvider,
    ProviderBacklogged,
}

impl NotAsked {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoProvider => "no approval provider is attached",
            Self::ProviderBacklogged => "the approval provider is not reading its prompts",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// A distinct, deterministic fingerprint per seed. Any 32 bytes would
    /// do; hashing keeps it honest about where a real one comes from.
    fn fingerprint(seed: u8) -> Fingerprint {
        Fingerprint::from_spki_der(&[seed; 8])
    }

    fn transfer(seed: u8) -> TransferId {
        TransferId::from_bytes(&[seed; 16]).expect("id")
    }

    fn offer(transfer_seed: u8, peer_seed: u8) -> IncomingOffer {
        IncomingOffer {
            transfer_id: transfer(transfer_seed),
            peer: fingerprint(peer_seed),
            peer_device_id: format!("device-{peer_seed}"),
            filename: "holiday.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
        }
    }

    /// Waits for an offer to reach the provider, with a bound so a broken
    /// implementation fails the test rather than hanging the suite.
    async fn next(rx: &mut mpsc::Receiver<ApprovalRequest>) -> ApprovalRequest {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("an offer should reach the provider")
            .expect("the provider channel should be open")
    }

    /// O / A. The whole point of the sprint's headless requirement: absence of
    /// a GUI is absence of consent, not consent.
    #[tokio::test]
    async fn with_no_provider_attached_an_offer_is_declined() {
        let approval = FileApproval::new(false);
        assert!(!approval.confirm_receive(&offer(1, 1)).await);
        assert_eq!(approval.pending(), 0);
    }

    /// B. The documented escape hatch keeps its documented meaning.
    #[tokio::test]
    async fn the_explicit_override_still_accepts_without_asking() {
        let approval = FileApproval::new(true);
        assert!(approval.confirm_receive(&offer(1, 1)).await);
        // And it asked nobody: nothing was ever queued.
        assert_eq!(approval.pending(), 0);
    }

    /// The override is not reachable from a provider. Attaching one to a
    /// daemon started with the flag changes nothing about the answer.
    #[tokio::test]
    async fn a_provider_cannot_turn_the_override_off() {
        let approval = FileApproval::new(true);
        let (_epoch, mut rx) = approval.attach();
        assert!(approval.confirm_receive(&offer(1, 1)).await);
        assert!(rx.try_recv().is_err(), "the provider was not asked");
    }

    /// C. The affirmative path.
    #[tokio::test]
    async fn a_provider_that_accepts_produces_an_acceptance() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };

        let asked = next(&mut rx).await;
        assert_eq!(asked.transfer_id, transfer(1));
        assert_eq!(asked.filename, "holiday.jpg");
        assert_eq!(asked.size_bytes, 4096);
        assert_eq!(approval.decide(transfer(1), true), Decision::Answered);

        assert!(asking.await.expect("join"));
        assert_eq!(approval.pending(), 0);
    }

    /// D. The negative path, and the one the default must agree with.
    #[tokio::test]
    async fn a_provider_that_declines_produces_a_decline() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };

        next(&mut rx).await;
        assert_eq!(approval.decide(transfer(1), false), Decision::Answered);
        assert!(!asking.await.expect("join"));
        assert_eq!(approval.pending(), 0);
    }

    /// F. Two offers, one answer. The other must be untouched.
    #[tokio::test]
    async fn a_decision_for_one_offer_does_not_resolve_another() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let first = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        next(&mut rx).await;
        let second = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(2, 1)).await })
        };
        next(&mut rx).await;
        assert_eq!(approval.pending(), 2);

        assert_eq!(approval.decide(transfer(1), true), Decision::Answered);
        assert!(first.await.expect("join"));

        // The second is still waiting: an answer to the first did not leak
        // into it, and it is still answerable on its own terms.
        assert_eq!(approval.pending(), 1);
        assert!(!second.is_finished());
        assert_eq!(approval.decide(transfer(2), false), Decision::Answered);
        assert!(!second.await.expect("join"));
    }

    /// G. The same, across peers. A transfer id is minted per transfer, so
    /// two peers can never collide on one — and the pending entry records the
    /// authenticated fingerprint, not a name the peer chose.
    #[tokio::test]
    async fn a_decision_for_one_peer_does_not_resolve_anothers_offer() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let alice = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 0xaa)).await })
        };
        next(&mut rx).await;
        let bob = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(2, 0xbb)).await })
        };
        next(&mut rx).await;

        assert_eq!(approval.pending_peer(transfer(1)), Some(fingerprint(0xaa)));
        assert_eq!(approval.pending_peer(transfer(2)), Some(fingerprint(0xbb)));

        assert_eq!(approval.decide(transfer(1), true), Decision::Answered);
        assert!(alice.await.expect("join"));
        assert!(!bob.is_finished());

        assert_eq!(approval.decide(transfer(2), false), Decision::Answered);
        assert!(!bob.await.expect("join"));
    }

    /// J / K / L. Answering twice, in any combination, changes nothing.
    #[tokio::test]
    async fn a_second_decision_on_the_same_offer_is_inert() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        next(&mut rx).await;

        assert_eq!(approval.decide(transfer(1), false), Decision::Answered);
        assert!(!asking.await.expect("join"));

        // L: a stale Accept after a Decline. It resolves nothing, and there
        // is nothing left for it to resolve.
        assert_eq!(approval.decide(transfer(1), true), Decision::Unknown);
        // J and K: duplicates of either answer.
        assert_eq!(approval.decide(transfer(1), true), Decision::Unknown);
        assert_eq!(approval.decide(transfer(1), false), Decision::Unknown);
        assert_eq!(approval.pending(), 0);
    }

    /// An answer to an id that was never offered is refused, not invented.
    #[tokio::test]
    async fn an_unknown_transfer_id_answers_nothing() {
        let approval = FileApproval::new(false);
        let (_epoch, _rx) = approval.attach();
        assert_eq!(approval.decide(transfer(99), true), Decision::Unknown);
    }

    /// H. The transfer ended underneath the prompt. Whatever the human does
    /// afterwards, the answer this seam produces is a decline.
    #[tokio::test]
    async fn a_withdrawn_offer_declines_and_cannot_be_accepted_afterwards() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        next(&mut rx).await;

        approval.withdraw(transfer(1));
        assert!(!asking.await.expect("join"));
        assert_eq!(approval.decide(transfer(1), true), Decision::Unknown);
    }

    /// I. The offer expires. `files.v1` enforces that by dropping the future,
    /// so this reproduces the drop rather than describing it — and the entry
    /// must not survive it.
    #[tokio::test]
    async fn dropping_the_question_removes_it_so_a_late_accept_finds_nothing() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let timed_out = {
            let approval = Arc::clone(&approval);
            let offer = offer(1, 1);
            tokio::spawn(async move {
                tokio::time::timeout(Duration::from_millis(50), approval.confirm_receive(&offer))
                    .await
                    .unwrap_or(false)
            })
        };
        next(&mut rx).await;

        assert!(!timed_out.await.expect("join"), "a timeout is a decline");
        assert_eq!(approval.pending(), 0, "the dropped question left no entry");
        assert_eq!(approval.decide(transfer(1), true), Decision::Unknown);
    }

    /// The GUI went away. Nothing it was being asked may be accepted by its
    /// successor, and nothing auto-accepts.
    #[tokio::test]
    async fn detaching_a_provider_declines_what_it_was_being_asked() {
        let approval = Arc::new(FileApproval::new(false));
        let (epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        next(&mut rx).await;

        approval.detach(epoch);
        assert!(!asking.await.expect("join"));

        // And with nobody attached, the next offer is declined outright.
        assert!(!approval.confirm_receive(&offer(2, 1)).await);
    }

    /// A departing session must not detach the one that replaced it.
    #[tokio::test]
    async fn a_stale_detach_does_not_unhook_the_live_provider() {
        let approval = Arc::new(FileApproval::new(false));
        let (first_epoch, _first_rx) = approval.attach();
        let (_second_epoch, mut second_rx) = approval.attach();

        approval.detach(first_epoch);

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        let asked = next(&mut second_rx).await;
        assert_eq!(asked.transfer_id, transfer(1));
        assert_eq!(approval.decide(transfer(1), true), Decision::Answered);
        assert!(asking.await.expect("join"));
    }

    /// A GUI that restarts mid-prompt declines the question it abandoned,
    /// rather than leaving it for the new window to answer blind.
    #[tokio::test]
    async fn replacing_a_provider_declines_the_old_ones_questions() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, mut rx) = approval.attach();

        let asking = {
            let approval = Arc::clone(&approval);
            tokio::spawn(async move { approval.confirm_receive(&offer(1, 1)).await })
        };
        next(&mut rx).await;

        let (_new_epoch, _new_rx) = approval.attach();
        assert!(!asking.await.expect("join"));
        assert_eq!(approval.pending(), 0);
    }

    /// A provider that stopped reading is not a provider. The queue bound is
    /// what turns "stopped answering" into a decline rather than a backlog.
    #[tokio::test]
    async fn a_provider_that_stops_reading_declines_rather_than_queueing_forever() {
        let approval = Arc::new(FileApproval::new(false));
        let (_epoch, _rx) = approval.attach();

        // Fill the queue without reading a single prompt. Each of these is
        // left pending on purpose.
        let mut asking = Vec::new();
        for i in 0..PROVIDER_QUEUE {
            let approval = Arc::clone(&approval);
            let offer = offer(i as u8, 1);
            asking.push(tokio::spawn(async move {
                approval.confirm_receive(&offer).await
            }));
        }
        // Wait until every one of them is actually recorded, rather than
        // sleeping and hoping.
        while approval.pending() < PROVIDER_QUEUE {
            tokio::task::yield_now().await;
        }

        // The next one cannot be queued, and is declined rather than held.
        assert!(!approval.confirm_receive(&offer(0xff, 1)).await);
        assert_eq!(approval.pending(), PROVIDER_QUEUE);
    }
}
