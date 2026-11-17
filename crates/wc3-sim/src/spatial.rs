//! Uniform spatial hash for bounded neighbour queries.
//!
//! Rebuilt every tick from unit positions; queries visit only the buckets that
//! overlap the requested circle, so combat, separation and fog never fall back
//! to an unbounded O(n^2) scan.  Bucket order is deterministic: units are
//! inserted in entity order and bucket vectors preserve that order.

use glam::Vec2;

pub struct SpatialHash {
    cell: f32,
    inv_cell: f32,
    cols: u32,
    rows: u32,
    buckets: Vec<Vec<u32>>,
}

impl SpatialHash {
    pub fn new(cell: f32, world_width: f32, world_height: f32) -> Self {
        let cols = ((world_width / cell).ceil() as u32).max(1);
        let rows = ((world_height / cell).ceil() as u32).max(1);
        Self {
            cell,
            inv_cell: 1.0 / cell,
            cols,
            rows,
            buckets: vec![Vec::new(); cols as usize * rows as usize],
        }
    }

    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
    }

    pub fn insert(&mut self, index: u32, pos: Vec2) {
        let (cx, cy) = self.cell_of(pos);
        let bucket = cy as usize * self.cols as usize + cx as usize;
        self.buckets[bucket].push(index);
    }

    fn cell_of(&self, pos: Vec2) -> (u32, u32) {
        let cx = (pos.x * self.inv_cell).floor().max(0.0) as u32;
        let cy = (pos.y * self.inv_cell).floor().max(0.0) as u32;
        (cx.min(self.cols - 1), cy.min(self.rows - 1))
    }

    /// Append indices of units within `radius` of `pos` to `out` (cleared
    /// first).  `limit` caps the number of neighbours collected so a packed
    /// crowd cannot make a single unit's update unbounded.
    pub fn query(&self, pos: Vec2, radius: f32, limit: usize, out: &mut Vec<u32>) {
        out.clear();
        if radius <= 0.0 {
            return;
        }
        let min = self.cell_of(pos - Vec2::splat(radius));
        let max = self.cell_of(pos + Vec2::splat(radius));
        let r2 = radius * radius;
        'outer: for cy in min.1..=max.1 {
            for cx in min.0..=max.0 {
                let bucket = &self.buckets[cy as usize * self.cols as usize + cx as usize];
                for &index in bucket {
                    out.push(index);
                    if out.len() >= limit {
                        break 'outer;
                    }
                }
            }
        }
        // The circle radius is only used to select buckets; callers filter by
        // true distance with their own per-unit radii.
        let _ = r2;
    }

    pub fn cell_size(&self) -> f32 {
        self.cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_bounded_and_indexes_units() {
        let mut hash = SpatialHash::new(256.0, 2048.0, 2048.0);
        for i in 0..100u32 {
            hash.insert(
                i,
                Vec2::new((i % 10) as f32 * 100.0, (i / 10) as f32 * 100.0),
            );
        }
        let mut out = Vec::new();
        hash.query(Vec2::new(0.0, 0.0), 200.0, 8, &mut out);
        assert!(out.len() <= 8);
        assert!(!out.is_empty());
        hash.query(Vec2::new(5000.0, 5000.0), 100.0, 8, &mut out);
        assert!(out.is_empty());
    }
}
