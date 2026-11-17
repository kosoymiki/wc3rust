//! Deterministic grid pathfinding: A*, flow fields, line of sight.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::Vec2;

#[derive(Debug, Clone)]
pub struct Grid {
    pub width: u32,
    pub height: u32,
    pub cell_size: f32,
    walkable: Vec<bool>,
}

impl Grid {
    pub fn new(width: u32, height: u32, cell_size: f32) -> Self {
        Self {
            width,
            height,
            cell_size,
            walkable: vec![true; width as usize * height as usize],
        }
    }

    pub fn from_walkable(width: u32, height: u32, cell_size: f32, walkable: Vec<bool>) -> Self {
        assert_eq!(walkable.len(), width as usize * height as usize);
        Self {
            width,
            height,
            cell_size,
            walkable,
        }
    }

    pub fn index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width && y < self.height).then_some(y as usize * self.width as usize + x as usize)
    }

    pub fn walkable(&self, x: u32, y: u32) -> bool {
        self.index(x, y).map(|i| self.walkable[i]).unwrap_or(false)
    }

    pub fn set_walkable(&mut self, x: u32, y: u32, walkable: bool) {
        if let Some(i) = self.index(x, y) {
            self.walkable[i] = walkable;
        }
    }

    pub fn world_to_cell(&self, world: Vec2) -> (u32, u32) {
        (
            (world.x / self.cell_size).floor().max(0.0) as u32,
            (world.y / self.cell_size).floor().max(0.0) as u32,
        )
    }

    pub fn cell_to_world(&self, x: u32, y: u32) -> Vec2 {
        Vec2::new(
            (x as f32 + 0.5) * self.cell_size,
            (y as f32 + 0.5) * self.cell_size,
        )
    }

    /// A* with 8-neighbour movement; diagonal steps require both orthogonal
    /// neighbours to be walkable (no corner cutting).
    pub fn astar(&self, start: (u32, u32), goal: (u32, u32)) -> Option<Vec<(u32, u32)>> {
        if !self.walkable(start.0, start.1) || !self.walkable(goal.0, goal.1) {
            return None;
        }
        if start == goal {
            return Some(vec![start]);
        }
        let size = self.width as usize * self.height as usize;
        let mut cost = vec![u32::MAX; size];
        let mut parent = vec![u32::MAX; size];
        let start_i = self.index(start.0, start.1)?;
        let goal_i = self.index(goal.0, goal.1)?;
        cost[start_i] = 0;
        let mut open: BinaryHeap<Reverse<(u32, u32, usize)>> = BinaryHeap::new();
        open.push(Reverse((self.heuristic(start, goal), 0, start_i)));
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
        while let Some(Reverse((_, g, current))) = open.pop() {
            if current == goal_i {
                let mut path = vec![(goal.0, goal.1)];
                let mut node = goal_i;
                while node != start_i {
                    let p = parent[node];
                    if p == u32::MAX {
                        break;
                    }
                    path.push((p as u32 % self.width, p as u32 / self.width));
                    node = p as usize;
                }
                path.reverse();
                return Some(path);
            }
            if g > cost[current] {
                continue;
            }
            let cx = (current % self.width as usize) as i32;
            let cy = (current / self.width as usize) as i32;
            for (dx, dy, step) in neighbours {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                if !self.walkable(nx as u32, ny as u32) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    if !self.walkable((cx + dx) as u32, cy as u32)
                        || !self.walkable(cx as u32, (cy + dy) as u32)
                    {
                        continue;
                    }
                }
                let next = ny as usize * self.width as usize + nx as usize;
                let new_cost = g + step;
                if new_cost < cost[next] {
                    cost[next] = new_cost;
                    parent[next] = current as u32;
                    let h = self.heuristic((nx as u32, ny as u32), goal);
                    open.push(Reverse((new_cost + h, new_cost, next)));
                }
            }
        }
        None
    }

    fn heuristic(&self, a: (u32, u32), b: (u32, u32)) -> u32 {
        let dx = a.0.abs_diff(b.0);
        let dy = a.1.abs_diff(b.1);
        10 * dx.max(dy) + 4 * dx.min(dy)
    }

    /// Distance field (Dijkstra) used by group movement.
    pub fn flow_field(&self, goal: (u32, u32)) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.width as usize * self.height as usize];
        if !self.walkable(goal.0, goal.1) {
            return dist;
        }
        let goal_i = goal.1 as usize * self.width as usize + goal.0 as usize;
        dist[goal_i] = 0;
        let mut heap = BinaryHeap::new();
        heap.push(Reverse((0u32, goal_i)));
        while let Some(Reverse((d, current))) = heap.pop() {
            if d > dist[current] {
                continue;
            }
            let cx = (current % self.width as usize) as i32;
            let cy = (current / self.width as usize) as i32;
            for (dx, dy, step) in [(-1, 0, 10), (1, 0, 10), (0, -1, 10), (0, 1, 10)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                if !self.walkable(nx as u32, ny as u32) {
                    continue;
                }
                let next = ny as usize * self.width as usize + nx as usize;
                if d + step < dist[next] {
                    dist[next] = d + step;
                    heap.push(Reverse((d + step, next)));
                }
            }
        }
        dist
    }

    pub fn line_of_sight(&self, a: (u32, u32), b: (u32, u32)) -> bool {
        let (mut x0, mut y0) = (a.0 as i32, a.1 as i32);
        let (x1, y1) = (b.0 as i32, b.1 as i32);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if !self.walkable(x0 as u32, y0 as u32) {
                return false;
            }
            if x0 == x1 && y0 == y1 {
                return true;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_path_around_wall() {
        let mut grid = Grid::new(8, 8, 1.0);
        for y in 0..7 {
            grid.set_walkable(4, y, false);
        }
        let path = grid.astar((0, 0), (7, 0)).expect("path");
        assert_eq!(path.first(), Some(&(0, 0)));
        assert_eq!(path.last(), Some(&(7, 0)));
        assert!(path.iter().all(|&(x, y)| grid.walkable(x, y)));
        assert!(path.iter().any(|&(_, y)| y == 7));
    }

    #[test]
    fn rejects_blocked_goal() {
        let mut grid = Grid::new(4, 4, 1.0);
        grid.set_walkable(1, 1, false);
        assert!(grid.astar((0, 0), (1, 1)).is_none());
    }

    #[test]
    fn flow_field_is_monotonic_toward_goal() {
        let grid = Grid::new(6, 6, 1.0);
        let field = grid.flow_field((5, 5));
        for y in 0..6u32 {
            for x in 0..6u32 {
                let d = field[y as usize * 6 + x as usize];
                assert!(d < u32::MAX);
                if x < 5 {
                    assert!(field[y as usize * 6 + x as usize + 1] <= d);
                }
            }
        }
    }

    #[test]
    fn world_cell_roundtrip() {
        let grid = Grid::new(16, 16, 128.0);
        let cell = grid.world_to_cell(Vec2::new(300.0, 700.0));
        assert_eq!(cell, (2, 5));
        let world = grid.cell_to_world(cell.0, cell.1);
        assert_eq!(grid.world_to_cell(world), cell);
    }
}
