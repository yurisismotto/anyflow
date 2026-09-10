//! What this desktop announces it can do, and how that changes mid-session.
//!
//! # A role is a statement about what is physically possible right now
//!
//! Not about what a peer is allowed to do — that is the grant — and not about
//! what this build supports in principle — that is the `HELLO`. ADR-0017 §6
//! tabulates why collapsing any two of the three would be a consent failure;
//! the practical consequence here is short:
//!
//! * this device announces `SINK` **only while a notification server is
//!   actually reachable**, because a desktop with no server cannot display
//!   anything for anyone and saying otherwise makes a peer send content into a
//!   void;
//! * **the peer grant is deliberately not an input.** Holding a grant does not
//!   manufacture a notification server, and having a notification server
//!   grants nothing to any peer. The two questions are answered in two places
//!   on purpose.
//!
//! # What N2 does not claim
//!
//! `DISMISS_REPORTER` means *"I will tell you when a human dismissed a mirror
//! I displayed"*. N2 observes those closes and records their reasons
//! accurately, but it sends no `DismissRequest` — that runtime is N4 — so
//! announcing the role would be a claim this wave cannot honour, and a peer
//! that believed it would wait for reports that never come. A role is a
//! promise; the honest answer in N2 is `SINK` and nothing else.
//!
//! `SOURCE` is never claimed at all. There is no supported way for an ordinary
//! client to observe other applications' notifications through
//! `org.freedesktop.Notifications` — doing it means becoming the notification
//! server, and only one process may own that name. Linux is a sink in v1, and
//! the protocol is built so it does not have to stay one.

use std::collections::BTreeSet;

use anyflow_core::notifications::Role;
use anyflow_proto::v1::capabilities as pb;

/// This device's announcement state for one connection.
///
/// The epoch is **per connection**, starting at 1 and strictly increasing. It
/// is reset when a new session attaches, because a receiver's rule is "not
/// strictly greater than the last accepted is refused" and a fresh connection
/// has accepted nothing.
#[derive(Debug, Default)]
pub struct LocalRoles {
    announced: Option<BTreeSet<Role>>,
    epoch: u32,
}

impl LocalRoles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// How many roles were last announced. A count, for a log line.
    pub fn count(&self) -> usize {
        self.announced.as_ref().map_or(0, |r| r.len())
    }

    /// Forgets everything. Called when a new session attaches to this peer.
    pub fn reset(&mut self) {
        self.announced = None;
        self.epoch = 0;
    }

    /// The set this device should be announcing, given what it can do.
    ///
    /// One input, deliberately: whether a notification server is reachable.
    pub fn desired(server_available: bool) -> BTreeSet<Role> {
        if server_available {
            [Role::Sink].into_iter().collect()
        } else {
            BTreeSet::new()
        }
    }

    /// Produces the next announcement, or `None` when nothing changed.
    ///
    /// Returning `None` for an unchanged set is what stops a flapping backend
    /// from filling a peer's log with identical messages, and it is what makes
    /// the epoch mean something: every increment is a real change.
    pub fn announce(&mut self, server_available: bool) -> Option<pb::NotificationRoles> {
        let desired = Self::desired(server_available);
        if self.announced.as_ref() == Some(&desired) {
            return None;
        }
        self.epoch = self.epoch.saturating_add(1);
        self.announced = Some(desired.clone());
        Some(pb::NotificationRoles {
            roles: desired.iter().map(|r| to_wire(*r) as i32).collect(),
            epoch: self.epoch,
        })
    }
}

fn to_wire(role: Role) -> pb::NotificationRole {
    match role {
        Role::Source => pb::NotificationRole::Source,
        Role::Sink => pb::NotificationRole::Sink,
        Role::DismissTarget => pb::NotificationRole::DismissTarget,
        Role::DismissReporter => pb::NotificationRole::DismissReporter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n2_announces_sink_and_only_sink() {
        let desired = LocalRoles::desired(true);
        assert!(desired.contains(&Role::Sink));
        assert_eq!(desired.len(), 1, "N2 claims exactly one role");
        assert!(
            !desired.contains(&Role::DismissReporter),
            "N4 owns dismissal reporting; claiming it now is a promise this \
             wave cannot keep"
        );
        assert!(
            !desired.contains(&Role::Source),
            "Linux cannot observe other applications' notifications"
        );
    }

    #[test]
    fn no_server_means_no_roles() {
        assert!(LocalRoles::desired(false).is_empty());
    }

    #[test]
    fn the_first_announcement_is_epoch_one() {
        let mut roles = LocalRoles::new();
        let first = roles.announce(true).expect("something to say");
        assert_eq!(first.epoch, 1);
        assert_eq!(first.roles, vec![pb::NotificationRole::Sink as i32]);
    }

    #[test]
    fn an_unchanged_set_produces_no_announcement() {
        let mut roles = LocalRoles::new();
        assert!(roles.announce(true).is_some());
        assert!(roles.announce(true).is_none());
        assert!(roles.announce(true).is_none());
        assert_eq!(roles.epoch(), 1, "the epoch counts changes, not calls");
    }

    #[test]
    fn narrowing_and_widening_both_take_a_strictly_higher_epoch() {
        let mut roles = LocalRoles::new();
        assert_eq!(roles.announce(true).expect("sink").epoch, 1);

        let narrowed = roles.announce(false).expect("the server went away");
        assert_eq!(narrowed.epoch, 2);
        assert!(narrowed.roles.is_empty(), "an empty set is meaningful");

        let widened = roles.announce(true).expect("the server came back");
        assert_eq!(widened.epoch, 3);
        assert_eq!(widened.roles, vec![pb::NotificationRole::Sink as i32]);
    }

    #[test]
    fn a_new_session_starts_the_epoch_again() {
        let mut roles = LocalRoles::new();
        roles.announce(true);
        roles.announce(false);
        assert_eq!(roles.epoch(), 2);

        roles.reset();
        assert_eq!(roles.epoch(), 0);
        // A fresh connection has accepted nothing, so 1 is strictly greater
        // than what the peer holds.
        assert_eq!(roles.announce(true).expect("sink").epoch, 1);
    }
}
