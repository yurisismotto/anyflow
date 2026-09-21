//! Turning a [`TrayAction`] into a window on the screen.
//!
//! # The seam, and why it is a trait
//!
//! ```text
//! TrayAction  ->  ApplicationActivator  ->  org.freedesktop.Application
//!                                            io.github.yurisismotto.omnibridge
//!                                              -> the GAction the GUI already exports
//! ```
//!
//! The daemon does not link against `omnibridge-gui`, does not depend on GTK,
//! and does not know how a Quick Panel is built. It knows three action names.
//! Everything else is the session bus's problem, which is the point: the GUI
//! exported `app.quick-panel`, `app.settings` and `app.transfers` for exactly
//! this caller, one sprint before this caller existed.
//!
//! The trait exists so that the deterministic tests can watch what the tray
//! decided without a GTK application, a display, or a window ever being
//! involved. There is one production implementation and it is below.
//!
//! # What this must never become
//!
//! Not `Command::new("omnibridge-gui")`, and not `sh -c` around it. `omnibridged`
//! runs under a hardened `systemd --user` unit — `NoNewPrivileges`,
//! `ProtectSystem=strict`, `ProtectHome=read-only`, `MemoryDenyWriteExecute`,
//! `SystemCallFilter=@system-service` minus `@privileged`, and
//! `RestrictAddressFamilies` down to four families. A child process inherits
//! every one of those, so a GTK application launched as a child of the daemon
//! would be a GTK application running inside a sandbox designed for a socket
//! server: read-only `$HOME`, no window-system paths guaranteed, no ability to
//! ever write a file the user asked it to save.
//!
//! Activation over the bus has none of that problem, and it was measured
//! rather than assumed. A cold activation on this machine produced a GUI whose
//! parent is the systemd user manager and whose cgroup is
//! `app.slice/dbus-:1.2-io.github.yurisismotto.omnibridge@0.service` — a
//! transient unit of its own, with no relationship to `omnibridged.service` at
//! all.

use std::collections::HashMap;

use zbus::zvariant::Value;

use super::model::{TrayAction, DESKTOP_APP_ID, DESKTOP_APP_OBJECT_PATH};

/// The interface GApplication exports for exactly this purpose.
const APPLICATION_IFACE: &str = "org.freedesktop.Application";

/// Why an activation did not happen.
///
/// Deliberately coarse. What a caller does with this is log it and carry on;
/// nothing branches on the variant, and a richer type would only invite
/// putting the failing detail — which comes from the bus — into a message.
#[derive(Debug)]
pub enum ActivationError {
    /// The bus refused, the name could not be started, or the call timed out.
    Failed(String),
}

impl std::fmt::Display for ActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivationError::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for ActivationError {}

/// Presents one of OmniBridge's desktop surfaces.
#[async_trait::async_trait]
pub trait ApplicationActivator: Send + Sync + 'static {
    /// Presents the surface `action` names.
    ///
    /// `activation_token` is the XDG activation token the shell handed over
    /// just before the click, if it handed one over. It is a focus-stealing
    /// credential for this one activation and nothing else — see
    /// [`DbusActivator::activate`].
    async fn activate(
        &self,
        action: TrayAction,
        activation_token: Option<String>,
    ) -> Result<(), ActivationError>;
}

/// The production activator: a method call on the session bus.
pub struct DbusActivator {
    connection: zbus::Connection,
}

impl DbusActivator {
    /// Uses `connection` to reach the desktop application.
    ///
    /// The same connection the item is served on. One connection, because the
    /// two directions are the same session bus and holding a second would only
    /// mean two things to notice going away.
    pub fn new(connection: zbus::Connection) -> Self {
        Self { connection }
    }
}

#[async_trait::async_trait]
impl ApplicationActivator for DbusActivator {
    async fn activate(
        &self,
        action: TrayAction,
        activation_token: Option<String>,
    ) -> Result<(), ActivationError> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            DESKTOP_APP_ID,
            DESKTOP_APP_OBJECT_PATH,
            APPLICATION_IFACE,
        )
        .await
        .map_err(|e| ActivationError::Failed(describe_bus_error(&e)))?;

        // `platform-data`, the third argument of `ActivateAction`. The one key
        // that goes in is the compositor's activation token, which is what
        // lets the window GTK is about to present actually take focus on
        // Wayland instead of quietly asking for attention behind whatever the
        // person was looking at.
        //
        // It arrived from the shell over D-Bus, which is why it is bounded
        // before it is forwarded: a token is a short opaque handle, and
        // anything that is not one is not made harmless by passing it on. It
        // is never treated as a name, a path or a command — it is one value
        // under one fixed key, and GTK is the only thing that reads it.
        let mut platform: HashMap<&str, Value<'_>> = HashMap::new();
        if let Some(token) = activation_token.filter(|t| usable_token(t)) {
            platform.insert("activation-token", Value::from(token));
        }

        // `ActivateAction(action-name, parameter, platform-data)`. The
        // parameter array is empty because all three actions are
        // parameterless — measured on the running GUI, whose `DescribeAll`
        // reports `signature ''` for each of them.
        let parameter: Vec<Value<'_>> = Vec::new();
        proxy
            .call_method(
                "ActivateAction",
                &(action.gapplication_action(), parameter, platform),
            )
            .await
            .map_err(|e| ActivationError::Failed(describe_bus_error(&e)))?;
        Ok(())
    }
}

/// Whether an activation token is worth forwarding.
///
/// Tokens are short, opaque, printable handles. This is not a security
/// boundary — GTK validates the token with the compositor, and a bad one
/// costs a lost focus rather than anything worse — it is a refusal to
/// forward something that is obviously not a token.
fn usable_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 512
        && token
            .chars()
            .all(|c| c.is_ascii_graphic() || c == ' ' || c == '-' || c == '_')
}

/// A D-Bus error, reduced to something safe to log.
///
/// The error *name* and nothing else. A zbus error's `Display` includes the
/// message the far end wrote, and the far end here is another process on the
/// session bus.
pub(super) fn describe_bus_error(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(name, _, _) => name.as_str().to_string(),
        zbus::Error::NameTaken => "name taken".into(),
        zbus::Error::InterfaceNotFound => "interface not found".into(),
        zbus::Error::InputOutput(_) => "i/o error".into(),
        zbus::Error::Address(_) => "no usable bus address".into(),
        _ => "bus error".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_path_is_the_application_id_mechanically() {
        // The rule from the D-Bus Application specification, applied here
        // rather than trusted from the constant.
        let derived = format!("/{}", DESKTOP_APP_ID.replace('.', "/"));
        assert_eq!(derived, DESKTOP_APP_OBJECT_PATH);
    }

    #[test]
    fn an_obviously_wrong_activation_token_is_not_forwarded() {
        assert!(usable_token("gnome-shell-1234-omnibridge-TOKEN_abc"));
        assert!(!usable_token(""));
        assert!(!usable_token(&"x".repeat(513)));
        assert!(!usable_token("has\nnewline"));
        assert!(!usable_token("has\0nul"));
    }
}
