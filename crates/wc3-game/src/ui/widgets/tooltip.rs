//! Tooltips: content model and placement (hud.md §3.8).
//!
//! A tooltip never leaves the viewport, never covers the control it describes
//! when there is room elsewhere, and never stacks a second copy of itself. The
//! content carries the same fields the command model does, so an unavailable
//! command explains *why* instead of going grey silently.

use crate::ui::core::geometry::{Insets, Point, Rect, Size};
use crate::ui::layout::text_fit;
use crate::ui::theme::typography::TypeScale;

/// The side a tooltip prefers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Side {
    #[default]
    Above,
    Below,
    Left,
    Right,
}

/// Everything a tooltip shows.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Tooltip {
    pub title: String,
    pub body: String,
    /// Cost / requirement lines (gold, lumber, food, research).
    pub costs: Vec<String>,
    /// Hotkey, when the command really has one.
    pub hotkey: Option<char>,
    /// Cooldown line, when a cooldown is running.
    pub cooldown: Option<String>,
    /// Why the command is unavailable.
    pub disabled: Option<String>,
}

impl Tooltip {
    pub fn new(title: impl Into<String>) -> Self {
        Tooltip {
            title: title.into(),
            ..Tooltip::default()
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn cost(mut self, line: impl Into<String>) -> Self {
        self.costs.push(line.into());
        self
    }

    pub fn hotkey(mut self, hotkey: char) -> Self {
        self.hotkey = Some(hotkey);
        self
    }

    pub fn cooldown(mut self, line: impl Into<String>) -> Self {
        self.cooldown = Some(line.into());
        self
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled = Some(reason.into());
        self
    }

    /// Everything that has to be painted, in reading order.
    pub fn lines(&self) -> Vec<&str> {
        let mut lines = Vec::new();
        lines.push(self.title.as_str());
        if !self.body.is_empty() {
            lines.push(self.body.as_str());
        }
        for cost in &self.costs {
            lines.push(cost.as_str());
        }
        if let Some(cooldown) = &self.cooldown {
            lines.push(cooldown.as_str());
        }
        if let Some(disabled) = &self.disabled {
            lines.push(disabled.as_str());
        }
        lines
    }

    /// The hotkey line, if any.
    pub fn hotkey_line(&self) -> Option<String> {
        self.hotkey.map(|key| format!("Горячая клавиша: {key}"))
    }
}

/// The solved box: where each part is painted.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TooltipLayout {
    pub panel: Rect,
    pub title: Rect,
    pub body: Rect,
    pub footer: Rect,
    pub lines: u16,
}

/// Preferred width for a tooltip at `density`.
pub fn preferred_width(scale: &TypeScale, density: f32) -> f32 {
    // ~38 body characters per line is the retail reading measure.
    text_fit::advance("XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", scale.px(crate::ui::theme::TypeRole::Tooltip, density))
}

/// Layout a tooltip of `size` next to `anchor`, inside `viewport`.
///
/// The box prefers `side`, flips to the opposite side when it would leave the
/// viewport, and is finally clamped so it is always fully visible.
pub fn place(
    anchor: Rect,
    size: Size,
    viewport: Rect,
    safe: Insets,
    side: Side,
) -> Rect {
    let bounds = Rect::new(
        viewport.x + safe.left,
        viewport.y + safe.top,
        (viewport.w - safe.horizontal()).max(1.0),
        (viewport.h - safe.vertical()).max(1.0),
    );
    let gap = 6.0;
    let above = Rect::new(
        anchor.x,
        anchor.y - gap - size.h,
        size.w,
        size.h,
    );
    let below = Rect::new(anchor.x, anchor.bottom() + gap, size.w, size.h);
    let left = Rect::new(anchor.x - gap - size.w, anchor.y, size.w, size.h);
    let right = Rect::new(anchor.right() + gap, anchor.y, size.w, size.h);
    let candidates = match side {
        Side::Above => [above, below, left, right],
        Side::Below => [below, above, right, left],
        Side::Left => [left, right, above, below],
        Side::Right => [right, left, above, below],
    };
    for candidate in candidates {
        if candidate.is_inside(bounds) {
            return candidate;
        }
    }
    // Nothing fits beside the anchor: centre over it and clamp.
    Rect::new(
        anchor.centre().x - size.w * 0.5,
        anchor.y - size.h - gap,
        size.w,
        size.h,
    )
    .clamp_inside(bounds)
}

/// Solve the internal boxes of a tooltip.
pub fn layout(
    panel: Rect,
    tooltip: &Tooltip,
    scale: &TypeScale,
    density: f32,
) -> TooltipLayout {
    let pad = 8.0 * density;
    let title_h = scale.px(crate::ui::theme::TypeRole::Heading, density) * 1.25;
    let body_h = scale.px(crate::ui::theme::TypeRole::Tooltip, density) * 1.25;
    let inner = panel.inset(pad, pad);
    let title = Rect::new(inner.x, inner.y, inner.w, title_h.min(inner.h));
    let rest = (inner.h - title.h).max(0.0);
    let footer_lines = tooltip.costs.len()
        + usize::from(tooltip.cooldown.is_some())
        + usize::from(tooltip.disabled.is_some())
        + usize::from(tooltip.hotkey_line().is_some());
    let footer_h = (footer_lines as f32 * body_h).min(rest * 0.5);
    let body = Rect::new(
        inner.x,
        title.bottom(),
        inner.w,
        (rest - footer_h).max(0.0),
    );
    let footer = Rect::new(inner.x, body.bottom(), inner.w, footer_h);
    TooltipLayout {
        panel,
        title,
        body,
        footer,
        lines: tooltip.lines().len() as u16,
    }
}

/// The anchor a hovered control gives a tooltip: the control's own rect.
pub fn anchor_for(control: Rect) -> Rect {
    control
}

/// A pointer position that keeps a tooltip alive while the pointer walks into
/// it (§3.8 "не исчезать при перемещении указателя внутрь самой подсказки").
pub fn keeps_open(tooltip: Rect, pointer: Point, slop: f32) -> bool {
    tooltip.inset(-slop, -slop).contains(pointer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::TypeScale;

    #[test]
    fn a_tooltip_flips_when_the_preferred_side_does_not_fit() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 360.0);
        let anchor = Rect::new(100.0, 10.0, 95.0, 95.0);
        let size = Size::new(300.0, 120.0);
        let rect = place(anchor, size, viewport, Insets::ZERO, Side::Above);
        assert!(rect.is_inside(viewport), "{rect:?}");
        assert!(rect.y >= anchor.bottom(), "flipped below: {rect:?}");
    }

    #[test]
    fn a_tooltip_never_leaves_the_safe_band() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 360.0);
        let safe = Insets::new(40.0, 20.0, 40.0, 32.0);
        let anchor = Rect::new(740.0, 300.0, 48.0, 48.0);
        let rect = place(anchor, Size::new(320.0, 150.0), viewport, safe, Side::Right);
        assert!(rect.x >= 40.0 && rect.right() <= 760.0, "{rect:?}");
        assert!(rect.y >= 20.0 && rect.bottom() <= 328.0, "{rect:?}");
    }

    #[test]
    fn the_pointer_can_walk_into_the_tooltip() {
        let tip = Rect::new(100.0, 100.0, 200.0, 100.0);
        assert!(keeps_open(tip, Point::new(150.0, 150.0), 2.0));
        assert!(keeps_open(tip, Point::new(98.0, 150.0), 4.0));
        assert!(!keeps_open(tip, Point::new(50.0, 150.0), 4.0));
    }

    #[test]
    fn content_lines_are_in_reading_order() {
        let tooltip = Tooltip::new("TRAIN FOOTMAN")
            .body("Basic melee infantry.")
            .cost("Золото 135")
            .cooldown("Восстановление 2.0 с")
            .disabled("Недостаточно пищи")
            .hotkey('F');
        let lines = tooltip.lines();
        assert_eq!(lines[0], "TRAIN FOOTMAN");
        assert_eq!(lines.len(), 5);
        assert!(tooltip.hotkey_line().unwrap().contains('F'));
    }

    #[test]
    fn the_internal_boxes_stay_inside_the_panel() {
        let scale = TypeScale::direction();
        let panel = Rect::new(10.0, 20.0, 400.0, 160.0);
        let tooltip = Tooltip::new("HOLY LIGHT").body("Heals a friendly unit.").cost("Мана 65");
        let layout = layout(panel, &tooltip, &scale, 3.0);
        for part in [layout.title, layout.body, layout.footer] {
            assert!(part.is_inside(layout.panel), "{part:?}");
        }
        assert!(layout.title.h > 0.0);
    }
}
