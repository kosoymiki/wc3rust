//! Bounded pathfinding and group movement helpers.
//!
//! A* uses generation-stamped scratch arrays that are reused across searches,
//! so a 200-unit manoeuvre does not allocate a fresh visited set per unit.
//! Group moves use a single flow field (`Grid::flow_field`) plus a truncated
//! gradient descent per unit, which keeps cost linear in unit count instead of
//! running hundreds of independent searches in the same tick.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::Vec2;
use wc3_path::Grid;

/// Hard cap on expanded nodes for one search.
///
/// A 512x384 map (the largest melee map shipped with `(2)Justice.w3x`) has
/// 196 608 cells, so the old 1 000 000 cap let a single unreachable target
/// expand every cell and cost ~0.1 s per request in a debug build.  65 536
/// expansions still covers a 100+ cell detour around any obstacle while
/// keeping an unreachable search bounded.
pub const MAX_ASTAR_NODES: u32 = 65_536;

pub struct PathFinder {
    width: u32,
    height: u32,
    cost: Vec<u32>,
    parent: Vec<u32>,
    stamp: Vec<u32>,
    generation: u32,
    heap: BinaryHeap<Reverse<(u32, u32, u32)>>,
}

impl PathFinder {
    pub fn new(width: u32, height: u32) -> Self {
        let size = width as usize * height as usize;
        Self {
            width,
            height,
            cost: vec![0; size],
            parent: vec![u32::MAX; size],
            stamp: vec![0; size],
            generation: 0,
            heap: BinaryHeap::new(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        let size = width as usize * height as usize;
        self.width = width;
        self.height = height;
        self.cost = vec![0; size];
        self.parent = vec![u32::MAX; size];
        self.stamp = vec![0; size];
        self.generation = 0;
    }

    fn next_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.fill(0);
            self.generation = 1;
        }
    }

    fn heuristic(&self, a: (u32, u32), b: (u32, u32)) -> u32 {
        let dx = a.0.abs_diff(b.0);
        let dy = a.1.abs_diff(b.1);
        10 * dx.max(dy) + 4 * dx.min(dy)
    }

    /// A* with 8-neighbour movement, no corner cutting, and a node budget.
    /// Returns the cell path including start and goal.
    pub fn astar(
        &mut self,
        grid: &Grid,
        start: (u32, u32),
        goal: (u32, u32),
        max_nodes: u32,
    ) -> Option<Vec<(u32, u32)>> {
        if start.0 >= self.width || start.1 >= self.height {
            return None;
        }
        if goal.0 >= self.width || goal.1 >= self.height {
            return None;
        }
        if !grid.walkable(start.0, start.1) || !grid.walkable(goal.0, goal.1) {
            return None;
        }
        if start == goal {
            return Some(vec![start]);
        }
        self.next_generation();
        let start_i = start.1 as usize * self.width as usize + start.0 as usize;
        let goal_i = goal.1 as usize * self.width as usize + goal.0 as usize;
        self.stamp[start_i] = self.generation;
        self.cost[start_i] = 0;
        self.parent[start_i] = u32::MAX;
        self.heap.clear();
        self.heap
            .push(Reverse((self.heuristic(start, goal), 0, start_i as u32)));
        let neighbours = [
            (-1i32, 0i32, 10u32),
            (1, 0, 10),
            (0, -1, 10),
            (0, 1, 10),
            (-1, -1, 14),
            (-1, 1, 14),
            (1, -1, 14),
            (1, 1, 14),
        ];
        let mut expanded = 0u32;
        while let Some(Reverse((_, g, current))) = self.heap.pop() {
            let current_index = current as usize;
            if self.stamp[current_index] != self.generation || g > self.cost[current_index] {
                continue;
            }
            if current_index == goal_i {
                let mut path = vec![goal];
                let mut node = goal_i;
                while node != start_i {
                    let parent = self.parent[node];
                    if parent == u32::MAX {
                        break;
                    }
                    path.push((parent % self.width, parent / self.width));
                    node = parent as usize;
                }
                path.reverse();
                return Some(path);
            }
            expanded += 1;
            if expanded > max_nodes {
                return None;
            }
            let cx = (current_index % self.width as usize) as i32;
            let cy = (current_index / self.width as usize) as i32;
            for (dx, dy, step) in neighbours {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                if !grid.walkable(nx as u32, ny as u32) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    if !grid.walkable((cx + dx) as u32, cy as u32)
                        || !grid.walkable(cx as u32, (cy + dy) as u32)
                    {
                        continue;
                    }
                }
                let next = ny as usize * self.width as usize + nx as usize;
                let new_cost = g + step;
                let known = self.stamp[next] == self.generation;
                if known && new_cost >= self.cost[next] {
                    continue;
                }
                self.stamp[next] = self.generation;
                self.cost[next] = new_cost;
                self.parent[next] = current;
                let h = self.heuristic((nx as u32, ny as u32), goal);
                self.heap
                    .push(Reverse((new_cost + h, new_cost, next as u32)));
            }
        }
        None
    }
}

/// Deterministic nearest-walkable cell search (concentric rings), mirroring the
/// WC3 "nearest pathable point" behaviour when an order lands on a wall or
/// building footprint.
pub fn nearest_walkable(grid: &Grid, cell: (u32, u32), max_ring: u32) -> Option<(u32, u32)> {
    if grid.walkable(cell.0, cell.1) {
        return Some(cell);
    }
    for ring in 1..=max_ring {
        let x0 = cell.0 as i64 - ring as i64;
        let x1 = cell.0 as i64 + ring as i64;
        let y0 = cell.1 as i64 - ring as i64;
        let y1 = cell.1 as i64 + ring as i64;
        // Top and bottom rows.
        for x in x0..=x1 {
            for y in [y0, y1] {
                if x >= 0 && y >= 0 && x < grid.width as i64 && y < grid.height as i64 {
                    if grid.walkable(x as u32, y as u32) {
                        return Some((x as u32, y as u32));
                    }
                }
            }
        }
        // Left and right columns (corners already checked).
        for y in (y0 + 1)..y1 {
            for x in [x0, x1] {
                if x >= 0 && y >= 0 && x < grid.width as i64 && y < grid.height as i64 {
                    if grid.walkable(x as u32, y as u32) {
                        return Some((x as u32, y as u32));
                    }
                }
            }
        }
    }
    None
}

/// Follows a distance field downhill, producing a short cell path.
///
/// The field is a derived buffer and must hold exactly one value per grid cell:
/// a short field would be indexed past its end while descending, and a longer
/// one means the caller mixed two grids. Both are refused before any indexing
/// (`width * height` is checked, not wrapping). `None` is also the answer for a
/// start cell outside the grid or on a non-walkable cell.
pub fn trace_flow(
    grid: &Grid,
    field: &[u32],
    start: (u32, u32),
    max_steps: usize,
) -> Option<Vec<(u32, u32)>> {
    let cells = (grid.width as usize).checked_mul(grid.height as usize)?;
    if field.len() != cells {
        return None;
    }
    if !grid.walkable(start.0, start.1) {
        return None;
    }
    let index = |x: u32, y: u32| y as usize * grid.width as usize + x as usize;
    let mut start_field = field[index(start.0, start.1)];
    if start_field == u32::MAX {
        return None;
    }
    let mut path = vec![start];
    let mut cx = start.0 as i32;
    let mut cy = start.1 as i32;
    for _ in 0..max_steps {
        if start_field == 0 {
            return Some(path);
        }
        let mut best: Option<(i32, i32, u32)> = None;
        for (dx, dy) in [
            (-1i32, 0i32),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= grid.width as i32 || ny >= grid.height as i32 {
                continue;
            }
            if !grid.walkable(nx as u32, ny as u32) {
                continue;
            }
            if dx != 0 && dy != 0 {
                if !grid.walkable((cx + dx) as u32, cy as u32)
                    || !grid.walkable(cx as u32, (cy + dy) as u32)
                {
                    continue;
                }
            }
            let value = field[index(nx as u32, ny as u32)];
            if value == u32::MAX || value >= start_field {
                continue;
            }
            // Prefer straight steps on ties: deterministic, cheap.
            let better = match best {
                None => true,
                Some((bx, by, bv)) => {
                    value < bv || (value == bv && (dx.abs() + dy.abs()) < (bx.abs() + by.abs()))
                }
            };
            if better {
                best = Some((dx, dy, value));
            }
        }
        let Some((dx, dy, value)) = best else {
            return Some(path);
        };
        cx += dx;
        cy += dy;
        start_field = value;
        path.push((cx as u32, cy as u32));
    }
    Some(path)
}

/// Formation slots in a box centred on `center`, oriented along `facing`.
pub fn formation_slots(center: Vec2, facing: Vec2, count: usize, spacing: f32) -> Vec<Vec2> {
    if count == 0 {
        return Vec::new();
    }
    let dir = facing.normalize_or_zero();
    let dir = if dir == Vec2::ZERO {
        Vec2::new(1.0, 0.0)
    } else {
        dir
    };
    let side = Vec2::new(-dir.y, dir.x);
    let cols = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(cols);
    let spacing = spacing.max(24.0);
    let mut slots = Vec::with_capacity(count);
    for index in 0..count {
        let col = index % cols;
        let row = index / cols;
        let offset_side = (col as f32 - (cols as f32 - 1.0) * 0.5) * spacing;
        let offset_depth = (row as f32 - (rows as f32 - 1.0) * 0.5) * spacing;
        slots.push(center + side * offset_side + dir * offset_depth);
    }
    slots
}

/// True when `from` has a clear straight line to `to` in cell space.
pub fn clear_line(grid: &Grid, from: Vec2, to: Vec2) -> bool {
    let a = grid.world_to_cell(from);
    let b = grid.world_to_cell(to);
    grid.line_of_sight(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn astar_routes_around_a_wall() {
        let mut grid = Grid::new(16, 16, 128.0);
        for y in 0..14 {
            grid.set_walkable(8, y, false);
        }
        let mut finder = PathFinder::new(16, 16);
        let path = finder
            .astar(&grid, (0, 0), (15, 0), MAX_ASTAR_NODES)
            .expect("path");
        assert_eq!(path.first(), Some(&(0, 0)));
        assert_eq!(path.last(), Some(&(15, 0)));
        assert!(path.iter().all(|&(x, y)| grid.walkable(x, y)));
        assert!(path.iter().any(|&(_, y)| y >= 14));
    }

    #[test]
    fn astar_respects_the_node_budget() {
        let mut grid = Grid::new(128, 128, 128.0);
        for y in 0..120 {
            grid.set_walkable(64, y, false);
        }
        let mut finder = PathFinder::new(128, 128);
        assert!(finder.astar(&grid, (0, 0), (127, 127), 32).is_none());
        assert!(
            finder
                .astar(&grid, (0, 0), (127, 127), MAX_ASTAR_NODES)
                .is_some()
        );
    }

    #[test]
    fn nearest_walkable_finds_the_closest_ring() {
        let mut grid = Grid::new(8, 8, 128.0);
        grid.set_walkable(4, 4, false);
        let cell = nearest_walkable(&grid, (4, 4), 3).expect("neighbour");
        assert!(grid.walkable(cell.0, cell.1));
        assert!(cell.0.abs_diff(4) <= 1 && cell.1.abs_diff(4) <= 1);
    }

    #[test]
    fn flow_trace_reaches_the_goal() {
        let grid = Grid::new(12, 12, 128.0);
        let field = grid.flow_field((11, 11));
        let path = trace_flow(&grid, &field, (0, 0), 64).expect("flow");
        assert_eq!(path.last(), Some(&(11, 11)));
    }

    #[test]
    fn formation_is_centred_and_spaced() {
        let slots = formation_slots(Vec2::new(100.0, 100.0), Vec2::new(1.0, 0.0), 9, 64.0);
        assert_eq!(slots.len(), 9);
        let center = slots.iter().fold(Vec2::ZERO, |a, b| a + *b) / slots.len() as f32;
        assert!(center.distance(Vec2::new(100.0, 100.0)) < 1.0);
        for (i, a) in slots.iter().enumerate() {
            for b in &slots[i + 1..] {
                assert!(a.distance(*b) >= 60.0);
            }
        }
    }
}
