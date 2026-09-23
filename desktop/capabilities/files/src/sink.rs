//! Where a received file lands — the seam, not the answer.
//!
//! `files.v1` has opinions about *how* a file is written that are not
//! negotiable and are not the platform's to reinterpret:
//!
//! * a dedicated directory, chosen by this machine;
//! * the destination is never influenced by the peer — an offer carries a
//!   filename and no path at all;
//! * a name is reserved by *creating* it exclusively, so a racing transfer
//!   cannot take it;
//! * the final name only ever appears on a complete, verified file, via an
//!   atomic rename;
//! * [`crate::filename::sanitize`] runs before anything touches storage.
//!
//! What *is* the platform's business is one question: where is "downloads",
//! and what does private mean there. `$XDG_DOWNLOAD_DIR` and mode 0700 have
//! no Windows equivalent — `FOLDERID_Downloads` and a DACL are the answer
//! there — and iOS has no Downloads concept at all.
//!
//! So the invariants above live here, in the capability, and only the
//! resolution and the protection live in the implementation.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::transfer::TransferId;

/// The storage a completed transfer is written into.
///
/// Implementations must honour the invariants in the module documentation.
/// In particular [`reserve`] must be a *create*, not a lookup: the
/// test-then-create form has a window in which two concurrent transfers both
/// see a name as free and the second destroys the first, which is exactly the
/// "never overwrite" property it exists to provide.
///
/// [`reserve`]: FileSink::reserve
pub trait FileSink: Send + Sync + std::fmt::Debug {
    /// The directory received files are written into.
    fn directory(&self) -> &Path;

    /// Creates the directory if needed, at the platform's private protection.
    fn prepare(&self) -> io::Result<()>;

    /// Opens the temp file a transfer streams into.
    ///
    /// It must sit **beside** its destination, because `rename(2)` is only
    /// atomic within one filesystem and a system temp directory is routinely
    /// a different one.
    fn open_temp(&self, id: TransferId) -> io::Result<(std::fs::File, PathBuf)>;

    /// Reserves a final name without ever overwriting an existing file.
    fn reserve(&self, name: &str) -> io::Result<PathBuf>;

    /// Promotes a verified temp file to its final name, atomically.
    fn promote(&self, temp: &Path, name: &str) -> io::Result<PathBuf>;

    /// One line for diagnostics.
    fn describe(&self) -> String {
        self.directory().display().to_string()
    }
}

/// A handle to this machine's file destination.
///
/// A concrete type over `Arc<dyn FileSink>` so that every existing caller
/// reaches `reserve`, `promote` and the rest without importing the trait, and
/// so `FilesConfig` stays portable while what is behind it does not.
#[derive(Clone, Debug)]
pub struct Destination(Arc<dyn FileSink>);

impl Destination {
    /// Wraps any implementation.
    pub fn from_sink(sink: Arc<dyn FileSink>) -> Self {
        Self(sink)
    }

    pub fn sink(&self) -> &Arc<dyn FileSink> {
        &self.0
    }

    /// The directory received files are stored in.
    pub fn dir(&self) -> &Path {
        self.0.directory()
    }

    pub fn prepare(&self) -> io::Result<()> {
        self.0.prepare()
    }

    pub fn open_temp(&self, id: TransferId) -> io::Result<(std::fs::File, PathBuf)> {
        self.0.open_temp(id)
    }

    pub fn reserve(&self, name: &str) -> io::Result<PathBuf> {
        self.0.reserve(name)
    }

    pub fn promote(&self, temp: &Path, name: &str) -> io::Result<PathBuf> {
        self.0.promote(temp, name)
    }

    pub fn describe(&self) -> String {
        self.0.describe()
    }
}

#[cfg(feature = "unix-fs")]
impl Destination {
    /// A destination at an explicit directory, on this machine's filesystem.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self(Arc::new(crate::destination::UnixDownloadSink::new(dir)))
    }

    /// The platform-derived default: `<downloads>/OmniBridge`.
    pub fn default_location() -> Self {
        Self::new(crate::destination::default_download_dir().join("OmniBridge"))
    }
}
