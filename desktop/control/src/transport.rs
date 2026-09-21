//! The shape of the local control endpoint, without naming one.
//!
//! On Linux the endpoint is a Unix domain socket in `$XDG_RUNTIME_DIR`, and
//! Wave 0 ships exactly that one implementation. The trait exists because
//! `tokio::net::UnixStream` is `#[cfg(unix)]`, because Windows `AF_UNIX`
//! carries no peer credentials, and because a Windows named pipe needs an
//! explicit DACL — its default grants read access to *Everyone*, including
//! anonymous logon.
//!
//! # The one contract that is not in the signatures
//!
//! [`ControlTransport::bind`] must fail, **distinguishably**, when the
//! endpoint name is already owned by another process, and it must never fall
//! back to a different name. A fallback is exactly what an attacker wants: if
//! the agent quietly moves to `control-2.sock` when `control.sock` is taken,
//! clients have to search, and a squatter that grabbed the first name gets to
//! answer for the agent. The Windows mitigation for this is
//! `FILE_FLAG_FIRST_PIPE_INSTANCE` plus an abort; the error variant it needs
//! is [`BindError::AlreadyOwned`], and it exists in Wave 0 — even though
//! Linux barely needs it — because retrofitting a variant into a trait after
//! adapters exist is precisely the churn Wave 0 is meant to prevent.

use tokio::io::{AsyncRead, AsyncWrite};

/// Why binding the local control endpoint failed.
#[derive(Debug)]
pub enum BindError {
    /// Another process already owns this endpoint.
    ///
    /// **Fatal. Never retry, and never bind a different name.** See the
    /// module documentation.
    AlreadyOwned {
        detail: String,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyOwned { detail } => write!(
                f,
                "the local control endpoint is already owned by another \
                 process: {detail}. OmniBridge will not bind an alternative name, \
                 because a client that has to search for the agent can be \
                 answered by whatever squatted on the first one"
            ),
            Self::Io(e) => write!(f, "could not bind the local control endpoint: {e}"),
        }
    }
}

impl std::error::Error for BindError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::AlreadyOwned { .. } => None,
        }
    }
}

impl From<std::io::Error> for BindError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One accepted control connection.
///
/// Only the two halves of a byte stream are needed: the protocol above is
/// newline-delimited JSON and has no socket semantics in it at all, which is
/// what makes a named pipe a drop-in later.
pub trait ControlStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> ControlStream for T {}

/// A bound local control endpoint.
#[async_trait::async_trait]
pub trait ControlListener: Send + Sync + 'static {
    type Stream: ControlStream;

    /// Waits for the next local client.
    async fn accept(&self) -> std::io::Result<Self::Stream>;

    /// One line for the log, e.g. the socket path. Never a secret.
    fn describe(&self) -> String;

    /// Releases the endpoint — unlinking a socket file, closing a pipe.
    ///
    /// Idempotent, and called on shutdown. It is a method rather than a
    /// `Drop` impl because the daemon unbinds explicitly, in a place where it
    /// can log the failure.
    fn release(&self);
}

/// Binds the local control endpoint for this platform.
pub trait ControlTransport: Send + Sync {
    type Listener: ControlListener;

    /// Binds, or fails.
    ///
    /// Implementations **must** return [`BindError::AlreadyOwned`] when the
    /// name is held by a live process, and **must not** fall back to another
    /// name.
    fn bind(&self) -> Result<Self::Listener, BindError>;

    /// Where this transport binds, for diagnostics and for the client half to
    /// find. Never a secret.
    fn endpoint(&self) -> String;
}
