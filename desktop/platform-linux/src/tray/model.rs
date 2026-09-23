//! The tray's model: identity, menu, and the closed set of things a click can
//! ask for.
//!
//! Nothing in this file talks to D-Bus, GTK or the desktop. It is the half of
//! the StatusNotifierItem that can be reasoned about — and tested — without a
//! bus, and the half that decides what a message from the shell is *allowed*
//! to mean.
//!
//! # Why a closed enum and not a string
//!
//! The shell sends a menu item id (`i`) and an event name (`s`). The GUI is
//! driven by `GAction` names on the session bus. The obvious implementation
//! carries a `String` from the first to the second, and the obvious
//! implementation is wrong: it makes every action OmniBridge's GUI will ever
//! export reachable from a D-Bus message, including ones added later by
//! someone who never thought about the tray. [`TrayAction`] has three
//! variants, no payload, and one function that turns a variant into a name.
//! The name is a `&'static str` in this file; it cannot come from a message.

/// The desktop application's identity on the session bus.
///
/// The same string as `APP_ID` in `omnibridge-gui`, the desktop entry's
/// basename, its `Icon=` key, the D-Bus service file's `Name=` and this
/// crate's [`ICON_NAME`]. `tray_identity.rs` reads the repository's own files
/// and asserts every one of those copies agrees, so the tray cannot be the
/// place the identity drifts.
pub const DESKTOP_APP_ID: &str = "io.github.yurisismotto.omnibridge";

/// Where `org.freedesktop.Application` lives for [`DESKTOP_APP_ID`].
///
/// Mechanically the application id with `.` replaced by `/` and a leading
/// slash — the rule in the D-Bus Application specification, which GApplication
/// implements. Written out rather than computed so that it is greppable, and
/// checked against the rule by a test so that writing it out cannot make it
/// wrong.
pub const DESKTOP_APP_OBJECT_PATH: &str = "/io/github/yurisismotto/omnibridge";

/// The icon the shell is asked to draw.
///
/// A *theme name*, never a path. The icon it resolves to is the one
/// `install-desktop-metadata.sh` puts in `hicolor`, which is the canonical
/// the OmniBridge mark from `docs/design/assets/omnibridge-app-icon.svg`. A tray
/// item that carried an
/// absolute path would be naming this checkout, and one that carried pixels
/// would be a second copy of the mark that the brand documentation does not
/// know about.
pub const ICON_NAME: &str = DESKTOP_APP_ID;

/// The item's persistent identity, as the tray host stores it.
///
/// Plasma's system tray uses `Id` as the configuration key for whether the
/// user has pinned or hidden an item (`systemtraymodel.cpp` reads it straight
/// into `m_shownItems` / `m_hiddenItems`), so it has to be the same string on
/// the next login — and it has to be ours alone, because a collision means
/// inheriting somebody else's "hidden".
pub const ITEM_ID: &str = DESKTOP_APP_ID;

/// The item's human-readable name.
///
/// The product name, because this is what a person reads in a tooltip or in
/// the tray's own configuration list. Not the application id: that is an
/// identifier, and showing an identifier to a person is a category error.
pub const ITEM_TITLE: &str = "OmniBridge";

/// `ApplicationStatus`, from the category enum in KDE's
/// `kstatusnotifieritem.h`.
///
/// The categories are `ApplicationStatus`, `Communications`,
/// `SystemServices`, `Hardware` and `Reserved`. `SystemServices` is tempting
/// — `omnibridged` genuinely is a background service — but the category
/// describes *the icon*, and this icon is the entry point to an application
/// with windows, which is precisely what `ApplicationStatus` is documented as
/// ("an icon for a normal application, can be seen as its taskbar entry").
pub const ITEM_CATEGORY: &str = "ApplicationStatus";

/// `Active` — the only status this version ever reports.
///
/// The enum has three values and two of them would be untrue here:
///
/// * **`NeedsAttention`** is for an item that wants the user *now*; shells
///   respond by unhiding it and, on Plasma, animating it. OmniBridge's phone
///   being asleep, off the network or not paired at all is not an emergency,
///   and a tray icon that demands attention because a device is offline is
///   the behaviour this product exists not to have. Nothing in v1 sets it.
/// * **`Passive`** means "not important enough to show", which shells take as
///   permission to hide the item entirely. A running OmniBridge is a running
///   service the user can reach; it is not a thing to hide. There is no state
///   in v1 where `Passive` would be the truthful answer, so it is never sent.
///
/// The daemon owns the item, and the daemon's own liveness is the only thing
/// the item's presence claims. It is claimed by the item existing at all,
/// which is why `Active` is a constant here rather than a reading of anything.
pub const ITEM_STATUS: &str = "Active";

/// The tooltip title.
pub const TOOLTIP_TITLE: &str = "OmniBridge";

/// The tooltip's second line.
///
/// The product's tagline, and deliberately nothing else. What a tooltip could
/// usefully say — "connected to Yuri's phone", "sending holiday-photos.zip" —
/// is exactly what must not be here: the `ToolTip` property is a public
/// property of a public object on the session bus, readable by every process
/// in the session, at any time, without a click. The tray is the one OmniBridge
/// surface whose contents leave the process without anyone asking, so the
/// only safe contents are the ones that were already public.
pub const TOOLTIP_BODY: &str = "One bridge. Any device.";

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// What the tray is allowed to ask the desktop application to do.
///
/// Three variants, no fields. Adding a fourth is a deliberate edit to this
/// file with a `&'static str` beside it; it cannot happen by a message
/// arriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayAction {
    /// Present the Quick Panel — `app.quick-panel`.
    QuickPanel,
    /// Present Settings on its Transfers page — `app.transfers`.
    ///
    /// Named `Files` here and `transfers` there because those are the two
    /// surfaces' own names: the menu entry a person reads says "Files", the
    /// action the GUI has exported since the Quick Panel sprint is
    /// `transfers`. The mapping is this enum's job precisely so that neither
    /// name has to change to suit the other.
    Files,
    /// Present the Settings window — `app.settings`.
    Settings,
}

impl TrayAction {
    /// Every variant, for exhaustive tests and for the menu.
    pub const ALL: [TrayAction; 3] = [
        TrayAction::QuickPanel,
        TrayAction::Files,
        TrayAction::Settings,
    ];

    /// The `GAction` name on `org.gtk.Actions` / `org.freedesktop.Application`.
    ///
    /// These three names are asserted against `omnibridge-gui`'s own public
    /// constants by `tray_identity.rs`; the GUI cannot rename one without
    /// this crate's tests going red.
    pub const fn gapplication_action(self) -> &'static str {
        match self {
            TrayAction::QuickPanel => "quick-panel",
            TrayAction::Files => "transfers",
            TrayAction::Settings => "settings",
        }
    }
}

/// What a left click means.
///
/// One thing, always. The Quick Panel is the everyday surface — that is what
/// it was built for — and a tray icon whose primary click did something
/// different depending on state would be a tray icon nobody could learn.
pub const fn primary_activation() -> TrayAction {
    TrayAction::QuickPanel
}

// ---------------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------------

/// The DBusMenu root.
///
/// Zero, by the specification: `GetLayout(0, …)` is how a host asks for the
/// whole menu, and `com.canonical.dbusmenu` reserves that id for the root.
pub const MENU_ROOT_ID: i32 = 0;

/// One row of the tray menu.
pub struct MenuEntry {
    /// The DBusMenu id. A literal, not a position.
    pub id: i32,
    /// What the person reads.
    pub label: &'static str,
    /// What choosing it does.
    pub action: TrayAction,
}

/// The whole menu, in display order.
///
/// **The ids are literals and they are stable.** Deriving an id from a
/// position in this array would compile, would work, and would silently
/// rewire every id the moment somebody inserted a row — while a shell that
/// had already fetched the layout went on sending the old ones. An id is an
/// identity, so it is written down once and never computed.
///
/// Three entries, matching the three surfaces the GUI already exports. What
/// is deliberately absent is everything that carries authority: pairing,
/// granting, revoking, sending a file, sending the clipboard. Those decide
/// who may read this machine, and a decision like that belongs on a surface
/// where the person can see what they are deciding about — not two clicks
/// deep in a menu that is drawn by another process. "Quit OmniBridge" is absent
/// for a different reason: the only thing it could honestly quit is
/// `omnibridged`, and stopping the continuity service from a tray menu is not
/// closing a window, it is turning the product off.
pub const MENU: &[MenuEntry] = &[
    MenuEntry {
        id: 1,
        label: "Quick Panel",
        action: TrayAction::QuickPanel,
    },
    MenuEntry {
        id: 2,
        label: "Files",
        action: TrayAction::Files,
    },
    MenuEntry {
        id: 3,
        label: "Settings",
        action: TrayAction::Settings,
    },
];

/// The action a menu id stands for, or `None` if no such row exists.
///
/// `None` is the answer for the root, for a negative id, and for anything a
/// host invents. It is not an error and nothing is logged: a host asking
/// about an id we do not have is a host asking a question, and the answer is
/// simply that there is nothing there.
pub fn action_for_menu_id(id: i32) -> Option<TrayAction> {
    MENU.iter().find(|e| e.id == id).map(|e| e.action)
}

/// The row an id stands for.
pub fn entry_for_menu_id(id: i32) -> Option<&'static MenuEntry> {
    MENU.iter().find(|e| e.id == id)
}

/// The DBusMenu event that counts as "the person chose this".
///
/// The specification lists `clicked` and `hovered`, and lets a host invent
/// `x-vendor-` events of its own; Plasma's importer additionally sends
/// `opened` and `closed` when a menu is shown and hidden. Exactly one of
/// those is a decision, so exactly one of them does anything here.
pub const ACTIVATION_EVENT: &str = "clicked";

/// What a `com.canonical.dbusmenu` `Event` should do.
///
/// Both halves are checked, and the event type is checked *first*, because
/// the failure that matters is not an unknown id — it is a known id arriving
/// with `hovered` and opening a window because nobody looked at the verb.
pub fn action_for_menu_event(id: i32, event: &str) -> Option<TrayAction> {
    if event != ACTIVATION_EVENT {
        return None;
    }
    action_for_menu_id(id)
}
