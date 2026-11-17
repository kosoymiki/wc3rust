//! Exact 2D geometry for unit collision and line of sight.
//!
//! The algorithms are ports of the formulas in the pinned `parry` donor
//! (`donors/parry`, commit `b3c3584ce9be9bf6edb821fd7548b817087131ff`,
//! version 0.31.1). Only the pieces the RTS needs are ported, with `glam` as
//! the vector type and no dependency on parry's shape/dispatch machinery:
//!
//! | ours | donor |
//! |---|---|
//! | [`distance_ball_ball`] | `src/query/distance/distance_ball_ball.rs` |
//! | [`contact_ball_ball`] | `src/query/contact/contact_ball_ball.rs` |
//! | [`distance_ball_aabb`] | `src/query/distance/distance_ball_convex_polyhedron.rs` + `src/query/point/point_aabb.rs` |
//! | [`ray_aabb_toi`] | `src/query/ray/ray_aabb.rs` |
//! | [`sweep_ball_ball`] | `src/query/shape_cast/shape_cast_ball_ball.rs` + `src/query/ray/ray_ball.rs` |
//!
//! The two grid helpers ([`ball_standable`], [`capsule_path_clear`]) replace
//! the simulation's five-point/24-pixel sampling with exact circle-vs-cell and
//! segment-vs-expanded-cell tests. They are deliberately conservative on the
//! rounded (capsule) part of the swept test: a blocked cell is inflated by the
//! unit radius and the segment is clipped against that box, which can only
//! reject a path, never accept an overlapping one.
//!
//! Units in the game are `(x, z)` world coordinates; `y` is height and is
//! ignored here.

use glam::Vec2;
use wc3_path::Grid;

/// A disc: units, trees, mines and projectiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    pub center: Vec2,
    pub radius: f32,
}

impl Ball {
    pub const fn new(center: Vec2, radius: f32) -> Self {
        Self { center, radius }
    }
}

/// An axis-aligned box: building footprints, grid cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub mins: Vec2,
    pub maxs: Vec2,
}

impl Aabb {
    pub fn new(mins: Vec2, maxs: Vec2) -> Self {
        Self {
            mins: mins.min(maxs),
            maxs: mins.max(maxs),
        }
    }

    pub fn from_center_half(center: Vec2, half: Vec2) -> Self {
        Self::new(center - half, center + half)
    }

    pub fn from_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new(Vec2::new(x, y), Vec2::new(x + w, y + h))
    }

    pub fn center(&self) -> Vec2 {
        (self.mins + self.maxs) * 0.5
    }

    pub fn half_extents(&self) -> Vec2 {
        (self.maxs - self.mins) * 0.5
    }

    /// Nearest point of the box (clamped; inside points return themselves).
    pub fn closest_point(&self, point: Vec2) -> Vec2 {
        point.clamp(self.mins, self.maxs)
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.mins.x
            && point.y >= self.mins.y
            && point.x <= self.maxs.x
            && point.y <= self.maxs.y
    }

    pub fn intersects(&self, other: &Aabb) -> bool {
        self.mins.x <= other.maxs.x
            && self.maxs.x >= other.mins.x
            && self.mins.y <= other.maxs.y
            && self.maxs.y >= other.mins.y
    }

    /// Grow the box by `radius` on every side.
    pub fn expanded(&self, radius: f32) -> Self {
        Self::new(
            self.mins - Vec2::splat(radius),
            self.maxs + Vec2::splat(radius),
        )
    }
}

/// One pair of contact points and the separating normal, in world space.
///
/// `normal` points from the first shape to the second; `point1`/`point2` are
/// the witness points on each shape. `dist` is negative when the discs overlap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub point1: Vec2,
    pub point2: Vec2,
    pub normal: Vec2,
    pub dist: f32,
}

/// Result of a swept disc test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    /// Fraction of the motion vector at which the discs touch, in `0..=1`.
    pub time_of_impact: f32,
    /// Unit normal at the hit, pointing from the moving disc to the target.
    pub normal: Vec2,
}

/// Distance between two discs (0 when they overlap).
///
/// Donor: `src/query/distance/distance_ball_ball.rs`.
pub fn distance_ball_ball(a: &Ball, b: &Ball) -> f32 {
    let delta = b.center - a.center;
    let sum = a.radius + b.radius;
    let distance_squared = delta.length_squared();
    if distance_squared <= sum * sum {
        0.0
    } else {
        distance_squared.sqrt() - sum
    }
}

/// Contact between two discs within `prediction` distance.
///
/// Donor: `src/query/contact/contact_ball_ball.rs` (same normal fallback to
/// `+X` when the centres coincide).
pub fn contact_ball_ball(a: &Ball, b: &Ball, prediction: f32) -> Option<Contact> {
    let delta = b.center - a.center;
    let distance_squared = delta.length_squared();
    let sum = a.radius + b.radius;
    let limit = sum + prediction;
    if distance_squared >= limit * limit {
        return None;
    }
    let normal = if distance_squared > 0.0 {
        delta.normalize()
    } else {
        Vec2::X
    };
    Some(Contact {
        point1: a.center + normal * a.radius,
        point2: b.center - normal * b.radius,
        normal,
        dist: distance_squared.sqrt() - sum,
    })
}

/// Distance from a point to a box (0 inside).
///
/// Donor: `src/query/point/point_aabb.rs` (`distance_to_local_point`).
pub fn distance_point_aabb(point: Vec2, aabb: &Aabb) -> f32 {
    let mins = aabb.mins - point;
    let maxs = point - aabb.maxs;
    let shift = mins.max(maxs).max(Vec2::ZERO);
    shift.length()
}

/// Distance from a disc to a box (0 when they overlap).
///
/// Donor: `src/query/distance/distance_ball_convex_polyhedron.rs`
/// (`distance_convex_polyhedron_ball`: project the disc centre, subtract the
/// radius, clamp at zero).
pub fn distance_ball_aabb(ball: &Ball, aabb: &Aabb) -> f32 {
    (distance_point_aabb(ball.center, aabb) - ball.radius).max(0.0)
}

/// True when the disc and the box overlap or touch.
///
/// Closed test (`<=`), unlike parry's strict `<` contact predicate: a unit
/// whose edge grazes a blocked cell is treated as not standing there.
pub fn ball_aabb_intersects(ball: &Ball, aabb: &Aabb) -> bool {
    distance_point_aabb(ball.center, aabb) <= ball.radius
}

/// Segment/AABB clip, returning the entry time in `0..=max_toi`.
///
/// Donor: `src/query/ray/ray_aabb.rs` (`cast_local_ray`, `solid = true`).
pub fn ray_aabb_toi(origin: Vec2, direction: Vec2, aabb: &Aabb, max_toi: f32) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = max_toi;
    for axis in 0..2 {
        let dir = direction[axis];
        let start = origin[axis];
        let (low, high) = (aabb.mins[axis], aabb.maxs[axis]);
        if dir == 0.0 {
            if start < low || start > high {
                return None;
            }
        } else {
            let denom = 1.0 / dir;
            let mut near = (low - start) * denom;
            let mut far = (high - start) * denom;
            if near > far {
                std::mem::swap(&mut near, &mut far);
            }
            tmin = tmin.max(near);
            tmax = tmax.min(far);
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmin > 0.0 && tmin > max_toi {
        None
    } else {
        Some(tmin)
    }
}

/// True when the segment `from -> to` touches the box.
pub fn segment_aabb_intersects(from: Vec2, to: Vec2, aabb: &Aabb) -> bool {
    let direction = to - from;
    if direction.length_squared() == 0.0 {
        return aabb.contains(from);
    }
    ray_aabb_toi(from, direction, aabb, 1.0).is_some()
}

/// Swept contact of disc `a` moving by `delta` against a static disc `b`.
///
/// Donor: `src/query/shape_cast/shape_cast_ball_ball.rs` feeding
/// `src/query/ray/ray_ball.rs::ray_toi_with_ball`; the quadratic below is the
/// same closed form with the relative velocity written directly.
pub fn sweep_ball_ball(a: &Ball, delta: Vec2, b: &Ball, max_toi: f32) -> Option<SweepHit> {
    let radius = a.radius + b.radius;
    let offset = a.center - b.center;
    let aq = delta.length_squared();
    let cq = offset.length_squared() - radius * radius;
    if aq == 0.0 {
        return (cq <= 0.0).then_some(SweepHit {
            time_of_impact: 0.0,
            normal: offset.normalize_or_zero(),
        });
    }
    let bq = offset.dot(delta);
    if cq > 0.0 && bq > 0.0 {
        // Moving apart: the first root is behind the start.
        return None;
    }
    let discriminant = bq * bq - aq * cq;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let mut t = (-bq - root) / aq;
    if t <= 0.0 {
        // Started touching/penetrating (inside): impact at t = 0.
        if cq > 0.0 {
            return None;
        }
        t = 0.0;
    }
    if t > max_toi {
        return None;
    }
    let at = offset + delta * t;
    Some(SweepHit {
        time_of_impact: t,
        // `offset` points from B to A, so `-at` is the normal from the moving
        // disc toward the static one (parry's `normal1` at the hit point).
        normal: (-at).normalize_or_zero(),
    })
}

/// The box of one walkability cell.
pub fn cell_aabb(grid: &Grid, x: i32, y: i32) -> Aabb {
    let size = grid.cell_size;
    Aabb::new(
        Vec2::new(x as f32 * size, y as f32 * size),
        Vec2::new((x as f32 + 1.0) * size, (y as f32 + 1.0) * size),
    )
}

fn cell_walkable(grid: &Grid, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= grid.width as i32 || y >= grid.height as i32 {
        false
    } else {
        grid.walkable(x as u32, y as u32)
    }
}

/// Exact standability of a disc on the walkability grid.
///
/// A unit may stand when no blocked cell overlaps its disc. Cells outside the
/// grid count as blocked. This is the exact replacement for sampling five
/// points around the unit, which misses blocked corners between the samples.
pub fn ball_standable(grid: &Grid, center: Vec2, radius: f32) -> bool {
    let radius = radius.max(0.0);
    let size = grid.cell_size;
    if size <= 0.0 {
        return false;
    }
    let ball = Ball::new(center, radius);
    let min_x = ((center.x - radius) / size).floor() as i32;
    let max_x = ((center.x + radius) / size).floor() as i32;
    let min_y = ((center.y - radius) / size).floor() as i32;
    let max_y = ((center.y + radius) / size).floor() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if cell_walkable(grid, x, y) {
                continue;
            }
            if ball_aabb_intersects(&ball, &cell_aabb(grid, x, y)) {
                return false;
            }
        }
    }
    true
}

/// Exact line of sight for a unit of `radius` moving in a straight line.
///
/// Every blocked cell is inflated by the unit radius (Minkowski sum) and the
/// segment is clipped against it. The inflated box is conservative at rounded
/// corners, so this never reports a blocked path as clear; it does catch thin
/// blocked cells that 24-pixel point sampling steps over.
pub fn capsule_path_clear(grid: &Grid, from: Vec2, to: Vec2, radius: f32) -> bool {
    let radius = radius.max(0.0);
    let size = grid.cell_size;
    if size <= 0.0 {
        return false;
    }
    if from == to {
        return ball_standable(grid, from, radius);
    }
    let min = from.min(to) - Vec2::splat(radius);
    let max = from.max(to) + Vec2::splat(radius);
    let min_x = (min.x / size).floor() as i32;
    let max_x = (max.x / size).floor() as i32;
    let min_y = (min.y / size).floor() as i32;
    let max_y = (max.y / size).floor() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if cell_walkable(grid, x, y) {
                continue;
            }
            let cell = cell_aabb(grid, x, y).expanded(radius);
            if segment_aabb_intersects(from, to, &cell) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(size: u32, walkable: &[(u32, u32, bool)]) -> Grid {
        let mut grid = Grid::new(size, size, 32.0);
        for &(x, y, walk) in walkable {
            grid.set_walkable(x, y, walk);
        }
        grid
    }

    /// The simulation's pre-port standability test: five samples at 0.7r.
    fn sampled_standable(grid: &Grid, pos: Vec2, radius: f32) -> bool {
        let samples = [
            pos,
            pos + Vec2::new(radius * 0.7, 0.0),
            pos - Vec2::new(radius * 0.7, 0.0),
            pos + Vec2::new(0.0, radius * 0.7),
            pos - Vec2::new(0.0, radius * 0.7),
        ];
        for sample in samples {
            let c = grid.world_to_cell(sample);
            if !grid.walkable(c.0, c.1) {
                return false;
            }
        }
        true
    }

    /// The simulation's pre-port line test: 24 px steps with five offsets.
    fn sampled_line_clear(grid: &Grid, from: Vec2, to: Vec2, radius: f32) -> bool {
        let delta = to - from;
        let distance = delta.length();
        if distance < 1.0 {
            return true;
        }
        let steps = (distance / 24.0).ceil() as u32;
        let dir = delta / distance;
        for step in 1..steps {
            let sample = from + dir * (step as f32 * 24.0);
            for offset in [
                Vec2::ZERO,
                Vec2::new(radius, 0.0),
                Vec2::new(-radius, 0.0),
                Vec2::new(0.0, radius),
                Vec2::new(0.0, -radius),
            ] {
                let cell = grid.world_to_cell(sample + offset);
                if !grid.walkable(cell.0, cell.1) {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn ball_ball_distance_matches_parry_closed_form() {
        let a = Ball::new(Vec2::ZERO, 10.0);
        let b = Ball::new(Vec2::new(100.0, 0.0), 5.0);
        assert!((distance_ball_ball(&a, &b) - 85.0).abs() < 1e-4);
        let overlapping = Ball::new(Vec2::new(10.0, 0.0), 5.0);
        assert_eq!(distance_ball_ball(&a, &overlapping), 0.0);
        let touching = Ball::new(Vec2::new(15.0, 0.0), 5.0);
        assert_eq!(distance_ball_ball(&a, &touching), 0.0);
    }

    #[test]
    fn ball_ball_contact_matches_parry_normals_and_witnesses() {
        let a = Ball::new(Vec2::ZERO, 10.0);
        let b = Ball::new(Vec2::new(100.0, 0.0), 5.0);
        let contact = contact_ball_ball(&a, &b, 90.0).expect("within prediction");
        assert!((contact.dist - 85.0).abs() < 1e-4);
        assert_eq!(contact.normal, Vec2::X);
        assert_eq!(contact.point1, Vec2::new(10.0, 0.0));
        assert_eq!(contact.point2, Vec2::new(95.0, 0.0));
        let overlapping = Ball::new(Vec2::new(10.0, 0.0), 5.0);
        let contact = contact_ball_ball(&a, &overlapping, 0.0).expect("overlap is a contact");
        assert!(contact.dist < 0.0);
        // Coincident centres use the donor's +X fallback instead of NaN.
        let same = Ball::new(Vec2::ZERO, 10.0);
        let contact = contact_ball_ball(&a, &same, 0.0).expect("coincident");
        assert_eq!(contact.normal, Vec2::X);
    }

    #[test]
    fn swept_ball_ball_hits_at_the_analytic_time() {
        let a = Ball::new(Vec2::ZERO, 10.0);
        let b = Ball::new(Vec2::new(100.0, 0.0), 5.0);
        let hit = sweep_ball_ball(&a, Vec2::new(100.0, 0.0), &b, 1.0).expect("hit");
        assert!((hit.time_of_impact - 0.85).abs() < 1e-4, "{hit:?}");
        assert!((hit.normal - Vec2::X).length() < 1e-4);
        assert!(sweep_ball_ball(&a, Vec2::new(10.0, 0.0), &b, 1.0).is_none());
        let inside = Ball::new(Vec2::ZERO, 5.0);
        let hit = sweep_ball_ball(&a, Vec2::new(10.0, 0.0), &inside, 1.0).expect("inside");
        assert_eq!(hit.time_of_impact, 0.0);
    }

    #[test]
    fn ray_aabb_clips_like_parry() {
        let box_ = Aabb::from_rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            ray_aabb_toi(Vec2::new(-10.0, 5.0), Vec2::new(20.0, 0.0), &box_, 1.0),
            Some(0.5)
        );
        assert_eq!(
            ray_aabb_toi(Vec2::new(5.0, 5.0), Vec2::new(20.0, 0.0), &box_, 1.0),
            Some(0.0)
        );
        assert_eq!(
            ray_aabb_toi(Vec2::new(-10.0, 50.0), Vec2::new(20.0, 0.0), &box_, 1.0),
            None
        );
        assert_eq!(
            ray_aabb_toi(Vec2::new(-10.0, 5.0), Vec2::new(2.0, 0.0), &box_, 1.0),
            None
        );
    }

    #[test]
    fn ball_aabb_distance_is_zero_inside_and_positive_outside() {
        let box_ = Aabb::from_rect(0.0, 0.0, 100.0, 100.0);
        let inside = Ball::new(Vec2::new(50.0, 50.0), 10.0);
        assert_eq!(distance_ball_aabb(&inside, &box_), 0.0);
        let outside = Ball::new(Vec2::new(110.0, 50.0), 10.0);
        assert_eq!(distance_ball_aabb(&outside, &box_), 0.0);
        let far = Ball::new(Vec2::new(115.0, 50.0), 10.0);
        assert!((distance_ball_aabb(&far, &box_) - 5.0).abs() < 1e-4);
        assert!(ball_aabb_intersects(&outside, &box_));
        assert!(!ball_aabb_intersects(&far, &box_));
    }

    #[test]
    fn exact_standability_is_never_looser_than_sampling() {
        let mut walk = Vec::new();
        for x in 0..12u32 {
            for y in 0..12u32 {
                walk.push((x, y, (x * 7 + y * 3) % 11 != 0));
            }
        }
        let grid = grid(12, &walk);
        let mut missed = 0u32;
        let mut radius = 6.0;
        while radius <= 26.0 {
            let mut y = 0.0f32;
            while y <= 384.0 {
                let mut x = 0.0f32;
                while x <= 384.0 {
                    let pos = Vec2::new(x, y);
                    let sampled = sampled_standable(&grid, pos, radius);
                    let exact = ball_standable(&grid, pos, radius);
                    if sampled && !exact {
                        missed += 1;
                    }
                    assert!(
                        !exact || sampled,
                        "exact accepted a position the five-point sample blocks at {pos:?} r={radius}"
                    );
                    x += 8.0;
                }
                y += 8.0;
            }
            radius += 5.0;
        }
        assert!(
            missed > 0,
            "the sampled test must have corner false negatives"
        );
    }

    #[test]
    fn exact_standability_catches_the_corner_the_sample_misses() {
        // One blocked cell (2,2) covering [64,96]^2. A unit of radius 26 at
        // (46,46) reaches the corner (64,64) at distance 25.46, but all five
        // 0.7r samples fall in walkable cells.
        let grid = grid(8, &[(2, 2, false)]);
        let pos = Vec2::new(46.0, 46.0);
        assert!(sampled_standable(&grid, pos, 26.0));
        assert!(!ball_standable(&grid, pos, 26.0));
    }

    #[test]
    fn exact_line_of_sight_catches_a_thin_blocked_cell() {
        let mut grid = Grid::new(128, 128, 1.0);
        grid.set_walkable(60, 60, false);
        let from = Vec2::new(2.0, 2.0);
        let to = Vec2::new(100.0, 100.0);
        // 24 px sampling steps land at x = 18.97, 35.94, 52.91, 69.88, 86.85;
        // the 1x1 blocked cell at (60,60) sits between samples but exactly on
        // the line.
        assert!(sampled_line_clear(&grid, from, to, 0.25));
        assert!(!capsule_path_clear(&grid, from, to, 0.25));
    }

    #[test]
    fn capsule_line_matches_standability_for_a_point() {
        let grid = grid(8, &[(4, 4, false)]);
        assert!(!capsule_path_clear(
            &grid,
            Vec2::new(144.0, 144.0),
            Vec2::new(144.0, 144.0),
            8.0
        ));
        assert!(capsule_path_clear(
            &grid,
            Vec2::new(16.0, 16.0),
            Vec2::new(16.0, 100.0),
            4.0
        ));
    }
}
