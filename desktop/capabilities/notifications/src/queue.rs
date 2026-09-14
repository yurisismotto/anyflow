//! The bounded, coalescing hand-off between the session and the display work.
//!
//! # Why there is a queue at all
//!
//! `anyflow_core::session` awaits `Capability::on_message` before reading the
//! next frame, and every capability on a connection shares that one dispatch
//! loop. So a `Notify` call that took four seconds because gnome-shell was
//! wedged would hold up `battery.v1`, `files.v1` and `clipboard.v1` for four
//! seconds on the same session. **A broken notification sink must not break
//! the other capabilities**, and that is a hard gate, so the D-Bus work
//! happens on a task of its own and this is the seam.
//!
//! It is bounded so that moving the work off the dispatch loop does not turn
//! a flooding peer into unbounded memory. One peer's queue is one peer's:
//! a flood costs that peer its own oldest pending update and costs nobody else
//! anything.
//!
//! # Coalescing, and the one thing that is never coalesced away
//!
//! A progress bar updating sixty times a second produces sixty upserts for one
//! identity. Queueing them all would display fifty-nine states nobody will ever
//! see. So an [`Work::Upsert`] for an identity already pending **replaces it in
//! place**, keeping its position: the sink shows the current state, once.
//!
//! **Terminal work is never coalesced away.** A [`Work::Remove`] supersedes a
//! pending upsert for the same identity — the notification is gone, so
//! displaying it and then removing it is two D-Bus calls to reach the state one
//! call reaches — but the removal itself always survives. Losing a removal
//! would leave a notification on a screen for ever, which is the one failure
//! worse than being slow. When the queue is full the **oldest non-terminal**
//! item is evicted; only if every pending item is terminal is a terminal one
//! dropped, and that is counted and logged rather than silent.
//!
//! # Coalescing never crosses a snapshot marker
//!
//! An upsert is only merged into a pending one if no marker has been queued
//! since. Merging across a `BEGIN` would move an upsert that arrived after the
//! snapshot into it, marking an identity as named in a snapshot it was not part
//! of — which would then protect it from the reconciliation that should have
//! removed it. Failing to remove is the safe direction, but it is still wrong,
//! and the fix costs one backward scan.

use std::collections::VecDeque;
use std::sync::Mutex;

use anyflow_core::notifications::NotificationId;

use crate::limits::MAX_QUEUED_WORK;

/// One unit of display work for one peer.
///
/// Everything that reaches the sink for a peer goes through here, in order,
/// and is performed by that peer's single worker task. That is what makes the
/// outbound side a **single ordered producer**: results, role announcements
/// and everything else a peer receives are written by one task draining one
/// queue, so a `NotificationResult` can never overtake the announcement that
/// preceded it.
#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    /// Announce this device's roles, recomputed from current availability.
    AnnounceRoles,
    /// Display or replace one notification.
    Upsert(Box<crate::Incoming>),
    /// Close one notification and forget it.
    Remove(NotificationId),
    /// Ask the source to dismiss one notification a human closed here.
    ///
    /// The only outbound work item that asks a peer to *do* something, and it
    /// goes through this queue rather than being sent from the close pump for
    /// the reason every other outbound message does: one peer's traffic is
    /// written by one task, so a dismiss can never overtake the result or the
    /// role announcement that preceded it.
    ///
    /// It is **not terminal**. Losing one leaves the notification in the
    /// phone's own shade, where the person can still dismiss it; losing a
    /// `Remove` would leave a notification on a desktop nothing can ever take
    /// off. And it carries `origin_device_id` because the message does — the
    /// source requires that it names the source.
    Dismiss {
        id: NotificationId,
        origin_device_id: String,
    },
    /// A snapshot bracket marker, carried verbatim.
    Sync(anyflow_proto::v1::capabilities::SyncMarker),
    /// The screen locked or unlocked: re-evaluate what is on it.
    LockChanged(bool),
    /// Close every mirror this peer holds. Revocation, and the end of the
    /// reconnect grace.
    CloseAll(CloseAllReason),
}

/// Why every mirror for a peer is being closed. A log line's worth of
/// vocabulary, never sent to a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAllReason {
    /// The `notifications.v1` grant was withdrawn, or the pairing revoked.
    Revoked,
    /// The peer disconnected and the grace window expired.
    GraceExpired,
    /// The peer announced that it is no longer a source.
    RoleNarrowed,
}

impl CloseAllReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Revoked => "revoked",
            Self::GraceExpired => "grace expired",
            Self::RoleNarrowed => "peer is no longer a source",
        }
    }
}

impl Work {
    /// Whether losing this item would leave the two ends disagreeing for ever.
    fn is_terminal(&self) -> bool {
        matches!(self, Self::Remove(_) | Self::CloseAll(_))
    }

    /// Whether this item orders the ones around it, so coalescing must not
    /// move an upsert past it.
    fn is_barrier(&self) -> bool {
        matches!(
            self,
            Self::Sync(_) | Self::CloseAll(_) | Self::LockChanged(_)
        )
    }

    /// A short, content-free name for a log line.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AnnounceRoles => "roles",
            Self::Upsert(_) => "upsert",
            Self::Remove(_) => "remove",
            Self::Dismiss { .. } => "dismiss",
            Self::Sync(_) => "sync",
            Self::LockChanged(_) => "lock",
            Self::CloseAll(_) => "close-all",
        }
    }
}

/// What happened when an item was offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offered {
    /// Queued as a new item.
    Queued,
    /// Merged into an item already pending for the same identity. No new slot
    /// was used and nothing was lost.
    Coalesced,
    /// Queued, and the oldest non-terminal item was evicted to make room.
    ///
    /// The sink converges anyway: the next upsert for that identity, or the
    /// next snapshot, restores what should be on screen.
    Evicted,
    /// The queue was full of terminal items and this one was dropped.
    ///
    /// Expected to stay at zero: reaching it needs
    /// [`MAX_QUEUED_WORK`] simultaneous removals for one peer. It is counted
    /// rather than assumed impossible.
    Dropped,
}

/// One peer's pending work.
///
/// `std::sync::Mutex` rather than tokio's: every operation is a few pointer
/// moves and none of them awaits, so an async mutex would buy a scheduling
/// point and nothing else.
#[derive(Debug, Default)]
pub struct WorkQueue {
    inner: Mutex<QueueState>,
}

#[derive(Debug, Default)]
struct QueueState {
    items: VecDeque<Work>,
    coalesced: u64,
    evicted: u64,
    dropped_terminal: u64,
    high_water: usize,
}

/// Counters, for `anyflow notifications status` and for a log line. No
/// identities and no content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueStats {
    pub pending: usize,
    pub coalesced: u64,
    pub evicted: u64,
    pub dropped_terminal: u64,
    /// The deepest this queue has ever been.
    ///
    /// `pending` is a sample, and sampling a queue that one task is filling
    /// while another drains it measures the scheduler rather than the bound.
    /// This is the measurement: it can only rise, so a burst that peaked
    /// between two reads cannot hide from it — which is what makes "the bound
    /// held" a statement about the run rather than about when the test
    /// happened to look.
    pub high_water: usize,
}

impl WorkQueue {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn stats(&self) -> QueueStats {
        let state = self.lock();
        QueueStats {
            pending: state.items.len(),
            coalesced: state.coalesced,
            evicted: state.evicted,
            dropped_terminal: state.dropped_terminal,
            high_water: state.high_water,
        }
    }

    /// Offers one item. Never blocks and never awaits.
    pub fn offer(&self, work: Work) -> Offered {
        let mut state = self.lock();

        // 1. Coalescing, before anything is added.
        match &work {
            Work::Upsert(incoming) => {
                if let Some(slot) = state.upsert_slot(&incoming.id) {
                    state.items[slot] = work;
                    state.coalesced = state.coalesced.saturating_add(1);
                    return Offered::Coalesced;
                }
            }
            Work::Remove(id) => {
                // A removal supersedes a pending upsert for the same identity:
                // showing it and then removing it reaches the same state as
                // removing it, with one extra D-Bus call and one extra flash on
                // the user's screen. The *removal* still gets its own slot —
                // the mirror may already be displayed from an earlier update.
                if let Some(slot) = state.upsert_slot(id) {
                    state.items.remove(slot);
                }
            }
            // Two announcements in a row would send two identical messages,
            // because the set is recomputed when the item is *handled* rather
            // than when it is queued — so the second would compute the same
            // answer, find it unchanged, and produce nothing.
            Work::AnnounceRoles if state.items.iter().any(|w| matches!(w, Work::AnnounceRoles)) => {
                state.coalesced = state.coalesced.saturating_add(1);
                return Offered::Coalesced;
            }
            _ => {}
        }

        // 2. The ceiling.
        let mut evicted = false;
        if state.items.len() >= MAX_QUEUED_WORK {
            match state.items.iter().position(|w| !w.is_terminal()) {
                Some(index) => {
                    state.items.remove(index);
                    state.evicted = state.evicted.saturating_add(1);
                    evicted = true;
                }
                None => {
                    // Every pending item is terminal. Dropping one is the only
                    // remaining option and it is counted, because a removal
                    // lost here is a notification that stays on the screen.
                    state.dropped_terminal = state.dropped_terminal.saturating_add(1);
                    return Offered::Dropped;
                }
            }
        }

        state.items.push_back(work);
        state.high_water = state.high_water.max(state.items.len());
        if evicted {
            Offered::Evicted
        } else {
            Offered::Queued
        }
    }

    /// Takes everything pending, in order.
    pub fn drain(&self) -> Vec<Work> {
        let mut state = self.lock();
        state.items.drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().items.is_empty()
    }
}

impl QueueState {
    /// The index of a pending upsert for `id`, if merging into it is safe.
    ///
    /// Scans from the back and stops at the first barrier, so an upsert never
    /// moves across a snapshot marker, a lock change or a bulk close.
    fn upsert_slot(&self, id: &NotificationId) -> Option<usize> {
        for (index, item) in self.items.iter().enumerate().rev() {
            if item.is_barrier() {
                return None;
            }
            if let Work::Upsert(pending) = item {
                if &pending.id == id {
                    return Some(index);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Incoming;
    use anyflow_proto::v1::capabilities as pb;

    /// Sixteen bytes seeded from a `u16`, so a test can make more distinct
    /// identities than a byte allows — the queue ceiling is 256.
    fn bytes(seed: u16) -> Vec<u8> {
        let mut out = vec![0u8; 16];
        out[0] = (seed >> 8) as u8;
        out[1] = (seed & 0xff) as u8;
        out
    }

    fn id(seed: u16) -> NotificationId {
        NotificationId::from_bytes(&bytes(seed)).expect("16 bytes")
    }

    fn upsert(seed: u16, body: &str) -> Work {
        Work::Upsert(Box::new(Incoming {
            id: id(seed),
            message: pb::NotificationUpsert {
                notification_id: bytes(seed),
                origin_device_id: "0".repeat(32),
                body: body.to_string(),
                ..pb::NotificationUpsert::default()
            },
        }))
    }

    fn marker(phase: pb::sync_marker::Phase) -> Work {
        Work::Sync(pb::SyncMarker {
            sync_id: vec![7; 16],
            phase: phase as i32,
        })
    }

    #[test]
    fn a_storm_for_one_identity_occupies_one_slot_and_keeps_the_newest() {
        let queue = WorkQueue::new();
        assert_eq!(queue.offer(upsert(1, "0%")), Offered::Queued);
        for percent in 1..=60 {
            assert_eq!(
                queue.offer(upsert(1, &format!("{percent}%"))),
                Offered::Coalesced
            );
        }
        let drained = queue.drain();
        assert_eq!(drained.len(), 1);
        match &drained[0] {
            Work::Upsert(u) => assert_eq!(u.message.body, "60%"),
            other => panic!("expected the newest upsert, got {}", other.kind()),
        }
        assert_eq!(queue.stats().coalesced, 60);
    }

    #[test]
    fn a_removal_supersedes_a_pending_upsert_but_is_itself_queued() {
        let queue = WorkQueue::new();
        queue.offer(upsert(1, "hello"));
        queue.offer(upsert(2, "other"));
        queue.offer(Work::Remove(id(1)));

        let drained = queue.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].kind(), "upsert");
        assert_eq!(drained[1], Work::Remove(id(1)));
    }

    #[test]
    fn coalescing_never_crosses_a_snapshot_marker() {
        let queue = WorkQueue::new();
        queue.offer(upsert(1, "inside"));
        queue.offer(marker(pb::sync_marker::Phase::End));
        queue.offer(upsert(1, "after"));

        let drained = queue.drain();
        assert_eq!(drained.len(), 3, "the second upsert kept its own slot");
        match (&drained[0], &drained[2]) {
            (Work::Upsert(first), Work::Upsert(second)) => {
                assert_eq!(first.message.body, "inside");
                assert_eq!(second.message.body, "after");
            }
            _ => panic!("expected two distinct upserts around the marker"),
        }
    }

    #[test]
    fn the_ceiling_evicts_the_oldest_non_terminal_item() {
        let queue = WorkQueue::new();
        for seed in 0..MAX_QUEUED_WORK {
            assert_eq!(queue.offer(upsert(seed as u16, "x")), Offered::Queued);
        }
        // Distinct identity, so it does not coalesce.
        assert_eq!(
            queue.offer(marker(pb::sync_marker::Phase::Begin)),
            Offered::Evicted
        );

        let stats = queue.stats();
        assert_eq!(stats.pending, MAX_QUEUED_WORK);
        assert_eq!(stats.evicted, 1);
        assert_eq!(stats.dropped_terminal, 0);
    }

    #[test]
    fn a_removal_survives_a_full_queue_of_upserts() {
        let queue = WorkQueue::new();
        for seed in 0..MAX_QUEUED_WORK {
            queue.offer(upsert(seed as u16, "x"));
        }
        assert_eq!(queue.offer(Work::Remove(id(4000))), Offered::Evicted);

        let drained = queue.drain();
        assert_eq!(
            drained.last(),
            Some(&Work::Remove(id(4000))),
            "a removal is never the thing that gets dropped while an upsert \
             could go instead"
        );
    }

    #[test]
    fn a_terminal_drop_is_counted_rather_than_silent() {
        let queue = WorkQueue::new();
        for seed in 0..MAX_QUEUED_WORK {
            queue.offer(Work::Remove(id(seed as u16)));
        }
        assert_eq!(queue.offer(Work::Remove(id(4001))), Offered::Dropped);
        assert_eq!(queue.stats().dropped_terminal, 1);
    }

    #[test]
    fn two_role_announcements_collapse_into_one() {
        let queue = WorkQueue::new();
        assert_eq!(queue.offer(Work::AnnounceRoles), Offered::Queued);
        assert_eq!(queue.offer(Work::AnnounceRoles), Offered::Coalesced);
        assert_eq!(queue.drain().len(), 1);
    }

    #[test]
    fn draining_preserves_order() {
        let queue = WorkQueue::new();
        queue.offer(Work::AnnounceRoles);
        queue.offer(marker(pb::sync_marker::Phase::Begin));
        queue.offer(upsert(1, "a"));
        queue.offer(marker(pb::sync_marker::Phase::End));
        queue.offer(Work::Remove(id(1)));

        let kinds: Vec<_> = queue.drain().iter().map(|w| w.kind()).collect();
        assert_eq!(kinds, ["roles", "sync", "upsert", "sync", "remove"]);
    }
}
