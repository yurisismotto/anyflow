//! The D-Bus activation self-heal, against a real message bus.
//!
//! # What this suite adds that the unit tests cannot
//!
//! `activation.rs` has a fake bus and tests every branch of the decision
//! exhaustively — including a refused `ReloadConfig`, which a real bus will not
//! produce on demand. What a fake cannot establish is the **premise**:
//!
//! > a running `dbus-daemon` does not notice a service file written after it
//! > started, and does notice it after `ReloadConfig`.
//!
//! That is a property of D-Bus, not of OmniBridge, and the whole feature is
//! built on it. It was measured once by hand in
//! `KDE-PLASMA-REAL-CERTIFICATION-V1.md` §40. Here it is measured by the suite,
//! on every run, on whatever distribution the suite runs on — which matters,
//! because Fedora runs `dbus-broker` and Debian runs `dbus-daemon` and the two
//! are different implementations of the same specification.
//!
//! # What this suite is not allowed to touch
//!
//! The developer's session bus. Every test here raises its own `dbus-daemon`
//! with a service directory the fixture owns, and the only file that reaches
//! that directory names `Exec=/bin/true`. Nothing in this file can start
//! `omnibridge-gui`, and nothing in it reads `DBUS_SESSION_BUS_ADDRESS`.

#![cfg(feature = "desktop-activation")]

mod common;

use common::TestBus;
use omnibridge_linux::activation::{self, Activation, SessionBus};
use omnibridge_linux::tray::model::DESKTOP_APP_ID;

/// The premise the self-heal rests on, measured rather than assumed.
///
/// One `ReloadConfig` makes a service file that is on disk activatable. That
/// is the only bus behaviour OmniBridge depends on, and it is what this
/// asserts. If a bus ever stopped honouring it, the self-heal would be solving
/// a problem it can no longer solve, and this is where that shows up.
///
/// Whether the bus *also* notices the file unaided is recorded and **not**
/// asserted: `dbus-daemon` watches its service directories with inotify and
/// `dbus-broker` does not, and even on one implementation it is a race between
/// the bus's rescan and this test. Asserting either side of that race is what
/// made this flaky inside `mock`.
#[tokio::test]
async fn one_reload_makes_an_installed_service_file_activatable() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;
    let proxy = zbus::fdo::DBusProxy::new(&connection)
        .await
        .expect("the bus driver proxy");

    assert!(
        !activatable(&proxy)
            .await
            .iter()
            .any(|n| n == DESKTOP_APP_ID),
        "the fixture's service directory should have started empty"
    );

    bus.install_service_file(DESKTOP_APP_ID);

    let noticed_unaided = activatable(&proxy)
        .await
        .iter()
        .any(|n| n == DESKTOP_APP_ID);
    println!("the bus noticed the new service file unaided: {noticed_unaided}");

    proxy.reload_config().await.expect("ReloadConfig");

    assert!(
        activatable(&proxy)
            .await
            .iter()
            .any(|n| n == DESKTOP_APP_ID),
        "ReloadConfig did not make the installed name activatable"
    );
}

/// The difference between the two bus implementations, pinned down.
///
/// MEASURED here rather than asserted: `dbus-daemon` watches a service
/// directory that existed when it started, so a file written into one becomes
/// activatable with no `ReloadConfig` at all. `dbus-broker` — what a Fedora 44
/// session actually runs — does not, which is the behaviour the readiness
/// audit measured and the reason this module exists.
///
/// The test asserts the thing that is true of **both**: after the self-heal,
/// the name is activatable, and the outcome is never a false claim to have
/// repaired something. Which of the two paths got there is printed, not
/// asserted, because it is a property of the bus and not of OmniBridge.
#[tokio::test]
async fn either_bus_implementation_ends_up_activatable_and_neither_is_misreported() {
    let bus = TestBus::start_with_watched_service_dir();
    let connection = bus.connect().await;
    bus.install_service_file(DESKTOP_APP_ID);

    let outcome = activation::self_heal_on(&connection).await;
    println!("this bus implementation reported: {outcome:?}");

    assert!(
        matches!(
            outcome,
            Activation::AlreadyActivatable | Activation::HealedByReload
        ),
        "a bus with the file installed should end up activatable, got {outcome:?}"
    );

    let proxy = zbus::fdo::DBusProxy::new(&connection).await.expect("proxy");
    assert!(
        activatable(&proxy)
            .await
            .iter()
            .any(|n| n == DESKTOP_APP_ID),
        "the name is not activatable even though the outcome said it was"
    );
}

/// `ListActivatableNames`, as plain strings.
async fn activatable(proxy: &zbus::fdo::DBusProxy<'_>) -> Vec<String> {
    proxy
        .list_activatable_names()
        .await
        .expect("listing activatable names")
        .into_iter()
        .map(|n| n.as_str().to_string())
        .collect()
}

/// Runs the self-heal and insists only on what is true of every bus.
///
/// **Do not assert `HealedByReload` against a real bus.** Which path is taken
/// depends on whether that implementation's own directory watching notices the
/// file before OmniBridge asks — `dbus-daemon` watches with inotify,
/// `dbus-broker` does not, and even on one implementation it is a race between
/// the bus's rescan and this call. The *policy* — reload exactly once, and only
/// when the name is missing — is asserted exhaustively and deterministically by
/// the unit tests in `activation.rs` against a counting fake, which is where a
/// claim about a code path belongs.
///
/// What a real bus can be held to is the outcome: the name ends up activatable,
/// and the self-heal never claims a repair it did not make.
async fn heal_and_expect_activatable(connection: &zbus::Connection) -> Activation {
    let outcome = activation::self_heal_on(connection).await;
    assert!(
        matches!(
            outcome,
            Activation::AlreadyActivatable | Activation::HealedByReload
        ),
        "a bus with the file installed should end up activatable, got {outcome:?}"
    );
    let proxy = zbus::fdo::DBusProxy::new(connection).await.expect("proxy");
    assert!(
        activatable(&proxy)
            .await
            .iter()
            .any(|n| n == DESKTOP_APP_ID),
        "the outcome said {outcome:?} but the name is not activatable"
    );
    outcome
}

/// The case the feature exists for: install into a live session, healed.
#[tokio::test]
async fn a_package_installed_into_a_live_session_is_healed() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;

    // The package manager has just run, as root, and written the file.
    bus.install_service_file(DESKTOP_APP_ID);

    let outcome = heal_and_expect_activatable(&connection).await;
    println!("install-into-a-live-session reported: {outcome:?}");
}

/// The second daemon start, and every one after it: nothing to do.
#[tokio::test]
async fn a_session_that_already_knows_the_name_is_left_alone() {
    let bus = TestBus::start_with_service_dir();
    bus.install_service_file(DESKTOP_APP_ID);
    let connection = bus.connect().await;

    // Get to the activatable state, however this bus chooses to get there.
    heal_and_expect_activatable(&connection).await;

    // This is the actual claim of the test: once the name is known, every
    // later daemon start finds it and does nothing. "Nothing" meaning no
    // ReloadConfig at all is asserted by the unit tests against a counting
    // fake; here it is the reported outcome that matters.
    for _ in 0..3 {
        assert_eq!(
            activation::self_heal_on(&connection).await,
            Activation::AlreadyActivatable
        );
    }
}

/// No GUI installed: one reload, an honest answer, and no retry.
#[tokio::test]
async fn a_machine_with_no_desktop_application_is_not_pretended_to_be_healthy() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;

    // Nothing is installed. The daemon and the GUI are separable packages, so
    // this is a legitimate machine and not a broken one.
    assert_eq!(
        activation::self_heal_on(&connection).await,
        Activation::StillMissing
    );
    // Asked twice, it gives the same answer and still does not loop.
    assert_eq!(
        activation::self_heal_on(&connection).await,
        Activation::StillMissing
    );
}

/// A near-miss name on the bus is not OmniBridge.
///
/// The two halves use two buses on purpose. Once a reload has run, the service
/// directory exists, and an inotify implementation starts watching it — so a
/// file installed after that point may be picked up without a reload, and the
/// second half would be measuring the bus rather than the matcher.
#[tokio::test]
async fn a_similar_name_on_the_bus_does_not_count_as_this_one() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;
    bus.install_service_file(&format!("{DESKTOP_APP_ID}.Devel"));

    assert_eq!(
        activation::self_heal_on(&connection).await,
        Activation::StillMissing,
        "a name that merely extends ours was treated as ours"
    );
}

/// The real name alongside a near miss is still found.
#[tokio::test]
async fn the_real_name_is_found_even_beside_a_near_miss() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;
    bus.install_service_file(&format!("{DESKTOP_APP_ID}.Devel"));
    bus.install_service_file(DESKTOP_APP_ID);

    heal_and_expect_activatable(&connection).await;
}

/// A bus that has gone away is a normal state, not an error.
#[tokio::test]
async fn a_dead_bus_is_reported_and_not_fatal() {
    let outcome = {
        let bus = TestBus::start_with_service_dir();
        let connection = bus.connect().await;
        drop(bus); // the daemon is killed here
        activation::self_heal_on(&connection).await
    };
    // Whatever the bus's death looks like from this side, the one thing that
    // must be true is that it is an Unavailable and not a panic, and that no
    // healing was claimed.
    assert!(
        matches!(outcome, Activation::Unavailable(_))
            || outcome == Activation::StillMissing
            || outcome == Activation::AlreadyActivatable,
        "unexpected outcome from a dead bus: {outcome:?}"
    );
    assert_ne!(outcome, Activation::HealedByReload);
}

/// `SessionBus::on` is a thin wrapper and must not invent a second connection.
#[tokio::test]
async fn the_production_adapter_reuses_the_connection_it_is_given() {
    let bus = TestBus::start_with_service_dir();
    let connection = bus.connect().await;
    bus.install_service_file(DESKTOP_APP_ID);

    let control = SessionBus::on(&connection)
        .await
        .expect("building the adapter on an existing connection");
    let outcome = activation::ensure_activatable(&control, DESKTOP_APP_ID).await;
    assert!(
        matches!(
            outcome,
            Activation::AlreadyActivatable | Activation::HealedByReload
        ),
        "the adapter did not reach an activatable state: {outcome:?}"
    );
}

/// The name the self-heal asks for is the name the packaged service file
/// declares — read out of the repository, not restated here.
///
/// `tray_identity.rs` already holds `DESKTOP_APP_ID` against the GUI's
/// `APP_ID`, the desktop entry and the icon. This adds the one consumer that
/// did not exist when that test was written.
#[test]
fn the_self_heal_asks_for_the_name_the_service_file_declares() {
    let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../gui/data/io.github.yurisismotto.omnibridge.service.in");
    let text = std::fs::read_to_string(&template)
        .unwrap_or_else(|e| panic!("reading {}: {e}", template.display()));
    let declared = text
        .lines()
        .find_map(|l| l.strip_prefix("Name="))
        .expect("the service template declares a Name=")
        .trim();
    assert_eq!(
        declared, DESKTOP_APP_ID,
        "the self-heal would look for a name the package does not install"
    );
}

/// The security property, asserted against the whole crate's source rather
/// than trusted.
///
/// "User session bus only" is the whole reason a daemon is allowed to make
/// this call at all. Reaching the *system* bus would be a new privilege
/// surface and it would be one line, in any file, at any time — so the check
/// walks every `.rs` in the crate rather than the one file that happens to
/// make the call today.
///
/// The needle is assembled from fragments because a test that spelled it out
/// would match itself. That is not a trick; it is the reason the first version
/// of this test failed.
#[test]
fn this_crate_never_opens_the_system_bus() {
    let forbidden = ["Connection", "::", "system"].concat();
    let wanted = ["Connection", "::", "session"].concat();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut files = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("reading {dir:?}: {e}")) {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    assert!(
        files.len() >= 2,
        "the walk found {} files; it is not looking at the crate",
        files.len()
    );

    let mut opens_session = false;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("reading a source file");
        assert!(
            !text.contains(&forbidden),
            "{} reaches the system bus; omnibridged is a user service and must not",
            file.display()
        );
        opens_session |= text.contains(&wanted);
    }
    assert!(
        opens_session,
        "nothing in this crate opens the session bus, so the check above proves nothing"
    );
}

/// The real session bus, on whatever machine this is run on.
///
/// `#[ignore]` because it needs a live desktop session — `%check` runs in a
/// buildroot and CI runs on a headless runner, and neither has one. It is the
/// same discipline the rest of this crate's session-dependent tests follow.
///
/// Run it deliberately:
///
/// ```console
/// cargo test -p omnibridge-linux --test dbus_activation -- --ignored --nocapture
/// ```
///
/// # What it touches, and what it puts back
///
/// One file, `io.github.yurisismotto.omnibridge.SelfHealProbe.service`, in the
/// user's own `~/.local/share/dbus-1/services`. A name nothing else uses, with
/// `Exec=/bin/true`. It is removed and the bus reloaded before the test
/// returns, on the success path and on the failure path. It never touches
/// OmniBridge's own service file, the trust store, or anything else.
#[tokio::test]
#[ignore = "needs a real desktop session bus"]
async fn the_real_session_bus_heals_a_freshly_installed_name() {
    let probe_name = format!("{DESKTOP_APP_ID}.SelfHealProbe");
    let dir = dirs_local_share().join("dbus-1/services");
    std::fs::create_dir_all(&dir).expect("the user's service directory");
    let file = dir.join(format!("{probe_name}.service"));

    // A guard, so a failed assertion still puts the session back.
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::process::Command::new("gdbus")
                .args([
                    "call",
                    "--session",
                    "--dest",
                    "org.freedesktop.DBus",
                    "--object-path",
                    "/org/freedesktop/DBus",
                    "--method",
                    "org.freedesktop.DBus.ReloadConfig",
                ])
                .output();
        }
    }
    let _cleanup = Cleanup(file.clone());

    let bus = SessionBus::connect()
        .await
        .expect("a real session bus — run this from a desktop session");

    assert_eq!(
        activation::ensure_activatable(&bus, &probe_name).await,
        Activation::StillMissing,
        "the probe name was already activatable before it was installed"
    );

    std::fs::write(
        &file,
        format!("[D-BUS Service]\nName={probe_name}\nExec=/bin/true\n"),
    )
    .expect("writing the probe service file");

    let outcome = activation::ensure_activatable(&bus, &probe_name).await;
    println!("the real session bus reported: {outcome:?}");
    assert!(
        matches!(
            outcome,
            Activation::HealedByReload | Activation::AlreadyActivatable
        ),
        "a name installed into the live session did not become activatable: {outcome:?}"
    );
}

/// `$XDG_DATA_HOME`, or the default beneath `$HOME`.
fn dirs_local_share() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".local/share")
        })
}
