//! Console plate: left/right caps + the wide centre panel.
//!
//! WC3 puts the console under the play area (`analysis/agent-re/ui_audio/
//! ENGINE_MAP.md`: CommandBar ctor `0x6f12ae80`, pitch `0x6f692cbc`,
//! gap `0x6f69ac20`, button `0x6f699c40`). The band's geometry is authored by
//! the one live composer, `mobile_hud::HudLayout::compute`; this module only
//! draws the three plate pieces into the rect that composer hands it. The old
//! `HEIGHT_FRACTION`/`layout` pair (0.129 of the screen, 3.2x shorter than the
//! live 0.42 band) was a second geometry authority with no callers and is gone
//! (WC3-GAME-003).
//!
//! The plate is three atlas elements:
//!
//! * `console_left_cap` 457x202 - the left ornament,
//! * `console_right_cap` 457x202 - its authored mirror image (the right column
//!   of the sheet is drawn with the ornament on the right, so no UV flip is
//!   needed),
//! * the 592x208 wide plate - the centre band. The manifest row for that art is
//!   named `menu_button_pressed` (row 22); `analysis/agent-hud/
//!   ui_atlas_insets.json` calls the same rect `console_plate`. Row 23
//!   `wide_panel` is the 216x118 two-row strip, *not* a console band. The centre
//!   role is therefore chosen by authored size ([`PLATE_ART`]), not by name:
//!   whatever row carries the 592x208 plate is used, with `wide_panel` only as a
//!   last-resort fallback.
//!
//! ## Measured nine-slice insets
//!
//! [`INSETS`] is measured from `analysis/h2/atlas/elements/
//! 22_menu_button_pressed_592x208.png` (592x208 RGBA, chroma-keyed): walk each
//! edge inward until the warm bevel ornament (gold bevel + red gem, alpha > 200,
//! `r >= 60 && r - b >= 25`) is gone from the perpendicular middle band
//! (25%..75%, so the left/right gems are not sampled when measuring top/bottom
//! and vice versa). Result **left 56, top 32, right 56, bottom 64 px**, and the
//! rectangle they cut out (`[56..536] x [32..144]`) contains **0** ornament
//! pixels - the stretched centre is plain stone, the invariant a nine-patch
//! needs. The rectangle is minimal: shrinking any single side by 1 px admits
//! ornament again (left 1, top 165, right 1, bottom 3 warm pixels), so the
//! values are measured, not padded.
//!
//! The two end caps are *single ornaments*, not slices: their width is derived
//! from the authored aspect ([`cap_width`] = `rect.h * atlas_w / atlas_h`), so
//! the whole element is a uniform scale of the art. Slicing them with fixed
//! pixel insets would keep the corners at authored size and deform the frame
//! between them at any other band height; the original art is one piece, so the
//! module zeroes the cap insets and lets [`Skin::nine`] take its one-quad path
//! (`stretch` of the full element). That still goes through `Skin::nine` - the
//! module never computes UVs.
//!
//! ## Scale
//!
//! `scale` is screen pixels per atlas pixel (the same `art_scale` notion as
//! `Chrome::frame` in `style.rs`); it multiplies the centre-panel insets so a
//! denser screen keeps the bevel at its authored proportion. `scale = 1.0`
//! draws the atlas 1:1. Caps ignore it - their geometry is fully determined by
//! `rect.h` and the authored aspect.

use super::{Skin, ROLES};
use crate::hud::{Rect, UiBatch};

/// Left end cap role.
pub const ROLE_LEFT: &str = "console_left_cap";
/// Right end cap role (the sheet already stores it mirrored).
pub const ROLE_RIGHT: &str = "console_right_cap";
/// Centre band role: manifest row 22, the 592x208 console plate.
pub const ROLE_CENTRE: &str = "menu_button_pressed";
/// Centre fallback role (216x118 two-row strip), used only when no element of
/// the authored plate size is present in the skin.
pub const ROLE_CENTRE_FALLBACK: &str = "wide_panel";

/// Authored size of the console centre plate, in atlas px (`[w, h]`).
pub const PLATE_ART: [f32; 2] = [592.0, 208.0];

/// Measured nine-slice insets of the centre plate, atlas px, `[l, t, r, b]`.
/// Minimal ornament-free boundary of the 592x208 art (see the module docs).
pub const INSETS: [f32; 4] = [56.0, 32.0, 56.0, 64.0];

/// Screen rects of the three plate pieces, for the caller's minimap/card.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlateParts {
    pub left: Rect,
    pub centre: Rect,
    pub right: Rect,
}

impl PlateParts {
    /// Whole plate: from the left cap's left edge to the right cap's right edge.
    pub fn total(&self) -> Rect {
        let right = self.right.right().max(self.left.right());
        Rect::new(
            self.left.x,
            self.centre.y.min(self.left.y).min(self.right.y),
            (right - self.left.x).max(0.0),
            self.left.h.max(self.centre.h).max(self.right.h),
        )
    }
}

/// Natural cap width for a band of `height`: the authored aspect is kept, so
/// `w = h * atlas_w / atlas_h`.
pub fn cap_width(skin: &Skin, role: &str, height: f32) -> f32 {
    let Some(index) = Skin::role_index(role) else {
        return 0.0;
    };
    let [_, _, aw, ah] = skin.rects[index];
    if aw <= 0.0 || ah <= 0.0 {
        0.0
    } else {
        (height * aw / ah).max(0.0)
    }
}

/// Atlas index of the centre plate: the 592x208 element, whatever its row is
/// named, then the named centre role, then the fallback.
fn centre_index(skin: &Skin) -> Option<usize> {
    let roles = [ROLE_CENTRE, ROLE_CENTRE_FALLBACK];
    roles
        .iter()
        .filter_map(|role| Skin::role_index(role))
        .find(|index| {
            let [_, _, w, h] = skin.rects[*index];
            (w - PLATE_ART[0]).abs() < 0.5 && (h - PLATE_ART[1]).abs() < 0.5
        })
        .or_else(|| roles.iter().find_map(|role| Skin::role_index(role)))
}

/// Role name of the centre plate, if the skin carries one.
pub fn centre_role(skin: &Skin) -> Option<&'static str> {
    centre_index(skin).map(|index| ROLES[index])
}

/// Skin clone with the per-region slicing this plate needs: caps are one quad,
/// the centre carries the measured insets scaled by `scale`. `None` when the
/// skin already matches.
fn prepared(skin: &Skin, centre: Option<usize>, scale: f32) -> Option<Skin> {
    let mut out = skin.clone();
    let mut changed = false;
    for role in [ROLE_LEFT, ROLE_RIGHT] {
        if let Some(index) = Skin::role_index(role) {
            if out.insets[index] != [0.0; 4] {
                out.insets[index] = [0.0; 4];
                changed = true;
            }
        }
    }
    if let Some(index) = centre {
        let mut insets = skin.insets[index];
        if insets.iter().sum::<f32>() <= 0.0 {
            insets = INSETS;
        }
        let scaled = [
            insets[0] * scale,
            insets[1] * scale,
            insets[2] * scale,
            insets[3] * scale,
        ];
        if scaled != out.insets[index] {
            out.insets[index] = scaled;
            changed = true;
        }
    }
    changed.then_some(out)
}

/// One cap rect: aspect-locked width/height, vertically centred in the band.
fn cap_rect(skin: &Skin, role: &str, rect: Rect, width: f32, at_left: bool) -> Rect {
    let Some(index) = Skin::role_index(role) else {
        let x = if at_left { rect.x } else { rect.right() };
        return Rect::new(x, rect.y, 0.0, 0.0);
    };
    let [_, _, aw, ah] = skin.rects[index];
    let h = if aw > 0.0 { width * ah / aw } else { rect.h };
    let y = rect.y + (rect.h - h) * 0.5;
    let x = if at_left {
        rect.x
    } else {
        rect.right() - width
    };
    Rect::new(x, y, width, h)
}

/// Draw the console plate into `rect` (the band returned by [`layout`]).
///
/// Emits the left cap, the centre plate and the right cap, in that order, and
/// returns their screen rects. Caps keep the authored `457:202` aspect; the
/// centre stretches horizontally between them through `Skin::nine`. Nothing is
/// drawn outside `rect`; a skin without the plate roles simply draws fewer
/// pieces (the returned rects stay valid, zero-width where no role exists).
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    scale: f32,
    tint: [f32; 4],
) -> PlateParts {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return PlateParts {
            left: Rect::new(rect.x, rect.y, 0.0, 0.0),
            centre: Rect::new(rect.x, rect.y, 0.0, 0.0),
            right: Rect::new(rect.x, rect.y, 0.0, 0.0),
        };
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let centre = centre_index(skin);
    let prepared_skin = prepared(skin, centre, scale);
    let skin = prepared_skin.as_ref().unwrap_or(skin);

    // Caps may not exceed half the band each, otherwise the centre would have a
    // negative width; when that happens the cap shrinks uniformly (width *and*
    // height), so the ornament is never stretched.
    let half = rect.w * 0.5;
    let left_w = cap_width(skin, ROLE_LEFT, rect.h).min(half);
    let right_w = cap_width(skin, ROLE_RIGHT, rect.h).min(half);
    let left = cap_rect(skin, ROLE_LEFT, rect, left_w, true);
    let right = cap_rect(skin, ROLE_RIGHT, rect, right_w, false);
    let centre_w = (right.x - left.right()).max(0.0);
    let centre_rect = Rect::new(left.right(), rect.y, centre_w, rect.h);

    if left.w > 0.0 && left.h > 0.0 {
        skin.nine(batch, ROLE_LEFT, left, tint);
    }
    if centre_w > 0.0 {
        if let Some(index) = centre {
            let role = ROLES[index];
            // The atlas plates carry a **central medallion** (measured: red/gold
            // band at columns 236..316 of the 592 px console plate), so a single
            // span stretches that ornament into a wide smear — the "plank"
            // artifact. WC3 tiles its console art instead of stretching it:
            // repeat the plate at its natural aspect and clip the last tile.
            let [_, _, art_w, art_h] = skin.rects[index];
            let fit = rect.h / art_h.max(1.0);              // height-fit scale
            let tile_w = (art_w * fit).max(1.0);
            // A span within ~1.35x of the plate's natural height-fit width keeps
            // the authored nine-patch (corners preserved); beyond that the
            // medallion would smear, so the band is tiled and the last tile is
            // clipped to the remaining width.
            if centre_w <= tile_w * 1.35 {
                skin.nine(batch, role, centre_rect, tint);
            } else {
                let [ax, ay, aw, ah] = skin.rects[index];
                let mut x = centre_rect.x;
                let mut guard = 0;
                while x < centre_rect.right() - 0.5 && guard < 64 {
                    guard += 1;
                    let w = tile_w.min(centre_rect.right() - x);
                    let frac = (w / tile_w).clamp(0.0, 1.0);
                    let uv = [
                        ax / skin.width,
                        ay / skin.height,
                        (ax + aw * frac) / skin.width,
                        (ay + ah) / skin.height,
                    ];
                    batch.textured_quad(skin.texture, Rect::new(x, rect.y, w, rect.h), uv, tint);
                    x += w;
                }
            }
            let _ = role;
        }
    }
    if right.w > 0.0 && right.h > 0.0 {
        skin.nine(batch, ROLE_RIGHT, right, tint);
    }

    PlateParts {
        left,
        centre: centre_rect,
        right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    const CAP_ART: [f32; 2] = [457.0, 202.0];

    /// Rebuild the screen rect and UV range of every quad in the batch.
    fn quads(batch: &UiBatch) -> Vec<(Rect, [f32; 4])> {
        let mut out = Vec::new();
        for draw in &batch.textured {
            for vertices in draw.vertices.chunks_exact(6) {
                let mut min = [f32::MAX; 2];
                let mut max = [f32::MIN; 2];
                for vertex in vertices {
                    min[0] = min[0].min(vertex.pos[0]);
                    min[1] = min[1].min(vertex.pos[1]);
                    max[0] = max[0].max(vertex.pos[0]);
                    max[1] = max[1].max(vertex.pos[1]);
                }
                let u0 = vertices.iter().map(|v| v.uv[0]).fold(f32::MAX, f32::min);
                let u1 = vertices.iter().map(|v| v.uv[0]).fold(f32::MIN, f32::max);
                let v0 = vertices.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min);
                let v1 = vertices.iter().map(|v| v.uv[1]).fold(f32::MIN, f32::max);
                out.push((
                    Rect::new(min[0], min[1], max[0] - min[0], max[1] - min[1]),
                    [u0, v0, u1, v1],
                ));
            }
        }
        out
    }

    fn centre_quads(batch: &UiBatch, parts: &PlateParts) -> Vec<(Rect, [f32; 4])> {
        quads(batch)
            .into_iter()
            .filter(|(rect, _)| {
                rect.x >= parts.centre.x - 0.01
                    && rect.right() <= parts.centre.right() + 0.01
                    && rect.y >= parts.centre.y - 0.01
                    && rect.bottom() <= parts.centre.bottom() + 0.01
            })
            .collect()
    }

    fn has(quads: &[(Rect, [f32; 4])], w: f32, h: f32, uv: [f32; 4]) -> bool {
        quads.iter().any(|(rect, quad_uv)| {
            (rect.w - w).abs() <= 0.01
                && (rect.h - h).abs() <= 0.01
                && (quad_uv[0] - uv[0]).abs() <= 1e-5
                && (quad_uv[1] - uv[1]).abs() <= 1e-5
                && (quad_uv[2] - uv[2]).abs() <= 1e-5
                && (quad_uv[3] - uv[3]).abs() <= 1e-5
        })
    }

    #[test]
    fn nine_patch_corners_are_identical_at_1x_and_2x_target() {
        let mut skin = sample_skin();
        let centre = centre_index(&skin).expect("sample skin carries the plate");
        assert_eq!(centre, 22, "row 22 is the 592x208 console plate");
        skin.insets[centre] = INSETS;

        let rect_1x = Rect::new(0.0, 600.0, 1200.0, PLATE_ART[1]);
        let rect_2x = Rect::new(0.0, 600.0, 2400.0, PLATE_ART[1]);
        let mut batch_1x = UiBatch::default();
        let mut batch_2x = UiBatch::default();
        let parts_1x = draw(&skin, &mut batch_1x, rect_1x, 1.0, [1.0; 4]);
        let parts_2x = draw(&skin, &mut batch_2x, rect_2x, 1.0, [1.0; 4]);

        let quads_1x = centre_quads(&batch_1x, &parts_1x);
        let quads_2x = centre_quads(&batch_2x, &parts_2x);
        assert_eq!(quads_1x.len(), 9, "a comfortable span keeps the 3x3 nine-patch");
        // Beyond ~1.35x of the plate's natural height-fit width the band is
        // **tiled** instead of stretched (the atlas plate carries a centred
        // medallion that would smear), so 2x is several clipped repeats rather
        // than nine cells.
        assert!(
            quads_2x.len() >= 2,
            "a wide target must tile, got {} quads",
            quads_2x.len()
        );
        let tile_w = skin.rects[centre][2] * (rect_2x.h / skin.rects[centre][3]);
        for (quad, _) in &quads_2x {
            assert!(
                quad.w <= tile_w + 0.5,
                "a tile was stretched to {} > {tile_w}",
                quad.w
            );
        }

        let [ax, ay, aw, ah] = skin.rects[centre];
        let [l, t, r, b] = INSETS;
        let uv = |x0: f32, y0: f32, x1: f32, y1: f32| {
            [
                x0 / skin.width,
                y0 / skin.height,
                x1 / skin.width,
                y1 / skin.height,
            ]
        };
        let corners = [
            (l, t, uv(ax, ay, ax + l, ay + t)),
            (r, t, uv(ax + aw - r, ay, ax + aw, ay + t)),
            (l, b, uv(ax, ay + ah - b, ax + l, ay + ah)),
            (r, b, uv(ax + aw - r, ay + ah - b, ax + aw, ay + ah)),
        ];
        for (w, h, corner_uv) in corners {
            assert!(
                has(&quads_1x, w, h, corner_uv),
                "1x target lost the {w}x{h} corner"
            );
            // (2x tiles, so corner identity is only meaningful on the nine-patch
            // path; tiled repeats keep the full plate per tile by construction.)
        }
    }

    #[test]
    fn no_quad_outside_the_target_rect_or_its_element() {
        let skin = sample_skin();
        let rect = Rect::new(13.25, 640.5, 1601.0, 211.0);
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, rect, 1.5, [1.0; 4]);
        assert!(parts.left.w > 0.0 && parts.right.w > 0.0 && parts.centre.w > 0.0);

        let centre = centre_index(&skin).expect("plate role");
        let element_rects = [skin.rects[0], skin.rects[1], skin.rects[centre]];
        let element_uv: Vec<[f32; 4]> = element_rects
            .iter()
            .map(|&[ax, ay, aw, ah]| {
                [
                    ax / skin.width,
                    ay / skin.height,
                    (ax + aw) / skin.width,
                    (ay + ah) / skin.height,
                ]
            })
            .collect();

        let emitted = quads(&batch);
        assert!(!emitted.is_empty(), "nothing drawn");
        for (quad, uv) in emitted {
            assert!(quad.w > 0.0 && quad.h > 0.0, "degenerate quad {quad:?}");
            assert!(
                quad.x >= rect.x - 0.01
                    && quad.right() <= rect.right() + 0.01
                    && quad.y >= rect.y - 0.01
                    && quad.bottom() <= rect.bottom() + 0.01,
                "quad {quad:?} escapes the console rect {rect:?}"
            );
            assert!(
                uv.iter().all(|value| (0.0..=1.0).contains(value)),
                "uv {uv:?} outside the atlas"
            );
            let inside_element = element_uv.iter().any(|bounds| {
                uv[0] >= bounds[0] - 1e-6
                    && uv[1] >= bounds[1] - 1e-6
                    && uv[2] <= bounds[2] + 1e-6
                    && uv[3] <= bounds[3] + 1e-6
            });
            assert!(
                inside_element,
                "quad samples outside its element rects: uv {uv:?}"
            );
        }
    }

    #[test]
    fn caps_keep_the_authored_aspect_within_one_pixel() {
        let skin = sample_skin();
        for (screen_w, screen_h) in [
            (1920.0, 1080.0),
            (1080.0, 2414.0),
            (2560.0, 1080.0),
            (400.0, 1080.0), // narrower than the two caps: clamp path
        ] {
            // Any band rect: the plate pieces scale to whatever the live
            // composer hands them (the band geometry is authored elsewhere).
            let rect = Rect::new(0.0, 0.0, screen_w, screen_h * 0.42);
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, rect, 1.0, [1.0; 4]);
            let aspect = CAP_ART[0] / CAP_ART[1];
            assert!(
                (parts.left.w - parts.left.h * aspect).abs() <= 1.0,
                "{screen_w}x{screen_h}: left cap {}x{} breaks 457:202",
                parts.left.w,
                parts.left.h
            );
            assert!(
                (parts.right.w - parts.right.h * aspect).abs() <= 1.0,
                "{screen_w}x{screen_h}: right cap {}x{} breaks 457:202",
                parts.right.w,
                parts.right.h
            );
            assert!((parts.left.x - rect.x).abs() <= 0.01);
            assert!((parts.right.right() - rect.right()).abs() <= 0.01);
            assert!((parts.centre.x - parts.left.right()).abs() <= 0.01);
            assert!((parts.centre.right() - parts.right.x).abs() <= 0.01);
            let total = parts.total();
            assert!((total.w - rect.w).abs() <= 0.01, "plate must span the band");
            for (quad, _) in quads(&batch) {
                assert!(
                    quad.x >= rect.x - 0.01
                        && quad.right() <= rect.right() + 0.01
                        && quad.y >= rect.y - 0.01
                        && quad.bottom() <= rect.bottom() + 0.01,
                    "{screen_w}x{screen_h}: quad {quad:?} outside {rect:?}"
                );
            }
        }
    }

    #[test]
    fn measured_insets_are_used_when_the_skin_has_none() {
        let mut skin = sample_skin();
        skin.insets = [[0.0; 4]; 24];
        let rect = Rect::new(0.0, 0.0, 1200.0, PLATE_ART[1]);
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, rect, 1.0, [1.0; 4]);
        let centre_quads = centre_quads(&batch, &parts);
        assert_eq!(centre_quads.len(), 9, "INSETS must restore the nine cells");
        assert!(
            centre_quads
                .iter()
                .any(|(quad, _)| (quad.w - INSETS[0]).abs() <= 0.01
                    && (quad.h - INSETS[1]).abs() <= 0.01),
            "top-left cell must be {}x{}",
            INSETS[0],
            INSETS[1]
        );
        assert!(
            centre_quads
                .iter()
                .any(|(quad, _)| (quad.w - INSETS[2]).abs() <= 0.01
                    && (quad.h - INSETS[3]).abs() <= 0.01),
            "bottom-right cell must be {}x{}",
            INSETS[2],
            INSETS[3]
        );
    }

    #[test]
    fn scale_multiplies_the_centre_insets() {
        let mut skin = sample_skin();
        let centre = centre_index(&skin).expect("plate role");
        skin.insets[centre] = INSETS;
        let rect = Rect::new(0.0, 0.0, 1600.0, PLATE_ART[1] * 1.5);
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, rect, 1.5, [1.0; 4]);
        let quads = centre_quads(&batch, &parts);
        assert!(
            quads
                .iter()
                .any(|(quad, _)| (quad.w - INSETS[0] * 1.5).abs() <= 0.01
                    && (quad.h - INSETS[1] * 1.5).abs() <= 0.01),
            "1.5x art scale must give a {}x{} corner",
            INSETS[0] * 1.5,
            INSETS[1] * 1.5
        );
    }

    #[test]
    fn degenerate_rect_draws_nothing_and_stays_in_bounds() {
        let skin = sample_skin();
        for rect in [
            Rect::new(10.0, 10.0, 0.0, 139.0),
            Rect::new(10.0, 10.0, 400.0, 0.0),
            Rect::new(10.0, 10.0, 100.0, 139.0),
        ] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, rect, 1.0, [1.0; 4]);
            assert!(parts.centre.w >= 0.0);
            for (quad, _) in quads(&batch) {
                assert!(quad.x >= rect.x - 0.01 && quad.right() <= rect.right() + 0.01);
                assert!(quad.y >= 0.0);
            }
        }
    }
}

#[cfg(test)]
mod tiling_tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    /// The console centre must **tile** at the plate's natural aspect instead of
    /// one stretched span: the atlas plate has a central medallion that smears
    /// into a wide band when stretched (the "plank" artifact on device).
    #[test]
    fn centre_plate_tiles_at_natural_aspect() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let rect = Rect::new(0.0, 700.0, 2308.0, 316.8);
        let parts = draw(&skin, &mut batch, rect, 1.1, [1.0; 4]);
        assert!(parts.centre.w > 0.0, "centre must exist");
        // Height-fit tile width for the 592x208 plate: 316.8 * 592/208 = 901.6.
        let fit = rect.h / 208.0;
        let tile_w = 592.0 * fit;
        let quads = batch.textured[0].vertices.len() / 6;
        let expected = (parts.centre.w / tile_w).ceil() as usize;
        assert!(
            quads >= expected,
            "expected >= {expected} tiles for {} px centre, got {quads} quads",
            parts.centre.w
        );
        // No quad may be wider than the natural tile width.
        for quad in batch.textured[0].vertices.chunks(6) {
            let xs: Vec<f32> = quad.iter().map(|v| v.pos[0]).collect();
            let w = xs.iter().cloned().fold(f32::MIN, f32::max)
                - xs.iter().cloned().fold(f32::MAX, f32::min);
            assert!(w <= tile_w + 0.5, "a tile was stretched to {w} > {tile_w}");
        }
    }
}
