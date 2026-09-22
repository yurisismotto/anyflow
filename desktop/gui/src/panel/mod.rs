//! The Quick Panel — the everyday surface.
//!
//! ```text
//! ┌────────────────────────────────────┐
//! │ (A) OmniBridge                     ⚙  │
//! │     One bridge. Any device.          │
//! │                                    │
//! │  ● SM-X620                      ✓  │
//! │    Connected · 78%                 │
//! │    Clipboard · Files · Notifications│
//! │                                    │
//! │  [ Send file ]  [ Send clipboard ] │
//! │                                    │
//! │  Notifications                  On │
//! │  Clipboard                      On │
//! │  Files                          On │
//! │                                    │
//! │  Open OmniBridge Settings             │
//! └────────────────────────────────────┘
//! ```
//!
//! # What it is
//!
//! A *view and a controller* over operations the daemon has already
//! authorised. It opens no socket of its own beyond the control client the
//! rest of the application uses, starts no process, holds no state the
//! Settings window cannot see, and adds no authority: every button here ends
//! in a control request the `omnibridge` CLI could make by hand.
//!
//! # What it deliberately is not
//!
//! Not a dashboard, not a second daemon, and **not a history**. There is no
//! list of past clips, past notifications or past files anywhere in this
//! module. The only content that ever appears is the filename of a transfer
//! that is moving at that moment, which is an active transfer surface and
//! vanishes when it finishes.
//!
//! # Where the decisions live
//!
//! Not here. [`model::PanelModel`] decides what this draws and what each
//! button would do, on plain data with no GTK in it, and is tested without a
//! display. This file turns that into widgets and turns clicks back into the
//! requests the model composed. If a rule appears in this file, it is in the
//! wrong file.

pub mod model;

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use omnibridge_control::Response;

use crate::selection::Selection;
use crate::widgets::{self, Status, SPACING_MD, SPACING_SM, SPACING_XS};
use crate::{client, DaemonState};
use model::{Action, Battery, Health, Link, PanelModel, StatusValue};

/// Wide enough for a device name and two buttons, narrow enough to read as a
/// utility rather than a window. Height follows the content.
const WIDTH: i32 = 380;

/// The panel window and what it was last drawn from.
///
/// Held by the application, not by the window, so that closing the panel
/// leaves the poll — and the Settings window, and the file-approval
/// attachment — entirely alone.
pub struct QuickPanel {
    pub(crate) window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    content: gtk::Box,
    /// Asks the application to redraw this panel from the current state.
    ///
    /// Late-bound because the application owns the poll and this owns the
    /// window: the cell is filled in once both exist.
    redraw: RefCell<Option<Rc<dyn Fn()>>>,
    /// The model the widgets currently represent.
    ///
    /// Redrawing only on a real change is not a micro-optimisation: a widget
    /// tree rebuilt underneath an assistive technology is a control that
    /// cannot be activated, because AT-SPI locates an object and activates it
    /// in two separate round trips. `views/mod.rs` documents the measurement
    /// that established this; the panel inherits the rule.
    drawn: RefCell<Option<PanelModel>>,
}

impl QuickPanel {
    /// Builds the panel. It is presented by [`QuickPanel::present`].
    pub fn new(app: &adw::Application) -> Rc<QuickPanel> {
        let content = widgets::column(SPACING_SM);
        content.set_margin_top(SPACING_SM);
        content.set_margin_bottom(SPACING_MD);
        content.set_margin_start(SPACING_MD);
        content.set_margin_end(SPACING_MD);

        let scroll = gtk::ScrolledWindow::builder()
            .child(&content)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            // Past this the panel would stop being a panel. Content beyond
            // it scrolls rather than growing the window off the screen.
            .max_content_height(640)
            .build();

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&scroll));

        let header = adw::HeaderBar::new();
        header.add_css_class("flat");
        let title = adw::WindowTitle::new("OmniBridge", "One bridge. Any device.");
        header.set_title_widget(Some(&title));

        let mark = widgets::brand_mark(20);
        mark.set_margin_start(SPACING_XS);
        header.pack_start(&mark);

        // Icon-only, and therefore labelled. An unlabelled gear is a control
        // a screen reader reads out as "button".
        // The same cog the sidebar's Settings row uses, and the same one the
        // "Open OmniBridge Settings" button below carries: one action, one
        // metaphor. See `widgets::SETTINGS_ICON`.
        let settings = gtk::Button::from_icon_name(widgets::SETTINGS_ICON);
        settings.add_css_class("flat");
        settings.set_tooltip_text(Some("Open OmniBridge Settings"));
        settings.update_property(&[gtk::accessible::Property::Label("Open OmniBridge Settings")]);
        settings.set_action_name(Some("app.settings"));
        header.pack_end(&settings);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&toasts));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("OmniBridge")
            .default_width(WIDTH)
            .width_request(WIDTH)
            .resizable(false)
            .content(&toolbar)
            .build();
        window.add_css_class("ob-root");
        window.add_css_class("ob-quick-panel");

        Rc::new(QuickPanel {
            window,
            toasts,
            content,
            redraw: RefCell::new(None),
            drawn: RefCell::new(None),
        })
    }

    /// Installs the redraw the application performs when the *choice*
    /// changes — something the daemon knows nothing about, so waiting for the
    /// next poll would leave a click unacknowledged for up to two seconds.
    pub fn set_redraw(&self, redraw: Rc<dyn Fn()>) {
        *self.redraw.borrow_mut() = Some(redraw);
    }

    /// Brings the panel to the front, creating nothing and starting nothing.
    ///
    /// Idempotent on purpose: the future tray item will call this every time
    /// someone clicks it, and a second click must raise the panel rather than
    /// open a second one.
    pub fn present(&self) {
        self.window.present();
    }

    /// Runs `f` when the panel is closed, so the application can forget it.
    pub fn on_close(self: &Rc<Self>, f: impl Fn() + 'static) {
        self.window.connect_close_request(move |_| {
            f();
            // Let the window close. Nothing else stops: the daemon is a
            // separate process, the poll belongs to the application, and no
            // trust is touched by a window going away.
            gtk::glib::Propagation::Proceed
        });
    }

    /// Redraws from the latest poll, if anything the panel shows has changed.
    pub fn render(self: &Rc<Self>, state: &DaemonState, selection: &Rc<Selection>) {
        let model = PanelModel::build(state, selection.current().as_deref());
        if self.drawn.borrow().as_ref() == Some(&model) {
            return;
        }
        *self.drawn.borrow_mut() = Some(model.clone());
        self.draw(&model, selection);
    }

    /// Forces a redraw from the last state — used when the *choice* changes,
    /// which is a change the daemon knows nothing about.
    pub fn redraw_now(self: &Rc<Self>, state: &DaemonState, selection: &Rc<Selection>) {
        self.drawn.borrow_mut().take();
        self.render(state, selection);
    }

    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn draw(self: &Rc<Self>, model: &PanelModel, selection: &Rc<Selection>) {
        widgets::clear(&self.content);

        match &model.health {
            Health::Available => {}
            Health::Reaching => {
                self.content.append(&widgets::body_muted(
                    "Connecting to the OmniBridge service…",
                ));
            }
            Health::Unavailable { headline } => {
                self.content.append(&widgets::security_notice(
                    headline,
                    "OmniBridge keeps trying. Your devices stay paired, and nothing is lost.",
                    true,
                ));
            }
        }

        if model.health.is_available() {
            if model.peers.is_empty() {
                let empty = widgets::body_muted(
                    "No device is paired yet. Pair one from OmniBridge Settings.",
                );
                self.content.append(&empty);
            } else {
                self.content.append(&self.peer_list(model, selection));
            }

            if let model::Target::MustChoose { stale_choice } = &model.target {
                let note = widgets::caption(if *stale_choice {
                    "The device you had chosen is no longer paired. Choose one above."
                } else {
                    "Choose a device above to send to it."
                });
                self.content.append(&note);
            }

            self.content.append(&self.actions(model));

            if let Some(transfer) = &model.transfer {
                let line = widgets::row(SPACING_XS);
                line.append(&widgets::status_badge(Status::Transferring));
                line.append(&widgets::caption(&transfer.label()));
                line.set_tooltip_text(Some(&format!(
                    "{} {} {}",
                    transfer.label(),
                    if transfer.outgoing { "to" } else { "from" },
                    transfer.peer_name
                )));
                self.content.append(&line);
            }

            if !model.recent.is_empty() {
                self.content.append(&recent_section(&model.recent));
            }

            self.content.append(&widgets::separator());
            self.content.append(&status_row(
                "Notifications",
                "preferences-system-notifications-symbolic",
                &model.notifications,
            ));
            self.content.append(&status_row(
                "Clipboard",
                "edit-paste-symbolic",
                &model.clipboard,
            ));
            self.content
                .append(&status_row("Files", "folder-symbolic", &model.files));
        }

        self.content.append(&widgets::separator());
        let settings =
            widgets::secondary_button("Open OmniBridge Settings", Some(widgets::SETTINGS_ICON));
        settings.set_action_name(Some("app.settings"));
        self.content.append(&settings);
    }

    /// The device rows.
    ///
    /// A `GtkListBox` because it is what the rest of this application uses for
    /// a list of devices, with a radio button per row when there is a choice
    /// to make.
    ///
    /// The radio is not decoration and it is not the first thing that was
    /// tried. Selection started as the list box's own — `SelectionMode::Single`
    /// and the chosen row selected — which failed on the real desktop in two
    /// ways at once, both found by reading the accessibility tree of the
    /// running panel rather than by reasoning about it:
    ///
    /// 1. A `GtkListBox` in single-selection mode selects a row by itself when
    ///    the list first takes keyboard focus, which a freshly opened panel
    ///    does. With no choice made, the first device announced itself as the
    ///    selected one while the panel said "choose a device above" and both
    ///    actions were correctly disabled. `unselect_all()` does not hold — the
    ///    focus handler simply selects again. Nothing routed wrongly, because a
    ///    destination comes from the stored fingerprint and never from a
    ///    widget, but a screen reader was being told the opposite of what the
    ///    screen said.
    /// 2. An activatable `GtkListBoxRow` exposes **no AT-SPI action**, so a
    ///    row was something an assistive technology could read and could not
    ///    operate. Measured: `get_action_iface()` reports zero actions on it.
    ///
    /// A `GtkCheckButton` in radio mode fixes both. It has a real checked
    /// state that is set from the model and that GTK never changes on its own,
    /// it is a first-class keyboard control, and it exposes the action an
    /// assistive technology needs to *make* the choice rather than only
    /// observe it.
    fn peer_list(self: &Rc<Self>, model: &PanelModel, selection: &Rc<Selection>) -> gtk::ListBox {
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        // With one trusted peer there is nothing to choose between, so no
        // selector is forced on the person; the row is still a row.
        let choosing = model.peers.len() > 1;
        // The widget therefore never holds selection state of its own. What
        // is selected is what the radios say, and they say what the model
        // says.
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_accessible_role(gtk::AccessibleRole::ListBox);
        list.update_property(&[gtk::accessible::Property::Label(if choosing {
            "Devices. Choose which one OmniBridge sends to."
        } else {
            "Devices"
        })]);

        let mut radios: Vec<gtk::CheckButton> = Vec::new();
        for peer in &model.peers {
            let radio = choosing.then(|| {
                let radio = gtk::CheckButton::new();
                if let Some(leader) = radios.first() {
                    radio.set_group(Some(leader));
                }
                radio.set_valign(gtk::Align::Center);
                // Named for what pressing it does, not for what it is.
                let label = format!("Send to {}", peer.name);
                radio.set_tooltip_text(Some(&label));
                radio.update_property(&[gtk::accessible::Property::Label(&label)]);
                // Set before any handler is connected, below, so rebuilding
                // the list cannot write the choice back to itself.
                radio.set_active(peer.selected);
                radios.push(radio.clone());
                radio
            });

            let row = gtk::ListBoxRow::builder()
                .child(&peer_card(peer, radio.as_ref()))
                .build();
            // Clicking anywhere on the row is the same as pressing its radio.
            row.set_activatable(choosing);
            // Never selectable: see the note above.
            row.set_selectable(false);
            // Everything the row says visually, said once as a phrase.
            row.update_property(&[gtk::accessible::Property::Label(&peer.announcement())]);
            list.append(&row);
        }

        if choosing {
            // Connected only now that every radio holds the model's answer:
            // a handler attached earlier would fire while the list was still
            // being built and write a choice nobody made.
            for (radio, peer) in radios.iter().zip(&model.peers) {
                let panel = Rc::downgrade(self);
                let selection = selection.clone();
                // The *fingerprint* is what is stored. Nothing downstream can
                // see this row's position, and nothing reads its name.
                let fingerprint = peer.fingerprint.clone();
                radio.connect_toggled(move |radio| {
                    if !radio.is_active() {
                        return;
                    }
                    selection.choose(&fingerprint);
                    if let Some(panel) = panel.upgrade() {
                        panel.request_redraw();
                    }
                });
            }

            let radios = radios.clone();
            list.connect_row_activated(move |_, row| {
                let index = row.index().max(0) as usize;
                // Through the radio rather than straight to the selection, so
                // a click and a keypress take exactly one path.
                if let Some(radio) = radios.get(index) {
                    radio.set_active(true);
                }
            });
        }
        list
    }

    fn request_redraw(&self) {
        // Cloned out of the cell before the call: the redraw re-enters this
        // object to draw, and holding the borrow across it would panic.
        let redraw = self.redraw.borrow().clone();
        if let Some(redraw) = redraw {
            redraw();
        }
    }

    fn actions(self: &Rc<Self>, model: &PanelModel) -> gtk::Box {
        let row = widgets::row(SPACING_XS);
        row.set_homogeneous(true);

        let send_file = widgets::cta_button("Send file", Some("document-send-symbolic"));
        bind(&send_file, &model.send_file, "Send file");
        if model.send_file.is_ready() {
            let panel = Rc::downgrade(self);
            let action = model.send_file.clone();
            send_file.connect_clicked(move |button| {
                if let Some(panel) = panel.upgrade() {
                    panel.choose_and_send(button, &action);
                }
            });
        }
        row.append(&send_file);

        let send_clipboard =
            widgets::secondary_button("Send clipboard", Some("edit-paste-symbolic"));
        bind(&send_clipboard, &model.send_clipboard, "Send clipboard");
        if let Some(request) = model::send_clipboard_request(&model.send_clipboard) {
            let panel = Rc::downgrade(self);
            let peer = peer_name(&model.send_clipboard);
            send_clipboard.connect_clicked(move |_| {
                let (panel, peer) = (panel.clone(), peer.clone());
                // The local clipboard is read by the *daemon*, and only
                // because a person pressed this. Nothing about the clip
                // enters this process, is shown here, or is kept anywhere.
                client::send(request.clone(), move |reply| {
                    let Some(panel) = panel.upgrade() else { return };
                    match reply {
                        Ok(Response::Error { message }) => {
                            panel.toast(&model::clipboard_send_error_message(&message));
                        }
                        // QP-DEBT-06: not "Clipboard sent". The daemon answers
                        // this as soon as the frame is on the session, and the
                        // device's verdict arrives afterwards — in the
                        // clipboard status row, which the panel's own poll
                        // refreshes within a couple of seconds. Claiming
                        // delivery here was optimistic by exactly one round
                        // trip, and on hardware the two came apart.
                        Ok(_) => panel.toast(&model::clipboard_submitted_message(&peer)),
                        Err(_) => panel.toast("The OmniBridge service is not available"),
                    }
                });
            });
        }
        row.append(&send_clipboard);
        row
    }

    /// The native file chooser, then the existing `files.v1` offer.
    ///
    /// The GUI never opens the file. It hands the daemon a path, and the
    /// daemon streams the bytes over the authenticated session it already
    /// holds — this process is not on the data path at any point, and the
    /// receiving end still has to approve the offer.
    fn choose_and_send(self: &Rc<Self>, button: &gtk::Button, action: &Action) {
        let window = button.root().and_downcast::<gtk::Window>();
        let dialog = gtk::FileDialog::builder().title("Send a file").build();
        let panel = Rc::downgrade(self);
        let action = action.clone();
        dialog.open(
            window.as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                let Some(panel) = panel.upgrade() else { return };
                let Ok(file) = result else { return };
                let Some(path) = file.path() else {
                    panel.toast("That file is not on this computer's filesystem");
                    return;
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // Re-derived from the action rather than remembered from the
                // draw: the destination is whatever the model said when the
                // button was pressed, and the daemon re-checks it again.
                let Some(request) =
                    model::send_file_request(&action, path.to_string_lossy().into_owned())
                else {
                    return;
                };
                let peer = peer_name(&action);
                let panel = Rc::downgrade(&panel);
                client::send(request, move |reply| {
                    let Some(panel) = panel.upgrade() else { return };
                    match reply {
                        Ok(Response::Error { message }) => {
                            panel.toast(&format!("Could not send: {message}"));
                        }
                        // The daemon streams transfer events on that connection
                        // and the next poll picks the transfer up, so the only
                        // thing worth saying here is that it started.
                        Ok(_) => panel.toast(&format!("Offering {name} to {peer}")),
                        Err(_) => panel.toast("The OmniBridge service is not available"),
                    }
                });
            },
        );
    }
}

/// The destination's display name, for a sentence. Never for routing.
fn peer_name(action: &Action) -> String {
    match action {
        Action::Ready { peer_name, .. } => peer_name.clone(),
        Action::Blocked { .. } => String::new(),
    }
}

/// Wires an action's availability onto a button, including its reason.
///
/// A disabled control that cannot say why is a dead end. The reason is set as
/// both the tooltip and the accessible description, so it is reachable by
/// pointer and by screen reader; the accessible *name* stays the button's own
/// label so the control is still announced as what it is.
fn bind(button: &gtk::Button, action: &Action, label: &str) {
    button.set_sensitive(action.is_ready());
    match action {
        Action::Ready { peer_name, .. } => {
            let text = format!("{label} to {peer_name}");
            button.set_tooltip_text(Some(&text));
            button.update_property(&[gtk::accessible::Property::Label(&text)]);
        }
        Action::Blocked { reason } => {
            button.set_tooltip_text(Some(reason));
            button.update_property(&[
                gtk::accessible::Property::Label(label),
                gtk::accessible::Property::Description(reason),
            ]);
        }
    }
}

/// One device row. `radio` is present only when there is a choice to make.
fn peer_card(peer: &model::PeerCard, radio: Option<&gtk::CheckButton>) -> gtk::Box {
    let outer = widgets::row(SPACING_SM);
    outer.set_margin_top(SPACING_XS);
    outer.set_margin_bottom(SPACING_XS);
    outer.set_margin_start(SPACING_SM);
    outer.set_margin_end(SPACING_SM);

    if let Some(radio) = radio {
        outer.append(radio);
    }

    let tile = widgets::icon_tile(
        if peer.is_mobile() {
            "phone-symbolic"
        } else {
            "computer-symbolic"
        },
        // Offline devices are muted rather than drawn as though they were
        // live. The word next to them is what actually carries the state.
        if peer.link.is_live() {
            "ob-tile-blue"
        } else {
            "ob-tile-neutral"
        },
    );
    tile.set_valign(gtk::Align::Center);
    outer.append(&tile);

    let text = widgets::column(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let name = widgets::subtitle(&peer.name);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_wrap(false);
    text.append(&name);

    // "Connected · 78%" — the state in words, then the battery, and the
    // battery only when there is something true to say about it.
    let mut state_line = peer.link.label().to_string();
    if !matches!(peer.battery, Battery::Unavailable) {
        state_line.push_str(" · ");
        state_line.push_str(&peer.battery.label());
    }
    let state = widgets::caption(&state_line);
    if peer.link == Link::Connected {
        state.add_css_class(Status::Connected.text_class());
    }
    text.append(&state);

    let caps = peer.available_capabilities();
    let caps_line = widgets::caption(&if caps.is_empty() {
        if peer.link.is_live() {
            "No capabilities available".to_string()
        } else {
            "Available when connected".to_string()
        }
    } else {
        caps.join(" · ")
    });
    // The full picture, including what is *not* available, without spending
    // three lines of a compact panel on it.
    caps_line.set_tooltip_text(Some(&capability_detail(peer)));
    text.append(&caps_line);
    outer.append(&text);

    // Selected is a word as well as the radio's own state — so the meaning
    // survives a theme that draws the radio faintly, and the row's accessible
    // label says "selected device" on top of both.
    if peer.selected {
        let check = widgets::row(widgets::SPACING_XS);
        check.set_valign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name("object-select-symbolic");
        icon.set_pixel_size(16);
        check.append(&icon);
        let label = widgets::caption("Selected");
        check.append(&label);
        check.set_accessible_role(gtk::AccessibleRole::Group);
        check.update_property(&[gtk::accessible::Property::Label("Selected device")]);
        outer.append(&check);
    }
    outer
}

/// Every capability and its state, for the row's tooltip.
///
/// Spelled out rather than implied, and the distinction the model draws —
/// granted versus live on this session — is kept, because "you never allowed
/// this" and "this session did not negotiate it" have different fixes.
fn capability_detail(peer: &model::PeerCard) -> String {
    let describe = |name: &str, c: model::Capability| {
        let state = if !c.granted {
            "not enabled"
        } else if c.live {
            "available"
        } else {
            "enabled, not available on this session"
        };
        format!("{name}: {state}")
    };
    [
        describe("Clipboard", peer.clipboard),
        describe("Files", peer.files),
        describe("Notifications", peer.notifications),
        format!("Fingerprint: {}", peer.fingerprint_short),
    ]
    .join("\n")
}

/// The recent-transfer section: a heading, up to three static rows, and the
/// way into the full list.
///
/// # Static on purpose
///
/// A row is a label, not a control. There is no useful thing for clicking one
/// to do — the panel opens no file manager and this polish adds no
/// file-manager action — and a row that looks activatable and does nothing is
/// worse than a row that does not. The one control here is a real
/// `GtkButton`, which is the only widget in this application's experience
/// that an assistive technology can reliably operate.
///
/// # No decisions
///
/// Everything drawn is read off [`model::RecentTransfer`]. This function does
/// not know what a terminal state is, which of them is newer, or how many to
/// show; if it did, the panel would have grown a rule of its own.
fn recent_section(recent: &[model::RecentTransfer]) -> gtk::Box {
    let section = widgets::column(SPACING_XS);
    section.append(&widgets::separator());
    section.append(&widgets::section_label("Recent transfers"));

    for item in recent {
        section.append(&recent_row(item));
    }

    // Named for what it opens, and an ordinary button: keyboard-focusable,
    // Space- and Enter-operable, and carrying a real AT-SPI action.
    let all = widgets::secondary_button("View all transfers", Some("folder-symbolic"));
    all.set_action_name(Some("app.transfers"));
    all.set_tooltip_text(Some(
        "Opens the Transfers page in OmniBridge Settings. It does not change which device you send to.",
    ));
    all.update_property(&[gtk::accessible::Property::Description(
        "Opens the Transfers page in OmniBridge Settings. It does not change which device you send to.",
    )]);
    section.append(&all);
    section
}

/// One finished transfer.
///
/// The direction appears three times over and never only as an arrow: in the
/// glyph, in the word "To"/"From" on the row, and in the accessible name as a
/// sentence. The arrow is the decoration, not the information.
fn recent_row(item: &model::RecentTransfer) -> gtk::Box {
    let row = widgets::row(SPACING_SM);

    let arrow = gtk::Image::from_icon_name(if item.outgoing {
        "go-up-symbolic"
    } else {
        "go-down-symbolic"
    });
    arrow.set_pixel_size(16);
    arrow.set_valign(gtk::Align::Start);
    arrow.set_margin_top(2);
    // Decorative: the row's own accessible name already says the direction in
    // words, and having the image repeat it makes a screen reader say it
    // twice.
    arrow.set_can_focus(false);
    arrow.update_property(&[gtk::accessible::Property::Label(item.direction_word())]);
    row.append(&arrow);

    let text = widgets::column(0);
    // A filename, and nothing else about the file. No size, no hash, no
    // stored path, no transfer id.
    let name = widgets::caption(&item.filename);
    name.add_css_class("ob-text-primary");
    name.set_halign(gtk::Align::Start);
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_xalign(0.0);
    text.append(&name);

    let line = widgets::caption(&item.line());
    line.set_halign(gtk::Align::Start);
    line.set_xalign(0.0);
    // The existing semantic vocabulary, and a word either way — never colour
    // carrying the outcome on its own.
    line.add_css_class(if item.outcome.succeeded() {
        Status::Connected.text_class()
    } else {
        Status::Disconnected.text_class()
    });
    text.append(&line);
    text.set_hexpand(true);
    row.append(&text);

    let detail = item.detail();
    row.set_tooltip_text(Some(&detail));
    row.set_accessible_role(gtk::AccessibleRole::Group);
    row.update_property(&[
        gtk::accessible::Property::Label(&item.accessible_label()),
        gtk::accessible::Property::Description(&detail),
    ]);
    row
}

/// One status row: a name, a word, and the sentence behind the word.
fn status_row(label: &str, icon_name: &str, line: &model::StatusLine) -> gtk::Box {
    let row = widgets::row(SPACING_SM);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    icon.set_valign(gtk::Align::Center);
    row.append(&icon);

    let name = widgets::caption(label);
    name.add_css_class("ob-text-primary");
    name.set_hexpand(true);
    row.append(&name);

    let value = widgets::caption(line.value.label());
    // Semantic, not brand: "on" is not a brand statement, and an inactive
    // state is neutral grey rather than a colour with a meaning of its own.
    if line.value == StatusValue::On {
        value.add_css_class(Status::Connected.text_class());
    }
    row.append(&value);

    row.set_tooltip_text(Some(&line.detail));
    row.set_accessible_role(gtk::AccessibleRole::Group);
    row.update_property(&[
        gtk::accessible::Property::Label(&format!("{label}: {}", line.value.label())),
        gtk::accessible::Property::Description(&line.detail),
    ]);
    row
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use omnibridge_control::{
        BatteryReport, ClipboardPeerReport, ClipboardStatusReport, ConnectionReport, DeviceReport,
        DeviceState, StatusReport, TransferReport,
    };

    /// A connected Android peer with everything granted and negotiated.
    fn device(name: &str, fingerprint: &str) -> DeviceReport {
        DeviceReport {
            device_id: format!("id-{fingerprint}"),
            device_name: name.into(),
            platform: "android".into(),
            fingerprint: fingerprint.into(),
            fingerprint_short: format!("SHORT {fingerprint}"),
            paired_at_unix: 0,
            granted_capabilities: vec![
                model::FILES.into(),
                model::CLIPBOARD.into(),
                model::BATTERY.into(),
            ],
            revoked: false,
            paired: true,
            connected: true,
            state: DeviceState::Connected,
            silent_secs: Some(1),
            last_seen_secs_ago: None,
            battery: Some(BatteryReport {
                percentage: 78,
                charging_state: "Discharging".into(),
                age_secs: 2,
                stale: false,
            }),
        }
    }

    fn state(devices: Vec<DeviceReport>) -> DaemonState {
        let caps: Vec<String> = vec![
            model::FILES.into(),
            model::CLIPBOARD.into(),
            model::BATTERY.into(),
        ];
        DaemonState {
            clipboard: Some(ClipboardStatusReport {
                enabled: true,
                backend: "wl-clipboard".into(),
                backend_detail: String::new(),
                backend_available: true,
                watch_available: false,
                sensitive_available: true,
                sensitive_detail: String::new(),
                event_cache_entries: 0,
                suppression_cache_entries: 0,
                peers: devices
                    .iter()
                    .map(|d| ClipboardPeerReport {
                        device_id: d.device_id.clone(),
                        device_name: d.device_name.clone(),
                        fingerprint_short: d.fingerprint_short.clone(),
                        granted: true,
                        revoked: false,
                        connected: d.connected,
                        allow_send: true,
                        allow_receive: true,
                        auto_send: false,
                        auto_receive: true,
                        last_outcome: None,
                    })
                    .collect(),
                pending: Vec::new(),
            }),
            status: Some(StatusReport {
                device_name: "Fedora".into(),
                device_id: "self".into(),
                fingerprint: "ff".repeat(32),
                fingerprint_short: "SELF".into(),
                key_backing: "software".into(),
                listen_port: 55432,
                listen_families: "IPv4+IPv6".into(),
                protocol_version_min: 1,
                protocol_version_max: 1,
                capabilities: caps.clone(),
                paired_devices: devices.len(),
                connections: devices
                    .iter()
                    .filter(|d| d.connected)
                    .map(|d| ConnectionReport {
                        device_id: d.device_id.clone(),
                        device_name: d.device_name.clone(),
                        fingerprint_short: d.fingerprint_short.clone(),
                        negotiated_capabilities: caps.clone(),
                        battery: d.battery.clone(),
                        session_id: 1,
                        state: d.state,
                        silent_secs: 1,
                    })
                    .collect(),
                devices: devices.clone(),
                pairing_active: false,
            }),
            devices: Some(devices),
            transfers: Some(Vec::new()),
            notifications: None,
            error: None,
        }
    }

    /// Builds a panel, draws it from `state`, and hands back the content box.
    ///
    /// A fresh application and a fresh selection file each time. Sharing
    /// either would make one section's chosen device the next section's
    /// starting state, which is exactly the kind of order dependence that
    /// makes a failing test lie about which change broke it.
    fn panel(
        state: &DaemonState,
        chosen: Option<&str>,
    ) -> (Rc<QuickPanel>, gtk::Box, Rc<Selection>) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NTH: AtomicU32 = AtomicU32::new(0);
        let nth = NTH.fetch_add(1, Ordering::Relaxed);

        // A distinct id per instance: even a NON_UNIQUE GApplication exports
        // `org.gtk.Application` at an object path derived from its id, and a
        // second one with the same id in the same process collides there.
        let app = crate::application_gate::test_application(&format!("PanelTest{nth}"));

        let panel = QuickPanel::new(&app);
        let selection = Rc::new(Selection::at(std::env::temp_dir().join(format!(
            "omnibridge-panel-test-{}-{nth}/gui.json",
            std::process::id()
        ))));
        if let Some(chosen) = chosen {
            selection.choose(chosen);
        }
        panel.render(state, &selection);
        let content = panel.content.clone();
        (panel, content, selection)
    }

    fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut child = root.first_child();
        while let Some(w) = child {
            out.push(w.clone());
            out.extend(descendants(&w));
            child = w.next_sibling();
        }
        out
    }

    fn labels(root: &gtk::Box) -> Vec<String> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect()
    }

    fn buttons(root: &gtk::Box) -> Vec<gtk::Button> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .collect()
    }

    /// The text of every button, taken from the labels inside it.
    fn button_text(button: &gtk::Button) -> String {
        descendants(button.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn button(root: &gtk::Box, text: &str) -> gtk::Button {
        buttons(root)
            .into_iter()
            .find(|b| button_text(b).contains(text))
            .unwrap_or_else(|| panic!("no button reading {text:?} in {:?}", labels(root)))
    }

    fn lists(root: &gtk::Box) -> Vec<gtk::ListBox> {
        descendants(root.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
            .collect()
    }

    /// Every widget-tree assertion for the panel, in one call.
    ///
    /// One rather than several because GTK binds itself to the thread that
    /// initialised it and libtest gives every `#[test]` a thread of its own.
    /// `views::display_gate` is the single entry point; each section below
    /// still fails with its own name.
    pub(crate) fn the_quick_panel_widget_tree() {
        a_connected_peer_row_states_everything_in_words();
        a_disabled_action_says_why_it_is_disabled();
        the_selected_row_is_the_chosen_peer_and_says_so();
        choosing_a_row_stores_that_rows_fingerprint();
        one_peer_is_not_forced_through_a_selector();
        an_unreachable_daemon_draws_no_live_actions();
        the_panel_always_offers_the_way_into_settings();
        no_recent_section_is_drawn_when_nothing_has_finished();
        a_recent_row_states_its_direction_and_outcome_in_words();
        at_most_three_recent_rows_are_drawn_newest_first();
        view_all_transfers_is_a_button_that_names_the_settings_action();
        a_recent_row_is_a_label_and_not_a_control();
    }

    /// A finished transfer of the daemon's own vocabulary.
    fn finished(seq: u64, filename: &str, peer: &str, direction: &str) -> TransferReport {
        TransferReport {
            transfer_id: format!("{seq:032x}"),
            seq,
            device_name: peer.into(),
            fingerprint_short: format!("SHORT {peer}"),
            direction: direction.into(),
            filename: filename.into(),
            mime_type: "application/octet-stream".into(),
            size_bytes: 1024,
            bytes_transferred: 1024,
            percentage: Some(100),
            state: omnibridge_control::transfer_state::COMPLETED.into(),
            failure: None,
            failure_code: None,
            stored_at: None,
        }
    }

    fn with_transfers(mut st: DaemonState, transfers: Vec<TransferReport>) -> DaemonState {
        st.transfers = Some(transfers);
        st
    }

    /// An empty section is worse than none: it is a heading promising
    /// something, and a panel this small cannot spend a line on nothing.
    fn no_recent_section_is_drawn_when_nothing_has_finished() {
        let (_p, content, _s) = panel(&state(vec![device("SM-X620", "aa11")]), Some("aa11"));
        let text = labels(&content).join(" | ");
        assert!(
            !text.contains("Recent transfers"),
            "a section was drawn with nothing in it: {text}"
        );
        assert!(
            buttons(&content)
                .iter()
                .all(|b| button_text(b) != "View all transfers"),
            "the way into the full list was drawn with nothing to view"
        );
    }

    /// The direction is a word on the row, not only an arrow. An arrow's
    /// rotation is invisible to a screen reader and ambiguous to everyone
    /// else.
    fn a_recent_row_states_its_direction_and_outcome_in_words() {
        let base = state(vec![device("SM-X620", "aa11")]);
        let st = with_transfers(
            base,
            vec![
                finished(
                    2,
                    "document.pdf",
                    "SM-X620",
                    omnibridge_control::transfer_direction::SENDING,
                ),
                finished(
                    1,
                    "photo.jpg",
                    "SM-X620",
                    omnibridge_control::transfer_direction::RECEIVING,
                ),
            ],
        );
        let (_p, content, _s) = panel(&st, Some("aa11"));
        let text = labels(&content).join(" | ");

        assert!(text.contains("Recent transfers"), "{text}");
        assert!(text.contains("document.pdf"), "{text}");
        assert!(text.contains("To SM-X620 · Sent"), "{text}");
        assert!(text.contains("photo.jpg"), "{text}");
        assert!(text.contains("From SM-X620 · Received"), "{text}");
        // No size, no path, no hash, no id anywhere on the drawn panel.
        assert!(!text.contains("1024"), "{text}");
        assert!(!text.contains("1.0 KB"), "{text}");
        assert!(!text.to_lowercase().contains("sha"), "{text}");
    }

    /// Three rows at most, newest first — and "newest" is the daemon's
    /// counter, handed over here in an order that is neither.
    fn at_most_three_recent_rows_are_drawn_newest_first() {
        let base = state(vec![device("SM-X620", "aa11")]);
        let st = with_transfers(
            base,
            (1..=6)
                .map(|n| {
                    finished(
                        // 3, 1, 5, 2, 6, 4 — no accidental ordering to lean on.
                        [3, 1, 5, 2, 6, 4][(n - 1) as usize],
                        &format!("file-{}.txt", [3, 1, 5, 2, 6, 4][(n - 1) as usize]),
                        "SM-X620",
                        omnibridge_control::transfer_direction::SENDING,
                    )
                })
                .collect(),
        );
        let (_p, content, _s) = panel(&st, Some("aa11"));
        let drawn: Vec<String> = labels(&content)
            .into_iter()
            .filter(|l| l.starts_with("file-"))
            .collect();
        assert_eq!(
            drawn,
            vec!["file-6.txt", "file-5.txt", "file-4.txt"],
            "drawn rows were {drawn:?}"
        );
        assert_eq!(drawn.len(), model::RECENT_LIMIT);
    }

    /// The one control in the section is a real button carrying the
    /// application action — not a click handler of its own, and not a
    /// second process.
    fn view_all_transfers_is_a_button_that_names_the_settings_action() {
        let base = state(vec![device("SM-X620", "aa11")]);
        let st = with_transfers(
            base,
            vec![finished(
                1,
                "document.pdf",
                "SM-X620",
                omnibridge_control::transfer_direction::SENDING,
            )],
        );
        let (_p, content, _s) = panel(&st, Some("aa11"));

        let all = button(&content, "View all transfers");
        assert_eq!(
            all.action_name().map(|s| s.to_string()).as_deref(),
            Some("app.transfers"),
            "the button does not go through the application's action"
        );
        // Operable from the keyboard, which is what a `GtkButton` gives and
        // an activatable row measurably does not.
        assert!(all.can_focus());
        // Sensitivity is deliberately *not* asserted here. GTK disables a
        // button whose action does not exist on its application, and the
        // throwaway application these sections build has no actions — which
        // is itself the proof that the button is driven by the action group
        // and not by a handler of its own. That the action exists and opens
        // the right page is asserted in `application_gate`, on a real `App`.
    }

    /// A row is a label. Nothing in the section is activatable except the
    /// one button, because there is nothing useful for a row's click to do
    /// and a control that looks live and is not is worse than a label.
    fn a_recent_row_is_a_label_and_not_a_control() {
        let base = state(vec![device("SM-X620", "aa11")]);
        let st = with_transfers(
            base,
            vec![finished(
                1,
                "document.pdf",
                "SM-X620",
                omnibridge_control::transfer_direction::SENDING,
            )],
        );
        let (_p, content, _s) = panel(&st, Some("aa11"));

        let texts: Vec<String> = buttons(&content).iter().map(button_text).collect();
        assert!(
            !texts.iter().any(|t| t.contains("document.pdf")),
            "a recent row was drawn as a button: {texts:?}"
        );
        // And the section adds no list box: the only one in the panel is the
        // device chooser, whose rows *are* a choice.
        assert_eq!(
            lists(&content).len(),
            1,
            "the recent section grew a selectable list"
        );
    }

    fn a_connected_peer_row_states_everything_in_words() {
        let (_panel, content, _selection) = panel(&state(vec![device("SM-X620", "aa11")]), None);
        let text = labels(&content).join(" | ");
        assert!(text.contains("SM-X620"), "{text}");
        // The state and the battery are words on the row, not a colour.
        assert!(text.contains("Connected · 78%"), "{text}");
        assert!(text.contains("Clipboard · Files"), "{text}");
    }

    /// GTK 4 exposes no way to read an accessible property back, so what a
    /// widget test can check is the tooltip — the same sentence, reaching a
    /// pointer instead of a screen reader. That the *accessible* description
    /// carries it too is one line in `bind`, and the sentence itself is
    /// tested in `model::tests`. The real accessibility tree is captured from
    /// the running application and recorded in the sprint report.
    fn a_disabled_action_says_why_it_is_disabled() {
        let mut offline = device("SM-X620", "aa11");
        offline.connected = false;
        offline.state = DeviceState::Disconnected;
        offline.battery = None;
        let (_panel, content, _selection) = panel(&state(vec![offline]), None);

        let send = button(&content, "Send file");
        assert!(!send.is_sensitive(), "an offline peer cannot be sent to");
        assert_eq!(
            send.tooltip_text().map(|t| t.to_string()).as_deref(),
            Some("SM-X620 is offline."),
            "a disabled control that cannot say why is a dead end"
        );
        assert!(!button(&content, "Send clipboard").is_sensitive());
    }

    fn rows(content: &gtk::Box) -> Vec<gtk::ListBoxRow> {
        let list = lists(content).into_iter().next().expect("a device list");
        descendants(list.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::ListBoxRow>().ok())
            .collect()
    }

    fn radios(root: &gtk::Widget) -> Vec<gtk::CheckButton> {
        descendants(root)
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::CheckButton>().ok())
            .collect()
    }

    fn row_labels(row: &gtk::ListBoxRow) -> Vec<String> {
        descendants(row.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.label().to_string())
            .collect()
    }

    /// The chosen peer is the row whose radio is on, and the row says so in a
    /// word as well — so the meaning does not rest on a radio a theme may draw
    /// faintly.
    fn the_selected_row_is_the_chosen_peer_and_says_so() {
        let devices = vec![device("Tablet", "aa11"), device("Phone", "bb22")];
        let (_panel, content, _selection) = panel(&state(devices), Some("bb22"));
        let rows = rows(&content);
        assert_eq!(rows.len(), 2);
        // Clicking a row is a choice; being *selected* is not a state the
        // widget is allowed to invent. See the note on `peer_list`.
        assert!(rows.iter().all(|r| r.is_activatable()));
        assert!(rows.iter().all(|r| !r.is_selectable()));

        let on: Vec<String> = rows
            .iter()
            .filter(|r| radios(r.upcast_ref()).iter().any(|c| c.is_active()))
            .flat_map(row_labels)
            .collect();
        assert!(on.contains(&"Phone".to_string()), "radio state was {on:?}");
        assert!(on.contains(&"Selected".to_string()), "{on:?}");
        // Exactly one row is marked, in both forms.
        assert_eq!(
            labels(&content).iter().filter(|l| *l == "Selected").count(),
            1
        );
        assert_eq!(
            radios(content.upcast_ref())
                .iter()
                .filter(|c| c.is_active())
                .count(),
            1
        );
    }

    /// The choice that a press actually records.
    ///
    /// Driven through the radio, which is what both a click and a keypress
    /// reach, and asserted on the *stored* value — the fingerprint, not the
    /// row, not the name.
    fn choosing_a_row_stores_that_rows_fingerprint() {
        let devices = vec![device("Tablet", "aa11"), device("Phone", "bb22")];
        let state = state(devices);
        let (_panel, content, selection) = panel(&state, None);
        assert_eq!(selection.current(), None, "nothing is chosen to begin with");

        // Rows are ordered by name among connected peers: Phone, then Tablet.
        let rows = rows(&content);
        let phone = rows
            .iter()
            .find(|r| row_labels(r).contains(&"Phone".to_string()))
            .expect("a Phone row");
        radios(phone.upcast_ref())[0].set_active(true);

        assert_eq!(
            selection.current().as_deref(),
            Some("bb22"),
            "activating the Phone row must store the Phone's fingerprint"
        );

        // And the other way round, which is what "switching the selected peer
        // immediately changes the action target" looks like from the widget.
        let tablet = rows
            .iter()
            .find(|r| row_labels(r).contains(&"Tablet".to_string()))
            .expect("a Tablet row");
        radios(tablet.upcast_ref())[0].set_active(true);
        assert_eq!(selection.current().as_deref(), Some("aa11"));

        // The panel drawn from that choice sends to it and says so.
        let model = model::PanelModel::build(&state, selection.current().as_deref());
        assert_eq!(model.send_file.target(), Some("aa11"));
    }

    fn one_peer_is_not_forced_through_a_selector() {
        let (_panel, content, _selection) = panel(&state(vec![device("SM-X620", "aa11")]), None);
        let rows = rows(&content);
        assert_eq!(rows.len(), 1);
        assert!(
            !rows[0].is_activatable(),
            "with one trusted peer there is nothing to choose between, so the \
             row is not offered as a choice"
        );
        assert!(
            radios(rows[0].upcast_ref()).is_empty(),
            "a selector for a choice that does not exist"
        );
        // It still sends, and it still says where.
        assert!(button(&content, "Send file").is_sensitive());
    }

    fn an_unreachable_daemon_draws_no_live_actions() {
        let state = DaemonState {
            error: Some("could not reach the OmniBridge daemon at /run/…: ENOENT".into()),
            ..DaemonState::default()
        };
        let (_panel, content, _selection) = panel(&state, None);
        let text = labels(&content).join(" | ");
        assert!(
            text.contains("OmniBridge service is not available"),
            "{text}"
        );
        // The raw error stays out of the panel.
        assert!(!text.contains("ENOENT"), "{text}");
        // No device rows, and no action that could be pressed.
        assert!(lists(&content).is_empty(), "{text}");
        assert!(
            buttons(&content)
                .iter()
                .all(|b| button_text(b).contains("Settings")),
            "the only control left is the way into Settings"
        );
    }

    fn the_panel_always_offers_the_way_into_settings() {
        for state in [
            state(vec![device("SM-X620", "aa11")]),
            DaemonState::default(),
        ] {
            let (_panel, content, _selection) = panel(&state, None);
            let settings = button(&content, "Open OmniBridge Settings");
            // Through the application action, not through a reference to a
            // window: this is the same seam a tray would use.
            assert_eq!(
                settings.action_name().map(|n| n.to_string()).as_deref(),
                Some("app.settings")
            );
            // Still reachable by name, and still carrying one. An icon
            // change must not cost the control its accessible label.
            assert!(
                button_text(&settings).contains("Settings"),
                "the settings control lost its name"
            );
        }
    }

    /// Settings wears one metaphor wherever it appears.
    ///
    /// The Quick Panel header, the Quick Panel's own button and the sidebar
    /// row are three unrelated call sites for one action. They now share a
    /// constant, and this is what says why: `preferences-system-symbolic`
    /// draws crossed tools, which left the desktop using a different picture
    /// from Android's gear for the same thing.
    #[test]
    fn the_settings_action_uses_one_icon_everywhere() {
        assert_eq!(
            crate::widgets::SETTINGS_ICON,
            "applications-system-symbolic"
        );
        assert_ne!(
            crate::widgets::SETTINGS_ICON,
            "preferences-system-symbolic",
            "that name is Adwaita's crossed tools, not a settings gear"
        );
        assert_eq!(crate::Page::Settings.icon(), crate::widgets::SETTINGS_ICON);
    }
}
