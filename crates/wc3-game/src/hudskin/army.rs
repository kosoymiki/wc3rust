//! Army chips and control-group slots.
//!
//! Two HUD regions that share the same atlas language:
//!
//! * the row of unit chips under the selection panel — one nine-patch chip per
//!   selected unit, the icon inside the frame and a 4 px micro HP bar across the
//!   bottom of that icon ([`draw_chip`], [`draw_army_row`]);
//! * the ten `1..9, 0` control-group buttons at the console centre — a small
//!   square plate with the key box reserved for the HUD font ([`draw_group`]).
//!
//! Geometry comes from the atlas, not from constants in this file: the border
//! between a chip frame and its icon is the element's own nine-patch inset
//! scaled from the source rect to the destination rect, so a 34 px chip gets a
//! 4 px border and a 68 px chip 8 px, exactly like the art. [`CHIP_UNIT`] is
//! 34 px because that is the live chip side at scale 1
//! (`mobile_hud.rs::army_slots`, 34 px chip / 5 px gap), and the group buttons
//! default to the live 40 px / 6 px row of `game.rs::group_row_slots`.
//!
//! Draw order inside a chip is *icon first, then chip*: the caller paints the
//! unit portrait into the [`Rect`] returned by [`draw_chip`], and the frame and
//! HP bar are emitted over it — WC3 draws the health bar across the icon.

use glam::Vec2;

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Selected chip frame (`slot_gold`, 102x99 in the atlas).
pub const ROLE: &str = "slot_gold";

/// Unselected chip frame (`slot_dark`, 102x99 in the atlas).
pub const ROLE_DARK: &str = "slot_dark";

/// Control-group button plate, the 216x118 atlas element. The art brief calls
/// this element `small_panel`; `hudskin::ROLES` (read-only) carries it under the
/// name `wide_panel`, so the constant has to be the name that actually resolves
/// in [`Skin::role_index`]. [`ROLE_GROUP_ART_NAME`] keeps the brief's spelling.
pub const ROLE_GROUP: &str = "wide_panel";

/// Art-brief name of [`ROLE_GROUP`].
pub const ROLE_GROUP_ART_NAME: &str = "small_panel";

/// Chip side at scale 1 (`mobile_hud.rs` army slot, 34 px).
pub const CHIP_UNIT: f32 = 34.0;

/// Height of the micro HP bar at the bottom of a chip, at scale 1.
pub const HP_BAR_H: f32 = 4.0;

/// Dark backing behind the HP fill, so an empty unit still reads as a bar.
pub const HP_TRACK: [f32; 4] = [0.03, 0.03, 0.035, 0.92];

/// Interior padding around a group button's key box, at scale 1.
pub const GROUP_TEXT_PAD: f32 = 3.0;

/// Full-strength tint for a row chip: the atlas role already carries the gold /
/// dark difference, so the row never tints a frame away from its art.
pub const CHIP_TINT: [f32; 4] = [1.0; 4];

/// Tint of a control-group plate that holds units.
pub const GROUP_TINT_OCCUPIED: [f32; 4] = [0.95, 0.90, 0.72, 1.0];

/// Tint of an empty control-group plate.
pub const GROUP_TINT_EMPTY: [f32; 4] = [0.52, 0.55, 0.62, 0.55];

// ---------------------------------------------------------------------------
// Chip geometry
// ---------------------------------------------------------------------------

/// Border width of a chip in destination pixels: the larger horizontal atlas
/// inset projected from the 102 px source element onto the destination width,
/// capped at a quarter of the chip so the icon always keeps half the chip. The
/// cap matters with the measured art (`ui_atlas_insets.json` gives `slot_gold`
/// 42/26/42/28 px, which is most of a 34 px chip): the ornament is drawn, but
/// the portrait hole stays usable at phone sizes.
pub fn chip_border(skin: &Skin, rect: Rect) -> f32 {
    let Some(index) = Skin::role_index(ROLE) else {
        return 0.0;
    };
    let atlas_w = skin.rects[index][2].max(1.0);
    let inset = skin.insets[index][0].max(skin.insets[index][2]).max(0.0);
    let max = (rect.w.min(rect.h) * 0.25).max(0.0);
    (inset * (rect.w / atlas_w)).clamp(0.0, max)
}

/// The icon hole of a chip: the target rect inset by [`chip_border`].
pub fn chip_icon_rect(skin: &Skin, rect: Rect) -> Rect {
    let border = chip_border(skin, rect);
    rect.inset(border, border)
}

/// The micro HP bar rect: [`HP_BAR_H`] scaled by `scale`, never more than a
/// quarter of the icon height, pinned to the bottom of the icon hole so it is
/// inside both the icon and the chip frame.
pub fn chip_hp_rect(skin: &Skin, rect: Rect, scale: f32) -> Rect {
    let icon = chip_icon_rect(skin, rect);
    let height = (HP_BAR_H * scale.max(0.0)).min(icon.h * 0.25).max(0.0);
    Rect::new(icon.x, icon.bottom() - height, icon.w, height)
}

/// HP colour: red at zero, olive at half, green at full health.
pub fn hp_color(ratio: f32) -> [f32; 4] {
    const LOW: [f32; 4] = [0.86, 0.14, 0.10, 1.0];
    const MID: [f32; 4] = [0.66, 0.62, 0.12, 1.0];
    const FULL: [f32; 4] = [0.24, 0.78, 0.22, 1.0];
    let t = ratio.clamp(0.0, 1.0);
    let (from, to, k) = if t < 0.5 {
        (LOW, MID, t * 2.0)
    } else {
        (MID, FULL, (t - 0.5) * 2.0)
    };
    [
        from[0] + (to[0] - from[0]) * k,
        from[1] + (to[1] - from[1]) * k,
        from[2] + (to[2] - from[2]) * k,
        1.0,
    ]
}

// ---------------------------------------------------------------------------
// Chip drawing
// ---------------------------------------------------------------------------

/// Draw one army chip: `slot_gold` when `selected`, `slot_dark` otherwise, plus
/// the micro HP bar. Returns the icon rect the caller paints the portrait into.
///
/// Call this *after* the icon so the frame and the bar stay on top.
pub fn draw_chip(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    hp_ratio: f32,
    selected: bool,
    scale: f32,
    tint: [f32; 4],
) -> Rect {
    let icon = chip_icon_rect(skin, rect);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return icon;
    }
    let role = if selected { ROLE } else { ROLE_DARK };
    skin.nine(batch, role, rect, tint);

    let bar = chip_hp_rect(skin, rect, scale);
    if bar.w > 0.0 && bar.h > 0.0 {
        batch.rect(bar, HP_TRACK);
        let ratio = hp_ratio.clamp(0.0, 1.0);
        let fill = Rect::new(bar.x, bar.y, bar.w * ratio, bar.h);
        if fill.w > 0.0 {
            batch.rect(fill, hp_color(ratio));
        }
    }
    icon
}

// ---------------------------------------------------------------------------
// Control-group buttons
// ---------------------------------------------------------------------------

/// The box a group button reserves for its key glyph. The module has no `Font`
/// (the hudskin contract is rects + quads only), so the caller draws `key`
/// inside this rect with the HUD font, dense element class.
pub fn group_text_rect(skin: &Skin, rect: Rect, scale: f32) -> Rect {
    let border = match Skin::role_index(ROLE_GROUP) {
        Some(index) => {
            let atlas_w = skin.rects[index][2].max(1.0);
            skin.insets[index][0].max(0.0) * (rect.w / atlas_w)
        }
        None => 0.0,
    };
    let pad = (border + GROUP_TEXT_PAD * scale.max(0.0))
        .min(rect.w.min(rect.h) * 0.25)
        .max(0.0);
    rect.inset(pad, pad)
}

/// Draw one control-group plate and return the rect the key glyph goes into.
///
/// `occupied` picks the plate tint (a group with units reads warm and solid, an
/// empty one dim and translucent); the key itself is the caller's text job, so
/// the reserved box is the only thing this function returns.
pub fn draw_group(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    key: &str,
    occupied: bool,
    scale: f32,
) -> Rect {
    debug_assert!(
        !key.is_empty() && key.chars().count() <= 3,
        "control-group keys are 1-3 glyphs ({key:?})"
    );
    debug_assert!(key.is_ascii(), "control-group keys are ASCII ({key:?})");
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Rect::new(rect.x, rect.y, 0.0, 0.0);
    }
    let tint = if occupied {
        GROUP_TINT_OCCUPIED
    } else {
        GROUP_TINT_EMPTY
    };
    skin.nine(batch, ROLE_GROUP, rect, tint);
    group_text_rect(skin, rect, scale)
}

/// Left-to-right row of square group buttons starting at `origin`.
pub fn group_row(origin: Vec2, count: usize, size: f32, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let pitch = size.max(0.0) + gap.max(0.0);
    (0..count)
        .map(|index| {
            Rect::new(
                origin.x + index as f32 * pitch,
                origin.y,
                size.max(0.0),
                size.max(0.0),
            )
        })
        .collect()
}

/// A group row centred in `area`, shrunk by the [`fit_row`] rule when it does
/// not fit (`game.rs::group_row_slots` centres its ten buttons the same way).
pub fn group_row_centred(area: Rect, count: usize, size: f32, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let (size, gap) = fit_row(area.w, count, size, gap);
    let total = count as f32 * size + (count as f32 - 1.0) * gap;
    let x0 = area.x + ((area.w - total) * 0.5).max(0.0);
    let y = area.y + ((area.h - size) * 0.5).max(0.0);
    (0..count)
        .map(|index| Rect::new(x0 + index as f32 * (size + gap), y, size, size))
        .collect()
}

// ---------------------------------------------------------------------------
// Row layout
// ---------------------------------------------------------------------------

/// The one shrink rule of this module.
///
/// A row of `count` cells at `chip` + `gap` needs
/// `count*chip + (count-1)*gap`. When that exceeds `avail` the pitch keeps its
/// `chip : gap` ratio and the row is solved exactly for `avail`:
/// `gap = avail / (count*chip/gap + count - 1)`, `chip = gap * chip/gap`.
/// With `gap == 0` the chips simply split `avail`. The row therefore always
/// fits, and only the *ratio* between chip and gap is preserved — a 12-unit row
/// stays visually rhythmic instead of collapsing into a line of gaps.
pub fn fit_row(avail: f32, count: usize, chip: f32, gap: f32) -> (f32, f32) {
    let chip = chip.max(0.0);
    let gap = gap.max(0.0);
    if count == 0 || avail <= 0.0 || chip <= 0.0 {
        return (0.0, 0.0);
    }
    let needed = count as f32 * chip + (count as f32 - 1.0) * gap;
    if needed <= avail {
        return (chip, gap);
    }
    if gap <= f32::EPSILON {
        return (avail / count as f32, 0.0);
    }
    let ratio = chip / gap;
    let gap = avail / (count as f32 * ratio + count as f32 - 1.0);
    (ratio * gap, gap)
}

/// Right-aligned row of square chips inside `area`; the last chip's right edge
/// is `area.right()` and every chip's top is `area.y`.
pub fn army_row_rects(area: Rect, count: usize, chip: f32, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let (chip, gap) = fit_row(area.w, count, chip, gap);
    let total = count as f32 * chip + (count as f32 - 1.0) * gap;
    let x0 = area.right() - total;
    (0..count)
        .map(|index| Rect::new(x0 + index as f32 * (chip + gap), area.y, chip, chip))
        .collect()
}

/// Draw the army row inside a known selection area and return the chip rects.
///
/// The first chip is the active subgroup and takes the `slot_gold` frame; the
/// rest take `slot_dark`. The micro HP bar scale follows the chip size
/// ([`CHIP_UNIT`]), because the row has no separate scale parameter.
pub fn draw_army_row_in(
    skin: &Skin,
    batch: &mut UiBatch,
    area: Rect,
    count: usize,
    chip: f32,
    gap: f32,
    hp: impl Fn(usize) -> f32,
) -> Vec<Rect> {
    let rects = army_row_rects(area, count, chip, gap);
    for (index, rect) in rects.iter().enumerate() {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let scale = (rect.w / CHIP_UNIT).max(0.0);
        draw_chip(skin, batch, *rect, hp(index), index == 0, scale, CHIP_TINT);
    }
    rects
}

/// Draw the army row anchored at `origin`, the required region entry point.
///
/// `origin.x` is the row's right edge (right-aligned) and `origin.y` its top
/// edge. The available width is the span from the HUD's left edge (`x == 0`) to
/// `origin.x`; callers that own the selection rect and want the row bounded by
/// it use [`draw_army_row_in`] directly, which is the same layout with an
/// explicit left bound.
pub fn draw_army_row(
    skin: &Skin,
    batch: &mut UiBatch,
    origin: Vec2,
    count: usize,
    chip: f32,
    gap: f32,
    hp: impl Fn(usize) -> f32,
) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let area = Rect::new(0.0, origin.y, origin.x.max(0.0), chip.max(0.0));
    draw_army_row_in(skin, batch, area, count, chip, gap, hp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    const EPS: f32 = 1e-3;

    /// Screen rects of the white (solid) quads, in emission order.
    fn white_rects(batch: &UiBatch) -> Vec<Rect> {
        batch
            .white
            .chunks(6)
            .map(|vertices| {
                let x0 = vertices[0].pos[0];
                let y0 = vertices[0].pos[1];
                let x1 = vertices[2].pos[0];
                let y1 = vertices[2].pos[1];
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            })
            .collect()
    }

    #[test]
    fn chip_border_scales_with_the_chip() {
        let skin = sample_skin();
        // slot_gold is 102 px wide with a 12 px inset: the border is 12/102 of
        // the destination width (4 px at the live 34 px chip).
        for (chip, border) in [(34.0, 4.0), (68.0, 8.0), (102.0, 12.0)] {
            let rect = Rect::new(10.0, 20.0, chip, chip);
            let got = chip_border(&skin, rect);
            assert!((got - border).abs() < EPS, "chip {chip}: border {got}");
            let icon = chip_icon_rect(&skin, rect);
            assert!(icon.is_inside(&rect));
            assert!(
                (icon.w - (chip - 2.0 * border)).abs() < EPS,
                "icon {icon:?}"
            );
        }
        // A 4 px chip keeps a 1 px icon instead of a negative hole.
        let tiny = chip_icon_rect(&skin, Rect::new(0.0, 0.0, 4.0, 4.0));
        assert!(tiny.w > 0.0 && tiny.w < 4.0);
    }

    #[test]
    fn chip_border_keeps_the_icon_usable_with_the_measured_insets() {
        // Real art (`analysis/h2/atlas/ui_atlas_insets.json`): slot_gold is
        // 42/26/42/28 and slot_dark 21/27/21/29 on a 102x99 element. A 34 px
        // chip must still show a portrait hole of at least half its side.
        let mut skin = sample_skin();
        skin.insets[16] = [42.0, 26.0, 42.0, 28.0];
        skin.insets[17] = [21.0, 27.0, 21.0, 29.0];
        for chip in [24.0, 34.0, 68.0] {
            let rect = Rect::new(0.0, 0.0, chip, chip);
            let border = chip_border(&skin, rect);
            let icon = chip_icon_rect(&skin, rect);
            assert!(border <= chip * 0.25 + EPS, "chip {chip}: border {border}");
            assert!(icon.w >= chip * 0.5 - EPS, "chip {chip}: icon {icon:?}");
            assert!(chip_hp_rect(&skin, rect, chip / CHIP_UNIT).h > 0.0);
        }
        // 34 px chip with the measured 42 px inset: capped at 8.5 px.
        let rect = Rect::new(0.0, 0.0, 34.0, 34.0);
        assert!((chip_border(&skin, rect) - 8.5).abs() < EPS);
    }

    #[test]
    fn hp_bar_sits_inside_the_chip_and_icon() {
        let skin = sample_skin();
        for chip in [24.0, 34.0, 40.0, 68.0, 102.0] {
            let rect = Rect::new(300.0, 400.0, chip, chip);
            let icon = chip_icon_rect(&skin, rect);
            let bar = chip_hp_rect(&skin, rect, chip / CHIP_UNIT);
            assert!(bar.is_inside(&rect), "chip {chip}: {bar:?} outside chip");
            assert!(bar.is_inside(&icon), "chip {chip}: {bar:?} outside icon");
            assert!(bar.h > 0.0 && bar.h <= icon.h * 0.25 + EPS);
            assert!(
                (bar.bottom() - icon.bottom()).abs() < EPS,
                "bar pinned to icon bottom"
            );
            assert!((bar.w - icon.w).abs() < EPS, "bar spans the icon");
        }
        // The live 34 px chip gets exactly the requested 4 px bar.
        let bar = chip_hp_rect(&skin, Rect::new(0.0, 0.0, 34.0, 34.0), 1.0);
        assert!((bar.h - HP_BAR_H).abs() < EPS, "bar {bar:?}");
    }

    #[test]
    fn hp_bar_fill_follows_the_ratio() {
        let skin = sample_skin();
        let rect = Rect::new(50.0, 60.0, 68.0, 68.0);
        let bar = chip_hp_rect(&skin, rect, 2.0);
        for ratio in [0.0f32, 0.25, 0.5, 1.0] {
            let mut batch = UiBatch::default();
            draw_chip(&skin, &mut batch, rect, ratio, true, 2.0, CHIP_TINT);
            let whites = white_rects(&batch);
            assert_eq!(
                whites.len(),
                if ratio > 0.0 { 2 } else { 1 },
                "track + fill"
            );
            assert!((whites[0].x - bar.x).abs() < EPS && (whites[0].w - bar.w).abs() < EPS);
            if ratio > 0.0 {
                assert!(
                    (whites[1].w - bar.w * ratio).abs() < EPS,
                    "fill width at {ratio}"
                );
                assert!((whites[1].x - bar.x).abs() < EPS);
            }
        }
    }

    #[test]
    fn hp_colour_runs_red_to_green() {
        let low = hp_color(0.0);
        let mid = hp_color(0.5);
        let full = hp_color(1.0);
        assert!(low[0] > low[1], "0% must be red-dominant: {low:?}");
        assert!(full[1] > full[0], "100% must be green-dominant: {full:?}");
        assert!(low[1] < mid[1] && mid[1] < full[1], "green rises with hp");
        assert!(low[0] > mid[0] && mid[0] > full[0], "red falls with hp");
        assert_eq!(hp_color(-2.0), low);
        assert_eq!(hp_color(3.0), full);
    }

    #[test]
    fn selected_and_normal_chips_use_different_roles() {
        let skin = sample_skin();
        let rect = Rect::new(200.0, 120.0, 68.0, 68.0);
        let mut gold = UiBatch::default();
        let mut dark = UiBatch::default();
        draw_chip(&skin, &mut gold, rect, 0.8, true, 2.0, CHIP_TINT);
        draw_chip(&skin, &mut dark, rect, 0.8, false, 2.0, CHIP_TINT);
        let (gold_u, dark_u) = (
            gold.textured[0].vertices[0].uv,
            dark.textured[0].vertices[0].uv,
        );
        assert!(
            (gold_u[0] - 1087.0 / 1536.0).abs() < 1e-5,
            "slot_gold uv {gold_u:?}"
        );
        assert!(
            (dark_u[0] - 1203.0 / 1536.0).abs() < 1e-5,
            "slot_dark uv {dark_u:?}"
        );
        assert!((gold_u[0] - dark_u[0]).abs() > 0.01, "roles must differ");
        // Both frames are the same element size; the top-left corner cell of
        // the nine-patch keeps the authored 12 px inset.
        let gold_u1 = gold.textured[0].vertices[1].uv[0];
        assert!(
            (gold_u1 - gold_u[0] - 12.0 / 1536.0).abs() < 1e-5,
            "corner cell keeps the 12 px inset"
        );
    }

    #[test]
    fn chip_quads_stay_inside_the_chip() {
        let skin = sample_skin();
        let rect = Rect::new(400.0, 250.0, 34.0, 34.0);
        let mut batch = UiBatch::default();
        let icon = draw_chip(&skin, &mut batch, rect, 0.5, true, 1.0, CHIP_TINT);
        assert!(icon.is_inside(&rect));
        assert!(!batch.textured.is_empty(), "chip frame must emit quads");
        for draw in &batch.textured {
            for vertex in &draw.vertices {
                assert!(vertex.pos[0] >= rect.x - 0.5 && vertex.pos[0] <= rect.right() + 0.5);
                assert!(vertex.pos[1] >= rect.y - 0.5 && vertex.pos[1] <= rect.bottom() + 0.5);
                assert!((0.0..=1.0).contains(&vertex.uv[0]));
                assert!((0.0..=1.0).contains(&vertex.uv[1]));
            }
        }
        for solid in white_rects(&batch) {
            assert!(
                solid.is_inside(&rect),
                "hp bar piece {solid:?} outside chip"
            );
        }
    }

    #[test]
    fn chips_never_overlap() {
        let skin = sample_skin();
        for count in 1..=12usize {
            for (chip, gap) in [(34.0, 5.0), (68.0, 10.0), (40.0, 6.0), (12.0, 2.0)] {
                let area = Rect::new(100.0, 200.0, 480.0, 200.0);
                let mut batch = UiBatch::default();
                let rects = draw_army_row_in(&skin, &mut batch, area, count, chip, gap, |index| {
                    (index % 5) as f32 / 4.0
                });
                assert_eq!(rects.len(), count, "one rect per unit");
                for pair in rects.windows(2) {
                    assert!(
                        pair[0].right() <= pair[1].x + EPS,
                        "overlap count {count} chip {chip}: {:?} {:?}",
                        pair[0],
                        pair[1]
                    );
                }
                for rect in &rects {
                    assert!(
                        rect.is_inside(&area),
                        "count {count}: {rect:?} outside area"
                    );
                }
                if count > 0 && chip > 0.0 {
                    assert!(!batch.textured.is_empty(), "row must draw");
                }
            }
        }
    }

    #[test]
    fn row_is_right_aligned_and_shrinks_to_fit() {
        let skin = sample_skin();
        let area = Rect::new(120.0, 180.0, 480.0, 160.0);
        let rects = draw_army_row_in(&skin, &mut UiBatch::default(), area, 12, 68.0, 10.0, |_| {
            1.0
        });
        assert_eq!(rects.len(), 12);
        assert!(
            (rects.last().unwrap().right() - area.right()).abs() < EPS,
            "right-aligned"
        );
        assert!(
            rects.iter().all(|rect| rect.is_inside(&area)),
            "row escapes the selection area"
        );
        assert!(
            rects[0].w < 68.0,
            "12 chips must shrink below the requested 68 px"
        );
        assert!(rects[0].w > 0.0);
        // Ratio preserved by the documented rule.
        let ratio_before = 68.0 / 10.0;
        let ratio_after = rects[0].w / (rects[1].x - rects[0].right());
        assert!(
            (ratio_after - ratio_before).abs() < 0.05,
            "ratio {ratio_after} vs {ratio_before}"
        );

        // A row that fits keeps the requested size exactly.
        let wide = Rect::new(0.0, 0.0, 2000.0, 100.0);
        let fits = army_row_rects(wide, 5, 68.0, 10.0);
        assert!((fits[0].w - 68.0).abs() < EPS);
        assert!((fits[1].x - fits[0].right() - 10.0).abs() < EPS);
    }

    #[test]
    fn army_row_chip_sizes_for_5_8_12_units() {
        // Evidence table for analysis/hudskin/army.md: a 480 px selection column
        // at UI scale 2 (chip 34*2 = 68, gap 5*2 = 10).
        let area = Rect::new(0.0, 0.0, 480.0, 180.0);
        let mut report = Vec::new();
        for count in [5usize, 8, 12] {
            let rects = army_row_rects(area, count, 68.0, 10.0);
            let chip = rects[0].w;
            let gap = rects[1].x - rects[0].right();
            let width = rects.last().unwrap().right() - rects[0].x;
            assert!(rects.iter().all(|rect| rect.is_inside(&area)));
            assert!(
                (width
                    - if count == 5 {
                        5.0 * 68.0 + 4.0 * 10.0
                    } else {
                        area.w
                    })
                .abs()
                    < EPS
            );
            report.push(format!(
                "count={count} chip={chip:.2} gap={gap:.2} width={width:.2}"
            ));
        }
        println!("{}", report.join("\n"));
    }

    #[test]
    fn draw_army_row_anchors_the_right_edge_at_origin() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let rects = draw_army_row(
            &skin,
            &mut batch,
            Vec2::new(1000.0, 120.0),
            5,
            34.0,
            5.0,
            |_| 0.5,
        );
        assert_eq!(rects.len(), 5);
        assert!((rects.last().unwrap().right() - 1000.0).abs() < EPS);
        assert!((rects[0].y - 120.0).abs() < EPS);
        assert!(rects[0].x >= 0.0);
        assert!(!batch.textured.is_empty(), "chips must draw");
        assert_eq!(white_rects(&batch).len(), 10, "track + fill per chip");
        // The first chip is the active subgroup: gold frame, rest dark.
        let first_uv = batch.textured[0].vertices[0].uv[0];
        assert!((first_uv - 1087.0 / 1536.0).abs() < 1e-5);
    }

    #[test]
    fn draw_army_row_shrinks_when_the_screen_is_narrow() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        // 12 chips of 34 px + 11 gaps of 5 px need 463 px; only 300 are free,
        // so the required entry point applies the same shrink rule.
        let rects = draw_army_row(
            &skin,
            &mut batch,
            Vec2::new(300.0, 40.0),
            12,
            34.0,
            5.0,
            |_| 1.0,
        );
        assert_eq!(rects.len(), 12);
        assert!(
            rects
                .iter()
                .all(|rect| rect.x >= -EPS && rect.right() <= 300.0 + EPS)
        );
        assert!(
            (rects.last().unwrap().right() - 300.0).abs() < EPS,
            "right-aligned"
        );
        assert!(rects[0].w < 34.0 && rects[0].w > 0.0, "chip {}", rects[0].w);
    }

    #[test]
    fn zero_count_draws_nothing() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let rects = draw_army_row(
            &skin,
            &mut batch,
            Vec2::new(900.0, 80.0),
            0,
            34.0,
            5.0,
            |_| 1.0,
        );
        assert!(rects.is_empty());
        assert!(batch.textured.is_empty(), "no frame quads");
        assert!(batch.white.is_empty(), "no hp quads");
        assert!(army_row_rects(Rect::new(0.0, 0.0, 400.0, 100.0), 0, 34.0, 5.0).is_empty());
        assert!(group_row(Vec2::ZERO, 0, 40.0, 6.0).is_empty());
        assert!(group_row_centred(Rect::new(0.0, 0.0, 400.0, 60.0), 0, 40.0, 6.0).is_empty());
        assert_eq!(fit_row(400.0, 0, 34.0, 5.0), (0.0, 0.0));
    }

    #[test]
    fn group_buttons_are_evenly_spaced() {
        let skin = sample_skin();
        let area = Rect::new(200.0, 500.0, 400.0, 60.0);
        let buttons = group_row_centred(area, 10, 34.0, 6.0);
        assert_eq!(buttons.len(), 10);
        let pitch = buttons[1].centre().x - buttons[0].centre().x;
        for pair in buttons.windows(2) {
            let step = pair[1].centre().x - pair[0].centre().x;
            assert!((step - pitch).abs() < EPS, "uneven pitch {step} vs {pitch}");
        }
        assert!((pitch - (34.0 + 6.0)).abs() < EPS, "pitch is size + gap");
        let left = buttons[0].x - area.x;
        let right = area.right() - buttons.last().unwrap().right();
        assert!(
            (left - right).abs() < EPS,
            "row must be centred ({left} vs {right})"
        );

        let mut batch = UiBatch::default();
        for (index, rect) in buttons.iter().enumerate() {
            let key = if index < 9 {
                (index + 1).to_string()
            } else {
                "0".to_string()
            };
            let text = draw_group(&skin, &mut batch, *rect, &key, index % 2 == 0, 1.0);
            assert!(
                text.is_inside(rect),
                "key box {text:?} outside button {rect:?}"
            );
            assert!(text.w > 0.0 && text.h > 0.0);
        }
        assert!(!batch.textured.is_empty(), "group plates must draw");
    }

    #[test]
    fn group_row_shrinks_inside_a_narrow_area() {
        let area = Rect::new(0.0, 0.0, 300.0, 40.0);
        let buttons = group_row_centred(area, 10, 40.0, 6.0);
        assert_eq!(buttons.len(), 10);
        assert!(buttons.iter().all(|rect| rect.is_inside(&area)));
        assert!((buttons.last().unwrap().right() - area.right()).abs() < 30.0);
        assert!(buttons[0].w < 40.0, "chips shrink, not overflow");
    }

    #[test]
    fn group_plate_tint_reports_occupancy() {
        let skin = sample_skin();
        let rect = Rect::new(20.0, 20.0, 40.0, 40.0);
        let mut occupied = UiBatch::default();
        let mut empty = UiBatch::default();
        draw_group(&skin, &mut occupied, rect, "3", true, 1.0);
        draw_group(&skin, &mut empty, rect, "4", false, 1.0);
        let hot = occupied.textured[0].vertices[0].color;
        let dim = empty.textured[0].vertices[0].color;
        assert!(
            hot[3] > dim[3],
            "occupied plate is more solid: {hot:?} vs {dim:?}"
        );
        assert!(
            hot[0] >= dim[0],
            "occupied plate is warmer: {hot:?} vs {dim:?}"
        );
    }

    #[test]
    fn group_text_box_never_collapses() {
        let skin = sample_skin();
        for size in [24.0, 40.0, 64.0, 120.0] {
            let rect = Rect::new(0.0, 0.0, size, size);
            let text = group_text_rect(&skin, rect, 1.0);
            assert!(text.is_inside(&rect));
            assert!(text.w > 0.0 && text.h > 0.0, "size {size}: {text:?}");
            assert!(text.w < rect.w, "size {size}: the plate keeps a border");
        }
    }
}
