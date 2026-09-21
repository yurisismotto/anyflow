//! The sink's rules, against a fake desktop.
//!
//! Deterministic on purpose: every assertion here is about a rule, and a rule
//! that only holds when gnome-shell happens to be running is not a rule. The
//! real D-Bus server is exercised separately, in `real_dbus.rs`.

mod common;

use common::*;
use omnibridge_capability_notifications::backend::{
    CloseReason, SinkCapabilities, SinkError, Urgency,
};
use omnibridge_capability_notifications::{limits, LockPolicy, NotificationPolicy};
use omnibridge_proto::v1::capabilities as pb;

// ---------------------------------------------------------------------------
// Create, update, remove
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_first_upsert_creates_one_desktop_notification() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let displays = h.sink.displays();
    assert_eq!(displays.len(), 1);
    let (replaces, mirror) = &displays[0];
    assert_eq!(*replaces, None, "the first display replaces nothing");
    assert_eq!(mirror.summary, "Ana");
    assert_eq!(mirror.body, "lunch?");
    assert_eq!(mirror.app_name, "Chat");
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn an_update_replaces_the_same_notification_rather_than_adding_one() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send_upsert(upsert(1, "Ana", "lunch at one?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let displays = h.sink.displays();
    assert_eq!(displays.len(), 2);
    assert_eq!(displays[1].0, Some(1), "the second names the first's id");
    assert_eq!(
        h.sink.live_count(),
        1,
        "one Android notification is one desktop notification for its whole life"
    );
    assert_eq!(h.sink.live(1).expect("still there").body, "lunch at one?");
}

#[tokio::test]
async fn a_progress_storm_produces_one_notification_and_no_duplicates() {
    let h = Harness::start().await;
    for percent in 0..=40 {
        h.send_upsert(upsert(1, "Downloading", &format!("{percent}%")))
            .await;
    }

    // A coalesced update is answered by the one that replaced it: a
    // `NotificationResult` echoes the `notification_id`, and every message in
    // this storm carries the same one, so an answer per intermediate state
    // would be indistinguishable noise. What the source needs to know is the
    // verdict on the identity, and it gets it.
    h.wait_for("the storm to settle", || {
        h.sink.live(1).is_some_and(|live| live.body == "40%")
    })
    .await;

    assert_eq!(h.sink.live_count(), 1, "one entry, not forty-one");
    assert!(
        h.sink.displays().len() <= 41,
        "coalescing may reduce the D-Bus calls but must never add one"
    );
    assert_eq!(
        h.sink.live(1).expect("still there").body,
        "40%",
        "and the one entry shows the newest state"
    );
}

#[tokio::test]
async fn an_identical_resend_is_a_duplicate_and_touches_no_desktop() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;

    assert_eq!(
        h.sink.displays().len(),
        1,
        "no D-Bus call, no visual change, no re-alert"
    );
}

#[tokio::test]
async fn the_sink_never_recomputes_content_hash() {
    let mut h = Harness::start().await;

    // Content that changed, carrying a digest that did not. A sink that
    // recomputed the hash would notice the mismatch and redisplay; one that
    // treats the digest as the source's opaque de-duplication value answers
    // DUPLICATE — which is the contract N1 and N2 actually have.
    let first = upsert(1, "Ana", "lunch?");
    h.send_upsert(first.clone()).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let mut lying = upsert(1, "Ana", "COMPLETELY DIFFERENT TEXT");
    lying.content_hash = first.content_hash.clone();
    h.send_upsert(lying).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;

    assert_eq!(h.sink.displays().len(), 1);

    // And the converse: identical content with a different digest redisplays,
    // because the sink believes the digest and not its own reading of the text.
    let mut same_text = upsert(1, "Ana", "lunch?");
    same_text.content_hash = vec![0xAB; 32];
    h.send_upsert(same_text).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays().len(), 2);
}

#[tokio::test]
async fn an_upsert_with_no_content_hash_is_always_displayed() {
    // `content_hash` is optional on the wire. Absent means the source is not
    // offering a de-duplication value, and inventing one here would be
    // recomputing it by another name.
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "lunch?");
    message.content_hash.clear();

    h.send_upsert(message.clone()).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    assert_eq!(h.sink.displays().len(), 2);
    assert_eq!(h.sink.live_count(), 1, "still one notification");
}

#[tokio::test]
async fn a_timestamp_only_change_does_not_redisplay() {
    // The source excludes `posted_at_unix_ms` from its digest precisely so an
    // identical re-post collapses. The sink honours that by comparing the
    // digest rather than the message.
    let mut h = Harness::start().await;
    let first = upsert(1, "Ana", "lunch?");
    h.send_upsert(first.clone()).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let mut later = first.clone();
    later.posted_at_unix_ms += 60_000;
    h.send_upsert(later).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;
    assert_eq!(h.sink.displays().len(), 1);
}

#[tokio::test]
async fn removing_a_known_mirror_closes_it() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&remove(1)).await;
    h.expect_outcome(pb::NotificationOutcome::Removed).await;

    assert_eq!(h.sink.closes(), vec![1]);
    assert_eq!(h.sink.live_count(), 0);
}

#[tokio::test]
async fn removing_an_unknown_mirror_converges_rather_than_failing() {
    let mut h = Harness::start().await;
    h.send(&remove(99)).await;
    h.expect_outcome(pb::NotificationOutcome::UnknownNotification)
        .await;
    assert!(h.sink.closes().is_empty(), "nothing was closed");
}

#[tokio::test]
async fn removal_is_idempotent() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&remove(1)).await;
    h.expect_outcome(pb::NotificationOutcome::Removed).await;
    h.send(&remove(1)).await;
    h.expect_outcome(pb::NotificationOutcome::UnknownNotification)
        .await;
    h.send(&remove(1)).await;
    h.expect_outcome(pb::NotificationOutcome::UnknownNotification)
        .await;

    assert_eq!(h.sink.closes(), vec![1], "exactly one close ever happened");
}

// ---------------------------------------------------------------------------
// Peer namespacing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_peers_sending_the_same_identity_do_not_collide() {
    let mut h = Harness::start().await;
    let mut other = h.second_peer(0xcd).await;

    h.send_upsert(upsert(1, "Ana", "from the phone")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.manager
        .handle_control(
            other.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                1,
                "Bea",
                "from the tablet",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = other.next_result().await;
    assert_eq!(outcome, pb::NotificationOutcome::Displayed);

    assert_eq!(
        h.sink.live_count(),
        2,
        "the same 16 bytes from two peers are two notifications"
    );
}

#[tokio::test]
async fn one_peer_cannot_remove_another_peers_mirror() {
    let mut h = Harness::start().await;
    let mut other = h.second_peer(0xcd).await;

    h.send_upsert(upsert(1, "Ana", "private")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    // The second peer replays the first peer's identity verbatim.
    h.manager
        .handle_control(other.peer, &remove(1))
        .await
        .expect("remove");
    let (_, outcome) = other.next_result().await;
    assert_eq!(
        outcome,
        pb::NotificationOutcome::UnknownNotification,
        "the replay reached its own namespace, which is empty"
    );

    assert!(h.sink.closes().is_empty());
    assert_eq!(h.sink.live_count(), 1, "the first peer's mirror is intact");
}

#[tokio::test]
async fn a_spoofed_origin_device_id_changes_nothing() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "lunch?");
    message.origin_device_id = "f".repeat(32);
    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    // Removing it names a different (also spoofed) origin. The mirror is keyed
    // on the pinned fingerprint, so the field is display data either way.
    h.send(&control(pb::notification_control::Body::Remove(
        pb::NotificationRemove {
            notification_id: id_bytes(1),
            origin_device_id: "a".repeat(32),
        },
    )))
    .await;
    h.expect_outcome(pb::NotificationOutcome::Removed).await;
    assert_eq!(h.sink.closes(), vec![1]);
}

// ---------------------------------------------------------------------------
// Grants and roles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_ungranted_peer_displays_nothing() {
    let mut h = Harness::start_ungranted().await;
    h.expect_roles().await;
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::NotAuthorized)
        .await;

    assert!(h.sink.displays().is_empty(), "nothing reached the desktop");
}

#[tokio::test]
async fn revoking_a_grant_stops_notifications_immediately_and_closes_the_mirrors() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.policies.revoke(&h.peer).await;
    h.manager.revoke_peer(&h.peer).await;

    h.send_upsert(upsert(2, "Ana", "still here?")).await;
    h.expect_outcome(pb::NotificationOutcome::NotAuthorized)
        .await;

    h.wait_for("the revoked peer's mirrors to close", || {
        h.sink.closes() == vec![1]
    })
    .await;
    assert_eq!(h.sink.live_count(), 0);
    assert_eq!(h.sink.displays().len(), 1, "and nothing new went up");
}

#[tokio::test]
async fn a_peer_that_never_claimed_source_is_not_listened_to() {
    let mut h = Harness::start_ungranted().await;
    h.expect_roles().await;
    h.policies
        .grant(h.peer, NotificationPolicy::default())
        .await;
    // Granted, connected, and silent about its roles.

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedRole)
        .await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn a_peer_that_stops_being_a_source_has_its_mirrors_closed() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    // What an Android device announces the moment notification access is
    // revoked: the same set, empty, one epoch higher.
    h.send(&roles(&[], 2)).await;
    h.wait_for("the mirrors to close", || h.sink.closes() == vec![1])
        .await;
    assert_eq!(h.sink.live_count(), 0);
}

#[tokio::test]
async fn this_device_announces_the_two_sink_side_roles_and_no_others() {
    let mut h = Harness::start_ungranted().await;
    let announced = h.expect_roles().await;
    assert_eq!(announced.epoch, 1);
    // ADR-0017 §1's v1 assignment for Linux, complete as of N4.
    assert_eq!(
        announced.roles,
        vec![
            pb::NotificationRole::Sink as i32,
            pb::NotificationRole::DismissReporter as i32
        ]
    );
    assert!(
        !announced
            .roles
            .contains(&(pb::NotificationRole::Source as i32)),
        "Linux cannot observe other applications' notifications"
    );
    assert!(
        !announced
            .roles
            .contains(&(pb::NotificationRole::DismissTarget as i32)),
        "Linux sources nothing, so there is nothing here to be asked to dismiss"
    );
}

#[tokio::test]
async fn a_dismiss_request_is_refused_because_this_device_sources_nothing() {
    // Unchanged by N4. Reporting a dismissal and acting on one are two roles
    // precisely so that gaining the first cannot quietly grant the second.
    let mut h = Harness::start().await;
    h.send(&control(pb::notification_control::Body::Dismiss(
        pb::DismissRequest {
            notification_id: id_bytes(1),
            origin_device_id: ORIGIN.to_string(),
        },
    )))
    .await;
    h.expect_outcome(pb::NotificationOutcome::RejectedRole)
        .await;
}

#[tokio::test]
async fn a_stale_role_epoch_cannot_rewiden_a_narrowed_set() {
    let mut h = Harness::start().await;
    h.send(&roles(&[], 2)).await;
    // Epoch 1 again, claiming SOURCE. Refused: not strictly greater.
    h.send(&roles(&[pb::NotificationRole::Source], 1)).await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedRole)
        .await;
    assert!(h.sink.displays().is_empty());
}

// ---------------------------------------------------------------------------
// Lock state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unlocked_session_shows_the_whole_notification() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let (_, mirror) = &h.sink.displays()[0];
    assert_eq!(mirror.body, "the code is 123456");
    assert!(!mirror.redacted);
}

#[tokio::test]
async fn a_locked_session_shows_the_app_name_and_no_body() {
    let mut h = Harness::start().await;
    h.lock.set_locked(true).await;

    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let (_, mirror) = &h.sink.displays()[0];
    assert_eq!(mirror.summary, "Chat");
    assert_eq!(mirror.body, "", "the body is absent, not hidden");
    assert!(mirror.redacted);
    // And the withheld text is nowhere in what reached the platform.
    let rendered = format!("{mirror:?}");
    assert!(!rendered.contains("123456"));
    assert!(!rendered.contains("Ana"));
}

#[tokio::test]
async fn an_unknown_lock_state_behaves_as_locked() {
    let mut h = Harness::start().await;
    // The platform cannot answer: no logind, a D-Bus error, an odd session.
    h.lock.set_unknown(true);

    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let (_, mirror) = &h.sink.displays()[0];
    assert_eq!(
        mirror.body, "",
        "a privacy control that fails open is not one"
    );
    assert!(mirror.redacted);
}

#[tokio::test]
async fn a_suppress_policy_displays_nothing_while_locked() {
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
    h.lock.set_locked(true).await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn a_full_policy_shows_everything_while_locked() {
    // The setting exists, POC-NOTIF-02 proved the detection works, and a user
    // who chooses it gets what they chose.
    let mut h = Harness::start().await;
    h.policies
        .grant(
            h.peer,
            NotificationPolicy {
                when_sink_locked: LockPolicy::Full,
                ..NotificationPolicy::default()
            },
        )
        .await;
    h.lock.set_locked(true).await;

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[0].1.body, "lunch?");
}

#[tokio::test]
async fn locking_reduces_what_is_already_on_the_screen_without_duplicating_it() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(
        h.sink.live(1).expect("displayed").body,
        "the code is 123456"
    );

    h.manager.set_locked(true).await;
    h.barrier().await;

    let displays = h.sink.displays();
    assert_eq!(displays.len(), 2, "one re-post, not one new notification");
    assert_eq!(displays[1].0, Some(1), "through replaces_id");
    assert_eq!(h.sink.live_count(), 1);
    let live = h.sink.live(1).expect("still one entry");
    assert_eq!(live.body, "");
    assert_eq!(live.summary, "Chat");
    assert!(live.redacted);
}

#[tokio::test]
async fn unlocking_does_not_restore_content_the_sink_never_kept() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.manager.set_locked(true).await;
    h.barrier().await;
    let after_lock = h.sink.displays().len();

    h.manager.set_locked(false).await;
    h.barrier().await;

    assert_eq!(
        h.sink.displays().len(),
        after_lock,
        "restoring the body would mean this process had kept it, which is a \
         notification history by another name"
    );
    assert_eq!(h.sink.live(1).expect("still there").body, "");
}

#[tokio::test]
async fn a_reduced_mirror_shows_its_new_content_when_the_source_updates_it() {
    // The counterpart to the rule above: nothing is replayed, but the next
    // real update is displayed in full, on the unlocked screen it arrives at.
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "the code is 123456")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.manager.set_locked(true).await;
    h.barrier().await;
    h.manager.set_locked(false).await;
    h.barrier().await;

    h.send_upsert(upsert(1, "Ana", "never mind")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.live(1).expect("still there").body, "never mind");
}

#[tokio::test]
async fn locking_with_a_suppress_policy_takes_mirrors_off_the_screen() {
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
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.manager.set_locked(true).await;
    h.wait_for("the mirrors to come off the locked screen", || {
        h.sink.closes() == vec![1]
    })
    .await;
    assert_eq!(h.sink.live_count(), 0);
}

// ---------------------------------------------------------------------------
// Privacy, importance and unknown values
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_secret_notification_is_never_displayed_even_though_the_source_should_not_send_one() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Bank", "your balance is …");
    message.privacy = pb::NotificationPrivacy::Secret as i32;

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn an_unknown_privacy_value_is_treated_as_secret_and_not_displayed() {
    // A value from a future implementation this build cannot reason about.
    // The conservative resolution is `SECRET`, which means "do not display".
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Bank", "your balance is …");
    message.privacy = 9999;

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::RejectedPolicy)
        .await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn high_importance_is_normal_urgency_not_critical() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "lunch?");
    message.importance = pb::NotificationImportance::High as i32;

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[0].1.urgency, Urgency::Normal);
}

#[tokio::test]
async fn low_importance_is_low_urgency() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "lunch?");
    message.importance = pb::NotificationImportance::Low as i32;

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[0].1.urgency, Urgency::Low);
}

#[tokio::test]
async fn a_notification_with_no_title_is_headed_by_the_application() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "", "just a body")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[0].1.summary, "Chat");
}

// ---------------------------------------------------------------------------
// Text safety
// ---------------------------------------------------------------------------

#[tokio::test]
async fn markup_in_a_body_is_escaped_before_it_reaches_a_body_markup_server() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(
        1,
        "Ana",
        "<b>URGENT</b> <a href='http://evil'>click</a> & more",
    ))
    .await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let body = &h.sink.displays()[0].1.body;
    assert!(!body.contains("<b>"), "a peer cannot forge emphasis");
    assert!(!body.contains("<a "), "or a hyperlink");
    assert!(body.contains("&lt;b&gt;"));
    assert!(body.contains("&amp; more"));
}

#[tokio::test]
async fn bidi_overrides_and_control_characters_never_reach_the_desktop() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(
        1,
        "invoice\u{202e}cod.exe",
        "line\u{7}one\u{200f}\u{2066}two",
    ))
    .await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let (_, mirror) = &h.sink.displays()[0];
    for hostile in ['\u{202e}', '\u{200f}', '\u{2066}', '\u{7}'] {
        assert!(!mirror.summary.contains(hostile));
        assert!(!mirror.body.contains(hostile));
    }
    assert_eq!(mirror.summary, "invoicecod.exe");
    assert_eq!(mirror.body, "lineonetwo");
}

#[tokio::test]
async fn a_server_without_body_markup_gets_the_text_unescaped() {
    let mut h = Harness::start_ungranted().await;
    h.sink.set_capabilities(SinkCapabilities {
        body_markup: false,
        body: true,
        persistence: false,
        dismiss_reporting: true,
    });
    // The capability set is read once at construction, so a fresh manager is
    // needed for the change to be the one under test. Rather than reaching
    // into the manager, this asserts the rule at the level it is decided.
    let escaped = omnibridge_capability_notifications::text::body("tea & biscuits", false);
    assert_eq!(escaped, "tea & biscuits");
    let escaped = omnibridge_capability_notifications::text::body("tea & biscuits", true);
    assert_eq!(escaped, "tea &amp; biscuits");
    let _ = &mut h;
}

#[tokio::test]
async fn an_oversized_field_is_refused_rather_than_truncated_by_the_receiver() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "x");
    message.body = "y".repeat(omnibridge_core::notifications::MAX_BODY_BYTES + 1);

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::TooLarge).await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn a_nul_bearing_field_is_refused_non_fatally() {
    let mut h = Harness::start().await;
    let mut message = upsert(1, "Ana", "before\0after");
    message.content_hash.clear();

    h.send_upsert(message).await;
    h.expect_outcome(pb::NotificationOutcome::Invalid).await;

    // The session is unharmed: the next message is handled normally.
    h.send_upsert(upsert(2, "Bea", "hello")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
}

#[tokio::test]
async fn a_bad_width_identifier_is_refused_and_not_answered() {
    let mut h = Harness::start().await;
    h.send(&control(pb::notification_control::Body::Upsert(
        pb::NotificationUpsert {
            notification_id: vec![1; 8],
            origin_device_id: ORIGIN.to_string(),
            ..pb::NotificationUpsert::default()
        },
    )))
    .await;

    // Nothing is answered: a result echoes an id, and there is no coherent one.
    // The barrier proves the session is still healthy and that the malformed
    // message produced no reply of its own.
    h.barrier().await;
    assert!(h.sink.displays().is_empty());
}

#[tokio::test]
async fn a_malformed_payload_does_not_break_the_session() {
    let h = Harness::start().await;
    let result = h
        .manager
        .handle_control(h.peer, &[0xff, 0xff, 0xff, 0xff])
        .await;
    assert!(result.is_err(), "the capability reports the problem");

    // …and reporting it is all that happens: `session.rs` logs a capability
    // error and keeps going. The next message proves it.
    let mut h = h;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

const SYNC_A: &[u8] = &[0xA1; 16];
const SYNC_B: &[u8] = &[0xB2; 16];

#[tokio::test]
async fn a_complete_snapshot_removes_what_it_did_not_name() {
    let mut h = Harness::start().await;
    for seed in 1..=3 {
        h.send_upsert(upsert(seed, "Ana", &format!("message {seed}")))
            .await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    }

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(1, "Ana", "message 1")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;
    h.send_upsert(upsert(3, "Ana", "message 3")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert_eq!(h.sink.closes(), vec![2], "only the unnamed one goes");
    assert_eq!(h.sink.live_count(), 2);
}

#[tokio::test]
async fn a_snapshot_that_names_everything_removes_nothing_and_duplicates_nothing() {
    // The reconnect case: the phone comes back and re-states what is active.
    let mut h = Harness::start().await;
    for seed in 1..=3 {
        h.send_upsert(upsert(seed, "Ana", &format!("message {seed}")))
            .await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    }
    let before = h.sink.displays().len();

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    for seed in 1..=3 {
        h.send_upsert(upsert(seed, "Ana", &format!("message {seed}")))
            .await;
        h.expect_outcome(pb::NotificationOutcome::Duplicate).await;
    }
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert!(h.sink.closes().is_empty());
    assert_eq!(h.sink.live_count(), 3, "no wall of duplicates");
    assert_eq!(h.sink.displays().len(), before, "and no re-display either");
}

#[tokio::test]
async fn an_end_with_no_begin_removes_nothing() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert!(
        h.sink.closes().is_empty(),
        "an unpaired END must never be read as 'remove everything'"
    );
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn an_end_for_a_different_snapshot_removes_nothing_and_leaves_the_real_one_open() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "one")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send_upsert(upsert(2, "Ana", "two")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(1, "Ana", "one")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;

    // A stale or interleaved snapshot cannot silently become authoritative.
    h.send(&marker(SYNC_B, pb::sync_marker::Phase::End)).await;
    h.barrier().await;
    assert!(h.sink.closes().is_empty());

    // The real END still completes it.
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;
    assert_eq!(h.sink.closes(), vec![2]);
}

#[tokio::test]
async fn a_restarted_snapshot_removes_nothing_on_the_abandoned_one() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "one")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send_upsert(upsert(2, "Ana", "two")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(1, "Ana", "one")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;

    // A second BEGIN. The first can never be completed, so it applies nothing.
    h.send(&marker(SYNC_B, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(2, "Ana", "two")).await;
    h.expect_outcome(pb::NotificationOutcome::Duplicate).await;
    h.send(&marker(SYNC_B, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert_eq!(
        h.sink.closes(),
        vec![1],
        "the second snapshot named 2 and not 1, and only it was applied"
    );
}

#[tokio::test]
async fn an_incomplete_snapshot_is_abandoned_and_removes_nothing() {
    tokio::time::pause();
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "one")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.barrier().await;
    assert!(h.report().await.snapshot_open);

    tokio::time::advance(limits::SYNC_TIMEOUT + std::time::Duration::from_secs(1)).await;
    // Let the worker's timer branch run.
    tokio::task::yield_now().await;
    h.barrier().await;

    assert!(!h.report().await.snapshot_open);
    assert!(h.sink.closes().is_empty(), "nothing was removed");
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn a_snapshot_for_one_peer_never_touches_another_peers_mirrors() {
    let mut h = Harness::start().await;
    let mut other = h.second_peer(0xcd).await;

    h.send_upsert(upsert(1, "Ana", "from the phone")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    h.manager
        .handle_control(
            other.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                5,
                "Bea",
                "from the tablet",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = other.next_result().await;
    assert_eq!(outcome, pb::NotificationOutcome::Displayed);

    // The first peer sends an empty snapshot: everything of *its* own is gone.
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;
    other.barrier(&h.manager).await;

    assert_eq!(h.sink.closes(), vec![1]);
    assert_eq!(h.sink.live_count(), 1, "the tablet's mirror is untouched");
}

// ---------------------------------------------------------------------------
// Desktop-initiated closes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_user_dismissal_drops_the_mapping_so_the_next_update_creates_afresh() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    // The server invalidates the id before it signals, so a later
    // `replaces_id` naming it would create a second notification.
    h.sink.user_closes(1, CloseReason::Dismissed).await;
    h.barrier().await;

    h.send_upsert(upsert(1, "Ana", "still there?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let displays = h.sink.displays();
    assert_eq!(displays[1].0, None, "a fresh notification, not a stale id");
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn no_dismiss_request_is_ever_sent_to_a_peer() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    // A human closed it. N4 is the wave that may act on this; N2 records it.
    h.sink.user_closes(1, CloseReason::Dismissed).await;
    h.barrier().await;

    // The barrier's own answer is the only thing that came back. Anything else
    // queued would have arrived before it, because one worker sends in order.
    h.send_upsert(upsert(2, "Bea", "hello")).await;
    let next = h.next_outbound().await;
    assert!(
        matches!(next.body, Some(pb::notification_control::Body::Result(_))),
        "the only thing this device sends back is a result, never a dismissal"
    );
}

#[tokio::test]
async fn an_expiry_is_recorded_as_an_expiry_and_not_as_a_dismissal() {
    // The distinction is N4's most important rule: a desktop banner timing out
    // must never clear somebody's phone. N2 keeps the reasons apart so that
    // the wave which acts on them has something true to act on.
    assert!(CloseReason::Dismissed.is_human_dismissal());
    assert!(!CloseReason::Expired.is_human_dismissal());
    assert!(!CloseReason::Closed.is_human_dismissal());
    assert!(!CloseReason::Undefined.is_human_dismissal());

    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.sink.user_closes(1, CloseReason::Expired).await;
    h.barrier().await;

    // Locally it means the same thing — the id is dead either way.
    h.send_upsert(upsert(1, "Ana", "again")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[1].0, None);
}

// ---------------------------------------------------------------------------
// Backend failure, restart and isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_backend_failure_is_reported_and_the_session_survives() {
    let mut h = Harness::start().await;
    h.sink
        .set_failure(Some(SinkError::Failed("refused".into())));

    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Failed).await;

    h.sink.set_failure(None);
    h.send_upsert(upsert(2, "Bea", "hello")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
}

#[tokio::test]
async fn an_absent_server_narrows_the_sink_role_without_a_reconnect() {
    let mut h = Harness::start().await;
    assert!(h.manager.is_available());

    h.sink.go_away().await;
    let narrowed = h.next_roles().await;
    assert!(narrowed.roles.is_empty(), "no server, no SINK");
    assert_eq!(narrowed.epoch, 2, "strictly higher, on the same connection");
    assert!(!h.manager.is_available());

    h.sink.come_back().await;
    let widened = h.next_roles().await;
    assert_eq!(
        widened.roles,
        vec![
            pb::NotificationRole::Sink as i32,
            pb::NotificationRole::DismissReporter as i32
        ]
    );
    assert_eq!(widened.epoch, 3);
}

#[tokio::test]
async fn a_notification_server_restart_invalidates_the_stale_ids() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    assert_eq!(h.sink.displays()[0].0, None);

    // gnome-shell dies and comes back. It tells nobody what it was holding,
    // and it starts numbering again — so id 1 may now belong to somebody
    // else's notification. Each transition is awaited through the role
    // announcement it causes, which is also the observable this desktop offers
    // a peer.
    h.sink.go_away().await;
    let narrowed = h.next_roles().await;
    assert!(narrowed.roles.is_empty());
    h.sink.come_back().await;
    let widened = h.next_roles().await;
    assert_eq!(
        widened.roles,
        vec![
            pb::NotificationRole::Sink as i32,
            pb::NotificationRole::DismissReporter as i32
        ]
    );
    h.barrier().await;

    h.send_upsert(upsert(1, "Ana", "still here?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    assert_eq!(
        h.sink.displays()[1].0,
        None,
        "the next update creates a notification rather than replacing a \
         number that now means something else"
    );
    assert_eq!(h.sink.live_count(), 1);
}

#[tokio::test]
async fn a_snapshot_reconverges_after_a_server_restart() {
    let mut h = Harness::start().await;
    for seed in 1..=2 {
        h.send_upsert(upsert(seed, "Ana", &format!("message {seed}")))
            .await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    }

    h.sink.go_away().await;
    h.next_roles().await;
    h.sink.come_back().await;
    h.next_roles().await;
    h.barrier().await;

    // The phone re-states what is active. Nothing was persisted to survive the
    // restart; asking the source again is free and is the whole design.
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::Begin)).await;
    h.send_upsert(upsert(1, "Ana", "message 1")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    h.send(&marker(SYNC_A, pb::sync_marker::Phase::End)).await;
    h.barrier().await;

    assert_eq!(h.sink.live_count(), 1, "exactly what the phone still has");
}

#[tokio::test]
async fn the_mirror_table_is_bounded_per_peer() {
    let mut h = Harness::start().await;
    for seed in 0..(limits::MAX_MIRRORS_PER_PEER as u16 + 10) {
        h.send_upsert(upsert(seed, "Ana", &format!("message {seed}")))
            .await;
        h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    }

    let report = h.report().await;
    assert_eq!(report.mirrors, limits::MAX_MIRRORS_PER_PEER);
    assert_eq!(report.evicted, 10);
    assert_eq!(
        h.sink.live_count(),
        limits::MAX_MIRRORS_PER_PEER,
        "an evicted mirror is closed, not merely forgotten"
    );
}

#[tokio::test]
async fn one_peers_flood_does_not_evict_another_peers_mirrors() {
    let mut h = Harness::start().await;
    let mut other = h.second_peer(0xcd).await;

    h.manager
        .handle_control(
            other.peer,
            &control(pb::notification_control::Body::Upsert(upsert(
                7, "Bea", "quiet",
            ))),
        )
        .await
        .expect("upsert");
    let (_, outcome) = other.next_result().await;
    assert_eq!(outcome, pb::NotificationOutcome::Displayed);

    for seed in 0..(limits::MAX_MIRRORS_PER_PEER as u16 + 50) {
        h.send_upsert(upsert(seed, "Ana", "flood")).await;
        let (_, outcome) = h.next_result().await;
        assert_eq!(outcome, pb::NotificationOutcome::Displayed);
    }

    other.barrier(&h.manager).await;
    let reports = h.manager.peer_reports().await;
    let quiet = reports
        .iter()
        .find(|r| r.peer == other.peer)
        .expect("a report");
    assert_eq!(quiet.mirrors, 1, "the cap is per peer, and it held");
    assert_eq!(quiet.evicted, 0);
}

// ---------------------------------------------------------------------------
// No persistence, no relay, no content anywhere it should not be
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_peers_notification_is_never_forwarded_to_another_peer() {
    let mut h = Harness::start().await;
    let mut other = h.second_peer(0xcd).await;

    h.send_upsert(upsert(1, "Ana", "OMNIBRIDGE-N2-CANARY"))
        .await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    other.barrier(&h.manager).await;

    // The only thing the second peer ever received is its own barrier's
    // answer, which the barrier itself asserts. Anything relayed would have
    // arrived before it, because one worker sends in order.
    h.send_upsert(upsert(2, "Ana", "another")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;
    other.barrier(&h.manager).await;
}

#[tokio::test]
async fn no_report_renders_any_notification_content() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "OMNIBRIDGE-N2-TITLE", "OMNIBRIDGE-N2-BODY"))
        .await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let report = h.report().await;
    let rendered = format!("{report:?}");
    for canary in [
        "OMNIBRIDGE-N2-TITLE",
        "OMNIBRIDGE-N2-BODY",
        "com.example.chat",
    ] {
        assert!(
            !rendered.contains(canary),
            "{canary} reached a report that a log line or a CLI row prints"
        );
    }
}

#[tokio::test]
async fn a_peer_report_counts_rather_than_describes() {
    let mut h = Harness::start().await;
    h.send_upsert(upsert(1, "Ana", "lunch?")).await;
    h.expect_outcome(pb::NotificationOutcome::Displayed).await;

    let report = h.report().await;
    assert!(report.connected);
    assert_eq!(report.mirrors, 1);
    assert_eq!(report.displayed, 1);
    assert_eq!(report.local_roles, 2, "SINK and DISMISS_REPORTER");
    assert_eq!(report.local_epoch, 1);
    assert!(report.local_reports_dismissals);
    assert!(report.peer_is_source);
    assert!(
        !report.peer_is_dismiss_target,
        "this harness's peer announces SOURCE alone"
    );
    assert_eq!(report.peer_epoch, 1);
    assert!(!report.snapshot_open);
    assert_eq!(report.dismissals_sent, 0);
    assert_eq!(report.dismissals_refused, 0);
}
