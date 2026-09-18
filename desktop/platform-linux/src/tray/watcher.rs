//! Registering with the desktop shell, and surviving it restarting.
//!
//! # The protocol, as Plasma actually implements it
//!
//! `org.kde.StatusNotifierWatcher` at `/StatusNotifierWatcher` exposes
//! `RegisterStatusNotifierItem(s)`. Plasma's implementation
//! (`plasma-workspace/statusnotifierwatcher/statusnotifierwatcher.cpp`) reads
//! the argument two ways: a string starting with `/` is an object path and the
//! service is taken from the *sender*; anything else is a bus name and the
//! path is assumed to be `/StatusNotifierItem`. This module sends the bus
//! name, which is what KDE's own client sends.
//!
//! Two behaviours of that implementation shape this one:
//!
//! * **it verifies.** Before accepting, the watcher builds a proxy at the
//!   item's path and checks it is valid, which means an introspection call.
//!   The item object must already be exported when the registration is sent —
//!   see [`super::item`].
//! * **it de-duplicates.** A second registration of the same service and path
//!   returns immediately. That is a safety net and not a licence: the loop
//!   below registers once per watcher owner because a shell that *did* honour
//!   a duplicate would show two AnyFlow icons.
//!
//! # Why this is not a retry loop
//!
//! A tray icon's absence is the normal case on most of the world's desktops.
//! On GNOME — the session this was developed on — `org.kde.StatusNotifierWatcher`
//! has no owner and never will, and a daemon that responded by retrying every
//! second would spend a laptop's battery producing a log line that says
//! nothing changed. So there is no timer here at all. The loop parks on
//! `NameOwnerChanged` for one name and wakes when that name gains or loses an
//! owner, which is the only moment anything could have changed.

use futures_lite::StreamExt as _;

/// The watcher's bus name.
pub const WATCHER_BUS_NAME: &str = "org.kde.StatusNotifierWatcher";
/// The watcher's object path.
pub const WATCHER_OBJECT_PATH: &str = "/StatusNotifierWatcher";
/// The watcher's interface.
pub const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";
/// The registration method.
pub const REGISTER_METHOD: &str = "RegisterStatusNotifierItem";

/// Follows the shell for as long as `connection` lives.
///
/// Returns when the bus goes away, which is when the signal stream ends. It
/// does not return on a registration failure: a watcher that refused once may
/// be a watcher that is starting up, and the next owner change is a better
/// trigger to try again than a guess about how long to wait.
pub async fn follow(connection: &zbus::Connection, item_bus_name: &str) -> zbus::Result<()> {
    let dbus = zbus::fdo::DBusProxy::new(connection).await?;

    // Subscribed before the first look, and that order is the whole race:
    // between asking "is the watcher there?" and getting an answer, a shell
    // can finish starting. Subscribing first means that appearance arrives as
    // a signal instead of falling into the gap.
    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, WATCHER_BUS_NAME)])
        .await?;

    // The unique name of the watcher this item is currently registered with,
    // or `None`. Holding the *owner* rather than a boolean is what makes one
    // case behave: a shell can be replaced in a single `NameOwnerChanged`
    // where both the old and the new owner are non-empty. A boolean would read
    // that as "still registered" and the new shell would never hear of this
    // item.
    let mut registered_with: Option<String> = None;

    match dbus.get_name_owner(WATCHER_BUS_NAME.try_into()?).await {
        Ok(owner) => {
            let owner = owner.as_str().to_string();
            if register(connection, item_bus_name).await {
                registered_with = Some(owner);
            }
        }
        Err(_) => {
            // No watcher. Said once, at info, and then never again for the
            // life of the process unless one appears — this is the ordinary
            // state of a GNOME session, not a fault, and the daemon is
            // entirely healthy without one.
            tracing::info!(
                "no desktop tray host on this session; the AnyFlow tray item is \
                 published and will register itself if one appears"
            );
        }
    }

    while let Some(change) = changes.next().await {
        let Ok(args) = change.args() else { continue };
        match args.new_owner().as_ref() {
            Some(owner) => {
                let owner = owner.as_str().to_string();
                if registered_with.as_deref() == Some(owner.as_str()) {
                    // The same shell, announcing itself again. Registering a
                    // second time is how an item becomes two icons.
                    continue;
                }
                if register(connection, item_bus_name).await {
                    registered_with = Some(owner);
                }
            }
            None => {
                if registered_with.take().is_some() {
                    // The item object stays exported and the bus name stays
                    // owned. Tearing them down would mean rebuilding them
                    // under time pressure when the shell came back, and there
                    // is nothing to gain: an exported object nobody is looking
                    // at costs nothing.
                    tracing::info!(
                        "the desktop tray host went away; the AnyFlow tray item stays \
                         published and will re-register when one returns"
                    );
                }
            }
        }
    }
    Ok(())
}

/// Sends one registration. `true` if the watcher accepted it.
async fn register(connection: &zbus::Connection, item_bus_name: &str) -> bool {
    let proxy = match zbus::Proxy::new(
        connection,
        WATCHER_BUS_NAME,
        WATCHER_OBJECT_PATH,
        WATCHER_INTERFACE,
    )
    .await
    {
        Ok(p) => p,
        Err(_) => return false,
    };
    match proxy.call_method(REGISTER_METHOD, &(item_bus_name,)).await {
        Ok(_) => {
            tracing::info!(item = %item_bus_name, "registered an AnyFlow tray item with the desktop shell");
            true
        }
        Err(e) => {
            // Not fatal and not retried on a timer. The next owner change is
            // the next attempt.
            tracing::info!(
                reason = %super::activate::describe_bus_error(&e),
                "the desktop tray host refused the AnyFlow tray item"
            );
            false
        }
    }
}
