//! The storage seam: bytes in, bytes out, and the difference between
//! "absent" and "unreadable".
//!
//! # Why this exists
//!
//! Everything *policy* about persisted state — the schema version, refusing a
//! newer schema, the JSON shape, the trust-store semantics, the
//! write-then-rename — is portable and stays in [`crate::store`]. What is not
//! portable is the last step: where the bytes go, and how the platform is
//! asked to keep them private. Unix modes have no Windows equivalent; a DACL
//! is not `0o600`, and quietly dropping the check on a platform that spells
//! it differently would be a silent security regression.
//!
//! # The distinction this trait exists to preserve
//!
//! Before Wave 0, `Store::open` asked `Path::exists()` and treated `false` as
//! "first run". `Path::exists()` returns `false` for *any* metadata error —
//! `EACCES`, a broken symlink, a parent directory that cannot be traversed —
//! so an identity that was merely unreadable was indistinguishable from one
//! that had never existed, and the next line overwrote `state.json`. That
//! destroys the trust store: every pairing, every capability grant, and the
//! fingerprint continuity every peer has pinned.
//!
//! So the contract here is deliberately narrow:
//!
//! ```text
//!   Ok(None)                       the item is genuinely absent
//!   Err(NotPrivate)                present, readable, and not private enough
//!   Err(PermissionDenied)          present, and this process may not read it
//!   Err(Io)                        present or absent — we could not find out
//!   Err(Corrupted)                 present, readable, and not what it claims
//! ```
//!
//! **`Ok(None)` is the only value that may lead to creating a new identity.**
//! Nothing else is permitted to collapse into it.

use std::sync::Arc;

use crate::error::Error;

/// The item name the identity's key material is stored under.
///
/// A name, not a path: a Windows or Apple implementation stores it somewhere
/// that is not a file at all.
pub const IDENTITY_SECRET: &str = "identity";

/// Why a secret-store operation did not produce a value.
///
/// Each variant is a distinct answer that must not be merged with another.
/// Merging is precisely the defect Wave 0 fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreAccessError {
    /// The item exists but this process is not allowed to read it.
    ///
    /// **Not** absence. An identity behind `EACCES` is an identity that
    /// exists.
    PermissionDenied { item: String, detail: String },

    /// The item exists and is readable, but the platform's private-storage
    /// guarantee is not met — a key at mode 0644, a data directory other
    /// local users can enter.
    ///
    /// A hard error rather than a warning: continuing would mean running with
    /// an identity that is compromised by construction.
    NotPrivate { item: String, detail: String },

    /// Something went wrong that is neither absence nor permission — a full
    /// disk, a disconnected network mount, a transient failure. We do not
    /// know whether the item exists, and **not knowing is never a licence to
    /// create one**.
    Io { item: String, detail: String },

    /// Present and readable, but not what it claims to be.
    Corrupted { item: String, detail: String },
}

impl StoreAccessError {
    pub fn item(&self) -> &str {
        match self {
            Self::PermissionDenied { item, .. }
            | Self::NotPrivate { item, .. }
            | Self::Io { item, .. }
            | Self::Corrupted { item, .. } => item,
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::PermissionDenied { detail, .. }
            | Self::NotPrivate { detail, .. }
            | Self::Io { detail, .. }
            | Self::Corrupted { detail, .. } => detail,
        }
    }

    /// Whether a retry, without human intervention, could plausibly succeed.
    ///
    /// Only [`StoreAccessError::Io`]. A permission problem, a loose mode and
    /// a corrupt file all need somebody to do something.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Io { .. })
    }
}

impl std::fmt::Display for StoreAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied { item, detail } => {
                write!(f, "{item} exists but cannot be read: {detail}")
            }
            Self::NotPrivate { item, detail } => write!(f, "{detail} ({item})"),
            Self::Io { item, detail } => write!(f, "{item} could not be accessed: {detail}"),
            Self::Corrupted { item, detail } => write!(f, "{item} is corrupted: {detail}"),
        }
    }
}

impl std::error::Error for StoreAccessError {}

impl From<StoreAccessError> for Error {
    fn from(e: StoreAccessError) -> Self {
        Error::Store(e.to_string())
    }
}

pub type StoreResult<T> = std::result::Result<T, StoreAccessError>;

/// Where identity material and persisted state actually live.
///
/// Implementations must honour three rules that the signatures cannot say:
///
/// * **`Ok(None)` means genuinely absent.** Never return it for a permission
///   or I/O error. This is the load-bearing rule.
/// * **Reads enforce protection before returning bytes.** A key that is not
///   private enough must fail with [`StoreAccessError::NotPrivate`], not be
///   returned with a warning.
/// * **Writes are atomic and are created already at the final protection.**
///   A temp file created world-readable and chmod-ed afterwards has a window
///   in which the private key is exposed; a Windows implementation that sets
///   its DACL after `CreateFile` reintroduces exactly that window.
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    /// Reads one secret. `Ok(None)` if, and only if, it does not exist.
    fn read_secret(&self, name: &str) -> StoreResult<Option<Vec<u8>>>;

    /// Writes one secret, atomically, at the platform's private protection.
    fn write_secret(&self, name: &str, data: &[u8]) -> StoreResult<()>;

    /// Reads the state document. `Ok(None)` if, and only if, it does not
    /// exist.
    fn read_state(&self) -> StoreResult<Option<Vec<u8>>>;

    /// Writes the state document, atomically.
    fn write_state(&self, data: &[u8]) -> StoreResult<()>;

    /// Enforces the platform's private-storage guarantee on the container
    /// itself: 0700 on Unix, an owner-only DACL on Windows.
    ///
    /// A hard error, never a warning.
    fn harden(&self) -> StoreResult<()>;

    /// Checks one secret's protection without reading it.
    fn verify_protection(&self, name: &str) -> StoreResult<()>;

    /// One line for diagnostics. Must not contain secret material.
    fn describe(&self) -> String;
}

impl<T: SecretStore + ?Sized> SecretStore for Arc<T> {
    fn read_secret(&self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        (**self).read_secret(name)
    }
    fn write_secret(&self, name: &str, data: &[u8]) -> StoreResult<()> {
        (**self).write_secret(name, data)
    }
    fn read_state(&self) -> StoreResult<Option<Vec<u8>>> {
        (**self).read_state()
    }
    fn write_state(&self, data: &[u8]) -> StoreResult<()> {
        (**self).write_state(data)
    }
    fn harden(&self) -> StoreResult<()> {
        (**self).harden()
    }
    fn verify_protection(&self, name: &str) -> StoreResult<()> {
        (**self).verify_protection(name)
    }
    fn describe(&self) -> String {
        (**self).describe()
    }
}
