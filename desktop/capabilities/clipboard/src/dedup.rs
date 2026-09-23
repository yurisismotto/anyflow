//! The two bounded caches that make clipboard sync converge instead of loop.
//!
//! Neither cache stores clipboard content. They hold ids, hashes and
//! monotonic instants, which is the whole reason "no persistence, no history"
//! and "no sync storms" are compatible requirements.
//!
//! # The loop these prevent
//!
//! ```text
//!   Fedora clipboard changes
//!        │
//!        ▼  watcher fires  ──────────► CLIPBOARD_UPDATE ──────► Android
//!                                                                  │
//!                                                     setPrimaryClip
//!                                                                  │
//!                                              Android's own watcher fires
//!                                                                  │
//!        ◄──────────────────────── CLIPBOARD_UPDATE ◄──────────────┘
//!        │
//!   write to Fedora clipboard
//!        │
//!   watcher fires again … for ever
//! ```
//!
//! Two independent mechanisms break it, and both are needed:
//!
//! * [`EventCache`] makes *the same event* idempotent. It stops a retry, a
//!   duplicate, or a replay of one update from being applied or answered
//!   twice.
//! * [`SuppressionCache`] makes *a locally applied remote clip* invisible to
//!   the local watcher. It stops the loop above at the point where a remote
//!   write would otherwise look exactly like a user copying something.
//!
//! # Why not `if new_text == old_text`
//!
//! Because it is wrong in both directions. It suppresses a legitimate re-copy
//! of the same text — a user copying the same command twice is not a loop,
//! and swallowing the second copy makes the feature look broken — and it does
//! not suppress a genuine loop where two peers happen to produce equal
//! content from different events. The suppression entry below is instead
//! **single-use and short-lived**: it is consumed by the first matching
//! watcher event and expires quickly if that event never arrives, so a
//! re-copy a second later is sent normally.

use std::collections::VecDeque;

use tokio::time::{Duration, Instant};

use crate::limits::{EVENT_CACHE_ENTRIES, EVENT_CACHE_TTL, SUPPRESSION_ENTRIES, SUPPRESSION_TTL};

/// Remembers which clipboard events have already been handled.
///
/// Bounded twice over — by entry count and by age — so that neither a quiet
/// session nor a flooding peer can grow it without limit. `tokio::time::
/// Instant` rather than `SystemTime`: expiry must not be steerable by a clock
/// change, and a test must be able to age an entry without sleeping.
#[derive(Debug)]
pub struct EventCache {
    entries: VecDeque<(Vec<u8>, Instant)>,
    capacity: usize,
    ttl: Duration,
}

impl Default for EventCache {
    fn default() -> Self {
        Self::new(EVENT_CACHE_ENTRIES, EVENT_CACHE_TTL)
    }
}

impl EventCache {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(64)),
            capacity,
            ttl,
        }
    }

    /// Records `event_id` as handled.
    ///
    /// Returns `true` if it is new, `false` if it was already known — in
    /// which case the caller must treat the update as a duplicate and do
    /// nothing else with it.
    pub fn admit(&mut self, event_id: &[u8]) -> bool {
        self.expire();
        if self.entries.iter().any(|(id, _)| id == event_id) {
            return false;
        }
        self.entries.push_back((event_id.to_vec(), Instant::now()));
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
        true
    }

    /// Whether an id is currently remembered. Does not record anything.
    pub fn contains(&self, event_id: &[u8]) -> bool {
        let now = Instant::now();
        self.entries
            .iter()
            .any(|(id, at)| id == event_id && now.duration_since(*at) < self.ttl)
    }

    pub fn len(&mut self) -> usize {
        self.expire();
        self.entries.len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    fn expire(&mut self) {
        let now = Instant::now();
        while let Some((_, at)) = self.entries.front() {
            if now.duration_since(*at) >= self.ttl {
                self.entries.pop_front();
            } else {
                // Entries are pushed in time order, so the first live one
                // means every later one is live too.
                break;
            }
        }
    }
}

/// Marks content that this device wrote to its own clipboard *because a peer
/// sent it*, so that the resulting local change is not echoed back out.
///
/// # Single-use, on purpose
///
/// [`take`] removes the entry it matches. The consequences are the ones that
/// make this different from a content comparison:
///
/// * a remote clip applied locally suppresses **exactly one** subsequent
///   watcher event — the one it caused;
/// * copying that same text again by hand, afterwards, is a new local event
///   and is sent normally;
/// * an entry whose watcher event never arrives (the platform coalesced it,
///   the compositor dropped it) expires on its own rather than sitting there
///   swallowing a later copy.
///
/// [`take`]: Self::take
#[derive(Debug)]
pub struct SuppressionCache {
    entries: VecDeque<Suppressed>,
    capacity: usize,
    ttl: Duration,
}

#[derive(Debug)]
struct Suppressed {
    hash: [u8; 32],
    at: Instant,
    /// Device id the content originally came from. Not used for matching —
    /// only so the manager can report *why* something was suppressed without
    /// naming the content.
    origin_device_id: String,
}

impl Default for SuppressionCache {
    fn default() -> Self {
        Self::new(SUPPRESSION_ENTRIES, SUPPRESSION_TTL)
    }
}

impl SuppressionCache {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(16)),
            capacity,
            ttl,
        }
    }

    /// Records that `hash` is about to be written locally on behalf of
    /// `origin_device_id`, and must not be sent back out.
    pub fn arm(&mut self, hash: [u8; 32], origin_device_id: impl Into<String>) {
        self.expire();
        self.entries.push_back(Suppressed {
            hash,
            at: Instant::now(),
            origin_device_id: origin_device_id.into(),
        });
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }

    /// Consumes a suppression entry for `hash`, if one is live.
    ///
    /// `Some(origin_device_id)` means this local clipboard change was caused
    /// by that peer and must not produce an outbound update. `None` means it
    /// is a genuine local copy.
    pub fn take(&mut self, hash: &[u8; 32]) -> Option<String> {
        self.expire();
        let index = self.entries.iter().position(|e| &e.hash == hash)?;
        self.entries.remove(index).map(|e| e.origin_device_id)
    }

    pub fn len(&mut self) -> usize {
        self.expire();
        self.entries.len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    fn expire(&mut self) {
        let now = Instant::now();
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.at) >= self.ttl {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::content_hash;

    #[test]
    fn a_new_event_is_admitted_once() {
        let mut cache = EventCache::default();
        assert!(cache.admit(b"0123456789abcdef"), "first sighting is new");
        assert!(
            !cache.admit(b"0123456789abcdef"),
            "the same id must not be admitted twice"
        );
    }

    #[test]
    fn distinct_events_with_equal_content_are_both_admitted() {
        // The property a content comparison gets wrong: two different clipboard
        // events may legitimately carry the same text.
        let mut cache = EventCache::default();
        assert!(cache.admit(b"aaaaaaaaaaaaaaaa"));
        assert!(cache.admit(b"bbbbbbbbbbbbbbbb"));
    }

    #[tokio::test(start_paused = true)]
    async fn events_expire_by_age() {
        let mut cache = EventCache::new(1024, Duration::from_secs(60));
        assert!(cache.admit(b"0123456789abcdef"));
        assert!(cache.contains(b"0123456789abcdef"));

        tokio::time::advance(Duration::from_secs(61)).await;

        assert!(!cache.contains(b"0123456789abcdef"), "should have expired");
        assert_eq!(cache.len(), 0, "expired entries must be dropped, not kept");
        assert!(cache.admit(b"0123456789abcdef"), "and the id is free again");
    }

    #[test]
    fn the_event_cache_is_bounded_by_count() {
        let mut cache = EventCache::new(8, Duration::from_secs(3600));
        for i in 0..1000u32 {
            let id = [i.to_be_bytes().as_slice(), &[0u8; 12]].concat();
            assert!(cache.admit(&id));
        }
        assert_eq!(cache.len(), 8, "the cache must not grow without bound");
    }

    #[test]
    fn suppression_is_single_use() {
        let mut cache = SuppressionCache::default();
        let hash = content_hash("shared text");

        cache.arm(hash, "desktop-1");
        assert_eq!(
            cache.take(&hash).as_deref(),
            Some("desktop-1"),
            "the echo of the remote write is suppressed"
        );
        assert_eq!(
            cache.take(&hash),
            None,
            "a later copy of the same text is a real local event"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_suppression_entry_expires_rather_than_swallowing_a_later_copy() {
        let mut cache = SuppressionCache::new(64, Duration::from_secs(10));
        let hash = content_hash("re-copied later");
        cache.arm(hash, "phone-1");

        // The watcher event never arrived — the platform coalesced it.
        tokio::time::advance(Duration::from_secs(11)).await;

        assert_eq!(
            cache.take(&hash),
            None,
            "a stale entry must not suppress a genuine copy"
        );
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn the_suppression_cache_is_bounded() {
        let mut cache = SuppressionCache::new(4, Duration::from_secs(3600));
        for i in 0..500u32 {
            cache.arm(content_hash(&format!("text {i}")), "peer");
        }
        assert_eq!(cache.len(), 4);
    }

    #[test]
    fn suppression_matches_content_not_order() {
        let mut cache = SuppressionCache::default();
        let a = content_hash("first");
        let b = content_hash("second");
        cache.arm(a, "peer-a");
        cache.arm(b, "peer-b");

        // The clipboard settled on the second write; the first entry is still
        // live and must not be consumed by it.
        assert_eq!(cache.take(&b).as_deref(), Some("peer-b"));
        assert_eq!(cache.take(&a).as_deref(), Some("peer-a"));
    }
}
