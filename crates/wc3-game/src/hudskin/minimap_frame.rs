//! Minimap frame, header band, image slot and the camera-rectangle slot.
//!
//! WC3 keeps a square minimap at the far left of the console and puts the word
//! MINIMAP on a bar directly above it. The atlas art is one ornamented
//! `dialog_wide` panel drawn as a nine-patch around the square plus a
//! `thin_bar` band; this module owns that widget's geometry and the two chrome
//! draws, and nothing else. The minimap texture, the fog mask, the blips and the
//! white camera rectangle are *not* drawn here: the image and the camera slot
//! are returned so the caller keeps those passes in one place (fog and blips are
//! screen-space overlays the minimap view already owns).
//!
//! Geometry contract: `origin`/`side` describe the **image** square (the
//! 216x216 authored minimap, i.e. `MobileHudLayout::minimap`), so the layout's
//! hit rect stays the image and the ornament is dilated outwards. The border is
//! the role's measured nine-patch inset, which is also the corner size
//! `Skin::nine` slices at, which makes the nine-patch's centre cell *exactly*
//! the image rect (`ui_minimap_nine_patch_centre_cell_is_the_image_rect`).

use glam::Vec2;

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region: the frame ornament (nine-patch).
pub const ROLE: &str = "dialog_wide";
/// Header band role: the bar above the frame (`thin_bar`, 465x38 px).
pub const HEADER_ROLE: &str = "thin_bar";

/// Smallest ornament thickness, in screen px. A skin that carries no measured
/// inset for [`ROLE`] still gets a frame that reaches past the image edge, so
/// `image` is strictly inside `frame` for every skin.
pub const BORDER_FLOOR_PX: f32 = 1.0;

/// The ornament may take at most this fraction of `side` per edge. `dialog_wide`
/// is a whole 391x267 dialog panel whose stretch cuts are 45/113/39/56 px; used
/// unbudgeted around a 216 px square that frame is 385 px tall and no longer
/// fits the 288 px console. The hudskin skin table is authored at 12 px
/// (`tests_support`), which sits well inside the budget.
pub const MAX_BORDER_FRACTION: f32 = 0.125;

/// Minimum header band height, in authored px (multiplied by `scale`). The bar
/// art gives 17.7 px at a 216 px square; under ~12 px the word MINIMAP stops
/// rendering legibly, so small minimaps keep a floor instead of a hairline.
pub const HEADER_MIN_PX: f32 = 12.0;

/// The camera rectangle is drawn this many authored px (`* scale`) inside the
/// image, so the white outline is never clipped by the frame ornament.
pub const VIEWPORT_INSET_PX: f32 = 1.0;

/// Rects produced by one [`draw`] call. All four are in screen space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinimapParts {
    /// Outer ornamented frame: the square dilated by the measured border.
    pub frame: Rect,
    /// Minimap texture target (terrain + fog + blips). Strictly inside `frame`.
    pub image: Rect,
    /// Header band above the frame; the caller draws the word MINIMAP here.
    pub header: Rect,
    /// Rect the normalised camera rectangle maps into ([`camera_rect`]).
    pub viewport_rect_slot: Rect,
}

impl MinimapParts {
    /// True when the request was degenerate (zero/negative/non-finite side), in
    /// which case nothing was drawn and every rect is empty.
    pub fn is_empty(&self) -> bool {
        self.image.w <= 0.0 || self.image.h <= 0.0
    }
}

/// Draw the minimap chrome and return its slots.
///
/// `origin` is the top-left corner of the image square, `side` its side in
/// screen px, `scale` the HUD scale (`MobileHudLayout::scale`) used for the
/// camera inset and the header floor. Draws the frame nine-patch and the header
/// band; returns the four rects. Never samples outside the two atlas elements.
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    origin: Vec2,
    side: f32,
    scale: f32,
    tint: [f32; 4],
) -> MinimapParts {
    if !origin.is_finite() || !side.is_finite() || side <= 0.0 {
        return MinimapParts::default();
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let border = borders(skin, side);
    let frame = Rect::new(
        origin.x - border[0],
        origin.y - border[1],
        side + border[0] + border[2],
        side + border[1] + border[3],
    );
    let image = Rect::new(origin.x, origin.y, side, side);
    skin.nine(batch, ROLE, frame, tint);
    let header = header_rect(skin, frame, image.x, side, scale);
    skin.stretch(batch, HEADER_ROLE, header, tint);
    let inset = VIEWPORT_INSET_PX * scale;
    MinimapParts {
        frame,
        image,
        header,
        viewport_rect_slot: image.inset(inset, inset),
    }
}

/// Map a camera rectangle into [`MinimapParts::viewport_rect_slot`].
///
/// `camera` is `[x0, y0, x1, y1]` in normalised map space (0..1, y down like the
/// minimap texture). Corners may arrive unordered, and a camera frustum can
/// overhang the map edge on any side, so both axes are ordered and clamped
/// before mapping. Returns an empty rect when the slot or the camera is invalid.
pub fn camera_rect(slot: Rect, camera: [f32; 4]) -> Rect {
    if slot.w <= 0.0 || slot.h <= 0.0 || !camera.iter().all(|value| value.is_finite()) {
        return Rect::default();
    }
    let x0 = camera[0].min(camera[2]).clamp(0.0, 1.0);
    let x1 = camera[0].max(camera[2]).clamp(0.0, 1.0);
    let y0 = camera[1].min(camera[3]).clamp(0.0, 1.0);
    let y1 = camera[1].max(camera[3]).clamp(0.0, 1.0);
    Rect::new(
        slot.x + x0 * slot.w,
        slot.y + y0 * slot.h,
        (x1 - x0) * slot.w,
        (y1 - y0) * slot.h,
    )
}

/// Ornament thickness per side, in screen px: the role's measured nine-patch
/// insets, floored at [`BORDER_FLOOR_PX`] and capped at
/// [`MAX_BORDER_FRACTION`] of `side` so the frame cannot outgrow the console
/// slot the layout gives it.
fn borders(skin: &Skin, side: f32) -> [f32; 4] {
    let Some(index) = Skin::role_index(ROLE) else {
        return [BORDER_FLOOR_PX; 4];
    };
    let budget = (side * MAX_BORDER_FRACTION).max(BORDER_FLOOR_PX);
    let mut border = [BORDER_FLOOR_PX; 4];
    for (slot, measured) in border.iter_mut().zip(skin.insets[index]) {
        if measured.is_finite() {
            *slot = measured.clamp(BORDER_FLOOR_PX, budget);
        }
    }
    border
}

/// Header band: `side` wide, sitting on the frame's top edge, as tall as the bar
/// art's own aspect at that width (so the bar is a uniform downscale, not a
/// vertical squash) with [`HEADER_MIN_PX`] as the legibility floor.
fn header_rect(skin: &Skin, frame: Rect, x: f32, side: f32, scale: f32) -> Rect {
    let mut height = 0.0;
    if let Some(index) = Skin::role_index(HEADER_ROLE) {
        let [_, _, art_w, art_h] = skin.rects[index];
        if art_w > 0.0 && art_h > 0.0 && art_w.is_finite() && art_h.is_finite() {
            height = side * (art_h / art_w);
        }
    }
    let height = height.max(HEADER_MIN_PX * scale);
    Rect::new(x, frame.y - height, side, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::mobile_hud::{HudView, MobileHud, CONSOLE_HEIGHT};
    use crate::hudskin::tests_support::sample_skin;

    const TINT: [f32; 4] = [1.0; 4];

    /// Quads emitted by one batch, as `(dest rect, uv rect)` in draw order.
    fn quads(batch: &UiBatch) -> Vec<(Rect, [f32; 4])> {
        let mut out = Vec::new();
        for draw in &batch.textured {
            for chunk in draw.vertices.chunks_exact(6) {
                let first = chunk[0];
                let third = chunk[2];
                out.push((
                    Rect::new(
                        first.pos[0],
                        first.pos[1],
                        third.pos[0] - first.pos[0],
                        third.pos[1] - first.pos[1],
                    ),
                    [first.uv[0], first.uv[1], third.uv[0], third.uv[1]],
                ));
            }
        }
        out
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-3
    }

    fn assert_rect(actual: Rect, expected: Rect) {
        assert!(
            close(actual.x, expected.x)
                && close(actual.y, expected.y)
                && close(actual.w, expected.w)
                && close(actual.h, expected.h),
            "rect {actual:?} != {expected:?}"
        );
    }

    #[test]
    fn ui_minimap_frame_wraps_the_square_with_the_measured_border() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(100.0, 50.0), 216.0, 1.0, TINT);
        assert!(!parts.is_empty(), "a 216 px square must produce slots");
        // Measured 12 px inset on every side: frame = square + 2*inset.
        assert_rect(parts.image, Rect::new(100.0, 50.0, 216.0, 216.0));
        assert_rect(parts.frame, Rect::new(88.0, 38.0, 240.0, 240.0));
        assert!(close(parts.frame.w - parts.frame.h, 0.0), "frame is square");
        // Band on the frame's top edge, spanning the image width.
        assert!(close(parts.header.bottom(), parts.frame.y));
        assert!(close(parts.header.x, parts.image.x));
        assert!(close(parts.header.w, parts.image.w));
        assert!(parts.header.h > 0.0);
        // Slot: one authored px inside the image at scale 1.
        assert_rect(
            parts.viewport_rect_slot,
            Rect::new(101.0, 51.0, 214.0, 214.0),
        );
        assert!(parts.viewport_rect_slot.is_inside(&parts.image));
    }

    #[test]
    fn ui_minimap_image_rect_is_strictly_inside_the_frame() {
        let skin = sample_skin();
        for side in [64.0f32, 216.0, 480.0] {
            for scale in [0.62f32, 1.0, 2.0] {
                let mut batch = UiBatch::default();
                let parts = draw(&skin, &mut batch, Vec2::new(40.0, 30.0), side, scale, TINT);
                let border = [
                    parts.image.x - parts.frame.x,
                    parts.image.y - parts.frame.y,
                    parts.frame.right() - parts.image.right(),
                    parts.frame.bottom() - parts.image.bottom(),
                ];
                for (edge, thickness) in ["left", "top", "right", "bottom"].iter().zip(border) {
                    assert!(
                        thickness >= BORDER_FLOOR_PX - 1e-3,
                        "{edge} border {thickness} px at side {side} scale {scale}"
                    );
                }
                assert!(parts.image.x > parts.frame.x, "strict left edge");
                assert!(parts.image.y > parts.frame.y, "strict top edge");
                assert!(parts.image.right() < parts.frame.right(), "strict right");
                assert!(parts.image.bottom() < parts.frame.bottom(), "strict bottom");
                assert!(parts.frame.contains_rect(&parts.image));
                // The band never overlaps the ornament or the map itself.
                assert!(parts.header.bottom() <= parts.frame.y + 1e-3);
                assert!(!parts.header.intersects(&parts.image));
                assert!(!parts.header.intersects(&parts.frame));
            }
        }
    }

    #[test]
    fn ui_minimap_nine_patch_centre_cell_is_the_image_rect() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(100.0, 50.0), 216.0, 1.0, TINT);
        let emitted = quads(&batch);
        assert_eq!(emitted.len(), 10, "nine frame cells plus one header band");
        // Row-major nine-patch: index 4 is the stretched centre.
        assert_rect(emitted[4].0, parts.image);
        assert!(close(emitted[4].0.w, 216.0) && close(emitted[4].0.h, 216.0));
        // The corners keep their measured 12 px: the ornament is not deformed.
        for corner in [0usize, 2, 6, 8] {
            assert!(
                close(emitted[corner].0.w, 12.0) && close(emitted[corner].0.h, 12.0),
                "corner {corner} is {:?}",
                emitted[corner].0
            );
        }
    }

    #[test]
    fn ui_minimap_frame_keeps_the_square_aspect() {
        let skin = sample_skin();
        for side in [96.0f32, 216.0, 512.0] {
            let mut batch = UiBatch::default();
            let parts = draw(&skin, &mut batch, Vec2::ZERO, side, 1.0, TINT);
            assert!(
                close(parts.frame.w, parts.frame.h),
                "frame {}x{} is not square at side {side}",
                parts.frame.w,
                parts.frame.h
            );
            assert!(close(parts.image.w, parts.image.h), "image stays square");
            assert!(close(parts.image.w, side), "image keeps the requested side");
        }
    }

    #[test]
    fn ui_minimap_asymmetric_ornament_keeps_the_square_image() {
        let mut skin = sample_skin();
        let index = Skin::role_index(ROLE).expect("dialog_wide is a role");
        skin.insets[index] = [10.0, 14.0, 8.0, 6.0];
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(50.0, 60.0), 200.0, 1.0, TINT);
        assert_rect(parts.image, Rect::new(50.0, 60.0, 200.0, 200.0));
        // Each side grows by its own measured inset, so the image stays square.
        assert_rect(parts.frame, Rect::new(40.0, 46.0, 218.0, 220.0));
        let emitted = quads(&batch);
        assert_eq!(emitted.len(), 10);
        assert_rect(emitted[4].0, parts.image);
    }

    #[test]
    fn ui_minimap_corner_identity_is_stable_at_1x_and_2x() {
        let skin = sample_skin();
        let mut one = UiBatch::default();
        let mut two = UiBatch::default();
        let at1 = draw(&skin, &mut one, Vec2::new(80.0, 40.0), 216.0, 1.0, TINT);
        let at2 = draw(&skin, &mut two, Vec2::new(80.0, 40.0), 432.0, 2.0, TINT);
        let q1 = quads(&one);
        let q2 = quads(&two);
        assert_eq!(q1.len(), q2.len());
        for corner in [0usize, 2, 6, 8] {
            let (rect1, uv1) = q1[corner];
            let (rect2, uv2) = q2[corner];
            // Same atlas pixels: no resampling drift between scales.
            for (a, b) in uv1.iter().zip(uv2.iter()) {
                assert!((a - b).abs() < 1e-6, "corner {corner} uv {uv1:?} vs {uv2:?}");
            }
            // Same destination size: the ornament keeps its authored thickness.
            assert!(close(rect1.w, rect2.w) && close(rect1.h, rect2.h));
            assert!(close(rect1.w, 12.0) && close(rect1.h, 12.0));
        }
        // The frame grew by exactly the square's growth (the border did not).
        assert!(close(at2.frame.w - at1.frame.w, 216.0));
        assert!(close(at2.frame.h - at1.frame.h, 216.0));
        assert!(close(at1.frame.x - at2.frame.x, 0.0));
    }

    #[test]
    fn ui_minimap_oversized_atlas_cuts_stay_inside_the_console_budget() {
        // The painter's-atlas stretch cuts for `dialog_wide` (panel_dialog,
        // analysis/h2/atlas/ui_atlas_insets.json): 45/113/39/56.
        let mut skin = sample_skin();
        let index = Skin::role_index(ROLE).expect("dialog_wide is a role");
        skin.insets[index] = [45.0, 113.0, 39.0, 56.0];
        let side = 216.0 * 1.1;
        let scale = 1.1;
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(26.4, 52.8), side, scale, TINT);
        let budget = side * MAX_BORDER_FRACTION;
        assert!(close(parts.frame.w, side + 2.0 * budget));
        assert!(close(parts.frame.h, side + 2.0 * budget));
        assert_rect(parts.image, Rect::new(26.4, 52.8, side, side));
        // Unbudgeted those cuts make a 385 px frame: taller than the console.
        let console_h = CONSOLE_HEIGHT * scale;
        assert!(side + 113.0 + 56.0 > console_h, "the cap is load bearing");
        assert!(parts.frame.h < console_h, "budgeted frame fits the console");
        // The ornament still starts at least one pixel outside the image.
        assert!(parts.image.x - parts.frame.x >= BORDER_FLOOR_PX - 1e-3);
    }

    #[test]
    fn ui_minimap_square_at_2414x1080_clears_the_console_and_the_card() {
        let hud = MobileHud::new();
        let view = HudView::new(Vec2::new(2414.0, 1080.0));
        let layout = hud.layout(&view);
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let origin = Vec2::new(layout.minimap.x, layout.minimap.y);
        let parts = draw(
            &skin,
            &mut batch,
            origin,
            layout.minimap.w,
            layout.scale,
            TINT,
        );
        println!(
            "2414x1080 scale={} console={:?} card={:?} frame={:?} image={:?} header={:?}",
            layout.scale, layout.console, layout.card, parts.frame, parts.image, parts.header
        );
        // The image is the layout's minimap hit rect, the frame is dilated out.
        assert_rect(parts.image, layout.minimap);
        assert!(
            layout.console.contains_rect(&parts.frame),
            "frame {:?} must sit inside the console {:?}",
            parts.frame,
            layout.console
        );
        assert!(
            layout.console.contains_rect(&parts.header),
            "header {:?} must sit inside the console",
            parts.header
        );
        // The command card is fixed geometry: the ornament must not reach it.
        assert_eq!(
            parts.frame.intersection_area(&layout.card),
            0.0,
            "frame {:?} overlaps the card {:?}",
            parts.frame,
            layout.card
        );
        assert!(!parts.frame.intersects(&layout.card));
        // A border, not a body: the map keeps most of the widget.
        assert!(parts.image.w / parts.frame.w > 0.8);
        assert!(!batch.textured.is_empty(), "the chrome must be emitted");
    }

    #[test]
    fn ui_minimap_square_at_1080x2414_clears_the_console_and_the_card() {
        let hud = MobileHud::new();
        let view = HudView::new(Vec2::new(1080.0, 2414.0));
        let layout = hud.layout(&view);
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let origin = Vec2::new(layout.minimap.x, layout.minimap.y);
        let parts = draw(
            &skin,
            &mut batch,
            origin,
            layout.minimap.w,
            layout.scale,
            TINT,
        );
        println!(
            "1080x2414 scale={} console={:?} card={:?} frame={:?} image={:?} header={:?}",
            layout.scale, layout.console, layout.card, parts.frame, parts.image, parts.header
        );
        assert_rect(parts.image, layout.minimap);
        assert!(
            layout.console.contains_rect(&parts.frame),
            "frame {:?} must sit inside the console {:?}",
            parts.frame,
            layout.console
        );
        assert!(layout.console.contains_rect(&parts.header));
        assert_eq!(
            parts.frame.intersection_area(&layout.card),
            0.0,
            "frame {:?} overlaps the card {:?}",
            parts.frame,
            layout.card
        );
        assert!(!parts.frame.intersects(&layout.card));
        assert!(parts.image.w / parts.frame.w > 0.8);
    }

    #[test]
    fn ui_minimap_header_band_spans_the_square_and_keeps_the_bar_aspect() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(0.0, 100.0), 216.0, 1.0, TINT);
        // thin_bar is 465x38: a uniform downscale to the square's width.
        assert!(
            close(parts.header.h, 216.0 * 38.0 / 465.0),
            "band height {} is not the bar aspect",
            parts.header.h
        );
        let emitted = quads(&batch);
        let band = emitted[emitted.len() - 1];
        assert_rect(band.0, parts.header);
        let index = Skin::role_index(HEADER_ROLE).expect("thin_bar is a role");
        let [ax, ay, aw, ah] = skin.rects[index];
        let expected = [
            ax / skin.width,
            ay / skin.height,
            (ax + aw) / skin.width,
            (ay + ah) / skin.height,
        ];
        for (a, b) in band.1.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-6, "band uv {:?} != {expected:?}", band.1);
        }
        // Small minimap: the legibility floor takes over the art aspect.
        let mut small = UiBatch::default();
        let tiny = draw(&skin, &mut small, Vec2::ZERO, 40.0, 1.0, TINT);
        assert!(close(tiny.header.h, HEADER_MIN_PX));
        assert!(close(tiny.header.w, 40.0));
    }

    #[test]
    fn ui_minimap_degenerate_side_draws_nothing() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        for side in [0.0f32, -5.0, f32::NAN, f32::INFINITY] {
            let parts = draw(&skin, &mut batch, Vec2::new(10.0, 10.0), side, 1.0, TINT);
            assert!(parts.is_empty(), "side {side} must produce no widget");
            assert_eq!(parts, MinimapParts::default());
        }
        assert!(batch.textured.is_empty(), "no quad for a degenerate side");
        let parts = draw(&skin, &mut batch, Vec2::new(f32::NAN, 0.0), 216.0, 1.0, TINT);
        assert!(parts.is_empty());
        assert!(batch.textured.is_empty());
    }

    #[test]
    fn ui_minimap_quads_stay_inside_the_frame_and_the_atlas_element() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::new(100.0, 50.0), 216.0, 1.0, TINT);
        let frame_index = Skin::role_index(ROLE).expect("dialog_wide is a role");
        let [ax, ay, aw, ah] = skin.rects[frame_index];
        let frame_uv = [
            ax / skin.width,
            ay / skin.height,
            (ax + aw) / skin.width,
            (ay + ah) / skin.height,
        ];
        let emitted = quads(&batch);
        for (index, (rect, uv)) in emitted.iter().enumerate() {
            let outer = if index == emitted.len() - 1 {
                parts.header
            } else {
                parts.frame
            };
            assert!(
                outer.contains_rect(rect),
                "quad {index} {rect:?} escapes {outer:?}"
            );
            if index < emitted.len() - 1 {
                assert!(
                    uv[0] >= frame_uv[0] - 1e-6
                        && uv[2] <= frame_uv[2] + 1e-6
                        && uv[1] >= frame_uv[1] - 1e-6
                        && uv[3] <= frame_uv[3] + 1e-6,
                    "quad {index} uv {uv:?} leaves the atlas element {frame_uv:?}"
                );
            }
            for value in uv {
                assert!((0.0..=1.0).contains(value), "uv {value} outside the atlas");
            }
        }
    }

    #[test]
    fn ui_minimap_camera_rect_maps_into_the_viewport_slot() {
        let slot = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_rect(camera_rect(slot, [0.0, 0.0, 1.0, 1.0]), slot);
        // Unordered corners are ordered first.
        assert_rect(
            camera_rect(slot, [0.5, 0.9, 0.2, 0.2]),
            Rect::new(30.0, 30.0, 30.0, 35.0),
        );
        // A frustum overhanging the map edge clamps to the slot.
        assert_rect(
            camera_rect(slot, [-0.5, 0.25, 0.5, 1.5]),
            Rect::new(10.0, 32.5, 50.0, 37.5),
        );
        // Degenerate inputs produce an empty rect, never NaN geometry.
        assert_eq!(camera_rect(slot, [f32::NAN, 0.0, 1.0, 1.0]), Rect::default());
        assert_eq!(camera_rect(Rect::default(), [0.0; 4]), Rect::default());
        // A mapped camera rect always lands inside the slot it was given.
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(&skin, &mut batch, Vec2::ZERO, 216.0, 1.0, TINT);
        let camera = camera_rect(parts.viewport_rect_slot, [0.25, 0.25, 0.75, 0.75]);
        assert!(camera.is_inside(&parts.viewport_rect_slot));
        assert!(camera.is_inside(&parts.image));
        assert!(camera.w > 0.0 && camera.h > 0.0);
    }

    #[test]
    fn ui_minimap_viewport_slot_scales_with_the_hud() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let one = draw(&skin, &mut batch, Vec2::new(10.0, 20.0), 216.0, 1.0, TINT);
        let two = draw(&skin, &mut batch, Vec2::new(10.0, 20.0), 216.0, 2.0, TINT);
        // The inset follows the HUD scale so the white outline stays visible.
        assert!(close(one.viewport_rect_slot.w, 216.0 - 2.0 * VIEWPORT_INSET_PX));
        assert!(close(two.viewport_rect_slot.w, 216.0 - 4.0 * VIEWPORT_INSET_PX));
        // Skin::nine slices at atlas px, so the ornament does not grow with
        // `scale`: the frame is identical at 1x and 2x HUD scale.
        assert!(close(one.frame.w, two.frame.w));
        assert!(close(one.frame.h, two.frame.h));
        assert!(one.viewport_rect_slot.is_inside(&one.image));
        assert!(two.viewport_rect_slot.is_inside(&two.image));
    }
}
