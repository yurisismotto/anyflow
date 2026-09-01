//! The Linux adapter.
//!
//! Everything the AnyFlow Agent needs that is specific to a Linux desktop
//! session, and nothing else:
//!
//! * the control endpoint — a Unix domain socket under `$XDG_RUNTIME_DIR`,
//!   implementing [`anyflow_control::transport::ControlTransport`];
//! * the client half of that endpoint, so the CLI and the GUI can reach the
//!   agent without depending on the agent;
//! * the store adapter — `$XDG_DATA_HOME/anyflow`, 0600 keys in a 0700
//!   directory — assembled from the pieces in `anyflow-core`.
//!
//! # What is *not* here
//!
//! Protocol, TLS, pairing, pinning, the session state machine, the capability
//! registry, filename sanitisation, clipboard policy. None of it is
//! platform-specific and none of it may move here. If a future change wants
//! to put a protocol decision in an adapter, that is the signal that the seam
//! is in the wrong place.
//!
//! # The AnyFlow Agent
//!
//! "Agent" is the portable name for the always-on user-session process. It is
//! one concept with a different lifetime on each platform:
//!
//! | Platform | Lifetime |
//! | --- | --- |
//! | **Linux** | `systemd --user` unit — *this crate* |
//! | Windows | a user-session agent, not a Windows Service |
//! | macOS | a `LoginItem` / `SMAppService` agent |
//! | Android | a foreground service |
//! | iOS | foreground-oriented, with no background socket |
//!
//! Wave 0 implements the Linux row and only the Linux row. The rows below it
//! are recorded so that the shape of this crate — bind an endpoint, resolve
//! paths, enforce local protection — is legible as *one row of a table*
//! rather than as the way AnyFlow works.

use std::path::{Path, PathBuf};

use anyflow_control::transport::{BindError, ControlListener, ControlTransport};
use tokio::net::{UnixListener, UnixStream};

pub use anyflow_core::platform::unix_fs::{default_data_dir, default_device_name, FileSecretStore};

/// Opens the store at `dir` using this platform's storage and identity
/// backing.
///
/// The one place that says "this machine is a Linux machine". Before Wave 0
/// the value was hardcoded inside `anyflow-core`'s persistence layer, which
/// meant the storage code decided what kind of device this was.
pub fn open_store(dir: impl AsRef<Path>) -> anyflow_core::Result<anyflow_core::store::Store> {
    use std::sync::Arc;
    anyflow_core::store::Store::open_with(anyflow_core::store::StoreConfig {
        secrets: Arc::new(FileSecretStore::new(dir.as_ref())),
        backend: Arc::new(anyflow_core::identity::SoftwareBacking),
        platform: anyflow_proto::v1::Platform::Linux,
        default_device_name: default_device_name(),
    })
}

// ---------------------------------------------------------------------------
// The control endpoint
// ---------------------------------------------------------------------------

/// Path of the control socket.
///
/// `XDG_RUNTIME_DIR` is per-user and mode 0700, so the socket is not
/// reachable by other local users. If it is unset (an unusual login), we fall
/// back to a per-uid path under `/tmp` and create it 0700 ourselves.
pub fn control_socket_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/anyflow-{}", nix_uid())));
    base.join("anyflow").join("control.sock")
}

fn nix_uid() -> u32 {
    // Avoids a `libc`/`nix` dependency for one number. `/proc/self/status` is
    // always present on Linux, which is the only platform this adapter is
    // for — that is what makes reading it acceptable here and unacceptable in
    // the portable crates.
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Uid:"))
                .and_then(|l| l.split_whitespace().next().map(str::to_string))
        })
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// The Unix-domain-socket control transport.
#[derive(Debug, Clone)]
pub struct UnixControlTransport {
    path: PathBuf,
}

impl UnixControlTransport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The default location for this session.
    pub fn default_endpoint() -> Self {
        Self::new(control_socket_path())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Releases the endpoint without holding the listener.
    ///
    /// The agent hands its listener to a task and then wants to unbind on
    /// shutdown, which is a different lifetime from the listener's. Naming it
    /// here rather than calling `remove_file` in `main` keeps "unbind the
    /// control endpoint" a concept the adapter owns — a named pipe has no
    /// file to unlink, and the binary should not have to know that.
    pub fn release(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl ControlTransport for UnixControlTransport {
    type Listener = UnixControlListener;

    fn bind(&self) -> Result<Self::Listener, BindError> {
        bind(&self.path)
    }

    fn endpoint(&self) -> String {
        self.path.display().to_string()
    }
}

/// A bound control socket.
#[derive(Debug)]
pub struct UnixControlListener {
    inner: UnixListener,
    path: PathBuf,
}

#[async_trait::async_trait]
impl ControlListener for UnixControlListener {
    type Stream = UnixStream;

    async fn accept(&self) -> std::io::Result<Self::Stream> {
        let (stream, _) = self.inner.accept().await?;
        Ok(stream)
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }

    fn release(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Binds the control socket, replacing a stale one left by a crash.
///
/// # Live owner versus stale file
///
/// A leftover socket file from an unclean shutdown must be removed, or `bind`
/// fails for ever. A socket file with a *live* daemon behind it must not be,
/// because removing it would silently steal the endpoint from a running
/// agent. The two are told apart by connecting: a stale socket refuses with
/// `ECONNREFUSED`, a live one accepts.
///
/// The pre-Wave-0 code removed unconditionally. That was defensible when only
/// one implementation existed and the reasoning was "a second live daemon
/// would have failed its own port bind first" — but the port bind happens
/// *after* this one in `main`, and on Windows the equivalent mistake is a
/// named-pipe squat. The distinguishable [`BindError::AlreadyOwned`] is what
/// a named-pipe implementation needs, so Linux establishes the semantics now.
pub fn bind(path: &Path) -> Result<UnixControlListener, BindError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        harden(parent, 0o700)?;
    }

    match path.try_exists() {
        Ok(true) => {
            if socket_has_live_owner(path) {
                return Err(BindError::AlreadyOwned {
                    detail: format!("{} is accepting connections", path.display()),
                });
            }
            // Stale. Safe to remove: only this user can reach the directory.
            std::fs::remove_file(path)?;
        }
        Ok(false) => {}
        // A metadata error is not absence. Refusing here is the same rule the
        // identity store follows: never act on "probably not there".
        Err(e) => {
            return Err(BindError::Io(std::io::Error::new(
                e.kind(),
                format!("{} could not be examined: {e}", path.display()),
            )))
        }
    }

    let listener = UnixListener::bind(path)?;
    harden(path, 0o600)?;
    Ok(UnixControlListener {
        inner: listener,
        path: path.to_path_buf(),
    })
}

/// Whether something is listening on this socket right now.
fn socket_has_live_owner(path: &Path) -> bool {
    // A blocking connect on a Unix socket to a local path either succeeds
    // immediately or fails immediately; there is no network round trip to
    // wait for, so this cannot hang the way a TCP probe could.
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

fn harden(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

/// Connects to the agent's control endpoint.
///
/// The client half, so that `anyflow-cli` and `anyflow-gui` reach the agent
/// through this crate rather than through the agent's own crate.
pub async fn connect(path: &Path) -> std::io::Result<UnixStream> {
    UnixStream::connect(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binding_twice_reports_already_owned_and_not_a_generic_io_error() {
        // The property a Windows named-pipe implementation depends on: a name
        // that is already owned must be distinguishable, so the agent can
        // abort instead of quietly choosing another name.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");

        let _first = bind(&path).expect("first bind");
        match bind(&path) {
            Err(BindError::AlreadyOwned { .. }) => {}
            other => panic!("expected AlreadyOwned, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_stale_socket_file_is_replaced_rather_than_refused() {
        // The ordinary case after an unclean shutdown: a socket file with
        // nothing behind it. Refusing here would leave the agent unable to
        // start until someone deleted a file by hand.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");

        {
            let _listener = bind(&path).expect("first bind");
        } // dropped: the file remains, the listener does not

        assert!(path.exists(), "the socket file must survive the drop");
        let _second = bind(&path).expect("a stale socket must be replaced");
    }

    #[tokio::test]
    async fn a_bound_socket_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");
        let _listener = bind(&path).expect("bind");
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    }

    #[tokio::test]
    async fn release_removes_the_endpoint() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("control.sock");
        let listener = bind(&path).expect("bind");
        assert!(path.exists());
        listener.release();
        assert!(!path.exists());
    }

    #[test]
    fn the_control_socket_path_follows_the_runtime_directory() {
        // `XDG_RUNTIME_DIR` is only read, never set: mutating the environment
        // would race every other test in this binary.
        let path = control_socket_path();
        assert!(path.ends_with("anyflow/control.sock"), "{path:?}");
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            assert!(path.starts_with(PathBuf::from(runtime)), "{path:?}");
        }
    }

    #[test]
    fn the_store_adapter_reports_this_platform_and_a_software_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_store(dir.path()).expect("open");
        assert_eq!(
            store.identity().platform(),
            anyflow_proto::v1::Platform::Linux
        );
        assert_eq!(
            store.key_backing(),
            anyflow_core::identity::KeyBacking::Software
        );
    }
}
