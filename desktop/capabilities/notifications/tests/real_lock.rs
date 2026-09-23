//! The lock-state gate, against the real logind session.
//!
//! `#[ignore]`d like the D-Bus sink gate, and for the same reason: it needs a
//! real session.
//!
//! ```console
//! cargo test -p omnibridge-capability-notifications --test real_lock -- --ignored --nocapture
//! ```
//!
//! **It does not lock the screen.** Locking a maintainer's session from a test
//! suite would mean they had to type their password to get back to what they
//! were doing, and a suite that does that once will not be run again. What it
//! proves is that the implementation reads the same value the platform
//! reports — which is checkable against `loginctl` without touching the lock
//! at all:
//!
//! ```console
//! $ loginctl show-session "$XDG_SESSION_ID" -p LockedHint
//! LockedHint=no
//! ```
//!
//! The locked half of the behaviour is proved twice elsewhere: deterministically
//! in `sink.rs` against a steerable lock source, and on the real session in the
//! N2 hardware gate, where locking is a deliberate step a human performs.

// This whole target exercises the logind lock source, which exists only behind the
// `linux-dbus` feature. With the feature off it compiles to an empty
// test binary rather than a compile error, so the portable Windows gate
// can build every test target in this crate without an exclusion list.
#![cfg(feature = "linux-dbus")]

use omnibridge_capability_notifications::backend::{logind::LogindLock, LockSource, UnknownLock};

#[tokio::test]
#[ignore = "reads the real logind session; run with --ignored"]
async fn the_implementation_agrees_with_loginctl() {
    let lock = match LogindLock::connect().await {
        Some(lock) => lock,
        None => panic!(
            "logind has no session for this process.\n\n\
             Not necessarily a defect: a container or a machine without \
             systemd has none. Check with:\n  \
             loginctl show-session \"$XDG_SESSION_ID\" -p LockedHint"
        ),
    };

    eprintln!("lock source: {}", lock.describe());
    let observed = lock.is_locked().await;
    eprintln!("LockedHint via this implementation: {observed}");

    // The authority, read with a different tool, about the same session.
    //
    // Deliberately *not* `loginctl show-session self`: this test process may
    // belong to no session at all (a build runner, an ssh login, an agent),
    // and asking about "self" then either fails or answers about a session
    // with no screen. Which session is the right one to ask about is exactly
    // what the implementation had to get right, so the oracle resolves it the
    // same way a person would — through the user's `Display` session — and
    // then reads `LockedHint` off it.
    // By numeric uid, not `self`: `self` is unresolvable from a process that
    // belongs to no session, which is the situation this oracle exists for.
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .expect("id runs");
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();

    let display = std::process::Command::new("loginctl")
        .args(["show-user", &uid, "-p", "Display"])
        .output()
        .expect("loginctl runs");
    let display = String::from_utf8_lossy(&display.stdout);
    let session = display
        .trim()
        .strip_prefix("Display=")
        .filter(|id| !id.is_empty())
        .expect("the user has a graphical session")
        .to_string();

    let output = std::process::Command::new("loginctl")
        .args(["show-session", &session, "-p", "LockedHint", "-p", "Type"])
        .output()
        .expect("loginctl runs");
    let reported = String::from_utf8_lossy(&output.stdout);
    eprintln!("loginctl, session {session}:\n{}", reported.trim());

    let expected = reported.lines().any(|line| line == "LockedHint=yes");
    assert_eq!(
        observed, expected,
        "this implementation and loginctl must agree about the lock state of \
         the session the notifications appear on"
    );
}

#[tokio::test]
#[ignore = "reads the real logind session; run with --ignored"]
async fn a_platform_with_no_lock_source_reports_locked() {
    // The composition a machine without logind gets. It is not a stub that
    // says "unlocked because we do not know"; it is the fail-closed answer
    // written down as a type.
    assert!(UnknownLock.is_locked().await);
}
