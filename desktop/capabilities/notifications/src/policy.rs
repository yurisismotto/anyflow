//! Who is allowed to put a notification on this screen, asked freshly.
//!
//! The policy *type* lives in [`omnibridge_core::notification_policy`], because
//! it is persisted and the trust store is core's. What lives here is the
//! question the capability actually asks at runtime, and the contract the
//! daemon answers it with.

pub use omnibridge_core::notification_policy::{LockPolicy, NotificationPolicy};

/// Answers, freshly, what a peer is allowed to display here.
///
/// Consulted on **every inbound message**, never captured once at handshake
/// time. `files.v1` and `clipboard.v1` follow the same rule for the same
/// reason — a grant withdrawn one second ago has to be in force now, including
/// against a session that is already established — and `notifications.v1`
/// needs it more than either, because the transport's own grant filter is
/// applied when the session is built and a revocation after that point would
/// otherwise not be noticed until the phone reconnected.
#[async_trait::async_trait]
pub trait NotificationAuthorizer: Send + Sync {
    /// The effective policy for `peer` right now.
    ///
    /// Implementations must return [`NotificationPolicy::DENIED`] when the
    /// peer is unknown, revoked, or has no `notifications.v1` grant — the
    /// grant check and the policy lookup are one call so that a caller cannot
    /// do one and forget the other.
    async fn policy_for(&self, peer: &omnibridge_core::Fingerprint) -> NotificationPolicy;
}

/// The authorizer used before one is wired, and the one a misconfigured
/// composition gets.
///
/// Denies everything. A capability with no authorizer is not a capability with
/// no restrictions; "we cannot tell" and "yes" are different answers and only
/// one of them is safe.
pub struct DenyAll;

#[async_trait::async_trait]
impl NotificationAuthorizer for DenyAll {
    async fn policy_for(&self, _peer: &omnibridge_core::Fingerprint) -> NotificationPolicy {
        NotificationPolicy::DENIED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnibridge_core::Fingerprint;

    #[tokio::test]
    async fn the_default_authorizer_denies() {
        let fp = Fingerprint::from_hex(&"ab".repeat(32)).expect("fingerprint");
        assert!(DenyAll.policy_for(&fp).await.is_denied());
    }
}
