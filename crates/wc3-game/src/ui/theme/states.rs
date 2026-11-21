//! Component state styling (hud.md §5.4).
//!
//! One table maps `(widget role, state)` to the colours a painter needs. A
//! component asks for its state and gets a complete style, so "disabled" cannot
//! look like "normal" in one widget and "unavailable" in another.

use crate::ui::core::semantics::WidgetState;
use crate::ui::theme::palette::{Palette, Rgba};

/// What an interactive role is painted as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// Command-card slot / inventory slot.
    Slot,
    /// Push button.
    Button,
    /// Text row / list row.
    Row,
    /// Modal panel.
    Panel,
}

/// The resolved look of one widget state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StateStyle {
    pub fill: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub icon_tint: Rgba,
    /// Extra tint applied over the icon (cooldown veil, press darkening).
    pub overlay: Option<Rgba>,
}

/// Style for `state` on `surface`.
pub fn style(palette: &Palette, surface: Surface, state: WidgetState) -> StateStyle {
    let (fill, border, text, icon_tint, overlay) = match (surface, state) {
        (_, WidgetState::Disabled) => (
            palette.unavailable,
            palette.border,
            palette.text_secondary,
            Rgba::rgba(0.55, 0.55, 0.55, 0.55),
            None,
        ),
        (_, WidgetState::Error) => (
            palette.danger.with_alpha(0.30),
            palette.danger,
            palette.danger,
            Rgba::WHITE,
            None,
        ),
        (_, WidgetState::Loading) => (
            palette.bg_pressed,
            palette.border,
            palette.text_secondary,
            Rgba::rgba(0.8, 0.8, 0.8, 0.6),
            Some(palette.shadow.with_alpha(0.35)),
        ),
        (_, WidgetState::Pressed) => (
            palette.bg_pressed,
            palette.border_hot,
            palette.text_primary,
            Rgba::rgb(0.92, 0.92, 0.92),
            Some(Rgba::rgba(0.0, 0.0, 0.0, 0.22)),
        ),
        (_, WidgetState::Hover) => (
            palette.bg_raised,
            palette.border_hot,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (_, WidgetState::Focused) => (
            palette.bg_raised,
            palette.accent,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (_, WidgetState::Selected) => (
            palette.selected.with_alpha(0.35),
            palette.accent,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (Surface::Slot, WidgetState::Normal) => (
            palette.bg_secondary,
            palette.border,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (Surface::Button, WidgetState::Normal) => (
            palette.bg_raised,
            palette.border,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (Surface::Row, WidgetState::Normal) => (
            Rgba::TRANSPARENT,
            palette.border,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
        (Surface::Panel, WidgetState::Normal) => (
            palette.bg_primary,
            palette.border,
            palette.text_primary,
            Rgba::WHITE,
            None,
        ),
    };
    StateStyle {
        fill,
        border,
        text,
        icon_tint,
        overlay,
    }
}

/// A cooldown veil: the part of the slot still on cooldown.
pub fn cooldown_veil(remaining: f32, total: f32) -> Option<Rgba> {
    if !remaining.is_finite() || !total.is_finite() || total <= 0.0 || remaining <= 0.0 {
        return None;
    }
    let ratio = (remaining / total).clamp(0.0, 1.0);
    Some(Rgba::rgba(0.0, 0.0, 0.0, 0.55 * ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_is_distinguishable_from_normal() {
        let palette = Palette::classic_fantasy();
        let normal = style(&palette, Surface::Slot, WidgetState::Normal);
        for state in [
            WidgetState::Hover,
            WidgetState::Pressed,
            WidgetState::Focused,
            WidgetState::Selected,
            WidgetState::Disabled,
            WidgetState::Loading,
            WidgetState::Error,
        ] {
            let other = style(&palette, Surface::Slot, state);
            assert_ne!(normal, other, "{state:?} looks like normal");
        }
    }

    #[test]
    fn disabled_is_never_the_same_as_selected() {
        let palette = Palette::classic_fantasy();
        let disabled = style(&palette, Surface::Slot, WidgetState::Disabled);
        let selected = style(&palette, Surface::Slot, WidgetState::Selected);
        assert_ne!(disabled.text, selected.text);
        assert_ne!(disabled.icon_tint, selected.icon_tint);
    }

    #[test]
    fn cooldown_veil_tracks_the_remaining_share() {
        assert!(cooldown_veil(0.0, 2.0).is_none());
        let full = cooldown_veil(2.0, 2.0).unwrap();
        let half = cooldown_veil(1.0, 2.0).unwrap();
        assert!(full.a > half.a);
        assert!((full.a - 0.55).abs() < 0.01);
    }
}
