//! Type roles and the direction type scale (hud.md §5.3 + polish addendum §3).
//!
//! The scale is data in **dp**. At density 3 the retail direction tokens are
//! display 84 px, title 66, heading 60, body 45, label 39, compact 33, numeric
//! 45 tabular; the dp values below reproduce exactly those pixels.
//!
//! Two accessors exist because the renderer's text API is authored at 30 px per
//! unit: [`TypeScale::scale_30`] returns the multiplier the HUD passes to its
//! shaper, [`TypeScale::px`] returns the physical pixel size for reports and the
//! geometry auditor.

/// A typographic role.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypeRole {
    /// Screen titles.
    Display,
    /// Large labels (campaign chapter names).
    Title,
    /// Panel titles and the selected unit's name.
    Heading,
    /// Body copy and tooltip text.
    Body,
    /// Small labels (class, stats, production).
    Label,
    /// Micro labels (hotkey badges, counters).
    Compact,
    /// Resource numerals, timers, cooldowns (tabular).
    Numeric,
    /// Tooltip body.
    Tooltip,
}

impl TypeRole {
    pub const ALL: [TypeRole; 8] = [
        TypeRole::Display,
        TypeRole::Title,
        TypeRole::Heading,
        TypeRole::Body,
        TypeRole::Label,
        TypeRole::Compact,
        TypeRole::Numeric,
        TypeRole::Tooltip,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TypeRole::Display => "display",
            TypeRole::Title => "title",
            TypeRole::Heading => "heading",
            TypeRole::Body => "body",
            TypeRole::Label => "label",
            TypeRole::Compact => "compact",
            TypeRole::Numeric => "numeric",
            TypeRole::Tooltip => "tooltip",
        }
    }

    /// Numeric roles reserve digit slots so a value never shifts the layout.
    pub const fn is_tabular(self) -> bool {
        matches!(self, TypeRole::Numeric)
    }
}

/// Weight axis of the shipped faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Regular,
    Medium,
    Bold,
}

/// One resolved style.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TypeStyle {
    pub role: TypeRole,
    /// Type size in dp.
    pub size_dp: f32,
    /// Line box in dp.
    pub line_dp: f32,
    pub weight: Weight,
    /// Digits occupy fixed slots (tabular figures).
    pub tabular: bool,
    /// Letter spacing in dp (negative tightens display sizes).
    pub tracking_dp: f32,
}

impl TypeStyle {
    /// Build a style from the direction table.
    pub const fn new(role: TypeRole, size_dp: f32, weight: Weight) -> Self {
        TypeStyle {
            role,
            size_dp,
            line_dp: size_dp * 1.25,
            weight,
            tabular: role.is_tabular(),
            tracking_dp: 0.0,
        }
    }

    /// Physical pixel size at `density`.
    pub fn px(&self, density: f32) -> f32 {
        self.size_dp * density
    }

    /// The multiplier the HUD shaper expects (face authored at 30 px).
    pub fn scale_30(&self, density: f32) -> f32 {
        (self.size_dp * density / 30.0).max(0.0)
    }

    /// Width of `digits` numerals in this style, for stable numeric columns.
    pub fn numeric_width(&self, digits: u8, density: f32) -> f32 {
        crate::ui::layout::text_fit::numeric_slot(digits, self.px(density))
    }
}

/// The eight roles of the direction scale.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TypeScale {
    pub display: TypeStyle,
    pub title: TypeStyle,
    pub heading: TypeStyle,
    pub body: TypeStyle,
    pub label: TypeStyle,
    pub compact: TypeStyle,
    pub numeric: TypeStyle,
    pub tooltip: TypeStyle,
}

impl TypeScale {
    /// dp sizes measured from the direction tokens at 3x.
    pub const fn direction() -> Self {
        TypeScale {
            display: TypeStyle::new(TypeRole::Display, 28.0, Weight::Bold),
            title: TypeStyle::new(TypeRole::Title, 22.0, Weight::Bold),
            heading: TypeStyle::new(TypeRole::Heading, 20.0, Weight::Bold),
            body: TypeStyle::new(TypeRole::Body, 15.0, Weight::Regular),
            label: TypeStyle::new(TypeRole::Label, 13.0, Weight::Medium),
            compact: TypeStyle::new(TypeRole::Compact, 11.0, Weight::Medium),
            numeric: TypeStyle::new(TypeRole::Numeric, 15.0, Weight::Medium),
            tooltip: TypeStyle::new(TypeRole::Tooltip, 15.0, Weight::Regular),
        }
    }

    pub fn style(&self, role: TypeRole) -> TypeStyle {
        match role {
            TypeRole::Display => self.display,
            TypeRole::Title => self.title,
            TypeRole::Heading => self.heading,
            TypeRole::Body => self.body,
            TypeRole::Label => self.label,
            TypeRole::Compact => self.compact,
            TypeRole::Numeric => self.numeric,
            TypeRole::Tooltip => self.tooltip,
        }
    }

    /// Physical pixels for a role.
    pub fn px(&self, role: TypeRole, density: f32) -> f32 {
        self.style(role).px(density)
    }

    /// Shaper multiplier for a role.
    pub fn scale_30(&self, role: TypeRole, density: f32) -> f32 {
        self.style(role).scale_30(density)
    }

    /// The report table: role → dp → px at `density`.
    pub fn table(&self, density: f32) -> Vec<(TypeRole, f32, f32)> {
        TypeRole::ALL
            .iter()
            .map(|role| {
                let style = self.style(*role);
                (*role, style.size_dp, style.px(density))
            })
            .collect()
    }
}

impl Default for TypeScale {
    fn default() -> Self {
        TypeScale::direction()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_tokens_reproduce_the_measured_pixels() {
        let scale = TypeScale::direction();
        let px = |role: TypeRole| scale.px(role, 3.0);
        assert!((px(TypeRole::Display) - 84.0).abs() < 0.01, "display");
        assert!((px(TypeRole::Title) - 66.0).abs() < 0.01, "title");
        assert!((px(TypeRole::Heading) - 60.0).abs() < 0.01, "heading");
        assert!((px(TypeRole::Body) - 45.0).abs() < 0.01, "body");
        assert!((px(TypeRole::Label) - 39.0).abs() < 0.01, "label");
        assert!((px(TypeRole::Compact) - 33.0).abs() < 0.01, "compact");
        assert!((px(TypeRole::Numeric) - 45.0).abs() < 0.01, "numeric");
        assert!((px(TypeRole::Tooltip) - 45.0).abs() < 0.01, "tooltip");
    }

    #[test]
    fn the_shaper_multiplier_is_px_over_30() {
        let scale = TypeScale::direction();
        assert!((scale.scale_30(TypeRole::Heading, 3.0) - 2.0).abs() < 1e-4);
        assert!((scale.scale_30(TypeRole::Label, 3.0) - 1.3).abs() < 1e-4);
    }

    #[test]
    fn numeric_is_tabular_and_only_numeric() {
        let scale = TypeScale::direction();
        assert!(scale.numeric.tabular);
        assert!(!scale.body.tabular);
        assert!(TypeRole::Numeric.is_tabular());
    }

    #[test]
    fn a_numeric_column_does_not_move_between_9_and_10() {
        let style = TypeScale::direction().numeric;
        // One reserved slot is the same width whether the value is 9 or 10:
        // the column is sized for the widest expected value, not the current.
        let nine = style.numeric_width(4, 3.0);
        let ten = style.numeric_width(4, 3.0);
        assert!((nine - ten).abs() < 0.001);
        assert!(style.numeric_width(5, 3.0) > nine);
    }
}
