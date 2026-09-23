//! N1–N18 — the tray against a host that behaves like GNOME's extension.
//!
//! # Why this file exists at all
//!
//! GNOME Shell has no tray. What it has is an extension —
//! `appindicatorsupport@rgcjonas.gmail.com`, "AppIndicator and
//! KStatusNotifierItem Support" — which owns
//! `org.kde.StatusNotifierWatcher` and draws the items that register with it.
//! So the question this sprint had to answer was not *"how do we build a
//! GNOME tray backend"* but *"is the StatusNotifierItem we already publish the
//! thing that extension consumes"*, and the answer is yes.
//!
//! That makes this file a **compatibility** suite, not a second
//! implementation. Nothing here tests a GNOME-only code path, because there
//! is none: `tray_dbus.rs` and this file drive the same
//! `org.kde.StatusNotifierItem` and the same `com.canonical.dbusmenu` through
//! two different hosts.
//!
//! # The host is modelled on the source, not on the README
//!
//! Every behaviour the fake below implements was read out of the extension's
//! own JavaScript, at the version Fedora 44 packages
//! (`gnome-shell-extension-appindicator-64`, upstream `v64`), and the file
//! that it came from is named at each one. The places where GNOME differs
//! from Plasma are the reason the fake is not simply `tray_dbus.rs`'s:
//!
//! | GNOME does | Plasma does | Where |
//! | --- | --- | --- |
//! | resolves the registered name to a **unique** name, then talks to that | talks to the name it was given | `dbusUtils.js:getUniqueBusName` |
//! | **introspects** the item to decide whether `Activate` exists | assumes it | `appIndicator.js:456` |
//! | fetches the menu in **two phases** | fetches it whole | `dbusMenu.js:354`, `:301` |
//! | never reads `ToolTip` — it is commented out of its XML | shows it | `interfaces-xml/StatusNotifierItem.xml:50` |
//! | never reads `ItemIsMenu` | decides the click from it | (absent from every `.js`) |
//! | never calls `ContextMenu` | never calls it either | (absent from every `.js`) |
//!
//! `GNOME-APPINDICATOR-V1.md` §5 carries the full table and the evidence.

#![cfg(feature = "tray")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use omnibridge_linux::tray::activate::{ActivationError, ApplicationActivator};
use omnibridge_linux::tray::item::{ITEM_OBJECT_PATH, MENU_OBJECT_PATH};
use omnibridge_linux::tray::model::TrayAction;
use omnibridge_linux::tray::{publish, PublishedItem};
use zbus::zvariant::{OwnedValue, Value};

mod common;
use common::{stays, until, TestBus};

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";

// ===========================================================================
// The extension's own helpers, reimplemented from its source
// ===========================================================================

/// `DBusUtils.BUS_ADDRESS_REGEX` — `dbusUtils.js:22`.
///
/// ```js
/// /([a-zA-Z0-9._-]+\.[a-zA-Z0-9.-]+)|(:[0-9]+\.[0-9]+)$/
/// ```
///
/// The `$` binds to the second alternative only, so the first is an
/// unanchored search for "something, a dot, something". That is the branch
/// OmniBridge's `org.kde.StatusNotifierItem-<pid>-<n>` takes, and it is written
/// out here rather than pulled in as a regex crate because the point is to
/// reproduce the extension's decision, not to acquire a dependency.
fn bus_address_regex_matches(name: &str) -> bool {
    let head = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-';
    let tail = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-';
    name.char_indices().any(|(i, c)| {
        c == '.'
            && name[..i].chars().next_back().is_some_and(head)
            && name[i + c.len_utf8()..].chars().next().is_some_and(tail)
    })
}

/// `Util.indicatorId` — `util.js:33`.
///
/// This is the extension's identity for an item, and therefore the thing that
/// decides whether a second registration is a second icon or the same one
/// again.
fn indicator_id(service: Option<&str>, bus_name: &str, object_path: &str) -> String {
    if let Some(service) = service {
        if service != bus_name && bus_address_regex_matches(service) {
            return service.to_string();
        }
    }
    format!("{bus_name}@{object_path}")
}

/// The body of one `<interface>` in an introspection document.
///
/// Used only on the item object, which has no child nodes — see
/// [`node_summary`] for why depth would otherwise matter.
fn interface_block<'a>(xml: &'a str, interface: &str) -> Option<&'a str> {
    let open = format!("<interface name=\"{interface}\">");
    let start = xml.find(&open)? + open.len();
    let rest = &xml[start..];
    let end = rest.find("</interface>")?;
    Some(&rest[..end])
}

/// What `interfaceInfo.lookup_method(name)` would answer — `appIndicator.js:456`.
fn declares_method(xml: &str, interface: &str, method: &str) -> bool {
    interface_block(xml, interface).is_some_and(|body| {
        body.contains(&format!("<method name=\"{method}\">"))
            || body.contains(&format!("<method name=\"{method}\"/>"))
    })
}

/// One node's **own** interfaces, and its **direct** children.
///
/// Depth matters, and getting it wrong is the kind of mistake that makes a
/// test pass for the wrong reason. `zbus` serves the entire subtree inline:
/// introspecting `/` returns a document that contains `/StatusNotifierItem`'s
/// interfaces nested inside it. A scan that searched that document as flat
/// text would conclude `/` implements `org.kde.StatusNotifierItem` and
/// register the item at the root path.
///
/// The extension does not make that mistake, because it hands the document to
/// `Gio.DBusNodeInfo.new_for_xml` and asks `lookup_interface`
/// (`dbusUtils.js:120`), which answers about that node alone. So this walks
/// the tags and counts depth, which is the same question asked the same way.
fn node_summary(xml: &str) -> (Vec<String>, Vec<String>) {
    let mut interfaces = Vec::new();
    let mut children = Vec::new();
    let mut depth: i32 = 0;
    let mut rest = xml;

    let name_of = |tag: &str| -> Option<String> {
        let needle = "name=\"";
        let start = tag.find(needle)? + needle.len();
        let end = tag[start..].find('"')?;
        Some(tag[start..start + end].to_string())
    };

    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        if tag.starts_with("node") {
            // Recorded before the descent, so that a child is attributed to
            // the node it hangs from rather than to itself.
            if depth == 1 {
                if let Some(name) = name_of(tag) {
                    children.push(name);
                }
            }
            if !tag.ends_with('/') {
                depth += 1;
            }
        } else if tag.starts_with("/node") {
            depth -= 1;
        } else if tag.starts_with("interface") && depth == 1 {
            if let Some(name) = name_of(tag) {
                interfaces.push(name);
            }
        }
        rest = &rest[end + 1..];
    }
    (interfaces, children)
}

async fn introspect(
    conn: &zbus::Connection,
    dest: &str,
    path: &str,
) -> Result<String, zbus::fdo::Error> {
    zbus::fdo::IntrospectableProxy::builder(conn)
        .destination(dest.to_string())?
        .path(path.to_string())?
        .build()
        .await?
        .introspect()
        .await
}

// ===========================================================================
// The fake GNOME host
// ===========================================================================

/// What the host learned about an item, by doing what the extension does.
#[derive(Clone, Debug, Default)]
struct ItemFacts {
    /// `AppIndicator.supportsActivation` — `appIndicator.js:456`.
    supports_activation: bool,
    /// `AppIndicator._hasAyatanaSecondaryActivate` — `appIndicator.js:457`.
    has_ayatana_secondary: bool,
    id: String,
    title: String,
    status: String,
    menu_path: String,
    icon_name: String,
    icon_theme_path: String,
    icon_pixmaps: usize,
}

impl ItemFacts {
    /// `AppIndicator._checkIfReady` — `appIndicator.js:480`.
    ///
    /// `hasNameOwner && this.id && this.menuPath`. An item with an empty `Id`
    /// or an empty `Menu` never becomes ready, and an item that never becomes
    /// ready never gets a panel icon.
    fn is_ready(&self) -> bool {
        !self.id.is_empty() && !self.menu_path.is_empty()
    }

    /// `IndicatorStatusIcon` visibility — `indicatorStatusIcon.js:321`.
    fn is_visible(&self) -> bool {
        self.status != "Passive"
    }
}

#[derive(Clone, Default)]
struct HostLog {
    /// Indicator ids, in registration order. The extension's `_items` map.
    items: Arc<Mutex<Vec<String>>>,
    /// What it learned about each.
    facts: Arc<Mutex<HashMap<String, ItemFacts>>>,
    /// Registrations that hit `_ensureItemRegistered`'s "reset instead" path.
    resets: Arc<Mutex<Vec<String>>>,
    /// Every method the host was called on, in order. Idle traffic shows up
    /// here or nowhere.
    calls: Arc<Mutex<Vec<String>>>,
}

impl HostLog {
    fn items(&self) -> Vec<String> {
        self.items.lock().expect("host log").clone()
    }
    fn count(&self) -> usize {
        self.items.lock().expect("host log").len()
    }
    fn resets(&self) -> usize {
        self.resets.lock().expect("host log").len()
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("host log").clone()
    }
    fn facts_for(&self, id: &str) -> ItemFacts {
        self.facts
            .lock()
            .expect("host log")
            .get(id)
            .cloned()
            .unwrap_or_else(|| panic!("the host never registered {id}"))
    }
}

struct FakeGnomeHost {
    log: HostLog,
    /// A second connection, used the way the extension's proxies are: to look
    /// at the item while the object serving this interface stays free to
    /// answer. Separate so a verification can never wait on itself.
    verifier: zbus::Connection,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl FakeGnomeHost {
    /// `StatusNotifierWatcher.RegisterStatusNotifierItemAsync` —
    /// `statusNotifierWatcher.js:207`, then `_ensureItemRegistered` (`:134`)
    /// and `_registerItem` (`:89`).
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.log
            .calls
            .lock()
            .expect("host log")
            .push("RegisterStatusNotifierItem".into());

        // The extension reads the argument two ways: a leading `/` is an
        // object path and the bus name comes from the sender; anything that
        // matches the address regex is a *well-known name*, which it then
        // resolves to the unique name before touching the item.
        let (bus_name, object_path) = if service.starts_with('/') {
            (
                header
                    .sender()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".into()),
                service.clone(),
            )
        } else if bus_address_regex_matches(&service) {
            let dbus = zbus::fdo::DBusProxy::new(&self.verifier)
                .await
                .map_err(|_| zbus::fdo::Error::Failed("no bus proxy".into()))?;
            let unique = dbus
                .get_name_owner(
                    service
                        .as_str()
                        .try_into()
                        .map_err(|_| zbus::fdo::Error::InvalidArgs("not a bus name".into()))?,
                )
                .await
                .map_err(|_| zbus::fdo::Error::InvalidArgs("no such name".into()))?;
            (unique.as_str().to_string(), ITEM_OBJECT_PATH.to_string())
        } else {
            // The extension answers `org.gnome.gjs.JSError.ValueError` here.
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "impossible to register an indicator for parameters '{service}'"
            )));
        };

        let id = indicator_id(Some(&service), &bus_name, &object_path);

        // `_ensureItemRegistered`: an id the host already holds is *reset*,
        // never added a second time. This is the reason a duplicate
        // registration cannot become a duplicate icon.
        if self.log.items().iter().any(|held| held == &id) {
            self.log.resets.lock().expect("host log").push(id);
            return Ok(());
        }

        // `new AppIndicator(...)`: introspect for feature detection, then read
        // the properties readiness depends on.
        let xml = introspect(&self.verifier, &bus_name, &object_path)
            .await
            .map_err(|_| zbus::fdo::Error::InvalidArgs("unreachable item".into()))?;

        let props = zbus::fdo::PropertiesProxy::builder(&self.verifier)
            .destination(bus_name.as_str())
            .map_err(|_| zbus::fdo::Error::InvalidArgs("destination".into()))?
            .path(object_path.as_str())
            .map_err(|_| zbus::fdo::Error::InvalidArgs("path".into()))?
            .build()
            .await
            .map_err(|_| zbus::fdo::Error::InvalidArgs("properties proxy".into()))?;
        let all = props
            .get_all(
                zbus::names::InterfaceName::try_from(ITEM_INTERFACE)
                    .map_err(|_| zbus::fdo::Error::Failed("interface name".into()))?,
            )
            .await
            .map_err(|_| zbus::fdo::Error::InvalidArgs("item has no properties".into()))?;

        let text = |key: &str| -> String {
            all.get(key)
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| String::try_from(v).ok())
                .unwrap_or_default()
        };
        let facts = ItemFacts {
            supports_activation: declares_method(&xml, ITEM_INTERFACE, "Activate"),
            has_ayatana_secondary: declares_method(
                &xml,
                ITEM_INTERFACE,
                "XAyatanaSecondaryActivate",
            ),
            id: text("Id"),
            title: text("Title"),
            status: text("Status"),
            menu_path: all
                .get("Menu")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v).ok())
                .map(|p| p.as_str().to_string())
                .unwrap_or_default(),
            icon_name: text("IconName"),
            icon_theme_path: text("IconThemePath"),
            icon_pixmaps: all
                .get("IconPixmap")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| Vec::<(i32, i32, Vec<u8>)>::try_from(v).ok())
                .map(|p| p.len())
                .unwrap_or_default(),
        };

        if !facts.is_ready() {
            return Err(zbus::fdo::Error::InvalidArgs(
                "impossible to get basic properties".into(),
            ));
        }

        self.log
            .facts
            .lock()
            .expect("host log")
            .insert(id.clone(), facts);
        self.log.items.lock().expect("host log").push(id);
        Ok(())
    }

    /// `RegisterStatusNotifierHostAsync` — the extension refuses this
    /// outright (`statusNotifierWatcher.js:262`).
    fn register_status_notifier_host(&self, _service: String) -> zbus::fdo::Result<()> {
        self.log
            .calls
            .lock()
            .expect("host log")
            .push("RegisterStatusNotifierHost".into());
        Err(zbus::fdo::Error::NotSupported(
            "Registering additional notification hosts is not supported".into(),
        ))
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.log.items()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

/// A host that owns the watcher name for as long as the connection lives.
async fn start_host(bus: &TestBus) -> (zbus::Connection, HostLog) {
    let log = HostLog::default();
    let verifier = bus.connect().await;
    let conn = bus.connect().await;
    conn.object_server()
        .at(
            WATCHER_PATH,
            FakeGnomeHost {
                log: log.clone(),
                verifier,
            },
        )
        .await
        .expect("serving the fake GNOME host");
    conn.request_name(WATCHER_NAME)
        .await
        .expect("owning the watcher name");
    (conn, log)
}

// ===========================================================================
// The item, and a recorder for what it decided
// ===========================================================================

#[derive(Clone, Default)]
struct Recorder {
    actions: Arc<Mutex<Vec<TrayAction>>>,
    tokens: Arc<Mutex<Vec<Option<String>>>>,
}

impl Recorder {
    fn actions(&self) -> Vec<TrayAction> {
        self.actions.lock().expect("recorder").clone()
    }
    fn tokens(&self) -> Vec<Option<String>> {
        self.tokens.lock().expect("recorder").clone()
    }
}

#[async_trait::async_trait]
impl ApplicationActivator for Recorder {
    async fn activate(
        &self,
        action: TrayAction,
        activation_token: Option<String>,
    ) -> Result<(), ActivationError> {
        self.actions.lock().expect("recorder").push(action);
        self.tokens.lock().expect("recorder").push(activation_token);
        Ok(())
    }
}

struct Item {
    bus_name: String,
    task: tokio::task::JoinHandle<()>,
    _connection: zbus::Connection,
}

impl Drop for Item {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start_item(bus: &TestBus, recorder: &Recorder) -> Item {
    let connection = bus.connect().await;
    let published: PublishedItem = publish(connection.clone(), Arc::new(recorder.clone()))
        .await
        .expect("publishing the item");
    let bus_name = published.bus_name().to_string();
    let task = tokio::spawn(async move {
        let _ = published.follow_shell().await;
    });
    Item {
        bus_name,
        task,
        _connection: connection,
    }
}

async fn proxy_onto<'a>(
    conn: &'a zbus::Connection,
    item: &Item,
    interface: &'a str,
    path: &'a str,
) -> zbus::Proxy<'a> {
    zbus::Proxy::new(conn, item.bus_name.clone(), path.to_string(), interface)
        .await
        .expect("a proxy onto the item")
}

// ===========================================================================
// N1–N5 — registration, as the extension performs it
// ===========================================================================

/// N1 — the host is already there when the daemon starts.
#[tokio::test(flavor = "multi_thread")]
async fn n1_a_host_that_is_already_there_registers_the_item_exactly_once() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;

    until("the item to register", || log.count() == 1).await;

    // The id the extension stores is the *well-known* name, because
    // `indicatorId` prefers a service that is not the unique name. That is
    // what makes the item's identity survive the daemon reconnecting.
    assert_eq!(log.items(), vec![item.bus_name.clone()]);
    stays("one item", Duration::from_millis(300), || log.count() == 1).await;
}

/// N2 — the host arrives after the daemon. The ordinary case on a session
/// where the extension is enabled later, or re-enabled after the lock screen.
#[tokio::test(flavor = "multi_thread")]
async fn n2_a_host_that_appears_later_gets_exactly_one_registration() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;

    let (_host, log) = start_host(&bus).await;
    until("the item to notice the new host", || log.count() == 1).await;
    assert_eq!(log.items(), vec![item.bus_name.clone()]);
    stays("one item", Duration::from_millis(300), || log.count() == 1).await;
}

/// N3 — the extension is disabled and enabled again.
///
/// `extension.js:61` unowns the name on `disable()` and `:87` takes it again
/// on `enable()`, which is one `NameOwnerChanged` each way. The item must
/// register with the new owner and must not register twice with it.
#[tokio::test(flavor = "multi_thread")]
async fn n3_a_host_that_is_replaced_is_registered_with_exactly_once() {
    let bus = TestBus::start();
    let (first_host, first) = start_host(&bus).await;
    let _item = start_item(&bus, &Recorder::default()).await;
    until("the first registration", || first.count() == 1).await;

    drop(first_host);
    let (_second_host, second) = start_host(&bus).await;
    until("the second registration", || second.count() == 1).await;
    stays(
        "one item on the new host",
        Duration::from_millis(300),
        || second.count() == 1,
    )
    .await;
}

/// N4 — the host goes away and the daemon does not.
#[tokio::test(flavor = "multi_thread")]
async fn n4_the_host_going_away_leaves_the_daemon_and_the_item_alone() {
    let bus = TestBus::start();
    let (host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    drop(host);

    // The item object stays exported and the name stays owned: a caller that
    // is not the host can still read it. Tearing the item down when the shell
    // blinked would mean rebuilding it under time pressure when it came back.
    let caller = bus.connect().await;
    let proxy = proxy_onto(&caller, &item, ITEM_INTERFACE, ITEM_OBJECT_PATH).await;
    let title: String = proxy.get_property("Title").await.expect("Title");
    assert_eq!(title, "OmniBridge");
}

/// N5 — a duplicate registration is not a duplicate icon.
///
/// Belt and braces: the daemon registers once per owner, *and* the extension
/// would collapse a second registration of the same id into a reset. This
/// asserts the second half, by sending the registration the daemon does not
/// send.
#[tokio::test(flavor = "multi_thread")]
async fn n5_a_repeated_registration_never_becomes_a_second_icon() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    let caller = bus.connect().await;
    let watcher = zbus::Proxy::new(&caller, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME)
        .await
        .expect("a proxy onto the host");
    for _ in 0..3 {
        watcher
            .call_method("RegisterStatusNotifierItem", &(item.bus_name.as_str(),))
            .await
            .expect("the host accepts a repeat");
    }

    assert_eq!(log.count(), 1, "a repeat registration added an icon");
    assert_eq!(log.resets(), 3, "the repeats did not take the reset path");
}

// ===========================================================================
// N6–N9 — what the extension reads before it draws anything
// ===========================================================================

/// N6 — the item satisfies `_checkIfReady`, and is not hidden by `Status`.
#[tokio::test(flavor = "multi_thread")]
async fn n6_the_item_is_ready_and_visible_by_the_extensions_own_rules() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    let facts = log.facts_for(&item.bus_name);
    assert!(facts.is_ready(), "the extension would never draw this item");
    assert_eq!(facts.id, "io.github.yurisismotto.omnibridge");
    assert_eq!(facts.menu_path, MENU_OBJECT_PATH);
    assert_eq!(facts.title, "OmniBridge");

    // `Passive` is the one status that makes the extension hide the icon.
    // OmniBridge's `Active` is not a decoration: it is the reason the icon is on
    // the panel at all.
    assert_eq!(facts.status, "Active");
    assert!(facts.is_visible());
}

/// N7 — the feature detection the extension performs by introspection.
///
/// Two answers come out of one `Introspect` call, and both of them decide
/// behaviour that no property expresses:
///
/// * `Activate` **present** is what makes a double click open the Quick
///   Panel. Without it `supportsActivation` is false and the extension turns
///   *every* left click into a menu.
/// * `XAyatanaSecondaryActivate` **absent** is what makes a middle click
///   arrive as plain `SecondaryActivate`.
#[tokio::test(flavor = "multi_thread")]
async fn n7_feature_detection_finds_activate_and_no_ayatana_secondary() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    let facts = log.facts_for(&item.bus_name);
    assert!(
        facts.supports_activation,
        "without Activate, GNOME would open the menu on every click"
    );
    assert!(
        !facts.has_ayatana_secondary,
        "OmniBridge does not implement the Ayatana variant, and must not appear to"
    );
}

/// N8 — the icon is a theme name, and that is the whole of it.
///
/// With `IconThemePath` empty the extension skips its own theme lookup and
/// hands the name to `new Gio.ThemedIcon({name})` (`appIndicator.js:1157`),
/// which is St resolving it through the session's icon theme — the `hicolor`
/// entry `install-desktop-metadata.sh` writes. Sending pixmaps instead would
/// be sending a second copy of the mark, rasterised at a size guessed by the
/// sender.
#[tokio::test(flavor = "multi_thread")]
async fn n8_the_icon_is_a_theme_name_with_no_path_and_no_pixels() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    let facts = log.facts_for(&item.bus_name);
    assert_eq!(facts.icon_name, "io.github.yurisismotto.omnibridge");
    assert_eq!(facts.icon_theme_path, "");
    assert_eq!(facts.icon_pixmaps, 0);
}

/// N9 — the item is discoverable by the extension's brute-force scan.
///
/// `tools/busAnalyzer.js` walks every name on the bus and introspects it from
/// `/` downwards looking for `org.kde.StatusNotifierItem`
/// (`dbusUtils.js:introspectBusObject`). It exists because some applications
/// never re-register when the extension is toggled. OmniBridge does re-register
/// — but if the object tree were not walkable from `/`, this fallback would
/// silently not cover it, so the walk is asserted rather than assumed.
///
/// The same test pins the other half: what the scan finds resolves to the id
/// the item **already** registered under, so the fallback cannot produce a
/// second icon.
#[tokio::test(flavor = "multi_thread")]
async fn n9_the_brute_force_scan_finds_the_item_and_not_a_second_one() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    let caller = bus.connect().await;
    let dbus = zbus::fdo::DBusProxy::new(&caller).await.expect("bus proxy");
    let unique = dbus
        .get_name_owner(item.bus_name.as_str().try_into().expect("a bus name"))
        .await
        .expect("the item's unique name")
        .as_str()
        .to_string();

    // The walk, exactly as `introspectBusObject` performs it: introspect a
    // path, ask whether *that* node implements the interface, then recurse
    // into its direct children.
    let mut found = Vec::new();
    let mut visited = Vec::new();
    let mut queue = vec!["/".to_string()];
    while let Some(path) = queue.pop() {
        let xml = introspect(&caller, &unique, &path)
            .await
            .expect("introspect");
        let (interfaces, children) = node_summary(&xml);
        visited.push(path.clone());
        if interfaces.iter().any(|name| name == ITEM_INTERFACE) {
            found.push(path.clone());
        }
        let base = if path == "/" { "" } else { path.as_str() };
        for child in children {
            queue.push(format!("{base}/{child}"));
        }
    }
    visited.sort();

    // The walk really did descend — otherwise "found it once" would also be
    // true of a walk that never left `/`.
    assert_eq!(
        visited,
        vec![
            "/".to_string(),
            MENU_OBJECT_PATH.to_string(),
            ITEM_OBJECT_PATH.to_string(),
        ],
        "the scan did not visit the whole object tree"
    );

    assert_eq!(
        found,
        vec![ITEM_OBJECT_PATH.to_string()],
        "the scan must find the item once, at the path the spec fixes"
    );

    // And what it found is already registered, under the same id.
    let rediscovered = indicator_id(Some(&item.bus_name), &unique, ITEM_OBJECT_PATH);
    assert!(
        log.items().contains(&rediscovered),
        "the fallback would have registered a second icon"
    );
}

// ===========================================================================
// N10–N13 — the menu, fetched and driven the way the extension does it
// ===========================================================================

async fn menu_proxy<'a>(conn: &'a zbus::Connection, item: &Item) -> zbus::Proxy<'a> {
    proxy_onto(conn, item, MENU_INTERFACE, MENU_OBJECT_PATH).await
}

type LayoutItem = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// N10 — the two-phase fetch.
///
/// Phase one asks for the shape only — `GetLayout(0, -1, ['type',
/// 'children-display'])`, `dbusMenu.js:354` — because the extension replaces
/// an item outright if its type changes, so it reads type before anything
/// else. Phase two asks for the rest by id, `GetGroupProperties(ids, [])`,
/// `dbusMenu.js:301`. A menu that only answered the whole-layout question
/// would draw as three blank rows.
#[tokio::test(flavor = "multi_thread")]
async fn n10_the_two_phase_menu_fetch_returns_the_three_rows() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    // ---- phase one -------------------------------------------------------
    let shape: (u32, LayoutItem) = menu
        .call(
            "GetLayout",
            &(0_i32, -1_i32, vec!["type", "children-display"]),
        )
        .await
        .expect("GetLayout");
    let (revision, (root_id, root_props, children)) = shape;
    assert_eq!(revision, 1, "the layout is a constant; so is its revision");
    assert_eq!(root_id, 0);

    // The one property that makes the extension descend at all:
    // `dbusMenu.js:590` treats an item as a submenu only if it says so.
    let children_display =
        String::try_from(root_props["children-display"].try_clone().expect("clone"))
            .expect("children-display");
    assert_eq!(children_display, "submenu");
    assert_eq!(children.len(), 3, "the extension would draw a short menu");

    // Phase one asked for two properties the rows do not have, and got
    // neither. That is the filter working, not the menu being empty.
    let ids: Vec<i32> = children
        .iter()
        .map(|child| {
            let item: LayoutItem =
                LayoutItem::try_from(child.try_clone().expect("clone")).expect("a child");
            assert!(
                item.1.is_empty(),
                "a row answered a property it was not asked for"
            );
            item.0
        })
        .collect();
    assert_eq!(ids, vec![1, 2, 3]);

    // ---- phase two -------------------------------------------------------
    let mut wanted = vec![0];
    wanted.extend(ids);
    let (props,): (Vec<(i32, HashMap<String, OwnedValue>)>,) = menu
        .call("GetGroupProperties", &(wanted, Vec::<String>::new()))
        .await
        .map(|p: Vec<(i32, HashMap<String, OwnedValue>)>| (p,))
        .expect("GetGroupProperties");

    let label = |id: i32| -> String {
        let row = props
            .iter()
            .find(|(row_id, _)| *row_id == id)
            .unwrap_or_else(|| panic!("no properties for {id}"));
        String::try_from(row.1["label"].try_clone().expect("clone")).expect("label")
    };
    assert_eq!(label(1), "Quick Panel");
    assert_eq!(label(2), "Files");
    assert_eq!(label(3), "Settings");
}

/// N11 — every property has the type the extension mandates.
///
/// `PropertyStore.MandatedTypes` (`dbusMenu.js:74`) drops a property whose
/// type does not match, silently, and falls back to `DefaultValues` — where
/// `label` is the empty string. A `label` sent as anything but `s` is
/// therefore not an error anywhere; it is three blank rows on the panel.
#[tokio::test(flavor = "multi_thread")]
async fn n11_every_menu_property_has_the_type_the_extension_mandates() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    let props: Vec<(i32, HashMap<String, OwnedValue>)> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .expect("GetGroupProperties");

    // The subset of `MandatedTypes` OmniBridge sends anything for.
    let mandated: HashMap<&str, &str> = [
        ("visible", "b"),
        ("enabled", "b"),
        ("label", "s"),
        ("type", "s"),
        ("children-display", "s"),
    ]
    .into_iter()
    .collect();

    let mut seen = 0;
    for (id, row) in &props {
        for (name, value) in row {
            let expected = mandated.get(name.as_str()).unwrap_or_else(|| {
                panic!("item {id} sends `{name}`, which the extension does not know")
            });
            assert_eq!(
                value.value_signature().to_string(),
                *expected,
                "item {id}'s `{name}` would be dropped by the extension"
            );
            seen += 1;
        }
    }
    assert_eq!(
        seen,
        1 + 3 * 3,
        "root + three rows of label/enabled/visible"
    );
}

/// N12 — the event sequence a menu choice actually produces.
///
/// `AboutToShow(0)`, then `opened` on the root, then `clicked` on the row,
/// then `closed` on the root — `dbusMenu.js:894`, `:963`, `:682`, `:966`.
/// Three of those four are not decisions, and exactly one surface opens.
#[tokio::test(flavor = "multi_thread")]
async fn n12_each_row_opens_the_surface_it_names_and_the_rest_do_nothing() {
    for (id, expected) in [
        (1, TrayAction::QuickPanel),
        (2, TrayAction::Files),
        (3, TrayAction::Settings),
    ] {
        let bus = TestBus::start();
        let recorder = Recorder::default();
        let item = start_item(&bus, &recorder).await;
        let caller = bus.connect().await;
        let menu = menu_proxy(&caller, &item).await;

        let needs_update: bool = menu
            .call("AboutToShow", &(0_i32))
            .await
            .expect("AboutToShow");
        assert!(
            !needs_update,
            "a constant menu asked the shell to re-read it"
        );

        // The extension passes `GLib.Variant.new_int32(0)` for an event with
        // no data (`dbusMenu.js:194`), and a real timestamp for `clicked`.
        let nothing = Value::from(0_i32);
        for (row, event, timestamp) in [
            (0_i32, "opened", 0_u32),
            (id, "clicked", 1_234_567_u32),
            (0_i32, "closed", 0_u32),
        ] {
            menu.call_method("Event", &(row, event, &nothing, timestamp))
                .await
                .expect("Event");
        }

        until("the surface to be asked for", || {
            !recorder.actions().is_empty()
        })
        .await;
        assert_eq!(
            recorder.actions(),
            vec![expected],
            "menu id {id} opened the wrong surface, or more than one"
        );
    }
}

/// N13 — opening and closing the menu without choosing opens nothing.
///
/// The `opened`/`closed` pair brackets every single right click, and a
/// person who looks at the menu and moves on must not have started anything.
#[tokio::test(flavor = "multi_thread")]
async fn n13_looking_at_the_menu_and_closing_it_opens_no_window() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    let nothing = Value::from(0_i32);
    for (row, event) in [(0_i32, "opened"), (1, "hovered"), (0, "closed")] {
        menu.call_method("Event", &(row, event, &nothing, 0_u32))
            .await
            .expect("Event");
    }

    stays("nothing opened", Duration::from_millis(200), || {
        recorder.actions().is_empty()
    })
    .await;
}

// ===========================================================================
// N14–N16 — the interactions GNOME sends that OmniBridge deliberately ignores
// ===========================================================================

/// N14 — a middle click, and the deliberate no-op behind it.
///
/// `indicatorStatusIcon.js:423` turns `BUTTON_MIDDLE` into
/// `AppIndicator.secondaryActivate`, which offers an activation token
/// (`appIndicator.js:820`) and then — the Ayatana variant being absent —
/// calls plain `SecondaryActivate`. OmniBridge does nothing with it, and this
/// test is what makes that a decision rather than an omission: the KDE sprint
/// left it a no-op for want of a real host, and a real host's source now says
/// the event arrives. It stays a no-op because on GNOME every surface is
/// already one click (the menu) or two (Activate) away, so a third route to
/// one of them would be a gesture nobody could learn and nobody could undo.
#[tokio::test(flavor = "multi_thread")]
async fn n14_a_middle_click_provides_a_token_and_then_does_nothing() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let proxy = proxy_onto(&caller, &item, ITEM_INTERFACE, ITEM_OBJECT_PATH).await;

    proxy
        .call_method("ProvideXdgActivationToken", &("gnome-shell-middle-click",))
        .await
        .expect("ProvideXdgActivationToken");
    proxy
        .call_method("SecondaryActivate", &(10_i32, 20_i32))
        .await
        .expect("SecondaryActivate");

    stays("nothing opened", Duration::from_millis(200), || {
        recorder.actions().is_empty()
    })
    .await;

    // And the token the middle click left behind is spent by the next real
    // activation rather than leaking into it stale: `Activate` takes whatever
    // is in the slot, and the shell always offers a fresh one first.
    proxy
        .call_method("ProvideXdgActivationToken", &("gnome-shell-real-click",))
        .await
        .expect("ProvideXdgActivationToken");
    proxy
        .call_method("Activate", &(10_i32, 20_i32))
        .await
        .expect("Activate");
    until("the panel", || !recorder.actions().is_empty()).await;
    assert_eq!(recorder.actions(), vec![TrayAction::QuickPanel]);
    assert_eq!(
        recorder.tokens(),
        vec![Some("gnome-shell-real-click".to_string())],
        "a stale token from the middle click was forwarded"
    );
}

/// N15 — `ContextMenu` is never needed, and stays a no-op.
///
/// The extension draws the menu itself from `com.canonical.dbusmenu`; the
/// string `ContextMenu` does not occur in any of its JavaScript. The whole
/// menu is therefore reachable without ever calling it — which is what this
/// asserts, because "we implement a method nobody calls" is only safe while
/// the thing they call instead actually works.
#[tokio::test(flavor = "multi_thread")]
async fn n15_the_menu_is_reachable_without_contextmenu_ever_being_called() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;

    let menu = menu_proxy(&caller, &item).await;
    let (_revision, (_root, _props, children)): (u32, LayoutItem) = menu
        .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
        .await
        .expect("GetLayout");
    assert_eq!(children.len(), 3, "the menu needed a ContextMenu call");

    // And calling it anyway is inert — no window, no popup, no GTK in the
    // daemon.
    let item_proxy = proxy_onto(&caller, &item, ITEM_INTERFACE, ITEM_OBJECT_PATH).await;
    item_proxy
        .call_method("ContextMenu", &(5_i32, 5_i32))
        .await
        .expect("ContextMenu");
    stays("nothing opened", Duration::from_millis(200), || {
        recorder.actions().is_empty()
    })
    .await;
}

/// N16 — nothing the extension can read is about a person, a file or a peer.
///
/// Scoped deliberately to *what GNOME reads*: every property in its own
/// `StatusNotifierItem.xml`, plus the menu. `ToolTip` is not in that list —
/// the extension comments it out — but OmniBridge publishes one anyway, on a
/// public object, so it is included here rather than excused.
#[tokio::test(flavor = "multi_thread")]
async fn n16_nothing_the_extension_reads_is_private() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;

    let props = zbus::fdo::PropertiesProxy::builder(&caller)
        .destination(item.bus_name.as_str())
        .expect("destination")
        .path(ITEM_OBJECT_PATH)
        .expect("path")
        .build()
        .await
        .expect("a properties proxy");
    let all = props
        .get_all(zbus::names::InterfaceName::try_from(ITEM_INTERFACE).expect("interface"))
        .await
        .expect("GetAll");

    let mut strings = Vec::new();
    for value in all.values() {
        collect_strings(value, &mut strings);
    }

    let menu = menu_proxy(&caller, &item).await;
    let rows: Vec<(i32, HashMap<String, OwnedValue>)> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .expect("GetGroupProperties");
    for (_, row) in &rows {
        for value in row.values() {
            collect_strings(value, &mut strings);
        }
    }

    // The closed set. Everything on both objects is one of these, and each is
    // a compile-time constant in `model.rs` or a word from the protocol.
    let allowed = [
        "",
        "io.github.yurisismotto.omnibridge",
        "OmniBridge",
        "One bridge. Any device.",
        "ApplicationStatus",
        "Active",
        "normal",
        "submenu",
        "Quick Panel",
        "Files",
        "Settings",
    ];
    for found in &strings {
        assert!(
            allowed.contains(&found.as_str()),
            "the tray publishes {found:?}, which is not one of the constants"
        );
    }
}

fn collect_strings(value: &Value<'_>, out: &mut Vec<String>) {
    match value {
        Value::Str(s) => out.push(s.to_string()),
        Value::ObjectPath(_) | Value::Signature(_) => {}
        Value::Array(items) => {
            for item in items.iter() {
                collect_strings(item, out);
            }
        }
        Value::Structure(fields) => {
            for field in fields.fields() {
                collect_strings(field, out);
            }
        }
        Value::Value(inner) => collect_strings(inner, out),
        Value::Dict(dict) => {
            for (key, item) in dict.iter() {
                collect_strings(key, out);
                collect_strings(item, out);
            }
        }
        _ => {}
    }
}

// ===========================================================================
// N17, N18 — the two things this sprint promised not to do
// ===========================================================================

/// N17 — no tray backend was added to reach GNOME.
///
/// The whole premise of the sprint: the extension consumes
/// `org.kde.StatusNotifierItem`, so the KDE implementation *is* the GNOME
/// implementation. If a later change reaches for `libappindicator`,
/// `libayatana-appindicator`, a `ksni` crate or a GTK tray, this fails —
/// which is the only way "we did not need a second tray" stays true after
/// this sprint stops watching.
#[test]
fn n17_no_tray_backend_dependency_was_added() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("reading the crate manifest");
    let code = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    for forbidden in [
        "appindicator",
        "ayatana",
        "ksni",
        "libdbusmenu",
        "gtk",
        "libayatana",
    ] {
        assert!(
            !code.contains(forbidden),
            "`{forbidden}` appears in omnibridge-linux's dependencies; the GNOME \
             extension needs none of them"
        );
    }
}

/// N18 — the item says nothing to the host it was not asked to say.
///
/// One registration, and then silence. No polling for a watcher that is
/// already there, no re-announcement, no keep-alive: the only reason this
/// module ever speaks is a `NameOwnerChanged` for one name. `tray_dbus.rs`
/// proves the no-timer half by reading the source; this proves the
/// on-the-wire half by counting what a host actually receives.
#[tokio::test(flavor = "multi_thread")]
async fn n18_after_registering_the_item_says_nothing_further() {
    let bus = TestBus::start();
    let (_host, log) = start_host(&bus).await;
    let _item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    stays("silence", Duration::from_millis(500), || {
        log.calls().len() == 1
    })
    .await;
    assert_eq!(log.calls(), vec!["RegisterStatusNotifierItem".to_string()]);
}
