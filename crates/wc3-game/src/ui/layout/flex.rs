//! Row/column distribution for the solver.
//!
//! One pure function decides how a line of children shares its main axis:
//! fixed and content-sized items take what they need, the leftover is split
//! between weighted fill items, every item is clamped into its own floor and
//! ceiling, and anything that still does not fit is reported as an overflow
//! instead of being pushed off-screen.

use crate::ui::core::constraints::Extent;

/// One item on the main axis.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Item {
    /// The declared extent on the main axis.
    pub extent: Extent,
    /// Fill weight (`<= 0` behaves like 1).
    pub weight: f32,
}

impl Item {
    pub const fn new(extent: Extent) -> Self {
        Item { extent, weight: 1.0 }
    }

    pub const fn weighted(extent: Extent, weight: f32) -> Self {
        Item { extent, weight }
    }

    fn weight(&self) -> f32 {
        if self.weight.is_finite() && self.weight > 0.0 {
            self.weight
        } else {
            1.0
        }
    }
}

/// Result of one distribution pass.
#[derive(Clone, Debug, PartialEq)]
pub struct Distribution {
    /// Granted size per item, in declaration order.
    pub sizes: Vec<f32>,
    /// Sum of granted sizes.
    pub used: f32,
    /// Space still free after every item (never negative).
    pub slack: f32,
    /// How much the items wanted beyond `avail` (0 when everything fits).
    pub overflow: f32,
    /// Indexes that had to be clamped below their floor because the line is
    /// too short to honour it.
    pub squeezed: Vec<usize>,
}

/// Distribute `avail` dp across `items` with `gap` dp between them.
pub fn distribute(avail: f32, gap: f32, items: &[Item]) -> Distribution {
    let avail = if avail.is_finite() { avail.max(0.0) } else { 0.0 };
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let count = items.len();
    if count == 0 {
        return Distribution {
            sizes: Vec::new(),
            used: 0.0,
            slack: avail,
            overflow: 0.0,
            squeezed: Vec::new(),
        };
    }
    let gaps = gap * (count.saturating_sub(1)) as f32;
    let inner = (avail - gaps).max(0.0);

    // Pass 1: every item that does not fill takes its desired size.
    let mut sizes = vec![0.0_f32; count];
    let mut fixed_total = 0.0_f32;
    let mut fill_weight = 0.0_f32;
    for (index, item) in items.iter().enumerate() {
        if item.extent.fills() {
            fill_weight += item.weight();
        } else {
            let size = item.extent.resolve(inner, 0.0);
            sizes[index] = size;
            fixed_total += size;
        }
    }

    // Pass 2: the leftover is shared by weight; a clamped item returns its
    // surplus to the pool, which the remaining fill items then split.
    let mut leftover = (inner - fixed_total).max(0.0);
    if fill_weight > 0.0 && leftover > 0.0 {
        let mut pending: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.extent.fills())
            .map(|(index, _)| index)
            .collect();
        // Three rounds are enough for the HUD's nesting depth (one fill per
        // line in practice) and keep the function O(3n).
        for _ in 0..3 {
            if pending.is_empty() || leftover <= 0.0 {
                break;
            }
            let weight: f32 = pending.iter().map(|i| items[*i].weight()).sum();
            if weight <= 0.0 {
                break;
            }
            let pool = leftover;
            let mut spent = 0.0_f32;
            let mut still: Vec<usize> = Vec::new();
            for index in pending {
                let item = &items[index];
                let want = pool * item.weight() / weight;
                let size = item.extent.resolve(inner, want);
                sizes[index] = size;
                spent += size;
                // An item that wanted more than its share and got clamped by
                // `max` should not keep the leftover: mark it saturated.
                if want < item.extent.desired(inner, want) - 0.01 {
                    still.push(index);
                }
            }
            leftover = (leftover - spent).max(0.0);
            pending = still;
        }
    }

    let used: f32 = sizes.iter().sum::<f32>() + gaps;
    let mut squeezed = Vec::new();
    let over = used > avail + 0.01;
    for (index, item) in items.iter().enumerate() {
        // An item below its floor is squeezed; so is every item on a line that
        // cannot host the declared floors at all (the caller must re-flow).
        if sizes[index] + 0.01 < item.extent.floor() || (over && item.extent.floor() > 0.0) {
            squeezed.push(index);
        }
    }
    Distribution {
        sizes,
        used,
        slack: (avail - used).max(0.0),
        overflow: (used - avail).max(0.0),
        squeezed,
    }
}

/// Column/row cross-axis alignment helper.
pub fn align_offset(align: f32, space: f32, size: f32) -> f32 {
    match align {
        0.0 => 0.0,
        0.5 => (space - size) * 0.5,
        _ => space - size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_items_take_their_size_and_fill_shares_the_rest() {
        let items = [
            Item::new(Extent::fixed(48.0)),
            Item::new(Extent::fill()),
            Item::new(Extent::fixed(100.0)),
        ];
        let d = distribute(400.0, 8.0, &items);
        assert!((d.sizes[0] - 48.0).abs() < 0.01);
        assert!((d.sizes[2] - 100.0).abs() < 0.01);
        // 400 - 48 - 100 - 2*8 = 236
        assert!((d.sizes[1] - 236.0).abs() < 0.01);
        assert!(d.overflow.abs() < 0.01);
        assert!(d.slack.abs() < 0.01);
    }

    #[test]
    fn the_six_150px_buttons_incident_is_reported_not_clipped() {
        // 6 * 150 dp + 5 * 8 dp = 940 dp inside an 864 dp frame.
        let items = [Item::new(Extent::fixed(150.0)); 6];
        let d = distribute(864.0, 8.0, &items);
        assert!(d.overflow > 75.0, "overflow {:.1}", d.overflow);
        assert_eq!(d.used, 940.0);
        assert!(d.slack.abs() < 0.01);
        // Sizes still sum to `used`: the caller must pick a fallback plan, and
        // the arithmetic is honest about what it asked for.
        let sum: f32 = d.sizes.iter().sum::<f32>() + 40.0;
        assert!((sum - 940.0).abs() < 0.01);
    }

    #[test]
    fn weights_split_the_leftover_proportionally() {
        let items = [
            Item::weighted(Extent::fill(), 2.0),
            Item::weighted(Extent::fill(), 1.0),
        ];
        let d = distribute(308.0, 8.0, &items);
        assert!((d.sizes[0] - 200.0).abs() < 0.5, "{:?}", d.sizes);
        assert!((d.sizes[1] - 100.0).abs() < 0.5, "{:?}", d.sizes);
    }

    #[test]
    fn a_floor_larger_than_the_line_is_reported_as_squeezed() {
        let items = [Item::new(Extent::at_least(48.0)); 4];
        let d = distribute(100.0, 8.0, &items);
        assert_eq!(d.squeezed.len(), 4, "every item reports its floor");
        assert!(d.overflow > 100.0, "4 x 48 + 3 x 8 cannot fit 100 dp");
        for size in &d.sizes {
            assert!(*size <= 100.0, "no item exceeds the line");
        }
    }

    #[test]
    fn empty_line_is_all_slack() {
        let d = distribute(500.0, 8.0, &[]);
        assert_eq!(d.used, 0.0);
        assert_eq!(d.slack, 500.0);
    }
}
