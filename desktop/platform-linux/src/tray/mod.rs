//! The KDE Plasma tray item — a `StatusNotifierItem` on the session bus.
//!
//! # Who owns the icon
//!
//! `anyflowd` does. That is the decision this module exists to implement and
//! the one worth defending, because the alternative is easier and wrong.
//!
//! A tray icon has to be there whenever the product is there. AnyFlow's
//! "whenever the product is there" is `anyflowd`: a `systemd --user` service
//! that starts at login and holds the TCP listener, the mDNS record, the trust
//! store and every capability. The GUI is not that, deliberately — it is two
//! windows a person opens and closes, and `desktop/gui/src/lib.rs` has said so
//! since the Quick Panel sprint: *"the agent is `anyflowd` and stays the only
//! long-lived process AnyFlow runs."*
//!
//! So the item is owned by the process that is already always running. The
//! obvious shortcut — keep `anyflow-gui` alive forever, hidden, because GTK
//! makes drawing a tray icon easy — would have made AnyFlow a product with two
//! resident processes, one of which exists only to hold an icon, and would
//! have reversed a stated architectural position as a side effect of a UI
//! feature.
//!
//! ```text
//! anyflowd  ──owns──►  StatusNotifierItem  ──click──►  session D-Bus
//!  (always)             /StatusNotifierItem                 │
//!                       /MenuBar (DBusMenu)                 ▼
//!                                              io.github.yurisismotto.anyflow
//!                                                 org.freedesktop.Application
//!                                                     ActivateAction(…)
//!                                                          │
//!                                              anyflow-gui, started by the bus
//!                                              if it is not already running,
//!                                              and gone again when its window
//!                                              is closed
//! ```
//!
//! # No GTK, no KDE, no new dependency
//!
//! The daemon does not depend on `anyflow-gui`, on GTK, on Qt, on KDE
//! Frameworks, on `libappindicator` or on a tray crate. It speaks the two
//! D-Bus interfaces directly, with the `zbus` that AnyFlow's D-Bus-using
//! capabilities already resolve. `KDE-STATUSNOTIFIER-V1.md` §16 records the
//! dependency review.
//!
//! # Layout
//!
//! | Module | What it is |
//! | --- | --- |
//! | [`model`] | identity, the menu, the closed action set — no D-Bus at all |
//! | [`activate`] | `TrayAction` → the GUI's existing `GAction`s |
//! | [`item`] | `org.kde.StatusNotifierItem` |
//! | [`menu`] | `com.canonical.dbusmenu` |
//! | [`watcher`] | registering, and re-registering when the shell restarts |

pub mod activate;
pub mod item;
pub mod menu;
pub mod model;
pub mod watcher;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub use activate::{ActivationError, ApplicationActivator, DbusActivator};
pub use model::{TrayAction, ICON_NAME, ITEM_ID, ITEM_STATUS, ITEM_TITLE};

/// Distinguishes items within one process, exactly as KDE's client does.
///
/// KDE names an item `org.kde.StatusNotifierItem-<pid>-<n>` with `n` counting
/// up per process. `anyflowd` publishes one item and one only, so `n` is
/// always 1 in production — the counter exists so that the deterministic tests
/// can raise two items in a single test binary without inventing a naming
/// scheme no shell has ever seen.
static ITEM_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Why the tray could not be published.
#[derive(Debug)]
pub enum TrayError {
    /// There is no session bus to publish on.
    ///
    /// A normal state: a headless machine, a `ssh` session, a container.
    NoSessionBus(String),
    /// The bus refused the item's name or its objects.
    Publish(String),
}

impl std::fmt::Display for TrayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrayError::NoSessionBus(why) => write!(f, "no session bus ({why})"),
            TrayError::Publish(why) => write!(f, "could not publish the tray item ({why})"),
        }
    }
}

impl std::error::Error for TrayError {}

/// A published item: its bus name, and the connection serving it.
pub struct PublishedItem {
    connection: zbus::Connection,
    bus_name: String,
}

impl PublishedItem {
    /// The well-known name the watcher is told to look at.
    pub fn bus_name(&self) -> &str {
        &self.bus_name
    }

    /// The connection the item and its menu are exported on.
    pub fn connection(&self) -> &zbus::Connection {
        &self.connection
    }

    /// Follows the desktop shell until the bus goes away.
    pub async fn follow_shell(&self) -> zbus::Result<()> {
        watcher::follow(&self.connection, &self.bus_name).await
    }
}

/// Exports the item and its menu on `connection`, then claims the item's name.
///
/// **The objects go up before the name does.** Plasma's watcher validates a
/// registration by introspecting the item, so a window in which the name
/// answers and the object does not is a window in which an item can be
/// rejected for being empty. Serving first closes it.
pub async fn publish(
    connection: zbus::Connection,
    activator: Arc<dyn ApplicationActivator>,
) -> Result<PublishedItem, TrayError> {
    let bus_name = format!(
        "org.kde.StatusNotifierItem-{}-{}",
        std::process::id(),
        ITEM_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
    );

    let server = connection.object_server();
    server
        .at(
            item::ITEM_OBJECT_PATH,
            item::StatusNotifierItem::new(Arc::clone(&activator)),
        )
        .await
        .map_err(|e| TrayError::Publish(activate::describe_bus_error(&e)))?;
    server
        .at(item::MENU_OBJECT_PATH, menu::TrayMenu::new(activator))
        .await
        .map_err(|e| TrayError::Publish(activate::describe_bus_error(&e)))?;

    connection
        .request_name(bus_name.as_str())
        .await
        .map_err(|e| TrayError::Publish(activate::describe_bus_error(&e)))?;

    Ok(PublishedItem {
        connection,
        bus_name,
    })
}

/// Keeps the tray alive for as long as it is held.
///
/// Dropping it stops the tray and nothing else.
pub struct TrayHandle {
    supervisor: tokio::task::JoinHandle<()>,
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        self.supervisor.abort();
    }
}

/// Starts the tray in the background.
///
/// # Supervision
///
/// Everything below this line is convenience. The tray is not part of
/// AnyFlow's security core, it holds no key, it answers no peer, and nothing
/// else in the daemon reads its state — so its failure must cost exactly the
/// tray and nothing more. Two things make that true:
///
/// * **it is not in the daemon's `select!`.** `anyflowd` races the network
///   listener, the control server and `ctrl_c`, and the first of those to
///   finish ends the process. The tray is a separate `tokio::spawn`, so it can
///   end — cleanly, with an error, or by panicking — without the daemon
///   noticing anything except a log line.
/// * **a panic is not silent.** A bare `tokio::spawn` swallows a panic into a
///   `JoinHandle` nobody awaits. The handle here *is* awaited, by a supervisor
///   task whose only job is to say so if the tray died that way. "The tray
///   stopped working and nothing was written down" is the failure mode this
///   costs one task to remove.
///
/// The session bus itself going away ends `follow_shell` normally; the
/// supervisor logs it once and returns. Nothing reconnects, because a session
/// bus that has gone away is a session that is ending.
pub fn spawn(activator_for: ActivatorChoice) -> TrayHandle {
    let inner = tokio::spawn(async move {
        match run(activator_for).await {
            Ok(()) => tracing::info!("the session bus closed; the AnyFlow tray item is gone"),
            Err(e) => tracing::info!(reason = %e, "no tray integration on this session"),
        }
    });
    let supervisor = tokio::spawn(async move {
        if let Err(e) = inner.await {
            if e.is_panic() {
                tracing::error!(
                    "the tray task stopped unexpectedly; AnyFlow continues without a \
                     tray item and everything else is unaffected"
                );
            }
        }
    });
    TrayHandle { supervisor }
}

/// How the tray reaches the desktop application.
///
/// One production choice, and a seam a test can take instead. An enum rather
/// than an `Option<Arc<dyn …>>` so that the production path names itself.
pub enum ActivatorChoice {
    /// `org.freedesktop.Application` on the session bus — the real one.
    SessionBus,
    /// Something else entirely. Used by the deterministic tests.
    Custom(Arc<dyn ApplicationActivator>),
}

async fn run(choice: ActivatorChoice) -> Result<(), TrayError> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| TrayError::NoSessionBus(activate::describe_bus_error(&e)))?;
    let activator: Arc<dyn ApplicationActivator> = match choice {
        ActivatorChoice::SessionBus => Arc::new(DbusActivator::new(connection.clone())),
        ActivatorChoice::Custom(a) => a,
    };
    let published = publish(connection, activator).await?;
    tracing::info!(
        item = %published.bus_name(),
        "AnyFlow tray item published on the session bus"
    );
    published
        .follow_shell()
        .await
        .map_err(|e| TrayError::Publish(activate::describe_bus_error(&e)))
}
