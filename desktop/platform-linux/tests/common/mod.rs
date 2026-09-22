//! The bus fixture the tray's integration suites share.
//!
//! Two suites raise a message bus and put a fake desktop shell on it:
//! `tray_dbus.rs` (Plasma's watcher) and `tray_gnome.rs` (the GNOME
//! extension's). What differs between them is the *host* — its registration
//! algorithm, what it introspects, which menu calls it makes. What must not
//! differ is the bus underneath, because a suite that silently ran against a
//! bus with service directories would be a suite that could start the
//! developer's real `omnibridge-gui` and pass for the wrong reason.
//!
//! So the bus lives here, once.

// Each suite uses the part of this fixture its own tests need. `stays` is a
// Plasma-side helper and `until` a shared one; neither file has to use both.
#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// A private session bus that can activate nothing.
pub struct TestBus {
    child: Child,
    address: String,
    _dir: tempfile::TempDir,
    service_dir: Option<std::path::PathBuf>,
}

/// Whether this bus gets a service directory, and whether it is there yet.
enum ServiceDir {
    /// No `<servicedir>` at all — the tray suites' bus.
    None,
    /// Configured, but not created. Neither implementation can watch it.
    Missing,
    /// Configured and created, so an inotify implementation watches it.
    Present,
}

impl TestBus {
    /// Starts a `dbus-daemon` with a configuration written here.
    ///
    /// **No `<servicedir>`.** Two consequences, both deliberate:
    ///
    /// * nothing in these tests can reach the developer's session — no real
    ///   Plasma, no real GNOME Shell, and above all no real `omnibridge-gui`,
    ///   which a bus with the normal service directories would happily start;
    /// * nothing is activatable, so a test that expected D-Bus activation to
    ///   rescue it fails rather than quietly succeeding for the wrong reason.
    pub fn start() -> TestBus {
        TestBus::start_inner(ServiceDir::None)
    }

    /// The same bus, plus a `<servicedir>` this fixture owns which **does not
    /// exist** when the bus starts.
    ///
    /// **Only `dbus_activation.rs` may use this.** The tray suites depend on
    /// nothing being activatable — see the note on [`TestBus::start`] — and a
    /// bus with a service directory could, in principle, start something.
    ///
    /// It is still not the developer's session: the directory is a fresh
    /// temporary one and the only file that ever lands in it is written by
    /// [`TestBus::install_service_file`] with `Exec=/bin/true`. The activation
    /// suite never activates anything in any case — `ListActivatableNames`
    /// reports names and starts no process — but the `Exec` is `/bin/true` so
    /// that a future test which did would start nothing that matters.
    ///
    /// # Why the directory is missing at start, and why that is not a trick
    ///
    /// MEASURED on Fedora 44, and it changed how this suite is written.
    ///
    /// `dbus-daemon` — the reference implementation, and what this fixture
    /// spawns — **watches its service directories with inotify**. A file
    /// written into a directory that existed when it started becomes
    /// activatable on its own, with no `ReloadConfig`. `dbus-broker`, which is
    /// what actually runs a Fedora 44 session (`dbus-broker-37-8.fc44`), does
    /// not: that is the behaviour the readiness audit measured and the reason
    /// the self-heal exists.
    ///
    /// A directory that does not exist cannot be watched by either of them, so
    /// with one missing at start-up the two implementations agree — absent,
    /// then present after exactly one `ReloadConfig`. That is what makes this
    /// suite deterministic on both, rather than passing on Fedora and
    /// vacuously succeeding on a Debian runner.
    ///
    /// It is also a real case and not a contrivance: `/usr/share/dbus-1/services`
    /// does not exist on a machine where nothing has ever shipped a D-Bus
    /// service, and the OmniBridge package is then the thing that creates it.
    ///
    /// [`TestBus::start_with_watched_service_dir`] is the other half, for the
    /// one test that pins the difference down.
    pub fn start_with_service_dir() -> TestBus {
        TestBus::start_inner(ServiceDir::Missing)
    }

    /// A `<servicedir>` that **exists** when the bus starts, so an inotify
    /// implementation can watch it.
    ///
    /// Used by exactly one test, which records what this bus implementation
    /// does. See [`TestBus::start_with_service_dir`] for why the distinction
    /// matters.
    pub fn start_with_watched_service_dir() -> TestBus {
        TestBus::start_inner(ServiceDir::Present)
    }

    /// The directory the bus was told to scan. It may not exist yet.
    pub fn service_dir(&self) -> &std::path::Path {
        self.service_dir
            .as_deref()
            .expect("this bus was started without a service directory")
    }

    /// Writes a `.service` file for `name` into the bus's service directory,
    /// creating the directory if this is the first one.
    ///
    /// Returns without telling the bus. That is the point: the file is on
    /// disk and the running bus has not been asked to look.
    pub fn install_service_file(&self, name: &str) {
        let dir = self.service_dir();
        std::fs::create_dir_all(dir).expect("the service directory");
        let path = dir.join(format!("{name}.service"));
        std::fs::write(
            &path,
            format!("[D-BUS Service]\nName={name}\nExec=/bin/true\n"),
        )
        .expect("writing the service file");
    }

    fn start_inner(service: ServiceDir) -> TestBus {
        let dir = tempfile::tempdir().expect("a temporary directory for the bus");
        let config = dir.path().join("bus.conf");
        // `<listen>` uses a short path under /tmp because a Unix socket
        // address is bounded by `sun_path`, and a temporary directory deep
        // under a home directory will exceed it.
        let service_dir = match service {
            ServiceDir::None => None,
            ServiceDir::Missing => Some(dir.path().join("services")),
            ServiceDir::Present => {
                let d = dir.path().join("services");
                std::fs::create_dir(&d).expect("the service directory");
                Some(d)
            }
        };
        let servicedir_element = match &service_dir {
            Some(d) => format!("  <servicedir>{}</servicedir>\n", d.display()),
            None => String::new(),
        };
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
{servicedir_element}  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#
            ),
        )
        .expect("writing the bus configuration");

        let mut child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address")
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon should be installed");
        let stdout = child.stdout.take().expect("the bus prints its address");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("reading the bus address");
        let address = line.trim().to_string();
        assert!(
            address.starts_with("unix:"),
            "unexpected bus address {address:?}"
        );
        TestBus {
            child,
            address,
            _dir: dir,
            service_dir,
        }
    }

    /// A fresh connection to this bus.
    pub async fn connect(&self) -> zbus::Connection {
        zbus::conn::Builder::address(self.address.as_str())
            .expect("the address parses")
            .build()
            .await
            .expect("connecting to the private bus")
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Waits for `check` to hold, or fails.
///
/// Nothing here sleeps for a fixed time and then asserts: that is how a suite
/// becomes flaky on a loaded machine.
pub async fn until<F: FnMut() -> bool>(what: &str, mut check: F) {
    for _ in 0..600 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Holds for the whole window, rather than merely at the end of it.
pub async fn stays<F: FnMut() -> bool>(what: &str, window: Duration, mut check: F) {
    let deadline = std::time::Instant::now() + window;
    while std::time::Instant::now() < deadline {
        assert!(check(), "{what} stopped holding");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
