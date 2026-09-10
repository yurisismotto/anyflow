//! Pairing: show a code, check a fingerprint, decide.
//!
//! The QR is rendered here rather than shown as the daemon's ASCII art. That
//! is not only cosmetic: the terminal rendering inverts on a dark background
//! and ZXing will not decode an inverted code, so the ASCII form is
//! unscannable on a dark terminal. A drawn code is always the right way up.
//!
//! What the code contains is unchanged — the same payload string the daemon
//! emits — and the fingerprint check is unchanged too. Nothing here shortens
//! or hides the fingerprint, because comparing it on both screens is the
//! entire reason pairing has no man-in-the-middle window.

use adw::prelude::*;
use anyflow_control::Event;
use std::cell::RefCell;
use std::rc::Rc;

use super::Pages;
use crate::client;
use crate::widgets::{self, SPACING_MD, SPACING_SM, SPACING_XS};

pub fn present_pairing_dialog(parent: Option<&gtk::Window>, pages: &Pages) {
    let dialog = adw::Dialog::builder()
        .title("Pair a new device")
        .content_width(760)
        .content_height(560)
        .build();

    let root = widgets::column(SPACING_MD);
    root.set_margin_top(SPACING_MD);
    root.set_margin_bottom(SPACING_MD);
    root.set_margin_start(SPACING_MD);
    root.set_margin_end(SPACING_MD);

    root.append(&widgets::heading("Pair a new device"));
    root.append(&widgets::body_muted(
        "Scan this code with AnyFlow on your other device.",
    ));

    let columns = widgets::row(SPACING_MD);

    // --- the code ---------------------------------------------------------
    let qr_card = widgets::card();
    qr_card.set_hexpand(true);
    let qr_area = gtk::DrawingArea::builder()
        .content_width(300)
        .content_height(300)
        .hexpand(true)
        .vexpand(true)
        .build();
    qr_area.set_accessible_role(gtk::AccessibleRole::Img);
    qr_area.update_property(&[gtk::accessible::Property::Label(
        "Pairing QR code. Scan it with AnyFlow on the other device.",
    )]);
    qr_card.append(&qr_area);
    let expiry = widgets::caption("Opening a pairing window…");
    qr_card.append(&expiry);
    columns.append(&qr_card);

    // --- the identity, once a device answers ------------------------------
    let identity_card = widgets::card();
    identity_card.set_size_request(300, -1);
    let identity_body = widgets::column(SPACING_XS);
    identity_card.append(&widgets::section_label("Trusted device identity"));
    identity_card.append(&identity_body);
    identity_body.append(&widgets::body_muted(
        "Waiting for a device to scan the code…",
    ));
    columns.append(&identity_card);
    root.append(&columns);

    // --- what the person is actually deciding ------------------------------
    let notes = widgets::card();
    notes.append(&widgets::section_label("Before you confirm"));
    for (icon, text) in [
        (
            "security-high-symbolic",
            "Direct local connection. The code and the session never leave your network.",
        ),
        (
            "network-transmit-receive-symbolic",
            "Pinned identity. The scanning device pins this computer's key before it \
             opens a socket, so there is no window in which it could be impersonated.",
        ),
        (
            "camera-photo-symbolic",
            "Check the fingerprint matches on both screens before confirming. That \
             comparison is what makes the pairing safe.",
        ),
    ] {
        let r = widgets::row(SPACING_XS);
        let i = gtk::Image::from_icon_name(icon);
        i.set_pixel_size(16);
        i.set_valign(gtk::Align::Start);
        i.add_css_class("af-status-connected");
        r.append(&i);
        r.append(&widgets::caption(text));
        notes.append(&r);
    }
    root.append(&notes);

    let actions = widgets::row(SPACING_SM);
    let cancel = widgets::secondary_button("Cancel", None);
    let confirm = widgets::cta_button("Confirm & pair", Some("object-select-symbolic"));
    // Nothing to confirm until a device has proved it holds the code.
    confirm.set_sensitive(false);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    actions.append(&spacer);
    actions.append(&cancel);
    actions.append(&confirm);
    root.append(&actions);

    dialog.set_child(Some(&root));

    // --- the stream --------------------------------------------------------
    let handle: Rc<RefCell<Option<client::PairHandle>>> = Rc::new(RefCell::new(None));
    let payload: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    {
        let payload = payload.clone();
        qr_area.set_draw_func(move |_, cr, width, height| {
            if let Some(text) = payload.borrow().as_deref() {
                draw_qr(cr, width, height, text);
            }
        });
    }

    let stream = {
        let qr_area = qr_area.clone();
        let expiry = expiry.clone();
        let identity_body = identity_body.clone();
        let confirm = confirm.clone();
        let payload = payload.clone();
        let dialog = dialog.clone();
        let pages = pages.clone();
        client::pair(None, move |event| match event {
            Ok(Event::PairingReady {
                payload: text,
                expires_in_secs,
                ..
            }) => {
                *payload.borrow_mut() = Some(text);
                qr_area.queue_draw();
                expiry.set_label(&format!("This code expires in {expires_in_secs} seconds."));
                true
            }
            Ok(Event::ConfirmRequest {
                device_name,
                device_id,
                fingerprint,
                ..
            }) => {
                widgets::clear(&identity_body);
                identity_body.append(&widgets::subtitle(&device_name));
                identity_body.append(&widgets::caption(&format!("Device id {device_id}")));
                identity_body.append(&widgets::section_label("Device fingerprint"));
                identity_body.append(&widgets::fingerprint(&fingerprint));
                identity_body.append(&widgets::security_notice(
                    "Check this against the other screen",
                    "Confirm only if the fingerprint shown on the other device is \
                     exactly the same.",
                    true,
                ));
                confirm.set_sensitive(true);
                true
            }
            Ok(Event::Finished { status, detail }) => {
                widgets::clear(&identity_body);
                identity_body.append(&widgets::body(&detail));
                confirm.set_sensitive(false);
                expiry.set_label(&status);
                pages.refresh_now();
                if status == "paired" {
                    dialog.close();
                }
                false
            }
            Ok(_) => true,
            Err(e) => {
                widgets::clear(&identity_body);
                identity_body.append(&widgets::security_notice(
                    "Pairing could not start",
                    &e.to_string(),
                    true,
                ));
                false
            }
        })
    };
    *handle.borrow_mut() = Some(stream);

    {
        let handle = handle.clone();
        confirm.connect_clicked(move |b| {
            b.set_sensitive(false);
            if let Some(h) = handle.borrow().as_ref() {
                h.confirm(true);
            }
        });
    }
    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| {
            dialog.close();
        });
    }
    {
        // Dropping the handle closes the stream, which ends the daemon's
        // pairing window rather than leaving it open until it times out.
        let handle = handle.clone();
        dialog.connect_closed(move |_| {
            handle.borrow_mut().take();
        });
    }

    dialog.present(parent);
}

/// Draws the pairing payload as a QR code, with the mark in the middle.
///
/// Error correction is set to the highest level precisely so the centre can
/// carry the mark without making the code harder to read.
fn draw_qr(cr: &gtk::cairo::Context, width: i32, height: i32, payload: &str) {
    use qrcode::{EcLevel, QrCode};

    let Ok(code) = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::H) else {
        return;
    };
    let colors = code.to_colors();
    let modules = code.width();
    let quiet = 2;
    let total = modules + quiet * 2;
    let size = width.min(height) as f64;
    let scale = size / total as f64;
    let ox = (width as f64 - size) / 2.0;
    let oy = (height as f64 - size) / 2.0;

    // Always light-on-dark in the conventional direction, whatever the app
    // theme is. A themed QR is an unscannable QR.
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(ox, oy, size, size);
    let _ = cr.fill();

    cr.set_source_rgb(0.059, 0.090, 0.165); // Ink
    for (i, color) in colors.iter().enumerate() {
        if *color == qrcode::Color::Dark {
            let x = (i % modules + quiet) as f64;
            let y = (i / modules + quiet) as f64;
            cr.rectangle(ox + x * scale, oy + y * scale, scale, scale);
        }
    }
    let _ = cr.fill();

    // The mark, on a white keep-out square the code can spare at level H.
    let logo = size * 0.20;
    let lx = ox + (size - logo) / 2.0;
    let ly = oy + (size - logo) / 2.0;
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(
        lx - scale,
        ly - scale,
        logo + scale * 2.0,
        logo + scale * 2.0,
    );
    let _ = cr.fill();
    // The mark, drawn with the same geometry as the SVG rather than blitted
    // from one: a path scales crisply at any QR size and needs no decode
    // inside a draw callback.
    draw_ribbon(cr, lx, ly, logo);
}

/// The Flowing Ribbon, in Cairo. Geometry mirrors
/// `docs/design/assets/icon-flowing-ribbon.svg` in its 64-unit box.
fn draw_ribbon(cr: &gtk::cairo::Context, x: f64, y: f64, size: f64) {
    let s = size / 64.0;
    cr.save().ok();
    cr.translate(x, y);
    cr.scale(s, s);

    let gradient = gtk::cairo::LinearGradient::new(12.0, 46.0, 52.0, 20.0);
    gradient.add_color_stop_rgb(0.0, 0.086, 0.722, 0.651); // #16B8A6
    gradient.add_color_stop_rgb(0.5, 0.310, 0.486, 1.000); // #4F7CFF
    gradient.add_color_stop_rgb(1.0, 0.545, 0.361, 0.965); // #8B5CF6
    let _ = cr.set_source(&gradient);

    cr.set_line_width(6.5);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.move_to(14.0, 44.0);
    cr.curve_to(30.0, 44.0, 24.0, 21.0, 50.0, 22.0);
    let _ = cr.stroke();

    cr.set_source_rgb(0.086, 0.722, 0.651);
    cr.arc(14.0, 44.0, 7.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    cr.set_source_rgb(0.545, 0.361, 0.965);
    cr.arc(50.0, 22.0, 7.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();

    cr.restore().ok();
}
