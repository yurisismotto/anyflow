//! Lock state from `org.freedesktop.login1`, on the system bus.
//!
//! # Why logind and not the screensaver
//!
//! [POC-NOTIF-02](../../../../../docs/research/notifications-v1/poc/POC-NOTIF-02.md)
//! measured three candidate sources on this exact hardware and only one of
//! them is usable:
//!
//! | Source | Verdict |
//! | --- | --- |
//! | `org.freedesktop.login1.Session.LockedHint` | **Authoritative.** Locked within 218 ms on the Super+L path, 1 ms on `loginctl lock-session`, and correctly stays `no` when the screen merely blanks |
//! | `org.gnome.ScreenSaver.ActiveChanged` | **Not a lock signal.** It *lagged* `LockedHint` by 665 ms on a real `ScreenSaver.Lock()` — a fail-**open** window in which the screen is locked and a sink gated on this signal is still posting full bodies — and it reports active for a blank *without* a lock, withholding content from an unlocked session |
//! | `org.freedesktop.ScreenSaver` | **A trap.** The portable-looking name is served on GNOME by the idle-inhibit implementation and refuses `GetActive` outright, so a detector built on it would report "unlocked" for ever |
//!
//! So `LockedHint` is read at connect and tracked through `PropertiesChanged`
//! on the session object. `ActiveChanged` is not subscribed at all: the
//! research permits subscribing it purely as a prompt to re-read `LockedHint`,
//! and this implementation declines even that, because a signal that fires for
//! a blank-without-a-lock would produce re-reads that change nothing and a
//! reader of this file would reasonably wonder which of the two was deciding.
//!
//! # The rule
//!
//! **Anything other than a confident "unlocked" is locked.** No system bus, no
//! logind, no session object, a property that will not deserialize, a call
//! that errors — every one of them answers `true`. A privacy control that
//! fails open is not a control, and this module is where that is decided so
//! that no caller has to remember it.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc;

use super::LockSource;

const LOGIND_DEST: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";
const USER_IFACE: &str = "org.freedesktop.login1.User";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

/// `LockedHint` on this process's own logind session.
pub struct LogindLock {
    connection: zbus::Connection,
    session_path: zbus::zvariant::OwnedObjectPath,
    /// The last value read, so a caller that asks between signals gets an
    /// answer without a round trip — and, critically, so a *failed* round trip
    /// does not have to guess. It never has to: the fallback is `true`.
    cached: AtomicBool,
    events: std::sync::Mutex<Option<mpsc::Receiver<bool>>>,
}

impl LogindLock {
    /// Connects to logind and resolves the session whose screen these
    /// notifications appear on.
    ///
    /// `None` when there is no system bus, no logind, or no session to
    /// resolve — a container, a build runner, a machine without systemd. The
    /// caller then composes [`super::UnknownLock`], which reports locked, so
    /// the degradation is toward withholding content rather than toward
    /// showing it.
    ///
    /// # Which session, and why it is not this process's
    ///
    /// The obvious implementation asks `GetSessionByPID(self)`, and it is
    /// wrong twice over. **It fails outright** for a daemon under
    /// `systemd --user`, whose processes live in `user@.service` and belong to
    /// no session at all. And when it does answer it can answer with the
    /// *wrong* session: a user with a graphical login and an ssh login has
    /// two, only one of them has a screen, and the other one never locks — so
    /// a sink that asked about it would report "unlocked" for ever. That is a
    /// **fail-open** answer on a privacy control, which is the exact failure
    /// this whole module exists to prevent.
    ///
    /// Measured on this host while the graphical session was locked:
    ///
    /// ```console
    /// $ gdbus call --system -d org.freedesktop.login1 \
    ///       -o /org/freedesktop/login1/user/_1000 \
    ///       -m org.freedesktop.DBus.Properties.Get \
    ///       org.freedesktop.login1.User Sessions
    /// (<[('3', objectpath '…/session/_33'), ('2', '…/session/_32')]>,)
    ///
    /// … session/_32  Type=wayland      LockedHint=true
    /// … session/_33  Type=unspecified  LockedHint=false
    /// ```
    ///
    /// So the question is answered by `User.Display` — logind's own name for
    /// the user's primary graphical session, which is where the notification
    /// server lives and therefore the screen these notifications are on. The
    /// two PID-based answers remain as fallbacks for the case where there is
    /// no `Display` session but the process is in one.
    pub async fn connect() -> Option<Self> {
        let connection = zbus::Connection::system().await.ok()?;
        let session_path = resolve_session(&connection).await?;

        let lock = Self {
            connection,
            session_path,
            cached: AtomicBool::new(true),
            events: std::sync::Mutex::new(None),
        };

        // Read once, up front, so the first notification is judged against the
        // truth rather than against the fail-closed default.
        let locked = lock.read_locked_hint().await;
        lock.cached.store(locked, Ordering::Release);
        tracing::info!(
            session = %lock.session_path.as_str(),
            locked,
            "lock state from logind LockedHint"
        );

        lock.spawn_property_pump().await;
        Some(lock)
    }

    /// Reads `LockedHint`. Any failure answers **locked**.
    async fn read_locked_hint(&self) -> bool {
        let proxy = match zbus::Proxy::new(
            &self.connection,
            LOGIND_DEST,
            self.session_path.as_str(),
            SESSION_IFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(_) => return true,
        };
        match proxy.get_property::<bool>("LockedHint").await {
            Ok(locked) => locked,
            Err(e) => {
                tracing::warn!(
                    error = %class(&e),
                    "could not read LockedHint; treating the session as locked"
                );
                true
            }
        }
    }

    /// Tracks `PropertiesChanged` on the session object.
    ///
    /// Event-driven, so ADR-0014's standing no-polling rule holds. A session
    /// whose signal subscription fails keeps the value read at connect and
    /// says so — which means the lock policy applies to notifications that
    /// arrive after a lock only if the lock happened before the daemon
    /// started. That is a real degradation, which is why
    /// [`LockSource::describe`] reports which source is in use.
    async fn spawn_property_pump(&self) {
        let proxy = match zbus::Proxy::new(
            &self.connection,
            LOGIND_DEST,
            self.session_path.as_str(),
            PROPERTIES_IFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %class(&e),
                    "cannot watch logind properties on this session"
                );
                return;
            }
        };

        let mut stream = match proxy.receive_signal("PropertiesChanged").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    error = %class(&e),
                    "cannot subscribe to logind PropertiesChanged"
                );
                return;
            }
        };

        let (tx, rx) = mpsc::channel(16);
        let connection = self.connection.clone();
        let session_path = self.session_path.clone();

        tokio::spawn(async move {
            use futures_lite::StreamExt as _;
            while let Some(message) = stream.next().await {
                // The body is borrowed from the message, so it has to outlive
                // the match below rather than being a temporary inside it.
                let body = message.body();
                let Ok((iface, changed, invalidated)) = body.deserialize::<(
                    String,
                    std::collections::HashMap<String, zbus::zvariant::Value<'_>>,
                    Vec<String>,
                )>() else {
                    continue;
                };
                if iface != SESSION_IFACE {
                    continue;
                }

                let locked = match changed.get("LockedHint") {
                    // A `LockedHint` that is not a boolean is a fact this
                    // process cannot interpret, and an uninterpretable privacy
                    // signal is locked.
                    Some(zbus::zvariant::Value::Bool(locked)) => *locked,
                    Some(_) => true,
                    None => {
                        if !invalidated.iter().any(|p| p == "LockedHint") {
                            continue;
                        }
                        // The property was invalidated rather than sent, so it
                        // has to be read back. A failed read answers locked,
                        // in `read_locked_hint`.
                        reread(&connection, session_path.as_str()).await
                    }
                };

                if tx.send(locked).await.is_err() {
                    return;
                }
            }
        });

        if let Ok(mut guard) = self.events.lock() {
            *guard = Some(rx);
        }
    }
}

/// Finds the session whose screen the notifications appear on.
///
/// In order: the user's primary graphical session, then this process's own
/// session, then the one `XDG_SESSION_ID` names. Each fallback is less
/// specific about *which screen*, which is why they are in this order and not
/// the reverse.
async fn resolve_session(connection: &zbus::Connection) -> Option<zbus::zvariant::OwnedObjectPath> {
    let manager = zbus::Proxy::new(connection, LOGIND_DEST, MANAGER_PATH, MANAGER_IFACE)
        .await
        .ok()?;

    // 1. `User.Display` — the primary graphical session.
    let uid = users_uid();
    if let Ok(user_path) = manager
        .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetUser", &(uid,))
        .await
    {
        if let Ok(user) = zbus::Proxy::new(connection, LOGIND_DEST, user_path, USER_IFACE).await {
            if let Ok((id, path)) = user
                .get_property::<(String, zbus::zvariant::OwnedObjectPath)>("Display")
                .await
            {
                if !id.is_empty() {
                    tracing::debug!(session = %id, "logind Display session");
                    return Some(path);
                }
            }
        }
    }

    // 2. This process's own session. Correct when the daemon runs inside the
    //    session scope; absent under `systemd --user`.
    if let Ok(path) = manager
        .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetSessionByPID", &(std::process::id(),))
        .await
    {
        return Some(path);
    }

    // 3. Whatever the environment says. Last, because the variable can be
    //    absent, stale, or inherited from a different session.
    let id = std::env::var("XDG_SESSION_ID")
        .ok()
        .filter(|v| !v.is_empty())?;
    let path = manager
        .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetSession", &(id.as_str(),))
        .await
        .ok()?;
    Some(path)
}

/// This process's real uid.
///
/// Read from `/proc/self/status` rather than through `libc::getuid`, because
/// this crate forbids `unsafe` and an FFI call for one integer is not worth
/// relaxing that for. A failure to parse resolves to `0`, whose `GetUser`
/// answer is root's — which has no `Display` session for an unprivileged
/// daemon, so the resolution simply falls through to the next candidate
/// instead of pointing at the wrong screen.
fn users_uid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("Uid:"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

async fn reread(connection: &zbus::Connection, path: &str) -> bool {
    let Ok(proxy) = zbus::Proxy::new(connection, LOGIND_DEST, path, SESSION_IFACE).await else {
        return true;
    };
    proxy
        .get_property::<bool>("LockedHint")
        .await
        .unwrap_or(true)
}

#[async_trait::async_trait]
impl LockSource for LogindLock {
    fn id(&self) -> &'static str {
        "logind"
    }

    fn describe(&self) -> String {
        format!(
            "org.freedesktop.login1.Session.LockedHint on {}",
            self.session_path.as_str()
        )
    }

    async fn is_locked(&self) -> bool {
        // Read live rather than from the cache. The cache exists so that a
        // signal-less session still has an answer, but the authoritative
        // question — "is the screen locked *right now*, before this
        // notification goes up" — deserves the round trip, and it is one
        // system-bus property read.
        let locked = self.read_locked_hint().await;
        self.cached.store(locked, Ordering::Release);
        locked
    }

    fn lock_events(&self) -> Option<mpsc::Receiver<bool>> {
        self.events.lock().ok()?.take()
    }
}

/// Renders a `zbus::Error` as a class, never as its message.
///
/// Same discipline as the notification sink's: a D-Bus error message is
/// written by the other end and is not this process's to forward into a log.
fn class(error: &zbus::Error) -> &'static str {
    match error {
        zbus::Error::MethodError(_, _, _) => "method-error",
        zbus::Error::InputOutput(_) => "io",
        zbus::Error::Address(_) => "address",
        zbus::Error::Handshake(_) => "handshake",
        zbus::Error::InvalidReply => "invalid-reply",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_renders_as_a_class_and_never_as_its_message() {
        let io = zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other(
            "SECRET-SESSION-DETAIL",
        )));
        assert_eq!(class(&io), "io");
    }

    #[test]
    fn the_screensaver_names_appear_nowhere_in_this_module() {
        // The trap is `org.freedesktop.ScreenSaver`, which on GNOME serves
        // idle-inhibit and refuses `GetActive`; the near-miss is
        // `org.gnome.ScreenSaver.ActiveChanged`, which measured 665 ms behind
        // the truth in the fail-open direction. Neither is used, and a future
        // edit that reaches for one has to delete this test to do it.
        // Assembled at run time so that this test's own source does not
        // contain the string it is looking for.
        let needle = format!("Screen{}", "Saver");
        let source = include_str!("logind.rs");
        let uses: Vec<&str> = source
            .lines()
            .filter(|line| {
                let code = line.trim_start();
                !code.starts_with("//") && !code.starts_with('*')
            })
            .filter(|line| line.contains(&needle))
            .collect();
        assert!(
            uses.is_empty(),
            "a screensaver interface reached the lock source: {uses:?}"
        );
    }
}
