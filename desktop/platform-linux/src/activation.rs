//! Making the desktop application activatable in a session that is already
//! running.
//!
//! # The problem, in one sentence
//!
//! A package installs
//! `/usr/share/dbus-1/services/io.github.yurisismotto.omnibridge.service`, and
//! the session bus that is *already running* does not read it until something
//! tells it to — so until the user logs out, clicking OmniBridge in the tray
//! gets `org.freedesktop.DBus.Error.ServiceUnknown` on a machine where
//! everything is installed correctly.
//!
//! ```text
//!   dnf install omnibridge          (as root)
//!        │
//!        ├─► /usr/share/dbus-1/services/…omnibridge.service   written
//!        │
//!        └─►  the user's session bus                          has not re-read it
//!                     │
//!   click the tray ───┴──► ActivateAction  ──►  ServiceUnknown
//! ```
//!
//! Fresh sessions are fine: a bus reads its service directories at start-up.
//! This is specifically the install-or-upgrade-into-a-live-session case, which
//! is exactly when a package manager runs.
//! `KDE-PLASMA-REAL-CERTIFICATION-V1.md` §40 measured both halves.
//!
//! # Why the daemon and not the package
//!
//! `%post` and `postinst` run as **root**. Root has no route to an arbitrary
//! user's session bus, and inventing one — enumerating `/run/user/*/bus` and
//! connecting as root — would be a privilege-boundary violation dressed up as
//! a fix. The readiness audit measured that no package on a Fedora 44 host
//! ships an rpm file trigger for `/usr/share/dbus-1/services`, and none can:
//! the session bus has no root-side equivalent of
//! `update-desktop-database`.
//!
//! `omnibridged` is the process that *is* in the right place. It runs as the
//! user, in the user's session, and it already holds a session-bus connection
//! for the tray and for notifications. It fires at precisely the right
//! moments: fresh install → the user enables the unit → the daemon starts →
//! this runs; upgrade → the user restarts the daemon → this runs.
//!
//! Audit §8.2, and the alternatives it rejected, is the authority for this
//! design.
//!
//! # What this is allowed to do, and what it must never do
//!
//! | Must | Must never |
//! | --- | --- |
//! | ask the **user session** bus | touch the system bus |
//! | call `ReloadConfig` **at most once** per daemon start | poll, retry, or loop |
//! | match the activation name **exactly** | match a prefix, a suffix or a substring |
//! | be a no-op when the name is already there | start, stop or activate anything |
//! | let the daemon start regardless of the outcome | make D-Bus a startup dependency |
//!
//! `ReloadConfig` asks the bus to re-read its own configuration. It starts no
//! process — including `omnibridge-gui`, which this must never cause to
//! launch. Nothing here reads a peer's input, touches the trust store, or
//! changes the protocol.
//!
//! # Best effort means best effort
//!
//! Every failure path returns a value and logs; none of them is an error the
//! daemon can fail on. A headless machine, an `ssh` session, a container, a
//! bus that refuses `ReloadConfig` under a hardened policy — all of them are
//! normal, and all of them end with OmniBridge moving files as usual and one
//! line in the journal.

/// What one self-heal attempt did.
///
/// Returned rather than logged-and-discarded so that a test can assert the
/// decision, and so that the daemon's log line is written in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
    /// The bus already knew the name. **Nothing was done** — no reload.
    ///
    /// The common case: every freshly booted session, and every daemon
    /// restart after the first.
    AlreadyActivatable,
    /// The name was missing; one `ReloadConfig` made it appear.
    ///
    /// This is the case the whole module exists for: a package installed into
    /// a live session, healed without a logout.
    HealedByReload,
    /// The name was missing and one `ReloadConfig` did not make it appear.
    ///
    /// Not an error here. It is what a machine with no `omnibridge-gui`
    /// installed looks like — the daemon and the GUI are separable — and it is
    /// also what a genuinely broken install looks like. Distinguishing them is
    /// not this code's job.
    StillMissing,
    /// The bus could not be asked. Best effort ends here.
    Unavailable(Unavailable),
}

/// Why a self-heal attempt could not reach a conclusion.
///
/// Each carries the *bus error name* and never the far end's message: the far
/// end is another process on the session bus, and its text is not ours to put
/// in a log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    /// There is no session bus. A headless machine, `ssh`, a container.
    NoSessionBus(String),
    /// `ListActivatableNames` failed, so nothing is known.
    ListFailed(String),
    /// The name was missing and the bus refused to reload.
    ReloadFailed(String),
    /// The reload was accepted but the re-query failed.
    RecheckFailed(String),
}

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unavailable::NoSessionBus(why) => write!(f, "no session bus ({why})"),
            Unavailable::ListFailed(why) => write!(f, "ListActivatableNames failed ({why})"),
            Unavailable::ReloadFailed(why) => write!(f, "ReloadConfig failed ({why})"),
            Unavailable::RecheckFailed(why) => write!(f, "the re-query failed ({why})"),
        }
    }
}

impl std::fmt::Display for Activation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Activation::AlreadyActivatable => write!(f, "already activatable"),
            Activation::HealedByReload => write!(f, "activatable after one ReloadConfig"),
            Activation::StillMissing => write!(f, "not activatable after one ReloadConfig"),
            Activation::Unavailable(why) => write!(f, "{why}"),
        }
    }
}

/// The two calls this module makes, and no others.
///
/// A trait rather than a concrete `zbus` type so that the decision table above
/// can be tested exhaustively — including the paths a real bus will not
/// produce on demand, like a refused `ReloadConfig` — without a session, a
/// display or a package install.
///
/// It is deliberately this small. A wider trait would be a place to put a
/// third call, and there is no third call.
#[async_trait::async_trait]
pub trait SessionBusControl: Send + Sync {
    /// `org.freedesktop.DBus.ListActivatableNames`.
    async fn list_activatable_names(&self) -> Result<Vec<String>, String>;

    /// `org.freedesktop.DBus.ReloadConfig`.
    ///
    /// Re-reads the bus's own configuration and service directories. Starts
    /// nothing.
    async fn reload_config(&self) -> Result<(), String>;
}

/// Asks the bus for `name`, and reloads **once** if it is missing.
///
/// The whole policy is here, in one function with no I/O of its own, so that
/// "at most one reload" is a property of something readable rather than of a
/// call graph.
///
/// The name comparison is `==`. Not `contains`, not `starts_with`:
/// `io.github.yurisismotto.omnibridge` and
/// `io.github.yurisismotto.omnibridge.Devel` are different applications, and a
/// prefix match would report the wrong one as healed.
pub async fn ensure_activatable(bus: &dyn SessionBusControl, name: &str) -> Activation {
    let names = match bus.list_activatable_names().await {
        Ok(names) => names,
        Err(why) => return Activation::Unavailable(Unavailable::ListFailed(why)),
    };
    if names.iter().any(|n| n == name) {
        return Activation::AlreadyActivatable;
    }

    // The one reload. There is no loop around this and no second call below
    // it; the function returns on every path after it.
    if let Err(why) = bus.reload_config().await {
        return Activation::Unavailable(Unavailable::ReloadFailed(why));
    }

    match bus.list_activatable_names().await {
        Ok(names) if names.iter().any(|n| n == name) => Activation::HealedByReload,
        Ok(_) => Activation::StillMissing,
        Err(why) => Activation::Unavailable(Unavailable::RecheckFailed(why)),
    }
}

mod session {
    use super::{Activation, SessionBusControl, Unavailable};
    use crate::tray::activate::describe_bus_error;
    use crate::tray::model::DESKTOP_APP_ID;

    /// The real thing: `org.freedesktop.DBus` on the **user session** bus.
    ///
    /// `zbus::Connection::session()` reads `DBUS_SESSION_BUS_ADDRESS`, or falls
    /// back to `$XDG_RUNTIME_DIR/bus`. Nothing in this crate constructs a
    /// connection to the *system* bus, and `dbus_activation.rs` asserts that
    /// by walking every source file — because "user session bus only" is a
    /// security property and not a convention.
    ///
    /// (That test matches on the constructor's literal name, which is why the
    /// sentence above spells it out in prose instead.)
    pub struct SessionBus {
        proxy: zbus::fdo::DBusProxy<'static>,
    }

    impl SessionBus {
        /// Connects to the session bus, or says why it could not.
        pub async fn connect() -> Result<Self, Unavailable> {
            let connection = zbus::Connection::session()
                .await
                .map_err(|e| Unavailable::NoSessionBus(describe_bus_error(&e)))?;
            Self::on(&connection).await
        }

        /// Uses a connection somebody else already has.
        ///
        /// The daemon has one for the tray and for notifications; a second
        /// would only be a second thing to notice going away.
        pub async fn on(connection: &zbus::Connection) -> Result<Self, Unavailable> {
            let proxy = zbus::fdo::DBusProxy::new(connection)
                .await
                .map_err(|e| Unavailable::NoSessionBus(describe_bus_error(&e)))?;
            Ok(Self { proxy })
        }
    }

    #[async_trait::async_trait]
    impl SessionBusControl for SessionBus {
        async fn list_activatable_names(&self) -> Result<Vec<String>, String> {
            self.proxy
                .list_activatable_names()
                .await
                .map(|names| names.into_iter().map(|n| n.as_str().to_string()).collect())
                .map_err(|e| describe_fdo_error(&e))
        }

        async fn reload_config(&self) -> Result<(), String> {
            self.proxy
                .reload_config()
                .await
                .map_err(|e| describe_fdo_error(&e))
        }
    }

    /// An `fdo` error reduced to something safe to log — its name, never the
    /// far end's message.
    fn describe_fdo_error(error: &zbus::fdo::Error) -> String {
        match error {
            zbus::fdo::Error::ZBus(e) => describe_bus_error(e),
            zbus::fdo::Error::AccessDenied(_) => "org.freedesktop.DBus.Error.AccessDenied".into(),
            zbus::fdo::Error::ServiceUnknown(_) => {
                "org.freedesktop.DBus.Error.ServiceUnknown".into()
            }
            zbus::fdo::Error::NotSupported(_) => "org.freedesktop.DBus.Error.NotSupported".into(),
            zbus::fdo::Error::Failed(_) => "org.freedesktop.DBus.Error.Failed".into(),
            _ => "bus error".into(),
        }
    }

    /// One self-heal attempt against the user's session bus, start to finish.
    ///
    /// Never returns an error: the daemon calls this and carries on whatever
    /// happens. It is `async` and does one round trip; nothing here blocks
    /// start-up for longer than the bus takes to answer twice.
    pub async fn self_heal_desktop_activation() -> Activation {
        match SessionBus::connect().await {
            Ok(bus) => super::ensure_activatable(&bus, DESKTOP_APP_ID).await,
            Err(why) => Activation::Unavailable(why),
        }
    }

    /// The same, reusing a connection the caller already holds.
    pub async fn self_heal_on(connection: &zbus::Connection) -> Activation {
        match SessionBus::on(connection).await {
            Ok(bus) => super::ensure_activatable(&bus, DESKTOP_APP_ID).await,
            Err(why) => Activation::Unavailable(why),
        }
    }
}

pub use session::{self_heal_desktop_activation, self_heal_on, SessionBus};

/// Writes the outcome down, at the level it deserves.
///
/// One place, so that the daemon's start-up log cannot disagree with what the
/// function decided, and so that the "we changed something" case is the only
/// one that is more than `debug`.
pub fn log(outcome: &Activation) {
    match outcome {
        // The overwhelmingly common case. Nothing happened; nothing to say at
        // a level anyone reads.
        Activation::AlreadyActivatable => {
            tracing::debug!("the desktop application is already activatable on the session bus");
        }
        // Something was actually repaired. This is worth a line, because it
        // means a package was installed into this live session.
        Activation::HealedByReload => {
            tracing::info!(
                "the session bus had not read OmniBridge's D-Bus service file; one \
                 ReloadConfig fixed it, so the desktop application activates without a logout"
            );
        }
        Activation::StillMissing => {
            tracing::debug!(
                "the desktop application is not activatable on this session bus after a \
                 reload; omnibridge-gui is probably not installed. The daemon is unaffected"
            );
        }
        Activation::Unavailable(why) => {
            tracing::debug!(reason = %why, "could not check D-Bus activation; continuing");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const NAME: &str = "io.github.yurisismotto.omnibridge";

    /// A bus whose answers are scripted and whose calls are counted.
    ///
    /// `answers` is consumed one `list_activatable_names` at a time, so a test
    /// states the *sequence* the bus gives back rather than a flag the fake
    /// interprets.
    struct FakeBus {
        answers: Mutex<Vec<Result<Vec<String>, String>>>,
        reload: Result<(), String>,
        reloads: AtomicUsize,
        lists: AtomicUsize,
    }

    impl FakeBus {
        fn new(answers: Vec<Result<Vec<String>, String>>) -> Self {
            Self {
                answers: Mutex::new(answers),
                reload: Ok(()),
                reloads: AtomicUsize::new(0),
                lists: AtomicUsize::new(0),
            }
        }
        fn refusing_reload(mut self, why: &str) -> Self {
            self.reload = Err(why.to_string());
            self
        }
        fn reloads(&self) -> usize {
            self.reloads.load(Ordering::SeqCst)
        }
        fn lists(&self) -> usize {
            self.lists.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl SessionBusControl for FakeBus {
        async fn list_activatable_names(&self) -> Result<Vec<String>, String> {
            self.lists.fetch_add(1, Ordering::SeqCst);
            let mut answers = self.answers.lock().expect("the fake's lock");
            if answers.is_empty() {
                panic!("the code asked the bus more times than the test scripted");
            }
            answers.remove(0)
        }
        async fn reload_config(&self) -> Result<(), String> {
            self.reloads.fetch_add(1, Ordering::SeqCst);
            self.reload.clone()
        }
    }

    fn present() -> Vec<String> {
        vec![
            "org.freedesktop.Notifications".into(),
            NAME.into(),
            "org.gnome.Shell".into(),
        ]
    }
    fn absent() -> Vec<String> {
        vec![
            "org.freedesktop.Notifications".into(),
            "org.gnome.Shell".into(),
        ]
    }

    #[tokio::test]
    async fn already_visible_does_nothing_at_all() {
        let bus = FakeBus::new(vec![Ok(present())]);
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::AlreadyActivatable
        );
        // The point of the case: no reload. A bus on a freshly booted session
        // must not be asked to re-read its configuration on every daemon
        // start.
        assert_eq!(bus.reloads(), 0);
        assert_eq!(bus.lists(), 1);
    }

    #[tokio::test]
    async fn missing_then_reload_then_visible() {
        let bus = FakeBus::new(vec![Ok(absent()), Ok(present())]);
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::HealedByReload
        );
        assert_eq!(bus.reloads(), 1);
        assert_eq!(bus.lists(), 2);
    }

    #[tokio::test]
    async fn missing_then_reload_then_still_missing_stops() {
        let bus = FakeBus::new(vec![Ok(absent()), Ok(absent())]);
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::StillMissing
        );
        // The failure mode this asserts against is a retry loop: exactly one
        // reload, exactly two queries, and then it gives up.
        assert_eq!(bus.reloads(), 1);
        assert_eq!(bus.lists(), 2);
    }

    #[tokio::test]
    async fn a_refused_reload_is_not_retried() {
        let bus = FakeBus::new(vec![Ok(absent())])
            .refusing_reload("org.freedesktop.DBus.Error.AccessDenied");
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::Unavailable(Unavailable::ReloadFailed(
                "org.freedesktop.DBus.Error.AccessDenied".into()
            ))
        );
        assert_eq!(bus.reloads(), 1);
        // And it did not re-query after a reload that did not happen.
        assert_eq!(bus.lists(), 1);
    }

    #[tokio::test]
    async fn a_bus_that_cannot_be_listed_is_not_reloaded() {
        let bus = FakeBus::new(vec![Err("org.freedesktop.DBus.Error.Failed".into())]);
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::Unavailable(Unavailable::ListFailed(
                "org.freedesktop.DBus.Error.Failed".into()
            ))
        );
        // Nothing is known, so nothing is changed. Reloading a bus we could
        // not even query would be acting on a guess.
        assert_eq!(bus.reloads(), 0);
    }

    #[tokio::test]
    async fn a_failed_recheck_is_reported_rather_than_guessed() {
        let bus = FakeBus::new(vec![Ok(absent()), Err("i/o error".into())]);
        assert_eq!(
            ensure_activatable(&bus, NAME).await,
            Activation::Unavailable(Unavailable::RecheckFailed("i/o error".into()))
        );
        assert_eq!(bus.reloads(), 1);
    }

    #[tokio::test]
    async fn the_name_must_match_exactly() {
        // Every one of these contains the name as a substring, is a prefix of
        // it, or differs only in case. None of them is OmniBridge.
        for near_miss in [
            "io.github.yurisismotto.omnibridge.Devel",
            "io.github.yurisismotto.omnibridg",
            "io.github.yurisismotto.OmniBridge",
            "xio.github.yurisismotto.omnibridge",
            "io.github.yurisismotto.omnibridge2",
        ] {
            let bus = FakeBus::new(vec![Ok(vec![near_miss.into()]), Ok(vec![near_miss.into()])]);
            assert_eq!(
                ensure_activatable(&bus, NAME).await,
                Activation::StillMissing,
                "{near_miss} was treated as {NAME}"
            );
        }
    }

    #[tokio::test]
    async fn an_exact_match_is_found_wherever_it_sits_in_the_list() {
        for names in [
            vec![NAME.to_string()],
            vec!["a.b.c".into(), NAME.to_string()],
            vec![NAME.to_string(), "a.b.c".into()],
        ] {
            let bus = FakeBus::new(vec![Ok(names)]);
            assert_eq!(
                ensure_activatable(&bus, NAME).await,
                Activation::AlreadyActivatable
            );
            assert_eq!(bus.reloads(), 0);
        }
    }
}
