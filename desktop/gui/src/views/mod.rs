//! The pages, and what keeps them in step with the daemon.
//!
//! A page is rebuilt wholesale from [`DaemonState`], which with a handful of
//! devices and transfers is far cheaper in bugs than a per-widget diffing
//! layer: what is on screen stays a pure function of what the daemon last
//! said, and no stale widget can survive a state change.
//!
//! What that originally left out is *when*. Every page was rebuilt on every
//! control-socket reply — five rebuilds per two-second interval by the time
//! `notifications.v1` added a fifth request — and a widget tree rebuilt five
//! times a second is not something an assistive technology can operate. AT-SPI
//! locates a control and activates it in two separate round trips; if the tree
//! was rebuilt in between, the activation is delivered to an object that has
//! been destroyed, and a screen-reader user loses their place for the same
//! reason. Measured during the N3 desktop gate: activations that never reached
//! the daemon at `REFRESH_SECS = 2`, landing first time only when the interval
//! was stretched to 45 seconds.
//!
//! So the rebuild is now conditional, and the condition is the data:
//!
//! * one poll produces one state update (see `Cycle` in `lib.rs`), so a
//!   refresh can rebuild at most once rather than once per reply;
//! * [`Pages::render`] rebuilds a page only when the part of [`DaemonState`]
//!   that page reads has actually changed.
//!
//! Together those give the property the accessibility fix needs — *unchanged
//! data leaves the widget tree untouched* — without a diffing layer and
//! without any special case for accessibility. It is not a notifications fix:
//! every page gets it, and the pages that carry controls (Notifications,
//! Trusted peers, Clipboard) are the ones it matters most for.

mod clipboard;
mod dashboard;
mod devices;
mod files;
mod notifications;
mod pairing;
mod peers;
mod settings;

pub use notifications::Readiness;
pub use pairing::present_pairing_dialog;

use adw::prelude::*;
use anyflow_control::{
    ClipboardStatusReport, NotificationsStatusReport, StatusReport, TransferReport,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::widgets::{self, SPACING_MD, SPACING_SM, SPACING_XS};
use crate::{DaemonState, Page};

/// Handles to every page, so a refresh can redraw them all.
///
/// Cheap to clone: GTK widgets are reference-counted handles, and the state
/// is behind an `Rc`.
#[derive(Clone)]
pub struct Pages {
    pub state: Rc<RefCell<DaemonState>>,
    /// What each page was last drawn from. See [`Pages::render`].
    drawn: Rc<RefCell<Drawn>>,
    /// Asks the daemon again. Installed by `build_window`, because the poll it
    /// triggers has to hold a `Pages` of its own.
    refresh: Refresh,
    dashboard: gtk::Box,
    files: gtk::Box,
    clipboard: gtk::Box,
    notifications: gtk::Box,
    devices: gtk::Box,
    peers: gtk::Box,
    settings: gtk::Box,
    /// The identity and network block pinned to the bottom of the sidebar.
    pub sidebar_footer: gtk::Box,
    /// "Secure connection … Local network" along the bottom of the window.
    pub statusbar: gtk::Box,
}

/// The slices of [`DaemonState`] each page was last drawn from.
///
/// Held by value rather than as a hash: the reports are small, `PartialEq` on
/// them is exact, and a hash would trade a real comparison for a collision
/// nobody would ever debug.
#[derive(Default)]
struct Drawn {
    /// `false` until the first draw, which is therefore unconditional.
    any: bool,
    status: Option<StatusReport>,
    transfers: Option<Vec<TransferReport>>,
    clipboard: Option<ClipboardStatusReport>,
    notifications: Option<NotificationsStatusReport>,
    error: Option<String>,
}

/// The poll a control triggers after the daemon has answered it.
///
/// Late-bound because the poll has to hold a [`Pages`] of its own: the cell is
/// filled in by `build_window` once both exist.
type Refresh = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Which slices differ from [`Drawn`] this time round.
struct Changed {
    status: bool,
    transfers: bool,
    clipboard: bool,
    notifications: bool,
    error: bool,
}

impl Changed {
    fn anything(&self) -> bool {
        self.status || self.transfers || self.clipboard || self.notifications || self.error
    }
}

fn page_box() -> gtk::Box {
    let b = widgets::column(SPACING_SM);
    b.set_margin_top(SPACING_MD);
    b.set_margin_bottom(SPACING_MD);
    b.set_margin_start(SPACING_MD);
    b.set_margin_end(SPACING_MD);
    b
}

impl Pages {
    pub fn new(stack: &gtk::Stack, state: Rc<RefCell<DaemonState>>) -> Self {
        let pages = Pages {
            state,
            drawn: Rc::new(RefCell::new(Drawn::default())),
            refresh: Rc::new(RefCell::new(None)),
            dashboard: page_box(),
            files: page_box(),
            clipboard: page_box(),
            notifications: page_box(),
            devices: page_box(),
            peers: page_box(),
            settings: page_box(),
            sidebar_footer: widgets::column(SPACING_XS),
            statusbar: widgets::row(SPACING_XS),
        };
        stack.add_named(&pages.dashboard, Some(Page::Dashboard.name()));
        stack.add_named(&pages.files, Some(Page::Files.name()));
        stack.add_named(&pages.clipboard, Some(Page::Clipboard.name()));
        stack.add_named(&pages.notifications, Some(Page::Notifications.name()));
        stack.add_named(&pages.devices, Some(Page::Devices.name()));
        stack.add_named(&pages.peers, Some(Page::TrustedPeers.name()));
        stack.add_named(&pages.settings, Some(Page::Settings.name()));

        pages.sidebar_footer.set_margin_start(SPACING_SM);
        pages.sidebar_footer.set_margin_end(SPACING_SM);
        pages.sidebar_footer.set_margin_bottom(SPACING_SM);
        pages.statusbar.add_css_class("af-statusbar");
        pages
    }

    /// Installs the poll that [`Pages::refresh_now`] triggers.
    pub fn set_refresh(&self, refresh: Rc<dyn Fn()>) {
        *self.refresh.borrow_mut() = Some(refresh);
    }

    /// Asks the daemon again, now.
    ///
    /// What a control calls once the daemon has answered the change it
    /// requested. Deliberately not [`Pages::render`]: the daemon is the
    /// authority and the state here is only what it last said, so redrawing
    /// would redraw the answer to the previous question — and if the daemon
    /// *refused* the change, redrawing stale state is exactly how a switch
    /// comes to disagree with the thing it controls.
    pub fn refresh_now(&self) {
        // Cloned out of the cell before the call: a refresh may complete
        // synchronously and ask for another one.
        let refresh = self.refresh.borrow().clone();
        if let Some(refresh) = refresh {
            refresh();
        }
    }

    /// Redraws the pages whose data changed, and only those.
    ///
    /// Each page reads a known slice of [`DaemonState`]; a page whose slice is
    /// equal to what it was last drawn from is left alone entirely, down to
    /// the widget objects, so a control an assistive technology has located is
    /// still the same object when it is activated. See the module docs.
    pub fn render(&self) {
        let state = self.state.borrow();

        let changed = {
            let drawn = self.drawn.borrow();
            // The first draw is unconditional: with an unreachable daemon
            // every slice is `None` on both sides, and the pages would stay
            // blank instead of saying so.
            let first = !drawn.any;
            Changed {
                status: first || drawn.status != state.status,
                transfers: first || drawn.transfers != state.transfers,
                clipboard: first || drawn.clipboard != state.clipboard,
                notifications: first || drawn.notifications != state.notifications,
                error: first || drawn.error != state.error,
            }
        };
        if !changed.anything() {
            return;
        }

        // Recorded before drawing, not after: building a page connects
        // handlers, and a handler that asks for a render must compare against
        // what is going on screen rather than against what it replaced.
        {
            let mut drawn = self.drawn.borrow_mut();
            drawn.any = true;
            if changed.status {
                drawn.status = state.status.clone();
            }
            if changed.transfers {
                drawn.transfers = state.transfers.clone();
            }
            if changed.clipboard {
                drawn.clipboard = state.clipboard.clone();
            }
            if changed.notifications {
                drawn.notifications = state.notifications.clone();
            }
            if changed.error {
                drawn.error = state.error.clone();
            }
        }

        if changed.status || changed.transfers || changed.error {
            dashboard::render(&self.dashboard, &state, self);
        }
        if changed.transfers {
            files::render(&self.files, &state);
        }
        if changed.clipboard {
            clipboard::render(&self.clipboard, &state, self);
        }
        if changed.notifications {
            notifications::render(&self.notifications, &state, self);
        }
        if changed.status {
            devices::render(&self.devices, &state);
            peers::render(&self.peers, &state, self);
            settings::render(&self.settings, &state);
        }
        if changed.status || changed.error {
            self.render_sidebar_footer(&state);
            self.render_statusbar(&state);
        }
    }

    fn render_sidebar_footer(&self, state: &DaemonState) {
        widgets::clear(&self.sidebar_footer);

        let net = widgets::row(SPACING_XS);
        net.add_css_class("af-card-sunken");
        let icon = gtk::Image::from_icon_name(if state.reachable() {
            "security-high-symbolic"
        } else {
            "security-low-symbolic"
        });
        icon.set_pixel_size(16);
        icon.add_css_class(if state.reachable() {
            "af-status-connected"
        } else {
            "af-status-disconnected"
        });
        net.append(&icon);
        let text = widgets::column(0);
        text.append(&widgets::caption(if state.reachable() {
            "Network"
        } else {
            "Daemon"
        }));
        let value = widgets::caption(if state.reachable() {
            "Local network"
        } else {
            "Not running"
        });
        value.add_css_class("af-text-primary");
        text.append(&value);
        net.append(&text);
        self.sidebar_footer.append(&net);

        // This machine's own identity. The fingerprint is here rather than
        // buried in Settings because it is what someone reads out while
        // pairing from the other side.
        if let Some(status) = &state.status {
            let me = widgets::row(SPACING_XS);
            me.append(&widgets::brand_mark(20));
            let text = widgets::column(0);
            let name = widgets::caption(&status.device_name);
            name.add_css_class("af-text-primary");
            text.append(&name);
            let fp = widgets::caption(&status.fingerprint_short);
            fp.add_css_class("af-mono");
            text.append(&fp);
            me.append(&text);
            self.sidebar_footer.append(&me);
        }
    }

    /// The bottom strip.
    ///
    /// Deliberately not "end-to-end encrypted". The link is a direct,
    /// mutually authenticated TLS 1.3 session pinned to the key approved at
    /// pairing, and that is what this says — in the project's own terms
    /// rather than a marketing phrase whose meaning does not exactly match.
    fn render_statusbar(&self, state: &DaemonState) {
        widgets::clear(&self.statusbar);

        let left = widgets::row(SPACING_XS);
        let (icon_name, class, text, detail) = if state.reachable() {
            (
                "security-high-symbolic",
                "af-status-connected",
                "Secure connection",
                "TLS 1.3 with ALPN anyflow/1, mutually authenticated and pinned to the \
                 key you approved when pairing. Direct on your network — no relay, no cloud.",
            )
        } else {
            (
                "security-low-symbolic",
                "af-status-disconnected",
                "Daemon not reachable",
                "Start the AnyFlow daemon to connect to your devices.",
            )
        };
        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(14);
        icon.add_css_class(class);
        left.append(&icon);
        let label = widgets::caption(text);
        label.add_css_class(class);
        left.append(&label);
        left.set_tooltip_text(Some(detail));
        self.statusbar.append(&left);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        self.statusbar.append(&spacer);

        if let Some(status) = &state.status {
            self.statusbar.append(&widgets::caption(&format!(
                "Local network · port {} · {}",
                status.listen_port, status.listen_families
            )));
        }
    }
}
