//! Device-independent geometry for the pure UI layer.
//!
//! The layer works in **dp** (logical units) only. `px = dp * density` happens
//! exactly once, at the render/adapter boundary (`MobileHudLayout::to_px`), so
//! nothing in the solver can double-scale a rect. These types are plain data
//! with no `glam` or GPU dependency, which is what makes `solve_layout`
//! auditable and testable without a window.

/// A point in dp.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    pub fn distance(self, other: Point) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A size in dp.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const ZERO: Size = Size { w: 0.0, h: 0.0 };

    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }

    pub const fn splat(v: f32) -> Self {
        Self { w: v, h: v }
    }

    /// A size that can never be negative or NaN.
    pub fn sane(self) -> Self {
        Self {
            w: if self.w.is_finite() { self.w.max(0.0) } else { 0.0 },
            h: if self.h.is_finite() { self.h.max(0.0) } else { 0.0 },
        }
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn area(self) -> f32 {
        self.w * self.h
    }
}

/// An axis-aligned rectangle in dp: origin at the top-left, `w`/`h` positive.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn from_size(size: Size) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: size.w,
            h: size.h,
        }
    }

    pub const fn from_origin_size(origin: Point, size: Size) -> Self {
        Self {
            x: origin.x,
            y: origin.y,
            w: size.w,
            h: size.h,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub fn centre(self) -> Point {
        Point::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    pub fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn area(self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.w * self.h
        }
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.right()
            && point.y >= self.y
            && point.y < self.bottom()
    }

    /// `self` is fully inside `other` (touching edges allowed).
    pub fn is_inside(self, other: Rect) -> bool {
        const EPS: f32 = 0.01;
        self.x >= other.x - EPS
            && self.y >= other.y - EPS
            && self.right() <= other.right() + EPS
            && self.bottom() <= other.bottom() + EPS
    }

    /// `self` fully contains `other`.
    pub fn contains_rect(self, other: Rect) -> bool {
        other.is_inside(self)
    }

    /// Symmetric insets: `inset(4.0, 8.0)` removes 8 horizontally, 4 vertically.
    pub fn inset(self, x: f32, y: f32) -> Rect {
        Rect::new(
            self.x + x,
            self.y + y,
            (self.w - 2.0 * x).max(0.0),
            (self.h - 2.0 * y).max(0.0),
        )
    }

    pub fn inset_each(self, insets: Insets) -> Rect {
        Rect::new(
            self.x + insets.left,
            self.y + insets.top,
            (self.w - insets.horizontal()).max(0.0),
            (self.h - insets.vertical()).max(0.0),
        )
    }

    pub fn translate(self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    pub fn centre_in(self, size: Size) -> Rect {
        Rect::new(
            self.x + (self.w - size.w) * 0.5,
            self.y + (self.h - size.h) * 0.5,
            size.w,
            size.h,
        )
    }

    /// Area of the intersection with `other` (0 when they only touch).
    pub fn intersection_area(self, other: Rect) -> f32 {
        let dx = self.right().min(other.right()) - self.x.max(other.x);
        let dy = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if dx <= 0.0 || dy <= 0.0 {
            0.0
        } else {
            dx * dy
        }
    }

    pub fn intersection(self, other: Rect) -> Rect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        if x1 <= x0 || y1 <= y0 {
            Rect::ZERO
        } else {
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }
    }

    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// Move `self` the minimum distance that puts it inside `bounds`, shrinking
    /// it only when it is larger than the bounds.
    pub fn clamp_inside(self, bounds: Rect) -> Rect {
        let w = self.w.min(bounds.w.max(0.0));
        let h = self.h.min(bounds.h.max(0.0));
        let x = self.x.clamp(bounds.x, (bounds.right() - w).max(bounds.x));
        let y = self.y.clamp(bounds.y, (bounds.bottom() - h).max(bounds.y));
        Rect::new(x, y, w, h)
    }

    /// Snap to whole dp (sub-dp chrome is invisible and it keeps dumps stable).
    pub fn rounded(self) -> Rect {
        Rect::new(
            self.x.round(),
            self.y.round(),
            self.w.round(),
            self.h.round(),
        )
    }
}

/// Per-edge insets in dp: system bars, notches, gesture areas, padding.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const ZERO: Insets = Insets {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn splat(v: f32) -> Self {
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }

    /// Symmetric horizontal / vertical pair: `Insets::xy(8.0, 4.0)`.
    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x,
            bottom: y,
        }
    }

    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    pub fn is_zero(self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }

    /// Clamp every edge to `[0, viewport * max_fraction]`.
    ///
    /// A platform that reports a bogus inset (a rotated display, a stale
    /// navigation bar) must not be able to eat the interface: the solver keeps
    /// at least `1 - 2 * max_fraction` of each axis available.
    pub fn sanitised(self, viewport: Size, max_fraction: f32) -> Insets {
        let max_x = (viewport.w * max_fraction).max(0.0);
        let max_y = (viewport.h * max_fraction).max(0.0);
        let sane = |v: f32, max: f32| if v.is_finite() { v.clamp(0.0, max) } else { 0.0 };
        Insets {
            left: sane(self.left, max_x),
            top: sane(self.top, max_y),
            right: sane(self.right, max_x),
            bottom: sane(self.bottom, max_y),
        }
    }

    pub fn scaled(self, factor: f32) -> Insets {
        Insets {
            left: self.left * factor,
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_report_edges_and_containment() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert_eq!(r.centre(), Point::new(60.0, 45.0));
        assert!(r.contains(Point::new(10.0, 20.0)));
        assert!(!r.contains(Point::new(110.0, 20.0)));
        assert!(Rect::new(20.0, 30.0, 10.0, 10.0).is_inside(r));
        assert!(!Rect::new(0.0, 30.0, 10.0, 10.0).is_inside(r));
    }

    #[test]
    fn overlap_counts_area_and_touching_edges_are_free() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection_area(b), 0.0);
        assert_eq!(a.intersection_area(Rect::new(5.0, 5.0, 10.0, 10.0)), 25.0);
        assert_eq!(a.union(b).w, 20.0);
    }

    #[test]
    fn insets_sanitise_a_bogus_platform_report() {
        let viewport = Size::new(800.0, 360.0);
        let bad = Insets::new(-5.0, f32::NAN, 400.0, 500.0);
        let safe = bad.sanitised(viewport, 0.4);
        assert_eq!(safe.left, 0.0);
        assert_eq!(safe.top, 0.0);
        assert_eq!(safe.right, 320.0);
        assert_eq!(safe.bottom, 144.0);
        assert!(viewport.w - safe.horizontal() >= viewport.w * 0.2);
    }

    #[test]
    fn clamp_inside_never_shrinks_below_zero() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        let moved = Rect::new(120.0, -20.0, 40.0, 40.0).clamp_inside(bounds);
        assert_eq!(moved, Rect::new(60.0, 0.0, 40.0, 40.0));
        let huge = Rect::new(-50.0, -50.0, 400.0, 400.0).clamp_inside(bounds);
        assert_eq!(huge, bounds);
    }
}
