//! Converging a live session on a grant that was made after its handshake.
//!
//! # The defect this exists to correct
//!
//! A session's capability set is decided once, during `HELLO`, as *what both
//! sides implement* intersected with *what this peer is allowed to do*
//! ([`anyflow_core::session`], the `PeerStatus::Trusted` arm). The resulting
//! vector is then the authority for the whole life of that session, in two
//! places that matter:
//!
//! * the `on_peer_connected` fan-out, which is the only thing that makes a
//!   capability announce itself to the peer at all;
//! * the per-message inbound filter, which answers `UNSUPPORTED_CAPABILITY`
//!   to anything the vector does not name.
//!
//! Freezing it is right: re-reading a grant per message would let an
//! authorization **widen** without a handshake, and widening is the direction
//! that has to be deliberate ([ADR-0017 §3](../../../docs/adr/ADR-0017-capability-roles.md)).
//!
//! What was missing is the other half of that rule. ADR-0017 says a grant
//! needs *a reconnect* to widen — and nothing in the daemon ever asked for
//! one. So a person who granted `notifications.v1` to a phone that was
//! already connected got a session that could neither announce a `SINK` role
//! nor accept the phone's `SOURCE` announcement, and the only way out was to
//! press Disconnect and Connect by hand. Observed on hardware twice, in N3
//! §G4 and N4 §17.
//!
//! # The correction
//!
//! When a grant widens past what the peer's live session negotiated, ask that
//! session to end. The phone's own `ConnectionCoordinator` treats a session
//! that ended as proof the endpoint works, resets its ladder and redials on
//! the ordinary transient backoff — about two seconds. `HELLO` runs again,
//! the intersection is recomputed against the grant that now exists, and both
//! ends announce roles normally.
//!
//! This adds no wire message, no timer and no retry loop. Reconnection is
//! already owned by exactly one component and it stays that way: all this
//! does is stop a session that has become unable to do what the user just
//! asked for.
//!
//! # Why not widen the live session in place
//!
//! It was considered and rejected. Re-intersecting inside the running session
//! would need the peer's advertised set kept alive for the session's lifetime
//! (today only the intersection survives the handshake), a command to mutate
//! the vector the dispatch loop reads, and a second lifecycle event for
//! "this capability just became available" — and at the end of it an
//! authorization would have widened without a handshake, which is the thing
//! ADR-0017 §3 asks not to happen. A reconnect costs about two seconds of
//! control session. It does not cost the pairing, and it does not cost an
//! in-flight `files.v1` transfer either: a data stream is a separate
//! connection and survives the control session's death.
//!
//! # The bound
//!
//! **At most one request per session.** Not per second, not per peer: per
//! session. A session can only be asked to end once, and the session that
//! replaces it exists *because* a reconnect already happened — so a storm
//! would need the user to switch a grant off and on again, faster than the
//! phone can redial, forever. There is no clock in here to get wrong and
//! nothing to tune.

use std::collections::HashMap;

use anyflow_core::session::SessionId;
use anyflow_core::Fingerprint;
use tokio::sync::Mutex;

/// What a grant change means for the peer's live session.
///
/// Every variant is a reason, not a status code: the log line a caller writes
/// is the one thing that tells an operator why their grant did or did not
/// take effect, and "no" has four quite different meanings here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The grant was withdrawn, not given.
    ///
    /// Narrowing never needs a reconnect and must never wait for one:
    /// inbound authorization is re-read from the trust store per message, so
    /// a withdrawal already bites on the session that is up. Rebuilding the
    /// session would be strictly worse than doing nothing — it would take a
    /// permission away and then immediately hand the peer a fresh session.
    Withdrawn,

    /// The peer has no live session, so nothing is frozen.
    ///
    /// Its next connection reads the trust store as it finds it.
    NoSession,

    /// The live session already negotiated this capability.
    ///
    /// This is the ordinary case for every grant made while disconnected, and
    /// for the second grant of a capability that was already granted.
    AlreadyNegotiated,

    /// This exact session has already been asked to end.
    ///
    /// The coalescing point. Several writes in one burst — a grant, then a
    /// policy, then an allow-list — reach this after the first, and so does a
    /// repeat of the same grant.
    AlreadyRequested,

    /// The session cannot use the capability that was just granted. End it.
    Reconnect,
}

impl Decision {
    /// Whether the caller should shut the session down.
    pub fn is_reconnect(self) -> bool {
        matches!(self, Decision::Reconnect)
    }

    /// A stable, content-free word for a log line.
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Withdrawn => "withdrawn",
            Decision::NoSession => "no-session",
            Decision::AlreadyNegotiated => "already-negotiated",
            Decision::AlreadyRequested => "already-requested",
            Decision::Reconnect => "reconnect",
        }
    }
}

/// The live session a decision is about, reduced to the two facts that decide
/// it.
///
/// Taking this rather than a `SessionHandle` is what makes the rule testable
/// as a table: there is no transport, no trust store and no clock in the
/// decision, so every row below is a statement about the rule itself.
#[derive(Debug, Clone, Copy)]
pub struct LiveSession<'a> {
    pub id: SessionId,
    pub negotiated: &'a [String],
}

/// The rule, as a pure function.
///
/// `requested` is the session this peer was last asked to reconnect, if any.
pub fn decide(
    granted: bool,
    capability: &str,
    session: Option<LiveSession<'_>>,
    requested: Option<SessionId>,
) -> Decision {
    if !granted {
        return Decision::Withdrawn;
    }
    let Some(session) = session else {
        return Decision::NoSession;
    };
    if session.negotiated.iter().any(|c| c == capability) {
        return Decision::AlreadyNegotiated;
    }
    if requested == Some(session.id) {
        return Decision::AlreadyRequested;
    }
    Decision::Reconnect
}

/// Remembers which session each peer was last asked to reconnect.
///
/// One entry per peer, overwritten rather than appended, and dropped when the
/// peer reconnects or is forgotten — so this is bounded by the size of the
/// trust store and holds nothing but a `u64`.
#[derive(Default)]
pub struct Renegotiation {
    requested: Mutex<HashMap<Fingerprint, SessionId>>,
}

impl Renegotiation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides, and records the request if there is one.
    ///
    /// Recording inside the same lock as the decision is what makes the
    /// "once per session" bound hold when two control clients flip the same
    /// switch at once.
    pub async fn on_grant_changed(
        &self,
        peer: &Fingerprint,
        capability: &str,
        granted: bool,
        session: Option<LiveSession<'_>>,
    ) -> Decision {
        let mut requested = self.requested.lock().await;
        let decision = decide(granted, capability, session, requested.get(peer).copied());
        if let (Decision::Reconnect, Some(session)) = (decision, session) {
            requested.insert(*peer, session.id);
        }
        decision
    }

    /// Forgets any outstanding request for a peer.
    ///
    /// Called when a session registers — the request it named is over,
    /// whatever replaced it — and when a peer is forgotten.
    pub async fn clear(&self, peer: &Fingerprint) {
        self.requested.lock().await.remove(peer);
    }

    /// The session this peer was last asked to reconnect. Tests and
    /// diagnostics only.
    pub async fn outstanding(&self, peer: &Fingerprint) -> Option<SessionId> {
        self.requested.lock().await.get(peer).copied()
    }

    /// How many peers have an outstanding request. Tests and diagnostics only.
    pub async fn len(&self) -> usize {
        self.requested.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTIFICATIONS: &str = "notifications.v1";
    const CLIPBOARD: &str = "clipboard.v1";

    fn caps(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    fn peer(byte: u8) -> Fingerprint {
        Fingerprint::from_spki_der(&[byte])
    }

    /// **A — a grant that the live session cannot use asks for one reconnect.**
    ///
    /// Brief §4 A. This is the whole feature in one row.
    #[tokio::test]
    async fn a_widening_past_the_session_requests_exactly_one_reconnect() {
        let r = Renegotiation::new();
        let p = peer(1);
        let negotiated = caps(&["battery.v1", "clipboard.v1"]);
        let session = LiveSession {
            id: 7,
            negotiated: &negotiated,
        };

        assert_eq!(
            r.on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
                .await,
            Decision::Reconnect
        );
        assert_eq!(r.outstanding(&p).await, Some(7));

        // The same transition asserted again — a control client that retries,
        // or a GUI that writes the switch twice — must not ask twice.
        assert_eq!(
            r.on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
                .await,
            Decision::AlreadyRequested
        );
    }

    /// **B — a burst of writes produces at most one reconnect.**
    ///
    /// Brief §4 B. The policy writes are not grants and never reach this at
    /// all; what is pinned here is that the grant write is idempotent within
    /// one session, so a burst that repeats it cannot multiply.
    #[tokio::test]
    async fn b_a_burst_of_writes_produces_one_reconnect() {
        let r = Renegotiation::new();
        let p = peer(2);
        let negotiated = caps(&["battery.v1"]);
        let session = LiveSession {
            id: 11,
            negotiated: &negotiated,
        };

        let mut reconnects = 0;
        for _ in 0..8 {
            if r.on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
                .await
                .is_reconnect()
            {
                reconnects += 1;
            }
        }
        assert_eq!(reconnects, 1, "a burst must collapse to one reconnect");
    }

    /// **C — a session already asked to end is not asked again.**
    ///
    /// Brief §4 C. Between the shutdown and the new session's registration
    /// the old handle is still the registered one, so this is the state a
    /// second grant write genuinely lands in.
    #[tokio::test]
    async fn c_a_session_already_asked_is_not_asked_again() {
        let r = Renegotiation::new();
        let p = peer(3);
        let negotiated = caps(&[]);
        let session = LiveSession {
            id: 21,
            negotiated: &negotiated,
        };

        assert!(r
            .on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
            .await
            .is_reconnect());
        assert_eq!(
            r.on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
                .await,
            Decision::AlreadyRequested
        );
        // And a *different* capability granted on the same dying session is
        // also not a second reconnect: one reconnect fixes both.
        assert_eq!(
            r.on_grant_changed(&p, CLIPBOARD, true, Some(session)).await,
            Decision::AlreadyRequested
        );
    }

    /// **D — the connection dying is not this component's problem.**
    ///
    /// Brief §4 D. With no session there is nothing frozen and nothing to
    /// ask; the peer's own coordinator redials and the next `HELLO` reads the
    /// grant that now exists. Asserted as a decision rather than as an
    /// absence so that a future change that starts dialling from here fails
    /// this test.
    #[tokio::test]
    async fn d_no_session_means_no_request_and_no_retry() {
        let r = Renegotiation::new();
        let p = peer(4);
        assert_eq!(
            r.on_grant_changed(&p, NOTIFICATIONS, true, None).await,
            Decision::NoSession
        );
        assert!(
            r.is_empty().await,
            "nothing may be recorded for a dead peer"
        );
    }

    /// **E — revocation never reconnects.**
    ///
    /// Brief §4 E. Withdrawal is immediate and fail-closed through the
    /// per-message authorizer; a reconnect would hand the peer a brand new
    /// session at the exact moment the user took a permission away.
    #[tokio::test]
    async fn e_withdrawing_a_grant_never_reconnects() {
        let r = Renegotiation::new();
        let p = peer(5);
        let negotiated = caps(&["notifications.v1"]);
        let session = LiveSession {
            id: 31,
            negotiated: &negotiated,
        };
        assert_eq!(
            r.on_grant_changed(&p, NOTIFICATIONS, false, Some(session))
                .await,
            Decision::Withdrawn
        );
        assert!(r.is_empty().await);

        // Even from a session that could not use it either way.
        let empty = caps(&[]);
        assert_eq!(
            r.on_grant_changed(
                &p,
                NOTIFICATIONS,
                false,
                Some(LiveSession {
                    id: 32,
                    negotiated: &empty
                })
            )
            .await,
            Decision::Withdrawn
        );
    }

    /// **F — on/off/on converges without a storm.**
    ///
    /// Brief §4 F. Each new session may be asked once, and a session that
    /// comes up while the grant is on negotiates it and is never asked at
    /// all — so the toggling terminates. The worst case here is the
    /// adversarial one: the user switches the grant off before every
    /// reconnect completes, so each new session still lacks it.
    #[tokio::test]
    async fn f_rapid_toggling_is_bounded_by_the_reconnects_it_causes() {
        let r = Renegotiation::new();
        let p = peer(6);
        let empty = caps(&[]);
        let mut reconnects = 0;
        let mut session_id = 100;

        for _ in 0..20 {
            // on
            let session = LiveSession {
                id: session_id,
                negotiated: &empty,
            };
            if r.on_grant_changed(&p, NOTIFICATIONS, true, Some(session))
                .await
                .is_reconnect()
            {
                reconnects += 1;
                // The reconnect actually happens: a new session registers,
                // which is what clears the request.
                r.clear(&p).await;
                session_id += 1;
            }
            // off — never a request
            assert_eq!(
                r.on_grant_changed(
                    &p,
                    NOTIFICATIONS,
                    false,
                    Some(LiveSession {
                        id: session_id,
                        negotiated: &empty
                    })
                )
                .await,
                Decision::Withdrawn
            );
        }

        // One per session, never more — and every one of them was a session
        // that genuinely could not use the grant.
        assert_eq!(reconnects, 20);
        assert_eq!(session_id, 120, "each reconnect produced a new session");

        // The realistic case: the reconnect succeeds while the grant is still
        // on, so the new session negotiates it and the loop stops dead.
        let granted = caps(&["notifications.v1"]);
        assert_eq!(
            r.on_grant_changed(
                &p,
                NOTIFICATIONS,
                true,
                Some(LiveSession {
                    id: session_id,
                    negotiated: &granted
                })
            )
            .await,
            Decision::AlreadyNegotiated
        );
    }

    /// **G — a grant the session already has changes nothing.**
    ///
    /// Brief §4 G's shape. A clipboard or files grant re-asserted on a
    /// session that negotiated it must not restart anything, and neither must
    /// a notifications grant that is already in effect.
    #[tokio::test]
    async fn g_a_grant_the_session_already_negotiated_is_inert() {
        let r = Renegotiation::new();
        let p = peer(7);
        let negotiated = caps(&["battery.v1", "clipboard.v1", "notifications.v1"]);
        let session = LiveSession {
            id: 41,
            negotiated: &negotiated,
        };
        for capability in ["clipboard.v1", "notifications.v1", "battery.v1"] {
            assert_eq!(
                r.on_grant_changed(&p, capability, true, Some(session))
                    .await,
                Decision::AlreadyNegotiated
            );
        }
        assert!(r.is_empty().await);
    }

    /// Two peers are independent: asking one to reconnect says nothing about
    /// the other, and clearing one leaves the other outstanding.
    #[tokio::test]
    async fn requests_are_per_peer() {
        let r = Renegotiation::new();
        let (a, b) = (peer(8), peer(9));
        let empty = caps(&[]);

        assert!(r
            .on_grant_changed(
                &a,
                NOTIFICATIONS,
                true,
                Some(LiveSession {
                    id: 51,
                    negotiated: &empty
                })
            )
            .await
            .is_reconnect());
        assert!(r
            .on_grant_changed(
                &b,
                NOTIFICATIONS,
                true,
                Some(LiveSession {
                    id: 52,
                    negotiated: &empty
                })
            )
            .await
            .is_reconnect());
        assert_eq!(r.len().await, 2);

        r.clear(&a).await;
        assert_eq!(r.outstanding(&a).await, None);
        assert_eq!(r.outstanding(&b).await, Some(52));
    }

    /// A new session for a peer that had an outstanding request is asked
    /// again only if it *still* cannot use the grant.
    #[tokio::test]
    async fn a_replacement_session_is_judged_on_its_own_capabilities() {
        let r = Renegotiation::new();
        let p = peer(10);
        let empty = caps(&[]);
        let granted = caps(&["notifications.v1"]);

        assert!(r
            .on_grant_changed(
                &p,
                NOTIFICATIONS,
                true,
                Some(LiveSession {
                    id: 61,
                    negotiated: &empty
                })
            )
            .await
            .is_reconnect());

        // The replacement negotiated it: nothing further happens, and this is
        // true even before `clear` runs.
        assert_eq!(
            r.on_grant_changed(
                &p,
                NOTIFICATIONS,
                true,
                Some(LiveSession {
                    id: 62,
                    negotiated: &granted
                })
            )
            .await,
            Decision::AlreadyNegotiated
        );
    }

    /// Every decision has a distinct, content-free name.
    #[test]
    fn decision_names_are_distinct_and_carry_nothing() {
        let all = [
            Decision::Withdrawn,
            Decision::NoSession,
            Decision::AlreadyNegotiated,
            Decision::AlreadyRequested,
            Decision::Reconnect,
        ];
        let names: std::collections::BTreeSet<_> = all.iter().map(|d| d.as_str()).collect();
        assert_eq!(names.len(), all.len());
        assert!(all.iter().filter(|d| d.is_reconnect()).count() == 1);
    }
}
