//! The Unix filesystem implementation of [`SecretStore`].
//!
//! Everything AnyFlow has always done, moved behind the trait and with the
//! absent/unreadable confusion removed:
//!
//! * `$XDG_DATA_HOME/anyflow`, else `~/.local/share/anyflow`;
//! * `identity.key` at mode 0600 inside a directory at mode 0700;
//! * write-then-rename, with the temp file created **already** at the final
//!   mode so there is no window in which the key is world-readable;
//! * a loose mode is a hard error on read, not a warning.
//!
//! The only behavioural change is the one Wave 0 exists to make:
//! [`Path::try_exists`] replaces [`Path::exists`], and each error kind is
//! reported as itself. `Path::exists()` answers `false` for `EACCES`, for a
//! broken symlink, and for a parent directory that cannot be traversed — so
//! an identity that merely could not be read looked exactly like one that had
//! never been created, and the caller's next act was to overwrite the trust
//! store.

use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::secret_store::{SecretStore, StoreAccessError, StoreResult};

/// Mode for the data directory: owner-only, and not traversable by anyone
/// else, so no other local user can even see what is in it.
const DIR_MODE: u32 = 0o700;
/// Mode for key material and for `state.json`.
const FILE_MODE: u32 = 0o600;

/// The state document's filename. Unchanged from before Wave 0 (CC-3).
const STATE_FILE: &str = "state.json";

/// A secret store backed by a private directory on a Unix filesystem.
#[derive(Debug, Clone)]
pub struct FileSecretStore {
    dir: PathBuf,
}

impl FileSecretStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Resolves a secret *name* to a file.
    ///
    /// Names are a fixed vocabulary chosen by this crate, never by a peer.
    /// The separator check is defence in depth: if a name ever became
    /// caller-supplied, a `../` in it must not escape the data directory.
    fn secret_path(&self, name: &str) -> StoreResult<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return Err(StoreAccessError::Corrupted {
                item: name.to_string(),
                detail: "not a valid secret name".into(),
            });
        }
        Ok(self.dir.join(format!("{name}.key")))
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join(STATE_FILE)
    }
}

/// Turns an `io::Error` into the one thing it actually means.
///
/// The whole point of this function is that `NotFound` is *not* produced
/// here: absence is `Ok(None)` at the call site, and every error kind that is
/// not absence stays an error.
fn classify(item: &str, path: &Path, e: &io::Error) -> StoreAccessError {
    match e.kind() {
        io::ErrorKind::PermissionDenied => StoreAccessError::PermissionDenied {
            item: item.to_string(),
            detail: format!("{}: {e}", path.display()),
        },
        _ => StoreAccessError::Io {
            item: item.to_string(),
            detail: format!("{}: {e}", path.display()),
        },
    }
}

/// Whether a path exists, without ever answering "no" for the wrong reason.
///
/// `Path::try_exists()` distinguishes "stat said ENOENT" from "stat failed",
/// which is the distinction the pre-Wave-0 code did not have.
fn exists(item: &str, path: &Path) -> StoreResult<bool> {
    path.try_exists().map_err(|e| classify(item, path, &e))
}

/// Refuses a file that anyone but its owner can reach.
///
/// A hard error, not a warning. Continuing would mean running with an
/// identity that is compromised by construction.
fn require_private_mode(item: &str, path: &Path) -> StoreResult<()> {
    let meta = std::fs::metadata(path).map_err(|e| classify(item, path, &e))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(StoreAccessError::NotPrivate {
            item: item.to_string(),
            detail: format!(
                "{} has mode {:o}; the private key must not be group- or \
                 world-accessible. Fix with: chmod 600 {}",
                path.display(),
                mode,
                path.display()
            ),
        });
    }
    Ok(())
}

/// Writes via a temp file + rename so a crash mid-write cannot leave a
/// truncated trust store.
///
/// The temp file is created **with the final mode**, so there is no window in
/// which the key is world-readable. Any reimplementation of this trait must
/// preserve that property: setting the protection after the file exists is a
/// different, weaker thing.
fn write_atomic(item: &str, path: &Path, data: &[u8], mode: u32) -> StoreResult<()> {
    use std::io::Write;

    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&tmp)
        .map_err(|e| classify(item, &tmp, &e))?;
    f.write_all(data).map_err(|e| classify(item, &tmp, &e))?;
    f.sync_all().map_err(|e| classify(item, &tmp, &e))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| classify(item, path, &e))?;
    Ok(())
}

impl SecretStore for FileSecretStore {
    fn read_secret(&self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        let path = self.secret_path(name)?;
        if !exists(name, &path)? {
            return Ok(None);
        }
        // Protection is checked *before* the bytes are handed over, which is
        // strictly earlier than any caller could do it.
        require_private_mode(name, &path)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            // A file that existed a moment ago and does not now is a race,
            // not an absence: reporting it as absence would be exactly the
            // collapse this trait forbids.
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(StoreAccessError::Io {
                item: name.to_string(),
                detail: format!("{} vanished while being read", path.display()),
            }),
            Err(e) => Err(classify(name, &path, &e)),
        }
    }

    fn write_secret(&self, name: &str, data: &[u8]) -> StoreResult<()> {
        let path = self.secret_path(name)?;
        write_atomic(name, &path, data, FILE_MODE)
    }

    fn read_state(&self) -> StoreResult<Option<Vec<u8>>> {
        let path = self.state_path();
        if !exists(STATE_FILE, &path)? {
            return Ok(None);
        }
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Err(StoreAccessError::Io {
                item: STATE_FILE.into(),
                detail: format!("{} vanished while being read", path.display()),
            }),
            Err(e) => Err(classify(STATE_FILE, &path, &e)),
        }
    }

    fn write_state(&self, data: &[u8]) -> StoreResult<()> {
        write_atomic(STATE_FILE, &self.state_path(), data, FILE_MODE)
    }

    fn harden(&self) -> StoreResult<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| classify("data directory", &self.dir, &e))?;
        let meta =
            std::fs::metadata(&self.dir).map_err(|e| classify("data directory", &self.dir, &e))?;
        let mut perms = meta.permissions();
        if perms.mode() & 0o077 != 0 {
            perms.set_mode(DIR_MODE);
            std::fs::set_permissions(&self.dir, perms)
                .map_err(|e| classify("data directory", &self.dir, &e))?;
        }
        Ok(())
    }

    fn verify_protection(&self, name: &str) -> StoreResult<()> {
        let path = self.secret_path(name)?;
        require_private_mode(name, &path)
    }

    fn describe(&self) -> String {
        format!("unix filesystem at {}", self.dir.display())
    }
}

/// Default location: `$XDG_DATA_HOME/anyflow`, else `~/.local/share/…`.
pub fn default_data_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("anyflow")
    } else {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share/anyflow")
    }
}

/// This machine's name, for the `device_name` a peer sees.
///
/// `/etc/hostname` is the Linux answer; the fallback is deliberately no
/// longer the name of one distribution. A user who sees "Fedora" on their
/// phone while pairing a Debian laptop is being told something false, and the
/// string was only ever there because Fedora was the only target.
pub fn default_device_name() -> String {
    // `/etc/hostname` first because it is the configured name; the `procfs`
    // entry second because `/etc/hostname` is legitimately empty on a machine
    // whose name came from DHCP or from `hostnamectl --transient`, which is
    // how the old code ended up showing a distribution name to peers.
    for source in ["/etc/hostname", "/proc/sys/kernel/hostname"] {
        if let Some(name) = std::fs::read_to_string(source)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return name;
        }
    }
    "AnyFlow Desktop".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_store::IDENTITY_SECRET;

    fn store() -> (tempfile::TempDir, FileSecretStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileSecretStore::new(dir.path());
        store.harden().expect("harden");
        (dir, store)
    }

    #[test]
    fn an_absent_secret_is_none_and_not_an_error() {
        let (_g, s) = store();
        assert_eq!(s.read_secret(IDENTITY_SECRET), Ok(None));
        assert_eq!(s.read_state(), Ok(None));
    }

    #[test]
    fn a_written_secret_reads_back_and_is_owner_only() {
        let (_g, s) = store();
        s.write_secret(IDENTITY_SECRET, b"key bytes")
            .expect("write");
        assert_eq!(
            s.read_secret(IDENTITY_SECRET).expect("read"),
            Some(b"key bytes".to_vec())
        );
        let mode = std::fs::metadata(s.dir().join("identity.key"))
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, FILE_MODE);
    }

    #[test]
    fn a_loose_mode_is_a_hard_error_rather_than_a_warning() {
        let (_g, s) = store();
        s.write_secret(IDENTITY_SECRET, b"key bytes")
            .expect("write");
        std::fs::set_permissions(
            s.dir().join("identity.key"),
            std::fs::Permissions::from_mode(0o644),
        )
        .expect("chmod");

        let err = s
            .read_secret(IDENTITY_SECRET)
            .expect_err("a world-readable key must not be returned");
        assert!(matches!(err, StoreAccessError::NotPrivate { .. }));
        assert!(err
            .to_string()
            .contains("must not be group- or world-accessible"));
    }

    #[test]
    fn an_unreadable_secret_is_permission_denied_and_never_absence() {
        // The defect Wave 0 fixes: a key inside a directory this process
        // cannot traverse used to read back as "no key at all".
        let (_g, s) = store();
        s.write_secret(IDENTITY_SECRET, b"key bytes")
            .expect("write");
        std::fs::set_permissions(s.dir(), std::fs::Permissions::from_mode(0o000)).expect("chmod");

        let result = s.read_secret(IDENTITY_SECRET);

        // Restore before asserting, so a failure does not leave an
        // undeletable temp directory behind.
        std::fs::set_permissions(s.dir(), std::fs::Permissions::from_mode(0o700)).expect("chmod");

        match result {
            Err(StoreAccessError::PermissionDenied { .. }) => {}
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn an_unreadable_state_file_is_permission_denied_and_never_absence() {
        let (_g, s) = store();
        s.write_state(b"{}").expect("write");
        std::fs::set_permissions(s.dir(), std::fs::Permissions::from_mode(0o000)).expect("chmod");

        let result = s.read_state();

        std::fs::set_permissions(s.dir(), std::fs::Permissions::from_mode(0o700)).expect("chmod");

        match result {
            Err(StoreAccessError::PermissionDenied { .. }) => {}
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn harden_makes_a_loose_directory_private() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data");
        std::fs::create_dir(&path).expect("mkdir");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).expect("chmod");

        FileSecretStore::new(&path).harden().expect("harden");

        let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    }

    #[test]
    fn a_secret_name_can_never_escape_the_data_directory() {
        let (_g, s) = store();
        for hostile in ["../escape", "a/b", "a\\b", "..", ""] {
            assert!(
                s.read_secret(hostile).is_err(),
                "{hostile} must not resolve"
            );
        }
    }

    #[test]
    fn a_write_replaces_atomically_and_leaves_no_temp_behind() {
        let (_g, s) = store();
        s.write_state(b"first").expect("write");
        s.write_state(b"second").expect("write");
        assert_eq!(s.read_state().expect("read"), Some(b"second".to_vec()));
        assert!(!s.dir().join("state.tmp").exists());
    }
}
