//! Who is allowed to use the clipboard, asked freshly.
//!
//! The policy *type* lives in [`omnibridge_core::clipboard_policy`], because it
//! is persisted in the trust store and the trust store is core's. What lives
//! here is the question the capability actually asks at runtime, and the
//! contract the daemon answers it with.

pub use omnibridge_core::clipboard_policy::ClipboardPolicy;

/// Answers, freshly, what a peer is allowed to do with the clipboard.
///
/// Consulted on every inbound update and before every outbound one, never
/// captured once at handshake time — the same rule `files.v1`'s
/// `FilesAuthorizer` follows, and for the same reason: a grant withdrawn one
/// second ago has to be in force now, including against a session that is
/// already established. `clipboard.v1` needs this more than `files.v1` does,
/// because there is no second connection to tear down and therefore nothing
/// else that would notice.
#[async_trait::async_trait]
pub trait ClipboardAuthorizer: Send + Sync {
    /// The effective policy for `peer` right now.
    ///
    /// Implementations must return [`ClipboardPolicy::DENIED`] when the peer
    /// is unknown, revoked, or has no `clipboard.v1` grant — the grant check
    /// and the policy lookup are one call so that a caller cannot do one and
    /// forget the other.
    async fn policy_for(&self, peer: &omnibridge_core::Fingerprint) -> ClipboardPolicy;

    /// Every peer that should receive local clipboard changes automatically:
    /// granted `clipboard.v1`, not revoked, and [`ClipboardPolicy::
    /// may_auto_send`] true.
    ///
    /// This is what decides whether a local clipboard watcher runs at all. An
    /// empty answer means the desktop watches nothing, holds no helper
    /// process and opens no X connection — which is the difference between a
    /// feature that is off and a feature that is merely quiet.
    ///
    /// [`ClipboardPolicy::may_auto_send`]: ClipboardPolicy::may_auto_send
    async fn auto_send_peers(&self) -> Vec<omnibridge_core::Fingerprint>;
}
