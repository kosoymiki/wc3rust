//! HP/mana bars and the stats rows frame.
//!
//! WC3 unit-info region, top to bottom:
//!
//! * HP bar — `hp_bar` nine-patch frame, a green `thin_bar` fill sitting in the
//!   frame's inner band, and the HP number drawn inside the bar;
//! * mana bar — the same frame one bar gap lower, filled blue;
//! * stats rows — ARMOR / DAMAGE / RANGE / SPEED, one 216x118 panel strip
//!   (`wide_panel` in the manifest, `small_panel` in the lane brief) per row
//!   with an alternating tint.
//!
//! Geometry contract (shared with the tests and `analysis/hudskin/stats.md`):
//!
//! * the frame is drawn with [`Skin::nine`], so the inner band is exactly the
//!   target rect minus the atlas insets of `hp_bar`; [`inner_rect`] mirrors
//!   that rule (and `Skin::nine`'s single-quad fallback) so the fill can never
//!   land on a corner;
//! * the fill starts at the left edge of the inner band and is
//!   `ratio.clamp(0, 1) * inner.w` wide, so `0` emits no fill quad at all and
//!   `1` covers the whole inner band;
//! * `scale` is the HUD art scale used for the text padding and the stats row
//!   height; the nine-patch and its inner band follow the skin's atlas insets
//!   exactly as [`Skin::nine`] lays them out, so fill and frame stay aligned at
//!   every scale.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region (HP and mana share the frame art).
pub const ROLE: &str = "hp_bar";

/// Flat fill strip stretched to the current ratio.
pub const FILL_ROLE: &str = "thin_bar";

/// One stats row strip: the 216x118 panel (`wide_panel` in the atlas
/// manifest; the lane brief calls the same element `small_panel`).
pub const ROW_ROLE: &str = "wide_panel";

/// Green HP fill.
pub const HP_FILL: [f32; 4] = [0.22, 0.75, 0.24, 1.0];

/// Blue mana fill.
pub const MANA_FILL: [f32; 4] = [0.25, 0.45, 0.95, 1.0];

/// Frame tint for the mana bar (no caller tint in the signature).
pub const MANA_FRAME_TINT: [f32; 4] = [1.0; 4];

/// Alternating stats row tints: dark and slightly lighter, both translucent.
pub const ROW_TINTS: [[f32; 4]; 2] = [
    [0.10, 0.09, 0.07, 0.32],
    [0.18, 0.16, 0.12, 0.20],
];

/// Row height in atlas pixels at `scale == 1.0`.
pub const ROW_HEIGHT: f32 = 18.0;

/// Draw the region into `rect` (primary role only; kept for the stub contract).
pub fn draw(skin: &Skin, batch: &mut UiBatch, rect: Rect, tint: [f32; 4]) -> bool {
    skin.nine(batch, ROLE, rect, tint)
}

/// The frame's inner band for `rect`: the target inset by the `hp_bar` atlas
/// insets, mirroring [`Skin::nine`].
///
/// When the target is too small for a nine-patch, `Skin::nine` stretches the
/// whole element into one quad; the trough is then the whole target minus a
/// 1 px rim so the stretched frame edge stays visible. A skin without the
/// `hp_bar` role has no frame, so `rect` itself is usable as the trough.
pub fn inner_rect(skin: &Skin, rect: Rect) -> Rect {
    let rect = usable(rect);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Rect::new(rect.x, rect.y, 0.0, 0.0);
    }
    let Some(index) = Skin::role_index(ROLE) else {
        return rect;
    };
    let [l, t, r, b] = skin.insets[index];
    let (l, t, r, b) = (l.max(0.0), t.max(0.0), r.max(0.0), b.max(0.0));
    if rect.w <= l + r + 1.0 || rect.h <= t + b + 1.0 {
        return rect.inset(1.0, 1.0);
    }
    rect.inset_sides(l, t, r, b)
}

/// Draw the HP bar: `hp_bar` nine-patch frame in `tint`, green `thin_bar` fill
/// at `ratio`, and return the rect the HP number goes into (inside the fill).
pub fn draw_hp(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    ratio: f32,
    scale: f32,
    tint: [f32; 4],
) -> Rect {
    let scale = sane_scale(scale);
    let rect = usable(rect);
    let inner = inner_rect(skin, rect);
    skin.nine(batch, ROLE, rect, tint);
    fill_inner(skin, batch, inner, ratio, HP_FILL);
    number_rect(inner, scale)
}

/// Draw the mana bar under the HP bar: same frame, blue fill, and return the
/// rect the mana number goes into.
pub fn draw_mana(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    ratio: f32,
    scale: f32,
) -> Rect {
    let scale = sane_scale(scale);
    let rect = usable(rect);
    let inner = inner_rect(skin, rect);
    skin.nine(batch, ROLE, rect, MANA_FRAME_TINT);
    fill_inner(skin, batch, inner, ratio, MANA_FILL);
    number_rect(inner, scale)
}

/// Draw one alternating panel strip per stat row and return the text rect for
/// each row, top to bottom.
///
/// Rows never overlap and always stay inside `rect`; when the rows would not
/// fit at the scaled [`ROW_HEIGHT`], the row height shrinks to `rect.h / n`
/// (the block never spills out of the caller's slot).
pub fn draw_stats(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    rows: &[(&str, &str)],
    scale: f32,
) -> Vec<Rect> {
    let scale = sane_scale(scale);
    let rect = usable(rect);
    if rows.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
        return Vec::new();
    }
    let count = rows.len() as f32;
    let row_h = (ROW_HEIGHT * scale).min(rect.h / count);
    if row_h <= 0.0 {
        return Vec::new();
    }
    let pad_x = (4.0 * scale).min(rect.w * 0.25);
    let mut text = Vec::with_capacity(rows.len());
    for (index, _row) in rows.iter().enumerate() {
        let row = Rect::new(rect.x, rect.y + row_h * index as f32, rect.w, row_h);
        skin.nine(batch, ROW_ROLE, row, ROW_TINTS[index % 2]);
        let pad_y = (1.0 * scale).min(row.h * 0.25);
        text.push(row.inset_sides(pad_x, pad_y, pad_x, pad_y));
    }
    text
}

/// Emit the stretched fill quad, or nothing for an empty/out-of-range ratio.
fn fill_inner(skin: &Skin, batch: &mut UiBatch, inner: Rect, ratio: f32, color: [f32; 4]) {
    let ratio = if ratio.is_finite() { ratio.clamp(0.0, 1.0) } else { 0.0 };
    if ratio <= 0.0 || inner.w <= 0.0 || inner.h <= 0.0 {
        return;
    }
    let width = (inner.w * ratio).min(inner.w);
    if width <= 0.0 {
        return;
    }
    skin.stretch(batch, FILL_ROLE, Rect::new(inner.x, inner.y, width, inner.h), color);
}

/// The rect text is centred in: the fill area minus a small scaled padding.
fn number_rect(inner: Rect, scale: f32) -> Rect {
    let pad_x = (2.0 * scale).min(inner.w * 0.25);
    let pad_y = (1.0 * scale).min(inner.h * 0.25);
    inner.inset_sides(pad_x, pad_y, pad_x, pad_y)
}

/// `scale` is a multiplier; reject zero/NaN/infinite so geometry stays finite.
fn sane_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Clamp a target rect to non-negative, finite extents.
fn usable(rect: Rect) -> Rect {
    if !rect.x.is_finite() || !rect.y.is_finite() || !rect.w.is_finite() || !rect.h.is_finite() {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }
    Rect::new(rect.x, rect.y, rect.w.max(0.0), rect.h.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    /// One decoded `textured_quad` (six vertices -> rect + UVs + tint).
    #[derive(Clone, Copy, Debug)]
    struct Quad {
        rect: Rect,
        uv: [f32; 4],
        color: [f32; 4],
    }

    fn quads(batch: &UiBatch) -> Vec<Quad> {
        let mut out = Vec::new();
        for draw in &batch.textured {
            for vertex in draw.vertices.chunks_exact(6) {
                let x0 = vertex[0].pos[0];
                let y0 = vertex[0].pos[1];
                let x1 = vertex[2].pos[0];
                let y1 = vertex[2].pos[1];
                out.push(Quad {
                    rect: Rect::new(x0, y0, x1 - x0, y1 - y0),
                    uv: [vertex[0].uv[0], vertex[0].uv[1], vertex[2].uv[0], vertex[2].uv[1]],
                    color: vertex[0].color,
                });
            }
        }
        out
    }

    fn role_uv(skin: &Skin, role: &str) -> [f32; 4] {
        let index = Skin::role_index(role).expect("role exists");
        let [ax, ay, aw, ah] = skin.rects[index];
        [
            ax / skin.width,
            ay / skin.height,
            (ax + aw) / skin.width,
            (ay + ah) / skin.height,
        ]
    }

    fn inside(outer: Rect, inner: Rect) -> bool {
        outer.contains_rect(&inner)
    }

    fn uv_close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-6)
    }

    /// Vertex colours are linear (`hud::linear_color`), the fill constants are
    /// authored in sRGB: classify a quad by its authored colour, never by the
    /// raw vertex bytes.
    fn is_authored(linear: [f32; 4], authored: [f32; 4]) -> bool {
        crate::hud::authored_color(linear)
            .iter()
            .zip(authored.iter())
            .all(|(actual, expected)| (actual - expected).abs() <= 1e-6)
    }

    fn is_hp_fill(quad: &Quad) -> bool {
        is_authored(quad.color, HP_FILL)
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

    #[test]
    fn fill_width_tracks_ratio() {
        let skin = sample_skin();
        let rect = Rect::new(100.0, 50.0, 400.0, 120.0);
        let inner = inner_rect(&skin, rect);
        println!("hp_bar rect={rect:?} inner={inner:?}");
        assert!(inner.w > 0.0 && inner.h > 0.0, "no trough: {inner:?}");

        for ratio in [0.0_f32, 0.5, 1.0] {
            let mut batch = UiBatch::default();
            let number = draw_hp(&skin, &mut batch, rect, ratio, 1.0, [1.0; 4]);
            let fills: Vec<Quad> = quads(&batch)
                .into_iter()
                .filter(is_hp_fill)
                .collect();
            println!(
                "ratio={ratio}: fills={} widths={:?}",
                fills.len(),
                fills.iter().map(|f| f.rect.w).collect::<Vec<_>>()
            );

            if ratio == 0.0 {
                assert!(fills.is_empty(), "ratio 0 must not draw a fill");
            } else {
                assert_eq!(fills.len(), 1, "exactly one fill quad");
                let fill = fills[0].rect;
                assert!(
                    (fill.w - inner.w * ratio).abs() <= 1.0,
                    "fill {:.2}px != ratio {ratio} * inner {:.2}px",
                    fill.w,
                    inner.w
                );
                assert!(
                    fill.x >= inner.x - 0.01 && fill.right() <= inner.right() + 0.01,
                    "fill x outside the trough"
                );
                assert!(
                    fill.y >= inner.y - 0.01 && fill.bottom() <= inner.bottom() + 0.01,
                    "fill y outside the trough"
                );
            }

            // The number sits inside both the fill area and the frame.
            assert!(inside(rect, number), "number rect outside the frame");
            assert!(inside(inner, number), "number rect outside the trough");
        }
    }

    /// `ui.colour.stats-fill` — the HP fill is the authored [`HP_FILL`] colour
    /// written as a linear vertex colour, it tracks the ratio on the inner band
    /// (no byte, no hardcoded pixel width), and the frame quads carry the frame
    /// tint instead. Mana is the same contract with [`MANA_FILL`].
    #[test]
    fn ui_colour_stats_fill_is_authored_and_tracks_the_ratio() {
        let skin = sample_skin();
        let rect = Rect::new(60.0, 200.0, 320.0, 96.0);
        let inner = inner_rect(&skin, rect);
        let tint = [0.9, 0.85, 0.8, 1.0];
        for ratio in [0.25_f32, 0.5, 0.75, 1.0] {
            let mut batch = UiBatch::default();
            draw_hp(&skin, &mut batch, rect, ratio, 1.0, tint);
            let all = quads(&batch);
            let fills: Vec<&Quad> = all.iter().filter(|quad| is_hp_fill(quad)).collect();
            assert_eq!(fills.len(), 1, "ratio {ratio}: one HP fill");
            let fill = fills[0];
            for channel in 0..3 {
                assert!(
                    (fill.color[channel] - crate::hud::srgb_to_linear(HP_FILL[channel])).abs() < 1e-6,
                    "ratio {ratio}: fill channel {channel} is not linear HP_FILL"
                );
            }
            assert_eq!(fill.color[3], 1.0, "the fill is opaque");
            assert!(
                (fill.rect.w - inner.w * ratio).abs() <= 1.0,
                "ratio {ratio}: fill {} != inner {} * {ratio}",
                fill.rect.w,
                inner.w
            );
            // The frame is everything that is not the fill and carries the tint.
            let frame: Vec<&Quad> = all.iter().filter(|quad| !is_hp_fill(quad)).collect();
            assert_eq!(frame.len(), 9, "ratio {ratio}: a nine-patch frame");
            for piece in frame {
                assert!(
                    is_authored(piece.color, tint),
                    "ratio {ratio}: frame piece is tinted {tint:?}, got {:?}",
                    piece.color
                );
            }
        }

        // Mana: a blue authored fill on the same trough.
        let mut batch = UiBatch::default();
        let mana_rect = Rect::new(60.0, 320.0, 320.0, 96.0);
        draw_mana(&skin, &mut batch, mana_rect, 0.5, 1.0);
        let mana: Vec<Quad> = quads(&batch)
            .into_iter()
            .filter(|quad| is_authored(quad.color, MANA_FILL))
            .collect();
        assert_eq!(mana.len(), 1, "one mana fill");
        assert!((mana[0].rect.w - inner_rect(&skin, mana_rect).w * 0.5).abs() <= 1.0);
    }

    #[test]
    fn ratio_one_fills_the_whole_inner_rect() {
        let skin = sample_skin();
        let rect = Rect::new(20.0, 640.0, 360.0, 88.0);
        let inner = inner_rect(&skin, rect);
        let mut batch = UiBatch::default();
        draw_hp(&skin, &mut batch, rect, 1.0, 1.0, [1.0; 4]);
        let fills: Vec<Quad> = quads(&batch)
            .into_iter()
            .filter(is_hp_fill)
            .collect();
        assert_eq!(fills.len(), 1);
        let fill = fills[0].rect;
        assert!((fill.x - inner.x).abs() <= 0.01, "fill left edge");
        assert!((fill.y - inner.y).abs() <= 0.01, "fill top edge");
        assert!((fill.w - inner.w).abs() <= 1.0, "fill width");
        assert!((fill.h - inner.h).abs() <= 1.0, "fill height");
    }

    #[test]
    fn fill_never_exceeds_the_frame() {
        let skin = sample_skin();
        let frame_tint = [0.9, 0.8, 0.7, 1.0];
        for rect in [
            Rect::new(10.0, 20.0, 400.0, 120.0),
            Rect::new(10.0, 20.0, 240.0, 22.0), // retail bar height: nine-patch fallback
            Rect::new(10.0, 20.0, 30.0, 18.0),  // smaller than the insets
            Rect::new(10.0, 20.0, 1.0, 1.0),
        ] {
            for ratio in [0.0_f32, 0.25, 0.5, 1.0] {
                for scale in [1.0_f32, 2.0] {
                    let mut batch = UiBatch::default();
                    draw_hp(&skin, &mut batch, rect, ratio, scale, frame_tint);
                    let inner = inner_rect(&skin, rect);
                    for quad in quads(&batch) {
                        assert!(inside(rect, quad.rect), "{rect:?} quad {:?} escapes frame", quad.rect);
                        if is_hp_fill(&quad) {
                            assert!(
                                inside(inner, quad.rect),
                                "{rect:?} fill {:?} escapes trough {inner:?}",
                                quad.rect
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn frame_corners_keep_their_atlas_size() {
        let skin = sample_skin();
        let rect = Rect::new(40.0, 80.0, 500.0, 90.0);
        let mut batch = UiBatch::default();
        draw_hp(&skin, &mut batch, rect, 0.25, 1.0, [1.0; 4]);
        let frame: Vec<Quad> = quads(&batch)
            .into_iter()
            .filter(|quad| !is_hp_fill(quad))
            .collect();
        assert_eq!(frame.len(), 9, "a full nine-patch must emit nine pieces");
        let hp = role_uv(&skin, ROLE);
        let [l, t, r, b] = skin.insets[Skin::role_index(ROLE).unwrap()];

        let top_left = frame
            .iter()
            .find(|q| uv_close([q.uv[0], q.uv[1], q.uv[0], q.uv[1]], [hp[0], hp[1], hp[0], hp[1]]))
            .expect("top-left piece");
        assert!((top_left.rect.x - rect.x).abs() <= 0.01);
        assert!((top_left.rect.y - rect.y).abs() <= 0.01);
        assert!((top_left.rect.w - l).abs() <= 0.01, "corner width changed");
        assert!((top_left.rect.h - t).abs() <= 0.01, "corner height changed");

        let top_right = frame
            .iter()
            .find(|q| uv_close([q.uv[2], q.uv[1], q.uv[2], q.uv[1]], [hp[2], hp[1], hp[2], hp[1]]))
            .expect("top-right piece");
        assert!((top_right.rect.right() - rect.right()).abs() <= 0.01);
        assert!((top_right.rect.w - r).abs() <= 0.01, "corner width changed");
        assert!((top_right.rect.h - t).abs() <= 0.01, "corner height changed");

        let bottom_left = frame
            .iter()
            .find(|q| uv_close([q.uv[0], q.uv[3], q.uv[0], q.uv[3]], [hp[0], hp[3], hp[0], hp[3]]))
            .expect("bottom-left piece");
        assert!((bottom_left.rect.bottom() - rect.bottom()).abs() <= 0.01);
        assert!((bottom_left.rect.w - l).abs() <= 0.01);
        assert!((bottom_left.rect.h - b).abs() <= 0.01, "corner height changed");
    }

    #[test]
    fn mana_bar_sits_strictly_under_hp() {
        let skin = sample_skin();
        let hp_rect = Rect::new(60.0, 120.0, 320.0, 96.0);
        let mana_rect = Rect::new(hp_rect.x, hp_rect.bottom() + 2.0, hp_rect.w, 48.0);
        let mut batch = UiBatch::default();
        let hp_number = draw_hp(&skin, &mut batch, hp_rect, 0.75, 1.0, [1.0; 4]);
        let after_hp = quads(&batch).len();
        let mana_number = draw_mana(&skin, &mut batch, mana_rect, 0.5, 1.0);
        let all = quads(&batch);

        assert!(after_hp > 0, "HP bar emitted nothing");
        assert!(all.len() > after_hp, "mana bar emitted nothing");
        for quad in &all[after_hp..] {
            assert!(
                quad.rect.y >= hp_rect.bottom() - 0.01,
                "mana quad {:?} is not under the HP frame",
                quad.rect
            );
        }
        assert!(mana_rect.y > hp_number.bottom(), "mana starts inside the HP bar");
        assert!(inside(mana_rect, mana_number), "mana number outside its bar");

        let mana_fills: Vec<Quad> = all
            .iter()
            .copied()
            .filter(|quad| is_authored(quad.color, MANA_FILL))
            .collect();
        assert_eq!(mana_fills.len(), 1, "one mana fill");
        let fill = mana_fills[0].rect;
        let inner = inner_rect(&skin, mana_rect);
        assert!((fill.w - inner.w * 0.5).abs() <= 1.0);
        assert!(inside(inner, fill));
    }

    #[test]
    fn stats_rows_stay_inside_and_do_not_overlap() {
        let skin = sample_skin();
        let rect = Rect::new(80.0, 300.0, 300.0, 150.0);
        let rows = [
            ("ARMOR", "5"),
            ("DAMAGE", "12-14"),
            ("RANGE", "300"),
            ("SPEED", "320"),
        ];
        let mut batch = UiBatch::default();
        let text = draw_stats(&skin, &mut batch, rect, &rows, 1.0);
        assert_eq!(text.len(), rows.len(), "one text rect per row");

        for (index, row) in text.iter().enumerate() {
            assert!(inside(rect, *row), "row {index} escapes the stats slot");
            for (other_index, other) in text.iter().enumerate().skip(index + 1) {
                assert!(
                    !row.intersects(other),
                    "rows {index} and {other_index} overlap"
                );
            }
            if index > 0 {
                assert!(
                    row.y >= text[index - 1].bottom() - 0.01,
                    "row {index} is above row {}",
                    index - 1
                );
            }
        }

        let row_uv = role_uv(&skin, ROW_ROLE);
        let panels: Vec<Quad> = quads(&batch)
            .into_iter()
            .filter(|quad| {
                quad.uv[0] >= row_uv[0] - 1e-6
                    && quad.uv[1] >= row_uv[1] - 1e-6
                    && quad.uv[2] <= row_uv[2] + 1e-6
                    && quad.uv[3] <= row_uv[3] + 1e-6
            })
            .collect();
        assert!(panels.len() >= rows.len(), "every row needs a panel strip");
        for panel in &panels {
            assert!(inside(rect, panel.rect), "panel strip escapes the slot");
        }
        // Alternation: rows 0/2 and 1/3 share a tint, neighbours differ.
        assert!(
            is_authored(panels[0].color, ROW_TINTS[0]),
            "row 0 tint is not the authored {:?}: {:?}",
            ROW_TINTS[0],
            panels[0].color
        );
        assert!(
            is_authored(panels[1].color, ROW_TINTS[1]),
            "row 1 tint is not the authored {:?}: {:?}",
            ROW_TINTS[1],
            panels[1].color
        );
        assert_ne!(ROW_TINTS[0], ROW_TINTS[1]);
    }

    #[test]
    fn stats_empty_rows_draw_nothing() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let text = draw_stats(&skin, &mut batch, Rect::new(0.0, 0.0, 200.0, 100.0), &[], 1.0);
        assert!(text.is_empty());
        assert!(quads(&batch).is_empty());
    }
}
