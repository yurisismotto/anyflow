use std::fmt;

/// Errors surfaced by the core library.
///
/// Display impls deliberately avoid embedding user content or secrets:
/// these strings end up in logs (principle 17).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("tls error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("protocol decode failed")]
    Decode(#[from] prost::DecodeError),

    #[error("protocol violation: {0}")]
    Protocol(&'static str),

    #[error("frame too large: {0} bytes (limit {1})")]
    FrameTooLarge(u32, u32),

    #[error("no mutually supported protocol version")]
    VersionUnsupported,

    #[error("peer is not authorized")]
    NotAuthorized,

    #[error("pairing failed: {0}")]
    Pairing(PairingError),

    #[error("certificate error: {0}")]
    Certificate(&'static str),

    #[error("identity store error: {0}")]
    Store(String),

    #[error("connection closed by peer")]
    Closed,

    #[error("peer sent a fatal error: {0:?}")]
    PeerError(anyflow_proto::v1::ErrorCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingError {
    /// No pairing session is currently open.
    NotInPairingMode,
    /// The HMAC proof did not verify.
    BadProof,
    /// The token's lifetime elapsed.
    Expired,
    /// The token was already consumed (single-use).
    AlreadyUsed,
    /// Too many failed attempts; the session was aborted.
    RateLimited,
    /// The human declined.
    DeclinedByUser,
}

impl fmt::Display for PairingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NotInPairingMode => "not in pairing mode",
            Self::BadProof => "invalid proof",
            Self::Expired => "token expired",
            Self::AlreadyUsed => "token already used",
            Self::RateLimited => "rate limited",
            Self::DeclinedByUser => "declined by user",
        };
        f.write_str(s)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
