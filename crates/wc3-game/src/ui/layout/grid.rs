//! Grid planning for the command card, the group tray and the inventory.
//!
//! The card is a `columns × rows` field of square-ish targets inside a panel.
//! The planner is the only place that decides the cell size, and it returns an
//! explicit [`GridPlan::meets_floor`] instead of silently shrinking cells below
//! the touch minimum — that shrink was how 12 command slots ended up at 36 dp
//! on a narrow band.

use crate::ui::core::geometry::{Point, Rect, Size};

/// A solved grid field.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridPlan {
    /// The field the grid was fitted into.
    pub area: Rect,
    /// Cell size (equal for every cell).
    pub cell: Size,
    /// Cell-to-cell pitch (cell + gap).
    pub pitch: Size,
    /// Top-left corner of cell 0.
    pub origin: Point,
    pub columns: usize,
    pub rows: usize,
    pub gap: f32,
    /// Every cell is at least `min_cell` on both axes.
    pub meets_floor: bool,
    /// Space left over on each axis (used for centring).
    pub slack: Size,
}

impl GridPlan {
    pub fn count(&self) -> usize {
        self.columns * self.rows
    }

    /// Rect of cell `index`, row-major.
    pub fn cell_rect(&self, index: usize) -> Rect {
        let column = index % self.columns.max(1);
        let row = index / self.columns.max(1);
        Rect::new(
            self.origin.x + column as f32 * self.pitch.w,
            self.origin.y + row as f32 * self.pitch.h,
            self.cell.w,
            self.cell.h,
        )
    }

    /// Every cell, row-major.
    pub fn cells(&self) -> Vec<Rect> {
        (0..self.count()).map(|index| self.cell_rect(index)).collect()
    }

    /// Cell under a point, if any.
    pub fn cell_at(&self, point: Point) -> Option<usize> {
        for index in 0..self.count() {
            if self.cell_rect(index).contains(point) {
                return Some(index);
            }
        }
        None
    }

    /// Smallest cell side in dp.
    pub fn min_side(&self) -> f32 {
        self.cell.w.min(self.cell.h)
    }
}

/// Fit `columns × rows` cells into `area` with `gap` dp between them.
///
/// Cells stay square when `square` is set (the command card is 4x3 of 48 dp
/// targets). The result is centred and always inside `area`.
pub fn plan(
    area: Rect,
    columns: usize,
    rows: usize,
    gap: f32,
    min_cell: f32,
    square: bool,
) -> GridPlan {
    let columns = columns.max(1);
    let rows = rows.max(1);
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let area = Rect::new(
        area.x,
        area.y,
        area.w.max(0.0),
        area.h.max(0.0),
    );
    let avail_w = (area.w - gap * (columns - 1) as f32).max(0.0);
    let avail_h = (area.h - gap * (rows - 1) as f32).max(0.0);
    let mut cell_w = avail_w / columns as f32;
    let mut cell_h = avail_h / rows as f32;
    if square {
        let side = cell_w.min(cell_h);
        cell_w = side;
        cell_h = side;
    }
    let grid_w = cell_w * columns as f32 + gap * (columns - 1) as f32;
    let grid_h = cell_h * rows as f32 + gap * (rows - 1) as f32;
    let slack = Size::new((area.w - grid_w).max(0.0), (area.h - grid_h).max(0.0));
    let origin = Point::new(
        area.x + slack.w * 0.5,
        area.y + slack.h * 0.5,
    );
    let min_cell = if min_cell.is_finite() { min_cell.max(0.0) } else { 0.0 };
    GridPlan {
        area,
        cell: Size::new(cell_w, cell_h),
        pitch: Size::new(cell_w + gap, cell_h + gap),
        origin,
        columns,
        rows,
        gap,
        meets_floor: cell_w + 0.01 >= min_cell && cell_h + 0.01 >= min_cell,
        slack,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_cells_tile_without_overlap() {
        // 232 x 176 dp panel: 4 x 3 of 48 dp with 8 dp gaps (trim included).
        let plan = plan(Rect::new(0.0, 0.0, 216.0, 160.0), 4, 3, 8.0, 48.0, true);
        assert!(plan.meets_floor, "48 dp cells");
        let cells = plan.cells();
        assert_eq!(cells.len(), 12);
        for i in 0..cells.len() {
            for j in (i + 1)..cells.len() {
                assert!(
                    cells[i].intersection_area(cells[j]) < 0.01,
                    "{i} and {j} overlap"
                );
            }
            assert!(cells[i].is_inside(plan.area));
        }
        assert!((plan.cell.w - 48.0).abs() < 0.01);
        assert!((plan.cell.h - 48.0).abs() < 0.01);
    }

    #[test]
    fn a_narrow_panel_reports_the_floor_violation() {
        // The old 0.75 * 48 fallback: 4 columns in 168 dp.
        let plan = plan(Rect::new(0.0, 0.0, 168.0, 120.0), 4, 3, 8.0, 48.0, true);
        assert!(!plan.meets_floor);
        assert!(plan.min_side() < 48.0);
        // Cells still tile: the caller sees `meets_floor == false` and must
        // choose a bigger plan or a drawer, never draw a 30 dp target.
        assert_eq!(plan.cells().len(), 12);
    }

    #[test]
    fn non_square_grids_use_the_full_field() {
        let plan = plan(Rect::new(10.0, 20.0, 300.0, 100.0), 3, 2, 10.0, 40.0, false);
        assert!(plan.meets_floor);
        assert!(plan.cell.w > plan.cell.h);
        assert!(plan.origin.x >= 10.0 && plan.origin.y >= 20.0);
    }

    #[test]
    fn hit_testing_matches_the_cells() {
        let plan = plan(Rect::new(0.0, 0.0, 216.0, 160.0), 4, 3, 8.0, 48.0, true);
        for index in 0..12 {
            let cell = plan.cell_rect(index);
            assert_eq!(plan.cell_at(cell.centre()), Some(index));
        }
        assert_eq!(plan.cell_at(Point::new(-5.0, -5.0)), None);
    }
}
