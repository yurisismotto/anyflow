//! OmniBridge design tokens for the desktop.
//!
//! The canonical values live in `docs/design/tokens.json`; `tests/tokens.rs`
//! reads that file and fails if this module drifts from it. The Android side
//! is held to the same file, so the two front ends cannot quietly disagree
//! about what "connected teal" is.
//!
//! # Two colour families, and the split is load-bearing
//!
//! The brand hues are chosen for identity, not legibility: brand teal on
//! white is 2.49:1, well under the 4.5:1 WCAG AA needs for text. So:
//!
//! * [`brand`] — fills, marks, gradients, the indicator dot itself;
//! * [`on_light`] / [`on_dark`] — the same hues corrected until they clear AA,
//!   for every text label and small icon.
//!
//! Reaching for a `brand` colour where text is involved is the one mistake
//! this module exists to prevent.

/// The identity palette. Decorative surfaces only.
pub mod brand {
    pub const TEAL: &str = "#16B8A6";
    pub const BLUE: &str = "#4F7CFF";
    pub const VIOLET: &str = "#8B5CF6";
    pub const INK: &str = "#0F172A";
    pub const PAPER: &str = "#F8FAFC";
}

/// Accents corrected for text on light surfaces. All >= 4.5:1 on white.
pub mod on_light {
    pub const TEAL: &str = "#0F766E";
    pub const BLUE: &str = "#3B5BDB";
    pub const VIOLET: &str = "#7C3AED";
    pub const AMBER: &str = "#B45309";
    pub const RED: &str = "#DC2626";
}

/// Accents lifted for text on dark surfaces. All >= 6.4:1 on Ink.
pub mod on_dark {
    pub const TEAL: &str = "#2DD4BF";
    pub const BLUE: &str = "#8FA9FF";
    pub const VIOLET: &str = "#A78BFA";
    pub const AMBER: &str = "#FBBF24";
    pub const RED: &str = "#F87171";
}

/// The gradient that may carry a label.
///
/// Its teal start is deepened until white clears 4.5:1 at every interpolated
/// point along the sweep — not merely at the stops.
pub const CTA_GRADIENT: [&str; 3] = ["#0B7F72", "#3B5BDB", "#7C3AED"];

/// The identity gradient. Marks, ribbons, progress fills — never text.
pub const BRAND_GRADIENT: [&str; 3] = [brand::TEAL, brand::BLUE, brand::VIOLET];

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
        "#F8FAFC",
        "#FFFFFF",
        "#FFFFFF",
        "#F1F5F9",
        "#E2E8F0",
        "#CBD5E1",
        "#0F172A",
        "#475569",
        "#64748B",
        "#94A3B8",
        on_light::TEAL,
        on_light::BLUE,
        on_light::VIOLET,
        on_light::AMBER,
        on_light::RED,
    )
}

/// The dark-theme token block.
///
/// Derived from Ink rather than inverted: the background sits just *below*
/// Ink and Ink itself becomes the card surface, so a card reads as lifted out
/// of the page exactly as it does in light.
pub fn dark_tokens() -> String {
    tokens(
        "#0A0F1C",
        "#0F172A",
        "#1B2436",
        "#070B14",
        "#1E293B",
        "#334155",
        "#F1F5F9",
        "#94A3B8",
        "#7C8CA3",
        "#475569",
        on_dark::TEAL,
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
    teal: &str,
    blue: &str,
    violet: &str,
    amber: &str,
    red: &str,
) -> String {
    format!(
        "@define-color af_background {background};
@define-color af_surface {surface};
@define-color af_surface_elevated {surface_elevated};
@define-color af_surface_sunken {surface_sunken};
@define-color af_border {border};
@define-color af_border_strong {border_strong};
@define-color af_text_primary {text_primary};
@define-color af_text_secondary {text_secondary};
@define-color af_text_muted {text_muted};
@define-color af_disabled {disabled};
@define-color af_teal {teal};
@define-color af_blue {blue};
@define-color af_violet {violet};
@define-color af_amber {amber};
@define-color af_red {red};
@define-color af_dot_teal {};
@define-color af_dot_blue {};
@define-color af_dot_violet {};
",
        brand::TEAL,
        brand::BLUE,
        brand::VIOLET,
    )
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
        assert_eq!(brand::TEAL, hex(&t, &["brand", "teal"]));
        assert_eq!(brand::BLUE, hex(&t, &["brand", "blue"]));
        assert_eq!(brand::VIOLET, hex(&t, &["brand", "violet"]));
        assert_eq!(brand::INK, hex(&t, &["brand", "ink"]));
        assert_eq!(brand::PAPER, hex(&t, &["brand", "paper"]));
    }

    #[test]
    fn the_corrected_accents_match_the_canonical_tokens() {
        let t = tokens_json();
        assert_eq!(on_light::TEAL, hex(&t, &["on_light", "teal"]));
        assert_eq!(on_light::BLUE, hex(&t, &["on_light", "blue"]));
        assert_eq!(on_light::VIOLET, hex(&t, &["on_light", "violet"]));
        assert_eq!(on_light::AMBER, hex(&t, &["on_light", "amber"]));
        assert_eq!(on_light::RED, hex(&t, &["on_light", "red"]));
        assert_eq!(on_dark::TEAL, hex(&t, &["on_dark", "teal"]));
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
            ("teal", on_light::TEAL),
            ("blue", on_light::BLUE),
            ("violet", on_light::VIOLET),
            ("amber", on_light::AMBER),
            ("red", on_light::RED),
        ] {
            let white = contrast(colour, "#FFFFFF");
            let paper = contrast(colour, brand::PAPER);
            assert!(white >= floor, "on_light::{name} is {white:.2}:1 on white");
            assert!(paper >= floor, "on_light::{name} is {paper:.2}:1 on Paper");
        }
        for (name, colour) in [
            ("teal", on_dark::TEAL),
            ("blue", on_dark::BLUE),
            ("violet", on_dark::VIOLET),
            ("amber", on_dark::AMBER),
            ("red", on_dark::RED),
        ] {
            let ink = contrast(colour, brand::INK);
            assert!(ink >= floor, "on_dark::{name} is {ink:.2}:1 on Ink");
        }
    }

    /// The brand hues are *not* safe as text, and that must stay documented.
    ///
    /// A guard rather than a curiosity: if a future palette change made brand
    /// teal legible as text, the two-family split could be collapsed — and
    /// this failing is how anyone would find out.
    #[test]
    fn the_raw_brand_hues_are_known_to_fail_as_text() {
        let floor = num(&tokens_json(), &["contrast_floor", "text_on_surface"]);
        assert!(
            contrast(brand::TEAL, "#FFFFFF") < floor,
            "brand teal now passes AA on white — revisit the two-family palette split \
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
                    if name.starts_with("af_") {
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
