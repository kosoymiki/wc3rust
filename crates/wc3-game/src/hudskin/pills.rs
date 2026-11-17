//! Resource pills: gold/lumber/supply frames plus the upkeep band.
//!
//! WC3 layout for this lane: three framed pills sit right-aligned on the
//! console's resource shelf, and the upkeep line is a wide band directly above
//! them. A pill is `158 x 34` px at the 1080p reference (`0.147 * 1080` wide),
//! the gap is `6 px`, and the upkeep band is `18 px` tall. Every dimension
//! multiplies the caller's HUD scale; the caller still owns the strings and
//! paints them with `Font` into the rects returned here.
//!
//! Drawing stays inside the `hudskin` contract (`mod.rs`): this module only
//! decides which atlas role goes where, `Skin::nine` / `Skin::stretch` build
//! the quads, and no vertex ever leaves the target rect.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Atlas role of one pill frame (`resource_bar`, 313x62).
pub const ROLE: &str = "resource_bar";
/// Atlas role of the upkeep band (`thin_bar`, 465x38).
pub const UPKEEP_ROLE: &str = "thin_bar";

/// Pills on the shelf, left to right: gold, lumber, supply.
pub const PILL_COUNT: usize = 3;
/// Pill width at the 1080p reference (`0.147 * 1080 = 158`).
pub const PILL_WIDTH: f32 = 158.0;
/// Pill height at the 1080p reference.
pub const PILL_HEIGHT: f32 = 34.0;
/// Gap between two pills at the 1080p reference.
pub const PILL_GAP: f32 = 6.0;
/// Upkeep band height at the 1080p reference.
pub const UPKEEP_HEIGHT: f32 = 18.0;

/// Negative/NaN scales collapse to zero instead of mirroring the row.
fn sanitize(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.max(0.0)
    } else {
        0.0
    }
}

/// The pill row inside `shelf`: `PILL_COUNT` equal frames, right-aligned and
/// bottom-aligned. When the shelf cannot fit the reference width the pills
/// shrink (and their gaps with it) instead of escaping the shelf.
pub fn pill_row(shelf: Rect, scale: f32) -> [Rect; PILL_COUNT] {
    let s = sanitize(scale);
    let count = PILL_COUNT as f32;
    let shelf_w = shelf.w.max(0.0);
    // A degenerate shelf keeps the gaps inside it as well.
    let gap = (PILL_GAP * s).clamp(0.0, shelf_w / count);
    let gaps = gap * (count - 1.0);
    let width = (PILL_WIDTH * s).min((shelf_w - gaps).max(0.0) / count);
    let height = (PILL_HEIGHT * s).min(shelf.h.max(0.0));
    let row_width = width * count + gaps;
    let left = shelf.right() - row_width;
    let top = shelf.bottom() - height;
    let mut row = [Rect::default(); PILL_COUNT];
    for (index, pill) in row.iter_mut().enumerate() {
        *pill = Rect::new(left + index as f32 * (width + gap), top, width, height);
    }
    row
}

/// Upkeep band above the pill row: same left/right edges, `UPKEEP_HEIGHT`
/// tall, its bottom edge resting on the top edge of the pills.
pub fn upkeep_band(shelf: Rect, scale: f32) -> Rect {
    let s = sanitize(scale);
    let row = pill_row(shelf, s);
    let first = row[0];
    let last = row[PILL_COUNT - 1];
    Rect::new(
        first.x,
        first.y - UPKEEP_HEIGHT * s,
        last.right() - first.x,
        UPKEEP_HEIGHT * s,
    )
}

/// Icon slot at the left of a pill: a square inset by `4 * scale`, at most
/// `24 * scale` a side, vertically centred and always inside the pill.
pub fn pill_icon_rect(pill: Rect, scale: f32) -> Rect {
    let s = sanitize(scale);
    let pad = 4.0 * s;
    let side = (pill.h - pad * 2.0).min(24.0 * s).max(0.0);
    Rect::new(pill.x + pad, pill.y + (pill.h - side) * 0.5, side, side).clamped_inside(&pill)
}

/// Number row (top) and caption row (bottom) boxes right of the icon. Both
/// span the same width so centred text shares one optical column.
pub fn pill_text_rects(pill: Rect, scale: f32) -> (Rect, Rect) {
    let s = sanitize(scale);
    let icon = pill_icon_rect(pill, s);
    let pad = 4.0 * s;
    let x = icon.right() + pad;
    let width = (pill.right() - pad - x).max(0.0);
    let height = (pill.h - pad * 2.0).max(0.0);
    let top = pill.y + pad;
    let number_h = height * 0.58;
    let number = Rect::new(x, top, width, number_h);
    let caption = Rect::new(x, top + number_h, width, (height - number_h).max(0.0));
    (number.clamped_inside(&pill), caption.clamped_inside(&pill))
}

/// Number box of a pill (upper row of [`pill_text_rects`]).
pub fn pill_number_rect(pill: Rect, scale: f32) -> Rect {
    pill_text_rects(pill, scale).0
}

/// Caption box of a pill (lower row of [`pill_text_rects`]).
pub fn pill_caption_rect(pill: Rect, scale: f32) -> Rect {
    pill_text_rects(pill, scale).1
}

/// Frame tint: the active pill keeps the caller's tint, idle pills recede.
pub fn pill_tint(active: bool, tint: [f32; 4]) -> [f32; 4] {
    if active {
        tint
    } else {
        [tint[0] * 0.72, tint[1] * 0.72, tint[2] * 0.72, tint[3]]
    }
}

/// Draw one pill frame into `rect` and return its icon slot.
///
/// The frame is `resource_bar` nine-patched (the `Skin::nine` contract keeps
/// the atlas corners at authored size); the caller draws the resource icon in
/// the returned rect and the number/caption in [`pill_text_rects`].
pub fn draw_pill(
    skin: &Skin,
    batch: &mut UiBatch,
    rect: Rect,
    active: bool,
    scale: f32,
    tint: [f32; 4],
) -> Rect {
    let _ = skin.nine(batch, ROLE, rect, pill_tint(active, tint));
    pill_icon_rect(rect, scale)
}

/// Text box of the upkeep band: the band inset by `8 * scale` horizontally and
/// `1 * scale` vertically, so glyphs never touch the stretched frame.
pub fn upkeep_text_rect(band: Rect, scale: f32) -> Rect {
    let s = sanitize(scale);
    band.inset_sides(8.0 * s, 1.0 * s, 8.0 * s, 1.0 * s)
        .clamped_inside(&band)
}

/// Draw the upkeep band into `rect` (one stretched `thin_bar` quad) and return
/// the text box for the upkeep label.
pub fn draw_upkeep(skin: &Skin, batch: &mut UiBatch, rect: Rect, scale: f32) -> Rect {
    let _ = skin.stretch(batch, UPKEEP_ROLE, rect, [1.0; 4]);
    upkeep_text_rect(rect, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::HudLayout;
    use crate::hudskin::tests_support::sample_skin;
    use glam::Vec2;

    /// HUD scale used by the app for a viewport (`hud::HudLayout`).
    fn scale_for(viewport: Vec2) -> f32 {
        HudLayout::new(viewport).scale
    }

    /// Reference resource shelf of this lane: 720x`PILL_HEIGHT` at 1080p,
    /// right/bottom aligned 24 px inside the safe frame. The runtime passes the
    /// console's real shelf (`mobile_hud::MobileHudLayout::resources`).
    fn shelf_fixture(viewport: Vec2, scale: f32) -> Rect {
        Rect::new(
            viewport.x - 24.0 * scale - 720.0 * scale,
            viewport.y - 24.0 * scale - PILL_HEIGHT * scale,
            720.0 * scale,
            PILL_HEIGHT * scale,
        )
    }

    fn assert_close(a: f32, b: f32, what: &str) {
        assert!((a - b).abs() < 1e-4, "{what}: {a} != {b}");
    }

    #[test]
    fn hudskin_pills_row_right_aligned_2414x1080() {
        let viewport = Vec2::new(2414.0, 1080.0);
        let scale = scale_for(viewport);
        assert_close(scale, 1.0, "landscape scale");
        let shelf = shelf_fixture(viewport, scale);
        assert_eq!(shelf, Rect::new(1670.0, 1022.0, 720.0, 34.0));
        let pills = pill_row(shelf, scale);
        assert_eq!(pills[0], Rect::new(1904.0, 1022.0, 158.0, 34.0));
        assert_eq!(pills[1], Rect::new(2068.0, 1022.0, 158.0, 34.0));
        assert_eq!(pills[2], Rect::new(2232.0, 1022.0, 158.0, 34.0));
        assert_close(pills[2].right(), shelf.right(), "right edge");
        for pill in pills {
            assert!(pill.is_inside(&shelf), "pill outside shelf: {pill:?}");
        }
    }

    #[test]
    fn hudskin_pills_row_right_aligned_1080x2414() {
        let viewport = Vec2::new(1080.0, 2414.0);
        let scale = scale_for(viewport);
        assert_close(scale, 1.0, "portrait scale");
        let shelf = shelf_fixture(viewport, scale);
        assert_eq!(shelf, Rect::new(336.0, 2356.0, 720.0, 34.0));
        let pills = pill_row(shelf, scale);
        assert_eq!(pills[0], Rect::new(570.0, 2356.0, 158.0, 34.0));
        assert_eq!(pills[1], Rect::new(734.0, 2356.0, 158.0, 34.0));
        assert_eq!(pills[2], Rect::new(898.0, 2356.0, 158.0, 34.0));
        assert_close(pills[2].right(), shelf.right(), "right edge");
        for pill in pills {
            assert!(pill.is_inside(&shelf), "pill outside shelf: {pill:?}");
        }
    }

    #[test]
    fn hudskin_pills_row_never_overlaps() {
        for viewport in [
            Vec2::new(2414.0, 1080.0),
            Vec2::new(1080.0, 2414.0),
            Vec2::new(1920.0, 1080.0),
            Vec2::new(800.0, 480.0),
        ] {
            let scale = scale_for(viewport);
            let shelf = shelf_fixture(viewport, scale);
            let pills = pill_row(shelf, scale);
            for pair in pills.windows(2) {
                let gap = pair[1].x - pair[0].right();
                assert!(
                    gap >= PILL_GAP * scale - 1e-3,
                    "gap {gap} below {} at {viewport:?}",
                    PILL_GAP * scale
                );
                assert!(!pair[0].intersects(&pair[1]), "overlap at {viewport:?}");
            }
        }
    }

    #[test]
    fn hudskin_pills_row_clamps_to_narrow_shelf() {
        let shelf = Rect::new(100.0, 50.0, 240.0, 34.0);
        let pills = pill_row(shelf, 1.0);
        assert_eq!(pills[0].x, shelf.x);
        assert_eq!(pills[2].right(), shelf.right());
        assert!(
            pills[0].w < PILL_WIDTH,
            "row must shrink, got {}",
            pills[0].w
        );
        assert_close(pills[0].w, pills[2].w, "equal widths");
        assert_close(pills[1].x - pills[0].right(), PILL_GAP, "gap kept");
        for pill in pills {
            assert!(
                pill.is_inside(&shelf),
                "pill outside narrow shelf: {pill:?}"
            );
        }

        // Zero-sized shelf is still a valid (empty) layout, never inverted.
        let empty = Rect::new(10.0, 20.0, 0.0, 0.0);
        for pill in pill_row(empty, 1.0) {
            assert_eq!(pill.w, 0.0);
            assert_eq!(pill.h, 0.0);
            assert_eq!(pill.x, empty.right());
            assert_eq!(pill.y, empty.bottom());
        }
    }

    #[test]
    fn hudskin_pills_icon_inside_pill() {
        let skin = sample_skin();
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let scale = scale_for(viewport);
            let shelf = shelf_fixture(viewport, scale);
            for pill in pill_row(shelf, scale) {
                let mut batch = UiBatch::default();
                let icon = draw_pill(&skin, &mut batch, pill, true, scale, [1.0; 4]);
                assert_eq!(icon, pill_icon_rect(pill, scale));
                assert!(icon.is_inside(&pill), "icon outside pill: {icon:?}");
                assert_close(icon.w, 24.0 * scale, "icon side");
                assert_close(icon.h, icon.w, "icon square");
                assert!(icon.x > pill.x && icon.right() < pill.right());
                assert!(!batch.textured.is_empty(), "no pill quads");
                let draw = &batch.textured[0];
                assert_eq!(draw.texture, skin.texture);
                for vertex in &draw.vertices {
                    let (x, y) = (vertex.pos[0], vertex.pos[1]);
                    assert!(
                        x >= pill.x - 0.5 && x <= pill.right() + 0.5,
                        "pill x outside: {x}"
                    );
                    assert!(
                        y >= pill.y - 0.5 && y <= pill.bottom() + 0.5,
                        "pill y outside: {y}"
                    );
                    assert!(vertex.uv[0] >= 0.0 && vertex.uv[0] <= 1.0);
                    assert!(vertex.uv[1] >= 0.0 && vertex.uv[1] <= 1.0);
                }
            }
        }
    }

    #[test]
    fn hudskin_pills_nine_patch_corner_identity() {
        let skin = sample_skin();
        let index = Skin::role_index(ROLE).expect("resource_bar role");
        let [ax, ay, aw, ah] = skin.rects[index];
        let [l, t, ..] = skin.insets[index];
        let pill = Rect::new(100.0, 50.0, PILL_WIDTH, PILL_HEIGHT);
        let mut batch = UiBatch::default();
        let _ = draw_pill(&skin, &mut batch, pill, true, 1.0, [1.0; 4]);
        assert_eq!(batch.textured.len(), 1, "one merged pill draw");
        let draw = &batch.textured[0];
        assert_eq!(
            draw.vertices.len(),
            9 * 6,
            "nine patches, six vertices each"
        );

        // Top-left corner keeps its atlas pixel size and its atlas UV corner.
        let top_left = &draw.vertices[0..6];
        assert_close(top_left[0].pos[0], pill.x, "corner x0");
        assert_close(top_left[0].pos[1], pill.y, "corner y0");
        assert_close(top_left[1].pos[0], pill.x + l, "corner width");
        assert_close(top_left[5].pos[1], pill.y + t, "corner height");
        assert_close(top_left[0].uv[0], ax / skin.width, "corner u0");
        assert_close(top_left[0].uv[1], ay / skin.height, "corner v0");
        assert_close(top_left[1].uv[0], (ax + l) / skin.width, "corner u1");

        // Bottom-right corner lands on the target corner and atlas corner.
        let bottom_right = &draw.vertices[8 * 6..9 * 6];
        assert_close(bottom_right[2].pos[0], pill.right(), "target right");
        assert_close(bottom_right[2].pos[1], pill.bottom(), "target bottom");
        assert_close(bottom_right[2].uv[0], (ax + aw) / skin.width, "atlas u1");
        assert_close(bottom_right[2].uv[1], (ay + ah) / skin.height, "atlas v1");
    }

    #[test]
    fn hudskin_pills_text_rects_inside() {
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let scale = scale_for(viewport);
            let shelf = shelf_fixture(viewport, scale);
            for pill in pill_row(shelf, scale) {
                let (number, caption) = pill_text_rects(pill, scale);
                assert_eq!(number, pill_number_rect(pill, scale));
                assert_eq!(caption, pill_caption_rect(pill, scale));
                assert!(number.is_inside(&pill), "number outside: {number:?}");
                assert!(caption.is_inside(&pill), "caption outside: {caption:?}");
                assert!(number.w > 0.0 && number.h > 0.0);
                assert!(caption.w > 0.0 && caption.h > 0.0);
                assert_close(number.x, caption.x, "shared column");
                assert_close(number.w, caption.w, "shared width");
                assert!(number.bottom() <= caption.y + 1e-4, "rows overlap");
                assert!(
                    number.x > pill_icon_rect(pill, scale).right(),
                    "number left of icon"
                );
            }
        }
    }

    #[test]
    fn hudskin_pills_active_state_is_brighter() {
        let skin = sample_skin();
        let pill = Rect::new(0.0, 0.0, PILL_WIDTH, PILL_HEIGHT);
        let tint = [0.9, 0.8, 0.7, 1.0];
        let mut active = UiBatch::default();
        let mut idle = UiBatch::default();
        let _ = draw_pill(&skin, &mut active, pill, true, 1.0, tint);
        let _ = draw_pill(&skin, &mut idle, pill, false, 1.0, tint);
        let a = active.textured[0].vertices[0].color;
        let i = idle.textured[0].vertices[0].color;
        // Vertex colours are linear; the authored tint is what the surface
        // displays, so the comparison goes through `authored_color`.
        let on = crate::hud::authored_color(a);
        let off = crate::hud::authored_color(i);
        for channel in 0..3 {
            assert!(
                (on[channel] - tint[channel]).abs() < 1e-6,
                "active keeps the caller tint: channel {channel} is {}",
                on[channel]
            );
            assert!(
                (off[channel] - pill_tint(false, tint)[channel]).abs() < 1e-6,
                "idle is dimmed by the auth value: channel {channel} is {}",
                off[channel]
            );
        }
        assert!(
            i[0] < a[0] && i[1] < a[1] && i[2] < a[2],
            "idle not dimmed in linear space"
        );
        assert_eq!(i[3], a[3], "alpha unchanged");
        assert_eq!(pill_tint(true, tint), tint);
    }

    /// `ui.colour.pills-tint` — the pill frame carries the caller's sRGB tint;
    /// the idle state dims it by the authored 0.72 factor (exact per channel in
    /// sRGB) and therefore emits less linear light. No byte is pinned.
    #[test]
    fn ui_colour_pills_tint_is_authored_and_dimmed_in_linear() {
        let skin = sample_skin();
        let pill = Rect::new(40.0, 400.0, PILL_WIDTH, PILL_HEIGHT);
        let tints = [
            [0.9, 0.8, 0.7, 1.0],
            [1.0, 1.0, 1.0, 0.8],
            [0.35, 0.42, 0.55, 1.0],
        ];
        for tint in tints {
            let mut active = UiBatch::default();
            let mut idle = UiBatch::default();
            let _ = draw_pill(&skin, &mut active, pill, true, 1.0, tint);
            let _ = draw_pill(&skin, &mut idle, pill, false, 1.0, tint);
            let a = active.textured[0].vertices[0].color;
            let i = idle.textured[0].vertices[0].color;
            for channel in 0..3 {
                let expected = crate::hud::srgb_to_linear(tint[channel]);
                assert!(
                    (a[channel] - expected).abs() < 1e-6,
                    "tint {tint:?} channel {channel}: vertex {a} is not linear",
                    a = a[channel]
                );
                let dimmed = crate::hud::srgb_to_linear(tint[channel] * 0.72);
                assert!(
                    (i[channel] - dimmed).abs() < 1e-6,
                    "tint {tint:?} channel {channel}: idle vertex {} is not the linear dim",
                    i[channel]
                );
            }
            assert_eq!(a[3], tint[3], "alpha is authored, not converted");
            assert_eq!(i[3], tint[3], "idle keeps the alpha");
            assert!(
                crate::hud::linear_luminance(i) < crate::hud::linear_luminance(a),
                "tint {tint:?}: idle must emit less light"
            );
        }
    }

    #[test]
    fn hudskin_pills_upkeep_band_above_row() {
        let skin = sample_skin();
        let index = Skin::role_index(UPKEEP_ROLE).expect("thin_bar role");
        let [ax, ay, aw, ah] = skin.rects[index];
        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let scale = scale_for(viewport);
            let shelf = shelf_fixture(viewport, scale);
            let pills = pill_row(shelf, scale);
            let band = upkeep_band(shelf, scale);
            assert_close(band.bottom(), pills[0].y, "band rests on the row");
            assert_close(band.x, pills[0].x, "band left edge");
            assert_close(
                band.right(),
                pills[PILL_COUNT - 1].right(),
                "band right edge",
            );
            assert_close(band.h, UPKEEP_HEIGHT * scale, "band height");

            let mut batch = UiBatch::default();
            let text = draw_upkeep(&skin, &mut batch, band, scale);
            assert_eq!(batch.textured.len(), 1, "upkeep is one stretched quad");
            let draw = &batch.textured[0];
            assert_eq!(draw.vertices.len(), 6);
            assert_close(draw.vertices[0].pos[0], band.x, "quad x0");
            assert_close(draw.vertices[0].pos[1], band.y, "quad y0");
            assert_close(draw.vertices[2].pos[0], band.right(), "quad x1");
            assert_close(draw.vertices[2].pos[1], band.bottom(), "quad y1");
            assert_close(draw.vertices[0].uv[0], ax / skin.width, "thin u0");
            assert_close(draw.vertices[0].uv[1], ay / skin.height, "thin v0");
            assert_close(draw.vertices[2].uv[0], (ax + aw) / skin.width, "thin u1");
            assert_close(draw.vertices[2].uv[1], (ay + ah) / skin.height, "thin v1");
            assert!(text.is_inside(&band), "upkeep text outside: {text:?}");
            assert_eq!(text, upkeep_text_rect(band, scale), "returned text box");
        }
    }

    #[test]
    fn hudskin_pills_fit_mobile_hud_resource_shelf() {
        use crate::hud::mobile_hud::{HudView, MobileHud};

        for viewport in [Vec2::new(2414.0, 1080.0), Vec2::new(1080.0, 2414.0)] {
            let layout = MobileHud::new().layout(&HudView::new(viewport));
            let scale = scale_for(viewport);
            let pills = pill_row(layout.resources, scale);
            for pill in pills {
                assert!(
                    pill.is_inside(&layout.resources),
                    "pill outside real shelf at {viewport:?}: {pill:?} vs {:?}",
                    layout.resources
                );
            }
            assert_close(
                pills[PILL_COUNT - 1].right(),
                layout.resources.right(),
                "row right edge on the real shelf",
            );
            for pair in pills.windows(2) {
                assert!(pair[0].right() <= pair[1].x + 1e-3, "overlap on real shelf");
            }
        }
    }
}
