//! Top bar: the thin stone band across the top of the play area.
//!
//! Warcraft III keeps a single ~14 px strip at the top of the safe frame that
//! carries the map name on the left and the FPS/timer read-out on the right.
//! The strip is *not* a panel: it is a horizontally stretchable band with two
//! fixed-width end ornaments and a gold underline. This module builds that
//! layout from the fantasy atlas and emits it into the caller's [`UiBatch`].
//!
//! Three art pieces compose the bar ([`layout`] returns all the rectangles, so a
//! caller can place other widgets against them):
//!
//! * [`ROLE_CENTER`] (`topbar_center`, 526x130) — nine-patch body. The atlas art
//!   is a full panel; at a 14 px bar the source UV lines stay on the art's
//!   measured insets and the on-screen cells are `inset * bar_height / art_height`
//!   ([`nine_sliced`]), so only the middle band stretches: the end gems and the
//!   gold rim stay proportional instead of the whole 130 px panel being smeared.
//! * [`ROLE_LEFT`] (`topbar_left`, 475x78) — left end ornament at its natural
//!   aspect, mirrored on the right. Ornaments never stretch: their width follows
//!   from the bar height times the art aspect.
//! * [`ROLE_UNDERLINE`] (`thin_bar`, 465x38) — the gold line. It is drawn on the
//!   bar's own bottom `2 * scale` px, so the bar's footprint stays exactly
//!   `14 * scale` tall and the strip never bleeds into the play area.
//!
//! [`ROLE_SMALL_PANEL`] (`wide_panel`, 216x118) belongs to this lane's atlas
//! budget but is a stacked plate, not part of the thin bar; its measured insets
//! are published here so the lane's ownership table is complete and the art can
//! be measured once.
//!
//! ## Measured insets
//!
//! The `INSETS_*` values below are measured, not guessed: the seam method from
//! `analysis/agent-hud/tools/atlas_insets_author.py` (for each edge, cut where
//! the mean colour delta between the two columns/rows either side of the cut
//! drops to the noise floor and stays there for three consecutive cuts) was
//! re-run against `analysis/h2/atlas/ui_atlas.png` for the four rects of this
//! lane on 2026-11-15. The re-run reproduced the shipped sidecar
//! `analysis/h2/atlas/ui_atlas_insets.json` byte-for-byte for all four roles:
//!
//! | role | art | L | T | R | B | seam energy (L,T,R,B) |
//! |---|---|---|---|---|---|---|
//! | `topbar_center` | 526x130 | 55 | 36 | 55 | 55 | 15.10, 21.63, 11.41, 15.76 |
//! | `topbar_left` | 475x78 | 18 | 33 | 33 | 33 | 67.07, 6.04, 57.69, 12.79 |
//! | `thin_bar` | 465x38 | 16 | 16 | 16 | 16 | 23.58, 35.14, 38.54, 43.38 |
//! | `wide_panel` | 216x118 | 19 | 46 | 21 | 50 | 15.65, 9.42, 13.04, 15.56 |
//!
//! The runtime [`Skin`] carries the same numbers per role; the constants are the
//! fallback for a skin whose insets do not fit its own rect (a hand-edited
//! manifest, a zeroed sidecar), so a bad atlas entry degrades to measured art
//! values instead of a stretched smear.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Bar height at reference scale. WC3's top strip is ~14 px at 1080p.
pub const BAR_HEIGHT: f32 = 14.0;

/// Height of the gold underline, drawn on the bar's bottom edge.
pub const UNDERLINE_HEIGHT: f32 = 2.0;

/// Gap between an end ornament and the text block it guards.
pub const TEXT_PAD: f32 = 6.0;

/// Gap between the map-name block and the FPS block.
pub const TEXT_GAP: f32 = 8.0;

/// Lowest scale honoured; a zero or negative scale would collapse the bar.
pub const MIN_SCALE: f32 = 0.05;

/// Primary atlas role of this region (the nine-patch body).
pub const ROLE: &str = "topbar_center";

/// Nine-patch body, stretched across the safe frame.
pub const ROLE_CENTER: &str = "topbar_center";

/// Left end ornament; the right end reuses it mirrored.
pub const ROLE_LEFT: &str = "topbar_left";

/// Gold underline strip.
pub const ROLE_UNDERLINE: &str = "thin_bar";

/// Stacked plate owned by this lane (not part of the thin bar).
pub const ROLE_SMALL_PANEL: &str = "wide_panel";

/// Atlas art size of [`ROLE_CENTER`], in atlas px.
pub const ART_TOPBAR_CENTER: [f32; 2] = [526.0, 130.0];

/// Atlas art size of [`ROLE_LEFT`], in atlas px.
pub const ART_TOPBAR_LEFT: [f32; 2] = [475.0, 78.0];

/// Atlas art size of [`ROLE_UNDERLINE`], in atlas px.
pub const ART_THIN_BAR: [f32; 2] = [465.0, 38.0];

/// Atlas art size of [`ROLE_SMALL_PANEL`], in atlas px.
pub const ART_SMALL_PANEL: [f32; 2] = [216.0, 118.0];

/// Measured nine-patch insets of [`ROLE_CENTER`] (left, top, right, bottom),
/// atlas px.
pub const INSETS_TOPBAR_CENTER: [f32; 4] = [55.0, 36.0, 55.0, 55.0];

/// Measured nine-patch insets of [`ROLE_LEFT`], atlas px.
pub const INSETS_TOPBAR_LEFT: [f32; 4] = [18.0, 33.0, 33.0, 33.0];

/// Measured nine-patch insets of [`ROLE_UNDERLINE`], atlas px.
pub const INSETS_THIN_BAR: [f32; 4] = [16.0, 16.0, 16.0, 16.0];

/// Measured nine-patch insets of [`ROLE_SMALL_PANEL`], atlas px.
pub const INSETS_SMALL_PANEL: [f32; 4] = [19.0, 46.0, 21.0, 50.0];

/// Every rectangle of the top strip, in screen space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopBarLayout {
    /// The strip itself: safe frame left/top, safe frame width, `14 * scale`.
    pub bar: Rect,
    /// Left end ornament (full bar height, natural art aspect).
    pub left_ornament: Rect,
    /// Right end ornament, mirror image of [`Self::left_ornament`].
    pub right_ornament: Rect,
    /// Gold underline on the bar's bottom `2 * scale` px.
    pub underline: Rect,
    /// Map-name text block handed to the font draw.
    pub map_name: Rect,
    /// FPS text block handed to the font draw.
    pub fps: Rect,
}

/// Compute the top-strip layout for `safe_frame`.
///
/// `map_name_x` / `fps_x` are the caller's requested left edges for the two text
/// blocks in screen px; both are clamped into the band between the end
/// ornaments, and the map-name block never runs into the FPS block. Returns
/// `None` only when the strip would have no area (non-positive safe-frame width
/// or a scale that cannot produce a positive height).
pub fn layout(
    skin: &Skin,
    safe_frame: Rect,
    scale: f32,
    map_name_x: f32,
    fps_x: f32,
) -> Option<TopBarLayout> {
    let scale = if scale.is_finite() {
        scale.max(MIN_SCALE)
    } else {
        MIN_SCALE
    };
    let width = safe_frame.w;
    let height = BAR_HEIGHT * scale;
    if !(width > 0.0) || !(height > 0.0) {
        return None;
    }

    let bar = Rect::new(safe_frame.x, safe_frame.y, width, height);

    // Ornaments keep the art's aspect and never stretch: width follows height.
    let ornament_width = ornament_width(skin, height).min(width * 0.5);
    let left_ornament = Rect::new(bar.x, bar.y, ornament_width, height);
    let right_ornament = Rect::new(bar.right() - ornament_width, bar.y, ornament_width, height);

    let underline_height = (UNDERLINE_HEIGHT * scale).min(height);
    let underline = Rect::new(
        bar.x,
        bar.bottom() - underline_height,
        bar.w,
        underline_height,
    );

    // Text never sits on the underline and never runs under an ornament.
    let text_height = (height - underline_height).max(0.0);
    let pad = TEXT_PAD * scale;
    let gap = TEXT_GAP * scale;
    let band_left = bar.x + ornament_width + pad;
    let band_right = (bar.right() - ornament_width - pad).max(band_left);

    let fps_left = fps_x.clamp(band_left, band_right);
    let fps = Rect::new(fps_left, bar.y, band_right - fps_left, text_height);

    let map_right = (fps_left - gap).max(band_left);
    let map_left = map_name_x.clamp(band_left, map_right);
    let map_name = Rect::new(map_left, bar.y, (map_right - map_left).max(0.0), text_height);

    Some(TopBarLayout {
        bar,
        left_ornament,
        right_ornament,
        underline,
        map_name,
        fps,
    })
}

/// Draw the top strip into `batch` and return the two text rectangles
/// `(map_name, fps)` for the caller's font draw.
///
/// Paint order is body, ornaments, underline: the ornaments cover the body's end
/// cells and the underline closes the strip along its bottom edge. `tint` is
/// applied to every quad (the caller fades the HUD with it).
///
/// Returns `None` only for a degenerate strip (see [`layout`]); a skin that
/// lacks one of the roles simply skips that piece.
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    safe_frame: Rect,
    scale: f32,
    map_name_x: f32,
    fps_x: f32,
    tint: [f32; 4],
) -> Option<(Rect, Rect)> {
    let top_bar = layout(skin, safe_frame, scale, map_name_x, fps_x)?;

    draw_body(skin, batch, top_bar.bar, tint);
    if Skin::role_index(ROLE_LEFT).is_some() {
        skin.stretch(batch, ROLE_LEFT, top_bar.left_ornament, tint);
        mirror_stretch(skin, batch, ROLE_LEFT, top_bar.right_ornament, tint);
    }
    skin.stretch(batch, ROLE_UNDERLINE, top_bar.underline, tint);

    Some((top_bar.map_name, top_bar.fps))
}

/// Nine-patch the body, with the measured art insets fitted to the bar height.
fn draw_body(skin: &Skin, batch: &mut UiBatch, bar: Rect, tint: [f32; 4]) -> bool {
    let Some(index) = Skin::role_index(ROLE_CENTER) else {
        return false;
    };
    let [_, _, art_width, art_height] = skin.rects[index];
    if !(art_width > 0.0) || !(art_height > 0.0) {
        return false;
    }
    nine_sliced(
        skin,
        ROLE_CENTER,
        index,
        runtime_insets(skin, index),
        batch,
        bar,
        tint,
    )
}

/// Nine-patch where the ornament band of the art and the on-screen cell size are
/// given separately: the source UVs stay on the art's measured inset lines, the
/// destination cells are `inset * art_scale`.
///
/// This is the geometry of `Skin::nine_scaled` (same cells, same formulas,
/// same degenerate guard), which cannot be used here: it floors `art_scale` at
/// 0.25, and a 14 px bar on the 130 px `topbar_center` art needs 0.108. Above
/// that floor the two are identical, which `body_matches_the_shared_nine_patch`
/// pins down by comparing against `Skin::nine`.
fn nine_sliced(
    skin: &Skin,
    role: &str,
    index: usize,
    insets: [f32; 4],
    batch: &mut UiBatch,
    rect: Rect,
    tint: [f32; 4],
) -> bool {
    let [ax, ay, aw, ah] = skin.rects[index];
    if !(aw > 0.0) || !(ah > 0.0) || skin.width <= 0.0 || skin.height <= 0.0 {
        return false;
    }
    let [al, at, ar, ab] = insets;
    let cells = fitted_insets(insets, [aw, ah], rect.h);
    let [l, t, r, b] = cells;
    if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
        return skin.stretch(batch, role, rect, tint);
    }
    let sx = [0.0, al / aw, (aw - ar).max(al) / aw, 1.0];
    let sy = [0.0, at / ah, (ah - ab).max(at) / ah, 1.0];
    let dx = [rect.x, rect.x + l, rect.right() - r, rect.right()];
    let dy = [rect.y, rect.y + t, rect.bottom() - b, rect.bottom()];
    for row in 0..3 {
        for col in 0..3 {
            let cell = Rect::new(
                dx[col],
                dy[row],
                dx[col + 1] - dx[col],
                dy[row + 1] - dy[row],
            );
            if cell.w <= 0.0 || cell.h <= 0.0 {
                continue;
            }
            let u0 = (ax + sx[col] * aw) / skin.width;
            let u1 = (ax + sx[col + 1] * aw) / skin.width;
            let v0 = (ay + sy[row] * ah) / skin.height;
            let v1 = (ay + sy[row + 1] * ah) / skin.height;
            batch.textured_quad(skin.texture, cell, [u0, v0, u1, v1], tint);
        }
    }
    true
}

/// The skin's own insets when they describe the art, else the measured values.
fn runtime_insets(skin: &Skin, index: usize) -> [f32; 4] {
    let [_, _, art_width, art_height] = skin.rects[index];
    let insets = skin.insets[index];
    let [left, top, right, bottom] = insets;
    let usable = left >= 0.0
        && top >= 0.0
        && right >= 0.0
        && bottom >= 0.0
        && left + right < art_width
        && top + bottom < art_height;
    if usable {
        insets
    } else {
        INSETS_TOPBAR_CENTER
    }
}

/// Scale the art's insets by the height ratio `dest_height / art_height`.
///
/// A nine-patch cell is an ornament: its size in the art is proportional to the
/// art's height, so a bar that is shorter than the art shrinks the cells by the
/// same factor and only the middle band is stretched. Each inset is capped at
/// 45 % of its axis so a pathological ratio cannot eat the whole target.
fn fitted_insets(measured: [f32; 4], art: [f32; 2], dest_height: f32) -> [f32; 4] {
    let [art_width, art_height] = art;
    if !(art_height > 0.0) || !(dest_height > 0.0) {
        return [0.0; 4];
    }
    let scale = dest_height / art_height;
    [
        (measured[0] * scale).clamp(0.0, art_width * 0.45),
        (measured[1] * scale).clamp(0.0, art_height * 0.45),
        (measured[2] * scale).clamp(0.0, art_width * 0.45),
        (measured[3] * scale).clamp(0.0, art_height * 0.45),
    ]
}

/// Ornament width for a given bar height, from the loaded art's aspect.
fn ornament_width(skin: &Skin, bar_height: f32) -> f32 {
    let aspect = Skin::role_index(ROLE_LEFT)
        .map(|index| {
            let [_, _, art_width, art_height] = skin.rects[index];
            if art_width > 0.0 && art_height > 0.0 {
                art_width / art_height
            } else {
                ART_TOPBAR_LEFT[0] / ART_TOPBAR_LEFT[1]
            }
        })
        .unwrap_or(ART_TOPBAR_LEFT[0] / ART_TOPBAR_LEFT[1]);
    bar_height * aspect
}

/// [`Skin::stretch`] with the U axis flipped: the right end ornament is the same
/// art piece mirrored. `Skin` has no mirror helper, so the UV rectangle that
/// `stretch` would build is written out once, with `u0`/`u1` swapped.
fn mirror_stretch(
    skin: &Skin,
    batch: &mut UiBatch,
    role: &str,
    rect: Rect,
    tint: [f32; 4],
) -> bool {
    if skin.width <= 0.0 || skin.height <= 0.0 {
        return false;
    }
    let Some(index) = Skin::role_index(role) else {
        return false;
    };
    let [x, y, w, h] = skin.rects[index];
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let u0 = x / skin.width;
    let v0 = y / skin.height;
    let u1 = (x + w) / skin.width;
    let v1 = (y + h) / skin.height;
    batch.textured_quad(skin.texture, rect, [u1, v0, u0, v1], tint);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    const EPS: f32 = 0.01;
    const SAFE: Rect = Rect {
        x: 48.0,
        y: 80.0,
        w: 1824.0,
        h: 952.0,
    };

    /// One emitted quad: destination rect plus atlas UV rect.
    #[derive(Clone, Copy, Debug)]
    struct Quad {
        rect: Rect,
        uv: [f32; 4],
    }

    fn quads(batch: &UiBatch) -> Vec<Quad> {
        let mut out = Vec::new();
        for draw in &batch.textured {
            assert_eq!(
                draw.vertices.len() % 6,
                0,
                "quad list must be whole six-vertex quads"
            );
            for chunk in draw.vertices.chunks_exact(6) {
                let first = chunk[0];
                let third = chunk[2];
                out.push(Quad {
                    rect: Rect::new(
                        first.pos[0],
                        first.pos[1],
                        third.pos[0] - first.pos[0],
                        third.pos[1] - first.pos[1],
                    ),
                    uv: [first.uv[0], first.uv[1], third.uv[0], third.uv[1]],
                });
            }
        }
        out
    }

    /// Which lane role a quad samples, by atlas rect containment of its UVs.
    fn role_of(skin: &Skin, quad: &Quad) -> Option<&'static str> {
        let x0 = quad.uv[0] * skin.width;
        let y0 = quad.uv[1] * skin.height;
        let x1 = quad.uv[2] * skin.width;
        let y1 = quad.uv[3] * skin.height;
        let (lx, hx) = (x0.min(x1), x0.max(x1));
        let (ly, hy) = (y0.min(y1), y0.max(y1));
        for role in [ROLE_CENTER, ROLE_LEFT, ROLE_UNDERLINE] {
            let index = Skin::role_index(role).expect("role is in ROLES");
            let [rx, ry, rw, rh] = skin.rects[index];
            if lx >= rx - EPS
                && hx <= rx + rw + EPS
                && ly >= ry - EPS
                && hy <= ry + rh + EPS
            {
                return Some(role);
            }
        }
        None
    }

    fn inside(inner: Rect, outer: Rect, eps: f32) -> bool {
        inner.x >= outer.x - eps
            && inner.y >= outer.y - eps
            && inner.right() <= outer.right() + eps
            && inner.bottom() <= outer.bottom() + eps
    }

    fn draw_into(skin: &Skin, scale: f32, safe: Rect) -> (TopBarLayout, Vec<Quad>) {
        let mut batch = UiBatch::default();
        let rects = draw(
            skin,
            &mut batch,
            safe,
            scale,
            safe.x + 140.0 * scale,
            safe.right() - 360.0 * scale,
            [1.0; 4],
        )
        .expect("strip has area");
        let top_bar = layout(
            skin,
            safe,
            scale,
            safe.x + 140.0 * scale,
            safe.right() - 360.0 * scale,
        )
        .expect("strip has area");
        assert_eq!(rects, (top_bar.map_name, top_bar.fps));
        (top_bar, quads(&batch))
    }

    #[test]
    fn bar_is_fourteen_pixels_at_scale_one_and_scales_uniformly() {
        let skin = sample_skin();
        let one = layout(&skin, SAFE, 1.0, SAFE.x + 140.0, SAFE.right() - 360.0).unwrap();
        assert!((one.bar.h - BAR_HEIGHT).abs() < EPS, "14 px at scale 1");
        assert!((one.bar.w - SAFE.w).abs() < EPS, "spans the safe frame");
        assert!((one.bar.x - SAFE.x).abs() < EPS);
        assert!((one.bar.y - SAFE.y).abs() < EPS);

        let two = layout(&skin, SAFE, 2.0, SAFE.x + 280.0, SAFE.right() - 720.0).unwrap();
        assert!((two.bar.h - 2.0 * BAR_HEIGHT).abs() < EPS);
        assert!((two.underline.h - 2.0 * UNDERLINE_HEIGHT).abs() < EPS);
        assert!((two.left_ornament.w - 2.0 * one.left_ornament.w).abs() < EPS);
    }

    #[test]
    fn quads_stay_inside_the_bar_rect() {
        let skin = sample_skin();
        let (top_bar, emitted) = draw_into(&skin, 1.0, SAFE);
        assert!(!emitted.is_empty(), "no quads emitted");
        for quad in &emitted {
            assert!(
                inside(quad.rect, top_bar.bar, EPS),
                "quad {:?} leaves the bar {:?}",
                quad.rect,
                top_bar.bar
            );
            assert!(
                role_of(&skin, quad).is_some(),
                "quad samples outside its role: uv {:?}",
                quad.uv
            );
            assert!(quad.rect.w > 0.0 && quad.rect.h > 0.0, "degenerate quad");
        }

        // Every piece is present: nine body cells, two ornaments, one underline.
        let mut centre = 0;
        let mut ornaments = 0;
        let mut underline = 0;
        for quad in &emitted {
            match role_of(&skin, quad) {
                Some(ROLE_CENTER) => centre += 1,
                Some(ROLE_LEFT) => ornaments += 1,
                Some(ROLE_UNDERLINE) => underline += 1,
                _ => {}
            }
        }
        assert_eq!(centre, 9, "nine-patch body must emit nine cells");
        assert_eq!(ornaments, 2, "left and mirrored right ornament");
        assert_eq!(underline, 1, "one underline quad");
    }

    /// Corner cells of the nine-patch body: the quads that touch both a
    /// horizontal and a vertical edge of the `topbar_center` atlas rect.
    fn is_corner(skin: &Skin, quad: &Quad) -> bool {
        let index = Skin::role_index(ROLE_CENTER).unwrap();
        let [ax, ay, aw, ah] = skin.rects[index];
        let (x0, y0) = (quad.uv[0] * skin.width, quad.uv[1] * skin.height);
        let (x1, y1) = (quad.uv[2] * skin.width, quad.uv[3] * skin.height);
        let at_left = (x0 - ax).abs() < 0.01;
        let at_right = (x1 - (ax + aw)).abs() < 0.01;
        let at_top = (y0 - ay).abs() < 0.01;
        let at_bottom = (y1 - (ay + ah)).abs() < 0.01;
        (at_left || at_right) && (at_top || at_bottom)
    }

    #[test]
    fn corner_cells_are_identical_at_1x_and_2x() {
        let skin = sample_skin();
        let (one_layout, one) = draw_into(&skin, 1.0, SAFE);
        let (two_layout, two) = draw_into(&skin, 2.0, SAFE);
        assert_eq!(one.len(), two.len(), "same quad set at both scales");

        let mut corners = 0;
        for (a, b) in one.iter().zip(two.iter()) {
            assert!(
                (a.uv[0] - b.uv[0]).abs() < EPS
                    && (a.uv[1] - b.uv[1]).abs() < EPS
                    && (a.uv[2] - b.uv[2]).abs() < EPS
                    && (a.uv[3] - b.uv[3]).abs() < EPS,
                "same art sampled at both scales: {:?} vs {:?}",
                a.uv,
                b.uv
            );
            if is_corner(&skin, a) {
                corners += 1;
                assert!(
                    (b.rect.w - 2.0 * a.rect.w).abs() < EPS
                        && (b.rect.h - 2.0 * a.rect.h).abs() < EPS,
                    "corner cell must double with the scale: {:?} vs {:?}",
                    a.rect,
                    b.rect
                );
                // A corner is the same art rectangle placed at the bar's edge:
                // its offset from that edge is unchanged when the bar doubles.
                let at_right = a.rect.x > one_layout.bar.centre().x;
                let (edge_a, edge_b) = if at_right {
                    (one_layout.bar.right() - a.rect.right(), two_layout.bar.right() - b.rect.right())
                } else {
                    (a.rect.x - one_layout.bar.x, b.rect.x - two_layout.bar.x)
                };
                assert!((edge_a - edge_b).abs() < EPS, "corner hugs its edge");
            }
        }
        assert_eq!(corners, 4, "four corner cells in a nine-patch");

        assert!(
            (two_layout.left_ornament.w - 2.0 * one_layout.left_ornament.w).abs() < EPS
                && (two_layout.left_ornament.h - 2.0 * one_layout.left_ornament.h).abs() < EPS,
            "ornament doubles with the scale"
        );
        assert!(
            (two_layout.underline.h - 2.0 * one_layout.underline.h).abs() < EPS,
            "underline doubles with the scale"
        );
    }

    #[test]
    fn ornaments_keep_their_aspect_and_never_stretch() {
        let skin = sample_skin();
        let aspect = ART_TOPBAR_LEFT[0] / ART_TOPBAR_LEFT[1];
        let narrow = Rect::new(SAFE.x, SAFE.y, 900.0, SAFE.h);
        let wide = Rect::new(SAFE.x, SAFE.y, 1824.0, SAFE.h);

        let a = layout(&skin, narrow, 1.0, narrow.x + 140.0, narrow.right() - 360.0).unwrap();
        let b = layout(&skin, wide, 1.0, wide.x + 140.0, wide.right() - 360.0).unwrap();

        assert!((a.left_ornament.w / a.left_ornament.h - aspect).abs() < EPS);
        assert!((a.right_ornament.w / a.right_ornament.h - aspect).abs() < EPS);
        assert!((a.left_ornament.h - BAR_HEIGHT).abs() < EPS, "full height");
        assert!(
            (a.left_ornament.w - b.left_ornament.w).abs() < EPS,
            "bar width must not change the ornament"
        );
        assert!(
            (a.right_ornament.x - b.right_ornament.x).abs() > 1.0,
            "right ornament hugs the right end"
        );

        // The mirrored right end samples the same art with U flipped.
        let (top_bar, emitted) = draw_into(&skin, 1.0, SAFE);
        let left: Vec<_> = emitted
            .iter()
            .filter(|q| role_of(&skin, q) == Some(ROLE_LEFT) && q.rect.x < top_bar.bar.x + top_bar.left_ornament.w - EPS)
            .collect();
        let right: Vec<_> = emitted
            .iter()
            .filter(|q| role_of(&skin, q) == Some(ROLE_LEFT) && q.rect.x > top_bar.bar.x + top_bar.left_ornament.w)
            .collect();
        assert_eq!(left.len(), 1);
        assert_eq!(right.len(), 1);
        assert!(left[0].uv[0] < left[0].uv[2], "left end is unmirrored");
        assert!(
            (right[0].uv[0] - left[0].uv[2]).abs() < EPS
                && (right[0].uv[2] - left[0].uv[0]).abs() < EPS,
            "right end mirrors the left UVs"
        );
    }

    #[test]
    fn underline_closes_the_bar_and_never_enters_the_play_area() {
        let skin = sample_skin();
        let (top_bar, emitted) = draw_into(&skin, 1.0, SAFE);
        let play_area = Rect::new(
            SAFE.x,
            top_bar.bar.bottom(),
            SAFE.w,
            (SAFE.h - top_bar.bar.h).max(0.0),
        );
        assert!((top_bar.underline.h - UNDERLINE_HEIGHT).abs() < EPS);
        assert!(inside(top_bar.underline, top_bar.bar, EPS));
        assert!(
            top_bar.underline.bottom() <= top_bar.bar.bottom() + EPS,
            "underline must not hang below the bar"
        );
        assert!(
            !top_bar.underline.intersects(&play_area),
            "underline overlaps the play area"
        );

        let underline: Vec<_> = emitted
            .iter()
            .filter(|q| role_of(&skin, q) == Some(ROLE_UNDERLINE))
            .collect();
        assert_eq!(underline.len(), 1);
        let quad = underline[0];
        assert!(
            (quad.rect.y - top_bar.underline.y).abs() < EPS
                && (quad.rect.h - top_bar.underline.h).abs() < EPS
                && (quad.rect.w - top_bar.underline.w).abs() < EPS,
            "underline quad must cover the underline strip"
        );
        assert!(
            !quad.rect.intersects(&play_area),
            "underline quad enters the play area"
        );
        // The text blocks sit above the underline, not on it.
        assert!(top_bar.map_name.bottom() <= top_bar.underline.y + EPS);
        assert!(top_bar.fps.bottom() <= top_bar.underline.y + EPS);
    }

    #[test]
    fn text_rects_sit_between_the_ornaments() {
        let skin = sample_skin();
        let (top_bar, _) = draw_into(&skin, 1.0, SAFE);
        let pad = TEXT_PAD;
        assert!(
            top_bar.map_name.x >= top_bar.left_ornament.right() + pad - EPS,
            "map name runs under the left ornament"
        );
        assert!(
            top_bar.fps.right() <= top_bar.right_ornament.x - pad + EPS,
            "fps runs under the right ornament"
        );
        assert!(
            top_bar.map_name.right() <= top_bar.fps.x - TEXT_GAP + EPS,
            "map name overlaps the fps block"
        );
        assert!((top_bar.map_name.h - (BAR_HEIGHT - UNDERLINE_HEIGHT)).abs() < EPS);
        assert!((top_bar.fps.h - (BAR_HEIGHT - UNDERLINE_HEIGHT)).abs() < EPS);
        assert!(top_bar.map_name.w > 0.0 && top_bar.fps.w > 0.0);
    }

    #[test]
    fn text_rects_clamp_into_the_band() {
        let skin = sample_skin();
        let weird = layout(&skin, SAFE, 1.0, -5000.0, 999_999.0).unwrap();
        assert!(weird.map_name.x >= weird.left_ornament.right() - EPS);
        assert!(weird.fps.right() <= weird.right_ornament.x + EPS);
        assert!(weird.map_name.w >= 0.0 && weird.fps.w >= 0.0);
        assert!(inside(weird.map_name, weird.bar, EPS));
        assert!(inside(weird.fps, weird.bar, EPS));

        // Caller puts the FPS block left of the map name: no negative widths.
        let crossed = layout(&skin, SAFE, 1.0, SAFE.x + 900.0, SAFE.x + 200.0).unwrap();
        assert!(crossed.map_name.w >= 0.0);
        assert!(crossed.fps.w >= 0.0);
        assert!(inside(crossed.map_name, crossed.bar, EPS));
        assert!(inside(crossed.fps, crossed.bar, EPS));
    }

    #[test]
    fn real_atlas_insets_fit_the_fourteen_pixel_bar() {
        // The shipped sidecar (`analysis/h2/atlas/ui_atlas_insets.json`) values,
        // not the 12 px fixture: prove the measured art works at 14 px.
        let mut skin = sample_skin();
        let center = Skin::role_index(ROLE_CENTER).unwrap();
        let left = Skin::role_index(ROLE_LEFT).unwrap();
        let underline = Skin::role_index(ROLE_UNDERLINE).unwrap();
        let panel = Skin::role_index(ROLE_SMALL_PANEL).unwrap();
        skin.insets[center] = INSETS_TOPBAR_CENTER;
        skin.insets[left] = INSETS_TOPBAR_LEFT;
        skin.insets[underline] = INSETS_THIN_BAR;
        skin.insets[panel] = INSETS_SMALL_PANEL;

        let fitted = fitted_insets(INSETS_TOPBAR_CENTER, ART_TOPBAR_CENTER, BAR_HEIGHT);
        let ratio = BAR_HEIGHT / ART_TOPBAR_CENTER[1];
        assert!((fitted[0] - 55.0 * ratio).abs() < EPS, "left cell 5.92 px");
        assert!((fitted[1] - 36.0 * ratio).abs() < EPS, "top cell 3.88 px");
        assert!((fitted[2] - 55.0 * ratio).abs() < EPS, "right cell 5.92 px");
        assert!((fitted[3] - 55.0 * ratio).abs() < EPS, "bottom cell 5.92 px");
        assert!(
            fitted[1] + fitted[3] < BAR_HEIGHT,
            "measured insets must leave a stone band at 14 px"
        );

        let (top_bar, emitted) = draw_into(&skin, 1.0, SAFE);
        let centre_quads = emitted
            .iter()
            .filter(|q| role_of(&skin, q) == Some(ROLE_CENTER))
            .count();
        assert_eq!(
            centre_quads, 9,
            "real insets must keep the nine-patch (not the stretch fallback)"
        );
        let corner = emitted
            .iter()
            .find(|q| role_of(&skin, q) == Some(ROLE_CENTER) && is_corner(&skin, q))
            .expect("corner cell");
        assert!((corner.rect.w - fitted[0]).abs() < EPS);
        assert!((corner.rect.h - fitted[1]).abs() < EPS);
        for quad in &emitted {
            assert!(inside(quad.rect, top_bar.bar, EPS));
        }
        // Ornament width follows the art aspect, not the bar width.
        let expected = BAR_HEIGHT * (ART_TOPBAR_LEFT[0] / ART_TOPBAR_LEFT[1]);
        assert!((top_bar.left_ornament.w - expected).abs() < EPS);
        assert!((expected - 85.26).abs() < 0.01, "85.26 px at 1080p");
    }

    #[test]
    fn body_matches_the_shared_nine_patch_at_natural_height() {
        // At the art's own height the fitted cells equal the measured insets, so
        // the local slicer must reproduce `Skin::nine` quad for quad: the bar
        // uses the module's nine-patch, not a divergent reimplementation.
        let skin = sample_skin();
        let rect = Rect::new(50.0, 20.0, 400.0, ART_TOPBAR_CENTER[1]);
        let mut mine = UiBatch::default();
        let mut shared = UiBatch::default();
        assert!(draw_body(&skin, &mut mine, rect, [0.5; 4]));
        assert!(skin.nine(&mut shared, ROLE_CENTER, rect, [0.5; 4]));
        let a = quads(&mine);
        let b = quads(&shared);
        assert_eq!(a.len(), b.len(), "same cell count as Skin::nine");
        assert_eq!(a.len(), 9);
        for (mine, shared) in a.iter().zip(b.iter()) {
            assert!((mine.rect.x - shared.rect.x).abs() < EPS);
            assert!((mine.rect.y - shared.rect.y).abs() < EPS);
            assert!((mine.rect.w - shared.rect.w).abs() < EPS);
            assert!((mine.rect.h - shared.rect.h).abs() < EPS);
            for k in 0..4 {
                assert!((mine.uv[k] - shared.uv[k]).abs() < EPS, "uv {k} differs");
            }
        }
    }

    #[test]
    fn degenerate_inputs_do_not_emit_or_panic() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        assert!(
            draw(&skin, &mut batch, Rect::new(0.0, 0.0, 0.0, 500.0), 1.0, 0.0, 10.0, [1.0; 4])
                .is_none(),
            "a zero-width safe frame has no strip"
        );
        assert!(draw(&skin, &mut batch, SAFE, 0.0, 0.0, 10.0, [1.0; 4]).is_some());
        assert!(!batch.textured.is_empty(), "min scale still draws");

        // A skin whose insets cannot describe its art falls back to the
        // measured art values instead of a stretched smear.
        let mut broken = sample_skin();
        broken.insets[2] = [900.0, 900.0, 900.0, 900.0];
        let mut batch = UiBatch::default();
        assert!(draw(&broken, &mut batch, SAFE, 1.0, 140.0, 1400.0, [1.0; 4]).is_some());
        let fitted = fitted_insets(
            runtime_insets(&broken, Skin::role_index(ROLE_CENTER).unwrap()),
            ART_TOPBAR_CENTER,
            BAR_HEIGHT,
        );
        let expected = fitted_insets(INSETS_TOPBAR_CENTER, ART_TOPBAR_CENTER, BAR_HEIGHT);
        for i in 0..4 {
            assert!(
                (fitted[i] - expected[i]).abs() < EPS,
                "measured insets must be the fallback"
            );
        }
    }
}
