//! AnyFlow for the desktop.
//!
//! A GTK4 / libadwaita front end for the daemon. It is a *client* of the same
//! local control socket the `anyflow` CLI uses and adds no protocol, no
//! capability and no privilege of its own: everything on screen is something
//! the daemon already reports, and every action is a request the CLI can make
//! too. See `desktop/daemon/src/control.rs`.
//!
//! Deliberately not an Electron app and not a web view. This is a Linux
//! desktop program: it honours the system font, the system colour scheme and
//! the system's accessibility settings because it is built out of the
//! platform's own widgets.

pub mod approval;
pub mod client;
pub mod theme;
pub mod views;
pub mod widgets;

use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use anyflow_control::{Request, Response};

const APP_ID: &str = "io.github.yurisismotto.anyflow";

/// How often the daemon is asked for fresh state.
///
/// The control socket cannot push, so this polls. Two seconds is chosen to
/// feel live without being wasteful: everything requested is metadata the
/// daemon already holds in memory, and no clipboard content is ever among it.
const REFRESH_SECS: u64 = 2;

/// Starts the application.
pub fn run() -> glib::ExitCode {
    gtk::gio::resources_register_include!("anyflow.gresource")
        .expect("the compiled-in resources should load");

    // `--page <name>` opens straight to one page. It exists so the UI can be
    // driven without a pointer — for a screenshot pass, or simply to land
    // where you meant to. GApplication is given an empty argv so it does not
    // reject an option it knows nothing about.
    let initial = std::env::args()
        .skip_while(|a| a != "--page")
        .nth(1)
        .and_then(|name| Page::from_name(&name))
        .unwrap_or(Page::Dashboard);

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        adw::init().expect("libadwaita should initialise");
        install_styles();
    });
    app.connect_activate(move |app| build_window(app, initial));
    app.run_with_args::<&str>(&[])
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

/// Which page the content pane is showing.
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
/// `PartialEq` is what lets [`views::Pages::render`] tell an unchanged poll
/// from a real change, and it is a property of the control types rather than
/// of this struct: every field is a report `anyflow-control` defines, so two
/// states are equal exactly when the daemon said the same thing twice.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct DaemonState {
    pub status: Option<anyflow_control::StatusReport>,
    pub devices: Option<Vec<anyflow_control::DeviceReport>>,
    pub transfers: Option<Vec<anyflow_control::TransferReport>>,
    pub clipboard: Option<anyflow_control::ClipboardStatusReport>,
    /// Counts, states and platform identifiers. **No field on this report can
    /// hold a notification's title, body or application name**, which is what
    /// makes the notifications page structurally incapable of becoming the
    /// history the design forbids.
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

fn build_window(app: &adw::Application, initial: Page) {
    let state: Rc<RefCell<DaemonState>> = Rc::new(RefCell::new(DaemonState::default()));

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(160)
        .build();

    let pages = views::Pages::new(&stack, state.clone());

    // --- sidebar --------------------------------------------------------
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
    let initial_index = Page::ALL.iter().position(|p| *p == initial).unwrap_or(0) as i32;
    nav.select_row(nav.row_at_index(initial_index).as_ref());
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

    // --- window ---------------------------------------------------------
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

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&split));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("AnyFlow")
        .default_width(1000)
        .default_height(680)
        .width_request(360)
        .height_request(420)
        .content(&toolbar)
        .build();
    window.add_css_class("af-root");

    // Narrow windows collapse the sidebar rather than squeezing the content.
    // The reference is a wide desktop layout; this is what stops it from
    // becoming unusable at half that width.
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(breakpoint);

    // --- refresh loop ---------------------------------------------------
    //
    // One poll is five exchanges, and nothing is drawn until all five have
    // answered. Drawing per reply is what rebuilt the whole widget tree five
    // times every `REFRESH_SECS`; a tree rebuilt underneath an assistive
    // technology is a control that cannot be activated, because the object it
    // located no longer exists by the time it acts on it.
    let cycle = Rc::new(RefCell::new(Cycle {
        generation: 0,
        outstanding: 0,
        pending: DaemonState::default(),
    }));

    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let pages = pages.clone();
        let cycle = cycle.clone();
        Rc::new(move || {
            let generation = {
                let mut c = cycle.borrow_mut();
                c.generation = c.generation.wrapping_add(1);
                c.outstanding = CYCLE_REQUESTS;
                // Seeded from what is already known, so one unanswered
                // request blanks nothing: "not answered yet" and "answered
                // with nothing" have to stay different states.
                c.pending = state.borrow().clone();
                c.generation
            };

            let collect = |request: Request, fold: fn(&mut DaemonState, Reply)| {
                let (state, pages, cycle) = (state.clone(), pages.clone(), cycle.clone());
                client::send(request, move |result| {
                    let last = {
                        let mut c = cycle.borrow_mut();
                        // A reply from an earlier poll is discarded rather
                        // than committed over a newer one. It is also what
                        // stops a daemon that stopped answering from wedging
                        // the loop: the next tick simply starts a new cycle.
                        if c.generation != generation {
                            return;
                        }
                        fold(&mut c.pending, result);
                        c.outstanding = c.outstanding.saturating_sub(1);
                        c.outstanding == 0
                    };
                    if last {
                        let settled = std::mem::take(&mut cycle.borrow_mut().pending);
                        *state.borrow_mut() = settled;
                        pages.render();
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

    // Every control calls this once the daemon has answered it — see
    // `Pages::refresh_now`.
    pages.set_refresh(refresh.clone());

    // The incoming-file approval provider.
    //
    // Owned by the *window*, not by this scope: `build_window` returns
    // immediately, and a handle dropped here would detach the moment the
    // window appeared. The signal handler below holds it, so it lives exactly
    // as long as the window does and detaches when the window goes — which
    // returns the daemon to declining every file, the safe direction and the
    // behaviour a desktop with no GUI has always had.
    let approval = RefCell::new(Some(approval::install(&window)));
    window.connect_destroy(move |_| {
        approval.borrow_mut().take();
    });

    // Draw the empty state once, so the window is not blank while the first
    // poll is in flight.
    pages.render();
    refresh();
    glib::timeout_add_seconds_local(REFRESH_SECS as u32, move || {
        refresh();
        glib::ControlFlow::Continue
    });

    window.present();
}
