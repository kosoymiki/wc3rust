//! Selection portrait: the ornamented frame, the unit-name band and the class line.
//!
//! Retail layout (WC3 TFT): the selection column carries the unit name above the
//! portrait, the class line under the name, and the framed 128x128 portrait
//! image at the bottom of the column. This module owns that trio:
//!
//! * the frame is the `portrait_frame` atlas role drawn as a nine-patch, fitted
//!   to the role's authored 198:217 aspect so the ornament never stretches;
//! * the portrait image goes exactly into the nine-patch centre cell (the
//!   reported `portrait_rect`), the frame's own dark window;
//! * the name band (`0.022 H`) and the class band (`0.019 H`) stack above the
//!   frame, bottom-up, so neither can ever cover the portrait;
//! * the frame tint is multiplied by the unit kind: warm gold for heroes, a red
//!   rim for enemy units, the caller's tint unchanged for everything else.
//!
//! Measured geometry (observed on the shipped slice
//! `analysis/h2/atlas/elements/06_portrait_frame_198x217.png`): the interior
//! fill occupies columns 34..=162 and rows 49..=182, i.e. a 129x134 opening with
//! a 34/49/35/34 border (left/top/right/bottom). The nine-patch borders this
//! module partitions with are `Skin::insets` instead (sidecar
//! `analysis/h2/atlas/ui_atlas_insets.json`, role `frame_medium` =
//! `[49, 65, 50, 48]`): those are the ornament's stretch boundaries, and the
//! centre cell they define is the window the portrait belongs in. With the
//! shipped atlas the frame needs at least `100 x 114` px of budget (49+50+1,
//! 65+48+1) before a real centre cell exists; below that `Skin::nine` falls back
//! to one stretched quad and the reported portrait rect is clamped to fit.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region.
pub const ROLE: &str = "portrait_frame";

/// Authored frame size in atlas pixels (`analysis/h2/atlas/slices.json`).
pub const AUTHORED_SIZE: [f32; 2] = [198.0, 217.0];
/// Portrait opening measured on the shipped slice, in atlas pixels.
pub const OPENING_SIZE: [f32; 2] = [129.0, 134.0];
/// Border around the opening measured on the same slice.
pub const MEASURED_BORDER: [f32; 4] = [34.0, 49.0, 35.0, 34.0];
/// Authored frame aspect ratio (width / height).
pub const FRAME_ASPECT: f32 = AUTHORED_SIZE[0] / AUTHORED_SIZE[1];

/// Reference screen height every band fraction below is quoted against.
pub const REFERENCE_HEIGHT: f32 = 1080.0;
/// Unit-name band height as a fraction of the screen height.
pub const NAME_BAND_FRACTION: f32 = 0.022;
/// Class-line band height as a fraction of the screen height (retail keeps the
/// class line one text size below the name).
pub const CLASS_BAND_FRACTION: f32 = 0.019;
/// Vertical gap between the two bands and the frame top, as a fraction of the
/// screen height.
pub const BAND_GAP_FRACTION: f32 = 0.004;
/// Band width at the reference scale: the retail name row runs wider than the
/// frame (`MobileHudLayout::name` uses the same 380-unit width).
pub const BAND_WIDTH_UNITS: f32 = 380.0;

/// Portrait frame variant selected by the unit kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PortraitFrameKind {
    #[default]
    Normal,
    Hero,
    Enemy,
}

impl PortraitFrameKind {
    /// Multiplier applied to the caller's tint. `Normal` is a pass-through,
    /// `Hero` keeps red and drops blue (gold), `Enemy` drops green and blue and
    /// darkens slightly (red rim).
    pub fn tint_mul(self) -> [f32; 4] {
        match self {
            Self::Normal => [1.0, 1.0, 1.0, 1.0],
            Self::Hero => [1.0, 0.88, 0.58, 1.0],
            Self::Enemy => [0.92, 0.60, 0.58, 1.0],
        }
    }
}

/// Screen rects produced by [`draw`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PortraitParts {
    /// Frame window: draw the unit portrait here.
    pub portrait_rect: Rect,
    /// Unit-name band, above the frame.
    pub name_rect: Rect,
    /// Class line, between the name band and the frame.
    pub class_rect: Rect,
}

/// Draw the frame and report the three rects of the selection portrait column.
///
/// `rect` is the budget the frame may occupy: the nine-patch is fitted to
/// [`FRAME_ASPECT`] and centred inside it. `scale` is the HUD scale (1.0 at the
/// 1080 px reference height) and only sizes the text bands; the frame's corner
/// ornaments keep their atlas pixel size because `Skin::nine` partitions fixed
/// insets.
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    scale: f32,
    frame: PortraitFrameKind,
    tint: [f32; 4],
) -> PortraitParts {
    let frame_rect = fit_aspect(rect, FRAME_ASPECT);
    let mul = frame.tint_mul();
    let frame_tint = [
        tint[0] * mul[0],
        tint[1] * mul[1],
        tint[2] * mul[2],
        tint[3] * mul[3],
    ];
    skin.nine(batch, ROLE, frame_rect, frame_tint);

    let border = border_for(skin, frame_rect);
    let portrait_rect = frame_rect.inset_sides(border[0], border[1], border[2], border[3]);

    let s = scale.max(0.01);
    let gap = BAND_GAP_FRACTION * REFERENCE_HEIGHT * s;
    let name_h = NAME_BAND_FRACTION * REFERENCE_HEIGHT * s;
    let class_h = CLASS_BAND_FRACTION * REFERENCE_HEIGHT * s;
    let band_w = frame_rect.w.max(BAND_WIDTH_UNITS * s);
    // Bottom-up from the frame: class line, then the name band above it.
    let class_rect = Rect::new(frame_rect.x, frame_rect.y - gap - class_h, band_w, class_h);
    let name_rect = Rect::new(frame_rect.x, class_rect.y - gap - name_h, band_w, name_h);

    PortraitParts {
        portrait_rect,
        name_rect,
        class_rect,
    }
}

/// Frame box that hosts a portrait image of `portrait` size at the authored
/// opening ratio, centred on the image box.
///
/// The layout hands out the portrait image box (128x128 at scale 1), not the
/// ornament overhang, so call sites that only know the image box use this to get
/// the budget [`draw`] expects.
pub fn frame_rect_for(portrait: Rect) -> Rect {
    let sx = portrait.w / OPENING_SIZE[0];
    let sy = portrait.h / OPENING_SIZE[1];
    let s = sx.min(sy).max(0.0);
    let w = AUTHORED_SIZE[0] * s;
    let h = AUTHORED_SIZE[1] * s;
    Rect::new(
        portrait.x + (portrait.w - w) * 0.5,
        portrait.y + (portrait.h - h) * 0.5,
        w,
        h,
    )
}

/// Largest rect of `aspect` that fits inside `budget`, centred.
fn fit_aspect(budget: Rect, aspect: f32) -> Rect {
    if budget.w <= 0.0 || budget.h <= 0.0 || aspect <= 0.0 {
        return Rect::new(budget.x, budget.y, 0.0, 0.0);
    }
    let by_height = budget.h * aspect;
    let (w, h) = if by_height <= budget.w {
        (by_height, budget.h)
    } else {
        (budget.w, budget.w / aspect)
    };
    Rect::new(
        budget.x + (budget.w - w) * 0.5,
        budget.y + (budget.h - h) * 0.5,
        w,
        h,
    )
}

/// Nine-patch border actually available inside `frame`: the role insets, but at
/// least 1 px per side and never more than the frame can give while keeping a
/// (fractionally) positive window. Matches the centre cell `Skin::nine` draws
/// whenever the frame is large enough.
fn border_for(skin: &Skin, frame: Rect) -> [f32; 4] {
    let raw = Skin::role_index(ROLE)
        .map(|index| skin.insets[index])
        .unwrap_or(MEASURED_BORDER);
    let mut border = [
        raw[0].max(1.0),
        raw[1].max(1.0),
        raw[2].max(1.0),
        raw[3].max(1.0),
    ];
    let avail_w = (frame.w - 1.0).max(0.0);
    let sum_w = border[0] + border[2];
    if sum_w > avail_w {
        let k = if sum_w > 0.0 { avail_w / sum_w } else { 0.0 };
        border[0] *= k;
        border[2] *= k;
    }
    let avail_h = (frame.h - 1.0).max(0.0);
    let sum_h = border[1] + border[3];
    if sum_h > avail_h {
        let k = if sum_h > 0.0 { avail_h / sum_h } else { 0.0 };
        border[1] *= k;
        border[3] *= k;
    }
    border
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::mobile_hud::{HudView, MobileHud};
    use crate::hudskin::tests_support::sample_skin;

    fn drawn_bounds(batch: &UiBatch) -> Rect {
        let mut min = glam::Vec2::splat(f32::MAX);
        let mut max = glam::Vec2::splat(f32::MIN);
        for draw in &batch.textured {
            for vertex in &draw.vertices {
                min = min.min(glam::Vec2::from(vertex.pos));
                max = max.max(glam::Vec2::from(vertex.pos));
            }
        }
        Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 0.01
    }

    fn assert_quads_inside(batch: &UiBatch, rect: Rect) {
        assert!(!batch.textured.is_empty(), "no quads emitted");
        for draw in &batch.textured {
            for vertex in &draw.vertices {
                assert!(
                    vertex.pos[0] >= rect.x - 0.01 && vertex.pos[0] <= rect.right() + 0.01,
                    "x outside the frame: {}",
                    vertex.pos[0]
                );
                assert!(
                    vertex.pos[1] >= rect.y - 0.01 && vertex.pos[1] <= rect.bottom() + 0.01,
                    "y outside the frame: {}",
                    vertex.pos[1]
                );
                assert!(vertex.uv[0] >= 0.0 && vertex.uv[0] <= 1.0);
                assert!(vertex.uv[1] >= 0.0 && vertex.uv[1] <= 1.0);
            }
        }
    }

    #[test]
    fn inner_rect_strictly_inside_the_frame() {
        let skin = sample_skin();
        let [l, t, r, b] = skin.insets[Skin::role_index(ROLE).expect("portrait_frame role")];
        for (w, h) in [
            (128.0, 128.0),
            (198.0, 217.0),
            (400.0, 120.0),
            (37.0, 211.0),
        ] {
            let mut batch = UiBatch::default();
            let budget = Rect::new(100.0, 50.0, w, h);
            let parts = draw(
                &skin,
                &mut batch,
                budget,
                1.0,
                PortraitFrameKind::Normal,
                [1.0; 4],
            );
            let frame = drawn_bounds(&batch);
            let inner = parts.portrait_rect;
            assert!(inner.x > frame.x, "{w}x{h}: left edge not inside");
            assert!(inner.y > frame.y, "{w}x{h}: top edge not inside");
            assert!(
                inner.right() < frame.right(),
                "{w}x{h}: right edge not inside"
            );
            assert!(
                inner.bottom() < frame.bottom(),
                "{w}x{h}: bottom edge not inside"
            );
            assert!(frame.is_inside(&budget), "{w}x{h}: frame left the budget");
            assert_quads_inside(&batch, budget);
            // The reported window is the nine-patch centre cell itself.
            assert!(approx(inner.x - frame.x, l) && approx(frame.right() - inner.right(), r));
            assert!(approx(inner.y - frame.y, t) && approx(frame.bottom() - inner.bottom(), b));
        }
    }

    #[test]
    fn name_and_class_bands_do_not_overlap_the_portrait() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let budget = Rect::new(240.0, 300.0, 200.0, 220.0);
        let parts = draw(
            &skin,
            &mut batch,
            budget,
            1.0,
            PortraitFrameKind::Hero,
            [1.0; 4],
        );
        // Bands are above the frame: bottom edge of the class band ends at the
        // frame top minus the gap, the name band sits above the class band.
        assert!(
            parts.class_rect.bottom() < budget.y + 0.01,
            "class band reached the frame"
        );
        assert!(
            parts.name_rect.bottom() < parts.class_rect.y,
            "name band overlaps the class band"
        );
        assert!(
            !parts.name_rect.intersects(&parts.portrait_rect),
            "name band overlaps the portrait"
        );
        assert!(
            !parts.class_rect.intersects(&parts.portrait_rect),
            "class band overlaps the portrait"
        );
        // Retail fractions: 0.022 H name, 0.019 H class.
        assert!(approx(parts.name_rect.h, 0.022 * REFERENCE_HEIGHT * 1.0));
        assert!(approx(parts.class_rect.h, 0.019 * REFERENCE_HEIGHT * 1.0));
        // Bands are at least as wide as the frame they annotate.
        let frame = drawn_bounds(&batch);
        assert!(parts.name_rect.w >= frame.w - 0.01);
        assert!(parts.class_rect.w >= frame.w - 0.01);
        assert_quads_inside(&batch, budget);
    }

    #[test]
    fn frame_keeps_aspect() {
        let skin = sample_skin();
        // A wide budget: the ornament must not stretch to fill it.
        let mut batch = UiBatch::default();
        let budget = Rect::new(10.0, 20.0, 500.0, 120.0);
        draw(
            &skin,
            &mut batch,
            budget,
            1.0,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        let frame = drawn_bounds(&batch);
        assert!(
            approx(frame.w / frame.h, FRAME_ASPECT),
            "aspect drifted: {}",
            frame.w / frame.h
        );
        // Fitted by height and centred horizontally.
        assert!(approx(frame.h, budget.h));
        assert!(approx(frame.x + frame.w * 0.5, budget.centre().x));
        // A tall budget: fitted by width instead.
        let mut batch = UiBatch::default();
        let budget = Rect::new(0.0, 0.0, 100.0, 500.0);
        draw(
            &skin,
            &mut batch,
            budget,
            1.0,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        let frame = drawn_bounds(&batch);
        assert!(approx(frame.w / frame.h, FRAME_ASPECT));
        assert!(approx(frame.w, budget.w));
        // The portrait image box maps onto a frame with the same ratio.
        let portrait = frame_rect_for(budget);
        assert!(approx(portrait.w / portrait.h, FRAME_ASPECT));
    }

    #[test]
    fn corner_identity_1x_vs_2x() {
        let skin = sample_skin();
        let [l, t, _, _] = skin.insets[Skin::role_index(ROLE).expect("role")];
        let one = Rect::new(10.0, 10.0, 198.0, 217.0);
        let two = Rect::new(20.0, 20.0, 396.0, 434.0);
        let mut batch_one = UiBatch::default();
        let mut batch_two = UiBatch::default();
        draw(
            &skin,
            &mut batch_one,
            one,
            1.0,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        draw(
            &skin,
            &mut batch_two,
            two,
            2.0,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        let first_one = &batch_one.textured[0];
        let first_two = &batch_two.textured[0];
        // Top-left corner: same atlas texels, same screen size at both scales
        // (a nine-patch corner never stretches). The first six vertices are the
        // top-left cell; `textured_quad` appends the remaining cells to the same
        // draw.
        for (quad, label) in [(first_one, "1x"), (first_two, "2x")] {
            let positions: Vec<[f32; 2]> = quad.vertices[0..6].iter().map(|v| v.pos).collect();
            let xs = positions.iter().map(|p| p[0]);
            let ys = positions.iter().map(|p| p[1]);
            let w = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
            let h = ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min);
            assert!(approx(w, l), "{label}: corner width {}", w);
            assert!(approx(h, t), "{label}: corner height {}", h);
        }
        let uv_one: Vec<[f32; 2]> = first_one.vertices[0..6].iter().map(|v| v.uv).collect();
        let uv_two: Vec<[f32; 2]> = first_two.vertices[0..6].iter().map(|v| v.uv).collect();
        assert_eq!(uv_one, uv_two, "corner sampled different texels");
        // The opposite corner keeps its own texels.
        let tail = first_one.vertices.len() - 6;
        let uv_br: Vec<[f32; 2]> = first_one.vertices[tail..].iter().map(|v| v.uv).collect();
        assert_ne!(uv_one, uv_br);
    }

    #[test]
    fn hero_and_enemy_tints_differ() {
        let skin = sample_skin();
        let base = [1.0, 1.0, 1.0, 1.0];
        let samples: Vec<[f32; 4]> = [
            PortraitFrameKind::Normal,
            PortraitFrameKind::Hero,
            PortraitFrameKind::Enemy,
        ]
        .into_iter()
        .map(|kind| {
            let mut batch = UiBatch::default();
            draw(
                &skin,
                &mut batch,
                Rect::new(0.0, 0.0, 198.0, 217.0),
                1.0,
                kind,
                base,
            );
            batch.textured[0].vertices[0].color
        })
        .collect();
        let (normal, hero, enemy) = (samples[0], samples[1], samples[2]);
        assert_eq!(normal, base, "the normal frame must pass the tint through");
        // Gold: red survives, blue drops.
        assert!(hero[0] > hero[1] && hero[1] > hero[2]);
        // Enemy rim: less green than the gold frame and darker overall.
        assert!(enemy[1] < hero[1] - 0.1, "enemy green must drop below hero");
        let sum = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(sum(enemy) < sum(hero) - 0.1, "enemy frame must be darker");
        assert!(
            (hero[0] - enemy[0]).abs() + (hero[1] - enemy[1]).abs() > 0.1,
            "hero and enemy tints are numerically identical"
        );
        // Caller tint still multiplies through: 0.5 grey halves every channel.
        let mut batch = UiBatch::default();
        draw(
            &skin,
            &mut batch,
            Rect::new(0.0, 0.0, 198.0, 217.0),
            1.0,
            PortraitFrameKind::Hero,
            [0.5, 0.5, 0.5, 0.5],
        );
        let scaled = batch.textured[0].vertices[0].color;
        // `0.5` is an authored (sRGB) tint multiplier; the vertex colour is
        // linear, so the halving is asserted on the authored value and the
        // linear channel must be its sRGB->linear image.
        let hero_authored = crate::hud::authored_color(hero);
        let scaled_authored = crate::hud::authored_color(scaled);
        for channel in 0..3 {
            assert!(
                approx(scaled_authored[channel], 0.5 * hero_authored[channel]),
                "channel {channel}: authored {} != 0.5 * {}",
                scaled_authored[channel],
                hero_authored[channel]
            );
            assert!(
                approx(
                    scaled[channel],
                    crate::hud::srgb_to_linear(0.5 * hero_authored[channel])
                ),
                "channel {channel}: linear {} is not the linear of the half tint",
                scaled[channel]
            );
        }
        assert!(approx(scaled[3], 0.5));
    }

    /// `ui.colour.portrait-frame-tint` — the frame variant multiplies the
    /// caller's sRGB tint (`tint_mul`) and the caller's tint still multiplies
    /// through per channel; the vertex colour is the linear image of that
    /// product. Gold keeps red above blue, the enemy rim is darker than gold.
    #[test]
    fn ui_colour_portrait_frame_tint_is_authored_then_linear() {
        let skin = sample_skin();
        let base = [0.8, 0.75, 0.7, 1.0];
        let cases = [
            (PortraitFrameKind::Normal, PortraitFrameKind::Normal.tint_mul()),
            (PortraitFrameKind::Hero, PortraitFrameKind::Hero.tint_mul()),
            (PortraitFrameKind::Enemy, PortraitFrameKind::Enemy.tint_mul()),
        ];
        let mut emitted = Vec::new();
        for (kind, mul) in cases {
            let mut batch = UiBatch::default();
            draw(
                &skin,
                &mut batch,
                Rect::new(10.0, 10.0, 198.0, 217.0),
                1.0,
                kind,
                base,
            );
            let linear = batch.textured[0].vertices[0].color;
            for channel in 0..3 {
                let authored = base[channel] * mul[channel];
                assert!(
                    approx(crate::hud::authored_color(linear)[channel], authored),
                    "{kind:?} channel {channel}: {} is not the authored {authored}",
                    crate::hud::authored_color(linear)[channel]
                );
                assert!(
                    approx(linear[channel], crate::hud::srgb_to_linear(authored)),
                    "{kind:?} channel {channel}: vertex is not linear"
                );
            }
            assert_eq!(linear[3], base[3], "{kind:?} keeps the alpha");
            emitted.push(linear);
        }
        let (normal, hero, enemy) = (emitted[0], emitted[1], emitted[2]);
        assert!(
            crate::hud::linear_luminance(normal) > crate::hud::linear_luminance(hero),
            "the gold frame is a dim"
        );
        assert!(
            crate::hud::linear_luminance(hero) > crate::hud::linear_luminance(enemy),
            "the enemy rim is darker than gold"
        );
        assert!(
            crate::hud::authored_color(hero)[0] > crate::hud::authored_color(hero)[2],
            "gold keeps red above blue"
        );
    }

    #[test]
    fn shipped_insets_keep_a_real_window() {
        let mut skin = sample_skin();
        let index = Skin::role_index(ROLE).expect("portrait_frame role");
        // `analysis/h2/atlas/ui_atlas_insets.json`, role `frame_medium`.
        skin.insets[index] = [49.0, 65.0, 50.0, 48.0];
        let mut batch = UiBatch::default();
        let budget = Rect::new(40.0, 400.0, AUTHORED_SIZE[0], AUTHORED_SIZE[1]);
        let parts = draw(
            &skin,
            &mut batch,
            budget,
            1.0,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        // The nine-patch centre cell at the authored size: 198-49-50 x 217-65-48.
        assert!(
            approx(parts.portrait_rect.w, 99.0),
            "window width {}",
            parts.portrait_rect.w
        );
        assert!(
            approx(parts.portrait_rect.h, 104.0),
            "window height {}",
            parts.portrait_rect.h
        );
        assert!(parts.portrait_rect.x > budget.x);
        assert!(parts.portrait_rect.right() < budget.right());
        assert_quads_inside(&batch, budget);
        // Below the ornament budget no real window fits: the rect is clamped to
        // a 1 px interior instead of inverting.
        let mut small = UiBatch::default();
        let tiny = Rect::new(0.0, 0.0, 90.0, 100.0);
        let parts = draw(
            &skin,
            &mut small,
            tiny,
            1.0,
            PortraitFrameKind::Enemy,
            [1.0; 4],
        );
        assert!(
            approx(parts.portrait_rect.w, 1.0),
            "degenerate window {}",
            parts.portrait_rect.w
        );
        assert!(parts.portrait_rect.x > tiny.x && parts.portrait_rect.right() < tiny.right());
    }

    /// The real call site geometry: landscape 2414x1080 workbook layout.
    #[test]
    fn draw_reports_parts_for_the_retail_layout() {
        let hud = MobileHud::new();
        let view = HudView::new(glam::Vec2::new(2414.0, 1080.0));
        let layout = hud.layout(&view);
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(
            &skin,
            &mut batch,
            layout.portrait,
            layout.scale,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        assert!(parts.portrait_rect.is_inside(&layout.portrait));
        assert!(!parts.class_rect.intersects(&parts.portrait_rect));
        assert!(!parts.name_rect.intersects(&parts.portrait_rect));
        println!(
            "2414x1080 layout (synthetic insets 12): scale={:.4} console={:?} portrait_box={:?} frame={:?}",
            layout.scale,
            layout.console,
            layout.portrait,
            drawn_bounds(&batch)
        );
        println!(
            "2414x1080 zones: minimap={:?} layout.name={:?} layout.class={:?}",
            layout.minimap, layout.name, layout.class
        );
        println!("portrait_rect={:?}", parts.portrait_rect);
        println!("name_rect={:?}", parts.name_rect);
        println!("class_rect={:?}", parts.class_rect);

        // Shipped-atlas pass: the frame budget that hosts this portrait image at
        // the authored opening ratio, with the sidecar ornament insets.
        let mut shipped = sample_skin();
        let index = Skin::role_index(ROLE).expect("portrait_frame role");
        shipped.insets[index] = [49.0, 65.0, 50.0, 48.0];
        let mut direct = UiBatch::default();
        let direct_parts = draw(
            &shipped,
            &mut direct,
            layout.portrait,
            layout.scale,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        println!(
            "2414x1080 shipped insets on the raw portrait box: frame={:?} window={:?}",
            drawn_bounds(&direct),
            direct_parts.portrait_rect
        );
        let budget = frame_rect_for(layout.portrait);
        let mut batch = UiBatch::default();
        let parts = draw(
            &shipped,
            &mut batch,
            budget,
            layout.scale,
            PortraitFrameKind::Normal,
            [1.0; 4],
        );
        println!(
            "2414x1080 shipped insets (49/65/50/48): budget={budget:?} frame={:?}",
            drawn_bounds(&batch)
        );
        println!("shipped portrait_rect={:?}", parts.portrait_rect);
        println!("shipped name_rect={:?}", parts.name_rect);
        println!("shipped class_rect={:?}", parts.class_rect);
        assert!(parts.portrait_rect.is_inside(&budget));
        assert!(
            parts.portrait_rect.w > 90.0,
            "portrait window too small: {}",
            parts.portrait_rect.w
        );
    }
}
