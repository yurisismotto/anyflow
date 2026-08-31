//! The pages, and what keeps them in step with the daemon.
//!
//! Each page re-renders wholesale from [`DaemonState`] on every refresh. With
//! a handful of devices and transfers that is far cheaper in bugs than a
//! diffing layer, and it makes what is on screen a pure function of what the
//! daemon last said — there is no path by which a stale widget can survive a
//! state change.

mod clipboard;
mod dashboard;
mod devices;
mod files;
mod pairing;
mod peers;
mod settings;

pub use pairing::present_pairing_dialog;

use adw::prelude::*;
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
    dashboard: gtk::Box,
    files: gtk::Box,
    clipboard: gtk::Box,
    devices: gtk::Box,
    peers: gtk::Box,
    settings: gtk::Box,
    /// The identity and network block pinned to the bottom of the sidebar.
    pub sidebar_footer: gtk::Box,
    /// "Secure connection … Local network" along the bottom of the window.
    pub statusbar: gtk::Box,
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
            dashboard: page_box(),
            files: page_box(),
            clipboard: page_box(),
            devices: page_box(),
            peers: page_box(),
            settings: page_box(),
            sidebar_footer: widgets::column(SPACING_XS),
            statusbar: widgets::row(SPACING_XS),
        };
        stack.add_named(&pages.dashboard, Some(Page::Dashboard.name()));
        stack.add_named(&pages.files, Some(Page::Files.name()));
        stack.add_named(&pages.clipboard, Some(Page::Clipboard.name()));
        stack.add_named(&pages.devices, Some(Page::Devices.name()));
        stack.add_named(&pages.peers, Some(Page::TrustedPeers.name()));
        stack.add_named(&pages.settings, Some(Page::Settings.name()));

        pages.sidebar_footer.set_margin_start(SPACING_SM);
        pages.sidebar_footer.set_margin_end(SPACING_SM);
        pages.sidebar_footer.set_margin_bottom(SPACING_SM);
        pages.statusbar.add_css_class("af-statusbar");
        pages
    }

    /// Redraws every page from the current state.
    pub fn render(&self) {
        let state = self.state.borrow();
        dashboard::render(&self.dashboard, &state, self);
        files::render(&self.files, &state);
        clipboard::render(&self.clipboard, &state, self);
        devices::render(&self.devices, &state);
        peers::render(&self.peers, &state, self);
        settings::render(&self.settings, &state);
        self.render_sidebar_footer(&state);
        self.render_statusbar(&state);
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
