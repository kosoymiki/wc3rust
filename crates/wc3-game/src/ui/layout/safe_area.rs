//! Safe-area as a solver input (hud.md §1.4 / §7.4).
//!
//! The platform reports insets in framebuffer pixels; the solver needs dp. This
//! is the one place the conversion happens, and the result is always sanitised:
//! a stale or rotated value can never remove more than `MAX_SAFE_FRACTION` of
//! an axis, and a zero report is a legal input (never the default assumption
//! that lets a notch sit on top of a button).

use crate::ui::core::geometry::{Insets, Rect, Size};

/// No single edge may claim more than this fraction of its axis.
pub const MAX_SAFE_FRACTION: f32 = 0.40;

/// Convert platform pixel insets into dp and clamp them.
pub fn from_pixels(insets: Insets, density: f32, viewport: Size) -> Insets {
    let d = sane_density(density);
    Insets::new(
        insets.left / d,
        insets.top / d,
        insets.right / d,
        insets.bottom / d,
    )
    .sanitised(viewport, MAX_SAFE_FRACTION)
}

/// The dp viewport after the safe insets.
pub fn content_rect(viewport: Size, safe: Insets) -> Rect {
    let safe = safe.sanitised(viewport, MAX_SAFE_FRACTION);
    Rect::new(
        safe.left,
        safe.top,
        (viewport.w - safe.horizontal()).max(1.0),
        (viewport.h - safe.vertical()).max(1.0),
    )
}

/// Density that can never divide by zero or invert a layout.
pub fn sane_density(density: f32) -> f32 {
    if density.is_finite() && density > 0.05 {
        density.clamp(0.5, 8.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_insets_become_dp() {
        let viewport = Size::new(804.7, 360.0);
        let safe = from_pixels(Insets::new(210.0, 120.0, 90.0, 96.0), 3.0, viewport);
        assert!((safe.left - 70.0).abs() < 0.01);
        assert!((safe.top - 40.0).abs() < 0.01);
        assert!((safe.right - 30.0).abs() < 0.01);
        assert!((safe.bottom - 32.0).abs() < 0.01);
    }

    #[test]
    fn insane_insets_cannot_eat_the_screen() {
        let viewport = Size::new(800.0, 360.0);
        let safe = from_pixels(Insets::new(9999.0, 9999.0, 9999.0, 9999.0), 3.0, viewport);
        assert!(viewport.w - safe.horizontal() >= viewport.w * 0.2 - 0.01);
        assert!(viewport.h - safe.vertical() >= viewport.h * 0.2 - 0.01);
    }

    #[test]
    fn content_rect_keeps_at_least_one_dp() {
        let viewport = Size::new(10.0, 10.0);
        let rect = content_rect(viewport, Insets::splat(40.0));
        assert!(rect.w >= 1.0 && rect.h >= 1.0);
    }

    #[test]
    fn zero_density_falls_back_to_one() {
        assert_eq!(sane_density(0.0), 1.0);
        assert_eq!(sane_density(f32::NAN), 1.0);
        assert_eq!(sane_density(3.0), 3.0);
    }
}
