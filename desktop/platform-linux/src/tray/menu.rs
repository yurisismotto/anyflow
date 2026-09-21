//! `com.canonical.dbusmenu`, the menu behind the tray icon.
//!
//! # Where the contract came from
//!
//! Plasma does not draw a menu an application pops up; it fetches a
//! description of one and draws it itself. The description protocol is
//! `com.canonical.dbusmenu`, and the version of it that matters is the one
//! Plasma's own importer speaks. That was read, not remembered:
//! `plasma-workspace/libdbusmenuqt/com.canonical.dbusmenu.xml` declares
//! exactly two properties (`Version`, `Status`), five methods (`Event`,
//! `GetProperty`, `GetLayout`, `GetGroupProperties`, `AboutToShow`) and three
//! signals (`ItemsPropertiesUpdated`, `LayoutUpdated`,
//! `ItemActivationRequested`), and `dbusmenuimporter.cpp` shows which of them
//! it actually calls:
//!
//! ```text
//! GetLayout(id, 1, [])                      to fetch a level
//! AboutToShow(id)                           before showing one
//! Event(id, "clicked"|"opened"|"closed", …) on interaction
//! ```
//!
//! This file implements that interface and nothing beyond it. `EventGroup` and
//! `AboutToShowGroup` exist in later revisions of the protocol elsewhere; they
//! are not in the file Plasma generates from and no caller here would use
//! them, so implementing them would be implementing a guess.
//!
//! # Why the menu is a constant
//!
//! It has three rows that never change, never grey out and never depend on
//! state. That is what makes `LayoutUpdated` a signal this file declares and
//! never emits, `AboutToShow` a function that always answers "no update
//! needed", and the revision a fixed number. A menu that reported a device's
//! name, a transfer in progress or a connection state would be a menu that
//! changed — and, worse, a menu whose contents were readable off the session
//! bus by anything that cared to call `GetLayout`.

use std::collections::HashMap;
use std::sync::Arc;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Structure, Value};

use super::activate::ApplicationActivator;
use super::model::{self, TrayAction, MENU, MENU_ROOT_ID};

/// The interface name.
pub const MENU_INTERFACE: &str = "com.canonical.dbusmenu";

/// The protocol revision this implementation answers to.
///
/// Fixed. The layout is a constant, so there is no second revision to move
/// to; a host that caches the layout against this number is caching something
/// that cannot go stale.
const LAYOUT_REVISION: u32 = 1;

/// One item as `GetLayout` returns it: `(ia{sv}av)`.
type LayoutItem = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// One item as `GetGroupProperties` returns it: `(ia{sv})`.
type ItemProperties = (i32, HashMap<String, OwnedValue>);

/// OmniBridge's tray menu.
pub struct TrayMenu {
    activator: Arc<dyn ApplicationActivator>,
}

impl TrayMenu {
    /// Builds the menu.
    pub fn new(activator: Arc<dyn ApplicationActivator>) -> Self {
        Self { activator }
    }

    fn dispatch(&self, action: TrayAction) {
        let activator = Arc::clone(&self.activator);
        tokio::spawn(async move {
            // No activation token: a menu choice arrives through the menu
            // object, which the shell never offers a token on. The window
            // still opens; on Wayland it may not steal focus, which is the
            // correct outcome when nothing vouched for the click.
            if let Err(e) = activator.activate(action, None).await {
                tracing::info!(
                    action = action.gapplication_action(),
                    reason = %e,
                    "could not present the OmniBridge window the tray menu asked for"
                );
            }
        });
    }
}

/// The properties of the root item.
///
/// `children-display: submenu` is the one that matters: it is how a host is
/// told that this item has children worth fetching. Without it Plasma's
/// importer treats the root as a leaf and draws an empty menu.
fn root_properties() -> HashMap<String, OwnedValue> {
    let mut props = HashMap::new();
    insert(&mut props, "children-display", Value::from("submenu"));
    props
}

/// The properties of one real row.
fn entry_properties(entry: &model::MenuEntry) -> HashMap<String, OwnedValue> {
    let mut props = HashMap::new();
    insert(&mut props, "label", Value::from(entry.label));
    // Both stated rather than left to the host's default. Every row is always
    // available: each one presents a window, and presenting a window is
    // something OmniBridge can always do — if the GUI is not running, the bus
    // starts it. There is no state in which one of these should be greyed
    // out, and a row that greyed itself out would be a row telling the person
    // something about the daemon that the daemon has not been asked.
    insert(&mut props, "enabled", Value::from(true));
    insert(&mut props, "visible", Value::from(true));
    props
}

/// Adds a property, dropping it if it cannot be made owned.
///
/// `OwnedValue::try_from` fails only for values holding a file descriptor,
/// which nothing here constructs. Written as a fallible conversion anyway
/// because the alternative is an `expect` in a D-Bus method handler.
fn insert(props: &mut HashMap<String, OwnedValue>, key: &str, value: Value<'_>) {
    if let Ok(owned) = OwnedValue::try_from(value) {
        props.insert(key.to_string(), owned);
    }
}

/// Keeps only the properties a caller asked for.
///
/// An empty request means "all of them", which is what the specification says
/// and what Plasma's importer relies on — it calls `GetLayout` with an empty
/// list and filters afterwards.
fn filtered(
    mut props: HashMap<String, OwnedValue>,
    wanted: &[String],
) -> HashMap<String, OwnedValue> {
    if wanted.is_empty() {
        return props;
    }
    props.retain(|key, _| wanted.iter().any(|w| w == key));
    props
}

/// One row, with no children.
fn leaf(entry: &model::MenuEntry, wanted: &[String]) -> LayoutItem {
    (
        entry.id,
        filtered(entry_properties(entry), wanted),
        Vec::new(),
    )
}

/// The root, with as much of the menu below it as was asked for.
///
/// `recursion_depth` follows the specification: `-1` is "everything", `0` is
/// "this item only", and a positive number is that many levels. The menu is
/// one level deep, so anything but zero produces the same three rows.
fn root(recursion_depth: i32, wanted: &[String]) -> LayoutItem {
    let children = if recursion_depth == 0 {
        Vec::new()
    } else {
        MENU.iter()
            .filter_map(|entry| {
                let item = leaf(entry, wanted);
                OwnedValue::try_from(Value::from(Structure::from(item))).ok()
            })
            .collect()
    };
    (MENU_ROOT_ID, filtered(root_properties(), wanted), children)
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl TrayMenu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    /// `normal`, always.
    ///
    /// The other value the specification defines is `notice`, which asks the
    /// host to make the menu more prominent because something needs the
    /// person. That is the menu's version of `NeedsAttention`, and it is
    /// refused here for the same reason: nothing OmniBridge does is an
    /// interruption.
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    /// The layout, from `parent_id` down.
    ///
    /// An id that is not the root and not one of the three rows is an error
    /// rather than an empty answer — `InvalidArgs` is what libdbusmenu's own
    /// exporter returns, and a host that gets a plausible-looking empty menu
    /// back for a made-up id has been told something false.
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, LayoutItem)> {
        if parent_id == MENU_ROOT_ID {
            return Ok((LAYOUT_REVISION, root(recursion_depth, &property_names)));
        }
        match model::entry_for_menu_id(parent_id) {
            Some(entry) => Ok((LAYOUT_REVISION, leaf(entry, &property_names))),
            None => Err(zbus::fdo::Error::InvalidArgs(
                "no such menu item".to_string(),
            )),
        }
    }

    /// Properties for a set of ids.
    ///
    /// An empty `ids` means every item, root included, per the specification.
    /// Ids that do not exist are simply absent from the answer: this is a
    /// bulk query, and failing the whole call because one id in a batch was
    /// stale is how a host ends up with no menu at all after a race it could
    /// not have avoided.
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<ItemProperties> {
        let wanted = &property_names;
        let mut out = Vec::new();
        if ids.is_empty() || ids.contains(&MENU_ROOT_ID) {
            out.push((MENU_ROOT_ID, filtered(root_properties(), wanted)));
        }
        for entry in MENU {
            if ids.is_empty() || ids.contains(&entry.id) {
                out.push((entry.id, filtered(entry_properties(entry), wanted)));
            }
        }
        out
    }

    /// One property of one item.
    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let props = if id == MENU_ROOT_ID {
            root_properties()
        } else {
            match model::entry_for_menu_id(id) {
                Some(entry) => entry_properties(entry),
                None => {
                    return Err(zbus::fdo::Error::InvalidArgs(
                        "no such menu item".to_string(),
                    ))
                }
            }
        };
        props
            .get(&name)
            .and_then(|v| v.try_clone().ok())
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such property".to_string()))
    }

    /// Something happened to a menu item.
    ///
    /// Four kinds of message arrive here and one of them is a decision.
    /// `opened` and `closed` bracket the menu being shown; `hovered` is the
    /// pointer passing over a row. [`model::action_for_menu_event`] checks the
    /// verb before the id, and an id that does not name a row produces
    /// nothing at all — no action, no error, no log line. There is no path
    /// from this argument list to a `GAction` name: the `String` that arrives
    /// is compared against one constant and then dropped.
    ///
    /// `_data` and `_timestamp` are the specification's per-event payload and
    /// clock. Neither is read. The payload is a `v` from another process and
    /// nothing here has any use for one.
    fn event(
        &self,
        id: i32,
        event_id: String,
        _data: OwnedValue,
        _timestamp: u32,
    ) -> zbus::fdo::Result<()> {
        if let Some(action) = model::action_for_menu_event(id, &event_id) {
            self.dispatch(action);
        }
        Ok(())
    }

    /// The host is about to show the menu under `id`.
    ///
    /// Always `false`: nothing has changed, so nothing needs re-fetching. An
    /// implementation that answered `true` would make Plasma re-read a
    /// constant before every single right click.
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    // Declared because the interface declares them; never emitted because the
    // layout and the properties are compile-time constants.

    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &SignalEmitter<'_>,
        updated: Vec<ItemProperties>,
        removed: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn item_activation_requested(
        emitter: &SignalEmitter<'_>,
        id: i32,
        timestamp: u32,
    ) -> zbus::Result<()>;
}
