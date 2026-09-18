//! D1–D15 — the tray against a real message bus.
//!
//! # The fixture
//!
//! Every test in this file raises its **own** `dbus-daemon`, from a
//! configuration written here, with **no service directories at all**. Two
//! consequences, both deliberate:
//!
//! * nothing in these tests can reach the developer's session — no real
//!   Plasma, no real GNOME, and above all no real `anyflow-gui`, which a bus
//!   with the normal service directories would happily start;
//! * nothing is activatable, so a test that expected D-Bus activation to
//!   rescue it would fail rather than quietly succeed for the wrong reason.
//!
//! The fake watcher serves `org.kde.StatusNotifierWatcher` with the
//! signatures from KDE's own `org.kde.StatusNotifierWatcher.xml`, and — like
//! the real one in `plasma-workspace/statusnotifierwatcher.cpp` — it
//! **verifies** a registration by reading the item's properties before
//! accepting it. An item that owned its name but exported no object would be
//! refused here exactly as Plasma refuses it.

#![cfg(feature = "tray")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyflow_linux::tray::activate::{ActivationError, ApplicationActivator};
use anyflow_linux::tray::item::{ITEM_OBJECT_PATH, MENU_OBJECT_PATH};
use anyflow_linux::tray::model::TrayAction;
use anyflow_linux::tray::{publish, PublishedItem};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

// ===========================================================================
// A private session bus
// ===========================================================================

struct TestBus {
    child: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl TestBus {
    fn start() -> TestBus {
        let dir = tempfile::tempdir().expect("a temporary directory for the bus");
        let config = dir.path().join("bus.conf");
        // No `<servicedir>`: this bus can start nothing. `<listen>` uses a
        // short path under /tmp because a Unix socket address is bounded by
        // `sun_path`, and a temporary directory deep under a home directory
        // will exceed it.
        std::fs::write(
            &config,
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
        )
        .expect("writing the bus configuration");

        let mut child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address")
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon should be installed");
        let stdout = child.stdout.take().expect("the bus prints its address");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("reading the bus address");
        let address = line.trim().to_string();
        assert!(
            address.starts_with("unix:"),
            "unexpected bus address {address:?}"
        );
        TestBus {
            child,
            address,
            _dir: dir,
        }
    }

    async fn connect(&self) -> zbus::Connection {
        zbus::conn::Builder::address(self.address.as_str())
            .expect("the address parses")
            .build()
            .await
            .expect("connecting to the private bus")
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Waits for `check` to hold, or fails. Nothing here sleeps for a fixed time
/// and then asserts: that is how a suite becomes flaky on a loaded machine.
async fn until<F: FnMut() -> bool>(what: &str, mut check: F) {
    for _ in 0..600 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Holds for the whole window, rather than merely at the end of it.
async fn stays<F: FnMut() -> bool>(what: &str, window: Duration, mut check: F) {
    let deadline = std::time::Instant::now() + window;
    while std::time::Instant::now() < deadline {
        assert!(check(), "{what} stopped holding");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ===========================================================================
// The fake watcher — the real signatures, and the real verification
// ===========================================================================

/// `GetLayout`'s reply item, `(ia{sv}av)`.
type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
/// One row of `GetGroupProperties`, `(ia{sv})`.
type Properties = (i32, HashMap<String, OwnedValue>);

const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

#[derive(Clone, Default)]
struct WatcherLog(Arc<Mutex<Vec<String>>>);

impl WatcherLog {
    fn registered(&self) -> Vec<String> {
        self.0.lock().expect("watcher log").clone()
    }
    fn count(&self) -> usize {
        self.0.lock().expect("watcher log").len()
    }
}

struct FakeWatcher {
    log: WatcherLog,
    /// A second connection to the same bus, used to check the item the way
    /// Plasma checks it. Separate from the one serving this object so that a
    /// verification can never be waiting on the connection that has to answer
    /// it.
    verifier: zbus::Connection,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl FakeWatcher {
    /// The real implementation reads a leading `/` as an object path and takes
    /// the service from the sender; anything else is a bus name and the path
    /// is `/StatusNotifierItem`. Both branches are here so that a change in
    /// what AnyFlow sends is visible.
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let (name, path) = if service.starts_with('/') {
            (
                header
                    .sender()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "?".into()),
                service.clone(),
            )
        } else {
            (service.clone(), ITEM_OBJECT_PATH.to_string())
        };

        // Plasma refuses a registration whose item does not answer. So does
        // this: a test that accepted an item with no object would be a test
        // that could not tell a working tray from a name.
        let proxy = zbus::Proxy::new(
            &self.verifier,
            name.as_str(),
            path.as_str(),
            "org.kde.StatusNotifierItem",
        )
        .await
        .map_err(|_| zbus::fdo::Error::InvalidArgs("unreachable item".into()))?;
        let id: String = proxy
            .get_property("Id")
            .await
            .map_err(|_| zbus::fdo::Error::InvalidArgs("item has no Id".into()))?;
        assert!(!id.is_empty());

        self.log
            .0
            .lock()
            .expect("watcher log")
            .push(format!("{name}{path}"));
        Ok(())
    }

    fn register_status_notifier_host(&self, _service: String) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.log.registered()
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

/// A watcher that is on the bus for as long as the returned connection lives.
async fn start_watcher(bus: &TestBus) -> (zbus::Connection, WatcherLog) {
    let log = WatcherLog::default();
    let verifier = bus.connect().await;
    let conn = bus.connect().await;
    conn.object_server()
        .at(
            WATCHER_PATH,
            FakeWatcher {
                log: log.clone(),
                verifier,
            },
        )
        .await
        .expect("serving the fake watcher");
    conn.request_name(WATCHER_NAME)
        .await
        .expect("owning the watcher name");
    (conn, log)
}

// ===========================================================================
// A recording activator
// ===========================================================================

#[derive(Clone, Default)]
struct Recorder {
    actions: Arc<Mutex<Vec<TrayAction>>>,
    tokens: Arc<Mutex<Vec<Option<String>>>>,
    fail: Arc<AtomicUsize>,
}

impl Recorder {
    fn actions(&self) -> Vec<TrayAction> {
        self.actions.lock().expect("recorder").clone()
    }
    fn always_fail(&self) {
        self.fail.store(1, Ordering::SeqCst);
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
        if self.fail.load(Ordering::SeqCst) == 1 {
            return Err(ActivationError::Failed("test failure".into()));
        }
        Ok(())
    }
}

// ===========================================================================
// The item under test
// ===========================================================================

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

/// A proxy onto the item's own interface, from a caller's point of view.
async fn item_proxy<'a>(
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
// D1, D2 — the two starting conditions
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d1_registers_when_the_watcher_is_already_there() {
    let bus = TestBus::start();
    let (_watcher, log) = start_watcher(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;

    until("the item to register", || log.count() == 1).await;
    assert_eq!(
        log.registered(),
        vec![format!("{}{}", item.bus_name, ITEM_OBJECT_PATH)],
        "the watcher was told a bus name, and assumed the standard path"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn d2_an_absent_watcher_is_not_an_error() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;

    // The item is published and answering even though nothing is listening.
    let caller = bus.connect().await;
    let proxy = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;
    let id: String = proxy.get_property("Id").await.expect("Id");
    assert_eq!(id, "io.github.yurisismotto.anyflow");

    // And the task that follows the shell is still alive, waiting.
    stays(
        "the tray task to stay alive",
        Duration::from_millis(300),
        || !item.task.is_finished(),
    )
    .await;
}

// ===========================================================================
// D3, D4, D5, D13 — the shell's lifecycle
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d3_a_watcher_that_appears_later_gets_a_registration() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;

    // Nothing to register with yet.
    stays(
        "no registration without a watcher",
        Duration::from_millis(200),
        || true,
    )
    .await;

    let (_watcher, log) = start_watcher(&bus).await;
    until("the item to notice the new shell", || log.count() == 1).await;
    assert_eq!(log.registered().len(), 1);
    assert!(log.registered()[0].starts_with(&item.bus_name));
}

#[tokio::test(flavor = "multi_thread")]
async fn d4_the_watcher_going_away_does_not_take_the_item_with_it() {
    let bus = TestBus::start();
    let (watcher, log) = start_watcher(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("the first registration", || log.count() == 1).await;

    drop(watcher);

    // The task lives, the name is still owned, the object still answers.
    stays(
        "the tray task to survive the shell",
        Duration::from_millis(400),
        || !item.task.is_finished(),
    )
    .await;
    let caller = bus.connect().await;
    let proxy = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;
    let status: String = proxy.get_property("Status").await.expect("Status");
    assert_eq!(status, "Active");
}

#[tokio::test(flavor = "multi_thread")]
async fn d5_a_shell_that_comes_back_gets_exactly_one_new_registration() {
    let bus = TestBus::start();
    let (watcher, first) = start_watcher(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("the first registration", || first.count() == 1).await;

    drop(watcher);
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (_watcher2, second) = start_watcher(&bus).await;
    until("the second registration", || second.count() == 1).await;

    // One, and it stays one: a shell restart must not leave two AnyFlow icons
    // behind.
    stays(
        "exactly one registration",
        Duration::from_millis(400),
        || second.count() == 1,
    )
    .await;
    assert!(second.registered()[0].starts_with(&item.bus_name));
}

#[tokio::test(flavor = "multi_thread")]
async fn d13_a_shell_that_announces_itself_repeatedly_is_registered_with_once() {
    let bus = TestBus::start();
    let (watcher, log) = start_watcher(&bus).await;
    let item = start_item(&bus, &Recorder::default()).await;
    until("the first registration", || log.count() == 1).await;

    // A name being requested again by the connection that already owns it
    // produces no owner change at all, and several unrelated names appearing
    // and vanishing produce owner changes that are not this one. Neither may
    // cause a second registration.
    for _ in 0..5 {
        watcher
            .request_name(WATCHER_NAME)
            .await
            .expect("re-requesting a name already owned");
        let noise = bus.connect().await;
        noise
            .request_name("org.example.Noise")
            .await
            .expect("an unrelated name");
        drop(noise);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    stays(
        "exactly one registration",
        Duration::from_millis(300),
        || log.count() == 1,
    )
    .await;
    assert!(log.registered()[0].starts_with(&item.bus_name));
}

// ===========================================================================
// D6 — one item
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d6_the_watcher_sees_exactly_one_anyflow_item() {
    let bus = TestBus::start();
    let (watcher_conn, log) = start_watcher(&bus).await;
    let _item = start_item(&bus, &Recorder::default()).await;
    until("registration", || log.count() == 1).await;

    // Asked of the watcher over the bus, the way a shell would.
    let caller = bus.connect().await;
    let proxy = zbus::Proxy::new(&caller, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME)
        .await
        .expect("a proxy onto the watcher");
    let items: Vec<String> = proxy
        .get_property("RegisteredStatusNotifierItems")
        .await
        .expect("RegisteredStatusNotifierItems");
    assert_eq!(items.len(), 1, "more than one AnyFlow item: {items:?}");

    // And the item's name says which process it belongs to, exactly as KDE's
    // own client names its items.
    let expected_prefix = format!("org.kde.StatusNotifierItem-{}-", std::process::id());
    assert!(
        items[0].starts_with(&expected_prefix),
        "{:?} is not an `org.kde.StatusNotifierItem-<pid>-<n>` name",
        items[0]
    );
    drop(watcher_conn);
}

// ===========================================================================
// D7 — the properties
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d7_the_item_properties_have_the_types_and_values_the_spec_requires() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let proxy = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;

    // Read as a whole, so the assertion is about everything the interface
    // publishes rather than the subset somebody listed.
    let all: HashMap<String, OwnedValue> = proxy
        .get_property::<HashMap<String, OwnedValue>>("__none__")
        .await
        .err()
        .map(|_| HashMap::new())
        .unwrap_or_default();
    assert!(all.is_empty(), "a property named __none__ exists");

    let props = zbus::fdo::PropertiesProxy::builder(&caller)
        .destination(item.bus_name.as_str())
        .expect("destination")
        .path(ITEM_OBJECT_PATH)
        .expect("path")
        .build()
        .await
        .expect("a properties proxy");
    let all = props
        .get_all(
            zbus::names::InterfaceName::try_from("org.kde.StatusNotifierItem")
                .expect("interface name"),
        )
        .await
        .expect("GetAll");

    // `OwnedValue` dereferences to the `Value` it holds.
    let sig = |k: &str| {
        all.get(k)
            .unwrap_or_else(|| panic!("no property {k}"))
            .value_signature()
            .to_string()
    };
    let text = |k: &str| String::try_from(all[k].try_clone().expect("clone")).expect(k);

    // The signatures come from `org.kde.StatusNotifierItem.xml`.
    assert_eq!(sig("Category"), "s");
    assert_eq!(sig("Id"), "s");
    assert_eq!(sig("Title"), "s");
    assert_eq!(sig("Status"), "s");
    assert_eq!(sig("WindowId"), "i");
    assert_eq!(sig("IconThemePath"), "s");
    assert_eq!(sig("Menu"), "o");
    assert_eq!(sig("ItemIsMenu"), "b");
    assert_eq!(sig("IconName"), "s");
    assert_eq!(sig("IconPixmap"), "a(iiay)");
    assert_eq!(sig("OverlayIconName"), "s");
    assert_eq!(sig("OverlayIconPixmap"), "a(iiay)");
    assert_eq!(sig("AttentionIconName"), "s");
    assert_eq!(sig("AttentionIconPixmap"), "a(iiay)");
    assert_eq!(sig("AttentionMovieName"), "s");
    assert_eq!(sig("ToolTip"), "(sa(iiay)ss)");

    // And the values.
    assert_eq!(text("Category"), "ApplicationStatus");
    assert_eq!(text("Id"), "io.github.yurisismotto.anyflow");
    assert_eq!(text("Title"), "AnyFlow");
    assert_eq!(text("Status"), "Active");
    assert_eq!(text("IconName"), "io.github.yurisismotto.anyflow");
    assert_eq!(text("IconThemePath"), "", "the item names a directory");
    assert_eq!(text("AttentionIconName"), "");
    assert_eq!(text("AttentionMovieName"), "");
    assert_eq!(text("OverlayIconName"), "");

    let menu: OwnedObjectPath =
        OwnedObjectPath::try_from(all["Menu"].try_clone().expect("clone")).expect("Menu");
    assert_eq!(menu.as_str(), MENU_OBJECT_PATH);

    let is_menu =
        bool::try_from(all["ItemIsMenu"].try_clone().expect("clone")).expect("ItemIsMenu");
    assert!(!is_menu, "a left click would open the menu, not the panel");

    let window_id = i32::try_from(all["WindowId"].try_clone().expect("clone")).expect("WindowId");
    assert_eq!(window_id, 0, "the daemon claimed to own a window");

    // No pixmaps anywhere: the icon is a theme name.
    for key in ["IconPixmap", "OverlayIconPixmap", "AttentionIconPixmap"] {
        let pixmaps: Vec<(i32, i32, Vec<u8>)> =
            Vec::try_from(all[key].try_clone().expect("clone")).expect(key);
        assert!(pixmaps.is_empty(), "{key} carries image data");
    }
}

// ===========================================================================
// D8 — Activate
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d8_a_left_click_asks_for_the_quick_panel_and_nothing_else() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let proxy = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;

    proxy
        .call_method("Activate", &(120_i32, 40_i32))
        .await
        .expect("Activate");
    until("the quick panel to be asked for", || {
        recorder.actions() == vec![TrayAction::QuickPanel]
    })
    .await;

    // The coordinates changed nothing, and neither did calling it again.
    proxy
        .call_method("Activate", &(0_i32, 0_i32))
        .await
        .expect("Activate");
    until("the second click", || recorder.actions().len() == 2).await;
    assert_eq!(
        recorder.actions(),
        vec![TrayAction::QuickPanel, TrayAction::QuickPanel]
    );

    // The gestures whose correct behaviour is "nothing" do nothing.
    proxy
        .call_method("SecondaryActivate", &(1_i32, 1_i32))
        .await
        .expect("SecondaryActivate");
    proxy
        .call_method("ContextMenu", &(1_i32, 1_i32))
        .await
        .expect("ContextMenu");
    for delta in [-120_i32, 120, 1, -1] {
        proxy
            .call_method("Scroll", &(delta, "vertical"))
            .await
            .expect("Scroll");
        proxy
            .call_method("Scroll", &(delta, "horizontal"))
            .await
            .expect("Scroll");
    }
    stays("no extra activation", Duration::from_millis(250), || {
        recorder.actions().len() == 2
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_activation_token_is_forwarded_once_and_then_forgotten() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let proxy = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;

    proxy
        .call_method("ProvideXdgActivationToken", &("kwin-token-1",))
        .await
        .expect("ProvideXdgActivationToken");
    proxy
        .call_method("Activate", &(0_i32, 0_i32))
        .await
        .expect("Activate");
    until("the first activation", || recorder.actions().len() == 1).await;

    proxy
        .call_method("Activate", &(0_i32, 0_i32))
        .await
        .expect("Activate");
    until("the second activation", || recorder.actions().len() == 2).await;

    let tokens = recorder.tokens.lock().expect("tokens").clone();
    assert_eq!(
        tokens,
        vec![Some("kwin-token-1".to_string()), None],
        "a spent activation token was offered a second time"
    );
}

// ===========================================================================
// D9, D10, D11, D12 — the menu
// ===========================================================================

async fn menu_proxy<'a>(conn: &'a zbus::Connection, item: &Item) -> zbus::Proxy<'a> {
    item_proxy(conn, item, "com.canonical.dbusmenu", MENU_OBJECT_PATH).await
}

async fn click(proxy: &zbus::Proxy<'_>, id: i32) {
    let empty = Value::from("");
    proxy
        .call_method("Event", &(id, "clicked", &empty, 0_u32))
        .await
        .expect("Event");
}

#[tokio::test(flavor = "multi_thread")]
async fn d9_d10_d11_each_menu_row_opens_the_surface_it_names() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    // The ids are read back out of the layout rather than typed here, so this
    // test asks the item what its menu is and then uses the answer — which is
    // what Plasma does.
    let (_revision, root): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, 1_i32, Vec::<String>::new()))
        .await
        .expect("GetLayout");
    assert_eq!(root.0, 0);
    assert_eq!(root.2.len(), 3, "the menu does not have three rows");

    let rows: Vec<(i32, String)> = root
        .2
        .iter()
        .map(|child| {
            let s = zbus::zvariant::Structure::try_from(child.try_clone().expect("clone"))
                .expect("a child item");
            let fields = s.fields();
            let id = i32::try_from(fields[0].try_clone().expect("clone")).expect("id");
            let props: HashMap<String, OwnedValue> =
                HashMap::try_from(fields[1].try_clone().expect("clone")).expect("props");
            let label =
                String::try_from(props["label"].try_clone().expect("clone")).expect("label");
            (id, label)
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (1, "Quick Panel".to_string()),
            (2, "Files".to_string()),
            (3, "Settings".to_string()),
        ]
    );

    for (id, expected) in [
        (1, TrayAction::QuickPanel),
        (2, TrayAction::Files),
        (3, TrayAction::Settings),
    ] {
        let before = recorder.actions().len();
        click(&menu, id).await;
        until("the row to act", || recorder.actions().len() == before + 1).await;
        assert_eq!(*recorder.actions().last().expect("an action"), expected);
    }
    assert_eq!(
        recorder.actions(),
        vec![
            TrayAction::QuickPanel,
            TrayAction::Files,
            TrayAction::Settings
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn d12_an_invented_menu_id_or_event_does_nothing_at_all() {
    let bus = TestBus::start();
    let recorder = Recorder::default();
    let item = start_item(&bus, &recorder).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    // Ids that do not exist, including the root and the extremes of the type.
    for id in [-2_147_483_648_i32, -7, 0, 4, 77, 2_147_483_647] {
        click(&menu, id).await;
    }
    // Real ids with verbs that are not decisions — this is the half that a
    // careless implementation gets wrong, because the id is valid.
    for event in ["hovered", "opened", "closed", "x-kde-poke", "", "CLICKED"] {
        let empty = Value::from("");
        menu.call_method("Event", &(1_i32, event, &empty, 0_u32))
            .await
            .expect("Event");
    }
    stays("nothing to happen", Duration::from_millis(300), || {
        recorder.actions().is_empty()
    })
    .await;

    // And a real click still works afterwards: refusing nonsense must not have
    // left the menu wedged.
    click(&menu, 1).await;
    until("a real click", || {
        recorder.actions() == vec![TrayAction::QuickPanel]
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_menu_answers_the_rest_of_the_protocol_plasma_uses() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let menu = menu_proxy(&caller, &item).await;

    let version: u32 = menu.get_property("Version").await.expect("Version");
    assert_eq!(version, 3);
    let status: String = menu.get_property("Status").await.expect("Status");
    assert_eq!(status, "normal", "the menu asks for prominence");

    // `AboutToShow` is called before every single right click; answering
    // `true` would make Plasma re-read a constant each time.
    let needs_update: bool = menu
        .call("AboutToShow", &(0_i32,))
        .await
        .expect("AboutToShow");
    assert!(!needs_update);

    // The root advertises children, or the host draws an empty menu.
    let (_r, root): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, 1_i32, Vec::<String>::new()))
        .await
        .expect("GetLayout");
    let display = String::try_from(root.1["children-display"].try_clone().expect("clone"))
        .expect("children-display");
    assert_eq!(display, "submenu");

    // Depth 0 is "this item only".
    let (_r, shallow): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, 0_i32, Vec::<String>::new()))
        .await
        .expect("GetLayout");
    assert!(shallow.2.is_empty());

    // A made-up parent is an error, not a plausible empty menu.
    let bogus: zbus::Result<(u32, Layout)> = menu
        .call("GetLayout", &(99_i32, 1_i32, Vec::<String>::new()))
        .await;
    assert!(bogus.is_err(), "a made-up menu id returned a layout");

    // Group properties, with and without a filter.
    let everything: Vec<Properties> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .expect("GetGroupProperties");
    assert_eq!(everything.len(), 4, "the root and three rows");
    let filtered: Vec<Properties> = menu
        .call(
            "GetGroupProperties",
            &(vec![2_i32], vec!["label".to_string()]),
        )
        .await
        .expect("GetGroupProperties");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].0, 2);
    assert_eq!(filtered[0].1.keys().collect::<Vec<_>>(), vec!["label"]);

    // `GetProperty` returns `v`, so the answer arrives boxed.
    let label: OwnedValue = menu
        .call("GetProperty", &(3_i32, "label"))
        .await
        .expect("GetProperty");
    assert_eq!(
        String::try_from(label.try_clone().expect("clone")).expect("label"),
        "Settings"
    );
    let missing: zbus::Result<OwnedValue> = menu.call("GetProperty", &(3_i32, "nonesuch")).await;
    assert!(
        missing.is_err(),
        "an invented property name returned a value"
    );
}

/// The D-Bus half of T15: an activator that always fails.
///
/// The failure has to be a non-event. The item stays registered, the menu
/// answers exactly as it did, and the next click is tried — which is the
/// behaviour that makes "the GUI could not be started" something a person
/// recovers from by clicking again rather than by restarting the daemon.
#[tokio::test(flavor = "multi_thread")]
async fn an_activation_that_always_fails_changes_nothing_about_the_tray() {
    let bus = TestBus::start();
    let (_watcher, log) = start_watcher(&bus).await;
    let recorder = Recorder::default();
    recorder.always_fail();
    let item = start_item(&bus, &recorder).await;
    until("registration", || log.count() == 1).await;

    let caller = bus.connect().await;
    let sni = item_proxy(
        &caller,
        &item,
        "org.kde.StatusNotifierItem",
        ITEM_OBJECT_PATH,
    )
    .await;
    let menu = menu_proxy(&caller, &item).await;

    let before: Vec<Properties> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .expect("GetGroupProperties");

    for _ in 0..5 {
        sni.call_method("Activate", &(0_i32, 0_i32))
            .await
            .expect("Activate still returns successfully");
        click(&menu, 2).await;
    }
    until("every click to have been attempted", || {
        recorder.actions().len() == 10
    })
    .await;

    // Still registered — the watcher was never told anything changed.
    assert_eq!(log.count(), 1);
    // Still the same three rows, the same ids, the same labels.
    let after: Vec<Properties> = menu
        .call(
            "GetGroupProperties",
            &(Vec::<i32>::new(), Vec::<String>::new()),
        )
        .await
        .expect("GetGroupProperties");
    let shape = |rows: &[Properties]| {
        rows.iter()
            .map(|(id, props)| {
                let mut keys: Vec<String> = props.keys().cloned().collect();
                keys.sort();
                (*id, keys)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&before), shape(&after));
    // And the item still says it is Active.
    let status: String = sni.get_property("Status").await.expect("Status");
    assert_eq!(status, "Active");
}

// ===========================================================================
// D14 — no busy loop
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d14_with_no_watcher_the_item_sends_nothing_at_all() {
    let bus = TestBus::start();

    // A monitor connection, which receives a copy of every message on the bus.
    let monitor = bus.connect().await;
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let monitor_proxy = zbus::Proxy::new(
        &monitor,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus.Monitoring",
    )
    .await
    .expect("the monitoring interface");
    monitor_proxy
        .call_method("BecomeMonitor", &(Vec::<String>::new(), 0_u32))
        .await
        .expect("BecomeMonitor");

    let item = start_item(&bus, &Recorder::default()).await;
    let item_unique = item
        ._connection
        .unique_name()
        .expect("the item has a unique name")
        .to_string();

    let collector = {
        let seen = Arc::clone(&seen);
        let stream = zbus::MessageStream::from(monitor.clone());
        tokio::spawn(async move {
            use futures_lite::StreamExt as _;
            let mut stream = stream;
            while let Some(Ok(message)) = stream.next().await {
                let header = message.header();
                if header.sender().map(|s| s.to_string()).as_deref() == Some(item_unique.as_str()) {
                    seen.lock()
                        .expect("monitor log")
                        .push(format!("{:?}", header.member()));
                }
            }
        })
    };

    // Let everything the item does at start-up — Hello, RequestName, AddMatch,
    // GetNameOwner — happen and settle.
    tokio::time::sleep(Duration::from_millis(500)).await;
    seen.lock().expect("monitor log").clear();

    // Then two seconds of a session with no tray host in it. A daemon that
    // polled for the watcher, at any interval a person would choose, would
    // show up here.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let traffic = seen.lock().expect("monitor log").clone();
    assert!(
        traffic.is_empty(),
        "the idle tray sent {} messages in two seconds: {traffic:?}",
        traffic.len()
    );
    collector.abort();
    drop(item);
}

#[test]
fn d14_the_source_contains_no_timer_at_all() {
    // The second leg of the same claim, and the one that keeps holding when
    // nobody reruns the two-second measurement: there is no interval, no
    // sleep, no backoff and no retry counter anywhere in the tray.
    for file in [
        "src/tray/watcher.rs",
        "src/tray/mod.rs",
        "src/tray/item.rs",
        "src/tray/menu.rs",
        "src/tray/activate.rs",
    ] {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file);
        let code: String = std::fs::read_to_string(&path)
            .expect("reading the tray source")
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with('*')
            })
            .collect::<Vec<_>>()
            .join("\n");
        for pattern in [
            "sleep(",
            "interval(",
            "Duration::from",
            "retry",
            "backoff",
            "loop {",
        ] {
            assert!(
                !code.contains(pattern),
                "{file} contains {pattern:?}; the watcher is meant to be purely event-driven"
            );
        }
    }
}

// ===========================================================================
// D15 — nothing sensitive is on the bus
// ===========================================================================

#[tokio::test(flavor = "multi_thread")]
async fn d15_every_string_on_the_tray_objects_is_one_of_eight_constants() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;

    // Everything either object will say, gathered by asking rather than by
    // listing: `GetAll` on both interfaces plus the whole menu layout.
    let mut strings: Vec<String> = Vec::new();

    let props = zbus::fdo::PropertiesProxy::builder(&caller)
        .destination(item.bus_name.as_str())
        .expect("destination")
        .path(ITEM_OBJECT_PATH)
        .expect("path")
        .build()
        .await
        .expect("properties");
    let all = props
        .get_all(
            zbus::names::InterfaceName::try_from("org.kde.StatusNotifierItem").expect("interface"),
        )
        .await
        .expect("GetAll");
    for value in all.values() {
        collect_strings(value, &mut strings);
    }

    let menu = menu_proxy(&caller, &item).await;
    let (_r, root): (u32, Layout) = menu
        .call("GetLayout", &(0_i32, -1_i32, Vec::<String>::new()))
        .await
        .expect("GetLayout");
    collect_strings(
        &Value::from(zbus::zvariant::Structure::from((root.0, root.1, root.2))),
        &mut strings,
    );
    let status: String = menu.get_property("Status").await.expect("Status");
    strings.push(status);

    let allowed = [
        "",
        "io.github.yurisismotto.anyflow",
        "AnyFlow",
        "ApplicationStatus",
        "Active",
        "One flow. Any device.",
        "Quick Panel",
        "Files",
        "Settings",
        "submenu",
        "label",
        "enabled",
        "visible",
        "children-display",
        "normal",
    ];
    for s in &strings {
        assert!(
            allowed.contains(&s.as_str()),
            "the tray published the string {s:?}, which is not one of the constants \
             this product has decided may leave the process unasked"
        );
    }

    // And the shapes that would matter most if one ever did.
    for s in &strings {
        for forbidden in ["/home/", "/tmp/", "content://", "file://", "://", "192.168"] {
            assert!(!s.contains(forbidden), "{s:?} looks like {forbidden}");
        }
    }
}

fn collect_strings(value: &Value<'_>, out: &mut Vec<String>) {
    match value {
        Value::Str(s) => out.push(s.to_string()),
        Value::ObjectPath(_) | Value::Signature(_) => {}
        Value::Array(a) => {
            for v in a.iter() {
                collect_strings(v, out);
            }
        }
        Value::Structure(s) => {
            for v in s.fields() {
                collect_strings(v, out);
            }
        }
        Value::Dict(d) => {
            for (k, v) in d.iter() {
                collect_strings(k, out);
                collect_strings(v, out);
            }
        }
        Value::Value(inner) => collect_strings(inner, out),
        _ => {}
    }
}

// ===========================================================================
// Spec compliance — the exported interfaces, against the declarations the
// shell's own code is generated from
// ===========================================================================

/// Members of one interface, as `name(in-signature) -> out-signature` lines.
fn members(xml: &str, interface: &str) -> Vec<String> {
    let start = xml
        .find(&format!("<interface name=\"{interface}\">"))
        .unwrap_or_else(|| panic!("{interface} is not exported:\n{xml}"));
    let end = xml[start..]
        .find("</interface>")
        .expect("the interface closes")
        + start;
    let body = &xml[start..end];

    let attr = |tag: &str, key: &str| -> Option<String> {
        tag.find(&format!("{key}=\""))
            .map(|i| &tag[i + key.len() + 2..])
            .and_then(|rest| rest.find('"').map(|j| rest[..j].to_string()))
    };

    let mut out = Vec::new();
    let mut current: Option<(String, String, String, String)> = None; // kind, name, in, out
    for raw in body.lines() {
        let tag = raw.trim();
        for kind in ["method", "signal"] {
            if tag.starts_with(&format!("<{kind} name=")) {
                if let Some((k, n, i, o)) = current.take() {
                    out.push(format!("{k} {n}({i})->{o}"));
                }
                current = Some((
                    kind.to_string(),
                    attr(tag, "name").expect("a name"),
                    String::new(),
                    String::new(),
                ));
                if tag.ends_with("/>") {
                    if let Some((k, n, i, o)) = current.take() {
                        out.push(format!("{k} {n}({i})->{o}"));
                    }
                }
            }
        }
        if tag.starts_with("<arg ") {
            if let Some((_, _, ins, outs)) = current.as_mut() {
                let ty = attr(tag, "type").expect("an arg type");
                match attr(tag, "direction").as_deref() {
                    Some("out") => outs.push_str(&ty),
                    _ => ins.push_str(&ty),
                }
            }
        }
        if tag.starts_with("</method>") || tag.starts_with("</signal>") {
            if let Some((k, n, i, o)) = current.take() {
                out.push(format!("{k} {n}({i})->{o}"));
            }
        }
        if tag.starts_with("<property ") {
            out.push(format!(
                "property {} {} {}",
                attr(tag, "name").expect("a name"),
                attr(tag, "type").expect("a type"),
                attr(tag, "access").expect("an access")
            ));
        }
    }
    if let Some((k, n, i, o)) = current.take() {
        out.push(format!("{k} {n}({i})->{o}"));
    }
    out.sort();
    out
}

async fn introspect(conn: &zbus::Connection, item: &Item, path: &str) -> String {
    let proxy = item_proxy(conn, item, "org.freedesktop.DBus.Introspectable", path).await;
    proxy.call("Introspect", &()).await.expect("Introspect")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_notifier_item_interface_is_the_one_kde_declares() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let xml = introspect(&caller, &item, ITEM_OBJECT_PATH).await;

    // Transcribed from `org.kde.StatusNotifierItem.xml` in KDE's
    // `kstatusnotifieritem` framework — the file both KDE's client and
    // Plasma's tray are generated from. Every property, every method and
    // every signal it declares, with its signature. This is the gate that
    // makes "SNI spec compliance" a measurement rather than an opinion.
    let expected: Vec<String> = [
        "property Category s read",
        "property Id s read",
        "property Title s read",
        "property Status s read",
        "property WindowId i read",
        "property IconThemePath s read",
        "property Menu o read",
        "property ItemIsMenu b read",
        "property IconName s read",
        "property IconPixmap a(iiay) read",
        "property OverlayIconName s read",
        "property OverlayIconPixmap a(iiay) read",
        "property AttentionIconName s read",
        "property AttentionIconPixmap a(iiay) read",
        "property AttentionMovieName s read",
        "property ToolTip (sa(iiay)ss) read",
        "method ProvideXdgActivationToken(s)->",
        "method ContextMenu(ii)->",
        "method Activate(ii)->",
        "method SecondaryActivate(ii)->",
        "method Scroll(is)->",
        "signal NewTitle()->",
        "signal NewIcon()->",
        "signal NewAttentionIcon()->",
        "signal NewOverlayIcon()->",
        "signal NewMenu()->",
        "signal NewToolTip()->",
        "signal NewStatus(s)->",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    let mut expected = expected;
    expected.sort();

    assert_eq!(
        members(&xml, "org.kde.StatusNotifierItem"),
        expected,
        "the exported interface is not the one the specification declares"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_menu_interface_is_the_one_plasmas_importer_is_generated_from() {
    let bus = TestBus::start();
    let item = start_item(&bus, &Recorder::default()).await;
    let caller = bus.connect().await;
    let xml = introspect(&caller, &item, MENU_OBJECT_PATH).await;

    // Transcribed from `plasma-workspace/libdbusmenuqt/com.canonical.dbusmenu.xml`
    // — the copy Plasma's own `DBusMenuImporter` is generated from, and
    // therefore the exact set of calls a Plasma tray can make.
    let mut expected: Vec<String> = [
        "property Version u read",
        "property Status s read",
        "method Event(isvu)->",
        "method GetProperty(is)->v",
        "method GetLayout(iias)->u(ia{sv}av)",
        "method GetGroupProperties(aias)->a(ia{sv})",
        "method AboutToShow(i)->b",
        "signal ItemsPropertiesUpdated(a(ia{sv})a(ias))->",
        "signal LayoutUpdated(ui)->",
        "signal ItemActivationRequested(iu)->",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    expected.sort();

    assert_eq!(members(&xml, "com.canonical.dbusmenu"), expected);
}
