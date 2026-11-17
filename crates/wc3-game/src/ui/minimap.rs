//! Minimap model (mechanic `ui.minimap` + `fog.visibility`).
//!
//! Blips come from the canonical `Sim` units: the player's own units are always
//! shown, other owners and neutral creeps only while visible. The fog mask is a
//! sampled view of the sim's per-player visibility/explored grid, and
//! click-to-move maps a minimap pixel to a world camera centre with clamping.

use glam::Vec2;
use wc3_sim::Sim;

use crate::hud::Rect;

/// Your own units (green in WC3).
pub const BLIP_PLAYER: [f32; 4] = [0.30, 0.95, 0.35, 1.0];
/// Allied units (blue); the 1v1 sim has no allies yet.
pub const BLIP_ALLY: [f32; 4] = [0.35, 0.65, 1.0, 1.0];
/// Enemy units (red).
pub const BLIP_ENEMY: [f32; 4] = [1.0, 0.32, 0.30, 1.0];
/// Neutral creeps (yellow).
pub const BLIP_NEUTRAL: [f32; 4] = [0.95, 0.82, 0.30, 1.0];
/// Last-known enemy contact: explored but not visible right now (faded red).
pub const BLIP_LAST_SEEN: [f32; 4] = [1.0, 0.32, 0.30, 0.35];
pub const BLIP_BUILDING_SIZE: f32 = 4.0;
pub const BLIP_UNIT_SIZE: f32 = 2.5;

/// WC3 minimap colour for a unit owner relative to the local player.
pub fn owner_color(owner: u8, player: u8) -> [f32; 4] {
    if owner == player {
        BLIP_PLAYER
    } else if owner >= 2 {
        BLIP_NEUTRAL
    } else {
        // A second player slot that is not the local player: enemy by default.
        // Ally colour is exposed for future team games.
        BLIP_ENEMY
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blip {
    /// Normalised map position (0..1 on both axes).
    pub pos: Vec2,
    pub color: [f32; 4],
    pub neutral: bool,
    pub size: f32,
    /// `true` for a last-known contact in explored fog (faded, non-live).
    pub last_seen: bool,
}

/// Units visible on the minimap: own units always, others only under fog.
pub fn blips(sim: &Sim, player: u8, world: Vec2) -> Vec<Blip> {
    let world = world.max(Vec2::splat(1.0));
    let mut out = Vec::new();
    for unit in sim.alive_units() {
        let own = unit.owner == player;
        let neutral = unit.owner >= 2;
        if !own && !sim.is_visible(player, unit.pos) {
            continue;
        }
        out.push(Blip {
            pos: Vec2::new(
                (unit.pos.x / world.x).clamp(0.0, 1.0),
                (unit.pos.y / world.y).clamp(0.0, 1.0),
            ),
            color: if neutral {
                BLIP_NEUTRAL
            } else {
                owner_color(unit.owner, player)
            },
            neutral,
            size: if unit.is_building() {
                BLIP_BUILDING_SIZE
            } else {
                BLIP_UNIT_SIZE
            },
            last_seen: false,
        });
    }
    // Explored-but-not-visible enemy contacts: the sim remembers the last
    // observed position, WC3 keeps a faded blip there until it is re-scouted.
    for (id, seen) in sim.last_seen_all(player) {
        let Some(unit) = sim.unit(id) else { continue };
        if unit.owner == player {
            continue;
        }
        let cell = sim.grid.world_to_cell(seen.pos);
        if sim.visibility(player, cell) != wc3_sim::Visibility::Explored {
            continue;
        }
        out.push(Blip {
            pos: Vec2::new(
                (seen.pos.x / world.x).clamp(0.0, 1.0),
                (seen.pos.y / world.y).clamp(0.0, 1.0),
            ),
            color: BLIP_LAST_SEEN,
            neutral: unit.owner >= 2,
            size: BLIP_UNIT_SIZE,
            last_seen: true,
        });
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fog {
    pub width: u32,
    pub height: u32,
    pub cell_size: f32,
    pub visible: Vec<bool>,
    pub explored: Vec<bool>,
}

impl Fog {
    pub fn visible_at(&self, x: u32, y: u32) -> bool {
        self.index(x, y)
            .map(|index| self.visible[index])
            .unwrap_or(false)
    }

    pub fn explored_at(&self, x: u32, y: u32) -> bool {
        self.index(x, y)
            .map(|index| self.explored[index])
            .unwrap_or(false)
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        (x < self.width && y < self.height).then_some(y as usize * self.width as usize + x as usize)
    }

    pub fn visible_fraction(&self) -> f32 {
        if self.visible.is_empty() {
            return 0.0;
        }
        let seen = self.visible.iter().filter(|value| **value).count();
        seen as f32 / self.visible.len() as f32
    }
}

/// Sample the sim's per-player fog grid at every cell centre.
pub fn fog(sim: &Sim, player: u8) -> Fog {
    let width = sim.grid.width;
    let height = sim.grid.height;
    let cell_size = sim.grid.cell_size;
    let mut visible = Vec::with_capacity((width * height) as usize);
    let mut explored = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let pos = Vec2::new(
                (x as f32 + 0.5) * cell_size,
                (y as f32 + 0.5) * cell_size,
            );
            visible.push(sim.is_visible(player, pos));
            explored.push(sim.is_explored(player, pos));
        }
    }
    Fog {
        width,
        height,
        cell_size,
        visible,
        explored,
    }
}

/// Minimap click -> world camera centre. The minimap rect is the visible frame
/// (the hit rect); the world point clamps to the map bounds.
pub fn camera_from_click(rect: Rect, click: Vec2, world: Vec2) -> Vec2 {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return world * 0.5;
    }
    let nx = ((click.x - rect.x) / rect.w).clamp(0.0, 1.0);
    let ny = ((click.y - rect.y) / rect.h).clamp(0.0, 1.0);
    Vec2::new(nx * world.x, ny * world.y)
}

/// Normalised camera box for the minimap overlay (`[x, y, w, h]` in 0..1).
pub fn camera_rect(centre: Vec2, half_extents: Vec2, world: Vec2) -> [f32; 4] {
    let world = world.max(Vec2::splat(1.0));
    let x = ((centre.x - half_extents.x) / world.x).clamp(0.0, 1.0);
    let y = ((centre.y - half_extents.y) / world.y).clamp(0.0, 1.0);
    let w = ((half_extents.x * 2.0) / world.x).clamp(0.0, 1.0);
    let h = ((half_extents.y * 2.0) / world.y).clamp(0.0, 1.0);
    [x, y, w.min(1.0 - x), h.min(1.0 - y)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use wc3_sim::{OrderSpec, Vec2};

    fn sim_with_units() -> Sim {
        let mut sim = Sim::new(0xB11F, 64, 64);
        sim.passive_income = false;
        sim
    }

    #[test]
    fn ui_minimap_owner_colors_are_distinct() {
        let player = owner_color(0, 0);
        let enemy = owner_color(1, 0);
        let neutral = owner_color(2, 0);
        assert_eq!(player, BLIP_PLAYER);
        assert_eq!(enemy, BLIP_ENEMY);
        assert_eq!(neutral, BLIP_NEUTRAL);
        assert_ne!(player, enemy);
        assert_ne!(enemy, neutral);
        assert_eq!(owner_color(0, 0), BLIP_PLAYER, "own colour follows the player");
    }

    #[test]
    fn fog_visibility_mask_matches_sim_grid() {
        let mut sim = sim_with_units();
        sim.spawn_def(0, "hfoo", Vec2::new(600.0, 600.0)).expect("unit");
        sim.step(&[]);
        let mask = fog(&sim, 0);
        assert_eq!(mask.width, sim.grid.width);
        assert_eq!(mask.height, sim.grid.height);
        assert_eq!(mask.visible.len(), (mask.width * mask.height) as usize);
        assert!(mask.visible_fraction() > 0.0, "sight reveals cells");
        for (x, y) in [(0u32, 0u32), (1, 1), (4, 4), (10, 10), (40, 40), (63, 63)] {
            let pos = Vec2::new(
                (x as f32 + 0.5) * mask.cell_size,
                (y as f32 + 0.5) * mask.cell_size,
            );
            assert_eq!(
                mask.visible_at(x, y),
                sim.is_visible(0, pos),
                "fog cell {x},{y} agrees with sim"
            );
            if mask.visible_at(x, y) {
                assert!(mask.explored_at(x, y), "visible implies explored");
            }
        }
    }

    #[test]
    fn ui_minimap_blips_owner_colors_fog_and_creeps() {
        let mut sim = sim_with_units();
        let world = Vec2::new(64.0 * 128.0, 64.0 * 128.0);
        sim.spawn_def(0, "hfoo", Vec2::new(600.0, 600.0)).expect("own");
        sim.spawn_def(1, "hfoo", Vec2::new(700.0, 600.0)).expect("near enemy");
        sim.spawn_def(1, "hfoo", Vec2::new(7000.0, 7000.0))
            .expect("far enemy");
        sim.spawn_def(2, "nban", Vec2::new(1000.0, 1000.0)).expect("creep");
        sim.step(&[]);
        let blips = blips(&sim, 0, world);
        assert!(blips.iter().any(|blip| blip.color == BLIP_PLAYER), "own unit blip");
        assert!(
            blips.iter().any(|blip| blip.color == BLIP_ENEMY && !blip.neutral),
            "visible enemy blip"
        );
        assert!(
            blips
                .iter()
                .any(|blip| blip.color == BLIP_NEUTRAL && blip.neutral),
            "creep neutral blip"
        );
        let far = Vec2::new(7000.0 / world.x, 7000.0 / world.y);
        assert!(
            !blips.iter().any(|blip| blip.pos.distance(far) < 0.01),
            "enemy outside fog is hidden"
        );
        for blip in &blips {
            assert!((0.0..=1.0).contains(&blip.pos.x));
            assert!((0.0..=1.0).contains(&blip.pos.y));
            assert!(blip.size > 0.0);
        }
    }

    #[test]
    fn ui_minimap_click_to_move_camera_maps_pixels_to_world() {
        let rect = Rect::new(100.0, 700.0, 200.0, 200.0);
        let world = Vec2::new(8192.0, 8192.0);
        let centre = camera_from_click(rect, rect.centre(), world);
        assert!((centre - world * 0.5).length() < 0.01, "centre maps to world centre");
        assert_eq!(camera_from_click(rect, Vec2::new(rect.x, rect.y), world), Vec2::ZERO);
        assert_eq!(
            camera_from_click(rect, Vec2::new(rect.x + rect.w, rect.y + rect.h), world),
            Vec2::new(8192.0, 8192.0)
        );
        let clamped = camera_from_click(rect, Vec2::new(-500.0, 5000.0), world);
        assert_eq!(clamped.x, 0.0, "outside the minimap clamps to the edge");
        assert!((clamped.y - 8192.0).abs() < 0.01);
    }

    #[test]
    fn ui_minimap_camera_rect_is_normalized_and_clamped() {
        let world = Vec2::new(8192.0, 8192.0);
        let rect = camera_rect(world * 0.5, world * 0.25, world);
        assert!((rect[0] - 0.25).abs() < 1e-4);
        assert!((rect[1] - 0.25).abs() < 1e-4);
        assert!((rect[2] - 0.5).abs() < 1e-4);
        assert!((rect[3] - 0.5).abs() < 1e-4);
        let edge = camera_rect(Vec2::ZERO, world * 0.25, world);
        assert!(edge[0] >= 0.0 && edge[1] >= 0.0 && edge[2] <= 1.0 && edge[3] <= 1.0);
        let _ = OrderSpec::Hold;
    }
}
