//! A whole sink, in one process, with a fake desktop.
//!
//! Everything here drives the **real** [`NotificationManager`] — the real
//! validation, the real authorizer question, the real mirror table, the real
//! worker task and the real ordering — against an in-memory notification
//! server and an in-memory lock. Nothing is stubbed except the platform, which
//! is exactly the boundary the [`NotificationSink`] seam exists to draw.
//!
//! # Waiting, without sleeping
//!
//! Display work happens on a task of its own, so a test that asserted
//! immediately after sending would race it. Rather than sleeping, every
//! assertion waits on the thing the worker actually produces: a
//! `NotificationResult`. Because one worker drains one queue in order, a
//! result for message *n* proves messages *1..n* have been handled — which is
//! what [`Harness::barrier`] uses to wait for a snapshot marker, which
//! produces no result of its own.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use anyflow_capability_notifications::backend::{
    CloseReason, LockSource, MemoryLock, MemorySink, NotificationSink,
};
use anyflow_capability_notifications::{
    NotificationAuthorizer, NotificationManager, NotificationPolicy, CAPABILITY_ID,
};
use anyflow_core::capability::OutboundMessage;
use anyflow_core::Fingerprint;
use anyflow_proto::v1::capabilities as pb;
use anyflow_proto::Message;
use tokio::sync::{mpsc, RwLock};

pub const TIMEOUT: Duration = Duration::from_secs(5);

/// A trust store that a test can edit.
pub struct Policies {
    inner: RwLock<std::collections::HashMap<Fingerprint, NotificationPolicy>>,
}

impl Policies {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(std::collections::HashMap::new()),
        })
    }

    pub async fn grant(&self, peer: Fingerprint, policy: NotificationPolicy) {
        self.inner.write().await.insert(peer, policy);
    }

    pub async fn revoke(&self, peer: &Fingerprint) {
        self.inner.write().await.remove(peer);
    }
}

#[async_trait::async_trait]
impl NotificationAuthorizer for Policies {
    async fn policy_for(&self, peer: &Fingerprint) -> NotificationPolicy {
        // Absent means ungranted, and ungranted means DENIED. Exactly what the
        // daemon's own implementation answers for an unknown or revoked peer.
        self.inner
            .read()
            .await
            .get(peer)
            .copied()
            .unwrap_or(NotificationPolicy::DENIED)
    }
}

pub fn fingerprint(seed: u8) -> Fingerprint {
    Fingerprint::from_hex(&format!("{seed:02x}").repeat(32)).expect("fingerprint")
}

/// Sixteen bytes seeded from a `u16`, so a test can make more distinct
/// identities than a byte allows.
pub fn id_bytes(seed: u16) -> Vec<u8> {
    let mut out = vec![0u8; 16];
    out[0] = (seed >> 8) as u8;
    out[1] = (seed & 0xff) as u8;
    out
}

pub const ORIGIN: &str = "0123456789abcdef0123456789abcdef";

/// A `NotificationUpsert` with the fields a test usually cares about.
pub fn upsert(seed: u16, title: &str, body: &str) -> pb::NotificationUpsert {
    pb::NotificationUpsert {
        notification_id: id_bytes(seed),
        origin_device_id: ORIGIN.to_string(),
        app_id: "com.example.chat".to_string(),
        app_label: "Chat".to_string(),
        title: title.to_string(),
        body: body.to_string(),
        posted_at_unix_ms: 1_700_000_000_000,
        importance: pb::NotificationImportance::Normal as i32,
        privacy: pb::NotificationPrivacy::Private as i32,
        category: pb::NotificationCategory::Message as i32,
        content_hash: content_hash(title, body),
        ..pb::NotificationUpsert::default()
    }
}

/// A digest that stands in for the one the **source** computes.
///
/// Deliberately *not* the construction ADR-0016 specifies. This sink never
/// recomputes `content_hash`, so any 32 bytes that change when the content
/// changes exercise the code exactly as well as the real digest would — and
/// using something that is obviously not the real construction is how this
/// suite proves the sink is not secretly checking.
pub fn content_hash(title: &str, body: &str) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    for (index, byte) in title.bytes().chain(body.bytes()).enumerate() {
        out[index % 32] ^= byte.wrapping_add(index as u8);
    }
    out
}

pub fn control(body: pb::notification_control::Body) -> Vec<u8> {
    pb::NotificationControl { body: Some(body) }.encode_to_vec()
}

pub fn roles(roles: &[pb::NotificationRole], epoch: u32) -> Vec<u8> {
    control(pb::notification_control::Body::Roles(
        pb::NotificationRoles {
            roles: roles.iter().map(|r| *r as i32).collect(),
            epoch,
        },
    ))
}

pub fn marker(sync_id: &[u8], phase: pb::sync_marker::Phase) -> Vec<u8> {
    control(pb::notification_control::Body::Sync(pb::SyncMarker {
        sync_id: sync_id.to_vec(),
        phase: phase as i32,
    }))
}

/// A `DismissRequest`, for the inbound-refusal tests. The desktop is a sink:
/// it must keep refusing these however much of the dismissal runtime it gains.
pub fn dismiss(seed: u16) -> Vec<u8> {
    control(pb::notification_control::Body::Dismiss(
        pb::DismissRequest {
            notification_id: id_bytes(seed),
            origin_device_id: ORIGIN.to_string(),
        },
    ))
}

/// A `NotificationResult`, as a source answering a `DismissRequest`.
pub fn result(seed: u16, outcome: pb::NotificationOutcome) -> Vec<u8> {
    control(pb::notification_control::Body::Result(
        pb::NotificationResult {
            notification_id: id_bytes(seed),
            outcome: outcome as i32,
        },
    ))
}

pub fn remove(seed: u16) -> Vec<u8> {
    control(pb::notification_control::Body::Remove(
        pb::NotificationRemove {
            notification_id: id_bytes(seed),
            origin_device_id: ORIGIN.to_string(),
        },
    ))
}

/// One peer, one fake desktop, one real manager.
pub struct Harness {
    pub manager: Arc<NotificationManager>,
    pub sink: Arc<MemorySink>,
    pub lock: Arc<MemoryLock>,
    pub policies: Arc<Policies>,
    pub peer: Fingerprint,
    outbound: mpsc::Receiver<OutboundMessage>,
    sender: mpsc::Sender<OutboundMessage>,
    /// Distinct identities used by `barrier`, so a barrier never collides with
    /// a notification a test is asserting on.
    barrier_seed: u16,
}

impl Harness {
    /// A granted peer that has already announced `SOURCE`, which is the state
    /// almost every test wants to start from.
    ///
    /// Dismiss sync is **off**, because that is the default and a suite whose
    /// baseline had it on would never notice it becoming the default.
    pub async fn start() -> Self {
        let mut harness = Self::start_ungranted().await;
        harness
            .policies
            .grant(harness.peer, NotificationPolicy::default())
            .await;
        harness.announce_source().await;
        harness
    }

    /// Everything N4 needs: the grant, dismiss sync on, and a peer that has
    /// announced both `SOURCE` and `DISMISS_TARGET`.
    pub async fn start_dismissing() -> Self {
        let mut harness = Self::start_ungranted().await;
        harness
            .policies
            .grant(
                harness.peer,
                NotificationPolicy {
                    allow_dismiss_sync: true,
                    ..NotificationPolicy::default()
                },
            )
            .await;
        harness.expect_roles().await;
        harness
            .send(&roles(
                &[
                    pb::NotificationRole::Source,
                    pb::NotificationRole::DismissTarget,
                ],
                1,
            ))
            .await;
        harness
    }

    /// Puts one notification on the fake desktop and returns its server id.
    ///
    /// The id comes from the fake server rather than from counting calls: the
    /// two diverge the moment a replacement or a restart is involved, and the
    /// whole point of storing the returned id is that it is authoritative.
    pub async fn display(&mut self, seed: u16) -> u32 {
        self.send_upsert(upsert(seed, "Ana", "lunch?")).await;
        self.expect_outcome(pb::NotificationOutcome::Displayed)
            .await;
        self.sink
            .last_server_id()
            .expect("the fake desktop returned an id")
    }

    /// Brings the peer back on a fresh session, keeping the same channel.
    ///
    /// Everything the slot holds — its worker, its queue, its mirrors — is
    /// deliberately not recreated, because that is exactly what a real
    /// reconnect does.
    pub async fn reattach(&self) {
        self.manager
            .attach_session(self.peer, self.sender.clone())
            .await;
    }

    /// Closes one notification on the fake desktop and **waits until the
    /// manager has finished deciding what that means**.
    ///
    /// Never call `sink.user_closes` directly from a test. The close pump is a
    /// task of its own, so `user_closes` returns as soon as the signal is
    /// buffered — before `note_closed` has run. A negative assertion made in
    /// that window would pass because the thing it was watching for had not
    /// happened *yet*, which is the most comfortable kind of wrong.
    pub async fn close(&self, server_id: u32, reason: CloseReason) {
        let before = self.manager.closes_observed();
        self.sink.user_closes(server_id, reason).await;
        self.wait_for("the close signal to be decided", || {
            self.manager.closes_observed() > before
        })
        .await;
    }

    /// The next `DismissRequest` this device sends, as `(id, origin)`.
    pub async fn next_dismiss(&mut self) -> (Vec<u8>, String) {
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Dismiss(d)) => {
                (d.notification_id, d.origin_device_id)
            }
            other => panic!("expected a dismiss request, got {other:?}"),
        }
    }

    /// Asserts that no `DismissRequest` was produced, by pushing a message
    /// through the same ordered path and seeing what comes out first.
    ///
    /// One worker drains one queue in order, so if a dismissal had been queued
    /// before the barrier's removal it would be answered first. This is why it
    /// is a barrier rather than a sleep: it proves an ordering, not a delay.
    pub async fn expect_no_dismiss(&mut self) {
        self.barrier_seed += 1;
        let seed = self.barrier_seed;
        self.send(&remove(seed)).await;
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Result(r)) => assert_eq!(
                r.notification_id,
                id_bytes(seed),
                "something was sent before the barrier's answer"
            ),
            other => panic!("a message was sent that should not have been: {other:?}"),
        }
    }

    /// Reads everything the peer has been sent until nothing more arrives.
    ///
    /// Needed by the burst tests and by nothing else. The outbound channel is
    /// bounded, so a test that queues five hundred upserts and never reads
    /// their answers wedges the worker against a full channel — and then
    /// [`barrier`](Self::barrier) reads the *first* backed-up answer rather
    /// than its own, which looks like an ordering bug and is not one.
    ///
    /// Returns how many messages were drained, so a test can say what it saw
    /// rather than merely that it waited.
    pub async fn drain_outbound(&mut self) -> usize {
        let mut seen = 0;
        while tokio::time::timeout(Duration::from_millis(250), self.outbound.recv())
            .await
            .ok()
            .flatten()
            .is_some()
        {
            seen += 1;
        }
        seen
    }

    /// Disconnects the peer, arming the reconnect grace.
    ///
    /// The mirrors deliberately stay: closing them here is what the grace
    /// exists to postpone.
    pub async fn detach(&self) {
        self.manager.detach_session(&self.peer).await;
    }

    /// How many items are queued for this peer right now.
    pub async fn pending_work(&self) -> usize {
        self.manager
            .peer_reports()
            .await
            .into_iter()
            .find(|r| r.peer == self.peer)
            .map(|r| r.queue.pending)
            .unwrap_or(0)
    }

    /// How deep this peer's work queue has ever been.
    pub async fn queue_high_water(&self) -> usize {
        self.manager
            .peer_reports()
            .await
            .into_iter()
            .find(|r| r.peer == self.peer)
            .map(|r| r.queue.high_water)
            .unwrap_or(0)
    }

    /// A connected peer with no grant and no announced role.
    pub async fn start_ungranted() -> Self {
        Self::start_with(MemorySink::new()).await
    }

    /// The same, over a sink a test has already configured.
    ///
    /// Needed for the failures that have to be in place *before* the manager
    /// is built: `closed_events` is taken once, at construction, so a sink
    /// that refuses to hand it over has to refuse from the start.
    pub async fn start_with(sink: MemorySink) -> Self {
        let sink = Arc::new(sink);
        let lock = Arc::new(MemoryLock::new());
        let policies = Policies::new();

        let manager = NotificationManager::new(
            Arc::clone(&sink) as Arc<dyn NotificationSink>,
            Arc::clone(&lock) as Arc<dyn LockSource>,
        )
        .await;
        manager
            .set_authorizer(Arc::clone(&policies) as Arc<dyn NotificationAuthorizer>)
            .await;
        manager.spawn_platform_pumps();

        let (sender, outbound) = mpsc::channel(256);
        let peer = fingerprint(0xab);
        manager.attach_session(peer, sender.clone()).await;

        Self {
            manager,
            sink,
            lock,
            policies,
            peer,
            outbound,
            sender,
            barrier_seed: 0xF000,
        }
    }

    /// Attaches a second peer, with its own outbound channel.
    pub async fn second_peer(&self, seed: u8) -> Peer {
        let (sender, outbound) = mpsc::channel(256);
        let peer = fingerprint(seed);
        self.policies
            .grant(peer, NotificationPolicy::default())
            .await;
        self.manager.attach_session(peer, sender.clone()).await;

        let mut second = Peer {
            peer,
            outbound,
            sender,
            barrier_seed: 0xE000,
        };
        second.expect_roles().await;
        self.manager
            .handle_control(peer, &roles(&[pb::NotificationRole::Source], 1))
            .await
            .expect("roles");
        second
    }

    pub async fn send(&self, payload: &[u8]) {
        self.manager
            .handle_control(self.peer, payload)
            .await
            .expect("a notifications.v1 message must never fail the session");
    }

    pub async fn send_upsert(&self, message: pb::NotificationUpsert) {
        self.send(&control(pb::notification_control::Body::Upsert(message)))
            .await;
    }

    /// The next message this device sends the peer.
    pub async fn next_outbound(&mut self) -> pb::NotificationControl {
        let message = tokio::time::timeout(TIMEOUT, self.outbound.recv())
            .await
            .expect("the sink answered within the timeout")
            .expect("the outbound channel is open");
        assert_eq!(message.capability_id, CAPABILITY_ID);
        pb::NotificationControl::decode(message.payload.as_slice()).expect("decodes")
    }

    /// The next `NotificationResult`, as `(id, outcome)`.
    pub async fn next_result(&mut self) -> (Vec<u8>, pb::NotificationOutcome) {
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Result(result)) => (
                result.notification_id,
                pb::NotificationOutcome::try_from(result.outcome)
                    .unwrap_or(pb::NotificationOutcome::Unspecified),
            ),
            other => panic!("expected a result, got {other:?}"),
        }
    }

    pub async fn expect_outcome(&mut self, expected: pb::NotificationOutcome) {
        let (_, outcome) = self.next_result().await;
        assert_eq!(outcome, expected);
    }

    /// The next `NotificationRoles` this device announces.
    pub async fn next_roles(&mut self) -> pb::NotificationRoles {
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Roles(roles)) => roles,
            other => panic!("expected a roles announcement, got {other:?}"),
        }
    }

    /// Consumes the announcement every fresh session begins with.
    pub async fn expect_roles(&mut self) -> pb::NotificationRoles {
        self.next_roles().await
    }

    async fn announce_source(&mut self) {
        self.expect_roles().await;
        self.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    }

    /// Waits until everything sent so far has been handled.
    ///
    /// Sends a removal for an identity nothing has ever used and waits for its
    /// answer. One worker drains one queue in order, so that answer cannot
    /// arrive before every earlier item has been processed — including a
    /// snapshot marker, which produces no answer of its own.
    ///
    /// The *outcome* is deliberately not asserted, only the identity: the
    /// answer depends on the peer's current grant and role, and a barrier that
    /// insisted on `UNKNOWN_NOTIFICATION` would stop working for exactly the
    /// tests that revoke something.
    pub async fn barrier(&mut self) {
        self.barrier_seed += 1;
        let seed = self.barrier_seed;
        self.send(&remove(seed)).await;
        let (id, _) = self.next_result().await;
        assert_eq!(id, id_bytes(seed), "the barrier answered the wrong message");
    }

    /// Waits for something the fake desktop can be asked about.
    ///
    /// The barrier above proves the worker reached a message; this proves an
    /// *effect* happened, which is what a test needs when the effect is
    /// triggered by something other than a message — a revocation, a lock, a
    /// server going away. Polling rather than sleeping a fixed time, so a
    /// loaded machine makes the suite slower rather than flaky.
    pub async fn wait_for(&self, what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            if condition() {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// This peer's report, after everything queued has been handled.
    pub async fn report(&mut self) -> anyflow_capability_notifications::PeerReport {
        self.barrier().await;
        self.manager
            .peer_reports()
            .await
            .into_iter()
            .find(|r| r.peer == self.peer)
            .expect("the peer has a report")
    }
}

/// A second connected peer.
pub struct Peer {
    pub peer: Fingerprint,
    outbound: mpsc::Receiver<OutboundMessage>,
    sender: mpsc::Sender<OutboundMessage>,
    barrier_seed: u16,
}

impl Peer {
    pub async fn next_outbound(&mut self) -> pb::NotificationControl {
        let message = tokio::time::timeout(TIMEOUT, self.outbound.recv())
            .await
            .expect("answered within the timeout")
            .expect("the outbound channel is open");
        pb::NotificationControl::decode(message.payload.as_slice()).expect("decodes")
    }

    pub async fn next_result(&mut self) -> (Vec<u8>, pb::NotificationOutcome) {
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Result(result)) => (
                result.notification_id,
                pb::NotificationOutcome::try_from(result.outcome)
                    .unwrap_or(pb::NotificationOutcome::Unspecified),
            ),
            other => panic!("expected a result, got {other:?}"),
        }
    }

    pub async fn expect_roles(&mut self) -> pb::NotificationRoles {
        match self.next_outbound().await.body {
            Some(pb::notification_control::Body::Roles(roles)) => roles,
            other => panic!("expected a roles announcement, got {other:?}"),
        }
    }

    pub async fn barrier(&mut self, manager: &Arc<NotificationManager>) {
        self.barrier_seed += 1;
        let seed = self.barrier_seed;
        manager
            .handle_control(self.peer, &remove(seed))
            .await
            .expect("remove");
        let (id, _) = self.next_result().await;
        assert_eq!(id, id_bytes(seed));
    }
}
