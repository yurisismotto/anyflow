//! The OmniBridge desktop component set.
//!
//! Every screen is assembled from these, so a card, a status or a button
//! cannot look one way on the dashboard and another on the pairing page. The
//! Android side has the same set under `ui/components`, deliberately named
//! the same things.

use adw::prelude::*;

use crate::theme::space;

/// Spacing shorthands for layout code, so a screen never reaches for a bare
/// number. The full scale — and the test that pins it to
/// `docs/design/tokens.json` — lives in [`crate::theme::space`].
pub const SPACING_XS: i32 = space::XS;
pub const SPACING_SM: i32 = space::SM;
pub const SPACING_MD: i32 = space::MD;

/// Removes every child of a container before it is rebuilt.
///
/// The pages re-render wholesale on each refresh rather than diffing. With a
/// handful of devices and transfers that is cheaper in bugs than a diffing
/// layer, and it makes "what is on screen" a pure function of daemon state.
pub fn clear(container: &impl IsA<gtk::Widget>) {
    while let Some(child) = container.as_ref().first_child() {
        child.unparent();
    }
}

/// The status vocabulary, shared with the Android side.
///
/// # The rule this type enforces
///
/// **Nothing says something with colour alone.** A status is a dot, an icon
/// and a word, and the word is what carries the meaning. There is no variant
/// here that is only a colour, because there is no constructor for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Connected,
    Available,
    Connecting,
    Transferring,
    Success,
    Stale,
    Warning,
    Error,
    Disconnected,
    Revoked,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Connected => "Connected",
            Status::Available => "Available",
            Status::Connecting => "Connecting",
            Status::Transferring => "Transferring",
            Status::Success => "Done",
            Status::Stale => "Not responding",
            Status::Warning => "Needs attention",
            Status::Error => "Failed",
            Status::Disconnected => "Disconnected",
            Status::Revoked => "Revoked",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Status::Connected => "network-transmit-receive-symbolic",
            Status::Available => "computer-symbolic",
            Status::Connecting | Status::Transferring => "document-open-recent-symbolic",
            Status::Success => "object-select-symbolic",
            Status::Stale | Status::Warning | Status::Error => "dialog-warning-symbolic",
            Status::Disconnected => "network-offline-symbolic",
            Status::Revoked => "security-low-symbolic",
        }
    }

    /// The AA-corrected text colour class.
    pub fn text_class(self) -> &'static str {
        match self {
            Status::Connected => "ob-status-connected",
            Status::Available => "ob-status-available",
            Status::Connecting | Status::Transferring => "ob-status-transferring",
            Status::Success => "ob-status-success",
            Status::Stale => "ob-status-stale",
            Status::Warning => "ob-status-warning",
            Status::Error => "ob-status-error",
            Status::Revoked => "ob-status-revoked",
            Status::Disconnected => "ob-status-disconnected",
        }
    }

    /// The full-strength brand hue, for the dot only.
    pub fn dot_class(self) -> &'static str {
        match self {
            Status::Connected | Status::Success => "ob-dot-cyan",
            Status::Available | Status::Connecting | Status::Transferring => "ob-dot-blue",
            Status::Stale | Status::Warning => "ob-dot-amber",
            Status::Error | Status::Revoked => "ob-dot-red",
            Status::Disconnected => "ob-dot-neutral",
        }
    }

    /// Maps the daemon's own device state onto the vocabulary.
    ///
    /// `Stale` is kept distinct from `Connected` rather than folded into it:
    /// a session that has gone quiet is exactly the case where anything the
    /// device last told us is history, and showing it as live is the bug
    /// `DeviceState` was introduced to prevent.
    pub fn from_device_state(state: omnibridge_control::DeviceState) -> Self {
        use omnibridge_control::DeviceState as D;
        match state {
            D::Connected => Status::Connected,
            D::Stale => Status::Stale,
            D::Disconnected => Status::Available,
            D::Revoked => Status::Revoked,
        }
    }
}

/// A vertical card: one surface, one hairline border, one radius.
pub fn card() -> gtk::Box {
    let b = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(space::XS)
        .build();
    b.add_css_class("ob-card");
    b
}

pub fn row(spacing: i32) -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(spacing)
        .build()
}

pub fn column(spacing: i32) -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(spacing)
        .build()
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::Word)
        .build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn title(text: &str) -> gtk::Label {
    label(text, &["ob-title", "ob-text-primary"])
}

pub fn heading(text: &str) -> gtk::Label {
    label(text, &["ob-heading", "ob-text-primary"])
}

pub fn subtitle(text: &str) -> gtk::Label {
    label(text, &["ob-subtitle", "ob-text-primary"])
}

pub fn body(text: &str) -> gtk::Label {
    label(text, &["ob-body", "ob-text-primary"])
}

pub fn body_muted(text: &str) -> gtk::Label {
    label(text, &["ob-body", "ob-text-secondary"])
}

pub fn caption(text: &str) -> gtk::Label {
    label(text, &["ob-caption", "ob-text-secondary"])
}

/// The small grey heading above a group. A real heading for assistive tech.
pub fn section_label(text: &str) -> gtk::Label {
    let l = label(text, &["ob-label", "ob-text-secondary"]);
    l.set_accessible_role(gtk::AccessibleRole::Heading);
    l
}

/// A fingerprint, device id or hash.
///
/// Monospace and *selectable*, never shortened. This is the string a person
/// compares against another screen before trusting a device; abbreviating it
/// for visual balance would trade away the only thing that makes pairing
/// safe.
pub fn fingerprint(text: &str) -> gtk::Label {
    let grouped = group_fingerprint(text);
    let l = gtk::Label::builder()
        .label(&grouped)
        .xalign(0.0)
        .selectable(true)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    l.add_css_class("ob-mono");
    l.add_css_class("ob-text-primary");
    l.update_property(&[gtk::accessible::Property::Label(&format!(
        "Fingerprint {}",
        grouped.split_whitespace().collect::<Vec<_>>().join(", ")
    ))]);
    l
}

/// Breaks a fingerprint into four-character groups.
///
/// Purely for the human doing the comparison, and it is not cosmetic: reading
/// a 64-character run off one screen and checking it against another is where
/// mistakes happen, and grouping is what makes a mismatch visible. A value
/// that already carries spaces — the daemon's short form — is left alone.
pub fn group_fingerprint(text: &str) -> String {
    if text.contains(' ') {
        return text.to_string();
    }
    text.to_uppercase()
        .chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Dot, icon, word — all three, always.
pub fn status_badge(status: Status) -> gtk::Box {
    let b = row(space::XXS);
    b.set_valign(gtk::Align::Center);

    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("ob-dot");
    dot.add_css_class(status.dot_class());
    dot.set_valign(gtk::Align::Center);
    b.append(&dot);

    let icon = gtk::Image::from_icon_name(status.icon());
    icon.set_pixel_size(16);
    icon.add_css_class(status.text_class());
    b.append(&icon);

    let text = label(status.label(), &["ob-label", status.text_class()]);
    text.set_wrap(false);
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    b.append(&text);

    // Announced once, as a phrase, rather than three times as fragments.
    b.set_accessible_role(gtk::AccessibleRole::Group);
    b.update_property(&[gtk::accessible::Property::Label(status.label())]);
    b
}

/// An icon on a soft tint of its own accent.
pub fn icon_tile(icon_name: &str, tone: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("ob-tile");
    b.add_css_class(tone);
    b.set_halign(gtk::Align::Center);
    b.set_valign(gtk::Align::Center);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(20);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    b.append(&icon);
    // Without this the tile grows to whatever the row gives it and the card
    // it sits in stretches with it.
    b.set_halign(gtk::Align::Start);
    b.set_valign(gtk::Align::Start);
    b
}

/// The branded primary action. Wears the CTA gradient, never the identity one.
pub fn cta_button(text: &str, icon_name: Option<&str>) -> gtk::Button {
    let content = row(space::XS);
    content.set_halign(gtk::Align::Center);
    if let Some(name) = icon_name {
        let icon = gtk::Image::from_icon_name(name);
        icon.set_pixel_size(16);
        content.append(&icon);
    }
    content.append(&gtk::Label::new(Some(text)));
    let b = gtk::Button::builder().child(&content).build();
    b.add_css_class("ob-cta");
    b
}

pub fn secondary_button(text: &str, icon_name: Option<&str>) -> gtk::Button {
    let content = row(space::XS);
    content.set_halign(gtk::Align::Center);
    if let Some(name) = icon_name {
        let icon = gtk::Image::from_icon_name(name);
        icon.set_pixel_size(16);
        content.append(&icon);
    }
    content.append(&gtk::Label::new(Some(text)));
    let b = gtk::Button::builder().child(&content).build();
    b.add_css_class("ob-secondary");
    b
}

/// An action that takes something away. Outlined, red, kept apart.
pub fn destructive_button(text: &str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("ob-destructive");
    b
}

/// A quiet security or privacy statement.
///
/// Deliberately calm. Most of these are reassurance, and a UI that shouts
/// every security fact teaches people to stop reading them; `caution` raises
/// the volume only when the message needs acting on.
pub fn security_notice(title_text: &str, body_text: &str, caution: bool) -> gtk::Box {
    let b = row(space::SM);
    b.add_css_class("ob-notice");
    if caution {
        b.add_css_class("ob-notice-caution");
    }
    let icon = gtk::Image::from_icon_name(if caution {
        "dialog-warning-symbolic"
    } else {
        "security-high-symbolic"
    });
    icon.set_pixel_size(20);
    icon.set_valign(gtk::Align::Start);
    icon.add_css_class(if caution {
        "ob-status-warning"
    } else {
        "ob-status-available"
    });
    b.append(&icon);

    let text = column(space::XXS);
    text.append(&label(title_text, &["ob-label", "ob-text-primary"]));
    if !body_text.is_empty() {
        text.append(&caption(body_text));
    }
    b.append(&text);
    b
}

/// Nothing here yet.
///
/// Uses the OmniBridge mark rather than a stock illustration: the mark already
/// means "one bridge, any device", which is what the person is being invited to
/// create.
pub fn empty_state(title_text: &str, subtitle_text: &str) -> gtk::Box {
    let b = column(space::SM);
    b.set_halign(gtk::Align::Center);
    b.set_valign(gtk::Align::Center);
    b.set_vexpand(true);
    b.set_margin_top(space::XXL);
    b.set_margin_bottom(space::XXL);

    // Capped, not merely requested — see `brand_mark`. An empty state is
    // mostly whitespace, so this is exactly the place a Picture would have
    // grown to fill the page.
    //
    // 96 rather than the 200 the old size request asked for: the mark is the
    // illustration here, not the subject. At 200 it out-weighed the sentence
    // underneath it that actually says what is going on, which is the
    // "logo as decoration" failure the brand guidance warns about.
    let art = brand_mark(96);
    b.append(&art);

    let t = heading(title_text);
    t.set_justify(gtk::Justification::Center);
    t.set_halign(gtk::Align::Center);
    b.append(&t);

    let s = body_muted(subtitle_text);
    s.set_justify(gtk::Justification::Center);
    s.set_halign(gtk::Align::Center);
    s.set_max_width_chars(48);
    b.append(&s);
    b
}

/// The OmniBridge mark.
///
/// One mark, everywhere: this is `docs/design/assets/omnibridge-mark.svg`, the
/// same artwork the launcher icon wears on Android and the same one the tray and
/// the application icon wear here. `desktop/gui/tests/brand_assets.rs` asserts
/// that this is the file being drawn.
///
/// There is no second, heavier cut for small sizes any more, and that is a
/// property of the artwork rather than an omission. The mark the AnyFlow era
/// drew was a *stroked* ribbon, so below about 24 px its 8-unit stroke fell
/// under a pixel and a half and greyed out, which is why a thicker variant had
/// to exist. The OmniBridge mark is filled, so it has no stroke to thin: it
/// scales down as area, not as line weight, and one file answers for every size.
/// The icon for "open Settings", everywhere it appears.
///
/// A constant rather than a convention, because this action shows up in three
/// unrelated places — the sidebar row, the Quick Panel header button and the
/// Quick Panel's own "Open OmniBridge Settings" — and a fourth caller
/// reaching for a different name is how one action ends up with two
/// metaphors.
///
/// It is the cog. Adwaita's `preferences-system-symbolic`, which this used to
/// be, draws a hammer crossed with a wrench: it reads as "tools", and it left
/// the desktop showing crossed tools for the concept Android draws as a gear.
pub const SETTINGS_ICON: &str = "applications-system-symbolic";

pub fn brand_mark(size: i32) -> gtk::Image {
    let i = gtk::Image::from_resource("/io/github/yurisismotto/omnibridge/omnibridge-mark.svg");
    // `set_pixel_size` is a *cap*; `set_size_request` — which this used to
    // use, on a `GtkPicture` — is a floor. A Picture given a 22px size
    // request and a header bar with room to spare takes the room: the mark
    // rendered about 350px tall across the top of the window, which is how a
    // 22 in the source became a logo the size of the content beneath it.
    i.set_pixel_size(size);
    i.set_halign(gtk::Align::Center);
    i.set_valign(gtk::Align::Center);
    i.set_hexpand(false);
    i.set_vexpand(false);
    // Decoration: every surface that shows the mark also says the name.
    i.set_accessible_role(gtk::AccessibleRole::Presentation);
    i
}

/// Transfer progress in the brand gradient.
pub fn progress(fraction: Option<f64>) -> gtk::ProgressBar {
    let p = gtk::ProgressBar::new();
    p.add_css_class("ob-progress");
    match fraction {
        Some(f) => p.set_fraction(f.clamp(0.0, 1.0)),
        // A zero-byte file has no meaningful percentage; pulsing says
        // "working" without asserting a number that would be a lie.
        None => p.pulse(),
    }
    p
}

/// A horizontal hairline.
pub fn separator() -> gtk::Separator {
    let s = gtk::Separator::new(gtk::Orientation::Horizontal);
    s.add_css_class("ob-separator");
    s
}

#[cfg(test)]
mod tests {
    use super::group_fingerprint;

    #[test]
    fn a_raw_hex_fingerprint_is_grouped_in_fours() {
        assert_eq!(group_fingerprint("1b2707ba7d289da2"), "1B27 07BA 7D28 9DA2");
    }

    #[test]
    fn an_already_grouped_fingerprint_is_left_alone() {
        // The daemon's short form arrives pre-grouped; regrouping it would
        // shift the boundaries and make the two forms disagree on screen.
        assert_eq!(
            group_fingerprint("1B27 07BA 7D28 9DA2"),
            "1B27 07BA 7D28 9DA2"
        );
    }

    #[test]
    fn a_trailing_partial_group_is_kept() {
        assert_eq!(group_fingerprint("abcdef"), "ABCD EF");
    }
}
