//! OmniBridge design tokens for the desktop.
//!
//! The canonical values live in `docs/design/tokens.json`; `tests/tokens.rs`
//! reads that file and fails if this module drifts from it. The Android side
//! is held to the same file, so the two front ends cannot quietly disagree
//! about what "connected cyan" is.
//!
//! # Two colour families, and the split is load-bearing
//!
//! The brand hues are chosen for identity, not legibility: Bridge Cyan on
//! white is 2.40:1, well under the 4.5:1 WCAG AA needs for text. So:
//!
//! * [`brand`] — fills, marks, gradients, the indicator dot itself;
//! * [`on_light`] / [`on_dark`] — the same hues corrected until they clear AA,
//!   for every text label and small icon.
//!
//! Reaching for a `brand` colour where text is involved is the one mistake
//! this module exists to prevent.

/// The identity palette. Decorative surfaces only.
pub mod brand {
    /// Bridge Cyan — the flow origin, connected, toggles.
    pub const CYAN: &str = "#18B8C9";
    /// Primary Blue — primary actions, links, progress, selection.
    pub const BLUE: &str = "#4F6BFF";
    /// Accent Violet — secondary accent, the far end of the sweep.
    pub const VIOLET: &str = "#7C5CFC";
    /// Dark — primary text in light, and the card surface in dark.
    pub const DARK: &str = "#0B1020";
    /// Surface — the application background in light.
    pub const SURFACE: &str = "#F7F9FC";
}

/// Accents corrected for text on light surfaces. All >= 4.5:1 on white and Surface.
pub mod on_light {
    pub const CYAN: &str = "#10747E";
    pub const BLUE: &str = "#445CDD";
    pub const VIOLET: &str = "#6A49EE";
    pub const AMBER: &str = "#B45309";
    pub const RED: &str = "#DC2626";
}

/// Accents lifted for text on dark surfaces. All >= 6.2:1 on Dark and elevated.
pub mod on_dark {
    pub const CYAN: &str = "#3DC9D7";
    pub const BLUE: &str = "#90A1FF";
    pub const VIOLET: &str = "#A28CFA";
    pub const AMBER: &str = "#FBBF24";
    pub const RED: &str = "#F87171";
}

/// The gradient that may carry a label.
///
/// It is exactly the [`on_light`] accent triple rather than a fourth colour
/// set: the same three brand hues, already corrected, so white clears 4.5:1
/// at every interpolated point along the sweep — 5.48:1 at the worst point —
/// and there is nothing extra to keep in step the next time they are retuned.
pub const CTA_GRADIENT: [&str; 3] = [on_light::CYAN, on_light::BLUE, on_light::VIOLET];

/// The identity gradient. Marks, ribbons, progress fills — never text.
pub const BRAND_GRADIENT: [&str; 3] = [brand::CYAN, brand::BLUE, brand::VIOLET];

/// The transfer-progress fill. No text sits on it, so the vivid stops are fine.
pub const PROGRESS_GRADIENT: [&str; 2] = [brand::CYAN, brand::BLUE];

/// Spacing scale, 4px base.
pub mod space {
    pub const XXS: i32 = 4;
    pub const XS: i32 = 8;
    pub const SM: i32 = 12;
    pub const MD: i32 = 16;
    pub const LG: i32 = 20;
    pub const XL: i32 = 24;
    pub const XXL: i32 = 32;
}

/// Corner radii.
pub mod radius {
    pub const SMALL: i32 = 8;
    pub const MEDIUM: i32 = 12;
    pub const LARGE: i32 = 16;
}

/// The light-theme token block, as GTK CSS `@define-color` declarations.
pub fn light_tokens() -> String {
    tokens(
        brand::SURFACE,
        "#FFFFFF",
        "#FFFFFF",
        "#F1F4F9",
        "#E2E7F0",
        "#CBD3E1",
        brand::DARK,
        "#475269",
        "#64718B",
        "#94A0B8",
        on_light::CYAN,
        on_light::BLUE,
        on_light::VIOLET,
        on_light::AMBER,
        on_light::RED,
    )
}

/// The dark-theme token block.
///
/// Derived from Dark rather than inverted: the background sits just *below*
/// Dark and Dark itself becomes the card surface, so a card reads as lifted
/// out of the page exactly as it does in light.
pub fn dark_tokens() -> String {
    tokens(
        "#080C18",
        brand::DARK,
        "#141B30",
        "#05070F",
        "#1E263B",
        "#333E55",
        "#F1F4F9",
        "#94A0B8",
        "#7C88A3",
        "#475269",
        on_dark::CYAN,
        on_dark::BLUE,
        on_dark::VIOLET,
        on_dark::AMBER,
        on_dark::RED,
    )
}

#[allow(clippy::too_many_arguments)]
fn tokens(
    background: &str,
    surface: &str,
    surface_elevated: &str,
    surface_sunken: &str,
    border: &str,
    border_strong: &str,
    text_primary: &str,
    text_secondary: &str,
    text_muted: &str,
    disabled: &str,
    cyan: &str,
    blue: &str,
    violet: &str,
    amber: &str,
    red: &str,
) -> String {
    format!(
        "@define-color ob_background {background};
@define-color ob_surface {surface};
@define-color ob_surface_elevated {surface_elevated};
@define-color ob_surface_sunken {surface_sunken};
@define-color ob_border {border};
@define-color ob_border_strong {border_strong};
@define-color ob_text_primary {text_primary};
@define-color ob_text_secondary {text_secondary};
@define-color ob_text_muted {text_muted};
@define-color ob_disabled {disabled};
@define-color ob_cyan {cyan};
@define-color ob_blue {blue};
@define-color ob_violet {violet};
@define-color ob_amber {amber};
@define-color ob_red {red};
@define-color ob_dot_cyan {};
@define-color ob_dot_blue {};
@define-color ob_dot_violet {};
",
        brand::CYAN,
        brand::BLUE,
        brand::VIOLET,
    )
}

/// One `#RRGGBB` token as Cairo's 0..1 float triple.
///
/// Cairo wants floats and the tokens are hex, and the gap between the two is
/// where a hand-converted `0.086, 0.722, 0.651` sits quietly out of date
/// after the palette moves. Converting at the call site instead means the
/// drawn mark and the stylesheet cannot disagree.
///
/// # Panics
///
/// If `hex` is not `#RRGGBB`. Every caller passes a constant from this
/// module, so a failure here is a typo in the token table, not bad input.
pub fn rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    assert_eq!(h.len(), 6, "{hex} is not a #RRGGBB colour token");
    let channel = |i: usize| {
        f64::from(u8::from_str_radix(&h[i..i + 2], 16).expect("a colour token is hex")) / 255.0
    };
    (channel(0), channel(2), channel(4))
}

/// The structural stylesheet, shared by both themes.
pub const STRUCTURE: &str = include_str!("../data/style.css");

/// Everything, for the given colour scheme.
pub fn stylesheet(dark: bool) -> String {
    let tokens = if dark { dark_tokens() } else { light_tokens() };
    format!("{tokens}\n{STRUCTURE}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// The cross-platform contract. Android reads the very same file.
    fn tokens_json() -> Value {
        serde_json::from_str(include_str!("../../../docs/design/tokens.json"))
            .expect("docs/design/tokens.json should be valid JSON")
    }

    fn hex(v: &Value, path: &[&str]) -> String {
        let mut node = v;
        for key in path {
            node = node
                .get(key)
                .unwrap_or_else(|| panic!("tokens.json is missing {}", path.join(".")));
        }
        node.as_str()
            .unwrap_or_else(|| panic!("{} should be a string", path.join(".")))
            .to_ascii_uppercase()
    }

    fn num(v: &Value, path: &[&str]) -> f64 {
        let mut node = v;
        for key in path {
            node = node
                .get(key)
                .unwrap_or_else(|| panic!("tokens.json is missing {}", path.join(".")));
        }
        node.as_f64()
            .unwrap_or_else(|| panic!("{} should be a number", path.join(".")))
    }

    #[test]
    fn the_brand_palette_matches_the_canonical_tokens() {
        let t = tokens_json();
        assert_eq!(brand::CYAN, hex(&t, &["brand", "cyan"]));
        assert_eq!(brand::BLUE, hex(&t, &["brand", "blue"]));
        assert_eq!(brand::VIOLET, hex(&t, &["brand", "violet"]));
        assert_eq!(brand::DARK, hex(&t, &["brand", "dark"]));
        assert_eq!(brand::SURFACE, hex(&t, &["brand", "surface"]));
    }

    #[test]
    fn the_corrected_accents_match_the_canonical_tokens() {
        let t = tokens_json();
        assert_eq!(on_light::CYAN, hex(&t, &["on_light", "cyan"]));
        assert_eq!(on_light::BLUE, hex(&t, &["on_light", "blue"]));
        assert_eq!(on_light::VIOLET, hex(&t, &["on_light", "violet"]));
        assert_eq!(on_light::AMBER, hex(&t, &["on_light", "amber"]));
        assert_eq!(on_light::RED, hex(&t, &["on_light", "red"]));
        assert_eq!(on_dark::CYAN, hex(&t, &["on_dark", "cyan"]));
        assert_eq!(on_dark::BLUE, hex(&t, &["on_dark", "blue"]));
        assert_eq!(on_dark::VIOLET, hex(&t, &["on_dark", "violet"]));
        assert_eq!(on_dark::AMBER, hex(&t, &["on_dark", "amber"]));
        assert_eq!(on_dark::RED, hex(&t, &["on_dark", "red"]));
    }

    #[test]
    fn the_gradients_match_the_canonical_tokens() {
        let t = tokens_json();
        let cta: Vec<String> = t["gradient"]["cta"]["stops"]
            .as_array()
            .expect("cta stops")
            .iter()
            .map(|s| {
                s.as_str()
                    .expect("a gradient stop is a hex string")
                    .to_ascii_uppercase()
            })
            .collect();
        assert_eq!(CTA_GRADIENT.to_vec(), cta);

        let brand: Vec<String> = t["gradient"]["brand"]["stops"]
            .as_array()
            .expect("brand stops")
            .iter()
            .map(|s| {
                s.as_str()
                    .expect("a gradient stop is a hex string")
                    .to_ascii_uppercase()
            })
            .collect();
        assert_eq!(BRAND_GRADIENT.to_vec(), brand);
    }

    #[test]
    fn the_scales_match_the_canonical_tokens() {
        let t = tokens_json();
        for (name, value) in [
            ("xxs", space::XXS),
            ("xs", space::XS),
            ("sm", space::SM),
            ("md", space::MD),
            ("lg", space::LG),
            ("xl", space::XL),
            ("xxl", space::XXL),
        ] {
            assert_eq!(value as f64, num(&t, &["spacing", name]), "spacing.{name}");
        }
        for (name, value) in [
            ("small", radius::SMALL),
            ("medium", radius::MEDIUM),
            ("large", radius::LARGE),
        ] {
            assert_eq!(value as f64, num(&t, &["radius", name]), "radius.{name}");
        }
    }

    // ---- the part that makes the accessibility claim executable -----------

    fn luminance(hex: &str) -> f64 {
        let h = hex.trim_start_matches('#');
        let channel = |i: usize| {
            let v = u8::from_str_radix(&h[i..i + 2], 16).expect("hex pair") as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Every accent used for text must clear WCAG AA on its own surface.
    ///
    /// This is the whole reason `on_light` and `on_dark` exist apart from
    /// `brand`. If someone "simplifies" the palette by pointing a label at a
    /// brand hue, this test is what stops it.
    #[test]
    fn every_text_accent_clears_aa_on_its_own_surface() {
        let floor = num(&tokens_json(), &["contrast_floor", "text_on_surface"]);
        for (name, colour) in [
            ("cyan", on_light::CYAN),
            ("blue", on_light::BLUE),
            ("violet", on_light::VIOLET),
            ("amber", on_light::AMBER),
            ("red", on_light::RED),
        ] {
            let white = contrast(colour, "#FFFFFF");
            let surface = contrast(colour, brand::SURFACE);
            assert!(white >= floor, "on_light::{name} is {white:.2}:1 on white");
            assert!(
                surface >= floor,
                "on_light::{name} is {surface:.2}:1 on Surface"
            );
        }
        for (name, colour) in [
            ("cyan", on_dark::CYAN),
            ("blue", on_dark::BLUE),
            ("violet", on_dark::VIOLET),
            ("amber", on_dark::AMBER),
            ("red", on_dark::RED),
        ] {
            let dark = contrast(colour, brand::DARK);
            assert!(dark >= floor, "on_dark::{name} is {dark:.2}:1 on Dark");
            // Elevated is the lighter of the two dark surfaces, so it is the
            // harder one; a card on an elevated sheet must stay AA too.
            let elevated = contrast(colour, "#141B30");
            assert!(
                elevated >= floor,
                "on_dark::{name} is {elevated:.2}:1 on the elevated dark surface"
            );
        }
    }

    /// The brand hues are *not* safe as text, and that must stay documented.
    ///
    /// A guard rather than a curiosity: if a future palette change made
    /// Bridge Cyan legible as text, the two-family split could be collapsed —
    /// and this failing is how anyone would find out.
    #[test]
    fn the_raw_brand_hues_are_known_to_fail_as_text() {
        let floor = num(&tokens_json(), &["contrast_floor", "text_on_surface"]);
        assert!(
            contrast(brand::CYAN, "#FFFFFF") < floor,
            "Bridge Cyan now passes AA on white — revisit the two-family palette split \
             in docs/design/BRAND.md before using it for text"
        );
    }

    /// White must stay legible across the whole CTA sweep, not just its stops.
    ///
    /// Sampling the interpolation is the point: a gradient can pass at both
    /// ends and fail in the middle, and the label sits on all of it.
    #[test]
    fn white_stays_legible_across_the_whole_cta_gradient() {
        let floor = num(&tokens_json(), &["contrast_floor", "white_on_cta_gradient"]);
        let parse = |h: &str| {
            let h = h.trim_start_matches('#');
            [0usize, 2, 4].map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex") as f64)
        };
        let mut worst = f64::MAX;
        let mut worst_at = String::new();
        for pair in CTA_GRADIENT.windows(2) {
            let (a, b) = (parse(pair[0]), parse(pair[1]));
            for step in 0..=20 {
                let t = f64::from(step) / 20.0;
                let mixed = format!(
                    "#{:02X}{:02X}{:02X}",
                    (a[0] + (b[0] - a[0]) * t).round() as u8,
                    (a[1] + (b[1] - a[1]) * t).round() as u8,
                    (a[2] + (b[2] - a[2]) * t).round() as u8,
                );
                let ratio = contrast("#FFFFFF", &mixed);
                if ratio < worst {
                    worst = ratio;
                    worst_at = mixed;
                }
            }
        }
        assert!(
            worst >= floor,
            "white on the CTA gradient falls to {worst:.2}:1 at {worst_at}"
        );
    }

    /// Every literal hex in the stylesheet must be a token this module owns.
    ///
    /// GTK CSS cannot interpolate between two `@colors` in a gradient, so the
    /// CTA sweep and the progress fill are spelled out as literals in
    /// `style.css`. That is a standing drift risk and this is what closes it:
    /// retune the palette without touching the stylesheet and the literals
    /// left behind stop matching any token, and this fails.
    #[test]
    fn every_literal_hex_in_the_stylesheet_is_a_known_token() {
        let known: Vec<String> = [
            brand::CYAN,
            brand::BLUE,
            brand::VIOLET,
            brand::DARK,
            brand::SURFACE,
            on_light::CYAN,
            on_light::BLUE,
            on_light::VIOLET,
            on_light::AMBER,
            on_light::RED,
            on_dark::CYAN,
            on_dark::BLUE,
            on_dark::VIOLET,
            on_dark::AMBER,
            on_dark::RED,
            // The status dot hues, which are semantic rather than brand.
            "#F59E0B",
            "#EF4444",
            "#FFFFFF",
        ]
        .iter()
        .map(|h| h.to_ascii_uppercase())
        .collect();

        let mut found = 0;
        for line in STRUCTURE.lines() {
            let mut rest = line;
            while let Some(at) = rest.find('#') {
                let candidate: String = rest[at..]
                    .chars()
                    .take(7)
                    .collect::<String>()
                    .to_ascii_uppercase();
                if candidate.len() == 7 && candidate[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    found += 1;
                    assert!(
                        known.contains(&candidate),
                        "style.css contains {candidate}, which is not a token this \
                         module defines — retune it or add it to the token table"
                    );
                }
                rest = &rest[at + 1..];
            }
        }
        assert!(
            found > 0,
            "expected the stylesheet to contain literal hexes"
        );
    }

    /// The CTA gradient in the stylesheet must be the CTA gradient token.
    ///
    /// The button's sweep is the one literal that carries a *label*, so a
    /// stale value there is an accessibility regression rather than a
    /// cosmetic one.
    #[test]
    fn the_stylesheet_cta_gradient_matches_the_token() {
        let cta = STRUCTURE
            .lines()
            .find(|l| l.contains("linear-gradient") && l.contains("to right"))
            .expect("style.css should define the CTA gradient");
        for stop in CTA_GRADIENT {
            assert!(
                cta.to_ascii_uppercase()
                    .contains(&stop.to_ascii_uppercase()),
                "the CTA gradient in style.css is missing {stop}: {cta}"
            );
        }
    }

    /// The dark surfaces stay ordered, or a card stops reading as lifted.
    #[test]
    fn the_dark_surfaces_stay_ordered() {
        let tokens = dark_tokens();
        let value = |name: &str| {
            tokens
                .lines()
                .find(|l| l.starts_with(&format!("@define-color {name} ")))
                .and_then(|l| l.split_whitespace().nth(2))
                .map(|v| v.trim_end_matches(';').to_string())
                .unwrap_or_else(|| panic!("dark theme has no {name}"))
        };
        let lum = |name: &str| luminance(&value(name));
        assert!(
            lum("ob_surface_sunken") < lum("ob_background")
                && lum("ob_background") < lum("ob_surface")
                && lum("ob_surface") < lum("ob_surface_elevated"),
            "dark surfaces are out of order: sunken {:.4}, background {:.4}, \
             surface {:.4}, elevated {:.4}",
            lum("ob_surface_sunken"),
            lum("ob_background"),
            lum("ob_surface"),
            lum("ob_surface_elevated"),
        );
    }

    /// The neutral ramp's two ends are the brand, not merely near it.
    #[test]
    fn the_light_theme_is_anchored_by_the_brand() {
        let tokens = light_tokens();
        assert!(
            tokens.contains(&format!("@define-color ob_background {};", brand::SURFACE)),
            "the light background must be Surface"
        );
        assert!(
            tokens.contains(&format!("@define-color ob_text_primary {};", brand::DARK)),
            "light primary text must be Dark"
        );
    }

    /// The stylesheet must resolve every token it names.
    ///
    /// GTK does not fail loudly on an undefined `@colour` — it falls back and
    /// carries on, so a typo would silently paint the wrong thing.
    #[test]
    fn the_stylesheet_only_uses_colours_the_theme_defines() {
        for dark in [false, true] {
            let sheet = stylesheet(dark);
            let defined: Vec<&str> = sheet
                .lines()
                .filter_map(|l| l.strip_prefix("@define-color "))
                .filter_map(|l| l.split_whitespace().next())
                .collect();
            for line in STRUCTURE.lines() {
                for token in line.split('@').skip(1) {
                    let name: String = token
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if name.starts_with("ob_") {
                        assert!(
                            defined.contains(&name.as_str()),
                            "style.css uses @{name}, which the {} theme never defines",
                            if dark { "dark" } else { "light" }
                        );
                    }
                }
            }
        }
    }
}
