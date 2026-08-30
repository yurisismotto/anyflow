//! Where a received file goes, and how it gets there safely.
//!
//! # The directory
//!
//! One dedicated directory, chosen by *this* machine, never influenced by the
//! peer. It follows the XDG user directories so that a received file lands
//! where the desktop already puts downloads:
//!
//! 1. `$XDG_DOWNLOAD_DIR`, if set to an absolute path;
//! 2. `XDG_DOWNLOAD_DIR` from `${XDG_CONFIG_HOME:-$HOME/.config}/user-dirs.dirs`,
//!    which is where `xdg-user-dirs` records a localised Downloads folder —
//!    so a Brazilian desktop gets `~/Downloads` or `~/Transferências`,
//!    whichever it actually uses;
//! 3. `$HOME/Downloads`.
//!
//! …then `AnyFlow/` underneath. `$HOME` is read from the environment and no
//! path is hardcoded: there is no `/home/<user>` anywhere in this file.
//!
//! # Writing
//!
//! Received bytes go to a hidden temp file **inside the destination
//! directory**, not to `/tmp`. That is not tidiness: `rename(2)` is only
//! atomic within one filesystem, and `/tmp` is routinely a different one. A
//! temp file next to its destination means the promotion at the end is a
//! genuine atomic rename rather than a copy that can be interrupted halfway.
//!
//! Every file is created with `O_EXCL` and mode 0600. `O_EXCL` is what makes
//! this safe against a symlink planted in the download directory: the open
//! fails outright rather than following it, so a local attacker cannot
//! redirect a received file onto something else.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::limits::MAX_DUPLICATE_SUFFIX;
use crate::transfer::TransferId;

/// The directory received files are stored in.
#[derive(Debug, Clone)]
pub struct Destination {
    dir: PathBuf,
}

impl Destination {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The XDG-derived default: `<downloads>/AnyFlow`.
    pub fn default_location() -> Self {
        Self::new(default_download_dir().join("AnyFlow"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Creates the directory if it does not exist.
    ///
    /// Mode 0700: a received file may be anything, and there is no reason for
    /// other local users to enumerate what this machine has been sent.
    pub fn prepare(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&self.dir)?;
        let mut perms = meta.permissions();
        if perms.mode() & 0o077 != 0 {
            perms.set_mode(0o700);
            std::fs::set_permissions(&self.dir, perms)?;
        }
        Ok(())
    }

    /// Opens the temp file a transfer streams into.
    ///
    /// Named after the transfer, hidden, and suffixed `.part` so that a file
    /// left behind by a crash is recognisable and obviously incomplete. It
    /// lives beside its destination so the final rename is atomic.
    pub fn open_temp(&self, id: TransferId) -> io::Result<(std::fs::File, PathBuf)> {
        self.prepare()?;
        let path = self.dir.join(format!(".anyflow-{}.part", id.to_hex()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        Ok((file, path))
    }

    /// Reserves a final name for a verified file, without ever overwriting an
    /// existing one.
    ///
    /// `photo.jpg` is taken, so `photo (1).jpg` is tried, then `photo (2).jpg`
    /// and so on. This is the convention every desktop browser uses, so it
    /// needs no explaining to a user.
    ///
    /// The name is reserved by *creating* it with `O_EXCL` rather than by
    /// testing for existence and then creating it. The test-then-create form
    /// has a window in which two concurrent transfers both see the name free
    /// and the second silently destroys the first — precisely the "never
    /// overwrite" property this function exists to provide.
    pub fn reserve(&self, name: &str) -> io::Result<PathBuf> {
        self.prepare()?;

        let (stem, extension) = split_extension(name);
        for attempt in 0..=MAX_DUPLICATE_SUFFIX {
            let candidate = if attempt == 0 {
                name.to_string()
            } else {
                format!("{stem} ({attempt}){extension}")
            };

            let path = self.dir.join(&candidate);

            // Re-check that the joined path really is a direct child of the
            // destination. After `filename::sanitize` it cannot be anything
            // else, so this is defence in depth — the threat model asks for
            // the final path to be re-checked, and a cheap assertion here
            // means a future change to the sanitizer cannot quietly become a
            // traversal.
            if path.parent() != Some(self.dir.as_path()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "refusing a destination outside the download directory",
                ));
            }

            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(_) => return Ok(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "too many files with this name already",
        ))
    }

    /// Promotes a verified temp file to its final name.
    ///
    /// The rename replaces the placeholder [`reserve`] created, and is atomic:
    /// at no instant does a file with the final name exist containing partial
    /// or unverified data.
    ///
    /// [`reserve`]: Self::reserve
    pub fn promote(&self, temp: &Path, name: &str) -> io::Result<PathBuf> {
        let final_path = self.reserve(name)?;
        // Make the file readable by its owner in the normal way now that it
        // is a real, complete file; it was 0600 while it was a fragment.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(temp, std::fs::Permissions::from_mode(0o600));
        match std::fs::rename(temp, &final_path) {
            Ok(()) => Ok(final_path),
            Err(e) => {
                // The placeholder must not be left behind as an empty file
                // pretending to be a received one.
                let _ = std::fs::remove_file(&final_path);
                Err(e)
            }
        }
    }
}

/// Splits `photo.tar.gz` into `("photo.tar", ".gz")`, and `README` into
/// `("README", "")`. A leading dot is part of the stem, so `.bashrc`
/// duplicates as `.bashrc (1)` rather than ` (1).bashrc`.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

/// Resolves the user's Downloads directory without hardcoding a path.
pub fn default_download_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_DOWNLOAD_DIR") {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return path;
        }
    }

    if let Some(dir) = download_dir_from_user_dirs() {
        return dir;
    }

    home_dir().join("Downloads")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        // A session with no HOME is broken, but falling back to the working
        // directory keeps the daemon running and writing somewhere the user
        // owns, rather than panicking or writing to `/`.
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Parses `XDG_DOWNLOAD_DIR` out of `user-dirs.dirs`.
///
/// The file is shell-syntax-ish, written by `xdg-user-dirs-update`:
///
/// ```text
/// XDG_DOWNLOAD_DIR="$HOME/Downloads"
/// ```
///
/// Only that one form is handled — `$HOME/…` or an absolute path — because
/// that is the only form the tool writes. Anything else falls through to the
/// default rather than being interpreted, since running a shell to expand a
/// config file would be a much larger thing to trust.
fn download_dir_from_user_dirs() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"));

    let raw = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;

    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some(value) = line.strip_prefix("XDG_DOWNLOAD_DIR=") else {
            continue;
        };
        let value = value.trim().trim_matches('"');

        if let Some(rest) = value.strip_prefix("$HOME/") {
            return Some(home_dir().join(rest));
        }
        let path = PathBuf::from(value);
        if path.is_absolute() {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_destination() -> (tempfile::TempDir, Destination) {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = Destination::new(dir.path().join("AnyFlow"));
        dest.prepare().expect("prepare");
        (dir, dest)
    }

    #[test]
    fn the_directory_is_created_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_guard, dest) = temp_destination();
        let mode = std::fs::metadata(dest.dir())
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o077,
            0,
            "mode {mode:o} is group- or world-accessible"
        );
    }

    #[test]
    fn a_free_name_is_used_as_is() {
        let (_guard, dest) = temp_destination();
        let path = dest.reserve("photo.jpg").expect("reserve");
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("photo.jpg"));
    }

    #[test]
    fn a_duplicate_name_is_numbered_and_never_overwrites() {
        let (_guard, dest) = temp_destination();

        let first = dest.reserve("photo.jpg").expect("first");
        std::fs::write(&first, b"original").expect("write");

        let second = dest.reserve("photo.jpg").expect("second");
        assert_eq!(
            second.file_name().and_then(|s| s.to_str()),
            Some("photo (1).jpg")
        );

        let third = dest.reserve("photo.jpg").expect("third");
        assert_eq!(
            third.file_name().and_then(|s| s.to_str()),
            Some("photo (2).jpg")
        );

        // The original is untouched. This is FILE-10.
        assert_eq!(std::fs::read(&first).expect("read"), b"original");
    }

    #[test]
    fn a_name_with_no_extension_is_numbered_at_the_end() {
        let (_guard, dest) = temp_destination();
        dest.reserve("README").expect("first");
        let second = dest.reserve("README").expect("second");
        assert_eq!(
            second.file_name().and_then(|s| s.to_str()),
            Some("README (1)")
        );
    }

    #[test]
    fn a_dotfile_keeps_its_leading_dot_when_numbered() {
        let (_guard, dest) = temp_destination();
        dest.reserve(".gitignore").expect("first");
        let second = dest.reserve(".gitignore").expect("second");
        assert_eq!(
            second.file_name().and_then(|s| s.to_str()),
            Some(".gitignore (1)")
        );
    }

    #[test]
    fn a_double_extension_keeps_only_the_last_part() {
        let (_guard, dest) = temp_destination();
        dest.reserve("archive.tar.gz").expect("first");
        let second = dest.reserve("archive.tar.gz").expect("second");
        assert_eq!(
            second.file_name().and_then(|s| s.to_str()),
            Some("archive.tar (1).gz")
        );
    }

    #[test]
    fn reserving_creates_the_file_so_a_racing_transfer_cannot_take_it() {
        // The reservation must be a create, not a lookup: two transfers of
        // the same name must never be handed the same path.
        let (_guard, dest) = temp_destination();
        let a = dest.reserve("x.bin").expect("a");
        let b = dest.reserve("x.bin").expect("b");
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());
    }

    #[test]
    fn a_symlink_planted_in_the_directory_is_not_followed() {
        // The local-attacker case: something has pre-created `photo.jpg` as a
        // symlink to a file that must not be overwritten.
        let (_guard, dest) = temp_destination();
        let victim = dest.dir().join("victim.txt");
        std::fs::write(&victim, b"precious").expect("write");
        std::os::unix::fs::symlink(&victim, dest.dir().join("photo.jpg")).expect("symlink");

        // O_EXCL refuses to open through the symlink, so the name is taken
        // and the numbered alternative is used instead.
        let path = dest.reserve("photo.jpg").expect("reserve");
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some("photo (1).jpg")
        );
        assert_eq!(std::fs::read(&victim).expect("read"), b"precious");
    }

    #[test]
    fn promotion_is_a_rename_that_leaves_no_partial_file_behind() {
        let (_guard, dest) = temp_destination();
        let id = TransferId::from_bytes(&[7u8; 16]).expect("id");
        let (file, temp) = dest.open_temp(id).expect("temp");
        drop(file);
        std::fs::write(&temp, b"verified bytes").expect("write");

        let final_path = dest.promote(&temp, "result.bin").expect("promote");
        assert_eq!(std::fs::read(&final_path).expect("read"), b"verified bytes");
        assert!(!temp.exists(), "the temp file must be gone");
    }

    #[test]
    fn a_temp_file_is_hidden_marked_partial_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_guard, dest) = temp_destination();
        let id = TransferId::from_bytes(&[9u8; 16]).expect("id");
        let (_file, temp) = dest.open_temp(id).expect("temp");

        let name = temp.file_name().and_then(|s| s.to_str()).expect("name");
        assert!(name.starts_with(".anyflow-"), "{name}");
        assert!(name.ends_with(".part"), "{name}");
        assert_eq!(temp.parent(), Some(dest.dir()));

        let mode = std::fs::metadata(&temp).expect("stat").permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }

    #[test]
    fn the_temp_file_sits_beside_its_destination_so_the_rename_is_atomic() {
        let (_guard, dest) = temp_destination();
        let id = TransferId::from_bytes(&[1u8; 16]).expect("id");
        let (_file, temp) = dest.open_temp(id).expect("temp");
        // Same directory, therefore necessarily the same filesystem.
        assert_eq!(temp.parent(), Some(dest.dir()));
    }

    #[test]
    fn the_default_location_is_derived_from_the_environment() {
        // The property is that the path comes from `$HOME` (or an XDG
        // setting), not that it avoids any particular string — on a machine
        // whose home really is `/home/<user>`, the correct answer contains
        // exactly that.
        //
        // `HOME` is only read, never set: mutating the environment would race
        // every other test in this binary.
        let dir = default_download_dir();
        assert!(dir.is_absolute(), "{dir:?}");

        // With no XDG override, the answer must sit under this session's own
        // home directory, wherever that is.
        if let (None, Some(home)) = (
            std::env::var_os("XDG_DOWNLOAD_DIR"),
            std::env::var_os("HOME"),
        ) {
            assert!(
                dir.starts_with(PathBuf::from(home)),
                "{dir:?} is not under $HOME"
            );
        }
    }

    #[test]
    fn the_anyflow_subdirectory_is_used() {
        let dest = Destination::default_location();
        assert_eq!(
            dest.dir().file_name().and_then(|s| s.to_str()),
            Some("AnyFlow")
        );
    }
}
