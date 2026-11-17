//! Command tiles: the 4x3 card, its six states and the hotkey badge.
//!
//! Retail contract (r2, `analysis/agent-re/ui_audio/ENGINE_MAP.md` §1):
//! `CCommandBar` (**0x6f12ae80**) creates **12** `CCommandButton`s in a 4x3
//! row-major loop and sizes itself `(gap + size) * n - gap`; the constants read
//! from the image are `size = 0.038`, `gap = 0.0075`, `pitch = 0.0455`, i.e.
//! `4 * 0.0455 - 0.0075 = 0.1745` wide and `3 * 0.0455 - 0.0075 = 0.129` tall.
//! `CCommandButton` (**0x6f12c2b0**) owns exactly **one** text frame — the
//! `MasterFont` hotkey glyph, `CommandButtonNumber` over
//! `CommandButtonNumberOverlay` — so a tile never carries a name or a cost
//! (`ToolTipName`/`ToolTipCost` are tooltip payload).
//!
//! Geometry: [`TILE_H`], [`GAP_H`] and [`PITCH_H`] are fractions of the card
//! reference height supplied by [`reference_height`]. [`grid`] derives the pitch
//! from the card rect it is handed (`pitch = (h + gap) / 3` with
//! `gap = pitch * GAP_H / PITCH_H`), so a card measured on screen and a card
//! built from [`card_rect`] produce the same pitch, and the 4x3 grid keeps
//! square tiles in either orientation.
//!
//! Draw order: [`draw_tile`] paints the state frame (and the cooldown sweep);
//! the caller then paints the ability icon into [`TileRects::icon`] and the
//! hotkey letter into [`TileRects::hotkey`]. The icon rect is the art's own
//! window (see [`WINDOW`]), so a later icon never covers the gold ring.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region (the normal/hover/pressed family shares
/// the tile silhouette).
pub const ROLE: &str = "tile_normal";

/// Card columns (`CCommandBar` `[+0x59] = 0xc`, four `PlaceButton` calls per row).
pub const COLUMNS: usize = 4;
/// Card rows (three `PlaceButton(col, row, frame, 1)` rows).
pub const ROWS: usize = 3;
/// Fixed command-card capacity: 12 buttons, row-major (`slot = row * 4 + col`).
pub const SLOTS: usize = COLUMNS * ROWS;

/// Tile side, fraction of the card reference height (`0.038` in the image).
pub const TILE_H: f32 = 0.038;
/// Gap between tiles, fraction of the reference height (`0x6f69ac20` = `0.0075`).
pub const GAP_H: f32 = 0.0075;
/// Tile pitch, fraction of the reference height (`size + gap` = `0.0455`).
pub const PITCH_H: f32 = 0.0455;
/// Card width, fraction of the reference height (`4 * pitch - gap`).
pub const CARD_W_H: f32 = 0.1745;
/// Card height, fraction of the reference height (`3 * pitch - gap`).
pub const CARD_H_H: f32 = 0.129;

/// Hotkey badge square at the tile's top-left corner, as a fraction of the tile.
/// The engine anchors its single text frame to the button's own corner `4`
/// (`func_0x6f013520(4, ...)` at `0x6f12c3d3`) with a `-0.000061` x nudge.
pub const HOTKEY_BADGE: f32 = 0.42;

/// The tile art's icon window, as fractions of the tile (left, top, right,
/// bottom). Measured on the shipped element art — mid-line scans of
/// `analysis/h2/atlas/elements/{10_tile_normal,15_tile_empty}.png`; every tile
/// state shares the silhouette, so one conservative table serves all six:
/// `tile_normal` ends its gold band at x=12/89, y=12/90, x=76/89 and y=72/90,
/// `tile_empty` at x=12/91, y=9/91, x=78/91 and y=77/91. The union is
/// `[13/89, 13/90, 13/89, 17/90]`. Measurements and the scan output live in
/// `analysis/hudskin/tiles.md`.
pub const WINDOW: [f32; 4] = [0.146, 0.144, 0.146, 0.189];

/// An icon never takes less than this share of its tile side, at any `scale`
/// or tile size, so [`TileRects::icon`] is always strictly inside the frame.
const MIN_ICON_SHARE: f32 = 0.10;

/// Tint multiplier of the cooldown sweep over `tile_dark`: a dark, slightly
/// blue veil so the icon still reads through it.
const COOLDOWN_DARKEN: [f32; 4] = [0.25, 0.25, 0.30, 0.85];

/// Draw state of one command tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileState {
    /// Idle slot: `tile_normal`.
    Normal,
    /// Pointer over the slot: `tile_hover`.
    Hover,
    /// Held down: `tile_pressed`.
    Pressed,
    /// Present but unusable: `tile_dark` (the `%sDIS%s` disabled art is a
    /// separate icon concern, not a separate tile frame).
    Disabled,
    /// On cooldown for `t` of the tile height (`0.0..=1.0` remaining): the
    /// normal frame plus a `tile_dark` sweep over the top `t` of the tile.
    Cooldown(f32),
    /// No ability in this slot: `tile_empty`.
    Empty,
}

impl TileState {
    /// Atlas role of the frame this state paints.
    pub fn role(self) -> &'static str {
        match self {
            TileState::Normal | TileState::Cooldown(_) => "tile_normal",
            TileState::Hover => "tile_hover",
            TileState::Pressed => "tile_pressed",
            TileState::Disabled => "tile_dark",
            TileState::Empty => "tile_empty",
        }
    }

    /// Remaining cooldown fraction, clamped to `0.0..=1.0`; `None` when the
    /// state is not a cooldown.
    pub fn cooldown(self) -> Option<f32> {
        match self {
            TileState::Cooldown(t) => Some(t.clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// What [`draw_tile`] leaves for the caller to paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileRects {
    /// Hotkey glyph box: top-left corner, [`HOTKEY_BADGE`] of the tile square.
    pub hotkey: Rect,
    /// Ability icon box: the frame art's window (see [`WINDOW`]).
    pub icon: Rect,
}

/// Viewport height used by the retail UI unit space (`0.6` units = height), so
/// the card fractions above always read as "of the screen height".
pub fn reference_height(viewport: [f32; 2]) -> f32 {
    viewport[1].max(0.0)
}

/// The command-card rect the retail numbers describe: `0.1745 x 0.129` of the
/// reference height, placed at `(x, y)`.
pub fn card_rect(x: f32, y: f32, reference_height: f32) -> Rect {
    let h = reference_height.max(0.0);
    Rect::new(x, y, CARD_W_H * h, CARD_H_H * h)
}

/// The 12 slot rects of a 4x3 card, row-major, square, inside `card`.
///
/// The pitch comes from the card itself (`pitch = (h + gap) / 3`,
/// `gap = pitch * GAP_H / PITCH_H`), so the retail plate yields
/// `pitch = 0.0455 * h_card / 0.129` and a hand-measured plate keeps the same
/// proportions. The narrower of the two axes wins, so the grid never leaves
/// the card, and it is centred on both axes.
pub fn grid(card: Rect) -> [Rect; SLOTS] {
    let gap_ratio = GAP_H / PITCH_H;
    let card_w = card.w.max(0.0);
    let card_h = card.h.max(0.0);
    let pitch_x = card_w / (COLUMNS as f32 - gap_ratio);
    let pitch_y = card_h / (ROWS as f32 - gap_ratio);
    let pitch = pitch_x.min(pitch_y);
    let gap = pitch * gap_ratio;
    let side = (pitch - gap).max(0.0);
    let grid_w = COLUMNS as f32 * pitch - gap;
    let grid_h = ROWS as f32 * pitch - gap;
    let x0 = card.x + (card_w - grid_w) * 0.5;
    let y0 = card.y + (card_h - grid_h) * 0.5;
    let mut slots = [Rect::new(x0, y0, side, side); SLOTS];
    for row in 0..ROWS {
        for col in 0..COLUMNS {
            slots[row * COLUMNS + col] = Rect::new(
                x0 + col as f32 * pitch,
                y0 + row as f32 * pitch,
                side,
                side,
            );
        }
    }
    slots
}

/// Split a measured left/right (or top/bottom) window pair over `size`,
/// scaling both sides and keeping at least [`MIN_ICON_SHARE`] of `size` free.
fn inset_pair(first: f32, second: f32, scale: f32, size: f32) -> (f32, f32) {
    let scale = if scale.is_finite() { scale.max(0.0) } else { 1.0 };
    let (mut a, mut b) = (first * scale, second * scale);
    let limit = (1.0 - MIN_ICON_SHARE).max(0.0);
    let sum = a + b;
    if sum > limit && sum > 0.0 {
        let k = limit / sum;
        a *= k;
        b *= k;
    }
    (a * size, b * size)
}

/// Draw one command tile (frame + cooldown sweep) and return the boxes the
/// caller still has to fill: the hotkey glyph and the icon.
///
/// `state` selects the atlas role ([`TileState::role`]); `scale` is the HUD art
/// scale: it grows the frame ornament (`Skin::nine_scaled`, so the 12 px bevel
/// stays 12 px at 1.0) and the measured art window ([`WINDOW`]) together, which
/// keeps the icon in the ring at every zoom. The icon always keeps at least
/// [`MIN_ICON_SHARE`] of the tile. `tint` multiplies the whole tile.
pub fn draw_tile(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    state: TileState,
    scale: f32,
    tint: [f32; 4],
) -> TileRects {
    skin.nine_scaled(batch, state.role(), rect, tint, scale);
    if let Some(t) = state.cooldown() {
        if t > 0.0 {
            let [r, g, b, a] = COOLDOWN_DARKEN;
            let sweep = Rect::new(rect.x, rect.y, rect.w, rect.h * t);
            skin.stretch(
                batch,
                "tile_dark",
                sweep,
                [tint[0] * r, tint[1] * g, tint[2] * b, tint[3] * a],
            );
        }
    }
    let (left, right) = inset_pair(WINDOW[0], WINDOW[2], scale, rect.w);
    let (top, bottom) = inset_pair(WINDOW[1], WINDOW[3], scale, rect.h);
    TileRects {
        hotkey: Rect::new(
            rect.x,
            rect.y,
            rect.w * HOTKEY_BADGE,
            rect.h * HOTKEY_BADGE,
        ),
        icon: rect.inset_sides(left, top, right, bottom),
    }
}

/// Draw the whole 4x3 card: lays out [`SLOTS`] tiles at the retail pitch,
/// paints each with `state_for(slot)` and returns the slot rects (row-major),
/// so the caller can hit-test and fill icons without recomputing the grid.
pub fn draw_card(
    skin: &Skin,
    batch: &mut UiBatch,
    card_rect: Rect,
    state_for: impl Fn(usize) -> TileState,
    scale: f32,
) -> [Rect; SLOTS] {
    let slots = grid(card_rect);
    for (index, slot) in slots.iter().enumerate() {
        draw_tile(skin, batch, *slot, state_for(index), scale, [1.0; 4]);
    }
    slots
}

/// Normal-state tile frame (the region default; state-aware callers use
/// [`draw_tile`] for the frame, the cooldown sweep and the content rects).
pub fn draw(skin: &Skin, batch: &mut UiBatch, rect: Rect, tint: [f32; 4]) -> bool {
    skin.nine(batch, ROLE, rect, tint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::Rect;
    use crate::hudskin::tests_support::sample_skin;

    /// Landscape phone (2414x1080) and the same panel rotated (1080x2414).
    const VIEWPORTS: [[f32; 2]; 2] = [[2414.0, 1080.0], [1080.0, 2414.0]];

    fn near(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn card_for(viewport: [f32; 2]) -> Rect {
        card_rect(120.0, 700.0, reference_height(viewport))
    }

    /// The last 6 vertices of the batch are the most recent quad (the batch
    /// only merges into the previous draw, painter order preserved).
    fn last_quad(batch: &UiBatch) -> [(f32, f32); 4] {
        let draw = batch.textured.last().expect("quads emitted");
        let v = &draw.vertices[draw.vertices.len() - 6..];
        [
            (v[0].pos[0], v[0].pos[1]),
            (v[1].pos[0], v[1].pos[1]),
            (v[2].pos[0], v[2].pos[1]),
            (v[5].pos[0], v[5].pos[1]),
        ]
    }

    #[test]
    fn grid_is_twelve_slots_on_the_retail_pitch() {
        for viewport in VIEWPORTS {
            let h = reference_height(viewport);
            let card = card_for(viewport);
            let slots = grid(card);
            assert_eq!(slots.len(), SLOTS, "12 slots");
            let pitch = PITCH_H * h;
            let side = TILE_H * h;
            assert!(
                near(pitch, 0.0455 * h, 1e-3),
                "pitch {} != 0.0455H {}",
                pitch,
                0.0455 * h
            );
            for slot in slots {
                assert!(near(slot.w, side, 1e-3), "tile side {} != 0.038H {}", slot.w, side);
                assert!(near(slot.h, slot.w, 1e-4), "tiles are square");
            }
            for row in 0..ROWS {
                let first = slots[row * COLUMNS];
                for col in 1..COLUMNS {
                    let slot = slots[row * COLUMNS + col];
                    assert!(
                        near(slot.x - first.x, col as f32 * pitch, 1e-3),
                        "column {col} pitch of row {row}"
                    );
                    assert!(near(slot.y, first.y, 1e-4), "row {row} shares one y");
                }
                if row > 0 {
                    let above = slots[(row - 1) * COLUMNS];
                    assert!(
                        near(first.y - above.y, pitch, 1e-3),
                        "row {row} pitch"
                    );
                }
            }
            // The retail plate is exactly filled: first slot at the card origin
            // and the last slot's right/bottom edge on the card's far edge.
            let first = slots[0];
            let last = slots[SLOTS - 1];
            assert!(near(first.x, card.x, 1e-3) && near(first.y, card.y, 1e-3));
            assert!(near(last.right(), card.right(), 1e-3), "grid fills the width");
            assert!(near(last.bottom(), card.bottom(), 1e-3), "grid fills the height");
            // And nothing leaves the card.
            for slot in slots {
                assert!(slot.x >= card.x - 1e-4 && slot.right() <= card.right() + 1e-4);
                assert!(slot.y >= card.y - 1e-4 && slot.bottom() <= card.bottom() + 1e-4);
            }
            println!(
                "viewport {:?}: card {:.2}x{:.2} pitch {:.3} gap {:.3} tile {:.3}",
                viewport,
                card.w,
                card.h,
                pitch,
                pitch - side,
                side
            );
        }
    }

    #[test]
    fn every_quad_stays_inside_its_tile() {
        let skin = sample_skin();
        let states = [
            TileState::Normal,
            TileState::Hover,
            TileState::Pressed,
            TileState::Disabled,
            TileState::Cooldown(0.4),
            TileState::Empty,
        ];
        for viewport in VIEWPORTS {
            for slot in grid(card_for(viewport)) {
                for state in states {
                    let mut batch = UiBatch::default();
                    let rects = draw_tile(&skin, &mut batch, slot, state, 1.0, [1.0; 4]);
                    assert!(!batch.textured.is_empty(), "{state:?} drew nothing");
                    for draw in &batch.textured {
                        for vertex in &draw.vertices {
                            let (x, y) = (vertex.pos[0], vertex.pos[1]);
                            assert!(
                                x >= slot.x - 0.5 && x <= slot.right() + 0.5,
                                "{state:?} x {x} outside {:?}",
                                slot
                            );
                            assert!(
                                y >= slot.y - 0.5 && y <= slot.bottom() + 0.5,
                                "{state:?} y {y} outside {:?}",
                                slot
                            );
                            assert!((0.0..=1.0).contains(&vertex.uv[0]));
                            assert!((0.0..=1.0).contains(&vertex.uv[1]));
                        }
                    }
                    for part in [rects.icon, rects.hotkey] {
                        assert!(part.x >= slot.x - 1e-4 && part.right() <= slot.right() + 1e-4);
                        assert!(part.y >= slot.y - 1e-4 && part.bottom() <= slot.bottom() + 1e-4);
                    }
                }
            }
        }
    }

    #[test]
    fn icon_and_hotkey_are_strictly_inside_the_frame() {
        let skin = sample_skin();
        let tile = Rect::new(200.0, 300.0, 68.4, 68.4);
        for state in [
            TileState::Normal,
            TileState::Hover,
            TileState::Pressed,
            TileState::Disabled,
            TileState::Cooldown(0.5),
            TileState::Empty,
        ] {
            for scale in [0.5, 1.0, 2.0, 50.0] {
                let mut batch = UiBatch::default();
                let rects = draw_tile(&skin, &mut batch, tile, state, scale, [1.0; 4]);
                let icon = rects.icon;
                assert!(
                    icon.x > tile.x && icon.y > tile.y,
                    "{state:?}@{scale}: icon origin not inside: {icon:?}"
                );
                assert!(
                    icon.right() < tile.right() && icon.bottom() < tile.bottom(),
                    "{state:?}@{scale}: icon edge not inside: {icon:?}"
                );
                assert!(icon.w >= tile.w * MIN_ICON_SHARE - 1e-3, "icon width share");
                assert!(icon.h >= tile.h * MIN_ICON_SHARE - 1e-3, "icon height share");
                let badge = rects.hotkey;
                assert_eq!(badge.x, tile.x, "badge is top-left anchored");
                assert_eq!(badge.y, tile.y, "badge is top-left anchored");
                assert!(near(badge.w, tile.w * 0.42, 1e-4), "badge is 0.42 of the tile");
                assert!(near(badge.h, tile.h * 0.42, 1e-4), "badge is 0.42 of the tile");
                assert!(badge.right() < tile.right() && badge.bottom() < tile.bottom());
            }
        }
        // At scale 1 the icon is the measured art window.
        let mut batch = UiBatch::default();
        let rects = draw_tile(&skin, &mut batch, tile, TileState::Normal, 1.0, [1.0; 4]);
        assert!(near(rects.icon.x - tile.x, tile.w * WINDOW[0], 1e-4));
        assert!(near(rects.icon.y - tile.y, tile.h * WINDOW[1], 1e-4));
        assert!(near(tile.right() - rects.icon.right(), tile.w * WINDOW[2], 1e-4));
        assert!(near(tile.bottom() - rects.icon.bottom(), tile.h * WINDOW[3], 1e-4));
    }

    #[test]
    fn nine_patch_corners_keep_their_pixels_at_2x() {
        let skin = sample_skin();
        let one_x = Rect::new(100.0, 50.0, 68.0, 68.0);
        let two_x = Rect::new(100.0, 50.0, 136.0, 136.0);
        let mut small = UiBatch::default();
        let mut large = UiBatch::default();
        draw_tile(&skin, &mut small, one_x, TileState::Normal, 1.0, [1.0; 4]);
        draw_tile(&skin, &mut large, two_x, TileState::Normal, 1.0, [1.0; 4]);
        // Top-left corner: same vertices and same UVs at both sizes (12 px insets).
        let corner_small = &small.textured[0].vertices[..6];
        let corner_large = &large.textured[0].vertices[..6];
        for (a, b) in corner_small.iter().zip(corner_large.iter()) {
            assert_eq!(a.pos, b.pos, "corner vertex moved at 2x");
            assert_eq!(a.uv, b.uv, "corner UV changed at 2x");
        }
        assert!(near(corner_small[2].pos[0] - one_x.x, 12.0, 1e-4), "1x corner is 12 px");
        assert!(near(corner_large[2].pos[0] - two_x.x, 12.0, 1e-4), "2x corner is 12 px");
        // Positive control: the frame really grew — far edge vertices differ.
        let far_small = &small.textured[0].vertices;
        let far_large = &large.textured[0].vertices;
        assert_eq!(far_small.len(), far_large.len(), "same nine slices");
        let right_small = far_small[far_small.len() - 4].pos[0];
        let right_large = far_large[far_large.len() - 4].pos[0];
        assert!(near(right_small, one_x.right(), 1e-4), "1x right edge");
        assert!(
            near(right_large - right_small, 68.0, 1e-4),
            "edge stretched by the tile delta"
        );

        // The HUD art scale grows the ornament instead of shrinking the border:
        // same 68 px tile, `scale = 2` → a 24 px corner at the same UV texels.
        let mut scaled = UiBatch::default();
        draw_tile(&skin, &mut scaled, one_x, TileState::Normal, 2.0, [1.0; 4]);
        let corner_scaled = &scaled.textured[0].vertices[..6];
        assert!(
            near(corner_scaled[2].pos[0] - one_x.x, 24.0, 1e-4),
            "art_scale 2 doubles the corner: {}",
            corner_scaled[2].pos[0] - one_x.x
        );
        assert!(
            near(corner_scaled[2].pos[1] - one_x.y, 24.0, 1e-4),
            "art_scale 2 doubles the corner height"
        );
        for (a, b) in corner_small.iter().zip(corner_scaled.iter()) {
            assert_eq!(a.uv, b.uv, "corner texels are scale-free");
        }
    }

    #[test]
    fn cooldown_sweep_height_is_proportional_to_t() {
        let skin = sample_skin();
        let tile = Rect::new(40.0, 60.0, 68.4, 68.4);
        let vertices = |t: f32| {
            let mut batch = UiBatch::default();
            draw_tile(&skin, &mut batch, tile, TileState::Cooldown(t), 1.0, [1.0; 4]);
            let total: usize = batch.textured.iter().map(|d| d.vertices.len()).sum();
            let quad = last_quad(&batch);
            (total, quad[3].1 - quad[0].1, quad)
        };
        let (base, sweep_zero, _) = vertices(0.0);
        assert_eq!(base, 9 * 6, "t=0 draws the frame only");
        assert!(near(sweep_zero, 12.0, 1e-4), "no sweep quad at t=0 (last quad is the corner)");
        let (_, quarter, _) = vertices(0.25);
        let (_, half, _) = vertices(0.5);
        let (_, three_quarters, _) = vertices(0.75);
        let (full_count, full, full_quad) = vertices(1.0);
        assert_eq!(full_count, 9 * 6 + 6, "one sweep quad");
        assert!(near(quarter, tile.h * 0.25, 1e-3), "25% sweep = {} vs {}", quarter, tile.h * 0.25);
        assert!(near(half, tile.h * 0.5, 1e-3));
        assert!(near(three_quarters, tile.h * 0.75, 1e-3));
        assert!(near(full, tile.h, 1e-3), "full sweep covers the tile");
        assert!(near(three_quarters / quarter, 3.0, 1e-3), "height tracks t");
        // The sweep starts at the tile's top edge and spans its width.
        assert!(near(full_quad[0].1, tile.y, 1e-4), "sweep anchored to the top edge");
        assert!(near(full_quad[1].0 - full_quad[0].0, tile.w, 1e-4), "sweep spans the width");
        // Clamped outside 0..1.
        assert!(near(vertices(1.7).1, tile.h, 1e-3), "t > 1 clamps to the full tile");
        assert_eq!(vertices(-0.3).0, 9 * 6, "t < 0 draws the frame only");
    }

    #[test]
    fn state_picks_its_atlas_role() {
        let skin = sample_skin();
        let tile = Rect::new(0.0, 0.0, 68.0, 68.0);
        for (state, role) in [
            (TileState::Normal, "tile_normal"),
            (TileState::Hover, "tile_hover"),
            (TileState::Pressed, "tile_pressed"),
            (TileState::Disabled, "tile_dark"),
            (TileState::Empty, "tile_empty"),
            (TileState::Cooldown(0.3), "tile_normal"),
        ] {
            let index = Skin::role_index(role).expect("role exists");
            let [ax, ay, aw, ah] = skin.rects[index];
            let mut batch = UiBatch::default();
            draw_tile(&skin, &mut batch, tile, state, 1.0, [1.0; 4]);
            let first = &batch.textured[0].vertices[0];
            let (u, v) = (first.uv[0], first.uv[1]);
            assert!(
                u >= ax / skin.width - 1e-6 && u <= (ax + aw) / skin.width + 1e-6,
                "{state:?} U {u} outside {role}"
            );
            assert!(
                v >= ay / skin.height - 1e-6 && v <= (ay + ah) / skin.height + 1e-6,
                "{state:?} V {v} outside {role}"
            );
            assert_eq!(state.role(), role, "role accessor agrees with the paint");
        }
        // Disabled and empty are distinct art, never the normal frame.
        assert_ne!(TileState::Disabled.role(), TileState::Normal.role());
        assert_ne!(TileState::Empty.role(), TileState::Normal.role());
    }

    #[test]
    fn card_draws_every_slot_and_reports_its_rects() {
        let skin = sample_skin();
        let card = card_for(VIEWPORTS[0]);
        let mut batch = UiBatch::default();
        let slots = draw_card(&skin, &mut batch, card, |_| TileState::Normal, 1.0);
        // Nine-slice of a 41 px tile with 12 px insets = 9 quads = 54 vertices.
        let vertices: usize = batch.textured.iter().map(|draw| draw.vertices.len()).sum();
        assert_eq!(vertices, SLOTS * 9 * 6, "12 nine-patched tiles");
        assert_eq!(slots, grid(card), "draw_card returns the grid");

        // States are honoured per slot: slot 5 on cooldown adds exactly one quad.
        let mut mixed = UiBatch::default();
        let slots = draw_card(
            &skin,
            &mut mixed,
            card,
            |index| if index == 5 { TileState::Cooldown(0.5) } else { TileState::Normal },
            1.0,
        );
        let mixed_vertices: usize = mixed.textured.iter().map(|d| d.vertices.len()).sum();
        assert_eq!(mixed_vertices, SLOTS * 9 * 6 + 6, "one sweep quad for slot 5");
        assert_eq!(slots[5], grid(card)[5]);

        // A zero-area slot cannot panic and cannot produce a negative icon.
        let mut empty = UiBatch::default();
        let rects = draw_tile(
            &skin,
            &mut empty,
            Rect::new(0.0, 0.0, 0.0, 0.0),
            TileState::Normal,
            1.0,
            [1.0; 4],
        );
        assert_eq!(rects.icon.w, 0.0);
        assert_eq!(rects.icon.h, 0.0);
    }

    #[test]
    fn region_draws_inside_its_rect() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let rect = Rect::new(100.0, 50.0, 400.0, 120.0);
        assert!(draw(&skin, &mut batch, rect, [1.0; 4]), "role must exist");
        assert!(!batch.textured.is_empty(), "no quads emitted");
        for quad in &batch.textured {
            for vertex in &quad.vertices {
                let x = vertex.pos[0];
                let y = vertex.pos[1];
                assert!(x >= rect.x - 0.5 && x <= rect.right() + 0.5, "x outside");
                assert!(y >= rect.y - 0.5 && y <= rect.bottom() + 0.5, "y outside");
                assert!(vertex.uv[0] >= 0.0 && vertex.uv[0] <= 1.0);
                assert!(vertex.uv[1] >= 0.0 && vertex.uv[1] <= 1.0);
            }
        }
    }
}
