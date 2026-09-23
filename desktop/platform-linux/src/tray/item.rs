//! `org.kde.StatusNotifierItem`, served on the session bus.
//!
//! # Where the contract came from
//!
//! Not from memory. The interface below is the one declared in
//! `org.kde.StatusNotifierItem.xml`, which ships in KDE's
//! `kstatusnotifieritem` framework and is the file both KDE's own client and
//! Plasma's tray are generated from. Every property name, every signature and
//! every method argument here was read out of that file, and the surrounding
//! behaviour — the object path, the bus name shape, what the watcher does
//! with a registration — out of `kstatusnotifieritemdbus_p.cpp`,
//! `kstatusnotifieritem.cpp` and Plasma's own
//! `statusnotifierwatcher.cpp`. `KDE-STATUSNOTIFIER-V1.md` §8 records the
//! whole derivation.
//!
//! # The item is served, then named, then registered
//!
//! That order is load-bearing, and it is Plasma's code that makes it so.
//! `StatusNotifierWatcher::RegisterStatusNotifierItem` does not take the
//! caller's word for it: it builds a proxy at the service's
//! `/StatusNotifierItem` and calls `isValid()`, which introspects. An item
//! that requests its bus name before it has exported the object can be asked
//! that question in the window between the two, and the honest answer at that
//! moment is "there is nothing here" — after which the watcher drops the
//! registration and nothing appears in the tray.

use std::sync::Arc;
use std::sync::Mutex;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use super::activate::ApplicationActivator;
use super::model::{
    self, TrayAction, ICON_NAME, ITEM_CATEGORY, ITEM_ID, ITEM_STATUS, ITEM_TITLE, TOOLTIP_BODY,
    TOOLTIP_TITLE,
};

/// The interface name, as the specification spells it.
pub const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";

/// Where the item object lives.
///
/// `/StatusNotifierItem`, which is what the watcher assumes when a
/// registration names a bus name rather than a path. Hard-coded in KDE's
/// adaptor and in Plasma's watcher alike.
pub const ITEM_OBJECT_PATH: &str = "/StatusNotifierItem";

/// Where the item's menu lives.
///
/// `/MenuBar`, the path KDE's own item exports its `DBusMenuExporter` at. The
/// name is a historical accident of the menu protocol's origin in exported
/// application menu bars; nothing about it is a menu bar here.
pub const MENU_OBJECT_PATH: &str = "/MenuBar";

/// A pixmap array, `a(iiay)`: width, height, ARGB32 rows.
///
/// Always empty. See [`StatusNotifierItem::icon_pixmap`].
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

/// The tooltip struct, `(sa(iiay)ss)`: icon name, icon data, title, body.
type ToolTip = (String, Pixmaps, String, String);

/// OmniBridge's tray item.
pub struct StatusNotifierItem {
    activator: Arc<dyn ApplicationActivator>,
    /// The most recent XDG activation token the shell offered.
    ///
    /// Taken, not read: a token authorises one activation, and holding on to
    /// a spent one would mean handing the compositor a stale credential on the
    /// next click and being refused focus for a reason nobody could see.
    activation_token: Mutex<Option<String>>,
}

impl StatusNotifierItem {
    /// Builds the item.
    pub fn new(activator: Arc<dyn ApplicationActivator>) -> Self {
        Self {
            activator,
            activation_token: Mutex::new(None),
        }
    }

    /// Runs an action without making the caller wait for it.
    ///
    /// The shell's `Activate` call gets its reply immediately and the window
    /// opens when it opens. That matters in the case this sprint cares most
    /// about: a *cold* activation starts a process, initialises GTK and builds
    /// a window, and doing that inside a method handler would hold the reply —
    /// and this connection's dispatch — for as long as it took.
    ///
    /// A failure here is logged and forgotten. The item stays registered, the
    /// menu stays exactly as it was, and the next click tries again: nothing
    /// about the tray's state depends on an activation having worked, which is
    /// what makes "the GUI could not be started" a non-event rather than a
    /// broken tray.
    fn dispatch(&self, action: TrayAction) {
        let activator = Arc::clone(&self.activator);
        let token = self
            .activation_token
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        tokio::spawn(async move {
            if let Err(e) = activator.activate(action, token).await {
                // The action is one of three compile-time constants and the
                // reason is a D-Bus error name. No token, no window, no peer,
                // no path.
                tracing::info!(
                    action = action.gapplication_action(),
                    reason = %e,
                    "could not present the OmniBridge window the tray asked for"
                );
            }
        });
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    // ---- identity ---------------------------------------------------------

    #[zbus(property)]
    fn category(&self) -> &str {
        ITEM_CATEGORY
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        ITEM_ID
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        ITEM_TITLE
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        ITEM_STATUS
    }

    /// Zero: this item belongs to a daemon, which has no window.
    ///
    /// `WindowId` exists so a shell can do window-manager things to the
    /// application's main window — raise it, or fall back to toggling it when
    /// the item offers no other behaviour. There is no such window here, and
    /// claiming an id would be pointing the shell at somebody else's.
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    // ---- the icon ---------------------------------------------------------

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ICON_NAME
    }

    /// Empty, deliberately.
    ///
    /// `IconName` and `IconPixmap` are alternatives and the specification
    /// prefers the name: the shell then resolves it through the icon theme,
    /// at the size and scale it is actually drawing, honouring the user's
    /// theme — which is how the Flow A ends up crisp on a 200% display
    /// without OmniBridge knowing anything about the display. Sending pixels
    /// would be sending a second copy of the mark, rasterised at a size
    /// guessed by the sender, that the brand documentation does not know
    /// exists. If a real KDE session ever proves the name does not resolve
    /// *with the metadata correctly installed*, that is a reason to revisit
    /// this; a suspicion is not.
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }

    /// Empty: nothing is overlaid on the mark.
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }

    /// Empty, and it stays empty while [`ITEM_STATUS`] is `Active`.
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }

    /// Empty. An animation is a demand for attention with extra steps.
    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }

    /// Empty: the icon is found in the session's own theme, not in a
    /// directory this process names.
    ///
    /// `IconThemePath` is how an application that ships icons outside the
    /// theme tells the shell where to look, and it is a path out of this
    /// process into a shell — which on a Flatpak or a development checkout is
    /// a path that means nothing on the other side. OmniBridge installs its icon
    /// into `hicolor` like any other application, so there is nothing to
    /// point at.
    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }

    // ---- the menu ---------------------------------------------------------

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        // The path is a compile-time constant that `menu_object_path_is_valid`
        // parses in a test, so the `unwrap_or_default` below is unreachable
        // rather than a fallback anybody relies on. It is written this way
        // because a panic inside a property getter would take down the
        // connection, and a tray is not worth that.
        ObjectPath::try_from(MENU_OBJECT_PATH)
            .map(OwnedObjectPath::from)
            .unwrap_or_default()
    }

    /// False: the icon is a button first and a menu second.
    ///
    /// `ItemIsMenu = true` tells the shell that this item has no meaningful
    /// primary action and that a left click should just open the menu. OmniBridge
    /// has a meaningful primary action — the Quick Panel — so the left click
    /// belongs to it and the menu stays on the secondary button.
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    // ---- the tooltip ------------------------------------------------------

    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
        (
            // No separate tooltip icon: the shell already has the item's icon
            // and drawing a second one would be decoration.
            String::new(),
            Vec::new(),
            TOOLTIP_TITLE.to_string(),
            TOOLTIP_BODY.to_string(),
        )
    }

    // ---- interaction ------------------------------------------------------

    /// The compositor token for the click that is about to arrive.
    ///
    /// Plasma sends this immediately before `Activate` or `SecondaryActivate`
    /// on a Wayland session. Storing it and handing it to GApplication is what
    /// makes the window it opens actually come to the front.
    fn provide_xdg_activation_token(&self, token: String) {
        if let Ok(mut slot) = self.activation_token.lock() {
            *slot = Some(token);
        }
    }

    /// A left click.
    ///
    /// The coordinates are where the icon is on screen. They are ignored: the
    /// GUI decides where its own windows go, and a daemon telling a window
    /// manager where to put a window is how an application ends up placing
    /// itself wrongly on the one monitor arrangement nobody tested.
    fn activate(&self, _x: i32, _y: i32) {
        self.dispatch(model::primary_activation());
    }

    /// A middle click. Deliberately nothing.
    ///
    /// The specification does not say what this means, and shells disagree:
    /// some applications toggle, some mute, some open a second surface. The
    /// honest options were "open Settings" and "do nothing", and choosing the
    /// first would mean shipping an interaction that no real KDE session has
    /// ever run — see the physical-certification gate, which is blocked. A
    /// no-op is the one behaviour that cannot be wrong, and it is recorded as
    /// a debt rather than a decision that is finished.
    fn secondary_activate(&self, _x: i32, _y: i32) {}

    /// A right click, on a host that asks the item rather than drawing the
    /// menu itself. Deliberately nothing.
    ///
    /// Plasma does not use this: it reads the `Menu` property and draws the
    /// DBusMenu at `/MenuBar` from its own process. The method exists because
    /// the interface declares it, and doing nothing is right because the only
    /// alternative — an item popping up a window of its own where it guesses
    /// the pointer is — needs a GUI toolkit in the daemon, which is the thing
    /// this architecture exists to avoid.
    fn context_menu(&self, _x: i32, _y: i32) {}

    /// A scroll over the icon. Deliberately nothing.
    ///
    /// A wheel event is something a pointer does on the way past. Applications
    /// that bind it bind it to a volume or a workspace — a small, reversible,
    /// continuous quantity. OmniBridge has no such quantity: everything it could
    /// change is a device, a grant or a transfer, and none of those should
    /// ever be altered by a gesture the person may not have known they made.
    fn scroll(&self, _delta: i32, _orientation: String) {}

    // ---- change signals ---------------------------------------------------
    //
    // Declared because the interface declares them, and never emitted because
    // nothing they describe changes: the id, title, icon, menu, tooltip and
    // status are all compile-time constants in `model.rs`. A shell that
    // connects to them — Plasma connects to all of them — simply never hears
    // one, which is the truth. The day any of those stops being a constant is
    // the day the matching signal has to start being emitted, and having the
    // declaration already here is what makes that a one-line change.

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_attention_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_overlay_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_menu(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_object_path_is_valid() {
        assert!(ObjectPath::try_from(MENU_OBJECT_PATH).is_ok());
        assert!(ObjectPath::try_from(ITEM_OBJECT_PATH).is_ok());
    }
}
