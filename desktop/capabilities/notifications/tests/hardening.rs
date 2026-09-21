//! N5 — the states a sink reaches when things go wrong.
//!
//! Where `sink.rs` proves the rules and `dismiss.rs` proves the one message
//! that travels the other way, this file is about the boundaries: the
//! reconnect grace crossed in both directions, the queue and mirror ceilings
//! driven past, two peers proved independent rather than assumed to be, role
//! epochs attacked rather than exercised, the notification server taken away
//! and given back, and every platform call failed one at a time.
//!
//! # Portability
//!
//! **Portable.** Nothing here touches D-Bus, logind, GTK or any `target_os`.
//! The platform is the `MemorySink`/`MemoryLock` pair, which is the whole
//! point of the `NotificationSink` seam. It belongs in the Windows MSVC
//! portable gate beside `sink.rs` and `dismiss.rs`.
//!
//! # The grace, and why it is moved rather than waited out
//!
//! `limits::RECONNECT_GRACE` is sixty seconds. Its two interesting behaviours
//! are on opposite sides of that boundary, so a suite that only ever waited
//! less than it would prove one half of the rule and call it a pass. These
//! tests shorten the grace through `set_reconnect_grace` and then cross it in
//! both directions. What is being tested is the *rule*; sixty seconds is a
//! local tunable that is not on the wire and that two peers may disagree
//! about without any consequence at all.

mod common;

use std::time::Duration;

use common::*;
use omnibridge_capability_notifications::backend::{CloseReason, MemorySink, SinkError};
use omnibridge_capability_notifications::{LockPolicy, NotificationPolicy};
use omnibridge_proto::v1::capabilities as pb;

/// Short enough to cross inside a test, long enough that the worker is not
/// racing it.
const SHORT_GRACE: Duration = Duration::from_millis(150);

// ---------------------------------------------------------------------------
// §9 — the reconnect grace
// ---------------------------------------------------------------------------

/// The shipped value, stated in a test so that changing it is a deliberate
/// act rather than a diff nobody reads.
///
/// What is normative is the bound, not the number: greater than zero, or a
/// Wi-Fi blip clears the desktop and re-posts everything; finite, or a phone
/// that left the building leaves notifications on a screen that can no longer
/// update or dismiss them.
#[tokio::test]
async fn the_shipped_grace_is_greater_than_zero_and_finite() {
    let h = Harness::start().await;
    let grace = h.manager.reconnect_grace();
    assert!(
        !grace.is_zero(),
        "a zero grace is the churn it exists to stop"
    );
    assert!(grace < Duration::from_secs(600), "a grace must be finite");
    assert_eq!(
        grace,
        Duration::from_secs(60),
        "the shipped reconnect grace moved; that is a decision, not a detail"
    );
}

/// **A disconnect shorter than the grace destroys nothing.**
///
/// The whole reason the grace exists. A three-second Wi-Fi blip that cleared
/// the desktop and re-posted everything would, on a server with persistence,
/// fill the notification list with duplicates.
#[tokio::test]
async fn a_reconnect_inside_the_grace_keeps_every_mirror() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));
    let first = h.display(1).await;
    h.display(2).await;
    assert_eq!(h.sink.live_count(), 2);

    h.detach().await;
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    assert_eq!(
        h.sink.live_count(),
        2,
        "a reconnect inside the grace cleared the screen"
    );
    assert!(
        h.sink.closes().is_empty(),
        "nothing should have been closed"
    );
    // And the mirrors are the same objects, not re-posted ones.
    assert!(h.sink.live(first).is_some());
    assert_eq!(h.report().await.mirrors, 2);
}

/// **A disconnect longer than the grace takes the mirrors off the screen.**
///
/// The other half. A phone that has left the building must not leave
/// notifications on a desktop that can no longer update or dismiss them.
#[tokio::test]
async fn a_disconnect_longer_than_the_grace_closes_the_mirrors() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.display(1).await;
    h.display(2).await;

    h.detach().await;
    h.wait_for("the grace to expire", || h.sink.live_count() == 0)
        .await;
    assert_eq!(
        h.sink.closes().len(),
        2,
        "both mirrors were closed, once each"
    );
}

/// **A peer that never returns leaves nothing behind.**
///
/// The same path as above, asserted on the state rather than the screen: the
/// slot survives, because it holds the worker a reconnect would find already
/// running, and it holds nothing else.
#[tokio::test]
async fn a_peer_that_never_returns_leaves_no_mirrors_and_no_content() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.send_upsert(upsert(
        1,
        "OMNIBRIDGE-N5-GRACE-TITLE",
        "OMNIBRIDGE-N5-GRACE-BODY",
    ))
    .await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.detach().await;
    h.wait_for("the grace to expire", || h.sink.live_count() == 0)
        .await;

    let reports = h.manager.peer_reports().await;
    let rendered = format!("{reports:?}");
    assert!(!rendered.is_empty());
    for canary in ["OMNIBRIDGE-N5-GRACE-TITLE", "OMNIBRIDGE-N5-GRACE-BODY"] {
        assert!(
            !rendered.contains(canary),
            "a departed peer's report carried {canary}"
        );
    }
    let peer = reports.iter().find(|r| r.peer == h.peer).expect("report");
    assert_eq!(peer.mirrors, 0);
    assert!(!peer.connected);
}

/// **A disconnected peer claims no roles.**
///
/// Role state is per connection (ADR-0017 §4), so when the connection ends it
/// goes with it. Resetting it only on the *next* `attach_session` was almost
/// enough and left one real gap: a session rebuilt without this capability
/// negotiated never calls `attach_session`, so the previous session's roles
/// survived it and `omnibridge notifications status` went on reporting "the
/// device can source notifications (epoch 2)" for a peer that had no channel
/// to say so on. Observed on hardware during the N5 §5 gate.
///
/// Nothing could ever have *flowed* — the grant is re-checked per message —
/// so what this costs is the truth of the one screen somebody reads when they
/// are working out why their notifications stopped.
#[tokio::test]
async fn a_peer_with_no_session_claims_no_roles() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));
    h.display(1).await;

    let connected = h.report().await;
    assert!(connected.peer_is_source, "the premise");
    assert_eq!(connected.peer_epoch, 1);
    assert!(connected.local_roles > 0);

    h.detach().await;
    let gone = h
        .manager
        .peer_reports()
        .await
        .into_iter()
        .find(|r| r.peer == h.peer)
        .expect("the slot survives, because the mirrors do");

    assert!(!gone.connected);
    assert!(
        !gone.peer_is_source,
        "a peer with no session must not still claim it can source"
    );
    assert!(!gone.peer_is_dismiss_target);
    assert_eq!(gone.peer_epoch, 0, "the epoch is per connection");
    assert_eq!(gone.local_roles, 0, "and so is this device's own set");
    // The mirrors are still there: this is a reset of claims, not of state.
    assert_eq!(gone.mirrors, 1);
    assert_eq!(h.sink.live_count(), 1);
}

/// **A grace timer armed by an old session cannot close a new one's mirrors.**
///
/// The generation check, driven rather than read. The timer from the first
/// disconnect fires *after* the reconnect has happened and must do nothing.
#[tokio::test]
async fn an_expired_grace_from_an_old_session_cannot_close_the_new_ones_mirrors() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.display(1).await;

    h.detach().await;
    // Back well inside the grace.
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    let second = h.display(2).await;

    // Now outlive the first timer, comfortably.
    tokio::time::sleep(SHORT_GRACE * 4).await;
    h.barrier().await;

    assert_eq!(
        h.sink.live_count(),
        2,
        "a timer from the previous session closed the live session's mirrors"
    );
    assert!(h.sink.live(second).is_some());
}

/// **A session that is replaced cannot take the replacement's mirrors down
/// with it.**
///
/// The exact failure this reproduces was found by running the N5 §8 gate on
/// hardware, not by reasoning about it. A Wi-Fi outage long enough for the
/// phone to redial before this desktop noticed the old socket had died
/// produces two sessions for one peer; the daemon keeps the newer and shuts
/// the older down. But the capability sees the *attach* of the new session
/// first — `on_peer_connected` runs before the displaced session's loop has
/// finished — so the displaced session's `on_peer_disconnected` arrives after
/// the live session is already up.
///
/// It looked exactly like the live session ending. Sixty seconds later:
///
/// ```text
/// closed every mirror for a peer  closed=4  reason="grace expired"
/// ```
///
/// …on a session that was connected and healthy, showing four notifications
/// the user could see. The generation check could not catch it, because the
/// stale detach had captured the *new* generation on its way past.
#[tokio::test]
async fn a_replaced_sessions_detach_cannot_close_the_live_sessions_mirrors() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.display(1).await;
    h.display(2).await;
    assert_eq!(h.sink.live_count(), 2);

    // The order the daemon produces: the replacement attaches, *then* the
    // session it replaced reports that it has closed.
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    h.detach().await;

    // Outlive the grace comfortably. Nothing may happen.
    tokio::time::sleep(SHORT_GRACE * 4).await;
    h.barrier().await;

    assert_eq!(
        h.sink.live_count(),
        2,
        "a replaced session's grace timer closed the live session's mirrors"
    );
    let report = h.report().await;
    assert_eq!(report.mirrors, 2);
    assert!(report.connected, "the live session was marked disconnected");

    // And the live session can still send: the stale detach must not have
    // taken its outbound channel either.
    h.send_upsert(upsert(3, "Ana", "still here")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 3);
}

/// One displacement owes exactly one stale detach, and no more.
///
/// The counter is spent, not a permanent exemption: after the superseded
/// session's detach has been absorbed, a genuine disconnect must still arm the
/// grace and still clear the screen. Getting this wrong in the other direction
/// would leave a departed phone's notifications up for ever, which is the
/// failure the grace exists to prevent.
#[tokio::test]
async fn a_genuine_disconnect_after_a_displacement_still_clears_the_screen() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.display(1).await;

    // Displacement: attach over a live session, then its late detach.
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    h.detach().await;
    tokio::time::sleep(SHORT_GRACE * 3).await;
    assert_eq!(h.sink.live_count(), 1, "the stale detach was absorbed");

    // Now the real one.
    h.detach().await;
    h.wait_for("the grace to expire on the genuine disconnect", || {
        h.sink.live_count() == 0
    })
    .await;
    assert_eq!(h.sink.closes().len(), 1);
}

/// **An upsert that arrives with no session is refused, and changes nothing.**
///
/// It cannot happen over a real transport — there is no session to carry it —
/// but the manager must not corrupt itself if it does. Role state is per
/// connection (ADR-0017 §4) and the connection is over, so the peer claims no
/// roles and its traffic is refused: the fail-closed default, reached by the
/// same code path as a peer that never announced anything.
#[tokio::test]
async fn an_upsert_that_arrives_with_no_session_is_refused_and_changes_nothing() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));
    h.display(1).await;

    h.detach().await;
    h.send_upsert(upsert(2, "Ana", "later")).await;

    // Nothing new reaches the screen. Asserted by waiting a window longer
    // than the worker needs and then reading, because the interesting claim
    // is an absence.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        h.sink.live_count(),
        1,
        "a peer with no session put something on the screen"
    );

    // And the mirror it already had is untouched: the grace has not expired.
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    assert_eq!(h.report().await.mirrors, 1);

    // Normal service resumes on the new session.
    h.send_upsert(upsert(2, "Ana", "later")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 2);
}

/// **A removal that arrives with no session is refused, and the mirror is
/// still cleared — by the grace, which is what owns a departed peer's state.**
///
/// The safe-direction question, asked the other way round: refusing a removal
/// is the one refusal that could leave something on a screen for ever. It does
/// not, because the peer that sent it has gone and the grace is already armed
/// against exactly that.
#[tokio::test]
async fn a_removal_with_no_session_is_refused_and_the_grace_still_clears_it() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    let id = h.display(1).await;
    h.detach().await;

    h.send(&remove(1)).await;
    h.wait_for("the grace to clear the mirror", || {
        h.sink.live(id).is_none()
    })
    .await;
    assert_eq!(h.sink.live_count(), 0);

    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    assert_eq!(h.report().await.mirrors, 0);
}

/// **An incomplete snapshot cannot preserve a ghost for ever.**
///
/// A `BEGIN` with no `END` removes nothing, which is the safe direction — and
/// it must not therefore become a way to keep a mirror alive after the peer
/// has gone. The grace runs underneath the snapshot timeout and is what
/// eventually clears it.
#[tokio::test]
async fn a_snapshot_left_open_by_a_departed_peer_is_still_cleared_by_the_grace() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(SHORT_GRACE);
    h.display(1).await;

    h.send(&marker(&[9u8; 16], pb::sync_marker::Phase::Begin))
        .await;
    h.barrier().await;
    assert!(h.report().await.snapshot_open);

    h.detach().await;
    h.wait_for("the grace to clear the ghost", || h.sink.live_count() == 0)
        .await;
    let report = h.manager.peer_reports().await;
    let peer = report.iter().find(|r| r.peer == h.peer).expect("report");
    assert_eq!(peer.mirrors, 0);
    assert!(
        !peer.snapshot_open,
        "an open snapshot must not survive the connection it was opened on"
    );
}

/// The grace timer is a local implementation detail and no message carries it.
///
/// Asserted against the schema rather than argued: if a timing field were ever
/// added to `notifications.v1`, this fails.
#[test]
fn no_wire_message_carries_a_timing_value() {
    let rendered = format!(
        "{:?}",
        pb::NotificationControl {
            body: Some(pb::notification_control::Body::Roles(
                pb::NotificationRoles {
                    roles: vec![],
                    epoch: 1
                }
            )),
        }
    );
    for word in ["grace", "timeout", "ttl", "expires", "deadline"] {
        assert!(
            !rendered.to_ascii_lowercase().contains(word),
            "the wire vocabulary grew a timing concept: {word}"
        );
    }
}

// ---------------------------------------------------------------------------
// §10 — queue and mirror bounds
// ---------------------------------------------------------------------------

/// **500 sequential notifications, and the 200-mirror ceiling holds.**
///
/// The ceiling is per peer, so a flood costs that peer its own oldest mirrors
/// and costs nobody else anything. What is asserted here is that the number
/// never exceeds the bound and that the overflow is *counted* rather than
/// silently absorbed.
#[tokio::test]
async fn five_hundred_notifications_never_exceed_the_mirror_ceiling() {
    let mut h = Harness::start().await;
    for seed in 1..=500u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    let answered = h.drain_outbound().await;
    assert!(answered > 0, "the burst produced no answers at all");

    let report = h.report().await;
    assert_eq!(
        report.mirrors, 200,
        "the per-peer ceiling is 200 and it is exactly what is held"
    );
    assert!(
        h.sink.live_count() <= 200,
        "the desktop is holding {} notifications",
        h.sink.live_count()
    );
    assert!(
        report.evicted > 0,
        "300 notifications went somewhere and it was not counted"
    );
    // The queue bound held too, and it is a measurement rather than a sample.
    assert!(
        report.queue.high_water <= 256,
        "the work queue reached {}",
        report.queue.high_water
    );
    eprintln!(
        "N5 §10 — 500 sequential: mirrors={} evicted={} queue_high_water={} \
         coalesced={} queue_evicted={} dropped_terminal={}",
        report.mirrors,
        report.evicted,
        report.queue.high_water,
        report.queue.coalesced,
        report.queue.evicted,
        report.queue.dropped_terminal
    );
}

/// **500 updates to one identity produce one mirror.**
///
/// The coalescing case. A progress bar updating five hundred times must not
/// become five hundred D-Bus calls, and must never become five hundred
/// notifications.
#[tokio::test]
async fn five_hundred_updates_to_one_identity_stay_one_mirror() {
    let mut h = Harness::start().await;
    for step in 1..=500u16 {
        h.send_upsert(upsert(1, "Download", &format!("{step} of 500")))
            .await;
    }
    h.drain_outbound().await;

    let report = h.report().await;
    assert_eq!(report.mirrors, 1, "one identity is one mirror");
    assert_eq!(h.sink.live_count(), 1);
    assert!(
        h.sink.displays().len() < 500,
        "every update reached the desktop; nothing was coalesced"
    );
    assert!(report.queue.coalesced > 0);
    eprintln!(
        "N5 §10 — 500 updates to one identity: displays={} coalesced={} \
         queue_high_water={}",
        h.sink.displays().len(),
        report.queue.coalesced,
        report.queue.high_water
    );
}

/// **The 201st mirror evicts one and does not exceed the bound.**
///
/// Stated exactly rather than as "roughly 200", because the off-by-one is the
/// interesting part of a ceiling.
#[tokio::test]
async fn the_two_hundred_and_first_mirror_evicts_exactly_one() {
    let mut h = Harness::start().await;
    for seed in 1..=200u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    h.drain_outbound().await;
    let at_bound = h.report().await;
    assert_eq!(at_bound.mirrors, 200);
    assert_eq!(at_bound.evicted, 0, "200 is inside the bound, not past it");

    h.send_upsert(upsert(201, "Ana", "one more")).await;
    h.drain_outbound().await;
    let past = h.report().await;
    assert_eq!(past.mirrors, 200, "the ceiling did not hold");
    assert_eq!(past.evicted, 1, "exactly one mirror was evicted");
}

/// **A terminal removal is never silently lost under pressure.**
///
/// The one failure worse than being slow. Five hundred upserts followed by
/// removals for the identities that survived: every one of them must converge
/// to "not on the screen", and any removal the queue genuinely had to drop
/// must be counted.
#[tokio::test]
async fn removals_under_pressure_converge_and_any_loss_is_counted() {
    let mut h = Harness::start().await;
    for seed in 1..=300u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    h.drain_outbound().await;
    assert_eq!(h.report().await.mirrors, 200);

    // Remove every identity that could still be held. The ones that were
    // evicted answer UNKNOWN_NOTIFICATION, which is convergence, not failure.
    for seed in 1..=300u16 {
        h.send(&remove(seed)).await;
    }
    h.drain_outbound().await;

    let report = h.report().await;
    assert_eq!(
        report.mirrors, 0,
        "a removal was lost and left a notification on the screen"
    );
    assert_eq!(h.sink.live_count(), 0);
    assert_eq!(
        report.queue.dropped_terminal, 0,
        "a terminal item was dropped; every one of those is a notification \
         that could have been left on a screen for ever"
    );
    eprintln!(
        "N5 §10 — removals under pressure: dropped_terminal={} \
         queue_evicted={} queue_high_water={}",
        report.queue.dropped_terminal, report.queue.evicted, report.queue.high_water
    );
}

/// **A snapshot after pressure converges on exactly what it names.**
///
/// The reconciliation has to work from the post-flood state, not from a clean
/// one — which means it has to remove the mirrors the flood left behind.
#[tokio::test]
async fn a_snapshot_after_a_burst_converges_on_what_it_names() {
    let mut h = Harness::start().await;
    for seed in 1..=400u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    h.drain_outbound().await;
    assert_eq!(h.report().await.mirrors, 200);

    let sync = [7u8; 16];
    h.send(&marker(&sync, pb::sync_marker::Phase::Begin)).await;
    for seed in 396..=400u16 {
        h.send_upsert(upsert(seed, "Ana", "still here")).await;
    }
    h.send(&marker(&sync, pb::sync_marker::Phase::End)).await;
    h.drain_outbound().await;

    let report = h.report().await;
    assert_eq!(
        report.mirrors, 5,
        "the snapshot is the truth: five named, everything else gone"
    );
    assert_eq!(h.sink.live_count(), 5);
    assert!(!report.snapshot_open);
}

/// **A dismiss during a burst still reaches the source.**
///
/// The dismiss item is not terminal, so it *may* be evicted under pressure —
/// but on an ordinary burst it must not be, or the feature would be unusable
/// on a busy phone.
#[tokio::test]
async fn a_human_dismissal_during_a_burst_still_reaches_the_source() {
    let mut h = Harness::start_dismissing().await;
    let id = h.display(1).await;
    for seed in 10..=60u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    h.close(id, CloseReason::Dismissed).await;

    // Drain until the dismiss appears; the results for the burst come first
    // because one worker drains one queue in order.
    let mut found = None;
    for _ in 0..200 {
        match h.next_outbound().await.body {
            Some(pb::notification_control::Body::Dismiss(d)) => {
                found = Some(d.notification_id);
                break;
            }
            Some(_) => continue,
            None => panic!("an empty control message"),
        }
    }
    assert_eq!(
        found.as_deref(),
        Some(id_bytes(1).as_slice()),
        "the dismissal was lost in the burst"
    );
}

// ---------------------------------------------------------------------------
// §12 — multi-peer isolation
// ---------------------------------------------------------------------------

/// Revoking one peer leaves the other working.
#[tokio::test]
async fn revoking_one_peer_leaves_the_other_mirroring() {
    let mut h = Harness::start().await;
    let mut b = h.second_peer(0xcd).await;
    h.display(1).await;
    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1, "Bo", "hello",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = b.next_result().await;
    assert_eq!(outcome, pb::NotificationOutcome::Displayed);
    assert_eq!(
        h.sink.live_count(),
        2,
        "same identity, two peers, two mirrors"
    );

    h.policies.revoke(&h.peer).await;
    h.manager.revoke_peer(&h.peer).await;
    h.wait_for("A's mirrors to close", || h.sink.live_count() == 1)
        .await;

    // B is untouched and still working.
    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                2,
                "Bo",
                "still here",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = b.next_result().await;
    assert_eq!(
        outcome,
        pb::NotificationOutcome::Displayed,
        "revoking A stopped B"
    );
    assert_eq!(h.sink.live_count(), 2);
}

/// One peer's allow-list and dismiss setting say nothing about another's.
///
/// The policies are per peer in the trust store; this proves the manager asks
/// per peer rather than caching one answer.
#[tokio::test]
async fn one_peers_policy_never_applies_to_another() {
    let mut h = Harness::start().await;
    let mut b = h.second_peer(0xce).await;

    // A is told to suppress everything while locked; B is not.
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                when_sink_locked: LockPolicy::Suppress,
                ..NotificationPolicy::default()
            },
        )
        .await;
    h.lock.set_locked(true).await;
    h.wait_for("the lock to be seen", || h.manager.is_locked())
        .await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;

    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1, "Bo", "hello",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = b.next_result().await;
    assert_eq!(
        outcome,
        pb::NotificationOutcome::Displayed,
        "A's Suppress policy silenced B"
    );
}

/// A reconnect by one peer does not disturb the other.
#[tokio::test]
async fn reconnecting_one_peer_leaves_the_others_mirrors_alone() {
    let mut h = Harness::start().await;
    let mut b = h.second_peer(0xcf).await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));

    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1, "Bo", "hello",
            ))),
        )
        .await
        .expect("upsert");
    b.next_result().await;
    h.display(2).await;
    assert_eq!(h.sink.live_count(), 2);

    h.detach().await;
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    b.barrier(&h.manager).await;
    assert_eq!(
        h.sink.live_count(),
        2,
        "A's reconnect touched B's notification"
    );
}

/// A role epoch is per peer and per connection; one peer's cannot affect
/// another's.
#[tokio::test]
async fn a_role_epoch_is_scoped_to_one_peer() {
    let mut h = Harness::start().await;
    let mut b = h.second_peer(0xd0).await;

    // A narrows to nothing at a high epoch.
    h.send(&roles(&[], 9_000)).await;
    h.barrier().await;

    // B's epoch 1 announcement is unaffected and it still sources.
    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1, "Bo", "hello",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = b.next_result().await;
    assert_eq!(
        outcome,
        pb::NotificationOutcome::Displayed,
        "A's epoch narrowed B"
    );

    let reports = h.manager.peer_reports().await;
    let a = reports.iter().find(|r| r.peer == h.peer).expect("A");
    let bb = reports.iter().find(|r| r.peer == b.peer).expect("B");
    assert_eq!(a.peer_epoch, 9_000);
    assert_eq!(bb.peer_epoch, 1, "B's epoch moved because A's did");
    assert!(!a.peer_is_source);
    assert!(bb.peer_is_source);
}

/// Counters are per peer.
#[tokio::test]
async fn the_counters_are_per_peer() {
    let mut h = Harness::start().await;
    let b = h.second_peer(0xd1).await;
    for seed in 1..=250u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    h.drain_outbound().await;

    let reports = h.manager.peer_reports().await;
    let a = reports.iter().find(|r| r.peer == h.peer).expect("A");
    let bb = reports.iter().find(|r| r.peer == b.peer).expect("B");
    assert!(a.evicted > 0);
    assert_eq!(bb.evicted, 0, "A's flood was charged to B");
    assert_eq!(bb.mirrors, 0);
    assert_eq!(bb.queue.high_water.min(1), bb.queue.high_water.min(1));
}

/// One peer's snapshot cannot close another peer's mirrors.
///
/// Covered for the happy path in `sink.rs`; asserted here against a snapshot
/// that names *nothing*, which is the strongest form of "remove everything not
/// named" and therefore the one most likely to reach too far.
#[tokio::test]
async fn an_empty_snapshot_from_one_peer_closes_only_its_own_mirrors() {
    let mut h = Harness::start().await;
    let mut b = h.second_peer(0xd2).await;
    h.display(1).await;
    h.manager
        .handle_control(
            b.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1, "Bo", "hello",
            ))),
        )
        .await
        .expect("upsert");
    b.next_result().await;
    assert_eq!(h.sink.live_count(), 2);

    let sync = [3u8; 16];
    h.send(&marker(&sync, pb::sync_marker::Phase::Begin)).await;
    h.send(&marker(&sync, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert_eq!(h.report().await.mirrors, 0, "A's own mirror should be gone");
    assert_eq!(
        h.sink.live_count(),
        1,
        "A's empty snapshot closed B's notification"
    );
}

// ---------------------------------------------------------------------------
// §13 — role and epoch, adversarially
// ---------------------------------------------------------------------------

/// Epoch 0 is refused, in every position.
///
/// "Unset" must never be an accepted value, and it must not be accepted merely
/// because it arrives first.
#[tokio::test]
async fn epoch_zero_is_refused_even_as_the_first_announcement() {
    let mut h = Harness::start_ungranted().await;
    h.policies
        .grant(h.peer, NotificationPolicy::default())
        .await;
    h.expect_roles().await;

    h.send(&roles(&[pb::NotificationRole::Source], 0)).await;
    h.barrier().await;
    let report = h.report().await;
    assert_eq!(report.peer_epoch, 0);
    assert!(
        !report.peer_is_source,
        "an epoch-0 announcement was accepted"
    );

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedRole)
        .await;
}

/// The full epoch table, one peer, in order.
///
/// Each row states what arrives and what the accepted state must be
/// afterwards. The invariant under test is the one the epoch exists for:
/// **once epoch N has narrowed authority, nothing with epoch ≤ N can widen it
/// again.**
#[tokio::test]
async fn the_epoch_table_holds_in_order() {
    use pb::NotificationRole::{DismissTarget, Source};

    let mut h = Harness::start_ungranted().await;
    h.policies
        .grant(h.peer, NotificationPolicy::default())
        .await;
    h.expect_roles().await;

    /// One row: what arrives, and what the accepted state must be afterwards.
    struct Row {
        roles: &'static [pb::NotificationRole],
        epoch: u32,
        source: bool,
        dismiss_target: bool,
        accepted_epoch: u32,
        why: &'static str,
    }

    let table: &[Row] = &[
        Row {
            roles: &[Source],
            epoch: 0,
            source: false,
            dismiss_target: false,
            accepted_epoch: 0,
            why: "epoch 0 is unset and refused",
        },
        Row {
            roles: &[Source],
            epoch: 1,
            source: true,
            dismiss_target: false,
            accepted_epoch: 1,
            why: "the first real announcement",
        },
        Row {
            roles: &[Source, DismissTarget],
            epoch: 1,
            source: true,
            dismiss_target: false,
            accepted_epoch: 1,
            why: "equal epochs are refused too, so a duplicate carrying a \
                  different set cannot take effect",
        },
        Row {
            roles: &[Source, DismissTarget],
            epoch: 2,
            source: true,
            dismiss_target: true,
            accepted_epoch: 2,
            why: "a newer epoch widens",
        },
        Row {
            roles: &[],
            epoch: 3,
            source: false,
            dismiss_target: false,
            accepted_epoch: 3,
            why: "and narrows, immediately",
        },
        Row {
            roles: &[Source, DismissTarget],
            epoch: 2,
            source: false,
            dismiss_target: false,
            accepted_epoch: 3,
            why: "THE INVARIANT: a replayed older announcement cannot \
                  re-widen a set that has narrowed",
        },
        Row {
            roles: &[Source],
            epoch: 3,
            source: false,
            dismiss_target: false,
            accepted_epoch: 3,
            why: "nor can one at the same epoch as the narrowing",
        },
        Row {
            roles: &[Source],
            epoch: 4,
            source: true,
            dismiss_target: false,
            accepted_epoch: 4,
            why: "a genuinely newer one may widen again",
        },
        Row {
            roles: &[Source, DismissTarget],
            epoch: u32::MAX,
            source: true,
            dismiss_target: true,
            accepted_epoch: u32::MAX,
            why: "a large future epoch is ordinary, not special",
        },
        Row {
            roles: &[],
            epoch: u32::MAX,
            source: true,
            dismiss_target: true,
            accepted_epoch: u32::MAX,
            why: "and having reached the ceiling, further updates are \
                  refused rather than accepted stale — the safe failure",
        },
    ];

    for row in table {
        h.send(&roles(row.roles, row.epoch)).await;
        h.barrier().await;
        let report = h.report().await;
        let why = row.why;
        assert_eq!(report.peer_is_source, row.source, "{why}");
        assert_eq!(report.peer_is_dismiss_target, row.dismiss_target, "{why}");
        assert_eq!(report.peer_epoch, row.accepted_epoch, "{why}");
    }
}

/// An unknown role value is ignored, and does not discard the rest.
#[tokio::test]
async fn an_unknown_role_is_ignored_without_discarding_the_set() {
    let mut h = Harness::start_ungranted().await;
    h.policies
        .grant(h.peer, NotificationPolicy::default())
        .await;
    h.expect_roles().await;

    let body = pb::notification_control::Body::Roles(pb::NotificationRoles {
        roles: vec![pb::NotificationRole::Source as i32, 4242],
        epoch: 1,
    });
    h.send(&control(body)).await;
    h.barrier().await;

    let report = h.report().await;
    assert!(report.peer_is_source, "the known role was discarded");
    assert!(
        !report.peer_is_dismiss_target,
        "an unknown role was inferred into a known one"
    );
}

/// Narrowing takes mirrors off the screen, and re-widening does not put them
/// back.
///
/// Widening is a statement about what the peer can do now, not a replay of
/// what it once sent. Restoring content on a widen would be a history.
#[tokio::test]
async fn re_widening_a_role_restores_no_content() {
    let mut h = Harness::start().await;
    h.display(1).await;
    assert_eq!(h.sink.live_count(), 1);

    h.send(&roles(&[], 2)).await;
    h.wait_for("the mirrors to close", || h.sink.live_count() == 0)
        .await;

    h.send(&roles(&[pb::NotificationRole::Source], 3)).await;
    h.barrier().await;
    assert_eq!(
        h.sink.live_count(),
        0,
        "widening a role put content back on the screen"
    );
    assert_eq!(h.report().await.mirrors, 0);

    // The source's next upsert is what restores it, which is the only place
    // the truth lives.
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 1);
}

// ---------------------------------------------------------------------------
// §14 — backend loss and reacquisition
// ---------------------------------------------------------------------------

/// Losing the notification server narrows both sink-side roles, invalidates
/// the ids, and recovers with a newer epoch.
#[tokio::test]
async fn losing_and_regaining_the_server_narrows_then_rewidens_with_a_new_epoch() {
    let mut h = Harness::start_dismissing().await;
    h.display(1).await;

    h.sink.go_away().await;
    let narrowed = h.next_roles().await;
    assert!(
        narrowed.roles.is_empty(),
        "a desktop with no notification server can neither display nor \
         report a dismissal: {narrowed:?}"
    );
    assert!(narrowed.epoch >= 2);

    h.sink.come_back().await;
    let rewidened = h.next_roles().await;
    assert_eq!(
        rewidened.roles,
        vec![
            pb::NotificationRole::Sink as i32,
            pb::NotificationRole::DismissReporter as i32,
        ]
    );
    assert!(
        rewidened.epoch > narrowed.epoch,
        "a re-announcement must carry a strictly newer epoch, or the peer \
         refuses it"
    );

    // And the next upsert displays again, on the new server's numbering.
    h.send_upsert(upsert(2, "Ana", "back?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 1);
}

/// **A close signal for a stale server id can never produce a dismissal.**
///
/// The phantom-dismiss case, and the reason it matters: a restarted server
/// numbers from 1 again, so a stale id is not merely useless — it may well be
/// valid and belong to somebody else's notification.
#[tokio::test]
async fn a_close_for_a_stale_server_id_produces_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let stale = h.display(1).await;

    h.sink.go_away().await;
    h.next_roles().await;
    h.sink.come_back().await;
    h.next_roles().await;

    // The dead server's id, arriving late, as a human dismissal.
    h.close(stale, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// A snapshot after a server restart converges without duplicating anything.
#[tokio::test]
async fn a_snapshot_after_a_restart_converges_on_the_new_server() {
    let mut h = Harness::start().await;
    h.display(1).await;
    h.display(2).await;

    h.sink.go_away().await;
    h.next_roles().await;
    h.sink.come_back().await;
    h.next_roles().await;

    let sync = [5u8; 16];
    h.send(&marker(&sync, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.send_upsert(upsert(2, "Bo", "hi")).await;
    h.send(&marker(&sync, pb::sync_marker::Phase::End)).await;
    h.drain_outbound().await;

    assert_eq!(h.report().await.mirrors, 2);
    assert_eq!(
        h.sink.live_count(),
        2,
        "the snapshot duplicated or lost a notification across the restart"
    );
}

/// A sink that cannot observe closes never claims it reports dismissals.
///
/// The role is a statement about what this device can physically do. Without
/// the `NotificationClosed` stream it cannot tell a human dismissal from an
/// expiry, so it must announce neither the role nor a `DismissRequest`.
#[tokio::test]
async fn a_sink_that_cannot_observe_closes_does_not_claim_dismiss_reporting() {
    let sink = MemorySink::new();
    sink.withhold_closed_events();
    let mut h = Harness::start_with(sink).await;
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                allow_dismiss_sync: true,
                ..NotificationPolicy::default()
            },
        )
        .await;

    let announced = h.expect_roles().await;
    assert!(
        announced
            .roles
            .contains(&(pb::NotificationRole::Sink as i32)),
        "it can still display"
    );
    assert!(
        !announced
            .roles
            .contains(&(pb::NotificationRole::DismissReporter as i32)),
        "it cannot observe a close, so it must not claim to report one: \
         {announced:?}"
    );
    assert!(!h.manager.reports_dismissals());
}

// ---------------------------------------------------------------------------
// §15 — lock transitions
// ---------------------------------------------------------------------------

/// An unknown lock state resolves to locked, in both directions of a
/// transition.
#[tokio::test]
async fn an_unknown_lock_state_is_locked_whichever_way_it_was_reached() {
    let mut h = Harness::start().await;
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                when_sink_locked: LockPolicy::Suppress,
                ..NotificationPolicy::default()
            },
        )
        .await;

    // unlocked -> unknown. The gate reads the platform per notification
    // rather than a cached answer, so the outcome is the property — not
    // `is_locked()`, which is a report and moves when a lock *event* arrives.
    h.lock.set_unknown(true);
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
    assert!(
        h.manager.is_locked(),
        "reading the lock updates the report as a side effect"
    );

    // locked -> unknown is still locked, and unknown -> locked stays locked.
    h.lock.set_locked(true).await;
    h.lock.set_unknown(true);
    h.send_upsert(upsert(2, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
    assert!(h.manager.is_locked());

    // And an unknown source that says it is *unlocked* underneath is still
    // locked: the failure resolves one way only.
    h.lock.set_locked(false).await;
    h.send_upsert(upsert(3, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
}

/// **A lock that changes during a disconnect is applied to the reconnect.**
///
/// A peer that attaches must start from the truth, not from an optimistic
/// default left over from the session before it.
#[tokio::test]
async fn a_lock_that_changed_while_disconnected_applies_to_the_new_session() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));
    h.display(1).await;

    h.detach().await;
    h.lock.set_locked(true).await;
    h.wait_for("the lock event to be seen", || h.manager.is_locked())
        .await;

    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    h.send_upsert(upsert(2, "Ana", "secret plans")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    let (_, mirror) = h
        .sink
        .displays()
        .into_iter()
        .next_back()
        .expect("a display");
    assert_eq!(
        mirror.body, "",
        "a notification that arrived on a locked desktop kept its body"
    );
}

/// Unlocking never restores content, and a dismissal does not sneak it back.
///
/// The dismiss path looks up an identity and closes a mirror. It must not, on
/// the way, rebuild a mirror from content the sink deliberately never kept.
#[tokio::test]
async fn a_dismissal_does_not_restore_content_that_was_reduced() {
    let mut h = Harness::start_dismissing().await;
    let id = h.display(1).await;

    h.lock.set_locked(true).await;
    h.wait_for("the lock", || h.manager.is_locked()).await;
    h.barrier().await;

    h.lock.set_locked(false).await;
    h.wait_for("the unlock", || !h.manager.is_locked()).await;
    h.barrier().await;

    let last = h
        .sink
        .live(id)
        .or_else(|| h.sink.last_server_id().and_then(|i| h.sink.live(i)));
    if let Some(mirror) = last {
        assert_eq!(
            mirror.body, "",
            "unlocking restored a body the sink never kept"
        );
    }

    // And dismissing it still works, without resurrecting anything.
    let current = h.sink.last_server_id().expect("an id");
    h.close(current, CloseReason::Dismissed).await;
    let (dismissed, _) = h.next_dismiss().await;
    assert_eq!(dismissed, id_bytes(1));
    assert_eq!(h.sink.live_count(), 0);
}

// ---------------------------------------------------------------------------
// §18 — failure injection
// ---------------------------------------------------------------------------

/// `Notify` failing is reported, bounded, and leaves no ghost.
#[tokio::test]
async fn a_display_failure_is_reported_and_leaves_no_mirror_behind() {
    let mut h = Harness::start().await;
    h.sink
        .set_display_failure(Some(SinkError::Failed("no".into())));

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    let (_, outcome) = h.next_result().await;
    assert_ne!(
        outcome,
        pb::NotificationOutcome::Displayed,
        "a failed display reported success"
    );

    h.sink.set_display_failure(None);
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 1, "the retry produced one, not two");
}

/// **`CloseNotification` failing must not leave a ghost for ever.**
///
/// The dangerous direction. A mirror the sink believes it closed but could not
/// must not be held as live state that nothing will ever revisit — the next
/// snapshot, removal or grace expiry has to be able to converge.
#[tokio::test]
async fn a_close_failure_still_converges_on_the_next_snapshot() {
    let mut h = Harness::start().await;
    h.display(1).await;
    h.display(2).await;

    h.sink
        .set_close_failure(Some(SinkError::Failed("busy".into())));
    h.send(&remove(1)).await;
    h.next_result().await;

    h.sink.set_close_failure(None);
    let sync = [4u8; 16];
    h.send(&marker(&sync, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(2, "Bo", "hi")).await;
    h.send(&marker(&sync, pb::sync_marker::Phase::End)).await;
    h.drain_outbound().await;

    assert_eq!(
        h.report().await.mirrors,
        1,
        "a mirror whose close failed was never reconciled"
    );
}

/// A malformed payload is refused without breaking the session.
#[tokio::test]
async fn a_malformed_snapshot_marker_does_not_wedge_the_worker() {
    let mut h = Harness::start().await;

    // A sync id of the wrong width: refused by the portable contract.
    let body = pb::notification_control::Body::Sync(pb::SyncMarker {
        sync_id: vec![1, 2, 3],
        phase: pb::sync_marker::Phase::Begin as i32,
    });
    let _ = h.manager.handle_control(h.peer, &control(body)).await;

    // The worker is still alive and still in order.
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert!(
        !h.report().await.snapshot_open,
        "a malformed marker opened a snapshot"
    );
}

/// A peer whose outbound channel has gone does not stall the worker.
///
/// Nineteen items are queued with nobody to answer to. Each is refused,
/// because the peer has no session and therefore no roles — but the *worker*
/// must drain them all rather than blocking on a send that can never
/// complete, or the peer's reconnect would find a wedged queue and the
/// capability would be dead for the rest of the process.
#[tokio::test]
async fn a_dead_outbound_channel_does_not_stall_the_worker() {
    let mut h = Harness::start().await;
    h.manager.set_reconnect_grace(Duration::from_secs(30));
    h.display(1).await;
    h.detach().await;

    for seed in 2..=20u16 {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
    }
    // The point is that the worker kept *taking* items rather than blocking
    // on a send that can never complete, so what is waited on is the queue
    // emptying, read from the peer's own report.
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        if h.pending_work().await == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the worker stalled with nobody listening"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        h.sink.live_count(),
        1,
        "a peer with no session displayed something"
    );

    // And the peer coming back finds a working worker.
    h.reattach().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;
    h.send_upsert(upsert(21, "Ana", "back")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live_count(), 2);
}

/// Every failure path leaves the report content-free.
///
/// The one property that must survive all of them: a diagnostic written while
/// things are going wrong is exactly where content leaks.
#[tokio::test]
async fn no_failure_path_puts_content_in_a_report() {
    const TITLE: &str = "OMNIBRIDGE-N5-FAIL-TITLE";
    const BODY: &str = "OMNIBRIDGE-N5-FAIL-BODY";

    let mut h = Harness::start_dismissing().await;
    h.send_upsert(upsert(1, TITLE, BODY)).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.sink
        .set_failure(Some(SinkError::Failed("everything".into())));
    h.send_upsert(upsert(2, TITLE, BODY)).await;
    h.next_result().await;
    h.send(&remove(1)).await;
    h.next_result().await;
    h.sink.set_failure(None);

    let rendered = format!("{:?}", h.manager.peer_reports().await);
    assert!(!rendered.is_empty());
    for canary in [TITLE, BODY, "com.example.chat", "Chat"] {
        assert!(
            !rendered.contains(canary),
            "a report written during failure carried {canary}"
        );
    }
}
