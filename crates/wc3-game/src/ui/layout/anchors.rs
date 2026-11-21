//! Anchor resolution (hud.md §6.1 `layout/anchors.rs`).
//!
//! Anchors are how the HUD expresses *intent* ("the command card hugs the safe
//! right edge, vertically centred in the band") instead of coordinates. The
//! solver resolves them against the rect the parent actually got, so a zone can
//! never be anchored to a screen edge the safe area has already moved.

use crate::ui::core::element::{AlignX, AlignY, Anchor};
use crate::ui::core::geometry::{Point, Rect, Size};

/// Resolve an anchor to a top-left corner inside `parent`.
pub fn resolve(anchor: Anchor, parent: Rect, size: Size) -> Point {
    let x = match anchor.x {
        AlignX::Start => parent.x,
        AlignX::Center => parent.x + (parent.w - size.w) * 0.5,
        AlignX::End => parent.right() - size.w,
        AlignX::Stretch => parent.x,
    };
    let y = match anchor.y {
        AlignY::Start => parent.y,
        AlignY::Center => parent.y + (parent.h - size.h) * 0.5,
        AlignY::End => parent.bottom() - size.h,
        AlignY::Stretch => parent.y,
    };
    Point::new(x, y)
}

/// Size an anchored child gets: a stretch edge overrides the declared length.
pub fn stretch_size(anchor: Anchor, declared: Size, parent: Rect) -> Size {
    Size::new(
        if anchor.x == AlignX::Stretch {
            parent.w
        } else {
            declared.w
        },
        if anchor.y == AlignY::Stretch {
            parent.h
        } else {
            declared.h
        },
    )
}

/// Place `size` against `anchor` with a dp offset, clamped inside `bounds`.
pub fn place(anchor: Anchor, bounds: Rect, size: Size, offset: Point) -> Rect {
    let origin = resolve(anchor, bounds, size);
    Rect::new(
        origin.x + offset.x,
        origin.y + offset.y,
        size.w,
        size.h,
    )
    .clamp_inside(bounds)
}

/// A full-band rect starting at `y` (rows that span the parent width).
pub fn band(parent: Rect, y: f32, height: f32) -> Rect {
    Rect::new(parent.x, y, parent.w, height.max(0.0))
}

/// The rect above `bottom` with `height`, never entering `top_limit`.
pub fn above(bottom: f32, height: f32, x: f32, width: f32, top_limit: f32) -> Rect {
    let y = (bottom - height).max(top_limit);
    Rect::new(x, y, width, height.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_pin_the_parent_corners() {
        let parent = Rect::new(10.0, 20.0, 200.0, 100.0);
        let size = Size::new(48.0, 48.0);
        assert_eq!(
            resolve(Anchor::TOP_LEFT, parent, size),
            Point::new(10.0, 20.0)
        );
        assert_eq!(
            resolve(Anchor::BOTTOM_RIGHT, parent, size),
            Point::new(162.0, 72.0)
        );
        assert_eq!(
            resolve(Anchor::CENTER, parent, size),
            Point::new(86.0, 46.0)
        );
    }

    #[test]
    fn stretch_overrides_declared_size() {
        let parent = Rect::new(0.0, 0.0, 300.0, 50.0);
        let size = stretch_size(Anchor::FILL, Size::new(10.0, 10.0), parent);
        assert_eq!(size, Size::new(300.0, 50.0));
        let top = stretch_size(Anchor::TOP_LEFT, Size::new(10.0, 10.0), parent);
        assert_eq!(top, Size::new(10.0, 10.0));
    }

    #[test]
    fn placement_is_clamped_inside_the_bounds() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        let rect = place(
            Anchor::TOP_LEFT,
            bounds,
            Size::new(48.0, 48.0),
            Point::new(90.0, 90.0),
        );
        assert_eq!(rect, Rect::new(52.0, 52.0, 48.0, 48.0));
    }

    #[test]
    fn above_never_crosses_the_limit() {
        let rect = above(500.0, 44.0, 10.0, 200.0, 100.0);
        assert!(rect.y >= 100.0);
        assert_eq!(rect.bottom(), 500.0);
    }
}
