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
//! * it announces `DISMISS_REPORTER` **only while the backend can positively
//!   identify a human dismissal** — see below;
//! * **the peer grant is deliberately not an input.** Holding a grant does not
//!   manufacture a notification server, and having a notification server
//!   grants nothing to any peer. The two questions are answered in two places
//!   on purpose.
//!
//! # `DISMISS_REPORTER` is a second platform capability, not a second name for
//! `SINK`
//!
//! `DISMISS_REPORTER` means *"I will tell you when a human dismissed a mirror I
//! displayed"*, and being able to **display** a notification does not imply
//! being able to say **why it closed**. On freedesktop the two are different
//! parts of the interface: `Notify` is a method every server implements, and
//! `NotificationClosed` is a signal this process has to be subscribed to, whose
//! `reason` field is what separates a person clicking the X (2) from a banner
//! timing out (1) or from our own `CloseNotification` coming back (3).
//!
//! So the role has its own input — [`crate::backend::SinkCapabilities::dismiss_reporting`]
//! — which is `false` unless the backend actually holds that subscription. A
//! sink that could display but not report would announce `SINK` alone, and the
//! phone would correctly conclude that dismissing on this desktop does
//! nothing to it. **Dismiss-sync policy is deliberately not an input either**:
//! a role says what the machine can do, and whether a particular dismissal may
//! travel is a per-peer policy question answered per event (ADR-0017 §6).
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

    /// Whether the set last announced to this peer includes `role`.
    ///
    /// Read before a `DismissRequest` is sent, so that this device cannot do
    /// something it has not told the peer it does. It is not a security check —
    /// the two policies and the peer's own `DISMISS_TARGET` claim are what
    /// gate the effect — it is a consistency one: announcing `SINK` alone and
    /// then sending dismissals would make the role vocabulary meaningless.
    pub fn announced(&self, role: Role) -> bool {
        self.announced.as_ref().is_some_and(|r| r.contains(&role))
    }

    /// Forgets everything. Called when a new session attaches to this peer.
    pub fn reset(&mut self) {
        self.announced = None;
        self.epoch = 0;
    }

    /// The set this device should be announcing, given what it can do.
    ///
    /// Two inputs, both about the platform and neither about a peer: whether a
    /// notification server is reachable, and whether the backend can report a
    /// human dismissal. No server means **no roles at all**, including no
    /// `DISMISS_REPORTER`: there is nothing on the screen to dismiss, so
    /// promising to report a dismissal would be promising to report an event
    /// that cannot happen.
    pub fn desired(server_available: bool, dismiss_reporting: bool) -> BTreeSet<Role> {
        if !server_available {
            return BTreeSet::new();
        }
        let mut roles: BTreeSet<Role> = [Role::Sink].into_iter().collect();
        if dismiss_reporting {
            roles.insert(Role::DismissReporter);
        }
        roles
    }

    /// Produces the next announcement, or `None` when nothing changed.
    ///
    /// Returning `None` for an unchanged set is what stops a flapping backend
    /// from filling a peer's log with identical messages, and it is what makes
    /// the epoch mean something: every increment is a real change.
    pub fn announce(
        &mut self,
        server_available: bool,
        dismiss_reporting: bool,
    ) -> Option<pb::NotificationRoles> {
        let desired = Self::desired(server_available, dismiss_reporting);
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

    /// The v1 assignment: ADR-0017 §1 says Linux advertises `SINK` and
    /// `DISMISS_REPORTER`, and as of N4 it does both.
    #[test]
    fn a_reporting_desktop_announces_sink_and_dismiss_reporter() {
        let desired = LocalRoles::desired(true, true);
        assert!(desired.contains(&Role::Sink));
        assert!(desired.contains(&Role::DismissReporter));
        assert_eq!(desired.len(), 2);
        assert!(
            !desired.contains(&Role::Source),
            "Linux cannot observe other applications' notifications"
        );
        assert!(
            !desired.contains(&Role::DismissTarget),
            "Linux sources nothing, so there is nothing here for a peer to \
             ask it to dismiss"
        );
    }

    /// Displaying and reporting are two capabilities, and this is the state
    /// that proves the second is not inferred from the first.
    #[test]
    fn a_backend_that_cannot_report_closes_announces_sink_alone() {
        let desired = LocalRoles::desired(true, false);
        assert_eq!(desired.len(), 1);
        assert!(desired.contains(&Role::Sink));
        assert!(
            !desired.contains(&Role::DismissReporter),
            "a desktop that cannot tell a human dismissal from an expiry must \
             never promise to report one"
        );
    }

    #[test]
    fn no_server_means_no_roles_at_all() {
        assert!(LocalRoles::desired(false, false).is_empty());
        assert!(
            LocalRoles::desired(false, true).is_empty(),
            "with nothing on the screen there is no dismissal to report"
        );
    }

    #[test]
    fn the_first_announcement_is_epoch_one() {
        let mut roles = LocalRoles::new();
        let first = roles.announce(true, true).expect("something to say");
        assert_eq!(first.epoch, 1);
        assert_eq!(
            first.roles,
            vec![
                pb::NotificationRole::Sink as i32,
                pb::NotificationRole::DismissReporter as i32
            ],
            "sorted by wire number, so the encoding is deterministic"
        );
    }

    #[test]
    fn an_unchanged_set_produces_no_announcement() {
        let mut roles = LocalRoles::new();
        assert!(roles.announce(true, true).is_some());
        assert!(roles.announce(true, true).is_none());
        assert!(roles.announce(true, true).is_none());
        assert_eq!(roles.epoch(), 1, "the epoch counts changes, not calls");
    }

    #[test]
    fn narrowing_and_widening_both_take_a_strictly_higher_epoch() {
        let mut roles = LocalRoles::new();
        assert_eq!(roles.announce(true, true).expect("sink").epoch, 1);

        let narrowed = roles.announce(false, true).expect("the server went away");
        assert_eq!(narrowed.epoch, 2);
        assert!(narrowed.roles.is_empty(), "an empty set is meaningful");

        let widened = roles.announce(true, true).expect("the server came back");
        assert_eq!(widened.epoch, 3);
        assert_eq!(
            widened.roles,
            vec![
                pb::NotificationRole::Sink as i32,
                pb::NotificationRole::DismissReporter as i32
            ]
        );
    }

    /// Losing only the *reporting* half narrows the set without dropping
    /// `SINK`, which is exactly the partial narrowing roles exist to express:
    /// mirroring keeps working and dismissal sync stops claiming to.
    #[test]
    fn losing_dismiss_reporting_narrows_without_losing_the_sink() {
        let mut roles = LocalRoles::new();
        roles.announce(true, true);
        let narrowed = roles
            .announce(true, false)
            .expect("the reporting half went away");
        assert_eq!(narrowed.epoch, 2);
        assert_eq!(narrowed.roles, vec![pb::NotificationRole::Sink as i32]);
        assert!(roles.announced(Role::Sink));
        assert!(!roles.announced(Role::DismissReporter));
    }

    #[test]
    fn what_was_announced_can_be_read_back_and_starts_as_nothing() {
        let mut roles = LocalRoles::new();
        assert!(
            !roles.announced(Role::DismissReporter),
            "before any announcement this device claims nothing"
        );
        roles.announce(true, true);
        assert!(roles.announced(Role::DismissReporter));
        assert!(roles.announced(Role::Sink));
        assert!(!roles.announced(Role::Source));
    }

    #[test]
    fn a_new_session_starts_the_epoch_again() {
        let mut roles = LocalRoles::new();
        roles.announce(true, true);
        roles.announce(false, true);
        assert_eq!(roles.epoch(), 2);

        roles.reset();
        assert_eq!(roles.epoch(), 0);
        assert!(
            !roles.announced(Role::Sink),
            "a reset device has told the new connection nothing"
        );
        // A fresh connection has accepted nothing, so 1 is strictly greater
        // than what the peer holds.
        assert_eq!(roles.announce(true, true).expect("sink").epoch, 1);
    }
}
