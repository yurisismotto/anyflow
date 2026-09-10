//! `org.freedesktop.Notifications`, over the session bus.
//!
//! # What was measured, and where
//!
//! Every behavioural claim below was checked against GNOME Shell 50.4 on
//! Fedora 44 during the research wave and again in N2
//! ([00 §2](../../../../../docs/research/notifications-v1/00-RESEARCH-FINDINGS.md)):
//!
//! ```console
//! $ gdbus call --session --dest org.freedesktop.Notifications \
//!       --object-path /org/freedesktop/Notifications \
//!       --method org.freedesktop.Notifications.GetServerInformation
//! ('gnome-shell', 'GNOME', '50.4', '1.2')
//!
//! $ … GetCapabilities
//! (['actions', 'body', 'body-markup', 'icon-static', 'persistence', 'sound'],)
//! ```
//!
//! Two of those matter here. **`body-markup`** means a body containing `<b>`
//! would be *parsed*, so every body is escaped before it is sent — the
//! escaping is [`crate::text`]'s and it happens above this module, because
//! whether to escape is this module's answer but how to escape is not a D-Bus
//! question. **`persistence`** means every notification lands in the GNOME
//! notification list and stays there until it is acknowledged, which is what
//! makes duplicate suppression a *visible* requirement rather than a tidy one:
//! a bug here produces a wall of stale entries the user clears by hand.
//!
//! # Three spec details this implementation is built around
//!
//! * **`replaces_id` is the update mechanism.** *"The server must atomically
//!   (ie with no flicker or other visual cues) replace the given notification
//!   with this one."* One remote identity maps to one freedesktop id for its
//!   whole life, and every update is a `Notify` with that id set. Measured:
//!   three calls with `replaces_id=5` returned `5` every time. The **returned**
//!   id is nevertheless what gets stored — see the note on the seam.
//! * **A closed id is dead.** *"The ID specified in the signal is invalidated
//!   before the signal is sent and may not be used in any further
//!   communications with the server."* So `NotificationClosed` must purge the
//!   mapping, or a later update creates a second notification instead of
//!   replacing the first.
//! * **`CloseNotification` on an unknown id.** The specification says the
//!   server answers with an error. GNOME answers with nothing at all — three
//!   closes of the same id produced one signal and no errors, HOST VERIFIED.
//!   Both are "the notification is gone", which is what was asked for, so this
//!   implementation reports **success** either way. Writing that down now is
//!   cheaper than debugging it on dunst, mako or KDE later.
//!
//! # What is deliberately not sent
//!
//! No actions — the `actions` array is always empty, so no button can ever
//! appear on a mirrored notification and there is nothing for an
//! `ActionInvoked` signal to be about. No `app_icon`: v1 carries no images.
//! No `image-data`, no `image-path`, no `sound-file`, no hyperlink. `urgency`
//! is 0 or 1 and never 2.
//!
//! One hint is sent, `desktop-entry`, naming AnyFlow's own application id. It
//! is how a future Linux *source* would recognise its own output and refuse to
//! mirror it back — the "recognise your own output" discipline applied on the
//! platform where it will be needed next.

use tokio::sync::mpsc;
use zbus::zvariant::Value;

use super::{Closed, Mirror, NotificationSink, ServerId, SinkCapabilities, SinkError, SinkResult};

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";

/// The application id AnyFlow posts under.
///
/// Matches the desktop file the packaging installs, so the shell shows the
/// right name and icon for a mirrored notification, and so a future Linux
/// source can recognise this process's own notifications by their
/// `desktop-entry` hint.
const APP_ID: &str = "io.github.yurisismotto.anyflow";

/// `-1` asks the server for its own default expiry.
///
/// Not `0` ("never expire"), which would leave every mirror on the screen
/// until it was clicked, and not a number of our own: how long a banner stays
/// up is a desktop-environment preference and overriding it would make AnyFlow
/// behave unlike every other application on the machine.
const EXPIRE_DEFAULT: i32 = -1;

/// The freedesktop notification server.
pub struct DbusSink {
    connection: zbus::Connection,
    capabilities: SinkCapabilities,
    server: String,
    closed_rx: std::sync::Mutex<Option<mpsc::Receiver<Closed>>>,
    availability_rx: std::sync::Mutex<Option<mpsc::Receiver<bool>>>,
}

impl DbusSink {
    /// Connects to the session bus and reads the server's own description.
    ///
    /// `None` when there is no session bus or no notification server. That is
    /// a normal, reportable state — a headless machine, a session without a
    /// shell — not an error, and not a reason for the daemon to fail to start.
    /// The capability is still registered; it simply announces no `SINK` role.
    pub async fn connect() -> Option<Self> {
        let connection = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                tracing::info!(
                    error = %ErrorClass(&e),
                    "no session bus; notifications.v1 has no sink on this session"
                );
                return None;
            }
        };

        let proxy = zbus::Proxy::new(&connection, DEST, PATH, IFACE)
            .await
            .ok()?;

        // `GetServerInformation` is the probe: it is the cheapest call that
        // fails when nothing owns the name, and its answer is what
        // `anyflow notifications status` reports.
        let (name, vendor, version, spec): (String, String, String, String) =
            match proxy.call("GetServerInformation", &()).await {
                Ok(info) => info,
                Err(e) => {
                    tracing::info!(
                        error = %ErrorClass(&e),
                        "no notification server on this session"
                    );
                    return None;
                }
            };

        let advertised: Vec<String> = proxy
            .call("GetCapabilities", &())
            .await
            .unwrap_or_else(|_| Vec::new());

        let capabilities = SinkCapabilities {
            body_markup: advertised.iter().any(|c| c == "body-markup"),
            body: advertised.iter().any(|c| c == "body"),
            persistence: advertised.iter().any(|c| c == "persistence"),
        };

        tracing::info!(
            server = %name,
            vendor = %vendor,
            version = %version,
            spec = %spec,
            body_markup = capabilities.body_markup,
            persistence = capabilities.persistence,
            "notification server"
        );

        let sink = Self {
            connection,
            capabilities,
            server: format!("{name} {version} (spec {spec}, {vendor})"),
            closed_rx: std::sync::Mutex::new(None),
            availability_rx: std::sync::Mutex::new(None),
        };
        sink.spawn_signal_pumps().await;
        Some(sink)
    }

    /// Subscribes to the two signals this sink acts on.
    ///
    /// Both are event-driven; neither polls. ADR-0014's standing rule applies
    /// to notifications exactly as it applies to the clipboard, and a periodic
    /// "is gnome-shell still there?" probe would be the thing it forbids.
    async fn spawn_signal_pumps(&self) {
        let (closed_tx, closed_rx) = mpsc::channel(64);
        let (availability_tx, availability_rx) = mpsc::channel(16);

        // `NotificationClosed(id, reason)`.
        if let Ok(proxy) = zbus::Proxy::new(&self.connection, DEST, PATH, IFACE).await {
            match proxy.receive_signal("NotificationClosed").await {
                Ok(mut stream) => {
                    tokio::spawn(async move {
                        use futures_lite::StreamExt as _;
                        while let Some(message) = stream.next().await {
                            let Ok((id, reason)) = message.body().deserialize::<(u32, u32)>()
                            else {
                                continue;
                            };
                            let closed = Closed {
                                id,
                                reason: super::CloseReason::from_wire(reason),
                            };
                            if closed_tx.send(closed).await.is_err() {
                                return;
                            }
                        }
                    });
                }
                Err(e) => tracing::info!(
                    error = %ErrorClass(&e),
                    "cannot observe notification closes on this session"
                ),
            }
        }

        // `NameOwnerChanged` for the notification server.
        //
        // This is what turns a GNOME Shell restart from a silent failure into
        // an observed event: the name loses its owner, the `SINK` role
        // narrows, every stale numeric id is dropped, and when the name is
        // acquired again the role widens with a strictly higher epoch. No
        // reconnect, no polling, and no persistence to survive the restart —
        // the phone is the thing that knows what is active, and asking it
        // again is free.
        let dbus = zbus::Proxy::new(
            &self.connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await;
        if let Ok(dbus) = dbus {
            match dbus.receive_signal("NameOwnerChanged").await {
                Ok(mut stream) => {
                    tokio::spawn(async move {
                        use futures_lite::StreamExt as _;
                        while let Some(message) = stream.next().await {
                            let Ok((name, _old, new)) =
                                message.body().deserialize::<(String, String, String)>()
                            else {
                                continue;
                            };
                            if name != DEST {
                                continue;
                            }
                            if availability_tx.send(!new.is_empty()).await.is_err() {
                                return;
                            }
                        }
                    });
                }
                Err(e) => tracing::info!(
                    error = %ErrorClass(&e),
                    "cannot observe notification server restarts on this session"
                ),
            }
        }

        if let Ok(mut guard) = self.closed_rx.lock() {
            *guard = Some(closed_rx);
        }
        if let Ok(mut guard) = self.availability_rx.lock() {
            *guard = Some(availability_rx);
        }
    }

    async fn proxy(&self) -> SinkResult<zbus::Proxy<'_>> {
        zbus::Proxy::new(&self.connection, DEST, PATH, IFACE)
            .await
            .map_err(|e| SinkError::Failed(ErrorClass(&e).to_string()))
    }
}

#[async_trait::async_trait]
impl NotificationSink for DbusSink {
    fn id(&self) -> &'static str {
        "freedesktop"
    }

    fn describe(&self) -> String {
        format!("org.freedesktop.Notifications — {}", self.server)
    }

    fn capabilities(&self) -> SinkCapabilities {
        self.capabilities.clone()
    }

    async fn availability(&self) -> SinkResult<()> {
        let proxy = self.proxy().await?;
        proxy
            .call::<_, _, (String, String, String, String)>("GetServerInformation", &())
            .await
            .map(|_| ())
            .map_err(|e| SinkError::Unavailable(ErrorClass(&e).to_string()))
    }

    async fn display(&self, mirror: &Mirror, replaces: Option<ServerId>) -> SinkResult<ServerId> {
        let proxy = self.proxy().await?;

        let mut hints: std::collections::HashMap<&str, Value<'_>> =
            std::collections::HashMap::new();
        hints.insert("urgency", Value::U8(mirror.urgency.as_hint()));
        hints.insert("desktop-entry", Value::new(APP_ID));

        // The `actions` array is empty and always will be. `notifications.v1`
        // carries no action, so there is nothing here to name — and a local
        // button that triggered a remote effect is precisely the line between
        // mirroring a notification and remotely controlling the device that
        // raised it.
        let actions: Vec<&str> = Vec::new();

        let id: u32 = proxy
            .call(
                "Notify",
                &(
                    mirror.app_name.as_str(),
                    replaces.unwrap_or(0),
                    // No icon. v1 carries no images and inventing one here
                    // would mean choosing an icon on an application's behalf.
                    "",
                    mirror.summary.as_str(),
                    mirror.body.as_str(),
                    actions,
                    hints,
                    EXPIRE_DEFAULT,
                ),
            )
            .await
            .map_err(classify)?;

        Ok(id)
    }

    async fn close(&self, id: ServerId) -> SinkResult<()> {
        let proxy = self.proxy().await?;
        match proxy.call::<_, _, ()>("CloseNotification", &(id,)).await {
            Ok(()) => Ok(()),
            Err(e) => match classify(e) {
                // "No such notification" is what the specification says a
                // conforming server answers, and the notification is gone
                // either way — which is exactly what the caller asked for. A
                // sink that reported this as a failure would log one on every
                // dismissal against a spec-literal server.
                SinkError::Failed(_) => Ok(()),
                // An absent server or a timeout is a different fact and is
                // reported, because it changes what this desktop can claim.
                other => Err(other),
            },
        }
    }

    fn closed_events(&self) -> Option<mpsc::Receiver<Closed>> {
        self.closed_rx.lock().ok()?.take()
    }

    fn availability_events(&self) -> Option<mpsc::Receiver<bool>> {
        self.availability_rx.lock().ok()?.take()
    }
}

/// Separates "there is no server" from "the server refused".
///
/// The distinction is load-bearing: the first narrows the `SINK` role for
/// every peer, and the second is one call going wrong.
fn classify(error: zbus::Error) -> SinkError {
    let class = ErrorClass(&error).to_string();
    match &error {
        zbus::Error::MethodError(name, _, _) => {
            let name = name.as_str();
            if name.contains("ServiceUnknown") || name.contains("NameHasNoOwner") {
                SinkError::Unavailable(class)
            } else {
                SinkError::Failed(class)
            }
        }
        zbus::Error::InputOutput(_) | zbus::Error::Address(_) => SinkError::Unavailable(class),
        _ => SinkError::Failed(class),
    }
}

/// Renders a `zbus::Error` as a **class**, never as its message.
///
/// A D-Bus error message is written by the notification server, and on some
/// servers it quotes the arguments of the call that failed — which for
/// `Notify` means the summary and the body. Printing `{e}` would put a
/// person's message into the daemon log through a path nobody would think to
/// audit. So the log gets the variant name, and the method-error name, and
/// nothing else.
struct ErrorClass<'a>(&'a zbus::Error);

impl std::fmt::Display for ErrorClass<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            zbus::Error::MethodError(name, _, _) => write!(f, "method-error:{}", name.as_str()),
            zbus::Error::InputOutput(_) => f.write_str("io"),
            zbus::Error::Address(_) => f.write_str("address"),
            zbus::Error::Handshake(_) => f.write_str("handshake"),
            zbus::Error::InvalidReply => f.write_str("invalid-reply"),
            zbus::Error::NameTaken => f.write_str("name-taken"),
            _ => f.write_str("other"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_renders_as_a_class_and_never_as_its_message() {
        // The message a server sends back is attacker-influenced text: on some
        // servers it quotes the call's arguments, which for `Notify` is
        // somebody's message.
        let rendered = ErrorClass(&zbus::Error::InvalidReply).to_string();
        assert_eq!(rendered, "invalid-reply");

        let io = zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other(
            "SECRET-NOTIFICATION-BODY",
        )));
        let rendered = ErrorClass(&io).to_string();
        assert_eq!(rendered, "io");
        assert!(!rendered.contains("SECRET"));
    }

    #[test]
    fn an_io_error_means_the_server_is_gone_rather_than_refusing() {
        let io =
            zbus::Error::InputOutput(std::sync::Arc::new(std::io::Error::other("broken pipe")));
        assert!(matches!(classify(io), SinkError::Unavailable(_)));
    }

    #[test]
    fn the_default_expiry_is_the_servers_own() {
        // Not zero, which means "never expire" and would leave every mirror on
        // the screen until it was clicked.
        assert_eq!(EXPIRE_DEFAULT, -1);
        assert_ne!(EXPIRE_DEFAULT, 0);
    }
}
