//! What this desktop is currently showing, for one peer.
//!
//! # The key is the peer's pinned identity, and that is the whole security
//! property
//!
//! A mirror is addressed by `(peer_fingerprint, notification_id)`. The
//! fingerprint is the **pinned TLS identity of the sending peer** — the same
//! thing that decides whether the session exists at all — and never
//! `origin_device_id`, which is display data a peer fills in for itself.
//!
//! So a peer can only ever address its own mirrors. Two phones sending the
//! same 16 bytes touch two different entries; a peer that copies another
//! peer's `notification_id` out of the air and replays it reaches its own
//! namespace and nothing else; a peer that lies about `origin_device_id`
//! changes what a diagnostic prints and nothing more. This is
//! NOTIF-SEC-05, -07 and -09, and it holds structurally rather than by check.
//!
//! Because the collision domain is per peer, the per-install derivation
//! secret on the source makes the same guarantee a second time, independently
//! (ADR-0016).
//!
//! # What an entry holds, exactly
//!
//! There is **no title and no body on this type**, and no field that could
//! hold one. What is retained for a live mirror is:
//!
//! | Field | Why it has to be here |
//! | --- | --- |
//! | `server_id` | The local handle. Without it an update creates a second notification instead of replacing the first |
//! | `content_hash` | An opaque digest the *source* computed. Comparing it is how an identical re-send becomes a no-op |
//! | `app_name` | So that when the screen locks, the mirrors already on it can be re-posted reduced to the app's name |
//! | `urgency` | Same: a reduced re-post must not become louder or quieter than the notification it replaces |
//! | `reduced` | Whether what is on screen is already the reduced form, so a lock event does not re-post what is already reduced |
//!
//! `app_name` is the only peer-supplied *text* retained, it is bounded at 64
//! characters by [`crate::text`], and it is the one thing an `AppOnly`
//! presentation displays. Retaining a title or a body in order to restore it
//! on unlock was considered and rejected: it would be a notification history
//! in memory, and the design forbids one. The consequence is stated where it
//! is decided — locking reduces, unlocking does not restore.
//!
//! Nothing here is ever written to disk. There is no serde derive on this
//! module on purpose.

use std::collections::BTreeMap;

use anyflow_core::notifications::NotificationId;

use crate::backend::{ServerId, Urgency};
use crate::limits::MAX_MIRRORS_PER_PEER;

/// One notification this desktop is currently showing on a peer's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorEntry {
    /// The notification server's own handle, once it has given us one.
    ///
    /// `None` while the mirror is known but not displayed — which happens when
    /// the policy is `Suppress` under a locked screen, and when a `display`
    /// call failed and the entry is waiting for the next update to try again.
    pub server_id: Option<ServerId>,
    /// The `content_hash` the source sent with the last accepted upsert.
    ///
    /// **Opaque.** This sink never computes one and never checks one against
    /// the content. It compares the bytes it was given last with the bytes it
    /// is given now, for one peer and one identity, and that is all. See
    /// `crate::lib`'s note on why recomputing it would turn a source-side
    /// optimisation into a wire-format contract.
    pub content_hash: Vec<u8>,
    /// The application's display name, already sanitized.
    pub app_name: String,
    pub urgency: Urgency,
    /// Whether what is currently on screen is the lock-reduced form.
    pub reduced: bool,
    /// Insertion order, for oldest-first eviction at the ceiling.
    seq: u64,
}

/// Every mirror one peer currently holds here.
///
/// Bounded at [`MAX_MIRRORS_PER_PEER`]. The cap is per peer rather than global
/// so that a flooding peer evicts only its own notifications: a global cap
/// would let one device push another device's messages off the screen, which
/// is a denial of service with a very short attack path.
#[derive(Debug, Default)]
pub struct MirrorTable {
    entries: BTreeMap<NotificationId, MirrorEntry>,
    next_seq: u64,
    /// How many entries have been evicted for the ceiling, ever. Reported so
    /// that hitting the cap is observable rather than silent.
    evicted: u64,
}

impl MirrorTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    pub fn get(&self, id: &NotificationId) -> Option<&MirrorEntry> {
        self.entries.get(id)
    }

    pub fn get_mut(&mut self, id: &NotificationId) -> Option<&mut MirrorEntry> {
        self.entries.get_mut(id)
    }

    pub fn contains(&self, id: &NotificationId) -> bool {
        self.entries.contains_key(id)
    }

    /// Inserts or replaces an entry, evicting the oldest if the ceiling is
    /// reached.
    ///
    /// Returns the entry evicted to make room, so the caller can close the
    /// notification it names. Evicting without closing would leave a
    /// notification on the screen that nothing can ever update or remove,
    /// which is the one failure mode worse than dropping it.
    pub fn insert(
        &mut self,
        id: NotificationId,
        server_id: Option<ServerId>,
        content_hash: Vec<u8>,
        app_name: String,
        urgency: Urgency,
        reduced: bool,
    ) -> Option<(NotificationId, MirrorEntry)> {
        let evicted = if self.entries.contains_key(&id) {
            // Replacing an existing identity frees no slot and needs none.
            None
        } else {
            self.evict_if_full()
        };

        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);

        self.entries.insert(
            id,
            MirrorEntry {
                server_id,
                content_hash,
                app_name,
                urgency,
                reduced,
                seq,
            },
        );
        evicted
    }

    fn evict_if_full(&mut self) -> Option<(NotificationId, MirrorEntry)> {
        if self.entries.len() < MAX_MIRRORS_PER_PEER {
            return None;
        }
        // Oldest by insertion, not by id: the id is a digest and its ordering
        // is arbitrary, so evicting by it would drop a random notification.
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.seq)
            .map(|(id, _)| id.clone())?;
        self.evicted = self.evicted.saturating_add(1);
        self.entries.remove_entry(&oldest)
    }

    /// Forgets one entry, returning it.
    pub fn remove(&mut self, id: &NotificationId) -> Option<MirrorEntry> {
        self.entries.remove(id)
    }

    /// Forgets the entry whose server id is `server_id`, returning its
    /// identity.
    ///
    /// This is the `NotificationClosed` path. The server invalidates an id
    /// **before** it sends the signal — *"may not be used in any further
    /// communications with the server"* — so the entry has to go, or the next
    /// upsert for that identity would name a dead id and quietly create a
    /// second notification instead of updating the first.
    pub fn forget_server_id(&mut self, server_id: ServerId) -> Option<NotificationId> {
        let id = self
            .entries
            .iter()
            .find(|(_, e)| e.server_id == Some(server_id))
            .map(|(id, _)| id.clone())?;
        self.entries.remove(&id);
        Some(id)
    }

    /// Drops every server id without dropping the entries.
    ///
    /// What a notification-server restart calls. The identities are still
    /// live — the phone still has those notifications — but every local handle
    /// is stale, and a restarted server may well have reissued the same
    /// numbers to somebody else's notifications. So the numbers go and the
    /// identities stay, and the next upsert or the next snapshot recreates
    /// what should be on screen. Nothing is persisted to survive the restart;
    /// the source is the thing that knows, and asking it again is free.
    pub fn invalidate_server_ids(&mut self) {
        for entry in self.entries.values_mut() {
            entry.server_id = None;
        }
    }

    /// Every entry, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&NotificationId, &MirrorEntry)> {
        let mut all: Vec<_> = self.entries.iter().collect();
        all.sort_by_key(|(_, e)| e.seq);
        all.into_iter()
    }

    /// The identities held here that are **not** in `named`.
    ///
    /// The snapshot reconciliation step, and the only place a mirror is
    /// removed because of something that was *not* said.
    pub fn not_named_in<'a>(
        &'a self,
        named: &'a std::collections::BTreeSet<NotificationId>,
    ) -> Vec<NotificationId> {
        self.entries
            .keys()
            .filter(|id| !named.contains(*id))
            .cloned()
            .collect()
    }

    /// Empties the table, returning every entry so the caller can close each.
    pub fn drain(&mut self) -> Vec<(NotificationId, MirrorEntry)> {
        std::mem::take(&mut self.entries).into_iter().collect()
    }
}

/// Renders without any peer-supplied text at all.
///
/// A `MirrorEntry` holds one string a peer influenced — `app_name` — and a
/// derived `Debug` would put it into every log line and every test failure
/// that touched one. Even an application's *name* is something the user did
/// not ask to have written to a journal, so it is counted and not printed.
impl std::fmt::Display for MirrorTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MirrorTable(entries={}, displayed={}, evicted={})",
            self.entries.len(),
            self.entries
                .values()
                .filter(|e| e.server_id.is_some())
                .count(),
            self.evicted
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u8) -> NotificationId {
        NotificationId::from_bytes(&[seed; 16]).expect("16 bytes")
    }

    fn put(table: &mut MirrorTable, seed: u8, server: u32) {
        table.insert(
            id(seed),
            Some(server),
            vec![seed; 32],
            "App".into(),
            Urgency::Normal,
            false,
        );
    }

    #[test]
    fn an_update_replaces_in_place_and_frees_no_slot() {
        let mut table = MirrorTable::new();
        put(&mut table, 1, 10);
        put(&mut table, 1, 10);
        assert_eq!(table.len(), 1);
        assert_eq!(table.evicted(), 0);
    }

    #[test]
    fn the_ceiling_evicts_the_oldest_and_hands_it_back_to_be_closed() {
        let mut table = MirrorTable::new();
        for seed in 0..MAX_MIRRORS_PER_PEER {
            put(&mut table, seed as u8, seed as u32 + 1);
        }
        assert_eq!(table.len(), MAX_MIRRORS_PER_PEER);

        let evicted = table.insert(
            id(250),
            Some(9999),
            vec![0; 32],
            "App".into(),
            Urgency::Normal,
            false,
        );
        let (evicted_id, entry) = evicted.expect("the ceiling evicts");
        assert_eq!(evicted_id, id(0), "the oldest goes, not an arbitrary one");
        assert_eq!(entry.server_id, Some(1), "so the caller can close it");
        assert_eq!(table.len(), MAX_MIRRORS_PER_PEER);
        assert_eq!(table.evicted(), 1);
    }

    #[test]
    fn a_close_signal_forgets_the_entry_by_its_server_id() {
        let mut table = MirrorTable::new();
        put(&mut table, 1, 10);
        put(&mut table, 2, 11);
        assert_eq!(table.forget_server_id(11), Some(id(2)));
        assert!(!table.contains(&id(2)));
        assert!(table.contains(&id(1)));
        // A second signal for the same id finds nothing, and says so rather
        // than removing something else.
        assert_eq!(table.forget_server_id(11), None);
    }

    #[test]
    fn a_server_restart_drops_the_handles_and_keeps_the_identities() {
        let mut table = MirrorTable::new();
        put(&mut table, 1, 10);
        put(&mut table, 2, 11);
        table.invalidate_server_ids();
        assert_eq!(table.len(), 2, "the phone still has these notifications");
        assert!(table.iter().all(|(_, e)| e.server_id.is_none()));
    }

    #[test]
    fn reconciliation_names_only_what_the_snapshot_omitted() {
        let mut table = MirrorTable::new();
        put(&mut table, 1, 10);
        put(&mut table, 2, 11);
        put(&mut table, 3, 12);

        let named: std::collections::BTreeSet<_> = [id(1), id(3)].into_iter().collect();
        assert_eq!(table.not_named_in(&named), vec![id(2)]);

        // An empty snapshot names nothing, so everything is stale. That is
        // correct and is exactly what a phone with an empty shade says.
        let empty = std::collections::BTreeSet::new();
        assert_eq!(table.not_named_in(&empty).len(), 3);
    }

    #[test]
    fn rendering_counts_and_never_prints_a_name() {
        let mut table = MirrorTable::new();
        table.insert(
            id(1),
            Some(10),
            vec![0; 32],
            "Very Distinctive App Name".into(),
            Urgency::Normal,
            false,
        );
        table.insert(
            id(2),
            None,
            vec![0; 32],
            "Another".into(),
            Urgency::Low,
            true,
        );

        let rendered = table.to_string();
        assert_eq!(rendered, "MirrorTable(entries=2, displayed=1, evicted=0)");
        assert!(!rendered.contains("Distinctive"));
        assert!(!rendered.contains("Another"));
    }
}
