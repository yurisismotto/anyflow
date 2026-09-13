//! Dismissal synchronisation: what may travel to the phone, and what may not.
//!
//! The rule this file exists to hold in place is one sentence long:
//!
//! > **A human closing a mirrored notification on this desktop is the only
//! > thing that may dismiss the original on the source device.**
//!
//! Everything else here is a way of being wrong about that sentence. A banner
//! timing out is not a human. Our own `CloseNotification` coming back is not a
//! human. A grant being revoked, a screen locking, a snapshot reconciling, a
//! notification server restarting and the per-peer ceiling evicting are not
//! humans either, and each of them closes notifications on a real desktop
//! every day.
//!
//! So most of this suite is negative, and the negative tests are the valuable
//! ones: a bug here does not show up as a failure on this machine, it shows up
//! as somebody's phone quietly clearing itself while they are away from their
//! desk. `expect_no_dismiss` is the assertion that catches it, and it is an
//! *ordering* proof rather than a sleep — it pushes a message through the same
//! single-worker queue and fails if anything comes out ahead of the answer.

mod common;

use anyflow_capability_notifications::backend::{CloseReason, SinkError};
use anyflow_capability_notifications::{limits, LockPolicy, NotificationPolicy};
use anyflow_proto::v1::capabilities as pb;
use common::*;

// ---------------------------------------------------------------------------
// The one thing that works
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_human_dismissal_produces_exactly_one_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.close(server_id, CloseReason::Dismissed).await;

    let (id, origin) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1), "it names the notification that was closed");
    assert_eq!(
        origin, ORIGIN,
        "it names the device the source itself named, because the source \
         requires that it is the origin"
    );

    // And exactly one. A second request would mean the mirror entry survived
    // the first, which is the property `forget_server_id` provides.
    h.expect_no_dismiss().await;
    assert_eq!(h.report().await.dismissals_sent, 1);
}

/// The identity on the wire is the source's own opaque one, and nothing else
/// travels with it. Written as an assertion about the *decoded message* rather
/// than about the code, so adding a field to `DismissRequest` would have to
/// break this test before it could ship.
#[tokio::test]
async fn a_dismiss_request_carries_an_identity_and_nothing_else() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;

    let message = match h.next_outbound().await.body {
        Some(pb::notification_control::Body::Dismiss(d)) => d,
        other => panic!("expected a dismiss, got {other:?}"),
    };
    assert_eq!(message.notification_id.len(), 16);
    assert_eq!(message.origin_device_id.len(), 32);

    // The encoded form is two fields. No action index, no intent, no reply and
    // no free text — because the schema has nowhere to put them.
    let encoded = anyflow_proto::Message::encode_to_vec(&message);
    assert!(
        encoded.len() < 64,
        "a dismiss request is an identity and a device id; {} bytes is not that",
        encoded.len()
    );
}

// ---------------------------------------------------------------------------
// The close reasons that must never travel
// ---------------------------------------------------------------------------

/// The single most important test in the wave.
///
/// A desktop banner times out on its own, every day, on a screen nobody is
/// looking at. If that cleared the phone, the feature would be a bug report.
#[tokio::test]
async fn an_expiry_never_dismisses_anything_on_the_source() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.close(server_id, CloseReason::Expired).await;
    h.expect_no_dismiss().await;
    assert_eq!(h.report().await.dismissals_sent, 0);
}

#[tokio::test]
async fn a_programmatic_close_never_dismisses_anything_on_the_source() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    // Reason 3 is our own `CloseNotification` returning to us. Acting on it
    // would be an immediate self-inflicted loop.
    h.close(server_id, CloseReason::Closed).await;
    h.expect_no_dismiss().await;
}

#[tokio::test]
async fn an_undefined_close_reason_never_dismisses_anything() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    // Reason 4, and anything a future server invents, resolves here. A reason
    // that carries no information cannot justify an action on another device.
    h.close(server_id, CloseReason::Undefined).await;
    h.expect_no_dismiss().await;
}

/// The exhaustive form of the three tests above: whatever the four reasons
/// are, exactly one of them may travel.
#[tokio::test]
async fn only_one_of_the_four_close_reasons_can_dismiss() {
    for reason in [
        CloseReason::Expired,
        CloseReason::Closed,
        CloseReason::Undefined,
    ] {
        let mut h = Harness::start_dismissing().await;
        let server_id = h.display(1).await;
        h.close(server_id, reason).await;
        h.expect_no_dismiss().await;
    }

    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));
}

// ---------------------------------------------------------------------------
// Every other way a mirror comes off this screen
// ---------------------------------------------------------------------------

/// A `NotificationRemove` from the phone closes the mirror here, and the close
/// signal that produces must not travel back as a dismissal for a notification
/// the phone has already removed.
#[tokio::test]
async fn a_source_removal_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.send(&remove(1)).await;
    h.expect_outcome(pb::NotificationOutcome::Removed).await;

    // On a real server the close we just performed comes back as a signal.
    // The entry is already gone, so it finds nothing.
    h.close(server_id, CloseReason::Closed).await;
    // And even if a server reported it as a human dismissal — which GNOME does
    // not, but a spec-loose one might — there is no entry to act on.
    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

#[tokio::test]
async fn a_lock_suppression_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                allow_dismiss_sync: true,
                when_sink_locked: LockPolicy::Suppress,
                ..NotificationPolicy::default()
            },
        )
        .await;
    let server_id = h.display(1).await;

    h.manager.set_locked(true).await;
    h.wait_for("the lock to take the mirror off the screen", || {
        h.sink.live(server_id).is_none()
    })
    .await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

#[tokio::test]
async fn a_grant_revocation_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.policies.revoke(&h.peer).await;
    h.manager.revoke_peer(&h.peer).await;
    h.wait_for("the revocation to close the mirror", || {
        h.sink.live(server_id).is_none()
    })
    .await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

#[tokio::test]
async fn a_role_narrowing_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    // The phone's notification access was revoked: it announces an empty set.
    h.send(&roles(&[], 2)).await;
    h.wait_for("the narrowing to close the mirror", || {
        h.sink.live(server_id).is_none()
    })
    .await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// A GNOME Shell restart reissues its numbering from 1, so a stale id may well
/// be valid again and belong to somebody else's notification. Dismissing the
/// phone's notification because a *different* application's banner closed
/// would be the worst failure in this file.
#[tokio::test]
async fn a_backend_restart_makes_a_stale_close_harmless() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.sink.go_away().await;
    // `set_available(true)` is what drops every stale handle, and it does so
    // before it returns — so by the next line the identities are still here
    // and the numbers are not.
    h.manager.set_available(false).await;
    h.manager.set_available(true).await;
    h.sink.come_back().await;

    // The old number is live again on the restarted server, and belongs to
    // somebody else's notification.
    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

#[tokio::test]
async fn a_mirror_evicted_at_the_ceiling_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let first = h.display(1).await;

    for seed in 2..=(limits::MAX_MIRRORS_PER_PEER as u16) {
        h.send_upsert(upsert(seed, "Ana", "lunch?")).await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    }
    // One more than the ceiling: the oldest is evicted and closed.
    h.send_upsert(upsert(9000, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.close(first, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// A snapshot that does not name a mirror removes it. That is the source
/// saying it no longer has the notification, so asking the source to dismiss
/// it would be asking it to remove something it just said it does not have.
#[tokio::test]
async fn a_snapshot_reconciliation_generates_no_dismiss_request() {
    let mut h = Harness::start_dismissing().await;
    let stale = h.display(1).await;

    let sync_id = vec![7u8; 16];
    h.send(&marker(&sync_id, pb::sync_marker::Phase::Begin))
        .await;
    h.send_upsert(upsert(2, "Bo", "hello")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send(&marker(&sync_id, pb::sync_marker::Phase::End)).await;
    h.wait_for("the reconciliation to close the unnamed mirror", || {
        h.sink.live(stale).is_none()
    })
    .await;

    h.close(stale, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// A close for an id nothing was ever displayed under. On a shared session
/// this is every other application's notification closing.
#[tokio::test]
async fn a_close_for_an_id_this_desktop_never_used_generates_nothing() {
    let mut h = Harness::start_dismissing().await;
    h.display(1).await;

    h.close(4242, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

// ---------------------------------------------------------------------------
// Idempotency and duplicate signals
// ---------------------------------------------------------------------------

/// A duplicate `NotificationClosed` — a server that emits twice, or two pumps
/// racing — must not double-ask. The entry is consumed by the first.
#[tokio::test]
async fn a_duplicate_close_signal_sends_at_most_one_request() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.close(server_id, CloseReason::Dismissed).await;
    h.close(server_id, CloseReason::Dismissed).await;

    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));
    h.expect_no_dismiss().await;
    assert_eq!(h.report().await.dismissals_sent, 1);
}

/// The echo. The source cancels, observes its own cancellation, and — if it
/// did not suppress the echo, or if it sends the removal to a *different* peer
/// that shares the mirror — a `NotificationRemove` arrives for a mirror that is
/// already gone. It must converge silently and start nothing.
#[tokio::test]
async fn the_source_removal_after_a_dismissal_converges_silently() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.close(server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));

    // The source answers, and then removes.
    h.send(&result(1, pb::NotificationOutcome::Removed)).await;
    h.send(&remove(1)).await;

    // `UNKNOWN_NOTIFICATION`, because the mirror was purged before the request
    // was ever sent. An answer, not an error — and not a second dismissal.
    h.expect_outcome(pb::NotificationOutcome::UnknownNotification)
        .await;
    h.expect_no_dismiss().await;
    assert_eq!(
        h.report().await.dismissals_sent,
        1,
        "the loop has one lap and no second"
    );
}

/// The same identity, dismissed twice, with the source re-posting in between.
/// The second dismissal must name the notification that is on the screen now,
/// which is the same identity with a new server id.
#[tokio::test]
async fn a_reposted_notification_dismisses_its_current_identity() {
    let mut h = Harness::start_dismissing().await;
    let first_server_id = h.display(1).await;

    h.close(first_server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));

    // The app posts again under the same identity. `replaces_id` names an id
    // the server has invalidated, so a fresh notification is created.
    h.send_upsert(upsert(1, "Ana", "still lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    let second_server_id = h.sink.last_server_id().expect("displayed again");
    assert_ne!(
        second_server_id, first_server_id,
        "a closed freedesktop id is dead and may not be reused"
    );

    // A late signal for the *old* number must not dismiss the new one.
    h.close(first_server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;

    // And the current one still can.
    h.close(second_server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));
    assert_eq!(h.report().await.dismissals_sent, 2);
}

/// An update replaces in place and keeps its server id, so a dismissal after
/// an update still names the right identity.
#[tokio::test]
async fn a_dismissal_after_an_update_names_the_same_identity() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.send_upsert(upsert(1, "Ana", "lunch at one?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.close(server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));
}

/// A peer that disconnects between the dismissal and the send gets nothing.
/// A dismiss for an offline peer is harmless: dropped, never queued
/// (ADR-0015 §6).
#[tokio::test]
async fn a_dismissal_for_a_departed_peer_is_dropped_and_not_queued() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.manager.detach_session(&h.peer).await;
    h.close(server_id, CloseReason::Dismissed).await;

    // Reconnect and look. If the dismissal had been *queued* rather than
    // dropped, this is the session it would arrive on — which is precisely
    // the behaviour ADR-0015 §6 forbids, because by now the phone's own
    // snapshot is the truth about what is in its shade.
    h.reattach().await;
    h.expect_roles().await;
    h.expect_no_dismiss().await;
    assert_eq!(h.report().await.dismissals_sent, 0);
}

// ---------------------------------------------------------------------------
// The four gates
// ---------------------------------------------------------------------------

/// The default. Nothing in a fresh install sends a dismissal to anybody.
///
/// Every *other* gate is deliberately open here — the peer announces
/// `DISMISS_TARGET`, this desktop reports dismissals, the session is up and a
/// real human close happens — so that the policy is the only thing left
/// holding it, and this test fails if that is the thing that breaks. A version
/// of it that started from `Harness::start()` passed for the wrong reason:
/// the peer had never claimed `DISMISS_TARGET`, so the role gate was doing the
/// work and the policy was never consulted.
#[tokio::test]
async fn dismiss_sync_is_off_by_default_and_sends_nothing() {
    assert!(
        !NotificationPolicy::default().allow_dismiss_sync,
        "the stored default is off"
    );

    let mut h = Harness::start_ungranted().await;
    // The stored default, verbatim: granted, mirroring, dismiss sync off.
    h.policies
        .grant(h.peer, NotificationPolicy::default())
        .await;
    h.expect_roles().await;
    h.send(&roles(
        &[
            pb::NotificationRole::Source,
            pb::NotificationRole::DismissTarget,
        ],
        1,
    ))
    .await;

    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
    assert_eq!(h.report().await.dismissals_sent, 0);
}

#[tokio::test]
async fn a_peer_that_never_claimed_dismiss_target_is_never_asked() {
    let mut h = Harness::start_ungranted().await;
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                allow_dismiss_sync: true,
                ..NotificationPolicy::default()
            },
        )
        .await;
    h.expect_roles().await;
    // `SOURCE` alone: an older phone, or one whose listener cannot cancel.
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// Turning mirroring off makes a stale `allow_dismiss_sync` inert, the same
/// containment rule `ClipboardPolicy::may_auto_send` applies.
#[tokio::test]
async fn mirroring_off_defeats_a_stale_dismiss_sync_flag() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                allow_mirror: false,
                allow_dismiss_sync: true,
                ..NotificationPolicy::default()
            },
        )
        .await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// The grant is re-read at the moment the dismissal would be sent, not reused
/// from the upsert that put the notification on the screen.
#[tokio::test]
async fn a_grant_revoked_after_the_upsert_stops_the_dismissal() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;

    h.policies.revoke(&h.peer).await;

    h.close(server_id, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}

/// A desktop whose notification server cannot report why a notification closed
/// announces no `DISMISS_REPORTER`, and therefore asks nothing of anybody —
/// even with both policies on and the peer claiming `DISMISS_TARGET`.
#[tokio::test]
async fn a_desktop_that_cannot_report_dismissals_announces_no_reporter_role() {
    use anyflow_capability_notifications::backend::{
        LockSource, MemoryLock, MemorySink, NotificationSink, SinkCapabilities,
    };
    use anyflow_capability_notifications::{NotificationAuthorizer, NotificationManager};
    use std::sync::Arc;

    let sink = Arc::new(MemorySink::new());
    sink.set_capabilities(SinkCapabilities {
        body_markup: true,
        body: true,
        persistence: true,
        dismiss_reporting: false,
    });
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

    assert!(!manager.reports_dismissals());

    let (sender, mut outbound) = tokio::sync::mpsc::channel(64);
    let peer = fingerprint(0xcd);
    policies
        .grant(
            peer,
            NotificationPolicy {
                allow_dismiss_sync: true,
                ..NotificationPolicy::default()
            },
        )
        .await;
    manager.attach_session(peer, sender).await;

    let announced = tokio::time::timeout(TIMEOUT, outbound.recv())
        .await
        .expect("announced within the timeout")
        .expect("a message");
    let control =
        <pb::NotificationControl as anyflow_proto::Message>::decode(announced.payload.as_slice())
            .expect("decodes");
    let roles = match control.body {
        Some(pb::notification_control::Body::Roles(r)) => r,
        other => panic!("expected roles, got {other:?}"),
    };
    assert_eq!(
        roles.roles,
        vec![pb::NotificationRole::Sink as i32],
        "SINK alone: it can display, and it cannot say why anything closed"
    );
}

// ---------------------------------------------------------------------------
// Results, and what they are counted as
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_source_that_refuses_a_dismissal_is_counted_as_a_refusal() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let _ = h.next_dismiss().await;

    // An ongoing notification. The source refuses, and the mirror here stays
    // gone — the person did close it on this screen, and it is the phone that
    // said no.
    h.send(&result(1, pb::NotificationOutcome::NotDismissible))
        .await;

    let report = h.report().await;
    assert_eq!(report.dismissals_sent, 1);
    assert_eq!(report.dismissals_refused, 1);
}

/// Both ends converging on "it is gone" is the correct outcome, and it must
/// not be reported to the operator as the phone having refused.
#[tokio::test]
async fn an_already_gone_answer_is_convergence_and_not_a_refusal() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let _ = h.next_dismiss().await;

    h.send(&result(1, pb::NotificationOutcome::UnknownNotification))
        .await;
    let report = h.report().await;
    assert_eq!(report.dismissals_sent, 1);
    assert_eq!(report.dismissals_refused, 0);
}

#[tokio::test]
async fn a_successful_dismissal_is_not_counted_as_a_refusal() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let _ = h.next_dismiss().await;

    h.send(&result(1, pb::NotificationOutcome::Removed)).await;
    let report = h.report().await;
    assert_eq!(report.dismissals_sent, 1);
    assert_eq!(report.dismissals_refused, 0);
}

/// A peer's own policy refusal is counted, because "I turned it on and my
/// phone is not clearing" has an answer and it is this one.
#[tokio::test]
async fn a_policy_refusal_from_the_source_is_counted() {
    let mut h = Harness::start_dismissing().await;
    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let _ = h.next_dismiss().await;

    h.send(&result(1, pb::NotificationOutcome::RejectedPolicy))
        .await;
    assert_eq!(h.report().await.dismissals_refused, 1);
}

// ---------------------------------------------------------------------------
// This desktop is still not a dismiss target
// ---------------------------------------------------------------------------

/// N4 makes this desktop a dismiss *reporter*. The inbound half stays refused,
/// and the two are separate roles precisely so that implementing one cannot
/// quietly implement the other.
#[tokio::test]
async fn an_inbound_dismiss_request_is_still_refused() {
    let mut h = Harness::start_dismissing().await;
    h.display(1).await;

    h.send(&dismiss(1)).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedRole)
        .await;

    // And the mirror is untouched: refusing a dismiss must not close anything.
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn an_inbound_dismiss_from_an_ungranted_peer_is_not_authorized() {
    let mut h = Harness::start_dismissing().await;
    h.policies.revoke(&h.peer).await;

    h.send(&dismiss(1)).await;
    h.expect_outcome(pb::NotificationOutcome::NotAuthorized)
        .await;
}

/// Announcing `DISMISS_REPORTER` must not be read anywhere as announcing
/// `DISMISS_TARGET`. This is the assertion that catches a future edit adding
/// the second to the same set.
#[tokio::test]
async fn this_desktop_announces_reporter_and_never_target() {
    let mut h = Harness::start_ungranted().await;
    let announcement = h.expect_roles().await;
    assert!(announcement
        .roles
        .contains(&(pb::NotificationRole::Sink as i32)));
    assert!(announcement
        .roles
        .contains(&(pb::NotificationRole::DismissReporter as i32)));
    assert!(
        !announcement
            .roles
            .contains(&(pb::NotificationRole::DismissTarget as i32)),
        "Linux sources nothing; there is nothing here for a peer to dismiss"
    );
    assert!(
        !announcement
            .roles
            .contains(&(pb::NotificationRole::Source as i32)),
        "Linux cannot observe other applications' notifications"
    );
}

// ---------------------------------------------------------------------------
// Two peers
// ---------------------------------------------------------------------------

/// One peer's dismissal reaches that peer alone. There is no relay: a mirror
/// belongs to the fingerprint that sent it, and closing it says nothing to
/// anybody else.
#[tokio::test]
async fn a_dismissal_reaches_only_the_peer_that_sourced_the_notification() {
    let mut h = Harness::start_dismissing().await;
    let mut other = h.second_peer(0x33).await;
    h.policies
        .grant(
            other.peer,
            NotificationPolicy {
                allow_dismiss_sync: true,
                ..NotificationPolicy::default()
            },
        )
        .await;

    let server_id = h.display(1).await;
    h.close(server_id, CloseReason::Dismissed).await;
    let (id, _) = h.next_dismiss().await;
    assert_eq!(id, id_bytes(1));

    // The second peer is sent nothing at all about it.
    other.barrier(&h.manager).await;
    let reports = h.manager.peer_reports().await;
    let second = reports
        .iter()
        .find(|r| r.peer == other.peer)
        .expect("a report");
    assert_eq!(second.dismissals_sent, 0);
}

// ---------------------------------------------------------------------------
// Backend failures
// ---------------------------------------------------------------------------

/// A close signal for a mirror whose `display` failed. There is no server id,
/// so nothing on any screen closed, so no human closed anything.
#[tokio::test]
async fn a_never_displayed_mirror_cannot_be_humanly_dismissed() {
    let mut h = Harness::start_dismissing().await;
    h.sink
        .set_failure(Some(SinkError::Failed("refused".into())));
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Failed).await;
    h.sink.set_failure(None);

    // Whatever number a signal names, it is not this mirror's — it has none.
    h.close(1, CloseReason::Dismissed).await;
    h.expect_no_dismiss().await;
}
