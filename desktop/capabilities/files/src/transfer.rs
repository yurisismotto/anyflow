//! Transfer identity and the state machine.
//!
//! The state machine is explicit and lives here, in one type, with one
//! function that says which transitions are legal. Nothing derives a
//! transfer's state by inspecting a socket, a file on disk or a log line:
//! there is exactly one place where a transfer changes state, and it refuses
//! a transition that is not in the table.

use std::fmt;

use omnibridge_proto::v1::capabilities as pb;

use crate::limits::TRANSFER_ID_LEN;

/// A transfer's identifier: 128 cryptographically random bits.
///
/// Deliberately a distinct type from the envelope's `message_id`, and never
/// derived from one. The two have different lifecycles — a message id is
/// meaningful for one frame and is garbage-collected by the replay window,
/// while a transfer id must stay meaningful across many frames and two
/// connections — and sharing them would couple a transfer's identity to the
/// dedup window's bookkeeping.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransferId([u8; TRANSFER_ID_LEN]);

impl TransferId {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; TRANSFER_ID_LEN] = bytes.try_into().ok()?;
        Some(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; TRANSFER_ID_LEN] {
        &self.0
    }

    pub fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The inverse of [`to_hex`], for the local control protocol.
    ///
    /// Exact, and never a prefix: the full id is how a human's answer to one
    /// offer is bound to that offer. A prefix match would make the binding
    /// depend on which other transfers happened to exist at the time, which
    /// is not a property a consent decision may have. Case-insensitive
    /// because that is a spelling of the same id, not a different one.
    ///
    /// [`to_hex`]: Self::to_hex
    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        // Checked up front, and it does more than reject rubbish: it is what
        // makes every two-byte slice below a valid `str`, and what stops
        // `from_str_radix` from accepting a signed form like `+a` as a
        // spelling of `0a`. One id, one spelling.
        if bytes.len() != TRANSFER_ID_LEN * 2 || !bytes.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let mut out = [0u8; TRANSFER_ID_LEN];
        for (i, byte) in out.iter_mut().enumerate() {
            let hex = std::str::from_utf8(&bytes[i * 2..i * 2 + 2]).ok()?;
            *byte = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(Self(out))
    }

    /// The form that may appear in a log: the first 8 hex characters.
    ///
    /// Enough to correlate lines about one transfer, far too little to help
    /// anyone guess the id — and the id is not a bearer token anyway (see
    /// [`crate::auth`]).
    pub fn to_display_short(self) -> String {
        self.to_hex()[..8].to_string()
    }
}

/// Prints the truncated form, so a transfer id cannot reach a log in full by
/// someone reaching for the obvious formatter.
impl fmt::Display for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_display_short())
    }
}

impl fmt::Debug for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TransferId({})", self.to_display_short())
    }
}

/// Which way the bytes go, from the point of view of this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// This device is the sender: it made the offer and provides the bytes.
    Sending,
    /// This device is the receiver: it verifies and stores the bytes.
    Receiving,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Receiving => "receiving",
        }
    }
}

/// Where a transfer is.
///
/// The set is small on purpose. Every state below is either waiting on a
/// specific event with a specific timeout, or terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    /// Sender side: the offer is out, no answer yet.
    Offered,
    /// Receiver side: the offer arrived and a human has not answered.
    /// Bounded by [`crate::limits::ACCEPT_TIMEOUT`].
    WaitingAccept,
    /// Both sides agreed. The data stream is opening or open; `bytes` says
    /// how far it has got. Bounded by
    /// [`crate::limits::STREAM_OPEN_TIMEOUT`] until the stream authenticates,
    /// and by [`crate::limits::STREAM_IDLE_TIMEOUT`] after that.
    Transferring,
    /// Receiver side: every byte is in, and the hash is being checked and the
    /// file promoted. No further bytes are accepted in this state.
    Verifying,
    /// Terminal. On the receiver this means, and only means, that the hash
    /// matched and the file was promoted to its final name.
    Completed,
    /// Terminal.
    Failed,
    /// Terminal.
    Cancelled,
}

impl TransferState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// True while the transfer still holds resources a peer can consume: a
    /// slot in the concurrency limit, a temp file, possibly a data stream.
    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offered => "offered",
            Self::WaitingAccept => "waiting_accept",
            Self::Transferring => "transferring",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// The transition table. This is the whole state machine.
    ///
    /// Note what is absent: nothing leaves a terminal state, so a duplicate
    /// `FILE_COMPLETE`, a late cancel or a second data stream for a finished
    /// transfer are all rejected by this one function rather than by a check
    /// remembered at each call site.
    pub fn can_transition_to(self, next: Self) -> bool {
        use TransferState::*;
        match (self, next) {
            // Agreement.
            (Offered, Transferring) => true,
            (WaitingAccept, Transferring) => true,

            // Receiving finished; verify before claiming anything.
            (Transferring, Verifying) => true,
            (Verifying, Completed) => true,

            // A sender has no verify step of its own: the receiver's
            // FILE_COMPLETE is what completes it.
            (Transferring, Completed) => true,

            // Giving up is allowed from any non-terminal state.
            (s, Failed) if !s.is_terminal() => true,
            (s, Cancelled) if !s.is_terminal() => true,

            _ => false,
        }
    }
}

impl fmt::Display for TransferState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Maps a failure reason onto the wire enum.
pub fn reason_to_proto(reason: FailureReason) -> pb::TransferFailureReason {
    use pb::TransferFailureReason as W;
    match reason {
        FailureReason::DeclinedByUser => W::DeclinedByUser,
        FailureReason::NotAuthorized => W::NotAuthorized,
        FailureReason::TooManyTransfers => W::TooManyTransfers,
        FailureReason::TooLarge => W::TooLarge,
        FailureReason::BadMetadata => W::BadMetadata,
        FailureReason::TimedOut => W::TimedOut,
        FailureReason::Integrity => W::Integrity,
        FailureReason::Storage => W::Storage,
        FailureReason::Transport => W::Transport,
        FailureReason::CancelledByUser => W::CancelledByUser,
        FailureReason::Revoked => W::Revoked,
        FailureReason::UnknownTransfer => W::UnknownTransfer,
    }
}

pub fn reason_from_proto(reason: pb::TransferFailureReason) -> FailureReason {
    use pb::TransferFailureReason as W;
    match reason {
        W::DeclinedByUser => FailureReason::DeclinedByUser,
        W::NotAuthorized => FailureReason::NotAuthorized,
        W::TooManyTransfers => FailureReason::TooManyTransfers,
        W::TooLarge => FailureReason::TooLarge,
        W::BadMetadata => FailureReason::BadMetadata,
        W::TimedOut => FailureReason::TimedOut,
        W::Integrity => FailureReason::Integrity,
        W::Storage => FailureReason::Storage,
        W::Transport => FailureReason::Transport,
        W::CancelledByUser => FailureReason::CancelledByUser,
        W::Revoked => FailureReason::Revoked,
        // An unspecified or unrecognised reason from a newer peer is not an
        // error in itself; it just tells us nothing.
        W::UnknownTransfer | W::Unspecified => FailureReason::UnknownTransfer,
    }
}

/// Why a transfer ended other than successfully.
///
/// Every variant is safe to show a user and safe to send a peer: no paths, no
/// filenames, no errno text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    DeclinedByUser,
    NotAuthorized,
    TooManyTransfers,
    TooLarge,
    BadMetadata,
    TimedOut,
    Integrity,
    Storage,
    Transport,
    CancelledByUser,
    Revoked,
    UnknownTransfer,
}

impl FailureReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeclinedByUser => "declined by the user",
            Self::NotAuthorized => "peer is not allowed to transfer files",
            Self::TooManyTransfers => "too many transfers at once",
            Self::TooLarge => "file is larger than the configured limit",
            Self::BadMetadata => "the offer's metadata was unusable",
            Self::TimedOut => "timed out",
            Self::Integrity => "the received data did not match the offer",
            Self::Storage => "could not store the file",
            Self::Transport => "the connection ended mid-transfer",
            Self::CancelledByUser => "cancelled",
            Self::Revoked => "the device's pairing was revoked",
            Self::UnknownTransfer => "no such transfer",
        }
    }

    /// Whether this reason should be reported as a cancellation rather than a
    /// failure. Keeping them distinct is what stops "the user pressed cancel"
    /// from being displayed as an error.
    pub fn is_cancellation(self) -> bool {
        matches!(self, Self::CancelledByUser | Self::DeclinedByUser)
    }

    /// The stable token a local front end may branch on.
    ///
    /// [`as_str`] is a sentence for a person to read, and rewording one is a
    /// copy change that must stay free. A front end that needs to tell
    /// *declined* from *timed out* — to choose its own calm wording for each —
    /// cannot get that from prose without making every reword a silent
    /// behaviour change somewhere else. This is the machine-readable half, and
    /// the two are deliberately separate.
    ///
    /// The tokens are the names in
    /// [`omnibridge_control::transfer_failure`](../../../control/src/lib.rs); the
    /// correspondence is pinned by a test in `desktop/runtime`, which is the
    /// one crate that can see both.
    ///
    /// [`as_str`]: Self::as_str
    pub fn code(self) -> &'static str {
        match self {
            Self::DeclinedByUser => "declined_by_user",
            Self::NotAuthorized => "not_authorized",
            Self::TooManyTransfers => "too_many_transfers",
            Self::TooLarge => "too_large",
            Self::BadMetadata => "bad_metadata",
            Self::TimedOut => "timed_out",
            Self::Integrity => "integrity",
            Self::Storage => "storage",
            Self::Transport => "transport",
            Self::CancelledByUser => "cancelled_by_user",
            Self::Revoked => "revoked",
            Self::UnknownTransfer => "unknown_transfer",
        }
    }

    /// Every variant, so a new one cannot be added without the tests that
    /// enumerate them noticing.
    pub const ALL: [FailureReason; 12] = [
        Self::DeclinedByUser,
        Self::NotAuthorized,
        Self::TooManyTransfers,
        Self::TooLarge,
        Self::BadMetadata,
        Self::TimedOut,
        Self::Integrity,
        Self::Storage,
        Self::Transport,
        Self::CancelledByUser,
        Self::Revoked,
        Self::UnknownTransfer,
    ];
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A code is a token, not a sentence: no spaces, and distinct per
    /// variant. A front end branches on these, so a collision would silently
    /// merge two outcomes into one label.
    #[test]
    fn every_failure_reason_has_a_distinct_machine_code() {
        let mut seen = std::collections::BTreeSet::new();
        for reason in FailureReason::ALL {
            let code = reason.code();
            assert!(
                !code.is_empty() && !code.contains(' ') && code == code.to_ascii_lowercase(),
                "{code:?} is not a token"
            );
            assert!(seen.insert(code), "two reasons share the code {code:?}");
            // The prose and the token are separate on purpose: rewording the
            // sentence must not change what anything branches on.
            assert_ne!(code, reason.as_str(), "the token is just the prose");
        }
        assert_eq!(seen.len(), FailureReason::ALL.len());
    }

    #[test]
    fn a_transfer_id_is_exactly_sixteen_bytes() {
        assert!(TransferId::from_bytes(&[0u8; 16]).is_some());
        assert!(TransferId::from_bytes(&[0u8; 15]).is_none());
        assert!(TransferId::from_bytes(&[0u8; 17]).is_none());
        assert!(TransferId::from_bytes(&[]).is_none());
    }

    #[test]
    fn a_transfer_id_round_trips_through_hex() {
        let id = TransferId::from_bytes(&[
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0, 1, 2, 3, 4, 5, 6, 7,
        ])
        .expect("id");
        assert_eq!(TransferId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(TransferId::from_hex(&id.to_hex().to_uppercase()), Some(id));
    }

    #[test]
    fn a_transfer_id_is_never_parsed_from_a_prefix_or_from_rubbish() {
        let id = TransferId::from_bytes(&[0xab; 16]).expect("id");
        let hex = id.to_hex();
        // A prefix is not an id: a consent decision must name exactly one
        // transfer, whatever else happens to exist.
        assert_eq!(TransferId::from_hex(&hex[..8]), None);
        assert_eq!(TransferId::from_hex(&format!("{hex}00")), None);
        assert_eq!(TransferId::from_hex(""), None);
        assert_eq!(TransferId::from_hex(&"z".repeat(32)), None);
        // Multi-byte characters are 32 *bytes* here and must not be sliced
        // into on the way to a parse.
        assert_eq!(TransferId::from_hex(&"é".repeat(16)), None);
        // And there is one spelling of an id: `from_str_radix` would take a
        // sign on each pair otherwise.
        assert_eq!(TransferId::from_hex(&"+a".repeat(16)), None);
    }

    #[test]
    fn a_transfer_id_never_formats_in_full() {
        let id = TransferId::from_bytes(&[0xab; 16]).expect("id");
        assert_eq!(id.to_string().len(), 8);
        assert_eq!(format!("{id:?}"), "TransferId(abababab)");
        // The full value is still available when one is genuinely needed.
        assert_eq!(id.to_hex().len(), 32);
    }

    #[test]
    fn nothing_leaves_a_terminal_state() {
        use TransferState::*;
        for terminal in [Completed, Failed, Cancelled] {
            for next in [
                Offered,
                WaitingAccept,
                Transferring,
                Verifying,
                Completed,
                Failed,
                Cancelled,
            ] {
                assert!(
                    !terminal.can_transition_to(next),
                    "{terminal} must not become {next}"
                );
            }
        }
    }

    #[test]
    fn a_receiver_must_verify_before_completing() {
        use TransferState::*;
        // The receiving path goes through Verifying; there is no shortcut
        // from the last byte to "done".
        assert!(Transferring.can_transition_to(Verifying));
        assert!(Verifying.can_transition_to(Completed));
        // Verifying cannot go back to moving bytes.
        assert!(!Verifying.can_transition_to(Transferring));
    }

    #[test]
    fn giving_up_is_allowed_from_every_live_state() {
        use TransferState::*;
        for live in [Offered, WaitingAccept, Transferring, Verifying] {
            assert!(live.can_transition_to(Failed), "{live}");
            assert!(live.can_transition_to(Cancelled), "{live}");
        }
    }

    #[test]
    fn a_transfer_cannot_go_backwards() {
        use TransferState::*;
        assert!(!Transferring.can_transition_to(Offered));
        assert!(!Transferring.can_transition_to(WaitingAccept));
        assert!(!Verifying.can_transition_to(WaitingAccept));
        assert!(!Offered.can_transition_to(WaitingAccept));
    }

    #[test]
    fn reason_codes_round_trip_across_the_wire() {
        for reason in [
            FailureReason::DeclinedByUser,
            FailureReason::NotAuthorized,
            FailureReason::TooManyTransfers,
            FailureReason::TooLarge,
            FailureReason::BadMetadata,
            FailureReason::TimedOut,
            FailureReason::Integrity,
            FailureReason::Storage,
            FailureReason::Transport,
            FailureReason::CancelledByUser,
            FailureReason::Revoked,
            FailureReason::UnknownTransfer,
        ] {
            assert_eq!(reason_from_proto(reason_to_proto(reason)), reason);
        }
    }

    #[test]
    fn no_failure_reason_can_leak_local_detail() {
        // Every reason is a fixed string chosen here, so a path or an errno
        // cannot reach a peer through this channel.
        for reason in [FailureReason::Storage, FailureReason::Integrity] {
            assert!(!reason.as_str().contains('/'));
        }
    }
}
