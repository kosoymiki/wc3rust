//! Size constraints for the layout solver.
//!
//! Every element declares how each axis is sized. The solver resolves the
//! declaration against the space it actually received, then clamps into the
//! element's own `min`/`max`. Nothing can request "176 dp" and silently get
//! drawn outside its parent: the only sizing primitives are fixed dp, a
//! fraction of the parent, a weighted fill of the leftover space, and hug
//! (content minimum).

use super::geometry::Size;

/// How one axis picks a size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Len {
    /// Exactly this many dp (clamped into `min`/`max`).
    Fixed(f32),
    /// A fraction of the parent's available size on this axis, in `0..=1`.
    Fraction(f32),
    /// Share the leftover space with the siblings that also fill, by weight.
    Fill,
    /// The content minimum (never larger than the parent).
    Hug,
}

impl Default for Len {
    fn default() -> Self {
        Len::Hug
    }
}

/// One axis: a [`Len`] plus the absolute bounds it may never leave.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Extent {
    pub len: Len,
    pub min: f32,
    pub max: f32,
}

impl Default for Extent {
    fn default() -> Self {
        Extent {
            len: Len::Hug,
            min: 0.0,
            max: f32::INFINITY,
        }
    }
}

impl Extent {
    /// Exactly `v` dp.
    pub const fn fixed(v: f32) -> Self {
        Extent {
            len: Len::Fixed(v),
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    /// At least `v` dp; the upper bound stays open (use with `Fill`/`Fraction`).
    pub const fn at_least(v: f32) -> Self {
        Extent {
            len: Len::Fill,
            min: v,
            max: f32::INFINITY,
        }
    }

    /// Between `min` and `max` dp, filling the parent.
    pub const fn range(min: f32, max: f32) -> Self {
        Extent {
            len: Len::Fill,
            min,
            max,
        }
    }

    /// `fraction` of the parent (`0.34` = 34 %).
    pub const fn fraction(fraction: f32) -> Self {
        Extent {
            len: Len::Fraction(fraction),
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    /// Weighted fill of the leftover space.
    pub const fn fill() -> Self {
        Extent {
            len: Len::Fill,
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    /// Content minimum.
    pub const fn hug() -> Self {
        Extent {
            len: Len::Hug,
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub const fn with_min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub const fn with_max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    /// The smallest size this axis may take.
    pub fn floor(&self) -> f32 {
        if self.min.is_finite() {
            self.min.max(0.0)
        } else {
            0.0
        }
    }

    /// What this axis would take if space were free.
    ///
    /// This is the number the diagnostics compare against the granted size: a
    /// request that does not fit is reported, never silently clipped.
    pub fn desired(&self, avail: f32, share: f32) -> f32 {
        let avail = sane_avail(avail);
        let raw = match self.len {
            Len::Fixed(v) => v,
            Len::Fraction(f) => avail * f.clamp(0.0, 1.0),
            Len::Fill => {
                if share > 0.0 {
                    share
                } else {
                    avail
                }
            }
            Len::Hug => self.floor(),
        };
        let low = self.floor();
        let high = if self.max.is_finite() {
            self.max.max(low)
        } else {
            f32::INFINITY
        };
        if raw.is_finite() {
            raw.clamp(low, high)
        } else {
            low
        }
    }

    /// Resolve one axis inside a parent that can only give `avail`.
    ///
    /// * `avail` — the space the parent can give on this axis.
    /// * `share` — the space already reserved for this element when it fills
    ///   (leftover split by weight); ignored for the other lengths.
    ///
    /// The result is always `<= avail`: the solver's `used <= avail` contract
    /// starts from this single function.
    pub fn resolve(&self, avail: f32, share: f32) -> f32 {
        let avail = sane_avail(avail);
        self.desired(avail, share).min(avail)
    }

    /// True when this axis wants a share of the leftover space.
    pub fn fills(&self) -> bool {
        matches!(self.len, Len::Fill | Len::Fraction(_))
    }
}

/// Full constraint set for one element.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Constraints {
    pub width: Extent,
    pub height: Extent,
    /// Weight inside a flex parent when both axes fill (default 1).
    pub weight: f32,
    /// Enforced width / height ratio (`w / h`), applied after both axes resolve.
    pub aspect: Option<f32>,
}

impl Constraints {
    /// A fixed dp box.
    pub const fn fixed(w: f32, h: f32) -> Self {
        Constraints {
            width: Extent::fixed(w),
            height: Extent::fixed(h),
            weight: 1.0,
            aspect: None,
        }
    }

    /// Fill the parent on both axes.
    pub const fn fill() -> Self {
        Constraints {
            width: Extent::fill(),
            height: Extent::fill(),
            weight: 1.0,
            aspect: None,
        }
    }

    /// Fill horizontally, fixed height.
    pub const fn band(h: f32) -> Self {
        Constraints {
            width: Extent::fill(),
            height: Extent::fixed(h),
            weight: 1.0,
            aspect: None,
        }
    }

    /// A square box between `min` and `max` dp that fills what it is given.
    pub const fn square(min: f32, max: f32) -> Self {
        Constraints {
            width: Extent::range(min, max),
            height: Extent::range(min, max),
            weight: 1.0,
            aspect: Some(1.0),
        }
    }

    pub const fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub const fn with_aspect(mut self, aspect: f32) -> Self {
        self.aspect = Some(aspect);
        self
    }

    /// Resolve both axes; the aspect ratio wins over the raw height.
    pub fn resolve(&self, avail: Size, share: Size) -> Size {
        let avail = Size::new(sane_avail(avail.w), sane_avail(avail.h));
        let mut w = self.width.resolve(avail.w, share.w);
        let mut h = self.height.resolve(avail.h, share.h);
        if let Some(aspect) = self.aspect {
            if aspect.is_finite() && aspect > 0.0 && w > 0.0 {
                // Square-ish chrome keeps the smaller side so it never clips.
                h = w / aspect;
                if h > avail.h {
                    h = avail.h;
                    w = h * aspect;
                }
                h = h.min(avail.h);
                w = w.min(avail.w);
            }
        }
        Size::new(w, h)
    }

    /// The smallest size this element accepts on both axes.
    pub fn floor(&self) -> Size {
        Size::new(self.width.floor(), self.height.floor())
    }
}

/// `avail` with NaN/infinity/negatives removed.
fn sane_avail(avail: f32) -> f32 {
    if avail.is_finite() {
        avail.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_is_clamped_into_bounds() {
        let e = Extent::fixed(200.0).with_max(120.0);
        assert_eq!(e.resolve(400.0, 0.0), 120.0);
        let e = Extent::fixed(10.0).with_min(48.0);
        assert_eq!(e.resolve(400.0, 0.0), 48.0);
    }

    #[test]
    fn fraction_and_fill_differ() {
        let f = Extent::fraction(0.25);
        assert_eq!(f.resolve(800.0, 0.0), 200.0);
        let fill = Extent::fill();
        assert_eq!(fill.resolve(800.0, 260.0), 260.0);
        assert_eq!(fill.resolve(800.0, 0.0), 800.0);
    }

    #[test]
    fn square_constraint_keeps_the_smaller_side() {
        let c = Constraints::square(96.0, 144.0);
        let s = c.resolve(Size::new(400.0, 120.0), Size::ZERO);
        assert!((s.w - s.h).abs() < 0.01);
        assert!(s.h <= 120.0 + 0.01);
        assert!(s.w >= 96.0 - 0.01);
    }

    #[test]
    fn hug_never_exceeds_the_parent() {
        let e = Extent::hug().with_min(300.0);
        assert_eq!(e.resolve(120.0, 0.0), 120.0);
    }
}
