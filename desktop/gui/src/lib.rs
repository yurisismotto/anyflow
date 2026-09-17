//! AnyFlow for the desktop.
//!
//! A GTK4 / libadwaita front end for the daemon. It is a *client* of the same
//! local control socket the `anyflow` CLI uses and adds no protocol, no
//! capability and no privilege of its own: everything on screen is something
//! the daemon already reports, and every action is a request the CLI can make
//! too. See `desktop/runtime/src/server.rs`.
//!
//! Deliberately not an Electron app and not a web view. This is a Linux
//! desktop program: it honours the system font, the system colour scheme and
//! the system's accessibility settings because it is built out of the
//! platform's own widgets.
//!
//! # Two surfaces, one application
//!
//! ```text
//! anyflow-gui                     the GtkApplication
//! ├── Quick Panel                 everyday glance and actions   (panel/)
//! └── Settings                    devices, policies, diagnostics (views/)
//! ```
//!
//! One process, one application id, one poll of the daemon, one stored choice
//! of device. The two windows are two views of that; neither owns it, and
//! closing either leaves the other — and the daemon — entirely alone. There is
//! no `anyflow-quickpanel` anything: the agent is `anyflowd` and stays the
//! only long-lived process AnyFlow runs.
//!
//! # Activation
//!
//! Both surfaces are reachable as `GAction`s on the application:
//!
//! ```text
//! app.quick-panel     present the Quick Panel
//! app.settings        present the Settings window
//! app.transfers       present Settings on its Transfers page
//! ```
//!
//! and from a command line, which is forwarded to the running instance:
//!
//! ```console
//! $ anyflow-gui                  # Settings — the existing behaviour
//! $ anyflow-gui --quick-panel    # the Quick Panel
//! $ anyflow-gui --page files     # Settings, on one page
//! ```
//!
//! That pair is the seam a later desktop-shell integration — a KDE
//! StatusNotifierItem, say — is meant to use. GApplication exports its action
//! group on the session bus as `org.gtk.Actions`, so a tray item can raise the
//! panel by *name* without linking against this crate or knowing one thing
//! about how the panel is built. **No StatusNotifierItem is implemented here**
//! and no KDE dependency is taken; the seam is all this sprint owes it.

pub mod approval;
pub mod client;
pub mod panel;
pub mod selection;
pub mod theme;
pub mod views;
pub mod widgets;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use anyflow_control::{Request, Response};
use panel::QuickPanel;
use selection::Selection;

const APP_ID: &str = "io.github.yurisismotto.anyflow";

/// The application action that raises the Quick Panel.
pub const ACTION_QUICK_PANEL: &str = "quick-panel";
/// The application action that raises the Settings window.
pub const ACTION_SETTINGS: &str = "settings";
/// The application action that raises Settings on its Transfers page.
///
/// A third parameterless action rather than a parameter on
/// [`ACTION_SETTINGS`], which stays parameterless so that `app.settings` can
/// go on a button, a keybinding or the exported `org.gtk.Actions` interface
/// without a caller having to know a variant type. The Quick Panel's "View
/// all transfers" is its first caller; a tray item would be its second.
pub const ACTION_TRANSFERS: &str = "transfers";

/// How often the daemon is asked for fresh state.
///
/// The control socket cannot push, so this polls. Two seconds is chosen to
/// feel live without being wasteful: everything requested is metadata the
/// daemon already holds in memory, and no clipboard content is ever among it.
const REFRESH_SECS: u64 = 2;

/// What the command line asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// `--quick-panel`.
    QuickPanel,
    /// Anything else, including no arguments at all.
    ///
    /// Opening Settings on a bare launch is the behaviour this application
    /// has always had, and it stays: a person who runs `anyflow-gui` today
    /// gets the window they got yesterday. The Quick Panel is additive and
    /// asks for itself by name.
    Settings(Option<Page>),
}

impl Launch {
    /// Parses an argument vector, including the one a remote invocation
    /// forwards to the running instance.
    ///
    /// Tolerant by design: an argument this build does not know is not worth
    /// refusing to open a window over.
    pub fn parse<S: AsRef<str>>(args: &[S]) -> Launch {
        let mut page = None;
        let mut iter = args.iter().map(|a| a.as_ref()).skip(1);
        while let Some(arg) = iter.next() {
            match arg {
                "--quick-panel" | "--panel" => return Launch::QuickPanel,
                "--page" => page = iter.next().and_then(Page::from_name),
                other => {
                    if let Some(name) = other.strip_prefix("--page=") {
                        page = Page::from_name(name);
                    }
                }
            }
        }
        Launch::Settings(page)
    }
}

/// Starts the application.
pub fn run() -> glib::ExitCode {
    gio::resources_register_include!("anyflow.gresource")
        .expect("the compiled-in resources should load");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        // Without this, a second `anyflow-gui --quick-panel` would activate
        // the running instance and the running instance would never see the
        // flag — it would re-present whatever window it opened with. The
        // remote argv has to reach the primary instance for the activation
        // seam to mean anything.
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let handle: Rc<RefCell<Option<Rc<App>>>> = Rc::new(RefCell::new(None));

    {
        let handle = handle.clone();
        app.connect_startup(move |app| {
            adw::init().expect("libadwaita should initialise");
            install_styles();
            install_icons();
            *handle.borrow_mut() = Some(App::start(app));
        });
    }

    {
        let handle = handle.clone();
        app.connect_command_line(move |_, command_line| {
            let args: Vec<String> = command_line
                .arguments()
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            if let Some(app) = handle.borrow().as_ref() {
                app.launch(Launch::parse(&args));
            }
            0
        });
    }

    // A bare `activate` — the desktop file's `Activate` D-Bus method, or a
    // second launch with no arguments — opens the surface a bare launch
    // opens.
    {
        let handle = handle.clone();
        app.connect_activate(move |_| {
            if let Some(app) = handle.borrow().as_ref() {
                app.launch(Launch::Settings(None));
            }
        });
    }

    app.run()
}

/// Loads the stylesheet and keeps it in step with the system colour scheme.
fn install_styles() {
    let provider = gtk::CssProvider::new();
    let manager = adw::StyleManager::default();

    let apply = {
        let provider = provider.clone();
        move |dark: bool| provider.load_from_string(&theme::stylesheet(dark))
    };
    apply(manager.is_dark());

    // Dark is a derived palette rather than an inversion, so it cannot simply
    // be a filter over the light one — the sheet is swapped instead.
    manager.connect_dark_notify(move |m| apply(m.is_dark()));

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Makes the application icon resolvable by name *inside this process*.
///
/// # What this does, and what it cannot do
///
/// The icon is compiled into the binary, so anything AnyFlow draws itself can
/// ask for it by name — and on **X11** that is also enough for the window
/// list, because GTK resolves the default icon name through this same theme
/// and attaches the result to the window as `_NET_WM_ICON`. Measured under
/// Xwayland on this build: a 48×48 `_NET_WM_ICON` is present and correct.
///
/// On **Wayland it is not enough, and cannot be.** There is no window-icon
/// channel in xdg-shell — no `_NET_WM_ICON` equivalent, and Mutter 50
/// advertises no `xdg_toplevel_icon_manager_v1` — so the shell never receives
/// an icon from the application at all. It derives one:
///
/// ```text
/// xdg_toplevel.set_app_id("io.github.yurisismotto.anyflow")   <- this process
///     -> the .desktop file with that id, from XDG_DATA_DIRS    <- the session
///         -> its Icon= name
///             -> that name in the *shell's* icon theme
/// ```
///
/// Only the first line is ours. A resource path added here is private to this
/// process and invisible to GNOME Shell, and
/// [`gtk::Window::set_default_icon_name`] has nowhere to send anything. What
/// makes the Wayland case work is the two files being installed where the
/// session looks, which `tools/install-desktop-metadata.sh` does and which a
/// package does with the same two files at a different prefix. This function
/// is deliberately *not* that: an application that installs things into a
/// person's home directory when they run it would be doing something they did
/// not ask for.
///
/// The name is identical in every case, which is the point of it being one
/// string — see [`APP_ID`].
fn install_icons() {
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_resource_path("/io/github/yurisismotto/anyflow/icons");
    }
    gtk::Window::set_default_icon_name(APP_ID);
}

/// Which page the Settings content pane is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Files,
    Clipboard,
    Notifications,
    Devices,
    TrustedPeers,
    Settings,
}

impl Page {
    const ALL: [Page; 7] = [
        Page::Dashboard,
        Page::Files,
        Page::Clipboard,
        Page::Notifications,
        Page::Devices,
        Page::TrustedPeers,
        Page::Settings,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::Files => "Files",
            Page::Clipboard => "Clipboard",
            Page::Notifications => "Notifications",
            Page::Devices => "Devices",
            Page::TrustedPeers => "Trusted peers",
            Page::Settings => "Settings",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Dashboard => "go-home-symbolic",
            Page::Files => "folder-symbolic",
            Page::Clipboard => "edit-paste-symbolic",
            Page::Notifications => "preferences-system-notifications-symbolic",
            Page::Devices => "computer-symbolic",
            Page::TrustedPeers => "system-users-symbolic",
            Page::Settings => "preferences-system-symbolic",
        }
    }

    /// Parses the `--page` argument.
    pub fn from_name(name: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Page::Dashboard => "dashboard",
            Page::Files => "files",
            Page::Clipboard => "clipboard",
            Page::Notifications => "notifications",
            Page::Devices => "devices",
            Page::TrustedPeers => "peers",
            Page::Settings => "settings",
        }
    }
}

/// Everything the daemon last told us.
///
/// `None` on a field means "not answered yet", which is displayed as such
/// rather than as an empty list — "no devices" and "the daemon is not running"
/// are different situations and the UI must not conflate them.
///
/// `PartialEq` is what lets [`views::Pages::render`] and
/// [`panel::QuickPanel::render`] tell an unchanged poll from a real change,
/// and it is a property of the control types rather than of this struct: every
/// field is a report `anyflow-control` defines, so two states are equal
/// exactly when the daemon said the same thing twice.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct DaemonState {
    pub status: Option<anyflow_control::StatusReport>,
    pub devices: Option<Vec<anyflow_control::DeviceReport>>,
    pub transfers: Option<Vec<anyflow_control::TransferReport>>,
    pub clipboard: Option<anyflow_control::ClipboardStatusReport>,
    /// Counts, states and platform identifiers. **No field on this report can
    /// hold a notification's title, body or application name**, which is what
    /// makes the notifications page — and the Quick Panel's notifications row
    /// — structurally incapable of becoming the history the design forbids.
    pub notifications: Option<anyflow_control::NotificationsStatusReport>,
    /// Set when the daemon could not be reached at all.
    pub error: Option<String>,
}

impl DaemonState {
    pub fn reachable(&self) -> bool {
        self.error.is_none() && self.status.is_some()
    }
}

/// How many control requests make up one refresh.
const CYCLE_REQUESTS: usize = 5;

type Reply = anyhow::Result<Response>;

/// One poll of the daemon, collected before anything is drawn.
///
/// The control socket answers one question per exchange, so a refresh is
/// several of them completing at their own pace. This holds the answers until
/// the last one lands and then commits them together, so the UI never draws a
/// half-updated poll and — far more importantly — draws once per interval
/// rather than once per reply.
struct Cycle {
    /// Bumped per poll, so a late answer can be recognised and dropped.
    generation: u64,
    /// Replies still to come in this poll.
    outstanding: usize,
    /// The state being assembled. Committed when `outstanding` reaches zero.
    pending: DaemonState,
}

fn fold_status(s: &mut DaemonState, reply: Reply) {
    match reply {
        Ok(Response::Status(report)) => {
            s.status = Some(report);
            s.error = None;
        }
        Ok(Response::Error { message }) => s.error = Some(message),
        Ok(_) => s.error = Some("unexpected reply from the daemon".into()),
        Err(e) => s.error = Some(e.to_string()),
    }
}

fn fold_devices(s: &mut DaemonState, reply: Reply) {
    if let Ok(Response::Devices(list)) = reply {
        s.devices = Some(list);
    }
}

fn fold_transfers(s: &mut DaemonState, reply: Reply) {
    if let Ok(Response::Transfers(list)) = reply {
        s.transfers = Some(list);
    }
}

fn fold_clipboard(s: &mut DaemonState, reply: Reply) {
    if let Ok(Response::Clipboard(report)) = reply {
        s.clipboard = Some(report);
    }
}

fn fold_notifications(s: &mut DaemonState, reply: Reply) {
    if let Ok(Response::Notifications(report)) = reply {
        s.notifications = Some(report);
    }
}

// ---------------------------------------------------------------------------
// The application
// ---------------------------------------------------------------------------

/// The Settings window and the handles needed to drive it.
struct SettingsWindow {
    window: adw::ApplicationWindow,
    pages: views::Pages,
    nav: gtk::ListBox,
}

/// Everything that outlives a window.
///
/// The poll, the daemon state, the chosen device and the incoming-file
/// approval attachment all belong here rather than to a window, which is what
/// makes the lifecycle rules true rather than merely intended: closing the
/// Quick Panel cannot stop a poll the panel does not own, and cannot detach an
/// approval provider the panel does not hold.
struct App {
    app: adw::Application,
    state: Rc<RefCell<DaemonState>>,
    selection: Rc<Selection>,
    /// Installed once the poll exists. See [`App::refresh_now`].
    refresh: RefCell<Option<Rc<dyn Fn()>>>,
    settings: RefCell<Option<SettingsWindow>>,
    panel: RefCell<Option<Rc<QuickPanel>>>,
    /// The daemon's incoming-file approval provider, for the life of the
    /// application.
    ///
    /// Application-scoped, not window-scoped. It used to belong to the one
    /// window there was; with two windows, hanging it on either would mean
    /// closing that one silently stopped the machine from being able to
    /// accept a file. Dropping it — which happens when the application exits
    /// — returns the daemon to declining every offer, which is the safe
    /// direction and the behaviour a desktop with no GUI has always had.
    approval: RefCell<Option<Rc<client::ApprovalHandle>>>,
}

impl App {
    /// The struct, with nothing running yet.
    fn bare(app: &adw::Application) -> Rc<App> {
        Rc::new(App {
            app: app.clone(),
            state: Rc::new(RefCell::new(DaemonState::default())),
            selection: Rc::new(Selection::load()),
            refresh: RefCell::new(None),
            settings: RefCell::new(None),
            panel: RefCell::new(None),
            approval: RefCell::new(None),
        })
    }

    fn start(app: &adw::Application) -> Rc<App> {
        let this = App::bare(app);
        this.install_actions();
        this.install_poll();

        // Parented to whichever window is in front when an offer arrives. The
        // question belongs to the machine, not to one window of it.
        let weak = Rc::downgrade(&this);
        *this.approval.borrow_mut() = Some(approval::install(move || {
            weak.upgrade().and_then(|a| a.app.active_window())
        }));

        this
    }

    /// The window and action behaviour, with no poll and no daemon
    /// attachment.
    ///
    /// The production path also starts a two-second poll and becomes the
    /// daemon's file-approval provider. A test must do neither: the poll
    /// would talk to whatever daemon the developer has running, and attaching
    /// as the approval provider would take that role away from their real
    /// session for as long as the test process lived.
    #[cfg(test)]
    fn for_test(app: &adw::Application) -> Rc<App> {
        let this = App::bare(app);
        this.install_actions();
        this
    }

    /// The activation seam. See the module documentation.
    fn install_actions(self: &Rc<Self>) {
        let quick_panel = gio::SimpleAction::new(ACTION_QUICK_PANEL, None);
        {
            let this = Rc::downgrade(self);
            quick_panel.connect_activate(move |_, _| {
                if let Some(this) = this.upgrade() {
                    this.show_quick_panel();
                }
            });
        }
        self.app.add_action(&quick_panel);

        let settings = gio::SimpleAction::new(ACTION_SETTINGS, None);
        {
            let this = Rc::downgrade(self);
            settings.connect_activate(move |_, _| {
                if let Some(this) = this.upgrade() {
                    this.show_settings(None);
                }
            });
        }
        self.app.add_action(&settings);

        // Opening the existing Settings window on its existing Transfers
        // page. It builds no second surface, starts no second process and —
        // this is the part that matters — touches no selection: looking at
        // what already happened is not a decision about where the next file
        // goes.
        let transfers = gio::SimpleAction::new(ACTION_TRANSFERS, None);
        {
            let this = Rc::downgrade(self);
            transfers.connect_activate(move |_, _| {
                if let Some(this) = this.upgrade() {
                    this.show_settings(Some(Page::Files));
                }
            });
        }
        self.app.add_action(&transfers);
    }

    fn launch(self: &Rc<Self>, launch: Launch) {
        match launch {
            Launch::QuickPanel => self.show_quick_panel(),
            Launch::Settings(page) => self.show_settings(page),
        }
    }

    /// Presents the Quick Panel, building it the first time.
    ///
    /// Repeated invocation presents the window that already exists. A second
    /// panel would be a second view of one state with no way to tell them
    /// apart, and the tray that will call this cannot be trusted not to call
    /// it twice.
    fn show_quick_panel(self: &Rc<Self>) {
        // Cloned out of the cell before anything is called on it. Every
        // borrow of these cells is kept this short on purpose: a window's
        // close handler takes the cell it lives in, and a borrow held across
        // any call that could reach one is a panic waiting for a slow day.
        let existing = self.panel.borrow().clone();
        if let Some(panel) = existing {
            panel.present();
            return;
        }
        let panel = QuickPanel::new(&self.app);
        {
            let this = Rc::downgrade(self);
            panel.set_redraw(Rc::new(move || {
                if let Some(this) = this.upgrade() {
                    this.redraw_panel();
                    // The choice is ours, but everything it changes is the
                    // daemon's answer to a question we have not asked yet.
                    this.refresh_now();
                }
            }));
        }
        {
            let this = Rc::downgrade(self);
            panel.on_close(move || {
                if let Some(this) = this.upgrade() {
                    // Forgotten, not shut down. The poll, the approval
                    // attachment and the daemon are all untouched by this.
                    this.panel.borrow_mut().take();
                }
            });
        }
        panel.render(&self.state.borrow(), &self.selection);
        panel.present();
        *self.panel.borrow_mut() = Some(panel);
        // Opening a surface is a good moment to be current.
        self.refresh_now();
    }

    /// Presents the Settings window, building it the first time.
    fn show_settings(self: &Rc<Self>, page: Option<Page>) {
        let existing = self
            .settings
            .borrow()
            .as_ref()
            .map(|s| (s.window.clone(), s.nav.clone()));
        if let Some((window, nav)) = existing {
            if let Some(page) = page {
                select_page(&nav, page);
            }
            window.present();
            return;
        }
        let settings = self.build_settings(page.unwrap_or(Page::Dashboard));
        settings.window.present();
        *self.settings.borrow_mut() = Some(settings);
        self.refresh_now();
    }

    /// Redraws every window that exists, from the state as it stands.
    fn render(&self) {
        let pages = self.settings.borrow().as_ref().map(|s| s.pages.clone());
        if let Some(pages) = pages {
            pages.render();
        }
        let panel = self.panel.borrow().clone();
        if let Some(panel) = panel {
            panel.render(&self.state.borrow(), &self.selection);
        }
    }

    /// Redraws after a change the daemon knows nothing about — the choice of
    /// device, which lives here and not in the daemon.
    fn redraw_panel(&self) {
        let panel = self.panel.borrow().clone();
        if let Some(panel) = panel {
            panel.redraw_now(&self.state.borrow(), &self.selection);
        }
        let pages = self.settings.borrow().as_ref().map(|s| s.pages.clone());
        if let Some(pages) = pages {
            pages.redraw_now();
        }
    }

    fn refresh_now(&self) {
        let refresh = self.refresh.borrow().clone();
        if let Some(refresh) = refresh {
            refresh();
        }
    }

    /// The poll, owned by the application for its whole life.
    ///
    /// One poll is five exchanges, and nothing is drawn until all five have
    /// answered. Drawing per reply is what rebuilt the whole widget tree five
    /// times every `REFRESH_SECS`; a tree rebuilt underneath an assistive
    /// technology is a control that cannot be activated, because the object it
    /// located no longer exists by the time it acts on it.
    fn install_poll(self: &Rc<Self>) {
        let cycle = Rc::new(RefCell::new(Cycle {
            generation: 0,
            outstanding: 0,
            pending: DaemonState::default(),
        }));

        let refresh: Rc<dyn Fn()> = {
            let this = Rc::downgrade(self);
            let cycle = cycle.clone();
            Rc::new(move || {
                let Some(this) = this.upgrade() else { return };
                let generation = {
                    let mut c = cycle.borrow_mut();
                    c.generation = c.generation.wrapping_add(1);
                    c.outstanding = CYCLE_REQUESTS;
                    // Seeded from what is already known, so one unanswered
                    // request blanks nothing: "not answered yet" and "answered
                    // with nothing" have to stay different states.
                    c.pending = this.state.borrow().clone();
                    c.generation
                };

                let collect = |request: Request, fold: fn(&mut DaemonState, Reply)| {
                    let (this, cycle) = (Rc::downgrade(&this), cycle.clone());
                    client::send(request, move |result| {
                        let Some(this) = this.upgrade() else { return };
                        let last = {
                            let mut c = cycle.borrow_mut();
                            // A reply from an earlier poll is discarded rather
                            // than committed over a newer one. It is also what
                            // stops a daemon that stopped answering from
                            // wedging the loop: the next tick simply starts a
                            // new cycle.
                            if c.generation != generation {
                                return;
                            }
                            fold(&mut c.pending, result);
                            c.outstanding = c.outstanding.saturating_sub(1);
                            c.outstanding == 0
                        };
                        if last {
                            let settled = std::mem::take(&mut cycle.borrow_mut().pending);
                            *this.state.borrow_mut() = settled;
                            this.render();
                        }
                    });
                };

                collect(Request::Status, fold_status);
                collect(Request::Devices, fold_devices);
                collect(Request::Transfers, fold_transfers);
                collect(Request::ClipboardStatus, fold_clipboard);
                collect(Request::NotificationsStatus, fold_notifications);
            })
        };

        *self.refresh.borrow_mut() = Some(refresh.clone());
        refresh();
        glib::timeout_add_seconds_local(REFRESH_SECS as u32, move || {
            refresh();
            glib::ControlFlow::Continue
        });
    }

    /// The management surface: the window this application has always had.
    fn build_settings(self: &Rc<Self>, initial: Page) -> SettingsWindow {
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(160)
            .build();

        let pages = views::Pages::new(&stack, self.state.clone(), self.selection.clone());

        // --- sidebar ----------------------------------------------------
        let nav = gtk::ListBox::new();
        nav.set_selection_mode(gtk::SelectionMode::Single);
        nav.add_css_class("af-nav");
        nav.add_css_class("navigation-sidebar");
        for page in Page::ALL {
            let r = widgets::row(widgets::SPACING_SM);
            r.set_margin_start(8);
            r.set_margin_end(8);
            let icon = gtk::Image::from_icon_name(page.icon());
            icon.set_pixel_size(18);
            r.append(&icon);
            r.append(&gtk::Label::new(Some(page.title())));
            let row = gtk::ListBoxRow::builder().child(&r).build();
            row.update_property(&[gtk::accessible::Property::Label(page.title())]);
            nav.append(&row);
        }
        {
            let stack = stack.clone();
            nav.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    let i = row.index().max(0) as usize;
                    if let Some(page) = Page::ALL.get(i) {
                        stack.set_visible_child_name(page.name());
                    }
                }
            });
        }
        select_page(&nav, initial);
        stack.set_visible_child_name(initial.name());

        let sidebar = widgets::column(0);
        sidebar.add_css_class("af-sidebar");
        let nav_scroll = gtk::ScrolledWindow::builder()
            .child(&nav)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        sidebar.append(&nav_scroll);
        sidebar.append(&pages.sidebar_footer);

        // --- window -------------------------------------------------------
        let content = widgets::column(0);
        let content_scroll = gtk::ScrolledWindow::builder()
            .child(&stack)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        content.append(&content_scroll);
        content.append(&pages.statusbar);
        content.add_css_class("af-content");

        let split = adw::NavigationSplitView::builder()
            .sidebar(
                &adw::NavigationPage::builder()
                    .title("AnyFlow")
                    .child(&sidebar)
                    .build(),
            )
            .content(
                &adw::NavigationPage::builder()
                    .title("AnyFlow")
                    .child(&content)
                    .build(),
            )
            .min_sidebar_width(190.0)
            .max_sidebar_width(240.0)
            .build();

        let header = adw::HeaderBar::new();
        let title_box = widgets::row(widgets::SPACING_XS);
        title_box.append(&widgets::brand_mark(22));
        title_box.append(&gtk::Label::new(Some("AnyFlow")));
        header.set_title_widget(Some(&title_box));

        // The way back to the everyday surface, so the two are not two
        // separate programs that happen to share a name.
        let panel_button = gtk::Button::from_icon_name("view-grid-symbolic");
        panel_button.add_css_class("flat");
        panel_button.set_tooltip_text(Some("Open the AnyFlow Quick Panel"));
        panel_button.update_property(&[gtk::accessible::Property::Label(
            "Open the AnyFlow Quick Panel",
        )]);
        panel_button.set_action_name(Some("app.quick-panel"));
        header.pack_end(&panel_button);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&split));

        let window = adw::ApplicationWindow::builder()
            .application(&self.app)
            .title("AnyFlow Settings")
            .default_width(1000)
            .default_height(680)
            .width_request(360)
            .height_request(420)
            .content(&toolbar)
            .build();
        window.add_css_class("af-root");

        // Narrow windows collapse the sidebar rather than squeezing the
        // content.
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            700.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(breakpoint);

        {
            let this = Rc::downgrade(self);
            window.connect_close_request(move |_| {
                if let Some(this) = this.upgrade() {
                    this.settings.borrow_mut().take();
                }
                glib::Propagation::Proceed
            });
        }

        // Every control calls this once the daemon has answered it.
        {
            let this = Rc::downgrade(self);
            pages.set_refresh(Rc::new(move || {
                if let Some(this) = this.upgrade() {
                    this.refresh_now();
                }
            }));
        }
        {
            let this = Rc::downgrade(self);
            pages.set_redraw(Rc::new(move || {
                if let Some(this) = this.upgrade() {
                    this.redraw_panel();
                }
            }));
        }

        // Draw the empty state once, so the window is not blank while the
        // first poll is in flight.
        pages.render();

        SettingsWindow { window, pages, nav }
    }
}

fn select_page(nav: &gtk::ListBox, page: Page) {
    let index = Page::ALL.iter().position(|p| *p == page).unwrap_or(0) as i32;
    nav.select_row(nav.row_at_index(index).as_ref());
}

/// Which page the sidebar has selected, read back off the widget.
///
/// The inverse of [`select_page`], so a test can assert on what the window
/// shows rather than on what it was asked to show.
#[cfg(test)]
fn selected_page(nav: &gtk::ListBox) -> Option<Page> {
    let row = nav.selected_row()?;
    Page::ALL.get(row.index() as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command line is the seam a desktop shell uses, so what it accepts
    /// is a contract and not an implementation detail.
    #[test]
    fn the_quick_panel_is_reachable_from_the_command_line() {
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--quick-panel"]),
            Launch::QuickPanel
        );
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--panel"]),
            Launch::QuickPanel
        );
    }

    /// A bare launch opens what it has always opened. Changing that under
    /// existing users would be a surprise with nothing to gain by it.
    #[test]
    fn a_bare_launch_still_opens_settings() {
        assert_eq!(Launch::parse(&["anyflow-gui"]), Launch::Settings(None));
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--unknown-to-this-build"]),
            Launch::Settings(None)
        );
    }

    #[test]
    fn a_page_can_still_be_named() {
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--page", "clipboard"]),
            Launch::Settings(Some(Page::Clipboard))
        );
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--page=peers"]),
            Launch::Settings(Some(Page::TrustedPeers))
        );
        // An unknown page is not worth refusing to open a window over.
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--page", "nonsense"]),
            Launch::Settings(None)
        );
    }

    /// `--quick-panel` wins wherever it appears: a tray that also passes a
    /// page must still get the panel.
    #[test]
    fn the_panel_flag_is_not_order_dependent() {
        assert_eq!(
            Launch::parse(&["anyflow-gui", "--page", "files", "--quick-panel"]),
            Launch::QuickPanel
        );
    }

    /// The action names are what a future desktop-shell integration will
    /// activate over `org.gtk.Actions`. Renaming one is a breaking change to
    /// that seam and should fail here first.
    #[test]
    fn the_activation_seam_has_stable_names() {
        assert_eq!(ACTION_QUICK_PANEL, "quick-panel");
        assert_eq!(ACTION_SETTINGS, "settings");
        assert_eq!(APP_ID, "io.github.yurisismotto.anyflow");
    }
}

/// Window and application-action behaviour, checked against real GTK objects.
///
/// Called from `views::display_gate` rather than being `#[test]`s of its own:
/// GTK binds itself to the thread that initialised it and libtest gives every
/// `#[test]` a thread, so a second display test anywhere in this crate panics
/// inside GTK instead of failing on an assertion.
///
/// Deliberately not a window-geometry test. Where GTK decides to put a window
/// and how large a shell lets it be are not this application's decisions, and
/// asserting them would produce a test that fails on somebody else's desktop.
#[cfg(test)]
pub(crate) mod application_gate {
    use super::*;

    /// A registered, throwaway application for one section to build windows
    /// on.
    ///
    /// Registered because GTK refuses to attach a window to an application
    /// that has not emitted `::startup`. `NON_UNIQUE` because a unique one
    /// would single-instance the test against whatever AnyFlow the developer
    /// is running. And `suffix` because even a non-unique GApplication
    /// exports `org.gtk.Application` at an object path derived from its id, so
    /// two of them sharing an id in one process collide on the bus.
    pub(crate) fn test_application(suffix: &str) -> adw::Application {
        let app = adw::Application::builder()
            .application_id(format!("{APP_ID}.{suffix}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gio::Cancellable::NONE)
            .expect("a non-unique application registers locally");
        app
    }

    fn application() -> adw::Application {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NTH: AtomicU32 = AtomicU32::new(0);
        test_application(&format!(
            "WindowTest{}",
            NTH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    pub(crate) fn the_application_window_behaviour() {
        both_surfaces_are_reachable_as_application_actions();
        the_quick_panel_is_created_once_and_presented_again();
        closing_the_quick_panel_leaves_the_state_and_the_settings_window();
        settings_and_the_panel_belong_to_one_application();
        opening_a_surface_starts_no_daemon();
        view_all_transfers_opens_the_existing_settings_window_on_its_files_page();
        opening_the_transfer_list_does_not_change_the_chosen_device();
    }

    /// "View all transfers" opens the Settings window that already exists, on
    /// the page that already exists. It builds no second window, and a second
    /// press does not open another.
    fn view_all_transfers_opens_the_existing_settings_window_on_its_files_page() {
        let app = application();
        let this = App::for_test(&app);

        // Nothing open yet: the action has to be able to open Settings, not
        // merely re-page an open one.
        app.activate_action(ACTION_TRANSFERS, None);
        assert_eq!(app.windows().len(), 1, "no window was opened");
        let opened = this
            .settings
            .borrow()
            .as_ref()
            .map(|s| s.window.clone())
            .expect("the Settings window exists");

        // The Transfers page is the selected one, read back off the sidebar
        // rather than from what we asked for.
        let nav = this
            .settings
            .borrow()
            .as_ref()
            .map(|s| s.nav.clone())
            .expect("the sidebar exists");
        assert_eq!(selected_page(&nav), Some(Page::Files));

        // Pressed again — from a second panel, or twice by a shaky hand — and
        // it is the same window, not a second one.
        for _ in 0..3 {
            app.activate_action(ACTION_TRANSFERS, None);
        }
        assert_eq!(
            app.windows().len(),
            1,
            "a second Settings window was opened"
        );
        assert_eq!(
            this.settings.borrow().as_ref().map(|s| s.window.clone()),
            Some(opened)
        );

        // And it did not open a Quick Panel, or anything else, on the way.
        assert!(this.panel.borrow().is_none());
    }

    /// Looking at what happened is not a decision about what happens next.
    ///
    /// The recent list names the peer each transfer was with, which is not
    /// the peer the next file goes to. Opening the list must not touch the
    /// stored choice in either direction — neither adopting a transfer's peer
    /// nor clearing what was already chosen.
    fn opening_the_transfer_list_does_not_change_the_chosen_device() {
        let app = application();
        let this = App::for_test(&app);

        let before = this.selection.current();
        app.activate_action(ACTION_TRANSFERS, None);
        assert_eq!(
            this.selection.current(),
            before,
            "opening the transfer list moved the selection"
        );

        // Also with a choice already made, which is the case that would
        // actually lose something.
        this.selection.choose(&"ab".repeat(32));
        let chosen = this.selection.current();
        app.activate_action(ACTION_TRANSFERS, None);
        app.activate_action(ACTION_QUICK_PANEL, None);
        app.activate_action(ACTION_TRANSFERS, None);
        assert_eq!(this.selection.current(), chosen);
        this.selection.clear();
    }

    /// The seam a desktop-shell integration will use, asserted on the action
    /// group rather than on the constants that name it.
    fn both_surfaces_are_reachable_as_application_actions() {
        let app = application();
        let _this = App::for_test(&app);
        let mut actions = app.list_actions();
        actions.sort();
        assert!(
            actions.iter().any(|a| a == ACTION_QUICK_PANEL),
            "actions were {actions:?}"
        );
        assert!(actions.iter().any(|a| a == ACTION_SETTINGS));
        // Parameterless, so `app.quick-panel` can be activated by name from a
        // button, a keybinding or the exported `org.gtk.Actions` interface
        // without anyone having to know a parameter type.
        assert_eq!(app.action_parameter_type(ACTION_QUICK_PANEL), None);
        assert_eq!(app.action_parameter_type(ACTION_SETTINGS), None);
    }

    fn the_quick_panel_is_created_once_and_presented_again() {
        let app = application();
        let this = App::for_test(&app);

        this.show_quick_panel();
        let first = this
            .panel
            .borrow()
            .as_ref()
            .map(Rc::as_ptr)
            .expect("a panel exists after the action");
        assert_eq!(app.windows().len(), 1);

        // What a tray item does on every click.
        for _ in 0..3 {
            app.activate_action(ACTION_QUICK_PANEL, None);
        }
        assert_eq!(app.windows().len(), 1, "a second panel was opened");
        assert_eq!(
            this.panel.borrow().as_ref().map(Rc::as_ptr),
            Some(first),
            "the panel was rebuilt rather than presented"
        );
    }

    /// Closing a window forgets a window. It does not stop a poll, drop the
    /// daemon state, or touch the other surface.
    fn closing_the_quick_panel_leaves_the_state_and_the_settings_window() {
        let app = application();
        let this = App::for_test(&app);
        this.show_settings(None);
        this.show_quick_panel();
        assert_eq!(app.windows().len(), 2);

        // The state object the poll writes into, before and after.
        let state = Rc::as_ptr(&this.state);
        let selection = Rc::as_ptr(&this.selection);

        let window = this
            .panel
            .borrow()
            .as_ref()
            .map(|p| p.window.clone())
            .expect("a panel");
        window.close();
        assert!(this.panel.borrow().is_none(), "the panel was not forgotten");
        assert!(
            this.settings.borrow().is_some(),
            "closing the panel closed Settings"
        );
        assert_eq!(
            Rc::as_ptr(&this.state),
            state,
            "the state object was replaced"
        );
        assert_eq!(Rc::as_ptr(&this.selection), selection);

        // And it comes back, wired to the same state rather than to a copy.
        this.show_quick_panel();
        assert!(this.panel.borrow().is_some());
        assert_eq!(Rc::as_ptr(&this.state), state);
    }

    fn settings_and_the_panel_belong_to_one_application() {
        let app = application();
        let this = App::for_test(&app);
        this.show_quick_panel();
        this.show_settings(None);
        for window in app.windows() {
            assert_eq!(
                window.application().map(|a| a.application_id()),
                Some(app.application_id()),
                "a window escaped the application"
            );
        }
        // Two windows, one application, one process. Not two programs that
        // happen to share a name.
        assert_eq!(app.windows().len(), 2);

        // And one *identity*. This is the icon requirement, not a tidiness
        // one: on Wayland a window gets its icon by having its app_id matched
        // against an installed desktop file, and GTK takes that app_id from
        // the application a window belongs to. Two windows on one application
        // therefore cannot resolve to two different icons — and a panel built
        // on an application of its own would be exactly how they could.
        //
        // The id here carries a per-section suffix so the throwaway test
        // applications do not collide on the session bus; what matters is
        // that both windows answer with the *same* one, and that the real
        // application's id is the string the desktop entry is named after.
        let ids: std::collections::BTreeSet<String> = app
            .windows()
            .iter()
            .filter_map(|w| w.application())
            .map(|a| {
                a.application_id()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(
            ids.len(),
            1,
            "the two surfaces have different identities: {ids:?}"
        );
        assert!(
            ids.iter().next().is_some_and(|id| id.starts_with(APP_ID)),
            "the identity is not derived from APP_ID: {ids:?}"
        );
    }

    /// Opening a surface starts nothing in the background.
    ///
    /// The Quick Panel is a view over `anyflowd`; it is not a daemon, it does
    /// not launch one, and there is no second long-lived process anywhere in
    /// this application. Asserted structurally — nothing here spawns, and the
    /// approval attachment is the application's and predates any window.
    fn opening_a_surface_starts_no_daemon() {
        let app = application();
        let this = App::for_test(&app);
        this.show_quick_panel();
        assert!(
            this.approval.borrow().is_none(),
            "a window must not create the daemon attachment; the application does"
        );
        assert!(
            this.refresh.borrow().is_none(),
            "a window must not create the poll; the application does"
        );
    }
}
