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
        let dir = tempfile::tempdir().expect("a temporary directory for the bus");
        let config = dir.path().join("bus.conf");
        // `<listen>` uses a short path under /tmp because a Unix socket
        // address is bounded by `sun_path`, and a temporary directory deep
        // under a home directory will exceed it.
        std::fs::write(
            &config,
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
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
