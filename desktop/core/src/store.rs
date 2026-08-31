//! Persistent state: local identity, trusted peers, per-capability grants.
//!
//! # What is stored
//!
//! Only what the spec allows: identity, paired devices, their pinned public
//! keys, capability permissions, settings. No clipboard content, no transfer
//! history, no message log.
//!
//! # Format
//!
//! JSON with an explicit top-level `schema_version`. JSON because the state
//! is tiny (tens of records), human-auditable — which matters a lot for a
//! file that decides who is trusted — and trivially migratable. The version
//! field exists from commit one precisely so migrations are possible later.
//!
//! # On-disk protection
//!
//! The private key lives in `identity.key` with mode 0600 inside a 0700
//! directory under `$XDG_DATA_HOME`. On load we *verify* those modes and
//! refuse to start if they are loose, rather than silently continuing with a
//! world-readable key.
//!
//! We deliberately do not use the Secret Service (gnome-keyring) API here: a
//! `systemd --user` daemon can start before any keyring is unlocked, and a
//! daemon that blocks on a locked keyring at boot is worse than useless.
//! See ADR-0006 for the full trade-off and the TPM2 follow-up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::clipboard_policy::ClipboardPolicy;
use crate::error::{Error, Result};
use crate::fingerprint::Fingerprint;
use crate::identity::LocalIdentity;

pub const SCHEMA_VERSION: u32 = 1;

/// A device this one has paired with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub device_id: String,
    pub device_name: String,
    pub platform: i32,
    /// The pinned SPKI fingerprint. This, and only this, decides identity.
    pub fingerprint: Fingerprint,
    pub paired_at_unix: i64,
    /// Capabilities this peer is allowed to use, independent of what it
    /// advertises. An empty map means "nothing granted yet".
    #[serde(default)]
    pub granted_capabilities: BTreeMap<String, bool>,
    /// Highest protocol version ever negotiated with this peer. Recorded so a
    /// future release can detect and refuse a silent downgrade.
    #[serde(default)]
    pub last_protocol_version: u32,
    /// Set instead of deleting the record, so a revoked device stays
    /// recognisable and cannot silently re-pair without the user noticing.
    #[serde(default)]
    pub revoked: bool,
    /// Per-peer `clipboard.v1` direction and automation settings.
    ///
    /// Stored next to the grant but deliberately separate from it: the grant
    /// says whether this device may speak clipboard at all, and this says in
    /// which directions and how automatically. Both are decided locally — no
    /// protocol message writes either — and neither holds clipboard content.
    ///
    /// `#[serde(default)]` matters here: a trust store written before this
    /// capability existed has no such object, and it must deserialize to the
    /// safe defaults (automatic directions off) rather than failing the load
    /// or, worse, defaulting to `false` across the board and silently
    /// disabling a direction the user had enabled.
    #[serde(default)]
    pub clipboard_policy: ClipboardPolicy,
}

impl TrustedPeer {
    pub fn allows(&self, capability_id: &str) -> bool {
        !self.revoked
            && *self
                .granted_capabilities
                .get(capability_id)
                .unwrap_or(&false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub device_name: String,
    pub listen_port: u16,
    /// Automatically grant a capability when a newly paired peer advertises
    /// it. `battery.v1` is read-only telemetry with no side effects, so it is
    /// on by default; anything with side effects must not be.
    pub auto_grant: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            listen_port: crate::DEFAULT_PORT,
            auto_grant: vec!["battery.v1".to_string()],
        }
    }
}

fn default_device_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Fedora".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct StateFile {
    schema_version: u32,
    device_id: String,
    /// DER certificate, base64. The matching private key is in a separate,
    /// stricter file.
    certificate_der_b64: String,
    settings: Settings,
    peers: Vec<TrustedPeer>,
}

/// Owns the on-disk state and the in-memory view of it.
pub struct Store {
    dir: PathBuf,
    identity: LocalIdentity,
    settings: Settings,
    peers: BTreeMap<Fingerprint, TrustedPeer>,
}

impl Store {
    /// Opens the store, generating a fresh identity on first run.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(Error::Io)?;
        harden_dir(&dir)?;

        let state_path = dir.join("state.json");
        let key_path = dir.join("identity.key");

        if state_path.exists() && key_path.exists() {
            Self::load(dir, &state_path, &key_path)
        } else {
            Self::initialize(dir, &state_path, &key_path)
        }
    }

    fn initialize(dir: PathBuf, state_path: &Path, key_path: &Path) -> Result<Self> {
        let settings = Settings::default();
        let identity =
            LocalIdentity::generate(&settings.device_name, anyflow_proto::v1::Platform::Linux)?;

        let store = Self {
            dir,
            identity,
            settings,
            peers: BTreeMap::new(),
        };
        store.write_key(key_path)?;
        store.write_state(state_path)?;
        Ok(store)
    }

    fn load(dir: PathBuf, state_path: &Path, key_path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(state_path).map_err(Error::Io)?;
        let state: StateFile =
            serde_json::from_str(&raw).map_err(|e| Error::Store(format!("state.json: {e}")))?;

        if state.schema_version > SCHEMA_VERSION {
            return Err(Error::Store(format!(
                "state.json schema v{} is newer than supported v{}; refusing to \
                 downgrade and risk losing trust records",
                state.schema_version, SCHEMA_VERSION
            )));
        }

        require_private_mode(key_path)?;
        let key_der = std::fs::read(key_path).map_err(Error::Io)?;
        let cert_der = data_encoding::BASE64
            .decode(state.certificate_der_b64.as_bytes())
            .map_err(|_| Error::Store("certificate is not valid base64".into()))?;

        let identity = LocalIdentity::from_parts(
            state.device_id,
            state.settings.device_name.clone(),
            anyflow_proto::v1::Platform::Linux,
            cert_der,
            key_der,
        )?;

        let peers = state
            .peers
            .into_iter()
            .map(|p| (p.fingerprint, p))
            .collect();

        Ok(Self {
            dir,
            identity,
            settings: state.settings,
            peers,
        })
    }

    fn write_key(&self, path: &Path) -> Result<()> {
        write_atomic(path, self.identity.private_key_pkcs8_der(), 0o600)
    }

    fn write_state(&self, path: &Path) -> Result<()> {
        let state = StateFile {
            schema_version: SCHEMA_VERSION,
            device_id: self.identity.device_id().to_string(),
            certificate_der_b64: data_encoding::BASE64.encode(self.identity.certificate_der()),
            settings: self.settings.clone(),
            peers: self.peers.values().cloned().collect(),
        };
        let json = serde_json::to_vec_pretty(&state)
            .map_err(|e| Error::Store(format!("serializing state: {e}")))?;
        write_atomic(path, &json, 0o600)
    }

    pub fn persist(&self) -> Result<()> {
        self.write_state(&self.dir.join("state.json"))
    }

    pub fn identity(&self) -> &LocalIdentity {
        &self.identity
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn peers(&self) -> impl Iterator<Item = &TrustedPeer> {
        self.peers.values()
    }

    /// Looks up a peer by pinned fingerprint. Revoked peers are *not*
    /// returned: to every caller they are simply not trusted.
    pub fn trusted_peer(&self, fp: &Fingerprint) -> Option<&TrustedPeer> {
        self.peers.get(fp).filter(|p| !p.revoked)
    }

    /// Looks up a peer including revoked ones (for `anyflow devices` output).
    pub fn peer_record(&self, fp: &Fingerprint) -> Option<&TrustedPeer> {
        self.peers.get(fp)
    }

    pub fn find_by_device_id(&self, device_id: &str) -> Option<&TrustedPeer> {
        self.peers.values().find(|p| p.device_id == device_id)
    }

    /// Adds or refreshes a trusted peer. Re-pairing an existing fingerprint
    /// clears a previous revocation, which is the intended way back in.
    pub fn add_peer(&mut self, peer: TrustedPeer) -> Result<()> {
        self.peers.insert(peer.fingerprint, peer);
        self.persist()
    }

    /// Revokes a pairing. The record is kept (with `revoked = true`) rather
    /// than deleted, so the device remains listed and a later reconnection
    /// attempt is attributable instead of appearing as a stranger.
    pub fn revoke_peer(&mut self, fp: &Fingerprint) -> Result<bool> {
        match self.peers.get_mut(fp) {
            Some(p) => {
                p.revoked = true;
                p.granted_capabilities.clear();
                self.persist()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn set_capability_grant(
        &mut self,
        fp: &Fingerprint,
        capability_id: &str,
        granted: bool,
    ) -> Result<()> {
        if let Some(p) = self.peers.get_mut(fp) {
            p.granted_capabilities
                .insert(capability_id.to_string(), granted);
        }
        self.persist()
    }

    /// Replaces one peer's clipboard policy.
    ///
    /// Persisted immediately, because the two questions a user asks after
    /// changing it — "did that take?" and "will it survive a restart?" —
    /// should have the same answer.
    pub fn set_clipboard_policy(
        &mut self,
        fp: &Fingerprint,
        policy: ClipboardPolicy,
    ) -> Result<()> {
        if let Some(p) = self.peers.get_mut(fp) {
            p.clipboard_policy = policy;
        }
        self.persist()
    }

    pub fn record_protocol_version(&mut self, fp: &Fingerprint, version: u32) -> Result<()> {
        if let Some(p) = self.peers.get_mut(fp) {
            if version > p.last_protocol_version {
                p.last_protocol_version = version;
            }
        }
        self.persist()
    }
}

/// Default location: `$XDG_DATA_HOME/anyflow`, else `~/.local/share/...`.
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

/// Writes via a temp file + rename so a crash mid-write cannot leave a
/// truncated trust store. The temp file is created with the final mode, so
/// there is no window in which the key is world-readable.
fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(&tmp)
        .map_err(Error::Io)?;
    f.write_all(data).map_err(Error::Io)?;
    f.sync_all().map_err(Error::Io)?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(Error::Io)?;
    Ok(())
}

fn harden_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(dir).map_err(Error::Io)?;
    let mut perms = meta.permissions();
    if perms.mode() & 0o077 != 0 {
        perms.set_mode(0o700);
        std::fs::set_permissions(dir, perms).map_err(Error::Io)?;
    }
    Ok(())
}

/// Refuses to load a private key that is readable by anyone else.
///
/// This is a hard error, not a warning. Continuing would mean running with a
/// compromised-by-construction identity.
fn require_private_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(Error::Io)?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(Error::Store(format!(
            "{} has mode {:o}; the private key must not be group- or \
             world-accessible. Fix with: chmod 600 {}",
            path.display(),
            mode,
            path.display()
        )));
    }
    Ok(())
}
