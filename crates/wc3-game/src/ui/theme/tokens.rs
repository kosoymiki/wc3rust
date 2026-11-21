//! The theme: palette + type scale + metrics + state table.
//!
//! `Theme::classic_fantasy()` is the whole design system as data. Nothing in
//! the render path may invent a colour, a size or a gap: it asks the theme.

use crate::ui::theme::metrics::Metrics;
use crate::ui::theme::palette::{ContrastRow, Palette, Rgba};
use crate::ui::theme::states::{self, StateStyle, Surface};
use crate::ui::theme::typography::{TypeRole, TypeScale, TypeStyle};
use crate::ui::core::semantics::WidgetState;

/// A complete design system.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub palette: Palette,
    pub type_scale: TypeScale,
    pub metrics: Metrics,
}

impl Theme {
    pub const fn classic_fantasy() -> Self {
        Theme {
            palette: Palette::classic_fantasy(),
            type_scale: TypeScale::direction(),
            metrics: Metrics::direction(),
        }
    }

    pub fn style(&self, role: TypeRole) -> TypeStyle {
        self.type_scale.style(role)
    }

    pub fn px(&self, role: TypeRole, density: f32) -> f32 {
        self.type_scale.px(role, density)
    }

    pub fn scale_30(&self, role: TypeRole, density: f32) -> f32 {
        self.type_scale.scale_30(role, density)
    }

    pub fn widget(&self, surface: Surface, state: WidgetState) -> StateStyle {
        states::style(&self.palette, surface, state)
    }

    /// Health bar colour for a ratio.
    pub fn hp(&self, ratio: f32) -> Rgba {
        self.palette.hp(ratio)
    }

    pub fn contrast_audit(&self) -> Vec<ContrastRow> {
        self.palette.contrast_audit()
    }

    /// The report table: element → token → px at `density`.
    pub fn type_table(&self, density: f32) -> Vec<(&'static str, TypeRole, f32, f32)> {
        // (element, token, dp, px)
        let rows: [(&'static str, TypeRole); 9] = [
            ("unit name", TypeRole::Heading),
            ("unit class", TypeRole::Label),
            ("unit stats", TypeRole::Label),
            ("production label", TypeRole::Label),
            ("resource numerals", TypeRole::Numeric),
            ("map name", TypeRole::Body),
            ("tooltip title", TypeRole::Heading),
            ("tooltip body", TypeRole::Tooltip),
            ("hotkey badge", TypeRole::Compact),
        ];
        rows.iter()
            .map(|(element, role)| {
                let style = self.type_scale.style(*role);
                (*element, *role, style.size_dp, style.px(density))
            })
            .collect()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::classic_fantasy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::metrics;

    #[test]
    fn the_theme_is_the_single_source_of_the_console_numbers() {
        let theme = Theme::classic_fantasy();
        assert_eq!(theme.metrics.touch, metrics::TOUCH_DP);
        assert_eq!(theme.metrics.card_w, metrics::CARD_W_DP);
        assert_eq!(theme.metrics.card_slots(), 12);
        assert_eq!(theme.palette.accent, theme.palette.gold);
    }

    #[test]
    fn the_panel_rows_use_the_direction_type_scale() {
        let theme = Theme::classic_fantasy();
        let by_element = |name: &str| {
            theme
                .type_table(3.0)
                .into_iter()
                .find(|(element, ..)| *element == name)
                .unwrap()
        };
        assert_eq!(by_element("unit name").3, 60.0);
        assert_eq!(by_element("unit class").3, 39.0);
        assert_eq!(by_element("resource numerals").3, 45.0);
        assert_eq!(by_element("tooltip body").3, 45.0);
    }

    #[test]
    fn contrast_passes_on_the_shipped_palette() {
        let theme = Theme::classic_fantasy();
        assert!(theme.contrast_audit().iter().all(|row| row.passes));
    }

    #[test]
    fn widget_styles_come_from_the_table() {
        let theme = Theme::classic_fantasy();
        let normal = theme.widget(Surface::Slot, WidgetState::Normal);
        let disabled = theme.widget(Surface::Slot, WidgetState::Disabled);
        assert_ne!(normal.fill, disabled.fill);
    }
}
