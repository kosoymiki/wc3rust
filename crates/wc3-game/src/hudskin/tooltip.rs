//! WC3 tooltip box: name line, description block, cost/requirement section.
//!
//! Retail composition (`CToolTipWar3` ctor `0x6f154a20`,
//! `analysis/agent-re/ui_audio/ENGINE_MAP.md` §3.1): the engine builds a name
//! frame, a description frame and **at most eight** cost/requirement rows
//! (`in_ECX[0x21] = 8`), with `0.005` line padding and `0.01` / `0.0019`
//! backdrop insets. The cost row is the only section the shipped UI paints
//! gold (`FED312`, [`crate::style::GOLD`]).
//!
//! The lane owns geometry only: it emits the `tooltip_box` nine-patch and the
//! `thin_bar` divider, then hands the title/body/cost rects back so the caller
//! draws the font. Placement follows WC3: the box floats above the hovered
//! command slot ([`Rect`] `anchor`), flips below when the slot sits near the top
//! edge, and is clamped inside the safe frame — a tooltip never leaves it.

use super::Skin;
use crate::hud::{Rect, UiBatch};

/// Primary atlas role of this region.
pub const ROLE: &str = "tooltip_box";

/// Divider role: one thin bar between the title and the description.
pub const DIVIDER_ROLE: &str = "thin_bar";

/// Retail cost/requirement colour (`FED312`), shared with the live HUD.
pub const COST_TINT: [f32; 4] = crate::style::GOLD;

/// Widest tooltip at scale 1 (live HUD constant, `mobile_hud.rs` tooltip pass).
pub const MAX_WIDTH: f32 = 380.0;

/// The box never takes more than this share of the safe frame.
pub const MAX_SAFE_FRACTION: f32 = 0.42;

/// Visible content lines: title + description + cost rows.
pub const MAX_LINES: usize = 9;

/// Retail cost/requirement rows (`CToolTipWar3` ctor: `in_ECX[0x21] = 8`).
pub const MAX_COST_LINES: usize = 8;

/// Narrowest box that still shows the full nine-patch ornament.
pub const MIN_WIDTH: f32 = 120.0;

/// Lines assumed when the caller has no measurement yet (`desired` empty).
pub const DEFAULT_LINES: usize = 2;

/// Horizontal content inset per scale (live HUD: `10 * s`).
pub const PAD_X: f32 = 10.0;

/// Vertical content inset per scale (live HUD: `6 * s`).
pub const PAD_Y: f32 = 6.0;

/// Gap between the hovered slot and the box per scale.
pub const ANCHOR_GAP: f32 = 6.0;

/// Divider thickness per scale (clamped to 2..=10 px).
pub const DIVIDER_HEIGHT: f32 = 6.0;

/// Clearance above and below the divider per scale.
pub const DIVIDER_GAP: f32 = 3.0;

/// Glyph em box of the HUD atlas at scale 1 (`hud.rs` rasterises at 30 px).
const LINE_BASE: f32 = 30.0;

/// Leading included in one text line (`Font::line_box` = 37.5 px at scale 1).
const LINE_RATIO: f32 = 1.25;

/// The description is always visible: costs never claim the last two lines.
const MIN_BODY_LINES: usize = 1;

/// What the caller asks for. `desired` in [`draw`] is this request packed into
/// a [`Rect`]; callers that know their exact section counts use
/// [`draw_request`] instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooltipRequest {
    /// Preferred box width in pixels; `<= 0` asks for [`MAX_WIDTH`] at this scale.
    pub width: f32,
    /// Requested content lines: one title line + description + cost rows.
    pub lines: usize,
    /// Requested cost/requirement rows. Capacity is also bounded by `lines`
    /// (title and one description line always win) and by [`MAX_COST_LINES`].
    pub cost_lines: usize,
}

impl TooltipRequest {
    pub fn new(width: f32, lines: usize, cost_lines: usize) -> Self {
        Self {
            width,
            lines,
            cost_lines,
        }
    }

    /// Request encoded by the [`draw`] `desired` rect: `w` is the preferred
    /// width, `h` the measured height of every text line. The cost section
    /// claims whatever capacity the requested height leaves after the title and
    /// one description line.
    pub fn from_desired(desired: Rect, scale: f32) -> Self {
        Self {
            width: desired.w,
            lines: lines_from_height(desired.h, scale),
            cost_lines: MAX_COST_LINES,
        }
    }
}

/// Geometry of one tooltip: the shell plus the rects the caller draws text into.
#[derive(Clone, Debug, Default)]
pub struct TooltipParts {
    /// The `tooltip_box` nine-patch target.
    pub box_rect: Rect,
    /// Name line (one line box, gold-free: the caller owns the title colour).
    pub title_rect: Rect,
    /// Description block; height is a whole number of line boxes.
    pub body_rect: Rect,
    /// One rect per visible cost/requirement row, top to bottom. Capacity only:
    /// the caller zips its own cost strings with this slice and ignores the rest.
    pub cost_rects: Vec<Rect>,
    /// The `thin_bar` drawn between [`Self::title_rect`] and [`Self::body_rect`].
    pub divider_rect: Rect,
    /// Line box height shared by every text rect above.
    pub line_height: f32,
    /// Visible content lines the box was sized for (`0` when [`Self::is_empty`]).
    pub lines: usize,
    /// True when the box nine-patch and the divider were emitted.
    pub drawn: bool,
    /// True when the box sits above the anchor, false when it flipped below.
    pub above: bool,
}

impl TooltipParts {
    /// No safe frame, nothing to draw: every rect is zero-sized.
    pub fn is_empty(&self) -> bool {
        self.box_rect.w <= 0.0 || self.box_rect.h <= 0.0
    }
}

/// Height of one text line box at `scale`, floored at the readable prose size
/// exactly like `Font::line_box` / `mobile_hud::tooltip_line_box`.
pub fn line_height(scale: f32) -> f32 {
    LINE_BASE * LINE_RATIO * text_scale(scale)
}

/// Content lines requested by a measured content height, clamped to the retail
/// cap. `h <= 0` (no measurement) yields [`DEFAULT_LINES`].
fn lines_from_height(height: f32, scale: f32) -> usize {
    let line = line_height(scale);
    if !height.is_finite() || height <= 0.0 || line <= 0.0 {
        return DEFAULT_LINES;
    }
    // -1e-3 so an exact multiple (3.0 line boxes) is not rounded up by float
    // noise, while a partial line still counts as visible.
    let raw = (height / line - 1e-3).ceil().max(1.0);
    (raw as usize).clamp(1, MAX_LINES)
}

/// Pure geometry: place the box and cut the text sections. No drawing.
pub fn layout(anchor: Rect, request: TooltipRequest, safe: Rect, scale: f32) -> TooltipParts {
    if !rect_is_finite(&safe) || safe.w <= 0.0 || safe.h <= 0.0 {
        return TooltipParts::default();
    }
    let scale = sanitised_scale(scale);
    let line = line_height(scale);
    let (divider_h, gap) = divider_metrics(scale);
    let pad_x = PAD_X * scale;
    let pad_y = PAD_Y * scale;
    let chrome_h = divider_h + 2.0 * gap;

    // Visible lines: the request, capped by the retail line cap and by what the
    // safe frame can actually show at this scale.
    let room = (safe.h - chrome_h - 2.0 * pad_y) / line;
    let max_lines = (room.max(1.0) as usize).clamp(1, MAX_LINES);
    let lines = request.lines.clamp(1, max_lines);
    let costs = request
        .cost_lines
        .min(MAX_COST_LINES)
        .min(lines.saturating_sub(1 + MIN_BODY_LINES));
    let body_lines = lines - 1 - costs;

    let w = width_for(request.width, safe, scale);
    let h = lines as f32 * line + chrome_h + 2.0 * pad_y;

    // Anchor in the frame: above the slot by default, below when the slot is
    // too close to the top edge, and bottom/top clamped as the last resort.
    let anchor = anchor_or_centre(anchor, safe);
    let slot_gap = ANCHOR_GAP * scale;
    let x = clamp_span(anchor.centre().x - w * 0.5, safe.x, safe.right() - w);
    let space_above = anchor.y - safe.y;
    let space_below = safe.bottom() - anchor.bottom();
    let need = h + slot_gap;
    let above = if space_above >= need {
        true
    } else if space_below >= need {
        false
    } else {
        space_above >= space_below
    };
    let y = if above {
        anchor.y - slot_gap - h
    } else {
        anchor.bottom() + slot_gap
    };
    let y = clamp_span(y, safe.y, safe.bottom() - h);

    // The frame owns the box: a box that still overflows (frame smaller than one
    // line of chrome) is shrunk rather than allowed to leave the safe area.
    let box_rect = Rect::new(x, y, w, h).clamped_inside(&safe);

    // Sections. `inner` is exactly `lines` line boxes plus the divider chrome.
    let inner = box_rect.inset_sides(pad_x, pad_y, pad_x, pad_y);
    let title_rect = Rect::new(inner.x, inner.y, inner.w, line);
    let divider_rect = Rect::new(inner.x, title_rect.bottom() + gap, inner.w, divider_h);
    let body_top = divider_rect.bottom() + gap;
    let cost_top = inner.bottom() - costs as f32 * line;
    let body_rect = Rect::new(
        inner.x,
        body_top,
        inner.w,
        (cost_top - body_top).max(0.0).min(body_lines as f32 * line),
    );
    let cost_rects: Vec<Rect> = (0..costs)
        .map(|row| Rect::new(inner.x, cost_top + row as f32 * line, inner.w, line))
        .collect();

    TooltipParts {
        box_rect,
        title_rect: fit(title_rect, box_rect),
        body_rect: fit(body_rect, box_rect),
        cost_rects: cost_rects.into_iter().map(|rect| fit(rect, box_rect)).collect(),
        divider_rect: fit(divider_rect, box_rect),
        line_height: line,
        lines,
        drawn: false,
        above,
    }
}

/// Draw the shell and the divider, returning the text rects.
///
/// `desired.w` is the preferred box width (`<= 0` = [`MAX_WIDTH`] at `scale`),
/// `desired.h` the measured height of every text line; an empty rect asks for
/// the conservative default ([`DEFAULT_LINES`], maximum cost capacity). The
/// width is always `min(380 * scale, 42% of the safe width)` or narrower, the
/// height is `lines * line height` (capped at [`MAX_LINES`]) plus the fixed
/// padding/divider chrome.
pub fn draw(
    skin: &Skin,
    batch: &mut UiBatch,
    anchor: Rect,
    desired: Rect,
    safe: Rect,
    scale: f32,
    tint: [f32; 4],
) -> TooltipParts {
    draw_request(
        skin,
        batch,
        anchor,
        TooltipRequest::from_desired(desired, scale),
        safe,
        scale,
        tint,
    )
}

/// [`draw`] with the section counts stated explicitly (a caller that already
/// measured its title/body/cost lines).
pub fn draw_request(
    skin: &Skin,
    batch: &mut UiBatch,
    anchor: Rect,
    request: TooltipRequest,
    safe: Rect,
    scale: f32,
    tint: [f32; 4],
) -> TooltipParts {
    let mut parts = layout(anchor, request, safe, scale);
    if parts.is_empty() {
        return parts;
    }
    let mut drawn = skin.nine(batch, ROLE, parts.box_rect, tint);
    if parts.divider_rect.h > 0.0 {
        // A divider is one-dimensional: the bar's whole slice is stretched, its
        // corners are not ornaments to preserve.
        drawn |= skin.stretch(batch, DIVIDER_ROLE, parts.divider_rect, tint);
    }
    parts.drawn = drawn;
    parts
}

/// Box width for one request: `min(380 * scale, 42% of the safe width)`, with
/// the caller's smaller preference honoured down to the ornament's minimum.
fn width_for(request_w: f32, safe: Rect, scale: f32) -> f32 {
    let cap = (MAX_WIDTH * scale).min(safe.w * MAX_SAFE_FRACTION).max(0.0);
    if cap <= 0.0 {
        return 0.0;
    }
    let floor = (MIN_WIDTH * scale).min(cap);
    let wanted = if request_w.is_finite() && request_w > 0.0 {
        request_w
    } else {
        MAX_WIDTH * scale
    };
    wanted.clamp(floor, cap)
}

/// Divider band height and the clearance above/below it.
fn divider_metrics(scale: f32) -> (f32, f32) {
    let scale = sanitised_scale(scale);
    ((DIVIDER_HEIGHT * scale).clamp(2.0, 10.0), DIVIDER_GAP * scale)
}

fn text_scale(scale: f32) -> f32 {
    sanitised_scale(scale).max(crate::hud::MIN_TEXT_PX / LINE_BASE)
}

fn sanitised_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn rect_is_finite(rect: &Rect) -> bool {
    rect.x.is_finite() && rect.y.is_finite() && rect.w.is_finite() && rect.h.is_finite()
}

/// A hovered slot is required for anchoring; without one the box floats at the
/// frame centre instead of inheriting a NaN.
fn anchor_or_centre(anchor: Rect, safe: Rect) -> Rect {
    let usable = rect_is_finite(&anchor) && anchor.w > 0.0 && anchor.h > 0.0;
    if usable {
        anchor
    } else {
        Rect::new(safe.centre().x, safe.centre().y, 0.0, 0.0)
    }
}

fn clamp_span(value: f32, low: f32, high: f32) -> f32 {
    if !value.is_finite() {
        return low;
    }
    if high < low {
        low
    } else {
        value.clamp(low, high)
    }
}

/// Clip `rect` to `bounds` (keeps every returned rect inside the box).
fn fit(rect: Rect, bounds: Rect) -> Rect {
    let x = rect.x.max(bounds.x);
    let y = rect.y.max(bounds.y);
    let right = rect.right().min(bounds.right()).max(x);
    let bottom = rect.bottom().min(bounds.bottom()).max(y);
    Rect::new(x, y, right - x, bottom - y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hudskin::tests_support::sample_skin;

    fn safe_1080() -> Rect {
        Rect::new(0.0, 0.0, 1920.0, 1080.0)
    }

    fn anchor(x: f32, y: f32) -> Rect {
        Rect::new(x, y, 64.0, 64.0)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    /// `ui.tooltips`: the shell and every text section stay inside the frame, and
    /// the four corner placements are the documented ones (6 lines, 3 cost rows,
    /// 380x249 at scale 1 on 1920x1080).
    #[test]
    fn ui_tooltips_box_stays_inside_safe_at_four_corner_anchors() {
        let safe = safe_1080();
        let corners = [
            ("top-left", anchor(40.0, 20.0), Rect::new(0.0, 90.0, 380.0, 249.0)),
            ("top-right", anchor(1816.0, 20.0), Rect::new(1540.0, 90.0, 380.0, 249.0)),
            ("bottom-left", anchor(40.0, 996.0), Rect::new(0.0, 741.0, 380.0, 249.0)),
            ("bottom-right", anchor(1816.0, 996.0), Rect::new(1540.0, 741.0, 380.0, 249.0)),
        ];
        for (name, slot, expected) in corners {
            let parts = layout(slot, TooltipRequest::new(0.0, 6, 3), safe, 1.0);
            eprintln!(
                "{name}: anchor ({:.0},{:.0},{:.0},{:.0}) -> box x={:.1} y={:.1} w={:.1} h={:.1} above={} lines={} costs={}",
                slot.x,
                slot.y,
                slot.w,
                slot.h,
                parts.box_rect.x,
                parts.box_rect.y,
                parts.box_rect.w,
                parts.box_rect.h,
                parts.above,
                parts.lines,
                parts.cost_rects.len(),
            );
            assert!(
                close(parts.box_rect.x, expected.x)
                    && close(parts.box_rect.y, expected.y)
                    && close(parts.box_rect.w, expected.w)
                    && close(parts.box_rect.h, expected.h),
                "{name}: box {:?} != expected {:?}",
                parts.box_rect,
                expected
            );
            assert!(
                safe.contains_rect(&parts.box_rect),
                "{name}: box {:?} left the safe frame",
                parts.box_rect
            );
            assert!(parts.box_rect.w > 0.0 && parts.box_rect.h > 0.0, "{name}: empty");
            assert!(parts.box_rect.contains_rect(&parts.title_rect), "{name}: title");
            assert!(parts.box_rect.contains_rect(&parts.body_rect), "{name}: body");
            assert!(parts.box_rect.contains_rect(&parts.divider_rect), "{name}: divider");
            for (row, cost) in parts.cost_rects.iter().enumerate() {
                assert!(parts.box_rect.contains_rect(cost), "{name}: cost {row}");
            }
        }
    }

    /// `ui.tooltips`: a slot near the top edge flips the box below it.
    #[test]
    fn ui_tooltips_anchor_near_top_flips_the_box_below() {
        let safe = safe_1080();
        let top = anchor(900.0, 20.0);
        let below = layout(top, TooltipRequest::new(0.0, 5, 2), safe, 1.0);
        assert!(!below.above, "top-edge slot must place the box below");
        assert!(
            below.box_rect.y >= top.bottom(),
            "box {:?} is not below slot {:?}",
            below.box_rect,
            top
        );

        let bottom = anchor(900.0, 990.0);
        let above = layout(bottom, TooltipRequest::new(0.0, 5, 2), safe, 1.0);
        assert!(above.above, "bottom-edge slot must keep the box above");
        assert!(
            above.box_rect.bottom() <= bottom.y,
            "box {:?} is not above slot {:?}",
            above.box_rect,
            bottom
        );
    }

    /// `ui.tooltips`: the retail width cap on a 1080p frame.
    #[test]
    fn ui_tooltips_width_is_380_at_1080p() {
        let safe = safe_1080();
        let slot = anchor(960.0, 700.0);
        let default = layout(slot, TooltipRequest::new(0.0, 4, 0), safe, 1.0);
        assert!(close(default.box_rect.w, MAX_WIDTH), "default width {}", default.box_rect.w);

        let wider = layout(slot, TooltipRequest::new(640.0, 4, 0), safe, 1.0);
        assert!(close(wider.box_rect.w, MAX_WIDTH), "640 requested {}", wider.box_rect.w);

        let narrow = layout(slot, TooltipRequest::new(200.0, 4, 0), safe, 1.0);
        assert!(close(narrow.box_rect.w, 200.0), "200 requested {}", narrow.box_rect.w);

        // 42% of a 700 px frame is the tighter cap, not the 380 px constant.
        let tight = layout(slot, TooltipRequest::new(0.0, 4, 0), Rect::new(0.0, 0.0, 700.0, 1080.0), 1.0);
        assert!(close(tight.box_rect.w, 700.0 * MAX_SAFE_FRACTION), "42% cap {}", tight.box_rect.w);

        // The cap scales with the HUD.
        let big = layout(slot, TooltipRequest::new(0.0, 4, 0), safe, 2.0);
        assert!(close(big.box_rect.w, MAX_WIDTH * 2.0), "scale 2 width {}", big.box_rect.w);
    }

    /// `ui.tooltips`: section 3 rows sit below the description, never over it.
    #[test]
    fn ui_tooltips_cost_rows_never_overlap_the_body() {
        let parts = layout(anchor(900.0, 700.0), TooltipRequest::new(0.0, 9, 8), safe_1080(), 1.0);
        assert_eq!(parts.cost_rects.len(), 7, "title + one body line win first");
        assert!(parts.cost_rects.len() <= MAX_COST_LINES);
        assert!(parts.body_rect.h > 0.0, "description must stay visible");
        for (row, cost) in parts.cost_rects.iter().enumerate() {
            assert_eq!(
                parts.body_rect.intersection_area(cost),
                0.0,
                "cost row {row} overlaps the body"
            );
            assert!(cost.y >= parts.body_rect.bottom() - 0.01, "cost row {row} above body");
            for other in parts.cost_rects.iter().skip(row + 1) {
                assert_eq!(cost.intersection_area(other), 0.0, "cost rows {row} overlap");
            }
            assert!(close(cost.h, parts.line_height), "cost row {row} is one line box");
        }
        for pair in parts.cost_rects.windows(2) {
            assert!(close(pair[1].y, pair[0].bottom()), "cost rows must stack");
        }

        // A caller with two cost rows gets two rects and a taller description.
        let two = layout(anchor(900.0, 700.0), TooltipRequest::new(0.0, 9, 2), safe_1080(), 1.0);
        assert_eq!(two.cost_rects.len(), 2);
        assert!(two.body_rect.h > parts.body_rect.h);
    }

    /// `ui.tooltips`: the nine-patch corners keep their atlas pixel size.
    #[test]
    fn ui_tooltips_nine_patch_corners_keep_atlas_pixel_size() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw_request(
            &skin,
            &mut batch,
            anchor(900.0, 700.0),
            TooltipRequest::new(0.0, 6, 2),
            safe_1080(),
            1.0,
            [1.0; 4],
        );
        assert!(parts.drawn, "roles must exist");
        let draw = batch.textured.first().expect("box quads");
        let v = &draw.vertices;
        let index = Skin::role_index(ROLE).expect("box role");
        let [ax, ay, aw, ah] = skin.rects[index];
        let [l, t, r, b] = skin.insets[index];
        let box_rect = parts.box_rect;

        // Top-left corner: dest `l x t`, uv `l/aw x t/ah` of the tooltip slice.
        assert!(close(v[0].pos[0], box_rect.x), "tl x");
        assert!(close(v[0].pos[1], box_rect.y), "tl y");
        assert!(close(v[2].pos[0], box_rect.x + l), "corner width");
        assert!(close(v[2].pos[1], box_rect.y + t), "corner height");
        assert!(
            (v[2].uv[0] - (ax + l) / skin.width).abs() < 1e-6,
            "corner u span"
        );
        assert!(
            (v[2].uv[1] - (ay + t) / skin.height).abs() < 1e-6,
            "corner v span"
        );

        // Bottom-right corner: same identity at the opposite side.
        let last = 8 * 6;
        assert!(close(v[last].pos[0], box_rect.right() - r), "br x");
        assert!(close(v[last].pos[1], box_rect.bottom() - b), "br y");
        assert!(close(v[last + 2].pos[0], box_rect.right()), "br right");
        assert!(close(v[last + 2].pos[1], box_rect.bottom()), "br bottom");
        assert!((v[last + 2].uv[0] - (ax + aw) / skin.width).abs() < 1e-6, "br u");
        assert!((v[last + 2].uv[1] - (ay + ah) / skin.height).abs() < 1e-6, "br v");
    }

    /// `ui.tooltips`: twenty requested lines still render a nine-line box.
    #[test]
    fn ui_tooltips_line_cap_holds_for_twenty_requested_lines() {
        let safe = safe_1080();
        let scale = 1.0;
        let line = line_height(scale);
        let (divider_h, gap) = divider_metrics(scale);
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw(
            &skin,
            &mut batch,
            anchor(900.0, 700.0),
            Rect::new(0.0, 0.0, 0.0, 20.0 * line),
            safe,
            scale,
            [1.0; 4],
        );

        assert_eq!(parts.lines, MAX_LINES, "cap is nine visible lines");
        let text: f32 = parts.title_rect.h
            + parts.body_rect.h
            + parts.cost_rects.iter().map(|rect| rect.h).sum::<f32>();
        assert!(
            close(text, MAX_LINES as f32 * line),
            "{text} px of text in a nine-line cap"
        );
        assert!(parts.cost_rects.len() <= MAX_COST_LINES);
        assert!(close(parts.body_rect.h, line), "one description line survives");
        assert!(
            close(
                parts.box_rect.h,
                MAX_LINES as f32 * line + divider_h + 2.0 * gap + 2.0 * PAD_Y * scale
            ),
            "box height {}",
            parts.box_rect.h
        );
        assert!(safe.contains_rect(&parts.box_rect));
    }

    /// `ui.tooltips`: the divider is the `thin_bar` slice between the sections.
    #[test]
    fn ui_tooltips_divider_sits_between_title_and_body() {
        let skin = sample_skin();
        let mut batch = UiBatch::default();
        let parts = draw_request(
            &skin,
            &mut batch,
            anchor(900.0, 700.0),
            TooltipRequest::new(0.0, 6, 2),
            safe_1080(),
            1.0,
            [1.0; 4],
        );
        assert!(
            close(parts.divider_rect.y, parts.title_rect.bottom() + DIVIDER_GAP),
            "divider must follow the title line"
        );
        assert!(
            parts.body_rect.y >= parts.divider_rect.bottom(),
            "divider must precede the description"
        );

        let draw = batch.textured.first().expect("quads");
        let v = &draw.vertices;
        let index = Skin::role_index(DIVIDER_ROLE).expect("divider role");
        let [ax, ay, aw, ah] = skin.rects[index];
        let tail = v.len() - 6;
        assert!((v[tail].uv[0] - ax / skin.width).abs() < 1e-6, "divider u0");
        assert!((v[tail].uv[1] - ay / skin.height).abs() < 1e-6, "divider v0");
        assert!((v[tail + 2].uv[0] - (ax + aw) / skin.width).abs() < 1e-6, "divider u1");
        assert!((v[tail + 2].uv[1] - (ay + ah) / skin.height).abs() < 1e-6, "divider v1");
        assert!(close(v[tail].pos[0], parts.divider_rect.x), "divider starts at the inset");
    }

    /// `ui.tooltips`: the cost section is painted retail gold `FED312`.
    #[test]
    fn ui_tooltips_cost_tint_is_retail_gold() {
        assert_eq!((COST_TINT[0] * 255.0).round() as u32, 0xFE);
        assert_eq!((COST_TINT[1] * 255.0).round() as u32, 0xD3);
        assert_eq!((COST_TINT[2] * 255.0).round() as u32, 0x12);
        assert_eq!(COST_TINT[3], 1.0);
    }

    /// `ui.tooltips`: no frame, no slot or a bad scale never panics.
    #[test]
    fn ui_tooltips_degenerate_inputs_are_safe() {
        let request = TooltipRequest::new(0.0, 6, 3);
        let none = layout(anchor(900.0, 700.0), request, Rect::new(0.0, 0.0, 0.0, 0.0), 1.0);
        assert!(none.is_empty());

        let no_slot = layout(Rect::default(), request, safe_1080(), 1.0);
        assert!(safe_1080().contains_rect(&no_slot.box_rect), "fallback box inside frame");

        let bad_scale = layout(anchor(900.0, 700.0), request, safe_1080(), f32::NAN);
        assert!(bad_scale.box_rect.x.is_finite() && bad_scale.box_rect.w > 0.0);

        // A frame smaller than the chrome still yields a box inside the frame.
        let tiny = layout(anchor(10.0, 10.0), request, Rect::new(0.0, 0.0, 60.0, 40.0), 1.0);
        assert!(Rect::new(0.0, 0.0, 60.0, 40.0).contains_rect(&tiny.box_rect));
    }

    /// `ui.tooltips`: `draw` reads width and line budget from `desired`.
    #[test]
    fn ui_tooltips_draw_matches_layout_from_desired() {
        let skin = sample_skin();
        let safe = safe_1080();
        let slot = anchor(900.0, 700.0);
        let desired = Rect::new(250.0, 0.0, 250.0, 6.0 * line_height(1.0));
        let mut batch = UiBatch::default();
        let drawn = draw(&skin, &mut batch, slot, desired, safe, 1.0, [1.0; 4]);
        let planned = layout(slot, TooltipRequest::new(250.0, 6, MAX_COST_LINES), safe, 1.0);
        assert_eq!(drawn.lines, 6);
        assert!(close(drawn.box_rect.w, 250.0), "requested width {}", drawn.box_rect.w);
        assert!(close(drawn.box_rect.x, planned.box_rect.x), "same anchor maths");
        assert!(close(drawn.box_rect.y, planned.box_rect.y), "same flip maths");
        assert!(close(drawn.box_rect.h, planned.box_rect.h), "same height");
        assert!(drawn.drawn && !batch.textured.is_empty());
    }
}
