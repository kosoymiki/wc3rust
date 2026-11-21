//! Colour roles (hud.md §5.2).
//!
//! Every widget asks for a *role* — `text_primary`, `danger`, `unavailable` —
//! and the palette is the only place a literal colour lives. The values are
//! sampled from the retail console art (dark iron, warm leather, gold trim,
//! parchment text), so the HUD and the menu cannot drift apart, and the
//! contrast audit can walk the same table the renderer uses.

/// Linear-free sRGB colour with an alpha channel, stored as authored.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const WHITE: Rgba = Rgba::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Rgba = Rgba::rgba(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Rgba = Rgba::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Rgba { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }

    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Rgba { a, ..self }
    }

    /// Composite over `under` with the source alpha.
    pub fn over(self, under: Rgba) -> Rgba {
        let a = self.a.clamp(0.0, 1.0);
        Rgba {
            r: self.r * a + under.r * (1.0 - a),
            g: self.g * a + under.g * (1.0 - a),
            b: self.b * a + under.b * (1.0 - a),
            a: 1.0,
        }
    }

    /// WCAG relative luminance.
    pub fn luminance(self) -> f32 {
        let channel = |v: f32| {
            let v = v.clamp(0.0, 1.0);
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio between two colours (1.0 … 21.0).
    pub fn contrast(self, other: Rgba) -> f32 {
        let a = self.luminance();
        let b = other.luminance();
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

/// The colour roles the interface is allowed to use.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    /// Screen background behind everything.
    pub bg_primary: Rgba,
    /// Secondary surface (console plate, panel well).
    pub bg_secondary: Rgba,
    /// Raised surface (button face, tooltip).
    pub bg_raised: Rgba,
    /// Pressed surface.
    pub bg_pressed: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    /// Text drawn on the accent colour.
    pub text_on_accent: Rgba,
    pub accent: Rgba,
    pub selected: Rgba,
    pub danger: Rgba,
    pub warning: Rgba,
    pub unavailable: Rgba,
    pub positive: Rgba,
    pub border: Rgba,
    pub border_hot: Rgba,
    pub shadow: Rgba,
    pub hp_good: Rgba,
    pub hp_warn: Rgba,
    pub hp_low: Rgba,
    pub gold: Rgba,
    pub lumber: Rgba,
    pub supply: Rgba,
}

/// The role table, in report order.
pub const ROLES: [&str; 22] = [
    "bg_primary",
    "bg_secondary",
    "bg_raised",
    "bg_pressed",
    "text_primary",
    "text_secondary",
    "text_on_accent",
    "accent",
    "selected",
    "danger",
    "warning",
    "unavailable",
    "positive",
    "border",
    "border_hot",
    "shadow",
    "hp_good",
    "hp_warn",
    "hp_low",
    "gold",
    "lumber",
    "supply",
];

impl Palette {
    /// Retail-sampled classic fantasy palette.
    pub const fn classic_fantasy() -> Self {
        Palette {
            bg_primary: Rgba::rgba(0.055, 0.045, 0.035, 0.98),
            bg_secondary: Rgba::rgba(0.078, 0.086, 0.098, 0.98),
            bg_raised: Rgba::rgba(0.10, 0.085, 0.06, 0.98),
            bg_pressed: Rgba::rgba(0.055, 0.045, 0.03, 0.98),
            text_primary: Rgba::rgb(0.93, 0.88, 0.76),
            text_secondary: Rgba::rgba(0.68, 0.62, 0.50, 0.95),
            text_on_accent: Rgba::rgb(0.07, 0.05, 0.03),
            accent: Rgba::rgb(0.95, 0.76, 0.30),
            selected: Rgba::rgba(0.72, 0.58, 0.28, 0.95),
            danger: Rgba::rgb(1.0, 0.35, 0.37),
            warning: Rgba::rgb(0.95, 0.76, 0.30),
            unavailable: Rgba::rgba(0.40, 0.36, 0.28, 0.55),
            positive: Rgba::rgb(0.36, 0.89, 0.54),
            border: Rgba::rgba(0.52, 0.42, 0.22, 0.90),
            border_hot: Rgba::rgba(0.72, 0.58, 0.28, 0.95),
            shadow: Rgba::rgba(0.0, 0.0, 0.0, 0.65),
            hp_good: Rgba::rgb(0.32, 0.86, 0.52),
            hp_warn: Rgba::rgb(0.95, 0.76, 0.30),
            hp_low: Rgba::rgb(1.0, 0.42, 0.40),
            gold: Rgba::rgb(0.95, 0.76, 0.30),
            lumber: Rgba::rgb(0.48, 0.85, 0.56),
            supply: Rgba::rgb(0.44, 0.71, 1.0),
        }
    }

    /// Look up a role by its report name.
    pub fn role(&self, name: &str) -> Option<Rgba> {
        Some(match name {
            "bg_primary" => self.bg_primary,
            "bg_secondary" => self.bg_secondary,
            "bg_raised" => self.bg_raised,
            "bg_pressed" => self.bg_pressed,
            "text_primary" => self.text_primary,
            "text_secondary" => self.text_secondary,
            "text_on_accent" => self.text_on_accent,
            "accent" => self.accent,
            "selected" => self.selected,
            "danger" => self.danger,
            "warning" => self.warning,
            "unavailable" => self.unavailable,
            "positive" => self.positive,
            "border" => self.border,
            "border_hot" => self.border_hot,
            "shadow" => self.shadow,
            "hp_good" => self.hp_good,
            "hp_warn" => self.hp_warn,
            "hp_low" => self.hp_low,
            "gold" => self.gold,
            "lumber" => self.lumber,
            "supply" => self.supply,
            _ => return None,
        })
    }

    /// Health colour for a ratio.
    pub fn hp(&self, ratio: f32) -> Rgba {
        if ratio > 0.55 {
            self.hp_good
        } else if ratio > 0.25 {
            self.hp_warn
        } else {
            self.hp_low
        }
    }

    /// One contrast row per text role on each surface it is used on.
    ///
    /// `minimum` is the WCAG AA body floor (4.5) or the large-text floor (3.0).
    pub fn contrast_audit(&self) -> Vec<ContrastRow> {
        let mut rows = Vec::new();
        let text_pairs: [(&str, Rgba, Rgba, f32); 8] = [
            ("text_primary on bg_primary", self.text_primary, self.bg_primary, 4.5),
            ("text_primary on bg_raised", self.text_primary, self.bg_raised, 4.5),
            ("text_primary on bg_secondary", self.text_primary, self.bg_secondary, 4.5),
            ("text_secondary on bg_primary", self.text_secondary, self.bg_primary, 4.5),
            ("text_on_accent on accent", self.text_on_accent, self.accent, 4.5),
            ("danger on bg_raised", self.danger, self.bg_raised, 4.5),
            ("positive on bg_raised", self.positive, self.bg_raised, 4.5),
            ("gold on bg_secondary", self.gold, self.bg_secondary, 4.5),
        ];
        for (label, fg, bg, minimum) in text_pairs {
            let ratio = fg.over(bg).contrast(bg);
            rows.push(ContrastRow {
                label,
                role_fg: label.split(' ').next().unwrap_or("").to_string(),
                ratio,
                minimum,
                passes: ratio + 0.001 >= minimum,
            });
        }
        rows
    }
}

/// One row of the contrast table.
#[derive(Clone, PartialEq, Debug)]
pub struct ContrastRow {
    pub label: &'static str,
    pub role_fg: String,
    pub ratio: f32,
    pub minimum: f32,
    pub passes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_maths_matches_the_known_extremes() {
        assert!((Rgba::WHITE.contrast(Rgba::BLACK) - 21.0).abs() < 0.01);
        assert!((Rgba::WHITE.contrast(Rgba::WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_shipped_palette_passes_its_own_contrast_audit() {
        let palette = Palette::classic_fantasy();
        for row in palette.contrast_audit() {
            assert!(
                row.passes,
                "{} fails: {:.2} < {:.2}",
                row.label, row.ratio, row.minimum
            );
        }
    }

    #[test]
    fn every_role_name_resolves() {
        let palette = Palette::classic_fantasy();
        for role in ROLES {
            assert!(palette.role(role).is_some(), "{role}");
        }
        assert!(palette.role("nope").is_none());
    }

    #[test]
    fn hp_ratio_picks_the_three_bands() {
        let p = Palette::classic_fantasy();
        assert_eq!(p.hp(0.9), p.hp_good);
        assert_eq!(p.hp(0.4), p.hp_warn);
        assert_eq!(p.hp(0.1), p.hp_low);
    }

    #[test]
    fn alpha_compositing_keeps_the_top_colour() {
        let base = Rgba::rgb(0.0, 0.0, 0.0);
        let half = Rgba::rgba(1.0, 1.0, 1.0, 0.5).over(base);
        assert!((half.r - 0.5).abs() < 0.01);
    }
}
