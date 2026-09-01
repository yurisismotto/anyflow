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
    Devices,
    TrustedPeers,
    Settings,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Dashboard,
        Page::Files,
        Page::Clipboard,
        Page::Devices,
        Page::TrustedPeers,
        Page::Settings,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::Files => "Files",
            Page::Clipboard => "Clipboard",
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
#[derive(Default)]
pub struct DaemonState {
    pub status: Option<anyflow_control::StatusReport>,
    pub devices: Option<Vec<anyflow_control::DeviceReport>>,
    pub transfers: Option<Vec<anyflow_control::TransferReport>>,
    pub clipboard: Option<anyflow_control::ClipboardStatusReport>,
    /// Set when the daemon could not be reached at all.
    pub error: Option<String>,
}

impl DaemonState {
    pub fn reachable(&self) -> bool {
        self.error.is_none() && self.status.is_some()
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
    let refresh = {
        let state = state.clone();
        let pages = pages.clone();
        move || {
            let (state1, pages1) = (state.clone(), pages.clone());
            let (state2, pages2) = (state.clone(), pages.clone());
            let (state3, pages3) = (state.clone(), pages.clone());
            let (state4, pages4) = (state.clone(), pages.clone());

            client::send(Request::Status, move |result| {
                {
                    let mut s = state1.borrow_mut();
                    match result {
                        Ok(Response::Status(report)) => {
                            s.status = Some(report);
                            s.error = None;
                        }
                        Ok(Response::Error { message }) => s.error = Some(message),
                        Ok(_) => s.error = Some("unexpected reply from the daemon".into()),
                        Err(e) => s.error = Some(e.to_string()),
                    }
                }
                pages1.render();
            });

            client::send(Request::Devices, move |result| {
                if let Ok(Response::Devices(list)) = result {
                    state2.borrow_mut().devices = Some(list);
                    pages2.render();
                }
            });

            client::send(Request::Transfers, move |result| {
                if let Ok(Response::Transfers(list)) = result {
                    state3.borrow_mut().transfers = Some(list);
                    pages3.render();
                }
            });

            client::send(Request::ClipboardStatus, move |result| {
                if let Ok(Response::Clipboard(report)) = result {
                    state4.borrow_mut().clipboard = Some(report);
                    pages4.render();
                }
            });
        }
    };
    refresh();
    glib::timeout_add_seconds_local(REFRESH_SECS as u32, move || {
        refresh();
        glib::ControlFlow::Continue
    });

    window.present();
}
