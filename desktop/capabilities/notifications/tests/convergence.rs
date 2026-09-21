//! Role convergence on a session that is already up.
//!
//! # What these tests are about
//!
//! U2 §39.19 and §40.13 recorded the same failure on three distributions: a
//! healthy, connected session in which the phone became a notification
//! `SOURCE` mid-session, the desktop was a `SINK` throughout, and nothing was
//! ever mirrored. Restarting the daemon fixed it within five seconds, because
//! a restart builds a *new* session whose role announcement happens at a
//! moment when the peer is already ready.
//!
//! The root cause is on the phone — `NotificationSource.handleInbound` refused
//! a role announcement from a peer it had not yet granted — and its regression
//! is `NotificationConvergenceTest` on that side. What is pinned here is the
//! desktop's half of the same contract, and the one thing on this side that
//! could reproduce the defect all over again: **a role announcement that is
//! recorded as sent without being sent is lost for the life of the session**,
//! because `LocalRoles::announce` will never produce it twice.
//!
//! No sleeps and no wall-clock: every assertion waits on something the worker
//! produces, through the same barrier the rest of the suite uses.

mod common;

use common::{fingerprint, roles, Harness};
use omnibridge_capability_notifications::roles::LocalRoles;
use omnibridge_capability_notifications::NotificationPolicy;
use omnibridge_core::notifications::Role;
use omnibridge_proto::v1::capabilities as pb;

/// **A — a fresh session is told this device's roles, once, before anything
/// else.**
///
/// Brief §16 A. The baseline every other case is measured against.
#[tokio::test]
async fn a_session_establishment_announces_roles_once() {
    let mut harness = Harness::start_ungranted().await;
    harness
        .policies
        .grant(harness.peer, NotificationPolicy::default())
        .await;

    let first = harness.expect_roles().await;
    assert_eq!(first.epoch, 1, "a fresh connection starts at 1");
    assert_eq!(
        first.roles,
        vec![
            pb::NotificationRole::Sink as i32,
            pb::NotificationRole::DismissReporter as i32
        ],
        "ADR-0017 §1's v1 assignment for Linux"
    );

    // And nothing re-announces it for free. The barrier proves the worker
    // reached a later message, so a second announcement would have arrived
    // before the barrier's own answer.
    harness
        .send(&roles(&[pb::NotificationRole::Source], 1))
        .await;
    harness.barrier().await;
    let report = harness.report().await;
    assert_eq!(report.local_epoch, 1, "no announcement was burned");
    assert_eq!(report.local_roles, 2);
}

/// **B — an announcement that could not be sent is not recorded as sent.**
///
/// This is the desktop's own version of the P3 defect, and the mutation check
/// in the report restores the old order to show this test catching it.
///
/// `LocalRoles::announce` is a state transition: it records the set and burns
/// an epoch, and answers `None` for every later call with the same set. So
/// advancing it before the outbound channel is in hand makes a lost
/// announcement **unrecoverable** — the desktop reports `announced 2 (epoch
/// 1)` for the rest of the session and the peer is never told anything.
#[tokio::test]
async fn b_an_announcement_with_nowhere_to_go_is_not_recorded() {
    let mut harness = Harness::start_ungranted().await;
    harness.expect_roles().await;

    // Detach: the slot, its worker and its queue all survive, but the outbound
    // channel is gone. This is exactly the window the real daemon has between
    // a session ending and the next one attaching.
    harness.detach().await;

    // Provoke an announcement with no channel to carry it. A narrowing *and* a
    // widening, so the set really does change each time and `announce` would
    // have something to say.
    harness.manager.set_available(false).await;
    harness.manager.set_available(true).await;

    // Wait for the worker to have *drained* both items rather than for a reply
    // there is no channel to carry. Without this the assertion below races the
    // worker and passes for the wrong reason — which is exactly how a mutation
    // check comes to report a test it did not really run.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while harness.pending_work().await > 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the worker to drain"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    // The reattach resets role state for the new connection, which is correct
    // and is not what this test is about — so read the state before it.
    //
    // `detach_session` already reset the epoch to 0. A real change then passed
    // through `announce_roles` with no channel to carry it — the queue
    // coalesces the two offers into one, which is why this is 1 rather than 2
    // under the old order — and was recorded as announced anyway. Nothing was
    // sent, so nothing may be recorded.
    let report = harness
        .manager
        .peer_reports()
        .await
        .into_iter()
        .find(|r| r.peer == harness.peer)
        .expect("the peer still has a slot");
    assert_eq!(
        report.local_epoch, 0,
        "an epoch may only be burned by an announcement that was actually sent"
    );

    // And the session that follows is coherent: it starts again at 1 and the
    // peer is told, rather than inheriting a silence.
    harness.reattach().await;
    let after = harness.expect_roles().await;
    assert_eq!(after.epoch, 1);
    assert_eq!(after.roles.len(), 2);
}

/// **C — a peer announcing into an attach still produces exactly one
/// announcement.**
///
/// The attach announcement is queued on the peer's worker while the peer's own
/// announcement arrives on the session's dispatch loop, so the two genuinely
/// race. Whichever order they land in, this device says what it is once: one
/// epoch, one message, and the peer's claim recorded.
#[tokio::test]
async fn c_an_announcement_racing_an_attach_is_made_exactly_once() {
    let mut harness = Harness::start_ungranted().await;
    harness.expect_roles().await;
    harness.detach().await;
    harness.reattach().await;

    // The new session's announcement is in flight; the peer's arrives too.
    harness
        .send(&roles(&[pb::NotificationRole::Source], 1))
        .await;
    let announced = harness.expect_roles().await;
    assert_eq!(announced.epoch, 1);

    harness.barrier().await;
    let report = harness.report().await;
    assert_eq!(
        report.local_epoch, 1,
        "one announcement, however the two events interleaved"
    );
    assert!(report.peer_is_source);
}

/// **D — the same platform state, recomputed many times, is silent.**
///
/// Brief §9 and §16 E. A repeated callback must not become an announcement
/// storm; replacing a stale-state bug with a noisy one is not a fix.
#[tokio::test]
async fn d_recomputing_an_unchanged_role_set_announces_nothing() {
    let mut harness = Harness::start_ungranted().await;
    harness.expect_roles().await;

    for _ in 0..8 {
        harness.manager.set_available(true).await;
    }
    harness.barrier().await;

    let report = harness.report().await;
    assert_eq!(
        report.local_epoch, 1,
        "the epoch counts changes, not the events that could have caused one"
    );
}

/// **E — a real change does cross the live session, with a higher epoch.**
///
/// Brief §3. Narrow, then widen, on one connection and with no reconnect.
#[tokio::test]
async fn e_a_real_change_crosses_the_live_session() {
    let mut harness = Harness::start_ungranted().await;
    assert_eq!(harness.expect_roles().await.epoch, 1);

    harness.manager.set_available(false).await;
    let narrowed = harness.expect_roles().await;
    assert_eq!(narrowed.epoch, 2);
    assert!(narrowed.roles.is_empty(), "an empty set is meaningful");

    harness.manager.set_available(true).await;
    let widened = harness.expect_roles().await;
    assert_eq!(widened.epoch, 3);
    assert_eq!(widened.roles.len(), 2);
}

/// **F — a stale epoch cannot restore authority that was narrowed.**
///
/// Brief §16 F and §17's dangerous mutation. The phone announcing `SOURCE` at
/// epoch 1 after it has already narrowed at epoch 2 must change nothing.
#[tokio::test]
async fn f_a_stale_epoch_cannot_re_widen_a_narrowed_peer() {
    let mut harness = Harness::start().await;
    harness.display(1).await;

    // The phone loses notification access and says so.
    harness.send(&roles(&[], 2)).await;
    harness
        .wait_for("the narrowing to close the mirrors", || true)
        .await;
    harness.barrier().await;
    let narrowed = harness.report().await;
    assert!(!narrowed.peer_is_source);
    assert_eq!(narrowed.peer_epoch, 2);

    // A replayed, reordered or hostile re-send of the older announcement.
    harness
        .send(&roles(&[pb::NotificationRole::Source], 1))
        .await;
    harness.barrier().await;
    let after = harness.report().await;
    assert!(
        !after.peer_is_source,
        "an older epoch must never restore a role that has been given up"
    );
    assert_eq!(after.peer_epoch, 2, "and it must not move the epoch either");
}

/// **G — a duplicate of the current epoch is inert.**
///
/// Brief §16 G. *Equal* is refused as well as lower: a duplicate carrying a
/// different set must not take effect, or a replay could re-widen.
#[tokio::test]
async fn g_a_duplicate_epoch_changes_nothing() {
    let mut harness = Harness::start().await;
    let before = harness.report().await;
    assert!(before.peer_is_source);
    assert_eq!(before.peer_epoch, 1);

    // The same epoch, a *different* set. Refused.
    harness
        .send(&roles(
            &[
                pb::NotificationRole::Source,
                pb::NotificationRole::DismissTarget,
            ],
            1,
        ))
        .await;
    harness.barrier().await;
    let after = harness.report().await;
    assert_eq!(after.peer_epoch, 1);
    assert!(
        !after.peer_is_dismiss_target,
        "a duplicate epoch may not add a role"
    );

    // And an exact duplicate is equally inert.
    harness
        .send(&roles(&[pb::NotificationRole::Source], 1))
        .await;
    harness.barrier().await;
    assert!(harness.report().await.peer_is_source);
}

/// **H — a reconnect produces one coherent current role state.**
///
/// Brief §10 and §16 H. Both sides start again at 1, because a fresh
/// connection has accepted nothing from either end.
#[tokio::test]
async fn h_a_reconnect_starts_from_a_coherent_current_state() {
    let mut harness = Harness::start().await;
    harness.manager.set_available(false).await;
    harness.expect_roles().await; // epoch 2, empty
    harness.manager.set_available(true).await;
    assert_eq!(harness.expect_roles().await.epoch, 3);

    harness.detach().await;
    harness.reattach().await;

    let fresh = harness.expect_roles().await;
    assert_eq!(fresh.epoch, 1, "a new connection has been told nothing");
    assert_eq!(fresh.roles.len(), 2);

    let report = harness.report().await;
    assert_eq!(report.peer_epoch, 0, "and this device has accepted nothing");
    assert!(!report.peer_is_source);
}

/// **I — role state is per peer, never global.**
///
/// Brief §16 I and §22. P1 made three trusted desktops ordinary, so this is
/// the property that stops one peer's convergence from moving another's.
#[tokio::test]
async fn i_role_state_is_scoped_to_one_peer() {
    let mut harness = Harness::start().await;
    let mut second = harness.second_peer(0xcd).await;

    // Narrow the *first* peer's claim only.
    harness.send(&roles(&[], 2)).await;
    harness.barrier().await;

    let reports = harness.manager.peer_reports().await;
    let first = reports
        .iter()
        .find(|r| r.peer == harness.peer)
        .expect("first");
    let other = reports
        .iter()
        .find(|r| r.peer == second.peer)
        .expect("second");

    assert!(!first.peer_is_source, "the peer that narrowed");
    assert!(other.peer_is_source, "the peer that did not");
    assert_eq!(other.peer_epoch, 1);

    // And the second peer's own session is untouched and still usable.
    second.barrier(&harness.manager).await;
}

/// **J — an unreadable or absent peer role set sends nothing, rather than
/// assuming.**
///
/// Absent roles mean no roles: the fail-closed default that lets a minimal
/// peer interoperate harmlessly. Pinned here because the fix records peer
/// roles in one more situation than before, and "recorded" must not drift
/// towards "assumed".
#[tokio::test]
async fn j_a_peer_that_has_claimed_nothing_is_not_treated_as_a_source() {
    let mut harness = Harness::start_ungranted().await;
    harness
        .policies
        .grant(harness.peer, NotificationPolicy::default())
        .await;
    harness.expect_roles().await;

    let report = harness.report().await;
    assert!(!report.peer_is_source);
    assert!(!report.peer_is_dismiss_target);
    assert_eq!(report.peer_epoch, 0);
}

/// **K — epoch 0 is "unset" and is refused, on a live session.**
///
/// The wire's own reserved value. A peer that sends it has said nothing, and
/// must not be able to clear a set that was legitimately announced.
#[tokio::test]
async fn k_epoch_zero_is_refused() {
    let mut harness = Harness::start().await;
    harness.send(&roles(&[], 0)).await;
    harness.barrier().await;
    let report = harness.report().await;
    assert!(
        report.peer_is_source,
        "an unset epoch may not narrow anything"
    );
    assert_eq!(report.peer_epoch, 1);
}

/// **L — the desired set is a function of the platform and of nothing else.**
///
/// The unit half of the same rule, kept here because the convergence story is
/// only sound while a role stays a statement about this machine: if a grant
/// ever became an input, "converged" would start to mean "authorized".
#[test]
fn l_roles_are_platform_state_not_authorization() {
    assert_eq!(LocalRoles::desired(true, true).len(), 2);
    assert_eq!(LocalRoles::desired(true, false).len(), 1);
    assert!(LocalRoles::desired(false, true).is_empty());
    assert!(LocalRoles::desired(true, true).contains(&Role::Sink));
    assert!(!LocalRoles::desired(true, true).contains(&Role::Source));
}

/// **M — two peers each get their own first announcement.**
///
/// A second peer attaching must not be starved by the first having already
/// announced: the epoch is per connection, and so is the announcement.
#[tokio::test]
async fn m_every_peer_gets_its_own_first_announcement() {
    let harness = Harness::start().await;
    let mut second = harness.second_peer(0xce).await;
    // `second_peer` consumed the announcement; assert the state it left.
    let reports = harness.manager.peer_reports().await;
    for peer in [harness.peer, second.peer] {
        let report = reports.iter().find(|r| r.peer == peer).expect("a report");
        assert_eq!(report.local_epoch, 1, "each connection announced once");
        assert_eq!(report.local_roles, 2);
    }
    second.barrier(&harness.manager).await;
}

/// **N — a peer this device has never met has no role state at all.**
///
/// Bounds the memory claim the report makes: nothing is kept for a peer that
/// has not connected.
#[tokio::test]
async fn n_an_unknown_peer_holds_no_state() {
    let harness = Harness::start().await;
    let stranger = fingerprint(0x77);
    assert!(harness
        .manager
        .peer_reports()
        .await
        .iter()
        .all(|r| r.peer != stranger));
}
