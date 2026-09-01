//! The six identity states, and the one rule that matters.
//!
//! > A new identity may be generated **only** from `IDENTITY_NOT_CREATED`.
//! > Every other outcome is fatal, and leaves `state.json` untouched.
//!
//! Before Wave 0 this was false. `Store::open` asked `Path::exists()` for the
//! state file and the key and, on `false`, generated a new identity —
//! and `Path::exists()` answers `false` for *any* metadata error, `EACCES`
//! included. An identity that was merely unreadable was indistinguishable
//! from one that had never existed, so the next line overwrote the trust
//! store: every pairing, every capability grant, and the fingerprint that
//! every peer had pinned.
//!
//! Each test below injects one fault, asserts that the store refuses to
//! start, and then asserts the far more important thing: that `state.json` is
//! byte-for-byte what it was, and that once the fault is removed the *same*
//! identity comes back.

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use anyflow_core::identity::{IdentityBackend, IdentityState, KeyBacking, SoftwareBacking};
use anyflow_core::secret_store::{SecretStore, StoreAccessError, StoreResult, IDENTITY_SECRET};
use anyflow_core::store::{Store, StoreConfig};
use anyflow_core::Fingerprint;

/// A store with an identity in it, plus a snapshot of what is on disk.
struct Fixture {
    dir: tempfile::TempDir,
    fingerprint: Fingerprint,
    device_id: String,
    state_bytes: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let (fingerprint, device_id) = {
            let store = Store::open(dir.path()).expect("first run");
            (
                store.identity().fingerprint(),
                store.identity().device_id().to_string(),
            )
        };
        let state_bytes = std::fs::read(dir.path().join("state.json")).expect("read state");
        Self {
            dir,
            fingerprint,
            device_id,
            state_bytes,
        }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn state_path(&self) -> std::path::PathBuf {
        self.dir.path().join("state.json")
    }

    fn key_path(&self) -> std::path::PathBuf {
        self.dir.path().join("identity.key")
    }

    /// The assertion that the whole wave exists for.
    fn assert_state_untouched(&self, context: &str) {
        let now = std::fs::read(self.state_path()).expect("state.json must still be readable");
        assert_eq!(
            now, self.state_bytes,
            "{context}: state.json was rewritten. That destroys the trust store"
        );
    }

    /// After the fault is removed, the original identity must come back.
    fn assert_identity_survived(&self, context: &str) {
        let store = Store::open(self.path()).expect("reopen after the fault is removed");
        assert_eq!(
            store.identity().fingerprint(),
            self.fingerprint,
            "{context}: the identity changed"
        );
        assert_eq!(store.identity().device_id(), self.device_id, "{context}");
    }
}

fn refusal(dir: &std::path::Path) -> String {
    Store::open(dir)
        .expect_err("the store must refuse to start")
        .to_string()
}

// ---------------------------------------------------------------------------
// IDENTITY_NOT_CREATED — the only state that may create a key
// ---------------------------------------------------------------------------

#[test]
fn an_empty_directory_is_not_created_and_may_generate() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        Store::probe_identity_at(dir.path()),
        IdentityState::NotCreated
    );
    assert!(Store::probe_identity_at(dir.path()).may_create());

    let store = Store::open(dir.path()).expect("first run must succeed");
    assert_eq!(store.key_backing(), KeyBacking::Software);
}

#[test]
fn a_directory_that_does_not_exist_yet_is_also_a_first_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("a/b/anyflow");
    let store = Store::open(&nested).expect("first run must succeed");
    assert!(nested.join("identity.key").exists());
    assert_eq!(store.key_backing(), KeyBacking::Software);
}

// ---------------------------------------------------------------------------
// IDENTITY_AVAILABLE
// ---------------------------------------------------------------------------

#[test]
fn a_healthy_store_reports_available_and_reloads_the_same_identity() {
    let f = Fixture::new();
    assert_eq!(Store::probe_identity_at(f.path()), IdentityState::Available);
    f.assert_identity_survived("healthy store");
}

// ---------------------------------------------------------------------------
// IDENTITY_LOST — the case the old code got wrong
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_key_refuses_to_start_and_never_regenerates() {
    // The headline defect. `chmod 000` on the directory makes the key
    // unreadable *and* makes `Path::exists()` answer `false` — which is
    // exactly how the pre-Wave-0 code decided this was a first run.
    let f = Fixture::new();
    let key = f.key_path();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o000)).expect("chmod");

    let err = refusal(f.path());
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).expect("restore");

    assert!(err.contains("IDENTITY_LOST"), "{err}");
    assert!(err.contains("NOT been replaced"), "{err}");
    f.assert_state_untouched("unreadable key");
    f.assert_identity_survived("unreadable key");
}

#[test]
fn a_non_traversable_data_directory_refuses_to_start_and_never_regenerates() {
    let f = Fixture::new();
    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o000)).expect("chmod");

    let err = Store::open(f.path()).expect_err("must refuse").to_string();

    std::fs::set_permissions(f.path(), std::fs::Permissions::from_mode(0o700)).expect("restore");

    assert!(
        err.contains("IDENTITY_LOST") || err.contains("cannot be read"),
        "{err}"
    );
    f.assert_state_untouched("non-traversable directory");
    f.assert_identity_survived("non-traversable directory");
}

#[test]
fn a_missing_key_beside_an_existing_state_is_fatal() {
    let f = Fixture::new();
    std::fs::remove_file(f.key_path()).expect("remove key");

    let err = refusal(f.path());
    assert!(err.contains("IDENTITY_LOST"), "{err}");
    assert!(
        err.contains("re-pairing"),
        "the remedy must be named: {err}"
    );
    f.assert_state_untouched("missing key");
}

#[test]
fn a_key_with_no_state_beside_it_is_fatal_rather_than_a_first_run() {
    // The half-written case: key material exists, the document describing it
    // does not. Treating this as a first run would overwrite a key that may
    // already have been used to pair.
    let f = Fixture::new();
    std::fs::remove_file(f.state_path()).expect("remove state");
    let key_before = std::fs::read(f.key_path()).expect("read key");

    let err = refusal(f.path());
    assert!(err.contains("IDENTITY_LOST"), "{err}");
    assert_eq!(
        std::fs::read(f.key_path()).expect("read key"),
        key_before,
        "the key must not be overwritten"
    );
}

// ---------------------------------------------------------------------------
// IDENTITY_CORRUPTED
// ---------------------------------------------------------------------------

#[test]
fn a_malformed_state_file_is_corrupted_and_never_overwritten() {
    let f = Fixture::new();
    std::fs::write(f.state_path(), b"{ this is not json").expect("write");

    let err = refusal(f.path());
    assert!(err.contains("IDENTITY_CORRUPTED"), "{err}");
    assert_eq!(
        std::fs::read(f.state_path()).expect("read"),
        b"{ this is not json",
        "a malformed state file must be left alone for the operator to fix"
    );
}

#[test]
fn an_unparseable_key_is_corrupted_and_never_replaced() {
    let f = Fixture::new();
    std::fs::write(f.key_path(), b"not a pkcs8 key at all").expect("write");
    std::fs::set_permissions(f.key_path(), std::fs::Permissions::from_mode(0o600)).expect("chmod");

    let err = refusal(f.path());
    assert!(err.contains("not a usable P-256 private key"), "{err}");
    f.assert_state_untouched("unparseable key");
    assert_eq!(
        std::fs::read(f.key_path()).expect("read"),
        b"not a pkcs8 key at all",
        "the key must not be replaced"
    );
}

#[test]
fn a_key_that_does_not_match_the_certificate_is_corrupted() {
    // Certificate/public-key continuity. A valid key from a *different*
    // identity must not load: it would fail every handshake with a signature
    // error that named neither, and the fingerprint peers pinned would no
    // longer be the key in use.
    let f = Fixture::new();

    let other = tempfile::tempdir().expect("tempdir");
    {
        let _ = Store::open(other.path()).expect("second identity");
    }
    let other_key = std::fs::read(other.path().join("identity.key")).expect("read");
    std::fs::write(f.key_path(), &other_key).expect("write");
    std::fs::set_permissions(f.key_path(), std::fs::Permissions::from_mode(0o600)).expect("chmod");

    let err = refusal(f.path());
    assert!(
        err.contains("does not match the stored certificate"),
        "{err}"
    );
    f.assert_state_untouched("mismatched key");
}

#[test]
fn a_world_readable_key_is_refused_with_the_remedy_and_nothing_is_rewritten() {
    let f = Fixture::new();
    std::fs::set_permissions(f.key_path(), std::fs::Permissions::from_mode(0o644)).expect("chmod");

    let err = refusal(f.path());
    assert!(
        err.contains("must not be group- or world-accessible"),
        "{err}"
    );
    assert!(err.contains("chmod 600"), "the remedy must be named: {err}");
    f.assert_state_untouched("world-readable key");

    std::fs::set_permissions(f.key_path(), std::fs::Permissions::from_mode(0o600))
        .expect("restore");
    f.assert_identity_survived("world-readable key");
}

// ---------------------------------------------------------------------------
// IDENTITY_HARDWARE_UNAVAILABLE
// ---------------------------------------------------------------------------

#[test]
fn a_recorded_hardware_backing_this_build_cannot_open_is_fatal() {
    // PLAT-DEC-015. The distinction that only a *recorded expectation* can
    // make: "this device never had hardware backing" versus "the hardware
    // backing has gone away". Falling back to software here would be a
    // security downgrade the user cannot see; regenerating would break every
    // pairing.
    let f = Fixture::new();
    let raw = std::fs::read_to_string(f.state_path()).expect("read");
    let rewritten = raw.replace("\"key_backing\": \"software\"", "\"key_backing\": \"tpm\"");
    assert_ne!(raw, rewritten, "key_backing must be present in state.json");
    std::fs::write(f.state_path(), &rewritten).expect("write");

    let err = refusal(f.path());
    assert!(err.contains("IDENTITY_HARDWARE_UNAVAILABLE"), "{err}");
    assert!(
        err.contains("never done"),
        "the policy must be named: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(f.state_path()).expect("read"),
        rewritten,
        "nothing may be rewritten"
    );
}

#[test]
fn key_backing_round_trips_through_state_json() {
    let f = Fixture::new();
    let raw = std::fs::read_to_string(f.state_path()).expect("read");
    assert!(raw.contains("\"key_backing\": \"software\""), "{raw}");

    let store = Store::open(f.path()).expect("reopen");
    assert_eq!(store.key_backing(), KeyBacking::Software);
}

#[test]
fn a_schema_1_state_file_loads_and_reads_back_as_software() {
    // CC-3: a pre-Wave-0 install upgrades in place. Same identity, same
    // device id, same fingerprint, same peers, no re-pairing.
    let f = Fixture::new();
    let raw = std::fs::read_to_string(f.state_path()).expect("read");

    // Reconstruct exactly what schema 1 looked like: version 1, no
    // `key_backing` field at all.
    let value: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let mut object = value.as_object().expect("object").clone();
    object.remove("key_backing");
    object.insert("schema_version".into(), serde_json::json!(1));
    std::fs::write(
        f.state_path(),
        serde_json::to_vec_pretty(&object).expect("encode"),
    )
    .expect("write");

    let store = Store::open(f.path()).expect("a schema 1 file must still load");
    assert_eq!(store.identity().fingerprint(), f.fingerprint);
    assert_eq!(store.identity().device_id(), f.device_id);
    assert_eq!(
        store.key_backing(),
        KeyBacking::Software,
        "an identity written before key_backing existed is a software key"
    );
}

// ---------------------------------------------------------------------------
// IDENTITY_TEMPORARILY_UNAVAILABLE
// ---------------------------------------------------------------------------

/// A secret store that fails whichever read it is told to.
///
/// Drives the states that a filesystem will not produce on demand — and, more
/// usefully, proves the classification is a property of the *seam* rather
/// than of one implementation.
#[derive(Debug)]
struct FaultyStore {
    inner: anyflow_core::platform::unix_fs::FileSecretStore,
    fault: Option<StoreAccessError>,
    on_state: bool,
    writes: std::sync::Mutex<Vec<&'static str>>,
}

impl FaultyStore {
    fn new(dir: &std::path::Path, fault: Option<StoreAccessError>, on_state: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: anyflow_core::platform::unix_fs::FileSecretStore::new(dir),
            fault,
            on_state,
            writes: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn writes(&self) -> Vec<&'static str> {
        self.writes.lock().expect("lock").clone()
    }
}

impl SecretStore for FaultyStore {
    fn read_secret(&self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        if !self.on_state {
            if let Some(f) = &self.fault {
                return Err(f.clone());
            }
        }
        self.inner.read_secret(name)
    }
    fn write_secret(&self, name: &str, data: &[u8]) -> StoreResult<()> {
        self.writes.lock().expect("lock").push("secret");
        self.inner.write_secret(name, data)
    }
    fn read_state(&self) -> StoreResult<Option<Vec<u8>>> {
        if self.on_state {
            if let Some(f) = &self.fault {
                return Err(f.clone());
            }
        }
        self.inner.read_state()
    }
    fn write_state(&self, data: &[u8]) -> StoreResult<()> {
        self.writes.lock().expect("lock").push("state");
        self.inner.write_state(data)
    }
    fn harden(&self) -> StoreResult<()> {
        self.inner.harden()
    }
    fn verify_protection(&self, name: &str) -> StoreResult<()> {
        self.inner.verify_protection(name)
    }
    fn describe(&self) -> String {
        format!("faulty({})", self.inner.describe())
    }
}

fn config(secrets: Arc<FaultyStore>) -> StoreConfig {
    StoreConfig {
        secrets,
        backend: Arc::new(SoftwareBacking),
        platform: anyflow_proto::v1::Platform::Linux,
        default_device_name: "Test Device".into(),
    }
}

#[test]
fn a_transient_io_failure_is_temporarily_unavailable_and_writes_nothing() {
    let f = Fixture::new();

    for on_state in [true, false] {
        let secrets = FaultyStore::new(
            f.path(),
            Some(StoreAccessError::Io {
                item: "identity".into(),
                detail: "the network mount went away".into(),
            }),
            on_state,
        );
        let probe = Store::probe_identity(secrets.as_ref(), &SoftwareBacking);
        assert!(
            matches!(probe, IdentityState::TemporarilyUnavailable(_)),
            "on_state={on_state}: got {probe:?}"
        );
        assert!(probe.is_retryable());
        assert!(!probe.may_create());

        let err = Store::open_with(config(Arc::clone(&secrets)))
            .expect_err("must refuse")
            .to_string();
        assert!(err.contains("IDENTITY_TEMPORARILY_UNAVAILABLE"), "{err}");
        assert!(err.contains("retry"), "{err}");
        assert!(
            secrets.writes().is_empty(),
            "nothing may be written while the store is unreadable"
        );
    }

    f.assert_state_untouched("transient io failure");
    f.assert_identity_survived("transient io failure");
}

#[test]
fn a_permission_failure_is_never_reported_as_absence() {
    let f = Fixture::new();
    let secrets = FaultyStore::new(
        f.path(),
        Some(StoreAccessError::PermissionDenied {
            item: "identity".into(),
            detail: "EACCES".into(),
        }),
        false,
    );

    let probe = Store::probe_identity(secrets.as_ref(), &SoftwareBacking);
    assert!(matches!(probe, IdentityState::Lost(_)), "{probe:?}");
    assert!(!probe.may_create(), "a permission error must never create");

    assert!(Store::open_with(config(Arc::clone(&secrets))).is_err());
    assert!(secrets.writes().is_empty());
    f.assert_state_untouched("permission failure");
}

// ---------------------------------------------------------------------------
// The rule itself
// ---------------------------------------------------------------------------

#[test]
fn every_fault_leaves_the_identity_intact_and_only_absence_creates() {
    // The whole property in one place: across every injectable fault, the
    // number of times a new identity was created is zero.
    let f = Fixture::new();

    /// One injectable fault: a name, and something that breaks the store.
    type Fault = (&'static str, Box<dyn Fn()>);

    let faults: Vec<Fault> = vec![
        (
            "malformed state",
            Box::new({
                let p = f.state_path();
                move || std::fs::write(&p, b"not json").expect("write")
            }),
        ),
        (
            "truncated state",
            Box::new({
                let p = f.state_path();
                move || std::fs::write(&p, b"").expect("write")
            }),
        ),
        (
            "empty key",
            Box::new({
                let p = f.key_path();
                move || std::fs::write(&p, b"").expect("write")
            }),
        ),
    ];

    for (name, apply) in faults {
        let state_before = std::fs::read(f.state_path()).expect("read");
        let key_before = std::fs::read(f.key_path()).expect("read");
        apply();

        assert!(
            Store::open(f.path()).is_err(),
            "{name}: the store must refuse"
        );
        assert!(
            !Store::probe_identity_at(f.path()).may_create(),
            "{name}: this state must never be allowed to create an identity"
        );

        // Whatever the fault did, AnyFlow did not add to it.
        std::fs::write(f.state_path(), &state_before).expect("restore state");
        std::fs::write(f.key_path(), &key_before).expect("restore key");
        std::fs::set_permissions(f.key_path(), std::fs::Permissions::from_mode(0o600))
            .expect("chmod");
        f.assert_identity_survived(name);
    }
}

#[test]
fn the_state_codes_are_the_ones_the_specification_names() {
    // The vocabulary is part of the contract: these strings appear in logs
    // and in the operator-facing refusal, and a rename would silently break
    // anything reading them.
    assert_eq!(IdentityState::NotCreated.code(), "IDENTITY_NOT_CREATED");
    assert_eq!(IdentityState::Available.code(), "IDENTITY_AVAILABLE");
    assert_eq!(
        IdentityState::TemporarilyUnavailable(String::new()).code(),
        "IDENTITY_TEMPORARILY_UNAVAILABLE"
    );
    assert_eq!(
        IdentityState::HardwareUnavailable(String::new()).code(),
        "IDENTITY_HARDWARE_UNAVAILABLE"
    );
    assert_eq!(
        IdentityState::Corrupted(String::new()).code(),
        "IDENTITY_CORRUPTED"
    );
    assert_eq!(IdentityState::Lost(String::new()).code(), "IDENTITY_LOST");
}

#[test]
fn only_not_created_may_create() {
    for state in [
        IdentityState::Available,
        IdentityState::TemporarilyUnavailable("x".into()),
        IdentityState::HardwareUnavailable("x".into()),
        IdentityState::Corrupted("x".into()),
        IdentityState::Lost("x".into()),
    ] {
        assert!(!state.may_create(), "{} must not create", state.code());
    }
    assert!(IdentityState::NotCreated.may_create());
}

#[test]
fn the_software_backend_stores_its_material_under_the_documented_name() {
    // CC-4: `identity.key` stays PKCS#8 at 0600. Wave 0 changed how the key
    // is *reached*, not what is on disk.
    assert_eq!(SoftwareBacking.secret_name(), IDENTITY_SECRET);
    assert_eq!(SoftwareBacking.backing(), KeyBacking::Software);

    let f = Fixture::new();
    let mode = std::fs::metadata(f.key_path())
        .expect("stat")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
