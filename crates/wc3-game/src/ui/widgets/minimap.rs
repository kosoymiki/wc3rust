//! Minimap model and the world ↔ minimap transform (hud.md §3.7).
//!
//! The map texture is square; the world is usually not. The transform therefore
//! letterboxes the world inside the square, keeps the aspect ratio, and clips
//! every marker against the same rect the painter uses. Clicks convert back to
//! world coordinates through the inverse of that single affine map — there is no
//! second mapping anywhere.

use crate::ui::core::geometry::{Point, Rect};

/// The playable world rectangle in world units.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorldRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl WorldRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        WorldRect { x, y, w, h }
    }

    /// The retail map size (2048 × 1536 world units is the classic 4:3).
    pub const fn classic() -> Self {
        WorldRect::new(0.0, 0.0, 2048.0, 1536.0)
    }

    pub fn centre(self) -> Point {
        Point::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.w
            && point.y >= self.y
            && point.y <= self.y + self.h
    }

    pub fn aspect(self) -> f32 {
        if self.h > 0.0 {
            self.w / self.h
        } else {
            1.0
        }
    }
}

/// The affine world → minimap mapping.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MinimapTransform {
    /// The square the map content is drawn into.
    pub image: Rect,
    /// The world rectangle shown inside it.
    pub world: WorldRect,
    /// The letterboxed sub-rect actually covered by the world (aspect kept).
    pub content: Rect,
    /// World units per minimap dp.
    pub scale: f32,
}

impl MinimapTransform {
    /// Fit `world` into the square `image`, preserving the aspect ratio.
    pub fn letterbox(image: Rect, world: WorldRect) -> Self {
        let image = Rect::new(
            image.x,
            image.y,
            image.w.max(0.0),
            image.h.max(0.0),
        );
        let world_w = world.w.max(0.001);
        let world_h = world.h.max(0.001);
        let side = image.w.min(image.h);
        let world_aspect = world_w / world_h;
        let (content_w, content_h) = if world_aspect >= 1.0 {
            (side, side / world_aspect)
        } else {
            (side * world_aspect, side)
        };
        let content = Rect::new(
            image.x + (image.w - content_w) * 0.5,
            image.y + (image.h - content_h) * 0.5,
            content_w,
            content_h,
        );
        let scale = if content_w.min(content_h) > 0.0 {
            (world_w / content_w).max(world_h / content_h)
        } else {
            1.0
        };
        MinimapTransform {
            image,
            world,
            content,
            scale,
        }
    }

    /// World point → normalised map point in `0..=1` (clamped).
    pub fn world_to_unit(&self, world: Point) -> Point {
        Point::new(
            ((world.x - self.world.x) / self.world.w.max(0.001)).clamp(0.0, 1.0),
            ((world.y - self.world.y) / self.world.h.max(0.001)).clamp(0.0, 1.0),
        )
    }

    /// Normalised `0..=1` → minimap dp point.
    pub fn unit_to_map(&self, unit: Point) -> Point {
        Point::new(
            self.content.x + unit.x * self.content.w,
            self.content.y + unit.y * self.content.h,
        )
    }

    /// World → minimap dp.
    pub fn world_to_map(&self, world: Point) -> Point {
        self.unit_to_map(self.world_to_unit(world))
    }

    /// Minimap dp → world (inverse of [`Self::world_to_map`]).
    pub fn map_to_world(&self, map: Point) -> Point {
        let unit = self.map_to_unit(map);
        Point::new(
            self.world.x + unit.x * self.world.w,
            self.world.y + unit.y * self.world.h,
        )
    }

    /// Minimap dp → normalised `0..=1`, clamped to the content rect.
    pub fn map_to_unit(&self, map: Point) -> Point {
        Point::new(
            ((map.x - self.content.x) / self.content.w.max(0.001)).clamp(0.0, 1.0),
            ((map.y - self.content.y) / self.content.h.max(0.001)).clamp(0.0, 1.0),
        )
    }

    /// Is a minimap dp point inside the map content?
    pub fn contains(&self, map: Point) -> bool {
        self.content.contains(map)
    }

    /// A world-space camera rectangle as a minimap dp rect, clipped inside the
    /// content so a camera looking off the map cannot paint outside the frame.
    pub fn camera_rect(&self, camera: WorldRect) -> Rect {
        let top_left = self.world_to_map(Point::new(camera.x, camera.y));
        let bottom_right = self.world_to_map(Point::new(
            camera.x + camera.w,
            camera.y + camera.h,
        ));
        let rect = Rect::new(
            top_left.x,
            top_left.y,
            (bottom_right.x - top_left.x).max(0.0),
            (bottom_right.y - top_left.y).max(0.0),
        );
        rect.intersection(self.content)
    }

    /// One fog tile as a minimap dp rect (`grid` cells across the world).
    pub fn tile_rect(&self, column: u32, row: u32, grid: (u32, u32)) -> Rect {
        let (cols, rows) = (grid.0.max(1), grid.1.max(1));
        let cell_w = self.content.w / cols as f32;
        let cell_h = self.content.h / rows as f32;
        Rect::new(
            self.content.x + column as f32 * cell_w,
            self.content.y + row as f32 * cell_h,
            cell_w,
            cell_h,
        )
    }

    /// Tap/drag delta in minimap dp → world delta.
    pub fn drag_to_world(&self, delta: Point) -> Point {
        Point::new(delta.x * self.scale, delta.y * self.scale)
    }

    /// Zoom factor from a pinch (`distance_now / distance_start`).
    pub fn pinch_zoom(start: f32, now: f32) -> f32 {
        if !start.is_finite() || start <= 1.0 || !now.is_finite() {
            return 1.0;
        }
        (start / now).clamp(0.5, 2.0)
    }
}

/// What a minimap gesture asks the game to do.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MinimapAction {
    /// Move the camera centre to this world point (tap).
    MoveCamera(Point),
    /// Pan the camera by a world delta (drag).
    PanCamera(Point),
    /// Zoom by a factor (pinch).
    Zoom(f32),
    /// Nothing (the gesture started outside the map content).
    Ignore,
}

/// Resolve one gesture against the transform.
pub fn action_for(transform: &MinimapTransform, gesture: Gesture) -> MinimapAction {
    match gesture {
        Gesture::Tap(point) => {
            if transform.contains(point) {
                MinimapAction::MoveCamera(transform.map_to_world(point))
            } else {
                MinimapAction::Ignore
            }
        }
        Gesture::Drag { from, to } => {
            if !transform.contains(from) && !transform.contains(to) {
                return MinimapAction::Ignore;
            }
            MinimapAction::PanCamera(transform.drag_to_world(Point::new(
                to.x - from.x,
                to.y - from.y,
            )))
        }
        Gesture::Pinch { start, now } => MinimapAction::Zoom(MinimapTransform::pinch_zoom(start, now)),
    }
}

/// A pointer gesture on the minimap.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Tap(Point),
    Drag { from: Point, to: Point },
    Pinch { start: f32, now: f32 },
}

/// Fog state of one tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fog {
    #[default]
    Hidden,
    Explored,
    Visible,
}

impl Fog {
    pub fn from_cell(value: u8) -> Self {
        match value {
            0 => Fog::Hidden,
            1 => Fog::Explored,
            _ => Fog::Visible,
        }
    }

    /// Alpha for the fog veil painted over one tile.
    pub fn veil_alpha(self) -> f32 {
        match self {
            Fog::Hidden => 0.92,
            Fog::Explored => 0.45,
            Fog::Visible => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transform() -> MinimapTransform {
        MinimapTransform::letterbox(
            Rect::new(100.0, 50.0, 144.0, 144.0),
            WorldRect::classic(),
        )
    }

    #[test]
    fn a_square_image_letterboxes_a_4_3_world() {
        let t = transform();
        assert!((t.content.w - 144.0).abs() < 0.01);
        assert!((t.content.h - 108.0).abs() < 0.01);
        assert!((t.content.y - (50.0 + 18.0)).abs() < 0.01, "vertically centred");
        assert!(t.content.is_inside(t.image));
    }

    #[test]
    fn world_and_map_round_trip() {
        let t = transform();
        let world = Point::new(512.0, 1152.0);
        let map = t.world_to_map(world);
        assert!(t.contains(map), "{map:?} outside {:?}", t.content);
        let back = t.map_to_world(map);
        assert!((back.x - world.x).abs() < 0.5, "{back:?}");
        assert!((back.y - world.y).abs() < 0.5, "{back:?}");
    }

    #[test]
    fn the_map_corners_map_to_the_content_corners() {
        let t = transform();
        let tl = t.world_to_map(Point::new(0.0, 0.0));
        let br = t.world_to_map(Point::new(2048.0, 1536.0));
        assert!((tl.x - t.content.x).abs() < 0.01);
        assert!((tl.y - t.content.y).abs() < 0.01);
        assert!((br.x - t.content.right()).abs() < 0.01);
        assert!((br.y - t.content.bottom()).abs() < 0.01);
    }

    #[test]
    fn a_camera_rect_is_clipped_inside_the_map() {
        let t = transform();
        let camera = t.camera_rect(WorldRect::new(-500.0, -500.0, 2000.0, 2000.0));
        assert!(camera.is_inside(t.content));
        assert!(camera.w > 0.0 && camera.h > 0.0);
    }

    #[test]
    fn a_tap_outside_the_content_is_ignored() {
        let t = transform();
        assert_eq!(
            action_for(&t, Gesture::Tap(Point::new(101.0, 51.0))),
            MinimapAction::Ignore
        );
        let inside = t.content.centre();
        assert!(matches!(
            action_for(&t, Gesture::Tap(inside)),
            MinimapAction::MoveCamera(_)
        ));
    }

    #[test]
    fn a_drag_reports_a_world_delta() {
        let t = transform();
        let from = t.content.centre();
        let to = Point::new(from.x + 10.0, from.y);
        let MinimapAction::PanCamera(delta) = action_for(&t, Gesture::Drag { from, to }) else {
            panic!("expected a pan");
        };
        assert!(delta.x > 0.0);
        assert!(delta.y.abs() < 0.01);
    }

    #[test]
    fn fog_tiles_tile_the_content() {
        let t = transform();
        let tile = t.tile_rect(0, 0, (32, 32));
        assert!((tile.w - t.content.w / 32.0).abs() < 0.01);
        let last = t.tile_rect(31, 31, (32, 32));
        assert!((last.right() - t.content.right()).abs() < 0.01);
        assert_eq!(Fog::from_cell(2), Fog::Visible);
        assert!(Fog::Hidden.veil_alpha() > Fog::Explored.veil_alpha());
    }
}
